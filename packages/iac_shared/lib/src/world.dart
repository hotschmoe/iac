/// Deterministic procedural world generation.
/// Demo-quality port of src/shared/world.zig — same coordinates + seed
/// always produce the same sector template and edge connectivity.
library;

import 'dart:math' as math;

import 'constants.dart';
import 'hex.dart';

/// Seed-based sector generation.
class WorldGen {
  final int worldSeed;

  const WorldGen(this.worldSeed);

  /// Zig-style constructor alias.
  factory WorldGen.init(int seed) => WorldGen(seed);

  /// Generate base sector properties from coordinates.
  SectorTemplate generateSector(Hex coord) {
    final seed = sectorSeed(coord);
    final rng = _Rng(seed);

    final dist = coord.distFromOrigin;
    final zone = Zone.fromDistance(dist);

    final terrain = _rollTerrain(rng, zone);
    final resources = _rollResources(rng, zone, terrain);
    final npc = _rollNpcPresence(rng, zone, dist);

    return SectorTemplate(
      coord: coord,
      zone: zone,
      terrain: terrain,
      metalDensity: resources.$1,
      crystalDensity: resources.$2,
      deutDensity: resources.$3,
      npcTemplate: npc,
    );
  }

  /// Bitmask of traversable directions (bit 0 = east … bit 5 = SE).
  int sectorConnections(Hex coord) {
    var mask = 0;
    var connectedCount = 0;
    var lastDir = 0;

    for (var i = 0; i < 6; i++) {
      final dir = HexDirection.all[i];
      final neighbor = coord.neighbor(dir);
      if (edgeExists(coord, neighbor)) {
        mask |= 1 << i;
        connectedCount++;
        lastDir = i;
      }
    }

    // Guarantee at least one connection.
    if (connectedCount == 0) {
      mask |= 1 << lastDir;
    }
    return mask;
  }

  /// Traversable neighbor coordinates.
  List<Hex> connectedNeighbors(Hex coord) {
    final mask = sectorConnections(coord);
    final result = <Hex>[];
    for (var i = 0; i < 6; i++) {
      if ((mask & (1 << i)) != 0) {
        result.add(coord.neighbor(HexDirection.all[i]));
      }
    }
    return result;
  }

  bool edgeExists(Hex a, Hex b) {
    // Central hub (origin) is fully connected for navigation playability.
    if (a == Hex.origin || b == Hex.origin) return true;

    final keyA = a.toKey();
    final keyB = b.toKey();
    final edgeSeed = keyA < keyB ? _hashTwo(keyA, keyB) : _hashTwo(keyB, keyA);

    final maxDist = math.max(a.distFromOrigin, b.distFromOrigin);
    final zone = Zone.fromDistance(maxDist);
    final survivalPct = zone.edgeSurvivalPct(maxDist);

    final roll = edgeSeed % 100;
    return roll < survivalPct;
  }

  TerrainType _rollTerrain(_Rng rng, Zone zone) {
    final roll = rng.nextInt(101);
    return switch (zone) {
      Zone.centralHub => TerrainType.empty,
      Zone.innerRing => () {
          if (roll < 55) return TerrainType.asteroidField;
          if (roll < 70) return TerrainType.nebula;
          if (roll < 80) return TerrainType.debrisField;
          return TerrainType.empty;
        }(),
      Zone.outerRing => () {
          if (roll < 35) return TerrainType.asteroidField;
          if (roll < 50) return TerrainType.nebula;
          if (roll < 70) return TerrainType.debrisField;
          if (roll < 75) return TerrainType.anomaly;
          return TerrainType.empty;
        }(),
      Zone.wandering => () {
          if (roll < 30) return TerrainType.asteroidField;
          if (roll < 45) return TerrainType.nebula;
          if (roll < 65) return TerrainType.debrisField;
          if (roll < 80) return TerrainType.anomaly;
          return TerrainType.empty;
        }(),
    };
  }

  (Density, Density, Density) _rollResources(
    _Rng rng,
    Zone zone,
    TerrainType terrain,
  ) {
    if (terrain == TerrainType.empty) {
      return (Density.none, Density.none, Density.none);
    }

    final zoneBoost = switch (zone) {
      Zone.centralHub => 0,
      Zone.innerRing => 30,
      Zone.outerRing => 40,
      Zone.wandering => 50,
    };

    final metalChance =
        (terrain == TerrainType.asteroidField ? 60 : 20) + zoneBoost;
    final crystalChance =
        (terrain == TerrainType.asteroidField || terrain == TerrainType.nebula
                ? 40
                : 10) +
            zoneBoost;
    final deutChance = (terrain == TerrainType.nebula ? 50 : 15) + zoneBoost;

    return (
      _rollDensity(rng, metalChance),
      _rollDensity(rng, crystalChance),
      _rollDensity(rng, deutChance),
    );
  }

  Density _rollDensity(_Rng rng, int chance) {
    final roll = rng.nextInt(101);
    if (roll >= chance) return Density.none;
    final quality = rng.nextInt(101);
    if (quality < 40) return Density.sparse;
    if (quality < 70) return Density.moderate;
    if (quality < 90) return Density.rich;
    return Density.pristine;
  }

  NpcTemplate? _rollNpcPresence(_Rng rng, Zone zone, int dist) {
    final presenceChance = switch (zone) {
      Zone.centralHub => 0,
      Zone.innerRing => 30,
      Zone.outerRing => 70,
      Zone.wandering => 80,
    };
    if (rng.nextInt(101) >= presenceChance) return null;

    if (dist <= 8) {
      final behavior =
          rng.nextBool() ? NpcBehavior.passive : NpcBehavior.patrol;
      return NpcTemplate(
        shipClass: ShipClass.scout,
        count: 1,
        behavior: behavior,
        statMultiplier: 0.6,
      );
    } else if (dist <= 15) {
      return NpcTemplate(
        shipClass: ShipClass.corvette,
        count: 3 + rng.nextInt(6),
        behavior: NpcBehavior.patrol,
        statMultiplier: 0.8,
      );
    } else if (dist <= 25) {
      return NpcTemplate(
        shipClass: ShipClass.frigate,
        count: 5 + rng.nextInt(11),
        behavior: NpcBehavior.aggressive,
        statMultiplier: 1.0,
      );
    } else {
      return NpcTemplate(
        shipClass: ShipClass.cruiser,
        count: 10 + rng.nextInt(21),
        behavior: NpcBehavior.swarm,
        statMultiplier: 1.2,
      );
    }
  }

  int sectorSeed(Hex coord) => _hashTwo(coord.toKey(), worldSeed);

  /// 32-bit FNV-1a style combiner — JS-safe (Flutter web / dart2js).
  int _hashTwo(int a, int b) {
    var h = 0x811C9DC5;
    h = _mix(h, a);
    h = _mix(h, b);
    h = _mix(h, worldSeed);
    return h & 0x7FFFFFFF;
  }

  int _mix(int h, int v) {
    h ^= v & 0xFFFFFFFF;
    h = (h * 0x01000193) & 0xFFFFFFFF;
    h ^= (v >> 16) & 0xFFFF;
    h = (h * 0x01000193) & 0xFFFFFFFF;
    return h;
  }
}

class NpcTemplate {
  final ShipClass shipClass;
  final int count;
  final NpcBehavior behavior;
  final double statMultiplier;

  const NpcTemplate({
    required this.shipClass,
    required this.count,
    required this.behavior,
    this.statMultiplier = 1.0,
  });
}

enum NpcBehavior {
  passive,
  patrol,
  aggressive,
  swarm;

  String get wireName => name;

  static NpcBehavior fromWire(String? name, {NpcBehavior fallback = NpcBehavior.passive}) {
    if (name == null) return fallback;
    for (final b in NpcBehavior.values) {
      if (b.name == name) return b;
    }
    return fallback;
  }
}

class SectorTemplate {
  final Hex coord;
  final Zone zone;
  final TerrainType terrain;
  final Density metalDensity;
  final Density crystalDensity;
  final Density deutDensity;
  final NpcTemplate? npcTemplate;

  const SectorTemplate({
    required this.coord,
    required this.zone,
    required this.terrain,
    required this.metalDensity,
    required this.crystalDensity,
    required this.deutDensity,
    this.npcTemplate,
  });
}

/// Lightweight deterministic PRNG (mulberry32) — 32-bit, JS-safe.
class _Rng {
  int _state;

  _Rng(int seed) : _state = (seed == 0 ? 0xDEADBEEF : seed) & 0xFFFFFFFF;

  int nextInt(int max) {
    if (max <= 0) return 0;
    return next() % max;
  }

  bool nextBool() => (next() & 1) == 1;

  int next() {
    _state = (_state + 0x6D2B79F5) & 0xFFFFFFFF;
    var t = _state;
    t = ((t ^ (t >> 15)) * (1 | t)) & 0xFFFFFFFF;
    t = (t + ((t ^ (t >> 7)) * (61 | t))) & 0xFFFFFFFF;
    return ((t ^ (t >> 14)) & 0x7FFFFFFF);
  }
}
