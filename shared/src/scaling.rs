// Single source of truth for all balance formulas, costs, times, and modifiers.
// Pure data/formula file -- no side effects, no allocations.

use serde::{Deserialize, Serialize};

use crate::constants::{ResourceKind, Resources, ShipClass, ShipStats};
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
        }
    }

    /// Number of building types.
    pub const COUNT: usize = 9;

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
            _ => None,
        }
    }
}


#[derive(Debug, Clone)]
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
pub fn building_time(building: BuildingType, level: u8, pace: &Pace) -> u64 {
    if level == 0 {
        return 0;
    }
    pace.econ_ticks(cost_seconds(building_cost(building, level), BUILD_RATE, 1.0))
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
}

impl ResearchType {
    pub const ALL: [ResearchType; 12] = [
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
        }
    }

    /// Number of research types.
    pub const COUNT: usize = 12;

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
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
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
    }
}

/// Per-level cost growth of the leveled techs.
pub const RESEARCH_GROWTH: f64 = 1.6;

pub fn research_cost(tech: ResearchType, level: u8) -> Resources {
    let (base, growth) = research_cost_curve(tech);
    grown_cost(base, growth, level)
}

pub fn research_time(tech: ResearchType, level: u8, pace: &Pace) -> u64 {
    if level == 0 {
        return 0;
    }
    pace.econ_ticks(cost_seconds(research_cost(tech, level), RESEARCH_RATE, 1.0))
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

pub fn fuel_rate_modifier(fuel_eff_level: u8) -> f32 {
    1.0 - 0.10 * fuel_eff_level as f32
}

pub fn fuel_capacity_modifier(tanks_level: u8) -> f32 {
    1.0 + 0.15 * tanks_level as f32
}

pub fn fuel_depot_modifier(depot_level: u8) -> f32 {
    1.0 + 0.10 * depot_level as f32
}

pub fn sensor_range(sensor_level: u8) -> u8 {
    sensor_level
}

pub fn navigation_cooldown_reduction(nav_level: u8) -> u16 {
    nav_level as u16
}

pub fn harvest_rate_modifier(harvest_eff_level: u8) -> f32 {
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

#[cfg(test)]
mod golden_tests {
    //! Values copied from the generated tables in docs/design/economy/spec.md.
    use super::*;
    use BuildingType::*;

    const P1: Pace = Pace::PERSISTENT;

    fn cost(m: f32, c: f32, d: f32) -> Resources {
        Resources { metal: m, crystal: c, deuterium: d }
    }

    fn per_hour(b: BuildingType, level: u8) -> f32 {
        production_per_tick(b, level, &P1) * 3600.0
    }

    #[test]
    fn production_per_hour_matches_the_table() {
        for (level, metal) in [(1, 11.0), (5, 81.0), (10, 259.0), (15, 627.0), (20, 1345.0)] {
            assert!((per_hour(MetalMine, level) - metal).abs() < 0.6, "metal L{level}");
        }
        assert!((per_hour(CrystalMine, 10) - 182.0).abs() < 0.6);
        assert!((per_hour(CrystalMine, 20) - 942.0).abs() < 0.6);
        assert!((per_hour(DeuteriumSynthesizer, 2) - 12.0).abs() < 0.6);
        assert!((per_hour(DeuteriumSynthesizer, 20) - 673.0).abs() < 0.6);
        assert_eq!(per_hour(Shipyard, 5), 0.0);
        assert_eq!(per_hour(MetalMine, 0), 0.0);
    }

    #[test]
    fn production_scales_linearly_with_pace() {
        let p = Pace::new(600.0).unwrap();
        let base = production_per_tick(MetalMine, 7, &P1);
        assert!((production_per_tick(MetalMine, 7, &p) / base - 600.0).abs() < 1e-3);
    }

    #[test]
    fn building_costs_match_the_table() {
        let cases = [
            (MetalMine, 1, cost(60.0, 15.0, 0.0)),
            (MetalMine, 2, cost(90.0, 22.0, 0.0)),
            (MetalMine, 3, cost(135.0, 34.0, 0.0)),
            (MetalMine, 5, cost(304.0, 76.0, 0.0)),
            (MetalMine, 8, cost(1025.0, 256.0, 0.0)),
            (MetalMine, 10, cost(2307.0, 577.0, 0.0)),
            (MetalMine, 12, cost(5190.0, 1297.0, 0.0)),
            (MetalMine, 15, cost(17516.0, 4379.0, 0.0)),
            (MetalMine, 18, cost(59116.0, 14779.0, 0.0)),
            (MetalMine, 20, cost(133010.0, 33253.0, 0.0)),
            (Shipyard, 1, cost(200.0, 100.0, 50.0)),
            (Shipyard, 2, cost(360.0, 180.0, 90.0)),
            (Shipyard, 3, cost(648.0, 324.0, 162.0)),
            (Shipyard, 5, cost(2100.0, 1050.0, 525.0)),
            (Shipyard, 8, cost(12244.0, 6122.0, 3061.0)),
            (Shipyard, 10, cost(39672.0, 19836.0, 9918.0)),
            (DeuteriumSynthesizer, 1, cost(150.0, 50.0, 0.0)),
            (ResearchLab, 8, cost(6122.0, 12244.0, 3061.0)),
            (Shipyard, 4, cost(1166.0, 583.0, 292.0)),
            (Shipyard, 6, cost(3779.0, 1890.0, 945.0)),
        ];
        for (b, level, want) in cases {
            assert_eq!(building_cost(b, level), want, "{b:?} L{level}");
        }
    }

    fn hours(ticks: u64) -> f64 {
        ticks as f64 / 3600.0
    }

    #[test]
    fn building_times_match_the_table() {
        assert_eq!(building_time(MetalMine, 2, &P1), 576, "(90 + 22) / 700 h");
        assert!((hours(building_time(MetalMine, 1, &P1)) * 60.0 - 6.4).abs() < 0.1);
        assert!((hours(building_time(MetalMine, 5, &P1)) * 60.0 - 33.0).abs() < 1.0);
        assert!((hours(building_time(MetalMine, 10, &P1)) - 4.1).abs() < 0.1);
        assert!((hours(building_time(MetalMine, 15, &P1)) - 31.3).abs() < 0.1);
        assert!((hours(building_time(Shipyard, 1, &P1)) * 60.0 - 26.0).abs() < 1.0);
        assert!((hours(building_time(Shipyard, 5, &P1)) - 4.5).abs() < 0.05);
        assert!((hours(building_time(Shipyard, 8, &P1)) - 26.2).abs() < 0.1);
        assert!((hours(building_time(ResearchLab, 8, &P1)) - 26.2).abs() < 0.1);
        assert_eq!(building_time(MetalMine, 0, &P1), 0);
    }

    #[test]
    fn building_times_by_pace_match_the_table() {
        let season = Pace::new(10.0).unwrap();
        let blitz = Pace::new(600.0).unwrap();
        let test = Pace::new(8000.0).unwrap();
        assert_eq!(building_time(MetalMine, 2, &season), 58, "58s");
        assert_eq!(building_time(MetalMine, 2, &blitz), 1);
        assert_eq!(building_time(MetalMine, 2, &test), 1);
        assert_eq!(building_time(MetalMine, 10, &blitz), 25, "25s");
        assert_eq!(building_time(Shipyard, 4, &blitz), 15, "15s");
        assert_eq!(building_time(MetalMine, 15, &test), 15, "14s in the table, which rounds");
    }

    #[test]
    fn research_costs_match_the_table() {
        assert_eq!(research_cost(ResearchType::CorvetteTech, 1), cost(400.0, 200.0, 100.0));
        assert_eq!(research_cost(ResearchType::HaulerTech, 1), cost(600.0, 300.0, 200.0));
        assert_eq!(research_cost(ResearchType::FrigateTech, 1), cost(2000.0, 1200.0, 600.0));
        assert_eq!(research_cost(ResearchType::CruiserTech, 1), cost(8000.0, 5000.0, 2500.0));
        assert_eq!(research_cost(ResearchType::FuelEfficiency, 1), cost(200.0, 100.0, 100.0));
        assert_eq!(research_cost(ResearchType::FuelEfficiency, 3), cost(512.0, 256.0, 256.0));
        assert_eq!(research_cost(ResearchType::ReinforcedHulls, 2), cost(480.0, 160.0, 0.0));
    }

    #[test]
    fn research_times_match_the_table() {
        let t = |tech, pace: &Pace| research_time(tech, 1, pace);
        assert_eq!(t(ResearchType::CorvetteTech, &P1), 4320, "72 minutes");
        assert!((hours(t(ResearchType::FrigateTech, &P1)) - 6.4).abs() < 0.01);
        assert!((hours(t(ResearchType::CruiserTech, &P1)) - 26.0).abs() < 0.01);
        let blitz = Pace::new(600.0).unwrap();
        assert_eq!(t(ResearchType::CorvetteTech, &blitz), 8, "about 7s");
        assert_eq!(t(ResearchType::CruiserTech, &blitz), 156, "about 3m");
    }

    #[test]
    fn ship_times_match_the_table() {
        assert_eq!(ship_build_time(ShipClass::Corvette, 0, &P1), 1500, "25 minutes");
        assert!((hours(ship_build_time(ShipClass::Cruiser, 0, &P1)) - 3.75).abs() < 0.01);
        assert_eq!(ship_build_time(ShipClass::Corvette, 5, &P1), 1000, "shipyard 5 is 1.5x faster");
        let blitz = Pace::new(600.0).unwrap();
        assert_eq!(ship_build_time(ShipClass::Cruiser, 0, &blitz), 23, "22s in the table");
        assert_eq!(ship_build_time(ShipClass::Scout, 0, &Pace::new(8000.0).unwrap()), 1);
    }

    #[test]
    fn corvettes_unlock_at_shipyard_one() {
        let mut b = BuildingLevels::default();
        let r = ResearchLevels::default();
        assert!(!research_prerequisites_met(ResearchType::CorvetteTech, &b, &r));
        b.set(Shipyard, 1);
        assert!(research_prerequisites_met(ResearchType::CorvetteTech, &b, &r));
    }

    #[test]
    fn cost_and_time_share_an_exponent() {
        let c = |l| {
            let k = building_cost(MetalMine, l);
            (k.metal + k.crystal) as f64
        };
        let ratio_cost = c(20) / c(10);
        let ratio_time = building_time(MetalMine, 20, &P1) as f64 / building_time(MetalMine, 10, &P1) as f64;
        assert!((ratio_cost / ratio_time - 1.0).abs() < 0.01);
    }

    #[test]
    fn storage_caps_match_the_table() {
        let cap = |level, pace: f64| storage_cap(level, &Pace::new(pace).unwrap());
        let near = |got: Resources, m: f32, c: f32, d: f32| {
            assert!((got.metal - m).abs() < 1.5 && (got.crystal - c).abs() < 1.5 && (got.deuterium - d).abs() < 1.5, "{got:?}");
        };
        near(cap(0, 1.0), 5000.0, 3500.0, 2500.0);
        near(cap(1, 1.0), 7500.0, 5250.0, 3750.0);
        near(cap(5, 1.0), 37968.0, 26578.0, 18984.0);
        near(cap(10, 1.0), 288325.0, 201827.0, 144162.0);
        near(cap(0, 10.0), 28117.0, 19681.0, 14058.0);
        near(cap(3, 10.0), 94895.0, 66426.0, 47447.0);
        near(cap(0, 600.0), 606154.0, 424308.0, 303077.0);
        near(cap(14, 600.0), 176954278.0, 123867995.0, 88477139.0);
    }

    #[test]
    fn vault_costs_match_the_table() {
        assert_eq!(building_cost(StorageVault, 1), cost(200.0, 100.0, 0.0));
        assert_eq!(building_cost(StorageVault, 2), cost(340.0, 170.0, 0.0));
        assert_eq!(building_cost(StorageVault, 3), cost(578.0, 289.0, 0.0));
        assert_eq!(building_cost(StorageVault, 6), cost(2840.0, 1420.0, 0.0), "the table truncates 2839.7");
        assert_eq!(building_cost(StorageVault, 10), cost(23718.0, 11859.0, 0.0));
    }

    #[test]
    fn raids_cannot_skim_the_protected_share() {
        let p = storage_protected(5, &P1);
        assert!((p.metal - 0.4 * 37968.75).abs() < 1.0);
        assert_eq!(storage_protected(0, &P1).metal, 0.0);
        let capped = storage_protected(12, &P1);
        assert!((capped.metal / storage_cap(12, &P1).metal - 0.6).abs() < 1e-5);
    }

    #[test]
    fn the_vault_level_a_payment_needs() {
        assert_eq!(vault_level_for(5000.0, ResourceKind::Metal, &P1), Some(0));
        assert_eq!(vault_level_for(8000.0, ResourceKind::Metal, &P1), Some(2));
        assert_eq!(vault_level_for(3000.0, ResourceKind::Deuterium, &P1), Some(1));
        assert_eq!(vault_level_for(1e12, ResourceKind::Metal, &P1), None);
    }
}
