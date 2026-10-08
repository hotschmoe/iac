// Axial hex coordinates. Mirrors shared/src/hex.rs exactly (flat-top,
// cube s = -q - r). Golden fixtures in <repo>/fixtures/hex pin the results.

import 'dart:math' as math;

/// Axial hex coordinate. JSON form: {"q": int, "r": int}.
class Hex {
  final int q;
  final int r;

  const Hex(this.q, this.r);

  factory Hex.fromJson(Object? json) {
    final m = json as Map<String, dynamic>;
    return Hex(m['q'] as int, m['r'] as int);
  }

  Map<String, dynamic> toJson() => {'q': q, 'r': r};

  static const Hex origin = Hex(0, 0);

  /// Cube coordinate s, derived.
  int get s => -q - r;

  /// Distance from the origin.
  int get distFromOrigin => distance(this, origin);

  /// Cube distance between two hexes.
  static int distance(Hex a, Hex b) =>
      math.max((a.q - b.q).abs(), math.max((a.r - b.r).abs(), (a.s - b.s).abs()));

  Hex operator +(Hex o) => Hex(q + o.q, r + o.r);
  Hex operator -(Hex o) => Hex(q - o.q, r - o.r);

  Hex neighbor(HexDirection dir) => this + dir.vec;

  /// The 6 neighbors, in [HexDirection.values] order.
  List<Hex> neighbors() => [for (final d in HexDirection.values) neighbor(d)];

  /// Pack into the same u32 key Rust uses (`Hex::to_key`).
  int toKey() => (r & 0xFFFF) * 0x10000 + (q & 0xFFFF); // arithmetic: JS-safe

  /// Inverse of [toKey] (`Hex::from_key`).
  static Hex fromKey(int key) {
    int i16(int v) => v >= 0x8000 ? v - 0x10000 : v;
    return Hex(i16(key % 0x10000), i16((key ~/ 0x10000) % 0x10000));
  }

  @override
  bool operator ==(Object other) => other is Hex && other.q == q && other.r == r;

  @override
  int get hashCode => Object.hash(q, r);

  @override
  String toString() => '[$q,$r]';
}

/// The 6 hex directions (flat-top), in the same order as Rust's
/// `HexDirection::ALL`.
enum HexDirection {
  east('E', Hex(1, 0)),
  northEast('NE', Hex(1, -1)),
  northWest('NW', Hex(0, -1)),
  west('W', Hex(-1, 0)),
  southWest('SW', Hex(-1, 1)),
  southEast('SE', Hex(0, 1));

  final String label;
  final Hex vec;
  const HexDirection(this.label, this.vec);

  HexDirection get opposite => HexDirection.values[(index + 3) % 6];
}

/// All hexes at exactly [radius] from [center], walking from the south-west
/// corner east then counter-clockwise (same order as Rust `hex_ring`).
List<Hex> hexRing(Hex center, int radius) {
  if (radius == 0) return [center];
  var cur = center;
  for (var i = 0; i < radius; i++) {
    cur = cur.neighbor(HexDirection.southWest);
  }
  const walk = [
    HexDirection.east,
    HexDirection.northEast,
    HexDirection.northWest,
    HexDirection.west,
    HexDirection.southWest,
    HexDirection.southEast,
  ];
  final out = <Hex>[];
  for (final dir in walk) {
    for (var step = 0; step < radius; step++) {
      out.add(cur);
      cur = cur.neighbor(dir);
    }
  }
  return out;
}

/// All hexes within [maxRadius] of [center]: center, then ring 1, ring 2 ...
/// (same order as Rust `hex_spiral`).
List<Hex> hexSpiral(Hex center, int maxRadius) => [
      for (var r = 0; r <= maxRadius; r++) ...hexRing(center, r),
    ];
