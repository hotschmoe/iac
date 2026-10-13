import '../protocol/protocol.dart' as proto;
import '../state/state_mapper.dart' show amountsOf;
import 'intel.dart' show clockFmt, groupDigits;

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

/// Who a fleet or a group of a fight is, to the player reading it. A
/// [battery] is no fleet: the defences of the place raided, under the name
/// the server gives them, with the empire's when they are another's.
String sideName({required int fleetId, required String? owner, required String label, required bool mine, bool battery = false}) {
  if (battery) return mine || owner == null ? label : "$owner's $label";
  return mine ? 'F$fleetId' : owner != null ? "$owner's F$fleetId" : label.isEmpty ? 'hostile group' : label;
}

String groupName(proto.BattleGroup g) => sideName(fleetId: g.fleetId, owner: g.owner, label: g.label, mine: g.mine, battery: g.battery);

String lossesName(proto.BattleLosses l) => sideName(fleetId: l.fleetId, owner: l.owner, label: l.label, mine: l.mine, battery: l.battery);

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
      proto.BattleOutcome.withdrawn => 'RECALLED',
      proto.BattleOutcome.standoff => 'STAND-OFF',
      proto.BattleOutcome.fled => 'FLED UNDER FIRE',
      proto.BattleOutcome.outlasted => 'OUTLASTED',
    };

/// The same as a sentence of the log.
String outcomeSentence(proto.BattleOutcome o) => switch (o) {
      proto.BattleOutcome.hostilesDestroyed => 'the hostiles are destroyed',
      proto.BattleOutcome.fleetsDestroyed => 'every fleet is destroyed',
      proto.BattleOutcome.bothDestroyed => 'both sides fell in the same round',
      proto.BattleOutcome.withdrawn => 'the last fleet made the emergency jump home; the hostiles keep what they lost and mend in time',
      proto.BattleOutcome.standoff => 'a stand-off: neither side could hurt the other, and both stay as they are',
      proto.BattleOutcome.fled => 'the last fleet jumped out under fire; the hostiles keep what they lost and mend in time',
      proto.BattleOutcome.outlasted => 'the raiders gave up with a defender still standing and left with nothing',
    };

String roundsOf(int n) => '$n round${n == 1 ? '' : 's'}';

// ── Raids ──────────────────────────────────────────────────────

/// "ROUND 2 OF 6": a raid's round against the most it is fought for.
String raidRoundOf(int round) => 'ROUND $round OF ${proto.raidRounds}';

/// "4 rounds left at the most", from the state's `rounds_left`.
String roundsLeftOf(int n) => n == 0 ? 'no round left' : '${roundsOf(n)} left at the most';

/// "75%": a share of the fights the server played, or of a side's worth.
String sharePct(double share) => '${(share * 100).round()}%';

/// "destroyed 25%, outlasted 75%, lost 0%": how the fights the server
/// played ended, in the forecast's own three shares.
String raidShares(proto.RaidOdds o) => 'destroyed ${sharePct(o.destroyed)}, outlasted ${sharePct(o.outlasted)}, lost ${sharePct(o.lost)}';

/// "you keep 62% of what stands here, the raiders 31% of theirs": the
/// forecast's `kept` and `raiders_kept`.
String raidKept(proto.RaidOdds o) => 'you keep ${sharePct(o.kept)} of what stands here, the raiders ${sharePct(o.raidersKept)} of theirs';

/// "HELD: a defender stands when the raiders give up; ...": the label of a
/// forecast with the sentence Rust holds for it.
String raidVerdict(proto.RaidOdds o) => '${o.label.label}: ${o.label.outcome}';

/// What the three shares of a forecast are shares of.
const raidSharesRule = 'The server plays the fight forward a few times over. Destroyed: no raider is left, and at the homeworld its wreckage is. '
    'Outlasted: a defender still stands after ${proto.raidRounds} rounds and the raiders leave with nothing. '
    'Lost: no defender is left and the raid takes its share.';

/// What a raid's fight costs for good and what it does not, in the engine's
/// three figures.
const rebuildRule = 'Of the defences that fall in a raid about ${proto.defenceRebuildPct} in a hundred stand again the moment it ends, for nothing: '
    'each on its own chance where fewer than ${proto.defenceRebuildEachBelow} of a kind fell, a share within ${proto.defenceRebuildSpread} '
    'either side of it where more did. Ships and Solar Satellites lost are lost.';

/// "a raid of 12x Light Fighter, 2x Cruiser (power 1,351)": what a
/// forecast says is flown. [opening] starts a sentence with it.
String raidForce(List<proto.ShipCount> ships, double power, {bool opening = false}) =>
    '${opening ? 'A' : 'a'} raid of ${shipsList(ships)} (power ${groupDigits(power.round())})';

/// "Being fought, 4 rounds left at the most", "Arrives in 2:50" or "None
/// announced; one of typical size": where a forecast's raid stands at
/// [tick], from `rounds_left` and `arrival_tick`.
String raidWhen(proto.RaidForecast raid, int tick) => switch ((raid.roundsLeft, raid.arrivalTick)) {
      (final left?, _) => 'Being fought, ${roundsLeftOf(left)}',
      (null, final at?) => 'Arrives in ${clockFmt(at > tick ? at - tick : 0)}',
      _ => 'None announced; one of typical size',
    };

/// "FOUGHT" for a raid that has arrived, "2:50" for one on its way, in the
/// room a bar has; null for a forecast of no raid in particular.
String? raidClock(proto.RaidForecast? raid, int tick) => switch ((raid?.roundsLeft, raid?.arrivalTick)) {
      (_?, _) => 'FOUGHT',
      (null, final at?) => clockFmt(at > tick ? at - tick : 0),
      _ => null,
    };

/// A forecast in one sentence, for the log, a notice and the command bar.
String raidForecastLine(proto.RaidForecast raid, int tick) =>
    '${raidWhen(raid, tick)}: ${raidForce(raid.ships, raid.power)}. Forecast ${raidVerdict(raid.odds)} '
    '(${raidShares(raid.odds)}; ${raidKept(raid.odds)})';

/// How a raid ended, in the three ways a raid's fight ends and the fourth
/// that is no fight.
enum RaidEnding {
  destroyed('RAID DESTROYED', 'destroyed', 'no raider is left'),
  outlasted('RAID OUTLASTED', 'outlasted', 'the raiders gave up with a defender still standing and took nothing'),
  lost('RAID LOST', 'lost', 'no defender was left standing'),
  unopposed('RAID LOST WITHOUT A FIGHT', 'lost without a fight', 'nothing stood in its way');

  final String title;
  final String word;
  final String sentence;
  const RaidEnding(this.title, this.word, this.sentence);

  /// The server's verdict on the raid (`defended`) and how its fight ended
  /// (`outcome`, absent when no round was fought).
  static RaidEnding of(proto.RaidResolvedEvent e) => switch ((e.defended, e.outcome)) {
        (false, null) => unopposed,
        (false, _) => lost,
        (true, proto.BattleOutcome.outlasted) => outlasted,
        (true, _) => destroyed,
      };
}

/// "Raid on your homeworld" or "Raid on your outpost at 10, -2".
String raidPlace(proto.RaidResolvedEvent e) => switch (e.outpost) {
      null => 'Raid on your homeworld',
      final at => 'Raid on your outpost at ${at.q}, ${at.r}',
    };

/// "Rebuilt at once: 3x Rocket Launcher", or that none was, when defences
/// fell at the homeworld; null when none did, and for an outpost, whose
/// platform is not rebuilt unit by unit.
String? raidRebuilt(proto.RaidResolvedEvent e) => e.rebuilt.isNotEmpty
    ? 'Rebuilt at once: ${unitsList(e.rebuilt)}'
    : e.structuresLost > 0 && e.outpost == null
        ? 'None of the defences that fell was rebuilt'
        : null;

/// "Taken: 2,525 Fe, 1,865 Cr": what a lost raid took; null for one repelled.
String? raidTaken(proto.RaidResolvedEvent e) => e.defended ? null : 'Taken: ${amountsOf(e.resourcesLost)}';

/// "Wreckage left: 8,100 Fe, 2,700 Cr, which drifts away in 10:00 ...";
/// null when the raid left none. [tick] turns the pile's last tick into a
/// time from now.
String? raidWreckage(proto.RaidResolvedEvent e, {int? tick}) {
  final pile = e.salvageDropped;
  if (pile == null) return null;
  final drift = switch ((e.salvageDespawnTick, tick)) {
    (final at?, final now?) => ', which drifts away in ${clockFmt(at > now ? at - now : 0)} unless a fleet docked at home scoops it',
    _ => '',
  };
  return 'Wreckage left: ${amountsOf(pile)}$drift';
}

/// What a breach means, when the raid left one.
String? raidBreach(proto.RaidResolvedEvent e) =>
    e.breached ? 'The outpost is breached: no extractor, no refuelling and no platform until it has repaired itself from its store' : null;

/// A raid's ending as the lines of its summary, every figure the event's:
/// what the raiders flew and lost, what the defender lost by unit, what was
/// rebuilt at once, what was taken, the wreckage and a breach.
List<String> raidSummary(proto.RaidResolvedEvent e, {int? tick}) => [
      if (e.raiders.isNotEmpty) 'The raiders flew ${shipsList(e.raiders)} and lost ${shipsList(e.raidersLost)}',
      'You lost ${unitsList(e.lost)}',
      ?raidRebuilt(e),
      ?raidTaken(e),
      ?raidWreckage(e, tick: tick),
      ?raidBreach(e),
    ];

/// "Raid on your homeworld destroyed after 4 rounds: no raider is left":
/// where a raid was and how it ended, for a line that has to be short.
String raidHeadline(proto.RaidResolvedEvent e) {
  final ending = RaidEnding.of(e);
  final after = ending == RaidEnding.unopposed ? '' : ' after ${roundsOf(e.rounds)}';
  return '${raidPlace(e)} ${ending.word}$after: ${ending.sentence}';
}

/// The headline and the whole summary in one line, for a notice.
String raidLine(proto.RaidResolvedEvent e, {int? tick}) => '${raidHeadline(e)}. ${raidSummary(e, tick: tick).join('. ')}';

/// "4x Rocket Launcher"; "none" for a platform that is not standing.
String platformList(List<proto.UnitCount> units) => units.isEmpty ? 'none' : unitsList(units);

String ticksOf(int n) => '$n tick${n == 1 ? '' : 's'}';

String hexesOf(int n) => '$n hex${n == 1 ? '' : 'es'}';

String _fuelOf(double fuel) {
  final tenths = (fuel * 10).round() / 10;
  return tenths == tenths.roundToDouble() ? groupDigits(tenths.round()) : tenths.toStringAsFixed(1);
}

/// "5 fuel, gone in 2 ticks, 1 round under fire": what a jump out of a
/// fight costs, in the quote's own figures.
String flightPrice(proto.FlightQuote q) => '${_fuelOf(q.fuel)} fuel, gone in ${ticksOf(q.ticks)}, ${roundsOf(q.rounds)} under fire';

/// Who would follow a fleet that left now, and how far: the quote's `chase`.
/// A group that fires no round at a hex still catches a fleet that stops.
String chaseLine(proto.ChaseForecast? c) {
  if (c == null) return 'Nothing follows.';
  final far = 'The ${c.label} follows ${hexesOf(c.hexes)}';
  return c.roundsPerHex > 0
      ? '$far and fires ${roundsOf(c.roundsPerHex)} at each on a fleet that jumps on at once; one that stops is caught.'
      : '$far and lands too late to fire on a fleet that jumps on at once; one that stops is caught.';
}

/// "Leaving for 3, -1: jump in 2 ticks, 1 more round under fire": a fleet
/// charging its jump out of a fight.
String fleeingLine(proto.Fleeing f) =>
    'Leaving for ${f.to.q}, ${f.to.r}: jump in ${ticksOf(f.ticks)}, ${f.rounds == 0 ? 'no more rounds' : '${roundsOf(f.rounds)} more'} under fire';

/// What calling off a jump out of a fight does, for every place Stop is offered.
const stopFlightRule = 'Stop calls the jump off: the fleet fights on and the fuel it burned is forfeit.';

/// "The 3x cruiser pack lands in 1 tick, from 4, -2": the group on a
/// fleet's trail.
String pursuitLine(proto.Pursuit p) => 'The ${p.label} lands in ${ticksOf(p.arrivesIn)}, from ${p.from.q}, ${p.from.r}';

/// Whether the pursuer lands before the fleet can be gone, for a fleet that
/// is [charging] a jump and for one that is not, and how much further it
/// follows: `catches` and `hexes_left`, worded.
String pursuitVerdict(proto.Pursuit p, {required bool charging}) {
  final lands = p.catches
      ? (charging ? 'It lands before the jump under way does and fires on the fleet.' : 'It lands before a jump ordered now would, and fires on the fleet.')
      : (charging ? 'The jump under way is gone before it lands.' : 'A jump ordered now is gone before it lands; a fleet that stays is caught.');
  final leash = p.hexesLeft == 0 ? 'It turns back after this jump.' : 'It follows ${hexesOf(p.hexesLeft)} more after this one.';
  return '$lands $leash';
}

/// A `Pursuit` event as the log prints it.
String pursuitEventLine(proto.PursuitEvent e) {
  final whom = e.mine ? 'F${e.fleetId}' : "${e.owner}'s F${e.fleetId}";
  final landing = switch ((e.to, e.arrivesIn)) {
    (final to?, final ticks?) => ': it jumps to ${to.q}, ${to.r} and lands in ${ticksOf(ticks)}',
    _ => '',
  };
  return switch (e.phase) {
    proto.PursuitPhase.began =>
      'Pursuit: the ${e.label} leaves ${e.sector.q}, ${e.sector.r} after $whom$landing${e.hexesLeft == 0 ? ', its last' : ', and follows ${hexesOf(e.hexesLeft)} more'}',
    proto.PursuitPhase.continued =>
      'Pursuit: the ${e.label} found ${e.sector.q}, ${e.sector.r} empty and goes on after $whom$landing${e.hexesLeft == 0 ? ', its last' : ', and follows ${hexesOf(e.hexesLeft)} more'}',
    proto.PursuitPhase.abandoned => 'The ${e.label} gave up on $whom at ${e.sector.q}, ${e.sector.r}${e.why == null ? '' : ': ${e.why}'}'
        '${e.to == null ? '' : '. It goes back by ${e.to!.q}, ${e.to!.r}'}',
  };
}

/// "3 of 5 ships" for a group with losses, "5 ships" for one with none.
String musterOf(proto.NpcFleetInfo g) => g.shipCount < g.muster ? '${groupDigits(g.shipCount)} of ${shipsOf(g.muster)}' : shipsOf(g.shipCount);

/// How far a group follows a fleet that leaves a fight with it, and the
/// ticks one jump takes it.
String chaseHabit(proto.NpcFleetInfo g) =>
    '${g.chaseHexes == 0 ? 'Follows nobody' : 'Follows ${hexesOf(g.chaseHexes)}'}, a jump takes it ${ticksOf(g.jumpTicks)}';

/// What a group that is out of its own sector is doing: its `errand`, as of
/// [tick]. [mine] says the fleet it follows is the player's.
String errandLine(proto.GroupErrand e, int tick, {required bool mine}) {
  final landing = switch ((e.to, e.arrivesAt)) {
    (final to?, final at?) => ', jumping to ${to.q}, ${to.r}, lands in ${ticksOf(at > tick ? at - tick : 0)}',
    _ => '',
  };
  if (e.quarry == null) {
    return e.to == null ? 'Away from its own sector at ${e.home.q}, ${e.home.r}' : 'On its way back to ${e.home.q}, ${e.home.r}$landing';
  }
  final whom = mine ? 'your F${e.quarry}' : 'F${e.quarry}${e.quarryOwner == null ? '' : ' of ${e.quarryOwner}'}';
  return 'After $whom$landing${e.hexesLeft == 0 ? ', then turns back' : ', and ${hexesOf(e.hexesLeft)} more'}';
}

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
