import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../console/economy_view.dart';
import '../../console/intel.dart';
import '../../design/gauges.dart';
import '../../design/panel.dart';
import '../../design/tokens.dart';
import '../../models/fleet.dart';
import '../../models/game_state.dart';
import '../../protocol/protocol.dart' as proto;
import '../../state/game_controller.dart';
import 'spec.dart';

Widget _notReported(String what) => Padding(
      padding: const EdgeInsets.all(12),
      child: ConsolePanel(
        key: const ValueKey('hw-not-reported'),
        title: what,
        padding: const EdgeInsets.all(16),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text('NOT REPORTED BY SERVER YET', style: T.cond(size: 16, color: C.a300, spacing: 2)),
          const SizedBox(height: 6),
          Text('This server does not send $what data. The panel fills in when it does.', style: T.mono(size: 11, color: C.text3, height: 1.4)),
        ]),
      ),
    );

class DefenceTab extends StatelessWidget {
  final EconomyView eco;
  final GameController ctrl;
  final ValueChanged<int> onSelect;
  const DefenceTab({super.key, required this.eco, required this.ctrl, required this.onSelect});

  @override
  Widget build(BuildContext context) {
    final d = eco.defence;
    if (d == null) return _notReported('Defence');
    final cover = d.total / math.max(1, d.raidEstimate);
    final raidIn = d.raidInTicks;
    return Padding(
      padding: const EdgeInsets.all(12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        ConsolePanel(
          title: 'Raid forecast',
          subText: 'estimate',
          padding: const EdgeInsets.all(14),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Row(crossAxisAlignment: CrossAxisAlignment.end, children: [
              Flexible(
                child: Text(raidIn == null ? 'NO RAID ANNOUNCED' : 'RAID IN ${clockFmt(raidIn)}',
                    overflow: TextOverflow.ellipsis, style: T.cond(size: 26, color: raidIn == null ? C.text2 : C.ember, weight: FontWeight.w700, spacing: 2)),
              ),
              const SizedBox(width: 8),
              Text('DEFENCE ${cover.toStringAsFixed(2)}X THE RAID',
                  key: const Key('defence-cover'), style: T.cond(size: 13, color: cover >= 1 ? C.ok : C.ember, spacing: 1.4)),
            ]),
            const SizedBox(height: 10),
            _breakdown(d),
            const SizedBox(height: 6),
            Wrap(spacing: 14, runSpacing: 4, children: [
              _legend('Docked fleet and grid', d.other, C.own),
              _legend('Structures', d.structures, C.crystal),
              _legend('Next raid est.', d.raidEstimate, C.ember),
            ]),
          ]),
        ),
        const SizedBox(height: 10),
        ConsolePanel(
          title: 'Structures',
          subText: 'BATCH x${ctrl.shipBatch}',
          child: Column(children: [for (var i = 0; i < d.rows.length; i++) _structure(i, d.rows[i])]),
        ),
      ]),
    );
  }

  Widget _legend(String n, double v, Color c) => Row(mainAxisSize: MainAxisSize.min, children: [
        Container(width: 8, height: 8, color: c),
        const SizedBox(width: 5),
        Text('${n.toUpperCase()} ${v.round()}', style: T.mono(size: 10.5, color: C.text2)),
      ]);

  Widget _breakdown(DefenceView d) {
    final max = (d.total > d.raidEstimate ? d.total : d.raidEstimate) * 1.05;
    Widget seg(double v, Color c) => Expanded(flex: (v / max * 1000).round().clamp(1, 1000), child: Container(height: 12, color: c.withValues(alpha: .75)));
    return Column(children: [
      Row(children: [
        seg(d.other, C.own),
        seg(d.structures, C.crystal),
        Expanded(flex: ((max - d.total) / max * 1000).round().clamp(1, 1000), child: const SizedBox()),
      ]),
      const SizedBox(height: 4),
      Row(children: [
        Expanded(flex: (d.raidEstimate / max * 1000).round().clamp(1, 1000), child: Container(height: 12, color: C.ember.withValues(alpha: .75))),
        Expanded(flex: ((max - d.raidEstimate) / max * 1000).round().clamp(1, 1000), child: const SizedBox()),
      ]),
    ]);
  }

  Widget _structure(int i, DefenceRow r) {
    final spec = specFor(ctrl, index: i);
    final sel = ctrl.hwCursor == i;
    final o = r.option;
    return GestureDetector(
      key: ValueKey('hw-card-$i'),
      behavior: HitTestBehavior.opaque,
      onTap: () => onSelect(i),
      child: Container(
        padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
        decoration: BoxDecoration(color: sel ? C.a(C.a500, .06) : null, border: const Border(bottom: BorderSide(color: C.lineLo))),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Row(children: [
            Expanded(child: Text(r.kind.label.toUpperCase(), style: T.cond(size: 13, color: sel ? C.a100 : C.a200, spacing: 1.2))),
            Flexible(child: Text('x${r.count}${r.restoring > 0 ? ' (+${r.restoring} restoring)' : ''}', overflow: TextOverflow.ellipsis, style: T.mono(size: 12, color: C.a100))),
            const SizedBox(width: 12),
            Text('PWR ${r.power.round()}', style: T.mono(size: 11, color: C.text2)),
          ]),
          const SizedBox(height: 2),
          if (o != null)
            Text(
                'UNIT ${o.power.round()} PWR   COST ${o.unitCost.metal.round()} FE ${o.unitCost.crystal.round()} CR ${o.unitCost.deuterium.round()} DE   ${clockFmt(o.ticksPerUnit)}',
                style: T.mono(size: 10, color: C.text3)),
          const SizedBox(height: 6),
          ConsoleButton('Build x${ctrl.shipBatch}',
              small: true,
              keyHint: sel ? 'Enter' : null,
              onPressed: spec != null && spec.canQueue ? () => ctrl.activateHomeworldCard(i) : null,
              reason: spec?.reason),
          if (spec?.reason != null) WhyText(spec!.reason!),
        ]),
      ),
    );
  }
}

/// Shares of what fits that a load offers, in percent. The whole of it is
/// sent with no amount, which the server reads as all that fits.
const loadShares = [25, 50, 75, 100];

const _abbr = {proto.ResourceKind.metal: 'Fe', proto.ResourceKind.crystal: 'Cr', proto.ResourceKind.deuterium: 'De'};

/// Load the hold of a fleet docked at home from the stockpile, or put it
/// back. The server moves the cargo and words every refusal; the panel shows
/// what is aboard and what would fit.
class HoldPanel extends StatefulWidget {
  final GameState state;
  final GameController ctrl;
  const HoldPanel({super.key, required this.state, required this.ctrl});
  @override
  State<HoldPanel> createState() => _HoldPanelState();
}

class _HoldPanelState extends State<HoldPanel> {
  int? _fleet;
  proto.ResourceKind? _resource;
  int _share = 100;

  @override
  Widget build(BuildContext context) {
    final st = widget.state;
    final c = widget.ctrl;
    final docked = [for (final f in st.fleets) if (f.sector == st.homeworld) f];
    if (docked.isEmpty) {
      return const ConsolePanel(
        key: ValueKey('hold-panel'),
        title: 'Load a hold',
        subText: 'no fleet at the dock',
        padding: EdgeInsets.all(14),
        child: WhyText('A hold is loaded from the stockpile at the homeworld only, and no fleet is docked here.', key: Key('hold-why')),
      );
    }
    final fleet = docked.where((f) => f.id == _fleet).firstOrNull ?? docked.where((f) => f.id == c.currentFleet.id).firstOrNull ?? docked.first;
    final capped = st.hw?.storage.capped ?? const <proto.ResourceKind>[];
    final resource = _resource ?? capped.firstOrNull ?? proto.ResourceKind.metal;
    final hold = st.holds[fleet.id] ?? const proto.Resources();
    final room = math.max(0.0, fleet.holdCapacity - hold.total);
    final stock = resource.of(st.stock);
    final fits = math.min(stock, room);
    final amount = _share == 100 ? null : fits * _share / 100;
    final free = fleet.status == FleetStatus.idle || fleet.status == FleetStatus.docked;
    final why = fleet.policy != null
        ? '${fleet.name} is under standing orders (${fleet.policy}), which unload at the dock. Set it to manual to load it by hand.'
        : !free
            ? '${fleet.name} is busy.'
            : room < 1
                ? 'The hold of ${fleet.name} is full.'
                : stock < 1
                    ? 'The stockpile holds no ${resource.name}.'
                    : null;
    final note = fleet.laden
        ? 'KEPT ABOARD UNTIL THE FLEET HAS BEEN AWAY'
        : fleet.cargoBlocked
            ? 'WAITING FOR ROOM IN THE STOCKPILE'
            : null;

    return ConsolePanel(
      key: const ValueKey('hold-panel'),
      title: 'Load a hold',
      subText: 'at the home dock',
      padding: const EdgeInsets.all(14),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
        const Lbl('Fleet'),
        const SizedBox(height: 4),
        Wrap(spacing: 6, runSpacing: 6, children: [
          for (final f in docked)
            ConsoleButton('${f.name} ${(st.holds[f.id]?.total ?? 0).floor()}/${f.holdCapacity.floor()}',
                key: ValueKey('hold-fleet-${f.id}'), small: true, primary: f.id == fleet.id, onPressed: () => setState(() => _fleet = f.id)),
        ]),
        const SizedBox(height: 10),
        Row(crossAxisAlignment: CrossAxisAlignment.baseline, textBaseline: TextBaseline.alphabetic, children: [
          const Lbl('Aboard'),
          const SizedBox(width: 10),
          Text('${hold.total.floor()} / ${fleet.holdCapacity.floor()}', key: const Key('hold-aboard'), style: T.mono(size: 13, color: C.text)),
          const Spacer(),
          if (fleet.laden) const Lamp('Laden', color: C.ok),
        ]),
        const SizedBox(height: 4),
        LedBar(fleet.holdCapacity > 0 ? hold.total / fleet.holdCapacity : 0, color: C.own),
        const SizedBox(height: 5),
        Text.rich(
          TextSpan(children: [
            for (final k in proto.ResourceKind.values) ...[
              TextSpan(text: '${_abbr[k]} ${k.of(hold).floor()}', style: TextStyle(color: C.ore(k.name))),
              const TextSpan(text: '   '),
            ],
            if (note != null) TextSpan(text: note, style: const TextStyle(color: C.text3)),
          ]),
          key: const Key('hold-cargo'),
          style: T.mono(size: 10.5),
        ),
        const SizedBox(height: 10),
        const Lbl('Take from the stockpile'),
        const SizedBox(height: 4),
        Wrap(spacing: 6, runSpacing: 6, children: [
          for (final k in proto.ResourceKind.values)
            ConsoleButton('${_abbr[k]} ${k.of(st.stock).floor()}${capped.contains(k) ? ' full' : ''}',
                key: ValueKey('hold-res-${k.name}'),
                small: true,
                primary: k == resource,
                semanticsLabel: '${k.name} ${k.of(st.stock).floor()} in the stockpile',
                onPressed: () => setState(() => _resource = k)),
        ]),
        const SizedBox(height: 8),
        const Lbl('How much'),
        const SizedBox(height: 4),
        Wrap(spacing: 6, runSpacing: 6, children: [
          for (final s in loadShares)
            ConsoleButton(s == 100 ? 'All that fits' : '$s%',
                key: ValueKey('hold-share-$s'), small: true, primary: s == _share, onPressed: () => setState(() => _share = s)),
        ]),
        const SizedBox(height: 8),
        Kv('Room in the hold', key: const Key('hold-room'), v: '${room.floor()}'),
        const SizedBox(height: 10),
        Wrap(spacing: 8, runSpacing: 6, children: [
          ConsoleButton(why == null ? 'Load ${(amount ?? fits).floor()} ${_abbr[resource]}' : 'Load',
              key: const ValueKey('hold-load'),
              small: true,
              primary: why == null,
              onPressed: why == null ? () => c.loadHold(fleet.id, resource, amount: amount) : null,
              reason: why),
          ConsoleButton('Unload',
              key: const ValueKey('hold-unload'),
              small: true,
              onPressed: fleet.laden ? () => c.unloadHold(fleet.id) : null,
              reason: 'Nothing aboard was loaded by hand'),
        ]),
        if (why != null) WhyText(why, key: const Key('hold-why')),
        const WhyText('A loaded hold stays aboard while the fleet is docked: fly it to the hub and trade. What the fleet brings home '
            'unloads as always. A raid counts cargo at home with the stockpile.'),
      ]),
    );
  }
}

class StorageTab extends StatelessWidget {
  final EconomyView eco;
  final GameState state;
  final GameController ctrl;
  const StorageTab({super.key, required this.eco, required this.state, required this.ctrl});

  @override
  Widget build(BuildContext context) {
    final st = eco.storage;
    if (st == null) return _notReported('Storage');
    final rows = [
      ('metal', 'Metal', C.metal, state.stock.metal, state.resources.metal.rate),
      ('crystal', 'Crystal', C.crystal, state.stock.crystal, state.resources.crystal.rate),
      ('deut', 'Deuterium', C.deut, state.stock.deuterium, state.resources.deut.rate),
    ];
    return Padding(
      padding: const EdgeInsets.all(12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        ConsolePanel(
          title: 'Storage vault',
          subText: 'LEVEL ${st.vaultLevel}',
          padding: const EdgeInsets.all(14),
          child: Column(children: [for (final r in rows) _row(st, r.$1, r.$2, r.$3, r.$4, r.$5)]),
        ),
        const SizedBox(height: 10),
        HoldPanel(state: state, ctrl: ctrl),
        const SizedBox(height: 10),
        _upgrade(),
      ]),
    );
  }

  Widget _upgrade() {
    final cat = state.catalog;
    final o = cat?.buildings.where((b) => b.buildingType == proto.BuildingType.storageVault).firstOrNull;
    final hw = state.hw;
    String? reason;
    if (o == null) {
      reason = 'The server catalog has no Storage Vault';
    } else {
      reason = whyNot(
          requires: o.requires,
          maxed: o.next == null,
          queued: hw == null ? 0 : hw.buildQueue.length + hw.buildPending.length,
          capacity: hw == null ? 0 : hw.buildSlots + hw.queueWaitingMax);
    }
    return ConsolePanel(
      title: 'Upgrade',
      padding: const EdgeInsets.all(14),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        if (o?.next != null)
          Text('L${o!.level} > L${o.next!.level}   ${o.next!.cost.metal.round()} FE ${o.next!.cost.crystal.round()} CR ${o.next!.cost.deuterium.round()} DE   ${clockFmt(o.next!.ticks)}',
              style: T.mono(size: 11, color: C.text2)),
        const SizedBox(height: 6),
        ConsoleButton('Upgrade vault',
            key: const ValueKey('hw-upgrade-vault'),
            small: true,
            onPressed: reason == null ? () => ctrl.sendCommand(proto.BuildCommand(buildingType: proto.BuildingType.storageVault)) : null,
            reason: reason),
        if (reason != null) WhyText(reason),
      ]),
    );
  }

  Widget _row(StorageView st, String k, String name, Color c, double stock, double rate) {
    final cap = st.cap[k] ?? 1;
    final prot = st.protected[k] ?? 0;
    final full = st.fullIn(k, stock, rate);
    final near = stock >= cap * .85;
    return Padding(
      padding: const EdgeInsets.only(bottom: 14),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Row(crossAxisAlignment: CrossAxisAlignment.baseline, textBaseline: TextBaseline.alphabetic, children: [
          Text(name.toUpperCase(), style: T.cond(size: 12, color: c, spacing: 1.6)),
          const SizedBox(width: 10),
          Register(stock, size: 20, glow: true),
          Text(' / ${cap.round()}', style: T.mono(size: 12, color: C.text3)),
          const Spacer(),
          if (near) Lamp(stock >= cap * .98 ? 'Full' : 'Near cap', color: C.ember, blink: true),
        ]),
        const SizedBox(height: 4),
        LedBar(stock / cap, color: near ? C.ember : c, height: 10, mark: C.ember, markAt: .85),
        const SizedBox(height: 6),
        Text(
            'PROTECTED ${prot.round()}   RAIDABLE ${(stock - prot).clamp(0, cap).round()}   '
            '${full == null ? 'NOT FILLING' : full == 0 ? 'FULL' : 'FULL IN ${clockFmt(full)}'}   +${rate.toStringAsFixed(2)}/T',
            style: T.mono(size: 10, color: C.text3)),
      ]),
    );
  }
}
