import 'token_store_stub.dart' if (dart.library.js_interop) 'token_store_web.dart' as platform;

/// Remembers the account token the server issued, per server and player name,
/// so a returning player is logged in without typing it.
abstract class TokenStore {
  /// The browser's localStorage on web, memory elsewhere.
  factory TokenStore.platform() => platform.createTokenStore();

  String? load(String server, String name);

  /// Returns whether the token will outlive this page (false when storage is
  /// unavailable, in which case the player must keep a copy themselves).
  bool save(String server, String name, String token);
}

/// Process-lifetime store; the fallback when real storage is unavailable.
class MemoryTokenStore implements TokenStore {
  final Map<String, String> _tokens = {};

  @override
  String? load(String server, String name) => _tokens[tokenKey(server, name)];

  @override
  bool save(String server, String name, String token) {
    _tokens[tokenKey(server, name)] = token;
    return true;
  }
}

String tokenKey(String server, String name) => 'iac.token|$server|$name';

/// Minimal string storage, implemented over `window.localStorage` on web.
abstract interface class KeyValueStorage {
  String? read(String key);
  void write(String key, String value);
}

/// A token store over [KeyValueStorage] that treats every storage access as
/// fallible (private windows, blocked site data, quota): failures are
/// swallowed and a per-session memory copy keeps the current page working.
class GuardedTokenStore implements TokenStore {
  GuardedTokenStore(this._open);

  /// Opened lazily and inside try/catch: even obtaining `localStorage` can throw.
  final KeyValueStorage Function() _open;
  final MemoryTokenStore _session = MemoryTokenStore();

  @override
  String? load(String server, String name) {
    try {
      final stored = _open().read(tokenKey(server, name));
      if (stored != null && stored.isNotEmpty) return stored;
    } catch (_) {
      // fall through to the session copy
    }
    return _session.load(server, name);
  }

  @override
  bool save(String server, String name, String token) {
    _session.save(server, name, token);
    try {
      _open().write(tokenKey(server, name), token);
      return true;
    } catch (_) {
      return false;
    }
  }
}
