import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../console/battle.dart' show damageLine, dockRepairLine, platformList, shipsOf;
import '../console/intel.dart';
import '../console/outpost.dart';
import '../console/services.dart';
import '../design/gauges.dart';
import '../design/panel.dart';
import '../design/tokens.dart';
import '../protocol/protocol.dart' as proto;
import '../state/game_controller.dart';
import '../state/state_mapper.dart' show amountsOf, shortBy;
import 'battle_view.dart' show BattlePanel, RaidForecastReadout, RaidSummary, ShipRows, UnitRows, damagedColor;
import 'windshield/parts.dart' show RaidExits;
import 'homeworld/economy_tabs.dart' show HoldDock, HoldPanel;

const _abbr = {proto.ResourceKind.metal: 'Fe', proto.ResourceKind.crystal: 'Cr', proto.ResourceKind.deuterium: 'De'};

String _whole(double v) => groupDigits(v.floor());

/// What a survey says of a site, as rows: the ore and its grade, what level
/// 1 would make, the way home, the platform of level 1 and how a typical
/// raid of the ring would go against it alone, the time, and the cost set
/// against what [hold] carries.
class SurveyReadout extends StatelessWidget {
  final proto.OutpostSurvey survey;
  final proto.Resources hold;
  const SurveyReadout({super.key, required this.survey, required this.hold});

  @override
  Widget build(BuildContext context) {
    final s = survey;
    final site = s.site;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
      Kv('Site', v: '${s.sector.q}, ${s.sector.r} - RING ${s.ring}'),
      if (site == null)
        const Kv('Grade', key: Key('survey-grade'), v: 'NOT ON YOUR CHART', vc: C.text3)
      else ...[
        Kv('Grade', key: const Key('survey-grade'), v: gradeOfMax(site.grade)),
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 3),
          child: Text.rich(
            TextSpan(children: [
              for (final (k, density) in [
                (proto.ResourceKind.metal, site.densities.metal),
                (proto.ResourceKind.crystal, site.densities.crystal),
                (proto.ResourceKind.deuterium, site.densities.deuterium),
              ])
                TextSpan(text: '${_abbr[k]} ${density.label.toUpperCase()}   ', style: TextStyle(color: C.ore(k.name))),
            ]),
            style: T.mono(size: 10),
          ),
        ),
        Kv('Level 1 makes', key: const Key('survey-yield'), v: '${yieldAnHour(site.production).toUpperCase()} AN HOUR'),
        Kv('Way home', v: hopsHome(site.hopsHome).toUpperCase()),
      ],
      Kv('Platform at level 1', key: const Key('survey-platform'), v: platformList(s.platform).toUpperCase()),
      Kv('Raids of this ring, power', key: const Key('survey-raid'), v: raidRange(s.raidPower).toUpperCase()),
      if (s.raid case final raid?) ...[
        const SizedBox(height: 6),
        const Lbl('A raid of this ring against that platform alone'),
        const SizedBox(height: 3),
        RaidForecastReadout(raid, tick: 0, tag: 'survey-forecast', rule: false, headline: false),
        const SizedBox(height: 6),
      ],
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
        const WhyText('One outpost an empire, on a sector of $outpostRings. Its first level is paid from this hold; until it is '
            'finished it has a store and nothing else. Raiders come for it as they do for the homeworld: its platform and the ships '
            'standing there fight them, and a raid that leaves none of them standing breaches it and takes its share of the store.'),
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
            'You have no outpost. One is founded by a fleet standing in a sector of $outpostRings with the cost of level 1 in its hold: '
            'open the founding survey there with O on the windshield, or type found.',
            key: Key('outpost-none')),
      );
    }
    final work = o.work;
    final raid = o.raid;
    final repair = o.repairCost;
    final repairShort = o.repairWaitingFor;
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
                '${repairShort == null ? '' : ', WHICH IS ${shortBy(repairShort).toUpperCase()}: IT STAYS BREACHED UNTIL A FLEET UNLOADS THAT, '
                    'OR CRYSTAL OR DEUTERIUM IN ITS PLACE, A UNIT FOR 1.5 AND FOR 2 OF METAL'}',
                key: const Key('outpost-repair'),
                style: T.mono(size: 10.5, color: repairShort == null ? C.text2 : C.ember, height: 1.35)),
          ),
        const SizedBox(height: 10),
        const Lbl('Store'),
        for (final k in proto.ResourceKind.values) _store(o, k),
        const WhyText(
            'Protected is what the next repair would take: metal, and where the metal runs out crystal, then deuterium, a unit for '
            '1.5 and for 2 of metal.',
            key: Key('outpost-floor')),
        const SizedBox(height: 4),
        Kv('Yield an hour',
            key: const Key('outpost-yield'),
            v: o.inService ? yieldAnHour(o.production).toUpperCase() : 'NOTHING: ${o.breached ? 'BREACHED' : 'NOT YET OPEN'}',
            vc: o.inService ? null : C.text3),
        Kv('Site grade', v: gradeOfMax(o.grade)),
        Kv('Way home', v: hopsHome(o.hopsHome).toUpperCase()),
        const SizedBox(height: 8),
        const Lbl('What fights a raid here'),
        if (o.platform.isEmpty)
          Padding(
              padding: const EdgeInsets.only(top: 3),
              child: Text('NO PLATFORM: ${o.breached ? 'BREACHED' : 'NOT YET OPEN'}',
                  key: const Key('outpost-platform-none'), style: T.mono(size: 10.5, color: o.breached ? C.ember : C.text3)))
        else ...[
          Padding(padding: const EdgeInsets.only(top: 4), child: Text('THE PLATFORM', style: T.mono(size: 10.5, color: C.text2))),
          UnitRows(o.platform, tag: 'outpost-platform'),
        ],
        for (final f in st.fleets)
          if (f.atOutpost) ...[
            Padding(
                padding: const EdgeInsets.only(top: 5),
                child: Text('${f.name} HERE - ${shipsOf(f.shipCount).toUpperCase()}', key: Key('outpost-fleet-${f.id}'), style: T.mono(size: 10.5, color: C.text2))),
            ShipRows(f.ships, tag: 'outpost-fleet-${f.id}'),
            if (f.repair case final q?)
              if (q.outpost case final dock?)
                Text('${damageLine(q)}; the outpost ${dockRepairLine(q, dock)}'.toUpperCase(),
                    key: Key('outpost-mend-${f.id}'), style: T.mono(size: 10, color: damagedColor, height: 1.35)),
          ],
        const SizedBox(height: 4),
        Kv('Raids of this ring, power', key: const Key('outpost-raid-range'), v: raidRange(o.raidPower).toUpperCase()),
        const SizedBox(height: 8),
        const Lbl('Raid forecast'),
        const SizedBox(height: 4),
        if (raid == null)
          Text('NOTHING RAIDS AN OUTPOST BEFORE ITS FIRST LEVEL IS FINISHED.', key: const Key('outpost-raid-none'), style: T.mono(size: 10.5, color: C.text3))
        else
          RaidForecastReadout(raid, tick: st.tick, tag: 'outpost-raid', rule: false),
        const WhyText('A raid is a fight, a round a tick for at most ${proto.raidRounds}: the raiders against the platform and every ship of '
            'yours standing here. The platform stands whole again after every fight that does not breach the outpost. A raid that leaves '
            'no defender standing breaches it and takes its share of what the store holds above the protected floor.'),
        const SizedBox(height: 10),
        ..._next(o),
        const SizedBox(height: 12),
        ..._abandon(o),
      ]),
    );
  }

  Widget _store(proto.OutpostState o, proto.ResourceKind k) {
    final stock = k.of(o.store), cap = k.of(o.storeCap), floor = k.of(o.protected);
    final lost = o.yieldLost.contains(k);
    final full = lost || storeAtCap(o, k);
    final color = C.ore(k.name);
    return Padding(
      key: Key('outpost-store-${k.name}'),
      padding: const EdgeInsets.only(top: 6),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
        Row(crossAxisAlignment: CrossAxisAlignment.baseline, textBaseline: TextBaseline.alphabetic, children: [
          Text(k.name.toUpperCase(), style: T.cond(size: 10.5, color: color, spacing: 1.4)),
          const SizedBox(width: 8),
          Expanded(child: Text('${_whole(stock)} / ${_whole(cap)}', textAlign: TextAlign.right, style: T.mono(size: 11.5, color: lost ? C.ember : C.text))),
          if (full) ...[const SizedBox(width: 8), Lamp(lost ? 'Full, yield lost' : 'Full', color: lost ? C.ember : C.a300)],
        ]),
        const SizedBox(height: 3),
        LedBar(cap > 0 ? stock / cap : 0, color: lost ? C.ember : color, height: 7, mark: C.text2, markAt: cap > 0 ? (floor / cap).clamp(0.0, 1.0) : null),
        const SizedBox(height: 2),
        Wrap(spacing: 10, children: [
          Text('PROTECTED ${_whole(floor)}', style: T.mono(size: 10, color: C.text3)),
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
    final why = o.upgradeRefusal;
    return [
      Lbl(building == null ? 'Next: level ${next.level}' : 'After level $building: level ${next.level}'),
      Kv('Costs, from the store', key: const Key('outpost-next-cost'), v: amountsOf(next.cost).toUpperCase()),
      Kv('Takes', v: clockFmt(next.ticks)),
      Kv('Makes an hour', v: yieldAnHour(next.production).toUpperCase()),
      Kv('Store takes', v: amountsOf(next.storeCap).toUpperCase()),
      Kv('Platform', key: const Key('outpost-next-platform'), v: platformList(next.platform).toUpperCase(), size: 10),
      const SizedBox(height: 8),
      Align(
        alignment: Alignment.centerLeft,
        child: ConsoleButton('Upgrade to level ${next.level}',
            key: const ValueKey('outpost-upgrade'), small: true, primary: why == null, onPressed: why == null ? c.upgradeOutpost : null, reason: why),
      ),
      if (why != null)
        WhyText(why, key: const Key('outpost-upgrade-why'))
      else
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

/// What a raid on the outpost is doing and did, under the outpost's panel
/// on its own screen: the fight round by round while it lasts and its report
/// after, the ways out for the fleets standing there, and the last raid's
/// summary.
class OutpostRaid extends StatelessWidget {
  final GameController controller;
  const OutpostRaid({super.key, required this.controller});

  @override
  Widget build(BuildContext context) {
    final st = controller.state;
    final post = st.outpost;
    if (post == null) return const SizedBox.shrink();
    final fight = st.raidAtOutpost;
    final last = st.lastOutpostRaid;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
      if (fight != null) Padding(padding: const EdgeInsets.only(top: 10), child: BattlePanel(key: ValueKey(fight.sector), battle: fight)),
      RaidExits(controller: controller, at: post.location),
      if (last != null && !(fight?.over ?? false))
        Padding(
          padding: const EdgeInsets.only(top: 10),
          child: ConsolePanel(
            title: 'Last raid',
            padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
            child: RaidSummary(last, tick: st.tick, tag: 'outpost-last-raid'),
          ),
        ),
    ]);
  }
}

/// The outpost's screen: its panel with a raid's fight under it, and the
/// dock's load controls, side by side where there is the width.
class OutpostView extends StatelessWidget {
  const OutpostView({super.key});

  static const _twoColumns = 760.0;

  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    return ListenableBuilder(
      listenable: con.game,
      builder: (context, _) => LayoutBuilder(builder: (context, box) {
        final station = Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
          OutpostPanel(controller: con.game),
          OutpostRaid(controller: con.game),
        ]);
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

/// The relay of the sector the selected fleet stands in, or the survey of
/// the sector for founding one there: the store against its cap, how long
/// it stands unvisited, the deuterium moved to and from the hold, and the
/// order that founds it. The verdict is the server's: its sentence is shown
/// as it gave it.
class RelayPanel extends StatelessWidget {
  final GameController controller;
  const RelayPanel({super.key, required this.controller});

  @override
  Widget build(BuildContext context) {
    final c = controller;
    final st = c.state;
    final fleet = c.currentFleet;
    final relays = st.relays;
    final quote = fleet.relay;
    final here = quote == null ? null : st.relayAt(fleet.sector);
    final hold = st.holds[fleet.id] ?? const proto.Resources();
    final body = <Widget>[
      Kv('Relays kept', key: const Key('relay-count'), v: '${relays.relays.length} OF ${relays.limit} - ${relays.technology.label.toUpperCase()} L${relays.level}'),
      Kv('Fills tanks to', key: const Key('relay-fill'), v: '${relays.fillJumps} JUMPS'),
      const SizedBox(height: 6),
    ];
    if (here != null && quote != null) {
      final loadWhy = quote.loadRefusal;
      final unloadWhy = quote.unloadRefusal;
      body.addAll([
        Kv('Store', key: const Key('relay-store'), v: '${_whole(here.store)} OF ${_whole(proto.relayStore)} DEUTERIUM'),
        const SizedBox(height: 4),
        LedBar(here.store / proto.relayStore, color: C.ore('deuterium')),
        const SizedBox(height: 6),
        Kv('Gone in', key: const Key('relay-decay'), v: clockFmt(here.decayIn)),
        const SizedBox(height: 10),
        Wrap(spacing: 8, runSpacing: 6, children: [
          ConsoleButton(loadWhy == null ? 'Load ${_whole(quote.load)} De' : 'Load',
              key: const ValueKey('relay-load'),
              small: true,
              primary: loadWhy == null,
              onPressed: loadWhy == null ? () => c.loadHold(fleet.id, proto.ResourceKind.deuterium) : null,
              reason: loadWhy),
          ConsoleButton('Unload',
              key: const ValueKey('relay-unload'),
              small: true,
              onPressed: unloadWhy == null ? () => c.unloadHold(fleet.id) : null,
              reason: unloadWhy),
        ]),
        if (loadWhy != null) WhyText(loadWhy, key: const Key('relay-why')),
        const WhyText(relayDockVoice),
      ]);
    } else {
      c.requestRelaySurvey(fleet.id, fleet.sector);
      final survey = st.relaySurveyOf(fleet.id, fleet.sector)?.survey;
      final why = survey == null ? 'Waiting for the survey.' : survey.refusal;
      body.add(survey == null
          ? Text('ASKING FOR THE SURVEY', key: const Key('relay-pending'), style: T.mono(size: 11, color: C.text3))
          : Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              Kv('Site', v: '${survey.sector.q}, ${survey.sector.r} - RING ${survey.ring}'),
              Kv('Store', v: '${_whole(proto.relayStore)} DEUTERIUM'),
              Kv('Gone after', v: '${clockFmt(survey.decayTicks)} UNVISITED'),
              const SizedBox(height: 6),
              const Lbl('Costs, from this hold'),
              for (final k in proto.ResourceKind.values)
                if (k.of(survey.cost) > 0)
                  Padding(
                    key: Key('relay-cost-${k.name}'),
                    padding: const EdgeInsets.only(top: 3),
                    child: Row(children: [
                      SizedBox(width: 22, child: Text(_abbr[k]!.toUpperCase(), style: T.cond(size: 10.5, color: C.ore(k.name), spacing: 1.2))),
                      Text(groupDigits(k.of(survey.cost).ceil()), style: T.mono(size: 11, color: C.text)),
                      const Spacer(),
                      Text('ABOARD ${_whole(k.of(hold))}', style: T.mono(size: 10.5, color: k.of(hold) < k.of(survey.cost) ? C.ember : C.text3)),
                    ]),
                  ),
            ]));
      body.addAll([
        const SizedBox(height: 10),
        Align(
          alignment: Alignment.centerLeft,
          child: ConsoleButton('Found relay',
              key: const ValueKey('relay-found'), small: true, primary: why == null, onPressed: why == null ? c.foundRelay : null, reason: why),
        ),
        if (why != null) WhyText(why, key: const Key('relay-found-why')),
        const WhyText('A depot of deuterium on a sector of ring ${proto.relayMinRing} or deeper, yours alone: it makes nothing and no raid '
            'comes to it. The fuel in it is what you haul there.'),
      ]);
    }
    final others = [for (final r in relays.relays) if (r.sector != fleet.sector) r];
    if (others.isNotEmpty) {
      body.addAll([
        const SizedBox(height: 10),
        const Lbl('Your other relays'),
        for (final r in others) Padding(padding: const EdgeInsets.only(top: 3), child: Text(relayLine(r), key: Key('relay-${r.sector.q}-${r.sector.r}'), style: T.mono(size: 10.5, color: C.text2))),
      ]);
    }
    return ConsolePanel(
      key: const ValueKey('relay-panel'),
      title: here == null ? 'Found a relay' : 'Relay',
      subText: fleet.name,
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: body),
    );
  }
}
