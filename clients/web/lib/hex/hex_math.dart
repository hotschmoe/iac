import 'dart:math' as math;

import 'package:flutter/painting.dart';
import '../protocol/hex.dart';

/// Zone names by distance from the hub (constants.rs `Zone`: inner ring <= 8,
/// outer ring <= 20).
extension HexZone on Hex {
  String get zone {
    final d = distFromOrigin;
    if (d == 0) return 'Central Hub';
    if (d <= 8) return 'Inner Ring';
    if (d <= 20) return 'Outer Ring';
    return 'The Wandering';
  }
}

const double sqrt3 = 1.7320508075688772;

Offset hexToPixel(int q, int r, double size) {
  final x = size * (1.5 * q);
  final y = size * (sqrt3 / 2 * q + sqrt3 * r);
  return Offset(x, y);
}

typedef Prng = double Function();

Prng mulberry32(int seed) {
  int a = seed;
  return () {
    a = (a + 0x6D2B79F5) & 0xFFFFFFFF;
    int t = math.max(0, ((a ^ (a >> 15)) * (1 | a)) & 0xFFFFFFFF);
    t = ((t + ((t ^ (t >> 7)) * (61 | t)) & 0xFFFFFFFF) ^ t) & 0xFFFFFFFF;
    return ((t ^ (t >> 14)) & 0x7FFFFFFF) / 2147483648.0;
  };
}

void drawMapText(
  Canvas canvas,
  String text,
  double x,
  double y,
  double size,
  Color color, {
  bool center = false,
  bool bold = false,
}) {
  final tp = TextPainter(
    text: TextSpan(
      text: text,
      style: TextStyle(
        fontFamily: 'JetBrains Mono',
        fontSize: size,
        color: color,
        fontWeight: bold ? FontWeight.bold : FontWeight.normal,
      ),
    ),
    textDirection: TextDirection.ltr,
  )..layout();
  final offset = center
      ? Offset(x - tp.width / 2, y - tp.height / 2)
      : Offset(x, y - tp.height / 2);
  tp.paint(canvas, offset);
}

void drawStarField(Canvas canvas, Size size, int seed, int count, Color color) {
  final rng = mulberry32(seed);
  final paint = Paint()..color = color;
  for (int i = 0; i < count; i++) {
    canvas.drawCircle(
      Offset(rng() * size.width, rng() * size.height),
      rng() * 1.5,
      paint,
    );
  }
}
