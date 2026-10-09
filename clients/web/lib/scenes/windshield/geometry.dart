import 'dart:math' as math;
import 'dart:ui';

import '../../protocol/protocol.dart' as proto;

/// Screen-space compass of the windshield: key, vector, bearing label.
/// Directions are named by screen position; the vector is the contract.
class Dir {
  final int i;
  final String name;
  final proto.Hex v;
  final String key;
  final String bearing;
  const Dir(this.i, this.name, this.v, this.key, this.bearing);

  /// Angle on screen of this direction (flat-top axial projection).
  double get angle => math.atan2(math.sqrt(3) / 2 * v.q + math.sqrt(3) * v.r, 1.5 * v.q);
}

const dirs = [
  Dir(0, 'N', proto.Hex(0, -1), 'w', '000'),
  Dir(1, 'NE', proto.Hex(1, -1), 'e', '060'),
  Dir(2, 'SE', proto.Hex(1, 0), 'd', '120'),
  Dir(3, 'S', proto.Hex(0, 1), 's', '180'),
  Dir(4, 'SW', proto.Hex(-1, 1), 'a', '240'),
  Dir(5, 'NW', proto.Hex(-1, 0), 'q', '300'),
];

/// Arena placement inside the main area (prototype resize()).
class Geo {
  final Size size;
  final bool narrow;
  late final double cx, cy, rh, sc;

  Geo(this.size) : narrow = size.width < 900 {
    final w = size.width, h = size.height;
    final left = narrow ? 0.0 : 236.0, right = narrow ? 0.0 : 324.0;
    final aw = w - left - right, ah = h - (narrow ? 150 : 112);
    cx = left + aw / 2;
    cy = (narrow ? 50 : 10) + ah / 2 + (narrow ? 12 : 22);
    rh = math.max(130.0, math.min(math.min(aw / (narrow ? 2.8 : 2.7), ah / (narrow ? 2.3 : 2.45)), 300.0));
    sc = rh / 300;
  }

  Offset wp(double x, double y) => Offset(cx + x * rh, cy + y * rh);

  Offset gatePoint(Dir d) => Offset(cx + math.cos(d.angle) * rh * .866, cy + math.sin(d.angle) * rh * .866);

  /// Centre of the gate card, pushed outside the hex.
  Offset cardCenter(Dir d) {
    final a = d.angle;
    final g = gatePoint(d);
    final gw = narrow ? 46.0 : 75.0, gh = narrow ? 14.0 : 36.0;
    final dd = gw * math.cos(a).abs() + gh * math.sin(a).abs() + (narrow ? 8 : 14);
    var kx = g.dx + math.cos(a) * dd;
    final ky = g.dy + math.sin(a) * dd;
    if (narrow) kx = kx.clamp(48.0, size.width - 48);
    return Offset(kx, ky);
  }
}
