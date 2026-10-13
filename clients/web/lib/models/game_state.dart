import 'dart:ui';

import '../protocol/protocol.dart' as proto;
import 'fleet.dart';
import 'homeworld.dart';
import 'resources.dart';

/// One line of the event log.
class LogEntry {
  final int tick;
  final String message;
  final EventLevel level;

  const LogEntry({
    required this.tick,
    required this.message,
    required this.level,
  });
}

/// Each line of [log], newest first, with a name that stays its own while
/// newer lines arrive above it: of two lines alike, the older is `#0`.
List<(String, LogEntry)> namedLog(List<LogEntry> log) {
  final seen = <String, int>{};
  final named = [
    for (final e in log.reversed)
      ('${e.tick}|${e.message}#${seen.update('${e.tick}|${e.message}', (n) => n + 1, ifAbsent: () => 0)}', e),
  ];
  return named.reversed.toList();
}

/// One report from a fleet's standing orders, in the engine's words.
class PolicyAct {
  /// When the engine last said it. A hold is said again while it lasts, with
  /// fresh figures; [since] is when it was first said.
  final int tick;
  final int since;
  final proto.PolicyPreset preset;

  /// The engine's verb: `engage` confirms new orders, `clear` ends them,
  /// `hold` is the fleet standing still, the rest are what it did.
  final String action;
  final String reason;
  const PolicyAct({required this.tick, required this.since, required this.preset, required this.action, required this.reason});

  bool get isHold => action == 'hold';

  /// The fleet left a fight because its odds fell below the doctrine's.
  bool get isFlight => action == policyFlee;

  /// The verb as a report's heading says it.
  String get heading => isFlight ? 'left a fight' : action;
}

/// The engine's verb for a fleet under orders that jumps out of a fight.
const policyFlee = 'flee';

enum EventLevel {
  full(Color(0xFFFFB000)),
  bright(Color(0xD9FFB000)),
  normal(Color(0x99FFB000)),
  dim(Color(0x4DFFB000));

  final Color color;
  const EventLevel(this.color);
}

class Alert {
  final String icon;
  final String message;
  final String detail;
  final AlertTone level;

  const Alert({
    required this.icon,
    required this.message,
    this.detail = '',
    required this.level,
  });
}

enum AlertTone {
  glow(Color(0xFFFFB000)),
  bright(Color(0xD9FFB000)),
  normal(Color(0x99FFB000)),
  dim(Color(0x4DFFB000));

  final Color color;
  const AlertTone(this.color);
}

class Waypoint {
  final String id;
  final proto.Hex coord;
  final String note;

  const Waypoint({
    required this.id,
    required this.coord,
    required this.note,
  });
}

/// A `preview_move` answer and the tick it arrived.
class PreviewEntry {
  final proto.MovePreview preview;
  final int tick;
  const PreviewEntry(this.preview, this.tick);
}

/// The server's odds for one fleet in one sector and the tick they arrived.
class OddsEntry {
  final proto.SectorOdds odds;
  final int tick;
  const OddsEntry(this.odds, this.tick);
}

/// An `exchange_quote` answer and the tick it arrived.
class QuoteEntry {
  final proto.ExchangeQuote quote;
  final int tick;
  const QuoteEntry(this.quote, this.tick);
}

/// A trade at the exchange as the client names it: one quote is kept per key.
typedef TradeKey = (int fleetId, proto.ResourceKind give, proto.ResourceKind want);

/// An `outpost_survey` answer and the tick it arrived. The figures are the
/// site's own and do not age; the refusal is true of that tick.
class SurveyEntry {
  final proto.OutpostSurvey survey;
  final int tick;
  const SurveyEntry(this.survey, this.tick);
}

/// A fight the player has a fleet in or is present at, as the console
/// follows it: what the state says of it, the last round it was sent, and
/// the end once it has ended. The rounds before the last are not kept.
class Battle {
  final proto.Hex sector;

  /// Rounds fought so far, as the state counts them.
  final int rounds;

  /// The last round the server sent; null before the first is fought.
  final proto.BattleRoundEvent? round;

  /// What each fleet and group has lost so far, as the state lists it.
  final List<proto.BattleLosses> losses;

  /// How it ended and the tick it did; null while it goes on.
  final proto.CombatEndedEvent? ended;
  final int endedTick;

  /// The fight is a raid on a homeworld or an outpost: what the state says
  /// of it while it is fought, and the last of that once it has ended.
  final proto.RaidBattle? raid;

  /// What the raid came to, once it has ended: the player's own raids only.
  final proto.RaidResolvedEvent? resolved;

  const Battle({
    required this.sector,
    required this.rounds,
    this.round,
    this.losses = const [],
    this.ended,
    this.endedTick = 0,
    this.raid,
    this.resolved,
  });

  bool get over => ended != null;

  /// A raid's fight, as the state, its rounds or its end mark it.
  bool get isRaid => raid != null || (round?.raid ?? false) || (ended?.raid ?? false);

  /// This fight with what its raid came to.
  Battle resolvedAs(proto.RaidResolvedEvent how) => Battle(
      sector: sector, rounds: rounds, round: round, losses: losses, ended: ended, endedTick: endedTick, raid: raid, resolved: how);
}

/// A raid on the player's homeworld or outpost that has ended, and the tick
/// it did. The console keeps the last of each for as long as it is open.
class RaidReport {
  final proto.RaidResolvedEvent raid;
  final int tick;
  const RaidReport(this.raid, this.tick);
}

/// Ticks a fight's end stays on the round panel after its last round.
const battleReportTicks = 90;

/// Ticks a preview or odds stay on screen, and how soon they are asked for
/// again (the refresh lands well before they lapse, so the readout never
/// blinks out).
const previewFreshTicks = 8;
const previewRefreshTicks = 3;

class GameState {
  final int tick;
  final int clockSec;
  final Resources resources;
  final List<FleetState> fleets;
  final List<QueueItem> buildQueue;
  final List<QueueItem> shipyard;

  /// Every ship of every fleet the player has, as the server counts them.
  final int shipCount;
  final ResearchState research;
  final List<LogEntry> events;
  final List<Alert> alerts;
  final proto.Hex homeworld;
  final List<Waypoint> waypoints;

  /// Every sector the server has told us about, keyed by location.
  /// Absent = unexplored. Connections come from here (server-authoritative).
  final Map<proto.Hex, proto.SectorState> sectors;

  /// Faint scan contacts in not-yet-explored sectors.
  final Map<proto.Hex, proto.SignalKind> signals;

  /// Faint scan contacts' coarse danger (1: T1-3, 2: T4-6, 3: T7-9), by sector.
  final Map<proto.Hex, int> signalBands;

  /// The server's latest `preview_move` per (fleet id, target sector).
  final Map<(int, proto.Hex), PreviewEntry> previews;

  /// The server's latest `odds` answer per (fleet id, sector). The client
  /// never works a ratio out itself; read them with [oddsFor].
  final Map<(int, proto.Hex), OddsEntry> odds;

  /// The server's latest `exchange_quote` per (fleet id, give, want).
  final Map<TradeKey, QuoteEntry> quotes;

  /// Where the player's meter at the hub's exchange stands.
  final proto.ExchangeMeter exchange;

  /// Exact hold of each fleet by id (fleet cargo above is rounded for display).
  final Map<int, proto.Resources> holds;

  /// Exact stockpile (the resource stocks above are rounded for display).
  final proto.Resources stock;

  /// What the homeworld can build, research and launch; null before the
  /// first server message.
  final proto.HomeworldCatalog? catalog;

  /// The server's whole homeworld snapshot (storage, queues, defences);
  /// null before the first server message.
  final proto.HomeworldState? hw;

  /// Fixed world settings (pace); null before the first full state.
  final proto.WorldInfo? world;

  /// The last score table the server sent, if asked for.
  final proto.LeaderboardReply? leaderboard;

  /// The last raid on the homeworld that ended while the console was open,
  /// and the last on the outpost.
  final RaidReport? lastRaid;
  final RaidReport? lastOutpostRaid;

  /// The player's outpost as the server last sent it; null with none.
  final proto.OutpostState? outpost;

  /// The server's latest `outpost_survey` per (fleet id, sector).
  final Map<(int, proto.Hex), SurveyEntry> surveys;

  /// What each fleet's standing orders reported lately, newest first, by
  /// fleet id.
  final Map<int, List<PolicyAct>> policyActs;

  /// The fights under way that the player is in or present at, and those
  /// that ended in the last [battleReportTicks], by sector.
  final Map<proto.Hex, Battle> battles;

  const GameState({
    required this.tick,
    required this.clockSec,
    required this.resources,
    required this.fleets,
    required this.buildQueue,
    required this.shipyard,
    this.shipCount = 0,
    required this.research,
    required this.events,
    required this.alerts,
    required this.homeworld,
    required this.waypoints,
    this.sectors = const {},
    this.signals = const {},
    this.signalBands = const {},
    this.previews = const {},
    this.odds = const {},
    this.quotes = const {},
    this.exchange = const proto.ExchangeMeter(),
    this.holds = const {},
    this.stock = const proto.Resources(),
    this.catalog,
    this.hw,
    this.world,
    this.leaderboard,
    this.lastRaid,
    this.lastOutpostRaid,
    this.outpost,
    this.surveys = const {},
    this.policyActs = const {},
    this.battles = const {},
  });

  /// The fight in [sector], under way or just ended.
  Battle? battleAt(proto.Hex sector) => battles[sector];

  /// The raid on the homeworld as a fight, while it is fought and for
  /// [battleReportTicks] after; null when there is none.
  Battle? get raidAtHome => _raidAt(homeworld);

  /// The same for the player's outpost.
  Battle? get raidAtOutpost => switch (outpost) { final post? => _raidAt(post.location), null => null };

  Battle? _raidAt(proto.Hex sector) => switch (battles[sector]) { final fight? when fight.isRaid => fight, _ => null };

  /// The fight [fleet] has to be told of: the one where it stands, or else
  /// the one it has left, once that has ended.
  Battle? battleOf(FleetState fleet) {
    final here = battles[fleet.sector];
    final left = battles[fleet.foughtAt];
    if (left == null || !left.over || (here != null && (!here.over || here.endedTick >= left.endedTick))) return here;
    return left;
  }

  /// The survey of [sector] for [fleetId] as the server last answered it.
  SurveyEntry? surveyOf(int fleetId, proto.Hex sector) => surveys[(fleetId, sector)];

  bool surveyNeedsRefresh(int fleetId, proto.Hex sector) {
    final e = surveys[(fleetId, sector)];
    return e == null || tick - e.tick >= previewRefreshTicks;
  }

  /// Why [fleet] stands still under its orders: the hold they last
  /// reported, unless they have acted since or been cleared.
  PolicyAct? holdOf(FleetState fleet) {
    final last = policyActs[fleet.id]?.firstOrNull;
    return fleet.policy != null && last != null && last.isHold ? last : null;
  }

  /// Whether [at] is a hub sector that offers the exchange.
  bool exchangeAt(proto.Hex at) => world?.servicesAt(at).contains(proto.HubService.exchange) ?? false;

  /// The quote for a trade if it is recent enough to show.
  proto.ExchangeQuote? quoteFor(int fleetId, proto.ResourceKind give, proto.ResourceKind want) {
    final e = quotes[(fleetId, give, want)];
    return e != null && tick - e.tick <= previewFreshTicks ? e.quote : null;
  }

  bool quoteNeedsRefresh(int fleetId, proto.ResourceKind give, proto.ResourceKind want) {
    final e = quotes[(fleetId, give, want)];
    return e == null || tick - e.tick >= previewRefreshTicks;
  }

  /// The order the slot is working on; waiting orders follow it in the list
  /// but are not running, so `buildQueue.first` is not "the running order".
  QueueItem? get runningBuild => buildQueue.where((q) => q.active).firstOrNull;
  QueueItem? get runningShip => shipyard.where((q) => q.active).firstOrNull;
  List<QueueItem> get waitingBuild => [for (final q in buildQueue) if (!q.active) q];
  List<QueueItem> get waitingShips => [for (final q in shipyard) if (!q.active) q];

  /// The preview for (fleet, target) if it is recent enough to show.
  proto.MovePreview? previewFor(int fleetId, proto.Hex target) {
    final e = previews[(fleetId, target)];
    return e != null && tick - e.tick <= previewFreshTicks ? e.preview : null;
  }

  bool previewNeedsRefresh(int fleetId, proto.Hex target) {
    final e = previews[(fleetId, target)];
    return e == null || tick - e.tick >= previewRefreshTicks;
  }

  /// The server's odds for (fleet, sector) if recent enough to show: the
  /// newer of an `odds` answer and a preview of that jump.
  proto.SectorOdds? oddsFor(int fleetId, proto.Hex sector) {
    final e = _latestOdds(fleetId, sector);
    return e != null && tick - e.tick <= previewFreshTicks ? e.odds : null;
  }

  bool oddsNeedRefresh(int fleetId, proto.Hex sector) {
    final e = _latestOdds(fleetId, sector);
    return e == null || tick - e.tick >= previewRefreshTicks;
  }

  OddsEntry? _latestOdds(int fleetId, proto.Hex sector) {
    final asked = odds[(fleetId, sector)];
    final jump = previews[(fleetId, sector)];
    if (jump == null || (asked != null && asked.tick >= jump.tick)) return asked;
    return OddsEntry(jump.preview.odds, jump.tick);
  }

  String get clockDisplay {
    final h = clockSec ~/ 3600;
    final m = (clockSec % 3600) ~/ 60;
    final s = clockSec % 60;
    return '${h.toString().padLeft(2, '0')}:'
        '${m.toString().padLeft(2, '0')}:'
        '${s.toString().padLeft(2, '0')}';
  }

  GameState copyWith({
    int? tick,
    int? clockSec,
    Resources? resources,
    List<FleetState>? fleets,
    List<QueueItem>? buildQueue,
    List<QueueItem>? shipyard,
    int? shipCount,
    ResearchState? research,
    List<LogEntry>? events,
    List<Alert>? alerts,
    proto.Hex? homeworld,
    List<Waypoint>? waypoints,
    Map<proto.Hex, proto.SectorState>? sectors,
    Map<proto.Hex, proto.SignalKind>? signals,
    Map<proto.Hex, int>? signalBands,
    Map<(int, proto.Hex), PreviewEntry>? previews,
    Map<(int, proto.Hex), OddsEntry>? odds,
    Map<TradeKey, QuoteEntry>? quotes,
    proto.ExchangeMeter? exchange,
    Map<int, proto.Resources>? holds,
    proto.Resources? stock,
    proto.HomeworldCatalog? catalog,
    proto.HomeworldState? hw,
    proto.WorldInfo? world,
    proto.LeaderboardReply? leaderboard,
    RaidReport? lastRaid,
    RaidReport? lastOutpostRaid,
    proto.OutpostState? outpost,
    Map<(int, proto.Hex), SurveyEntry>? surveys,
    Map<int, List<PolicyAct>>? policyActs,
    Map<proto.Hex, Battle>? battles,
  }) =>
      GameState(
        tick: tick ?? this.tick,
        clockSec: clockSec ?? this.clockSec,
        resources: resources ?? this.resources,
        fleets: fleets ?? this.fleets,
        buildQueue: buildQueue ?? this.buildQueue,
        shipyard: shipyard ?? this.shipyard,
        shipCount: shipCount ?? this.shipCount,
        research: research ?? this.research,
        events: events ?? this.events,
        alerts: alerts ?? this.alerts,
        homeworld: homeworld ?? this.homeworld,
        waypoints: waypoints ?? this.waypoints,
        sectors: sectors ?? this.sectors,
        signals: signals ?? this.signals,
        signalBands: signalBands ?? this.signalBands,
        previews: previews ?? this.previews,
        odds: odds ?? this.odds,
        quotes: quotes ?? this.quotes,
        exchange: exchange ?? this.exchange,
        holds: holds ?? this.holds,
        stock: stock ?? this.stock,
        catalog: catalog ?? this.catalog,
        hw: hw ?? this.hw,
        world: world ?? this.world,
        leaderboard: leaderboard ?? this.leaderboard,
        lastRaid: lastRaid ?? this.lastRaid,
        lastOutpostRaid: lastOutpostRaid ?? this.lastOutpostRaid,
        outpost: outpost ?? this.outpost,
        surveys: surveys ?? this.surveys,
        policyActs: policyActs ?? this.policyActs,
        battles: battles ?? this.battles,
      );
}
