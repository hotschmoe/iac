// Client command handling: routes a protocol `Command` to the engine and words
// the refusal. Pure state in, state out; hosts add transport and sessions.

use iac_shared::constants::{ResourceKind, Resources, MAX_FLEETS_PER_PLAYER, MAX_FLEETS_TOTAL, MAX_FLEET_SHIPS};
use iac_shared::protocol::{
    ClientMessage, Command, ErrorCode, ErrorMessage, ExchangeQuote, FleetOdds, HarvestResource, ServerMessage,
    ShipCount, MAX_ODDS_SECTORS,
};

use crate::engine::{AuthError, ExchangeDeal, Fleet, FleetStatus, GameEngine};
use crate::views;

/// What a command produced for its sender: an optional answer, an optional
/// refusal (code plus a sentence saying which fleet, sector or queue is at fault).
#[derive(Debug, Default)]
pub struct CommandOutcome {
    pub reply: Option<ServerMessage>,
    pub error: Option<(ErrorCode, String)>,
}

impl GameEngine {
    /// Run one client command for `pid`.
    pub fn execute(&mut self, pid: u64, cmd: Command) -> CommandOutcome {
        let mut reply: Option<ServerMessage> = None;
        let result = match cmd {
            Command::Move { fleet_id, target } => self.handle_move(pid, fleet_id, target),
            Command::Harvest { fleet_id, resource } => self.handle_harvest(pid, fleet_id, resource),
            Command::Recall { fleet_id } => self.handle_recall(pid, fleet_id),
            Command::CollectSalvage { fleet_id } => self.handle_collect_salvage(pid, fleet_id),
            Command::Attack { fleet_id, target_fleet_id } => self.handle_attack(pid, fleet_id, target_fleet_id),
            Command::Build { building_type, reserve } => self.handle_build(pid, building_type, reserve),
            Command::Research { tech, reserve } => self.handle_research(pid, tech, reserve),
            Command::BuildShip { ship_class, count } => self.handle_build_ship(pid, ship_class, count),
            Command::BuildDefence { kind, count } => self.handle_build_defence(pid, kind, count),
            Command::CancelBuild { queue_type, id } => self.handle_cancel_build(pid, queue_type, id),
            Command::CancelQueued { queue_type, id } => self.handle_cancel_queued(pid, queue_type, id),
            Command::Stop { fleet_id } => self.handle_stop(pid, fleet_id),
            Command::Scan { fleet_id } => self.handle_scan(pid, fleet_id),
            Command::ExploreSite { fleet_id } => self.handle_explore_site(pid, fleet_id),
            Command::Split { fleet_id, ref ships, damaged_first } => {
                self.handle_split(pid, fleet_id, ships, damaged_first).map(|_| ())
            }
            Command::Merge { fleet_id, other_fleet_id } => self.handle_merge(pid, fleet_id, other_fleet_id),
            Command::Leaderboard { limit } => {
                reply = Some(ServerMessage::Leaderboard(crate::score::leaderboard(self, limit as usize, pid)));
                Ok(())
            }
            Command::PreviewMove { fleet_id, target } => self.preview_move(pid, fleet_id, target).map(|p| {
                reply = Some(ServerMessage::PreviewMove(p));
            }),
            Command::Odds { fleet_id, ref sectors } => self.odds_for(pid, fleet_id, sectors).map(|sectors| {
                reply = Some(ServerMessage::Odds(FleetOdds { fleet_id, sectors }));
            }),
            Command::Exchange { fleet_id, give, want, amount } => self.handle_exchange(pid, fleet_id, give, want, amount),
            Command::ExchangeQuote { fleet_id, give, want, amount } => self
                .exchange_deal(pid, fleet_id, give, want, amount)
                .map(|deal| {
                    let refusal = deal.refusal.map(|r| r.reason);
                    reply = Some(ServerMessage::ExchangeQuote(ExchangeQuote { terms: deal.terms, refusal }));
                })
                .map_err(|refusal| refusal.code),
            Command::Load { fleet_id, resource, amount } => self.handle_load(pid, fleet_id, resource, amount),
            Command::Unload { fleet_id } => self.handle_unload(pid, fleet_id),
            Command::Refine { fleet_id } => self.handle_refine(pid, fleet_id),
            Command::Refuel { fleet_id, give, amount } => self.handle_refuel(pid, fleet_id, give, amount),
            Command::FoundOutpost { fleet_id } => self.handle_found_outpost(pid, fleet_id),
            Command::OutpostSurvey { fleet_id, sector } => self
                .outpost_survey(pid, fleet_id, sector)
                .map(|survey| reply = Some(ServerMessage::OutpostSurvey(survey)))
                .map_err(|refusal| refusal.code),
            Command::UpgradeOutpost => self.handle_upgrade_outpost(pid),
            Command::AbandonOutpost => self.handle_abandon_outpost(pid),
            Command::FoundRelay { fleet_id } => self.handle_found_relay(pid, fleet_id),
            Command::RelaySurvey { fleet_id, sector } => self
                .relay_survey(pid, fleet_id, sector)
                .map(|survey| reply = Some(ServerMessage::RelaySurvey(survey)))
                .map_err(|refusal| refusal.code),
        };
        let error = result.err().map(|code| (code, command_error_message(self, pid, &cmd, code)));
        CommandOutcome { reply, error }
    }

    /// Everything a session logged in as `pid` is sent for one message, in
    /// order. Logging in is the host's own step: it knows what an account
    /// must present.
    pub fn answer(&mut self, pid: u64, msg: ClientMessage) -> Vec<ServerMessage> {
        match msg {
            ClientMessage::Auth(_) => vec![refusal(ErrorCode::AlreadyAuthenticated, "Already authenticated")],
            ClientMessage::Command(cmd) => {
                let outcome = self.execute(pid, cmd);
                let error = outcome.error.map(|(code, message)| refusal(code, &message));
                outcome.reply.into_iter().chain(error).collect()
            }
            ClientMessage::PolicyUpdate(update) => self
                .handle_policy_update(pid, update.fleet_id, update.preset, update.params)
                .err()
                .map(|code| {
                    let fleet = update.fleet_id;
                    refusal(code, &format!("fleet {fleet} not found (or it has no ships left); its standing orders were not changed"))
                })
                .into_iter()
                .collect(),
            ClientMessage::RequestFullState => {
                views::full_state(self, pid).map(ServerMessage::FullState).into_iter().collect()
            }
        }
    }
}

/// Whether `cmd` only asks: it is answered and leaves the world as it was.
pub fn is_query(cmd: &Command) -> bool {
    matches!(
        cmd,
        Command::Odds { .. }
            | Command::PreviewMove { .. }
            | Command::Leaderboard { .. }
            | Command::ExchangeQuote { .. }
            | Command::OutpostSurvey { .. }
            | Command::RelaySurvey { .. }
    )
}

/// What a session that has not logged in is sent for anything but `auth`.
pub fn not_logged_in(msg: &ClientMessage) -> Option<ServerMessage> {
    match msg {
        ClientMessage::Command(_) | ClientMessage::PolicyUpdate(_) => {
            Some(refusal(ErrorCode::AuthFailed, "Not authenticated"))
        }
        ClientMessage::Auth(_) | ClientMessage::RequestFullState => None,
    }
}

fn refusal(code: ErrorCode, message: &str) -> ServerMessage {
    ServerMessage::Error(ErrorMessage { code, message: message.to_string() })
}

/// The error code and sentence for a refused login.
pub fn auth_failure(name: &str, err: AuthError) -> (ErrorCode, String) {
    match err {
        AuthError::InvalidName(reason) => (ErrorCode::InvalidName, format!("invalid player name: {reason}")),
        AuthError::TokenRequired => (
            ErrorCode::TokenRequired,
            format!("player '{name}' is already registered; present its token to log in"),
        ),
        AuthError::InvalidToken => (ErrorCode::InvalidToken, format!("token rejected for player '{name}'")),
        AuthError::WorldFull => (
            ErrorCode::WorldFull,
            format!("this world is full: all {} home seats are taken", iac_shared::world::home_seats().count()),
        ),
    }
}

fn command_fleet_id(cmd: &Command) -> Option<u64> {
    match cmd {
        Command::Move { fleet_id, .. }
        | Command::Harvest { fleet_id, .. }
        | Command::Attack { fleet_id, .. }
        | Command::Recall { fleet_id }
        | Command::CollectSalvage { fleet_id }
        | Command::Stop { fleet_id }
        | Command::Scan { fleet_id }
        | Command::ExploreSite { fleet_id }
        | Command::Split { fleet_id, .. }
        | Command::PreviewMove { fleet_id, .. }
        | Command::Odds { fleet_id, .. }
        | Command::Exchange { fleet_id, .. }
        | Command::ExchangeQuote { fleet_id, .. }
        | Command::Load { fleet_id, .. }
        | Command::Unload { fleet_id }
        | Command::Refine { fleet_id }
        | Command::Refuel { fleet_id, .. }
        | Command::FoundOutpost { fleet_id }
        | Command::OutpostSurvey { fleet_id, .. }
        | Command::FoundRelay { fleet_id }
        | Command::RelaySurvey { fleet_id, .. }
        | Command::Merge { fleet_id, .. } => Some(*fleet_id),
        Command::Build { .. }
        | Command::Research { .. }
        | Command::BuildShip { .. }
        | Command::BuildDefence { .. }
        | Command::Leaderboard { .. }
        | Command::CancelBuild { .. }
        | Command::CancelQueued { .. }
        | Command::UpgradeOutpost
        | Command::AbandonOutpost => None,
    }
}

fn harvest_label(r: HarvestResource) -> &'static str {
    match r {
        HarvestResource::Metal => "metal",
        HarvestResource::Crystal => "crystal",
        HarvestResource::Deuterium => "deuterium",
        HarvestResource::Auto => "resources",
    }
}

/// "X needs Y level N (you have M) and ..." for a build, research or ship
/// order, from the same requirement lists the catalog sends.
fn missing_prerequisites_message(engine: &GameEngine, player_id: u64, cmd: &Command) -> Option<String> {
    use iac_shared::protocol::{
        building_requirements, defence_requirements, missing_requirements_message, research_requirements,
        ship_requirements,
    };
    let p = engine.players.get(&player_id)?;
    let (buildings, research) = (p.projected_buildings(), p.projected_research());
    let (subject, requires) = match cmd {
        Command::Build { building_type, .. } => {
            (building_type.label(), building_requirements(&buildings, &research, *building_type))
        }
        Command::Research { tech, .. } => {
            let level = research.get(*tech).saturating_add(1);
            let energy = p.power(&engine.pace()).made;
            (tech.label(), research_requirements(&buildings, &research, *tech, level, energy))
        }
        Command::BuildShip { ship_class, .. } => (ship_class.label(), ship_requirements(&buildings, &research, *ship_class)),
        Command::BuildDefence { kind, .. } => (kind.label(), defence_requirements(&buildings, &research, *kind)),
        _ => return None,
    };
    missing_requirements_message(subject, &requires)
}

#[derive(Clone, Copy, PartialEq)]
enum QueueSlot {
    Running,
    Waiting,
}

/// Where a queue id actually lives, as (queue, slot).
fn find_queue_item(engine: &GameEngine, player_id: u64, id: u64) -> Option<(iac_shared::protocol::QueueType, QueueSlot)> {
    use iac_shared::protocol::QueueType;
    let p = engine.players.get(&player_id)?;
    if p.building_queue.iter().any(|q| q.id == id) { return Some((QueueType::Building, QueueSlot::Running)); }
    if p.building_pending.iter().any(|q| q.id == id) { return Some((QueueType::Building, QueueSlot::Waiting)); }
    if p.research_queue.as_ref().is_some_and(|q| q.id == id) { return Some((QueueType::Research, QueueSlot::Running)); }
    if p.research_pending.iter().any(|q| q.id == id) { return Some((QueueType::Research, QueueSlot::Waiting)); }
    if p.ship_queue.as_ref().is_some_and(|q| q.id == id) { return Some((QueueType::Ship, QueueSlot::Running)); }
    if p.ship_pending.iter().any(|q| q.id == id) { return Some((QueueType::Ship, QueueSlot::Waiting)); }
    None
}

fn queue_name(queue_type: iac_shared::protocol::QueueType) -> &'static str {
    use iac_shared::protocol::QueueType;
    match queue_type {
        QueueType::Building => "building",
        QueueType::Research => "research",
        QueueType::Ship => "shipyard",
    }
}

/// Why a cancel by id found nothing: the id is unknown (finished or already
/// cancelled), in another queue, or in the other state.
fn cancel_refusal(engine: &GameEngine, player_id: u64, queue_type: iac_shared::protocol::QueueType, id: u64, wanted: QueueSlot) -> String {
    let (verb, other) = match wanted {
        QueueSlot::Running => ("under way", "cancel_queued"),
        QueueSlot::Waiting => ("waiting", "cancel_build"),
    };
    match find_queue_item(engine, player_id, id) {
        None => format!("no order {id} in any queue: it finished, was cancelled, or the id is wrong"),
        Some((found, _)) if found != queue_type => format!(
            "order {id} is in the {} queue, not the {} queue; nothing was cancelled",
            queue_name(found), queue_name(queue_type)
        ),
        Some((_, slot)) if slot != wanted => format!(
            "order {id} is not {verb} in the {} queue; use {other} for it",
            queue_name(queue_type)
        ),
        Some(_) => format!("order {id} cannot be cancelled"),
    }
}

/// Names what a rejected order costs and which level of which storage
/// building would hold it.
fn storage_too_small_message(engine: &GameEngine, player_id: u64, cmd: &Command) -> String {
    use iac_shared::scaling::{storage_building, storage_cap, storage_level_for};
    let (Some((label, cost)), Some(p)) = (engine.command_cost(player_id, cmd), engine.players.get(&player_id)) else {
        return "the payment is larger than a storage cap".to_string();
    };
    let cap = storage_cap(&p.buildings, &engine.pace());
    let over: Vec<ResourceKind> = ResourceKind::ALL.into_iter().filter(|&k| cost.get(k) > cap.get(k)).collect();
    let worst = over
        .iter()
        .copied()
        .max_by(|&a, &b| (cost.get(a) / cap.get(a)).total_cmp(&(cost.get(b) / cap.get(b))))
        .unwrap_or(ResourceKind::Metal);
    let levels: Vec<(&str, Option<u8>)> =
        over.iter().map(|&k| (storage_building(k).label(), storage_level_for(cost.get(k), &engine.pace()))).collect();
    let fix = match levels.iter().find(|(_, level)| level.is_none()) {
        Some((building, _)) => format!("no {building} can hold it"),
        None => {
            let each: Vec<String> =
                levels.iter().map(|(building, level)| format!("{building} level {}", level.unwrap_or(0))).collect();
            format!("needs {}", listed(&each))
        }
    };
    let smaller = match batch_of(cmd) {
        Some((unit, count)) if count > 1 => match units_covered(cap, unit) {
            0 => String::new(),
            fits => format!(", or order {fits} or fewer at a time"),
        },
        _ => String::new(),
    };
    format!(
        "{label} costs {} {}, more than the {} {} your stockpile holds; {fix}{smaller}",
        cost.get(worst).ceil(), worst.label(), cap.get(worst).floor(), worst.label(),
    )
}

/// The unit price and count of a shipyard order.
fn batch_of(cmd: &Command) -> Option<(Resources, u16)> {
    match cmd {
        Command::BuildShip { ship_class, count } => Some((ship_class.build_cost(), *count)),
        Command::BuildDefence { kind, count } => Some((kind.build_cost(), *count)),
        _ => None,
    }
}

/// How many units at `unit` price `stock` pays for in full.
fn units_covered(stock: Resources, unit: Resources) -> u32 {
    ResourceKind::ALL
        .into_iter()
        .filter(|&k| unit.get(k) > 0.0)
        .map(|k| (stock.get(k) / unit.get(k)).floor().max(0.0) as u32)
        .min()
        .unwrap_or(0)
}

/// "759 metal and 190 crystal": the non-zero parts of an amount, each
/// rounded up to a whole unit.
pub(crate) fn amount_words(amount: Resources) -> String {
    let parts: Vec<String> = ResourceKind::ALL
        .into_iter()
        .filter(|&k| amount.get(k) > 0.0)
        .map(|k| format!("{} {}", amount.get(k).ceil(), k.label()))
        .collect();
    if parts.is_empty() { "nothing".to_string() } else { listed(&parts) }
}

/// "a", "a and b", "a, b and c".
fn listed(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [one] => one.clone(),
        [first @ .., last] => format!("{} and {last}", first.join(", ")),
    }
}

/// A shipyard batch is paid in full when it is ordered; this names what the
/// stockpile lacks, per resource, and how many units it would cover.
fn insufficient_resources_message(engine: &GameEngine, player_id: u64, cmd: &Command) -> String {
    let (Some((label, cost)), Some(p)) = (engine.command_cost(player_id, cmd), engine.players.get(&player_id)) else {
        return "the stockpile does not cover this batch; nothing was ordered".to_string();
    };
    let short = p.resources.shortfall(cost);
    let covered = match batch_of(cmd) {
        Some((unit, count)) if count > 1 => match units_covered(p.resources, unit) {
            0 => String::new(),
            fits => format!(" The stockpile covers {fits}."),
        },
        _ => String::new(),
    };
    format!(
        "{label} costs {}; the stockpile is short by {}. Nothing was ordered: a batch is paid in full when it is ordered.{covered}",
        amount_words(cost),
        amount_words(short),
    )
}

/// Why a `split` naming `ships` was refused for `fleet`.
fn split_refusal(fleet: Option<&Fleet>, ships: &[ShipCount]) -> String {
    let Some(fleet) = fleet else {
        return "split needs a fleet of yours with ships aboard".to_string();
    };
    if ships.is_empty() {
        return "split needs at least one class and a count of its ships to take".to_string();
    }
    for (i, order) in ships.iter().enumerate() {
        let label = order.ship_class.label();
        if order.count == 0 {
            return format!("split names {label} with a count of 0");
        }
        if ships[..i].iter().any(|earlier| earlier.ship_class == order.ship_class) {
            return format!("split names {label} twice; give each class one count");
        }
        let aboard = fleet.ships.count(order.ship_class);
        if order.count > aboard {
            return format!("fleet {} has {aboard} {label}, not the {} the split names", fleet.id, order.count);
        }
    }
    format!(
        "the split would take all {} ships of fleet {}; at least one ship must stay behind",
        fleet.ships.total(),
        fleet.id,
    )
}

/// Human-readable explanation for a rejected command. The code stays the
/// machine-readable part; this says which fleet, sector or queue is at fault.
pub fn command_error_message(engine: &GameEngine, player_id: u64, cmd: &Command, code: ErrorCode) -> String {
    let fleet_id = command_fleet_id(cmd);
    let fleet = fleet_id
        .and_then(|id| engine.fleets.get(&id))
        .filter(|f| f.owner_id == player_id && !f.ships.is_empty());
    let fid = fleet_id.unwrap_or(0);

    match (code, cmd) {
        (
            _,
            Command::Exchange { fleet_id, give, want, amount } | Command::ExchangeQuote { fleet_id, give, want, amount },
        ) => match engine.exchange_deal(player_id, *fleet_id, *give, *want, *amount) {
            Err(refusal) | Ok(ExchangeDeal { refusal: Some(refusal), .. }) => refusal.reason,
            Ok(_) => format!("command rejected ({code:?})"),
        },
        (_, Command::Load { fleet_id, resource, amount }) => {
            match engine.load_order(player_id, *fleet_id, *resource, *amount) {
                Err(refusal) => refusal.reason,
                Ok(_) => format!("command rejected ({code:?})"),
            }
        }
        (_, Command::Unload { fleet_id }) => match engine.unload_refusal(player_id, *fleet_id) {
            Some(refusal) => refusal.reason,
            None => format!("command rejected ({code:?})"),
        },
        (_, Command::Refine { fleet_id }) => match engine.refine_order(player_id, *fleet_id) {
            Err(refusal) => refusal.reason,
            Ok(_) => format!("command rejected ({code:?})"),
        },
        (_, Command::Refuel { fleet_id, give, amount }) => {
            match engine.refuel_deal(player_id, *fleet_id, *give, *amount) {
                Err(refusal) => refusal.reason,
                Ok(_) => format!("command rejected ({code:?})"),
            }
        }
        (_, Command::FoundOutpost { .. }) => match fleet.map(|f| engine.outpost_site(player_id, f, f.location)) {
            Some(Err(refusal)) => refusal.reason,
            Some(Ok(())) => format!("command rejected ({code:?})"),
            None => format!("fleet {fid} not found (or it has no ships left)"),
        },
        (_, Command::OutpostSurvey { fleet_id, sector }) => match engine.outpost_survey(player_id, *fleet_id, *sector) {
            Err(refusal) => refusal.reason,
            Ok(_) => format!("command rejected ({code:?})"),
        },
        (_, Command::FoundRelay { .. }) => match fleet.map(|f| engine.relay_site(player_id, f, f.location)) {
            Some(Err(refusal)) => refusal.reason,
            Some(Ok(())) => format!("command rejected ({code:?})"),
            None => format!("fleet {fid} not found (or it has no ships left)"),
        },
        (_, Command::RelaySurvey { fleet_id, sector }) => match engine.relay_survey(player_id, *fleet_id, *sector) {
            Err(refusal) => refusal.reason,
            Ok(_) => format!("command rejected ({code:?})"),
        },
        (_, Command::UpgradeOutpost) => match engine.outpost_upgrade(player_id) {
            Err(refusal) => refusal.reason,
            Ok(_) => format!("command rejected ({code:?})"),
        },
        (ErrorCode::InvalidCommand, Command::AbandonOutpost) => "you have no outpost to abandon".to_string(),
        (ErrorCode::OnCooldown, Command::AbandonOutpost) => {
            "your outpost is under raid; it cannot be abandoned until the fight is over".to_string()
        }
        (ErrorCode::FleetNotFound, _) => format!("fleet {fid} not found (or it has no ships left)"),
        (ErrorCode::OnCooldown, Command::Attack { target_fleet_id, .. })
            if fleet.is_some_and(|f| f.state != FleetStatus::InCombat && f.state != FleetStatus::Moving) =>
        {
            format!("hostile fleet {target_fleet_id} is already locked in combat")
        }
        (ErrorCode::OnCooldown, _) => match fleet {
            Some(f) if f.state == FleetStatus::InCombat => format!("fleet {fid} is in combat"),
            Some(f) if f.state == FleetStatus::Moving && engine.combat_of(fid).is_some() => match engine.flight_order(f) {
                Err(refusal) => refusal.reason,
                Ok(_) => format!("fleet {fid} is leaving a fight"),
            },
            Some(f) if f.state == FleetStatus::Moving => format!("fleet {fid} is in transit"),
            Some(f) if f.state == FleetStatus::Exploring => format!("fleet {fid} is busy boarding a derelict"),
            Some(f) if f.action_cooldown > 0 => {
                format!("fleet {fid} is on cooldown for {} more ticks", f.action_cooldown)
            }
            _ => format!("fleet {fid} is busy"),
        },
        (ErrorCode::NoConnection, Command::Move { target, .. } | Command::PreviewMove { target, .. }) => match fleet {
            Some(f) => format!("no lane from {} to {}", f.location, target),
            None => format!("no lane to {target}"),
        },
        (ErrorCode::InsufficientFuel, Command::Recall { .. }) => match fleet {
            Some(f) => format!(
                "fleet {fid} has {:.0} fuel; an emergency jump home costs {:.0} (twice a normal jump per hex). \
                 Walk home with move (one jump costs {:.0}) or wait for the emergency reserve",
                f.fuel, engine.recall_fuel_cost(f), engine.hop_fuel_cost(f),
            ),
            None => format!("fleet {fid} lacks the fuel for an emergency jump home"),
        },
        (ErrorCode::InsufficientFuel, Command::Move { .. }) if fleet.is_some_and(|f| f.state == FleetStatus::InCombat) => {
            match fleet.map(|f| engine.flight_order(f)) {
                Some(Err(refusal)) => refusal.reason,
                _ => format!("fleet {fid} lacks the fuel to leave the fight"),
            }
        }
        (ErrorCode::InsufficientFuel, _) => match fleet {
            Some(f) => format!(
                "fleet {fid} has {:.0} fuel; this jump costs {:.0}. A stranded fleet regains one jump of fuel every few minutes",
                f.fuel, engine.hop_fuel_cost(f),
            ),
            None => format!("fleet {fid} lacks the fuel for this jump"),
        },
        (ErrorCode::FleetLimitReached, Command::Split { .. }) => format!(
            "fleet limit reached: at most {MAX_FLEETS_TOTAL} fleets in all and {MAX_FLEETS_PER_PLAYER} deployed away from home"
        ),
        (ErrorCode::FleetLimitReached, Command::Move { .. }) if fleet.is_some_and(|f| f.state == FleetStatus::InCombat) => {
            match fleet.map(|f| engine.flight_order(f)) {
                Some(Err(refusal)) => refusal.reason,
                _ => format!("fleet limit reached: at most {MAX_FLEETS_PER_PLAYER} fleets can be deployed"),
            }
        }
        (ErrorCode::FleetLimitReached, _) => {
            format!("fleet limit reached: at most {MAX_FLEETS_PER_PLAYER} fleets can be deployed")
        }
        (ErrorCode::NotInSector, Command::Merge { other_fleet_id, .. }) => {
            format!("fleets {fid} and {other_fleet_id} are not in the same sector")
        }
        (ErrorCode::InvalidTarget, Command::Split { ships, .. }) => split_refusal(fleet, ships),
        (ErrorCode::InvalidTarget, Command::Merge { .. }) => "a fleet cannot merge with itself".to_string(),
        (ErrorCode::InvalidCommand, Command::Merge { .. }) => {
            format!("the merged fleet would exceed {MAX_FLEET_SHIPS} ships")
        }
        (ErrorCode::NoResources, Command::Harvest { .. }) => "this sector has nothing left to harvest".to_string(),
        (ErrorCode::NoResources, Command::CollectSalvage { .. }) => "no salvage in this sector".to_string(),
        (ErrorCode::StorageTooSmall, _) => storage_too_small_message(engine, player_id, cmd),
        (ErrorCode::InsufficientResources, _) => insufficient_resources_message(engine, player_id, cmd),
        (ErrorCode::DefenceLocked, Command::BuildDefence { kind, .. }) => {
            missing_prerequisites_message(engine, player_id, cmd).unwrap_or_else(|| format!("{} is locked", kind.label()))
        }
        (ErrorCode::MaxLevelReached, Command::BuildDefence { kind, .. }) => {
            let most = kind.max_count().unwrap_or(0);
            let have = engine.players.get(&player_id).map_or(0, |p| GameEngine::defences_owned_and_ordered(p, *kind));
            format!(
                "no more than {most} {} may stand; {have} stand or are on order",
                kind.label()
            )
        }
        (ErrorCode::ResourceNotPresent, Command::Harvest { resource, .. }) => {
            format!("this sector has no {} to harvest", harvest_label(*resource))
        }
        (ErrorCode::CargoFull, _) => format!("fleet {fid} cargo hold is full; return home to unload"),
        (ErrorCode::InvalidTarget, Command::Attack { target_fleet_id, .. }) => {
            format!("hostile fleet {target_fleet_id} is not in this sector")
        }
        (ErrorCode::InvalidTarget, Command::ExploreSite { .. }) => "no derelict to board in this sector".to_string(),
        (ErrorCode::InvalidCommand, Command::Stop { .. }) => format!("fleet {fid} has nothing to stop"),
        (ErrorCode::InvalidCommand, Command::Recall { .. }) => {
            let rule = "recall only brings a fleet home from another sector";
            match fleet.map(|f| (f.state, f.move_target)) {
                Some((FleetStatus::Moving, Some(target))) => format!(
                    "fleet {fid} is at its homeworld, charging a jump to {target}; {rule}. \
                     To call the jump off use stop (the fuel it burned is not returned)"
                ),
                Some((FleetStatus::Harvesting | FleetStatus::Exploring, _)) => {
                    format!("fleet {fid} is at its homeworld and at work there; {rule}. To halt it use stop")
                }
                _ => format!("fleet {fid} is already at its homeworld; {rule}"),
            }
        }
        (ErrorCode::InvalidCommand, Command::Odds { sectors, .. }) => {
            format!("odds takes at most {MAX_ODDS_SECTORS} sectors a request; {} were named", sectors.len())
        }
        (ErrorCode::QueueFull, Command::Build { .. }) => {
            let slots = engine.players.get(&player_id).map_or(1, |p| p.building_slots());
            format!(
                "the building queue is full ({slots} running + {} waiting); cancel a waiting order with cancel_queued or wait for one to start",
                iac_shared::scaling::QUEUE_WAITING
            )
        }
        (ErrorCode::QueueFull, Command::Research { .. }) => {
            format!(
                "the research queue is full (1 running + {} waiting); cancel a waiting order with cancel_queued or wait for one to start",
                iac_shared::scaling::QUEUE_WAITING
            )
        }
        (ErrorCode::QueueFull, Command::BuildShip { .. } | Command::BuildDefence { .. }) => {
            format!(
                "the shipyard queue is full (1 running + {} waiting); cancel a waiting order with cancel_queued or wait for one to start",
                iac_shared::scaling::QUEUE_WAITING
            )
        }
        (ErrorCode::InvalidTarget, Command::CancelBuild { queue_type, id }) => {
            cancel_refusal(engine, player_id, *queue_type, *id, QueueSlot::Running)
        }
        (ErrorCode::InvalidTarget, Command::CancelQueued { queue_type, id }) => {
            cancel_refusal(engine, player_id, *queue_type, *id, QueueSlot::Waiting)
        }
        (ErrorCode::InvalidCommand, Command::BuildShip { .. } | Command::BuildDefence { .. }) => "a shipyard order needs a count of at least 1".to_string(),
        (ErrorCode::MaxLevelReached, Command::Build { building_type, .. }) => {
            format!("{} is already at its maximum level", building_type.label())
        }
        (ErrorCode::MaxLevelReached, Command::Research { tech, .. }) => {
            format!("{} is already at its maximum level", tech.label())
        }
        (
            ErrorCode::PrerequisitesNotMet | ErrorCode::NoShipyard | ErrorCode::NoResearchLab | ErrorCode::ShipLocked,
            Command::Build { .. } | Command::Research { .. } | Command::BuildShip { .. } | Command::BuildDefence { .. },
        ) => missing_prerequisites_message(engine, player_id, cmd)
            .unwrap_or_else(|| format!("command rejected ({code:?})")),
        (ErrorCode::ServerError, _) => "internal server error".to_string(),
        (code, _) => format!("command rejected ({code:?})"),
    }
}
