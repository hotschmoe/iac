import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/models/game_state.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/console/kv_store.dart';
import 'package:iac_client/console/prefs.dart';
import 'package:iac_client/design/panel.dart';
import 'package:iac_client/state/command_parser.dart';
import 'package:iac_client/state/connection_provider.dart';
import 'package:iac_client/state/game_controller.dart';
import 'package:iac_client/state/token_store.dart';

import 'support/demo_controller.dart';
import 'support/fake_server.dart';
import 'support/harness.dart';

/// Demo-backed controller with controllable stock and economy availability.
class _Fake extends DemoController {
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
        shipCount: s.shipCount,
        research: s.research,
        events: s.events,
        alerts: s.alerts,
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
    expect(find.textContaining('Needs Computer Technology level 3 (you have 1)'), findsOneWidget);
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

  testWidgets('the queue cards follow the build slots the server reports, and the research and shipyard lines keep their own depth', (tester) async {
    final c = await _boot(tester);
    Finder inCard(String card, String text) => find.descendant(of: find.byKey(Key(card)), matching: find.textContaining(text));
    expect(inCard('card-slot-a', 'queue 3/4'), findsOneWidget);
    expect(inCard('card-research', 'queue 1/4'), findsOneWidget);

    c.learn(proto.ResearchType.computerTechnology, 3);
    await tester.pump(const Duration(seconds: 1));
    expect(find.text('BUILD SLOTS B-C'), findsOneWidget);
    expect(inCard('card-slot-b', 'SLOT C LOCKED'), findsOneWidget);
    expect(inCard('card-slot-b', 'Needs Computer Technology level 6 (you have 3)'), findsOneWidget);
    expect(inCard('card-slot-b', c.state.hw!.buildQueue[1].buildingType.label.toUpperCase()), findsOneWidget);

    c.learn(proto.ResearchType.computerTechnology, 6);
    await tester.pump(const Duration(seconds: 1));
    expect(c.state.hw!.buildSlots, 3);
    expect(find.text('BUILD SLOTS B-C'), findsOneWidget);
    expect(find.textContaining('LOCKED'), findsNothing);
    for (final build in c.state.hw!.buildQueue.skip(1)) {
      expect(inCard('card-slot-b', build.buildingType.label.toUpperCase()), findsOneWidget);
    }
    expect(inCard('card-slot-a', 'queue 3/6'), findsOneWidget);
    expect(inCard('card-research', 'queue 1/4'), findsOneWidget, reason: 'research runs one order whatever the build slots');
    expect(inCard('card-shipyard', 'queue 1/4'), findsOneWidget);
    await shot(tester, 'desktop-three-slots');
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

  testWidgets('a tech the stockpile does not cover queues from its tile, which says what it is short of; a ship batch stays off', (tester) async {
    final c = await _boot(tester, stock: const proto.Resources(metal: 1, crystal: 1, deuterium: 0));
    c.cancelHomeworldQueue(proto.QueueType.research);
    await tester.pump(const Duration(milliseconds: 1200));
    final tiles = find.byWidgetPredicate((w) => w.key is ValueKey<String> && (w.key as ValueKey<String>).value.startsWith('sug-research-'));
    await _scrollTo(tester, tiles.first);
    expect(find.descendant(of: tiles.first, matching: find.textContaining(RegExp(r'^Short by .*: it queues now and starts when that is covered\.$'))), findsOneWidget);
    await shot(tester, 'overview-short-tiles-1440');
    final more = find.descendant(of: find.byKey(const Key('card-shipyard')), matching: find.byType(ConsoleButton)).first;
    expect(tester.widget<ConsoleButton>(more).onPressed, isNull, reason: 'a ship batch is paid in full when ordered, so short of it there is nothing to order');
    expect(tester.widget<ConsoleButton>(more).reason, startsWith('Short by'));
    await tester.tap(tiles.first);
    await tester.pump(const Duration(milliseconds: 200));
    expect(c.state.events.where((e) => e.message.startsWith('> research ')).length, 1, reason: 'a building or a tech queues ahead of its resources');
    await endShell(tester, c);
  });

  testWidgets('near cap and full lamps carry text, not only colour', (tester) async {
    final c = await _boot(tester);
    final cap = c.state.hw!.storage.cap;
    c.stock(proto.Resources(metal: cap.metal, crystal: cap.crystal * .88, deuterium: 100));
    await tester.pump(const Duration(milliseconds: 100));
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
    expect(find.descendant(of: log, matching: find.text('UPLINK ESTABLISHED. HOMEWORLD AT [4,-2].')), findsOneWidget);
    await tester.tap(find.descendant(of: log, matching: find.text('COMBAT')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.descendant(of: log, matching: find.textContaining('UPLINK ESTABLISHED')), findsNothing);
    expect(find.descendant(of: log, matching: find.textContaining('DESTROYED')), findsOneWidget);
    await endShell(tester, c);
  });

  testWidgets('the log holds the whole answer to help and what came before it, and scrolls through them; help spells out orders, load and unload', (tester) async {
    final c = await _boot(tester);
    c.note('Before the question');
    c.handleCommand('help');
    await tester.pump(const Duration(milliseconds: 100));
    final lines = c.state.events.map((e) => e.message).toList();
    expect(lines.take(helpLines.length), helpLines, reason: 'the answer reads top down');
    expect(lines, containsAll(['> help', 'Before the question']), reason: 'and has not pushed out what came before it');
    expect(
        helpLines.join('\n'),
        allOf(contains('p[olicy] <manual|prospect|mine|salvage|patrol>'), contains('range=<hexes from home, 1-30>'), contains('load <metal|crystal|deut> [amount|all]'),
            contains('unload    put a hold loaded by hand back'), isNot(contains('TAB'))));
    final log = find.byKey(const Key('log-panel'));
    await _scrollTo(tester, log);
    expect(find.descendant(of: log, matching: find.text(helpLines.first.toUpperCase())), findsOneWidget);
    final scroll = tester.state<ScrollableState>(find.descendant(of: find.byKey(const Key('log-lines')), matching: find.byType(Scrollable)));
    expect(scroll.position.maxScrollExtent, greaterThan(0), reason: 'the panel shows a screenful and scrolls to the rest');
    await shot(tester, 'overview-log-help-1440');
    scroll.position.jumpTo(scroll.position.maxScrollExtent);
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.descendant(of: log, matching: find.text('BEFORE THE QUESTION')).hitTestable(), findsOneWidget);
    expect(tester.takeException(), isNull);
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

  testWidgets('built-in bots stand under the ranking, tagged BOT and without a rank', (tester) async {
    final server = FakeServer();
    await tester.runAsync(server.start);
    addTearDown(() => tester.runAsync(server.stop));
    server.accounts['Admiral'] = issuedToken;
    final c = GameController(tokens: MemoryTokenStore());
    await tester.runAsync(() => c.start(params: ConnectParams(url: server.url, token: issuedToken), name: 'Admiral'));
    Future<void> settle(bool Function() cond) async {
      await tester.runAsync(() async {
        for (var i = 0; i < 600 && !cond(); i++) {
          await Future<void>.delayed(const Duration(milliseconds: 5));
        }
      });
      expect(cond(), isTrue, reason: 'the server and client did not get there');
    }

    await settle(() => c.isLive);
    server.send(golden('server_message', 'leaderboard__with_bots'));
    await settle(() => c.state.leaderboard != null);
    await pumpShell(tester, controller: c, prefs: ConsolePrefs(MemoryKv()));

    final panel = find.byKey(const Key('rank-panel'));
    await _scrollTo(tester, panel);
    Finder inPanel(Finder f) => find.descendant(of: panel, matching: f);
    expect(inPanel(find.text('001')), findsOneWidget, reason: 'the one ranked player');
    expect(inPanel(find.text('HUM')), findsOneWidget);
    for (final bot in ['BOT-explorer-2', 'BOT-turtle-1']) {
      final row = find.byKey(Key('rank-bot-$bot'));
      expect(find.descendant(of: row, matching: find.text(bot)), findsOneWidget);
      expect(find.descendant(of: row, matching: find.text('BOT')), findsOneWidget);
      expect(find.descendant(of: row, matching: find.text('---')), findsOneWidget, reason: 'a bot takes no rank');
    }
    expect(tester.getTopLeft(find.byKey(const Key('rank-bot-BOT-explorer-2'))).dy, greaterThan(tester.getTopLeft(inPanel(find.text('Admiral'))).dy));
    await endShell(tester, c);
    c.dispose();
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
