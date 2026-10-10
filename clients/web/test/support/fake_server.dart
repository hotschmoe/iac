import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

import '../fixtures_util.dart';

/// The token the fake server issues to every new account.
final issuedToken = 'a' * 64;

/// A WebSocket server that follows the Rust server's login rules (an unknown
/// name registers and gets a token once, a known name needs it) and then
/// sends whatever the test tells it to. Its frames are built from the golden
/// fixtures, so it cannot drift from the Rust shapes.
class FakeServer {
  FakeServer({this.shape});

  /// Edits the golden `full_state` before it is sent (the player name is
  /// already set).
  final void Function(Map<String, dynamic> state)? shape;

  late final HttpServer _http;
  final Map<String, String> accounts = {};
  final List<Map<String, dynamic>> authRequests = [];

  /// Every frame the clients sent after logging in.
  final List<Map<String, dynamic>> received = [];
  final List<WebSocket> _sockets = [];
  final List<StreamSubscription<dynamic>> _subs = [];

  Future<void> start() async {
    _http = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    _http.listen((req) async {
      final ws = await WebSocketTransformer.upgrade(req);
      _sockets.add(ws);
      _subs.add(ws.listen((data) {
        final m = jsonDecode(data as String) as Map<String, dynamic>;
        if (m['type'] != 'auth') {
          received.add(m);
          return;
        }
        authRequests.add(m);
        final name = m['player_name'] as String;
        final token = m['token'] as String?;
        final known = accounts[name];
        if (known == null) {
          accounts[name] = issuedToken;
          ws.add(jsonEncode({'type': 'auth_result', 'success': true, 'player_id': 7, 'token': issuedToken}));
          ws.add(jsonEncode(fullState(name)));
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
          ws.add(jsonEncode(fullState(name)));
        }
      }));
    });
  }

  String get url => 'ws://127.0.0.1:${_http.port}/ws';

  /// Send one frame to every connected client.
  void send(Map<String, dynamic> message) {
    final frame = jsonEncode(message);
    for (final ws in _sockets) {
      ws.add(frame);
    }
  }

  Future<void> stop() async {
    for (final s in _subs) {
      await s.cancel();
    }
    await _http.close(force: true);
  }

  Map<String, dynamic> fullState(String name) {
    final state = golden('server_message', 'full_state');
    (state['player'] as Map<String, dynamic>)['name'] = name;
    shape?.call(state);
    return state;
  }
}

/// A fresh copy of one golden fixture, e.g. `golden('event_kind', 'scan_completed')`.
Map<String, dynamic> golden(String group, String name) =>
    readJson(fixtureFiles('protocol/$group').firstWhere((f) => baseName(f) == name)) as Map<String, dynamic>;

/// Completes when [cond] holds, checking each time [source] notifies.
Future<void> until(Listenable source, bool Function() cond) async {
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
