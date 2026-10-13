import 'dart:math' as math;

import '../../console/economy_view.dart';
import '../../console/intel.dart';
import '../../protocol/protocol.dart' as proto;
import '../../state/game_controller.dart';

/// What the inspector shows for the selected card, and whether queueing it
/// is possible. Built from the server catalog; the reason string is the
/// single source for tooltip, caption and keyboard toast.
class ItemSpec {
  final String kind;
  final String title;
  final String subtitle;
  final proto.Resources? cost;
  final int ticks;
  final List<proto.Requirement> requires;
  final String? reason;
  final String actionLabel;
  final String note;

  /// What a level of a building or a research does, now and at the next.
  final List<proto.Effect> effects;
  final proto.ShipOption? ship;
  final proto.DefenceOption? defence;

  /// What else in the catalogue asks for this item: "Cruiser (level 2)".
  final List<String> neededBy;
  const ItemSpec({
    required this.kind,
    required this.title,
    required this.subtitle,
    this.cost,
    this.ticks = 0,
    this.requires = const [],
    this.reason,
    required this.actionLabel,
    this.note = '',
    this.effects = const [],
    this.ship,
    this.defence,
    this.neededBy = const [],
  });

  bool get canQueue => reason == null;

  /// A shipyard order whose batch is the player's to choose: not a defence
  /// of which only so many may stand.
  bool get batched => ship != null || (defence != null && defence!.max == null);
}

/// Locked, maxed and queue-full block queueing. For a building or a tech
/// cost does not: an order that cannot be paid yet waits in line and pays
/// when it starts. A shipyard batch is paid in full when it is ordered, so
/// [specFor] adds the shortfall of the whole batch as its reason.
String? whyNot({
  required List<proto.Requirement> requires,
  required bool maxed,
  required int queued,
  required int capacity,
}) {
  final unmet = [for (final r in requires) if (!r.met) r.label];
  if (unmet.isNotEmpty) return 'Locked: needs ${unmet.join(', ')}';
  if (maxed) return 'Already at maximum level';
  if (capacity > 0 && queued >= capacity) return 'Queue full ($queued/$capacity)';
  return null;
}

/// A figure of [kind] as the console prints it. What the figure is, and its
/// label, are the engine's; the unit it is written in is chosen here.
String effectValue(proto.EffectKind kind, double v) => switch (kind) {
      proto.EffectKind.metalPerTick ||
      proto.EffectKind.crystalPerTick ||
      proto.EffectKind.deuteriumPerTick ||
      proto.EffectKind.fusionBurnPerTick =>
        '${anHour(v)}/h',
      proto.EffectKind.energyUsed ||
      proto.EffectKind.energyMade ||
      proto.EffectKind.buildSlots ||
      proto.EffectKind.storageCap ||
      proto.EffectKind.storageProtected =>
        groupDigits(v.round()),
      proto.EffectKind.sensorHops => hopCount(v.round()),
      proto.EffectKind.buildingSpeed ||
      proto.EffectKind.shipyardSpeed ||
      proto.EffectKind.researchSpeed ||
      proto.EffectKind.tankFactor ||
      proto.EffectKind.attack ||
      proto.EffectKind.shield ||
      proto.EffectKind.hull ||
      proto.EffectKind.speed ||
      proto.EffectKind.jumpFuel ||
      proto.EffectKind.oreYield =>
        'x${trimmed(v)}',
      proto.EffectKind.recallRisk => '-${(v * 100).round()}%',
    };

/// "metal 33.0/h": the first thing a building or a research does, for a tile.
String? effectCaption(List<proto.Effect> effects) => switch (effects.firstOrNull) {
      null => null,
      final e => '${e.kind.label} ${effectValue(e.kind, e.now)}',
    };

/// Everything in [cat] that asks for the item labelled [label], with the level it asks for.
List<String> neededBy(proto.HomeworldCatalog cat, String label) {
  String? asks(List<proto.Requirement> requires) {
    for (final r in requires) {
      if (!r.energy && r.name == label) return 'level ${r.need}';
    }
    return null;
  }

  return [
    for (final o in cat.buildings)
      if (asks(o.requires) case final level?) '${o.buildingType.label} ($level)',
    for (final o in cat.research)
      if (asks(o.requires) case final level?) '${o.tech.label} ($level)',
    for (final o in cat.ships)
      if (asks(o.requires) case final level?) '${o.shipClass.label} ($level)',
    for (final o in cat.defences)
      if (asks(o.requires) case final level?) '${o.kind.label} ($level)',
  ];
}

ItemSpec? specFor(GameController c, {int? index}) {
  final cat = c.state.catalog;
  final i = index ?? c.hwCursor;
  if (cat == null) return null;
  final hw = c.state.hw;
  final yardQueued = hw == null ? 0 : (hw.shipyardQueue == null ? 0 : 1) + hw.shipyardPending.length;
  final yardCapacity = hw == null ? 0 : 1 + hw.queueWaitingMax;
  switch (c.hwTab) {
    case HomeworldTab.buildings:
      if (i < 0 || i >= cat.buildings.length) return null;
      final o = cat.buildings[i];
      return ItemSpec(
        kind: 'Building',
        title: o.buildingType.label,
        subtitle: o.next == null ? 'L${o.level}  MAX' : 'L${o.level} > L${o.next!.level}',
        cost: o.next?.cost,
        ticks: o.next?.ticks ?? 0,
        requires: o.requires,
        reason: whyNot(
            requires: o.requires,
            maxed: o.next == null,
            queued: hw == null ? 0 : hw.buildQueue.length + hw.buildPending.length,
            capacity: hw == null ? 0 : hw.buildSlots + hw.queueWaitingMax),
        actionLabel: o.next == null ? 'Maxed' : 'Queue to L${o.next!.level}',
        effects: o.effects,
        neededBy: neededBy(cat, o.buildingType.label),
      );
    case HomeworldTab.research:
      if (i < 0 || i >= cat.research.length) return null;
      final o = cat.research[i];
      return ItemSpec(
        kind: 'Research',
        title: o.tech.label,
        subtitle: o.next == null ? 'L${o.level}  MAX' : 'L${o.level} > L${o.next!.level}',
        cost: o.next?.cost,
        ticks: o.next?.ticks ?? 0,
        requires: o.requires,
        reason: whyNot(
            requires: o.requires,
            maxed: o.next == null,
            queued: hw == null ? 0 : (hw.researchActive == null ? 0 : 1) + hw.researchPending.length,
            capacity: hw == null ? 0 : 1 + hw.queueWaitingMax),
        actionLabel: o.next == null ? 'Maxed' : 'Research L${o.next!.level}',
        effects: o.effects,
        neededBy: neededBy(cat, o.tech.label),
      );
    case HomeworldTab.shipyard:
      if (i < 0 || i >= cat.ships.length) return null;
      final o = cat.ships[i];
      final n = c.shipBatch;
      final total = batchCost(o.unitCost, n);
      return ItemSpec(
        kind: 'Ship',
        title: o.shipClass.label,
        subtitle: 'BATCH x$n',
        cost: total,
        ticks: o.ticksPerShip * n,
        requires: o.requires,
        reason: whyNot(requires: o.requires, maxed: false, queued: yardQueued, capacity: yardCapacity) ?? shortReason(c.state.stock, total),
        actionLabel: 'Build x$n',
        note: o.flies
            ? 'The whole batch is paid when ordered. Ships join the dock fleet when built.'
            : 'The whole batch is paid when ordered. It stays at the homeworld and joins no fleet: it makes energy, '
                'stands in every raid\'s fight, and one that falls there is not rebuilt.',
        ship: o,
      );
    case HomeworldTab.defence:
      if (i < 0 || i >= cat.defences.length) return null;
      final o = cat.defences[i];
      final n = math.max(1, defenceBatch(o, c.shipBatch));
      final total = batchCost(o.unitCost, n);
      return ItemSpec(
        kind: 'Defence',
        title: o.kind.label,
        subtitle: 'BATCH x$n',
        cost: total,
        ticks: o.ticksPerUnit * n,
        requires: o.requires,
        reason: whyNot(requires: o.requires, maxed: false, queued: yardQueued, capacity: yardCapacity) ??
            limitReason(o) ??
            shortReason(c.state.stock, total),
        actionLabel: 'Build x$n',
        note: 'Built in the shipyard queue, the whole batch paid when ordered. It never moves and burns no fuel.',
        defence: o,
      );
    case HomeworldTab.storage:
      return null;
  }
}

String fmtTicks(int t) => clockFmt(t);
