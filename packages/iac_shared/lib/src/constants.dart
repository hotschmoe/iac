/// Tunable game values — single source of truth for balance.
library;

// ── Tick ──────────────────────────────────────────────────────────

/// Server tick rate (Hz).
const int tickRateHz = 1;

/// Duration of one tick in nanoseconds.
const int tickDurationNs = 1000000000 ~/ tickRateHz;

// ── Zones ─────────────────────────────────────────────────────────

/// World zones defined by cube distance from origin.
enum Zone {
  centralHub,
  innerRing,
  outerRing,
  wandering;

  /// Maximum distance (inclusive) for inner ring boundary.
  static const int innerRingRadius = 8;

  /// Maximum distance (inclusive) for outer ring boundary.
  static const int outerRingRadius = 20;

  static Zone fromDistance(int dist) {
    if (dist == 0) return Zone.centralHub;
    if (dist <= innerRingRadius) return Zone.innerRing;
    if (dist <= outerRingRadius) return Zone.outerRing;
    return Zone.wandering;
  }

  /// Edge survival probability (0–100) for procedural connectivity.
  int edgeSurvivalPct(int dist) => switch (this) {
        Zone.centralHub => 100,
        Zone.innerRing => 95,
        Zone.outerRing => 80,
        Zone.wandering => () {
            // Decreases from 60% at dist 21 to 40% at dist 60+.
            const base = 60;
            final decay = ((dist - 20).clamp(0, 40)) ~/ 2;
            return base - decay;
          }(),
      };

  String get label => switch (this) {
        Zone.centralHub => 'Central Hub',
        Zone.innerRing => 'Inner Ring',
        Zone.outerRing => 'Outer Ring',
        Zone.wandering => 'The Wandering',
      };

  String get wireName => switch (this) {
        Zone.centralHub => 'central_hub',
        Zone.innerRing => 'inner_ring',
        Zone.outerRing => 'outer_ring',
        Zone.wandering => 'wandering',
      };

  static Zone? fromWire(String? name) {
    if (name == null) return null;
    return switch (name) {
      'central_hub' || 'centralHub' => Zone.centralHub,
      'inner_ring' || 'innerRing' => Zone.innerRing,
      'outer_ring' || 'outerRing' => Zone.outerRing,
      'wandering' => Zone.wandering,
      _ => null,
    };
  }
}

// ── Density ───────────────────────────────────────────────────────

/// Resource density levels.
enum Density {
  none(0),
  sparse(1),
  moderate(2),
  rich(3),
  pristine(4);

  final int value;
  const Density(this.value);

  double get harvestMultiplier => switch (this) {
        Density.none => 0.0,
        Density.sparse => 0.5,
        Density.moderate => 1.0,
        Density.rich => 2.0,
        Density.pristine => 4.0,
      };

  double get depletionThreshold => switch (this) {
        Density.none => 0.0,
        Density.sparse => 10.0,
        Density.moderate => 20.0,
        Density.rich => 30.0,
        Density.pristine => 40.0,
      };

  Density downgrade() => switch (this) {
        Density.pristine => Density.rich,
        Density.rich => Density.moderate,
        Density.moderate => Density.sparse,
        Density.sparse => Density.none,
        Density.none => Density.none,
      };

  Density upgrade() => switch (this) {
        Density.none => Density.sparse,
        Density.sparse => Density.moderate,
        Density.moderate => Density.rich,
        Density.rich => Density.pristine,
        Density.pristine => Density.pristine,
      };

  String get label => switch (this) {
        Density.none => 'None',
        Density.sparse => 'Sparse',
        Density.moderate => 'Moderate',
        Density.rich => 'Rich',
        Density.pristine => 'Pristine',
      };

  String get wireName => name;

  static Density fromWire(String? name, {Density fallback = Density.none}) {
    if (name == null) return fallback;
    for (final d in Density.values) {
      if (d.name == name || d.wireName == name) return d;
    }
    // Accept numeric
    final n = int.tryParse(name);
    if (n != null) {
      for (final d in Density.values) {
        if (d.value == n) return d;
      }
    }
    return fallback;
  }

  static Density fromValue(int? v, {Density fallback = Density.none}) {
    if (v == null) return fallback;
    for (final d in Density.values) {
      if (d.value == v) return d;
    }
    return fallback;
  }
}

// ── Terrain ───────────────────────────────────────────────────────

/// Terrain types for sectors.
enum TerrainType {
  empty,
  asteroidField,
  nebula,
  debrisField,
  anomaly;

  String get label => switch (this) {
        TerrainType.empty => 'Empty Space',
        TerrainType.asteroidField => 'Asteroid Field',
        TerrainType.nebula => 'Nebula',
        TerrainType.debrisField => 'Debris Field',
        TerrainType.anomaly => 'Anomaly',
      };

  /// Map symbol for rendering.
  String get symbol => switch (this) {
        TerrainType.empty => '·',
        TerrainType.asteroidField => '*',
        TerrainType.nebula => '≈',
        TerrainType.debrisField => '×',
        TerrainType.anomaly => '?',
      };

  String get wireName => switch (this) {
        TerrainType.empty => 'empty',
        TerrainType.asteroidField => 'asteroid_field',
        TerrainType.nebula => 'nebula',
        TerrainType.debrisField => 'debris_field',
        TerrainType.anomaly => 'anomaly',
      };

  static TerrainType fromWire(
    String? name, {
    TerrainType fallback = TerrainType.empty,
  }) {
    if (name == null) return fallback;
    return switch (name) {
      'empty' => TerrainType.empty,
      'asteroid_field' || 'asteroidField' || 'asteroid' =>
        TerrainType.asteroidField,
      'nebula' => TerrainType.nebula,
      'debris_field' || 'debrisField' || 'debris' => TerrainType.debrisField,
      'anomaly' => TerrainType.anomaly,
      _ => fallback,
    };
  }
}

// ── Ships ─────────────────────────────────────────────────────────

/// Base stats for a ship class.
class ShipStats {
  final double hull;
  final double shield;
  final double weapon;
  final int speed;
  final int cargo;
  final int fuel;

  const ShipStats({
    required this.hull,
    required this.shield,
    required this.weapon,
    required this.speed,
    required this.cargo,
    required this.fuel,
  });
}

/// Ship class definitions.
enum ShipClass {
  scout,
  corvette,
  frigate,
  cruiser,
  hauler;

  static const List<ShipClass> all = ShipClass.values;

  ShipStats get baseStats => switch (this) {
        ShipClass.scout => const ShipStats(
            hull: 30,
            shield: 10,
            weapon: 5,
            speed: 10,
            cargo: 20,
            fuel: 60,
          ),
        ShipClass.corvette => const ShipStats(
            hull: 50,
            shield: 20,
            weapon: 15,
            speed: 8,
            cargo: 10,
            fuel: 50,
          ),
        ShipClass.frigate => const ShipStats(
            hull: 120,
            shield: 60,
            weapon: 30,
            speed: 5,
            cargo: 30,
            fuel: 80,
          ),
        ShipClass.cruiser => const ShipStats(
            hull: 200,
            shield: 100,
            weapon: 80,
            speed: 4,
            cargo: 50,
            fuel: 120,
          ),
        ShipClass.hauler => const ShipStats(
            hull: 80,
            shield: 20,
            weapon: 5,
            speed: 6,
            cargo: 200,
            fuel: 100,
          ),
      };

  String get label => switch (this) {
        ShipClass.scout => 'Scout',
        ShipClass.corvette => 'Corvette',
        ShipClass.frigate => 'Frigate',
        ShipClass.cruiser => 'Cruiser',
        ShipClass.hauler => 'Hauler',
      };

  /// Build cost in resources.
  ResourceAmounts get buildCost => switch (this) {
        ShipClass.scout =>
          const ResourceAmounts(metal: 200, crystal: 50, deuterium: 30),
        ShipClass.corvette =>
          const ResourceAmounts(metal: 400, crystal: 100, deuterium: 60),
        ShipClass.frigate =>
          const ResourceAmounts(metal: 1000, crystal: 400, deuterium: 200),
        ShipClass.cruiser =>
          const ResourceAmounts(metal: 3000, crystal: 1500, deuterium: 800),
        ShipClass.hauler =>
          const ResourceAmounts(metal: 600, crystal: 200, deuterium: 150),
      };

  /// Rapid-fire multiplier against [target] (0 = no bonus).
  int rapidFireVs(ShipClass target) => switch (this) {
        ShipClass.corvette => target == ShipClass.scout ? 3 : 0,
        ShipClass.frigate => target == ShipClass.corvette ? 2 : 0,
        ShipClass.cruiser => target == ShipClass.frigate ? 2 : 0,
        _ => 0,
      };

  String get wireName => name;

  static ShipClass fromWire(
    String? name, {
    ShipClass fallback = ShipClass.scout,
  }) {
    if (name == null) return fallback;
    for (final c in ShipClass.values) {
      if (c.name == name || c.wireName == name) return c;
    }
    return fallback;
  }
}

// ── Resources ─────────────────────────────────────────────────────

/// Three-resource stock (metal / crystal / deuterium).
class ResourceAmounts {
  final double metal;
  final double crystal;
  final double deuterium;

  const ResourceAmounts({
    this.metal = 0,
    this.crystal = 0,
    this.deuterium = 0,
  });

  static const zero = ResourceAmounts();

  ResourceAmounts add(ResourceAmounts other) => ResourceAmounts(
        metal: metal + other.metal,
        crystal: crystal + other.crystal,
        deuterium: deuterium + other.deuterium,
      );

  ResourceAmounts sub(ResourceAmounts other) => ResourceAmounts(
        metal: metal - other.metal,
        crystal: crystal - other.crystal,
        deuterium: deuterium - other.deuterium,
      );

  ResourceAmounts scale(double factor) => ResourceAmounts(
        metal: metal * factor,
        crystal: crystal * factor,
        deuterium: deuterium * factor,
      );

  bool canAfford(ResourceAmounts cost) =>
      metal >= cost.metal &&
      crystal >= cost.crystal &&
      deuterium >= cost.deuterium;

  Map<String, dynamic> toJson() => {
        'metal': metal,
        'crystal': crystal,
        'deuterium': deuterium,
      };

  factory ResourceAmounts.fromJson(Map<String, dynamic>? json) {
    if (json == null) return zero;
    return ResourceAmounts(
      metal: (json['metal'] as num?)?.toDouble() ?? 0,
      crystal: (json['crystal'] as num?)?.toDouble() ?? 0,
      deuterium: (json['deuterium'] as num?)?.toDouble() ??
          (json['deut'] as num?)?.toDouble() ??
          0,
    );
  }

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is ResourceAmounts &&
          other.metal == metal &&
          other.crystal == crystal &&
          other.deuterium == deuterium;

  @override
  int get hashCode => Object.hash(metal, crystal, deuterium);

  @override
  String toString() =>
      'ResourceAmounts(m=$metal, c=$crystal, d=$deuterium)';
}

/// Alias matching Zig `Resources`.
typedef Resources = ResourceAmounts;

// ── Starting / limits ─────────────────────────────────────────────

const ResourceAmounts startingResources = ResourceAmounts(
  metal: 500,
  crystal: 300,
  deuterium: 100,
);

const int startingScouts = 2;
const int maxFleets = 3;
const int maxFleetsPerPlayer = maxFleets;

/// Homeworld spawn range (cube distance from origin).
const int homeworldMinDist = 3;
const int homeworldMaxDist = 6;

// ── Cooldowns (ticks) ─────────────────────────────────────────────

const int moveBaseCooldown = 1;
const int harvestCooldown = 1;
const int scanCooldown = 5;

// ── Combat ────────────────────────────────────────────────────────

const double damageVarianceMin = 0.8;
const double damageVarianceMax = 1.2;
const int shieldRegenIdleTicks = 10;

// ── Emergency recall ──────────────────────────────────────────────

const double recallFuelMultiplier = 2.0;
const double recallDamageChancePerHex = 0.02;
const double recallDamageChanceCap = 0.60;
const double recallHullDamageMin = 0.20;
const double recallHullDamageMax = 0.80;

// ── Fuel / regen ──────────────────────────────────────────────────

/// Fuel consumption per hex: fleet_mass * this value.
const double fuelRatePerMass = 0.1;

/// Resource regeneration: sectors regen this fraction per tick.
const double sectorRegenRate = 0.0001;

// ── NPC ───────────────────────────────────────────────────────────

const int npcRespawnInner = 300; // 5 min @ 1Hz
const int npcRespawnOuter = 600; // 10 min
const int npcRespawnWandering = 1200; // 20 min
const int npcPatrolInterval = 15;

int npcRespawnDelay(Zone zone) => switch (zone) {
      Zone.centralHub => npcRespawnInner,
      Zone.innerRing => npcRespawnInner,
      Zone.outerRing => npcRespawnOuter,
      Zone.wandering => npcRespawnWandering,
    };

// ── Salvage ───────────────────────────────────────────────────────

const double salvageFraction = 0.30;
const int salvageDespawnTicks = 60;

// ── Network / world defaults ──────────────────────────────────────

const int defaultPort = 7777;
const String defaultHost = '127.0.0.1';

/// World seed (overridable at server start).
/// Kept within JS safe integer range so the same package compiles for Flutter web.
const int defaultWorldSeed = 0xDEADBEEF;
