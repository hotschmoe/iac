import 'hex.dart';

enum FleetStatus {
  docked('DOCKED'),
  enRoute('EN ROUTE'),
  harvesting('HARVESTING'),
  combat('COMBAT'),
  returning('RETURNING');

  final String label;
  const FleetStatus(this.label);
}

class ShipState {
  final String shipClass;
  final int count;
  final int hull;
  final int hullMax;

  const ShipState({
    required this.shipClass,
    required this.count,
    required this.hull,
    required this.hullMax,
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

class FleetState {
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

  const FleetState({
    required this.name,
    required this.sector,
    required this.status,
    required this.shipCount,
    this.cargoPercent = 0,
    this.fuelPercent = 100,
    this.ships = const [],
    this.cargo = const FleetCargo(capacity: 280),
    this.fuel = 500,
    this.fuelMax = 500,
  });

  FleetState copyWith({
    Hex? sector,
    FleetStatus? status,
    int? cargoPercent,
    int? fuelPercent,
  }) =>
      FleetState(
        name: name,
        sector: sector ?? this.sector,
        status: status ?? this.status,
        shipCount: shipCount,
        cargoPercent: cargoPercent ?? this.cargoPercent,
        fuelPercent: fuelPercent ?? this.fuelPercent,
        ships: ships,
        cargo: cargo,
        fuel: fuel,
        fuelMax: fuelMax,
      );
}
