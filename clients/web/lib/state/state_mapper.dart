import 'package:iac_shared/iac_shared.dart' as shared;

import '../models/fleet.dart';
import '../models/game_state.dart';
import '../models/hex.dart';
import '../models/homeworld.dart';
import '../models/resources.dart';

/// Maps shared protocol snapshots → amber UI presentation models.
class StateMapper {
  shared.FullState? lastFull;
  shared.PlayerState? player;
  List<shared.FleetState> fleets = [];
  shared.HomeworldState? homeworld;
  List<shared.SectorState> sectors = [];
  final List<GameEvent> events = [];
  final List<Alert> alerts = [];
  int tick = 0;
  int startTick = 0;
  DateTime? connectedAt;

  void applyFull(shared.FullState state) {
    lastFull = state;
    tick = state.tick;
    startTick = state.tick;
    connectedAt = DateTime.now();
    player = state.player;
    fleets = List.of(state.fleets);
    homeworld = state.homeworld;
    sectors = List.of(state.knownSectors);
    _pushEvent(GameEvent(
      tick: tick,
      message: 'Full state sync — tick $tick, homeworld ${state.player.homeworld}',
      level: EventLevel.bright,
    ));
  }

  void applyTick(shared.TickUpdate update) {
    tick = update.tick;
    if (update.player != null) player = update.player;
    if (update.fleetUpdates != null) fleets = List.of(update.fleetUpdates!);
    if (update.homeworldUpdate != null) homeworld = update.homeworldUpdate;
    if (update.sectorUpdates != null) {
      // Merge sector updates by location
      final map = {for (final s in sectors) s.location.toKey(): s};
      for (final s in update.sectorUpdates!) {
        map[s.location.toKey()] = s;
      }
      sectors = map.values.toList();
    }
    if (update.events != null) {
      for (final e in update.events!) {
        _ingestSharedEvent(e);
      }
    }
  }

  void applyError(shared.ErrorMessage err) {
    _pushEvent(GameEvent(
      tick: tick,
      message: 'ERR ${err.code.code}: ${err.message}',
      level: EventLevel.bright,
    ));
    alerts.insert(
      0,
      Alert(
        icon: '!',
        message: err.message,
        detail: err.code.wireName,
        level: AlertLevel.glow,
      ),
    );
    if (alerts.length > 8) alerts.removeRange(8, alerts.length);
  }

  void ingestSharedEvent(shared.GameEvent e) {
    _ingestSharedEvent(e);
  }

  void pushLocalEvent(GameEvent e) => _pushEvent(e);

  void _ingestSharedEvent(shared.GameEvent e) {
    final msg = _formatEvent(e);
    if (msg == null) return;
    final level = msg.startsWith('!') || msg.toLowerCase().contains('combat')
        ? EventLevel.bright
        : EventLevel.normal;
    _pushEvent(GameEvent(tick: e.tick, message: msg, level: level));

    // Surface alerts
    final kind = e.kind;
    if (kind.containsKey('alert')) {
      final a = kind['alert'] as Map<String, dynamic>? ?? {};
      final text = a['message'] as String? ?? msg;
      final lvl = a['level'] as String? ?? 'info';
      alerts.insert(
        0,
        Alert(
          icon: lvl == 'critical' ? '!' : (lvl == 'warning' ? '!' : '.'),
          message: text,
          detail: '',
          level: lvl == 'info' ? AlertLevel.dim : AlertLevel.glow,
        ),
      );
      if (alerts.length > 8) alerts.removeRange(8, alerts.length);
    }
  }

  String? _formatEvent(shared.GameEvent e) {
    final kind = e.kind;
    if (kind.isEmpty) return null;
    final key = kind.keys.first;
    final body = kind[key];
    if (body is! Map) return key;
    final m = Map<String, dynamic>.from(body);
    switch (key) {
      case 'alert':
        return m['message'] as String? ?? 'alert';
      case 'resource_harvested':
        return 'Fleet ${m['fleet_id']}: harvested '
            '${(m['amount'] as num?)?.toStringAsFixed(0) ?? '?'} '
            '${m['resource_type']}';
      case 'sector_entered':
        final first = m['first_visit'] == true ? ' (first visit)' : '';
        return 'Fleet ${m['fleet_id']} entered sector ${m['sector']}$first';
      case 'fleet_arrived':
        return 'Fleet ${m['fleet_id']} arrived ${m['sector']}';
      case 'building_completed':
        return 'Building complete: ${m['building_type']} Lv${m['new_level']}';
      case 'research_completed':
        return 'Research complete: ${m['tech']} Lv${m['new_level']}';
      case 'ship_built':
        return 'Shipyard: ${m['count']}× ${m['ship_class']} ready';
      case 'combat_started':
        return '! Combat started in ${m['sector']}';
      case 'combat_ended':
        final win = m['player_victory'] == true ? 'victory' : 'defeat';
        return '! Combat ended — $win';
      case 'ship_destroyed':
        return 'Ship destroyed: ${m['ship_class']}';
      default:
        return key.replaceAll('_', ' ');
    }
  }

  void _pushEvent(GameEvent e) {
    events.insert(0, e);
    if (events.length > 14) events.removeRange(14, events.length);
  }

  int get clockSec {
    if (connectedAt == null) return tick;
    return DateTime.now().difference(connectedAt!).inSeconds;
  }

  GameState toUiState() {
    final p = player;
    final hw = homeworld;
    final res = p?.resources ?? const shared.ResourceAmounts();

    // Caps are presentation-only; rates from building levels if we have homeworld
    final metalRate = _prodRate(shared.BuildingType.metalMine, hw);
    final crystalRate = _prodRate(shared.BuildingType.crystalMine, hw);
    final deutRate = _prodRate(shared.BuildingType.deuteriumSynthesizer, hw);

    final uiFleets = fleets.map(_mapFleet).toList();
    if (uiFleets.isEmpty) {
      uiFleets.add(const FleetState(
        name: '—',
        sector: Hex(0, 0),
        status: FleetStatus.docked,
        shipCount: 0,
      ));
    }

    final activeFleet = fleets.isNotEmpty ? fleets.first : null;
    final sectorUi = _mapSector(
      activeFleet?.location ?? p?.homeworld ?? const shared.Hex(0, 0),
    );

    return GameState(
      tick: tick,
      clockSec: clockSec,
      resources: Resources(
        metal: ResourceStock(
          amount: res.metal.round(),
          rate: metalRate,
          cap: 50000,
        ),
        crystal: ResourceStock(
          amount: res.crystal.round(),
          rate: crystalRate,
          cap: 40000,
        ),
        deut: ResourceStock(
          amount: res.deuterium.round(),
          rate: deutRate,
          cap: 20000,
        ),
      ),
      fleets: uiFleets,
      buildQueue: _mapBuildQueue(hw),
      shipyard: _mapShipyard(hw),
      docked: _dockedSummary(hw),
      research: _mapResearch(hw),
      events: List.of(events),
      alerts: alerts.isEmpty
          ? [
              const Alert(
                icon: '.',
                message: 'Uplink established',
                detail: 'Live server feed',
                level: AlertLevel.dim,
              ),
            ]
          : List.of(alerts),
      sector: sectorUi,
      homeworld: _toUiHex(p?.homeworld ?? const shared.Hex(0, 0)),
      waypoints: [
        if (p != null)
          Waypoint(
            id: 'HW',
            coord: _toUiHex(p.homeworld),
            note: 'Homeworld',
          ),
        for (final f in fleets)
          Waypoint(
            id: f.name ?? 'F${f.id}',
            coord: _toUiHex(f.location),
            note: f.state.wireName,
          ),
      ],
    );
  }

  int _prodRate(shared.BuildingType type, shared.HomeworldState? hw) {
    if (hw == null) return 0;
    int? level;
    for (final b in hw.buildings) {
      if (b.buildingType == type) {
        level = b.level;
        break;
      }
    }
    if (level == null || level == 0) return 0;
    return shared.productionPerTick(type, level).round().clamp(0, 999);
  }

  FleetState _mapFleet(shared.FleetState f) {
    final status = switch (f.state) {
      shared.FleetStatus.idle => FleetStatus.docked,
      shared.FleetStatus.docked => FleetStatus.docked,
      shared.FleetStatus.moving => FleetStatus.enRoute,
      shared.FleetStatus.harvesting => FleetStatus.harvesting,
      shared.FleetStatus.inCombat => FleetStatus.combat,
      shared.FleetStatus.returning => FleetStatus.returning,
    };

    // Aggregate ships by class for UI
    final byClass = <String, ({int count, double hull, double hullMax})>{};
    for (final s in f.ships) {
      final label = s.class_.label;
      final prev = byClass[label];
      if (prev == null) {
        byClass[label] = (count: 1, hull: s.hull, hullMax: s.hullMax);
      } else {
        byClass[label] = (
          count: prev.count + 1,
          hull: prev.hull + s.hull,
          hullMax: prev.hullMax + s.hullMax,
        );
      }
    }

    final cargoCap = f.ships.fold<double>(
      0,
      (sum, s) => sum + s.class_.baseStats.cargo,
    );
    final cargoTotal = f.cargo.metal + f.cargo.crystal + f.cargo.deuterium;
    final fuelPct =
        f.fuelMax > 0 ? ((f.fuel / f.fuelMax) * 100).round().clamp(0, 100) : 0;
    final cargoPct =
        cargoCap > 0 ? ((cargoTotal / cargoCap) * 100).round().clamp(0, 100) : 0;

    return FleetState(
      name: f.name ?? 'Fleet ${f.id}',
      sector: _toUiHex(f.location),
      status: status,
      shipCount: f.ships.length,
      cargoPercent: cargoPct,
      fuelPercent: fuelPct,
      ships: byClass.entries
          .map(
            (e) => ShipState(
              shipClass: e.key,
              count: e.value.count,
              hull: e.value.hull.round(),
              hullMax: e.value.hullMax.round().clamp(1, 99999),
            ),
          )
          .toList(),
      cargo: FleetCargo(
        metal: f.cargo.metal.round(),
        crystal: f.cargo.crystal.round(),
        deut: f.cargo.deuterium.round(),
        capacity: cargoCap.round().clamp(1, 99999),
      ),
      fuel: f.fuel.round(),
      fuelMax: f.fuelMax.round().clamp(1, 999999),
    );
  }

  List<QueueItem> _mapBuildQueue(shared.HomeworldState? hw) {
    if (hw?.buildQueue == null) return const [];
    final q = hw!.buildQueue!;
    final total = (q.endTick - q.startTick).clamp(1, 1 << 30);
    final done = (tick - q.startTick).clamp(0, total);
    final pct = (done / total * 100).clamp(0, 100).toDouble();
    final remaining = (q.endTick - tick).clamp(0, 1 << 30);
    return [
      QueueItem(
        name: '${q.buildingType.label} Lv.${q.targetLevel}',
        time: _fmtTicks(remaining),
        pct: pct,
        active: true,
      ),
    ];
  }

  List<QueueItem> _mapShipyard(shared.HomeworldState? hw) {
    if (hw?.shipyardQueue == null) return const [];
    final q = hw!.shipyardQueue!;
    final total = (q.endTick - q.startTick).clamp(1, 1 << 30);
    final done = (tick - q.startTick).clamp(0, total);
    final pct = (done / total * 100).clamp(0, 100).toDouble();
    final remaining = (q.endTick - tick).clamp(0, 1 << 30);
    return [
      QueueItem(
        name: '${q.shipClass.label} x${q.count} (${q.built} built)',
        time: _fmtTicks(remaining),
        pct: pct,
        active: true,
      ),
    ];
  }

  String _dockedSummary(shared.HomeworldState? hw) {
    if (hw == null || hw.dockedShips.isEmpty) return 'none';
    final counts = <String, int>{};
    for (final s in hw.dockedShips) {
      counts[s.class_.label] = (counts[s.class_.label] ?? 0) + 1;
    }
    return counts.entries.map((e) => '${e.value}x${e.key}').join('  ');
  }

  ResearchState _mapResearch(shared.HomeworldState? hw) {
    if (hw?.researchActive != null) {
      final q = hw!.researchActive!;
      final total = (q.endTick - q.startTick).clamp(1, 1 << 30);
      final done = (tick - q.startTick).clamp(0, total);
      final pct = (done / total * 100).clamp(0, 100).toDouble();
      final remaining = (q.endTick - tick).clamp(0, 1 << 30);
      final completed = hw.research
          .where((r) => r.level > 0)
          .map((r) => '${r.tech.label} ${r.level}')
          .toList();
      return ResearchState(
        name: '${q.tech.label} Lv.${q.targetLevel}',
        time: _fmtTicks(remaining),
        pct: pct,
        fragments: '—',
        completed: completed,
      );
    }
    final completed = hw?.research
            .where((r) => r.level > 0)
            .map((r) => '${r.tech.label} ${r.level}')
            .toList() ??
        const <String>[];
    return ResearchState(
      name: 'Idle',
      time: '—',
      pct: 0,
      fragments: '—',
      completed: completed,
    );
  }

  SectorInfo _mapSector(shared.Hex loc) {
    shared.SectorState? found;
    for (final s in sectors) {
      if (s.location == loc) {
        found = s;
        break;
      }
    }
    if (found == null) {
      return SectorInfo(
        terrain: 'Unknown',
        metal: '—',
        crystal: '—',
        deut: '—',
        hostile: '—',
        adjacent: '—',
        exits: '—',
      );
    }
    final hostiles = found.hostiles;
    String hostileStr = 'None in sector';
    if (hostiles != null && hostiles.isNotEmpty) {
      hostileStr = hostiles
          .map((h) => h.ships.map((s) => '${s.count}x${s.class_.label}').join(' '))
          .join('; ');
    }
    return SectorInfo(
      terrain: found.terrain.label,
      metal: found.resources.metal.label,
      crystal: found.resources.crystal.label,
      deut: found.resources.deuterium.label,
      hostile: hostileStr,
      adjacent: '${found.connections.length} links',
      exits: '${found.connections.length} of 6',
    );
  }

  Hex _toUiHex(shared.Hex h) => Hex(h.q, h.r);

  String _fmtTicks(int t) {
    final h = t ~/ 3600;
    final m = (t % 3600) ~/ 60;
    final s = t % 60;
    if (h > 0) {
      return '${h.toString().padLeft(2, '0')}:'
          '${m.toString().padLeft(2, '0')}:'
          '${s.toString().padLeft(2, '0')}';
    }
    return '${m.toString().padLeft(2, '0')}:'
        '${s.toString().padLeft(2, '0')}';
  }
}
