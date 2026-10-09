// Pumps the Homeworld view against the demo provider: it must render the
// catalog and send the right command when a card is tapped or Enter is hit.

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:iac_client/state/game_controller.dart';
import 'package:iac_client/theme/amber_theme.dart';
import 'package:iac_client/views/shell.dart';

void main() {
  setUpAll(() => GoogleFonts.config.allowRuntimeFetching = false);

  Future<GameController> openHomeworld(WidgetTester tester) async {
    tester.view.physicalSize = const Size(1400, 1000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final c = GameController();
    addTearDown(() {
      c.stop();
      c.dispose();
    });
    c.startDemo();
    await tester.pumpWidget(MaterialApp(
      theme: Amber.themeData(),
      home: Scaffold(body: Shell(controller: c)),
    ));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.enterText(find.byType(TextField), 'hw');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump(const Duration(milliseconds: 300));
    return c;
  }

  // Stops the demo timer inside the test body: the framework checks for
  // pending timers before tearDown callbacks run.
  void homeworldTest(String name, Future<void> Function(WidgetTester, GameController) body) {
    testWidgets(name, (tester) async {
      final c = await openHomeworld(tester);
      await body(tester, c);
      c.stop();
    });
  }

  homeworldTest('renders every building with its level, cost and locked prerequisites', (tester, c) async {
    expect(find.text('METAL MINE'), findsOneWidget);
    expect(find.text('DEFENSE GRID'), findsOneWidget);
    expect(find.byKey(const ValueKey('hw-card-7')), findsOneWidget);
    // Metal mine is Lv 4 in the demo; defense grid needs Shipyard >= 3.
    expect(find.text('Lv 4'), findsOneWidget);
    expect(find.text('[--] Shipyard >= 3'), findsOneWidget);
    expect(find.textContaining('ticks'), findsWidgets);
    expect(tester.takeException(), isNull);
  });

  homeworldTest('shows the storage cap beside each stockpile', (tester, c) async {
    expect(c.state.hw!.storage.cap.metal, greaterThan(5000));
    expect(find.textContaining(' / ', findRichText: true), findsWidgets);
    expect(find.text('Storage Vault'.toUpperCase()), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  homeworldTest('tapping a ship card queues the selected batch', (tester, c) async {
    await tester.tap(find.byKey(const ValueKey('hw-tab-shipyard')));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('hw-batch-5')));
    await tester.pump();
    expect(c.shipBatch, 5);

    // The demo has a corvette batch running; cancel it first, then queue.
    await tester.tap(find.byKey(const ValueKey('hw-cancel-ship')));
    await tester.pump();
    expect(c.state.shipyard, isEmpty);

    await tester.tap(find.byKey(const ValueKey('hw-card-0')));
    await tester.pump();
    expect(c.state.shipyard.single.name, contains('Scout x5'));
    expect(c.state.events.any((e) => e.message.contains('> ship Scout x5')), isTrue);
  });

  homeworldTest('a locked card is answered locally and a server error reaches the log', (tester, c) async {
    await tester.tap(find.byKey(const ValueKey('hw-card-7'))); // Defense Grid
    await tester.pump();
    expect(c.state.events.first.message, contains('locked: needs Shipyard >= 3'));

    // The demo already has a building in progress: the server's refusal shows up.
    await tester.tap(find.byKey(const ValueKey('hw-card-0')));
    await tester.pump();
    expect(c.state.events.first.message, contains('Build queue busy'));
  });

  homeworldTest('keyboard: arrows select, brackets switch tab, Enter queues research', (tester, c) async {
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    expect(c.hwCursor, 2);
    await tester.sendKeyEvent(LogicalKeyboardKey.bracketRight);
    await tester.sendKeyEvent(LogicalKeyboardKey.bracketRight);
    expect(c.hwTab, HomeworldTab.research);
    expect(c.hwCursor, 0);
    // Demo research is busy with Fuel Efficiency: Enter reports the server's answer.
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    expect(c.state.events.first.message, contains('Lab busy'));
    await tester.sendKeyEvent(LogicalKeyboardKey.delete);
    await tester.pump();
    expect(c.state.research.name, 'Idle');
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    expect(c.state.research.name, isNot('Idle'));
  });
}
