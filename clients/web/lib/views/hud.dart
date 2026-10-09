import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../console/economy_view.dart';
import '../models/game_state.dart';
import '../console/services.dart';
import '../design/beat.dart';
import '../design/gauges.dart';
import '../design/icons.dart';
import '../design/panel.dart';
import '../design/tokens.dart';

class Hud extends StatelessWidget {
  final bool narrow;
  final bool alertsOpen;
  final VoidCallback onSound;
  final VoidCallback onHelp;
  final VoidCallback onAlerts;
  const Hud({super.key, required this.narrow, required this.alertsOpen, required this.onSound, required this.onHelp, required this.onAlerts});

  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    return Container(
      height: narrow ? 46 : 56,
      decoration: const BoxDecoration(color: C.hud, border: Border(bottom: BorderSide(color: C.lineHi))),
      child: ListenableBuilder(
        listenable: Listenable.merge([con.game, con.prefs]),
        builder: (context, _) {
          final eco = con.economy;
          final alerts = con.state.alerts.where((a) => a.level == AlertTone.glow).length;
          final buttons = [
            _IconBtn(con.prefs.sound ? 'sound' : 'soundoff', con.prefs.sound ? 'Sound on (M)' : 'Sound off (M)', onSound, active: con.prefs.sound),
            _IconBtn('help', 'Keys and settings (?)', onHelp),
            _IconBtn('bell', 'Alerts', onAlerts, dot: alerts > 0, active: alertsOpen),
          ];
          return Row(children: [
            Container(
              padding: EdgeInsets.symmetric(horizontal: narrow ? 8 : 14),
              decoration: const BoxDecoration(border: Border(right: BorderSide(color: C.lineLo))),
              alignment: Alignment.center,
              child: Row(children: [
                CustomPaint(size: const Size(26, 26), painter: _LogoPainter()),
                if (!narrow) ...[
                  const SizedBox(width: 10),
                  Column(mainAxisAlignment: MainAxisAlignment.center, crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text('IN AMBER CLAD', style: T.cond(size: 14, color: C.a300, weight: FontWeight.w700, spacing: 3.4)),
                    const SizedBox(height: 2),
                    Text('OPERATOR CONSOLE / MOD 4', style: T.cond(size: 8.5, color: C.text3, weight: FontWeight.w400, spacing: 1.8)),
                  ]),
                ],
              ]),
            ),
            Expanded(
              child: Row(children: [
                for (final k in const ['metal', 'crystal', 'deut']) Expanded(child: _ResChip(k, eco, narrow)),
              ]),
            ),
            if (!narrow) ...[
              _Field('World', con.game.isLive || con.game.isDemo ? (eco.world?.label ?? (con.game.isDemo ? 'DEMO' : 'LIVE')) : con.game.linkLabel, alert: !con.game.isLive && !con.game.isDemo),
              _TickField(),
              ...buttons.map((b) => Padding(padding: const EdgeInsets.symmetric(horizontal: 3), child: b)),
              _Who(eco),
            ] else
              ...buttons.map((b) => Padding(padding: const EdgeInsets.symmetric(horizontal: 1), child: b)),
            if (narrow) const SizedBox(width: 4),
          ]);
        },
      ),
    );
  }
}

class _LogoPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    canvas.save();
    canvas.scale(size.width / 32);
    final p = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.4
      ..strokeJoin = StrokeJoin.miter;
    canvas.drawPath(Path()..moveTo(16, 2.5)..lineTo(28, 9.5)..lineTo(28, 22.5)..lineTo(16, 29.5)..lineTo(4, 22.5)..lineTo(4, 9.5)..close(), p..color = C.a500);
    canvas.drawPath(Path()..moveTo(9, 22)..lineTo(16, 8)..lineTo(23, 22)..moveTo(12, 17)..lineTo(20, 17), p..color = C.a200);
    canvas.restore();
  }

  @override
  bool shouldRepaint(_LogoPainter o) => false;
}

class _Field extends StatelessWidget {
  final String label, value;
  final bool alert;
  const _Field(this.label, this.value, {this.alert = false});
  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.symmetric(horizontal: 12),
        decoration: const BoxDecoration(border: Border(left: BorderSide(color: C.lineLo))),
        child: Column(mainAxisAlignment: MainAxisAlignment.center, crossAxisAlignment: CrossAxisAlignment.start, children: [
          Lbl(label, size: 9),
          const SizedBox(height: 2),
          Text(value, style: T.mono(size: 13, color: alert ? C.ember : C.a200, spacing: .5)),
        ]),
      );
}

class _TickField extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12),
      decoration: const BoxDecoration(border: Border(left: BorderSide(color: C.lineLo))),
      child: Column(mainAxisAlignment: MainAxisAlignment.center, crossAxisAlignment: CrossAxisAlignment.start, children: [
        const Lbl('Tick', size: 9),
        const SizedBox(height: 2),
        Register(con.state.tick, size: 13),
        const SizedBox(height: 3),
        Blink(period: 1, onSteps: 1, builder: (_, _) {
          final n = math.min(10, (con.sinceTick * 10).floor() + 1);
          return SegBar(10, n, w: 5, h: 5, color: C.a400);
        }),
      ]),
    );
  }
}

class _Who extends StatelessWidget {
  final EconomyView eco;
  const _Who(this.eco);
  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    final hw = con.state.homeworld;
    final zone = hw.distFromOrigin == 0
        ? 'Hub'
        : hw.distFromOrigin <= 8
            ? 'Inner Ring'
            : hw.distFromOrigin <= 20
                ? 'Outer Ring'
                : 'Wandering';
    final rank = eco.rank;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 14),
      decoration: const BoxDecoration(border: Border(left: BorderSide(color: C.lineLo))),
      child: Column(mainAxisAlignment: MainAxisAlignment.center, crossAxisAlignment: CrossAxisAlignment.end, children: [
        Text(con.game.playerName.isEmpty ? 'DEMO' : con.game.playerName.toUpperCase(),
            style: T.cond(size: 11, color: C.a300, spacing: 1.3)),
        Text(rank == null ? zone.toUpperCase() : '${zone.toUpperCase()} - RANK ${rank.rank.toString().padLeft(3, '0')}',
            style: T.cond(size: 11, color: C.text2, weight: FontWeight.w400, spacing: 1.3)),
      ]),
    );
  }
}

class _IconBtn extends StatefulWidget {
  final String icon, tip;
  final VoidCallback onTap;
  final bool dot, active;
  const _IconBtn(this.icon, this.tip, this.onTap, {this.dot = false, this.active = false});
  @override
  State<_IconBtn> createState() => _IconBtnState();
}

class _IconBtnState extends State<_IconBtn> {
  bool _hover = false;
  @override
  Widget build(BuildContext context) {
    final on = _hover || widget.active;
    return Tooltip(
      message: widget.tip,
      child: Semantics(
        button: true,
        label: widget.tip,
        excludeSemantics: true,
        onTap: widget.onTap,
        child: FocusableActionDetector(
          mouseCursor: SystemMouseCursors.click,
          onShowHoverHighlight: (v) => setState(() => _hover = v),
          actions: {ActivateIntent: CallbackAction<ActivateIntent>(onInvoke: (_) {
            widget.onTap();
            return null;
          })},
          child: GestureDetector(
            onTap: widget.onTap,
            child: Container(
              width: 32,
              height: 32,
              alignment: Alignment.center,
              decoration: BoxDecoration(
                  border: Border.all(color: on ? C.a500 : C.line), color: on ? C.a(C.a500, .10) : null),
              child: Stack(clipBehavior: Clip.none, alignment: Alignment.center, children: [
                LineIcon(widget.icon, size: 16, color: on ? C.a100 : C.text2),
                if (widget.dot)
                  Positioned(
                      top: -9,
                      right: -9,
                      child: Blink(period: 10, onSteps: 5, builder: (_, v) => Opacity(opacity: v ? 1 : .35, child: Container(width: 6, height: 6, color: C.ember)))),
              ]),
            ),
          ),
        ),
      ),
    );
  }
}

class _ResChip extends StatelessWidget {
  final String k;
  final EconomyView eco;
  final bool narrow;
  const _ResChip(this.k, this.eco, this.narrow);

  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    final st = con.state;
    final (label, color, name, stock, rate) = switch (k) {
      'metal' => ('Fe', C.metal, 'Metal', st.stock.metal, st.resources.metal.rate),
      'crystal' => ('Cr', C.crystal, 'Crystal', st.stock.crystal, st.resources.crystal.rate),
      _ => ('De', C.deut, 'Deuterium', st.stock.deuterium, st.resources.deut.rate),
    };
    final cap = eco.storage?.cap[k];
    return Container(
      padding: EdgeInsets.symmetric(horizontal: narrow ? 6 : 14),
      decoration: const BoxDecoration(border: Border(right: BorderSide(color: C.lineLo))),
      constraints: BoxConstraints(minWidth: narrow ? 0 : 168),
      child: Blink(
        period: 1,
        onSteps: 1,
        builder: (context, _) {
          final dt = math.min(1.2, con.sinceTick);
          var v = stock + rate * dt;
          if (cap != null) v = math.min(v, cap);
          final full = cap != null && v >= cap * .98;
          final near = cap != null && v >= cap * .85;
          return Semantics(
            label: '$name ${v.floor()}${cap != null ? ' of ${cap.floor()}' : ''}',
            excludeSemantics: true,
            child: Column(mainAxisAlignment: MainAxisAlignment.center, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              Row(crossAxisAlignment: CrossAxisAlignment.baseline, textBaseline: TextBaseline.alphabetic, children: [
                if (!narrow) ...[
                  SizedBox(width: 26, child: Text(label.toUpperCase(), style: T.cond(size: 9.5, color: color, spacing: 1.5))),
                ],
                Flexible(
                    child: FittedBox(
                        fit: BoxFit.scaleDown,
                        alignment: Alignment.centerLeft,
                        child: Register(v, size: narrow ? 13 : 16, glow: true, color: full ? C.ember : C.a100))),
                if (!narrow) ...[
                  const Spacer(),
                  Text('+${rate.toStringAsFixed(1)}/T', style: T.mono(size: 10, color: C.text3)),
                ],
              ]),
              SizedBox(height: narrow ? 1 : 3),
              if (cap != null)
                Blink(
                    period: 12,
                    onSteps: 7,
                    builder: (_, on) => LedBar(v / cap, color: full && !on ? C.ember.withValues(alpha: .4) : near ? C.ember : color, height: narrow ? 4 : 6))
              else
                Container(height: narrow ? 4 : 6, decoration: const BoxDecoration(border: Border(bottom: BorderSide(color: C.lineLo)))),
            ]),
          );
        },
      ),
    );
  }
}
