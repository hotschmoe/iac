import 'dart:async';
import 'dart:math' as math;

import 'dart:ui' show PointerDeviceKind;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/threat.dart';
import 'package:iac_client/console/services.dart';
import 'package:iac_client/models/fleet.dart' as ui;
import 'package:iac_client/models/game_state.dart';
import 'package:iac_client/models/homeworld.dart';
import 'package:iac_client/models/resources.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/state/game_controller.dart';

import 'support/harness.dart';

/// Controller whose state is set by the test and whose commands are recorded.
class FakeController extends GameController {
  FakeController(this.fake);
  GameState fake;
  final List<proto.ClientMessage> sent = [];
  final StreamController<proto.GameEvent> ev = StreamController.broadcast(sync: true);

  @override
  GameState get state => fake;
  @override
  ui.FleetState get currentFleet => fake.fleets[activeFleet];
  @override
  int? get activeFleetId => fake.fleets[activeFleet].id;
  @override
  List<ui.FleetState> get mergeCandidates => [for (final f in fake.fleets) if (f.id != currentFleet.id && f.sector == currentFleet.sector) f];
  @override
  void send(proto.ClientMessage msg) => sent.add(msg);
  @override
  Stream<proto.GameEvent> get gameEvents => ev.stream;
  @override
  bool get hasState => true;
  @override
  bool get isDemo => true;

  void push(GameState s) {
    fake = s;
    notifyListeners();
  }

  Iterable<proto.Command> get commands => sent.whereType<proto.CommandMessage>().map((m) => m.command);
}

const here = proto.Hex(2, 0);
const home = proto.Hex(0, 0);

proto.SectorState sector(proto.Hex h,
        {List<proto.Hex> lanes = const [],
        List<proto.NpcFleetInfo>? hostiles,
        proto.SectorResources res = const proto.SectorResources(metal: proto.Density.rich, crystal: proto.Density.sparse, deuterium: proto.Density.none),
        proto.Resources? salvage,
        bool live = true,
        proto.TerrainType terrain = proto.TerrainType.asteroidField}) =>
    proto.SectorState(
        location: h,
        terrain: terrain,
        resources: res,
        connections: lanes,
        hostiles: hostiles,
        salvage: salvage,
        threat: _threatOf(hostiles),
        lastSeen: 100,
        live: live);

/// What the server would rate these groups: their ships' power, Aggressive and Swarm +25 percent.
proto.ThreatInfo _threatOf(List<proto.NpcFleetInfo>? groups) {
  if (groups == null || groups.isEmpty) return const proto.ThreatInfo(rating: 1, estPower: 4, basis: proto.ThreatBasis.estimate);
  var power = 0.0;
  for (final g in groups) {
    final ships = g.ships.fold(0.0, (n, s) => n + shipStats[s.shipClass]!.power * s.count);
    final pushy = g.behavior == proto.NpcBehavior.aggressive || g.behavior == proto.NpcBehavior.swarm;
    power += ships * (pushy ? 1.25 : 1);
  }
  final rating = power <= 4 ? 1 : (1 + (math.log(power / 4) / math.ln2).floor()).clamp(1, 9);
  return proto.ThreatInfo(rating: rating, estPower: power, basis: proto.ThreatBasis.observed);
}

ui.FleetState fleet(int id, List<(String, int)> groups, {proto.Hex at = here, ui.FleetStatus status = ui.FleetStatus.idle, int fuel = 80}) => ui.FleetState(
      id: id,
      name: 'F$id',
      sector: at,
      status: status,
      shipCount: groups.fold(0, (a, g) => a + g.$2),
      ships: [for (var i = 0; i < groups.length; i++) ui.ShipState(shipClass: groups[i].$1, count: groups[i].$2, hull: 50.0 * groups[i].$2 ~/ 1, hullMax: 50 * groups[i].$2, ids: [for (var k = 0; k < groups[i].$2; k++) id * 100 + i * 10 + k])],
      cargo: const ui.FleetCargo(capacity: 100),
      fuel: fuel,
      fuelMax: 120,
      jumpFuel: 5,
      homeFuel: 10,
      power: groups.fold(0.0, (a, g) => a + shipStats[proto.ShipClass.values.firstWhere((c) => c.label == g.$1)]!.power * g.$2),
    );

GameState world(List<ui.FleetState> fleets, Map<proto.Hex, proto.SectorState> sectors, {int tick = 500}) => GameState(
      tick: tick,
      clockSec: 0,
      resources: const Resources(metal: ResourceStock(amount: 0, rate: 0), crystal: ResourceStock(amount: 0, rate: 0), deut: ResourceStock(amount: 0, rate: 0)),
      fleets: fleets,
      buildQueue: const [],
      shipyard: const [],
      docked: 'none',
      research: const ResearchState(name: 'Idle', time: '-', pct: 0, completed: []),
      events: const [],
      alerts: const [],
      sector: SectorInfo.unknown,
      homeworld: home,
      waypoints: const [],
      sectors: sectors,
    );

GameState calm() => world([
      fleet(1, [('Scout', 1), ('Corvette', 2)]),
      fleet(2, [('Hauler', 1)], at: home),
    ], {
      here: sector(here, lanes: [const proto.Hex(2, -1), const proto.Hex(3, -1), const proto.Hex(3, 0)], salvage: const proto.Resources(metal: 40, crystal: 10, deuterium: 3)),
      const proto.Hex(2, -1): sector(const proto.Hex(2, -1), lanes: [here]),
      const proto.Hex(3, -1): sector(const proto.Hex(3, -1), lanes: [here], hostiles: const [
        proto.NpcFleetInfo(id: 77, ships: [proto.NpcShipInfo(shipClass: proto.ShipClass.corvette, count: 2)], behavior: proto.NpcBehavior.patrol)
      ]),
      home: sector(home, lanes: [const proto.Hex(1, 0)]),
    });

GameState hostileHere({ui.FleetStatus status = ui.FleetStatus.idle, int scouts = 1}) => world([
      fleet(1, [('Scout', scouts)], status: status),
    ], {
      here: sector(here, lanes: [const proto.Hex(2, -1)], hostiles: const [
        proto.NpcFleetInfo(id: 77, ships: [proto.NpcShipInfo(shipClass: proto.ShipClass.frigate, count: 1), proto.NpcShipInfo(shipClass: proto.ShipClass.corvette, count: 2)], behavior: proto.NpcBehavior.aggressive)
      ]),
      const proto.Hex(2, -1): sector(const proto.Hex(2, -1), lanes: [here]),
    });

Future<FakeController> pumpFake(WidgetTester tester, GameState s, {Size size = const Size(1440, 900)}) async {
  final c = FakeController(s);
  await pumpShell(tester, screen: Screen.windshield, size: size, controller: c);
  return c;
}

Future<void> key(WidgetTester tester, LogicalKeyboardKey k, String ch) async {
  await tester.sendKeyEvent(k, character: ch);
  await tester.pump(const Duration(milliseconds: 50));
}

Future<void> finish(WidgetTester tester, FakeController c) async {
  await endShell(tester, c);
  await c.ev.close();
}

proto.MovePreview preview(int fleetId, proto.Hex target, {double ratio = 1.3, proto.RatioLabel label = proto.RatioLabel.even, int hops = 2, int? charted = 17}) => proto.MovePreview(
      fleetId: fleetId,
      target: target,
      fuelCost: 5,
      fuelAfter: 75,
      canJump: true,
      hopsHome: hops,
      chartedHopsHome: charted,
      routeUnexplored: true,
      fuelToReturn: hops * 5.0,
      canReturn: true,
      threat: const proto.ThreatInfo(rating: 3, estPower: 20, basis: proto.ThreatBasis.estimate),
      fleetPower: 26,
      ratio: ratio,
      label: label,
    );

Future<TestGesture> mouseOver(WidgetTester tester, Finder f) async {
  final g = await tester.createGesture(kind: PointerDeviceKind.mouse);
  await g.addPointer(location: Offset.zero);
  await g.moveTo(tester.getCenter(f));
  await tester.pump(const Duration(milliseconds: 20));
  return g;
}

void main() {
  testWidgets('hovering a gate asks the server for that fleet\'s preview once it rests, and shows the verdict', (tester) async {
    final c = await pumpFake(tester, calm());
    final e = find.byKey(const ValueKey('gate-e'));
    final g = await mouseOver(tester, e);
    expect(c.commands.whereType<proto.PreviewMoveCommand>(), isEmpty, reason: 'debounced');
    await tester.pump(const Duration(milliseconds: 250));
    final asked = c.commands.whereType<proto.PreviewMoveCommand>().toList();
    expect(asked, hasLength(1));
    expect(asked.single.fleetId, 1, reason: 'the selected fleet, not the strongest');
    expect(asked.single.target, const proto.Hex(3, -1));

    final s = c.fake;
    c.push(s.copyWith(previews: {(1, const proto.Hex(3, -1)): PreviewEntry(preview(1, const proto.Hex(3, -1)), s.tick)}));
    await tester.pump(const Duration(milliseconds: 50));
    final odds = tester.widget<Text>(find.descendant(of: find.byKey(const ValueKey('gate-odds-e')), matching: find.byType(Text)).first);
    expect(odds.textSpan!.toPlainText(), contains('1.3X EVEN'));
    expect(odds.textSpan!.toPlainText(), contains('HOME 2H'));
    await g.removePointer();
    await tester.pump(const Duration(seconds: 1));
    await finish(tester, c);
  });

  testWidgets('a hover that moves on before the debounce asks nothing', (tester) async {
    final c = await pumpFake(tester, calm());
    final g = await mouseOver(tester, find.byKey(const ValueKey('gate-e')));
    await tester.pump(const Duration(milliseconds: 80));
    await g.moveTo(const Offset(5, 5));
    await tester.pump(const Duration(milliseconds: 400));
    expect(c.commands.whereType<proto.PreviewMoveCommand>(), isEmpty);
    await g.removePointer();
    await finish(tester, c);
  });

  testWidgets('a faint contact shows its danger band on the uncharted gate', (tester) async {
    final s = calm();
    final c = await pumpFake(tester, s.copyWith(
      sectors: {...s.sectors}..remove(const proto.Hex(3, 0)),
      signals: {const proto.Hex(3, 0): proto.SignalKind.hostileMass},
      signalBands: {const proto.Hex(3, 0): 3},
    ));
    expect(find.textContaining('FAINT MASS READING / BAND 3'), findsOneWidget);
    await finish(tester, c);
  });

  testWidgets('ore tiles show what is left and when they refill', (tester) async {
    final s = calm();
    final here2 = proto.SectorState(
      location: here,
      terrain: proto.TerrainType.asteroidField,
      resources: const proto.SectorResources(metal: proto.Density.rich, crystal: proto.Density.sparse, deuterium: proto.Density.none),
      connections: const [proto.Hex(2, -1), proto.Hex(3, -1), proto.Hex(3, 0)],
      oreReserve: const proto.OreReserve(
        metal: proto.TileReserve(units: 12, maxUnits: 22, refillsInS: 105),
        crystal: proto.TileReserve(units: 3, maxUnits: 3),
        deuterium: proto.TileReserve(units: 0, maxUnits: 0),
      ),
      threat: const proto.ThreatInfo(rating: 1, estPower: 4, basis: proto.ThreatBasis.estimate),
      lastSeen: 100,
      live: true,
    );
    final c = await pumpFake(tester, s.copyWith(sectors: {...s.sectors, here: here2}));
    await tester.pump(const Duration(milliseconds: 500));
    expect(tester.widget<Text>(find.byKey(const Key('contact-reserve-metal'))).data, 'FE LEFT 12/22 REFILL 1:45');
    expect(tester.widget<Text>(find.byKey(const Key('contact-reserve-crystal'))).data, 'CR LEFT 3/3');
    expect(find.byKey(const Key('contact-reserve-deuterium')), findsNothing);
    await finish(tester, c);
  });

  testWidgets('idle shots at 1440 and 390', (tester) async {
    final c = await pumpFake(tester, calm());
    await tester.pump(const Duration(milliseconds: 500));
    await shot(tester, 'ws-idle-1440');
    expect(tester.takeException(), isNull);
    await finish(tester, c);
  });

  testWidgets('narrow layout has no overflow', (tester) async {
    final c = await pumpFake(tester, hostileHere(status: ui.FleetStatus.combat), size: const Size(390, 844));
    await tester.pump(const Duration(milliseconds: 500));
    await shot(tester, 'ws-combat-390');
    await tester.tap(find.byKey(const ValueKey('status-sheet')));
    await tester.pump(const Duration(milliseconds: 300));
    await shot(tester, 'ws-sheet-390');
    expect(tester.takeException(), isNull);
    await finish(tester, c);
  });

  testWidgets('E jumps north-east: sends the MoveCommand for the screen direction', (tester) async {
    final c = await pumpFake(tester, calm());
    await key(tester, LogicalKeyboardKey.keyE, 'e');
    final moves = c.commands.whereType<proto.MoveCommand>().toList();
    expect(moves, hasLength(1));
    expect(moves.single.fleetId, 1);
    expect(moves.single.target, const proto.Hex(3, -1));
    await tester.pump(const Duration(milliseconds: 300));
    await shot(tester, 'ws-warp-1440');
    await tester.pump(const Duration(seconds: 3));
    await finish(tester, c);
  });

  testWidgets('a key with no lane toasts and sends nothing', (tester) async {
    final c = await pumpFake(tester, calm());
    await key(tester, LogicalKeyboardKey.keyS, 's');
    expect(c.commands.whereType<proto.MoveCommand>(), isEmpty);
    expect(find.text('NO LANE'), findsOneWidget);
    await tester.pump(const Duration(seconds: 5));
    await finish(tester, c);
  });

  testWidgets('firing on deadly odds needs a second F', (tester) async {
    final c = await pumpFake(tester, hostileHere());
    await key(tester, LogicalKeyboardKey.keyF, 'f');
    expect(c.commands.whereType<proto.AttackCommand>(), isEmpty);
    expect(find.textContaining('POOR ODDS'), findsOneWidget);
    await key(tester, LogicalKeyboardKey.keyF, 'f');
    final atk = c.commands.whereType<proto.AttackCommand>().toList();
    expect(atk, hasLength(1));
    expect((atk.single.fleetId, atk.single.targetFleetId), (1, 77));
    await tester.pump(const Duration(seconds: 5));
    await finish(tester, c);
  });

  testWidgets('favourable odds fire at once', (tester) async {
    final s = world([fleet(1, [('Cruiser', 3)])], {
      here: sector(here, lanes: [const proto.Hex(2, -1)], hostiles: const [
        proto.NpcFleetInfo(id: 5, ships: [proto.NpcShipInfo(shipClass: proto.ShipClass.scout, count: 1)], behavior: proto.NpcBehavior.patrol)
      ]),
    });
    final c = await pumpFake(tester, s);
    await key(tester, LogicalKeyboardKey.keyF, 'f');
    expect(c.commands.whereType<proto.AttackCommand>(), hasLength(1));
    await finish(tester, c);
  });

  testWidgets('harvest opens the resource bar and sends the chosen resource', (tester) async {
    final c = await pumpFake(tester, calm());
    await key(tester, LogicalKeyboardKey.keyH, 'h');
    expect(find.byKey(const ValueKey('res-c')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('res-c')));
    await tester.pump(const Duration(milliseconds: 100));
    final h = c.commands.whereType<proto.HarvestCommand>().single;
    expect((h.fleetId, h.resource), (1, proto.HarvestResource.crystal));
    await finish(tester, c);
  });

  testWidgets('a disabled dock action says why', (tester) async {
    final c = await pumpFake(tester, calm());
    await tester.tap(find.byKey(const ValueKey('dock-fire')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('No hostile contact here.'), findsWidgets);
    expect(c.commands.whereType<proto.AttackCommand>(), isEmpty);
    await tester.pump(const Duration(seconds: 5));
    await finish(tester, c);
  });

  testWidgets('combat events draw and the scene survives a kill', (tester) async {
    final c = await pumpFake(tester, hostileHere(scouts: 3));
    c.push(hostileHere(status: ui.FleetStatus.combat, scouts: 3));
    c.ev.add(const proto.GameEvent(tick: 501, kind: proto.CombatStartedEvent(playerFleetId: 1, owner: 'Pilot', enemyFleetId: 77, sector: here, mine: true)));
    for (var i = 0; i < 6; i++) {
      c.ev.add(proto.GameEvent(
          tick: 501,
          kind: proto.CombatRoundEvent(
              sector: here, attackerFleetId: i.isEven ? 1 : 77, attackerOwner: null, attackerShipId: 100 + i, targetFleetId: i.isEven ? 77 : 1, targetOwner: null, targetShipId: 100 + i, damage: 18, shieldAbsorbed: 8, hullDamage: 10, rapidFire: false, mine: true)));
    }
    await tester.pump(const Duration(milliseconds: 150));
    await shot(tester, 'ws-combat-1440');
    await tester.pump(const Duration(milliseconds: 250));
    c.ev.add(const proto.GameEvent(tick: 502, kind: proto.ShipDestroyedEvent(shipId: 0, shipClass: proto.ShipClass.corvette, ownerFleetId: 77, isNpc: true, sector: here, owner: null, mine: false)));
    await tester.pump(const Duration(milliseconds: 100));
    await shot(tester, 'ws-kill-1440');
    expect(tester.takeException(), isNull);
    await tester.pump(const Duration(seconds: 3));
    await finish(tester, c);
  });

  testWidgets('scan sweep and cooldown', (tester) async {
    final c = await pumpFake(tester, calm());
    await key(tester, LogicalKeyboardKey.keyV, 'v');
    expect(c.commands.whereType<proto.ScanCommand>(), hasLength(1));
    await tester.pump(const Duration(milliseconds: 500));
    await shot(tester, 'ws-scan-1440');
    await key(tester, LogicalKeyboardKey.keyV, 'v');
    expect(c.commands.whereType<proto.ScanCommand>(), hasLength(1), reason: 'cooldown blocks a second scan');
    await tester.pump(const Duration(seconds: 6));
    await finish(tester, c);
  });

  testWidgets('number keys select fleets', (tester) async {
    final c = GameController()..startDemo();
    await pumpShell(tester, screen: Screen.windshield, controller: c);
    await key(tester, LogicalKeyboardKey.digit2, '2');
    expect(c.activeFleet, 1);
    await endShell(tester, c);
  });

  testWidgets('split a ship off and merge it back through the panel (demo)', (tester) async {
    final c = GameController()..startDemo();
    await pumpShell(tester, screen: Screen.windshield, controller: c);
    await tester.tap(find.byKey(const ValueKey('manage-toggle')));
    await tester.pump(const Duration(milliseconds: 100));
    final before = c.state.fleets.length;
    final active = c.activeFleetId!;
    final cls = c.currentFleet.ships.last.shipClass;
    await tester.tap(find.byKey(ValueKey('fm-plus-$cls')));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('fm-split')));
    await tester.pump(const Duration(milliseconds: 200));
    expect(c.state.fleets.length, before + 1);
    expect(c.currentFleet.id, active);
    final newId = c.mergeCandidates.single.id;
    await tester.tap(find.byKey(ValueKey('fm-merge-$newId')));
    await tester.pump(const Duration(milliseconds: 200));
    expect(c.state.fleets.length, before);
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  testWidgets('frame cost of the windshield scene', (tester) async {
    final c = await pumpFake(tester, hostileHere(status: ui.FleetStatus.combat, scouts: 3));
    final sw = Stopwatch()..start();
    const n = 120;
    for (var i = 0; i < n; i++) {
      await tester.pump(const Duration(milliseconds: 16));
    }
    sw.stop();
    // ignore: avoid_print
    print('WINDSHIELD build+layout+paint-record: ${(sw.elapsedMicroseconds / n / 1000).toStringAsFixed(2)} ms/frame over $n frames');
    await finish(tester, c);
  });
}
