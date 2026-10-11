// Decode -> encode round trips over the Rust-generated golden fixtures.
// If these fail, the Dart protocol drifted from shared/src/protocol.rs
// (or the Rust types changed: regenerate with UPDATE_FIXTURES=1).

import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/economy_view.dart' show EconomyView;
import 'package:iac_client/protocol/protocol.dart';
import 'package:iac_client/state/state_mapper.dart';

import 'fixtures_util.dart';

/// Structural equality for decoded JSON (numbers compare by value, so
/// `100` == `100.0`; map key order is irrelevant).
bool jsonEq(Object? a, Object? b) {
  if (a is Map && b is Map) {
    if (a.length != b.length) return false;
    for (final k in a.keys) {
      if (!b.containsKey(k) || !jsonEq(a[k], b[k])) return false;
    }
    return true;
  }
  if (a is List && b is List) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (!jsonEq(a[i], b[i])) return false;
    }
    return true;
  }
  if (a is num && b is num) return a == b;
  return a == b;
}

/// Re-encode through a real JSON string, as it would go over the wire.
Object? wire(Object? v) => jsonDecode(jsonEncode(v));

void roundTrip(String dir, Object? Function(Object?) decodeEncode) {
  group(dir, () {
    for (final f in fixtureFiles('protocol/$dir')) {
      test(baseName(f), () {
        final original = readJson(f);
        final out = wire(decodeEncode(original));
        expect(jsonEq(out, original), isTrue, reason: 'round trip changed JSON\n  in:  ${jsonEncode(original)}\n  out: ${jsonEncode(out)}');
      });
    }
  });
}

void main() {
  roundTrip('client_message', (j) => ClientMessage.fromJson(j).toJson());
  roundTrip('command', (j) => Command.fromJson(j).toJson());
  roundTrip('server_message', (j) => ServerMessage.fromJson(j).toJson());
  roundTrip('event_kind', (j) => GameEvent.fromJson(j).toEventJson());

  group('decoded types', () {
    test('every fixture decodes to the sealed variant its name says', () {
      final expected = <String, Type>{
        'client_message/auth': AuthRequest,
        'client_message/command': CommandMessage,
        'client_message/policy_update': PolicyUpdate,
        'client_message/request_full_state': RequestFullState,
        'server_message/auth_result': AuthResult,
        'server_message/tick_update': TickUpdate,
        'server_message/full_state': GameState,
        'server_message/event': GameEvent,
        'server_message/error__server_error': ErrorMessage,
        'server_message/preview_move': MovePreview,
        'server_message/leaderboard': LeaderboardReply,
        'server_message/exchange_quote': ExchangeQuote,
      };
      for (final e in expected.entries) {
        final dir = e.key.split('/')[0];
        final name = e.key.split('/')[1];
        final f = fixtureFiles('protocol/$dir').firstWhere((f) => baseName(f) == name);
        final j = readJson(f);
        final decoded = dir == 'client_message' ? ClientMessage.fromJson(j) : ServerMessage.fromJson(j);
        expect(decoded.runtimeType, e.value, reason: e.key);
      }
    });

    test('event_kind fixtures decode to distinct kinds covering all 27 variants', () {
      final kinds = {
        for (final f in fixtureFiles('protocol/event_kind')) GameEvent.fromJson(readJson(f)).kind.runtimeType,
      };
      expect(kinds.length, 27);
    });

    test('command fixtures cover all 21 commands', () {
      final kinds = {for (final f in fixtureFiles('protocol/command')) Command.fromJson(readJson(f)).runtimeType};
      expect(kinds.length, 21);
    });

    test('full_state names the hub and the player meter at its exchange', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final s = ServerMessage.fromJson(readJson(f)) as GameState;
      expect(s.world.servicesAt(Hex.origin), [HubService.exchange]);
      expect(s.world.servicesAt(const Hex(1, 0)), isEmpty);
      expect(s.world.nearest(HubService.exchange, const Hex(4, -2)), Hex.origin);
      expect((s.player.exchange.pressure, s.player.exchange.span, s.player.exchange.clearIn), (632.5, 4185, 2322));
      expect(s.player.exchange.fill, closeTo(632.5 / 4185, 1e-9));
    });

    test('a quote is kept for the fleet and the pair it was asked for, a refused one with its reason', () {
      final files = fixtureFiles('protocol/server_message');
      final mapper = StateMapper();
      for (final name in ['exchange_quote', 'exchange_quote__refused']) {
        mapper.apply(ServerMessage.fromJson(readJson(files.firstWhere((f) => baseName(f) == name))));
      }
      final open = mapper.quotes[(3, ResourceKind.metal, ResourceKind.deuterium)]!.quote;
      expect((open.terms.amount, open.terms.receive, open.refusal), (632.5, 225, null));
      final refused = mapper.quotes[(3, ResourceKind.deuterium, ResourceKind.metal)]!.quote;
      expect(refused.refusal, contains('3 hexes from the exchange at the hub'));
      expect(refused.terms.meterAfter.clearIn, isNull);
      expect(mapper.log, isEmpty, reason: 'quotes are asked for by the panel; they do not write to the log');
    });

    test('full_state exposes sector connections', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final s = ServerMessage.fromJson(readJson(f)) as GameState;
      expect(s.knownSectors.first.connections, isNotEmpty);
      expect(s.fleets.first.policy, isNotNull);
      expect(s.homeworld.buildings.length, BuildingType.values.length);
      expect(s.homeworld.storage.cap.metal, 7500);
      expect(s.homeworld.storage.capped, [ResourceKind.deuterium]);
      expect(s.homeworld.storage.fullInS.crystal, isNull);
    });

    test('the ratio verdict bands match the server', () {
      final cases = readJson(File('${fixturesDir().path}/protocol/ratio_labels.json')) as List;
      expect(cases, isNotEmpty);
      for (final c in cases) {
        final r = (c['ratio'] as num).toDouble();
        expect(RatioLabel.forRatio(r).wire, c['label'], reason: 'ratio $r');
      }
    });

    test('a move preview is kept for the screens, with both ways home and the verdict', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'preview_move__stranding');
      final msg = ServerMessage.fromJson(readJson(f)) as MovePreview;
      final mapper = StateMapper()..apply(msg);
      final kept = mapper.previews[(msg.fleetId, msg.target.toKey())]!.preview;
      expect(kept.fuelCost, 31.5);
      expect(kept.canJump, isFalse);
      expect(kept.hopsHome, 4);
      expect(kept.chartedHopsHome, 17);
      expect(kept.routeUnexplored, isTrue);
      expect(kept.label, RatioLabel.even);
      expect(mapper.log, isEmpty, reason: 'previews are asked for by hovering; they do not write to the log');
    });

    test('an odds answer is kept per fleet and sector, with the server ratio and verdict untouched', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'odds');
      final msg = ServerMessage.fromJson(readJson(f)) as FleetOdds;
      final mapper = StateMapper()..apply(msg);
      expect(mapper.odds.keys.map((k) => k.$1).toSet(), {3});
      final far = mapper.odds[(3, const Hex(3, -6).toKey())]!.odds;
      expect(far.ratio, closeTo(0.84, 1e-6));
      expect((far.label, far.threat.estPower), (RatioLabel.deadly, 46.5));
      expect(mapper.odds[(3, const Hex(2, -4).toKey())]!.odds.label, RatioLabel.safe);
      expect(mapper.log, isEmpty);
    });

    test('the homeworld lists defences, defence power and the next raid estimate', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final hw = (ServerMessage.fromJson(readJson(f)) as GameState).homeworld;
      expect(hw.defences.first.kind, DefenceKind.pulseTurret);
      expect(hw.defences.first.count, 12);
      expect(hw.defences.first.restoring, 2);
      expect(hw.homeDefencePower, 340.5);
      expect(hw.nextRaidEstimatePower, 300.25);
      expect(hw.catalog.defences.map((d) => d.count), [12, 0, 3]);
      expect(hw.catalog.defences.first.requires.first.label, 'Shipyard level 1');
    });

    test('the mapper keeps the table, and the log has its lines with the agent label for a player who asks', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'leaderboard');
      final mapper = StateMapper()..apply(ServerMessage.fromJson(readJson(f)));
      expect(mapper.leaderboard!.entries.length, 2);
      expect(mapper.leaderboard!.entries[1].agent, isTrue);
      expect(mapper.leaderboard!.you!.rank, 14);
      expect(boardLines(mapper.leaderboard!), contains(startsWith('#2 Claude-7 [AI]')));
      expect(mapper.log, isEmpty, reason: 'a board nobody typed for feeds the ranking panel, not the log');
      expect(mapper.toUiState().leaderboard, same(mapper.leaderboard));
    });

    test('built-in bots are listed apart from the ranking, tagged by their name', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'leaderboard__with_bots');
      final mapper = StateMapper()..apply(ServerMessage.fromJson(readJson(f)));
      final board = mapper.leaderboard!;
      expect(board.entries.map((e) => e.name), ['Admiral']);
      expect(board.bots.map((e) => e.name), ['BOT-explorer-2', 'BOT-turtle-1']);
      expect(board.bots.every((e) => isBotName(e.name)), isTrue);
      expect(board.entries.any((e) => isBotName(e.name)), isFalse);
      expect(boardLines(board), contains('2 built-in bots, unranked; best BOT-explorer-2 505.0'));
      final rank = EconomyView.of(mapper.toUiState(), me: 'Admiral').rank!;
      expect(rank.board.map((r) => (r.$1, r.$2, r.$3)), [(1, 'Admiral', 'HUM'), (0, 'BOT-explorer-2', 'BOT'), (0, 'BOT-turtle-1', 'BOT')]);
    });

    test('a sector says whether its chart is aboard or delivered', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final marks = (ServerMessage.fromJson(readJson(f)) as GameState).knownSectors.map((s) => s.chart).toSet();
      expect(marks, {null, ChartMark.aboard, ChartMark.delivered});
    });

    test('fleets carry power and range', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final fleet = (ServerMessage.fromJson(readJson(f)) as GameState).fleets.first;
      expect(fleet.power, 61.5);
      expect(fleet.rangeHops, 4);
    });

    test('full_state carries running and waiting queue items', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final hw = (ServerMessage.fromJson(readJson(f)) as GameState).homeworld;
      expect(hw.buildSlots, 2);
      expect(hw.queueWaitingMax, 3);
      expect(hw.buildQueue.length, 2);
      expect(hw.buildPending.first.waitingFor!.crystal, 120.5);
      expect(hw.buildPending.last.waitingFor, isNull);
      expect(hw.shipyardPending.map((q) => q.item.label), ['Frigate', 'Pulse Turret']);
      expect(hw.shipyardPending.first.count, 2);
      expect(hw.researchPending.single.tech, ResearchType.cruiserTech);
    });

    test('the mapper lists waiting items after the running ones and says what they wait for', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final mapper = StateMapper()..apply(ServerMessage.fromJson(readJson(f)));
      final ui = mapper.toUiState();
      expect(ui.buildQueue.map((q) => q.active), [true, true, false, false]);
      expect(ui.buildQueue[2].time, 'short by 121 Cr, starts in about 1:35');
      expect(ui.buildQueue[3].time, 'held by a reserved order, starts in about 1:35');
      expect(ui.shipyard.map((q) => q.name).skip(1), ['Frigate x2', 'Pulse Turret x10']);
      expect(ui.shipyard.map((q) => q.time).skip(1), ['paid, waiting for the yard, starts in about 4:00', 'paid, waiting for a prerequisite']);
      expect(ui.research.waiting.single.time, 'short by 1,200 De, starts in about 30:00');
    });

    test('full_state carries the homeworld catalog the server computed', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final cat = (ServerMessage.fromJson(readJson(f)) as GameState).homeworld.catalog;
      expect(cat.buildings.map((b) => b.buildingType), BuildingType.values);
      expect(cat.research.map((r) => r.tech), ResearchType.values);
      expect(cat.ships.map((s) => s.shipClass), ShipClass.values);
      final yard = cat.buildings.firstWhere((b) => b.buildingType == BuildingType.shipyard);
      expect(yard.next, isNotNull);
      expect(yard.requires.single.label, 'Metal Mine level 2');
      expect(cat.ships.first.requires.first.label, 'Shipyard level 1');
    });
  });

  group('lenient inputs (serde defaults)', () {
    for (final f in fixtureFiles('protocol/lenient')) {
      test(baseName(f), () {
        final doc = readJson(f) as Map<String, dynamic>;
        final input = doc['input'];
        final out = switch (doc['type']) {
          'Command' => Command.fromJson(input).toJson(),
          'ClientMessage' => ClientMessage.fromJson(input).toJson(),
          'ServerMessage' => ServerMessage.fromJson(input).toJson(),
          final t => fail('unknown lenient type $t'),
        };
        expect(jsonEq(wire(out), doc['canonical']), isTrue, reason: 'in ${jsonEncode(input)} out ${jsonEncode(out)}');
      });
    }
  });

  group('enums', () {
    final Map<String, dynamic> enums = readJson(File('${fixturesDir().path}/protocol/enums.json')) as Map<String, dynamic>;

    void check<T>(String name, T Function(Object?) from, Object? Function(T) to) {
      test(name, () {
        final names = enums[name] as List;
        expect(names, isNotEmpty);
        for (final n in names) {
          expect(to(from(n)), n, reason: '$name.$n');
        }
      });
    }

    check('ShipClass', ShipClass.fromJson, (ShipClass e) => e.toJson());
    check('Density', Density.fromJson, (Density e) => e.toJson());
    check('TerrainType', TerrainType.fromJson, (TerrainType e) => e.toJson());
    check('FleetStatus', FleetStatus.fromJson, (FleetStatus e) => e.toJson());
    check('NpcBehavior', NpcBehavior.fromJson, (NpcBehavior e) => e.toJson());
    check('SiteRisk', SiteRisk.fromJson, (SiteRisk e) => e.toJson());
    check('SignalKind', SignalKind.fromJson, (SignalKind e) => e.toJson());
    check('AlertLevel', AlertLevel.fromJson, (AlertLevel e) => e.toJson());
    check('HarvestResource', HarvestResource.fromJson, (HarvestResource e) => e.toJson());
    check('QueueType', QueueType.fromJson, (QueueType e) => e.toJson());
    check('WaitReason', WaitReason.fromJson, (WaitReason e) => e.toJson());
    check('BatchWait', BatchWait.fromJson, (BatchWait e) => e.toJson());
    check('PolicyPreset', PolicyPreset.fromJson, (PolicyPreset e) => e.toJson());
    check('BuildingType', BuildingType.fromJson, (BuildingType e) => e.toJson());
    check('ResearchType', ResearchType.fromJson, (ResearchType e) => e.toJson());
    check('ResourceKind', ResourceKind.fromJson, (ResourceKind e) => e.toJson());
    check('DefenceKind', DefenceKind.fromJson, (DefenceKind e) => e.toJson());
    check('ErrorCode', ErrorCode.fromJson, (ErrorCode e) => e.toJson());
    check('ChartMark', ChartMark.fromJson, (ChartMark e) => e.toJson());
    check('HubService', HubService.fromJson, (HubService e) => e.toJson());

    test('Dart enums have no values Rust lacks', () {
      void same(String name, int dartCount) => expect(dartCount, (enums[name] as List).length, reason: name);
      same('ShipClass', ShipClass.values.length);
      same('Density', Density.values.length);
      same('TerrainType', TerrainType.values.length);
      same('FleetStatus', FleetStatus.values.length);
      same('NpcBehavior', NpcBehavior.values.length);
      same('SiteRisk', SiteRisk.values.length);
      same('SignalKind', SignalKind.values.length);
      same('AlertLevel', AlertLevel.values.length);
      same('HarvestResource', HarvestResource.values.length);
      same('QueueType', QueueType.values.length);
      same('WaitReason', WaitReason.values.length);
      same('BatchWait', BatchWait.values.length);
      same('PolicyPreset', PolicyPreset.values.length);
      same('BuildingType', BuildingType.values.length);
      same('ResearchType', ResearchType.values.length);
      same('ResourceKind', ResourceKind.values.length);
      same('DefenceKind', DefenceKind.values.length);
      same('ErrorCode', ErrorCode.values.length);
      same('ChartMark', ChartMark.values.length);
      same('HubService', HubService.values.length);
    });

    test('error code numbers are not on the wire but match Rust repr', () {
      expect(ErrorCode.invalidCommand.code, 1000);
      expect(ErrorCode.authFailed.code, 2000);
      expect(ErrorCode.serverError.code, 5000);
    });
  });

  test('unknown tags are rejected, not silently ignored', () {
    expect(() => ServerMessage.fromJson({'type': 'nope'}), throwsFormatException);
    expect(() => Command.fromJson({'action': 'nope'}), throwsFormatException);
    expect(() => EventKind.fromJson({'kind': 'Nope'}), throwsFormatException);
    expect(() => ShipClass.fromJson('Dreadnought'), throwsFormatException);
  });
}
