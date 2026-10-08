import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import '../protocol/protocol.dart';

/// Where to connect and who to be, derived from the page URL.
///
///   ?ws=ws://host:port/ws   override the WebSocket URL
///   ?name=Admiral           player name (skips the login prompt)
///   ?token=...              account token for [name]; otherwise the one the
///                           browser remembered (or typed at the login form)
class ConnectParams {
  final String url;
  final String? name;
  final String? token;
  const ConnectParams({required this.url, this.name, this.token});

  /// Port of the Rust server when the page is not served by it.
  static const defaultPort = 7777;

  /// Path the client connects to. The Rust server accepts a WebSocket
  /// upgrade on any path; `/ws` keeps it distinct from static files.
  static const wsPath = '/ws';

  factory ConnectParams.fromUri(Uri base) {
    final q = base.queryParameters;
    final override = q['ws'];
    final String url;
    if (override != null && override.isNotEmpty) {
      url = override;
    } else if (base.scheme == 'http' || base.scheme == 'https') {
      // Same origin as the page: ws://<page host>:<page port>/ws
      final scheme = base.scheme == 'https' ? 'wss' : 'ws';
      url = '$scheme://${base.host}:${base.port}$wsPath';
    } else {
      url = 'ws://127.0.0.1:$defaultPort$wsPath';
    }
    final name = q['name']?.trim();
    final token = q['token'];
    return ConnectParams(
      url: url,
      name: (name == null || name.isEmpty) ? null : name,
      token: (token == null || token.isEmpty) ? null : token,
    );
  }
}

enum ConnectionStatus { disconnected, connecting, connected, error }

/// Live WebSocket connection to the Rust IAC server. Speaks the protocol in
/// lib/protocol/protocol.dart: JSON text frames, one message per frame.
class GameConnection {
  WebSocketChannel? _channel;
  StreamSubscription<dynamic>? _sub;
  final _messages = StreamController<ServerMessage>.broadcast();
  final _status = StreamController<ConnectionStatus>.broadcast();

  bool _connected = false;

  /// Called for frames that fail to decode (never silently dropped).
  void Function(String error)? onProtocolError;

  bool get connected => _connected;
  Stream<ServerMessage> get messages => _messages.stream;
  Stream<ConnectionStatus> get status => _status.stream;

  Future<void> connect(String url, {Duration timeout = const Duration(seconds: 5)}) async {
    await disconnect();
    _status.add(ConnectionStatus.connecting);
    try {
      final channel = WebSocketChannel.connect(Uri.parse(url));
      _channel = channel;
      await channel.ready.timeout(timeout);
      _connected = true;
      _status.add(ConnectionStatus.connected);
      _sub = channel.stream.listen(
        (data) {
          try {
            final raw = data is String ? data : String.fromCharCodes(data as List<int>);
            _messages.add(ServerMessage.decode(raw));
          } catch (e) {
            debugPrint('WS decode error: $e');
            onProtocolError?.call('$e');
          }
        },
        onError: (Object e) {
          debugPrint('WS error: $e');
          _connected = false;
          _status.add(ConnectionStatus.error);
        },
        onDone: () {
          _connected = false;
          _status.add(ConnectionStatus.disconnected);
        },
        cancelOnError: false,
      );
    } catch (e) {
      _connected = false;
      _status.add(ConnectionStatus.error);
      _closeQuietly(_channel);
      _channel = null;
      rethrow;
    }
  }

  void send(ClientMessage message) {
    if (!_connected || _channel == null) return;
    _channel!.sink.add(message.encode());
  }

  void auth(String playerName, {String? token}) =>
      send(AuthRequest(playerName: playerName, token: token));

  void sendCommand(Command command) => send(CommandMessage(command));

  void requestFullState() => send(const RequestFullState());

  Future<void> disconnect() async {
    await _sub?.cancel();
    _sub = null;
    _closeQuietly(_channel);
    _channel = null;
    _connected = false;
  }

  /// Fire-and-forget close: on a socket that never opened the close future
  /// may never complete, so never await it.
  void _closeQuietly(WebSocketChannel? ch) {
    if (ch == null) return;
    unawaited(ch.sink.close().then<void>((_) {}, onError: (Object _) {}));
  }

  void dispose() {
    disconnect();
    _messages.close();
    _status.close();
  }
}
