import 'package:flutter/material.dart';

import '../../console/economy_view.dart';
import '../../design/panel.dart';
import '../../design/tokens.dart';
import '../../models/game_state.dart';
import '../../models/homeworld.dart';
import '../../protocol/protocol.dart' as proto;
import '../../state/game_controller.dart';

/// The queue pills: one for each building slot the server reports, the next
/// slot while it can still be unlocked, research, shipyard.
class QueueStrip extends StatelessWidget {
  final GameState state;
  final EconomyView eco;
  final GameController ctrl;
  final bool narrow;
  const QueueStrip({super.key, required this.state, required this.eco, required this.ctrl, required this.narrow});

  static Key _slotKey(int index) => Key('hw-pill-slot-${slotLetter(index).toLowerCase()}');

  /// The pill of a building slot after the first, which carries the queue.
  Widget _slot(int index, QueueItem? build) => _Pill(
      key: _slotKey(index),
      label: 'Build slot ${slotLetter(index)}',
      title: build?.name ?? 'Idle',
      time: build?.time,
      pct: build?.pct,
      cancelKey: 'hw-cancel-building-${slotLetter(index).toLowerCase()}',
      onCancel: build == null ? null : () => ctrl.cancelHomeworldQueue(proto.QueueType.building, index: index),
      onTap: () => ctrl.selectHomeworldTab(HomeworldTab.buildings));

  @override
  Widget build(BuildContext context) {
    final slots = eco.slots;
    final running = [for (final q in state.buildQueue) if (q.active) q];
    final b = running.firstOrNull;
    final sh = state.runningShip;
    final bWait = state.waitingBuild.firstOrNull;
    final shWait = state.waitingShips.firstOrNull;
    final rs = state.research.name == 'Idle' ? null : state.research;
    final rsWait = state.research.waiting.firstOrNull;
    bool built(proto.BuildingType t) => (state.catalog?.buildings.where((b) => b.buildingType == t).firstOrNull?.level ?? 0) > 0;
    final pills = <Widget>[
      _Pill(
          key: const Key('hw-pill-building'),
          label: 'Build slot A${slots == null ? '' : ' - queue ${slots.depth}/${slots.maxDepth}'}',
          title: b?.name ?? (bWait == null ? 'Idle: pick a building' : 'Idle, an order is waiting'),
          time: b?.time,
          waiting: b == null ? bWait : null,
          pct: b?.pct,
          cancelKey: 'hw-cancel-building',
          onCancel: b == null && bWait == null ? null : () => ctrl.cancelHomeworldQueue(proto.QueueType.building, waiting: b == null),
          onTap: () => ctrl.selectHomeworldTab(HomeworldTab.buildings)),
      if (slots != null) ...[
        for (var i = 1; i < slots.slotCount; i++) _slot(i, running.elementAtOrNull(i)),
        if (slots.nextSlot case final needs?)
          _Pill(key: _slotKey(slots.slotCount), label: 'Build slot ${slotLetter(slots.slotCount)}', title: 'Locked', sub: needs, locked: true),
      ],
      _Pill(
          key: const Key('hw-pill-research'),
          label: 'Research',
          title: rs?.name ??
              (rsWait != null
                  ? 'Idle, an order is waiting'
                  : built(proto.BuildingType.researchLab)
                      ? 'Idle: pick a tech'
                      : 'No Research Lab yet'),
          time: rs?.time,
          waiting: rs == null ? rsWait : null,
          pct: rs?.pct,
          cancelKey: 'hw-cancel-research',
          onCancel: rs == null && rsWait == null ? null : () => ctrl.cancelHomeworldQueue(proto.QueueType.research, waiting: rs == null),
          onTap: () => ctrl.selectHomeworldTab(HomeworldTab.research)),
      _Pill(
          key: const Key('hw-pill-ship'),
          label: 'Shipyard',
          title: sh?.name ??
              (shWait != null
                  ? 'Idle, a batch is waiting'
                  : built(proto.BuildingType.shipyard)
                      ? 'Idle: build a ship'
                      : 'No Shipyard yet'),
          time: sh?.time,
          waiting: sh == null ? shWait : null,
          pct: sh?.pct,
          cancelKey: 'hw-cancel-ship',
          onCancel: sh == null && shWait == null ? null : () => ctrl.cancelHomeworldQueue(proto.QueueType.ship, waiting: sh == null),
          onTap: () => ctrl.selectHomeworldTab(HomeworldTab.shipyard)),
    ];
    return Container(
      decoration: const BoxDecoration(color: C.hud, border: Border(bottom: BorderSide(color: C.line))),
      child: narrow
          ? Wrap(children: [for (final p in pills) FractionallySizedBox(widthFactor: 1, child: p)])
          : IntrinsicHeight(child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [for (final p in pills) Expanded(child: p)])),
    );
  }
}

/// One queue's box. A running item shows its clock beside the title. A
/// waiting order's name and what it waits for (a shortfall of up to three
/// resources and an estimate) is a sentence of any length, so it gets the
/// full width under the title and wraps there.
class _Pill extends StatelessWidget {
  final String label, title;
  final String? sub, time, cancelKey;
  final QueueItem? waiting;
  final double? pct;
  final bool locked;
  final VoidCallback? onCancel, onTap;
  const _Pill(
      {super.key, required this.label, required this.title, this.sub, this.time, this.waiting, this.pct, this.locked = false, this.cancelKey, this.onCancel, this.onTap});

  @override
  Widget build(BuildContext context) {
    final idle = pct == null && !locked;
    final next = waiting;
    return GestureDetector(
      onTap: onTap,
      behavior: HitTestBehavior.opaque,
      child: Opacity(
        opacity: locked ? .55 : 1,
        child: Container(
          constraints: const BoxConstraints(minHeight: 42),
          decoration: BoxDecoration(border: const Border(right: BorderSide(color: C.lineLo), bottom: BorderSide(color: C.lineLo)), color: idle ? C.a(C.a500, .03) : null),
          child: Stack(children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(14, 6, 10, 8),
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
                Row(children: [
                  Expanded(
                    child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
                      Lbl(label, size: 9),
                      Text(
                        title.toUpperCase(),
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
                ]),
                if (next != null)
                  Padding(
                    padding: const EdgeInsets.only(top: 3),
                    child: Text.rich(TextSpan(children: [
                      TextSpan(text: '${next.name.toUpperCase()}  ', style: T.mono(size: 10.5, color: C.text2, weight: FontWeight.w500)),
                      TextSpan(text: next.time, style: T.mono(size: 10.5, color: C.text3, height: 1.3)),
                    ])),
                  ),
                if (locked && sub != null)
                  Padding(padding: const EdgeInsets.only(top: 3), child: Text(sub!, style: T.mono(size: 10.5, color: C.text3, height: 1.3))),
              ]),
            ),
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
