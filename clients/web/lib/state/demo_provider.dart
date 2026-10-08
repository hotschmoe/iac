import 'dart:async';
import 'dart:math';

import '../protocol/protocol.dart';

/// Offline stand-in for the server: produces the same protocol messages
/// (full_state, tick_update, error) so the UI has a single render path.
/// It simulates just enough (move, scan, harvest, recall, stop, queues,
/// policies) to click around; combat, salvage and derelicts are not modelled.
/// Its catalog uses a rough cost model of its own, only meant to look like the
/// real one; a live server sends the real numbers.
class DemoProvider {
  static const _playerId = 1;
  static const _home = Hex(4, -2);

  final void Function(List<ServerMessage>) _emit;
  final Random _rng = Random(7);
  Timer? _timer;

  int _tick = 4821;
  final List<GameEvent> _pending = [];
  final Map<int, SectorState> _sectors = {};

  late Resources _resources = const Resources(metal: 12847, crystal: 6214, deuterium: 2105);
  late List<FleetState> _fleets;
  late HomeworldState _hw;

  DemoProvider(this._emit) {
    final ships = [
      _ship(1011, ShipClass.scout, 80),
      _ship(1012, ShipClass.corvette, 100),
      _ship(1013, ShipClass.hauler, 60),
    ];
    _fleets = [
      FleetState(
        id: 101,
        location: const Hex(5, -3),
        state: FleetStatus.harvesting,
        ships: ships,
        cargo: const Resources(metal: 98, crystal: 61, deuterium: 28),
        cargoCapacity: 270,
        fuel: 271,
        fuelMax: 500,
        jumpFuel: 25,
        homeFuel: 50,
      ),
      FleetState(
        id: 102,
        location: _home,
        state: FleetStatus.docked,
        ships: [_ship(1021, ShipClass.frigate, 40)],
        cargo: const Resources(),
        cargoCapacity: 30,
        fuel: 500,
        fuelMax: 500,
      ),
      FleetState(
        id: 103,
        location: const Hex(3, -1),
        state: FleetStatus.idle,
        ships: [_ship(1031, ShipClass.scout, 100)],
        cargo: const Resources(),
        cargoCapacity: 20,
        fuel: 440,
        fuelMax: 500,
        jumpFuel: 15,
        homeFuel: 45,
        policy: PolicyPreset.prospect,
      ),
    ];
    final buildings = [
      for (final t in BuildingType.values)
        BuildingState(
          buildingType: t,
          level: switch (t) {
            BuildingType.metalMine => 4,
            BuildingType.crystalMine => 3,
            BuildingType.deuteriumSynthesizer => 2,
            BuildingType.shipyard => 1,
            BuildingType.researchLab => 1,
            _ => 0,
          },
        ),
    ];
    final research = [
      for (final t in ResearchType.values)
        ResearchState(tech: t, level: (t == ResearchType.fuelEfficiency || t == ResearchType.reinforcedHulls) ? 1 : 0),
    ];
    _hw = HomeworldState(
      location: _home,
      production: _production(buildings),
      buildings: buildings,
      research: research,
      catalog: _catalogFor(buildings, research),
      buildQueue: BuildQueueItem(
        buildingType: BuildingType.crystalMine,
        targetLevel: 4,
        startTick: _tick - 100,
        endTick: _tick + 140,
      ),
      shipyardQueue: ShipyardQueueItem(
        shipClass: ShipClass.corvette,
        count: 2,
        built: 1,
        startTick: _tick - 40,
        endTick: _tick + 80,
      ),
      researchActive: ResearchItem(
        tech: ResearchType.fuelEfficiency,
        targetLevel: 2,
        startTick: _tick - 30,
        endTick: _tick + 400,
      ),
      dockedShips: [_ship(2001, ShipClass.cruiser, 100)],
    );
    for (final f in _fleets) {
      _reveal(f.location, 2);
    }
    _reveal(_home, 3);
    // One hostile patrol next to Alpha, for atmosphere.
    final alpha = _fleets.first.location;
    final next = _sectors[alpha.toKey()]!.connections.first.toKey();
    final s = _sectors[next]!;
    _sectors[next] = SectorState(
      location: s.location,
      terrain: s.terrain,
      resources: s.resources,
      connections: s.connections,
      hostiles: const [
        NpcFleetInfo(
          id: 9001,
          ships: [NpcShipInfo(shipClass: ShipClass.corvette, count: 3)],
          behavior: NpcBehavior.patrol,
        ),
      ],
    );
  }

  ShipState _ship(int id, ShipClass c, double hull) =>
      ShipState(id: id, shipClass: c, hull: hull, hullMax: 100, shield: 20, shieldMax: 20, weaponPower: 5);

  // ── World ─────────────────────────────────────────────────────

  static int _hash(int a, int b) => ((a * 73856093) ^ (b * 19349663) ^ 0x5bd1e9) & 0x7fffffff;

  /// Symmetric pseudo-random lanes: roughly 1 in 7 neighbor links is closed.
  static bool _edge(Hex a, Hex b) {
    final lo = a.toKey() < b.toKey() ? a : b;
    final hi = identical(lo, a) ? b : a;
    return _hash(lo.toKey(), hi.toKey()) % 7 != 0;
  }

  SectorState _gen(Hex h) {
    final x = _hash(h.q, h.r);
    final terrain = TerrainType.values[x % 5];
    Density d(int shift) =>
        terrain == TerrainType.empty ? Density.none : Density.values[((x >> shift) % 4) + 1];
    return SectorState(
      location: h,
      terrain: terrain,
      resources: SectorResources(metal: d(3), crystal: d(6), deuterium: d(9)),
      connections: [for (final n in h.neighbors()) if (_edge(h, n)) n],
    );
  }

  void _reveal(Hex center, int radius) {
    for (final h in hexSpiral(center, radius)) {
      _sectors.putIfAbsent(h.toKey(), () => _gen(h));
    }
  }

  // ── Lifecycle ─────────────────────────────────────────────────

  GameState initial() => GameState(
        tick: _tick,
        player: _player(),
        fleets: List.of(_fleets),
        homeworld: _hw,
        knownSectors: _sectors.values.toList(),
      );

  PlayerState _player() => PlayerState(id: _playerId, name: 'Demo', resources: _resources, homeworld: _home);

  void start() {
    _timer?.cancel();
    _timer = Timer.periodic(const Duration(seconds: 1), (_) => _step());
  }

  void stop() {
    _timer?.cancel();
    _timer = null;
  }

  void _step() {
    _tick++;
    final p = _hw.production;
    _resources = Resources(
      metal: _resources.metal + p.metal,
      crystal: _resources.crystal + p.crystal,
      deuterium: _resources.deuterium + p.deuterium,
    );

    _fleets = [
      for (final f in _fleets)
        if (f.state == FleetStatus.harvesting && f.cargo.total < f.cargoCapacity)
          _copy(f, cargo: Resources(
            metal: f.cargo.metal + 0.8,
            crystal: f.cargo.crystal + 0.3,
            deuterium: f.cargo.deuterium + 0.1,
          ))
        else
          f,
    ];

    final b = _hw.buildQueue;
    if (b != null && _tick >= b.endTick) {
      _pending.add(GameEvent(
        tick: _tick,
        kind: BuildingCompletedEvent(buildingType: b.buildingType, newLevel: b.targetLevel),
      ));
      _hw = _hwWith(
        buildings: [
          for (final e in _hw.buildings)
            e.buildingType == b.buildingType ? BuildingState(buildingType: e.buildingType, level: b.targetLevel) : e,
        ],
        clearBuild: true,
      );
    }
    final r = _hw.researchActive;
    if (r != null && _tick >= r.endTick) {
      _pending.add(GameEvent(
        tick: _tick,
        kind: ResearchCompletedEvent(tech: r.tech, newLevel: r.targetLevel),
      ));
      _hw = _hwWith(
        research: [
          for (final e in _hw.research)
            e.tech == r.tech ? ResearchState(tech: e.tech, level: r.targetLevel) : e,
        ],
        clearResearch: true,
      );
    }

    if (_tick % 15 == 0) {
      _pending.add(GameEvent(
        tick: _tick,
        kind: _rng.nextBool()
            ? ResourceHarvestedEvent(
                fleetId: 101,
                resourceType: HarvestResource.metal,
                amount: 10.0 + _rng.nextInt(30),
              )
            : const AlertEvent(level: AlertLevel.info, message: 'Demo mode: no server connected'),
      ));
    }

    _emit([_tickUpdate()]);
  }

  TickUpdate _tickUpdate({bool sectors = false}) {
    final events = List.of(_pending);
    _pending.clear();
    return TickUpdate(
      tick: _tick,
      player: _player(),
      fleets: List.of(_fleets),
      homeworldUpdate: _hw,
      sectorUpdates: sectors ? _sectors.values.toList() : null,
      events: events.isEmpty ? null : events,
    );
  }

  // ── Commands ──────────────────────────────────────────────────

  FleetState _copy(
    FleetState f, {
    Hex? location,
    FleetStatus? state,
    Resources? cargo,
    PolicyPreset? policy,
    bool clearPolicy = false,
  }) =>
      FleetState(
        id: f.id,
        location: location ?? f.location,
        state: state ?? f.state,
        ships: f.ships,
        cargo: cargo ?? f.cargo,
        cargoCapacity: f.cargoCapacity,
        fuel: f.fuel,
        fuelMax: f.fuelMax,
        jumpFuel: f.jumpFuel,
        homeFuel: f.homeFuel,
        cooldownRemaining: f.cooldownRemaining,
        policy: clearPolicy ? null : (policy ?? f.policy),
      );

  HomeworldState _hwWith({
    List<BuildingState>? buildings,
    List<ResearchState>? research,
    BuildQueueItem? build,
    ShipyardQueueItem? ship,
    ResearchItem? researchItem,
    bool clearBuild = false,
    bool clearShip = false,
    bool clearResearch = false,
  }) =>
      HomeworldState(
        location: _hw.location,
        production: _production(buildings ?? _hw.buildings),
        buildings: buildings ?? _hw.buildings,
        research: research ?? _hw.research,
        buildQueue: clearBuild ? null : (build ?? _hw.buildQueue),
        shipyardQueue: clearShip ? null : (ship ?? _hw.shipyardQueue),
        researchActive: clearResearch ? null : (researchItem ?? _hw.researchActive),
        dockedShips: _hw.dockedShips,
        catalog: _catalogFor(buildings ?? _hw.buildings, research ?? _hw.research),
      );

  // Per-level cost and prerequisite tables for the demo catalog.
  static const _buildingBase = {
    BuildingType.metalMine: Resources(metal: 60, crystal: 15),
    BuildingType.crystalMine: Resources(metal: 48, crystal: 24),
    BuildingType.deuteriumSynthesizer: Resources(metal: 225, crystal: 75),
    BuildingType.shipyard: Resources(metal: 200, crystal: 100, deuterium: 50),
    BuildingType.researchLab: Resources(metal: 100, crystal: 200, deuterium: 50),
    BuildingType.fuelDepot: Resources(metal: 150, crystal: 50, deuterium: 100),
    BuildingType.sensorArray: Resources(metal: 100, crystal: 150, deuterium: 75),
    BuildingType.defenseGrid: Resources(metal: 300, crystal: 200, deuterium: 100),
  };
  static const _buildingNeeds = {
    BuildingType.shipyard: (BuildingType.metalMine, 2),
    BuildingType.researchLab: (BuildingType.crystalMine, 2),
    BuildingType.fuelDepot: (BuildingType.deuteriumSynthesizer, 2),
    BuildingType.sensorArray: (BuildingType.researchLab, 1),
    BuildingType.defenseGrid: (BuildingType.shipyard, 3),
  };
  static const _shipCost = {
    ShipClass.scout: Resources(metal: 200, crystal: 50, deuterium: 30),
    ShipClass.corvette: Resources(metal: 400, crystal: 100, deuterium: 60),
    ShipClass.frigate: Resources(metal: 1000, crystal: 400, deuterium: 200),
    ShipClass.cruiser: Resources(metal: 3000, crystal: 1500, deuterium: 800),
    ShipClass.hauler: Resources(metal: 600, crystal: 200, deuterium: 150),
  };
  static const _shipTicks = {
    ShipClass.scout: 30,
    ShipClass.corvette: 60,
    ShipClass.frigate: 120,
    ShipClass.cruiser: 240,
    ShipClass.hauler: 90,
  };
  static const _shipTech = {
    ShipClass.corvette: ResearchType.corvetteTech,
    ShipClass.frigate: ResearchType.frigateTech,
    ShipClass.cruiser: ResearchType.cruiserTech,
    ShipClass.hauler: ResearchType.haulerTech,
  };

  static Resources _times(Resources r, num n) =>
      Resources(metal: r.metal * n, crystal: r.crystal * n, deuterium: r.deuterium * n);

  static HomeworldCatalog _catalogFor(List<BuildingState> buildings, List<ResearchState> research) {
    int bl(BuildingType t) => buildings.firstWhere((b) => b.buildingType == t).level;
    int rl(ResearchType t) => research.firstWhere((r) => r.tech == t).level;
    Requirement needBuilding(BuildingType t, int l) =>
        Requirement(name: t.label, need: l, have: bl(t), met: bl(t) >= l);

    return HomeworldCatalog(
      buildings: [
        for (final t in BuildingType.values)
          BuildingOption(
            buildingType: t,
            level: bl(t),
            maxLevel: 20,
            next: bl(t) >= 20
                ? null
                : UpgradeStep(level: bl(t) + 1, cost: _times(_buildingBase[t]!, bl(t) + 1), ticks: 30 * (bl(t) + 1)),
            requires: [
              if (_buildingNeeds[t] case (final b, final l)) needBuilding(b, l),
            ],
          ),
      ],
      research: [
        for (final t in ResearchType.values)
          ResearchOption(
            tech: t,
            level: rl(t),
            maxLevel: _shipTech.containsValue(t) ? 1 : 5,
            next: rl(t) >= (_shipTech.containsValue(t) ? 1 : 5)
                ? null
                : UpgradeStep(
                    level: rl(t) + 1,
                    cost: _times(const Resources(metal: 200, crystal: 150, deuterium: 100), rl(t) + 1),
                    ticks: 60 * (rl(t) + 1),
                  ),
            requires: [
              needBuilding(BuildingType.researchLab, 1),
              if (t == ResearchType.corvetteTech) needBuilding(BuildingType.shipyard, 2),
            ],
          ),
      ],
      ships: [
        for (final c in ShipClass.values)
          ShipOption(
            shipClass: c,
            unitCost: _shipCost[c]!,
            ticksPerShip: _shipTicks[c]!,
            requires: [
              needBuilding(BuildingType.shipyard, 1),
              if (_shipTech[c] case final t?) Requirement(name: t.label, need: 1, have: rl(t), met: rl(t) >= 1),
            ],
          ),
      ],
    );
  }

  /// Spend [cost] if the stockpile covers it; otherwise the error to send.
  ServerMessage? _pay(Resources cost) {
    if (_resources.metal < cost.metal ||
        _resources.crystal < cost.crystal ||
        _resources.deuterium < cost.deuterium) {
      return _err(ErrorCode.noResources, 'Not enough resources');
    }
    _resources = Resources(
      metal: _resources.metal - cost.metal,
      crystal: _resources.crystal - cost.crystal,
      deuterium: _resources.deuterium - cost.deuterium,
    );
    return null;
  }

  /// The demo plays the server: mine output at the given building levels.
  static Resources _production(List<BuildingState> buildings) {
    double rate(BuildingType t, double base) {
      final l = buildings.firstWhere((b) => b.buildingType == t).level;
      return l == 0 ? 0 : base * l * pow(1.1, l);
    }

    return Resources(
      metal: rate(BuildingType.metalMine, 0.5),
      crystal: rate(BuildingType.crystalMine, 0.3),
      deuterium: rate(BuildingType.deuteriumSynthesizer, 0.15),
    );
  }

  FleetState? _find(int id) {
    for (final f in _fleets) {
      if (f.id == id) return f;
    }
    return null;
  }

  void _setFleet(FleetState f) {
    _fleets = [for (final e in _fleets) e.id == f.id ? f : e];
  }

  ServerMessage _err(ErrorCode c, String m) => ErrorMessage(code: c, message: m);

  /// Apply a client message locally; returns what the server would answer.
  List<ServerMessage> handle(ClientMessage msg) {
    switch (msg) {
      case CommandMessage(:final command):
        return _command(command);
      case PolicyUpdate(:final fleetId, :final preset):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        _setFleet(_copy(f, policy: preset, clearPolicy: preset == PolicyPreset.manual));
        return [_tickUpdate()];
      case RequestFullState():
        return [initial()];
      case AuthRequest():
        return const [];
    }
  }

  List<ServerMessage> _command(Command c) {
    FleetState? fleet(int id) => _find(id);
    switch (c) {
      case MoveCommand(:final fleetId, :final target):
        final f = fleet(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        final here = _sectors[f.location.toKey()];
        if (here == null || !here.connections.contains(target)) {
          return [_err(ErrorCode.noConnection, 'No lane from ${f.location} to $target')];
        }
        _reveal(target, 1);
        _setFleet(_copy(f, location: target, state: FleetStatus.idle));
        _pending
          ..add(GameEvent(tick: _tick, kind: FleetArrivedEvent(fleetId: fleetId, sector: target)))
          ..add(GameEvent(tick: _tick, kind: SectorEnteredEvent(fleetId: fleetId, sector: target, firstVisit: false)));
        return [_tickUpdate(sectors: true)];
      case HarvestCommand(:final fleetId):
        final f = fleet(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        _setFleet(_copy(f, state: FleetStatus.harvesting));
        return [_tickUpdate()];
      case RecallCommand(:final fleetId):
        final f = fleet(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        _setFleet(_copy(f, location: _home, state: FleetStatus.docked));
        return [_tickUpdate()];
      case StopCommand(:final fleetId):
        final f = fleet(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        _setFleet(_copy(f, state: FleetStatus.idle));
        return [_tickUpdate()];
      case ScanCommand(:final fleetId):
        final f = fleet(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        final before = _sectors.length;
        _reveal(f.location, 2);
        final far = hexRing(f.location, 3)[_rng.nextInt(18)];
        _pending.add(GameEvent(
          tick: _tick,
          kind: ScanCompletedEvent(
            fleetId: fleetId,
            sector: f.location,
            sectorsRevealed: _sectors.length - before,
            hostilesDetected: 0,
            signals: [SignalContact(sector: far, signal: SignalKind.richOre)],
          ),
        ));
        return [_tickUpdate(sectors: true)];
      case BuildCommand(:final buildingType):
        if (_hw.buildQueue != null) return [_err(ErrorCode.queueFull, 'Build queue busy')];
        final o = _hw.catalog.buildings.firstWhere((o) => o.buildingType == buildingType);
        if (o.next == null) return [_err(ErrorCode.maxLevelReached, '${buildingType.label} is maxed')];
        if (o.requires.any((r) => !r.met)) return [_err(ErrorCode.prerequisitesNotMet, 'Prerequisites not met')];
        final broke = _pay(o.next!.cost);
        if (broke != null) return [broke];
        _hw = _hwWith(
          build: BuildQueueItem(
            buildingType: buildingType,
            targetLevel: o.next!.level,
            startTick: _tick,
            endTick: _tick + o.next!.ticks,
          ),
        );
        return [_tickUpdate()];
      case ResearchCommand(:final tech):
        if (_hw.researchActive != null) return [_err(ErrorCode.queueFull, 'Lab busy')];
        final o = _hw.catalog.research.firstWhere((o) => o.tech == tech);
        if (o.requires.first.met == false) return [_err(ErrorCode.noResearchLab, 'Build a research lab first')];
        if (o.next == null) return [_err(ErrorCode.maxLevelReached, '${tech.label} is maxed')];
        if (o.requires.any((r) => !r.met)) return [_err(ErrorCode.prerequisitesNotMet, 'Prerequisites not met')];
        final broke = _pay(o.next!.cost);
        if (broke != null) return [broke];
        _hw = _hwWith(
          researchItem: ResearchItem(
            targetLevel: o.next!.level,
            tech: tech,
            startTick: _tick,
            endTick: _tick + o.next!.ticks,
          ),
        );
        return [_tickUpdate()];
      case BuildShipCommand(:final shipClass, :final count):
        if (_hw.shipyardQueue != null) return [_err(ErrorCode.queueFull, 'Shipyard busy')];
        final o = _hw.catalog.ships.firstWhere((o) => o.shipClass == shipClass);
        if (!o.requires.first.met) return [_err(ErrorCode.noShipyard, 'Build a shipyard first')];
        if (o.requires.any((r) => !r.met)) return [_err(ErrorCode.shipLocked, 'Ship class not unlocked')];
        final broke = _pay(_times(o.unitCost, count));
        if (broke != null) return [broke];
        _hw = _hwWith(
          ship: ShipyardQueueItem(
            shipClass: shipClass,
            count: count,
            built: 0,
            startTick: _tick,
            endTick: _tick + o.ticksPerShip * count,
          ),
        );
        return [_tickUpdate()];
      case CancelBuildCommand(:final queueType):
        _hw = _hwWith(
          clearBuild: queueType == QueueType.building,
          clearShip: queueType == QueueType.ship,
          clearResearch: queueType == QueueType.research,
        );
        return [_tickUpdate()];
      case SplitCommand(:final fleetId, :final shipIds):
        final f = fleet(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        final taken = [for (final s in f.ships) if (shipIds.contains(s.id)) s];
        if (taken.isEmpty || taken.length >= f.ships.length) {
          return [_err(ErrorCode.invalidTarget, 'Split needs some ships, and one must stay')];
        }
        if (_fleets.length >= 8) return [_err(ErrorCode.fleetLimitReached, 'Fleet limit reached')];
        final share = taken.length / f.ships.length;
        final newId = 1 + _fleets.map((e) => e.id).reduce(max);
        final rest = [for (final s in f.ships) if (!shipIds.contains(s.id)) s];
        _fleets = [
          for (final e in _fleets)
            if (e.id == fleetId)
              FleetState(
                id: e.id,
                location: e.location,
                state: e.state,
                ships: rest,
                cargo: e.cargo,
                cargoCapacity: e.cargoCapacity * (1 - share),
                fuel: e.fuel * (1 - share),
                fuelMax: e.fuelMax * (1 - share),
                policy: e.policy,
              )
            else
              e,
          FleetState(
            id: newId,
            location: f.location,
            state: FleetStatus.idle,
            ships: taken,
            cargo: const Resources(),
            cargoCapacity: f.cargoCapacity * share,
            fuel: f.fuel * share,
            fuelMax: f.fuelMax * share,
          ),
        ];
        _pending.add(GameEvent(
          tick: _tick,
          kind: AlertEvent(
            level: AlertLevel.info,
            message: 'fleet $fleetId split: ${taken.length} ship(s) now fly as fleet $newId',
            fleetId: newId,
          ),
        ));
        return [_tickUpdate()];
      case MergeCommand(:final fleetId, :final otherFleetId):
        final a = fleet(fleetId);
        final b = fleet(otherFleetId);
        if (a == null || b == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        if (a.id == b.id) return [_err(ErrorCode.invalidTarget, 'A fleet cannot merge with itself')];
        if (a.location != b.location) return [_err(ErrorCode.notInSector, 'Fleets are not in the same sector')];
        _fleets = [
          for (final e in _fleets)
            if (e.id == fleetId)
              FleetState(
                id: a.id,
                location: a.location,
                state: a.state,
                ships: [...a.ships, ...b.ships],
                cargo: Resources(
                  metal: a.cargo.metal + b.cargo.metal,
                  crystal: a.cargo.crystal + b.cargo.crystal,
                  deuterium: a.cargo.deuterium + b.cargo.deuterium,
                ),
                cargoCapacity: a.cargoCapacity + b.cargoCapacity,
                fuel: a.fuel + b.fuel,
                fuelMax: a.fuelMax + b.fuelMax,
                policy: a.policy,
              )
            else if (e.id != otherFleetId)
              e,
        ];
        _pending.add(GameEvent(
          tick: _tick,
          kind: AlertEvent(
            level: AlertLevel.info,
            message: 'fleet $otherFleetId merged into fleet $fleetId',
            fleetId: fleetId,
          ),
        ));
        return [_tickUpdate()];
      case AttackCommand() || CollectSalvageCommand() || ExploreSiteCommand():
        return [_err(ErrorCode.invalidCommand, 'Demo mode: not simulated')];
    }
  }
}
