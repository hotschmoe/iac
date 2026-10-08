import 'package:web/web.dart' as web;

import 'token_store.dart';

TokenStore createTokenStore() => GuardedTokenStore(() => _LocalStorage());

class _LocalStorage implements KeyValueStorage {
  @override
  String? read(String key) => web.window.localStorage.getItem(key);

  @override
  void write(String key, String value) => web.window.localStorage.setItem(key, value);
}
