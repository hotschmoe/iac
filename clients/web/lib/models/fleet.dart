import '../protocol/hex.dart';

enum FleetStatus {
  docked('DOCKED'),
  idle('IDLE'),
  enRoute('EN ROUTE'),
  harvesting('HARVESTING'),
  combat('COMBAT'),
  returning('RETURNING'),
  exploring('EXPLORING');

  final String label;
  const FleetStatus(this.label);
}

class ShipState {
  final String shipClass;
  final int count;
  final int hull;
  final int hullMax;

  /// Server ids of the ships in this group, for split commands.
  final List<int> ids;

  const ShipState({
    required this.shipClass,
    required this.count,
    required this.hull,
    required this.hullMax,
    this.ids = const [],
  });

  double get hullFraction => hullMax > 0 ? hull / hullMax : 0;
}

class FleetCargo {
  final int metal;
  final int crystal;
  final int deut;
  final int capacity;

  const FleetCargo({
    this.metal = 0,
    this.crystal = 0,
    this.deut = 0,
    required this.capacity,
  });

  int get total => metal + crystal + deut;
  double get fraction => capacity > 0 ? total / capacity : 0;
}

/// Presentation model of one of the player's fleets. [id] is the server's
/// fleet id (commands are addressed with it).
class FleetState {
  final int id;
  final String name;
  final Hex sector;
  final FleetStatus status;
  final int shipCount;
  final int cargoPercent;
  final int fuelPercent;
  final List<ShipState> ships;
  final FleetCargo cargo;
  final int fuel;
  final int fuelMax;

  /// Fuel per jump and fuel for the charted way home (0 at the homeworld).
  final int jumpFuel;
  final int homeFuel;
  final int cooldown;
  final double power;
  final int rangeHops;
  final String? policy;

  const FleetState({
    required this.id,
    required this.name,
    required this.sector,
    required this.status,
    required this.shipCount,
    this.cargoPercent = 0,
    this.fuelPercent = 100,
    this.ships = const [],
    this.cargo = const FleetCargo(capacity: 1),
    this.fuel = 0,
    this.fuelMax = 1,
    this.jumpFuel = 0,
    this.homeFuel = 0,
    this.cooldown = 0,
    this.power = 0,
    this.rangeHops = 0,
    this.policy,
  });

  /// Away from home with less than one jump of fuel.
  bool get stranded => homeFuel > 0 && jumpFuel > 0 && fuel < jumpFuel;

  /// Cannot afford the way home with the fuel aboard.
  bool get shortOfHomeFuel => homeFuel > fuel;
}
