// Login flow against a fake server that follows the Rust server's rules:
// unknown name registers and returns a token once, known name needs it.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/state/connection_provider.dart';
import 'package:iac_client/state/game_controller.dart';
import 'package:iac_client/state/token_store.dart';

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

Future<void> _until(Listenable source, bool Function() cond) async {
  if (cond()) return;
  final done = Completer<void>();
  void check() {
    if (cond() && !done.isCompleted) done.complete();
  }

  source.addListener(check);
  try {
    await done.future;
  } finally {
    source.removeListener(check);
  }
}

void main() {
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
    await _until(first, () => first.isLive);
    expect(server.authRequests.single.containsKey('token'), isFalse);
    expect(tokens.load(server.url, 'Admiral'), _issued);
    expect(first.authRejection, isNull);
    expect(first.state.alerts.any((a) => a.message == 'New account token' && a.detail == _issued), isTrue);
    first.stop();

    final second = controller(tokens);
    await second.start(params: params, name: 'Admiral');
    await _until(second, () => second.isLive);
    expect(server.authRequests.last['token'], _issued);
    expect(second.state.alerts.any((a) => a.message == 'New account token'), isFalse, reason: 'token is shown once');
  });

  test('a known name without a token is sent back to the login form, and a typed token fixes it', () async {
    server.accounts['Taken'] = _issued;
    final c = controller(MemoryTokenStore());
    await c.start(params: ConnectParams(url: server.url), name: 'Taken');
    await _until(c, () => c.authRejection != null);
    expect(c.authRejection!.needsToken, isTrue);
    expect(c.authRejection!.message, contains('already registered'));
    expect(c.isLive, isFalse);

    await c.start(params: ConnectParams(url: server.url), name: 'Taken', token: _issued);
    await _until(c, () => c.isLive);
    expect(c.authRejection, isNull);
  });

  test('a wrong remembered token is rejected as invalid, and not retried forever', () async {
    server.accounts['Taken'] = _issued;
    final tokens = MemoryTokenStore()..save(server.url, 'Taken', 'f' * 64);
    final c = controller(tokens);
    await c.start(params: ConnectParams(url: server.url), name: 'Taken');
    await _until(c, () => c.authRejection != null);
    expect(c.authRejection!.code, proto.ErrorCode.invalidToken);
    await Future<void>.delayed(const Duration(milliseconds: 300));
    expect(server.authRequests, hasLength(1));
  });

  test('?token= is honoured and then remembered', () async {
    server.accounts['Agent'] = _issued;
    final tokens = MemoryTokenStore();
    final c = controller(tokens);
    await c.start(params: ConnectParams(url: server.url, token: _issued), name: 'Agent');
    await _until(c, () => c.isLive);
    expect(tokens.load(server.url, 'Agent'), _issued);
  });

}
