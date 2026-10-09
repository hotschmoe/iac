import 'kv_store_stub.dart' if (dart.library.js_interop) 'kv_store_web.dart' as platform;

/// Best-effort string storage for UI preferences; every call may silently do nothing.
abstract class KvStore {
  factory KvStore() => platform.createKv();
  String? read(String key);
  void write(String key, String value);
}

class MemoryKv implements KvStore {
  final Map<String, String> _m = {};
  @override
  String? read(String key) => _m[key];
  @override
  void write(String key, String value) => _m[key] = value;
}
