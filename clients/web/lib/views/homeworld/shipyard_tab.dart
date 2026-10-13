import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../console/intel.dart';
import '../../design/panel.dart';
import '../../design/tokens.dart';
import '../../protocol/protocol.dart' as proto;
import '../../scenes/homeworld/plan.dart';
import '../../state/game_controller.dart';
import '../../state/state_mapper.dart' show amountsOf;
import 'spec.dart';

class ShipyardTab extends StatelessWidget {
  final GameController ctrl;
  final ValueChanged<int> onSelect;
  const ShipyardTab({super.key, required this.ctrl, required this.onSelect});

  @override
  Widget build(BuildContext context) {
    final cat = ctrl.state.catalog;
    if (cat == null) return const SizedBox.shrink();
    return LayoutBuilder(builder: (context, box) {
      final cols = math.max(1, math.min(3, (box.maxWidth / 250).floor()));
      ctrl.hwColumns = cols;
      const gap = 10.0;
      final cw = (box.maxWidth - 24 - (cols - 1) * gap) / cols;
      return Padding(
        padding: const EdgeInsets.all(12),
        child: Wrap(spacing: gap, runSpacing: gap, children: [
          for (var i = 0; i < cat.ships.length; i++) SizedBox(width: cw, child: _card(i, cat.ships[i])),
        ]),
      );
    });
  }

  /// The second line of a card: what the class is for, in its own figures.
  String _role(proto.ShipOption o) {
    final hw = ctrl.state.hw;
    if (!o.flies) return 'STAYS AT HOME${hw == null ? '' : '   +${hw.energy.perSatellite} ENERGY EACH   ${hw.satellites} AT HOME'}';
    return 'POWER ${groupDigits(o.power.round())}   JUMP ${o.jumpTicks}T   FUEL ${trimmed(o.jumpFuel, 1)}';
  }

  Widget _card(int i, proto.ShipOption o) {
    final locked = o.requires.any((r) => !r.met);
    final sel = ctrl.hwCursor == i;
    final spec = specFor(ctrl, index: i);
    final blocked = locked ? null : spec?.reason;
    return Semantics(
      button: true,
      selected: sel,
      label: '${o.shipClass.label}${locked ? ', locked' : blocked == null ? '' : ', unavailable: $blocked'}',
      excludeSemantics: true,
      onTap: () => onSelect(i),
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        child: GestureDetector(
          key: ValueKey('hw-card-$i'),
          behavior: HitTestBehavior.opaque,
          onTap: () => onSelect(i),
          child: Opacity(
            opacity: locked ? .6 : blocked != null ? .8 : 1,
            child: ConsolePanel(
              border: sel ? C.a200 : (spec?.canQueue ?? false) ? C.ownDim : C.line,
              mark: sel ? C.a200 : null,
              child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                SizedBox(height: 120, child: RepaintBoundary(child: CustomPaint(painter: ShipDraftPainter(o.shipClass, sel)))),
                Container(
                  padding: const EdgeInsets.fromLTRB(10, 6, 10, 8),
                  decoration: const BoxDecoration(border: Border(top: BorderSide(color: C.lineLo))),
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text(o.shipClass.label.toUpperCase(), style: T.cond(size: 14, color: sel ? C.a100 : C.a200, spacing: 1.4)),
                    const SizedBox(height: 4),
                    Text(
                        'HULL ${groupDigits(o.hull.round())}  SHLD ${groupDigits(o.shield.round())}  WPN ${groupDigits(o.weapon.round())}'
                        '${o.flies ? '  CARGO ${groupDigits(o.cargo.round())}' : ''}',
                        style: T.mono(size: 10, color: C.text2)),
                    Text(_role(o), style: T.mono(size: 10, color: C.text3)),
                    const SizedBox(height: 4),
                    Text('${amountsOf(o.unitCost)}   ${fmtTicks(o.ticksPerShip)}', style: T.mono(size: 10, color: C.a300)),
                    if (locked) WhyText('Locked: needs ${o.requires.where((r) => !r.met).map((r) => r.label).join(', ')}'),
                    if (blocked != null) ...[
                      const SizedBox(height: 3),
                      Text('X${ctrl.shipBatch} UNAVAILABLE', style: T.cond(size: 10, color: C.threat(5), spacing: 1.2)),
                      Text(blocked, style: T.mono(size: 10, color: C.threat(5), height: 1.3)),
                    ],
                  ]),
                ),
              ]),
            ),
          ),
        ),
      ),
    );
  }
}
