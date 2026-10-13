import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/state/game_controller.dart';

import 'demo_world.dart';

/// A [GameController] logged in to the scripted [DemoWorld]: its messages
/// arrive as a connection's would, and what the controller sends is
/// answered on the spot.
class DemoController extends GameController {
  DemoWorld? _world;

  void startDemo() {
    _world?.stop();
    final world = _world = DemoWorld(_deliver);
    receive(const proto.AuthResult(success: true, playerId: 1));
    receive(world.initial());
    world.start();
  }

  void _deliver(List<proto.ServerMessage> messages) {
    for (final m in messages) {
      receive(m);
    }
  }

  /// Put a fleet of the scripted world at [at] with [cargo] aboard.
  void place(int fleetId, proto.Hex at, proto.Resources cargo) {
    final world = _world!;
    world.place(fleetId, at, cargo);
    receive(world.initial());
  }

  /// Give the scripted world's empire [tech] at [level].
  void learn(proto.ResearchType tech, int level) {
    final world = _world!;
    world.learn(tech, level);
    receive(world.initial());
  }

  /// Stand a building of the scripted world's empire at [level].
  void raise(proto.BuildingType building, int level) {
    final world = _world!;
    world.raise(building, level);
    receive(world.initial());
  }

  /// Put [stock] in the scripted world's stockpile.
  void stock(proto.Resources stock) {
    final world = _world!;
    world.stock(stock);
    receive(world.initial());
  }

  /// Announce a raid of [power] on the scripted world's outpost, arriving
  /// in [inTicks].
  void raidOutpost(double power, {int inTicks = 10}) => _world!.raidOutpost(power, inTicks: inTicks);

  /// Announce a raid of [power] on the scripted world's homeworld, arriving
  /// in [inTicks], in place of the one the script had on its way.
  void raidHome(double power, {int inTicks = 10}) => _world!.raidHome(power, inTicks: inTicks);

  /// Make what stands at the scripted homeworld that is no fleet [defences]
  /// and [satellites].
  void garrison(Map<proto.DefenceKind, int> defences, {int satellites = 0}) {
    final world = _world!;
    world.garrison(defences, satellites: satellites);
    receive(world.initial());
  }

  @override
  String? get account => 'demo';

  @override
  void send(proto.ClientMessage msg) {
    final world = _world;
    if (world != null) _deliver(world.handle(msg));
  }

  @override
  void stop() {
    _world?.stop();
    super.stop();
  }
}
