// The single-player front door and its line to the engine, against a port
// that speaks the worker's messages (solo/web/worker.js). The engine itself
// is exercised for real by `cargo test -p iac-solo` and, in a browser, by
// tool/solo_e2e.dart.

import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/main.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/solo/solo_engine.dart';
import 'package:iac_client/state/connection_provider.dart';
import 'package:iac_client/state/game_controller.dart';
import 'package:iac_client/state/token_store.dart';
import 'package:iac_client/views/solo_screen.dart';

import 'support/fake_server.dart';
import 'support/harness.dart';

const _options = {
  'presets': [
    {'name': 'season', 'pace': 10.0, 'first_cruiser_s': 165600, 'endgame_s': 622000},
    {'name': 'blitz', 'pace': 600.0, 'first_cruiser_s': 2520, 'endgame_s': 10800},
  ],
  'personalities': ['turtle', 'miner', 'raider', 'explorer', 'mixed'],
  'attentions': ['check-in', 'hybrid', 'hands-on'],
  'max_bots': 23,
  'catch_up_limit': 604800,
};

Map<String, dynamic> _world({int tick = 0, int behind = 0, Map<String, int>? catchUp, List<String> notices = const []}) => {
      'commander': 'Admiral',
      'pace': 10.0,
      'preset': 'season',
      'seed': '00000000DEADBEEF',
      'tick': tick,
      'bots': 8,
      'behind': behind,
      'catch_up': ?catchUp,
      'notices': notices,
      'save_due': false,
    };

/// The worker's end of the line: records what the page posts, and says
/// what the test tells it to.
class _Worker implements SoloPort {
  void Function(String type, String text)? _handler;
  final List<(String type, String text)> posted = [];

  @override
  set onMessage(void Function(String type, String text) handler) => _handler = handler;

  @override
  void post(String type, [String text = '']) => posted.add((type, text));

  @override
  void protectStorage() {}

  @override
  void close() {}

  void status(String phase, {Map<String, dynamic>? world, String? refused, String? error, double rate = 0}) =>
      _handler!('status', jsonEncode({'phase': phase, 'options': _options, 'world': world, 'refused': ?refused, 'error': ?error, 'rate': rate}));

  void frame(Map<String, dynamic> message) => _handler!('frame', jsonEncode(message));

  List<String> get types => [for (final p in posted) p.$1];
}

/// Pumps the front door on [engine]; [entered] gets `newWorld` of every entry.
Future<void> _pumpSolo(WidgetTester tester, SoloEngine engine, List<bool> entered) async {
  await tester.runAsync(loadFonts);
  tester.view.physicalSize = const Size(1280, 900);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(MaterialApp(
      home: Scaffold(body: SoloScreen(engine: engine, onEnter: ({required bool newWorld}) => entered.add(newWorld)))));
  await tester.pump();
}

Future<void> _tap(WidgetTester tester, String label) async {
  await tester.tap(find.text(label.toUpperCase()));
  await tester.pump(const Duration(milliseconds: 100));
}

void main() {
  testWidgets('a new game is asked of the engine with the chosen pace and seats, and entered when it runs', (tester) async {
    final worker = _Worker();
    final engine = SoloEngine(worker);
    final entered = <bool>[];
    await _pumpSolo(tester, engine, entered);
    expect(find.text('LOADING THE ENGINE ...'), findsOneWidget);

    worker.status('idle');
    await tester.pump();
    expect(find.text('NEW GAME'), findsOneWidget);
    await _tap(tester, 'blitz x600');
    await _tap(tester, '4');
    await _tap(tester, 'start');
    final asked = jsonDecode(worker.posted.single.$2) as Map<String, dynamic>;
    expect(worker.posted.single.$1, 'new_game');
    expect(asked, {
      'name': 'Admiral',
      'pace': 'blitz',
      'bots': ['turtle:hands-on', 'miner:hands-on', 'raider:hands-on', 'explorer:hands-on'],
    }, reason: 'a world that is over in an hour gets bots that stay at the controls');

    worker.status('idle', error: 'invalid player name: that name is reserved');
    await tester.pump();
    expect(find.byKey(const Key('solo-error')), findsOneWidget);
    expect(entered, isEmpty);

    await _tap(tester, 'start');
    worker.status('running', world: _world());
    await tester.pump();
    expect(entered, [true], reason: 'entered once, as a world just started');
  });

  testWidgets('a saved world is continued through its absence and the report, or abandoned after asking', (tester) async {
    final worker = _Worker();
    final engine = SoloEngine(worker);
    final entered = <bool>[];
    await _pumpSolo(tester, engine, entered);
    worker.status('held', world: _world(tick: 5000, behind: 7200));
    await tester.pump();
    expect(find.text('SAVED WORLD'), findsOneWidget);
    expect(find.textContaining('Away 2h 00m'), findsOneWidget);

    await _tap(tester, 'abandon this world');
    expect(find.textContaining('cannot be brought back'), findsOneWidget);
    expect(worker.posted, isEmpty, reason: 'nothing is deleted before the player confirms');
    await _tap(tester, 'keep it');

    await _tap(tester, 'continue');
    expect(worker.types, ['continue']);
    worker.status('running', world: _world(tick: 6000, behind: 6200, catchUp: {'played': 1000, 'remaining': 6200}), rate: 500);
    await tester.pump();
    expect(find.text('CATCHING UP'), findsOneWidget);
    expect(find.textContaining('500 ticks a second'), findsOneWidget);
    expect(find.textContaining('about 0m 13s left'), findsOneWidget);
    expect(entered, isEmpty);

    await _tap(tester, 'take command now');
    expect(worker.types, ['continue', 'take_command']);
    const notices = [
      'Away 2h 00m: the world ran on without you for 16m 40s (1000 ticks); you took command early and the last 1h 43m were skipped.',
      'While away: 1 building finished.',
    ];
    worker.status('running', world: _world(tick: 6000, notices: notices));
    await tester.pump();
    expect(find.text('WHILE YOU WERE AWAY'), findsOneWidget);
    expect(find.text(notices[0]), findsOneWidget);
    expect(find.text(notices[1]), findsOneWidget);
    expect(entered, isEmpty, reason: 'the report waits to be read');
    await _tap(tester, 'take command');
    expect(entered, [false], reason: 'entered once, as a world that was already there');

    final second = _Worker();
    await tester.pumpWidget(const SizedBox());
    await _pumpSolo(tester, SoloEngine(second), []);
    second.status('held', world: _world(tick: 5000));
    await tester.pump();
    await _tap(tester, 'abandon this world');
    await _tap(tester, 'delete the save');
    expect(second.types, ['abandon']);
    second.status('idle', refused: 'this save is economy version 1, this build runs version 2; start a new game');
    await tester.pump();
    expect(find.text('THE SAVE IN THIS BROWSER WAS NOT LOADED'), findsOneWidget);
    expect(find.byKey(const Key('solo-refused')), findsOneWidget);
  });

  test('a session with the engine is the wire protocol over the worker, and ends when the engine stops', () async {
    final worker = _Worker();
    final engine = SoloEngine(worker);
    worker.status('running', world: _world());
    final c = GameController(tokens: MemoryTokenStore());
    addTearDown(() {
      c.stop();
      c.dispose();
    });

    await c.open(Uplink(id: 'single-player', label: 'SINGLE PLAYER', feed: 'The engine in this tab', dial: engine.connect), name: 'Admiral');
    await Future<void>.delayed(Duration.zero);
    expect(worker.types, ['open', 'frame']);
    expect(jsonDecode(worker.posted.last.$2), {'type': 'auth', 'player_name': 'Admiral', 'agent': false});

    worker.frame({'type': 'auth_result', 'success': true, 'player_id': 7});
    final state = golden('server_message', 'full_state');
    (state['player'] as Map<String, dynamic>)['name'] = 'Admiral';
    worker.frame(state);
    await until(c, () => c.isLive);
    expect(c.linkLabel, 'SINGLE PLAYER');
    expect(c.issuedToken, isNull, reason: 'a single-player account has no token to keep');
    expect(c.state.alerts.single.detail, 'The engine in this tab');

    c.sendCommand(const proto.ScanCommand(fleetId: 3));
    await Future<void>.delayed(Duration.zero);
    expect(jsonDecode(worker.posted.last.$2), {'type': 'command', 'action': 'scan', 'fleet_id': 3});

    worker.status('failed', error: 'tick 12 failed');
    await until(c, () => c.link == LinkState.reconnecting);
    expect(engine.status.value.error, 'tick 12 failed');
  });

  testWidgets('the front door is the engine in the single-player bundle and a server login everywhere else', (tester) async {
    await tester.runAsync(loadFonts);
    tester.view.physicalSize = const Size(1280, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    const params = ConnectParams(url: 'ws://127.0.0.1:9/ws');

    await tester.pumpWidget(const MaterialApp(home: GameScreen(params: params, boot: false)));
    await tester.pump();
    expect(find.text('IDENTIFY YOURSELF'), findsOneWidget);
    expect(find.text('SINGLE PLAYER'), findsNothing);
    expect(find.text('OFFLINE DEMO'), findsNothing);

    final worker = _Worker();
    await tester.pumpWidget(const SizedBox());
    await tester.pumpWidget(MaterialApp(home: GameScreen(solo: SoloEngine(worker), params: params, boot: false)));
    await tester.pump();
    worker.status('idle');
    await tester.pump();
    expect(find.text('NEW GAME'), findsOneWidget);
    await _tap(tester, 'connect to a server instead');
    expect(find.text('IDENTIFY YOURSELF'), findsOneWidget);
    await _tap(tester, 'single player');
    expect(find.text('NEW GAME'), findsOneWidget);
  });
}
