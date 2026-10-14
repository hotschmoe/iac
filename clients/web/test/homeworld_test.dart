import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/battle.dart'
    show RaidEnding, flightPrice, raidKept, raidSummary, roundsLeftOf, roundsOf, shipsList, unitsList;
import 'package:iac_client/models/fleet.dart' show FleetStatus;
import 'package:iac_client/state/state_mapper.dart' show amountsOf;
import 'package:iac_client/console/economy_view.dart';
import 'package:iac_client/console/intel.dart' show clockFmt, groupDigits;
import 'package:iac_client/console/notifier.dart';
import 'package:iac_client/console/services.dart';
import 'package:iac_client/console/prefs.dart';
import 'package:iac_client/console/sfx.dart';
import 'package:iac_client/console/toasts.dart';
import 'package:iac_client/design/panel.dart';
import 'package:iac_client/design/theme.dart';
import 'package:iac_client/design/tokens.dart';
import 'package:iac_client/views/homeworld/homeworld_view.dart';
import 'package:iac_client/models/game_state.dart';
import 'package:iac_client/models/homeworld.dart';
import 'package:iac_client/models/resources.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/state/game_controller.dart';
import 'package:iac_client/views/homeworld/economy_tabs.dart';

import 'support/demo_controller.dart';
import 'support/harness.dart';

final _rootKey = GlobalKey();

/// What the text keyed [key] says.
String said(WidgetTester tester, String key) {
  final text = tester.widget<Text>(find.byKey(Key(key)));
  return text.data ?? text.textSpan!.toPlainText();
}

/// The caption keyed [key], which stands in a padded box of its own.
String why(WidgetTester tester, String key) => tester.widget<Text>(find.descendant(of: find.byKey(Key(key)), matching: find.byType(Text))).data!;

Future<void> snap(WidgetTester tester, String name) async {
  final dir = Platform.environment['IAC_SHOTS'];
  if (dir == null) return;
  await tester.runAsync(() async {
    final boundary = _rootKey.currentContext!.findRenderObject()! as RenderRepaintBoundary;
    final image = await boundary.toImage(pixelRatio: 1);
    final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
    final f = File('$dir/$name.png');
    await f.parent.create(recursive: true);
    await f.writeAsBytes(bytes!.buffer.asUint8List());
  });
}

void main() {
  // The view alone inside a Console scope (HUD, rail and the other screens
  // are covered by their own tests).
  Future<GameController> open(WidgetTester tester, {Size size = const Size(1440, 900)}) async {
    await tester.runAsync(loadFonts);
    tester.view.physicalSize = size;
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final c = DemoController()..startDemo();
    final con = Console(game: c, prefs: ConsolePrefs(), toasts: ToastController(), sfx: NoSfx(), notifier: NoNotifier())..screen = Screen.homeworld;
    bool onKey(KeyEvent e) => e is KeyDownEvent && con.dispatchKey(e);
    HardwareKeyboard.instance.addHandler(onKey);
    addTearDown(() => HardwareKeyboard.instance.removeHandler(onKey));
    await tester.pumpWidget(MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: consoleTheme(),
      home: RepaintBoundary(key: _rootKey, child: ConsoleScope(console: con, child: const Material(color: C.deep, child: HomeworldView()))),
    ));
    await tester.pump(const Duration(milliseconds: 400));
    return c;
  }

  Future<void> endShell(WidgetTester tester, GameController c) async {
    c.stop();
    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(seconds: 5));
  }

  Future<void> cancelAll(WidgetTester tester) async {
    for (final k in ['building', 'ship', 'research']) {
      final f = find.byKey(ValueKey('hw-cancel-$k'));
      if (f.evaluate().isNotEmpty) {
        await tester.tap(f);
        await tester.pump();
      }
    }
  }

  testWidgets('buildings: tile selects, queue button sends BuildCommand after the queue is cleared', (tester) async {
    final c = await open(tester);
    expect(find.text('METAL MINE'), findsWidgets);
    await tester.tap(find.byKey(const ValueKey('hw-card-0')));
    await tester.pump();
    expect(c.hwCursor, 0);
    // the demo has orders waiting; an order that fits is never blocked by cost
    expect(find.textContaining('Cannot afford'), findsNothing);
    await tester.tap(find.byKey(const ValueKey('hw-cancel-building')));
    await tester.pump();
    c.cancelHomeworldQueue(proto.QueueType.building, waiting: true);
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('hw-queue')));
    await tester.pump();
    expect(c.state.events.any((e) => e.message.contains('> build Metal Mine')), isTrue);
    expect(c.state.buildQueue, isNotEmpty);
    await snap(tester, 'homeworld/buildings-1440');
    await endShell(tester, c);
  });

  testWidgets('locked card shows its reason text', (tester) async {
    final c = await open(tester);
    await cancelAll(tester);
    final cat = c.state.catalog!;
    await tester.tap(find.byKey(const ValueKey('hw-tab-research')));
    await tester.pump();
    final i = cat.research.indexWhere((b) => b.requires.any((r) => !r.met));
    expect(i, greaterThanOrEqualTo(0));
    await tester.tap(find.byKey(ValueKey('hw-card-$i')));
    await tester.pump();
    expect(find.textContaining('Locked: needs'), findsWidgets);
    await endShell(tester, c);
  });

  testWidgets('shipyard: batch selection changes the BuildShipCommand count', (tester) async {
    final c = await open(tester);
    await tester.tap(find.byKey(const ValueKey('hw-tab-shipyard')));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('hw-batch-5')));
    await tester.pump();
    expect(c.shipBatch, 5);
    await tester.tap(find.byKey(const ValueKey('hw-cancel-ship')));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('hw-card-0')));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('hw-queue')));
    await tester.pump();
    final first = proto.ShipClass.values.first.label;
    expect(c.state.events.any((e) => e.message.contains('> ship $first x5')), isTrue);
    expect(c.state.shipyard.single.name, contains('$first x5'));
    await snap(tester, 'homeworld/shipyard-1440');
    await endShell(tester, c);
  });

  testWidgets('cancel sends CancelBuildCommand and keyboard model works', (tester) async {
    final c = await open(tester);
    await tester.tap(find.byKey(const ValueKey('hw-cancel-ship')));
    await tester.pump();
    expect(c.state.events.any((e) => e.message.contains('> cancel ship')), isTrue);
    expect(c.state.shipyard, isEmpty);

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    expect(c.hwCursor, 2);
    await tester.sendKeyEvent(LogicalKeyboardKey.bracketRight);
    expect(c.hwTab, HomeworldTab.research);
    await tester.sendKeyEvent(LogicalKeyboardKey.digit5);
    expect(c.hwTab, HomeworldTab.storage);
    await tester.sendKeyEvent(LogicalKeyboardKey.digit3);
    await tester.sendKeyEvent(LogicalKeyboardKey.equal);
    expect(c.shipBatch, 5);
    await tester.pump();
    await endShell(tester, c);
  });

  testWidgets('research tab renders the tech graph, with a node for every tech of the catalog', (tester) async {
    final c = await open(tester);
    await tester.tap(find.byKey(const ValueKey('hw-tab-research')));
    await tester.pump();
    expect(find.text('ENERGY TECHNOLOGY'), findsWidgets);
    final techs = c.state.catalog!.research;
    expect([for (final o in techs) o.tech], proto.ResearchType.values);
    for (var i = 0; i < techs.length; i++) {
      expect(find.byKey(ValueKey('hw-card-$i')), findsOneWidget, reason: '${techs[i].tech.label} has a node');
    }
    final plasma = techs.indexWhere((o) => o.tech == proto.ResearchType.plasmaTechnology);
    await tester.tap(find.byKey(ValueKey('hw-card-$plasma')));
    await tester.pump();
    expect(c.hwCursor, plasma);
    expect(tester.widget<Text>(find.byKey(const Key('hw-needed-by'))).data, 'BOMBER (LEVEL 5), PLASMA TURRET (LEVEL 7)',
        reason: 'a research with no figure of its own says what asks for it');
    await snap(tester, 'homeworld/research-plasma-1440');
    await tester.tap(find.byKey(const ValueKey('hw-card-1')));
    await tester.pump();
    expect(c.hwCursor, 1);
    expect(find.byKey(const ValueKey('hw-effect-buildSlots')), findsOneWidget, reason: 'Computer Technology says what a level of it does');
    await snap(tester, 'homeworld/research-1440');
    await endShell(tester, c);
  });

  testWidgets('the queue strip has a pill for each build slot the server reports, and for the next one while the server names what opens it', (tester) async {
    final c = await open(tester) as DemoController;
    Finder inPill(String letter, String text) => find.descendant(of: find.byKey(Key('hw-pill-slot-$letter')), matching: find.textContaining(text));
    expect(inPill('b', 'LOCKED'), findsOneWidget);
    expect(inPill('b', 'Needs Computer Technology level 3 (you have 1)'), findsOneWidget);
    expect(find.byKey(const Key('hw-pill-slot-c')), findsNothing);

    c.learn(proto.ResearchType.computerTechnology, 3);
    await tester.pump(const Duration(seconds: 1));
    expect(c.state.hw!.buildSlots, 2);
    expect(inPill('b', 'LOCKED'), findsNothing);
    expect(inPill('c', 'LOCKED'), findsOneWidget);
    expect(inPill('c', 'Needs Computer Technology level 6 (you have 3)'), findsOneWidget);

    c.learn(proto.ResearchType.computerTechnology, 6);
    await tester.pump(const Duration(seconds: 1));
    expect(c.state.hw!.buildSlots, 3);
    expect(c.state.hw!.buildQueue, hasLength(3), reason: 'the two waiting orders took the new slots');
    for (final (letter, build) in [('b', c.state.hw!.buildQueue[1]), ('c', c.state.hw!.buildQueue[2])]) {
      expect(inPill(letter, build.buildingType.label.toUpperCase()), findsOneWidget);
    }
    expect(find.text('LOCKED'), findsNothing);
    expect(find.byKey(const Key('hw-pill-slot-d')), findsNothing);
    await snap(tester, 'homeworld/three-slots-1440');

    await tester.tap(find.byKey(const ValueKey('hw-cancel-building-c')));
    await tester.pump();
    expect(c.state.hw!.buildQueue, hasLength(2));
    await endShell(tester, c);
  });

  testWidgets('defence and storage render from the demo: a row for every defence, a dome that is one of a kind, a building for each stockpile', (tester) async {
    final c = await open(tester) as DemoController;
    await tester.tap(find.byKey(const ValueKey('hw-tab-defence')));
    await tester.pump();
    final raid = c.state.hw!.raid;
    expect(said(tester, 'raid-when'), 'RAID IN ${clockFmt(raid.arrivalTick! - c.state.tick)}', reason: 'the countdown is to the tick the state announces');
    expect((said(tester, 'raid-label'), said(tester, 'raid-sentence')), ('REPELLED', proto.RaidLabel.repelled.outcome.toUpperCase()),
        reason: 'the forecast is the server\'s label with Rust\'s sentence for it, and no comparison of two powers');
    expect(said(tester, 'raid-ships'), 'IT FLIES  ${shipsList(raid.ships).toUpperCase()}   POWER ${groupDigits(raid.power.round())}');
    expect(textUnder(tester, find.byKey(const Key('raid-shares'))), 'DESTROYED 100%\nOUTLASTED 0%\nLOST 0%');
    expect(said(tester, 'raid-kept'), '${raidKept(raid.odds).toUpperCase()}, BEFORE ANYTHING IS REBUILT.');
    expect(find.byKey(const Key('defence-cover')), findsNothing);
    expect(
        why(tester, 'defence-rebuild'),
        allOf(contains('about ${proto.defenceRebuildPct} in a hundred'), contains('fewer than ${proto.defenceRebuildEachBelow} of a kind'),
            contains('within ${proto.defenceRebuildSpread} either side')),
        reason: 'the rebuild is told from the engine\'s three figures');
    expect(why(tester, 'defence-rebuild'), allOf(startsWith('Of the defences and of the Solar Satellites that fall in a raid'), endsWith('Ships lost are lost.')),
        reason: 'one rule for everything at home that is not a ship');
    expect(textUnder(tester, find.byKey(const Key('defence-satellites'))), contains('those that fall in it are rebuilt as defences are'));
    expect(find.textContaining('restoring'), findsNothing);
    expect(said(tester, 'defence-standing'), contains('EVERY FLEET DOCKED HERE, EVERY SOLAR SATELLITE AND EVERY DEFENCE'));
    expect(find.byKey(Key('docked-${proto.ShipClass.smallCargo.wire}')), findsOneWidget, reason: 'the fleet at the dock is listed as part of what fights');
    final defences = c.state.catalog!.defences;
    for (var i = 0; i < defences.length; i++) {
      final row = find.byKey(ValueKey('hw-card-$i'));
      expect(find.descendant(of: row, matching: find.text(defences[i].kind.label.toUpperCase())), findsOneWidget, reason: '${defences[i].kind.label} has a row');
      expect(find.descendant(of: row, matching: find.textContaining('BUILD X')), findsOneWidget, reason: 'and is ordered from it');
    }
    expect(find.text('ONE OF A KIND'), findsNWidgets(2), reason: 'the two domes');
    expect(find.descendant(of: find.byKey(const Key('defence-satellites')), matching: find.text('x2')), findsOneWidget);
    await snap(tester, 'homeworld/defence-1440');

    c.learn(proto.ResearchType.shieldingTechnology, 2);
    await tester.pump(const Duration(milliseconds: 100));
    final dome = defences.indexWhere((o) => o.kind == proto.DefenceKind.smallShieldDome);
    await tester.ensureVisible(find.byKey(ValueKey('hw-card-$dome')));
    await tester.tap(find.byKey(ValueKey('hw-card-$dome')));
    await tester.pump();
    expect(tester.widget<Kv>(find.byKey(const Key('hw-defence-limit'))).v, 'ONE OF A KIND');
    c.setShipBatch(5);
    await tester.pump();
    expect(find.descendant(of: find.byKey(ValueKey('hw-card-$dome')), matching: find.text('BUILD X1')), findsOneWidget, reason: 'a batch of a dome is the one that may stand');
    c.activateHomeworldCard(dome);
    await tester.pump(const Duration(milliseconds: 100));
    expect(c.state.events.map((e) => e.message), contains('> defence Small Shield Dome x1'));
    expect(c.state.catalog!.defences[dome].room, 0);
    expect(find.descendant(of: find.byKey(ValueKey('hw-card-$dome')), matching: find.text('At most 1 may stand, counting those on order')), findsOneWidget);
    await snap(tester, 'homeworld/defence-dome-1440');

    await tester.tap(find.byKey(const ValueKey('hw-tab-storage')));
    await tester.pump();
    for (final kind in proto.ResourceKind.values) {
      final row = find.byKey(Key('storage-row-${kind.name}'));
      expect(find.descendant(of: row, matching: find.textContaining('RAISE ${kind.storage.toUpperCase()} TO L')), findsOneWidget,
          reason: '${kind.name} is held by its own building, which is raised from its row');
    }
    await snap(tester, 'homeworld/storage-1440');
    await tester.tap(find.byKey(const ValueKey('hw-upgrade-crystal')));
    await tester.pump();
    expect(c.state.hw!.buildPending.last.buildingType, proto.BuildingType.crystalStorage);
    await endShell(tester, c);
  });

  /// The Defence tab of the scripted world with a raid of [power] arriving
  /// in two ticks, at the tick it arrives.
  Future<DemoController> raided(WidgetTester tester, double power, {Size size = const Size(1440, 900)}) async {
    final c = await open(tester, size: size) as DemoController;
    c.selectHomeworldTab(HomeworldTab.defence);
    c.raidHome(power, inTicks: 2);
    for (var i = 0; i < 4 && c.state.raidAtHome == null; i++) {
      await tester.pump(const Duration(seconds: 1));
    }
    return c;
  }

  /// Ticks until the raid at home has ended, and what it came to.
  Future<proto.RaidResolvedEvent> fought(WidgetTester tester, DemoController c) async {
    for (var i = 0; i <= proto.raidRounds && !(c.state.raidAtHome?.over ?? false); i++) {
      await tester.pump(const Duration(seconds: 1));
    }
    return c.state.raidAtHome!.resolved!;
  }

  testWidgets(
      'a raid on the homeworld is fought round by round on the Defence tab: the forecast becomes the fight, the defences stand apart from the fleet, '
      'and a raid destroyed leaves its wreckage (demo)', (tester) async {
    final c = await open(tester) as DemoController;
    c.selectHomeworldTab(HomeworldTab.defence);
    await tester.pump();
    final announced = c.state.hw!.raid;
    c.raidHome(announced.power, inTicks: 2);
    for (var i = 0; i < 4 && c.state.raidAtHome == null; i++) {
      await tester.pump(const Duration(seconds: 1));
    }
    final home = c.state.homeworld;
    final panel = find.byKey(const Key('battle-panel'));
    expect((said(tester, 'raid-when'), said(tester, 'raid-note')), ('RAID BEING FOUGHT', '${proto.raidRounds} ROUNDS LEFT AT THE MOST'),
        reason: 'on the tick it arrives the forecast is the fight, and no round is fought yet');
    expect(textUnder(tester, panel), allOf(contains('RAID\n'), contains('ROUND 0 OF ${proto.raidRounds}\n'), contains('THE RAID STANDS HERE')));
    final docked = c.state.fleets.singleWhere((f) => f.sector == home);
    expect((docked.status, docked.flight?.chase), (FleetStatus.combat, null), reason: 'the fleet at the dock is in the fight, and may leave it');
    expect(find.byKey(const Key('flight-panel')), findsOneWidget, reason: 'the way out is offered here, without the windshield');
    expect(find.descendant(of: find.byKey(const Key('flight-panel')), matching: find.textContaining('Recall (R)')), findsNothing);
    expect(c.state.events.first.message, startsWith('! the raid of ${announced.ships.single.count} Light Fighter has arrived at your homeworld'));

    await tester.pump(const Duration(seconds: 1));
    final round = c.state.raidAtHome!.round!;
    expect((round.raid, round.round), (true, 1));
    expect(round.fleets.groups.map((g) => (g.fleetId, g.battery)), [(docked.id, false), (0, true)]);
    expect(textUnder(tester, find.byKey(const Key('battle-fleets'))), startsWith('DEFENDERS\n${round.fleets.standing} STANDING'));
    expect(textUnder(tester, find.byKey(const Key('battle-hostiles'))), startsWith('RAIDERS\n${round.hostiles.standing} STANDING'));
    expect(textUnder(tester, panel), contains('ROUND 1 OF ${proto.raidRounds}'));
    final standing = c.state.raidAtHome!.raid!;
    expect(said(tester, 'battle-raid'), startsWith('${roundsLeftOf(standing.roundsLeft).toUpperCase()}.  AS IT STANDS  ${standing.odds.label.label}'));
    expect(said(tester, 'raid-note'), roundsLeftOf(c.state.hw!.raid.roundsLeft!).toUpperCase());
    expect(tester.takeException(), isNull);
    await snap(tester, 'homeworld/raid-fought-1440');

    expect(find.byKey(const ValueKey('battle-open')), findsOneWidget, reason: 'the raid as a picture, and its rounds in full, are a press away: the battle view');

    final resolved = await fought(tester, c);
    expect((RaidEnding.of(resolved), resolved.defended, resolved.outcome), (RaidEnding.destroyed, true, proto.BattleOutcome.hostilesDestroyed));
    expect(said(tester, 'battle-outcome'), 'RAID DESTROYED');
    expect(said(tester, 'battle-wreckage'), 'WRECKAGE LEFT: ${amountsOf(resolved.salvageDropped!).toUpperCase()}');
    expect(find.byKey(const Key('battle-taken')), findsNothing, reason: 'a raid repelled takes nothing');
    expect(resolved.raidersLost.single.count, resolved.raiders.single.count);
    final satellitesBack = resolved.rebuilt.where((u) => u.unit.ship == proto.ShipClass.solarSatellite).fold(0, (n, u) => n + u.count);
    expect(c.state.hw!.satellites, 2 - resolved.satellitesLost + satellitesBack, reason: 'a Solar Satellite that fell is gone unless it was rebuilt');
    expect(c.state.hw!.defences.first.count, 6 - resolved.structuresLost + resolved.structuresRestored);
    expect(c.state.hw!.raid.roundsLeft, isNull, reason: 'with the raid over the forecast is of the next one');
    expect(c.state.fleets.singleWhere((f) => f.id == docked.id).status, FleetStatus.docked);
    expect(find.byKey(const Key('flight-panel')), findsNothing);
    final told = c.state.events.map((e) => e.message).toList();
    expect(told, contains('! Raid on your homeworld destroyed after ${roundsOf(resolved.rounds)}: no raider is left'));
    expect(told, contains('${raidSummary(resolved, tick: c.state.lastRaid!.tick).join('. ')}.'), reason: 'the log has the summary under its headline');
    expect(c.state.events.where((e) => e.message.contains('Combat ended')), isEmpty);
    expect(find.byKey(const Key('last-raid')), findsNothing, reason: 'the fight\'s own report says it while it is on show');
    await snap(tester, 'homeworld/raid-destroyed-1440');

    for (var i = 0; i <= battleReportTicks; i++) {
      await tester.pump(const Duration(seconds: 1));
    }
    expect(panel, findsNothing);
    expect(said(tester, 'last-raid-ending'), 'RAID DESTROYED', reason: 'the last raid stays on the tab once the fight\'s report is put away');
    expect(textUnder(tester, find.byKey(const Key('last-raid'))), contains('WRECKAGE LEFT: ${amountsOf(resolved.salvageDropped!).toUpperCase()}'));
    await endShell(tester, c);
  });

  testWidgets(
      'a raid that leaves no defender is lost: the fleet that stayed is destroyed, the defences and the satellites that fell are rebuilt at the engine\'s rate, '
      'and the raid takes its share (demo)', (tester) async {
    final c = await raided(tester, 60 * proto.ShipClass.lightFighter.power);
    final forecast = c.state.hw!.raid.odds;
    expect((forecast.label, forecast.lost, said(tester, 'raid-label')), (proto.RaidLabel.lost, 1, 'LOST'));
    final full = c.state.hw!.storage.cap;
    c.stock(full);
    final resolved = await fought(tester, c);
    expect((RaidEnding.of(resolved), resolved.defended), (RaidEnding.lost, false));
    expect(said(tester, 'battle-outcome'), 'RAID LOST');
    expect(resolved.structuresRestored, resolved.structuresLost * proto.defenceRebuildPct ~/ 100, reason: 'the scripted world rebuilds the engine\'s share, cut off');
    expect(said(tester, 'battle-rebuilt'), 'REBUILT AT ONCE: ${unitsList(resolved.rebuilt).toUpperCase()}.');
    expect(said(tester, 'battle-taken'), 'TAKEN: ${amountsOf(resolved.resourcesLost).toUpperCase()}.');
    expect(find.byKey(const Key('battle-wreckage')), findsNothing, reason: 'only a raid destroyed to its last ship leaves wreckage');
    expect(c.state.hw!.defences.first.count, resolved.structuresRestored, reason: 'what was rebuilt stands at once');
    expect(resolved.satellitesLost, 2, reason: 'both satellites fell');
    expect(resolved.rebuilt.first, const proto.UnitCount(unit: proto.ShipyardItem.ship(proto.ShipClass.solarSatellite), count: 1),
        reason: 'the satellites are rebuilt by the defences\' rule and listed first, as the catalogue lists them');
    expect((c.state.hw!.satellites, c.state.fleets.where((f) => f.sector == c.state.homeworld).length), (1, 0),
        reason: 'the satellites rebuilt stand at once, and ships lost are lost');
    expect(resolved.resourcesLost.metal, greaterThan(0), reason: 'a full stockpile holds more than its storage protects');
    expect(c.state.stock.metal, lessThan(full.metal - resolved.resourcesLost.metal * .9), reason: 'what was taken is gone from the stockpile');
    expect(c.state.events.any((e) => e.message.contains('YOUR FLEET F102 DESTROYED')), isTrue);
    expect(tester.takeException(), isNull);
    await snap(tester, 'homeworld/raid-lost-1440');
    await endShell(tester, c);
  });

  testWidgets('a dome the raiders cannot scratch outlasts them: the raid leaves with nothing and no wreckage (demo)', (tester) async {
    final c = await open(tester) as DemoController;
    c.selectHomeworldTab(HomeworldTab.defence);
    c.place(102, const proto.Hex(3, -1), const proto.Resources());
    c.garrison({proto.DefenceKind.largeShieldDome: 1});
    c.raidHome(c.state.hw!.raid.power, inTicks: 2);
    await tester.pump(const Duration(seconds: 1));
    final forecast = c.state.hw!.raid.odds;
    expect((forecast.label, forecast.outlasted, said(tester, 'raid-label'), said(tester, 'raid-sentence')),
        (proto.RaidLabel.held, 1, 'HELD', proto.RaidLabel.held.outcome.toUpperCase()));
    await tester.pump(const Duration(seconds: 1));
    expect(find.byKey(const Key('flight-panel')), findsNothing, reason: 'no fleet is at the dock');
    final resolved = await fought(tester, c);
    expect((RaidEnding.of(resolved), resolved.defended, resolved.outcome), (RaidEnding.outlasted, true, proto.BattleOutcome.outlasted));
    expect(said(tester, 'battle-outcome'), 'RAID OUTLASTED');
    expect(textUnder(tester, find.byKey(const Key('battle-panel'))), contains('THE RAIDERS GAVE UP WITH A DEFENDER STILL STANDING AND TOOK NOTHING'));
    for (final key in ['battle-wreckage', 'battle-taken', 'battle-rebuilt']) {
      expect(find.byKey(Key(key)), findsNothing, reason: key);
    }
    expect(resolved.raidersLost, isEmpty);
    expect(c.state.hw!.defences.singleWhere((d) => d.kind == proto.DefenceKind.largeShieldDome).count, 1);
    expect(tester.takeException(), isNull);
    await snap(tester, 'homeworld/raid-outlasted-1440');
    await endShell(tester, c);
  });

  testWidgets('a fleet in a raid\'s fight leaves from the Defence tab: a lane, the rounds under fire, Stop, and out before the raid is settled (demo)',
      (tester) async {
    final c = await raided(tester, 3 * proto.ShipClass.lightFighter.power, size: const Size(390, 844));
    final home = c.state.homeworld;
    final quote = c.state.fleets.singleWhere((f) => f.sector == home).flight!;
    expect(said(tester, 'flight-price'), flightPrice(quote).toUpperCase());
    expect(said(tester, 'flight-chase'), 'NOTHING FOLLOWS.');
    expect(tester.takeException(), isNull);
    await snap(tester, 'homeworld/raid-arrived-390');
    final lane = find.byWidgetPredicate((w) => w.key is ValueKey<String> && (w.key! as ValueKey<String>).value.startsWith('flee-'));
    expect(lane, findsWidgets);
    await tester.ensureVisible(lane.first);
    await tester.tap(lane.first);
    await tester.pump();
    final leaving = c.state.fleets.singleWhere((f) => f.id == 102);
    expect((leaving.status, leaving.fleeing!.rounds), (FleetStatus.enRoute, quote.rounds));
    expect(find.byKey(const Key('leaving-panel')), findsOneWidget);
    expect(find.byKey(const Key('flight-panel')), findsNothing);
    await tester.ensureVisible(find.byKey(const Key('battle-panel')));
    await tester.pump();
    expect(tester.takeException(), isNull);
    await snap(tester, 'homeworld/raid-leaving-390');

    await tester.ensureVisible(find.byKey(const ValueKey('leaving-stop')));
    await tester.tap(find.byKey(const ValueKey('leaving-stop')));
    await tester.pump();
    expect(c.state.fleets.singleWhere((f) => f.id == 102).status, FleetStatus.combat, reason: 'Stop calls the jump off and the fleet fights on');
    expect(c.state.events.first.message, contains('called off its jump'));

    await tester.ensureVisible(lane.first);
    await tester.tap(lane.first);
    await tester.pump();
    final ships = c.state.fleets.singleWhere((f) => f.id == 102).shipCount;
    for (var i = 0; i <= quote.rounds + 1 && c.state.fleets.any((f) => f.id == 102 && f.sector == home); i++) {
      await tester.pump(const Duration(seconds: 1));
    }
    final away = c.state.fleets.singleWhere((f) => f.id == 102);
    expect((away.sector == home, away.status, away.fleeing), (false, FleetStatus.idle, null),
        reason: 'the fleet is out of the raid\'s reach, with what the parting rounds left of it');
    expect(away.shipCount, lessThanOrEqualTo(ships));
    final resolved = await fought(tester, c);
    expect(resolved.lost.where((u) => u.unit.ship == proto.ShipClass.smallCargo), isEmpty, reason: 'the defences fought on without it');
    expect(find.byKey(const Key('leaving-panel')), findsNothing);
    await endShell(tester, c);
  });

  testWidgets('short of energy the readout warns, with what is made and needed, and says when the reactor has nothing to burn', (tester) async {
    final c = await open(tester) as DemoController;
    expect(find.descendant(of: find.byKey(const Key('energy-readout')), matching: find.text('MINES AT 100%')), findsOneWidget);
    expect(find.byKey(const Key('energy-warning')), findsNothing, reason: 'the script opens on mines with all the energy they need');
    c.raise(proto.BuildingType.solarPlant, 8);
    await tester.pump(const Duration(milliseconds: 100));
    final energy = c.state.hw!.energy;
    expect(energy.fraction, lessThan(1));
    final share = (energy.fraction * 100).floor();
    expect(find.descendant(of: find.byKey(const Key('energy-readout')), matching: find.text('MINES AT $share%')), findsOneWidget);
    expect(tester.widget<Text>(find.byKey(const Key('energy-warning'))).data, 'EVERY MINE MAKES $share% OF ITS OUTPUT UNTIL MORE ENERGY IS MADE.');
    expect(tester.widget<Text>(find.byKey(const Key('energy-figures'))).textSpan!.toPlainText(), '${energy.made} MADE / ${energy.needed} NEEDED');
    expect(tester.widget<Text>(find.byKey(const Key('energy-satellites'))).data, 'SOLAR SATELLITES 2, +${energy.perSatellite} EACH');
    await snap(tester, 'homeworld/energy-short-1440');

    c.raise(proto.BuildingType.fusionReactor, 3);
    c.stock(const proto.Resources(metal: 5000, crystal: 5000));
    await tester.pump(const Duration(milliseconds: 100));
    expect(c.state.hw!.energy.fusionStarved, isTrue);
    expect(tester.widget<Text>(find.byKey(const Key('energy-fusion'))).data, 'FUSION REACTOR STOPPED: NO DEUTERIUM TO BURN');
    c.stock(const proto.Resources(metal: 5000, crystal: 5000, deuterium: 900));
    await tester.pump(const Duration(milliseconds: 100));
    expect(tester.widget<Text>(find.byKey(const Key('energy-fusion'))).data, startsWith('FUSION REACTOR BURNS '));
    await snap(tester, 'homeworld/energy-fusion-1440');
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  String offer(WidgetTester tester) => tester.widget<ConsoleButton>(find.byKey(const ValueKey('hold-load'))).label;
  String cargo(WidgetTester tester) => tester.widget<Text>(find.byKey(const Key('hold-cargo'))).textSpan!.toPlainText();

  Future<void> press(WidgetTester tester, String key) async {
    await tester.ensureVisible(find.byKey(ValueKey(key)));
    await tester.tap(find.byKey(ValueKey(key)));
    await tester.pump(const Duration(milliseconds: 50));
  }

  testWidgets('storage: the hold panel shows what is aboard and what fits, loads a share and all that fits, and unloads', (tester) async {
    final c = await open(tester);
    await tester.tap(find.byKey(const ValueKey('hw-tab-storage')));
    await tester.pump();
    expect(find.byKey(const ValueKey('hold-fleet-102')), findsOneWidget, reason: 'the fleet docked at home');
    expect(find.byKey(const ValueKey('hold-fleet-101')), findsNothing, reason: 'a fleet away cannot load');
    final room = c.state.fleets.firstWhere((f) => f.id == 102).holdCapacity;
    final half = room / 2;
    String whole(double v) => groupDigits(v.floor());
    expect(room, lessThan(c.state.stock.metal), reason: 'the hold of two Small Cargoes is smaller than the stockpile');
    expect(tester.widget<Text>(find.byKey(const Key('hold-aboard'))).data, '0 / ${whole(room)}');
    expect(offer(tester), 'Load ${whole(room)} Fe', reason: 'all that fits');
    expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('hold-unload'))).onPressed, isNull);

    await press(tester, 'hold-share-50');
    expect(offer(tester), 'Load ${whole(half)} Fe');
    final metal = c.state.stock.metal;
    await press(tester, 'hold-load');
    expect(c.state.events.map((e) => e.message), contains('> load ${half.round()} metal into F102'));
    expect(c.state.events.map((e) => e.message),
        contains(startsWith('fleet 102 loaded ${half.round()} metal from the stockpile: it carries ${half.round()} of ${room.round()}')));
    expect((c.state.holds[102]!.metal, c.state.stock.metal), (half, metal - half));
    expect(c.state.events.map((e) => e.message), contains('F102 loaded from the stockpile: ${groupDigits(half.round())} Fe'),
        reason: 'what leaves the stockpile is told by the Stockpile event');
    expect(c.state.fleets.firstWhere((f) => f.id == 102).laden, isTrue);
    expect(tester.widget<Text>(find.byKey(const Key('hold-aboard'))).data, '${whole(half)} / ${whole(room)}');
    expect(cargo(tester), 'Fe ${whole(half)}   Cr 0   De 0   KEPT ABOARD UNTIL THE FLEET HAS BEEN AWAY');
    expect(find.text('LADEN'), findsOneWidget);

    await press(tester, 'hold-res-crystal');
    await press(tester, 'hold-share-100');
    expect(offer(tester), 'Load ${whole(half)} Cr', reason: 'what is left of the hold');
    await press(tester, 'hold-load');
    expect(c.state.holds[102]!.crystal, closeTo(half, 1e-6));
    expect(tester.widget<ConsoleButton>(find.byKey(const ValueKey('hold-load'))).onPressed, isNull);
    expect(offer(tester), 'Load');
    expect(tester.widget<Text>(find.descendant(of: find.byKey(const Key('hold-why')), matching: find.byType(Text))).data, 'The hold of F102 is full.');
    await tester.pump(const Duration(milliseconds: 600));
    await snap(tester, 'homeworld/hold-1440');

    await press(tester, 'hold-unload');
    expect(c.state.events.map((e) => e.message), contains('fleet 102 unloaded ${half.ceil()} metal and ${half.ceil()} crystal into the stockpile'));
    final back = 'F102 unloaded into the stockpile: ${groupDigits(half.round())} Fe, ${groupDigits(half.round())} Cr';
    expect(c.state.events.map((e) => e.message), contains(back), reason: 'what enters the stockpile is told by the Stockpile event');
    expect(c.state.alerts.map((a) => a.message), contains(back), reason: 'and raised as an alert in the same words');
    expect(c.state.holds[102]!.total, 0);
    expect(c.state.stock.metal, closeTo(metal, 10), reason: 'what was loaded is back, and a tick or two of the mines');
    expect(find.text('LADEN'), findsNothing);
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  testWidgets('load and unload work from the command bar, and a refusal is the server\'s sentence', (tester) async {
    final c = await open(tester);
    c.handleCommand('load');
    expect(c.state.events.first.message, startsWith('load: <metal|crystal|deut> [amount|all]'));
    c.handleCommand('load metal 50');
    expect(c.state.events.first.message, contains('fleet 101 is at [6,-4], 2 hexes from home [4,-2]'));
    expect(c.state.holds[101]!.metal, 34, reason: 'a refused load moves nothing');

    c.selectFleet(1);
    c.handleCommand('load deut 50000');
    expect(c.state.events.first.message, endsWith('the stockpile holds 8800.0 deuterium; fleet 102 cannot load 50000.0'));
    final room = c.state.fleets.firstWhere((f) => f.id == 102).holdCapacity;
    c.handleCommand('load crystal ${room - 279.5}');
    c.handleCommand('load deut');
    final hold = c.state.holds[102]!;
    expect([hold.metal, hold.crystal, hold.deuterium], [0, closeTo(room - 279.5, 1e-6), closeTo(279.5, 1e-6)], reason: 'an amount, then all that fits');
    c.handleCommand('unload');
    expect(c.state.holds[102]!.total, 0);
    c.handleCommand('unload');
    expect(c.state.events.first.message, contains('fleet 102 was not loaded by hand'));
    await tester.pump();
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  testWidgets('390 wide: the hold panel fits with a laden hold and a reason on show', (tester) async {
    final c = await open(tester, size: const Size(390, 844));
    c.selectHomeworldTab(HomeworldTab.storage);
    c.selectFleet(1);
    c.handleCommand('load metal');
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('LADEN'), findsOneWidget);
    expect(find.byKey(const Key('hold-why')), findsOneWidget);
    await tester.ensureVisible(find.byKey(const ValueKey('hold-load')));
    await tester.pump(const Duration(milliseconds: 600));
    await snap(tester, 'homeworld/hold-390');
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });

  testWidgets('defence and storage show the not-reported state without server data', (tester) async {
    final g = GameController();
    addTearDown(g.dispose);
    await tester.pumpWidget(MaterialApp(
      theme: consoleTheme(),
      home: Material(
        child: Column(children: [
          Expanded(child: DefenceTab(eco: EconomyView.empty, ctrl: g, onSelect: (_) {})),
          Expanded(child: StorageTab(eco: EconomyView.empty, state: _emptyState(), ctrl: g)),
        ]),
      ),
    ));
    expect(find.text('NOT REPORTED BY SERVER YET'), findsNWidgets(2));
  });

  testWidgets('no overflow at 390x844 on every tab', (tester) async {
    final c = await open(tester, size: const Size(390, 844));
    for (final t in HomeworldTab.values) {
      c.selectHomeworldTab(t);
      await tester.pump(const Duration(milliseconds: 100));
      expect(tester.takeException(), isNull, reason: t.name);
      await snap(tester, 'homeworld/${t.name}-390');
    }
    await endShell(tester, c);
  });
}

GameState _emptyState() => const GameState(
      tick: 0,
      clockSec: 0,
      resources: Resources(metal: ResourceStock(amount: 0, rate: 0), crystal: ResourceStock(amount: 0, rate: 0), deut: ResourceStock(amount: 0, rate: 0)),
      fleets: [],
      buildQueue: [],
      shipyard: [],
      research: ResearchState(name: 'Idle', time: '', pct: 0, completed: []),
      events: [],
      alerts: [],
      homeworld: proto.Hex.origin,
      waypoints: [],
    );
