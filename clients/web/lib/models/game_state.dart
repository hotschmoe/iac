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

class SectorInfo {
  final String terrain;
  final String metal;
  final String crystal;
  final String deut;
  final String hostile;
  final String adjacent;
  final String exits;
  final String salvage;
  final String site;

  /// "live", or how old the remembered intel on this sector is.
  final String intel;

  const SectorInfo({
    required this.terrain,
    required this.metal,
    required this.crystal,
    required this.deut,
    required this.hostile,
    required this.adjacent,
    required this.exits,
    this.salvage = '—',
    this.site = '—',
    this.intel = '—',
  });

  static const unknown = SectorInfo(
    terrain: 'Unknown',
    metal: '—',
    crystal: '—',
    deut: '—',
    hostile: '—',
    adjacent: '—',
    exits: '—',
  );
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

class GameState {
  final int tick;
  final int clockSec;
  final Resources resources;
  final List<FleetState> fleets;
  final List<QueueItem> buildQueue;
  final List<QueueItem> shipyard;
  final String docked;
  final ResearchState research;
  final List<LogEntry> events;
  final List<Alert> alerts;
  final SectorInfo sector;
  final proto.Hex homeworld;
  final List<Waypoint> waypoints;

  /// Every sector the server has told us about, keyed by location.
  /// Absent = unexplored. Connections come from here (server-authoritative).
  final Map<proto.Hex, proto.SectorState> sectors;

  /// Faint scan contacts in not-yet-explored sectors.
  final Map<proto.Hex, proto.SignalKind> signals;

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

  const GameState({
    required this.tick,
    required this.clockSec,
    required this.resources,
    required this.fleets,
    required this.buildQueue,
    required this.shipyard,
    required this.docked,
    required this.research,
    required this.events,
    required this.alerts,
    required this.sector,
    required this.homeworld,
    required this.waypoints,
    this.sectors = const {},
    this.signals = const {},
    this.stock = const proto.Resources(),
    this.catalog,
    this.hw,
    this.world,
  });

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
    String? docked,
    ResearchState? research,
    List<LogEntry>? events,
    List<Alert>? alerts,
    SectorInfo? sector,
    proto.Hex? homeworld,
    List<Waypoint>? waypoints,
    Map<proto.Hex, proto.SectorState>? sectors,
    Map<proto.Hex, proto.SignalKind>? signals,
    proto.Resources? stock,
    proto.HomeworldCatalog? catalog,
    proto.HomeworldState? hw,
    proto.WorldInfo? world,
  }) =>
      GameState(
        tick: tick ?? this.tick,
        clockSec: clockSec ?? this.clockSec,
        resources: resources ?? this.resources,
        fleets: fleets ?? this.fleets,
        buildQueue: buildQueue ?? this.buildQueue,
        shipyard: shipyard ?? this.shipyard,
        docked: docked ?? this.docked,
        research: research ?? this.research,
        events: events ?? this.events,
        alerts: alerts ?? this.alerts,
        sector: sector ?? this.sector,
        homeworld: homeworld ?? this.homeworld,
        waypoints: waypoints ?? this.waypoints,
        sectors: sectors ?? this.sectors,
        signals: signals ?? this.signals,
        stock: stock ?? this.stock,
        catalog: catalog ?? this.catalog,
        hw: hw ?? this.hw,
        world: world ?? this.world,
      );
}
