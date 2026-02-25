import '../models/game_state.dart';
import '../models/protocol.dart';

abstract class ConnectionProvider {
  bool get connected;
  Stream<GameState> get stateUpdates;
  void send(ClientMessage message);
  void connect(String url);
  void disconnect();
}

class StubConnectionProvider implements ConnectionProvider {
  @override
  bool get connected => false;

  @override
  Stream<GameState> get stateUpdates => const Stream.empty();

  @override
  void send(ClientMessage message) {}

  @override
  void connect(String url) {}

  @override
  void disconnect() {}
}
