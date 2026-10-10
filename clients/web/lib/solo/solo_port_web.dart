import 'dart:convert';
import 'dart:js_interop';

import 'package:web/web.dart' as web;

import 'solo_engine.dart';

/// The worker script of the single-player bundle, beside the page.
const _workerUrl = 'solo/worker.js';

SoloPort openPort() => _WorkerPort();

extension type _Envelope._(JSObject _) implements JSObject {
  external factory _Envelope({String type, String text});
  external String get type;
  external String get text;
}

class _WorkerPort implements SoloPort {
  _WorkerPort() {
    try {
      final worker = web.Worker(_workerUrl.toJS);
      worker.onmessage = ((web.MessageEvent event) {
        final envelope = event.data as _Envelope;
        _handler?.call(envelope.type, envelope.text);
      }).toJS;
      worker.onerror = ((web.Event event) {
        _fail('the engine worker stopped: ${event.isA<web.ErrorEvent>() ? (event as web.ErrorEvent).message : 'could not load $_workerUrl'}');
      }).toJS;
      _worker = worker;
    } catch (e) {
      _failure = 'this browser cannot start the engine worker: $e';
    }
  }

  web.Worker? _worker;
  String? _failure;
  void Function(String type, String text)? _handler;

  void _fail(String error) {
    _failure = error;
    _handler?.call('status', jsonEncode({'phase': SoloPhase.failed.name, 'error': error}));
  }

  @override
  set onMessage(void Function(String type, String text) handler) {
    _handler = handler;
    final failure = _failure;
    if (failure != null) Future.microtask(() => _fail(failure));
  }

  @override
  void post(String type, [String text = '']) => _worker?.postMessage(_Envelope(type: type, text: text));

  @override
  void protectStorage() {
    try {
      web.window.navigator.storage.persist();
    } catch (_) {
      // Not every browser has it; the save works without.
    }
  }

  @override
  void close() {
    _worker?.terminate();
    _worker = null;
  }
}
