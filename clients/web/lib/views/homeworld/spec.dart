import '../../console/intel.dart';
import '../../console/threat.dart';
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
  final proto.ShipClass? ship;
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
    this.ship,
  });

  bool get canQueue => reason == null;
}

String shortfall(proto.Resources cost, proto.Resources stock) {
  final parts = <String>[];
  void add(double need, double have, String n) {
    if (need > have) parts.add('${(need - have).ceil()} more $n');
  }

  add(cost.metal, stock.metal, 'Fe');
  add(cost.crystal, stock.crystal, 'Cr');
  add(cost.deuterium, stock.deuterium, 'De');
  return parts.join(', ');
}

/// Locked, maxed and queue-full block queueing; cost never does: an order
/// that cannot be paid yet waits in line and pays when it starts.
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

const buildingNotes = <proto.BuildingType, String>{
  proto.BuildingType.metalMine: 'Primary metal income.',
  proto.BuildingType.crystalMine: 'Crystal income for research and hulls.',
  proto.BuildingType.deuteriumSynthesizer: 'Deuterium is fleet fuel and a build cost.',
  proto.BuildingType.shipyard: 'Higher levels build ships faster and unlock classes.',
  proto.BuildingType.researchLab: 'Higher levels speed research and unlock techs.',
  proto.BuildingType.fuelDepot: 'Fuel handling at the homeworld.',
  proto.BuildingType.sensorArray: 'Longer sensor reach around home.',
  proto.BuildingType.defenseGrid: 'Raises homeworld defence power.',
  proto.BuildingType.storageVault: 'Raises the stockpile caps and the protected share.',
  proto.BuildingType.fabricator: 'Speeds up shipyard builds.',
};

ItemSpec? specFor(GameController c, {int? index}) {
  final cat = c.state.catalog;
  final i = index ?? c.hwCursor;
  if (cat == null) return null;
  final hw = c.state.hw;
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
        note: buildingNotes[o.buildingType] ?? '',
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
      );
    case HomeworldTab.shipyard:
      if (i < 0 || i >= cat.ships.length) return null;
      final o = cat.ships[i];
      final n = c.shipBatch;
      final total = proto.Resources(
          metal: o.unitCost.metal * n, crystal: o.unitCost.crystal * n, deuterium: o.unitCost.deuterium * n);
      return ItemSpec(
        kind: 'Ship',
        title: o.shipClass.label,
        subtitle: 'BATCH x$n',
        cost: total,
        ticks: o.ticksPerShip * n,
        requires: o.requires,
        reason: whyNot(
            requires: o.requires,
            maxed: false,
            queued: hw == null ? 0 : (hw.shipyardQueue == null ? 0 : 1) + hw.shipyardPending.length,
            capacity: hw == null ? 0 : 1 + hw.queueWaitingMax),
        actionLabel: 'Build x$n',
        note: 'Ships join the dock fleet when built.',
        ship: o.shipClass,
      );
    case HomeworldTab.defence:
      if (i < 0 || i >= cat.defences.length) return null;
      final o = cat.defences[i];
      final n = c.shipBatch;
      final total = proto.Resources(metal: o.unitCost.metal * n, crystal: o.unitCost.crystal * n, deuterium: o.unitCost.deuterium * n);
      return ItemSpec(
        kind: 'Defence',
        title: o.kind.label,
        subtitle: 'BATCH x$n',
        cost: total,
        ticks: o.ticksPerUnit * n,
        requires: o.requires,
        reason: whyNot(
            requires: o.requires,
            maxed: false,
            queued: hw == null ? 0 : (hw.shipyardQueue == null ? 0 : 1) + hw.shipyardPending.length,
            capacity: hw == null ? 0 : 1 + hw.queueWaitingMax),
        actionLabel: 'Build x$n',
        note: 'Built in the shipyard queue. Power ${o.power.toStringAsFixed(0)} each.',
      );
    case HomeworldTab.storage:
      return null;
  }
}

String fmtTicks(int t) => clockFmt(t);

double powerOf(proto.ShipClass c) => shipStats[c]!.power;
