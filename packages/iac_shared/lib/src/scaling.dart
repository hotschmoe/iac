/// Balance formulas, building/research types, and modifiers.
/// Port of src/shared/scaling.zig — pure data, no side effects.
library;

import 'constants.dart';

const int maxBuildingLevel = 20;
const double cancelRefundFraction = 0.50;

// ── Building System ───────────────────────────────────────────────

enum BuildingType {
  metalMine,
  crystalMine,
  deuteriumSynthesizer,
  shipyard,
  researchLab,
  fuelDepot,
  sensorArray,
  defenseGrid;

  String get label => switch (this) {
        BuildingType.metalMine => 'Metal Mine',
        BuildingType.crystalMine => 'Crystal Mine',
        BuildingType.deuteriumSynthesizer => 'Deut Synthesizer',
        BuildingType.shipyard => 'Shipyard',
        BuildingType.researchLab => 'Research Lab',
        BuildingType.fuelDepot => 'Fuel Depot',
        BuildingType.sensorArray => 'Sensor Array',
        BuildingType.defenseGrid => 'Defense Grid',
      };

  String get wireName => switch (this) {
        BuildingType.metalMine => 'metal_mine',
        BuildingType.crystalMine => 'crystal_mine',
        BuildingType.deuteriumSynthesizer => 'deuterium_synthesizer',
        BuildingType.shipyard => 'shipyard',
        BuildingType.researchLab => 'research_lab',
        BuildingType.fuelDepot => 'fuel_depot',
        BuildingType.sensorArray => 'sensor_array',
        BuildingType.defenseGrid => 'defense_grid',
      };

  static BuildingType fromWire(String? name, {BuildingType fallback = BuildingType.metalMine}) {
    if (name == null) return fallback;
    for (final b in BuildingType.values) {
      if (b.wireName == name || b.name == name) return b;
    }
    return fallback;
  }
}

class BuildingLevels {
  int metalMine;
  int crystalMine;
  int deuteriumSynthesizer;
  int shipyard;
  int researchLab;
  int fuelDepot;
  int sensorArray;
  int defenseGrid;

  BuildingLevels({
    this.metalMine = 1,
    this.crystalMine = 1,
    this.deuteriumSynthesizer = 0,
    this.shipyard = 0,
    this.researchLab = 0,
    this.fuelDepot = 0,
    this.sensorArray = 0,
    this.defenseGrid = 0,
  });

  int get(BuildingType building) => switch (building) {
        BuildingType.metalMine => metalMine,
        BuildingType.crystalMine => crystalMine,
        BuildingType.deuteriumSynthesizer => deuteriumSynthesizer,
        BuildingType.shipyard => shipyard,
        BuildingType.researchLab => researchLab,
        BuildingType.fuelDepot => fuelDepot,
        BuildingType.sensorArray => sensorArray,
        BuildingType.defenseGrid => defenseGrid,
      };

  void set(BuildingType building, int level) {
    switch (building) {
      case BuildingType.metalMine:
        metalMine = level;
      case BuildingType.crystalMine:
        crystalMine = level;
      case BuildingType.deuteriumSynthesizer:
        deuteriumSynthesizer = level;
      case BuildingType.shipyard:
        shipyard = level;
      case BuildingType.researchLab:
        researchLab = level;
      case BuildingType.fuelDepot:
        fuelDepot = level;
      case BuildingType.sensorArray:
        sensorArray = level;
      case BuildingType.defenseGrid:
        defenseGrid = level;
    }
  }

  BuildingLevels copy() => BuildingLevels(
        metalMine: metalMine,
        crystalMine: crystalMine,
        deuteriumSynthesizer: deuteriumSynthesizer,
        shipyard: shipyard,
        researchLab: researchLab,
        fuelDepot: fuelDepot,
        sensorArray: sensorArray,
        defenseGrid: defenseGrid,
      );
}

/// production_per_tick(level) = base_rate * level * 1.1^level
double productionPerTick(BuildingType building, int level) {
  if (level == 0) return 0;
  final base = switch (building) {
    BuildingType.metalMine => 0.5,
    BuildingType.crystalMine => 0.3,
    BuildingType.deuteriumSynthesizer => 0.15,
    _ => 0.0,
  };
  return base * level * _pow(1.1, level);
}

ResourceAmounts buildingCost(BuildingType building, int level) {
  final n = level.toDouble();
  return switch (building) {
    BuildingType.metalMine => ResourceAmounts(metal: n * 60, crystal: n * 15),
    BuildingType.crystalMine => ResourceAmounts(metal: n * 48, crystal: n * 24),
    BuildingType.deuteriumSynthesizer =>
      ResourceAmounts(metal: n * 225, crystal: n * 75),
    BuildingType.shipyard =>
      ResourceAmounts(metal: n * 200, crystal: n * 100, deuterium: n * 50),
    BuildingType.researchLab =>
      ResourceAmounts(metal: n * 100, crystal: n * 200, deuterium: n * 50),
    BuildingType.fuelDepot =>
      ResourceAmounts(metal: n * 150, crystal: n * 50, deuterium: n * 100),
    BuildingType.sensorArray =>
      ResourceAmounts(metal: n * 100, crystal: n * 150, deuterium: n * 75),
    BuildingType.defenseGrid =>
      ResourceAmounts(metal: n * 300, crystal: n * 200, deuterium: n * 100),
  };
}

/// build_ticks = base_ticks * level * 1.5^level
int buildingTime(BuildingType building, int level) {
  if (level == 0) return 0;
  final base = switch (building) {
    BuildingType.metalMine ||
    BuildingType.crystalMine ||
    BuildingType.deuteriumSynthesizer =>
      30,
    BuildingType.shipyard || BuildingType.researchLab => 60,
    BuildingType.fuelDepot => 40,
    BuildingType.sensorArray => 50,
    BuildingType.defenseGrid => 90,
  };
  final ticks = base * level * _pow(1.5, level);
  return ticks < 1 ? 1 : ticks.round();
}

class BuildingPrerequisite {
  final BuildingType building;
  final int level;
  const BuildingPrerequisite(this.building, this.level);
}

BuildingPrerequisite? buildingPrerequisites(BuildingType building) =>
    switch (building) {
      BuildingType.metalMine ||
      BuildingType.crystalMine ||
      BuildingType.deuteriumSynthesizer =>
        null,
      BuildingType.shipyard =>
        const BuildingPrerequisite(BuildingType.metalMine, 2),
      BuildingType.researchLab =>
        const BuildingPrerequisite(BuildingType.crystalMine, 2),
      BuildingType.fuelDepot =>
        const BuildingPrerequisite(BuildingType.deuteriumSynthesizer, 2),
      BuildingType.sensorArray =>
        const BuildingPrerequisite(BuildingType.researchLab, 1),
      BuildingType.defenseGrid =>
        const BuildingPrerequisite(BuildingType.shipyard, 3),
    };

bool buildingPrerequisitesMet(BuildingType building, BuildingLevels levels) {
  final prereq = buildingPrerequisites(building);
  if (prereq == null) return true;
  return levels.get(prereq.building) >= prereq.level;
}

// ── Research System ───────────────────────────────────────────────

enum ResearchType {
  fuelEfficiency,
  extendedFuelTanks,
  reinforcedHulls,
  advancedShields,
  weaponsResearch,
  navigation,
  harvestingEfficiency,
  corvetteTech,
  frigateTech,
  cruiserTech,
  haulerTech,
  emergencyJump;

  String get label => switch (this) {
        ResearchType.fuelEfficiency => 'Fuel Efficiency',
        ResearchType.extendedFuelTanks => 'Extended Tanks',
        ResearchType.reinforcedHulls => 'Reinforced Hulls',
        ResearchType.advancedShields => 'Advanced Shields',
        ResearchType.weaponsResearch => 'Weapons Research',
        ResearchType.navigation => 'Navigation',
        ResearchType.harvestingEfficiency => 'Harvesting Eff.',
        ResearchType.corvetteTech => 'Corvette Tech',
        ResearchType.frigateTech => 'Frigate Tech',
        ResearchType.cruiserTech => 'Cruiser Tech',
        ResearchType.haulerTech => 'Hauler Tech',
        ResearchType.emergencyJump => 'Emergency Jump',
      };

  String get wireName => switch (this) {
        ResearchType.fuelEfficiency => 'fuel_efficiency',
        ResearchType.extendedFuelTanks => 'extended_fuel_tanks',
        ResearchType.reinforcedHulls => 'reinforced_hulls',
        ResearchType.advancedShields => 'advanced_shields',
        ResearchType.weaponsResearch => 'weapons_research',
        ResearchType.navigation => 'navigation',
        ResearchType.harvestingEfficiency => 'harvesting_efficiency',
        ResearchType.corvetteTech => 'corvette_tech',
        ResearchType.frigateTech => 'frigate_tech',
        ResearchType.cruiserTech => 'cruiser_tech',
        ResearchType.haulerTech => 'hauler_tech',
        ResearchType.emergencyJump => 'emergency_jump',
      };

  static ResearchType fromWire(String? name, {ResearchType fallback = ResearchType.fuelEfficiency}) {
    if (name == null) return fallback;
    for (final t in ResearchType.values) {
      if (t.wireName == name || t.name == name) return t;
    }
    return fallback;
  }
}

class ResearchLevels {
  int fuelEfficiency;
  int extendedFuelTanks;
  int reinforcedHulls;
  int advancedShields;
  int weaponsResearch;
  int navigation;
  int harvestingEfficiency;
  int corvetteTech;
  int frigateTech;
  int cruiserTech;
  int haulerTech;
  int emergencyJump;

  ResearchLevels({
    this.fuelEfficiency = 0,
    this.extendedFuelTanks = 0,
    this.reinforcedHulls = 0,
    this.advancedShields = 0,
    this.weaponsResearch = 0,
    this.navigation = 0,
    this.harvestingEfficiency = 0,
    this.corvetteTech = 0,
    this.frigateTech = 0,
    this.cruiserTech = 0,
    this.haulerTech = 0,
    this.emergencyJump = 0,
  });

  int get(ResearchType tech) => switch (tech) {
        ResearchType.fuelEfficiency => fuelEfficiency,
        ResearchType.extendedFuelTanks => extendedFuelTanks,
        ResearchType.reinforcedHulls => reinforcedHulls,
        ResearchType.advancedShields => advancedShields,
        ResearchType.weaponsResearch => weaponsResearch,
        ResearchType.navigation => navigation,
        ResearchType.harvestingEfficiency => harvestingEfficiency,
        ResearchType.corvetteTech => corvetteTech,
        ResearchType.frigateTech => frigateTech,
        ResearchType.cruiserTech => cruiserTech,
        ResearchType.haulerTech => haulerTech,
        ResearchType.emergencyJump => emergencyJump,
      };

  void set(ResearchType tech, int level) {
    switch (tech) {
      case ResearchType.fuelEfficiency:
        fuelEfficiency = level;
      case ResearchType.extendedFuelTanks:
        extendedFuelTanks = level;
      case ResearchType.reinforcedHulls:
        reinforcedHulls = level;
      case ResearchType.advancedShields:
        advancedShields = level;
      case ResearchType.weaponsResearch:
        weaponsResearch = level;
      case ResearchType.navigation:
        navigation = level;
      case ResearchType.harvestingEfficiency:
        harvestingEfficiency = level;
      case ResearchType.corvetteTech:
        corvetteTech = level;
      case ResearchType.frigateTech:
        frigateTech = level;
      case ResearchType.cruiserTech:
        cruiserTech = level;
      case ResearchType.haulerTech:
        haulerTech = level;
      case ResearchType.emergencyJump:
        emergencyJump = level;
    }
  }

  ResearchLevels copy() => ResearchLevels(
        fuelEfficiency: fuelEfficiency,
        extendedFuelTanks: extendedFuelTanks,
        reinforcedHulls: reinforcedHulls,
        advancedShields: advancedShields,
        weaponsResearch: weaponsResearch,
        navigation: navigation,
        harvestingEfficiency: harvestingEfficiency,
        corvetteTech: corvetteTech,
        frigateTech: frigateTech,
        cruiserTech: cruiserTech,
        haulerTech: haulerTech,
        emergencyJump: emergencyJump,
      );
}

int researchMaxLevel(ResearchType tech) => switch (tech) {
      ResearchType.corvetteTech ||
      ResearchType.frigateTech ||
      ResearchType.cruiserTech ||
      ResearchType.haulerTech =>
        1,
      ResearchType.emergencyJump => 3,
      _ => 5,
    };

ResourceAmounts researchCost(ResearchType tech, int level) {
  final n = level.toDouble();
  return switch (tech) {
    ResearchType.corvetteTech =>
      const ResourceAmounts(metal: 400, crystal: 200, deuterium: 100),
    ResearchType.frigateTech =>
      const ResourceAmounts(metal: 1000, crystal: 600, deuterium: 300),
    ResearchType.cruiserTech =>
      const ResourceAmounts(metal: 3000, crystal: 2000, deuterium: 1000),
    ResearchType.haulerTech =>
      const ResourceAmounts(metal: 600, crystal: 300, deuterium: 200),
    ResearchType.fuelEfficiency =>
      ResourceAmounts(metal: n * 200, crystal: n * 100, deuterium: n * 100),
    ResearchType.extendedFuelTanks =>
      ResourceAmounts(metal: n * 150, crystal: n * 75, deuterium: n * 150),
    ResearchType.reinforcedHulls =>
      ResourceAmounts(metal: n * 300, crystal: n * 100),
    ResearchType.advancedShields =>
      ResourceAmounts(metal: n * 100, crystal: n * 300, deuterium: n * 50),
    ResearchType.weaponsResearch =>
      ResourceAmounts(metal: n * 200, crystal: n * 200, deuterium: n * 100),
    ResearchType.navigation =>
      ResourceAmounts(metal: n * 100, crystal: n * 150, deuterium: n * 50),
    ResearchType.harvestingEfficiency =>
      ResourceAmounts(metal: n * 150, crystal: n * 100, deuterium: n * 75),
    ResearchType.emergencyJump =>
      ResourceAmounts(metal: n * 500, crystal: n * 400, deuterium: n * 300),
  };
}

int researchTime(ResearchType tech, int level) {
  if (level == 0) return 0;
  return switch (tech) {
    ResearchType.corvetteTech => 60,
    ResearchType.frigateTech => 120,
    ResearchType.cruiserTech => 240,
    ResearchType.haulerTech => 90,
    _ => () {
        final ticks = 60 * level * _pow(1.5, level);
        return ticks < 1 ? 1 : ticks.round();
      }(),
  };
}

bool researchPrerequisitesMet(
  ResearchType tech,
  BuildingLevels buildings,
  ResearchLevels research,
) {
  switch (tech) {
    case ResearchType.fuelEfficiency:
      return buildings.deuteriumSynthesizer >= 3;
    case ResearchType.extendedFuelTanks:
      return buildings.fuelDepot >= 2;
    case ResearchType.reinforcedHulls:
      return buildings.shipyard >= 2;
    case ResearchType.advancedShields:
      return buildings.researchLab >= 3;
    case ResearchType.weaponsResearch:
      return buildings.researchLab >= 3;
    case ResearchType.navigation:
      return buildings.researchLab >= 2;
    case ResearchType.harvestingEfficiency:
      return buildings.researchLab >= 2;
    case ResearchType.corvetteTech:
      return buildings.shipyard >= 2;
    case ResearchType.frigateTech:
      return research.corvetteTech >= 1 && buildings.shipyard >= 4;
    case ResearchType.cruiserTech:
      return research.frigateTech >= 1 && buildings.shipyard >= 6;
    case ResearchType.haulerTech:
      return buildings.shipyard >= 3;
    case ResearchType.emergencyJump:
      return research.navigation >= 1 && buildings.researchLab >= 4;
  }
}

// ── Research modifiers ────────────────────────────────────────────

ShipStats applyResearchToStats(ShipStats base, ResearchLevels research) {
  final hullBonus = 1.0 + 0.10 * research.reinforcedHulls;
  final shieldBonus = 1.0 + 0.10 * research.advancedShields;
  final weaponBonus = 1.0 + 0.10 * research.weaponsResearch;
  final fuelCap = fuelCapacityModifier(research.extendedFuelTanks);
  return ShipStats(
    hull: base.hull * hullBonus,
    shield: base.shield * shieldBonus,
    weapon: base.weapon * weaponBonus,
    speed: base.speed,
    cargo: base.cargo,
    fuel: (base.fuel * fuelCap).round(),
  );
}

double fuelRateModifier(int fuelEffLevel) => 1.0 - 0.10 * fuelEffLevel;

double fuelCapacityModifier(int tanksLevel) => 1.0 + 0.15 * tanksLevel;

double fuelDepotModifier(int depotLevel) => 1.0 + 0.10 * depotLevel;

int sensorRange(int sensorLevel) => sensorLevel;

int navigationCooldownReduction(int navLevel) => navLevel;

double harvestRateModifier(int harvestEffLevel) => 1.0 + 0.20 * harvestEffLevel;

double recallDamageReduction(int ejLevel) => 0.05 * ejLevel;

bool shipClassUnlocked(ShipClass class_, ResearchLevels research) =>
    switch (class_) {
      ShipClass.scout => true,
      ShipClass.corvette => research.corvetteTech >= 1,
      ShipClass.frigate => research.frigateTech >= 1,
      ShipClass.cruiser => research.cruiserTech >= 1,
      ShipClass.hauler => research.haulerTech >= 1,
    };

int shipBuildTime(ShipClass class_, int shipyardLevel) {
  final base = switch (class_) {
    ShipClass.scout => 30.0,
    ShipClass.corvette => 60.0,
    ShipClass.frigate => 120.0,
    ShipClass.cruiser => 240.0,
    ShipClass.hauler => 90.0,
  };
  final divisor = 1.0 + 0.1 * shipyardLevel;
  final ticks = base / divisor;
  return ticks < 1 ? 1 : ticks.round();
}

enum QueueType {
  building,
  ship,
  research;

  String get wireName => name;

  static QueueType fromWire(String? name, {QueueType fallback = QueueType.building}) {
    if (name == null) return fallback;
    for (final q in QueueType.values) {
      if (q.name == name) return q;
    }
    return fallback;
  }
}

double _pow(double base, int exp) {
  var result = 1.0;
  for (var i = 0; i < exp; i++) {
    result *= base;
  }
  return result;
}
