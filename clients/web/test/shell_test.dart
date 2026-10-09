import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/services.dart';
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
