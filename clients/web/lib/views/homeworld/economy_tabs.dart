import 'package:flutter/material.dart';

import '../../console/economy_view.dart';
import '../../console/intel.dart';
import '../../design/gauges.dart';
import '../../design/panel.dart';
import '../../design/tokens.dart';
import '../../models/game_state.dart';

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

const _noDefenceBuild = 'Defence structures cannot be built yet: the server has no defence build command';

class DefenceTab extends StatelessWidget {
  final EconomyView eco;
  const DefenceTab({super.key, required this.eco});

  @override
  Widget build(BuildContext context) {
    final d = eco.defence;
    if (d == null) return _notReported('Defence');
    final info = RatioInfo.of(d.total, d.raidEstimate);
    return Padding(
      padding: const EdgeInsets.all(12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        ConsolePanel(
          title: 'Raid forecast',
          subText: 'estimate',
          padding: const EdgeInsets.all(14),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Row(crossAxisAlignment: CrossAxisAlignment.end, children: [
              Text('RAID IN ${clockFmt(d.raidInTicks)}', style: T.cond(size: 26, color: C.ember, weight: FontWeight.w700, spacing: 2)),
              const Spacer(),
              Text('${info.label}  ${info.r.toStringAsFixed(2)}', style: T.cond(size: 13, color: info.color, spacing: 1.4)),
            ]),
            const SizedBox(height: 10),
            _breakdown(d),
            const SizedBox(height: 6),
            Wrap(spacing: 14, runSpacing: 4, children: [
              _legend('Docked fleet', d.dockedFleet, C.own),
              _legend('Grid', d.grid, C.a400),
              _legend('Structures', d.structures, C.crystal),
              _legend('Raid est.', d.raidEstimate, C.ember),
            ]),
            const SizedBox(height: 4),
            RatioGauge(info.r),
          ]),
        ),
        const SizedBox(height: 10),
        ConsolePanel(
          title: 'Structures',
          child: Column(children: [for (final s in d.structureList) _structure(s)]),
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
        seg(d.dockedFleet, C.own),
        seg(d.grid, C.a400),
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

  Widget _structure(DefenceStructure s) => Container(
        padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
        decoration: const BoxDecoration(border: Border(bottom: BorderSide(color: C.lineLo))),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Row(children: [
            Expanded(child: Text(s.name.toUpperCase(), style: T.cond(size: 13, color: C.a200, spacing: 1.2))),
            Text('L${s.level}/${s.maxLevel}', style: T.mono(size: 12, color: C.a100)),
            const SizedBox(width: 12),
            Text('PWR ${s.power.round()}', style: T.mono(size: 11, color: C.text2)),
          ]),
          const SizedBox(height: 2),
          Text('NEEDS ${s.needs.toUpperCase()}   COST ${s.cost.toUpperCase()}', style: T.mono(size: 10, color: C.text3)),
          const SizedBox(height: 6),
          ConsoleButton('Build', small: true, reason: _noDefenceBuild),
          const WhyText(_noDefenceBuild),
        ]),
      );
}

class StorageTab extends StatelessWidget {
  final EconomyView eco;
  final GameState state;
  const StorageTab({super.key, required this.eco, required this.state});

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
        ConsolePanel(
          title: 'Upgrade',
          padding: const EdgeInsets.all(14),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            const ConsoleButton('Upgrade vault', small: true, reason: 'The server catalog has no Storage Vault building yet'),
            const WhyText('The server catalog has no Storage Vault building yet'),
          ]),
        ),
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
