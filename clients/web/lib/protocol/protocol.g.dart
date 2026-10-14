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

/// Jump to the connected sector `target`. The fuel is paid at once and
/// the jump charges for the fleet's `jump_ticks`, set by its slowest
/// ship.
///
/// From inside a fight it is flight (`FleetState.flight` quotes it): the
/// fleet stops firing and is fired on every round until it is gone, one
/// round fewer than its jump takes ticks and never fewer than
/// `FLEE_MIN_ROUNDS`. `stop` calls the jump off, the fuel is forfeit and
/// the fleet fights on. When the last fleet has left, a group that was
/// beating it may follow (`FleetState.pursued_by`).
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

/// Order `count` defences in the shipyard queue; same rule as
/// `build_ship`. They stay at home and never burn fuel; rejected with
/// `DefenceLocked` until the Shipyard and research it needs are in
/// place, and with `MaxLevelReached` for a second dome of a kind.
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

/// Detach `ships`, so many of each class named, from `fleet_id` into a
/// new fleet at the same sector. Whole ships of a class go first and
/// damaged ones only when the whole run out; with `damaged_first` the
/// most damaged go first, which is how the wounded are sent home. The
/// new fleet starts with no standing orders, fights with the levels of
/// the fleet it left (`FleetState.refit`) and takes a share of fuel and
/// cargo proportional to the tank and hold it carries off. At least one
/// ship must stay behind. The fleet must be idle. Rejected with
/// `FleetLimitReached` at the fleet cap, `InvalidTarget` for a class
/// named twice, a count of zero, more ships of a class than the fleet
/// has, or an attempt to take every ship. At the homeworld this is also
/// how docked ships are launched as their own fleet. The reply is an
/// `Alert` (info) event naming the new fleet id.
class SplitCommand extends Command {
  final int fleetId;
  final List<ShipCount> ships;
  final bool damagedFirst;

  const SplitCommand({required this.fleetId, required this.ships, this.damagedFirst = false});

  factory SplitCommand.fromJson(Object? json) {
    final m = _obj(json);
    return SplitCommand(
      fleetId: _int(m['fleet_id']),
      ships: _list(m['ships'], ShipCount.fromJson),
      damagedFirst: _opt(m['damaged_first'], _bool) ?? false,
    );
  }

  @override
  Json toJson() => {
    'action': 'split',
    'fleet_id': fleetId,
    'ships': ships.map((e) => e.toJson()).toList(),
    'damaged_first': damagedFirst,
  };
}

/// Fold `other_fleet_id` into `fleet_id`. Both must be yours, idle, and in
/// the same sector (`NotInSector` otherwise). Ships, cargo and fuel are
/// pooled (fuel up to the combined tank), the merged fleet keeps the
/// standing orders of `fleet_id`, and `other_fleet_id` ceases to exist.
/// Away from home it fights with the lower of the two fleets' Weapons,
/// Shielding and Armour levels until it docks at home. Refused with
/// `InvalidCommand` when the two hold more than `MAX_FLEET_SHIPS` ships
/// between them. This works anywhere, so a fresh fleet can rescue a
/// stranded one.
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

  /// Leave a fight once the fleet's odds in it fall below this / 10. Every
  /// `POLICY_EVAL_INTERVAL` ticks of a fight the odds are read again from
  /// the two sides as they then stand, and below the ratio the fleet jumps
  /// out toward home under fire, as a `move` out of a fight does, and
  /// reports it as a `PolicyAction` `flee` with the odds. Whatever doctrine
  /// it flies, and however it came into the fight. Clamped to 1..=100 and
  /// never above `engage_ratio_x10`. Default 9 (0.9x, the edge of RISKY).
  final int disengageRatioX10;

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
    this.disengageRatioX10 = 9,
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
      disengageRatioX10: _opt(m['disengage_ratio_x10'], _int) ?? 9,
      fuelBudgetPct: _opt(m['fuel_budget_pct'], _int) ?? 20,
      refine: _opt(m['refine'], _bool) ?? false,
    );
  }

  Json toJson() => {
    'min_fuel_pct': minFuelPct,
    'cargo_return_pct': cargoReturnPct,
    'max_range': maxRange,
    'engage_ratio_x10': engageRatioX10,
    'disengage_ratio_x10': disengageRatioX10,
    'fuel_budget_pct': fuelBudgetPct,
    'refine': refine,
  };

  List<Object?> get _props => [
    minFuelPct,
    cargoReturnPct,
    maxRange,
    engageRatioX10,
    disengageRatioX10,
    fuelBudgetPct,
    refine,
  ];

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

  /// The defences the platform is at level 1.
  final List<UnitCount> platform;

  /// The raid of this ring at its typical power against that platform
  /// alone: what it flies and how the fight is expected to end.
  final RaidForecast? raid;

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
    this.platform = const [],
    this.raid,
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
      platform: _opt(m['platform'], (v) => _list(v, UnitCount.fromJson)) ?? const [],
      raid: _opt(m['raid'], RaidForecast.fromJson),
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
    'platform': platform.map((e) => e.toJson()).toList(),
    'raid': ?raid?.toJson(),
    'refusal': ?refusal,
  };
}

/// A raid on the homeworld or on the outpost, announced or expected, and how
/// it would go against what stands there now.
///
/// A raid is a battle (`BattleRound`, one a tick): the raiders against every
/// ship of the owner standing there and, at home, every defence and Solar
/// Satellite, or at the outpost its platform. It is fought for at most
/// `RAID_ROUNDS` rounds. It is repelled when no raider is left or when the
/// rounds are over and a defender still stands; it is lost only when no
/// defender is left.
class RaidForecast {
  /// The tick an announced raid arrives. Absent while none is on its way
  /// and while one is being fought: with neither, `power` and `ships` are
  /// those of a raid of typical size were one announced now.
  final int? arrivalTick;

  /// The raid has arrived and is being fought: the rounds still to fight
  /// at the most, as `BattleState.raid` of the fight says them. `power`
  /// and `ships` are then what it flew in with, and `odds` how the fight
  /// would end from where it stands. Absent otherwise.
  final int? roundsLeft;

  /// The raid's power on the scale `FleetState.power` uses. It is fixed
  /// when the raid is announced.
  final double power;

  /// What it flies, by class in catalogue order. Fixed when announced.
  final List<ShipCount> ships;

  /// How it would end against what stands in its way this tick.
  final RaidOdds odds;

  const RaidForecast({this.arrivalTick, this.roundsLeft, required this.power, required this.ships, required this.odds});

  factory RaidForecast.fromJson(Object? json) {
    final m = _obj(json);
    return RaidForecast(
      arrivalTick: _opt(m['arrival_tick'], _int),
      roundsLeft: _opt(m['rounds_left'], _int),
      power: _double(m['power']),
      ships: _list(m['ships'], ShipCount.fromJson),
      odds: RaidOdds.fromJson(m['odds']),
    );
  }

  Json toJson() => {
    'arrival_tick': ?arrivalTick,
    'rounds_left': ?roundsLeft,
    'power': power,
    'ships': ships.map((e) => e.toJson()).toList(),
    'odds': odds.toJson(),
  };
}

/// How a raid would end against what stands in its way now: the server
/// plays the fight itself, the round a raid is fought by, a few times over
/// with fixed dice for `RAID_ROUNDS` rounds, as `SectorOdds` are worked out.
/// `destroyed`, `outlasted` and `lost` are the shares of the fights played
/// that ended each way and add up to 1.
class RaidOdds {
  final RaidLabel label;

  /// No raider was left: the raid is repelled and leaves wreckage.
  final double destroyed;

  /// The rounds were over and a defender still stood: the raid is
  /// repelled and leaves nothing.
  final double outlasted;

  /// No defender was left: the raid takes its share of the stockpile.
  final double lost;

  /// The share of their worth the defenders are expected to keep, before
  /// any defence or Solar Satellite is rebuilt, and the share of theirs
  /// the raiders keep.
  final double kept;
  final double raidersKept;

  const RaidOdds({
    required this.label,
    required this.destroyed,
    required this.outlasted,
    required this.lost,
    required this.kept,
    required this.raidersKept,
  });

  factory RaidOdds.fromJson(Object? json) {
    final m = _obj(json);
    return RaidOdds(
      label: RaidLabel.fromJson(m['label']),
      destroyed: _double(m['destroyed']),
      outlasted: _double(m['outlasted']),
      lost: _double(m['lost']),
      kept: _double(m['kept']),
      raidersKept: _double(m['raiders_kept']),
    );
  }

  Json toJson() => {
    'label': label.toJson(),
    'destroyed': destroyed,
    'outlasted': outlasted,
    'lost': lost,
    'kept': kept,
    'raiders_kept': raidersKept,
  };
}

/// What `RaidOdds` come to, from the worst for the defender to the best.
enum RaidLabel {
  /// Lost in every fight played, or nothing stands in its way.
  lost('lost', label: 'LOST', outcome: 'nothing here stands through the fight: the raid takes its share'),

  /// Lost in some of the fights played.
  atRisk('at_risk', label: 'AT RISK', outcome: 'the fight may leave no defender standing'),

  /// Lost in none, and not destroyed in every one: the defence lasts the
  /// rounds out.
  held('held', label: 'HELD', outcome: 'a defender stands when the raiders give up; what falls before that is lost'),

  /// The raiders were destroyed in every fight played.
  repelled('repelled', label: 'REPELLED', outcome: 'the raiders are destroyed and leave their wreckage');

  const RaidLabel(this.wire, {required this.label, required this.outcome});

  final String wire;
  final String label;
  final String outcome;

  static RaidLabel fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
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

/// The score table: one ranking for every player, human or agent. A score
/// is what an empire has built and keeps:
/// `score = buildings + research + fleet + outpost + defence`, each part
/// the cost of what stands, weighted 1 : 1.5 : 2 for
/// metal : crystal : deuterium, per thousand. `fleet` is the ships the
/// empire has now, its Solar Satellites among them, so a fleet destroyed
/// leaves the score; `defence` is the defence structures at the share of
/// their cost the score gives them (`iac_sim::score::DEFENCE_SHARE`, none
/// today). Kills, charts and derelicts score nothing: they pay in resources.
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
  final double buildings;
  final double research;
  final double fleet;

  /// The finished levels of the empire's outpost, while it stands.
  final double outpost;
  final double defence;

  const LeaderboardEntry({
    required this.rank,
    required this.name,
    required this.agent,
    required this.score,
    required this.buildings,
    required this.research,
    required this.fleet,
    required this.outpost,
    required this.defence,
  });

  factory LeaderboardEntry.fromJson(Object? json) {
    final m = _obj(json);
    return LeaderboardEntry(
      rank: _int(m['rank']),
      name: _string(m['name']),
      agent: _bool(m['agent']),
      score: _double(m['score']),
      buildings: _double(m['buildings']),
      research: _double(m['research']),
      fleet: _double(m['fleet']),
      outpost: _double(m['outpost']),
      defence: _double(m['defence']),
    );
  }

  Json toJson() => {
    'rank': rank,
    'name': name,
    'agent': agent,
    'score': score,
    'buildings': buildings,
    'research': research,
    'fleet': fleet,
    'outpost': outpost,
    'defence': defence,
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

  /// The fleet is in a fight, so this jump is flight: what it costs, and
  /// whether a group would follow it to `target`. Absent outside a fight.
  final FlightQuote? flight;

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
    this.flight,
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
      flight: _opt(m['flight'], FlightQuote.fromJson),
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
    'flight': ?flight?.toJson(),
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
/// The server plays the fight itself, the round a fight is fought by, a few
/// times over with fixed dice and the two sides as they stand, damage and
/// refit included, both scaled down together when they are large. `ratio`
/// puts the share of its worth each side kept on the scale of a power ratio
/// between equal ship classes (1.0 is an even match, 0.1 to 10), and `label`
/// is its band (`RatioLabel`): safe from 2.5, favourable from 1.5, even
/// from 1.1, risky from 0.9, below that deadly. A sector with nothing in
/// sight is rated against the group its ring spawns. Standing orders use
/// the same number (`engage_ratio_x10`).
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

  /// Every fight under way that the player has a fleet in or is present
  /// at, whole, every tick, as in `GameState.battles` but without
  /// `last_round`: the round fought this tick is the `BattleRound` event
  /// of the same update. Left out when there is none: a client replaces
  /// its list with this one.
  final List<BattleState> battles;
  final List<GameEvent>? events;

  const TickUpdate({
    required this.tick,
    this.player,
    required this.fleets,
    this.sectorUpdates,
    this.homeworldUpdate,
    this.outpost,
    this.battles = const [],
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
      battles: _opt(m['battles'], (v) => _list(v, BattleState.fromJson)) ?? const [],
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
    if (battles.isNotEmpty) 'battles': battles.map((e) => e.toJson()).toList(),
    'events': ?events?.map((e) => e.toJson()).toList(),
  };
}

/// Full snapshot, sent on login and on `request_full_state`.
///
/// `known_sectors` is the player's whole chart: every sector the player has
/// ever had eyes on (fleet present, the homeworld's sensors, active scan), not only the
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

  /// The fights under way that the player has a fleet in or is present
  /// at, each with its last round: what a client that arrives in the
  /// middle of one has missed. The rounds before that are not kept.
  /// A raid being fought at the player's homeworld or outpost is one of
  /// them, with `raid` set. Left out when there is none.
  final List<BattleState> battles;
  final WorldInfo world;

  const GameState({
    required this.tick,
    required this.player,
    required this.fleets,
    required this.homeworld,
    this.outpost,
    required this.knownSectors,
    this.battles = const [],
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
      battles: _opt(m['battles'], (v) => _list(v, BattleState.fromJson)) ?? const [],
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
    if (battles.isNotEmpty) 'battles': battles.map((e) => e.toJson()).toList(),
    'world': world.toJson(),
  };
}

/// A fight under way, as `GameState.battles` and `TickUpdate.battles` carry it.
class BattleState {
  final Hex sector;

  /// Rounds fought so far.
  final int rounds;

  /// The last of them, in a full state; absent before the first is
  /// fought, and in a tick update, whose events carry the round.
  final BattleRoundEvent? lastRound;

  /// What each fleet and group has lost so far, as `CombatEnded` will say.
  final List<BattleLosses> losses;

  /// The fight is a raid on a homeworld or an outpost; absent for a fight
  /// in open space.
  final RaidBattle? raid;

  const BattleState({required this.sector, required this.rounds, this.lastRound, required this.losses, this.raid});

  factory BattleState.fromJson(Object? json) {
    final m = _obj(json);
    return BattleState(
      sector: Hex.fromJson(m['sector']),
      rounds: _int(m['rounds']),
      lastRound: _opt(m['last_round'], BattleRoundEvent.fromJson),
      losses: _list(m['losses'], BattleLosses.fromJson),
      raid: _opt(m['raid'], RaidBattle.fromJson),
    );
  }

  Json toJson() => {
    'sector': sector.toJson(),
    'rounds': rounds,
    'last_round': ?lastRound?.toJson(),
    'losses': losses.map((e) => e.toJson()).toList(),
    'raid': ?raid?.toJson(),
  };
}

/// A raid that has arrived and is being fought.
class RaidBattle {
  /// The empire raided.
  final String owner;

  /// It is the receiver's own homeworld or outpost.
  final bool mine;

  /// The raid is on the owner's outpost, not the homeworld.
  final bool outpost;

  /// The power it was announced with.
  final double power;

  /// What it flew in with, by class.
  final List<ShipCount> ships;

  /// Rounds still to fight at the most: the raiders leave after the last.
  final int roundsLeft;

  /// How the fight would end from where it stands, over the rounds left.
  final RaidOdds odds;

  const RaidBattle({
    required this.owner,
    this.mine = false,
    required this.outpost,
    required this.power,
    required this.ships,
    required this.roundsLeft,
    required this.odds,
  });

  factory RaidBattle.fromJson(Object? json) {
    final m = _obj(json);
    return RaidBattle(
      owner: _string(m['owner']),
      mine: _opt(m['mine'], _bool) ?? false,
      outpost: _bool(m['outpost']),
      power: _double(m['power']),
      ships: _list(m['ships'], ShipCount.fromJson),
      roundsLeft: _int(m['rounds_left']),
      odds: RaidOdds.fromJson(m['odds']),
    );
  }

  Json toJson() => {
    'owner': owner,
    'mine': mine,
    'outpost': outpost,
    'power': power,
    'ships': ships.map((e) => e.toJson()).toList(),
    'rounds_left': roundsLeft,
    'odds': odds.toJson(),
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
/// (`RaidIncoming` and `RaidResolved` carry `outpost`), and the raid is a
/// battle as a raid on the homeworld is (`RaidForecast`): the raiders
/// against the `platform` and every ship of the owner standing there. A
/// lost raid, one that left no defender, takes a quarter of what lies above
/// `protected` in the store and breaches the outpost. No raid destroys it,
/// the platform stands whole again after every fight it is not breached
/// by, and a fleet standing there is in the fight.
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
  /// (1.5 and 2).
  final Resources protected;

  /// What the extractor adds to the store each tick now; zero while not
  /// `in_service`.
  final Resources production;

  /// The resources the extractor makes and the store holds all it takes
  /// of: that yield is lost until a fleet loads some.
  final List<ResourceKind> yieldLost;

  /// The site's grade, 0 to 12; see `SurveySite.grade`.
  final double grade;

  /// What the platform adds up to on the power scale; 0 while breached or
  /// unfinished.
  final double platformPower;

  /// The defences the platform is; empty while breached or unfinished.
  final List<UnitCount> platform;

  /// What the owner's ships standing in the sector add up to.
  final double shipsPower;

  /// `platform_power` and `ships_power` together. It is a sum on the
  /// power scale and decides nothing: `raid.odds` is how a raid would go.
  final double defencePower;

  /// The least and the most a raid on this outpost can be.
  final PowerRange raidPower;

  /// Tick an announced raid arrives; absent when none is on its way.
  final int? raidArrivalTick;

  /// The raid being fought, else the one announced, else one of typical
  /// size for the ring, against the platform and the ships standing there
  /// now. Absent before level 1 is finished: nothing raids a building
  /// site.
  final RaidForecast? raid;

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
    this.platform = const [],
    required this.shipsPower,
    required this.defencePower,
    required this.raidPower,
    this.raidArrivalTick,
    this.raid,
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
      platform: _opt(m['platform'], (v) => _list(v, UnitCount.fromJson)) ?? const [],
      shipsPower: _double(m['ships_power']),
      defencePower: _double(m['defence_power']),
      raidPower: PowerRange.fromJson(m['raid_power']),
      raidArrivalTick: _opt(m['raid_arrival_tick'], _int),
      raid: _opt(m['raid'], RaidForecast.fromJson),
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
    'platform': platform.map((e) => e.toJson()).toList(),
    'ships_power': shipsPower,
    'defence_power': defencePower,
    'raid_power': raidPower.toJson(),
    'raid_arrival_tick': ?raidArrivalTick,
    'raid': ?raid?.toJson(),
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

  /// The defences the platform is at this level.
  final List<UnitCount> platform;

  /// What the extractor makes each tick at this level, on this site.
  final Resources production;

  const OutpostLevel({
    required this.level,
    required this.cost,
    required this.ticks,
    required this.storeCap,
    required this.platformPower,
    this.platform = const [],
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
      platform: _opt(m['platform'], (v) => _list(v, UnitCount.fromJson)) ?? const [],
      production: Resources.fromJson(m['production']),
    );
  }

  Json toJson() => {
    'level': level,
    'cost': cost.toJson(),
    'ticks': ticks,
    'store_cap': storeCap.toJson(),
    'platform_power': platformPower,
    'platform': platform.map((e) => e.toJson()).toList(),
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

  /// Every rapid-fire figure of the catalogue: which ship class fires
  /// again at what, ships and defences alike. It never changes.
  final List<RapidFire> rapidFire;

  const WorldInfo({
    required this.pace,
    this.preset,
    required this.tickHz,
    required this.economyVersion,
    required this.worldgenVersion,
    this.estimate,
    this.hub = const [],
    this.rapidFire = const [],
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
      rapidFire: _opt(m['rapid_fire'], (v) => _list(v, RapidFire.fromJson)) ?? const [],
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
    'rapid_fire': rapidFire.map((e) => e.toJson()).toList(),
  };
}

/// After a shot at a unit of `against`, a ship of `ship_class` fires again
/// with probability `(shots - 1) / shots`.
class RapidFire {
  final ShipClass shipClass;

  /// A ship class or a defence: the wire carries just its name.
  final ShipyardItem against;
  final int shots;

  const RapidFire({required this.shipClass, required this.against, required this.shots});

  factory RapidFire.fromJson(Object? json) {
    final m = _obj(json);
    return RapidFire(
      shipClass: ShipClass.fromJson(m['class']),
      against: ShipyardItem.fromJson(m['against']),
      shots: _int(m['shots']),
    );
  }

  Json toJson() => {'class': shipClass.toJson(), 'against': against.toJson(), 'shots': shots};
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

  /// Every ship of every fleet the player has, docked or away.
  final int shipCount;

  /// Where the player's meter at the hub's exchange stands this tick.
  final ExchangeMeter exchange;

  const PlayerState({
    required this.id,
    required this.name,
    required this.resources,
    required this.homeworld,
    required this.shipCount,
    this.exchange = const ExchangeMeter(),
  });

  factory PlayerState.fromJson(Object? json) {
    final m = _obj(json);
    return PlayerState(
      id: _int(m['id']),
      name: _string(m['name']),
      resources: Resources.fromJson(m['resources']),
      homeworld: Hex.fromJson(m['homeworld']),
      shipCount: _int(m['ship_count']),
      exchange: _opt(m['exchange'], ExchangeMeter.fromJson) ?? const ExchangeMeter(),
    );
  }

  Json toJson() => {
    'id': id,
    'name': name,
    'resources': resources.toJson(),
    'homeworld': homeworld.toJson(),
    'ship_count': shipCount,
    'exchange': exchange.toJson(),
  };
}

class FleetState {
  final int id;
  final Hex location;
  final FleetStatus state;

  /// Its ships by class, in catalogue order; a class it has none of is
  /// left out. A fleet is a count for each class and the hull of its
  /// damaged ships: no ship has an id.
  final List<ShipGroupState> ships;

  /// Every ship of every class.
  final int shipCount;

  /// Hull the whole fleet holds, and at full. Hull lost stays lost until a
  /// dock mends it (`repair`).
  final int hull;
  final int hullMax;

  /// What the fleet fires in one round, and the shield it raises at the
  /// start of each, with `refit` counted.
  final int attack;
  final int shield;

  /// The Weapons, Shielding and Armour Technology the fleet fights with:
  /// the levels it last docked at home with. Research that finishes while
  /// it is away reaches it when it next docks there, not at the outpost.
  final Refit refit;

  /// The owner's levels are above `refit`: docking at home would raise it.
  final bool refitDue;

  /// What mending its damaged ships would take; absent when none is damaged.
  final RepairQuote? repair;

  /// The fleet is in a fight and firing: what a `move` out of it would
  /// cost right now, and why it would be refused if it would. Absent
  /// outside a fight and once the fleet is leaving one.
  final FlightQuote? flight;

  /// The fleet is charging a jump while hostiles fire on it: it does not
  /// answer, and stays a target until it is gone. Its `state` is `Moving`.
  final Fleeing? fleeing;

  /// A hostile group is following the fleet and jumping to where it stands.
  final Pursuit? pursuedBy;
  final Resources cargo;

  /// Hold size: the most `cargo` can total, as the server enforces it.
  final double cargoCapacity;
  final double fuel;
  final double fuelMax;

  /// Fuel one jump costs this fleet right now: the fuel figure of each of
  /// its ships, less Energy Technology's saving. A fleet with less than this and away
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

  /// Ticks one jump takes this fleet from the order to the arrival, set
  /// by its slowest ship and that ship's drive. A route of `n` jumps flown
  /// without a pause takes `n * jump_ticks`.
  final int jumpTicks;

  /// Hops a `scan` by this fleet reveals: `SCAN_PROBE_RANGE` with an
  /// Espionage Probe aboard, `SCAN_BASE_RANGE` without.
  final int scanRange;

  /// Ticks before the fleet takes its next order. While it boards a
  /// derelict (`Exploring`) it is what the boarding has left, out of
  /// `EXPLORE_DURATION_TICKS`.
  final int cooldownRemaining;

  /// Combat power of the whole fleet on the scale raids and threat ratings
  /// share: each ship counts its attack and a tenth of its shield and of
  /// the hull it holds, with `refit` counted.
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
  /// here, and a raid on it finds the fleet in its fight.
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
    required this.shipCount,
    required this.hull,
    required this.hullMax,
    required this.attack,
    required this.shield,
    required this.refit,
    this.refitDue = false,
    this.repair,
    this.flight,
    this.fleeing,
    this.pursuedBy,
    required this.cargo,
    required this.cargoCapacity,
    required this.fuel,
    required this.fuelMax,
    this.jumpFuel = 0,
    this.homeFuel = 0,
    required this.hopsHome,
    required this.jumpTicks,
    required this.scanRange,
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
      ships: _list(m['ships'], ShipGroupState.fromJson),
      shipCount: _int(m['ship_count']),
      hull: _int(m['hull']),
      hullMax: _int(m['hull_max']),
      attack: _int(m['attack']),
      shield: _int(m['shield']),
      refit: Refit.fromJson(m['refit']),
      refitDue: _opt(m['refit_due'], _bool) ?? false,
      repair: _opt(m['repair'], RepairQuote.fromJson),
      flight: _opt(m['flight'], FlightQuote.fromJson),
      fleeing: _opt(m['fleeing'], Fleeing.fromJson),
      pursuedBy: _opt(m['pursued_by'], Pursuit.fromJson),
      cargo: Resources.fromJson(m['cargo']),
      cargoCapacity: _double(m['cargo_capacity']),
      fuel: _double(m['fuel']),
      fuelMax: _double(m['fuel_max']),
      jumpFuel: _opt(m['jump_fuel'], _double) ?? 0,
      homeFuel: _opt(m['home_fuel'], _double) ?? 0,
      hopsHome: _int(m['hops_home']),
      jumpTicks: _int(m['jump_ticks']),
      scanRange: _int(m['scan_range']),
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
    'ship_count': shipCount,
    'hull': hull,
    'hull_max': hullMax,
    'attack': attack,
    'shield': shield,
    'refit': refit.toJson(),
    'refit_due': refitDue,
    'repair': ?repair?.toJson(),
    'flight': ?flight?.toJson(),
    'fleeing': ?fleeing?.toJson(),
    'pursued_by': ?pursuedBy?.toJson(),
    'cargo': cargo.toJson(),
    'cargo_capacity': cargoCapacity,
    'fuel': fuel,
    'fuel_max': fuelMax,
    'jump_fuel': jumpFuel,
    'home_fuel': homeFuel,
    'hops_home': hopsHome,
    'jump_ticks': jumpTicks,
    'scan_range': scanRange,
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

/// What leaving a fight costs a fleet, as things stand this tick. The price
/// of flight is the rounds it takes without firing back, the damage, which
/// stays until a dock mends it, the fuel, nothing earned from a group that
/// was not destroyed, and pursuit if the fleet is the slower.
class FlightQuote {
  /// Fuel the jump out burns: `jump_fuel` times `FLEE_FUEL_MULTIPLIER`.
  final double fuel;

  /// Ticks from the order until the fleet is gone.
  final int ticks;

  /// Rounds of fire it takes in that time without answering.
  final int rounds;

  /// The odds of the fight as it stands now, both sides with the losses
  /// and the damage they have taken: what `SectorOdds` would say of it,
  /// and what a standing order's `disengage_ratio_x10` is held against.
  final double ratio;
  final RatioLabel label;

  /// Why a `move` out would be refused now, in the engine's words; absent
  /// when it would be taken.
  final String? refusal;

  /// The group that would follow the fleet if it left now and the others
  /// with it; absent when none would. In a `MovePreview` it is for the
  /// jump to `target`: no group follows into home ground, the hub or a
  /// sector an outpost stands in.
  final ChaseForecast? chase;

  const FlightQuote({
    required this.fuel,
    required this.ticks,
    required this.rounds,
    required this.ratio,
    required this.label,
    this.refusal,
    this.chase,
  });

  factory FlightQuote.fromJson(Object? json) {
    final m = _obj(json);
    return FlightQuote(
      fuel: _double(m['fuel']),
      ticks: _int(m['ticks']),
      rounds: _int(m['rounds']),
      ratio: _double(m['ratio']),
      label: RatioLabel.fromJson(m['label']),
      refusal: _opt(m['refusal'], _string),
      chase: _opt(m['chase'], ChaseForecast.fromJson),
    );
  }

  Json toJson() => {
    'fuel': fuel,
    'ticks': ticks,
    'rounds': rounds,
    'ratio': ratio,
    'label': label.toJson(),
    'refusal': ?refusal,
    'chase': ?chase?.toJson(),
  };
}

/// How a group would follow a fleet that left it now.
class ChaseForecast {
  final int groupId;

  /// What the group is ("3x cruiser pack").
  final String label;

  /// Jumps it would make after the fleet before it turns back.
  final int hexes;

  /// Ticks one jump takes it.
  final int jumpTicks;

  /// Rounds it would fire on the fleet at each of those hexes, if the
  /// fleet orders its next jump the moment it lands: the difference of the
  /// two jump times, 0 for a fleet as fast as the group. A fleet that
  /// stops is caught and the fight is joined again.
  final int roundsPerHex;

  const ChaseForecast({
    required this.groupId,
    required this.label,
    required this.hexes,
    required this.jumpTicks,
    required this.roundsPerHex,
  });

  factory ChaseForecast.fromJson(Object? json) {
    final m = _obj(json);
    return ChaseForecast(
      groupId: _int(m['group_id']),
      label: _string(m['label']),
      hexes: _int(m['hexes']),
      jumpTicks: _int(m['jump_ticks']),
      roundsPerHex: _int(m['rounds_per_hex']),
    );
  }

  Json toJson() => {
    'group_id': groupId,
    'label': label,
    'hexes': hexes,
    'jump_ticks': jumpTicks,
    'rounds_per_hex': roundsPerHex,
  };
}

/// A jump out of a fight under way.
class Fleeing {
  final Hex to;

  /// Ticks until the fleet is gone.
  final int ticks;

  /// Rounds of fire it still takes.
  final int rounds;

  const Fleeing({required this.to, required this.ticks, required this.rounds});

  factory Fleeing.fromJson(Object? json) {
    final m = _obj(json);
    return Fleeing(to: Hex.fromJson(m['to']), ticks: _int(m['ticks']), rounds: _int(m['rounds']));
  }

  Json toJson() => {'to': to.toJson(), 'ticks': ticks, 'rounds': rounds};
}

/// A group on a fleet's trail, as the fleet's owner is told of it.
class Pursuit {
  final int groupId;

  /// What the group is ("3x cruiser pack").
  final String label;

  /// The sector it is jumping from.
  final Hex from;

  /// Ticks until it lands where the fleet stands.
  final int arrivesIn;

  /// It lands before the fleet can be gone: before the jump the fleet is
  /// charging lands or, for a fleet that is not charging one, before a
  /// jump ordered now would. It then fires on the fleet.
  final bool catches;

  /// Jumps it will still make after this one before it turns back.
  final int hexesLeft;

  const Pursuit({
    required this.groupId,
    required this.label,
    required this.from,
    required this.arrivesIn,
    required this.catches,
    required this.hexesLeft,
  });

  factory Pursuit.fromJson(Object? json) {
    final m = _obj(json);
    return Pursuit(
      groupId: _int(m['group_id']),
      label: _string(m['label']),
      from: Hex.fromJson(m['from']),
      arrivesIn: _int(m['arrives_in']),
      catches: _bool(m['catches']),
      hexesLeft: _int(m['hexes_left']),
    );
  }

  Json toJson() => {
    'group_id': groupId,
    'label': label,
    'from': from.toJson(),
    'arrives_in': arrivesIn,
    'catches': catches,
    'hexes_left': hexesLeft,
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

/// The ships of one class in a fleet or a hostile group. Shields are not a
/// state: every unit's is full at the start of every round.
class ShipGroupState {
  final ShipClass shipClass;
  final int count;

  /// How many are not whole.
  final int damaged;

  /// How many of those stand at or under `EXPLODE_BELOW_PCT` hundredths of
  /// their hull: each may explode the next time a shot breaks its shield.
  final int atRisk;

  /// Hull they hold between them, and at full.
  final int hull;
  final int hullMax;

  /// One ship's attack, shield and full hull in a round, with what its
  /// fleet or group fights with counted.
  final int unitAttack;
  final int unitShield;
  final int unitHull;

  const ShipGroupState({
    required this.shipClass,
    required this.count,
    required this.damaged,
    required this.atRisk,
    required this.hull,
    required this.hullMax,
    required this.unitAttack,
    required this.unitShield,
    required this.unitHull,
  });

  factory ShipGroupState.fromJson(Object? json) {
    final m = _obj(json);
    return ShipGroupState(
      shipClass: ShipClass.fromJson(m['class']),
      count: _int(m['count']),
      damaged: _int(m['damaged']),
      atRisk: _int(m['at_risk']),
      hull: _int(m['hull']),
      hullMax: _int(m['hull_max']),
      unitAttack: _int(m['unit_attack']),
      unitShield: _int(m['unit_shield']),
      unitHull: _int(m['unit_hull']),
    );
  }

  Json toJson() => {
    'class': shipClass.toJson(),
    'count': count,
    'damaged': damaged,
    'at_risk': atRisk,
    'hull': hull,
    'hull_max': hullMax,
    'unit_attack': unitAttack,
    'unit_shield': unitShield,
    'unit_hull': unitHull,
  };
}

/// What mending a fleet's damaged ships takes at each dock.
class RepairQuote {
  /// Damaged ships, and the hull they lack between them.
  final int ships;
  final int hull;

  /// The home dock: it mends a fleet the tick it docks, and every tick it
  /// stands there.
  final DockRepair home;

  /// The owner's outpost; absent when they have none.
  final DockRepair? outpost;

  const RepairQuote({required this.ships, required this.hull, required this.home, this.outpost});

  factory RepairQuote.fromJson(Object? json) {
    final m = _obj(json);
    return RepairQuote(
      ships: _int(m['ships']),
      hull: _int(m['hull']),
      home: DockRepair.fromJson(m['home']),
      outpost: _opt(m['outpost'], DockRepair.fromJson),
    );
  }

  Json toJson() => {'ships': ships, 'hull': hull, 'home': home.toJson(), 'outpost': ?outpost?.toJson()};
}

/// One dock's answer to a fleet's damage, as things stand this tick.
class DockRepair {
  /// The whole repair priced in metal: for each ship the dock's share of
  /// its cost at par, times the share of its hull that is gone. 0 at a
  /// dock that mends for nothing.
  final double price;

  /// What the dock's stock would pay now: metal first, crystal and
  /// deuterium at their weights where the metal runs out.
  final Resources payment;

  /// Ships that payment makes whole, the most damaged first, and the hull
  /// it restores in all. A ship the stock does not cover is left as it
  /// is, and so is every ship after it.
  final int ships;
  final int hull;

  /// Why this dock would mend nothing, in the engine's words; absent when
  /// it would mend.
  final String? refusal;

  const DockRepair({required this.price, required this.payment, required this.ships, required this.hull, this.refusal});

  factory DockRepair.fromJson(Object? json) {
    final m = _obj(json);
    return DockRepair(
      price: _double(m['price']),
      payment: Resources.fromJson(m['payment']),
      ships: _int(m['ships']),
      hull: _int(m['hull']),
      refusal: _opt(m['refusal'], _string),
    );
  }

  Json toJson() => {'price': price, 'payment': payment.toJson(), 'ships': ships, 'hull': hull, 'refusal': ?refusal};
}

/// One sector as the player knows it.
///
/// `live: true` means the server is watching the sector this tick (a fleet
/// is there, the homeworld's sensors cover it, or a scan has not expired) and every
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

/// A sector's chart is made by entering it and pays once, in resources.
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

/// A hostile group. `ships` is by class as a fleet's is. A group keeps the
/// losses and the damage of its fights: it is listed as it stands, and
/// mends only at rest in its own sector.
class NpcFleetInfo {
  final int id;
  final List<ShipGroupState> ships;

  /// Every ship of every class.
  final int shipCount;
  final NpcBehavior behavior;

  /// Ships it has when whole: `muster - ship_count` are lost.
  final int muster;

  /// The tick it will be whole again, ships and hull, if nothing disturbs
  /// it; absent for a group that is whole. It mends in steps and only at
  /// rest in its own sector: a fight or a pursuit stops the clock and the
  /// tick moves. On a sector that is not `live` it is what was true at
  /// `last_seen`; the odds count such a group whole once the tick is past.
  final int? wholeAt;

  /// Ticks one jump takes it.
  final int jumpTicks;

  /// The most jumps it makes after a fleet that leaves a fight with it:
  /// 0 for a group that never follows.
  final int chaseHexes;

  /// It is following a fleet or on its way back to its own sector.
  final GroupErrand? errand;

  const NpcFleetInfo({
    required this.id,
    required this.ships,
    required this.shipCount,
    required this.behavior,
    required this.muster,
    this.wholeAt,
    required this.jumpTicks,
    required this.chaseHexes,
    this.errand,
  });

  factory NpcFleetInfo.fromJson(Object? json) {
    final m = _obj(json);
    return NpcFleetInfo(
      id: _int(m['id']),
      ships: _list(m['ships'], ShipGroupState.fromJson),
      shipCount: _int(m['ship_count']),
      behavior: NpcBehavior.fromJson(m['behavior']),
      muster: _int(m['muster']),
      wholeAt: _opt(m['whole_at'], _int),
      jumpTicks: _int(m['jump_ticks']),
      chaseHexes: _int(m['chase_hexes']),
      errand: _opt(m['errand'], GroupErrand.fromJson),
    );
  }

  Json toJson() => {
    'id': id,
    'ships': ships.map((e) => e.toJson()).toList(),
    'ship_count': shipCount,
    'behavior': behavior.toJson(),
    'muster': muster,
    'whole_at': ?wholeAt,
    'jump_ticks': jumpTicks,
    'chase_hexes': chaseHexes,
    'errand': ?errand?.toJson(),
  };
}

/// What takes a group out of its own sector. Its sector is empty meanwhile.
class GroupErrand {
  /// The sector it lairs in and goes back to.
  final Hex home;

  /// The fleet it follows and the empire that owns it; absent when it has
  /// turned back.
  final int? quarry;
  final String? quarryOwner;

  /// The sector it is jumping to and the tick it lands there; absent while
  /// it fights.
  final Hex? to;
  final int? arrivesAt;

  /// Jumps it will still make after a fleet before it turns back.
  final int hexesLeft;

  const GroupErrand({
    required this.home,
    this.quarry,
    this.quarryOwner,
    this.to,
    this.arrivesAt,
    required this.hexesLeft,
  });

  factory GroupErrand.fromJson(Object? json) {
    final m = _obj(json);
    return GroupErrand(
      home: Hex.fromJson(m['home']),
      quarry: _opt(m['quarry'], _int),
      quarryOwner: _opt(m['quarry_owner'], _string),
      to: _opt(m['to'], Hex.fromJson),
      arrivesAt: _opt(m['arrives_at'], _int),
      hexesLeft: _int(m['hexes_left']),
    );
  }

  Json toJson() => {
    'home': home.toJson(),
    'quarry': ?quarry,
    'quarry_owner': ?quarryOwner,
    'to': ?to?.toJson(),
    'arrives_at': ?arrivesAt,
    'hexes_left': hexesLeft,
  };
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

  /// Its ships by class, in catalogue order; a class it has none of is left out.
  final List<ShipCount> shipClasses;

  const FleetBrief({required this.id, required this.ownerName, required this.shipCount, this.shipClasses = const []});

  factory FleetBrief.fromJson(Object? json) {
    final m = _obj(json);
    return FleetBrief(
      id: _int(m['id']),
      ownerName: _string(m['owner_name']),
      shipCount: _int(m['ship_count']),
      shipClasses: _opt(m['ship_classes'], (v) => _list(v, ShipCount.fromJson)) ?? const [],
    );
  }

  Json toJson() => {
    'id': id,
    'owner_name': ownerName,
    'ship_count': shipCount,
    'ship_classes': shipClasses.map((e) => e.toJson()).toList(),
  };
}

/// So many ships of one class.
class ShipCount {
  final ShipClass shipClass;
  final int count;

  const ShipCount({required this.shipClass, required this.count});

  factory ShipCount.fromJson(Object? json) {
    final m = _obj(json);
    return ShipCount(shipClass: ShipClass.fromJson(m['class']), count: _int(m['count']));
  }

  Json toJson() => {'class': shipClass.toJson(), 'count': count};

  List<Object?> get _props => [shipClass, count];

  @override
  bool operator ==(Object other) => other is ShipCount && _same(_props, other._props);

  @override
  int get hashCode => Object.hashAll(_props);
}

class HomeworldState {
  final Hex location;

  /// What the stockpile gains each tick at current levels and the energy
  /// there is (before the storage cap discards the excess; see
  /// `storage`): the mines at `energy.fraction`, the natural production,
  /// less the deuterium the Fusion Reactor burns, so deuterium may be
  /// negative.
  final Resources production;
  final EnergyState energy;

  /// Solar Satellites at home. They are built in the shipyard queue as
  /// the ship class `SolarSatellite` and never join a fleet. Those a raid
  /// destroys are counted out as they fall and the ones rebuilt are back
  /// the tick the raid ends, as the defences are.
  final int satellites;
  final StorageState storage;
  final List<BuildingState> buildings;
  final List<ResearchState> research;

  /// Buildings under construction, at most `build_slots` of them.
  final List<BuildQueueItem> buildQueue;

  /// Waiting behind them; nothing is paid until an item starts.
  final List<QueuedBuild> buildPending;

  /// How many buildings may be under construction at once.
  final int buildSlots;

  /// What opens one more building slot; absent when every slot is open.
  final Requirement? buildSlotUnlock;

  /// Orders that may wait behind the running ones in each queue. An order
  /// that fits is always accepted, started or not; only a full waiting
  /// line (or an order that can never start) is refused with `QueueFull`.
  final int queueWaitingMax;
  final ShipyardQueueItem? shipyardQueue;
  final List<QueuedShip> shipyardPending;
  final ResearchItem? researchActive;
  final List<QueuedResearch> researchPending;

  /// The ships of every fleet standing at home, summed by class, in
  /// catalogue order, and how many they are in all.
  final List<ShipGroupState> dockedShips;
  final int dockedShipCount;

  /// Defences standing at home. Those a raid destroys are counted out as
  /// they fall and the ones rebuilt are back the tick the raid ends.
  final List<DefenceState> defences;

  /// Docked ships, Solar Satellites and all defences together, on the
  /// power scale. It is a sum and decides nothing: a raid is a battle,
  /// and `raid.odds` is how it would go.
  final double homeDefencePower;

  /// The share of `home_defence_power` the ships standing at home add.
  final double dockedPower;

  /// The Solar Satellites' share of `home_defence_power`.
  final double satellitePower;

  /// The standing defences' share of `home_defence_power`.
  final double structurePower;

  /// Typical power of the next raid, from the empire's building and
  /// research points; the actual raid rolls 0.80 to 1.15 times this.
  final double nextRaidEstimatePower;

  /// The raid being fought (`rounds_left`), else the one announced
  /// (`arrival_tick`), else one of typical size were it announced now:
  /// what it flies, and how it would end against the ships, satellites
  /// and defences standing at home this tick.
  final RaidForecast raid;

  /// What can be queued right now and what it costs; see `HomeworldCatalog`.
  final HomeworldCatalog catalog;

  const HomeworldState({
    required this.location,
    required this.production,
    required this.energy,
    required this.satellites,
    required this.storage,
    required this.buildings,
    required this.research,
    required this.buildQueue,
    required this.buildPending,
    required this.buildSlots,
    this.buildSlotUnlock,
    required this.queueWaitingMax,
    this.shipyardQueue,
    required this.shipyardPending,
    this.researchActive,
    required this.researchPending,
    required this.dockedShips,
    required this.dockedShipCount,
    required this.defences,
    required this.homeDefencePower,
    required this.dockedPower,
    required this.satellitePower,
    required this.structurePower,
    required this.nextRaidEstimatePower,
    required this.raid,
    required this.catalog,
  });

  factory HomeworldState.fromJson(Object? json) {
    final m = _obj(json);
    return HomeworldState(
      location: Hex.fromJson(m['location']),
      production: Resources.fromJson(m['production']),
      energy: EnergyState.fromJson(m['energy']),
      satellites: _int(m['satellites']),
      storage: StorageState.fromJson(m['storage']),
      buildings: _list(m['buildings'], BuildingState.fromJson),
      research: _list(m['research'], ResearchState.fromJson),
      buildQueue: _list(m['build_queue'], BuildQueueItem.fromJson),
      buildPending: _list(m['build_pending'], QueuedBuild.fromJson),
      buildSlots: _int(m['build_slots']),
      buildSlotUnlock: _opt(m['build_slot_unlock'], Requirement.fromJson),
      queueWaitingMax: _int(m['queue_waiting_max']),
      shipyardQueue: _opt(m['shipyard_queue'], ShipyardQueueItem.fromJson),
      shipyardPending: _list(m['shipyard_pending'], QueuedShip.fromJson),
      researchActive: _opt(m['research_active'], ResearchItem.fromJson),
      researchPending: _list(m['research_pending'], QueuedResearch.fromJson),
      dockedShips: _list(m['docked_ships'], ShipGroupState.fromJson),
      dockedShipCount: _int(m['docked_ship_count']),
      defences: _list(m['defences'], DefenceState.fromJson),
      homeDefencePower: _double(m['home_defence_power']),
      dockedPower: _double(m['docked_power']),
      satellitePower: _double(m['satellite_power']),
      structurePower: _double(m['structure_power']),
      nextRaidEstimatePower: _double(m['next_raid_estimate_power']),
      raid: RaidForecast.fromJson(m['raid']),
      catalog: HomeworldCatalog.fromJson(m['catalog']),
    );
  }

  Json toJson() => {
    'location': location.toJson(),
    'production': production.toJson(),
    'energy': energy.toJson(),
    'satellites': satellites,
    'storage': storage.toJson(),
    'buildings': buildings.map((e) => e.toJson()).toList(),
    'research': research.map((e) => e.toJson()).toList(),
    'build_queue': buildQueue.map((e) => e.toJson()).toList(),
    'build_pending': buildPending.map((e) => e.toJson()).toList(),
    'build_slots': buildSlots,
    'build_slot_unlock': ?buildSlotUnlock?.toJson(),
    'queue_waiting_max': queueWaitingMax,
    'shipyard_queue': ?shipyardQueue?.toJson(),
    'shipyard_pending': shipyardPending.map((e) => e.toJson()).toList(),
    'research_active': ?researchActive?.toJson(),
    'research_pending': researchPending.map((e) => e.toJson()).toList(),
    'docked_ships': dockedShips.map((e) => e.toJson()).toList(),
    'docked_ship_count': dockedShipCount,
    'defences': defences.map((e) => e.toJson()).toList(),
    'home_defence_power': homeDefencePower,
    'docked_power': dockedPower,
    'satellite_power': satellitePower,
    'structure_power': structurePower,
    'next_raid_estimate_power': nextRaidEstimatePower,
    'raid': raid.toJson(),
    'catalog': catalog.toJson(),
  };
}

class DefenceState {
  final DefenceKind kind;
  final int count;

  /// Power of one with the owner's Weapons, Shielding and Armour Technology.
  final double power;

  const DefenceState({required this.kind, required this.count, required this.power});

  factory DefenceState.fromJson(Object? json) {
    final m = _obj(json);
    return DefenceState(kind: DefenceKind.fromJson(m['kind']), count: _int(m['count']), power: _double(m['power']));
  }

  Json toJson() => {'kind': kind.toJson(), 'count': count, 'power': power};
}

/// A homeworld's energy. The mines need it and the Solar Plant, the Fusion
/// Reactor and the Solar Satellites make it; it is not stored.
class EnergyState {
  final int made;
  final int needed;

  /// The fraction of their output the mines run at: `made / needed`, at
  /// most 1, and 1 when nothing is needed.
  final double fraction;

  /// Energy each Solar Satellite makes.
  final int perSatellite;

  /// Deuterium the Fusion Reactor burns each tick while it runs.
  final double fusionBurn;

  /// The Fusion Reactor stands and makes nothing: the stockpile has no
  /// deuterium for it to burn.
  final bool fusionStarved;

  const EnergyState({
    required this.made,
    required this.needed,
    required this.fraction,
    required this.perSatellite,
    required this.fusionBurn,
    required this.fusionStarved,
  });

  factory EnergyState.fromJson(Object? json) {
    final m = _obj(json);
    return EnergyState(
      made: _int(m['made']),
      needed: _int(m['needed']),
      fraction: _double(m['fraction']),
      perSatellite: _int(m['per_satellite']),
      fusionBurn: _double(m['fusion_burn']),
      fusionStarved: _bool(m['fusion_starved']),
    );
  }

  Json toJson() => {
    'made': made,
    'needed': needed,
    'fraction': fraction,
    'per_satellite': perSatellite,
    'fusion_burn': fusionBurn,
    'fusion_starved': fusionStarved,
  };
}

/// Stockpile limits, each from the level of its own storage building and
/// the world pace. Mines stop at `cap`; docking cargo unloads only up to it.
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
/// balance formulas: costs, build times, prerequisites and what each level
/// does, computed by the server from `scaling.rs` for the player's current
/// levels. Resent with every `HomeworldState`, so it tracks level changes
/// the tick they land. What never changes (a class's base figures, its
/// label, the rapid-fire table) is not here: the figures are columns of the
/// enums and the table is `WorldInfo.rapid_fire`. Whether the player can
/// pay is left to the client (compare `cost` with the stockpile in
/// `PlayerState.resources`).
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

/// One prerequisite: a building or tech that must reach level `need`, with
/// the player's current level, or (`energy`) an amount of energy the
/// homeworld must be making, with what it makes. The server's
/// "prerequisites not met" errors are written from the same values.
class Requirement {
  /// Building or tech label ("Shipyard", "Impulse Drive"), or "Energy".
  final String name;
  final int need;
  final int have;
  final bool met;

  /// `need` and `have` are energy made, not levels.
  final bool energy;

  const Requirement({
    required this.name,
    required this.need,
    required this.have,
    required this.met,
    required this.energy,
  });

  factory Requirement.fromJson(Object? json) {
    final m = _obj(json);
    return Requirement(
      name: _string(m['name']),
      need: _int(m['need']),
      have: _int(m['have']),
      met: _bool(m['met']),
      energy: _bool(m['energy']),
    );
  }

  Json toJson() => {'name': name, 'need': need, 'have': have, 'met': met, 'energy': energy};
}

/// What a level of a building or a research does, as a figure a client
/// prints beside its label. Rates are a tick at the world's pace.
enum EffectKind {
  /// What the mine makes with all the energy it needs; the natural
  /// production is not counted.
  metalPerTick('metal_per_tick', label: 'metal'),

  crystalPerTick('crystal_per_tick', label: 'crystal'),
  deuteriumPerTick('deuterium_per_tick', label: 'deuterium'),
  energyUsed('energy_used', label: 'energy used'),
  energyMade('energy_made', label: 'energy made'),

  /// Deuterium the Fusion Reactor burns while it runs.
  fusionBurnPerTick('fusion_burn_per_tick', label: 'deuterium burned'),

  /// What building, ship and defence, or research time is divided by,
  /// with every other building as it stands.
  buildingSpeed('building_speed', label: 'building speed'),

  shipyardSpeed('shipyard_speed', label: 'shipyard speed'),
  researchSpeed('research_speed', label: 'research speed'),

  /// The cap of the building's own resource, and the share of it a raid
  /// cannot take.
  storageCap('storage_cap', label: 'storage'),

  storageProtected('storage_protected', label: 'safe from raids'),

  /// What every fleet's tank is multiplied by.
  tankFactor('tank_factor', label: 'fleet tanks'),

  /// What attack, shield or hull is multiplied by: for defences and
  /// Solar Satellites at once, for a fleet from when it next docks at
  /// home (`FleetState.refit`).
  attack('attack', label: 'attack'),

  shield('shield', label: 'shield'),
  hull('hull', label: 'hull'),

  /// What the speed of the ships on this drive is multiplied by.
  speed('speed', label: 'speed'),

  /// What a jump's burn is multiplied by.
  jumpFuel('jump_fuel', label: 'fuel a jump'),

  /// Taken off the chance that an emergency jump harms a ship, as a share.
  recallRisk('recall_risk', label: 'emergency jump risk'),

  buildSlots('build_slots', label: 'building slots'),

  /// Hops around the homeworld that are seen.
  sensorHops('sensor_hops', label: 'sensor range'),

  /// What a unit of ore yields.
  oreYield('ore_yield', label: 'ore yield');

  const EffectKind(this.wire, {required this.label});

  final String wire;
  final String label;

  static EffectKind fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// One figure of a building or research: at the level it stands at, and at
/// the next (absent at the highest level).
class Effect {
  final EffectKind kind;
  final double now;
  final double? next;

  const Effect({required this.kind, required this.now, this.next});

  factory Effect.fromJson(Object? json) {
    final m = _obj(json);
    return Effect(kind: EffectKind.fromJson(m['kind']), now: _double(m['now']), next: _opt(m['next'], _double));
  }

  Json toJson() => {'kind': kind.toJson(), 'now': now, 'next': ?next};
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

  /// What the building does at its level and at the next.
  final List<Effect> effects;

  const BuildingOption({
    required this.buildingType,
    required this.level,
    required this.maxLevel,
    this.next,
    required this.requires,
    required this.effects,
  });

  factory BuildingOption.fromJson(Object? json) {
    final m = _obj(json);
    return BuildingOption(
      buildingType: BuildingType.fromJson(m['building_type']),
      level: _int(m['level']),
      maxLevel: _int(m['max_level']),
      next: _opt(m['next'], UpgradeStep.fromJson),
      requires: _list(m['requires'], Requirement.fromJson),
      effects: _list(m['effects'], Effect.fromJson),
    );
  }

  Json toJson() => {
    'building_type': buildingType.toJson(),
    'level': level,
    'max_level': maxLevel,
    'next': ?next?.toJson(),
    'requires': requires.map((e) => e.toJson()).toList(),
    'effects': effects.map((e) => e.toJson()).toList(),
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

  /// What the research does at its level and at the next; empty for one
  /// that is only a requirement of other things.
  final List<Effect> effects;

  const ResearchOption({
    required this.tech,
    required this.level,
    required this.maxLevel,
    this.next,
    required this.requires,
    required this.effects,
  });

  factory ResearchOption.fromJson(Object? json) {
    final m = _obj(json);
    return ResearchOption(
      tech: ResearchType.fromJson(m['tech']),
      level: _int(m['level']),
      maxLevel: _int(m['max_level']),
      next: _opt(m['next'], UpgradeStep.fromJson),
      requires: _list(m['requires'], Requirement.fromJson),
      effects: _list(m['effects'], Effect.fromJson),
    );
  }

  Json toJson() => {
    'tech': tech.toJson(),
    'level': level,
    'max_level': maxLevel,
    'next': ?next?.toJson(),
    'requires': requires.map((e) => e.toJson()).toList(),
    'effects': effects.map((e) => e.toJson()).toList(),
  };
}

class ShipOption {
  final ShipClass shipClass;

  /// Cost of one ship; a batch of `count` costs `count` times this.
  final Resources unitCost;

  /// Ticks to build one ship at the current Shipyard and Nanite Factory levels.
  final int ticksPerShip;
  final List<Requirement> requires;

  /// What a ship in a fleet docked at home fights with: the class's
  /// figures with the owner's Armour, Shielding and Weapons Technology,
  /// cut off to whole numbers as a round counts them. A fleet away from
  /// home fights with `FleetState.refit`.
  final double hull;
  final double shield;
  final double weapon;

  /// `weapon + (hull + shield) / 10` of those.
  final double power;

  /// One ship's hold at the world's pace.
  final double cargo;

  /// False for the Solar Satellite: it stays at the homeworld, counted in
  /// `HomeworldState.satellites`, and the flight figures below are 0.
  final bool flies;

  /// The drive it flies on with the owner's research; the Small Cargo and
  /// the Bomber change theirs.
  final Drive? drive;
  final double speed;

  /// Ticks one jump takes a fleet whose slowest ship this is.
  final int jumpTicks;

  /// Fuel one jump burns, with Energy Technology's saving.
  final double jumpFuel;

  /// The tank one ship adds to a fleet, with the Deuterium Tank counted.
  final double tank;

  /// Raw units of ore a tick it extracts at density multiplier 1.
  final double harvest;

  const ShipOption({
    required this.shipClass,
    required this.unitCost,
    required this.ticksPerShip,
    required this.requires,
    required this.hull,
    required this.shield,
    required this.weapon,
    required this.power,
    required this.cargo,
    required this.flies,
    this.drive,
    required this.speed,
    required this.jumpTicks,
    required this.jumpFuel,
    required this.tank,
    required this.harvest,
  });

  factory ShipOption.fromJson(Object? json) {
    final m = _obj(json);
    return ShipOption(
      shipClass: ShipClass.fromJson(m['ship_class']),
      unitCost: Resources.fromJson(m['unit_cost']),
      ticksPerShip: _int(m['ticks_per_ship']),
      requires: _list(m['requires'], Requirement.fromJson),
      hull: _double(m['hull']),
      shield: _double(m['shield']),
      weapon: _double(m['weapon']),
      power: _double(m['power']),
      cargo: _double(m['cargo']),
      flies: _bool(m['flies']),
      drive: _opt(m['drive'], Drive.fromJson),
      speed: _double(m['speed']),
      jumpTicks: _int(m['jump_ticks']),
      jumpFuel: _double(m['jump_fuel']),
      tank: _double(m['tank']),
      harvest: _double(m['harvest']),
    );
  }

  Json toJson() => {
    'ship_class': shipClass.toJson(),
    'unit_cost': unitCost.toJson(),
    'ticks_per_ship': ticksPerShip,
    'requires': requires.map((e) => e.toJson()).toList(),
    'hull': hull,
    'shield': shield,
    'weapon': weapon,
    'power': power,
    'cargo': cargo,
    'flies': flies,
    'drive': ?drive?.toJson(),
    'speed': speed,
    'jump_ticks': jumpTicks,
    'jump_fuel': jumpFuel,
    'tank': tank,
    'harvest': harvest,
  };
}

class DefenceOption {
  final DefenceKind kind;

  /// Standing now.
  final int count;

  /// The most that may stand, those on order included: 1 for a dome.
  /// Absent when there is no limit.
  final int? max;

  /// How many more may be ordered now: `max` less those standing and on
  /// order. Absent when there is no limit.
  final int? room;
  final Resources unitCost;
  final int ticksPerUnit;

  /// What one fights with: the kind's figures with the owner's Armour,
  /// Shielding and Weapons Technology as they stand now.
  final double hull;
  final double shield;
  final double weapon;

  /// `weapon + (hull + shield) / 10` of those.
  final double power;
  final List<Requirement> requires;

  const DefenceOption({
    required this.kind,
    required this.count,
    this.max,
    this.room,
    required this.unitCost,
    required this.ticksPerUnit,
    required this.hull,
    required this.shield,
    required this.weapon,
    required this.power,
    required this.requires,
  });

  factory DefenceOption.fromJson(Object? json) {
    final m = _obj(json);
    return DefenceOption(
      kind: DefenceKind.fromJson(m['kind']),
      count: _int(m['count']),
      max: _opt(m['max'], _int),
      room: _opt(m['room'], _int),
      unitCost: Resources.fromJson(m['unit_cost']),
      ticksPerUnit: _int(m['ticks_per_unit']),
      hull: _double(m['hull']),
      shield: _double(m['shield']),
      weapon: _double(m['weapon']),
      power: _double(m['power']),
      requires: _list(m['requires'], Requirement.fromJson),
    );
  }

  Json toJson() => {
    'kind': kind.toJson(),
    'count': count,
    'max': ?max,
    'room': ?room,
    'unit_cost': unitCost.toJson(),
    'ticks_per_unit': ticksPerUnit,
    'hull': hull,
    'shield': shield,
    'weapon': weapon,
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
      'BattleRound' => BattleRoundKind.fromJson(json),
      'ShipsLost' => ShipsLostEvent.fromJson(json),
      'FleetDestroyed' => FleetDestroyedEvent.fromJson(json),
      'ResourceHarvested' => ResourceHarvestedEvent.fromJson(json),
      'SectorEntered' => SectorEnteredEvent.fromJson(json),
      'CombatStarted' => CombatStartedEvent.fromJson(json),
      'CombatEnded' => CombatEndedEvent.fromJson(json),
      'Pursuit' => PursuitEvent.fromJson(json),
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

class BattleRoundKind extends EventKind {
  final BattleRoundEvent battleRound;

  const BattleRoundKind(this.battleRound);

  factory BattleRoundKind.fromJson(Object? json) => BattleRoundKind(BattleRoundEvent.fromJson(json));

  @override
  Json toJson() => {'kind': 'BattleRound', ...battleRound.toJson()};
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

  /// "Metal Mine Lv.5", "Light Fighter x3".
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

  /// Loot taken aboard, a relic's worth included (cargo-capped; overflow
  /// drops as sector salvage).
  final Resources resources;
  final ShipClass? recoveredShip;

  /// Instant tech level granted by a recovered data core.
  final ResearchType? techCache;

  /// An ancient relic came aboard: the loot of the site is worth
  /// `RELIC_LOOT_MULT` times more for it.
  final bool relic;

  const SiteExploredEvent({
    required this.fleetId,
    required this.sector,
    required this.tier,
    required this.resources,
    this.recoveredShip,
    this.techCache,
    this.relic = false,
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
  };
}

/// A fleet docked at the homeworld, or arrived at its owner's finished
/// outpost, and handed over the sectors it charted. Each new chart pays
/// `scaling::chart_value` into the stockpile, as far as its caps allow.
class ChartDeliveredEvent extends EventKind {
  final int fleetId;

  /// Sectors newly credited to the empire.
  final int sectors;

  /// What they paid into the stockpile. What a full store has no room
  /// for is lost.
  final Resources paid;

  const ChartDeliveredEvent({required this.fleetId, required this.sectors, required this.paid});

  factory ChartDeliveredEvent.fromJson(Object? json) {
    final m = _obj(json);
    return ChartDeliveredEvent(
      fleetId: _int(m['fleet_id']),
      sectors: _int(m['sectors']),
      paid: Resources.fromJson(m['paid']),
    );
  }

  @override
  Json toJson() => {'kind': 'ChartDelivered', 'fleet_id': fleetId, 'sectors': sectors, 'paid': paid.toJson()};
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

/// One round of a fight, one a tick while it lasts. It is OGame's round:
/// every unit's shield is full at its start; every unit of both sides then
/// fires, at once, each at one unit of the other side picked at random,
/// and again while its rapid fire against what it hit holds. A shot weaker
/// than a hundredth of the target's full shield bounces; one that breaks
/// the shield takes the rest from the hull; a unit left with no shield and
/// at or under `EXPLODE_BELOW_PCT` hundredths of its hull may explode. The
/// report is by class, never by ship, so it is the same size for five ships
/// and for five thousand.
class BattleRoundEvent {
  final Hex sector;

  /// 1 for the first round of the fight.
  final int round;

  /// The players' fleets, with what they fired this round.
  final BattleSide fleets;

  /// The hostile groups, with what they fired. In a raid they are one
  /// group, the raiders.
  final BattleSide hostiles;

  /// A fleet of the receiver's is in the fight, or the fight is a raid on
  /// the receiver's homeworld or outpost.
  final bool mine;

  /// The round is a raid's: `fleets` holds the raided empire's fleets at
  /// the dock and, marked `battery`, its defences. `BattleState.raid` of
  /// the same sector says how many rounds are left.
  final bool raid;

  const BattleRoundEvent({
    required this.sector,
    required this.round,
    required this.fleets,
    required this.hostiles,
    this.mine = false,
    this.raid = false,
  });

  factory BattleRoundEvent.fromJson(Object? json) {
    final m = _obj(json);
    return BattleRoundEvent(
      sector: Hex.fromJson(m['sector']),
      round: _int(m['round']),
      fleets: BattleSide.fromJson(m['fleets']),
      hostiles: BattleSide.fromJson(m['hostiles']),
      mine: _opt(m['mine'], _bool) ?? false,
      raid: _opt(m['raid'], _bool) ?? false,
    );
  }

  Json toJson() => {
    'sector': sector.toJson(),
    'round': round,
    'fleets': fleets.toJson(),
    'hostiles': hostiles.toJson(),
    'mine': mine,
    'raid': raid,
  };
}

/// One side of a round: who stands after it, and what the side fired.
class BattleSide {
  /// Every fleet or group of the side that entered the round, by id, as
  /// it stands after it.
  final List<BattleGroup> groups;

  /// Units the side stood in the round with, in all its groups, and how
  /// many of them still stand after it.
  final int entered;
  final int standing;

  /// The side's fire, one line for each pair of firing and target unit
  /// that exchanged a shot, in catalogue order.
  final List<Volley> volleys;

  /// Shots the side fired, their attack summed, what the other side's
  /// shields took of it, what reached its hulls, and the units of the
  /// other side it destroyed.
  final int shots;
  final int power;
  final int absorbed;
  final int hullDamage;
  final int destroyed;

  const BattleSide({
    required this.groups,
    required this.entered,
    required this.standing,
    required this.volleys,
    required this.shots,
    required this.power,
    required this.absorbed,
    required this.hullDamage,
    required this.destroyed,
  });

  factory BattleSide.fromJson(Object? json) {
    final m = _obj(json);
    return BattleSide(
      groups: _list(m['groups'], BattleGroup.fromJson),
      entered: _int(m['entered']),
      standing: _int(m['standing']),
      volleys: _list(m['volleys'], Volley.fromJson),
      shots: _int(m['shots']),
      power: _int(m['power']),
      absorbed: _int(m['absorbed']),
      hullDamage: _int(m['hull_damage']),
      destroyed: _int(m['destroyed']),
    );
  }

  Json toJson() => {
    'groups': groups.map((e) => e.toJson()).toList(),
    'entered': entered,
    'standing': standing,
    'volleys': volleys.map((e) => e.toJson()).toList(),
    'shots': shots,
    'power': power,
    'absorbed': absorbed,
    'hull_damage': hullDamage,
    'destroyed': destroyed,
  };
}

class BattleGroup {
  /// The fleet or the group. A raid's raiders carry an id of their own;
  /// the defences of the place raided carry 0 and `battery`.
  final int fleetId;

  /// Empire owning the fleet; `None` for a hostile group.
  final String? owner;

  /// What a hostile group is ("3x cruiser pack"); empty for a player's
  /// fleet. For a battery, what it is ("home defences", "outpost platform").
  final String label;

  /// The fleet is the receiver's.
  final bool mine;

  /// The fleet is leaving: it was fired on this round and did not fire.
  final bool fleeing;

  /// Not a fleet: the defences and Solar Satellites of a homeworld under
  /// raid, or an outpost's platform. It stands on the fleets' side.
  final bool battery;

  /// By unit, in catalogue order. A unit wiped out this round is listed
  /// once more with a count of 0.
  final List<BattleUnits> units;

  const BattleGroup({
    required this.fleetId,
    this.owner,
    this.label = '',
    this.mine = false,
    this.fleeing = false,
    this.battery = false,
    required this.units,
  });

  factory BattleGroup.fromJson(Object? json) {
    final m = _obj(json);
    return BattleGroup(
      fleetId: _int(m['fleet_id']),
      owner: _opt(m['owner'], _string),
      label: _opt(m['label'], _string) ?? '',
      mine: _opt(m['mine'], _bool) ?? false,
      fleeing: _opt(m['fleeing'], _bool) ?? false,
      battery: _opt(m['battery'], _bool) ?? false,
      units: _list(m['units'], BattleUnits.fromJson),
    );
  }

  Json toJson() => {
    'fleet_id': fleetId,
    'owner': ?owner,
    if (label.isNotEmpty) 'label': label,
    'mine': mine,
    'fleeing': fleeing,
    'battery': battery,
    'units': units.map((e) => e.toJson()).toList(),
  };
}

/// The units of one kind in a fleet or group after a round.
class BattleUnits {
  /// A ship class or a defence: the wire carries just its name.
  final ShipyardItem unit;

  /// How many stood in the round when it began, and how many stand after
  /// it. The rest of the row is of those that stand.
  final int entered;
  final int count;
  final int damaged;
  final int atRisk;
  final int hull;
  final int hullMax;

  /// Destroyed this round.
  final int lost;

  const BattleUnits({
    required this.unit,
    required this.entered,
    required this.count,
    required this.damaged,
    required this.atRisk,
    required this.hull,
    required this.hullMax,
    required this.lost,
  });

  factory BattleUnits.fromJson(Object? json) {
    final m = _obj(json);
    return BattleUnits(
      unit: ShipyardItem.fromJson(m['unit']),
      entered: _int(m['entered']),
      count: _int(m['count']),
      damaged: _int(m['damaged']),
      atRisk: _int(m['at_risk']),
      hull: _int(m['hull']),
      hullMax: _int(m['hull_max']),
      lost: _int(m['lost']),
    );
  }

  Json toJson() => {
    'unit': unit.toJson(),
    'entered': entered,
    'count': count,
    'damaged': damaged,
    'at_risk': atRisk,
    'hull': hull,
    'hull_max': hullMax,
    'lost': lost,
  };
}

/// What the units of one kind fired at the units of another in a round.
class Volley {
  final ShipyardItem from;
  final ShipyardItem at;

  /// Every shot, rapid fire included. Of them `bounced` did nothing to a
  /// shield they were too weak for, and `wasted` went at a unit already
  /// destroyed in this round.
  final int shots;
  final int bounced;
  final int wasted;

  /// Attack the targets' shields took, and what reached their hulls.
  final int absorbed;
  final int hullDamage;

  /// Targets destroyed.
  final int destroyed;

  const Volley({
    required this.from,
    required this.at,
    required this.shots,
    required this.bounced,
    required this.wasted,
    required this.absorbed,
    required this.hullDamage,
    required this.destroyed,
  });

  factory Volley.fromJson(Object? json) {
    final m = _obj(json);
    return Volley(
      from: ShipyardItem.fromJson(m['from']),
      at: ShipyardItem.fromJson(m['at']),
      shots: _int(m['shots']),
      bounced: _int(m['bounced']),
      wasted: _int(m['wasted']),
      absorbed: _int(m['absorbed']),
      hullDamage: _int(m['hull_damage']),
      destroyed: _int(m['destroyed']),
    );
  }

  Json toJson() => {
    'from': from.toJson(),
    'at': at.toJson(),
    'shots': shots,
    'bounced': bounced,
    'wasted': wasted,
    'absorbed': absorbed,
    'hull_damage': hullDamage,
    'destroyed': destroyed,
  };
}

/// How a fight ended.
enum BattleOutcome {
  /// No hostile unit is left and a fleet still stands.
  hostilesDestroyed('hostiles_destroyed'),

  /// No ship of any fleet is left and a hostile still stands.
  fleetsDestroyed('fleets_destroyed'),

  /// The last units of both sides fell in the same round.
  bothDestroyed('both_destroyed'),

  /// The last fleet made the emergency jump home. The hostiles keep the
  /// losses and the damage they took, and mend in time.
  withdrawn('withdrawn'),

  /// `STANDOFF_ROUNDS` rounds took no hull from either side: nobody can
  /// hurt anybody. Both sides stay where they are, as they are, and
  /// nothing starts the fight again but an arrival or an `attack`.
  standoff('standoff'),

  /// The last fleet jumped out of the sector under fire. The hostiles keep
  /// the losses and the damage they took; a group that was beating the
  /// fleet may follow it (`Pursuit`).
  fled('fled'),

  /// A raid's `RAID_ROUNDS` rounds are over, or a round took no hull from
  /// anybody, and a defender still stands: the raiders leave with nothing.
  outlasted('outlasted');

  const BattleOutcome(this.wire);

  final String wire;

  static BattleOutcome fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// What one fleet or group lost over a whole fight.
class BattleLosses {
  final int fleetId;
  final String? owner;
  final String label;
  final bool mine;

  /// As `BattleGroup.battery`: the defences of the place raided.
  final bool battery;

  /// Units destroyed, by kind, in catalogue order; empty when it lost none.
  final List<UnitCount> lost;

  /// Nothing of it is left.
  final bool destroyed;

  const BattleLosses({
    required this.fleetId,
    this.owner,
    this.label = '',
    this.mine = false,
    this.battery = false,
    required this.lost,
    required this.destroyed,
  });

  factory BattleLosses.fromJson(Object? json) {
    final m = _obj(json);
    return BattleLosses(
      fleetId: _int(m['fleet_id']),
      owner: _opt(m['owner'], _string),
      label: _opt(m['label'], _string) ?? '',
      mine: _opt(m['mine'], _bool) ?? false,
      battery: _opt(m['battery'], _bool) ?? false,
      lost: _list(m['lost'], UnitCount.fromJson),
      destroyed: _bool(m['destroyed']),
    );
  }

  Json toJson() => {
    'fleet_id': fleetId,
    'owner': ?owner,
    if (label.isNotEmpty) 'label': label,
    'mine': mine,
    'battery': battery,
    'lost': lost.map((e) => e.toJson()).toList(),
    'destroyed': destroyed,
  };
}

/// So many units of one kind, ship or defence.
class UnitCount {
  final ShipyardItem unit;
  final int count;

  const UnitCount({required this.unit, required this.count});

  factory UnitCount.fromJson(Object? json) {
    final m = _obj(json);
    return UnitCount(unit: ShipyardItem.fromJson(m['unit']), count: _int(m['count']));
  }

  Json toJson() => {'unit': unit.toJson(), 'count': count};

  List<Object?> get _props => [unit, count];

  @override
  bool operator ==(Object other) => other is UnitCount && _same(_props, other._props);

  @override
  int get hashCode => Object.hashAll(_props);
}

/// A group took up a fleet's trail, went on along it, or gave it up. A group
/// that catches its fleet says so with `CombatStarted`. It reaches the
/// fleet's owner and anyone present in `sector`.
class PursuitEvent extends EventKind {
  final int groupId;

  /// What the group is ("3x cruiser pack").
  final String label;

  /// The fleet followed and the empire owning it.
  final int fleetId;
  final String owner;

  /// The fleet is the receiver's.
  final bool mine;

  /// Where the group stands.
  final Hex sector;
  final PursuitPhase phase;

  /// The sector it jumps to and the ticks until it lands there: where the
  /// fleet stands for a pursuit begun or continued, the way back to its
  /// own sector for one given up. Absent when it gave up at home.
  final Hex? to;
  final int? arrivesIn;

  /// Jumps it will still make after this one before it turns back.
  final int hexesLeft;

  /// Why it gave up, in the engine's words; absent otherwise.
  final String? why;

  const PursuitEvent({
    required this.groupId,
    required this.label,
    required this.fleetId,
    required this.owner,
    this.mine = false,
    required this.sector,
    required this.phase,
    this.to,
    this.arrivesIn,
    required this.hexesLeft,
    this.why,
  });

  factory PursuitEvent.fromJson(Object? json) {
    final m = _obj(json);
    return PursuitEvent(
      groupId: _int(m['group_id']),
      label: _string(m['label']),
      fleetId: _int(m['fleet_id']),
      owner: _string(m['owner']),
      mine: _opt(m['mine'], _bool) ?? false,
      sector: Hex.fromJson(m['sector']),
      phase: PursuitPhase.fromJson(m['phase']),
      to: _opt(m['to'], Hex.fromJson),
      arrivesIn: _opt(m['arrives_in'], _int),
      hexesLeft: _int(m['hexes_left']),
      why: _opt(m['why'], _string),
    );
  }

  @override
  Json toJson() => {
    'kind': 'Pursuit',
    'group_id': groupId,
    'label': label,
    'fleet_id': fleetId,
    'owner': owner,
    'mine': mine,
    'sector': sector.toJson(),
    'phase': phase.toJson(),
    'to': ?to?.toJson(),
    'arrives_in': ?arrivesIn,
    'hexes_left': hexesLeft,
    'why': ?why,
  };
}

enum PursuitPhase {
  /// The fight broke off and the group set out after the fleet.
  began('began'),

  /// It landed where the fleet had been and jumped on after it.
  continued('continued'),

  /// It turned back.
  abandoned('abandoned');

  const PursuitPhase(this.wire);

  final String wire;

  static PursuitPhase fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// Ships a fleet lost outside a fight: those that broke up in an emergency
/// jump. A fight's losses are in `BattleRound` and `CombatEnded`, and a
/// raid's in `RaidResolved` as well.
class ShipsLostEvent extends EventKind {
  final int fleetId;
  final Hex sector;

  /// Empire that owned the ships.
  final String owner;

  /// The ships were the receiver's.
  final bool mine;
  final List<ShipCount> ships;

  const ShipsLostEvent({
    required this.fleetId,
    required this.sector,
    required this.owner,
    this.mine = false,
    required this.ships,
  });

  factory ShipsLostEvent.fromJson(Object? json) {
    final m = _obj(json);
    return ShipsLostEvent(
      fleetId: _int(m['fleet_id']),
      sector: Hex.fromJson(m['sector']),
      owner: _string(m['owner']),
      mine: _opt(m['mine'], _bool) ?? false,
      ships: _list(m['ships'], ShipCount.fromJson),
    );
  }

  @override
  Json toJson() => {
    'kind': 'ShipsLost',
    'fleet_id': fleetId,
    'sector': sector.toJson(),
    'owner': owner,
    'mine': mine,
    'ships': ships.map((e) => e.toJson()).toList(),
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

  /// What an NPC group was ("3x cruiser pack"); empty for a player fleet.
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

  /// What the hostile group is ("3x cruiser pack").
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

/// A fight is over. `owners` lists the empires that fought. `salvage` is the
/// pile the hostiles' destruction left in the sector (absent when they were
/// not destroyed); it despawns at `salvage_despawn_tick`. When the fight was
/// a raid, `raid` is set and the `RaidResolved` of the same tick says what
/// was rebuilt and what was taken.
class CombatEndedEvent extends EventKind {
  final Hex sector;

  /// `outcome` is `HostilesDestroyed`.
  final bool playerVictory;
  final BattleOutcome outcome;

  /// The fight was a raid on a homeworld or an outpost.
  final bool raid;

  /// Rounds fought.
  final int rounds;

  /// What every fleet and group that fought lost, fleets first.
  final List<BattleLosses> losses;
  final List<String> owners;
  final bool mine;
  final Resources? salvage;
  final int? salvageDespawnTick;

  const CombatEndedEvent({
    required this.sector,
    required this.playerVictory,
    required this.outcome,
    this.raid = false,
    required this.rounds,
    required this.losses,
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
      outcome: BattleOutcome.fromJson(m['outcome']),
      raid: _opt(m['raid'], _bool) ?? false,
      rounds: _int(m['rounds']),
      losses: _list(m['losses'], BattleLosses.fromJson),
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
    'outcome': outcome.toJson(),
    'raid': raid,
    'rounds': rounds,
    'losses': losses.map((e) => e.toJson()).toList(),
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
/// toward the player's outpost when `outpost` names its sector. A fleet may
/// leave before it arrives; one that stays is in the fight.
class RaidIncomingEvent extends EventKind {
  final int playerId;
  final int arrivalTick;

  /// `odds.label` in a word, read off the fight as it would go against
  /// what stands there now: "light" (the raiders are destroyed), "moderate"
  /// (the defence lasts the rounds out) or "heavy" (it may be lost).
  final String threat;

  /// The raid fleet's power, fixed at forecast time, on the scale
  /// `FleetState.power` uses.
  final double estPower;

  /// What it flies, by class, fixed at forecast time.
  final List<ShipCount> ships;

  /// How it would end against what stands there now. The same figures
  /// follow in `HomeworldState.raid` and `OutpostState.raid` every tick
  /// until it arrives, as what stands there changes.
  final RaidOdds? odds;

  /// The outpost the raid is bound for; absent for a raid on the homeworld.
  final Hex? outpost;

  const RaidIncomingEvent({
    required this.playerId,
    required this.arrivalTick,
    required this.threat,
    this.estPower = 0,
    this.ships = const [],
    this.odds,
    this.outpost,
  });

  factory RaidIncomingEvent.fromJson(Object? json) {
    final m = _obj(json);
    return RaidIncomingEvent(
      playerId: _int(m['player_id']),
      arrivalTick: _int(m['arrival_tick']),
      threat: _string(m['threat']),
      estPower: _opt(m['est_power'], _double) ?? 0,
      ships: _opt(m['ships'], (v) => _list(v, ShipCount.fromJson)) ?? const [],
      odds: _opt(m['odds'], RaidOdds.fromJson),
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
    'ships': ships.map((e) => e.toJson()).toList(),
    'odds': ?odds?.toJson(),
    'outpost': ?outpost?.toJson(),
  };
}

/// How a raid on the homeworld ended, or on the player's outpost when
/// `outpost` names its sector. The fight itself was told round by round
/// (`BattleRound`) and ends with a `CombatEnded` in the same tick; a raid
/// that found nothing standing in its way was lost without a round, has no
/// `outcome` and no `CombatEnded`.
class RaidResolvedEvent extends EventKind {
  final int playerId;

  /// The outpost that was raided; absent for a raid on the homeworld.
  final Hex? outpost;

  /// Repelled: no raider was left, or a defender stood when the rounds
  /// were over. False when no defender was left.
  final bool defended;

  /// How the fight ended: `hostiles_destroyed` and `both_destroyed` (no
  /// raider left), `outlasted`, `fleets_destroyed` (lost), or `fled` and
  /// `withdrawn` (lost: the last defender left). Absent when no round was
  /// fought.
  final BattleOutcome? outcome;

  /// Rounds fought.
  final int rounds;

  /// The raid broke the outpost: it makes nothing, fills no tank and has
  /// no platform until it has repaired itself from its store. Always
  /// false for a raid on the homeworld.
  final bool breached;

  /// What a lost raid took: its share of the stockpile above the
  /// protected floor, or of the outpost's store. What the ships destroyed
  /// in the fight carried was in the stockpile or the store by then, as
  /// far as it had room.
  final Resources resourcesLost;

  /// The wreckage of a raid destroyed to the last ship at a homeworld;
  /// absent otherwise, and always for a raid on an outpost.
  final Resources? salvageDropped;

  /// Tick the pile despawns (absent when none dropped). A fleet docked at
  /// home scoops it into storage on its own; without one, an alert warns
  /// when it has `HOME_SALVAGE_WARN_TICKS` left.
  final int? salvageDespawnTick;

  /// The power the raid was announced with, and what stood in its way
  /// when it arrived on the same scale. Neither decided anything.
  final double raidPower;
  final double defensePower;

  /// What the raid flew in with and what it lost, by class.
  final List<ShipCount> raiders;
  final List<ShipCount> raidersLost;

  /// Everything the defender lost in the fight, by unit: ships, Solar
  /// Satellites and defences, the satellites and the defences before any
  /// is rebuilt.
  final List<UnitCount> lost;

  /// The Solar Satellites and the defences rebuilt at once and for
  /// nothing when the fight ended, by unit, the satellites first: about
  /// `DEFENCE_REBUILD_PCT` in a hundred of those that fell. Ships are not
  /// rebuilt, and nothing is at an outpost.
  final List<UnitCount> rebuilt;

  /// Solar Satellites destroyed in the fight, before any is rebuilt; how
  /// many came back is their row of `rebuilt`.
  final int satellitesLost;

  /// Defences destroyed, and how many of them were rebuilt: the sums of
  /// `lost` and of `rebuilt` over the defences.
  final int structuresLost;
  final int structuresRestored;

  /// What the storage buildings kept out of reach of a lost raid. At an
  /// outpost, what its protected floor kept.
  final Resources protectedKept;

  /// Ships of the owner's fleets destroyed in the fight.
  final int shipsLost;

  const RaidResolvedEvent({
    required this.playerId,
    this.outpost,
    required this.defended,
    this.outcome,
    this.rounds = 0,
    this.breached = false,
    required this.resourcesLost,
    this.salvageDropped,
    this.salvageDespawnTick,
    required this.raidPower,
    required this.defensePower,
    this.raiders = const [],
    this.raidersLost = const [],
    this.lost = const [],
    this.rebuilt = const [],
    this.satellitesLost = 0,
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
      outcome: _opt(m['outcome'], BattleOutcome.fromJson),
      rounds: _opt(m['rounds'], _int) ?? 0,
      breached: _opt(m['breached'], _bool) ?? false,
      resourcesLost: Resources.fromJson(m['resources_lost']),
      salvageDropped: _opt(m['salvage_dropped'], Resources.fromJson),
      salvageDespawnTick: _opt(m['salvage_despawn_tick'], _int),
      raidPower: _double(m['raid_power']),
      defensePower: _double(m['defense_power']),
      raiders: _opt(m['raiders'], (v) => _list(v, ShipCount.fromJson)) ?? const [],
      raidersLost: _opt(m['raiders_lost'], (v) => _list(v, ShipCount.fromJson)) ?? const [],
      lost: _opt(m['lost'], (v) => _list(v, UnitCount.fromJson)) ?? const [],
      rebuilt: _opt(m['rebuilt'], (v) => _list(v, UnitCount.fromJson)) ?? const [],
      satellitesLost: _opt(m['satellites_lost'], _int) ?? 0,
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
    'outcome': ?outcome?.toJson(),
    'rounds': rounds,
    'breached': breached,
    'resources_lost': resourcesLost.toJson(),
    'salvage_dropped': ?salvageDropped?.toJson(),
    'salvage_despawn_tick': ?salvageDespawnTick,
    'raid_power': raidPower,
    'defense_power': defensePower,
    'raiders': raiders.map((e) => e.toJson()).toList(),
    'raiders_lost': raidersLost.map((e) => e.toJson()).toList(),
    'lost': lost.map((e) => e.toJson()).toList(),
    'rebuilt': rebuilt.map((e) => e.toJson()).toList(),
    'satellites_lost': satellitesLost,
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

/// The drive a ship flies on: the research that speeds it, and by how much a level.
enum Drive {
  combustion('Combustion', label: 'Combustion Drive', speedPerLevel: 0.1),
  impulse('Impulse', label: 'Impulse Drive', speedPerLevel: 0.2),
  hyperspace('Hyperspace', label: 'Hyperspace Drive', speedPerLevel: 0.3);

  const Drive(this.wire, {required this.label, required this.speedPerLevel});

  final String wire;
  final String label;
  final double speedPerLevel;

  static Drive fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// OGame's ships, in OGame's order, without the Colony Ship
/// (`docs/design/ogame.md`, sections 5.3 and 6.1).
enum ShipClass {
  smallCargo(
    'SmallCargo',
    label: 'Small Cargo',
    hull: 400,
    shield: 10,
    weapon: 5,
    cargo: 5000,
    power: 46,
    speed: 5000,
    fuel: 10,
    tankJumps: 12,
    harvest: 1,
    drive: 'Combustion Drive',
  ),
  largeCargo(
    'LargeCargo',
    label: 'Large Cargo',
    hull: 1200,
    shield: 25,
    weapon: 5,
    cargo: 25000,
    power: 127.5,
    speed: 7500,
    fuel: 50,
    tankJumps: 12,
    harvest: 1,
    drive: 'Combustion Drive',
  ),
  lightFighter(
    'LightFighter',
    label: 'Light Fighter',
    hull: 400,
    shield: 10,
    weapon: 50,
    cargo: 50,
    power: 91,
    speed: 12500,
    fuel: 20,
    tankJumps: 10,
    harvest: 0.5,
    drive: 'Combustion Drive',
  ),
  heavyFighter(
    'HeavyFighter',
    label: 'Heavy Fighter',
    hull: 1000,
    shield: 25,
    weapon: 150,
    cargo: 100,
    power: 252.5,
    speed: 10000,
    fuel: 75,
    tankJumps: 10,
    harvest: 0.5,
    drive: 'Impulse Drive',
  ),
  cruiser(
    'Cruiser',
    label: 'Cruiser',
    hull: 2700,
    shield: 50,
    weapon: 400,
    cargo: 800,
    power: 675,
    speed: 15000,
    fuel: 300,
    tankJumps: 8,
    harvest: 0.5,
    drive: 'Impulse Drive',
  ),
  battleship(
    'Battleship',
    label: 'Battleship',
    hull: 6000,
    shield: 200,
    weapon: 1000,
    cargo: 1500,
    power: 1620,
    speed: 10000,
    fuel: 500,
    tankJumps: 6,
    harvest: 0.5,
    drive: 'Hyperspace Drive',
  ),
  recycler(
    'Recycler',
    label: 'Recycler',
    hull: 1600,
    shield: 10,
    weapon: 1,
    cargo: 20000,
    power: 162,
    speed: 2000,
    fuel: 300,
    tankJumps: 12,
    harvest: 5,
    drive: 'Combustion Drive',
  ),
  espionageProbe(
    'EspionageProbe',
    label: 'Espionage Probe',
    hull: 100,
    shield: 0,
    weapon: 0,
    cargo: 5,
    power: 10,
    speed: 100000000,
    fuel: 1,
    tankJumps: 20,
    harvest: 0.5,
    drive: 'Combustion Drive',
  ),
  bomber(
    'Bomber',
    label: 'Bomber',
    hull: 7500,
    shield: 500,
    weapon: 1000,
    cargo: 500,
    power: 1800,
    speed: 4000,
    fuel: 1000,
    tankJumps: 6,
    harvest: 0.5,
    drive: 'Impulse Drive',
  ),
  solarSatellite(
    'SolarSatellite',
    label: 'Solar Satellite',
    hull: 200,
    shield: 1,
    weapon: 1,
    cargo: 0,
    power: 21.100000381469727,
    speed: 0,
    fuel: 0,
    tankJumps: 0,
    harvest: 0,
    drive: '',
  ),
  destroyer(
    'Destroyer',
    label: 'Destroyer',
    hull: 11000,
    shield: 500,
    weapon: 2000,
    cargo: 2000,
    power: 3150,
    speed: 5000,
    fuel: 1000,
    tankJumps: 6,
    harvest: 0.5,
    drive: 'Hyperspace Drive',
  ),
  deathstar(
    'Deathstar',
    label: 'Deathstar',
    hull: 900000,
    shield: 50000,
    weapon: 200000,
    cargo: 1000000,
    power: 295000,
    speed: 100,
    fuel: 1,
    tankJumps: 6,
    harvest: 0.5,
    drive: 'Hyperspace Drive',
  ),
  battlecruiser(
    'Battlecruiser',
    label: 'Battlecruiser',
    hull: 7000,
    shield: 400,
    weapon: 700,
    cargo: 750,
    power: 1440,
    speed: 10000,
    fuel: 250,
    tankJumps: 8,
    harvest: 0.5,
    drive: 'Hyperspace Drive',
  );

  const ShipClass(
    this.wire, {
    required this.label,
    required this.hull,
    required this.shield,
    required this.weapon,
    required this.cargo,
    required this.power,
    required this.speed,
    required this.fuel,
    required this.tankJumps,
    required this.harvest,
    required this.drive,
  });

  final String wire;
  final String label;
  final double hull;
  final double shield;
  final double weapon;
  final double cargo;
  final double power;
  final double speed;
  final double fuel;
  final double tankJumps;
  final double harvest;
  final String drive;

  static ShipClass fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// Amounts of the three resources. An amount is 64-bit wherever it is held,
/// carried, priced or sent, and so is a fleet's fuel: a 32-bit float steps by
/// a quarter at two and a half million, and a mine adds less than that a tick.
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
  metal('metal', label: 'metal', weight: 1, storage: 'Metal Storage'),
  crystal('crystal', label: 'crystal', weight: 1.5, storage: 'Crystal Storage'),
  deuterium('deuterium', label: 'deuterium', weight: 2, storage: 'Deuterium Tank');

  const ResourceKind(this.wire, {required this.label, required this.weight, required this.storage});

  final String wire;
  final String label;
  final double weight;
  final String storage;

  static ResourceKind fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

enum BuildingType {
  metalMine('MetalMine', label: 'Metal Mine'),
  crystalMine('CrystalMine', label: 'Crystal Mine'),
  deuteriumSynthesizer('DeuteriumSynthesizer', label: 'Deuterium Synthesizer'),
  solarPlant('SolarPlant', label: 'Solar Plant'),
  fusionReactor('FusionReactor', label: 'Fusion Reactor'),
  roboticsFactory('RoboticsFactory', label: 'Robotics Factory'),
  naniteFactory('NaniteFactory', label: 'Nanite Factory'),
  shipyard('Shipyard', label: 'Shipyard'),
  metalStorage('MetalStorage', label: 'Metal Storage'),
  crystalStorage('CrystalStorage', label: 'Crystal Storage'),
  deuteriumTank('DeuteriumTank', label: 'Deuterium Tank'),
  researchLab('ResearchLab', label: 'Research Lab');

  const BuildingType(this.wire, {required this.label});

  final String wire;
  final String label;

  static BuildingType fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

enum ResearchType {
  espionageTechnology('EspionageTechnology', label: 'Espionage Technology'),
  computerTechnology('ComputerTechnology', label: 'Computer Technology'),
  weaponsTechnology('WeaponsTechnology', label: 'Weapons Technology'),
  shieldingTechnology('ShieldingTechnology', label: 'Shielding Technology'),
  armourTechnology('ArmourTechnology', label: 'Armour Technology'),
  energyTechnology('EnergyTechnology', label: 'Energy Technology'),
  hyperspaceTechnology('HyperspaceTechnology', label: 'Hyperspace Technology'),
  combustionDrive('CombustionDrive', label: 'Combustion Drive'),
  impulseDrive('ImpulseDrive', label: 'Impulse Drive'),
  hyperspaceDrive('HyperspaceDrive', label: 'Hyperspace Drive'),
  laserTechnology('LaserTechnology', label: 'Laser Technology'),
  ionTechnology('IonTechnology', label: 'Ion Technology'),
  plasmaTechnology('PlasmaTechnology', label: 'Plasma Technology'),
  gravitonTechnology('GravitonTechnology', label: 'Graviton Technology');

  const ResearchType(this.wire, {required this.label});

  final String wire;
  final String label;

  static ResearchType fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// The Weapons, Shielding and Armour Technology a force fights with. A fleet
/// carries the levels it last docked at home with; a defence and a Solar
/// Satellite stand at home and are always at their owner's.
class Refit {
  final int weapons;
  final int shielding;
  final int armour;

  const Refit({required this.weapons, required this.shielding, required this.armour});

  factory Refit.fromJson(Object? json) {
    final m = _obj(json);
    return Refit(weapons: _int(m['weapons']), shielding: _int(m['shielding']), armour: _int(m['armour']));
  }

  Json toJson() => {'weapons': weapons, 'shielding': shielding, 'armour': armour};

  List<Object?> get _props => [weapons, shielding, armour];

  @override
  bool operator ==(Object other) => other is Refit && _same(_props, other._props);

  @override
  int get hashCode => Object.hashAll(_props);
}

/// OGame's defences, without the two missiles, built from the shipyard
/// queue. They never move, burn no fuel, and a raid can wear them down.
enum DefenceKind {
  rocketLauncher('RocketLauncher', label: 'Rocket Launcher', hull: 200, shield: 20, weapon: 80, power: 102),
  lightLaser('LightLaser', label: 'Light Laser', hull: 200, shield: 25, weapon: 100, power: 122.5),
  heavyLaser('HeavyLaser', label: 'Heavy Laser', hull: 800, shield: 100, weapon: 250, power: 340),
  gaussCannon('GaussCannon', label: 'Gauss Cannon', hull: 3500, shield: 200, weapon: 1100, power: 1470),
  ionCannon('IonCannon', label: 'Ion Cannon', hull: 800, shield: 500, weapon: 150, power: 280),
  plasmaTurret('PlasmaTurret', label: 'Plasma Turret', hull: 10000, shield: 300, weapon: 3000, power: 4030),
  smallShieldDome('SmallShieldDome', label: 'Small Shield Dome', hull: 2000, shield: 2000, weapon: 1, power: 401),
  largeShieldDome('LargeShieldDome', label: 'Large Shield Dome', hull: 10000, shield: 10000, weapon: 1, power: 2001);

  const DefenceKind(
    this.wire, {
    required this.label,
    required this.hull,
    required this.shield,
    required this.weapon,
    required this.power,
  });

  final String wire;
  final String label;
  final double hull;
  final double shield;
  final double weapon;
  final double power;

  static DefenceKind fromJson(Object? json) => _named(values, (v) => v.wire, json);

  String toJson() => wire;
}

/// What the shipyard queue builds: a ship class or a defence.
/// On the wire it is just the name ("Cruiser", "RocketLauncher").
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
/// is `outcome_ratio` of the fight itself played forward (`iac_sim::odds`),
/// so it counts rapid fire, the bounce, a shield that is full again every
/// round, the damage a fleet carries and what each side fights with; between
/// equal ship classes it is numbers against numbers. Damage exchange is
/// quadratic in fleet size, so the bands sit close together around 1.0 and
/// a fight is decided long before the ratio reaches 2.
enum RatioLabel {
  deadly('deadly', label: 'DEADLY', outcome: 'the fleet is lost unless it leaves'),
  risky('risky', label: 'RISKY', outcome: 'as likely lost as won, fought to the end'),
  even(
    'even',
    label: 'EVEN',
    outcome: 'a win three times in four or better, at the cost of a third of the fleet or more',
  ),
  favourable('favourable', label: 'FAVOURABLE', outcome: 'a win that keeps two thirds to nine tenths of the fleet'),
  safe('safe', label: 'SAFE', outcome: 'a win that keeps nine tenths of the fleet');

  const RatioLabel(this.wire, {required this.label, required this.outcome});

  final String wire;
  final String label;
  final String outcome;

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

/// Ratio at and above which a fight is SAFE: a win that keeps nine tenths
/// of the fleet. Both sides fire at once, so no fight against armed ships
/// is free. What each band means was measured on fleets of one class
/// against the lairs of thirteen rings, sixty fights a pairing: SAFE won
/// 58 to 60 of them and kept 0.85 or more of its hull.
const double ratioSafe = 2.5;

/// FAVOURABLE from here: won 59 or 60 of 60 and kept 0.63 to 0.91.
const double ratioFavourable = 1.5;

/// EVEN from here: won 45 to 60 of 60 and kept 0.36 to 0.90 on average,
/// the fights it lost counted as nothing kept.
const double ratioEven = 1.100000023841858;

/// RISKY from here: won 10 to 51 of 60. Below it (DEADLY) the fleet won 25
/// of 60 at best, most pairings none, and kept under a fifth.
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
const double storageNearCapRatio = 0.85;

/// Active scan: hops revealed from the scanning fleet's position.
/// Fleets carrying an Espionage Probe get the extended range; signals are
/// faint contacts detected one hop beyond the reveal radius.
const scanBaseRange = 1;

/// The scan range of a fleet with an Espionage Probe aboard.
const scanProbeRange = 2;

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
const double fuelDeutPerUnit = 0.08;

/// The share of the dock's rate that `refine` loses: deuterium turned into
/// fuel from a fleet's hold yields `scaling::refined_fuel_per_deuterium`.
/// Provisional, and the owner's ("like 15%"): it trades the reach of a fleet
/// with cargo ships against the worth of the docks as places to fill up.
const double refineLoss = 0.15;

/// The most rounds a raid is fought for: raiders came for the stockpile and
/// leave if they cannot reach it. OGame's. It trades a defence that can
/// outlast a raid against one that must beat it.
const raidRounds = 6;

/// Of the defences a raid destroys, this many in a hundred are rebuilt at
/// once and for nothing when it is over: where fewer than
/// `DEFENCE_REBUILD_EACH_BELOW` of a kind fell each has that chance, and
/// where more fell a share within `DEFENCE_REBUILD_SPREAD` either side of
/// it is rolled for the kind. OGame's. It trades what defences cost to hold
/// against what ships cost. The Solar Satellites a raid destroys at a
/// homeworld are rebuilt by the same rule, as a kind of their own, which
/// is not OGame's: there a satellite that falls is gone.
const defenceRebuildPct = 70;

const defenceRebuildSpread = 10;

const defenceRebuildEachBelow = 10;

/// The most ships one fleet holds. It bounds the worst round of a fight.
const maxFleetShips = 5000;

/// The fight (`docs/design/ogame.md`, section 7.1). A unit whose shield a
/// shot has left at zero and whose hull stands at or under this many
/// hundredths of full explodes with probability one minus its hull's share.
/// OGame's.
const explodeBelowPct = 70;

/// A fleet under orders whose hull has fallen below this share of full goes
/// home to be mended before it does anything else. Provisional.
const double policyHullReady = 0.8500000238418579;

const policyDisengageRatioX10Min = 1;

const policyDisengageRatioX10Max = 100;

/// Flight (`docs/design/ogame.md`, section 7.4). A fleet that orders a jump
/// out of a fight stops firing and is fired on until it is gone: for one
/// round fewer than its jump takes ticks, and never fewer than this.
/// Provisional: the least any flight costs.
const fleeMinRounds = 1;

/// The jump out of a fight burns this many times a jump's fuel. Provisional;
/// the emergency recall's is `RECALL_FUEL_MULTIPLIER`.
const double fleeFuelMultiplier = 1;

/// A group follows only what it was beating: its power when the fight broke
/// off must be at least this many times the fleet's. Provisional.
const double chaseMinRatio = 1;
