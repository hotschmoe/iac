// What an order lacks, and why a queued one has not started, must fit the
// box it is printed in. The world here is broke and everything costs
// 1,234,567 of each resource, so every shortfall names all three with seven
// digits; the screens are pumped at desktop width and at a phone's.

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/services.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/state/game_controller.dart';

import 'support/demo_controller.dart';
import 'support/demo_world.dart';
import 'support/harness.dart';

const _big = 1234567;
const _desktop = Size(1440, 900);
const _phone = Size(390, 844);
const _shortBy = 'Short by 1,234,567 Fe, 1,234,567 Cr, 1,234,567 De';
const _waitingShort = 'short by 1,234,567 Fe, 1,234,567 Cr, 1,234,567 De, starts in about 1d 10:17:36';

Map<String, dynamic> _cost([int times = 1]) => {'metal': _big * times, 'crystal': _big * times, 'deuterium': _big * times};

/// The demo world with an empty stockpile, idle slots, and one order of each
/// kind waiting: a building and a tech short of everything, a paid batch
/// held up by a prerequisite.
proto.ServerMessage _brokeWorld() {
  final json = DemoWorld((_) {}).initial().toJson();
  (json['player'] as Map<String, dynamic>)['resources'] = {'metal': 0, 'crystal': 0, 'deuterium': 0};
  final home = json['homeworld'] as Map<String, dynamic>;
  home
    ..['build_queue'] = <dynamic>[]
    ..['build_pending'] = [
      {
        'id': 901, 'building_type': 'DeuteriumSynthesizer', 'target_level': 12, 'reserve': true,
        'cost': _cost(), 'ticks': 86400, 'waiting_for': _cost(), 'waiting_on': 'Resources', 'start_in': 123456,
      },
      {
        'id': 902, 'building_type': 'DeuteriumTank', 'target_level': 9, 'reserve': false,
        'cost': _cost(), 'ticks': 86400, 'waiting_for': _cost(), 'waiting_on': 'Order', 'start_in': 123456,
      },
    ]
    ..remove('research_active')
    ..['research_pending'] = [
      {
        'id': 903, 'tech': 'HyperspaceTechnology', 'target_level': 5, 'reserve': false,
        'cost': _cost(), 'ticks': 86400, 'waiting_for': _cost(), 'waiting_on': 'Resources', 'start_in': 123456,
      },
    ]
    ..remove('shipyard_queue')
    ..['shipyard_pending'] = [
      {
        'id': 904, 'item': 'RocketLauncher', 'count': 1000,
        'cost': _cost(1000), 'unit_cost': _cost(), 'ticks': 86400, 'waiting_on': 'Prerequisite',
      },
    ];
  final catalog = home['catalog'] as Map<String, dynamic>;
  for (final o in [...catalog['buildings'] as List, ...catalog['research'] as List]) {
    final next = (o as Map<String, dynamic>)['next'] as Map<String, dynamic>?;
    if (next != null) next['cost'] = _cost();
  }
  for (final o in [...catalog['ships'] as List, ...catalog['defences'] as List]) {
    (o as Map<String, dynamic>)['unit_cost'] = _cost();
    for (final r in o['requires'] as List) {
      (r as Map<String, dynamic>)
        ..['have'] = r['need']
        ..['met'] = true;
    }
  }
  return proto.ServerMessage.fromJson(json);
}

/// Logged in to the broke world; keeps what it is asked to send instead of
/// sending it.
class _Broke extends DemoController {
  final sent = <proto.ClientMessage>[];

  _Broke() {
    receive(const proto.AuthResult(success: true, playerId: 1));
    receive(_brokeWorld());
  }

  Iterable<proto.Command> get orders => sent.whereType<proto.CommandMessage>().map((m) => m.command).where(
      (c) => c is proto.BuildCommand || c is proto.ResearchCommand || c is proto.BuildShipCommand || c is proto.BuildDefenceCommand);

  @override
  void send(proto.ClientMessage msg) => sent.add(msg);
}

Future<_Broke> _open(WidgetTester tester, Size size, Screen screen) async {
  final c = _Broke();
  addTearDown(c.dispose);
  await pumpShell(tester, size: size, screen: screen, controller: c);
  return c;
}

Future<void> _show(WidgetTester tester, Finder target) async {
  await tester.ensureVisible(target);
  await tester.pump(const Duration(milliseconds: 100));
}

void main() {
  for (final size in [_desktop, _phone]) {
    final at = '${size.width.round()}px';

    testWidgets('homeworld at $at: the queue boxes hold a waiting order and all it is short of', (tester) async {
      final c = await _open(tester, size, Screen.homeworld);
      expect(tester.takeException(), isNull);

      for (final (queue, order, waits) in [
        ('building', '${proto.BuildingType.deuteriumSynthesizer.label} Lv.12', _waitingShort),
        ('research', '${proto.ResearchType.hyperspaceTechnology.label} Lv.5', _waitingShort),
        ('ship', '${proto.DefenceKind.rocketLauncher.label} x1000', 'paid, waiting for a prerequisite'),
      ]) {
        final pill = find.byKey(Key('hw-pill-$queue'));
        expect(find.descendant(of: pill, matching: find.textContaining('${order.toUpperCase()}  $waits')), findsOneWidget,
            reason: 'the $queue box names its waiting order and why it waits');
        expectTextFits(tester, pill, reason: '$queue box');
      }
      expectTextFits(tester, find.byKey(const Key('hw-pill-slot-b')), reason: 'locked slot box');

      for (final tab in HomeworldTab.values) {
        c.selectHomeworldTab(tab);
        await tester.pump(const Duration(milliseconds: 150));
        expect(tester.takeException(), isNull, reason: '${tab.name} tab');
        await shot(tester, 'fit-homeworld-${tab.name}-${size.width.round()}');
      }
      await endShell(tester, c);
    });

    testWidgets('homeworld at $at: a batch the stockpile does not cover cannot be ordered, and the control says what is missing', (tester) async {
      final c = await _open(tester, size, Screen.homeworld);
      c.selectHomeworldTab(HomeworldTab.shipyard);
      await tester.pump(const Duration(milliseconds: 150));

      final card = find.byKey(const ValueKey('hw-card-0'));
      await _show(tester, card);
      await tester.tap(card);
      await tester.pump();
      expect(find.descendant(of: card, matching: find.text(_shortBy)), findsOneWidget, reason: 'the ship card names the shortfall');
      expectTextFits(tester, card, reason: 'ship card');

      final order = find.byKey(const ValueKey('hw-queue'));
      await _show(tester, order);
      expectTextFits(tester, find.byKey(const Key('hw-inspector')), reason: 'inspector');
      expect(find.descendant(of: find.byKey(const Key('hw-inspector')), matching: find.text(_shortBy)), findsOneWidget);
      await tester.tap(order, warnIfMissed: false);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      expect(c.orders, isEmpty, reason: 'neither the button nor Enter sends a batch that is not covered');
      await shot(tester, 'fit-homeworld-shipyard-short-${size.width.round()}');

      await tester.tap(find.byKey(const ValueKey('hw-batch-10')));
      await tester.pump();
      const ten = 'Short by 12,345,670 Fe, 12,345,670 Cr, 12,345,670 De';
      expect(find.descendant(of: find.byKey(const Key('hw-inspector')), matching: find.text(ten)), findsOneWidget, reason: 'the check is the whole batch at the chosen count');
      expectTextFits(tester, find.byKey(const Key('hw-inspector')), reason: 'inspector, batch of ten');
      expect(tester.takeException(), isNull);

      c.selectHomeworldTab(HomeworldTab.defence);
      await tester.pump(const Duration(milliseconds: 150));
      final turret = find.byKey(const ValueKey('hw-card-0'));
      await _show(tester, turret);
      expect(find.descendant(of: turret, matching: find.text(ten)), findsOneWidget, reason: 'a defence batch is a shipyard batch');
      expectTextFits(tester, turret, reason: 'defence row');
      await tester.tap(find.descendant(of: turret, matching: find.text('BUILD X10')), warnIfMissed: false);
      await tester.pump();
      expect(c.orders, isEmpty);
      expect(tester.takeException(), isNull);
      await endShell(tester, c);
    });

    testWidgets('overview at $at: order tiles and queue rows hold all three shortfalls', (tester) async {
      final c = await _open(tester, size, Screen.overview);
      expect(tester.takeException(), isNull);

      for (final card in ['card-slot-a', 'card-research', 'card-shipyard']) {
        final panel = find.byKey(Key(card));
        await _show(tester, panel);
        expectTextFits(tester, panel, reason: card);
        await shot(tester, 'fit-overview-$card-${size.width.round()}');
      }
      expect(find.descendant(of: find.byKey(const Key('card-slot-a')), matching: find.text(_waitingShort)), findsOneWidget);
      expect(find.descendant(of: find.byKey(const Key('card-shipyard')), matching: find.textContaining('paid, waiting for a prerequisite')), findsOneWidget);

      final ship = find.byKey(Key('sug-ship-${proto.ShipClass.values.first.name}'));
      await _show(tester, ship);
      expect(find.descendant(of: ship, matching: find.text(_shortBy)), findsOneWidget, reason: 'the ship tile names the shortfall');
      await tester.tap(ship, warnIfMissed: false);
      await tester.pump();
      expect(c.orders, isEmpty, reason: 'a ship tile the stockpile does not cover sends nothing');
      expect(tester.takeException(), isNull);
      await endShell(tester, c);
    });
  }
}
