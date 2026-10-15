// Roaming hubs: neutral posts in the Wandering that exchange and refuel and
// move. Where each stands is the seed's to say and moves when its stay ends,
// a world stopped and started again is the same world, a fleet far past its
// tank refuels at one from its hold and flies on, the warning comes before
// the departure and a fleet left standing is unharmed, every refusal says
// why, a hub never lands where something stands, and nothing hostile
// enters one.

#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::constants::{
    ResourceKind, Resources, ShipClass, TerrainType, EXCHANGE_FEE_MIN, FUEL_DEUT_PER_UNIT, HUB_COUNT, HUB_FIRST_RING,
};
use iac_shared::hex::Hex;
use iac_shared::protocol::{Command, ErrorCode, EventKind, FleetState, HubState};
use iac_shared::scaling::{self, ship_tank, ResearchLevels};
use iac_shared::world::{self, HubService, NpcBehaviorType};
use iac_sim::engine::{FleetStatus, Relay, Ships};
use iac_sim::GameEngine;

use ResourceKind::{Crystal, Deuterium, Metal};

fn fleet_of(e: &Empire) -> FleetState {
    e.fleet()
}

fn hubs_known(e: &Empire) -> Vec<HubState> {
    e.state().hubs
}

fn quiet_neighbour(engine: &mut GameEngine, at: Hex) -> Hex {
    let next = *engine
        .world_gen
        .connected_neighbors(at)
        .slice()
        .iter()
        .find(|n| {
            let t = engine.world_gen.generate_sector(**n);
            !t.has_derelict && t.terrain != TerrainType::Anomaly
        })
        .expect("a sector beside the hub");
    sweep(engine, &[next]);
    next
}

/// An empire whose one fleet, ten Light Fighters with an empty tank and the
/// given hold, stands in the sector hub 0 stands in now.
fn tanker(pace: &str, cargo: Resources) -> (Empire, Hex) {
    let mut e = Empire::join(world(pace), "Tanker");
    let hub = e.engine.hubs[0].sector;
    let fid = e.fid;
    let ships = Ships::of(&[(ShipClass::LightFighter, 10)]);
    let tank: f64 = 10.0 * ship_tank(ShipClass::LightFighter, &ResearchLevels::default());
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.ships = ships;
        fleet.location = hub;
        fleet.state = FleetStatus::Idle;
        fleet.move_target = None;
        fleet.cargo = cargo;
        fleet.fuel_max = tank;
        fleet.fuel = 0.0;
    });
    e.run(1);
    (e, hub)
}

fn untouched_by(e: &mut Empire, cmd: Command) -> (ErrorCode, String) {
    let before = e.engine.snapshot().hash();
    let refused = e.refusal(cmd);
    assert_eq!(e.engine.snapshot().hash(), before, "a refused order changes nothing");
    refused
}

#[test]
fn a_hub_stands_where_the_seed_says_and_moves_when_its_stay_ends() {
    let mut engine = world("blitz");
    let pace = engine.pace();
    let map = engine.world_gen.clone();
    let period = scaling::hub_period_ticks(&pace);
    assert_eq!(engine.hubs.len(), usize::from(HUB_COUNT));

    let mut moves = 0;
    let mut stays = vec![Vec::new(); usize::from(HUB_COUNT)];
    for _ in 0..3 * period {
        let tick = engine.current_tick();
        for hub in &engine.hubs {
            assert_eq!(hub.epoch, scaling::hub_epoch(hub.id, tick, &pace), "tick {tick}");
            assert_eq!(hub.sector, map.hub_site(hub.id, hub.epoch), "hub {} at tick {tick}", hub.id);
            let (first, last) = world::hub_band(hub.id);
            assert!((first..=last).contains(&hub.sector.dist_from_origin()));
            assert!(first >= HUB_FIRST_RING);
            let template = map.generate_sector(hub.sector);
            assert!(template.npc_template.is_none() && !template.has_derelict);
            assert_ne!(template.terrain, TerrainType::Anomaly);
            assert!(!world::is_hub(hub.sector) && !world::is_home_ground(hub.sector));
            assert_eq!(engine.hub_services_at(hub.sector), [HubService::Exchange, HubService::Refuel]);
            let stay = &mut stays[usize::from(hub.id)];
            if stay.last() != Some(&(hub.epoch, hub.sector)) {
                stay.push((hub.epoch, hub.sector));
            }
        }
        engine.tick().expect("tick");
        for hub in &engine.hubs {
            let before = scaling::hub_epoch(hub.id, tick, &pace);
            if hub.epoch != before {
                assert_eq!(tick + 1, scaling::hub_departs_at(hub.id, before, &pace), "hub {} leaves on its tick", hub.id);
                moves += 1;
            }
        }
    }
    assert!(moves >= 3 * usize::from(HUB_COUNT) - usize::from(HUB_COUNT), "every hub moved about three times: {moves}");
    for (id, list) in stays.iter().enumerate() {
        let sectors: std::collections::BTreeSet<u32> = list.iter().map(|(_, s)| s.to_key()).collect();
        assert!(sectors.len() > 1, "hub {id} went somewhere else: {list:?}");
    }
    let departures: std::collections::BTreeSet<u64> = (0..HUB_COUNT)
        .map(|id| scaling::hub_departs_at(id, scaling::hub_epoch(id, 0, &pace), &pace))
        .collect();
    assert_eq!(departures.len(), usize::from(HUB_COUNT), "the hubs do not all move at once");
}

#[test]
fn a_world_stopped_and_started_through_its_document_is_the_world_played_straight() {
    fn build() -> (GameEngine, u64, u64) {
        let mut engine = world("blitz");
        let (pid, fid) = join(&mut engine, "Watcher");
        let (other, _) = join(&mut engine, "Blind");
        assert_ne!(pid, other);
        let hub = engine.hubs[2].sector;
        muster(&mut engine, fid, &[(ShipClass::LightFighter, 5)], hub);
        (engine, pid, fid)
    }
    fn play(restart_every: Option<u64>) -> (u64, Vec<String>, Vec<HubState>) {
        let (mut engine, pid, _) = build();
        let mut log = Vec::new();
        let ticks = 2 * scaling::hub_period_ticks(&engine.pace()) + 60;
        for tick in 1..=ticks {
            engine.tick().expect("tick");
            log.extend(engine.drain_events().iter().map(|e| serde_json::to_string(e).unwrap()));
            if restart_every.is_some_and(|every| tick % every == 0) {
                engine = GameEngine::restore(stored(&engine));
            }
        }
        let seen = iac_sim::views::full_state(&engine, pid).unwrap().hubs;
        (engine.snapshot().hash(), log, seen)
    }

    let straight = play(None);
    assert!(!straight.2.is_empty(), "the watcher knows a hub");
    assert!(straight.1.iter().any(|line| line.contains("HubDeparting")), "a warning was sent");
    for every in [97, 60, 211] {
        let restarted = play(Some(every));
        assert_eq!(restarted.0, straight.0, "restarted every {every} ticks");
        assert_eq!(restarted.1, straight.1);
        assert_eq!(restarted.2, straight.2);
    }
}

#[test]
fn a_fleet_far_past_its_tank_refuels_from_its_hold_at_a_hub_and_flies_on() {
    let (mut e, hub) = tanker("season", Resources { metal: 20_000.0, crystal: 0.0, deuterium: 100.0 });
    let fid = e.fid;
    e.set_stockpile(Resources { metal: 777.0, crystal: 888.0, deuterium: 999.0 });
    let onward = quiet_neighbour(&mut e.engine, hub);

    let mut stranded = Empire { engine: GameEngine::restore(e.engine.snapshot()), pid: e.pid, fid, log: Vec::new() };
    let away = iac_shared::hex::hex_ring(Hex::ORIGIN, 30).next().unwrap();
    edit(&mut stranded.engine, |s| s.hubs[0].sector = away);
    let (code, _) = stranded.refusal(Command::Move { fleet_id: fid, target: onward });
    assert_eq!(code, ErrorCode::InsufficientFuel, "the same fleet without the hub is stranded");
    let (code, why) = stranded.refusal(Command::Refuel { fleet_id: fid, give: Deuterium, amount: None });
    assert_eq!(code, ErrorCode::NotInSector);
    assert!(why.contains("no hub that refuels is known"), "{why}");

    let (code, _) = e.refusal(Command::Move { fleet_id: fid, target: onward });
    assert_eq!(code, ErrorCode::InsufficientFuel);
    let there = fleet_of(&e);
    let quote = there.refuel.clone().expect("a fleet at a hub that refuels is quoted");
    assert_eq!(quote.refusal, None);
    assert_eq!(quote.missing, there.fuel_max - there.fuel);
    let kinds: Vec<ResourceKind> = quote.offers.iter().map(|o| o.give).collect();
    assert_eq!(kinds, [Metal, Deuterium]);
    let metal_offer = &quote.offers[0];
    assert!(metal_offer.fee >= EXCHANGE_FEE_MIN && metal_offer.fuel > 0.0);
    assert_eq!(quote.offers[1].fee, 0.0);

    let mark = e.mark();
    let fuel_before = there.fuel;
    e.order(Command::Refuel { fleet_id: fid, give: Deuterium, amount: Some(40.0) }).expect("deuterium buys fuel at the dock's rate");
    let now = e.engine.fleets[&fid].clone();
    assert!((now.fuel - (fuel_before + 40.0 / FUEL_DEUT_PER_UNIT)).abs() < 1e-6);
    assert_eq!(now.cargo.deuterium, 60.0);
    assert_eq!(e.stockpile(), Resources { metal: 777.0, crystal: 888.0, deuterium: 999.0 }, "never the stockpile");
    assert!(e.since(mark, |k| matches!(k, EventKind::Exchanged(_)).then_some(())).is_empty(), "deuterium is not exchanged");

    let pressure = e.engine.exchange_meter(&e.engine.players[&e.pid]).pressure;
    let mark = e.mark();
    e.order(Command::Refuel { fleet_id: fid, give: Metal, amount: None }).expect("metal buys the rest through the exchange");
    let full = e.engine.fleets[&fid].clone();
    assert_eq!(full.fuel, full.fuel_max, "the tank is full");
    let spent = 20_000.0 - full.cargo.metal;
    assert!(spent > 0.0 && full.cargo.metal > 0.0, "only what fills the tank leaves the hold: {spent}");
    let deals = e.since(mark, |k| match k {
        EventKind::Exchanged(x) => Some(x.terms.clone()),
        _ => None,
    });
    assert_eq!(deals.len(), 1);
    let terms = &deals[0];
    assert_eq!((terms.give, terms.want), (Metal, Deuterium));
    assert!((terms.amount - spent).abs() < 1e-6);
    assert!(terms.amount * Metal.weight() >= terms.receive * Deuterium.weight(), "the exchange returns no more than it takes");
    assert!(terms.fee >= EXCHANGE_FEE_MIN);
    assert!(e.engine.exchange_meter(&e.engine.players[&e.pid]).pressure > pressure, "the player's meter moved");
    assert_eq!(full.cargo.deuterium, 60.0, "the metal was exchanged and burnt, the hold's deuterium untouched");

    let (code, why) = e.refusal(Command::Refuel { fleet_id: fid, give: Deuterium, amount: None });
    assert_eq!(code, ErrorCode::InvalidCommand);
    assert!(why.contains("is full"), "{why}");

    e.order(Command::Move { fleet_id: fid, target: onward }).expect("and it flies on");
    e.run_until(200, |e| e.engine.fleets[&fid].location == onward && e.engine.fleets[&fid].state != FleetStatus::Moving);
    assert!(e.engine.fleets[&fid].fuel > 0.0);
}

#[test]
fn the_warning_comes_before_the_departure_and_a_fleet_left_standing_is_unharmed() {
    let mut e = Empire::join(world("blitz"), "Sitter");
    let fid = e.fid;
    let pace = e.engine.pace();
    let hub = e.engine.hubs[1];
    muster(&mut e.engine, fid, &[(ShipClass::LightFighter, 5)], hub.sector);
    let ships_before = e.engine.fleets[&fid].ships.total();
    let departs = scaling::hub_departs_at(hub.id, hub.epoch, &pace);
    let warn = scaling::hub_warn_ticks(&pace);
    assert!(warn > 0 && departs > warn + 2);

    assert!(hubs_known(&e).is_empty(), "nobody has seen it yet");
    e.run(1);
    let known = hubs_known(&e);
    assert_eq!(known.len(), 1);
    assert_eq!((known[0].id, known[0].sector, known[0].departs_at), (hub.id, hub.sector, departs));
    assert_eq!(known[0].services, [HubService::Exchange, HubService::Refuel]);
    assert!(!known[0].departing && !known[0].stale);
    let arrivals = e.since(0, |k| match k {
        EventKind::HubArrived(a) => Some((a.id, a.sector, a.departs_at)),
        _ => None,
    });
    assert_eq!(arrivals, [(hub.id, hub.sector, departs)], "told once, when it was first seen");

    e.run_until(2 * departs, |e| e.engine.current_tick() + warn == departs);
    let warned = e.since(0, |k| match k {
        EventKind::HubDeparting(d) => Some((d.id, d.sector, d.departs_at)),
        _ => None,
    });
    assert_eq!(warned, [(hub.id, hub.sector, departs)]);
    assert_eq!(departs - e.engine.current_tick(), warn, "the warning came {warn} ticks ahead");
    assert!(hubs_known(&e)[0].departing);
    assert_eq!(e.engine.hubs[1].sector, hub.sector, "it has not gone yet");

    e.run_until(2 * warn, |e| e.engine.hubs[1].epoch != hub.epoch);
    e.run(1);
    assert_eq!(e.engine.current_tick(), departs + 1);
    assert_ne!(e.engine.hubs[1].sector, hub.sector);
    assert!(e.engine.hub_services_at(hub.sector).is_empty(), "the sector is ordinary again");
    let left = &e.engine.fleets[&fid];
    assert_eq!((left.location, left.ships.total()), (hub.sector, ships_before), "the fleet is where it was and whole");
    let known = hubs_known(&e);
    assert_eq!((known.len(), known[0].sector, known[0].stale), (1, hub.sector, true), "remembered, and marked as left");
    assert!(!known[0].departing);
    assert!(fleet_of(&e).refuel.is_none(), "an ordinary sector refuels nobody");
    let (code, _) = e.refusal(Command::Refuel { fleet_id: fid, give: Deuterium, amount: None });
    assert_eq!(code, ErrorCode::NotInSector);
    let (code, _) = e.refusal(Command::Exchange { fleet_id: fid, give: Metal, want: Crystal, amount: Some(1.0) });
    assert_eq!(code, ErrorCode::NotInSector);
}

#[test]
fn a_scan_that_covers_a_hub_tells_of_it_and_a_blind_empire_is_told_nothing() {
    let mut e = Empire::join(world("season"), "Scanner");
    let hub = e.engine.hubs[0].sector;
    let near = quiet_neighbour(&mut e.engine, hub);
    let fid = e.fid;
    muster(&mut e.engine, fid, &[(ShipClass::EspionageProbe, 1)], near);
    assert!(hubs_known(&e).is_empty());
    e.order(Command::Scan { fleet_id: fid }).expect("a scan");
    e.run(2);
    let known = hubs_known(&e);
    assert_eq!(known.len(), 1);
    assert_eq!(known[0].sector, hub);

    let mut blind = Empire::join(world("season"), "Blind");
    blind.run(5);
    assert!(blind.state().hubs.is_empty());
    assert!(blind.since(0, |k| matches!(k, EventKind::HubArrived(_)).then_some(())).is_empty());
}

#[test]
fn every_refusal_of_a_refuelling_says_why_and_changes_nothing() {
    let (mut e, hub) = tanker("season", Resources { metal: 500.0, crystal: 0.0, deuterium: 0.0 });
    let fid = e.fid;
    e.set_stockpile(Resources { metal: 9_000.0, crystal: 9_000.0, deuterium: 9_000.0 });
    let order = |give, amount| Command::Refuel { fleet_id: fid, give, amount };

    let (code, why) = untouched_by(&mut e, order(Deuterium, None));
    assert_eq!(code, ErrorCode::NoResources);
    assert!(why.contains("holds no deuterium") && why.contains("never from the stockpile"), "{why}");
    let quote = fleet_of(&e).refuel.expect("quoted");
    assert_eq!(quote.offers.iter().map(|o| o.give).collect::<Vec<_>>(), [Metal]);

    let (code, why) = untouched_by(&mut e, order(Metal, Some(600.0)));
    assert_eq!(code, ErrorCode::NoResources);
    assert!(why.contains("holds 500.0 metal"), "{why}");

    for bad in [0.0, -3.0, f64::NAN] {
        let (code, _) = untouched_by(&mut e, order(Metal, Some(bad)));
        assert_eq!(code, ErrorCode::InvalidCommand);
    }
    let (code, why) = untouched_by(&mut e, order(Metal, Some(1.0)));
    assert_eq!(code, ErrorCode::InvalidCommand);
    assert!(why.contains("too little"), "{why}");

    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().state = FleetStatus::InCombat);
    let (code, why) = untouched_by(&mut e, order(Metal, None));
    assert_eq!(code, ErrorCode::OnCooldown);
    assert!(why.contains("in combat"), "{why}");
    assert!(fleet_of(&e).refuel.expect("quoted").refusal.is_some_and(|r| r.contains("in combat")));

    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().state = FleetStatus::Moving);
    let (code, why) = untouched_by(&mut e, order(Metal, None));
    assert_eq!(code, ErrorCode::OnCooldown);
    assert!(why.contains("in transit"), "{why}");

    let beside = quiet_neighbour(&mut e.engine, hub);
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.state = FleetStatus::Idle;
        fleet.location = beside;
    });
    let (code, why) = untouched_by(&mut e, order(Metal, None));
    assert_eq!(code, ErrorCode::NotInSector);
    assert!(why.contains(&format!("1 hex from the hub at {hub} that refuels")), "{why}");
    assert!(fleet_of(&e).refuel.is_none());
}

#[test]
fn a_hub_never_lands_on_a_relay_or_a_group_and_the_choice_is_kept() {
    let mut engine = world("blitz");
    let (pid, _) = join(&mut engine, "Founder");
    let pace = engine.pace();
    let map = engine.world_gen.clone();
    let next = |id: u8| scaling::hub_epoch(id, 0, &pace) + 1;
    let first = |id: u8| map.hub_site(id, next(id));
    let tick = engine.current_tick();

    let blocked_by_relay = first(0);
    let blocked_by_group = first(1);
    assert_ne!(blocked_by_relay, blocked_by_group);
    edit(&mut engine, |s| {
        s.relays.entry(pid).or_default().push(Relay {
            sector: blocked_by_relay,
            store: 0.0,
            founded_tick: tick,
            visited_tick: tick,
            dry_tanks: Vec::new(),
        });
    });
    horde(&mut engine, 900_001, blocked_by_group, &[(ShipClass::LightFighter, 3)], 1.0);

    let end = scaling::hub_departs_at(1, scaling::hub_epoch(1, 0, &pace), &pace);
    let end0 = scaling::hub_departs_at(0, scaling::hub_epoch(0, 0, &pace), &pace);
    assert!(end < end0, "hub 1 leaves first");
    run(&mut engine, end0 + 1);
    let hubs = engine.hubs.clone();
    let chosen = |id: u8, blocked: Hex| map.hub_sites(id, next(id)).find(|s| *s != blocked).unwrap();
    assert_ne!(hubs[0].sector, blocked_by_relay);
    assert_eq!(hubs[0].sector, chosen(0, blocked_by_relay), "the next the seed offers");
    assert_ne!(hubs[1].sector, blocked_by_group);
    assert_eq!(hubs[1].sector, chosen(1, blocked_by_group));
    assert!(engine.relays.contains_key(&pid) && engine.npc_fleets.contains_key(&900_001));

    let restarted = GameEngine::restore(stored(&engine));
    assert_eq!(restarted.hubs, hubs, "the choice is part of the stored world");
}

#[test]
fn nothing_hostile_enters_a_hub_and_no_station_is_founded_in_one() {
    let mut e = Empire::join(world("season"), "Visitor");
    let (pid, fid) = (e.pid, e.fid);
    let hub = e.engine.hubs[0].sector;
    muster(&mut e.engine, fid, &[(ShipClass::LightFighter, 40)], hub);
    e.run(2);
    assert!(e.engine.npc_fleets.values().all(|npc| npc.location != hub));
    assert_eq!(
        e.order(Command::Attack { fleet_id: fid, target_fleet_id: iac_sim::engine::template_npc_id(hub) }),
        Err(ErrorCode::InvalidTarget),
        "nothing to fight at a hub",
    );
    assert_eq!(e.order(Command::ExploreSite { fleet_id: fid }), Err(ErrorCode::InvalidTarget));

    let post = quiet_neighbour(&mut e.engine, hub);
    let onward = *e
        .engine
        .world_gen
        .connected_neighbors(post)
        .slice()
        .iter()
        .find(|n| **n != hub)
        .expect("another lane out of the post");
    let pack = 900_000;
    horde(&mut e.engine, pack, post, &[(ShipClass::LightFighter, 60)], 1.0);
    edit(&mut e.engine, |s| s.npc_fleets.get_mut(&pack).unwrap().behavior = NpcBehaviorType::Aggressive);
    e.order(Command::Move { fleet_id: fid, target: post }).expect("a jump out of the hub");
    e.run_until(40, |e| e.engine.fleets[&fid].state == FleetStatus::InCombat);

    let preview = |e: &mut Empire, target: Hex| match e.engine.execute(pid, Command::PreviewMove { fleet_id: fid, target }).reply {
        Some(iac_shared::protocol::ServerMessage::PreviewMove(p)) => p.flight.expect("a jump out of a fight is flight"),
        other => panic!("no preview: {other:?}"),
    };
    assert!(preview(&mut e, onward).chase.is_some(), "sixty fighters follow forty that leave them");
    assert_eq!(preview(&mut e, hub).chase, None, "and would not follow them into a hub");

    e.order(Command::Move { fleet_id: fid, target: hub }).expect("a fleet may leave a fight for a hub");
    e.run_until(40, |e| e.engine.fleets[&fid].location == hub);
    for _ in 0..20 {
        e.run(1);
        assert_eq!(e.engine.npc_fleets[&pack].location, post, "the pack stays out");
        assert!(e.engine.active_combats.is_empty());
    }
    assert!(e.fleet().pursued_by.is_none());

    let cost = scaling::relay_cost(hub.dist_from_origin());
    let fid = e.fid;
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.cargo = cost;
        fleet.state = FleetStatus::Idle;
    });
    let (code, why) = e.refusal(Command::FoundRelay { fleet_id: fid });
    assert_eq!(code, ErrorCode::InvalidTarget);
    assert!(why.contains("a hub stands at"), "{why}");
}
