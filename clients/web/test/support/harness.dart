import 'dart:io';
import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/prefs.dart';
import 'package:iac_client/console/services.dart';
import 'package:iac_client/design/theme.dart';
import 'package:iac_client/state/game_controller.dart';
import 'package:iac_client/views/shell.dart';

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
  Duration settle = const Duration(milliseconds: 600),
}) async {
  await tester.runAsync(loadFonts);
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  final c = controller ?? GameController();
  if (controller == null) {
    c.startDemo();
    addTearDown(() {
      c.stop();
      c.dispose();
    });
  }
  await tester.pumpWidget(MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: consoleTheme(),
    home: Shell(controller: c, prefs: prefs ?? ConsolePrefs(), initial: screen),
  ));
  await tester.pump(settle);
  return c;
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
