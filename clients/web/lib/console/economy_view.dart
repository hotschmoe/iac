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

class StorageView {
  final Map<String, double> cap;
  final Map<String, double> protected;
  final Map<String, int?> fullInS;
  final Set<String> capped;
  final int vaultLevel;
  const StorageView(this.cap, this.protected, this.fullInS, this.capped, this.vaultLevel);

  /// Seconds to full from [stock] at [perTick]; null when not filling.
  int? fullIn(String k, double stock, double perTick) {
    final room = (cap[k] ?? 0) - stock;
    if (room <= 0) return 0;
    if (perTick <= 0) return null;
    return (room / perTick).ceil();
  }
}

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

  /// What opens one more building slot, while the tech that adds them has a
  /// level left; null when it has none.
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
  final int restoring;

  /// Power of one structure at the current Defense Grid level.
  final double unitPower;
  final proto.DefenceOption? option;
  const DefenceRow(this.kind, this.count, this.restoring, this.unitPower, this.option);

  /// What the structures standing add to the home defence.
  double get power => count * unitPower;
}

class DefenceView {
  final double total;
  final double structures;
  final double raidEstimate;

  /// Ticks until the announced raid lands, or null when none is announced.
  final int? raidInTicks;
  final List<DefenceRow> rows;
  const DefenceView(this.total, this.structures, this.raidEstimate, this.raidInTicks, this.rows);

  /// Docked fleet and the Defense Grid: what is not a standing structure.
  double get other => math.max(0, total - structures);
  double get ratio => total / math.max(1, raidEstimate);
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
    final raid = s.raid;
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

    int lvl(proto.BuildingType t) => hw.buildings.where((b) => b.buildingType == t).map((b) => b.level).fold(0, math.max);
    final st = hw.storage;
    final storage = StorageView(
      {'metal': st.cap.metal, 'crystal': st.cap.crystal, 'deut': st.cap.deuterium},
      {'metal': st.protected.metal, 'crystal': st.protected.crystal, 'deut': st.protected.deuterium},
      {'metal': st.fullInS.metal, 'crystal': st.fullInS.crystal, 'deut': st.fullInS.deuterium},
      {
        for (final k in st.capped)
          switch (k) {
            proto.ResourceKind.metal => 'metal',
            proto.ResourceKind.crystal => 'crystal',
            proto.ResourceKind.deuterium => 'deut',
          },
      },
      lvl(proto.BuildingType.storageVault),
    );

    final tick = s.tick;
    QueueSlotView slot(proto.BuildQueueItem q) {
      final total = math.max(1, q.endTick - q.startTick);
      return QueueSlotView(q.buildingType.label, q.targetLevel, ((tick - q.startTick) / total).clamp(0.0, 1.0), math.max(0, q.endTick - tick), q.startTick);
    }

    // The engine's `scaling::building_slots` gives one slot for each level of Modular Fabrication.
    final fab = s.catalog?.research.where((r) => r.tech == proto.ResearchType.modularFabrication).firstOrNull;
    final fabNext = fab?.next;
    final unmet = [for (final r in fab?.requires ?? const <proto.Requirement>[]) if (!r.met) r.label];
    final slots = SlotsView(
      depth: hw.buildQueue.length + hw.buildPending.length,
      maxDepth: hw.buildSlots + hw.queueWaitingMax,
      singleMaxDepth: 1 + hw.queueWaitingMax,
      slotCount: hw.buildSlots,
      running: [for (final q in hw.buildQueue) slot(q)],
      queued: [for (final q in hw.buildPending) QueuedItemView('${q.buildingType.label} Lv.${q.targetLevel}', waitSummary(q.waitingOn, q.waitingFor, q.startIn))],
      shipQueued: [for (final q in hw.shipyardPending) QueuedItemView('${q.item.label} x${q.count}', batchWaitSummary(q.waitingOn, q.startIn))],
      researchQueued: [for (final q in hw.researchPending) QueuedItemView('${q.tech.label} Lv.${q.targetLevel}', waitSummary(q.waitingOn, q.waitingFor, q.startIn))],
      nextSlot: fab == null || fabNext == null
          ? null
          : 'Needs ${fab.tech.label} L${fabNext.level}${unmet.isEmpty ? '' : ' (requires ${unmet.join(', ')})'}',
    );

    final rows = [
      for (final d in hw.defences)
        DefenceRow(d.kind, d.count, d.restoring, d.power, s.catalog?.defences.where((o) => o.kind == d.kind).firstOrNull),
    ];
    final defence = DefenceView(
      hw.homeDefencePower,
      rows.fold(0.0, (a, r) => a + r.power),
      raid != null && raid.estPower > 0 ? raid.estPower : hw.nextRaidEstimatePower,
      raid == null ? null : math.max(0, raid.arrivalTick - tick),
      rows,
    );
    return EconomyView(world: world, storage: storage, slots: slots, defence: defence, rank: rank);
  }
}
