import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

import '../console/battle.dart';
import '../console/intel.dart' show clockFmt, groupDigits;
import '../console/prefs.dart' show Quality;
import '../console/services.dart';
import '../design/beat.dart';
import '../design/draw.dart';
import '../design/gauges.dart';
import '../design/panel.dart';
import '../design/symbols.dart';
import '../design/tokens.dart';
import '../models/fleet.dart' as ui;
import '../models/game_state.dart';
import '../protocol/protocol.dart' as proto;
import '../scenes/battle/field.dart';
import '../scenes/battle/field_painter.dart';
import '../state/state_mapper.dart' show amountsOf;

/// How a damaged ship and one at risk of exploding are coloured, wherever
/// a fleet is listed.
const damagedColor = C.a300;
const atRiskColor = C.ember;

TextStyle get _name => T.cond(size: 11.5, color: C.text2, spacing: 1.1);
TextStyle get _figure => T.mono(size: 11, color: C.text);
TextStyle get _small => T.mono(size: 10, color: C.text3, height: 1.35);

/// A thin bar of hull held against hull at full.
class HullBar extends StatelessWidget {
  final int hull;
  final int hullMax;
  final Color color;
  const HullBar(this.hull, this.hullMax, {super.key, this.color = C.own});

  @override
  Widget build(BuildContext context) => SizedBox(
        height: 3,
        child: DecoratedBox(
          decoration: BoxDecoration(color: C.a(color, .18)),
          child: FractionallySizedBox(
              alignment: Alignment.centerLeft, widthFactor: hullShare(hull, hullMax), child: ColoredBox(color: color)),
        ),
      );
}

/// "4 DAMAGED  2 AT RISK": how many ships of a row are not whole, and how
/// many of those may explode. Nothing for a row of whole ships.
class DamageMarks extends StatelessWidget {
  final int damaged;
  final int atRisk;
  const DamageMarks({super.key, required this.damaged, required this.atRisk});

  @override
  Widget build(BuildContext context) => Text.rich(
        TextSpan(children: [
          TextSpan(text: '${groupDigits(damaged)} DAMAGED', style: const TextStyle(color: damagedColor)),
          if (atRisk > 0) TextSpan(text: '  ${groupDigits(atRisk)} AT RISK', style: const TextStyle(color: atRiskColor)),
        ]),
        style: _small,
      );
}

/// A fleet, a hostile group or the ships of a dock, plain and by class: for
/// each class its count and its hull against full hull, and under a class
/// with damage how many are damaged and how many are at risk. Every figure
/// is a row of the state's.
class ShipRows extends StatelessWidget {
  final List<proto.ShipGroupState> ships;
  final Color color;

  /// A name for the rows' keys, where a screen lists more than one force.
  final String tag;
  const ShipRows(this.ships, {super.key, this.color = C.own, this.tag = 'ships'});

  @override
  Widget build(BuildContext context) {
    final risk = ships.any((g) => g.atRisk > 0);
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
      for (final g in ships)
        Padding(
          key: Key('$tag-${g.shipClass.wire}'),
          padding: const EdgeInsets.only(top: 3),
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
            Row(crossAxisAlignment: CrossAxisAlignment.baseline, textBaseline: TextBaseline.alphabetic, children: [
              Expanded(child: Text(g.shipClass.label.toUpperCase(), overflow: TextOverflow.ellipsis, style: _name)),
              const SizedBox(width: 6),
              Text(shortCount(g.count), style: _figure),
              SizedBox(width: 98, child: Text(hullOf(g.hull, g.hullMax), textAlign: TextAlign.right, style: _small)),
            ]),
            if (g.damaged > 0)
              Row(children: [
                Expanded(child: HullBar(g.hull, g.hullMax, color: color)),
                const SizedBox(width: 8),
                DamageMarks(damaged: g.damaged, atRisk: g.atRisk),
              ]),
          ]),
        ),
      if (risk) Padding(padding: const EdgeInsets.only(top: 4), child: Text(atRiskRule, style: _small)),
    ]);
  }
}

/// One mark a class with its count beside it, for a card that has a line
/// for a whole fleet.
class ClassMarks extends StatelessWidget {
  final List<proto.ShipGroupState> ships;
  final bool hostile;
  const ClassMarks(this.ships, {super.key, this.hostile = false});

  @override
  Widget build(BuildContext context) {
    final color = hostile ? C.ember : C.own;
    return Wrap(spacing: 10, runSpacing: 4, crossAxisAlignment: WrapCrossAlignment.center, children: [
      for (final g in ships)
        Row(mainAxisSize: MainAxisSize.min, children: [
          SymIcon(
              (c, s) => Draw.symbol(c, hostile ? Sym.hostile(g.shipClass) : Sym.own(g.shipClass), s.width / 2, s.height / 2, 9,
                  color: g.atRisk > 0 ? atRiskColor : color, lw: 1.3, bloom: false),
              size: 20),
          const SizedBox(width: 3),
          Text(shortCount(g.count), style: T.mono(size: 10.5, color: g.damaged > 0 ? damagedColor : C.text2)),
        ]),
    ]);
  }
}

/// What a fleet fights with, whether a refit waits for it, and what each
/// dock would do about its damage: the state's `refit`, `refit_due` and
/// `repair`, worded.
class FleetCondition extends StatelessWidget {
  final proto.Refit refit;
  final bool refitDue;
  final proto.RepairQuote? repair;
  const FleetCondition({super.key, required this.refit, required this.refitDue, required this.repair});

  Widget _dock(String name, proto.RepairQuote q, proto.DockRepair dock, Key key) => Padding(
        padding: const EdgeInsets.only(top: 3),
        child: Text.rich(
          key: key,
          TextSpan(children: [
            TextSpan(text: '$name ', style: TextStyle(color: dock.refusal == null ? C.a300 : C.threat(6), fontWeight: FontWeight.w600)),
            TextSpan(text: dockRepairLine(q, dock).toUpperCase()),
          ]),
          style: _small,
        ),
      );

  @override
  Widget build(BuildContext context) => Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
        Kv('Fights with', key: const Key('fleet-refit'), v: refitMark(refit), vc: refitDue ? damagedColor : null),
        if (refitDue) Padding(padding: const EdgeInsets.only(top: 3), child: Text(refitWaits, key: const Key('fleet-refit-due'), style: _small.copyWith(color: damagedColor))),
        if (repair case final q?) ...[
          Kv('Damage', key: const Key('fleet-damage'), v: damageLine(q).toUpperCase(), vc: damagedColor, size: 10),
          _dock('HOME DOCK', q, q.home, const Key('fleet-repair-home')),
          if (q.outpost case final post?) _dock('OUTPOST', q, post, const Key('fleet-repair-outpost')),
        ],
      ]);
}

Color _outcomeColor(proto.BattleOutcome o) => switch (o) {
      proto.BattleOutcome.hostilesDestroyed => C.own,
      proto.BattleOutcome.fleetsDestroyed || proto.BattleOutcome.bothDestroyed => C.ember,
      proto.BattleOutcome.withdrawn || proto.BattleOutcome.standoff || proto.BattleOutcome.fled || proto.BattleOutcome.outlasted => C.a300,
    };

Color _endingColor(RaidEnding e) => switch (e) {
      RaidEnding.destroyed => C.own,
      RaidEnding.outlasted => C.a300,
      RaidEnding.lost || RaidEnding.unopposed => C.ember,
    };

/// The colour of a raid forecast's label, from the worst to the best.
Color raidLabelColor(proto.RaidLabel l) => switch (l) {
      proto.RaidLabel.lost => C.ratioDeadly,
      proto.RaidLabel.atRisk => C.ratioRisky,
      proto.RaidLabel.held => C.ratioEven,
      proto.RaidLabel.repelled => C.ratioFav,
    };

/// How the fights the server played ended: one bar cut at the forecast's
/// three shares, with each share named under it.
class RaidShares extends StatelessWidget {
  final proto.RaidOdds odds;

  /// A name for the keys, where a screen has more than one forecast.
  final String tag;
  const RaidShares(this.odds, {super.key, required this.tag});

  @override
  Widget build(BuildContext context) {
    final parts = [('DESTROYED', odds.destroyed, C.ratioFav), ('OUTLASTED', odds.outlasted, C.ratioEven), ('LOST', odds.lost, C.ratioDeadly)];
    return Column(key: Key('$tag-shares'), crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
      SizedBox(
        height: 9,
        child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          for (final (_, share, color) in parts)
            if ((share * 1000).round() > 0) Expanded(flex: (share * 1000).round(), child: ColoredBox(color: C.a(color, .8))),
        ]),
      ),
      const SizedBox(height: 4),
      Wrap(spacing: 12, runSpacing: 2, children: [
        for (final (name, share, color) in parts)
          Row(mainAxisSize: MainAxisSize.min, children: [
            Container(width: 7, height: 7, color: color),
            const SizedBox(width: 5),
            Text('$name ${sharePct(share)}', style: T.mono(size: 10.5, color: share > 0 ? C.text : C.text3)),
          ]),
      ]),
    ]);
  }
}

/// A raid's forecast as every screen prints it, all of it the state's: where
/// the raid stands (being fought, on its way, or none announced and one of
/// typical size), the label with Rust's sentence for it, what it flies and
/// its power, how the fights the server played ended, and what each side
/// keeps. [tick] turns the arrival into a time from now.
class RaidForecastReadout extends StatelessWidget {
  final proto.RaidForecast raid;
  final int tick;

  /// A name for the keys: `raid` at home, `outpost-raid` at the outpost.
  final String tag;

  /// Print what the three shares are shares of.
  final bool rule;

  /// Print where the raid stands. Off for a survey's forecast, which is of
  /// no raid in particular.
  final bool headline;
  const RaidForecastReadout(this.raid, {super.key, required this.tick, required this.tag, this.rule = true, this.headline = true});

  @override
  Widget build(BuildContext context) {
    final odds = raid.odds;
    final color = raidLabelColor(odds.label);
    final (stands, hot, note) = switch ((raid.roundsLeft, raid.arrivalTick)) {
      (final left?, _) => ('RAID BEING FOUGHT', true, roundsLeftOf(left).toUpperCase()),
      (null, final at?) => ('RAID IN ${clockFmt(at > tick ? at - tick : 0)}', true, 'AGAINST WHAT STANDS HERE NOW'),
      _ => ('NO RAID ANNOUNCED', false, 'ONE OF TYPICAL SIZE, WERE IT ANNOUNCED NOW'),
    };
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
      if (headline) ...[
        Wrap(crossAxisAlignment: WrapCrossAlignment.end, spacing: 10, runSpacing: 2, children: [
          Text(stands, key: Key('$tag-when'), style: T.cond(size: 22, color: hot ? C.ember : C.text2, weight: FontWeight.w700, spacing: 2, height: 1.1)),
          Text(note, key: Key('$tag-note'), style: T.cond(size: 11, color: C.text3, spacing: 1.3)),
        ]),
        const SizedBox(height: 7),
      ],
      Wrap(crossAxisAlignment: WrapCrossAlignment.center, spacing: 10, runSpacing: 2, children: [
        Text(odds.label.label, key: Key('$tag-label'), style: T.cond(size: 17, color: color, weight: FontWeight.w700, spacing: 2.2)),
        Text(odds.label.outcome.toUpperCase(), key: Key('$tag-sentence'), style: _small.copyWith(color: C.text2)),
      ]),
      const SizedBox(height: 6),
      Text.rich(
        key: Key('$tag-ships'),
        TextSpan(children: [
          const TextSpan(text: 'IT FLIES  ', style: TextStyle(color: C.text3)),
          TextSpan(text: shipsList(raid.ships).toUpperCase()),
          const TextSpan(text: '   POWER ', style: TextStyle(color: C.text3)),
          TextSpan(text: groupDigits(raid.power.round())),
        ]),
        style: T.mono(size: 11, color: C.text, height: 1.35),
      ),
      const SizedBox(height: 7),
      RaidShares(odds, tag: tag),
      const SizedBox(height: 5),
      Text('${raidKept(odds).toUpperCase()}, BEFORE ANY DEFENCE IS REBUILT.', key: Key('$tag-kept'), style: _small.copyWith(color: C.text2)),
      if (rule) const WhyText(raidSharesRule),
    ]);
  }
}

/// Defences and Solar Satellites, or an outpost's platform: so many of each
/// kind, plain, a row a kind.
class UnitRows extends StatelessWidget {
  final List<proto.UnitCount> units;
  final String tag;
  const UnitRows(this.units, {super.key, required this.tag});

  @override
  Widget build(BuildContext context) => Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
        for (final u in units)
          Padding(
            key: Key('$tag-${u.unit.ship?.wire ?? u.unit.defence!.wire}'),
            padding: const EdgeInsets.only(top: 3),
            child: Row(crossAxisAlignment: CrossAxisAlignment.baseline, textBaseline: TextBaseline.alphabetic, children: [
              Expanded(child: Text(u.unit.label.toUpperCase(), overflow: TextOverflow.ellipsis, style: _name)),
              const SizedBox(width: 6),
              Text(shortCount(u.count), style: _figure),
            ]),
          ),
      ]);
}

/// A raid that has ended, as its summary: how it ended and after how many
/// rounds, then what the raiders flew and lost, what the defender lost,
/// what was rebuilt, what was taken, the wreckage and a breach. Every
/// figure is the `RaidResolved` event's.
class RaidSummary extends StatelessWidget {
  final RaidReport report;
  final int tick;
  final String tag;
  const RaidSummary(this.report, {super.key, required this.tick, required this.tag});

  @override
  Widget build(BuildContext context) {
    final e = report.raid;
    final ending = RaidEnding.of(e);
    final ago = tick - report.tick;
    return Column(key: Key(tag), crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
      Wrap(crossAxisAlignment: WrapCrossAlignment.end, spacing: 10, runSpacing: 2, children: [
        Text(ending.title, key: Key('$tag-ending'), style: T.cond(size: 15, color: _endingColor(ending), weight: FontWeight.w700, spacing: 2)),
        Text(
            '${ending == RaidEnding.unopposed ? '' : 'AFTER ${roundsOf(e.rounds).toUpperCase()}, '}${ago <= 0 ? 'JUST NOW' : '${clockFmt(ago)} AGO'}',
            style: T.cond(size: 10.5, color: C.text3, spacing: 1.3)),
      ]),
      Text('${ending.sentence}.'.toUpperCase(), style: _small.copyWith(color: C.text2)),
      const SizedBox(height: 4),
      for (final line in raidSummary(e, tick: tick)) Text('${line.toUpperCase()}.', style: _small.copyWith(color: C.text2)),
    ]);
  }
}

/// "PATROL - 3 OF 5 SHIPS": a hostile group's behaviour and its ships
/// against what it musters when whole.
String groupHeading(proto.NpcFleetInfo g) => '${g.behavior.wire} - ${musterOf(g)}'.toUpperCase();

/// A hostile group as every screen lists it under its heading: its ships by
/// class, when it will be whole again if it is not, how far it follows a
/// fleet that leaves it, and what it is out of its own sector for. Every
/// figure is the state's; [tick] turns the tick it is whole at into a time
/// from now.
class GroupBrief extends StatelessWidget {
  final proto.NpcFleetInfo group;
  final int tick;

  /// The sector it was seen in is in sensor range now. When it is not, what
  /// is said of the group is as it was last seen.
  final bool live;

  /// The player's fleet ids, so that a group after one of them says so.
  final Iterable<int> own;

  /// The fleet the screen is read for: its own jump time is printed beside
  /// the group's. Which of the two is caught is the state's to say, in a
  /// fleet's `flight`.
  final ui.FleetState? beside;

  /// A name for the rows' keys, where a screen lists more than one group.
  final String tag;
  const GroupBrief(this.group, {super.key, required this.tick, required this.live, this.own = const [], this.beside, required this.tag});

  String _mending(int wholeAt) {
    if (wholeAt > tick) return 'WHOLE AGAIN IN ${clockFmt(wholeAt - tick)}${live ? '' : ', AS LAST SEEN'}';
    return live ? 'WHOLE AGAIN ANY MOMENT' : 'WHOLE AGAIN BY NOW, IF NOTHING DISTURBED IT';
  }

  @override
  Widget build(BuildContext context) {
    final g = group;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
      ShipRows(g.ships, color: C.ember, tag: tag),
      if (g.wholeAt case final at?)
        Padding(
          padding: const EdgeInsets.only(top: 4),
          child: Text.rich(
            key: Key('$tag-whole'),
            TextSpan(children: [
              TextSpan(text: _mending(at), style: const TextStyle(color: damagedColor, fontWeight: FontWeight.w600)),
              const TextSpan(text: '  IT MENDS AT REST IN ITS OWN SECTOR; A FIGHT OR A PURSUIT STOPS THE CLOCK.'),
            ]),
            style: _small,
          ),
        ),
      Padding(
          padding: const EdgeInsets.only(top: 3),
          child: Text(
              '${chaseHabit(g)}${switch (beside) { final f? when f.jumpTicks > 0 => ', ${f.name} ${ticksOf(f.jumpTicks)}', _ => '' }}.'.toUpperCase(),
              key: Key('$tag-chase'),
              style: _small)),
      if (g.errand case final errand?)
        Padding(
          padding: const EdgeInsets.only(top: 3),
          child: Text('${errandLine(errand, tick, mine: own.contains(errand.quarry)).toUpperCase()}.',
              key: Key('$tag-errand'), style: _small.copyWith(color: errand.quarry == null ? C.text2 : atRiskColor)),
        ),
    ]);
  }
}

/// How a fight ended, as a heading, its colour and a sentence: in a raid's
/// own words when the server has said what the raid came to.
(String, Color, String) battleVerdict(Battle b, proto.CombatEndedEvent e) => switch (b.resolved) {
      final how? => (RaidEnding.of(how).title, _endingColor(RaidEnding.of(how)), RaidEnding.of(how).sentence),
      null => (outcomeTitle(e.outcome), _outcomeColor(e.outcome), outcomeSentence(e.outcome)),
    };

/// The names of a round's two sides: who defends a place raided and the
/// raiders, or the fleets and the hostiles.
(String, String) sideNames({required bool raid}) => raid ? ('Defenders', 'Raiders') : ('Fleets', 'Hostiles');

/// A fight in a few lines, beside the scene that draws it: the round it
/// stands at, what stands on each side and what the round took, the volley
/// that did the most on each side, and the fight's losses so far; once it
/// has ended, how it ended, after how many rounds and at what cost. Every
/// figure is the round's or the state's. The rounds in full, each side by
/// class and every volley, are the battle view's, which its button opens.
class BattlePanel extends StatelessWidget {
  final Battle battle;

  /// Puts the report of a fight that has ended away.
  final VoidCallback? onDismiss;

  /// The key that opens the battle view where the panel stands, if one does.
  final String? keyHint;
  const BattlePanel({super.key, required this.battle, this.onDismiss, this.keyHint});

  Battle get b => battle;

  @override
  Widget build(BuildContext context) {
    final round = b.round;
    final ended = b.ended;
    final raid = b.isRaid;
    final standing = ended == null ? b.raid : null;
    final at = '${b.sector.q}, ${b.sector.r}';
    return ConsolePanel(
      key: const Key('battle-panel'),
      title: raid ? 'Raid' : 'Battle',
      titleColor: C.ember,
      mark: C.ember,
      subText: ended != null
          ? 'ENDED - ${roundsOf(ended.rounds).toUpperCase()}'
          : raid
              ? raidRoundOf(b.rounds)
              : 'ROUND ${b.rounds} - $at',
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
        if (ended != null) ..._end(ended),
        if (standing != null) ...[
          Text.rich(
            key: const Key('battle-raid'),
            TextSpan(children: [
              TextSpan(text: '${roundsLeftOf(standing.roundsLeft).toUpperCase()}.  AS IT STANDS  ', style: const TextStyle(color: C.text3)),
              TextSpan(text: standing.odds.label.label, style: TextStyle(color: raidLabelColor(standing.odds.label), fontWeight: FontWeight.w600)),
              TextSpan(text: '  ${standing.odds.label.outcome.toUpperCase()}'),
            ]),
            style: _small.copyWith(color: C.text2),
          ),
          const SizedBox(height: 6),
        ],
        if (ended == null && round == null)
          Text(raid ? 'THE RAID STANDS HERE. THE FIRST ROUND IS FOUGHT ON THE NEXT TICK.' : 'THE FIRST ROUND IS NOT FOUGHT YET.', style: _small),
        if (round != null && ended == null) ..._round(round),
        if (ended == null) ...[
          const SizedBox(height: 9),
          const Lbl('Lost so far'),
          ..._losses(b.losses, key: 'battle-losses'),
        ],
        Padding(
            padding: const EdgeInsets.only(top: 8),
            child: Wrap(spacing: 6, runSpacing: 6, children: [
              ConsoleButton('Battle view',
                  key: const ValueKey('battle-open'),
                  small: true,
                  keyHint: keyHint,
                  semanticsLabel: 'Open the battle view',
                  onPressed: () => Console.of(context).openBattle(b.id)),
              if (ended != null && onDismiss != null)
                ConsoleButton('Dismiss', key: const ValueKey('battle-dismiss'), small: true, onPressed: onDismiss),
            ])),
      ]),
    );
  }

  List<Widget> _end(proto.CombatEndedEvent e) {
    final (title, color, sentence) = battleVerdict(b, e);
    final how = b.resolved;
    Widget line(String text, String key, {Color? color}) => Padding(
        padding: const EdgeInsets.only(top: 5),
        child: Text('${text.toUpperCase()}.', key: Key(key), style: _small.copyWith(color: color ?? C.text2)));
    return [
        Text(title, key: const Key('battle-outcome'), style: T.cond(size: 17, color: color, weight: FontWeight.w700, spacing: 2.4)),
        Text('$sentence, after ${roundsOf(e.rounds)} at ${e.sector.q}, ${e.sector.r}.'.toUpperCase(), style: _small),
        const SizedBox(height: 7),
        const Lbl('Losses'),
        ..._losses(e.losses, key: 'battle-end-losses', none: e.raid ? 'NOTHING WAS LOST ON EITHER SIDE.' : 'NO SHIP WAS LOST ON EITHER SIDE.'),
        if (how != null) ...[
          if (raidRebuilt(how) case final rebuilt?) line(rebuilt, 'battle-rebuilt', color: C.own),
          if (raidTaken(how) case final taken?) line(taken, 'battle-taken', color: atRiskColor),
          if (raidBreach(how) case final breach?) line(breach, 'battle-breach', color: atRiskColor),
        ],
        if (e.salvage case final pile?)
          Padding(padding: const EdgeInsets.only(top: 5), child: Text('WRECKAGE LEFT: ${amountsOf(pile).toUpperCase()}', key: const Key('battle-wreckage'), style: _small.copyWith(color: C.gold))),
      ];
  }

  List<Widget> _losses(List<proto.BattleLosses> losses, {required String key, String none = 'NOTHING YET.'}) => [
        if (losses.every((l) => l.lost.isEmpty))
          Text(none, key: Key(key), style: _small)
        else
          Column(key: Key(key), crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
            for (final l in losses)
              if (l.lost.isNotEmpty)
                Text(lossesLine(l).toUpperCase(), style: _small.copyWith(color: l.owner == null ? C.text2 : l.mine ? C.own : C.text3)),
          ]),
      ];

  /// The round in two lines a side: what stands and what the round took,
  /// and the volley that did the most. A fleet that is leaving is named.
  List<Widget> _round(proto.BattleRoundEvent r) {
    final (ours, theirs) = sideNames(raid: r.raid);
    return [
      _side(ours, r.fleets, lost: r.hostiles.destroyed, color: C.own, tag: 'battle-fleets'),
      const SizedBox(height: 7),
      _side(theirs, r.hostiles, lost: r.fleets.destroyed, color: C.ember, tag: 'battle-hostiles'),
    ];
  }

  Widget _side(String title, proto.BattleSide side, {required int lost, required Color color, required String tag}) {
    final heaviest = volleysByWeight(side.volleys).firstOrNull;
    return Column(key: Key(tag), crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
      Row(children: [
        Expanded(child: Lbl(title, color: color)),
        Text.rich(
          TextSpan(children: [
            TextSpan(text: '${shortCount(side.standing)} STANDING'),
            if (lost > 0) TextSpan(text: '  -${shortCount(lost)} THIS ROUND', style: const TextStyle(color: atRiskColor)),
          ]),
          style: _small.copyWith(color: C.text2),
        ),
      ]),
      for (final g in side.groups)
        if (g.fleeing)
          Text.rich(
            key: Key('$tag-group-${g.fleetId}'),
            TextSpan(children: [
              TextSpan(text: groupName(g).toUpperCase()),
              const TextSpan(text: '  LEAVING: FIRED ON, NOT FIRING', style: TextStyle(color: damagedColor)),
            ]),
            overflow: TextOverflow.ellipsis,
            style: _small,
          ),
      Text(
          side.shots == 0
              ? 'FIRED NOTHING'
              : 'FIRED ${shortCount(side.shots)} SHOT${side.shots == 1 ? '' : 'S'}, DESTROYED ${shortCount(side.destroyed)}',
          key: Key('$tag-fired'),
          style: _small),
      if (heaviest != null)
        Text(
            '${heaviest.from.label} > ${heaviest.at.label}'.toUpperCase() +
                (heaviest.destroyed > 0 ? '  ${shortCount(heaviest.destroyed)} DESTROYED' : '  HULL ${shortCount(heaviest.hullDamage)}'),
            key: Key('$tag-heaviest'),
            overflow: TextOverflow.ellipsis,
            style: T.cond(size: 10.5, color: C.text2, spacing: .9)),
    ]);
  }
}

/// A fight in one line, for the layout that keeps its panels in a sheet:
/// the round, what stands on each side and what the round took, or how the
/// fight ended.
class BattleStrip extends StatelessWidget {
  final Battle battle;
  final VoidCallback onTap;
  const BattleStrip({super.key, required this.battle, required this.onTap});

  @override
  Widget build(BuildContext context) {
    final r = battle.round;
    final ended = battle.ended;
    final Widget line;
    if (ended != null) {
      final (title, color) = switch (battle.resolved) {
        final how? => (RaidEnding.of(how).title, _endingColor(RaidEnding.of(how))),
        null => (outcomeTitle(ended.outcome), _outcomeColor(ended.outcome)),
      };
      line = Text('$title - ${roundsOf(ended.rounds).toUpperCase()}',
          overflow: TextOverflow.ellipsis, style: T.cond(size: 11.5, color: color, spacing: 1.2));
    } else if (r == null) {
      line = Text(battle.isRaid ? 'THE RAID HAS ARRIVED' : 'BATTLE JOINED', style: T.cond(size: 11.5, color: C.ember, spacing: 1.2));
    } else {
      final (ours, theirs) = r.raid ? ('DEFENDERS', 'RAIDERS') : ('FLEETS', 'HOSTILES');
      line = Text.rich(
        TextSpan(children: [
          TextSpan(text: '${r.raid ? raidRoundOf(r.round) : 'ROUND ${r.round}'}  ', style: const TextStyle(color: C.ember)),
          TextSpan(text: '$ours ${shortCount(r.fleets.standing)}', style: const TextStyle(color: C.own)),
          if (r.hostiles.destroyed > 0) TextSpan(text: ' -${shortCount(r.hostiles.destroyed)}', style: const TextStyle(color: atRiskColor)),
          const TextSpan(text: '  V  ', style: TextStyle(color: C.text3)),
          TextSpan(text: '$theirs ${shortCount(r.hostiles.standing)}', style: const TextStyle(color: C.text)),
          if (r.fleets.destroyed > 0) TextSpan(text: ' -${shortCount(r.fleets.destroyed)}', style: const TextStyle(color: atRiskColor)),
        ]),
        overflow: TextOverflow.ellipsis,
        style: T.mono(size: 11, color: C.text),
      );
    }
    return GestureDetector(
      key: const ValueKey('battle-strip'),
      behavior: HitTestBehavior.opaque,
      onTap: onTap,
      child: ConsolePanel(
        flat: true,
        border: C.ember,
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
        child: line,
      ),
    );
  }
}

/// The picture of a fight on a ground of its own, where no windshield
/// stands: both sides as marks, a mark a kind of unit, and a round's fire
/// between them. It draws the round [index] of the rounds held of
/// [battle]: each time that changes the round is played once, from what
/// the round before it left standing where that is held. Before a first
/// round it draws [idle], what the state says will stand in the fight.
class BattleScope extends StatefulWidget {
  final Battle battle;
  final int? index;
  final Sides idle;

  /// The fight is under way and [index] is its newest round: units a
  /// round wipes out leave the ground. Otherwise they stay, dimmed.
  final bool live;

  /// Raised to play the round again.
  final int replay;
  final bool still;
  final bool bloom;
  const BattleScope(
      {super.key, required this.battle, required this.index, required this.idle, required this.live, this.replay = 0, this.still = false, this.bloom = true});

  @override
  State<BattleScope> createState() => BattleScopeState();
}

class BattleScopeState extends State<BattleScope> with SingleTickerProviderStateMixin {
  final BattleField field = BattleField();
  final ValueNotifier<int> _frame = ValueNotifier(0);
  late final Ticker _ticker;
  Duration _last = Duration.zero;
  Mark? _hover;

  @override
  void initState() {
    super.initState();
    _ticker = createTicker((d) {
      final dt = math.min(.05, (d - _last).inMicroseconds / 1e6);
      _last = d;
      field.tick(dt, d.inMicroseconds / 1e6);
      _frame.value++;
    })
      ..start();
    _show(from: null);
  }

  @override
  void didUpdateWidget(BattleScope oldWidget) {
    super.didUpdateWidget(oldWidget);
    final same = oldWidget.battle.id == widget.battle.id;
    if (same && oldWidget.index == widget.index && oldWidget.replay == widget.replay) {
      if (widget.index == null) field.setSides(widget.idle);
      return;
    }
    _show(from: same && oldWidget.replay == widget.replay ? oldWidget.index : null);
  }

  /// Draw the round the widget names. Coming from the round before it the
  /// marks stand as they are and the round is played over them; coming
  /// from anywhere else they are first stood as the round before left
  /// them, when that round is held.
  void _show({required int? from}) {
    final held = widget.battle.held;
    final index = widget.index;
    field
      ..reduced = widget.still
      ..showFallen = !widget.live;
    if (index == null || index >= held.length) {
      field.setSides(widget.idle, snap: true);
      return;
    }
    final round = held[index];
    if (from == null || from != index - 1) {
      final before = index > 0 && held[index - 1].round == round.round - 1 ? held[index - 1] : round;
      field
        ..clear()
        ..setSides(Sides.ofRound(before), snap: true);
    }
    field
      ..play(round)
      ..setSides(Sides.ofRound(round));
  }

  @override
  void dispose() {
    _ticker.dispose();
    _frame.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final (ours, theirs) = sideNames(raid: widget.battle.isRaid);
    return LayoutBuilder(builder: (context, box) {
      final centre = Offset(box.maxWidth / 2, box.maxHeight / 2);
      void pick(Offset at) {
        final found = field.markAt(at - centre);
        if (found != _hover) setState(() => _hover = found);
      }

      return MouseRegion(
        onHover: (e) => pick(e.localPosition),
        onExit: (_) => setState(() => _hover = null),
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTapUp: (d) => pick(d.localPosition),
          child: RepaintBoundary(
            child: CustomPaint(
              key: const Key('battle-scope'),
              painter: ScopePainter(field, _frame, ours: ours.toUpperCase(), theirs: theirs.toUpperCase(), bloom: widget.bloom, hover: _hover),
              size: Size.infinite,
            ),
          ),
        ),
      );
    });
  }
}

/// The ground of a [BattleScope]: a frame, the line between the two sides
/// with each side named at its end, and the field drawn on it.
class ScopePainter extends CustomPainter {
  final BattleField field;
  final String ours, theirs;
  final bool bloom;
  final Mark? hover;
  ScopePainter(this.field, Listenable repaint, {required this.ours, required this.theirs, required this.bloom, this.hover}) : super(repaint: repaint);

  @override
  void paint(Canvas c, Size size) {
    final centre = Offset(size.width / 2, size.height / 2);
    double half(double from) => math.max(30.0, size.width / 2 - 12);
    field.ground(
        ours: Ground(near: 16, far: size.height / 2 - 6, halfWidth: half),
        theirs: Ground(near: 16, far: size.height / 2 - 6, halfWidth: half),
        scale: (math.min(size.width, size.height) / 330).clamp(.8, 1.9));
    c.save();
    c.clipRect(Offset.zero & size);
    c.drawRect(Offset.zero & size, Paint()..color = const Color(0xFF07050A));
    final hair = Draw.line(C.a500, 1, .1);
    for (var x = centre.dx % 40; x < size.width; x += 40) {
      c.drawLine(Offset(x, 0), Offset(x, size.height), hair);
    }
    for (var y = centre.dy % 40; y < size.height; y += 40) {
      c.drawLine(Offset(0, y), Offset(size.width, y), hair);
    }
    Draw.dashedLine(c, Offset(8, centre.dy), Offset(size.width - 8, centre.dy), Draw.line(C.a500, 1, .4), 5, 5);
    Draw.text(c, theirs, 8, centre.dy - 9, T.cond(size: 9, color: C.ember.withValues(alpha: .85), spacing: 1.6));
    Draw.text(c, ours, 8, centre.dy + 9, T.cond(size: 9, color: C.own.withValues(alpha: .85), spacing: 1.6));
    if (field.shake > 0) c.translate((field.rng.nextDouble() - .5) * field.shake, (field.rng.nextDouble() - .5) * field.shake);
    FieldPainter(c, field, centre, t: field.now, bloom: bloom, hover: hover).paint();
    c.restore();
    c.drawRect((Offset.zero & size).deflate(.5), Draw.line(C.a500, 1, .3));
  }

  @override
  bool shouldRepaint(ScopePainter old) => true;
}

/// The battle view: a fight as a report, round by round. The picture of the
/// round on the left or on top; beside or under it the two sides by class
/// with what entered the round, what it destroyed and what stands after
/// it, every volley of the round as a table of who fired at whom with what
/// result, and each side's totals. Opened while the fight is fought it
/// follows the rounds as they come; a step back holds it on a round, and
/// after the fight it steps through the rounds this console was sent.
/// Nothing in it is counted here: every figure is a field of a round or of
/// the fight's end.
class BattleView extends StatefulWidget {
  final Console con;

  /// The fight shown, by [Battle.id].
  final String fight;
  final ValueChanged<String> onPick;
  final VoidCallback onClose;
  const BattleView({super.key, required this.con, required this.fight, required this.onPick, required this.onClose});

  @override
  State<BattleView> createState() => BattleViewState();
}

class BattleViewState extends State<BattleView> {
  /// The round held on, among the rounds held of the fight; null follows
  /// the newest.
  int? _at;
  int _replay = 0;

  /// Width from which the picture stands beside the tables.
  static const _beside = 900.0;

  @override
  void didUpdateWidget(BattleView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.fight != widget.fight) _at = null;
  }

  /// Step [by] rounds from the one shown; past the newest it follows again.
  void step(int by) {
    final held = widget.con.state.fight(widget.fight)?.held.length ?? 0;
    if (held == 0) return;
    final to = ((_at ?? held - 1) + by).clamp(0, held - 1);
    setState(() => _at = to == held - 1 ? null : to);
  }

  void _first() => setState(() => _at = (widget.con.state.fight(widget.fight)?.held.length ?? 0) > 1 ? 0 : null);

  void _latest() => setState(() => _at = null);

  @override
  Widget build(BuildContext context) => Positioned.fill(
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: widget.onClose,
          child: ColoredBox(
            color: const Color(0xE6050308),
            child: LayoutBuilder(builder: (context, box) {
              final narrow = box.maxWidth < _beside;
              return Padding(
                padding: EdgeInsets.all(narrow ? 6 : 22),
                child: Center(
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 1260),
                    child: GestureDetector(
                      onTap: () {},
                      child: ListenableBuilder(listenable: widget.con.game, builder: (context, _) => _panel(context, narrow: narrow)),
                    ),
                  ),
                ),
              );
            }),
          ),
        ),
      );

  Widget _panel(BuildContext context, {required bool narrow}) {
    final st = widget.con.state;
    final b = st.fight(widget.fight);
    final close = ConsoleButton('Close', key: const ValueKey('bv-close'), small: true, keyHint: narrow ? null : 'ESC', onPressed: widget.onClose);
    if (b == null) {
      return ConsolePanel(
        key: const Key('battle-view'),
        title: 'Battle view',
        trailing: close,
        padding: const EdgeInsets.all(14),
        child: Text('THIS FIGHT IS NO LONGER HELD: THE CONSOLE KEEPS THE LAST ${Battle.fightsKept} THAT ENDED WHILE IT WAS OPEN.', style: _small),
      );
    }
    final held = b.held;
    final index = held.isEmpty ? null : (_at ?? held.length - 1).clamp(0, held.length - 1);
    final round = index == null ? null : held[index];
    final following = _at == null;
    final still = reducedMotion(context) || widget.con.prefs.reduceMotion;
    final scope = BattleScope(
        battle: b,
        index: index,
        idle: Sides.at(st, b.sector),
        live: !b.over && following,
        replay: _replay,
        still: still,
        bloom: widget.con.prefs.quality != Quality.low);
    final rows = _rows(b, round, st.tick, wide: !narrow);
    final others = st.fights;
    return ConsolePanel(
      key: const Key('battle-view'),
      title: 'Battle view',
      titleColor: C.ember,
      mark: C.ember,
      subText: '${b.isRaid ? 'RAID' : 'FIGHT'} AT ${b.sector.q}, ${b.sector.r}',
      trailing: close,
      padding: EdgeInsets.fromLTRB(narrow ? 8 : 14, 8, narrow ? 8 : 14, narrow ? 8 : 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        if (others.length > 1) _fights(others, st.tick),
        _status(b),
        const SizedBox(height: 6),
        _stepper(b, held, index, following: following, narrow: narrow),
        const SizedBox(height: 8),
        Expanded(
          child: narrow
              ? Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                  SizedBox(height: (MediaQuery.sizeOf(context).height * .28).clamp(150.0, 280.0), child: scope),
                  const SizedBox(height: 6),
                  Expanded(
                      child: ListView.builder(key: const Key('bv-rows'), itemCount: rows.length, itemBuilder: (context, i) => rows[i](context))),
                ])
              : Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                  Expanded(
                      flex: 9,
                      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                        Expanded(child: scope),
                        const SizedBox(height: 5),
                        Text(marksRule, key: const Key('bv-legend'), style: _small.copyWith(fontSize: 9.5)),
                      ])),
                  const SizedBox(width: 14),
                  Expanded(
                      flex: 11,
                      child: ListView.builder(key: const Key('bv-rows'), itemCount: rows.length, itemBuilder: (context, i) => rows[i](context))),
                ]),
        ),
      ]),
    );
  }

  /// The fights the console holds, to look at another.
  Widget _fights(List<Battle> fights, int tick) => Padding(
        padding: const EdgeInsets.only(bottom: 7),
        child: SizedBox(
          height: 24,
          child: ListView(key: const Key('bv-fights'), scrollDirection: Axis.horizontal, children: [
            for (final other in fights)
              Padding(
                padding: const EdgeInsets.only(right: 6),
                child: ConsoleButton(
                    '${other.sector.q}, ${other.sector.r}  ${other.over ? '${clockFmt(math.max(0, tick - other.endedTick))} AGO' : 'NOW'}',
                    key: ValueKey('bv-fight-${other.id}'),
                    small: true,
                    active: other.id == widget.fight,
                    semanticsLabel: 'Fight at ${other.sector.q}, ${other.sector.r}',
                    onPressed: () => widget.onPick(other.id)),
              ),
          ]),
        ),
      );

  /// Where the fight stands: under way, with a raid's rounds left and how
  /// it stands, or how it ended.
  Widget _status(Battle b) {
    final ended = b.ended;
    if (ended != null) {
      final (title, color, sentence) = battleVerdict(b, ended);
      return Wrap(crossAxisAlignment: WrapCrossAlignment.center, spacing: 10, runSpacing: 2, children: [
        Text(title, key: const Key('bv-outcome'), style: T.cond(size: 16, color: color, weight: FontWeight.w700, spacing: 2.2)),
        Text('${sentence.toUpperCase()}, AFTER ${roundsOf(ended.rounds).toUpperCase()}.', style: _small.copyWith(color: C.text2)),
      ]);
    }
    final raid = b.raid;
    return Wrap(crossAxisAlignment: WrapCrossAlignment.center, spacing: 10, runSpacing: 2, children: [
      const Lamp('Being fought', color: C.ember, blink: true),
      Text(b.isRaid ? raidRoundOf(b.rounds) : 'ROUND ${b.rounds}',
          key: const Key('bv-standing'), style: T.cond(size: 16, color: C.a100, weight: FontWeight.w700, spacing: 2.2)),
      if (raid != null)
        Text.rich(
          key: const Key('bv-raid'),
          TextSpan(children: [
            TextSpan(text: '${roundsLeftOf(raid.roundsLeft).toUpperCase()}.  AS IT STANDS  '),
            TextSpan(text: raid.odds.label.label, style: TextStyle(color: raidLabelColor(raid.odds.label), fontWeight: FontWeight.w600)),
          ]),
          style: _small.copyWith(color: C.text2),
        ),
    ]);
  }

  /// The round shown among those held, the steps back and forth, and what
  /// the console does not hold of the fight.
  Widget _stepper(Battle b, List<proto.BattleRoundEvent> held, int? index, {required bool following, required bool narrow}) {
    if (index == null) {
      return Text(
          b.over
              ? 'NO ROUND OF THIS FIGHT REACHED THIS CONSOLE.'
              : b.isRaid
                  ? 'THE RAID STANDS HERE. THE FIRST ROUND IS FOUGHT ON THE NEXT TICK.'
                  : 'THE FIRST ROUND IS NOT FOUGHT YET.',
          key: const Key('bv-round'),
          style: _small.copyWith(color: C.text2));
    }
    final round = held[index];
    final first = held.first.round;
    final missed = first > 1
        ? 'ROUND${first == 2 ? ' 1 WAS' : 'S 1 TO ${first - 1} WERE'} FOUGHT BEFORE THIS CONSOLE HAD THE FIGHT. THE SERVER KEEPS A FIGHT\'S LAST ROUND AND NOTHING OF ONE '
            'THAT IS OVER: THE ROUNDS HERE ARE THE ONES THIS PAGE RECEIVED.'
        : null;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
      Wrap(crossAxisAlignment: WrapCrossAlignment.center, spacing: 6, runSpacing: 6, children: [
        ConsoleButton('First', key: const ValueKey('bv-first'), small: true, semanticsLabel: 'First round held', onPressed: index > 0 ? _first : null, reason: 'This is the first round held.'),
        ConsoleButton('Back',
            key: const ValueKey('bv-prev'),
            small: true,
            keyHint: narrow ? null : '<',
            semanticsLabel: 'Round before',
            onPressed: index > 0 ? () => step(-1) : null,
            reason: 'This is the first round held.'),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 4),
          child: Text('ROUND ${round.round}', key: const Key('bv-round'), style: T.cond(size: 15, color: C.a100, weight: FontWeight.w700, spacing: 2)),
        ),
        ConsoleButton('Next',
            key: const ValueKey('bv-next'),
            small: true,
            keyHint: narrow ? null : '>',
            semanticsLabel: 'Round after',
            onPressed: index < held.length - 1 ? () => step(1) : null,
            reason: 'This is the newest round.'),
        ConsoleButton(b.over ? 'Last' : 'Follow',
            key: const ValueKey('bv-latest'),
            small: true,
            active: following,
            semanticsLabel: b.over ? 'Last round' : 'Follow the fight',
            onPressed: following ? null : _latest,
            reason: b.over ? 'This is the last round.' : 'The view follows the fight.'),
        ConsoleButton('Replay', key: const ValueKey('bv-replay'), small: true, semanticsLabel: 'Play the round again', onPressed: () => setState(() => _replay++)),
        Text(
            '${index + 1} OF ${roundsOf(held.length).toUpperCase()} HELD${b.over ? '' : following ? ', FOLLOWING' : ', HELD ON THIS ROUND'}',
            key: const Key('bv-held'),
            style: _small.copyWith(color: C.text2)),
      ]),
      if (missed != null) Padding(padding: const EdgeInsets.only(top: 4), child: Text(missed, key: const Key('bv-missed'), style: _small)),
    ]);
  }

  /// The report of [round] as rows, each built when it scrolls into view:
  /// the two sides by class, each side's fire with its totals, and what the
  /// fight has cost so far or cost in the end.
  List<WidgetBuilder> _rows(Battle b, proto.BattleRoundEvent? round, int tick, {required bool wide}) {
    final rows = <WidgetBuilder>[];
    void gap(double h) => rows.add((_) => SizedBox(height: h));
    if (round != null) {
      final (ours, theirs) = sideNames(raid: round.raid);
      _standing(rows, ours, round.fleets, lost: round.hostiles.destroyed, color: C.own, tag: 'bv-fleets', wide: wide);
      gap(10);
      _standing(rows, theirs, round.hostiles, lost: round.fleets.destroyed, color: C.ember, tag: 'bv-hostiles', wide: wide);
      gap(12);
      _fire(rows, '$ours fired', theirs, round.fleets, color: C.own, tag: 'bv-fire-fleets', wide: wide);
      gap(10);
      _fire(rows, '$theirs fired', ours, round.hostiles, color: C.ember, tag: 'bv-fire-hostiles', wide: wide);
      gap(12);
    }
    final ended = b.ended;
    rows.add((_) => Lbl(ended == null ? 'Lost so far' : 'Lost in the fight'));
    final losses = [for (final l in b.losses) if (l.lost.isNotEmpty) l];
    if (losses.isEmpty) {
      rows.add((_) => Text(ended == null ? 'NOTHING YET.' : 'NOTHING WAS LOST ON EITHER SIDE.', key: const Key('bv-losses'), style: _small));
    }
    for (final (i, l) in losses.indexed) {
      rows.add((_) => Text(lossesLine(l).toUpperCase(),
          key: Key(i == 0 ? 'bv-losses' : 'bv-losses-$i'), style: _small.copyWith(color: l.owner == null ? C.text2 : l.mine ? C.own : C.text3)));
    }
    final how = b.resolved;
    if (how != null) {
      for (final line in [?raidRebuilt(how), ?raidTaken(how), ?raidWreckage(how, tick: tick), ?raidBreach(how)]) {
        rows.add((_) => Padding(padding: const EdgeInsets.only(top: 4), child: Text('${line.toUpperCase()}.', style: _small.copyWith(color: C.text2))));
      }
    } else if (ended?.salvage case final pile?) {
      rows.add((_) => Padding(
          padding: const EdgeInsets.only(top: 4), child: Text('WRECKAGE LEFT: ${amountsOf(pile).toUpperCase()}', style: _small.copyWith(color: C.gold))));
    }
    if (!wide) {
      gap(10);
      rows.add((_) => Text(marksRule, key: const Key('bv-legend'), style: _small.copyWith(fontSize: 9.5)));
    }
    gap(8);
    return rows;
  }

  static const _cell = 46.0;

  TextStyle get _head => T.cond(size: 9, color: C.text3, weight: FontWeight.w500, spacing: 1.2);

  Widget _cells(String name, List<(String, Color)> figures, {Key? key, bool head = false, Color tint = C.text2, double width = _cell}) => Padding(
        key: key,
        padding: const EdgeInsets.only(top: 2),
        child: Row(crossAxisAlignment: CrossAxisAlignment.baseline, textBaseline: TextBaseline.alphabetic, children: [
          Expanded(child: Text(name, overflow: TextOverflow.ellipsis, style: head ? _head : _name.copyWith(color: tint))),
          for (final (text, color) in figures)
            SizedBox(width: width, child: Text(text, textAlign: TextAlign.right, style: head ? _head : _figure.copyWith(color: color))),
        ]),
      );

  /// One side of the round by class: what entered it, what it destroyed,
  /// what stands after it, and of those how many are damaged and at risk.
  void _standing(List<WidgetBuilder> rows, String title, proto.BattleSide side, {required int lost, required Color color, required String tag, required bool wide}) {
    rows.add((_) => Row(key: Key(tag), children: [
          Expanded(child: Lbl(title, color: color, size: 10.5)),
          Text.rich(
            TextSpan(children: [
              TextSpan(text: '${shortCount(side.entered)} ENTERED  '),
              if (lost > 0) TextSpan(text: '-${shortCount(lost)}  ', style: const TextStyle(color: atRiskColor)),
              TextSpan(text: '${shortCount(side.standing)} STAND', style: const TextStyle(color: C.text)),
            ]),
            style: _small.copyWith(color: C.text2),
          ),
        ]));
    rows.add((_) => _cells('', [for (final name in ['BEFORE', 'LOST', 'AFTER', 'DMG', 'RISK', if (wide) 'HULL']) (name, C.text3)], head: true));
    for (final g in side.groups) {
      if (side.groups.length > 1 || !g.mine || g.fleeing || g.battery) {
        rows.add((_) => Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Text.rich(
              key: Key(g.battery ? '$tag-battery' : '$tag-group-${g.fleetId}'),
              TextSpan(children: [
                TextSpan(text: groupName(g).toUpperCase(), style: g.battery ? const TextStyle(color: C.crystal) : null),
                if (g.battery) const TextSpan(text: '  NO FLEET: THEY STAND WHERE THEY ARE'),
                if (g.fleeing) const TextSpan(text: '  LEAVING: FIRED ON, NOT FIRING', style: TextStyle(color: damagedColor)),
              ]),
              overflow: TextOverflow.ellipsis,
              style: _small,
            )));
      }
      for (final u in g.units) {
        final gone = u.count == 0;
        rows.add((_) => _cells(
            u.unit.label.toUpperCase(),
            [
              (shortCount(u.entered), C.text2),
              (u.lost > 0 ? '-${shortCount(u.lost)}' : '', atRiskColor),
              (shortCount(u.count), gone ? C.text3 : C.text),
              (u.damaged > 0 ? shortCount(u.damaged) : '', damagedColor),
              (u.atRisk > 0 ? shortCount(u.atRisk) : '', atRiskColor),
              if (wide) (gone ? '' : shortCount(u.hull), C.text3),
            ],
            key: Key('$tag-${g.battery ? 'battery' : g.fleetId}-${u.unit.toJson()}'),
            tint: gone ? C.text4 : g.battery ? C.a(C.crystal, .85) : C.text2));
      }
    }
  }

  /// What a side fired in the round: its totals as a sentence, then every
  /// volley, the ones that decided the round first.
  void _fire(List<WidgetBuilder> rows, String title, String target, proto.BattleSide side,
      {required Color color, required String tag, required bool wide}) {
    rows.add((_) => Lbl(title, color: color, size: 10.5));
    rows.add((_) => Text(
        side.shots == 0
            ? 'NOT A SHOT.'
            : '${shortCount(side.shots)} SHOT${side.shots == 1 ? '' : 'S'} OF POWER ${shortCount(side.power)}. THE ${target.toUpperCase()}\' SHIELDS TOOK '
                '${shortCount(side.absorbed)}, ${shortCount(side.hullDamage)} REACHED HULLS, ${shortCount(side.destroyed)} DESTROYED.',
        key: Key(tag),
        style: _small.copyWith(color: C.text2)));
    final volleys = volleysByWeight(side.volleys);
    if (volleys.isEmpty) return;
    if (wide) {
      rows.add((_) => _cells('WHO AT WHOM', [for (final name in ['SHOTS', 'BOUNCED', 'WASTED', 'SHIELDS', 'HULL', 'DESTR']) (name, C.text3)], head: true, width: 58));
    }
    for (final (i, v) in volleys.indexed) {
      final name = '${v.from.label} > ${v.at.label}'.toUpperCase();
      rows.add(wide
          ? (_) => _cells(
              name,
              [
                (shortCount(v.shots), C.text),
                (v.bounced > 0 ? shortCount(v.bounced) : '', C.crystal),
                (v.wasted > 0 ? shortCount(v.wasted) : '', C.text3),
                (shortCount(v.absorbed), C.text2),
                (shortCount(v.hullDamage), v.hullDamage > 0 ? C.a300 : C.text3),
                (v.destroyed > 0 ? shortCount(v.destroyed) : '', color),
              ],
              key: Key('$tag-$i'),
              width: 58)
          : (_) => Padding(
              key: Key('$tag-$i'),
              padding: const EdgeInsets.only(top: 4),
              child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
                Row(children: [
                  Expanded(child: Text(name, overflow: TextOverflow.ellipsis, style: T.cond(size: 10.5, color: C.text2, spacing: .9))),
                  if (v.destroyed > 0) Text('${shortCount(v.destroyed)} DESTROYED', style: T.mono(size: 10, color: color, weight: FontWeight.w600)),
                ]),
                Wrap(spacing: 10, children: [for (final part in volleyFigures(v)) Text(part, style: _small)]),
              ])));
    }
  }
}
