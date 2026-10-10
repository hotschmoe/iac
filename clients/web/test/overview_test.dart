import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/models/game_state.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/state/game_controller.dart';

import 'support/harness.dart';

/// Demo-backed controller with controllable stock and economy availability.
class _Fake extends GameController {
  _Fake({this.noEco = false, this.stockOverride});
  final bool noEco;
  final proto.Resources? stockOverride;

  @override
  GameState get state {
    final s = super.state;
    if (noEco) {
      return GameState(
        tick: s.tick,
        clockSec: s.clockSec,
        resources: s.resources,
        fleets: s.fleets,
        buildQueue: s.buildQueue,
        shipyard: s.shipyard,
        docked: s.docked,
        research: s.research,
        events: s.events,
        alerts: s.alerts,
        sector: s.sector,
        homeworld: s.homeworld,
        waypoints: s.waypoints,
        sectors: s.sectors,
        stock: stockOverride ?? s.stock,
        catalog: s.catalog,
      );
    }
    return s.copyWith(
      stock: stockOverride,
      raid: proto.RaidIncomingEvent(playerId: 1, arrivalTick: s.tick + 270, threat: 'Moderate', estPower: 120),
    );
  }
}

Future<_Fake> _boot(WidgetTester tester, {Size size = const Size(1440, 900), bool noEco = false, proto.Resources? stock}) async {
  final c = _Fake(noEco: noEco, stockOverride: stock)..startDemo();
  c.sendCommand(const proto.LeaderboardCommand(limit: 10));
  await pumpShell(tester, size: size, controller: c);
  return c;
}

Future<void> _scrollTo(WidgetTester tester, Finder f) async {
  await tester.ensureVisible(f);
  await tester.pump(const Duration(milliseconds: 100));
}

void main() {
  testWidgets('desktop: queue cards, slot B reason, raid card, comms slot hidden', (tester) async {
    final c = await _boot(tester);
    expect(find.byKey(const Key('card-slot-a')), findsOneWidget);
    expect(find.text('BUILD SLOT B'), findsOneWidget);
    expect(find.textContaining('Needs Modular Fabrication'), findsOneWidget);
    expect(find.text('VIEW TECH'), findsOneWidget);
    expect(find.byKey(const Key('raid-card')), findsOneWidget);
    expect(find.text('COMMS'), findsNothing);
    expect(find.text('TEAM-UPS'), findsNothing);
    expect(find.byKey(const Key('slot-comms')), findsOneWidget);
    expect(find.byKey(const Key('rank-panel')), findsOneWidget);
    await shot(tester, 'desktop-top');
    await tester.drag(find.byKey(const Key('overview-scroll')), const Offset(0, -700));
    await tester.pump(const Duration(milliseconds: 200));
    await shot(tester, 'desktop-bottom');
    await endShell(tester, c);
  });

  testWidgets('open defence jumps to the homeworld defence tab', (tester) async {
    final c = await _boot(tester);
    final btn = find.text('OPEN DEFENCE');
    await _scrollTo(tester, btn);
    await tester.tap(btn);
    await tester.pump(const Duration(milliseconds: 100));
    expect(c.hwTab, HomeworldTab.defence);
    await endShell(tester, c);
  });

  testWidgets('idle research card queues a ResearchCommand', (tester) async {
    final c = await _boot(tester);
    c.cancelHomeworldQueue(proto.QueueType.research);
    await tester.pump(const Duration(milliseconds: 1200));
    expect(c.state.research.name, 'Idle');
    expect(find.text('NOTHING IN THE LAB'), findsOneWidget);
    final tiles = find.byWidgetPredicate((w) => w.key is ValueKey<String> && (w.key as ValueKey<String>).value.startsWith('sug-research-'));
    expect(tiles, findsWidgets);
    await _scrollTo(tester, tiles.first);
    await tester.tap(tiles.first);
    await tester.pump(const Duration(milliseconds: 1200));
    expect(c.state.events.any((e) => e.message.startsWith('> research ')), isTrue);
    expect(c.state.research.name, isNot('Idle'));
    await endShell(tester, c);
  });

  testWidgets('unaffordable suggestions are disabled and say why', (tester) async {
    final c = await _boot(tester, stock: const proto.Resources(metal: 1, crystal: 1, deuterium: 0));
    c.cancelHomeworldQueue(proto.QueueType.research);
    await tester.pump(const Duration(milliseconds: 1200));
    expect(find.textContaining('Short by'), findsWidgets);
    final before = c.state.events.length;
    final tiles = find.byWidgetPredicate((w) => w.key is ValueKey<String> && (w.key as ValueKey<String>).value.startsWith('sug-research-'));
    await _scrollTo(tester, tiles.first);
    await tester.tap(tiles.first, warnIfMissed: false);
    await tester.pump(const Duration(milliseconds: 200));
    expect(c.state.events.where((e) => e.message.startsWith('> research ')).length, 0);
    expect(before, isNonNegative);
    await endShell(tester, c);
  });

  testWidgets('near cap and full lamps carry text, not only colour', (tester) async {
    final c = await _boot(tester, stock: const proto.Resources(metal: 11200, crystal: 6930, deuterium: 100));
    final crystal = find.byKey(const Key('res-tile-crystal'));
    expect(find.descendant(of: crystal, matching: find.text('NEAR CAP 88%')), findsWidgets);
    final metal = find.byKey(const Key('res-tile-metal'));
    expect(find.descendant(of: metal, matching: find.text('FULL')), findsOneWidget);
    expect(find.descendant(of: metal, matching: find.text('STORAGE FULL. PRODUCTION IS WASTED.')), findsOneWidget);
    final deut = find.byKey(const Key('res-tile-deut'));
    expect(find.descendant(of: deut, matching: find.textContaining('NEAR CAP')), findsNothing);
    await endShell(tester, c);
  });

  testWidgets('event filter chips filter the log', (tester) async {
    final c = await _boot(tester);
    c.note('Hostile fleet 9 destroyed in [1,1]');
    await tester.pump(const Duration(milliseconds: 100));
    final log = find.byKey(const Key('log-panel'));
    await _scrollTo(tester, log);
    expect(find.descendant(of: log, matching: find.textContaining('FULL STATE SYNC')), findsOneWidget);
    await tester.tap(find.descendant(of: log, matching: find.text('COMBAT')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.descendant(of: log, matching: find.textContaining('FULL STATE SYNC')), findsNothing);
    expect(find.descendant(of: log, matching: find.textContaining('DESTROYED')), findsOneWidget);
    await endShell(tester, c);
  });

  testWidgets('fleet card: recall is disabled at home and says why; pilot selects and switches screen', (tester) async {
    final c = await _boot(tester);
    final home = c.state.fleets.firstWhere((f) => f.sector == c.state.homeworld);
    final card = find.byKey(Key('fleet-card-${home.id}'));
    await _scrollTo(tester, card);
    expect(find.descendant(of: card, matching: find.text('Already at the homeworld')), findsOneWidget);
    final other = c.state.fleets.indexWhere((f) => f.id != home.id);
    final otherCard = find.byKey(Key('fleet-card-${c.state.fleets[other].id}'));
    await tester.tap(find.descendant(of: otherCard, matching: find.text('PILOT')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(c.activeFleet, other);
    await endShell(tester, c);
  });

  testWidgets('without economy data: no raid card, no ranking, uncapped, slot B explains', (tester) async {
    final c = await _boot(tester, noEco: true);
    expect(find.byKey(const Key('raid-card')), findsNothing);
    expect(find.byKey(const Key('rank-panel')), findsNothing);
    expect(find.text('UNCAPPED'), findsNWidgets(3));
    expect(find.textContaining('Not available on this server yet'), findsOneWidget);
    expect(find.text('RANKING'), findsNothing);
    expect(find.text('COMMS'), findsNothing);
    await endShell(tester, c);
  });

  testWidgets('narrow 390x844: no overflow, no horizontal scroll', (tester) async {
    final c = await _boot(tester, size: const Size(390, 844));
    await shot(tester, 'narrow-0');
    final scroll = find.byKey(const Key('overview-scroll'));
    final w = tester.getSize(scroll).width;
    for (var i = 1; i <= 6; i++) {
      await tester.drag(scroll, const Offset(0, -700));
      await tester.pump(const Duration(milliseconds: 150));
      await shot(tester, 'narrow-$i');
    }
    expect(tester.takeException(), isNull);
    final sv = tester.widget<SingleChildScrollView>(scroll);
    expect(sv.scrollDirection, Axis.vertical);
    expect(w, 390);
    await endShell(tester, c);
  });

  testWidgets('mid width 1000 renders without overflow', (tester) async {
    final c = await _boot(tester, size: const Size(1000, 800));
    await tester.drag(find.byKey(const Key('overview-scroll')), const Offset(0, -900));
    await tester.pump(const Duration(milliseconds: 150));
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });
}
