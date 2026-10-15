// Single source of truth for all balance formulas, costs, times, and modifiers.
// Pure data/formula file -- no side effects, no allocations.
//
// The buildings, the research, the ships and the defences are classic
// OGame's, with its numbers (`docs/design/ogame.md`, section 6). What this
// game asks of them that OGame does not answer is a constant of its own in
// `constants.rs`, named there as provisional.

use serde::{Deserialize, Serialize};

use crate::constants::{
    Density, Drive, ResourceKind, Resources, ShipClass, ShipStats, BUILD_SLOT_LEVELS, EXCHANGE_FEE_MAX,
    EXCHANGE_FEE_MIN, EXCHANGE_METER_SHARE, EXCHANGE_WINDOW_TICKS, FUEL_DEUT_PER_UNIT, FUEL_SAVING_MAX,
    FUEL_SAVING_PER_LEVEL, HARVEST_YIELD_LEVELS, HARVEST_YIELD_PER_LEVEL, HOME_TEMPERATURE_MAX, JUMP_SPEED_REF,
    MAP_PAYS, NPC_DRIVE_FREE_RINGS, NPC_DRIVE_RINGS_PER_LEVEL, NPC_HEAL_SHARE, NPC_HEAL_STEPS, OUTPOST_BUILD_RATE,
    OUTPOST_COST, OUTPOST_GROWTH, OUTPOST_RAID_FACTOR, OUTPOST_REPAIR_SHARE, OUTPOST_STORE_BASE,
    OUTPOST_YIELD_PER_HOUR, RECALL_SAVING_PER_LEVEL, REFINE_LOSS, RELAY_BASE_MAX, RELAY_COST_BASE, RELAY_COST_GROWTH,
    DEPOT_FILL_JUMPS_BASE, DEPOT_FILL_JUMPS_PER_LEVEL, HUB_COUNT, HUB_PERIOD_TICKS, HUB_WARN_TICKS, RELAY_LEVELS_PER_STEP, RELAY_MIN_RING, SENSOR_HOPS_PER_LEVEL, TANK_PER_DEUT_TANK_LEVEL,
};
use crate::maths;
use crate::pace::Pace;

/// OGame has no ceiling on a level. This game keeps one, as a bound on
/// every table and every cost.
pub const MAX_BUILDING_LEVEL: u8 = 20;
pub const MAX_RESEARCH_LEVEL: u8 = 20;

// ── Building System ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum BuildingType {
    MetalMine = 0,
    CrystalMine,
    DeuteriumSynthesizer,
    SolarPlant,
    FusionReactor,
    RoboticsFactory,
    NaniteFactory,
    Shipyard,
    MetalStorage,
    CrystalStorage,
    DeuteriumTank,
    ResearchLab,
}

impl BuildingType {
    /// Number of building types.
    pub const COUNT: usize = 12;
    pub const ALL: [BuildingType; BuildingType::COUNT] = [
        BuildingType::MetalMine,
        BuildingType::CrystalMine,
        BuildingType::DeuteriumSynthesizer,
        BuildingType::SolarPlant,
        BuildingType::FusionReactor,
        BuildingType::RoboticsFactory,
        BuildingType::NaniteFactory,
        BuildingType::Shipyard,
        BuildingType::MetalStorage,
        BuildingType::CrystalStorage,
        BuildingType::DeuteriumTank,
        BuildingType::ResearchLab,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BuildingType::MetalMine => "Metal Mine",
            BuildingType::CrystalMine => "Crystal Mine",
            BuildingType::DeuteriumSynthesizer => "Deuterium Synthesizer",
            BuildingType::SolarPlant => "Solar Plant",
            BuildingType::FusionReactor => "Fusion Reactor",
            BuildingType::RoboticsFactory => "Robotics Factory",
            BuildingType::NaniteFactory => "Nanite Factory",
            BuildingType::Shipyard => "Shipyard",
            BuildingType::MetalStorage => "Metal Storage",
            BuildingType::CrystalStorage => "Crystal Storage",
            BuildingType::DeuteriumTank => "Deuterium Tank",
            BuildingType::ResearchLab => "Research Lab",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            BuildingType::MetalMine => "Metal",
            BuildingType::CrystalMine => "Crystal",
            BuildingType::DeuteriumSynthesizer => "Deut",
            BuildingType::SolarPlant => "Solar",
            BuildingType::FusionReactor => "Fusion",
            BuildingType::RoboticsFactory => "Robots",
            BuildingType::NaniteFactory => "Nanite",
            BuildingType::Shipyard => "Shipyd",
            BuildingType::MetalStorage => "MStore",
            BuildingType::CrystalStorage => "CStore",
            BuildingType::DeuteriumTank => "DTank",
            BuildingType::ResearchLab => "Lab",
        }
    }

    pub fn from_usize(idx: usize) -> Option<Self> {
        Self::ALL.get(idx).copied()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingLevels {
    pub metal_mine: u8,
    pub crystal_mine: u8,
    pub deuterium_synthesizer: u8,
    pub solar_plant: u8,
    pub fusion_reactor: u8,
    pub robotics_factory: u8,
    pub nanite_factory: u8,
    pub shipyard: u8,
    pub metal_storage: u8,
    pub crystal_storage: u8,
    pub deuterium_tank: u8,
    pub research_lab: u8,
}

impl Default for BuildingLevels {
    /// A new homeworld has a Metal Mine and a Crystal Mine at level 1 and the
    /// Solar Plant that powers exactly those two. OGame starts with nothing.
    fn default() -> Self {
        BuildingLevels {
            metal_mine: 1,
            crystal_mine: 1,
            deuterium_synthesizer: 0,
            solar_plant: 1,
            fusion_reactor: 0,
            robotics_factory: 0,
            nanite_factory: 0,
            shipyard: 0,
            metal_storage: 0,
            crystal_storage: 0,
            deuterium_tank: 0,
            research_lab: 0,
        }
    }
}

impl BuildingLevels {
    pub fn get(&self, building: BuildingType) -> u8 {
        match building {
            BuildingType::MetalMine => self.metal_mine,
            BuildingType::CrystalMine => self.crystal_mine,
            BuildingType::DeuteriumSynthesizer => self.deuterium_synthesizer,
            BuildingType::SolarPlant => self.solar_plant,
            BuildingType::FusionReactor => self.fusion_reactor,
            BuildingType::RoboticsFactory => self.robotics_factory,
            BuildingType::NaniteFactory => self.nanite_factory,
            BuildingType::Shipyard => self.shipyard,
            BuildingType::MetalStorage => self.metal_storage,
            BuildingType::CrystalStorage => self.crystal_storage,
            BuildingType::DeuteriumTank => self.deuterium_tank,
            BuildingType::ResearchLab => self.research_lab,
        }
    }

    pub fn set(&mut self, building: BuildingType, level: u8) {
        match building {
            BuildingType::MetalMine => self.metal_mine = level,
            BuildingType::CrystalMine => self.crystal_mine = level,
            BuildingType::DeuteriumSynthesizer => self.deuterium_synthesizer = level,
            BuildingType::SolarPlant => self.solar_plant = level,
            BuildingType::FusionReactor => self.fusion_reactor = level,
            BuildingType::RoboticsFactory => self.robotics_factory = level,
            BuildingType::NaniteFactory => self.nanite_factory = level,
            BuildingType::Shipyard => self.shipyard = level,
            BuildingType::MetalStorage => self.metal_storage = level,
            BuildingType::CrystalStorage => self.crystal_storage = level,
            BuildingType::DeuteriumTank => self.deuterium_tank = level,
            BuildingType::ResearchLab => self.research_lab = level,
        }
    }
}

// ── Production and energy ────────────────────────────────────────
//
// OGame's, an hour at universe speed 1, which is pace 1:
//
//   metal      floor(30 * L * 1.1^L) * fraction + 20
//   crystal    floor(20 * L * 1.1^L) * fraction + 10
//   deuterium  floor(10 * L * 1.1^L) * (1.28 - 0.002 * Tmax) * fraction
//   fraction   min(1, energy made / energy needed)
//
// The curves are whole numbers of 1.1^L, and 1.1 is 11/10, so they are
// worked in integers: a float puts 10 * 1.1 a hair above 11 and its ceiling
// at 12.

/// `base * level * 1.1^level` an hour: the mine's output at full power.
pub const PRODUCTION_PER_HOUR_METAL: u32 = 30;
pub const PRODUCTION_PER_HOUR_CRYSTAL: u32 = 20;
pub const PRODUCTION_PER_HOUR_DEUTERIUM: u32 = 10;
/// Made with no mine and no energy.
pub const NATURAL_PER_HOUR_METAL: f64 = 20.0;
pub const NATURAL_PER_HOUR_CRYSTAL: f64 = 10.0;
/// `base * level * 1.1^level`: energy a mine needs and a plant makes.
pub const ENERGY_PER_MINE: u32 = 10;
pub const ENERGY_PER_SYNTHESIZER: u32 = 20;
pub const ENERGY_PER_SOLAR_PLANT: u32 = 20;
/// The Fusion Reactor makes `base * level * (1.05 + 0.01 * Energy Technology)^level`
/// and burns `burn * level * 1.1^level` deuterium an hour.
pub const ENERGY_PER_FUSION_REACTOR: f64 = 30.0;
pub const FUSION_BURN_PER_HOUR: u32 = 10;
/// Graviton Technology costs no resources: the homeworld must be making
/// this much energy when the research is ordered and when it starts, and
/// three times as much for each further level.
pub const GRAVITON_ENERGY: u32 = 300_000;
pub const GRAVITON_GROWTH: u32 = 3;

/// `base * level * 1.1^level` as a whole number, cut off or (`up`) raised.
fn tenth_curve(base: u32, level: u8, up: bool) -> u32 {
    let level = u32::from(level);
    let (above, below) = (u128::from(base) * u128::from(level) * 11u128.pow(level), 10u128.pow(level));
    (if up { above.div_ceil(below) } else { above / below }) as u32
}

/// The share of OGame's deuterium figure a homeworld's temperature leaves.
pub fn synthesizer_factor() -> f64 {
    1.28 - 0.002 * f64::from(HOME_TEMPERATURE_MAX)
}

/// What a mine makes an hour at pace 1 with all the energy it needs,
/// without the natural production. 0 for a building that is not a mine.
pub fn mine_output_per_hour(building: BuildingType, level: u8) -> f64 {
    match building {
        BuildingType::MetalMine => f64::from(tenth_curve(PRODUCTION_PER_HOUR_METAL, level, false)),
        BuildingType::CrystalMine => f64::from(tenth_curve(PRODUCTION_PER_HOUR_CRYSTAL, level, false)),
        BuildingType::DeuteriumSynthesizer => {
            f64::from(tenth_curve(PRODUCTION_PER_HOUR_DEUTERIUM, level, false)) * synthesizer_factor()
        }
        _ => 0.0,
    }
}

/// Energy the building needs at `level`; 0 for one that needs none.
pub fn energy_needed_by(building: BuildingType, level: u8) -> u32 {
    match building {
        BuildingType::MetalMine | BuildingType::CrystalMine => tenth_curve(ENERGY_PER_MINE, level, true),
        BuildingType::DeuteriumSynthesizer => tenth_curve(ENERGY_PER_SYNTHESIZER, level, true),
        _ => 0,
    }
}

pub fn solar_plant_energy(level: u8) -> u32 {
    tenth_curve(ENERGY_PER_SOLAR_PLANT, level, false)
}

pub fn fusion_energy(level: u8, energy_technology: u8) -> u32 {
    let base = f64::from(105 + u32::from(energy_technology)) / 100.0;
    (ENERGY_PER_FUSION_REACTOR * f64::from(level) * maths::powi(base, i32::from(level))).floor() as u32
}

/// Deuterium the Fusion Reactor burns an hour at pace 1.
pub fn fusion_burn_per_hour(level: u8) -> u32 {
    tenth_curve(FUSION_BURN_PER_HOUR, level, true)
}

/// Energy one Solar Satellite makes: `floor(Tmax / 4 + 20)`, at least 1.
pub fn satellite_energy() -> u32 {
    (HOME_TEMPERATURE_MAX.div_euclid(4) + 20).max(1) as u32
}

/// A homeworld's energy: what its mines need, what is made for them, and
/// the deuterium the Fusion Reactor burns each tick to make its share.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Power {
    pub made: u32,
    pub needed: u32,
    pub fusion_burn: f64,
}

impl Power {
    /// The fraction of their output the mines run at.
    pub fn fraction(&self) -> f64 {
        if self.needed == 0 { 1.0 } else { (f64::from(self.made) / f64::from(self.needed)).min(1.0) }
    }
}

/// Energy made and needed. The Fusion Reactor runs only while `fusion_fed`:
/// the stockpile has deuterium for it to burn.
pub fn home_power(
    buildings: &BuildingLevels,
    research: &ResearchLevels,
    satellites: u32,
    fusion_fed: bool,
    pace: &Pace,
) -> Power {
    let needed = [BuildingType::MetalMine, BuildingType::CrystalMine, BuildingType::DeuteriumSynthesizer]
        .into_iter()
        .map(|mine| energy_needed_by(mine, buildings.get(mine)))
        .sum();
    let fusion = buildings.fusion_reactor;
    let running = fusion_fed && fusion > 0;
    let made = solar_plant_energy(buildings.solar_plant)
        .saturating_add(satellites.saturating_mul(satellite_energy()))
        .saturating_add(if running { fusion_energy(fusion, research.energy_technology) } else { 0 });
    let fusion_burn = if running { pace.rate(f64::from(fusion_burn_per_hour(fusion)) / 3600.0) } else { 0.0 };
    Power { made, needed, fusion_burn }
}

/// What the mines add to the stockpile each tick at `fraction` of their
/// power, natural production included. The Fusion Reactor's burn is not
/// taken off here.
pub fn production_per_tick(buildings: &BuildingLevels, fraction: f64, pace: &Pace) -> Resources {
    let mine = |building: BuildingType, natural: f64| {
        pace.rate((mine_output_per_hour(building, buildings.get(building)) * fraction + natural) / 3600.0)
    };
    Resources {
        metal: mine(BuildingType::MetalMine, NATURAL_PER_HOUR_METAL),
        crystal: mine(BuildingType::CrystalMine, NATURAL_PER_HOUR_CRYSTAL),
        deuterium: mine(BuildingType::DeuteriumSynthesizer, 0.0),
    }
}

// ── Times and queues ─────────────────────────────────────────────

/// (metal + crystal) of cost that one game hour of build, research or ship
/// time pays for, before the building that divides it. OGame's.
pub const BUILD_RATE: f64 = 2500.0;
pub const RESEARCH_RATE: f64 = 1000.0;
pub const SHIP_RATE: f64 = 2500.0;
/// Orders that may wait behind the running ones in each queue (building,
/// research, shipyard). The running items come on top, so the building queue
/// holds `slots + 3`. Three because a waiting order is a claim on future
/// income: enough to queue a whole session ahead (a mine, a lab level, a
/// ship batch) but too few to bank a long list of unaffordable upgrades that
/// holds the line, since the queue is first in, first out.
pub const QUEUE_WAITING: usize = 3;

/// Building time is divided by `1 + Robotics Factory` and halved by each
/// level of the Nanite Factory.
pub fn building_speed(buildings: &BuildingLevels) -> f64 {
    f64::from(1 + u32::from(buildings.robotics_factory)) * nanite_speed(buildings)
}

/// Ship and defence time is divided by `1 + Shipyard` and halved by each
/// level of the Nanite Factory.
pub fn shipyard_speed(buildings: &BuildingLevels) -> f64 {
    f64::from(1 + u32::from(buildings.shipyard)) * nanite_speed(buildings)
}

/// Research time is divided by `1 + Research Lab`.
pub fn research_speed(buildings: &BuildingLevels) -> f64 {
    f64::from(1 + u32::from(buildings.research_lab))
}

fn nanite_speed(buildings: &BuildingLevels) -> f64 {
    maths::powi(2.0, i32::from(buildings.nanite_factory))
}

// ── Storage ──────────────────────────────────────────────────────

/// OGame's storage: `STORAGE_BASE + STORAGE_STEP * ceil(STORAGE_GROWTH^level - 1)`
/// of each resource, by the level of its own building, before the pace
/// multiplier (which is this game's). 100,000 with no building, so a new
/// empire is not squeezed by its stores; `STORAGE_BASE` is the knob if it
/// should be.
pub const STORAGE_BASE: f64 = 100_000.0;
pub const STORAGE_STEP: f64 = 50_000.0;
pub const STORAGE_GROWTH: f64 = 1.6;
/// Share of the cap per level of its storage building that a raid cannot
/// skim, up to the max.
pub const STORAGE_PROTECT_PER_LEVEL: f64 = 0.08;
pub const STORAGE_PROTECT_MAX: f64 = 0.60;
/// `StorageNearCap` fires at this fill ratio.
pub const STORAGE_NEAR_CAP_RATIO: f64 = 0.85;

/// The building that holds `kind`.
pub fn storage_building(kind: ResourceKind) -> BuildingType {
    match kind {
        ResourceKind::Metal => BuildingType::MetalStorage,
        ResourceKind::Crystal => BuildingType::CrystalStorage,
        ResourceKind::Deuterium => BuildingType::DeuteriumTank,
    }
}

/// The cap one storage building gives its resource at `level`.
pub fn storage_cap_at(level: u8, pace: &Pace) -> f64 {
    let steps = (maths::powi(STORAGE_GROWTH, i32::from(level)) - 1.0).ceil();
    (STORAGE_BASE + STORAGE_STEP * steps) * pace.cap_mult()
}

fn per_storage(buildings: &BuildingLevels, amount: impl Fn(u8) -> f64) -> Resources {
    let of = |kind: ResourceKind| amount(buildings.get(storage_building(kind)));
    Resources {
        metal: of(ResourceKind::Metal),
        crystal: of(ResourceKind::Crystal),
        deuterium: of(ResourceKind::Deuterium),
    }
}

/// Stockpile cap per resource.
pub fn storage_cap(buildings: &BuildingLevels, pace: &Pace) -> Resources {
    per_storage(buildings, |level| storage_cap_at(level, pace))
}

/// The stock of one resource a raid cannot take, by its building's level.
pub fn storage_protected_at(level: u8, pace: &Pace) -> f64 {
    storage_cap_at(level, pace) * (STORAGE_PROTECT_PER_LEVEL * f64::from(level)).min(STORAGE_PROTECT_MAX)
}

/// Per resource, the stock a raid cannot take.
pub fn storage_protected(buildings: &BuildingLevels, pace: &Pace) -> Resources {
    per_storage(buildings, |level| storage_protected_at(level, pace))
}

/// Smallest level of a storage building whose cap holds `amount`; None
/// when even the highest cannot.
pub fn storage_level_for(amount: f64, pace: &Pace) -> Option<u8> {
    (0..=MAX_BUILDING_LEVEL).find(|&level| storage_cap_at(level, pace) >= amount)
}

// ── Costs ────────────────────────────────────────────────────────

/// Level-1 cost and the per-level growth factor of a building.
pub fn building_cost_curve(building: BuildingType) -> ([f64; 3], f64) {
    match building {
        BuildingType::MetalMine => ([60.0, 15.0, 0.0], 1.5),
        BuildingType::CrystalMine => ([48.0, 24.0, 0.0], 1.6),
        BuildingType::DeuteriumSynthesizer => ([225.0, 75.0, 0.0], 1.5),
        BuildingType::SolarPlant => ([75.0, 30.0, 0.0], 1.5),
        BuildingType::FusionReactor => ([900.0, 360.0, 180.0], 1.8),
        BuildingType::RoboticsFactory => ([400.0, 120.0, 200.0], 2.0),
        BuildingType::NaniteFactory => ([1_000_000.0, 500_000.0, 100_000.0], 2.0),
        BuildingType::Shipyard => ([400.0, 200.0, 100.0], 2.0),
        BuildingType::MetalStorage => ([2_000.0, 0.0, 0.0], 2.0),
        BuildingType::CrystalStorage => ([2_000.0, 1_000.0, 0.0], 2.0),
        BuildingType::DeuteriumTank => ([2_000.0, 2_000.0, 0.0], 2.0),
        BuildingType::ResearchLab => ([200.0, 400.0, 200.0], 2.0),
    }
}

/// `base * growth^(level - 1)`, rounded to whole units.
fn grown_cost(base: [f64; 3], growth: f64, level: u8) -> Resources {
    let factor = maths::powi(growth, i32::from(level.max(1)) - 1);
    let unit = |x: f64| (x * factor).round_ties_even();
    Resources { metal: unit(base[0]), crystal: unit(base[1]), deuterium: unit(base[2]) }
}

/// Cost to build level N.
pub fn building_cost(building: BuildingType, level: u8) -> Resources {
    let (base, growth) = building_cost_curve(building);
    grown_cost(base, growth, level)
}

/// Seconds (= ticks at pace 1) that `cost` takes at `rate` cost-units per
/// hour. Deuterium in a cost adds no time.
fn cost_seconds(cost: Resources, rate: f64, speed_divisor: f64) -> f64 {
    (cost.metal + cost.crystal) / rate * 3600.0 / speed_divisor
}

/// Ticks to build level N: derived from the cost, so time and cost share an
/// exponent.
pub fn building_time(building: BuildingType, level: u8, buildings: &BuildingLevels, pace: &Pace) -> u64 {
    if level == 0 {
        return 0;
    }
    pace.econ_ticks(cost_seconds(building_cost(building, level), BUILD_RATE, building_speed(buildings)))
}

// ── Requirements ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrereqRef {
    Building(BuildingType),
    Research(ResearchType),
}

/// What must stand at what level before something can be ordered.
pub type Prereqs = &'static [(PrereqRef, u8)];

pub fn prerequisites_met(prereqs: Prereqs, buildings: &BuildingLevels, research: &ResearchLevels) -> bool {
    prereqs.iter().all(|(p, need)| match p {
        PrereqRef::Building(b) => buildings.get(*b) >= *need,
        PrereqRef::Research(t) => research.get(*t) >= *need,
    })
}

pub fn building_prerequisites(building: BuildingType) -> Prereqs {
    use BuildingType as B;
    use PrereqRef::{Building, Research};
    use ResearchType as R;
    match building {
        B::FusionReactor => &[(Building(B::DeuteriumSynthesizer), 5), (Research(R::EnergyTechnology), 3)],
        B::NaniteFactory => &[(Building(B::RoboticsFactory), 10), (Research(R::ComputerTechnology), 10)],
        B::Shipyard => &[(Building(B::RoboticsFactory), 2)],
        B::MetalMine
        | B::CrystalMine
        | B::DeuteriumSynthesizer
        | B::SolarPlant
        | B::RoboticsFactory
        | B::MetalStorage
        | B::CrystalStorage
        | B::DeuteriumTank
        | B::ResearchLab => &[],
    }
}

pub fn building_prerequisites_met(
    building: BuildingType,
    buildings: &BuildingLevels,
    research: &ResearchLevels,
) -> bool {
    prerequisites_met(building_prerequisites(building), buildings, research)
}

// ── Research System ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ResearchType {
    EspionageTechnology = 0,
    ComputerTechnology,
    WeaponsTechnology,
    ShieldingTechnology,
    ArmourTechnology,
    EnergyTechnology,
    HyperspaceTechnology,
    CombustionDrive,
    ImpulseDrive,
    HyperspaceDrive,
    LaserTechnology,
    IonTechnology,
    PlasmaTechnology,
    GravitonTechnology,
}

impl ResearchType {
    /// Number of research types.
    pub const COUNT: usize = 14;
    pub const ALL: [ResearchType; ResearchType::COUNT] = [
        ResearchType::EspionageTechnology,
        ResearchType::ComputerTechnology,
        ResearchType::WeaponsTechnology,
        ResearchType::ShieldingTechnology,
        ResearchType::ArmourTechnology,
        ResearchType::EnergyTechnology,
        ResearchType::HyperspaceTechnology,
        ResearchType::CombustionDrive,
        ResearchType::ImpulseDrive,
        ResearchType::HyperspaceDrive,
        ResearchType::LaserTechnology,
        ResearchType::IonTechnology,
        ResearchType::PlasmaTechnology,
        ResearchType::GravitonTechnology,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ResearchType::EspionageTechnology => "Espionage Technology",
            ResearchType::ComputerTechnology => "Computer Technology",
            ResearchType::WeaponsTechnology => "Weapons Technology",
            ResearchType::ShieldingTechnology => "Shielding Technology",
            ResearchType::ArmourTechnology => "Armour Technology",
            ResearchType::EnergyTechnology => "Energy Technology",
            ResearchType::HyperspaceTechnology => "Hyperspace Technology",
            ResearchType::CombustionDrive => "Combustion Drive",
            ResearchType::ImpulseDrive => "Impulse Drive",
            ResearchType::HyperspaceDrive => "Hyperspace Drive",
            ResearchType::LaserTechnology => "Laser Technology",
            ResearchType::IonTechnology => "Ion Technology",
            ResearchType::PlasmaTechnology => "Plasma Technology",
            ResearchType::GravitonTechnology => "Graviton Technology",
        }
    }

    pub fn from_usize(idx: usize) -> Option<Self> {
        Self::ALL.get(idx).copied()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResearchLevels {
    pub espionage_technology: u8,
    pub computer_technology: u8,
    pub weapons_technology: u8,
    pub shielding_technology: u8,
    pub armour_technology: u8,
    pub energy_technology: u8,
    pub hyperspace_technology: u8,
    pub combustion_drive: u8,
    pub impulse_drive: u8,
    pub hyperspace_drive: u8,
    pub laser_technology: u8,
    pub ion_technology: u8,
    pub plasma_technology: u8,
    pub graviton_technology: u8,
}

impl ResearchLevels {
    pub fn get(&self, tech: ResearchType) -> u8 {
        match tech {
            ResearchType::EspionageTechnology => self.espionage_technology,
            ResearchType::ComputerTechnology => self.computer_technology,
            ResearchType::WeaponsTechnology => self.weapons_technology,
            ResearchType::ShieldingTechnology => self.shielding_technology,
            ResearchType::ArmourTechnology => self.armour_technology,
            ResearchType::EnergyTechnology => self.energy_technology,
            ResearchType::HyperspaceTechnology => self.hyperspace_technology,
            ResearchType::CombustionDrive => self.combustion_drive,
            ResearchType::ImpulseDrive => self.impulse_drive,
            ResearchType::HyperspaceDrive => self.hyperspace_drive,
            ResearchType::LaserTechnology => self.laser_technology,
            ResearchType::IonTechnology => self.ion_technology,
            ResearchType::PlasmaTechnology => self.plasma_technology,
            ResearchType::GravitonTechnology => self.graviton_technology,
        }
    }

    pub fn set(&mut self, tech: ResearchType, level: u8) {
        match tech {
            ResearchType::EspionageTechnology => self.espionage_technology = level,
            ResearchType::ComputerTechnology => self.computer_technology = level,
            ResearchType::WeaponsTechnology => self.weapons_technology = level,
            ResearchType::ShieldingTechnology => self.shielding_technology = level,
            ResearchType::ArmourTechnology => self.armour_technology = level,
            ResearchType::EnergyTechnology => self.energy_technology = level,
            ResearchType::HyperspaceTechnology => self.hyperspace_technology = level,
            ResearchType::CombustionDrive => self.combustion_drive = level,
            ResearchType::ImpulseDrive => self.impulse_drive = level,
            ResearchType::HyperspaceDrive => self.hyperspace_drive = level,
            ResearchType::LaserTechnology => self.laser_technology = level,
            ResearchType::IonTechnology => self.ion_technology = level,
            ResearchType::PlasmaTechnology => self.plasma_technology = level,
            ResearchType::GravitonTechnology => self.graviton_technology = level,
        }
    }

    pub fn drive(&self, drive: Drive) -> u8 {
        match drive {
            Drive::Combustion => self.combustion_drive,
            Drive::Impulse => self.impulse_drive,
            Drive::Hyperspace => self.hyperspace_drive,
        }
    }
}

/// Graviton does nothing past its first level; the rest stop at this
/// game's ceiling.
pub fn research_max_level(tech: ResearchType) -> u8 {
    match tech {
        ResearchType::GravitonTechnology => 1,
        _ => MAX_RESEARCH_LEVEL,
    }
}

/// Per-level cost growth of every research but Graviton.
pub const RESEARCH_GROWTH: f64 = 2.0;

/// Level-1 cost and per-level growth.
pub fn research_cost_curve(tech: ResearchType) -> ([f64; 3], f64) {
    let base = match tech {
        ResearchType::EspionageTechnology => [200.0, 1_000.0, 200.0],
        ResearchType::ComputerTechnology => [0.0, 400.0, 600.0],
        ResearchType::WeaponsTechnology => [800.0, 200.0, 0.0],
        ResearchType::ShieldingTechnology => [200.0, 600.0, 0.0],
        ResearchType::ArmourTechnology => [1_000.0, 0.0, 0.0],
        ResearchType::EnergyTechnology => [0.0, 800.0, 400.0],
        ResearchType::HyperspaceTechnology => [0.0, 4_000.0, 2_000.0],
        ResearchType::CombustionDrive => [400.0, 0.0, 600.0],
        ResearchType::ImpulseDrive => [2_000.0, 4_000.0, 600.0],
        ResearchType::HyperspaceDrive => [10_000.0, 20_000.0, 6_000.0],
        ResearchType::LaserTechnology => [200.0, 100.0, 0.0],
        ResearchType::IonTechnology => [1_000.0, 300.0, 100.0],
        ResearchType::PlasmaTechnology => [2_000.0, 4_000.0, 1_000.0],
        ResearchType::GravitonTechnology => return ([0.0, 0.0, 0.0], f64::from(GRAVITON_GROWTH)),
    };
    (base, RESEARCH_GROWTH)
}

pub fn research_cost(tech: ResearchType, level: u8) -> Resources {
    let (base, growth) = research_cost_curve(tech);
    grown_cost(base, growth, level)
}

/// Energy the homeworld must be making for `tech` to be researched to
/// `level`; 0 for every research but Graviton. It is not used up.
pub fn research_energy(tech: ResearchType, level: u8) -> u32 {
    match tech {
        ResearchType::GravitonTechnology => {
            GRAVITON_ENERGY.saturating_mul(GRAVITON_GROWTH.saturating_pow(u32::from(level.max(1)) - 1))
        }
        _ => 0,
    }
}

pub fn research_time(tech: ResearchType, level: u8, buildings: &BuildingLevels, pace: &Pace) -> u64 {
    if level == 0 {
        return 0;
    }
    pace.econ_ticks(cost_seconds(research_cost(tech, level), RESEARCH_RATE, research_speed(buildings)))
}

pub fn research_prerequisites(tech: ResearchType) -> Prereqs {
    use BuildingType::ResearchLab as Lab;
    use PrereqRef::{Building, Research};
    use ResearchType as R;
    match tech {
        R::EspionageTechnology => &[(Building(Lab), 3)],
        R::ComputerTechnology => &[(Building(Lab), 1)],
        R::WeaponsTechnology => &[(Building(Lab), 4)],
        R::ShieldingTechnology => &[(Building(Lab), 6), (Research(R::EnergyTechnology), 3)],
        R::ArmourTechnology => &[(Building(Lab), 2)],
        R::EnergyTechnology => &[(Building(Lab), 1)],
        R::HyperspaceTechnology => {
            &[(Building(Lab), 7), (Research(R::EnergyTechnology), 5), (Research(R::ShieldingTechnology), 5)]
        }
        R::CombustionDrive => &[(Research(R::EnergyTechnology), 1)],
        R::ImpulseDrive => &[(Building(Lab), 2), (Research(R::EnergyTechnology), 1)],
        R::HyperspaceDrive => &[(Research(R::HyperspaceTechnology), 3)],
        R::LaserTechnology => &[(Research(R::EnergyTechnology), 2)],
        R::IonTechnology => {
            &[(Building(Lab), 4), (Research(R::LaserTechnology), 5), (Research(R::EnergyTechnology), 4)]
        }
        R::PlasmaTechnology => &[
            (Research(R::EnergyTechnology), 8),
            (Research(R::LaserTechnology), 10),
            (Research(R::IonTechnology), 5),
        ],
        R::GravitonTechnology => &[(Building(Lab), 12)],
    }
}

pub fn research_prerequisites_met(
    tech: ResearchType,
    buildings: &BuildingLevels,
    research: &ResearchLevels,
) -> bool {
    prerequisites_met(research_prerequisites(tech), buildings, research)
}

// ── Research Modifier Functions ──────────────────────────────────

/// The Weapons, Shielding and Armour Technology a force fights with. A fleet
/// carries the levels it last docked at home with; a defence and a Solar
/// Satellite stand at home and are always at their owner's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Refit {
    pub weapons: u8,
    pub shielding: u8,
    pub armour: u8,
}

impl Refit {
    pub fn of(research: &ResearchLevels) -> Refit {
        Refit {
            weapons: research.weapons_technology,
            shielding: research.shielding_technology,
            armour: research.armour_technology,
        }
    }

    /// The lower of each level: what two fleets merged in the field fight with.
    pub fn lower(self, other: Refit) -> Refit {
        Refit {
            weapons: self.weapons.min(other.weapons),
            shielding: self.shielding.min(other.shielding),
            armour: self.armour.min(other.armour),
        }
    }

    /// Some level of `current` is above this one's: a dock at home would raise it.
    pub fn behind(self, current: Refit) -> bool {
        current.weapons > self.weapons || current.shielding > self.shielding || current.armour > self.armour
    }

    /// A tenth a level each, as OGame has it.
    pub fn arms(self) -> Arms {
        let grown = |level: u8| 1000 + 100 * u16::from(level);
        Arms { attack: grown(self.weapons), shield: grown(self.shielding), hull: grown(self.armour) }
    }
}

/// What a unit's attack, shield and hull are multiplied by in a fight, in
/// thousandths of the class's figures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Arms {
    pub attack: u16,
    pub shield: u16,
    pub hull: u16,
}

impl Arms {
    pub const BASE: Arms = Arms { attack: 1000, shield: 1000, hull: 1000 };

    /// A pirate group's: all three figures at one multiplier.
    pub fn of_grade(multiplier: f32) -> Arms {
        let grade = (multiplier * 1000.0).round().clamp(1.0, f32::from(u16::MAX)) as u16;
        Arms { attack: grade, shield: grade, hull: grade }
    }
}

/// One unit's whole-number figures in a fight. A round is fought on these
/// and on nothing else, so it is the same arithmetic on every target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnitFigures {
    pub hull: u32,
    pub shield: u32,
    pub attack: u32,
}

impl UnitFigures {
    /// `attack + (hull + shield) / 10`, the scale ships, raids and threat
    /// ratings share, for a unit holding `hull` of its hull.
    pub fn power_at(&self, hull: u32) -> f32 {
        self.attack as f32 + (hull as f32 + self.shield as f32) / 10.0
    }

    pub fn power(&self) -> f32 {
        self.power_at(self.hull)
    }
}

/// The figures of one `item` fighting with `arms`, cut off to whole numbers
/// as OGame cuts them.
pub fn unit_figures(item: ShipyardItem, arms: Arms) -> UnitFigures {
    let base = item.base_stats();
    let scaled = |figure: f32, per_mille: u16| (u64::from(figure as u32) * u64::from(per_mille) / 1000) as u32;
    UnitFigures {
        hull: scaled(base.hull, arms.hull),
        shield: scaled(base.shield, arms.shield),
        attack: scaled(base.weapon, arms.attack),
    }
}

pub fn ship_figures(class: ShipClass, arms: Arms) -> UnitFigures {
    unit_figures(ShipyardItem::Ship(class), arms)
}

/// Buildings that can be under construction at once: one, plus one for
/// each of `BUILD_SLOT_LEVELS` that Computer Technology has reached.
pub fn building_slots(research: &ResearchLevels) -> u8 {
    1 + BUILD_SLOT_LEVELS.iter().filter(|&&level| research.computer_technology >= level).count() as u8
}

/// The level of Computer Technology that opens the next building slot.
pub fn next_building_slot_level(research: &ResearchLevels) -> Option<u8> {
    BUILD_SLOT_LEVELS.into_iter().find(|&level| research.computer_technology < level)
}

/// The share of a jump's burn that Energy Technology leaves.
pub fn fuel_rate_modifier(energy_technology: u8) -> f64 {
    1.0 - (FUEL_SAVING_PER_LEVEL * f64::from(energy_technology)).min(FUEL_SAVING_MAX)
}

/// What the Deuterium Tank multiplies every fleet's tank by.
pub fn tank_modifier(deuterium_tank: u8) -> f64 {
    1.0 + TANK_PER_DEUT_TANK_LEVEL * f64::from(deuterium_tank)
}

/// Fuel one unit of deuterium becomes when a fleet refines it from its hold:
/// the home dock's rate less `REFINE_LOSS`.
pub fn refined_fuel_per_deuterium() -> f64 {
    (1.0 - REFINE_LOSS) / FUEL_DEUT_PER_UNIT
}

/// Hops around the homeworld that Espionage Technology shows.
pub fn sensor_range(espionage_technology: u8) -> u8 {
    espionage_technology.saturating_mul(SENSOR_HOPS_PER_LEVEL)
}

/// Cargo gained per raw unit extracted. Laser Technology raises the yield,
/// not the extraction speed, so it does not strip small tiles faster.
pub fn harvest_yield(laser_technology: u8) -> f64 {
    1.0 + HARVEST_YIELD_PER_LEVEL * f64::from(laser_technology.min(HARVEST_YIELD_LEVELS))
}

pub fn recall_damage_reduction(hyperspace_technology: u8) -> f32 {
    RECALL_SAVING_PER_LEVEL * f32::from(hyperspace_technology)
}

// ── Ships in flight ──────────────────────────────────────────────

/// The Small Cargo flies on the Impulse Drive from this level of it, at
/// twice its speed and twice its fuel.
pub const SMALL_CARGO_IMPULSE_LEVEL: u8 = 5;
pub const SMALL_CARGO_IMPULSE_SPEED: u32 = 10_000;
pub const SMALL_CARGO_IMPULSE_FUEL: u32 = 20;
/// The Bomber flies on the Hyperspace Drive from this level of it; its fuel
/// does not change.
pub const BOMBER_HYPERSPACE_LEVEL: u8 = 8;
pub const BOMBER_HYPERSPACE_SPEED: u32 = 5_000;

/// The drive a ship of `class` flies on with `research`, the base speed it
/// has on that drive and the fuel one jump burns. None for a ship that does
/// not fly.
pub fn ship_drive(class: ShipClass, research: &ResearchLevels) -> Option<(Drive, u32, u32)> {
    let stats = class.base_stats();
    let built_with = class.drive()?;
    Some(match class {
        ShipClass::SmallCargo if research.impulse_drive >= SMALL_CARGO_IMPULSE_LEVEL => {
            (Drive::Impulse, SMALL_CARGO_IMPULSE_SPEED, SMALL_CARGO_IMPULSE_FUEL)
        }
        ShipClass::Bomber if research.hyperspace_drive >= BOMBER_HYPERSPACE_LEVEL => {
            (Drive::Hyperspace, BOMBER_HYPERSPACE_SPEED, stats.fuel)
        }
        _ => (built_with, stats.speed, stats.fuel),
    })
}

/// Speed of a ship of `class`: its base speed and the share its drive's
/// research adds. 0 for a ship that does not fly.
pub fn ship_speed(class: ShipClass, research: &ResearchLevels) -> f64 {
    ship_drive(class, research).map_or(0.0, |(drive, speed, _)| {
        f64::from(speed) * (1.0 + drive.speed_per_level() * f64::from(research.drive(drive)))
    })
}

/// Ticks one jump takes at `speed`.
pub fn jump_ticks(speed: f64) -> u16 {
    if speed <= 0.0 {
        return u16::MAX;
    }
    (JUMP_SPEED_REF / speed).ceil().clamp(1.0, f64::from(u16::MAX)) as u16
}

/// Fuel one jump burns for a ship of `class`, before Energy Technology's saving.
pub fn ship_burn(class: ShipClass, research: &ResearchLevels) -> f64 {
    ship_drive(class, research).map_or(0.0, |(_, _, fuel)| f64::from(fuel))
}

/// A ship's tank before the Deuterium Tank: `tank_jumps` of its burn.
pub fn ship_tank(class: ShipClass, research: &ResearchLevels) -> f64 {
    f64::from(class.tank_jumps()) * ship_burn(class, research)
}

pub fn ship_prerequisites(class: ShipClass) -> Prereqs {
    use BuildingType::Shipyard;
    use PrereqRef::{Building, Research};
    use ResearchType as R;
    match class {
        ShipClass::LightFighter => &[(Building(Shipyard), 1), (Research(R::CombustionDrive), 1)],
        ShipClass::SmallCargo => &[(Building(Shipyard), 2), (Research(R::CombustionDrive), 2)],
        ShipClass::HeavyFighter => {
            &[(Building(Shipyard), 3), (Research(R::ArmourTechnology), 2), (Research(R::ImpulseDrive), 2)]
        }
        ShipClass::EspionageProbe => {
            &[(Building(Shipyard), 3), (Research(R::CombustionDrive), 3), (Research(R::EspionageTechnology), 2)]
        }
        ShipClass::LargeCargo => &[(Building(Shipyard), 4), (Research(R::CombustionDrive), 6)],
        ShipClass::Recycler => {
            &[(Building(Shipyard), 4), (Research(R::CombustionDrive), 6), (Research(R::ShieldingTechnology), 2)]
        }
        ShipClass::Cruiser => {
            &[(Building(Shipyard), 5), (Research(R::ImpulseDrive), 4), (Research(R::IonTechnology), 2)]
        }
        ShipClass::Battleship => &[(Building(Shipyard), 7), (Research(R::HyperspaceDrive), 4)],
        ShipClass::Battlecruiser => &[
            (Building(Shipyard), 8),
            (Research(R::HyperspaceTechnology), 5),
            (Research(R::HyperspaceDrive), 5),
            (Research(R::LaserTechnology), 12),
        ],
        ShipClass::Bomber => {
            &[(Building(Shipyard), 8), (Research(R::ImpulseDrive), 6), (Research(R::PlasmaTechnology), 5)]
        }
        ShipClass::Destroyer => &[
            (Building(Shipyard), 9),
            (Research(R::HyperspaceTechnology), 5),
            (Research(R::HyperspaceDrive), 6),
        ],
        ShipClass::Deathstar => &[
            (Building(Shipyard), 12),
            (Research(R::HyperspaceTechnology), 6),
            (Research(R::HyperspaceDrive), 7),
            (Research(R::GravitonTechnology), 1),
        ],
        ShipClass::SolarSatellite => &[(Building(Shipyard), 1)],
    }
}

pub fn ship_class_unlocked(class: ShipClass, buildings: &BuildingLevels, research: &ResearchLevels) -> bool {
    prerequisites_met(ship_prerequisites(class), buildings, research)
}

/// Ticks to build one ship.
pub fn ship_build_time(class: ShipClass, buildings: &BuildingLevels, pace: &Pace) -> u64 {
    pace.econ_ticks(cost_seconds(class.build_cost(), SHIP_RATE, shipyard_speed(buildings)))
}

pub const CANCEL_REFUND_FRACTION: f64 = 0.50;

// ── Defences ─────────────────────────────────────────────────────

/// OGame's defences, without the two missiles, built from the shipyard
/// queue. They never move, burn no fuel, and a raid can wear them down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum DefenceKind {
    RocketLauncher = 0,
    LightLaser,
    HeavyLaser,
    GaussCannon,
    IonCannon,
    PlasmaTurret,
    SmallShieldDome,
    LargeShieldDome,
}

impl DefenceKind {
    pub const COUNT: usize = 8;
    pub const ALL: [DefenceKind; DefenceKind::COUNT] = [
        DefenceKind::RocketLauncher,
        DefenceKind::LightLaser,
        DefenceKind::HeavyLaser,
        DefenceKind::GaussCannon,
        DefenceKind::IonCannon,
        DefenceKind::PlasmaTurret,
        DefenceKind::SmallShieldDome,
        DefenceKind::LargeShieldDome,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DefenceKind::RocketLauncher => "Rocket Launcher",
            DefenceKind::LightLaser => "Light Laser",
            DefenceKind::HeavyLaser => "Heavy Laser",
            DefenceKind::GaussCannon => "Gauss Cannon",
            DefenceKind::IonCannon => "Ion Cannon",
            DefenceKind::PlasmaTurret => "Plasma Turret",
            DefenceKind::SmallShieldDome => "Small Shield Dome",
            DefenceKind::LargeShieldDome => "Large Shield Dome",
        }
    }

    pub fn from_usize(idx: usize) -> Option<Self> {
        Self::ALL.get(idx).copied()
    }

    pub fn build_cost(self) -> Resources {
        let cost = |metal: f64, crystal: f64, deuterium: f64| Resources { metal, crystal, deuterium };
        match self {
            DefenceKind::RocketLauncher => cost(2_000.0, 0.0, 0.0),
            DefenceKind::LightLaser => cost(1_500.0, 500.0, 0.0),
            DefenceKind::HeavyLaser => cost(6_000.0, 2_000.0, 0.0),
            DefenceKind::GaussCannon => cost(20_000.0, 15_000.0, 2_000.0),
            DefenceKind::IonCannon => cost(2_000.0, 6_000.0, 0.0),
            DefenceKind::PlasmaTurret => cost(50_000.0, 50_000.0, 30_000.0),
            DefenceKind::SmallShieldDome => cost(10_000.0, 10_000.0, 0.0),
            DefenceKind::LargeShieldDome => cost(50_000.0, 50_000.0, 0.0),
        }
    }

    /// OGame's figures as a ship carries them; a defence has no speed, hold
    /// or fuel. Its hull is a tenth of its structure.
    pub fn base_stats(self) -> ShipStats {
        let unit = |structure: f32, shield: f32, weapon: f32| ShipStats {
            hull: structure / 10.0,
            shield,
            weapon,
            speed: 0,
            cargo: 0,
            fuel: 0,
        };
        match self {
            DefenceKind::RocketLauncher => unit(2_000.0, 20.0, 80.0),
            DefenceKind::LightLaser => unit(2_000.0, 25.0, 100.0),
            DefenceKind::HeavyLaser => unit(8_000.0, 100.0, 250.0),
            DefenceKind::GaussCannon => unit(35_000.0, 200.0, 1_100.0),
            DefenceKind::IonCannon => unit(8_000.0, 500.0, 150.0),
            DefenceKind::PlasmaTurret => unit(100_000.0, 300.0, 3_000.0),
            DefenceKind::SmallShieldDome => unit(20_000.0, 2_000.0, 1.0),
            DefenceKind::LargeShieldDome => unit(100_000.0, 10_000.0, 1.0),
        }
    }

    /// `weapon + (hull + shield) / 10`, the scale ships, raids and threat
    /// ratings share.
    pub fn power(self) -> f32 {
        stats_power(&self.base_stats())
    }

    /// The most that may stand: one dome of each kind, as in OGame.
    pub fn max_count(self) -> Option<u32> {
        match self {
            DefenceKind::SmallShieldDome | DefenceKind::LargeShieldDome => Some(1),
            _ => None,
        }
    }

    /// Building and research levels needed before this can be ordered.
    pub fn prerequisites(self) -> Prereqs {
        use BuildingType::Shipyard;
        use PrereqRef::{Building, Research};
        use ResearchType as R;
        match self {
            DefenceKind::RocketLauncher => &[(Building(Shipyard), 1)],
            DefenceKind::LightLaser => {
                &[(Building(Shipyard), 2), (Research(R::EnergyTechnology), 1), (Research(R::LaserTechnology), 3)]
            }
            DefenceKind::HeavyLaser => {
                &[(Building(Shipyard), 4), (Research(R::EnergyTechnology), 3), (Research(R::LaserTechnology), 6)]
            }
            DefenceKind::IonCannon => &[(Building(Shipyard), 4), (Research(R::IonTechnology), 4)],
            DefenceKind::GaussCannon => &[
                (Building(Shipyard), 6),
                (Research(R::EnergyTechnology), 6),
                (Research(R::WeaponsTechnology), 3),
                (Research(R::ShieldingTechnology), 1),
            ],
            DefenceKind::PlasmaTurret => &[(Building(Shipyard), 8), (Research(R::PlasmaTechnology), 7)],
            DefenceKind::SmallShieldDome => &[(Building(Shipyard), 1), (Research(R::ShieldingTechnology), 2)],
            DefenceKind::LargeShieldDome => &[(Building(Shipyard), 6), (Research(R::ShieldingTechnology), 6)],
        }
    }
}

pub fn defence_prerequisites_met(kind: DefenceKind, buildings: &BuildingLevels, research: &ResearchLevels) -> bool {
    prerequisites_met(kind.prerequisites(), buildings, research)
}

/// Ticks to build one defence; the Shipyard speeds it like ships.
pub fn defence_build_time(kind: DefenceKind, buildings: &BuildingLevels, pace: &Pace) -> u64 {
    pace.econ_ticks(cost_seconds(kind.build_cost(), SHIP_RATE, shipyard_speed(buildings)))
}

fn stats_power(stats: &ShipStats) -> f32 {
    stats.weapon + (stats.hull + stats.shield) / 10.0
}

/// Power of one defence with the owner's Weapons, Shielding and Armour
/// Technology: a defence stands at home and is always current.
pub fn structure_power(kind: DefenceKind, research: &ResearchLevels) -> f32 {
    unit_figures(ShipyardItem::Defence(kind), Refit::of(research).arms()).power()
}

/// Power of one Solar Satellite at home, with the owner's research.
pub fn satellite_power(research: &ResearchLevels) -> f32 {
    ship_figures(ShipClass::SolarSatellite, Refit::of(research).arms()).power()
}

/// What the shipyard queue builds: a ship class or a defence.
/// On the wire it is just the name ("Cruiser", "RocketLauncher").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ShipyardItem {
    Ship(ShipClass),
    Defence(DefenceKind),
}

impl ShipyardItem {
    /// Every kind of unit a fight can hold.
    pub const COUNT: usize = ShipClass::COUNT + DefenceKind::COUNT;

    /// Its place in the catalogue: the ships, then the defences.
    pub fn index(self) -> usize {
        match self {
            ShipyardItem::Ship(c) => c as usize,
            ShipyardItem::Defence(d) => ShipClass::COUNT + d as usize,
        }
    }

    pub fn from_index(index: usize) -> Option<ShipyardItem> {
        match ShipClass::ALL.get(index) {
            Some(&class) => Some(ShipyardItem::Ship(class)),
            None => DefenceKind::from_usize(index.checked_sub(ShipClass::COUNT)?).map(ShipyardItem::Defence),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ShipyardItem::Ship(c) => c.label(),
            ShipyardItem::Defence(d) => d.label(),
        }
    }

    pub fn unit_cost(self) -> Resources {
        match self {
            ShipyardItem::Ship(c) => c.build_cost(),
            ShipyardItem::Defence(d) => d.build_cost(),
        }
    }

    pub fn base_stats(self) -> ShipStats {
        match self {
            ShipyardItem::Ship(c) => c.base_stats(),
            ShipyardItem::Defence(d) => d.base_stats(),
        }
    }

    pub fn batch_cost(self, count: u16) -> Resources {
        self.unit_cost().scale(f64::from(count))
    }

    pub fn unit_ticks(self, buildings: &BuildingLevels, pace: &Pace) -> u64 {
        match self {
            ShipyardItem::Ship(c) => ship_build_time(c, buildings, pace),
            ShipyardItem::Defence(d) => defence_build_time(d, buildings, pace),
        }
    }

    pub fn prerequisites(self) -> Prereqs {
        match self {
            ShipyardItem::Ship(c) => ship_prerequisites(c),
            ShipyardItem::Defence(d) => d.prerequisites(),
        }
    }
}

// ── Rapid fire ───────────────────────────────────────────────────
//
// A figure r means: after a shot at a unit of that kind, the firing unit
// fires again with probability (r - 1) / r. No defence has rapid fire, and
// nothing has it against the Plasma Turret, the domes or the Deathstar.

/// Every armed ship against an Espionage Probe and a Solar Satellite.
pub const RAPID_FIRE_UNARMED: u16 = 5;
pub const RAPID_FIRE_UNARMED_DEATHSTAR: u16 = 1250;

fn rapid_fire_table(class: ShipClass) -> &'static [(ShipyardItem, u16)] {
    use DefenceKind as D;
    use ShipClass as S;
    use ShipyardItem::{Defence, Ship};
    match class {
        S::HeavyFighter => &[(Ship(S::SmallCargo), 3)],
        S::Cruiser => &[(Ship(S::LightFighter), 6), (Defence(D::RocketLauncher), 10)],
        S::Bomber => &[
            (Defence(D::RocketLauncher), 20),
            (Defence(D::LightLaser), 20),
            (Defence(D::HeavyLaser), 10),
            (Defence(D::IonCannon), 10),
        ],
        S::Destroyer => &[(Ship(S::Battlecruiser), 2), (Defence(D::LightLaser), 10)],
        S::Battlecruiser => &[
            (Ship(S::SmallCargo), 3),
            (Ship(S::LargeCargo), 3),
            (Ship(S::HeavyFighter), 4),
            (Ship(S::Cruiser), 4),
            (Ship(S::Battleship), 7),
        ],
        S::Deathstar => &[
            (Ship(S::SmallCargo), 250),
            (Ship(S::LargeCargo), 250),
            (Ship(S::LightFighter), 200),
            (Ship(S::HeavyFighter), 100),
            (Ship(S::Cruiser), 33),
            (Ship(S::Battleship), 30),
            (Ship(S::Recycler), 250),
            (Ship(S::Bomber), 25),
            (Ship(S::Destroyer), 5),
            (Ship(S::Battlecruiser), 15),
            (Defence(D::RocketLauncher), 200),
            (Defence(D::LightLaser), 200),
            (Defence(D::HeavyLaser), 100),
            (Defence(D::GaussCannon), 50),
            (Defence(D::IonCannon), 100),
        ],
        S::SmallCargo
        | S::LargeCargo
        | S::LightFighter
        | S::Battleship
        | S::Recycler
        | S::EspionageProbe
        | S::SolarSatellite => &[],
    }
}

/// Rapid fire of a ship of `class` against `target`; 0 for none.
pub fn rapid_fire_against(class: ShipClass, target: ShipyardItem) -> u16 {
    use ShipClass::{Deathstar, EspionageProbe, SolarSatellite};
    let unarmed = matches!(target, ShipyardItem::Ship(EspionageProbe | SolarSatellite));
    match class {
        EspionageProbe | SolarSatellite => 0,
        Deathstar if unarmed => RAPID_FIRE_UNARMED_DEATHSTAR,
        _ if unarmed => RAPID_FIRE_UNARMED,
        _ => rapid_fire_table(class).iter().find(|(against, _)| *against == target).map_or(0, |(_, r)| *r),
    }
}

/// Rapid fire of any unit against `target`; 0 for none, and for every defence.
pub fn rapid_fire_of(unit: ShipyardItem, target: ShipyardItem) -> u16 {
    match unit {
        ShipyardItem::Ship(class) => rapid_fire_against(class, target),
        ShipyardItem::Defence(_) => 0,
    }
}

/// Everything a ship of `class` has rapid fire against, ships first, each
/// in catalogue order.
pub fn rapid_fire(class: ShipClass) -> impl Iterator<Item = (ShipyardItem, u16)> {
    let ships = ShipClass::ALL.into_iter().map(ShipyardItem::Ship);
    let defences = DefenceKind::ALL.into_iter().map(ShipyardItem::Defence);
    ships.chain(defences).filter_map(move |target| {
        let figure = rapid_fire_against(class, target);
        (figure > 0).then_some((target, figure))
    })
}

// ── Raids and score points ───────────────────────────────────────

/// How much a bundle of resources counts toward score and raid sizing:
/// crystal and deuterium are scarcer than metal. Points are 32-bit, as
/// power and ratios are: this is where an amount stops being one.
pub fn resource_weight(r: &Resources) -> f32 {
    (par_value(r) / 1000.0) as f32
}

/// A bundle of resources at par, in units of metal: each unit counts its
/// `ResourceKind::weight`.
pub fn par_value(r: &Resources) -> f64 {
    ResourceKind::Metal.weight() * r.metal
        + ResourceKind::Crystal.weight() * r.crystal
        + ResourceKind::Deuterium.weight() * r.deuterium
}

// ── The hub exchange ─────────────────────────────────────────────
//
// The exchange and each player's meter count in weight units: a unit of a
// resource at its `ResourceKind::weight`. Selling raises the seller's meter
// by the weight sold; the fee on each unit of weight rises with the meter,
// from `EXCHANGE_FEE_MIN` when it is empty to `EXCHANGE_FEE_MAX` at one span,
// and a trade pays the fee of every unit it moves the meter through, so an
// order split into pieces returns what the whole order would.

/// One exchange window at this pace, in ticks.
pub fn exchange_window(pace: &Pace) -> u64 {
    pace.attention_ticks(EXCHANGE_WINDOW_TICKS as f64)
}

/// The span of a player's meter, in weight units: `EXCHANGE_METER_SHARE` of
/// what mines making `production` a tick turn out in one window.
pub fn exchange_span(production: &Resources, pace: &Pace) -> f64 {
    let per_tick: f64 = ResourceKind::ALL.into_iter().map(|k| production.get(k) * k.weight()).sum();
    EXCHANGE_METER_SHARE * per_tick * exchange_window(pace) as f64
}

/// The fee on the next unit of weight sold with the meter at `pressure`.
pub fn exchange_fee(pressure: f64, span: f64) -> f64 {
    let rise = if span > 0.0 { (pressure / span).clamp(0.0, 1.0) } else { 1.0 };
    EXCHANGE_FEE_MIN + (EXCHANGE_FEE_MAX - EXCHANGE_FEE_MIN) * rise
}

/// Weight handed back in all for selling from an empty meter up to `pressure`.
fn exchange_kept(pressure: f64, span: f64) -> f64 {
    let ramp = pressure.min(span).max(0.0);
    let on_ramp = if span > 0.0 {
        (1.0 - EXCHANGE_FEE_MIN) * ramp - (EXCHANGE_FEE_MAX - EXCHANGE_FEE_MIN) * ramp * ramp / (2.0 * span)
    } else {
        0.0
    };
    on_ramp + (1.0 - EXCHANGE_FEE_MAX) * (pressure - ramp)
}

/// Weight the exchange hands back for `value` weight sold with the meter at
/// `pressure`: never more than `1 - EXCHANGE_FEE_MIN` of it.
pub fn exchange_return(pressure: f64, value: f64, span: f64) -> f64 {
    exchange_kept(pressure + value, span) - exchange_kept(pressure, span)
}

/// The meter `elapsed` ticks after it read `pressure`: it forgets one span per window.
pub fn exchange_pressure_after(pressure: f64, elapsed: u64, span: f64, window: u64) -> f64 {
    (pressure - span * elapsed as f64 / window as f64).max(0.0)
}

/// Points for every completed building and research level, at the cost paid.
/// Fleets and defences do not count.
pub fn econ_points(buildings: &BuildingLevels, research: &ResearchLevels) -> f32 {
    building_points(buildings) + research_points(research)
}

/// Points for every completed building level, at the cost paid.
pub fn building_points(buildings: &BuildingLevels) -> f32 {
    BuildingType::ALL
        .iter()
        .flat_map(|&b| (1..=buildings.get(b)).map(move |level| resource_weight(&building_cost(b, level))))
        .sum()
}

/// Points for every completed research level, at the cost paid.
pub fn research_points(research: &ResearchLevels) -> f32 {
    ResearchType::ALL
        .iter()
        .flat_map(|&t| (1..=research.get(t)).map(move |level| resource_weight(&research_cost(t, level))))
        .sum()
}

/// A raid's mean power is `RAID_POWER_FLOOR + RAID_POWER_PER_POINT * econ^RAID_POWER_EXPONENT`.
/// The law is this game's; the two figures are provisional. The floor is
/// about half a Light Fighter, and a point spent on Rocket Launchers is 51
/// power: an empire of 1,000 points meets its raid (1,650) with 33 of them,
/// 3 percent of what it is worth, and one of 10,000 (9,050) with 2 percent.
pub const RAID_POWER_FLOOR: f32 = 50.0;
pub const RAID_POWER_PER_POINT: f32 = 9.0;
pub const RAID_POWER_EXPONENT: f32 = 0.75;

/// Mean power of the next raid on a player with `econ` points; the actual
/// raid is rolled between 0.80 and 1.15 times this.
pub fn raid_power_base(econ: f32) -> f32 {
    RAID_POWER_FLOOR + RAID_POWER_PER_POINT * maths::powf(econ, RAID_POWER_EXPONENT)
}

/// A class that flies in a raid on a homeworld once the defences it is the
/// answer to stand there in number: so many of `against` between them, and
/// then `share` of the raid's power goes into it.
pub struct RaidWing {
    pub class: ShipClass,
    pub share: f32,
    pub against: &'static [DefenceKind],
    pub from: u32,
}

/// What raiders fly beside their Light Fighters, so that a wall of one
/// cheap defence has an answer: Cruisers, with rapid fire against Rocket
/// Launchers, and Bombers, with rapid fire against both lasers, the Ion
/// Cannon and the Rocket Launcher. Provisional, every figure of it.
pub const RAID_WINGS: [RaidWing; 2] = [
    RaidWing { class: ShipClass::Cruiser, share: 0.4, against: &[DefenceKind::RocketLauncher], from: 20 },
    RaidWing {
        class: ShipClass::Bomber,
        share: 0.3,
        against: &[DefenceKind::LightLaser, DefenceKind::HeavyLaser, DefenceKind::IonCannon],
        from: 20,
    },
];
/// What the rest of a raid's power is flown as.
pub const RAID_ESCORT: ShipClass = ShipClass::LightFighter;

/// What a raid of `power` flies at a homeworld where `defences` stand, by
/// class in catalogue order, and the multiplier on every figure of its
/// ships that makes them add up to `power`. A wing flies only when its
/// share buys a whole ship of its class; the rest is Light Fighters, one
/// at the least. The power is `raid_power_base`'s: this decides the
/// classes and nothing else.
pub fn raid_composition(power: f32, defences: &[u32; DefenceKind::COUNT]) -> (Vec<(ShipClass, u32)>, f32) {
    let mut counts = [0u32; ShipClass::COUNT];
    let mut left = power;
    for wing in &RAID_WINGS {
        let standing: u32 = wing.against.iter().map(|&kind| defences[kind as usize]).sum();
        if standing < wing.from {
            continue;
        }
        let each = ship_power(wing.class);
        let count = (wing.share * power / each).floor();
        counts[wing.class as usize] = count as u32;
        left -= count * each;
    }
    let escorts = (left / ship_power(RAID_ESCORT)).round().max(0.0) as u32;
    counts[RAID_ESCORT as usize] += escorts;
    if counts.iter().all(|&count| count == 0) {
        counts[RAID_ESCORT as usize] = 1;
    }
    let ships: Vec<(ShipClass, u32)> =
        ShipClass::ALL.into_iter().map(|class| (class, counts[class as usize])).filter(|&(_, count)| count > 0).collect();
    let plain: f32 = ships.iter().map(|&(class, count)| count as f32 * ship_power(class)).sum();
    (ships, (power / plain).min(NPC_STAT_MULT_MAX))
}

// ── Outposts ─────────────────────────────────────────────────────
//
// One level sets everything an outpost is: what it costs, how long it takes,
// what its store holds, what its platform is worth and how many site rates
// its extractor makes. Level 0 is an outpost whose first level is still
// being built.

/// What level `level` costs: `OUTPOST_COST * 2.5^(level - 1)`.
pub fn outpost_cost(level: u8) -> Resources {
    grown_cost([OUTPOST_COST.metal, OUTPOST_COST.crystal, OUTPOST_COST.deuterium], OUTPOST_GROWTH, level)
}

/// Ticks to build level `level`, from its cost like every other build time.
pub fn outpost_build_ticks(level: u8, pace: &Pace) -> u64 {
    pace.econ_ticks(cost_seconds(outpost_cost(level), OUTPOST_BUILD_RATE, 1.0))
}

fn outpost_growth(level: u8) -> f64 {
    maths::powi(OUTPOST_GROWTH, i32::from(level.max(1)) - 1)
}

/// What the store holds per resource. An outpost under construction has the store of level 1.
pub fn outpost_store_cap(level: u8, pace: &Pace) -> Resources {
    OUTPOST_STORE_BASE.scale(outpost_growth(level) * pace.cargo_mult())
}

/// The defences an outpost's platform is at each level. They fight a raid
/// at the catalogue's figures, whatever their owner has researched, and
/// stand whole again after every fight. Provisional: it sets an undefended
/// outpost's odds.
pub fn outpost_platform(level: u8) -> &'static [(DefenceKind, u32)] {
    match level {
        0 => &[],
        1 => &[(DefenceKind::RocketLauncher, 4)],
        2 => &[(DefenceKind::RocketLauncher, 8), (DefenceKind::LightLaser, 4)],
        _ => &[(DefenceKind::RocketLauncher, 16), (DefenceKind::LightLaser, 8), (DefenceKind::HeavyLaser, 4)],
    }
}

/// Defence power of the platform; none before level 1 is finished.
pub fn outpost_platform_power(level: u8) -> f32 {
    outpost_platform(level).iter().map(|&(kind, count)| count as f32 * kind.power()).sum()
}

/// The grade of a site whose three ores are all Pristine: the most a grade can be.
pub const SITE_GRADE_MAX: f32 = 3.0 * Density::Pristine.harvest_multiplier();

/// A site's grade: the harvest multipliers of its metal, crystal and deuterium ore, summed. 0 to 12.
pub fn site_grade(densities: [Density; 3]) -> f32 {
    densities.into_iter().map(Density::harvest_multiplier).sum()
}

/// What the extractor of an outpost at `ring` adds to its store each tick,
/// from the densities the sector was generated with.
pub fn outpost_yield_per_tick(densities: [Density; 3], ring: u16, level: u8, pace: &Pace) -> Resources {
    let per_tick = |density: Density| {
        let per_hour = OUTPOST_YIELD_PER_HOUR * f64::from(density.harvest_multiplier()) * ring_mult(ring);
        pace.rate(per_hour / 3600.0 * f64::from(level))
    };
    let [metal, crystal, deuterium] = densities;
    Resources { metal: per_tick(metal), crystal: per_tick(crystal), deuterium: per_tick(deuterium) }
}

/// Mean power of a raid on an outpost at `ring`; the actual raid is rolled
/// between 0.80 and 1.15 times this, as a homeworld's is.
pub fn outpost_raid_power(ring: u16) -> f32 {
    OUTPOST_RAID_FACTOR * npc_group_power(ring)
}

/// What a raid of `power` on an outpost at `ring` flies: the class of the
/// ring's own groups, so many of it, and the multiplier on its figures, as
/// a group of that power would be made up there.
pub fn outpost_raid_composition(ring: u16, power: f32) -> (ShipClass, u32, f32) {
    let (class, count, multiplier) = npc_group(ring, power);
    (class, u32::from(count), multiplier)
}

/// What repairing a breached outpost of `level` costs, in metal: the par
/// value of `OUTPOST_REPAIR_SHARE` of the level's cost.
pub fn outpost_repair_price(level: u8) -> f64 {
    if level == 0 { 0.0 } else { par_value(&outpost_cost(level).scale(OUTPOST_REPAIR_SHARE)) }
}

/// What a repair of `level` takes from `store`, and how much metal the
/// store is short of paying it. The price is paid in metal; where the metal
/// runs out crystal pays at its par value, and then deuterium, so the store
/// keeps what is scarcest. The payment is also the floor of the store that
/// a raid cannot take.
pub fn outpost_repair_payment(level: u8, store: Resources) -> (Resources, f64) {
    pay_by_weight(outpost_repair_price(level), store)
}

/// What a price counted in metal takes from `stock`, and how much metal the
/// stock is short of paying it: metal first, then crystal and deuterium,
/// each unit at its par value. An outpost's own repair and the mending of
/// ships at a dock are both paid this way.
pub fn pay_by_weight(price: f64, stock: Resources) -> (Resources, f64) {
    let mut owed = price;
    let mut payment = Resources::default();
    for kind in ResourceKind::ALL {
        let (held, covers) = (stock.get(kind).max(0.0), owed / kind.weight());
        if held >= covers {
            *payment.get_mut(kind) = covers;
            owed = 0.0;
        } else {
            *payment.get_mut(kind) = held;
            owed -= held * kind.weight();
        }
    }
    (payment, owed)
}

pub fn outpost_repair_ticks(level: u8, pace: &Pace) -> u64 {
    pace.econ_ticks(cost_seconds(outpost_cost(level), OUTPOST_BUILD_RATE, 1.0) * OUTPOST_REPAIR_SHARE)
}

/// Score points of an outpost standing at `level`: every finished level at the weight of its cost.
pub fn outpost_points(level: u8) -> f32 {
    (1..=level).map(|l| resource_weight(&outpost_cost(l))).sum()
}

// ── Relays ───────────────────────────────────────────────────────

/// What founding a relay at `ring` takes from the hold: the base cost,
/// plus `RELAY_COST_GROWTH` of it for every ring past the first it may stand in.
pub fn relay_cost(ring: u16) -> Resources {
    let steps = f64::from(ring.saturating_sub(RELAY_MIN_RING));
    let unit = |x: f64| (x * (1.0 + RELAY_COST_GROWTH * steps)).round_ties_even();
    Resources {
        metal: unit(RELAY_COST_BASE.metal),
        crystal: unit(RELAY_COST_BASE.crystal),
        deuterium: unit(RELAY_COST_BASE.deuterium),
    }
}

/// How many relays an empire may keep: `RELAY_BASE_MAX`, and one more for
/// every `RELAY_LEVELS_PER_STEP` levels of Hyperspace Technology.
pub fn relay_limit(research: &ResearchLevels) -> u32 {
    RELAY_BASE_MAX + u32::from(research.hyperspace_technology) / RELAY_LEVELS_PER_STEP
}

/// Jumps of its own burn that a relay or a roaming hub fills a visiting
/// fleet's tank to: `DEPOT_FILL_JUMPS_BASE`, and `DEPOT_FILL_JUMPS_PER_LEVEL`
/// for every level of Hyperspace Technology.
pub fn depot_fill_jumps(research: &ResearchLevels) -> u32 {
    DEPOT_FILL_JUMPS_BASE + u32::from(research.hyperspace_technology) * DEPOT_FILL_JUMPS_PER_LEVEL
}

// ── Roaming hubs ─────────────────────────────────────────────────

/// Ticks a roaming hub stays where it is, at this pace.
pub fn hub_period_ticks(pace: &Pace) -> u64 {
    pace.attention_ticks(HUB_PERIOD_TICKS as f64).max(u64::from(HUB_COUNT))
}

/// Ticks of warning before a hub departs, at this pace; always less than a stay.
pub fn hub_warn_ticks(pace: &Pace) -> u64 {
    pace.attention_ticks(HUB_WARN_TICKS as f64).min(hub_period_ticks(pace) - 1)
}

/// The stay of hub `id` that `tick` falls in. Each hub's stays are offset by
/// its id's share of a period, so they do not all end at once.
pub fn hub_epoch(id: u8, tick: u64, pace: &Pace) -> u64 {
    let period = hub_period_ticks(pace);
    (tick + u64::from(id) * period / u64::from(HUB_COUNT)) / period
}

/// The tick hub `id` leaves the place of stay `epoch`: the first tick of the next.
pub fn hub_departs_at(id: u8, epoch: u64, pace: &Pace) -> u64 {
    let period = hub_period_ticks(pace);
    ((epoch + 1) * period).saturating_sub(u64::from(id) * period / u64::from(HUB_COUNT))
}

// ── Ore ──────────────────────────────────────────────────────────

pub const ORE_RING_GROWTH: f64 = 1.10;
pub const ORE_RING_FIRST: u16 = 3;
pub const ORE_RING_LAST: u16 = 20;
/// A stripped tile refills completely in this many game hours plus
/// `ORE_REGEN_HOURS_PER_RING` per ring.
pub const ORE_REGEN_BASE_HOURS: f32 = 120.0;
pub const ORE_REGEN_HOURS_PER_RING: f32 = 24.0;
/// Density rolls drift this much per ring (up to `ORE_DRIFT_MAX`) toward
/// Rich and Pristine.
pub const ORE_DRIFT_PER_RING: f32 = 0.012;
pub const ORE_DRIFT_MAX: f32 = 0.30;

/// Reserve multiplier of a ring: 1.0 inside ring 3, then 1.10 per ring up to
/// ring 20 (5.05 beyond).
pub fn ring_mult(dist: u16) -> f64 {
    if dist < ORE_RING_FIRST {
        1.0
    } else {
        maths::powi(ORE_RING_GROWTH, i32::from(dist.min(ORE_RING_LAST)) - i32::from(ORE_RING_FIRST))
    }
}

/// Raw units to harvest at `density` before it drops a level.
pub fn ore_step_units(density: Density, dist: u16) -> f64 {
    density.step_units() * ring_mult(dist)
}

/// Raw units a full tile of `density` holds.
pub fn ore_reserve_units(density: Density, dist: u16) -> f64 {
    density.reserve_units() * ring_mult(dist)
}

pub fn ore_regen_hours(dist: u16) -> f32 {
    ORE_REGEN_BASE_HOURS + ORE_REGEN_HOURS_PER_RING * dist as f32
}

/// Raw units a stripped tile with template `density` regains per tick: its
/// whole reserve over `ore_regen_hours`, faster at higher pace.
pub fn ore_regen_per_tick(density: Density, dist: u16, pace: &Pace) -> f64 {
    pace.rate(ore_reserve_units(density, dist) / (f64::from(ore_regen_hours(dist)) * 3600.0))
}

/// Raw units already taken from a tile generated at `template` that now
/// stands at `current` with `harvested` units gone from the current level.
pub fn ore_extracted(template: Density, current: Density, harvested: f64, dist: u16) -> f64 {
    let mut level = template;
    let mut gone = harvested;
    while level != current && level != Density::None {
        gone += ore_step_units(level, dist);
        level = level.downgrade();
    }
    gone
}

/// A tile stands at `current` with `harvested` units gone from that level;
/// give back `amount` units. Refilling a level upgrades the tile and carries
/// the rest into the next one; a tile never rises above `template`.
pub fn ore_regen_step(template: Density, current: Density, harvested: f64, amount: f64, dist: u16) -> (Density, f64) {
    let mut level = current;
    let mut gone = harvested - amount;
    while gone < 0.0 && level != template {
        level = level.upgrade();
        gone += ore_step_units(level, dist);
    }
    (level, gone.max(0.0))
}

/// Slack for float drift when harvesting empties a level exactly.
pub const ORE_EPSILON: f64 = 1e-3;

/// Chances of (Sparse, Moderate, Rich, Pristine) for an ore tile at `dist`.
pub fn ore_density_odds(dist: u16) -> [f32; 4] {
    let drift = (ORE_DRIFT_PER_RING * dist as f32).min(ORE_DRIFT_MAX);
    [0.40 - drift, 0.30 - 0.2 * drift, 0.20 + 0.6 * drift, 0.10 + 0.6 * drift]
}

// ── Loot ─────────────────────────────────────────────────────────

/// Loot paid P^0.5 times its constants before it followed production; the
/// constants carry that factor at the season preset, `sqrt(FIND_ANCHOR)`, so
/// a season find pays what it always did and `Pace::find_mult` is 1 there.
pub const SEASON_LOOT_LIFT: f64 = crate::maths::sqrt(crate::pace::FIND_ANCHOR);

/// A destroyed group leaves `WRECK_VALUE_K * power^WRECK_VALUE_EXPONENT`
/// units: three times 1.2 * `MAP_PAYS` / 10^0.8, the 10 being what a
/// group's power grew by with `NPC_POWER_BASE`. A kill scores nothing and
/// pays in its wreckage, so the wreckage is three times what it was when a
/// kill also scored. Provisional.
pub const WRECK_VALUE_K: f64 = 1.71 * SEASON_LOOT_LIFT;
pub const WRECK_VALUE_EXPONENT: f64 = 0.8;
/// Metal, crystal and deuterium shares of any loot value.
pub const LOOT_SPLIT: [f64; 3] = [0.55, 0.30, 0.15];
/// A derelict in ring `DERELICT_VALUE_RING` is worth `DERELICT_VALUE_BASE`;
/// each ring further out multiplies that by `DERELICT_VALUE_GROWTH`.
pub const DERELICT_VALUE_BASE: f64 = 240.0 * MAP_PAYS * SEASON_LOOT_LIFT;
pub const DERELICT_VALUE_GROWTH: f64 = 1.14;
pub const DERELICT_VALUE_RING: u16 = 4;
/// A site pays between these shares of its value, uniformly.
pub const DERELICT_ROLL_MIN: f32 = 0.6;
pub const DERELICT_ROLL_MAX: f32 = 1.4;
/// A site with an ancient relic aboard pays this many times its loot.
/// Provisional.
pub const RELIC_LOOT_MULT: f64 = 6.0;
/// A sector's chart, delivered to a dock, pays
/// `CHART_VALUE_BASE + CHART_VALUE_THREAT * T^CHART_VALUE_EXPONENT` units
/// of loot into the stockpile at pace 1, T being the threat rating its ring
/// gives it; the pace's find factor scales it like every find. Provisional.
pub const CHART_VALUE_BASE: f64 = 50.0;
pub const CHART_VALUE_THREAT: f64 = 50.0;
pub const CHART_VALUE_EXPONENT: f64 = 1.5;

/// Units of loot the chart of a sector at threat `rating` pays.
pub fn chart_value(rating: u8, pace: &Pace) -> f64 {
    (CHART_VALUE_BASE + CHART_VALUE_THREAT * maths::powf(f64::from(rating), CHART_VALUE_EXPONENT)) * pace.find_mult()
}

/// Anti-farming: every kill in a sector adds one point of heat, and the next
/// kill's wreckage is multiplied by `FARM_DECAY^heat`. Heat halves every
/// `FARM_HALF_LIFE_RESPAWNS` respawn delays, so a group left to respawn a
/// few times pays in full again but one sector is not an endless tap: at
/// the respawn rate its steady yield is about a fifth of a fresh kill.
pub const FARM_DECAY: f32 = 0.75;
pub const FARM_HALF_LIFE_RESPAWNS: f32 = 4.0;

/// Share of a fresh kill's wreckage a sector with `heat` pays.
pub fn farm_factor(heat: f32) -> f32 {
    maths::powf(FARM_DECAY, heat.max(0.0))
}

/// `heat` after `elapsed` ticks with the given half-life.
pub fn farm_heat_after(heat: f32, elapsed: u64, half_life_ticks: u64) -> f32 {
    if heat <= 0.0 || half_life_ticks == 0 {
        return 0.0;
    }
    heat * maths::exp2(-(elapsed as f32 / half_life_ticks as f32))
}

/// Units of loot (before the pace's find factor) a destroyed group leaves:
/// it grows slower than the danger, so far hunting stays a gamble.
pub fn wreck_value(group_power: f32) -> f64 {
    WRECK_VALUE_K * maths::powf(f64::from(group_power.max(0.0)), WRECK_VALUE_EXPONENT)
}

/// `value` units as metal, crystal and deuterium.
pub fn split_loot(value: f64) -> Resources {
    Resources { metal: value * LOOT_SPLIT[0], crystal: value * LOOT_SPLIT[1], deuterium: value * LOOT_SPLIT[2] }
}

/// Mean loot value (units, before the find factor) of a derelict `dist` rings out.
pub fn derelict_value(dist: u16) -> f64 {
    DERELICT_VALUE_BASE * maths::powi(DERELICT_VALUE_GROWTH, i32::from(dist) - i32::from(DERELICT_VALUE_RING))
}

/// Tier follows the threat band of the ring: 1 for T1 to T3, 2 for T4 to T6, 3 above.
pub fn derelict_tier(dist: u16) -> u8 {
    threat_band(threat_rating(npc_power(dist)))
}

/// Chance a boarding wakes the guard, before a probe's read and earlier failures.
pub fn derelict_ambush(rating: u8) -> f32 {
    (0.10 + 0.03 * (f32::from(rating) - 1.0)).min(0.35)
}

/// Share of the ring's group power a derelict's guard fields.
pub const AMBUSH_GUARD_SHARE: f32 = 0.6;

/// (class, count, stat multiplier) of the guard waiting in a derelict `dist` rings out.
pub fn ambush_guard(dist: u16) -> (ShipClass, u8, f32) {
    npc_group(dist, AMBUSH_GUARD_SHARE * npc_power(dist))
}

/// Chance a clean boarding also finds a ship that can be recovered.
pub fn recovery_chance(rating: u8) -> f32 {
    (0.05 * (f32::from(rating) - 2.0)).clamp(0.0, 0.30)
}

/// Chance of a data core: a free research level.
pub fn data_core_chance(rating: u8) -> f32 {
    (0.03 * (f32::from(rating) - 5.0)).clamp(0.0, 0.15)
}

/// Chance of an ancient relic, from T6: 2 percent per rating point above 5.
pub fn relic_chance(rating: u8) -> f32 {
    (0.02 * (f32::from(rating) - 5.0)).clamp(0.0, 0.08)
}

// ── Threat and the NPC gradient ──────────────────────────────────

/// The power of a group at ring 1, on the scale a Light Fighter is 91 on.
/// Provisional: it sets every ring's strength against a starting fleet.
pub const NPC_POWER_BASE: f32 = 40.0;
/// NPC group power grows by this factor per ring. With `NPC_MAX_SHIPS` it
/// decides whether pirates come in hundreds. Provisional.
pub const NPC_POWER_GROWTH: f32 = 1.22;
pub const NPC_MAX_SHIPS: usize = 32;
/// The classes pirates fly, lightest first.
pub const NPC_LADDER: [ShipClass; 4] =
    [ShipClass::LightFighter, ShipClass::HeavyFighter, ShipClass::Cruiser, ShipClass::Battleship];
/// A group flies the heaviest class of `NPC_LADDER` of which its power
/// makes at least this many ships, so a group that loses one keeps the
/// rest. Provisional: with it Light Fighters reach ring 15, Heavy Fighters
/// ring 20, Cruisers ring 25.
pub const NPC_MIN_PACK: f32 = 3.0;
/// The most an NPC ship's hull, shield and weapon are scaled by.
pub const NPC_STAT_MULT_MAX: f32 = 1.3;
/// The highest threat rating.
pub const MAX_THREAT: u8 = 9;
/// Observed Aggressive and Swarm groups are rated this much stronger than
/// their ships add up to: they come to you.
pub const THREAT_AGGRESSIVE_BONUS: f32 = 1.25;

/// How much one ship counts on the scale raids, threat and ratios share.
pub fn ship_power(class: ShipClass) -> f32 {
    stats_power(&class.base_stats())
}

/// Target combat power of the NPC group guarding a sector `dist` rings out.
pub fn npc_power(dist: u16) -> f32 {
    NPC_POWER_BASE * maths::powi(NPC_POWER_GROWTH, i32::from(dist.max(1)) - 1)
}

/// The class of the group at `dist`: the heaviest of `NPC_LADDER` that its
/// power makes `NPC_MIN_PACK` ships of at the ring's base multiplier, and
/// Light Fighters where none does.
pub fn npc_class(dist: u16) -> ShipClass {
    let (power, multiplier) = (npc_power(dist), npc_base_multiplier(dist));
    NPC_LADDER
        .into_iter()
        .rev()
        .find(|&class| power / (ship_power(class) * multiplier) >= NPC_MIN_PACK)
        .unwrap_or(NPC_LADDER[0])
}

/// The stat multiplier a group at `dist` is sized around.
pub fn npc_base_multiplier(dist: u16) -> f32 {
    (0.6 + 0.02 * dist as f32).min(NPC_STAT_MULT_MAX)
}

/// (class, ship count, stat multiplier) of a group worth `power` at `dist`.
/// The count comes from the ring's base multiplier, raised if the ships
/// would otherwise need more than `NPC_STAT_MULT_MAX`; the multiplier is then
/// fitted so the ships add up to `power`. Only the ship limit leaves a group
/// short of it.
fn npc_group(dist: u16, power: f32) -> (ShipClass, u8, f32) {
    let class = npc_class(dist);
    let per_ship = ship_power(class);
    let sized = (power / (per_ship * npc_base_multiplier(dist))).round();
    let needed = (power / (per_ship * NPC_STAT_MULT_MAX)).ceil();
    let count = sized.max(needed).clamp(1.0, NPC_MAX_SHIPS as f32);
    (class, count as u8, (power / (per_ship * count)).min(NPC_STAT_MULT_MAX))
}

/// (class, ship count, stat multiplier) of the group at `dist`: its real
/// power is `npc_power(dist)`.
pub fn npc_composition(dist: u16) -> (ShipClass, u8, f32) {
    npc_group(dist, npc_power(dist))
}

/// Power of the group that spawns at `dist`: `npc_power` until the ship
/// limit holds it down.
pub fn npc_group_power(dist: u16) -> f32 {
    npc_power(dist).min(ship_power(npc_class(dist)) * NPC_STAT_MULT_MAX * NPC_MAX_SHIPS as f32)
}

/// Chance in percent that a sector at `dist` holds a group.
pub fn npc_presence_pct(dist: u16) -> f32 {
    (25.0 + 2.5 * dist as f32).min(85.0)
}

/// Share (0 to 1) of groups that stay passive.
pub fn npc_passive_share(dist: u16) -> f32 {
    (0.5 - 0.08 * (dist as f32 - 8.0)).clamp(0.0, 0.5)
}

/// Game hours before a cleared group returns, at pace 1.
pub fn npc_respawn_hours(dist: u16) -> f32 {
    2.0 + 0.25 * dist as f32
}

/// Ticks before a cleared group at ring `dist` returns: a finds timer, as a
/// derelict's is, so a group worn down at a fast pace stays worn for a
/// player's session (ring 3 to 20: 26 to 66 minutes to mend at season, 4
/// to 9 at blitz).
pub fn npc_respawn_ticks(dist: u16, pace: &Pace) -> u64 {
    pace.finds_ticks(f64::from(npc_respawn_hours(dist)) * 3600.0)
}

/// Ticks between two steps of a wounded group's healing at ring `dist`:
/// `NPC_HEAL_SHARE` of the respawn delay, cut into `NPC_HEAL_STEPS`.
pub fn npc_heal_step_ticks(dist: u16, pace: &Pace) -> u64 {
    let whole = pace.finds_ticks(f64::from(npc_respawn_hours(dist)) * 3600.0 * NPC_HEAL_SHARE);
    whole.div_ceil(u64::from(NPC_HEAL_STEPS)).max(1)
}

/// Ships a group generated with `muster` gets back at one step of its healing.
pub fn npc_heal_ships(muster: u32) -> u32 {
    muster.div_ceil(NPC_HEAL_STEPS).max(1)
}

/// Hull a damaged ship whose full hull is `full` regains at one step.
pub fn npc_heal_hull(full: u32) -> u32 {
    full.div_ceil(NPC_HEAL_STEPS).max(1)
}

/// The level of its drive a group at ring `dist` flies with.
pub fn npc_drive_level(dist: u16) -> u8 {
    (dist.saturating_sub(NPC_DRIVE_FREE_RINGS).div_ceil(NPC_DRIVE_RINGS_PER_LEVEL)).min(u16::from(MAX_RESEARCH_LEVEL)) as u8
}

/// Ticks one jump takes a group of `class` whose lair is at ring `dist`.
pub fn npc_jump_ticks(class: ShipClass, dist: u16) -> u16 {
    let level = npc_drive_level(dist);
    let drives = ResearchLevels { combustion_drive: level, impulse_drive: level, hyperspace_drive: level, ..ResearchLevels::default() };
    jump_ticks(ship_speed(class, &drives))
}

/// T1 to T9: one step per doubling of `power` over `NPC_POWER_BASE`.
pub fn threat_rating(power: f32) -> u8 {
    if power.is_nan() || power <= NPC_POWER_BASE {
        return 1;
    }
    (1.0 + maths::log2(power / NPC_POWER_BASE).floor()).clamp(1.0, MAX_THREAT as f32) as u8
}

/// Coarse band of a rating: 1 for T1 to T3, 2 for T4 to T6, 3 for T7 to T9.
/// Derelict tiers follow it.
pub fn threat_band(rating: u8) -> u8 {
    rating.clamp(1, MAX_THREAT).div_ceil(3)
}

/// Ratio at and above which a fight is SAFE: a win that keeps nine tenths
/// of the fleet. Both sides fire at once, so no fight against armed ships
/// is free. What each band means was measured on fleets of one class
/// against the lairs of thirteen rings, sixty fights a pairing: SAFE won
/// 58 to 60 of them and kept 0.85 or more of its hull.
pub const RATIO_SAFE: f32 = 2.5;
/// FAVOURABLE from here: won 59 or 60 of 60 and kept 0.63 to 0.91.
pub const RATIO_FAVOURABLE: f32 = 1.5;
/// EVEN from here: won 45 to 60 of 60 and kept 0.36 to 0.90 on average,
/// the fights it lost counted as nothing kept.
pub const RATIO_EVEN: f32 = 1.1;
/// RISKY from here: won 10 to 51 of 60. Below it (DEADLY) the fleet won 25
/// of 60 at best, most pairings none, and kept under a fifth.
pub const RATIO_RISKY: f32 = 0.9;

/// What each ratio means, as (ratio, margin): the margin is the share of its
/// worth the fleet is expected to keep minus the share the other side is.
/// Measured on the round a fight is fought by (`iac_sim::combat`), in fights
/// to the end between equal ship classes, where the ratio is plain numbers
/// against numbers: 300 fights a point with 60 ships on the smaller side,
/// the mean of Light Fighters, Heavy Fighters, Cruisers, Battleships and
/// Destroyers, which lie within 0.03 of each other. (Battlecruisers, whose
/// shield takes more than half of one another's shot, end further apart:
/// 0.82 at 1.5.) So 1.5 is a fight that keeps about 69 percent of the
/// fleet's worth whatever the classes. An even fight has no margin: neither
/// side fires first.
pub const RATIO_MARGINS: [(f32, f32); 16] = [
    (0.1, -0.987),
    (0.5, -0.834),
    (0.625, -0.730),
    (0.75, -0.580),
    (0.83, -0.449),
    (0.9, -0.326),
    (1.0, 0.0),
    (1.1, 0.294),
    (1.25, 0.504),
    (1.5, 0.685),
    (1.75, 0.778),
    (2.0, 0.834),
    (2.5, 0.893),
    (3.0, 0.924),
    (4.0, 0.953),
    (10.0, 0.987),
];
const MARGIN_MAX: f32 = 0.99999;

/// The table is read on the log of what is left to the nearer end, where it
/// runs close to straight.
fn margin_depth(margin: f32) -> f32 {
    let m = margin.clamp(-MARGIN_MAX, MARGIN_MAX);
    if m >= 0.0 { -maths::ln(1.0 - m) } else { maths::ln(1.0 + m) }
}

/// A fight is rated FAVOURABLE or better only if the fleet won every time
/// the odds played it, and EVEN or better only if it won this share of
/// them. A handful of ships wins or loses whole, so what it keeps on
/// average says little of the risk; many ships end near the average and
/// the rule changes nothing for them.
pub const EVEN_WINS: f32 = 0.8;
/// How far under a band's edge a ratio is put that the rule above keeps
/// out of the band.
const RATIO_STEP: f32 = 0.01;

/// The ratio of a fight in which the fleet is expected to keep `ours_kept`
/// of its worth and the other side `theirs_kept` of its own (both 0 to 1):
/// the ratio of numbers at which equal ship classes end the same way. It is
/// then held under the band its win record does not earn: `won` is the
/// share of the fights played in which the other side was destroyed and
/// the fleet was not, `lost` the share in which the fleet was destroyed.
/// A fleet destroyed every time is DEADLY whatever it took with it.
pub fn outcome_ratio(ours_kept: f32, theirs_kept: f32, won: f32, lost: f32) -> f32 {
    let depth = margin_depth(ours_kept - theirs_kept);
    let mut low = RATIO_MARGINS[0];
    let mut ratio = RATIO_MARGINS[RATIO_MARGINS.len() - 1].0;
    for high in RATIO_MARGINS.into_iter().skip(1) {
        let (from, to) = (margin_depth(low.1), margin_depth(high.1));
        if depth <= to {
            ratio = low.0 + (high.0 - low.0) * ((depth - from) / (to - from)).clamp(0.0, 1.0);
            break;
        }
        low = high;
    }
    let most = if lost >= 1.0 {
        RATIO_RISKY - RATIO_STEP
    } else if won < EVEN_WINS {
        RATIO_EVEN - RATIO_STEP
    } else if won < 1.0 {
        RATIO_FAVOURABLE - RATIO_STEP
    } else {
        ratio
    };
    ratio.min(most)
}

/// How a fleet is expected to fare against what waits in a sector. The ratio
/// is `outcome_ratio` of the fight itself played forward (`iac_sim::odds`),
/// so it counts rapid fire, the bounce, a shield that is full again every
/// round, the damage a fleet carries and what each side fights with; between
/// equal ship classes it is numbers against numbers. Damage exchange is
/// quadratic in fleet size, so the bands sit close together around 1.0 and
/// a fight is decided long before the ratio reaches 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RatioLabel {
    Deadly,
    Risky,
    Even,
    Favourable,
    Safe,
}

impl RatioLabel {
    pub fn label(self) -> &'static str {
        match self {
            RatioLabel::Safe => "SAFE",
            RatioLabel::Favourable => "FAVOURABLE",
            RatioLabel::Even => "EVEN",
            RatioLabel::Risky => "RISKY",
            RatioLabel::Deadly => "DEADLY",
        }
    }

    /// What a fight in the band costs if it is fought to its end, as
    /// `RATIO_MARGINS` measured it: the sentence a client prints beside the
    /// label. A fleet may leave a fight; the odds do not count on it.
    pub fn outcome(self) -> &'static str {
        match self {
            RatioLabel::Safe => "a win that keeps nine tenths of the fleet",
            RatioLabel::Favourable => "a win that keeps two thirds to nine tenths of the fleet",
            RatioLabel::Even => "a win three times in four or better, at the cost of a third of the fleet or more",
            RatioLabel::Risky => "as likely lost as won, fought to the end",
            RatioLabel::Deadly => "the fleet is lost unless it leaves",
        }
    }
}

pub fn ratio_label(ratio: f32) -> RatioLabel {
    if ratio >= RATIO_SAFE {
        RatioLabel::Safe
    } else if ratio >= RATIO_FAVOURABLE {
        RatioLabel::Favourable
    } else if ratio >= RATIO_EVEN {
        RatioLabel::Even
    } else if ratio >= RATIO_RISKY {
        RatioLabel::Risky
    } else {
        RatioLabel::Deadly
    }
}
