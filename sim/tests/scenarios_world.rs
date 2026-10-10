// Scenarios that run the engine over time against the world around a
// homeworld: wreckage and what the chart remembers of it, doctrines that
// roam charted space, and ore tiles that empty and refill.

#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::constants::{
    Density, Resources, ShipClass, HOME_SALVAGE_WARN_TICKS, SALVAGE_DESPAWN_TICKS, SCAN_REVEAL_TICKS,
};
use iac_shared::hex::Hex;
use iac_shared::protocol::{Command, EventKind, HarvestResource, PolicyParams, PolicyPreset};
use iac_shared::scaling;
use iac_shared::world::NpcBehaviorType::Passive;
use iac_sim::engine::SectorOverride;

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
    let at = lone_scout_sector(&engine, Passive);
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
fn the_chart_remembers_a_pile_as_seen_until_its_own_timer_runs_out() {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, "Chartist");
    let at = lone_scout_sector(&engine, Passive);
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
    assert!(events.iter().any(|e| matches!(&e.kind, EventKind::SalvageCollected(c) if c.resources == small && c.remaining.is_none())));

    let cap = scaling::storage_cap(p.buildings.storage_vault, &engine.pace());
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

fn prospector(name: &str, weapon_power: f32, max_range: u8) -> (iac_sim::GameEngine, u64, u64, Hex) {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, name);
    let home = engine.players[&pid].homeworld;
    edit(&mut engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        for ship in &mut fleet.ships[..fleet.ship_count] {
            ship.weapon_power = weapon_power;
            if weapon_power < 1.0 {
                ship.hull = 1.0;
                ship.shield = 0.0;
            }
        }
        for h in ring_around(home, 2) {
            s.explored.insert((pid, h.to_key()));
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
    let (mut engine, _pid, fid, home) = prospector("Weakling", 0.1, 4);
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
    let (mut engine, pid, _fid, home) = prospector("Bruiser", 5000.0, 4);
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
            s.players.get_mut(&pid).unwrap().research.harvesting_efficiency = level;
            let fleet = s.fleets.get_mut(&fid).unwrap();
            fleet.location = at;
            for ship in &mut fleet.ships[..fleet.ship_count] {
                ship.ship_class = ShipClass::Hauler;
            }
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
    let (mut engine, _pid, fid, home) = prospector("Boxed", 5000.0, 2);
    let events = run(&mut engine, 200);
    let actions = policy_actions(&events);
    assert!(actions.iter().all(|(a, _)| a != "move"), "everything in range is charted: nothing to move to");
    let holds: Vec<_> = actions.iter().filter(|(a, _)| a == "hold").collect();
    assert!(!holds.is_empty(), "the hold must be announced");
    assert!(holds[0].1.contains("within 2 hops of home"), "{}", holds[0].1);
    assert!(holds.len() <= 4, "the same hold must not flood events: {}", holds.len());
    assert_eq!(engine.fleets[&fid].location, home);
}

#[test]
fn a_standing_hold_is_reported_once_a_minute_not_every_evaluation() {
    let (mut engine, ..) = prospector("Patient", 0.1, 4);
    let holds = |events: Vec<iac_shared::protocol::GameEvent>| {
        policy_actions(&events).into_iter().filter(|(a, _)| a == "hold").collect::<Vec<_>>()
    };
    let first_minute = holds(run(&mut engine, 55));
    assert_eq!(first_minute.len(), 1, "one report, not one per evaluation: {first_minute:?}");
    let later = holds(run(&mut engine, 125));
    assert!(later.len() <= 3, "at most one a minute afterwards: {later:?}");
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
