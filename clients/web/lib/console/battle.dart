import '../protocol/protocol.dart' as proto;
import '../state/state_mapper.dart' show amountsOf;
import 'intel.dart' show groupDigits;

/// "4,120", "165K", "1.25M", "30.5M": a count or a hull figure in a fleet or
/// battle row. Exact up to 99,999 and three figures above, so a row keeps
/// its width when a fleet grows from five ships to five thousand.
String shortCount(int v) {
  if (v.abs() < 100000) return groupDigits(v);
  const units = [(1000, 'K'), (1000000, 'M'), (1000000000, 'G'), (1000000000000, 'T')];
  for (var i = 0; i < units.length; i++) {
    final (size, suffix) = units[i];
    final m = v / size;
    final text = m.abs() >= 100 ? m.toStringAsFixed(0) : m.abs() >= 10 ? m.toStringAsFixed(1) : m.toStringAsFixed(2);
    if (double.parse(text).abs() < 1000 || i == units.length - 1) return '$text$suffix';
  }
  return groupDigits(v);
}

/// "1 ship", "412 ships".
String shipsOf(int n) => '${groupDigits(n)} ship${n == 1 ? '' : 's'}';

/// "164,960 / 165,600": hull held against hull at full.
String hullOf(int hull, int hullMax) => '${shortCount(hull)} / ${shortCount(hullMax)}';

/// The share of [hullMax] that [hull] is, for a bar; full with no hull to
/// lose.
double hullShare(int hull, int hullMax) => hullMax > 0 ? (hull / hullMax).clamp(0.0, 1.0) : 1;

/// What "at risk" means, in the engine's figure.
const atRiskRule = 'AT RISK: AT OR UNDER ${proto.explodeBelowPct}% HULL, AND MAY EXPLODE WHEN A SHOT NEXT BREAKS ITS SHIELD.';

/// "4x Light Fighter, 2x Cruiser"; "nothing" for none.
String unitsList(List<proto.UnitCount> units) =>
    units.isEmpty ? 'nothing' : [for (final u in units) '${groupDigits(u.count)}x ${u.unit.label}'].join(', ');

/// The same for ships named by class.
String shipsList(List<proto.ShipCount> ships) =>
    ships.isEmpty ? 'nothing' : [for (final s in ships) '${groupDigits(s.count)}x ${s.shipClass.label}'].join(', ');

/// "32x Light Fighter, 4x Cruiser": a fleet or a hostile group by class.
String groupsList(List<proto.ShipGroupState> ships) =>
    ships.isEmpty ? 'no ships' : [for (final g in ships) '${groupDigits(g.count)}x ${g.shipClass.label}'].join(', ');

/// Who a fleet or a group of a fight is, to the player reading it.
String sideName({required int fleetId, required String? owner, required String label, required bool mine}) =>
    mine ? 'F$fleetId' : owner != null ? "$owner's F$fleetId" : label.isEmpty ? 'hostile group' : label;

String groupName(proto.BattleGroup g) => sideName(fleetId: g.fleetId, owner: g.owner, label: g.label, mine: g.mine);

String lossesName(proto.BattleLosses l) => sideName(fleetId: l.fleetId, owner: l.owner, label: l.label, mine: l.mine);

/// "F3 lost 4x Light Fighter" or "3x cruiser pack lost 5x Cruiser and is
/// destroyed": one fleet's or group's losses over a fight.
String lossesLine(proto.BattleLosses l) {
  final who = lossesName(l);
  if (l.lost.isEmpty) return '$who lost nothing';
  return '$who lost ${unitsList(l.lost)}${l.destroyed ? ' and is destroyed' : ''}';
}

/// How a fight ended, as a heading.
String outcomeTitle(proto.BattleOutcome o) => switch (o) {
      proto.BattleOutcome.hostilesDestroyed => 'HOSTILES DESTROYED',
      proto.BattleOutcome.fleetsDestroyed => 'FLEETS DESTROYED',
      proto.BattleOutcome.bothDestroyed => 'BOTH SIDES DESTROYED',
      proto.BattleOutcome.withdrawn => 'FLEETS WITHDREW',
      proto.BattleOutcome.standoff => 'STAND-OFF',
    };

/// The same as a sentence of the log.
String outcomeSentence(proto.BattleOutcome o) => switch (o) {
      proto.BattleOutcome.hostilesDestroyed => 'the hostiles are destroyed',
      proto.BattleOutcome.fleetsDestroyed => 'every fleet is destroyed',
      proto.BattleOutcome.bothDestroyed => 'both sides fell in the same round',
      proto.BattleOutcome.withdrawn => 'the fleets left; the hostiles are whole again',
      proto.BattleOutcome.standoff => 'a stand-off: neither side could hurt the other, and the hostiles are whole again',
    };

String roundsOf(int n) => '$n round${n == 1 ? '' : 's'}';

/// A fight's end as the log prints it: the outcome, the rounds, what the
/// player's fleets lost and what was left behind.
String endedLine(proto.CombatEndedEvent e) {
  final mine = [for (final l in e.losses) if (l.mine && l.lost.isNotEmpty) lossesLine(l)];
  final pile = e.salvage == null ? '' : ' Wreckage ${amountsOf(e.salvage!)}.';
  return 'Combat ended in ${e.sector} after ${roundsOf(e.rounds)}: ${outcomeSentence(e.outcome)}.'
      '${mine.isEmpty ? '' : ' ${mine.join('; ')}.'}$pile';
}

/// One round as the log prints it, from the two sides' own totals.
String roundLine(proto.BattleRoundEvent r) =>
    'Round ${r.round} in ${r.sector}: fleets fired ${groupDigits(r.fleets.shots)} shots and destroyed ${groupDigits(r.fleets.destroyed)}, '
    'hostiles fired ${groupDigits(r.hostiles.shots)} and destroyed ${groupDigits(r.hostiles.destroyed)}; '
    '${groupDigits(r.fleets.standing)} against ${groupDigits(r.hostiles.standing)} still stand';

/// A side's volleys with the ones that decided the round first: by units
/// destroyed, then by hull damage, then by shots. The order is the panel's;
/// every figure is the round's.
List<proto.Volley> volleysByWeight(List<proto.Volley> volleys) => [...volleys]..sort((a, b) {
    final d = b.destroyed.compareTo(a.destroyed);
    if (d != 0) return d;
    final h = b.hullDamage.compareTo(a.hullDamage);
    return h != 0 ? h : b.shots.compareTo(a.shots);
  });

/// "24 SHOTS", "4 BOUNCED", "SHIELDS 200", "HULL 1,000": what a volley did,
/// a figure a piece, so that a narrow panel breaks the line between two.
List<String> volleyFigures(proto.Volley v) => [
      '${shortCount(v.shots)} SHOT${v.shots == 1 ? '' : 'S'}',
      if (v.bounced > 0) '${shortCount(v.bounced)} BOUNCED',
      if (v.wasted > 0) '${shortCount(v.wasted)} WASTED',
      'SHIELDS ${shortCount(v.absorbed)}',
      'HULL ${shortCount(v.hullDamage)}',
    ];

/// The same for all a side fired in a round, from the side's own totals.
List<String> fireFigures(proto.BattleSide side) => [
      '${shortCount(side.shots)} SHOT${side.shots == 1 ? '' : 'S'}',
      'POWER ${shortCount(side.power)}',
      'SHIELDS TOOK ${shortCount(side.absorbed)}',
      'HULL DAMAGE ${shortCount(side.hullDamage)}',
      'DESTROYED ${shortCount(side.destroyed)}',
    ];

/// "W3 S2 A1": the Weapons, Shielding and Armour levels a fleet fights with.
String refitMark(proto.Refit r) => 'W${r.weapons} S${r.shielding} A${r.armour}';

/// The same in full.
String refitLevels(proto.Refit r) => 'Weapons ${r.weapons}, Shielding ${r.shielding}, Armour ${r.armour}';

/// What a fleet's refit line says when the home dock would raise it.
const refitWaits = 'A REFIT WAITS AT THE HOME DOCK: IT FIGHTS WITH OLDER LEVELS UNTIL IT DOCKS THERE.';

/// "mends all 4 (640 hull) for nothing", "mends 3 of 4 (500 hull) for
/// 1,200 Fe", or why it would mend nothing, in the engine's words: one
/// dock's answer to a fleet's damage.
String dockRepairLine(proto.RepairQuote quote, proto.DockRepair dock) {
  if (dock.refusal case final why?) return 'will not mend it: $why';
  final ships = dock.ships >= quote.ships ? 'all ${groupDigits(quote.ships)}' : '${groupDigits(dock.ships)} of ${groupDigits(quote.ships)}';
  return 'mends $ships (${shortCount(dock.hull)} hull) for ${amountsOf(dock.payment)}';
}

/// "4 damaged ships lack 640 hull": what a fleet's damage amounts to.
String damageLine(proto.RepairQuote q) =>
    '${groupDigits(q.ships)} damaged ship${q.ships == 1 ? ' lacks' : 's lack'} ${shortCount(q.hull)} hull';

/// How many marks stand for [count] ships on a scope where one mark stands
/// for [scale]: a class with a ship in it always has one.
int marksFor(int count, int scale) => count <= 0 ? 0 : (count + scale - 1) ~/ scale;

/// How many ships one mark stands for on a scope that draws [sides], each a
/// list of ship counts by class, with at most [most] marks a side: the
/// smallest of 1, 2, 5, 10, 20, 50 and so on that fits them. A drawing rule,
/// and nothing of the game's.
int markScale(Iterable<Iterable<int>> sides, {required int most}) {
  var scale = 1;
  const steps = [2, 2.5, 2];
  for (var i = 0;; i++) {
    final fits = sides.every((side) => side.fold<int>(0, (n, count) => n + marksFor(count, scale)) <= most);
    if (fits || scale >= 1000000) return scale;
    scale = (scale * steps[i % steps.length]).round();
  }
}
