// Relays on the console: the founding survey and its order at a recorded
// site, and the panel of a relay that stands there with its store, its
// decay and the deuterium moved to and from the hold. The figures are the
// states the Rust side recorded (fixtures/protocol/server_message).

import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/services.dart';
import 'package:iac_client/design/panel.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;

import 'fixtures_util.dart';
import 'support/demo_controller.dart';
import 'support/harness.dart';

const _desktop = Size(1440, 900);

Map<String, dynamic> _recorded(String name) =>
    readJson(File('${fixturesDir().path}/protocol/server_message/$name.json')) as Map<String, dynamic>;

/// Logged in to a recorded world; keeps what it is asked to send instead of
/// sending it, and takes further recorded messages.
class _Recorded extends DemoController {
  final sent = <proto.ClientMessage>[];

  _Recorded(Map<String, dynamic> fullState) {
    receive(const proto.AuthResult(success: true, playerId: 42));
    feed(fullState);
  }

  void feed(Map<String, dynamic> json) => receive(proto.ServerMessage.fromJson(json));

  Iterable<proto.Command> get commands => sent.whereType<proto.CommandMessage>().map((m) => m.command);

  @override
  void send(proto.ClientMessage msg) => sent.add(msg);
}

Future<_Recorded> _open(WidgetTester tester, Map<String, dynamic> state) async {
  final c = _Recorded(state);
  addTearDown(c.dispose);
  await pumpShell(tester, size: _desktop, screen: Screen.windshield, controller: c);
  return c;
}

Future<void> _press(WidgetTester tester, String key) async {
  final target = find.byKey(ValueKey(key));
  await tester.ensureVisible(target);
  await tester.pump(const Duration(milliseconds: 50));
  await tester.tap(target);
  await tester.pump(const Duration(milliseconds: 100));
}

/// The recorded state with no outpost, fleet 7 standing in ring 10 and the
/// relays of the recorded survey: none, and the limit.
Map<String, dynamic> _bare() {
  final json = _recorded('full_state__with_outpost')..remove('outpost');
  for (final s in json['known_sectors'] as List) {
    (s as Map<String, dynamic>).remove('outpost');
  }
  for (final f in json['fleets'] as List) {
    (f as Map<String, dynamic>)
      ..remove('outpost_fuel')
      ..['at_outpost'] = false;
  }
  json['relays'] = {'limit': 1, 'technology': 'HyperspaceTechnology', 'level': 0, 'fill_jumps': 10, 'relays': []};
  return json;
}

void main() {
  testWidgets('Y opens the founding survey of the sector; the order waits for the server to refuse nothing and then founds', (tester) async {
    final c = await _open(tester, _bare());
    final fleet = c.currentFleet;
    await tester.sendKeyEvent(LogicalKeyboardKey.keyY, character: 'y');
    await tester.pump(const Duration(milliseconds: 50));
    expect(find.byKey(const ValueKey('relay-panel')), findsOneWidget);
    expect(textUnder(tester, find.byKey(const Key('relay-count'))), contains('0 OF 1 - HYPERSPACE TECHNOLOGY L0'));
    expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('relay-found'))).onPressed, isNull, reason: 'no survey has come yet');

    await tester.pump(const Duration(seconds: 1));
    final asked = c.commands.whereType<proto.RelaySurveyCommand>().single;
    expect(asked.fleetId, fleet.id);

    final refused = _recorded('relay_survey__refused')
      ..['fleet_id'] = fleet.id
      ..['sector'] = fleet.sector.toJson();
    c.feed(refused);
    await tester.pump(const Duration(milliseconds: 100));
    expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('relay-found'))).onPressed, isNull);
    expect(find.byKey(const Key('relay-found-why')), findsOneWidget);
    await _press(tester, 'relay-found');
    expect(c.commands.whereType<proto.FoundRelayCommand>(), isEmpty, reason: 'nothing is ordered against the server\'s refusal');

    final clear = _recorded('relay_survey')
      ..['fleet_id'] = fleet.id
      ..['sector'] = fleet.sector.toJson();
    c.feed(clear);
    await tester.pump(const Duration(milliseconds: 100));
    await _press(tester, 'relay-found');
    expect(c.commands.whereType<proto.FoundRelayCommand>().single.fleetId, fleet.id);
  });

  testWidgets('where a relay stands the panel shows its store and how long it has, and Load and Unload move deuterium', (tester) async {
    final json = _bare();
    final fleet = (json['fleets'] as List).first as Map<String, dynamic>;
    fleet['relay'] = {'load': 300.0, 'unload': 300.0};
    fleet['cargo'] = {'metal': 0.0, 'crystal': 0.0, 'deuterium': 300.0};
    fleet['cargo_capacity'] = 5000.0;
    json['relays'] = {
      'limit': 3,
      'technology': 'HyperspaceTechnology',
      'level': 4,
      'fill_jumps': 22,
      'relays': [
        {
          'sector': fleet['location'],
          'ring': 10,
          'store': 640.0,
          'decay_in': 15000,
          'founded_tick': 900,
        },
      ],
    };
    final c = await _open(tester, json);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyY, character: 'y');
    await tester.pump(const Duration(milliseconds: 50));
    expect(textUnder(tester, find.byKey(const Key('relay-count'))), contains('1 OF 3 - HYPERSPACE TECHNOLOGY L4'));
    expect(textUnder(tester, find.byKey(const Key('relay-store'))), contains('640 OF 800 DEUTERIUM'));
    expect(textUnder(tester, find.byKey(const Key('relay-decay'))), contains('4:10:00'));
    await _press(tester, 'relay-load');
    expect(c.commands.whereType<proto.LoadCommand>().single.resource, proto.ResourceKind.deuterium);
    await _press(tester, 'relay-unload');
    expect(c.commands.whereType<proto.UnloadCommand>(), hasLength(1));
  });
}
