import 'dart:math' as math;

import 'package:clock/clock.dart';

import '../models/fleet.dart';
import '../models/game_state.dart';
import '../models/homeworld.dart';
import '../console/battle.dart'
    show endedLine, pursuitEventLine, raidForce, raidHeadline, raidSummary, raidVerdict, roundLine, shipsList, ticksOf;
import '../console/intel.dart' show clockFmt, groupDigits, relicPct;
import '../models/resources.dart';
import '../protocol/protocol.dart' as proto;
import 'command_parser.dart' show QueueIds;

/// "1,234 Fe, 56 Cr": the parts of [r] that [whole] does not make zero, in
/// the units the stockpile counters carry.
String _amounts(proto.Resources r, int Function(double) whole) => [
      for (final (amount, unit) in [(whole(r.metal), 'Fe'), (whole(r.crystal), 'Cr'), (whole(r.deuterium), 'De')])
        if (amount > 0) '${groupDigits(amount)} $unit',
    ].join(', ');

/// "90 Fe, 22 Cr" as the console prints an amount paid, hauled, held or
/// lost; "nothing" for none.
String amountsOf(proto.Resources r) {
  final amounts = _amounts(r, (v) => v.round());
  return amounts.isEmpty ? 'nothing' : amounts;
}

/// "F84 unloaded into the stockpile: 4,312 Fe, 120 De": a `Stockpile` event
/// as the log and the alerts print it.
String stockpileLine(proto.StockpileEvent e) => 'F${e.fleetId} ${e.cause.phrase}: ${amountsOf(e.amount)}';

/// "short by 1,234 Fe, 56 Cr": the parts of [missing] above zero, each
/// rounded up. Every shortfall the console prints is this phrase; [opening]
/// starts a caption with it.
String shortBy(proto.Resources missing, {bool opening = false}) =>
    '${opening ? 'Short' : 'short'} by ${_amounts(missing, (v) => v.ceil())}';

/// "short by 35 Fe, 4 Cr, starts in about 0:40": why a queued order has not
/// started, the exact shortfall and the estimate. An order short of
/// something the homeworld does not produce has no estimate, and waiting
/// will not start it: the summary says so.
String waitSummary(proto.WaitReason on, proto.Resources? short, int? startIn) {
  final named = short != null && (short.metal > 0 || short.crystal > 0 || short.deuterium > 0);
  if (on == proto.WaitReason.resources && named && startIn == null) return '${shortBy(short)}, which home production will not cover';
  final why = switch (on) {
    proto.WaitReason.resources => named ? shortBy(short) : 'waiting for resources',
    proto.WaitReason.slot => 'waiting for a free slot',
    proto.WaitReason.prerequisite => 'waiting for a prerequisite',
    proto.WaitReason.order => 'held by a reserved order',
  };
  return _withStart(why, startIn);
}

/// "paid, waiting for the yard, starts in about 0:40": a shipyard batch is
/// paid for when it is ordered, so it only ever waits its turn.
String batchWaitSummary(proto.BatchWait on, int? startIn) => _withStart(
    switch (on) {
      proto.BatchWait.slot => 'paid, waiting for the yard',
      proto.BatchWait.prerequisite => 'paid, waiting for a prerequisite',
    },
    startIn);

String _withStart(String why, int? startIn) => startIn == null ? why : '$why, starts in about ${clockFmt(startIn)}';

/// "meter 632 of 4185, next unit at 33%, clear in 38:41": where a player
/// stands at the exchange.
String meterSummary(proto.ExchangeMeter m) {
  final clear = switch (m.clearIn) {
    null => ', does not fall',
    0 => '',
    final t => ', clear in ${clockFmt(t)}',
  };
  return 'meter ${m.pressure.round()} of ${m.span.round()}, next unit at ${(m.fee * 100).round()}%$clear';
}

/// A quote as the log prints it: the trade, the meter it leaves, and why
/// it would be refused, if it would.
List<String> quoteLines(proto.ExchangeQuote q) {
  final t = q.terms;
  return [
    'Quote F${t.fleetId}: ${t.amount.round()} ${t.give.name} gives ${t.receive.round()} ${t.want.name} '
        '(par ${t.par.round()}, fee ${(t.fee * 100).round()}%)',
    'After it: ${meterSummary(t.meterAfter)}',
    if (q.refusal != null) 'Cannot trade now: ${q.refusal}',
  ];
}

/// The board as the log prints it for a player who asked: the first five
/// ranks, the player's own row, and the best of the unranked bots.
List<String> boardLines(proto.LeaderboardReply b) => [
      for (final e in [...b.entries.take(5), if (b.you != null) b.you!])
        '#${e.rank} ${e.name}${e.agent ? ' [AI]' : ''}  ${e.score.toStringAsFixed(1)} '
            '(buildings ${e.buildings.toStringAsFixed(1)}, research ${e.research.toStringAsFixed(1)}, '
            'fleet ${e.fleet.toStringAsFixed(1)}${e.outpost > 0 ? ', outpost ${e.outpost.toStringAsFixed(1)}' : ''}'
            '${e.defence > 0 ? ', defence ${e.defence.toStringAsFixed(1)}' : ''})',
      if (b.bots.isNotEmpty) '${b.bots.length} built-in bots, unranked; best ${b.bots.first.name} ${b.bots.first.score.toStringAsFixed(1)}',
    ];

/// Holds the latest protocol snapshots (full_state + tick_update deltas) and
/// maps them to the amber UI presentation models.
class StateMapper {
  int tick = 0;
  DateTime? connectedAt;

  /// What feeds this state, as the quiet alert names it.
  String feed = 'Live server feed';

  proto.PlayerState? player;
  List<proto.FleetState> fleets = [];
  proto.HomeworldState? homeworld;
  proto.WorldInfo? world;
  proto.LeaderboardReply? leaderboard;

  /// The player's outpost as the server last sent it; null with none.
  proto.OutpostState? outpost;

  /// `outpost_survey` answers by (fleet id, sector key).
  final Map<(int, int), SurveyEntry> surveys = {};

  /// The player's relays as the server last sent them.
  proto.RelaysState relays = const proto.RelaysState();

  /// The roaming hubs the player knows, as the server last sent them.
  List<proto.HubState> hubs = const [];

  /// `relay_survey` answers by (fleet id, sector key).
  final Map<(int, int), RelaySurveyEntry> relaySurveys = {};

  /// The last raid on the homeworld that ended while the console was open,
  /// and the last on the outpost. The raid on its way or being fought is
  /// the state's: [homeworld] and [outpost] carry it.
  RaidReport? lastRaid;
  RaidReport? lastOutpostRaid;

  /// Known sectors keyed by [proto.Hex.toKey].
  final Map<int, proto.SectorState> sectors = {};

  /// Faint scan contacts for sectors we have no data on yet.
  final Map<int, proto.SignalKind> signals = {};

  /// Coarse danger band of each faint contact, from the scan that heard it.
  final Map<int, int> signalBands = {};

  /// `preview_move` answers by (fleet id, target key).
  final Map<(int, int), PreviewEntry> previews = {};

  /// `odds` answers by (fleet id, sector key).
  final Map<(int, int), OddsEntry> odds = {};

  /// `exchange_quote` answers by (fleet id, give, want).
  final Map<TradeKey, QuoteEntry> quotes = {};

  /// Lines the log keeps, newest first. The Overview's log scrolls through
  /// all of them.
  static const logLines = 120;

  final List<LogEntry> log = [];

  /// Reports a fleet's standing orders keep for the orders panel.
  static const policyActsKept = 6;

  /// What each fleet's standing orders reported lately, newest first.
  final Map<int, List<PolicyAct>> policyActs = {};

  /// The fights the player is in or present at and those just ended, by
  /// sector key. The state lists the ones under way; the round each stands
  /// at is the last `BattleRound` event, or a full state's `last_round`.
  final Map<int, Battle> battles = {};

  /// The last fights that ended, newest first, each with the rounds this
  /// console was sent of it: what the battle view steps through after a
  /// fight. The server keeps none of it, so a console opened anew has none.
  final List<Battle> pastFights = [];

  /// The log's line for the last round of each fight under way, by sector
  /// key: a fight keeps one such line, which each round replaces.
  final Map<int, LogEntry> _roundLines = {};

  /// Where each fleet last stood in a fight, by fleet id: a fleet that
  /// jumped out of one is shown that fight's end where it lands.
  final Map<int, proto.Hex> _foughtAt = {};
  final List<Alert> alerts = [];

  bool get hasState => player != null;

  void reset() {
    tick = 0;
    connectedAt = null;
    player = null;
    fleets = [];
    homeworld = null;
    world = null;
    leaderboard = null;
    outpost = null;
    surveys.clear();
    relays = const proto.RelaysState();
    hubs = const [];
    relaySurveys.clear();
    lastRaid = null;
    lastOutpostRaid = null;
    sectors.clear();
    signals.clear();
    signalBands.clear();
    policyActs.clear();
    battles.clear();
    pastFights.clear();
    _roundLines.clear();
    _foughtAt.clear();
    previews.clear();
    odds.clear();
    quotes.clear();
    log.clear();
    alerts.clear();
  }

  /// Apply any server message. Returns normally for every variant.
  void apply(proto.ServerMessage msg) {
    switch (msg) {
      case proto.GameState():
        _applyFull(msg);
      case proto.TickUpdate():
        _applyTick(msg);
      case proto.EventMessage(:final event):
        _ingestEvent(event);
      case proto.ErrorMessage():
        _applyError(msg);
      case proto.MovePreview():
        _applyPreview(msg);
      case proto.FleetOdds():
        for (final o in msg.sectors) {
          odds[(msg.fleetId, o.sector.toKey())] = OddsEntry(o, tick);
        }
        odds.removeWhere((_, e) => tick - e.tick > 60);
      case proto.ExchangeQuote():
        final t = msg.terms;
        quotes[(t.fleetId, t.give, t.want)] = QuoteEntry(msg, tick);
        quotes.removeWhere((_, e) => tick - e.tick > 60);
      case proto.LeaderboardReply():
        leaderboard = msg;
      case proto.OutpostSurvey():
        surveys[(msg.fleetId, msg.sector.toKey())] = SurveyEntry(msg, tick);
        surveys.removeWhere((_, e) => tick - e.tick > 600);
      case proto.RelaySurvey():
        relaySurveys[(msg.fleetId, msg.sector.toKey())] = RelaySurveyEntry(msg, tick);
        relaySurveys.removeWhere((_, e) => tick - e.tick > 600);
      case proto.AuthResult():
        break; // handled by the controller (connection state)
    }
  }

  void _applyFull(proto.GameState s) {
    final first = connectedAt == null;
    tick = s.tick;
    connectedAt = clock.now();
    player = s.player;
    fleets = List.of(s.fleets);
    homeworld = s.homeworld;
    outpost = s.outpost;
    relays = s.relays;
    hubs = s.hubs;
    world = s.world;
    sectors.clear();
    for (final sec in s.knownSectors) {
      sectors[sec.location.toKey()] = sec;
    }
    signals.removeWhere((k, _) => sectors.containsKey(k));
    signalBands.removeWhere((k, _) => sectors.containsKey(k));
    final followed = {
      for (final MapEntry(:key, value: b) in battles.entries)
        if (!b.over) key: b,
    };
    battles
      ..removeWhere((_, b) => !b.over)
      ..addAll({for (final b in s.battles) b.sector.toKey(): _resumed(followed[b.sector.toKey()], b, s.tick)});
    previews.clear();
    odds.clear();
    quotes.clear();
    surveys.clear();
    relaySurveys.clear();
    _noteFights();
    pushLog(first ? 'Uplink established. Homeworld at ${s.player.homeworld}.' : 'Uplink refreshed: the console is in step with the world again.',
        EventLevel.bright);
  }

  void _applyTick(proto.TickUpdate u) {
    tick = u.tick;
    if (u.player != null) player = u.player;
    fleets = List.of(u.fleets);
    if (u.homeworldUpdate != null) homeworld = u.homeworldUpdate;
    outpost = u.outpost;
    relays = u.relays;
    hubs = u.hubs;
    for (final sec in u.sectorUpdates ?? const <proto.SectorState>[]) {
      final key = sec.location.toKey();
      sectors[key] = sec;
      signals.remove(key);
      signalBands.remove(key);
    }
    for (final e in u.events ?? const <proto.GameEvent>[]) {
      _ingestEvent(e);
    }
    final live = {for (final b in u.battles) b.sector.toKey()};
    battles.removeWhere((key, b) => b.over ? tick - b.endedTick > battleReportTicks : !live.contains(key));
    for (final b in u.battles) {
      final known = battles[b.sector.toKey()];
      battles[b.sector.toKey()] =
          known == null || known.over ? Battle(sector: b.sector, since: tick, rounds: b.rounds, losses: b.losses, raid: b.raid) : known.listed(b);
    }
    policyActs.removeWhere((id, _) => fleets.every((f) => f.id != id));
    _noteFights();
  }

  /// A fight a full state lists. The console goes on with the rounds it
  /// holds of it when the state's last round is the last of them; else it
  /// starts from the one round the state carries, the only one the server
  /// has.
  Battle _resumed(Battle? known, proto.BattleState b, int at) {
    if (known != null && known.round?.round == b.lastRound?.round) return known.listed(b);
    return Battle(
        sector: b.sector, since: at, rounds: b.rounds, round: b.lastRound, history: [?b.lastRound], losses: b.losses, raid: b.raid);
  }

  /// Hold a fight that has ended among the last [Battle.fightsKept].
  void _keep(Battle ended) {
    pastFights
      ..removeWhere((b) => b.id == ended.id)
      ..insert(0, ended);
    if (pastFights.length > Battle.fightsKept) pastFights.removeRange(Battle.fightsKept, pastFights.length);
  }

  void _noteFights() {
    _foughtAt.removeWhere((id, at) => fleets.every((f) => f.id != id) || !battles.containsKey(at.toKey()));
    for (final f in fleets) {
      if (f.state == proto.FleetStatus.inCombat || f.fleeing != null) _foughtAt[f.id] = f.location;
    }
  }

  void _applyPreview(proto.MovePreview p) {
    previews[(p.fleetId, p.target.toKey())] = PreviewEntry(p, tick);
    previews.removeWhere((_, e) => tick - e.tick > 60);
  }

  void _applyError(proto.ErrorMessage err) {
    pushLog('Refused: ${err.message}', EventLevel.bright);
    _pushAlert(Alert(icon: '!', message: err.message, detail: 'Order refused at tick $tick', level: AlertTone.bright));
  }

  /// The server just issued this account's token. It is shown once: the
  /// guide shows it with a copy button and the alert keeps it for the
  /// session; [saved] says whether the browser kept it.
  void noticeToken(String token, {required bool saved}) {
    pushLog('New account: save the account token (it is your password; see Alerts)', EventLevel.bright);
    _pushAlert(Alert(
      icon: '!',
      message: 'New account token',
      detail: saved ? token : '$token  (NOT stored in this browser: copy it now)',
      level: AlertTone.glow,
    ));
  }

  void pushLog(String msg, [EventLevel level = EventLevel.normal]) => _log(LogEntry(tick: tick, message: msg, level: level));

  void _log(LogEntry entry) {
    log.insert(0, entry);
    if (log.length > logLines) log.removeRange(logLines, log.length);
  }

  void _pushAlert(Alert a) {
    alerts.insert(0, a);
    if (alerts.length > 8) alerts.removeRange(8, alerts.length);
  }

  String _fleet(int id) => 'F$id';

  void _ingestEvent(proto.GameEvent e) {
    final k = e.kind;
    String? msg;
    var level = EventLevel.normal;

    switch (k) {
      case proto.BattleRoundKind(battleRound: final round):
        final key = round.sector.toKey();
        final known = battles[key];
        battles[key] = (known == null || known.over ? Battle(sector: round.sector, since: e.tick, rounds: 0) : known).fought(round);
        if (!round.mine) return;
        log.remove(_roundLines.remove(key));
        final line = LogEntry(tick: e.tick, message: roundLine(round), level: EventLevel.dim);
        _roundLines[key] = line;
        _log(line);
        return;
      case proto.ShipsLostEvent():
        msg = k.mine
            ? '! ${_fleet(k.fleetId)} lost ${shipsList(k.ships)} breaking up in the emergency jump from ${k.sector}'
            : "${k.owner}'s fleet ${_fleet(k.fleetId)} lost ${shipsList(k.ships)} in an emergency jump from ${k.sector}";
        level = k.mine ? EventLevel.bright : EventLevel.dim;
      case proto.FleetDestroyedEvent():
        if (k.mine) {
          msg = '!! YOUR FLEET ${_fleet(k.fleetId)} DESTROYED in ${k.sector}';
          level = EventLevel.bright;
        } else if (k.isNpc) {
          msg = 'Hostile ${k.label.isEmpty ? 'fleet ${k.fleetId}' : k.label} destroyed in ${k.sector} -- wreckage ${amountsOf(k.salvage)}';
        } else {
          msg = "${k.owner}'s fleet ${_fleet(k.fleetId)} destroyed in ${k.sector}";
          level = EventLevel.dim;
        }
      case proto.ResourceHarvestedEvent():
        msg = '${_fleet(k.fleetId)} harvested ${amountsOf(k.resources)} over ${k.ticks}s';
      case proto.SectorEnteredEvent():
        msg = '${_fleet(k.fleetId)} entered ${k.sector}${k.firstVisit ? ' (first visit)' : ''}';
      case proto.CombatStartedEvent():
        msg = '! Combat in ${k.sector}: ${k.mine ? 'your' : "${k.owner}'s"} ${_fleet(k.playerFleetId)} vs hostile ${k.enemy.isEmpty ? 'fleet ${k.enemyFleetId}' : k.enemy}';
        level = k.mine ? EventLevel.bright : EventLevel.dim;
      case proto.CombatEndedEvent():
        final key = k.sector.toKey();
        _roundLines.remove(key);
        final fought = battles[key];
        final ended = (fought == null || fought.over ? Battle(sector: k.sector, since: e.tick, rounds: k.rounds) : fought).endedAs(k, e.tick);
        battles[key] = ended;
        _keep(ended);
        if (k.raid && k.mine) return;
        msg = '${k.mine ? '! ' : ''}${endedLine(k)}';
        level = k.mine ? EventLevel.bright : EventLevel.dim;
      case proto.SalvageCollectedEvent():
        msg = '${_fleet(k.fleetId)} collected salvage ${amountsOf(k.resources)}'
            '${k.remaining == null ? '' : ", ${amountsOf(k.remaining!)} left behind"}';
      case proto.StockpileEvent():
        msg = stockpileLine(k);
        if (k.cause != proto.StockpileCause.loaded) {
          _pushAlert(Alert(icon: '+', message: msg, detail: '', level: AlertTone.dim));
        }
      case proto.SalvageDespawnedEvent():
        msg = 'Wreckage in ${k.sector} drifted away (${amountsOf(k.resources)})';
        level = EventLevel.dim;
      case proto.StorageNearCapEvent():
        final eta = k.fullInS == null ? '' : ', full in ${_fmtTicks(k.fullInS!)}';
        msg = '! ${k.resource.name} storage at ${(k.ratio * 100).round()}%$eta';
        level = EventLevel.bright;
        _pushAlert(Alert(icon: '!', message: '${k.resource.name} storage nearly full', detail: eta.isEmpty ? '' : eta.substring(2), level: AlertTone.bright));
      case proto.StorageFullEvent():
        msg = '!! ${k.resource.name} storage FULL: production is wasted';
        level = EventLevel.bright;
        _pushAlert(Alert(icon: '!', message: '${k.resource.name} storage full', detail: 'Spend it or raise the ${k.resource.storage}', level: AlertTone.glow));
      case proto.FleetArrivedEvent():
        msg = '${_fleet(k.fleetId)} arrived at ${k.sector}';
      case proto.BuildingCompletedEvent():
        msg = 'Building complete: ${k.buildingType.label} Lv.${k.newLevel}';
        level = EventLevel.bright;
      case proto.ResearchCompletedEvent():
        msg = 'Research complete: ${k.tech.label} Lv.${k.newLevel}';
        level = EventLevel.bright;
      case proto.ShipBuiltEvent():
        msg = 'Shipyard: ${k.count}x ${k.shipClass.label} ready';
        level = EventLevel.bright;
      case proto.DefenceBuiltEvent():
        msg = 'Defence online: ${k.defence.label}';
        level = EventLevel.bright;
      case proto.QueueEvent():
        msg = switch (k.action) {
          proto.QueueAction.started => k.paid.metal + k.paid.crystal + k.paid.deuterium > 0
              ? '${k.item} started, paid ${amountsOf(k.paid)}'
              : '${k.item} started',
          proto.QueueAction.waiting => k.paid.metal + k.paid.crystal + k.paid.deuterium > 0
              ? '${k.item} ordered, paid ${amountsOf(k.paid)}, ${waitSummary(k.waitingOn ?? proto.WaitReason.slot, null, k.startIn)}'
              : '${k.item} queued, ${waitSummary(k.waitingOn ?? proto.WaitReason.slot, k.waitingFor, k.startIn)}',
          proto.QueueAction.cancelled => k.refunded.metal + k.refunded.crystal + k.refunded.deuterium > 0
              ? '${k.item} cancelled, refunded ${amountsOf(k.refunded)}'
              : '${k.item} cancelled (nothing had been paid)',
        };
      case proto.ScanCompletedEvent():
        msg = '${_fleet(k.fleetId)} scan at ${k.sector}: ${k.sectorsRevealed} sectors revealed, '
            '${k.hostilesDetected} hostiles, worst T${k.threats.fold<int>(0, (m, t) => math.max(m, t.threat.rating))}'
            '${k.signals.isEmpty ? '' : ', ${k.signals.length} faint contacts (threat band ${k.signals.map((s) => s.threatBand).join('/')})'}';
        for (final s in k.signals) {
          final key = s.sector.toKey();
          if (!sectors.containsKey(key)) {
            signals[key] = s.signal;
            signalBands[key] = s.threatBand;
          }
        }
      case proto.OutpostCompletedEvent():
        msg = 'Outpost at ${k.sector}: level ${k.level} complete';
        level = EventLevel.bright;
      case proto.RelayFoundedEvent():
        msg = 'Relay founded at ${k.sector}, ring ${k.ring}';
        level = EventLevel.bright;
      case proto.RelayDecayedEvent():
        msg = '! Relay at ${k.sector} stood unvisited and is gone, with ${k.lost.round()} deuterium';
        level = EventLevel.bright;
      case proto.HubArrivedEvent():
        msg = 'Hub ${k.id + 1} sighted at ${k.sector}, ring ${k.ring}: it leaves in ${clockFmt(math.max(0, k.departsAt - tick))}';
        level = EventLevel.bright;
      case proto.HubDepartingEvent():
        msg = '! Hub ${k.id + 1} at ${k.sector} departs in ${clockFmt(math.max(0, k.departsAt - tick))}';
        level = EventLevel.bright;
      case proto.RelicFoundEvent():
        msg = '${_fleet(k.fleetId)} found a ${k.relicKind.label} relic of ${relicPct(k.pct)} at ${k.sector}'
            '${k.carried ? ': it is claimed when the fleet docks at home, an outpost or a relay' : ', and left it for the stronger relic aboard'}';
        level = EventLevel.bright;
      case proto.RelicClaimedEvent():
        msg = '${_fleet(k.fleetId)} claimed a ${k.relicKind.label} relic of ${relicPct(k.pct)}'
            '${k.applied ? ', fading by tick ${k.expiresAt}' : ': a stronger one of the kind is already held'}';
        level = EventLevel.bright;
      case proto.RelicFadedEvent():
        msg = 'The ${k.relicKind.label} relic has faded';
      case proto.RaidIncomingEvent():
        final where = switch (k.outpost) { null => '', final at => ' on your outpost at $at' };
        final forecast = switch (k.odds) { null => '', final odds => ', forecast ${raidVerdict(odds)}' };
        final left = math.max(0, k.arrivalTick - tick);
        msg = '! RAID INCOMING$where: ${raidForce(k.ships, k.estPower)}$forecast -- arrives in ${clockFmt(left)}, at tick ${k.arrivalTick}';
        level = EventLevel.bright;
        _pushAlert(Alert(
          icon: '!',
          message: 'Raid incoming$where: ${shipsList(k.ships)}${switch (k.odds) { null => '', final odds => ', forecast ${odds.label.label}' }}',
          detail: 'Arrives tick ${k.arrivalTick} -- ${left}s',
          level: AlertTone.glow,
        ));
      case proto.RaidResolvedEvent():
        final report = RaidReport(k, e.tick);
        if (k.outpost == null) {
          lastRaid = report;
        } else {
          lastOutpostRaid = report;
        }
        final key = (k.outpost ?? player?.homeworld)?.toKey();
        if (battles[key] case final fought? when fought.over && fought.endedTick == e.tick) {
          battles[key!] = fought.resolvedAs(k);
          _keep(battles[key]!);
        }
        _log(LogEntry(tick: e.tick, message: '${raidSummary(k, tick: e.tick).join('. ')}.', level: EventLevel.normal));
        msg = '! ${raidHeadline(k)}';
        level = EventLevel.bright;
      case proto.SiteExplorationStartedEvent():
        msg = '${_fleet(k.fleetId)} boarding derelict (tier ${k.tier}) in ${k.sector}, done tick ${k.endTick}';
      case proto.SiteExploredEvent():
        msg = '${_fleet(k.fleetId)} derelict yielded ${amountsOf(k.resources)}'
            '${k.recoveredShip != null ? ', recovered ${k.recoveredShip!.label}' : ''}'
            '${k.techCache != null ? ', data core: ${k.techCache!.label}' : ''}'
            '${k.relic ? ', an ancient relic among it' : ''}';
        level = EventLevel.bright;
      case proto.ChartDeliveredEvent():
        msg = '${_fleet(k.fleetId)} delivered the chart of ${k.sectors} sectors';
        level = EventLevel.bright;
      case proto.ExchangedEvent():
        final t = k.terms;
        quotes.clear();
        msg = '${_fleet(t.fleetId)} traded ${t.amount.round()} ${t.give.name} for ${t.receive.round()} ${t.want.name} '
            'at the exchange (fee ${(t.fee * 100).round()}%)';
        level = EventLevel.bright;
      case proto.SiteAmbushEvent():
        msg = '! ${_fleet(k.fleetId)} ambushed aboard derelict in ${k.sector} (hostile ${k.npcFleetId})';
        level = EventLevel.bright;
      case proto.PolicyActionEvent():
        if (_notePolicy(e.tick, k)) return;
        final fled = k.action == policyFlee;
        msg = '${fled ? '! ' : ''}${_fleet(k.fleetId)} [${k.preset.label}] ${k.action}: ${k.reason}';
        level = fled ? EventLevel.bright : EventLevel.dim;
      case proto.PursuitEvent():
        final given = k.phase == proto.PursuitPhase.abandoned;
        msg = '${k.mine && !given ? '! ' : ''}${pursuitEventLine(k)}';
        level = !k.mine ? EventLevel.dim : given ? EventLevel.normal : EventLevel.bright;
        if (k.mine) {
          _pushAlert(Alert(
            icon: given ? '.' : '!',
            message: given ? 'The ${k.label} gave up on ${_fleet(k.fleetId)}' : 'The ${k.label} is after ${_fleet(k.fleetId)}',
            detail: switch ((given, k.to, k.arrivesIn)) {
              (true, _, _) => k.why ?? '',
              (false, final to?, final ticks?) => 'Lands at $to in ${ticksOf(ticks)}',
              _ => '',
            },
            level: given ? AlertTone.dim : AlertTone.glow,
          ));
        }
      case proto.AlertEvent():
        msg = '${k.level == proto.AlertLevel.info ? '' : '! '}${k.message}';
        level = k.level == proto.AlertLevel.info ? EventLevel.normal : EventLevel.bright;
        _pushAlert(Alert(
          icon: k.level == proto.AlertLevel.info ? '.' : '!',
          message: k.message,
          detail: [
            if (k.fleetId != null) _fleet(k.fleetId!),
            if (k.sector != null) '${k.sector}',
          ].join(' '),
          level: k.level == proto.AlertLevel.info ? AlertTone.dim : AlertTone.glow,
        ));
    }

    _log(LogEntry(tick: e.tick, message: msg, level: level));
  }

  /// A hold reason without its parenthesised figures, which drift while the
  /// cause stays the same.
  static String holdKey(String reason) => reason.replaceAll(RegExp(r'\([^)]*\)'), '');

  /// Keep what a fleet's orders just reported. A hold said again for the
  /// same cause takes the place of the one before it, with its figures, and
  /// is not news: true then, and the log does not repeat it.
  bool _notePolicy(int at, proto.PolicyActionEvent k) {
    final acts = policyActs.putIfAbsent(k.fleetId, () => []);
    final last = acts.firstOrNull;
    final again = last != null && last.isHold && k.action == 'hold' && last.preset == k.preset && holdKey(last.reason) == holdKey(k.reason);
    final act = PolicyAct(tick: at, since: again ? last.since : at, preset: k.preset, action: k.action, reason: k.reason);
    if (again) {
      acts[0] = act;
    } else {
      acts.insert(0, act);
      if (acts.length > policyActsKept) acts.removeLast();
    }
    return again;
  }

  int get clockSec =>
      connectedAt == null ? tick : clock.now().difference(connectedAt!).inSeconds;

  // ── UI mapping ────────────────────────────────────────────────

  GameState toUiState() {
    final p = player;
    final hw = homeworld;
    final res = p?.resources ?? const proto.Resources();

    final uiFleets = fleets.map(_mapFleet).toList();
    final home = p?.homeworld ?? hw?.location ?? proto.Hex.origin;

    return GameState(
      tick: tick,
      clockSec: clockSec,
      resources: Resources(
        metal: ResourceStock(
          amount: res.metal.round(),
          rate: hw?.production.metal ?? 0,
        ),
        crystal: ResourceStock(
          amount: res.crystal.round(),
          rate: hw?.production.crystal ?? 0,
        ),
        deut: ResourceStock(
          amount: res.deuterium.round(),
          rate: hw?.production.deuterium ?? 0,
        ),
      ),
      fleets: uiFleets,
      buildQueue: _mapBuildQueue(hw),
      shipyard: _mapShipyard(hw),
      shipCount: p?.shipCount ?? 0,
      research: _mapResearch(hw),
      events: List.of(log),
      alerts: alerts.isEmpty
          ? [Alert(icon: '.', message: 'Uplink established', detail: feed, level: AlertTone.dim)]
          : List.of(alerts),
      homeworld: home,
      waypoints: [
        if (p != null) Waypoint(id: 'HW', coord: home, note: 'Homeworld'),
        for (final f in fleets) Waypoint(id: _fleet(f.id), coord: f.location, note: f.state.wire),
      ],
      sectors: {for (final s in sectors.values) s.location: s},
      signals: {for (final e in signals.entries) proto.Hex.fromKey(e.key): e.value},
      signalBands: {for (final e in signalBands.entries) proto.Hex.fromKey(e.key): e.value},
      previews: {for (final e in previews.entries) (e.key.$1, proto.Hex.fromKey(e.key.$2)): e.value},
      odds: {for (final e in odds.entries) (e.key.$1, proto.Hex.fromKey(e.key.$2)): e.value},
      quotes: Map.of(quotes),
      exchange: p?.exchange ?? const proto.ExchangeMeter(),
      relics: p?.relics ?? const [],
      holds: {for (final f in fleets) f.id: f.cargo},
      stock: res,
      catalog: hw?.catalog,
      hw: hw,
      world: world,
      leaderboard: leaderboard,
      lastRaid: lastRaid,
      lastOutpostRaid: lastOutpostRaid,
      outpost: outpost,
      surveys: {for (final e in surveys.entries) (e.key.$1, proto.Hex.fromKey(e.key.$2)): e.value},
      relays: relays,
      hubs: hubs,
      relaySurveys: {for (final e in relaySurveys.entries) (e.key.$1, proto.Hex.fromKey(e.key.$2)): e.value},
      policyActs: {for (final f in fleets) f.id: ?policyActs[f.id]?.toList()},
      battles: {for (final b in battles.values) b.sector: b},
      pastFights: List.of(pastFights),
    );
  }

  FleetState _mapFleet(proto.FleetState f) {
    final status = switch (f.state) {
      proto.FleetStatus.idle => FleetStatus.idle,
      proto.FleetStatus.docked => FleetStatus.docked,
      proto.FleetStatus.moving => FleetStatus.enRoute,
      proto.FleetStatus.harvesting => FleetStatus.harvesting,
      proto.FleetStatus.inCombat => FleetStatus.combat,
      proto.FleetStatus.returning => FleetStatus.returning,
      proto.FleetStatus.exploring => FleetStatus.exploring,
    };

    final cap = f.cargoCapacity.round();
    final fuelPct = f.fuelMax > 0 ? ((f.fuel / f.fuelMax) * 100).round().clamp(0, 100) : 0;
    final cargoPct = cap > 0 ? ((f.cargo.total / cap) * 100).round().clamp(0, 100) : 0;

    return FleetState(
      id: f.id,
      name: _fleet(f.id),
      sector: f.location,
      status: status,
      shipCount: f.shipCount,
      cargoPercent: cargoPct,
      fuelPercent: fuelPct,
      ships: f.ships,
      hull: f.hull,
      hullMax: f.hullMax,
      attack: f.attack,
      shield: f.shield,
      refit: f.refit,
      refitDue: f.refitDue,
      repair: f.repair,
      cargo: FleetCargo(
        metal: f.cargo.metal.round(),
        crystal: f.cargo.crystal.round(),
        deut: f.cargo.deuterium.round(),
        capacity: math.max(1, cap),
      ),
      fuel: f.fuel.round(),
      fuelMax: math.max(1, f.fuelMax.round()),
      jumpFuel: f.jumpFuel.round(),
      homeFuel: f.homeFuel.round(),
      hopsHome: f.hopsHome,
      fuelExact: f.fuel,
      jumpFuelExact: f.jumpFuel,
      jumpTicks: f.jumpTicks,
      scanRange: f.scanRange,
      outpostFuel: f.outpostFuel?.round(),
      atOutpost: f.atOutpost,
      relay: f.relay,
      cooldown: f.cooldownRemaining,
      power: f.power,
      rangeHops: f.rangeHops,
      orders: f.policy,
      laden: f.laden,
      cargoBlocked: f.cargoBlocked,
      holdCapacity: f.cargoCapacity,
      refine: f.refine,
      refuel: f.refuel,
      flight: f.flight,
      fleeing: f.fleeing,
      pursuedBy: f.pursuedBy,
      relic: f.relic,
      foughtAt: _foughtAt[f.id],
    );
  }

  QueueItem _queueItem(String name, int start, int end) {
    final total = math.max(1, end - start);
    final done = (tick - start).clamp(0, total);
    return QueueItem(
      name: name,
      time: _fmtTicks(math.max(0, end - tick)),
      pct: done / total * 100,
      active: true,
    );
  }

  /// A queue entry that has not started; [waits] says why and when it will.
  QueueItem _waitingItem(String name, String waits) => QueueItem(name: name, time: waits, pct: 0, active: false);

  /// Stable ids of a queue's running and waiting items, in line order.
  QueueIds queueIds(proto.QueueType queue) {
    final hw = homeworld;
    if (hw == null) return const QueueIds();
    return switch (queue) {
      proto.QueueType.building =>
        QueueIds(running: [for (final q in hw.buildQueue) q.id], waiting: [for (final q in hw.buildPending) q.id]),
      proto.QueueType.ship => QueueIds(
          running: [if (hw.shipyardQueue != null) hw.shipyardQueue!.id],
          waiting: [for (final q in hw.shipyardPending) q.id]),
      proto.QueueType.research => QueueIds(
          running: [if (hw.researchActive != null) hw.researchActive!.id],
          waiting: [for (final q in hw.researchPending) q.id]),
    };
  }

  List<QueueItem> _mapBuildQueue(proto.HomeworldState? hw) {
    if (hw == null) return const [];
    return [
      for (final q in hw.buildQueue)
        _queueItem('${q.buildingType.label} Lv.${q.targetLevel}', q.startTick, q.endTick),
      for (final q in hw.buildPending)
        _waitingItem('${q.buildingType.label} Lv.${q.targetLevel}', waitSummary(q.waitingOn, q.waitingFor, q.startIn)),
    ];
  }

  List<QueueItem> _mapShipyard(proto.HomeworldState? hw) {
    if (hw == null) return const [];
    final q = hw.shipyardQueue;
    return [
      if (q != null) _queueItem('${q.item.label} x${q.count} (${q.built} built)', q.startTick, q.endTick),
      for (final p in hw.shipyardPending)
        _waitingItem('${p.item.label} x${p.count}', batchWaitSummary(p.waitingOn, p.startIn)),
    ];
  }

  ResearchState _mapResearch(proto.HomeworldState? hw) {
    final completed = [
      for (final r in hw?.research ?? const <proto.ResearchState>[])
        if (r.level > 0) '${r.tech.label} ${r.level}',
    ];
    final waiting = [
      for (final p in hw?.researchPending ?? const <proto.QueuedResearch>[])
        _waitingItem('${p.tech.label} Lv.${p.targetLevel}', waitSummary(p.waitingOn, p.waitingFor, p.startIn)),
    ];
    final a = hw?.researchActive;
    if (a == null) {
      return ResearchState(
          name: 'Idle', time: waiting.isEmpty ? '—' : '${waiting.length} waiting', pct: 0, completed: completed, waiting: waiting);
    }
    final item = _queueItem('${a.tech.label} Lv.${a.targetLevel}', a.startTick, a.endTick);
    return ResearchState(name: item.name, time: item.time, pct: item.pct, completed: completed, waiting: waiting);
  }

  static String _fmtTicks(int t) {
    final d = t ~/ 86400;
    final h = (t % 86400) ~/ 3600;
    final m = (t % 3600) ~/ 60;
    final s = t % 60;
    final mm = m.toString().padLeft(2, '0');
    final ss = s.toString().padLeft(2, '0');
    final hms = '${h.toString().padLeft(2, '0')}:$mm:$ss';
    return d > 0 ? '${d}d $hms' : h > 0 ? hms : '$mm:$ss';
  }
}
