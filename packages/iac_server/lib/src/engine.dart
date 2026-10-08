/// GameEngine: tick loop, commands, and protocol snapshots.
library;

import 'dart:math' as math;

import 'package:iac_shared/iac_shared.dart';

import 'combat.dart';
import 'database.dart';
import 'entities.dart';

/// Engine errors mapped to protocol error codes by the network layer.
class EngineException implements Exception {
  final ErrorCode code;
  final String message;
  EngineException(this.code, this.message);

  @override
  String toString() => 'EngineException(${code.wireName}: $message)';
}

class GameEngine {
  final WorldGen worldGen;
  final math.Random _rng;
  final GameDatabase? db;

  /// Persist dirty state every N ticks (mirrors Zig's persist_every_n_ticks).
  final int persistEveryNTicks;

  int currentTick = 0;
  int _nextId = 1;

  final Map<int, Player> players = {};
  final Map<int, Fleet> fleets = {};
  final Map<int, NpcFleet> npcFleets = {};
  final Map<int, Combat> activeCombats = {};
  final Map<int, SectorOverride> sectorOverrides = {};

  final List<GameEvent> pendingEvents = [];
  final Set<int> dirtyPlayers = {};
  final Set<int> dirtyFleets = {};
  final Set<int> dirtySectors = {};
  final Set<int> deletedFleetIds = {};

  /// Explored sector keys per player.
  final Map<int, Set<int>> exploredSectors = {};

  GameEngine({
    int? worldSeed,
    math.Random? random,
    this.db,
    this.persistEveryNTicks = 30,
  })  : worldGen = WorldGen(worldSeed ?? defaultWorldSeed),
        _rng = random ?? math.Random() {
    if (db != null) loadState();
  }

  int nextId() => _nextId++;

  int get nextIdPeek => _nextId;

  /// Load players/fleets/meta from SQLite if present.
  void loadState() {
    final database = db;
    if (database == null) return;

    final tick = database.loadTick();
    if (tick != null) currentTick = tick;
    final nid = database.loadNextId();
    if (nid != null) _nextId = nid;

    for (final p in database.loadPlayers()) {
      players[p.id] = p;
    }
    for (final f in database.loadFleets()) {
      fleets[f.id] = f;
    }
    exploredSectors.addAll(database.loadExplored());

    // Ensure nextId is above any loaded entity id
    for (final p in players.keys) {
      if (p >= _nextId) _nextId = p + 1;
    }
    for (final f in fleets.values) {
      if (f.id >= _nextId) _nextId = f.id + 1;
      for (final s in f.ships) {
        if (s.id >= _nextId) _nextId = s.id + 1;
      }
    }
  }

  /// Flush dirty entities to SQLite.
  void persistDirtyState() {
    final database = db;
    if (database == null) return;

    final dirtyP = dirtyPlayers
        .map((id) => players[id])
        .whereType<Player>()
        .toList();
    final dirtyF = dirtyFleets
        .map((id) => fleets[id])
        .whereType<Fleet>()
        .toList();

    database.persistDirty(
      tick: currentTick,
      nextId: _nextId,
      players: dirtyP,
      fleets: dirtyF,
      deletedFleetIds: List.of(deletedFleetIds),
      exploredSectors: exploredSectors,
    );
    dirtyPlayers.clear();
    dirtyFleets.clear();
    dirtySectors.clear();
    deletedFleetIds.clear();
  }

  // ── Registration ────────────────────────────────────────────────

  /// Register or reconnect a player by name. Returns player id.
  int registerPlayer(String name) {
    for (final p in players.values) {
      if (p.name == name) {
        return p.id;
      }
    }

    final playerId = nextId();
    final homeworld = findHomeworldLocation();
    if (homeworld == null) {
      throw EngineException(
        ErrorCode.serverError,
        'No homeworld locations available',
      );
    }

    final player = Player(
      id: playerId,
      name: name,
      resources: startingResources,
      homeworld: homeworld,
    );
    players[playerId] = player;

    final fleetId = nextId();
    final scoutStats = applyResearchToStats(
      ShipClass.scout.baseStats,
      player.research,
    );

    final ships = <Ship>[];
    for (var i = 0; i < startingScouts; i++) {
      ships.add(Ship.fromStats(nextId(), ShipClass.scout, scoutStats));
    }

    final fleet = Fleet(
      id: fleetId,
      ownerId: playerId,
      name: player.nextFleetName(),
      location: homeworld,
      status: FleetStatus.idle,
      ships: ships,
      // High fuel for demo playability
      fuel: 50000,
      fuelMax: 50000,
    );
    fleets[fleetId] = fleet;

    dirtyPlayers.add(playerId);
    dirtyFleets.add(fleetId);
    _recordExplored(playerId, homeworld);

    // Immediate persist for new registration (survive crash before next interval)
    if (db != null) {
      persistDirtyState();
    }

    pendingEvents.add(GameEvent.alert(
      tick: currentTick,
      message: 'Welcome, Admiral $name. Homeworld at $homeworld.',
      level: AlertLevel.info,
    ));

    return playerId;
  }

  Hex? findHomeworldLocation() {
    for (var attempt = 0; attempt < 100; attempt++) {
      final dist = homeworldMinDist +
          _rng.nextInt(homeworldMaxDist - homeworldMinDist + 1);
      final q = _rng.nextInt(dist * 2 + 1) - dist;
      final rMin = math.max(-dist, -q - dist);
      final rMax = math.min(dist, -q + dist);
      if (rMax < rMin) continue;
      final r = rMin + _rng.nextInt(rMax - rMin + 1);
      final candidate = Hex(q, r);
      if (candidate.distFromOrigin < homeworldMinDist ||
          candidate.distFromOrigin > homeworldMaxDist) {
        continue;
      }

      var tooClose = false;
      for (final p in players.values) {
        if (Hex.distance(p.homeworld, candidate) <= 1) {
          tooClose = true;
          break;
        }
      }
      if (!tooClose) return candidate;
    }
    return null;
  }

  // ── Tick ────────────────────────────────────────────────────────

  void tick() {
    currentTick++;
    pendingEvents.clear();

    processMovement();
    processCombat();
    processHarvesting();
    processNpcBehavior();
    processHomeworlds();
    processBuildQueues();
    processCooldowns();

    if (db != null &&
        persistEveryNTicks > 0 &&
        currentTick % persistEveryNTicks == 0) {
      persistDirtyState();
    }
  }

  void processMovement() {
    for (final fleet in fleets.values) {
      if (fleet.status != FleetStatus.moving) continue;

      if (fleet.moveCooldown > 0) {
        fleet.moveCooldown--;
        dirtyFleets.add(fleet.id);
        continue;
      }

      final target = fleet.moveTarget;
      if (target == null) {
        fleet.status = FleetStatus.idle;
        continue;
      }

      final firstVisit = !(exploredSectors[fleet.ownerId]?.contains(target.toKey()) ?? false);
      fleet.location = target;
      fleet.status = FleetStatus.idle;
      fleet.moveTarget = null;
      dirtyFleets.add(fleet.id);

      _recordExplored(fleet.ownerId, target);
      pendingEvents.add(GameEvent.sectorEntered(
        tick: currentTick,
        fleetId: fleet.id,
        sector: target,
        firstVisit: firstVisit,
      ));
      pendingEvents.add(GameEvent.fleetArrived(
        tick: currentTick,
        fleetId: fleet.id,
        sector: target,
      ));
      pendingEvents.add(GameEvent.alert(
        tick: currentTick,
        message: 'Fleet ${fleet.name} arrived at $target',
        fleetId: fleet.id,
        sector: target,
      ));

      _checkHomeworldDocking(fleet);
      _checkNpcEncounter(fleet);
    }
  }

  void processCombat() {
    final toRemove = <int>[];

    for (final entry in activeCombats.entries) {
      final combatId = entry.key;
      final combat = entry.value;

      final playerPtrs = <Fleet>[];
      for (final fid in combat.playerFleetIds) {
        final f = fleets[fid];
        if (f != null) playerPtrs.add(f);
      }

      final npcPtrs = <NpcFleet>[];
      for (final nid in combat.npcFleetIds) {
        final n = npcFleets[nid];
        if (n != null) npcPtrs.add(n);
      }

      if (playerPtrs.isEmpty || npcPtrs.isEmpty) {
        toRemove.add(combatId);
        continue;
      }

      final result = resolveCombatRound(
        activeCombat: combat,
        playerFleets: playerPtrs,
        npcFleets: npcPtrs,
        tick: currentTick,
        random: _rng,
      );
      pendingEvents.addAll(result.events);

      for (final fid in combat.playerFleetIds) {
        dirtyFleets.add(fid);
      }

      if (result.concluded) {
        toRemove.add(combatId);

        for (final pf in playerPtrs) {
          pf.status = pf.ships.isEmpty ? FleetStatus.docked : FleetStatus.idle;
          pf.idleTicks = 0;
        }

        pendingEvents.add(GameEvent.combatEnded(
          tick: currentTick,
          sector: combat.sector,
          playerVictory: result.playerWon,
        ));

        if (result.playerWon) {
          _dropSalvage(combat.sector, combat.npcValue);
          final key = combat.sector.toKey();
          final ov = _ensureOverride(key);
          ov.npcClearedTick = currentTick;
          dirtySectors.add(key);

          // Award salvage resources to first surviving player fleet (demo)
          for (final pf in playerPtrs) {
            if (pf.ships.isEmpty) continue;
            final salvage = combat.npcValue.scale(salvageFraction);
            final player = players[pf.ownerId];
            if (player != null) {
              player.resources = player.resources.add(salvage);
              dirtyPlayers.add(player.id);
              pendingEvents.add(GameEvent.alert(
                tick: currentTick,
                message:
                    'Victory! Salvage recovered: M${salvage.metal.toStringAsFixed(0)} '
                    'C${salvage.crystal.toStringAsFixed(0)}',
                sector: combat.sector,
                level: AlertLevel.info,
              ));
            }
            break;
          }
        }

        for (final nid in combat.npcFleetIds) {
          npcFleets.remove(nid);
        }
      }
    }

    for (final id in toRemove) {
      activeCombats.remove(id);
    }
  }

  void processHarvesting() {
    for (final fleet in fleets.values) {
      if (fleet.status != FleetStatus.harvesting) continue;

      final template = worldGen.generateSector(fleet.location);
      final sectorKey = fleet.location.toKey();
      final densities = SectorOverride.effectiveDensities(
        sectorOverrides[sectorKey],
        template,
      );

      final player = players[fleet.ownerId];
      final research = player?.research;
      final harvestPower = fleetHarvestPower(fleet, research);
      final maxCargo = fleetCargoCapacity(fleet);
      var remaining =
          maxCargo - (fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium);

      if (remaining <= 0) {
        fleet.status = FleetStatus.idle;
        dirtyFleets.add(fleet.id);
        pendingEvents.add(GameEvent.alert(
          tick: currentTick,
          message: 'Fleet ${fleet.name}: cargo full',
          fleetId: fleet.id,
        ));
        continue;
      }

      var harvestedAny = false;
      final targets = [
        (densities.metal, 'metal'),
        (densities.crystal, 'crystal'),
        (densities.deut, 'deuterium'),
      ];

      var metal = fleet.cargo.metal;
      var crystal = fleet.cargo.crystal;
      var deut = fleet.cargo.deuterium;

      for (final t in targets) {
        final amount = t.$1.harvestMultiplier * harvestPower;
        if (amount > 0 && remaining > 0) {
          final actual = math.min(amount, remaining);
          switch (t.$2) {
            case 'metal':
              metal += actual;
            case 'crystal':
              crystal += actual;
            case 'deuterium':
              deut += actual;
          }
          remaining -= actual;
          harvestedAny = true;
          _accumulateHarvest(sectorKey, t.$2, actual, t.$1);
          pendingEvents.add(GameEvent.resourceHarvested(
            tick: currentTick,
            fleetId: fleet.id,
            resourceType: t.$2,
            amount: actual,
          ));
        }
      }

      if (harvestedAny) {
        fleet.cargo = ResourceAmounts(
          metal: metal,
          crystal: crystal,
          deuterium: deut,
        );
        dirtyFleets.add(fleet.id);
      } else {
        fleet.status = FleetStatus.idle;
        dirtyFleets.add(fleet.id);
      }
    }
  }

  void _accumulateHarvest(
    int sectorKey,
    String resource,
    double amount,
    Density currentDensity,
  ) {
    if (currentDensity == Density.none) return;
    final ov = _ensureOverride(sectorKey);

    double harvested;
    switch (resource) {
      case 'metal':
        ov.metalHarvested += amount;
        harvested = ov.metalHarvested;
      case 'crystal':
        ov.crystalHarvested += amount;
        harvested = ov.crystalHarvested;
      default:
        ov.deutHarvested += amount;
        harvested = ov.deutHarvested;
    }

    final threshold = currentDensity.depletionThreshold;
    if (threshold > 0 && harvested >= threshold) {
      switch (resource) {
        case 'metal':
          ov.metalHarvested = 0;
          ov.metalDensity = currentDensity.downgrade();
        case 'crystal':
          ov.crystalHarvested = 0;
          ov.crystalDensity = currentDensity.downgrade();
        default:
          ov.deutHarvested = 0;
          ov.deutDensity = currentDensity.downgrade();
      }
      dirtySectors.add(sectorKey);
    }
  }

  void processNpcBehavior() {
    // Simple patrol: move non-combat patrol NPCs occasionally
    for (final npc in npcFleets.values) {
      if (npc.inCombat) continue;
      if (npc.behavior != NpcBehavior.patrol &&
          npc.behavior != NpcBehavior.aggressive) {
        continue;
      }
      if (npc.patrolTimer > 0) {
        npc.patrolTimer--;
        continue;
      }

      final connections = worldGen.connectedNeighbors(npc.location);
      if (connections.isEmpty) continue;

      npc.location = connections[_rng.nextInt(connections.length)];
      npc.patrolTimer = npcPatrolInterval;

      for (final fleet in fleets.values) {
        if (fleet.location == npc.location &&
            fleet.status != FleetStatus.inCombat &&
            fleet.ships.isNotEmpty) {
          _startCombat(fleet, npc);
          break;
        }
      }
    }
  }

  void processHomeworlds() {
    for (final player in players.values) {
      final metal = productionPerTick(
        BuildingType.metalMine,
        player.buildings.metalMine,
      );
      final crystal = productionPerTick(
        BuildingType.crystalMine,
        player.buildings.crystalMine,
      );
      final deut = productionPerTick(
        BuildingType.deuteriumSynthesizer,
        player.buildings.deuteriumSynthesizer,
      );
      player.resources = ResourceAmounts(
        metal: player.resources.metal + metal,
        crystal: player.resources.crystal + crystal,
        deuterium: player.resources.deuterium + deut,
      );
      dirtyPlayers.add(player.id);
    }
  }

  void processBuildQueues() {
    for (final player in players.values) {
      // Building queue
      final bq = player.buildingQueue;
      if (bq != null && currentTick >= bq.endTick) {
        player.buildings.set(bq.building, bq.targetLevel);
        player.buildingQueue = null;
        dirtyPlayers.add(player.id);
        pendingEvents.add(GameEvent.buildingCompleted(
          tick: currentTick,
          buildingType: bq.building,
          newLevel: bq.targetLevel,
          playerId: player.id,
        ));
        pendingEvents.add(GameEvent.alert(
          tick: currentTick,
          message:
              '${bq.building.label} upgraded to level ${bq.targetLevel}',
          level: AlertLevel.info,
        ));
        if (bq.building == BuildingType.fuelDepot) {
          _recalculatePlayerFleetFuel(player);
        }
      }

      // Ship queue
      final sq = player.shipQueue;
      if (sq != null && currentTick >= sq.endTick) {
        _addShipToHomeworld(player, sq.shipClass);
        sq.built += 1;
        dirtyPlayers.add(player.id);
        pendingEvents.add(GameEvent.shipBuilt(
          tick: currentTick,
          shipClass: sq.shipClass,
          count: 1,
          playerId: player.id,
        ));
        pendingEvents.add(GameEvent.alert(
          tick: currentTick,
          message: '${sq.shipClass.label} constructed at shipyard',
        ));

        if (sq.built >= sq.count) {
          player.shipQueue = null;
        } else {
          final perShip =
              shipBuildTime(sq.shipClass, player.buildings.shipyard);
          sq.endTick = currentTick + perShip;
        }
      }

      // Research queue
      final rq = player.researchQueue;
      if (rq != null && currentTick >= rq.endTick) {
        player.research.set(rq.tech, rq.targetLevel);
        player.researchQueue = null;
        dirtyPlayers.add(player.id);
        pendingEvents.add(GameEvent.researchCompleted(
          tick: currentTick,
          tech: rq.tech,
          newLevel: rq.targetLevel,
          playerId: player.id,
        ));
        pendingEvents.add(GameEvent.alert(
          tick: currentTick,
          message: '${rq.tech.label} research complete (Lv ${rq.targetLevel})',
        ));
        if (rq.tech == ResearchType.extendedFuelTanks) {
          _recalculatePlayerFleetFuel(player);
        }
      }
    }
  }

  void processCooldowns() {
    for (final fleet in fleets.values) {
      if (fleet.actionCooldown > 0) {
        fleet.actionCooldown--;
        dirtyFleets.add(fleet.id);
      }
      if (fleet.status == FleetStatus.idle) {
        fleet.idleTicks++;
        if (fleet.idleTicks >= shieldRegenIdleTicks) {
          var regen = false;
          for (final ship in fleet.ships) {
            if (ship.shield < ship.shieldMax) {
              ship.shield =
                  math.min(ship.shield + ship.shieldMax * 0.1, ship.shieldMax);
              regen = true;
            }
          }
          if (regen) dirtyFleets.add(fleet.id);
        }
      }
    }
  }

  // ── Commands ────────────────────────────────────────────────────

  void handleMove(int fleetId, Hex target) {
    final fleet = fleets[fleetId];
    if (fleet == null) {
      throw EngineException(ErrorCode.fleetNotFound, 'Fleet not found');
    }
    if (fleet.ships.isEmpty) {
      throw EngineException(ErrorCode.fleetNotFound, 'No ships');
    }
    if (fleet.status == FleetStatus.inCombat) {
      throw EngineException(ErrorCode.onCooldown, 'In combat');
    }
    if (fleet.actionCooldown > 0) {
      throw EngineException(ErrorCode.onCooldown, 'On cooldown');
    }

    final player = players[fleet.ownerId];
    if (player != null && fleet.location == player.homeworld) {
      if (_countDeployedFleets(fleet.ownerId, player.homeworld) >=
          maxFleetsPerPlayer) {
        throw EngineException(
          ErrorCode.fleetLimitReached,
          'Fleet limit reached',
        );
      }
    }

    final connections = worldGen.connectedNeighbors(fleet.location);
    if (!connections.any((c) => c == target)) {
      throw EngineException(ErrorCode.noConnection, 'No connection to target');
    }

    final research = player?.research;
    final fuelCost = fleetFuelCost(fleet, research);
    if (fleet.fuel < fuelCost) {
      throw EngineException(ErrorCode.insufficientFuel, 'Insufficient fuel');
    }

    fleet.fuel -= fuelCost;
    fleet.status = FleetStatus.moving;
    fleet.moveTarget = target;
    fleet.moveCooldown = fleetMoveCooldown(fleet, research);
    fleet.actionCooldown = fleet.moveCooldown;
    fleet.idleTicks = 0;
    dirtyFleets.add(fleetId);

    pendingEvents.add(GameEvent.alert(
      tick: currentTick,
      message: 'Fleet ${fleet.name} en route to $target',
      fleetId: fleetId,
    ));
  }

  void handleHarvest(int fleetId) {
    final fleet = fleets[fleetId];
    if (fleet == null) {
      throw EngineException(ErrorCode.fleetNotFound, 'Fleet not found');
    }
    if (fleet.ships.isEmpty) {
      throw EngineException(ErrorCode.fleetNotFound, 'No ships');
    }
    if (fleet.status == FleetStatus.inCombat) {
      throw EngineException(ErrorCode.onCooldown, 'In combat');
    }
    if (fleet.status == FleetStatus.moving) {
      throw EngineException(ErrorCode.onCooldown, 'Moving');
    }
    if (fleet.actionCooldown > 0) {
      throw EngineException(ErrorCode.onCooldown, 'On cooldown');
    }

    final template = worldGen.generateSector(fleet.location);
    final densities = SectorOverride.effectiveDensities(
      sectorOverrides[fleet.location.toKey()],
      template,
    );

    if (densities.metal == Density.none &&
        densities.crystal == Density.none &&
        densities.deut == Density.none) {
      throw EngineException(ErrorCode.noResources, 'No resources here');
    }

    final maxCargo = fleetCargoCapacity(fleet);
    final currentCargo =
        fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium;
    if (currentCargo >= maxCargo) {
      throw EngineException(ErrorCode.cargoFull, 'Cargo full');
    }

    fleet.status = FleetStatus.harvesting;
    fleet.actionCooldown = harvestCooldown;
    fleet.idleTicks = 0;
    dirtyFleets.add(fleetId);

    pendingEvents.add(GameEvent.alert(
      tick: currentTick,
      message: 'Fleet ${fleet.name}: harvesting in ${fleet.location}',
      fleetId: fleetId,
      sector: fleet.location,
    ));
  }

  void handleRecall(int fleetId) {
    final fleet = fleets[fleetId];
    if (fleet == null) {
      throw EngineException(ErrorCode.fleetNotFound, 'Fleet not found');
    }
    if (fleet.ships.isEmpty) {
      throw EngineException(ErrorCode.fleetNotFound, 'No ships');
    }
    final player = players[fleet.ownerId];
    if (player == null) {
      throw EngineException(ErrorCode.serverError, 'Player not found');
    }

    final dist = Hex.distance(fleet.location, player.homeworld);
    final fuelCost = fleetFuelCost(fleet, player.research) *
        dist *
        recallFuelMultiplier;
    if (fleet.fuel < fuelCost) {
      throw EngineException(ErrorCode.insufficientFuel, 'Insufficient fuel');
    }

    fleet.fuel -= fuelCost;

    final baseDamageChance = recallDamageChancePerHex * dist;
    final ejReduction = recallDamageReduction(player.research.emergencyJump);
    final damageChance = math.min(
      recallDamageChanceCap,
      math.max(0.0, baseDamageChance - ejReduction),
    );

    fleet.ships.removeWhere((ship) {
      if (_rng.nextDouble() < damageChance) {
        final damagePct = recallHullDamageMin +
            _rng.nextDouble() * (recallHullDamageMax - recallHullDamageMin);
        ship.hull -= ship.hullMax * damagePct;
        if (ship.hull <= 0) {
          pendingEvents.add(GameEvent.shipDestroyed(
            tick: currentTick,
            shipId: ship.id,
            shipClass: ship.class_,
            ownerFleetId: fleet.id,
            isNpc: false,
          ));
          return true;
        }
      }
      return false;
    });

    fleet.location = player.homeworld;
    fleet.status = fleet.ships.isEmpty ? FleetStatus.docked : FleetStatus.idle;
    fleet.moveTarget = null;
    fleet.moveCooldown = 0;

    pendingEvents.add(GameEvent.alert(
      tick: currentTick,
      message: 'Fleet ${fleet.name} emergency recall to homeworld',
      fleetId: fleetId,
      level: AlertLevel.warning,
    ));

    _dockFleet(fleet, player);
  }

  void handleBuild(int playerId, BuildingType buildingType) {
    final player = players[playerId];
    if (player == null) {
      throw EngineException(ErrorCode.serverError, 'Player not found');
    }
    if (player.buildingQueue != null) {
      throw EngineException(ErrorCode.queueFull, 'Build queue full');
    }

    final currentLevel = player.buildings.get(buildingType);
    if (currentLevel >= maxBuildingLevel) {
      throw EngineException(ErrorCode.maxLevelReached, 'Max level reached');
    }
    if (!buildingPrerequisitesMet(buildingType, player.buildings)) {
      throw EngineException(
        ErrorCode.prerequisitesNotMet,
        'Prerequisites not met',
      );
    }

    final targetLevel = currentLevel + 1;
    final cost = buildingCost(buildingType, targetLevel);
    if (!player.resources.canAfford(cost)) {
      throw EngineException(ErrorCode.noResources, 'Insufficient resources');
    }

    player.resources = player.resources.sub(cost);
    // Demo: shorten build times (min 2 ticks for mines) so upgrades feel responsive
    final duration = math.max(
      2,
      buildingTime(buildingType, targetLevel) ~/ 10,
    );
    player.buildingQueue = BuildingQueueItem(
      building: buildingType,
      targetLevel: targetLevel,
      startTick: currentTick,
      endTick: currentTick + duration,
    );
    dirtyPlayers.add(playerId);

    pendingEvents.add(GameEvent.alert(
      tick: currentTick,
      message:
          'Building ${buildingType.label} Lv$targetLevel (${duration}s)',
    ));
  }

  void handleResearch(int playerId, ResearchType tech) {
    final player = players[playerId];
    if (player == null) {
      throw EngineException(ErrorCode.serverError, 'Player not found');
    }
    if (player.researchQueue != null) {
      throw EngineException(ErrorCode.queueFull, 'Research queue full');
    }
    if (player.buildings.researchLab == 0) {
      throw EngineException(ErrorCode.noResearchLab, 'No research lab');
    }

    final currentLevel = player.research.get(tech);
    if (currentLevel >= researchMaxLevel(tech)) {
      throw EngineException(ErrorCode.maxLevelReached, 'Max level reached');
    }
    if (!researchPrerequisitesMet(tech, player.buildings, player.research)) {
      throw EngineException(
        ErrorCode.prerequisitesNotMet,
        'Prerequisites not met',
      );
    }

    final targetLevel = currentLevel + 1;
    final cost = researchCost(tech, targetLevel);
    if (!player.resources.canAfford(cost)) {
      throw EngineException(ErrorCode.noResources, 'Insufficient resources');
    }

    player.resources = player.resources.sub(cost);
    final duration = math.max(3, researchTime(tech, targetLevel) ~/ 10);
    player.researchQueue = ResearchQueueItem(
      tech: tech,
      targetLevel: targetLevel,
      startTick: currentTick,
      endTick: currentTick + duration,
    );
    dirtyPlayers.add(playerId);

    pendingEvents.add(GameEvent.alert(
      tick: currentTick,
      message: 'Researching ${tech.label} Lv$targetLevel (${duration}s)',
    ));
  }

  void handleBuildShip(int playerId, ShipClass shipClass, int count) {
    final player = players[playerId];
    if (player == null) {
      throw EngineException(ErrorCode.serverError, 'Player not found');
    }
    if (player.shipQueue != null) {
      throw EngineException(ErrorCode.queueFull, 'Ship queue full');
    }
    if (player.buildings.shipyard == 0) {
      throw EngineException(ErrorCode.noShipyard, 'No shipyard');
    }
    if (!shipClassUnlocked(shipClass, player.research)) {
      throw EngineException(ErrorCode.shipLocked, 'Ship class locked');
    }

    final unitCost = shipClass.buildCost;
    final totalCost = ResourceAmounts(
      metal: unitCost.metal * count,
      crystal: unitCost.crystal * count,
      deuterium: unitCost.deuterium * count,
    );
    if (!player.resources.canAfford(totalCost)) {
      throw EngineException(ErrorCode.noResources, 'Insufficient resources');
    }

    player.resources = player.resources.sub(totalCost);
    final perShip =
        math.max(2, shipBuildTime(shipClass, player.buildings.shipyard) ~/ 5);
    player.shipQueue = ShipQueueItem(
      shipClass: shipClass,
      count: count,
      built: 0,
      startTick: currentTick,
      endTick: currentTick + perShip,
    );
    dirtyPlayers.add(playerId);

    pendingEvents.add(GameEvent.alert(
      tick: currentTick,
      message: 'Building $count× ${shipClass.label}',
    ));
  }

  void handleAttack(int fleetId, int targetFleetId) {
    final fleet = fleets[fleetId];
    if (fleet == null) {
      throw EngineException(ErrorCode.fleetNotFound, 'Fleet not found');
    }
    if (fleet.ships.isEmpty) {
      throw EngineException(ErrorCode.fleetNotFound, 'No ships');
    }
    if (fleet.status == FleetStatus.inCombat) {
      throw EngineException(ErrorCode.onCooldown, 'Already in combat');
    }
    if (fleet.status == FleetStatus.moving) {
      throw EngineException(ErrorCode.onCooldown, 'Moving');
    }

    var npc = npcFleets[targetFleetId];
    if (npc == null) {
      final sectorKey = fleet.location.toKey();
      final ov = sectorOverrides[sectorKey];
      if (ov?.npcClearedTick != null) {
        throw EngineException(ErrorCode.invalidTarget, 'No hostiles');
      }
      final template = worldGen.generateSector(fleet.location);
      final tmpl = template.npcTemplate;
      if (tmpl != null) {
        npc = _spawnNpcFleet(fleet.location, tmpl);
        _startCombat(fleet, npc);
        return;
      }
      throw EngineException(ErrorCode.invalidTarget, 'Invalid target');
    }

    if (npc.location != fleet.location) {
      throw EngineException(ErrorCode.invalidTarget, 'Target not in sector');
    }
    if (npc.inCombat) {
      throw EngineException(ErrorCode.onCooldown, 'Target already fighting');
    }
    _startCombat(fleet, npc);
  }

  void handleStop(int playerId) {
    for (final fleet in fleets.values) {
      if (fleet.ownerId != playerId) continue;
      if (fleet.status == FleetStatus.moving ||
          fleet.status == FleetStatus.harvesting) {
        fleet.status = FleetStatus.idle;
        fleet.moveTarget = null;
        fleet.moveCooldown = 0;
        dirtyFleets.add(fleet.id);
        pendingEvents.add(GameEvent.alert(
          tick: currentTick,
          message: 'Fleet ${fleet.name} stopped',
          fleetId: fleet.id,
        ));
      }
    }
  }

  void handleCollectSalvage(int fleetId) {
    final fleet = fleets[fleetId];
    if (fleet == null) {
      throw EngineException(ErrorCode.fleetNotFound, 'Fleet not found');
    }
    if (fleet.ships.isEmpty) {
      throw EngineException(ErrorCode.fleetNotFound, 'No ships');
    }
    if (fleet.status == FleetStatus.inCombat ||
        fleet.status == FleetStatus.moving) {
      throw EngineException(ErrorCode.onCooldown, 'Busy');
    }

    final key = fleet.location.toKey();
    final ov = sectorOverrides[key];
    if (ov?.salvage == null) {
      throw EngineException(ErrorCode.noResources, 'No salvage');
    }
    _collectSalvage(fleet);
  }

  void handleCancelBuild(int playerId, QueueType queueType) {
    final player = players[playerId];
    if (player == null) {
      throw EngineException(ErrorCode.serverError, 'Player not found');
    }

    switch (queueType) {
      case QueueType.building:
        final q = player.buildingQueue;
        if (q == null) {
          throw EngineException(ErrorCode.noResources, 'No build in progress');
        }
        final cost = buildingCost(q.building, q.targetLevel);
        player.resources =
            player.resources.add(cost.scale(cancelRefundFraction));
        player.buildingQueue = null;
      case QueueType.ship:
        final q = player.shipQueue;
        if (q == null) {
          throw EngineException(ErrorCode.noResources, 'No ship build');
        }
        final remaining = (q.count - q.built).toDouble();
        final refund =
            q.shipClass.buildCost.scale(remaining).scale(cancelRefundFraction);
        player.resources = player.resources.add(refund);
        player.shipQueue = null;
      case QueueType.research:
        final q = player.researchQueue;
        if (q == null) {
          throw EngineException(ErrorCode.noResources, 'No research');
        }
        final cost = researchCost(q.tech, q.targetLevel);
        player.resources =
            player.resources.add(cost.scale(cancelRefundFraction));
        player.researchQueue = null;
    }
    dirtyPlayers.add(playerId);
  }

  // ── Snapshots ───────────────────────────────────────────────────

  List<GameEvent> drainEvents() {
    final events = List<GameEvent>.from(pendingEvents);
    // Keep events until broadcast clears via tick() clear; network copies.
    return events;
  }

  FullState buildFullState(int playerId) {
    final player = players[playerId];
    if (player == null) {
      throw EngineException(ErrorCode.serverError, 'Player not found');
    }

    final fleetStates = collectPlayerFleets(playerId);
    final sectors = collectVisibleSectors(playerId, fleetStates);
    final hw = buildHomeworldState(player);

    return FullState(
      tick: currentTick,
      player: PlayerState(
        id: player.id,
        name: player.name,
        resources: player.resources,
        homeworld: player.homeworld,
      ),
      fleets: fleetStates,
      homeworld: hw,
      knownSectors: sectors,
    );
  }

  TickUpdate buildTickUpdate(int playerId) {
    final player = players[playerId];
    final fleetStates = collectPlayerFleets(playerId);
    final sectors = collectVisibleSectors(playerId, fleetStates);
    final events = pendingEvents.where((e) => _isEventRelevant(e, playerId)).toList();

    return TickUpdate(
      tick: currentTick,
      player: player == null
          ? null
          : PlayerState(
              id: player.id,
              name: player.name,
              resources: player.resources,
              homeworld: player.homeworld,
            ),
      fleetUpdates: fleetStates.isEmpty ? null : fleetStates,
      sectorUpdates: sectors.isEmpty ? null : sectors,
      homeworldUpdate: player == null ? null : buildHomeworldState(player),
      events: events.isEmpty ? null : events,
    );
  }

  List<FleetState> collectPlayerFleets(int playerId) {
    final list = <FleetState>[];
    for (final fleet in fleets.values) {
      if (fleet.ownerId == playerId) {
        list.add(fleet.toState());
      }
    }
    return list;
  }

  List<SectorState> collectVisibleSectors(
    int playerId,
    List<FleetState> fleetStates,
  ) {
    final seen = <int>{};
    final list = <SectorState>[];

    void addSector(Hex coord) {
      final key = coord.toKey();
      if (seen.contains(key)) return;
      seen.add(key);
      list.add(buildSectorState(coord, playerId));
    }

    for (final fs in fleetStates) {
      addSector(fs.location);
    }

    final player = players[playerId];
    if (player != null) {
      addSector(player.homeworld);
      final range = sensorRange(player.buildings.sensorArray);
      if (range > 0) {
        for (final coord in getSensorRevealedCoords(player.homeworld, range)) {
          addSector(coord);
        }
      }
    }
    return list;
  }

  SectorState buildSectorState(Hex coord, int? excludePlayerId) {
    final template = worldGen.generateSector(coord);
    final neighbors = worldGen.connectedNeighbors(coord);
    final override = sectorOverrides[coord.toKey()];
    final densities =
        SectorOverride.effectiveDensities(override, template);

    final hostiles = <NpcFleetInfo>[];
    for (final npc in npcFleets.values) {
      if (npc.location != coord) continue;
      final counts = <ShipClass, int>{};
      for (final s in npc.ships) {
        counts[s.class_] = (counts[s.class_] ?? 0) + 1;
      }
      hostiles.add(NpcFleetInfo(
        id: npc.id,
        ships: counts.entries
            .map((e) => NpcShipInfo(class_: e.key, count: e.value))
            .toList(),
        behavior: npc.behavior,
      ));
    }

    if (template.npcTemplate != null &&
        hostiles.isEmpty &&
        override?.npcClearedTick == null) {
      final tmpl = template.npcTemplate!;
      hostiles.add(NpcFleetInfo(
        id: 0,
        ships: [
          NpcShipInfo(class_: tmpl.shipClass, count: tmpl.count),
        ],
        behavior: tmpl.behavior,
      ));
    }

    final playerFleetList = <FleetBrief>[];
    for (final fleet in fleets.values) {
      if (fleet.location != coord || fleet.ships.isEmpty) continue;
      if (excludePlayerId != null && fleet.ownerId == excludePlayerId) {
        continue;
      }
      final owner = players[fleet.ownerId];
      var scout = 0, corvette = 0, frigate = 0, cruiser = 0, hauler = 0;
      for (final s in fleet.ships) {
        switch (s.class_) {
          case ShipClass.scout:
            scout++;
          case ShipClass.corvette:
            corvette++;
          case ShipClass.frigate:
            frigate++;
          case ShipClass.cruiser:
            cruiser++;
          case ShipClass.hauler:
            hauler++;
        }
      }
      playerFleetList.add(FleetBrief(
        id: fleet.id,
        ownerName: owner?.name ?? 'Unknown',
        shipCount: fleet.ships.length,
        shipClasses: ShipClassCounts(
          scout: scout,
          corvette: corvette,
          frigate: frigate,
          cruiser: cruiser,
          hauler: hauler,
        ),
      ));
    }

    return SectorState(
      location: coord,
      terrain: template.terrain,
      resources: SectorResources(
        metal: densities.metal,
        crystal: densities.crystal,
        deuterium: densities.deut,
      ),
      connections: neighbors,
      hostiles: hostiles.isEmpty ? null : hostiles,
      playerFleets: playerFleetList.isEmpty ? null : playerFleetList,
      salvage: override?.salvage,
    );
  }

  HomeworldState buildHomeworldState(Player player) {
    final buildings = BuildingType.values
        .map((bt) => BuildingState(
              buildingType: bt,
              level: player.buildings.get(bt),
            ))
        .toList();

    final research = ResearchType.values
        .map((rt) => ResearchState(
              tech: rt,
              level: player.research.get(rt),
            ))
        .toList();

    BuildQueueItem? buildQueue;
    if (player.buildingQueue != null) {
      final q = player.buildingQueue!;
      buildQueue = BuildQueueItem(
        buildingType: q.building,
        targetLevel: q.targetLevel,
        startTick: q.startTick,
        endTick: q.endTick,
      );
    }

    ShipyardQueueItem? shipyardQueue;
    if (player.shipQueue != null) {
      final q = player.shipQueue!;
      shipyardQueue = ShipyardQueueItem(
        shipClass: q.shipClass,
        count: q.count,
        built: q.built,
        startTick: q.startTick,
        endTick: q.endTick,
      );
    }

    ResearchItem? researchActive;
    if (player.researchQueue != null) {
      final q = player.researchQueue!;
      researchActive = ResearchItem(
        tech: q.tech,
        targetLevel: q.targetLevel,
        startTick: q.startTick,
        endTick: q.endTick,
      );
    }

    final dockedShips = <ShipState>[];
    for (final fleet in fleets.values) {
      if (fleet.ownerId == player.id && fleet.location == player.homeworld) {
        dockedShips.addAll(fleet.ships.map((s) => s.toState()));
      }
    }

    return HomeworldState(
      location: player.homeworld,
      buildings: buildings,
      research: research,
      buildQueue: buildQueue,
      shipyardQueue: shipyardQueue,
      researchActive: researchActive,
      dockedShips: dockedShips,
    );
  }

  List<Hex> getSensorRevealedCoords(Hex origin, int maxHops) {
    if (maxHops <= 0) return const [];
    final visited = <int>{origin.toKey()};
    var frontier = <Hex>[origin];
    final result = <Hex>[];

    for (var hop = 0; hop < maxHops; hop++) {
      final next = <Hex>[];
      for (final coord in frontier) {
        for (final n in worldGen.connectedNeighbors(coord)) {
          final key = n.toKey();
          if (visited.contains(key)) continue;
          visited.add(key);
          next.add(n);
          result.add(n);
        }
      }
      frontier = next;
    }
    return result;
  }

  // ── Internals ───────────────────────────────────────────────────

  SectorOverride _ensureOverride(int sectorKey) {
    return sectorOverrides.putIfAbsent(sectorKey, SectorOverride.new);
  }

  void _recordExplored(int playerId, Hex coord) {
    final set = exploredSectors.putIfAbsent(playerId, () => <int>{});
    set.add(coord.toKey());
    for (final n in worldGen.connectedNeighbors(coord)) {
      set.add(n.toKey());
    }
  }

  int _countDeployedFleets(int playerId, Hex homeworld) {
    var count = 0;
    for (final f in fleets.values) {
      if (f.ownerId == playerId &&
          f.location != homeworld &&
          f.ships.isNotEmpty) {
        count++;
      }
    }
    return count;
  }

  void _checkHomeworldDocking(Fleet fleet) {
    final player = players[fleet.ownerId];
    if (player == null) return;
    if (fleet.location != player.homeworld) return;
    _dockFleet(fleet, player);
  }

  void _dockFleet(Fleet fleet, Player player) {
    // Deposit cargo
    player.resources = player.resources.add(fleet.cargo);
    fleet.cargo = ResourceAmounts.zero;

    // Merge other fleets at homeworld into this one
    final mergeIds = <int>[];
    for (final other in fleets.values) {
      if (other.id == fleet.id) continue;
      if (other.ownerId != player.id) continue;
      if (other.location != player.homeworld) continue;
      if (other.ships.isEmpty) continue;

      for (final ship in other.ships) {
        if (fleet.ships.length >= maxShipsPerFleet) break;
        fleet.ships.add(ship);
      }
      player.resources = player.resources.add(other.cargo);
      mergeIds.add(other.id);
    }
    for (final mid in mergeIds) {
      fleets.remove(mid);
      deletedFleetIds.add(mid);
    }

    fleet.fuelMax = fleetFuelMax(fleet, player);
    fleet.fuel = fleet.fuelMax;
    dirtyPlayers.add(player.id);
    dirtyFleets.add(fleet.id);
  }

  void _checkNpcEncounter(Fleet fleet) {
    for (final npc in npcFleets.values) {
      if (npc.location == fleet.location && !npc.inCombat) {
        if (npc.behavior == NpcBehavior.passive) continue;
        _startCombat(fleet, npc);
        return;
      }
    }

    final sectorKey = fleet.location.toKey();
    if (sectorOverrides[sectorKey]?.npcClearedTick != null) return;

    final template = worldGen.generateSector(fleet.location);
    final npcTmpl = template.npcTemplate;
    if (npcTmpl != null && npcTmpl.behavior != NpcBehavior.passive) {
      final spawned = _spawnNpcFleet(fleet.location, npcTmpl);
      _startCombat(fleet, spawned);
    }
  }

  NpcFleet _spawnNpcFleet(Hex location, NpcTemplate tmpl) {
    final id = nextId();
    final stats = tmpl.shipClass.baseStats;
    final m = tmpl.statMultiplier;
    final ships = <Ship>[];
    final count = math.min(tmpl.count, maxNpcShips);
    for (var i = 0; i < count; i++) {
      ships.add(Ship(
        id: nextId(),
        class_: tmpl.shipClass,
        hull: stats.hull * m,
        hullMax: stats.hull * m,
        shield: stats.shield * m,
        shieldMax: stats.shield * m,
        weaponPower: stats.weapon * m,
        speed: stats.speed,
      ));
    }
    final npc = NpcFleet(
      id: id,
      location: location,
      ships: ships,
      behavior: tmpl.behavior,
      homeSector: location,
    );
    npcFleets[id] = npc;
    return npc;
  }

  void _startCombat(Fleet fleet, NpcFleet npc) {
    // Join existing combat in sector
    for (final combat in activeCombats.values) {
      if (combat.sector == fleet.location) {
        if (!combat.hasPlayerFleet(fleet.id)) {
          combat.addPlayerFleet(fleet.id);
          fleet.status = FleetStatus.inCombat;
        }
        if (!combat.hasNpcFleet(npc.id)) {
          combat.addNpcFleet(npc.id);
          npc.inCombat = true;
          if (npc.ships.isNotEmpty) {
            combat.npcValue =
                combat.npcValue.add(npc.ships.first.class_.buildCost);
          }
        }
        pendingEvents.add(GameEvent.combatStarted(
          tick: currentTick,
          playerFleetId: fleet.id,
          enemyFleetId: npc.id,
          sector: fleet.location,
        ));
        _enrollAllPlayerFleetsInSectorCombat(combat);
        return;
      }
    }

    final combatId = nextId();
    final combat = Combat(
      id: combatId,
      sector: fleet.location,
      npcValue: npc.ships.isEmpty
          ? ResourceAmounts.zero
          : npc.ships.first.class_.buildCost,
    );
    combat.addPlayerFleet(fleet.id);
    combat.addNpcFleet(npc.id);
    activeCombats[combatId] = combat;
    fleet.status = FleetStatus.inCombat;
    npc.inCombat = true;

    pendingEvents.add(GameEvent.combatStarted(
      tick: currentTick,
      playerFleetId: fleet.id,
      enemyFleetId: npc.id,
      sector: fleet.location,
    ));
    pendingEvents.add(GameEvent.alert(
      tick: currentTick,
      message: 'Combat started in ${fleet.location}!',
      sector: fleet.location,
      level: AlertLevel.warning,
    ));
    _enrollAllPlayerFleetsInSectorCombat(combat);
  }

  void _enrollAllPlayerFleetsInSectorCombat(Combat combat) {
    for (final f in fleets.values) {
      if (f.location != combat.sector) continue;
      if (f.status == FleetStatus.inCombat) continue;
      if (f.ships.isEmpty) continue;
      if (combat.hasPlayerFleet(f.id)) continue;
      combat.addPlayerFleet(f.id);
      f.status = FleetStatus.inCombat;
    }
  }

  void _collectSalvage(Fleet fleet) {
    final key = fleet.location.toKey();
    final ov = sectorOverrides[key];
    if (ov?.salvage == null) return;
    final salvage = ov!.salvage!;

    final maxCargo = fleetCargoCapacity(fleet);
    var remaining =
        maxCargo - (fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium);
    if (remaining <= 0) return;

    final metal = math.min(salvage.metal, remaining);
    remaining -= metal;
    final crystal = math.min(salvage.crystal, remaining);
    remaining -= crystal;
    final deut = math.min(salvage.deuterium, remaining);

    final collected = ResourceAmounts(
      metal: metal,
      crystal: crystal,
      deuterium: deut,
    );
    fleet.cargo = fleet.cargo.add(collected);
    ov.salvage = null;
    ov.salvageDespawnTick = null;
    dirtyFleets.add(fleet.id);
    dirtySectors.add(key);

    pendingEvents.add(GameEvent.salvageCollected(
      tick: currentTick,
      fleetId: fleet.id,
      resources: collected,
    ));
  }

  void _dropSalvage(Hex sector, ResourceAmounts fleetValue) {
    final key = sector.toKey();
    final ov = _ensureOverride(key);
    ov.salvage = fleetValue.scale(salvageFraction);
    ov.salvageDespawnTick = currentTick + salvageDespawnTicks;
    dirtySectors.add(key);
  }

  void _addShipToHomeworld(Player player, ShipClass shipClass) {
    Fleet? docked;
    for (final f in fleets.values) {
      if (f.ownerId == player.id && f.location == player.homeworld) {
        docked = f;
        break;
      }
    }

    if (docked == null) {
      final fleetId = nextId();
      docked = Fleet(
        id: fleetId,
        ownerId: player.id,
        name: player.nextFleetName(),
        location: player.homeworld,
        status: FleetStatus.idle,
      );
      fleets[fleetId] = docked;
    }

    if (docked.ships.length >= maxShipsPerFleet) return;

    final base = shipClass.baseStats;
    final stats = applyResearchToStats(base, player.research);
    docked.ships.add(Ship.fromStats(nextId(), shipClass, stats));
    docked.fuelMax = fleetFuelMax(docked, player);
    docked.fuel = docked.fuelMax;
    dirtyFleets.add(docked.id);
  }

  void _recalculatePlayerFleetFuel(Player player) {
    for (final fleet in fleets.values) {
      if (fleet.ownerId != player.id) continue;
      if (fleet.ships.isEmpty) continue;
      final newMax = fleetFuelMax(fleet, player);
      if (newMax > fleet.fuelMax) {
        fleet.fuel += newMax - fleet.fuelMax;
        fleet.fuelMax = newMax;
        dirtyFleets.add(fleet.id);
      }
    }
  }

  bool _isEventRelevant(GameEvent event, int playerId) {
    final kind = event.kind;
    if (kind.containsKey('alert')) return true;
    if (kind.containsKey('combat_round')) return true;

    bool ownFleet(int? fid) {
      if (fid == null) return false;
      return fleets[fid]?.ownerId == playerId;
    }

    bool hasFleetAt(Hex? sector) {
      if (sector == null) return false;
      for (final f in fleets.values) {
        if (f.ownerId == playerId &&
            f.location == sector &&
            f.ships.isNotEmpty) {
          return true;
        }
      }
      return false;
    }

    for (final entry in kind.entries) {
      final data = entry.value;
      if (data is! Map) return true;
      final map = Map<String, dynamic>.from(data);

      switch (entry.key) {
        case 'sector_entered':
        case 'fleet_arrived':
        case 'resource_harvested':
        case 'salvage_collected':
          return ownFleet((map['fleet_id'] as num?)?.toInt());
        case 'building_completed':
        case 'research_completed':
        case 'ship_built':
          final pid = (map['player_id'] as num?)?.toInt();
          return pid == null || pid == playerId;
        case 'combat_started':
          return ownFleet((map['player_fleet_id'] as num?)?.toInt()) ||
              hasFleetAt(Hex.fromJson(map['sector'] as Map<String, dynamic>?));
        case 'combat_ended':
          return hasFleetAt(
            Hex.fromJson(map['sector'] as Map<String, dynamic>?),
          );
        case 'ship_destroyed':
          if (map['is_npc'] != true &&
              ownFleet((map['owner_fleet_id'] as num?)?.toInt())) {
            return true;
          }
          final f = fleets[(map['owner_fleet_id'] as num?)?.toInt()];
          if (f != null && hasFleetAt(f.location)) return true;
          return false;
        case 'fleet_destroyed':
          return map['is_npc'] == true ||
              ownFleet((map['fleet_id'] as num?)?.toInt());
        default:
          return true;
      }
    }
    return true;
  }
}

// ── Fleet helpers ─────────────────────────────────────────────────

int fleetMoveCooldown(Fleet fleet, ResearchLevels? research) {
  var minSpeed = 255;
  for (final ship in fleet.ships) {
    if (ship.speed < minSpeed) minSpeed = ship.speed;
  }
  if (minSpeed == 0) return moveBaseCooldown;
  final base = (moveBaseCooldown * 10) ~/ minSpeed;
  if (research != null) {
    final reduction = navigationCooldownReduction(research.navigation);
    return math.max(0, base - reduction);
  }
  return base;
}

double fleetFuelCost(Fleet fleet, ResearchLevels? research) {
  var totalMass = 0.0;
  for (final ship in fleet.ships) {
    totalMass += ship.hullMax;
  }
  final base = totalMass * fuelRatePerMass;
  if (research != null) {
    return base * fuelRateModifier(research.fuelEfficiency);
  }
  return base;
}

double fleetHarvestPower(Fleet fleet, ResearchLevels? research) {
  var power = 0.0;
  for (final ship in fleet.ships) {
    power += switch (ship.class_) {
      ShipClass.hauler => 5.0,
      ShipClass.scout => 1.0,
      _ => 0.5,
    };
  }
  if (research != null) {
    return power * harvestRateModifier(research.harvestingEfficiency);
  }
  return power;
}

double fleetFuelMax(Fleet fleet, Player player) {
  var totalFuel = 0.0;
  for (final ship in fleet.ships) {
    totalFuel += ship.class_.baseStats.fuel;
  }
  return totalFuel *
      fuelCapacityModifier(player.research.extendedFuelTanks) *
      fuelDepotModifier(player.buildings.fuelDepot);
}

double fleetCargoCapacity(Fleet fleet) {
  var cap = 0.0;
  for (final ship in fleet.ships) {
    cap += ship.class_.baseStats.cargo;
  }
  return cap;
}
