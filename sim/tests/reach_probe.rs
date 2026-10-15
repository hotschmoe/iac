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

#[allow(dead_code)]
mod common;

use std::collections::{BTreeMap, VecDeque};

use common::*;
use iac_shared::constants::{ShipClass, TerrainType, FUEL_DEUT_PER_UNIT, RELAY_MIN_RING, RELAY_STORE};
use iac_shared::hex::Hex;
use iac_shared::protocol::{Command, HarvestResource};
use iac_shared::scaling::{refined_fuel_per_deuterium, ship_tank, tank_modifier};
use iac_shared::world;
use iac_sim::engine::{FleetStatus, Ships};
use iac_sim::GameEngine;

const FLEET_SHIPS: u32 = 10;
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

fn walk_from(engine: &GameEngine, origin: Hex, depth: u32) -> BTreeMap<u32, Walk> {
    let mut seen = BTreeMap::new();
    seen.insert(origin.to_key(), Walk { hops: 0, parent: None });
    let mut queue = VecDeque::from([origin]);
    while let Some(at) = queue.pop_front() {
        let hops = seen[&at.to_key()].hops;
        if hops == depth {
            continue;
        }
        for &next in engine.world_gen.connected_neighbors(at).slice() {
            if seen.contains_key(&next.to_key()) {
                continue;
            }
            seen.insert(next.to_key(), Walk { hops: hops + 1, parent: Some(at) });
            queue.push_back(next);
        }
    }
    seen
}

fn path_between(engine: &GameEngine, from: Hex, to: Hex) -> Vec<Hex> {
    let walk = walk_from(engine, from, SEARCH_HOPS);
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

fn plan(engine: &GameEngine, home: Hex, jump_fuel: f64, jumps: f64, tier: Tier, relay_limit: usize) -> Plan {
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
        let near = walk_from(engine, here, reach);
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
        let path = path_between(&e.engine, chosen.nodes[i - 1], chosen.nodes[i]);
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
        let path = path_between(&e.engine, chosen.nodes[legs - 1], chosen.target);
        fly(&mut e, &path)?;
        let path = path_between(&e.engine, chosen.target, chosen.nodes[legs - 1]);
        fly(&mut e, &path)?;
    }
    for i in (0..legs).rev() {
        if use_clouds && is_cloud(&e.engine, chosen.nodes[i]) {
            top_up(&mut e, chosen.nodes[i], chosen.back_fuel[i] * jump_fuel);
        }
        if i > 0 {
            let path = path_between(&e.engine, chosen.nodes[i], chosen.nodes[i - 1]);
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

fn measure(class: ShipClass, level: u8, tier: Tier) -> Reading {
    let mut engine = world("season");
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
    let chosen = plan(&probe.engine, home, state.jump_fuel, jumps, tier, limit);
    let played = play(probe.engine, pid, fid, &chosen, state.jump_fuel, tier);
    Reading { jumps: jumps as u32, ring: chosen.ring, stops: chosen.nodes.len() - 1, played }
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
