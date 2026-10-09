import 'dart:math' as math;

import '../models/game_state.dart';

/// Presentation model for the economy features whose protocol fields are
/// still being built (branch `econ-core`): pace, storage caps, build slots,
/// defences and raids, score. Views read only this class.
///
/// SEAMS: [EconomyView.live] is where the real protocol maps in
/// (`world`, `homeworld.storage`, `homeworld.slots`, defences, score).
/// Until then it returns nulls and the views hide or de-emphasise the field.
/// [EconomyView.demo] fills every field with plausible numbers for demo mode.

class WorldView {
  final double pace;
  final String preset;
  const WorldView(this.pace, this.preset);
  String get label => '${preset.toUpperCase()} x${pace == pace.roundToDouble() ? pace.toInt() : pace}';
}

class StorageView {
  final Map<String, double> cap;
  final Map<String, double> protected;
  final int vaultLevel;
  const StorageView(this.cap, this.protected, this.vaultLevel);

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
  final int depth;
  final int maxDepth;
  final List<QueuedItemView> queued;
  final bool slotBUnlocked;
  final String slotBRequirement;
  final QueueSlotView? slotB;
  const SlotsView({required this.depth, required this.maxDepth, required this.queued, required this.slotBUnlocked, required this.slotBRequirement, this.slotB});
}

class DefenceStructure {
  final String name;
  final int level;
  final int maxLevel;
  final double power;
  final String needs;
  final String cost;
  const DefenceStructure(this.name, this.level, this.maxLevel, this.power, this.needs, this.cost);
}

class DefenceView {
  final double dockedFleet;
  final double grid;
  final double structures;
  final double raidEstimate;
  final int raidInTicks;
  final List<DefenceStructure> structureList;
  const DefenceView(this.dockedFleet, this.grid, this.structures, this.raidEstimate, this.raidInTicks, this.structureList);
  double get total => dockedFleet + grid + structures;
  double get ratio => total / math.max(1, raidEstimate);
}

class RankView {
  final int rank;
  final String zone;
  final String tag;
  final List<(int, String, String, int)> board;
  const RankView(this.rank, this.zone, this.tag, this.board);
}

class EconomyView {
  final WorldView? world;
  final StorageView? storage;
  final SlotsView? slots;
  final DefenceView? defence;
  final RankView? rank;

  const EconomyView({this.world, this.storage, this.slots, this.defence, this.rank});

  static const empty = EconomyView();

  /// SEAM: map from the real protocol when it lands.
  static EconomyView live(GameState s) => empty;

  /// Demo numbers shaped like the economy spec (Storage Vault L2 at pace 1).
  static EconomyView demo(GameState s) {
    final tick = s.tick;
    final raidIn = 300 - (tick % 600) % 300 + 4;
    return EconomyView(
      world: const WorldView(1, 'persistent'),
      storage: const StorageView({'metal': 11250, 'crystal': 7875, 'deut': 5625}, {'metal': 2500, 'crystal': 1750, 'deut': 1250}, 2),
      slots: const SlotsView(
        depth: 2,
        maxDepth: 3,
        queued: [QueuedItemView('Metal Mine Lv.5', 'waits: 3,420 Fe'), QueuedItemView('Storage Vault Lv.3', 'waits: 5,100 Fe')],
        slotBUnlocked: false,
        slotBRequirement: 'Needs Modular Fabrication (requires Research Lab 4)',
      ),
      defence: DefenceView(30, 23, 112, 120, raidIn, const [
        DefenceStructure('Pulse Turret', 3, 10, 36, 'Defense Grid 1', '120 Fe 40 Cr'),
        DefenceStructure('Lancer Battery', 1, 10, 40, 'Defense Grid 2, Weapons 2', '400 Fe 160 Cr'),
        DefenceStructure('Ion Bastion', 0, 10, 0, 'Defense Grid 4, Advanced Shields 2', '900 Fe 500 Cr 120 De'),
        DefenceStructure('Aegis Dome', 0, 5, 0, 'Defense Grid 5, Storage Vault 3', '1,800 Fe 1,100 Cr 400 De'),
      ]),
      rank: const RankView(14, 'Inner Ring', 'HUM', [
        (1, 'Wraith-7', 'AI', 9120),
        (2, 'Kestrel', 'HUM', 7410),
        (3, 'Orrery', 'AI', 6955),
        (14, 'Pilot', 'HUM', 1840),
      ]),
    );
  }
}
