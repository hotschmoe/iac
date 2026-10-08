import 'dart:math' as math;

class Hex {
  final int q;
  final int r;

  const Hex(this.q, this.r);

  int get s => -q - r;

  int get distance => math.max(q.abs(), math.max(r.abs(), s.abs()));

  Hex operator +(Hex other) => Hex(q + other.q, r + other.r);
  Hex operator -(Hex other) => Hex(q - other.q, r - other.r);

  String get zone {
    final d = distance;
    if (d == 0) return 'Central Hub';
    if (d < 9) return 'Inner Ring';
    if (d < 21) return 'Outer Ring';
    return 'The Wandering';
  }

  @override
  bool operator ==(Object other) =>
      other is Hex && other.q == q && other.r == r;

  @override
  int get hashCode => Object.hash(q, r);

  @override
  String toString() => '[$q, $r]';

  static const List<Hex> directions = [
    Hex(1, 0),   // E
    Hex(1, -1),  // NE
    Hex(0, -1),  // NW
    Hex(-1, 0),  // W
    Hex(-1, 1),  // SW
    Hex(0, 1),   // SE
  ];

  static const List<String> directionLabels = [
    'E', 'NE', 'NW', 'W', 'SW', 'SE',
  ];

  Hex neighbor(int direction) => this + directions[direction];

  static int hexDistance(Hex a, Hex b) => (a - b).distance;
}
