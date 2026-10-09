// Deterministic procedural world generation.
// Sectors are generated from their coordinates + world seed.
// Only modified sectors need database storage.
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hasher;
use std::sync::Arc;

use xxhash_rust::xxh3::Xxh3;

use crate::constants::{Density, ShipClass, TerrainType, Zone};
use crate::scaling::{npc_composition, npc_passive_share, npc_presence_pct, ore_density_odds, ship_power};
use crate::hex::Hex;

/// Seed-based sector generation. Pure function — same inputs always produce same output.
/// Results are memoised per generator (the map is only ever read at the same
/// sectors tick after tick), which changes speed and nothing else.
pub struct WorldGen {
    pub world_seed: u64,
    sectors: RefCell<HashMap<u32, SectorTemplate>>,
    links: RefCell<HashMap<u32, ConnectedList>>,
    reach: RefCell<HashMap<ReachKey, Arc<[Hex]>>>,
}

/// Origin sector key and hop count of a cached `reachable_within`.
type ReachKey = (u32, u8);

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
        let seed = self.sector_seed(coord);
        let mut rng = DeterministicRng::new(seed);
        let dist = coord.dist_from_origin();
        let zone = Zone::from_distance(dist);

        let terrain = self.roll_terrain(&mut rng, zone);
        let resources = self.roll_resources(&mut rng, zone, terrain, dist);
        let npc_template = self.roll_npc_presence(&mut rng, dist);
        let has_derelict = self.roll_derelict(&mut rng, terrain, dist);

        SectorTemplate {
            coord,
            zone,
            terrain,
            metal_density: resources.metal,
            crystal_density: resources.crystal,
            deut_density: resources.deut,
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
        let mut visited: HashMap<u32, ()> = HashMap::new();
        let mut frontier = vec![origin];
        let mut result: Vec<Hex> = Vec::new();
        visited.insert(origin.to_key(), ());
        for _ in 0..max_hops {
            let mut next = Vec::new();
            for coord in &frontier {
                for n in self.connected_neighbors(*coord).slice() {
                    if visited.insert(n.to_key(), ()).is_some() {
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
        let edge_seed = if key_a < key_b {
            self.hash_two(key_a, key_b)
        } else {
            self.hash_two(key_b, key_a)
        };

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

    fn roll_npc_presence(&self, rng: &mut DeterministicRng, dist: u16) -> Option<NpcTemplate> {
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
        Some(NpcTemplate { ship_class, count, behavior, stat_multiplier })
    }

    /// Dead ships drift where other ships died: debris fields and the
    /// empty dark, and only beyond the shipping lanes near the hub.
    fn roll_derelict(&self, rng: &mut DeterministicRng, terrain: TerrainType, dist: u16) -> bool {
        if dist < crate::constants::DERELICT_MIN_DIST {
            return false;
        }
        if terrain != TerrainType::DebrisField && terrain != TerrainType::Empty {
            return false;
        }
        rng.range(0..=99) < crate::constants::DERELICT_SPAWN_PCT
    }

    fn sector_seed(&self, coord: Hex) -> u64 {
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

/// How a hostile group is named in messages: "scout patrol", "3x corvette
/// pack". The numeric fleet id stays in the event fields.
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

#[derive(Debug, Clone)]
pub struct SectorTemplate {
    pub coord: Hex,
    pub zone: Zone,
    pub terrain: TerrainType,
    pub metal_density: Density,
    pub crystal_density: Density,
    pub deut_density: Density,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_gen_deterministic() {
        let world_gen = WorldGen::init(12345);
        let coord = Hex { q: 5, r: -3 };
        let s1 = world_gen.generate_sector(coord);
        let s2 = world_gen.generate_sector(coord);
        assert_eq!(s1.terrain, s2.terrain);
        assert_eq!(s1.metal_density, s2.metal_density);
    }

    #[test]
    fn generated_groups_follow_the_gradient() {
        use crate::scaling::{npc_power, ship_power};
        let world_gen = WorldGen::init(12345);
        let mut seen = 0;
        for q in -40i16..40 {
            for r in -40i16..40 {
                let coord = Hex { q, r };
                let dist = coord.dist_from_origin();
                let Some(t) = world_gen.generate_sector(coord).npc_template else { continue };
                seen += 1;
                assert!(dist > 0);
                let unit = ship_power(t.ship_class) * t.stat_multiplier;
                assert!(t.count == 1 || t.count as usize == crate::scaling::NPC_MAX_SHIPS || (t.power() - npc_power(dist)).abs() <= unit / 2.0 + 1e-3, "{coord:?}");
                if dist > 15 {
                    assert_ne!(t.behavior, NpcBehaviorType::Passive);
                }
            }
        }
        assert!(seen > 500);
    }

    #[test]
    fn group_presence_rises_with_distance() {
        let world_gen = WorldGen::init(777);
        let share = |lo: u16, hi: u16| {
            let (mut groups, mut total) = (0u32, 0u32);
            for q in -50i16..50 {
                for r in -50i16..50 {
                    let c = Hex { q, r };
                    if (lo..=hi).contains(&c.dist_from_origin()) {
                        total += 1;
                        groups += u32::from(world_gen.generate_sector(c).npc_template.is_some());
                    }
                }
            }
            groups as f32 / total as f32
        };
        let (near, mid, far) = (share(1, 4), share(10, 14), share(30, 40));
        assert!(near < mid && mid < far, "{near} {mid} {far}");
        assert!((near - 0.31).abs() < 0.08 && (far - 0.85).abs() < 0.05);
    }

    #[test]
    fn ore_is_richer_and_tiles_bigger_further_out() {
        use crate::scaling::ore_reserve_units;
        let world_gen = WorldGen::init(4242);
        let mean = |lo: u16, hi: u16| {
            let (mut sum, mut tiles) = (0.0f32, 0u32);
            let (mut level, mut count) = (0.0f32, 0u32);
            for q in -45i16..45 {
                for r in -45i16..45 {
                    let c = Hex { q, r };
                    let d = c.dist_from_origin();
                    if !(lo..=hi).contains(&d) {
                        continue;
                    }
                    let t = world_gen.generate_sector(c);
                    for den in [t.metal_density, t.crystal_density, t.deut_density] {
                        if den != Density::None {
                            sum += ore_reserve_units(den, d);
                            level += den as u8 as f32;
                            count += 1;
                            tiles += 1;
                        }
                    }
                }
            }
            (sum / tiles as f32, level / count as f32)
        };
        let (near_units, near_level) = mean(3, 8);
        let (mid_units, mid_level) = mean(12, 18);
        let (far_units, far_level) = mean(26, 40);
        assert!(near_units < mid_units && mid_units < far_units, "{near_units} {mid_units} {far_units}");
        assert!(near_level < mid_level && mid_level < far_level, "{near_level} {mid_level} {far_level}");
    }

    #[test]
    fn hostiles_are_named_by_what_they_are() {
        assert_eq!(hostile_label(ShipClass::Scout, 1, NpcBehaviorType::Patrol), "scout patrol");
        assert_eq!(hostile_label(ShipClass::Corvette, 3, NpcBehaviorType::Aggressive), "3x corvette pack");
        assert_eq!(hostile_label(ShipClass::Cruiser, 24, NpcBehaviorType::Swarm), "24x cruiser swarm");
        assert_eq!(hostile_label(ShipClass::Frigate, 1, NpcBehaviorType::Passive), "frigate convoy");
    }

    #[test]
    fn edge_symmetry() {
        let world_gen = WorldGen::init(12345);
        let a = Hex { q: 3, r: -1 };
        let b = Hex { q: 4, r: -1 };
        let conn_a = world_gen.sector_connections(a);
        let conn_b = world_gen.sector_connections(b);
        // b is east of a
        let a_has_east = (conn_a & 1) != 0;
        // a is west of b
        let b_has_west = (conn_b & (1 << 3)) != 0;
        assert_eq!(a_has_east, b_has_west);
    }

    #[test]
    fn hub_fully_connected() {
        let world_gen = WorldGen::init(12345);
        let conn = world_gen.sector_connections(Hex::ORIGIN);
        assert_eq!(conn, 0b111111);
    }

    #[test]
    fn at_least_one_connection() {
        let world_gen = WorldGen::init(12345);
        for q in 50..60 {
            let conn = world_gen.sector_connections(Hex { q, r: -30 });
            assert_ne!(conn, 0);
        }
    }
}
