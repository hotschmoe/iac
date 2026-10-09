import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/protocol/protocol.dart' as p;
import 'package:iac_client/state/demo_provider.dart';

const s0 = p.Hex(6, -4);
const north = p.Hex(6, -5); // T3 corvette patrol
const nebula = p.Hex(7, -5); // derelict tier 1
const debris = p.Hex(7, -4); // salvage pile
const nw = p.Hex(5, -4); // T5, uncharted
const passive = p.Hex(7, -3);

class Rig {
  final List<p.GameEvent> events = [];
  final List<p.TickUpdate> ticks = [];
  late final DemoProvider demo;
  late p.GameState initial;
  p.FleetState? lastFleet(int id) {
    for (final t in ticks.reversed) {
      for (final f in t.fleets) {
        if (f.id == id) return f;
      }
    }
    return initial.fleets.where((f) => f.id == id).firstOrNull;
  }

  p.PlayerState get player => ticks.isEmpty ? initial.player : ticks.last.player!;

  Rig() {
    demo = DemoProvider(_take);
    initial = demo.initial();
  }

  void _take(List<p.ServerMessage> msgs) {
    for (final m in msgs) {
      if (m is p.TickUpdate) {
        ticks.add(m);
        events.addAll(m.events ?? const []);
      }
    }
  }

  List<p.ServerMessage> send(p.Command c) {
    final r = demo.handle(p.CommandMessage(c));
    _take(r);
    return r;
  }

  void step([int n = 1]) {
    for (var i = 0; i < n; i++) {
      demo.step();
    }
  }

  Iterable<T> of<T extends p.EventKind>() => events.map((e) => e.kind).whereType<T>();

  p.ErrorCode? errorOf(List<p.ServerMessage> r) => r.whereType<p.ErrorMessage>().firstOrNull?.code;

  void move(int fleet, p.Hex to) {
    final r = send(p.MoveCommand(fleetId: fleet, target: to));
    expect(errorOf(r), isNull, reason: 'move to $to');
    step(2);
  }
}

p.SectorState sec(Rig r, p.Hex h) => r.initial.knownSectors.firstWhere((s) => s.location == h);

void main() {
  test('opening world: neighbours, hostiles, salvage, derelicts, chart memory', () {
    final r = Rig();
    final known = {for (final s in r.initial.knownSectors) s.location: s};
    expect(r.initial.fleets.map((f) => f.id), [101, 102, 103]);
    expect(r.initial.player.name, 'Demo');
    expect(r.initial.player.resources.metal, closeTo(7340, 1));

    expect(known.containsKey(nw), isFalse, reason: 'T5 sector stays uncharted until scanned');
    expect(known[s0]!.connections, contains(nw));
    expect(known[s0]!.live, isTrue);
    expect(known[north]!.hostiles!.single.ships.single.shipClass, p.ShipClass.corvette);
    expect(known[north]!.hostiles!.single.behavior, p.NpcBehavior.patrol);
    expect(known[passive]!.hostiles!.single.behavior, p.NpcBehavior.passive);
    expect(known[debris]!.salvage, isNotNull);
    expect(known[debris]!.salvageDespawnTick, greaterThan(r.initial.tick));
    expect(known[nebula]!.site!.tier, 1);
    expect(known[const p.Hex(8, -6)]!.site!.risk, p.SiteRisk.hot);
    expect(known[const p.Hex(9, -7)]!.hostiles!.single.behavior, p.NpcBehavior.swarm);

    final ages = [for (final s in known.values) r.initial.tick - s.lastSeen];
    expect(known.values.any((s) => s.live), isTrue);
    expect(ages.reduce((a, b) => a > b ? a : b), greaterThan(3000));
    expect(known.values.where((s) => !s.live && r.initial.tick - s.lastSeen < 600), isNotEmpty);

    final f = r.initial.fleets.first;
    expect(f.jumpFuel, 5);
    expect(f.homeFuel, p.Hex.distance(s0, p.Hex(4, -2)) * 5);
    expect(r.initial.fleets.last.policy, p.PolicyPreset.prospect);
  });

  test('a fight against the T3 patrol resolves with salvage', () {
    final r = Rig();
    final npc = sec(r, north).hostiles!.single;
    final fleetBefore = r.lastFleet(101)!;
    expect(r.errorOf(r.send(p.MoveCommand(fleetId: 101, target: north))), isNull);
    expect(r.errorOf(r.send(p.AttackCommand(fleetId: 101, targetFleetId: npc.id))), isNull);
    expect(r.lastFleet(101)!.state, p.FleetStatus.inCombat);
    r.step(30);
    final kinds = r.events.map((e) => e.kind).toList();
    final started = kinds.indexWhere((k) => k is p.CombatStartedEvent);
    final round = kinds.indexWhere((k) => k is p.CombatRoundEvent);
    final destroyed = kinds.indexWhere((k) => k is p.ShipDestroyedEvent && k.isNpc);
    final fleetGone = kinds.indexWhere((k) => k is p.FleetDestroyedEvent && k.isNpc);
    final ended = kinds.indexWhere((k) => k is p.CombatEndedEvent);
    expect([started, round, destroyed, fleetGone, ended].every((i) => i >= 0), isTrue);
    expect(started, lessThan(round));
    expect(round, lessThan(destroyed));
    expect(destroyed, lessThan(fleetGone));
    expect(fleetGone, lessThan(ended));
    final end = kinds[ended] as p.CombatEndedEvent;
    expect(end.playerVictory, isTrue);
    expect(end.salvage!.metal, greaterThan(0));
    final f = r.lastFleet(101)!;
    expect(f.state, p.FleetStatus.idle);
    expect(f.ships.length, lessThanOrEqualTo(fleetBefore.ships.length));
    final round1 = kinds.whereType<p.CombatRoundEvent>().first;
    expect(round1.damage, closeTo(round1.shieldAbsorbed + round1.hullDamage, 1e-6));
    final updates = [for (final t in r.ticks) ...?t.sectorUpdates?.where((s) => s.location == north)];
    expect(updates.last.hostiles, isNull);
    expect(updates.last.salvage!.metal, greaterThan(0));
  });

  test('collect salvage fills the hold', () {
    final r = Rig();
    r.move(101, debris);
    final before = r.lastFleet(101)!.cargo.total;
    final reply = r.send(const p.CollectSalvageCommand(fleetId: 101));
    expect(r.errorOf(reply), isNull);
    final got = r.of<p.SalvageCollectedEvent>().single;
    expect(got.resources.metal, 40);
    expect(r.lastFleet(101)!.cargo.total, closeTo(before + got.resources.total, 1e-6));
    expect(r.errorOf(r.send(const p.CollectSalvageCommand(fleetId: 101))), p.ErrorCode.invalidTarget);
  });

  test('boarding a quiet derelict pays out after its timer', () {
    final r = Rig();
    r.move(101, nebula);
    expect(r.errorOf(r.send(const p.ExploreSiteCommand(fleetId: 101))), isNull);
    expect(r.lastFleet(101)!.state, p.FleetStatus.exploring);
    expect(r.of<p.SiteExplorationStartedEvent>().single.tier, 1);
    r.step(13);
    final done = r.of<p.SiteExploredEvent>().single;
    expect(done.resources.total, greaterThan(0));
    expect(r.lastFleet(101)!.state, p.FleetStatus.idle);
    expect(r.errorOf(r.send(const p.ExploreSiteCommand(fleetId: 101))), p.ErrorCode.invalidTarget);
  });

  test('scan has a five tick cooldown and reveals the uncharted neighbour', () {
    final r = Rig();
    expect(r.errorOf(r.send(const p.ScanCommand(fleetId: 101))), isNull);
    final f = r.lastFleet(101)!;
    expect(f.cooldownRemaining, 5);
    final scan = r.of<p.ScanCompletedEvent>().single;
    expect(scan.sectorsRevealed, greaterThan(0));
    expect(scan.hostilesDetected, greaterThan(0));
    expect(r.ticks.last.sectorUpdates!.any((s) => s.location == nw && s.live), isTrue);
    expect(r.errorOf(r.send(const p.ScanCommand(fleetId: 101))), p.ErrorCode.onCooldown);
    r.step(5);
    expect(r.errorOf(r.send(const p.ScanCommand(fleetId: 101))), isNull);
  });

  test('moves cost fuel and a tick of cooldown; docking refuels and unloads', () {
    final r = Rig();
    final f0 = r.lastFleet(101)!;
    expect(r.errorOf(r.send(p.MoveCommand(fleetId: 101, target: debris))), isNull);
    expect(r.lastFleet(101)!.fuel, f0.fuel - 5);
    expect(r.errorOf(r.send(p.MoveCommand(fleetId: 101, target: s0))), p.ErrorCode.onCooldown);
    r.step();
    expect(r.errorOf(r.send(p.MoveCommand(fleetId: 101, target: s0))), isNull);
    r.step();
    expect(r.errorOf(r.send(const p.RecallCommand(fleetId: 101))), isNull);
    r.step(12);
    final home = r.lastFleet(101)!;
    expect(home.state, p.FleetStatus.docked);
    expect(home.fuel, home.fuelMax);
    expect(home.cargo.total, 0);
  });

  test('stockpiles clamp at the storage caps and near-cap raises the storage events', () {
    final r = Rig();
    final cap = r.initial.homeworld.storage.cap;
    expect(cap.metal, closeTo(11250, 1e-6));
    r.step(1200);
    final res = r.player.resources;
    expect(res.metal, lessThanOrEqualTo(cap.metal));
    expect(res.crystal, lessThanOrEqualTo(cap.crystal));
    expect(res.deuterium, lessThanOrEqualTo(cap.deuterium));
    expect(r.of<p.StorageNearCapEvent>().any((e) => e.resource == p.ResourceKind.crystal), isTrue);
  });

  test('raid cycle announces with an estimate and resolves against home defence', () {
    final r = Rig();
    r.step(5);
    final inc = r.of<p.RaidIncomingEvent>().first;
    expect(inc.estPower, greaterThan(0));
    expect(r.initial.homeworld.nextRaidEstimatePower, inc.estPower);
    r.step(inc.arrivalTick - r.ticks.last.tick);
    final res = r.of<p.RaidResolvedEvent>().first;
    expect(res.defended, isTrue);
    expect(res.defensePower, greaterThan(res.raidPower));
    expect(r.of<p.RaidIncomingEvent>().length, 2);
  });

  test('opening economy: slot B locked, pending builds wait, defences stand', () {
    final r = Rig();
    final hw = r.initial.homeworld;
    expect(hw.buildSlots, 1);
    expect(hw.buildQueue.length, 1);
    expect(hw.buildPending.length, 2);
    expect(hw.buildPending.every((q) => q.cost.total > 0), isTrue);
    expect(hw.defences.firstWhere((d) => d.kind == p.DefenceKind.pulseTurret).count, 6);
    expect(hw.homeDefencePower, greaterThan(hw.nextRaidEstimatePower));
    expect(r.initial.world.pace, 1);
    expect(r.initial.fleets.every((f) => f.power > 0 && f.rangeHops >= 0), isTrue);
  });

  test('queued items wait behind the running one, then run and a built defence adds power', () {
    final r = Rig();
    final before = r.initial.homeworld.homeDefencePower;
    expect(r.errorOf(r.send(const p.BuildDefenceCommand(kind: p.DefenceKind.pulseTurret, count: 2))), isNull);
    expect(r.ticks.last.homeworldUpdate!.shipyardPending.length, 1);
    r.step(400);
    expect(r.of<p.DefenceBuiltEvent>(), isNotEmpty);
    final hw = r.ticks.last.homeworldUpdate!;
    expect(hw.defences.firstWhere((d) => d.kind == p.DefenceKind.pulseTurret).count, 8);
    expect(hw.homeDefencePower, greaterThan(before));
    expect(hw.shipyardPending, isEmpty);
  });

  test('a waiting item can be cancelled for free', () {
    final r = Rig();
    expect(r.errorOf(r.send(const p.BuildDefenceCommand(kind: p.DefenceKind.pulseTurret, count: 1))), isNull);
    final stock = r.player.resources.metal;
    expect(r.errorOf(r.send(const p.CancelQueuedCommand(queueType: p.QueueType.ship, index: 0))), isNull);
    expect(r.ticks.last.homeworldUpdate!.shipyardPending, isEmpty);
    expect(r.player.resources.metal, stock);
  });

  test('an order that cannot be paid yet is accepted, says what it waits for, and cancels with a confirmation', () {
    final r = Rig();
    expect(r.errorOf(r.send(const p.BuildShipCommand(shipClass: p.ShipClass.scout, count: 200))), isNull,
        reason: 'unaffordable is not a refusal');
    final q = r.ticks.last.homeworldUpdate!.shipyardPending.last;
    expect(q.waitingFor, isNotNull);
    expect(q.waitingOn, p.WaitReason.resources);
    final wait = r.of<p.QueueEvent>().last;
    expect((wait.action, wait.item), (p.QueueAction.waiting, 'Scout x200'));
    final index = r.ticks.last.homeworldUpdate!.shipyardPending.length - 1;
    r.send(p.CancelQueuedCommand(queueType: p.QueueType.ship, index: index));
    final gone = r.of<p.QueueEvent>().last;
    expect((gone.action, gone.item), (p.QueueAction.cancelled, 'Scout x200'));
  });

  test('leaderboard reply mixes humans and agents', () {
    final r = Rig();
    final reply = r.send(const p.LeaderboardCommand()).whereType<p.LeaderboardReply>().single;
    expect(reply.entries.any((e) => e.agent), isTrue);
    expect(reply.entries.any((e) => !e.agent), isTrue);
    expect(reply.entries.map((e) => e.rank), orderedEquals([1, 2, 3, 4, 5, 6]));
  });

  test('aggressive groups engage on arrival and the player can lose', () {
    final r = Rig();
    r.move(101, nw);
    r.step(40);
    expect(r.of<p.CombatStartedEvent>(), isNotEmpty);
    final lost = r.of<p.FleetDestroyedEvent>().where((e) => e.mine);
    final won = r.of<p.CombatEndedEvent>().any((e) => e.playerVictory);
    expect(lost.isNotEmpty || won, isTrue);
    if (lost.isNotEmpty) {
      expect(r.lastFleet(101), isNull);
      expect(r.of<p.CombatEndedEvent>().any((e) => !e.playerVictory), isTrue);
    }
  });

  test('harvest honours the chosen resource and sector density', () {
    final r = Rig();
    final bad = r.send(const p.HarvestCommand(fleetId: 101, resource: p.HarvestResource.deuterium));
    expect(r.errorOf(bad), p.ErrorCode.resourceNotPresent);
    expect(r.errorOf(r.send(const p.HarvestCommand(fleetId: 101, resource: p.HarvestResource.crystal))), isNull);
    final c0 = r.lastFleet(101)!.cargo;
    r.step(10);
    final c1 = r.lastFleet(101)!.cargo;
    expect(c1.crystal, greaterThan(c0.crystal));
    expect(c1.metal, c0.metal);
  });

  test('catalog shows locked, unaffordable and maxed entries', () {
    final r = Rig();
    final cat = r.initial.homeworld.catalog;
    expect(cat.defences.any((d) => d.requires.any((q) => !q.met)), isTrue);
    expect(cat.research.any((o) => o.next == null), isTrue);
    expect(cat.research.any((o) => o.requires.any((q) => !q.met)), isTrue);
    expect(cat.ships.any((s) => s.requires.any((q) => !q.met)), isTrue);
    final res = r.initial.player.resources;
    expect(cat.ships.any((s) => s.unitCost.deuterium * 5 > res.deuterium), isTrue);
  });
}
