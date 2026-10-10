import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../console/onboarding.dart';
import '../console/services.dart';
import '../console/toasts.dart';
import '../design/beat.dart';
import '../design/gauges.dart';
import '../design/panel.dart';
import '../design/tokens.dart';
import 'overview/ov_panel.dart';

void _skip(Console con) {
  con.onboarding.setActive(false);
  con.toasts.show('Guide off', 'Turn it back on under Keys and settings (?).');
}

/// The guide's one line: the lesson the player is on, as a procedure strip
/// under the HUD. It takes its own row, so it never covers a control, and it
/// is gone when the guide is off or through.
class GuideStrip extends StatelessWidget {
  final Screen screen;
  final bool narrow;
  const GuideStrip({super.key, required this.screen, required this.narrow});

  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    final guide = con.onboarding;
    return ListenableBuilder(
      listenable: Listenable.merge([guide, con.game]),
      builder: (context, _) {
        final l = guide.current;
        if (l == null) return const SizedBox.shrink();
        final steps = guide.guided;
        final note = guide.note(l, screen);
        final token = l == Lesson.token ? con.game.issuedToken?.token : null;
        final elsewhere = l.screen != null && l.screen != screen;

        final head = Row(mainAxisSize: MainAxisSize.min, children: [
          const Lamp('', color: C.a400, blink: true),
          const SizedBox(width: 8),
          Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
            Lbl('Operator guide  ${steps.indexOf(l) + 1}/${steps.length}', size: 9),
            const SizedBox(height: 3),
            SegBar(steps.length, steps.where(guide.done).length, w: 9, h: 4),
          ]),
        ]);
        final text = Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(l.title.toUpperCase(), style: T.cond(size: 13, color: C.a100, spacing: 1.4)),
          const SizedBox(height: 2),
          Text.rich(
            TextSpan(children: [
              TextSpan(text: 'NOTE ', style: T.mono(size: 11, color: C.a700, weight: FontWeight.w600, spacing: 1)),
              TextSpan(text: note),
            ]),
            style: T.mono(size: 11, color: C.text2, height: 1.4),
          ),
        ]);
        final buttons = <Widget>[
          if (l.keys != null && !narrow && !elsewhere) for (final k in l.keys!.split(' ')) KeyCap(k),
          if (token != null) ...[
            ConsoleButton('Copy token', key: const Key('guide-copy'), small: true, primary: true, onPressed: () {
              Clipboard.setData(ClipboardData(text: token));
              con.toasts.show('Account token', 'Copied. Keep it somewhere safe.', tone: ToastTone.teal);
            }),
            ConsoleButton('I have saved it', key: const Key('guide-token-saved'), small: true, onPressed: guide.tokenSaved),
          ],
          if (elsewhere) ConsoleButton('Open ${l.screen!.name}', key: const Key('guide-go'), small: true, primary: true, onPressed: () => con.go(l.screen!)),
          if (l == Lesson.orders)
            ConsoleButton('Set mine + return', key: const Key('guide-orders'), small: true, primary: true, onPressed: () => con.openPalette('policy mine')),
          ConsoleButton('Skip guide', key: const Key('guide-skip'), small: true, onPressed: () => _skip(con)),
        ];
        final actions = Wrap(spacing: 6, runSpacing: 4, crossAxisAlignment: WrapCrossAlignment.center, children: buttons);
        final tokenBox = token == null
            ? null
            : Container(
                margin: const EdgeInsets.only(top: 6),
                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 5),
                decoration: BoxDecoration(color: C.inset, border: Border.all(color: C.line)),
                child: SelectableText(token, key: const Key('guide-token'), style: T.mono(size: 11.5, color: C.a100, spacing: .4)),
              );

        return Semantics(
          container: true,
          label: 'Operator guide, step ${steps.indexOf(l) + 1} of ${steps.length}. ${l.title}. $note',
          child: Container(
            key: const Key('guide-strip'),
            padding: EdgeInsets.fromLTRB(narrow ? 9 : 13, 7, narrow ? 8 : 12, 7),
            decoration: const BoxDecoration(
              color: Color(0xFF100B05),
              border: Border(left: BorderSide(color: C.a500, width: 3), bottom: BorderSide(color: C.lineHi)),
            ),
            child: narrow
                ? Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
                    head,
                    const SizedBox(height: 6),
                    text,
                    ?tokenBox,
                    const SizedBox(height: 6),
                    actions,
                  ])
                : Row(children: [
                    head,
                    const SizedBox(width: 16),
                    Expanded(child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [text, ?tokenBox])),
                    const SizedBox(width: 14),
                    ConstrainedBox(constraints: const BoxConstraints(maxWidth: 420), child: actions),
                  ]),
          ),
        );
      },
    );
  }
}

/// Wraps the control a lesson points at. While that lesson is current the
/// control carries stepped corner brackets; the child is never rebuilt or
/// moved for it.
class GuideTarget extends StatelessWidget {
  final String id;
  final Widget child;
  const GuideTarget(this.id, {super.key, required this.child});

  @override
  Widget build(BuildContext context) {
    final guide = Console.of(context).onboarding;
    final still = reducedMotion(context);
    return ListenableBuilder(
      listenable: guide,
      child: child,
      builder: (context, child) => CustomPaint(foregroundPainter: guide.target == id ? GuideBrackets(still: still) : null, child: child),
    );
  }
}

/// Corner brackets around the marked control, flicking on the shared 1 s
/// step. They sit just outside it unless [inset] pulls them in.
class GuideBrackets extends CustomPainter {
  final bool still;
  final double inset;
  GuideBrackets({this.still = false, this.inset = -4}) : super(repaint: still ? null : Beat.instance);

  @override
  void paint(Canvas canvas, Size size) {
    final lit = still || Beat.instance.step % 10 < 6;
    final r = (Offset.zero & size).deflate(inset);
    final arm = (size.shortestSide * .3).clamp(6.0, 14.0);
    final p = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2
      ..strokeCap = StrokeCap.butt
      ..color = lit ? C.a100 : C.a(C.a500, .4);
    for (final (corner, dx, dy) in [(r.topLeft, 1.0, 1.0), (r.topRight, -1.0, 1.0), (r.bottomRight, -1.0, -1.0), (r.bottomLeft, 1.0, -1.0)]) {
      canvas.drawPath(
          Path()
            ..moveTo(corner.dx + dx * arm, corner.dy)
            ..lineTo(corner.dx, corner.dy)
            ..lineTo(corner.dx, corner.dy + dy * arm),
          p);
    }
  }

  @override
  bool shouldRepaint(GuideBrackets old) => old.still != still || old.inset != inset;
}

/// The first-hour list, pinned on the Overview once the guided lessons are
/// through and until the player closes it: every lesson with a lamp that
/// lights when the game shows it done.
class FirstHourCard extends StatelessWidget {
  final double bottomGap;
  const FirstHourCard({super.key, required this.bottomGap});

  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    final guide = con.onboarding;
    return ListenableBuilder(
      listenable: guide,
      builder: (context, _) {
        if (!guide.active || guide.current != null) return const SizedBox.shrink();
        final items = guide.listed;
        final learned = items.where(guide.done).length;
        final through = learned == items.length;
        return Padding(
          padding: EdgeInsets.only(bottom: bottomGap),
          child: OvPanel(
            key: const Key('first-hour'),
            title: 'First hour',
            subText: '$learned/${items.length} done',
            padding: const EdgeInsets.fromLTRB(14, 10, 14, 10),
            footer: Align(
              alignment: Alignment.centerLeft,
              child: ConsoleButton(through ? 'Close' : 'Hide guide', key: const Key('first-hour-hide'), small: true, onPressed: () => _skip(con)),
            ),
            child: LayoutBuilder(builder: (context, box) {
              final cols = box.maxWidth >= 1040 ? 4 : box.maxWidth >= 700 ? 3 : box.maxWidth >= 440 ? 2 : 1;
              final w = (box.maxWidth - (cols - 1) * 14) / cols;
              return Wrap(spacing: 14, runSpacing: 8, children: [
                for (final l in items) SizedBox(width: w, child: _item(l, guide.done(l))),
              ]);
            }),
          ),
        );
      },
    );
  }

  Widget _item(Lesson l, bool done) => Semantics(
        label: '${l.title}: ${done ? 'done' : 'to do'}',
        excludeSemantics: true,
        child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Padding(padding: const EdgeInsets.only(top: 3), child: Lamp('', on: done, color: done ? C.own : C.a600)),
          const SizedBox(width: 8),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Row(children: [
                Flexible(
                    child: Text(l.title.toUpperCase(),
                        overflow: TextOverflow.ellipsis, style: T.cond(size: 11.5, color: done ? C.text3 : C.text, spacing: 1.1))),
                if (done) ...[const SizedBox(width: 8), const Tag('done', color: C.own)],
              ]),
              if (!done) Text(l.hint, style: T.mono(size: 10, color: C.text3, height: 1.35)),
            ]),
          ),
        ]),
      );
}
