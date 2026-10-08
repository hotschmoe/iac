// Wire protocol, mirroring shared/src/protocol.rs (the source of truth).
//
// Every type has fromJson / toJson using the exact tags and field names serde
// produces. Golden fixtures in <repo>/fixtures/protocol are decoded and
// re-encoded by test/protocol_fixtures_test.dart, so drift fails the build.
//
// Decoding is strict: unknown tags / enum names throw FormatException.
// Optional Rust fields (Option<T>, skip_serializing_if) are nullable here and
// omitted from the encoded JSON when null.

import 'dart:convert';

import 'hex.dart';

export 'hex.dart';

typedef Json = Map<String, dynamic>;

// ── Decode helpers ────────────────────────────────────────────────

Json _obj(Object? v) => v as Json;
double _f(Object? v) => (v as num).toDouble();
int _i(Object? v) => v as int;
List<T> _list<T>(Object? v, T Function(Object?) f) => [for (final e in v as List) f(e)];
T? _opt<T>(Object? v, T Function(Object?) f) => v == null ? null : f(v);

String _pascal(String n) => n[0].toUpperCase() + n.substring(1);
String _snake(String n) =>
    n.replaceAllMapped(RegExp('[A-Z]'), (m) => '_${m[0]!.toLowerCase()}');

T _enumFrom<T extends Enum>(List<T> values, Object? wire, String Function(String) enc) {
  for (final v in values) {
    if (enc(v.name) == wire) return v;
  }
  throw FormatException('unknown ${T.toString()} "$wire"');
}

void _put(Json m, String key, Object? v) {
  if (v != null) m[key] = v;
}

// ── Plain enums (serde default: PascalCase variant names) ────────

enum ShipClass {
  scout('Scout'),
  corvette('Corvette'),
  frigate('Frigate'),
  cruiser('Cruiser'),
  hauler('Hauler');

  final String label;
  const ShipClass(this.label);

  String get wire => _pascal(name);
  String toJson() => wire;
  static ShipClass fromJson(Object? v) => _enumFrom(values, v, _pascal);
}

enum Density {
  none,
  sparse,
  moderate,
  rich,
  pristine;

  String get wire => _pascal(name);
  String get label => wire;
  String toJson() => wire;
  static Density fromJson(Object? v) => _enumFrom(values, v, _pascal);
}

enum TerrainType {
  empty('Empty Space'),
  asteroidField('Asteroid Field'),
  nebula('Nebula'),
  debrisField('Debris Field'),
  anomaly('Anomaly');

  final String label;
  const TerrainType(this.label);

  String get wire => _pascal(name);
  String toJson() => wire;
  static TerrainType fromJson(Object? v) => _enumFrom(values, v, _pascal);
}

enum FleetStatus {
  idle,
  moving,
  harvesting,
  inCombat,
  returning,
  docked,
  exploring;

  String get wire => _pascal(name);
  String toJson() => wire;
  static FleetStatus fromJson(Object? v) => _enumFrom(values, v, _pascal);
}

enum NpcBehavior {
  passive,
  patrol,
  aggressive,
  swarm;

  String get wire => _pascal(name);
  String toJson() => wire;
  static NpcBehavior fromJson(Object? v) => _enumFrom(values, v, _pascal);
}

enum SignalKind {
  richOre('rich ore signature'),
  hostileMass('hostile mass reading'),
  derelict('derelict transponder'),
  anomaly('anomalous emissions');

  final String label;
  const SignalKind(this.label);

  String get wire => _pascal(name);
  String toJson() => wire;
  static SignalKind fromJson(Object? v) => _enumFrom(values, v, _pascal);
}

enum AlertLevel {
  info,
  warning,
  critical;

  String get wire => _pascal(name);
  String toJson() => wire;
  static AlertLevel fromJson(Object? v) => _enumFrom(values, v, _pascal);
}

enum HarvestResource {
  metal,
  crystal,
  deuterium,
  auto;

  String get wire => _pascal(name);
  String toJson() => wire;
  static HarvestResource fromJson(Object? v) => _enumFrom(values, v, _pascal);
}

enum QueueType {
  building,
  ship,
  research;

  String get wire => _pascal(name);
  String toJson() => wire;
  static QueueType fromJson(Object? v) => _enumFrom(values, v, _pascal);
}

enum BuildingType {
  metalMine('Metal Mine'),
  crystalMine('Crystal Mine'),
  deuteriumSynthesizer('Deut Synthesizer'),
  shipyard('Shipyard'),
  researchLab('Research Lab'),
  fuelDepot('Fuel Depot'),
  sensorArray('Sensor Array'),
  defenseGrid('Defense Grid');

  final String label;
  const BuildingType(this.label);

  String get wire => _pascal(name);
  String toJson() => wire;
  static BuildingType fromJson(Object? v) => _enumFrom(values, v, _pascal);
}

enum ResearchType {
  fuelEfficiency('Fuel Efficiency'),
  extendedFuelTanks('Extended Tanks'),
  reinforcedHulls('Reinforced Hulls'),
  advancedShields('Advanced Shields'),
  weaponsResearch('Weapons Research'),
  navigation('Navigation'),
  harvestingEfficiency('Harvesting Eff.'),
  corvetteTech('Corvette Tech'),
  frigateTech('Frigate Tech'),
  cruiserTech('Cruiser Tech'),
  haulerTech('Hauler Tech'),
  emergencyJump('Emergency Jump');

  final String label;
  const ResearchType(this.label);

  String get wire => _pascal(name);
  String toJson() => wire;
  static ResearchType fromJson(Object? v) => _enumFrom(values, v, _pascal);
}

/// Serialized as the variant name (the numeric repr is NOT on the wire).
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
  resourceNotPresent(1016),
  authFailed(2000),
  alreadyAuthenticated(2001),
  invalidName(2002),
  tokenRequired(2003),
  invalidToken(2004),
  serverError(5000);

  final int code;
  const ErrorCode(this.code);

  String get wire => _pascal(name);
  String toJson() => wire;
  static ErrorCode fromJson(Object? v) => _enumFrom(values, v, _pascal);
}

// ── snake_case enums (#[serde(rename_all = "snake_case")]) ───────

enum PolicyPreset {
  manual('manual'),
  prospect('prospect'),
  mineAndReturn('mine+return'),
  salvageAndSites('salvage+sites'),
  patrolHome('patrol home');

  /// Display label (PolicyPreset::label in Rust).
  final String label;
  const PolicyPreset(this.label);

  String get wire => _snake(name);
  String toJson() => wire;
  static PolicyPreset fromJson(Object? v) => _enumFrom(values, v, _snake);
}

enum SiteRisk {
  quiet,
  uneasy,
  hot;

  String get wire => _snake(name);
  String get label => wire;
  String toJson() => wire;
  static SiteRisk fromJson(Object? v) => _enumFrom(values, v, _snake);
}

// ── Shared structs ────────────────────────────────────────────────

class Resources {
  final double metal;
  final double crystal;
  final double deuterium;

  const Resources({this.metal = 0, this.crystal = 0, this.deuterium = 0});

  factory Resources.fromJson(Object? json) {
    final m = _obj(json);
    return Resources(metal: _f(m['metal']), crystal: _f(m['crystal']), deuterium: _f(m['deuterium']));
  }

  Json toJson() => {'metal': metal, 'crystal': crystal, 'deuterium': deuterium};

  double get total => metal + crystal + deuterium;
}

// ── Client → Server ───────────────────────────────────────────────

/// {"type": "auth" | "command" | "policy_update" | "request_full_state", ...}
sealed class ClientMessage {
  const ClientMessage();

  Json toJson();
  String encode() => jsonEncode(toJson());

  static ClientMessage fromJson(Object? json) {
    final m = _obj(json);
    return switch (m['type']) {
      'auth' => AuthRequest.fromJson(m),
      'command' => CommandMessage(Command.fromJson(m)),
      'policy_update' => PolicyUpdate.fromJson(m),
      'request_full_state' => const RequestFullState(),
      final t => throw FormatException('unknown ClientMessage type "$t"'),
    };
  }
}

class AuthRequest extends ClientMessage {
  final String playerName;
  final String? token;
  const AuthRequest({required this.playerName, this.token});

  factory AuthRequest.fromJson(Json m) =>
      AuthRequest(playerName: m['player_name'] as String, token: m['token'] as String?);

  @override
  Json toJson() {
    final m = <String, dynamic>{'type': 'auth', 'player_name': playerName};
    _put(m, 'token', token);
    return m;
  }
}

/// ClientMessage::Command — the command's own fields are flattened next to
/// "type": {"type":"command","action":"move",...}.
class CommandMessage extends ClientMessage {
  final Command command;
  const CommandMessage(this.command);

  @override
  Json toJson() => {'type': 'command', ...command.toJson()};
}

class PolicyUpdate extends ClientMessage {
  final int fleetId;
  final PolicyPreset preset;
  final PolicyParams? params;
  const PolicyUpdate({required this.fleetId, required this.preset, this.params});

  factory PolicyUpdate.fromJson(Json m) => PolicyUpdate(
        fleetId: _i(m['fleet_id']),
        preset: PolicyPreset.fromJson(m['preset']),
        params: _opt(m['params'], PolicyParams.fromJson),
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{'type': 'policy_update', 'fleet_id': fleetId, 'preset': preset.toJson()};
    _put(m, 'params', params?.toJson());
    return m;
  }
}

class RequestFullState extends ClientMessage {
  const RequestFullState();

  @override
  Json toJson() => {'type': 'request_full_state'};
}

class PolicyParams {
  final int minFuelPct;
  final int cargoReturnPct;
  final int maxRange;
  final int engageRatioX10;
  const PolicyParams({
    required this.minFuelPct,
    required this.cargoReturnPct,
    required this.maxRange,
    required this.engageRatioX10,
  });

  factory PolicyParams.fromJson(Object? json) {
    final m = _obj(json);
    return PolicyParams(
      minFuelPct: _i(m['min_fuel_pct']),
      cargoReturnPct: _i(m['cargo_return_pct']),
      maxRange: _i(m['max_range']),
      engageRatioX10: _i(m['engage_ratio_x10']),
    );
  }

  Json toJson() => {
        'min_fuel_pct': minFuelPct,
        'cargo_return_pct': cargoReturnPct,
        'max_range': maxRange,
        'engage_ratio_x10': engageRatioX10,
      };
}

/// {"action": "move" | "harvest" | ..., ...}
sealed class Command {
  const Command();

  Json toJson();

  static Command fromJson(Object? json) {
    final m = _obj(json);
    // `#[serde(default)]` fields: missing fleet_id -> 0, missing count -> 1.
    int fleet() => (m['fleet_id'] as int?) ?? 0;
    return switch (m['action']) {
      'move' => MoveCommand(fleetId: _i(m['fleet_id']), target: Hex.fromJson(m['target'])),
      'harvest' =>
        HarvestCommand(fleetId: _i(m['fleet_id']), resource: HarvestResource.fromJson(m['resource'])),
      'attack' => AttackCommand(fleetId: _i(m['fleet_id']), targetFleetId: _i(m['target_fleet_id'])),
      'recall' => RecallCommand(fleetId: _i(m['fleet_id'])),
      'collect_salvage' => CollectSalvageCommand(fleetId: _i(m['fleet_id'])),
      'build' => BuildCommand(buildingType: BuildingType.fromJson(m['building_type'])),
      'research' => ResearchCommand(tech: ResearchType.fromJson(m['tech'])),
      'build_ship' => BuildShipCommand(
          shipClass: ShipClass.fromJson(m['ship_class']),
          count: (m['count'] as int?) ?? 1,
        ),
      'cancel_build' => CancelBuildCommand(queueType: QueueType.fromJson(m['queue_type'])),
      'stop' => StopCommand(fleetId: fleet()),
      'scan' => ScanCommand(fleetId: fleet()),
      'explore_site' => ExploreSiteCommand(fleetId: fleet()),
      final a => throw FormatException('unknown Command action "$a"'),
    };
  }
}

class MoveCommand extends Command {
  final int fleetId;
  final Hex target;
  const MoveCommand({required this.fleetId, required this.target});
  @override
  Json toJson() => {'action': 'move', 'fleet_id': fleetId, 'target': target.toJson()};
}

class HarvestCommand extends Command {
  final int fleetId;
  final HarvestResource resource;
  const HarvestCommand({required this.fleetId, this.resource = HarvestResource.auto});
  @override
  Json toJson() => {'action': 'harvest', 'fleet_id': fleetId, 'resource': resource.toJson()};
}

class AttackCommand extends Command {
  final int fleetId;
  final int targetFleetId;
  const AttackCommand({required this.fleetId, required this.targetFleetId});
  @override
  Json toJson() => {'action': 'attack', 'fleet_id': fleetId, 'target_fleet_id': targetFleetId};
}

class RecallCommand extends Command {
  final int fleetId;
  const RecallCommand({required this.fleetId});
  @override
  Json toJson() => {'action': 'recall', 'fleet_id': fleetId};
}

class CollectSalvageCommand extends Command {
  final int fleetId;
  const CollectSalvageCommand({required this.fleetId});
  @override
  Json toJson() => {'action': 'collect_salvage', 'fleet_id': fleetId};
}

class BuildCommand extends Command {
  final BuildingType buildingType;
  const BuildCommand({required this.buildingType});
  @override
  Json toJson() => {'action': 'build', 'building_type': buildingType.toJson()};
}

class ResearchCommand extends Command {
  final ResearchType tech;
  const ResearchCommand({required this.tech});
  @override
  Json toJson() => {'action': 'research', 'tech': tech.toJson()};
}

class BuildShipCommand extends Command {
  final ShipClass shipClass;
  final int count;
  const BuildShipCommand({required this.shipClass, this.count = 1});
  @override
  Json toJson() => {'action': 'build_ship', 'ship_class': shipClass.toJson(), 'count': count};
}

class CancelBuildCommand extends Command {
  final QueueType queueType;
  const CancelBuildCommand({required this.queueType});
  @override
  Json toJson() => {'action': 'cancel_build', 'queue_type': queueType.toJson()};
}

class StopCommand extends Command {
  final int fleetId;
  const StopCommand({this.fleetId = 0});
  @override
  Json toJson() => {'action': 'stop', 'fleet_id': fleetId};
}

class ScanCommand extends Command {
  final int fleetId;
  const ScanCommand({this.fleetId = 0});
  @override
  Json toJson() => {'action': 'scan', 'fleet_id': fleetId};
}

class ExploreSiteCommand extends Command {
  final int fleetId;
  const ExploreSiteCommand({this.fleetId = 0});
  @override
  Json toJson() => {'action': 'explore_site', 'fleet_id': fleetId};
}

// ── Server → Client ───────────────────────────────────────────────

/// {"type": "auth_result" | "tick_update" | "full_state" | "event" | "error", ...}
sealed class ServerMessage {
  const ServerMessage();

  Json toJson();
  String encode() => jsonEncode(toJson());

  static ServerMessage decode(String raw) => fromJson(jsonDecode(raw));

  static ServerMessage fromJson(Object? json) {
    final m = _obj(json);
    return switch (m['type']) {
      'auth_result' => AuthResult.fromJson(m),
      'tick_update' => TickUpdate.fromJson(m),
      'full_state' => GameState.fromJson(m),
      'event' => GameEvent.fromJson(m),
      'error' => ErrorMessage.fromJson(m),
      final t => throw FormatException('unknown ServerMessage type "$t"'),
    };
  }
}

class AuthResult extends ServerMessage {
  final bool success;
  final int? playerId;
  final String? token;
  final ErrorCode? code;
  final String? message;
  const AuthResult({required this.success, this.playerId, this.token, this.code, this.message});

  factory AuthResult.fromJson(Json m) => AuthResult(
        success: m['success'] as bool,
        playerId: m['player_id'] as int?,
        token: m['token'] as String?,
        code: _opt(m['code'], ErrorCode.fromJson),
        message: m['message'] as String?,
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{'type': 'auth_result', 'success': success};
    _put(m, 'player_id', playerId);
    _put(m, 'token', token);
    _put(m, 'code', code?.toJson());
    _put(m, 'message', message);
    return m;
  }
}

/// Every owned fleet, every tick: the list is authoritative, so a fleet
/// missing from it is gone and an empty list means all fleets are lost.
class TickUpdate extends ServerMessage {
  final int tick;
  final PlayerState? player;
  final List<FleetState> fleets;
  final List<SectorState>? sectorUpdates;
  final HomeworldState? homeworldUpdate;
  final List<GameEvent>? events;
  const TickUpdate({
    required this.tick,
    this.player,
    required this.fleets,
    this.sectorUpdates,
    this.homeworldUpdate,
    this.events,
  });

  factory TickUpdate.fromJson(Json m) => TickUpdate(
        tick: _i(m['tick']),
        player: _opt(m['player'], PlayerState.fromJson),
        fleets: _list(m['fleets'], FleetState.fromJson),
        sectorUpdates: _opt(m['sector_updates'], (v) => _list(v, SectorState.fromJson)),
        homeworldUpdate: _opt(m['homeworld_update'], HomeworldState.fromJson),
        events: _opt(m['events'], (v) => _list(v, GameEvent.fromJson)),
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{'type': 'tick_update', 'tick': tick};
    _put(m, 'player', player?.toJson());
    m['fleets'] = fleets.map((e) => e.toJson()).toList();
    _put(m, 'sector_updates', sectorUpdates?.map((e) => e.toJson()).toList());
    _put(m, 'homeworld_update', homeworldUpdate?.toJson());
    _put(m, 'events', events?.map((e) => e.toEventJson()).toList());
    return m;
  }
}

/// ServerMessage::FullState (Rust struct `GameState`).
class GameState extends ServerMessage {
  final int tick;
  final PlayerState player;
  final List<FleetState> fleets;
  final HomeworldState homeworld;
  final List<SectorState> knownSectors;
  const GameState({
    required this.tick,
    required this.player,
    required this.fleets,
    required this.homeworld,
    required this.knownSectors,
  });

  factory GameState.fromJson(Json m) => GameState(
        tick: _i(m['tick']),
        player: PlayerState.fromJson(m['player']),
        fleets: _list(m['fleets'], FleetState.fromJson),
        homeworld: HomeworldState.fromJson(m['homeworld']),
        knownSectors: _list(m['known_sectors'], SectorState.fromJson),
      );

  @override
  Json toJson() => {
        'type': 'full_state',
        'tick': tick,
        'player': player.toJson(),
        'fleets': fleets.map((e) => e.toJson()).toList(),
        'homeworld': homeworld.toJson(),
        'known_sectors': knownSectors.map((e) => e.toJson()).toList(),
      };
}

class ErrorMessage extends ServerMessage {
  final ErrorCode code;
  final String message;
  const ErrorMessage({required this.code, required this.message});

  factory ErrorMessage.fromJson(Json m) =>
      ErrorMessage(code: ErrorCode.fromJson(m['code']), message: m['message'] as String);

  @override
  Json toJson() => {'type': 'error', 'code': code.toJson(), 'message': message};
}

// ── State types ───────────────────────────────────────────────────

class PlayerState {
  final int id;
  final String name;
  final Resources resources;
  final Hex homeworld;
  const PlayerState({required this.id, required this.name, required this.resources, required this.homeworld});

  factory PlayerState.fromJson(Object? json) {
    final m = _obj(json);
    return PlayerState(
      id: _i(m['id']),
      name: m['name'] as String,
      resources: Resources.fromJson(m['resources']),
      homeworld: Hex.fromJson(m['homeworld']),
    );
  }

  Json toJson() => {
        'id': id,
        'name': name,
        'resources': resources.toJson(),
        'homeworld': homeworld.toJson(),
      };
}

class FleetState {
  final int id;
  final Hex location;
  final FleetStatus state;
  final List<ShipState> ships;
  final Resources cargo;

  /// Hold size as the server enforces it; never derive it client-side.
  final double cargoCapacity;
  final double fuel;
  final double fuelMax;
  final int cooldownRemaining;
  final PolicyPreset? policy;
  const FleetState({
    required this.id,
    required this.location,
    required this.state,
    required this.ships,
    required this.cargo,
    required this.cargoCapacity,
    required this.fuel,
    required this.fuelMax,
    this.cooldownRemaining = 0,
    this.policy,
  });

  factory FleetState.fromJson(Object? json) {
    final m = _obj(json);
    return FleetState(
      id: _i(m['id']),
      location: Hex.fromJson(m['location']),
      state: FleetStatus.fromJson(m['state']),
      ships: _list(m['ships'], ShipState.fromJson),
      cargo: Resources.fromJson(m['cargo']),
      cargoCapacity: _f(m['cargo_capacity']),
      fuel: _f(m['fuel']),
      fuelMax: _f(m['fuel_max']),
      cooldownRemaining: (m['cooldown_remaining'] as int?) ?? 0,
      policy: _opt(m['policy'], PolicyPreset.fromJson),
    );
  }

  Json toJson() {
    final m = <String, dynamic>{
      'id': id,
      'location': location.toJson(),
      'state': state.toJson(),
      'ships': ships.map((e) => e.toJson()).toList(),
      'cargo': cargo.toJson(),
      'cargo_capacity': cargoCapacity,
      'fuel': fuel,
      'fuel_max': fuelMax,
      'cooldown_remaining': cooldownRemaining,
    };
    _put(m, 'policy', policy?.toJson());
    return m;
  }
}

/// JSON key for the class is "class".
class ShipState {
  final int id;
  final ShipClass shipClass;
  final double hull;
  final double hullMax;
  final double shield;
  final double shieldMax;
  final double weaponPower;
  const ShipState({
    required this.id,
    required this.shipClass,
    required this.hull,
    required this.hullMax,
    required this.shield,
    required this.shieldMax,
    required this.weaponPower,
  });

  factory ShipState.fromJson(Object? json) {
    final m = _obj(json);
    return ShipState(
      id: _i(m['id']),
      shipClass: ShipClass.fromJson(m['class']),
      hull: _f(m['hull']),
      hullMax: _f(m['hull_max']),
      shield: _f(m['shield']),
      shieldMax: _f(m['shield_max']),
      weaponPower: _f(m['weapon_power']),
    );
  }

  Json toJson() => {
        'id': id,
        'class': shipClass.toJson(),
        'hull': hull,
        'hull_max': hullMax,
        'shield': shield,
        'shield_max': shieldMax,
        'weapon_power': weaponPower,
      };
}

class SectorState {
  final Hex location;
  final TerrainType terrain;
  final SectorResources resources;

  /// Adjacent sectors reachable by a Move command (server-authoritative).
  final List<Hex> connections;
  final List<NpcFleetInfo>? hostiles;
  final List<FleetBrief>? playerFleets;
  final Resources? salvage;
  final SiteBrief? site;
  const SectorState({
    required this.location,
    required this.terrain,
    required this.resources,
    required this.connections,
    this.hostiles,
    this.playerFleets,
    this.salvage,
    this.site,
  });

  factory SectorState.fromJson(Object? json) {
    final m = _obj(json);
    return SectorState(
      location: Hex.fromJson(m['location']),
      terrain: TerrainType.fromJson(m['terrain']),
      resources: SectorResources.fromJson(m['resources']),
      connections: _list(m['connections'], Hex.fromJson),
      hostiles: _opt(m['hostiles'], (v) => _list(v, NpcFleetInfo.fromJson)),
      playerFleets: _opt(m['player_fleets'], (v) => _list(v, FleetBrief.fromJson)),
      salvage: _opt(m['salvage'], Resources.fromJson),
      site: _opt(m['site'], SiteBrief.fromJson),
    );
  }

  Json toJson() {
    final m = <String, dynamic>{
      'location': location.toJson(),
      'terrain': terrain.toJson(),
      'resources': resources.toJson(),
      'connections': connections.map((e) => e.toJson()).toList(),
    };
    _put(m, 'hostiles', hostiles?.map((e) => e.toJson()).toList());
    _put(m, 'player_fleets', playerFleets?.map((e) => e.toJson()).toList());
    _put(m, 'salvage', salvage?.toJson());
    _put(m, 'site', site?.toJson());
    return m;
  }
}

class SiteBrief {
  final int tier;
  final SiteRisk risk;
  const SiteBrief({required this.tier, required this.risk});

  factory SiteBrief.fromJson(Object? json) {
    final m = _obj(json);
    return SiteBrief(tier: _i(m['tier']), risk: SiteRisk.fromJson(m['risk']));
  }

  Json toJson() => {'tier': tier, 'risk': risk.toJson()};
}

class SectorResources {
  final Density metal;
  final Density crystal;
  final Density deuterium;
  const SectorResources({required this.metal, required this.crystal, required this.deuterium});

  factory SectorResources.fromJson(Object? json) {
    final m = _obj(json);
    return SectorResources(
      metal: Density.fromJson(m['metal']),
      crystal: Density.fromJson(m['crystal']),
      deuterium: Density.fromJson(m['deuterium']),
    );
  }

  Json toJson() => {'metal': metal.toJson(), 'crystal': crystal.toJson(), 'deuterium': deuterium.toJson()};
}

class NpcFleetInfo {
  final int id;
  final List<NpcShipInfo> ships;
  final NpcBehavior behavior;
  const NpcFleetInfo({required this.id, required this.ships, required this.behavior});

  factory NpcFleetInfo.fromJson(Object? json) {
    final m = _obj(json);
    return NpcFleetInfo(
      id: _i(m['id']),
      ships: _list(m['ships'], NpcShipInfo.fromJson),
      behavior: NpcBehavior.fromJson(m['behavior']),
    );
  }

  Json toJson() => {
        'id': id,
        'ships': ships.map((e) => e.toJson()).toList(),
        'behavior': behavior.toJson(),
      };

  int get shipCount => ships.fold(0, (s, e) => s + e.count);
}

/// JSON key for the class is "class".
class NpcShipInfo {
  final ShipClass shipClass;
  final int count;
  const NpcShipInfo({required this.shipClass, required this.count});

  factory NpcShipInfo.fromJson(Object? json) {
    final m = _obj(json);
    return NpcShipInfo(shipClass: ShipClass.fromJson(m['class']), count: _i(m['count']));
  }

  Json toJson() => {'class': shipClass.toJson(), 'count': count};
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

  factory FleetBrief.fromJson(Object? json) {
    final m = _obj(json);
    return FleetBrief(
      id: _i(m['id']),
      ownerName: m['owner_name'] as String,
      shipCount: _i(m['ship_count']),
      shipClasses: m['ship_classes'] == null
          ? const ShipClassCounts()
          : ShipClassCounts.fromJson(m['ship_classes']),
    );
  }

  Json toJson() => {
        'id': id,
        'owner_name': ownerName,
        'ship_count': shipCount,
        'ship_classes': shipClasses.toJson(),
      };
}

class ShipClassCounts {
  final int scout;
  final int corvette;
  final int frigate;
  final int cruiser;
  final int hauler;
  const ShipClassCounts({this.scout = 0, this.corvette = 0, this.frigate = 0, this.cruiser = 0, this.hauler = 0});

  factory ShipClassCounts.fromJson(Object? json) {
    final m = _obj(json);
    return ShipClassCounts(
      scout: _i(m['scout']),
      corvette: _i(m['corvette']),
      frigate: _i(m['frigate']),
      cruiser: _i(m['cruiser']),
      hauler: _i(m['hauler']),
    );
  }

  Json toJson() => {
        'scout': scout,
        'corvette': corvette,
        'frigate': frigate,
        'cruiser': cruiser,
        'hauler': hauler,
      };
}

class HomeworldState {
  final Hex location;

  /// Mine output per tick at current levels. There are no storage caps.
  final Resources production;
  final List<BuildingState> buildings;
  final List<ResearchState> research;
  final BuildQueueItem? buildQueue;
  final ShipyardQueueItem? shipyardQueue;
  final ResearchItem? researchActive;
  final List<ShipState> dockedShips;
  const HomeworldState({
    required this.location,
    required this.production,
    required this.buildings,
    required this.research,
    this.buildQueue,
    this.shipyardQueue,
    this.researchActive,
    required this.dockedShips,
  });

  factory HomeworldState.fromJson(Object? json) {
    final m = _obj(json);
    return HomeworldState(
      location: Hex.fromJson(m['location']),
      production: Resources.fromJson(m['production']),
      buildings: _list(m['buildings'], BuildingState.fromJson),
      research: _list(m['research'], ResearchState.fromJson),
      buildQueue: _opt(m['build_queue'], BuildQueueItem.fromJson),
      shipyardQueue: _opt(m['shipyard_queue'], ShipyardQueueItem.fromJson),
      researchActive: _opt(m['research_active'], ResearchItem.fromJson),
      dockedShips: _list(m['docked_ships'], ShipState.fromJson),
    );
  }

  Json toJson() {
    final m = <String, dynamic>{
      'location': location.toJson(),
      'production': production.toJson(),
      'buildings': buildings.map((e) => e.toJson()).toList(),
      'research': research.map((e) => e.toJson()).toList(),
      'docked_ships': dockedShips.map((e) => e.toJson()).toList(),
    };
    _put(m, 'build_queue', buildQueue?.toJson());
    _put(m, 'shipyard_queue', shipyardQueue?.toJson());
    _put(m, 'research_active', researchActive?.toJson());
    return m;
  }

  int buildingLevel(BuildingType t) {
    for (final b in buildings) {
      if (b.buildingType == t) return b.level;
    }
    return 0;
  }
}

class BuildingState {
  final BuildingType buildingType;
  final int level;
  const BuildingState({required this.buildingType, required this.level});

  factory BuildingState.fromJson(Object? json) {
    final m = _obj(json);
    return BuildingState(buildingType: BuildingType.fromJson(m['building_type']), level: _i(m['level']));
  }

  Json toJson() => {'building_type': buildingType.toJson(), 'level': level};
}

class ResearchState {
  final ResearchType tech;
  final int level;
  const ResearchState({required this.tech, required this.level});

  factory ResearchState.fromJson(Object? json) {
    final m = _obj(json);
    return ResearchState(tech: ResearchType.fromJson(m['tech']), level: _i(m['level']));
  }

  Json toJson() => {'tech': tech.toJson(), 'level': level};
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

  factory BuildQueueItem.fromJson(Object? json) {
    final m = _obj(json);
    return BuildQueueItem(
      buildingType: BuildingType.fromJson(m['building_type']),
      targetLevel: _i(m['target_level']),
      startTick: _i(m['start_tick']),
      endTick: _i(m['end_tick']),
    );
  }

  Json toJson() => {
        'building_type': buildingType.toJson(),
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

  factory ShipyardQueueItem.fromJson(Object? json) {
    final m = _obj(json);
    return ShipyardQueueItem(
      shipClass: ShipClass.fromJson(m['ship_class']),
      count: _i(m['count']),
      built: _i(m['built']),
      startTick: _i(m['start_tick']),
      endTick: _i(m['end_tick']),
    );
  }

  Json toJson() => {
        'ship_class': shipClass.toJson(),
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

  factory ResearchItem.fromJson(Object? json) {
    final m = _obj(json);
    return ResearchItem(
      tech: ResearchType.fromJson(m['tech']),
      targetLevel: _i(m['target_level']),
      startTick: _i(m['start_tick']),
      endTick: _i(m['end_tick']),
    );
  }

  Json toJson() => {
        'tech': tech.toJson(),
        'target_level': targetLevel,
        'start_tick': startTick,
        'end_tick': endTick,
      };
}

// ── Events ────────────────────────────────────────────────────────

/// {"tick": n, "kind": "PascalCaseVariant", ...kind fields}. Also a ServerMessage
/// when sent standalone: {"type":"event", "tick":..., "kind":..., ...}.
class GameEvent extends ServerMessage {
  final int tick;
  final EventKind kind;
  const GameEvent({required this.tick, required this.kind});

  factory GameEvent.fromJson(Object? json) {
    final m = _obj(json);
    return GameEvent(tick: _i(m['tick']), kind: EventKind.fromJson(m));
  }

  /// As embedded in `tick_update.events` (no "type" key).
  Json toEventJson() => {'tick': tick, ...kind.toJson()};

  @override
  Json toJson() => {'type': 'event', ...toEventJson()};
}

/// Internally tagged on "kind" (PascalCase variant name).
sealed class EventKind {
  const EventKind();

  Json toJson();

  static EventKind fromJson(Object? json) {
    final m = _obj(json);
    return switch (m['kind']) {
      'CombatRound' => CombatRoundEvent.fromJson(m),
      'ShipDestroyed' => ShipDestroyedEvent.fromJson(m),
      'FleetDestroyed' => FleetDestroyedEvent.fromJson(m),
      'ResourceHarvested' => ResourceHarvestedEvent.fromJson(m),
      'SectorEntered' => SectorEnteredEvent.fromJson(m),
      'CombatStarted' => CombatStartedEvent.fromJson(m),
      'CombatEnded' => CombatEndedEvent.fromJson(m),
      'SalvageCollected' => SalvageCollectedEvent.fromJson(m),
      'FleetArrived' => FleetArrivedEvent.fromJson(m),
      'BuildingCompleted' => BuildingCompletedEvent.fromJson(m),
      'ResearchCompleted' => ResearchCompletedEvent.fromJson(m),
      'ShipBuilt' => ShipBuiltEvent.fromJson(m),
      'ScanCompleted' => ScanCompletedEvent.fromJson(m),
      'RaidIncoming' => RaidIncomingEvent.fromJson(m),
      'RaidResolved' => RaidResolvedEvent.fromJson(m),
      'SiteExplorationStarted' => SiteExplorationStartedEvent.fromJson(m),
      'SiteExplored' => SiteExploredEvent.fromJson(m),
      'SiteAmbush' => SiteAmbushEvent.fromJson(m),
      'PolicyAction' => PolicyActionEvent.fromJson(m),
      'Alert' => AlertEvent.fromJson(m),
      final k => throw FormatException('unknown EventKind "$k"'),
    };
  }
}

class CombatRoundEvent extends EventKind {
  final int attackerShipId;
  final int targetShipId;
  final double damage;
  final double shieldAbsorbed;
  final double hullDamage;
  final bool rapidFire;
  const CombatRoundEvent({
    required this.attackerShipId,
    required this.targetShipId,
    required this.damage,
    required this.shieldAbsorbed,
    required this.hullDamage,
    required this.rapidFire,
  });

  factory CombatRoundEvent.fromJson(Json m) => CombatRoundEvent(
        attackerShipId: _i(m['attacker_ship_id']),
        targetShipId: _i(m['target_ship_id']),
        damage: _f(m['damage']),
        shieldAbsorbed: _f(m['shield_absorbed']),
        hullDamage: _f(m['hull_damage']),
        rapidFire: m['rapid_fire'] as bool,
      );

  @override
  Json toJson() => {
        'kind': 'CombatRound',
        'attacker_ship_id': attackerShipId,
        'target_ship_id': targetShipId,
        'damage': damage,
        'shield_absorbed': shieldAbsorbed,
        'hull_damage': hullDamage,
        'rapid_fire': rapidFire,
      };
}

class ShipDestroyedEvent extends EventKind {
  final int shipId;
  final ShipClass shipClass;
  final int ownerFleetId;
  final bool isNpc;
  const ShipDestroyedEvent({
    required this.shipId,
    required this.shipClass,
    required this.ownerFleetId,
    required this.isNpc,
  });

  factory ShipDestroyedEvent.fromJson(Json m) => ShipDestroyedEvent(
        shipId: _i(m['ship_id']),
        shipClass: ShipClass.fromJson(m['ship_class']),
        ownerFleetId: _i(m['owner_fleet_id']),
        isNpc: m['is_npc'] as bool,
      );

  @override
  Json toJson() => {
        'kind': 'ShipDestroyed',
        'ship_id': shipId,
        'ship_class': shipClass.toJson(),
        'owner_fleet_id': ownerFleetId,
        'is_npc': isNpc,
      };
}

class FleetDestroyedEvent extends EventKind {
  final int fleetId;
  final bool isNpc;
  final Resources salvage;
  const FleetDestroyedEvent({required this.fleetId, required this.isNpc, required this.salvage});

  factory FleetDestroyedEvent.fromJson(Json m) => FleetDestroyedEvent(
        fleetId: _i(m['fleet_id']),
        isNpc: m['is_npc'] as bool,
        salvage: Resources.fromJson(m['salvage']),
      );

  @override
  Json toJson() => {
        'kind': 'FleetDestroyed',
        'fleet_id': fleetId,
        'is_npc': isNpc,
        'salvage': salvage.toJson(),
      };
}

class ResourceHarvestedEvent extends EventKind {
  final int fleetId;
  final HarvestResource resourceType;
  final double amount;
  const ResourceHarvestedEvent({required this.fleetId, required this.resourceType, required this.amount});

  factory ResourceHarvestedEvent.fromJson(Json m) => ResourceHarvestedEvent(
        fleetId: _i(m['fleet_id']),
        resourceType: HarvestResource.fromJson(m['resource_type']),
        amount: _f(m['amount']),
      );

  @override
  Json toJson() => {
        'kind': 'ResourceHarvested',
        'fleet_id': fleetId,
        'resource_type': resourceType.toJson(),
        'amount': amount,
      };
}

class SectorEnteredEvent extends EventKind {
  final int fleetId;
  final Hex sector;
  final bool firstVisit;
  const SectorEnteredEvent({required this.fleetId, required this.sector, required this.firstVisit});

  factory SectorEnteredEvent.fromJson(Json m) => SectorEnteredEvent(
        fleetId: _i(m['fleet_id']),
        sector: Hex.fromJson(m['sector']),
        firstVisit: m['first_visit'] as bool,
      );

  @override
  Json toJson() => {
        'kind': 'SectorEntered',
        'fleet_id': fleetId,
        'sector': sector.toJson(),
        'first_visit': firstVisit,
      };
}

class CombatStartedEvent extends EventKind {
  final int playerFleetId;
  final int enemyFleetId;
  final Hex sector;
  const CombatStartedEvent({required this.playerFleetId, required this.enemyFleetId, required this.sector});

  factory CombatStartedEvent.fromJson(Json m) => CombatStartedEvent(
        playerFleetId: _i(m['player_fleet_id']),
        enemyFleetId: _i(m['enemy_fleet_id']),
        sector: Hex.fromJson(m['sector']),
      );

  @override
  Json toJson() => {
        'kind': 'CombatStarted',
        'player_fleet_id': playerFleetId,
        'enemy_fleet_id': enemyFleetId,
        'sector': sector.toJson(),
      };
}

class CombatEndedEvent extends EventKind {
  final Hex sector;
  final bool playerVictory;
  const CombatEndedEvent({required this.sector, required this.playerVictory});

  factory CombatEndedEvent.fromJson(Json m) =>
      CombatEndedEvent(sector: Hex.fromJson(m['sector']), playerVictory: m['player_victory'] as bool);

  @override
  Json toJson() =>
      {'kind': 'CombatEnded', 'sector': sector.toJson(), 'player_victory': playerVictory};
}

class SalvageCollectedEvent extends EventKind {
  final int fleetId;
  final Resources resources;
  const SalvageCollectedEvent({required this.fleetId, required this.resources});

  factory SalvageCollectedEvent.fromJson(Json m) => SalvageCollectedEvent(
        fleetId: _i(m['fleet_id']),
        resources: Resources.fromJson(m['resources']),
      );

  @override
  Json toJson() =>
      {'kind': 'SalvageCollected', 'fleet_id': fleetId, 'resources': resources.toJson()};
}

class FleetArrivedEvent extends EventKind {
  final int fleetId;
  final Hex sector;
  const FleetArrivedEvent({required this.fleetId, required this.sector});

  factory FleetArrivedEvent.fromJson(Json m) =>
      FleetArrivedEvent(fleetId: _i(m['fleet_id']), sector: Hex.fromJson(m['sector']));

  @override
  Json toJson() => {'kind': 'FleetArrived', 'fleet_id': fleetId, 'sector': sector.toJson()};
}

class BuildingCompletedEvent extends EventKind {
  final BuildingType buildingType;
  final int newLevel;
  final int? playerId;
  const BuildingCompletedEvent({required this.buildingType, required this.newLevel, this.playerId});

  factory BuildingCompletedEvent.fromJson(Json m) => BuildingCompletedEvent(
        buildingType: BuildingType.fromJson(m['building_type']),
        newLevel: _i(m['new_level']),
        playerId: m['player_id'] as int?,
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{
      'kind': 'BuildingCompleted',
      'building_type': buildingType.toJson(),
      'new_level': newLevel,
    };
    _put(m, 'player_id', playerId);
    return m;
  }
}

class ResearchCompletedEvent extends EventKind {
  final ResearchType tech;
  final int newLevel;
  final int? playerId;
  const ResearchCompletedEvent({required this.tech, required this.newLevel, this.playerId});

  factory ResearchCompletedEvent.fromJson(Json m) => ResearchCompletedEvent(
        tech: ResearchType.fromJson(m['tech']),
        newLevel: _i(m['new_level']),
        playerId: m['player_id'] as int?,
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{'kind': 'ResearchCompleted', 'tech': tech.toJson(), 'new_level': newLevel};
    _put(m, 'player_id', playerId);
    return m;
  }
}

class ShipBuiltEvent extends EventKind {
  final ShipClass shipClass;
  final int count;
  final int? playerId;
  const ShipBuiltEvent({required this.shipClass, required this.count, this.playerId});

  factory ShipBuiltEvent.fromJson(Json m) => ShipBuiltEvent(
        shipClass: ShipClass.fromJson(m['ship_class']),
        count: _i(m['count']),
        playerId: m['player_id'] as int?,
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{'kind': 'ShipBuilt', 'ship_class': shipClass.toJson(), 'count': count};
    _put(m, 'player_id', playerId);
    return m;
  }
}

class ScanCompletedEvent extends EventKind {
  final int fleetId;
  final Hex sector;
  final int sectorsRevealed;
  final int hostilesDetected;
  final List<SignalContact> signals;
  const ScanCompletedEvent({
    required this.fleetId,
    required this.sector,
    required this.sectorsRevealed,
    required this.hostilesDetected,
    this.signals = const [],
  });

  factory ScanCompletedEvent.fromJson(Json m) => ScanCompletedEvent(
        fleetId: _i(m['fleet_id']),
        sector: Hex.fromJson(m['sector']),
        sectorsRevealed: _i(m['sectors_revealed']),
        hostilesDetected: _i(m['hostiles_detected']),
        signals: m['signals'] == null ? const [] : _list(m['signals'], SignalContact.fromJson),
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{
      'kind': 'ScanCompleted',
      'fleet_id': fleetId,
      'sector': sector.toJson(),
      'sectors_revealed': sectorsRevealed,
      'hostiles_detected': hostilesDetected,
    };
    // skip_serializing_if = "Vec::is_empty"
    if (signals.isNotEmpty) m['signals'] = signals.map((e) => e.toJson()).toList();
    return m;
  }
}

class SignalContact {
  final Hex sector;
  final SignalKind signal;
  const SignalContact({required this.sector, required this.signal});

  factory SignalContact.fromJson(Object? json) {
    final m = _obj(json);
    return SignalContact(sector: Hex.fromJson(m['sector']), signal: SignalKind.fromJson(m['signal']));
  }

  Json toJson() => {'sector': sector.toJson(), 'signal': signal.toJson()};
}

class RaidIncomingEvent extends EventKind {
  final int playerId;
  final int arrivalTick;
  final String threat;
  const RaidIncomingEvent({required this.playerId, required this.arrivalTick, required this.threat});

  factory RaidIncomingEvent.fromJson(Json m) => RaidIncomingEvent(
        playerId: _i(m['player_id']),
        arrivalTick: _i(m['arrival_tick']),
        threat: m['threat'] as String,
      );

  @override
  Json toJson() => {
        'kind': 'RaidIncoming',
        'player_id': playerId,
        'arrival_tick': arrivalTick,
        'threat': threat,
      };
}

class RaidResolvedEvent extends EventKind {
  final int playerId;
  final bool defended;
  final Resources resourcesLost;
  final Resources? salvageDropped;
  final double raidPower;
  final double defensePower;
  const RaidResolvedEvent({
    required this.playerId,
    required this.defended,
    required this.resourcesLost,
    this.salvageDropped,
    required this.raidPower,
    required this.defensePower,
  });

  factory RaidResolvedEvent.fromJson(Json m) => RaidResolvedEvent(
        playerId: _i(m['player_id']),
        defended: m['defended'] as bool,
        resourcesLost: Resources.fromJson(m['resources_lost']),
        salvageDropped: _opt(m['salvage_dropped'], Resources.fromJson),
        raidPower: _f(m['raid_power']),
        defensePower: _f(m['defense_power']),
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{
      'kind': 'RaidResolved',
      'player_id': playerId,
      'defended': defended,
      'resources_lost': resourcesLost.toJson(),
      'raid_power': raidPower,
      'defense_power': defensePower,
    };
    _put(m, 'salvage_dropped', salvageDropped?.toJson());
    return m;
  }
}

class SiteExplorationStartedEvent extends EventKind {
  final int fleetId;
  final Hex sector;
  final int tier;
  final int endTick;
  const SiteExplorationStartedEvent({
    required this.fleetId,
    required this.sector,
    required this.tier,
    required this.endTick,
  });

  factory SiteExplorationStartedEvent.fromJson(Json m) => SiteExplorationStartedEvent(
        fleetId: _i(m['fleet_id']),
        sector: Hex.fromJson(m['sector']),
        tier: _i(m['tier']),
        endTick: _i(m['end_tick']),
      );

  @override
  Json toJson() => {
        'kind': 'SiteExplorationStarted',
        'fleet_id': fleetId,
        'sector': sector.toJson(),
        'tier': tier,
        'end_tick': endTick,
      };
}

class SiteExploredEvent extends EventKind {
  final int fleetId;
  final Hex sector;
  final int tier;
  final Resources resources;
  final ShipClass? recoveredShip;
  final ResearchType? techCache;
  const SiteExploredEvent({
    required this.fleetId,
    required this.sector,
    required this.tier,
    required this.resources,
    this.recoveredShip,
    this.techCache,
  });

  factory SiteExploredEvent.fromJson(Json m) => SiteExploredEvent(
        fleetId: _i(m['fleet_id']),
        sector: Hex.fromJson(m['sector']),
        tier: _i(m['tier']),
        resources: Resources.fromJson(m['resources']),
        recoveredShip: _opt(m['recovered_ship'], ShipClass.fromJson),
        techCache: _opt(m['tech_cache'], ResearchType.fromJson),
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{
      'kind': 'SiteExplored',
      'fleet_id': fleetId,
      'sector': sector.toJson(),
      'tier': tier,
      'resources': resources.toJson(),
    };
    _put(m, 'recovered_ship', recoveredShip?.toJson());
    _put(m, 'tech_cache', techCache?.toJson());
    return m;
  }
}

class SiteAmbushEvent extends EventKind {
  final int fleetId;
  final Hex sector;
  final int npcFleetId;
  const SiteAmbushEvent({required this.fleetId, required this.sector, required this.npcFleetId});

  factory SiteAmbushEvent.fromJson(Json m) => SiteAmbushEvent(
        fleetId: _i(m['fleet_id']),
        sector: Hex.fromJson(m['sector']),
        npcFleetId: _i(m['npc_fleet_id']),
      );

  @override
  Json toJson() => {
        'kind': 'SiteAmbush',
        'fleet_id': fleetId,
        'sector': sector.toJson(),
        'npc_fleet_id': npcFleetId,
      };
}

class PolicyActionEvent extends EventKind {
  final int fleetId;
  final PolicyPreset preset;
  final String action;
  final String reason;
  const PolicyActionEvent({
    required this.fleetId,
    required this.preset,
    required this.action,
    required this.reason,
  });

  factory PolicyActionEvent.fromJson(Json m) => PolicyActionEvent(
        fleetId: _i(m['fleet_id']),
        preset: PolicyPreset.fromJson(m['preset']),
        action: m['action'] as String,
        reason: m['reason'] as String,
      );

  @override
  Json toJson() => {
        'kind': 'PolicyAction',
        'fleet_id': fleetId,
        'preset': preset.toJson(),
        'action': action,
        'reason': reason,
      };
}

class AlertEvent extends EventKind {
  final AlertLevel level;
  final String message;
  final Hex? sector;
  final int? fleetId;
  const AlertEvent({required this.level, required this.message, this.sector, this.fleetId});

  factory AlertEvent.fromJson(Json m) => AlertEvent(
        level: AlertLevel.fromJson(m['level']),
        message: m['message'] as String,
        sector: _opt(m['sector'], Hex.fromJson),
        fleetId: m['fleet_id'] as int?,
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{'kind': 'Alert', 'level': level.toJson(), 'message': message};
    _put(m, 'sector', sector?.toJson());
    _put(m, 'fleet_id', fleetId);
    return m;
  }
}
