// All tunable game values. Single source of truth for balance: the planning
// simulation (docs/design/economy/sim.py) declares none of them and reads
// docs/design/economy/numbers.json, which sim/tests/economy_numbers.rs renders
// from here and from scaling.rs. After changing a number the simulation reads:
//   UPDATE_FIXTURES=1 cargo test -p iac-sim --test economy_numbers

use serde::{Deserialize, Serialize};

/// Bumped when costs, times, production formulas, the moment an order pays
/// or the arithmetic a formula is computed with change: whenever a stored
/// world would play on differently. A world stored under another economy
/// version, a server's file or a single-player save, is refused (start a
/// new world).
pub const ECONOMY_VERSION: u32 = 11;
/// Bumped when sector generation changes.
pub const WORLDGEN_VERSION: u32 = 4;

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
    /// The last ring of the Inner Ring.
    pub const INNER_RING_RADIUS: u16 = 8;
    /// The last ring of the Outer Ring; The Wandering starts one ring further out.
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
    pub const fn harvest_multiplier(self) -> f32 {
        match self {
            Density::None => 0.0,
            Density::Sparse => 0.5,
            Density::Moderate => 1.0,
            Density::Rich => 2.0,
            Density::Pristine => 4.0,
        }
    }

    /// Raw units harvested at this density before it drops one level, at
    /// ring 1 (`scaling::ore_step_units` applies the ring factor): 3, 7, 12
    /// and 20 times `MAP_PAYS`.
    pub fn step_units(self) -> f64 {
        let steps = match self {
            Density::None => 0.0,
            Density::Sparse => 3.0,
            Density::Moderate => 7.0,
            Density::Rich => 12.0,
            Density::Pristine => 20.0,
        };
        steps * MAP_PAYS
    }

    /// Everything a tile of this density holds, at ring 1: 0 / 9 / 30 / 66 / 126.
    pub fn reserve_units(self) -> f64 {
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

/// The drive a ship flies on: the research that speeds it, and by how much a level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Drive {
    Combustion,
    Impulse,
    Hyperspace,
}

impl Drive {
    /// Share of the base speed each level of the drive's research adds.
    pub fn speed_per_level(self) -> f64 {
        match self {
            Drive::Combustion => 0.1,
            Drive::Impulse => 0.2,
            Drive::Hyperspace => 0.3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Drive::Combustion => "Combustion Drive",
            Drive::Impulse => "Impulse Drive",
            Drive::Hyperspace => "Hyperspace Drive",
        }
    }
}

/// OGame's ships, in OGame's order, without the Colony Ship
/// (`docs/design/ogame.md`, sections 5.3 and 6.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ShipClass {
    SmallCargo,
    LargeCargo,
    #[default]
    LightFighter,
    HeavyFighter,
    Cruiser,
    Battleship,
    Recycler,
    EspionageProbe,
    Bomber,
    SolarSatellite,
    Destroyer,
    Deathstar,
    Battlecruiser,
}

impl ShipClass {
    pub const COUNT: usize = 13;
    pub const ALL: [ShipClass; ShipClass::COUNT] = [
        ShipClass::SmallCargo,
        ShipClass::LargeCargo,
        ShipClass::LightFighter,
        ShipClass::HeavyFighter,
        ShipClass::Cruiser,
        ShipClass::Battleship,
        ShipClass::Recycler,
        ShipClass::EspionageProbe,
        ShipClass::Bomber,
        ShipClass::SolarSatellite,
        ShipClass::Destroyer,
        ShipClass::Deathstar,
        ShipClass::Battlecruiser,
    ];

    /// OGame's figures. A unit's hull in a fight is a tenth of its structure.
    pub fn base_stats(self) -> ShipStats {
        let unit = |structure: f32, shield: f32, weapon: f32, cargo: u32, speed: u32, fuel: u32| ShipStats {
            hull: structure / 10.0,
            shield,
            weapon,
            speed,
            cargo,
            fuel,
        };
        match self {
            ShipClass::SmallCargo => unit(4_000.0, 10.0, 5.0, 5_000, 5_000, 10),
            ShipClass::LargeCargo => unit(12_000.0, 25.0, 5.0, 25_000, 7_500, 50),
            ShipClass::LightFighter => unit(4_000.0, 10.0, 50.0, 50, 12_500, 20),
            ShipClass::HeavyFighter => unit(10_000.0, 25.0, 150.0, 100, 10_000, 75),
            ShipClass::Cruiser => unit(27_000.0, 50.0, 400.0, 800, 15_000, 300),
            ShipClass::Battleship => unit(60_000.0, 200.0, 1_000.0, 1_500, 10_000, 500),
            ShipClass::Recycler => unit(16_000.0, 10.0, 1.0, 20_000, 2_000, 300),
            ShipClass::EspionageProbe => unit(1_000.0, 0.0, 0.0, 5, 100_000_000, 1),
            ShipClass::Bomber => unit(75_000.0, 500.0, 1_000.0, 500, 4_000, 1_000),
            ShipClass::SolarSatellite => unit(2_000.0, 1.0, 1.0, 0, 0, 0),
            ShipClass::Destroyer => unit(110_000.0, 500.0, 2_000.0, 2_000, 5_000, 1_000),
            ShipClass::Deathstar => unit(9_000_000.0, 50_000.0, 200_000.0, 1_000_000, 100, 1),
            ShipClass::Battlecruiser => unit(70_000.0, 400.0, 700.0, 750, 10_000, 250),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ShipClass::SmallCargo => "Small Cargo",
            ShipClass::LargeCargo => "Large Cargo",
            ShipClass::LightFighter => "Light Fighter",
            ShipClass::HeavyFighter => "Heavy Fighter",
            ShipClass::Cruiser => "Cruiser",
            ShipClass::Battleship => "Battleship",
            ShipClass::Recycler => "Recycler",
            ShipClass::EspionageProbe => "Espionage Probe",
            ShipClass::Bomber => "Bomber",
            ShipClass::SolarSatellite => "Solar Satellite",
            ShipClass::Destroyer => "Destroyer",
            ShipClass::Deathstar => "Deathstar",
            ShipClass::Battlecruiser => "Battlecruiser",
        }
    }

    /// Build cost in resources.
    pub fn build_cost(self) -> Resources {
        let cost = |metal: f64, crystal: f64, deuterium: f64| Resources { metal, crystal, deuterium };
        match self {
            ShipClass::SmallCargo => cost(2_000.0, 2_000.0, 0.0),
            ShipClass::LargeCargo => cost(6_000.0, 6_000.0, 0.0),
            ShipClass::LightFighter => cost(3_000.0, 1_000.0, 0.0),
            ShipClass::HeavyFighter => cost(6_000.0, 4_000.0, 0.0),
            ShipClass::Cruiser => cost(20_000.0, 7_000.0, 2_000.0),
            ShipClass::Battleship => cost(45_000.0, 15_000.0, 0.0),
            ShipClass::Recycler => cost(10_000.0, 6_000.0, 2_000.0),
            ShipClass::EspionageProbe => cost(0.0, 1_000.0, 0.0),
            ShipClass::Bomber => cost(50_000.0, 25_000.0, 15_000.0),
            ShipClass::SolarSatellite => cost(0.0, 2_000.0, 500.0),
            ShipClass::Destroyer => cost(60_000.0, 50_000.0, 15_000.0),
            ShipClass::Deathstar => cost(5_000_000.0, 4_000_000.0, 1_000_000.0),
            ShipClass::Battlecruiser => cost(30_000.0, 40_000.0, 15_000.0),
        }
    }

    /// The drive the class is built with. Two classes change drive with
    /// research (`scaling::ship_drive`); a Solar Satellite has none.
    pub fn drive(self) -> Option<Drive> {
        match self {
            ShipClass::SmallCargo
            | ShipClass::LargeCargo
            | ShipClass::LightFighter
            | ShipClass::Recycler
            | ShipClass::EspionageProbe => Some(Drive::Combustion),
            ShipClass::HeavyFighter | ShipClass::Cruiser | ShipClass::Bomber => Some(Drive::Impulse),
            ShipClass::Battleship | ShipClass::Destroyer | ShipClass::Deathstar | ShipClass::Battlecruiser => {
                Some(Drive::Hyperspace)
            }
            ShipClass::SolarSatellite => None,
        }
    }

    /// A Solar Satellite stays at the homeworld: it is counted there and is
    /// never part of a fleet.
    pub fn flies(self) -> bool {
        self != ShipClass::SolarSatellite
    }

    /// `TANK_JUMPS`: jumps a full tank holds, before the Deuterium Tank.
    /// OGame has no tank; this column is this game's and provisional. It
    /// trades reach against the worth of docks and of deuterium in a hold.
    pub fn tank_jumps(self) -> u32 {
        match self {
            ShipClass::EspionageProbe => 20,
            ShipClass::SmallCargo | ShipClass::LargeCargo | ShipClass::Recycler => 12,
            ShipClass::LightFighter | ShipClass::HeavyFighter => 10,
            ShipClass::Cruiser | ShipClass::Battlecruiser => 8,
            ShipClass::Battleship | ShipClass::Bomber | ShipClass::Destroyer | ShipClass::Deathstar => 6,
            ShipClass::SolarSatellite => 0,
        }
    }

    /// Raw units a tick the ship extracts at density multiplier 1. This
    /// game's column, provisional: the Recycler is the ship that mines.
    pub fn harvest_power(self) -> f64 {
        match self {
            ShipClass::Recycler => 5.0,
            ShipClass::SmallCargo | ShipClass::LargeCargo => 1.0,
            ShipClass::SolarSatellite => 0.0,
            _ => 0.5,
        }
    }
}

/// A class's figures before research. `fuel` is what one jump burns; the
/// tank is `ShipClass::tank_jumps` of it.
#[derive(Debug, Clone, Copy)]
pub struct ShipStats {
    pub hull: f32,
    pub shield: f32,
    pub weapon: f32,
    pub speed: u32,
    pub cargo: u32,
    pub fuel: u32,
}

/// Amounts of the three resources. An amount is 64-bit wherever it is held,
/// carried, priced or sent, and so is a fleet's fuel: a 32-bit float steps by
/// a quarter at two and a half million, and a mine adds less than that a tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Resources {
    pub metal: f64,
    pub crystal: f64,
    pub deuterium: f64,
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

    pub fn scale(self, factor: f64) -> Resources {
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

    /// What one unit counts toward score, and its par value at the exchange.
    pub fn weight(self) -> f64 {
        match self {
            ResourceKind::Metal => 1.0,
            ResourceKind::Crystal => 1.5,
            ResourceKind::Deuterium => 2.0,
        }
    }
}

impl Resources {
    pub fn get(&self, kind: ResourceKind) -> f64 {
        match kind {
            ResourceKind::Metal => self.metal,
            ResourceKind::Crystal => self.crystal,
            ResourceKind::Deuterium => self.deuterium,
        }
    }

    pub fn get_mut(&mut self, kind: ResourceKind) -> &mut f64 {
        match kind {
            ResourceKind::Metal => &mut self.metal,
            ResourceKind::Crystal => &mut self.crystal,
            ResourceKind::Deuterium => &mut self.deuterium,
        }
    }

    /// Add `gain` without pushing any component past `cap`; a component
    /// already above its cap stays where it is.
    pub fn add_capped(self, gain: Resources, cap: Resources) -> Resources {
        let one = |stock: f64, gain: f64, cap: f64| if stock >= cap { stock } else { (stock + gain).min(cap) };
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

    pub fn total(&self) -> f64 {
        self.metal + self.crystal + self.deuterium
    }
}

/// Starting resources for new players.
pub const STARTING_RESOURCES: Resources = Resources {
    metal: 500.0,
    crystal: 300.0,
    deuterium: 100.0,
};

/// What a new empire flies. OGame starts with no ships; this is this game's
/// and provisional. It sets what an empire can do on its first day.
pub const STARTING_SHIPS: &[(ShipClass, u32)] = &[(ShipClass::LightFighter, 2)];

/// What the map pays, against the game as it was before OGame's catalogue:
/// ore in a tile, a derelict's hold, a group's wreckage and the outpost's
/// cost, store and yield are each this many times what they were. OGame's
/// mines make three times what this game's did, and the map is to stay the
/// share of an empire's income the owner put it at. Provisional.
pub const MAP_PAYS: f64 = 3.0;

/// The highest temperature of every homeworld, in degrees. OGame gives each
/// planet its own; here all homes are alike. It sets the Deuterium
/// Synthesizer's output (x1.20) and a Solar Satellite's energy (30).
/// Provisional.
pub const HOME_TEMPERATURE_MAX: i32 = 40;

/// A jump takes `max(1, ceil(sqrt(JUMP_SPEED_REF / speed)))` ticks, at the
/// speed of the fleet's slowest ship: OGame's law of flight time, for one
/// hop. Provisional: it trades how much speed matters against how long
/// every trip takes.
pub const JUMP_SPEED_REF: f64 = 50_000.0;

/// Each level of the Deuterium Tank adds this share to every fleet's tank.
/// Provisional.
pub const TANK_PER_DEUT_TANK_LEVEL: f64 = 0.25;
/// Each level of Energy Technology takes this share off what a jump burns,
/// up to `FUEL_SAVING_MAX`. Provisional.
pub const FUEL_SAVING_PER_LEVEL: f64 = 0.05;
pub const FUEL_SAVING_MAX: f64 = 0.5;
/// Each of the first `HARVEST_YIELD_LEVELS` levels of Laser Technology adds
/// this share to what a unit of ore yields. Provisional.
pub const HARVEST_YIELD_PER_LEVEL: f64 = 0.20;
pub const HARVEST_YIELD_LEVELS: u8 = 5;
/// The levels of Computer Technology that each open one more building slot.
/// Provisional.
pub const BUILD_SLOT_LEVELS: [u8; 2] = [3, 6];
/// Each level of Hyperspace Technology takes this much off the chance that
/// an emergency jump harms a ship. Provisional.
pub const RECALL_SAVING_PER_LEVEL: f32 = 0.05;
/// Hops around the homeworld seen for each level of Espionage Technology.
/// Provisional.
pub const SENSOR_HOPS_PER_LEVEL: u8 = 1;

/// Fleets that may be away from the homeworld at once.
pub const MAX_FLEETS_PER_PLAYER: usize = 3;
/// All fleets a player may own, docked or deployed; splits stop here.
pub const MAX_FLEETS_TOTAL: usize = 8;
/// The most ships one fleet holds. It bounds the worst round of a fight.
pub const MAX_FLEET_SHIPS: u32 = 5_000;

/// Rings that hold the home seats (cube distance from origin); see `world::is_home_seat`.
pub const HOMEWORLD_MIN_DIST: u16 = 3;
pub const HOMEWORLD_MAX_DIST: u16 = 5;

/// Cooldowns (in ticks).
pub const MOVE_BASE_COOLDOWN: u16 = 1;
pub const HARVEST_COOLDOWN: u16 = 1;
/// Ticks a fleet takes no order after a scan.
pub const SCAN_COOLDOWN: u16 = 5;

/// The fight (`docs/design/ogame.md`, section 7.1). A unit whose shield a
/// shot has left at zero and whose hull stands at or under this many
/// hundredths of full explodes with probability one minus its hull's share.
/// OGame's.
pub const EXPLODE_BELOW_PCT: u32 = 70;
/// A fight in open space ends when this many rounds in a row took no hull
/// from either side. OGame's.
pub const STANDOFF_ROUNDS: u16 = 1;
/// The odds of a fight are the real round played `ODDS_RUNS` times with
/// fixed dice, on forces scaled down together until the larger has at most
/// `ODDS_UNITS` units, for at most `ODDS_ROUNDS` rounds each. A smaller
/// fight is cheaper to play and turns more on its dice, so it is played
/// more often, as many times as the same work pays for and at most
/// `ODDS_RUNS_MOST`. They trade how steady the odds are against what they
/// cost.
pub const ODDS_RUNS: u32 = 8;
pub const ODDS_RUNS_MOST: u32 = 64;
pub const ODDS_UNITS: u32 = 200;
pub const ODDS_ROUNDS: u32 = 64;

/// Flight (`docs/design/ogame.md`, section 7.4). A fleet that orders a jump
/// out of a fight stops firing and is fired on until it is gone: for one
/// round fewer than its jump takes ticks, and never fewer than this.
/// Provisional: the least any flight costs.
pub const FLEE_MIN_ROUNDS: u16 = 1;
/// The jump out of a fight burns this many times a jump's fuel. Provisional;
/// the emergency recall's is `RECALL_FUEL_MULTIPLIER`.
pub const FLEE_FUEL_MULTIPLIER: f64 = 1.0;
/// Pursuit. The most jumps a group makes after a fleet that left it before
/// it has been home again, by its behaviour: Passive, Patrol, Aggressive,
/// Swarm. Provisional: how far the outer rings follow.
pub const CHASE_HEXES: [u8; 4] = [0, 1, 2, 2];
/// A group follows only what it was beating: its power when the fight broke
/// off must be at least this many times the fleet's. Provisional.
pub const CHASE_MIN_RATIO: f32 = 1.0;
/// A group's ships jump on a drive this many levels up for every
/// `NPC_DRIVE_RINGS_PER_LEVEL` rings past `NPC_DRIVE_FREE_RINGS`, and on a
/// bare one up to there. Provisional: how fast a fleet must be to outrun a
/// ring.
pub const NPC_DRIVE_FREE_RINGS: u16 = 8;
pub const NPC_DRIVE_RINGS_PER_LEVEL: u16 = 4;

/// Pirates heal (section 7.5). A group that has fought keeps its losses and
/// its damage; at rest in its own sector it is whole again after this share
/// of the time a destroyed group takes to return
/// (`scaling::npc_respawn_hours`, an economy timer), in `NPC_HEAL_STEPS`
/// steps: at each, that part of the ships it was generated with return, as
/// far as it is short, and every damaged ship regains that part of its full
/// hull. Provisional, and the owner's range: "an hour" to "a few hours" at
/// pace 1.
pub const NPC_HEAL_SHARE: f64 = 0.5;
pub const NPC_HEAL_STEPS: u32 = 10;

/// Repair. The home dock makes every ship of a fleet whole for this share of
/// the ship's cost times the share of hull restored; the outpost's dock for
/// its own share, paid from its store. Provisional: they trade the cost of
/// a lost fight against how often a fleet goes out.
pub const REPAIR_COST_SHARE_HOME: f64 = 0.0;
pub const REPAIR_COST_SHARE_OUTPOST: f64 = 0.10;
/// Ticks a fleet takes no order after a dock has mended it. Provisional.
pub const REPAIR_TICKS: u16 = 0;

/// Emergency recall.
pub const RECALL_FUEL_MULTIPLIER: f64 = 2.0;
pub const RECALL_DAMAGE_CHANCE_PER_HEX: f32 = 0.02;
pub const RECALL_DAMAGE_CHANCE_CAP: f32 = 0.60;
pub const RECALL_HULL_DAMAGE_MIN: f32 = 0.20;
pub const RECALL_HULL_DAMAGE_MAX: f32 = 0.80;

/// A fleet stranded away from home (idle, fuel below one hop) regains one
/// hop of fuel over this many ticks from empty: five minutes at 1 Hz.
pub const STRANDED_RECOVERY_TICKS: u64 = 300;

/// Deuterium paid per fuel unit topped up at the homeworld. Not scaled by
/// pace: fuel is the price of range, not an economy timer.
pub const FUEL_DEUT_PER_UNIT: f64 = 0.08;

/// The share of the dock's rate that `refine` loses: deuterium turned into
/// fuel from a fleet's hold yields `scaling::refined_fuel_per_deuterium`.
/// Provisional, and the owner's ("like 15%"): it trades the reach of a fleet
/// with cargo ships against the worth of the docks as places to fill up.
pub const REFINE_LOSS: f64 = 0.15;

/// Active scan: hops revealed from the scanning fleet's position.
/// Fleets carrying an Espionage Probe get the extended range; signals are
/// faint contacts detected one hop beyond the reveal radius.
pub const SCAN_BASE_RANGE: u8 = 1;
/// The scan range of a fleet with an Espionage Probe aboard.
pub const SCAN_PROBE_RANGE: u8 = 2;
/// How long actively-scanned sectors keep streaming live updates.
pub const SCAN_REVEAL_TICKS: u64 = 120;

/// Homeworld raids: forecasted pressure, never a random tax. A raid is
/// rolled periodically, announced in advance, sized from the player's
/// economy points (`scaling::raid_power_base`) and resolved against docked
/// ships, Solar Satellites and defences. Losses are hard-capped.
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
pub const RAID_LOSS_CAP_METAL: f64 = 0.08;
pub const RAID_LOSS_CAP_CRYSTAL: f64 = 0.08;
pub const RAID_LOSS_CAP_DEUT: f64 = 0.05;
/// Salvage reward for repelling a raid (fraction of raid fleet value).
pub const RAID_DEFENSE_SALVAGE_FRACTION: f64 = 0.30;
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
/// (at least one) and of the Solar Satellites: ships parked at home defend,
/// and when the defence fails they are what the raiders reach.
pub const RAID_LOST_SHIP_FRACTION: f32 = 0.25;
/// An empire is raided once its Shipyard stands at this level, or it has
/// any defence. Provisional.
pub const RAID_MIN_SHIPYARD: u8 = 2;

/// Wreckage piles drift away after this long. Piles are worth thousands at
/// a fast pace and a cargo ship's round trip from the outer ring takes about a
/// minute, so 60 s lost most of them.
pub const SALVAGE_DESPAWN_TICKS: u32 = 180;

/// A pile on the owner's own homeworld that nobody can scoop raises an alert
/// when it has this long left.
pub const HOME_SALVAGE_WARN_TICKS: u32 = 60;

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
/// Ticks a boarding takes, at every pace.
pub const EXPLORE_DURATION_TICKS: u16 = 20;
/// An Espionage Probe aboard reads the hulk before breaching (ambush chance
/// down); a Recycler's rigging doubles what the boarding party can strip.
pub const EXPLORE_PROBE_AMBUSH_REDUCTION: f32 = 0.10;
pub const EXPLORE_RECYCLER_LOOT_MULTIPLIER: f64 = 2.0;
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
pub const POLICY_DEFAULT_ENGAGE_RATIO_X10: u8 = 15;
/// Share of deuterium income, in percent, a fleet under orders may burn.
pub const POLICY_DEFAULT_FUEL_BUDGET_PCT: u8 = 20;
/// At or above this `fuel_budget_pct` the fleet refuels without a bound; it
/// is also the most the setting can be.
pub const POLICY_FUEL_UNBOUNDED_PCT: u8 = 100;
/// The bounds `policy_update` holds the other doctrine thresholds to.
pub const POLICY_MIN_FUEL_PCT_MAX: u8 = 90;
pub const POLICY_CARGO_RETURN_PCT_MIN: u8 = 10;
pub const POLICY_CARGO_RETURN_PCT_MAX: u8 = 100;
pub const POLICY_RANGE_MIN: u8 = 1;
pub const POLICY_RANGE_MAX: u8 = 30;
pub const POLICY_ENGAGE_RATIO_X10_MIN: u8 = 1;
pub const POLICY_ENGAGE_RATIO_X10_MAX: u8 = 100;
/// A fleet under orders leaves a fight once its odds in it, as the fight
/// stands, fall below this / 10: the edge of RISKY. Held to the same bounds
/// as the engage ratio and never above it. Provisional: how long a fleet
/// under orders stays in a losing fight.
pub const POLICY_DEFAULT_DISENGAGE_RATIO_X10: u8 = 9;
pub const POLICY_DISENGAGE_RATIO_X10_MIN: u8 = 1;
pub const POLICY_DISENGAGE_RATIO_X10_MAX: u8 = 100;
/// MineAndReturn treats a sector holding fewer raw units than this as
/// stripped: a tile that has only begun to refill is not worth a jump.
pub const POLICY_MIN_SEAM_UNITS: f64 = 3.0 * MAP_PAYS;
/// A doctrine flies out for ore or wreckage only if what it can load is
/// worth this many times the deuterium the jumps there and back burn, both
/// at their score weights.
pub const POLICY_FUEL_PAYBACK: f32 = 1.5;
/// A fleet under orders whose hull has fallen below this share of full goes
/// home to be mended before it does anything else. Provisional.
pub const POLICY_HULL_READY: f32 = 0.85;
/// How far along the lanes a doctrine looks for its next sector.
pub const POLICY_SEARCH_HOPS: u8 = 32;

/// How far along the lanes the engine looks for the way between two sectors:
/// a fleet's way home or to its outpost, the hops from an outpost or a
/// surveyed site to the homeworld. A way longer than this is not found.
pub const LANE_SEARCH_HOPS: u8 = 64;

/// The hub exchange (`scaling::exchange_return`). These four numbers are a
/// proposal the owner has not signed off. Change them here only:
/// `docs/design/economy/sim.py` reads them from `numbers.json`, which
/// `sim/tests/economy_numbers.rs` renders.
/// The fee on the first unit sold with an empty meter, as a share of par.
pub const EXCHANGE_FEE_MIN: f64 = 0.25;
/// The fee once the meter reads its span or more.
pub const EXCHANGE_FEE_MAX: f64 = 0.75;
/// The meter's span: this share of what the player's mines make in one window.
pub const EXCHANGE_METER_SHARE: f64 = 0.15;
/// One window in pace-1 ticks (attention class); the meter forgets one span per window.
pub const EXCHANGE_WINDOW_TICKS: u64 = 86_400;

/// Outposts (`docs/design/outposts.md`). Every number here is a proposal the
/// owner has not signed off. Change it here only; the simulation reads it
/// from `numbers.json`. None of them is a duration; the build and
/// repair times follow from `OUTPOST_BUILD_RATE` (economy class), the store
/// from `OUTPOST_STORE_BASE` (finds class, like the holds that fill it) and
/// the yield from `OUTPOST_YIELD_PER_HOUR` (economy class). Costs, the
/// platform and the raid do not scale with pace.
/// The rings an outpost may stand in.
pub const OUTPOST_MIN_RING: u16 = 9;
pub const OUTPOST_MAX_RING: u16 = 20;
/// Fewest hexes between two outposts.
pub const OUTPOST_SPACING: u16 = 3;
pub const OUTPOST_MAX_LEVEL: u8 = 3;
/// Level 1; each further level costs `OUTPOST_GROWTH` times the last.
pub const OUTPOST_COST: Resources =
    Resources { metal: 1000.0 * MAP_PAYS, crystal: 500.0 * MAP_PAYS, deuterium: 200.0 * MAP_PAYS };
/// Cost and store grow by this factor a level.
pub const OUTPOST_GROWTH: f64 = 2.5;
/// Metal plus crystal of a level's cost that one game hour of building pays
/// for. It follows `MAP_PAYS` as the cost does, so the knob moves no time.
pub const OUTPOST_BUILD_RATE: f64 = 100.0 * MAP_PAYS;
/// The store at level 1, before the pace multiplier.
pub const OUTPOST_STORE_BASE: Resources =
    Resources { metal: 3000.0 * MAP_PAYS, crystal: 1500.0 * MAP_PAYS, deuterium: 750.0 * MAP_PAYS };
/// Units a game hour, per level, for each point of the site's grade, times the ring factor.
pub const OUTPOST_YIELD_PER_HOUR: f64 = 2.0 * MAP_PAYS;
/// A raid on an outpost is this many times the group its ring spawns.
pub const OUTPOST_RAID_FACTOR: f32 = 2.0;
/// Share of every resource above the protected floor that a lost raid takes.
pub const OUTPOST_RAID_LOSS: f64 = 0.25;
/// A repair costs the par value of this share of the level's cost and takes this share of its build time.
pub const OUTPOST_REPAIR_SHARE: f64 = 0.25;

/// WebSocket server defaults.
pub const DEFAULT_PORT: u16 = 7777;
pub const DEFAULT_HOST: &str = "127.0.0.1";

/// World seed.
pub const DEFAULT_WORLD_SEED: u64 = 0xDEAD_BEEF_CAFE_BABE;
