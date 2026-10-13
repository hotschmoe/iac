import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/economy_view.dart';
import 'package:iac_client/console/notifier.dart';
import 'package:iac_client/console/services.dart';
import 'package:iac_client/console/prefs.dart';
import 'package:iac_client/console/sfx.dart';
import 'package:iac_client/console/toasts.dart';
import 'package:iac_client/design/panel.dart';
import 'package:iac_client/design/theme.dart';
import 'package:iac_client/design/tokens.dart';
import 'package:iac_client/views/homeworld/homeworld_view.dart';
import 'package:iac_client/models/game_state.dart';
import 'package:iac_client/models/homeworld.dart';
import 'package:iac_client/models/resources.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/state/game_controller.dart';
import 'package:iac_client/views/homeworld/economy_tabs.dart';

import 'support/demo_controller.dart';
import 'support/harness.dart';

final _rootKey = GlobalKey();

Future<void> snap(WidgetTester tester, String name) async {
  final dir = Platform.environment['IAC_SHOTS'];
  if (dir == null) return;
  await tester.runAsync(() async {
    final boundary = _rootKey.currentContext!.findRenderObject()! as RenderRepaintBoundary;
    final image = await boundary.toImage(pixelRatio: 1);
    final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
    final f = File('$dir/$name.png');
    await f.parent.create(recursive: true);
    await f.writeAsBytes(bytes!.buffer.asUint8List());
  });
}

void main() {
  // The view alone inside a Console scope (HUD, rail and the other screens
  // are covered by their own tests).
  Future<GameController> open(WidgetTester tester, {Size size = const Size(1440, 900)}) async {
    await tester.runAsync(loadFonts);
    tester.view.physicalSize = size;
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final c = DemoController()..startDemo();
    final con = Console(game: c, prefs: ConsolePrefs(), toasts: ToastController(), sfx: NoSfx(), notifier: NoNotifier())..screen = Screen.homeworld;
    bool onKey(KeyEvent e) => e is KeyDownEvent && con.dispatchKey(e);
    HardwareKeyboard.instance.addHandler(onKey);
    addTearDown(() => HardwareKeyboard.instance.removeHandler(onKey));
    await tester.pumpWidget(MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: consoleTheme(),
      home: RepaintBoundary(key: _rootKey, child: ConsoleScope(console: con, child: const Material(color: C.deep, child: HomeworldView()))),
    ));
    await tester.pump(const Duration(milliseconds: 400));
    return c;
  }

  Future<void> endShell(WidgetTester tester, GameController c) async {
    c.stop();
    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(seconds: 5));
  }

  Future<void> cancelAll(WidgetTester tester) async {
    for (final k in ['building', 'ship', 'research']) {
      final f = find.byKey(ValueKey('hw-cancel-$k'));
      if (f.evaluate().isNotEmpty) {
        await tester.tap(f);
        await tester.pump();
      }
    }
  }

  testWidgets('buildings: tile selects, queue button sends BuildCommand after the queue is cleared', (tester) async {
    final c = await open(tester);
    expect(find.text('METAL MINE'), findsWidgets);
    await tester.tap(find.byKey(const ValueKey('hw-card-0')));
    await tester.pump();
    expect(c.hwCursor, 0);
    // the demo has orders waiting; an order that fits is never blocked by cost
    expect(find.textContaining('Cannot afford'), findsNothing);
    await tester.tap(find.byKey(const ValueKey('hw-cancel-building')));
    await tester.pump();
    c.cancelHomeworldQueue(proto.QueueType.building, waiting: true);
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('hw-queue')));
    await tester.pump();
    expect(c.state.events.any((e) => e.message.contains('> build Metal Mine')), isTrue);
    expect(c.state.buildQueue, isNotEmpty);
    await snap(tester, 'homeworld/buildings-1440');
    await endShell(tester, c);
  });

  testWidgets('locked card shows its reason text', (tester) async {
    final c = await open(tester);
    await cancelAll(tester);
    final cat = c.state.catalog!;
    await tester.tap(find.byKey(const ValueKey('hw-tab-research')));
    await tester.pump();
    final i = cat.research.indexWhere((b) => b.requires.any((r) => !r.met));
    expect(i, greaterThanOrEqualTo(0));
    await tester.tap(find.byKey(ValueKey('hw-card-$i')));
    await tester.pump();
    expect(find.textContaining('Locked: needs'), findsWidgets);
    await endShell(tester, c);
  });

  testWidgets('shipyard: batch selection changes the BuildShipCommand count', (tester) async {
    final c = await open(tester);
    await tester.tap(find.byKey(const ValueKey('hw-tab-shipyard')));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('hw-batch-5')));
    await tester.pump();
    expect(c.shipBatch, 5);
    await tester.tap(find.byKey(const ValueKey('hw-cancel-ship')));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('hw-card-0')));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('hw-queue')));
    await tester.pump();
    expect(c.state.events.any((e) => e.message.contains('> ship Scout x5')), isTrue);
    expect(c.state.shipyard.single.name, contains('Scout x5'));
    await snap(tester, 'homeworld/shipyard-1440');
    await endShell(tester, c);
  });

  testWidgets('cancel sends CancelBuildCommand and keyboard model works', (tester) async {
    final c = await open(tester);
    await tester.tap(find.byKey(const ValueKey('hw-cancel-ship')));
    await tester.pump();
    expect(c.state.events.any((e) => e.message.contains('> cancel ship')), isTrue);
    expect(c.state.shipyard, isEmpty);

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    expect(c.hwCursor, 2);
    await tester.sendKeyEvent(LogicalKeyboardKey.bracketRight);
    expect(c.hwTab, HomeworldTab.research);
    await tester.sendKeyEvent(LogicalKeyboardKey.digit5);
    expect(c.hwTab, HomeworldTab.storage);
    await tester.sendKeyEvent(LogicalKeyboardKey.digit3);
    await tester.sendKeyEvent(LogicalKeyboardKey.equal);
    expect(c.shipBatch, 5);
    await tester.pump();
    await endShell(tester, c);
  });

  testWidgets('research tab renders the tech graph, with a node for every tech of the catalog', (tester) async {
    final c = await open(tester);
    await tester.tap(find.byKey(const ValueKey('hw-tab-research')));
    await tester.pump();
    expect(find.text('FUEL EFFICIENCY'), findsWidgets);
    final techs = c.state.catalog!.research;
    expect([for (final o in techs) o.tech], proto.ResearchType.values);
    for (var i = 0; i < techs.length; i++) {
      expect(find.byKey(ValueKey('hw-card-$i')), findsOneWidget, reason: '${techs[i].tech.label} has a node');
    }
    final fab = techs.indexWhere((o) => o.tech == proto.ResearchType.modularFabrication);
    await tester.tap(find.byKey(ValueKey('hw-card-$fab')));
    await tester.pump();
    expect(c.hwCursor, fab);
    await tester.tap(find.byKey(const ValueKey('hw-card-1')));
    await tester.pump();
    expect(c.hwCursor, 1);
    await snap(tester, 'homeworld/research-1440');
    await endShell(tester, c);
  });

  testWidgets('the queue strip has a pill for each build slot the server reports, and for the next one while a level of Modular Fabrication is left', (tester) async {
    final c = await open(tester) as DemoController;
    Finder inPill(String letter, String text) => find.descendant(of: find.byKey(Key('hw-pill-slot-$letter')), matching: find.textContaining(text));
    expect(inPill('b', 'LOCKED'), findsOneWidget);
    expect(inPill('b', 'Needs Modular Fabrication L1'), findsOneWidget);
    expect(find.byKey(const Key('hw-pill-slot-c')), findsNothing);

    c.learn(proto.ResearchType.modularFabrication, 1);
    await tester.pump(const Duration(seconds: 1));
    expect(c.state.hw!.buildSlots, 2);
    expect(inPill('b', 'LOCKED'), findsNothing);
    expect(inPill('c', 'LOCKED'), findsOneWidget);
    expect(inPill('c', 'Needs Modular Fabrication L2'), findsOneWidget);

    c.learn(proto.ResearchType.modularFabrication, 2);
    await tester.pump(const Duration(seconds: 1));
    expect(c.state.hw!.buildSlots, 3);
    expect(c.state.hw!.buildQueue, hasLength(3), reason: 'the two waiting orders took the new slots');
    for (final (letter, build) in [('b', c.state.hw!.buildQueue[1]), ('c', c.state.hw!.buildQueue[2])]) {
      expect(inPill(letter, build.buildingType.label.toUpperCase()), findsOneWidget);
    }
    expect(find.text('LOCKED'), findsNothing);
    expect(find.byKey(const Key('hw-pill-slot-d')), findsNothing);
    await snap(tester, 'homeworld/three-slots-1440');

    await tester.tap(find.byKey(const ValueKey('hw-cancel-building-c')));
    await tester.pump();
    expect(c.state.hw!.buildQueue, hasLength(2));
    await endShell(tester, c);
  });

  testWidgets('defence and storage render from the demo', (tester) async {
    final c = await open(tester);
    await tester.tap(find.byKey(const ValueKey('hw-tab-defence')));
    await tester.pump();
    expect(find.textContaining('RAID'), findsWidgets);
    expect(find.text('PULSE TURRET'), findsWidgets);
    expect(find.textContaining('BUILD X'), findsWidgets);
    await snap(tester, 'homeworld/defence-1440');
    await tester.tap(find.byKey(const ValueKey('hw-tab-storage')));
    await tester.pump();
    expect(find.text('STORAGE VAULT'), findsOneWidget);
    await snap(tester, 'homeworld/storage-1440');
    await endShell(tester, c);
  });

  String offer(WidgetTester tester) => tester.widget<ConsoleButton>(find.byKey(const ValueKey('hold-load'))).label;
  String cargo(WidgetTester tester) => tester.widget<Text>(find.byKey(const Key('hold-cargo'))).textSpan!.toPlainText();

  Future<void> press(WidgetTester tester, String key) async {
    await tester.ensureVisible(find.byKey(ValueKey(key)));
    await tester.tap(find.byKey(ValueKey(key)));
    await tester.pump(const Duration(milliseconds: 50));
  }

  testWidgets('storage: the hold panel shows what is aboard and what fits, loads a share and all that fits, and unloads', (tester) async {
    final c = await open(tester);
    await tester.tap(find.byKey(const ValueKey('hw-tab-storage')));
    await tester.pump();
    expect(find.byKey(const ValueKey('hold-fleet-102')), findsOneWidget, reason: 'the fleet docked at home');
    expect(find.byKey(const ValueKey('hold-fleet-101')), findsNothing, reason: 'a fleet away cannot load');
    expect(tester.widget<Text>(find.byKey(const Key('hold-aboard'))).data, '0 / 400');
    expect(offer(tester), 'Load 400 Fe', reason: 'all that fits: the hold is smaller than the stockpile');
    expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('hold-unload'))).onPressed, isNull);

    await press(tester, 'hold-share-50');
    expect(offer(tester), 'Load 200 Fe');
    final metal = c.state.stock.metal;
    await press(tester, 'hold-load');
    expect(c.state.events.map((e) => e.message), contains('> load 200 metal into F102'));
    expect(c.state.events.map((e) => e.message), contains(startsWith('fleet 102 loaded 200 metal from the stockpile: it carries 200 of 400')));
    expect((c.state.holds[102]!.metal, c.state.stock.metal), (200, metal - 200));
    expect(c.state.fleets.firstWhere((f) => f.id == 102).laden, isTrue);
    expect(tester.widget<Text>(find.byKey(const Key('hold-aboard'))).data, '200 / 400');
    expect(cargo(tester), 'Fe 200   Cr 0   De 0   KEPT ABOARD UNTIL THE FLEET HAS BEEN AWAY');
    expect(find.text('LADEN'), findsOneWidget);

    await press(tester, 'hold-res-crystal');
    await press(tester, 'hold-share-100');
    expect(offer(tester), 'Load 200 Cr', reason: 'what is left of the hold');
    await press(tester, 'hold-load');
    expect(c.state.holds[102]!.crystal, 200);
    expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('hold-load'))).onPressed, isNull);
    expect(offer(tester), 'Load');
    expect(tester.widget<Text>(find.descendant(of: find.byKey(const Key('hold-why')), matching: find.byType(Text))).data, 'The hold of F102 is full.');
    await tester.pump(const Duration(milliseconds: 600));
    await snap(tester, 'homeworld/hold-1440');

    await press(tester, 'hold-unload');
    expect(c.state.events.map((e) => e.message), contains('fleet 102 unloaded 200 metal and 200 crystal into the stockpile'));
    expect(c.state.holds[102]!.total, 0);
    expect(c.state.stock.metal, closeTo(metal, 10), reason: 'what was loaded is back, and a tick or two of the mines');
    expect(find.text('LADEN'), findsNothing);
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  testWidgets('load and unload work from the command bar, and a refusal is the server\'s sentence', (tester) async {
    final c = await open(tester);
    c.handleCommand('load');
    expect(c.state.events.first.message, startsWith('load: <metal|crystal|deut> [amount|all]'));
    c.handleCommand('load metal 50');
    expect(c.state.events.first.message, contains('fleet 101 is at [6,-4], 2 hexes from home [4,-2]'));
    expect(c.state.holds[101]!.metal, 34, reason: 'a refused load moves nothing');

    c.selectFleet(1);
    c.handleCommand('load deut 5000');
    expect(c.state.events.first.message, endsWith('the stockpile holds 880.0 deuterium; fleet 102 cannot load 5000.0'));
    c.handleCommand('load crystal 120.5');
    c.handleCommand('load deut');
    final hold = c.state.holds[102]!;
    expect((hold.metal, hold.crystal, hold.deuterium), (0, 120.5, 279.5), reason: 'an amount, then all that fits');
    c.handleCommand('unload');
    expect(c.state.holds[102]!.total, 0);
    c.handleCommand('unload');
    expect(c.state.events.first.message, contains('fleet 102 was not loaded by hand'));
    await tester.pump();
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  testWidgets('390 wide: the hold panel fits with a laden hold and a reason on show', (tester) async {
    final c = await open(tester, size: const Size(390, 844));
    c.selectHomeworldTab(HomeworldTab.storage);
    c.selectFleet(1);
    c.handleCommand('load metal');
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('LADEN'), findsOneWidget);
    expect(find.byKey(const Key('hold-why')), findsOneWidget);
    await tester.ensureVisible(find.byKey(const ValueKey('hold-load')));
    await tester.pump(const Duration(milliseconds: 600));
    await snap(tester, 'homeworld/hold-390');
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  testWidgets('defence and storage show the not-reported state without server data', (tester) async {
    final g = GameController();
    addTearDown(g.dispose);
    await tester.pumpWidget(MaterialApp(
      theme: consoleTheme(),
      home: Material(
        child: Column(children: [
          Expanded(child: DefenceTab(eco: EconomyView.empty, ctrl: g, onSelect: (_) {})),
          Expanded(child: StorageTab(eco: EconomyView.empty, state: _emptyState(), ctrl: g)),
        ]),
      ),
    ));
    expect(find.text('NOT REPORTED BY SERVER YET'), findsNWidgets(2));
  });

  testWidgets('no overflow at 390x844 on every tab', (tester) async {
    final c = await open(tester, size: const Size(390, 844));
    for (final t in HomeworldTab.values) {
      c.selectHomeworldTab(t);
      await tester.pump(const Duration(milliseconds: 100));
      expect(tester.takeException(), isNull, reason: t.name);
      await snap(tester, 'homeworld/${t.name}-390');
    }
    await endShell(tester, c);
  });
}

GameState _emptyState() => const GameState(
      tick: 0,
      clockSec: 0,
      resources: Resources(metal: ResourceStock(amount: 0, rate: 0), crystal: ResourceStock(amount: 0, rate: 0), deut: ResourceStock(amount: 0, rate: 0)),
      fleets: [],
      buildQueue: [],
      shipyard: [],
      docked: '',
      research: ResearchState(name: 'Idle', time: '', pct: 0, completed: []),
      events: [],
      alerts: [],
      sector: SectorInfo.unknown,
      homeworld: proto.Hex.origin,
      waypoints: [],
    );
