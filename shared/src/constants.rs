// All tunable game values. Single source of truth for balance.

use serde::{Deserialize, Serialize};

/// Bumped when costs, times or production formulas change; a database from
/// another economy version is refused (start a new world).
pub const ECONOMY_VERSION: u32 = 2;
/// Bumped when sector generation changes.
pub const WORLDGEN_VERSION: u32 = 2;

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

    /// Raw units harvested at this density before it drops one level, at
    /// ring 1 (`scaling::ore_step_units` applies the ring factor).
    pub fn step_units(self) -> f32 {
        match self {
            Density::None => 0.0,
            Density::Sparse => 3.0,
            Density::Moderate => 7.0,
            Density::Rich => 12.0,
            Density::Pristine => 20.0,
        }
    }

    /// Everything a tile of this density holds, at ring 1: 0 / 3 / 10 / 22 / 42.
    pub fn reserve_units(self) -> f32 {
        let mut level = self;
        let mut total = 0.0;
        while level != Density::None {
            total += level.step_units();
            level = level.downgrade();
        }
        total
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
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

/// One of the three stockpiled resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Metal,
    Crystal,
    Deuterium,
}

impl ResourceKind {
    pub const ALL: [ResourceKind; 3] = [ResourceKind::Metal, ResourceKind::Crystal, ResourceKind::Deuterium];

    pub fn label(self) -> &'static str {
        match self {
            ResourceKind::Metal => "metal",
            ResourceKind::Crystal => "crystal",
            ResourceKind::Deuterium => "deuterium",
        }
    }
}

impl Resources {
    pub fn get(&self, kind: ResourceKind) -> f32 {
        match kind {
            ResourceKind::Metal => self.metal,
            ResourceKind::Crystal => self.crystal,
            ResourceKind::Deuterium => self.deuterium,
        }
    }

    pub fn get_mut(&mut self, kind: ResourceKind) -> &mut f32 {
        match kind {
            ResourceKind::Metal => &mut self.metal,
            ResourceKind::Crystal => &mut self.crystal,
            ResourceKind::Deuterium => &mut self.deuterium,
        }
    }

    /// Add `gain` without pushing any component past `cap`; a component
    /// already above its cap stays where it is.
    pub fn add_capped(self, gain: Resources, cap: Resources) -> Resources {
        let one = |stock: f32, gain: f32, cap: f32| if stock >= cap { stock } else { (stock + gain).min(cap) };
        Resources {
            metal: one(self.metal, gain.metal, cap.metal),
            crystal: one(self.crystal, gain.crystal, cap.crystal),
            deuterium: one(self.deuterium, gain.deuterium, cap.deuterium),
        }
    }

    /// Component-wise minimum.
    pub fn min(self, other: Resources) -> Resources {
        Resources {
            metal: self.metal.min(other.metal),
            crystal: self.crystal.min(other.crystal),
            deuterium: self.deuterium.min(other.deuterium),
        }
    }

    /// What `self` lacks to pay `cost`: each component is clamped at 0.
    pub fn shortfall(self, cost: Resources) -> Resources {
        Resources {
            metal: (cost.metal - self.metal).max(0.0),
            crystal: (cost.crystal - self.crystal).max(0.0),
            deuterium: (cost.deuterium - self.deuterium).max(0.0),
        }
    }

    pub fn total(&self) -> f32 {
        self.metal + self.crystal + self.deuterium
    }
}

/// Starting resources for new players.
pub const STARTING_RESOURCES: Resources = Resources {
    metal: 500.0,
    crystal: 300.0,
    deuterium: 100.0,
};

pub const STARTING_SCOUTS: usize = 2;
/// Fleets that may be away from the homeworld at once.
pub const MAX_FLEETS_PER_PLAYER: usize = 3;
/// All fleets a player may own, docked or deployed; splits stop here.
pub const MAX_FLEETS_TOTAL: usize = 8;

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

/// A fleet stranded away from home (idle, fuel below one hop) regains one
/// hop of fuel over this many ticks from empty: five minutes at 1 Hz.
pub const STRANDED_RECOVERY_TICKS: u64 = 300;

/// Fuel consumption per hex: fleet_mass * this value.
pub const FUEL_RATE_PER_MASS: f32 = 0.1;

/// Deuterium paid per fuel unit topped up at the homeworld. Not scaled by
/// pace: fuel is the price of range, not an economy timer.
pub const FUEL_DEUT_PER_UNIT: f32 = 0.08;

pub const NPC_PATROL_INTERVAL: u16 = 15;

/// Active scan: hops revealed from the scanning fleet's position.
/// Fleets carrying a Scout get the extended range; signals are faint
/// contacts detected one hop beyond the reveal radius.
pub const SCAN_BASE_RANGE: u8 = 1;
pub const SCAN_SCOUT_RANGE: u8 = 2;
/// How long actively-scanned sectors keep streaming live updates.
pub const SCAN_REVEAL_TICKS: u64 = 120;

/// Homeworld raids: forecasted pressure, never a random tax. A raid is
/// rolled periodically, announced in advance, sized from the player's
/// economy points (`scaling::raid_power_base`) and resolved against docked
/// ships, the Defense Grid and defence structures. Losses are hard-capped.
/// The cadence is attention-class: it follows P^0.75, not P.
pub const RAID_ROLL_INTERVAL: u64 = 21_600;
pub const RAID_ROLL_CHANCE: f32 = 0.30;
/// Each eligible roll that misses raises the next roll's chance by this much,
/// so a raid is never more than a few rolls late (30, 45, 60, 75 percent ...).
pub const RAID_PITY_STEP: f32 = 0.15;
pub const RAID_MIN_INTERVAL: u64 = 64_800;
pub const RAID_MIN_PLAYER_AGE: u64 = 86_400;
/// The raid fleet's power is `raid_power_base` times a roll in this range.
pub const RAID_POWER_ROLL_MIN: f32 = 0.80;
pub const RAID_POWER_ROLL_MAX: f32 = 1.15;
/// Loss caps when a raid succeeds (fraction of stockpile above the
/// protected share).
pub const RAID_LOSS_CAP_METAL: f32 = 0.08;
pub const RAID_LOSS_CAP_CRYSTAL: f32 = 0.08;
pub const RAID_LOSS_CAP_DEUT: f32 = 0.05;
/// Salvage reward for repelling a raid (fraction of raid fleet value).
pub const RAID_DEFENSE_SALVAGE_FRACTION: f32 = 0.30;
/// After losing a raid, no raids for this long (no death spiral).
pub const RAID_SUPPRESS_AFTER_LOSS: u64 = 172_800;
/// A repelled raid wears down structures: this fraction of them, scaled by
/// raid power over defence power (at most 1), is destroyed.
pub const RAID_STRUCTURE_DAMAGE: f32 = 0.5;
/// Of the structures a repelled raid destroys, this share is rebuilt free.
pub const RAID_STRUCTURE_RESTORE: f32 = 0.7;
/// Delay before they are rebuilt, in pace-1 ticks (finds class).
pub const RAID_RESTORE_TICKS: u64 = 600;
/// A lost raid destroys this fraction of the structures, with no rebuild.
pub const RAID_LOST_STRUCTURE_FRACTION: f32 = 0.5;
/// A lost raid also destroys this fraction of the ships docked at home
/// (at least one): ships parked at home defend, and when the defence fails
/// they are what the raiders reach.
pub const RAID_LOST_SHIP_FRACTION: f32 = 0.25;

/// DefenseGrid: virtual defensive power per level (in "scout units",
/// multiplied by scout combat power). Levels 4+ scale by +30%/level. Real
/// structures provide the bulk of home defence.
pub fn defense_grid_scout_units(level: u8) -> f32 {
    match level {
        0 => 0.0,
        1 => 1.0,
        2 => 2.5,
        3 => 4.0,
        n => 4.0 * 1.3f32.powi(n as i32 - 3),
    }
}

/// Wreckage piles drift away after this long. Piles are worth thousands at
/// a fast pace and a hauler round trip from the outer ring takes about a
/// minute, so 60 s lost most of them.
pub const SALVAGE_DESPAWN_TICKS: u32 = 180;

/// Ticks of harvesting summed into one `ResourceHarvested` event.
pub const HARVEST_REPORT_TICKS: u64 = 10;

/// NPC fleets that spawn from a sector's template get `base + sector key` as
/// their id, so a sector view can name the hostile before it materialises and
/// `attack` / `CombatStarted` use the same id. Real counter ids stay far below.
pub const TEMPLATE_NPC_ID_BASE: u64 = 1 << 32;

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
/// A stripped derelict is replaced after 120 game hours, finds class.
pub const DERELICT_RESPAWN_TICKS: u64 = 432_000;

/// Fleet standing orders: how often an auto-piloted fleet re-evaluates,
/// and the default doctrine thresholds.
pub const POLICY_EVAL_INTERVAL: u16 = 3;
pub const POLICY_DEFAULT_MIN_FUEL_PCT: u8 = 10;
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
