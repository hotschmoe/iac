import 'dart:math' as math;

import '../models/hex.dart';
import '../models/sector.dart';
import 'hex_math.dart';

SectorState getSectorData(int q, int r) {
  final rng = sectorRng(q, r);
  final dist = hexDist(q, r);

  final terrains = Terrain.values;
  final tw = dist < 8
      ? [0.2, 0.4, 0.15, 0.15, 0.1]
      : dist < 20
          ? [0.3, 0.3, 0.15, 0.15, 0.1]
          : [0.4, 0.2, 0.15, 0.15, 0.1];

  double tc = 0;
  final tr = rng();
  var terrain = terrains[0];
  for (int i = 0; i < terrains.length; i++) {
    tc += tw[i];
    if (tr < tc) {
      terrain = terrains[i];
      break;
    }
  }

  final densities = Density.values;
  var resMetal = Density.none;
  var resCrystal = Density.none;
  var resDeut = Density.none;

  if (terrain == Terrain.asteroid ||
      terrain == Terrain.nebula ||
      terrain == Terrain.debris) {
    final ri = math.min(4, (rng() * (dist < 8 ? 3 : dist < 20 ? 4 : 5)).floor());
    resMetal = densities[ri];
    resCrystal = densities[math.max(0, ri - 1 - (rng() * 2).floor())];
    resDeut = densities[math.max(0, ri - 2 - (rng() * 2).floor())];
  }

  final hasHostile = rng() < math.min(0.6, dist * 0.03);
  final hostileCount =
      hasHostile ? (rng() * math.min(8, dist * 0.4)).ceil() : 0;

  final prunePct = dist < 8
      ? 0.95
      : dist < 20
          ? 0.8
          : math.max(0.4, 0.6 - dist * 0.005);

  final explored = dist < 18 && rng() > 0.3;

  return SectorState(
    q: q,
    r: r,
    terrain: terrain,
    resMetal: resMetal,
    resCrystal: resCrystal,
    resDeut: resDeut,
    hasHostile: hasHostile,
    hostileCount: hostileCount,
    prunePct: prunePct,
    explored: explored,
    dist: dist,
  );
}

List<Hex> getNeighbors(int q, int r) {
  final neighbors = <Hex>[];
  for (final dir in Hex.directions) {
    final nq = q + dir.q;
    final nr = r + dir.r;
    if (getEdge(q, r, nq, nr)) {
      neighbors.add(Hex(nq, nr));
    }
  }
  return neighbors;
}
