// Decode -> encode round trips over the Rust-generated golden fixtures.
// If these fail, the Dart protocol drifted from shared/src/protocol.rs
// (or the Rust types changed: regenerate with UPDATE_FIXTURES=1).

import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/protocol/protocol.dart';

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

    test('event_kind fixtures decode to distinct kinds covering all 23 variants', () {
      final kinds = {
        for (final f in fixtureFiles('protocol/event_kind')) GameEvent.fromJson(readJson(f)).kind.runtimeType,
      };
      expect(kinds.length, 23);
    });

    test('command fixtures cover all 14 commands', () {
      final kinds = {for (final f in fixtureFiles('protocol/command')) Command.fromJson(readJson(f)).runtimeType};
      expect(kinds.length, 14);
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

    test('full_state carries the homeworld catalog the server computed', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final cat = (ServerMessage.fromJson(readJson(f)) as GameState).homeworld.catalog;
      expect(cat.buildings.map((b) => b.buildingType), BuildingType.values);
      expect(cat.research.map((r) => r.tech), ResearchType.values);
      expect(cat.ships.map((s) => s.shipClass), ShipClass.values);
      final yard = cat.buildings.firstWhere((b) => b.buildingType == BuildingType.shipyard);
      expect(yard.next, isNotNull);
      expect(yard.requires.single.label, 'Metal Mine >= 2');
      expect(cat.ships.first.requires.first.label, 'Shipyard >= 1');
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
    check('PolicyPreset', PolicyPreset.fromJson, (PolicyPreset e) => e.toJson());
    check('BuildingType', BuildingType.fromJson, (BuildingType e) => e.toJson());
    check('ResearchType', ResearchType.fromJson, (ResearchType e) => e.toJson());
    check('ResourceKind', ResourceKind.fromJson, (ResourceKind e) => e.toJson());
    check('ErrorCode', ErrorCode.fromJson, (ErrorCode e) => e.toJson());

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
      same('PolicyPreset', PolicyPreset.values.length);
      same('BuildingType', BuildingType.values.length);
      same('ResearchType', ResearchType.values.length);
      same('ResourceKind', ResourceKind.values.length);
      same('ErrorCode', ErrorCode.values.length);
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
