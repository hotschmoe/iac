import 'package:flutter/material.dart';

import '../../console/intel.dart';
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
          TextSpan(text: groupDigits(have.floor()), style: T.mono(size: 11, color: short ? C.ember : C.text2)),
          TextSpan(text: ' / ', style: T.mono(size: 11, color: C.text3)),
          TextSpan(text: groupDigits(need.ceil()), style: T.mono(size: 11, color: C.a100, weight: FontWeight.w600)),
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
        if (s.effects.isNotEmpty) ..._effects(s.effects),
        if (s.energyAfter case final after?) ..._energyAfter(after),
        if (s.ship case final ship?) ..._shipStats(ship),
        if (s.defence case final defence?) ..._defenceStats(defence),
        if (cost != null) ...[
          const SizedBox(height: 12),
          const Lbl('Cost'),
          _costRow('Metal', C.metal, cost.metal, stock.metal),
          _costRow('Crystal', C.crystal, cost.crystal, stock.crystal),
          _costRow('Deuterium', C.deut, cost.deuterium, stock.deuterium),
          Kv('Build time', v: fmtTicks(s.ticks), vc: C.a100),
        ],
        const SizedBox(height: 12),
        const Lbl('Requires'),
        const SizedBox(height: 4),
        if (s.requires.isEmpty)
          Text('NO PREREQUISITES', style: T.mono(size: 11, color: C.text2))
        else
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
                Expanded(child: Text(_requirement(r), style: T.mono(size: 11, color: r.met ? C.text2 : C.ember))),
              ]),
            ),
        if (s.neededBy.isNotEmpty) ...[
          const SizedBox(height: 12),
          const Lbl('Needed by'),
          const SizedBox(height: 4),
          Text(s.neededBy.join(', ').toUpperCase(), key: const Key('hw-needed-by'), style: T.mono(size: 10.5, color: C.text2, height: 1.4)),
        ],
        if (s.batched) ...[
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

  /// "SHIPYARD L4  (HAVE 2)", or for energy what must be made and what is.
  String _requirement(proto.Requirement r) => r.energy
      ? '${groupDigits(r.need)} ENERGY MADE  (YOU MAKE ${groupDigits(r.have)})'
      : '${r.name.toUpperCase()} L${r.need}  (HAVE ${r.have})';

  List<Widget> _effects(List<proto.Effect> effects) => [
        const SizedBox(height: 10),
        const Lbl('Does'),
        for (final e in effects)
          Kv(e.kind.label,
              key: ValueKey('hw-effect-${e.kind.name}'),
              vw: Text.rich(TextSpan(children: [
                TextSpan(text: effectValue(e.kind, e.now), style: T.mono(size: 11, color: C.text)),
                if (e.next case final next?) ...[
                  TextSpan(text: '  >  ', style: T.mono(size: 11, color: C.text3)),
                  TextSpan(text: effectValue(e.kind, next), style: T.mono(size: 11, color: C.a100, weight: FontWeight.w600)),
                ],
              ]))),
      ];

  List<Widget> _energyAfter(proto.EnergyAfter after) => [
        Kv('Energy after',
            key: const ValueKey('hw-energy-after'),
            vw: Text.rich(TextSpan(children: [
              TextSpan(
                  text: '${(after.fraction * 100).round()}%',
                  style: T.mono(size: 11, color: after.fraction < 1 ? C.ember : C.text, weight: FontWeight.w600)),
              TextSpan(
                  text: '  ${groupDigits(after.made)} MADE / ${groupDigits(after.needed)} NEEDED',
                  style: T.mono(size: 11, color: C.text3)),
            ]))),
        if (after.plantMaxed)
          Text('SOLAR PLANT AT ITS TOP LEVEL', key: const Key('hw-plant-maxed'), style: T.mono(size: 10.5, color: C.ember)),
      ];

  List<Widget> _fight(double hull, double shield, double weapon, double power) => [
        Kv('Hull', v: groupDigits(hull.round())),
        Kv('Shield', v: groupDigits(shield.round())),
        Kv('Weapon', v: groupDigits(weapon.round())),
        Kv('Power', v: groupDigits(power.round()), vc: C.a100),
      ];

  List<Widget> _shipStats(proto.ShipOption o) {
    final hw = ctrl.state.hw;
    final world = ctrl.state.world;
    return [
      const SizedBox(height: 10),
      const Lbl('Specification'),
      ..._fight(o.hull, o.shield, o.weapon, o.power),
      if (o.flies) ...[
        Kv('Cargo', v: groupDigits(o.cargo.round())),
        if (o.drive case final drive?) Kv('Drive', v: drive.label.toUpperCase()),
        Kv('Speed', v: groupDigits(o.speed.round())),
        Kv('A jump takes', v: '${o.jumpTicks} TICK${o.jumpTicks == 1 ? '' : 'S'}'),
        Kv('A jump burns', v: '${trimmed(o.jumpFuel, 1)} FUEL'),
        Kv('Tank', v: groupDigits(o.tank.round())),
        Kv('Harvest', v: 'x${trimmed(o.harvest)}'),
      ] else ...[
        Kv('Flies', v: 'NO: STAYS AT HOME', vc: C.a300),
        if (hw != null) ...[
          Kv('Energy each', key: const Key('hw-satellite-energy'), v: '+${hw.energy.perSatellite}', vc: C.a100),
          Kv('At home', v: '${hw.satellites}'),
        ],
      ],
      if (world != null) ..._rapidFire(world.rapidFireOf(o.shipClass), world.rapidFireAgainst(proto.ShipyardItem.ship(o.shipClass))),
    ];
  }

  List<Widget> _defenceStats(proto.DefenceOption o) {
    final world = ctrl.state.world;
    return [
      const SizedBox(height: 10),
      const Lbl('Specification'),
      ..._fight(o.hull, o.shield, o.weapon, o.power),
      Kv('Standing', v: '${o.count}'),
      if (o.max case final most?) Kv('Limit', key: const Key('hw-defence-limit'), v: most == 1 ? 'ONE OF A KIND' : 'AT MOST $most', vc: C.a300),
      if (world != null) ..._rapidFire(const [], world.rapidFireAgainst(proto.ShipyardItem.defence(o.kind))),
    ];
  }

  /// What the selected class fires again at, and what fires again at it:
  /// after a shot at one of those it shoots once more, the more often the
  /// higher the figure.
  List<Widget> _rapidFire(List<proto.RapidFire> at, List<proto.RapidFire> from) => [
        if (at.isNotEmpty) ...[
          const SizedBox(height: 10),
          const Lbl('Rapid fire at'),
          const SizedBox(height: 4),
          Text([for (final r in at) '${r.against.label} ${r.shots}'].join(', ').toUpperCase(),
              key: const Key('hw-rapid-at'), style: T.mono(size: 10.5, color: C.own, height: 1.4)),
        ],
        if (from.isNotEmpty) ...[
          const SizedBox(height: 10),
          const Lbl('Rapid fire from'),
          const SizedBox(height: 4),
          Text([for (final r in from) '${r.shipClass.label} ${r.shots}'].join(', ').toUpperCase(),
              key: const Key('hw-rapid-from'), style: T.mono(size: 10.5, color: C.ember, height: 1.4)),
        ],
      ];
}
