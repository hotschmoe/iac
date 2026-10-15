// Fading relics on the console: the held ones as the server lists them with
// what each gives now and how long it has left, a marker on the fleet that
// carries one, and the scripted world's claim, fade and drop by the engine's
// rules. The figures are the server's; nothing is worked out beyond the bar.

import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/protocol/protocol.dart' as proto;

import 'fixtures_util.dart';
import 'support/demo_controller.dart';
import 'support/harness.dart';

const _desktop = Size(1440, 900);

Map<String, dynamic> _recorded(String name) =>
    readJson(File('${fixturesDir().path}/protocol/server_message/$name.json')) as Map<String, dynamic>;

class _Recorded extends DemoController {
  _Recorded(Map<String, dynamic> fullState) {
    receive(const proto.AuthResult(success: true, playerId: 42));
    receive(proto.ServerMessage.fromJson(fullState));
  }
}

void main() {
  testWidgets('the relics the server lists are rows with what they give now and when they are gone', (tester) async {
    final json = _recorded('full_state');
    final c = _Recorded(json);
    addTearDown(c.dispose);
    final held = c.state.relics;
    expect(held, isNotEmpty);
    await pumpShell(tester, size: _desktop, controller: c);

    final panel = find.byKey(const Key('relics-panel'));
    await tester.ensureVisible(panel);
    await tester.pump(const Duration(milliseconds: 100));
    expect(panel, findsOneWidget);
    for (final r in held) {
      final row = find.byKey(Key('relic-${r.kind.wire}'));
      expect(row, findsOneWidget);
      final text = textUnder(tester, row);
      expect(text, contains(r.kind.label.toUpperCase()));
      expect(text, contains('FADES IN'));
    }
  });

  testWidgets('a fleet that carries a relic says so, and a fleet that does not says nothing', (tester) async {
    final json = _recorded('full_state');
    final c = _Recorded(json);
    addTearDown(c.dispose);
    final carrier = c.state.fleets.firstWhere((f) => f.relic != null);
    await pumpShell(tester, size: _desktop, controller: c);
    final tag = find.byKey(Key('fleet-relic-${carrier.id}'));
    await tester.ensureVisible(tag);
    expect(tag, findsOneWidget);
    expect(textUnder(tester, find.byKey(Key('fleet-relic-line-${carrier.id}'))), contains(carrier.relic!.kind.label.toUpperCase()));
    for (final other in c.state.fleets.where((f) => f.relic == null)) {
      expect(find.byKey(Key('fleet-relic-${other.id}')), findsNothing);
    }
  });

  testWidgets('a relic carried home is claimed at the dock, fades, and is gone with an event line', (tester) async {
    final c = DemoController()..startDemo();
    addTearDown(c.dispose);
    final fid = c.state.fleets.firstWhere((f) => f.sector == c.state.homeworld).id;
    expect(c.state.relics, isEmpty);

    c.carryRelic(fid, proto.RelicKind.metalOutput, 0.2);
    expect(c.state.fleets.firstWhere((f) => f.id == fid).relic?.kind, proto.RelicKind.metalOutput);
    final before = c.state.hw!.production.metal;

    c.advance(1);
    final claimed = c.state.relics.single;
    expect((claimed.kind, claimed.pct, claimed.pctNow), (proto.RelicKind.metalOutput, 0.2, 0.2));
    expect(c.state.fleets.firstWhere((f) => f.id == fid).relic, isNull);
    expect(c.state.hw!.production.metal, closeTo(before * 1.2, 1e-9));

    final span = claimed.expiresAt - claimed.claimedTick;
    c.advance(span ~/ 2);
    expect(c.state.relics.single.pctNow, closeTo(0.1, 1e-4));

    c.advance(span - span ~/ 2);
    expect(c.state.relics, isEmpty);
    expect(c.state.events.any((e) => e.message.contains('metal output relic has faded')), isTrue);
    c.stop();
  });
}
