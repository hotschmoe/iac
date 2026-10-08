// Split and merge through the windshield panel, the command bar and the
// protocol: wire shapes, the parser, and the panel buttons against the demo.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/state/command_parser.dart';
import 'package:iac_client/state/game_controller.dart';
import 'package:iac_client/theme/amber_theme.dart';
import 'package:iac_client/views/shell.dart';

proto.FleetState _fleet(int id, proto.Hex at, List<proto.ShipClass> classes) => proto.FleetState(
      id: id,
      location: at,
      state: proto.FleetStatus.idle,
      ships: [
        for (var i = 0; i < classes.length; i++)
          proto.ShipState(
            id: id * 100 + i,
            shipClass: classes[i],
            hull: 10,
            hullMax: 10,
            shield: 0,
            shieldMax: 0,
            weaponPower: 1,
          ),
      ],
      cargo: const proto.Resources(),
      cargoCapacity: 10,
      fuel: 5,
      fuelMax: 5,
    );

CommandContext _ctx(List<proto.FleetState> fleets, int active) => CommandContext(
      fleetId: active,
      fleetSector: fleets.firstWhere((f) => f.id == active).location,
      sectors: const {},
      fleets: fleets,
      cursor: proto.Hex.origin,
      tick: 1,
    );

void main() {
  setUpAll(() => GoogleFonts.config.allowRuntimeFetching = false);

  group('wire', () {
    test('split and merge round-trip', () {
      const split = proto.SplitCommand(fleetId: 3, shipIds: [7, 8]);
      expect(split.toJson(), {'action': 'split', 'fleet_id': 3, 'ship_ids': [7, 8]});
      expect((proto.Command.fromJson(split.toJson()) as proto.SplitCommand).shipIds, [7, 8]);
      const merge = proto.MergeCommand(fleetId: 3, otherFleetId: 4);
      expect(proto.Command.fromJson(merge.toJson()).toJson(), merge.toJson());
    });

    test('partial policy params take the server defaults', () {
      final p = proto.PolicyParams.fromJson({'max_range': 2});
      expect(p.maxRange, 2);
      expect(p.minFuelPct, 10);
      expect(p.cargoReturnPct, 85);
      expect(p.engageRatioX10, 12);
    });
  });

  group('command bar', () {
    final here = proto.Hex(2, 1);
    final fleets = [
      _fleet(1, here, [proto.ShipClass.scout, proto.ShipClass.scout, proto.ShipClass.hauler]),
      _fleet(2, here, [proto.ShipClass.corvette]),
      _fleet(3, proto.Hex(0, 0), [proto.ShipClass.corvette]),
    ];

    test('split picks ships of the class by id', () {
      final r = parseCommand('split scout 1', _ctx(fleets, 1)) as ParsedSend;
      final cmd = (r.messages.single as proto.CommandMessage).command as proto.SplitCommand;
      expect(cmd.fleetId, 1);
      expect(cmd.shipIds, [100]);
    });

    test('split refuses to take everything or ships that are not aboard', () {
      expect(parseCommand('split scout 5', _ctx(fleets, 1)), isA<ParsedError>());
      expect(parseCommand('split frigate', _ctx(fleets, 1)), isA<ParsedError>());
      expect(parseCommand('split corvette', _ctx(fleets, 2)), isA<ParsedError>());
    });

    test('merge only offers fleets in the same sector', () {
      final r = parseCommand('merge', _ctx(fleets, 1)) as ParsedSend;
      final cmd = (r.messages.single as proto.CommandMessage).command as proto.MergeCommand;
      expect((cmd.fleetId, cmd.otherFleetId), (1, 2));
      expect(parseCommand('merge 3', _ctx(fleets, 1)), isA<ParsedError>());
    });
  });

  testWidgets('the windshield panel splits a ship off and merges it back', (tester) async {
    tester.view.physicalSize = const Size(1400, 1000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final c = GameController();
    addTearDown(() {
      c.stop();
      c.dispose();
    });
    c.startDemo();
    await tester.pumpWidget(MaterialApp(
      theme: Amber.themeData(),
      home: Scaffold(body: Shell(controller: c)),
    ));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.enterText(find.byType(TextField), 'ws');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump(const Duration(milliseconds: 300));

    final before = c.state.fleets.length;
    final active = c.activeFleetId!;
    expect(c.currentFleet.shipCount, 3);

    await tester.tap(find.byKey(const ValueKey('fm-plus-Hauler')));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('fm-split')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(c.state.fleets.length, before + 1);
    expect(c.currentFleet.id, active);
    expect(c.currentFleet.shipCount, 2);
    final newId = c.mergeCandidates.single.id;
    expect(c.state.events.any((e) => e.message.contains('split')), isTrue);

    await tester.tap(find.byKey(ValueKey('fm-merge-$newId')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(c.state.fleets.length, before);
    expect(c.currentFleet.shipCount, 3);
    expect(tester.takeException(), isNull);
    c.stop();
  });
}
