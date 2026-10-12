import 'dart:math' as math;

import 'package:clock/clock.dart';

import '../models/fleet.dart';
import '../models/game_state.dart';
import '../models/homeworld.dart';
import '../console/intel.dart' show clockFmt, despawnLabel, groupDigits;
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
            '(core ${e.core.toStringAsFixed(1)}, combat ${e.combat.toStringAsFixed(1)}, explore ${e.explore.toStringAsFixed(1)})',
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

  /// The raid on the homeworld the server last announced and has not yet
  /// resolved. A raid on the outpost is told by [outpost].
  proto.RaidIncomingEvent? raid;

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
    raid = null;
    sectors.clear();
    signals.clear();
    signalBands.clear();
    policyActs.clear();
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
    world = s.world;
    sectors.clear();
    for (final sec in s.knownSectors) {
      sectors[sec.location.toKey()] = sec;
    }
    signals.removeWhere((k, _) => sectors.containsKey(k));
    signalBands.removeWhere((k, _) => sectors.containsKey(k));
    previews.clear();
    odds.clear();
    quotes.clear();
    surveys.clear();
    pushLog(first ? 'Uplink established. Homeworld at ${s.player.homeworld}.' : 'Uplink refreshed: the console is in step with the world again.',
        EventLevel.bright);
  }

  void _applyTick(proto.TickUpdate u) {
    tick = u.tick;
    if (u.player != null) player = u.player;
    fleets = List.of(u.fleets);
    if (u.homeworldUpdate != null) homeworld = u.homeworldUpdate;
    outpost = u.outpost;
    for (final sec in u.sectorUpdates ?? const <proto.SectorState>[]) {
      final key = sec.location.toKey();
      sectors[key] = sec;
      signals.remove(key);
      signalBands.remove(key);
    }
    for (final e in u.events ?? const <proto.GameEvent>[]) {
      _ingestEvent(e);
    }
    policyActs.removeWhere((id, _) => fleets.every((f) => f.id != id));
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
      case proto.CombatRoundEvent():
        return; // far too chatty for the log
      case proto.ShipDestroyedEvent():
        msg = k.mine
            ? '! Your ${k.shipClass.label} lost in ${k.sector} (fleet ${k.ownerFleetId})'
            : '${k.isNpc ? 'Hostile' : "${k.owner}'s"} ${k.shipClass.label} destroyed in ${k.sector}';
        level = k.mine ? EventLevel.bright : (k.isNpc ? EventLevel.normal : EventLevel.dim);
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
        final outcome = k.mine ? (k.playerVictory ? 'victory' : 'defeat') : (k.playerVictory ? 'won by others' : 'lost by others');
        final pile = k.salvage == null ? '' : ' -- wreckage ${amountsOf(k.salvage!)}';
        msg = '! Combat ended in ${k.sector}: $outcome$pile';
        level = k.mine ? EventLevel.bright : EventLevel.dim;
      case proto.SalvageCollectedEvent():
        msg = '${_fleet(k.fleetId)} collected salvage ${amountsOf(k.resources)}'
            '${k.remaining == null ? '' : ", ${amountsOf(k.remaining!)} left behind"}';
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
        _pushAlert(Alert(icon: '!', message: '${k.resource.name} storage full', detail: 'Spend it or build a Storage Vault', level: AlertTone.glow));
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
      case proto.RaidIncomingEvent(outpost: final at?):
        msg = '! RAID INCOMING on your outpost at $at (${k.threat}, power ${k.estPower.round()}) -- arrival tick ${k.arrivalTick}';
        level = EventLevel.bright;
        _pushAlert(Alert(
          icon: '!',
          message: 'Raid incoming on your outpost (${k.threat}, power ${k.estPower.round()})',
          detail: 'Arrives tick ${k.arrivalTick} -- ${math.max(0, k.arrivalTick - tick)}s',
          level: AlertTone.glow,
        ));
      case proto.RaidResolvedEvent(outpost: final at?):
        final fight = 'raid ${k.raidPower.toStringAsFixed(0)} vs defense ${k.defensePower.toStringAsFixed(0)}';
        msg = k.defended
            ? '! Raid on your outpost at $at repelled ($fight)'
            : '! Raid broke your outpost at $at ($fight): lost ${amountsOf(k.resourcesLost)}'
                '${k.shipsLost > 0 ? ', ${k.shipsLost} ship${k.shipsLost == 1 ? '' : 's'} there destroyed' : ''}'
                '${k.breached ? '; it is breached until it has repaired itself' : ''}';
        level = EventLevel.bright;
      case proto.RaidIncomingEvent():
        raid = k;
        msg = '! RAID INCOMING (${k.threat}, power ${k.estPower.round()}) -- arrival tick ${k.arrivalTick}';
        level = EventLevel.bright;
        _pushAlert(Alert(
          icon: '!',
          message: 'Raid incoming (${k.threat}, power ${k.estPower.round()})',
          detail: 'Arrives tick ${k.arrivalTick} -- ${math.max(0, k.arrivalTick - tick)}s',
          level: AlertTone.glow,
        ));
      case proto.RaidResolvedEvent():
        raid = null;
        final structures = k.structuresLost == 0
            ? ''
            : ', ${k.structuresLost} structures lost (${k.structuresRestored} will be rebuilt)';
        final pile = k.salvageDropped == null
            ? ''
            : ', wreckage ${amountsOf(k.salvageDropped!)}${k.salvageDespawnTick == null ? '' : ' drifts away in ${clockFmt(math.max(0, k.salvageDespawnTick! - tick))}'}';
        msg = k.defended
            ? '! Raid repelled (raid ${k.raidPower.toStringAsFixed(0)} vs defense ${k.defensePower.toStringAsFixed(0)})$structures$pile'
            : '! Raid broke through (raid ${k.raidPower.toStringAsFixed(0)} vs defense ${k.defensePower.toStringAsFixed(0)}): lost ${amountsOf(k.resourcesLost)}$structures${k.shipsLost > 0 ? ', ${k.shipsLost} docked ships destroyed' : ''}';
        level = EventLevel.bright;
      case proto.SiteExplorationStartedEvent():
        msg = '${_fleet(k.fleetId)} boarding derelict (tier ${k.tier}) in ${k.sector}, done tick ${k.endTick}';
      case proto.SiteExploredEvent():
        msg = '${_fleet(k.fleetId)} derelict yielded ${amountsOf(k.resources)}'
            '${k.recoveredShip != null ? ', recovered ${k.recoveredShip!.label}' : ''}'
            '${k.techCache != null ? ', data core: ${k.techCache!.label}' : ''}'
            '${k.relic ? ', ANCIENT RELIC' : ''}${k.points > 0 ? ' (+${k.points.toStringAsFixed(2)} explore)' : ''}';
        level = EventLevel.bright;
      case proto.ChartDeliveredEvent():
        msg = '${_fleet(k.fleetId)} delivered the chart of ${k.sectors} sectors '
            '(+${k.points.toStringAsFixed(2)} explore)';
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
        msg = '${_fleet(k.fleetId)} [${k.preset.label}] ${k.action}: ${k.reason}';
        level = EventLevel.dim;
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

  static String _age(int ticks) {
    final t = math.max(0, ticks);
    if (t < 120) return '${t}s';
    if (t < 7200) return '${t ~/ 60}m';
    return '${t ~/ 3600}h';
  }

  int get clockSec =>
      connectedAt == null ? tick : clock.now().difference(connectedAt!).inSeconds;

  // ── UI mapping ────────────────────────────────────────────────

  GameState toUiState({int activeFleet = 0}) {
    final p = player;
    final hw = homeworld;
    final res = p?.resources ?? const proto.Resources();

    final uiFleets = fleets.map(_mapFleet).toList();
    final active = (activeFleet >= 0 && activeFleet < fleets.length) ? fleets[activeFleet] : null;
    final home = p?.homeworld ?? hw?.location ?? proto.Hex.origin;
    final focus = active?.location ?? home;

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
      docked: _dockedSummary(hw),
      research: _mapResearch(hw),
      events: List.of(log),
      alerts: alerts.isEmpty
          ? [Alert(icon: '.', message: 'Uplink established', detail: feed, level: AlertTone.dim)]
          : List.of(alerts),
      sector: _mapSector(focus),
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
      holds: {for (final f in fleets) f.id: f.cargo},
      stock: res,
      catalog: hw?.catalog,
      hw: hw,
      world: world,
      leaderboard: leaderboard,
      raid: raid,
      outpost: outpost,
      surveys: {for (final e in surveys.entries) (e.key.$1, proto.Hex.fromKey(e.key.$2)): e.value},
      policyActs: {for (final f in fleets) f.id: ?policyActs[f.id]?.toList()},
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

    final byClass = <proto.ShipClass, ({int count, double hull, double hullMax, List<int> ids})>{};
    for (final s in f.ships) {
      final prev = byClass[s.shipClass];
      byClass[s.shipClass] = (
        count: (prev?.count ?? 0) + 1,
        hull: (prev?.hull ?? 0) + s.hull,
        hullMax: (prev?.hullMax ?? 0) + s.hullMax,
        ids: [...?prev?.ids, s.id],
      );
    }

    final cap = f.cargoCapacity.round();
    final fuelPct = f.fuelMax > 0 ? ((f.fuel / f.fuelMax) * 100).round().clamp(0, 100) : 0;
    final cargoPct = cap > 0 ? ((f.cargo.total / cap) * 100).round().clamp(0, 100) : 0;

    return FleetState(
      id: f.id,
      name: _fleet(f.id),
      sector: f.location,
      status: status,
      shipCount: f.ships.length,
      cargoPercent: cargoPct,
      fuelPercent: fuelPct,
      ships: [
        for (final e in byClass.entries)
          ShipState(
            shipClass: e.key.label,
            count: e.value.count,
            hull: e.value.hull.round(),
            hullMax: e.value.hullMax.round().clamp(1, 99999),
            ids: e.value.ids,
          ),
      ],
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
      outpostFuel: f.outpostFuel?.round(),
      atOutpost: f.atOutpost,
      cooldown: f.cooldownRemaining,
      power: f.power,
      rangeHops: f.rangeHops,
      orders: f.policy,
      laden: f.laden,
      cargoBlocked: f.cargoBlocked,
      holdCapacity: f.cargoCapacity,
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

  String _dockedSummary(proto.HomeworldState? hw) {
    if (hw == null || hw.dockedShips.isEmpty) return 'none';
    final counts = <String, int>{};
    for (final s in hw.dockedShips) {
      counts[s.shipClass.label] = (counts[s.shipClass.label] ?? 0) + 1;
    }
    return counts.entries.map((e) => '${e.value}x${e.key}').join('  ');
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

  SectorInfo _mapSector(proto.Hex loc) {
    final sec = sectors[loc.toKey()];
    if (sec == null) return SectorInfo.unknown;

    final hostiles = sec.hostiles;
    final hostileStr = (hostiles == null || hostiles.isEmpty)
        ? 'None in sector'
        : hostiles
            .map((h) => '#${h.id} ${h.ships.map((s) => '${s.count}x${s.shipClass.label}').join(' ')} ${h.behavior.wire.toLowerCase()}')
            .join('; ');

    final near = <String>[];
    for (final c in sec.connections) {
      final h = sectors[c.toKey()]?.hostiles;
      if (h != null && h.isNotEmpty) {
        near.add('${h.fold<int>(0, (s, e) => s + e.shipCount)} hostile ships at $c');
      }
    }

    final salvage = sec.salvage;
    final site = sec.site;
    return SectorInfo(
      terrain: sec.terrain.label,
      metal: sec.resources.metal.label,
      crystal: sec.resources.crystal.label,
      deut: sec.resources.deuterium.label,
      hostile: hostileStr,
      intel: sec.live ? 'live' : 'chart, ${_age(tick - sec.lastSeen)} old',
      adjacent: near.isEmpty ? 'none known' : near.join('; '),
      exits: '${sec.connections.length} of 6',
      salvage: (salvage == null || salvage.total <= 0)
          ? 'none'
          : '${amountsOf(salvage)}${despawnLabel(sec, tick) == null ? '' : ', ${despawnLabel(sec, tick)}'}${sec.pinsStale ? ' (stale)' : ''}',
      site: site == null ? 'none' : 'tier ${site.tier} (${site.risk.label})',
    );
  }

  static String _fmtTicks(int t) {
    final h = t ~/ 3600;
    final m = (t % 3600) ~/ 60;
    final s = t % 60;
    final mm = m.toString().padLeft(2, '0');
    final ss = s.toString().padLeft(2, '0');
    return h > 0 ? '${h.toString().padLeft(2, '0')}:$mm:$ss' : '$mm:$ss';
  }
}
