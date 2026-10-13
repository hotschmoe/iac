import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../console/economy_view.dart';
import '../../console/intel.dart';
import '../../console/outpost.dart' show storeAtCap;
import '../../design/gauges.dart';
import '../../design/panel.dart';
import '../../design/tokens.dart';
import '../../models/fleet.dart';
import '../../models/game_state.dart';
import '../../protocol/protocol.dart' as proto;
import '../../state/game_controller.dart';
import '../../state/state_mapper.dart' show amountsOf;
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

/// The homeworld's energy as the server states it: what is made and what
/// the mines need, the share of their output the mines run at, what the
/// Solar Satellites give and what the Fusion Reactor burns. Short of energy
/// it warns, and it says so when the reactor has nothing to burn.
class EnergyReadout extends StatelessWidget {
  final proto.HomeworldState hw;
  const EnergyReadout({super.key, required this.hw});

  @override
  Widget build(BuildContext context) {
    final e = hw.energy;
    final short = e.fraction < 1;
    final share = (e.fraction * 100).floor();
    final col = short ? C.ember : C.own;
    return Semantics(
      label: 'Energy: ${e.made} made, ${e.needed} needed, mines at $share percent',
      child: ConsolePanel(
        key: const Key('energy-readout'),
        title: 'Energy',
        subText: short ? 'short' : 'covered',
        border: short ? C.ember : null,
        padding: const EdgeInsets.fromLTRB(14, 10, 14, 12),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Wrap(crossAxisAlignment: WrapCrossAlignment.center, spacing: 14, runSpacing: 4, children: [
            Text.rich(
                key: const Key('energy-figures'),
                TextSpan(children: [
                  TextSpan(text: groupDigits(e.made), style: T.mono(size: 20, color: short ? C.ember : C.a100, weight: FontWeight.w500)),
                  TextSpan(text: ' MADE / ', style: T.mono(size: 11, color: C.text3)),
                  TextSpan(text: groupDigits(e.needed), style: T.mono(size: 20, color: C.a100, weight: FontWeight.w500)),
                  TextSpan(text: ' NEEDED', style: T.mono(size: 11, color: C.text3)),
                ])),
            Text('MINES AT $share%', key: const Key('energy-share'), style: T.cond(size: 14, color: col, weight: FontWeight.w700, spacing: 1.6)),
            if (short) const Lamp('Short of energy', color: C.ember, blink: true),
          ]),
          const SizedBox(height: 6),
          LedBar(e.fraction.clamp(0.0, 1.0), color: col, height: 8),
          const SizedBox(height: 6),
          Wrap(spacing: 16, runSpacing: 2, children: [
            if (short)
              Text('EVERY MINE MAKES $share% OF ITS OUTPUT UNTIL MORE ENERGY IS MADE.',
                  key: const Key('energy-warning'), style: T.mono(size: 10.5, color: C.ember, weight: FontWeight.w600)),
            Text('SOLAR SATELLITES ${hw.satellites}, +${e.perSatellite} EACH', key: const Key('energy-satellites'), style: T.mono(size: 10.5, color: C.text3)),
            if (e.fusionStarved)
              Text('FUSION REACTOR STOPPED: NO DEUTERIUM TO BURN', key: const Key('energy-fusion'), style: T.mono(size: 10.5, color: C.ember, weight: FontWeight.w600))
            else if (e.fusionBurn > 0)
              Text('FUSION REACTOR BURNS ${anHour(e.fusionBurn)} DE/H', key: const Key('energy-fusion'), style: T.mono(size: 10.5, color: C.text3)),
          ]),
        ]),
      ),
    );
  }
}

class DefenceTab extends StatelessWidget {
  final EconomyView eco;
  final GameController ctrl;
  final ValueChanged<int> onSelect;
  const DefenceTab({super.key, required this.eco, required this.ctrl, required this.onSelect});

  @override
  Widget build(BuildContext context) {
    final d = eco.defence;
    if (d == null) return _notReported('Defence');
    final cover = d.ratio;
    final raidIn = d.raidInTicks;
    return Padding(
      padding: const EdgeInsets.all(12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        ConsolePanel(
          title: 'Raid forecast',
          subText: 'estimate',
          padding: const EdgeInsets.all(14),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Wrap(crossAxisAlignment: WrapCrossAlignment.end, spacing: 8, runSpacing: 2, children: [
              Text(raidIn == null ? 'NO RAID ANNOUNCED' : 'RAID IN ${clockFmt(raidIn)}',
                  style: T.cond(size: 26, color: raidIn == null ? C.text2 : C.ember, weight: FontWeight.w700, spacing: 2)),
              Text('DEFENCE ${cover.toStringAsFixed(2)}X THE RAID',
                  key: const Key('defence-cover'), style: T.cond(size: 13, color: cover >= 1 ? C.ok : C.ember, spacing: 1.4)),
            ]),
            const SizedBox(height: 10),
            _breakdown(d),
            const SizedBox(height: 6),
            Wrap(spacing: 14, runSpacing: 4, children: [
              _legend('Docked ships', d.docked, C.own),
              _legend('Solar satellites', d.satellites, C.a400),
              _legend('Defences', d.structures, C.crystal),
              _legend('Next raid est.', d.raidEstimate, C.ember),
            ]),
          ]),
        ),
        const SizedBox(height: 10),
        ConsolePanel(
          key: const Key('defence-satellites'),
          title: 'Solar satellites',
          subText: 'x${d.satelliteCount}',
          padding: const EdgeInsets.fromLTRB(12, 8, 12, 10),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text('POWER ${groupDigits(d.satellites.round())} IN THE HOME DEFENCE', style: T.mono(size: 11, color: C.text2)),
            WhyText('They stand at the homeworld and join no fleet. A raid that breaks through destroys ${proto.raidLostShipPct}% of them, '
                'as it does of the ships docked here. They are built in the shipyard.'),
            const SizedBox(height: 6),
            ConsoleButton('Open shipyard', key: const ValueKey('defence-open-shipyard'), small: true, onPressed: () => ctrl.selectHomeworldTab(HomeworldTab.shipyard)),
          ]),
        ),
        const SizedBox(height: 10),
        ConsolePanel(
          title: 'Defences',
          subText: 'BATCH x${ctrl.shipBatch}',
          child: Column(children: [for (var i = 0; i < d.rows.length; i++) _structure(i, d.rows[i])]),
        ),
      ]),
    );
  }

  Widget _legend(String n, double v, Color c) => Row(mainAxisSize: MainAxisSize.min, children: [
        Container(width: 8, height: 8, color: c),
        const SizedBox(width: 5),
        Text('${n.toUpperCase()} ${groupDigits(v.round())}', style: T.mono(size: 10.5, color: C.text2)),
      ]);

  Widget _breakdown(DefenceView d) {
    final max = math.max(1.0, (d.total > d.raidEstimate ? d.total : d.raidEstimate) * 1.05);
    Widget seg(double v, Color c) => Expanded(flex: (v / max * 1000).round().clamp(1, 1000), child: Container(height: 12, color: c.withValues(alpha: .75)));
    return Column(children: [
      Row(children: [
        seg(d.docked, C.own),
        seg(d.satellites, C.a400),
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
            if (o?.max == 1) ...[const Tag('One of a kind'), const SizedBox(width: 8)],
            Flexible(child: Text('x${r.count}${r.restoring > 0 ? ' (+${r.restoring} restoring)' : ''}', overflow: TextOverflow.ellipsis, style: T.mono(size: 12, color: C.a100))),
          ]),
          const SizedBox(height: 2),
          if (o != null)
            Text('EACH ${groupDigits(r.unitPower.round())} PWR   ${amountsOf(o.unitCost).toUpperCase()}   ${clockFmt(o.ticksPerUnit)}',
                style: T.mono(size: 10, color: C.text3)),
          const SizedBox(height: 6),
          ConsoleButton('Build x${o == null ? ctrl.shipBatch : math.max(1, defenceBatch(o, ctrl.shipBatch))}',
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

/// The two docks a hold is filled and emptied at by hand: the homeworld,
/// from the stockpile, and the player's outpost, from its store.
enum HoldDock { home, outpost }

/// Load the hold of a fleet standing at a dock from that dock's stock, or
/// empty it there. The server moves the cargo and words every refusal; the
/// panel shows what is aboard and what would fit.
class HoldPanel extends StatefulWidget {
  final GameState state;
  final GameController ctrl;
  final HoldDock dock;
  const HoldPanel({super.key, required this.state, required this.ctrl, this.dock = HoldDock.home});
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
    final outpost = widget.dock == HoldDock.outpost ? st.outpost : null;
    final atHome = outpost == null;
    final source = atHome ? 'stockpile' : 'store';
    final docked = [for (final f in st.fleets) if (atHome ? f.sector == st.homeworld : f.atOutpost) f];
    if (docked.isEmpty) {
      return ConsolePanel(
        key: const ValueKey('hold-panel'),
        title: 'Load a hold',
        subText: atHome ? 'no fleet at the dock' : 'no fleet at the outpost',
        padding: const EdgeInsets.all(14),
        child: WhyText(
            atHome
                ? 'A hold is loaded from the stockpile at the homeworld only, and no fleet is docked here.'
                : 'A hold is loaded from the outpost\'s store, and emptied into it, by a fleet standing at the outpost. None of yours stands there.',
            key: const Key('hold-why')),
      );
    }
    final fleet = docked.where((f) => f.id == _fleet).firstOrNull ?? docked.where((f) => f.id == c.currentFleet.id).firstOrNull ?? docked.first;
    final from = outpost?.store ?? st.stock;
    final capped = outpost == null
        ? st.hw?.storage.capped ?? const <proto.ResourceKind>[]
        : [for (final k in proto.ResourceKind.values) if (storeAtCap(outpost, k)) k];
    final resource = _resource ?? capped.firstOrNull ?? proto.ResourceKind.metal;
    final hold = st.holds[fleet.id] ?? const proto.Resources();
    final room = math.max(0.0, fleet.holdCapacity - hold.total);
    final stock = resource.of(from);
    final fits = math.min(stock, room);
    final amount = _share == 100 ? null : fits * _share / 100;
    final free = fleet.status == FleetStatus.idle || fleet.status == FleetStatus.docked;
    final why = fleet.policy != null
        ? '${fleet.name} is under standing orders (${fleet.policy}), which ${atHome ? 'unload at the dock' : 'do not use the outpost'}. Set it to manual to load it by hand.'
        : !free
            ? '${fleet.name} is busy.'
            : room < 1
                ? 'The hold of ${fleet.name} is full.'
                : stock < 1
                    ? 'The $source holds no ${resource.name}.'
                    : null;
    final note = !atHome
        ? null
        : fleet.laden
            ? 'KEPT ABOARD UNTIL THE FLEET HAS BEEN AWAY'
            : fleet.cargoBlocked
                ? 'WAITING FOR ROOM IN THE STOCKPILE'
                : null;
    final canUnload = atHome ? fleet.laden : hold.total >= 1;

    return ConsolePanel(
      key: const ValueKey('hold-panel'),
      title: 'Load a hold',
      subText: atHome ? 'at the home dock' : 'at the outpost',
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
        Lbl('Take from the $source'),
        const SizedBox(height: 4),
        Wrap(spacing: 6, runSpacing: 6, children: [
          for (final k in proto.ResourceKind.values)
            ConsoleButton('${_abbr[k]} ${k.of(from).floor()}${capped.contains(k) ? ' full' : ''}',
                key: ValueKey('hold-res-${k.name}'),
                small: true,
                primary: k == resource,
                semanticsLabel: '${k.name} ${k.of(from).floor()} in the $source',
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
              onPressed: canUnload ? () => c.unloadHold(fleet.id) : null,
              reason: atHome ? 'Nothing aboard was loaded by hand' : 'The hold is empty'),
        ]),
        if (why != null) WhyText(why, key: const Key('hold-why')),
        WhyText(
            atHome
                ? 'A loaded hold stays aboard while the fleet is docked: fly it to the hub and trade. What the fleet brings home '
                    'unloads as always. A raid counts cargo at home with the stockpile.'
                : 'Nothing moves by itself here: Unload puts the whole hold into the store, as far as the store has room. '
                    'A raid the outpost loses takes its share of the store and of the holds here above the protected floor, '
                    'and destroys ${proto.raidLostShipPct}% of the ships standing here.',
            key: const Key('hold-rule')),
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
    final hw = state.hw;
    if (st == null || hw == null) return _notReported('Storage');
    return Padding(
      padding: const EdgeInsets.all(12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        EnergyReadout(hw: hw),
        const SizedBox(height: 10),
        ConsolePanel(
          title: 'Storage',
          subText: 'one building a resource',
          padding: const EdgeInsets.fromLTRB(14, 14, 14, 2),
          child: Column(children: [for (final s in stockpiles) _row(st, hw, s.kind, s.name)]),
        ),
        const SizedBox(height: 10),
        HoldPanel(state: state, ctrl: ctrl),
      ]),
    );
  }

  /// The building that holds [k]: its level, its next and the order for it.
  Widget _upgrade(proto.HomeworldState hw, proto.ResourceKind k, proto.BuildingOption? o) {
    final next = o?.next;
    final reason = o == null
        ? 'The server catalog has no ${k.storage}'
        : whyNot(
            requires: o.requires,
            maxed: next == null,
            queued: hw.buildQueue.length + hw.buildPending.length,
            capacity: hw.buildSlots + hw.queueWaitingMax);
    final cap = o?.effects.where((e) => e.kind == proto.EffectKind.storageCap).firstOrNull?.next;
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Wrap(crossAxisAlignment: WrapCrossAlignment.center, spacing: 10, runSpacing: 4, children: [
        ConsoleButton(next == null ? '${k.storage} maxed' : 'Raise ${k.storage} to L${next.level}',
            key: ValueKey('hw-upgrade-${k.name}'),
            small: true,
            onPressed: reason == null && o != null ? () => ctrl.sendCommand(proto.BuildCommand(buildingType: o.buildingType)) : null,
            reason: reason),
        if (next != null)
          Text('${amountsOf(next.cost).toUpperCase()}   ${clockFmt(next.ticks)}${cap == null ? '' : '   CAP ${groupDigits(cap.round())}'}',
              style: T.mono(size: 10, color: C.text3)),
      ]),
      if (reason != null) WhyText(reason),
    ]);
  }

  Widget _row(StorageView st, proto.HomeworldState hw, proto.ResourceKind k, String name) {
    final c = C.ore(k.name);
    final stock = k.of(state.stock);
    final rate = k.of(hw.production);
    final cap = st.cap(k);
    final prot = st.protected(k);
    final full = st.fullIn(k);
    final near = proto.storageNearCap(stock, cap);
    return Padding(
      key: Key('storage-row-${k.name}'),
      padding: const EdgeInsets.only(bottom: 14),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Row(crossAxisAlignment: CrossAxisAlignment.baseline, textBaseline: TextBaseline.alphabetic, children: [
          Text(name.toUpperCase(), style: T.cond(size: 12, color: c, spacing: 1.6)),
          const SizedBox(width: 10),
          Register(stock, size: 20, glow: true),
          Flexible(child: Text(' / ${groupDigits(cap.round())}', overflow: TextOverflow.ellipsis, style: T.mono(size: 12, color: C.text3))),
          const Spacer(),
          if (near) Lamp(st.capped(k) ? 'Full' : 'Near cap', color: C.ember, blink: true) else Text(st.buildingTag(k).toUpperCase(), style: T.mono(size: 9.5, color: C.text3)),
        ]),
        const SizedBox(height: 4),
        LedBar(cap > 0 ? stock / cap : 0, color: near ? C.ember : c, height: 10, mark: C.ember, markAt: proto.storageNearCapRatio),
        const SizedBox(height: 6),
        Text(
            'SAFE FROM RAIDS ${groupDigits(prot.round())}   '
            '${full == null ? 'NOT FILLING' : full == 0 ? 'FULL' : 'FULL IN ${clockFmt(full)}'}   ${signedAnHour(rate)}/H',
            style: T.mono(size: 10, color: rate < 0 ? C.ember : C.text3)),
        const SizedBox(height: 6),
        _upgrade(hw, k, st.building(k)),
      ]),
    );
  }
}
