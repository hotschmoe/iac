import 'dart:io';
import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/notifier.dart';
import 'package:iac_client/console/prefs.dart';
import 'package:iac_client/console/services.dart';
import 'package:iac_client/design/theme.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/state/game_controller.dart';
import 'package:iac_client/views/shell.dart';

import 'demo_controller.dart';

bool _fontsLoaded = false;

/// Registers the bundled Plex files so tests and screenshots measure real text.
Future<void> loadFonts() async {
  if (_fontsLoaded) return;
  _fontsLoaded = true;
  Future<void> family(String name, List<String> files) async {
    final l = FontLoader(name);
    for (final f in files) {
      final bytes = await File('assets/fonts/$f.ttf').readAsBytes();
      l.addFont(Future.value(ByteData.sublistView(bytes)));
    }
    await l.load();
  }

  await family('IBMPlexMono', ['IBMPlexMono-Regular', 'IBMPlexMono-Medium', 'IBMPlexMono-SemiBold']);
  await family('IBMPlexSansCondensed',
      ['IBMPlexSansCondensed-Regular', 'IBMPlexSansCondensed-Medium', 'IBMPlexSansCondensed-SemiBold', 'IBMPlexSansCondensed-Bold']);
}

/// Pumps the whole Shell on the demo world at [size]. Fonts come from the
/// bundled assets (declared in pubspec), so goldens show Plex.
Future<GameController> pumpShell(
  WidgetTester tester, {
  Size size = const Size(1440, 900),
  Screen screen = Screen.overview,
  GameController? controller,
  ConsolePrefs? prefs,
  Notifier? notifier,
  Duration settle = const Duration(milliseconds: 600),
}) async {
  await tester.runAsync(loadFonts);
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  final GameController c;
  if (controller == null) {
    c = DemoController()..startDemo();
    addTearDown(() {
      c.stop();
      c.dispose();
    });
  } else {
    c = controller;
  }
  await tester.pumpWidget(MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: consoleTheme(),
    home: Shell(controller: c, prefs: prefs ?? ConsolePrefs(), notifier: notifier, initial: screen),
  ));
  await tester.pump(settle);
  return c;
}

/// [count] ships of [cls] as a fleet's or a group's row lists them, on the
/// class's base figures: [damaged] of them not whole, [atRisk] of those at
/// risk, and [hullLost] hull gone between them.
proto.ShipGroupState shipRow(proto.ShipClass cls, int count, {int damaged = 0, int atRisk = 0, int hullLost = 0}) => proto.ShipGroupState(
      shipClass: cls,
      count: count,
      damaged: damaged,
      atRisk: atRisk,
      hull: count * cls.hull.round() - hullLost,
      hullMax: count * cls.hull.round(),
      unitAttack: cls.weapon.round(),
      unitShield: cls.shield.round(),
      unitHull: cls.hull.round(),
    );

/// A hostile group of [rows], counted as the server counts it: whole unless
/// [muster] says how many ships it has when it is, with the tick it is
/// whole again at, the hexes it follows a fleet, the ticks of its jump and
/// the errand it is on, as a test sets them.
proto.NpcFleetInfo hostileGroup(int id, List<proto.ShipGroupState> rows, proto.NpcBehavior behavior,
    {int? muster, int? wholeAt, int chaseHexes = 0, int jumpTicks = 1, proto.GroupErrand? errand}) {
  final count = rows.fold(0, (n, g) => n + g.count);
  return proto.NpcFleetInfo(
      id: id,
      ships: rows,
      shipCount: count,
      behavior: behavior,
      muster: muster ?? count,
      wholeAt: wholeAt,
      jumpTicks: jumpTicks,
      chaseHexes: chaseHexes,
      errand: errand);
}

/// Everything printed under [box], a line a piece of text.
String textUnder(WidgetTester tester, Finder box) => [
      for (final e in find.descendant(of: box, matching: find.byType(RichText)).evaluate()) (e.widget as RichText).text.toPlainText(),
    ].join('\n');

/// Every piece of text under [box] is drawn whole and inside it.
void expectTextFits(WidgetTester tester, Finder box, {required String reason}) {
  expect(box, findsOneWidget, reason: reason);
  final frame = tester.getRect(box).inflate(.5);
  final texts = find.descendant(of: box, matching: find.byType(RichText));
  expect(texts, findsWidgets, reason: reason);
  for (final e in texts.evaluate()) {
    final paragraph = e.renderObject! as RenderParagraph;
    final rect = paragraph.localToGlobal(Offset.zero) & paragraph.size;
    final words = paragraph.text.toPlainText();
    expect(frame.contains(rect.topLeft) && frame.contains(rect.bottomRight), isTrue, reason: '$reason: "$words" at $rect leaves $frame');
    expect(paragraph.didExceedMaxLines, isFalse, reason: '$reason: "$words" is cut short');
  }
}

/// Writes a PNG of the current frame to $IAC_SHOTS (when set).
Future<void> shot(WidgetTester tester, String name) async {
  final dir = Platform.environment['IAC_SHOTS'];
  if (dir == null) return;
  await tester.runAsync(() async {
    final boundary = tester.renderObject<RenderRepaintBoundary>(find.byType(RepaintBoundary).first);
    final image = await boundary.toImage(pixelRatio: 1);
    final bytes = await image.toByteData(format: ImageByteFormat.png);
    await Directory(dir).create(recursive: true);
    await File('$dir/$name.png').writeAsBytes(bytes!.buffer.asUint8List());
  });
}

/// Tear the shell and demo timers down before the test body returns
/// (the framework checks for pending timers before tearDown callbacks run).
Future<void> endShell(WidgetTester tester, GameController c) async {
  c.stop();
  await tester.pumpWidget(const SizedBox());
  await tester.pump(const Duration(seconds: 5));
}
