// One player's whole arc through the public command surface at test pace:
// the world as a client sees it (`views`), commands as a client sends them
// (`GameEngine::execute`), time passing only through `tick`.

#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::constants::{Resources, ShipClass};
use iac_shared::hex::Hex;
use iac_shared::protocol::{Command, ErrorCode, EventKind, QueueAction, QueueEvent, QueueType, ServerMessage, WaitReason};
use iac_shared::scaling::{self, BuildingType, ResearchType};
use iac_shared::world::NpcBehaviorType;
use iac_sim::engine::{template_npc_id, AuthError};
use iac_sim::score;

fn recruit(name: &str) -> Empire {
    let mut engine = world("test");
    let issued = "t".repeat(64);
    let auth = engine.authenticate(name, None, false, || issued.clone()).expect("registration");
    assert!(auth.registered && auth.new_token.as_deref() == Some(issued.as_str()));
    let again = engine.authenticate(&name.to_uppercase(), auth.new_token.as_deref(), false, || unreachable!());
    assert!(matches!(again, Ok(a) if a.player_id == auth.player_id && !a.registered), "names are case-insensitive accounts");
    assert!(matches!(engine.authenticate(name, None, false, || issued.clone()), Err(AuthError::TokenRequired)));
    engine.authenticate("Idler", None, false, || issued.clone()).expect("a second empire");
    let pid = auth.player_id;
    let fid = engine.fleets.values().find(|f| f.owner_id == pid).expect("starting fleet").id;
    Empire { engine, pid, fid, log: Vec::new() }
}

fn chart_the_neighbourhood(e: &mut Empire) {
    let home = e.engine.players[&e.pid].homeworld;
    let quiet = |h: Hex| e.engine.world_gen.generate_sector(h).npc_template.is_none();
    let rivals: Vec<Hex> = e.engine.players.values().map(|p| p.homeworld).collect();
    let out = e
        .engine
        .world_gen
        .connected_neighbors(home)
        .slice()
        .iter()
        .copied()
        .find(|&h| quiet(h) && !rivals.contains(&h))
        .expect("a quiet neighbour");

    let mark = e.mark();
    e.travel(out);
    assert_eq!(e.engine.players[&e.pid].explore_points, 0.0, "nothing is credited in the field");
    e.travel(home);
    let delivered = e.since(mark, |k| match k {
        EventKind::ChartDelivered(d) => Some(d.clone()),
        _ => None,
    });
    assert_eq!(delivered.len(), 1);
    let paid = e.engine.players[&e.pid].explore_points;
    assert!(paid > 0.0 && (paid - delivered[0].points).abs() < 1e-5);

    let row = e.score_row();
    assert!(row.explore > score::EXTRA_CAP * row.core, "the chart outweighs a new empire's core");
    assert!((row.score - row.core * (1.0 + score::EXTRA_CAP)).abs() < 1e-4, "extras are capped at a quarter of the core");

    e.travel(out);
    e.travel(home);
    assert_eq!(e.engine.players[&e.pid].explore_points, paid, "the same sector never pays twice");
}

fn item_started(events: &[QueueEvent], item: &str) -> Option<usize> {
    events.iter().position(|q| q.action == QueueAction::Started && q.item.starts_with(item))
}

fn build_the_foundations(e: &mut Empire) {
    let mark = e.mark();
    for building in [BuildingType::MetalMine, BuildingType::CrystalMine, BuildingType::Shipyard, BuildingType::ResearchLab] {
        e.order(Command::Build { building_type: building, reserve: false }).expect("the line has room");
    }
    assert_eq!(
        e.order(Command::Build { building_type: BuildingType::DeuteriumSynthesizer, reserve: false }),
        Err(ErrorCode::QueueFull)
    );

    let events = e.queue_events(mark);
    let waiting: Vec<_> = events.iter().filter(|q| q.action == QueueAction::Waiting).collect();
    assert_eq!(events[0].action, QueueAction::Started);
    assert_eq!(waiting.iter().map(|q| q.waiting_on).collect::<Vec<_>>(), [Some(WaitReason::Slot), Some(WaitReason::Prerequisite), Some(WaitReason::Prerequisite)]);
    let lab = waiting[2].id;

    assert_eq!(e.order(Command::CancelBuild { queue_type: QueueType::Building, id: lab }), Err(ErrorCode::InvalidTarget), "waiting is not running");
    assert_eq!(e.order(Command::CancelQueued { queue_type: QueueType::Ship, id: lab }), Err(ErrorCode::InvalidTarget), "wrong queue");
    let cancel_mark = e.mark();
    e.order(Command::CancelQueued { queue_type: QueueType::Building, id: lab }).expect("cancel the waiting lab by id");
    let cancelled = e.queue_events(cancel_mark);
    assert_eq!((cancelled[0].id, cancelled[0].action, cancelled[0].refunded), (lab, QueueAction::Cancelled, Resources::default()));
    assert_eq!(e.order(Command::CancelQueued { queue_type: QueueType::Building, id: lab }), Err(ErrorCode::InvalidTarget), "already gone");

    e.order(Command::Build { building_type: BuildingType::DeuteriumSynthesizer, reserve: false }).expect("room again");
    e.run_until(100, |e| {
        let home = e.state().homeworld;
        home.build_queue.is_empty() && home.build_pending.is_empty()
    });
    let done: Vec<_> = e.since(mark, |k| match k {
        EventKind::BuildingCompleted(b) => Some((b.building_type, b.new_level)),
        _ => None,
    });
    assert_eq!(
        done,
        [(BuildingType::MetalMine, 2), (BuildingType::CrystalMine, 2), (BuildingType::Shipyard, 1), (BuildingType::DeuteriumSynthesizer, 1)],
        "the order the line ran in"
    );
}

#[test]
fn an_empire_grows_fights_charts_and_survives_a_raid() {
    let mut e = recruit("Arc");
    let start = e.state();
    assert_eq!(start.fleets.len(), 1);
    assert_eq!(start.fleets[0].ships.len(), 2);
    chart_the_neighbourhood(&mut e);
    build_the_foundations(&mut e);
    mind_the_queue(&mut e);
    research_a_chain(&mut e);
    let (lair, power) = hostile_lair(&e);
    let need = ((3.0 * power - e.fleet().power) / scaling::ship_power(ShipClass::Corvette)).ceil().max(3.0) as u16;
    build_a_battle_group(&mut e, need);
    hunt_pirates(&mut e, lair);
    withstand_a_raid(&mut e);
    hold_the_line(&mut e, 4000);
    settle_the_score(&mut e);
}

fn mind_the_queue(e: &mut Empire) {
    let pid = e.pid;
    let next_cost = |e: &Empire, b: BuildingType| {
        e.state().homeworld.catalog.buildings.iter().find(|o| o.building_type == b).and_then(|o| o.next).expect("upgrade available").cost
    };
    let cheap = next_cost(e, BuildingType::MetalMine);
    assert!(next_cost(e, BuildingType::DeuteriumSynthesizer).metal > cheap.metal);
    edit(&mut e.engine, |s| {
        let player = s.players.get_mut(&pid).unwrap();
        player.resources = cheap;
        player.research.modular_fabrication = 1;
    });
    assert_eq!(e.state().homeworld.build_slots, 2);
    let mark = e.mark();
    e.order(Command::Build { building_type: BuildingType::DeuteriumSynthesizer, reserve: true }).expect("reserving order");
    e.order(Command::Build { building_type: BuildingType::MetalMine, reserve: false }).expect("cheap order behind it");
    let pending = e.state().homeworld.build_pending;
    assert_eq!(
        pending.iter().map(|q| q.waiting_on).collect::<Vec<_>>(),
        [WaitReason::Resources, WaitReason::Order],
        "the cheap order may not jump a reserving one"
    );
    e.run_until(200, |e| e.state().homeworld.build_pending.is_empty());
    let started = e.queue_events(mark);
    let deuterium = item_started(&started, "Deut").expect("reserving order started");
    let metal = item_started(&started, "Metal Mine").expect("cheap order started");
    assert!(deuterium < metal);

    e.run(30);
    let mark = e.mark();
    e.order(Command::Build { building_type: BuildingType::StorageVault, reserve: false }).expect("vault");
    let running = e.queue_events(mark);
    let (id, paid) = (running[0].id, running[0].paid);
    e.order(Command::CancelBuild { queue_type: QueueType::Building, id }).expect("cancel the running vault by id");
    let cancelled = e.queue_events(mark);
    let refund = cancelled.iter().find(|q| q.action == QueueAction::Cancelled).expect("cancel confirmed");
    assert_eq!(refund.id, id);
    assert_eq!(refund.refunded, paid.scale(scaling::CANCEL_REFUND_FRACTION), "half of what it paid comes back");
}

fn research_a_chain(e: &mut Empire) {
    let mark = e.mark();
    for building in [BuildingType::ResearchLab, BuildingType::ResearchLab] {
        e.order(Command::Build { building_type: building, reserve: false }).expect("lab level queued");
    }
    for tech in [ResearchType::Navigation, ResearchType::Navigation, ResearchType::CorvetteTech] {
        e.order(Command::Research { tech, reserve: false }).expect("research queues behind the lab that is itself queued");
    }
    let waiting = e.state().homeworld.research_pending;
    assert_eq!((waiting[0].tech, waiting[0].target_level, waiting[0].waiting_on), (ResearchType::Navigation, 1, WaitReason::Prerequisite));
    assert_eq!((waiting[1].tech, waiting[1].target_level), (ResearchType::Navigation, 2), "a second order builds on the first");

    e.run_until(300, |e| {
        let home = e.state().homeworld;
        home.research_active.is_none() && home.research_pending.is_empty() && home.build_queue.is_empty() && home.build_pending.is_empty()
    });
    let finished: Vec<String> = e
        .log[mark..]
        .iter()
        .filter_map(|ev| match &ev.kind {
            EventKind::BuildingCompleted(b) if b.building_type == BuildingType::ResearchLab => Some(format!("lab{}", b.new_level)),
            EventKind::ResearchCompleted(r) => Some(format!("{:?}{}", r.tech, r.new_level)),
            _ => None,
        })
        .collect();
    let at = |name: &str| finished.iter().position(|f| f == name).unwrap_or_else(|| panic!("{name} missing from {finished:?}"));
    assert!(at("lab2") < at("Navigation1") && at("Navigation1") < at("Navigation2"), "{finished:?}");
    at("CorvetteTech1");
}

fn hostile_lair(e: &Empire) -> (Hex, f32) {
    let home = e.engine.players[&e.pid].homeworld;
    let homes: Vec<Hex> = e.engine.players.values().map(|p| p.homeworld).collect();
    let mut best: Option<(usize, Hex, f32)> = None;
    for q in -9i16..=9 {
        for r in -9i16..=9 {
            let at = Hex { q: home.q + q, r: home.r + r };
            let Some(t) = e.engine.world_gen.generate_sector(at).npc_template else { continue };
            if t.behavior != NpcBehaviorType::Patrol || t.power() < 10.0 || homes.contains(&at) || e.engine.derelict_site_at(at).is_some() {
                continue;
            }
            let Some(path) = lane_path(&e.engine, home, at, 8) else { continue };
            if best.is_none_or(|(hops, ..)| path.len() < hops) {
                best = Some((path.len(), at, t.power()));
            }
        }
    }
    let (_, at, power) = best.expect("a pirate lair within eight hops");
    (at, power)
}

fn build_a_battle_group(e: &mut Empire, count: u16) {
    let pid = e.pid;
    let unit = ShipClass::Corvette.build_cost();
    edit(&mut e.engine, |s| s.players.get_mut(&pid).unwrap().resources = unit.scale(2.0));
    let mark = e.mark();
    e.order(Command::BuildShip { ship_class: ShipClass::Corvette, count, reserve: false }).expect("batch accepted");
    let started = e.queue_events(mark);
    assert_eq!(started.iter().filter(|q| q.action == QueueAction::Started).count(), 1);
    assert_eq!(started[0].paid, unit, "a batch pays one unit as it starts");
    assert_eq!(e.state().player.resources, unit, "the stockpile paid one unit, not the batch");

    let mut starved = false;
    e.run_until(2000, |e| {
        let home = e.state().homeworld;
        starved |= home.shipyard_pending.iter().any(|q| q.waiting_on == WaitReason::Resources && q.built > 0 && q.unit_cost == unit);
        home.shipyard_queue.is_none() && home.shipyard_pending.is_empty()
    });
    assert!(starved, "the batch waited for money between units");
    let built: u16 = e.since(mark, |k| match k {
        EventKind::ShipBuilt(b) if b.ship_class == ShipClass::Corvette => Some(b.count),
        _ => None,
    }).iter().sum();
    assert_eq!(built, count);
    assert_eq!(e.fleet().ships.len(), 2 + usize::from(count), "new hulls reinforce the fleet at home");
}

fn hunt_pirates(e: &mut Empire, lair: Hex) {
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;

    let mut piles = Vec::new();
    let mut points = Vec::new();
    for round in 0..4 {
        let before = e.engine.players[&pid].combat_points;
        let mark = e.mark();
        if round == 0 {
            e.travel(lair);
        } else {
            e.run_until(3000, |e| e.engine.pending_template_npc(lair).is_some());
        }
        let engaged = |e: &Empire| e.since(mark, |k| matches!(k, EventKind::CombatStarted(_)).then_some(())).len();
        if engaged(e) == 0 {
            e.order(Command::Attack { fleet_id: fid, target_fleet_id: template_npc_id(lair) }).expect("attack the listed hostile");
        }
        e.run_until(120, |e| e.engine.active_combats.is_empty() && engaged(e) > 0);
        let (pile, _) = pile_of(&e.log[mark..]);
        assert!(e.log[mark..].iter().any(|ev| matches!(&ev.kind, EventKind::CombatEnded(c) if c.player_victory && c.mine)));
        piles.push(pile.total());
        points.push(e.engine.players[&pid].combat_points - before);

        e.order(Command::CollectSalvage { fleet_id: fid }).expect("scoop the wreckage");
        let collected = e.since(mark, |k| match k {
            EventKind::SalvageCollected(c) => Some(c.resources),
            _ => None,
        });
        assert_eq!(collected.last(), Some(&pile), "the hold takes exactly what was advertised");
    }
    assert!(points[0] > 0.0);
    assert!(piles.windows(2).all(|w| w[1] < w[0]), "each repeat kill leaves less: {piles:?}");
    assert!(points.windows(2).all(|w| w[1] < w[0]), "and pays less: {points:?}");

    let stock = e.state().player.resources;
    e.travel(home);
    e.run(2);
    assert_eq!(e.fleet().cargo, Resources::default(), "docking unloads the hold");
    let unloaded = e.state().player.resources;
    assert!(unloaded.metal > stock.metal && unloaded.crystal > stock.crystal);
}

fn withstand_a_raid(e: &mut Empire) {
    let home_sector = e.engine.players[&e.pid].homeworld;
    e.order(Command::Build { building_type: BuildingType::Shipyard, reserve: false }).expect("second shipyard level");
    e.run_until(100, |e| e.state().homeworld.build_queue.is_empty() && e.state().homeworld.build_pending.is_empty());
    let mark = e.mark();

    let home = e.state().homeworld;
    let wanted = 1.6 * home.next_raid_estimate_power;
    if home.home_defence_power < wanted {
        let count = ((wanted - home.home_defence_power) / scaling::ship_power(ShipClass::Corvette)).ceil() as u16;
        e.order(Command::BuildShip { ship_class: ShipClass::Corvette, count, reserve: false }).expect("reinforcements");
        e.run_until(3000, |e| e.state().homeworld.shipyard_queue.is_none() && e.state().homeworld.shipyard_pending.is_empty());
    }
    assert!(e.state().homeworld.home_defence_power >= wanted);

    e.run_until(5000, |e| e.log[mark..].iter().any(|ev| matches!(ev.kind, EventKind::RaidIncoming(_))));
    let forecast = e.since(mark, |k| match k {
        EventKind::RaidIncoming(r) => Some(r.clone()),
        _ => None,
    });
    let warned_at = e.engine.current_tick();
    assert_eq!(forecast[0].arrival_tick - warned_at, e.engine.pace().raid_warning_ticks());

    e.run_until(100, |e| e.log[mark..].iter().any(|ev| matches!(ev.kind, EventKind::RaidResolved(_))));
    let resolved = e.since(mark, |k| match k {
        EventKind::RaidResolved(r) => Some(r.clone()),
        _ => None,
    });
    assert!(resolved[0].defended, "{:?}", resolved[0]);
    assert_eq!((resolved[0].ships_lost, resolved[0].resources_lost), (0, Resources::default()));
    assert!((resolved[0].raid_power - forecast[0].est_power).abs() < 1e-3, "the forecast was the raid");
    let pile = resolved[0].salvage_dropped.expect("a repelled raid leaves wreckage");

    e.run(2);
    let scooped = e.since(mark, |k| match k {
        EventKind::SalvageCollected(c) if c.sector == home_sector => Some(c.resources),
        _ => None,
    });
    assert_eq!(scooped, [pile], "the docked fleet scoops the wreckage into storage");
}

fn settle_the_score(e: &mut Empire) {
    let state = e.state();
    let mut buildings = scaling::BuildingLevels::default();
    for b in &state.homeworld.buildings {
        buildings.set(b.building_type, b.level);
    }
    let mut research = scaling::ResearchLevels::default();
    for r in &state.homeworld.research {
        research.set(r.tech, r.level);
    }
    let fleet: f32 = state
        .fleets
        .iter()
        .flat_map(|f| &f.ships)
        .filter(|s| s.hull > 0.0)
        .map(|s| scaling::resource_weight(&s.ship_class.build_cost()))
        .sum();
    let core = scaling::econ_points(&buildings, &research) + fleet;

    let row = e.score_row();
    assert!((row.core - core).abs() < 1e-3 * core, "core {} vs what the empire holds {core}", row.core);
    let player = &e.engine.players[&e.pid];
    assert_eq!((row.combat, row.explore), (player.combat_points, player.explore_points));
    assert!(row.combat > 0.0 && row.explore > 0.0);
    let expected = core + (row.combat + row.explore).min(score::EXTRA_CAP * core);
    assert!((row.score - expected).abs() < 1e-3 * expected, "{} vs {expected}", row.score);

    let outcome = e.engine.execute(e.pid, Command::Leaderboard { limit: 10 });
    let Some(ServerMessage::Leaderboard(table)) = outcome.reply else { panic!("no table") };
    let names: Vec<_> = table.entries.iter().map(|r| (r.rank, r.name.as_str())).collect();
    assert_eq!(names, [(1, "Arc"), (2, "Idler")]);
    assert!(table.you.is_none());
}

fn hold_the_line(e: &mut Empire, ticks: u64) {
    let mark = e.mark();
    for _ in 0..ticks {
        e.run(1);
        let home = e.state().homeworld;
        let wanted = 1.6 * home.next_raid_estimate_power;
        if home.home_defence_power < wanted && home.shipyard_queue.is_none() && home.shipyard_pending.is_empty() {
            let count = ((wanted - home.home_defence_power) / scaling::ship_power(ShipClass::Corvette)).ceil() as u16;
            e.order(Command::BuildShip { ship_class: ShipClass::Corvette, count, reserve: false }).expect("reinforcements");
        }
    }
    let raids = e.since(mark, |k| match k {
        EventKind::RaidResolved(r) => Some(r.clone()),
        _ => None,
    });
    assert!(raids.len() >= 2, "a long watch sees several raids: {}", raids.len());
    assert!(raids.iter().all(|r| r.defended && r.ships_lost == 0), "{raids:?}");
}
