// Outposts on the console: the founding survey and the order, the outpost's
// panel with its dock, the typed commands, the map's mark and inspector.
// The figures come from the states the Rust side recorded
// (fixtures/protocol/server_message) and the flows from the scripted world,
// which founds, builds, is raided and repairs in seconds.

import 'dart:io';
import 'dart:ui' show PointerDeviceKind;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/battle.dart' show RaidEnding, shipsList;
import 'package:iac_client/console/intel.dart' show groupDigits;
import 'package:iac_client/console/services.dart';
import 'package:iac_client/design/panel.dart';
import 'package:iac_client/models/fleet.dart' show FleetStatus;
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/scenes/battle/field.dart' show Formation;
import 'package:iac_client/scenes/windshield/painter.dart' show ArenaPainter;
import 'package:iac_client/views/overlays.dart' show ToastLayer;
import 'package:iac_client/views/rail.dart';
import 'package:iac_client/views/windshield/parts.dart' show FleetRail, GateCard;

import 'fixtures_util.dart';
import 'map_test.dart' show at, painterOf;
import 'support/demo_controller.dart';
import 'support/harness.dart';

const _desktop = Size(1440, 900);
const _phone = Size(390, 844);

/// The narrowest window that still gets the windshield's desktop layout.
const _least = Size(1167, 800);

/// A site of the scripted world an outpost can be founded on, one a group
/// holds, the scripted homeworld, at ring 4, and what level 1 costs there.
const _site = proto.Hex(10, 0);
const _held = proto.Hex(10, -3);
const _demoHome = proto.Hex(4, -2);
const _levelOne = proto.Resources(metal: 3000, crystal: 1500, deuterium: 600);

Map<String, dynamic> _recorded(String name) =>
    readJson(File('${fixturesDir().path}/protocol/server_message/$name.json')) as Map<String, dynamic>;

Map<String, dynamic> _each(num v) => {'metal': v, 'crystal': v, 'deuterium': v};

const _bigRefusal = 'outpost level 3 costs 23456789 metal, 23456789 crystal and 23456789 deuterium; the store at [10,-2] is short by 22222222 metal, '
    '22222222 crystal and 22222222 deuterium. A level is paid in full from the outpost\'s own store when it is ordered';

/// A platform of three kinds of defence, each by the million.
const _bigPlatform = [
  {'unit': 'RocketLauncher', 'count': 1234567},
  {'unit': 'LightLaser', 'count': 2345678},
  {'unit': 'HeavyLaser', 'count': 3456789},
];

/// The recorded state with an outpost, every figure of the outpost at seven
/// digits and more, and the longest sentence an upgrade is refused with.
Map<String, dynamic> _bigOutpost() {
  final json = _recorded('full_state__with_outpost');
  (json['outpost'] as Map<String, dynamic>)
    ..['store'] = _each(1234567.8)
    ..['store_cap'] = _each(12345678)
    ..['protected'] = _each(1234567)
    ..['production'] = _each(12.3456)
    ..['platform'] = _bigPlatform
    ..['raid_power'] = {'least': 6543210.5, 'most': 7654321.5}
    ..['upgrade_refusal'] = _bigRefusal
    ..['hops_home'] = 64;
  ((json['outpost'] as Map<String, dynamic>)['raid'] as Map<String, dynamic>)
    ..['power'] = 7654321.5
    ..['ships'] = [
      {'class': 'Cruiser', 'count': 1234567},
      {'class': 'Battlecruiser', 'count': 2345678},
    ];
  ((json['outpost'] as Map<String, dynamic>)['next_level'] as Map<String, dynamic>)
    ..['cost'] = _each(23456789)
    ..['store_cap'] = _each(123456789)
    ..['production'] = _each(123.456)
    ..['platform'] = _bigPlatform
    ..['ticks'] = 3456789;
  for (final f in json['fleets'] as List) {
    (f as Map<String, dynamic>)
      ..['cargo'] = _each(123456.7)
      ..['cargo_capacity'] = 1234567;
  }
  return json;
}

/// The recorded state as it would be without the outpost: fleet 7 stands in
/// ring 10 with [hold] aboard.
Map<String, dynamic> _beforeFounding(Map<String, dynamic> hold) {
  final json = _recorded('full_state__with_outpost')..remove('outpost');
  for (final s in json['known_sectors'] as List) {
    (s as Map<String, dynamic>).remove('outpost');
  }
  for (final f in json['fleets'] as List) {
    (f as Map<String, dynamic>)
      ..remove('outpost_fuel')
      ..['cargo'] = hold
      ..['cargo_capacity'] = 12345678;
  }
  return json;
}

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

Future<_Recorded> _open(WidgetTester tester, Map<String, dynamic> state, {Size size = _desktop, Screen screen = Screen.windshield}) async {
  final c = _Recorded(state);
  addTearDown(c.dispose);
  await pumpShell(tester, size: size, screen: screen, controller: c);
  return c;
}

Future<void> _key(WidgetTester tester, LogicalKeyboardKey k, String ch) async {
  await tester.sendKeyEvent(k, character: ch);
  await tester.pump(const Duration(milliseconds: 50));
}

Future<void> _press(WidgetTester tester, String key) async {
  final target = find.byKey(ValueKey(key));
  await tester.ensureVisible(target);
  await tester.pump(const Duration(milliseconds: 50));
  await tester.tap(target);
  await tester.pump(const Duration(milliseconds: 100));
}

String _said(WidgetTester tester, String key) {
  final text = tester.widget<Text>(find.byKey(Key(key)));
  return text.data ?? text.textSpan!.toPlainText();
}

/// The caption under a control, which stands in a padded box of its own.
String _why(WidgetTester tester, String key) => tester.widget<Text>(find.descendant(of: find.byKey(Key(key)), matching: find.byType(Text))).data!;

Finder _railTile() => find.descendant(of: find.byType(Rail), matching: find.text('OUTPOST'));

final _hudLine = find.byKey(const ValueKey('hud-outpost'));

Screen _screen(WidgetTester tester) => Console.of(tester.element(find.byType(Rail))).screen;

void main() {
  testWidgets('at the recorded outpost the station is a contact and a dock tile; O opens its panel with the store, the yield, the defence, the raid on its way and the next level; Upgrade, Abandon and the dock send their orders',
      (tester) async {
    final c = await _open(tester, _recorded('full_state__with_outpost'));

    final contact = _said(tester, 'contact-outpost');
    expect(contact, contains('LEVEL 1 OF 3 - BUILDING LEVEL 2, 3:43:00 LEFT'));
    expect(contact, contains('A FLEET STANDING HERE WHEN A RAID ARRIVES IS IN ITS FIGHT'), reason: 'a fleet at the outpost is in a raid\'s reach, and the contact says so');
    expect(contact.toUpperCase(), isNot(contains('SAFE')));
    expect(find.text('YOUR OUTPOST'), findsWidgets);
    expect(find.descendant(of: find.byKey(const ValueKey('dock-outpost')), matching: find.text('OUTPOST')), findsOneWidget);
    expect(_railTile(), findsOneWidget);
    expect(find.descendant(of: _hudLine, matching: find.text('OUTPOST 10,-2')), findsOneWidget);
    expect(find.descendant(of: _hudLine, matching: find.text('L1 STORE 8% RAID 2:50')), findsOneWidget);
    expect(find.descendant(of: find.byKey(const Key('status-way-outpost')), matching: find.text('AT THE OUTPOST')), findsOneWidget);
    expect(find.byKey(const ValueKey('outpost-panel')), findsNothing);
    await shot(tester, 'outpost-contact-1440');

    await _key(tester, LogicalKeyboardKey.keyO, 'o');
    expect(_said(tester, 'outpost-standing'), 'LEVEL 1 OF 3 - BUILDING LEVEL 2, 3:43:00 LEFT');
    expect(find.descendant(of: find.byKey(const Key('outpost-store-metal')), matching: find.text('412 / 9,486')), findsOneWidget);
    expect(find.descendant(of: find.byKey(const Key('outpost-store-metal')), matching: find.text('PROTECTED 412')), findsOneWidget,
        reason: 'the store holds less than the repair of the level it is building, so all of it is floor');
    expect(find.descendant(of: find.byKey(const Key('outpost-store-metal')), matching: find.text('RAIDABLE 0')), findsOneWidget);
    expect(find.descendant(of: find.byKey(const Key('outpost-store-deuterium')), matching: find.text('180 / 2,371')), findsOneWidget);
    expect(find.descendant(of: find.byKey(const Key('outpost-yield')), matching: find.text('38.9 FE, 19.4 DE')), findsOneWidget);
    expect(textUnder(tester, find.byKey(const Key('outpost-platform-RocketLauncher'))), 'ROCKET LAUNCHER\n4', reason: 'the platform is listed as the defences it is');
    expect(find.byKey(const Key('outpost-defence')), findsNothing, reason: 'no sum of two powers stands for the defence');
    expect(find.descendant(of: find.byKey(const Key('outpost-raid-range')), matching: find.text('383 TO 551')), findsOneWidget);
    expect((_said(tester, 'outpost-raid-when'), _said(tester, 'outpost-raid-label'), _said(tester, 'outpost-raid-sentence')),
        ('RAID IN 2:50', 'AT RISK', proto.RaidLabel.atRisk.outcome.toUpperCase()),
        reason: 'the forecast of the raid on its way is the state\'s, with the server\'s label');
    expect(_said(tester, 'outpost-raid-ships'), 'IT FLIES  1X CRUISER   POWER 513');
    expect(textUnder(tester, find.byKey(const Key('outpost-raid-shares'))), 'DESTROYED 0%\nOUTLASTED 63%\nLOST 38%');
    expect(_said(tester, 'outpost-raid-kept'), 'YOU KEEP 21% OF WHAT STANDS HERE, THE RAIDERS 97% OF THEIRS, BEFORE ANYTHING IS REBUILT.');
    expect(find.byKey(const Key('outpost-cover')), findsNothing);
    expect(find.text('AFTER LEVEL 2: LEVEL 3'), findsOneWidget, reason: 'the level that could be ordered once the one being built is done');
    expect(find.descendant(of: find.byKey(const Key('outpost-next-platform')), matching: find.text('16X ROCKET LAUNCHER, 8X LIGHT LASER, 4X HEAVY LASER')),
        findsOneWidget);
    expect(find.descendant(of: find.byKey(const Key('outpost-next-cost')), matching: find.text('6,250 FE, 3,125 CR, 1,250 DE')), findsOneWidget);
    expect(find.text('9:22:30'), findsOneWidget);
    expect(_why(tester, 'outpost-upgrade-why'), 'your outpost at [10,-2] is building level 2 until tick 18200; it does one job at a time',
        reason: 'the server says beforehand why an upgrade would be refused, and the panel shows its sentence');
    await shot(tester, 'outpost-panel-1440');

    expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('outpost-upgrade'))).onPressed, isNull);
    await _press(tester, 'outpost-upgrade');
    expect(c.commands.whereType<proto.UpgradeOutpostCommand>(), isEmpty, reason: 'nothing is ordered against the server\'s refusal');

    await _press(tester, 'outpost-abandon');
    expect(c.commands.whereType<proto.AbandonOutpostCommand>(), isEmpty, reason: 'one press only asks');
    expect(_said(tester, 'outpost-abandon-warning'), contains('NOTHING IS REFUNDED'));
    await _press(tester, 'outpost-abandon-cancel');
    expect(find.byKey(const Key('outpost-abandon-warning')), findsNothing);
    await _press(tester, 'outpost-abandon');
    await _press(tester, 'outpost-abandon-confirm');
    expect(c.commands.whereType<proto.AbandonOutpostCommand>(), hasLength(1));

    expect(find.text('at the outpost'), findsOneWidget);
    expect(find.text('TAKE FROM THE STORE'), findsOneWidget);
    expect(find.descendant(of: find.byKey(const ValueKey('hold-res-metal')), matching: find.text('FE 412')), findsOneWidget);
    expect(_why(tester, 'hold-rule'), allOf(contains('Nothing moves by itself here'), contains('A fleet standing here is in a raid\'s fight')));
    await _press(tester, 'hold-res-deuterium');
    await _press(tester, 'hold-load');
    final load = c.commands.whereType<proto.LoadCommand>().single;
    expect((load.fleetId, load.resource, load.amount), (7, proto.ResourceKind.deuterium, null));
    await _press(tester, 'hold-unload');
    expect(c.commands.whereType<proto.UnloadCommand>().single.fleetId, 7);

    c.selectFleet(1);
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.descendant(of: find.byKey(const Key('status-way-outpost')), matching: find.text('221 FUEL')), findsOneWidget, reason: 'a fleet elsewhere reads what the way to the outpost costs it');
    expect(find.byKey(const ValueKey('dock-outpost')), findsNothing, reason: 'the tile stands at the outpost only');
    expect(find.byKey(const Key('contact-outpost')), findsNothing);
    c.selectFleet(0);

    c.feed(_recorded('tick_update__outpost_breached'));
    await tester.pump(const Duration(milliseconds: 600));
    expect(_said(tester, 'outpost-standing'), 'LEVEL 3 OF 3 - BREACHED, THE REPAIR WAITS FOR ITS STORE');
    expect(_said(tester, 'outpost-repair'),
        'THE REPAIR COSTS 2,188 FE, 781 CR FROM THE STORE, WHICH IS SHORT BY 988 FE: IT STAYS BREACHED UNTIL A FLEET UNLOADS THAT, '
        'OR CRYSTAL OR DEUTERIUM IN ITS PLACE, A UNIT FOR 1.5 AND FOR 2 OF METAL');
    expect(_why(tester, 'outpost-floor'), startsWith('Protected is what the next repair would take: metal, and where the metal runs out crystal, then deuterium'));
    expect(find.descendant(of: find.byKey(const Key('outpost-yield')), matching: find.text('NOTHING: BREACHED')), findsOneWidget);
    expect(_said(tester, 'outpost-max'), 'LEVEL 3 IS THE HIGHEST.');
    expect(find.byKey(const ValueKey('outpost-upgrade')), findsNothing);
    expect(find.descendant(of: find.byKey(const Key('status-way-outpost')), matching: find.text('AT THE OUTPOST')), findsOneWidget,
        reason: 'the fleet is told to stand at the outpost while it is breached and fills no tank');
    expect(find.byKey(const ValueKey('dock-outpost')), findsOneWidget);
    expect(find.descendant(of: _hudLine, matching: find.text('L3 STORE 3% BREACHED')), findsOneWidget);
    expect(_said(tester, 'contact-outpost'), allOf(contains('NO TANK IS FILLED HERE WHILE IT IS BREACHED'), contains('IS IN ITS FIGHT, BESIDE THE PLATFORM')));
    expect(_said(tester, 'outpost-platform-none'), 'NO PLATFORM: BREACHED');
    expect((_said(tester, 'outpost-raid-when'), _said(tester, 'outpost-raid-label')), ('NO RAID ANNOUNCED', 'LOST'),
        reason: 'with no platform and no ship there, the forecast of the next raid is the server\'s LOST');
    expect(
        find.descendant(
            of: find.byType(ToastLayer), matching: find.text('Raid on your outpost at 10, -2 lost after 2 rounds: no defender was left standing')),
        findsOneWidget,
        reason: 'the raid\'s end is told in a line a toast has room for');
    expect(c.state.events.map((e) => e.message), contains(startsWith('The raiders flew 1x Cruiser and lost nothing. You lost 1x Light Fighter, 4x Rocket Launcher. Taken: nothing.')));
    await shot(tester, 'outpost-breached-1440');
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  testWidgets('1440 wide: a toast raised with the outpost\'s panel open stands over the sector head, clear of the panel, its buttons, the gate cards and the fleets, and the toast of the step before does not have to make way', (tester) async {
    final c = await _open(tester, _recorded('full_state__with_outpost'));
    c.feed({'type': 'error', 'code': 'InvalidTarget', 'message': 'sample refusal of the step before'});
    await tester.pump(const Duration(milliseconds: 500));
    final earlier = find.descendant(of: find.byType(ToastLayer), matching: find.text('sample refusal of the step before'));
    expect(earlier, findsOneWidget);

    await _key(tester, LogicalKeyboardKey.keyO, 'o');
    expect(earlier, findsOneWidget, reason: 'on a desktop the panel opens in the right-hand column, where no toast stands');
    await tester.pump(const Duration(seconds: 10));
    expect(earlier, findsNothing);

    await _press(tester, 'outpost-abandon');
    c.feed({'type': 'error', 'code': 'InvalidTarget', 'message': _bigRefusal});
    await tester.pump(const Duration(milliseconds: 50));
    await tester.pump(const Duration(milliseconds: 500));
    final toast = tester.getRect(find.descendant(of: find.byType(ToastLayer), matching: find.text(_bigRefusal))).inflate(14);
    await shot(tester, 'outpost-panel-toast-1440');

    expect(toast.right < tester.getRect(find.byKey(const ValueKey('outpost-panel'))).left, isTrue, reason: 'the toast stands left of the right-hand column: $toast');
    expect(toast.left >= tester.getRect(find.byKey(const ValueKey('arena'))).left, isTrue, reason: 'inside the screen, not over the rail');
    for (final key in const ['outpost-abandon-confirm', 'outpost-abandon-cancel', 'hold-load', 'hold-unload', 'orders-toggle', 'dock-stop']) {
      expect(toast.overlaps(tester.getRect(find.byKey(ValueKey(key)))), isFalse, reason: 'it does not lie over $key');
    }
    for (final card in find.byType(GateCard).evaluate()) {
      expect(toast.overlaps(tester.getRect(find.byWidget(card.widget))), isFalse, reason: 'nor over a gate card, at eight lines');
    }
    expect(toast.bottom < tester.getRect(find.byType(FleetRail)).top, isTrue, reason: 'nor over the fleets');
    expect(tester.takeException(), isNull);
    await tester.pump(const Duration(seconds: 10));
    await endShell(tester, c);
  });

  testWidgets('390 wide: the toast of the step before makes way for the sheet the outpost\'s panel opens in, and one raised afterwards shows', (tester) async {
    final c = await _open(tester, _recorded('full_state__with_outpost'), size: _phone);
    c.feed({'type': 'error', 'code': 'InvalidTarget', 'message': 'sample refusal of the step before'});
    await tester.pump(const Duration(milliseconds: 500));
    final earlier = find.descendant(of: find.byType(ToastLayer), matching: find.text('sample refusal of the step before'));
    expect(earlier, findsOneWidget);
    await _press(tester, 'dock-outpost');
    expect(find.byKey(const ValueKey('outpost-panel')), findsOneWidget);
    expect(earlier, findsNothing, reason: 'the sheet opens where the toasts stand');
    c.feed({'type': 'error', 'code': 'InvalidTarget', 'message': 'sample refusal raised with the sheet open'});
    await tester.pump(const Duration(milliseconds: 500));
    expect(find.descendant(of: find.byType(ToastLayer), matching: find.text('sample refusal raised with the sheet open')), findsOneWidget);
    await tester.pump(const Duration(seconds: 10));
    await endShell(tester, c);
  });

  testWidgets('a window dragged from the desktop layout to the sheet layout, to a phone\'s width and back keeps the outpost\'s panel open and an Abandon that was asked', (tester) async {
    final c = await _open(tester, _recorded('full_state__with_outpost'));
    await _key(tester, LogicalKeyboardKey.keyO, 'o');
    await _press(tester, 'outpost-abandon');
    expect(_said(tester, 'outpost-abandon-warning'), contains('NOTHING IS REFUNDED'));
    for (final size in const [Size(1100, 800), _phone, Size(1100, 800), _desktop]) {
      tester.view.physicalSize = size;
      await tester.pump(const Duration(milliseconds: 100));
      await tester.pump(const Duration(milliseconds: 100));
      final wide = size.width.round();
      expect(find.byKey(const ValueKey('outpost-panel')), findsOneWidget, reason: 'the panel is still open at $wide');
      expect(find.byKey(const Key('outpost-abandon-warning')), findsOneWidget, reason: 'and still asks at $wide');
      expect(tester.takeException(), isNull, reason: 'at $wide');
    }
    await _press(tester, 'outpost-abandon-confirm');
    expect(c.commands.whereType<proto.AbandonOutpostCommand>(), hasLength(1));
    await endShell(tester, c);
  });

  for (final size in [_desktop, _least, _phone]) {
    final wide = size.width.round();
    testWidgets('$wide wide: the outpost panel and its dock hold seven-digit figures, in the windshield and on the outpost\'s own screen', (tester) async {
      final c = await _open(tester, _bigOutpost(), size: size);
      await _press(tester, 'dock-outpost');
      await tester.pump(const Duration(milliseconds: 500));
      expectTextFits(tester, find.byKey(const ValueKey('outpost-panel')), reason: 'outpost panel, $wide');
      expectTextFits(tester, find.byKey(const ValueKey('hold-panel')), reason: 'dock, $wide');
      expect(find.descendant(of: find.byKey(const Key('outpost-store-crystal')), matching: find.text('1,234,567 / 12,345,678')), findsOneWidget);
      expect(find.descendant(of: find.byKey(const Key('outpost-yield')), matching: find.text('44,444 FE, 44,444 CR, 44,444 DE')), findsOneWidget);
      expect(find.descendant(of: find.byKey(const Key('outpost-raid-range')), matching: find.text('6,543,211 TO 7,654,322')), findsOneWidget);
      expect(textUnder(tester, find.byKey(const Key('outpost-platform-HeavyLaser'))), 'HEAVY LASER\n3.46M');
      expect(_said(tester, 'outpost-raid-ships'), 'IT FLIES  1,234,567X CRUISER, 2,345,678X BATTLECRUISER   POWER 7,654,322');
      expect(_said(tester, 'outpost-raid-label'), 'AT RISK');
      expect(_why(tester, 'outpost-upgrade-why'), _bigRefusal);
      expect(tester.takeException(), isNull, reason: 'the windshield at $wide');
      await shot(tester, 'outpost-big-windshield-$wide');
      await tester.ensureVisible(find.byKey(const ValueKey('hold-panel')));
      await tester.pump(const Duration(milliseconds: 300));
      await shot(tester, 'outpost-big-dock-$wide');

      if (size == _phone) {
        expect(find.descendant(of: _hudLine, matching: find.text('OUTPOST 10,-2 L1 STORE 10% RAID 2:50')), findsOneWidget,
            reason: 'where the bar has no room the line stands under it');
      }
      await tester.tap(_railTile());
      await tester.pump(const Duration(milliseconds: 500));
      expect(_screen(tester), Screen.outpost);
      expectTextFits(tester, find.byKey(const ValueKey('outpost-panel')), reason: 'outpost screen, $wide');
      expectTextFits(tester, find.byKey(const ValueKey('hold-panel')), reason: 'dock on the outpost screen, $wide');
      expect(tester.takeException(), isNull, reason: 'the outpost screen at $wide');
      await shot(tester, 'outpost-big-screen-$wide');
      await endShell(tester, c);
    });

    testWidgets('$wide wide: the founding panel sets the cost against a seven-digit hold and shows a long refusal as the server gave it, with Found off; a site that qualifies turns it on',
        (tester) async {
      const refusal = 'the hold of fleet 7 lacks 1000 metal, 500 crystal and 200 deuterium of the 1000 metal, 500 crystal and 200 deuterium an outpost costs; '
          'it is built from what the founding fleet carries, loaded at home';
      final c = await _open(tester, _beforeFounding({'metal': 0, 'crystal': 1234567.8, 'deuterium': 199.5}), size: size);
      expect(find.descendant(of: find.byKey(const ValueKey('dock-outpost')), matching: find.text('FOUND')), findsOneWidget, reason: 'the fleet stands in ring 10');
      expect(_railTile(), findsNothing);
      await _press(tester, 'dock-outpost');
      expect(find.byKey(const Key('found-pending')), findsOneWidget);
      expect(_why(tester, 'found-why'), 'Waiting for the survey.');
      await tester.pump(const Duration(milliseconds: 250));
      final asked = c.commands.whereType<proto.OutpostSurveyCommand>().single;
      expect((asked.fleetId, asked.sector), (7, const proto.Hex(10, -2)));

      final refused = _recorded('outpost_survey')..['refusal'] = refusal;
      (refused['site'] as Map<String, dynamic>)['production'] = _each(12.3456);
      c.feed(refused);
      await tester.pump(const Duration(milliseconds: 300));
      expect(_why(tester, 'found-why'), refusal);
      expect(find.descendant(of: find.byKey(const Key('survey-grade')), matching: find.text('3.0 OF 12')), findsOneWidget);
      expect(find.descendant(of: find.byKey(const Key('survey-yield')), matching: find.text('44,444 FE, 44,444 CR, 44,444 DE AN HOUR')), findsOneWidget);
      expect(find.descendant(of: find.byKey(const Key('survey-raid')), matching: find.text('383 TO 551')), findsOneWidget);
      expect(find.descendant(of: find.byKey(const Key('survey-platform')), matching: find.text('4X ROCKET LAUNCHER')), findsOneWidget);
      expect((_said(tester, 'survey-forecast-label'), _said(tester, 'survey-forecast-sentence')), ('AT RISK', proto.RaidLabel.atRisk.outcome.toUpperCase()),
          reason: 'the survey says how a raid of the ring would go against the platform of level 1 alone');
      expect(_said(tester, 'survey-forecast-ships'), 'IT FLIES  1X CRUISER   POWER 467');
      expect(textUnder(tester, find.byKey(const Key('survey-forecast-shares'))), 'DESTROYED 0%\nOUTLASTED 50%\nLOST 50%');
      expect(find.byKey(const Key('survey-forecast-when')), findsNothing, reason: 'it is the forecast of no raid in particular');
      expect(find.descendant(of: find.byKey(const Key('survey-time')), matching: find.text('1:30:00')), findsOneWidget);
      expect(find.descendant(of: find.byKey(const Key('survey-cost-metal')), matching: find.text('ABOARD 0 - SHORT 1,000')), findsOneWidget);
      expect(find.descendant(of: find.byKey(const Key('survey-cost-crystal')), matching: find.text('ABOARD 1,234,567')), findsOneWidget);
      expect(find.descendant(of: find.byKey(const Key('survey-cost-deuterium')), matching: find.text('ABOARD 199 - SHORT 1')), findsOneWidget);
      expectTextFits(tester, find.byKey(const ValueKey('founding-panel')), reason: 'founding panel, $wide');
      await _press(tester, 'found-order');
      expect(c.commands.whereType<proto.FoundOutpostCommand>(), isEmpty, reason: 'nothing is founded against the server\'s refusal');
      expect(tester.takeException(), isNull, reason: 'refused at $wide');
      await shot(tester, 'founding-refused-$wide');

      c.feed(_recorded('outpost_survey'));
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.byKey(const Key('found-why')), findsNothing);
      expect(find.descendant(of: find.byKey(const Key('survey-yield')), matching: find.text('77.9 FE, 39.0 DE AN HOUR')), findsOneWidget);
      await _press(tester, 'found-order');
      expect(c.commands.whereType<proto.FoundOutpostCommand>().single.fleetId, 7);
      expect(tester.takeException(), isNull, reason: 'clear at $wide');
      await shot(tester, 'founding-clear-$wide');
      await endShell(tester, c);
    });
  }

  testWidgets('a fleet in ring 10 founds the outpost from its hold, the level is built, the store is stocked and drawn on, a level is added, a raid breaches it and it repairs itself, and Abandon gives it up (demo)',
      (tester) async {
    final c = DemoController()..startDemo();
    c.place(101, _site, _levelOne);
    await pumpShell(tester, screen: Screen.windshield, controller: c);
    expect(_railTile(), findsNothing);
    expect(find.byKey(const ValueKey('hud-outpost')), findsNothing);
    await _key(tester, LogicalKeyboardKey.keyO, 'o');
    await tester.pump(const Duration(milliseconds: 250));
    await tester.pump(const Duration(milliseconds: 50));
    expect(find.descendant(of: find.byKey(const Key('survey-grade')), matching: find.text('1.5 OF 12')), findsOneWidget);
    expect(find.byKey(const Key('found-why')), findsNothing);

    await _press(tester, 'found-order');
    expect(c.state.outpost!.location, _site);
    expect(c.state.holds[101]!.total, 0, reason: 'the hold paid level 1');
    expect(c.state.events.map((e) => e.message),
        contains(startsWith('fleet 101 founded your outpost at [10,0] for 3000 metal, 1500 crystal and 600 deuterium: level 1 is finished at tick')));
    expect(_said(tester, 'outpost-standing'), startsWith('NOT YET OPEN - BUILDING LEVEL 1, 0:'));
    expect(find.descendant(of: find.byKey(const Key('outpost-yield')), matching: find.text('NOTHING: NOT YET OPEN')), findsOneWidget);
    expect(find.descendant(of: _hudLine, matching: find.text('NOT OPEN STORE 0%')), findsOneWidget, reason: 'no level stands yet, so the bar names none');
    expect(_railTile(), findsOneWidget);

    await tester.pump(const Duration(seconds: 20));
    await tester.pump(const Duration(seconds: 1));
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('OUTPOST OPEN'), findsOneWidget);
    expect(find.descendant(of: find.byType(ToastLayer), matching: find.text('Outpost at [10,0]: level 1 complete')), findsOneWidget);
    expect(_said(tester, 'outpost-standing'), 'LEVEL 1 OF 3 - IN SERVICE');
    expect(c.state.fleets.first.outpostFuel, 0);

    c.handleCommand('outpost upgrade');
    await tester.pump(const Duration(milliseconds: 300));
    final short = c.state.events.first.message;
    expect(short, startsWith('Refused: outpost level 2 costs 7500 metal, 3750 crystal and 1500 deuterium; the store at [10,0] is short by 7500 metal, 37'));
    expect(find.descendant(of: find.byType(ToastLayer), matching: find.text(short.substring('Refused: '.length))), findsOneWidget,
        reason: 'the refusal is shown as the world worded it');
    expect(_why(tester, 'outpost-upgrade-why'), short.substring('Refused: '.length), reason: 'and the panel had said it beforehand, in the same sentence');
    expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('outpost-upgrade'))).onPressed, isNull);

    c.place(101, _site, const proto.Resources(metal: 7600, crystal: 3800, deuterium: 1600));
    c.handleCommand('unload');
    await tester.pump(const Duration(milliseconds: 300));
    expect(c.state.events.map((e) => e.message), contains('fleet 101 unloaded 7600 metal, 3800 crystal and 1600 deuterium into the outpost\'s store'));
    expect(find.byKey(const Key('outpost-upgrade-why')), findsNothing);
    await _press(tester, 'outpost-upgrade');
    expect(_said(tester, 'outpost-standing'), startsWith('LEVEL 1 OF 3 - BUILDING LEVEL 2, 0:'));
    expect(find.text('AFTER LEVEL 2: LEVEL 3'), findsOneWidget);

    await _press(tester, 'hold-load');
    expect(c.state.holds[101]!.metal, 100, reason: 'what the level left of the metal in the store');
    expect(c.state.outpost!.store.metal, 0);
    expect(c.state.events.map((e) => e.message),
        contains('fleet 101 loaded 100 metal from the outpost\'s store: it carries 100 of ${c.currentFleet.holdCapacity.round()}'));
    expect(c.state.fleets.first.laden, isFalse, reason: 'nothing is marked at the outpost: no hold empties by itself there');

    await tester.pump(const Duration(seconds: 51));
    await tester.pump(const Duration(milliseconds: 300));
    expect(_said(tester, 'outpost-standing'), 'LEVEL 2 OF 3 - IN SERVICE');
    expect(find.text('OUTPOST LEVEL 2'), findsOneWidget);

    expect(textUnder(tester, find.byKey(const Key('outpost-platform-RocketLauncher'))), 'ROCKET LAUNCHER\n8');
    expect(textUnder(tester, find.byKey(const Key('outpost-platform-LightLaser'))), 'LIGHT LASER\n4', reason: 'level 2 stands with the platform of its level');
    expect(_said(tester, 'outpost-raid-when'), 'NO RAID ANNOUNCED');
    final typical = c.state.outpost!.raid!;
    expect(_said(tester, 'outpost-raid-ships'), 'IT FLIES  ${shipsList(typical.ships).toUpperCase()}   POWER ${groupDigits(typical.power.round())}');

    c.raidOutpost(5000, inTicks: 5);
    await tester.pump(const Duration(seconds: 1));
    await tester.pump(const Duration(milliseconds: 300));
    expect(_said(tester, 'outpost-raid-when'), startsWith('RAID IN 0:0'));
    expect((_said(tester, 'outpost-raid-label'), c.state.outpost!.raid!.odds.lost), ('LOST', 1), reason: 'the platform and five ships do not stand through this raid');
    for (var i = 0; i < 6 && c.state.raidAtOutpost == null; i++) {
      await tester.pump(const Duration(seconds: 1));
    }
    final fight = c.state.raidAtOutpost!;
    expect((fight.raid!.outpost, fight.raid!.roundsLeft, fight.round), (true, proto.raidRounds, null), reason: 'the raid stands at the outpost on the tick it arrives');
    expect(_said(tester, 'outpost-raid-when'), 'RAID BEING FOUGHT');
    expect(find.descendant(of: _hudLine, matching: find.textContaining('RAID FOUGHT')), findsOneWidget);
    expect(textUnder(tester, find.byKey(const Key('battle-panel'))), allOf(contains('RAID\n'), contains('THE RAID STANDS HERE')));
    final quote = c.state.fleets.first.flight!;
    expect((c.state.fleets.first.status, quote.chase, find.byKey(const Key('flight-panel')).evaluate().length), (FleetStatus.combat, null, 1),
        reason: 'the fleet standing there is in the fight, and may leave it');
    final scope = tester.widgetList<CustomPaint>(find.byType(CustomPaint)).map((p) => p.painter).whereType<ArenaPainter>().single.sim.field;
    expect({for (final m in scope.marks.where((m) => m.battery)) m.unit.label: m.shown.count}, {'Rocket Launcher': 8, 'Light Laser': 4},
        reason: 'the scope draws the platform as a battery of defence marks beside the fleet');
    expect(scope.captions[Formation.batteryId], 'OUTPOST PLATFORM');
    expect(scope.theirs.map((m) => m.formation).toSet(), {Formation.raidId}, reason: 'and the raiders as the hostile side');
    await shot(tester, 'outpost-raid-arrived-1440');

    c.place(101, _demoHome, const proto.Resources());
    await tester.pump(const Duration(seconds: 1));
    final round = c.state.raidAtOutpost!.round!;
    expect(round.raid, isTrue);
    expect(round.fleets.groups.map((g) => (g.fleetId, g.battery, g.label)), [(0, true, 'outpost platform')],
        reason: 'with the fleet taken away by the script, the platform fights alone, as a battery');
    for (var i = 0; i <= proto.raidRounds && !(c.state.raidAtOutpost?.over ?? false); i++) {
      await tester.pump(const Duration(seconds: 1));
    }
    final resolved = c.state.lastOutpostRaid!.raid;
    expect((RaidEnding.of(resolved), resolved.breached, resolved.salvageDropped), (RaidEnding.lost, true, null), reason: 'no raid on an outpost leaves wreckage');
    expect(resolved.lost.map((u) => (u.unit.defence, u.count)), [(proto.DefenceKind.rocketLauncher, 8), (proto.DefenceKind.lightLaser, 4)],
        reason: 'what the raid destroyed is the platform, unit by unit');
    expect(c.state.outpost!.breached, isTrue);
    expect(c.state.outpost!.platform, isEmpty);
    expect(_said(tester, 'battle-outcome'), 'RAID LOST', reason: 'the fight\'s end is told to the fleet that was in it, where it now stands');
    expect(_said(tester, 'battle-breach'), startsWith('THE OUTPOST IS BREACHED'));
    expect(find.byKey(const Key('battle-rebuilt')), findsNothing, reason: 'a platform is not rebuilt unit by unit');
    await shot(tester, 'outpost-raid-lost-1440');

    c.place(101, _site, const proto.Resources());
    await tester.pump(const Duration(milliseconds: 300));
    expect(_said(tester, 'outpost-standing'), 'LEVEL 2 OF 3 - BREACHED, THE REPAIR WAITS FOR ITS STORE');
    expect(_said(tester, 'outpost-platform-none'), 'NO PLATFORM: BREACHED');
    expect(_said(tester, 'outpost-repair'), contains('WHICH IS SHORT BY'));
    expect(c.state.events.map((e) => e.message), contains(startsWith('! your outpost at [10,0] is breached and cannot repair itself: the repair costs 4032 metal, or crystal at 1.5 and deuterium at 2 a unit in its place, and its store is short by ')));

    c.place(101, _site, const proto.Resources(metal: 3600, crystal: 400, deuterium: 100));
    c.handleCommand('unload');
    await tester.pump(const Duration(seconds: 2));
    await tester.pump(const Duration(milliseconds: 300));
    expect(_said(tester, 'outpost-standing'), startsWith('LEVEL 2 OF 3 - BREACHED, UNDER REPAIR, 0:'));
    await tester.pump(const Duration(seconds: 14));
    await tester.pump(const Duration(milliseconds: 300));
    expect(_said(tester, 'outpost-standing'), 'LEVEL 2 OF 3 - IN SERVICE');
    await shot(tester, 'outpost-demo-1440');

    await _press(tester, 'outpost-abandon');
    await _press(tester, 'outpost-abandon-confirm');
    expect(c.state.outpost, isNull);
    expect(c.state.events.map((e) => e.message), contains(startsWith('your outpost at [10,0] is abandoned: ')));
    expect(_railTile(), findsNothing);
    expect(find.byKey(const ValueKey('founding-panel')), findsOneWidget, reason: 'with no outpost the panel is the founding survey again');
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  testWidgets('the founding panel opens anywhere with O and gives the server\'s reason: the wrong ring at home, a pack holding the sector in ring 10; the tile stands in rings 9 to 20 only (demo)',
      (tester) async {
    final c = DemoController()..startDemo();
    c.place(101, _demoHome, _levelOne);
    await pumpShell(tester, screen: Screen.windshield, controller: c);
    expect(find.byKey(const ValueKey('dock-outpost')), findsNothing);
    c.handleCommand('found');
    await tester.pump(const Duration(milliseconds: 400));
    final refused = find.descendant(of: find.byType(ToastLayer), matching: find.text('fleet 101 is at ring 4; an outpost stands in rings 9 to 20'));
    expect(refused, findsOneWidget);
    await _key(tester, LogicalKeyboardKey.keyO, 'o');
    await tester.pump(const Duration(milliseconds: 250));
    await tester.pump(const Duration(milliseconds: 50));
    expect(refused, findsOneWidget, reason: 'on a desktop the panel opens in the right-hand column, which no toast reaches: the toast of the step before stays to be read');
    expect(_why(tester, 'found-why'), 'fleet 101 is at ring 4; an outpost stands in rings 9 to 20');
    expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('found-order'))).onPressed, isNull);

    c.place(101, _held, _levelOne);
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pump(const Duration(milliseconds: 250));
    await tester.pump(const Duration(milliseconds: 50));
    expect(find.byKey(const ValueKey('dock-outpost')), findsOneWidget);
    final temper = c.state.sectors[_held]!.hostiles!.first.behavior.wire.toLowerCase();
    expect(_why(tester, 'found-why'), 'a $temper group holds [10,-3]; an outpost is founded on a sector its founder has won');
    c.handleCommand('found');
    await tester.pump(const Duration(milliseconds: 300));
    expect(c.state.outpost, isNull);
    expect(c.state.events.first.message, 'Refused: a $temper group holds [10,-3]; an outpost is founded on a sector its founder has won');

    await _key(tester, LogicalKeyboardKey.escape, '');
    expect(find.byKey(const ValueKey('founding-panel')), findsNothing);
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  testWidgets('typed: survey prints the site in three lines and the verdict, found and outpost upgrade and abandon send their orders, and a mistyped one says how (demo)', (tester) async {
    final c = DemoController()..startDemo();
    c.place(101, _site, _levelOne);
    await pumpShell(tester, controller: c);
    List<String> log() => c.state.events.map((e) => e.message).toList();

    c.handleCommand('survey');
    expect(log().take(6), [
      'Site [10,0], ring 10: grade 1.5, level 1 makes 117 Cr, 58.5 De an hour, 8 hops from home',
      'Level 1 stands with a platform of 4x Rocket Launcher; raids of this ring are of power 383 to 551',
      'Against that platform alone a raid of 1x Cruiser (power 467) is LOST: ${proto.RaidLabel.lost.outcome} (destroyed 0%, outlasted 0%, lost 100%)',
      'Level 1 costs 3,000 Fe, 1,500 Cr, 600 De from the founding hold and takes 0:20',
      'F101 can found it now',
      '> survey',
    ]);
    c.handleCommand('survey 4 -2');
    expect(log()[3], 'Cannot be founded now: [4,-2] is at ring 4; an outpost stands in rings 9 to 20');
    c.handleCommand('survey 20 -10');
    expect(log().take(5), [
      'Site [20,-10], ring 20: not on your chart, so its ore and its yield are not known',
      'Level 1 stands with a platform of 4x Rocket Launcher; raids of this ring are of power 2,799 to 4,024',
      allOf(startsWith('Against that platform alone a raid of '), contains(' is LOST: '), endsWith('(destroyed 0%, outlasted 0%, lost 100%)')),
      'Level 1 costs 3,000 Fe, 1,500 Cr, 600 De from the founding hold and takes 0:20',
      'Cannot be founded now: [20,-10] is not on your chart; a survey reads the site of a sector you have seen',
    ], reason: 'a sector never seen is answered with what its ring decides and nothing of what is there');
    c.handleCommand('survey here');
    expect(log().first, startsWith('survey: [q r]'));
    c.handleCommand('outpost');
    expect(log().first, 'outpost: upgrade|abandon');
    c.handleCommand('outpost upgrade');
    expect(log().first, 'Refused: you have no outpost to upgrade');
    c.handleCommand('outpost abandon');
    expect(log().first, 'Refused: you have no outpost to abandon');

    c.handleCommand('found');
    expect(c.state.outpost!.location, _site);
    c.handleCommand('found');
    expect(log().first, 'Refused: you already have an outpost at [10,0]; an empire has one, and abandon_outpost gives it up');
    c.handleCommand('outpost upgrade');
    expect(log().first, startsWith('Refused: your outpost at [10,0] is building level 1 until tick '));
    await tester.pump(const Duration(milliseconds: 300));
    await tester.tap(_railTile());
    await tester.pump(const Duration(milliseconds: 300));
    expect(_screen(tester), Screen.outpost);
    expect(find.byKey(const ValueKey('outpost-panel')), findsOneWidget);
    c.handleCommand('outpost abandon');
    await tester.pump(const Duration(milliseconds: 300));
    expect(c.state.outpost, isNull);
    expect(_why(tester, 'outpost-none'), contains('You have no outpost.'), reason: 'the outpost\'s screen says so, and how one is founded');
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  testWidgets('the map marks an outpost, the player\'s own filled; the inspector names owner, level and breach, says a remembered one may have changed, and Survey site asks the server and prints its answer',
      (tester) async {
    const mine = proto.Hex(10, -2), theirs = proto.Hex(14, -6);
    final c = await _open(tester, _recorded('full_state__with_outpost'), screen: Screen.map);
    final marks = painterOf(tester).data.sectors;
    expect((marks[mine]!.outpost!.mine, marks[theirs]!.outpost!.mine, marks[theirs]!.live), (true, false, false));
    expect(find.text('OUTPOST, YOURS FILLED'), findsOneWidget, reason: 'the legend names the mark');

    final m = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await m.addPointer(location: Offset.zero);
    await m.moveTo(at(theirs, _desktop, centre: mine));
    await tester.pump(const Duration(milliseconds: 50));
    expect(_why(tester, 'inspector-outpost'), 'OUTPOST OF RIVAL. NOT YET OPEN. AS SEEN 28M AGO: IT MAY HAVE CHANGED SINCE, OR BE GONE.');
    await shot(tester, 'map-outpost-remembered-1440');

    await m.moveTo(at(mine, _desktop, centre: mine));
    await tester.pump(const Duration(milliseconds: 50));
    expect(_why(tester, 'inspector-outpost'), 'YOUR OUTPOST. LEVEL 1.');
    await m.removePointer();
    await tester.tapAt(at(mine, _desktop, centre: mine));
    await tester.pump(const Duration(milliseconds: 100));
    await _press(tester, 'inspector-survey');
    final asked = c.commands.whereType<proto.OutpostSurveyCommand>().single;
    expect((asked.fleetId, asked.sector), (7, mine));
    expect(find.byKey(const Key('inspector-survey-reply')), findsNothing);

    c.feed(_recorded('outpost_survey__far'));
    await tester.pump(const Duration(milliseconds: 300));
    final reply = find.byKey(const Key('inspector-survey-reply'));
    for (final line in [
      'Site [10,-2], ring 10: grade 3.0, level 1 makes 77.9 Fe, 39.0 De an hour, more than 64 hops from home',
      'Level 1 stands with a platform of 4x Rocket Launcher; raids of this ring are of power 383 to 551',
      'Against that platform alone a raid of 1x Cruiser (power 467) is AT RISK: ${proto.RaidLabel.atRisk.outcome} (destroyed 0%, outlasted 50%, lost 50%)',
      'Level 1 costs 1,000 Fe, 500 Cr, 200 De from the founding hold and takes 1:30:00',
    ]) {
      expect(find.descendant(of: reply, matching: find.text(line)), findsOneWidget);
    }
    expect(_said(tester, 'inspector-survey-verdict'), 'fleet 7 is at [4,-2], 6 hexes from [10,-2]; an outpost is founded by a fleet standing in its sector',
        reason: 'the refusal as the server gave it');
    expect(c.state.events[4].message, startsWith('Cannot be founded now: fleet 7 is at [4,-2], 6 hexes from [10,-2]'), reason: 'and in the log, as a typed survey is');
    expectTextFits(tester, reply, reason: 'survey reply');
    await shot(tester, 'map-outpost-survey-1440');

    c.feed(_recorded('tick_update__outpost_breached'));
    await tester.pump(const Duration(milliseconds: 300));
    expect(_why(tester, 'inspector-outpost'), 'YOUR OUTPOST. LEVEL 3, BREACHED.');
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  testWidgets('390 wide: the map\'s sheet holds the outpost note and the survey reply', (tester) async {
    const mine = proto.Hex(10, -2);
    final c = await _open(tester, _recorded('full_state__with_outpost'), size: _phone, screen: Screen.map);
    c.feed(_recorded('outpost_survey__far'));
    await tester.tapAt(at(mine, _phone, centre: mine));
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.byKey(const Key('inspector-outpost')), findsOneWidget);
    expectTextFits(tester, find.byKey(const Key('inspector-survey-reply')), reason: 'survey reply at 390');
    expect(tester.takeException(), isNull);
    await shot(tester, 'map-outpost-survey-390');
    await endShell(tester, c);
  });
}
