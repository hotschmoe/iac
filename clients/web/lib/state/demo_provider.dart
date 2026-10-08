import 'dart:async';
import 'dart:math';

import '../protocol/protocol.dart';

/// Offline stand-in for the server: produces the same protocol messages
/// (full_state, tick_update, error) so the UI has a single render path.
/// It simulates just enough (move, scan, harvest, recall, stop, queues,
/// policies) to click around; combat, salvage and derelicts are not modelled.
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
        fuel: 271,
        fuelMax: 500,
      ),
      FleetState(
        id: 102,
        location: _home,
        state: FleetStatus.docked,
        ships: [_ship(1021, ShipClass.frigate, 40)],
        cargo: const Resources(),
        fuel: 500,
        fuelMax: 500,
      ),
      FleetState(
        id: 103,
        location: const Hex(3, -1),
        state: FleetStatus.idle,
        ships: [_ship(1031, ShipClass.scout, 100)],
        cargo: const Resources(),
        fuel: 440,
        fuelMax: 500,
        policy: PolicyPreset.prospect,
      ),
    ];
    _hw = HomeworldState(
      location: _home,
      buildings: [
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
      ],
      research: [
        for (final t in ResearchType.values)
          ResearchState(tech: t, level: (t == ResearchType.fuelEfficiency || t == ResearchType.reinforcedHulls) ? 1 : 0),
      ],
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
    double rate(BuildingType t, double base) {
      final l = _hw.buildingLevel(t);
      return l == 0 ? 0 : base * l * pow(1.1, l);
    }

    _resources = Resources(
      metal: _resources.metal + rate(BuildingType.metalMine, 0.5),
      crystal: _resources.crystal + rate(BuildingType.crystalMine, 0.3),
      deuterium: _resources.deuterium + rate(BuildingType.deuteriumSynthesizer, 0.15),
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
        fuel: f.fuel,
        fuelMax: f.fuelMax,
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
        buildings: buildings ?? _hw.buildings,
        research: research ?? _hw.research,
        buildQueue: clearBuild ? null : (build ?? _hw.buildQueue),
        shipyardQueue: clearShip ? null : (ship ?? _hw.shipyardQueue),
        researchActive: clearResearch ? null : (researchItem ?? _hw.researchActive),
        dockedShips: _hw.dockedShips,
      );

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
        _hw = _hwWith(
          build: BuildQueueItem(
            buildingType: buildingType,
            targetLevel: _hw.buildingLevel(buildingType) + 1,
            startTick: _tick,
            endTick: _tick + 90,
          ),
        );
        return [_tickUpdate()];
      case ResearchCommand(:final tech):
        if (_hw.researchActive != null) return [_err(ErrorCode.queueFull, 'Lab busy')];
        _hw = _hwWith(
          researchItem: ResearchItem(targetLevel: 1, tech: tech, startTick: _tick, endTick: _tick + 120),
        );
        return [_tickUpdate()];
      case BuildShipCommand(:final shipClass, :final count):
        if (_hw.shipyardQueue != null) return [_err(ErrorCode.queueFull, 'Shipyard busy')];
        _hw = _hwWith(
          ship: ShipyardQueueItem(
            shipClass: shipClass,
            count: count,
            built: 0,
            startTick: _tick,
            endTick: _tick + 60 * count,
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
      case AttackCommand() || CollectSalvageCommand() || ExploreSiteCommand():
        return [_err(ErrorCode.invalidCommand, 'Demo mode: not simulated')];
    }
  }
}
