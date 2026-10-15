// Roaming hubs on the console: a hub the server lists is a place a fleet
// trades and refuels, with the server's offers and its refusal, and one it
// marks as left is neither. The figures are the states the Rust side
// recorded (fixtures/protocol/server_message) with a hub put in them.

import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/services.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;

import 'fixtures_util.dart';
import 'support/demo_controller.dart';
import 'support/harness.dart';

const _desktop = Size(1440, 900);

Map<String, dynamic> _recorded(String name) =>
    readJson(File('${fixturesDir().path}/protocol/server_message/$name.json')) as Map<String, dynamic>;

class _Recorded extends DemoController {
  final sent = <proto.ClientMessage>[];

  _Recorded(Map<String, dynamic> fullState) {
    receive(const proto.AuthResult(success: true, playerId: 42));
    receive(proto.ServerMessage.fromJson(fullState));
  }

  Iterable<proto.Command> get commands => sent.whereType<proto.CommandMessage>().map((m) => m.command);

  @override
  void send(proto.ClientMessage msg) => sent.add(msg);
}

/// The recorded state with its first fleet standing in a hub's sector, the
/// hub listed (left or not) and the fleet carrying the server's refuel quote.
Map<String, dynamic> _at({required bool stale, String? refusal}) {
  final json = _recorded('full_state__with_outpost');
  final fleet = (json['fleets'] as List).first as Map<String, dynamic>;
  fleet['cargo'] = {'metal': 300.0, 'crystal': 0.0, 'deuterium': 40.0};
  fleet['refuel'] = {
    'missing': 540.0,
    'offers': [
      {'give': 'metal', 'amount': 300.0, 'fuel': 540.0, 'fee': 0.25},
      {'give': 'deuterium', 'amount': 40.0, 'fuel': 500.0, 'fee': 0.0},
    ],
    'refusal': ?refusal,
  };
  json['hubs'] = [
    {
      'id': 1,
      'sector': fleet['location'],
      'ring': 30,
      'services': ['exchange', 'refuel'],
      'departs_at': 90000,
      'departing': false,
      'stale': stale,
    },
  ];
  return json;
}

void main() {
  testWidgets('a hub the server lists is an exchange, and its refuel offers are the server\'s', (tester) async {
    final c = _Recorded(_at(stale: false));
    addTearDown(c.dispose);
    await pumpShell(tester, size: _desktop, screen: Screen.windshield, controller: c);
    expect(c.state.exchangeAt(c.currentFleet.sector), isTrue);

    await tester.sendKeyEvent(LogicalKeyboardKey.keyT, character: 't');
    await tester.pump(const Duration(milliseconds: 100));
    expect(textUnder(tester, find.byKey(const Key('refuel-offer-metal'))), contains('300 Fe BUYS 540 FUEL - FEE 25%'));
    expect(textUnder(tester, find.byKey(const Key('refuel-offer-deuterium'))), contains('40 De BUYS 500 FUEL'));

    final target = find.byKey(const ValueKey('refuel-metal'));
    await tester.ensureVisible(target);
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tap(target);
    await tester.pump(const Duration(milliseconds: 100));
    final order = c.commands.whereType<proto.RefuelCommand>().single;
    expect((order.fleetId, order.give, order.amount), (c.currentFleet.id, proto.ResourceKind.metal, null));
    await tester.pump(const Duration(seconds: 1));
  });

  testWidgets('a hub the server marks as left is not a place to trade', (tester) async {
    final json = _at(stale: true);
    (json['fleets'] as List).cast<Map<String, dynamic>>().first.remove('refuel');
    final c = _Recorded(json);
    addTearDown(c.dispose);
    await pumpShell(tester, size: _desktop, screen: Screen.windshield, controller: c);
    expect(c.state.hubs.single.stale, isTrue);
    expect(c.state.exchangeAt(c.currentFleet.sector), isFalse);
    expect(c.currentFleet.refuel, isNull);
  });
}
