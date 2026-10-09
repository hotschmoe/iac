import 'package:flutter/material.dart';

import '../../console/threat.dart';
import '../../design/panel.dart';
import '../../design/tokens.dart';
import '../../protocol/protocol.dart' as proto;
import '../../state/game_controller.dart';
import 'spec.dart';

class Inspector extends StatelessWidget {
  final ItemSpec? spec;
  final GameController ctrl;
  final VoidCallback onQueue;
  const Inspector({super.key, required this.spec, required this.ctrl, required this.onQueue});

  Widget _costRow(String name, Color col, double need, double have) {
    final short = need > have;
    return Kv(name,
        vw: Text.rich(TextSpan(children: [
          TextSpan(text: '${have.floor()}', style: T.mono(size: 11, color: short ? C.ember : C.text2)),
          TextSpan(text: ' / ', style: T.mono(size: 11, color: C.text3)),
          TextSpan(text: '${need.ceil()}', style: T.mono(size: 11, color: C.a100, weight: FontWeight.w600)),
        ])));
  }

  @override
  Widget build(BuildContext context) {
    final s = spec;
    if (s == null) {
      return const ConsolePanel(
          title: 'Inspector', child: Padding(padding: EdgeInsets.all(14), child: Lbl('Select a card')));
    }
    final stock = ctrl.state.stock;
    final cost = s.cost;
    return ConsolePanel(
      title: s.kind,
      padding: const EdgeInsets.fromLTRB(16, 10, 16, 14),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text(s.title.toUpperCase(), style: T.cond(size: 26, color: C.a100, weight: FontWeight.w700, spacing: 1.6, height: 1.05)),
        const SizedBox(height: 6),
        Text(s.subtitle, style: T.mono(size: 18, color: C.a200, weight: FontWeight.w500)),
        if (s.ship != null) ..._shipStats(s.ship!),
        if (cost != null) ...[
          const SizedBox(height: 12),
          const Lbl('Cost'),
          _costRow('Metal', C.metal, cost.metal, stock.metal),
          _costRow('Crystal', C.crystal, cost.crystal, stock.crystal),
          _costRow('Deuterium', C.deut, cost.deuterium, stock.deuterium),
          Kv('Build time', v: fmtTicks(s.ticks), vc: C.a100),
        ],
        if (s.requires.isNotEmpty) ...[
          const SizedBox(height: 12),
          const Lbl('Requires'),
          const SizedBox(height: 4),
          for (final r in s.requires)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 2),
              child: Row(children: [
                Container(
                    width: 11,
                    height: 11,
                    decoration: BoxDecoration(border: Border.all(color: r.met ? C.own : C.ember)),
                    alignment: Alignment.center,
                    child: Text(r.met ? 'Y' : 'N', style: T.mono(size: 8, color: r.met ? C.own : C.ember, weight: FontWeight.w600))),
                const SizedBox(width: 8),
                Expanded(child: Text('${r.name.toUpperCase()} L${r.need}  (HAVE ${r.have})', style: T.mono(size: 11, color: r.met ? C.text2 : C.ember))),
              ]),
            )
        ] else ...[
          const SizedBox(height: 12),
          const Lbl('Requires'),
          const SizedBox(height: 4),
          Text('NO PREREQUISITES', style: T.mono(size: 11, color: C.text2)),
        ],
        if (s.ship != null) ...[
          const SizedBox(height: 12),
          const Lbl('Batch'),
          const SizedBox(height: 4),
          Row(children: [
            for (final n in shipBatches)
              Padding(
                padding: const EdgeInsets.only(right: 6),
                child: ConsoleButton('x$n',
                    key: ValueKey('hw-batch-$n'), small: true, active: ctrl.shipBatch == n, onPressed: () => ctrl.setShipBatch(n)),
              ),
          ]),
        ],
        const SizedBox(height: 14),
        ConsoleButton(s.actionLabel,
            key: const ValueKey('hw-queue'),
            primary: true,
            keyHint: 'Enter',
            onPressed: s.canQueue ? onQueue : null,
            reason: s.reason),
        if (s.reason != null) WhyText(s.reason!),
        if (s.note.isNotEmpty) ...[
          const SizedBox(height: 10),
          Text.rich(TextSpan(children: [
            TextSpan(text: 'NOTE ', style: T.mono(size: 11, color: C.a700, weight: FontWeight.w600, spacing: 1)),
            TextSpan(text: s.note, style: T.mono(size: 11, color: C.text3)),
          ])),
        ],
      ]),
    );
  }

  List<Widget> _shipStats(proto.ShipClass c) {
    final st = shipStats[c]!;
    return [
      const SizedBox(height: 10),
      const Lbl('Specification'),
      Kv('Hull', v: '${st.hull.round()}'),
      Kv('Shield', v: '${st.shield.round()}'),
      Kv('Weapon', v: '${st.weapon.round()}'),
      Kv('Cargo', v: '${st.cargo.round()}'),
      Kv('Power', v: st.power.toStringAsFixed(1), vc: C.a100),
    ];
  }
}
