import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../console/prefs.dart';
import '../../design/draw.dart';
import '../../design/symbols.dart';
import '../../design/tokens.dart';
import '../../protocol/protocol.dart' as proto;
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
const _cyan = Color(0xFF7FD4FF);

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
    if (sim.shake > 0) canvas.translate((sim.rng.nextDouble() - .5) * sim.shake, (sim.rng.nextDouble() - .5) * sim.shake);
    _stars(size, t);
    if (sim.loc == null) {
      canvas.restore();
      return;
    }
    _arena(t);
    if (sim.atHome) _planet(t);
    _terrain(t);
    for (final a in sim.ast) {
      _asteroid(a, t);
    }
    _salvage(t);
    _derelict(t);
    _hostiles(t);
    _ownShips(t);
    _beams(t);
    _shots(t);
    _scan(t);
    _fx(t);
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
    final danger = sim.hostiles.isNotEmpty && _rating() >= 3;
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

  int _rating() {
    var p = 0.0;
    for (final h in sim.hostiles) {
      if (!h.dead) p += _pw(h.cls);
    }
    return p <= 0 ? 0 : (1 + (math.log(math.max(1, p) / 4) / math.ln2).floor()).clamp(1, 9);
  }

  double _pw(proto.ShipClass c) => const {proto.ShipClass.scout: 9.0, proto.ShipClass.corvette: 22.0, proto.ShipClass.frigate: 48.0, proto.ShipClass.cruiser: 110.0, proto.ShipClass.hauler: 15.0}[c]!;

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
    c.save();
    c.translate(o.dx, o.dy);
    c.rotate(a.rot);
    final pts = <Offset>[];
    for (var i = 0; i < a.n; i++) {
      final an = i / a.n * tau, rr = r * (.75 + .35 * ((math.sin(a.seed * 50 + i * 7.3) + 1) / 2));
      pts.add(Offset(math.cos(an) * rr, math.sin(an) * rr));
    }
    final p = Path()..addPolygon(pts, true);
    c.drawPath(p, Draw.line(glowOn ? const Color(0xFFFFE2A3) : _amber, glowOn ? 1.6 : 1.2, glowOn ? .95 : .62));
    if (bloom) {
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
    c.drawPath(f, Draw.line(_amber, 1, .3));
    c.restore();
    if (a.ore != null) {
      final oc = _oreCol(a.ore!), pu = .55 + .45 * math.sin(t * 2 + a.seed * 9);
      c.drawRect(Rect.fromCenter(center: o, width: 5, height: 5), Draw.line(oc, 1.2, .5 + .4 * pu));
      Draw.text(c, _oreName(a.ore!), o.dx + r * .9 + 3, o.dy + r * .5 + 8, T.mono(size: 9, color: oc.withValues(alpha: .85), weight: FontWeight.w500));
    }
  }

  Color _oreCol(String k) => C.ore(k);
  String _oreName(String k) => k == 'metal' ? 'Fe' : k == 'crystal' ? 'Cr' : 'De';

  void _salvage(double t) {
    final sv = sim.salvage;
    if (sv == null) return;
    var o = wp(sv.x, sv.y);
    if (sv.fly >= 0) {
      final p = math.min(1.0, (sim.now - sv.fly) / .6);
      final u = sim.own.where((u) => !u.dead).firstOrNull;
      if (u != null) {
        final to = wp(u.x, u.y);
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
    final b = sim.boarding;
    if (b != null) {
      final p = math.min(1.0, (sim.now - b.t0) / b.dur), seg = 16, full = (p * seg).floor();
      final rr = s * 3.4;
      for (var i = 0; i < full; i++) {
        final a0 = -math.pi / 2 + i / seg * tau + .05, a1 = -math.pi / 2 + (i + 1) / seg * tau - .05;
        c.drawArc(Rect.fromCircle(center: o, radius: rr), a0, a1 - a0, false, Draw.line(_violet, 3, .9));
      }
      c.drawCircle(o, rr, Draw.line(_violet, 1, .25));
      Draw.text(c, 'BOARDING ${(p * 100).round()}%', o.dx, o.dy + rr + 16, T.mono(size: 10, color: _violet.withValues(alpha: .9), weight: FontWeight.w500), anchor: .5);
    }
    if (hl) Draw.reticle(c, o.dx, o.dy, s * 3.7, _violet, label: 'DERELICT T${d.tier}');
  }

  void _ship(Unit u, bool enemy, double t, {double alphaMul = 1, Offset? at}) {
    final o = at ?? wp(u.x, u.y);
    final s = WindshieldSim.shipPx(u.cls) * g.sc * 1.95, rot = u.ang;
    final col = enemy ? _ember : _own;
    final fl = u.flash;
    final warp = sim.warp != null;
    if (!u.dead && alphaMul == 1) {
      final f = (.55 + .45 * math.sin(t * 22 + u.id)) * (warp ? 2.4 : 1);
      final ca = math.cos(rot), sa = math.sin(rot);
      Offset l(double lx, double ly) => Offset(o.dx + lx * ca - ly * sa, o.dy + lx * sa + ly * ca);
      for (final k in const [-.28, 0.0, .28]) {
        c.drawLine(l(k * s, s * .9), l(k * s, s * (.9 + .5 * f * (k == 0 ? 1.5 : 1))), Draw.line(enemy ? const Color(0xFFFF965A) : const Color(0xFFA0FFF0), 1.1, .6));
      }
    }
    final def = (enemy ? Sym.hostile : Sym.own)[u.cls]!;
    final cc = fl > .3 ? const Color(0xFFFFF0DC) : col;
    Draw.symbol(c, def, o.dx, o.dy, s, rot: rot, color: cc, lw: 1.3 + fl * .9, alpha: math.min(1.0, .92 + fl) * alphaMul, bloom: bloom && alphaMul == 1);
    if (alphaMul != 1) return;
    if (u.shieldMax > 0 && u.shield > 0 && (sim.combat || fl > 0)) {
      Draw.dashedPath(c, Path()..addOval(Rect.fromCircle(center: o, radius: s * 1.35)), Draw.line(_cyan, 1, .18 + .6 * fl), 3, 4);
    }
    if (sim.combat && !u.dead) {
      final w = s * 1.7, hy = o.dy - s * 1.5;
      c.drawRect(Rect.fromLTWH(o.dx - w / 2, hy, w, 4), Draw.line(col, 1, .45));
      c.drawRect(Rect.fromLTWH(o.dx - w / 2 + 1, hy + 1, (w - 2) * math.max(0, u.hull / math.max(1, u.hullMax)).clamp(0, 1), 2), Paint()..color = col.withValues(alpha: .9));
      if (u.shieldMax > 0) {
        c.drawRect(Rect.fromLTWH(o.dx - w / 2 + 1, hy - 3, (w - 2) * math.max(0, u.shield / u.shieldMax).clamp(0, 1), 1.4), Paint()..color = _cyan.withValues(alpha: .9));
      }
    }
  }

  void _hostiles(double t) {
    final tg = sim.target == 'h' || sim.hoverT == 'h' || sim.combat;
    var i = 0;
    for (final h in sim.hostiles) {
      if (h.dead) continue;
      _ship(h, true, t);
      if (tg) {
        final o = wp(h.x, h.y), r = WindshieldSim.shipPx(h.cls) * g.sc * 2.1;
        c.save();
        c.translate(o.dx, o.dy);
        c.rotate(math.pi / 4);
        c.drawRect(Rect.fromLTWH(-r * .8, -r * .8, r * 1.6, r * 1.6), Draw.line(_ember, 1.1, sim.combat ? .55 + .35 * math.sin(t * 8 + i) : .8));
        c.restore();
        if (i == 0 && !sim.combat) Draw.reticle(c, o.dx, o.dy, r * 1.15, _ember, label: 'HOSTILE T${_rating()}');
      }
      i++;
    }
  }

  void _ownShips(double t) {
    if (quality == Quality.high) {
      for (var gi = 0; gi < sim.ghosts.length; gi++) {
        final gh = sim.ghosts[gi];
        final a = .12 * (gi + 1) / sim.ghosts.length;
        for (var i = 0; i < gh.length && i < sim.own.length; i++) {
          if (sim.own[i].dead) continue;
          _ship(sim.own[i], false, t, alphaMul: a, at: wp(gh[i].dx, gh[i].dy));
        }
      }
    }
    for (final u in sim.own) {
      if (u.dead) continue;
      final bob = sim.warp == null ? math.sin(t * 1.2 + u.id) * .004 : 0.0;
      final y0 = u.y;
      u.y += bob;
      _ship(u, false, t);
      u.y = y0;
    }
  }

  void _beams(double t) {
    if (!sim.harvesting || sim.warp != null) return;
    final us = sim.own.where((u) => !u.dead).toList();
    final as = sim.ast.where((a) => a.ore != null).toList();
    if (as.isEmpty) return;
    for (var i = 0; i < us.length; i++) {
      final a = as[i % as.length];
      final p1 = wp(us[i].x, us[i].y), p2 = wp(a.x, a.y);
      Draw.dashedLine(c, p1 + const Offset(0, -8), p2, Draw.line(_oreCol(a.ore!), 1.2, .8), 3, 4, reduced ? 0 : t * 40);
    }
  }

  void _shots(double t) {
    for (final s in sim.shots) {
      final p = (sim.now - s.t0) / s.dur;
      if (p < 0) continue;
      final a = wp(s.a.x, s.a.y), b = wp(s.t.x, s.t.y);
      final q = p.clamp(0.0, 1.0), q0 = math.max(0.0, p - .22);
      final col = s.side ? const Color(0xFF9FFFF0) : const Color(0xFFFF7A5C);
      Draw.seg(c, Offset.lerp(a, b, q0)!, Offset.lerp(a, b, q)!, col, 1, 1.6, bloom: bloom);
      final head = Offset.lerp(a, b, q)!;
      c.drawRect(Rect.fromCenter(center: head, width: 3, height: 3), Paint()..color = col);
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

  void _fx(double t) {
    for (final p in sim.sparks) {
      final o = wp(p.x, p.y), k = math.max(.001, math.sqrt(p.vx * p.vx + p.vy * p.vy)), l = p.len * g.rh;
      c.drawLine(o, Offset(o.dx - p.vx / k * l, o.dy - p.vy / k * l), Draw.line(p.c, 1.2, math.max(0, p.l / p.m)));
    }
    for (final f in sim.frags) {
      final o = wp(f.x, f.y), l = f.len * g.rh;
      c.drawLine(Offset(o.dx - math.cos(f.rot) * l, o.dy - math.sin(f.rot) * l), Offset(o.dx + math.cos(f.rot) * l, o.dy + math.sin(f.rot) * l), Draw.line(f.col, 1.3, math.max(0, f.l / f.m)));
    }
    for (final r in sim.rings) {
      final p = (sim.now - r.t0) / .7;
      final o = wp(r.x, r.y), rr = r.r * g.rh * .6 * (1 + p * 5);
      final path = Path();
      for (var k = 0; k < 8; k++) {
        final a = k * tau / 8 + .4;
        final pt = Offset(o.dx + math.cos(a) * rr, o.dy + math.sin(a) * rr);
        k == 0 ? path.moveTo(pt.dx, pt.dy) : path.lineTo(pt.dx, pt.dy);
      }
      path.close();
      c.drawPath(path, Draw.line(_gold, 1.4, math.max(0, (1 - p) * .9)));
    }
    for (final f in sim.floats) {
      final p = (sim.now - f.t0) / 1.1;
      final o = wp(f.x, f.y);
      Draw.text(c, f.text, o.dx, o.dy - p * 30, T.mono(size: 12, color: f.color.withValues(alpha: math.max(0, 1 - p)), weight: FontWeight.w500), anchor: .5);
    }
  }

  @override
  bool shouldRepaint(ArenaPainter o) => true;
}
