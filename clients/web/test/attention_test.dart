// Background notices, driven by the demo world's own event sequence: a raid
// is announced on the first tick and lands 279 ticks in, a Light Fighter
// leaves the yard at 170 and the Crystal Mine finishes at 260.

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/battle.dart' show raidForce, raidLine, raidVerdict;
import 'package:iac_client/console/kv_store.dart';
import 'package:iac_client/console/notifier.dart';
import 'package:iac_client/console/prefs.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/views/overlays.dart';

import 'support/demo_controller.dart';
import 'support/harness.dart';

class _Browser implements Notifier {
  _Browser(this.permission);
  @override
  NotifyPermission permission;
  int asked = 0;
  final List<({String title, String body, String tag})> shown = [];

  @override
  Future<NotifyPermission> request() async {
    asked++;
    return permission = NotifyPermission.granted;
  }

  @override
  void show(String title, String body, {required String tag}) => shown.add((title: title, body: body, tag: tag));
}

/// Follows the tab title the app sets, and puts the page back in front when
/// the test ends.
ValueNotifier<String> _tabTitle(WidgetTester tester) {
  final title = ValueNotifier('');
  tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
    if (call.method == 'SystemChrome.setApplicationSwitcherDescription') {
      title.value = (call.arguments as Map<dynamic, dynamic>)['label'] as String;
    }
    return null;
  });
  addTearDown(() async {
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, null);
    await _lifecycle(tester, AppLifecycleState.resumed);
  });
  return title;
}

/// The engine's own lifecycle message, as the browser's `visibilitychange` sends it.
Future<void> _lifecycle(WidgetTester tester, AppLifecycleState s) =>
    tester.binding.defaultBinaryMessenger.handlePlatformMessage('flutter/lifecycle', const StringCodec().encodeMessage('$s'), (_) {});

Future<void> _background(WidgetTester tester, Duration d) async {
  await _lifecycle(tester, AppLifecycleState.hidden);
  await tester.pump(d);
}

Future<void> _foreground(WidgetTester tester) async {
  await _lifecycle(tester, AppLifecycleState.resumed);
  await tester.pump(const Duration(milliseconds: 200));
}

void main() {
  testWidgets('in the background a finished queue, a closing raid and its outcome reach the browser and the tab title; back in front they are toasts', (tester) async {
    final title = _tabTitle(tester);
    final browser = _Browser(NotifyPermission.granted);
    final c = await pumpShell(tester, prefs: ConsolePrefs(MemoryKv())..setNotify(true), notifier: browser);

    await _background(tester, const Duration(seconds: 5));
    expect(browser.shown.single.title, 'Raid incoming - In Amber Clad');
    final announced = c.state.hw!.raid;
    expect(browser.shown.single.body,
        allOf(startsWith('${raidForce(announced.ships, announced.power, opening: true)} reaches your homeworld in 4:3'), endsWith('Forecast ${raidVerdict(announced.odds)}.')),
        reason: 'the notice says what flies and how the server expects it to end, not two powers');
    expect(title.value, '(1) IN AMBER CLAD');

    await tester.pump(const Duration(seconds: 170));
    expect(browser.shown[1], (title: 'Shipyard order complete - In Amber Clad', body: '1 x Light Fighter. The next order is under way.', tag: 'queue-ship'));

    await tester.pump(const Duration(seconds: 50));
    final closing = browser.shown[2];
    expect(closing.title, 'Raid closing - In Amber Clad');
    expect(closing.body, allOf(contains('arrives in 1:00'), contains('Forecast REPELLED')));
    expect(c.state.events.any((e) => e.message.startsWith('! Raid closing: a raid of ')), isTrue, reason: 'the closing raid is in the log, which is what toasts it in front');

    await tester.pump(const Duration(seconds: 60));
    expect(browser.shown.skip(3).map((n) => n.title.replaceAll(' - In Amber Clad', '')), ['Building complete', 'Raid destroyed', 'Raid incoming'],
        reason: 'the mine finishes, the raid is fought and destroyed, and the next one is announced; the raid closes only once');
    expect(browser.shown[3].body, 'Crystal Mine level 8. The next order is under way.');
    expect(browser.shown[4].body, '${raidLine(c.state.lastRaid!.raid, tick: c.state.lastRaid!.tick)}.', reason: 'the raid\'s end is its summary');
    expect(browser.shown[4].body, allOf(startsWith('Raid on your homeworld destroyed after '), contains('You lost '), contains('Wreckage left: ')));

    final lastHull = c.state.hw!.shipyardQueue!.endTick - c.state.tick;
    await tester.pump(Duration(seconds: lastHull + 1));
    expect(browser.shown[6], (title: 'Shipyard idle - In Amber Clad', body: '1 x Light Fighter. Nothing else is queued.', tag: 'queue-ship'));
    expect(title.value, '(7) IN AMBER CLAD');

    await _foreground(tester);
    expect(title.value, 'IN AMBER CLAD');
    final toasts = find.byType(ToastLayer);
    for (final missed in ['BUILDING COMPLETE', 'RAID DESTROYED', 'RAID INCOMING', 'SHIPYARD IDLE']) {
      expect(find.descendant(of: toasts, matching: find.text(missed)), findsOneWidget);
    }
    await tester.pump(const Duration(seconds: 150));
    expect(c.state.research.name, 'Idle', reason: 'research finished while the page was in front');
    expect(browser.shown, hasLength(7));
    expect(title.value, 'IN AMBER CLAD');
    await endShell(tester, c);
  });

  testWidgets('the browser is asked for permission only from a button, and the setting turns notices off again', (tester) async {
    final title = _tabTitle(tester);
    final browser = _Browser(NotifyPermission.undecided);
    final prefs = ConsolePrefs(MemoryKv());
    final c = await pumpShell(tester, prefs: prefs, notifier: browser);

    await _background(tester, const Duration(seconds: 5));
    expect(title.value, '(1) IN AMBER CLAD', reason: 'the tab title needs no permission');
    expect(browser.shown, isEmpty);
    expect(browser.asked, 0);
    await _foreground(tester);

    c.cancelHomeworldQueue(proto.QueueType.research);
    await tester.pump(const Duration(milliseconds: 1200));
    final tile = find.byWidgetPredicate((w) => w.key is ValueKey<String> && (w.key as ValueKey<String>).value.startsWith('sug-research-')).first;
    await tester.ensureVisible(tile);
    await tester.pump(const Duration(milliseconds: 100));
    await tester.tap(tile);
    await tester.pump(const Duration(milliseconds: 1200));
    await tester.pump(const Duration(milliseconds: 500));
    expect(find.text('TELL YOU WHEN THIS FINISHES?'), findsOneWidget);
    expect(browser.asked, 0);
    await tester.tap(find.text('NOTIFY ME'));
    await tester.pump(const Duration(milliseconds: 200));
    expect(browser.asked, 1);
    expect(prefs.notify, isTrue);

    await _background(tester, const Duration(seconds: 180));
    expect(browser.shown.map((n) => n.tag), contains('queue-ship'));
    await _foreground(tester);

    await tester.sendKeyEvent(LogicalKeyboardKey.slash, character: '?');
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('NOTIFICATIONS ON'), findsOneWidget);
    await tester.tap(find.byKey(const Key('set-notify')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('NOTIFICATIONS OFF'), findsOneWidget);
    expect(prefs.notify, isFalse);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);

    final raised = browser.shown.length;
    await _background(tester, const Duration(seconds: 120));
    expect(browser.shown, hasLength(raised));
    expect(title.value, isNot('IN AMBER CLAD'));
    await _foreground(tester);
    expect(prefs.notifyAsked, isTrue);
    await endShell(tester, c);
  });

  testWidgets('in the background the outpost\'s finished level, a raid on it, the breach, the repair that waits for its materials and a full store reach the browser, each once; back in front they are toasts',
      (tester) async {
    _tabTitle(tester);
    const site = proto.Hex(10, 0);
    final browser = _Browser(NotifyPermission.granted);
    final c = DemoController()..startDemo();
    c.place(101, site, const proto.Resources(metal: 3000, crystal: 1500, deuterium: 600));
    c.handleCommand('found');
    await pumpShell(tester, controller: c, prefs: ConsolePrefs(MemoryKv())..setNotify(true), notifier: browser);
    List<({String title, String body, String tag})> outpost() => [
          for (final n in browser.shown)
            if (n.tag.startsWith('outpost')) (title: n.title.replaceAll(' - In Amber Clad', ''), body: n.body, tag: n.tag),
        ];

    await _background(tester, const Duration(seconds: 21));
    expect(outpost(), [(title: 'Outpost open', body: 'Level 1 of your outpost at 10, 0 is finished.', tag: 'outpost-level')]);

    c.place(101, const proto.Hex(4, -2), const proto.Resources());
    c.raidOutpost(5000, inTicks: 3);
    await tester.pump(const Duration(seconds: 1));
    expect(outpost().last.title, 'Raid on your outpost');
    expect(outpost().last.body,
        allOf(startsWith('A raid of 7x Cruiser (power 5,000) reaches your outpost at 10, 0 in 0:0'), endsWith('Forecast ${raidVerdict(c.state.outpost!.raid!.odds)}.')));
    expect(c.state.outpost!.raid!.odds.label, proto.RaidLabel.lost, reason: 'with the fleet flown home the platform of level 1 stands alone');

    await tester.pump(const Duration(seconds: 4));
    expect(outpost().skip(2).map((n) => n.title), ['Outpost breached', 'Outpost repair waiting']);
    expect(
        outpost()[2].body,
        allOf(startsWith('Raid on your outpost at 10, 0 lost after 1 round: no defender was left standing.'), contains('You lost 4x Rocket Launcher'),
            contains('until it has repaired itself from its store')));
    expect(outpost()[3].body,
        allOf(startsWith('Your outpost at 10, 0 is breached and cannot repair itself: the repair costs 1,612 Fe and its store is short by 1,613 Fe.'),
            endsWith('until a fleet unloads that, or crystal or deuterium in its place.')));

    await tester.pump(const Duration(seconds: 5));
    expect(outpost(), hasLength(4), reason: 'a repair that still waits is said once');
    c.place(101, site, const proto.Resources(metal: 900, crystal: 400, deuterium: 100));
    c.handleCommand('unload');
    await tester.pump(const Duration(seconds: 8));
    expect(c.state.outpost!.inService, isTrue, reason: 'the store covered the repair and the repair ran');

    c.place(101, site, const proto.Resources(crystal: 15000));
    c.handleCommand('unload');
    await tester.pump(const Duration(seconds: 1));
    expect(outpost().skip(4).single, (
      title: 'Outpost store full',
      body: 'Your outpost at 10, 0 holds all the crystal its store takes: the extractor makes no more of it until a fleet loads some.',
      tag: 'outpost-store-crystal',
    ));
    await tester.pump(const Duration(seconds: 5));
    expect(outpost(), hasLength(5), reason: 'a store that stays full is said once');

    await _foreground(tester);
    final toasts = find.byType(ToastLayer);
    for (final missed in ['OUTPOST BREACHED', 'OUTPOST REPAIR WAITING', 'OUTPOST STORE FULL']) {
      expect(find.descendant(of: toasts, matching: find.text(missed)), findsOneWidget);
    }
    expect(tester.takeException(), isNull);
    await endShell(tester, c);
  });
}
