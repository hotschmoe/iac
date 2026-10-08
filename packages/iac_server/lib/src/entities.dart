/// Mutable server-side game entities.
library;

import 'package:iac_shared/iac_shared.dart';

const int maxShipsPerFleet = 64;
const int maxNpcShips = 32;
const int maxCombatFleets = 8;

/// Greek-letter fleet names for UI.
const List<String> fleetNames = [
  'Alpha',
  'Beta',
  'Gamma',
  'Delta',
  'Epsilon',
  'Zeta',
  'Eta',
  'Theta',
];

class BuildingQueueItem {
  BuildingType building;
  int targetLevel;
  int startTick;
  int endTick;

  BuildingQueueItem({
    required this.building,
    required this.targetLevel,
    required this.startTick,
    required this.endTick,
  });
}

class ShipQueueItem {
  ShipClass shipClass;
  int count;
  int built;
  int startTick;
  int endTick;

  ShipQueueItem({
    required this.shipClass,
    required this.count,
    this.built = 0,
    required this.startTick,
    required this.endTick,
  });
}

class ResearchQueueItem {
  ResearchType tech;
  int targetLevel;
  int startTick;
  int endTick;

  ResearchQueueItem({
    required this.tech,
    required this.targetLevel,
    required this.startTick,
    required this.endTick,
  });
}

class Player {
  final int id;
  final String name;
  ResourceAmounts resources;
  Hex homeworld;
  BuildingLevels buildings;
  ResearchLevels research;
  BuildingQueueItem? buildingQueue;
  ShipQueueItem? shipQueue;
  ResearchQueueItem? researchQueue;
  int fleetNameIndex = 0;

  Player({
    required this.id,
    required this.name,
    required this.resources,
    required this.homeworld,
    BuildingLevels? buildings,
    ResearchLevels? research,
    this.buildingQueue,
    this.shipQueue,
    this.researchQueue,
  })  : buildings = buildings ?? BuildingLevels(),
        research = research ?? ResearchLevels();

  String nextFleetName() {
    final name = fleetNames[fleetNameIndex % fleetNames.length];
    fleetNameIndex++;
    return name;
  }
}

class Ship {
  final int id;
  ShipClass class_;
  double hull;
  double hullMax;
  double shield;
  double shieldMax;
  double weaponPower;
  int speed;

  Ship({
    required this.id,
    required this.class_,
    required this.hull,
    required this.hullMax,
    required this.shield,
    required this.shieldMax,
    required this.weaponPower,
    required this.speed,
  });

  factory Ship.fromStats(int id, ShipClass class_, ShipStats stats) => Ship(
        id: id,
        class_: class_,
        hull: stats.hull,
        hullMax: stats.hull,
        shield: stats.shield,
        shieldMax: stats.shield,
        weaponPower: stats.weapon,
        speed: stats.speed,
      );

  ShipState toState() => ShipState(
        id: id,
        class_: class_,
        hull: hull,
        hullMax: hullMax,
        shield: shield,
        shieldMax: shieldMax,
        weaponPower: weaponPower,
      );
}

class Fleet {
  final int id;
  final int ownerId;
  String name;
  Hex location;
  FleetStatus status;
  final List<Ship> ships;
  ResourceAmounts cargo;
  double fuel;
  double fuelMax;
  int moveCooldown;
  int actionCooldown;
  Hex? moveTarget;
  int idleTicks;

  Fleet({
    required this.id,
    required this.ownerId,
    required this.name,
    required this.location,
    this.status = FleetStatus.idle,
    List<Ship>? ships,
    ResourceAmounts? cargo,
    this.fuel = 0,
    this.fuelMax = 0,
    this.moveCooldown = 0,
    this.actionCooldown = 0,
    this.moveTarget,
    this.idleTicks = 0,
  })  : ships = ships ?? <Ship>[],
        cargo = cargo ?? ResourceAmounts.zero;

  int get shipCount => ships.length;

  FleetState toState() => FleetState(
        id: id,
        location: location,
        state: status,
        ships: ships.map((s) => s.toState()).toList(),
        cargo: cargo,
        fuel: fuel,
        fuelMax: fuelMax,
        cooldownRemaining: actionCooldown,
        name: name,
      );
}

class NpcFleet {
  final int id;
  Hex location;
  final List<Ship> ships;
  NpcBehavior behavior;
  Hex homeSector;
  int patrolTimer;
  bool inCombat;

  NpcFleet({
    required this.id,
    required this.location,
    List<Ship>? ships,
    required this.behavior,
    Hex? homeSector,
    this.patrolTimer = 0,
    this.inCombat = false,
  })  : ships = ships ?? <Ship>[],
        homeSector = homeSector ?? location;

  int get shipCount => ships.length;
}

class Combat {
  final int id;
  Hex sector;
  final List<int> playerFleetIds = [];
  final List<int> npcFleetIds = [];
  ResourceAmounts npcValue;
  int round;

  Combat({
    required this.id,
    required this.sector,
    ResourceAmounts? npcValue,
    this.round = 0,
  }) : npcValue = npcValue ?? ResourceAmounts.zero;

  void addPlayerFleet(int fleetId) {
    if (playerFleetIds.contains(fleetId)) return;
    if (playerFleetIds.length >= maxCombatFleets) return;
    playerFleetIds.add(fleetId);
  }

  void addNpcFleet(int fleetId) {
    if (npcFleetIds.contains(fleetId)) return;
    if (npcFleetIds.length >= maxCombatFleets) return;
    npcFleetIds.add(fleetId);
  }

  bool hasPlayerFleet(int fleetId) => playerFleetIds.contains(fleetId);
  bool hasNpcFleet(int fleetId) => npcFleetIds.contains(fleetId);
}

class SectorOverride {
  Density? metalDensity;
  Density? crystalDensity;
  Density? deutDensity;
  double metalHarvested = 0;
  double crystalHarvested = 0;
  double deutHarvested = 0;
  ResourceAmounts? salvage;
  int? salvageDespawnTick;
  int? npcClearedTick;

  SectorOverride();

  static ({Density metal, Density crystal, Density deut}) effectiveDensities(
    SectorOverride? override,
    SectorTemplate template,
  ) {
    if (override == null) {
      return (
        metal: template.metalDensity,
        crystal: template.crystalDensity,
        deut: template.deutDensity,
      );
    }
    return (
      metal: override.metalDensity ?? template.metalDensity,
      crystal: override.crystalDensity ?? template.crystalDensity,
      deut: override.deutDensity ?? template.deutDensity,
    );
  }
}
