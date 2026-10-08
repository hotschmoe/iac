// Pumps the real Shell against the demo provider and walks every view, to
// catch layout/index errors that unit tests cannot.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:iac_client/state/connection_provider.dart';
import 'package:iac_client/state/game_controller.dart';
import 'package:iac_client/theme/amber_theme.dart';
import 'package:iac_client/views/shell.dart';

void main() {
  setUpAll(() => GoogleFonts.config.allowRuntimeFetching = false);

  testWidgets('shell renders all views in demo mode and takes commands', (tester) async {
    tester.view.physicalSize = const Size(1400, 900);
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
    expect(c.isDemo, isTrue);
    expect(c.state.fleets.length, 3);

    for (final cmd in ['2', '3', '1']) {
      await tester.enterText(find.byType(TextField), cmd);
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await tester.pump(const Duration(milliseconds: 200));
    }

    // Commands through the real command bar path.
    c.handleCommand('scan');
    c.handleCommand('f 2');
    c.handleCommand('harvest');
    c.handleCommand('move 99 99');
    c.handleCommand('build lab');
    c.handleCommand('help');
    await tester.pump(const Duration(seconds: 2)); // a couple of demo ticks
    expect(c.state.events.any((e) => e.message.contains('scan')), isTrue);

    // Walk views via the controller-driven widgets.
    for (final v in [GameView.windshield, GameView.starMap, GameView.commandCenter]) {
      await tester.pumpWidget(MaterialApp(
        theme: Amber.themeData(),
        home: Scaffold(body: Shell(controller: c)),
      ));
      await tester.enterText(find.byType(TextField), v == GameView.windshield ? 'ws' : v == GameView.starMap ? 'map' : 'cc');
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await tester.pump(const Duration(milliseconds: 300));
      c.moveFleet(0);
      c.moveToCursor();
      c.cycleFleet();
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(tester.takeException(), isNull);
    c.stop(); // cancel the demo timer before the framework checks for leaks
  });

  test('connecting to a dead port falls back to demo without throwing', () async {
    final c = GameController();
    addTearDown(() {
      c.stop();
      c.dispose();
    });
    await c.start(
      params: const ConnectParams(url: 'ws://127.0.0.1:1/ws'),
      name: 'x',
    );
    expect(c.isDemo, isTrue);
    expect(c.connectionError, contains('unavailable'));
  });
}
