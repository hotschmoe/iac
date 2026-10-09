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

enum QueueAction {
  started,
  waiting,
  cancelled;

  String get wire => _pascal(name);
  String toJson() => wire;
  static QueueAction fromJson(Object? v) => _enumFrom(values, v, _pascal);
}

/// Why a queued order has not started yet.
enum WaitReason {
  /// Every slot is busy.
  slot,

  /// The stockpile does not cover the cost.
  resources,

  /// A level, building or tech it needs is still being built.
  prerequisite,

  /// It could start, but an earlier order is first in line and short of
  /// resources (first in, first out).
  order;

  String get wire => _pascal(name);
  String toJson() => wire;
  static WaitReason fromJson(Object? v) => _enumFrom(values, v, _pascal);
}

enum BuildingType {
  metalMine('Metal Mine'),
  crystalMine('Crystal Mine'),
  deuteriumSynthesizer('Deut Synthesizer'),
  shipyard('Shipyard'),
  researchLab('Research Lab'),
  fuelDepot('Fuel Depot'),
  sensorArray('Sensor Array'),
  defenseGrid('Defense Grid'),
  storageVault('Storage Vault'),
  fabricator('Fabricator');

  final String label;
  const BuildingType(this.label);

  String get wire => _pascal(name);
  String toJson() => wire;
  static BuildingType fromJson(Object? v) => _enumFrom(values, v, _pascal);
}

/// Home defence structures, built from the shipyard queue.
enum DefenceKind {
  pulseTurret('Pulse Turret'),
  lancerBattery('Lancer Battery'),
  ionBastion('Ion Bastion');

  final String label;
  const DefenceKind(this.label);

  String get wire => _pascal(name);
  String toJson() => wire;
  static DefenceKind fromJson(Object? v) => _enumFrom(values, v, _pascal);
}

/// What the shipyard queue builds: a ship class or a defence structure.
/// On the wire it is just the name ("Corvette", "PulseTurret").
class ShipyardItem {
  final ShipClass? ship;
  final DefenceKind? defence;
  const ShipyardItem.ship(ShipClass this.ship) : defence = null;
  const ShipyardItem.defence(DefenceKind this.defence) : ship = null;

  factory ShipyardItem.fromJson(Object? v) {
    for (final c in ShipClass.values) {
      if (c.wire == v) return ShipyardItem.ship(c);
    }
    return ShipyardItem.defence(DefenceKind.fromJson(v));
  }

  String get label => ship?.label ?? defence!.label;
  String toJson() => ship?.toJson() ?? defence!.toJson();

  @override
  bool operator ==(Object other) => other is ShipyardItem && other.ship == ship && other.defence == defence;
  @override
  int get hashCode => Object.hash(ship, defence);
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
  emergencyJump('Emergency Jump'),
  modularFabrication('Modular Fabrication');

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
  storageTooSmall(1017),
  defenceLocked(1018),
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

/// One of the three stockpiled resources.
enum ResourceKind {
  metal,
  crystal,
  deuterium;

  String get wire => _snake(name);
  String toJson() => wire;
  static ResourceKind fromJson(Object? v) => _enumFrom(values, v, _snake);
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

enum ThreatBasis {
  observed,
  template,
  estimate;

  String get wire => _snake(name);
  String toJson() => wire;
  static ThreatBasis fromJson(Object? v) => _enumFrom(values, v, _snake);
}

/// Verdict on a fleet-power to threat-power ratio: safe from 3.0, favourable
/// from 2.0, risky from 1.2, deadly below.
enum RatioLabel {
  safe('SAFE'),
  favourable('FAVOURABLE'),
  risky('RISKY'),
  deadly('DEADLY');

  final String label;
  const RatioLabel(this.label);

  String get wire => _snake(name);
  String toJson() => wire;
  static RatioLabel fromJson(Object? v) => _enumFrom(values, v, _snake);
}

// ── Shared structs ────────────────────────────────────────────────

/// How dangerous a sector is: T1 to T9 from the power of what is there.
class ThreatInfo {
  final int rating;
  final double estPower;
  final ThreatBasis basis;
  const ThreatInfo({required this.rating, required this.estPower, required this.basis});

  factory ThreatInfo.fromJson(Object? json) {
    final m = _obj(json);
    return ThreatInfo(
      rating: _i(m['rating']),
      estPower: _f(m['est_power']),
      basis: ThreatBasis.fromJson(m['basis']),
    );
  }

  Json toJson() => {'rating': rating, 'est_power': estPower, 'basis': basis.toJson()};
}

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

  /// True when an AI agent flies this account (read only at creation; it only
  /// labels the leaderboard).
  final bool agent;
  const AuthRequest({required this.playerName, this.token, this.agent = false});

  factory AuthRequest.fromJson(Json m) => AuthRequest(
        playerName: m['player_name'] as String,
        token: m['token'] as String?,
        agent: (m['agent'] as bool?) ?? false,
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{'type': 'auth', 'player_name': playerName, 'agent': agent};
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

/// Doctrine thresholds. Every field is optional on the wire; the defaults
/// here are the server's (constants.rs).
class PolicyParams {
  /// Spare fuel, percent of the tank, kept on top of the route home.
  final int minFuelPct;
  final int cargoReturnPct;

  /// Hexes from the homeworld, never from the last claim.
  final int maxRange;
  final int engageRatioX10;
  const PolicyParams({
    this.minFuelPct = 10,
    this.cargoReturnPct = 85,
    this.maxRange = 4,
    this.engageRatioX10 = 12,
  });

  factory PolicyParams.fromJson(Object? json) {
    final m = _obj(json);
    return PolicyParams(
      minFuelPct: (m['min_fuel_pct'] as int?) ?? 10,
      cargoReturnPct: (m['cargo_return_pct'] as int?) ?? 85,
      maxRange: (m['max_range'] as int?) ?? 4,
      engageRatioX10: (m['engage_ratio_x10'] as int?) ?? 12,
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
      'build_defence' => BuildDefenceCommand(
          kind: DefenceKind.fromJson(m['kind']),
          count: (m['count'] as int?) ?? 1,
        ),
      'cancel_build' => CancelBuildCommand(
          queueType: QueueType.fromJson(m['queue_type']),
          index: (m['index'] as int?) ?? 0,
        ),
      'cancel_queued' => CancelQueuedCommand(queueType: QueueType.fromJson(m['queue_type']), index: _i(m['index'])),
      'leaderboard' => LeaderboardCommand(limit: (m['limit'] as int?) ?? 20),
      'preview_move' => PreviewMoveCommand(fleetId: _i(m['fleet_id']), target: Hex.fromJson(m['target'])),
      'stop' => StopCommand(fleetId: fleet()),
      'scan' => ScanCommand(fleetId: fleet()),
      'explore_site' => ExploreSiteCommand(fleetId: fleet()),
      'split' => SplitCommand(fleetId: _i(m['fleet_id']), shipIds: _list(m['ship_ids'], _i)),
      'merge' => MergeCommand(fleetId: _i(m['fleet_id']), otherFleetId: _i(m['other_fleet_id'])),
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

class BuildDefenceCommand extends Command {
  final DefenceKind kind;
  final int count;
  const BuildDefenceCommand({required this.kind, this.count = 1});
  @override
  Json toJson() => {'action': 'build_defence', 'kind': kind.toJson(), 'count': count};
}

/// Cancel an item that has started (half refunded); [index] picks among the
/// running buildings.
class CancelBuildCommand extends Command {
  final QueueType queueType;
  final int index;
  const CancelBuildCommand({required this.queueType, this.index = 0});
  @override
  Json toJson() => {'action': 'cancel_build', 'queue_type': queueType.toJson(), 'index': index};
}

/// Remove an item still waiting in a queue; nothing was paid.
class CancelQueuedCommand extends Command {
  final QueueType queueType;
  final int index;
  const CancelQueuedCommand({required this.queueType, required this.index});
  @override
  Json toJson() => {'action': 'cancel_queued', 'queue_type': queueType.toJson(), 'index': index};
}

/// Ask for the score table; the reply is a [LeaderboardReply].
class LeaderboardCommand extends Command {
  final int limit;
  const LeaderboardCommand({this.limit = 20});
  @override
  Json toJson() => {'action': 'leaderboard', 'limit': limit};
}

/// Ask what one jump to the adjacent sector would cost; the reply is a
/// [MovePreview].
class PreviewMoveCommand extends Command {
  final int fleetId;
  final Hex target;
  const PreviewMoveCommand({required this.fleetId, required this.target});
  @override
  Json toJson() => {'action': 'preview_move', 'fleet_id': fleetId, 'target': target.toJson()};
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

/// Detach [shipIds] from the fleet into a new fleet at the same sector (at the
/// homeworld this launches docked ships). The reply is an Alert event naming
/// the new fleet id.
class SplitCommand extends Command {
  final int fleetId;
  final List<int> shipIds;
  const SplitCommand({required this.fleetId, required this.shipIds});
  @override
  Json toJson() => {'action': 'split', 'fleet_id': fleetId, 'ship_ids': shipIds};
}

/// Fold [otherFleetId] into [fleetId]; both idle, same sector.
class MergeCommand extends Command {
  final int fleetId;
  final int otherFleetId;
  const MergeCommand({required this.fleetId, required this.otherFleetId});
  @override
  Json toJson() => {'action': 'merge', 'fleet_id': fleetId, 'other_fleet_id': otherFleetId};
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
      'preview_move' => MovePreview.fromJson(m),
      'leaderboard' => LeaderboardReply.fromJson(m),
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
  final WorldInfo world;
  const GameState({
    required this.tick,
    required this.player,
    required this.fleets,
    required this.homeworld,
    required this.knownSectors,
    required this.world,
  });

  factory GameState.fromJson(Json m) => GameState(
        tick: _i(m['tick']),
        player: PlayerState.fromJson(m['player']),
        fleets: _list(m['fleets'], FleetState.fromJson),
        homeworld: HomeworldState.fromJson(m['homeworld']),
        knownSectors: _list(m['known_sectors'], SectorState.fromJson),
        world: WorldInfo.fromJson(m['world']),
      );

  @override
  Json toJson() => {
        'type': 'full_state',
        'tick': tick,
        'player': player.toJson(),
        'fleets': fleets.map((e) => e.toJson()).toList(),
        'homeworld': homeworld.toJson(),
        'known_sectors': knownSectors.map((e) => e.toJson()).toList(),
        'world': world.toJson(),
      };
}

/// The world's fixed settings. [pace] scales economy timers and never
/// changes for the life of a world.
class WorldInfo {
  final double pace;
  final String? preset;
  final int tickHz;
  final int economyVersion;
  final int worldgenVersion;
  final WorldEstimate? estimate;
  const WorldInfo({
    required this.pace,
    this.preset,
    required this.tickHz,
    required this.economyVersion,
    required this.worldgenVersion,
    this.estimate,
  });

  factory WorldInfo.fromJson(Object? json) {
    final m = _obj(json);
    return WorldInfo(
      pace: _f(m['pace']),
      preset: m['preset'] as String?,
      tickHz: _i(m['tick_hz']),
      economyVersion: _i(m['economy_version']),
      worldgenVersion: _i(m['worldgen_version']),
      estimate: _opt(m['estimate'], WorldEstimate.fromJson),
    );
  }

  Json toJson() {
    final m = <String, dynamic>{
      'pace': pace,
      'tick_hz': tickHz,
      'economy_version': economyVersion,
      'worldgen_version': worldgenVersion,
    };
    _put(m, 'preset', preset);
    _put(m, 'estimate', estimate?.toJson());
    return m;
  }

  /// "x600 blitz" or "x42".
  String get label {
    final n = pace == pace.roundToDouble() ? pace.round().toString() : pace.toString();
    return preset == null ? 'x$n' : 'x$n $preset';
  }
}

class WorldEstimate {
  final int firstCruiserS;
  final int endgameS;
  const WorldEstimate({required this.firstCruiserS, required this.endgameS});

  factory WorldEstimate.fromJson(Object? json) {
    final m = _obj(json);
    return WorldEstimate(firstCruiserS: _i(m['first_cruiser_s']), endgameS: _i(m['endgame_s']));
  }

  Json toJson() => {'first_cruiser_s': firstCruiserS, 'endgame_s': endgameS};
}

/// The score table: one ranking for every player, human or agent.
class LeaderboardReply extends ServerMessage {
  final int tick;
  final List<LeaderboardEntry> entries;

  /// The asking player's own row when it falls outside [entries].
  final LeaderboardEntry? you;
  const LeaderboardReply({required this.tick, required this.entries, this.you});

  factory LeaderboardReply.fromJson(Json m) => LeaderboardReply(
        tick: _i(m['tick']),
        entries: _list(m['entries'], LeaderboardEntry.fromJson),
        you: _opt(m['you'], LeaderboardEntry.fromJson),
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{
      'type': 'leaderboard',
      'tick': tick,
      'entries': entries.map((e) => e.toJson()).toList(),
    };
    _put(m, 'you', you?.toJson());
    return m;
  }
}

class LeaderboardEntry {
  final int rank;
  final String name;
  final bool agent;
  final double score;
  final double core;

  /// Raw points before the 25 percent cap on combat plus exploration.
  final double combat;
  final double explore;
  const LeaderboardEntry({
    required this.rank,
    required this.name,
    required this.agent,
    required this.score,
    required this.core,
    required this.combat,
    required this.explore,
  });

  factory LeaderboardEntry.fromJson(Object? json) {
    final m = _obj(json);
    return LeaderboardEntry(
      rank: _i(m['rank']),
      name: m['name'] as String,
      agent: m['agent'] as bool,
      score: _f(m['score']),
      core: _f(m['core']),
      combat: _f(m['combat']),
      explore: _f(m['explore']),
    );
  }

  Json toJson() => {
        'rank': rank,
        'name': name,
        'agent': agent,
        'score': score,
        'core': core,
        'combat': combat,
        'explore': explore,
      };
}

/// Answer to [PreviewMoveCommand]: the fuel side of a jump. Threat for the
/// destination comes with the threat rating work and is not sent yet.
class MovePreview extends ServerMessage {
  final int fleetId;
  final Hex target;
  final double fuelCost;
  final double fuelAfter;
  final bool canJump;
  final int hopsHome;
  final double fuelToReturn;
  final bool canReturn;
  final ThreatInfo threat;
  final double fleetPower;
  final double ratio;
  final RatioLabel label;
  const MovePreview({
    required this.fleetId,
    required this.target,
    required this.fuelCost,
    required this.fuelAfter,
    required this.canJump,
    required this.hopsHome,
    required this.fuelToReturn,
    required this.canReturn,
    required this.threat,
    required this.fleetPower,
    required this.ratio,
    required this.label,
  });

  factory MovePreview.fromJson(Json m) => MovePreview(
        fleetId: _i(m['fleet_id']),
        target: Hex.fromJson(m['target']),
        fuelCost: _f(m['fuel_cost']),
        fuelAfter: _f(m['fuel_after']),
        canJump: m['can_jump'] as bool,
        hopsHome: _i(m['hops_home']),
        fuelToReturn: _f(m['fuel_to_return']),
        canReturn: m['can_return'] as bool,
        threat: ThreatInfo.fromJson(m['threat']),
        fleetPower: _f(m['fleet_power']),
        ratio: _f(m['ratio']),
        label: RatioLabel.fromJson(m['label']),
      );

  @override
  Json toJson() => {
        'type': 'preview_move',
        'fleet_id': fleetId,
        'target': target.toJson(),
        'fuel_cost': fuelCost,
        'fuel_after': fuelAfter,
        'can_jump': canJump,
        'hops_home': hopsHome,
        'fuel_to_return': fuelToReturn,
        'can_return': canReturn,
        'threat': threat.toJson(),
        'fleet_power': fleetPower,
        'ratio': ratio,
        'label': label.toJson(),
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

  /// Fuel one jump costs this fleet; below it, away from home, it is stranded.
  final double jumpFuel;

  /// Fuel the way home costs over charted lanes (0 at the homeworld).
  final double homeFuel;
  final int cooldownRemaining;

  /// Combat power of the whole fleet (the raid and threat scale).
  final double power;

  /// Outward hops the fleet can still fly and get home.
  final int rangeHops;

  /// At the homeworld with cargo the stockpile has no room for.
  final bool cargoBlocked;
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
    this.jumpFuel = 0,
    this.homeFuel = 0,
    this.cooldownRemaining = 0,
    this.power = 0,
    this.rangeHops = 0,
    this.cargoBlocked = false,
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
      jumpFuel: m['jump_fuel'] == null ? 0 : _f(m['jump_fuel']),
      homeFuel: m['home_fuel'] == null ? 0 : _f(m['home_fuel']),
      cooldownRemaining: (m['cooldown_remaining'] as int?) ?? 0,
      power: m['power'] == null ? 0 : _f(m['power']),
      rangeHops: (m['range_hops'] as int?) ?? 0,
      cargoBlocked: (m['cargo_blocked'] as bool?) ?? false,
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
      'jump_fuel': jumpFuel,
      'home_fuel': homeFuel,
      'cooldown_remaining': cooldownRemaining,
      'power': power,
      'range_hops': rangeHops,
      'cargo_blocked': cargoBlocked,
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

  /// Tick the wreckage pile despawns if nobody collects it.
  final int? salvageDespawnTick;
  final SiteBrief? site;

  /// Raw units left on the ore tiles, as of [lastSeen]; null without ore.
  final OreReserve? oreReserve;

  /// Danger of the sector; when not [live] it is the rating last seen.
  final ThreatInfo threat;

  /// Tick this sector was last observed; the current tick when [live].
  final int lastSeen;

  /// True when the server is watching the sector this tick. False is chart
  /// memory: resources, hostiles, salvage and site are as last observed at
  /// [lastSeen] and may have changed since.
  final bool live;
  const SectorState({
    required this.location,
    required this.terrain,
    required this.resources,
    required this.connections,
    this.hostiles,
    this.playerFleets,
    this.salvage,
    this.salvageDespawnTick,
    this.site,
    this.oreReserve,
    required this.threat,
    required this.lastSeen,
    required this.live,
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
      salvageDespawnTick: _opt(m['salvage_despawn_tick'], _i),
      site: _opt(m['site'], SiteBrief.fromJson),
      oreReserve: _opt(m['ore_reserve'], OreReserve.fromJson),
      threat: ThreatInfo.fromJson(m['threat']),
      lastSeen: _i(m['last_seen']),
      live: m['live'] as bool,
    );
  }

  Json toJson() {
    final m = <String, dynamic>{
      'location': location.toJson(),
      'terrain': terrain.toJson(),
      'resources': resources.toJson(),
      'connections': connections.map((e) => e.toJson()).toList(),
      'threat': threat.toJson(),
      'last_seen': lastSeen,
      'live': live,
    };
    _put(m, 'hostiles', hostiles?.map((e) => e.toJson()).toList());
    _put(m, 'player_fleets', playerFleets?.map((e) => e.toJson()).toList());
    _put(m, 'salvage', salvage?.toJson());
    _put(m, 'salvage_despawn_tick', salvageDespawnTick);
    _put(m, 'site', site?.toJson());
    _put(m, 'ore_reserve', oreReserve?.toJson());
    return m;
  }
}

/// The ore of a sector tile by tile: harvesting takes [TileReserve.units]
/// toward 0 and a tile refills on a timer while no fleet sits on it.
class OreReserve {
  final TileReserve metal;
  final TileReserve crystal;
  final TileReserve deuterium;
  const OreReserve({required this.metal, required this.crystal, required this.deuterium});

  factory OreReserve.fromJson(Object? json) {
    final m = _obj(json);
    return OreReserve(
      metal: TileReserve.fromJson(m['metal']),
      crystal: TileReserve.fromJson(m['crystal']),
      deuterium: TileReserve.fromJson(m['deuterium']),
    );
  }

  Json toJson() => {'metal': metal.toJson(), 'crystal': crystal.toJson(), 'deuterium': deuterium.toJson()};
}

class TileReserve {
  /// Raw units left to harvest.
  final double units;

  /// Units the tile holds when full (0 for a sector without this ore).
  final double maxUnits;

  /// Seconds until the tile is full again, if it is not.
  final int? refillsInS;
  const TileReserve({required this.units, required this.maxUnits, this.refillsInS});

  factory TileReserve.fromJson(Object? json) {
    final m = _obj(json);
    return TileReserve(
      units: _f(m['units']),
      maxUnits: _f(m['max_units']),
      refillsInS: _opt(m['refills_in_s'], _i),
    );
  }

  Json toJson() {
    final m = <String, dynamic>{'units': units, 'max_units': maxUnits};
    _put(m, 'refills_in_s', refillsInS);
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

  /// Mine output per tick at current levels (the cap discards the excess).
  final Resources production;
  final StorageState storage;
  final List<BuildingState> buildings;
  final List<ResearchState> research;

  /// Buildings under construction, at most [buildSlots] of them.
  final List<BuildQueueItem> buildQueue;

  /// Waiting behind them; nothing is paid until an item starts.
  final List<QueuedBuild> buildPending;
  final int buildSlots;

  /// Orders that may wait behind the running ones in each queue; the
  /// building queue holds [buildSlots] + this, research and shipyard 1 + this.
  final int queueWaitingMax;
  final ShipyardQueueItem? shipyardQueue;
  final List<QueuedShip> shipyardPending;
  final ResearchItem? researchActive;
  final List<QueuedResearch> researchPending;
  final List<ShipState> dockedShips;

  /// Structures standing at home.
  final List<DefenceState> defences;

  /// Docked ships, the Defense Grid and structures: what the next raid meets.
  final double homeDefencePower;

  /// Typical power of the next raid at the empire's current size.
  final double nextRaidEstimatePower;

  /// Costs, times and prerequisites for every building, tech and ship class.
  final HomeworldCatalog catalog;
  const HomeworldState({
    required this.location,
    required this.production,
    required this.storage,
    required this.buildings,
    required this.research,
    this.buildQueue = const [],
    this.buildPending = const [],
    this.buildSlots = 1,
    this.queueWaitingMax = 3,
    this.shipyardQueue,
    this.shipyardPending = const [],
    this.researchActive,
    this.researchPending = const [],
    required this.dockedShips,
    this.defences = const [],
    this.homeDefencePower = 0,
    this.nextRaidEstimatePower = 0,
    required this.catalog,
  });

  factory HomeworldState.fromJson(Object? json) {
    final m = _obj(json);
    return HomeworldState(
      location: Hex.fromJson(m['location']),
      production: Resources.fromJson(m['production']),
      storage: StorageState.fromJson(m['storage']),
      buildings: _list(m['buildings'], BuildingState.fromJson),
      research: _list(m['research'], ResearchState.fromJson),
      buildQueue: _list(m['build_queue'], BuildQueueItem.fromJson),
      buildPending: _list(m['build_pending'], QueuedBuild.fromJson),
      buildSlots: _i(m['build_slots']),
      queueWaitingMax: _i(m['queue_waiting_max']),
      shipyardQueue: _opt(m['shipyard_queue'], ShipyardQueueItem.fromJson),
      shipyardPending: _list(m['shipyard_pending'], QueuedShip.fromJson),
      researchActive: _opt(m['research_active'], ResearchItem.fromJson),
      researchPending: _list(m['research_pending'], QueuedResearch.fromJson),
      dockedShips: _list(m['docked_ships'], ShipState.fromJson),
      defences: _list(m['defences'], DefenceState.fromJson),
      homeDefencePower: _f(m['home_defence_power']),
      nextRaidEstimatePower: _f(m['next_raid_estimate_power']),
      catalog: HomeworldCatalog.fromJson(m['catalog']),
    );
  }

  Json toJson() {
    final m = <String, dynamic>{
      'location': location.toJson(),
      'production': production.toJson(),
      'storage': storage.toJson(),
      'buildings': buildings.map((e) => e.toJson()).toList(),
      'research': research.map((e) => e.toJson()).toList(),
      'build_queue': buildQueue.map((e) => e.toJson()).toList(),
      'build_pending': buildPending.map((e) => e.toJson()).toList(),
      'build_slots': buildSlots,
      'queue_waiting_max': queueWaitingMax,
      'shipyard_pending': shipyardPending.map((e) => e.toJson()).toList(),
      'research_pending': researchPending.map((e) => e.toJson()).toList(),
      'docked_ships': dockedShips.map((e) => e.toJson()).toList(),
      'defences': defences.map((e) => e.toJson()).toList(),
      'home_defence_power': homeDefencePower,
      'next_raid_estimate_power': nextRaidEstimatePower,
      'catalog': catalog.toJson(),
    };
    _put(m, 'shipyard_queue', shipyardQueue?.toJson());
    _put(m, 'research_active', researchActive?.toJson());
    return m;
  }

  /// A copy with some fields replaced (for tests and the offline demo).
  HomeworldState copyWith({
    Resources? production,
    StorageState? storage,
    List<BuildingState>? buildings,
    List<ResearchState>? research,
    List<DefenceState>? defences,
    double? homeDefencePower,
    List<BuildQueueItem>? buildQueue,
    List<QueuedBuild>? buildPending,
    int? buildSlots,
    ShipyardQueueItem? shipyardQueue,
    bool clearShipyardQueue = false,
    List<QueuedShip>? shipyardPending,
    ResearchItem? researchActive,
    bool clearResearchActive = false,
    List<QueuedResearch>? researchPending,
    HomeworldCatalog? catalog,
  }) =>
      HomeworldState(
        location: location,
        production: production ?? this.production,
        storage: storage ?? this.storage,
        buildings: buildings ?? this.buildings,
        research: research ?? this.research,
        buildQueue: buildQueue ?? this.buildQueue,
        buildPending: buildPending ?? this.buildPending,
        buildSlots: buildSlots ?? this.buildSlots,
        queueWaitingMax: queueWaitingMax,
        shipyardQueue: clearShipyardQueue ? null : (shipyardQueue ?? this.shipyardQueue),
        shipyardPending: shipyardPending ?? this.shipyardPending,
        researchActive: clearResearchActive ? null : (researchActive ?? this.researchActive),
        researchPending: researchPending ?? this.researchPending,
        dockedShips: dockedShips,
        defences: defences ?? this.defences,
        homeDefencePower: homeDefencePower ?? this.homeDefencePower,
        nextRaidEstimatePower: nextRaidEstimatePower,
        catalog: catalog ?? this.catalog,
      );

  int buildingLevel(BuildingType t) {
    for (final b in buildings) {
      if (b.buildingType == t) return b.level;
    }
    return 0;
  }
}

class DefenceState {
  final DefenceKind kind;
  final int count;

  /// Structures a repelled raid destroyed that come back for free.
  final int restoring;

  /// Power of one structure at the current Defense Grid level.
  final double power;
  const DefenceState({required this.kind, required this.count, this.restoring = 0, required this.power});

  factory DefenceState.fromJson(Object? json) {
    final m = _obj(json);
    return DefenceState(
      kind: DefenceKind.fromJson(m['kind']),
      count: _i(m['count']),
      restoring: _i(m['restoring']),
      power: _f(m['power']),
    );
  }

  Json toJson() => {'kind': kind.toJson(), 'count': count, 'restoring': restoring, 'power': power};
}

/// Stockpile limits from the Storage Vault level and the world pace.
class StorageState {
  final Resources cap;

  /// What a raid cannot skim.
  final Resources protected;

  /// Seconds until each stockpile is full at current production; 0 when it
  /// is, null when it is not filling.
  final ResourceEta fullInS;

  /// Resources whose stockpile is at the cap right now.
  final List<ResourceKind> capped;
  const StorageState({required this.cap, required this.protected, required this.fullInS, required this.capped});

  factory StorageState.fromJson(Object? json) {
    final m = _obj(json);
    return StorageState(
      cap: Resources.fromJson(m['cap']),
      protected: Resources.fromJson(m['protected']),
      fullInS: ResourceEta.fromJson(m['full_in_s']),
      capped: _list(m['capped'], ResourceKind.fromJson),
    );
  }

  Json toJson() => {
        'cap': cap.toJson(),
        'protected': protected.toJson(),
        'full_in_s': fullInS.toJson(),
        'capped': capped.map((e) => e.toJson()).toList(),
      };
}

class ResourceEta {
  final int? metal;
  final int? crystal;
  final int? deuterium;
  const ResourceEta({this.metal, this.crystal, this.deuterium});

  factory ResourceEta.fromJson(Object? json) {
    final m = _obj(json);
    return ResourceEta(
      metal: m['metal'] as int?,
      crystal: m['crystal'] as int?,
      deuterium: m['deuterium'] as int?,
    );
  }

  Json toJson() => {'metal': metal, 'crystal': crystal, 'deuterium': deuterium};

  int? of(ResourceKind k) => switch (k) {
        ResourceKind.metal => metal,
        ResourceKind.crystal => crystal,
        ResourceKind.deuterium => deuterium,
      };
}

/// Server-computed menu of homeworld actions (see `HomeworldCatalog` in
/// shared/src/protocol.rs). Whether the player can pay is the client's call:
/// compare a cost with the stockpile.
class HomeworldCatalog {
  final List<BuildingOption> buildings;
  final List<ResearchOption> research;
  final List<ShipOption> ships;
  final List<DefenceOption> defences;
  const HomeworldCatalog({
    required this.buildings,
    required this.research,
    required this.ships,
    this.defences = const [],
  });

  factory HomeworldCatalog.fromJson(Object? json) {
    final m = _obj(json);
    return HomeworldCatalog(
      buildings: _list(m['buildings'], BuildingOption.fromJson),
      research: _list(m['research'], ResearchOption.fromJson),
      ships: _list(m['ships'], ShipOption.fromJson),
      defences: _list(m['defences'], DefenceOption.fromJson),
    );
  }

  Json toJson() => {
        'buildings': buildings.map((e) => e.toJson()).toList(),
        'research': research.map((e) => e.toJson()).toList(),
        'ships': ships.map((e) => e.toJson()).toList(),
        'defences': defences.map((e) => e.toJson()).toList(),
      };
}

/// A building or tech that must reach [need]; [have] is the player's level.
class Requirement {
  final String name;
  final int need;
  final int have;
  final bool met;
  const Requirement({required this.name, required this.need, required this.have, required this.met});

  /// Compact form for cards: "Shipyard >= 4".
  String get label => '$name >= $need';

  factory Requirement.fromJson(Object? json) {
    final m = _obj(json);
    return Requirement(name: m['name'] as String, need: _i(m['need']), have: _i(m['have']), met: m['met'] as bool);
  }

  Json toJson() => {'name': name, 'need': need, 'have': have, 'met': met};
}

/// Cost and duration of reaching [level].
class UpgradeStep {
  final int level;
  final Resources cost;
  final int ticks;
  const UpgradeStep({required this.level, required this.cost, required this.ticks});

  factory UpgradeStep.fromJson(Object? json) {
    final m = _obj(json);
    return UpgradeStep(level: _i(m['level']), cost: Resources.fromJson(m['cost']), ticks: _i(m['ticks']));
  }

  Json toJson() => {'level': level, 'cost': cost.toJson(), 'ticks': ticks};
}

List<Requirement> _requirements(Object? v) => _list(v, Requirement.fromJson);

class BuildingOption {
  final BuildingType buildingType;
  final int level;
  final int maxLevel;

  /// Null at [maxLevel].
  final UpgradeStep? next;
  final List<Requirement> requires;
  const BuildingOption({
    required this.buildingType,
    required this.level,
    required this.maxLevel,
    this.next,
    required this.requires,
  });

  factory BuildingOption.fromJson(Object? json) {
    final m = _obj(json);
    return BuildingOption(
      buildingType: BuildingType.fromJson(m['building_type']),
      level: _i(m['level']),
      maxLevel: _i(m['max_level']),
      next: _opt(m['next'], UpgradeStep.fromJson),
      requires: _requirements(m['requires']),
    );
  }

  Json toJson() {
    final m = <String, dynamic>{
      'building_type': buildingType.toJson(),
      'level': level,
      'max_level': maxLevel,
      'requires': requires.map((e) => e.toJson()).toList(),
    };
    _put(m, 'next', next?.toJson());
    return m;
  }
}

class ResearchOption {
  final ResearchType tech;
  final int level;
  final int maxLevel;

  /// Null at [maxLevel].
  final UpgradeStep? next;
  final List<Requirement> requires;
  const ResearchOption({
    required this.tech,
    required this.level,
    required this.maxLevel,
    this.next,
    required this.requires,
  });

  factory ResearchOption.fromJson(Object? json) {
    final m = _obj(json);
    return ResearchOption(
      tech: ResearchType.fromJson(m['tech']),
      level: _i(m['level']),
      maxLevel: _i(m['max_level']),
      next: _opt(m['next'], UpgradeStep.fromJson),
      requires: _requirements(m['requires']),
    );
  }

  Json toJson() {
    final m = <String, dynamic>{
      'tech': tech.toJson(),
      'level': level,
      'max_level': maxLevel,
      'requires': requires.map((e) => e.toJson()).toList(),
    };
    _put(m, 'next', next?.toJson());
    return m;
  }
}

class ShipOption {
  final ShipClass shipClass;

  /// Cost of one ship; a batch costs count times this.
  final Resources unitCost;
  final int ticksPerShip;
  final List<Requirement> requires;
  const ShipOption({
    required this.shipClass,
    required this.unitCost,
    required this.ticksPerShip,
    required this.requires,
  });

  factory ShipOption.fromJson(Object? json) {
    final m = _obj(json);
    return ShipOption(
      shipClass: ShipClass.fromJson(m['ship_class']),
      unitCost: Resources.fromJson(m['unit_cost']),
      ticksPerShip: _i(m['ticks_per_ship']),
      requires: _requirements(m['requires']),
    );
  }

  Json toJson() => {
        'ship_class': shipClass.toJson(),
        'unit_cost': unitCost.toJson(),
        'ticks_per_ship': ticksPerShip,
        'requires': requires.map((e) => e.toJson()).toList(),
      };
}

class DefenceOption {
  final DefenceKind kind;

  /// Standing now.
  final int count;
  final Resources unitCost;
  final int ticksPerUnit;
  final double power;
  final List<Requirement> requires;
  const DefenceOption({
    required this.kind,
    required this.count,
    required this.unitCost,
    required this.ticksPerUnit,
    required this.power,
    required this.requires,
  });

  factory DefenceOption.fromJson(Object? json) {
    final m = _obj(json);
    return DefenceOption(
      kind: DefenceKind.fromJson(m['kind']),
      count: _i(m['count']),
      unitCost: Resources.fromJson(m['unit_cost']),
      ticksPerUnit: _i(m['ticks_per_unit']),
      power: _f(m['power']),
      requires: _requirements(m['requires']),
    );
  }

  Json toJson() => {
        'kind': kind.toJson(),
        'count': count,
        'unit_cost': unitCost.toJson(),
        'ticks_per_unit': ticksPerUnit,
        'power': power,
        'requires': requires.map((e) => e.toJson()).toList(),
      };
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

/// A building waiting for a free slot, resources or a prerequisite.
/// [cost] and [ticks] are what it would cost and take if it started now;
/// [waitingFor] is the resources still missing (null when it waits for
/// something else).
class QueuedBuild {
  final BuildingType buildingType;
  final int targetLevel;
  final Resources cost;
  final int ticks;
  final Resources? waitingFor;
  final WaitReason waitingOn;
  final int? startIn;
  const QueuedBuild({
    required this.buildingType,
    required this.targetLevel,
    required this.cost,
    required this.ticks,
    this.waitingFor,
    this.waitingOn = WaitReason.slot,
    this.startIn,
  });

  factory QueuedBuild.fromJson(Object? json) {
    final m = _obj(json);
    return QueuedBuild(
      buildingType: BuildingType.fromJson(m['building_type']),
      targetLevel: _i(m['target_level']),
      cost: Resources.fromJson(m['cost']),
      ticks: _i(m['ticks']),
      waitingFor: _opt(m['waiting_for'], Resources.fromJson),
      waitingOn: WaitReason.fromJson(m['waiting_on']),
      startIn: _opt(m['start_in'], _i),
    );
  }

  Json toJson() {
    final m = <String, dynamic>{
      'building_type': buildingType.toJson(),
      'target_level': targetLevel,
      'cost': cost.toJson(),
      'ticks': ticks,
    };
    _put(m, 'waiting_for', waitingFor?.toJson());
    m['waiting_on'] = waitingOn.toJson();
    _put(m, 'start_in', startIn);
    return m;
  }
}

class QueuedResearch {
  final ResearchType tech;
  final int targetLevel;
  final Resources cost;
  final int ticks;
  final Resources? waitingFor;
  final WaitReason waitingOn;
  final int? startIn;
  const QueuedResearch({
    required this.tech,
    required this.targetLevel,
    required this.cost,
    required this.ticks,
    this.waitingFor,
    this.waitingOn = WaitReason.slot,
    this.startIn,
  });

  factory QueuedResearch.fromJson(Object? json) {
    final m = _obj(json);
    return QueuedResearch(
      tech: ResearchType.fromJson(m['tech']),
      targetLevel: _i(m['target_level']),
      cost: Resources.fromJson(m['cost']),
      ticks: _i(m['ticks']),
      waitingFor: _opt(m['waiting_for'], Resources.fromJson),
      waitingOn: WaitReason.fromJson(m['waiting_on']),
      startIn: _opt(m['start_in'], _i),
    );
  }

  Json toJson() {
    final m = <String, dynamic>{
      'tech': tech.toJson(),
      'target_level': targetLevel,
      'cost': cost.toJson(),
      'ticks': ticks,
    };
    _put(m, 'waiting_for', waitingFor?.toJson());
    m['waiting_on'] = waitingOn.toJson();
    _put(m, 'start_in', startIn);
    return m;
  }
}

/// A shipyard order waiting its turn. [cost] is for the whole batch,
/// [ticks] for one ship.
class QueuedShip {
  final ShipyardItem item;
  final int count;
  final Resources cost;
  final int ticks;
  final Resources? waitingFor;
  final WaitReason waitingOn;
  final int? startIn;
  const QueuedShip({
    required this.item,
    required this.count,
    required this.cost,
    required this.ticks,
    this.waitingFor,
    this.waitingOn = WaitReason.slot,
    this.startIn,
  });

  factory QueuedShip.fromJson(Object? json) {
    final m = _obj(json);
    return QueuedShip(
      item: ShipyardItem.fromJson(m['item']),
      count: _i(m['count']),
      cost: Resources.fromJson(m['cost']),
      ticks: _i(m['ticks']),
      waitingFor: _opt(m['waiting_for'], Resources.fromJson),
      waitingOn: WaitReason.fromJson(m['waiting_on']),
      startIn: _opt(m['start_in'], _i),
    );
  }

  Json toJson() {
    final m = <String, dynamic>{
      'item': item.toJson(),
      'count': count,
      'cost': cost.toJson(),
      'ticks': ticks,
    };
    _put(m, 'waiting_for', waitingFor?.toJson());
    m['waiting_on'] = waitingOn.toJson();
    _put(m, 'start_in', startIn);
    return m;
  }
}

class ShipyardQueueItem {
  final ShipyardItem item;
  final int count;
  final int built;
  final int startTick;
  final int endTick;
  const ShipyardQueueItem({
    required this.item,
    required this.count,
    required this.built,
    required this.startTick,
    required this.endTick,
  });

  factory ShipyardQueueItem.fromJson(Object? json) {
    final m = _obj(json);
    return ShipyardQueueItem(
      item: ShipyardItem.fromJson(m['item']),
      count: _i(m['count']),
      built: _i(m['built']),
      startTick: _i(m['start_tick']),
      endTick: _i(m['end_tick']),
    );
  }

  Json toJson() => {
        'item': item.toJson(),
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
      'DefenceBuilt' => DefenceBuiltEvent.fromJson(m),
      'ScanCompleted' => ScanCompletedEvent.fromJson(m),
      'RaidIncoming' => RaidIncomingEvent.fromJson(m),
      'RaidResolved' => RaidResolvedEvent.fromJson(m),
      'SiteExplorationStarted' => SiteExplorationStartedEvent.fromJson(m),
      'SiteExplored' => SiteExploredEvent.fromJson(m),
      'SiteAmbush' => SiteAmbushEvent.fromJson(m),
      'PolicyAction' => PolicyActionEvent.fromJson(m),
      'Alert' => AlertEvent.fromJson(m),
      'SalvageDespawned' => SalvageDespawnedEvent.fromJson(m),
      'StorageNearCap' => StorageNearCapEvent.fromJson(m),
      'StorageFull' => StorageFullEvent.fromJson(m),
      'ChartDelivered' => ChartDeliveredEvent.fromJson(m),
      'Queue' => QueueEvent.fromJson(m),
      final k => throw FormatException('unknown EventKind "$k"'),
    };
  }
}

/// One shot in a fight. `attackerOwner` / `targetOwner` name the owning
/// empire; null is an NPC. [mine]: one of the two fleets is the receiver's.
class CombatRoundEvent extends EventKind {
  final Hex sector;
  final int attackerFleetId;
  final String? attackerOwner;
  final int attackerShipId;
  final int targetFleetId;
  final String? targetOwner;
  final int targetShipId;
  final double damage;
  final double shieldAbsorbed;
  final double hullDamage;
  final bool rapidFire;
  final bool mine;
  const CombatRoundEvent({
    required this.sector,
    required this.attackerFleetId,
    required this.attackerOwner,
    required this.attackerShipId,
    required this.targetFleetId,
    required this.targetOwner,
    required this.targetShipId,
    required this.damage,
    required this.shieldAbsorbed,
    required this.hullDamage,
    required this.rapidFire,
    required this.mine,
  });

  factory CombatRoundEvent.fromJson(Json m) => CombatRoundEvent(
        sector: Hex.fromJson(m['sector']),
        attackerFleetId: _i(m['attacker_fleet_id']),
        attackerOwner: m['attacker_owner'] as String?,
        attackerShipId: _i(m['attacker_ship_id']),
        targetFleetId: _i(m['target_fleet_id']),
        targetOwner: m['target_owner'] as String?,
        targetShipId: _i(m['target_ship_id']),
        damage: _f(m['damage']),
        shieldAbsorbed: _f(m['shield_absorbed']),
        hullDamage: _f(m['hull_damage']),
        rapidFire: m['rapid_fire'] as bool,
        mine: m['mine'] as bool? ?? false,
      );

  @override
  Json toJson() => {
        'kind': 'CombatRound',
        'sector': sector.toJson(),
        'attacker_fleet_id': attackerFleetId,
        'attacker_owner': attackerOwner,
        'attacker_ship_id': attackerShipId,
        'target_fleet_id': targetFleetId,
        'target_owner': targetOwner,
        'target_ship_id': targetShipId,
        'damage': damage,
        'shield_absorbed': shieldAbsorbed,
        'hull_damage': hullDamage,
        'rapid_fire': rapidFire,
        'mine': mine,
      };
}

class ShipDestroyedEvent extends EventKind {
  final int shipId;
  final ShipClass shipClass;
  final int ownerFleetId;
  final bool isNpc;
  final Hex sector;

  /// Empire that owned the ship; null for an NPC.
  final String? owner;

  /// The ship was the receiver's.
  final bool mine;
  const ShipDestroyedEvent({
    required this.shipId,
    required this.shipClass,
    required this.ownerFleetId,
    required this.isNpc,
    required this.sector,
    required this.owner,
    required this.mine,
  });

  factory ShipDestroyedEvent.fromJson(Json m) => ShipDestroyedEvent(
        shipId: _i(m['ship_id']),
        shipClass: ShipClass.fromJson(m['ship_class']),
        ownerFleetId: _i(m['owner_fleet_id']),
        isNpc: m['is_npc'] as bool,
        sector: Hex.fromJson(m['sector']),
        owner: m['owner'] as String?,
        mine: m['mine'] as bool? ?? false,
      );

  @override
  Json toJson() => {
        'kind': 'ShipDestroyed',
        'ship_id': shipId,
        'ship_class': shipClass.toJson(),
        'owner_fleet_id': ownerFleetId,
        'is_npc': isNpc,
        'sector': sector.toJson(),
        'owner': owner,
        'mine': mine,
      };
}

/// A fleet lost every ship. For an NPC, [salvage] is the wreckage left in
/// [sector], exactly what `collect_salvage` can take; player fleets leave none.
class FleetDestroyedEvent extends EventKind {
  final int fleetId;
  final bool isNpc;
  final Hex sector;
  final String? owner;
  final bool mine;
  final Resources salvage;

  /// What an NPC group was ("3x corvette pack"); empty for a player fleet.
  final String label;
  const FleetDestroyedEvent({
    required this.fleetId,
    required this.isNpc,
    required this.sector,
    required this.owner,
    required this.mine,
    required this.salvage,
    this.label = '',
  });

  factory FleetDestroyedEvent.fromJson(Json m) => FleetDestroyedEvent(
        fleetId: _i(m['fleet_id']),
        isNpc: m['is_npc'] as bool,
        sector: Hex.fromJson(m['sector']),
        owner: m['owner'] as String?,
        mine: m['mine'] as bool? ?? false,
        salvage: Resources.fromJson(m['salvage']),
        label: m['label'] as String? ?? '',
      );

  @override
  Json toJson() => {
        'kind': 'FleetDestroyed',
        'fleet_id': fleetId,
        'is_npc': isNpc,
        'sector': sector.toJson(),
        'owner': owner,
        'mine': mine,
        'salvage': salvage.toJson(),
        if (label.isNotEmpty) 'label': label,
      };
}

/// What a harvesting fleet took aboard over the last [ticks] ticks.
class ResourceHarvestedEvent extends EventKind {
  final int fleetId;
  final Resources resources;
  final int ticks;
  const ResourceHarvestedEvent({required this.fleetId, required this.resources, required this.ticks});

  factory ResourceHarvestedEvent.fromJson(Json m) => ResourceHarvestedEvent(
        fleetId: _i(m['fleet_id']),
        resources: Resources.fromJson(m['resources']),
        ticks: _i(m['ticks']),
      );

  @override
  Json toJson() => {
        'kind': 'ResourceHarvested',
        'fleet_id': fleetId,
        'resources': resources.toJson(),
        'ticks': ticks,
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

  /// Empire owning [playerFleetId].
  final String owner;
  final int enemyFleetId;

  /// What the hostile group is ("3x corvette pack"); may be empty.
  final String enemy;
  final Hex sector;

  /// [playerFleetId] is the receiver's.
  final bool mine;
  const CombatStartedEvent({
    required this.playerFleetId,
    required this.owner,
    required this.enemyFleetId,
    this.enemy = '',
    required this.sector,
    required this.mine,
  });

  factory CombatStartedEvent.fromJson(Json m) => CombatStartedEvent(
        playerFleetId: _i(m['player_fleet_id']),
        owner: m['owner'] as String,
        enemyFleetId: _i(m['enemy_fleet_id']),
        enemy: m['enemy'] as String? ?? '',
        sector: Hex.fromJson(m['sector']),
        mine: m['mine'] as bool? ?? false,
      );

  @override
  Json toJson() => {
        'kind': 'CombatStarted',
        'player_fleet_id': playerFleetId,
        'owner': owner,
        'enemy_fleet_id': enemyFleetId,
        if (enemy.isNotEmpty) 'enemy': enemy,
        'sector': sector.toJson(),
        'mine': mine,
      };
}

class CombatEndedEvent extends EventKind {
  final Hex sector;
  final bool playerVictory;

  /// Empires that fought.
  final List<String> owners;
  final bool mine;

  /// The wreckage now lying in [sector] after a victory.
  final Resources? salvage;
  final int? salvageDespawnTick;
  const CombatEndedEvent({
    required this.sector,
    required this.playerVictory,
    required this.owners,
    required this.mine,
    this.salvage,
    this.salvageDespawnTick,
  });

  factory CombatEndedEvent.fromJson(Json m) => CombatEndedEvent(
        sector: Hex.fromJson(m['sector']),
        playerVictory: m['player_victory'] as bool,
        owners: _list(m['owners'], (v) => v as String),
        mine: m['mine'] as bool? ?? false,
        salvage: _opt(m['salvage'], Resources.fromJson),
        salvageDespawnTick: _opt(m['salvage_despawn_tick'], _i),
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{
      'kind': 'CombatEnded',
      'sector': sector.toJson(),
      'player_victory': playerVictory,
      'owners': owners,
      'mine': mine,
    };
    _put(m, 'salvage', salvage?.toJson());
    _put(m, 'salvage_despawn_tick', salvageDespawnTick);
    return m;
  }
}

/// A fleet scooped wreckage. [remaining] is what is still on the ground when
/// the hold filled first.
class SalvageCollectedEvent extends EventKind {
  final int fleetId;
  final Hex sector;
  final Resources resources;
  final Resources? remaining;
  const SalvageCollectedEvent({
    required this.fleetId,
    required this.sector,
    required this.resources,
    this.remaining,
  });

  factory SalvageCollectedEvent.fromJson(Json m) => SalvageCollectedEvent(
        fleetId: _i(m['fleet_id']),
        sector: Hex.fromJson(m['sector']),
        resources: Resources.fromJson(m['resources']),
        remaining: _opt(m['remaining'], Resources.fromJson),
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{
      'kind': 'SalvageCollected',
      'fleet_id': fleetId,
      'sector': sector.toJson(),
      'resources': resources.toJson(),
    };
    _put(m, 'remaining', remaining?.toJson());
    return m;
  }
}

/// A wreckage pile expired uncollected.
class SalvageDespawnedEvent extends EventKind {
  final Hex sector;
  final Resources resources;
  const SalvageDespawnedEvent({required this.sector, required this.resources});

  factory SalvageDespawnedEvent.fromJson(Json m) => SalvageDespawnedEvent(
        sector: Hex.fromJson(m['sector']),
        resources: Resources.fromJson(m['resources']),
      );

  @override
  Json toJson() => {'kind': 'SalvageDespawned', 'sector': sector.toJson(), 'resources': resources.toJson()};
}

/// A stockpile passed 85 percent of its cap.
class StorageNearCapEvent extends EventKind {
  final int? playerId;
  final ResourceKind resource;
  final double ratio;

  /// Seconds until full at current production; null when not filling.
  final int? fullInS;
  const StorageNearCapEvent({this.playerId, required this.resource, required this.ratio, this.fullInS});

  factory StorageNearCapEvent.fromJson(Json m) => StorageNearCapEvent(
        playerId: m['player_id'] as int?,
        resource: ResourceKind.fromJson(m['resource']),
        ratio: _f(m['ratio']),
        fullInS: m['full_in_s'] as int?,
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{'kind': 'StorageNearCap', 'resource': resource.toJson(), 'ratio': ratio};
    _put(m, 'player_id', playerId);
    _put(m, 'full_in_s', fullInS);
    return m;
  }
}

/// A stockpile reached its cap; production beyond it is discarded.
class StorageFullEvent extends EventKind {
  final int? playerId;
  final ResourceKind resource;
  const StorageFullEvent({this.playerId, required this.resource});

  factory StorageFullEvent.fromJson(Json m) =>
      StorageFullEvent(playerId: m['player_id'] as int?, resource: ResourceKind.fromJson(m['resource']));

  @override
  Json toJson() {
    final m = <String, dynamic>{'kind': 'StorageFull', 'resource': resource.toJson()};
    _put(m, 'player_id', playerId);
    return m;
  }
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

class DefenceBuiltEvent extends EventKind {
  final DefenceKind defence;
  final int count;
  final int? playerId;
  const DefenceBuiltEvent({required this.defence, required this.count, this.playerId});

  factory DefenceBuiltEvent.fromJson(Json m) => DefenceBuiltEvent(
        defence: DefenceKind.fromJson(m['defence']),
        count: _i(m['count']),
        playerId: m['player_id'] as int?,
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{'kind': 'DefenceBuilt', 'defence': defence.toJson(), 'count': count};
    _put(m, 'player_id', playerId);
    return m;
  }
}

class ScanCompletedEvent extends EventKind {
  final int fleetId;
  final Hex sector;
  final int sectorsRevealed;
  final int hostilesDetected;
  final List<SectorThreat> threats;
  final List<SignalContact> signals;
  const ScanCompletedEvent({
    required this.fleetId,
    required this.sector,
    required this.sectorsRevealed,
    required this.hostilesDetected,
    this.threats = const [],
    this.signals = const [],
  });

  factory ScanCompletedEvent.fromJson(Json m) => ScanCompletedEvent(
        fleetId: _i(m['fleet_id']),
        sector: Hex.fromJson(m['sector']),
        sectorsRevealed: _i(m['sectors_revealed']),
        hostilesDetected: _i(m['hostiles_detected']),
        threats: m['threats'] == null ? const [] : _list(m['threats'], SectorThreat.fromJson),
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
    if (threats.isNotEmpty) m['threats'] = threats.map((e) => e.toJson()).toList();
    if (signals.isNotEmpty) m['signals'] = signals.map((e) => e.toJson()).toList();
    return m;
  }
}

class SignalContact {
  final Hex sector;
  final SignalKind signal;

  /// Coarse danger: 1 for T1 to T3, 2 for T4 to T6, 3 for T7 to T9.
  final int threatBand;
  const SignalContact({required this.sector, required this.signal, required this.threatBand});

  factory SignalContact.fromJson(Object? json) {
    final m = _obj(json);
    return SignalContact(
      sector: Hex.fromJson(m['sector']),
      signal: SignalKind.fromJson(m['signal']),
      threatBand: _i(m['threat_band']),
    );
  }

  Json toJson() => {'sector': sector.toJson(), 'signal': signal.toJson(), 'threat_band': threatBand};
}

class SectorThreat {
  final Hex sector;
  final ThreatInfo threat;
  const SectorThreat({required this.sector, required this.threat});

  factory SectorThreat.fromJson(Object? json) {
    final m = _obj(json);
    return SectorThreat(sector: Hex.fromJson(m['sector']), threat: ThreatInfo.fromJson(m['threat']));
  }

  Json toJson() => {'sector': sector.toJson(), 'threat': threat.toJson()};
}

class RaidIncomingEvent extends EventKind {
  final int playerId;
  final int arrivalTick;
  final String threat;

  /// The raid fleet's power, on the same scale as the homeworld's defence.
  final double estPower;
  const RaidIncomingEvent({
    required this.playerId,
    required this.arrivalTick,
    required this.threat,
    this.estPower = 0,
  });

  factory RaidIncomingEvent.fromJson(Json m) => RaidIncomingEvent(
        playerId: _i(m['player_id']),
        arrivalTick: _i(m['arrival_tick']),
        threat: m['threat'] as String,
        estPower: m['est_power'] == null ? 0 : _f(m['est_power']),
      );

  @override
  Json toJson() => {
        'kind': 'RaidIncoming',
        'player_id': playerId,
        'arrival_tick': arrivalTick,
        'threat': threat,
        'est_power': estPower,
      };
}

class RaidResolvedEvent extends EventKind {
  final int playerId;
  final bool defended;
  final Resources resourcesLost;
  final Resources? salvageDropped;
  final double raidPower;
  final double defensePower;
  final int structuresLost;
  final int structuresRestored;
  final Resources protectedKept;

  /// Ships docked at home that a lost raid destroyed.
  final int shipsLost;
  const RaidResolvedEvent({
    required this.playerId,
    required this.defended,
    required this.resourcesLost,
    this.salvageDropped,
    required this.raidPower,
    required this.defensePower,
    this.structuresLost = 0,
    this.structuresRestored = 0,
    this.protectedKept = const Resources(),
    this.shipsLost = 0,
  });

  factory RaidResolvedEvent.fromJson(Json m) => RaidResolvedEvent(
        playerId: _i(m['player_id']),
        defended: m['defended'] as bool,
        resourcesLost: Resources.fromJson(m['resources_lost']),
        salvageDropped: _opt(m['salvage_dropped'], Resources.fromJson),
        raidPower: _f(m['raid_power']),
        defensePower: _f(m['defense_power']),
        structuresLost: (m['structures_lost'] as int?) ?? 0,
        structuresRestored: (m['structures_restored'] as int?) ?? 0,
        protectedKept: m['protected_kept'] == null ? const Resources() : Resources.fromJson(m['protected_kept']),
        shipsLost: (m['ships_lost'] as int?) ?? 0,
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
      'structures_lost': structuresLost,
      'structures_restored': structuresRestored,
      'protected_kept': protectedKept.toJson(),
      'ships_lost': shipsLost,
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

  /// An ancient relic came aboard (worth 25 explore points).
  final bool relic;

  /// Explore points this boarding earned (0 when boarded before).
  final double points;
  const SiteExploredEvent({
    required this.fleetId,
    required this.sector,
    required this.tier,
    required this.resources,
    this.recoveredShip,
    this.techCache,
    this.relic = false,
    this.points = 0,
  });

  factory SiteExploredEvent.fromJson(Json m) => SiteExploredEvent(
        fleetId: _i(m['fleet_id']),
        sector: Hex.fromJson(m['sector']),
        tier: _i(m['tier']),
        resources: Resources.fromJson(m['resources']),
        recoveredShip: _opt(m['recovered_ship'], ShipClass.fromJson),
        techCache: _opt(m['tech_cache'], ResearchType.fromJson),
        relic: m['relic'] as bool? ?? false,
        points: m['points'] == null ? 0 : _f(m['points']),
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
    // skip_serializing_if = "Not::not" / "is_zero"
    if (relic) m['relic'] = true;
    if (points != 0) m['points'] = points;
    return m;
  }
}

/// A fleet docked and handed over the sectors it charted.
class ChartDeliveredEvent extends EventKind {
  final int fleetId;
  final int sectors;
  final double points;
  const ChartDeliveredEvent({required this.fleetId, required this.sectors, required this.points});

  factory ChartDeliveredEvent.fromJson(Json m) => ChartDeliveredEvent(
        fleetId: _i(m['fleet_id']),
        sectors: _i(m['sectors']),
        points: _f(m['points']),
      );

  @override
  Json toJson() => {'kind': 'ChartDelivered', 'fleet_id': fleetId, 'sectors': sectors, 'points': points};
}

/// The answer to a build, research, ship or defence order or a cancel, and
/// the notice that a waiting order started by itself.
class QueueEvent extends EventKind {
  final int? playerId;
  final QueueType queueType;

  /// "Metal Mine Lv.5", "Corvette x3".
  final String item;
  final QueueAction action;

  /// [QueueAction.started]: what was paid.
  final Resources paid;

  /// [QueueAction.cancelled]: what came back.
  final Resources refunded;

  /// [QueueAction.waiting]: the exact shortfall, null when nothing is short.
  final Resources? waitingFor;
  final WaitReason? waitingOn;

  /// [QueueAction.waiting]: estimated ticks until it starts.
  final int? startIn;
  const QueueEvent({
    this.playerId,
    required this.queueType,
    required this.item,
    required this.action,
    this.paid = const Resources(),
    this.refunded = const Resources(),
    this.waitingFor,
    this.waitingOn,
    this.startIn,
  });

  factory QueueEvent.fromJson(Json m) => QueueEvent(
        playerId: _opt(m['player_id'], _i),
        queueType: QueueType.fromJson(m['queue_type']),
        item: m['item'] as String,
        action: QueueAction.fromJson(m['action']),
        paid: m['paid'] == null ? const Resources() : Resources.fromJson(m['paid']),
        refunded: m['refunded'] == null ? const Resources() : Resources.fromJson(m['refunded']),
        waitingFor: _opt(m['waiting_for'], Resources.fromJson),
        waitingOn: _opt(m['waiting_on'], WaitReason.fromJson),
        startIn: _opt(m['start_in'], _i),
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{
      'kind': 'Queue',
      'queue_type': queueType.toJson(),
      'item': item,
      'action': action.toJson(),
      'paid': paid.toJson(),
      'refunded': refunded.toJson(),
    };
    _put(m, 'player_id', playerId);
    _put(m, 'waiting_for', waitingFor?.toJson());
    _put(m, 'waiting_on', waitingOn?.toJson());
    _put(m, 'start_in', startIn);
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
  /// Recipient; null is a server-wide notice.
  final int? playerId;
  final AlertLevel level;
  final String message;
  final Hex? sector;
  final int? fleetId;
  const AlertEvent({this.playerId, required this.level, required this.message, this.sector, this.fleetId});

  factory AlertEvent.fromJson(Json m) => AlertEvent(
        playerId: m['player_id'] as int?,
        level: AlertLevel.fromJson(m['level']),
        message: m['message'] as String,
        sector: _opt(m['sector'], Hex.fromJson),
        fleetId: m['fleet_id'] as int?,
      );

  @override
  Json toJson() {
    final m = <String, dynamic>{'kind': 'Alert', 'level': level.toJson(), 'message': message};
    _put(m, 'player_id', playerId);
    _put(m, 'sector', sector?.toJson());
    _put(m, 'fleet_id', fleetId);
    return m;
  }
}
