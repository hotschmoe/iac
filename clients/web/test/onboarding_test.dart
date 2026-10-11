// The first-five-minutes guide, driven end to end: a real GameController on
// a WebSocket to a server that speaks the golden protocol frames, and the
// whole Shell on top.

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/kv_store.dart';
import 'package:iac_client/console/onboarding.dart';
import 'package:iac_client/console/prefs.dart';
import 'package:iac_client/console/services.dart';
import 'package:iac_client/state/connection_provider.dart';
import 'package:iac_client/state/game_controller.dart';
import 'package:iac_client/state/token_store.dart';
import 'package:iac_client/views/onboarding.dart';
import 'package:iac_client/views/rail.dart';

import 'support/fake_server.dart';
import 'support/harness.dart';

const _home = {'q': 4, 'r': -2};
const _oreField = {'q': 3, 'r': -2};

/// The golden state cut back to a first login: one fleet docked at home, an
/// empty build queue, and the home sector charted with its lane to the ore
/// field next door.
void _freshEmpire(Map<String, dynamic> s) {
  s['fleets'] = [_fleet()];
  final hw = s['homeworld'] as Map<String, dynamic>;
  hw['build_queue'] = <dynamic>[];
  hw['build_pending'] = <dynamic>[];
  final sectors = s['known_sectors'] as List<dynamic>;
  final field = sectors.cast<Map<String, dynamic>>().firstWhere((x) => x['location']['q'] == _oreField['q'] && x['location']['r'] == _oreField['r']);
  field.remove('hostiles');
  sectors.add({
    'location': _home,
    'terrain': field['terrain'],
    'resources': {'metal': 'None', 'crystal': 'None', 'deuterium': 'None'},
    'connections': [_oreField],
    'threat': field['threat'],
    'last_seen': field['last_seen'],
    'live': true,
  });
}

Map<String, dynamic> _fleet({Map<String, int> at = _home, String state = 'Docked', String? policy}) {
  final f = (golden('server_message', 'full_state')['fleets'] as List<dynamic>).first as Map<String, dynamic>;
  f
    ..['location'] = at
    ..['state'] = state
    ..['cooldown_remaining'] = 0
    ..['home_fuel'] = at == _home ? 0.0 : 31.5
    ..remove('policy');
  if (policy != null) {
    f['policy'] = {
      'preset': policy,
      'params': {'min_fuel_pct': 10, 'cargo_return_pct': 85, 'max_range': 4, 'engage_ratio_x10': 15, 'fuel_budget_pct': 20},
    };
  }
  return f;
}

Map<String, dynamic> _tick(int tick, Map<String, dynamic> fleet, {Map<String, dynamic>? homeworld, List<Map<String, dynamic>> events = const []}) => {
      'type': 'tick_update',
      'tick': tick,
      'fleets': [fleet],
      'homeworld_update': ?homeworld,
      if (events.isNotEmpty) 'events': [for (final e in events) {...e, 'tick': tick}],
    };

/// Let the sockets run until [cond] holds, then draw a frame.
Future<void> _settle(WidgetTester tester, bool Function() cond) async {
  await tester.runAsync(() async {
    for (var i = 0; i < 600 && !cond(); i++) {
      await Future<void>.delayed(const Duration(milliseconds: 5));
    }
  });
  expect(cond(), isTrue, reason: 'the server and client did not get there');
  await tester.pump(const Duration(milliseconds: 100));
}

Onboarding _guide(WidgetTester t) => Console.of(t.element(find.byType(Rail))).onboarding;

Finder _marked(String id) => find.descendant(
    of: find.byWidgetPredicate((w) => w is GuideTarget && w.id == id),
    matching: find.byWidgetPredicate((w) => w is CustomPaint && w.foregroundPainter is GuideBrackets));

Finder _strip(String text) => find.descendant(of: find.byKey(const Key('guide-strip')), matching: find.textContaining(text, findRichText: true));

void main() {
  testWidgets('a new account is walked through its first steps by what it does, and the next login picks up where it left off', (tester) async {
    final server = FakeServer(shape: _freshEmpire);
    await tester.runAsync(server.start);
    addTearDown(() => tester.runAsync(server.stop));
    final tokens = MemoryTokenStore();
    final browser = MemoryKv();
    String? copied;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
      if (call.method == 'Clipboard.setData') copied = (call.arguments as Map<dynamic, dynamic>)['text'] as String;
      return null;
    });
    addTearDown(() => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, null));

    final c = GameController(tokens: tokens);
    await tester.runAsync(() => c.start(params: ConnectParams(url: server.url), name: 'Rookie'));
    await _settle(tester, () => c.isLive);
    await pumpShell(tester, controller: c, prefs: ConsolePrefs(browser));
    final home = c.state.hw!;

    expect(_strip('SAVE YOUR ACCOUNT TOKEN'), findsOneWidget);
    expect(find.descendant(of: find.byKey(const Key('guide-token')), matching: find.text(issuedToken)), findsOneWidget);
    expect(c.state.events.any((e) => e.message.contains(issuedToken)), isFalse, reason: 'the token is not dumped into the log');
    await tester.tap(find.byKey(const Key('guide-copy')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(copied, issuedToken);
    await tester.tap(find.byKey(const Key('guide-token-saved')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(c.issuedToken, isNull);

    expect(_strip('QUEUE A BUILDING'), findsOneWidget);
    expect(_marked('build'), findsNWidgets(2), reason: 'the next-up card offers the upgrade and so does build slot A');
    expect(find.byKey(const Key('first-hour')), findsNothing, reason: 'the list waits until the walk is through');
    await tester.ensureVisible(find.byKey(const Key('sug-build-metalMine')));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.tap(find.byKey(const Key('sug-build-metalMine')));
    await _settle(tester, () => server.received.any((m) => m['action'] == 'build' && m['building_type'] == 'MetalMine'));
    expect(_strip('QUEUE A BUILDING'), findsOneWidget, reason: 'the lesson waits for the server, not for the click');
    final building = Map<String, dynamic>.of(home.toJson())
      ..['build_queue'] = [
        {'id': 311, 'building_type': 'MetalMine', 'target_level': 1, 'start_tick': 4821, 'end_tick': 4986},
      ];
    server.send(_tick(4821, _fleet(), homeworld: building, events: [golden('event_kind', 'queue__started')]));
    await _settle(tester, () => _guide(tester).current == Lesson.scan);

    expect(_strip('SCAN THE LANES'), findsOneWidget);
    expect(_strip('Open the Windshield.'), findsOneWidget);
    expect(_marked('build'), findsNothing);
    expect(tester.widget<Rail>(find.byType(Rail)).mark, Screen.windshield);
    await tester.tap(find.byKey(const Key('guide-go')));
    await tester.pump(const Duration(milliseconds: 300));
    expect(Console.of(tester.element(find.byType(Rail))).screen, Screen.windshield);
    expect(tester.widget<Rail>(find.byType(Rail)).mark, isNull);
    expect(_strip('Press SCAN.'), findsOneWidget);
    expect(_marked('dock-scan'), findsOneWidget);
    double strip() => tester.getSize(find.byKey(const Key('guide-strip'))).height;
    final kept = strip();
    await tester.tap(find.byKey(const ValueKey('dock-scan')));
    await _settle(tester, () => server.received.any((m) => m['action'] == 'scan'));
    server.send(_tick(4822, _fleet(), events: [golden('event_kind', 'scan_completed')]));
    await _settle(tester, () => _guide(tester).current == Lesson.jump);

    expect(_strip('JUMP THROUGH A GATE'), findsOneWidget);
    expect(_strip('The marked gates are quiet.'), findsOneWidget);
    expect(_marked('gates'), findsOneWidget);
    expect(strip(), kept, reason: 'the strip keeps the height of its longest note, so the screen under it does not move between lessons');
    server.send(_tick(4823, _fleet(at: _oreField, state: 'Idle')));
    await _settle(tester, () => _guide(tester).current == Lesson.harvest);
    expect(_strip('Ore in this sector.'), findsOneWidget);
    expect(_marked('dock-harvest'), findsOneWidget);

    server.send(_tick(4824, _fleet(at: _oreField, state: 'Harvesting')));
    await _settle(tester, () => _guide(tester).current == Lesson.unload);
    expect(_marked('dock-recall'), findsOneWidget);

    server.send(_tick(4825, _fleet()));
    await _settle(tester, () => _guide(tester).current == Lesson.orders);
    expect(_strip('Press ORDERS under Fleet status'), findsOneWidget);
    expect(_marked('orders'), findsOneWidget);
    expect(strip(), kept);
    await tester.tap(find.byKey(const ValueKey('orders-toggle')));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.tap(find.byKey(const ValueKey('orders-doctrine-mineAndReturn')));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.ensureVisible(find.byKey(const ValueKey('orders-set')));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.tap(find.byKey(const ValueKey('orders-set')));
    await _settle(tester, () => server.received.any((m) => m['type'] == 'policy_update' && m['preset'] == 'mine_and_return'));
    expect(server.received.last.containsKey('params'), isFalse, reason: 'the doctrine at its defaults is what a typed "policy mine" sends');
    server.send(_tick(4826, _fleet(policy: 'mine_and_return')));
    await _settle(tester, () => _guide(tester).current == null);

    expect(find.byKey(const Key('guide-strip')), findsNothing);
    Console.of(tester.element(find.byType(Rail))).go(Screen.overview);
    await tester.pump(const Duration(milliseconds: 300));
    final card = find.byKey(const Key('first-hour'));
    expect(find.descendant(of: card, matching: find.text('6/11 done')), findsOneWidget);
    expect(tester.takeException(), isNull);
    await endShell(tester, c);

    final again = GameController(tokens: tokens);
    await tester.runAsync(() => again.start(params: ConnectParams(url: server.url), name: 'Rookie'));
    await _settle(tester, () => again.isLive);
    await pumpShell(tester, controller: again, prefs: ConsolePrefs(browser));
    expect(_guide(tester).active, isTrue);
    expect(_guide(tester).done(Lesson.scan), isTrue, reason: 'a scan leaves no trace in the state; the browser remembered it');
    expect(find.descendant(of: find.byKey(const Key('first-hour')), matching: find.text('6/11 done')), findsOneWidget);
    await tester.tap(find.byKey(const Key('first-hour-hide')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.byKey(const Key('first-hour')), findsNothing);
    await endShell(tester, again);
    again.dispose();
    c.dispose();
  });

  testWidgets('a returning player on a new browser gets no guide until they ask, and it then shows only what is left', (tester) async {
    final server = FakeServer();
    await tester.runAsync(server.start);
    addTearDown(() => tester.runAsync(server.stop));
    server.accounts['Veteran'] = issuedToken;

    final c = GameController(tokens: MemoryTokenStore());
    await tester.runAsync(() => c.start(params: ConnectParams(url: server.url, token: issuedToken), name: 'Veteran'));
    await _settle(tester, () => c.isLive);
    await pumpShell(tester, controller: c, prefs: ConsolePrefs(MemoryKv()));
    expect(_guide(tester).active, isFalse);
    expect(find.byKey(const Key('guide-strip')), findsNothing);
    expect(find.byKey(const Key('first-hour')), findsNothing);

    await tester.sendKeyEvent(LogicalKeyboardKey.slash, character: '?');
    await tester.pump(const Duration(milliseconds: 100));
    await tester.tap(find.byKey(const Key('set-guide')));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump(const Duration(milliseconds: 100));

    final guide = _guide(tester);
    expect(guide.done(Lesson.build), isTrue, reason: 'the golden empire has a build queue');
    expect(guide.done(Lesson.jump), isTrue, reason: 'and fleets away from home');
    expect(guide.current, Lesson.scan);
    expect(_strip('SCAN THE LANES'), findsOneWidget);
    await tester.tap(find.byKey(const Key('guide-skip')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.byKey(const Key('guide-strip')), findsNothing);
    expect(guide.active, isFalse);
    await endShell(tester, c);
    c.dispose();
  });

  testWidgets('the strip takes its own row on a phone and nothing overflows on any screen', (tester) async {
    final c = await pumpShell(tester, size: const Size(390, 844));
    final con = Console.of(tester.element(find.byType(Rail)));
    con.onboarding.setActive(true);
    await tester.pump(const Duration(milliseconds: 200));
    expect(find.byKey(const Key('guide-strip')), findsOneWidget);
    for (final s in Screen.values) {
      con.go(s);
      await tester.pump(const Duration(milliseconds: 300));
      expect(tester.takeException(), isNull, reason: s.name);
    }
    final strip = tester.getRect(find.byKey(const Key('guide-strip')));
    expect(strip.width, 390);
    expect(strip.bottom, lessThanOrEqualTo(tester.getRect(find.byType(IndexedStack).first).top));
    await endShell(tester, c);
  });
}
