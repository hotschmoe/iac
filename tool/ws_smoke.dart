import 'dart:convert';

import 'package:web_socket_channel/web_socket_channel.dart';

Future<void> main() async {
  final ch = WebSocketChannel.connect(Uri.parse('ws://127.0.0.1:7777/ws'));
  await ch.ready;
  ch.sink.add(jsonEncode({'type': 'auth', 'player_name': 'Admiral'}));
  var sawAuth = false;
  var sawState = false;
  await for (final msg in ch.stream.timeout(const Duration(seconds: 4))) {
    final s = msg is String ? msg : msg.toString();
    print(s.length > 300 ? '${s.substring(0, 300)}…' : s);
    if (s.contains('auth_result')) sawAuth = true;
    if (s.contains('full_state') || s.contains('tick_update')) sawState = true;
    if (sawAuth && sawState) {
      await ch.sink.close();
      break;
    }
  }
  if (!sawAuth || !sawState) {
    throw StateError('smoke failed auth=$sawAuth state=$sawState');
  }
  print('WS smoke OK');
}
