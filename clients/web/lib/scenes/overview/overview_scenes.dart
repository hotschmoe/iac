import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

import '../../console/intel.dart';
import '../../console/prefs.dart';
import '../../console/threat.dart';
import '../../design/beat.dart';
import '../../design/draw.dart';
import '../../design/symbols.dart';
import '../../design/tokens.dart';
import '../../hex/hex_math.dart';
import '../../models/fleet.dart' as ui;
import '../../protocol/protocol.dart' as proto;

/// Drives a repaint-only clock. Static (time 0) under reduced motion or Quality.low.
class SceneClock extends StatefulWidget {
  final ConsolePrefs prefs;
  final CustomPainter Function(ValueNotifier<double> time, bool still) painter;
  const SceneClock({super.key, required this.prefs, required this.painter});
  @override
  State<SceneClock> createState() => _SceneClockState();
}

class _SceneClockState extends State<SceneClock> with SingleTickerProviderStateMixin {
  final ValueNotifier<double> _t = ValueNotifier(0);
  Ticker? _ticker;

  @override
  void initState() {
    super.initState();
    _ticker = createTicker((d) => _t.value = d.inMicroseconds / 1e6);
  }

  @override
  void dispose() {
    _ticker?.dispose();
    _t.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: widget.prefs,
      builder: (context, _) {
        final still = reducedMotion(context) || widget.prefs.reduceMotion || widget.prefs.quality == Quality.low;
        final t = _ticker!;
        if (still) {
          if (t.isActive) t.stop();
          _t.value = 0;
        } else if (!t.isActive) {
          t.start();
        }
        return RepaintBoundary(child: CustomPaint(painter: widget.painter(_t, still), size: Size.infinite));
      },
    );
  }
}

/// Hero backdrop: wireframe planet, meridians, hatched night side, orbit platforms.
class PlanetPainter extends CustomPainter {
  final ValueNotifier<double> time;
  final bool still;
  final bool bloom;
  PlanetPainter(this.time, this.still, {this.bloom = true}) : super(repaint: time);

  static final List<(double, double, double)> _stars = () {
    final r = mulberry32(11);
    return [for (var i = 0; i < 110; i++) (r(), r(), r() * 6)];
  }();

  @override
  void paint(Canvas c, Size s) {
    final t = still ? 0.0 : time.value;
    final w = s.width, h = s.height;
    for (final st in _stars) {
      final a = .2 + .3 * math.pow(math.sin(t + st.$3), 2);
      c.drawRect(Rect.fromLTWH((st.$1 * w).roundToDouble(), (st.$2 * h).roundToDouble(), 1.2, 1.2), Paint()..color = C.a200.withValues(alpha: a));
    }
    final nar = w < 600;
    final r = nar ? h * .19 : math.min(h * .36, w * .14);
    final x = nar ? w * .8 : w * .76, y = nar ? h * .7 : h * .52;
    final rot = t * .12;
    final grid = Draw.line(C.a500, 1, .10);
    for (var k = 1; k < 12; k++) {
      c.drawLine(Offset(w * k / 12, 0), Offset(w * k / 12, h), grid);
    }
    for (var k = 1; k < 6; k++) {
      c.drawLine(Offset(0, h * k / 6), Offset(w, h * k / 6), grid);
    }
    final ax = Draw.line(C.a500, 1, .22);
    c.drawLine(Offset(x - r * 2.4, y), Offset(x + r * 2.4, y), ax);
    c.drawLine(Offset(x, y - r * 1.7), Offset(x, y + r * 1.7), ax);

    final ctr = Offset(x, y);
    if (bloom) c.drawCircle(ctr, r, Paint()..style = PaintingStyle.stroke..strokeWidth = 7..blendMode = BlendMode.plus..color = C.a500.withValues(alpha: .14));
    c.drawCircle(ctr, r, Draw.line(const Color(0xFFFFD278), 1.6, .95));
    for (var i = 0; i < 6; i++) {
      final a = rot + i * math.pi / 6;
      final rx = math.max(.5, math.cos(a).abs() * r);
      c.drawOval(Rect.fromCenter(center: ctr, width: rx * 2, height: r * 2), Draw.line(C.a500, 1, .18 + .35 * math.sin(a).abs()));
    }
    for (var k = -3; k <= 3; k++) {
      final yy = k * r * .25;
      final rx = math.sqrt(math.max(0, r * r - yy * yy));
      c.drawOval(Rect.fromCenter(center: Offset(x, y + yy), width: rx * 2, height: rx * .32),
          k == 0 ? Draw.line(const Color(0xFFFFD278), 1, .6) : Draw.line(C.a500, 1, .3));
    }
    // night side: hatch clipped to the disc minus the lit ellipse
    c.save();
    final disc = Path()..addOval(Rect.fromCircle(center: ctr, radius: r - 1));
    final lit = Path()..addOval(Rect.fromCenter(center: Offset(x - r * .55, y), width: r * 2.5, height: r * 2.6));
    c.clipPath(Path.combine(PathOperation.difference, disc, lit));
    final hp = Draw.line(C.a500, 1, .22);
    for (var k = -r * 2; k < r * 2; k += 5) {
      c.drawLine(Offset(x + r * .2 + k, y + r), Offset(x + r * .2 + k + r * 2, y - r), hp);
    }
    c.restore();
    for (final o in const [(1.7, .34, -.3, 0xFFFFC864, 3, 0), (2.2, .46, -.3, 0xFF5EE0CF, 2, 1)]) {
      final k = o.$1, e = o.$2, rt = o.$3;
      c.save();
      c.translate(x, y);
      c.rotate(rt);
      c.drawOval(Rect.fromCenter(center: Offset.zero, width: r * k * 2, height: r * k * e * 2), Draw.line(Color(o.$4), 1, .4));
      c.restore();
      for (var i = 0; i < o.$5; i++) {
        final a = t * (.28 - o.$6 * .1) + i * 2 * math.pi / o.$5;
        final ex = r * k * math.cos(a), ey = r * k * e * math.sin(a);
        final px = x + ex * math.cos(rt) - ey * math.sin(rt), py = y + ex * math.sin(rt) + ey * math.cos(rt);
        c.drawRect(Rect.fromCenter(center: Offset(px, py), width: 7, height: 7), Draw.line(o.$6 == 1 ? const Color(0xFFFF8A5C) : C.own, 1.2));
      }
    }
    Draw.text(c, 'HOME / ORBIT SCHEMATIC / NOT TO SCALE', w - 12, h - 12, T.mono(size: 9, color: C.text2.withValues(alpha: .7)), anchor: 1);
  }

  @override
  bool shouldRepaint(PlanetPainter o) => o.still != still;
}

/// Nearby-space PPI scope: range rings, known sectors, threat diamonds, fleets, sweep.
class PpiPainter extends CustomPainter {
  final ValueNotifier<double> time;
  final bool still;
  final proto.Hex home;
  final Map<proto.Hex, proto.SectorState> sectors;
  final List<ui.FleetState> fleets;
  final int tick;
  PpiPainter(this.time, this.still, this.home, this.sectors, this.fleets, this.tick) : super(repaint: time);

  @override
  void paint(Canvas c, Size s) {
    final t = still ? 0.0 : time.value;
    final cx = s.width / 2, cy = s.height / 2 + 14;
    final u = math.max(8.0, math.min(14.0, math.min(s.width, s.height) / 20));
    final rad = math.min(s.width, s.height) / 2 - 18;
    final ctr = Offset(cx, cy);
    Offset p(proto.Hex h) {
      final o = hexToPixel(h.q - home.q, h.r - home.r, u);
      return Offset(cx + o.dx, cy + o.dy);
    }

    c.drawRect(Offset.zero & s, Paint()..color = const Color(0xB3050403));
    final ring = Draw.line(C.a500, 1, .18);
    for (var k = 1; k <= 3; k++) {
      c.drawCircle(ctr, rad * k / 3, ring);
    }
    c.drawLine(Offset(cx - rad, cy), Offset(cx + rad, cy), ring);
    c.drawLine(Offset(cx, cy - rad), Offset(cx, cy + rad), ring);
    final tk = Draw.line(C.a500, 1, .3);
    for (var i = 0; i < 72; i++) {
      final a = i / 72 * 2 * math.pi, l = i % 6 == 0 ? 6.0 : 3.0;
      c.drawLine(ctr + Offset(math.cos(a), math.sin(a)) * rad, ctr + Offset(math.cos(a), math.sin(a)) * (rad - l), tk);
    }
    c.save();
    c.clipPath(Path()..addOval(Rect.fromCircle(center: ctr, radius: rad)));
    for (final e in sectors.entries) {
      if (proto.Hex.distance(e.key, home) > 5) continue;
      final it = Intel.of(e.value, tick);
      if (!it.known) continue;
      final o = p(e.key);
      final th = SectorThreat.of(e.value);
      if (th.hostile) {
        final col = C.threat(th.rating);
        c.save();
        c.translate(o.dx, o.dy);
        c.rotate(math.pi / 4);
        c.drawRect(Rect.fromCenter(center: Offset.zero, width: 6.4, height: 6.4), Draw.line(col, 1.1, .35 + .6 * it.strength));
        c.restore();
        Draw.text(c, '${th.rating}', o.dx + 6, o.dy, T.mono(size: 8, color: col.withValues(alpha: .5 + .5 * it.strength), weight: FontWeight.w600));
      } else {
        c.drawRect(Rect.fromCenter(center: Offset(o.dx.roundToDouble(), o.dy.roundToDouble()), width: 2, height: 2), Paint()..color = C.a500.withValues(alpha: .2 + .5 * it.strength));
      }
      if (e.value.site != null) Draw.symbol(c, Sym.derelict, o.dx, o.dy - 5, 4, color: C.rare, lw: 1, bloom: false);
      final sv = e.value.salvage;
      if (sv != null && sv.total > 0) Draw.symbol(c, Sym.salvage, o.dx + 5, o.dy + 5, 4, color: C.gold, lw: 1, bloom: false);
    }
    for (final f in fleets) {
      final o = p(f.sector);
      Draw.symbol(c, Sym.fleet, o.dx, o.dy, 7, color: C.own, bloom: false);
    }
    final hp = p(home);
    final hpaint = Draw.line(C.a500, 1.3);
    c.drawCircle(hp, 6, hpaint);
    c.drawLine(hp - const Offset(9, 0), hp + const Offset(9, 0), hpaint);
    for (var k = 0; k < 26; k++) {
      final a = t * 1.1 - k * .04;
      c.drawLine(hp, hp + Offset(math.cos(a), math.sin(a)) * rad * 1.6, Draw.line(C.own, 1, (1 - k / 26) * .3));
    }
    c.restore();
    c.drawCircle(ctr, rad, Draw.line(C.a500, 1.2, .5));
    final lb = T.mono(size: 8.5, color: C.text2.withValues(alpha: .7));
    Draw.text(c, 'R01', cx + rad / 3 + 3, cy - 6, lb);
    Draw.text(c, 'R03', cx + rad * 2 / 3 + 3, cy - 6, lb);
  }

  @override
  bool shouldRepaint(PpiPainter o) => o.tick != tick || o.still != still || o.sectors != sectors || o.fleets != fleets;
}
