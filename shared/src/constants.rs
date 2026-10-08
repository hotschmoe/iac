// All tunable game values. Single source of truth for balance.

use serde::{Deserialize, Serialize};

/// Server tick rate.
pub const TICK_RATE_HZ: u64 = 1;
pub const TICK_DURATION_NS: u64 = 1_000_000_000 / TICK_RATE_HZ;

/// World zones defined by cube distance from origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Zone {
    CentralHub,
    InnerRing,
    OuterRing,
    Wandering,
}

impl Zone {
    pub const INNER_RING_RADIUS: u16 = 8;
    pub const OUTER_RING_RADIUS: u16 = 20;

    pub fn from_distance(dist: u16) -> Zone {
        match dist {
            0 => Zone::CentralHub,
            1..=Self::INNER_RING_RADIUS => Zone::InnerRing,
            d if d <= Self::OUTER_RING_RADIUS => Zone::OuterRing,
            _ => Zone::Wandering,
        }
    }

    /// Edge survival probability for procedural connectivity.
    pub fn edge_survival_pct(&self, dist: u16) -> u8 {
        match self {
            Zone::CentralHub => 100,
            Zone::InnerRing => 95,
            Zone::OuterRing => 80,
            Zone::Wandering => {
                // Decreases from 60% at dist 21 to 40% at dist 60+
                let base: u16 = 60;
                let decay = (dist.saturating_sub(20)).min(40) / 2;
                (base - decay) as u8
            }
        }
    }
}

/// Resource density levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Density {
    None = 0,
    Sparse = 1,
    Moderate = 2,
    Rich = 3,
    Pristine = 4,
}

impl Density {
    pub fn harvest_multiplier(self) -> f32 {
        match self {
            Density::None => 0.0,
            Density::Sparse => 0.5,
            Density::Moderate => 1.0,
            Density::Rich => 2.0,
            Density::Pristine => 4.0,
        }
    }

    pub fn depletion_threshold(self) -> f32 {
        match self {
            Density::None => 0.0,
            Density::Sparse => 10.0,
            Density::Moderate => 20.0,
            Density::Rich => 30.0,
            Density::Pristine => 40.0,
        }
    }

    pub fn downgrade(self) -> Density {
        match self {
            Density::Pristine => Density::Rich,
            Density::Rich => Density::Moderate,
            Density::Moderate => Density::Sparse,
            Density::Sparse => Density::None,
            Density::None => Density::None,
        }
    }

    pub fn upgrade(self) -> Density {
        match self {
            Density::None => Density::Sparse,
            Density::Sparse => Density::Moderate,
            Density::Moderate => Density::Rich,
            Density::Rich => Density::Pristine,
            Density::Pristine => Density::Pristine,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Density::None => "None",
            Density::Sparse => "Sparse",
            Density::Moderate => "Moderate",
            Density::Rich => "Rich",
            Density::Pristine => "Pristine",
        }
    }
}

/// Terrain types for sectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum TerrainType {
    Empty,
    AsteroidField,
    Nebula,
    DebrisField,
    Anomaly,
}

impl TerrainType {
    pub fn label(self) -> &'static str {
        match self {
            TerrainType::Empty => "Empty Space",
            TerrainType::AsteroidField => "Asteroid Field",
            TerrainType::Nebula => "Nebula",
            TerrainType::DebrisField => "Debris Field",
            TerrainType::Anomaly => "Anomaly",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            TerrainType::Empty => "·",
            TerrainType::AsteroidField => "*",
            TerrainType::Nebula => "≈",
            TerrainType::DebrisField => "×",
            TerrainType::Anomaly => "?",
        }
    }
}

/// Ship class definitions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ShipClass {
    #[default]
    Scout,
    Corvette,
    Frigate,
    Cruiser,
    Hauler,
}

impl ShipClass {
    pub const ALL: [ShipClass; 5] = [
        ShipClass::Scout,
        ShipClass::Corvette,
        ShipClass::Frigate,
        ShipClass::Cruiser,
        ShipClass::Hauler,
    ];

    pub fn base_stats(self) -> ShipStats {
        match self {
            ShipClass::Scout => ShipStats {
                hull: 30.0,
                shield: 10.0,
                weapon: 5.0,
                speed: 10,
                cargo: 20,
                fuel: 60,
            },
            ShipClass::Corvette => ShipStats {
                hull: 50.0,
                shield: 20.0,
                weapon: 15.0,
                speed: 8,
                cargo: 10,
                fuel: 50,
            },
            ShipClass::Frigate => ShipStats {
                hull: 120.0,
                shield: 60.0,
                weapon: 30.0,
                speed: 5,
                cargo: 30,
                fuel: 80,
            },
            ShipClass::Cruiser => ShipStats {
                hull: 200.0,
                shield: 100.0,
                weapon: 80.0,
                speed: 4,
                cargo: 50,
                fuel: 120,
            },
            ShipClass::Hauler => ShipStats {
                hull: 80.0,
                shield: 20.0,
                weapon: 5.0,
                speed: 6,
                cargo: 200,
                fuel: 100,
            },
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ShipClass::Scout => "Scout",
            ShipClass::Corvette => "Corvette",
            ShipClass::Frigate => "Frigate",
            ShipClass::Cruiser => "Cruiser",
            ShipClass::Hauler => "Hauler",
        }
    }

    /// Build cost in resources.
    pub fn build_cost(self) -> Resources {
        match self {
            ShipClass::Scout => Resources {
                metal: 200.0,
                crystal: 50.0,
                deuterium: 30.0,
            },
            ShipClass::Corvette => Resources {
                metal: 400.0,
                crystal: 100.0,
                deuterium: 60.0,
            },
            ShipClass::Frigate => Resources {
                metal: 1000.0,
                crystal: 400.0,
                deuterium: 200.0,
            },
            ShipClass::Cruiser => Resources {
                metal: 3000.0,
                crystal: 1500.0,
                deuterium: 800.0,
            },
            ShipClass::Hauler => Resources {
                metal: 600.0,
                crystal: 200.0,
                deuterium: 150.0,
            },
        }
    }

    /// Rapid-fire table: returns multiplier against target class (0 = no bonus).
    pub fn rapid_fire_vs(self, target: ShipClass) -> u8 {
        match self {
            ShipClass::Corvette if target == ShipClass::Scout => 3,
            ShipClass::Frigate if target == ShipClass::Corvette => 2,
            ShipClass::Cruiser if target == ShipClass::Frigate => 2,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ShipStats {
    pub hull: f32,
    pub shield: f32,
    pub weapon: f32,
    pub speed: u8,
    pub cargo: u16,
    pub fuel: u16,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Resources {
    pub metal: f32,
    pub crystal: f32,
    pub deuterium: f32,
}

impl Resources {
    // Mirrors the Zig API; intentionally not the std::ops::Add trait.
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, other: Resources) -> Resources {
        Resources {
            metal: self.metal + other.metal,
            crystal: self.crystal + other.crystal,
            deuterium: self.deuterium + other.deuterium,
        }
    }

    // Mirrors the Zig API; intentionally not the std::ops::Sub trait.
    #[allow(clippy::should_implement_trait)]
    pub fn sub(self, other: Resources) -> Resources {
        Resources {
            metal: self.metal - other.metal,
            crystal: self.crystal - other.crystal,
            deuterium: self.deuterium - other.deuterium,
        }
    }

    pub fn scale(self, factor: f32) -> Resources {
        Resources {
            metal: self.metal * factor,
            crystal: self.crystal * factor,
            deuterium: self.deuterium * factor,
        }
    }

    pub fn can_afford(self, cost: Resources) -> bool {
        self.metal >= cost.metal
            && self.crystal >= cost.crystal
            && self.deuterium >= cost.deuterium
    }
}

/// Starting resources for new players.
pub const STARTING_RESOURCES: Resources = Resources {
    metal: 500.0,
    crystal: 300.0,
    deuterium: 100.0,
};

pub const STARTING_SCOUTS: usize = 2;
pub const MAX_FLEETS_PER_PLAYER: usize = 3;

/// Homeworld spawn range (cube distance from origin).
pub const HOMEWORLD_MIN_DIST: u16 = 3;
pub const HOMEWORLD_MAX_DIST: u16 = 6;

/// Cooldowns (in ticks).
pub const MOVE_BASE_COOLDOWN: u16 = 1;
pub const HARVEST_COOLDOWN: u16 = 1;
pub const SCAN_COOLDOWN: u16 = 5;

/// Combat.
pub const DAMAGE_VARIANCE_MIN: f32 = 0.8;
pub const DAMAGE_VARIANCE_MAX: f32 = 1.2;
pub const SHIELD_REGEN_IDLE_TICKS: u16 = 10;

/// Emergency recall.
pub const RECALL_FUEL_MULTIPLIER: f32 = 2.0;
pub const RECALL_DAMAGE_CHANCE_PER_HEX: f32 = 0.02;
pub const RECALL_DAMAGE_CHANCE_CAP: f32 = 0.60;
pub const RECALL_HULL_DAMAGE_MIN: f32 = 0.20;
pub const RECALL_HULL_DAMAGE_MAX: f32 = 0.80;

/// Fuel consumption per hex: fleet_mass * this value.
pub const FUEL_RATE_PER_MASS: f32 = 0.1;

/// Resource regeneration: sectors regen this fraction per tick.
pub const SECTOR_REGEN_RATE: f32 = 0.0001;

/// NPC respawn delays (in ticks at 1Hz).
pub const NPC_RESPAWN_INNER: u64 = 300;
pub const NPC_RESPAWN_OUTER: u64 = 600;
pub const NPC_RESPAWN_WANDERING: u64 = 1200;
pub const NPC_PATROL_INTERVAL: u16 = 15;

pub fn npc_respawn_delay(zone: Zone) -> u64 {
    match zone {
        Zone::CentralHub | Zone::InnerRing => NPC_RESPAWN_INNER,
        Zone::OuterRing => NPC_RESPAWN_OUTER,
        Zone::Wandering => NPC_RESPAWN_WANDERING,
    }
}

/// Active scan: hops revealed from the scanning fleet's position.
/// Fleets carrying a Scout get the extended range; signals are faint
/// contacts detected one hop beyond the reveal radius.
pub const SCAN_BASE_RANGE: u8 = 1;
pub const SCAN_SCOUT_RANGE: u8 = 2;
/// How long actively-scanned sectors keep streaming live updates.
pub const SCAN_REVEAL_TICKS: u64 = 120;

/// Homeworld raids: forecasted pressure, never a random tax.
/// A raid is rolled periodically, announced in advance, resolves against
/// docked ships + DefenseGrid, and losses are hard-capped.
pub const RAID_ROLL_INTERVAL: u64 = 300;
pub const RAID_ROLL_CHANCE: f32 = 0.20;
pub const RAID_WARNING_TICKS: u64 = 60;
pub const RAID_MIN_INTERVAL: u64 = 900;
pub const RAID_MIN_PLAYER_AGE: u64 = 600;
/// Raid fleet targets this fraction of the player's home defense power.
pub const RAID_POWER_FRACTION_MIN: f32 = 0.35;
pub const RAID_POWER_FRACTION_MAX: f32 = 0.55;
/// Loss caps when a raid succeeds (fraction of stockpile).
pub const RAID_LOSS_CAP_METAL: f32 = 0.08;
pub const RAID_LOSS_CAP_CRYSTAL: f32 = 0.08;
pub const RAID_LOSS_CAP_DEUT: f32 = 0.05;
/// Salvage reward for repelling a raid (fraction of raid fleet value).
pub const RAID_DEFENSE_SALVAGE_FRACTION: f32 = 0.30;
/// After losing a raid, no raids for this long (no death spiral).
pub const RAID_SUPPRESS_AFTER_LOSS: u64 = 1800;

/// DefenseGrid: virtual defensive power per level (in "scout units",
/// multiplied by scout combat power). Levels 4+ scale by +35%/level.
pub fn defense_grid_scout_units(level: u8) -> f32 {
    match level {
        0 => 0.0,
        1 => 2.0,   // ≈ 2 scouts
        2 => 5.0,   // ≈ 1 corvette + 2 scouts
        3 => 8.0,   // ≈ 2 corvettes
        n => 8.0 * 1.35f32.powi(n as i32 - 3),
    }
}

/// Salvage.
pub const SALVAGE_FRACTION: f32 = 0.30;
pub const SALVAGE_DESPAWN_TICKS: u32 = 60;

/// Derelict sites: content behind the "derelict transponder" signal.
/// Boarding takes time, pays in loot (sometimes a recoverable ship or a
/// tech cache), and risks waking whatever killed the crew.
pub const DERELICT_SPAWN_PCT: u8 = 8;
pub const DERELICT_MIN_DIST: u16 = 4;
pub const EXPLORE_DURATION_TICKS: u16 = 20;
/// A Scout aboard reads the hulk before breaching (ambush chance down);
/// a Hauler's rigging doubles what the boarding party can strip.
pub const EXPLORE_SCOUT_AMBUSH_REDUCTION: f32 = 0.10;
pub const EXPLORE_HAULER_LOOT_MULTIPLIER: f32 = 2.0;
/// Each failed/aborted boarding leaves the site angrier.
pub const EXPLORE_RETRY_AMBUSH_BUMP: f32 = 0.10;

/// Derelict tier by distance from the hub: bigger wrecks drift farther out.
pub fn derelict_tier(dist: u16) -> u8 {
    match dist {
        0..=8 => 1,
        9..=20 => 2,
        _ => 3,
    }
}

/// (ambush chance, ship recovery chance, tech cache chance) per tier.
pub fn derelict_tier_odds(tier: u8) -> (f32, f32, f32) {
    match tier {
        1 => (0.10, 0.00, 0.00),
        2 => (0.20, 0.15, 0.00),
        _ => (0.30, 0.25, 0.10),
    }
}

/// Loot ranges per tier: (metal, crystal, deut) each as (min, max).
pub fn derelict_loot_ranges(tier: u8) -> [(f32, f32); 3] {
    match tier {
        1 => [(300.0, 700.0), (80.0, 250.0), (0.0, 80.0)],
        2 => [(800.0, 1800.0), (300.0, 900.0), (100.0, 400.0)],
        _ => [(2000.0, 3500.0), (800.0, 1800.0), (300.0, 900.0)],
    }
}

/// Fleet standing orders: how often an auto-piloted fleet re-evaluates,
/// and the default doctrine thresholds.
pub const POLICY_EVAL_INTERVAL: u16 = 3;
pub const POLICY_DEFAULT_MIN_FUEL_PCT: u8 = 30;
pub const POLICY_DEFAULT_CARGO_RETURN_PCT: u8 = 85;
pub const POLICY_DEFAULT_MAX_RANGE: u8 = 4;
/// Engage only if our power ≥ theirs × (this / 10).
pub const POLICY_DEFAULT_ENGAGE_RATIO_X10: u8 = 12;
/// PatrolHome keeps within this many hops of the homeworld.
pub const POLICY_PATROL_RADIUS: u8 = 2;
/// PatrolHome heads home to lick wounds below this average hull fraction.
pub const POLICY_PATROL_MIN_HULL: f32 = 0.60;

/// WebSocket server defaults.
pub const DEFAULT_PORT: u16 = 7777;
pub const DEFAULT_HOST: &str = "127.0.0.1";

/// World seed.
pub const DEFAULT_WORLD_SEED: u64 = 0xDEAD_BEEF_CAFE_BABE;
