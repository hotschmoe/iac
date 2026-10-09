import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/economy_view.dart';
import 'package:iac_client/console/services.dart';
import 'package:iac_client/console/prefs.dart';
import 'package:iac_client/console/sfx.dart';
import 'package:iac_client/console/toasts.dart';
import 'package:iac_client/design/theme.dart';
import 'package:iac_client/design/tokens.dart';
import 'package:iac_client/views/homeworld/homeworld_view.dart';
import 'package:iac_client/models/game_state.dart';
import 'package:iac_client/models/homeworld.dart';
import 'package:iac_client/models/resources.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/state/game_controller.dart';
import 'package:iac_client/views/homeworld/economy_tabs.dart';

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
    final c = GameController()..startDemo();
    final con = Console(game: c, prefs: ConsolePrefs(), toasts: ToastController(), sfx: NoSfx())..screen = Screen.homeworld;
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
    // the demo queue is full (3 of 3): queueing is blocked with a reason
    expect(find.textContaining('Queue full'), findsWidgets);
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

  testWidgets('research tab renders the tech graph', (tester) async {
    final c = await open(tester);
    await tester.tap(find.byKey(const ValueKey('hw-tab-research')));
    await tester.pump();
    expect(find.text('FUEL EFFICIENCY'), findsWidgets);
    await tester.tap(find.byKey(const ValueKey('hw-card-1')));
    await tester.pump();
    expect(c.hwCursor, 1);
    await snap(tester, 'homeworld/research-1440');
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
