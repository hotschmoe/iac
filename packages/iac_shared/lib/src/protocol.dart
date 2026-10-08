/// Wire protocol types shared between server and client.
/// JSON over WebSocket. Externally-tagged unions match Zig std.json.
library;

import 'dart:convert';

import 'constants.dart';
import 'hex.dart';
import 'scaling.dart';
import 'world.dart';

// ── Client → Server ───────────────────────────────────────────────

/// Coerce JSON map values that may be `Map<dynamic, dynamic>`.
Map<String, dynamic>? _asStringKeyMap(Object? value) {
  if (value == null) return null;
  if (value is Map<String, dynamic>) return value;
  if (value is Map) {
    return value.map((k, v) => MapEntry(k.toString(), v));
  }
  return null;
}

/// Top-level client message (externally tagged).
sealed class ClientMessage {
  const ClientMessage();

  Map<String, dynamic> toJson();

  String encode() => jsonEncode(toJson());

  factory ClientMessage.fromJson(Map<String, dynamic> json) {
    if (json.containsKey('auth') && json['auth'] is Map) {
      return AuthRequest.fromJson(json['auth'] as Map<String, dynamic>);
    }
    if (json.containsKey('policy_update')) {
      return PolicyUpdate.fromJson(
        json['policy_update'] as Map<String, dynamic>,
      );
    }
    if (json.containsKey('request_full_state')) {
      return const RequestFullState();
    }

    // Externally tagged Zig: {"command": {"move": {...}}} or {"command": "stop"}
    if (json.containsKey('command') && json['type'] == null) {
      return Command.fromJson(json['command']);
    }

    // Flat form: {"type": "auth"|"command"|...}
    final type = json['type'] as String?;
    if (type == 'auth') {
      return AuthRequest.fromJson(json);
    }
    if (type == 'command') {
      // {"type":"command","command":"move","fleet_id":1,"target":{...}}
      final cmdName = json['command'];
      if (cmdName is String) {
        return _flatCommand(cmdName, json);
      }
      return Command.fromJson(json['command']);
    }
    if (type == 'move' ||
        type == 'harvest' ||
        type == 'attack' ||
        type == 'recall' ||
        type == 'build' ||
        type == 'research' ||
        type == 'build_ship' ||
        type == 'stop' ||
        type == 'scan' ||
        type == 'collect_salvage' ||
        type == 'cancel_build') {
      return _flatCommand(type!, json);
    }
    if (type == 'request_full_state') return const RequestFullState();
    // Nested Zig-style without type: {"move": {...}}
    for (final key in const [
      'move',
      'harvest',
      'attack',
      'recall',
      'build',
      'research',
      'build_ship',
      'stop',
      'scan',
      'collect_salvage',
      'cancel_build',
    ]) {
      if (json.containsKey(key)) {
        return Command.fromJson(json);
      }
    }
    throw FormatException('Unknown client message: $json');
  }

  static ClientMessage decode(String raw) =>
      ClientMessage.fromJson(jsonDecode(raw) as Map<String, dynamic>);
}

class AuthRequest extends ClientMessage {
  final String playerName;
  final String? token;

  const AuthRequest({required this.playerName, this.token});

  factory AuthRequest.fromJson(Map<String, dynamic> json) => AuthRequest(
        playerName: (json['player_name'] as String?) ??
            (json['playerName'] as String?) ??
            'Admiral',
        token: json['token'] as String?,
      );

  @override
  Map<String, dynamic> toJson() => {
        'type': 'auth',
        'player_name': playerName,
        if (token != null) 'token': token,
      };
}

class PolicyUpdate extends ClientMessage {
  final int fleetId;
  final List<PolicyRule> rules;

  const PolicyUpdate({required this.fleetId, required this.rules});

  factory PolicyUpdate.fromJson(Map<String, dynamic> json) => PolicyUpdate(
        fleetId: (json['fleet_id'] as num).toInt(),
        rules: ((json['rules'] as List?) ?? [])
            .map((e) => PolicyRule.fromJson(e as Map<String, dynamic>))
            .toList(),
      );

  @override
  Map<String, dynamic> toJson() => {
        'policy_update': {
          'fleet_id': fleetId,
          'rules': rules.map((r) => r.toJson()).toList(),
        },
      };
}

class PolicyRule {
  final String condition;
  final String action;
  final int priority;

  const PolicyRule({
    required this.condition,
    required this.action,
    required this.priority,
  });

  factory PolicyRule.fromJson(Map<String, dynamic> json) => PolicyRule(
        condition: json['condition'] as String? ?? '',
        action: json['action'] as String? ?? '',
        priority: (json['priority'] as num?)?.toInt() ?? 0,
      );

  Map<String, dynamic> toJson() => {
        'condition': condition,
        'action': action,
        'priority': priority,
      };
}

class RequestFullState extends ClientMessage {
  const RequestFullState();

  @override
  Map<String, dynamic> toJson() => {'request_full_state': null};
}

// ── Commands (also ClientMessages) ────────────────────────────────

/// Game command. Flat JSON uses `type: command` + `command: <name>`.
sealed class Command extends ClientMessage {
  const Command();

  factory Command.fromJson(dynamic raw) {
    if (raw is String) {
      if (raw == 'stop') return const StopCommand();
      if (raw == 'scan') return const ScanCommand();
      throw FormatException('Unknown command string: $raw');
    }
    final json = raw as Map<String, dynamic>;

    // Externally tagged: {"move": {...}}
    if (json.containsKey('move')) {
      final m = json['move'] as Map<String, dynamic>;
      return MoveCommand(
        fleetId: (m['fleet_id'] as num).toInt(),
        target: Hex.fromJson(m['target'] as Map<String, dynamic>?),
      );
    }
    if (json.containsKey('harvest')) {
      final m = json['harvest'] as Map<String, dynamic>;
      return HarvestCommand(fleetId: (m['fleet_id'] as num).toInt());
    }
    if (json.containsKey('attack')) {
      final m = json['attack'] as Map<String, dynamic>;
      return AttackCommand(
        fleetId: (m['fleet_id'] as num).toInt(),
        targetFleetId: (m['target_fleet_id'] as num).toInt(),
      );
    }
    if (json.containsKey('recall')) {
      final m = json['recall'] as Map<String, dynamic>;
      return RecallCommand(fleetId: (m['fleet_id'] as num).toInt());
    }
    if (json.containsKey('collect_salvage')) {
      final m = json['collect_salvage'] as Map<String, dynamic>;
      return CollectSalvageCommand(fleetId: (m['fleet_id'] as num).toInt());
    }
    if (json.containsKey('build')) {
      final m = json['build'] as Map<String, dynamic>;
      return BuildCommand(
        buildingType: BuildingType.fromWire(m['building_type'] as String?),
      );
    }
    if (json.containsKey('research')) {
      final m = json['research'] as Map<String, dynamic>;
      return ResearchCommand(
        tech: ResearchType.fromWire(m['tech'] as String?),
      );
    }
    if (json.containsKey('build_ship')) {
      final m = json['build_ship'] as Map<String, dynamic>;
      return BuildShipCommand(
        shipClass: ShipClass.fromWire(m['ship_class'] as String?),
        count: (m['count'] as num?)?.toInt() ?? 1,
      );
    }
    if (json.containsKey('cancel_build')) {
      final m = json['cancel_build'] as Map<String, dynamic>;
      return CancelBuildCommand(
        queueType: QueueType.fromWire(m['queue_type'] as String?),
      );
    }
    if (json.containsKey('stop') || json['type'] == 'stop') {
      return const StopCommand();
    }
    if (json.containsKey('scan') || json['type'] == 'scan') {
      return const ScanCommand();
    }

    // Flat form with type field
    final type = json['type'] as String?;
    switch (type) {
      case 'move':
        return MoveCommand(
          fleetId: (json['fleet_id'] as num).toInt(),
          target: Hex.fromJson(json['target'] as Map<String, dynamic>?),
        );
      case 'harvest':
        return HarvestCommand(fleetId: (json['fleet_id'] as num).toInt());
      case 'attack':
        return AttackCommand(
          fleetId: (json['fleet_id'] as num).toInt(),
          targetFleetId: (json['target_fleet_id'] as num).toInt(),
        );
      case 'recall':
        return RecallCommand(fleetId: (json['fleet_id'] as num).toInt());
      case 'collect_salvage':
        return CollectSalvageCommand(
          fleetId: (json['fleet_id'] as num).toInt(),
        );
      case 'build':
        return BuildCommand(
          buildingType:
              BuildingType.fromWire(json['building_type'] as String?),
        );
      case 'research':
        return ResearchCommand(
          tech: ResearchType.fromWire(json['tech'] as String?),
        );
      case 'build_ship':
        return BuildShipCommand(
          shipClass: ShipClass.fromWire(json['ship_class'] as String?),
          count: (json['count'] as num?)?.toInt() ?? 1,
        );
      case 'cancel_build':
        return CancelBuildCommand(
          queueType: QueueType.fromWire(json['queue_type'] as String?),
        );
      case 'stop':
        return const StopCommand();
      case 'scan':
        return const ScanCommand();
    }

    throw FormatException('Unknown command: $json');
  }
}

/// Parse flat `{"type":"command","command":"move",...}` or bare command name.
ClientMessage _flatCommand(String name, Map<String, dynamic> json) {
  switch (name) {
    case 'move':
      return MoveCommand(
        fleetId: (json['fleet_id'] as num).toInt(),
        target: Hex.fromJson(json['target'] as Map<String, dynamic>?),
      );
    case 'harvest':
      return HarvestCommand(fleetId: (json['fleet_id'] as num).toInt());
    case 'attack':
      return AttackCommand(
        fleetId: (json['fleet_id'] as num).toInt(),
        targetFleetId: (json['target_fleet_id'] as num).toInt(),
      );
    case 'recall':
      return RecallCommand(fleetId: (json['fleet_id'] as num).toInt());
    case 'collect_salvage':
      return CollectSalvageCommand(
        fleetId: (json['fleet_id'] as num).toInt(),
      );
    case 'build':
      return BuildCommand(
        buildingType: BuildingType.fromWire(json['building_type'] as String?),
      );
    case 'research':
      return ResearchCommand(
        tech: ResearchType.fromWire(json['tech'] as String?),
      );
    case 'build_ship':
      return BuildShipCommand(
        shipClass: ShipClass.fromWire(json['ship_class'] as String?),
        count: (json['count'] as num?)?.toInt() ?? 1,
      );
    case 'cancel_build':
      return CancelBuildCommand(
        queueType: QueueType.fromWire(json['queue_type'] as String?),
      );
    case 'stop':
      return const StopCommand();
    case 'scan':
      return const ScanCommand();
    default:
      throw FormatException('Unknown command name: $name');
  }
}

class MoveCommand extends Command {
  final int fleetId;
  final Hex target;
  const MoveCommand({required this.fleetId, required this.target});

  @override
  Map<String, dynamic> toJson() => {
        'move': {
          'fleet_id': fleetId,
          'target': target.toJson(),
        },
      };
}

class HarvestCommand extends Command {
  final int fleetId;
  const HarvestCommand({required this.fleetId});

  @override
  Map<String, dynamic> toJson() => {
        'harvest': {'fleet_id': fleetId, 'resource': 'auto'},
      };
}

class AttackCommand extends Command {
  final int fleetId;
  final int targetFleetId;
  const AttackCommand({required this.fleetId, required this.targetFleetId});

  @override
  Map<String, dynamic> toJson() => {
        'attack': {
          'fleet_id': fleetId,
          'target_fleet_id': targetFleetId,
        },
      };
}

class RecallCommand extends Command {
  final int fleetId;
  const RecallCommand({required this.fleetId});

  @override
  Map<String, dynamic> toJson() => {
        'recall': {'fleet_id': fleetId},
      };
}

class CollectSalvageCommand extends Command {
  final int fleetId;
  const CollectSalvageCommand({required this.fleetId});

  @override
  Map<String, dynamic> toJson() => {
        'collect_salvage': {'fleet_id': fleetId},
      };
}

class BuildCommand extends Command {
  final BuildingType buildingType;
  const BuildCommand({required this.buildingType});

  @override
  Map<String, dynamic> toJson() => {
        'build': {'building_type': buildingType.wireName},
      };
}

class ResearchCommand extends Command {
  final ResearchType tech;
  const ResearchCommand({required this.tech});

  @override
  Map<String, dynamic> toJson() => {
        'research': {'tech': tech.wireName},
      };
}

class BuildShipCommand extends Command {
  final ShipClass shipClass;
  final int count;
  const BuildShipCommand({required this.shipClass, this.count = 1});

  @override
  Map<String, dynamic> toJson() => {
        'build_ship': {
          'ship_class': shipClass.wireName,
          'count': count,
        },
      };
}

class CancelBuildCommand extends Command {
  final QueueType queueType;
  const CancelBuildCommand({required this.queueType});

  @override
  Map<String, dynamic> toJson() => {
        'cancel_build': {'queue_type': queueType.wireName},
      };
}

class StopCommand extends Command {
  const StopCommand();

  @override
  Map<String, dynamic> toJson() => {'stop': null};
}

class ScanCommand extends Command {
  const ScanCommand();

  @override
  Map<String, dynamic> toJson() => {'scan': null};
}

// ── Server → Client ───────────────────────────────────────────────

sealed class ServerMessage {
  const ServerMessage();
  Map<String, dynamic> toJson();
  String encode() => jsonEncode(toJson());

  factory ServerMessage.fromJson(Map<String, dynamic> json) {
    if (json.containsKey('auth_result')) {
      final raw = json['auth_result'];
      return AuthResult.fromJson(
        raw is Map<String, dynamic> ? raw : json,
      );
    }
    if (json.containsKey('tick_update')) {
      final raw = json['tick_update'];
      return TickUpdate.fromJson(
        raw is Map<String, dynamic> ? raw : json,
      );
    }
    if (json.containsKey('full_state')) {
      final raw = json['full_state'];
      return FullState.fromJson(
        raw is Map<String, dynamic> ? raw : json,
      );
    }
    if (json.containsKey('event')) {
      final raw = json['event'];
      return GameEvent.fromJson(
        raw is Map<String, dynamic> ? raw : json,
      );
    }
    if (json.containsKey('error')) {
      final raw = json['error'];
      return ErrorMessage.fromJson(
        raw is Map<String, dynamic> ? raw : json,
      );
    }
    // Flat type form
    switch (json['type'] as String?) {
      case 'auth_result':
        return AuthResult.fromJson(json);
      case 'tick_update':
        return TickUpdate.fromJson(json);
      case 'full_state':
        return FullState.fromJson(json);
      case 'event':
        return GameEvent.fromJson(json);
      case 'error':
        return ErrorMessage.fromJson(json);
    }
    throw FormatException('Unknown server message: $json');
  }

  static ServerMessage decode(String raw) =>
      ServerMessage.fromJson(jsonDecode(raw) as Map<String, dynamic>);
}

class AuthResult extends ServerMessage {
  final bool success;
  final int? playerId;
  final String? token;
  final String? message;

  const AuthResult({
    required this.success,
    this.playerId,
    this.token,
    this.message,
  });

  factory AuthResult.fromJson(Map<String, dynamic> json) => AuthResult(
        success: json['success'] as bool? ?? false,
        playerId: (json['player_id'] as num?)?.toInt(),
        token: json['token'] as String?,
        message: json['message'] as String?,
      );

  @override
  Map<String, dynamic> toJson() => {
        'auth_result': {
          'success': success,
          if (playerId != null) 'player_id': playerId,
          if (token != null) 'token': token,
          if (message != null) 'message': message,
        },
      };
}

class TickUpdate extends ServerMessage {
  final int tick;
  final PlayerState? player;
  final List<FleetState>? fleetUpdates;
  final List<SectorState>? sectorUpdates;
  final HomeworldState? homeworldUpdate;
  final List<GameEvent>? events;

  const TickUpdate({
    required this.tick,
    this.player,
    this.fleetUpdates,
    this.sectorUpdates,
    this.homeworldUpdate,
    this.events,
  });

  factory TickUpdate.fromJson(Map<String, dynamic> json) => TickUpdate(
        tick: (json['tick'] as num).toInt(),
        player: json['player'] != null
            ? PlayerState.fromJson(json['player'] as Map<String, dynamic>)
            : null,
        fleetUpdates: (json['fleet_updates'] as List?)
            ?.map((e) => FleetState.fromJson(e as Map<String, dynamic>))
            .toList(),
        sectorUpdates: (json['sector_updates'] as List?)
            ?.map((e) => SectorState.fromJson(e as Map<String, dynamic>))
            .toList(),
        homeworldUpdate: json['homeworld_update'] != null
            ? HomeworldState.fromJson(
                json['homeworld_update'] as Map<String, dynamic>,
              )
            : null,
        events: (json['events'] as List?)
            ?.map((e) => GameEvent.fromJson(e as Map<String, dynamic>))
            .toList(),
      );

  @override
  Map<String, dynamic> toJson() => {
        'tick_update': {
          'tick': tick,
          if (player != null) 'player': player!.toJson(),
          if (fleetUpdates != null)
            'fleet_updates': fleetUpdates!.map((e) => e.toJson()).toList(),
          if (sectorUpdates != null)
            'sector_updates': sectorUpdates!.map((e) => e.toJson()).toList(),
          if (homeworldUpdate != null)
            'homeworld_update': homeworldUpdate!.toJson(),
          if (events != null) 'events': events!.map((e) => e.toJson()).toList(),
        },
      };
}

/// Full game state snapshot (`full_state` message).
class FullState extends ServerMessage {
  final int tick;
  final PlayerState player;
  final List<FleetState> fleets;
  final HomeworldState homeworld;
  final List<SectorState> knownSectors;

  const FullState({
    required this.tick,
    required this.player,
    required this.fleets,
    required this.homeworld,
    required this.knownSectors,
  });

  factory FullState.fromJson(Map<String, dynamic> json) => FullState(
        tick: (json['tick'] as num).toInt(),
        player: PlayerState.fromJson(_asStringKeyMap(json['player'])!),
        fleets: ((json['fleets'] as List?) ?? [])
            .map((e) => FleetState.fromJson(_asStringKeyMap(e)!))
            .toList(),
        homeworld: HomeworldState.fromJson(_asStringKeyMap(json['homeworld'])!),
        knownSectors: ((json['known_sectors'] as List?) ?? [])
            .map((e) => SectorState.fromJson(_asStringKeyMap(e)!))
            .toList(),
      );

  /// Back-compat alias constructor name.
  factory FullState.game({
    required int tick,
    required PlayerState player,
    required List<FleetState> fleets,
    required HomeworldState homeworld,
    required List<SectorState> knownSectors,
  }) =>
      FullState(
        tick: tick,
        player: player,
        fleets: fleets,
        homeworld: homeworld,
        knownSectors: knownSectors,
      );

  @override
  Map<String, dynamic> toJson() => {
        'full_state': {
          'tick': tick,
          'player': player.toJson(),
          'fleets': fleets.map((e) => e.toJson()).toList(),
          'homeworld': homeworld.toJson(),
          'known_sectors': knownSectors.map((e) => e.toJson()).toList(),
        },
      };
}

/// Alias for [FullState] (Zig / older docs).
typedef FullGameState = FullState;

// ── State Types ───────────────────────────────────────────────────

class PlayerState {
  final int id;
  final String name;
  final ResourceAmounts resources;
  final Hex homeworld;

  const PlayerState({
    required this.id,
    required this.name,
    required this.resources,
    required this.homeworld,
  });

  factory PlayerState.fromJson(Map<String, dynamic> json) => PlayerState(
        id: (json['id'] as num).toInt(),
        name: json['name'] as String? ?? '',
        resources: ResourceAmounts.fromJson(_asStringKeyMap(json['resources'])),
        homeworld: Hex.fromJson(_asStringKeyMap(json['homeworld'])),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'name': name,
        'resources': resources.toJson(),
        'homeworld': homeworld.toJson(),
      };
}

enum FleetStatus {
  idle,
  moving,
  harvesting,
  inCombat,
  returning,
  docked;

  String get wireName => switch (this) {
        FleetStatus.inCombat => 'in_combat',
        _ => name,
      };

  static FleetStatus fromWire(String? name, {FleetStatus fallback = FleetStatus.idle}) {
    if (name == null) return fallback;
    return switch (name) {
      'idle' => FleetStatus.idle,
      'moving' => FleetStatus.moving,
      'harvesting' => FleetStatus.harvesting,
      'in_combat' || 'inCombat' => FleetStatus.inCombat,
      'returning' => FleetStatus.returning,
      'docked' => FleetStatus.docked,
      _ => fallback,
    };
  }
}

class ShipState {
  final int id;
  final ShipClass class_;
  final double hull;
  final double hullMax;
  final double shield;
  final double shieldMax;
  final double weaponPower;

  const ShipState({
    required this.id,
    required this.class_,
    required this.hull,
    required this.hullMax,
    required this.shield,
    required this.shieldMax,
    required this.weaponPower,
  });

  factory ShipState.fromJson(Map<String, dynamic> json) => ShipState(
        id: (json['id'] as num).toInt(),
        class_: ShipClass.fromWire(json['class'] as String?),
        hull: (json['hull'] as num?)?.toDouble() ?? 0,
        hullMax: (json['hull_max'] as num?)?.toDouble() ?? 0,
        shield: (json['shield'] as num?)?.toDouble() ?? 0,
        shieldMax: (json['shield_max'] as num?)?.toDouble() ?? 0,
        weaponPower: (json['weapon_power'] as num?)?.toDouble() ?? 0,
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'class': class_.wireName,
        'hull': hull,
        'hull_max': hullMax,
        'shield': shield,
        'shield_max': shieldMax,
        'weapon_power': weaponPower,
      };
}

class FleetState {
  final int id;
  final Hex location;
  final FleetStatus state;
  final List<ShipState> ships;
  final ResourceAmounts cargo;
  final double fuel;
  final double fuelMax;
  final int cooldownRemaining;
  final String? name;

  const FleetState({
    required this.id,
    required this.location,
    required this.state,
    required this.ships,
    required this.cargo,
    required this.fuel,
    required this.fuelMax,
    this.cooldownRemaining = 0,
    this.name,
  });

  factory FleetState.fromJson(Map<String, dynamic> json) => FleetState(
        id: (json['id'] as num).toInt(),
        location: Hex.fromJson(_asStringKeyMap(json['location'])),
        state: FleetStatus.fromWire(
          (json['state'] as String?) ?? (json['status'] as String?),
        ),
        ships: ((json['ships'] as List?) ?? [])
            .map((e) => ShipState.fromJson(_asStringKeyMap(e)!))
            .toList(),
        cargo: ResourceAmounts.fromJson(_asStringKeyMap(json['cargo'])),
        fuel: (json['fuel'] as num?)?.toDouble() ?? 0,
        fuelMax: (json['fuel_max'] as num?)?.toDouble() ?? 0,
        cooldownRemaining: (json['cooldown_remaining'] as num?)?.toInt() ?? 0,
        name: json['name'] as String?,
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'location': location.toJson(),
        'state': state.wireName,
        'ships': ships.map((e) => e.toJson()).toList(),
        'cargo': cargo.toJson(),
        'fuel': fuel,
        'fuel_max': fuelMax,
        'cooldown_remaining': cooldownRemaining,
        if (name != null) 'name': name,
      };
}

class SectorResources {
  final Density metal;
  final Density crystal;
  final Density deuterium;

  const SectorResources({
    required this.metal,
    required this.crystal,
    required this.deuterium,
  });

  factory SectorResources.fromJson(Map<String, dynamic> json) =>
      SectorResources(
        metal: Density.fromWire(json['metal'] as String?),
        crystal: Density.fromWire(json['crystal'] as String?),
        deuterium: Density.fromWire(
          (json['deuterium'] as String?) ?? (json['deut'] as String?),
        ),
      );

  Map<String, dynamic> toJson() => {
        'metal': metal.wireName,
        'crystal': crystal.wireName,
        'deuterium': deuterium.wireName,
      };
}

class NpcShipInfo {
  final ShipClass class_;
  final int count;

  const NpcShipInfo({required this.class_, required this.count});

  factory NpcShipInfo.fromJson(Map<String, dynamic> json) => NpcShipInfo(
        class_: ShipClass.fromWire(json['class'] as String?),
        count: (json['count'] as num?)?.toInt() ?? 0,
      );

  Map<String, dynamic> toJson() => {
        'class': class_.wireName,
        'count': count,
      };
}

class NpcFleetInfo {
  final int id;
  final List<NpcShipInfo> ships;
  final NpcBehavior behavior;

  const NpcFleetInfo({
    required this.id,
    required this.ships,
    required this.behavior,
  });

  factory NpcFleetInfo.fromJson(Map<String, dynamic> json) => NpcFleetInfo(
        id: (json['id'] as num).toInt(),
        ships: ((json['ships'] as List?) ?? [])
            .map((e) => NpcShipInfo.fromJson(e as Map<String, dynamic>))
            .toList(),
        behavior: NpcBehavior.fromWire(json['behavior'] as String?),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'ships': ships.map((e) => e.toJson()).toList(),
        'behavior': behavior.wireName,
      };
}

class ShipClassCounts {
  final int scout;
  final int corvette;
  final int frigate;
  final int cruiser;
  final int hauler;

  const ShipClassCounts({
    this.scout = 0,
    this.corvette = 0,
    this.frigate = 0,
    this.cruiser = 0,
    this.hauler = 0,
  });

  factory ShipClassCounts.fromJson(Map<String, dynamic>? json) {
    if (json == null) return const ShipClassCounts();
    return ShipClassCounts(
      scout: (json['scout'] as num?)?.toInt() ?? 0,
      corvette: (json['corvette'] as num?)?.toInt() ?? 0,
      frigate: (json['frigate'] as num?)?.toInt() ?? 0,
      cruiser: (json['cruiser'] as num?)?.toInt() ?? 0,
      hauler: (json['hauler'] as num?)?.toInt() ?? 0,
    );
  }

  Map<String, dynamic> toJson() => {
        'scout': scout,
        'corvette': corvette,
        'frigate': frigate,
        'cruiser': cruiser,
        'hauler': hauler,
      };
}

class FleetBrief {
  final int id;
  final String ownerName;
  final int shipCount;
  final ShipClassCounts shipClasses;

  const FleetBrief({
    required this.id,
    required this.ownerName,
    required this.shipCount,
    this.shipClasses = const ShipClassCounts(),
  });

  factory FleetBrief.fromJson(Map<String, dynamic> json) => FleetBrief(
        id: (json['id'] as num).toInt(),
        ownerName: json['owner_name'] as String? ?? '',
        shipCount: (json['ship_count'] as num?)?.toInt() ?? 0,
        shipClasses: ShipClassCounts.fromJson(
          json['ship_classes'] as Map<String, dynamic>?,
        ),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'owner_name': ownerName,
        'ship_count': shipCount,
        'ship_classes': shipClasses.toJson(),
      };
}

class SectorState {
  final Hex location;
  final TerrainType terrain;
  final SectorResources resources;
  final List<Hex> connections;
  final List<NpcFleetInfo>? hostiles;
  final List<FleetBrief>? playerFleets;
  final ResourceAmounts? salvage;

  const SectorState({
    required this.location,
    required this.terrain,
    required this.resources,
    required this.connections,
    this.hostiles,
    this.playerFleets,
    this.salvage,
  });

  factory SectorState.fromJson(Map<String, dynamic> json) => SectorState(
        location: Hex.fromJson(json['location'] as Map<String, dynamic>?),
        terrain: TerrainType.fromWire(json['terrain'] as String?),
        resources: SectorResources.fromJson(
          json['resources'] as Map<String, dynamic>? ?? {},
        ),
        connections: ((json['connections'] as List?) ?? [])
            .map((e) => Hex.fromJson(e as Map<String, dynamic>))
            .toList(),
        hostiles: (json['hostiles'] as List?)
            ?.map((e) => NpcFleetInfo.fromJson(e as Map<String, dynamic>))
            .toList(),
        playerFleets: (json['player_fleets'] as List?)
            ?.map((e) => FleetBrief.fromJson(e as Map<String, dynamic>))
            .toList(),
        salvage: json['salvage'] != null
            ? ResourceAmounts.fromJson(json['salvage'] as Map<String, dynamic>)
            : null,
      );

  Map<String, dynamic> toJson() => {
        'location': location.toJson(),
        'terrain': terrain.wireName,
        'resources': resources.toJson(),
        'connections': connections.map((e) => e.toJson()).toList(),
        if (hostiles != null)
          'hostiles': hostiles!.map((e) => e.toJson()).toList(),
        if (playerFleets != null)
          'player_fleets': playerFleets!.map((e) => e.toJson()).toList(),
        if (salvage != null) 'salvage': salvage!.toJson(),
      };
}

class BuildingState {
  final BuildingType buildingType;
  final int level;

  const BuildingState({required this.buildingType, required this.level});

  factory BuildingState.fromJson(Map<String, dynamic> json) => BuildingState(
        buildingType:
            BuildingType.fromWire(json['building_type'] as String?),
        level: (json['level'] as num?)?.toInt() ?? 0,
      );

  Map<String, dynamic> toJson() => {
        'building_type': buildingType.wireName,
        'level': level,
      };
}

class ResearchState {
  final ResearchType tech;
  final int level;

  const ResearchState({required this.tech, required this.level});

  factory ResearchState.fromJson(Map<String, dynamic> json) => ResearchState(
        tech: ResearchType.fromWire(json['tech'] as String?),
        level: (json['level'] as num?)?.toInt() ?? 0,
      );

  Map<String, dynamic> toJson() => {
        'tech': tech.wireName,
        'level': level,
      };
}

class BuildQueueItem {
  final BuildingType buildingType;
  final int targetLevel;
  final int startTick;
  final int endTick;

  const BuildQueueItem({
    required this.buildingType,
    required this.targetLevel,
    required this.startTick,
    required this.endTick,
  });

  factory BuildQueueItem.fromJson(Map<String, dynamic> json) => BuildQueueItem(
        buildingType:
            BuildingType.fromWire(json['building_type'] as String?),
        targetLevel: (json['target_level'] as num).toInt(),
        startTick: (json['start_tick'] as num).toInt(),
        endTick: (json['end_tick'] as num).toInt(),
      );

  Map<String, dynamic> toJson() => {
        'building_type': buildingType.wireName,
        'target_level': targetLevel,
        'start_tick': startTick,
        'end_tick': endTick,
      };
}

class ShipyardQueueItem {
  final ShipClass shipClass;
  final int count;
  final int built;
  final int startTick;
  final int endTick;

  const ShipyardQueueItem({
    required this.shipClass,
    required this.count,
    required this.built,
    required this.startTick,
    required this.endTick,
  });

  factory ShipyardQueueItem.fromJson(Map<String, dynamic> json) =>
      ShipyardQueueItem(
        shipClass: ShipClass.fromWire(json['ship_class'] as String?),
        count: (json['count'] as num).toInt(),
        built: (json['built'] as num?)?.toInt() ?? 0,
        startTick: (json['start_tick'] as num).toInt(),
        endTick: (json['end_tick'] as num).toInt(),
      );

  Map<String, dynamic> toJson() => {
        'ship_class': shipClass.wireName,
        'count': count,
        'built': built,
        'start_tick': startTick,
        'end_tick': endTick,
      };
}

class ResearchItem {
  final ResearchType tech;
  final int targetLevel;
  final int startTick;
  final int endTick;

  const ResearchItem({
    required this.tech,
    required this.targetLevel,
    required this.startTick,
    required this.endTick,
  });

  factory ResearchItem.fromJson(Map<String, dynamic> json) => ResearchItem(
        tech: ResearchType.fromWire(json['tech'] as String?),
        targetLevel: (json['target_level'] as num).toInt(),
        startTick: (json['start_tick'] as num).toInt(),
        endTick: (json['end_tick'] as num).toInt(),
      );

  Map<String, dynamic> toJson() => {
        'tech': tech.wireName,
        'target_level': targetLevel,
        'start_tick': startTick,
        'end_tick': endTick,
      };
}

class HomeworldState {
  final Hex location;
  final List<BuildingState> buildings;
  final List<ResearchState> research;
  final BuildQueueItem? buildQueue;
  final ShipyardQueueItem? shipyardQueue;
  final ResearchItem? researchActive;
  final List<ShipState> dockedShips;

  const HomeworldState({
    required this.location,
    required this.buildings,
    required this.research,
    this.buildQueue,
    this.shipyardQueue,
    this.researchActive,
    this.dockedShips = const [],
  });

  factory HomeworldState.fromJson(Map<String, dynamic> json) => HomeworldState(
        location: Hex.fromJson(_asStringKeyMap(json['location'])),
        buildings: ((json['buildings'] as List?) ?? [])
            .map((e) => BuildingState.fromJson(_asStringKeyMap(e)!))
            .toList(),
        research: ((json['research'] as List?) ?? [])
            .map((e) => ResearchState.fromJson(_asStringKeyMap(e)!))
            .toList(),
        buildQueue: json['build_queue'] != null
            ? BuildQueueItem.fromJson(_asStringKeyMap(json['build_queue'])!)
            : null,
        shipyardQueue: json['shipyard_queue'] != null
            ? ShipyardQueueItem.fromJson(
                _asStringKeyMap(json['shipyard_queue'])!,
              )
            : null,
        researchActive: json['research_active'] != null
            ? ResearchItem.fromJson(_asStringKeyMap(json['research_active'])!)
            : null,
        dockedShips: ((json['docked_ships'] as List?) ?? [])
            .map((e) => ShipState.fromJson(_asStringKeyMap(e)!))
            .toList(),
      );

  Map<String, dynamic> toJson() => {
        'location': location.toJson(),
        'buildings': buildings.map((e) => e.toJson()).toList(),
        'research': research.map((e) => e.toJson()).toList(),
        if (buildQueue != null) 'build_queue': buildQueue!.toJson(),
        if (shipyardQueue != null) 'shipyard_queue': shipyardQueue!.toJson(),
        if (researchActive != null)
          'research_active': researchActive!.toJson(),
        'docked_ships': dockedShips.map((e) => e.toJson()).toList(),
      };
}

// ── Events ────────────────────────────────────────────────────────

enum AlertLevel {
  info,
  warning,
  critical;

  String get wireName => name;

  static AlertLevel fromWire(String? name, {AlertLevel fallback = AlertLevel.info}) {
    if (name == null) return fallback;
    for (final a in AlertLevel.values) {
      if (a.name == name) return a;
    }
    return fallback;
  }
}

class GameEvent extends ServerMessage {
  final int tick;
  final Map<String, dynamic> kind;

  const GameEvent({required this.tick, required this.kind});

  factory GameEvent.fromJson(Map<String, dynamic> json) => GameEvent(
        tick: (json['tick'] as num).toInt(),
        kind: (json['kind'] as Map<String, dynamic>?) ?? {},
      );

  @override
  Map<String, dynamic> toJson() => {
        // When sent as top-level `event` message:
        // network wraps; as embedded in tick_update it's bare.
        'tick': tick,
        'kind': kind,
      };

  Map<String, dynamic> toEnvelope() => {
        'event': toJson(),
      };

  // Convenience constructors
  factory GameEvent.alert({
    required int tick,
    required String message,
    AlertLevel level = AlertLevel.info,
    Hex? sector,
    int? fleetId,
  }) =>
      GameEvent(
        tick: tick,
        kind: {
          'alert': {
            'level': level.wireName,
            'message': message,
            if (sector != null) 'sector': sector.toJson(),
            if (fleetId != null) 'fleet_id': fleetId,
          },
        },
      );

  factory GameEvent.resourceHarvested({
    required int tick,
    required int fleetId,
    required String resourceType,
    required double amount,
  }) =>
      GameEvent(
        tick: tick,
        kind: {
          'resource_harvested': {
            'fleet_id': fleetId,
            'resource_type': resourceType,
            'amount': amount,
          },
        },
      );

  factory GameEvent.sectorEntered({
    required int tick,
    required int fleetId,
    required Hex sector,
    required bool firstVisit,
  }) =>
      GameEvent(
        tick: tick,
        kind: {
          'sector_entered': {
            'fleet_id': fleetId,
            'sector': sector.toJson(),
            'first_visit': firstVisit,
          },
        },
      );

  factory GameEvent.fleetArrived({
    required int tick,
    required int fleetId,
    required Hex sector,
  }) =>
      GameEvent(
        tick: tick,
        kind: {
          'fleet_arrived': {
            'fleet_id': fleetId,
            'sector': sector.toJson(),
          },
        },
      );

  factory GameEvent.buildingCompleted({
    required int tick,
    required BuildingType buildingType,
    required int newLevel,
    int? playerId,
  }) =>
      GameEvent(
        tick: tick,
        kind: {
          'building_completed': {
            'building_type': buildingType.wireName,
            'new_level': newLevel,
            if (playerId != null) 'player_id': playerId,
          },
        },
      );

  factory GameEvent.researchCompleted({
    required int tick,
    required ResearchType tech,
    required int newLevel,
    int? playerId,
  }) =>
      GameEvent(
        tick: tick,
        kind: {
          'research_completed': {
            'tech': tech.wireName,
            'new_level': newLevel,
            if (playerId != null) 'player_id': playerId,
          },
        },
      );

  factory GameEvent.shipBuilt({
    required int tick,
    required ShipClass shipClass,
    required int count,
    int? playerId,
  }) =>
      GameEvent(
        tick: tick,
        kind: {
          'ship_built': {
            'ship_class': shipClass.wireName,
            'count': count,
            if (playerId != null) 'player_id': playerId,
          },
        },
      );

  factory GameEvent.combatStarted({
    required int tick,
    required int playerFleetId,
    required int enemyFleetId,
    required Hex sector,
  }) =>
      GameEvent(
        tick: tick,
        kind: {
          'combat_started': {
            'player_fleet_id': playerFleetId,
            'enemy_fleet_id': enemyFleetId,
            'sector': sector.toJson(),
          },
        },
      );

  factory GameEvent.combatEnded({
    required int tick,
    required Hex sector,
    required bool playerVictory,
  }) =>
      GameEvent(
        tick: tick,
        kind: {
          'combat_ended': {
            'sector': sector.toJson(),
            'player_victory': playerVictory,
          },
        },
      );

  factory GameEvent.shipDestroyed({
    required int tick,
    required int shipId,
    required ShipClass shipClass,
    required int ownerFleetId,
    required bool isNpc,
  }) =>
      GameEvent(
        tick: tick,
        kind: {
          'ship_destroyed': {
            'ship_id': shipId,
            'ship_class': shipClass.wireName,
            'owner_fleet_id': ownerFleetId,
            'is_npc': isNpc,
          },
        },
      );

  factory GameEvent.fleetDestroyed({
    required int tick,
    required int fleetId,
    required bool isNpc,
    ResourceAmounts salvage = ResourceAmounts.zero,
  }) =>
      GameEvent(
        tick: tick,
        kind: {
          'fleet_destroyed': {
            'fleet_id': fleetId,
            'is_npc': isNpc,
            'salvage': salvage.toJson(),
          },
        },
      );

  factory GameEvent.combatRound({
    required int tick,
    required int attackerShipId,
    required int targetShipId,
    required double damage,
    required double shieldAbsorbed,
    required double hullDamage,
    required bool rapidFire,
  }) =>
      GameEvent(
        tick: tick,
        kind: {
          'combat_round': {
            'attacker_ship_id': attackerShipId,
            'target_ship_id': targetShipId,
            'damage': damage,
            'shield_absorbed': shieldAbsorbed,
            'hull_damage': hullDamage,
            'rapid_fire': rapidFire,
          },
        },
      );

  factory GameEvent.salvageCollected({
    required int tick,
    required int fleetId,
    required ResourceAmounts resources,
  }) =>
      GameEvent(
        tick: tick,
        kind: {
          'salvage_collected': {
            'fleet_id': fleetId,
            'resources': resources.toJson(),
          },
        },
      );
}

// ── Errors ────────────────────────────────────────────────────────

enum ErrorCode {
  invalidCommand(1000),
  invalidTarget(1001),
  noConnection(1002),
  insufficientFuel(1003),
  onCooldown(1004),
  fleetNotFound(1005),
  notInSector(1006),
  noResources(1007),
  cargoFull(1008),
  prerequisitesNotMet(1009),
  maxLevelReached(1010),
  queueFull(1011),
  shipLocked(1012),
  noShipyard(1013),
  noResearchLab(1014),
  fleetLimitReached(1015),
  authFailed(2000),
  alreadyAuthenticated(2001),
  serverError(5000);

  final int code;
  const ErrorCode(this.code);

  String get wireName => switch (this) {
        ErrorCode.invalidCommand => 'invalid_command',
        ErrorCode.invalidTarget => 'invalid_target',
        ErrorCode.noConnection => 'no_connection',
        ErrorCode.insufficientFuel => 'insufficient_fuel',
        ErrorCode.onCooldown => 'on_cooldown',
        ErrorCode.fleetNotFound => 'fleet_not_found',
        ErrorCode.notInSector => 'not_in_sector',
        ErrorCode.noResources => 'no_resources',
        ErrorCode.cargoFull => 'cargo_full',
        ErrorCode.prerequisitesNotMet => 'prerequisites_not_met',
        ErrorCode.maxLevelReached => 'max_level_reached',
        ErrorCode.queueFull => 'queue_full',
        ErrorCode.shipLocked => 'ship_locked',
        ErrorCode.noShipyard => 'no_shipyard',
        ErrorCode.noResearchLab => 'no_research_lab',
        ErrorCode.fleetLimitReached => 'fleet_limit_reached',
        ErrorCode.authFailed => 'auth_failed',
        ErrorCode.alreadyAuthenticated => 'already_authenticated',
        ErrorCode.serverError => 'server_error',
      };
}

class ErrorMessage extends ServerMessage {
  final ErrorCode code;
  final String message;

  const ErrorMessage({required this.code, required this.message});

  factory ErrorMessage.fromJson(Map<String, dynamic> json) {
    final codeRaw = json['code'];
    ErrorCode code = ErrorCode.serverError;
    if (codeRaw is num) {
      for (final c in ErrorCode.values) {
        if (c.code == codeRaw.toInt()) {
          code = c;
          break;
        }
      }
    } else if (codeRaw is String) {
      for (final c in ErrorCode.values) {
        if (c.wireName == codeRaw || c.name == codeRaw) {
          code = c;
          break;
        }
      }
    }
    return ErrorMessage(
      code: code,
      message: json['message'] as String? ?? 'error',
    );
  }

  @override
  Map<String, dynamic> toJson() => {
        'error': {
          'code': code.code,
          'message': message,
        },
      };
}
