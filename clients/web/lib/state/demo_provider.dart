import 'dart:async';
import 'dart:math';

import '../protocol/protocol.dart';

/// A hostile ship while a fight is running.
class _NpcShip {
  final int id;
  final ShipClass cls;
  double hull;
  double shield;
  final double weapon;
  _NpcShip(this.id, this.cls, this.hull, this.shield, this.weapon);
}

class _Combat {
  final int fleetId;
  final Hex sector;
  final NpcFleetInfo npc;
  final List<_NpcShip> ships;
  int round = 0;
  _Combat(this.fleetId, this.sector, this.npc, this.ships);
}

class _Explore {
  final int fleetId;
  final Hex sector;
  final int endTick;
  _Explore(this.fleetId, this.sector, this.endTick);
}

/// Offline stand-in for the server: produces the same protocol messages
/// (full_state, tick_update, error, game events) so the UI has a single
/// render path. It simulates move, scan, harvest, recall, stop, queues,
/// policies (as labels), NPC fights, salvage, derelict boarding and a raid
/// cycle, and the economy rework (storage caps, build slots, pending queues,
/// defences, raids). Its numbers follow the economy formulas at pace 1, with
/// a demo time-lapse on production and build times so things move; a live
/// server sends the real ones.
///
/// Layout (home H=(4,-2); fleet 101 starts at S0=(6,-4)):
///   N (6,-5) T3 corvette patrol    NE (7,-5) nebula, derelict tier 1
///   SE (7,-4) debris, salvage      SW (5,-3) toward home
///   NW (5,-4) T5 aggressive (uncharted until scanned)
///   (7,-3) passive scout           (8,-6) T4 patrol, hot derelict
///   (9,-7) T7 swarm (stale chart)
class DemoProvider {
  static const _playerId = 1;
  static const _home = Hex(4, -2);
  static const _s0 = Hex(6, -4);
  static const _hubRadius = 24;
  static const _pace = 1.0;

  /// Demo-only time-lapse on mine output and build times.
  static const _lapse = 30.0;
  static const _raidPeriod = 300;
  static const _raidEstimate = 120.0;
  static const _gridPowerPerLevel = 20.0;

  final void Function(List<ServerMessage>) _emit;
  final Random _rng = Random(7);
  Timer? _timer;

  int _tick = 4821;
  final List<GameEvent> _pending = [];

  /// Last tick each known sector was observed; presence means "charted".
  final Map<int, int> _seen = {};
  final Map<int, int> _liveUntil = {};
  final Map<int, SectorState> _baseCache = {};
  final Map<int, List<NpcFleetInfo>> _hostiles = {};
  final Map<int, (Resources, int)> _salvage = {};
  final Map<int, SiteBrief> _sites = {};
  int _queueId = 500;
  int _newQueueId() => _queueId++;
  final Set<int> _dirty = {};
  Set<int> _liveNow = {};

  final Map<int, _Combat> _combats = {};
  final Map<int, _Explore> _explores = {};
  final Map<int, HarvestResource> _harvest = {};
  final Map<int, List<Hex>> _recall = {};
  final Set<int> _provoked = {};
  int _nextNpcFleet = 9100;
  int _nextShip = 3000;
  int _raidArrival = 0;
  final Set<ResourceKind> _capAlerted = {};
  final Set<ResourceKind> _fullAlerted = {};

  late Resources _resources = const Resources(metal: 7340, crystal: 6900, deuterium: 880);
  late List<FleetState> _fleets;
  late HomeworldState _hw;

  // ── Ship tables (shared/src/constants.rs base stats) ──────────

  static const _stats = {
    ShipClass.scout: (30.0, 10.0, 5.0, 20.0, 2),
    ShipClass.corvette: (50.0, 20.0, 15.0, 10.0, 4),
    ShipClass.frigate: (120.0, 60.0, 30.0, 30.0, 5),
    ShipClass.cruiser: (200.0, 100.0, 80.0, 50.0, 7),
    ShipClass.hauler: (80.0, 20.0, 5.0, 200.0, 5),
  };
  static const _power = {
    ShipClass.scout: 9.0,
    ShipClass.corvette: 22.0,
    ShipClass.frigate: 48.0,
    ShipClass.cruiser: 110.0,
    ShipClass.hauler: 15.0,
  };

  DemoProvider(this._emit) {
    final v = [
      _ship(1011, ShipClass.corvette),
      _ship(1012, ShipClass.corvette),
      _ship(1013, ShipClass.corvette),
      _ship(1014, ShipClass.scout),
      _ship(1015, ShipClass.hauler),
    ];
    _fleets = [
      _norm(FleetState(
        id: 101,
        location: _s0,
        state: FleetStatus.idle,
        ships: v,
        cargo: const Resources(metal: 34, crystal: 12),
        cargoCapacity: 250,
        fuel: 96,
        fuelMax: 120,
      )),
      _norm(FleetState(
        id: 102,
        location: _home,
        state: FleetStatus.docked,
        ships: [_ship(1021, ShipClass.hauler), _ship(1022, ShipClass.hauler)],
        cargo: const Resources(),
        cargoCapacity: 400,
        fuel: 200,
        fuelMax: 200,
      )),
      _norm(FleetState(
        id: 103,
        location: const Hex(3, -1),
        state: FleetStatus.harvesting,
        ships: [_ship(1031, ShipClass.scout), _ship(1032, ShipClass.scout)],
        cargo: const Resources(metal: 19, crystal: 6),
        cargoCapacity: 40,
        fuel: 44,
        fuelMax: 60,
        policy: PolicyPreset.prospect,
      )),
    ];
    _harvest[103] = HarvestResource.auto;

    final buildings = [
      for (final t in BuildingType.values)
        BuildingState(buildingType: t, level: _buildingLevels[t] ?? 0),
    ];
    final research = [
      for (final t in ResearchType.values) ResearchState(tech: t, level: _researchLevels[t] ?? 0),
    ];
    _hw = HomeworldState(
      location: _home,
      production: _production(buildings),
      storage: _storage(buildings, _resources, _production(buildings)),
      buildings: buildings,
      research: research,
      catalog: _catalogFor(buildings, research),
      buildSlots: 1,
      buildQueue: [BuildQueueItem(id: 401, buildingType: BuildingType.crystalMine, targetLevel: 8, startTick: _tick - 420, endTick: _tick + 260)],
      shipyardQueue: ShipyardQueueItem(id: 402, item: const ShipyardItem.ship(ShipClass.corvette), count: 3, built: 1, startTick: _tick - 200, endTick: _tick + 170),
      researchActive: ResearchItem(id: 403, tech: ResearchType.weaponsResearch, targetLevel: 4, startTick: _tick - 300, endTick: _tick + 410),
      dockedShips: const [],
      defences: [
        for (final k in DefenceKind.values) DefenceState(kind: k, count: _defenceCount[k] ?? 0, power: _defencePower[k]! * (1 + 0.04 * (_buildingLevels[BuildingType.defenseGrid] ?? 0))),
      ],
      nextRaidEstimatePower: _raidEstimate,
    );
    final pendingBuilds = <QueuedBuild>[];
    for (final t in const [BuildingType.metalMine, BuildingType.storageVault]) {
      final (cost, ticks) = _buildingStep(t, (_buildingLevels[t] ?? 0) + 1);
      pendingBuilds.add(QueuedBuild(id: _newQueueId(), buildingType: t, targetLevel: (_buildingLevels[t] ?? 0) + 1, cost: cost, ticks: ticks, waitingFor: _missing(cost)));
    }
    _hw = _hwWith(buildPending: pendingBuilds);

    _seedWorld();
    _liveNow = _computeLive();
    for (final k in _liveNow) {
      _seen[k] = _tick;
    }
    _raidArrival = (_tick ~/ _raidPeriod + 1) * _raidPeriod;
    _pending.add(GameEvent(tick: _tick, kind: _raidIncoming()));
  }

  static const _buildingLevels = {
    BuildingType.metalMine: 9,
    BuildingType.crystalMine: 7,
    BuildingType.deuteriumSynthesizer: 5,
    BuildingType.shipyard: 4,
    BuildingType.researchLab: 3,
    BuildingType.fuelDepot: 2,
    BuildingType.sensorArray: 3,
    BuildingType.defenseGrid: 2,
    BuildingType.storageVault: 2,
  };
  static const _researchLevels = {
    ResearchType.fuelEfficiency: 5,
    ResearchType.reinforcedHulls: 2,
    ResearchType.weaponsResearch: 3,
    ResearchType.navigation: 1,
    ResearchType.harvestingEfficiency: 2,
    ResearchType.corvetteTech: 1,
    ResearchType.frigateTech: 1,
    ResearchType.haulerTech: 1,
  };

  ShipState _ship(int id, ShipClass c, {double? hull}) {
    final s = _stats[c]!;
    return ShipState(id: id, shipClass: c, hull: hull ?? s.$1, hullMax: s.$1, shield: s.$2, shieldMax: s.$2, weaponPower: s.$3);
  }

  RaidIncomingEvent _raidIncoming() =>
      RaidIncomingEvent(playerId: _playerId, arrivalTick: _raidArrival, threat: 'moderate', estPower: _raidEstimate);

  // ── World ─────────────────────────────────────────────────────

  static int _hash(int a, int b, [int c = 0]) {
    var x = ((a * 73856093) ^ (b * 19349663) ^ (c * 83492791) ^ 0x5bd1e9) & 0x7fffffff;
    x = ((x ^ (x >> 13)) * 1103515) & 0x7fffffff;
    x = (x ^ (x >> 16)) & 0x7fffffff;
    return x;
  }

  static double _h01(int a, int b, int c) => _hash(a, b, c) / 2147483648.0;

  static const _forcedOpen = [
    (Hex(6, -4), Hex(6, -5)),
    (Hex(6, -4), Hex(7, -5)),
    (Hex(6, -4), Hex(7, -4)),
    (Hex(6, -4), Hex(5, -3)),
    (Hex(6, -4), Hex(5, -4)),
    (Hex(7, -4), Hex(7, -3)),
    (Hex(7, -5), Hex(8, -6)),
    (Hex(8, -6), Hex(9, -7)),
    (Hex(5, -3), Hex(4, -2)),
    (Hex(5, -3), Hex(4, -2)),
    (Hex(4, -2), Hex(3, -1)),
  ];
  static const _forcedClosed = [(Hex(6, -4), Hex(6, -3))];

  static bool _same(Hex a, Hex b, (Hex, Hex) e) => (e.$1 == a && e.$2 == b) || (e.$1 == b && e.$2 == a);

  /// Symmetric pseudo-random lanes, denser near the hub.
  static bool _edge(Hex a, Hex b) {
    for (final e in _forcedClosed) {
      if (_same(a, b, e)) return false;
    }
    for (final e in _forcedOpen) {
      if (_same(a, b, e)) return true;
    }
    final lo = a.toKey() < b.toKey() ? a : b;
    final hi = identical(lo, a) ? b : a;
    final m = max(a.distFromOrigin, b.distFromOrigin);
    return _h01(lo.toKey(), hi.toKey(), 21) < (m <= 1 ? 1.0 : m <= 8 ? .78 : .62);
  }

  static List<Hex> _lanes(Hex h) => [for (final n in h.neighbors()) if (_edge(h, n)) n];

  /// The ring's group power and its rating, as the server's `npc_power` and
  /// `threat_rating` compute them.
  static ThreatInfo _ringThreat(Hex h) {
    final power = 4.0 * pow(1.22, max(1, h.distFromOrigin) - 1);
    return ThreatInfo(rating: _rating(power), estPower: power, basis: ThreatBasis.estimate);
  }

  static int _rating(double power) => power <= 4 ? 1 : (1 + (log(power / 4) / ln2).floor()).clamp(1, 9);

  /// A hostile group rated by its own ships (Aggressive and Swarm count 25 percent more).
  static ThreatInfo _observedThreat(List<NpcFleetInfo> groups) {
    var power = 0.0;
    for (final g in groups) {
      final ships = g.ships.fold(0.0, (n, s) => n + _ratingPower[s.shipClass]! * s.count);
      final pushy = g.behavior == NpcBehavior.aggressive || g.behavior == NpcBehavior.swarm;
      power += ships * (pushy ? 1.25 : 1.0);
    }
    return ThreatInfo(rating: _rating(power), estPower: power, basis: ThreatBasis.observed);
  }

  /// Reserve units a full tile holds `d` rings out: 3 / 10 / 22 / 42 by
  /// density, times 1.10 per ring from ring 3.
  static TileReserve _tile(Density den, Hex h) {
    const totals = [0.0, 3.0, 10.0, 22.0, 42.0];
    final d = h.distFromOrigin;
    final max = totals[den.index] * (d < 3 ? 1.0 : pow(1.1, min(d, 20) - 3));
    return TileReserve(units: max, maxUnits: max);
  }

  static double _fleetPower(FleetState f) => f.ships.fold(0.0, (n, s) => n + s.weaponPower + (s.hull + s.shield) / 10);

  static const _ratingPower = {ShipClass.scout: 9, ShipClass.corvette: 22, ShipClass.frigate: 48, ShipClass.cruiser: 110, ShipClass.hauler: 15};

  /// Authored opening neighbourhood (terrain, ore m/c/d, hostiles, salvage, site).
  static final Map<int, ({TerrainType t, List<int> ore, List<(ShipClass, int)>? npc, NpcBehavior? b, int tier, SiteRisk? risk, Resources? salv})> _authored = {
    const Hex(6, -4).toKey(): (t: TerrainType.asteroidField, ore: [3, 2, 0], npc: null, b: null, tier: 0, risk: null, salv: null),
    const Hex(6, -5).toKey(): (t: TerrainType.empty, ore: [1, 0, 1], npc: [(ShipClass.corvette, 1)], b: NpcBehavior.patrol, tier: 0, risk: null, salv: null),
    const Hex(7, -5).toKey(): (t: TerrainType.nebula, ore: [0, 1, 3], npc: null, b: null, tier: 1, risk: SiteRisk.uneasy, salv: null),
    const Hex(7, -4).toKey(): (t: TerrainType.debrisField, ore: [2, 0, 0], npc: null, b: null, tier: 0, risk: null, salv: Resources(metal: 40, crystal: 10, deuterium: 4)),
    const Hex(5, -3).toKey(): (t: TerrainType.empty, ore: [1, 1, 0], npc: null, b: null, tier: 0, risk: null, salv: null),
    const Hex(5, -4).toKey(): (t: TerrainType.anomaly, ore: [0, 3, 1], npc: [(ShipClass.frigate, 1), (ShipClass.corvette, 2)], b: NpcBehavior.aggressive, tier: 0, risk: null, salv: null),
    const Hex(3, -1).toKey(): (t: TerrainType.asteroidField, ore: [2, 1, 0], npc: null, b: null, tier: 0, risk: null, salv: null),
    const Hex(7, -3).toKey(): (t: TerrainType.empty, ore: [1, 1, 0], npc: [(ShipClass.scout, 1)], b: NpcBehavior.passive, tier: 0, risk: null, salv: null),
    const Hex(8, -6).toKey(): (t: TerrainType.asteroidField, ore: [4, 2, 1], npc: [(ShipClass.corvette, 2)], b: NpcBehavior.patrol, tier: 1, risk: SiteRisk.hot, salv: null),
    const Hex(9, -7).toKey(): (t: TerrainType.empty, ore: [1, 1, 1], npc: [(ShipClass.cruiser, 1), (ShipClass.frigate, 2), (ShipClass.corvette, 3)], b: NpcBehavior.swarm, tier: 0, risk: null, salv: null),
  };

  /// Terrain, ore and lanes only; hostiles, salvage and sites are mutable state.
  SectorState _base(Hex h) => _baseCache.putIfAbsent(h.toKey(), () {
        final au = _authored[h.toKey()];
        final d = h.distFromOrigin;
        final x = _h01(h.q, h.r, 1);
        final terrain = au?.t ??
            (x < .42 ? TerrainType.empty : x < .64 ? TerrainType.asteroidField : x < .78 ? TerrainType.nebula : x < .9 ? TerrainType.debrisField : TerrainType.anomaly);
        final rich = 1.2 + d * .11;
        int o(int s, double bonus) => max(0, min(4, (_h01(h.q, h.r, s) * rich + bonus).floor()));
        final ore = au?.ore ??
            [
              o(2, terrain == TerrainType.asteroidField ? 1 : 0),
              o(3, terrain == TerrainType.asteroidField ? .6 : 0),
              o(4, terrain == TerrainType.nebula ? 1 : 0),
            ];
        return SectorState(
          location: h,
          terrain: terrain,
          resources: SectorResources(metal: Density.values[ore[0]], crystal: Density.values[ore[1]], deuterium: Density.values[ore[2]]),
          connections: _lanes(h),
          oreReserve: terrain == TerrainType.empty
              ? null
              : OreReserve(
                  metal: _tile(Density.values[ore[0]], h),
                  crystal: _tile(Density.values[ore[1]], h),
                  deuterium: _tile(Density.values[ore[2]], h)),
          threat: _ringThreat(h),
          lastSeen: 0,
          live: false,
        );
      });

  void _seedWorld() {
    _authored.forEach((k, au) {
      if (au.npc != null) _hostiles[k] = [_npcGroup(au.npc!, au.b!)];
      if (au.salv != null) _salvage[k] = (au.salv!, _tick + 2400);
      if (au.risk != null) _sites[k] = SiteBrief(tier: au.tier, risk: au.risk!);
    });
    // Procedural NPCs, salvage and sites in the rest of the charted area.
    for (final h in hexSpiral(_home, 12)) {
      final k = h.toKey();
      if (_authored.containsKey(k)) continue;
      final d = h.distFromOrigin;
      if (Hex.distance(h, _home) > 2 && _h01(h.q, h.r, 5) < min(.85, .25 + .025 * d)) {
        final cls = d <= 7 ? ShipClass.scout : d <= 12 ? ShipClass.corvette : d <= 20 ? ShipClass.frigate : ShipClass.cruiser;
        final target = 4 * pow(1.22, d - 1) * 1.8;
        final n = max(1, min(12, (target / _ratingPower[cls]!).round()));
        final beh = _h01(h.q, h.r, 6) < max(0, .5 - .08 * (d - 8))
            ? NpcBehavior.passive
            : _ratingPower[cls]! * n >= 256 ? NpcBehavior.swarm : _ratingPower[cls]! * n >= 64 ? NpcBehavior.aggressive : NpcBehavior.patrol;
        _hostiles[k] = [_npcGroup([(cls, n)], beh)];
      }
      if (_h01(h.q, h.r, 8) < .08) {
        _salvage[k] = (Resources(metal: (20 + _h01(h.q, h.r, 9) * 60 + d * 4).roundToDouble(), crystal: (5 + _h01(h.q, h.r, 10) * 20).roundToDouble(), deuterium: (_h01(h.q, h.r, 11) * 12).roundToDouble()), _tick + 1500 + _hash(h.q, h.r, 3) % 3000);
      }
      if (d > 2 && _h01(h.q, h.r, 12) < .05 + d * .004) {
        _sites[k] = SiteBrief(tier: d <= 11 ? 1 : d <= 21 ? 2 : 3, risk: SiteRisk.values[(_h01(h.q, h.r, 13) * 3).floor()]);
      }
    }
    // Chart memory: ages from live to fossil.
    const trail = [Hex(5, -3), Hex(6, -4), Hex(7, -5), Hex(8, -6), Hex(9, -7), Hex(10, -8), Hex(3, -1), Hex(2, 1), Hex(1, 3), Hex(0, 5), Hex(8, -3), Hex(10, -4)];
    for (final h in hexSpiral(_home, 6)) {
      if (h == const Hex(5, -4)) continue; // stays uncharted until scanned
      final dh = Hex.distance(h, _home);
      double? age;
      if (dh <= 3) {
        age = 30 + _h01(h.q, h.r, 31) * 500;
      } else {
        for (final t in trail) {
          final dd = Hex.distance(h, t);
          if (dd <= 1) {
            age = 100 + _h01(h.q, h.r, 32) * 900 + t.distFromOrigin * 150;
            break;
          }
        }
        if (age == null && _h01(h.q, h.r, 33) < .55) age = 900 + _h01(h.q, h.r, 34) * 2500;
      }
      if (age != null) _seen[h.toKey()] = _tick - age.floor();
    }
    _seen[const Hex(8, -6).toKey()] = _tick - 2400;
    _seen[const Hex(9, -7).toKey()] = _tick - 5200;
    for (final h in [_s0, ..._s0.neighbors(), ..._fleets.map((f) => f.location), _home]) {
      if (h != const Hex(5, -4)) _seen.putIfAbsent(h.toKey(), () => _tick - 60);
    }
  }

  NpcFleetInfo _npcGroup(List<(ShipClass, int)> comp, NpcBehavior b) =>
      NpcFleetInfo(id: _nextNpcFleet++, ships: [for (final c in comp) NpcShipInfo(shipClass: c.$1, count: c.$2)], behavior: b);

  Set<int> _computeLive() {
    final out = <int>{};
    for (final f in _fleets) {
      for (final h in hexSpiral(f.location, 1)) {
        if (_seen.containsKey(h.toKey()) || h == f.location) out.add(h.toKey());
      }
    }
    _liveUntil.removeWhere((_, until) => until <= _tick);
    out.addAll(_liveUntil.keys);
    return out;
  }

  SectorState _sector(Hex h, {bool? live}) {
    final b = _base(h);
    final k = h.toKey();
    final isLive = live ?? _liveNow.contains(k);
    final hs = _hostiles[k];
    final sv = _salvage[k];
    return SectorState(
      location: h,
      terrain: b.terrain,
      resources: b.resources,
      connections: b.connections,
      oreReserve: b.oreReserve,
      threat: hs == null || hs.isEmpty ? b.threat : _observedThreat(hs),
      hostiles: hs == null || hs.isEmpty ? null : hs,
      salvage: sv?.$1,
      salvageDespawnTick: sv?.$2,
      site: _sites[k],
      lastSeen: isLive ? _tick : (_seen[k] ?? 0),
      live: isLive,
      pinsStale: !isLive && (sv != null || _sites[k] != null),
    );
  }

  List<SectorState> _knownSectors() => [for (final k in _seen.keys) _sector(Hex.fromKey(k))];

  // ── Lifecycle ─────────────────────────────────────────────────

  GameState initial() => GameState(
        tick: _tick,
        player: _player(),
        fleets: [for (final f in _fleets) _norm(f)],
        homeworld: _hw,
        knownSectors: _knownSectors(),
        world: const WorldInfo(pace: _pace, preset: 'persistent', tickHz: 1, economyVersion: 2, worldgenVersion: 2),
      );

  PlayerState _player() => PlayerState(id: _playerId, name: 'Demo', resources: _resources, homeworld: _home);

  void start() {
    _timer?.cancel();
    _timer = Timer.periodic(const Duration(seconds: 1), (_) => step());
  }

  void stop() {
    _timer?.cancel();
    _timer = null;
  }

  void _ev(EventKind k) => _pending.add(GameEvent(tick: _tick, kind: k));

  Resources _clampCaps(Resources r) {
    final cap = _hw.storage.cap;
    return Resources(metal: min(r.metal, cap.metal), crystal: min(r.crystal, cap.crystal), deuterium: min(r.deuterium, cap.deuterium));
  }

  /// One server tick. Public so tests can drive the simulation.
  void step() {
    _tick++;
    final p = _hw.production;
    _resources = _clampCaps(Resources(
      metal: _resources.metal + p.metal,
      crystal: _resources.crystal + p.crystal,
      deuterium: _resources.deuterium + p.deuterium,
    ));
    _capAlerts();

    _fleets = [
      for (final f in _fleets) _copy(f, cooldown: max(0, f.cooldownRemaining - 1)),
    ];
    _stepHarvest();
    _stepRecall();
    _stepExplore();
    _stepCombat();
    _stepQueues();
    _stepSalvage();
    _stepRaid();

    if (_tick % 120 == 0) {
      _ev(const AlertEvent(level: AlertLevel.info, message: 'Demo mode: no server connected'));
    }

    final before = _liveNow;
    _liveNow = _computeLive();
    for (final k in _liveNow) {
      _seen[k] = _tick;
    }
    _dirty
      ..addAll(_liveNow)
      ..addAll(before.difference(_liveNow));
    _emit([_tickUpdate()]);
  }

  void _capAlerts() {
    final cap = _hw.storage.cap;
    final eta = _hw.storage.fullInS;
    for (final e in [
      (ResourceKind.metal, _resources.metal, cap.metal, eta.metal),
      (ResourceKind.crystal, _resources.crystal, cap.crystal, eta.crystal),
      (ResourceKind.deuterium, _resources.deuterium, cap.deuterium, eta.deuterium),
    ]) {
      final ratio = e.$2 / e.$3;
      if (ratio >= .85) {
        if (_capAlerted.add(e.$1)) _ev(StorageNearCapEvent(playerId: _playerId, resource: e.$1, ratio: ratio, fullInS: e.$4));
      } else if (ratio < .8) {
        _capAlerted.remove(e.$1);
      }
      if (ratio >= .999) {
        if (_fullAlerted.add(e.$1)) _ev(StorageFullEvent(playerId: _playerId, resource: e.$1));
      } else {
        _fullAlerted.remove(e.$1);
      }
    }
  }

  TickUpdate _tickUpdate() {
    final events = List.of(_pending);
    _pending.clear();
    final secs = [for (final k in _dirty) _sector(Hex.fromKey(k))];
    _dirty.clear();
    _fleets = [for (final f in _fleets) _norm(f)];
    _hw = _hwWith();
    return TickUpdate(
      tick: _tick,
      player: _player(),
      fleets: List.of(_fleets),
      homeworldUpdate: _hw,
      sectorUpdates: secs.isEmpty ? null : secs,
      events: events.isEmpty ? null : events,
    );
  }

  // ── Per-tick systems ──────────────────────────────────────────

  static const _densityRate = {Density.none: 0.0, Density.sparse: .5, Density.moderate: 1.0, Density.rich: 1.6, Density.pristine: 2.5};

  Density _density(SectorState s, String k) => switch (k) {
        'metal' => s.resources.metal,
        'crystal' => s.resources.crystal,
        _ => s.resources.deuterium,
      };

  void _stepHarvest() {
    for (final f in List.of(_fleets)) {
      if (f.state != FleetStatus.harvesting) continue;
      final pick = _harvest[f.id] ?? HarvestResource.auto;
      final sec = _base(f.location);
      final keys = [
        for (final k in const ['metal', 'crystal', 'deut'])
          if ((pick == HarvestResource.auto || pick.name == k || (pick == HarvestResource.deuterium && k == 'deut')) && _density(sec, k) != Density.none) k
      ];
      final free = f.cargoCapacity - f.cargo.total;
      if (keys.isEmpty || free <= 0) {
        _setFleet(_copy(f, state: FleetStatus.idle));
        _ev(AlertEvent(level: AlertLevel.info, message: free <= 0 ? 'Cargo full: ${f.id} stops harvesting' : 'Nothing left to harvest', fleetId: f.id, sector: f.location));
        continue;
      }
      var m = 0.0, c = 0.0, d = 0.0;
      for (final k in keys) {
        final g = min(free / keys.length, _densityRate[_density(sec, k)]! * (1 + f.ships.length * .15));
        if (k == 'metal') m += g;
        if (k == 'crystal') c += g;
        if (k == 'deut') d += g;
      }
      final cargo = Resources(metal: f.cargo.metal + m, crystal: f.cargo.crystal + c, deuterium: f.cargo.deuterium + d);
      _setFleet(_copy(f, cargo: cargo));
      if (_tick % 10 == 0) {
        _ev(ResourceHarvestedEvent(fleetId: f.id, resources: Resources(metal: m * 10, crystal: c * 10, deuterium: d * 10), ticks: 10));
      }
    }
  }

  void _stepRecall() {
    for (final id in _recall.keys.toList()) {
      final f = _find(id);
      final path = _recall[id]!;
      if (f == null || f.state != FleetStatus.returning) {
        _recall.remove(id);
        continue;
      }
      if (f.cooldownRemaining > 0) continue;
      if (path.isEmpty) {
        _recall.remove(id);
        continue;
      }
      if (f.fuel < _jump(f.ships)) {
        _recall.remove(id);
        _setFleet(_copy(f, state: FleetStatus.idle));
        _ev(AlertEvent(level: AlertLevel.warning, message: 'Out of fuel: fleet ${f.id} stranded', fleetId: f.id, sector: f.location));
        continue;
      }
      _hop(f, path.removeAt(0), returning: true);
    }
  }

  void _stepExplore() {
    for (final id in _explores.keys.toList()) {
      final x = _explores[id]!;
      final f = _find(id);
      if (f == null || f.state != FleetStatus.exploring) {
        _explores.remove(id);
        continue;
      }
      if (_tick < x.endTick) continue;
      _explores.remove(id);
      final k = x.sector.toKey();
      final site = _sites[k];
      if (site == null) {
        _setFleet(_copy(f, state: FleetStatus.idle));
        continue;
      }
      _sites.remove(k);
      _dirty.add(k);
      if (site.risk == SiteRisk.hot && _rng.nextDouble() < .6) {
        final g = _npcGroup([(ShipClass.corvette, 2)], NpcBehavior.aggressive);
        _hostiles[k] = [...?_hostiles[k], g];
        _ev(SiteAmbushEvent(fleetId: id, sector: x.sector, npcFleetId: g.id));
        _setFleet(_copy(f, state: FleetStatus.idle));
        continue;
      }
      final t = site.tier;
      final res = Resources(
        metal: (60 * t * (.7 + _rng.nextDouble() * .6)).roundToDouble(),
        crystal: (25 * t * (.7 + _rng.nextDouble() * .6)).roundToDouble(),
        deuterium: (10 * t * _rng.nextDouble()).roundToDouble(),
      );
      final roll = _rng.nextDouble();
      ShipClass? ship;
      ResearchType? tech;
      var ships = f.ships;
      if (roll > .6) {
        tech = ResearchType.navigation;
      } else if (roll > .35) {
        ship = ShipClass.scout;
        ships = [...f.ships, _ship(_nextShip++, ShipClass.scout)];
      }
      _resources = _clampCaps(Resources(
        metal: _resources.metal + res.metal,
        crystal: _resources.crystal + res.crystal,
        deuterium: _resources.deuterium + res.deuterium,
      ));
      _ev(SiteExploredEvent(fleetId: id, sector: x.sector, tier: t, resources: res, recoveredShip: ship, techCache: tech));
      _setFleet(_copy(f, state: FleetStatus.idle, ships: ships));
    }
  }

  void _stepSalvage() {
    for (final k in _salvage.keys.toList()) {
      final e = _salvage[k]!;
      if (e.$2 <= _tick) {
        _salvage.remove(k);
        _dirty.add(k);
        _ev(SalvageDespawnedEvent(sector: Hex.fromKey(k), resources: e.$1));
      }
    }
  }

  void _stepQueues() {
    for (final b in _hw.buildQueue.where((b) => _tick >= b.endTick).toList()) {
      _ev(BuildingCompletedEvent(buildingType: b.buildingType, newLevel: b.targetLevel));
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
      _ev(ResearchCompletedEvent(tech: r.tech, newLevel: r.targetLevel));
      _hw = _hwWith(
        research: [for (final e in _hw.research) e.tech == r.tech ? ResearchState(tech: e.tech, level: r.targetLevel) : e],
        clearResearch: true,
      );
    }
    final s = _hw.shipyardQueue;
    if (s != null && _tick >= s.endTick) {
      if (s.item.ship case final cls?) {
        _setDocked([..._hw.dockedShips, _ship(_nextShip++, cls)]);
        _ev(ShipBuiltEvent(shipClass: cls, count: 1));
      } else {
        final k = s.item.defence!;
        _hw = _hwWith(defences: [
          for (final d in _hw.defences) d.kind == k ? DefenceState(kind: k, count: d.count + 1, restoring: d.restoring, power: d.power) : d,
        ]);
        _ev(DefenceBuiltEvent(defence: k, count: 1, playerId: _playerId));
      }
      // A batch pays per unit: between units it goes back to the front of the
      // line with its progress, and the next unit starts when it can be paid.
      final rest = s.count - s.built - 1;
      _hw = _hwWith(clearShip: true, shipPending: [
        if (rest > 0) _waitingShip(s.id, s.item, s.count, s.built + 1, false),
        ..._hw.shipyardPending,
      ]);
    }
    _startWaiting();
  }

  void _setDocked(List<ShipState> docked) {
    final h = _hw;
    _hw = HomeworldState(
      location: h.location,
      production: h.production,
      storage: h.storage,
      buildings: h.buildings,
      research: h.research,
      buildQueue: h.buildQueue,
      buildPending: h.buildPending,
      buildSlots: h.buildSlots,
      queueWaitingMax: h.queueWaitingMax,
      shipyardQueue: h.shipyardQueue,
      shipyardPending: h.shipyardPending,
      researchActive: h.researchActive,
      researchPending: h.researchPending,
      dockedShips: docked,
      defences: h.defences,
      homeDefencePower: h.homeDefencePower,
      nextRaidEstimatePower: h.nextRaidEstimatePower,
      catalog: h.catalog,
    );
  }

  double _dockedPower() {
    var p = 0.0;
    for (final f in _fleets) {
      if (f.location == _home && f.state == FleetStatus.docked) {
        for (final s in f.ships) {
          p += _power[s.shipClass]!;
        }
      }
    }
    for (final s in _hw.dockedShips) {
      p += _power[s.shipClass]!;
    }
    return p;
  }

  void _stepRaid() {
    if (_tick < _raidArrival) return;
    final raid = _raidEstimate * (.85 + .3 * _h01(_raidArrival, 3, 7));
    final defense = _homePower(_hw.defences, _hw.buildings);
    final defended = defense >= raid;
    var lost = const Resources();
    var kept = const Resources();
    if (!defended) {
      final prot = _hw.storage.protected;
      double skim(double v, double p) => max(0, min(v - p, v * .15));
      lost = Resources(metal: skim(_resources.metal, prot.metal), crystal: skim(_resources.crystal, prot.crystal), deuterium: skim(_resources.deuterium, prot.deuterium));
      kept = prot;
      _resources = Resources(metal: _resources.metal - lost.metal, crystal: _resources.crystal - lost.crystal, deuterium: _resources.deuterium - lost.deuterium);
    }
    _ev(RaidResolvedEvent(playerId: _playerId, defended: defended, resourcesLost: lost, raidPower: raid, defensePower: defense, protectedKept: kept));
    _raidArrival += _raidPeriod;
    _ev(_raidIncoming());
  }

  // ── Combat ────────────────────────────────────────────────────

  void _startCombat(FleetState f, NpcFleetInfo npc) {
    final ships = <_NpcShip>[];
    var i = 0;
    for (final g in npc.ships) {
      final s = _stats[g.shipClass]!;
      for (var n = 0; n < g.count; n++) {
        ships.add(_NpcShip(npc.id * 100 + i++, g.shipClass, s.$1, s.$2, s.$3));
      }
    }
    _combats[f.id] = _Combat(f.id, f.location, npc, ships);
    _recall.remove(f.id);
    _explores.remove(f.id);
    _setFleet(_copy(f, state: FleetStatus.inCombat));
    _ev(CombatStartedEvent(playerFleetId: f.id, owner: 'Demo', enemyFleetId: npc.id, sector: f.location, mine: true));
  }

  void _stepCombat() {
    // Aggressive groups engage fleets that share their sector.
    for (final f in List.of(_fleets)) {
      if (f.state == FleetStatus.docked || f.state == FleetStatus.inCombat || _combats.containsKey(f.id)) continue;
      final hs = _hostiles[f.location.toKey()];
      if (hs == null || hs.isEmpty) continue;
      final npc = hs.first;
      final provoked = _provoked.remove(f.id);
      final engage = npc.behavior == NpcBehavior.aggressive ||
          npc.behavior == NpcBehavior.swarm ||
          (npc.behavior == NpcBehavior.patrol && provoked && _rng.nextDouble() < .4);
      if (engage && !_combats.values.any((c) => c.npc.id == npc.id)) _startCombat(f, npc);
    }
    for (final c in _combats.values.toList()) {
      var f = _find(c.fleetId);
      if (f == null) {
        _combats.remove(c.fleetId);
        continue;
      }
      c.round++;
      final mine = [...f.ships];
      final foes = c.ships.where((s) => s.hull > 0).toList();
      final hull = {for (final s in mine) s.id: s.hull};
      final shield = {for (final s in mine) s.id: s.shield};
      for (final a in mine) {
        if (foes.isEmpty) break;
        final t = foes[_rng.nextInt(foes.length)];
        if (t.hull <= 0) continue;
        final dmg = a.weaponPower * (.8 + _rng.nextDouble() * .4);
        final sh = min(t.shield, dmg);
        t.shield -= sh;
        t.hull -= dmg - sh;
        _ev(CombatRoundEvent(
          sector: c.sector,
          attackerFleetId: f.id,
          attackerOwner: 'Demo',
          attackerShipId: a.id,
          targetFleetId: c.npc.id,
          targetOwner: null,
          targetShipId: t.id,
          damage: dmg,
          shieldAbsorbed: sh,
          hullDamage: dmg - sh,
          rapidFire: false,
          mine: true,
        ));
        if (t.hull <= 0) {
          _ev(ShipDestroyedEvent(shipId: t.id, shipClass: t.cls, ownerFleetId: c.npc.id, isNpc: true, sector: c.sector, owner: null, mine: false));
          foes.remove(t);
        }
      }
      final alive = [for (final s in mine) s];
      for (final e in c.ships.where((s) => s.hull > 0)) {
        if (alive.isEmpty) break;
        final t = alive[_rng.nextInt(alive.length)];
        final dmg = e.weapon * (.8 + _rng.nextDouble() * .4);
        final sh = min(shield[t.id]!, dmg);
        shield[t.id] = shield[t.id]! - sh;
        hull[t.id] = hull[t.id]! - (dmg - sh);
        _ev(CombatRoundEvent(
          sector: c.sector,
          attackerFleetId: c.npc.id,
          attackerOwner: null,
          attackerShipId: e.id,
          targetFleetId: f.id,
          targetOwner: 'Demo',
          targetShipId: t.id,
          damage: dmg,
          shieldAbsorbed: sh,
          hullDamage: dmg - sh,
          rapidFire: false,
          mine: true,
        ));
        if (hull[t.id]! <= 0) {
          _ev(ShipDestroyedEvent(shipId: t.id, shipClass: t.shipClass, ownerFleetId: f.id, isNpc: false, sector: c.sector, owner: 'Demo', mine: true));
          alive.remove(t);
        }
      }
      final survivors = [
        for (final s in mine)
          if (hull[s.id]! > 0)
            ShipState(id: s.id, shipClass: s.shipClass, hull: hull[s.id]!, hullMax: s.hullMax, shield: shield[s.id]!, shieldMax: s.shieldMax, weaponPower: s.weaponPower)
      ];
      final npcLeft = c.ships.any((s) => s.hull > 0);
      if (survivors.isEmpty) {
        _combats.remove(c.fleetId);
        _fleets = [for (final e in _fleets) if (e.id != f.id) e];
        _ev(FleetDestroyedEvent(fleetId: f.id, isNpc: false, sector: c.sector, owner: 'Demo', mine: true, salvage: const Resources()));
        _ev(CombatEndedEvent(sector: c.sector, playerVictory: false, owners: const ['Demo'], mine: true));
        continue;
      }
      f = _copy(f, ships: survivors);
      _setFleet(f);
      if (!npcLeft) {
        _combats.remove(c.fleetId);
        final th = _threatOf(c.npc);
        final loot = Resources(metal: 30.0 * th + 12, crystal: 8.0 * th, deuterium: 3.0 * th);
        final k = c.sector.toKey();
        _hostiles[k] = [for (final h in _hostiles[k] ?? <NpcFleetInfo>[]) if (h.id != c.npc.id) h];
        final prev = _salvage[k]?.$1;
        final total = Resources(metal: loot.metal + (prev?.metal ?? 0), crystal: loot.crystal + (prev?.crystal ?? 0), deuterium: loot.deuterium + (prev?.deuterium ?? 0));
        final despawn = _tick + 600;
        _salvage[k] = (total, despawn);
        _dirty.add(k);
        _ev(FleetDestroyedEvent(fleetId: c.npc.id, isNpc: true, sector: c.sector, owner: null, mine: false, salvage: loot));
        _ev(CombatEndedEvent(sector: c.sector, playerVictory: true, owners: const ['Demo'], mine: true, salvage: total, salvageDespawnTick: despawn));
        _setFleet(_copy(f, state: FleetStatus.idle));
      }
    }
  }

  static int _threatOf(NpcFleetInfo n) {
    var p = 0.0;
    for (final s in n.ships) {
      p += _ratingPower[s.shipClass]! * s.count;
    }
    return max(1, min(9, 1 + (log(max(1, p) / 4) / ln2).floor()));
  }

  // ── Fleet helpers ─────────────────────────────────────────────

  static int _jump(List<ShipState> ships) => ships.fold<int>(1, (m, s) => max(m, _stats[s.shipClass]!.$5));

  FleetState _norm(FleetState f) => FleetState(
        id: f.id,
        location: f.location,
        state: f.state,
        ships: f.ships,
        cargo: f.cargo,
        cargoCapacity: f.cargoCapacity,
        fuel: f.fuel,
        fuelMax: f.fuelMax,
        jumpFuel: _jump(f.ships).toDouble(),
        homeFuel: f.location == _home ? 0 : (Hex.distance(f.location, _home) * _jump(f.ships)).toDouble(),
        cooldownRemaining: f.cooldownRemaining,
        power: f.ships.fold<double>(0, (p, s) => p + _power[s.shipClass]!),
        rangeHops: f.fuel ~/ _jump(f.ships),
        policy: f.policy,
      );

  FleetState _copy(
    FleetState f, {
    Hex? location,
    FleetStatus? state,
    Resources? cargo,
    PolicyPreset? policy,
    bool clearPolicy = false,
    List<ShipState>? ships,
    double? fuel,
    int? cooldown,
  }) =>
      FleetState(
        id: f.id,
        location: location ?? f.location,
        state: state ?? f.state,
        ships: ships ?? f.ships,
        cargo: cargo ?? f.cargo,
        cargoCapacity: f.cargoCapacity,
        fuel: fuel ?? f.fuel,
        fuelMax: f.fuelMax,
        jumpFuel: f.jumpFuel,
        homeFuel: f.homeFuel,
        cooldownRemaining: cooldown ?? f.cooldownRemaining,
        power: f.power,
        rangeHops: f.rangeHops,
        policy: clearPolicy ? null : (policy ?? f.policy),
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

  /// Chart [h] and its neighbours up to [radius]; the uncharted NW contact
  /// only appears through a scan.
  int _reveal(Hex h, int radius, {bool scan = false}) {
    var fresh = 0;
    for (final n in hexSpiral(h, radius)) {
      if (!scan && n == const Hex(5, -4)) continue;
      if (n.distFromOrigin > _hubRadius) continue;
      if (_seen.putIfAbsent(n.toKey(), () => _tick) == _tick) fresh++;
      _dirty.add(n.toKey());
    }
    return fresh;
  }

  void _hop(FleetState f, Hex target, {bool returning = false}) {
    final first = !_seen.containsKey(target.toKey());
    final home = target == _home;
    final cur = _find(f.id) ?? f;
    var next = _copy(
      cur,
      location: target,
      state: home ? FleetStatus.docked : (returning ? FleetStatus.returning : FleetStatus.idle),
      fuel: cur.fuel - _jump(cur.ships),
      cooldown: 1,
    );
    if (home) {
      _resources = _clampCaps(Resources(
        metal: _resources.metal + cur.cargo.metal,
        crystal: _resources.crystal + cur.cargo.crystal,
        deuterium: _resources.deuterium + cur.cargo.deuterium,
      ));
      next = _copy(next, cargo: const Resources(), fuel: cur.fuelMax, cooldown: 0);
      _recall.remove(cur.id);
    }
    _setFleet(next);
    _reveal(target, 1);
    _dirty.add(cur.location.toKey());
    _ev(FleetArrivedEvent(fleetId: cur.id, sector: target));
    _ev(SectorEnteredEvent(fleetId: cur.id, sector: target, firstVisit: first));
    final hs = _hostiles[target.toKey()];
    if (hs != null && hs.isNotEmpty && hs.first.behavior != NpcBehavior.passive) _provoked.add(cur.id);
  }

  List<Hex>? _pathHome(Hex from) {
    if (from == _home) return [];
    final prev = <Hex, Hex>{};
    final q = <Hex>[from];
    final seen = {from};
    for (var i = 0; i < q.length && i < 4000; i++) {
      final cur = q[i];
      if (cur == _home) break;
      for (final n in _lanes(cur)) {
        if (seen.add(n) && n.distFromOrigin <= _hubRadius) {
          prev[n] = cur;
          q.add(n);
        }
      }
    }
    if (!prev.containsKey(_home)) return null;
    final out = <Hex>[];
    for (var h = _home; h != from; h = prev[h]!) {
      out.insert(0, h);
    }
    return out;
  }

  // ── Catalog ───────────────────────────────────────────────────

  HomeworldState _hwWith({
    List<BuildingState>? buildings,
    List<ResearchState>? research,
    List<BuildQueueItem>? buildQueue,
    List<QueuedBuild>? buildPending,
    ShipyardQueueItem? ship,
    List<QueuedShip>? shipPending,
    ResearchItem? researchItem,
    List<QueuedResearch>? researchPending,
    List<DefenceState>? defences,
    bool clearShip = false,
    bool clearResearch = false,
  }) {
    final b = buildings ?? _hw.buildings;
    final r = research ?? _hw.research;
    final slots = 1 + r.firstWhere((e) => e.tech == ResearchType.modularFabrication).level;
    final power = _homePower(defences ?? _hw.defences, b);
    return _hw.copyWith(
      homeDefencePower: power,
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
      defences: defences,
      catalog: _catalogFor(b, r),
    );
  }

  Resources _unitCostOf(ShipyardItem item) => item.ship != null ? _shipCost[item.ship]! : _defenceCost[item.defence]!;

  QueuedShip _waitingShip(int id, ShipyardItem item, int count, int built, bool reserve) {
    final unit = _unitCostOf(item);
    return QueuedShip(
      id: id,
      item: item,
      count: count,
      built: built,
      reserve: reserve,
      cost: _times(unit, count - built),
      unitCost: unit,
      ticks: item.ship != null ? _hw.catalog.ships.firstWhere((o) => o.shipClass == item.ship).ticksPerShip : _hw.catalog.defences.firstWhere((o) => o.kind == item.defence).ticksPerUnit,
      waitingFor: _missing(unit),
      waitingOn: WaitReason.slot,
    );
  }

  List<ServerMessage> _queueYard(ShipyardItem item, int count, bool reserve) {
    if ((_hw.shipyardQueue == null ? 0 : 1) + _hw.shipyardPending.length >= 1 + _hw.queueWaitingMax) {
      return [_err(ErrorCode.queueFull, 'The shipyard queue is full (1 running + ${_hw.queueWaitingMax} waiting)')];
    }
    final order = _waitingShip(_newQueueId(), item, count, 0, reserve);
    _hw = _hwWith(shipPending: [..._hw.shipyardPending, order]);
    _startWaiting();
    final waiting = _hw.shipyardPending.where((q) => q.id == order.id).firstOrNull;
    if (waiting != null) _announceWait(order.id, QueueType.ship, '${item.label} x$count', waiting.waitingFor, waiting.waitingOn);
    return [_tickUpdate()];
  }

  void _announceWait(int id, QueueType type, String item, Resources? short, WaitReason on) {
    _ev(QueueEvent(playerId: _playerId, queueType: type, id: id, item: item, action: QueueAction.waiting, waitingFor: short, waitingOn: on));
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

  bool _buildingReady(BuildingType t, int target) =>
      _hw.buildQueue.every((q) => q.buildingType != t) && _bl(t) + 1 == target && _buildingPrereqs(t, _bl);

  bool _researchReady(ResearchType t, int target) => _bl(BuildingType.researchLab) > 0 && _rl(t) + 1 == target;

  WaitReason _waitOn({required bool ready, required Resources cost, required bool slotFree}) {
    if (!ready) return WaitReason.prerequisite;
    if (!_affordable(cost)) return WaitReason.resources;
    if (!slotFree) return WaitReason.slot;
    return WaitReason.order;
  }

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

  void _startBuilding(int id, BuildingType t, int target) {
    final (cost, ticks) = _buildingStep(t, target);
    _pay(cost);
    _hw = _hwWith(buildQueue: [
      ..._hw.buildQueue,
      BuildQueueItem(id: id, buildingType: t, targetLevel: target, startTick: _tick, endTick: _tick + ticks),
    ]);
    _ev(QueueEvent(playerId: _playerId, queueType: QueueType.building, id: id, item: '${t.label} Lv.$target', action: QueueAction.started, paid: cost));
  }

  void _startResearch(int id, ResearchType t, int target) {
    final (cost, ticks) = _researchStep(t, target);
    _pay(cost);
    _hw = _hwWith(researchItem: ResearchItem(id: id, tech: t, targetLevel: target, startTick: _tick, endTick: _tick + ticks));
    _ev(QueueEvent(playerId: _playerId, queueType: QueueType.research, id: id, item: '${t.label} Lv.$target', action: QueueAction.started, paid: cost));
  }

  /// A free slot starts the earliest waiting order that can start now. An
  /// order that cannot keeps its place, unless it reserves: then nothing
  /// behind it starts while it is ready and short.
  int _firstStartable<T>(List<T> orders, bool Function(T) ready, bool Function(T) affordable, bool Function(T) reserve) {
    for (var i = 0; i < orders.length; i++) {
      if (!ready(orders[i])) continue;
      if (affordable(orders[i])) return i;
      if (reserve(orders[i])) return -1;
    }
    return -1;
  }

  void _startWaiting() {
    var again = true;
    while (again) {
      again = false;
      if (_hw.buildQueue.length >= _hw.buildSlots) break;
      final bi = _firstStartable<QueuedBuild>(
          _hw.buildPending, (q) => _buildingReady(q.buildingType, q.targetLevel), (q) => _affordable(q.cost), (q) => q.reserve);
      if (bi >= 0) {
        final q = _hw.buildPending[bi];
        _hw = _hwWith(buildPending: [for (var i = 0; i < _hw.buildPending.length; i++) if (i != bi) _hw.buildPending[i]]);
        _startBuilding(q.id, q.buildingType, q.targetLevel);
        again = true;
      }
    }
    if (_hw.researchActive == null) {
      final ri = _firstStartable<QueuedResearch>(
          _hw.researchPending, (q) => _researchReady(q.tech, q.targetLevel), (q) => _affordable(q.cost), (q) => q.reserve);
      if (ri >= 0) {
        final q = _hw.researchPending[ri];
        _hw = _hwWith(researchPending: [for (var i = 0; i < _hw.researchPending.length; i++) if (i != ri) _hw.researchPending[i]]);
        _startResearch(q.id, q.tech, q.targetLevel);
      }
    }
    if (_hw.shipyardQueue == null) {
      final si = _firstStartable<QueuedShip>(_hw.shipyardPending, (_) => true, (q) => _affordable(q.unitCost), (q) => q.reserve);
      if (si >= 0) {
        final q = _hw.shipyardPending[si];
        _pay(q.unitCost);
        _hw = _hwWith(
          shipPending: [for (var i = 0; i < _hw.shipyardPending.length; i++) if (i != si) _hw.shipyardPending[i]],
          ship: ShipyardQueueItem(id: q.id, item: q.item, count: q.count, built: q.built, startTick: _tick, endTick: _tick + q.ticks),
        );
        if (q.built == 0) {
          _ev(QueueEvent(playerId: _playerId, queueType: QueueType.ship, id: q.id, item: '${q.item.label} x${q.count}', action: QueueAction.started, paid: q.unitCost));
        }
      }
    }
    _refreshWaits();
  }

  /// Bring every waiting order's shortfall and reason up to date.
  void _refreshWaits() {
    final slotFree = _hw.buildQueue.length < _hw.buildSlots;
    // A reserving order that is ready and short holds back everything behind it.
    WaitReason on<T>(List<T> line, int i, bool ready, Resources cost, bool slotFree, bool Function(T) reserve) {
      final base = _waitOn(ready: ready, cost: cost, slotFree: slotFree);
      if (base != WaitReason.slot && base != WaitReason.order) return base;
      return line.take(i).any((e) => reserve(e)) && slotFree ? WaitReason.order : base;
    }

    _hw = _hwWith(
      buildPending: [
        for (var i = 0; i < _hw.buildPending.length; i++)
          () {
            final q = _hw.buildPending[i];
            return QueuedBuild(
              id: q.id,
              buildingType: q.buildingType,
              targetLevel: q.targetLevel,
              reserve: q.reserve,
              cost: q.cost,
              ticks: q.ticks,
              waitingFor: _missing(q.cost),
              waitingOn: on<QueuedBuild>(_hw.buildPending, i, _buildingReady(q.buildingType, q.targetLevel), q.cost, slotFree, (e) => e.reserve),
            );
          }(),
      ],
      researchPending: [
        for (var i = 0; i < _hw.researchPending.length; i++)
          () {
            final q = _hw.researchPending[i];
            return QueuedResearch(
              id: q.id,
              tech: q.tech,
              targetLevel: q.targetLevel,
              reserve: q.reserve,
              cost: q.cost,
              ticks: q.ticks,
              waitingFor: _missing(q.cost),
              waitingOn: on<QueuedResearch>(_hw.researchPending, i, _researchReady(q.tech, q.targetLevel), q.cost, _hw.researchActive == null, (e) => e.reserve),
            );
          }(),
      ],
      shipPending: [
        for (var i = 0; i < _hw.shipyardPending.length; i++)
          () {
            final q = _hw.shipyardPending[i];
            return QueuedShip(
              id: q.id,
              item: q.item,
              count: q.count,
              built: q.built,
              reserve: q.reserve,
              cost: q.cost,
              unitCost: q.unitCost,
              ticks: q.ticks,
              waitingFor: _missing(q.unitCost),
              waitingOn: on<QueuedShip>(_hw.shipyardPending, i, true, q.unitCost, _hw.shipyardQueue == null, (e) => e.reserve),
            );
          }(),
      ],
    );
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
  static const _techShipyard = {
    ResearchType.corvetteTech: 2,
    ResearchType.frigateTech: 4,
    ResearchType.cruiserTech: 6,
    ResearchType.haulerTech: 2,
  };
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
      max(1, ((cost.metal + cost.crystal) / rate * 3600 / speed / _pace / _lapse).ceil());

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
              if (_techShipyard[t] case final l?) needBuilding(BuildingType.shipyard, l),
              if (t == ResearchType.modularFabrication) needBuilding(BuildingType.researchLab, 4),
              if (t == ResearchType.emergencyJump) Requirement(name: ResearchType.navigation.label, need: 3, have: rl(ResearchType.navigation), met: rl(ResearchType.navigation) >= 3),
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

  /// Docked ships, the Defense Grid and structures.
  double _homePower(List<DefenceState> defences, List<BuildingState> buildings) {
    var p = _dockedPower();
    p += _gridPowerPerLevel * buildings.firstWhere((e) => e.buildingType == BuildingType.defenseGrid).level;
    for (final d in defences) {
      p += d.count * d.power;
    }
    return p;
  }

  /// The demo plays the server: mine output at the given building levels.
  static Resources _production(List<BuildingState> buildings) {
    double rate(BuildingType t, double perHour) {
      final l = buildings.firstWhere((b) => b.buildingType == t).level;
      return l == 0 ? 0 : perHour / 3600 * l * pow(1.1, l) * _pace * _lapse;
    }

    return Resources(
      metal: rate(BuildingType.metalMine, 10),
      crystal: rate(BuildingType.crystalMine, 7),
      deuterium: rate(BuildingType.deuteriumSynthesizer, 5),
    );
  }

  // ── Commands ──────────────────────────────────────────────────

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

  void _stopActivities(FleetState f) {
    _recall.remove(f.id);
    _explores.remove(f.id);
    _harvest.remove(f.id);
    final c = _combats.remove(f.id);
    if (c != null) {
      _ev(CombatEndedEvent(sector: c.sector, playerVictory: false, owners: const ['Demo'], mine: true));
    }
  }

  List<ServerMessage> _command(Command c) {
    switch (c) {
      case MoveCommand(:final fleetId, :final target):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        final here = _base(f.location);
        if (!here.connections.contains(target)) {
          return [_err(ErrorCode.noConnection, 'No lane from ${f.location} to $target')];
        }
        if (f.cooldownRemaining > 0) return [_err(ErrorCode.onCooldown, 'Drive recharging: ${f.cooldownRemaining} tick(s)')];
        if (f.fuel < _jump(f.ships)) return [_err(ErrorCode.insufficientFuel, 'Not enough fuel for a jump')];
        _stopActivities(f);
        _hop(f, target);
        return [_tickUpdate()];
      case HarvestCommand(:final fleetId, :final resource):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        final sec = _base(f.location);
        final keys = [
          for (final k in const ['metal', 'crystal', 'deut'])
            if ((resource == HarvestResource.auto || resource.name == k || (resource == HarvestResource.deuterium && k == 'deut')) &&
                _density(sec, k) != Density.none)
              k
        ];
        if (f.location == _home || keys.isEmpty) return [_err(ErrorCode.resourceNotPresent, 'No ${resource == HarvestResource.auto ? 'ore' : resource.name} to harvest here')];
        if (f.cargo.total >= f.cargoCapacity) return [_err(ErrorCode.cargoFull, 'Cargo hold is full')];
        _stopActivities(f);
        _harvest[fleetId] = resource;
        _setFleet(_copy(f, state: FleetStatus.harvesting));
        return [_tickUpdate()];
      case RecallCommand(:final fleetId):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        final path = _pathHome(f.location);
        if (path == null) return [_err(ErrorCode.noConnection, 'No known lane home')];
        _stopActivities(f);
        if (path.isEmpty) {
          _setFleet(_copy(f, state: FleetStatus.docked));
        } else {
          _recall[fleetId] = path;
          _setFleet(_copy(f, state: FleetStatus.returning));
        }
        return [_tickUpdate()];
      case StopCommand(:final fleetId):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        _stopActivities(f);
        _setFleet(_copy(f, state: f.location == _home ? FleetStatus.docked : FleetStatus.idle));
        return [_tickUpdate()];
      case ScanCommand(:final fleetId):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        if (f.cooldownRemaining > 0) return [_err(ErrorCode.onCooldown, 'Scanner recharging: ${f.cooldownRemaining} tick(s)')];
        final range = f.ships.any((s) => s.shipClass == ShipClass.scout) ? 2 : 1;
        final fresh = _reveal(f.location, range, scan: true);
        var hostile = 0;
        for (final h in hexSpiral(f.location, range)) {
          _liveUntil[h.toKey()] = _tick + 120;
          hostile += (_hostiles[h.toKey()]?.length ?? 0);
        }
        _liveNow = _computeLive();
        for (final k in _liveNow) {
          _seen[k] = _tick;
        }
        final signals = <SignalContact>[];
        final far = [...hexRing(f.location, range + 1), ...hexRing(f.location, range + 2)]
            .where((h) => !_seen.containsKey(h.toKey()) && h.distFromOrigin <= _hubRadius)
            .toList()
          ..sort((a, b) => _hash(a.q, a.r, _tick ~/ 5).compareTo(_hash(b.q, b.r, _tick ~/ 5)));
        for (final h in far) {
          if (signals.length >= 3) break;
          final k = h.toKey();
          final b = _base(h);
          final kind = (_hostiles[k]?.isNotEmpty ?? false)
              ? SignalKind.hostileMass
              : _sites.containsKey(k)
                  ? SignalKind.derelict
                  : b.terrain == TerrainType.anomaly
                      ? SignalKind.anomaly
                      : (b.resources.metal.index + b.resources.crystal.index + b.resources.deuterium.index) >= 6
                          ? SignalKind.richOre
                          : null;
          if (kind != null) {
            final hs = _hostiles[k];
            final rating = hs == null || hs.isEmpty ? b.threat.rating : _observedThreat(hs).rating;
            signals.add(SignalContact(sector: h, signal: kind, threatBand: (rating + 2) ~/ 3));
          }
        }
        _setFleet(_copy(f, cooldown: 5));
        _ev(ScanCompletedEvent(
          fleetId: fleetId,
          sector: f.location,
          sectorsRevealed: fresh,
          hostilesDetected: hostile,
          threats: [for (final h in hexSpiral(f.location, range)) SectorThreat(sector: h, threat: _sector(h).threat)],
          signals: signals,
        ));
        return [_tickUpdate()];
      case AttackCommand(:final fleetId, :final targetFleetId):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        if (f.state == FleetStatus.docked) return [_err(ErrorCode.invalidTarget, 'Docked fleets cannot attack')];
        if (_combats.containsKey(fleetId)) return [_err(ErrorCode.invalidCommand, 'Already in combat')];
        final hs = _hostiles[f.location.toKey()] ?? const <NpcFleetInfo>[];
        final npc = hs.where((h) => targetFleetId == 0 || h.id == targetFleetId).firstOrNull;
        if (npc == null) return [_err(ErrorCode.invalidTarget, 'No such hostile in this sector')];
        _stopActivities(f);
        _startCombat(_find(fleetId)!, npc);
        return [_tickUpdate()];
      case CollectSalvageCommand(:final fleetId):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        final k = f.location.toKey();
        final pile = _salvage[k];
        if (pile == null) return [_err(ErrorCode.invalidTarget, 'No salvage in this sector')];
        final free = f.cargoCapacity - f.cargo.total;
        if (free <= 0) return [_err(ErrorCode.cargoFull, 'Cargo hold is full')];
        final res = pile.$1;
        final share = min(1.0, free / max(1, res.total));
        final take = Resources(metal: (res.metal * share).floorToDouble(), crystal: (res.crystal * share).floorToDouble(), deuterium: (res.deuterium * share).floorToDouble());
        final left = Resources(metal: res.metal - take.metal, crystal: res.crystal - take.crystal, deuterium: res.deuterium - take.deuterium);
        if (left.total <= 0) {
          _salvage.remove(k);
        } else {
          _salvage[k] = (left, pile.$2);
        }
        _dirty.add(k);
        _setFleet(_copy(f, cargo: Resources(metal: f.cargo.metal + take.metal, crystal: f.cargo.crystal + take.crystal, deuterium: f.cargo.deuterium + take.deuterium)));
        _ev(SalvageCollectedEvent(fleetId: fleetId, sector: f.location, resources: take, remaining: left.total > 0 ? left : null));
        return [_tickUpdate()];
      case ExploreSiteCommand(:final fleetId):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        final k = f.location.toKey();
        final site = _sites[k];
        if (site == null) return [_err(ErrorCode.invalidTarget, 'No derelict in this sector')];
        if (_hostiles[k]?.isNotEmpty ?? false) return [_err(ErrorCode.invalidTarget, 'Hostiles in range: clear the sector first')];
        _stopActivities(f);
        final end = _tick + 12 * site.tier;
        _explores[fleetId] = _Explore(fleetId, f.location, end);
        _setFleet(_copy(f, state: FleetStatus.exploring));
        _ev(SiteExplorationStartedEvent(fleetId: fleetId, sector: f.location, tier: site.tier, endTick: end));
        return [_tickUpdate()];
      case BuildCommand(:final buildingType, :final reserve):
        final proj = _projectedBuilding(buildingType);
        if (proj >= 20) return [_err(ErrorCode.maxLevelReached, '${buildingType.label} is maxed')];
        if (!_buildingPrereqs(buildingType, _projectedBuilding)) {
          return [_err(ErrorCode.prerequisitesNotMet, 'Prerequisites not met')];
        }
        if (_hw.buildQueue.length + _hw.buildPending.length >= _hw.buildSlots + _hw.queueWaitingMax) {
          return [_err(ErrorCode.queueFull, 'The building queue is full (${_hw.buildSlots} running + ${_hw.queueWaitingMax} waiting)')];
        }
        final (cost, ticks) = _buildingStep(buildingType, proj + 1);
        final id = _newQueueId();
        _hw = _hwWith(buildPending: [
          ..._hw.buildPending,
          QueuedBuild(id: id, buildingType: buildingType, targetLevel: proj + 1, reserve: reserve, cost: cost, ticks: ticks, waitingFor: _missing(cost)),
        ]);
        _startWaiting();
        final waiting = _hw.buildPending.where((q) => q.id == id).firstOrNull;
        if (waiting != null) _announceWait(id, QueueType.building, '${buildingType.label} Lv.${proj + 1}', waiting.waitingFor, waiting.waitingOn);
        return [_tickUpdate()];
      case ResearchCommand(:final tech, :final reserve):
        if (_bl(BuildingType.researchLab) == 0) return [_err(ErrorCode.noResearchLab, 'Build a research lab first')];
        final maxLevel = _shipTech.containsValue(tech) ? 1 : (tech == ResearchType.modularFabrication ? 2 : 5);
        final proj = _projectedResearch(tech);
        if (proj >= maxLevel) return [_err(ErrorCode.maxLevelReached, '${tech.label} is maxed')];
        if (_hw.catalog.research.firstWhere((o) => o.tech == tech).requires.any((r) => !r.met)) {
          return [_err(ErrorCode.prerequisitesNotMet, 'Prerequisites not met')];
        }
        if ((_hw.researchActive == null ? 0 : 1) + _hw.researchPending.length >= 1 + _hw.queueWaitingMax) {
          return [_err(ErrorCode.queueFull, 'The research queue is full (1 running + ${_hw.queueWaitingMax} waiting)')];
        }
        final (cost, ticks) = _researchStep(tech, proj + 1);
        final id = _newQueueId();
        _hw = _hwWith(researchPending: [
          ..._hw.researchPending,
          QueuedResearch(id: id, tech: tech, targetLevel: proj + 1, reserve: reserve, cost: cost, ticks: ticks, waitingFor: _missing(cost)),
        ]);
        _startWaiting();
        final waiting = _hw.researchPending.where((q) => q.id == id).firstOrNull;
        if (waiting != null) _announceWait(id, QueueType.research, '${tech.label} Lv.${proj + 1}', waiting.waitingFor, waiting.waitingOn);
        return [_tickUpdate()];
      case BuildShipCommand(:final shipClass, :final count, :final reserve):
        final o = _hw.catalog.ships.firstWhere((o) => o.shipClass == shipClass);
        if (!o.requires.first.met) return [_err(ErrorCode.noShipyard, 'Build a shipyard first')];
        if (o.requires.any((r) => !r.met)) return [_err(ErrorCode.shipLocked, 'Ship class not unlocked')];
        return _queueYard(ShipyardItem.ship(shipClass), count, reserve);
      case BuildDefenceCommand(:final kind, :final count, :final reserve):
        final o = _hw.catalog.defences.firstWhere((o) => o.kind == kind);
        if (!o.requires.first.met) return [_err(ErrorCode.noShipyard, 'Build a shipyard first')];
        if (o.requires.any((r) => !r.met)) return [_err(ErrorCode.defenceLocked, '${kind.label} is locked')];
        return _queueYard(ShipyardItem.defence(kind), count, reserve);
      case CancelBuildCommand(:final queueType, :final id):
        late final String label;
        late final Resources paid;
        switch (queueType) {
          case QueueType.building:
            final gone = _hw.buildQueue.where((q) => q.id == id).firstOrNull;
            if (gone == null) return [_err(ErrorCode.invalidTarget, 'No order $id is under way in the building queue')];
            label = '${gone.buildingType.label} Lv.${gone.targetLevel}';
            paid = _buildingStep(gone.buildingType, gone.targetLevel).$1;
            _hw = _hwWith(
              buildQueue: [for (final q in _hw.buildQueue) if (q.id != id) q],
              buildPending: [
                for (final q in _hw.buildPending)
                  q.buildingType == gone.buildingType && q.targetLevel > gone.targetLevel
                      ? QueuedBuild(
                          id: q.id,
                          buildingType: q.buildingType,
                          targetLevel: q.targetLevel - 1,
                          reserve: q.reserve,
                          cost: q.cost,
                          ticks: q.ticks,
                          waitingFor: q.waitingFor)
                      : q,
              ],
            );
          case QueueType.ship:
            final gone = _hw.shipyardQueue;
            if (gone == null || gone.id != id) return [_err(ErrorCode.invalidTarget, 'No order $id is under way in the shipyard queue')];
            label = gone.built > 0 ? '${gone.item.label} x${gone.count} (${gone.built} already built and kept)' : '${gone.item.label} x${gone.count}';
            paid = _unitCostOf(gone.item);
            _hw = _hwWith(clearShip: true);
          case QueueType.research:
            final gone = _hw.researchActive;
            if (gone == null || gone.id != id) return [_err(ErrorCode.invalidTarget, 'No order $id is under way in the research queue')];
            label = '${gone.tech.label} Lv.${gone.targetLevel}';
            paid = _researchStep(gone.tech, gone.targetLevel).$1;
            _hw = _hwWith(clearResearch: true, researchPending: [
              for (final q in _hw.researchPending)
                q.tech == gone.tech && q.targetLevel > gone.targetLevel
                    ? QueuedResearch(
                        id: q.id, tech: q.tech, targetLevel: q.targetLevel - 1, reserve: q.reserve, cost: q.cost, ticks: q.ticks, waitingFor: q.waitingFor)
                    : q,
            ]);
        }
        final refund = _times(paid, 0.5);
        _resources = _clampCaps(Resources(
          metal: _resources.metal + refund.metal,
          crystal: _resources.crystal + refund.crystal,
          deuterium: _resources.deuterium + refund.deuterium,
        ));
        _ev(QueueEvent(playerId: _playerId, queueType: queueType, id: id, item: label, action: QueueAction.cancelled, refunded: refund));
        _startWaiting();
        return [_tickUpdate()];
      case CancelQueuedCommand(:final queueType, :final id):
        late final String label;
        switch (queueType) {
          case QueueType.building when _hw.buildPending.any((q) => q.id == id):
            final q = _hw.buildPending.firstWhere((q) => q.id == id);
            label = '${q.buildingType.label} Lv.${q.targetLevel}';
            _hw = _hwWith(buildPending: [
              for (final e in _hw.buildPending)
                if (e.id != id) e,
            ]);
          case QueueType.ship when _hw.shipyardPending.any((q) => q.id == id):
            final q = _hw.shipyardPending.firstWhere((q) => q.id == id);
            label = '${q.item.label} x${q.count}';
            _hw = _hwWith(shipPending: [
              for (final e in _hw.shipyardPending)
                if (e.id != id) e,
            ]);
          case QueueType.research when _hw.researchPending.any((q) => q.id == id):
            final q = _hw.researchPending.firstWhere((q) => q.id == id);
            label = '${q.tech.label} Lv.${q.targetLevel}';
            _hw = _hwWith(researchPending: [
              for (final e in _hw.researchPending)
                if (e.id != id) e,
            ]);
          default:
            return [_err(ErrorCode.invalidTarget, 'No order $id is waiting in the ${queueType.name} queue')];
        }
        _ev(QueueEvent(playerId: _playerId, queueType: queueType, id: id, item: label, action: QueueAction.cancelled));
        _startWaiting();
        return [_tickUpdate()];
      case SplitCommand(:final fleetId, :final shipIds):
        final f = _find(fleetId);
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
        final a = _find(fleetId);
        final b = _find(otherFleetId);
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
        const rows = [
          LeaderboardEntry(rank: 1, name: 'Vega', agent: true, score: 14.2, core: 12.8, combat: 1.4, explore: 0),
          LeaderboardEntry(rank: 2, name: 'Kestrel', agent: false, score: 11.6, core: 9.4, combat: 0.9, explore: 1.3),
          LeaderboardEntry(rank: 3, name: 'Orrery', agent: true, score: 9.9, core: 8.7, combat: 0.6, explore: 0.6),
          LeaderboardEntry(rank: 4, name: 'Demo', agent: false, score: 9.6, core: 9.1, combat: 0.5, explore: 0),
          LeaderboardEntry(rank: 5, name: 'Orrin', agent: false, score: 4.1, core: 4.1, combat: 0, explore: 0),
          LeaderboardEntry(rank: 6, name: 'Pike-2', agent: true, score: 3.4, core: 3.0, combat: 0.4, explore: 0),
        ];
        return [LeaderboardReply(tick: _tick, entries: rows.take(limit).toList(), you: rows[3])];
      case PreviewMoveCommand(:final fleetId, :final target):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        final here = _base(f.location);
        if (!here.connections.contains(target)) {
          return [_err(ErrorCode.noConnection, 'No lane from ${f.location} to $target')];
        }
        final hops = Hex.distance(target, _home);
        final left = f.fuel - f.jumpFuel;
        final threat = _sector(target, live: _liveNow.contains(target.toKey())).threat;
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
            chartedHopsHome: hops,
            fuelToReturn: hops * f.jumpFuel,
            canReturn: left >= hops * f.jumpFuel,
            threat: threat,
            fleetPower: power,
            ratio: ratio,
            label: RatioLabel.forRatio(ratio),
          ),
        ];
    }
  }
}
