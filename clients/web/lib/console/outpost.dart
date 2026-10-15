import 'dart:math' as math;

import '../protocol/protocol.dart' as proto;
import '../state/state_mapper.dart' show amountsOf;
import 'battle.dart' show platformList, raidClock, raidForce, raidShares, raidVerdict;
import 'intel.dart' show anHour, clockFmt, groupDigits, hopCount;

/// Whether [h] lies in the rings an outpost stands in, which are the
/// engine's. The server is the judge of a site; this only decides where the
/// console offers to found.
bool inOutpostRings(proto.Hex h) => h.distFromOrigin >= proto.outpostMinRing && h.distFromOrigin <= proto.outpostMaxRing;

/// Whether [h] lies in the rings a relay may stand in, which are the
/// engine's. The server is the judge of a site; this only decides where the
/// console offers to found.
bool inRelayRings(proto.Hex h) => h.distFromOrigin >= proto.relayMinRing;

/// What a relay does and does not do for a fleet standing at it.
const relayDockVoice = 'A FLEET FLOWN BY HAND IS REFUELLED HERE FROM THE STORE, AND LOADS AND UNLOADS DEUTERIUM BY ORDER. '
    'IT MAKES NOTHING, MENDS NOTHING AND NO RAID COMES TO IT. THE FUEL IN IT IS WHAT YOU HAULED THERE.';

/// "RELAY 12,-3 STORE 640 OF 1000 GONE IN 4:10:00".
String relayLine(proto.RelayState r) =>
    'RELAY ${r.sector.q},${r.sector.r} STORE ${r.store.round()} OF ${r.storeCap.round()} GONE IN ${clockFmt(r.decayIn)}';

/// A relay survey in its lines: the ring and the cost, the store and the
/// span it stands unvisited, and how many the player keeps of how many the
/// technology allows.
List<String> relaySurveyLines(proto.RelaySurvey s) => [
      'Relay site ${s.sector}, ring ${s.ring}: costs ${amountsOf(s.cost)} from the founding hold',
      'It holds ${s.storeCap.round()} deuterium and is gone with it after ${clockFmt(s.decayTicks)} with none of your fleets there',
      'You keep ${s.count} of ${s.limit} relays, raised by ${s.technology.label}',
    ];

/// A relay survey as the log prints it, ending with whether it can be
/// founded now, in the server's sentence when it cannot.
List<String> relaySurveyReport(proto.RelaySurvey s) => [
      ...relaySurveyLines(s),
      if (s.refusal case final why?) 'Cannot be founded now: $why' else 'F${s.fleetId} can found it now',
    ];

/// What the outpost does and does not do for a fleet standing at it.
String outpostDockVoice(proto.OutpostState o) =>
    '${o.inService ? 'A FLEET FLOWN BY HAND IS REFUELLED HERE FROM THE STORE, AND' : 'NO TANK IS FILLED HERE WHILE IT IS ${o.breached ? 'BREACHED' : 'NOT YET OPEN'}. A FLEET'} '
    'LOADS AND UNLOADS BY ORDER.\n'
    '${o.inService ? 'ITS DAMAGED SHIPS ARE MENDED FROM THE STORE, AT A PRICE; NO FLEET IS REFITTED HERE.\n' : ''}'
    'IT IS IN A RAID\'S REACH: A FLEET STANDING HERE WHEN A RAID ARRIVES IS IN ITS FIGHT, BESIDE THE PLATFORM.';

/// "77.9 Fe, 39.0 De" made in a game hour; "nothing" when [perTick] is zero.
String yieldAnHour(proto.Resources perTick) {
  final parts = [
    for (final (v, unit) in [(perTick.metal, 'Fe'), (perTick.crystal, 'Cr'), (perTick.deuterium, 'De')])
      if (v > 0) '${anHour(v)} $unit',
  ];
  return parts.isEmpty ? 'nothing' : parts.join(', ');
}

/// "7 hops", or what the server means by leaving it out.
String hopsHome(int? hops) => hops == null ? 'more than ${proto.laneSearchHops} hops' : hopCount(hops);

/// "7.5 OF 12": a site's grade against the most a grade can be.
String gradeOfMax(double grade) => '${grade.toStringAsFixed(1)} OF ${proto.siteGradeMax.toStringAsFixed(0)}';

/// "rings 9 to 20": where an outpost may stand.
const outpostRings = 'rings ${proto.outpostMinRing} to ${proto.outpostMaxRing}';

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

/// "RAID 2:50" for a raid on its way, "RAID FOUGHT" for one that has
/// arrived, from the outpost's forecast; null with neither.
String? outpostRaidMark(proto.OutpostState o, int tick) => switch (raidClock(o.raid, tick)) { null => null, final clock => 'RAID $clock' };

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

/// "L2", or what stands for a level while the first is being built.
String outpostLevelMark(proto.OutpostState o) => o.level == 0 ? 'NOT OPEN' : 'L${o.level}';

/// "OUTPOST 12,-3 L2 STORE 41%", with a breach or an announced raid after it.
String outpostHudLine(proto.OutpostState o, int tick) => [
      'OUTPOST ${o.location.q},${o.location.r} ${outpostLevelMark(o)} STORE ${(storeFill(o) * 100).round()}%',
      if (o.breached) 'BREACHED',
      ?outpostRaidMark(o, tick),
    ].join(' ');

/// A survey in its lines: the site and its yield, the platform of level 1
/// and the raids of the ring, the forecast of a typical one of them against
/// that platform alone, the cost and the time. Of a sector the player has
/// never seen the server tells no site, and the first line says so; outside
/// the rings an outpost stands in it forecasts no raid, and that line is
/// left out.
List<String> surveyLines(proto.OutpostSurvey s) => [
      switch (s.site) {
        final site? => 'Site ${s.sector}, ring ${s.ring}: grade ${site.grade.toStringAsFixed(1)}, level 1 makes ${yieldAnHour(site.production)} an hour, '
            '${hopsHome(site.hopsHome)} from home',
        null => 'Site ${s.sector}, ring ${s.ring}: not on your chart, so its ore and its yield are not known',
      },
      'Level 1 stands with a platform of ${platformList(s.platform)}; raids of this ring are of power ${raidRange(s.raidPower)}',
      if (s.raid case final raid?)
        'Against that platform alone ${raidForce(raid.ships, raid.power)} is ${raidVerdict(raid.odds)} (${raidShares(raid.odds)})',
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
