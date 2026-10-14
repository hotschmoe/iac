// Deterministic procedural world generation.
// Sectors are generated from their coordinates + world seed.
// Only modified sectors are held and stored.
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::hash::Hasher;
use std::sync::Arc;

use xxhash_rust::xxh3::Xxh3;

use crate::constants::{
    CLOUD_CELL, CLOUD_DEUT_DENSITY, CLOUD_MIN_DIST, CLOUD_PCT, CLOUD_RADIUS_BASE, CLOUD_RADIUS_MAX,
    CLOUD_RADIUS_PER_RING, Density, ShipClass, TerrainType, Zone, HOMEWORLD_MAX_DIST, HOMEWORLD_MIN_DIST};
use crate::scaling::{
    npc_composition, npc_passive_share, npc_presence_pct, ore_density_odds, ore_reserve_units, ship_power,
};
use crate::hex::{hex_ring, Hex};
use crate::keymap::{KeyMap, KeySet};

/// A home seat is a sector of the home band whose `q - r` is a multiple of
/// three. Every rotation and mirror of the grid maps that lattice onto
/// itself, so the seat set has no favoured direction, and no two of its
/// points are adjacent: the nearest seats sit two hexes apart.
pub fn is_home_seat(coord: Hex) -> bool {
    (HOMEWORLD_MIN_DIST..=HOMEWORLD_MAX_DIST).contains(&coord.dist_from_origin())
        && (i32::from(coord.q) - i32::from(coord.r)).rem_euclid(3) == 0
}

/// Every home seat of a world, inner ring first.
pub fn home_seats() -> impl Iterator<Item = Hex> {
    (HOMEWORLD_MIN_DIST..=HOMEWORLD_MAX_DIST)
        .flat_map(|ring| hex_ring(Hex::ORIGIN, ring))
        .filter(|coord| is_home_seat(*coord))
}

/// A home seat or a sector beside one. No group that shoots first is
/// generated here and none wanders in; a convoy, which never does, may stand
/// here, on the seat itself too.
pub fn is_home_ground(coord: Hex) -> bool {
    is_home_seat(coord) || coord.neighbors().into_iter().any(is_home_seat)
}

/// What a sector of the hub offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HubService {
    /// The NPC exchange: a fleet here trades one resource in its hold for another.
    Exchange,
}

/// The neutral post at the centre: its sectors and what each offers. A hub
/// sector is empty space with no ore, no hostile group and no derelict, and
/// no hostile group wanders in.
pub const HUB: &[(Hex, &[HubService])] = &[(Hex::ORIGIN, &[HubService::Exchange])];

/// What `coord` offers as part of the hub; nothing anywhere else.
pub fn hub_services(coord: Hex) -> &'static [HubService] {
    HUB.iter().find(|(at, _)| *at == coord).map_or(&[], |(_, services)| *services)
}

pub fn is_hub(coord: Hex) -> bool {
    HUB.iter().any(|(at, _)| *at == coord)
}

/// The hub sector nearest `from` that offers `service`.
pub fn nearest_hub(from: Hex, service: HubService) -> Option<Hex> {
    HUB.iter()
        .filter(|(_, services)| services.contains(&service))
        .map(|(at, _)| *at)
        .min_by_key(|at| Hex::distance(at, &from))
}

/// Seed-based sector generation. Pure function — same inputs always produce same output.
/// Results are memoised per generator (the map is only ever read at the same
/// sectors tick after tick), which changes speed and nothing else.
pub struct WorldGen {
    pub world_seed: u64,
    sectors: RefCell<KeyMap<u32, SectorTemplate>>,
    links: RefCell<KeyMap<u32, ConnectedList>>,
    reach: RefCell<KeyMap<ReachKey, Arc<[Hex]>>>,
    hops: RefCell<KeyMap<HopsKey, Option<u32>>>,
}

/// Origin sector key and hop count of a cached `reachable_within`.
type ReachKey = (u32, u8);
/// The two sector keys and the hop limit of a cached `lane_hops`.
type HopsKey = (u32, u32, u8);

/// Keeps a cloud's cell hash apart from every other roll of the seed.
const CLOUD_SALT: u64 = 0x636c_6f75_6400_0001;

/// Entries kept per cache before it is emptied; far above any real play area.
const CACHE_LIMIT: usize = 1 << 21;

impl Clone for WorldGen {
    fn clone(&self) -> WorldGen {
        WorldGen::init(self.world_seed)
    }
}

impl WorldGen {
    pub fn init(seed: u64) -> WorldGen {
        WorldGen {
            world_seed: seed,
            sectors: RefCell::default(),
            links: RefCell::default(),
            reach: RefCell::default(),
            hops: RefCell::default(),
        }
    }

    /// Generate the base properties of a sector from its coordinates.
    pub fn generate_sector(&self, coord: Hex) -> SectorTemplate {
        let key = coord.to_key();
        if let Some(found) = self.sectors.borrow().get(&key) {
            return found.clone();
        }
        let made = self.compute_sector(coord);
        let mut cache = self.sectors.borrow_mut();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, made.clone());
        made
    }

    fn compute_sector(&self, coord: Hex) -> SectorTemplate {
        if is_hub(coord) {
            return SectorTemplate {
                coord,
                zone: Zone::from_distance(coord.dist_from_origin()),
                terrain: TerrainType::Empty,
                metal_density: Density::None,
                crystal_density: Density::None,
                deut_density: Density::None,
                ore_units: [0.0; 3],
                npc_template: None,
                has_derelict: false,
            };
        }
        let seed = self.sector_seed(coord);
        let mut rng = DeterministicRng::new(seed);
        let dist = coord.dist_from_origin();
        let zone = Zone::from_distance(dist);

        let cloud = self.cloud_depth(coord);
        let terrain = if cloud.is_some() { TerrainType::Nebula } else { self.roll_terrain(&mut rng, zone) };
        let mut resources = self.roll_resources(&mut rng, zone, terrain, dist);
        if let Some(depth) = cloud {
            resources.deut = CLOUD_DEUT_DENSITY[depth.min(CLOUD_DEUT_DENSITY.len() - 1)];
        }
        let npc_template = self.roll_npc_presence(&mut rng, coord);
        let has_derelict = self.roll_derelict(&mut rng, terrain, coord);

        SectorTemplate {
            coord,
            zone,
            terrain,
            metal_density: resources.metal,
            crystal_density: resources.crystal,
            deut_density: resources.deut,
            ore_units: [resources.metal, resources.crystal, resources.deut].map(|d| ore_reserve_units(d, dist)),
            npc_template,
            has_derelict,
        }
    }

    /// Determine which edges from a hex are traversable.
    /// Returns a bitmask of the 6 directions (bit 0 = east, etc.).
    pub fn sector_connections(&self, coord: Hex) -> u8 {
        let mut mask: u8 = 0;
        let mut connected_count: u32 = 0;
        let mut last_dir: u32 = 0;

        for (i, &dir) in crate::hex::HexDirection::ALL.iter().enumerate() {
            let neighbor = coord.neighbor(dir);
            if self.edge_exists(coord, neighbor) {
                mask |= 1u8 << i;
                connected_count += 1;
                last_dir = i as u32;
            }
        }

        // Guarantee: at least 1 connection.
        if connected_count == 0 {
            mask |= 1u8 << last_dir;
        }

        mask
    }

    /// Get list of traversable neighbor coordinates.
    pub fn connected_neighbors(&self, coord: Hex) -> ConnectedList {
        let key = coord.to_key();
        if let Some(found) = self.links.borrow().get(&key) {
            return *found;
        }
        let made = self.compute_neighbors(coord);
        let mut cache = self.links.borrow_mut();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, made);
        made
    }

    fn compute_neighbors(&self, coord: Hex) -> ConnectedList {
        let mask = self.sector_connections(coord);
        let mut result = ConnectedList::new();

        for (i, &dir) in crate::hex::HexDirection::ALL.iter().enumerate() {
            if mask & (1u8 << i) != 0 {
                result.push(coord.neighbor(dir));
            }
        }

        result
    }

    /// Every sector within `max_hops` jumps of `origin` along open lanes,
    /// nearest first (the origin itself excluded).
    pub fn reachable_within(&self, origin: Hex, max_hops: u8) -> Arc<[Hex]> {
        let key = (origin.to_key(), max_hops);
        if let Some(found) = self.reach.borrow().get(&key) {
            return Arc::clone(found);
        }
        let mut visited: KeySet<u32> = KeySet::default();
        let mut frontier = vec![origin];
        let mut result: Vec<Hex> = Vec::new();
        visited.insert(origin.to_key());
        for _ in 0..max_hops {
            let mut next = Vec::new();
            for coord in &frontier {
                for n in self.connected_neighbors(*coord).slice() {
                    if !visited.insert(n.to_key()) {
                        continue;
                    }
                    next.push(*n);
                    result.push(*n);
                }
            }
            frontier = next;
        }
        let found: Arc<[Hex]> = result.into();
        let mut cache = self.reach.borrow_mut();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, Arc::clone(&found));
        found
    }

    /// Fewest jumps from `from` to `to` along open lanes, None beyond `max_hops`.
    pub fn lane_hops(&self, from: Hex, to: Hex, max_hops: u8) -> Option<u32> {
        if from == to {
            return Some(0);
        }
        let key = (from.to_key(), to.to_key(), max_hops);
        if let Some(found) = self.hops.borrow().get(&key) {
            return *found;
        }
        let goal = to.to_key();
        let mut visited: KeySet<u32> = KeySet::from_iter([from.to_key()]);
        let mut frontier = vec![from];
        let mut found = None;
        'search: for hops in 1..=u32::from(max_hops) {
            let mut next = Vec::new();
            for coord in &frontier {
                for n in self.connected_neighbors(*coord).slice() {
                    if !visited.insert(n.to_key()) {
                        continue;
                    }
                    if n.to_key() == goal {
                        found = Some(hops);
                        break 'search;
                    }
                    next.push(*n);
                }
            }
            frontier = next;
        }
        let mut cache = self.hops.borrow_mut();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, found);
        found
    }

    /// Check if an edge exists between two adjacent hexes.
    /// Edges touching the central hub always survive — the hub is guaranteed
    /// fully connected. (The Zig original relied on hash luck for this; the
    /// Wyhash→Xxh3 swap broke that luck, so the guarantee is explicit here.)
    fn edge_exists(&self, a: Hex, b: Hex) -> bool {
        if a == Hex::ORIGIN || b == Hex::ORIGIN {
            return true;
        }
        let key_a = u64::from(a.to_key());
        let key_b = u64::from(b.to_key());
        let pair = self.hash_two(key_a.min(key_b), key_a.max(key_b));
        let edge_seed = self.hash_two(pair, self.world_seed);

        let max_dist = a.dist_from_origin().max(b.dist_from_origin());
        let zone = Zone::from_distance(max_dist);
        let survival_pct = zone.edge_survival_pct(max_dist);

        ((edge_seed % 100) as u8) < survival_pct
    }

    fn roll_terrain(&self, rng: &mut DeterministicRng, zone: Zone) -> TerrainType {
        let roll = rng.range(0..=100);
        match zone {
            Zone::CentralHub => TerrainType::Empty,
            Zone::InnerRing => {
                if roll < 55 {
                    TerrainType::AsteroidField
                } else if roll < 70 {
                    TerrainType::Nebula
                } else if roll < 80 {
                    TerrainType::DebrisField
                } else {
                    TerrainType::Empty
                }
            }
            Zone::OuterRing => {
                if roll < 35 {
                    TerrainType::AsteroidField
                } else if roll < 50 {
                    TerrainType::Nebula
                } else if roll < 70 {
                    TerrainType::DebrisField
                } else if roll < 75 {
                    TerrainType::Anomaly
                } else {
                    TerrainType::Empty
                }
            }
            Zone::Wandering => {
                if roll < 30 {
                    TerrainType::AsteroidField
                } else if roll < 45 {
                    TerrainType::Nebula
                } else if roll < 65 {
                    TerrainType::DebrisField
                } else if roll < 80 {
                    TerrainType::Anomaly
                } else {
                    TerrainType::Empty
                }
            }
        }
    }

    fn roll_resources(
        &self,
        rng: &mut DeterministicRng,
        zone: Zone,
        terrain: TerrainType,
        dist: u16,
    ) -> ResourceRoll {
        if terrain == TerrainType::Empty {
            return ResourceRoll {
                metal: Density::None,
                crystal: Density::None,
                deut: Density::None,
            };
        }

        let zone_boost: u8 = match zone {
            Zone::CentralHub => 0,
            Zone::InnerRing => 30,
            Zone::OuterRing => 40,
            Zone::Wandering => 50,
        };

        ResourceRoll {
            metal: roll_density(
                rng,
                dist,
                if terrain == TerrainType::AsteroidField {
                    60 + zone_boost
                } else {
                    20
                },
            ),
            crystal: roll_density(
                rng,
                dist,
                if terrain == TerrainType::AsteroidField || terrain == TerrainType::Nebula {
                    40 + zone_boost
                } else {
                    10
                },
            ),
            deut: roll_density(
                rng,
                dist,
                if terrain == TerrainType::Nebula {
                    50 + zone_boost
                } else {
                    15
                },
            ),
        }
    }

    fn roll_npc_presence(&self, rng: &mut DeterministicRng, coord: Hex) -> Option<NpcTemplate> {
        let dist = coord.dist_from_origin();
        if dist == 0 || f32::from(rng.range(0..=99)) >= npc_presence_pct(dist) {
            return None;
        }
        let (ship_class, count, stat_multiplier) = npc_composition(dist);
        let behavior = if f32::from(rng.range(0..=99)) < npc_passive_share(dist) * 100.0 {
            NpcBehaviorType::Passive
        } else if dist <= 15 {
            NpcBehaviorType::Patrol
        } else if dist <= 25 {
            NpcBehaviorType::Aggressive
        } else {
            NpcBehaviorType::Swarm
        };
        if behavior != NpcBehaviorType::Passive && is_home_ground(coord) {
            return None;
        }
        Some(NpcTemplate { ship_class, count, behavior, stat_multiplier })
    }

    /// Dead ships drift where other ships died: debris fields and the
    /// empty dark, and only beyond the shipping lanes near the hub. Never on
    /// a home seat: a boarding can wake a guard.
    fn roll_derelict(&self, rng: &mut DeterministicRng, terrain: TerrainType, coord: Hex) -> bool {
        if coord.dist_from_origin() < crate::constants::DERELICT_MIN_DIST || is_home_seat(coord) {
            return false;
        }
        if terrain != TerrainType::DebrisField && terrain != TerrainType::Empty {
            return false;
        }
        rng.range(0..=99) < crate::constants::DERELICT_SPAWN_PCT
    }

    /// How many hexes `coord` stands from the centre of the deuterium cloud
    /// that holds it, if one does. Decided from the sector's own cell and the
    /// eight around it, so no neighbour state is read.
    pub fn cloud_depth(&self, coord: Hex) -> Option<usize> {
        if coord.dist_from_origin() < CLOUD_MIN_DIST || is_home_ground(coord) {
            return None;
        }
        let (cell_q, cell_r) = (i32::from(coord.q).div_euclid(CLOUD_CELL), i32::from(coord.r).div_euclid(CLOUD_CELL));
        let mut found: Option<usize> = None;
        for cq in cell_q - 1..=cell_q + 1 {
            for cr in cell_r - 1..=cell_r + 1 {
                let Some(centre) = self.cloud_centre(cq, cr) else { continue };
                let ring = u32::from(centre.dist_from_origin());
                let radius = (CLOUD_RADIUS_BASE + CLOUD_RADIUS_PER_RING * ring).min(CLOUD_RADIUS_MAX);
                let depth = u32::from(Hex::distance(&centre, &coord));
                if depth * 100 <= radius {
                    let depth = depth as usize;
                    found = Some(found.map_or(depth, |best| best.min(depth)));
                }
            }
        }
        found
    }

    fn cloud_centre(&self, cell_q: i32, cell_r: i32) -> Option<Hex> {
        let cell = (u64::from(cell_q as u32) << 32) | u64::from(cell_r as u32);
        let h = self.hash_two(cell, self.world_seed ^ CLOUD_SALT);
        if h % 100 >= CLOUD_PCT {
            return None;
        }
        let cell_size = CLOUD_CELL as u64;
        let q = cell_q * CLOUD_CELL + ((h >> 16) % cell_size) as i32;
        let r = cell_r * CLOUD_CELL + ((h >> 32) % cell_size) as i32;
        let centre = Hex { q: i16::try_from(q).ok()?, r: i16::try_from(r).ok()? };
        (centre.dist_from_origin() >= CLOUD_MIN_DIST && !is_home_ground(centre)).then_some(centre)
    }

    /// The seed every roll about `coord` in this world starts from.
    pub fn sector_seed(&self, coord: Hex) -> u64 {
        self.hash_two(u64::from(coord.to_key()), self.world_seed)
    }

    fn hash_two(&self, a: u64, b: u64) -> u64 {
        let mut hasher = Xxh3::new();
        hasher.write(&a.to_le_bytes());
        hasher.write(&b.to_le_bytes());
        hasher.finish()
    }
}

struct ResourceRoll {
    metal: Density,
    crystal: Density,
    deut: Density,
}

fn roll_density(rng: &mut DeterministicRng, dist: u16, chance: u8) -> Density {
    if rng.range(0..=100) >= chance {
        return Density::None;
    }
    let [sparse, moderate, rich, _] = ore_density_odds(dist);
    let quality = f32::from(rng.range(0..=99)) / 100.0;
    if quality < sparse {
        Density::Sparse
    } else if quality < sparse + moderate {
        Density::Moderate
    } else if quality < sparse + moderate + rich {
        Density::Rich
    } else {
        Density::Pristine
    }
}

/// Simple deterministic RNG based on xxhash.
struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    fn new(seed: u64) -> Self {
        DeterministicRng { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let mut hasher = Xxh3::new();
        hasher.write(&self.state.to_le_bytes());
        hasher.finish()
    }

    fn range(&mut self, range: std::ops::RangeInclusive<u8>) -> u8 {
        let lo = *range.start();
        let hi = *range.end();
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u64() % span) as u8
    }
}

#[derive(Debug, Clone)]
pub struct NpcTemplate {
    pub ship_class: ShipClass,
    pub count: u8,
    pub behavior: NpcBehaviorType,
    pub stat_multiplier: f32,
}

/// How a hostile group is named in messages: "light fighter patrol", "3x
/// cruiser pack". The numeric fleet id stays in the event fields.
pub fn hostile_label(class: ShipClass, count: usize, behavior: NpcBehaviorType) -> String {
    let noun = match behavior {
        NpcBehaviorType::Passive => "convoy",
        NpcBehaviorType::Patrol => "patrol",
        NpcBehaviorType::Aggressive => "pack",
        NpcBehaviorType::Swarm => "swarm",
    };
    let class = class.label().to_lowercase();
    if count > 1 { format!("{count}x {class} {noun}") } else { format!("{class} {noun}") }
}

impl NpcTemplate {
    pub fn label(&self) -> String {
        hostile_label(self.ship_class, usize::from(self.count), self.behavior)
    }

    /// Combined power of the group as spawned.
    pub fn power(&self) -> f32 {
        ship_power(self.ship_class) * self.stat_multiplier * f32::from(self.count)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NpcBehaviorType {
    Passive,
    Patrol,
    Aggressive,
    Swarm,
}

impl NpcBehaviorType {
    /// The most jumps a group of this behaviour follows a fleet that left it.
    pub fn chase_hexes(self) -> u8 {
        crate::constants::CHASE_HEXES[self as usize]
    }
}

#[derive(Debug, Clone)]
pub struct SectorTemplate {
    pub coord: Hex,
    pub zone: Zone,
    pub terrain: TerrainType,
    pub metal_density: Density,
    pub crystal_density: Density,
    pub deut_density: Density,
    /// Raw units the full metal, crystal and deuterium tiles hold
    /// (`scaling::ore_reserve_units`).
    pub ore_units: [f64; 3],
    pub npc_template: Option<NpcTemplate>,
    pub has_derelict: bool,
}

/// Fixed-size list of connected hex neighbors (max 6).
#[derive(Debug, Clone, Copy)]
pub struct ConnectedList {
    items: [Hex; 6],
    len: u8,
}

impl ConnectedList {
    fn new() -> Self {
        ConnectedList {
            items: [Hex::ORIGIN; 6],
            len: 0,
        }
    }

    fn push(&mut self, hex: Hex) {
        if (self.len as usize) < 6 {
            self.items[self.len as usize] = hex;
            self.len += 1;
        }
    }

    pub fn slice(&self) -> &[Hex] {
        &self.items[..self.len as usize]
    }
}
