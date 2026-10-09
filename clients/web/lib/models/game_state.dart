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

/// A `preview_move` answer and the tick it arrived.
class PreviewEntry {
  final proto.MovePreview preview;
  final int tick;
  const PreviewEntry(this.preview, this.tick);
}

/// Ticks a preview stays on screen, and how soon it is asked for again (the
/// refresh lands well before it lapses, so the readout never blinks out).
const previewFreshTicks = 8;
const previewRefreshTicks = 3;

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

  /// Faint scan contacts' coarse danger (1: T1-3, 2: T4-6, 3: T7-9), by sector.
  final Map<proto.Hex, int> signalBands;

  /// The server's latest `preview_move` per (fleet id, target sector).
  final Map<(int, proto.Hex), PreviewEntry> previews;

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

  /// The announced, unresolved raid.
  final proto.RaidIncomingEvent? raid;

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
    this.signalBands = const {},
    this.previews = const {},
    this.stock = const proto.Resources(),
    this.catalog,
    this.hw,
    this.world,
    this.leaderboard,
    this.raid,
  });

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
    Map<proto.Hex, int>? signalBands,
    Map<(int, proto.Hex), PreviewEntry>? previews,
    proto.Resources? stock,
    proto.HomeworldCatalog? catalog,
    proto.HomeworldState? hw,
    proto.WorldInfo? world,
    proto.LeaderboardReply? leaderboard,
    proto.RaidIncomingEvent? raid,
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
        signalBands: signalBands ?? this.signalBands,
        previews: previews ?? this.previews,
        stock: stock ?? this.stock,
        catalog: catalog ?? this.catalog,
        hw: hw ?? this.hw,
        world: world ?? this.world,
        leaderboard: leaderboard ?? this.leaderboard,
        raid: raid ?? this.raid,
      );
}
