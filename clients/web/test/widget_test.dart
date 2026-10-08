import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/hex/hex_math.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/state/command_parser.dart';
import 'package:iac_client/state/connection_provider.dart';
import 'package:iac_client/state/demo_provider.dart';
import 'package:iac_client/state/state_mapper.dart';

void main() {
  test('hexToPixel returns flat-top offsets', () {
    final origin = hexToPixel(0, 0, 30);
    expect(origin.dx, 0);
    expect(origin.dy, 0);
    expect(hexToPixel(1, 0, 30).dx, 45); // 30 * 1.5
  });

  test('mulberry32 is deterministic', () {
    final a = mulberry32(42);
    final b = mulberry32(42);
    expect(a(), b());
    expect(a(), b());
  });

  test('zone names follow constants.rs thresholds', () {
    expect(const proto.Hex(0, 0).zone, 'Central Hub');
    expect(const proto.Hex(8, 0).zone, 'Inner Ring');
    expect(const proto.Hex(9, 0).zone, 'Outer Ring');
    expect(const proto.Hex(21, 0).zone, 'The Wandering');
  });

  group('ConnectParams', () {
    test('same origin as the page, /ws path', () {
      final p = ConnectParams.fromUri(Uri.parse('http://box.lan:8080/index.html?name=Ann&token=t'));
      expect(p.url, 'ws://box.lan:8080/ws');
      expect(p.name, 'Ann');
      expect(p.token, 't');
    });
    test('https uses wss and default port', () {
      expect(ConnectParams.fromUri(Uri.parse('https://iac.example/')).url, 'wss://iac.example:443/ws');
    });
    test('?ws= overrides', () {
      final p = ConnectParams.fromUri(Uri.parse('http://localhost:5000/?ws=ws://127.0.0.1:7801/x'));
      expect(p.url, 'ws://127.0.0.1:7801/x');
      expect(p.name, isNull);
    });
    test('non-http base falls back to localhost:7777', () {
      expect(ConnectParams.fromUri(Uri.parse('file:///x')).url, 'ws://127.0.0.1:7777/ws');
    });
  });

  group('demo + mapper + commands', () {
    late DemoProvider demo;
    late StateMapper mapper;
    setUp(() {
      demo = DemoProvider((msgs) => msgs.forEach(mapper.apply));
      mapper = StateMapper()..apply(demo.initial());
    });

    CommandContext ctx([int fleet = 0]) {
      final ui = mapper.toUiState(activeFleet: fleet);
      return CommandContext(
        fleetId: ui.fleets[fleet].id,
        fleetSector: ui.fleets[fleet].sector,
        sectors: ui.sectors,
        fleets: mapper.fleets,
        cursor: ui.fleets[fleet].sector,
        tick: mapper.tick,
      );
    }

    test('fleet ids come from state', () {
      final ui = mapper.toUiState();
      expect(ui.fleets.map((f) => f.id), [101, 102, 103]);
      final r = parseCommand('scan', ctx(1));
      expect(((r as ParsedSend).messages.single as proto.CommandMessage).command, isA<proto.ScanCommand>());
      expect((((r).messages.single as proto.CommandMessage).command as proto.ScanCommand).fleetId, 102);
    });

    test('move uses server-provided lanes and round-trips through the demo', () {
      final c = ctx();
      final here = c.here!;
      final target = here.connections.first;
      final r = parseCommand('move ${target.q} ${target.r}', c) as ParsedSend;
      final replies = demo.handle(r.messages.single);
      replies.forEach(mapper.apply);
      expect(mapper.fleets.firstWhere((f) => f.id == 101).location, target);
    });

    test('move to a non-lane is rejected client-side', () {
      final r = parseCommand('move 40 40', ctx());
      expect(r, isA<ParsedError>());
    });

    test('server error shows up in the log and alerts', () {
      mapper.apply(const proto.ErrorMessage(code: proto.ErrorCode.noConnection, message: 'No lane'));
      final ui = mapper.toUiState();
      expect(ui.events.first.message, contains('No lane'));
      expect(ui.alerts.first.message, 'No lane');
    });

    test('policy with no argument cycles to the next preset', () {
      final r = parseCommand('p', ctx(2)) as ParsedSend; // fleet 103 is on prospect
      expect((r.messages.single as proto.PolicyUpdate).preset, proto.PolicyPreset.mineAndReturn);
    });

    test('unknown commands report an error', () {
      expect(parseCommand('dance', ctx()), isA<ParsedError>());
    });

    test('a destroyed own fleet disappears', () {
      mapper.apply(const proto.GameEvent(
        tick: 5,
        kind: proto.FleetDestroyedEvent(fleetId: 101, isNpc: false, salvage: proto.Resources()),
      ));
      expect(mapper.fleets.map((f) => f.id), isNot(contains(101)));
    });
  });
}
