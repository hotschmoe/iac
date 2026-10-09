import 'dart:async';

import 'package:clock/clock.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../console/prefs.dart';
import '../console/services.dart';
import '../console/toasts.dart';
import '../design/beat.dart';
import '../design/lens.dart';
import '../design/panel.dart';
import '../design/tokens.dart';
import '../models/game_state.dart';

Color _toneColor(ToastTone t) => switch (t) {
      ToastTone.amber => C.a500,
      ToastTone.red => C.ember,
      ToastTone.teal => C.own,
      ToastTone.violet => C.rare,
    };

class ToastLayer extends StatelessWidget {
  final ToastController toasts;
  final bool narrow;
  const ToastLayer({super.key, required this.toasts, required this.narrow});

  @override
  Widget build(BuildContext context) => Positioned(
        right: 16,
        bottom: narrow ? 70 : 16,
        left: narrow ? 16 : null,
        child: ListenableBuilder(
          listenable: toasts,
          builder: (context, _) => Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [for (final t in toasts.items) Padding(padding: const EdgeInsets.only(top: 6), child: _ToastTile(key: ValueKey(t.id), t: t, toasts: toasts))],
          ),
        ),
      );
}

class _ToastTile extends StatefulWidget {
  final Toast t;
  final ToastController toasts;
  const _ToastTile({super.key, required this.t, required this.toasts});
  @override
  State<_ToastTile> createState() => _ToastTileState();
}

class _ToastTileState extends State<_ToastTile> with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(vsync: this, duration: const Duration(milliseconds: 350))..forward();
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _timer = Timer(widget.t.until.difference(clock.now()), () {
      if (mounted) widget.toasts.dismiss(widget.t.id);
    });
  }

  @override
  void dispose() {
    _timer?.cancel();
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final col = _toneColor(widget.t.tone);
    return Semantics(
      liveRegion: true,
      label: '${widget.t.title}. ${widget.t.message}',
      excludeSemantics: true,
      child: AnimatedBuilder(
        animation: _c,
        builder: (_, child) => ClipRect(
            child: Align(alignment: Alignment.centerLeft, widthFactor: reducedMotion(context) ? 1 : (_c.value * 22).floor() / 22, child: child)),
        child: Container(
          constraints: const BoxConstraints(maxWidth: 340),
          padding: const EdgeInsets.fromLTRB(12, 7, 12, 7),
          decoration: BoxDecoration(
              color: const Color(0xFF0B0805), border: Border(left: BorderSide(color: col, width: 3), top: const BorderSide(color: C.lineHi), right: const BorderSide(color: C.lineHi), bottom: const BorderSide(color: C.lineHi))),
          child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(widget.t.title.toUpperCase(), style: T.cond(size: 10, color: C.a300, spacing: 1.6)),
            const SizedBox(height: 2),
            Text(widget.t.message, style: T.mono(size: 11, color: C.text, height: 1.35)),
          ]),
        ),
      ),
    );
  }
}

class HelpOverlay extends StatelessWidget {
  final Console con;
  final FrameGovernor governor;
  final VoidCallback onClose;
  const HelpOverlay({super.key, required this.con, required this.governor, required this.onClose});

  Widget _row(String keys, String text) => Padding(
        padding: const EdgeInsets.symmetric(vertical: 3),
        child: Row(children: [
          SizedBox(width: 108, child: Wrap(spacing: 4, children: [for (final k in keys.split(' ')) KeyCap(k)])),
          Expanded(child: Text(text, style: T.mono(size: 11.5, color: C.text2))),
        ]),
      );

  Widget _rule() => Container(margin: const EdgeInsets.symmetric(vertical: 8), height: 1, color: C.line);

  @override
  Widget build(BuildContext context) {
    return Positioned.fill(
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onClose,
        child: Container(
          color: const Color(0xD1050302),
          alignment: Alignment.center,
          child: GestureDetector(
            onTap: () {},
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 520, maxHeight: 640),
              child: ConsolePanel(
                title: 'Key reference',
                subText: '? or Esc to close',
                child: SingleChildScrollView(
                  padding: const EdgeInsets.fromLTRB(20, 10, 20, 16),
                  child: ListenableBuilder(
                    listenable: con.prefs,
                    builder: (context, _) {
                      final p = con.prefs;
                      return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                        const Lbl('Settings'),
                        const SizedBox(height: 6),
                        Wrap(spacing: 6, runSpacing: 6, children: [
                          ConsoleButton('Sound ${p.sound ? 'on' : 'off'}', small: true, active: p.sound, onPressed: () => p.setSound(!p.sound)),
                          ConsoleButton('Jump retrace ${p.retrace ? 'on' : 'off'}', small: true, active: p.retrace, onPressed: () => p.setRetrace(!p.retrace)),
                          ConsoleButton('Scanlines ${p.scanlines ? 'on' : 'off'}', small: true, active: p.scanlines, onPressed: () => p.setScanlines(!p.scanlines)),
                          ConsoleButton('Reduce motion ${p.reduceMotion ? 'on' : 'off'}', small: true, active: p.reduceMotion, onPressed: () => p.setReduceMotion(!p.reduceMotion)),
                        ]),
                        const SizedBox(height: 8),
                        Wrap(spacing: 6, runSpacing: 6, crossAxisAlignment: WrapCrossAlignment.center, children: [
                          const Lbl('Quality'),
                          ConsoleButton('Auto', small: true, active: p.pinned == null, onPressed: () => p.setQuality(null)),
                          for (final q in Quality.values)
                            ConsoleButton(q.name, small: true, active: p.pinned == q, onPressed: () => p.setQuality(q)),
                        ]),
                        const SizedBox(height: 8),
                        Text('${governor.summary}  ${p.quality.name.toUpperCase()}', style: T.mono(size: 10, color: C.text3)),
                        _rule(),
                        _row('G O', 'Overview'),
                        _row('G W', 'Windshield'),
                        _row('G M', 'Map'),
                        _row('G H', 'Homeworld'),
                        _row('/', 'Command palette'),
                        _row('1 - 9', 'Select fleet'),
                        _rule(),
                        _row('Q W E', 'Jump through gate NW, N, NE'),
                        _row('A S D', 'Jump SW, S, SE'),
                        _row('V', 'Scan'),
                        _row('H', 'Harvest (then pick resource)'),
                        _row('F', 'Fire; press again on bad odds'),
                        _row('C', 'Collect salvage'),
                        _row('B', 'Board derelict'),
                        _row('R X', 'Recall home, stop'),
                        _rule(),
                        _row('Click', 'Map: plan a route'),
                        _row('Shift Click', 'Map: add waypoint'),
                        _row('Enter', 'Map: engage route'),
                        _row('Esc', 'Cancel or close'),
                        _row('M', 'Sound on or off'),
                      ]);
                    },
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class AlertsPopover extends StatelessWidget {
  final Console con;
  final bool narrow;
  final VoidCallback onClose;
  const AlertsPopover({super.key, required this.con, required this.narrow, required this.onClose});

  @override
  Widget build(BuildContext context) => Positioned.fill(
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: onClose,
          child: Align(
            alignment: Alignment.topRight,
            child: Padding(
              padding: EdgeInsets.only(top: narrow ? 50 : 60, right: 8, left: 8),
              child: GestureDetector(
                onTap: () {},
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 360, maxHeight: 420),
                  child: ListenableBuilder(
                    listenable: con.game,
                    builder: (context, _) => ConsolePanel(
                      title: 'Alerts',
                      subText: 'Esc to close',
                      child: ListView(shrinkWrap: true, padding: const EdgeInsets.all(8), children: [
                        for (final a in con.state.alerts) _AlertRow(a),
                      ]),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      );
}

class _AlertRow extends StatelessWidget {
  final Alert a;
  const _AlertRow(this.a);
  @override
  Widget build(BuildContext context) {
    final hot = a.level == AlertTone.glow;
    return Container(
      margin: const EdgeInsets.only(bottom: 6),
      padding: const EdgeInsets.all(8),
      decoration: BoxDecoration(border: Border(left: BorderSide(color: hot ? C.ember : C.a700, width: 3), top: const BorderSide(color: C.lineLo), right: const BorderSide(color: C.lineLo), bottom: const BorderSide(color: C.lineLo))),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text(a.message.toUpperCase(), style: T.cond(size: 11, color: hot ? C.ember : C.a300, spacing: 1.2)),
        if (a.detail.isNotEmpty) Text(a.detail, style: T.mono(size: 10.5, color: C.text2)),
      ]),
    );
  }
}

/// `/` command palette. Closes on submit and Esc so hotkeys work again.
class CommandPalette extends StatefulWidget {
  final void Function(String) onSubmit;
  final VoidCallback onClose;
  final String initial;
  final bool narrow;
  const CommandPalette({super.key, required this.onSubmit, required this.onClose, this.initial = '', required this.narrow});
  @override
  State<CommandPalette> createState() => _CommandPaletteState();
}

class _CommandPaletteState extends State<CommandPalette> {
  late final TextEditingController _t = TextEditingController(text: widget.initial);
  final FocusNode _f = FocusNode();

  @override
  void dispose() {
    _t.dispose();
    _f.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Positioned(
      left: widget.narrow ? 8 : 90,
      right: widget.narrow ? 8 : 16,
      bottom: widget.narrow ? 62 : 16,
      child: ConsolePanel(
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          child: Row(children: [
            Text('/', style: T.mono(size: 14, color: C.a500, weight: FontWeight.w600)),
            const SizedBox(width: 8),
            Expanded(
              child: Focus(
                onKeyEvent: (n, e) {
                  if (e is KeyDownEvent && e.logicalKey == LogicalKeyboardKey.escape) {
                    widget.onClose();
                    return KeyEventResult.handled;
                  }
                  return KeyEventResult.ignored;
                },
                child: TextField(
                  key: const Key('palette-field'),
                  controller: _t,
                  focusNode: _f,
                  autofocus: true,
                  style: T.mono(size: 13, color: C.a100),
                  cursorColor: C.a500,
                  decoration: InputDecoration(
                      isDense: true,
                      border: InputBorder.none,
                      hintText: 'scan, move 5,-3, harvest metal, ws, map, hw ...',
                      hintStyle: T.mono(size: 13, color: C.text4)),
                  onSubmitted: (v) {
                    widget.onSubmit(v);
                  },
                ),
              ),
            ),
            const KeyCap('Esc'),
          ]),
        ),
      ),
    );
  }
}
