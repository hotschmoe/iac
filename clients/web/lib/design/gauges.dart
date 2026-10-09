import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../protocol/protocol.dart' as proto;
import 'beat.dart';
import 'tokens.dart';

/// Register numeral: zero-padded, leading zeros dimmed to 28 %.
class Register extends StatelessWidget {
  final num value;
  final int width;
  final double size;
  final Color color;
  final bool glow;
  final String? suffix;
  const Register(this.value, {super.key, this.width = 6, this.size = 16, this.color = C.a100, this.glow = false, this.suffix});

  @override
  Widget build(BuildContext context) {
    final s = math.max(0, value.floor()).toString();
    final w = math.max(width, s.length);
    final z = '0' * (w - s.length);
    final base = T.mono(size: size, color: color, weight: FontWeight.w500, spacing: size * .04);
    final st = glow ? base.copyWith(shadows: const [Shadow(color: Color(0x40FFC864), blurRadius: 6)]) : base;
    return Semantics(
      label: s,
      excludeSemantics: true,
      child: Text.rich(
        TextSpan(children: [
          if (z.isNotEmpty) TextSpan(text: z, style: st.copyWith(color: color.withValues(alpha: .28), shadows: const [])),
          TextSpan(text: s),
          if (suffix != null) TextSpan(text: suffix, style: st.copyWith(fontSize: size * .55, color: C.text3, shadows: const [])),
        ]),
        style: st,
        maxLines: 1,
      ),
    );
  }
}

/// LED bar graph: 3 px segments with 1 px gaps and a 10 % tick scale beneath.
class LedBar extends StatelessWidget {
  final double frac;
  final Color color;
  final double height;
  final bool ticks;
  final Color? mark;
  final double? markAt;
  final bool animate;
  const LedBar(this.frac,
      {super.key, this.color = C.a400, this.height = 8, this.ticks = true, this.mark, this.markAt, this.animate = true});

  @override
  Widget build(BuildContext context) {
    final f = frac.isNaN ? 0.0 : frac.clamp(0.0, 1.0);
    final anim = animate && !reducedMotion(context);
    return SizedBox(
      height: height + (ticks ? 4 : 0),
      child: anim
          ? TweenAnimationBuilder<double>(
              tween: Tween(end: f),
              duration: const Duration(milliseconds: 500),
              curve: const _Steps(14),
              builder: (_, v, _) => CustomPaint(painter: _LedPainter(v, color, height, ticks, mark, markAt), size: Size.infinite))
          : CustomPaint(painter: _LedPainter(f, color, height, ticks, mark, markAt), size: Size.infinite),
    );
  }
}

class _Steps extends Curve {
  final int n;
  const _Steps(this.n);
  @override
  double transformInternal(double t) => (t * n).floor() / n;
}

class _LedPainter extends CustomPainter {
  final double f;
  final Color color;
  final double h;
  final bool ticks;
  final Color? mark;
  final double? markAt;
  _LedPainter(this.f, this.color, this.h, this.ticks, this.mark, this.markAt);

  @override
  void paint(Canvas c, Size s) {
    final box = Rect.fromLTWH(0, 0, s.width, h);
    c.drawRect(box.deflate(.5), Paint()..color = const Color(0x0AFFB000));
    c.drawRect(box.deflate(.5), Paint()
      ..style = PaintingStyle.stroke
      ..color = C.line);
    final inner = box.deflate(2);
    final fill = inner.width * f;
    final p = Paint()..color = color;
    for (var x = 0.0; x < fill; x += 4) {
      c.drawRect(Rect.fromLTWH(inner.left + x, inner.top, math.min(3, fill - x), inner.height), p);
    }
    if (ticks) {
      final tp = Paint()..color = C.lineHi.withValues(alpha: .7);
      for (var i = 0; i <= 10; i++) {
        c.drawRect(Rect.fromLTWH(math.min(s.width - 1, s.width * i / 10), h + 1, 1, 3), tp);
      }
    }
    if (markAt != null) {
      c.drawRect(Rect.fromLTWH(s.width * markAt! - .5, -2, 1.5, h + 4), Paint()..color = mark ?? C.ember);
    }
  }

  @override
  bool shouldRepaint(_LedPainter o) => o.f != f || o.color != color || o.markAt != markAt || o.h != h;
}

/// Row of discrete cells (tick bar, threat bar).
class SegBar extends StatelessWidget {
  final int n;
  final int filled;
  final Color color;
  final double w;
  final double h;
  const SegBar(this.n, this.filled, {super.key, this.color = C.a400, this.w = 5, this.h = 7});
  @override
  Widget build(BuildContext context) => Row(mainAxisSize: MainAxisSize.min, children: [
        for (var i = 0; i < n; i++)
          Container(
            width: w,
            height: h,
            margin: EdgeInsets.only(right: i == n - 1 ? 0 : 1),
            decoration: BoxDecoration(
                color: i < filled ? color : Colors.transparent,
                border: Border.all(color: i < filled ? color : color.withValues(alpha: .4))),
          ),
      ]);
}

/// Threat rating: digit, nine cells, never colour alone.
class ThreatBadge extends StatelessWidget {
  final int t;
  final bool sm;
  final bool showBar;
  const ThreatBadge(this.t, {super.key, this.sm = false, this.showBar = true});
  @override
  Widget build(BuildContext context) {
    final col = C.threat(t);
    return Semantics(
      label: t == 0 ? 'Threat clear' : 'Threat $t of 9',
      excludeSemantics: true,
      child: Row(mainAxisSize: MainAxisSize.min, children: [
        Text(t == 0 ? '--' : 'T$t', style: T.cond(size: sm ? 11 : 13, color: col, weight: FontWeight.w700, spacing: .9)),
        if (showBar) ...[
          const SizedBox(width: 6),
          SegBar(9, t, color: col, w: sm ? 3 : 4, h: sm ? 7 : 9),
        ],
      ]),
    );
  }
}

/// Status lamp: 7 px square, filled and bloomed when on.
class Lamp extends StatelessWidget {
  final String label;
  final bool on;
  final Color color;
  final bool blink;
  const Lamp(this.label, {super.key, this.on = true, this.color = C.own, this.blink = false});
  @override
  Widget build(BuildContext context) {
    Widget sq(bool lit) => Container(
          width: 7,
          height: 7,
          decoration: BoxDecoration(
              color: lit ? color : Colors.transparent,
              border: Border.all(color: color),
              boxShadow: lit ? [BoxShadow(color: color.withValues(alpha: .6), blurRadius: 5)] : null),
        );
    return Row(mainAxisSize: MainAxisSize.min, children: [
      blink ? Blink(period: 11, onSteps: 6, builder: (_, v) => sq(on && v)) : sq(on),
      if (label.isNotEmpty) ...[
        const SizedBox(width: 5),
        Text(label.toUpperCase(), style: T.cond(size: 9, color: color, spacing: 1.4)),
      ],
    ]);
  }
}

/// A ratio verdict in the console's colours. The bands mirror the server's
/// `scaling::ratio_label` (fixtures/protocol/ratio_labels.json guards the
/// match); [RatioInfo.ofLabel] takes the server's own verdict.
class RatioInfo {
  final double r;
  final proto.RatioLabel verdict;
  final Color color;
  final int winPct;
  const RatioInfo(this.r, this.verdict, this.color, this.winPct);

  String get label => verdict.label;
  bool get deadly => verdict == proto.RatioLabel.deadly;

  /// Sim win rate (like-for-like fleets) behind each verdict.
  static RatioInfo ofLabel(double r, proto.RatioLabel v) => switch (v) {
        proto.RatioLabel.safe => RatioInfo(r, v, C.ratioSafe, 99),
        proto.RatioLabel.favourable => RatioInfo(r, v, C.ratioFav, 97),
        proto.RatioLabel.even => RatioInfo(r, v, C.ratioEven, 90),
        proto.RatioLabel.risky => RatioInfo(r, v, C.ratioRisky, 50),
        proto.RatioLabel.deadly => RatioInfo(r, v, C.ratioDeadly, 5),
      };

  /// Power against power, for sectors the server cannot preview (the one
  /// the fleet is in, or a far hop of a route).
  static RatioInfo of(double mine, double theirs) {
    final r = mine / math.max(1, theirs);
    return ofLabel(r, proto.RatioLabel.forRatio(r));
  }

  /// The server's `preview_move` verdict.
  static RatioInfo fromPreview(proto.MovePreview p) => ofLabel(p.ratio, p.label);
}

/// Raw ore left on a tile and when it refills, as the server reports it.
String reserveText(proto.TileReserve r) {
  if (r.maxUnits <= 0) return '';
  final refill = r.refillsInS;
  final eta = refill == null ? '' : ' REFILL ${refill ~/ 60}:${(refill % 60).toString().padLeft(2, '0')}';
  return '${r.units.round()}/${r.maxUnits.round()}$eta';
}

/// Ratio gauge: five zones (<0.9, 0.9-1.1, 1.1-1.5, 1.5-2.5, >=2.5) with a pointer.
class RatioGauge extends StatelessWidget {
  final double ratio;
  const RatioGauge(this.ratio, {super.key});
  @override
  Widget build(BuildContext context) => SizedBox(
        height: 28,
        child: CustomPaint(painter: _RatioPainter(ratio), size: Size.infinite),
      );
}

class _RatioPainter extends CustomPainter {
  final double r;
  _RatioPainter(this.r);
  @override
  void paint(Canvas c, Size s) {
    const zones = [(.225, C.ratioDeadly), (.05, C.ratioRisky), (.10, C.ratioEven), (.25, C.ratioFav), (.375, C.ratioSafe)];
    var x = 0.0;
    for (final z in zones) {
      final rect = Rect.fromLTWH(x, 6, s.width * z.$1, 8);
      c.drawRect(rect, Paint()..color = z.$2.withValues(alpha: .22));
      c.drawRect(rect.deflate(.5), Paint()
        ..style = PaintingStyle.stroke
        ..color = z.$2.withValues(alpha: .75));
      x += s.width * z.$1;
    }
    final px = (math.min(4.0, r) / 4) * s.width;
    c.drawPath(Path()..moveTo(px - 4, 0)..lineTo(px + 4, 0)..lineTo(px, 6.5)..close(), Paint()..color = C.a100);
    for (final m in const [(.225, '0.9'), (.375, '1.5'), (.625, '2.5')]) {
      final tp = TextPainter(text: TextSpan(text: m.$2, style: T.mono(size: 8.5, color: C.text3)), textDirection: TextDirection.ltr)..layout();
      tp.paint(c, Offset(s.width * m.$1 - tp.width / 2, 15));
    }
  }

  @override
  bool shouldRepaint(_RatioPainter o) => o.r != r;
}

/// 240-degree instrument dial: 24 ticks, warn arc, needle.
class Dial extends StatelessWidget {
  final double frac;
  final double warn;
  final String label;
  final String value;
  final Color color;
  final double width;
  const Dial(
      {super.key, required this.frac, this.warn = .2, required this.label, required this.value, this.color = C.a500, this.width = 112});

  @override
  Widget build(BuildContext context) {
    final f = (frac.isNaN ? 0.0 : frac).clamp(0.0, 1.0);
    final anim = !reducedMotion(context);
    return Semantics(
      label: '$label $value',
      excludeSemantics: true,
      child: SizedBox(
        width: width,
        height: width * 92 / 120,
        child: anim
            ? TweenAnimationBuilder<double>(
                tween: Tween(end: f),
                duration: const Duration(milliseconds: 700),
                curve: Curves.easeOutCubic,
                builder: (_, v, _) => CustomPaint(painter: _DialPainter(v, warn, label, value, color)))
            : CustomPaint(painter: _DialPainter(f, warn, label, value, color)),
      ),
    );
  }
}

class _DialPainter extends CustomPainter {
  final double f, warn;
  final String label, value;
  final Color color;
  _DialPainter(this.f, this.warn, this.label, this.value, this.color);

  @override
  void paint(Canvas c, Size size) {
    c.save();
    c.scale(size.width / 120);
    const cx = 60.0, cy = 56.0, R = 44.0, a0 = 150.0, sw = 240.0;
    Offset pt(double a, double r) => Offset(cx + r * math.cos(a * math.pi / 180), cy + r * math.sin(a * math.pi / 180));
    void arc(double f0, double f1, double r, Paint p) => c.drawArc(Rect.fromCircle(center: const Offset(cx, cy), radius: r),
        (a0 + sw * f0) * math.pi / 180, (f1 - f0) * sw * math.pi / 180, false, p);
    Paint stroke(Color col, double w, [double a = 1]) => Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = w
      ..color = col.withValues(alpha: a);
    arc(0, 1, R + 4, stroke(C.line, 1));
    final tick = stroke(C.a600, 1);
    for (var i = 0; i <= 24; i++) {
      final a = a0 + sw * i / 24;
      c.drawLine(pt(a, R), pt(a, R - (i % 4 == 0 ? 8 : 4)), tick);
    }
    arc(0, warn, R - 11, stroke(C.ember, 3));
    arc(warn, 1, R - 11, stroke(color, 1, .6));
    final ang = (a0 + sw * f) * math.pi / 180;
    c.save();
    c.translate(cx, cy);
    c.rotate(ang);
    c.drawLine(const Offset(-6, 0), const Offset(R - 14, 0), stroke(C.a100, 1.6));
    c.drawPath(Path()..moveTo(R - 14, -2)..lineTo(R - 6, 0)..lineTo(R - 14, 2)..close(), Paint()..color = C.a100);
    c.restore();
    c.drawCircle(const Offset(cx, cy), 4, Paint()..color = C.void_);
    c.drawCircle(const Offset(cx, cy), 4, stroke(C.a300, 1.2));
    final vt = TextPainter(text: TextSpan(text: value, style: T.mono(size: 9.5, color: C.a100)), textDirection: TextDirection.ltr)..layout();
    vt.paint(c, Offset(cx - vt.width / 2, 78 - vt.computeDistanceToActualBaseline(TextBaseline.alphabetic)));
    final lt = TextPainter(text: TextSpan(text: label, style: T.cond(size: 7.5, color: C.text3, weight: FontWeight.w400, spacing: 1.5)), textDirection: TextDirection.ltr)..layout();
    lt.paint(c, Offset(cx - lt.width / 2, 88 - lt.computeDistanceToActualBaseline(TextBaseline.alphabetic)));
    c.restore();
  }

  @override
  bool shouldRepaint(_DialPainter o) => o.f != f || o.value != value || o.color != color || o.warn != warn;
}

/// 40-tick bezel ring with a centre label (queue progress).
class TickRing extends StatelessWidget {
  final double frac;
  final String center;
  final double size;
  final Color color;
  const TickRing({super.key, required this.frac, this.center = '', this.size = 62, this.color = C.a400});
  @override
  Widget build(BuildContext context) => SizedBox(
        width: size,
        height: size,
        child: CustomPaint(painter: _RingPainter(frac.clamp(0.0, 1.0), center, color)),
      );
}

class _RingPainter extends CustomPainter {
  final double f;
  final String center;
  final Color color;
  _RingPainter(this.f, this.center, this.color);
  @override
  void paint(Canvas c, Size s) {
    final ctr = s.center(Offset.zero);
    final r = s.width / 2 - 2;
    for (var i = 0; i < 40; i++) {
      final a = -math.pi / 2 + i / 40 * 2 * math.pi;
      final on = i / 40 < f;
      c.drawLine(
          ctr + Offset(math.cos(a), math.sin(a)) * (r - (i % 5 == 0 ? 7 : 4)),
          ctr + Offset(math.cos(a), math.sin(a)) * r,
          Paint()
            ..strokeWidth = on ? 1.6 : 1
            ..color = on ? color : C.a(C.a500, .2));
    }
    if (center.isNotEmpty) {
      final tp = TextPainter(text: TextSpan(text: center, style: T.mono(size: 12, color: C.a100, weight: FontWeight.w500)), textDirection: TextDirection.ltr)..layout();
      tp.paint(c, ctr - Offset(tp.width / 2, tp.height / 2));
    }
  }

  @override
  bool shouldRepaint(_RingPainter o) => o.f != f || o.center != center || o.color != color;
}

/// A symbol drawn into a small box (lists, buttons).
class SymIcon extends StatelessWidget {
  final void Function(Canvas, Size) paint;
  final double size;
  const SymIcon(this.paint, {super.key, this.size = 18});
  @override
  Widget build(BuildContext context) => CustomPaint(size: Size(size, size), painter: _FnPainter(paint));
}

class _FnPainter extends CustomPainter {
  final void Function(Canvas, Size) fn;
  _FnPainter(this.fn);
  @override
  void paint(Canvas canvas, Size size) => fn(canvas, size);
  @override
  bool shouldRepaint(_FnPainter o) => true;
}
