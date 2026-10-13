import 'package:flutter/material.dart';

import '../console/battle.dart';
import '../console/intel.dart' show clockFmt, groupDigits;
import '../design/draw.dart';
import '../design/gauges.dart';
import '../design/panel.dart';
import '../design/symbols.dart';
import '../design/tokens.dart';
import '../models/fleet.dart' as ui;
import '../models/game_state.dart';
import '../protocol/protocol.dart' as proto;
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
      proto.BattleOutcome.withdrawn || proto.BattleOutcome.standoff || proto.BattleOutcome.fled => C.a300,
    };

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

/// How many volleys of a side the round panel shows before the rest are
/// asked for.
const volleysShown = 3;

/// A fight as text: for the round it stands at, what each side has left by
/// class and lost in it, what each side fired, and the fight's losses so
/// far; once it has ended, how it ended, after how many rounds and at what
/// cost to each side. Every figure is the round's or the state's.
class BattlePanel extends StatefulWidget {
  final Battle battle;

  /// Puts the report of a fight that has ended away.
  final VoidCallback? onDismiss;
  const BattlePanel({super.key, required this.battle, this.onDismiss});

  @override
  State<BattlePanel> createState() => _BattlePanelState();
}

class _BattlePanelState extends State<BattlePanel> {
  bool _allFleets = false, _allHostiles = false, _lastRound = false;

  Battle get b => widget.battle;

  @override
  Widget build(BuildContext context) {
    final round = b.round;
    final ended = b.ended;
    return ConsolePanel(
      key: const Key('battle-panel'),
      title: 'Battle',
      titleColor: C.ember,
      mark: C.ember,
      subText: ended != null ? 'ENDED - ${roundsOf(ended.rounds).toUpperCase()}' : 'ROUND ${b.rounds} - ${b.sector.q}, ${b.sector.r}',
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
        if (ended != null) ..._end(ended),
        if (ended == null && round == null) Text('THE FIRST ROUND IS NOT FOUGHT YET.', style: _small),
        if (round != null && (ended == null || _lastRound)) ..._round(round),
        if (ended == null) ...[
          const SizedBox(height: 9),
          const Lbl('Lost so far'),
          ..._losses(b.losses, key: 'battle-losses'),
        ],
      ]),
    );
  }

  List<Widget> _end(proto.CombatEndedEvent e) => [
        Text(outcomeTitle(e.outcome),
            key: const Key('battle-outcome'), style: T.cond(size: 17, color: _outcomeColor(e.outcome), weight: FontWeight.w700, spacing: 2.4)),
        Text('${outcomeSentence(e.outcome)}, after ${roundsOf(e.rounds)} at ${e.sector.q}, ${e.sector.r}.'.toUpperCase(), style: _small),
        const SizedBox(height: 7),
        const Lbl('Losses'),
        ..._losses(e.losses, key: 'battle-end-losses', none: 'NO SHIP WAS LOST ON EITHER SIDE.'),
        if (e.salvage case final pile?)
          Padding(padding: const EdgeInsets.only(top: 5), child: Text('WRECKAGE LEFT: ${amountsOf(pile).toUpperCase()}', key: const Key('battle-wreckage'), style: _small.copyWith(color: C.gold))),
        Padding(
            padding: const EdgeInsets.only(top: 8),
            child: Wrap(spacing: 6, runSpacing: 6, children: [
              if (b.round != null)
                ConsoleButton(_lastRound ? 'Hide last round' : 'Show last round',
                    key: const ValueKey('battle-last-round'), small: true, active: _lastRound, onPressed: () => setState(() => _lastRound = !_lastRound)),
              if (widget.onDismiss != null)
                ConsoleButton('Dismiss', key: const ValueKey('battle-dismiss'), small: true, onPressed: widget.onDismiss),
            ])),
        if (_lastRound) const SizedBox(height: 8),
      ];

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

  List<Widget> _round(proto.BattleRoundEvent r) => [
        _standing('Fleets', r.fleets, lost: r.hostiles.destroyed, color: C.own, tag: 'battle-fleets'),
        const SizedBox(height: 8),
        _standing('Hostiles', r.hostiles, lost: r.fleets.destroyed, color: C.ember, tag: 'battle-hostiles'),
        const SizedBox(height: 9),
        _fire('Fleets fired', r.fleets, all: _allFleets, color: C.own, tag: 'battle-fire-fleets', toggle: () => setState(() => _allFleets = !_allFleets)),
        const SizedBox(height: 8),
        _fire('Hostiles fired', r.hostiles, all: _allHostiles, color: C.ember, tag: 'battle-fire-hostiles', toggle: () => setState(() => _allHostiles = !_allHostiles)),
      ];

  /// What a side has standing after the round, group by group and class by
  /// class, with what the round took from it.
  Widget _standing(String title, proto.BattleSide side, {required int lost, required Color color, required String tag}) => Column(
        key: Key(tag),
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
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
          _cells('', 'LEFT', 'LOST', 'DMG', 'RISK', head: true),
          for (final g in side.groups) ...[
            if (side.groups.length > 1 || !g.mine || g.fleeing)
              Padding(
                  padding: const EdgeInsets.only(top: 3),
                  child: Text.rich(
                    key: Key('$tag-group-${g.fleetId}'),
                    TextSpan(children: [
                      TextSpan(text: groupName(g).toUpperCase()),
                      if (g.fleeing) const TextSpan(text: '  LEAVING: FIRED ON, NOT FIRING', style: TextStyle(color: damagedColor)),
                    ]),
                    overflow: TextOverflow.ellipsis,
                    style: _small,
                  )),
            for (final u in g.units)
              _cells(u.unit.label.toUpperCase(), shortCount(u.count), u.lost > 0 ? '-${shortCount(u.lost)}' : '',
                  u.damaged > 0 ? shortCount(u.damaged) : '', u.atRisk > 0 ? shortCount(u.atRisk) : '',
                  gone: u.count == 0),
          ],
        ],
      );

  /// A row of the standing table: a kind of unit, how many are left, how
  /// many the round destroyed, and of those left how many are damaged and
  /// how many at risk.
  Widget _cells(String name, String left, String lost, String damaged, String atRisk, {bool head = false, bool gone = false}) {
    final label = T.cond(size: 9, color: C.text3, weight: FontWeight.w500, spacing: 1.2);
    Widget cell(String text, Color color) =>
        SizedBox(width: 40, child: Text(text, textAlign: TextAlign.right, style: head ? label : _figure.copyWith(color: color)));
    return Padding(
      padding: const EdgeInsets.only(top: 2),
      child: Row(crossAxisAlignment: CrossAxisAlignment.baseline, textBaseline: TextBaseline.alphabetic, children: [
        Expanded(child: Text(name, overflow: TextOverflow.ellipsis, style: _name.copyWith(color: gone ? C.text4 : C.text2))),
        cell(left, gone ? C.text3 : C.text),
        cell(lost, atRiskColor),
        cell(damaged, damagedColor),
        cell(atRisk, atRiskColor),
      ]),
    );
  }

  /// What a side fired: its own totals, then its volleys with the ones that
  /// decided the round first and the rest behind a button.
  Widget _fire(String title, proto.BattleSide side, {required bool all, required Color color, required String tag, required VoidCallback toggle}) {
    final volleys = volleysByWeight(side.volleys);
    final hidden = volleys.length - volleysShown;
    return Column(key: Key(tag), crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
      Lbl(title, color: color),
      _figures(fireFigures(side), _small.copyWith(color: C.text2)),
      for (final v in all ? volleys : volleys.take(volleysShown)) _volley(v, color),
      if (hidden > 0)
        Padding(
            padding: const EdgeInsets.only(top: 5),
            child: Align(
                alignment: Alignment.centerLeft,
                child: ConsoleButton(all ? 'Fewer volleys' : '$hidden more volley${hidden == 1 ? '' : 's'}',
                    key: ValueKey('$tag-more'), small: true, active: all, onPressed: toggle))),
    ]);
  }

  Widget _volley(proto.Volley v, Color color) => Padding(
        padding: const EdgeInsets.only(top: 4),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
          Row(children: [
            Expanded(
                child: Text('${v.from.label} > ${v.at.label}'.toUpperCase(),
                    overflow: TextOverflow.ellipsis, style: T.cond(size: 10.5, color: C.text2, spacing: .9))),
            if (v.destroyed > 0) Text('${shortCount(v.destroyed)} DESTROYED', style: T.mono(size: 10, color: color, weight: FontWeight.w600)),
          ]),
          _figures(volleyFigures(v), _small),
        ]),
      );

  Widget _figures(List<String> parts, TextStyle style) =>
      Wrap(spacing: 10, children: [for (final part in parts) Text(part, style: style)]);
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
      line = Text('${outcomeTitle(ended.outcome)} - ${roundsOf(ended.rounds).toUpperCase()}',
          overflow: TextOverflow.ellipsis, style: T.cond(size: 11.5, color: _outcomeColor(ended.outcome), spacing: 1.2));
    } else if (r == null) {
      line = Text('BATTLE JOINED', style: T.cond(size: 11.5, color: C.ember, spacing: 1.2));
    } else {
      line = Text.rich(
        TextSpan(children: [
          TextSpan(text: 'ROUND ${r.round}  ', style: const TextStyle(color: C.ember)),
          TextSpan(text: 'FLEETS ${shortCount(r.fleets.standing)}', style: const TextStyle(color: C.own)),
          if (r.hostiles.destroyed > 0) TextSpan(text: ' -${shortCount(r.hostiles.destroyed)}', style: const TextStyle(color: atRiskColor)),
          const TextSpan(text: '  V  ', style: TextStyle(color: C.text3)),
          TextSpan(text: 'HOSTILES ${shortCount(r.hostiles.standing)}', style: const TextStyle(color: C.text)),
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
