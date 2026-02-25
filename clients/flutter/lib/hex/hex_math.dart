import 'dart:math' as math;

import 'package:flutter/painting.dart';

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

int sectorHash(int q, int r) {
  return (q * 73856093 ^ r * 19349663 ^ 83492791) & 0xFFFFFFFF;
}

Prng sectorRng(int q, int r) => mulberry32(sectorHash(q, r));

bool getEdge(int q1, int r1, int q2, int r2) {
  final a = (q1 * 73856093 ^ r1 * 19349663) & 0xFFFFFFFF;
  final b = (q2 * 73856093 ^ r2 * 19349663) & 0xFFFFFFFF;
  final lo = math.min(a, b);
  final hi = math.max(a, b);
  final seed = (lo * 83492791 ^ hi) & 0xFFFFFFFF;
  final rng = mulberry32(seed);
  final d1 = hexDist(q1, r1);
  final d2 = hexDist(q2, r2);
  final dist = math.max(d1, d2);
  final pct = dist < 8 ? 0.95 : dist < 20 ? 0.8 : math.max(0.4, 0.6 - dist * 0.005);
  return rng() < pct;
}

int hexDist(int q, int r) {
  return math.max(q.abs(), math.max(r.abs(), (q + r).abs()));
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
