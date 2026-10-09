import 'dart:ui';

import 'package:flutter/services.dart';
import 'package:flutter/material.dart' show Tooltip;
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/services.dart';
import 'package:iac_client/hex/hex_math.dart' as hm;
import 'package:iac_client/models/fleet.dart' as ui;
import 'package:iac_client/models/game_state.dart';
import 'package:iac_client/models/homeworld.dart';
import 'package:iac_client/models/resources.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/scenes/map/map_painter.dart';
import 'package:iac_client/scenes/map/map_state.dart';
import 'package:iac_client/state/game_controller.dart';

import 'support/harness.dart';

const _home = proto.Hex(0, 0);

/// A controller whose state the test owns: a lattice of known sectors, one T6
/// cruiser at (3,0), a fleet at (1,0), and a log of the commands it sends.
class FakeGame extends GameController {
  late GameState fake;
  final List<proto.Command> sent = [];
  ui.FleetState? fleet;

  FakeGame() {
    fleet = _fleet(const proto.Hex(1, 0));
    fake = _state(1000, [fleet!]);
  }

  @override
  GameState get state => fake;
  @override
  bool get isDemo => true;
  @override
  bool get isLive => false;
  @override
  ui.FleetState get currentFleet => fake.fleets.isEmpty ? super.currentFleet : fake.fleets.first;
  @override
  void sendCommand(proto.Command c) => sent.add(c);
  @override
  void selectFleet(int i) {}

  void push(int tick, {proto.Hex? at}) {
    final f = at == null ? fake.fleets.first : _fleet(at);
    fake = _state(tick, [f]);
    notifyListeners();
  }
}

ui.FleetState _fleet(proto.Hex at, {int fuel = 100}) => ui.FleetState(
      id: 7,
      name: 'F7',
      sector: at,
      status: ui.FleetStatus.idle,
      shipCount: 3,
      ships: const [ui.ShipState(shipClass: 'Corvette', count: 3, hull: 150, hullMax: 150, ids: [1, 2, 3])],
      fuel: fuel,
      fuelMax: 120,
      jumpFuel: 5,
      homeFuel: 5,
    );

GameState _state(int tick, List<ui.FleetState> fleets) {
  final sectors = <proto.Hex, proto.SectorState>{};
  for (final h in spiral(6)) {
    final hostile = h == const proto.Hex(3, 0);
    final minor = h == const proto.Hex(0, 3);
    sectors[h] = proto.SectorState(
      location: h,
      terrain: h.q == 2 && h.r == -2 ? proto.TerrainType.asteroidField : proto.TerrainType.empty,
      resources: const proto.SectorResources(metal: proto.Density.rich, crystal: proto.Density.sparse, deuterium: proto.Density.none),
      connections: [for (final n in h.neighbors()) if (n.distFromOrigin <= 6) n],
      hostiles: hostile
          ? const [proto.NpcFleetInfo(id: 9, ships: [proto.NpcShipInfo(shipClass: proto.ShipClass.cruiser, count: 1)], behavior: proto.NpcBehavior.aggressive)]
          : minor
              ? const [proto.NpcFleetInfo(id: 10, ships: [proto.NpcShipInfo(shipClass: proto.ShipClass.scout, count: 1)], behavior: proto.NpcBehavior.patrol)]
              : null,
      threat: hostile
          ? const proto.ThreatInfo(rating: 6, estPower: 137.5, basis: proto.ThreatBasis.observed)
          : minor
              ? const proto.ThreatInfo(rating: 2, estPower: 9, basis: proto.ThreatBasis.observed)
              : const proto.ThreatInfo(rating: 1, estPower: 4, basis: proto.ThreatBasis.estimate),
      salvage: h == const proto.Hex(2, 1) ? const proto.Resources(metal: 40, crystal: 10) : null,
      site: h == const proto.Hex(-2, 2) ? const proto.SiteBrief(tier: 1, risk: proto.SiteRisk.uneasy) : null,
      lastSeen: h.distFromOrigin <= 2 ? tick : tick - (h.distFromOrigin * 400),
      live: h.distFromOrigin <= 2,
    );
  }
  return GameState(
    tick: tick,
    clockSec: tick,
    resources: const Resources(
        metal: ResourceStock(amount: 7340, rate: 2.4), crystal: ResourceStock(amount: 6900, rate: 1.1), deut: ResourceStock(amount: 880, rate: .4)),
    fleets: fleets,
    buildQueue: const [],
    shipyard: const [],
    docked: 'none',
    research: const ResearchState(name: 'Idle', time: '-', pct: 0, completed: []),
    events: const [],
    alerts: const [],
    sector: SectorInfo.unknown,
    homeworld: _home,
    waypoints: const [],
    sectors: sectors,
    signals: {const proto.Hex(7, 0): proto.SignalKind.hostileMass},
    stock: const proto.Resources(metal: 7340, crystal: 6900, deuterium: 880),
  );
}

Iterable<proto.Hex> spiral(int r) sync* {
  for (var q = -r; q <= r; q++) {
    for (var rr = -r; rr <= r; rr++) {
      final h = proto.Hex(q, rr);
      if (h.distFromOrigin <= r) yield h;
    }
  }
}

/// Screen position of [h] with the map centred on the fleet at (1,0), zoom 1.15.
Offset at(proto.Hex h, Size win, {proto.Hex centre = const proto.Hex(1, 0), double s = 1.15}) {
  final narrow = win.width < 820;
  final origin = Offset(narrow ? 0 : 78, narrow ? 46 : 56);
  final size = Size(win.width - origin.dx, win.height - origin.dy - (narrow ? 54 : 0));
  final p = hm.hexToPixel(h.q, h.r, mapU), c = hm.hexToPixel(centre.q, centre.r, mapU);
  return origin + Offset(size.width / 2, size.height / 2) + (p - c) * s;
}

MapPainter painterOf(WidgetTester t) => t.widget<CustomPaint>(find.byKey(const Key('map-canvas'))).painter! as MapPainter;

const _win = Size(1440, 900);

Future<FakeGame> openMap(WidgetTester tester, {Size size = _win}) async {
  final g = FakeGame();
  await pumpShell(tester, controller: g, screen: Screen.map, size: size);
  return g;
}

Future<void> clickHex(WidgetTester tester, proto.Hex h, {Size size = _win}) async {
  await tester.tapAt(at(h, size));
  await tester.pump(const Duration(milliseconds: 100));
}

String _text(WidgetTester t, String key) => (t.widget(find.byKey(Key(key))) as Text).textSpan?.toPlainText() ?? (t.widget(find.byKey(Key(key))) as Text).data ?? '';

void main() {
  testWidgets('clicking a known sector plans a route with hops and fuel', (tester) async {
    final g = await openMap(tester);
    await clickHex(tester, const proto.Hex(4, -1));
    expect(find.text('ROUTE PLAN'), findsOneWidget);
    expect(_text(tester, 'route-hops'), '3');
    expect(_text(tester, 'route-fuel'), '15 of 100');
    expect(_text(tester, 'route-return'), contains('OK, margin 65'));
    expect(find.byKey(const Key('route-preview-text')), findsOneWidget);
    expect(painterOf(tester).route!.path.length, 3);
    await endShell(tester, g);
  });

  testWidgets('shift+click appends a waypoint and re-plans through it', (tester) async {
    final g = await openMap(tester);
    await clickHex(tester, const proto.Hex(4, -1));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await clickHex(tester, const proto.Hex(4, -3));
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    expect(_text(tester, 'route-hops'), '5');
    expect(painterOf(tester).route!.waypoints.length, 2);
    await endShell(tester, g);
  });

  testWidgets('engage starts the runner and sends one move per idle tick', (tester) async {
    final g = await openMap(tester);
    await clickHex(tester, const proto.Hex(4, -1));
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    final con = Console.of(tester.element(find.byKey(const Key('map-canvas'))));
    expect(con.runner.active, isTrue);
    var moves = g.sent.whereType<proto.MoveCommand>().toList();
    expect(moves.length, 1);
    final first = moves.single.target;
    expect(proto.Hex.distance(first, const proto.Hex(1, 0)), 1);

    g.push(1000);
    await tester.pump();
    expect(g.sent.whereType<proto.MoveCommand>().length, 1, reason: 'same tick: nothing new');

    g.push(1001, at: first);
    await tester.pump();
    moves = g.sent.whereType<proto.MoveCommand>().toList();
    expect(moves.length, 2);
    expect(proto.Hex.distance(moves.last.target, first), 1);
    await tester.pump(const Duration(seconds: 5));
    await endShell(tester, g);
  });

  testWidgets('route holds before a T5+ sector; Continue goes on, Abort stops', (tester) async {
    for (final abort in [false, true]) {
      final g = await openMap(tester);
      await clickHex(tester, const proto.Hex(3, 0));
      expect(painterOf(tester).route!.holds, isNotEmpty);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      final first = g.sent.whereType<proto.MoveCommand>().single.target;
      g.push(1001, at: first);
      await tester.pump();
      expect(find.byKey(const Key('hold-panel')), findsOneWidget);
      expect(g.sent.whereType<proto.MoveCommand>().length, 1, reason: 'held: no move sent');
      if (abort) {
        await tester.tap(find.text('ABORT'));
        await tester.pump();
        expect(find.byKey(const Key('hold-panel')), findsNothing);
        expect(g.sent.last, isA<proto.StopCommand>());
      } else {
        await tester.tap(find.text('CONTINUE'));
        await tester.pump();
        final moves = g.sent.whereType<proto.MoveCommand>().toList();
        expect(moves.length, 2);
        expect(moves.last.target, const proto.Hex(3, 0));
      }
      await tester.pump(const Duration(seconds: 5));
      await endShell(tester, g);
    }
  });

  testWidgets('layer keys toggle, and zoom and pan work', (tester) async {
    final g = await openMap(tester);
    expect(painterOf(tester).layers.threat, isFalse);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyL);
    await tester.pump();
    expect(painterOf(tester).layers.threat, isTrue);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyZ);
    await tester.pump();
    expect(painterOf(tester).layers.zones, isFalse);
    await tester.tap(find.byKey(const Key('layer-ore')));
    await tester.pump();
    expect(painterOf(tester).layers.ore, isTrue);

    final vp = painterOf(tester).vp;
    final s0 = vp.s;
    await tester.tap(find.byKey(const Key('zoom-in')));
    await tester.pump();
    expect(vp.s, greaterThan(s0));
    final x0 = vp.x;
    await tester.dragFrom(const Offset(700, 500), const Offset(-120, 0));
    await tester.pump();
    expect(vp.x, greaterThan(x0));
    await endShell(tester, g);
  });

  testWidgets('an uncharted sector explains why it cannot be routed', (tester) async {
    final g = await openMap(tester);
    await clickHex(tester, const proto.Hex(7, 0));
    expect(find.byKey(const Key('route-why')), findsOneWidget);
    expect(_text(tester, 'route-why'), contains('No charted lane'));
    expect(find.text('ROUTE PLAN'), findsNothing);
    await endShell(tester, g);
  });

  testWidgets('refresh intel is disabled away from the sector and says why', (tester) async {
    final g = await openMap(tester);
    await clickHex(tester, const proto.Hex(0, 5));
    expect(find.text('REFRESH INTEL'), findsOneWidget);
    final btn = tester.widget<Tooltip>(find.ancestor(of: find.text('REFRESH INTEL'), matching: find.byType(Tooltip)).first);
    expect(btn.message, contains('Needs a fleet in this sector'));
    await endShell(tester, g);
  });

  testWidgets('static layer is cached across small pans', (tester) async {
    final g = await openMap(tester);
    final p = painterOf(tester);
    final rec0 = p.cache.records;
    p.vp.panBy(const Offset(10, 4));
    await tester.pump();
    p.vp.panBy(const Offset(10, 4));
    await tester.pump();
    expect(p.cache.records, rec0, reason: 'pan within the margin reuses the picture');
    expect(p.cache.reuses, greaterThan(0));

    final sw = Stopwatch()..start();
    const n = 30;
    for (var i = 0; i < n; i++) {
      p.paint(Canvas(PictureRecorder()), const Size(1362, 844));
    }
    // ignore: avoid_print
    print('map paint (cached static layer, debug VM): ${(sw.elapsedMicroseconds / n / 1000).toStringAsFixed(2)} ms/frame');
    await endShell(tester, g);
  });

  testWidgets('map shots', (tester) async {
    final g = await openMap(tester);
    await shot(tester, 'default-1440');
    await tester.sendKeyEvent(LogicalKeyboardKey.keyL);
    await tester.pump(const Duration(milliseconds: 100));
    await shot(tester, 'threat-1440');
    await clickHex(tester, const proto.Hex(4, -1));
    await shot(tester, 'route-1440');
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await clickHex(tester, const proto.Hex(3, 0));
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    g.push(1001, at: g.sent.whereType<proto.MoveCommand>().single.target);
    await tester.pump(const Duration(milliseconds: 200));
    await shot(tester, 'hold-1440');
    await tester.pump(const Duration(seconds: 5));
    await endShell(tester, g);
  });

  testWidgets('390 wide: no overflow, panel sheet shows the route', (tester) async {
    const win = Size(390, 844);
    final g = await openMap(tester, size: win);
    await shot(tester, 'default-390');
    await tester.sendKeyEvent(LogicalKeyboardKey.keyL);
    await tester.pump(const Duration(milliseconds: 100));
    await shot(tester, 'threat-390');
    await tester.tapAt(at(const proto.Hex(4, -1), win));
    await tester.pump(const Duration(milliseconds: 200));
    expect(find.text('ROUTE PLAN'), findsOneWidget);
    await shot(tester, 'route-390');
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    await tester.tapAt(at(const proto.Hex(3, 0), win));
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    g.push(1001, at: g.sent.whereType<proto.MoveCommand>().single.target);
    await tester.pump(const Duration(milliseconds: 200));
    expect(find.byKey(const Key('hold-panel')), findsOneWidget);
    await shot(tester, 'hold-390');
    expect(tester.takeException(), isNull);
    await tester.pump(const Duration(seconds: 5));
    await endShell(tester, g);
  });
}
