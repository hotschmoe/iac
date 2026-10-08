/// Axial hex coordinates with cube distance calculations.
/// Flat-top orientation. Stored as (q, r); cube s = -q - r is derived.
///
/// Layout (flat-top, axial):
///
///        -r
///    NW  /  NE
///      \/
///  W ──    ── E      +q →
///      /\
///    SW  \  SE
///        +r
library;

/// The 6 hex directions (flat-top orientation).
enum HexDirection {
  east,
  northEast,
  northWest,
  west,
  southWest,
  southEast;

  static const List<HexDirection> all = [
    HexDirection.east,
    HexDirection.northEast,
    HexDirection.northWest,
    HexDirection.west,
    HexDirection.southWest,
    HexDirection.southEast,
  ];

  /// Direction vector in axial coordinates.
  Hex toVec() => switch (this) {
        HexDirection.east => const Hex(1, 0),
        HexDirection.northEast => const Hex(1, -1),
        HexDirection.northWest => const Hex(0, -1),
        HexDirection.west => const Hex(-1, 0),
        HexDirection.southWest => const Hex(-1, 1),
        HexDirection.southEast => const Hex(0, 1),
      };

  /// Opposite direction.
  HexDirection opposite() => HexDirection.all[(index + 3) % 6];

  /// Short label for display.
  String get label => switch (this) {
        HexDirection.east => 'E',
        HexDirection.northEast => 'NE',
        HexDirection.northWest => 'NW',
        HexDirection.west => 'W',
        HexDirection.southWest => 'SW',
        HexDirection.southEast => 'SE',
      };

  static HexDirection? fromLabel(String label) {
    final upper = label.toUpperCase();
    for (final d in all) {
      if (d.label == upper) return d;
    }
    return null;
  }

  static HexDirection? fromIndex(int i) {
    if (i < 0 || i >= 6) return null;
    return all[i];
  }
}

/// Hex coordinate in axial form.
class Hex {
  final int q;
  final int r;

  const Hex(this.q, this.r);

  static const Hex origin = Hex(0, 0);

  /// Cube coordinate s, derived from q and r.
  int get s => -q - r;

  /// Cube distance between two hexes.
  static int distance(Hex a, Hex b) {
    final dq = a.q - b.q;
    final dr = a.r - b.r;
    final ds = a.s - b.s;
    final adq = dq.abs();
    final adr = dr.abs();
    final ads = ds.abs();
    return adq > adr
        ? (adq > ads ? adq : ads)
        : (adr > ads ? adr : ads);
  }

  /// Distance from the origin (0, 0).
  int get distFromOrigin => distance(this, origin);

  Hex operator +(Hex other) => Hex(q + other.q, r + other.r);

  Hex operator -(Hex other) => Hex(q - other.q, r - other.r);

  /// Neighbor in the given direction.
  Hex neighbor(HexDirection dir) => this + dir.toVec();

  /// All 6 neighbor coordinates.
  List<Hex> neighbors() =>
      HexDirection.all.map((d) => neighbor(d)).toList(growable: false);

  /// Pack into a 32-bit key for hashing / map lookups.
  /// Uses bit-cast of signed 16-bit q/r into a u32 (low 16 = q, high 16 = r).
  int toKey() {
    final uq = q & 0xFFFF;
    final ur = r & 0xFFFF;
    return (uq | (ur << 16)) & 0xFFFFFFFF;
  }

  /// Unpack from a 32-bit key.
  static Hex fromKey(int key) {
    final uq = key & 0xFFFF;
    final ur = (key >> 16) & 0xFFFF;
    // Sign-extend 16-bit values.
    final q = uq >= 0x8000 ? uq - 0x10000 : uq;
    final r = ur >= 0x8000 ? ur - 0x10000 : ur;
    return Hex(q, r);
  }

  Map<String, dynamic> toJson() => {'q': q, 'r': r};

  factory Hex.fromJson(Map<String, dynamic>? json) {
    if (json == null) return origin;
    return Hex(
      (json['q'] as num?)?.toInt() ?? 0,
      (json['r'] as num?)?.toInt() ?? 0,
    );
  }

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is Hex && other.q == q && other.r == r;

  @override
  int get hashCode => Object.hash(q, r);

  @override
  String toString() => '[$q,$r]';
}

/// All hexes in a ring at [radius] from [center].
Iterable<Hex> hexRing(Hex center, int radius) sync* {
  if (radius == 0) {
    yield center;
    return;
  }
  var current = center;
  for (var i = 0; i < radius; i++) {
    current = current.neighbor(HexDirection.southWest);
  }
  const walkDirs = HexDirection.all;
  for (var side = 0; side < 6; side++) {
    for (var step = 0; step < radius; step++) {
      yield current;
      current = current.neighbor(walkDirs[side]);
    }
  }
}

/// All hexes in a spiral from [center] out to [maxRadius] (inclusive).
Iterable<Hex> hexSpiral(Hex center, int maxRadius) sync* {
  for (var radius = 0; radius <= maxRadius; radius++) {
    yield* hexRing(center, radius);
  }
}
