import 'package:iac_server/iac_server.dart';
import 'package:iac_shared/iac_shared.dart';
import 'package:test/test.dart';

void main() {
  group('GameEngine', () {
    test('registerPlayer and tick increases resources', () {
      final engine = GameEngine(worldSeed: 42);

      final playerId = engine.registerPlayer('Admiral');
      expect(playerId, greaterThan(0));

      final player = engine.players[playerId]!;
      expect(player.name, 'Admiral');
      expect(player.resources.metal, startingResources.metal);
      expect(player.resources.crystal, startingResources.crystal);

      // Starting fleet with scouts
      final fleets = engine.fleets.values.where((f) => f.ownerId == playerId);
      expect(fleets.length, 1);
      final fleet = fleets.first;
      expect(fleet.ships.length, startingScouts);
      expect(fleet.location, player.homeworld);

      final metalBefore = player.resources.metal;
      final crystalBefore = player.resources.crystal;

      // Metal mine Lv1 produces each tick
      engine.tick();

      expect(player.resources.metal, greaterThan(metalBefore));
      expect(player.resources.crystal, greaterThan(crystalBefore));
      expect(engine.currentTick, 1);
    });

    test('reconnect same name returns same player', () {
      final engine = GameEngine(worldSeed: 1);
      final a = engine.registerPlayer('Admiral');
      final b = engine.registerPlayer('Admiral');
      expect(a, b);
      expect(engine.players.length, 1);
    });

    test('build metal mine queues and completes', () {
      final engine = GameEngine(worldSeed: 7);
      final playerId = engine.registerPlayer('Builder');
      final player = engine.players[playerId]!;

      // Give resources for upgrade to level 2
      player.resources = const ResourceAmounts(
        metal: 10000,
        crystal: 10000,
        deuterium: 10000,
      );

      engine.handleBuild(playerId, BuildingType.metalMine);
      expect(player.buildingQueue, isNotNull);
      expect(player.buildingQueue!.targetLevel, 2);

      final end = player.buildingQueue!.endTick;
      while (engine.currentTick < end) {
        engine.tick();
      }

      expect(player.buildings.metalMine, 2);
      expect(player.buildingQueue, isNull);
    });

    test('full_state snapshot includes player fleets', () {
      final engine = GameEngine(worldSeed: 99);
      final playerId = engine.registerPlayer('Scout');
      final state = engine.buildFullState(playerId);

      expect(state.player.id, playerId);
      expect(state.fleets, isNotEmpty);
      expect(state.homeworld.buildings, isNotEmpty);
      expect(state.knownSectors, isNotEmpty);
    });

    test('move requires connection', () {
      final engine = GameEngine(worldSeed: 3);
      final playerId = engine.registerPlayer('Mover');
      final fleet = engine.fleets.values.firstWhere((f) => f.ownerId == playerId);

      final neighbors = engine.worldGen.connectedNeighbors(fleet.location);
      expect(neighbors, isNotEmpty);

      engine.handleMove(fleet.id, neighbors.first);
      expect(fleet.status, FleetStatus.moving);
      expect(fleet.moveTarget, neighbors.first);

      // Arrive after cooldown ticks
      final wait = fleet.moveCooldown + 1;
      for (var i = 0; i < wait; i++) {
        engine.tick();
      }
      expect(fleet.location, neighbors.first);
    });

    test('sqlite persist and reload player', () {
      final db = GameDatabase.open(':memory:');
      final engine = GameEngine(worldSeed: 11, db: db, persistEveryNTicks: 1);
      final id = engine.registerPlayer('Persisted');
      final metal = engine.players[id]!.resources.metal;
      engine.tick(); // production + persist
      final metal2 = engine.players[id]!.resources.metal;
      expect(metal2, greaterThan(metal));
      engine.persistDirtyState();

      // New engine loads from same in-memory db
      final engine2 = GameEngine(worldSeed: 11, db: db);
      expect(engine2.players.containsKey(id), isTrue);
      expect(engine2.players[id]!.name, 'Persisted');
      expect(engine2.players[id]!.resources.metal, metal2);
      expect(engine2.fleets.values.where((f) => f.ownerId == id), isNotEmpty);
      db.close();
    });
  });
}
