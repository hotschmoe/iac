import 'fleet.dart';
import 'hex.dart';
import 'homeworld.dart';
import 'resources.dart';

class GameEvent {
  final int tick;
  final String message;
  final EventLevel level;

  const GameEvent({
    required this.tick,
    required this.message,
    required this.level,
  });
}

enum EventLevel { full, bright, normal, dim }

class Alert {
  final String icon;
  final String message;
  final String detail;
  final AlertLevel level;

  const Alert({
    required this.icon,
    required this.message,
    this.detail = '',
    required this.level,
  });
}

enum AlertLevel { glow, bright, normal, dim }

class SectorInfo {
  final String terrain;
  final String metal;
  final String crystal;
  final String deut;
  final String hostile;
  final String adjacent;
  final String exits;

  const SectorInfo({
    required this.terrain,
    required this.metal,
    required this.crystal,
    required this.deut,
    required this.hostile,
    required this.adjacent,
    required this.exits,
  });
}

class Waypoint {
  final String id;
  final Hex coord;
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
  final List<GameEvent> events;
  final List<Alert> alerts;
  final SectorInfo sector;
  final Hex homeworld;
  final List<Waypoint> waypoints;

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
    List<GameEvent>? events,
    List<Alert>? alerts,
    SectorInfo? sector,
    Hex? homeworld,
    List<Waypoint>? waypoints,
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
      );
}
