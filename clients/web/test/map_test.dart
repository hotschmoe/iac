import 'dart:ui';

import 'package:flutter/services.dart';
import 'package:flutter/material.dart' show Tooltip;
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/services.dart';
import 'package:iac_client/design/panel.dart' show KeyCap, Kv;
import 'package:iac_client/hex/hex_math.dart' as hm;
import 'package:iac_client/models/fleet.dart' as ui;
import 'package:iac_client/models/game_state.dart';
import 'package:iac_client/models/homeworld.dart';
import 'package:iac_client/models/resources.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/scenes/map/map_painter.dart';
import 'package:iac_client/scenes/map/map_state.dart';
import 'package:iac_client/state/game_controller.dart';
import 'package:iac_client/views/map/map_panels.dart' show LayerBar;

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
  bool get isLive => true;
  @override
  bool get hasState => true;
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

ui.FleetState _fleet(proto.Hex at, {int fuel = 100, double? jumpFuelExact, proto.Pursuit? pursuedBy}) => ui.FleetState(
      id: 7,
      name: 'F7',
      sector: at,
      pursuedBy: pursuedBy,
      status: ui.FleetStatus.idle,
      shipCount: 3,
      ships: [shipRow(proto.ShipClass.lightFighter, 3)],
      hull: 1200,
      hullMax: 1200,
      fuel: fuel,
      fuelMax: 120,
      jumpFuel: 5,
      jumpFuelExact: jumpFuelExact,
      homeFuel: 5,
      jumpTicks: 3,
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
          ? [hostileGroup(9, [shipRow(proto.ShipClass.cruiser, 1)], proto.NpcBehavior.aggressive)]
          : minor
              ? [hostileGroup(10, [shipRow(proto.ShipClass.lightFighter, 1)], proto.NpcBehavior.patrol)]
              : null,
      threat: hostile
          ? const proto.ThreatInfo(rating: 6, estPower: 137.5, basis: proto.ThreatBasis.observed)
          : minor
              ? const proto.ThreatInfo(rating: 2, estPower: 9, basis: proto.ThreatBasis.observed)
              : const proto.ThreatInfo(rating: 1, estPower: 4, basis: proto.ThreatBasis.estimate),
      oreReserve: h == const proto.Hex(2, -2)
          ? const proto.OreReserve(
              metal: proto.TileReserve(units: 12, maxUnits: 22, refillsInS: 105),
              crystal: proto.TileReserve(units: 3, maxUnits: 3),
              deuterium: proto.TileReserve(units: 0, maxUnits: 0),
            )
          : null,
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
    research: const ResearchState(name: 'Idle', time: '-', pct: 0, completed: []),
    events: const [],
    alerts: const [],
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

proto.MovePreview _preview(proto.Hex target,
        {double ratio = 1.3, proto.RatioLabel label = proto.RatioLabel.even, int hops = 1, int? charted = 9, proto.FlightQuote? flight}) =>
    proto.MovePreview(
      flight: flight,
      fleetId: 7,
      target: target,
      fuelCost: 5,
      fuelAfter: 95,
      canJump: true,
      hopsHome: hops,
      chartedHopsHome: charted,
      routeUnexplored: true,
      fuelToReturn: hops * 5.0,
      canReturn: true,
      threat: const proto.ThreatInfo(rating: 3, estPower: 20, basis: proto.ThreatBasis.template),
      fleetPower: 26,
      ratio: ratio,
      label: label,
    );

void _answer(FakeGame g, proto.MovePreview p) {
  g.fake = g.fake.copyWith(previews: {...g.fake.previews, (p.fleetId, p.target): PreviewEntry(p, g.fake.tick)});
  g.notifyListeners();
}

Iterable<proto.PreviewMoveCommand> _asked(FakeGame g) => g.sent.whereType<proto.PreviewMoveCommand>();

void main() {
  testWidgets('a route asks the server about its first hop and shows the verdict, both ways home included', (tester) async {
    final g = await openMap(tester);
    await clickHex(tester, const proto.Hex(4, -1));
    await tester.pump(const Duration(milliseconds: 300));
    final first = painterOf(tester).route!.path.first;
    expect(_asked(g), hasLength(1));
    expect(_asked(g).single.fleetId, 7, reason: 'the selected fleet');
    expect(_asked(g).single.target, first);

    // More ticks while the answer is pending or fresh do not ask again.
    g.push(1001);
    g.push(1002);
    await tester.pump(const Duration(milliseconds: 300));
    expect(_asked(g), hasLength(1));

    _answer(g, _preview(first, hops: 2, charted: 17));
    await tester.pump(const Duration(milliseconds: 50));
    expect(_text(tester, 'route-home'), '2 hops direct (17 charted), through unexplored space, 10 fuel');
    expect(_text(tester, 'route-preview-text'), contains('NEXT HOP'));
    expect(_text(tester, 'route-preview-text'), contains('1.3 EVEN'));
    await endShell(tester, g);
  });

  testWidgets('a hover sweep asks once, for the first hop of the route it ends on', (tester) async {
    final g = await openMap(tester);
    final m = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await m.addPointer(location: Offset.zero);
    for (final h in const [proto.Hex(3, 0), proto.Hex(2, 0), proto.Hex(2, 1)]) {
      await m.moveTo(at(h, _win));
      await tester.pump(const Duration(milliseconds: 40));
    }
    await tester.pump(const Duration(milliseconds: 400));
    expect(_asked(g).map((c) => c.target), [const proto.Hex(2, 0)], reason: 'the first hop of the route on show, asked once');

    await m.moveTo(at(const proto.Hex(5, 0), _win));
    await tester.pump(const Duration(milliseconds: 400));
    expect(_asked(g), hasLength(1), reason: 'a far sector is not a jump the server can preview; its first hop is already asked');
    await m.removePointer();
    await endShell(tester, g);
  });

  testWidgets('the inspector rates an adjacent sector with the server verdict for the selected fleet and lists ore left', (tester) async {
    final g = await openMap(tester);
    final tile = const proto.Hex(2, -2);
    final m = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await m.addPointer(location: Offset.zero);
    await m.moveTo(at(const proto.Hex(2, -1), _win));
    await tester.pump(const Duration(milliseconds: 300));
    _answer(g, _preview(const proto.Hex(2, -1), ratio: 0.95, label: proto.RatioLabel.risky));
    await tester.pump(const Duration(milliseconds: 50));
    expect(_text(tester, 'inspector-ratio'), contains('0.95 RISKY'));
    expect(_text(tester, 'inspector-ratio'), contains('F7 v power 20'));
    expect(_text(tester, 'inspector-basis'), 'ESTIMATE');

    await m.moveTo(at(tile, _win));
    await tester.pump(const Duration(milliseconds: 50));
    expect(_text(tester, 'ore-reserve-Metal'), '12/22 REFILL 1:45');
    expect(_text(tester, 'ore-reserve-Crystal'), '3/3');
    expect(find.byKey(const Key('ore-reserve-Deut')), findsNothing);
    expect(find.textContaining('FOR THIS DISTANCE'), findsNothing);

    await m.moveTo(at(const proto.Hex(3, 0), _win));
    await tester.pump(const Duration(milliseconds: 50));
    expect(tester.widget<Kv>(find.byKey(const Key('inspector-hostile-9'))).v, 'AGGRESSIVE - 1 SHIP');
    expect(textUnder(tester, find.byKey(const Key('inspector-hostile-9-Cruiser'))), 'CRUISER\n1\n2,700 / 2,700',
        reason: 'a group in sight is listed by class: its count and its hull against full hull');
    await shot(tester, 'map-hostile-group-1440');
    await m.moveTo(at(const proto.Hex(1, 0), _win));
    await tester.pump(const Duration(milliseconds: 50));
    expect(tester.widget<Kv>(find.byKey(const Key('inspector-fleet-7'))).v, '3 SHIPS - HULL 1,200 / 1,200');
    await m.removePointer();
    await endShell(tester, g);
  });

  testWidgets('a pursuit is drawn on the map and told in the sector panel: the lane the group jumps down, the fleet it is after, what it musters and when it is whole',
      (tester) async {
    const lair = proto.Hex(3, 0), landed = proto.Hex(2, 0), old = proto.Hex(0, 3);
    final g = await openMap(tester);
    final was = _state(1000, [
      _fleet(landed, pursuedBy: const proto.Pursuit(groupId: 9, label: 'cruiser pack', from: lair, arrivesIn: 2, catches: false, hexesLeft: 1)),
    ]);
    proto.SectorState holding(proto.Hex h, proto.NpcFleetInfo group, {required bool live}) => proto.SectorState(
        location: h,
        terrain: proto.TerrainType.empty,
        resources: was.sectors[h]!.resources,
        connections: was.sectors[h]!.connections,
        hostiles: [group],
        threat: was.sectors[h]!.threat,
        lastSeen: live ? 1000 : 400,
        live: live);
    g.fake = was.copyWith(sectors: {
      ...was.sectors,
      lair: holding(
          lair,
          hostileGroup(9, [shipRow(proto.ShipClass.cruiser, 1, damaged: 1, hullLost: 900)], proto.NpcBehavior.aggressive,
              muster: 2,
              wholeAt: 1300,
              chaseHexes: 2,
              jumpTicks: 2,
              errand: const proto.GroupErrand(home: lair, quarry: 7, quarryOwner: 'Pilot', to: landed, arrivesAt: 1002, hexesLeft: 1)),
          live: true),
      old: holding(old, hostileGroup(10, [shipRow(proto.ShipClass.lightFighter, 1)], proto.NpcBehavior.patrol, muster: 3, wholeAt: 900, chaseHexes: 1), live: false),
    });
    g.notifyListeners();
    await tester.pump(const Duration(milliseconds: 100));

    final m = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await m.addPointer(location: Offset.zero);
    await m.moveTo(at(lair, _win));
    await tester.pump(const Duration(milliseconds: 50));
    expect(tester.widget<Kv>(find.byKey(const Key('inspector-hostile-9'))).v, 'AGGRESSIVE - 1 OF 2 SHIPS');
    expect(textUnder(tester, find.byKey(const Key('inspector-hostile-9-whole'))), startsWith('WHOLE AGAIN IN 5:00  '));
    expect(_text(tester, 'inspector-hostile-9-chase'), 'FOLLOWS 2 HEXES, A JUMP TAKES IT 2 TICKS, F7 3 TICKS.');
    expect(_text(tester, 'inspector-hostile-9-errand'), 'AFTER YOUR F7, JUMPING TO 2, 0, LANDS IN 2 TICKS, AND 1 HEX MORE.');
    await shot(tester, 'map-pursuit-1440');

    await m.moveTo(at(landed, _win));
    await tester.pump(const Duration(milliseconds: 50));
    expect(_text(tester, 'inspector-pursued-7'),
        'PURSUED: THE CRUISER PACK LANDS IN 2 TICKS, FROM 3, 0. A JUMP ORDERED NOW IS GONE BEFORE IT LANDS; A FLEET THAT STAYS IS CAUGHT. IT FOLLOWS 1 HEX MORE AFTER THIS ONE.');
    expect(_text(tester, 'fleet-chip-status-7'), 'PURSUED - IDLE');

    await m.moveTo(at(old, _win));
    await tester.pump(const Duration(milliseconds: 50));
    expect(tester.widget<Kv>(find.byKey(const Key('inspector-hostile-10'))).v, 'PATROL - 1 OF 3 SHIPS');
    expect(textUnder(tester, find.byKey(const Key('inspector-hostile-10-whole'))), startsWith('WHOLE AGAIN BY NOW, IF NOTHING DISTURBED IT  '),
        reason: 'a group last seen wounded, whose time to mend has passed since');
    expect(tester.takeException(), isNull, reason: 'the lanes of the pursuit are drawn');
    await m.removePointer();
    await endShell(tester, g);
  });

  testWidgets('a jump the server previews for a fleet in a fight is shown as flight, in the sector panel and in the route, with the quote for that lane', (tester) async {
    const quote = proto.FlightQuote(
        fuel: 5,
        ticks: 3,
        rounds: 2,
        ratio: .7,
        label: proto.RatioLabel.deadly,
        chase: proto.ChaseForecast(groupId: 9, label: 'cruiser pack', hexes: 2, jumpTicks: 2, roundsPerHex: 1));
    const said = 'THE FLEET IS IN A FIGHT, SO THIS JUMP IS FLIGHT: 5 FUEL, GONE IN 3 TICKS, 2 ROUNDS UNDER FIRE. '
        'THE CRUISER PACK FOLLOWS 2 HEXES AND FIRES 1 ROUND AT EACH ON A FLEET THAT JUMPS ON AT ONCE; ONE THAT STOPS IS CAUGHT.';
    final g = await openMap(tester);
    final m = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await m.addPointer(location: Offset.zero);
    await m.moveTo(at(const proto.Hex(2, -1), _win));
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.byKey(const Key('inspector-flight')), findsNothing, reason: 'a fleet in no fight has no flight to show');
    _answer(g, _preview(const proto.Hex(2, -1), flight: quote));
    await tester.pump(const Duration(milliseconds: 50));
    expect(_text(tester, 'inspector-flight'), said);
    expect(_text(tester, 'route-flight'), said);
    await m.removePointer();
    await endShell(tester, g);
  });

  testWidgets('the map marks the hub the world names, and the inspector says what its exchange charges this player', (tester) async {
    const hub = proto.Hex(2, -1);
    final g = FakeGame();
    g.fake = g.fake.copyWith(
      world: const proto.WorldInfo(
        pace: 10,
        tickHz: 1,
        economyVersion: 2,
        worldgenVersion: 3,
        hub: [proto.HubSite(location: hub, services: [proto.HubService.exchange])],
      ),
      exchange: const proto.ExchangeMeter(pressure: 1046, span: 4185, fee: .375, windowTicks: 15364, clearIn: 3841),
    );
    await pumpShell(tester, size: _win, screen: Screen.map, controller: g);
    await tester.pump(const Duration(milliseconds: 100));
    final painter = tester.widgetList<CustomPaint>(find.byType(CustomPaint)).map((p) => p.painter).whereType<MapPainter>().first;
    expect(painter.data.hub.single.location, hub);

    final m = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await m.addPointer(location: Offset.zero);
    await m.moveTo(at(const proto.Hex(1, -1), _win));
    await tester.pump(const Duration(milliseconds: 50));
    expect(find.byKey(const Key('inspector-hub')), findsNothing);
    await m.moveTo(at(hub, _win));
    await tester.pump(const Duration(milliseconds: 50));
    final note = tester.widget<Text>(find.descendant(of: find.byKey(const Key('inspector-hub')), matching: find.byType(Text)));
    expect(note.data, contains('TRADES CARGO FOR CARGO AT THE EXCHANGE'));
    expect(note.data, contains('YOUR NEXT UNIT PAYS 38%; METER 1046 OF 4185'));
    await shot(tester, 'hub-1440');
    expect(tester.takeException(), isNull);
    await m.removePointer();
    await endShell(tester, g);
  });

  testWidgets('a route into hostiles shows no ratio until the server sends its odds, then the server\'s', (tester) async {
    final g = await openMap(tester);
    const lair = proto.Hex(3, 0);
    await clickHex(tester, lair);
    await tester.pump(const Duration(milliseconds: 300));
    final asked = g.sent.whereType<proto.OddsCommand>().expand((c) => c.sectors).toSet();
    expect(asked, contains(lair));
    expect(g.sent.whereType<proto.OddsCommand>().every((c) => c.fleetId == 7), isTrue);
    expect(find.byKey(const Key('route-odds-pending')), findsOneWidget);
    expect(_text(tester, 'route-preview-text'), contains('RATIO PENDING'));
    expect(find.textContaining('Worst ratio'), findsNothing);

    final threat = g.fake.sectors[lair]!.threat;
    g.fake = g.fake.copyWith(odds: {
      (7, lair): OddsEntry(proto.SectorOdds(sector: lair, threat: threat, ratio: 0.4, label: proto.RatioLabel.deadly), g.fake.tick),
    });
    g.notifyListeners();
    await tester.pump(const Duration(milliseconds: 50));
    expect(find.byKey(const Key('route-odds-pending')), findsNothing);
    expect(_text(tester, 'route-preview-text'), contains('RATIO 0.4 DEADLY'));
    expect(find.textContaining('0.40 DEADLY', findRichText: true), findsWidgets);
    await endShell(tester, g);
  });

  testWidgets('clicking a known sector plans a route: hops, fuel, the charted way home and the time by the fleet\'s own jump', (tester) async {
    final g = await openMap(tester);
    await clickHex(tester, const proto.Hex(4, -1));
    expect(find.text('ROUTE PLAN'), findsOneWidget);
    expect(_text(tester, 'route-hops'), '3');
    expect(_text(tester, 'route-fuel'), '15 of 100');
    expect(_text(tester, 'route-return'), 'OK BY THE CHART, 4 hops, margin 65 fuel');
    expect(_text(tester, 'route-eta'), '0:09', reason: 'three jumps of three ticks each');
    expect(find.byKey(const Key('route-preview-text')), findsOneWidget);
    expect(painterOf(tester).route!.path.length, 3);
    await endShell(tester, g);
  });

  testWidgets('a route prices its fuel from the unrounded jump cost and rounds the total', (tester) async {
    final g = await openMap(tester);
    g.fake = _state(1000, [_fleet(const proto.Hex(1, 0), jumpFuelExact: 5.1)]);
    g.notifyListeners();
    await tester.pump(const Duration(milliseconds: 50));
    await clickHex(tester, const proto.Hex(4, -1));
    expect(_text(tester, 'route-hops'), '3');
    expect(_text(tester, 'route-fuel'), '15 of 100', reason: '3 * 5.1 rounds to 15, which 5 * 3 also does');
    expect(_text(tester, 'route-return'), 'OK BY THE CHART, 4 hops, margin 64 fuel', reason: 'the return is 4 * 5.1, and the margin is rounded once');
    expect(_text(tester, 'route-preview-text'), contains('FUEL 15 OF 100 (16 HOPS LEFT TO RETURN)'));
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
    final layers = tester.getRect(find.byType(LayerBar)), panel = tester.getRect(find.byKey(const Key('sheet-toggle')));
    expect(layers.right < panel.left, isTrue, reason: 'the layer row ends at its own frame, short of the Panel button');
    expect(tester.getRect(find.byKey(const Key('layer-zones'))).left > layers.right, isTrue, reason: 'the last layers are reached by scrolling the row inside that frame');
    expect(find.descendant(of: find.byType(LayerBar), matching: find.byType(KeyCap)), findsNothing, reason: 'a phone has no keys to hint at');
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
