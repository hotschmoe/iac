// Single source of truth for all balance formulas, costs, times, and modifiers.
// Pure data/formula file -- no side effects, no allocations.

use serde::{Deserialize, Serialize};

use crate::constants::{Density, ResourceKind, Resources, ShipClass, ShipStats};
use crate::pace::Pace;

pub const MAX_BUILDING_LEVEL: u8 = 20;

// ── Building System ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum BuildingType {
    MetalMine = 0,
    CrystalMine,
    DeuteriumSynthesizer,
    Shipyard,
    ResearchLab,
    FuelDepot,
    SensorArray,
    DefenseGrid,
    StorageVault,
    Fabricator,
}

impl BuildingType {
    pub fn label(self) -> &'static str {
        match self {
            BuildingType::MetalMine => "Metal Mine",
            BuildingType::CrystalMine => "Crystal Mine",
            BuildingType::DeuteriumSynthesizer => "Deut Synthesizer",
            BuildingType::Shipyard => "Shipyard",
            BuildingType::ResearchLab => "Research Lab",
            BuildingType::FuelDepot => "Fuel Depot",
            BuildingType::SensorArray => "Sensor Array",
            BuildingType::DefenseGrid => "Defense Grid",
            BuildingType::StorageVault => "Storage Vault",
            BuildingType::Fabricator => "Fabricator",
        }
    }

    /// Number of building types.
    pub const COUNT: usize = 10;

    pub fn short_label(self) -> &'static str {
        match self {
            BuildingType::MetalMine => "Metal",
            BuildingType::CrystalMine => "Crystal",
            BuildingType::DeuteriumSynthesizer => "Deut",
            BuildingType::Shipyard => "Shipyd",
            BuildingType::ResearchLab => "Lab",
            BuildingType::FuelDepot => "Depot",
            BuildingType::SensorArray => "Sensor",
            BuildingType::DefenseGrid => "DefGrd",
            BuildingType::StorageVault => "Vault",
            BuildingType::Fabricator => "Fab",
        }
    }
    pub fn from_usize(idx: usize) -> Option<Self> {
        match idx {
            0 => Some(BuildingType::MetalMine),
            1 => Some(BuildingType::CrystalMine),
            2 => Some(BuildingType::DeuteriumSynthesizer),
            3 => Some(BuildingType::Shipyard),
            4 => Some(BuildingType::ResearchLab),
            5 => Some(BuildingType::FuelDepot),
            6 => Some(BuildingType::SensorArray),
            7 => Some(BuildingType::DefenseGrid),
            8 => Some(BuildingType::StorageVault),
            9 => Some(BuildingType::Fabricator),
            _ => None,
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingLevels {
    pub metal_mine: u8,
    pub crystal_mine: u8,
    pub deuterium_synthesizer: u8,
    pub shipyard: u8,
    pub research_lab: u8,
    pub fuel_depot: u8,
    pub sensor_array: u8,
    pub defense_grid: u8,
    pub storage_vault: u8,
    pub fabricator: u8,
}

impl Default for BuildingLevels {
    /// New homeworlds start with level-1 metal and crystal mines.
    fn default() -> Self {
        BuildingLevels {
            metal_mine: 1,
            crystal_mine: 1,
            deuterium_synthesizer: 0,
            shipyard: 0,
            research_lab: 0,
            fuel_depot: 0,
            sensor_array: 0,
            defense_grid: 0,
            storage_vault: 0,
            fabricator: 0,
        }
    }
}

impl BuildingLevels {
    pub fn get(&self, building: BuildingType) -> u8 {
        match building {
            BuildingType::MetalMine => self.metal_mine,
            BuildingType::CrystalMine => self.crystal_mine,
            BuildingType::DeuteriumSynthesizer => self.deuterium_synthesizer,
            BuildingType::Shipyard => self.shipyard,
            BuildingType::ResearchLab => self.research_lab,
            BuildingType::FuelDepot => self.fuel_depot,
            BuildingType::SensorArray => self.sensor_array,
            BuildingType::DefenseGrid => self.defense_grid,
            BuildingType::StorageVault => self.storage_vault,
            BuildingType::Fabricator => self.fabricator,
        }
    }

    pub fn set(&mut self, building: BuildingType, level: u8) {
        match building {
            BuildingType::MetalMine => self.metal_mine = level,
            BuildingType::CrystalMine => self.crystal_mine = level,
            BuildingType::DeuteriumSynthesizer => self.deuterium_synthesizer = level,
            BuildingType::Shipyard => self.shipyard = level,
            BuildingType::ResearchLab => self.research_lab = level,
            BuildingType::FuelDepot => self.fuel_depot = level,
            BuildingType::SensorArray => self.sensor_array = level,
            BuildingType::DefenseGrid => self.defense_grid = level,
            BuildingType::StorageVault => self.storage_vault = level,
            BuildingType::Fabricator => self.fabricator = level,
        }
    }
}

/// Per game hour at pace 1: `base * level * 1.1^level`.
pub const PRODUCTION_PER_HOUR_METAL: f64 = 10.0;
pub const PRODUCTION_PER_HOUR_CRYSTAL: f64 = 7.0;
pub const PRODUCTION_PER_HOUR_DEUTERIUM: f64 = 5.0;
pub const PRODUCTION_GROWTH: f64 = 1.1;

/// (metal + crystal) of cost that one game hour of build, research or ship
/// time pays for.
pub const BUILD_RATE: f64 = 700.0;
pub const RESEARCH_RATE: f64 = 500.0;
pub const SHIP_RATE: f64 = 1200.0;
/// Shipyard level divides ship time by `1 + this * level`.
pub const SHIPYARD_SPEED: f64 = 0.10;
/// Fabricator level divides building time by `1 + this * level`.
pub const FABRICATOR_SPEED: f64 = 0.15;
/// Research Lab level divides research time by `1 + this * level`.
pub const LAB_SPEED: f64 = 0.10;
/// Orders that may wait behind the running ones in each queue (building,
/// research, shipyard). The running items come on top, so the building queue
/// holds `slots + 3`. Three because a waiting order is a claim on future
/// income: enough to queue a whole session ahead (a mine, a lab level, a
/// ship batch) but too few to bank a long list of unaffordable upgrades that
/// holds the line, since the queue is first in, first out.
pub const QUEUE_WAITING: usize = 3;

/// Stockpile cap with no Vault, before the pace multiplier.
pub const STORAGE_BASE: Resources = Resources { metal: 5000.0, crystal: 3500.0, deuterium: 2500.0 };
/// Each Vault level multiplies the caps by this.
pub const STORAGE_GROWTH: f32 = 1.5;
/// Share of the cap per Vault level that a raid cannot skim, up to the max.
pub const STORAGE_PROTECT_PER_LEVEL: f32 = 0.08;
pub const STORAGE_PROTECT_MAX: f32 = 0.60;
/// `StorageNearCap` fires at this fill ratio.
pub const STORAGE_NEAR_CAP_RATIO: f32 = 0.85;

/// Stockpile cap per resource: `base * 1.5^vault * P^0.75`.
pub fn storage_cap(vault_level: u8, pace: &Pace) -> Resources {
    STORAGE_BASE.scale(STORAGE_GROWTH.powi(vault_level as i32) * pace.cap_mult())
}

/// Per resource, the stock a raid cannot take.
pub fn storage_protected(vault_level: u8, pace: &Pace) -> Resources {
    let share = (STORAGE_PROTECT_PER_LEVEL * vault_level as f32).min(STORAGE_PROTECT_MAX);
    storage_cap(vault_level, pace).scale(share)
}

/// Smallest Vault level whose cap holds `amount` of `kind`; None when even
/// level 20 cannot.
pub fn vault_level_for(amount: f32, kind: ResourceKind, pace: &Pace) -> Option<u8> {
    (0..=MAX_BUILDING_LEVEL).find(|&level| storage_cap(level, pace).get(kind) >= amount)
}

/// Mine output per tick at the world's pace.
pub fn production_per_tick(building: BuildingType, level: u8, pace: &Pace) -> f32 {
    if level == 0 {
        return 0.0;
    }
    let per_hour = match building {
        BuildingType::MetalMine => PRODUCTION_PER_HOUR_METAL,
        BuildingType::CrystalMine => PRODUCTION_PER_HOUR_CRYSTAL,
        BuildingType::DeuteriumSynthesizer => PRODUCTION_PER_HOUR_DEUTERIUM,
        _ => return 0.0,
    };
    let level = level as f64;
    pace.rate((per_hour / 3600.0 * level * PRODUCTION_GROWTH.powf(level)) as f32)
}

/// Level-1 cost and the per-level growth factor of a building.
pub fn building_cost_curve(building: BuildingType) -> ([f64; 3], f64) {
    match building {
        BuildingType::MetalMine => ([60.0, 15.0, 0.0], 1.5),
        BuildingType::CrystalMine => ([48.0, 24.0, 0.0], 1.6),
        BuildingType::DeuteriumSynthesizer => ([150.0, 50.0, 0.0], 1.5),
        BuildingType::Shipyard => ([200.0, 100.0, 50.0], 1.8),
        BuildingType::ResearchLab => ([100.0, 200.0, 50.0], 1.8),
        BuildingType::FuelDepot => ([150.0, 50.0, 100.0], 1.7),
        BuildingType::SensorArray => ([100.0, 150.0, 75.0], 1.7),
        BuildingType::DefenseGrid => ([300.0, 200.0, 100.0], 1.8),
        BuildingType::StorageVault => ([200.0, 100.0, 0.0], 1.7),
        BuildingType::Fabricator => ([120.0, 80.0, 40.0], 1.9),
    }
}

/// `base * growth^(level - 1)`, rounded to whole units.
fn grown_cost(base: [f64; 3], growth: f64, level: u8) -> Resources {
    let factor = growth.powi(level.max(1) as i32 - 1);
    let unit = |x: f64| (x * factor).round_ties_even() as f32;
    Resources { metal: unit(base[0]), crystal: unit(base[1]), deuterium: unit(base[2]) }
}

/// Cost to build level N.
pub fn building_cost(building: BuildingType, level: u8) -> Resources {
    let (base, growth) = building_cost_curve(building);
    grown_cost(base, growth, level)
}

/// Seconds (= ticks at pace 1) that `cost` takes at `rate` cost-units per hour.
fn cost_seconds(cost: Resources, rate: f64, speed_divisor: f64) -> f64 {
    (cost.metal as f64 + cost.crystal as f64) / rate * 3600.0 / speed_divisor
}

/// Ticks to build level N: derived from the cost, so time and cost share an
/// exponent.
pub fn building_time(building: BuildingType, level: u8, fabricator_level: u8, pace: &Pace) -> u64 {
    if level == 0 {
        return 0;
    }
    let divisor = 1.0 + FABRICATOR_SPEED * fabricator_level as f64;
    pace.econ_ticks(cost_seconds(building_cost(building, level), BUILD_RATE, divisor))
}

#[derive(Debug, Clone, Copy)]
pub struct Prerequisite {
    pub building: BuildingType,
    pub level: u8,
}

pub fn building_prerequisites(building: BuildingType) -> Option<Prerequisite> {
    match building {
        BuildingType::MetalMine
        | BuildingType::CrystalMine
        | BuildingType::DeuteriumSynthesizer => None,
        BuildingType::Shipyard => Some(Prerequisite {
            building: BuildingType::MetalMine,
            level: 2,
        }),
        BuildingType::ResearchLab => Some(Prerequisite {
            building: BuildingType::CrystalMine,
            level: 2,
        }),
        BuildingType::FuelDepot => Some(Prerequisite {
            building: BuildingType::DeuteriumSynthesizer,
            level: 2,
        }),
        BuildingType::SensorArray => Some(Prerequisite {
            building: BuildingType::ResearchLab,
            level: 1,
        }),
        BuildingType::DefenseGrid => Some(Prerequisite {
            building: BuildingType::Shipyard,
            level: 3,
        }),
        BuildingType::StorageVault => Some(Prerequisite {
            building: BuildingType::MetalMine,
            level: 2,
        }),
        BuildingType::Fabricator => Some(Prerequisite {
            building: BuildingType::Shipyard,
            level: 2,
        }),
    }
}

pub fn building_prerequisites_met(building: BuildingType, levels: &BuildingLevels) -> bool {
    if let Some(prereq) = building_prerequisites(building) {
        levels.get(prereq.building) >= prereq.level
    } else {
        true
    }
}

// ── Research System ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ResearchType {
    FuelEfficiency = 0,
    ExtendedFuelTanks,
    ReinforcedHulls,
    AdvancedShields,
    WeaponsResearch,
    Navigation,
    HarvestingEfficiency,
    CorvetteTech,
    FrigateTech,
    CruiserTech,
    HaulerTech,
    EmergencyJump,
    ModularFabrication,
}

impl ResearchType {
    pub const ALL: [ResearchType; 13] = [
        ResearchType::FuelEfficiency,
        ResearchType::ExtendedFuelTanks,
        ResearchType::ReinforcedHulls,
        ResearchType::AdvancedShields,
        ResearchType::WeaponsResearch,
        ResearchType::Navigation,
        ResearchType::HarvestingEfficiency,
        ResearchType::CorvetteTech,
        ResearchType::FrigateTech,
        ResearchType::CruiserTech,
        ResearchType::HaulerTech,
        ResearchType::EmergencyJump,
        ResearchType::ModularFabrication,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ResearchType::FuelEfficiency => "Fuel Efficiency",
            ResearchType::ExtendedFuelTanks => "Extended Tanks",
            ResearchType::ReinforcedHulls => "Reinforced Hulls",
            ResearchType::AdvancedShields => "Advanced Shields",
            ResearchType::WeaponsResearch => "Weapons Research",
            ResearchType::Navigation => "Navigation",
            ResearchType::HarvestingEfficiency => "Harvesting Eff.",
            ResearchType::CorvetteTech => "Corvette Tech",
            ResearchType::FrigateTech => "Frigate Tech",
            ResearchType::CruiserTech => "Cruiser Tech",
            ResearchType::HaulerTech => "Hauler Tech",
            ResearchType::EmergencyJump => "Emergency Jump",
            ResearchType::ModularFabrication => "Modular Fabrication",
        }
    }

    /// Number of research types.
    pub const COUNT: usize = 13;

    pub fn from_usize(idx: usize) -> Option<Self> {
        match idx {
            0 => Some(ResearchType::FuelEfficiency),
            1 => Some(ResearchType::ExtendedFuelTanks),
            2 => Some(ResearchType::ReinforcedHulls),
            3 => Some(ResearchType::AdvancedShields),
            4 => Some(ResearchType::WeaponsResearch),
            5 => Some(ResearchType::Navigation),
            6 => Some(ResearchType::HarvestingEfficiency),
            7 => Some(ResearchType::CorvetteTech),
            8 => Some(ResearchType::FrigateTech),
            9 => Some(ResearchType::CruiserTech),
            10 => Some(ResearchType::HaulerTech),
            11 => Some(ResearchType::EmergencyJump),
            12 => Some(ResearchType::ModularFabrication),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResearchLevels {
    pub fuel_efficiency: u8,
    pub extended_fuel_tanks: u8,
    pub reinforced_hulls: u8,
    pub advanced_shields: u8,
    pub weapons_research: u8,
    pub navigation: u8,
    pub harvesting_efficiency: u8,
    pub corvette_tech: u8,
    pub frigate_tech: u8,
    pub cruiser_tech: u8,
    pub hauler_tech: u8,
    pub emergency_jump: u8,
    pub modular_fabrication: u8,
}

impl ResearchLevels {
    pub fn get(&self, tech: ResearchType) -> u8 {
        match tech {
            ResearchType::FuelEfficiency => self.fuel_efficiency,
            ResearchType::ExtendedFuelTanks => self.extended_fuel_tanks,
            ResearchType::ReinforcedHulls => self.reinforced_hulls,
            ResearchType::AdvancedShields => self.advanced_shields,
            ResearchType::WeaponsResearch => self.weapons_research,
            ResearchType::Navigation => self.navigation,
            ResearchType::HarvestingEfficiency => self.harvesting_efficiency,
            ResearchType::CorvetteTech => self.corvette_tech,
            ResearchType::FrigateTech => self.frigate_tech,
            ResearchType::CruiserTech => self.cruiser_tech,
            ResearchType::HaulerTech => self.hauler_tech,
            ResearchType::EmergencyJump => self.emergency_jump,
            ResearchType::ModularFabrication => self.modular_fabrication,
        }
    }

    pub fn set(&mut self, tech: ResearchType, level: u8) {
        match tech {
            ResearchType::FuelEfficiency => self.fuel_efficiency = level,
            ResearchType::ExtendedFuelTanks => self.extended_fuel_tanks = level,
            ResearchType::ReinforcedHulls => self.reinforced_hulls = level,
            ResearchType::AdvancedShields => self.advanced_shields = level,
            ResearchType::WeaponsResearch => self.weapons_research = level,
            ResearchType::Navigation => self.navigation = level,
            ResearchType::HarvestingEfficiency => self.harvesting_efficiency = level,
            ResearchType::CorvetteTech => self.corvette_tech = level,
            ResearchType::FrigateTech => self.frigate_tech = level,
            ResearchType::CruiserTech => self.cruiser_tech = level,
            ResearchType::HaulerTech => self.hauler_tech = level,
            ResearchType::EmergencyJump => self.emergency_jump = level,
            ResearchType::ModularFabrication => self.modular_fabrication = level,
        }
    }
}

pub fn research_max_level(tech: ResearchType) -> u8 {
    match tech {
        ResearchType::CorvetteTech
        | ResearchType::FrigateTech
        | ResearchType::CruiserTech
        | ResearchType::HaulerTech => 1,
        ResearchType::EmergencyJump => 3,
        ResearchType::ModularFabrication => 2,
        _ => 5,
    }
}

/// Level-1 cost and per-level growth. Ship techs are one-shot (growth 1).
pub fn research_cost_curve(tech: ResearchType) -> ([f64; 3], f64) {
    match tech {
        ResearchType::CorvetteTech => ([400.0, 200.0, 100.0], 1.0),
        ResearchType::FrigateTech => ([2000.0, 1200.0, 600.0], 1.0),
        ResearchType::CruiserTech => ([8000.0, 5000.0, 2500.0], 1.0),
        ResearchType::HaulerTech => ([600.0, 300.0, 200.0], 1.0),
        ResearchType::FuelEfficiency => ([200.0, 100.0, 100.0], RESEARCH_GROWTH),
        ResearchType::ExtendedFuelTanks => ([150.0, 75.0, 150.0], RESEARCH_GROWTH),
        ResearchType::ReinforcedHulls => ([300.0, 100.0, 0.0], RESEARCH_GROWTH),
        ResearchType::AdvancedShields => ([100.0, 300.0, 50.0], RESEARCH_GROWTH),
        ResearchType::WeaponsResearch => ([200.0, 200.0, 100.0], RESEARCH_GROWTH),
        ResearchType::Navigation => ([100.0, 150.0, 50.0], RESEARCH_GROWTH),
        ResearchType::HarvestingEfficiency => ([150.0, 100.0, 75.0], RESEARCH_GROWTH),
        ResearchType::EmergencyJump => ([500.0, 400.0, 300.0], RESEARCH_GROWTH),
        ResearchType::ModularFabrication => ([1500.0, 1200.0, 500.0], 2.0),
    }
}

/// Per-level cost growth of the leveled techs.
pub const RESEARCH_GROWTH: f64 = 1.6;

pub fn research_cost(tech: ResearchType, level: u8) -> Resources {
    let (base, growth) = research_cost_curve(tech);
    grown_cost(base, growth, level)
}

pub fn research_time(tech: ResearchType, level: u8, lab_level: u8, pace: &Pace) -> u64 {
    if level == 0 {
        return 0;
    }
    let divisor = 1.0 + LAB_SPEED * lab_level as f64;
    pace.econ_ticks(cost_seconds(research_cost(tech, level), RESEARCH_RATE, divisor))
}

#[derive(Debug, Clone, Copy)]
pub enum ResearchPrereqKind {
    Building(Prerequisite),
    Research { tech: ResearchType, level: u8 },
}

pub fn research_prerequisites(tech: ResearchType) -> [Option<ResearchPrereqKind>; 2] {
    match tech {
        ResearchType::FuelEfficiency => [
            Some(ResearchPrereqKind::Building(Prerequisite {
                building: BuildingType::DeuteriumSynthesizer,
                level: 3,
            })),
            None,
        ],
        ResearchType::ExtendedFuelTanks => [
            Some(ResearchPrereqKind::Building(Prerequisite {
                building: BuildingType::FuelDepot,
                level: 2,
            })),
            None,
        ],
        ResearchType::ReinforcedHulls => [
            Some(ResearchPrereqKind::Building(Prerequisite {
                building: BuildingType::Shipyard,
                level: 2,
            })),
            None,
        ],
        ResearchType::AdvancedShields => [
            Some(ResearchPrereqKind::Building(Prerequisite {
                building: BuildingType::ResearchLab,
                level: 3,
            })),
            None,
        ],
        ResearchType::WeaponsResearch => [
            Some(ResearchPrereqKind::Building(Prerequisite {
                building: BuildingType::ResearchLab,
                level: 3,
            })),
            None,
        ],
        ResearchType::Navigation => [
            Some(ResearchPrereqKind::Building(Prerequisite {
                building: BuildingType::ResearchLab,
                level: 2,
            })),
            None,
        ],
        ResearchType::HarvestingEfficiency => [
            Some(ResearchPrereqKind::Building(Prerequisite {
                building: BuildingType::ResearchLab,
                level: 2,
            })),
            None,
        ],
        ResearchType::CorvetteTech => [
            Some(ResearchPrereqKind::Building(Prerequisite {
                building: BuildingType::Shipyard,
                level: 1,
            })),
            None,
        ],
        ResearchType::FrigateTech => [
            Some(ResearchPrereqKind::Research {
                tech: ResearchType::CorvetteTech,
                level: 1,
            }),
            Some(ResearchPrereqKind::Building(Prerequisite {
                building: BuildingType::Shipyard,
                level: 4,
            })),
        ],
        ResearchType::CruiserTech => [
            Some(ResearchPrereqKind::Research {
                tech: ResearchType::FrigateTech,
                level: 1,
            }),
            Some(ResearchPrereqKind::Building(Prerequisite {
                building: BuildingType::Shipyard,
                level: 6,
            })),
        ],
        ResearchType::HaulerTech => [
            Some(ResearchPrereqKind::Building(Prerequisite {
                building: BuildingType::Shipyard,
                level: 3,
            })),
            None,
        ],
        ResearchType::EmergencyJump => [
            Some(ResearchPrereqKind::Research {
                tech: ResearchType::Navigation,
                level: 1,
            }),
            Some(ResearchPrereqKind::Building(Prerequisite {
                building: BuildingType::ResearchLab,
                level: 4,
            })),
        ],
        ResearchType::ModularFabrication => [
            Some(ResearchPrereqKind::Building(Prerequisite {
                building: BuildingType::ResearchLab,
                level: 4,
            })),
            None,
        ],
    }
}

pub fn research_prerequisites_met(
    tech: ResearchType,
    buildings: &BuildingLevels,
    research: &ResearchLevels,
) -> bool {
    for p in research_prerequisites(tech).into_iter().flatten() {
        match p {
            ResearchPrereqKind::Building(b) => {
                if buildings.get(b.building) < b.level {
                    return false;
                }
            }
            ResearchPrereqKind::Research { tech: t, level: l } => {
                if research.get(t) < l {
                    return false;
                }
            }
        }
    }
    true
}

// ── Research Modifier Functions ──────────────────────────────────

pub fn apply_research_to_stats(base: ShipStats, research: &ResearchLevels) -> ShipStats {
    let hull_bonus = 1.0 + 0.10 * research.reinforced_hulls as f32;
    let shield_bonus = 1.0 + 0.10 * research.advanced_shields as f32;
    let weapon_bonus = 1.0 + 0.10 * research.weapons_research as f32;
    let fuel_cap = fuel_capacity_modifier(research.extended_fuel_tanks);
    ShipStats {
        hull: base.hull * hull_bonus,
        shield: base.shield * shield_bonus,
        weapon: base.weapon * weapon_bonus,
        speed: base.speed,
        cargo: base.cargo,
        fuel: (base.fuel as f32 * fuel_cap) as u16,
    }
}

/// Buildings that can be under construction at once: one, plus one per
/// Modular Fabrication level.
pub fn building_slots(research: &ResearchLevels) -> u8 {
    1 + research.modular_fabrication
}

pub fn fuel_rate_modifier(fuel_eff_level: u8) -> f32 {
    1.0 - 0.10 * fuel_eff_level as f32
}

pub fn fuel_capacity_modifier(tanks_level: u8) -> f32 {
    1.0 + 0.15 * tanks_level as f32
}

pub fn fuel_depot_modifier(depot_level: u8) -> f32 {
    1.0 + 0.25 * depot_level as f32
}

pub fn sensor_range(sensor_level: u8) -> u8 {
    sensor_level
}

pub fn navigation_cooldown_reduction(nav_level: u8) -> u16 {
    nav_level as u16
}

/// Cargo gained per raw unit extracted. Research raises the yield, not the
/// extraction speed, so it does not strip small tiles faster.
pub fn harvest_yield(harvest_eff_level: u8) -> f32 {
    1.0 + 0.20 * harvest_eff_level as f32
}

pub fn recall_damage_reduction(ej_level: u8) -> f32 {
    0.05 * ej_level as f32
}

/// The research that unlocks a ship class, if it needs one.
pub fn ship_class_tech(class: ShipClass) -> Option<ResearchType> {
    match class {
        ShipClass::Scout => None,
        ShipClass::Corvette => Some(ResearchType::CorvetteTech),
        ShipClass::Frigate => Some(ResearchType::FrigateTech),
        ShipClass::Cruiser => Some(ResearchType::CruiserTech),
        ShipClass::Hauler => Some(ResearchType::HaulerTech),
    }
}

pub fn ship_class_unlocked(class: ShipClass, research: &ResearchLevels) -> bool {
    ship_class_tech(class).is_none_or(|tech| research.get(tech) >= 1)
}

/// Ticks to build one ship; the Shipyard divides it by `1 + 0.1 * level`.
pub fn ship_build_time(class: ShipClass, shipyard_level: u8, pace: &Pace) -> u64 {
    let divisor = 1.0 + SHIPYARD_SPEED * shipyard_level as f64;
    pace.econ_ticks(cost_seconds(class.build_cost(), SHIP_RATE, divisor))
}

pub const CANCEL_REFUND_FRACTION: f32 = 0.50;

// ── Defence structures ───────────────────────────────────────────

/// Home defences built from the shipyard queue. They never move, burn no
/// fuel, and a raid can wear them down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DefenceKind {
    PulseTurret = 0,
    LancerBattery,
    IonBastion,
}

impl DefenceKind {
    pub const COUNT: usize = 3;
    pub const ALL: [DefenceKind; 3] = [DefenceKind::PulseTurret, DefenceKind::LancerBattery, DefenceKind::IonBastion];

    pub fn label(self) -> &'static str {
        match self {
            DefenceKind::PulseTurret => "Pulse Turret",
            DefenceKind::LancerBattery => "Lancer Battery",
            DefenceKind::IonBastion => "Ion Bastion",
        }
    }

    pub fn from_usize(idx: usize) -> Option<Self> {
        Self::ALL.get(idx).copied()
    }

    pub fn build_cost(self) -> Resources {
        match self {
            DefenceKind::PulseTurret => Resources { metal: 120.0, crystal: 20.0, deuterium: 0.0 },
            DefenceKind::LancerBattery => Resources { metal: 400.0, crystal: 120.0, deuterium: 20.0 },
            DefenceKind::IonBastion => Resources { metal: 1200.0, crystal: 500.0, deuterium: 100.0 },
        }
    }

    /// (weapon, hull, shield), the same stats ships carry.
    fn stats(self) -> (f32, f32, f32) {
        match self {
            DefenceKind::PulseTurret => (12.0, 40.0, 10.0),
            DefenceKind::LancerBattery => (35.0, 120.0, 40.0),
            DefenceKind::IonBastion => (90.0, 300.0, 150.0),
        }
    }

    /// `weapon + (hull + shield) / 10`, the scale ships, raids and threat
    /// ratings share.
    pub fn power(self) -> f32 {
        let (weapon, hull, shield) = self.stats();
        weapon + (hull + shield) / 10.0
    }

    /// Building and research levels needed before this can be ordered.
    pub fn prerequisites(self) -> &'static [(PrereqRef, u8)] {
        match self {
            DefenceKind::PulseTurret => &[(PrereqRef::Building(BuildingType::DefenseGrid), 1)],
            DefenceKind::LancerBattery => &[
                (PrereqRef::Building(BuildingType::DefenseGrid), 2),
                (PrereqRef::Research(ResearchType::WeaponsResearch), 2),
            ],
            DefenceKind::IonBastion => &[
                (PrereqRef::Building(BuildingType::DefenseGrid), 4),
                (PrereqRef::Research(ResearchType::WeaponsResearch), 4),
                (PrereqRef::Research(ResearchType::AdvancedShields), 3),
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrereqRef {
    Building(BuildingType),
    Research(ResearchType),
}

pub fn defence_prerequisites_met(kind: DefenceKind, buildings: &BuildingLevels, research: &ResearchLevels) -> bool {
    kind.prerequisites().iter().all(|(p, need)| match p {
        PrereqRef::Building(b) => buildings.get(*b) >= *need,
        PrereqRef::Research(t) => research.get(*t) >= *need,
    })
}

/// Ticks to build one structure; the Shipyard speeds it like ships.
pub fn defence_build_time(kind: DefenceKind, shipyard_level: u8, pace: &Pace) -> u64 {
    let divisor = 1.0 + SHIPYARD_SPEED * shipyard_level as f64;
    pace.econ_ticks(cost_seconds(kind.build_cost(), SHIP_RATE, divisor))
}

/// Each Defense Grid level strengthens every structure by this much.
pub const DEFENCE_GRID_BONUS: f32 = 0.04;

/// Power of one structure at a Defense Grid level.
pub fn structure_power(kind: DefenceKind, grid_level: u8) -> f32 {
    kind.power() * (1.0 + DEFENCE_GRID_BONUS * grid_level as f32)
}

/// What the shipyard queue builds: a ship class or a defence structure.
/// On the wire it is just the name ("Corvette", "PulseTurret").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ShipyardItem {
    Ship(ShipClass),
    Defence(DefenceKind),
}

impl ShipyardItem {
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

    pub fn unit_ticks(self, shipyard_level: u8, pace: &Pace) -> u64 {
        match self {
            ShipyardItem::Ship(c) => ship_build_time(c, shipyard_level, pace),
            ShipyardItem::Defence(d) => defence_build_time(d, shipyard_level, pace),
        }
    }
}

// ── Raids and score points ───────────────────────────────────────

/// How much a bundle of resources counts toward score and raid sizing:
/// crystal and deuterium are scarcer than metal.
pub fn resource_weight(r: &Resources) -> f32 {
    (r.metal + 1.5 * r.crystal + 2.0 * r.deuterium) / 1000.0
}

/// Points for every completed building and research level, at the cost paid.
/// Fleets and defences do not count.
pub fn econ_points(buildings: &BuildingLevels, research: &ResearchLevels) -> f32 {
    let mut points = 0.0;
    for b in (0..BuildingType::COUNT).filter_map(BuildingType::from_usize) {
        for level in 1..=buildings.get(b) {
            points += resource_weight(&building_cost(b, level));
        }
    }
    for t in ResearchType::ALL {
        for level in 1..=research.get(t) {
            points += resource_weight(&research_cost(t, level));
        }
    }
    points
}

/// Mean power of the next raid on a player with `econ` points; the actual
/// raid is rolled between 0.80 and 1.15 times this.
pub fn raid_power_base(econ: f32) -> f32 {
    5.0 + 11.0 * econ.powf(0.75)
}

// ── Ore ──────────────────────────────────────────────────────────

pub const ORE_RING_GROWTH: f32 = 1.10;
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
pub fn ring_mult(dist: u16) -> f32 {
    if dist < ORE_RING_FIRST {
        1.0
    } else {
        ORE_RING_GROWTH.powi(i32::from(dist.min(ORE_RING_LAST)) - i32::from(ORE_RING_FIRST))
    }
}

/// Raw units to harvest at `density` before it drops a level.
pub fn ore_step_units(density: Density, dist: u16) -> f32 {
    density.step_units() * ring_mult(dist)
}

/// Raw units a full tile of `density` holds.
pub fn ore_reserve_units(density: Density, dist: u16) -> f32 {
    density.reserve_units() * ring_mult(dist)
}

pub fn ore_regen_hours(dist: u16) -> f32 {
    ORE_REGEN_BASE_HOURS + ORE_REGEN_HOURS_PER_RING * dist as f32
}

/// Raw units a stripped tile with template `density` regains per tick: its
/// whole reserve over `ore_regen_hours`, faster at higher pace.
pub fn ore_regen_per_tick(density: Density, dist: u16, pace: &Pace) -> f32 {
    pace.rate(ore_reserve_units(density, dist) / (ore_regen_hours(dist) * 3600.0))
}

/// Raw units already taken from a tile generated at `template` that now
/// stands at `current` with `harvested` units gone from the current level.
pub fn ore_extracted(template: Density, current: Density, harvested: f32, dist: u16) -> f32 {
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
pub fn ore_regen_step(template: Density, current: Density, harvested: f32, amount: f32, dist: u16) -> (Density, f32) {
    let mut level = current;
    let mut gone = harvested - amount;
    while gone < 0.0 && level != template {
        level = level.upgrade();
        gone += ore_step_units(level, dist);
    }
    (level, gone.max(0.0))
}

/// Slack for float drift when harvesting empties a level exactly.
pub const ORE_EPSILON: f32 = 1e-3;

/// Chances of (Sparse, Moderate, Rich, Pristine) for an ore tile at `dist`.
pub fn ore_density_odds(dist: u16) -> [f32; 4] {
    let drift = (ORE_DRIFT_PER_RING * dist as f32).min(ORE_DRIFT_MAX);
    [0.40 - drift, 0.30 - 0.2 * drift, 0.20 + 0.6 * drift, 0.10 + 0.6 * drift]
}

// ── Loot ─────────────────────────────────────────────────────────

pub const WRECK_VALUE_K: f32 = 1.2;
pub const WRECK_VALUE_EXPONENT: f32 = 0.8;
/// Metal, crystal and deuterium shares of any loot value.
pub const LOOT_SPLIT: [f32; 3] = [0.55, 0.30, 0.15];
pub const DERELICT_VALUE_BASE: f32 = 240.0;
pub const DERELICT_VALUE_GROWTH: f32 = 1.14;
/// A site pays between these shares of its value, uniformly.
pub const DERELICT_ROLL_MIN: f32 = 0.6;
pub const DERELICT_ROLL_MAX: f32 = 1.4;
/// Score points an ancient relic is worth.
pub const RELIC_POINTS: f32 = 30.0;

/// Anti-farming: every kill in a sector adds one point of heat, and the next
/// kill's loot and score are multiplied by `FARM_DECAY^heat`. Heat halves
/// every `FARM_HALF_LIFE_RESPAWNS` respawn delays, so a group left to respawn
/// a few times pays in full again but one sector is not an endless tap: at
/// the respawn rate its steady yield is about a fifth of a fresh kill.
pub const FARM_DECAY: f32 = 0.75;
pub const FARM_HALF_LIFE_RESPAWNS: f32 = 4.0;

/// Share of a fresh kill's loot and score a sector with `heat` pays.
pub fn farm_factor(heat: f32) -> f32 {
    FARM_DECAY.powf(heat.max(0.0))
}

/// `heat` after `elapsed` ticks with the given half-life.
pub fn farm_heat_after(heat: f32, elapsed: u64, half_life_ticks: u64) -> f32 {
    if heat <= 0.0 || half_life_ticks == 0 {
        return 0.0;
    }
    heat * 0.5f32.powf(elapsed as f32 / half_life_ticks as f32)
}

/// Units of loot (before the pace's finds factor) a destroyed group leaves:
/// it grows slower than the danger, so far hunting stays a gamble.
pub fn wreck_value(group_power: f32) -> f32 {
    WRECK_VALUE_K * group_power.max(0.0).powf(WRECK_VALUE_EXPONENT)
}

/// `value` units as metal, crystal and deuterium.
pub fn split_loot(value: f32) -> Resources {
    Resources { metal: value * LOOT_SPLIT[0], crystal: value * LOOT_SPLIT[1], deuterium: value * LOOT_SPLIT[2] }
}

/// Mean loot value (units, before the finds factor) of a derelict `dist` rings out.
pub fn derelict_value(dist: u16) -> f32 {
    DERELICT_VALUE_BASE * DERELICT_VALUE_GROWTH.powi(i32::from(dist) - 4)
}

/// Tier follows the threat band of the ring: 1 for T1 to T3, 2 for T4 to T6, 3 above.
pub fn derelict_tier(dist: u16) -> u8 {
    threat_band(threat_rating(npc_power(dist)))
}

/// Chance a boarding wakes the guard, before scouts and earlier failures.
pub fn derelict_ambush(rating: u8) -> f32 {
    (0.10 + 0.03 * (f32::from(rating) - 1.0)).min(0.35)
}

/// Share of the ring's group power a derelict's guard fields.
pub const AMBUSH_GUARD_SHARE: f32 = 0.6;

/// (class, count, stat multiplier) of the guard waiting in a derelict `dist` rings out.
pub fn ambush_guard(dist: u16) -> (ShipClass, u8, f32) {
    let class = npc_class(dist);
    let m = npc_stat_multiplier(dist);
    let count = (AMBUSH_GUARD_SHARE * npc_power(dist) / (ship_power(class) * m)).round();
    (class, count.clamp(1.0, NPC_MAX_SHIPS as f32) as u8, m)
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

pub const NPC_POWER_BASE: f32 = 4.0;
/// NPC group power grows by this factor per ring.
pub const NPC_POWER_GROWTH: f32 = 1.22;
pub const NPC_MAX_SHIPS: usize = 32;
pub const MAX_THREAT: u8 = 9;
/// Observed Aggressive and Swarm groups are rated this much stronger than
/// their ships add up to: they come to you.
pub const THREAT_AGGRESSIVE_BONUS: f32 = 1.25;

/// How much one ship counts on the scale raids, threat and ratios share.
pub fn ship_power(class: ShipClass) -> f32 {
    let s = class.base_stats();
    s.weapon + (s.hull + s.shield) / 10.0
}

/// Target combat power of the NPC group guarding a sector `dist` rings out.
pub fn npc_power(dist: u16) -> f32 {
    NPC_POWER_BASE * NPC_POWER_GROWTH.powi(dist.max(1) as i32 - 1)
}

pub fn npc_class(dist: u16) -> ShipClass {
    match dist {
        0..=5 => ShipClass::Scout,
        6..=12 => ShipClass::Corvette,
        13..=20 => ShipClass::Frigate,
        _ => ShipClass::Cruiser,
    }
}

pub fn npc_stat_multiplier(dist: u16) -> f32 {
    (0.6 + 0.02 * dist as f32).min(1.3)
}

/// (class, ship count, stat multiplier) of the group at `dist`. The count
/// makes the group's real power land on `npc_power(dist)`.
pub fn npc_composition(dist: u16) -> (ShipClass, u8, f32) {
    let class = npc_class(dist);
    let m = npc_stat_multiplier(dist);
    let count = (npc_power(dist) / (ship_power(class) * m)).round();
    (class, count.clamp(1.0, NPC_MAX_SHIPS as f32) as u8, m)
}

/// Chance in percent that a sector at `dist` holds a group.
pub fn npc_presence_pct(dist: u16) -> f32 {
    (25.0 + 2.5 * dist as f32).min(85.0)
}

/// Share (0 to 1) of groups that stay passive.
pub fn npc_passive_share(dist: u16) -> f32 {
    (0.5 - 0.08 * (dist as f32 - 8.0)).clamp(0.0, 0.5)
}

/// Game hours before a cleared group returns.
pub fn npc_respawn_hours(dist: u16) -> f32 {
    2.0 + 0.25 * dist as f32
}

/// T1 to T9: one step per doubling of `power` over 4.
pub fn threat_rating(power: f32) -> u8 {
    if power.is_nan() || power <= NPC_POWER_BASE {
        return 1;
    }
    (1.0 + (power / NPC_POWER_BASE).log2().floor()).clamp(1.0, MAX_THREAT as f32) as u8
}

/// Coarse band of a rating: 1 for T1 to T3, 2 for T4 to T6, 3 for T7 to T9.
/// Derelict tiers follow it.
pub fn threat_band(rating: u8) -> u8 {
    rating.clamp(1, MAX_THREAT).div_ceil(3)
}

/// Ratio at and above which a fight is SAFE: the winner keeps 98 percent of its hull.
pub const RATIO_SAFE: f32 = 2.5;
/// FAVOURABLE from here: a sure win that costs about 5 to 15 percent of the hull.
pub const RATIO_FAVOURABLE: f32 = 1.5;
/// EVEN from here: a win in practice, but 15 to 60 percent of the hull is lost.
pub const RATIO_EVEN: f32 = 1.1;
/// RISKY from here: win odds swing from nil to near-certain across this band.
/// Below it (DEADLY) like-for-like fleets lose every time.
pub const RATIO_RISKY: f32 = 0.9;

/// How a fleet's power compares with what waits in a sector. Damage
/// exchange is quadratic in fleet size, so the bands sit close together
/// around 1.0 and a fight is decided long before the ratio reaches 2.
/// The cutoffs come from simulated fights (`combat::tests`) between like
/// ship classes; a lighter class facing a heavier one (rapid fire, bigger
/// hulls) needs about 1.5 times the ratio for the same result.
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
