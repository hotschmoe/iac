import 'dart:math' as math;

import '../models/fleet.dart';
import '../models/game_state.dart';
import '../models/homeworld.dart';
import '../models/resources.dart';
import '../protocol/protocol.dart' as proto;

/// Holds the latest protocol snapshots (full_state + tick_update deltas) and
/// maps them to the amber UI presentation models.
class StateMapper {
  int tick = 0;
  DateTime? connectedAt;

  proto.PlayerState? player;
  List<proto.FleetState> fleets = [];
  proto.HomeworldState? homeworld;
  proto.WorldInfo? world;

  /// Known sectors keyed by [proto.Hex.toKey].
  final Map<int, proto.SectorState> sectors = {};

  /// Faint scan contacts for sectors we have no data on yet.
  final Map<int, proto.SignalKind> signals = {};

  final List<LogEntry> log = [];
  final List<Alert> alerts = [];

  bool get hasState => player != null;

  void reset() {
    tick = 0;
    connectedAt = null;
    player = null;
    fleets = [];
    homeworld = null;
    world = null;
    sectors.clear();
    signals.clear();
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
      case proto.GameEvent():
        _ingestEvent(msg);
      case proto.ErrorMessage():
        _applyError(msg);
      case proto.AuthResult():
        break; // handled by the controller (connection state)
    }
  }

  void _applyFull(proto.GameState s) {
    tick = s.tick;
    connectedAt = DateTime.now();
    player = s.player;
    fleets = List.of(s.fleets);
    homeworld = s.homeworld;
    world = s.world;
    sectors.clear();
    for (final sec in s.knownSectors) {
      sectors[sec.location.toKey()] = sec;
    }
    signals.removeWhere((k, _) => sectors.containsKey(k));
    pushLog('Full state sync: tick $tick, homeworld ${s.player.homeworld}', EventLevel.bright);
  }

  void _applyTick(proto.TickUpdate u) {
    tick = u.tick;
    if (u.player != null) player = u.player;
    fleets = List.of(u.fleets);
    if (u.homeworldUpdate != null) homeworld = u.homeworldUpdate;
    for (final sec in u.sectorUpdates ?? const <proto.SectorState>[]) {
      final key = sec.location.toKey();
      sectors[key] = sec;
      signals.remove(key);
    }
    for (final e in u.events ?? const <proto.GameEvent>[]) {
      _ingestEvent(e);
    }
  }

  void _applyError(proto.ErrorMessage err) {
    pushLog('ERR ${err.code.code} ${err.code.wire}: ${err.message}', EventLevel.bright);
    _pushAlert(Alert(
      icon: '!',
      message: err.message,
      detail: err.code.wire,
      level: AlertTone.glow,
    ));
  }

  /// The server just issued this account's token. It is shown once, so it
  /// goes in the log and alerts; [saved] says whether the browser kept it.
  void noticeToken(String token, {required bool saved}) {
    pushLog('Account token: $token (keep it; it is your password)', EventLevel.bright);
    _pushAlert(Alert(
      icon: '!',
      message: 'New account token',
      detail: saved ? token : '$token  (NOT stored in this browser: copy it now)',
      level: AlertTone.glow,
    ));
  }

  void pushLog(String msg, [EventLevel level = EventLevel.normal]) {
    log.insert(0, LogEntry(tick: tick, message: msg, level: level));
    if (log.length > 14) log.removeRange(14, log.length);
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
          msg = 'Hostile fleet ${k.fleetId} destroyed in ${k.sector} -- wreckage ${_res(k.salvage)}';
        } else {
          msg = "${k.owner}'s fleet ${_fleet(k.fleetId)} destroyed in ${k.sector}";
          level = EventLevel.dim;
        }
      case proto.ResourceHarvestedEvent():
        msg = '${_fleet(k.fleetId)} harvested ${_res(k.resources)} over ${k.ticks}s';
      case proto.SectorEnteredEvent():
        msg = '${_fleet(k.fleetId)} entered ${k.sector}${k.firstVisit ? ' (first visit)' : ''}';
      case proto.CombatStartedEvent():
        msg = '! Combat in ${k.sector}: ${k.mine ? 'your' : "${k.owner}'s"} ${_fleet(k.playerFleetId)} vs hostile ${k.enemyFleetId}';
        level = k.mine ? EventLevel.bright : EventLevel.dim;
      case proto.CombatEndedEvent():
        final outcome = k.mine ? (k.playerVictory ? 'victory' : 'defeat') : (k.playerVictory ? 'won by others' : 'lost by others');
        final pile = k.salvage == null ? '' : ' -- wreckage ${_res(k.salvage!)}';
        msg = '! Combat ended in ${k.sector}: $outcome$pile';
        level = k.mine ? EventLevel.bright : EventLevel.dim;
      case proto.SalvageCollectedEvent():
        msg = '${_fleet(k.fleetId)} collected salvage ${_res(k.resources)}'
            '${k.remaining == null ? '' : ", ${_res(k.remaining!)} left behind"}';
      case proto.SalvageDespawnedEvent():
        msg = 'Wreckage in ${k.sector} drifted away (${_res(k.resources)})';
        level = EventLevel.dim;
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
      case proto.ScanCompletedEvent():
        msg = '${_fleet(k.fleetId)} scan at ${k.sector}: ${k.sectorsRevealed} sectors revealed, '
            '${k.hostilesDetected} hostiles${k.signals.isEmpty ? '' : ', ${k.signals.length} faint contacts'}';
        for (final s in k.signals) {
          final key = s.sector.toKey();
          if (!sectors.containsKey(key)) signals[key] = s.signal;
        }
      case proto.RaidIncomingEvent():
        msg = '! RAID INCOMING (${k.threat}) -- arrival tick ${k.arrivalTick}';
        level = EventLevel.bright;
        _pushAlert(Alert(
          icon: '!',
          message: 'Raid incoming (${k.threat})',
          detail: 'Arrives tick ${k.arrivalTick} -- ${math.max(0, k.arrivalTick - tick)}s',
          level: AlertTone.glow,
        ));
      case proto.RaidResolvedEvent():
        msg = k.defended
            ? '! Raid repelled (raid ${k.raidPower.toStringAsFixed(0)} vs defense ${k.defensePower.toStringAsFixed(0)})'
            : '! Raid broke through: lost ${_res(k.resourcesLost)}';
        level = EventLevel.bright;
      case proto.SiteExplorationStartedEvent():
        msg = '${_fleet(k.fleetId)} boarding derelict (tier ${k.tier}) in ${k.sector}, done tick ${k.endTick}';
      case proto.SiteExploredEvent():
        msg = '${_fleet(k.fleetId)} derelict yielded ${_res(k.resources)}'
            '${k.recoveredShip != null ? ', recovered ${k.recoveredShip!.label}' : ''}'
            '${k.techCache != null ? ', data core: ${k.techCache!.label}' : ''}';
        level = EventLevel.bright;
      case proto.SiteAmbushEvent():
        msg = '! ${_fleet(k.fleetId)} ambushed aboard derelict in ${k.sector} (hostile ${k.npcFleetId})';
        level = EventLevel.bright;
      case proto.PolicyActionEvent():
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

    log.insert(0, LogEntry(tick: e.tick, message: msg, level: level));
    if (log.length > 14) log.removeRange(14, log.length);
  }

  static String _age(int ticks) {
    final t = math.max(0, ticks);
    if (t < 120) return '${t}s';
    if (t < 7200) return '${t ~/ 60}m';
    return '${t ~/ 3600}h';
  }

  static String _res(proto.Resources r) =>
      '${r.metal.round()}M ${r.crystal.round()}C ${r.deuterium.round()}D';

  int get clockSec =>
      connectedAt == null ? tick : DateTime.now().difference(connectedAt!).inSeconds;

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
          ? const [
              Alert(icon: '.', message: 'Uplink established', detail: 'Live server feed', level: AlertTone.dim),
            ]
          : List.of(alerts),
      sector: _mapSector(focus),
      homeworld: home,
      waypoints: [
        if (p != null) Waypoint(id: 'HW', coord: home, note: 'Homeworld'),
        for (final f in fleets) Waypoint(id: _fleet(f.id), coord: f.location, note: f.state.wire),
      ],
      sectors: {for (final s in sectors.values) s.location: s},
      signals: {for (final e in signals.entries) proto.Hex.fromKey(e.key): e.value},
      stock: res,
      catalog: hw?.catalog,
      world: world,
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
      cooldown: f.cooldownRemaining,
      policy: f.policy?.label,
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

  List<QueueItem> _mapBuildQueue(proto.HomeworldState? hw) {
    final q = hw?.buildQueue;
    if (q == null) return const [];
    return [_queueItem('${q.buildingType.label} Lv.${q.targetLevel}', q.startTick, q.endTick)];
  }

  List<QueueItem> _mapShipyard(proto.HomeworldState? hw) {
    final q = hw?.shipyardQueue;
    if (q == null) return const [];
    return [_queueItem('${q.shipClass.label} x${q.count} (${q.built} built)', q.startTick, q.endTick)];
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
    final a = hw?.researchActive;
    if (a == null) {
      return ResearchState(name: 'Idle', time: '—', pct: 0, completed: completed);
    }
    final item = _queueItem('${a.tech.label} Lv.${a.targetLevel}', a.startTick, a.endTick);
    return ResearchState(name: item.name, time: item.time, pct: item.pct, completed: completed);
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
          : '${_res(salvage)}${!sec.live && (sec.salvageDespawnTick ?? 1 << 60) <= tick ? ' (since gone)' : ''}',
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
