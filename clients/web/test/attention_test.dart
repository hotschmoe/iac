// Background notices, driven by the demo world's own event sequence: a raid
// is announced on the first tick and lands 279 ticks in, a Corvette leaves
// the yard at 170 and the Crystal Mine finishes at 260.

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/kv_store.dart';
import 'package:iac_client/console/notifier.dart';
import 'package:iac_client/console/prefs.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;
import 'package:iac_client/views/overlays.dart';

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
    expect(browser.shown.single.body, contains('reaches your homeworld in 4:3'));
    expect(title.value, '(1) IN AMBER CLAD');

    await tester.pump(const Duration(seconds: 170));
    expect(browser.shown[1], (title: 'Shipyard order complete - In Amber Clad', body: '1 x Corvette. The next order is under way.', tag: 'queue-ship'));

    await tester.pump(const Duration(seconds: 50));
    expect(browser.shown[2], (title: 'Shipyard idle - In Amber Clad', body: '1 x Corvette. Nothing else is queued.', tag: 'queue-ship'));
    final closing = browser.shown.last;
    expect(closing.title, 'Raid closing - In Amber Clad');
    expect(closing.body, contains('arrives in 1:00'));
    expect(c.state.events.any((e) => e.message.startsWith('! Raid of estimated power')), isTrue, reason: 'the closing raid is in the log, which is what toasts it in front');

    await tester.pump(const Duration(seconds: 60));
    expect(browser.shown.skip(4).map((n) => n.title.replaceAll(' - In Amber Clad', '')), ['Building complete', 'Raid repelled', 'Raid incoming'],
        reason: 'the mine finishes, the raid lands, and the next one is announced; the raid closes only once');
    expect(browser.shown[4].body, 'Crystal Mine level 8. The next order is under way.');
    expect(title.value, '(7) IN AMBER CLAD');

    await _foreground(tester);
    expect(title.value, 'IN AMBER CLAD');
    final toasts = find.byType(ToastLayer);
    for (final missed in ['RAID CLOSING', 'BUILDING COMPLETE', 'RAID REPELLED', 'RAID INCOMING']) {
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
}
