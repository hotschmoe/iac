// Login flow against a fake server that follows the Rust server's rules:
// unknown name registers and returns a token once, known name needs it.

import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/state/connection_provider.dart';
import 'package:iac_client/state/game_controller.dart';
import 'package:iac_client/state/token_store.dart';

import 'support/fake_server.dart';

void main() {
  late FakeServer server;
  setUp(() async {
    server = FakeServer();
    await server.start();
  });
  tearDown(() => server.stop());

  GameController controller(TokenStore tokens) {
    final c = GameController(tokens: tokens);
    addTearDown(() {
      c.stop();
      c.dispose();
    });
    return c;
  }

  test('first login registers, shows and remembers the token; the next login sends it', () async {
    final tokens = MemoryTokenStore();
    final params = ConnectParams(url: server.url);

    final first = controller(tokens);
    await first.start(params: params, name: 'Admiral');
    await until(first, () => first.isLive);
    expect(server.authRequests.single.containsKey('token'), isFalse);
    expect(tokens.load(server.url, 'Admiral'), issuedToken);
    expect(first.authRejection, isNull);
    expect(first.state.alerts.any((a) => a.message == 'New account token' && a.detail == issuedToken), isTrue);
    first.stop();

    final second = controller(tokens);
    await second.start(params: params, name: 'Admiral');
    await until(second, () => second.isLive);
    expect(server.authRequests.last['token'], issuedToken);
    expect(second.state.alerts.any((a) => a.message == 'New account token'), isFalse, reason: 'token is shown once');
  });

  test('a known name without a token is sent back to the login form, and a typed token fixes it', () async {
    server.accounts['Taken'] = issuedToken;
    final c = controller(MemoryTokenStore());
    await c.start(params: ConnectParams(url: server.url), name: 'Taken');
    await until(c, () => c.authRejection != null);
    expect(c.authRejection!.needsToken, isTrue);
    expect(c.authRejection!.message, contains('already registered'));
    expect(c.isLive, isFalse);

    await c.start(params: ConnectParams(url: server.url), name: 'Taken', token: issuedToken);
    await until(c, () => c.isLive);
    expect(c.authRejection, isNull);
  });

  test('a wrong remembered token is rejected as invalid, and not retried forever', () async {
    server.accounts['Taken'] = issuedToken;
    final tokens = MemoryTokenStore()..save(server.url, 'Taken', 'f' * 64);
    final c = controller(tokens);
    await c.start(params: ConnectParams(url: server.url), name: 'Taken');
    await until(c, () => c.authRejection != null);
    expect(c.authRejection!.code, proto.ErrorCode.invalidToken);
    await Future<void>.delayed(const Duration(milliseconds: 300));
    expect(server.authRequests, hasLength(1));
  });

  test('?token= is honoured and then remembered', () async {
    server.accounts['Agent'] = issuedToken;
    final tokens = MemoryTokenStore();
    final c = controller(tokens);
    await c.start(params: ConnectParams(url: server.url, token: issuedToken), name: 'Agent');
    await until(c, () => c.isLive);
    expect(tokens.load(server.url, 'Agent'), issuedToken);
  });

  test('a full world sends a new name back to the login form with the reason, and still admits its settlers', () async {
    server.accounts['Settler'] = issuedToken;
    server.seats = 1;
    final c = controller(MemoryTokenStore());
    await c.start(params: ConnectParams(url: server.url), name: 'Latecomer');
    await until(c, () => c.authRejection != null);
    expect(c.authRejection!.code, proto.ErrorCode.worldFull);
    expect(c.authRejection!.needsToken, isFalse);
    expect(c.authRejection!.message, contains('world is full'));
    expect(c.isLive, isFalse);
    await Future<void>.delayed(const Duration(milliseconds: 300));
    expect(server.authRequests, hasLength(1));

    await c.start(params: ConnectParams(url: server.url), name: 'Settler', token: issuedToken);
    await until(c, () => c.isLive);
    expect(c.authRejection, isNull);
  });
}
