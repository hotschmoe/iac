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
    AlertLevel, BattleOutcome, BattleRoundEvent, BattleSide, CombatEndedEvent, Command, ErrorCode, EventKind,
    HarvestResource, PolicyParams, PolicyPreset, ThreatBasis, Volley,
};
use iac_shared::scaling::{self, RatioLabel, ShipyardItem};
use iac_shared::world::NpcBehaviorType::Passive;
use iac_shared::world::{home_seats, is_home_seat};
use iac_sim::commands::auth_failure;
use iac_sim::engine::{template_npc_id, AuthError, FleetStatus, SectorOverride, Ships};
use iac_sim::snapshot::ExploreCredit;

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
    assert!(events.iter().any(|e| matches!(&e.kind, EventKind::SalvageCollected(c) if c.resources == small && c.remaining.is_none())));

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
            s.credited.insert((pid, h.to_key(), ExploreCredit::Charted));
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
    assert!(holds[0].1.contains("within 2 hops of home"), "{}", holds[0].1);
    assert!(holds.len() <= 4, "the same hold must not flood events: {}", holds.len());
    assert_eq!(engine.fleets[&fid].location, home);
}

#[test]
fn a_prospector_goes_back_for_the_chart_a_lost_fleet_never_brought_home() {
    let (mut engine, pid, fid, home) = prospector("Second", true, 2);
    let entered = engine.world_gen.connected_neighbors(home).slice()[0];
    let chart = (pid, entered.to_key(), ExploreCredit::Charted);
    edit(&mut engine, |s| {
        s.players.get_mut(&pid).unwrap().resources.deuterium = 5_000.0;
        assert!(s.explored.contains(&(pid, entered.to_key())));
        s.credited.remove(&chart);
    });

    let events = run_until(&mut engine, 300, |e| e.snapshot().credited.contains(&chart));
    assert!(policy_actions(&events).iter().any(|(a, why)| a == "move" && why == "pushing frontier"));
    assert_eq!(engine.fleets[&fid].location, home, "the chart is banked at the dock");
}

#[test]
fn a_standing_hold_is_reported_once_a_minute_not_every_evaluation() {
    let (mut engine, ..) = prospector("Patient", false, 4);
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
    assert!(holds(&idle).iter().any(|why| why.contains("repays the fuel")), "{:?}", holds(&idle));
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
    assert_eq!(
        acts[0],
        (
            "move".to_string(),
            "returning to be mended: hull at 81% of full, and a fleet under orders works at 85% or more".to_string(),
        ),
        "its first act under orders, and it scans nothing on the way: {acts:?}",
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
fn a_patrol_hunts_out_to_its_range_scoops_what_its_kills_leave_and_waits_at_the_dock() {
    let (mut engine, pid, fid, home) = delegate("Hunter", PolicyPreset::PatrolHome, 4);
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
    assert!(actions.iter().any(|(a, _)| a == "salvage"), "kills leave wreckage and the patrol takes it");
    let wandering: Vec<&str> = moves(&actions)
        .into_iter()
        .filter(|why| !why.starts_with("intercepting") && !why.starts_with("returning"))
        .collect();
    assert!(wandering.is_empty(), "every jump has a purpose: {wandering:?}");
    assert!(engine.players[&pid].combat_points > 0.0);
    assert!(
        holds(&actions).iter().any(|why| why.starts_with("no contact within 4 hops of home")),
        "with the beat cleared it waits at the dock: {:?}",
        holds(&actions),
    );
}

fn orders_flown(engine: &mut iac_sim::GameEngine, pid: u64, fid: u64) -> Vec<(String, String)> {
    engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, None).unwrap();
    engine.drain_events();
    policy_actions(&run(engine, 40))
}

#[test]
fn a_jump_cut_short_by_a_fight_or_a_recall_leaves_the_fleet_free_for_standing_orders() {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, "Jumper");
    let at = lone_fighter_sector(&engine, Passive);
    let escort = engine.handle_split(pid, fid, &counts(&[(ShipClass::LightFighter, 1)]), false).unwrap();
    deploy(&mut engine, fid, at, None);
    deploy(&mut engine, escort, at, Some(MIGHTY));
    let onward = engine.world_gen.connected_neighbors(at).slice()[0];

    let tank = engine.fleets[&fid].fuel;
    assert!(engine.execute(pid, Command::Move { fleet_id: fid, target: onward }).error.is_none());
    assert_eq!(engine.fleets[&fid].state, FleetStatus::Moving);
    assert_eq!(tank - engine.fleets[&fid].fuel, 20.0, "one Light Fighter burns 20 fuel a jump, paid when it is ordered");
    let attack = engine.execute(pid, Command::Attack { fleet_id: escort, target_fleet_id: template_npc_id(at) });
    assert!(attack.error.is_none(), "{:?}", attack.error);
    assert_eq!(engine.fleets[&fid].state, FleetStatus::InCombat, "a fleet still charging its jump is in the sector's fight");
    let fought = run_until(&mut engine, 10, |e| e.active_combats.is_empty());
    assert_eq!((engine.fleets[&fid].location, engine.fleets[&fid].state), (at, FleetStatus::Idle), "the jump is lost");
    assert_eq!(engine.fleets[&fid].fuel, tank - 20.0, "and so is the fuel it burned");
    let caught: Vec<_> = fought
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::Alert(a) if a.message.contains("was caught in a fight") => Some(a),
            _ => None,
        })
        .collect();
    assert_eq!(caught.len(), 1, "the fleet that was charging is told once, the fleet that picked the fight is not: {caught:?}");
    assert_eq!(
        caught[0].message,
        format!(
            "fleet {fid} was caught in a fight at {at} while charging its jump to {onward}: \
             the jump is abandoned, and the 20 fuel it burned for the jump is forfeit"
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
fn every_fleet_in_a_crowded_sector_fights_scores_and_comes_out_free() {
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
        assert!(engine.players[&pid].combat_points > 0.0, "{name} fought and was not credited");
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

fn fire<'a>(side: &'a BattleSide, from: ShipClass, at: ShipClass) -> Option<&'a Volley> {
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
    use ShipClass::{Battleship, EspionageProbe, Recycler};
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
fn a_starter_fleet_wins_through_ring_seven_gambles_at_ring_eight_and_dies_past_the_inner_ring() {
    use RatioLabel::{Deadly, Even, Favourable, Risky, Safe};
    let rings = [
        (5, Favourable, Safe, 60..=60),
        (7, Even, Even, 50..=60),
        (8, Risky, Risky, 25..=55),
        (9, Deadly, Deadly, 0..=3),
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
        (LightFighter, 3, 10, Deadly),
        (LightFighter, 8, 10, Even),
        (LightFighter, 16, 10, Favourable),
        (LightFighter, 40, 10, Safe),
        (HeavyFighter, 2, 13, Deadly),
        (HeavyFighter, 8, 13, Favourable),
        (HeavyFighter, 20, 13, Safe),
        (Cruiser, 1, 11, Safe),
        (Cruiser, 4, 17, Even),
        (Cruiser, 4, 15, Favourable),
        (Cruiser, 8, 15, Safe),
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
        let at = lair(&engine, 9, |b| b == Passive);
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
