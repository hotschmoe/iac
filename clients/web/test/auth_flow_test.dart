// Login flow against a fake server that follows the Rust server's rules:
// unknown name registers and returns a token once, known name needs it.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/state/connection_provider.dart';
import 'package:iac_client/state/game_controller.dart';
import 'package:iac_client/state/token_store.dart';
import 'package:iac_client/theme/amber_theme.dart';
import 'package:iac_client/views/login_screen.dart';

import 'fixtures_util.dart';

final _issued = 'a' * 64;

class _FakeServer {
  late final HttpServer _http;
  final Map<String, String> accounts = {}; // name -> token
  final List<Map<String, dynamic>> authRequests = [];
  final List<StreamSubscription<dynamic>> _subs = [];

  Future<void> start() async {
    _http = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    _http.listen((req) async {
      final ws = await WebSocketTransformer.upgrade(req);
      _subs.add(ws.listen((data) {
        final m = jsonDecode(data as String) as Map<String, dynamic>;
        if (m['type'] != 'auth') return;
        authRequests.add(m);
        final name = m['player_name'] as String;
        final token = m['token'] as String?;
        final known = accounts[name];
        if (known == null) {
          accounts[name] = _issued;
          ws.add(jsonEncode({'type': 'auth_result', 'success': true, 'player_id': 7, 'token': _issued}));
          ws.add(jsonEncode(_fullState(name)));
        } else if (token == null) {
          ws.add(jsonEncode({
            'type': 'auth_result',
            'success': false,
            'code': 'TokenRequired',
            'message': "player '$name' is already registered; present its token to log in",
          }));
        } else if (token != known) {
          ws.add(jsonEncode({
            'type': 'auth_result',
            'success': false,
            'code': 'InvalidToken',
            'message': "token rejected for player '$name'",
          }));
        } else {
          ws.add(jsonEncode({'type': 'auth_result', 'success': true, 'player_id': 7}));
          ws.add(jsonEncode(_fullState(name)));
        }
      }));
    });
  }

  String get url => 'ws://127.0.0.1:${_http.port}/ws';

  Future<void> stop() async {
    for (final s in _subs) {
      await s.cancel();
    }
    await _http.close(force: true);
  }

  /// The golden full_state, so this fake never drifts from the Rust shapes.
  Map<String, dynamic> _fullState(String name) {
    final file = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
    final state = readJson(file) as Map<String, dynamic>;
    (state['player'] as Map<String, dynamic>)['name'] = name;
    return state;
  }
}

Future<void> _until(bool Function() cond) async {
  final deadline = DateTime.now().add(const Duration(seconds: 5));
  while (!cond()) {
    if (DateTime.now().isAfter(deadline)) fail('condition not reached in time');
    await Future<void>.delayed(const Duration(milliseconds: 20));
  }
}

class _ThrowingStorage implements KeyValueStorage {
  @override
  String? read(String key) => throw StateError('storage blocked');
  @override
  void write(String key, String value) => throw StateError('storage blocked');
}

class _MapStorage implements KeyValueStorage {
  final Map<String, String> data = {};
  @override
  String? read(String key) => data[key];
  @override
  void write(String key, String value) => data[key] = value;
}

void main() {
  setUpAll(() => GoogleFonts.config.allowRuntimeFetching = false);

  testWidgets('login form re-prompts with the refusal and passes the typed token on', (tester) async {
    String? sentName, sentToken;
    await tester.pumpWidget(MaterialApp(
      theme: Amber.themeData(),
      home: Scaffold(
        body: LoginScreen(
          url: 'ws://x/ws',
          initialName: 'Taken',
          error: "player 'Taken' is already registered; present its token to log in",
          onConnect: (n, t) {
            sentName = n;
            sentToken = t;
          },
          onDemo: () {},
        ),
      ),
    ));
    expect(find.byKey(const Key('login-error')), findsOneWidget);
    expect(find.textContaining('already registered'), findsOneWidget);
    await tester.enterText(find.byKey(const Key('token-field')), '  secret  ');
    await tester.tap(find.text('CONNECT'));
    expect(sentName, 'Taken');
    expect(sentToken, 'secret');
  });

  testWidgets('first-time login form has no token field', (tester) async {
    await tester.pumpWidget(MaterialApp(
      theme: Amber.themeData(),
      home: Scaffold(body: LoginScreen(url: 'ws://x/ws', initialName: 'Admiral', onConnect: (_, _) {}, onDemo: () {})),
    ));
    expect(find.byKey(const Key('token-field')), findsNothing);
  });

  late _FakeServer server;
  setUp(() async {
    server = _FakeServer();
    await server.start();
  });
  tearDown(() => server.stop());

  GameController controller(TokenStore tokens) {
    final c = GameController(tokens: tokens);
    addTearDown(() {
      c.stop();
      c.dispose();
    });
    return c;
  }

  test('first login registers, shows and remembers the token; the next login sends it', () async {
    final tokens = MemoryTokenStore();
    final params = ConnectParams(url: server.url);

    final first = controller(tokens);
    await first.start(params: params, name: 'Admiral');
    await _until(() => first.isLive);
    expect(server.authRequests.single.containsKey('token'), isFalse);
    expect(tokens.load(server.url, 'Admiral'), _issued);
    expect(first.authRejection, isNull);
    expect(first.state.alerts.any((a) => a.message == 'New account token' && a.detail == _issued), isTrue);
    first.stop();

    final second = controller(tokens);
    await second.start(params: params, name: 'Admiral');
    await _until(() => second.isLive);
    expect(server.authRequests.last['token'], _issued);
    expect(second.state.alerts.any((a) => a.message == 'New account token'), isFalse, reason: 'token is shown once');
  });

  test('a known name without a token is sent back to the login form, and a typed token fixes it', () async {
    server.accounts['Taken'] = _issued;
    final c = controller(MemoryTokenStore());
    await c.start(params: ConnectParams(url: server.url), name: 'Taken');
    await _until(() => c.authRejection != null);
    expect(c.authRejection!.needsToken, isTrue);
    expect(c.authRejection!.message, contains('already registered'));
    expect(c.isLive, isFalse);

    await c.start(params: ConnectParams(url: server.url), name: 'Taken', token: _issued);
    await _until(() => c.isLive);
    expect(c.authRejection, isNull);
  });

  test('a wrong remembered token is rejected as invalid, and not retried forever', () async {
    server.accounts['Taken'] = _issued;
    final tokens = MemoryTokenStore()..save(server.url, 'Taken', 'f' * 64);
    final c = controller(tokens);
    await c.start(params: ConnectParams(url: server.url), name: 'Taken');
    await _until(() => c.authRejection != null);
    expect(c.authRejection!.code, proto.ErrorCode.invalidToken);
    await Future<void>.delayed(const Duration(milliseconds: 300));
    expect(server.authRequests, hasLength(1));
  });

  test('?token= is honoured and then remembered', () async {
    server.accounts['Agent'] = _issued;
    final tokens = MemoryTokenStore();
    final c = controller(tokens);
    await c.start(params: ConnectParams(url: server.url, token: _issued), name: 'Agent');
    await _until(() => c.isLive);
    expect(tokens.load(server.url, 'Agent'), _issued);
  });

  group('GuardedTokenStore', () {
    test('keeps working, from memory, when storage throws', () {
      final store = GuardedTokenStore(() => throw StateError('localStorage is not available'));
      expect(store.load('ws://x/ws', 'Ann'), isNull);
      expect(store.save('ws://x/ws', 'Ann', 'tok'), isFalse, reason: 'caller learns it is not durable');
      expect(store.load('ws://x/ws', 'Ann'), 'tok');

      final blocked = GuardedTokenStore(_ThrowingStorage.new);
      expect(blocked.save('s', 'n', 't'), isFalse);
      expect(blocked.load('s', 'n'), 't');
    });

    test('persists per server and name when storage works', () {
      final backing = _MapStorage();
      final store = GuardedTokenStore(() => backing);
      expect(store.save('ws://a/ws', 'Ann', 't1'), isTrue);
      store.save('ws://b/ws', 'Ann', 't2');
      expect(GuardedTokenStore(() => backing).load('ws://a/ws', 'Ann'), 't1');
      expect(GuardedTokenStore(() => backing).load('ws://b/ws', 'Ann'), 't2');
      expect(GuardedTokenStore(() => backing).load('ws://a/ws', 'ANN'), 't1',
          reason: 'names are case-insensitive on the server');
      expect(store.load('ws://a/ws', 'Bob'), isNull);
    });
  });
}
