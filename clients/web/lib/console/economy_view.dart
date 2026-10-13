import 'dart:math' as math;

import '../models/game_state.dart';
import '../protocol/protocol.dart' as proto;
import '../state/state_mapper.dart' show batchWaitSummary, shortBy, waitSummary;

bool canAfford(proto.Resources stock, proto.Resources cost) =>
    stock.metal >= cost.metal && stock.crystal >= cost.crystal && stock.deuterium >= cost.deuterium;

/// What [cost] needs beyond [stock], or null when the stockpile covers it.
proto.Resources? shortfall(proto.Resources stock, proto.Resources cost) => canAfford(stock, cost)
    ? null
    : proto.Resources(
        metal: math.max(0, cost.metal - stock.metal),
        crystal: math.max(0, cost.crystal - stock.crystal),
        deuterium: math.max(0, cost.deuterium - stock.deuterium));

/// "Short by 11 De" when [stock] does not cover [cost], else null. A
/// shipyard batch is paid in full when it is ordered and refused otherwise,
/// so this is the reason its control is off; the overview's suggestion tiles
/// use it for every kind of order.
String? shortReason(proto.Resources stock, proto.Resources cost) {
  final missing = shortfall(stock, cost);
  return missing == null ? null : shortBy(missing, opening: true);
}

/// What [n] units cost at [unit] each.
proto.Resources batchCost(proto.Resources unit, int n) =>
    proto.Resources(metal: unit.metal * n, crystal: unit.crystal * n, deuterium: unit.deuterium * n);

/// How many of [o] a batch of [batch] orders: all of it, or as many as the
/// server says its limit still has room for.
int defenceBatch(proto.DefenceOption o, int batch) => switch (o.room) {
      null => batch,
      final room => math.min(batch, room),
    };

/// Why no more of [o] can be ordered, when its limit is reached; else null.
String? limitReason(proto.DefenceOption o) =>
    o.room == 0 ? 'At most ${o.max} may stand, counting those on order' : null;

/// Presentation model for the economy side of the protocol: pace, storage
/// caps, build slots and queues, defences and raids, score. Views read only
/// this class; [EconomyView.of] is the single mapping from `GameState`
/// (server snapshot) and is null-safe before the first state and for older
/// servers that omit a block.

class WorldView {
  final double pace;
  final String preset;
  const WorldView(this.pace, this.preset);
  String get label => '${preset.toUpperCase()} x${pace == pace.roundToDouble() ? pace.toInt() : pace}';
}

/// The stockpile's limits as the server states them, a resource at a time.
class StorageView {
  final proto.StorageState _state;
  final proto.HomeworldCatalog? _catalog;
  const StorageView(this._state, this._catalog);

  double cap(proto.ResourceKind k) => k.of(_state.cap);
  double protected(proto.ResourceKind k) => k.of(_state.protected);

  /// Ticks until the stockpile of [k] is full, as the server counts them:
  /// 0 when it is, null when it is not filling.
  int? fullIn(proto.ResourceKind k) => _state.fullInS.of(k);

  bool capped(proto.ResourceKind k) => _state.capped.contains(k);

  /// The building that holds [k], with its level and its next.
  proto.BuildingOption? building(proto.ResourceKind k) => _catalog?.storageOf(k);

  /// "Metal Storage L2": the building of [k] and where it stands.
  String buildingTag(proto.ResourceKind k) => '${k.storage} L${building(k)?.level ?? 0}';
}

/// A stockpile as the console lists it: the resource, the unit it is
/// counted in, its name, and the key its widgets carry.
class Stockpile {
  final proto.ResourceKind kind;
  final String unit;
  final String name;
  final String key;
  const Stockpile(this.kind, this.unit, this.name, this.key);
}

const stockpiles = [
  Stockpile(proto.ResourceKind.metal, 'Fe', 'Metal', 'metal'),
  Stockpile(proto.ResourceKind.crystal, 'Cr', 'Crystal', 'crystal'),
  Stockpile(proto.ResourceKind.deuterium, 'De', 'Deuterium', 'deut'),
];

class QueueSlotView {
  final String name;
  final int toLevel;
  final double frac;
  final int etaTicks;
  final int startTick;
  const QueueSlotView(this.name, this.toLevel, this.frac, this.etaTicks, this.startTick);
}

class QueuedItemView {
  final String name;
  final String waits;
  const QueuedItemView(this.name, this.waits);
}

class SlotsView {
  /// Items running or waiting in the building queue, and the server's depth limit.
  final int depth;
  final int maxDepth;

  /// The most the research queue holds, and the shipyard's: each runs one
  /// order, and the server's waiting line stands behind it.
  final int singleMaxDepth;

  /// Buildings that may be under construction at once, as the server counts them.
  final int slotCount;

  /// The buildings under construction, in slot order: at most [slotCount].
  final List<QueueSlotView> running;
  final List<QueuedItemView> queued;
  final List<QueuedItemView> shipQueued;
  final List<QueuedItemView> researchQueued;

  /// What opens one more building slot, in the server's words; null when
  /// every slot is open.
  final String? nextSlot;
  const SlotsView({
    required this.depth,
    required this.maxDepth,
    required this.singleMaxDepth,
    required this.slotCount,
    required this.running,
    required this.queued,
    required this.shipQueued,
    required this.researchQueued,
    required this.nextSlot,
  });
}

/// "A", "B", "C": the name of the building slot at [index].
String slotLetter(int index) => String.fromCharCode(0x41 + index);

class DefenceRow {
  final proto.DefenceKind kind;
  final int count;

  /// Power of one with the owner's research.
  final double unitPower;
  final proto.DefenceOption? option;
  const DefenceRow(this.kind, this.count, this.unitPower, this.option);
}

/// The raid on the homeworld as the server forecasts it, and what stands at
/// home to fight it.
class DefenceView {
  /// The raid being fought, announced, or of typical size: the state's.
  final proto.RaidForecast raid;

  /// Ticks until the announced raid lands; null when none is announced or
  /// one is being fought.
  final int? raidInTicks;

  /// Solar Satellites at home.
  final int satelliteCount;

  /// Defences standing at home, of every kind.
  final int defenceCount;
  final List<DefenceRow> rows;
  const DefenceView({
    required this.raid,
    required this.raidInTicks,
    required this.satelliteCount,
    required this.defenceCount,
    required this.rows,
  });

  /// The raid has arrived and its fight is not over.
  bool get fought => raid.roundsLeft != null;

  /// A raid is on its way or being fought.
  bool get pressing => fought || raidInTicks != null;
}

class RankView {
  final int rank;
  final String tag;
  final double score;

  /// (rank, name, HUM/AI/BOT tag, score, is-you). Built-in bots follow the
  /// ranked rows with rank 0: they are on the board and not in the ranking.
  final List<(int, String, String, int, bool)> board;
  const RankView(this.rank, this.tag, this.score, this.board);
}

class EconomyView {
  final WorldView? world;
  final StorageView? storage;
  final SlotsView? slots;
  final DefenceView? defence;
  final RankView? rank;

  const EconomyView({this.world, this.storage, this.slots, this.defence, this.rank});

  static const empty = EconomyView();

  static EconomyView of(GameState s, {String? me}) {
    final hw = s.hw;
    final w = s.world;
    final lb = s.leaderboard;
    final world = w == null ? null : WorldView(w.pace, w.preset ?? 'custom');
    RankView? rank;
    final you = lb == null
        ? null
        : lb.you ?? lb.entries.where((e) => me != null && e.name.toLowerCase() == me.toLowerCase()).firstOrNull;
    if (lb != null && you != null) {
      rank = RankView(you.rank, you.agent ? 'AI' : 'HUM', you.score, [
        for (final e in lb.entries) (e.rank, e.name, e.agent ? 'AI' : 'HUM', e.score.round(), e.rank == you.rank && e.name == you.name),
        for (final e in lb.bots) (0, e.name, 'BOT', e.score.round(), false),
      ]);
    }
    if (hw == null) return EconomyView(world: world, rank: rank);

    final storage = StorageView(hw.storage, s.catalog);

    final tick = s.tick;
    QueueSlotView slot(proto.BuildQueueItem q) {
      final total = math.max(1, q.endTick - q.startTick);
      return QueueSlotView(q.buildingType.label, q.targetLevel, ((tick - q.startTick) / total).clamp(0.0, 1.0), math.max(0, q.endTick - tick), q.startTick);
    }

    final unlock = hw.buildSlotUnlock;
    final slots = SlotsView(
      depth: hw.buildQueue.length + hw.buildPending.length,
      maxDepth: hw.buildSlots + hw.queueWaitingMax,
      singleMaxDepth: 1 + hw.queueWaitingMax,
      slotCount: hw.buildSlots,
      running: [for (final q in hw.buildQueue) slot(q)],
      queued: [for (final q in hw.buildPending) QueuedItemView('${q.buildingType.label} Lv.${q.targetLevel}', waitSummary(q.waitingOn, q.waitingFor, q.startIn))],
      shipQueued: [for (final q in hw.shipyardPending) QueuedItemView('${q.item.label} x${q.count}', batchWaitSummary(q.waitingOn, q.startIn))],
      researchQueued: [for (final q in hw.researchPending) QueuedItemView('${q.tech.label} Lv.${q.targetLevel}', waitSummary(q.waitingOn, q.waitingFor, q.startIn))],
      nextSlot: unlock == null ? null : 'Needs ${unlock.label} (you have ${unlock.have})',
    );

    final rows = [
      for (final d in hw.defences) DefenceRow(d.kind, d.count, d.power, s.catalog?.defences.where((o) => o.kind == d.kind).firstOrNull),
    ];
    final defence = DefenceView(
      raid: hw.raid,
      raidInTicks: switch (hw.raid.arrivalTick) { null => null, final at => math.max(0, at - tick) },
      satelliteCount: hw.satellites,
      defenceCount: hw.defences.fold(0, (n, d) => n + d.count),
      rows: rows,
    );
    return EconomyView(world: world, storage: storage, slots: slots, defence: defence, rank: rank);
  }
}
