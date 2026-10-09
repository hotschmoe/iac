import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../console/economy_view.dart';
import '../../console/services.dart';
import '../../console/toasts.dart';
import '../../design/panel.dart';
import '../../design/tokens.dart';
import '../../state/game_controller.dart';
import 'buildings_tab.dart';
import 'economy_tabs.dart';
import 'inspector.dart';
import 'queue_strip.dart';
import 'research_tab.dart';
import 'shipyard_tab.dart';
import 'spec.dart';

class HomeworldView extends StatefulWidget {
  const HomeworldView({super.key});

  @override
  State<HomeworldView> createState() => _HomeworldViewState();
}

class _HomeworldViewState extends State<HomeworldView> {
  VoidCallback? _unreg;
  Console? _con;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final con = Console.of(context);
    if (_con != con) {
      _unreg?.call();
      _con = con;
      _unreg = con.registerKeys(Screen.homeworld, _onKey);
    }
  }

  @override
  void dispose() {
    _unreg?.call();
    super.dispose();
  }

  GameController get _ctrl => _con!.game;

  void _select(int i) {
    _ctrl.selectHomeworldCard(i);
    _con!.play('click');
  }

  /// Queue card [i] (the selection by default). A card that cannot be queued
  /// says why in a toast instead of sending anything.
  void _queue([int? i]) {
    final c = _ctrl;
    final spec = specFor(c, index: i);
    if (spec == null) return;
    if (spec.reason != null) {
      _con!.toasts.show('Cannot queue', spec.reason!, tone: ToastTone.red);
      _con!.play('alert');
      return;
    }
    c.activateHomeworldCard(i);
    _con!.play('ok');
  }

  bool _onKey(KeyEvent e) {
    final c = _ctrl;
    final k = e.logicalKey;
    final tab = c.hwTab;
    final listTab = tab == HomeworldTab.buildings || tab == HomeworldTab.research || tab == HomeworldTab.shipyard;
    if (k == LogicalKeyboardKey.bracketLeft) {
      c.cycleHomeworldTab(-1);
    } else if (k == LogicalKeyboardKey.bracketRight) {
      c.cycleHomeworldTab(1);
    } else if (k == LogicalKeyboardKey.digit1 || k == LogicalKeyboardKey.digit2 || k == LogicalKeyboardKey.digit3 || k == LogicalKeyboardKey.digit4 || k == LogicalKeyboardKey.digit5) {
      c.selectHomeworldTab(HomeworldTab.values[int.parse(e.character ?? '1') - 1]);
    } else if (!listTab) {
      return false;
    } else if (k == LogicalKeyboardKey.arrowLeft || k == LogicalKeyboardKey.arrowUp && tab != HomeworldTab.shipyard) {
      c.moveHomeworldCursor(-1, 0);
    } else if (k == LogicalKeyboardKey.arrowRight || k == LogicalKeyboardKey.arrowDown && tab != HomeworldTab.shipyard) {
      c.moveHomeworldCursor(1, 0);
    } else if (k == LogicalKeyboardKey.arrowUp) {
      c.moveHomeworldCursor(0, -1);
    } else if (k == LogicalKeyboardKey.arrowDown) {
      c.moveHomeworldCursor(0, 1);
    } else if (k == LogicalKeyboardKey.enter || k == LogicalKeyboardKey.numpadEnter) {
      _queue();
    } else if (k == LogicalKeyboardKey.delete || k == LogicalKeyboardKey.backspace) {
      c.cancelCurrentHomeworldQueue();
    } else if (k == LogicalKeyboardKey.equal || k == LogicalKeyboardKey.add) {
      c.cycleShipBatch(1);
    } else if (k == LogicalKeyboardKey.minus || k == LogicalKeyboardKey.numpadSubtract) {
      c.cycleShipBatch(-1);
    } else {
      return false;
    }
    return true;
  }

  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([con.game, con.prefs]),
      builder: (context, _) {
        final c = con.game;
        final eco = con.economy;
        return LayoutBuilder(builder: (context, box) {
          final narrow = box.maxWidth < Bp.narrow;
          final strip = QueueStrip(state: c.state, eco: eco, ctrl: c, narrow: narrow);
          final tabs = _Tabs(ctrl: c, narrow: narrow);
          final main = _main(c, eco, con);
          final listTab = c.hwTab != HomeworldTab.storage;
          final insp = listTab ? Inspector(spec: specFor(c), ctrl: c, onQueue: _queue) : null;
          if (narrow) {
            return Container(
              color: C.deep,
              child: SingleChildScrollView(
                child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                  strip,
                  tabs,
                  main,
                  if (insp != null) Padding(padding: const EdgeInsets.all(10), child: insp),
                ]),
              ),
            );
          }
          return Container(
            color: C.deep,
            child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              strip,
              tabs,
              Expanded(
                child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                  Expanded(child: SingleChildScrollView(child: main)),
                  if (insp != null)
                    Container(
                      width: 380,
                      decoration: const BoxDecoration(border: Border(left: BorderSide(color: C.line))),
                      child: SingleChildScrollView(padding: const EdgeInsets.all(12), child: insp),
                    ),
                ]),
              ),
            ]),
          );
        });
      },
    );
  }

  Widget _main(GameController c, EconomyView eco, Console con) => switch (c.hwTab) {
        HomeworldTab.buildings => BuildingsTab(ctrl: c, onSelect: _select, prefs: con.prefs),
        HomeworldTab.research => ResearchTab(ctrl: c, onSelect: _select),
        HomeworldTab.shipyard => ShipyardTab(ctrl: c, onSelect: _select),
        HomeworldTab.defence => DefenceTab(eco: eco, ctrl: c, onSelect: _select),
        HomeworldTab.storage => StorageTab(eco: eco, state: c.state, ctrl: c),
      };
}

class _Tabs extends StatelessWidget {
  final GameController ctrl;
  final bool narrow;
  const _Tabs({required this.ctrl, required this.narrow});

  @override
  Widget build(BuildContext context) {
    final items = [
      for (final t in HomeworldTab.values)
        _Tab(
          key: ValueKey('hw-tab-${t.name}'),
          label: t.label,
          hint: '${t.index + 1}',
          on: ctrl.hwTab == t,
          narrow: narrow,
          onTap: () => ctrl.selectHomeworldTab(t),
        ),
    ];
    return Container(
      decoration: const BoxDecoration(border: Border(bottom: BorderSide(color: C.line))),
      child: Row(children: [for (final t in items) narrow ? Expanded(child: t) : t]),
    );
  }
}

class _Tab extends StatefulWidget {
  final String label, hint;
  final bool on, narrow;
  final VoidCallback onTap;
  const _Tab({super.key, required this.label, required this.hint, required this.on, required this.narrow, required this.onTap});
  @override
  State<_Tab> createState() => _TabState();
}

class _TabState extends State<_Tab> {
  bool _hover = false, _focus = false;
  @override
  Widget build(BuildContext context) {
    final on = widget.on;
    final col = on ? const Color(0xFF1C1000) : (_hover || _focus) ? C.a300 : C.text3;
    return Semantics(
      button: true,
      selected: on,
      label: widget.label,
      excludeSemantics: true,
      onTap: widget.onTap,
      child: FocusableActionDetector(
        mouseCursor: SystemMouseCursors.click,
        onShowHoverHighlight: (v) => setState(() => _hover = v),
        onShowFocusHighlight: (v) => setState(() => _focus = v),
        actions: {ActivateIntent: CallbackAction<ActivateIntent>(onInvoke: (_) {
          widget.onTap();
          return null;
        })},
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: widget.onTap,
          child: Container(
            padding: EdgeInsets.symmetric(horizontal: widget.narrow ? 2 : 18, vertical: 9),
            decoration: BoxDecoration(
              color: on ? C.a500 : (_hover ? C.a(C.a500, .05) : null),
              border: Border(right: const BorderSide(color: C.lineLo), bottom: _focus && !on ? const BorderSide(color: C.a100, width: 2) : BorderSide.none),
            ),
            child: Row(mainAxisAlignment: MainAxisAlignment.center, mainAxisSize: MainAxisSize.min, children: [
              Flexible(child: Text(widget.label, overflow: TextOverflow.ellipsis, style: T.cond(size: widget.narrow ? 10 : 12, color: col, spacing: widget.narrow ? .8 : 2.4))),
              if (!widget.narrow) ...[const SizedBox(width: 8), KeyCap(widget.hint, color: col, size: 15)],
            ]),
          ),
        ),
      ),
    );
  }
}
