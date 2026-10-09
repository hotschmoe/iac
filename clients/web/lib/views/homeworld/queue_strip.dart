import 'package:flutter/material.dart';

import '../../console/economy_view.dart';
import '../../design/panel.dart';
import '../../design/tokens.dart';
import '../../models/game_state.dart';
import '../../protocol/protocol.dart' as proto;
import '../../state/game_controller.dart';

/// Four queue pills: build slot A, slot B (when reported), research, shipyard.
class QueueStrip extends StatelessWidget {
  final GameState state;
  final EconomyView eco;
  final GameController ctrl;
  final bool narrow;
  const QueueStrip({super.key, required this.state, required this.eco, required this.ctrl, required this.narrow});

  @override
  Widget build(BuildContext context) {
    final slots = eco.slots;
    final b = state.buildQueue.isEmpty ? null : state.buildQueue.first;
    final sh = state.shipyard.isEmpty ? null : state.shipyard.first;
    final rs = state.research.name == 'Idle' ? null : state.research;
    final pills = <Widget>[
      _Pill(
          label: 'Build slot A${slots == null ? '' : ' - queue ${slots.depth}/${slots.maxDepth}'}',
          title: b?.name ?? 'Idle: pick a building',
          time: b?.time,
          pct: b?.pct,
          cancelKey: 'hw-cancel-building',
          onCancel: b == null ? null : () => ctrl.cancelHomeworldQueue(proto.QueueType.building),
          onTap: () => ctrl.selectHomeworldTab(HomeworldTab.buildings)),
      if (slots != null)
        _Pill(
            label: 'Build slot B',
            title: slots.slotBUnlocked ? (slots.slotB?.name ?? 'Idle') : 'Locked',
            sub: slots.slotBUnlocked ? null : slots.slotBRequirement,
            locked: !slots.slotBUnlocked,
            time: null,
            pct: null),
      _Pill(
          label: 'Research',
          title: rs?.name ?? 'Idle: pick a tech',
          time: rs?.time,
          pct: rs?.pct,
          cancelKey: 'hw-cancel-research',
          onCancel: rs == null ? null : () => ctrl.cancelHomeworldQueue(proto.QueueType.research),
          onTap: () => ctrl.selectHomeworldTab(HomeworldTab.research)),
      _Pill(
          label: 'Shipyard',
          title: sh?.name ?? 'Idle: build a ship',
          time: sh?.time,
          pct: sh?.pct,
          cancelKey: 'hw-cancel-ship',
          onCancel: sh == null ? null : () => ctrl.cancelHomeworldQueue(proto.QueueType.ship),
          onTap: () => ctrl.selectHomeworldTab(HomeworldTab.shipyard)),
    ];
    return Container(
      decoration: const BoxDecoration(color: C.hud, border: Border(bottom: BorderSide(color: C.line))),
      child: narrow
          ? Wrap(children: [for (final p in pills) FractionallySizedBox(widthFactor: 1, child: p)])
          : Row(children: [for (final p in pills) Expanded(child: p)]),
    );
  }
}

class _Pill extends StatelessWidget {
  final String label, title;
  final String? sub, time, cancelKey;
  final double? pct;
  final bool locked;
  final VoidCallback? onCancel, onTap;
  const _Pill({required this.label, required this.title, this.sub, this.time, this.pct, this.locked = false, this.cancelKey, this.onCancel, this.onTap});

  @override
  Widget build(BuildContext context) {
    final idle = pct == null && !locked;
    return GestureDetector(
      onTap: onTap,
      behavior: HitTestBehavior.opaque,
      child: Opacity(
        opacity: locked ? .55 : 1,
        child: Container(
          constraints: const BoxConstraints(minHeight: 42),
          decoration: BoxDecoration(border: const Border(right: BorderSide(color: C.lineLo), bottom: BorderSide(color: C.lineLo)), color: idle ? C.a(C.a500, .03) : null),
          child: Stack(children: [
            Padding(padding: const EdgeInsets.fromLTRB(14, 6, 10, 8), child: Row(children: [
              Expanded(
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
                  Lbl(label, size: 9),
                  Text(
                    (locked ? 'LOCKED: ${sub ?? ''}' : title).toUpperCase(),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: T.cond(size: 13, color: idle ? C.a400 : C.a200, spacing: .8),
                  ),
                ]),
              ),
              if (time != null) Text(time!, style: T.mono(size: 15, color: C.a100, weight: FontWeight.w500)),
              if (onCancel != null) ...[
                const SizedBox(width: 8),
                ConsoleButton('Cancel', key: ValueKey<String>(cancelKey!), small: true, onPressed: onCancel),
              ],
            ])),
            if (pct != null)
              Positioned(
                left: 0,
                right: 0,
                bottom: 0,
                height: 3,
                child: FractionallySizedBox(alignment: Alignment.centerLeft, widthFactor: (pct! / 100).clamp(0.0, 1.0), child: Container(color: C.a400)),
              ),
          ]),
        ),
      ),
    );
  }
}
