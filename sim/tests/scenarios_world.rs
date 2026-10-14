// Scenarios that run the engine over time against the world around a
// homeworld: wreckage and what the chart remembers of it, doctrines that
// roam charted space, ore tiles that empty and refill, fights that end some
// other way than one side dying, and where a world seats its empires and what
// it keeps away from them.

#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::constants::{
    Density, ResourceKind, Resources, ShipClass, Zone, DEFAULT_WORLD_SEED, HOME_SALVAGE_WARN_TICKS,
    SALVAGE_DESPAWN_TICKS, SCAN_REVEAL_TICKS,
};
use iac_shared::hex::{hex_spiral, Hex};
use iac_shared::pace::Pace;
use iac_shared::protocol::{
    AlertLevel, BattleOutcome, BattleRoundEvent, BattleSide, ChaseForecast, CombatEndedEvent, Command, ErrorCode,
    EventKind, Fleeing, HarvestResource, PolicyParams, PolicyPreset, Pursuit, PursuitEvent, PursuitPhase, StockpileCause,
    ThreatBasis, Volley,
};
use iac_shared::scaling::{self, RatioLabel, ShipyardItem};
use iac_shared::world::NpcBehaviorType;
use iac_shared::world::NpcBehaviorType::Passive;
use iac_shared::world::{home_seats, is_home_seat};
use iac_sim::commands::auth_failure;
use iac_sim::engine::{template_npc_id, AuthError, FleetStatus, SectorOverride, Ships};

fn salvage_events(events: &[iac_shared::protocol::GameEvent]) -> Vec<iac_shared::protocol::SalvageDespawnedEvent> {
    events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::SalvageDespawned(d) => Some(d.clone()),
            _ => None,
        })
        .collect()
}

fn drop_home_pile(engine: &mut iac_sim::GameEngine, pid: u64, pile: Resources) -> u64 {
    let despawn = engine.current_tick() + u64::from(SALVAGE_DESPAWN_TICKS);
    edit(engine, |s| {
        let home = s.players[&pid].homeworld;
        let ov = s.sector_overrides.entry(home.to_key()).or_insert_with(SectorOverride::default);
        ov.salvage = Some(pile);
        ov.salvage_despawn_tick = Some(despawn);
    });
    despawn
}

#[test]
fn an_uncollected_pile_despawns_in_front_of_the_fleet_that_made_it() {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, "Witness");
    let at = lone_fighter_sector(&engine, Passive);
    let (pile, despawn) = pile_of(&fight(&mut engine, pid, fid, at));
    assert!(engine.sector_overrides[&at.to_key()].salvage.is_some());

    let before = run_until(&mut engine, 400, |e| e.current_tick() + 1 >= despawn);
    assert!(salvage_events(&before).is_empty());
    let after = run(&mut engine, 1);
    let gone = salvage_events(&after);
    assert_eq!(gone.len(), 1);
    assert_eq!((gone[0].sector, gone[0].resources), (at, pile));
    assert!(engine.sector_overrides[&at.to_key()].salvage.is_none());
}

#[test]
fn every_report_of_a_kills_wreckage_says_the_same_figures_and_names_the_pile_it_joins() {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, "Tally");
    let at = lone_fighter_sector(&engine, Passive);
    let kills = |events: &[iac_shared::protocol::GameEvent]| {
        events.iter().fold(Resources::default(), |sum, e| match &e.kind {
            EventKind::FleetDestroyed(d) if d.is_npc => sum.add(d.salvage),
            _ => sum,
        })
    };
    let ended = |events: &[iac_shared::protocol::GameEvent]| {
        events.iter().find_map(|e| match &e.kind {
            EventKind::CombatEnded(c) => Some(c.clone()),
            _ => None,
        }).expect("the fight ended")
    };

    let first = fight(&mut engine, pid, fid, at);
    let alone = ended(&first);
    assert_eq!((alone.salvage, alone.salvage_pile), (Some(kills(&first)), None), "a kill on clear ground: one figure");
    let left = kills(&first);

    horde(&mut engine, 7_000_001, at, &[(ShipClass::LightFighter, 1)], 1.0);
    let outcome = engine.execute(pid, Command::Attack { fleet_id: fid, target_fleet_id: 7_000_001 });
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let second = run_until(&mut engine, 10, |e| e.active_combats.is_empty());
    let stacked = ended(&second);
    let dropped = kills(&second);
    assert!(dropped.total() > 0.0);
    assert_eq!(stacked.salvage, Some(dropped), "the fight's wreckage is the sum of its kills, not the pile");
    assert_eq!(stacked.salvage_pile, Some(left.add(dropped)), "and the pile it joins is named apart");
    assert_eq!(engine.sector_overrides[&at.to_key()].salvage, stacked.salvage_pile, "which is what lies there");

    let outcome = engine.execute(pid, Command::CollectSalvage { fleet_id: fid });
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let scooped = run(&mut engine, 2).iter().find_map(|e| match &e.kind {
        EventKind::SalvageCollected(c) => Some((c.resources, c.remaining.unwrap_or_default())),
        _ => None,
    });
    let (took, rest) = scooped.expect("the fleet scooped");
    let pile = stacked.salvage_pile.unwrap();
    for kind in ResourceKind::ALL {
        assert!((took.get(kind) + rest.get(kind) - pile.get(kind)).abs() < 1e-9, "{kind:?}: {took:?} + {rest:?} against {pile:?}");
    }
}

#[test]
fn the_chart_remembers_a_pile_as_seen_until_its_own_timer_runs_out() {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, "Chartist");
    let at = lone_fighter_sector(&engine, Passive);
    let home = engine.players[&pid].homeworld;
    let (pile, despawn) = pile_of(&fight(&mut engine, pid, fid, at));
    run(&mut engine, 1);
    let seen_at = engine.current_tick();
    let live = &chart(&engine, pid)[&at.to_key()];
    assert!(live.live && live.salvage == Some(pile) && live.salvage_despawn_tick == Some(despawn));
    assert!(!live.pins_stale);

    deploy(&mut engine, fid, home, None);
    run(&mut engine, 1);
    let stale = &chart(&engine, pid)[&at.to_key()];
    assert!(!stale.live && stale.salvage == Some(pile), "out of sight the pin is kept");
    assert!(stale.pins_stale, "but marked unconfirmed");
    assert_eq!(stale.salvage_despawn_tick, Some(despawn));
    assert_eq!(stale.last_seen, seen_at);

    let mut cleared_updates = 0;
    while engine.current_tick() < despawn {
        engine.tick().unwrap();
        cleared_updates += engine
            .known
            .tick_updates(&engine, pid)
            .iter()
            .filter(|s| s.location == at && s.salvage.is_none())
            .count();
    }
    let gone = &chart(&engine, pid)[&at.to_key()];
    assert!(gone.salvage.is_none() && gone.salvage_despawn_tick.is_none() && !gone.pins_stale);
    assert!(!gone.live);
    assert_eq!(cleared_updates, 1, "the client is told once");
}

#[test]
fn scanned_sectors_go_stale_once_and_are_never_forgotten() {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, "Cartographer");
    assert!(engine.execute(pid, Command::Scan { fleet_id: fid }).error.is_none());
    run(&mut engine, 1);
    let revealed = engine.scan_revealed_coords(pid);
    assert!(!revealed.is_empty());
    let known_before = chart(&engine, pid).len();
    let scanned_at = engine.current_tick();

    let mut stale_notices = std::collections::HashMap::<u32, u32>::new();
    for _ in 0..SCAN_REVEAL_TICKS + 5 {
        engine.tick().unwrap();
        for s in engine.known.tick_updates(&engine, pid).into_iter().filter(|s| !s.live) {
            *stale_notices.entry(s.location.to_key()).or_default() += 1;
        }
    }
    assert!(engine.scan_revealed_coords(pid).is_empty());
    assert!(!stale_notices.is_empty());
    assert!(stale_notices.values().all(|&n| n == 1), "stale deltas are sent once: {stale_notices:?}");

    let chart = chart(&engine, pid);
    assert_eq!(chart.len(), known_before, "no sector is forgotten");
    let here = engine.fleets[&fid].location;
    for c in revealed.iter().filter(|c| **c != here) {
        let s = &chart[&c.to_key()];
        assert!(!s.live && s.last_seen >= scanned_at && s.last_seen < engine.current_tick());
    }
    assert!(chart[&here.to_key()].live, "the fleet's own sector stays live");
}

#[test]
fn home_salvage_nobody_can_scoop_warns_once_near_the_end() {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, "Away");
    let home = engine.players[&pid].homeworld;
    deploy(&mut engine, fid, Hex { q: home.q + 3, r: home.r }, None);
    let despawn = drop_home_pile(&mut engine, pid, Resources { metal: 900.0, crystal: 90.0, deuterium: 9.0 });

    let mut alerts = Vec::new();
    while engine.current_tick() + 1 < despawn {
        engine.tick().unwrap();
        for e in engine.drain_events() {
            if let EventKind::Alert(a) = e.kind
                && a.player_id == Some(pid)
            {
                alerts.push((engine.current_tick(), a));
            }
        }
    }
    assert_eq!(alerts.len(), 1, "one alert, not one per tick: {alerts:?}");
    let (tick, alert) = &alerts[0];
    assert_eq!(*tick, despawn - u64::from(HOME_SALVAGE_WARN_TICKS));
    assert!(alert.message.contains("salvage at home"), "{}", alert.message);
    assert_eq!(alert.sector, Some(home));
}

#[test]
fn a_fleet_docked_at_home_scoops_what_fits_and_the_rest_stays_with_an_alert() {
    let mut engine = world("persistent");
    let (pid, _fid) = join(&mut engine, "Home");
    edit(&mut engine, |s| s.players.get_mut(&pid).unwrap().resources = Resources::default());
    let small = Resources { metal: 900.0, crystal: 90.0, deuterium: 9.0 };
    drop_home_pile(&mut engine, pid, small);
    let events = run(&mut engine, 2);
    let p = &engine.players[&pid];
    assert!(p.resources.metal >= small.metal && p.resources.crystal >= small.crystal, "{:?}", p.resources);
    let home = p.homeworld;
    assert!(engine.sector_overrides[&home.to_key()].salvage.is_none());
    assert!(events.iter().any(|e| matches!(&e.kind, EventKind::Stockpile(s) if s.cause == StockpileCause::Wreckage && s.amount == small)));
    assert!(!events.iter().any(|e| matches!(&e.kind, EventKind::SalvageCollected(_))), "nothing went into a hold");

    let cap = scaling::storage_cap(&p.buildings, &engine.pace());
    edit(&mut engine, |s| s.players.get_mut(&pid).unwrap().resources = cap);
    let despawn = drop_home_pile(&mut engine, pid, Resources { metal: 500.0, crystal: 0.0, deuterium: 0.0 });
    let events = run_until(&mut engine, 400, |e| e.current_tick() + 1 >= despawn);
    let ov = &engine.sector_overrides[&home.to_key()];
    assert_eq!(ov.salvage.map(|s| s.metal), Some(500.0), "nothing fits, so the pile stays");
    let full = events
        .iter()
        .filter(|e| matches!(&e.kind, EventKind::Alert(a) if a.message.contains("storage is full")))
        .count();
    assert_eq!(full, 1);
}

fn ring_around(center: Hex, radius: u16) -> Vec<Hex> {
    let r = i16::try_from(radius).unwrap();
    (-r..=r)
        .flat_map(|q| (-r..=r).map(move |dr| Hex { q: center.q + q, r: center.r + dr }))
        .filter(|h| Hex::distance(h, &center) <= radius)
        .collect()
}

/// A fleet under `prospect` at home with two rings charted around it: the
/// starting fighters with research no empire reaches when `strong`, a lone
/// Small Cargo that loses to anything armed when not.
fn prospector(name: &str, strong: bool, max_range: u8) -> (iac_sim::GameEngine, u64, u64, Hex) {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, name);
    let home = engine.players[&pid].homeworld;
    if strong {
        empower(&mut engine, pid, fid);
    }
    edit(&mut engine, |s| {
        if !strong {
            s.fleets.get_mut(&fid).unwrap().ships = Ships::of(&[(ShipClass::SmallCargo, 1)]);
        }
        for h in ring_around(home, 2) {
            s.explored.insert((pid, h.to_key()));
            s.charted.insert((pid, h.to_key()));
        }
    });
    let params = PolicyParams { max_range, ..PolicyParams::default() };
    engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, Some(params)).unwrap();
    engine.drain_events();
    (engine, pid, fid, home)
}

fn policy_actions(events: &[iac_shared::protocol::GameEvent]) -> Vec<(String, String)> {
    events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::PolicyAction(p) => Some((p.action.clone(), p.reason.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_weak_prospector_never_leaves_the_charted_ring_into_a_fight_it_would_lose() {
    let (mut engine, _pid, fid, home) = prospector("Weakling", false, 4);
    let mut holds = Vec::new();
    for _ in 0..300 {
        engine.tick().unwrap();
        let at = engine.fleets[&fid].location;
        assert!(Hex::distance(&at, &home) <= 2, "the fleet left the charted ring for {at}");
        holds.extend(policy_actions(&engine.drain_events()).into_iter().filter(|(a, _)| a == "hold").map(|(_, r)| r));
    }
    assert!(holds.iter().any(|h| h.contains("DEADLY")), "the refusal names the verdict: {holds:?}");
    assert!(holds[0].contains("doctrine needs"), "{}", holds[0]);
}

#[test]
fn a_strong_prospector_pushes_the_frontier_past_the_charted_ring() {
    let (mut engine, pid, _fid, home) = prospector("Bruiser", true, 4);
    let events = run(&mut engine, 400);
    let moves = policy_actions(&events).iter().filter(|(a, _)| a == "move").count();
    assert!(moves > 0);
    let beyond = engine
        .snapshot()
        .explored
        .iter()
        .filter(|(p, k)| *p == pid && Hex::distance(&Hex::from_key(*k), &home) > 2)
        .count();
    assert!(beyond >= 3, "a strong fleet charts past the ring: {beyond}");
}

fn metal_tile(engine: &iac_sim::GameEngine) -> Hex {
    find_sector(engine, 6, |m, _, _| m == Density::Pristine)
}

fn strip_metal(engine: &mut iac_sim::GameEngine, at: Hex) {
    edit(engine, |s| {
        let ov = s.sector_overrides.entry(at.to_key()).or_default();
        ov.metal_density = Some(Density::None);
        ov.metal_harvested = 0.0;
    });
}

#[test]
fn a_stripped_tile_refills_on_the_spec_schedule_scaled_by_pace() {
    let mut engine = world("test");
    let at = metal_tile(&engine);
    strip_metal(&mut engine, at);
    let want = (f64::from(scaling::ore_regen_hours(at.dist_from_origin())) * 3600.0 / 8000.0).ceil() as u64;
    assert!(want > 50, "the test pace leaves enough ticks to measure: {want}");

    let first = engine.ore_reserve_at(at).unwrap().metal;
    assert_eq!(first.units, 0.0);
    assert!((first.refills_in_s.unwrap() as i64 - want as i64).abs() <= 1, "{:?} vs {want}", first.refills_in_s);

    run(&mut engine, want - 2);
    let almost = engine.ore_reserve_at(at).unwrap().metal;
    assert!(almost.units < almost.max_units && almost.refills_in_s.is_some(), "{almost:?}");
    run(&mut engine, 3);
    let full = engine.ore_reserve_at(at).unwrap().metal;
    assert_eq!(full.units, full.max_units);
    assert_eq!(full.refills_in_s, None);
}

#[test]
fn regeneration_runs_ten_times_faster_at_ten_times_the_pace() {
    let refilled = |pace: &str| {
        let mut engine = world(pace);
        let at = metal_tile(&engine);
        strip_metal(&mut engine, at);
        run(&mut engine, 2000);
        engine.ore_reserve_at(at).unwrap().metal.units
    };
    let (slow, fast) = (refilled("1"), refilled("10"));
    assert!(slow > 0.0 && (fast / slow - 10.0).abs() < 0.5, "{slow} {fast}");
}

#[test]
fn a_tile_does_not_refill_under_a_fleet() {
    let mut engine = world("test");
    let (_, fid) = join(&mut engine, "Squatter");
    let at = metal_tile(&engine);
    strip_metal(&mut engine, at);
    deploy(&mut engine, fid, at, None);
    run(&mut engine, 50);
    assert_eq!(engine.ore_reserve_at(at).unwrap().metal.units, 0.0);
    deploy(&mut engine, fid, Hex { q: 0, r: 0 }, None);
    run(&mut engine, 1);
    assert!(engine.ore_reserve_at(at).unwrap().metal.units > 0.0);
}

#[test]
fn harvesting_empties_a_tile_by_its_ring_reserve_and_research_only_raises_the_yield() {
    let mine = |level: u8| {
        let mut engine = world("persistent");
        let (pid, fid) = join(&mut engine, "Miner");
        let at = metal_tile(&engine);
        edit(&mut engine, |s| {
            s.players.get_mut(&pid).unwrap().research.laser_technology = level;
            let fleet = s.fleets.get_mut(&fid).unwrap();
            fleet.location = at;
            fleet.ships = Ships::of(&[(ShipClass::Recycler, 2)]);
        });
        let started = engine.execute(pid, Command::Harvest { fleet_id: fid, resource: HarvestResource::Metal });
        assert!(started.error.is_none());
        run(&mut engine, 400);
        (engine.fleets[&fid].cargo.metal, engine.ore_reserve_at(at).unwrap().metal, at)
    };
    let (plain, tile, at) = mine(0);
    let total = scaling::ore_reserve_units(Density::Pristine, at.dist_from_origin());
    assert_eq!(tile.units, 0.0, "the tile is emptied");
    assert!((plain - total).abs() < 0.5, "cargo holds the whole reserve: {plain} vs {total}");
    let (boosted, _, _) = mine(2);
    assert!((boosted / plain - 1.4).abs() < 0.01, "level 2 is a 40 percent better yield: {boosted} vs {plain}");
}

#[test]
fn a_prospector_with_nothing_left_in_range_holds_and_says_why_without_flooding() {
    let (mut engine, _pid, fid, home) = prospector("Boxed", true, 2);
    let events = run(&mut engine, 200);
    let actions = policy_actions(&events);
    assert!(actions.iter().all(|(a, _)| a != "move"), "everything in range is charted: nothing to move to");
    let holds: Vec<_> = actions.iter().filter(|(a, _)| a == "hold").collect();
    assert!(!holds.is_empty(), "the hold must be announced");
    assert!(holds[0].1.contains("and the doctrine goes no farther than 2 hops of home"), "{}", holds[0].1);
    assert_eq!(holds.len(), 1, "the same hold is told once: {holds:?}");
    assert_eq!(engine.fleets[&fid].location, home);
}

#[test]
fn a_prospector_goes_back_for_the_chart_a_lost_fleet_never_brought_home() {
    let (mut engine, pid, fid, home) = prospector("Second", true, 2);
    let entered = engine.world_gen.connected_neighbors(home).slice()[0];
    let chart = (pid, entered.to_key());
    edit(&mut engine, |s| {
        s.players.get_mut(&pid).unwrap().resources.deuterium = 5_000.0;
        assert!(s.explored.contains(&chart));
        s.charted.remove(&chart);
    });

    let events = run_until(&mut engine, 300, |e| e.snapshot().charted.contains(&chart));
    assert!(policy_actions(&events).iter().any(|(a, why)| a == "move" && why == "pushing frontier"));
    assert_eq!(engine.fleets[&fid].location, home, "the chart is banked at the dock");
}

#[test]
fn a_standing_hold_is_reported_when_it_begins_and_not_while_it_goes_on() {
    let (mut engine, ..) = prospector("Patient", false, 4);
    let holds = |events: Vec<iac_shared::protocol::GameEvent>| {
        policy_actions(&events).into_iter().filter(|(a, _)| a == "hold").collect::<Vec<_>>()
    };
    let first_minute = holds(run(&mut engine, 55));
    assert_eq!(first_minute.len(), 1, "one report, not one per evaluation: {first_minute:?}");
    let later = holds(run(&mut engine, 300));
    assert!(later.is_empty(), "nothing more while the reason stays the same: {later:?}");
}

#[test]
fn a_mining_run_keeps_its_claims_within_range_of_home_as_each_one_runs_dry() {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, "Miner");
    let home = engine.players[&pid].homeworld;
    let far = outward_chain(&engine, home, 4);
    edit(&mut engine, |s| {
        for h in &far {
            s.explored.insert((pid, h.to_key()));
        }
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.location = far[3];
        fleet.fuel = 1_000_000.0;
        fleet.fuel_max = 1_000_000.0;
    });
    let params = PolicyParams { max_range: 2, ..PolicyParams::default() };
    engine.handle_policy_update(pid, fid, PolicyPreset::MineAndReturn, Some(params)).unwrap();
    engine.drain_events();

    let mut dried = 0;
    for tick in 0..400 {
        engine.tick().unwrap();
        let Some(work) = engine.policies.get(&fid).and_then(|p| p.work_sector) else { continue };
        assert!(
            work == home || Hex::distance(&work, &home) <= 2,
            "claim {work} is {} from home at tick {tick}",
            Hex::distance(&work, &home)
        );
        if tick % 25 == 24 && work != home && dried < 6 {
            edit(&mut engine, |s| {
                let ov = s.sector_overrides.entry(work.to_key()).or_default();
                ov.metal_density = Some(Density::None);
                ov.crystal_density = Some(Density::None);
                ov.deut_density = Some(Density::None);
            });
            dried += 1;
        }
    }
    assert!(dried > 0, "the run claimed sectors to exhaust");
}

/// A fleet that outguns everything near home, so only the doctrine decides
/// where it goes, with its fuel left unbounded.
fn delegate(name: &str, preset: PolicyPreset, max_range: u8) -> (iac_sim::GameEngine, u64, u64, Hex) {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, name);
    let home = engine.players[&pid].homeworld;
    empower(&mut engine, pid, fid);
    edit(&mut engine, |s| {
        let player = s.players.get_mut(&pid).unwrap();
        player.buildings.deuterium_tank = 8;
        player.resources.deuterium = 5_000.0;
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.fuel_max *= scaling::tank_modifier(8);
        fleet.fuel = fleet.fuel_max;
    });
    let params = PolicyParams { max_range, fuel_budget_pct: 100, ..PolicyParams::default() };
    engine.handle_policy_update(pid, fid, preset, Some(params)).unwrap();
    engine.drain_events();
    (engine, pid, fid, home)
}

fn moves(actions: &[(String, String)]) -> Vec<&str> {
    actions.iter().filter(|(a, _)| a == "move").map(|(_, why)| why.as_str()).collect()
}

fn holds(actions: &[(String, String)]) -> Vec<&str> {
    actions.iter().filter(|(a, _)| a == "hold").map(|(_, why)| why.as_str()).collect()
}

#[test]
fn a_mining_fleet_brings_the_ore_in_reach_home_and_then_waits_for_the_seams_to_refill() {
    let (mut engine, pid, fid, home) = delegate("Stripper", PolicyPreset::MineAndReturn, 3);
    let in_reach: f64 = ring_around(home, 3)
        .into_iter()
        .filter_map(|h| engine.ore_reserve_at(h))
        .map(|o| o.metal.units + o.crystal.units + o.deuterium.units)
        .sum();
    assert!(in_reach > 100.0, "the seed puts ore within three hexes of home: {in_reach}");
    let events = run(&mut engine, 6000);
    let mined: f64 = events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ResourceHarvested(h) if h.fleet_id == fid => Some(h.resources.total()),
            _ => None,
        })
        .sum();
    assert!(mined > 0.5 * in_reach, "of {in_reach:.0} units of ore in reach, {mined:.0} were mined");

    let idle = policy_actions(&run(&mut engine, 1200));
    assert!(moves(&idle).is_empty(), "stripped seams are left to refill: {:?}", moves(&idle));
    assert_eq!(engine.fleets[&fid].location, home);
    assert_eq!(engine.fleets[&fid].cargo.total(), 0.0, "what was mined is in the stockpile");
    let worked = policy_actions(&events);
    let told: Vec<&str> = holds(&worked).into_iter().chain(holds(&idle)).collect();
    assert!(told.last().is_some_and(|why| why.contains("repays the fuel")), "the hold it went on with was told when it began: {told:?}");
    assert!(holds(&idle).len() <= 1, "and is not told again while it goes on: {:?}", holds(&idle));
    assert!(engine.players[&pid].resources.total() > 0.5 * in_reach);
}

#[test]
fn a_fleet_whose_cargo_the_full_store_cannot_take_waits_at_the_dock_until_it_can_unload() {
    let (mut engine, pid, fid, home) = delegate("Laden", PolicyPreset::MineAndReturn, 3);
    let hold = iac_sim::engine::fleet_cargo_capacity(&engine.fleets[&fid], &engine.world.pace);
    edit(&mut engine, |s| {
        let player = s.players.get_mut(&pid).unwrap();
        player.resources.metal = scaling::storage_cap(&player.buildings, &s.meta.pace).metal;
        s.fleets.get_mut(&fid).unwrap().cargo.metal = hold;
    });

    let blocked = policy_actions(&run(&mut engine, 300));
    assert!(moves(&blocked).is_empty(), "it would only come straight back: {:?}", moves(&blocked));
    assert!(holds(&blocked).iter().any(|why| why.contains("no room for the cargo aboard")), "{:?}", holds(&blocked));
    assert_eq!(engine.fleets[&fid].cargo.metal, hold);

    edit(&mut engine, |s| s.players.get_mut(&pid).unwrap().buildings.metal_storage = 1);
    let freed = policy_actions(&run_until(&mut engine, 100, |e| e.fleets[&fid].location != home));
    assert_eq!(engine.fleets[&fid].cargo.total(), 0.0);
    assert!(moves(&freed).contains(&"heading to claim"), "{freed:?}");
}

fn drop_pile(engine: &mut iac_sim::GameEngine, at: Hex, pile: Resources) {
    edit(engine, |s| {
        let ov = s.sector_overrides.entry(at.to_key()).or_insert_with(SectorOverride::default);
        ov.salvage = Some(pile);
        ov.salvage_despawn_tick = Some(s.tick + 1_000_000);
    });
}

/// Lane hops from `from` to every sector `passable` lets a fleet through, out to `limit`.
fn hops_from(engine: &iac_sim::GameEngine, from: Hex, limit: u32, passable: impl Fn(Hex) -> bool) -> Vec<(Hex, u32)> {
    let mut seen = std::collections::HashMap::from([(from.to_key(), 0u32)]);
    let mut queue = std::collections::VecDeque::from([from]);
    let mut out = Vec::new();
    while let Some(at) = queue.pop_front() {
        let hops = seen[&at.to_key()];
        if hops == limit {
            continue;
        }
        for &next in engine.world_gen.connected_neighbors(at).slice() {
            if seen.contains_key(&next.to_key()) || !passable(next) {
                continue;
            }
            seen.insert(next.to_key(), hops + 1);
            out.push((next, hops + 1));
            queue.push_back(next);
        }
    }
    out
}

#[test]
fn a_salvage_fleet_passes_over_wreckage_it_may_not_reach_for_wreckage_it_may() {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, "Picker");
    let home = engine.players[&pid].homeworld;
    muster(&mut engine, fid, &[(ShipClass::LightFighter, 2), (ShipClass::SmallCargo, 1)], home);
    edit(&mut engine, |s| {
        s.players.get_mut(&pid).unwrap().buildings.deuterium_tank = 8;
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.fuel_max = 3.0 * (2.0 * 200.0 + 120.0);
        fleet.fuel = fleet.fuel_max;
    });
    let quiet = |e: &iac_sim::GameEngine, h: Hex| {
        e.world_gen.generate_sector(h).npc_template.is_none() && e.derelict_site_at(h).is_none() && h != home
    };
    let rated = |e: &iac_sim::GameEngine, h: Hex| e.odds_for(pid, fid, &[h]).unwrap()[0].ratio;
    let open = |h: Hex| rated(&engine, h) >= 1.5;

    let everywhere = hops_from(&engine, home, 8, |_| true);
    let reachable = hops_from(&engine, home, 8, open);
    let (shut, shut_hops) = *everywhere
        .iter()
        .find(|(h, _)| !open(*h) && quiet(&engine, *h))
        .expect("a quiet sector two fighters and a cargo ship may not enter");
    let (within, _) = *reachable
        .iter()
        .find(|(h, hops)| *hops > shut_hops && quiet(&engine, *h))
        .expect("a quiet sector farther out that they may");
    let pile = Resources { metal: 3000.0, crystal: 0.0, deuterium: 0.0 };
    drop_pile(&mut engine, shut, pile);
    drop_pile(&mut engine, within, pile);
    edit(&mut engine, |s| s.players.get_mut(&pid).unwrap().resources.deuterium = 5_000.0);

    let params = PolicyParams { max_range: 30, fuel_budget_pct: 100, ..PolicyParams::default() };
    engine.handle_policy_update(pid, fid, PolicyPreset::SalvageAndSites, Some(params)).unwrap();
    let events = run_until(&mut engine, 3000, |e| e.sector_overrides[&within.to_key()].salvage.is_none());
    let scooped: Vec<Hex> = events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::SalvageCollected(c) if c.fleet_id == fid => Some(c.sector),
            _ => None,
        })
        .collect();
    assert!(scooped.contains(&within), "{scooped:?}");
    assert!(engine.sector_overrides[&shut.to_key()].salvage.is_some(), "the pile behind the odds is still there");
    assert_eq!(engine.fleets[&fid].ships.total(), 3);
}

#[test]
fn a_salvage_fleet_flies_out_only_for_wreckage_worth_more_than_the_fuel_to_fetch_it() {
    let (mut engine, _pid, fid, home) = delegate("Wary", PolicyPreset::SalvageAndSites, 3);
    let quiet = |h: Hex| {
        engine.world_gen.generate_sector(h).npc_template.is_none() && engine.derelict_site_at(h).is_none()
    };
    let (at, _) = hops_from(&engine, home, 3, |h| Hex::distance(&h, &home) <= 3)
        .into_iter()
        .find(|(h, hops)| *hops == 3 && quiet(*h))
        .expect("a quiet sector three jumps out");
    let burned = 6.0 * engine.hop_fuel_cost(&engine.fleets[&fid]) * iac_shared::constants::FUEL_DEUT_PER_UNIT;

    drop_pile(&mut engine, at, Resources { metal: burned, crystal: 0.0, deuterium: 0.0 });
    let scraps = policy_actions(&run(&mut engine, 200));
    assert!(moves(&scraps).is_empty(), "{burned:.1} metal for {burned:.1} deuterium of fuel: {:?}", moves(&scraps));
    assert!(holds(&scraps).iter().any(|why| why.contains("repays the fuel")), "{:?}", holds(&scraps));

    drop_pile(&mut engine, at, Resources { metal: 20.0 * burned, crystal: 0.0, deuterium: 0.0 });
    run_until(&mut engine, 200, |e| e.sector_overrides[&at.to_key()].salvage.is_none());
    assert!(engine.fleets[&fid].cargo.metal > 0.0);
}

#[test]
fn a_fleet_that_hauls_goes_round_a_fight_it_would_win() {
    let (mut engine, _pid, fid, home) = delegate("Roundabout", PolicyPreset::SalvageAndSites, 6);
    let lair = |h: Hex| engine.world_gen.generate_sector(h).npc_template.is_some_and(|t| t.behavior != Passive);
    let quiet = |h: Hex| engine.world_gen.generate_sector(h).npc_template.is_none() && engine.derelict_site_at(h).is_none();
    let in_range = |h: Hex| Hex::distance(&h, &home) <= 6;
    let direct: std::collections::HashMap<u32, u32> =
        hops_from(&engine, home, 6, in_range).into_iter().map(|(h, hops)| (h.to_key(), hops)).collect();
    let (at, _) = hops_from(&engine, home, 10, |h| in_range(h) && !lair(h))
        .into_iter()
        .find(|(h, round)| quiet(*h) && direct.get(&h.to_key()).is_some_and(|hops| hops < round))
        .expect("a quiet sector whose shortest lane from home crosses a lair");
    drop_pile(&mut engine, at, Resources { metal: 500.0, crystal: 0.0, deuterium: 0.0 });

    let events = run_until(&mut engine, 600, |e| e.sector_overrides[&at.to_key()].salvage.is_none());
    let fights = events.iter().filter(|e| matches!(e.kind, EventKind::CombatStarted(_))).count();
    assert_eq!(fights, 0, "the lane round the lair is longer and costs no hull");
    assert!(engine.fleets[&fid].cargo.metal > 0.0);
}

#[test]
fn a_mauled_fleet_under_orders_goes_home_to_be_mended_before_it_does_anything_else() {
    let (mut engine, _pid, fid, home) = prospector("Bruised", true, 4);
    let out = outward_chain(&engine, home, 2);
    edit(&mut engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.location = out[1];
        fleet.refit = scaling::Refit::of(&scaling::ResearchLevels::default());
        fleet.ships = squadron(ShipClass::LightFighter, 1, &[250]);
    });
    let (held, full) = engine.fleets[&fid].ships.hull(engine.fleets[&fid].arms());
    assert_eq!((held, full), (650, 800), "one Light Fighter whole and one at 250 of its 400 hull");

    let events = run_until(&mut engine, 200, |e| e.fleets[&fid].location == home);
    let acts = policy_actions(&events);
    assert_eq!(acts[0].0, "move", "its first act under orders: {acts:?}");
    assert!(
        acts[0].1.starts_with(&format!(
            "returning to be mended: hull at 81% of full, and a fleet under orders works at 85% or more; home is 2 jumps away by {}",
            out[0],
        )),
        "and it says why and which way home is: {acts:?}",
    );
    assert!(acts.iter().all(|(action, _)| action == "move"), "{acts:?}");

    let mended: Vec<String> = events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::Alert(a) if a.message.contains("was mended at the home dock") => Some(a.message.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(mended.len(), 1, "{mended:?}");
    let fleet = &engine.fleets[&fid];
    assert_eq!(fleet.ships.hull(fleet.arms()).0, fleet.ships.hull(fleet.arms()).1, "the home dock made it whole");
    let next = policy_actions(&run(&mut engine, 30));
    assert!(next.iter().all(|(_, why)| !why.contains("mended")), "and it goes on with its orders: {next:?}");
}

#[test]
fn a_patrol_hunts_out_to_its_range_brings_home_what_its_kills_leave_and_waits_at_the_dock() {
    let (mut engine, _, fid, home) = delegate("Hunter", PolicyPreset::PatrolHome, 4);
    let contacts = ring_around(home, 4)
        .into_iter()
        .filter(|h| engine.world_gen.generate_sector(*h).npc_template.is_some())
        .count();
    assert!(contacts > 0, "the seed puts lairs within four hexes of home");

    let mut fought_at = Vec::new();
    let mut actions = Vec::new();
    for _ in 0..4000 {
        engine.tick().unwrap();
        for (action, why) in policy_actions(&engine.drain_events()) {
            if action == "attack" {
                fought_at.push(engine.fleets[&fid].location);
            }
            actions.push((action, why));
        }
    }
    assert!(!fought_at.is_empty(), "{actions:?}");
    assert!(fought_at.iter().all(|at| Hex::distance(at, &home) <= 4), "{fought_at:?}");
    assert!(fought_at.iter().any(|at| Hex::distance(at, &home) > 2), "the hunt reaches past two hexes: {fought_at:?}");
    assert!(
        actions.iter().any(|(a, why)| a == "salvage" && why == "scooping the wreckage of the kill"),
        "kills leave wreckage and the patrol takes it: {actions:?}",
    );
    assert!(
        actions.iter().any(|(a, why)| a == "unload" && why.starts_with("unloaded ") && why.ends_with(" into the stockpile")),
        "and brings it home: {actions:?}",
    );
    let wandering: Vec<&str> = moves(&actions)
        .into_iter()
        .filter(|why| {
            !why.starts_with("intercepting") && !why.starts_with("collecting the wreckage at ") && !why.starts_with("returning")
        })
        .collect();
    assert!(wandering.is_empty(), "every jump has a purpose: {wandering:?}");
    assert!(
        holds(&actions).iter().any(|why| {
            why.starts_with("no contact within 4 hops of home") && why.ends_with(", and no wreckage that repays the fuel")
        }),
        "with the beat cleared and its wreckage collected it waits at the dock: {:?}",
        holds(&actions),
    );
    assert_eq!((engine.fleets[&fid].location, engine.fleets[&fid].cargo.total()), (home, 0.0), "with an empty hold");
}

fn orders_flown(engine: &mut iac_sim::GameEngine, pid: u64, fid: u64) -> Vec<(String, String)> {
    engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, None).unwrap();
    engine.drain_events();
    policy_actions(&run(engine, 40))
}

#[test]
fn a_jump_a_fight_catches_goes_on_under_fire_and_a_recall_leaves_the_fleet_free_for_standing_orders() {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, "Jumper");
    let at = lone_fighter_sector(&engine, Passive);
    let escort = engine.handle_split(pid, fid, &counts(&[(ShipClass::LightFighter, 1)]), false).unwrap();
    deploy(&mut engine, fid, at, None);
    deploy(&mut engine, escort, at, Some(MIGHTY));
    let onward = engine.world_gen.connected_neighbors(at).slice().iter().copied()
        .find(|n| engine.world_gen.generate_sector(*n).npc_template.is_none_or(|t| t.behavior == Passive))
        .expect("a sector next door that starts no fight");

    let tank = engine.fleets[&fid].fuel;
    assert!(engine.execute(pid, Command::Move { fleet_id: fid, target: onward }).error.is_none());
    assert_eq!(engine.fleets[&fid].state, FleetStatus::Moving);
    assert_eq!(tank - engine.fleets[&fid].fuel, 20.0, "one Light Fighter burns 20 fuel a jump, paid when it is ordered");
    let attack = engine.execute(pid, Command::Attack { fleet_id: escort, target_fleet_id: template_npc_id(at) });
    assert!(attack.error.is_none(), "{:?}", attack.error);
    let charging = &engine.fleets[&fid];
    assert_eq!((charging.state, charging.move_target), (FleetStatus::Moving, Some(onward)), "a fleet charging its jump keeps it");
    assert!(engine.combat_of(fid).is_some(), "and stands in the sector's fight until it is gone");
    let shown = iac_sim::views::full_state(&engine, pid).unwrap().fleets.into_iter().find(|f| f.id == fid).unwrap();
    assert_eq!(shown.fleeing, Some(Fleeing { to: onward, ticks: 2, rounds: 1 }), "which its owner is shown");
    assert_eq!(shown.flight, None, "a fleet that is leaving is quoted no second flight");
    let (code, why) = engine.execute(pid, Command::Move { fleet_id: fid, target: onward }).error.expect("it takes no other jump");
    assert_eq!(code, ErrorCode::OnCooldown);
    assert_eq!(why, format!("fleet {fid} is already leaving the fight at {at} for {onward}; stop calls the jump off, and its fuel is forfeit"));

    let fought = run_until(&mut engine, 10, |e| e.fleets[&fid].location == onward);
    let silent: Vec<(bool, u64)> = fought
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::BattleRound(r) => r.fleets.groups.iter().find(|g| g.fleet_id == fid).map(|g| (g.fleeing, r.fleets.shots)),
            _ => None,
        })
        .collect();
    assert_eq!(silent.len(), 1, "it stood in the one round fought before its jump landed");
    assert!(silent[0].0, "marked as leaving");
    assert_eq!(silent[0].1, 1, "and of the two fleets only the escort fired");
    assert_eq!(engine.fleets[&fid].state, FleetStatus::Idle, "the jump landed");
    assert_eq!(engine.fleets[&fid].fuel, tank - 20.0, "on the fuel it had paid");
    let caught: Vec<_> = fought
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::Alert(a) if a.message.contains("was caught") => Some(a),
            _ => None,
        })
        .collect();
    assert_eq!(caught.len(), 1, "the fleet that was charging is told once, the fleet that picked the fight is not: {caught:?}");
    assert_eq!(
        caught[0].message,
        format!(
            "fleet {fid} was caught at {at} while charging its jump to {onward}: \
             it is fired on and does not answer until the jump lands in 2 ticks"
        ),
    );
    assert!(
        matches!(caught[0].level, AlertLevel::Warning) && (caught[0].player_id, caught[0].fleet_id, caught[0].sector) == (Some(pid), Some(fid), Some(at)),
        "{:?}",
        caught[0],
    );
    let flown = orders_flown(&mut engine, pid, fid);
    assert!(!flown.is_empty(), "standing orders never ran after the fight");

    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, "Recaller");
    let home = engine.players[&pid].homeworld;
    let out = engine.world_gen.connected_neighbors(home).slice()[0];
    deploy(&mut engine, fid, out, None);
    let onward = engine.world_gen.connected_neighbors(out).slice().iter().copied().find(|next| *next != home).unwrap();
    assert!(engine.execute(pid, Command::Move { fleet_id: fid, target: onward }).error.is_none());
    assert_eq!(engine.fleets[&fid].state, FleetStatus::Moving);
    assert!(engine.execute(pid, Command::Recall { fleet_id: fid }).error.is_none());
    assert_eq!((engine.fleets[&fid].location, engine.fleets[&fid].state), (home, FleetStatus::Idle));
    assert_eq!(engine.fleets[&fid].move_target, None, "the jump it was charging is abandoned");
    let flown = orders_flown(&mut engine, pid, fid);
    assert!(!flown.is_empty(), "standing orders never ran after the recall");
}

/// A fleet that cannot hurt a hostile, which wears it down too slowly to
/// end it soon: Espionage Probes, which have no weapon, behind hulls 26
/// times their own. The fight lasts until something else stops it.
fn endless_fight(engine: &mut iac_sim::GameEngine, pid: u64, fid: u64, at: Hex) {
    muster(engine, fid, &[(ShipClass::EspionageProbe, 3)], at);
    edit(engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.refit.armour = MIGHTY.armour;
        fleet.fuel = 1e12;
        fleet.fuel_max = 1e12;
        fleet.cargo = Resources { metal: 40.0, ..Resources::default() };
    });
    let attack = engine.execute(pid, Command::Attack { fleet_id: fid, target_fleet_id: template_npc_id(at) });
    assert!(attack.error.is_none(), "{:?}", attack.error);
    run(engine, 3);
    assert_eq!(engine.fleets[&fid].state, FleetStatus::InCombat);
    assert_eq!(engine.active_combats.len(), 1);
}

fn ended_without_victory(events: &[iac_shared::protocol::GameEvent], at: Hex) -> usize {
    events
        .iter()
        .filter(|e| matches!(&e.kind, EventKind::CombatEnded(c) if c.sector == at && !c.player_victory && c.salvage.is_none()))
        .count()
}

#[test]
fn a_recalled_fleet_is_out_of_its_fight_and_the_hostiles_it_left_can_be_fought_again() {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, "Runner");
    let home = engine.players[&pid].homeworld;
    let at = lone_fighter_sector(&engine, Passive);
    endless_fight(&mut engine, pid, fid, at);
    let stock = engine.players[&pid].resources.metal;

    assert!(engine.execute(pid, Command::Recall { fleet_id: fid }).error.is_none());
    let on_arrival = engine.fleets[&fid].ships.clone();
    let after = run(&mut engine, 5);

    assert_eq!((engine.fleets[&fid].location, engine.fleets[&fid].state), (home, FleetStatus::Idle));
    assert!(engine.players[&pid].resources.metal >= stock + 40.0, "the cargo is banked");
    assert!(engine.active_combats.is_empty(), "the fight went on without the fleet in the sector");
    let rounds = after.iter().filter(|e| matches!(&e.kind, EventKind::BattleRound(_))).count();
    assert_eq!(rounds, 0, "a fleet at home was still trading fire");
    assert_eq!(engine.fleets[&fid].ships, on_arrival, "a fleet at home was still taking damage");
    assert_eq!(ended_without_victory(&after, at), 1, "the owner is told the fight is over");
    let how = after.iter().find_map(|e| match &e.kind {
        EventKind::CombatEnded(c) => Some((c.outcome, c.rounds, c.losses.iter().map(|l| l.fleet_id).collect::<Vec<_>>())),
        _ => None,
    });
    assert_eq!(
        how,
        Some((BattleOutcome::Withdrawn, 3, vec![fid, template_npc_id(at)])),
        "it ended because the last fleet left, after the rounds it stood, and both sides are accounted for",
    );

    let owner = engine.players[&pid].clone();
    let told = after.iter().filter_map(|e| iac_sim::views::event_for(e, &owner, &engine)).collect::<Vec<_>>();
    assert_eq!(ended_without_victory(&told, at), 1, "and the event reaches them at home");

    muster(&mut engine, fid, &[(ShipClass::LightFighter, 2)], home);
    pile_of(&fight(&mut engine, pid, fid, at));
}

#[test]
fn a_fight_nobody_is_left_in_frees_its_hostiles_for_the_next_fleet() {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, "Vanished");
    let (next, next_fleet) = join(&mut engine, "Next");
    let at = lone_fighter_sector(&engine, Passive);
    endless_fight(&mut engine, pid, fid, at);

    edit(&mut engine, |s| {
        s.fleets.remove(&fid);
    });
    run(&mut engine, 2);
    assert!(engine.active_combats.is_empty());
    let listed = &chart_after_visit(&mut engine, next, next_fleet, at).hostiles;
    assert_eq!(listed.as_ref().map(|h| h[0].id), Some(template_npc_id(at)), "the sector lists its hostile");

    pile_of(&fight(&mut engine, next, next_fleet, at));
}

fn chart_after_visit(engine: &mut iac_sim::GameEngine, pid: u64, fid: u64, at: Hex) -> iac_shared::protocol::SectorState {
    deploy(engine, fid, at, None);
    run(engine, 1);
    chart(engine, pid).remove(&at.to_key()).expect("the sector is charted")
}

#[test]
fn every_fleet_in_a_crowded_sector_fights_and_comes_out_free() {
    let mut engine = world("persistent");
    let crews: Vec<(u64, u64)> = (0..9).map(|i| join(&mut engine, &format!("Crew{i}"))).collect();
    let at = lone_fighter_sector(&engine, Passive);
    for &(_, fid) in &crews {
        deploy(&mut engine, fid, at, Some(MIGHTY));
    }
    let (lead, lead_fleet) = crews[0];
    let attack = engine.execute(lead, Command::Attack { fleet_id: lead_fleet, target_fleet_id: template_npc_id(at) });
    assert!(attack.error.is_none(), "{:?}", attack.error);
    let events = run_until(&mut engine, 10, |e| e.active_combats.is_empty());

    let fought = events
        .iter()
        .find_map(|e| match &e.kind {
            EventKind::CombatEnded(c) if c.player_victory => Some(c.owners.len()),
            _ => None,
        })
        .expect("the fight was won");
    assert_eq!(fought, crews.len(), "an empire in the sector was left out of the fight");
    for &(pid, fid) in &crews {
        let name = engine.players[&pid].name.clone();
        assert_eq!(engine.fleets[&fid].state, FleetStatus::Idle, "{name} is still marked in combat after the fight");
        let order = engine.execute(pid, Command::Scan { fleet_id: fid });
        assert!(order.error.is_none(), "{name} takes no orders: {:?}", order.error);
    }
}

const HORDE: u64 = 900_001;

/// A fleet of `ours` beside a hostile group of `theirs` on quiet ground
/// two jumps from home, and the fight between them played to its end:
/// every round as it was reported, and how it ended.
fn pitched(
    name: &str,
    ours: &[(ShipClass, u32)],
    theirs: &[(ShipClass, u32)],
) -> (iac_sim::GameEngine, u64, Vec<BattleRoundEvent>, CombatEndedEvent) {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, name);
    let home = engine.players[&pid].homeworld;
    let at = outward_chain(&engine, home, 2)[1];
    muster(&mut engine, fid, ours, at);
    horde(&mut engine, HORDE, at, theirs, 1.0);
    let attack = engine.execute(pid, Command::Attack { fleet_id: fid, target_fleet_id: HORDE });
    assert!(attack.error.is_none(), "{:?}", attack.error);
    let events = run_until(&mut engine, 500, |e| e.active_combats.is_empty());
    let rounds = events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::BattleRound(round) => Some(round.clone()),
            _ => None,
        })
        .collect();
    let ended = events
        .iter()
        .find_map(|e| match &e.kind {
            EventKind::CombatEnded(ended) => Some(ended.clone()),
            _ => None,
        })
        .expect("the fight ended");
    (engine, fid, rounds, ended)
}

fn fire(side: &BattleSide, from: ShipClass, at: ShipClass) -> Option<&Volley> {
    let (from, at) = (ShipyardItem::Ship(from), ShipyardItem::Ship(at));
    side.volleys.iter().find(|v| v.from == from && v.at == at)
}

#[test]
fn a_fight_of_hundreds_a_side_is_reported_a_round_a_tick_by_class_and_ends_with_what_each_side_lost() {
    use ShipClass::{Cruiser, HeavyFighter, LightFighter, SmallCargo};
    let ours = [(SmallCargo, 20), (LightFighter, 400), (Cruiser, 80)];
    let theirs = [(LightFighter, 400), (HeavyFighter, 60)];
    let (engine, fid, rounds, ended) = pitched("Admiral", &ours, &theirs);
    assert!(rounds.len() >= 3, "a fight of hundreds is not over in a volley: {} rounds", rounds.len());

    let count_of = |fleet: &[(ShipClass, u32)]| -> Vec<(ShipyardItem, u32)> {
        fleet.iter().map(|&(class, count)| (ShipyardItem::Ship(class), count)).collect()
    };
    let (mut standing, mut hostile) = (count_of(&ours), count_of(&theirs));
    let mut lost: Vec<(ShipyardItem, u32)> = Vec::new();
    for (i, round) in rounds.iter().enumerate() {
        assert_eq!((round.round, round.fleets.groups.len(), round.hostiles.groups.len()), (i as u32 + 1, 1, 1));
        let (fleet, group) = (&round.fleets.groups[0], &round.hostiles.groups[0]);
        assert_eq!((fleet.fleet_id, fleet.owner.as_deref(), group.fleet_id, group.label.as_str()), (fid, Some("Admiral"), HORDE, "horde"));

        for (side, units, before) in [(&round.fleets, &group.units, &mut hostile), (&round.hostiles, &fleet.units, &mut standing)] {
            let sum = |pick: fn(&Volley) -> u64| side.volleys.iter().map(pick).sum::<u64>();
            assert_eq!(
                (side.shots, side.absorbed, side.hull_damage, u64::from(side.destroyed)),
                (sum(|v| v.shots.into()), sum(|v| v.absorbed), sum(|v| v.hull_damage), sum(|v| v.destroyed.into())),
                "round {}: a side's totals are its volleys summed",
                round.round,
            );
            assert!(side.volleys.iter().all(|v| v.bounced + v.wasted <= v.shots && v.absorbed + v.hull_damage <= side.power));
            assert_eq!(side.destroyed, units.iter().map(|u| u.lost).sum::<u32>(), "what one side destroyed the other lost");
            for unit in units {
                let was = before.iter_mut().find(|(kind, _)| *kind == unit.unit).expect("a kind that entered the round");
                assert_eq!(was.1 - unit.lost, unit.count, "round {}: {:?}", round.round, unit.unit);
                was.1 = unit.count;
                assert!(unit.at_risk <= unit.damaged && unit.damaged <= unit.count && unit.hull <= unit.hull_max);
            }
        }
        for unit in fleet.units.iter().filter(|u| u.lost > 0) {
            match lost.iter_mut().find(|(kind, _)| *kind == unit.unit) {
                Some(so_far) => so_far.1 += unit.lost,
                None => lost.push((unit.unit, unit.lost)),
            }
        }
        assert!(round.fleets.volleys.len() <= 6 && round.hostiles.volleys.len() <= 6, "a line for each pair of classes and no more");
        let size = serde_json::to_string(round).unwrap().len();
        assert!(size < 3000, "round {} of nine hundred ships is {size} bytes on the wire", round.round);
    }

    let first = &rounds[0];
    let shots_from = |side: &BattleSide, class: ShipClass| -> u32 {
        side.volleys.iter().filter(|v| v.from == ShipyardItem::Ship(class)).map(|v| v.shots).sum()
    };
    assert_eq!(shots_from(&first.fleets, LightFighter), 400, "a Light Fighter has no rapid fire here: one shot a round");
    assert_eq!((shots_from(&first.fleets, SmallCargo), shots_from(&first.hostiles, LightFighter)), (20, 400));
    let heavy = shots_from(&first.hostiles, HeavyFighter);
    assert!((60..80).contains(&heavy), "sixty Heavy Fighters fire again only after a shot at a Small Cargo: {heavy}");
    let cruisers = u64::from(shots_from(&first.fleets, Cruiser));
    assert!(cruisers > 200, "eighty Cruisers among Light Fighters: {cruisers} shots");
    assert_eq!(first.fleets.power, 400 * 50 + cruisers * 400 + 20 * 5, "the power fired is every shot's attack");

    assert_eq!((ended.outcome, ended.player_victory, ended.rounds as usize), (BattleOutcome::HostilesDestroyed, true, rounds.len()));
    assert!(hostile.iter().all(|(_, left)| *left == 0), "{hostile:?}");
    let [mine, theirs_lost] = ended.losses.as_slice() else { panic!("one entry a side: {:?}", ended.losses) };
    assert_eq!((mine.fleet_id, mine.destroyed, theirs_lost.fleet_id, theirs_lost.destroyed), (fid, false, HORDE, true));
    lost.sort_by_key(|(kind, _)| kind.index());
    assert_eq!(mine.lost.iter().map(|l| (l.unit, l.count)).collect::<Vec<_>>(), lost, "the summary is the rounds' losses added up");
    assert_eq!(theirs_lost.lost.iter().map(|l| (l.unit, l.count)).collect::<Vec<_>>(), count_of(&[(LightFighter, 400), (HeavyFighter, 60)]));
    assert!(ended.salvage.is_some_and(|pile| pile.total() > 0.0), "the hostiles' destruction leaves their wreckage");

    let fleet = &engine.fleets[&fid];
    let last = &rounds.last().unwrap().fleets.groups[0];
    let after: Vec<(ShipyardItem, u32, u32)> =
        fleet.ships.groups().iter().map(|g| (ShipyardItem::Ship(g.ship_class), g.count, g.damaged.len() as u32)).collect();
    let reported: Vec<(ShipyardItem, u32, u32)> = last.units.iter().filter(|u| u.count > 0).map(|u| (u.unit, u.count, u.damaged)).collect();
    assert_eq!(after, reported, "the fleet is what the last round left standing");
    assert!(fleet.ships.has_damage(), "and it carries its damage out of the fight");
    let state = iac_sim::views::full_state(&engine, fleet.owner_id).unwrap();
    let told = state.fleets.iter().find(|f| f.id == fid).unwrap();
    assert!(told.hull < told.hull_max && told.repair.is_some() && state.battles.is_empty());
}

#[test]
fn cruisers_fire_six_times_a_round_among_light_fighters_and_heavy_fighters_once() {
    use ShipClass::{Cruiser, HeavyFighter, LightFighter};
    let (_, _, rounds, ended) = pitched("Broadside", &[(Cruiser, 60)], &[(LightFighter, 400)]);
    let first = &rounds[0];
    let volley = fire(&first.fleets, Cruiser, LightFighter).expect("the Cruisers fired at the Light Fighters");
    assert_eq!(first.fleets.volleys.len(), 1);
    assert!(
        (270..=450).contains(&volley.shots),
        "rapid fire 6 is a shot again five times in six: sixty Cruisers fire about 360 shots, not {}",
        volley.shots,
    );
    assert!(volley.destroyed > 150, "and a Cruiser's 400 breaks a Light Fighter's 400 hull at a shot: {volley:?}");
    assert!(volley.wasted > 0, "some of them at fighters already destroyed in the round: {volley:?}");
    let answer = fire(&first.hostiles, LightFighter, Cruiser).expect("the Light Fighters fired back");
    assert_eq!(answer.shots, 400, "all four hundred, those destroyed in the round too: fire is simultaneous");
    assert!(answer.absorbed > 0 && answer.hull_damage > 0 && answer.bounced == 0, "{answer:?}");
    assert_eq!((ended.outcome, ended.rounds <= 4), (BattleOutcome::HostilesDestroyed, true), "{} rounds", ended.rounds);

    let (_, _, rounds, _) = pitched("Plain", &[(HeavyFighter, 60)], &[(LightFighter, 400)]);
    assert_eq!(fire(&rounds[0].fleets, HeavyFighter, LightFighter).map(|v| v.shots), Some(60), "no rapid fire, one shot a ship");
}

#[test]
fn a_shot_too_weak_for_a_shield_bounces_and_a_fight_nobody_can_hurt_anybody_in_is_a_standoff() {
    use ShipClass::{Battleship, EspionageProbe, LightFighter, Recycler};
    let (_, _, rounds, ended) = pitched("Scrap", &[(Recycler, 5)], &[(Battleship, 2)]);
    for round in &rounds {
        let volley = fire(&round.fleets, Recycler, Battleship).expect("the Recyclers fired");
        assert_eq!(
            (volley.shots, volley.bounced, volley.absorbed, volley.hull_damage),
            (volley.shots, volley.shots, 0, 0),
            "an attack of 1 is under a hundredth of a Battleship's 200 shield: it does nothing at all",
        );
        let standing = &round.hostiles.groups[0].units[0];
        assert_eq!((standing.count, standing.damaged, standing.hull), (2, 0, standing.hull_max));
    }
    assert_eq!((ended.outcome, ended.player_victory), (BattleOutcome::FleetsDestroyed, false));
    assert!(ended.salvage.is_none() && ended.losses[0].destroyed && !ended.losses[1].destroyed);

    let (engine, fid, rounds, ended) = pitched("Watchers", &[(EspionageProbe, 5)], &[(EspionageProbe, 3)]);
    assert_eq!(rounds.len(), 1);
    assert_eq!((rounds[0].fleets.shots, rounds[0].hostiles.shots), (0, 0), "a Probe has no weapon");
    assert_eq!((ended.outcome, ended.rounds, ended.player_victory), (BattleOutcome::Standoff, 1, false));
    assert!(ended.losses.iter().all(|l| l.lost.is_empty() && !l.destroyed), "{:?}", ended.losses);
    let fleet = &engine.fleets[&fid];
    assert_eq!((fleet.state, fleet.ships.total()), (FleetStatus::Idle, 5), "the fleet stays where it is, with what it had");

    let (mut e, lane) = skirmish("persistent", &[(LightFighter, 4)], &[(EspionageProbe, 30)], Passive);
    let hurt = e.engine.npc_fleets[&PACK].ships.clone();
    assert!(hurt.total() < 30 || hurt.has_damage(), "four Light Fighters fired a round among the Probes");
    let mark = jump(&mut e, lane[1]);
    assert_eq!(
        fights_ended(&e, mark).iter().map(|c| c.outcome).collect::<Vec<_>>(),
        vec![BattleOutcome::Standoff],
        "a fleet that is leaving fires nothing and Probes have nothing to fire: a round nobody is hurt in",
    );
    assert_eq!(e.engine.npc_fleets[&PACK].ships, hurt, "a stand-off leaves a group as the fight had left it, not whole again");
}

const PACK: u64 = 900_002;

/// An empire whose fleet of `ours` has fought one round against a group of
/// `theirs` with `behavior`, in the first sector of an open lane, which is
/// the group's own.
fn skirmish(
    pace: &str,
    ours: &[(ShipClass, u32)],
    theirs: &[(ShipClass, u32)],
    behavior: NpcBehaviorType,
) -> (Empire, Vec<Hex>) {
    let mut e = Empire::join(world(pace), "Raider");
    let home = e.engine.players[&e.pid].homeworld;
    let lane = open_lane(&e.engine, home, 5);
    muster(&mut e.engine, e.fid, ours, lane[0]);
    horde(&mut e.engine, PACK, lane[0], theirs, 1.0);
    edit(&mut e.engine, |s| s.npc_fleets.get_mut(&PACK).unwrap().behavior = behavior);
    e.order(Command::Attack { fleet_id: e.fid, target_fleet_id: PACK }).expect("the fleet opens fire");
    e.run(1);
    (e, lane)
}

/// Order the jump to `to` and wait for it to land. A jump out of a fight
/// burns what the fleet was quoted. Returns where the log stood when the
/// order was given.
fn jump(e: &mut Empire, to: Hex) -> usize {
    let (mark, fid) = (e.mark(), e.fid);
    let quoted = e.fleet().flight.map(|quote| quote.fuel);
    let tank = e.engine.fleets[&fid].fuel;
    e.order(Command::Move { fleet_id: fid, target: to }).expect("the jump is taken");
    if let Some(fuel) = quoted {
        assert_eq!(tank - e.engine.fleets[&fid].fuel, fuel, "a flight burns the fuel it was quoted");
    }
    e.run_until(60, |e| e.engine.fleets[&fid].location == to);
    mark
}

/// The rounds the empire's fleet stood in since `mark`: where, whether it
/// was leaving, and the shots its side fired.
fn rounds_stood(e: &Empire, mark: usize) -> Vec<(Hex, bool, u64)> {
    let fid = e.fid;
    e.since(mark, |k| match k {
        EventKind::BattleRound(r) => r.fleets.groups.iter().find(|g| g.fleet_id == fid).map(|g| (r.sector, g.fleeing, r.fleets.shots)),
        _ => None,
    })
}

fn pursuits(e: &Empire, mark: usize) -> Vec<PursuitEvent> {
    e.since(mark, |k| match k {
        EventKind::Pursuit(p) => Some(p.clone()),
        _ => None,
    })
}

fn fights_ended(e: &Empire, mark: usize) -> Vec<CombatEndedEvent> {
    e.since(mark, |k| match k {
        EventKind::CombatEnded(c) => Some(c.clone()),
        _ => None,
    })
}

#[test]
fn a_fleet_as_fast_as_the_pack_it_leaves_is_followed_two_hexes_and_never_caught() {
    use ShipClass::Cruiser;
    let (mut e, lane) = skirmish("persistent", &[(Cruiser, 20)], &[(Cruiser, 24)], NpcBehaviorType::Aggressive);
    let fid = e.fid;
    let burn = 300.0 * f64::from(e.engine.fleets[&fid].ships.total());
    let quote = e.fleet().flight.expect("a fleet in a fight is quoted its flight");
    assert_eq!(
        (quote.fuel, quote.ticks, quote.rounds, quote.refusal.clone()),
        (burn, 2, 1, None),
        "a Cruiser burns 300 fuel a jump; a jump of two ticks is one round under fire",
    );
    assert!(quote.label < RatioLabel::Even, "twenty against twenty-four of a kind: {}", quote.ratio);
    assert_eq!(
        quote.chase,
        Some(ChaseForecast { group_id: PACK, label: "horde".into(), hexes: 2, jump_ticks: 2, rounds_per_hex: 0 }),
        "the pack would follow two hexes and, as fast as the fleet, never fire on it",
    );

    let mark = jump(&mut e, lane[1]);
    assert_eq!(rounds_stood(&e, mark), vec![(lane[0], true, 0)], "one parting round, taken without a shot fired");
    let ended = fights_ended(&e, mark);
    assert_eq!(ended.len(), 1);
    assert_eq!((ended[0].outcome, ended[0].player_victory, ended[0].salvage), (BattleOutcome::Fled, false, None));
    let began = pursuits(&e, mark);
    assert_eq!(began.len(), 1, "{began:?}");
    assert_eq!(
        (began[0].phase, began[0].group_id, began[0].fleet_id, began[0].mine, began[0].sector),
        (PursuitPhase::Began, PACK, fid, true, lane[0]),
    );
    assert_eq!((began[0].to, began[0].arrives_in, began[0].hexes_left), (Some(lane[1]), Some(2), 1));
    assert_eq!(
        e.fleet().pursued_by,
        Some(Pursuit { group_id: PACK, label: "horde".into(), from: lane[0], arrives_in: 2, catches: false, hexes_left: 1 }),
        "the fleet's owner is shown the pack two ticks behind, too late to catch a jump ordered now",
    );
    let got_away_with = e.engine.fleets[&fid].ships.clone();

    let mark = jump(&mut e, lane[2]);
    let went_on = pursuits(&e, mark);
    assert_eq!(went_on.len(), 1, "{went_on:?}");
    assert_eq!(
        (went_on[0].phase, went_on[0].sector, went_on[0].to, went_on[0].arrives_in, went_on[0].hexes_left),
        (PursuitPhase::Continued, lane[1], Some(lane[2]), Some(2), 0),
        "it lands where the fleet was and jumps on",
    );

    let mark = jump(&mut e, lane[3]);
    let gave_up = pursuits(&e, mark);
    assert_eq!(gave_up.len(), 1, "{gave_up:?}");
    assert_eq!((gave_up[0].phase, gave_up[0].sector, gave_up[0].to), (PursuitPhase::Abandoned, lane[2], Some(lane[1])));
    assert_eq!(
        gave_up[0].why.as_deref(),
        Some(format!("it has followed 2 jumps from its own sector at {} and goes no further", lane[0]).as_str()),
    );
    assert_eq!(e.fleet().pursued_by, None);
    assert_eq!(e.engine.fleets[&fid].ships, got_away_with, "nothing fired on the fleet after it left");
    assert!(e.engine.active_combats.is_empty());

    e.run(4);
    let pack = &e.engine.npc_fleets[&PACK];
    assert_eq!((pack.location, pack.errand, pack.strayed), (lane[0], None, 0), "two jumps later the pack is back in its own sector");
    assert!(!pack.whole(), "with the damage the fleet's one round did it");
}

#[test]
fn a_fleet_two_ticks_slower_than_the_pack_is_fired_on_for_two_rounds_at_every_hex_it_is_followed() {
    use ShipClass::{LargeCargo, LightFighter};
    let (mut e, lane) = skirmish("persistent", &[(LargeCargo, 100)], &[(LightFighter, 150)], NpcBehaviorType::Aggressive);
    let fid = e.fid;
    let quote = e.fleet().flight.expect("a quote");
    assert_eq!((quote.ticks, quote.rounds), (4, 3), "a Large Cargo jumps in four ticks: three rounds under fire");
    assert_eq!(
        quote.chase.as_ref().map(|c| (c.hexes, c.jump_ticks, c.rounds_per_hex)),
        Some((2, 2, 2)),
        "and Light Fighters jump in two: two rounds at each of two hexes",
    );
    let set_out_with = hull_held(&e.engine, fid);

    let mark = jump(&mut e, lane[1]);
    assert_eq!(rounds_stood(&e, mark), vec![(lane[0], true, 0); 3], "the parting rounds");
    let trail = e.fleet().pursued_by.expect("the pack follows");
    assert_eq!((trail.from, trail.arrives_in, trail.catches, trail.hexes_left), (lane[0], 2, true, 1));

    for (hex, last) in [(1, false), (2, true)] {
        let before = hull_held(&e.engine, fid);
        let mark = jump(&mut e, lane[hex + 1]);
        assert_eq!(
            rounds_stood(&e, mark),
            vec![(lane[hex], true, 0); 2],
            "caught at {} while it charged, for the two ticks it is the slower",
            lane[hex],
        );
        let joined = e.since(mark, |k| match k {
            EventKind::CombatStarted(c) => Some((c.enemy_fleet_id, c.player_fleet_id, c.sector)),
            _ => None,
        });
        assert_eq!(joined, vec![(PACK, fid, lane[hex])]);
        let told = e.since(mark, |k| match k {
            EventKind::Alert(a) if a.message.contains("was caught") => Some(a.message.clone()),
            _ => None,
        });
        assert_eq!(
            told,
            vec![format!(
                "fleet {fid} was caught at {} while charging its jump to {}: \
                 it is fired on and does not answer until the jump lands in 2 ticks",
                lane[hex],
                lane[hex + 1],
            )],
        );
        assert!(hull_held(&e.engine, fid) < before, "and it was hurt there");
        assert_eq!(e.fleet().pursued_by.is_some(), !last, "the pack follows two hexes and no further");
    }
    let lost = set_out_with - hull_held(&e.engine, fid);
    assert!(lost > 0 && e.engine.fleets[&fid].ships.total() > 0, "{lost} of {set_out_with} hull lost on the run");
    let fleet = e.fleet();
    assert!(fleet.hull < fleet.hull_max && fleet.repair.is_some(), "and what is left carries its damage until a dock");
    e.run(4);
    assert_eq!(e.engine.npc_fleets[&PACK].location, lane[0], "the pack is home again");
}

#[test]
fn a_cruiser_on_impulse_drive_4_jumps_in_one_tick_and_outruns_the_pack_it_leaves() {
    use ShipClass::Cruiser;
    let (mut e, lane) = skirmish("persistent", &[(Cruiser, 20)], &[(Cruiser, 24)], NpcBehaviorType::Aggressive);
    let (pid, fid) = (e.pid, e.fid);
    let impulse = |e: &mut Empire, level: u8| {
        edit(&mut e.engine, |s| s.players.get_mut(&pid).expect("player").research.impulse_drive = level);
        (e.fleet().jump_ticks, e.fleet().flight.expect("a quote"))
    };
    let (ticks, _) = impulse(&mut e, 3);
    assert_eq!(ticks, 2, "on Impulse Drive 3 a Cruiser flies 24,000: two ticks a jump, as fast as the pack");
    let (ticks, quote) = impulse(&mut e, 4);
    assert_eq!(
        (ticks, quote.ticks, quote.rounds),
        (1, 2, 1),
        "on Impulse Drive 4 it flies 27,000: one tick a jump, and a jump out of a fight takes its one parting round",
    );
    assert_eq!(
        quote.chase.map(|c| (c.hexes, c.jump_ticks, c.rounds_per_hex)),
        Some((2, 2, 0)),
        "the pack would follow two hexes, a tick behind at each",
    );

    let mark = jump(&mut e, lane[1]);
    assert_eq!(rounds_stood(&e, mark), vec![(lane[0], true, 0)], "the parting round");
    let got_away_with = e.engine.fleets[&fid].ships.clone();
    let trail = e.fleet().pursued_by.expect("the pack sets out after it");
    assert_eq!((trail.arrives_in, trail.catches), (2, false), "and lands a tick after the fleet has gone on");
    for &next in &lane[2..5] {
        jump(&mut e, next);
    }
    let chased = pursuits(&e, mark);
    assert!(chased.iter().any(|p| p.phase == PursuitPhase::Abandoned), "{chased:?}");
    assert_eq!(e.engine.fleets[&fid].ships, got_away_with, "nothing fired on the fleet after it left");
    assert!(e.engine.active_combats.is_empty() && e.fleet().pursued_by.is_none());
}

#[test]
fn a_fleet_destroyed_takes_its_ships_out_of_the_score() {
    use ShipClass::{Cruiser, LightFighter};
    let mut e = Empire::join(world("persistent"), "Doomed");
    let fid = e.fid;
    let home = e.engine.players[&e.pid].homeworld;
    let at = open_lane(&e.engine, home, 2)[0];
    muster(&mut e.engine, fid, &[(LightFighter, 6)], at);
    horde(&mut e.engine, PACK, at, &[(Cruiser, 30)], 1.0);
    let before = e.score_row();
    let weight = scaling::resource_weight(&LightFighter.build_cost());
    assert!((before.fleet - 6.0 * weight).abs() < 1e-3, "the fleet scores its six ships: {before:?}");
    e.order(Command::Attack { fleet_id: fid, target_fleet_id: PACK }).expect("the fleet opens fire");
    e.run_until(30, |e| e.engine.active_combats.is_empty());
    assert!(e.engine.fleets.get(&fid).is_none_or(|f| f.ships.is_empty()), "thirty Cruisers destroy six Light Fighters");
    let after = e.score_row();
    assert_eq!(after.fleet, 0.0, "a fleet destroyed leaves the score");
    assert!((before.score - after.score - before.fleet).abs() < 1e-3, "and nothing else moved: {before:?} {after:?}");
}

#[test]
fn a_pack_of_the_outer_ring_is_found_short_when_it_is_fought_again_before_it_mends() {
    use ShipClass::{HeavyFighter, LightFighter};
    let mut e = Empire::join(world("season"), "Returner");
    let (pid, fid) = (e.pid, e.fid);
    let ring = 12;
    let (class, generated, _) = scaling::npc_composition(ring);
    assert!(class == LightFighter && generated >= 3, "a group past the Inner Ring is a pack: {generated} {class:?}");
    let at = lair(&e.engine, ring, |behavior| behavior == Passive);
    let id = template_npc_id(at);
    let out = e.engine.world_gen.connected_neighbors(at).slice().iter().copied()
        .find(|n| e.engine.world_gen.generate_sector(*n).npc_template.is_none())
        .expect("a quiet sector beside the lair");
    muster(&mut e.engine, fid, &[(HeavyFighter, 3)], at);
    empower(&mut e.engine, pid, fid);
    e.order(Command::Attack { fleet_id: fid, target_fleet_id: id }).expect("the fleet opens fire");
    e.run_until(10, |e| e.engine.npc_fleets.get(&id).is_some_and(|g| g.ships.total() < u32::from(generated)));
    jump(&mut e, out);
    let left = e.engine.npc_fleets[&id].ships.total();
    assert!((1..u32::from(generated)).contains(&left), "the pack keeps {left} of its {generated}");

    let step = scaling::npc_heal_step_ticks(ring, &e.engine.pace());
    assert!(step > 60, "at season a step of its mending is {step} ticks");
    deploy(&mut e.engine, fid, at, None);
    let mark = e.mark();
    e.order(Command::Attack { fleet_id: fid, target_fleet_id: id }).expect("the fleet comes back for it");
    e.run(1);
    let met: Vec<u32> = e.since(mark, |k| match k {
        EventKind::BattleRound(r) => Some(r.hostiles.groups[0].units.iter().map(|u| u.count + u.lost).sum()),
        _ => None,
    });
    assert_eq!(met.first(), Some(&left), "it is met short of the ships it lost");
}

#[test]
fn how_far_a_group_follows_goes_by_its_behaviour_and_it_follows_only_what_it_was_beating() {
    use NpcBehaviorType::{Aggressive, Patrol, Swarm};
    use ShipClass::Cruiser;
    for (behavior, hexes) in [(Passive, 0), (Patrol, 1), (Aggressive, 2), (Swarm, 2)] {
        let (mut e, lane) = skirmish("persistent", &[(Cruiser, 20)], &[(Cruiser, 24)], behavior);
        let listed = iac_sim::intel::build_sector_state(&e.engine, lane[0], Some(e.pid)).hostiles.expect("the group is in sight");
        assert_eq!((listed[0].chase_hexes, listed[0].jump_ticks), (hexes, 2), "{behavior:?}");
        assert_eq!(e.fleet().flight.expect("a quote").chase.map_or(0, |c| c.hexes), hexes, "{behavior:?}");
        let mut followed = 0;
        for &next in &lane[1..] {
            jump(&mut e, next);
            followed += u8::from(e.fleet().pursued_by.is_some());
        }
        assert_eq!(followed, hexes, "{behavior:?}");
        assert!(e.engine.active_combats.is_empty(), "{behavior:?}");
    }

    let (mut e, lane) = skirmish("persistent", &[(Cruiser, 60)], &[(Cruiser, 40)], Aggressive);
    assert_eq!(e.fleet().flight.expect("a quote").chase, None, "forty Cruisers do not follow sixty that leave them");
    let mark = jump(&mut e, lane[1]);
    assert_eq!(fights_ended(&e, mark).iter().map(|c| c.outcome).collect::<Vec<_>>(), vec![BattleOutcome::Fled]);
    assert!(pursuits(&e, mark).is_empty() && e.fleet().pursued_by.is_none());
    let pack = &e.engine.npc_fleets[&PACK];
    assert_eq!((pack.location, pack.errand), (lane[0], None), "the pack stays and mends");
}

#[test]
fn flight_costs_its_fuel_and_its_parting_rounds_and_earns_nothing_from_a_group_left_standing() {
    use ShipClass::{Cruiser, EspionageProbe, HeavyFighter, LightFighter};
    let (mut e, lane) = skirmish("persistent", &[(Cruiser, 60)], &[(HeavyFighter, 200)], Passive);
    let fid = e.fid;
    let at = lane[0];
    assert!(e.engine.npc_fleets[&PACK].ships.total() < 200, "the first round took Heavy Fighters");

    let burn = |e: &Empire| 300.0 * f64::from(e.engine.fleets[&fid].ships.total());
    let fuel = |e: &Empire| e.engine.fleets[&fid].fuel;
    let tank = fuel(&e);
    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().fuel = 100.0);
    let dry = format!(
        "fleet {fid} has 100 fuel and the jump out of the fight at {at} burns {}: it cannot leave, \
         except by recall if it holds the fuel for that",
        burn(&e),
    );
    assert_eq!(e.fleet().flight.expect("a quote").refusal.as_deref(), Some(dry.as_str()));
    assert_eq!(e.refusal(Command::Move { fleet_id: fid, target: lane[1] }), (ErrorCode::InsufficientFuel, dry));
    assert_eq!(e.engine.fleets[&fid].state, FleetStatus::InCombat);
    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().fuel = tank);

    let cost = burn(&e);
    e.order(Command::Move { fleet_id: fid, target: lane[1] }).expect("the jump out");
    assert_eq!(e.fleet().fleeing, Some(Fleeing { to: lane[1], ticks: 2, rounds: 1 }));
    for order in [Command::Scan { fleet_id: fid }, Command::Attack { fleet_id: fid, target_fleet_id: PACK }] {
        assert_eq!(e.order(order), Err(ErrorCode::OnCooldown), "a fleet that is leaving does nothing else");
    }
    let mark = e.mark();
    e.order(Command::Stop { fleet_id: fid }).expect("stop calls the jump off");
    let told = e.since(mark, |k| match k {
        EventKind::Alert(a) => Some(a.message.clone()),
        _ => None,
    });
    assert_eq!(
        told,
        vec![format!("fleet {fid} called off its jump to {}: the {cost} fuel it burned for the jump is forfeit, and it fights on at {at}", lane[1])],
    );
    assert_eq!((e.engine.fleets[&fid].state, fuel(&e)), (FleetStatus::InCombat, tank - cost));
    e.run(1);
    let fought_on = rounds_stood(&e, mark);
    assert!(fought_on.len() == 1 && !fought_on[0].1 && fought_on[0].2 > 0, "and it fires again: {fought_on:?}");

    assert_eq!(e.fleet().flight.expect("a quote").fuel, burn(&e), "a second flight is paid for again");
    let mark = jump(&mut e, lane[1]);
    assert_eq!(rounds_stood(&e, mark), vec![(at, true, 0)]);
    let ended = fights_ended(&e, mark);
    assert_eq!(ended.len(), 1);
    assert_eq!((ended[0].outcome, ended[0].salvage, ended[0].player_victory), (BattleOutcome::Fled, None, false));
    let fallen: u32 = ended[0].losses.iter().filter(|l| l.fleet_id == PACK).flat_map(|l| &l.lost).map(|u| u.count).sum();
    assert!(fallen > 0, "the fight had destroyed Heavy Fighters: {:?}", ended[0].losses);
    assert_eq!(e.engine.npc_fleets[&PACK].ships.total(), 200 - fallen, "which the group stays short of");
    assert!(
        e.engine.sector_overrides.get(&at.to_key()).is_none_or(|o| o.salvage.is_none()),
        "a group pays its wreckage when it is destroyed and not before",
    );
    assert!(pursuits(&e, mark).is_empty(), "a convoy follows nobody");
    let fleet = e.fleet();
    assert!(fleet.hull < fleet.hull_max && fleet.repair.is_some(), "the damage stays until a dock mends it");

    let (mut e, lane) = skirmish("persistent", &[(EspionageProbe, 40)], &[(LightFighter, 3)], Passive);
    let quote = e.fleet().flight.expect("a quote");
    assert_eq!((e.fleet().jump_ticks, quote.ticks, quote.rounds), (1, 2, 1), "the fastest fleet still takes one round");
    let mark = jump(&mut e, lane[1]);
    assert_eq!(rounds_stood(&e, mark).len(), 1);
}

#[test]
fn a_group_left_alive_keeps_its_losses_mends_a_step_at_a_time_and_pays_its_whole_wreckage_when_it_falls() {
    use ShipClass::{Cruiser, LightFighter};
    let (mut e, lane) = skirmish("season", &[(Cruiser, 3)], &[(LightFighter, 30)], Passive);
    let (pid, fid) = (e.pid, e.fid);
    let at = lane[0];
    jump(&mut e, lane[1]);
    let left = e.engine.npc_fleets[&PACK].ships.clone();
    let short = 30 - left.total();
    assert!((4..=27).contains(&short), "three Cruisers fire about six times each among Light Fighters: {short} fell");

    let pace = e.engine.pace();
    let step = scaling::npc_heal_step_ticks(at.dist_from_origin(), &pace);
    let now = e.engine.current_tick();
    let seen = |e: &Empire| iac_sim::intel::build_sector_state(&e.engine, at, Some(pid)).hostiles.map(|mut list| list.remove(0));
    let listed = seen(&e).expect("the group stands in its sector");
    assert_eq!((listed.id, listed.ship_count, listed.muster), (PACK, left.total(), 30), "it is listed as it stands");
    assert!(
        listed.whole_at.is_some_and(|tick| tick > now + step && tick <= now + 10 * step),
        "it is whole in ten steps of {step} ticks at most: {listed:?} at tick {now}",
    );
    assert_eq!(scaling::npc_heal_ships(30), 3, "a tenth of thirty ships come back at a step");

    e.run(step - 2);
    assert_eq!(e.engine.npc_fleets[&PACK].ships.total(), left.total(), "nothing comes back before the first step");
    e.run(1);
    assert_eq!(e.engine.npc_fleets[&PACK].ships.total(), left.total() + 3.min(short), "three ships at the first step");
    assert_eq!(seen(&e).expect("still there").whole_at, listed.whole_at, "and the tick it will be whole by has not moved");

    e.run(5);
    deploy(&mut e.engine, fid, at, None);
    empower(&mut e.engine, pid, fid);
    let found = e.engine.npc_fleets[&PACK].ships.total();
    let resting = seen(&e).expect("still there").whole_at.expect("it is wounded");
    let mark = e.mark();
    e.order(Command::Attack { fleet_id: fid, target_fleet_id: PACK }).expect("the fleet comes back for it");
    assert_eq!(
        seen(&e).expect("in the fight").whole_at,
        Some(resting + 5),
        "a fight stops the clock of its mending, and the five ticks it had rested toward the next step are lost",
    );
    e.run_until(40, |e| e.engine.active_combats.is_empty());
    let met: Vec<u32> = e.since(mark, |k| match k {
        EventKind::BattleRound(r) => Some(r.hostiles.groups[0].units.iter().map(|u| u.count + u.lost).sum()),
        _ => None,
    });
    assert_eq!(met.first(), Some(&found), "it is found with the losses it has not yet made good");
    let ended = fights_ended(&e, mark);
    assert_eq!(ended.last().map(|c| c.outcome), Some(BattleOutcome::HostilesDestroyed));
    let whole = Ships::of(&[(LightFighter, 30)]).power(scaling::Arms::of_grade(1.0));
    let pile = scaling::split_loot(scaling::wreck_value(whole)).scale(pace.find_mult());
    assert_eq!(ended.last().and_then(|c| c.salvage), Some(pile), "the wreckage of the thirty it was generated with");
    assert!(!e.engine.npc_fleets.contains_key(&PACK));
}

#[test]
fn a_lair_whose_group_has_mended_is_its_template_again_and_the_odds_count_it_whole_from_that_tick() {
    let mut e = Empire::join(world("season"), "Visitor");
    let (pid, fid) = (e.pid, e.fid);
    let at = lair(&e.engine, 7, |behavior| behavior == Passive);
    let id = template_npc_id(at);
    let template = e.engine.pending_template_npc(at).expect("the lair stands");
    let away = e.engine.world_gen.connected_neighbors(at).slice().iter().copied()
        .find(|n| e.engine.world_gen.generate_sector(*n).npc_template.is_none())
        .expect("a quiet sector beside the lair");
    deploy(&mut e.engine, fid, at, None);
    e.order(Command::Attack { fleet_id: fid, target_fleet_id: id }).expect("the fleet opens fire");
    e.run(1);
    jump(&mut e, away);

    let group = e.engine.npc_fleets.get(&id).expect("the lair's group keeps what the fight did to it");
    assert!(!group.whole() && group.location == at && group.errand.is_none());
    assert!(e.engine.pending_template_npc(at).is_none(), "the lair is that group, not a fresh one");
    let whole_at = e.engine.group_whole_at(group).expect("it mends");
    let remembered = chart(&e.engine, pid).remove(&at.to_key()).expect("the sector is charted");
    assert!(!remembered.live, "the fleet has left it");
    let seen = &remembered.hostiles.as_ref().expect("the group was seen")[0];
    assert_eq!((seen.id, seen.muster, seen.whole_at), (id, u32::from(template.count), Some(whole_at)));

    let odds = |e: &mut Empire| match e.engine.execute(pid, Command::Odds { fleet_id: fid, sectors: vec![at] }).reply {
        Some(iac_shared::protocol::ServerMessage::Odds(reply)) => reply.sectors[0].ratio,
        other => panic!("no odds: {other:?}"),
    };
    let against_wounded = odds(&mut e);
    e.run_until(100_000, |e| e.engine.current_tick() + 1 >= whole_at);
    assert!(e.engine.npc_fleets.contains_key(&id), "a tick before, it is still mending");
    assert_eq!(odds(&mut e), against_wounded, "and the odds are against what was seen");
    e.run(1);
    assert!(!e.engine.npc_fleets.contains_key(&id), "whole again, the group stands down");
    assert!(e.engine.pending_template_npc(at).is_some(), "and the lair is its template");
    assert!(odds(&mut e) <= against_wounded, "from that tick the odds count the remembered group whole");
    let fresh = iac_sim::intel::build_sector_state(&e.engine, at, Some(pid)).hostiles.expect("the lair is listed")[0].clone();
    assert_eq!((fresh.id, fresh.ship_count, fresh.muster, fresh.whole_at), (id, u32::from(template.count), u32::from(template.count), None));
}

#[test]
fn a_fleet_under_orders_leaves_a_fight_its_odds_have_turned_against_and_runs_home_saying_why() {
    use ShipClass::Cruiser;
    let stage = |disengage_ratio_x10: u8| {
        let mut e = Empire::join(world("persistent"), "Delegate");
        let (pid, fid) = (e.pid, e.fid);
        let home = e.engine.players[&pid].homeworld;
        let at = open_lane(&e.engine, home, 1)[0];
        muster(&mut e.engine, fid, &[(Cruiser, 20)], at);
        horde(&mut e.engine, PACK, at, &[(Cruiser, 30)], 1.0);
        edit(&mut e.engine, |s| s.npc_fleets.get_mut(&PACK).unwrap().behavior = NpcBehaviorType::Aggressive);
        let params = PolicyParams { max_range: 12, disengage_ratio_x10, ..PolicyParams::default() };
        e.engine.handle_policy_update(pid, fid, PolicyPreset::PatrolHome, Some(params)).expect("orders");
        e.order(Command::Attack { fleet_id: fid, target_fleet_id: PACK }).expect("its owner sends it in");
        (e, at, home)
    };

    let (mut e, at, home) = stage(9);
    let fid = e.fid;
    e.run_until(400, |e| e.engine.fleets.get(&fid).is_none_or(|f| f.location == home));
    let actions = e.since(0, |k| match k {
        EventKind::PolicyAction(p) => Some((p.action.clone(), p.reason.clone())),
        _ => None,
    });
    let fled: Vec<&str> = actions.iter().filter(|(action, _)| action == "flee").map(|(_, why)| why.as_str()).collect();
    assert_eq!(fled.len(), 1, "{actions:?}");
    assert!(
        fled[0].starts_with(&format!("left the fight at {at}: odds fell to 0."))
            && fled[0].contains("x, DEADLY, below the doctrine's 0.9x; jumping to ")
            && fled[0].ends_with(", under fire for 1 round"),
        "{}",
        fled[0],
    );
    assert!(
        actions.iter().any(|(action, why)| action == "move" && why.starts_with("running for home: the horde is on its trail and lands here in")),
        "{actions:?}",
    );
    let trail = pursuits(&e, 0);
    assert_eq!(trail.first().map(|p| (p.phase, p.sector, p.fleet_id)), Some((PursuitPhase::Began, at, fid)), "{trail:?}");
    assert!(trail.iter().any(|p| p.phase == PursuitPhase::Abandoned && p.why.is_some()), "the pack gives up, and says why: {trail:?}");
    let fleet = e.engine.fleets.get(&fid).expect("the fleet lives");
    assert!(fleet.location == home && !fleet.ships.is_empty() && !fleet.ships.has_damage(), "home, and mended at the dock");
    assert_eq!(rounds_stood(&e, 0).iter().filter(|round| !round.1).count(), 0, "it left before it had fired a round");

    let (mut e, _, _) = stage(1);
    let fid = e.fid;
    e.run_until(200, |e| e.engine.active_combats.is_empty());
    assert!(e.engine.fleets.get(&fid).is_none_or(|f| f.ships.is_empty()), "a doctrine told to stay at any odds stays");
    assert!(e.since(0, |k| matches!(k, EventKind::PolicyAction(p) if p.action == "flee").then_some(())).is_empty());

    let (e, _, _) = stage(80);
    assert_eq!(
        e.fleet().policy.map(|orders| (orders.params.engage_ratio_x10, orders.params.disengage_ratio_x10)),
        Some((15, 15)),
        "the disengage ratio is never above the engage ratio",
    );
}

fn settle(engine: &mut iac_sim::GameEngine, name: &str) -> Hex {
    let (pid, _) = join(engine, name);
    engine.players[&pid].homeworld
}

fn new_world(seed: u64) -> iac_sim::GameEngine {
    iac_sim::GameEngine::new(seed, Pace::parse("persistent").expect("known pace"))
}

#[test]
fn a_world_spreads_its_empires_over_the_seats_then_refuses_the_next_one_untouched() {
    let mut engine = world("persistent");
    let seats: Vec<Hex> = home_seats().collect();
    let founder = engine.authenticate("Founder", None, false, || "t".repeat(64)).expect("the first seat");
    let mut homes = vec![engine.players[&founder.player_id].homeworld];
    assert!(is_home_seat(homes[0]));
    while homes.len() < seats.len() {
        let widest = seats
            .iter()
            .filter(|seat| !homes.contains(seat))
            .map(|seat| homes.iter().map(|home| Hex::distance(home, seat)).min())
            .max()
            .unwrap();
        let home = settle(&mut engine, &format!("Settler{}", homes.len()));
        assert!(is_home_seat(home) && !homes.contains(&home), "{home} is not a free seat");
        assert_eq!(homes.iter().map(|h| Hex::distance(h, &home)).min(), widest, "a wider seat than {home} was free");
        homes.push(home);
    }

    let full = engine.snapshot().hash();
    let refused = engine.authenticate("Latecomer", None, false, || unreachable!());
    assert_eq!(refused.err(), Some(AuthError::WorldFull));
    let (code, text) = auth_failure("Latecomer", AuthError::WorldFull);
    assert_eq!(code, ErrorCode::WorldFull);
    assert!(text.contains("world is full") && text.contains(&seats.len().to_string()), "{text}");
    assert_eq!(engine.snapshot().hash(), full, "a refused join leaves the world as it was");
    let back = engine.authenticate("Founder", founder.new_token.as_deref(), false, || unreachable!());
    assert!(back.is_ok_and(|a| a.player_id == founder.player_id), "a full world still admits its own");

    let seating = |seed: u64| {
        let mut other = new_world(seed);
        let homes: Vec<Hex> = (0..seats.len()).map(|i| settle(&mut other, &format!("Settler{i}"))).collect();
        (homes, other)
    };
    assert_eq!(seating(SEED).0, homes, "a world replays its own seating");
    let (elsewhere, other) = seating(SEED + 1);
    assert_ne!(elsewhere, homes, "another world seats its empires in another order");
    let rerouted = hex_spiral(Hex::ORIGIN, 12)
        .filter(|at| engine.world_gen.sector_connections(*at) != other.world_gen.sector_connections(*at))
        .count();
    assert!(rerouted > 100, "another world has its own lanes: {rerouted} sectors differ");
}

#[test]
fn home_ground_is_quiet_and_an_unscouted_lair_is_as_strong_as_the_chart_says() {
    for seed in [SEED, SEED + 1, DEFAULT_WORLD_SEED] {
        let mut engine = new_world(seed);
        let (pid, _) = join(&mut engine, "Surveyor");
        for seat in home_seats() {
            assert!(!engine.world_gen.generate_sector(seat).has_derelict, "a derelict on seat {seat}");
            for at in std::iter::once(seat).chain(seat.neighbors()) {
                let pack = engine.world_gen.generate_sector(at).npc_template;
                assert!(pack.is_none_or(|t| t.behavior == Passive), "a hostile pack at {at}, on or beside seat {seat}");
            }
        }

        let mut lairs = 0;
        for at in hex_spiral(Hex::ORIGIN, 40) {
            let Some(pack) = engine.world_gen.generate_sector(at).npc_template else { continue };
            let told = engine.known_threat(pid, at);
            if told.basis != ThreatBasis::Estimate {
                continue;
            }
            lairs += 1;
            assert!(
                (told.est_power - pack.power()).abs() <= 1e-3 * pack.power(),
                "{at}: the chart says {} and a {} of {} waits",
                told.est_power,
                pack.label(),
                pack.power()
            );
            let inner = Zone::from_distance(at.dist_from_origin()) == Zone::InnerRing;
            assert!(!inner || pack.ship_class == ShipClass::LightFighter, "a {} in the Inner Ring at {at}", pack.label());
        }
        assert!(lairs > 1000, "the gradient was walked: {lairs} lairs");
    }
}

#[test]
fn a_starter_fleet_wins_through_ring_seven_gambles_at_ring_eight_and_seldom_lives_past_the_inner_ring() {
    use RatioLabel::{Deadly, Even, Favourable, Risky, Safe};
    let rings = [
        (5, Favourable, Safe, 60..=60),
        (7, Even, Even, 50..=60),
        (8, Risky, Risky, 25..=55),
        (9, Risky, Risky, 2..=12),
        (10, Deadly, Deadly, 0..=3),
    ];
    for (ring, least, most, wins) in rings {
        let mut engine = world("persistent");
        let (pid, fid) = join(&mut engine, "Rookie");
        let at = lair(&engine, ring, |_| true);
        let unscouted = engine.fleet_odds(&engine.fleets[&fid], at).label;
        deploy(&mut engine, fid, at, None);
        let odds = engine.fleet_odds(&engine.fleets[&fid], at).label;
        assert_eq!(unscouted, odds, "ring {ring}: the estimate and the sighting agree");
        assert!((least..=most).contains(&odds), "ring {ring} was rated {}", odds.label());
        let Fights { fought, won, hull_kept } = fights(&engine, pid, fid, at, 60);
        assert!(wins.contains(&won), "ring {ring}, rated {}: won {won} of {fought}, kept {hull_kept:.2} of the hull", odds.label());
    }
}

#[test]
fn a_label_tells_the_outcome_whatever_the_classes() {
    use RatioLabel::{Deadly, Even, Favourable, Safe};
    use ShipClass::{Battleship, Cruiser, HeavyFighter, LightFighter};
    let cases = [
        (LightFighter, 2, 10, Deadly),
        (LightFighter, 4, 10, Favourable),
        (LightFighter, 8, 10, Safe),
        (HeavyFighter, 1, 13, Deadly),
        (HeavyFighter, 3, 13, Favourable),
        (HeavyFighter, 8, 13, Safe),
        (Cruiser, 1, 11, Safe),
        (Cruiser, 1, 17, Deadly),
        (Cruiser, 2, 17, Even),
        (Cruiser, 3, 17, Favourable),
        (Cruiser, 1, 15, Even),
        (Cruiser, 2, 15, Safe),
        (Battleship, 1, 20, Deadly),
        (Battleship, 12, 20, Safe),
    ];
    for (class, count, ring, told) in cases {
        let mut engine = world("persistent");
        let (pid, fid) = join(&mut engine, "Gauge");
        let home = engine.players[&pid].homeworld;
        let at = lair(&engine, ring, |_| true);
        muster(&mut engine, fid, &[(class, count)], home);
        let unscouted = engine.fleet_odds(&engine.fleets[&fid], at);
        assert_eq!(unscouted.threat.basis, ThreatBasis::Estimate);
        muster(&mut engine, fid, &[(class, count)], at);
        let seen = engine.fleet_odds(&engine.fleets[&fid], at);
        assert_eq!(seen.threat.basis, ThreatBasis::Observed);
        let what = format!("{count} {} at ring {ring}, rated {:.2}x {}", class.label(), seen.ratio, seen.label.label());
        assert_eq!((unscouted.label, seen.label), (told, told), "{what}");

        let Fights { fought, won, hull_kept } = fights(&engine, pid, fid, at, 60);
        // Both sides fire at once, so no label is free: what each means was
        // measured over 492 pairings of five classes and thirteen rings.
        let (wins, kept) = match told {
            Safe => (fought * 19 / 20..=fought, 0.84..=1.0),
            Favourable => (fought * 19 / 20..=fought, 0.60..=0.93),
            Even => (fought * 7 / 10..=fought, 0.33..=0.92),
            _ => (0..=fought * 9 / 20, 0.0..=0.22),
        };
        assert!(wins.contains(&won) && kept.contains(&hull_kept), "{what}: won {won} of {fought}, kept {hull_kept:.2} of the hull");
    }
}

#[test]
fn a_default_patrol_holds_where_power_alone_said_engage_and_fights_what_it_beats() {
    let convoy = |scouts: u32| {
        let mut engine = world("persistent");
        let (pid, fid) = join(&mut engine, "Warden");
        let home = engine.players[&pid].homeworld;
        let at = open_lane(&engine, home, 3)[1];
        horde(&mut engine, template_npc_id(at), at, &[(ShipClass::Cruiser, 1)], 0.29);
        muster(&mut engine, fid, &[(ShipClass::LightFighter, scouts)], at);
        (engine, pid, fid, at)
    };

    let (mut engine, pid, fid, at) = convoy(4);
    let by_power = iac_sim::engine::fleet_power(&engine.fleets[&fid]) / engine.sector_threat(at).est_power;
    assert!(by_power * 10.0 >= f32::from(PolicyParams::default().engage_ratio_x10), "power alone rates it {by_power:.2}x");
    let Fights { fought, won, hull_kept } = fights(&engine, pid, fid, at, 60);
    assert!(won * 3 <= fought * 2 && hull_kept < 0.4, "won {won} of {fought}, kept {hull_kept:.2} of the hull");
    engine.handle_policy_update(pid, fid, PolicyPreset::PatrolHome, None).unwrap();
    let orders = policy_actions(&run(&mut engine, 3));
    assert!(orders.iter().all(|(action, _)| action != "attack"), "{orders:?}");
    assert!(orders.iter().any(|(action, why)| action == "move" && why.contains("too strong")), "it withdraws: {orders:?}");
    assert_eq!(engine.fleets[&fid].ships.total(), 4);

    let (mut engine, pid, fid, at) = convoy(20);
    let Fights { fought, won, hull_kept } = fights(&engine, pid, fid, at, 60);
    assert!(won == fought && hull_kept > 0.84, "won {won} of {fought}, kept {hull_kept:.2} of the hull");
    engine.handle_policy_update(pid, fid, PolicyPreset::PatrolHome, None).unwrap();
    let orders = policy_actions(&run(&mut engine, 30));
    assert!(orders.iter().any(|(action, _)| action == "attack"), "{orders:?}");
}

#[test]
fn a_stockpile_of_millions_grows_by_what_its_mines_make_and_is_stored_to_the_last_digit() {
    let mut engine = world("persistent");
    let (pid, _) = join(&mut engine, "Hoarder");
    let stock = Resources { metal: 5_000_000.0, crystal: 2_500_000.0, deuterium: 1_250_000.0 };
    edit(&mut engine, |s| {
        let b = &mut s.players.get_mut(&pid).unwrap().buildings;
        (b.metal_storage, b.crystal_storage, b.deuterium_tank) = (10, 10, 10);
        (b.metal_mine, b.crystal_mine, b.deuterium_synthesizer, b.solar_plant) = (10, 10, 10, 20);
        s.players.get_mut(&pid).unwrap().resources = stock;
    });
    let pace = engine.pace();
    let made = engine.players[&pid].production_per_tick(&pace);
    let cap = scaling::storage_cap(&engine.players[&pid].buildings, &pace);
    for kind in ResourceKind::ALL {
        let (held, tick) = (stock.get(kind) as f32, made.get(kind) as f32);
        let added = held + tick - held;
        assert!(
            tick > 0.0 && (added - tick).abs() > 0.1 * tick,
            "a 32-bit float adds {added} for a tick of {tick} {} here",
            kind.label()
        );
        assert!(stock.get(kind) + 3600.0 * made.get(kind) < cap.get(kind), "the store has room for the hour");
    }

    run(&mut engine, 3600);
    let grown = engine.players[&pid].resources.sub(stock);
    for kind in ResourceKind::ALL {
        let hour = 3600.0 * made.get(kind);
        assert!((grown.get(kind) - hour).abs() < 1e-5, "{}: an hour of mining is {hour}, the stockpile grew by {}", kind.label(), grown.get(kind));
    }

    let odd = 2_950_448.335_970_960_6;
    edit(&mut engine, |s| s.players.get_mut(&pid).unwrap().resources.metal = odd);
    let kept = stored(&engine);
    assert_eq!(kept.players[&pid].resources.metal.to_bits(), odd.to_bits(), "a document gives back the amount it was written with");
    assert_eq!(kept.hash(), engine.snapshot().hash());
}
