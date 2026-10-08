// Single source of truth for all balance formulas, costs, times, and modifiers.
// Pure data/formula file -- no side effects, no allocations.

use serde::{Deserialize, Serialize};

use crate::constants::{Resources, ShipClass, ShipStats};

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
        }
    }

    /// Number of building types.
    pub const COUNT: usize = 8;

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
        }
    }
}

/// production_per_tick(level) = base_rate * level * 1.1^level
pub fn production_per_tick(building: BuildingType, level: u8) -> f32 {
    if level == 0 {
        return 0.0;
    }
    let base: f32 = match building {
        BuildingType::MetalMine => 0.5,
        BuildingType::CrystalMine => 0.3,
        BuildingType::DeuteriumSynthesizer => 0.15,
        _ => 0.0,
    };
    base * level as f32 * pow_f32(1.1, level)
}

/// Cost to build level N.
pub fn building_cost(building: BuildingType, level: u8) -> Resources {
    let n = level as f32;
    match building {
        BuildingType::MetalMine => Resources {
            metal: n * 60.0,
            crystal: n * 15.0,
            deuterium: 0.0,
        },
        BuildingType::CrystalMine => Resources {
            metal: n * 48.0,
            crystal: n * 24.0,
            deuterium: 0.0,
        },
        BuildingType::DeuteriumSynthesizer => Resources {
            metal: n * 225.0,
            crystal: n * 75.0,
            deuterium: 0.0,
        },
        BuildingType::Shipyard => Resources {
            metal: n * 200.0,
            crystal: n * 100.0,
            deuterium: n * 50.0,
        },
        BuildingType::ResearchLab => Resources {
            metal: n * 100.0,
            crystal: n * 200.0,
            deuterium: n * 50.0,
        },
        BuildingType::FuelDepot => Resources {
            metal: n * 150.0,
            crystal: n * 50.0,
            deuterium: n * 100.0,
        },
        BuildingType::SensorArray => Resources {
            metal: n * 100.0,
            crystal: n * 150.0,
            deuterium: n * 75.0,
        },
        BuildingType::DefenseGrid => Resources {
            metal: n * 300.0,
            crystal: n * 200.0,
            deuterium: n * 100.0,
        },
    }
}

/// build_ticks = base_ticks * level * 1.5^level
pub fn building_time(building: BuildingType, level: u8) -> u64 {
    if level == 0 {
        return 0;
    }
    let base: u64 = match building {
        BuildingType::MetalMine
        | BuildingType::CrystalMine
        | BuildingType::DeuteriumSynthesizer => 30,
        BuildingType::Shipyard | BuildingType::ResearchLab => 60,
        BuildingType::FuelDepot => 40,
        BuildingType::SensorArray => 50,
        BuildingType::DefenseGrid => 90,
    };
    let ticks = base as f32 * level as f32 * pow_f32(1.5, level);
    (ticks.max(1.0)) as u64
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

pub fn research_cost(tech: ResearchType, level: u8) -> Resources {
    let n = level as f32;
    match tech {
        ResearchType::CorvetteTech => Resources {
            metal: 400.0,
            crystal: 200.0,
            deuterium: 100.0,
        },
        ResearchType::FrigateTech => Resources {
            metal: 1000.0,
            crystal: 600.0,
            deuterium: 300.0,
        },
        ResearchType::CruiserTech => Resources {
            metal: 3000.0,
            crystal: 2000.0,
            deuterium: 1000.0,
        },
        ResearchType::HaulerTech => Resources {
            metal: 600.0,
            crystal: 300.0,
            deuterium: 200.0,
        },
        ResearchType::FuelEfficiency => Resources {
            metal: n * 200.0,
            crystal: n * 100.0,
            deuterium: n * 100.0,
        },
        ResearchType::ExtendedFuelTanks => Resources {
            metal: n * 150.0,
            crystal: n * 75.0,
            deuterium: n * 150.0,
        },
        ResearchType::ReinforcedHulls => Resources {
            metal: n * 300.0,
            crystal: n * 100.0,
            deuterium: 0.0,
        },
        ResearchType::AdvancedShields => Resources {
            metal: n * 100.0,
            crystal: n * 300.0,
            deuterium: n * 50.0,
        },
        ResearchType::WeaponsResearch => Resources {
            metal: n * 200.0,
            crystal: n * 200.0,
            deuterium: n * 100.0,
        },
        ResearchType::Navigation => Resources {
            metal: n * 100.0,
            crystal: n * 150.0,
            deuterium: n * 50.0,
        },
        ResearchType::HarvestingEfficiency => Resources {
            metal: n * 150.0,
            crystal: n * 100.0,
            deuterium: n * 75.0,
        },
        ResearchType::EmergencyJump => Resources {
            metal: n * 500.0,
            crystal: n * 400.0,
            deuterium: n * 300.0,
        },
    }
}

pub fn research_time(tech: ResearchType, level: u8) -> u64 {
    if level == 0 {
        return 0;
    }
    match tech {
        ResearchType::CorvetteTech => 60,
        ResearchType::FrigateTech => 120,
        ResearchType::CruiserTech => 240,
        ResearchType::HaulerTech => 90,
        _ => {
            let base: u64 = 60;
            let ticks = base as f32 * level as f32 * pow_f32(1.5, level);
            (ticks.max(1.0)) as u64
        }
    }
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
                level: 2,
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

pub fn ship_class_unlocked(class: ShipClass, research: &ResearchLevels) -> bool {
    match class {
        ShipClass::Scout => true,
        ShipClass::Corvette => research.corvette_tech >= 1,
        ShipClass::Frigate => research.frigate_tech >= 1,
        ShipClass::Cruiser => research.cruiser_tech >= 1,
        ShipClass::Hauler => research.hauler_tech >= 1,
    }
}

pub fn ship_build_time(class: ShipClass, shipyard_level: u8) -> u64 {
    let base: f32 = match class {
        ShipClass::Scout => 30.0,
        ShipClass::Corvette => 60.0,
        ShipClass::Frigate => 120.0,
        ShipClass::Cruiser => 240.0,
        ShipClass::Hauler => 90.0,
    };
    let divisor = 1.0 + 0.1 * shipyard_level as f32;
    (base / divisor).max(1.0) as u64
}

pub const CANCEL_REFUND_FRACTION: f32 = 0.50;

// ── Utility ─────────────────────────────────────────────────────

fn pow_f32(base: f32, exp: u8) -> f32 {
    base.powi(exp as i32)
}
