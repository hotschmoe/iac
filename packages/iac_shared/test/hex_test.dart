import 'package:iac_shared/iac_shared.dart';
import 'package:test/test.dart';

void main() {
  group('Hex distance', () {
    test('known distance', () {
      const a = Hex(0, 0);
      const b = Hex(3, -1);
      expect(Hex.distance(a, b), 3);
    });

    test('symmetric', () {
      const a = Hex(2, -5);
      const b = Hex(-1, 3);
      expect(Hex.distance(a, b), Hex.distance(b, a));
    });

    test('origin distance', () {
      expect(const Hex(3, -1).distFromOrigin, 3);
      expect(Hex.origin.distFromOrigin, 0);
    });
  });

  group('Hex neighbors', () {
    test('all neighbors are distance 1', () {
      const center = Hex(5, -3);
      final nbrs = center.neighbors();
      expect(nbrs, hasLength(6));
      for (final n in nbrs) {
        expect(Hex.distance(center, n), 1);
      }
    });

    test('east neighbor', () {
      expect(const Hex(0, 0).neighbor(HexDirection.east), const Hex(1, 0));
    });

    test('direction opposite', () {
      expect(HexDirection.east.opposite(), HexDirection.west);
      expect(HexDirection.northEast.opposite(), HexDirection.southWest);
    });
  });

  group('Hex key', () {
    test('roundtrip', () {
      const h = Hex(-42, 127);
      final key = h.toKey();
      final h2 = Hex.fromKey(key);
      expect(h2, h);
    });

    test('json roundtrip', () {
      const h = Hex(7, -2);
      final j = h.toJson();
      expect(j, {'q': 7, 'r': -2});
      expect(Hex.fromJson(j), h);
    });
  });

  group('Hex ring / spiral', () {
    test('ring radius 1 has 6 hexes', () {
      expect(hexRing(Hex.origin, 1).length, 6);
    });

    test('ring radius 2 has 12 hexes', () {
      expect(hexRing(Hex.origin, 2).length, 12);
    });

    test('spiral radius 2 has 19 hexes', () {
      // 1 + 6 + 12 = 19
      expect(hexSpiral(Hex.origin, 2).length, 19);
    });
  });

  group('WorldGen basics', () {
    test('deterministic sector generation', () {
      final gen = WorldGen.init(12345);
      final s1 = gen.generateSector(const Hex(5, -3));
      final s2 = gen.generateSector(const Hex(5, -3));
      expect(s1.terrain, s2.terrain);
      expect(s1.metalDensity, s2.metalDensity);
      expect(s1.crystalDensity, s2.crystalDensity);
    });

    test('edge symmetry', () {
      final gen = WorldGen.init(12345);
      const a = Hex(3, -1);
      const b = Hex(4, -1); // east of a
      final connA = gen.sectorConnections(a);
      final connB = gen.sectorConnections(b);
      final aHasEast = (connA & 1) != 0;
      final bHasWest = (connB & (1 << 3)) != 0;
      expect(aHasEast, bHasWest);
    });

    test('hub fully connected', () {
      final gen = WorldGen.init(12345);
      final conn = gen.sectorConnections(Hex.origin);
      expect(conn, 0x3F); // all 6 directions
    });

    test('at least one connection for far hexes', () {
      final gen = WorldGen.init(12345);
      for (var q = 50; q < 60; q++) {
        final conn = gen.sectorConnections(Hex(q, -30));
        expect(conn, isNot(0), reason: 'hex [$q,-30] should have a connection');
      }
    });
  });

  group('Constants / scaling smoke', () {
    test('zone from distance', () {
      expect(Zone.fromDistance(0), Zone.centralHub);
      expect(Zone.fromDistance(5), Zone.innerRing);
      expect(Zone.fromDistance(15), Zone.outerRing);
      expect(Zone.fromDistance(30), Zone.wandering);
    });

    test('production and costs positive', () {
      expect(productionPerTick(BuildingType.metalMine, 1), greaterThan(0));
      expect(buildingCost(BuildingType.metalMine, 2).metal, greaterThan(0));
      expect(buildingTime(BuildingType.shipyard, 1), greaterThan(0));
    });

    test('resource canAfford', () {
      const wallet = ResourceAmounts(metal: 100, crystal: 50, deuterium: 10);
      expect(
        wallet.canAfford(const ResourceAmounts(metal: 50, crystal: 50)),
        isTrue,
      );
      expect(
        wallet.canAfford(const ResourceAmounts(metal: 200)),
        isFalse,
      );
    });
  });

  group('Protocol JSON', () {
    test('auth request roundtrip', () {
      const msg = AuthRequest(playerName: 'ace', token: 'tok');
      final json = msg.toJson();
      expect(json['type'], 'auth');
      final parsed = ClientMessage.fromJson(json);
      expect(parsed, isA<AuthRequest>());
      expect((parsed as AuthRequest).playerName, 'ace');
      expect(parsed.token, 'tok');
    });

    test('move command', () {
      final msg = MoveCommand(fleetId: 1, target: const Hex(2, -1));
      final json = msg.toJson();
      expect(json.containsKey('move'), isTrue);
      final parsed = ClientMessage.fromJson(json) as MoveCommand;
      expect(parsed.fleetId, 1);
      expect(parsed.target, const Hex(2, -1));
    });

    test('full_state resilient parse', () {
      final json = {
        'type': 'full_state',
        'tick': 42,
        'player': {
          'id': 1,
          'name': 'ace',
          'resources': {'metal': 500, 'crystal': 300, 'deuterium': 100},
          'homeworld': {'q': 3, 'r': -2},
        },
        'fleets': [
          {
            'id': 10,
            'location': {'q': 3, 'r': -2},
            'status': 'idle',
            'ships': [],
            'cargo': {},
            'fuel': 50,
            'fuel_max': 60,
          },
        ],
        'homeworld': {
          'location': {'q': 3, 'r': -2},
          'buildings': [],
          'research': [],
          'docked_ships': [],
        },
        'known_sectors': [],
      };
      final msg = ServerMessage.fromJson(json);
      expect(msg, isA<FullState>());
      final fs = msg as FullState;
      expect(fs.tick, 42);
      expect(fs.player.name, 'ace');
      expect(fs.fleets, hasLength(1));
      expect(fs.fleets.first.fuelMax, 60);
    });
  });
}
