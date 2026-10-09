import 'dart:async';
import 'dart:math';

import '../protocol/protocol.dart';

/// Offline stand-in for the server: produces the same protocol messages
/// (full_state, tick_update, error) so the UI has a single render path.
/// It simulates just enough (move, scan, harvest, recall, stop, queues,
/// policies) to click around; combat, salvage and derelicts are not modelled.
/// Its catalog uses a rough cost model of its own (the economy spec's curves
/// at the blitz pace), only meant to look like the real one; a live server
/// sends the real numbers.
class DemoProvider {
  static const _pace = 600.0;
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
        power: 48,
        rangeHops: 4,
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
        power: 14,
        rangeHops: 7,
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
            BuildingType.defenseGrid => 1,
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
      storage: _storage(buildings, _resources, _production(buildings)),
      buildings: buildings,
      research: research,
      catalog: _catalogFor(buildings, research),
      buildSlots: 2,
      buildQueue: [
        BuildQueueItem(
          buildingType: BuildingType.crystalMine,
          targetLevel: 4,
          startTick: _tick - 100,
          endTick: _tick + 140,
        ),
      ],
      shipyardQueue: ShipyardQueueItem(
        item: const ShipyardItem.ship(ShipClass.corvette),
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
      defences: [
        for (final k in DefenceKind.values)
          DefenceState(kind: k, count: _defenceCount[k] ?? 0, power: _defencePower[k]! * 1.04),
      ],
      homeDefencePower: 6 * 17 * 1.04 + 40,
      nextRaidEstimatePower: 80,
    );
    for (final f in _fleets) {
      _reveal(f.location, 2);
    }
    _reveal(_home, 3);
    // Older charted space beyond sensor range, remembered but not live.
    for (final h in hexSpiral(_home, 6)) {
      _sectors.putIfAbsent(h.toKey(), () => _gen(h, live: false));
    }
    // One hostile patrol next to Alpha, for atmosphere.
    final alpha = _fleets.first.location;
    final next = _sectors[alpha.toKey()]!.connections.first.toKey();
    final s = _sectors[next]!;
    _sectors[next] = SectorState(
      location: s.location,
      terrain: s.terrain,
      resources: s.resources,
      connections: s.connections,
      threat: const ThreatInfo(rating: 4, estPower: 66, basis: ThreatBasis.observed),
      lastSeen: s.lastSeen,
      live: s.live,
      hostiles: const [
        NpcFleetInfo(
          id: 9001,
          ships: [NpcShipInfo(shipClass: ShipClass.corvette, count: 3)],
          behavior: NpcBehavior.patrol,
        ),
      ],
    );
  }

  /// The ring's group power and its rating, as the server's `npc_power` and
  /// `threat_rating` compute them.
  static ThreatInfo _ringThreat(Hex h, {ThreatBasis basis = ThreatBasis.estimate}) {
    final power = 4.0 * pow(1.22, max(1, h.distFromOrigin) - 1);
    return ThreatInfo(rating: _rating(power), estPower: power, basis: basis);
  }

  static int _rating(double power) => power <= 4 ? 1 : (1 + (log(power / 4) / ln2).floor()).clamp(1, 9);

  static double _fleetPower(FleetState f) =>
      f.ships.fold(0.0, (n, s) => n + s.weaponPower + (s.hull + s.shield) / 10);

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

  SectorState _gen(Hex h, {bool live = true}) {
    final x = _hash(h.q, h.r);
    final terrain = TerrainType.values[x % 5];
    Density d(int shift) =>
        terrain == TerrainType.empty ? Density.none : Density.values[((x >> shift) % 4) + 1];
    return SectorState(
      location: h,
      terrain: terrain,
      resources: SectorResources(metal: d(3), crystal: d(6), deuterium: d(9)),
      connections: [for (final n in h.neighbors()) if (_edge(h, n)) n],
      threat: _ringThreat(h),
      lastSeen: live ? _tick : _tick - 90 - x % 600,
      live: live,
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
        world: WorldInfo(
          pace: _pace,
          preset: 'blitz',
          tickHz: 1,
          economyVersion: 2,
          worldgenVersion: 2,
        ),
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
    final cap = _hw.storage.cap;
    _resources = Resources(
      metal: min(cap.metal, _resources.metal + p.metal),
      crystal: min(cap.crystal, _resources.crystal + p.crystal),
      deuterium: min(cap.deuterium, _resources.deuterium + p.deuterium),
    );
    _hw = _hwWith();

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

    for (final b in _hw.buildQueue.where((b) => _tick >= b.endTick).toList()) {
      _pending.add(GameEvent(
        tick: _tick,
        kind: BuildingCompletedEvent(buildingType: b.buildingType, newLevel: b.targetLevel),
      ));
      _hw = _hwWith(
        buildings: [
          for (final e in _hw.buildings)
            e.buildingType == b.buildingType ? BuildingState(buildingType: e.buildingType, level: b.targetLevel) : e,
        ],
        buildQueue: [for (final q in _hw.buildQueue) if (q != b) q],
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
    _startWaiting();

    if (_tick % 15 == 0) {
      _pending.add(GameEvent(
        tick: _tick,
        kind: _rng.nextBool()
            ? ResourceHarvestedEvent(
                fleetId: 101,
                resources: Resources(metal: 10.0 + _rng.nextInt(30)),
                ticks: 10,
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
        power: f.power,
        rangeHops: f.rangeHops,
        policy: clearPolicy ? null : (policy ?? f.policy),
      );

  HomeworldState _hwWith({
    List<BuildingState>? buildings,
    List<ResearchState>? research,
    List<BuildQueueItem>? buildQueue,
    List<QueuedBuild>? buildPending,
    ShipyardQueueItem? ship,
    List<QueuedShip>? shipPending,
    ResearchItem? researchItem,
    List<QueuedResearch>? researchPending,
    bool clearShip = false,
    bool clearResearch = false,
  }) {
    final b = buildings ?? _hw.buildings;
    final r = research ?? _hw.research;
    final slots = 1 + r.firstWhere((e) => e.tech == ResearchType.modularFabrication).level;
    return _hw.copyWith(
      production: _production(b),
      storage: _storage(b, _resources, _production(b)),
      buildings: b,
      research: r,
      buildQueue: buildQueue,
      buildPending: buildPending,
      buildSlots: slots,
      shipyardQueue: ship,
      clearShipyardQueue: clearShip,
      shipyardPending: shipPending,
      researchActive: researchItem,
      clearResearchActive: clearResearch,
      researchPending: researchPending,
      catalog: _catalogFor(b, r),
    );
  }

  List<ServerMessage> _queueYard(ShipyardItem item, Resources unitCost, int ticksPerUnit, int count) {
    if ((_hw.shipyardQueue == null ? 0 : 1) + _hw.shipyardPending.length >= _hw.queueDepth) {
      return [_err(ErrorCode.queueFull, 'The shipyard queue is full')];
    }
    final cost = _times(unitCost, count);
    if (_hw.shipyardQueue == null) {
      final broke = _pay(cost);
      if (broke != null) return [broke];
      _hw = _hwWith(
        ship: ShipyardQueueItem(
          item: item,
          count: count,
          built: 0,
          startTick: _tick,
          endTick: _tick + ticksPerUnit * count,
        ),
      );
    } else {
      _hw = _hwWith(shipPending: [
        ..._hw.shipyardPending,
        QueuedShip(item: item, count: count, cost: cost, ticks: ticksPerUnit, waitingFor: _missing(cost)),
      ]);
    }
    return [_tickUpdate()];
  }

  int _bl(BuildingType t) => _hw.buildings.firstWhere((b) => b.buildingType == t).level;
  int _rl(ResearchType t) => _hw.research.firstWhere((r) => r.tech == t).level;

  int _projectedBuilding(BuildingType t) => [
        _bl(t),
        for (final q in _hw.buildQueue) if (q.buildingType == t) q.targetLevel,
        for (final q in _hw.buildPending) if (q.buildingType == t) q.targetLevel,
      ].reduce(max);

  int _projectedResearch(ResearchType t) => [
        _rl(t),
        if (_hw.researchActive?.tech == t) _hw.researchActive!.targetLevel,
        for (final q in _hw.researchPending) if (q.tech == t) q.targetLevel,
      ].reduce(max);

  bool _buildingPrereqs(BuildingType t, int Function(BuildingType) level) {
    final need = _buildingNeeds[t];
    return need == null || level(need.$1) >= need.$2;
  }

  bool _canStartBuilding(BuildingType t, int target) =>
      _hw.buildQueue.length < _hw.buildSlots &&
      _hw.buildQueue.every((q) => q.buildingType != t) &&
      _bl(t) + 1 == target &&
      _buildingPrereqs(t, _bl);

  bool _canStartResearch(ResearchType t, int target) =>
      _hw.researchActive == null && _bl(BuildingType.researchLab) > 0 && _rl(t) + 1 == target;

  bool _affordable(Resources c) =>
      _resources.metal >= c.metal && _resources.crystal >= c.crystal && _resources.deuterium >= c.deuterium;

  Resources? _missing(Resources c) {
    final m = Resources(
      metal: max(0, c.metal - _resources.metal),
      crystal: max(0, c.crystal - _resources.crystal),
      deuterium: max(0, c.deuterium - _resources.deuterium),
    );
    return m.total > 0 ? m : null;
  }

  static (Resources, int) _buildingStep(BuildingType t, int level) {
    final (base, growth) = _buildingBase[t]!;
    final cost = _grown(base, growth, level);
    return (cost, _ticksFor(cost, 700));
  }

  static (Resources, int) _researchStep(ResearchType t, int level) {
    final cost = t == ResearchType.modularFabrication
        ? _grown(const Resources(metal: 1500, crystal: 1200, deuterium: 500), 2, level)
        : _grown(const Resources(metal: 200, crystal: 150, deuterium: 100), 1.6, level);
    return (cost, _ticksFor(cost, 500));
  }

  void _startBuilding(BuildingType t, int target) {
    final (cost, ticks) = _buildingStep(t, target);
    _pay(cost);
    _hw = _hwWith(buildQueue: [
      ..._hw.buildQueue,
      BuildQueueItem(buildingType: t, targetLevel: target, startTick: _tick, endTick: _tick + ticks),
    ]);
  }

  void _startResearch(ResearchType t, int target) {
    final (cost, ticks) = _researchStep(t, target);
    _pay(cost);
    _hw = _hwWith(researchItem: ResearchItem(tech: t, targetLevel: target, startTick: _tick, endTick: _tick + ticks));
  }

  /// Start the first waiting item of each queue that can run now.
  void _startWaiting() {
    for (final q in _hw.buildPending) {
      if (_canStartBuilding(q.buildingType, q.targetLevel) && _affordable(q.cost)) {
        _hw = _hwWith(buildPending: [for (final e in _hw.buildPending) if (e != q) e]);
        _startBuilding(q.buildingType, q.targetLevel);
        break;
      }
    }
    for (final q in _hw.researchPending) {
      if (_canStartResearch(q.tech, q.targetLevel) && _affordable(q.cost)) {
        _hw = _hwWith(researchPending: [for (final e in _hw.researchPending) if (e != q) e]);
        _startResearch(q.tech, q.targetLevel);
        break;
      }
    }
    final ship = _hw.shipyardPending.firstOrNull;
    if (_hw.shipyardQueue == null && ship != null && _affordable(ship.cost)) {
      _pay(ship.cost);
      _hw = _hwWith(
        shipPending: _hw.shipyardPending.skip(1).toList(),
        ship: ShipyardQueueItem(
          item: ship.item,
          count: ship.count,
          built: 0,
          startTick: _tick,
          endTick: _tick + ship.ticks,
        ),
      );
    }
  }

  // Level-1 cost and per-level growth for the demo catalog.
  static const _buildingBase = {
    BuildingType.metalMine: (Resources(metal: 60, crystal: 15), 1.5),
    BuildingType.crystalMine: (Resources(metal: 48, crystal: 24), 1.6),
    BuildingType.deuteriumSynthesizer: (Resources(metal: 150, crystal: 50), 1.5),
    BuildingType.shipyard: (Resources(metal: 200, crystal: 100, deuterium: 50), 1.8),
    BuildingType.researchLab: (Resources(metal: 100, crystal: 200, deuterium: 50), 1.8),
    BuildingType.fuelDepot: (Resources(metal: 150, crystal: 50, deuterium: 100), 1.7),
    BuildingType.sensorArray: (Resources(metal: 100, crystal: 150, deuterium: 75), 1.7),
    BuildingType.defenseGrid: (Resources(metal: 300, crystal: 200, deuterium: 100), 1.8),
    BuildingType.storageVault: (Resources(metal: 200, crystal: 100), 1.7),
    BuildingType.fabricator: (Resources(metal: 120, crystal: 80, deuterium: 40), 1.9),
  };
  static const _buildingNeeds = {
    BuildingType.shipyard: (BuildingType.metalMine, 2),
    BuildingType.researchLab: (BuildingType.crystalMine, 2),
    BuildingType.fuelDepot: (BuildingType.deuteriumSynthesizer, 2),
    BuildingType.sensorArray: (BuildingType.researchLab, 1),
    BuildingType.defenseGrid: (BuildingType.shipyard, 3),
    BuildingType.storageVault: (BuildingType.metalMine, 2),
    BuildingType.fabricator: (BuildingType.shipyard, 2),
  };
  static const _shipCost = {
    ShipClass.scout: Resources(metal: 200, crystal: 50, deuterium: 30),
    ShipClass.corvette: Resources(metal: 400, crystal: 100, deuterium: 60),
    ShipClass.frigate: Resources(metal: 1000, crystal: 400, deuterium: 200),
    ShipClass.cruiser: Resources(metal: 3000, crystal: 1500, deuterium: 800),
    ShipClass.hauler: Resources(metal: 600, crystal: 200, deuterium: 150),
  };
  static const _defenceCost = {
    DefenceKind.pulseTurret: Resources(metal: 120, crystal: 20),
    DefenceKind.lancerBattery: Resources(metal: 400, crystal: 120, deuterium: 20),
    DefenceKind.ionBastion: Resources(metal: 1200, crystal: 500, deuterium: 100),
  };
  static const _defencePower = {
    DefenceKind.pulseTurret: 17.0,
    DefenceKind.lancerBattery: 51.0,
    DefenceKind.ionBastion: 135.0,
  };
  static const _defenceCount = {DefenceKind.pulseTurret: 6};
  static const _shipTech = {
    ShipClass.corvette: ResearchType.corvetteTech,
    ShipClass.frigate: ResearchType.frigateTech,
    ShipClass.cruiser: ResearchType.cruiserTech,
    ShipClass.hauler: ResearchType.haulerTech,
  };

  static int _researchMax(ResearchType t) =>
      _shipTech.containsValue(t) ? 1 : (t == ResearchType.modularFabrication ? 2 : 5);

  static Resources _times(Resources r, num n) =>
      Resources(metal: r.metal * n, crystal: r.crystal * n, deuterium: r.deuterium * n);

  /// `base * growth^(level - 1)`, rounded.
  static Resources _grown(Resources base, double growth, int level) {
    final f = pow(growth, level - 1);
    return Resources(
      metal: (base.metal * f).roundToDouble(),
      crystal: (base.crystal * f).roundToDouble(),
      deuterium: (base.deuterium * f).roundToDouble(),
    );
  }

  /// Whole ticks for a cost at [rate] cost units per game hour, at the demo pace.
  static int _ticksFor(Resources cost, double rate, [double speed = 1]) =>
      max(1, ((cost.metal + cost.crystal) / rate * 3600 / speed / _pace).ceil());

  static StorageState _storage(List<BuildingState> buildings, Resources stock, Resources production) {
    final vault = buildings.firstWhere((b) => b.buildingType == BuildingType.storageVault).level;
    final scale = pow(1.5, vault) * pow(_pace, 0.75);
    final cap = Resources(metal: 5000 * scale.toDouble(), crystal: 3500 * scale.toDouble(), deuterium: 2500 * scale.toDouble());
    final share = min(0.6, 0.08 * vault);
    int? eta(double have, double limit, double rate) =>
        have >= limit ? 0 : (rate > 0 ? ((limit - have) / rate).ceil() : null);
    return StorageState(
      cap: cap,
      protected: Resources(metal: cap.metal * share, crystal: cap.crystal * share, deuterium: cap.deuterium * share),
      fullInS: ResourceEta(
        metal: eta(stock.metal, cap.metal, production.metal),
        crystal: eta(stock.crystal, cap.crystal, production.crystal),
        deuterium: eta(stock.deuterium, cap.deuterium, production.deuterium),
      ),
      capped: [
        if (stock.metal >= cap.metal) ResourceKind.metal,
        if (stock.crystal >= cap.crystal) ResourceKind.crystal,
        if (stock.deuterium >= cap.deuterium) ResourceKind.deuterium,
      ],
    );
  }

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
                : () {
                    final (cost, ticks) = _buildingStep(t, bl(t) + 1);
                    return UpgradeStep(level: bl(t) + 1, cost: cost, ticks: ticks);
                  }(),
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
            maxLevel: _researchMax(t),
            next: rl(t) >= _researchMax(t)
                ? null
                : () {
                    final (cost, ticks) = _researchStep(t, rl(t) + 1);
                    return UpgradeStep(level: rl(t) + 1, cost: cost, ticks: ticks);
                  }(),
            requires: [
              needBuilding(BuildingType.researchLab, 1),
              if (t == ResearchType.corvetteTech) needBuilding(BuildingType.shipyard, 1),
              if (t == ResearchType.modularFabrication) needBuilding(BuildingType.researchLab, 4),
            ],
          ),
      ],
      ships: [
        for (final c in ShipClass.values)
          ShipOption(
            shipClass: c,
            unitCost: _shipCost[c]!,
            ticksPerShip: _ticksFor(_shipCost[c]!, 1200, 1 + 0.1 * bl(BuildingType.shipyard)),
            requires: [
              needBuilding(BuildingType.shipyard, 1),
              if (_shipTech[c] case final t?) Requirement(name: t.label, need: 1, have: rl(t), met: rl(t) >= 1),
            ],
          ),
      ],
      defences: [
        for (final k in DefenceKind.values)
          DefenceOption(
            kind: k,
            count: _defenceCount[k] ?? 0,
            unitCost: _defenceCost[k]!,
            ticksPerUnit: _ticksFor(_defenceCost[k]!, 1200, 1 + 0.1 * bl(BuildingType.shipyard)),
            power: _defencePower[k]! * (1 + 0.04 * bl(BuildingType.defenseGrid)),
            requires: [
              needBuilding(BuildingType.shipyard, 1),
              needBuilding(BuildingType.defenseGrid, switch (k) {
                DefenceKind.pulseTurret => 1,
                DefenceKind.lancerBattery => 2,
                DefenceKind.ionBastion => 4,
              }),
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
    double rate(BuildingType t, double perHour) {
      final l = buildings.firstWhere((b) => b.buildingType == t).level;
      return l == 0 ? 0 : perHour / 3600 * l * pow(1.1, l) * _pace;
    }

    return Resources(
      metal: rate(BuildingType.metalMine, 10),
      crystal: rate(BuildingType.crystalMine, 7),
      deuterium: rate(BuildingType.deuteriumSynthesizer, 5),
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
            threats: [
              for (final h in hexSpiral(f.location, 2)) SectorThreat(sector: h, threat: _sectors[h.toKey()]!.threat),
            ],
            signals: [
              SignalContact(sector: far, signal: SignalKind.richOre, threatBand: (_ringThreat(far).rating + 2) ~/ 3),
            ],
          ),
        ));
        return [_tickUpdate(sectors: true)];
      case BuildCommand(:final buildingType):
        final proj = _projectedBuilding(buildingType);
        if (proj >= 20) return [_err(ErrorCode.maxLevelReached, '${buildingType.label} is maxed')];
        if (!_buildingPrereqs(buildingType, _projectedBuilding)) {
          return [_err(ErrorCode.prerequisitesNotMet, 'Prerequisites not met')];
        }
        if (_hw.buildQueue.length + _hw.buildPending.length >= _hw.queueDepth) {
          return [_err(ErrorCode.queueFull, 'The building queue is full')];
        }
        final (cost, ticks) = _buildingStep(buildingType, proj + 1);
        if (_canStartBuilding(buildingType, proj + 1)) {
          if (!_affordable(cost)) return [_err(ErrorCode.noResources, 'Not enough resources')];
          _startBuilding(buildingType, proj + 1);
        } else {
          _hw = _hwWith(buildPending: [
            ..._hw.buildPending,
            QueuedBuild(buildingType: buildingType, targetLevel: proj + 1, cost: cost, ticks: ticks, waitingFor: _missing(cost)),
          ]);
        }
        return [_tickUpdate()];
      case ResearchCommand(:final tech):
        if (_bl(BuildingType.researchLab) == 0) return [_err(ErrorCode.noResearchLab, 'Build a research lab first')];
        final maxLevel = _shipTech.containsValue(tech) ? 1 : (tech == ResearchType.modularFabrication ? 2 : 5);
        final proj = _projectedResearch(tech);
        if (proj >= maxLevel) return [_err(ErrorCode.maxLevelReached, '${tech.label} is maxed')];
        if (_hw.catalog.research.firstWhere((o) => o.tech == tech).requires.any((r) => !r.met)) {
          return [_err(ErrorCode.prerequisitesNotMet, 'Prerequisites not met')];
        }
        if ((_hw.researchActive == null ? 0 : 1) + _hw.researchPending.length >= _hw.queueDepth) {
          return [_err(ErrorCode.queueFull, 'The research queue is full')];
        }
        final (cost, ticks) = _researchStep(tech, proj + 1);
        if (_canStartResearch(tech, proj + 1)) {
          if (!_affordable(cost)) return [_err(ErrorCode.noResources, 'Not enough resources')];
          _startResearch(tech, proj + 1);
        } else {
          _hw = _hwWith(researchPending: [
            ..._hw.researchPending,
            QueuedResearch(tech: tech, targetLevel: proj + 1, cost: cost, ticks: ticks, waitingFor: _missing(cost)),
          ]);
        }
        return [_tickUpdate()];
      case BuildShipCommand(:final shipClass, :final count):
        final o = _hw.catalog.ships.firstWhere((o) => o.shipClass == shipClass);
        if (!o.requires.first.met) return [_err(ErrorCode.noShipyard, 'Build a shipyard first')];
        if (o.requires.any((r) => !r.met)) return [_err(ErrorCode.shipLocked, 'Ship class not unlocked')];
        return _queueYard(ShipyardItem.ship(shipClass), o.unitCost, o.ticksPerShip, count);
      case BuildDefenceCommand(:final kind, :final count):
        final o = _hw.catalog.defences.firstWhere((o) => o.kind == kind);
        if (!o.requires.first.met) return [_err(ErrorCode.noShipyard, 'Build a shipyard first')];
        if (o.requires.any((r) => !r.met)) return [_err(ErrorCode.defenceLocked, '${kind.label} is locked')];
        return _queueYard(ShipyardItem.defence(kind), o.unitCost, o.ticksPerUnit, count);
      case CancelBuildCommand(:final queueType, :final index):
        switch (queueType) {
          case QueueType.building:
            if (index >= _hw.buildQueue.length) return [_err(ErrorCode.invalidTarget, 'Nothing under way there')];
            final gone = _hw.buildQueue[index];
            _hw = _hwWith(
              buildQueue: [for (var i = 0; i < _hw.buildQueue.length; i++) if (i != index) _hw.buildQueue[i]],
              buildPending: [
                for (final q in _hw.buildPending)
                  q.buildingType == gone.buildingType && q.targetLevel > gone.targetLevel
                      ? QueuedBuild(
                          buildingType: q.buildingType,
                          targetLevel: q.targetLevel - 1,
                          cost: q.cost,
                          ticks: q.ticks,
                          waitingFor: q.waitingFor)
                      : q,
              ],
            );
          case QueueType.ship:
            if (_hw.shipyardQueue == null) return [_err(ErrorCode.invalidTarget, 'Nothing under way there')];
            _hw = _hwWith(clearShip: true);
          case QueueType.research:
            final gone = _hw.researchActive;
            if (gone == null) return [_err(ErrorCode.invalidTarget, 'Nothing under way there')];
            _hw = _hwWith(clearResearch: true, researchPending: [
              for (final q in _hw.researchPending)
                q.tech == gone.tech && q.targetLevel > gone.targetLevel
                    ? QueuedResearch(
                        tech: q.tech, targetLevel: q.targetLevel - 1, cost: q.cost, ticks: q.ticks, waitingFor: q.waitingFor)
                    : q,
            ]);
        }
        _startWaiting();
        return [_tickUpdate()];
      case CancelQueuedCommand(:final queueType, :final index):
        switch (queueType) {
          case QueueType.building when index < _hw.buildPending.length:
            _hw = _hwWith(buildPending: [
              for (var i = 0; i < _hw.buildPending.length; i++) if (i != index) _hw.buildPending[i],
            ]);
          case QueueType.ship when index < _hw.shipyardPending.length:
            _hw = _hwWith(shipPending: [
              for (var i = 0; i < _hw.shipyardPending.length; i++) if (i != index) _hw.shipyardPending[i],
            ]);
          case QueueType.research when index < _hw.researchPending.length:
            _hw = _hwWith(researchPending: [
              for (var i = 0; i < _hw.researchPending.length; i++) if (i != index) _hw.researchPending[i],
            ]);
          default:
            return [_err(ErrorCode.invalidTarget, 'Nothing is waiting there')];
        }
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
      case LeaderboardCommand(:final limit):
        final rows = [
          const LeaderboardEntry(rank: 1, name: 'Vega', agent: true, score: 14.2, core: 12.8, combat: 1.4, explore: 0),
          const LeaderboardEntry(rank: 2, name: 'Demo', agent: false, score: 9.6, core: 9.1, combat: 0.5, explore: 0),
          const LeaderboardEntry(rank: 3, name: 'Orrin', agent: false, score: 4.1, core: 4.1, combat: 0, explore: 0),
        ];
        return [LeaderboardReply(tick: _tick, entries: rows.take(limit).toList())];
      case PreviewMoveCommand(:final fleetId, :final target):
        final f = fleet(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        final here = _sectors[f.location.toKey()];
        if (here == null || !here.connections.contains(target)) {
          return [_err(ErrorCode.noConnection, 'No lane from ${f.location} to $target')];
        }
        final hops = Hex.distance(target, _home);
        final left = f.fuel - f.jumpFuel;
        final threat = _sectors[target.toKey()]?.threat ?? _ringThreat(target);
        final power = _fleetPower(f);
        final ratio = power / threat.estPower;
        return [
          MovePreview(
            fleetId: fleetId,
            target: target,
            fuelCost: f.jumpFuel,
            fuelAfter: max(0, left),
            canJump: left >= 0,
            hopsHome: hops,
            fuelToReturn: hops * f.jumpFuel,
            canReturn: left >= hops * f.jumpFuel,
            threat: threat,
            fleetPower: power,
            ratio: ratio,
            label: ratio >= 3
                ? RatioLabel.safe
                : ratio >= 2
                    ? RatioLabel.favourable
                    : ratio >= 1.2
                        ? RatioLabel.risky
                        : RatioLabel.deadly,
          ),
        ];
      case AttackCommand() || CollectSalvageCommand() || ExploreSiteCommand():
        return [_err(ErrorCode.invalidCommand, 'Demo mode: not simulated')];
    }
  }
}
