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

    test('a tick update replaces the fleet list, and an empty list means all fleets are lost', () {
      final survivors = mapper.fleets.where((f) => f.id != 101).toList();
      mapper.apply(proto.TickUpdate(tick: 6, fleets: survivors));
      expect(mapper.fleets.map((f) => f.id), isNot(contains(101)));
      expect(mapper.fleets, hasLength(survivors.length));

      mapper.apply(const proto.TickUpdate(tick: 7, fleets: []));
      expect(mapper.fleets, isEmpty);
      expect(mapper.toUiState().fleets, isEmpty);
    });

    test('cargo capacity and production rates come straight from the server', () {
      final base = mapper.fleets.first;
      final odd = proto.FleetState(
        id: base.id,
        location: base.location,
        state: base.state,
        ships: base.ships,
        cargo: const proto.Resources(metal: 10),
        cargoCapacity: 40,
        fuel: base.fuel,
        fuelMax: base.fuelMax,
      );
      final hw = mapper.homeworld!;
      mapper.apply(proto.TickUpdate(
        tick: 8,
        fleets: [odd],
        homeworldUpdate: proto.HomeworldState(
          location: hw.location,
          production: const proto.Resources(metal: 7.5, crystal: 3.25, deuterium: 0.5),
          buildings: hw.buildings,
          research: hw.research,
          dockedShips: hw.dockedShips,
          catalog: hw.catalog,
        ),
      ));
      final ui = mapper.toUiState();
      expect(ui.fleets.single.cargo.capacity, 40);
      expect(ui.fleets.single.cargoPercent, 25);
      expect(ui.resources.metal.rate, 7.5);
      expect(ui.resources.crystal.rate, 3.25);
      expect(ui.resources.deut.rate, 0.5);
    });

    test('a FleetDestroyed event is only logged; the fleet list comes from the tick', () {
      final before = mapper.fleets.length;
      mapper.apply(const proto.GameEvent(
        tick: 5,
        kind: proto.FleetDestroyedEvent(
          fleetId: 101,
          isNpc: false,
          sector: proto.Hex(2, 1),
          owner: 'Admiral',
          mine: true,
          salvage: proto.Resources(),
        ),
      ));
      expect(mapper.fleets, hasLength(before));
      final entry = mapper.toUiState().events.first;
      expect(entry.message, contains('YOUR FLEET'));
      expect(entry.message, contains('101'));
    });

    test('other empires and hostiles are told apart from your own losses', () {
      mapper.apply(const proto.GameEvent(
        tick: 6,
        kind: proto.FleetDestroyedEvent(
          fleetId: 55,
          isNpc: false,
          sector: proto.Hex(2, 1),
          owner: 'Rival',
          mine: false,
          salvage: proto.Resources(),
        ),
      ));
      expect(mapper.toUiState().events.first.message, contains("Rival's fleet"));
      mapper.apply(const proto.GameEvent(
        tick: 7,
        kind: proto.FleetDestroyedEvent(
          fleetId: 9001,
          isNpc: true,
          sector: proto.Hex(2, 1),
          owner: null,
          mine: false,
          salvage: proto.Resources(metal: 60, crystal: 15, deuterium: 9),
        ),
      ));
      expect(mapper.toUiState().events.first.message, contains('wreckage 60M 15C 9D'));
    });

    test('a critical alert from the server surfaces as a glowing alert', () {
      mapper.apply(const proto.GameEvent(
        tick: 8,
        kind: proto.AlertEvent(
          playerId: 1,
          level: proto.AlertLevel.critical,
          message: 'FLEET LOST: fleet 2 destroyed',
          fleetId: 2,
        ),
      ));
      expect(mapper.toUiState().alerts.first.message, contains('FLEET LOST'));
    });

    test('stale sectors are kept from full_state and reported with their age', () {
      const where = proto.Hex(3, -2);
      final stale = proto.SectorState(
        location: where,
        terrain: proto.TerrainType.asteroidField,
        resources: const proto.SectorResources(
            metal: proto.Density.rich, crystal: proto.Density.none, deuterium: proto.Density.none),
        connections: const [],
        salvage: const proto.Resources(metal: 60),
        salvageDespawnTick: 100,
        lastSeen: 40,
        live: false,
      );
      mapper.apply(proto.GameState(
        tick: 400,
        player: mapper.player!,
        fleets: const [],
        homeworld: mapper.homeworld!,
        knownSectors: [stale],
      ));
      expect(mapper.sectors[where.toKey()]!.live, isFalse);
      final info = mapper.toUiState().sectors[where]!;
      expect(info.lastSeen, 40);

      mapper.apply(const proto.TickUpdate(tick: 401, fleets: []));
      expect(mapper.sectors[where.toKey()]!.live, isFalse); // a tick without it leaves the memory alone

      mapper.apply(proto.TickUpdate(tick: 402, fleets: const [], sectorUpdates: [
        proto.SectorState(
          location: where,
          terrain: stale.terrain,
          resources: stale.resources,
          connections: const [],
          lastSeen: 402,
          live: true,
        ),
      ]));
      expect(mapper.sectors[where.toKey()]!.live, isTrue);
    });
  });
}
