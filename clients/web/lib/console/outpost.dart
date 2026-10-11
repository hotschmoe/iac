import 'dart:math' as math;

import '../protocol/protocol.dart' as proto;
import '../state/state_mapper.dart' show amountsOf;
import 'intel.dart' show clockFmt, groupDigits, hopCount;

/// The rings an outpost stands in, as RULES.md has them. The server is the
/// judge of a site; these only decide where the console offers to found.
const outpostMinRing = 9;
const outpostMaxRing = 20;

bool inOutpostRings(proto.Hex h) => h.distFromOrigin >= outpostMinRing && h.distFromOrigin <= outpostMaxRing;

/// What the outpost does and does not do for a fleet standing at it.
String outpostDockVoice(proto.OutpostState o) =>
    '${o.inService ? 'A FLEET FLOWN BY HAND IS REFUELLED HERE FROM THE STORE, AND' : 'NO TANK IS FILLED HERE WHILE IT IS ${o.breached ? 'BREACHED' : 'NOT YET OPEN'}. A FLEET'} '
    'LOADS AND UNLOADS BY ORDER.\n'
    'IT IS IN A RAID\'S REACH: A RAID THE OUTPOST LOSES DESTROYS A QUARTER OF THE SHIPS STANDING HERE.';

/// Ticks in a game hour, which is what the rules quote a yield by.
const _hour = 3600;

String _rate(double perTick) {
  final v = perTick * _hour;
  return v >= 100 ? groupDigits(v.round()) : v.toStringAsFixed(1);
}

/// "77.9 Fe, 39.0 De" made in a game hour; "nothing" when [perTick] is zero.
String yieldAnHour(proto.Resources perTick) {
  final parts = [
    for (final (v, unit) in [(perTick.metal, 'Fe'), (perTick.crystal, 'Cr'), (perTick.deuterium, 'De')])
      if (v > 0) '${_rate(v)} $unit',
  ];
  return parts.isEmpty ? 'nothing' : parts.join(', ');
}

/// "7 hops", or what the server means by leaving it out.
String hopsHome(int? hops) => hops == null ? 'more than 64 hops' : hopCount(hops);

/// The share of its cap the fullest resource of the store has reached: at
/// 1 the extractor makes no more of that resource.
double storeFill(proto.OutpostState o) {
  var fill = 0.0;
  for (final k in proto.ResourceKind.values) {
    final cap = k.of(o.storeCap);
    if (cap > 0) fill = math.max(fill, k.of(o.store) / cap);
  }
  return fill.clamp(0.0, 1.0);
}

/// Whether the store holds all of [k] it takes.
bool storeAtCap(proto.OutpostState o, proto.ResourceKind k) => k.of(o.storeCap) > 0 && k.of(o.store) >= k.of(o.storeCap);

/// "57 to 82": the least and the most a raid can be.
String raidRange(proto.PowerRange r) => '${groupDigits(r.least.round())} to ${groupDigits(r.most.round())}';

/// Where a defence stands beside the raids it may meet. A raid is decided
/// by the defence, taken up to a tenth higher or lower by chance, against
/// the raid's power, so only the two ends say much.
enum RaidCover {
  above('above the strongest raid'),
  within('within the range of the raids'),
  below('below the weakest raid');

  final String label;
  const RaidCover(this.label);

  static RaidCover of(double defence, proto.PowerRange raids) =>
      defence >= raids.most ? above : defence < raids.least ? below : within;
}

/// "LEVEL 2 OF 3 - BUILDING LEVEL 3, 0:40:00 LEFT": the level it stands at
/// and the one thing it is doing or waiting for.
String outpostStanding(proto.OutpostState o, int tick) {
  final work = o.work;
  final left = work == null ? '' : '${clockFmt(work.endTick - tick)} LEFT';
  final level = o.level == 0 ? 'NOT YET OPEN' : 'LEVEL ${o.level} OF ${o.maxLevel}';
  final doing = switch (work?.kind) {
    proto.OutpostWork.build => 'BUILDING LEVEL ${work!.level}, $left',
    proto.OutpostWork.repair => 'BREACHED, UNDER REPAIR, $left',
    null => o.breached ? 'BREACHED, THE REPAIR WAITS FOR ITS STORE' : 'IN SERVICE',
  };
  return '$level - ${o.breached && work?.kind == proto.OutpostWork.build ? 'BREACHED, ' : ''}$doing';
}

/// "OUTPOST 12,-3 L2 STORE 41%", with a breach or an announced raid after it.
String outpostHudLine(proto.OutpostState o, int tick) {
  final raid = o.raidArrivalTick;
  return [
    'OUTPOST ${o.location.q},${o.location.r} L${o.level} STORE ${(storeFill(o) * 100).round()}%',
    if (o.breached) 'BREACHED',
    if (raid != null) 'RAID ${clockFmt(raid - tick)}',
  ].join(' ');
}

/// A survey in three lines: the site and its yield, the raids against the
/// platform, the cost and the time. Of a sector the player has never seen
/// the server tells no site, and the first line says so.
List<String> surveyLines(proto.OutpostSurvey s) => [
      switch (s.site) {
        final site? => 'Site ${s.sector}, ring ${s.ring}: grade ${site.grade.toStringAsFixed(1)}, level 1 makes ${yieldAnHour(site.production)} an hour, '
            '${hopsHome(site.hopsHome)} from home',
        null => 'Site ${s.sector}, ring ${s.ring}: not on your chart, so its ore and its yield are not known',
      },
      'Raids of ${raidRange(s.raidPower)} against a platform of ${s.platformPower.round()}',
      'Level 1 costs ${amountsOf(s.cost)} from the founding hold and takes ${clockFmt(s.buildTicks)}',
    ];

/// What the survey says of founding there now, when the server refuses
/// nothing.
String surveyClear(proto.OutpostSurvey s) => 'F${s.fleetId} can found it now';

/// A survey as the log prints it: its three lines, then whether the site
/// can be founded now, in the server's sentence when it cannot.
List<String> surveyReport(proto.OutpostSurvey s) => [
      ...surveyLines(s),
      if (s.refusal case final why?) 'Cannot be founded now: $why' else surveyClear(s),
    ];
