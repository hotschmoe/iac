import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';

import '../../console/intel.dart';
import '../../console/threat.dart';
import '../../design/draw.dart';
import '../../design/symbols.dart';
import '../../design/tokens.dart';
import '../../hex/hex_math.dart' as hm;
import '../../models/fleet.dart' as ui_models;
import '../../protocol/protocol.dart' as proto;
import 'map_state.dart';

class RouteDraw {
  final proto.Hex from;
  final List<proto.Hex> path;
  final List<proto.Hex> waypoints;
  final List<int> holds;
  final bool active;
  const RouteDraw(this.from, this.path, this.waypoints, this.holds, {this.active = false});
}

/// Cache of the pan-invariant base layer (graticule, zones, lanes, hex
/// outlines), recorded in screen space and re-used while only panning.
class MapStaticCache {
  ui.Picture? pic;
  int key = 0;
  double vx = 0, vy = 0, vs = 0;
  Size size = Size.zero;
  int records = 0;
  int reuses = 0;

  void dispose() => pic?.dispose();
}

class MapPainter extends CustomPainter {
  final MapData data;
  final MapViewport vp;
  final MapLayers layers;
  final MapPlan plan;
  final ValueNotifier<double> clock;
  final MapStaticCache cache;
  final RouteDraw? route;
  final RouteDraw? preview;
  final bool still;
  final bool lowQuality;

  MapPainter({
    required this.data,
    required this.vp,
    required this.layers,
    required this.plan,
    required this.clock,
    required this.cache,
    required this.route,
    required this.preview,
    required this.still,
    required this.lowQuality,
  }) : super(repaint: Listenable.merge([vp, layers, plan, clock]));

  static ui.Image? _hatch;
  static ui.Image _hatchTile() {
    return _hatch ??= () {
      final rec = ui.PictureRecorder();
      final c = Canvas(rec);
      final p = Paint()
        ..color = const Color(0xE607050A)
        ..strokeWidth = 1.2
        ..style = PaintingStyle.stroke;
      c.drawLine(const Offset(-1, 9), const Offset(9, -1), p);
      c.drawLine(const Offset(-1, 5), const Offset(5, -1), p);
      c.drawLine(const Offset(3, 9), const Offset(9, 3), p);
      return rec.endRecording().toImageSync(8, 8);
    }();
  }

  double get s => vp.s;
  double get hr => mapU * s * .96;
  double get rn => hr * .5;

  Color _base(double f) => Color.lerp(C.fossil, C.a500, f)!;

  Intel _intel(proto.Hex h) => layers.age ? Intel.of(data.sectors[h], data.tick) : (data.sectors[h] == null ? Intel.unknown : const Intel(IntelAge.live, 0));
  double _f(Intel i) => layers.age ? i.strength : 1;

  @override
  void paint(Canvas canvas, Size size) {
    vp.resize(size);
    canvas.save();
    canvas.clipRect(Offset.zero & size);
    _drawStatic(canvas, size);
    if (layers.threat) canvas.drawRect(Offset.zero & size, Paint()..color = const Color(0x66060408));
    _drawNodes(canvas, size);
    _drawHomeHub(canvas);
    final t = clock.value;
    if (preview != null && route == null) _drawPreview(canvas, preview!, t);
    if (route != null) _drawRoute(canvas, route!, t);
    _drawBrackets(canvas, t);
    _drawChases(canvas, t);
    _drawFleets(canvas, t);
    canvas.restore();
  }

  /// A lane a group or a fleeing fleet is jumping down, from [from] to [to]:
  /// a dashed stroke that runs toward its end, an arrowhead there and, close
  /// in, a word for what it is.
  void _lane(Canvas c, proto.Hex from, proto.Hex to, Color col, String word, double t) {
    final a = vp.toScreen(from.q, from.r), b = vp.toScreen(to.q, to.r);
    final along = b - a;
    if (along.distance < 1) return;
    final unit = along / along.distance, across = Offset(-unit.dy, unit.dx);
    final tip = b - unit * rn * .55, tail = a + unit * rn * .55;
    c.drawLine(tail, tip, Draw.line(Colors.black, 6.5, .8));
    Draw.dashedLine(c, tail, tip, Draw.line(col, 3), 5, 3, still ? 0 : -t * 30);
    final barb = math.max(7.0, hr * .36);
    c.drawPath(
        Path()
          ..moveTo(tip.dx, tip.dy)
          ..lineTo(tip.dx - unit.dx * barb + across.dx * barb * .6, tip.dy - unit.dy * barb + across.dy * barb * .6)
          ..lineTo(tip.dx - unit.dx * barb - across.dx * barb * .6, tip.dy - unit.dy * barb - across.dy * barb * .6)
          ..close(),
        Paint()..color = col);
    if (s >= .9) {
      final m = (a + b) / 2 - Offset(0, hr * 1.2);
      final size = math.max(9.0, (hr * .3).roundToDouble());
      final plate = Rect.fromCenter(center: m, width: word.length * size * 1.25 + 8, height: size + 5);
      c.drawRect(plate, Paint()..color = const Color(0xE607050A));
      c.drawRect(plate, Draw.line(col, 1, .8));
      Draw.text(c, word.split('').join(' '), m.dx, m.dy, T.cond(size: size, color: col, spacing: 0), anchor: .5);
    }
  }

  /// Pursuit and flight as the state tells them: the lane a group on an
  /// errand is jumping down, the lane of the group on a fleet's trail, and
  /// the lane a fleet is leaving a fight by.
  void _drawChases(Canvas c, double t) {
    final trailed = <int>{};
    for (final f in data.fleets) {
      if (f.pursuedBy case final hunter?) {
        trailed.add(hunter.groupId);
        _lane(c, hunter.from, f.sector, C.ember, 'PURSUIT', t);
      }
      if (f.fleeing case final leaving?) _lane(c, f.sector, leaving.to, C.a300, 'FLEEING', t);
    }
    for (final sec in data.sectors.values) {
      for (final g in sec.hostiles ?? const <proto.NpcFleetInfo>[]) {
        final errand = g.errand;
        final to = errand?.to;
        if (errand == null || to == null || trailed.contains(g.id)) continue;
        _lane(c, sec.location, to, errand.quarry == null ? C.threat(4) : C.ember, errand.quarry == null ? 'RETURNING' : 'PURSUIT', t);
      }
    }
  }

  Rect _viewRect(Size size, [double m = 60]) => Rect.fromLTWH(-m, -m, size.width + 2 * m, size.height + 2 * m);

  // ---- static layer ----

  void _drawStatic(Canvas canvas, Size size) {
    final key = Object.hash(data.staticKey, layers.version, s, size.width, size.height, lowQuality);
    final c = cache;
    final dx = -(vp.x - c.vx) * s, dy = -(vp.y - c.vy) * s;
    final valid = c.pic != null && c.key == key && c.vs == s && c.size == size && dx.abs() < 180 && dy.abs() < 180;
    if (!valid) {
      c.pic?.dispose();
      final rec = ui.PictureRecorder();
      final rc = Canvas(rec);
      _recordStatic(rc, size);
      c.pic = rec.endRecording();
      c.key = key;
      c.vx = vp.x;
      c.vy = vp.y;
      c.vs = s;
      c.size = size;
      c.records++;
      canvas.drawPicture(c.pic!);
    } else {
      c.reuses++;
      canvas.save();
      canvas.translate(dx, dy);
      canvas.drawPicture(c.pic!);
      canvas.restore();
    }
  }

  void _recordStatic(Canvas c, Size size) {
    final vr = _viewRect(size, 240);
    _registration(c, size);
    if (layers.zones) _zones(c);
    final seen = <proto.Hex>{};
    final hexLine = Paint()..style = PaintingStyle.stroke..strokeWidth = 1;
    final threatFill = Paint();
    final lane = Paint()..style = PaintingStyle.stroke;
    for (final e in data.sectors.entries) {
      final h = e.key;
      final p = vp.toScreen(h.q, h.r);
      if (!vr.contains(p)) continue;
      final it = _intel(h);
      final f = _f(it);
      final th = SectorThreat.of(e.value);
      if (s >= .7) {
        hexLine.color = _base(f).withValues(alpha: .05 + .05 * f);
        c.drawPath(Draw.hexPath(p, hr * .98), hexLine);
      }
      if (layers.threat) {
        final col = C.threat(th.rating);
        final path = Draw.hexPath(p, hr * .98);
        if (th.rating > 0) {
          threatFill.color = col.withValues(alpha: ((.2 + .055 * th.rating) * (.4 + .6 * f) + (it.live ? .06 : 0)).clamp(0, 1));
          c.drawPath(path, threatFill);
          if (th.rating >= 4) {
            c.save();
            c.clipPath(path);
            final hp = Draw.line(col, 1, .28);
            final gap = math.max(4, 9 - th.rating).toDouble();
            for (var k = -hr * 2; k < hr * 2; k += gap) {
              c.drawLine(Offset(p.dx + k, p.dy + hr), Offset(p.dx + k + hr * 2, p.dy - hr), hp);
            }
            c.restore();
          }
          for (var k = 0; k < (th.rating - 1) ~/ 3; k++) {
            c.drawPath(Draw.hexPath(p, hr * (1.12 + k * .16)), Draw.line(col, 1, .3 + .3 * f));
          }
        } else {
          threatFill.color = col.withValues(alpha: .05 + .05 * f);
          c.drawPath(path, threatFill);
        }
      }
      if (layers.lanes) {
        for (final n in e.value.connections) {
          if (n.q < h.q || (n.q == h.q && n.r < h.r)) continue;
          final ns = data.sectors[n];
          final ni = _intel(n);
          final kn = ns != null;
          final fl = math.min(f, kn ? _f(ni) : .25);
          final np = vp.toScreen(n.q, n.r);
          final d = np - p;
          final len = d.distance == 0 ? 1.0 : d.distance;
          final u = d / len;
          final a = p + u * rn, b = np - u * rn * (kn ? 1 : .4);
          lane
            ..color = Color.lerp(C.fossil, const Color(0xFFFFC450), fl)!.withValues(alpha: .22 + .55 * fl)
            ..strokeWidth = math.max(1, s * 1.1);
          final dashed = layers.age && (!kn || it.age == IntelAge.stale || ni.age == IntelAge.stale);
          if (dashed) {
            Draw.dashedLine(c, a, b, lane, 4, 4);
          } else {
            c.drawLine(a, b, lane);
          }
          if (!kn) seen.add(n);
        }
      }
    }
    if (s > .8) {
      final fr = Draw.line(C.rare, 1, .5);
      for (final n in seen) {
        final p = vp.toScreen(n.q, n.r);
        if (!vr.contains(p)) continue;
        Draw.dashedPath(c, Draw.hexPath(p, rn * .75), fr, 2, 3);
        if (s > 1.1) Draw.text(c, '?', p.dx, p.dy, T.mono(size: 11, color: C.a(C.rare, .7), weight: FontWeight.w500), anchor: .5);
      }
    }
    for (final e in data.signals.entries) {
      final p = vp.toScreen(e.key.q, e.key.r);
      if (!vr.contains(p)) continue;
      final col = switch (e.value) {
        proto.SignalKind.hostileMass => C.ember,
        proto.SignalKind.richOre => C.metal,
        _ => C.rare,
      };
      Draw.dashedPath(c, Draw.hexPath(p, rn * .75), Draw.line(col, 1, .7), 2, 3);
      if (s > .6) Draw.text(c, '?', p.dx, p.dy, T.mono(size: 11, color: col, weight: FontWeight.w500), anchor: .5);
    }
  }

  void _registration(Canvas c, Size size) {
    final step = 4 * mapU * s * 1.5;
    if (step <= 28) return;
    final ox = (((size.width / 2 - vp.x * s) % step) + step) % step, oy = (((size.height / 2 - vp.y * s) % step) + step) % step;
    final p = Path();
    for (var x = ox - step; x < size.width + step; x += step) {
      for (var y = oy - step; y < size.height + step; y += step) {
        p..moveTo(x - 3, y)..lineTo(x + 3, y)..moveTo(x, y - 3)..lineTo(x, y + 3);
      }
    }
    c.drawPath(p, Draw.line(C.a500, 1, .10));
  }

  Path _ring(int d) {
    final pts = const [(1, -1), (1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1)];
    final p = Path();
    for (var i = 0; i < 6; i++) {
      final o = vp.toScreen(pts[i].$1 * d, pts[i].$2 * d);
      i == 0 ? p.moveTo(o.dx, o.dy) : p.lineTo(o.dx, o.dy);
    }
    return p..close();
  }

  void _zones(Canvas c) {
    final h = vp.toScreen(0, 0);
    for (final d in const [(0, -1), (1, -1), (1, 0), (0, 1), (-1, 1), (-1, 0)]) {
      final e = vp.toScreen(d.$1 * mapRMax, d.$2 * mapRMax);
      Draw.dashedLine(c, h, e, Draw.line(C.a500, 1, .10), 2, 6);
      if (s > .5) {
        final n = Offset(-(e.dy - h.dy), e.dx - h.dx);
        final len = n.distance == 0 ? 1.0 : n.distance;
        final tp = Draw.line(C.a500, 1, .3);
        for (var k = 4; k <= mapRMax; k += 4) {
          final t = vp.toScreen(d.$1 * k, d.$2 * k);
          c.drawLine(t - n / len * 3, t + n / len * 3, tp);
        }
      }
    }
    const rings = [
      (proto.zoneInnerRingRadius, proto.WorldZone.innerRing, C.a500),
      (proto.zoneOuterRingRadius, proto.WorldZone.outerRing, C.ember),
    ];
    for (final (radius, _, color) in rings) {
      Draw.dashedPath(c, _ring(radius), Draw.line(color, 1.2, .5), 8, 6);
    }
    if (s > .5) {
      for (final (radius, zone, color) in rings) {
        final p = vp.toScreen(0, -radius);
        Draw.text(c, 'R${'$radius'.padLeft(2, '0')}  ${zone.label.toUpperCase()}', p.dx, p.dy - 8,
            T.cond(size: 10, color: color.withValues(alpha: .85), spacing: 2.4), anchor: .5);
      }
      final p = vp.toScreen(0, -mapRMax);
      Draw.text(c, 'R${proto.zoneOuterRingRadius + 1}+  ${proto.WorldZone.wandering.label.toUpperCase()}', p.dx, p.dy - 10,
          T.cond(size: 10, color: C.a(C.rare, .7), spacing: 2.4), anchor: .5);
    }
  }

  // ---- live layer ----

  void _drawNodes(Canvas c, Size size) {
    final vr = _viewRect(size);
    final t = clock.value;
    final near = s >= 1.0;
    final fill = Paint();
    final stroke = Paint()..style = PaintingStyle.stroke;
    final hatch = Paint()..shader = ui.ImageShader(_hatchTile(), TileMode.repeated, TileMode.repeated, Matrix4.identity().storage);
    for (final e in data.sectors.entries) {
      final h = e.key;
      final p = vp.toScreen(h.q, h.r);
      if (!vr.contains(p)) continue;
      final sec = e.value;
      final it = _intel(h);
      final f = _f(it);
      final live = layers.age && it.live;
      final stale = layers.age && it.age == IntelAge.stale, aging = layers.age && it.age == IntelAge.aging;
      final th = SectorThreat.of(sec);
      final base = _base(f);
      final tc = th.rating > 0 ? C.threat(th.rating) : null;
      final hex = Draw.hexPath(p, rn);
      fill.color = tc != null ? tc.withValues(alpha: .07 + .02 * th.rating) : const Color(0xF20E0A06);
      c.drawPath(hex, fill);
      if (tc == null) {
        fill.color = base.withValues(alpha: .04 + .08 * f);
        c.drawPath(hex, fill);
      }
      if (stale || aging) {
        hatch.color = Colors.white.withValues(alpha: stale ? .9 : .4);
        c.drawPath(hex, hatch);
      }
      final col = tc ?? (live ? C.own : base);
      stroke
        ..color = col.withValues(alpha: (.4 + .55 * f).clamp(0, 1))
        ..strokeWidth = (live ? 1.5 : 1.2) + (th.rating >= 5 ? .6 : 0);
      if (stale) {
        Draw.dashedPath(c, hex, stroke, 3, 3);
      } else {
        c.drawPath(hex, stroke);
      }
      final gas = sec.terrain == proto.TerrainType.nebula ? sec.resources.deuterium.index : 0;
      if (gas >= proto.Density.rich.index && s >= .5) {
        final glow = C.deut.withValues(alpha: math.max(.3, f) * (.05 + .05 * gas));
        c.drawCircle(p, rn * (.3 + .14 * gas), Paint()..color = glow);
        c.drawCircle(
            p,
            rn * (.3 + .14 * gas),
            Paint()
              ..style = PaintingStyle.stroke
              ..strokeWidth = 1
              ..color = C.deut.withValues(alpha: math.max(.3, f) * .5));
      }
      if (!lowQuality && (live || th.rating >= 5)) {
        c.drawPath(
            hex,
            Paint()
              ..style = PaintingStyle.stroke
              ..strokeWidth = 4.5
              ..blendMode = BlendMode.plus
              ..color = col.withValues(alpha: .12));
      }
      if (th.rating >= 5 && !stale && s >= .55) {
        final on = still || ((t * 2 + h.q).floor() % 2 == 0);
        c.drawPath(Draw.hexPath(p, rn * 1.4), Draw.line(tc!, 1, on ? .6 : .2));
      }
      if (data.relays.containsKey(h.toKey()) && s >= .4) {
        final size = math.max(6.0, hr * .34);
        final seen = live || !layers.age ? 1.0 : math.max(.45, f);
        Draw.symbol(c, Sym.relay, p.dx, p.dy, size, color: C.own, alpha: seen, lw: 1.2, bloom: false);
      }
      if (sec.outpost case final post? when s >= .4) {
        final size = math.max(7.0, hr * .42);
        final seen = live || !layers.age ? 1.0 : math.max(.45, f);
        final station = post.breached ? C.ember : post.mine ? C.own : C.a200;
        if (post.mine) c.drawRect(Rect.fromCenter(center: p, width: size * .96, height: size * .96), Paint()..color = station.withValues(alpha: seen * .5));
        Draw.symbol(c, Sym.outpost, p.dx, p.dy, size, color: station, alpha: seen, lw: 1.3, bloom: false);
      } else if (th.rating > 0 && s >= .55) {
        if (s >= .8) {
          c.save();
          c.translate(p.dx, p.dy);
          c.rotate(math.pi / 4);
          final dq = rn * .62;
          c.drawRect(Rect.fromLTRB(-dq, -dq, dq, dq), Draw.line(tc!, 1, .5 + .4 * f));
          c.restore();
        }
        Draw.text(c, '${th.rating}', p.dx, p.dy + 1,
            T.cond(size: math.max(9, (rn * .9).roundToDouble()), color: f > .5 ? const Color(0xFFFFF3E8) : const Color(0xFFE6D5B8), weight: FontWeight.w700, spacing: 0),
            anchor: .5);
      } else if (sec.terrain == proto.TerrainType.anomaly && s >= .7) {
        final a = rn * .5;
        final pa = Draw.line(C.rare, 1, .85);
        c.drawLine(p - Offset(a, 0), p + Offset(a, 0), pa);
        c.drawLine(p - Offset(0, a), p + Offset(0, a), pa);
        c.drawLine(p - Offset(a * .7, a * .7), p + Offset(a * .7, a * .7), pa);
        c.drawLine(p + Offset(a * .7, -a * .7), p + Offset(-a * .7, a * .7), pa);
      } else if (s >= .7) {
        c.drawRect(Rect.fromCenter(center: p, width: 2, height: 2), Paint()..color = col.withValues(alpha: .4 + .4 * f));
      }
      if (near || layers.ore) {
        final oy = p.dy + rn * 1.35;
        final ks = [(sec.resources.metal, C.metal), (sec.resources.crystal, C.crystal), (sec.resources.deuterium, C.deut)];
        for (var i = 0; i < 3; i++) {
          final dn = ks[i].$1.index;
          if (dn == 0) continue;
          final bw = math.max(3.0, hr * .15), bx = p.dx + (i - 1) * bw * 1.6;
          final bh = 2 + dn * (layers.ore ? hr * .1 : hr * .065);
          c.drawLine(Offset(bx, oy + rn * .4), Offset(bx, oy + rn * .4 - bh), Draw.line(ks[i].$2, 1.5, math.max(.35, f)));
        }
      }
      if (near) {
        if (salvagePinVisible(sec, data.tick)) {
          Draw.symbol(c, Sym.salvage, p.dx - rn * 1.2, p.dy - rn * .75, math.max(6, hr * .3), color: const Color(0xFFFFBE3C), alpha: sec.pinsStale ? .35 : math.max(.5, f), bloom: false);
        }
        if (sec.site != null && layers.derelict) {
          Draw.symbol(c, Sym.derelict, p.dx + rn * 1.3, p.dy - rn * .6, math.max(8, hr * .4), color: C.rare, alpha: sec.pinsStale ? .35 : math.max(.6, f), lw: 1.1, bloom: false);
        }
        if (s > 1.6 && it.ticksOld > 6 && layers.age) {
          Draw.text(c, ago(it.ticksOld), p.dx, p.dy - rn * 1.25,
              T.mono(size: (hr * .26).roundToDouble(), color: stale ? const Color(0xFFC9B284) : const Color(0x80FFE2AA), weight: FontWeight.w400), anchor: .5);
        }
      } else {
        if (sec.site != null && layers.derelict && s >= .62) {
          c.drawRect(Rect.fromLTWH(p.dx + rn, p.dy - rn * 1.1, 4, 4), Draw.line(C.rare, 1, .95));
        }
        if (salvagePinVisible(sec, data.tick) && s >= .62) {
          c.drawRect(Rect.fromLTWH(p.dx - rn * 1.4, p.dy - rn * 1.1, 4, 4), Paint()..color = const Color(0xF2FFBE3C));
        }
      }
    }
  }

  void _drawHomeHub(Canvas c) {
    final hp = vp.toScreen(data.home.q, data.home.r);
    final r = math.max(10.0, hr * .62);
    Draw.symbol(c, Sym.home, hp.dx, hp.dy, r * 1.55, color: const Color(0xFFFFC864), lw: 1.3);
    Draw.text(c, 'HOME', hp.dx, hp.dy + r * 1.7, T.cond(size: math.max(9, (hr * .34).roundToDouble()), color: C.own, spacing: 1), anchor: .5);
    for (final site in data.hub) {
      final o = vp.toScreen(site.location.q, site.location.r);
      Draw.symbol(c, Sym.hub, o.dx, o.dy, math.max(10, hr * .9), color: C.a200, alpha: .9, lw: 1.2);
      if (s > .5) {
        final offers = site.services.contains(proto.HubService.exchange) ? 'H U B   E X C H A N G E' : 'H U B';
        Draw.text(c, offers, o.dx, o.dy - hr * 1.3, T.cond(size: 10, color: C.a(C.a200, .8), spacing: 1), anchor: .5);
      }
    }
  }

  // ---- routes ----

  Color _hopColor(int t) => t > 0 ? C.threat(t) : const Color(0xFFFFD27A);

  void _drawPreview(Canvas c, RouteDraw r, double t) {
    final pts = [r.from, ...r.path];
    final path = Path();
    for (var i = 0; i < pts.length; i++) {
      final p = vp.toScreen(pts[i].q, pts[i].r);
      i == 0 ? path.moveTo(p.dx, p.dy) : path.lineTo(p.dx, p.dy);
    }
    c.drawPath(path, Draw.line(Colors.black, 5, .7));
    Draw.dashedPath(c, path, Draw.line(const Color(0xFFFFECBE), 1.8, .95), 6, 6, -t * 26);
    if (r.path.isNotEmpty) _endMark(c, r.path.last, C.a(C.a200, .9));
  }

  void _endMark(Canvas c, proto.Hex h, Color col) {
    final p = vp.toScreen(h.q, h.r);
    final rr = mapU * s * .6;
    final pa = Draw.line(col, 1.4);
    c.drawCircle(p, rr, pa);
    for (final d in const [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)]) {
      c.drawLine(p + Offset(d.$1 * rr * .6, d.$2 * rr * .6), p + Offset(d.$1 * rr * 1.5, d.$2 * rr * 1.5), pa);
    }
  }

  void _drawRoute(Canvas c, RouteDraw r, double t) {
    var prev = r.from;
    for (var i = 0; i < r.path.length; i++) {
      final p = r.path[i];
      final th = SectorThreat.of(data.sectors[p]).rating;
      final col = _hopColor(th);
      final a = vp.toScreen(prev.q, prev.r), b = vp.toScreen(p.q, p.r);
      c.drawLine(a, b, Draw.line(Colors.black, 6, .75));
      Draw.dashedLine(c, a, b, Draw.line(col, 2.4), 7, 5, -t * 28);
      if (s >= .9) {
        final m = (a + b) / 2;
        final box = Rect.fromCenter(center: m, width: 12, height: 12);
        c.drawRect(box, Paint()..color = const Color(0xE607050A));
        c.drawRect(box, Draw.line(col, 1));
        Draw.text(c, '${i + 1}', m.dx, m.dy, T.mono(size: 8.5, color: col, weight: FontWeight.w500), anchor: .5);
      }
      prev = p;
    }
    for (var i = 0; i < r.waypoints.length; i++) {
      final p = vp.toScreen(r.waypoints[i].q, r.waypoints[i].r);
      final fp = Draw.line(C.a200, 1.4);
      c.drawLine(p, p - const Offset(0, 24), fp);
      final flag = Rect.fromLTWH(p.dx, p.dy - 30, 15, 12);
      c.drawRect(flag, Paint()..color = C.void_);
      c.drawRect(flag, fp);
      Draw.text(c, '${i + 1}', p.dx + 7.5, p.dy - 24, T.mono(size: 9, color: C.a200, weight: FontWeight.w500), anchor: .5);
    }
    for (final ix in r.holds) {
      if (ix >= r.path.length) continue;
      final p = vp.toScreen(r.path[ix].q, r.path[ix].r);
      final rr = math.max(9.0, mapU * s * .55) * 1.3;
      final dp = Path();
      for (var k = 0; k < 4; k++) {
        final a = k * math.pi / 2 + math.pi / 4;
        final q = p + Offset(math.cos(a), math.sin(a)) * rr;
        k == 0 ? dp.moveTo(q.dx, q.dy) : dp.lineTo(q.dx, q.dy);
      }
      dp.close();
      c.drawPath(dp, Draw.line(C.a100, 1.4));
      Draw.text(c, 'H O L D', p.dx, p.dy - rr * 1.2, T.cond(size: 9, color: C.a100, spacing: 1), anchor: .5);
    }
    if (r.path.isNotEmpty) _endMark(c, r.path.last, C.a(C.a200, .9));
  }

  void _drawBrackets(Canvas c, double t) {
    void one(proto.Hex? h, double w) {
      if (h == null) return;
      final p = vp.toScreen(h.q, h.r);
      final rr = hr * 1.02;
      final paint = Draw.line(C.a200, w, .95);
      for (var i = 0; i < 6; i++) {
        final a0 = i * math.pi / 3, a1 = (i + 1) * math.pi / 3;
        final p0 = p + Offset(math.cos(a0), math.sin(a0)) * rr, p1 = p + Offset(math.cos(a1), math.sin(a1)) * rr;
        c.drawLine(p0, p0 + (p1 - p0) * .4, paint);
        c.drawLine(p1, p1 - (p1 - p0) * .4, paint);
      }
    }

    one(plan.selected, 1.8);
    one(plan.hover, 1);
  }

  // ---- fleets ----

  Offset _fleetWorld(ui_models.FleetState f, double t) {
    final m = data.moves[f.id];
    if (m != null) {
      final p = ((t - m.t0) / 1.0).clamp(0.0, 1.0);
      if (p < 1) {
        final e = p < .5 ? 2 * p * p : 1 - math.pow(-2 * p + 2, 2) / 2;
        final a = hm.hexToPixel(m.from.q, m.from.r, mapU), b = hm.hexToPixel(m.to.q, m.to.r, mapU);
        return a + (b - a) * e;
      }
    }
    return hm.hexToPixel(f.sector.q, f.sector.r, mapU);
  }

  void _drawFleets(Canvas c, double t) {
    final seenAt = <proto.Hex, int>{};
    for (final f in data.fleets) {
      final idx = seenAt[f.sector] = (seenAt[f.sector] ?? -1) + 1;
      final sel = f.id == data.selectedFleet;
      final col = sel ? C.own : const Color(0xFF5EA096);
      var ang = 0.0;
      final head = data.headings[f.id];
      if (head != null) {
        final a = vp.toScreen(f.sector.q, f.sector.r), b = vp.toScreen(head.q, head.r);
        ang = math.atan2(b.dy - a.dy, b.dx - a.dx) + math.pi / 2;
      }
      final r = math.max(8.0, hr * .5);
      var p = vp.worldToScreen(_fleetWorld(f, t));
      final atHome = f.sector == data.home;
      p += Offset(atHome ? hr * .8 : idx * hr * .45, atHome ? -hr * .32 : 0);
      if (sel) {
        final k = r * 1.9, l = r * .7;
        final pa = Draw.line(col, 1.2, still ? .8 : .6 + .3 * math.sin(t * 4));
        for (final sg in const [(-1, -1), (1, -1), (1, 1), (-1, 1)]) {
          c.drawPath(
              Path()
                ..moveTo(p.dx + sg.$1 * k, p.dy + sg.$2 * (k - l))
                ..lineTo(p.dx + sg.$1 * k, p.dy + sg.$2 * k)
                ..lineTo(p.dx + sg.$1 * (k - l), p.dy + sg.$2 * k),
              pa);
        }
      }
      if (f.pursuedBy != null) {
        c.drawCircle(p, r * 2.3, Draw.line(C.ember, 1.6, still || (t * 2).floor().isEven ? .95 : .35));
      }
      Draw.symbol(c, Sym.fleet, p.dx, p.dy, r * 1.25, rot: ang, color: col, lw: 1.5, bloom: !lowQuality);
      if (s >= .7 && (sel || s > 1.6)) {
        Draw.text(c, f.name.toUpperCase().split('').join(' '), p.dx, p.dy - r * 2.3, T.cond(size: math.max(9, (hr * .32).roundToDouble()), color: col, spacing: 0), anchor: .5);
      }
    }
  }

  @override
  bool shouldRepaint(MapPainter o) =>
      o.data != data || o.route != route || o.preview != preview || o.still != still || o.lowQuality != lowQuality;
}
