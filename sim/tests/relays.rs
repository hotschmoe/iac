// Relays: a depot of deuterium on a sector of ring 9 or deeper, owner-only.
// A convoy hauls fuel to one, a fleet that could not have gone so far
// refuels from it and comes home, every rule that refuses a founding, the
// hold moved to and from the store, a relay nobody visits being gone, and
// the whole of it stopped and read back from its document.

#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::constants::{
    ResourceKind, Resources, ShipClass, TerrainType, FUEL_DEUT_PER_UNIT, RELAY_BASE_MAX, RELAY_LEVELS_PER_STEP,
    RELAY_STORE,
};
use iac_shared::hex::{hex_ring, Hex};
use iac_shared::protocol::{AlertLevel, Command, ErrorCode, EventKind, FleetState, RelayState, ServerMessage};
use iac_shared::scaling::{self, ship_tank};
use iac_shared::world;
use iac_sim::engine::{Fleet, FleetStatus, Ships};
use iac_sim::GameEngine;

use ResourceKind::{Crystal, Deuterium, Metal};

const STRIKE_SHIPS: u32 = 10;
const HOME_RING: u16 = 4;
const FAR_RING: u16 = 15;

fn res(metal: f64, crystal: f64, deuterium: f64) -> Resources {
    Resources { metal, crystal, deuterium }
}

fn open_ground(t: &world::SectorTemplate) -> bool {
    t.npc_template.is_none() && !t.has_derelict && t.terrain != TerrainType::Anomaly
}

fn find_deep(engine: &GameEngine, wanted: impl Fn(&world::SectorTemplate) -> bool) -> Hex {
    (9..=20u16)
        .flat_map(|ring| hex_ring(Hex::ORIGIN, ring))
        .find(|at| wanted(&engine.world_gen.generate_sector(*at)))
        .expect("such a sector in rings 9 to 20")
}

fn stow(e: &mut Empire, fid: u64, cargo: Resources) {
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.cargo = cargo;
        fleet.state = FleetStatus::Idle;
        fleet.move_target = None;
    });
}

fn fleet_of(e: &Empire, fid: u64) -> FleetState {
    e.state().fleets.into_iter().find(|f| f.id == fid).expect("fleet")
}

fn relays(e: &Empire) -> Vec<RelayState> {
    e.state().relays.relays
}

fn alerts(e: &Empire, from: usize) -> Vec<(AlertLevel, String)> {
    e.since(from, |kind| match kind {
        EventKind::Alert(alert) => Some((alert.level, alert.message.clone())),
        _ => None,
    })
}

fn ask(e: &mut Empire, sector: Option<Hex>) -> iac_shared::protocol::RelaySurvey {
    let outcome = e.engine.execute(e.pid, Command::RelaySurvey { fleet_id: e.fid, sector });
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let Some(ServerMessage::RelaySurvey(survey)) = outcome.reply else { panic!("no relay survey") };
    survey
}

/// An empire with a ring-4 home, a strike fleet of Light Fighters at it and a
/// hauler of two Large Cargo beside them, both with full tanks, and the lane
/// out of home swept of groups.
struct Convoy {
    e: Empire,
    home: Hex,
    chain: Vec<Hex>,
    hauler: u64,
}

fn convoy() -> Convoy {
    let mut engine = world("season");
    let (pid, fid) = join(&mut engine, "Courier");
    let home = world::home_seats().find(|at| at.dist_from_origin() == HOME_RING).expect("a ring-4 seat");
    edit(&mut engine, |s| {
        s.players.get_mut(&pid).unwrap().homeworld = home;
        s.fleets.get_mut(&fid).unwrap().location = home;
    });
    let quiet = |at: Hex| engine.world_gen.generate_sector(at).npc_template.is_none();
    let chain = hex_ring(Hex::ORIGIN, FAR_RING)
        .filter(|at| quiet(*at))
        .find_map(|at| lane_path_through(&engine, home, at, 24, &quiet))
        .expect("a quiet lane from home to the far ring");
    let strike = Ships::of(&[(ShipClass::LightFighter, STRIKE_SHIPS)]);
    let hold = Ships::of(&[(ShipClass::LargeCargo, 2)]);
    edit(&mut engine, |s| {
        let none = scaling::ResearchLevels::default();
        let tank = |ships: &Ships| -> f64 { ships.counts().map(|(class, n)| f64::from(n) * ship_tank(class, &none)).sum() };
        let main = s.fleets.get_mut(&fid).unwrap();
        main.fuel_max = tank(&strike);
        main.fuel = main.fuel_max;
        main.ships = strike;
        let mut second: Fleet = main.clone();
        second.id = s.next_id;
        s.next_id += 1;
        second.fuel_max = tank(&hold);
        second.fuel = second.fuel_max;
        second.ships = hold;
        s.fleets.insert(second.id, second);
    });
    run(&mut engine, 2);
    let stocked = &engine.fleets[&fid];
    assert_eq!(stocked.fuel, stocked.fuel_max);
    let hauler = engine.fleets.keys().copied().find(|id| *id != fid && engine.fleets[id].owner_id == pid).expect("hauler");
    let mut convoy = Convoy { e: Empire { engine, pid, fid, log: Vec::new() }, home, chain, hauler };
    sweep(&mut convoy.e.engine, &convoy.chain.clone());
    convoy
}

#[test]
fn a_relay_that_a_convoy_stocked_carries_a_fleet_past_what_its_tank_allows() {
    let Convoy { mut e, home, chain, hauler } = convoy();
    let fid = e.fid;

    let mut control = Empire { engine: GameEngine::restore(e.engine.snapshot()), pid: e.pid, fid, log: Vec::new() };
    let mut reached = 0;
    for &hop in &chain {
        if control.fleet().fuel < control.fleet().jump_fuel {
            break;
        }
        control.travel(hop);
        reached += 1;
    }
    let stuck = control.fleet();
    assert!(stuck.fuel < stuck.jump_fuel, "without a relay the fleet arrives with no jump left: {} fuel", stuck.fuel);
    let (code, _) = control.refusal(Command::Move { fleet_id: fid, target: chain[reached - 2] });
    assert_eq!(code, ErrorCode::InsufficientFuel, "and cannot come back");

    let chain = chain[..reached].to_vec();
    let jumps = reached;
    let depth = chain
        .iter()
        .position(|at| at.dist_from_origin() >= 9 && open_ground(&e.engine.world_gen.generate_sector(*at)))
        .expect("the lane reaches ring 9");
    let target = chain.len() - 1;
    let relay_at = chain[depth];
    let ring = relay_at.dist_from_origin();
    let cost = scaling::relay_cost(ring);
    assert!(
        depth < target && 2 * (target - depth) <= jumps,
        "the lane is laid out for the relay to be used: relay {depth}, target {target}, jumps {jumps}",
    );

    let manifest = cost.add(res(0.0, 0.0, 600.0));
    e.set_stockpile(manifest);
    for (resource, amount) in [(Metal, manifest.metal), (Crystal, manifest.crystal), (Deuterium, manifest.deuterium)] {
        e.order(Command::Load { fleet_id: hauler, resource, amount: Some(amount) }).expect("loaded at home");
    }
    assert_eq!(e.engine.fleets[&hauler].cargo, manifest);
    for &hop in &chain[..=depth] {
        e.travel_fleet(hauler, hop);
    }
    assert!(!fleet_of(&e, hauler).at_relay, "no relay stands there yet");
    let survey = {
        let outcome = e.engine.execute(e.pid, Command::RelaySurvey { fleet_id: hauler, sector: None });
        let Some(ServerMessage::RelaySurvey(survey)) = outcome.reply else { panic!("no relay survey") };
        survey
    };
    assert_eq!((survey.refusal.as_deref(), survey.cost, survey.ring), (None, cost, ring));
    e.order(Command::FoundRelay { fleet_id: hauler }).expect("the relay is founded from the hold");
    assert_eq!(e.engine.fleets[&hauler].cargo, res(0.0, 0.0, 600.0), "the hold paid the cost and nothing else");
    assert!(fleet_of(&e, hauler).at_relay);
    let founded = relays(&e);
    assert_eq!((founded.len(), founded[0].sector, founded[0].store), (1, relay_at, 0.0));

    let told = e.mark();
    e.order(Command::Unload { fleet_id: hauler }).expect("a hold is emptied into the relay");
    assert_eq!(e.engine.fleets[&hauler].cargo.total(), 0.0);
    assert!(alerts(&e, told).iter().any(|(_, text)| text.contains("unloaded 600 deuterium into the relay")));

    for &hop in &chain[..depth] {
        e.travel(hop);
    }
    let before = e.fleet();
    let stocked = relays(&e)[0].store;
    e.travel(relay_at);
    let there = e.fleet();
    assert!(there.at_relay);
    assert_eq!(there.fuel, there.fuel_max, "arriving at the relay fills the tank");
    let drawn = (there.fuel_max - (before.fuel - there.jump_fuel)) * FUEL_DEUT_PER_UNIT;
    assert!((relays(&e)[0].store - (stocked - drawn)).abs() < 1e-6, "the store paid for it at the dock's rate");

    for &hop in &chain[depth + 1..] {
        e.travel(hop);
    }
    assert_eq!(e.engine.fleets[&fid].location, chain[target]);
    for &hop in chain[depth..target].iter().rev() {
        e.travel(hop);
    }
    assert_eq!(e.fleet().fuel, e.fleet().fuel_max, "and again on the way back");
    for &hop in chain[..depth].iter().rev() {
        e.travel(hop);
    }
    e.travel(home);
    assert_eq!(e.engine.fleets[&fid].location, home, "the fleet that could not come back from the end of the lane did");
}

#[test]
fn every_rule_that_refuses_a_founding_says_why_and_the_survey_says_it_first() {
    let mut e = Empire::join(world("season"), "Rules");
    let fid = e.fid;
    let found_cargo = res(5000.0, 2500.0, 1000.0);
    let ground = |engine: &GameEngine, ring: u16| sector_in_ring(engine, ring, |_, t| open_ground(t));
    let neighbour = |engine: &GameEngine, at: Hex| {
        *engine
            .world_gen
            .connected_neighbors(at)
            .slice()
            .iter()
            .find(|n| n.dist_from_origin() >= 9 && open_ground(&engine.world_gen.generate_sector(**n)))
            .expect("open ground beside")
    };
    let refused = |e: &mut Empire, expect: ErrorCode, fragment: &str| {
        let survey = ask(e, None).refusal.expect("the survey refuses");
        let before = e.engine.snapshot().hash();
        let (code, text) = e.refusal(Command::FoundRelay { fleet_id: e.fid });
        assert_eq!(code, expect, "{text}");
        assert!(text.contains(fragment), "{text} should say {fragment}");
        assert_eq!(text, survey, "the survey and the order word the refusal alike");
        assert_eq!(e.engine.snapshot().hash(), before, "a refused founding changes nothing");
    };

    let shallow = ground(&e.engine, 8);
    muster(&mut e.engine, fid, &[(ShipClass::LargeCargo, 2)], shallow);
    stow(&mut e, fid, found_cargo);
    refused(&mut e, ErrorCode::InvalidTarget, "ring 9 and deeper");

    let den = find_deep(&e.engine, |t| t.npc_template.is_some() && !t.has_derelict && t.terrain != TerrainType::Anomaly);
    muster(&mut e.engine, fid, &[(ShipClass::LargeCargo, 2)], den);
    sweep(&mut e.engine, &[den]);
    stow(&mut e, fid, found_cargo);
    refused(&mut e, ErrorCode::InvalidTarget, "pirate lair");

    let derelict = find_deep(&e.engine, |t| t.has_derelict && t.npc_template.is_none());
    muster(&mut e.engine, fid, &[(ShipClass::LargeCargo, 2)], derelict);
    refused(&mut e, ErrorCode::InvalidTarget, "derelict");

    let first = ground(&e.engine, 12);
    sweep(&mut e.engine, &[first]);
    muster(&mut e.engine, fid, &[(ShipClass::LargeCargo, 2)], first);
    stow(&mut e, fid, res(10.0, 0.0, 0.0));
    refused(&mut e, ErrorCode::NoResources, "lacks");

    stow(&mut e, fid, found_cargo);
    let (limit, level) = (e.state().relays.limit, e.state().relays.level);
    assert_eq!((limit, level), (RELAY_BASE_MAX, 0));
    e.order(Command::FoundRelay { fleet_id: fid }).expect("the first relay");
    assert_eq!(e.engine.fleets[&fid].cargo, found_cargo.sub(scaling::relay_cost(12)));
    refused(&mut e, ErrorCode::InvalidTarget, "already stands");

    let second = neighbour(&e.engine, first);
    muster(&mut e.engine, fid, &[(ShipClass::LargeCargo, 2)], second);
    sweep(&mut e.engine, &[second]);
    refused(&mut e, ErrorCode::InvalidCommand, "Hyperspace Technology level 0 allows (1)");

    let pid = e.pid;
    edit(&mut e.engine, |s| {
        s.players.get_mut(&pid).unwrap().research.hyperspace_technology = RELAY_LEVELS_PER_STEP as u8;
    });
    assert_eq!(e.state().relays.limit, RELAY_BASE_MAX + 1, "a step of research raises the limit");
    e.order(Command::FoundRelay { fleet_id: fid }).expect("the second relay");
    assert_eq!(relays(&e).len(), 2);

    let third = neighbour(&e.engine, second);
    muster(&mut e.engine, fid, &[(ShipClass::LargeCargo, 2)], third);
    sweep(&mut e.engine, &[third]);
    refused(&mut e, ErrorCode::InvalidCommand, "allows (2)");

    muster(&mut e.engine, fid, &[(ShipClass::LargeCargo, 2)], second);
    let (code, text) = e.refusal(Command::FoundOutpost { fleet_id: fid });
    assert_eq!(code, ErrorCode::InvalidTarget);
    assert!(text.contains("relay stands at"), "{text}");
}

#[test]
fn a_relay_holds_deuterium_only_and_takes_what_the_store_has_room_for() {
    let mut e = Empire::join(world("season"), "Depot");
    let fid = e.fid;
    let at = sector_in_ring(&e.engine, 10, |_, t| open_ground(t));
    muster(&mut e.engine, fid, &[(ShipClass::LargeCargo, 2)], at);
    stow(&mut e, fid, scaling::relay_cost(10).add(res(300.0, 0.0, 4000.0)));
    e.order(Command::FoundRelay { fleet_id: fid }).expect("founded");

    let told = e.mark();
    e.order(Command::Unload { fleet_id: fid }).expect("unload");
    assert_eq!(relays(&e)[0].store, RELAY_STORE, "the store takes its capacity and no more");
    let cargo = e.engine.fleets[&fid].cargo;
    assert_eq!(cargo, res(300.0, 0.0, 4000.0 - RELAY_STORE), "the rest, metal among it, stays aboard");
    assert!(alerts(&e, told).iter().any(|(_, text)| text.contains("stays aboard, the store is full")));
    let (code, text) = e.refusal(Command::Unload { fleet_id: fid });
    assert_eq!(code, ErrorCode::CargoFull, "{text}");

    stow(&mut e, fid, res(300.0, 0.0, 0.0));
    let (code, text) = e.refusal(Command::Load { fleet_id: fid, resource: Metal, amount: None });
    assert_eq!(code, ErrorCode::InvalidCommand);
    assert!(text.contains("deuterium only"), "{text}");
    let (code, text) = e.refusal(Command::Load { fleet_id: fid, resource: Deuterium, amount: Some(RELAY_STORE + 5.0) });
    assert_eq!(code, ErrorCode::NoResources, "{text}");
    e.order(Command::Load { fleet_id: fid, resource: Deuterium, amount: Some(250.0) }).expect("load");
    assert_eq!((relays(&e)[0].store, e.engine.fleets[&fid].cargo.deuterium), (RELAY_STORE - 250.0, 250.0));
    e.order(Command::Unload { fleet_id: fid }).expect("what was loaded goes back");
    assert_eq!((relays(&e)[0].store, e.engine.fleets[&fid].cargo), (RELAY_STORE, res(300.0, 0.0, 0.0)));
    stow(&mut e, fid, res(0.0, 0.0, 0.0));
    let (code, text) = e.refusal(Command::Unload { fleet_id: fid });
    assert_eq!(code, ErrorCode::NoResources, "{text}");
}

#[test]
fn a_relay_fills_a_tank_to_the_jumps_research_allows_and_no_further() {
    let mut e = Empire::join(world("season"), "Pump");
    let fid = e.fid;
    let at = sector_in_ring(&e.engine, 10, |_, t| open_ground(t));
    muster(&mut e.engine, fid, &[(ShipClass::LightFighter, 10)], at);
    stow(&mut e, fid, scaling::relay_cost(10).add(res(0.0, 0.0, RELAY_STORE)));
    e.order(Command::FoundRelay { fleet_id: fid }).expect("founded");
    e.order(Command::Unload { fleet_id: fid }).expect("stocked");
    assert_eq!(relays(&e)[0].store, RELAY_STORE);
    let jump = e.engine.hop_fuel_cost(&e.engine.fleets[&fid]);
    let pid = e.pid;
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.fuel_max = 40.0 * jump;
        fleet.fuel = 0.0;
    });

    e.run(2);
    let jump = e.engine.hop_fuel_cost(&e.engine.fleets[&fid]);
    let base = f64::from(scaling::depot_fill_jumps(&scaling::ResearchLevels::default()));
    assert_eq!(e.state().relays.fill_jumps, base as u32);
    assert!((e.engine.fleets[&fid].fuel - base * jump).abs() < 1e-6, "{} of {} jumps", e.engine.fleets[&fid].fuel / jump, base);
    let drawn = RELAY_STORE - relays(&e)[0].store;
    assert!((drawn - base * jump * FUEL_DEUT_PER_UNIT).abs() < 0.1, "the store paid for what was filled: {drawn}");

    edit(&mut e.engine, |s| s.players.get_mut(&pid).unwrap().research.hyperspace_technology = 4);
    let more = f64::from(scaling::depot_fill_jumps(&e.engine.players[&pid].research));
    assert!(more > base);
    e.run(2);
    assert_eq!(e.state().relays.fill_jumps, more as u32);
    assert!((e.engine.fleets[&fid].fuel - more * jump).abs() < 1e-6, "research raises what a relay fills to");
}

#[test]
fn a_relay_nobody_visits_is_gone_with_its_store_and_a_visit_puts_that_off() {
    let mut e = Empire::join(world("test"), "Hermit");
    let fid = e.fid;
    let span = e.engine.relay_decay_ticks();
    let at = sector_in_ring(&e.engine, 10, |at, t| {
        open_ground(t)
            && e.engine.world_gen.connected_neighbors(at).slice().iter().any(|n| {
                open_ground(&e.engine.world_gen.generate_sector(*n))
            })
    });
    let aside = *e
        .engine
        .world_gen
        .connected_neighbors(at)
        .slice()
        .iter()
        .find(|n| open_ground(&e.engine.world_gen.generate_sector(**n)))
        .unwrap();
    muster(&mut e.engine, fid, &[(ShipClass::LargeCargo, 2)], at);
    sweep(&mut e.engine, &[aside]);
    stow(&mut e, fid, scaling::relay_cost(10).add(res(0.0, 0.0, 500.0)));
    e.order(Command::FoundRelay { fleet_id: fid }).expect("founded");
    e.order(Command::Unload { fleet_id: fid }).expect("stocked");
    assert_eq!(relays(&e)[0].decay_in, span, "a fleet in the sector holds it off");

    e.order(Command::Move { fleet_id: fid, target: aside }).expect("away");
    e.run_until(100, |e| e.engine.fleets[&fid].location == aside && e.engine.fleets[&fid].state != FleetStatus::Moving);
    e.run(span / 2);
    let left = relays(&e)[0].decay_in;
    assert!(left <= span / 2 + 2 && left > span / 2 - 10, "half the span has run: {left} of {span}");

    e.order(Command::Move { fleet_id: fid, target: at }).expect("back");
    e.run_until(100, |e| e.engine.fleets[&fid].location == at && e.engine.fleets[&fid].state != FleetStatus::Moving);
    e.run(2);
    assert_eq!(relays(&e)[0].decay_in, span, "the visit set it back");
    e.order(Command::Move { fleet_id: fid, target: aside }).expect("away again");
    e.run_until(100, |e| e.engine.fleets[&fid].location == aside && e.engine.fleets[&fid].state != FleetStatus::Moving);
    e.run(span - 12);
    assert_eq!(relays(&e).len(), 1, "it stands until the span is out");
    let kept = relays(&e)[0].store;
    let told = e.mark();
    e.run(20);
    assert!(relays(&e).is_empty(), "and then it is gone");
    let gone = e.since(told, |kind| match kind {
        EventKind::RelayDecayed(event) => Some((event.sector, event.lost)),
        _ => None,
    });
    assert_eq!(gone, vec![(at, kept)], "with the store it held");
    assert!(
        alerts(&e, told).iter().any(|(level, text)| matches!(level, AlertLevel::Warning) && text.contains("is gone")),
        "and the owner was told",
    );
    assert!(e.engine.relays.is_empty(), "an empty list is not kept");
}

fn story(ticks: u64, stops: &[u64]) -> u64 {
    let mut e = Empire::join(world("test"), "Teller");
    let fid = e.fid;
    let span = e.engine.relay_decay_ticks();
    let at = sector_in_ring(&e.engine, 10, |at, t| {
        open_ground(t)
            && e.engine.world_gen.connected_neighbors(at).slice().iter().any(|n| open_ground(&e.engine.world_gen.generate_sector(*n)))
    });
    let aside = *e
        .engine
        .world_gen
        .connected_neighbors(at)
        .slice()
        .iter()
        .find(|n| open_ground(&e.engine.world_gen.generate_sector(**n)))
        .unwrap();
    muster(&mut e.engine, fid, &[(ShipClass::LightFighter, 4), (ShipClass::LargeCargo, 1)], at);
    sweep(&mut e.engine, &[aside]);
    stow(&mut e, fid, scaling::relay_cost(10).add(res(0.0, 0.0, 700.0)));
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.fuel_max = 4000.0;
        fleet.fuel = 700.0;
    });
    e.order(Command::FoundRelay { fleet_id: fid }).expect("founded");
    e.order(Command::Unload { fleet_id: fid }).expect("stocked");
    let tour = [(5, aside), (span / 3, at), (span / 3 + 40, aside)];
    for _ in 0..ticks {
        let tick = e.engine.current_tick();
        if stops.contains(&tick) {
            e.engine = GameEngine::restore(stored(&e.engine));
        }
        for &(when, to) in &tour {
            if tick == when {
                let _ = e.order(Command::Move { fleet_id: fid, target: to });
            }
        }
        e.run(1);
    }
    e.engine.snapshot().hash()
}

#[test]
fn a_world_with_a_relay_replays_and_resumes_from_its_document_to_the_same_state() {
    let span = Empire::join(world("test"), "x").engine.relay_decay_ticks();
    let ticks = span * 2 + 100;
    let straight = story(ticks, &[]);
    assert_eq!(story(ticks, &[]), straight, "the same seed and the same orders give the same world");
    let stops = [3, 6, span / 3 + 1, span / 3 + 41, span / 3 + 41 + span - 1, span / 3 + 41 + span + 1];
    assert_eq!(story(ticks, &stops), straight, "stopped and read back at ticks {stops:?}, the world went on as if it had not been");
}
