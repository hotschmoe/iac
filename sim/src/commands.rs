// Client command handling: routes a protocol `Command` to the engine and words
// the refusal. Pure state in, state out; hosts add transport and sessions.

use iac_shared::constants::{MAX_FLEETS_PER_PLAYER, MAX_FLEETS_TOTAL};
use iac_shared::protocol::{Command, ErrorCode, HarvestResource, LeaderboardReply, ServerMessage};

use crate::engine::{AuthError, FleetStatus, GameEngine};

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
            Command::BuildShip { ship_class, count, reserve } => self.handle_build_ship(pid, ship_class, count, reserve),
            Command::BuildDefence { kind, count, reserve } => self.handle_build_defence(pid, kind, count, reserve),
            Command::CancelBuild { queue_type, id } => self.handle_cancel_build(pid, queue_type, id),
            Command::CancelQueued { queue_type, id } => self.handle_cancel_queued(pid, queue_type, id),
            Command::Stop { fleet_id } => self.handle_stop(pid, fleet_id),
            Command::Scan { fleet_id } => self.handle_scan(pid, fleet_id),
            Command::ExploreSite { fleet_id } => self.handle_explore_site(pid, fleet_id),
            Command::Split { fleet_id, ref ship_ids } => self.handle_split(pid, fleet_id, ship_ids).map(|_| ()),
            Command::Merge { fleet_id, other_fleet_id } => self.handle_merge(pid, fleet_id, other_fleet_id),
            Command::Leaderboard { limit } => {
                let (entries, you) = crate::score::leaderboard(self, limit as usize, pid);
                reply = Some(ServerMessage::Leaderboard(LeaderboardReply {
                    tick: self.current_tick(),
                    entries,
                    you,
                }));
                Ok(())
            }
            Command::PreviewMove { fleet_id, target } => self.preview_move(pid, fleet_id, target).map(|p| {
                reply = Some(ServerMessage::PreviewMove(p));
            }),
        };
        let error = result.err().map(|code| (code, command_error_message(self, pid, &cmd, code)));
        CommandOutcome { reply, error }
    }
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
        AuthError::Server => (ErrorCode::ServerError, "could not create the account".to_string()),
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
        | Command::Merge { fleet_id, .. } => Some(*fleet_id),
        Command::Build { .. }
        | Command::Research { .. }
        | Command::BuildShip { .. }
        | Command::BuildDefence { .. }
        | Command::Leaderboard { .. }
        | Command::CancelBuild { .. }
        | Command::CancelQueued { .. } => None,
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
    use iac_shared::protocol::{building_requirements, missing_requirements_message, research_requirements, ship_requirements};
    let p = engine.players.get(&player_id)?;
    let (buildings, research) = (p.projected_buildings(), p.projected_research());
    let (subject, requires) = match cmd {
        Command::Build { building_type, .. } => (building_type.label(), building_requirements(&buildings, *building_type)),
        Command::Research { tech, .. } => (tech.label(), research_requirements(&buildings, &research, *tech)),
        Command::BuildShip { ship_class, .. } => (ship_class.label(), ship_requirements(&buildings, &research, *ship_class)),
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

/// Names what a rejected order costs and which Storage Vault level would hold it.
fn storage_too_small_message(engine: &GameEngine, player_id: u64, cmd: &Command) -> String {
    use iac_shared::constants::ResourceKind;
    let (Some((label, cost)), Some(p)) = (engine.command_cost(player_id, cmd), engine.players.get(&player_id)) else {
        return "the payment is larger than a storage cap".to_string();
    };
    let cap = iac_shared::scaling::storage_cap(p.buildings.storage_vault, &engine.pace());
    let worst = ResourceKind::ALL
        .into_iter()
        .filter(|&k| cost.get(k) > cap.get(k))
        .max_by(|&a, &b| (cost.get(a) / cap.get(a)).total_cmp(&(cost.get(b) / cap.get(b))))
        .unwrap_or(ResourceKind::Metal);
    let need = ResourceKind::ALL
        .into_iter()
        .filter_map(|k| iac_shared::scaling::vault_level_for(cost.get(k), k, &engine.pace()))
        .max();
    let fix = match need {
        Some(level) => format!("needs Storage Vault level {level}"),
        None => "no Storage Vault can hold it".to_string(),
    };
    format!(
        "{label} costs {} {}, more than the {} {} your stockpile holds; {fix}",
        cost.get(worst).ceil(), worst.label(), cap.get(worst).floor(), worst.label(),
    )
}

/// Human-readable explanation for a rejected command. The code stays the
/// machine-readable part; this says which fleet, sector or queue is at fault.
pub fn command_error_message(engine: &GameEngine, player_id: u64, cmd: &Command, code: ErrorCode) -> String {
    let fleet_id = command_fleet_id(cmd);
    let fleet = fleet_id
        .and_then(|id| engine.fleets.get(&id))
        .filter(|f| f.owner_id == player_id && f.ship_count > 0);
    let fid = fleet_id.unwrap_or(0);

    match (code, cmd) {
        (ErrorCode::FleetNotFound, _) => format!("fleet {fid} not found (or it has no ships left)"),
        (ErrorCode::OnCooldown, Command::Attack { target_fleet_id, .. })
            if fleet.is_some_and(|f| f.state != FleetStatus::InCombat && f.state != FleetStatus::Moving) =>
        {
            format!("hostile fleet {target_fleet_id} is already locked in combat")
        }
        (ErrorCode::OnCooldown, _) => match fleet {
            Some(f) if f.state == FleetStatus::InCombat => format!("fleet {fid} is in combat"),
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
        (ErrorCode::FleetLimitReached, _) => {
            format!("fleet limit reached: at most {MAX_FLEETS_PER_PLAYER} fleets can be deployed")
        }
        (ErrorCode::NotInSector, Command::Merge { other_fleet_id, .. }) => {
            format!("fleets {fid} and {other_fleet_id} are not in the same sector")
        }
        (ErrorCode::InvalidTarget, Command::Split { .. }) => {
            "split needs at least one ship id aboard the fleet, and at least one ship must stay behind".to_string()
        }
        (ErrorCode::InvalidTarget, Command::Merge { .. }) => "a fleet cannot merge with itself".to_string(),
        (ErrorCode::InvalidCommand, Command::Merge { .. }) => "the merged fleet would exceed 64 ships".to_string(),
        (ErrorCode::NoResources, Command::Harvest { .. }) => "this sector has nothing left to harvest".to_string(),
        (ErrorCode::NoResources, Command::CollectSalvage { .. }) => "no salvage in this sector".to_string(),
        (ErrorCode::StorageTooSmall, _) => storage_too_small_message(engine, player_id, cmd),
        (ErrorCode::DefenceLocked, Command::BuildDefence { kind, .. }) => engine
            .players
            .get(&player_id)
            .and_then(|p| iac_shared::protocol::missing_requirements_message(kind.label(), &iac_shared::protocol::defence_requirements(&p.buildings, &p.research, *kind)))
            .unwrap_or_else(|| format!("{} is locked", kind.label())),
        (ErrorCode::ResourceNotPresent, Command::Harvest { resource, .. }) => {
            format!("this sector has no {} to harvest", harvest_label(*resource))
        }
        (ErrorCode::CargoFull, _) => format!("fleet {fid} cargo hold is full; return home to unload"),
        (ErrorCode::InvalidTarget, Command::Attack { target_fleet_id, .. }) => {
            format!("hostile fleet {target_fleet_id} is not in this sector")
        }
        (ErrorCode::InvalidTarget, Command::ExploreSite { .. }) => "no derelict to board in this sector".to_string(),
        (ErrorCode::InvalidCommand, Command::Stop { .. }) => format!("fleet {fid} has nothing to stop"),
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
            Command::Build { .. } | Command::Research { .. } | Command::BuildShip { .. },
        ) => missing_prerequisites_message(engine, player_id, cmd)
            .unwrap_or_else(|| format!("command rejected ({code:?})")),
        (ErrorCode::ServerError, _) => "internal server error".to_string(),
        (code, _) => format!("command rejected ({code:?})"),
    }
}
