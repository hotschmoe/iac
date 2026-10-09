import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'symbols.dart';
import 'tokens.dart';

/// Painter helpers shared by every scene: stroked symbols with an optional
/// additive bloom underlay, dashes, hatches, text.
class Draw {
  Draw._();

  static final _stroke = Paint()
    ..style = PaintingStyle.stroke
    ..strokeJoin = StrokeJoin.miter
    ..strokeCap = StrokeCap.butt
    ..isAntiAlias = true;
  static final _bloom = Paint()
    ..style = PaintingStyle.stroke
    ..strokeJoin = StrokeJoin.miter
    ..blendMode = BlendMode.plus
    ..isAntiAlias = true;

  /// Stroke [p] (in +-50 units) at [x],[y] scaled to half-size [s] pixels.
  static void path(Canvas c, Path p, double x, double y, double s, double rot, Color col, double lw,
      {double alpha = 1, bool bloom = true}) {
    final k = s / 50;
    c.save();
    c.translate(x, y);
    if (rot != 0) c.rotate(rot);
    c.scale(k, k);
    if (bloom) {
      _bloom
        ..strokeWidth = lw * 3.6 / k
        ..color = col.withValues(alpha: alpha * .12);
      c.drawPath(p, _bloom);
    }
    _stroke
      ..strokeWidth = lw / k
      ..color = col.withValues(alpha: alpha);
    c.drawPath(p, _stroke);
    c.restore();
  }

  static void symbol(Canvas c, SymbolDef d, double x, double y, double s,
      {double rot = 0, Color color = C.a500, double lw = 1.3, double alpha = 1, bool bloom = true}) {
    path(c, d.outline, x, y, s, rot, color, lw, alpha: alpha, bloom: bloom);
    if (d.detail != null) path(c, d.detail!, x, y, s, rot, color, 1, alpha: alpha * .55, bloom: false);
  }

  static Paint line(Color col, double w, [double alpha = 1]) => Paint()
    ..style = PaintingStyle.stroke
    ..strokeWidth = w
    ..color = col.withValues(alpha: alpha)
    ..isAntiAlias = true;

  static void seg(Canvas c, Offset a, Offset b, Color col, double alpha, double w, {bool bloom = false}) {
    if (bloom) {
      c.drawLine(
          a,
          b,
          Paint()
            ..style = PaintingStyle.stroke
            ..strokeWidth = w * 4
            ..blendMode = BlendMode.plus
            ..color = col.withValues(alpha: alpha * .13));
    }
    c.drawLine(a, b, line(col, w, alpha));
  }

  static void dashedPath(Canvas c, Path p, Paint paint, double on, double off, [double phase = 0]) {
    for (final m in p.computeMetrics()) {
      var d = phase % (on + off);
      d = -(on + off - d) % (on + off);
      while (d < m.length) {
        final s = math.max(0.0, d);
        final e = math.min(m.length, d + on);
        if (e > s) c.drawPath(m.extractPath(s, e), paint);
        d += on + off;
      }
    }
  }

  static void dashedLine(Canvas c, Offset a, Offset b, Paint paint, double on, double off, [double phase = 0]) {
    dashedPath(c, Path()..moveTo(a.dx, a.dy)..lineTo(b.dx, b.dy), paint, on, off, phase);
  }

  static Path hexPath(Offset center, double r, {double rot = 0}) {
    final p = Path();
    for (var i = 0; i < 6; i++) {
      final a = i * math.pi / 3 + rot;
      final x = center.dx + math.cos(a) * r, y = center.dy + math.sin(a) * r;
      i == 0 ? p.moveTo(x, y) : p.lineTo(x, y);
    }
    return p..close();
  }

  static void hatch(Canvas c, Path clip, Rect bounds, Color col, double alpha, {double gap = 5, double w = 1}) {
    c.save();
    c.clipPath(clip);
    final paint = line(col, w, alpha);
    for (var k = -bounds.height; k < bounds.width; k += gap) {
      c.drawLine(Offset(bounds.left + k, bounds.bottom), Offset(bounds.left + k + bounds.height, bounds.top), paint);
    }
    c.restore();
  }

  static final Map<int, TextPainter> _tpCache = {};

  static TextPainter tp(String s, TextStyle st, {double? maxWidth, TextAlign align = TextAlign.left}) {
    final key = Object.hash(s, st.fontSize, st.color, st.fontWeight, st.fontFamily, st.letterSpacing, maxWidth, align);
    final hit = _tpCache[key];
    if (hit != null) return hit;
    if (_tpCache.length > 600) _tpCache.clear();
    return _tpCache[key] = TextPainter(
      text: TextSpan(text: s, style: st),
      textDirection: TextDirection.ltr,
      textAlign: align,
      maxLines: 1,
    )..layout(maxWidth: maxWidth ?? double.infinity);
  }

  /// Draw text anchored by [anchor]: 0 left, 0.5 centre, 1 right; vertically centred on [y].
  static void text(Canvas c, String s, double x, double y, TextStyle st, {double anchor = 0, double? maxWidth}) {
    final t = tp(s, st, maxWidth: maxWidth);
    t.paint(c, Offset(x - t.width * anchor, y - t.height / 2));
  }

  /// Four-corner reticle brackets with an optional label.
  static void reticle(Canvas c, double x, double y, double r, Color col, {String? label, double alpha = .9}) {
    final p = line(col, 1.3, alpha);
    final k = r * .4;
    for (final s in const [(-1, -1), (1, -1), (1, 1), (-1, 1)]) {
      final path = Path()
        ..moveTo(x + s.$1 * r, y + s.$2 * (r - k))
        ..lineTo(x + s.$1 * r, y + s.$2 * r)
        ..lineTo(x + s.$1 * (r - k), y + s.$2 * r);
      c.drawPath(path, p);
    }
    if (label != null) {
      text(c, label, x + r + 6, y - r + 8, T.mono(size: 9.5, color: col.withValues(alpha: .95), weight: FontWeight.w500));
    }
  }
}
