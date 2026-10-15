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
// store: half is spent on the way out and half on the way back, as a cloud's,
// less a quarter held back for what the engine fills in whole units. A relay
// fills a tank only to `depot_fill_jumps` of the fleet's own burn.
//
// The hubs' model: the four roaming hubs where the world puts them at tick 0
// (a hub stays hours of ticks, and a plan is flown in a few hundred, so none
// has moved by the time it is used), at most one on a route, refuelling from a
// hold of deuterium the fleet leaves home with, half on the way out and half
// on the way back, to the same fill as a relay.
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
use iac_shared::scaling::depot_fill_jumps;
use iac_sim::engine::fleet_cargo_capacity;
use iac_shared::pace::Pace;
use iac_shared::hex::Hex;
use iac_shared::keymap::KeyMap;
use iac_shared::protocol::{Command, HarvestResource};
use iac_shared::scaling::{refined_fuel_per_deuterium, ship_tank, tank_modifier, Arms, Refit, NPC_MAX_SHIPS};
use iac_shared::world;
use iac_sim::engine::{FleetStatus, Ships};
use iac_sim::GameEngine;

const FLEET_SHIPS: u32 = 10;
const SEED: u64 = 42;
const SEEDS: [u64; 8] = [42, 7, 101, 2024, 31337, 555, 9001, 1234];
const LEVELS: [u8; 5] = [0, 3, 6, 10, 13];

fn levels_of(class: ShipClass) -> &'static [u8] {
    if class == ShipClass::Battleship {
        &[3, 10, 13]
    } else {
        &LEVELS
    }
}
const CLASSES: [ShipClass; 3] = [ShipClass::LightFighter, ShipClass::Cruiser, ShipClass::Battleship];

const FIGHT_MULTIPLES: [f32; 3] = [1.0, 2.0, 4.0];
const CENSUS_HOPS: u32 = 60;
const WORKERS: usize = 6;
const HOME_RING: u16 = 4;
const SEARCH_HOPS: u32 = 120;
const MAP_LIMIT: u16 = 40;
const MAX_STOPS: usize = 8;
const RELAY_TRIES: usize = 3;

#[derive(Clone, Copy)]
struct Tier {
    clouds: bool,
    relays: bool,
    hubs: bool,
}

const OFF: Tier = Tier { clouds: false, relays: false, hubs: false };
const CLOUDS: Tier = Tier { clouds: true, relays: false, hubs: false };
const RELAYS: Tier = Tier { clouds: false, relays: true, hubs: false };
const HUBS: Tier = Tier { clouds: false, relays: false, hubs: true };
const BOTH: Tier = Tier { clouds: true, relays: true, hubs: false };
const ALL: Tier = Tier { clouds: true, relays: true, hubs: true };

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

fn walk_from(engine: &GameEngine, origin: Hex, depth: u32, limit: Option<f32>) -> KeyMap<u32, Walk> {
    let mut seen = KeyMap::default();
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

struct Rig {
    jump_fuel: f64,
    jumps: f64,
    relay_gain: f64,
    fill: f64,
    hold_gain: f64,
    hubs: Vec<Hex>,
    relay_limit: usize,
}

fn refill(fuel: f64, gain: f64, cap: f64) -> f64 {
    fuel.max((fuel + gain).min(cap))
}

/// Fuel, in jumps, the fleet holds when it leaves each node outward and
/// homeward, or None when the round trip cannot be flown.
fn simulate(legs: &[u32], last: u32, gains: &[f64], caps: &[f64], jumps: f64) -> Option<(Vec<f64>, Vec<f64>)> {
    let mut fuel = jumps;
    let mut out = vec![jumps];
    for (i, &leg) in legs.iter().enumerate() {
        if fuel < f64::from(leg) {
            return None;
        }
        fuel = refill(fuel - f64::from(leg), gains[i + 1], caps[i + 1]);
        out.push(fuel);
    }
    if fuel < 2.0 * f64::from(last) {
        return None;
    }
    fuel = refill(fuel - 2.0 * f64::from(last), gains[legs.len()], caps[legs.len()]);
    let mut back = vec![0.0; legs.len() + 1];
    back[legs.len()] = fuel;
    for i in (0..legs.len()).rev() {
        if fuel < f64::from(legs[i]) {
            return None;
        }
        fuel = refill(fuel - f64::from(legs[i]), gains[i], caps[i]);
        back[i] = fuel;
    }
    Some((out, back))
}

struct Frontier {
    chain: Vec<Hex>,
    relay: Vec<bool>,
    legs: Vec<u32>,
    gains: Vec<f64>,
    caps: Vec<f64>,
    out: f64,
    hub: bool,
}

fn relay_ground(engine: &GameEngine, at: Hex) -> bool {
    let template = engine.world_gen.generate_sector(at);
    template.npc_template.is_none() && !template.has_derelict && template.terrain != TerrainType::Anomaly
}

fn plan(engine: &GameEngine, home: Hex, rig: &Rig, tier: Tier, limit: Option<f32>) -> Plan {
    let jumps = rig.jumps;
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
        caps: vec![jumps],
        out: jumps,
        hub: false,
    }]);
    while let Some(node) = queue.pop_front() {
        let here = *node.chain.last().unwrap();
        let reach = node.out.floor() as u32;
        let near = walk_from(engine, here, reach, limit);
        let extend = |node: &Frontier, at: Hex, hops: u32, gain: f64, cap: f64, relay: bool, hub: bool| {
            let mut chain = node.chain.clone();
            chain.push(at);
            let mut flags = node.relay.clone();
            flags.push(relay);
            let mut legs = node.legs.clone();
            legs.push(hops);
            let mut gains = node.gains.clone();
            gains.push(gain);
            let mut caps = node.caps.clone();
            caps.push(cap);
            let out = refill(node.out - f64::from(hops), gain, cap);
            Frontier { chain, relay: flags, legs, gains, caps, out, hub: node.hub || hub }
        };
        let mut targets: Vec<(u16, u32, Hex)> = Vec::new();
        let mut depots: Vec<(u16, u32, Hex)> = Vec::new();
        let mut keys: Vec<u32> = near.keys().copied().collect();
        keys.sort_unstable();
        for key in keys {
            let walk = &near[&key];
            let at = Hex::from_key(key);
            let ring = at.dist_from_origin();
            if ring > best.ring && f64::from(walk.hops) * 2.0 <= node.out {
                targets.push((ring, walk.hops, at));
            }
            let relays_used = node.relay.iter().filter(|used| **used).count();
            if tier.relays
                && relays_used < rig.relay_limit
                && (RELAY_MIN_RING..=MAP_LIMIT).contains(&ring)
                && walk.hops > 0
                && f64::from(walk.hops) <= node.out.min(rig.fill.min(jumps))
                && node.legs.len() < MAX_STOPS
                && !node.chain.contains(&at)
                && !is_cloud(engine, at)
                && relay_ground(engine, at)
            {
                depots.push((ring, walk.hops, at));
            }
            if tier.clouds && ring <= MAP_LIMIT && walk.hops > 0 && node.legs.len() < MAX_STOPS && !node.chain.contains(&at) {
                let gain = half_deposit(engine, at, rig.jump_fuel);
                let next = extend(&node, at, walk.hops, gain, jumps, false, false);
                if gain > 0.0 && f64::from(walk.hops) <= node.out && seen.get(&key).is_none_or(|&known| next.out > known) {
                    seen.insert(key, next.out);
                    queue.push_back(next);
                }
            }
            if tier.hubs
                && !node.hub
                && rig.hubs.contains(&at)
                && walk.hops > 0
                && f64::from(walk.hops) <= node.out
                && node.legs.len() < MAX_STOPS
            {
                let next = extend(&node, at, walk.hops, rig.hold_gain, rig.fill.min(jumps), false, true);
                if seen.get(&key).is_none_or(|&known| next.out > known) {
                    seen.insert(key, next.out);
                    queue.push_back(next);
                }
            }
        }
        let mut picks: Vec<(u16, u32, Hex)> = Vec::new();
        let by_depth = |a: &(u16, u32, Hex), b: &(u16, u32, Hex)| (b.0, a.1, a.2.to_key()).cmp(&(a.0, b.1, b.2.to_key()));
        let waste_free = (node.out - (rig.fill.min(jumps) - rig.relay_gain)).max(1.0);
        if let Some(&first) = depots
            .iter()
            .filter(|d| f64::from(d.1) >= waste_free)
            .min_by(|a, b| (a.1, std::cmp::Reverse(a.0), a.2.to_key()).cmp(&(b.1, std::cmp::Reverse(b.0), b.2.to_key())))
        {
            picks.push(first);
        }
        if let Some(&deepest) = depots.iter().min_by(|a, b| by_depth(a, b)) {
            picks.push(deepest);
            if let Some(&first) = picks.first() {
                let middle = (first.1 + deepest.1) / 2;
                if let Some(&mid) = depots
                    .iter()
                    .min_by_key(|d| (d.1.abs_diff(middle), std::cmp::Reverse(d.0), d.2.to_key()))
                {
                    picks.push(mid);
                }
            }
        }
        picks.dedup_by_key(|d| d.2.to_key());
        for &(_, hops, at) in picks.iter().take(RELAY_TRIES) {
            let next = extend(&node, at, hops, rig.relay_gain, rig.fill.min(jumps), true, false);
            if seen.get(&at.to_key()).is_none_or(|&known| next.out > known) {
                seen.insert(at.to_key(), next.out);
                queue.push_back(next);
            }
        }
        targets.sort_by_key(|&(ring, hops, at)| (std::cmp::Reverse(ring), hops, at.to_key()));
        targets.dedup_by_key(|target| target.0);
        for (ring, hops, at) in targets {
            if ring <= best.ring {
                break;
            }
            if let Some((out_fuel, back_fuel)) = simulate(&node.legs, hops, &node.gains, &node.caps, jumps) {
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
            let f = &e.engine.fleets[&fid];
            return Err(format!("out of fuel at {from}: {:.0} of {cost:.0}, tank {:.0}, relay here {}", f.fuel, f.fuel_max, e.engine.relay_at(from).is_some()));
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
    for i in 1..legs {
        if chosen.relay[i] {
            restock(&mut e, chosen.nodes[i], pid);
        }
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
    rig: Rig,
    hop_ticks: u16,
}

fn setup(seed: u64, class: ShipClass, level: u8) -> Setup {
    let mut engine = GameEngine::new(seed, Pace::parse("season").expect("known pace"));
    let (pid, fid) = join(&mut engine, "Prober");
    let home = world::home_seats().find(|at| at.dist_from_origin() == HOME_RING).expect("a ring-4 seat");
    let hubs: Vec<Hex> = engine.hubs.iter().map(|hub| hub.sector).collect();
    let mut research = iac_shared::scaling::ResearchLevels::default();
    edit(&mut engine, |s| {
        let player = s.players.get_mut(&pid).expect("player");
        player.homeworld = home;
        player.research.energy_technology = level;
        player.research.impulse_drive = level;
        player.buildings.deuterium_tank = level;
        player.research.hyperspace_technology = level;
        research = player.research.clone();
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
    let relay_limit = probe.engine.relay_limit(pid) as usize;
    let fleet = &probe.engine.fleets[&fid];
    let hold = fleet_cargo_capacity(fleet, &probe.engine.world.pace);
    let hop_ticks = probe.engine.hop_ticks(fleet);
    let rig = Rig {
        jump_fuel: state.jump_fuel,
        jumps,
        relay_gain: 0.75 * RELAY_STORE / FUEL_DEUT_PER_UNIT / state.jump_fuel / 2.0,
        fill: f64::from(depot_fill_jumps(&research)),
        hold_gain: hold / FUEL_DEUT_PER_UNIT / state.jump_fuel / 2.0,
        hubs,
        relay_limit,
    };
    Setup { engine: probe.engine, pid, fid, home, rig, hop_ticks }
}

fn measure(class: ShipClass, level: u8, tier: Tier) -> Reading {
    let s = setup(SEED, class, level);
    let chosen = plan(&s.engine, s.home, &s.rig, tier, None);
    let (jumps, jump_fuel) = (s.rig.jumps as u32, s.rig.jump_fuel);
    let played = play(s.engine, s.pid, s.fid, &chosen, jump_fuel, tier);
    Reading { jumps, ring: chosen.ring, stops: chosen.nodes.len() - 1, played }
}

#[test]
fn reach_with_and_without_clouds_and_relays() {
    println!("class          level  jumps  relays  ring_off  clouds  relays_on  both  (stops: clouds/relays/both)");
    for class in [ShipClass::LightFighter, ShipClass::Cruiser] {
        for level in [0u8, 3, 10, 13] {
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
    let engine = world("season");
    println!("ring band    sectors  in a cloud  share");
    for (lo, hi) in [(2u16, 8u16), (9, 20), (21, 30), (31, 45), (46, 60)] {
        let (mut all, mut clouded) = (0u32, 0u32);
        for ring in lo..=hi {
            for at in iac_shared::hex::hex_ring(Hex::ORIGIN, ring) {
                all += 1;
                clouded += u32::from(is_cloud(&engine, at));
            }
        }
        println!("{lo:>3}..{hi:<3}     {all:>7} {clouded:>11} {:>5.1}%", 100.0 * f64::from(clouded) / f64::from(all));
    }
}

struct Cell {
    seed: u64,
    class: ShipClass,
    level: u8,
    fuel: [u16; 6],
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
    let ring = |tier: Tier, limit: Option<f32>| plan(&s.engine, s.home, &s.rig, tier, limit);
    let all = ring(ALL, None);
    let (lairs, strongest) = route_danger(&s.engine, &all);
    let mut fight = FIGHT_MULTIPLES.map(|k| ring(ALL, Some(own * k)).ring);
    for i in 1..fight.len() {
        fight[i] = fight[i].max(fight[i - 1]);
    }
    let mut fuel = [OFF, CLOUDS, RELAYS, HUBS, BOTH].map(|tier| ring(tier, None).ring);
    fuel[4] = fuel[4].max(fuel[1]).max(fuel[2]);
    let everything = all.ring.max(fight[fight.len() - 1]).max(fuel[4]).max(fuel[3]);
    let fuel = [fuel[0], fuel[1], fuel[2], fuel[3], fuel[4], everything];
    Cell { seed, class, level, fuel, fight, lairs, strongest, own }
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
        .flat_map(|&seed| {
            CLASSES.iter().flat_map(move |&class| levels_of(class).iter().map(move |&level| (seed, class, level)))
        })
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
    println!("| class | level | off | clouds | relays | hubs | clouds+relays | all three |");
    println!("|---|---|---|---|---|---|---|---|");
    for class in CLASSES {
        for &level in levels_of(class) {
            let rows: Vec<&Cell> = cells.iter().filter(|c| c.class == class && c.level == level).collect();
            let col = |i: usize| spread(&rows.iter().map(|c| c.fuel[i]).collect::<Vec<_>>());
            println!(
                "| {} | {level} | {} | {} | {} | {} | {} | {} |",
                class.label(),
                col(0),
                col(1),
                col(2),
                col(3),
                col(4),
                col(5)
            );
            for c in &rows {
                assert!(c.fuel[1] >= c.fuel[0], "clouds never shorten the reach (seed {})", c.seed);
                assert!(c.fuel[2] >= c.fuel[0], "relays never shorten the reach (seed {})", c.seed);
                assert!(c.fuel[3] >= c.fuel[0], "hubs never shorten the reach (seed {})", c.seed);
                assert!(c.fuel[5] >= c.fuel[4].max(c.fuel[3]), "all three reach at least any two (seed {})", c.seed);
            }
        }
    }
    println!("\n| class | level | fleet power | fuel ring (all three) | fight 1x | fight 2x | fight 4x | lairs on route | strongest on route, x fleet |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for class in CLASSES {
        for &level in levels_of(class) {
            let rows: Vec<&Cell> = cells.iter().filter(|c| c.class == class && c.level == level).collect();
            let col = |i: usize| spread(&rows.iter().map(|c| c.fight[i]).collect::<Vec<_>>());
            let lairs: Vec<u16> = rows.iter().map(|c| c.lairs as u16).collect();
            let mut ratios: Vec<f64> = rows.iter().map(|c| f64::from(c.strongest / c.own)).collect();
            let top = ratios.iter().copied().fold(0.0, f64::max);
            println!(
                "| {} | {level} | {:.0} | {} | {} | {} | {} | {} | {:.2} (max {:.2}) |",
                class.label(),
                rows[0].own,
                spread(&rows.iter().map(|c| c.fuel[5]).collect::<Vec<_>>()),
                col(0),
                col(1),
                col(2),
                spread(&lairs),
                median_f(&mut ratios),
                top,
            );
            for c in &rows {
                assert!(c.fight[0] <= c.fight[1] && c.fight[1] <= c.fight[2], "a looser guard never reaches less (seed {})", c.seed);
                assert!(c.fight[2] <= c.fuel[5], "the fight never reaches past the fuel (seed {})", c.seed);
            }
        }
    }
    let s = setup(SEED, ShipClass::LightFighter, 0);
    let hub_stays: Vec<u64> = s
        .engine
        .hubs
        .iter()
        .map(|hub| iac_shared::scaling::hub_departs_at(hub.id, hub.epoch, &s.engine.world.pace))
        .collect();
    println!("\nhubs at tick 0 (seed {SEED}) leave at ticks {hub_stays:?}; a Light Fighter jump takes {} ticks", s.hop_ticks);
}

#[test]
fn cloud_distance_census() {
    type Census = (u64, [Option<u32>; 3], [u32; 3], [u16; 3]);
    let per_seed: Vec<Census> = std::thread::scope(|scope| {
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
                    let tanks = [0u8, 3, 6].map(|level| setup(seed, ShipClass::LightFighter, level).rig.jumps as u32);
                    let gains = [0u8, 3, 6].map(|level| {
                        let s = setup(seed, ShipClass::LightFighter, level);
                        let off = plan(&s.engine, s.home, &s.rig, OFF, None).ring;
                        plan(&s.engine, s.home, &s.rig, CLOUDS, None).ring - off
                    });
                    (seed, [cloud, deut, lair], tanks, gains)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("worker")).collect()
    });
    println!("| seed | nearest cloud | nearest deuterium Moderate+ | nearest lair | LF tank, jumps, levels 0/3/6 | rings added by clouds, levels 0/3/6 |");
    println!("|---|---|---|---|---|---|");
    let show = |v: Option<u32>| v.map_or("none".to_string(), |h| h.to_string());
    for (seed, near, tanks, gains) in &per_seed {
        println!(
            "| {seed} | {} | {} | {} | {}/{}/{} | +{}/+{}/+{} |",
            show(near[0]),
            show(near[1]),
            show(near[2]),
            tanks[0],
            tanks[1],
            tanks[2],
            gains[0],
            gains[1],
            gains[2]
        );
    }
    for (i, name) in ["cloud", "deuterium Moderate+", "lair"].iter().enumerate() {
        let mut hops: Vec<f64> = per_seed.iter().map(|(_, n, _, _)| n[i].map_or(f64::from(CENSUS_HOPS + 1), f64::from)).collect();
        println!("median hops to nearest {name}: {}", median_f(&mut hops));
    }
    for (li, level) in [0u8, 3, 6].iter().enumerate() {
        let within_tank = per_seed.iter().filter(|(_, n, t, _)| n[0].is_some_and(|h| h <= t[li])).count();
        let within_round_trip = per_seed.iter().filter(|(_, n, t, _)| n[0].is_some_and(|h| 2 * h <= t[li])).count();
        let gaining = per_seed.iter().filter(|(_, _, _, g)| g[li] > 0).count();
        println!(
            "level {level}: a cloud within a full tank of home in {within_tank}/{} seeds, within a round trip on one tank in {within_round_trip}/{}, adding rings in {gaining}/{}",
            SEEDS.len(),
            SEEDS.len(),
            SEEDS.len()
        );
    }
}
