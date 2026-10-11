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

  /// Announce a raid of [power] on the scripted world's outpost, arriving
  /// in [inTicks].
  void raidOutpost(double power, {int inTicks = 10}) => _world!.raidOutpost(power, inTicks: inTicks);

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
