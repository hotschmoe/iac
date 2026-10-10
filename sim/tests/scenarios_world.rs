// Scenarios that run the engine over time against the world around a
// homeworld: wreckage and what the chart remembers of it, doctrines that
// roam charted space, ore tiles that empty and refill, fights that end some
// other way than one side dying, and where a world seats its empires and what
// it keeps away from them.

#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::constants::{
    Density, Resources, ShipClass, Zone, DEFAULT_WORLD_SEED, HOME_SALVAGE_WARN_TICKS, SALVAGE_DESPAWN_TICKS,
    SCAN_REVEAL_TICKS,
};
use iac_shared::hex::{hex_ring, hex_spiral, Hex};
use iac_shared::pace::Pace;
use iac_shared::protocol::{Command, ErrorCode, EventKind, HarvestResource, PolicyParams, PolicyPreset, ThreatBasis};
use iac_shared::scaling::{self, RatioLabel};
use iac_shared::world::NpcBehaviorType::Passive;
use iac_shared::world::{home_seats, is_home_seat};
use iac_sim::commands::auth_failure;
use iac_sim::engine::{template_npc_id, AuthError, FleetStatus, SectorOverride};
use iac_sim::persist::ExploreCredit;

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
                ship.shield_max = 0.0;
            }
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
fn a_prospector_goes_back_for_the_chart_a_lost_fleet_never_brought_home() {
    let (mut engine, pid, fid, home) = prospector("Second", 5000.0, 2);
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

/// A fleet that outguns everything near home, so only the doctrine decides
/// where it goes, with its fuel left unbounded.
fn delegate(name: &str, preset: PolicyPreset, max_range: u8) -> (iac_sim::GameEngine, u64, u64, Hex) {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, name);
    let home = engine.players[&pid].homeworld;
    edit(&mut engine, |s| {
        let player = s.players.get_mut(&pid).unwrap();
        player.buildings.storage_vault = 8;
        player.resources.deuterium = 5_000.0;
        let fleet = s.fleets.get_mut(&fid).unwrap();
        for ship in &mut fleet.ships[..fleet.ship_count] {
            ship.weapon_power = 5000.0;
        }
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
    let in_reach: f32 = ring_around(home, 3)
        .into_iter()
        .filter_map(|h| engine.ore_reserve_at(h))
        .map(|o| o.metal.units + o.crystal.units + o.deuterium.units)
        .sum();
    assert!(in_reach > 100.0, "the seed puts ore within three hexes of home: {in_reach}");
    let events = run(&mut engine, 6000);
    let mined: f32 = events
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
        .expect("a quiet sector the starting scouts may not enter");
    let (within, _) = *reachable
        .iter()
        .find(|(h, hops)| *hops > shut_hops && quiet(&engine, *h))
        .expect("a quiet sector farther out that they may");
    let pile = Resources { metal: 30.0, crystal: 0.0, deuterium: 0.0 };
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
    assert_eq!(engine.fleets[&fid].ship_count, 2);
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
    let at = lone_scout_sector(&engine, Passive);
    let scout = engine.fleets[&fid].ships[0].id;
    let escort = engine.handle_split(pid, fid, &[scout]).unwrap();
    deploy(&mut engine, fid, at, None);
    deploy(&mut engine, escort, at, Some(5000.0));
    let onward = engine.world_gen.connected_neighbors(at).slice()[0];

    assert!(engine.execute(pid, Command::Move { fleet_id: fid, target: onward }).error.is_none());
    assert_eq!(engine.fleets[&fid].state, FleetStatus::Moving);
    let attack = engine.execute(pid, Command::Attack { fleet_id: escort, target_fleet_id: template_npc_id(at) });
    assert!(attack.error.is_none(), "{:?}", attack.error);
    assert_eq!(engine.fleets[&fid].state, FleetStatus::InCombat, "a fleet still charging its jump is in the sector's fight");
    run_until(&mut engine, 10, |e| e.active_combats.is_empty());
    assert_eq!((engine.fleets[&fid].location, engine.fleets[&fid].state), (at, FleetStatus::Idle), "the jump is lost");
    let flown = orders_flown(&mut engine, pid, fid);
    assert!(!flown.is_empty(), "standing orders never ran after the fight");

    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, "Recaller");
    let home = engine.players[&pid].homeworld;
    let out = engine.world_gen.connected_neighbors(home).slice()[0];
    assert!(engine.execute(pid, Command::Move { fleet_id: fid, target: out }).error.is_none());
    assert!(engine.execute(pid, Command::Recall { fleet_id: fid }).error.is_none());
    assert_eq!((engine.fleets[&fid].location, engine.fleets[&fid].state), (home, FleetStatus::Idle));
    let flown = orders_flown(&mut engine, pid, fid);
    assert!(!flown.is_empty(), "standing orders never ran after the recall");
}

/// A fleet and a hostile that cannot hurt each other enough to end it:
/// the fight lasts until something else stops it.
fn endless_fight(engine: &mut iac_sim::GameEngine, pid: u64, fid: u64, at: Hex) {
    deploy(engine, fid, at, Some(0.0));
    edit(engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
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
    let at = lone_scout_sector(&engine, Passive);
    endless_fight(&mut engine, pid, fid, at);
    let stock = engine.players[&pid].resources.metal;

    assert!(engine.execute(pid, Command::Recall { fleet_id: fid }).error.is_none());
    let hulls = |e: &iac_sim::GameEngine| e.fleets[&fid].ships[..e.fleets[&fid].ship_count].iter().map(|s| s.hull).collect::<Vec<_>>();
    let on_arrival = hulls(&engine);
    let after = run(&mut engine, 5);

    assert_eq!((engine.fleets[&fid].location, engine.fleets[&fid].state), (home, FleetStatus::Idle));
    assert!(engine.players[&pid].resources.metal >= stock + 40.0, "the cargo is banked");
    assert!(engine.active_combats.is_empty(), "the fight went on without the fleet in the sector");
    let shots = after
        .iter()
        .filter(|e| matches!(&e.kind, EventKind::CombatRound(r) if r.target_fleet_id == fid || r.attacker_fleet_id == fid))
        .count();
    assert_eq!(shots, 0, "a fleet at home was still trading fire");
    assert_eq!(hulls(&engine), on_arrival, "a fleet at home was still taking damage");
    assert_eq!(ended_without_victory(&after, at), 1, "the owner is told the fight is over");

    let owner = engine.players[&pid].clone();
    let told = after.iter().filter_map(|e| iac_sim::views::event_for(e, &owner, &engine)).collect::<Vec<_>>();
    assert_eq!(ended_without_victory(&told, at), 1, "and the event reaches them at home");

    pile_of(&fight(&mut engine, pid, fid, at));
}

#[test]
fn a_fight_nobody_is_left_in_frees_its_hostiles_for_the_next_fleet() {
    let mut engine = world("persistent");
    let (pid, fid) = join(&mut engine, "Vanished");
    let (next, next_fleet) = join(&mut engine, "Next");
    let at = lone_scout_sector(&engine, Passive);
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
    let at = lone_scout_sector(&engine, Passive);
    for &(_, fid) in &crews {
        deploy(&mut engine, fid, at, Some(5000.0));
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
            assert!(!inner || pack.ship_class == ShipClass::Scout, "a {} in the Inner Ring at {at}", pack.label());
        }
        assert!(lairs > 1000, "the gradient was walked: {lairs} lairs");
    }
}

#[test]
fn a_starter_fleet_wins_through_the_inner_ring_and_dies_past_it() {
    for (ring, survives) in [(7, true), (8, true), (9, false), (10, false)] {
        let mut engine = world("persistent");
        let (pid, fid) = join(&mut engine, "Rookie");
        let at = hex_ring(Hex::ORIGIN, ring)
            .find(|at| engine.world_gen.generate_sector(*at).npc_template.is_some_and(|t| t.behavior != Passive))
            .expect("a hostile lair in the ring");
        let unscouted = engine.fleet_odds(&engine.fleets[&fid], at).label;
        deploy(&mut engine, fid, at, None);
        let odds = engine.fleet_odds(&engine.fleets[&fid], at).label;
        assert_eq!(unscouted, odds, "ring {ring}: the estimate and the sighting agree");
        let outcome = engine.execute(pid, Command::Attack { fleet_id: fid, target_fleet_id: template_npc_id(at) });
        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        run_until(&mut engine, 2000, |e| e.active_combats.is_empty());
        let alive = engine.fleets.get(&fid).is_some_and(|f| f.ship_count > 0);
        assert_eq!(alive, survives, "ring {ring}, rated {}", odds.label());
        assert_eq!(odds > RatioLabel::Deadly, survives, "ring {ring} was rated {}", odds.label());
    }
}

#[test]
fn a_label_tells_the_outcome_whatever_the_classes() {
    use RatioLabel::{Deadly, Even, Favourable, Safe};
    use ShipClass::{Corvette, Frigate, Scout};
    let cases = [
        (Scout, 3, 10, Deadly),
        (Scout, 6, 9, Even),
        (Scout, 10, 9, Favourable),
        (Corvette, 1, 7, Safe),
        (Corvette, 5, 13, Even),
        (Corvette, 8, 13, Favourable),
        (Frigate, 1, 11, Safe),
        (Frigate, 3, 15, Safe),
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
        let (wins, kept) = match told {
            Safe => (fought..=fought, 0.97..=1.0),
            Favourable => (fought..=fought, 0.84..=0.995),
            Even => (fought * 9 / 10..=fought, 0.45..=0.88),
            _ => (0..=fought / 5, 0.0..=0.1),
        };
        assert!(wins.contains(&won) && kept.contains(&hull_kept), "{what}: won {won} of {fought}, kept {hull_kept:.2} of the hull");
    }
}

#[test]
fn a_default_patrol_holds_where_power_alone_said_engage_and_fights_what_it_beats() {
    let convoy = |scouts: usize| {
        let mut engine = world("persistent");
        let (pid, fid) = join(&mut engine, "Warden");
        let at = lair(&engine, 10, |b| b == Passive);
        muster(&mut engine, fid, &[(ShipClass::Scout, scouts)], at);
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
    assert_eq!(engine.fleets[&fid].ship_count, 4);

    let (mut engine, pid, fid, at) = convoy(8);
    let Fights { fought, won, hull_kept } = fights(&engine, pid, fid, at, 60);
    assert!(won == fought && hull_kept > 0.84, "won {won} of {fought}, kept {hull_kept:.2} of the hull");
    engine.handle_policy_update(pid, fid, PolicyPreset::PatrolHome, None).unwrap();
    let orders = policy_actions(&run(&mut engine, 30));
    assert!(orders.iter().any(|(action, _)| action == "attack"), "{orders:?}");
}
