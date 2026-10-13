import 'dart:async';
import 'dart:math' as math;

import 'dart:ui' show PointerDeviceKind;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/services.dart';
import 'package:iac_client/design/panel.dart';
import 'package:iac_client/models/fleet.dart' as ui;
import 'package:iac_client/models/game_state.dart';
import 'package:iac_client/models/homeworld.dart';
import 'package:iac_client/models/resources.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/scenes/windshield/painter.dart' show ArenaPainter;
import 'package:iac_client/state/command_parser.dart';
import 'package:iac_client/state/game_controller.dart';
import 'package:iac_client/views/overlays.dart' show ToastLayer;
import 'package:iac_client/views/rail.dart';
import 'package:iac_client/views/windshield/parts.dart' show ContactsPanel, Dock, Feed, FleetRail, GateCard, SectorHead, StatusPanel, doctrineLine;

import 'support/demo_controller.dart';
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
  bool get isLive => true;

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
        proto.SiteBrief? site,
        bool live = true,
        proto.TerrainType terrain = proto.TerrainType.asteroidField}) =>
    proto.SectorState(
        location: h,
        terrain: terrain,
        resources: res,
        connections: lanes,
        hostiles: hostiles,
        salvage: salvage,
        site: site,
        threat: _threatOf(hostiles),
        lastSeen: 100,
        live: live);

/// What the server would rate these groups: their ships' power, Aggressive and Swarm +25 percent.
proto.ThreatInfo _threatOf(List<proto.NpcFleetInfo>? groups) {
  if (groups == null || groups.isEmpty) return const proto.ThreatInfo(rating: 1, estPower: 4, basis: proto.ThreatBasis.estimate);
  var power = 0.0;
  for (final g in groups) {
    final ships = g.ships.fold(0.0, (n, s) => n + s.shipClass.power * s.count);
    final pushy = g.behavior == proto.NpcBehavior.aggressive || g.behavior == proto.NpcBehavior.swarm;
    power += ships * (pushy ? 1.25 : 1);
  }
  final rating = power <= 4 ? 1 : (1 + (math.log(power / 4) / math.ln2).floor()).clamp(1, 9);
  return proto.ThreatInfo(rating: rating, estPower: power, basis: proto.ThreatBasis.observed);
}

/// A fleet of whole ships, so many of each class named by its label; [rows]
/// gives the rows themselves, for a fleet with damage in it.
ui.FleetState fleet(int id, List<(String, int)> groups,
    {proto.Hex at = here,
    ui.FleetStatus status = ui.FleetStatus.idle,
    int fuel = 80,
    proto.StandingOrders? orders,
    proto.Refining? refine,
    List<proto.ShipGroupState>? rows,
    proto.Refit refit = const proto.Refit(weapons: 0, shielding: 0, armour: 0),
    bool refitDue = false,
    proto.RepairQuote? repair}) {
  final ships = rows ?? [for (final (label, count) in groups) shipRow(proto.ShipClass.values.firstWhere((c) => c.label == label), count)];
  return ui.FleetState(
    orders: orders,
    refine: refine,
    id: id,
    name: 'F$id',
    sector: at,
    status: status,
    shipCount: ships.fold(0, (a, g) => a + g.count),
    ships: ships,
    hull: ships.fold(0, (a, g) => a + g.hull),
    hullMax: ships.fold(0, (a, g) => a + g.hullMax),
    attack: ships.fold(0, (a, g) => a + g.count * g.unitAttack),
    shield: ships.fold(0, (a, g) => a + g.count * g.unitShield),
    refit: refit,
    refitDue: refitDue,
    repair: repair,
    cargo: const ui.FleetCargo(capacity: 100),
    fuel: fuel,
    fuelMax: 120,
    jumpFuel: 5,
    homeFuel: 10,
    power: ships.fold(0.0, (a, g) => a + g.shipClass.power * g.count),
  );
}

GameState world(List<ui.FleetState> fleets, Map<proto.Hex, proto.SectorState> sectors, {int tick = 500}) => GameState(
      tick: tick,
      clockSec: 0,
      resources: const Resources(metal: ResourceStock(amount: 0, rate: 0), crystal: ResourceStock(amount: 0, rate: 0), deut: ResourceStock(amount: 0, rate: 0)),
      fleets: fleets,
      buildQueue: const [],
      shipyard: const [],
      research: const ResearchState(name: 'Idle', time: '-', pct: 0, completed: []),
      events: const [],
      alerts: const [],
      homeworld: home,
      waypoints: const [],
      sectors: sectors,
    );

GameState calm() => world([
      fleet(1, [('Light Fighter', 1), ('Heavy Fighter', 2)]),
      fleet(2, [('Small Cargo', 1)], at: home),
    ], {
      here: sector(here, lanes: [const proto.Hex(2, -1), const proto.Hex(3, -1), const proto.Hex(3, 0)], salvage: const proto.Resources(metal: 40, crystal: 10, deuterium: 3)),
      const proto.Hex(2, -1): sector(const proto.Hex(2, -1), lanes: [here]),
      const proto.Hex(3, -1): sector(const proto.Hex(3, -1), lanes: [here], hostiles: [
        hostileGroup(77, [shipRow(proto.ShipClass.heavyFighter, 2)], proto.NpcBehavior.patrol)
      ]),
      home: sector(home, lanes: [const proto.Hex(1, 0)]),
    });

GameState hostileHere({ui.FleetStatus status = ui.FleetStatus.idle, int fighters = 1}) => world([
      fleet(1, [('Light Fighter', fighters)], status: status),
    ], {
      here: sector(here, lanes: [const proto.Hex(2, -1)], hostiles: [
        hostileGroup(77, [shipRow(proto.ShipClass.heavyFighter, 2), shipRow(proto.ShipClass.cruiser, 1)], proto.NpcBehavior.aggressive)
      ]),
      const proto.Hex(2, -1): sector(const proto.Hex(2, -1), lanes: [here]),
    });

/// The classes that fly, in catalogue order.
final flying = [for (final c in proto.ShipClass.values) if (c != proto.ShipClass.solarSatellite) c];

proto.BattleUnits unitsOf(proto.ShipClass cls, int count, {int lost = 0, int damaged = 0, int atRisk = 0, int hullLost = 0}) => proto.BattleUnits(
    unit: proto.ShipyardItem.ship(cls),
    count: count,
    damaged: damaged,
    atRisk: atRisk,
    hull: count * cls.hull.round() - hullLost,
    hullMax: count * cls.hull.round(),
    lost: lost);

proto.Volley volley(proto.ShipClass from, proto.ShipClass at, int shots, {int bounced = 0, int wasted = 0, int absorbed = 0, int hull = 0, int destroyed = 0}) =>
    proto.Volley(
        from: proto.ShipyardItem.ship(from),
        at: proto.ShipyardItem.ship(at),
        shots: shots,
        bounced: bounced,
        wasted: wasted,
        absorbed: absorbed,
        hullDamage: hull,
        destroyed: destroyed);

/// The second round of three Light Fighters against the pack of [hostileHere]:
/// a fighter and a Heavy Fighter fall in it.
proto.BattleRoundEvent skirmish() => proto.BattleRoundEvent(
      sector: here,
      round: 2,
      fleets: proto.BattleSide(
        groups: [
          proto.BattleGroup(fleetId: 1, owner: 'Pilot', mine: true, units: [unitsOf(proto.ShipClass.lightFighter, 2, lost: 1, damaged: 1, atRisk: 1, hullLost: 150)]),
        ],
        standing: 2,
        volleys: [
          volley(proto.ShipClass.lightFighter, proto.ShipClass.heavyFighter, 2, absorbed: 50, hull: 50, destroyed: 1),
          volley(proto.ShipClass.lightFighter, proto.ShipClass.cruiser, 1, absorbed: 10),
        ],
        shots: 3,
        power: 150,
        absorbed: 60,
        hullDamage: 50,
        destroyed: 1,
      ),
      hostiles: proto.BattleSide(
        groups: [
          proto.BattleGroup(
              fleetId: 77,
              label: '3x heavy fighter pack',
              units: [unitsOf(proto.ShipClass.heavyFighter, 1, lost: 1), unitsOf(proto.ShipClass.cruiser, 1)]),
        ],
        standing: 2,
        volleys: [
          volley(proto.ShipClass.heavyFighter, proto.ShipClass.lightFighter, 2, absorbed: 20, hull: 280, destroyed: 1),
          volley(proto.ShipClass.cruiser, proto.ShipClass.lightFighter, 1, bounced: 1),
        ],
        shots: 3,
        power: 700,
        absorbed: 20,
        hullDamage: 280,
        destroyed: 1,
      ),
      mine: true,
    );

/// The state a tick after [round] was fought: the fleet in the fight, and
/// the fight with its losses so far.
GameState fighting(proto.BattleRoundEvent round) => hostileHere(status: ui.FleetStatus.combat, fighters: 2).copyWith(battles: {
      here: Battle(sector: here, rounds: round.round, round: round, losses: const [
        proto.BattleLosses(fleetId: 1, owner: 'Pilot', mine: true, lost: [proto.UnitCount(unit: proto.ShipyardItem.ship(proto.ShipClass.lightFighter), count: 1)], destroyed: false),
        proto.BattleLosses(
            fleetId: 77,
            label: '3x heavy fighter pack',
            lost: [proto.UnitCount(unit: proto.ShipyardItem.ship(proto.ShipClass.heavyFighter), count: 2)],
            destroyed: false),
      ]),
    });

/// A round in which every class that flies fires at every class: a hundred
/// and forty-four volleys a side.
proto.BattleRoundEvent melee() {
  proto.BattleSide side({required bool ours}) => proto.BattleSide(
        groups: [
          proto.BattleGroup(
              fleetId: ours ? 1 : 77,
              owner: ours ? 'Pilot' : null,
              label: ours ? '' : '4800x small cargo swarm',
              mine: ours,
              units: [for (final c in flying) unitsOf(c, 388, lost: 12, damaged: 31, atRisk: 9, hullLost: 10 * c.hull.round())]),
        ],
        standing: 388 * flying.length,
        volleys: [
          for (final a in flying)
            for (final t in flying)
              volley(a, t, 34 + a.index,
                  bounced: a.index < 2 ? 34 + a.index : 0,
                  wasted: t.index,
                  absorbed: 1200 * (a.index + 1),
                  hull: 90000 * a.index + 1000 * t.index,
                  destroyed: a == proto.ShipClass.deathstar && t == proto.ShipClass.lightFighter ? 180 : a.index ~/ 4),
        ],
        shots: 5712,
        power: 123456789,
        absorbed: 2345678,
        hullDamage: 98765432,
        destroyed: 144,
      );
  return proto.BattleRoundEvent(sector: here, round: 7, fleets: side(ours: true), hostiles: side(ours: false), mine: true);
}

GameState rated(GameState s, proto.Hex at, double ratio, proto.RatioLabel label) => s.copyWith(odds: {
      (1, at): OddsEntry(proto.SectorOdds(sector: at, threat: s.sectors[at]!.threat, ratio: ratio, label: label), s.tick),
    });

const _emptyMeter = proto.ExchangeMeter(pressure: 0, span: 4185, fee: .25, windowTicks: 15364, clearIn: 0);

/// What the season-pace server answers for selling [amount] from an empty meter.
proto.ExchangeQuote quoteOf({
  double amount = 632,
  double receive = 225,
  proto.ResourceKind give = proto.ResourceKind.metal,
  proto.ResourceKind want = proto.ResourceKind.deuterium,
  String? refusal,
}) {
  final value = amount * give.weight;
  final par = value / want.weight;
  return proto.ExchangeQuote(
    terms: proto.ExchangeTerms(
      fleetId: 1,
      give: give,
      want: want,
      amount: amount,
      receive: receive,
      par: par,
      fee: 1 - receive / par,
      meter: _emptyMeter,
      meterAfter: proto.ExchangeMeter(
          pressure: value, span: 4185, fee: .25 + .5 * math.min(1, value / 4185), windowTicks: 15364, clearIn: (value / 4185 * 15364).ceil()),
    ),
    refusal: refusal,
  );
}

/// One Small Cargo of fleet 1 at [at] in a world whose hub, with its exchange, is the origin.
GameState atHub({
  proto.Hex at = proto.Hex.origin,
  proto.Resources hold = const proto.Resources(metal: 632, crystal: 0.4),
  List<proto.ExchangeQuote> quotes = const [],
}) {
  const none = proto.SectorResources(metal: proto.Density.none, crystal: proto.Density.none, deuterium: proto.Density.none);
  const beside = proto.Hex(1, 0);
  final s = world([
    fleet(1, [('Small Cargo', 1)], at: at),
  ], {
    proto.Hex.origin: sector(proto.Hex.origin, lanes: [beside, const proto.Hex(0, 1)], res: none, terrain: proto.TerrainType.empty),
    beside: sector(beside, lanes: [proto.Hex.origin], res: none, terrain: proto.TerrainType.empty),
  });
  return s.copyWith(
    homeworld: const proto.Hex(4, -2),
    world: const proto.WorldInfo(
      pace: 10,
      preset: 'season',
      tickHz: 1,
      economyVersion: 2,
      worldgenVersion: 3,
      hub: [proto.HubSite(location: proto.Hex.origin, services: [proto.HubService.exchange])],
    ),
    holds: {1: hold},
    exchange: _emptyMeter,
    quotes: {for (final q in quotes) (q.terms.fleetId, q.terms.give, q.terms.want): QuoteEntry(q, s.tick)},
  );
}

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

  testWidgets('Stop calls off a jump the windshield is still playing, by its tile and by X; nothing else acts until the jump is over', (tester) async {
    final c = await pumpFake(tester, calm());
    Iterable<int> stops() => c.commands.whereType<proto.StopCommand>().map((s) => s.fleetId);
    int moves() => c.commands.whereType<proto.MoveCommand>().length;

    await key(tester, LogicalKeyboardKey.keyE, 'e');
    await tester.pump(const Duration(milliseconds: 300));
    await key(tester, LogicalKeyboardKey.keyV, 'v');
    expect(c.commands.whereType<proto.ScanCommand>(), isEmpty, reason: 'a scan waits for the jump');
    await tester.tap(find.byKey(const ValueKey('dock-stop')));
    await tester.pump(const Duration(milliseconds: 50));
    expect(stops(), [1], reason: 'the tile sends the stop while the ships fly out');
    await shot(tester, 'ws-jump-called-off-1440');

    await key(tester, LogicalKeyboardKey.keyE, 'e');
    expect(moves(), 2, reason: 'the scene has given the jump up: the gate takes the next order at once');
    await tester.pump(const Duration(milliseconds: 300));
    await key(tester, LogicalKeyboardKey.keyX, 'x');
    expect(stops(), [1, 1], reason: 'and so does X');

    await key(tester, LogicalKeyboardKey.keyE, 'e');
    expect(moves(), 3);
    final there = fleet(1, [('Light Fighter', 1), ('Heavy Fighter', 2)], at: const proto.Hex(3, -1));
    c.push(c.fake.copyWith(fleets: [there, ...c.fake.fleets.skip(1)]));
    await tester.pump(const Duration(milliseconds: 100));
    await key(tester, LogicalKeyboardKey.keyX, 'x');
    expect(stops(), [1, 1], reason: 'a fleet that has arrived has no jump to call off');
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

  testWidgets('a fight the server has not rated yet asks for its odds, shows none of its own, and needs a second F', (tester) async {
    final c = await pumpFake(tester, hostileHere());
    expect(find.byKey(const Key('contact-odds-pending')), findsOneWidget);
    expect(find.byKey(const Key('contact-odds')), findsNothing);
    await tester.pump(const Duration(milliseconds: 250));
    final asked = c.commands.whereType<proto.OddsCommand>().toList();
    expect(asked, hasLength(1));
    expect(asked.single.fleetId, 1);
    expect(asked.single.sectors, [here]);

    await key(tester, LogicalKeyboardKey.keyF, 'f');
    expect(c.commands.whereType<proto.AttackCommand>(), isEmpty);
    expect(find.textContaining('NO ODDS YET'), findsOneWidget);
    await key(tester, LogicalKeyboardKey.keyF, 'f');
    expect(c.commands.whereType<proto.AttackCommand>(), hasLength(1));

    c.push(rated(c.fake, here, 0.31, proto.RatioLabel.deadly));
    await tester.pump(const Duration(milliseconds: 50));
    final shown = tester.widget<Text>(find.byKey(const Key('contact-odds'))).textSpan!.toPlainText();
    expect(shown, contains('DEADLY  ratio 0.31'));
    expect(find.byKey(const Key('contact-odds-pending')), findsNothing);
    await tester.pump(const Duration(seconds: 5));
    await finish(tester, c);
  });

  testWidgets('firing on deadly odds needs a second F', (tester) async {
    final c = await pumpFake(tester, rated(hostileHere(), here, 0.31, proto.RatioLabel.deadly));
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
      here: sector(here, lanes: [const proto.Hex(2, -1)], hostiles: [
        hostileGroup(5, [shipRow(proto.ShipClass.lightFighter, 1)], proto.NpcBehavior.patrol)
      ]),
    });
    final c = await pumpFake(tester, rated(s, here, 2.1, proto.RatioLabel.favourable));
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

  testWidgets('a derelict is boarded with a group in the sector, as the engine allows; with no derelict the order is not sent', (tester) async {
    GameState beside({proto.SiteBrief? site, ui.FleetStatus status = ui.FleetStatus.idle}) => world([
          fleet(1, [('Light Fighter', 1)], status: status),
        ], {
          here: sector(here, lanes: [const proto.Hex(2, -1)], site: site, hostiles: [
            hostileGroup(77, [shipRow(proto.ShipClass.lightFighter, 2)], proto.NpcBehavior.passive)
          ]),
          const proto.Hex(2, -1): sector(const proto.Hex(2, -1), lanes: [here]),
        });
    final c = await pumpFake(tester, beside());
    await key(tester, LogicalKeyboardKey.keyB, 'b');
    expect(find.text('No derelict here.'), findsWidgets);
    expect(c.commands.whereType<proto.ExploreSiteCommand>(), isEmpty);

    c.push(beside(site: const proto.SiteBrief(tier: 2, risk: proto.SiteRisk.uneasy)));
    await tester.pump(const Duration(milliseconds: 100));
    await key(tester, LogicalKeyboardKey.keyB, 'b');
    expect(c.commands.whereType<proto.ExploreSiteCommand>().single.fleetId, 1);
    expect(find.textContaining('Hostiles in range'), findsNothing);
    await tester.pump(const Duration(seconds: 5));
    await finish(tester, c);
  });

  testWidgets('a round is drawn on the scope and told in the panel: what stands, what fell, who fired, the losses so far, and the end', (tester) async {
    for (final size in const [Size(1440, 900), Size(390, 844)]) {
      final wide = size.width > 1000;
      final c = await pumpFake(tester, hostileHere(fighters: 3), size: size);
      expect(find.byKey(const Key('battle-panel')), findsNothing);
      final round = skirmish();
      c.ev.add(const proto.GameEvent(tick: 501, kind: proto.CombatStartedEvent(playerFleetId: 1, owner: 'Pilot', enemyFleetId: 77, sector: here, mine: true)));
      c.ev.add(proto.GameEvent(tick: 501, kind: proto.BattleRoundKind(round)));
      c.push(fighting(round));
      await tester.pump(const Duration(milliseconds: 150));
      if (!wide) {
        expect(textUnder(tester, find.byKey(const ValueKey('battle-strip'))), 'ROUND 2  2 -1  V  2 -1');
        await shot(tester, 'ws-combat-strip-390');
        await tester.tap(find.byKey(const ValueKey('battle-strip')));
        await tester.pump(const Duration(milliseconds: 100));
      }
      final panel = find.byKey(const Key('battle-panel'));
      final told = textUnder(tester, panel);
      expect(told, contains('ROUND 2 - 2, 0'));
      expect(textUnder(tester, find.byKey(const Key('battle-fleets'))), allOf(contains('2 STANDING  -1 THIS ROUND'), contains('LIGHT FIGHTER\n2\n-1'), contains('1 DAMAGED  1 AT RISK')));
      expect(textUnder(tester, find.byKey(const Key('battle-hostiles'))), allOf(contains('3X HEAVY FIGHTER PACK'), contains('HEAVY FIGHTER\n1\n-1'), contains('CRUISER\n1')));
      expect(textUnder(tester, find.byKey(const Key('battle-fire-fleets'))),
          allOf(contains('3 SHOTS  POWER 150  SHIELDS TOOK 60'), contains('LIGHT FIGHTER > HEAVY FIGHTER'), contains('1 DESTROYED'), contains('2 SHOTS  SHIELDS 50  HULL 50')));
      expect(textUnder(tester, find.byKey(const Key('battle-fire-hostiles'))), contains('1 SHOT  1 BOUNCED'), reason: 'a shot too weak for the shield is said to have bounced');
      expect(textUnder(tester, find.byKey(const Key('battle-losses'))), 'F1 LOST 1X LIGHT FIGHTER\n3X HEAVY FIGHTER PACK LOST 2X HEAVY FIGHTER');
      expectTextFits(tester, panel, reason: 'the round panel at ${size.width.round()}');
      await shot(tester, 'ws-combat-${size.width.round()}');
      await tester.pump(const Duration(seconds: 2));
      expect(tester.takeException(), isNull, reason: 'the scope draws the round and the marks it took');

      final end = proto.CombatEndedEvent(
          sector: here,
          playerVictory: true,
          outcome: proto.BattleOutcome.hostilesDestroyed,
          rounds: 3,
          losses: const [
            proto.BattleLosses(fleetId: 1, owner: 'Pilot', mine: true, lost: [proto.UnitCount(unit: proto.ShipyardItem.ship(proto.ShipClass.lightFighter), count: 1)], destroyed: false),
            proto.BattleLosses(
                fleetId: 77,
                label: '3x heavy fighter pack',
                lost: [
                  proto.UnitCount(unit: proto.ShipyardItem.ship(proto.ShipClass.heavyFighter), count: 2),
                  proto.UnitCount(unit: proto.ShipyardItem.ship(proto.ShipClass.cruiser), count: 1),
                ],
                destroyed: true),
          ],
          owners: const ['Pilot'],
          mine: true,
          salvage: const proto.Resources(metal: 900, crystal: 300));
      c.ev.add(proto.GameEvent(tick: 503, kind: end));
      c.push(hostileHere(fighters: 2).copyWith(
          battles: {here: Battle(sector: here, rounds: 3, round: round, losses: end.losses, ended: end, endedTick: 503)}, tick: 503));
      await tester.pump(const Duration(milliseconds: 150));
      expect(tester.widget<Text>(find.byKey(const Key('battle-outcome'))).data, 'HOSTILES DESTROYED');
      expect(textUnder(tester, panel), allOf(contains('ENDED - 3 ROUNDS'), contains('AFTER 3 ROUNDS AT 2, 0')));
      expect(textUnder(tester, find.byKey(const Key('battle-end-losses'))),
          'F1 LOST 1X LIGHT FIGHTER\n3X HEAVY FIGHTER PACK LOST 2X HEAVY FIGHTER, 1X CRUISER AND IS DESTROYED');
      expect(tester.widget<Text>(find.byKey(const Key('battle-wreckage'))).data, 'WRECKAGE LEFT: 900 FE, 300 CR');
      expect(find.byKey(const Key('battle-fire-fleets')), findsNothing, reason: 'the last round is behind its button once the fight is over');
      expectTextFits(tester, panel, reason: 'the end of the fight at ${size.width.round()}');
      await shot(tester, 'ws-combat-end-${size.width.round()}');
      await tester.pump(const Duration(seconds: 4));
      await finish(tester, c);
    }
  });

  testWidgets('twelve classes firing at twelve stay readable: the volleys that decided the round first, the rest behind a button', (tester) async {
    for (final size in const [Size(1440, 900), Size(390, 844)]) {
      final round = melee();
      final s = hostileHere(status: ui.FleetStatus.combat).copyWith(
          fleets: [fleet(1, const [], status: ui.FleetStatus.combat, rows: [for (final c in flying) shipRow(c, 400)])],
          battles: {here: Battle(sector: here, rounds: 7, round: round)});
      final c = await pumpFake(tester, s, size: size);
      if (size.width < 1000) {
        await tester.tap(find.byKey(const ValueKey('battle-strip')));
        await tester.pump(const Duration(milliseconds: 100));
      }
      final fire = find.byKey(const Key('battle-fire-fleets'));
      expect(' > '.allMatches(textUnder(tester, fire)), hasLength(3), reason: 'three volleys of a hundred and forty-four');
      expect(textUnder(tester, fire), contains('DEATHSTAR > LIGHT FIGHTER'), reason: 'the volley that destroyed the most comes first');
      expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('battle-fire-fleets-more'))).label, '141 more volleys');
      expectTextFits(tester, find.byKey(const Key('battle-panel')), reason: 'a round of twelve classes a side at ${size.width.round()}');
      await shot(tester, 'ws-melee-${size.width.round()}');
      await tester.ensureVisible(find.byKey(const ValueKey('battle-fire-fleets-more')));
      await tester.tap(find.byKey(const ValueKey('battle-fire-fleets-more')));
      await tester.pump(const Duration(milliseconds: 100));
      expect(' > '.allMatches(textUnder(tester, fire)), hasLength(144));
      expect(tester.takeException(), isNull);
      await finish(tester, c);
    }
  });

  testWidgets('a fleet of hundreds is a handful of marks on the scope and exact counts in the list beside it', (tester) async {
    for (final size in const [Size(1440, 900), Size(390, 844)]) {
      final rows = [shipRow(proto.ShipClass.smallCargo, 40), shipRow(proto.ShipClass.lightFighter, 412, damaged: 4, atRisk: 2, hullLost: 640), shipRow(proto.ShipClass.cruiser, 60)];
      final s = world([fleet(1, const [], rows: rows)], {
        here: sector(here, lanes: [const proto.Hex(2, -1)], hostiles: [
          hostileGroup(77, [shipRow(proto.ShipClass.lightFighter, 3200), shipRow(proto.ShipClass.battlecruiser, 30)], proto.NpcBehavior.swarm)
        ]),
        const proto.Hex(2, -1): sector(const proto.Hex(2, -1), lanes: [here]),
      });
      final c = await pumpFake(tester, s, size: size);
      final scope = tester.widgetList<CustomPaint>(find.byType(CustomPaint)).map((p) => p.painter).whereType<ArenaPainter>().single.sim;
      expect((scope.own.length, scope.hostiles.length), (3, 8), reason: 'a mark a class of the fleet, and seven for the 3,200 fighters of the swarm');
      expect(scope.scale, 500, reason: 'one scale for both sides: the swarm of 3,230 fits in sixteen marks at 500 a mark');
      if (size.width < 1000) {
        await tester.tap(find.byKey(const ValueKey('status-sheet')));
        await tester.pump(const Duration(milliseconds: 100));
      }
      expect(textUnder(tester, find.byKey(const Key('status-ships'))),
          allOf(contains('SMALL CARGO\n40\n16,000 / 16,000'), contains('LIGHT FIGHTER\n412\n164K / 165K'), contains('4 DAMAGED  2 AT RISK')));
      expect(tester.widget<Text>(find.byKey(const Key('contact-hostile-77'))).data, 'SWARM - 3,230 SHIPS');
      expect(textUnder(tester, find.byKey(const Key('contact-hostile-77-LightFighter'))), contains('3,200\n1.28M / 1.28M'));
      await shot(tester, 'ws-hundreds-${size.width.round()}');
      expect(tester.takeException(), isNull);
      await finish(tester, c);
    }
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
    final c = DemoController()..startDemo();
    await pumpShell(tester, screen: Screen.windshield, controller: c);
    await key(tester, LogicalKeyboardKey.digit2, '2');
    expect(c.activeFleet, 1);
    await endShell(tester, c);
  });

  testWidgets('split a ship off and merge it back through the panel (demo)', (tester) async {
    final c = DemoController()..startDemo();
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

  testWidgets('at the hub the exchange is a contact; T opens the panel, which asks for a quote, shows it and sends the trade', (tester) async {
    final c = await pumpFake(tester, atHub());
    expect(tester.widget<Text>(find.byKey(const Key('contact-exchange'))).data, contains('NEXT UNIT AT 25% - METER 0 OF 4185'));
    expect(find.text('THE HUB - EXCHANGE'), findsOneWidget);
    expect(find.byKey(const ValueKey('ex-trade')), findsNothing);
    await shot(tester, 'ws-hub-1440');

    await key(tester, LogicalKeyboardKey.keyT, 't');
    expect(find.byKey(const Key('ex-pending')), findsOneWidget);
    expect(tester.widget<Text>(find.descendant(of: find.byKey(const Key('ex-why')), matching: find.byType(Text))).data, 'Waiting for the quote.');
    expect(c.commands.whereType<proto.ExchangeQuoteCommand>(), isEmpty, reason: 'debounced');
    await tester.pump(const Duration(milliseconds: 250));
    var asked = c.commands.whereType<proto.ExchangeQuoteCommand>().single;
    expect((asked.fleetId, asked.give, asked.want, asked.amount), (1, proto.ResourceKind.metal, proto.ResourceKind.crystal, null), reason: 'what the hold carries most of, all of it');

    await tester.tap(find.byKey(const ValueKey('ex-want-deuterium')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 250));
    asked = c.commands.whereType<proto.ExchangeQuoteCommand>().last;
    expect((asked.give, asked.want, asked.amount), (proto.ResourceKind.metal, proto.ResourceKind.deuterium, null));
    await tester.tap(find.byKey(const ValueKey('ex-trade')));
    await tester.pump(const Duration(milliseconds: 50));
    expect(c.commands.whereType<proto.ExchangeCommand>(), isEmpty, reason: 'nothing is traded before the server has quoted it');

    c.push(atHub(quotes: [quoteOf()]));
    await tester.pump(const Duration(milliseconds: 600));
    expect(tester.widget<Text>(find.byKey(const Key('ex-quote'))).textSpan!.toPlainText(), '632 Fe  GIVES  225 De');
    expect(find.text('29% OF PAR 316 DE'), findsOneWidget);
    expect(find.text('0 TO 632 OF 4185'), findsOneWidget);
    expect(find.text('25% TO 33%'), findsOneWidget);
    expect(find.text('4185 PER 4:16:04 - EMPTY IN 38:41'), findsOneWidget);
    expect(find.descendant(of: find.byKey(const Key('ex-meter-note')), matching: find.textContaining('The meter is what you have sold here lately.')), findsOneWidget,
        reason: 'the panel says what its meter is');
    expect(find.byKey(const Key('ex-why')), findsNothing);
    await shot(tester, 'ws-exchange-1440');

    await tester.tap(find.byKey(const ValueKey('ex-trade')));
    await tester.pump(const Duration(milliseconds: 50));
    final trade = c.commands.whereType<proto.ExchangeCommand>().single;
    expect((trade.fleetId, trade.give, trade.want, trade.amount), (1, proto.ResourceKind.metal, proto.ResourceKind.deuterium, null));
    c.ev.add(proto.GameEvent(tick: 501, kind: proto.ExchangedEvent(sector: proto.Hex.origin, terms: quoteOf().terms)));
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('TRADED'), findsOneWidget);
    expect(find.text('632 METAL FOR 225 DEUTERIUM, FEE 29%.'), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey('ex-share-50')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 250));
    expect(c.commands.whereType<proto.ExchangeQuoteCommand>().last.amount, 316);
    expect(find.byKey(const Key('ex-pending')), findsOneWidget, reason: 'the quote on screen was for another amount');

    await key(tester, LogicalKeyboardKey.keyT, 't');
    expect(find.byKey(const ValueKey('ex-trade')), findsNothing);
    expect(tester.takeException(), isNull);
    await finish(tester, c);
  });

  testWidgets('a trade the server would refuse is disabled with the server\'s reason; away from the hub the panel only quotes', (tester) async {
    const why = 'fleet 1 is at [1,0], 1 hex from the exchange at the hub [0,0]; a fleet trades what is in its hold, on the spot';
    final c = await pumpFake(tester, atHub(at: const proto.Hex(1, 0), quotes: [quoteOf(want: proto.ResourceKind.crystal, receive: 300, refusal: why)]));
    expect(find.byKey(const Key('contact-exchange')), findsNothing);
    await tester.tap(find.byKey(const ValueKey('dock-trade')));
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('quote only'), findsOneWidget);
    expect(tester.widget<Text>(find.byKey(const Key('ex-quote'))).textSpan!.toPlainText(), '632 Fe  GIVES  300 Cr');
    expect(tester.widget<Text>(find.descendant(of: find.byKey(const Key('ex-why')), matching: find.byType(Text))).data, why);
    await tester.tap(find.byKey(const ValueKey('ex-trade')));
    await tester.pump(const Duration(milliseconds: 50));
    expect(c.commands.whereType<proto.ExchangeCommand>(), isEmpty);
    expect(tester.takeException(), isNull);
    await tester.pump(const Duration(seconds: 1));
    await finish(tester, c);
  });

  testWidgets('390 wide: the exchange panel opens in the sheet with no overflow, long numbers and a long refusal included', (tester) async {
    const big = proto.Resources(metal: 123456.7, crystal: 98765.4, deuterium: 54321);
    const why = '123457 metal returns 46296 deuterium, and the hold of fleet 1 has no room for it: it carries 276543 of 276600 and would carry 199382; trade a smaller amount';
    final c = await pumpFake(tester, atHub(hold: big, quotes: [quoteOf(amount: 123456.7, receive: 46296, want: proto.ResourceKind.crystal, refusal: why)]), size: const Size(390, 844));
    await tester.tap(find.byKey(const ValueKey('dock-trade')));
    await tester.pump(const Duration(milliseconds: 600));
    expect(find.byKey(const Key('ex-quote')), findsOneWidget);
    expect(find.byKey(const Key('ex-why')), findsOneWidget);
    await shot(tester, 'ws-exchange-390');
    expect(tester.takeException(), isNull);
    await finish(tester, c);

    final wide = await pumpFake(tester, atHub(hold: big, quotes: [quoteOf(amount: 123456.7, receive: 46296, want: proto.ResourceKind.crystal, refusal: why)]));
    await key(tester, LogicalKeyboardKey.keyT, 't');
    await tester.pump(const Duration(milliseconds: 600));
    expect(find.byKey(const Key('ex-why')), findsOneWidget);
    await shot(tester, 'ws-exchange-refused-1440');
    expect(tester.takeException(), isNull);
    await finish(tester, wide);
  });

  testWidgets('docked at home the homeworld contact offers the load control; L opens it for this fleet, and a laden hold says so (demo)', (tester) async {
    const demoHome = proto.Hex(4, -2);
    final c = DemoController()..startDemo();
    c.place(101, demoHome, const proto.Resources());
    await pumpShell(tester, screen: Screen.windshield, controller: c);
    Screen screen() => Console.of(tester.element(find.byType(Rail))).screen;
    final dock = find.byKey(const Key('contact-dock'));
    expect(tester.widget<Text>(dock).data, allOf(contains('NOTHING IN THE FIELD REACHES IT'), contains('A RAID THAT BREAKS THROUGH STILL DESTROYS SOME DOCKED SHIPS')),
        reason: 'the dock is out of the field, not out of a raid\'s reach');
    expect(tester.widget<Text>(dock).data, isNot(contains('SAFE')));

    await tester.tap(find.byKey(const ValueKey('contact-load')));
    await tester.pump(const Duration(milliseconds: 300));
    expect((screen(), c.hwTab), (Screen.homeworld, HomeworldTab.storage));
    expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('hold-fleet-101'))).primary, isTrue, reason: 'the fleet the player came from');
    await tester.ensureVisible(find.byKey(const ValueKey('hold-load')));
    await tester.tap(find.byKey(const ValueKey('hold-load')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(c.state.holds[101]!.metal, c.currentFleet.holdCapacity, reason: 'all that fits of the first resource');

    Console.of(tester.element(find.byType(Rail))).go(Screen.windshield);
    await tester.pump(const Duration(milliseconds: 300));
    expect(tester.widget<Text>(dock).data,
        allOf(contains('HOLD LADEN: IT STAYS ABOARD UNTIL THE FLEET HAS BEEN AWAY.'), contains('TAKES ITS SHARE OF THE HOLD'), isNot(contains('UNLOADS'))));
    final card = tester.getRect(find.byType(ContactsPanel)), words = tester.getRect(dock);
    expect(words.left >= card.left && words.right <= card.right, isTrue, reason: 'the longer sentence wraps inside the contacts panel');
    expect(find.text('CARGO - LADEN'), findsOneWidget);
    await shot(tester, 'ws-home-laden-1440');
    c.selectHomeworldTab(HomeworldTab.buildings);
    await key(tester, LogicalKeyboardKey.keyL, 'l');
    await tester.pump(const Duration(milliseconds: 300));
    expect((screen(), c.hwTab), (Screen.homeworld, HomeworldTab.storage));

    c.selectFleet(2);
    Console.of(tester.element(find.byType(Rail))).go(Screen.windshield);
    await tester.pump(const Duration(milliseconds: 300));
    await key(tester, LogicalKeyboardKey.keyL, 'l');
    await tester.pump(const Duration(milliseconds: 300));
    expect(screen(), Screen.windshield, reason: 'a fleet away from home is told why, and stays');
    expect(find.text('A hold is loaded from the stockpile at the homeworld only.'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  testWidgets('a fleet at the hub quotes and trades against the scripted world, by panel and by typed command (demo)', (tester) async {
    final c = DemoController()..startDemo();
    c.place(101, proto.Hex.origin, const proto.Resources(metal: 200, crystal: 12));
    await pumpShell(tester, screen: Screen.windshield, controller: c);
    expect(c.state.exchangeAt(c.currentFleet.sector), isTrue);
    expect(find.byKey(const Key('contact-exchange')), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey('contact-trade')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 250));
    await tester.pump(const Duration(milliseconds: 50));
    final quote = c.state.quoteFor(101, proto.ResourceKind.metal, proto.ResourceKind.crystal)!;
    expect(quote.refusal, isNull);
    expect(quote.terms.amount, 200);
    expect(quote.terms.receive, lessThanOrEqualTo(200 * .75 / 1.5));
    expect(tester.widget<Text>(find.byKey(const Key('ex-quote'))).textSpan!.toPlainText(), '200 Fe  GIVES  ${quote.terms.receive.round()} Cr');

    await tester.tap(find.byKey(const ValueKey('ex-trade')));
    await tester.pump(const Duration(milliseconds: 100));
    final hold = c.state.holds[101]!;
    expect((hold.metal, hold.crystal), (0, 12 + quote.terms.receive));
    expect(c.state.exchange.pressure, closeTo(200, 2), reason: 'the weight sold, less what a tick or two of the demo forgot');
    expect(c.state.events.map((e) => e.message), contains(startsWith('F101 traded 200 metal for ${quote.terms.receive.round()} crystal at the exchange')));
    expect(tester.widget<Text>(find.byKey(const Key('contact-exchange'))).data, matches(RegExp(r'METER (19\d|200) OF')));

    c.handleCommand('quote crystal metal 40');
    await tester.pump(const Duration(milliseconds: 50));
    expect(c.state.events.map((e) => e.message), contains(startsWith('Quote F101: 40 crystal gives')));
    c.handleCommand('trade crystal for metal all');
    await tester.pump(const Duration(milliseconds: 50));
    expect(c.state.holds[101]!.crystal, 0);
    c.handleCommand('trade metal metal');
    expect(c.state.events.first.message, 'trade: give and want are both metal');
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  testWidgets('the status panel offers to refine exactly what the state says a refine order would do, and is greyed without it', (tester) async {
    final c = await pumpFake(tester, calm());
    ConsoleButton button() => tester.widget<ConsoleButton>(find.byKey(const ValueKey('status-refine')));
    String line() => tester.widget<Text>(find.byKey(const Key('status-refine-yield'))).data!;
    expect(button().onPressed, isNull, reason: 'the state carries no offer');
    expect(line(), 'NOTHING TO REFINE');
    expect(button().reason, startsWith('Refining turns the deuterium in the hold into fuel, 15% less of it than a dock gives.'));

    c.push(calm().copyWith(fleets: [
      fleet(1, [('Light Fighter', 1), ('Heavy Fighter', 2)], refine: const proto.Refining(deuterium: 0.8, fuel: 8.5)),
      fleet(2, [('Small Cargo', 1)], at: home),
    ]));
    await tester.pump();
    expect(line(), '0.8 DE MAKES 8.5 FUEL');
    await tester.tap(find.byKey(const ValueKey('status-refine')));
    await tester.pump();
    expect(c.commands.whereType<proto.RefineCommand>().map((r) => r.fleetId), [1]);
    await finish(tester, c);
  });

  testWidgets('refine by the panel and by typed command is answered by the scripted world as the engine answers (demo)', (tester) async {
    final c = DemoController()..startDemo();
    c.place(101, proto.Hex.origin, const proto.Resources(metal: 7, deuterium: 1.6));
    await pumpShell(tester, screen: Screen.windshield, controller: c);
    String line() => tester.widget<Text>(find.byKey(const Key('status-refine-yield'))).data!;
    final fuel = c.currentFleet.fuel, tank = c.currentFleet.fuelMax;
    expect(tank - fuel, greaterThan(17), reason: 'the script opens on a tank with room in it');
    expect(line(), '1.6 DE MAKES 17 FUEL', reason: 'the dock would give 20 for it');

    await tester.tap(find.byKey(const ValueKey('status-refine')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(c.state.events.map((e) => e.message),
        contains('fleet 101 refined 1.6 deuterium into 17 fuel: the tank stands at ${fuel + 17} of $tank, and the hold has no deuterium left'));
    expect((c.currentFleet.fuel, c.state.holds[101]!.deuterium, c.state.holds[101]!.metal), (fuel + 17, 0, 7));
    expect(line(), 'NOTHING TO REFINE');
    expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('status-refine'))).onPressed, isNull);

    c.handleCommand('refine');
    expect(c.state.events.first.message, endsWith('the hold of fleet 101 carries no deuterium to refine'));
    c.handleCommand('refine all');
    expect(c.state.events.first.message, startsWith('refine: takes no argument'));

    c.place(101, proto.Hex.origin, const proto.Resources(deuterium: 40));
    c.handleCommand('refine');
    await tester.pump(const Duration(milliseconds: 100));
    expect(c.currentFleet.fuel, tank, reason: 'the tank is full');
    expect(c.state.holds[101]!.deuterium, closeTo(40 - (tank - fuel - 17) / 10.625, 1e-6), reason: 'and the rest stays aboard');
    c.handleCommand('refine');
    expect(c.state.events.first.message, endsWith('the tank of fleet 101 is full ($tank of $tank fuel); refining would add nothing'));
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  testWidgets('P opens the standing orders: every doctrine with its line, the settings in the rules\' units, and Set sends what the typed policy sends', (tester) async {
    final c = await pumpFake(tester, calm());
    expect(find.byKey(const ValueKey('orders-panel')), findsNothing);
    expect(tester.widget<Text>(find.descendant(of: find.byKey(const Key('status-orders')), matching: find.text('MANUAL'))).data, 'MANUAL');
    expect(find.descendant(of: find.byType(StatusPanel), matching: find.text('16 HOPS')), findsOneWidget, reason: 'fuel for 16 hops');
    expect(find.descendant(of: find.byKey(const Key('status-way-home')), matching: find.text('10 FUEL')), findsOneWidget, reason: 'the way home needs 10 fuel');
    expect(find.text('-5 FUEL'), findsNWidgets(3), reason: 'each gate card says what the jump burns, in words');

    await key(tester, LogicalKeyboardKey.keyP, 'p');
    final panel = find.byKey(const ValueKey('orders-panel'));
    expect(panel, findsOneWidget);
    expect(find.descendant(of: find.byKey(const Key('orders-in-force')), matching: find.text('MANUAL')), findsOneWidget);
    for (final p in proto.PolicyPreset.values) {
      expect(find.descendant(of: find.byKey(ValueKey('orders-doctrine-${p.name}')), matching: find.text(p.label.toUpperCase())), findsOneWidget);
      expect(find.descendant(of: find.byKey(ValueKey('orders-doctrine-${p.name}')), matching: find.text(doctrineLine(p))), findsOneWidget);
    }
    expect(find.byKey(const ValueKey('orders-range-up')), findsNothing, reason: 'manual has no settings');
    expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('orders-set'))).onPressed, isNull);
    expect(find.descendant(of: find.byKey(const Key('orders-why')), matching: find.text('F1 has no standing orders.')), findsOneWidget);
    expect(find.descendant(of: find.byKey(const Key('orders-report')), matching: find.text('NO REPORT YET')), findsOneWidget);
    await shot(tester, 'ws-orders-manual-1440');

    await tester.tap(find.byKey(const ValueKey('orders-doctrine-mineAndReturn')));
    await tester.pump();
    String shown(String id) => tester.widget<Text>(find.byKey(Key('orders-$id'))).data!;
    expect((shown('range'), shown('engage'), shown('fuel')), ('4 HEXES', '1.5X', '20%'), reason: 'the server\'s defaults');
    for (var i = 0; i < 4; i++) {
      await tester.tap(find.byKey(const ValueKey('orders-range-up')));
      await tester.pump();
    }
    await tester.ensureVisible(find.byKey(const ValueKey('orders-engage-up')));
    await tester.tap(find.byKey(const ValueKey('orders-engage-up')));
    await tester.pump();
    await tester.ensureVisible(find.byKey(const ValueKey('orders-fuel-up')));
    await tester.tap(find.byKey(const ValueKey('orders-fuel-up')));
    await tester.pump();
    expect((shown('range'), shown('engage'), shown('fuel')), ('8 HEXES', '2.0X', '35%'));
    await shot(tester, 'ws-orders-form-1440');

    await tester.ensureVisible(find.byKey(const ValueKey('orders-set')));
    expect(find.descendant(of: find.byKey(const ValueKey('orders-set')), matching: find.text('SET MINE+RETURN')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('orders-set')));
    await tester.pump(const Duration(milliseconds: 50));
    final typed = parseCommand(
        'policy mine range=8 engage=2 fuel=35',
        const CommandContext(fleetId: 1, fleetSector: here, sectors: {}, fleets: [], cursor: here, tick: 500)) as ParsedSend;
    expect(c.sent.whereType<proto.PolicyUpdate>().single.toJson(), typed.messages.single.toJson());
    expect(typed.messages.single.toJson(), {
      'type': 'policy_update',
      'fleet_id': 1,
      'preset': 'mine_and_return',
      'params': {'min_fuel_pct': 10, 'cargo_return_pct': 85, 'max_range': 8, 'engage_ratio_x10': 20, 'fuel_budget_pct': 35, 'refine': false},
    });

    const held = 'refuelling on a fuel budget of 50% of deuterium income (tank 80 of 120, full in about 236 s)';
    c.push(world([
      fleet(1, [('Light Fighter', 1), ('Heavy Fighter', 2)],
          orders: const proto.StandingOrders(
              preset: proto.PolicyPreset.mineAndReturn,
              params: proto.PolicyParams(minFuelPct: 15, cargoReturnPct: 60, maxRange: 6, engageRatioX10: 30, fuelBudgetPct: 50))),
    ], calm().sectors, tick: 520)
        .copyWith(policyActs: {
      1: const [
        PolicyAct(tick: 518, since: 506, preset: proto.PolicyPreset.mineAndReturn, action: 'hold', reason: held),
        PolicyAct(tick: 503, since: 503, preset: proto.PolicyPreset.mineAndReturn, action: 'move', reason: 'heading to claim'),
      ],
    }));
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.descendant(of: find.byKey(const Key('orders-in-force')), matching: find.text('MINE+RETURN')), findsOneWidget);
    expect(tester.widget<Text>(find.byKey(const Key('orders-settings'))).data, 'RANGE 6 HEXES - ENGAGE 3.0X - FUEL 50%',
        reason: 'the fleet row carries the settings in force, whoever gave the orders and whenever');
    expect((shown('range'), shown('engage'), shown('fuel')), ('6 HEXES', '3.0X', '50%'), reason: 'the steppers start from the orders in force, not from what was last set here');
    expect(find.text('It turns home at 60% hold, or with fuel down to the way back plus 15% of the tank.'), findsOneWidget);
    expect(tester.widget<Text>(find.byKey(const Key('orders-report-head'))).data, 'HOLDING FOR 0:14');
    expect(tester.widget<Text>(find.byKey(const Key('orders-report-reason'))).data, held, reason: 'the engine\'s sentence, as it sent it');
    expect(find.textContaining('heading to claim', findRichText: true), findsOneWidget);
    expect(find.descendant(of: find.byKey(const Key('status-orders')), matching: find.text('MINE+RETURN - HOLDING')), findsOneWidget);
    expect(find.descendant(of: find.byKey(const Key('status-hold')), matching: find.text(held)), findsOneWidget, reason: 'why it holds shows with the panel shut too');
    expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('orders-set'))).onPressed, isNull);
    expect(find.descendant(of: find.byKey(const Key('orders-why')), matching: find.text('These are the orders in force.')), findsOneWidget);
    await shot(tester, 'ws-orders-holding-1440');
    await tester.ensureVisible(find.byKey(const ValueKey('orders-range-up')));
    await tester.tap(find.byKey(const ValueKey('orders-range-up')));
    await tester.pump();
    expect(shown('range'), '7 HEXES');
    await tester.ensureVisible(find.byKey(const ValueKey('orders-set')));
    await tester.tap(find.byKey(const ValueKey('orders-set')));
    await tester.pump(const Duration(milliseconds: 50));
    expect(c.sent.whereType<proto.PolicyUpdate>().last.toJson()['params'],
        {'min_fuel_pct': 15, 'cargo_return_pct': 60, 'max_range': 7, 'engage_ratio_x10': 30, 'fuel_budget_pct': 50, 'refine': false},
        reason: 'one step changes one setting and keeps the others as they are in force');

    await key(tester, LogicalKeyboardKey.escape, '');
    expect(panel, findsNothing);
    expect(find.byKey(const Key('status-hold')), findsOneWidget);
    expect(tester.takeException(), isNull);
    await finish(tester, c);
  });

  testWidgets('orders given through the panel come back from the scripted world as the engine reports them: set, hold with its reason, a hand-loaded hold unloaded, cleared (demo)', (tester) async {
    const demoHome = proto.Hex(4, -2);
    final c = DemoController()..startDemo();
    c.place(101, demoHome, const proto.Resources());
    await pumpShell(tester, screen: Screen.windshield, controller: c);
    c.handleCommand('load metal 50');
    await tester.pump(const Duration(milliseconds: 100));
    expect(c.currentFleet.laden, isTrue);

    await tester.tap(find.byKey(const ValueKey('orders-toggle')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.byKey(const Key('orders-laden')), findsNothing, reason: 'manual leaves a hold loaded by hand alone');
    await tester.tap(find.byKey(const ValueKey('orders-doctrine-patrolHome')));
    await tester.pump();
    expect(tester.widget<Text>(find.byKey(const Key('orders-laden'))).data,
        'The hold of F101 was loaded by hand. Standing orders unload at the dock: giving them ends that, and the hold goes back to the stockpile.');
    await shot(tester, 'ws-orders-laden-1440');
    await tester.ensureVisible(find.byKey(const ValueKey('orders-set')));
    await tester.tap(find.byKey(const ValueKey('orders-set')));
    await tester.pump(const Duration(milliseconds: 100));
    String reason() => tester.widget<Text>(find.byKey(const Key('orders-report-reason'))).data!;
    String head() => tester.widget<Text>(find.byKey(const Key('orders-report-head'))).data!;
    const unloaded = 'doctrine set: patrol home (range 4, engage 1.5x, fuel budget 20% of deuterium income); the hold loaded by hand goes back to the stockpile';
    expect(find.descendant(of: find.byKey(const ValueKey('orders-panel')), matching: find.textContaining(unloaded, findRichText: true)), findsOneWidget,
        reason: 'the engine\'s report of the new orders, which says what became of the hold');
    expect((c.currentFleet.laden, c.state.holds[101]!.metal), (false, 0));
    expect(c.state.events.map((e) => e.message), containsAll(['F101 [patrol home] engage: $unloaded', '> policy patrol']),
        reason: 'the log shows the report and the line a player would have typed');
    expect(tester.widget<Text>(find.byKey(const Key('orders-settings'))).data, 'RANGE 4 HEXES - ENGAGE 1.5X - FUEL 20%');

    await tester.pump(const Duration(seconds: 1));
    expect(head(), startsWith('HOLDING FOR'));
    expect(reason(), 'no contact within 4 hops of home that the fleet rates 1.5x against');
    expect(find.descendant(of: find.byKey(const Key('status-hold')), matching: find.text(reason())), findsOneWidget);
    expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('orders-set'))).onPressed, isNull);
    expect(find.descendant(of: find.byKey(const Key('orders-why')), matching: find.text('These are the orders in force.')), findsOneWidget);

    await tester.ensureVisible(find.byKey(const ValueKey('orders-range-down')));
    await tester.tap(find.byKey(const ValueKey('orders-range-down')));
    await tester.pump();
    await tester.ensureVisible(find.byKey(const ValueKey('orders-fuel-up')));
    for (var i = 0; i < 4; i++) {
      await tester.tap(find.byKey(const ValueKey('orders-fuel-up')));
      await tester.pump();
    }
    expect(tester.widget<Text>(find.byKey(const Key('orders-fuel'))).data, 'NO BOUND');
    expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('orders-fuel-up'))).onPressed, isNull);
    await tester.ensureVisible(find.byKey(const ValueKey('orders-set')));
    await tester.tap(find.byKey(const ValueKey('orders-set')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(c.state.policyActs[101]!.map((a) => a.reason), contains('doctrine set: patrol home (range 3, engage 1.5x, fuel unbounded)'));
    expect(tester.widget<Text>(find.byKey(const Key('orders-settings'))).data, 'RANGE 3 HEXES - ENGAGE 1.5X - FUEL NO BOUND');
    await tester.pump(const Duration(seconds: 1));
    expect(reason(), 'no contact within 3 hops of home that the fleet rates 1.5x against');

    c.send(const proto.PolicyUpdate(fleetId: 101, preset: proto.PolicyPreset.patrolHome, params: proto.PolicyParams(maxRange: 2)));
    await tester.pump(const Duration(milliseconds: 100));
    expect(c.state.policyActs[101]!.map((a) => a.reason), contains('doctrine set: patrol home (range 2, engage 1.5x, fuel budget 20% of deuterium income)'));
    expect(tester.widget<Text>(find.byKey(const Key('orders-settings'))).data, 'RANGE 2 HEXES - ENGAGE 1.5X - FUEL 20%',
        reason: 'orders given from another console show with their settings, as the fleet row carries them');
    expect(tester.widget<Text>(find.byKey(const Key('orders-range'))).data, '2 HEXES', reason: 'and the steppers follow them');

    c.handleCommand('policy engage=3 cargo=60');
    await tester.pump(const Duration(milliseconds: 100));
    expect(tester.widget<Text>(find.byKey(const Key('orders-settings'))).data, 'RANGE 2 HEXES - ENGAGE 3.0X - FUEL 20%',
        reason: 'settings typed with no doctrine change the orders in force and leave the rest of them');
    expect(find.text('It turns home at 60% hold, or with fuel down to the way back plus 10% of the tank.'), findsOneWidget);

    final refineSwitch = find.byKey(const ValueKey('orders-refine'));
    await tester.ensureVisible(refineSwitch);
    expect(find.descendant(of: refineSwitch, matching: find.text('OFF')), findsOneWidget, reason: 'off unless the orders say so');
    await tester.tap(refineSwitch);
    await tester.pump();
    expect(find.descendant(of: refineSwitch, matching: find.text('ON')), findsOneWidget);
    await tester.ensureVisible(find.byKey(const ValueKey('orders-set')));
    await tester.tap(find.byKey(const ValueKey('orders-set')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(c.state.policyActs[101]!.map((a) => a.reason),
        contains('doctrine set: patrol home (range 2, engage 3.0x, fuel budget 20% of deuterium income, refining from the hold when short of fuel)'));
    expect(c.currentFleet.orders!.params.refine, isTrue);
    expect(c.state.events.map((e) => e.message), contains(allOf(startsWith('> policy patrol '), endsWith(' refine=on'))),
        reason: 'the line a player would have typed');
    c.handleCommand('policy refine=off');
    await tester.pump(const Duration(milliseconds: 100));
    expect((c.currentFleet.orders!.params.refine, c.currentFleet.orders!.params.maxRange), (false, 2),
        reason: 'the switch typed alone changes the orders in force and leaves the rest');
    c.handleCommand('policy refine=maybe');
    expect(c.state.events.first.message, startsWith('policy: manual|prospect|mine|salvage|patrol'));

    c.handleCommand('load metal 10');
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.descendant(of: find.byType(ToastLayer), matching: find.textContaining('is under standing orders (patrol home)')), findsOneWidget,
        reason: 'a hold is not loaded by hand under orders, and the refusal is a toast');

    await tester.ensureVisible(find.byKey(const ValueKey('orders-doctrine-manual')));
    await tester.tap(find.byKey(const ValueKey('orders-doctrine-manual')));
    await tester.pump();
    expect(find.descendant(of: find.byKey(const ValueKey('orders-set')), matching: find.text('CLEAR ORDERS')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('orders-set')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(head(), startsWith('LAST REPORT: CLEAR'));
    expect(reason(), 'standing orders cleared');
    expect(find.descendant(of: find.byKey(const Key('orders-in-force')), matching: find.text('MANUAL')), findsOneWidget);
    expect(find.byKey(const Key('status-hold')), findsNothing);
    expect(c.currentFleet.policy, isNull);
    expect(tester.takeException(), isNull);
    await tester.pump(const Duration(seconds: 10));
    await endShell(tester, c);
  });

  testWidgets('in the phone layout the open status sheet stands on an opaque ground, and a banner is drawn under it, at 390 and 1100 wide', (tester) async {
    for (final size in const [Size(390, 844), Size(1100, 800)]) {
      final c = await pumpFake(tester, calm(), size: size);
      await tester.tap(find.byKey(const ValueKey('status-sheet')));
      await tester.pump(const Duration(milliseconds: 100));
      final sheet = find.byKey(const ValueKey('status-sheet-open'));
      final ground = find.descendant(of: sheet, matching: find.byType(ColoredBox)).first;
      expect(tester.widget<ColoredBox>(ground).color.a, 1.0, reason: 'nothing of the map shows through the sheet');
      for (final panel in [find.byType(StatusPanel), find.byType(ContactsPanel)]) {
        final at = tester.getRect(panel), under = tester.getRect(ground);
        expect(under.left <= at.left && under.right >= at.right && under.top <= at.top && under.bottom >= at.bottom, isTrue, reason: 'the ground is under every panel of the sheet');
      }

      c.push(world([fleet(1, [('Light Fighter', 1), ('Heavy Fighter', 2)], at: const proto.Hex(2, -1)), fleet(2, [('Small Cargo', 1)], at: home)], calm().sectors, tick: 503));
      await tester.pump(const Duration(milliseconds: 100));
      await tester.pump(const Duration(milliseconds: 600));
      expect(find.descendant(of: find.byKey(const ValueKey('banner')), matching: find.textContaining('SECTOR')), findsOneWidget, reason: 'a banner is up');
      final layers = tester.widget<Stack>(find.ancestor(of: sheet, matching: find.byType(Stack)).first).children.map((w) => w.key).toList();
      expect(layers.indexOf(const ValueKey('banner')), lessThan(layers.indexOf(const ValueKey('status-sheet-open'))),
          reason: 'the sheet is drawn after the banner, so the banner does not cover what the player opened to read');
      await shot(tester, 'ws-sheet-banner-${size.width.round()}');
      expect(tester.takeException(), isNull);
      await finish(tester, c);
    }
  });

  testWidgets('with eight fleets the desktop fleet rail ends above the feed and scrolls; the fleet selected is brought into view', (tester) async {
    final eight = world([
      fleet(1, [('Light Fighter', 1), ('Heavy Fighter', 2)]),
      for (var id = 2; id <= 8; id++) fleet(id, [('Small Cargo', 1)], at: home),
    ], calm().sectors).copyWith(events: [
      for (var i = 0; i < 12; i++) LogEntry(tick: 480 + i, message: 'F1 harvested 40 Fe, 12 Cr over 10s', level: EventLevel.normal),
    ]);
    for (final size in const [Size(1440, 900), Size(1440, 720), Size(1200, 800)]) {
      final c = await pumpFake(tester, eight, size: size);
      final rail = tester.getRect(find.byType(FleetRail)), feed = tester.getRect(find.byType(Feed));
      expect(rail.bottom <= feed.top, isTrue, reason: 'at $size the rail ($rail) ends above the feed ($feed)');
      Rect card(int id) => tester.getRect(find.byKey(ValueKey('fleet-card-$id')));
      bool shown(int id) => card(id).top >= rail.top - .5 && card(id).bottom <= rail.bottom + .5;
      expect((shown(1), shown(8)), (true, false), reason: 'eight cards are more than the rail has room for: the last is scrolled out');

      await shot(tester, 'ws-rail-eight-${size.width.round()}x${size.height.round()}');
      c.activeFleet = 7;
      c.push(eight);
      await tester.pump(const Duration(milliseconds: 100));
      await tester.pump(const Duration(milliseconds: 100));
      expect(shown(8), isTrue, reason: 'selecting the eighth fleet brings its card into the rail');
      await shot(tester, 'ws-rail-eighth-selected-${size.width.round()}x${size.height.round()}');
      c.activeFleet = 0;
      c.push(eight);
      await tester.pump(const Duration(milliseconds: 100));
      await tester.pump(const Duration(milliseconds: 100));
      expect(shown(1), isTrue);
      expect(tester.takeException(), isNull);
      await finish(tester, c);
    }
  });

  testWidgets('a hold that starts or ends moves neither the form of the orders panel nor, with that panel shut, the buttons of the fleet status and the contacts, at 1440 and 390 wide', (tester) async {
    const orders = proto.StandingOrders(preset: proto.PolicyPreset.prospect, params: proto.PolicyParams(maxRange: 2, fuelBudgetPct: 35));
    const moved = PolicyAct(tick: 498, since: 498, preset: proto.PolicyPreset.prospect, action: 'move', reason: 'charting [3,0]');
    const refuelling = PolicyAct(
        tick: 500,
        since: 484,
        preset: proto.PolicyPreset.prospect,
        action: 'hold',
        reason: 'refuelling: a fuel budget of 35% of deuterium income earns nothing (tank 18 of 180); raise fuel_budget_pct or wait for the store to fill');
    const shields = PolicyAct(tick: 501, since: 501, preset: proto.PolicyPreset.prospect, action: 'hold', reason: 'shields recharging (76%)');
    final under = calm().copyWith(fleets: [fleet(1, [('Light Fighter', 1), ('Heavy Fighter', 2)], orders: orders), fleet(2, [('Small Cargo', 1)], at: home)]);
    const form = [
      ValueKey('orders-doctrine-manual'),
      ValueKey('orders-doctrine-patrolHome'),
      ValueKey('orders-range-down'),
      ValueKey('orders-fuel-up'),
      ValueKey('orders-set'),
    ];
    const footer = [ValueKey('orders-toggle'), ValueKey('manage-toggle')];

    for (final size in const [Size(1440, 900), Size(390, 844)]) {
      final c = await pumpFake(tester, under.copyWith(policyActs: {1: const [moved]}), size: size);
      if (size.width < 1440) {
        await tester.tap(find.byKey(const ValueKey('status-sheet')));
        await tester.pump(const Duration(milliseconds: 100));
      }
      List<Rect> places(List<ValueKey<String>> keys) => [for (final k in keys) tester.getRect(find.byKey(k))];
      Future<void> report(List<PolicyAct> acts) async {
        c.push(c.fake.copyWith(policyActs: {1: acts}));
        await tester.pump(const Duration(milliseconds: 100));
      }

      final status = [...places(footer), tester.getRect(find.byType(ContactsPanel))];
      expect(find.byKey(const Key('status-hold')), findsNothing);
      await report(const [refuelling, moved]);
      expect(find.descendant(of: find.byKey(const Key('status-hold')), matching: find.text(refuelling.reason)), findsOneWidget);
      expect([...places(footer), tester.getRect(find.byType(ContactsPanel))], status,
          reason: 'the fleet status has the room for a hold of four lines at ${size.width.round()} px: its buttons and the contacts stay');
      await report(const [moved]);
      expect([...places(footer), tester.getRect(find.byType(ContactsPanel))], status, reason: 'and when the hold ends');

      await tester.tap(find.byKey(const ValueKey('orders-toggle')));
      await tester.pump(const Duration(milliseconds: 100));
      final before = places(form);
      await report(const [refuelling, moved]);
      expect(tester.widget<Text>(find.byKey(const Key('orders-report-head'))).data, 'HOLDING FOR 0:16');
      expect(places(form), before, reason: 'a hold of four lines began at ${size.width.round()} px');
      await shot(tester, 'ws-orders-hold-in-place-${size.width.round()}');
      await report(const [shields, refuelling, moved]);
      expect(places(form), before, reason: 'another of one line took its place');
      await report(const [moved, shields, refuelling, moved]);
      expect(find.byKey(const Key('status-hold')), findsNothing);
      expect(places(form), before, reason: 'and the hold ended');
      expect(tester.takeException(), isNull);
      await finish(tester, c);
    }
  });

  testWidgets('a fleet under orders this console never gave shows their settings, and the steppers start from them (demo)', (tester) async {
    final c = DemoController()..startDemo();
    await pumpShell(tester, screen: Screen.windshield, controller: c);
    c.selectFleet(2);
    await tester.pump(const Duration(milliseconds: 100));
    expect((c.currentFleet.id, c.currentFleet.policy), (103, 'prospect'));
    expect(c.state.policyActs[103], isNull, reason: 'nothing about these orders was said in this session');

    await key(tester, LogicalKeyboardKey.keyP, 'p');
    await tester.pump(const Duration(milliseconds: 100));
    String shown(String id) => tester.widget<Text>(find.byKey(Key('orders-$id'))).data!;
    expect(find.descendant(of: find.byKey(const Key('orders-in-force')), matching: find.text('PROSPECT')), findsOneWidget);
    expect(shown('settings'), 'RANGE 6 HEXES - ENGAGE 2.0X - FUEL 35%');
    expect((shown('range'), shown('engage'), shown('fuel')), ('6 HEXES', '2.0X', '35%'));
    expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('orders-set'))).onPressed, isNull);
    expect(find.descendant(of: find.byKey(const Key('orders-why')), matching: find.text('These are the orders in force.')), findsOneWidget);
    await shot(tester, 'ws-orders-given-elsewhere-1440');
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  testWidgets('390, 1100 and 1440 wide: the orders panel opens with no overflow, from the status sheet where the layout has one, and a refusal shows with it without covering a phone\'s dock (demo)', (tester) async {
    for (final size in const [Size(390, 844), Size(1100, 800), Size(1440, 900)]) {
      final c = DemoController()..startDemo();
      c.place(101, const proto.Hex(4, -2), const proto.Resources());
      await pumpShell(tester, screen: Screen.windshield, controller: c, size: size);
      c.handleCommand('policy mine range=8 fuel=35');
      await tester.pump(const Duration(seconds: 1));
      if (size.width < 1440) {
        await tester.tap(find.byKey(const ValueKey('status-sheet')));
        await tester.pump(const Duration(milliseconds: 100));
        await tester.tap(find.byKey(const ValueKey('orders-toggle')));
      } else {
        expect(find.byKey(const ValueKey('status-sheet')), findsNothing);
        await key(tester, LogicalKeyboardKey.keyP, 'p');
      }
      await tester.pump(const Duration(milliseconds: 100));
      final panel = tester.getRect(find.byKey(const ValueKey('orders-panel')));
      expect(panel.left >= 0 && panel.right <= size.width, isTrue);
      if (size.height >= 900) {
        final set = tester.getRect(find.byKey(const ValueKey('orders-set')));
        const guideStrip = 66;
        expect(set.bottom + guideStrip <= tester.getRect(find.byType(Dock)).bottom, isTrue,
            reason: 'on a 900 px window the form shows down to its button without scrolling, with the guide strip above it too: $set');
      }
      expect(tester.widget<Text>(find.byKey(const Key('orders-report-reason'))).data, 'no seam within 8 hops of home repays the fuel to fetch it (stripped tiles refill over hours)');
      expect(tester.widget<Text>(find.byKey(const Key('orders-settings'))).data, 'RANGE 8 HEXES - ENGAGE 1.5X - FUEL 35%', reason: 'typed orders are known to the panel too');
      await shot(tester, 'ws-orders-${size.width.round()}');

      c.send(const proto.PolicyUpdate(fleetId: 999, preset: proto.PolicyPreset.prospect));
      await tester.pump(const Duration(milliseconds: 50));
      await tester.pump(const Duration(milliseconds: 500));
      final toast = find.descendant(
          of: find.byType(ToastLayer), matching: find.text('fleet 999 not found (or it has no ships left); its standing orders were not changed'));
      expect(toast, findsOneWidget, reason: 'the world\'s refusal, where the player is looking');
      final dock = tester.getRect(find.byKey(const ValueKey('dock-scan')));
      expect(tester.getRect(toast).bottom < dock.top, isTrue, reason: 'on the windshield a toast sits at the top at every width, clear of the dock');
      expect(tester.getRect(toast).left >= tester.getRect(find.byKey(const ValueKey('arena'))).left, isTrue, reason: 'inside the screen, not over the rail beside it');
      if (size.width < 1440) {
        final sheet = tester.getRect(find.byKey(const ValueKey('status-sheet')));
        expect(tester.getRect(toast).right < sheet.left, isTrue, reason: 'and to the left of the button that opens the sheet');
      } else {
        expect(tester.getRect(toast).right < panel.left, isTrue, reason: 'with the side columns up, it is left of the one that holds the panels and their buttons');
      }
      await shot(tester, 'ws-orders-refused-${size.width.round()}');
      if (size.width == 1100) {
        Console.of(tester.element(find.byType(Rail))).go(Screen.overview);
        await tester.pump(const Duration(milliseconds: 300));
        expect(tester.getRect(toast).top > size.height / 2 && tester.getRect(toast).right > size.width - 40, isTrue,
            reason: 'the Overview keeps its desktop layout at this width, and the same toast goes to its bottom right');
      }
      expect(tester.takeException(), isNull);
      await tester.pump(const Duration(seconds: 10));
      await endShell(tester, c);
    }
  });

  testWidgets('from the phone layout up to 1440 wide no gate card stands under a side column, the sector head or the feed', (tester) async {
    const all = [proto.Hex(2, -1), proto.Hex(3, -1), proto.Hex(3, 0), proto.Hex(2, 1), proto.Hex(1, 1), proto.Hex(1, 0)];
    final long = [
      for (var i = 0; i < 12; i++)
        LogEntry(
            tick: 400 + i,
            message: i.isEven
                ? 'Refused: Cruiser x1 costs 20000 metal, 7000 crystal and 2000 deuterium; the stockpile is short by 7800 metal and 100 deuterium. Nothing was ordered: a batch is paid in full when it is ordered.'
                : 'F1 scan at [2,0]: 18 sectors revealed, 4 hostiles, worst T2, 9 faint contacts (threat band 1/1/1/1/1/1/1/1/1)',
            level: EventLevel.normal),
    ];
    final state = world([
      fleet(1, [('Light Fighter', 1), ('Heavy Fighter', 2)]),
    ], {
      here: sector(here, lanes: all),
      for (final h in all) h: sector(h, lanes: [here]),
    }).copyWith(events: long.reversed.toList());
    final c = await pumpFake(tester, state);
    var desktop = 0, phone = 0;
    for (final height in const [700.0, 800.0, 900.0]) {
      for (final width in const [1000.0, 1100.0, 1160.0, 1170.0, 1200.0, 1280.0, 1366.0, 1440.0]) {
        tester.view.physicalSize = Size(width, height);
        await tester.pump(const Duration(milliseconds: 100));
        final at = '${width.round()}x${height.round()}';
        final cards = [for (final k in const ['q', 'w', 'e', 'a', 's', 'd']) tester.getRect(find.byKey(ValueKey('gate-$k')))];
        final arena = tester.getRect(find.byKey(const ValueKey('arena')));
        for (final card in cards) {
          expect(card.left >= arena.left && card.right <= arena.right, isTrue, reason: 'a gate card runs off the arena at $at');
        }
        final Map<String, Rect> blocks;
        if (find.byKey(const ValueKey('status-sheet')).evaluate().isNotEmpty) {
          phone++;
          expect(find.byType(StatusPanel), findsNothing, reason: 'the phone layout keeps the panels in its sheet at $at');
          expect(cards.every((card) => card.width == GateCard.width), isTrue, reason: 'an arena this wide has room for the full cards at $at');
          blocks = {
            'the Status button': tester.getRect(find.byKey(const ValueKey('status-sheet'))),
            'the dock': tester.getRect(find.byType(Dock)),
          };
        } else {
          desktop++;
          final feed = tester.getRect(find.byType(Feed));
          expect(feed.height, inInclusiveRange(14, Feed.mostLines * 18), reason: 'the feed keeps to its ${Feed.mostLines} rows at $at');
          expect(find.descendant(of: find.byType(Feed), matching: find.textContaining('00411', findRichText: true)), findsOneWidget,
              reason: 'and the newest line is the one it keeps at $at');
          expect(find.descendant(of: find.byType(Feed), matching: find.textContaining('00400', findRichText: true)), findsNothing,
              reason: 'the oldest of twelve long lines is left to the Overview\'s log at $at');
          blocks = {
            'the right column': tester.getRect(find.byType(StatusPanel)).expandToInclude(tester.getRect(find.byType(ContactsPanel))),
            'the feed': feed,
          };
        }
        blocks['the fleet rail'] = tester.getRect(find.byType(FleetRail));
        blocks['the sector head'] = tester.getRect(find.byType(SectorHead));
        for (final MapEntry(key: name, value: block) in blocks.entries) {
          for (final card in cards) {
            expect(card.overlaps(block), isFalse, reason: 'a gate card stands under $name at $at: $card and $block');
          }
        }
        await shot(tester, 'ws-fit-$at');
        expect(tester.takeException(), isNull, reason: at);
      }
    }
    expect(desktop > 0 && phone > 0, isTrue, reason: 'both layouts were looked at');
    await finish(tester, c);
  });
}
