import 'package:web/web.dart' as web;

import 'kv_store.dart';

KvStore createKv() => _WebKv();

class _WebKv implements KvStore {
  @override
  String? read(String key) {
    try {
      return web.window.localStorage.getItem(key);
    } catch (_) {
      return null;
    }
  }

  @override
  void write(String key, String value) {
    try {
      web.window.localStorage.setItem(key, value);
    } catch (_) {}
  }
}
