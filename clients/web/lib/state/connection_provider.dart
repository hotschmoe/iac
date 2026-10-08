import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:iac_shared/iac_shared.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

/// Live WebSocket connection to the Dart IAC server.
class GameConnection {
  WebSocketChannel? _channel;
  StreamSubscription? _sub;
  final _messages = StreamController<ServerMessage>.broadcast();
  final _status = StreamController<ConnectionStatus>.broadcast();

  bool _connected = false;
  int? playerId;

  bool get connected => _connected;
  Stream<ServerMessage> get messages => _messages.stream;
  Stream<ConnectionStatus> get status => _status.stream;

  /// Default WS URL: same host as the page when on web, else localhost.
  static String defaultUrl({String path = '/ws'}) {
    // Allow override: ?ws=ws://host:7777/ws
    if (kIsWeb) {
      final override = Uri.base.queryParameters['ws'];
      if (override != null && override.isNotEmpty) return override;
      final scheme = Uri.base.scheme == 'https' ? 'wss' : 'ws';
      final host = Uri.base.host.isEmpty ? '127.0.0.1' : Uri.base.host;
      final port = Uri.base.hasPort ? Uri.base.port : defaultPort;
      return '$scheme://$host:$port$path';
    }
    return 'ws://127.0.0.1:$defaultPort$path';
  }

  Future<void> connect(String url) async {
    await disconnect();
    _status.add(ConnectionStatus.connecting);
    try {
      final uri = Uri.parse(url);
      _channel = WebSocketChannel.connect(uri);
      await _channel!.ready;
      _connected = true;
      _status.add(ConnectionStatus.connected);
      _sub = _channel!.stream.listen(
        (data) {
          try {
            final raw = data is String ? data : String.fromCharCodes(data as List<int>);
            final msg = ServerMessage.decode(raw);
            if (msg is AuthResult && msg.success) {
              playerId = msg.playerId;
            }
            _messages.add(msg);
          } catch (e) {
            debugPrint('WS parse error: $e');
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
      rethrow;
    }
  }

  void send(ClientMessage message) {
    if (!_connected || _channel == null) return;
    _channel!.sink.add(message.encode());
  }

  void auth(String playerName) {
    send(AuthRequest(playerName: playerName));
  }

  void sendCommand(Command command) {
    send(command);
  }

  Future<void> disconnect() async {
    await _sub?.cancel();
    _sub = null;
    await _channel?.sink.close();
    _channel = null;
    _connected = false;
    playerId = null;
  }

  void dispose() {
    disconnect();
    _messages.close();
    _status.close();
  }
}

enum ConnectionStatus {
  disconnected,
  connecting,
  connected,
  error,
}
