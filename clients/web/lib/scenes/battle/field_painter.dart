import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui';

import 'package:flutter/painting.dart' show FontWeight, TextStyle;

import '../../console/battle.dart' show shortCount;
import '../../console/intel.dart' show groupDigits;
import '../../console/threat.dart' show unitAbbr;
import '../../design/draw.dart';
import '../../design/symbols.dart';
import '../../design/tokens.dart';
import '../../protocol/protocol.dart' as proto;
import 'field.dart';

const _own = Color(0xFF5EE0CF);
const _ember = Color(0xFFFF5A3C);
const _battery = Color(0xFF7FD4FF);
const _gold = Color(0xFFFFCC66);
const _ourFire = Color(0xFF9FFFF0);
const _theirFire = Color(0xFFFF7A5C);
const _plate = Color(0xB807050A);

/// How the three parts of a mark are coloured: its whole units in its
/// side's colour, the damaged amber and those at risk ember, as the lists
/// beside a scene colour them.
const damagedTint = C.a300;
const atRiskTint = C.ember;

/// What a block of pips is, for a scene that has room to say it.
const marksRule = 'A MARK IS ONE KIND OF UNIT. UP TO ${MarkShape.literal} ARE DRAWN A GLYPH EACH; MORE ARE ONE GLYPH AND A BLOCK THAT GROWS '
    'WITH THE SQUARE ROOT OF THE COUNT. THE BAR UNDER IT: WHOLE, DAMAGED, AT RISK.';

Color markColor(Mark m) => m.hostile ? _ember : m.battery ? _battery : _own;

/// Draws a [BattleField] about [centre]: every mark with its count and its
/// bar, the fire of the round in the air, and what it broke.
class FieldPainter {
  final Canvas c;
  final BattleField f;
  final Offset centre;
  final double t;
  final bool bloom;

  /// The mark the pointer is on: it is named in full.
  final Mark? hover;
  FieldPainter(this.c, this.f, this.centre, {required this.t, required this.bloom, this.hover});

  static final _points = Paint()
    ..strokeCap = StrokeCap.square
    ..isAntiAlias = false;
  static final _fill = Paint();

  void paint() {
    _haze();
    for (final m in f.marks) {
      _mark(m);
    }
    _captions();
    _beams();
    _fx();
    final named = hover;
    if (named != null && named.placed) _callout(named);
  }

  Offset _offsetOf(Mark m) => f.offsets[m.formation] ?? Offset.zero;

  double _bob(Mark m) => f.reduced || f.headings.containsKey(m.formation) ? 0 : math.sin(t * 1.2 + m.phase) * 1.2;

  /// Where fire leaves [m] from and lands on it, on the canvas.
  Offset _lead(Mark m) => centre + m.lead + _offsetOf(m);

  void _mark(Mark m) {
    if (!m.placed) return;
    final shape = m.shape;
    final col = markColor(m);
    final off = _offsetOf(m);
    final ghost = m.fallen;
    final alpha = ghost ? .3 : m.fleeing ? .78 : 1.0;
    final at = centre + m.cluster + off + Offset(0, _bob(m));
    final heading = f.headings[m.formation];
    final lit = m.flash > .3 ? const Color(0xFFFFF0DC) : col;
    final s = shape.glyph;
    final def = Sym.unit(m.unit, hostile: m.hostile);

    c.save();
    c.translate(at.dx, at.dy);
    if (heading != null) c.rotate(heading + math.pi / 2);
    if (m.hostile) c.scale(1, -1);
    if (m.unit.ship != null && m.unit.ship != proto.ShipClass.solarSatellite && !ghost && !f.reduced) {
      final burn = (.55 + .45 * math.sin(t * 22 + m.phase)) * (heading != null ? 2.2 : 1);
      final flare = Draw.line(m.hostile ? const Color(0xFFFF965A) : const Color(0xFFA0FFF0), 1, .55);
      for (final g in shape.glyphs) {
        c.drawLine(Offset(g.dx, g.dy + s * .92), Offset(g.dx, g.dy + s * (.92 + .45 * burn)), flare);
      }
    }
    for (final g in shape.glyphs) {
      Draw.symbol(c, def, g.dx, g.dy, s, color: lit, lw: (s < 8 ? 1.0 : 1.3) + m.flash * .9, alpha: math.min(1.0, alpha + m.flash), bloom: bloom && !ghost);
    }
    _pips(m, col, alpha);
    c.restore();

    if (m.screen > .05) {
      final lead = _lead(m);
      c.drawArc(Rect.fromCircle(center: lead, radius: s * 1.55), m.screenFrom - .8, 1.6, false, Draw.line(_battery, 1.6, .85 * m.screen));
      if (bloom) c.drawArc(Rect.fromCircle(center: lead, radius: s * 1.55), m.screenFrom - .8, 1.6, false, Draw.line(_battery, 5, .16 * m.screen));
    }
    _label(m, col, off, alpha);
  }

  /// The block in the three tints of the mark's bar, by the same counts.
  void _pips(Mark m, Color col, double alpha) {
    final pips = m.shape.pips;
    final n = pips.length ~/ 2;
    if (n == 0) return;
    final count = math.max(1, m.shown.count);
    final atRisk = m.shown.atRisk == 0 ? 0 : math.max(1, (n * m.shown.atRisk / count).round());
    final hurt = m.shown.damaged - m.shown.atRisk;
    final damaged = hurt <= 0 ? 0 : math.max(1, (n * hurt / count).round());
    final whole = math.max(0, n - atRisk - damaged);
    _points.strokeWidth = m.shape.pip;
    void run(int from, int to, Color color) {
      if (to <= from) return;
      _points.color = color.withValues(alpha: alpha * .92);
      c.drawRawPoints(PointMode.points, Float32List.sublistView(pips, from * 2, to * 2), _points);
    }

    run(0, whole, col);
    run(whole, math.min(n, whole + damaged), damagedTint);
    run(math.min(n, whole + damaged), n, atRiskTint);
  }

  /// The mark's short name and count on a plate, and under it the bar in
  /// three parts: whole, damaged, at risk.
  void _label(Mark m, Color col, Offset off, double alpha) {
    final shape = m.shape;
    final font = shape.font;
    final half = shape.size.height / 2;
    final mid = centre + Offset(m.x, m.y) + off;
    final labelY = m.hostile ? mid.dy - half + 4 + (font + 2) / 2 : mid.dy - half + shape.cluster.height + 3 + (font + 2) / 2;
    final barY = m.hostile ? mid.dy - half : mid.dy - half + shape.cluster.height + font + 6;
    final count = shortCount(m.shown.count);
    final tag = shape.dense ? '' : '${unitAbbr(m.unit)} ';
    final tagStyle = T.mono(size: font, color: C.text3.withValues(alpha: alpha));
    final countStyle = T.mono(size: font, color: (m.fallen ? C.text3 : C.text).withValues(alpha: alpha), weight: FontWeight.w500);
    final tagWide = tag.isEmpty ? 0.0 : Draw.tp(tag, tagStyle).width;
    final wide = tagWide + Draw.tp(count, countStyle).width;
    _fill.color = _plate;
    c.drawRect(Rect.fromCenter(center: Offset(mid.dx, labelY), width: wide + 6, height: font + 3), _fill);
    if (tag.isNotEmpty) Draw.text(c, tag, mid.dx - wide / 2, labelY, tagStyle);
    Draw.text(c, count, mid.dx - wide / 2 + tagWide, labelY, countStyle);

    final barWide = (shape.size.width - 4).clamp(16.0, 56.0);
    final left = mid.dx - barWide / 2;
    final whole = m.hostile ? const Color(0xFFA8402C) : col;
    final units = m.shown.count;
    if (units <= 0) return;
    final riskWide = barWide * m.shown.atRisk / units;
    final hurtWide = barWide * math.max(0, m.shown.damaged - m.shown.atRisk) / units;
    final wholeWide = math.max(0.0, barWide - riskWide - hurtWide);
    _fill.color = whole.withValues(alpha: (m.shown.hullMax == 0 ? .45 : .9) * alpha);
    c.drawRect(Rect.fromLTWH(left, barY, wholeWide, 3), _fill);
    if (hurtWide > 0) {
      _fill.color = damagedTint.withValues(alpha: alpha);
      c.drawRect(Rect.fromLTWH(left + wholeWide, barY, hurtWide, 3), _fill);
    }
    if (riskWide > 0) {
      _fill.color = atRiskTint.withValues(alpha: alpha);
      c.drawRect(Rect.fromLTWH(left + wholeWide + hurtWide, barY, riskWide, 3), _fill);
    }
  }

  void _captions() {
    for (final MapEntry(key: id, value: name) in f.captions.entries) {
      final box = f.boundsOf(id);
      if (box == null) continue;
      final m = f.marks.firstWhere((m) => m.formation == id);
      final off = _offsetOf(m);
      final y = m.hostile ? box.top - 9 : box.bottom + 9;
      final at = centre + Offset(box.center.dx, y) + off;
      final style = T.cond(size: 9, color: markColor(m).withValues(alpha: .85), spacing: 1.4);
      final wide = Draw.tp(name, style).width;
      _fill.color = _plate;
      c.drawRect(Rect.fromCenter(center: at, width: wide + 8, height: 11), _fill);
      Draw.text(c, name, at.dx, at.dy, style, anchor: .5);
    }
  }

  /// Every volley of the round that is no beam, as one faint stroke a side.
  void _haze() {
    if (f.haze.isEmpty || f.reduced) return;
    final p = ((f.now - f.hazeFrom) / .5).clamp(0.0, 1.0);
    final glow = math.sin(p * math.pi);
    if (glow <= 0) return;
    final ours = Path(), theirs = Path();
    for (final (from, to, mine) in f.haze) {
      final a = _lead(from), b = _lead(to);
      (mine ? ours : theirs)
        ..moveTo(a.dx, a.dy)
        ..lineTo(b.dx, b.dy);
    }
    c.drawPath(ours, Draw.line(_ourFire, .7, .1 * glow));
    c.drawPath(theirs, Draw.line(_theirFire, .7, .1 * glow));
  }

  /// A beam is a tracer from one mark to another, a pulse or three by the
  /// volley's shots. One that reached no hull stops at the target's shield.
  void _beams() {
    for (final b in f.beams) {
      final a = _lead(b.from), hit = _lead(b.to);
      final toward = (a - hit);
      final stop = b.hull || toward.distance == 0 ? hit : hit + toward / toward.distance * b.to.shape.glyph * 1.5;
      final col = b.ours ? _ourFire : _theirFire;
      if (f.reduced) {
        if (f.now >= b.t0) Draw.seg(c, a, stop, col, b.alpha * .5, b.weight * .7);
        continue;
      }
      for (var k = 0; k < b.pulses; k++) {
        final p = (f.now - b.t0 - k * .09) / b.dur;
        if (p < 0 || p > 1.12) continue;
        final head = Offset.lerp(a, stop, p.clamp(0.0, 1.0))!, tail = Offset.lerp(a, stop, (p - .3).clamp(0.0, 1.0))!;
        Draw.seg(c, tail, head, col, b.alpha, b.weight, bloom: bloom);
        _fill.color = col.withValues(alpha: b.alpha);
        c.drawRect(Rect.fromCenter(center: head, width: b.weight + 1.5, height: b.weight + 1.5), _fill);
      }
    }
  }

  void _fx() {
    for (final p in f.sparks) {
      final o = centre + Offset(p.x, p.y), k = math.max(.001, math.sqrt(p.vx * p.vx + p.vy * p.vy));
      c.drawLine(o, Offset(o.dx - p.vx / k * p.length, o.dy - p.vy / k * p.length), Draw.line(p.color, 1.2, (p.life / p.span).clamp(0.0, 1.0)));
    }
    for (final g in f.frags) {
      final o = centre + Offset(g.x, g.y);
      final d = Offset(math.cos(g.rot), math.sin(g.rot)) * g.length;
      c.drawLine(o - d, o + d, Draw.line(g.color, 1.3, (g.life / g.span).clamp(0.0, 1.0)));
    }
    for (final r in f.rings) {
      final p = (f.now - r.t0) / .7;
      final o = centre + Offset(r.x, r.y), rr = r.r * (1 + p * 5);
      c.drawPath(Draw.hexPath(o, rr, rot: .4), Draw.line(_gold, 1.4, math.max(0, (1 - p) * .9)));
    }
    for (final fl in f.floats) {
      final p = (f.now - fl.t0) / BattleField.floatSeconds;
      if (p < 0) continue;
      final o = centre + Offset(fl.x, fl.y);
      final style = T.mono(size: 12.5, color: fl.color.withValues(alpha: math.max(0, 1 - p * p)), weight: FontWeight.w600);
      Draw.text(c, fl.text, o.dx, o.dy - 10 - p * 22, style, anchor: .5);
    }
  }

  void _callout(Mark m) {
    final mid = centre + Offset(m.x, m.y) + _offsetOf(m);
    final box = Rect.fromCenter(center: mid, width: m.shape.size.width + 8, height: m.shape.size.height + 8);
    final col = markColor(m);
    Draw.reticle(c, mid.dx, mid.dy, math.max(box.width, box.height) / 2, col);
    final s = m.shown;
    final line = [
      m.unit.label.toUpperCase(),
      groupDigits(s.count),
      if (s.damaged > 0) '${groupDigits(s.damaged)} DAMAGED',
      if (s.atRisk > 0) '${groupDigits(s.atRisk)} AT RISK',
    ].join('  ');
    final TextStyle style = T.mono(size: 10.5, color: C.text, weight: FontWeight.w500);
    final wide = Draw.tp(line, style).width;
    final y = m.hostile ? box.bottom + 12 : box.top - 12;
    _fill.color = const Color(0xF007050A);
    c.drawRect(Rect.fromCenter(center: Offset(mid.dx, y), width: wide + 12, height: 16), _fill);
    c.drawRect(Rect.fromCenter(center: Offset(mid.dx, y), width: wide + 12, height: 16), Draw.line(col, 1, .7));
    Draw.text(c, line, mid.dx, y, style, anchor: .5);
  }
}
