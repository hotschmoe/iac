// GENERATED FILE, DO NOT EDIT.
//
// The wire protocol, written from the Rust types in shared/src (everything
// ClientMessage and ServerMessage carry) by shared/tests/wire. Change the
// Rust, then run
//
//   UPDATE_FIXTURES=1 cargo test -p iac-shared
//
// and `cargo test -p iac-shared` fails for as long as this file is stale.
// What the client adds to these types is in protocol.dart.
//
// Decoding is strict: an unknown tag or enum name throws FormatException.
// A field serde may leave out is nullable or has serde's default here, and
// is left out again when serde would leave it out.
//
// dart format width=120

import 'dart:convert';

import 'hex.dart';

typedef Json = Map<String, dynamic>;

Json _obj(Object? v) => v as Json;
bool _bool(Object? v) => v as bool;
int _int(Object? v) => v as int;
double _double(Object? v) => (v as num).toDouble();
String _string(Object? v) => v as String;
List<T> _list<T>(Object? v, T Function(Object?) f) => [for (final e in v as List) f(e)];
T? _opt<T>(Object? v, T Function(Object?) f) => v == null ? null : f(v);

T _named<T extends Enum>(List<T> values, String Function(T) wire, Object? v) {
  for (final value in values) {
    if (wire(value) == v) return value;
  }
  throw FormatException('unknown $T "$v"');
}

bool _same(List<Object?> a, List<Object?> b) {
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

sealed class ClientMessage {
  const ClientMessage();

  static ClientMessage fromJson(Object? json) {
    return switch (_obj(json)['type']) {
      'auth' => AuthRequest.fromJson(json),
      'command' => CommandMessage.fromJson(json),
      'policy_update' => PolicyUpdate.fromJson(json),
      'request_full_state' => const RequestFullStateMessage(),
      final tag => throw FormatException('unknown ClientMessage type "$tag"'),
    };
  }

  static ClientMessage decode(String raw) => fromJson(jsonDecode(raw));

  Json toJson();

  String encode() => jsonEncode(toJson());
}

class CommandMessage extends ClientMessage {
  final Command command;

  const CommandMessage(this.command);

  factory CommandMessage.fromJson(Object? json) => CommandMessage(Command.fromJson(json));

  @override
  Json toJson() => {'type': 'command', ...command.toJson()};
}

class RequestFullStateMessage extends ClientMessage {
  const RequestFullStateMessage();

  @override
  Json toJson() => {'type': 'request_full_state'};
}

/// First message on a connection. The server decides what it means from the
/// name alone, so there is no separate register/login action:
///
/// - unknown name: valid names register a new account and the reply carries
///   the new secret in `AuthResult.token` (this is the only time it is sent);
///   a world with no free home seat replies `WorldFull` instead;
/// - known name: `token` must match the one issued earlier, otherwise the
///   reply is `TokenRequired` / `InvalidToken`;
/// - an account from before tokens existed has none yet and is claimed by
///   its next login, which is issued one the same way.
///
/// Names are matched case-insensitively ("admiral" logs in to "Admiral"), so
/// no two accounts can differ only by case. An exact-case match wins, which
/// keeps any case-variant accounts registered before this rule reachable.
class AuthRequest extends ClientMessage {
  final String playerName;
  final String? token;

  /// True when an AI agent, not a person, flies this account. It is only
  /// read when the account is created and only labels the leaderboard:
  /// agents and humans are one population with one scoring formula.
  final bool agent;

  const AuthRequest({required this.playerName, this.token, this.agent = false});

  factory AuthRequest.fromJson(Object? json) {
    final m = _obj(json);
    return AuthRequest(
      playerName: _string(m['player_name']),
      token: _opt(m['token'], _string),
      agent: _opt(m['agent'], _bool) ?? false,
    );
  }

  @override
  Json toJson() => {'type': 'auth', 'player_name': playerName, 'token': ?token, 'agent': agent};
}

sealed class Command {
  const Command();

  static Command fromJson(Object? json) {
    return switch (_obj(json)['action']) {
      'move' => MoveCommand.fromJson(json),
      'harvest' => HarvestCommand.fromJson(json),
      'attack' => AttackCommand.fromJson(json),
      'recall' => RecallCommand.fromJson(json),
      'collect_salvage' => CollectSalvageCommand.fromJson(json),
      'build' => BuildCommand.fromJson(json),
      'research' => ResearchCommand.fromJson(json),
      'build_ship' => BuildShipCommand.fromJson(json),
      'build_defence' => BuildDefenceCommand.fromJson(json),
      'cancel_build' => CancelBuildCommand.fromJson(json),
      'cancel_queued' => CancelQueuedCommand.fromJson(json),
      'leaderboard' => LeaderboardCommand.fromJson(json),
      'preview_move' => PreviewMoveCommand.fromJson(json),
      'odds' => OddsCommand.fromJson(json),
      'stop' => StopCommand.fromJson(json),
      'scan' => ScanCommand.fromJson(json),
      'explore_site' => ExploreSiteCommand.fromJson(json),
      'split' => SplitCommand.fromJson(json),
      'merge' => MergeCommand.fromJson(json),
      'exchange' => ExchangeCommand.fromJson(json),
      'exchange_quote' => ExchangeQuoteCommand.fromJson(json),
      'load' => LoadCommand.fromJson(json),
      'unload' => UnloadCommand.fromJson(json),
      'refine' => RefineCommand.fromJson(json),
      'found_outpost' => FoundOutpostCommand.fromJson(json),
      'outpost_survey' => OutpostSurveyCommand.fromJson(json),
      'upgrade_outpost' => const UpgradeOutpostCommand(),
      'abandon_outpost' => const AbandonOutpostCommand(),
      final tag => throw FormatException('unknown Command action "$tag"'),
    };
  }

  Json toJson();
}

class MoveCommand extends Command {
  final int fleetId;
  final Hex target;

  const MoveCommand({required this.fleetId, required this.target});

  factory MoveCommand.fromJson(Object? json) {
    final m = _obj(json);
    return MoveCommand(fleetId: _int(m['fleet_id']), target: Hex.fromJson(m['target']));
  }

  @override
  Json toJson() => {'action': 'move', 'fleet_id': fleetId, 'target': target.toJson()};
}

class HarvestCommand extends Command {
  final int fleetId;
  final HarvestResource resource;

  const HarvestCommand({required this.fleetId, required this.resource});

  factory HarvestCommand.fromJson(Object? json) {
    final m = _obj(json);
    return HarvestCommand(fleetId: _int(m['fleet_id']), resource: HarvestResource.fromJson(m['resource']));
  }

  @override
  Json toJson() => {'action': 'harvest', 'fleet_id': fleetId, 'resource': resource.toJson()};
}

class AttackCommand extends Command {
  final int fleetId;
  final int targetFleetId;

  const AttackCommand({required this.fleetId, required this.targetFleetId});

  factory AttackCommand.fromJson(Object? json) {
    final m = _obj(json);
    return AttackCommand(fleetId: _int(m['fleet_id']), targetFleetId: _int(m['target_fleet_id']));
  }

  @override
  Json toJson() => {'action': 'attack', 'fleet_id': fleetId, 'target_fleet_id': targetFleetId};
}

class RecallCommand extends Command {
  final int fleetId;

  const RecallCommand({required this.fleetId});

  factory RecallCommand.fromJson(Object? json) {
    final m = _obj(json);
    return RecallCommand(fleetId: _int(m['fleet_id']));
  }

  @override
  Json toJson() => {'action': 'recall', 'fleet_id': fleetId};
}

class CollectSalvageCommand extends Command {
  final int fleetId;

  const CollectSalvageCommand({required this.fleetId});

  factory CollectSalvageCommand.fromJson(Object? json) {
    final m = _obj(json);
    return CollectSalvageCommand(fleetId: _int(m['fleet_id']));
  }

  @override
  Json toJson() => {'action': 'collect_salvage', 'fleet_id': fleetId};
}

/// Queue a building level. The order is accepted whenever it fits the
/// queue, free slot or not, affordable or not, and gets a stable `id`
/// (reported in the `Queue` event and the queue lists). A free slot
/// starts the earliest waiting order that can start now (prerequisites
/// built, stockpile covers it); an order that cannot start keeps its
/// place and is re-checked every tick. `reserve: true` makes the order
/// hold the line: while it is ready but short of resources, nothing
/// behind it in the same queue starts, so an expensive order cannot be
/// starved by cheap ones. Refused only when it can never start
/// (`PrerequisitesNotMet`, `MaxLevelReached`, `StorageTooSmall`) or the
/// waiting line is full (`QueueFull`).
class BuildCommand extends Command {
  final BuildingType buildingType;
  final bool reserve;

  const BuildCommand({required this.buildingType, this.reserve = false});

  factory BuildCommand.fromJson(Object? json) {
    final m = _obj(json);
    return BuildCommand(
      buildingType: BuildingType.fromJson(m['building_type']),
      reserve: _opt(m['reserve'], _bool) ?? false,
    );
  }

  @override
  Json toJson() => {'action': 'build', 'building_type': buildingType.toJson(), 'reserve': reserve};
}

/// Queue a research level; same rule as `build`.
class ResearchCommand extends Command {
  final ResearchType tech;
  final bool reserve;

  const ResearchCommand({required this.tech, this.reserve = false});

  factory ResearchCommand.fromJson(Object? json) {
    final m = _obj(json);
    return ResearchCommand(tech: ResearchType.fromJson(m['tech']), reserve: _opt(m['reserve'], _bool) ?? false);
  }

  @override
  Json toJson() => {'action': 'research', 'tech': tech.toJson(), 'reserve': reserve};
}

/// Order `count` ships as one batch. Unlike a building or research
/// order, a batch is accepted only if the stockpile covers every unit
/// now, and it pays then; otherwise it is refused with
/// `InsufficientResources` and a message naming the shortfall per
/// resource. A paid batch gets a stable `id` and starts at once when the
/// yard is free and what it needs is built; otherwise it waits for the
/// yard or for something queued ahead of it (a Shipyard level, a tech).
/// Also refused with `NoShipyard`, `ShipLocked`, `StorageTooSmall` (a
/// batch larger than a stockpile cap) or `QueueFull`.
class BuildShipCommand extends Command {
  final ShipClass shipClass;
  final int count;

  const BuildShipCommand({required this.shipClass, this.count = 1});

  factory BuildShipCommand.fromJson(Object? json) {
    final m = _obj(json);
    return BuildShipCommand(shipClass: ShipClass.fromJson(m['ship_class']), count: _opt(m['count'], _int) ?? 1);
  }

  @override
  Json toJson() => {'action': 'build_ship', 'ship_class': shipClass.toJson(), 'count': count};
}

/// Order `count` defence structures in the shipyard queue; same rule as
/// `build_ship`. They stay at home and never burn fuel; rejected with
/// `DefenceLocked` until the Defense Grid and research it needs are in place.
class BuildDefenceCommand extends Command {
  final DefenceKind kind;
  final int count;

  const BuildDefenceCommand({required this.kind, this.count = 1});

  factory BuildDefenceCommand.fromJson(Object? json) {
    final m = _obj(json);
    return BuildDefenceCommand(kind: DefenceKind.fromJson(m['kind']), count: _opt(m['count'], _int) ?? 1);
  }

  @override
  Json toJson() => {'action': 'build_defence', 'kind': kind.toJson(), 'count': count};
}

/// Cancel an item that has started, by its stable `id`. A building or
/// research level gets back half of what it paid. A shipyard batch gets
/// back half the price of every unit not yet delivered (the unit under
/// way is not delivered; units already built are kept). A `Queue` event
/// with `action: Cancelled` says what was refunded. `InvalidTarget` when
/// `id` is not running in `queue_type`.
class CancelBuildCommand extends Command {
  final QueueType queueType;
  final int id;

  const CancelBuildCommand({required this.queueType, required this.id});

  factory CancelBuildCommand.fromJson(Object? json) {
    final m = _obj(json);
    return CancelBuildCommand(queueType: QueueType.fromJson(m['queue_type']), id: _int(m['id']));
  }

  @override
  Json toJson() => {'action': 'cancel_build', 'queue_type': queueType.toJson(), 'id': id};
}

/// Remove an item that is still waiting in the queue, by its stable `id`.
/// A waiting building or research order has paid nothing, so nothing is
/// refunded; a waiting shipyard batch paid when it was ordered and gets
/// half back (a `Queue` event with `action: Cancelled` says how much).
/// `InvalidTarget` when `id` is not waiting in `queue_type`, even if it
/// exists in another queue.
class CancelQueuedCommand extends Command {
  final QueueType queueType;
  final int id;

  const CancelQueuedCommand({required this.queueType, required this.id});

  factory CancelQueuedCommand.fromJson(Object? json) {
    final m = _obj(json);
    return CancelQueuedCommand(queueType: QueueType.fromJson(m['queue_type']), id: _int(m['id']));
  }

  @override
  Json toJson() => {'action': 'cancel_queued', 'queue_type': queueType.toJson(), 'id': id};
}

/// Ask for the score table. The reply is a `leaderboard` message.
class LeaderboardCommand extends Command {
  final int limit;

  const LeaderboardCommand({this.limit = 20});

  factory LeaderboardCommand.fromJson(Object? json) {
    final m = _obj(json);
    return LeaderboardCommand(limit: _opt(m['limit'], _int) ?? 20);
  }

  @override
  Json toJson() => {'action': 'leaderboard', 'limit': limit};
}

/// Ask what one jump of `fleet_id` to the adjacent sector `target` would
/// cost, without moving. The reply is a `preview_move` message.
class PreviewMoveCommand extends Command {
  final int fleetId;
  final Hex target;

  const PreviewMoveCommand({required this.fleetId, required this.target});

  factory PreviewMoveCommand.fromJson(Object? json) {
    final m = _obj(json);
    return PreviewMoveCommand(fleetId: _int(m['fleet_id']), target: Hex.fromJson(m['target']));
  }

  @override
  Json toJson() => {'action': 'preview_move', 'fleet_id': fleetId, 'target': target.toJson()};
}

/// Ask how `fleet_id` would fare in each of `sectors` (at most
/// `MAX_ODDS_SECTORS`, anywhere on the map, its own sector included),
/// without moving. The reply is an `odds` message.
class OddsCommand extends Command {
  final int fleetId;
  final List<Hex> sectors;

  const OddsCommand({required this.fleetId, required this.sectors});

  factory OddsCommand.fromJson(Object? json) {
    final m = _obj(json);
    return OddsCommand(fleetId: _int(m['fleet_id']), sectors: _list(m['sectors'], Hex.fromJson));
  }

  @override
  Json toJson() => {'action': 'odds', 'fleet_id': fleetId, 'sectors': sectors.map((e) => e.toJson()).toList()};
}

class StopCommand extends Command {
  final int fleetId;

  const StopCommand({this.fleetId = 0});

  factory StopCommand.fromJson(Object? json) {
    final m = _obj(json);
    return StopCommand(fleetId: _opt(m['fleet_id'], _int) ?? 0);
  }

  @override
  Json toJson() => {'action': 'stop', 'fleet_id': fleetId};
}

class ScanCommand extends Command {
  final int fleetId;

  const ScanCommand({this.fleetId = 0});

  factory ScanCommand.fromJson(Object? json) {
    final m = _obj(json);
    return ScanCommand(fleetId: _opt(m['fleet_id'], _int) ?? 0);
  }

  @override
  Json toJson() => {'action': 'scan', 'fleet_id': fleetId};
}

class ExploreSiteCommand extends Command {
  final int fleetId;

  const ExploreSiteCommand({this.fleetId = 0});

  factory ExploreSiteCommand.fromJson(Object? json) {
    final m = _obj(json);
    return ExploreSiteCommand(fleetId: _opt(m['fleet_id'], _int) ?? 0);
  }

  @override
  Json toJson() => {'action': 'explore_site', 'fleet_id': fleetId};
}

/// Detach the listed ships from `fleet_id` into a new fleet at the same
/// sector. The new fleet starts with no standing orders and takes a share
/// of fuel and cargo proportional to the tank and hold it carries off.
/// At least one ship must stay behind. Both fleets must be idle. Rejected
/// with `FleetLimitReached` at the fleet cap, `InvalidTarget` for ship ids
/// not in the fleet or an attempt to take every ship. At the homeworld
/// this is also how docked ships are launched as their own fleet. The
/// reply is an `Alert` (info) event naming the new fleet id.
class SplitCommand extends Command {
  final int fleetId;
  final List<int> shipIds;

  const SplitCommand({required this.fleetId, required this.shipIds});

  factory SplitCommand.fromJson(Object? json) {
    final m = _obj(json);
    return SplitCommand(fleetId: _int(m['fleet_id']), shipIds: _list(m['ship_ids'], _int));
  }

  @override
  Json toJson() => {'action': 'split', 'fleet_id': fleetId, 'ship_ids': shipIds};
}

/// Fold `other_fleet_id` into `fleet_id`. Both must be yours, idle, and in
/// the same sector (`NotInSector` otherwise). Ships, cargo and fuel are
/// pooled (fuel up to the combined tank), the merged fleet keeps the
/// standing orders of `fleet_id`, and `other_fleet_id` ceases to exist.
/// This works anywhere, so a fresh fleet can rescue a stranded one.
class MergeCommand extends Command {
  final int fleetId;
  final int otherFleetId;

  const MergeCommand({required this.fleetId, required this.otherFleetId});

  factory MergeCommand.fromJson(Object? json) {
    final m = _obj(json);
    return MergeCommand(fleetId: _int(m['fleet_id']), otherFleetId: _int(m['other_fleet_id']));
  }

  @override
  Json toJson() => {'action': 'merge', 'fleet_id': fleetId, 'other_fleet_id': otherFleetId};
}

/// Sell `amount` units of `give` from the hold of `fleet_id` to the hub's
/// exchange and take `want` into the hold; without `amount`, all of
/// `give` the fleet carries. The fleet must stand idle on a hub sector
/// that offers the exchange (`WorldInfo.hub`). The answer is an
/// `Exchanged` event; see `ExchangeTerms` for the rate. Refused, with
/// nothing changed, when the fleet is elsewhere (`NotInSector`) or busy
/// (`OnCooldown`), holds less than `amount` or none (`NoResources`),
/// would get back less than one unit, names one resource twice or an
/// `amount` that is not positive (`InvalidCommand`), or the trade would
/// overfill the hold (`CargoFull`).
class ExchangeCommand extends Command {
  final int fleetId;
  final ResourceKind give;
  final ResourceKind want;
  final double? amount;

  const ExchangeCommand({required this.fleetId, required this.give, required this.want, this.amount});

  factory ExchangeCommand.fromJson(Object? json) {
    final m = _obj(json);
    return ExchangeCommand(
      fleetId: _int(m['fleet_id']),
      give: ResourceKind.fromJson(m['give']),
      want: ResourceKind.fromJson(m['want']),
      amount: _opt(m['amount'], _double),
    );
  }

  @override
  Json toJson() => {
    'action': 'exchange',
    'fleet_id': fleetId,
    'give': give.toJson(),
    'want': want.toJson(),
    'amount': ?amount,
  };
}

/// Ask what `exchange` with the same fields would give, without trading.
/// The reply is an `exchange_quote` message. It answers for a fleet
/// anywhere and for an `amount` the fleet does not carry: `refusal` then
/// says why the trade itself would be refused right now.
class ExchangeQuoteCommand extends Command {
  final int fleetId;
  final ResourceKind give;
  final ResourceKind want;
  final double? amount;

  const ExchangeQuoteCommand({required this.fleetId, required this.give, required this.want, this.amount});

  factory ExchangeQuoteCommand.fromJson(Object? json) {
    final m = _obj(json);
    return ExchangeQuoteCommand(
      fleetId: _int(m['fleet_id']),
      give: ResourceKind.fromJson(m['give']),
      want: ResourceKind.fromJson(m['want']),
      amount: _opt(m['amount'], _double),
    );
  }

  @override
  Json toJson() => {
    'action': 'exchange_quote',
    'fleet_id': fleetId,
    'give': give.toJson(),
    'want': want.toJson(),
    'amount': ?amount,
  };
}

/// Move `amount` units of `resource` from the stockpile into the hold of
/// `fleet_id`; without `amount`, all the stockpile holds and the hold
/// has room for. One resource an order. The fleet must stand idle at
/// its owner's homeworld and fly no standing orders. It then keeps
/// everything in its hold aboard while it is docked
/// (`FleetState.laden`); once it has been away, what it brings home
/// unloads as cargo always does. `unload`, or giving the fleet standing
/// orders, ends it sooner. The answer is an `Alert` (info) event saying what
/// went aboard. Refused, with nothing moved, when the fleet is elsewhere
/// (`NotInSector`) or busy (`OnCooldown`), the stockpile holds less than
/// `amount` or none (`NoResources`), the hold lacks the room
/// (`CargoFull`), `amount` is not positive or the fleet is under
/// standing orders (`InvalidCommand`).
///
/// At the player's outpost the same order takes from the outpost's
/// store instead. Nothing there unloads by itself, so the fleet is not
/// marked `laden`.
class LoadCommand extends Command {
  final int fleetId;
  final ResourceKind resource;
  final double? amount;

  const LoadCommand({required this.fleetId, required this.resource, this.amount});

  factory LoadCommand.fromJson(Object? json) {
    final m = _obj(json);
    return LoadCommand(
      fleetId: _int(m['fleet_id']),
      resource: ResourceKind.fromJson(m['resource']),
      amount: _opt(m['amount'], _double),
    );
  }

  @override
  Json toJson() => {'action': 'load', 'fleet_id': fleetId, 'resource': resource.toJson(), 'amount': ?amount};
}

/// End what `load` began: the hold of `fleet_id`, docked at home, goes
/// back to the stockpile as far as the caps allow, and the rest follows
/// as room opens, as for any docked fleet. The answer is an `Alert`
/// (info) event. Refused when the fleet is elsewhere (`NotInSector`),
/// busy (`OnCooldown`) or was not loaded (`InvalidCommand`).
///
/// At the player's outpost it puts the whole hold of any fleet into the
/// outpost's store as far as the store has room, and is never refused
/// for a hold that was not loaded by hand.
class UnloadCommand extends Command {
  final int fleetId;

  const UnloadCommand({required this.fleetId});

  factory UnloadCommand.fromJson(Object? json) {
    final m = _obj(json);
    return UnloadCommand(fleetId: _int(m['fleet_id']));
  }

  @override
  Json toJson() => {'action': 'unload', 'fleet_id': fleetId};
}

/// Turn deuterium in the hold of `fleet_id` into fuel, until the tank is
/// full or the deuterium is gone. It is done at once, anywhere, and at
/// a loss: a unit of deuterium fills `REFINE_LOSS` less of the tank than
/// it does at a dock. `FleetState.refine` says beforehand what the order
/// would turn into what. Deuterium loaded at a dock counts as any other.
/// The answer is an `Alert` (info) event with both figures. Refused,
/// with nothing changed, while the fleet is in combat or charging a
/// jump (`OnCooldown`), when its hold carries no deuterium
/// (`NoResources`) or its tank is full (`InvalidCommand`).
class RefineCommand extends Command {
  final int fleetId;

  const RefineCommand({required this.fleetId});

  factory RefineCommand.fromJson(Object? json) {
    final m = _obj(json);
    return RefineCommand(fleetId: _int(m['fleet_id']));
  }

  @override
  Json toJson() => {'action': 'refine', 'fleet_id': fleetId};
}

/// Found the player's outpost in the sector `fleet_id` stands in, paid
/// from its hold (`OutpostSurvey.cost`). The fleet must stand idle
/// there and fly no standing orders; the sector must lie in rings 9 to
/// 20, be no Anomaly, hold no derelict and no group, and lie at least
/// 3 hexes from every other outpost; a player has one outpost. The
/// answer is an `Alert` (info) event with the tick the first level is
/// finished. Refused, with nothing changed and the sentence
/// `outpost_survey` gives as `refusal`: `InvalidTarget` for a sector
/// that does not qualify, `InvalidCommand` with an outpost already or
/// under standing orders, `NoResources` naming what the hold lacks,
/// `OnCooldown` for a fleet that is moving, fighting or busy.
class FoundOutpostCommand extends Command {
  final int fleetId;

  const FoundOutpostCommand({required this.fleetId});

  factory FoundOutpostCommand.fromJson(Object? json) {
    final m = _obj(json);
    return FoundOutpostCommand(fleetId: _int(m['fleet_id']));
  }

  @override
  Json toJson() => {'action': 'found_outpost', 'fleet_id': fleetId};
}

/// Ask what an outpost in `sector` would be worth, without founding:
/// the sector `fleet_id` stands in when none is named, otherwise any
/// sector on the player's chart (`InvalidTarget` for one never seen).
/// The reply is an `outpost_survey` message.
class OutpostSurveyCommand extends Command {
  final int fleetId;
  final Hex? sector;

  const OutpostSurveyCommand({required this.fleetId, this.sector});

  factory OutpostSurveyCommand.fromJson(Object? json) {
    final m = _obj(json);
    return OutpostSurveyCommand(fleetId: _int(m['fleet_id']), sector: _opt(m['sector'], Hex.fromJson));
  }

  @override
  Json toJson() => {'action': 'outpost_survey', 'fleet_id': fleetId, 'sector': ?sector?.toJson()};
}

/// Build the next level of the player's outpost, paid in full from the
/// outpost's own store at that moment (`OutpostState.next_level`). The
/// answer is an `Alert` (info) event. Refused with
/// `InsufficientResources` and the exact shortfall when the store does
/// not cover it, `MaxLevelReached` at the last level, `InvalidCommand`
/// with no outpost, while it builds or repairs, or while it is breached.
class UpgradeOutpostCommand extends Command {
  const UpgradeOutpostCommand();

  @override
  Json toJson() => {'action': 'upgrade_outpost'};
}

/// Give up the player's outpost. Its store drifts as wreckage in the
/// sector, nothing is refunded and another may be founded. The answer
/// is an `Alert` (info) event. `InvalidCommand` with no outpost.
class AbandonOutpostCommand extends Command {
  const AbandonOutpostCommand();

  @override
  Json toJson() => {'action': 'abandon_outpost'};
}

/// Which resource a harvest order mines. `Auto` takes everything the sector
/// has, metal then crystal then deuterium, until the hold is full; a specific
/// resource mines only that one and is rejected with
/// `ErrorCode::ResourceNotPresent` if the sector has none of it.
enum HarvestResource {
  metal('Metal'),
  crystal('Crystal'),
  deuterium('Deuterium'),
  auto('Auto');

  const HarvestResource(this.wire);

  final String wire;

  static HarvestResource fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// Standing orders for a fleet: assign a doctrine, the server flies it.
/// `preset: "manual"` clears the policy.
class PolicyUpdate extends ClientMessage {
  final int fleetId;
  final PolicyPreset preset;
  final PolicyParams? params;

  const PolicyUpdate({required this.fleetId, required this.preset, this.params});

  factory PolicyUpdate.fromJson(Object? json) {
    final m = _obj(json);
    return PolicyUpdate(
      fleetId: _int(m['fleet_id']),
      preset: PolicyPreset.fromJson(m['preset']),
      params: _opt(m['params'], PolicyParams.fromJson),
    );
  }

  @override
  Json toJson() => {
    'type': 'policy_update',
    'fleet_id': fleetId,
    'preset': preset.toJson(),
    'params': ?params?.toJson(),
  };
}

enum PolicyPreset {
  manual('manual', label: 'manual'),
  prospect('prospect', label: 'prospect'),
  mineAndReturn('mine_and_return', label: 'mine+return'),
  salvageAndSites('salvage_and_sites', label: 'salvage+sites'),
  patrolHome('patrol_home', label: 'patrol home');

  const PolicyPreset(this.wire, {required this.label});

  final String wire;
  final String label;

  static PolicyPreset fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// Doctrine thresholds. Every field is optional on the wire; an omitted field
/// (or an omitted `params` object) takes the default in constants.rs.
class PolicyParams {
  /// Spare fuel, as a percent of the tank, kept on top of the exact fuel
  /// cost of the route home. The autopilot heads home when fuel falls below
  /// route cost plus this reserve, and never takes a hop outward that would
  /// leave it unable to return. Clamped to 0..=90. Default 10.
  final int minFuelPct;

  /// Head home when cargo fills past this percent. Clamped to 10..=100.
  /// Default 85.
  final int cargoReturnPct;

  /// Maximum distance in hexes from the HOMEWORLD that the autopilot will
  /// claim, prospect, salvage or hunt at; never measured from the last
  /// claim. Clamped to 1..=30. Default 4.
  final int maxRange;

  /// Engage hostiles only if the fleet's odds against them (`SectorOdds`
  /// `ratio`) are at least this / 10. It is also exactly the least ratio,
  /// against what the player knows of a sector, at which any doctrine will
  /// enter it, hostiles there or not: the setting is honoured as given,
  /// including a deliberate value below EVEN (1.1). A refused sector is
  /// reported as a `PolicyAction` hold with the reason; a standing hold is
  /// repeated at most once a minute and again only if its cause changes.
  /// Clamped to 1..=100. Default 15 (1.5x, FAVOURABLE).
  final int engageRatioX10;

  /// The share of the homeworld's deuterium income, in percent, that this
  /// fleet may spend on fuel while it flies under these orders. The fleet
  /// earns an allowance at that share of the current deuterium production
  /// and can save up the price of one full tank; it starts empty, so an
  /// empire with no deuterium income yet keeps its stock for its queues.
  /// At the dock its tank is filled in one go once the allowance and the stockpile cover what is missing, and
  /// until then the fleet waits there and says so (a `PolicyAction` hold
  /// with the time left); it never leaves on standing orders with a
  /// part-filled tank. While the deuterium store is full the fill is free
  /// of the allowance: that income would overflow anyway. 100 lifts the
  /// bound (the tank takes whatever the stockpile holds, ahead of every
  /// queue, as a fleet flown by hand does). Clamped to 0..=100. Default 20.
  final int fuelBudgetPct;

  /// Refine before turning home for fuel. When the fleet's fuel no longer
  /// covers the way home and the reserve, or the next jump outward would
  /// leave too little for them, it turns the deuterium in its hold into
  /// fuel first, as a `refine` order does and at the same loss, reports it
  /// as a `PolicyAction` `refine`, and is weighed again; it turns home
  /// only if that was not enough. What it refines is not charged to
  /// `fuel_budget_pct`. Default false.
  final bool refine;

  const PolicyParams({
    this.minFuelPct = 10,
    this.cargoReturnPct = 85,
    this.maxRange = 4,
    this.engageRatioX10 = 15,
    this.fuelBudgetPct = 20,
    this.refine = false,
  });

  factory PolicyParams.fromJson(Object? json) {
    final m = _obj(json);
    return PolicyParams(
      minFuelPct: _opt(m['min_fuel_pct'], _int) ?? 10,
      cargoReturnPct: _opt(m['cargo_return_pct'], _int) ?? 85,
      maxRange: _opt(m['max_range'], _int) ?? 4,
      engageRatioX10: _opt(m['engage_ratio_x10'], _int) ?? 15,
      fuelBudgetPct: _opt(m['fuel_budget_pct'], _int) ?? 20,
      refine: _opt(m['refine'], _bool) ?? false,
    );
  }

  Json toJson() => {
    'min_fuel_pct': minFuelPct,
    'cargo_return_pct': cargoReturnPct,
    'max_range': maxRange,
    'engage_ratio_x10': engageRatioX10,
    'fuel_budget_pct': fuelBudgetPct,
    'refine': refine,
  };

  List<Object?> get _props => [minFuelPct, cargoReturnPct, maxRange, engageRatioX10, fuelBudgetPct, refine];

  @override
  bool operator ==(Object other) => other is PolicyParams && _same(_props, other._props);

  @override
  int get hashCode => Object.hashAll(_props);
}

/// The standing orders a fleet flies under, as `FleetState` carries them:
/// the doctrine and every setting in force, after the server's clamping.
/// The doctrine is never `manual`; a fleet flown by hand has no orders.
class StandingOrders {
  final PolicyPreset preset;
  final PolicyParams params;

  const StandingOrders({required this.preset, required this.params});

  factory StandingOrders.fromJson(Object? json) {
    final m = _obj(json);
    return StandingOrders(preset: PolicyPreset.fromJson(m['preset']), params: PolicyParams.fromJson(m['params']));
  }

  Json toJson() => {'preset': preset.toJson(), 'params': params.toJson()};

  List<Object?> get _props => [preset, params];

  @override
  bool operator ==(Object other) => other is StandingOrders && _same(_props, other._props);

  @override
  int get hashCode => Object.hashAll(_props);
}

enum QueueType {
  building('Building'),
  ship('Ship'),
  research('Research');

  const QueueType(this.wire);

  final String wire;

  static QueueType fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

sealed class ServerMessage {
  const ServerMessage();

  static ServerMessage fromJson(Object? json) {
    return switch (_obj(json)['type']) {
      'auth_result' => AuthResult.fromJson(json),
      'tick_update' => TickUpdate.fromJson(json),
      'full_state' => GameState.fromJson(json),
      'event' => EventMessage.fromJson(json),
      'error' => ErrorMessage.fromJson(json),
      'preview_move' => MovePreview.fromJson(json),
      'odds' => FleetOdds.fromJson(json),
      'leaderboard' => LeaderboardReply.fromJson(json),
      'exchange_quote' => ExchangeQuote.fromJson(json),
      'outpost_survey' => OutpostSurvey.fromJson(json),
      final tag => throw FormatException('unknown ServerMessage type "$tag"'),
    };
  }

  static ServerMessage decode(String raw) => fromJson(jsonDecode(raw));

  Json toJson();

  String encode() => jsonEncode(toJson());
}

class EventMessage extends ServerMessage {
  final GameEvent event;

  const EventMessage(this.event);

  factory EventMessage.fromJson(Object? json) => EventMessage(GameEvent.fromJson(json));

  @override
  Json toJson() => {'type': 'event', ...event.toJson()};
}

/// Answer to `outpost_survey`: what an outpost in `sector` would cost, make
/// and face, worked out by the server. `ring`, `cost`, `build_ticks`,
/// `raid_power` and `platform_power` follow from where the sector lies and
/// are told of any sector. `site` is what only eyes on the sector tell.
/// `refusal` depends on the asking fleet and on the moment.
class OutpostSurvey extends ServerMessage {
  final int fleetId;
  final Hex sector;

  /// Hexes from the centre of the map.
  final int ring;

  /// The site itself. Absent for a sector the player has never seen:
  /// `refusal` then says so, and says nothing of what is there.
  final SurveySite? site;

  /// What founding takes from the hold: level 1.
  final Resources cost;

  /// Ticks level 1 takes to build at this world's pace.
  final int buildTicks;

  /// The least and the most a raid on an outpost in this ring can be.
  final PowerRange raidPower;

  /// What the platform defends with at level 1.
  final double platformPower;

  /// Why `found_outpost` would be refused right now; absent when the
  /// fleet could found here this tick.
  final String? refusal;

  const OutpostSurvey({
    required this.fleetId,
    required this.sector,
    required this.ring,
    this.site,
    required this.cost,
    required this.buildTicks,
    required this.raidPower,
    required this.platformPower,
    this.refusal,
  });

  factory OutpostSurvey.fromJson(Object? json) {
    final m = _obj(json);
    return OutpostSurvey(
      fleetId: _int(m['fleet_id']),
      sector: Hex.fromJson(m['sector']),
      ring: _int(m['ring']),
      site: _opt(m['site'], SurveySite.fromJson),
      cost: Resources.fromJson(m['cost']),
      buildTicks: _int(m['build_ticks']),
      raidPower: PowerRange.fromJson(m['raid_power']),
      platformPower: _double(m['platform_power']),
      refusal: _opt(m['refusal'], _string),
    );
  }

  @override
  Json toJson() => {
    'type': 'outpost_survey',
    'fleet_id': fleetId,
    'sector': sector.toJson(),
    'ring': ring,
    'site': ?site?.toJson(),
    'cost': cost.toJson(),
    'build_ticks': buildTicks,
    'raid_power': raidPower.toJson(),
    'platform_power': platformPower,
    'refusal': ?refusal,
  };
}

/// What a survey reads of a sector the player has seen.
class SurveySite {
  /// The ore the sector was made with, which is what the extractor reads:
  /// harvesting the tiles does not lower it.
  final SectorResources densities;

  /// 0 to 12: Sparse 0.5, Moderate 1, Rich 2, Pristine 4, summed over the
  /// three resources.
  final double grade;

  /// What the extractor would add to the store each tick at level 1.
  final Resources production;

  /// Lane hops from the sector to the homeworld; absent beyond `LANE_SEARCH_HOPS`.
  final int? hopsHome;

  const SurveySite({required this.densities, required this.grade, required this.production, this.hopsHome});

  factory SurveySite.fromJson(Object? json) {
    final m = _obj(json);
    return SurveySite(
      densities: SectorResources.fromJson(m['densities']),
      grade: _double(m['grade']),
      production: Resources.fromJson(m['production']),
      hopsHome: _opt(m['hops_home'], _int),
    );
  }

  Json toJson() => {
    'densities': densities.toJson(),
    'grade': grade,
    'production': production.toJson(),
    'hops_home': ?hopsHome,
  };
}

/// The bounds of a roll of combat power, on the scale `FleetState.power`
/// and threat ratings share: no roll is below `least` or above `most`.
class PowerRange {
  final double least;
  final double most;

  const PowerRange({required this.least, required this.most});

  factory PowerRange.fromJson(Object? json) {
    final m = _obj(json);
    return PowerRange(least: _double(m['least']), most: _double(m['most']));
  }

  Json toJson() => {'least': least, 'most': most};
}

/// One trade at the hub's exchange, quoted or made: `amount` of `give` for
/// `receive` of `want`.
///
/// The exchange trades resource for resource and always keeps a fee, so a
/// trade never raises what the player owns. Par values are the score
/// weights: metal 1, crystal 1.5, deuterium 2 a unit. Each player has a
/// meter that counts the weight they sold (`ExchangeMeter`). The fee on a
/// unit of weight is 25 percent of par while the meter is empty and rises in
/// a straight line to 75 percent when it reads `span`, and stays there
/// beyond. A trade pays the fee of every unit it moves the meter through,
/// so one large order and the same order in pieces return the same;
/// `receive` is rounded down to whole units. The meter falls by one `span`
/// every `window_ticks`, whether or not the player is connected.
class ExchangeTerms {
  final int fleetId;
  final ResourceKind give;
  final ResourceKind want;

  /// Units of `give` handed over.
  final double amount;

  /// Units of `want` that come back: whole units, rounded down.
  final double receive;

  /// Units of `want` the same `amount` is worth at par, with no fee.
  final double par;

  /// The share of par the exchange keeps on this trade, `1 - receive / par`.
  final double fee;

  /// The player's meter before the trade.
  final ExchangeMeter meter;

  /// And after it.
  final ExchangeMeter meterAfter;

  const ExchangeTerms({
    required this.fleetId,
    required this.give,
    required this.want,
    required this.amount,
    required this.receive,
    required this.par,
    required this.fee,
    required this.meter,
    required this.meterAfter,
  });

  factory ExchangeTerms.fromJson(Object? json) {
    final m = _obj(json);
    return ExchangeTerms(
      fleetId: _int(m['fleet_id']),
      give: ResourceKind.fromJson(m['give']),
      want: ResourceKind.fromJson(m['want']),
      amount: _double(m['amount']),
      receive: _double(m['receive']),
      par: _double(m['par']),
      fee: _double(m['fee']),
      meter: ExchangeMeter.fromJson(m['meter']),
      meterAfter: ExchangeMeter.fromJson(m['meter_after']),
    );
  }

  Json toJson() => {
    'fleet_id': fleetId,
    'give': give.toJson(),
    'want': want.toJson(),
    'amount': amount,
    'receive': receive,
    'par': par,
    'fee': fee,
    'meter': meter.toJson(),
    'meter_after': meterAfter.toJson(),
  };
}

/// A player's standing at the exchange.
class ExchangeMeter {
  /// Weight sold and not yet forgotten.
  final double pressure;

  /// The pressure at which the fee stops rising: 15 percent of what the
  /// player's own mines make in one window, in weight units. It grows
  /// with the mines; 0 for an empire with no mine output, which pays the
  /// top fee.
  final double span;

  /// The fee on the next unit of weight sold, 0.25 to 0.75.
  final double fee;

  /// The meter forgets one `span` every this many ticks.
  final int windowTicks;

  /// Ticks until the meter is empty again; 0 when it is, absent when it
  /// does not fall (`span` is 0).
  final int? clearIn;

  const ExchangeMeter({this.pressure = 0, this.span = 0, this.fee = 0, this.windowTicks = 0, this.clearIn});

  factory ExchangeMeter.fromJson(Object? json) {
    final m = _obj(json);
    return ExchangeMeter(
      pressure: _double(m['pressure']),
      span: _double(m['span']),
      fee: _double(m['fee']),
      windowTicks: _int(m['window_ticks']),
      clearIn: _opt(m['clear_in'], _int),
    );
  }

  Json toJson() => {'pressure': pressure, 'span': span, 'fee': fee, 'window_ticks': windowTicks, 'clear_in': ?clearIn};
}

/// Answer to `exchange_quote`.
class ExchangeQuote extends ServerMessage {
  final ExchangeTerms terms;

  /// Why `exchange` with the same fields would be refused right now;
  /// absent when it would go through.
  final String? refusal;

  const ExchangeQuote({required this.terms, this.refusal});

  factory ExchangeQuote.fromJson(Object? json) {
    final m = _obj(json);
    return ExchangeQuote(terms: ExchangeTerms.fromJson(json), refusal: _opt(m['refusal'], _string));
  }

  @override
  Json toJson() => {'type': 'exchange_quote', ...terms.toJson(), 'refusal': ?refusal};
}

/// The score table: one ranking for every player, human or agent.
/// `score = core + min(combat + explore, 0.25 * core)`, where `core` counts
/// completed building and research levels, living ships and half of the
/// defence structures, each at its cost weighted 1 : 1.5 : 2 for
/// metal : crystal : deuterium, per thousand.
///
/// Built-in bots are part of the world but not of the ranking: they are
/// never in `entries` and take no rank there. They are listed in `bots`,
/// ranked among themselves. A bot's account name always starts with
/// `BOT_NAME_PREFIX`, which no other account can register, so every place
/// that names a player (fleets in a sector, combat events) tags a bot by
/// its name alone.
class LeaderboardReply extends ServerMessage {
  final int tick;
  final List<LeaderboardEntry> entries;

  /// The asking player's own row, when it falls outside `entries`.
  final LeaderboardEntry? you;

  /// The world's built-in bots, best first; absent when it has none.
  final List<LeaderboardEntry> bots;

  const LeaderboardReply({required this.tick, required this.entries, this.you, this.bots = const []});

  factory LeaderboardReply.fromJson(Object? json) {
    final m = _obj(json);
    return LeaderboardReply(
      tick: _int(m['tick']),
      entries: _list(m['entries'], LeaderboardEntry.fromJson),
      you: _opt(m['you'], LeaderboardEntry.fromJson),
      bots: _opt(m['bots'], (v) => _list(v, LeaderboardEntry.fromJson)) ?? const [],
    );
  }

  @override
  Json toJson() => {
    'type': 'leaderboard',
    'tick': tick,
    'entries': entries.map((e) => e.toJson()).toList(),
    'you': ?you?.toJson(),
    if (bots.isNotEmpty) 'bots': bots.map((e) => e.toJson()).toList(),
  };
}

class LeaderboardEntry {
  final int rank;
  final String name;
  final bool agent;
  final double score;
  final double core;

  /// Raw combat points, before the 25 percent cap.
  final double combat;

  /// Raw exploration points, before the 25 percent cap.
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
      rank: _int(m['rank']),
      name: _string(m['name']),
      agent: _bool(m['agent']),
      score: _double(m['score']),
      core: _double(m['core']),
      combat: _double(m['combat']),
      explore: _double(m['explore']),
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

/// Answer to `preview_move`: what a jump costs and what waits at the far end.
///
/// `threat` is what the player knows of the target sector (observed if it is
/// live, the remembered rating if charted, else the distance estimate).
/// `ratio` and `label` are the odds of the fleet named by `fleet_id` (not the
/// strongest one) against it, as in `SectorOdds`.
class MovePreview extends ServerMessage {
  final int fleetId;
  final Hex target;

  /// Fuel the jump burns.
  final double fuelCost;

  /// Fuel left afterwards (0 when the fleet cannot afford the jump).
  final double fuelAfter;

  /// The fleet holds enough fuel for the jump.
  final bool canJump;

  /// Hops from `target` back to the homeworld by the shortest real route,
  /// charted or not: what the fleet can actually fly.
  final int hopsHome;

  /// Hops of the shortest route over sectors the player has entered; None
  /// when no such route exists. Never shorter than `hops_home`.
  final int? chartedHopsHome;

  /// The `hops_home` route crosses a sector the player has never entered.
  final bool routeUnexplored;

  /// Fuel the `hops_home` route costs.
  final double fuelToReturn;

  /// After the jump the fleet still holds enough fuel to get home.
  final bool canReturn;
  final ThreatInfo threat;

  /// Combat power of the whole fleet, on the scale of `threat.est_power`.
  /// The two do not divide into `ratio`: that weighs the ship classes.
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
    this.chartedHopsHome,
    this.routeUnexplored = false,
    required this.fuelToReturn,
    required this.canReturn,
    required this.threat,
    required this.fleetPower,
    required this.ratio,
    required this.label,
  });

  factory MovePreview.fromJson(Object? json) {
    final m = _obj(json);
    return MovePreview(
      fleetId: _int(m['fleet_id']),
      target: Hex.fromJson(m['target']),
      fuelCost: _double(m['fuel_cost']),
      fuelAfter: _double(m['fuel_after']),
      canJump: _bool(m['can_jump']),
      hopsHome: _int(m['hops_home']),
      chartedHopsHome: _opt(m['charted_hops_home'], _int),
      routeUnexplored: _opt(m['route_unexplored'], _bool) ?? false,
      fuelToReturn: _double(m['fuel_to_return']),
      canReturn: _bool(m['can_return']),
      threat: ThreatInfo.fromJson(m['threat']),
      fleetPower: _double(m['fleet_power']),
      ratio: _double(m['ratio']),
      label: RatioLabel.fromJson(m['label']),
    );
  }

  @override
  Json toJson() => {
    'type': 'preview_move',
    'fleet_id': fleetId,
    'target': target.toJson(),
    'fuel_cost': fuelCost,
    'fuel_after': fuelAfter,
    'can_jump': canJump,
    'hops_home': hopsHome,
    'charted_hops_home': chartedHopsHome,
    'route_unexplored': routeUnexplored,
    'fuel_to_return': fuelToReturn,
    'can_return': canReturn,
    'threat': threat.toJson(),
    'fleet_power': fleetPower,
    'ratio': ratio,
    'label': label.toJson(),
  };
}

/// Answer to `odds`, in the order asked.
class FleetOdds extends ServerMessage {
  final int fleetId;
  final List<SectorOdds> sectors;

  const FleetOdds({required this.fleetId, required this.sectors});

  factory FleetOdds.fromJson(Object? json) {
    final m = _obj(json);
    return FleetOdds(fleetId: _int(m['fleet_id']), sectors: _list(m['sectors'], SectorOdds.fromJson));
  }

  @override
  Json toJson() => {'type': 'odds', 'fleet_id': fleetId, 'sectors': sectors.map((e) => e.toJson()).toList()};
}

/// How a fleet would fare against what the player knows of a sector.
///
/// The server plays the fight on averages, with the ships on each side:
/// rapid fire between classes, each shot against hull and shield, the fleet
/// firing first. `ratio` puts the expected result on the scale of a power
/// ratio between equal ship classes (1.0 is an even match, 0.1 to 10), and
/// `label` is its band: at least 2.5 is safe (99 percent of the hull kept or
/// more), 1.5 favourable (86 to 99), 1.1 even (a win that keeps 53 to 86),
/// 0.9 risky (anything from a loss to a win with half the hull), below that
/// deadly. A sector with nothing in sight is rated against the group its
/// ring spawns. Standing orders use the same number (`engage_ratio_x10`).
class SectorOdds {
  final Hex sector;
  final ThreatInfo threat;
  final double ratio;
  final RatioLabel label;

  const SectorOdds({required this.sector, required this.threat, required this.ratio, required this.label});

  factory SectorOdds.fromJson(Object? json) {
    final m = _obj(json);
    return SectorOdds(
      sector: Hex.fromJson(m['sector']),
      threat: ThreatInfo.fromJson(m['threat']),
      ratio: _double(m['ratio']),
      label: RatioLabel.fromJson(m['label']),
    );
  }

  Json toJson() => {'sector': sector.toJson(), 'threat': threat.toJson(), 'ratio': ratio, 'label': label.toJson()};
}

/// How dangerous a sector is: T1 to T9 from the power of what is there.
class ThreatInfo {
  /// 1 to 9, or 0 where there is nothing to fight and nothing will come:
  /// a sector an outpost stands in.
  final int rating;
  final double estPower;
  final ThreatBasis basis;

  const ThreatInfo({required this.rating, required this.estPower, required this.basis});

  factory ThreatInfo.fromJson(Object? json) {
    final m = _obj(json);
    return ThreatInfo(
      rating: _int(m['rating']),
      estPower: _double(m['est_power']),
      basis: ThreatBasis.fromJson(m['basis']),
    );
  }

  Json toJson() => {'rating': rating, 'est_power': estPower, 'basis': basis.toJson()};
}

/// Where a `ThreatInfo` comes from.
enum ThreatBasis {
  /// A hostile group was seen; the power is the group's own (Aggressive and
  /// Swarm groups count 25 percent more). Or an outpost was seen, which
  /// keeps every group out: rating 0, power 0.
  observed('observed'),

  /// The sector's seed holds a group that is away or cleared; the power is
  /// the ring's.
  template('template'),

  /// Distance only: the ring's group power.
  estimate('estimate');

  const ThreatBasis(this.wire);

  final String wire;

  static ThreatBasis fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// Reply to `AuthRequest`. On success `token` is set only when the server just
/// issued one (new account or claimed legacy account): the client must store
/// it. On failure `code` says why (`InvalidName`, `TokenRequired`,
/// `InvalidToken`, or `WorldFull` when a new name finds every home seat
/// taken) and `message` is readable text.
class AuthResult extends ServerMessage {
  final bool success;
  final int? playerId;
  final String? token;
  final ErrorCode? code;
  final String? message;

  const AuthResult({required this.success, this.playerId, this.token, this.code, this.message});

  factory AuthResult.fromJson(Object? json) {
    final m = _obj(json);
    return AuthResult(
      success: _bool(m['success']),
      playerId: _opt(m['player_id'], _int),
      token: _opt(m['token'], _string),
      code: _opt(m['code'], ErrorCode.fromJson),
      message: _opt(m['message'], _string),
    );
  }

  @override
  Json toJson() => {
    'type': 'auth_result',
    'success': success,
    'player_id': ?playerId,
    'token': ?token,
    'code': ?code?.toJson(),
    'message': ?message,
  };
}

/// Per-tick update. `player`, `homeworld_update`, `sector_updates` and
/// `events` are optional and omitted when there is nothing to say.
/// `sector_updates` holds every live sector (current this tick) plus, once,
/// each sector that stopped being live this tick (`live: false`, so the
/// client knows to dim it). Stale sectors are not repeated after that. `fleets`
/// is not optional: it is the complete, authoritative list of the player's
/// fleets, sent whole every tick (like `GameState.fleets`), so a client
/// replaces its list wholesale. A fleet missing from the list no longer
/// exists, and an empty list means every fleet is lost.
class TickUpdate extends ServerMessage {
  final int tick;
  final PlayerState? player;
  final List<FleetState> fleets;
  final List<SectorState>? sectorUpdates;
  final HomeworldState? homeworldUpdate;

  /// The player's outpost, whole, every tick they have one. Absent means
  /// they have none: a client drops the one it held.
  final OutpostState? outpost;
  final List<GameEvent>? events;

  const TickUpdate({
    required this.tick,
    this.player,
    required this.fleets,
    this.sectorUpdates,
    this.homeworldUpdate,
    this.outpost,
    this.events,
  });

  factory TickUpdate.fromJson(Object? json) {
    final m = _obj(json);
    return TickUpdate(
      tick: _int(m['tick']),
      player: _opt(m['player'], PlayerState.fromJson),
      fleets: _list(m['fleets'], FleetState.fromJson),
      sectorUpdates: _opt(m['sector_updates'], (v) => _list(v, SectorState.fromJson)),
      homeworldUpdate: _opt(m['homeworld_update'], HomeworldState.fromJson),
      outpost: _opt(m['outpost'], OutpostState.fromJson),
      events: _opt(m['events'], (v) => _list(v, GameEvent.fromJson)),
    );
  }

  @override
  Json toJson() => {
    'type': 'tick_update',
    'tick': tick,
    'player': ?player?.toJson(),
    'fleets': fleets.map((e) => e.toJson()).toList(),
    'sector_updates': ?sectorUpdates?.map((e) => e.toJson()).toList(),
    'homeworld_update': ?homeworldUpdate?.toJson(),
    'outpost': ?outpost?.toJson(),
    'events': ?events?.map((e) => e.toJson()).toList(),
  };
}

/// Full snapshot, sent on login and on `request_full_state`.
///
/// `known_sectors` is the player's whole chart: every sector the player has
/// ever had eyes on (fleet present, sensor array, active scan), not only the
/// ones with fresh intel. Entries with `live: true` are current; the rest
/// carry what was last observed and the tick it was observed (`last_seen`).
class GameState extends ServerMessage {
  final int tick;
  final PlayerState player;
  final List<FleetState> fleets;
  final HomeworldState homeworld;

  /// The player's outpost; absent when they have none.
  final OutpostState? outpost;
  final List<SectorState> knownSectors;
  final WorldInfo world;

  const GameState({
    required this.tick,
    required this.player,
    required this.fleets,
    required this.homeworld,
    this.outpost,
    required this.knownSectors,
    required this.world,
  });

  factory GameState.fromJson(Object? json) {
    final m = _obj(json);
    return GameState(
      tick: _int(m['tick']),
      player: PlayerState.fromJson(m['player']),
      fleets: _list(m['fleets'], FleetState.fromJson),
      homeworld: HomeworldState.fromJson(m['homeworld']),
      outpost: _opt(m['outpost'], OutpostState.fromJson),
      knownSectors: _list(m['known_sectors'], SectorState.fromJson),
      world: WorldInfo.fromJson(m['world']),
    );
  }

  @override
  Json toJson() => {
    'type': 'full_state',
    'tick': tick,
    'player': player.toJson(),
    'fleets': fleets.map((e) => e.toJson()).toList(),
    'homeworld': homeworld.toJson(),
    'outpost': ?outpost?.toJson(),
    'known_sectors': knownSectors.map((e) => e.toJson()).toList(),
    'world': world.toJson(),
  };
}

/// A player's outpost: a station on one sector of rings 9 to 20, built from
/// what fleets carry to it. One level (1 to `max_level`) sets its store, its
/// platform and how many site rates its extractor makes.
///
/// It is a dock. `load` and `unload` there move cargo between a hold and
/// `store`, never the stockpile, and nothing moves by itself. A fleet
/// standing there without standing orders is refuelled from the store's
/// deuterium while the outpost is `in_service`; any fleet of the owner
/// arriving at a finished outpost hands over its charts; a fleet standing
/// at a finished outpost does not count against the deployed fleets.
/// Nothing in the store scores or can be spent at home.
///
/// Pirates of its ring raid it on the homeworld's raid clock
/// (`RaidIncoming` and `RaidResolved` carry `outpost`). A lost raid takes a
/// quarter of what lies above `protected` in the store and in the holds of
/// fleets standing there, destroys a quarter of those ships, and breaches
/// the outpost. No raid destroys it, and a fleet standing there is not safe.
class OutpostState {
  final Hex location;

  /// 0 while the first level is being built.
  final int level;
  final int maxLevel;

  /// Level 1 or more and not breached: the extractor runs, tanks are
  /// filled and the platform defends.
  final bool inService;

  /// A lost raid broke it: no extractor, no refuelling, no platform until
  /// the repair is done. `store`, `load`, `unload` and chart banking work.
  final bool breached;

  /// The one job under way: a level being built, or the repair.
  final OutpostWorkState? work;
  final Resources store;

  /// The most the store holds of each resource. The extractor stops
  /// there, and `unload` leaves aboard what does not fit.
  final Resources storeCap;

  /// What a raid cannot take: what the repair of the level it stands at,
  /// or of the level it is building, would take from the store as it
  /// stands. A repair is priced in metal; where the store lacks metal,
  /// crystal pays for it and then deuterium, each unit at its par value
  /// (1.5 and 2). A raid counts the holds of fleets standing there with
  /// the store and reads the floor off both.
  final Resources protected;

  /// What the extractor adds to the store each tick now; zero while not
  /// `in_service`.
  final Resources production;

  /// The resources the extractor makes and the store holds all it takes
  /// of: that yield is lost until a fleet loads some.
  final List<ResourceKind> yieldLost;

  /// The site's grade, 0 to 12; see `SurveySite.grade`.
  final double grade;

  /// What the platform defends with; 0 while breached or unfinished.
  final double platformPower;

  /// The platform and the owner's ships standing in the sector: what the
  /// next raid meets.
  final double defencePower;

  /// The least and the most a raid on this outpost can be. The outcome
  /// is `defence_power`, taken a tenth higher or lower by chance, against
  /// the raid's power.
  final PowerRange raidPower;

  /// Tick an announced raid arrives; absent when none is on its way.
  final int? raidArrivalTick;

  /// What `upgrade_outpost` would build; absent at `max_level`.
  final OutpostLevel? nextLevel;

  /// Why `upgrade_outpost` would be refused right now, in the sentence
  /// the refusal would carry; absent when it would be accepted this tick.
  final String? upgradeRefusal;

  /// What the repair will take from the store, paid as `protected` says;
  /// what the whole store does not cover is counted as metal. Present
  /// while breached with no repair under way.
  final Resources? repairCost;

  /// What the store lacks of `repair_cost`, in metal: the repair waits
  /// for a fleet to unload it, or crystal or deuterium to the same par
  /// value. Absent when no repair is waiting.
  final Resources? repairWaitingFor;

  /// Lane hops to the homeworld; absent beyond `LANE_SEARCH_HOPS`.
  final int? hopsHome;

  const OutpostState({
    required this.location,
    required this.level,
    required this.maxLevel,
    required this.inService,
    required this.breached,
    this.work,
    required this.store,
    required this.storeCap,
    required this.protected,
    required this.production,
    this.yieldLost = const [],
    required this.grade,
    required this.platformPower,
    required this.defencePower,
    required this.raidPower,
    this.raidArrivalTick,
    this.nextLevel,
    this.upgradeRefusal,
    this.repairCost,
    this.repairWaitingFor,
    this.hopsHome,
  });

  factory OutpostState.fromJson(Object? json) {
    final m = _obj(json);
    return OutpostState(
      location: Hex.fromJson(m['location']),
      level: _int(m['level']),
      maxLevel: _int(m['max_level']),
      inService: _bool(m['in_service']),
      breached: _bool(m['breached']),
      work: _opt(m['work'], OutpostWorkState.fromJson),
      store: Resources.fromJson(m['store']),
      storeCap: Resources.fromJson(m['store_cap']),
      protected: Resources.fromJson(m['protected']),
      production: Resources.fromJson(m['production']),
      yieldLost: _opt(m['yield_lost'], (v) => _list(v, ResourceKind.fromJson)) ?? const [],
      grade: _double(m['grade']),
      platformPower: _double(m['platform_power']),
      defencePower: _double(m['defence_power']),
      raidPower: PowerRange.fromJson(m['raid_power']),
      raidArrivalTick: _opt(m['raid_arrival_tick'], _int),
      nextLevel: _opt(m['next_level'], OutpostLevel.fromJson),
      upgradeRefusal: _opt(m['upgrade_refusal'], _string),
      repairCost: _opt(m['repair_cost'], Resources.fromJson),
      repairWaitingFor: _opt(m['repair_waiting_for'], Resources.fromJson),
      hopsHome: _opt(m['hops_home'], _int),
    );
  }

  Json toJson() => {
    'location': location.toJson(),
    'level': level,
    'max_level': maxLevel,
    'in_service': inService,
    'breached': breached,
    'work': ?work?.toJson(),
    'store': store.toJson(),
    'store_cap': storeCap.toJson(),
    'protected': protected.toJson(),
    'production': production.toJson(),
    'yield_lost': yieldLost.map((e) => e.toJson()).toList(),
    'grade': grade,
    'platform_power': platformPower,
    'defence_power': defencePower,
    'raid_power': raidPower.toJson(),
    'raid_arrival_tick': ?raidArrivalTick,
    'next_level': ?nextLevel?.toJson(),
    'upgrade_refusal': ?upgradeRefusal,
    'repair_cost': ?repairCost?.toJson(),
    'repair_waiting_for': ?repairWaitingFor?.toJson(),
    'hops_home': ?hopsHome,
  };
}

enum OutpostWork {
  build('build'),
  repair('repair');

  const OutpostWork(this.wire);

  final String wire;

  static OutpostWork fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

class OutpostWorkState {
  final OutpostWork kind;

  /// The level being built, or the level being repaired.
  final int level;
  final int startTick;
  final int endTick;

  const OutpostWorkState({required this.kind, required this.level, required this.startTick, required this.endTick});

  factory OutpostWorkState.fromJson(Object? json) {
    final m = _obj(json);
    return OutpostWorkState(
      kind: OutpostWork.fromJson(m['kind']),
      level: _int(m['level']),
      startTick: _int(m['start_tick']),
      endTick: _int(m['end_tick']),
    );
  }

  Json toJson() => {'kind': kind.toJson(), 'level': level, 'start_tick': startTick, 'end_tick': endTick};

  List<Object?> get _props => [kind, level, startTick, endTick];

  @override
  bool operator ==(Object other) => other is OutpostWorkState && _same(_props, other._props);

  @override
  int get hashCode => Object.hashAll(_props);
}

/// One level of an outpost, as an upgrade to it.
class OutpostLevel {
  final int level;
  final Resources cost;
  final int ticks;
  final Resources storeCap;
  final double platformPower;

  /// What the extractor makes each tick at this level, on this site.
  final Resources production;

  const OutpostLevel({
    required this.level,
    required this.cost,
    required this.ticks,
    required this.storeCap,
    required this.platformPower,
    required this.production,
  });

  factory OutpostLevel.fromJson(Object? json) {
    final m = _obj(json);
    return OutpostLevel(
      level: _int(m['level']),
      cost: Resources.fromJson(m['cost']),
      ticks: _int(m['ticks']),
      storeCap: Resources.fromJson(m['store_cap']),
      platformPower: _double(m['platform_power']),
      production: Resources.fromJson(m['production']),
    );
  }

  Json toJson() => {
    'level': level,
    'cost': cost.toJson(),
    'ticks': ticks,
    'store_cap': storeCap.toJson(),
    'platform_power': platformPower,
    'production': production.toJson(),
  };
}

/// An outpost standing in a sector, as anyone with eyes on the sector sees it.
class OutpostBrief {
  /// The empire it belongs to.
  final String owner;

  /// 0 while its first level is being built.
  final int level;
  final bool breached;

  /// It is the receiver's own.
  final bool mine;

  const OutpostBrief({required this.owner, required this.level, required this.breached, this.mine = false});

  factory OutpostBrief.fromJson(Object? json) {
    final m = _obj(json);
    return OutpostBrief(
      owner: _string(m['owner']),
      level: _int(m['level']),
      breached: _bool(m['breached']),
      mine: _opt(m['mine'], _bool) ?? false,
    );
  }

  Json toJson() => {'owner': owner, 'level': level, 'breached': breached, 'mine': mine};

  List<Object?> get _props => [owner, level, breached, mine];

  @override
  bool operator ==(Object other) => other is OutpostBrief && _same(_props, other._props);

  @override
  int get hashCode => Object.hashAll(_props);
}

/// The world's fixed settings, sent with every `full_state`. `pace` scales
/// economy timers (see `docs/design/economy/spec.md` section 1) and never
/// changes for the life of a world.
class WorldInfo {
  final double pace;

  /// Name of the preset the pace equals, if it equals one.
  final String? preset;
  final int tickHz;
  final int economyVersion;
  final int worldgenVersion;

  /// Simulation estimate for a competent player; present for presets.
  final WorldEstimate? estimate;

  /// The neutral post at the centre of the map: every hub sector and what
  /// it offers. A hub sector has no ore, no hostile group and no derelict.
  final List<HubSite> hub;

  const WorldInfo({
    required this.pace,
    this.preset,
    required this.tickHz,
    required this.economyVersion,
    required this.worldgenVersion,
    this.estimate,
    this.hub = const [],
  });

  factory WorldInfo.fromJson(Object? json) {
    final m = _obj(json);
    return WorldInfo(
      pace: _double(m['pace']),
      preset: _opt(m['preset'], _string),
      tickHz: _int(m['tick_hz']),
      economyVersion: _int(m['economy_version']),
      worldgenVersion: _int(m['worldgen_version']),
      estimate: _opt(m['estimate'], WorldEstimate.fromJson),
      hub: _opt(m['hub'], (v) => _list(v, HubSite.fromJson)) ?? const [],
    );
  }

  Json toJson() => {
    'pace': pace,
    'preset': ?preset,
    'tick_hz': tickHz,
    'economy_version': economyVersion,
    'worldgen_version': worldgenVersion,
    'estimate': ?estimate?.toJson(),
    'hub': hub.map((e) => e.toJson()).toList(),
  };
}

/// One sector of the hub.
class HubSite {
  final Hex location;
  final List<HubService> services;

  const HubSite({required this.location, required this.services});

  factory HubSite.fromJson(Object? json) {
    final m = _obj(json);
    return HubSite(location: Hex.fromJson(m['location']), services: _list(m['services'], HubService.fromJson));
  }

  Json toJson() => {'location': location.toJson(), 'services': services.map((e) => e.toJson()).toList()};
}

class WorldEstimate {
  final int firstCruiserS;
  final int endgameS;

  const WorldEstimate({required this.firstCruiserS, required this.endgameS});

  factory WorldEstimate.fromJson(Object? json) {
    final m = _obj(json);
    return WorldEstimate(firstCruiserS: _int(m['first_cruiser_s']), endgameS: _int(m['endgame_s']));
  }

  Json toJson() => {'first_cruiser_s': firstCruiserS, 'endgame_s': endgameS};
}

class PlayerState {
  final int id;
  final String name;
  final Resources resources;
  final Hex homeworld;

  /// Where the player's meter at the hub's exchange stands this tick.
  final ExchangeMeter exchange;

  const PlayerState({
    required this.id,
    required this.name,
    required this.resources,
    required this.homeworld,
    this.exchange = const ExchangeMeter(),
  });

  factory PlayerState.fromJson(Object? json) {
    final m = _obj(json);
    return PlayerState(
      id: _int(m['id']),
      name: _string(m['name']),
      resources: Resources.fromJson(m['resources']),
      homeworld: Hex.fromJson(m['homeworld']),
      exchange: _opt(m['exchange'], ExchangeMeter.fromJson) ?? const ExchangeMeter(),
    );
  }

  Json toJson() => {
    'id': id,
    'name': name,
    'resources': resources.toJson(),
    'homeworld': homeworld.toJson(),
    'exchange': exchange.toJson(),
  };
}

class FleetState {
  final int id;
  final Hex location;
  final FleetStatus state;
  final List<ShipState> ships;
  final Resources cargo;

  /// Hold size: the most `cargo` can total, as the server enforces it.
  final double cargoCapacity;
  final double fuel;
  final double fuelMax;

  /// Fuel one jump costs this fleet right now (it scales with the fleet's
  /// hull mass and fuel research). A fleet with less than this and away
  /// from home is stranded and only regains fuel from the emergency
  /// reserve, one jump per five minutes.
  final double jumpFuel;

  /// Fuel the way home costs: the fewest hops over the real lanes, charted
  /// or not, times `jump_fuel`. That is the way a fleet flies one jump at
  /// a time, and what standing orders budget for. 0 at the homeworld.
  /// Compare with `fuel`. An emergency recall does not fly this way: it
  /// jumps straight home and burns twice `jump_fuel` per hex of straight
  /// distance.
  final double homeFuel;

  /// Fewest hops home over the real lanes, charted or not. `home_fuel` is
  /// the fuel for these. 0 at the homeworld. Counted from where the fleet
  /// is bound: its destination while a jump is in progress, otherwise
  /// where it stands. An emergency recall does not fly these hops.
  final int hopsHome;

  /// Ticks one jump takes this fleet from the order to the arrival: the
  /// charge, which its slowest ship and Navigation set, and the tick it
  /// lands on. A route of `n` jumps flown without a pause takes
  /// `n * jump_ticks`.
  final int jumpTicks;

  /// Ticks before the fleet takes its next order. While it boards a
  /// derelict (`Exploring`) it is what the boarding has left, out of
  /// `EXPLORE_DURATION_TICKS`.
  final int cooldownRemaining;

  /// Combat power of the whole fleet on the scale raids and threat ratings
  /// share: each ship counts `weapon + (hull + shield) / 10`.
  final double power;

  /// How many more hops outward the fleet can fly and still get home:
  /// `(fuel - home_fuel) / (2 * jump_fuel)`, rounded down, 0 when it
  /// cannot. At the homeworld it is the round-trip reach.
  final int rangeHops;

  /// At the homeworld with cargo the stockpile has no room for; it stays
  /// aboard and unloads as space frees up.
  final bool cargoBlocked;

  /// Loaded by `load` and still at home: the dock leaves this hold alone.
  /// It ends with `unload`, with standing orders, or with the fleet's
  /// next jump, so what the fleet brings home unloads as any cargo does.
  final bool laden;

  /// The fleet is in the sector of its owner's outpost, whatever state
  /// the outpost is in: `load` and `unload` use the outpost's store
  /// here, and a raid on it finds the fleet.
  final bool atOutpost;

  /// Fuel the way to the owner's outpost costs: the fewest lane hops
  /// there times `jump_fuel`, 0 at the outpost. Present while that
  /// outpost is in service, which is when it fills a tank.
  final double? outpostFuel;

  /// What a `refine` order would do right now. Absent when it would be
  /// refused.
  final Refining? refine;

  /// Standing orders currently flying this fleet, if any: the doctrine
  /// with its settings, so the two cannot be told apart or disagree.
  final StandingOrders? policy;

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
    required this.hopsHome,
    required this.jumpTicks,
    this.cooldownRemaining = 0,
    this.power = 0,
    this.rangeHops = 0,
    this.cargoBlocked = false,
    this.laden = false,
    this.atOutpost = false,
    this.outpostFuel,
    this.refine,
    this.policy,
  });

  factory FleetState.fromJson(Object? json) {
    final m = _obj(json);
    return FleetState(
      id: _int(m['id']),
      location: Hex.fromJson(m['location']),
      state: FleetStatus.fromJson(m['state']),
      ships: _list(m['ships'], ShipState.fromJson),
      cargo: Resources.fromJson(m['cargo']),
      cargoCapacity: _double(m['cargo_capacity']),
      fuel: _double(m['fuel']),
      fuelMax: _double(m['fuel_max']),
      jumpFuel: _opt(m['jump_fuel'], _double) ?? 0,
      homeFuel: _opt(m['home_fuel'], _double) ?? 0,
      hopsHome: _int(m['hops_home']),
      jumpTicks: _int(m['jump_ticks']),
      cooldownRemaining: _opt(m['cooldown_remaining'], _int) ?? 0,
      power: _opt(m['power'], _double) ?? 0,
      rangeHops: _opt(m['range_hops'], _int) ?? 0,
      cargoBlocked: _opt(m['cargo_blocked'], _bool) ?? false,
      laden: _opt(m['laden'], _bool) ?? false,
      atOutpost: _opt(m['at_outpost'], _bool) ?? false,
      outpostFuel: _opt(m['outpost_fuel'], _double),
      refine: _opt(m['refine'], Refining.fromJson),
      policy: _opt(m['policy'], StandingOrders.fromJson),
    );
  }

  Json toJson() => {
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
    'hops_home': hopsHome,
    'jump_ticks': jumpTicks,
    'cooldown_remaining': cooldownRemaining,
    'power': power,
    'range_hops': rangeHops,
    'cargo_blocked': cargoBlocked,
    'laden': laden,
    'at_outpost': atOutpost,
    'outpost_fuel': ?outpostFuel,
    'refine': ?refine?.toJson(),
    'policy': ?policy?.toJson(),
  };
}

/// One `refine` order: the deuterium that leaves the hold and the fuel the
/// tank gains for it.
class Refining {
  final double deuterium;
  final double fuel;

  const Refining({required this.deuterium, required this.fuel});

  factory Refining.fromJson(Object? json) {
    final m = _obj(json);
    return Refining(deuterium: _double(m['deuterium']), fuel: _double(m['fuel']));
  }

  Json toJson() => {'deuterium': deuterium, 'fuel': fuel};
}

enum FleetStatus {
  idle('Idle'),
  moving('Moving'),
  harvesting('Harvesting'),
  inCombat('InCombat'),
  returning('Returning'),
  docked('Docked'),
  exploring('Exploring');

  const FleetStatus(this.wire);

  final String wire;

  static FleetStatus fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

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
      id: _int(m['id']),
      shipClass: ShipClass.fromJson(m['class']),
      hull: _double(m['hull']),
      hullMax: _double(m['hull_max']),
      shield: _double(m['shield']),
      shieldMax: _double(m['shield_max']),
      weaponPower: _double(m['weapon_power']),
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

/// One sector as the player knows it.
///
/// `live: true` means the server is watching the sector this tick (a fleet
/// is there, the sensor array covers it, or a scan has not expired) and every
/// field is current. `live: false` is chart memory: `resources`, `hostiles`,
/// `salvage` and `site` are what was last observed at tick `last_seen` and
/// may have changed since (a wreck despawns, a hostile moves, a derelict is
/// looted). `terrain` and `connections` never change. `player_fleets` is
/// only ever sent for live sectors; other empires' positions are not
/// remembered.
class SectorState {
  final Hex location;
  final TerrainType terrain;
  final SectorResources resources;
  final List<Hex> connections;

  /// Hostile fleets. `id` is the fleet id to pass to `attack`: it is stable
  /// whether or not the fleet has materialised yet, and it is the same id
  /// that later appears as `enemy_fleet_id` in `CombatStarted`.
  final List<NpcFleetInfo>? hostiles;
  final List<FleetBrief>? playerFleets;

  /// Collectible wreckage pile (what `collect_salvage` would take).
  final Resources? salvage;

  /// Tick the pile despawns if nobody collects it; seconds left are
  /// `salvage_despawn_tick - tick`. A remembered pile whose tick has passed
  /// is removed from the chart.
  final int? salvageDespawnTick;

  /// A boardable derelict hulk drifting in this sector.
  final SiteBrief? site;

  /// The `salvage` or `site` pin is chart memory from `last_seen` that
  /// nothing has confirmed since; it may be gone. Absent (false) on live
  /// sectors and on sectors with no pin. A pin the player saw removed (a
  /// boarding, a collected or despawned pile, a scan that found nothing) is
  /// dropped instead.
  final bool pinsStale;

  /// What the ore tiles hold, in raw units; absent when the sector has none.
  /// When not `live` it is as of `last_seen`.
  final OreReserve? oreReserve;

  /// Where the player's own chart of this sector stands; absent when no
  /// fleet of theirs holds or has delivered one. Always current, also on
  /// a sector that is not `live`: a sector whose mark changes is sent
  /// once more in `sector_updates`.
  final ChartMark? chart;

  /// The outpost standing here, whoever owns it. While it stands no group
  /// lives in the sector or wanders in. When not `live` it is the outpost
  /// last seen, which may have grown, been breached or been abandoned.
  final OutpostBrief? outpost;

  /// How dangerous the sector is. When not `live` it is the rating last
  /// seen, as old as `last_seen`.
  final ThreatInfo threat;

  /// Tick this sector was last observed; the current tick when `live`.
  final int lastSeen;

  /// See the struct docs.
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
    this.pinsStale = false,
    this.oreReserve,
    this.chart,
    this.outpost,
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
      salvageDespawnTick: _opt(m['salvage_despawn_tick'], _int),
      site: _opt(m['site'], SiteBrief.fromJson),
      pinsStale: _opt(m['pins_stale'], _bool) ?? false,
      oreReserve: _opt(m['ore_reserve'], OreReserve.fromJson),
      chart: _opt(m['chart'], ChartMark.fromJson),
      outpost: _opt(m['outpost'], OutpostBrief.fromJson),
      threat: ThreatInfo.fromJson(m['threat']),
      lastSeen: _int(m['last_seen']),
      live: _bool(m['live']),
    );
  }

  Json toJson() => {
    'location': location.toJson(),
    'terrain': terrain.toJson(),
    'resources': resources.toJson(),
    'connections': connections.map((e) => e.toJson()).toList(),
    'hostiles': ?hostiles?.map((e) => e.toJson()).toList(),
    'player_fleets': ?playerFleets?.map((e) => e.toJson()).toList(),
    'salvage': ?salvage?.toJson(),
    'salvage_despawn_tick': ?salvageDespawnTick,
    'site': ?site?.toJson(),
    if (pinsStale) 'pins_stale': pinsStale,
    'ore_reserve': ?oreReserve?.toJson(),
    'chart': ?chart?.toJson(),
    'outpost': ?outpost?.toJson(),
    'threat': threat.toJson(),
    'last_seen': lastSeen,
    'live': live,
  };
}

/// A sector's chart is made by entering it and pays explore points once.
enum ChartMark {
  /// A fleet that entered the sector still carries the chart: it is paid
  /// when that fleet docks at the homeworld or arrives at its owner's
  /// finished outpost, and lost if the fleet is.
  aboard('aboard'),

  /// Delivered and paid; entering again earns nothing.
  delivered('delivered');

  const ChartMark(this.wire);

  final String wire;

  static ChartMark fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// What a fleet on-site can tell about a derelict before boarding.
class SiteBrief {
  final int tier;

  /// Rough boarding risk read: "quiet" / "uneasy" / "hot".
  final SiteRisk risk;

  const SiteBrief({required this.tier, required this.risk});

  factory SiteBrief.fromJson(Object? json) {
    final m = _obj(json);
    return SiteBrief(tier: _int(m['tier']), risk: SiteRisk.fromJson(m['risk']));
  }

  Json toJson() => {'tier': tier, 'risk': risk.toJson()};
}

enum SiteRisk {
  quiet('quiet', label: 'quiet'),
  uneasy('uneasy', label: 'uneasy'),
  hot('hot', label: 'hot');

  const SiteRisk(this.wire, {required this.label});

  final String wire;
  final String label;

  static SiteRisk fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// The ore of a sector tile by tile. A tile is a small nugget: harvesting
/// takes `units` down toward 0, and it refills on a timer while no fleet sits on it.
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
      units: _double(m['units']),
      maxUnits: _double(m['max_units']),
      refillsInS: _opt(m['refills_in_s'], _int),
    );
  }

  Json toJson() => {'units': units, 'max_units': maxUnits, 'refills_in_s': ?refillsInS};
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
      id: _int(m['id']),
      ships: _list(m['ships'], NpcShipInfo.fromJson),
      behavior: NpcBehavior.fromJson(m['behavior']),
    );
  }

  Json toJson() => {'id': id, 'ships': ships.map((e) => e.toJson()).toList(), 'behavior': behavior.toJson()};
}

class NpcShipInfo {
  final ShipClass shipClass;
  final int count;

  const NpcShipInfo({required this.shipClass, required this.count});

  factory NpcShipInfo.fromJson(Object? json) {
    final m = _obj(json);
    return NpcShipInfo(shipClass: ShipClass.fromJson(m['class']), count: _int(m['count']));
  }

  Json toJson() => {'class': shipClass.toJson(), 'count': count};
}

enum NpcBehavior {
  passive('Passive'),
  patrol('Patrol'),
  aggressive('Aggressive'),
  swarm('Swarm');

  const NpcBehavior(this.wire);

  final String wire;

  static NpcBehavior fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
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
      id: _int(m['id']),
      ownerName: _string(m['owner_name']),
      shipCount: _int(m['ship_count']),
      shipClasses: _opt(m['ship_classes'], ShipClassCounts.fromJson) ?? const ShipClassCounts(),
    );
  }

  Json toJson() => {'id': id, 'owner_name': ownerName, 'ship_count': shipCount, 'ship_classes': shipClasses.toJson()};
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
      scout: _int(m['scout']),
      corvette: _int(m['corvette']),
      frigate: _int(m['frigate']),
      cruiser: _int(m['cruiser']),
      hauler: _int(m['hauler']),
    );
  }

  Json toJson() => {'scout': scout, 'corvette': corvette, 'frigate': frigate, 'cruiser': cruiser, 'hauler': hauler};
}

class HomeworldState {
  final Hex location;

  /// What the mines make each tick at current levels (before the storage
  /// cap discards the excess; see `storage`).
  final Resources production;
  final StorageState storage;
  final List<BuildingState> buildings;
  final List<ResearchState> research;

  /// Buildings under construction, at most `build_slots` of them.
  final List<BuildQueueItem> buildQueue;

  /// Waiting behind them; nothing is paid until an item starts.
  final List<QueuedBuild> buildPending;

  /// How many buildings may be under construction at once.
  final int buildSlots;

  /// Orders that may wait behind the running ones in each queue. An order
  /// that fits is always accepted, started or not; only a full waiting
  /// line (or an order that can never start) is refused with `QueueFull`.
  final int queueWaitingMax;
  final ShipyardQueueItem? shipyardQueue;
  final List<QueuedShip> shipyardPending;
  final ResearchItem? researchActive;
  final List<QueuedResearch> researchPending;
  final List<ShipState> dockedShips;

  /// Structures standing at home (those being rebuilt after a raid are
  /// counted in `restoring`, not `count`).
  final List<DefenceState> defences;

  /// Docked ships, the Defense Grid and all structures together: what the
  /// next raid meets.
  final double homeDefencePower;

  /// Typical power of the next raid, from the empire's building and
  /// research points; the actual raid rolls 0.80 to 1.15 times this.
  final double nextRaidEstimatePower;

  /// What can be queued right now and what it costs; see `HomeworldCatalog`.
  final HomeworldCatalog catalog;

  const HomeworldState({
    required this.location,
    required this.production,
    required this.storage,
    required this.buildings,
    required this.research,
    required this.buildQueue,
    required this.buildPending,
    required this.buildSlots,
    required this.queueWaitingMax,
    this.shipyardQueue,
    required this.shipyardPending,
    this.researchActive,
    required this.researchPending,
    required this.dockedShips,
    required this.defences,
    required this.homeDefencePower,
    required this.nextRaidEstimatePower,
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
      buildSlots: _int(m['build_slots']),
      queueWaitingMax: _int(m['queue_waiting_max']),
      shipyardQueue: _opt(m['shipyard_queue'], ShipyardQueueItem.fromJson),
      shipyardPending: _list(m['shipyard_pending'], QueuedShip.fromJson),
      researchActive: _opt(m['research_active'], ResearchItem.fromJson),
      researchPending: _list(m['research_pending'], QueuedResearch.fromJson),
      dockedShips: _list(m['docked_ships'], ShipState.fromJson),
      defences: _list(m['defences'], DefenceState.fromJson),
      homeDefencePower: _double(m['home_defence_power']),
      nextRaidEstimatePower: _double(m['next_raid_estimate_power']),
      catalog: HomeworldCatalog.fromJson(m['catalog']),
    );
  }

  Json toJson() => {
    'location': location.toJson(),
    'production': production.toJson(),
    'storage': storage.toJson(),
    'buildings': buildings.map((e) => e.toJson()).toList(),
    'research': research.map((e) => e.toJson()).toList(),
    'build_queue': buildQueue.map((e) => e.toJson()).toList(),
    'build_pending': buildPending.map((e) => e.toJson()).toList(),
    'build_slots': buildSlots,
    'queue_waiting_max': queueWaitingMax,
    'shipyard_queue': ?shipyardQueue?.toJson(),
    'shipyard_pending': shipyardPending.map((e) => e.toJson()).toList(),
    'research_active': ?researchActive?.toJson(),
    'research_pending': researchPending.map((e) => e.toJson()).toList(),
    'docked_ships': dockedShips.map((e) => e.toJson()).toList(),
    'defences': defences.map((e) => e.toJson()).toList(),
    'home_defence_power': homeDefencePower,
    'next_raid_estimate_power': nextRaidEstimatePower,
    'catalog': catalog.toJson(),
  };
}

class DefenceState {
  final DefenceKind kind;
  final int count;

  /// Structures a repelled raid destroyed that come back for free.
  final int restoring;

  /// Power of one structure at the current Defense Grid level.
  final double power;

  const DefenceState({required this.kind, required this.count, required this.restoring, required this.power});

  factory DefenceState.fromJson(Object? json) {
    final m = _obj(json);
    return DefenceState(
      kind: DefenceKind.fromJson(m['kind']),
      count: _int(m['count']),
      restoring: _int(m['restoring']),
      power: _double(m['power']),
    );
  }

  Json toJson() => {'kind': kind.toJson(), 'count': count, 'restoring': restoring, 'power': power};
}

/// Stockpile limits, from the Storage Vault level and the world pace.
/// Mines stop at `cap`; docking cargo unloads only up to it.
class StorageState {
  final Resources cap;

  /// What a raid cannot skim.
  final Resources protected;

  /// Seconds until each stockpile is full at the current production;
  /// 0 when it is, null when it is not filling.
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
      metal: _opt(m['metal'], _int),
      crystal: _opt(m['crystal'], _int),
      deuterium: _opt(m['deuterium'], _int),
    );
  }

  Json toJson() => {'metal': metal, 'crystal': crystal, 'deuterium': deuterium};
}

/// Everything a client needs to offer homeworld actions without knowing any
/// balance formulas: costs, build times and prerequisites, computed by the
/// server from `scaling.rs` for the player's current levels. Resent with
/// every `HomeworldState`, so it tracks level changes the tick they land.
/// Whether the player can pay is left to the client (compare `cost` with the
/// stockpile in `PlayerState.resources`).
class HomeworldCatalog {
  /// One entry per `BuildingType`, in declaration order.
  final List<BuildingOption> buildings;

  /// One entry per `ResearchType`, in declaration order.
  final List<ResearchOption> research;

  /// One entry per `ShipClass`, in declaration order.
  final List<ShipOption> ships;

  /// One entry per `DefenceKind`, in declaration order.
  final List<DefenceOption> defences;

  const HomeworldCatalog({
    required this.buildings,
    required this.research,
    required this.ships,
    required this.defences,
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

/// One prerequisite: a building or tech that must reach `need`, with the
/// player's current level. The server's "prerequisites not met" errors are
/// written from the same values.
class Requirement {
  /// Building or tech label ("Shipyard", "Frigate Tech").
  final String name;
  final int need;
  final int have;
  final bool met;

  const Requirement({required this.name, required this.need, required this.have, required this.met});

  factory Requirement.fromJson(Object? json) {
    final m = _obj(json);
    return Requirement(name: _string(m['name']), need: _int(m['need']), have: _int(m['have']), met: _bool(m['met']));
  }

  Json toJson() => {'name': name, 'need': need, 'have': have, 'met': met};
}

/// Cost and duration of the next level of a building or tech.
class UpgradeStep {
  final int level;
  final Resources cost;
  final int ticks;

  const UpgradeStep({required this.level, required this.cost, required this.ticks});

  factory UpgradeStep.fromJson(Object? json) {
    final m = _obj(json);
    return UpgradeStep(level: _int(m['level']), cost: Resources.fromJson(m['cost']), ticks: _int(m['ticks']));
  }

  Json toJson() => {'level': level, 'cost': cost.toJson(), 'ticks': ticks};
}

class BuildingOption {
  final BuildingType buildingType;
  final int level;
  final int maxLevel;

  /// The upgrade to `level + 1`; absent at `max_level`.
  final UpgradeStep? next;

  /// All prerequisites with their status; the entry is locked while any is unmet.
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
      level: _int(m['level']),
      maxLevel: _int(m['max_level']),
      next: _opt(m['next'], UpgradeStep.fromJson),
      requires: _list(m['requires'], Requirement.fromJson),
    );
  }

  Json toJson() => {
    'building_type': buildingType.toJson(),
    'level': level,
    'max_level': maxLevel,
    'next': ?next?.toJson(),
    'requires': requires.map((e) => e.toJson()).toList(),
  };
}

class ResearchOption {
  final ResearchType tech;
  final int level;
  final int maxLevel;

  /// The upgrade to `level + 1`; absent at `max_level`.
  final UpgradeStep? next;

  /// Includes the Research Lab the server requires before any research.
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
      level: _int(m['level']),
      maxLevel: _int(m['max_level']),
      next: _opt(m['next'], UpgradeStep.fromJson),
      requires: _list(m['requires'], Requirement.fromJson),
    );
  }

  Json toJson() => {
    'tech': tech.toJson(),
    'level': level,
    'max_level': maxLevel,
    'next': ?next?.toJson(),
    'requires': requires.map((e) => e.toJson()).toList(),
  };
}

class ShipOption {
  final ShipClass shipClass;

  /// Cost of one ship; a batch of `count` costs `count` times this.
  final Resources unitCost;

  /// Ticks to build one ship at the current shipyard level.
  final int ticksPerShip;

  /// Includes the Shipyard the server requires before any ship.
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
      ticksPerShip: _int(m['ticks_per_ship']),
      requires: _list(m['requires'], Requirement.fromJson),
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

  /// Power of one structure at the current Defense Grid level.
  final double power;

  /// Includes the Shipyard the server requires before anything is built.
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
      count: _int(m['count']),
      unitCost: Resources.fromJson(m['unit_cost']),
      ticksPerUnit: _int(m['ticks_per_unit']),
      power: _double(m['power']),
      requires: _list(m['requires'], Requirement.fromJson),
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
    return BuildingState(buildingType: BuildingType.fromJson(m['building_type']), level: _int(m['level']));
  }

  Json toJson() => {'building_type': buildingType.toJson(), 'level': level};
}

class ResearchState {
  final ResearchType tech;
  final int level;

  const ResearchState({required this.tech, required this.level});

  factory ResearchState.fromJson(Object? json) {
    final m = _obj(json);
    return ResearchState(tech: ResearchType.fromJson(m['tech']), level: _int(m['level']));
  }

  Json toJson() => {'tech': tech.toJson(), 'level': level};
}

/// A building under construction. `id` is the stable queue id: the one the
/// order got when it was queued, kept when it started.
class BuildQueueItem {
  final int id;
  final BuildingType buildingType;
  final int targetLevel;
  final int startTick;
  final int endTick;

  const BuildQueueItem({
    required this.id,
    required this.buildingType,
    required this.targetLevel,
    required this.startTick,
    required this.endTick,
  });

  factory BuildQueueItem.fromJson(Object? json) {
    final m = _obj(json);
    return BuildQueueItem(
      id: _int(m['id']),
      buildingType: BuildingType.fromJson(m['building_type']),
      targetLevel: _int(m['target_level']),
      startTick: _int(m['start_tick']),
      endTick: _int(m['end_tick']),
    );
  }

  Json toJson() => {
    'id': id,
    'building_type': buildingType.toJson(),
    'target_level': targetLevel,
    'start_tick': startTick,
    'end_tick': endTick,
  };
}

/// Why a queued order has not started yet.
enum WaitReason {
  /// Every slot is busy.
  slot('Slot'),

  /// The stockpile does not cover the cost (see `waiting_for`).
  resources('Resources'),

  /// A level, building or tech it needs is still being built.
  prerequisite('Prerequisite'),

  /// It could start, but an earlier order with `reserve` set is ready and
  /// short of resources, and holds the line.
  order('Order');

  const WaitReason(this.wire);

  final String wire;

  static WaitReason fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// Why a shipyard batch has not started yet. A batch is paid for when it is
/// ordered, so it never waits on resources or behind a reserving order.
enum BatchWait {
  /// The yard is busy with a batch ahead of it.
  slot('Slot'),

  /// A Shipyard level, building or tech it needs is still being built.
  prerequisite('Prerequisite');

  const BatchWait(this.wire);

  final String wire;

  static BatchWait fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// A building waiting to start. A free slot starts the earliest waiting
/// order that can start now; an order that cannot keeps its place unless it
/// has `reserve` set, in which case it holds the line while short. `id` is
/// stable for the life of the order (cancel by it). `cost` and `ticks` are
/// what it would cost and take if it started now; `waiting_for` is the exact
/// resources still missing (absent when nothing is short); `waiting_on`
/// names the binding reason; `start_in` is the estimated ticks until it
/// starts at the current production, ignoring what the other queues will
/// spend (absent when it depends on something that cannot be estimated).
class QueuedBuild {
  final int id;
  final BuildingType buildingType;
  final int targetLevel;
  final bool reserve;
  final Resources cost;
  final int ticks;
  final Resources? waitingFor;
  final WaitReason waitingOn;
  final int? startIn;

  const QueuedBuild({
    required this.id,
    required this.buildingType,
    required this.targetLevel,
    this.reserve = false,
    required this.cost,
    required this.ticks,
    this.waitingFor,
    required this.waitingOn,
    this.startIn,
  });

  factory QueuedBuild.fromJson(Object? json) {
    final m = _obj(json);
    return QueuedBuild(
      id: _int(m['id']),
      buildingType: BuildingType.fromJson(m['building_type']),
      targetLevel: _int(m['target_level']),
      reserve: _opt(m['reserve'], _bool) ?? false,
      cost: Resources.fromJson(m['cost']),
      ticks: _int(m['ticks']),
      waitingFor: _opt(m['waiting_for'], Resources.fromJson),
      waitingOn: WaitReason.fromJson(m['waiting_on']),
      startIn: _opt(m['start_in'], _int),
    );
  }

  Json toJson() => {
    'id': id,
    'building_type': buildingType.toJson(),
    'target_level': targetLevel,
    'reserve': reserve,
    'cost': cost.toJson(),
    'ticks': ticks,
    'waiting_for': ?waitingFor?.toJson(),
    'waiting_on': waitingOn.toJson(),
    'start_in': ?startIn,
  };
}

/// A research level waiting to start; see `QueuedBuild`.
class QueuedResearch {
  final int id;
  final ResearchType tech;
  final int targetLevel;
  final bool reserve;
  final Resources cost;
  final int ticks;
  final Resources? waitingFor;
  final WaitReason waitingOn;
  final int? startIn;

  const QueuedResearch({
    required this.id,
    required this.tech,
    required this.targetLevel,
    this.reserve = false,
    required this.cost,
    required this.ticks,
    this.waitingFor,
    required this.waitingOn,
    this.startIn,
  });

  factory QueuedResearch.fromJson(Object? json) {
    final m = _obj(json);
    return QueuedResearch(
      id: _int(m['id']),
      tech: ResearchType.fromJson(m['tech']),
      targetLevel: _int(m['target_level']),
      reserve: _opt(m['reserve'], _bool) ?? false,
      cost: Resources.fromJson(m['cost']),
      ticks: _int(m['ticks']),
      waitingFor: _opt(m['waiting_for'], Resources.fromJson),
      waitingOn: WaitReason.fromJson(m['waiting_on']),
      startIn: _opt(m['start_in'], _int),
    );
  }

  Json toJson() => {
    'id': id,
    'tech': tech.toJson(),
    'target_level': targetLevel,
    'reserve': reserve,
    'cost': cost.toJson(),
    'ticks': ticks,
    'waiting_for': ?waitingFor?.toJson(),
    'waiting_on': waitingOn.toJson(),
    'start_in': ?startIn,
  };
}

/// A shipyard batch that is paid for and waiting its turn. `id` is stable
/// for the life of the order (cancel by it; half of `cost` comes back).
/// `cost` is what the whole batch paid when it was ordered, `unit_cost` one
/// unit, `ticks` the time for one unit; `start_in` is the estimated ticks
/// until its first unit starts (absent when it depends on something that
/// cannot be estimated). A batch that has started builds every unit without
/// stopping.
class QueuedShip {
  final int id;

  /// A ship class or a defence structure: the wire carries just its name.
  final ShipyardItem item;
  final int count;
  final Resources cost;
  final Resources unitCost;
  final int ticks;
  final BatchWait waitingOn;
  final int? startIn;

  const QueuedShip({
    required this.id,
    required this.item,
    required this.count,
    required this.cost,
    required this.unitCost,
    required this.ticks,
    required this.waitingOn,
    this.startIn,
  });

  factory QueuedShip.fromJson(Object? json) {
    final m = _obj(json);
    return QueuedShip(
      id: _int(m['id']),
      item: ShipyardItem.fromJson(m['item']),
      count: _int(m['count']),
      cost: Resources.fromJson(m['cost']),
      unitCost: Resources.fromJson(m['unit_cost']),
      ticks: _int(m['ticks']),
      waitingOn: BatchWait.fromJson(m['waiting_on']),
      startIn: _opt(m['start_in'], _int),
    );
  }

  Json toJson() => {
    'id': id,
    'item': item.toJson(),
    'count': count,
    'cost': cost.toJson(),
    'unit_cost': unitCost.toJson(),
    'ticks': ticks,
    'waiting_on': waitingOn.toJson(),
    'start_in': ?startIn,
  };
}

/// The batch in production. `built` units of `count` are delivered and one
/// more is under way until `end_tick`; the whole batch was paid for when it
/// was ordered.
class ShipyardQueueItem {
  final int id;

  /// A ship class or a defence structure: the wire carries just its name.
  final ShipyardItem item;
  final int count;
  final int built;
  final int startTick;
  final int endTick;

  const ShipyardQueueItem({
    required this.id,
    required this.item,
    required this.count,
    required this.built,
    required this.startTick,
    required this.endTick,
  });

  factory ShipyardQueueItem.fromJson(Object? json) {
    final m = _obj(json);
    return ShipyardQueueItem(
      id: _int(m['id']),
      item: ShipyardItem.fromJson(m['item']),
      count: _int(m['count']),
      built: _int(m['built']),
      startTick: _int(m['start_tick']),
      endTick: _int(m['end_tick']),
    );
  }

  Json toJson() => {
    'id': id,
    'item': item.toJson(),
    'count': count,
    'built': built,
    'start_tick': startTick,
    'end_tick': endTick,
  };
}

class ResearchItem {
  final int id;
  final ResearchType tech;
  final int targetLevel;
  final int startTick;
  final int endTick;

  const ResearchItem({
    required this.id,
    required this.tech,
    required this.targetLevel,
    required this.startTick,
    required this.endTick,
  });

  factory ResearchItem.fromJson(Object? json) {
    final m = _obj(json);
    return ResearchItem(
      id: _int(m['id']),
      tech: ResearchType.fromJson(m['tech']),
      targetLevel: _int(m['target_level']),
      startTick: _int(m['start_tick']),
      endTick: _int(m['end_tick']),
    );
  }

  Json toJson() => {
    'id': id,
    'tech': tech.toJson(),
    'target_level': targetLevel,
    'start_tick': startTick,
    'end_tick': endTick,
  };
}

class GameEvent {
  final int tick;
  final EventKind kind;

  const GameEvent({required this.tick, required this.kind});

  factory GameEvent.fromJson(Object? json) {
    final m = _obj(json);
    return GameEvent(tick: _int(m['tick']), kind: EventKind.fromJson(json));
  }

  Json toJson() => {'tick': tick, ...kind.toJson()};
}

sealed class EventKind {
  const EventKind();

  static EventKind fromJson(Object? json) {
    return switch (_obj(json)['kind']) {
      'CombatRound' => CombatRoundEvent.fromJson(json),
      'ShipDestroyed' => ShipDestroyedEvent.fromJson(json),
      'FleetDestroyed' => FleetDestroyedEvent.fromJson(json),
      'ResourceHarvested' => ResourceHarvestedEvent.fromJson(json),
      'SectorEntered' => SectorEnteredEvent.fromJson(json),
      'CombatStarted' => CombatStartedEvent.fromJson(json),
      'CombatEnded' => CombatEndedEvent.fromJson(json),
      'SalvageCollected' => SalvageCollectedEvent.fromJson(json),
      'FleetArrived' => FleetArrivedEvent.fromJson(json),
      'BuildingCompleted' => BuildingCompletedEvent.fromJson(json),
      'ResearchCompleted' => ResearchCompletedEvent.fromJson(json),
      'ShipBuilt' => ShipBuiltEvent.fromJson(json),
      'DefenceBuilt' => DefenceBuiltEvent.fromJson(json),
      'ScanCompleted' => ScanCompletedEvent.fromJson(json),
      'RaidIncoming' => RaidIncomingEvent.fromJson(json),
      'RaidResolved' => RaidResolvedEvent.fromJson(json),
      'SiteExplorationStarted' => SiteExplorationStartedEvent.fromJson(json),
      'SiteExplored' => SiteExploredEvent.fromJson(json),
      'SiteAmbush' => SiteAmbushEvent.fromJson(json),
      'PolicyAction' => PolicyActionEvent.fromJson(json),
      'Alert' => AlertEvent.fromJson(json),
      'SalvageDespawned' => SalvageDespawnedEvent.fromJson(json),
      'StorageNearCap' => StorageNearCapEvent.fromJson(json),
      'StorageFull' => StorageFullEvent.fromJson(json),
      'ChartDelivered' => ChartDeliveredEvent.fromJson(json),
      'Queue' => QueueEvent.fromJson(json),
      'Exchanged' => ExchangedEvent.fromJson(json),
      'OutpostCompleted' => OutpostCompletedEvent.fromJson(json),
      final tag => throw FormatException('unknown EventKind kind "$tag"'),
    };
  }

  Json toJson();
}

/// A level of the player's outpost is finished. Level 1 opens it: the
/// extractor starts, tanks are filled and the raid clock begins to run.
class OutpostCompletedEvent extends EventKind {
  final int playerId;
  final Hex sector;
  final int level;

  const OutpostCompletedEvent({required this.playerId, required this.sector, required this.level});

  factory OutpostCompletedEvent.fromJson(Object? json) {
    final m = _obj(json);
    return OutpostCompletedEvent(
      playerId: _int(m['player_id']),
      sector: Hex.fromJson(m['sector']),
      level: _int(m['level']),
    );
  }

  @override
  Json toJson() => {'kind': 'OutpostCompleted', 'player_id': playerId, 'sector': sector.toJson(), 'level': level};
}

/// A fleet traded at the hub's exchange: what it gave, what it got, the fee
/// and where the player's meter now stands.
class ExchangedEvent extends EventKind {
  final Hex sector;
  final ExchangeTerms terms;

  const ExchangedEvent({required this.sector, required this.terms});

  factory ExchangedEvent.fromJson(Object? json) {
    final m = _obj(json);
    return ExchangedEvent(sector: Hex.fromJson(m['sector']), terms: ExchangeTerms.fromJson(json));
  }

  @override
  Json toJson() => {'kind': 'Exchanged', 'sector': sector.toJson(), ...terms.toJson()};
}

/// What happened to a queue order.
enum QueueAction {
  /// The order began.
  started('Started'),

  /// The order was accepted but cannot start yet.
  waiting('Waiting'),

  /// The order was cancelled.
  cancelled('Cancelled');

  const QueueAction(this.wire);

  final String wire;

  static QueueAction fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// The answer to a build, research, ship or defence order, and to a cancel,
/// and the notice that a waiting order started on its own.
class QueueEvent extends EventKind {
  final int? playerId;
  final QueueType queueType;

  /// The stable queue id of the order; what `cancel_build` and
  /// `cancel_queued` take.
  final int id;

  /// "Metal Mine Lv.5", "Corvette x3".
  final String item;
  final QueueAction action;

  /// What this event took from the stockpile. A building or research
  /// level pays when it starts (`Started`). A shipyard batch pays for
  /// every unit when it is ordered: on the event that answers the order,
  /// `Started` if it began at once or `Waiting` if it joined the line, and
  /// nothing on the later `Started` of a batch that waited.
  final Resources paid;

  /// `Cancelled`: what came back. Half of the payment for a started
  /// building or research level, nothing for a waiting one; for a
  /// shipyard batch, waiting or started, half the price of the units not
  /// yet delivered.
  final Resources refunded;

  /// `Waiting`: the exact shortfall of a building or research order,
  /// absent when nothing is short. Never present for a shipyard batch.
  final Resources? waitingFor;

  /// `Waiting`: why it has not started.
  final WaitReason? waitingOn;

  /// `Waiting`: estimated ticks until it starts, when it can be estimated.
  final int? startIn;

  const QueueEvent({
    this.playerId,
    required this.queueType,
    required this.id,
    required this.item,
    required this.action,
    this.paid = const Resources(),
    this.refunded = const Resources(),
    this.waitingFor,
    this.waitingOn,
    this.startIn,
  });

  factory QueueEvent.fromJson(Object? json) {
    final m = _obj(json);
    return QueueEvent(
      playerId: _opt(m['player_id'], _int),
      queueType: QueueType.fromJson(m['queue_type']),
      id: _int(m['id']),
      item: _string(m['item']),
      action: QueueAction.fromJson(m['action']),
      paid: _opt(m['paid'], Resources.fromJson) ?? const Resources(),
      refunded: _opt(m['refunded'], Resources.fromJson) ?? const Resources(),
      waitingFor: _opt(m['waiting_for'], Resources.fromJson),
      waitingOn: _opt(m['waiting_on'], WaitReason.fromJson),
      startIn: _opt(m['start_in'], _int),
    );
  }

  @override
  Json toJson() => {
    'kind': 'Queue',
    'player_id': ?playerId,
    'queue_type': queueType.toJson(),
    'id': id,
    'item': item,
    'action': action.toJson(),
    'paid': paid.toJson(),
    'refunded': refunded.toJson(),
    'waiting_for': ?waitingFor?.toJson(),
    'waiting_on': ?waitingOn?.toJson(),
    'start_in': ?startIn,
  };
}

/// A stockpile passed 85 percent of its cap.
class StorageNearCapEvent extends EventKind {
  final int? playerId;
  final ResourceKind resource;
  final double ratio;

  /// Seconds until full at the current production; absent when not filling.
  final int? fullInS;

  const StorageNearCapEvent({this.playerId, required this.resource, required this.ratio, this.fullInS});

  factory StorageNearCapEvent.fromJson(Object? json) {
    final m = _obj(json);
    return StorageNearCapEvent(
      playerId: _opt(m['player_id'], _int),
      resource: ResourceKind.fromJson(m['resource']),
      ratio: _double(m['ratio']),
      fullInS: _opt(m['full_in_s'], _int),
    );
  }

  @override
  Json toJson() => {
    'kind': 'StorageNearCap',
    'player_id': ?playerId,
    'resource': resource.toJson(),
    'ratio': ratio,
    'full_in_s': ?fullInS,
  };
}

/// A stockpile reached its cap; further production is discarded.
class StorageFullEvent extends EventKind {
  final int? playerId;
  final ResourceKind resource;

  const StorageFullEvent({this.playerId, required this.resource});

  factory StorageFullEvent.fromJson(Object? json) {
    final m = _obj(json);
    return StorageFullEvent(playerId: _opt(m['player_id'], _int), resource: ResourceKind.fromJson(m['resource']));
  }

  @override
  Json toJson() => {'kind': 'StorageFull', 'player_id': ?playerId, 'resource': resource.toJson()};
}

/// A boarding party is aboard the derelict; loot (or trouble) at end_tick.
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

  factory SiteExplorationStartedEvent.fromJson(Object? json) {
    final m = _obj(json);
    return SiteExplorationStartedEvent(
      fleetId: _int(m['fleet_id']),
      sector: Hex.fromJson(m['sector']),
      tier: _int(m['tier']),
      endTick: _int(m['end_tick']),
    );
  }

  @override
  Json toJson() => {
    'kind': 'SiteExplorationStarted',
    'fleet_id': fleetId,
    'sector': sector.toJson(),
    'tier': tier,
    'end_tick': endTick,
  };
}

/// The boarding paid off.
class SiteExploredEvent extends EventKind {
  final int fleetId;
  final Hex sector;
  final int tier;

  /// Loot taken aboard (cargo-capped; overflow drops as sector salvage).
  final Resources resources;
  final ShipClass? recoveredShip;

  /// Instant tech level granted by a recovered data core.
  final ResearchType? techCache;

  /// An ancient relic came aboard (worth 25 explore points; no other use yet).
  final bool relic;

  /// Explore points this boarding earned (0 when the site was boarded before).
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

  factory SiteExploredEvent.fromJson(Object? json) {
    final m = _obj(json);
    return SiteExploredEvent(
      fleetId: _int(m['fleet_id']),
      sector: Hex.fromJson(m['sector']),
      tier: _int(m['tier']),
      resources: Resources.fromJson(m['resources']),
      recoveredShip: _opt(m['recovered_ship'], ShipClass.fromJson),
      techCache: _opt(m['tech_cache'], ResearchType.fromJson),
      relic: _opt(m['relic'], _bool) ?? false,
      points: _opt(m['points'], _double) ?? 0,
    );
  }

  @override
  Json toJson() => {
    'kind': 'SiteExplored',
    'fleet_id': fleetId,
    'sector': sector.toJson(),
    'tier': tier,
    'resources': resources.toJson(),
    'recovered_ship': ?recoveredShip?.toJson(),
    'tech_cache': ?techCache?.toJson(),
    if (relic) 'relic': relic,
    if (points != 0) 'points': points,
  };
}

/// A fleet docked at the homeworld, or arrived at its owner's finished
/// outpost, and handed over the sectors it charted.
class ChartDeliveredEvent extends EventKind {
  final int fleetId;

  /// Sectors newly credited to the empire.
  final int sectors;

  /// Explore points they earned.
  final double points;

  const ChartDeliveredEvent({required this.fleetId, required this.sectors, required this.points});

  factory ChartDeliveredEvent.fromJson(Object? json) {
    final m = _obj(json);
    return ChartDeliveredEvent(fleetId: _int(m['fleet_id']), sectors: _int(m['sectors']), points: _double(m['points']));
  }

  @override
  Json toJson() => {'kind': 'ChartDelivered', 'fleet_id': fleetId, 'sectors': sectors, 'points': points};
}

/// Whatever killed the crew is still aboard.
class SiteAmbushEvent extends EventKind {
  final int fleetId;
  final Hex sector;
  final int npcFleetId;

  const SiteAmbushEvent({required this.fleetId, required this.sector, required this.npcFleetId});

  factory SiteAmbushEvent.fromJson(Object? json) {
    final m = _obj(json);
    return SiteAmbushEvent(
      fleetId: _int(m['fleet_id']),
      sector: Hex.fromJson(m['sector']),
      npcFleetId: _int(m['npc_fleet_id']),
    );
  }

  @override
  Json toJson() => {'kind': 'SiteAmbush', 'fleet_id': fleetId, 'sector': sector.toJson(), 'npc_fleet_id': npcFleetId};
}

/// A standing order acted on the player's behalf — always says why.
class PolicyActionEvent extends EventKind {
  final int fleetId;
  final PolicyPreset preset;

  /// Short verb: "scan", "move", "harvest", "return", "attack", …
  final String action;
  final String reason;

  const PolicyActionEvent({required this.fleetId, required this.preset, required this.action, required this.reason});

  factory PolicyActionEvent.fromJson(Object? json) {
    final m = _obj(json);
    return PolicyActionEvent(
      fleetId: _int(m['fleet_id']),
      preset: PolicyPreset.fromJson(m['preset']),
      action: _string(m['action']),
      reason: _string(m['reason']),
    );
  }

  @override
  Json toJson() => {
    'kind': 'PolicyAction',
    'fleet_id': fleetId,
    'preset': preset.toJson(),
    'action': action,
    'reason': reason,
  };
}

/// One shot in a fight.
class CombatRoundEvent extends EventKind {
  final Hex sector;
  final int attackerFleetId;

  /// Empire owning the attacking fleet; `None` for an NPC.
  final String? attackerOwner;
  final int attackerShipId;
  final int targetFleetId;

  /// Empire owning the target fleet; `None` for an NPC.
  final String? targetOwner;
  final int targetShipId;
  final double damage;
  final double shieldAbsorbed;
  final double hullDamage;
  final bool rapidFire;

  /// One of the two fleets is the receiver's.
  final bool mine;

  const CombatRoundEvent({
    required this.sector,
    required this.attackerFleetId,
    this.attackerOwner,
    required this.attackerShipId,
    required this.targetFleetId,
    this.targetOwner,
    required this.targetShipId,
    required this.damage,
    required this.shieldAbsorbed,
    required this.hullDamage,
    required this.rapidFire,
    this.mine = false,
  });

  factory CombatRoundEvent.fromJson(Object? json) {
    final m = _obj(json);
    return CombatRoundEvent(
      sector: Hex.fromJson(m['sector']),
      attackerFleetId: _int(m['attacker_fleet_id']),
      attackerOwner: _opt(m['attacker_owner'], _string),
      attackerShipId: _int(m['attacker_ship_id']),
      targetFleetId: _int(m['target_fleet_id']),
      targetOwner: _opt(m['target_owner'], _string),
      targetShipId: _int(m['target_ship_id']),
      damage: _double(m['damage']),
      shieldAbsorbed: _double(m['shield_absorbed']),
      hullDamage: _double(m['hull_damage']),
      rapidFire: _bool(m['rapid_fire']),
      mine: _opt(m['mine'], _bool) ?? false,
    );
  }

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

  /// Empire that owned the ship; `None` for an NPC.
  final String? owner;

  /// The ship was the receiver's.
  final bool mine;

  const ShipDestroyedEvent({
    required this.shipId,
    required this.shipClass,
    required this.ownerFleetId,
    required this.isNpc,
    required this.sector,
    this.owner,
    this.mine = false,
  });

  factory ShipDestroyedEvent.fromJson(Object? json) {
    final m = _obj(json);
    return ShipDestroyedEvent(
      shipId: _int(m['ship_id']),
      shipClass: ShipClass.fromJson(m['ship_class']),
      ownerFleetId: _int(m['owner_fleet_id']),
      isNpc: _bool(m['is_npc']),
      sector: Hex.fromJson(m['sector']),
      owner: _opt(m['owner'], _string),
      mine: _opt(m['mine'], _bool) ?? false,
    );
  }

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

/// Every ship of a fleet is gone. For an NPC fleet killed by players,
/// `salvage` is the wreckage it left in `sector`: exactly what
/// `collect_salvage` can take (when several NPC fleets die in one fight the
/// sector pile is the sum of their events). Player fleets leave no wreckage,
/// so `salvage` is zero. A destroyed fleet of the receiver's also produces a
/// `Critical` `Alert`.
class FleetDestroyedEvent extends EventKind {
  final int fleetId;
  final bool isNpc;
  final Hex sector;

  /// Empire that owned the fleet; `None` for an NPC.
  final String? owner;

  /// The fleet was the receiver's.
  final bool mine;
  final Resources salvage;

  /// What an NPC group was ("3x corvette pack"); empty for a player fleet.
  final String label;

  const FleetDestroyedEvent({
    required this.fleetId,
    required this.isNpc,
    required this.sector,
    this.owner,
    this.mine = false,
    required this.salvage,
    this.label = '',
  });

  factory FleetDestroyedEvent.fromJson(Object? json) {
    final m = _obj(json);
    return FleetDestroyedEvent(
      fleetId: _int(m['fleet_id']),
      isNpc: _bool(m['is_npc']),
      sector: Hex.fromJson(m['sector']),
      owner: _opt(m['owner'], _string),
      mine: _opt(m['mine'], _bool) ?? false,
      salvage: Resources.fromJson(m['salvage']),
      label: _opt(m['label'], _string) ?? '',
    );
  }

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

/// What a harvesting fleet took aboard. Aggregated: one event per fleet
/// every `HARVEST_REPORT_TICKS` ticks (and when harvesting stops) rather
/// than one per resource per tick; `ticks` is how many ticks it covers.
class ResourceHarvestedEvent extends EventKind {
  final int fleetId;
  final Resources resources;
  final int ticks;

  const ResourceHarvestedEvent({required this.fleetId, required this.resources, required this.ticks});

  factory ResourceHarvestedEvent.fromJson(Object? json) {
    final m = _obj(json);
    return ResourceHarvestedEvent(
      fleetId: _int(m['fleet_id']),
      resources: Resources.fromJson(m['resources']),
      ticks: _int(m['ticks']),
    );
  }

  @override
  Json toJson() => {'kind': 'ResourceHarvested', 'fleet_id': fleetId, 'resources': resources.toJson(), 'ticks': ticks};
}

class SectorEnteredEvent extends EventKind {
  final int fleetId;
  final Hex sector;
  final bool firstVisit;

  const SectorEnteredEvent({required this.fleetId, required this.sector, required this.firstVisit});

  factory SectorEnteredEvent.fromJson(Object? json) {
    final m = _obj(json);
    return SectorEnteredEvent(
      fleetId: _int(m['fleet_id']),
      sector: Hex.fromJson(m['sector']),
      firstVisit: _bool(m['first_visit']),
    );
  }

  @override
  Json toJson() => {'kind': 'SectorEntered', 'fleet_id': fleetId, 'sector': sector.toJson(), 'first_visit': firstVisit};
}

/// `enemy_fleet_id` is the same id the sector view listed under `hostiles`.
class CombatStartedEvent extends EventKind {
  final int playerFleetId;

  /// Empire owning `player_fleet_id`.
  final String owner;
  final int enemyFleetId;

  /// What the hostile group is ("3x corvette pack").
  final String enemy;
  final Hex sector;

  /// `player_fleet_id` is the receiver's.
  final bool mine;

  const CombatStartedEvent({
    required this.playerFleetId,
    required this.owner,
    required this.enemyFleetId,
    this.enemy = '',
    required this.sector,
    this.mine = false,
  });

  factory CombatStartedEvent.fromJson(Object? json) {
    final m = _obj(json);
    return CombatStartedEvent(
      playerFleetId: _int(m['player_fleet_id']),
      owner: _string(m['owner']),
      enemyFleetId: _int(m['enemy_fleet_id']),
      enemy: _opt(m['enemy'], _string) ?? '',
      sector: Hex.fromJson(m['sector']),
      mine: _opt(m['mine'], _bool) ?? false,
    );
  }

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

/// `owners` lists the empires that fought. `salvage` is the pile the victory
/// left in the sector (absent on defeat); it despawns at `salvage_despawn_tick`.
class CombatEndedEvent extends EventKind {
  final Hex sector;
  final bool playerVictory;
  final List<String> owners;
  final bool mine;
  final Resources? salvage;
  final int? salvageDespawnTick;

  const CombatEndedEvent({
    required this.sector,
    required this.playerVictory,
    required this.owners,
    this.mine = false,
    this.salvage,
    this.salvageDespawnTick,
  });

  factory CombatEndedEvent.fromJson(Object? json) {
    final m = _obj(json);
    return CombatEndedEvent(
      sector: Hex.fromJson(m['sector']),
      playerVictory: _bool(m['player_victory']),
      owners: _list(m['owners'], _string),
      mine: _opt(m['mine'], _bool) ?? false,
      salvage: _opt(m['salvage'], Resources.fromJson),
      salvageDespawnTick: _opt(m['salvage_despawn_tick'], _int),
    );
  }

  @override
  Json toJson() => {
    'kind': 'CombatEnded',
    'sector': sector.toJson(),
    'player_victory': playerVictory,
    'owners': owners,
    'mine': mine,
    'salvage': ?salvage?.toJson(),
    'salvage_despawn_tick': ?salvageDespawnTick,
  };
}

/// A fleet scooped wreckage. `resources` is what went into the hold; if the
/// hold filled first, `remaining` is what is still on the ground (it keeps
/// its original despawn time). Absent when the pile is gone.
class SalvageCollectedEvent extends EventKind {
  /// The collecting fleet. For the automatic scoop of a pile on the
  /// player's own homeworld, the lowest-id fleet docked there.
  final int fleetId;
  final Hex sector;
  final Resources resources;
  final Resources? remaining;

  const SalvageCollectedEvent({required this.fleetId, required this.sector, required this.resources, this.remaining});

  factory SalvageCollectedEvent.fromJson(Object? json) {
    final m = _obj(json);
    return SalvageCollectedEvent(
      fleetId: _int(m['fleet_id']),
      sector: Hex.fromJson(m['sector']),
      resources: Resources.fromJson(m['resources']),
      remaining: _opt(m['remaining'], Resources.fromJson),
    );
  }

  @override
  Json toJson() => {
    'kind': 'SalvageCollected',
    'fleet_id': fleetId,
    'sector': sector.toJson(),
    'resources': resources.toJson(),
    'remaining': ?remaining?.toJson(),
  };
}

/// A wreckage pile expired uncollected. Sent to players with a fleet or
/// their homeworld in `sector`.
class SalvageDespawnedEvent extends EventKind {
  final Hex sector;

  /// What was lost.
  final Resources resources;

  const SalvageDespawnedEvent({required this.sector, required this.resources});

  factory SalvageDespawnedEvent.fromJson(Object? json) {
    final m = _obj(json);
    return SalvageDespawnedEvent(sector: Hex.fromJson(m['sector']), resources: Resources.fromJson(m['resources']));
  }

  @override
  Json toJson() => {'kind': 'SalvageDespawned', 'sector': sector.toJson(), 'resources': resources.toJson()};
}

class FleetArrivedEvent extends EventKind {
  final int fleetId;
  final Hex sector;

  const FleetArrivedEvent({required this.fleetId, required this.sector});

  factory FleetArrivedEvent.fromJson(Object? json) {
    final m = _obj(json);
    return FleetArrivedEvent(fleetId: _int(m['fleet_id']), sector: Hex.fromJson(m['sector']));
  }

  @override
  Json toJson() => {'kind': 'FleetArrived', 'fleet_id': fleetId, 'sector': sector.toJson()};
}

class BuildingCompletedEvent extends EventKind {
  final BuildingType buildingType;
  final int newLevel;
  final int? playerId;

  const BuildingCompletedEvent({required this.buildingType, required this.newLevel, this.playerId});

  factory BuildingCompletedEvent.fromJson(Object? json) {
    final m = _obj(json);
    return BuildingCompletedEvent(
      buildingType: BuildingType.fromJson(m['building_type']),
      newLevel: _int(m['new_level']),
      playerId: _opt(m['player_id'], _int),
    );
  }

  @override
  Json toJson() => {
    'kind': 'BuildingCompleted',
    'building_type': buildingType.toJson(),
    'new_level': newLevel,
    'player_id': ?playerId,
  };
}

class ResearchCompletedEvent extends EventKind {
  final ResearchType tech;
  final int newLevel;
  final int? playerId;

  const ResearchCompletedEvent({required this.tech, required this.newLevel, this.playerId});

  factory ResearchCompletedEvent.fromJson(Object? json) {
    final m = _obj(json);
    return ResearchCompletedEvent(
      tech: ResearchType.fromJson(m['tech']),
      newLevel: _int(m['new_level']),
      playerId: _opt(m['player_id'], _int),
    );
  }

  @override
  Json toJson() => {'kind': 'ResearchCompleted', 'tech': tech.toJson(), 'new_level': newLevel, 'player_id': ?playerId};
}

class ShipBuiltEvent extends EventKind {
  final ShipClass shipClass;
  final int count;
  final int? playerId;

  const ShipBuiltEvent({required this.shipClass, required this.count, this.playerId});

  factory ShipBuiltEvent.fromJson(Object? json) {
    final m = _obj(json);
    return ShipBuiltEvent(
      shipClass: ShipClass.fromJson(m['ship_class']),
      count: _int(m['count']),
      playerId: _opt(m['player_id'], _int),
    );
  }

  @override
  Json toJson() => {'kind': 'ShipBuilt', 'ship_class': shipClass.toJson(), 'count': count, 'player_id': ?playerId};
}

class DefenceBuiltEvent extends EventKind {
  final DefenceKind defence;
  final int count;
  final int? playerId;

  const DefenceBuiltEvent({required this.defence, required this.count, this.playerId});

  factory DefenceBuiltEvent.fromJson(Object? json) {
    final m = _obj(json);
    return DefenceBuiltEvent(
      defence: DefenceKind.fromJson(m['defence']),
      count: _int(m['count']),
      playerId: _opt(m['player_id'], _int),
    );
  }

  @override
  Json toJson() => {'kind': 'DefenceBuilt', 'defence': defence.toJson(), 'count': count, 'player_id': ?playerId};
}

/// Result of an active fleet scan: sectors revealed plus faint signal
/// contacts detected one hop beyond scan range.
class ScanCompletedEvent extends EventKind {
  final int fleetId;

  /// The sector the scanning fleet is in. It is never one of the
  /// `signals`, which lie one hop beyond the scanned range.
  final Hex sector;
  final int sectorsRevealed;
  final int hostilesDetected;

  /// The threat of every revealed sector.
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

  factory ScanCompletedEvent.fromJson(Object? json) {
    final m = _obj(json);
    return ScanCompletedEvent(
      fleetId: _int(m['fleet_id']),
      sector: Hex.fromJson(m['sector']),
      sectorsRevealed: _int(m['sectors_revealed']),
      hostilesDetected: _int(m['hostiles_detected']),
      threats: _opt(m['threats'], (v) => _list(v, SectorThreat.fromJson)) ?? const [],
      signals: _opt(m['signals'], (v) => _list(v, SignalContact.fromJson)) ?? const [],
    );
  }

  @override
  Json toJson() => {
    'kind': 'ScanCompleted',
    'fleet_id': fleetId,
    'sector': sector.toJson(),
    'sectors_revealed': sectorsRevealed,
    'hostiles_detected': hostilesDetected,
    if (threats.isNotEmpty) 'threats': threats.map((e) => e.toJson()).toList(),
    if (signals.isNotEmpty) 'signals': signals.map((e) => e.toJson()).toList(),
  };
}

/// A faint contact at the edge of scan range — a hint, not full intel.
class SignalContact {
  final Hex sector;
  final SignalKind signal;

  /// Coarse danger of the sector: 1 for T1 to T3, 2 for T4 to T6, 3 for T7 to T9.
  final int threatBand;

  const SignalContact({required this.sector, required this.signal, required this.threatBand});

  factory SignalContact.fromJson(Object? json) {
    final m = _obj(json);
    return SignalContact(
      sector: Hex.fromJson(m['sector']),
      signal: SignalKind.fromJson(m['signal']),
      threatBand: _int(m['threat_band']),
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

enum SignalKind {
  richOre('RichOre', label: 'rich ore signature'),
  hostileMass('HostileMass', label: 'hostile mass reading'),
  derelict('Derelict', label: 'derelict transponder'),
  anomaly('Anomaly', label: 'anomalous emissions');

  const SignalKind(this.wire, {required this.label});

  final String wire;
  final String label;

  static SignalKind fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// Advance warning that a raid fleet is vectoring toward the homeworld, or
/// toward the player's outpost when `outpost` names its sector.
class RaidIncomingEvent extends EventKind {
  final int playerId;
  final int arrivalTick;

  /// Rough size read against the defence standing now: "light" (under
  /// half of it), "moderate" (under all of it) or "heavy".
  final String threat;

  /// The raid fleet's power, fixed at forecast time, on the same scale as
  /// `home_defence_power` and `OutpostState.defence_power`.
  final double estPower;

  /// The outpost the raid is bound for; absent for a raid on the homeworld.
  final Hex? outpost;

  const RaidIncomingEvent({
    required this.playerId,
    required this.arrivalTick,
    required this.threat,
    this.estPower = 0,
    this.outpost,
  });

  factory RaidIncomingEvent.fromJson(Object? json) {
    final m = _obj(json);
    return RaidIncomingEvent(
      playerId: _int(m['player_id']),
      arrivalTick: _int(m['arrival_tick']),
      threat: _string(m['threat']),
      estPower: _opt(m['est_power'], _double) ?? 0,
      outpost: _opt(m['outpost'], Hex.fromJson),
    );
  }

  @override
  Json toJson() => {
    'kind': 'RaidIncoming',
    'player_id': playerId,
    'arrival_tick': arrivalTick,
    'threat': threat,
    'est_power': estPower,
    'outpost': ?outpost?.toJson(),
  };
}

/// Outcome of a raid on the homeworld, or on the player's outpost when
/// `outpost` names its sector. An outpost has no structures and a repelled
/// raid on one leaves no salvage, so those fields are zero and absent then.
class RaidResolvedEvent extends EventKind {
  final int playerId;

  /// The outpost that was raided; absent for a raid on the homeworld.
  final Hex? outpost;
  final bool defended;

  /// The raid broke the outpost: it makes nothing, fills no tank and has
  /// no platform until it has repaired itself from its store. Always
  /// false for a raid on the homeworld.
  final bool breached;

  /// What a lost raid took: its share of the stockpile and of the cargo
  /// aboard fleets at home, counted as one pile above the protected
  /// floor. At an outpost, the same of its store and the fleets there.
  final Resources resourcesLost;
  final Resources? salvageDropped;

  /// Tick the pile on the homeworld despawns (absent when none dropped).
  /// A fleet docked at home scoops it into storage on its own; without
  /// one, an alert warns when it has `HOME_SALVAGE_WARN_TICKS` left.
  final int? salvageDespawnTick;
  final double raidPower;
  final double defensePower;

  /// Structures the raid destroyed.
  final int structuresLost;

  /// Of those, how many are rebuilt for free after a short delay.
  final int structuresRestored;

  /// What the Storage Vault kept out of reach of a successful raid, of
  /// the stockpile and the cargo aboard fleets at home together. At an
  /// outpost, what its protected floor kept.
  final Resources protectedKept;

  /// Ships docked at home, or standing at the outpost, that a lost raid
  /// destroyed.
  final int shipsLost;

  const RaidResolvedEvent({
    required this.playerId,
    this.outpost,
    required this.defended,
    this.breached = false,
    required this.resourcesLost,
    this.salvageDropped,
    this.salvageDespawnTick,
    required this.raidPower,
    required this.defensePower,
    this.structuresLost = 0,
    this.structuresRestored = 0,
    this.protectedKept = const Resources(),
    this.shipsLost = 0,
  });

  factory RaidResolvedEvent.fromJson(Object? json) {
    final m = _obj(json);
    return RaidResolvedEvent(
      playerId: _int(m['player_id']),
      outpost: _opt(m['outpost'], Hex.fromJson),
      defended: _bool(m['defended']),
      breached: _opt(m['breached'], _bool) ?? false,
      resourcesLost: Resources.fromJson(m['resources_lost']),
      salvageDropped: _opt(m['salvage_dropped'], Resources.fromJson),
      salvageDespawnTick: _opt(m['salvage_despawn_tick'], _int),
      raidPower: _double(m['raid_power']),
      defensePower: _double(m['defense_power']),
      structuresLost: _opt(m['structures_lost'], _int) ?? 0,
      structuresRestored: _opt(m['structures_restored'], _int) ?? 0,
      protectedKept: _opt(m['protected_kept'], Resources.fromJson) ?? const Resources(),
      shipsLost: _opt(m['ships_lost'], _int) ?? 0,
    );
  }

  @override
  Json toJson() => {
    'kind': 'RaidResolved',
    'player_id': playerId,
    'outpost': ?outpost?.toJson(),
    'defended': defended,
    'breached': breached,
    'resources_lost': resourcesLost.toJson(),
    'salvage_dropped': ?salvageDropped?.toJson(),
    'salvage_despawn_tick': ?salvageDespawnTick,
    'raid_power': raidPower,
    'defense_power': defensePower,
    'structures_lost': structuresLost,
    'structures_restored': structuresRestored,
    'protected_kept': protectedKept.toJson(),
    'ships_lost': shipsLost,
  };
}

/// A message for the player. `player_id` names the recipient; `None` is a
/// server-wide notice. Losing a fleet raises a `Critical` alert.
class AlertEvent extends EventKind {
  final int? playerId;
  final AlertLevel level;
  final String message;
  final Hex? sector;
  final int? fleetId;

  const AlertEvent({this.playerId, required this.level, required this.message, this.sector, this.fleetId});

  factory AlertEvent.fromJson(Object? json) {
    final m = _obj(json);
    return AlertEvent(
      playerId: _opt(m['player_id'], _int),
      level: AlertLevel.fromJson(m['level']),
      message: _string(m['message']),
      sector: _opt(m['sector'], Hex.fromJson),
      fleetId: _opt(m['fleet_id'], _int),
    );
  }

  @override
  Json toJson() => {
    'kind': 'Alert',
    'player_id': ?playerId,
    'level': level.toJson(),
    'message': message,
    'sector': ?sector?.toJson(),
    'fleet_id': ?fleetId,
  };
}

enum AlertLevel {
  info('Info'),
  warning('Warning'),
  critical('Critical');

  const AlertLevel(this.wire);

  final String wire;

  static AlertLevel fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

class ErrorMessage extends ServerMessage {
  final ErrorCode code;
  final String message;

  const ErrorMessage({required this.code, required this.message});

  factory ErrorMessage.fromJson(Object? json) {
    final m = _obj(json);
    return ErrorMessage(code: ErrorCode.fromJson(m['code']), message: _string(m['message']));
  }

  @override
  Json toJson() => {'type': 'error', 'code': code.toJson(), 'message': message};
}

enum ErrorCode {
  invalidCommand('InvalidCommand', code: 1000),
  invalidTarget('InvalidTarget', code: 1001),
  noConnection('NoConnection', code: 1002),
  insufficientFuel('InsufficientFuel', code: 1003),
  onCooldown('OnCooldown', code: 1004),
  fleetNotFound('FleetNotFound', code: 1005),
  notInSector('NotInSector', code: 1006),
  noResources('NoResources', code: 1007),
  cargoFull('CargoFull', code: 1008),
  prerequisitesNotMet('PrerequisitesNotMet', code: 1009),
  maxLevelReached('MaxLevelReached', code: 1010),
  queueFull('QueueFull', code: 1011),
  shipLocked('ShipLocked', code: 1012),
  noShipyard('NoShipyard', code: 1013),
  noResearchLab('NoResearchLab', code: 1014),
  fleetLimitReached('FleetLimitReached', code: 1015),
  resourceNotPresent('ResourceNotPresent', code: 1016),
  storageTooSmall('StorageTooSmall', code: 1017),
  defenceLocked('DefenceLocked', code: 1018),
  insufficientResources('InsufficientResources', code: 1019),
  authFailed('AuthFailed', code: 2000),
  alreadyAuthenticated('AlreadyAuthenticated', code: 2001),
  invalidName('InvalidName', code: 2002),
  tokenRequired('TokenRequired', code: 2003),
  invalidToken('InvalidToken', code: 2004),
  worldFull('WorldFull', code: 2005),
  serverError('ServerError', code: 5000);

  const ErrorCode(this.wire, {required this.code});

  final String wire;
  final int code;

  static ErrorCode fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// Resource density levels.
enum Density {
  none('None', label: 'None'),
  sparse('Sparse', label: 'Sparse'),
  moderate('Moderate', label: 'Moderate'),
  rich('Rich', label: 'Rich'),
  pristine('Pristine', label: 'Pristine');

  const Density(this.wire, {required this.label});

  final String wire;
  final String label;

  static Density fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// Terrain types for sectors.
enum TerrainType {
  empty('Empty', label: 'Empty Space'),
  asteroidField('AsteroidField', label: 'Asteroid Field'),
  nebula('Nebula', label: 'Nebula'),
  debrisField('DebrisField', label: 'Debris Field'),
  anomaly('Anomaly', label: 'Anomaly');

  const TerrainType(this.wire, {required this.label});

  final String wire;
  final String label;

  static TerrainType fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// Ship class definitions.
enum ShipClass {
  scout('Scout', label: 'Scout', hull: 30, shield: 10, weapon: 5, cargo: 20, power: 9),
  corvette('Corvette', label: 'Corvette', hull: 50, shield: 20, weapon: 15, cargo: 10, power: 22),
  frigate('Frigate', label: 'Frigate', hull: 120, shield: 60, weapon: 30, cargo: 30, power: 48),
  cruiser('Cruiser', label: 'Cruiser', hull: 200, shield: 100, weapon: 80, cargo: 50, power: 110),
  hauler('Hauler', label: 'Hauler', hull: 80, shield: 20, weapon: 5, cargo: 200, power: 15);

  const ShipClass(
    this.wire, {
    required this.label,
    required this.hull,
    required this.shield,
    required this.weapon,
    required this.cargo,
    required this.power,
  });

  final String wire;
  final String label;
  final double hull;
  final double shield;
  final double weapon;
  final double cargo;
  final double power;

  static ShipClass fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

class Resources {
  final double metal;
  final double crystal;
  final double deuterium;

  const Resources({this.metal = 0, this.crystal = 0, this.deuterium = 0});

  factory Resources.fromJson(Object? json) {
    final m = _obj(json);
    return Resources(metal: _double(m['metal']), crystal: _double(m['crystal']), deuterium: _double(m['deuterium']));
  }

  Json toJson() => {'metal': metal, 'crystal': crystal, 'deuterium': deuterium};
}

/// One of the three stockpiled resources.
enum ResourceKind {
  metal('metal', label: 'metal', weight: 1),
  crystal('crystal', label: 'crystal', weight: 1.5),
  deuterium('deuterium', label: 'deuterium', weight: 2);

  const ResourceKind(this.wire, {required this.label, required this.weight});

  final String wire;
  final String label;
  final double weight;

  static ResourceKind fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

enum BuildingType {
  metalMine('MetalMine', label: 'Metal Mine'),
  crystalMine('CrystalMine', label: 'Crystal Mine'),
  deuteriumSynthesizer('DeuteriumSynthesizer', label: 'Deut Synthesizer'),
  shipyard('Shipyard', label: 'Shipyard'),
  researchLab('ResearchLab', label: 'Research Lab'),
  fuelDepot('FuelDepot', label: 'Fuel Depot'),
  sensorArray('SensorArray', label: 'Sensor Array'),
  defenseGrid('DefenseGrid', label: 'Defense Grid'),
  storageVault('StorageVault', label: 'Storage Vault'),
  fabricator('Fabricator', label: 'Fabricator');

  const BuildingType(this.wire, {required this.label});

  final String wire;
  final String label;

  static BuildingType fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

enum ResearchType {
  fuelEfficiency('FuelEfficiency', label: 'Fuel Efficiency'),
  extendedFuelTanks('ExtendedFuelTanks', label: 'Extended Tanks'),
  reinforcedHulls('ReinforcedHulls', label: 'Reinforced Hulls'),
  advancedShields('AdvancedShields', label: 'Advanced Shields'),
  weaponsResearch('WeaponsResearch', label: 'Weapons Research'),
  navigation('Navigation', label: 'Navigation'),
  harvestingEfficiency('HarvestingEfficiency', label: 'Harvesting Eff.'),
  corvetteTech('CorvetteTech', label: 'Corvette Tech'),
  frigateTech('FrigateTech', label: 'Frigate Tech'),
  cruiserTech('CruiserTech', label: 'Cruiser Tech'),
  haulerTech('HaulerTech', label: 'Hauler Tech'),
  emergencyJump('EmergencyJump', label: 'Emergency Jump'),
  modularFabrication('ModularFabrication', label: 'Modular Fabrication');

  const ResearchType(this.wire, {required this.label});

  final String wire;
  final String label;

  static ResearchType fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// Home defences built from the shipyard queue. They never move, burn no
/// fuel, and a raid can wear them down.
enum DefenceKind {
  pulseTurret('PulseTurret', label: 'Pulse Turret'),
  lancerBattery('LancerBattery', label: 'Lancer Battery'),
  ionBastion('IonBastion', label: 'Ion Bastion');

  const DefenceKind(this.wire, {required this.label});

  final String wire;
  final String label;

  static DefenceKind fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// What the shipyard queue builds: a ship class or a defence structure.
/// On the wire it is just the name ("Corvette", "PulseTurret").
class ShipyardItem {
  final ShipClass? ship;
  final DefenceKind? defence;

  const ShipyardItem.ship(ShipClass this.ship) : defence = null;
  const ShipyardItem.defence(DefenceKind this.defence) : ship = null;

  factory ShipyardItem.fromJson(Object? json) {
    for (final v in ShipClass.values) {
      if (v.wire == json) return ShipyardItem.ship(v);
    }
    for (final v in DefenceKind.values) {
      if (v.wire == json) return ShipyardItem.defence(v);
    }
    throw FormatException('unknown ShipyardItem "$json"');
  }

  String toJson() => ship?.toJson() ?? defence!.toJson();

  List<Object?> get _props => [ship, defence];

  @override
  bool operator ==(Object other) => other is ShipyardItem && _same(_props, other._props);

  @override
  int get hashCode => Object.hashAll(_props);
}

/// How a fleet is expected to fare against what waits in a sector. The ratio
/// is `outcome_ratio` of the fight played on averages (`iac_sim::odds`), so it
/// counts rapid fire, shot size against hull and shield, and who fires first;
/// between equal ship classes it is close to power against power. Damage
/// exchange is quadratic in fleet size, so the bands sit close together
/// around 1.0 and a fight is decided long before the ratio reaches 2.
enum RatioLabel {
  deadly('deadly', label: 'DEADLY'),
  risky('risky', label: 'RISKY'),
  even('even', label: 'EVEN'),
  favourable('favourable', label: 'FAVOURABLE'),
  safe('safe', label: 'SAFE');

  const RatioLabel(this.wire, {required this.label});

  final String wire;
  final String label;

  static RatioLabel fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// What a sector of the hub offers.
enum HubService {
  /// The NPC exchange: a fleet here trades one resource in its hold for another.
  exchange('exchange');

  const HubService(this.wire);

  final String wire;

  static HubService fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// Start of every built-in bot's account name, reserved for them
/// (compared without regard to case).
const botNamePrefix = 'BOT-';

/// Most sectors one `odds` request may name.
const maxOddsSectors = 256;

/// Ratio at and above which a fight is SAFE: the fleet keeps 98 percent of its hull or more.
const double ratioSafe = 2.5;

/// FAVOURABLE from here: a sure win that costs about 1 to 15 percent of the hull.
const double ratioFavourable = 1.5;

/// EVEN from here: a win in practice, but 15 to 50 percent of the hull is lost.
const double ratioEven = 1.100000023841858;

/// RISKY from here: anything from a lost fleet to a win that keeps half the hull.
/// Below it (DEADLY) the fleet is lost, or wins one fight in six at best.
const double ratioRisky = 0.8999999761581421;

/// Outposts (`docs/design/outposts.md`). Every number here is a proposal the
/// owner has not signed off. Change it here only; the simulation reads it
/// from `numbers.json`. None of them is a duration; the build and
/// repair times follow from `OUTPOST_BUILD_RATE` (economy class), the store
/// from `OUTPOST_STORE_BASE` (finds class, like the holds that fill it) and
/// the yield from `OUTPOST_YIELD_PER_HOUR` (economy class). Costs, the
/// platform and the raid do not scale with pace.
/// The rings an outpost may stand in.
const outpostMinRing = 9;

const outpostMaxRing = 20;

/// The most characters an account name may have.
const nameMaxLen = 24;

/// The last ring of the Inner Ring.
const zoneInnerRingRadius = 8;

/// The last ring of the Outer Ring; The Wandering starts one ring further out.
const zoneOuterRingRadius = 20;

/// The highest threat rating.
const maxThreat = 9;

/// The grade of a site whose three ores are all Pristine: the most a grade can be.
const double siteGradeMax = 12;

/// How far along the lanes the engine looks for the way between two sectors:
/// a fleet's way home or to its outpost, the hops from an outpost or a
/// surveyed site to the homeworld. A way longer than this is not found.
const laneSearchHops = 64;

/// `StorageNearCap` fires at this fill ratio.
const double storageNearCapRatio = 0.8500000238418579;

/// Active scan: hops revealed from the scanning fleet's position.
/// Fleets carrying a Scout get the extended range; signals are faint
/// contacts detected one hop beyond the reveal radius.
const scanBaseRange = 1;

/// The scan range of a fleet with a Scout aboard.
const scanScoutRange = 2;

/// Ticks a fleet takes no order after a scan.
const scanCooldown = 5;

/// Ticks a boarding takes, at every pace.
const exploreDurationTicks = 20;

/// The bounds `policy_update` holds the other doctrine thresholds to.
const policyMinFuelPctMax = 90;

const policyCargoReturnPctMin = 10;

const policyCargoReturnPctMax = 100;

const policyRangeMin = 1;

const policyRangeMax = 30;

const policyEngageRatioX10Min = 1;

const policyEngageRatioX10Max = 100;

/// At or above this `fuel_budget_pct` the fleet refuels without a bound; it
/// is also the most the setting can be.
const policyFuelUnboundedPct = 100;

/// Deuterium paid per fuel unit topped up at the homeworld. Not scaled by
/// pace: fuel is the price of range, not an economy timer.
const double fuelDeutPerUnit = 0.07999999821186066;

/// The share of the dock's rate that `refine` loses: deuterium turned into
/// fuel from a fleet's hold yields `scaling::refined_fuel_per_deuterium`.
/// Provisional, and the owner's ("like 15%"): it trades the reach of a fleet
/// with cargo ships against the worth of the docks as places to fill up.
const double refineLoss = 0.15000000596046448;
