import 'dart:async';
import 'dart:convert';

import 'solo_engine.dart';

SoloPort openPort() => _NoPort();

/// Outside a browser there is no worker to run the engine in.
class _NoPort implements SoloPort {
  @override
  set onMessage(void Function(String type, String text) handler) {
    Future.microtask(() => handler(
        'status', jsonEncode({'phase': SoloPhase.failed.name, 'error': 'single player runs in a browser only'})));
  }

  @override
  void post(String type, [String text = '']) {}

  @override
  void close() {}
}
