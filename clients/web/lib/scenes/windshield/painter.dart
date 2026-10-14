import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../console/prefs.dart';
import '../../design/draw.dart';
import '../../design/symbols.dart';
import '../../design/tokens.dart';
import '../../protocol/protocol.dart' as proto;
import '../battle/field.dart';
import '../battle/field_painter.dart';
import 'geometry.dart';
import 'sim.dart';

class GateInfo {
  final Dir dir;
  final Color color;
  const GateInfo(this.dir, this.color);
}

const _amber = Color(0xFFFFB000);
const _ember = Color(0xFFFF5A3C);
const _own = Color(0xFF5EE0CF);
const _gold = Color(0xFFFFCC66);
const _violet = Color(0xFFB793FF);

/// Draws the whole sector scope. Repaints every frame off [repaint]; all
/// state lives in [WindshieldSim].
class ArenaPainter extends CustomPainter {
  final WindshieldSim sim;
  final List<GateInfo?> gates;
  final List<bool> lanes;
  final Quality quality;
  final bool reduced;
  ArenaPainter(this.sim, this.gates, this.lanes, this.quality, this.reduced, Listenable repaint) : super(repaint: repaint);

  late Geo g;
  late Canvas c;
  bool get bloom => quality != Quality.low;
  bool get glow => quality == Quality.high;

  @override
  void paint(Canvas canvas, Size size) {
    c = canvas;
    g = Geo(size);
    sim.layout(g);
    final t = sim.time;
    canvas.save();
    canvas.clipRect(Offset.zero & size);
    final shake = sim.field.shake;
    if (shake > 0) canvas.translate((sim.rng.nextDouble() - .5) * shake, (sim.rng.nextDouble() - .5) * shake);
    _stars(size, t);
    if (sim.loc == null) {
      canvas.restore();
      return;
    }
    _arena(t);
    if (sim.atHome) {
      final raided = sim.combat;
      if (raided) {
        canvas.saveLayer(Rect.fromCircle(center: Offset(g.cx - g.rh * .1, g.cy - g.rh * .12), radius: g.rh * .62), Paint()..color = const Color(0x52FFFFFF));
      }
      _planet(t);
      if (raided) canvas.restore();
    }
    if (sim.atExchange) _exchange(t);
    _terrain(t);
    for (final a in sim.ast) {
      _asteroid(a, t);
    }
    if (sim.outpost != null) _outpost(sim.outpost!);
    _salvage(t);
    _derelict(t);
    _pursuit(t);
    _flight(t);
    _beams(t);
    FieldPainter(canvas, sim.field, Offset(g.cx, g.cy), t: t, bloom: bloom, hover: sim.hoverMark).paint();
    _hostileReticle();
    _stamp(t);
    _scan(t);
    canvas.restore();
  }

  Offset wp(double x, double y) => g.wp(x, y);

  void _stars(Size size, double t) {
    final wv = sim.warpAmount();
    final p = Paint();
    for (var li = 0; li < sim.stars.length; li++) {
      final col = Color.fromRGBO(255, 226 - li * 20, 170 - li * 30, 1);
      for (final st in sim.stars[li]) {
        final x = st.x * size.width, y = st.y * size.height;
        final a = (.18 + li * .2) * (.6 + .4 * math.sin(t * 2 + st.tw));
        if (wv > .05) {
          final len = wv * (90 + li * 70);
          c.drawLine(Offset(x, y), Offset(x - sim.drift0 * len, y - sim.drift1 * len + .01), Draw.line(col, .9, math.min(1.0, a + wv * .5)));
        } else {
          p.color = col.withValues(alpha: a);
          c.drawRect(Rect.fromLTWH(x.roundToDouble(), y.roundToDouble(), 1.2, 1.2), p);
        }
      }
    }
  }

  Path _hex(double r) => Draw.hexPath(Offset(g.cx, g.cy), r);

  void _arena(double t) {
    final rh = g.rh, cx = g.cx, cy = g.cy;
    final danger = sim.field.theirs.isNotEmpty && _rating() >= 3;
    final hair = Draw.line(_amber, 1, .16);
    for (final f in [.33, .66]) {
      Draw.dashedPath(c, _hex(rh * f), hair, 2, 5);
    }
    c.drawLine(Offset(cx - rh * .98, cy), Offset(cx + rh * .98, cy), Draw.line(_amber, 1, .14));
    c.drawLine(Offset(cx, cy - rh * .866), Offset(cx, cy + rh * .866), Draw.line(_amber, 1, .14));
    final tk = Path();
    for (var i = -10; i <= 10; i++) {
      final l = i % 5 == 0 ? 6.0 : 3.0;
      final px = cx + i * rh * .098, py = cy + i * rh * .0866;
      tk.moveTo(px, cy - l);
      tk.lineTo(px, cy + l);
      if ((py - cy).abs() < rh * .866) {
        tk.moveTo(cx - l, py);
        tk.lineTo(cx + l, py);
      }
    }
    c.drawPath(tk, Draw.line(_amber, 1, .3));
    final sp = Draw.line(_amber, 1, .12);
    for (var i = 0; i < 6; i++) {
      final a = i * math.pi / 3 + math.pi / 6;
      Draw.dashedLine(c, Offset(cx, cy), Offset(cx + math.cos(a) * rh * .866, cy + math.sin(a) * rh * .866), sp, 2, 8);
    }
    final col = danger ? _ember : _amber;
    final al = danger ? .65 + .25 * math.sin(t * 5) : .72;
    final hx = _hex(rh);
    if (bloom) {
      c.drawPath(
          hx,
          Paint()
            ..style = PaintingStyle.stroke
            ..strokeWidth = 7
            ..blendMode = BlendMode.plus
            ..color = col.withValues(alpha: .1));
    }
    c.drawPath(hx, Draw.line(col, 1.6, al));
    c.drawPath(_hex(rh * 1.045), Draw.line(_amber, 1, .16));
    final vp = Draw.line(col, 1.4, .8);
    for (var i = 0; i < 6; i++) {
      final a = i * math.pi / 3;
      final v = Offset(cx + math.cos(a) * rh, cy + math.sin(a) * rh);
      c.drawLine(v + Offset(math.cos(a), math.sin(a)) * 3, v + Offset(math.cos(a), math.sin(a)) * 11, vp);
      final bl = ((90 + i * 60) % 360).toString().padLeft(3, '0');
      Draw.text(c, bl, v.dx + math.cos(a) * 24, v.dy + math.sin(a) * 24, T.mono(size: 8.5, color: const Color(0x8CBFA370)), anchor: .5);
    }
    final bz = Path();
    for (var i = 0; i < 60; i++) {
      final a = i / 60 * tau, r1 = rh * 1.1, r2 = rh * (i % 5 != 0 ? 1.12 : 1.145);
      bz.moveTo(cx + math.cos(a) * r1, cy + math.sin(a) * r1);
      bz.lineTo(cx + math.cos(a) * r2, cy + math.sin(a) * r2);
    }
    c.drawPath(bz, Draw.line(_amber, 1, .22));
    for (var i = 0; i < gates.length; i++) {
      final gi = gates[i];
      if (gi == null) continue;
      _gate(gi, i, t);
    }
    for (var i = 0; i < 6; i++) {
      if (lanes[i]) continue;
      final a = dirs[i].angle, nx = math.cos(a), ny = math.sin(a), px = -ny, py = nx, hw = rh * .42;
      final base = Offset(cx + nx * rh * .866, cy + ny * rh * .866);
      c.drawLine(base + Offset(px, py) * hw * .78, base - Offset(px, py) * hw * .78, Draw.line(_amber, 2.2, .55));
      final h = Path();
      for (var k = -5; k <= 5; k++) {
        final b = base + Offset(px, py) * hw * .15 * k.toDouble();
        h.moveTo(b.dx, b.dy);
        h.lineTo(b.dx - nx * 8 + px * 5, b.dy - ny * 8 + py * 5);
      }
      c.drawPath(h, Draw.line(_amber, 1, .28));
    }
  }

  /// The sector's threat rating, as the server rates it.
  int _rating() => sim.sector?.threat.rating ?? 0;

  void _gate(GateInfo gi, int i, double t) {
    final hv = sim.hover == i;
    final a = gi.dir.angle, nx = math.cos(a), ny = math.sin(a), px = -ny, py = nx;
    final gp = g.gatePoint(gi.dir);
    final hw = g.rh * .17, al = hv ? 1.0 : .8;
    Draw.seg(c, gp - Offset(px, py) * hw, gp + Offset(px, py) * hw, gi.color, al, hv ? 2.6 : 2, bloom: bloom);
    for (final sg in const [-1, 1]) {
      final e = gp + Offset(px, py) * hw * sg.toDouble();
      c.drawRect(Rect.fromCenter(center: e, width: 7, height: 7), Draw.line(gi.color, 1.4, al));
    }
    for (var k = 0; k < 3; k++) {
      final ph = reduced ? k / 3 : ((t * .55 + k / 3 + i * .13) % 1);
      final d0 = -g.rh * .17 + ph * g.rh * .22;
      final al2 = (1 - (ph - .5).abs() * 2) * (hv ? 1 : .7);
      final b = gp + Offset(nx, ny) * d0, w = g.rh * .07;
      final p = Path()
        ..moveTo(b.dx - px * w - nx * w * .55, b.dy - py * w - ny * w * .55)
        ..lineTo(b.dx + nx * w * .1, b.dy + ny * w * .1)
        ..lineTo(b.dx + px * w - nx * w * .55, b.dy + py * w - ny * w * .55);
      c.drawPath(p, Draw.line(gi.color, 1.3, math.max(0, al2)));
    }
    if (hv) {
      Draw.dashedLine(c, Offset(g.cx, g.cy + g.rh * .2), gp, Draw.line(C.a200, 1, .65), 5, 5, reduced ? 0 : t * 30);
    }
    c.drawLine(gp, gp + Offset(nx, ny) * g.rh * .26, Draw.line(gi.color, 1, .25));
  }

  void _planet(double t) {
    final r = g.rh * .3, x = g.cx - g.rh * .1, y = g.cy - g.rh * .12, rot = reduced ? 0.0 : t * .12;
    final ctr = Offset(x, y);
    c.drawCircle(ctr, r, Draw.line(const Color(0xFFFFC864), 1.5, .9));
    if (bloom) {
      c.drawCircle(
          ctr,
          r,
          Paint()
            ..style = PaintingStyle.stroke
            ..strokeWidth = 7
            ..blendMode = BlendMode.plus
            ..color = _amber.withValues(alpha: .12));
    }
    for (var i = 0; i < 6; i++) {
      final a = rot + i * math.pi / 6, rx = math.max(.5, math.cos(a).abs() * r);
      c.drawOval(Rect.fromCenter(center: ctr, width: rx * 2, height: r * 2), Draw.line(_amber, 1, .18 + .32 * math.sin(a).abs()));
    }
    for (var k = -3; k <= 3; k++) {
      final yy = k * r * .25, rx = math.sqrt(math.max(0, r * r - yy * yy));
      c.drawOval(Rect.fromCenter(center: Offset(x, y + yy), width: rx * 2, height: rx * .32), k == 0 ? Draw.line(const Color(0xFFFFC864), 1, .55) : Draw.line(_amber, 1, .3));
    }
    c.save();
    c.clipPath(Path()..addOval(Rect.fromCircle(center: ctr, radius: r - 1)));
    final lit = Path()..addOval(Rect.fromCenter(center: Offset(x - r * .5, y), width: r * 2.5, height: r * 2.6));
    final night = Path.combine(PathOperation.difference, Path()..addRect(Rect.fromCircle(center: ctr, radius: r)), lit);
    Draw.hatch(c, night, Rect.fromCircle(center: ctr, radius: r), _amber, .22, gap: 5);
    c.restore();
    c.save();
    c.translate(x, y);
    c.rotate(-.35);
    final orbit = Rect.fromCenter(center: Offset.zero, width: r * 3.3, height: r * .92);
    c.drawOval(orbit, Draw.line(_own, 1, .4));
    for (var i = 0; i < 3; i++) {
      final a = (reduced ? 0 : t * .25) + i * 2.1;
      final e = Offset(math.cos(a) * r * 1.65, math.sin(a) * r * .46);
      c.drawRect(Rect.fromCenter(center: e, width: 7, height: 7), Draw.line(_own, 1.2, .95));
      c.drawLine(e + const Offset(-7, 0), e + const Offset(-3.5, 0), Draw.line(_own, 1.2, .95));
      c.drawLine(e + const Offset(3.5, 0), e + const Offset(7, 0), Draw.line(_own, 1.2, .95));
    }
    c.restore();
  }

  void _terrain(double t) {
    for (final gs in sim.gas) {
      final o = wp(gs.x + math.sin(t * .05 + gs.o) * .02, gs.y + math.cos(t * .04 + gs.o) * .02);
      for (var k = 0; k < 3; k++) {
        final rr = gs.r * g.rh * (1 - k * .28);
        final p = Path();
        for (var i = 0; i <= gs.n * 2; i++) {
          final a = i / (gs.n * 2) * tau, w = rr * (1 + .12 * math.sin(a * gs.n + gs.o + k));
          final pt = Offset(o.dx + math.cos(a) * w, o.dy + math.sin(a) * w * .8);
          i == 0 ? p.moveTo(pt.dx, pt.dy) : p.lineTo(pt.dx, pt.dy);
        }
        p.close();
        Draw.dashedPath(c, p, Draw.line(_violet, 1, .28 - k * .06), 1, 4);
      }
    }
    for (var i = 0; i < sim.anomalyRings.length; i++) {
      final r = sim.anomalyRings[i];
      c.save();
      c.translate(g.cx + g.rh * .1, g.cy - g.rh * .05);
      c.rotate(t * .1 + i * 1.3);
      c.drawOval(Rect.fromCenter(center: Offset.zero, width: g.rh * r * 2 * (1 + .03 * math.sin(t + i * 1.3)), height: g.rh * r * 1.2 * (1 + .03 * math.sin(t + i * 1.3))),
          Draw.line(_violet, 1, .28 + .2 * math.sin(t * 1.5 + i * 1.3)));
      c.restore();
    }
    if (sim.bits.isNotEmpty) {
      final p = Path();
      for (final b in sim.bits) {
        final o = wp(b.x, b.y), l = b.r * g.rh * 1.6;
        p.moveTo(o.dx - math.cos(b.a) * l, o.dy - math.sin(b.a) * l);
        p.lineTo(o.dx + math.cos(b.a) * l, o.dy + math.sin(b.a) * l);
      }
      c.drawPath(p, Draw.line(const Color(0xFFB48C50), 1, .6));
    }
  }

  void _asteroid(Asteroid a, double t) {
    final o = wp(a.x, a.y), r = a.r * g.rh;
    final glowOn = sim.target == 'o' || sim.hoverT == 'o' || (sim.harvesting && a.ore != null);
    final back = sim.combat || _underAMark(o, r) ? .3 : 1.0;
    c.save();
    c.translate(o.dx, o.dy);
    c.rotate(a.rot);
    final pts = <Offset>[];
    for (var i = 0; i < a.n; i++) {
      final an = i / a.n * tau, rr = r * (.75 + .35 * ((math.sin(a.seed * 50 + i * 7.3) + 1) / 2));
      pts.add(Offset(math.cos(an) * rr, math.sin(an) * rr));
    }
    final p = Path()..addPolygon(pts, true);
    c.drawPath(p, Draw.line(glowOn ? const Color(0xFFFFE2A3) : _amber, glowOn ? 1.6 : 1.2, (glowOn ? .95 : .62) * back));
    if (bloom && back == 1) {
      c.drawPath(
          p,
          Paint()
            ..style = PaintingStyle.stroke
            ..strokeWidth = 5
            ..blendMode = BlendMode.plus
            ..color = (glowOn ? const Color(0xFFFFC878) : _amber).withValues(alpha: glowOn ? .2 : .07));
    }
    final f = Path();
    for (var i = 0; i < 3; i++) {
      final q = pts[(i * 3 + 1) % pts.length];
      f.moveTo(q.dx, q.dy);
      f.lineTo(r * .12 * math.cos(a.seed * 9 + i), r * .12 * math.sin(a.seed * 9 + i));
    }
    c.drawPath(f, Draw.line(_amber, 1, .3 * back));
    c.restore();
    if (a.ore != null) {
      final oc = _oreCol(a.ore!), pu = .55 + .45 * math.sin(t * 2 + a.seed * 9);
      c.drawRect(Rect.fromCenter(center: o, width: 5, height: 5), Draw.line(oc, 1.2, (.5 + .4 * pu) * back));
      Draw.text(c, _oreName(a.ore!), o.dx + r * .9 + 3, o.dy + r * .5 + 8, T.mono(size: 9, color: oc.withValues(alpha: .85 * back), weight: FontWeight.w500));
    }
  }

  /// A rock of radius [r] at [o] lies under a mark. Such a rock is drawn
  /// faint, as every rock is while a fight is on: the scope is for the
  /// marks first.
  bool _underAMark(Offset o, double r) {
    for (final m in sim.field.marks) {
      if (!m.placed) continue;
      final at = _centre + m.at + (sim.field.offsets[m.formation] ?? Offset.zero);
      if (Rect.fromCenter(center: at, width: m.shape.size.width + r * 2, height: m.shape.size.height + r * 2).contains(o)) return true;
    }
    return false;
  }

  Color _oreCol(String k) => C.ore(k);
  String _oreName(String k) => k == 'metal' ? 'Fe' : k == 'crystal' ? 'Cr' : 'De';

  void _salvage(double t) {
    final sv = sim.salvage;
    if (sv == null) return;
    var o = wp(sv.x, sv.y);
    if (sv.fly >= 0) {
      final p = math.min(1.0, (sim.now - sv.fly) / .6);
      final u = _flagship;
      if (u != null) {
        final to = _centre + u.lead;
        o = o + (to - o) * (p * p);
      }
    }
    final pu = .5 + .5 * math.sin(t * 3);
    Draw.symbol(c, Sym.salvage, o.dx, o.dy, g.rh * .075, color: _gold, alpha: .95, bloom: bloom);
    final oct = Path();
    for (var i = 0; i < 8; i++) {
      final a = i * tau / 8 + t * .2, rr = g.rh * (.08 + .012 * pu);
      final pt = Offset(o.dx + math.cos(a) * rr, o.dy + math.sin(a) * rr);
      i == 0 ? oct.moveTo(pt.dx, pt.dy) : oct.lineTo(pt.dx, pt.dy);
    }
    oct.close();
    c.drawPath(oct, Draw.line(_gold, 1, .25 + .3 * pu));
    if (sim.target == 's' || sim.hoverT == 's') Draw.reticle(c, o.dx, o.dy, g.rh * .1, _gold, label: 'SALVAGE');
  }

  void _exchange(double t) {
    final o = wp(.38, -.26), hl = sim.target == 'x' || sim.hoverT == 'x';
    final s = g.rh * .17;
    Draw.path(c, Sym.hub.outline, o.dx, o.dy, s, sim.reduced ? 0 : t * .04, _gold, 1.3, alpha: hl ? 1 : .85, bloom: bloom);
    Draw.text(c, 'EXCHANGE', o.dx, o.dy + s * 1.1, T.cond(size: 10, color: _gold.withValues(alpha: .9), spacing: 2), anchor: .5);
    if (hl) Draw.reticle(c, o.dx, o.dy, s, _gold);
  }

  /// The station stands where the exchange does at the hub, which no outpost
  /// shares a sector with, on a ground of its own: the rocks of the sector
  /// drift behind it. The player's own is filled; a breached one is drawn
  /// broken.
  void _outpost(proto.OutpostBrief post) {
    final o = wp(.38, -.26), hl = sim.target == 'p' || sim.hoverT == 'p';
    final s = g.rh * .15;
    final col = post.breached ? _ember : post.mine ? _own : _gold;
    final label = '${post.mine ? 'YOUR OUTPOST' : 'OUTPOST'}${post.level > 0 ? ' L${post.level}' : ''}${post.breached ? ' BREACHED' : ''}';
    final style = T.cond(size: 10, color: col.withValues(alpha: .9), spacing: 2);
    final name = Draw.tp(label, style);
    final ground = Paint()..color = const Color(0xF007050A);
    c.drawCircle(o, s * 1.05, ground);
    c.drawRect(Rect.fromCenter(center: o + Offset(0, s * 1.3), width: name.width + 10, height: name.height + 2), ground);
    if (post.mine) c.drawRect(Rect.fromCenter(center: o, width: s * .96, height: s * .96), Paint()..color = col.withValues(alpha: .22));
    Draw.symbol(c, Sym.outpost, o.dx, o.dy, s, color: col, lw: 1.3, alpha: hl ? 1 : .85, bloom: bloom);
    if (post.breached) Draw.dashedLine(c, o + Offset(-s * .48, s * .48), o + Offset(s * .48, -s * .48), Draw.line(col, 1.3), 4, 3);
    Draw.text(c, label, o.dx, o.dy + s * 1.3, style, anchor: .5);
    if (hl) Draw.reticle(c, o.dx, o.dy, s, col);
  }

  void _derelict(double t) {
    final d = sim.derelict;
    if (d == null) return;
    final o = wp(d.x, d.y), hl = sim.target == 'd' || sim.hoverT == 'd';
    final rc = d.risk == proto.SiteRisk.hot ? _ember : d.risk == proto.SiteRisk.uneasy ? const Color(0xFFFF9A2E) : const Color(0xFF8FE388);
    final s = g.rh * .1, rot = -.5 + math.sin(t * .2) * .05;
    Draw.path(c, Sym.derelict.outline, o.dx, o.dy, s * 2.3, rot, _violet, 1.3, alpha: hl ? 1 : .85, bloom: bloom);
    Draw.path(c, Sym.derelict.detail!, o.dx, o.dy, s * 2.3, rot, _violet, 1, alpha: .6, bloom: false);
    c.save();
    c.translate(o.dx, o.dy);
    c.rotate(rot);
    final boarding = sim.boarding != null;
    for (var i = 0; i < 5; i++) {
      final on = math.sin(t * (boarding ? 9 : 1.3) + i * 2.1) > .6;
      c.drawRect(Rect.fromLTWH(-s * 1.5 + i * s * .7, -s * .15, s * .14, s * .12), Paint()..color = Color.fromRGBO(255, 200, 120, on ? .9 : .15));
    }
    c.restore();
    Draw.dashedPath(c, Path()..addOval(Rect.fromCircle(center: o, radius: s * 2.9 + math.sin(t * 2) * 2)), Draw.line(rc, 1, .5), 2, 6);
    final p = sim.boardingProgress;
    if (p != null) {
      const seg = 16;
      final full = (p * seg).floor(), rr = s * 3.4;
      for (var i = 0; i < full; i++) {
        final a0 = -math.pi / 2 + i / seg * tau + .05, a1 = -math.pi / 2 + (i + 1) / seg * tau - .05;
        c.drawArc(Rect.fromCircle(center: o, radius: rr), a0, a1 - a0, false, Draw.line(_violet, 3, .9));
      }
      c.drawCircle(o, rr, Draw.line(_violet, 1, .25));
      Draw.text(c, 'BOARDING ${(p * 100).round()}%', o.dx, o.dy + rr + 16, T.mono(size: 10, color: _violet.withValues(alpha: .9), weight: FontWeight.w500), anchor: .5);
    }
    if (hl) Draw.reticle(c, o.dx, o.dy, s * 3.7, _violet, label: 'DERELICT T${d.tier}');
  }

  Offset get _centre => Offset(g.cx, g.cy);

  /// The first mark of the fleet the scope is watched from: where a beam
  /// or a scooped pile is drawn to.
  Mark? get _flagship {
    final id = sim.fleetId;
    if (id == null) return null;
    return sim.field.marks.where((m) => m.placed && m.formation == Formation.fleetId(id)).firstOrNull;
  }

  /// The hostile groups under the pointer or picked as the target, in one
  /// bracket with the sector's rating.
  void _hostileReticle() {
    if (sim.combat || !(sim.target == 'h' || sim.hoverT == 'h')) return;
    Rect? box;
    for (final m in sim.field.theirs) {
      if (!m.placed) continue;
      final r = Rect.fromCenter(center: _centre + m.at, width: m.shape.size.width, height: m.shape.size.height);
      box = box?.expandToInclude(r) ?? r;
    }
    if (box == null) return;
    Draw.reticle(c, box.center.dx, box.center.dy, math.max(box.width, box.height) / 2 + 8, _ember, label: 'HOSTILE T${_rating()}');
  }

  /// The round the fight stands at and how it stands, on the centre line
  /// between the two sides.
  void _stamp(double t) {
    final stamp = sim.stamp;
    if (stamp == null || sim.warp != null) return;
    final beat = reduced ? 0.0 : math.max(0.0, 1 - (sim.now - sim.field.hazeFrom) / .5);
    final left = Offset(g.cx - g.rh * .9, g.cy);
    final round = T.cond(size: g.compact ? 10 : 12, color: stamp.color.withValues(alpha: .75 + .25 * beat), weight: FontWeight.w700, spacing: 2);
    final plate = Paint()..color = const Color(0xD007050A);
    final wide = Draw.tp(stamp.round, round).width;
    c.drawRect(Rect.fromLTWH(left.dx - 4, left.dy - 9, wide + 8, 18), plate);
    Draw.text(c, stamp.round, left.dx, left.dy, round);
    final verdict = stamp.verdict;
    if (verdict == null) return;
    final style = T.cond(size: g.compact ? 10 : 12, color: stamp.color, weight: FontWeight.w700, spacing: 2);
    final w = Draw.tp(verdict, style).width;
    final right = g.cx + g.rh * .9;
    c.drawRect(Rect.fromLTWH(right - w - 4, g.cy - 9, w + 8, 18), plate);
    Draw.text(c, verdict, right, g.cy, style, anchor: 1);
  }

  /// A fleet charging its jump out of a fight: its course to the lane it
  /// leaves by, and the count of the state's `fleeing` beside it.
  void _flight(double t) {
    final leaving = sim.fleet?.fleeing;
    final lane = sim.exitLane;
    final id = sim.fleetId;
    if (leaving == null || lane == null || id == null) return;
    final box = sim.field.boundsOf(Formation.fleetId(id));
    if (box == null) return;
    final from = _centre + box.center + (sim.field.offsets[Formation.fleetId(id)] ?? Offset.zero);
    final gate = g.gatePoint(lane);
    const col = Color(0xFFFFCC66);
    Draw.dashedLine(c, from, gate, Draw.line(col, 1.3, .7), 6, 5, reduced ? 0 : -t * 34);
    final along = gate - from;
    final len = along.distance;
    if (len > 1) {
      final n = along / len, side = Offset(-n.dy, n.dx);
      for (var k = 0; k < 3; k++) {
        final ph = reduced ? (k + 1) / 4 : (t * .7 + k / 3) % 1;
        final at = from + n * (len * (.25 + .7 * ph));
        final p = Path()
          ..moveTo(at.dx - n.dx * 5 + side.dx * 5, at.dy - n.dy * 5 + side.dy * 5)
          ..lineTo(at.dx, at.dy)
          ..lineTo(at.dx - n.dx * 5 - side.dx * 5, at.dy - n.dy * 5 - side.dy * 5);
        c.drawPath(p, Draw.line(col, 1.4, (1 - (ph - .5).abs() * 2).clamp(0.0, 1.0)));
      }
    }
    final label = 'JUMP IN ${leaving.ticks}';
    final style = T.cond(size: g.compact ? 11 : 13, color: C.a100, weight: FontWeight.w700, spacing: 2);
    final note = T.mono(size: g.compact ? 8.5 : 9.5, color: col.withValues(alpha: .9));
    final at = Offset(from.dx, from.dy + box.height / 2 + 14);
    final wide = math.max(Draw.tp(label, style).width, Draw.tp('GUNS SILENT', note).width);
    c.drawRect(Rect.fromCenter(center: at + const Offset(0, 6), width: wide + 12, height: 30), Paint()..color = const Color(0xD007050A));
    Draw.text(c, label, at.dx, at.dy, style, anchor: .5);
    Draw.text(c, 'GUNS SILENT', at.dx, at.dy + 13, note, anchor: .5);
  }

  /// A group on the fleet's trail: the lane it comes down, marked from the
  /// gate inward, with the ticks until it lands and whether it lands before
  /// the fleet can be gone. All of it is the state's `pursued_by`.
  void _pursuit(double t) {
    final hunter = sim.fleet?.pursuedBy;
    final lane = sim.pursuitLane;
    if (hunter == null || lane == null) return;
    final a = lane.angle, n = Offset(math.cos(a), math.sin(a)), side = Offset(-n.dy, n.dx);
    final gate = g.gatePoint(lane);
    final inner = _centre + n * g.rh * .34;
    final pulse = reduced ? 1.0 : .6 + .4 * math.sin(t * 6);
    Draw.seg(c, gate - side * g.rh * .2, gate + side * g.rh * .2, _ember, pulse, 3, bloom: bloom);
    Draw.dashedLine(c, gate, inner, Draw.line(_ember, 1.2, .55), 4, 6, reduced ? 0 : t * 40);
    for (var k = 0; k < 4; k++) {
      final ph = reduced ? (k + 1) / 5 : (t * .9 + k / 4) % 1;
      final at = Offset.lerp(gate, inner, ph)!;
      final w = g.rh * .05;
      final p = Path()
        ..moveTo(at.dx + n.dx * w + side.dx * w, at.dy + n.dy * w + side.dy * w)
        ..lineTo(at.dx, at.dy)
        ..lineTo(at.dx + n.dx * w - side.dx * w, at.dy + n.dy * w - side.dy * w);
      c.drawPath(p, Draw.line(_ember, 1.6, (1 - ph) * .95));
    }
    final at = _centre + n * g.rh * .6 + side * g.rh * .3;
    final head = 'PURSUIT ${hunter.arrivesIn}';
    final verdict = hunter.catches ? 'CATCHES' : 'TOO LATE';
    final style = T.cond(size: g.compact ? 11 : 13, color: const Color(0xFFFFD6CC), weight: FontWeight.w700, spacing: 2);
    final note = T.mono(size: g.compact ? 8.5 : 9.5, color: hunter.catches ? _ember : C.ok, weight: FontWeight.w600);
    final wide = math.max(Draw.tp(head, style).width, Draw.tp(verdict, note).width);
    c.drawRect(Rect.fromCenter(center: at + const Offset(0, 6), width: wide + 12, height: 30), Paint()..color = const Color(0xE007050A));
    c.drawRect(Rect.fromCenter(center: at + const Offset(0, 6), width: wide + 12, height: 30), Draw.line(_ember, 1, .8 * pulse));
    Draw.text(c, head, at.dx, at.dy, style, anchor: .5);
    Draw.text(c, verdict, at.dx, at.dy + 13, note, anchor: .5);
  }

  void _beams(double t) {
    if (!sim.harvesting || sim.warp != null) return;
    final id = sim.fleetId;
    final us = [
      for (final m in sim.field.marks)
        if (m.placed && id != null && m.formation == Formation.fleetId(id)) m,
    ];
    final as = sim.ast.where((a) => a.ore != null).toList();
    if (as.isEmpty) return;
    for (var i = 0; i < us.length; i++) {
      final a = as[i % as.length];
      Draw.dashedLine(c, _centre + us[i].lead, wp(a.x, a.y), Draw.line(_oreCol(a.ore!), 1.2, .8), 3, 4, reduced ? 0 : t * 40);
    }
  }

  void _scan(double t) {
    final pl = sim.pulse;
    if (pl == null) return;
    final p = (sim.now - pl.t0) / 1.6;
    if (p > 1 || p < 0) return;
    final u = wp(0, .24);
    final ang = -math.pi / 2 + p * tau * 1.5, len = g.rh * 1.15 * math.min(1.0, p * 3.5);
    final wedge = reduced ? 8 : 22;
    for (var k = 0; k < wedge; k++) {
      final a = ang - k * .045;
      c.drawLine(u, u + Offset(math.cos(a), math.sin(a)) * len, Draw.line(_own, 1.3, (1 - k / wedge) * .5 * (1 - p * .6)));
    }
    final rr = p * g.rh * 1.4;
    final ring = Path();
    for (var i = 0; i < 6; i++) {
      final a = i * math.pi / 3;
      final pt = Offset(u.dx + math.cos(a) * rr, u.dy + math.sin(a) * rr);
      i == 0 ? ring.moveTo(pt.dx, pt.dy) : ring.lineTo(pt.dx, pt.dy);
    }
    ring.close();
    c.drawPath(ring, Draw.line(_own, 1.4, (1 - p) * .9));
  }

  @override
  bool shouldRepaint(ArenaPainter o) => true;
}
