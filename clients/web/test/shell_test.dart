import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/services.dart';
import 'package:iac_client/console/toasts.dart';
import 'package:iac_client/design/gauges.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/views/hud.dart';
import 'package:iac_client/views/overlays.dart';
import 'package:iac_client/views/rail.dart';

import 'support/harness.dart';

Screen _screen(WidgetTester t) => Console.of(t.element(find.byType(Rail))).screen;

Future<void> _type(WidgetTester t, String ch, LogicalKeyboardKey k) async {
  await t.sendKeyEvent(k, character: ch);
  await t.pump(const Duration(milliseconds: 50));
}

void main() {
  testWidgets('G chord navigates and ? toggles help', (tester) async {
    final c = await pumpShell(tester);
    expect(_screen(tester), Screen.overview);
    await _type(tester, 'g', LogicalKeyboardKey.keyG);
    await _type(tester, 'h', LogicalKeyboardKey.keyH);
    expect(_screen(tester), Screen.homeworld);
    await _type(tester, 'g', LogicalKeyboardKey.keyG);
    await _type(tester, 'w', LogicalKeyboardKey.keyW);
    expect(_screen(tester), Screen.windshield);

    await _type(tester, '?', LogicalKeyboardKey.slash);
    expect(find.text('KEY REFERENCE'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump(const Duration(milliseconds: 50));
    expect(find.text('KEY REFERENCE'), findsNothing);
    await endShell(tester, c);
  });

  testWidgets('/ palette takes focus, and hotkeys work again after submit and after Esc', (tester) async {
    final c = await pumpShell(tester);
    await _type(tester, '/', LogicalKeyboardKey.slash);
    expect(find.byKey(const Key('palette-field')), findsOneWidget);

    // Typed letters go to the field, not to the hotkeys.
    await tester.enterText(find.byKey(const Key('palette-field')), 'ws');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.byKey(const Key('palette-field')), findsNothing);
    expect(_screen(tester), Screen.windshield);

    await _type(tester, 'g', LogicalKeyboardKey.keyG);
    await _type(tester, 'm', LogicalKeyboardKey.keyM);
    expect(_screen(tester), Screen.map);

    await _type(tester, '/', LogicalKeyboardKey.slash);
    expect(find.byKey(const Key('palette-field')), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.byKey(const Key('palette-field')), findsNothing);

    await _type(tester, '/', LogicalKeyboardKey.slash);
    FocusManager.instance.primaryFocus!.unfocus();
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.byKey(const Key('palette-field')), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.byKey(const Key('palette-field')), findsNothing, reason: 'Esc closes a palette whose field has lost the keyboard');

    await _type(tester, 'g', LogicalKeyboardKey.keyG);
    await _type(tester, 'o', LogicalKeyboardKey.keyO);
    expect(_screen(tester), Screen.overview);
    await endShell(tester, c);
  });

  testWidgets('palette runs a game command and reports it as a toast', (tester) async {
    final c = await pumpShell(tester);
    await _type(tester, '/', LogicalKeyboardKey.slash);
    await tester.enterText(find.byKey(const Key('palette-field')), 'help');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('COMMAND'), findsOneWidget);
    expect(c.state.events.any((e) => e.message == '> help'), isTrue);
    await endShell(tester, c);
  });

  testWidgets('a toast slides in once: it stays whole when an earlier one leaves, a question outlasts plain ones, a tap puts a plain one away', (tester) async {
    final c = await pumpShell(tester);
    final toasts = Console.of(tester.element(find.byType(Rail))).toasts;
    double width(String message) => tester.getSize(find.ancestor(of: find.text(message), matching: find.byType(ClipRect)).first).width;

    toasts.show('Question', 'asked', life: const Duration(seconds: 30), actions: [ToastAction('Answer', () {})]);
    toasts.show('Plain', 'first');
    await tester.pump();
    await tester.pump(const Duration(seconds: 1));
    toasts.show('Plain', 'second');
    await tester.pump();
    await tester.pump(const Duration(seconds: 1));
    final whole = width('second');
    expect(whole, greaterThan(60));
    await tester.pump(const Duration(milliseconds: 2250));
    await tester.pump(const Duration(milliseconds: 50));
    expect(find.text('first'), findsNothing, reason: 'its 4.2 seconds are over');
    expect(width('second'), whole, reason: 'the toast under the one that left does not slide in again');

    for (final message in ['third', 'fourth', 'fifth', 'sixth']) {
      toasts.show('Plain', message);
    }
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 500));
    expect(find.text('asked'), findsOneWidget, reason: 'the oldest toast makes room, but a question is not the one to go');
    expect(find.text('second'), findsNothing);
    expect(find.text('sixth'), findsOneWidget);

    await tester.tap(find.text('sixth'));
    await tester.pump(const Duration(milliseconds: 50));
    expect(find.text('sixth'), findsNothing);
    expect(find.text('fifth'), findsOneWidget);
    await tester.tap(find.text('ANSWER'));
    await tester.pump(const Duration(milliseconds: 50));
    expect(find.text('asked'), findsNothing);
    await endShell(tester, c);
  });

  testWidgets('a refused order is shown at once in the world\'s own sentence, wherever the player is', (tester) async {
    final c = await pumpShell(tester, screen: Screen.homeworld);
    c.receive(const proto.ErrorMessage(code: proto.ErrorCode.insufficientResources, message: 'Scout x1 costs 200 metal; the stockpile is short by 60 metal. Nothing was ordered.'));
    await tester.pump(const Duration(milliseconds: 500));
    final toasts = find.byType(ToastLayer);
    expect(find.descendant(of: toasts, matching: find.text('REFUSED')), findsOneWidget);
    expect(find.descendant(of: toasts, matching: find.text('Scout x1 costs 200 metal; the stockpile is short by 60 metal. Nothing was ordered.')), findsOneWidget);
    expect(c.state.events.any((e) => e.message.startsWith('Refused: Scout x1 costs')), isTrue);
    await tester.pump(const Duration(seconds: 6));
    expect(find.descendant(of: toasts, matching: find.text('REFUSED')), findsOneWidget, reason: 'a sentence that long stays up long enough to read');
    await endShell(tester, c);
  });

  testWidgets('between a phone and a wide desktop the HUD gives the stockpiles the room, at full size', (tester) async {
    final c = await pumpShell(tester, size: const Size(1000, 760));
    expect(tester.takeException(), isNull);
    final stocks = find.descendant(of: find.byType(Hud), matching: find.byType(FittedBox));
    expect(stocks, findsNWidgets(3));
    for (final stock in stocks.evaluate()) {
      final digits = find.descendant(of: find.byElementPredicate((e) => e == stock), matching: find.byType(Register));
      expect(stock.size!.width, greaterThanOrEqualTo(tester.getSize(digits).width), reason: 'a stock is drawn at its own size, not shrunk to fit');
    }
    expect(find.descendant(of: find.byType(Hud), matching: find.text('IN AMBER CLAD')), findsNothing, reason: 'the name beside the logo gives way first');
    await endShell(tester, c);
  });

  testWidgets('narrow layout uses the bottom bar without overflow', (tester) async {
    final c = await pumpShell(tester, size: const Size(390, 844));
    expect(tester.takeException(), isNull);
    final rail = tester.getRect(find.byType(Rail));
    expect(rail.bottom, closeTo(844, 1));
    expect(rail.width, closeTo(390, 1));
    for (final s in Screen.values) {
      Console.of(tester.element(find.byType(Rail))).go(s);
      await tester.pump(const Duration(milliseconds: 200));
      expect(tester.takeException(), isNull, reason: s.name);
    }
    await endShell(tester, c);
  });

  testWidgets('sound is opt-in and the toggle persists in prefs', (tester) async {
    final c = await pumpShell(tester);
    final con = Console.of(tester.element(find.byType(Rail)));
    expect(con.prefs.sound, isFalse);
    await _type(tester, 'm', LogicalKeyboardKey.keyM);
    expect(con.prefs.sound, isTrue);
    await endShell(tester, c);
  });
}
