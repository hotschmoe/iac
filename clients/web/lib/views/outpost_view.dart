import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../console/economy_view.dart' show shortfall;
import '../console/intel.dart';
import '../console/outpost.dart';
import '../console/services.dart';
import '../design/gauges.dart';
import '../design/panel.dart';
import '../design/tokens.dart';
import '../protocol/protocol.dart' as proto;
import '../state/game_controller.dart';
import '../state/state_mapper.dart' show amountsOf, shortBy;
import 'homeworld/economy_tabs.dart' show HoldDock, HoldPanel;

const _abbr = {proto.ResourceKind.metal: 'Fe', proto.ResourceKind.crystal: 'Cr', proto.ResourceKind.deuterium: 'De'};

String _whole(double v) => groupDigits(v.floor());

/// What a survey says of a site, as rows: the ore and its grade, what level
/// 1 would make, the raids against the platform, the way home, the time,
/// and the cost set against what [hold] carries.
class SurveyReadout extends StatelessWidget {
  final proto.OutpostSurvey survey;
  final proto.Resources hold;
  const SurveyReadout({super.key, required this.survey, required this.hold});

  @override
  Widget build(BuildContext context) {
    final s = survey;
    final exposed = s.raidPower > s.platformPower;
    final ore = [(proto.ResourceKind.metal, s.densities.metal), (proto.ResourceKind.crystal, s.densities.crystal), (proto.ResourceKind.deuterium, s.densities.deuterium)];
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
      Kv('Site', v: '${s.sector.q}, ${s.sector.r} - RING ${s.ring}'),
      Kv('Grade', key: const Key('survey-grade'), v: '${s.grade.toStringAsFixed(1)} OF 12'),
      Padding(
        padding: const EdgeInsets.symmetric(vertical: 3),
        child: Text.rich(
          TextSpan(children: [
            for (final (k, density) in ore) TextSpan(text: '${_abbr[k]} ${density.label.toUpperCase()}   ', style: TextStyle(color: C.ore(k.name))),
          ]),
          style: T.mono(size: 10),
        ),
      ),
      Kv('Level 1 makes', key: const Key('survey-yield'), v: '${yieldAnHour(s.production).toUpperCase()} AN HOUR'),
      Kv('Raid against platform',
          key: const Key('survey-raid'), v: '${s.raidPower.round()} AGAINST ${s.platformPower.round()}', vc: exposed ? C.ember : C.ok),
      Kv('Way home', v: hopsHome(s.hopsHome).toUpperCase()),
      Kv('Level 1 takes', key: const Key('survey-time'), v: clockFmt(s.buildTicks)),
      const SizedBox(height: 6),
      const Lbl('Level 1 costs, from this hold'),
      for (final k in proto.ResourceKind.values) _costRow(k, k.of(s.cost), k.of(hold)),
    ]);
  }

  Widget _costRow(proto.ResourceKind k, double cost, double aboard) {
    final short = cost - aboard;
    return Padding(
      key: Key('survey-cost-${k.name}'),
      padding: const EdgeInsets.only(top: 3),
      child: Row(crossAxisAlignment: CrossAxisAlignment.baseline, textBaseline: TextBaseline.alphabetic, children: [
        SizedBox(width: 22, child: Text(_abbr[k]!.toUpperCase(), style: T.cond(size: 10.5, color: C.ore(k.name), spacing: 1.2))),
        Text(groupDigits(cost.ceil()), style: T.mono(size: 11, color: C.text)),
        const SizedBox(width: 8),
        Expanded(
            child: Text(short > 0 ? 'ABOARD ${_whole(aboard)} - SHORT ${groupDigits(short.ceil())}' : 'ABOARD ${_whole(aboard)}',
                textAlign: TextAlign.right, style: T.mono(size: 10.5, color: short > 0 ? C.ember : C.text3))),
      ]),
    );
  }
}

/// The survey of the sector the selected fleet stands in, with the cost set
/// against its hold, and the order that founds the outpost there. The
/// verdict is the server's: its sentence is shown as it gave it.
class FoundingPanel extends StatelessWidget {
  final GameController controller;
  const FoundingPanel({super.key, required this.controller});

  @override
  Widget build(BuildContext context) {
    final c = controller;
    final st = c.state;
    final fleet = c.currentFleet;
    c.requestSurvey(fleet.id, fleet.sector);
    final survey = st.surveyOf(fleet.id, fleet.sector)?.survey;
    final why = survey == null ? 'Waiting for the survey.' : survey.refusal;
    return ConsolePanel(
      key: const ValueKey('founding-panel'),
      title: 'Found an outpost',
      subText: fleet.name,
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
        if (survey == null)
          Text('ASKING FOR THE SURVEY', key: const Key('found-pending'), style: T.mono(size: 11, color: C.text3))
        else
          SurveyReadout(survey: survey, hold: st.holds[fleet.id] ?? const proto.Resources()),
        const SizedBox(height: 10),
        Align(
          alignment: Alignment.centerLeft,
          child: ConsoleButton('Found outpost', key: const ValueKey('found-order'), small: true, primary: why == null, onPressed: why == null ? c.foundOutpost : null, reason: why),
        ),
        if (why != null) WhyText(why, key: const Key('found-why')),
        const WhyText('One outpost an empire, on a sector of rings 9 to 20. Its first level is paid from this hold; until it is '
            'finished it has a store and nothing else. Raiders come for it as they do for the homeworld, and a raid it loses '
            'breaches it and destroys a quarter of the ships standing there.'),
      ]),
    );
  }
}

/// The player's outpost: what it stands at and is doing, its store against
/// cap and protected floor, its yield, its defence against the raids of its
/// ring, the next level with Upgrade, and Abandon behind a second press.
class OutpostPanel extends StatefulWidget {
  final GameController controller;
  const OutpostPanel({super.key, required this.controller});
  @override
  State<OutpostPanel> createState() => _OutpostPanelState();
}

class _OutpostPanelState extends State<OutpostPanel> {
  bool _abandoning = false;
  GameController get c => widget.controller;

  @override
  Widget build(BuildContext context) {
    final st = c.state;
    final o = st.outpost;
    if (o == null) {
      return const ConsolePanel(
        key: ValueKey('outpost-panel'),
        title: 'Outpost',
        subText: 'none',
        padding: EdgeInsets.fromLTRB(12, 8, 12, 12),
        child: WhyText(
            'You have no outpost. One is founded by a fleet standing in a sector of rings 9 to 20 with the cost of level 1 in its hold: '
            'open the founding survey there with O on the windshield, or type found.',
            key: Key('outpost-none')),
      );
    }
    final work = o.work;
    final raid = o.raidArrivalTick;
    final cover = o.defencePower / math.max(1, o.nextRaidEstimatePower);
    final ships = math.max(0.0, o.defencePower - o.platformPower);
    final repair = o.repairCost;
    final repairShort = repair == null ? null : shortfall(o.store, repair);
    return ConsolePanel(
      key: const ValueKey('outpost-panel'),
      title: 'Outpost',
      subText: '${o.location.q}, ${o.location.r} / ring ${o.location.distFromOrigin}',
      border: o.breached ? C.ember : null,
      mark: o.breached ? C.ember : null,
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
        Text(outpostStanding(o, st.tick), key: const Key('outpost-standing'), style: T.cond(size: 12.5, color: o.breached ? C.ember : C.a100, spacing: 1.3, height: 1.25)),
        if (work != null) ...[
          const SizedBox(height: 5),
          LedBar((st.tick - work.startTick) / math.max(1, work.endTick - work.startTick), color: work.kind == proto.OutpostWork.repair ? C.ember : C.own, height: 7),
        ],
        if (o.breached)
          WhyText('Breached: no extractor, no refuelling and no platform until the repair is done. The store, loading and unloading work.', key: const Key('outpost-breach')),
        if (repair != null)
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Text(
                'THE REPAIR COSTS ${amountsOf(repair).toUpperCase()} FROM THE STORE'
                '${repairShort == null ? '' : ', WHICH IS ${shortBy(repairShort).toUpperCase()}: IT STAYS BREACHED UNTIL A FLEET UNLOADS THE REST'}',
                key: const Key('outpost-repair'),
                style: T.mono(size: 10.5, color: repairShort == null ? C.text2 : C.ember, height: 1.35)),
          ),
        const SizedBox(height: 10),
        const Lbl('Store'),
        for (final k in proto.ResourceKind.values) _store(o, k),
        const SizedBox(height: 4),
        Kv('Yield an hour',
            key: const Key('outpost-yield'),
            v: o.inService ? yieldAnHour(o.production).toUpperCase() : 'NOTHING: ${o.breached ? 'BREACHED' : 'NOT YET OPEN'}',
            vc: o.inService ? null : C.text3),
        Kv('Site grade', v: '${o.grade.toStringAsFixed(1)} OF 12'),
        Kv('Way home', v: hopsHome(o.hopsHome).toUpperCase()),
        const SizedBox(height: 8),
        const Lbl('Defence'),
        Kv('Platform and ships here',
            key: const Key('outpost-defence'),
            v: '${groupDigits(o.platformPower.round())} + ${groupDigits(ships.round())} = ${groupDigits(o.defencePower.round())}'),
        Kv('Raid estimate', key: const Key('outpost-raid-estimate'), v: groupDigits(o.nextRaidEstimatePower.round())),
        Padding(
          padding: const EdgeInsets.only(top: 4),
          child: Wrap(spacing: 10, runSpacing: 2, crossAxisAlignment: WrapCrossAlignment.center, children: [
            Text(raid == null ? 'NO RAID ANNOUNCED' : 'RAID ARRIVES IN ${clockFmt(raid - st.tick)}',
                key: const Key('outpost-raid'), style: T.cond(size: 13, color: raid == null ? C.text2 : C.ember, weight: FontWeight.w700, spacing: 1.6)),
            Text('DEFENCE ${cover.toStringAsFixed(2)}X THE ESTIMATE',
                key: const Key('outpost-cover'), style: T.cond(size: 11, color: cover >= 1 ? C.ok : C.ember, spacing: 1.3)),
          ]),
        ),
        const WhyText('The estimate is the middle of the raids this ring sends; one may come nearly a fifth stronger. A raid the outpost '
            'loses breaches it, takes a quarter of what the store and the holds here carry above the protected floor, and destroys a '
            'quarter of the ships standing here.'),
        const SizedBox(height: 10),
        ..._next(o),
        const SizedBox(height: 12),
        ..._abandon(o),
      ]),
    );
  }

  Widget _store(proto.OutpostState o, proto.ResourceKind k) {
    final stock = k.of(o.store), cap = k.of(o.storeCap), floor = k.of(o.protected);
    final full = storeAtCap(o, k);
    final color = C.ore(k.name);
    return Padding(
      key: Key('outpost-store-${k.name}'),
      padding: const EdgeInsets.only(top: 6),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
        Row(crossAxisAlignment: CrossAxisAlignment.baseline, textBaseline: TextBaseline.alphabetic, children: [
          Text(k.name.toUpperCase(), style: T.cond(size: 10.5, color: color, spacing: 1.4)),
          const SizedBox(width: 8),
          Expanded(child: Text('${_whole(stock)} / ${_whole(cap)}', textAlign: TextAlign.right, style: T.mono(size: 11.5, color: full ? C.ember : C.text))),
          if (full) ...[const SizedBox(width: 8), const Lamp('Full', color: C.ember)],
        ]),
        const SizedBox(height: 3),
        LedBar(cap > 0 ? stock / cap : 0, color: full ? C.ember : color, height: 7, mark: C.text2, markAt: cap > 0 ? (floor / cap).clamp(0.0, 1.0) : null),
        const SizedBox(height: 2),
        Wrap(spacing: 10, children: [
          Text('PROTECTED ${groupDigits(floor.round())}', style: T.mono(size: 10, color: C.text3)),
          Text('RAIDABLE ${_whole(math.max(0, stock - floor))}', style: T.mono(size: 10, color: C.text3)),
        ]),
      ]),
    );
  }

  List<Widget> _next(proto.OutpostState o) {
    final next = o.nextLevel;
    if (next == null) {
      return [Text('LEVEL ${o.maxLevel} IS THE HIGHEST.', key: const Key('outpost-max'), style: T.mono(size: 10.5, color: C.text3))];
    }
    final building = o.work?.kind == proto.OutpostWork.build ? o.work!.level : null;
    final short = shortfall(o.store, next.cost);
    final ready = short == null && o.work == null && !o.breached;
    return [
      Lbl(building == null ? 'Next: level ${next.level}' : 'After level $building: level ${next.level}'),
      Kv('Costs, from the store', key: const Key('outpost-next-cost'), v: amountsOf(next.cost).toUpperCase()),
      Kv('Takes', v: clockFmt(next.ticks)),
      Kv('Makes an hour', v: yieldAnHour(next.production).toUpperCase()),
      Kv('Store takes', v: amountsOf(next.storeCap).toUpperCase()),
      Kv('Platform', v: groupDigits(next.platformPower.round())),
      if (short != null)
        Padding(
          padding: const EdgeInsets.only(top: 4),
          child: Text('THE STORE IS ${shortBy(short).toUpperCase()}', key: const Key('outpost-short'), style: T.mono(size: 10.5, color: C.ember, height: 1.35)),
        ),
      const SizedBox(height: 8),
      Align(
        alignment: Alignment.centerLeft,
        child: ConsoleButton('Upgrade to level ${next.level}', key: const ValueKey('outpost-upgrade'), small: true, primary: ready, onPressed: c.upgradeOutpost),
      ),
      const WhyText('A level is paid in full from the outpost\'s own store when it is ordered, and the outpost does one job at a time.'),
    ];
  }

  List<Widget> _abandon(proto.OutpostState o) => [
        if (!_abandoning)
          Align(
            alignment: Alignment.centerLeft,
            child: ConsoleButton('Abandon outpost', key: const ValueKey('outpost-abandon'), small: true, danger: true, onPressed: () => setState(() => _abandoning = true)),
          )
        else ...[
          Text(
              'ABANDON THE OUTPOST AT ${o.location.q}, ${o.location.r}? ITS STORE IS LEFT AS WRECKAGE IN THE SECTOR, ITS SCORE IS GONE AND NOTHING IS REFUNDED.',
              key: const Key('outpost-abandon-warning'),
              style: T.mono(size: 10.5, color: C.ember, height: 1.35)),
          const SizedBox(height: 6),
          Wrap(spacing: 6, runSpacing: 6, children: [
            ConsoleButton('Abandon it', key: const ValueKey('outpost-abandon-confirm'), small: true, danger: true, onPressed: () {
              setState(() => _abandoning = false);
              c.abandonOutpost();
            }),
            ConsoleButton('Keep it', key: const ValueKey('outpost-abandon-cancel'), small: true, onPressed: () => setState(() => _abandoning = false)),
          ]),
        ],
      ];
}

/// The outpost's screen: its panel and the dock's load controls, side by
/// side where there is the width.
class OutpostView extends StatelessWidget {
  const OutpostView({super.key});

  static const _twoColumns = 760.0;

  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    return ListenableBuilder(
      listenable: con.game,
      builder: (context, _) => LayoutBuilder(builder: (context, box) {
        final station = OutpostPanel(controller: con.game);
        final dock = con.state.outpost == null ? null : HoldPanel(state: con.state, ctrl: con.game, dock: HoldDock.outpost);
        final wide = box.maxWidth >= _twoColumns && dock != null;
        return Container(
          color: C.void_,
          alignment: Alignment.topCenter,
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(12),
            child: ConstrainedBox(
              constraints: BoxConstraints(maxWidth: wide ? 980 : 520),
              child: wide
                  ? Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                      Expanded(child: station),
                      const SizedBox(width: 12),
                      Expanded(child: dock),
                    ])
                  : Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                      station,
                      if (dock != null) ...[const SizedBox(height: 10), dock],
                    ]),
            ),
          ),
        );
      }),
    );
  }
}
