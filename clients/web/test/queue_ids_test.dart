import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/intel.dart';
import 'package:iac_client/protocol/protocol.dart' as p;
import 'package:iac_client/state/command_parser.dart';
import 'package:iac_client/state/state_mapper.dart';

import 'demo_world_test.dart' show Rig;

p.SectorState sector({p.Resources? salvage, int? despawn, bool live = true, bool stale = false}) => p.SectorState(
      location: const p.Hex(1, 1),
      terrain: p.TerrainType.empty,
      resources: const p.SectorResources(metal: p.Density.none, crystal: p.Density.none, deuterium: p.Density.none),
      connections: const [],
      salvage: salvage,
      salvageDespawnTick: despawn,
      threat: const p.ThreatInfo(rating: 0, estPower: 0, basis: p.ThreatBasis.estimate),
      lastSeen: 10,
      live: live,
      pinsStale: stale,
    );

void main() {
  test('cancel commands carry the stable id and build commands carry reserve', () {
    expect(const p.CancelBuildCommand(queueType: p.QueueType.ship, id: 322).toJson(),
        {'action': 'cancel_build', 'queue_type': 'Ship', 'id': 322});
    expect(const p.CancelQueuedCommand(queueType: p.QueueType.building, id: 331).toJson(),
        {'action': 'cancel_queued', 'queue_type': 'Building', 'id': 331});
    expect(const p.BuildCommand(buildingType: p.BuildingType.metalMine, reserve: true).toJson()['reserve'], true);
    final back = p.Command.fromJson({'action': 'build_ship', 'ship_class': 'Scout', 'count': 2});
    expect((back as p.BuildShipCommand).reserve, isFalse);
  });

  test('the command bar cancels by id, defaulting to the first item of the queue', () {
    const ctx = CommandContext(
      fleetId: null,
      fleetSector: null,
      sectors: {},
      fleets: [],
      cursor: p.Hex(0, 0),
      tick: 0,
      queues: {
        p.QueueType.ship: QueueIds(running: [7], waiting: [9, 10]),
      },
    );
    final run = parseCommand('cancel ship', ctx) as ParsedSend;
    expect((run.messages.single as p.CommandMessage).command.toJson()['id'], 7);
    final wait = parseCommand('cancel ship waiting', ctx) as ParsedSend;
    expect((wait.messages.single as p.CommandMessage).command, isA<p.CancelQueuedCommand>());
    final named = parseCommand('cancel ship 10 waiting', ctx) as ParsedSend;
    expect((named.messages.single as p.CommandMessage).command.toJson()['id'], 10);
    expect(parseCommand('cancel building', ctx), isA<ParsedError>());
  });

  test('a partly built batch shows built units and the next unit shortfall', () {
    final r = Rig();
    r.send(const p.BuildShipCommand(shipClass: p.ShipClass.scout, count: 200));
    final q = r.ticks.last.homeworldUpdate!.shipyardPending.last;
    expect((q.count, q.built), (200, 0));
    expect(q.unitCost.metal, greaterThan(0));
    expect(q.waitingFor, isNull, reason: 'one unit is affordable even though the batch is not');
    expect(StateMapper.shipOrderLabel(q), 'Scout x200');

    final mid = p.QueuedShip(
        id: q.id, item: q.item, count: 5, built: 2, cost: q.cost, unitCost: q.unitCost, ticks: q.ticks, waitingFor: q.unitCost);
    expect(StateMapper.shipOrderLabel(mid), 'Scout x5 (2 built)');
    expect(waitSummary(p.WaitReason.resources, mid.waitingFor, null), startsWith('short '));
  });

  test('a batch returns to the line between units with its progress and keeps its id', () {
    final r = Rig();
    final yard = r.initial.homeworld.shipyardQueue!;
    r.step(yard.endTick - r.initial.tick + 2);
    final hw = r.ticks.last.homeworldUpdate!;
    final running = hw.shipyardQueue;
    final pending = hw.shipyardPending.where((q) => q.id == yard.id);
    expect(running?.id == yard.id || pending.isNotEmpty, isTrue);
    expect(running?.built ?? pending.single.built, 2);
  });

  test('the first affordable order starts past one that cannot pay, and reserve holds the line', () {
    final r = Rig();
    r.send(const p.BuildShipCommand(shipClass: p.ShipClass.scout, count: 400, reserve: true));
    r.send(const p.BuildShipCommand(shipClass: p.ShipClass.scout, count: 1));
    final pending = r.ticks.last.homeworldUpdate!.shipyardPending;
    expect(pending.first.reserve, isTrue);
    expect(pending.last.reserve, isFalse);
  });

  test('salvage countdown and stale pins', () {
    final live = sector(salvage: const p.Resources(metal: 50), despawn: 130);
    expect(salvageSecondsLeft(live, 100), 30);
    expect(despawnLabel(live, 100), 'drifts away in 0:30');
    expect(salvagePinVisible(live, 129), isTrue);
    expect(salvagePinVisible(live, 130), isFalse, reason: 'a pile past its tick is gone');
    expect(despawnLabel(live, 130), isNull);
    expect(despawnLabel(sector(), 100), isNull);
    final stale = sector(salvage: const p.Resources(metal: 50), live: false, stale: true);
    expect(salvagePinVisible(stale, 100), isTrue, reason: 'no countdown, so kept and marked');
    expect(stale.pinsStale, isTrue);
    expect(p.SectorState.fromJson(stale.toJson()).pinsStale, isTrue);
  });

  test('the same hold reason is logged once, even as its figures drift', () {
    final m = StateMapper();
    p.GameEvent hold(int tick, String reason) => p.GameEvent(
        tick: tick,
        kind: p.PolicyActionEvent(fleetId: 3, preset: p.PolicyPreset.prospect, action: 'hold', reason: reason));
    m.apply(hold(1, 'no safe way to [7,-3] (T2 power 13 against your 18: 1.36x EVEN, needs 1.5x)'));
    m.apply(hold(61, 'no safe way to [7,-3] (T2 power 13 against your 19: 1.46x EVEN, needs 1.5x)'));
    m.apply(hold(121, 'no safe way to [7,-3] (T2 power 13 against your 19: 1.46x EVEN, needs 1.5x)'));
    expect(m.log.length, 1);
    m.apply(hold(181, 'range is outside 4 hexes'));
    expect(m.log.length, 2);
    m.apply(hold(241, 'no safe way to [7,-3] (T2 power 13 against your 19: 1.46x EVEN, needs 1.5x)'));
    expect(m.log.length, 3, reason: 'a changed cause is reported again');
  });
}
