// A measuring tool, not a verdict: one fleet of one class, research at one
// level, and the farthest ring from a ring-4 home it can fly to and come back
// from, with and without the deuterium clouds and the relays as places to
// refuel. The plan is worked out from the lanes and the deposits (a cloud
// sector pays half its deuterium on the way out and half on the way back), and
// then flown in the engine, harvest, refine and every jump, to see that the
// fleet does come home.
//
// The relays' model: the fleet founds up to the limit its Hyperspace
// Technology allows (the level of the row) on open ground of ring 9 or deeper,
// not in a cloud, paying the founding cost from a hold the probe tops up for
// the purpose, and a convoy that is not counted has stocked each with its whole
// store: half is spent on the way out and half on the way back, as a cloud's.
//
// The planner is a greedy search, so a ring it reports is a lower bound; the
// fight-reach tables walk only lanes into sectors whose group is at most so
// many times the fleet's own power, and a looser guard is never reported
// below a tighter one nor below the unguarded plan.

#[allow(dead_code)]
mod common;

use std::collections::{BTreeMap, VecDeque};

use common::*;
use iac_shared::constants::{Density, ShipClass, TerrainType, FUEL_DEUT_PER_UNIT, RELAY_MIN_RING, RELAY_STORE};
use iac_shared::pace::Pace;
use iac_shared::hex::Hex;
use iac_shared::protocol::{Command, HarvestResource};
use iac_shared::scaling::{refined_fuel_per_deuterium, ship_tank, tank_modifier, Arms, Refit, NPC_MAX_SHIPS};
use iac_shared::world;
use iac_sim::engine::{FleetStatus, Ships};
use iac_sim::GameEngine;

const FLEET_SHIPS: u32 = 10;
const SEED: u64 = 42;
const SEEDS: [u64; 8] = [42, 7, 101, 2024, 31337, 555, 9001, 1234];
const LEVELS: [u8; 5] = [0, 3, 6, 10, 13];
const CLASSES: [ShipClass; 2] = [ShipClass::LightFighter, ShipClass::Cruiser];
const FIGHT_MULTIPLES: [f32; 3] = [1.0, 2.0, 4.0];
const CENSUS_HOPS: u32 = 60;
const WORKERS: usize = 4;
const HOME_RING: u16 = 4;
const SEARCH_HOPS: u32 = 120;
const MAP_LIMIT: u16 = 40;
const TARGET_TRIES: usize = 8;
const MAX_STOPS: usize = 8;
const RELAY_TRIES: usize = 3;

#[derive(Clone, Copy)]
struct Tier {
    clouds: bool,
    relays: bool,
}

const OFF: Tier = Tier { clouds: false, relays: false };
const CLOUDS: Tier = Tier { clouds: true, relays: false };
const RELAYS: Tier = Tier { clouds: false, relays: true };
const BOTH: Tier = Tier { clouds: true, relays: true };

struct Walk {
    hops: u32,
    parent: Option<Hex>,
}

fn lair_power(engine: &GameEngine, at: Hex) -> f32 {
    engine.world_gen.generate_sector(at).npc_template.map_or(0.0, |npc| {
        let count = u32::from(npc.count).min(NPC_MAX_SHIPS as u32);
        Ships::of(&[(npc.ship_class, count)]).power(Arms::of_grade(npc.stat_multiplier))
    })
}

fn walk_from(engine: &GameEngine, origin: Hex, depth: u32, limit: Option<f32>) -> BTreeMap<u32, Walk> {
    let mut seen = BTreeMap::new();
    seen.insert(origin.to_key(), Walk { hops: 0, parent: None });
    let mut queue = VecDeque::from([origin]);
    while let Some(at) = queue.pop_front() {
        let hops = seen[&at.to_key()].hops;
        if hops == depth {
            continue;
        }
        for &next in engine.world_gen.connected_neighbors(at).slice() {
            if seen.contains_key(&next.to_key()) || limit.is_some_and(|cap| lair_power(engine, next) > cap) {
                continue;
            }
            seen.insert(next.to_key(), Walk { hops: hops + 1, parent: Some(at) });
            queue.push_back(next);
        }
    }
    seen
}

fn path_between(engine: &GameEngine, from: Hex, to: Hex, limit: Option<f32>) -> Vec<Hex> {
    let walk = walk_from(engine, from, SEARCH_HOPS, limit);
    let mut path = Vec::new();
    let mut at = to;
    while at != from {
        path.push(at);
        at = walk[&at.to_key()].parent.expect("a lane back to the origin");
    }
    path.reverse();
    path
}

fn is_cloud(engine: &GameEngine, at: Hex) -> bool {
    engine.world_gen.cloud_depth(at).is_some()
}

/// Half of what a cloud sector's deuterium refines to, in jumps.
fn half_deposit(engine: &GameEngine, at: Hex, jump_fuel: f64) -> f64 {
    if !is_cloud(engine, at) {
        return 0.0;
    }
    let fuel = engine.world_gen.generate_sector(at).ore_units[2] * refined_fuel_per_deuterium();
    fuel / jump_fuel / 2.0
}

struct Plan {
    nodes: Vec<Hex>,
    relay: Vec<bool>,
    target: Hex,
    ring: u16,
    out_fuel: Vec<f64>,
    back_fuel: Vec<f64>,
}

/// Fuel, in jumps, the fleet holds when it leaves each node outward and
/// homeward, or None when the round trip cannot be flown.
fn simulate(legs: &[u32], last: u32, gains: &[f64], jumps: f64) -> Option<(Vec<f64>, Vec<f64>)> {
    let mut fuel = jumps;
    let mut out = vec![jumps];
    for (i, &leg) in legs.iter().enumerate() {
        if fuel < f64::from(leg) {
            return None;
        }
        fuel = (fuel - f64::from(leg) + gains[i + 1]).min(jumps);
        out.push(fuel);
    }
    if fuel < 2.0 * f64::from(last) {
        return None;
    }
    fuel = (fuel - 2.0 * f64::from(last) + gains[legs.len()]).min(jumps);
    let mut back = vec![0.0; legs.len() + 1];
    back[legs.len()] = fuel;
    for i in (0..legs.len()).rev() {
        if fuel < f64::from(legs[i]) {
            return None;
        }
        fuel = (fuel - f64::from(legs[i]) + gains[i]).min(jumps);
        back[i] = fuel;
    }
    Some((out, back))
}

struct Frontier {
    chain: Vec<Hex>,
    relay: Vec<bool>,
    legs: Vec<u32>,
    gains: Vec<f64>,
    out: f64,
}

fn half_store(jump_fuel: f64) -> f64 {
    RELAY_STORE / FUEL_DEUT_PER_UNIT / jump_fuel / 2.0
}

fn relay_ground(engine: &GameEngine, at: Hex) -> bool {
    let template = engine.world_gen.generate_sector(at);
    template.npc_template.is_none() && !template.has_derelict && template.terrain != TerrainType::Anomaly
}

fn plan(engine: &GameEngine, home: Hex, jump_fuel: f64, jumps: f64, tier: Tier, relay_limit: usize, limit: Option<f32>) -> Plan {
    let mut best = Plan {
        nodes: vec![home],
        relay: vec![false],
        target: home,
        ring: home.dist_from_origin(),
        out_fuel: vec![jumps],
        back_fuel: vec![jumps],
    };
    let mut seen: BTreeMap<u32, f64> = BTreeMap::new();
    let mut queue = VecDeque::from([Frontier {
        chain: vec![home],
        relay: vec![false],
        legs: vec![],
        gains: vec![0.0],
        out: jumps,
    }]);
    while let Some(node) = queue.pop_front() {
        let here = *node.chain.last().unwrap();
        let reach = node.out.floor() as u32;
        let near = walk_from(engine, here, reach, limit);
        let mut targets: Vec<(u16, u32, Hex)> = Vec::new();
        let mut depots: Vec<(u16, u32, Hex)> = Vec::new();
        for (&key, walk) in &near {
            let at = Hex::from_key(key);
            let ring = at.dist_from_origin();
            if ring > best.ring && f64::from(walk.hops) * 2.0 <= node.out {
                targets.push((ring, walk.hops, at));
            }
            let relays_used = node.relay.iter().filter(|used| **used).count();
            if tier.relays
                && relays_used < relay_limit
                && (RELAY_MIN_RING..=MAP_LIMIT).contains(&ring)
                && walk.hops > 0
                && f64::from(walk.hops) <= node.out
                && node.legs.len() < MAX_STOPS
                && !node.chain.contains(&at)
                && !is_cloud(engine, at)
                && relay_ground(engine, at)
            {
                depots.push((ring, walk.hops, at));
            }
            if tier.clouds && ring <= MAP_LIMIT && walk.hops > 0 && node.legs.len() < MAX_STOPS && !node.chain.contains(&at) {
                let gain = half_deposit(engine, at, jump_fuel);
                let leaves = (node.out - f64::from(walk.hops) + gain).min(jumps);
                if gain > 0.0 && f64::from(walk.hops) <= node.out && seen.get(&key).is_none_or(|&known| leaves > known) {
                    seen.insert(key, leaves);
                    let mut chain = node.chain.clone();
                    chain.push(at);
                    let mut relay = node.relay.clone();
                    relay.push(false);
                    let mut legs = node.legs.clone();
                    legs.push(walk.hops);
                    let mut gains = node.gains.clone();
                    gains.push(gain);
                    queue.push_back(Frontier { chain, relay, legs, gains, out: leaves });
                }
            }
        }
        depots.sort_by_key(|&(ring, hops, at)| (std::cmp::Reverse(ring), hops, at.to_key()));
        for &(_, hops, at) in depots.iter().take(RELAY_TRIES) {
            let gain = half_store(jump_fuel);
            let leaves = (node.out - f64::from(hops) + gain).min(jumps);
            if seen.get(&at.to_key()).is_none_or(|&known| leaves > known) {
                seen.insert(at.to_key(), leaves);
                let mut chain = node.chain.clone();
                chain.push(at);
                let mut relay = node.relay.clone();
                relay.push(true);
                let mut legs = node.legs.clone();
                legs.push(hops);
                let mut gains = node.gains.clone();
                gains.push(gain);
                queue.push_back(Frontier { chain, relay, legs, gains, out: leaves });
            }
        }
        targets.sort_by_key(|&(ring, hops, at)| std::cmp::Reverse((ring, hops, at.to_key())));
        for (ring, hops, at) in targets.into_iter().take(TARGET_TRIES) {
            if ring <= best.ring {
                break;
            }
            if let Some((out_fuel, back_fuel)) = simulate(&node.legs, hops, &node.gains, jumps) {
                best = Plan { nodes: node.chain.clone(), relay: node.relay.clone(), target: at, ring, out_fuel, back_fuel };
                break;
            }
        }
    }
    best
}

fn top_up(e: &mut Empire, at: Hex, wanted: f64) {
    let fid = e.fid;
    for _ in 0..40 {
        sweep(&mut e.engine, &[at]);
        let f = &e.engine.fleets[&fid];
        let goal = wanted.min(f.fuel_max);
        if f.fuel >= goal - 1e-6 {
            return;
        }
        let before = f.fuel;
        e.run_until(2_000, |e| {
            let f = &e.engine.fleets[&fid];
            f.state == FleetStatus::Idle && f.action_cooldown == 0
        });
        let _ = e.order(Command::Harvest { fleet_id: fid, resource: HarvestResource::Deuterium });
        for _ in 0..20_000 {
            let f = &e.engine.fleets[&fid];
            if f.state != FleetStatus::Harvesting || f.fuel + f.cargo.deuterium * refined_fuel_per_deuterium() >= goal {
                break;
            }
            e.run(1);
        }
        if e.engine.fleets[&fid].state == FleetStatus::Harvesting {
            let _ = e.order(Command::Stop { fleet_id: fid });
        }
        let _ = e.order(Command::Refine { fleet_id: fid });
        if e.engine.fleets[&fid].fuel <= before + 1e-9 {
            return;
        }
    }
}

fn fly(e: &mut Empire, path: &[Hex]) -> Result<(), String> {
    let fid = e.fid;
    for &hop in path {
        sweep(&mut e.engine, &[hop]);
        let from = e.engine.fleets[&fid].location;
        let cost = e.engine.hop_fuel_cost(&e.engine.fleets[&fid]);
        if e.engine.fleets[&fid].fuel < cost - 1e-9 {
            return Err(format!("out of fuel at {from}"));
        }
        e.run_until(2_000, |e| {
            let f = &e.engine.fleets[&fid];
            f.state == FleetStatus::Docked || (f.state == FleetStatus::Idle && f.action_cooldown == 0)
        });
        e.order(Command::Move { fleet_id: fid, target: hop }).map_err(|code| format!("move refused: {code:?}"))?;
        e.run_until(2_000, |e| {
            let f = &e.engine.fleets[&fid];
            f.location == hop && f.state != FleetStatus::Moving
        });
    }
    Ok(())
}

fn found_and_stock(e: &mut Empire, at: Hex) -> Result<(), String> {
    let (pid, fid) = (e.pid, e.fid);
    let cost = iac_shared::scaling::relay_cost(at.dist_from_origin());
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).expect("fleet");
        fleet.cargo = fleet.cargo.add(cost);
    });
    e.order(Command::FoundRelay { fleet_id: fid }).map_err(|code| format!("founding refused: {code:?}"))?;
    restock(e, at, pid);
    Ok(())
}

fn restock(e: &mut Empire, at: Hex, pid: u64) {
    edit(&mut e.engine, |s| {
        let relay = s.relays.get_mut(&pid).and_then(|list| list.iter_mut().find(|r| r.sector == at)).expect("relay");
        relay.store = RELAY_STORE / 2.0;
    });
}

fn play(engine: GameEngine, pid: u64, fid: u64, chosen: &Plan, jump_fuel: f64, tier: Tier) -> Result<u64, String> {
    let use_clouds = tier.clouds;
    let mut e = Empire { engine, pid, fid, log: Vec::new() };
    let start = e.engine.current_tick();
    let legs = chosen.nodes.len();
    for i in 1..legs {
        let path = path_between(&e.engine, chosen.nodes[i - 1], chosen.nodes[i], None);
        fly(&mut e, &path)?;
        if use_clouds {
            top_up(&mut e, chosen.nodes[i], chosen.out_fuel[i] * jump_fuel);
        }
        if chosen.relay[i] {
            found_and_stock(&mut e, chosen.nodes[i])?;
            e.run(3);
            restock(&mut e, chosen.nodes[i], pid);
        }
    }
    if chosen.target != chosen.nodes[legs - 1] {
        let path = path_between(&e.engine, chosen.nodes[legs - 1], chosen.target, None);
        fly(&mut e, &path)?;
        let path = path_between(&e.engine, chosen.target, chosen.nodes[legs - 1], None);
        fly(&mut e, &path)?;
    }
    for i in (0..legs).rev() {
        if use_clouds && is_cloud(&e.engine, chosen.nodes[i]) {
            top_up(&mut e, chosen.nodes[i], chosen.back_fuel[i] * jump_fuel);
        }
        if i > 0 {
            let path = path_between(&e.engine, chosen.nodes[i], chosen.nodes[i - 1], None);
            fly(&mut e, &path)?;
        }
    }
    Ok(e.engine.current_tick() - start)
}

struct Reading {
    jumps: u32,
    ring: u16,
    stops: usize,
    played: Result<u64, String>,
}

struct Setup {
    engine: GameEngine,
    pid: u64,
    fid: u64,
    home: Hex,
    jump_fuel: f64,
    jumps: f64,
    limit: usize,
}

fn setup(seed: u64, class: ShipClass, level: u8) -> Setup {
    let mut engine = GameEngine::new(seed, Pace::parse("season").expect("known pace"));
    let (pid, fid) = join(&mut engine, "Prober");
    let home = world::home_seats().find(|at| at.dist_from_origin() == HOME_RING).expect("a ring-4 seat");
    edit(&mut engine, |s| {
        let player = s.players.get_mut(&pid).expect("player");
        player.homeworld = home;
        player.research.energy_technology = level;
        player.research.impulse_drive = level;
        player.buildings.deuterium_tank = level;
        player.research.hyperspace_technology = level;
        let tank = ship_tank(class, &player.research);
        let fleet = s.fleets.get_mut(&fid).expect("fleet");
        fleet.ships = Ships::of(&[(class, FLEET_SHIPS)]);
        fleet.location = home;
        fleet.fuel_max = f64::from(FLEET_SHIPS) * tank * tank_modifier(level);
        fleet.fuel = fleet.fuel_max;
    });
    let probe = Empire { engine, pid, fid, log: Vec::new() };
    let state = probe.fleet();
    let jumps = (state.fuel_max / state.jump_fuel).floor();
    let limit = probe.engine.relay_limit(pid) as usize;
    Setup { jump_fuel: state.jump_fuel, engine: probe.engine, pid, fid, home, jumps, limit }
}

fn measure(class: ShipClass, level: u8, tier: Tier) -> Reading {
    let s = setup(SEED, class, level);
    let chosen = plan(&s.engine, s.home, s.jump_fuel, s.jumps, tier, s.limit, None);
    let played = play(s.engine, s.pid, s.fid, &chosen, s.jump_fuel, tier);
    Reading { jumps: s.jumps as u32, ring: chosen.ring, stops: chosen.nodes.len() - 1, played }
}

#[test]
fn reach_with_and_without_clouds_and_relays() {
    println!("class          level  jumps  relays  ring_off  clouds  relays_on  both  (stops: clouds/relays/both)");
    for class in [ShipClass::LightFighter, ShipClass::Cruiser] {
        for level in [0u8, 3, 6, 10, 13] {
            let off = measure(class, level, OFF);
            let clouds = measure(class, level, CLOUDS);
            let relays = measure(class, level, RELAYS);
            let both = measure(class, level, BOTH);
            let played = |r: &Result<u64, String>| match r {
                Ok(_) => "ok".to_string(),
                Err(why) => format!("FAILED {why}"),
            };
            println!(
                "{:<14} {level:>5} {:>6} {:>7} {:>9} {:>7} {:>10} {:>5}  ({}/{}/{})  off: {}; clouds: {}; relays: {}; both: {}",
                class.label(),
                off.jumps,
                1 + u32::from(level) / 2,
                off.ring,
                clouds.ring,
                relays.ring,
                both.ring,
                clouds.stops,
                relays.stops,
                both.stops,
                played(&off.played),
                played(&clouds.played),
                played(&relays.played),
                played(&both.played),
            );
            assert!(clouds.ring >= off.ring, "clouds never shorten the reach");
            assert!(relays.ring >= off.ring, "relays never shorten the reach");
            assert!(both.ring >= relays.ring.max(clouds.ring), "the two together never reach less than either");
            assert!(relays.played.is_ok(), "the relays plan is flown home: {:?}", relays.played);
        }
    }
}

#[test]
fn cloud_census() {
    use iac_shared::constants::{CLOUD_RADIUS_BASE, CLOUD_RADIUS_MAX, CLOUD_RADIUS_PER_RING};
    let engine = world("season");
    println!("ring band    sectors  in a cloud  share  sectors per cloud at its low ring");
    for (lo, hi) in [(2u16, 8u16), (9, 20), (21, 30), (31, 45), (46, 60)] {
        let (mut all, mut clouded) = (0u32, 0u32);
        for ring in lo..=hi {
            for at in iac_shared::hex::hex_ring(Hex::ORIGIN, ring) {
                all += 1;
                clouded += u32::from(is_cloud(&engine, at));
            }
        }
        let depth = (CLOUD_RADIUS_BASE + CLOUD_RADIUS_PER_RING * u32::from(lo)).min(CLOUD_RADIUS_MAX) / 100;
        println!(
            "{lo:>3}..{hi:<3}     {all:>7} {clouded:>11} {:>5.1}%  {}",
            100.0 * f64::from(clouded) / f64::from(all),
            1 + 3 * depth * (depth + 1),
        );
    }
}

struct Cell {
    seed: u64,
    class: ShipClass,
    level: u8,
    fuel: [u16; 4],
    fight: [u16; 3],
    lairs: usize,
    strongest: f32,
    own: f32,
}

fn route_danger(engine: &GameEngine, chosen: &Plan) -> (usize, f32) {
    let mut crossed: BTreeMap<u32, f32> = BTreeMap::new();
    let mut leg = |from: Hex, to: Hex| {
        for at in path_between(engine, from, to, None) {
            let power = lair_power(engine, at);
            if power > 0.0 {
                crossed.insert(at.to_key(), power);
            }
        }
    };
    for pair in chosen.nodes.windows(2) {
        leg(pair[0], pair[1]);
    }
    let last = *chosen.nodes.last().unwrap();
    if chosen.target != last {
        leg(last, chosen.target);
    }
    (crossed.len(), crossed.values().copied().fold(0.0, f32::max))
}

fn cell(seed: u64, class: ShipClass, level: u8) -> Cell {
    let s = setup(seed, class, level);
    let own = Ships::of(&[(class, FLEET_SHIPS)]).power(Refit { weapons: level, shielding: level, armour: level }.arms());
    let ring = |tier: Tier, limit: Option<f32>| plan(&s.engine, s.home, s.jump_fuel, s.jumps, tier, s.limit, limit);
    let both = ring(BOTH, None);
    let (lairs, strongest) = route_danger(&s.engine, &both);
    let mut fight = FIGHT_MULTIPLES.map(|k| ring(BOTH, Some(own * k)).ring);
    for i in 1..fight.len() {
        fight[i] = fight[i].max(fight[i - 1]);
    }
    let both_ring = both.ring.max(fight[fight.len() - 1]);
    Cell {
        seed,
        class,
        level,
        fuel: [ring(OFF, None).ring, ring(CLOUDS, None).ring, ring(RELAYS, None).ring, both_ring],
        fight,
        lairs,
        strongest,
        own,
    }
}

fn median(values: &[u16]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        f64::from(sorted[mid])
    } else {
        f64::from(sorted[mid - 1] + sorted[mid]) / 2.0
    }
}

fn spread(values: &[u16]) -> String {
    format!("{} ({}-{})", median(values), values.iter().min().unwrap(), values.iter().max().unwrap())
}

fn median_f(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        values[mid]
    } else {
        (values[mid - 1] + values[mid]) / 2.0
    }
}

#[test]
fn reach_across_seeds_and_danger() {
    let jobs: Vec<(u64, ShipClass, u8)> = SEEDS
        .iter()
        .flat_map(|&seed| CLASSES.iter().flat_map(move |&class| LEVELS.iter().map(move |&level| (seed, class, level))))
        .collect();
    let cells: Vec<Cell> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..WORKERS)
            .map(|worker| {
                let mine: Vec<_> = jobs.iter().copied().skip(worker).step_by(WORKERS).collect();
                scope.spawn(move || mine.into_iter().map(|(seed, class, level)| cell(seed, class, level)).collect::<Vec<_>>())
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("worker")).collect()
    });
    println!("seeds: {SEEDS:?}; {FLEET_SHIPS} ships; cells are median (min-max) farthest ring\n");
    println!("| class | level | off | clouds | relays | both |");
    println!("|---|---|---|---|---|---|");
    for class in CLASSES {
        for level in LEVELS {
            let rows: Vec<&Cell> = cells.iter().filter(|c| c.class == class && c.level == level).collect();
            let col = |i: usize| spread(&rows.iter().map(|c| c.fuel[i]).collect::<Vec<_>>());
            println!("| {} | {level} | {} | {} | {} | {} |", class.label(), col(0), col(1), col(2), col(3));
            for c in &rows {
                assert!(c.fuel[1] >= c.fuel[0], "clouds never shorten the reach (seed {})", c.seed);
                assert!(c.fuel[2] >= c.fuel[0], "relays never shorten the reach (seed {})", c.seed);
                assert!(c.fuel[3] >= c.fuel[1].max(c.fuel[2]), "both reach at least either (seed {})", c.seed);
            }
        }
    }
    println!("\n| class | level | fleet power | fuel ring (both) | fight 1x | fight 2x | fight 4x | lairs on route | strongest on route, x fleet |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for class in CLASSES {
        for level in LEVELS {
            let rows: Vec<&Cell> = cells.iter().filter(|c| c.class == class && c.level == level).collect();
            let col = |i: usize| spread(&rows.iter().map(|c| c.fight[i]).collect::<Vec<_>>());
            let lairs: Vec<u16> = rows.iter().map(|c| c.lairs as u16).collect();
            let mut ratios: Vec<f64> = rows.iter().map(|c| f64::from(c.strongest / c.own)).collect();
            let top = ratios.iter().copied().fold(0.0, f64::max);
            println!(
                "| {} | {level} | {:.0} | {} | {} | {} | {} | {} | {:.2} (max {:.2}) |",
                class.label(),
                rows[0].own,
                spread(&rows.iter().map(|c| c.fuel[3]).collect::<Vec<_>>()),
                col(0),
                col(1),
                col(2),
                spread(&lairs),
                median_f(&mut ratios),
                top,
            );
            for c in &rows {
                assert!(c.fight[0] <= c.fight[1] && c.fight[1] <= c.fight[2], "a looser guard never reaches less (seed {})", c.seed);
                assert!(c.fight[2] <= c.fuel[3], "the fight never reaches past the fuel (seed {})", c.seed);
            }
        }
    }
}

#[test]
fn cloud_distance_census() {
    let per_seed: Vec<(u64, [Option<u32>; 3], [u32; 3])> = std::thread::scope(|scope| {
        let handles: Vec<_> = SEEDS
            .iter()
            .map(|&seed| {
                scope.spawn(move || {
                    let s = setup(seed, ShipClass::LightFighter, 0);
                    let walk = walk_from(&s.engine, s.home, CENSUS_HOPS, None);
                    let nearest = |keep: &dyn Fn(Hex) -> bool| {
                        walk.iter().filter(|(_, w)| w.hops > 0).filter(|&(&k, _)| keep(Hex::from_key(k))).map(|(_, w)| w.hops).min()
                    };
                    let cloud = nearest(&|at| is_cloud(&s.engine, at));
                    let deut = nearest(&|at| s.engine.world_gen.generate_sector(at).deut_density as u8 >= Density::Moderate as u8);
                    let lair = nearest(&|at| lair_power(&s.engine, at) > 0.0);
                    let tanks = [0u8, 3, 6].map(|level| setup(seed, ShipClass::LightFighter, level).jumps as u32);
                    (seed, [cloud, deut, lair], tanks)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("worker")).collect()
    });
    println!("| seed | nearest cloud | nearest deuterium Moderate+ | nearest lair | LF tank, jumps, levels 0/3/6 |");
    println!("|---|---|---|---|---|");
    let show = |v: Option<u32>| v.map_or("none".to_string(), |h| h.to_string());
    for (seed, near, tanks) in &per_seed {
        println!("| {seed} | {} | {} | {} | {}/{}/{} |", show(near[0]), show(near[1]), show(near[2]), tanks[0], tanks[1], tanks[2]);
    }
    for (i, name) in ["cloud", "deuterium Moderate+", "lair"].iter().enumerate() {
        let mut hops: Vec<f64> = per_seed.iter().map(|(_, n, _)| n[i].map_or(f64::from(CENSUS_HOPS + 1), f64::from)).collect();
        println!("median hops to nearest {name}: {}", median_f(&mut hops));
    }
    for (li, level) in [0u8, 3, 6].iter().enumerate() {
        let within_tank = per_seed.iter().filter(|(_, n, t)| n[0].is_some_and(|h| h <= t[li])).count();
        let within_round_trip = per_seed.iter().filter(|(_, n, t)| n[0].is_some_and(|h| 2 * h <= t[li])).count();
        println!(
            "level {level}: a cloud within a full tank of home in {within_tank}/{} seeds, within a round trip on one tank in {within_round_trip}/{}",
            SEEDS.len(),
            SEEDS.len()
        );
    }
}
