import '../protocol/hex.dart';
import '../protocol/protocol.dart' as proto
    show Fleeing, FlightQuote, Pursuit, Refining, RefuelQuote, Refit, RelayQuote, RepairQuote, ShipGroupState, StandingOrders, scanBaseRange;

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

  /// Every ship of every class, as the server counts them.
  final int shipCount;
  final int cargoPercent;
  final int fuelPercent;

  /// The fleet by class, as the server sends it: a count, how many are
  /// damaged and at risk, and the hull they hold.
  final List<proto.ShipGroupState> ships;

  /// Hull the whole fleet holds, and at full.
  final int hull;
  final int hullMax;

  /// What the fleet fires in a round and the shield it raises at the start
  /// of each.
  final int attack;
  final int shield;

  /// The Weapons, Shielding and Armour levels it fights with, and whether
  /// docking at home would raise them.
  final proto.Refit refit;
  final bool refitDue;

  /// What mending its damaged ships would take at each dock; null when
  /// none is damaged.
  final proto.RepairQuote? repair;
  final FleetCargo cargo;
  final int fuel;
  final int fuelMax;

  /// Fuel per jump, and fuel for the fewest hops home over the real lanes,
  /// charted or not (0 at the homeworld).
  final int jumpFuel;
  final int homeFuel;

  /// Fewest hops home over the real lanes, charted or not. [homeFuel] is the
  /// fuel for these. 0 at the homeworld.
  final int hopsHome;

  /// The server's fuel and jump cost before display rounding.
  final double? fuelExact;
  final double? jumpFuelExact;

  /// Ticks one jump takes this fleet, from the order to the arrival.
  final int jumpTicks;

  /// Hops a scan by this fleet reveals, as the server states it.
  final int scanRange;

  /// Fuel for the way to the player's outpost, 0 standing at it; null
  /// unless that outpost is in service.
  final int? outpostFuel;

  /// In the sector of the player's outpost, whatever state it is in.
  final bool atOutpost;

  /// What loading and unloading deuterium would do now, while the fleet is
  /// in the sector of one of the player's relays.
  final proto.RelayQuote? relay;
  bool get atRelay => relay != null;
  final int cooldown;
  final double power;
  final int rangeHops;

  /// The standing orders the fleet flies under, with the settings in force;
  /// null when it is flown by hand.
  final proto.StandingOrders? orders;

  /// Loaded by hand at the home dock, which leaves this hold alone until the
  /// fleet has been away.
  final bool laden;

  /// At home with cargo the stockpile has no room for yet.
  final bool cargoBlocked;

  /// Exact hold size as the server enforces it ([cargo] is rounded for display).
  final double holdCapacity;

  /// What a refine order would turn into what right now; null while the
  /// server would refuse it.
  final proto.Refining? refine;

  /// What a refuel would buy with each resource in the hold; non-null only
  /// while the fleet stands at a hub that refuels.
  final proto.RefuelQuote? refuel;

  /// What a jump out of the fight it is in would cost now, and who would
  /// follow; null unless it stands in a fight and fires.
  final proto.FlightQuote? flight;

  /// The jump out of a fight it is charging; null otherwise. Its [status]
  /// is [FleetStatus.enRoute] meanwhile.
  final proto.Fleeing? fleeing;

  /// The group on its trail; null with none.
  final proto.Pursuit? pursuedBy;

  /// Where it last stood in a fight, for the report of a fight it has left;
  /// null when it has been in none this console saw.
  final Hex? foughtAt;

  const FleetState({
    required this.id,
    required this.name,
    required this.sector,
    required this.status,
    required this.shipCount,
    this.cargoPercent = 0,
    this.fuelPercent = 100,
    this.ships = const [],
    this.hull = 0,
    this.hullMax = 0,
    this.attack = 0,
    this.shield = 0,
    this.refit = const proto.Refit(weapons: 0, shielding: 0, armour: 0),
    this.refitDue = false,
    this.repair,
    this.cargo = const FleetCargo(capacity: 1),
    this.fuel = 0,
    this.fuelMax = 1,
    this.jumpFuel = 0,
    this.homeFuel = 0,
    this.hopsHome = 0,
    this.fuelExact,
    this.jumpFuelExact,
    this.jumpTicks = 0,
    this.scanRange = proto.scanBaseRange,
    this.outpostFuel,
    this.atOutpost = false,
    this.relay,
    this.cooldown = 0,
    this.power = 0,
    this.rangeHops = 0,
    this.orders,
    this.laden = false,
    this.cargoBlocked = false,
    this.holdCapacity = 0,
    this.refine,
    this.refuel,
    this.flight,
    this.fleeing,
    this.pursuedBy,
    this.foughtAt,
  });

  /// The doctrine's label, as the server names it; null when flown by hand.
  String? get policy => orders?.preset.label;

  /// What the fleet is doing in a word: the state's, but for a fleet that
  /// is leaving a fight, which the state calls moving.
  String get statusLabel => fleeing == null ? status.label : 'FLEEING';

  /// In a fight, firing or leaving it under fire.
  bool get underFire => status == FleetStatus.combat || fleeing != null;

  int? get fuelForHops {
    final jf = jumpFuelExact ?? jumpFuel.toDouble();
    if (jf <= 0) return null;
    return ((fuelExact ?? fuel.toDouble()) / jf).floor();
  }

  /// Away from home with less than one jump of fuel.
  bool get stranded => homeFuel > 0 && jumpFuel > 0 && fuel < jumpFuel;

  /// Cannot afford the way home with the fuel aboard.
  bool get shortOfHomeFuel => homeFuel > fuel;
}
