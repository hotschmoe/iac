import 'dart:async';
import 'dart:math';

import '../models/fleet.dart';
import '../models/game_state.dart';
import '../models/hex.dart';
import '../models/homeworld.dart';
import '../models/resources.dart';

GameState createDemoState() {
  return GameState(
    tick: 4821,
    clockSec: 8061,
    resources: const Resources(
      metal: ResourceStock(amount: 12847, rate: 38, cap: 20000),
      crystal: ResourceStock(amount: 6214, rate: 21, cap: 16000),
      deut: ResourceStock(amount: 2105, rate: 11, cap: 8000),
    ),
    fleets: [
      FleetState(
        name: 'Alpha',
        sector: const Hex(14, -6),
        status: FleetStatus.harvesting,
        shipCount: 4,
        cargoPercent: 67,
        fuelPercent: 54,
        ships: const [
          ShipState(shipClass: 'Scout', count: 1, hull: 80, hullMax: 100),
          ShipState(shipClass: 'Corvette', count: 2, hull: 100, hullMax: 100),
          ShipState(shipClass: 'Hauler', count: 1, hull: 60, hullMax: 100),
        ],
        cargo: const FleetCargo(metal: 98, crystal: 61, deut: 28, capacity: 280),
        fuel: 271,
        fuelMax: 500,
      ),
      const FleetState(
        name: 'Beta',
        sector: Hex(3, -1),
        status: FleetStatus.enRoute,
        shipCount: 2,
        cargoPercent: 0,
        fuelPercent: 88,
        ships: [
          ShipState(shipClass: 'Corvette', count: 1, hull: 100, hullMax: 100),
          ShipState(shipClass: 'Scout', count: 1, hull: 100, hullMax: 100),
        ],
      ),
      const FleetState(
        name: 'Gamma',
        sector: Hex(4, -2),
        status: FleetStatus.docked,
        shipCount: 1,
        cargoPercent: 0,
        fuelPercent: 100,
        ships: [
          ShipState(shipClass: 'Frigate', count: 1, hull: 40, hullMax: 100),
        ],
      ),
    ],
    buildQueue: const [
      QueueItem(name: 'Crystal Mine Lv.5', time: '03:24:11', pct: 62, active: true),
      QueueItem(name: 'Fuel Depot Lv.3', time: 'queued', pct: 0, active: false),
      QueueItem(name: 'Sensor Array Lv.2', time: 'queued', pct: 0, active: false),
    ],
    shipyard: const [
      QueueItem(name: 'Corvette x2', time: '00:12:45', pct: 78, active: true),
      QueueItem(name: 'Frigate x1', time: 'queued', pct: 0, active: false),
    ],
    docked: '3xScout  2xCorvette  1xHauler',
    research: const ResearchState(
      name: 'Fuel Efficiency II',
      time: '01:45:00',
      pct: 24,
      fragments: '8/10',
      completed: [
        'Fuel Efficiency I',
        'Reinforced Hulls I',
        'Corvette Tech',
        'Harvesting Eff. I',
      ],
    ),
    events: const [
      GameEvent(tick: 4821, message: 'Fleet Alpha harvested 24 metal, 8 crystal from [14,-6]', level: EventLevel.normal),
      GameEvent(tick: 4820, message: 'Fleet Beta departed homeworld -> waypoint [3,-1]', level: EventLevel.dim),
      GameEvent(tick: 4818, message: 'Fleet Alpha: MLM scout destroyed -- salvage: 12 metal, 3 crystal', level: EventLevel.bright),
      GameEvent(tick: 4817, message: 'Metal Mine Lv.4 complete', level: EventLevel.dim),
      GameEvent(tick: 4815, message: 'Fleet Alpha engaged MLM scout in [14,-6] -- combat resolved 2 ticks', level: EventLevel.normal),
      GameEvent(tick: 4810, message: 'Research complete: Harvesting Efficiency I', level: EventLevel.dim),
      GameEvent(tick: 4802, message: 'Fleet Gamma docked for repairs -- hull damage 60%', level: EventLevel.dim),
    ],
    alerts: const [
      Alert(
        icon: '!',
        message: 'Fleet Alpha: MLM patrol detected in [15,-6]',
        detail: '3 corvettes -- adjacent sector, approaching\nPolicy: hold position, shields raised',
        level: AlertLevel.glow,
      ),
      Alert(icon: '.', message: 'Fleet Beta: waypoint reached [3,-1]', detail: 'Awaiting orders', level: AlertLevel.dim),
      Alert(icon: '.', message: 'Corvette construction: 12m remaining', detail: '', level: AlertLevel.dim),
    ],
    sector: const SectorInfo(
      terrain: 'Asteroid Field',
      metal: 'Rich',
      crystal: 'Moderate',
      deut: 'Sparse',
      hostile: 'None in sector',
      adjacent: '3 MLM corvettes at [15,-6]',
      exits: '4 of 6',
    ),
    homeworld: const Hex(4, -2),
    waypoints: const [
      Waypoint(id: 'WP-1', coord: Hex(14, -6), note: 'Rich Fe'),
      Waypoint(id: 'WP-2', coord: Hex(8, -2), note: 'Hub gate'),
      Waypoint(id: 'WP-3', coord: Hex(22, -11), note: 'Deep recon'),
    ],
  );
}

class DemoProvider {
  Timer? _timer;
  final Random _rng = Random();
  final void Function(GameState) _onTick;
  GameState _state;

  DemoProvider(this._onTick) : _state = createDemoState();

  GameState get state => _state;

  void start() {
    _timer = Timer.periodic(const Duration(seconds: 1), (_) => _tick());
  }

  void stop() {
    _timer?.cancel();
    _timer = null;
  }

  void _tick() {
    final newTick = _state.tick + 1;
    final newClock = _state.clockSec + 1;

    final newResources = _state.resources.tick();

    // Advance build progress
    final newBuildQueue = _state.buildQueue.map((item) {
      if (item.active) return item.withPct(item.pct + 0.05);
      return item;
    }).toList();

    final newShipyard = _state.shipyard.map((item) {
      if (item.active) return item.withPct(item.pct + 0.15);
      return item;
    }).toList();

    final newResearch = _state.research.withPct(_state.research.pct + 0.02);

    // Fleet cargo jitter
    var newFleets = List<FleetState>.from(_state.fleets);
    if (_rng.nextDouble() > 0.7) {
      newFleets[0] = newFleets[0].copyWith(
        cargoPercent: (newFleets[0].cargoPercent + 1).clamp(0, 100),
      );
    }

    // Random events
    var newEvents = List<GameEvent>.from(_state.events);
    if (newTick % 15 == 0 && newEvents.length < 20) {
      final msgs = [
        GameEvent(
          tick: newTick,
          message: 'Fleet Alpha harvested ${10 + _rng.nextInt(30)} metal from [14,-6]',
          level: EventLevel.normal,
        ),
        GameEvent(
          tick: newTick,
          message: 'Sensor sweep complete -- no new contacts',
          level: EventLevel.dim,
        ),
        GameEvent(
          tick: newTick,
          message: 'MLM patrol movement detected in adjacent sector',
          level: EventLevel.bright,
        ),
        GameEvent(
          tick: newTick,
          message: 'Deuterium synthesizer output: +${5 + _rng.nextInt(10)} units',
          level: EventLevel.dim,
        ),
      ];
      newEvents.insert(0, msgs[_rng.nextInt(msgs.length)]);
      if (newEvents.length > 12) newEvents = newEvents.sublist(0, 12);
    }

    _state = _state.copyWith(
      tick: newTick,
      clockSec: newClock,
      resources: newResources,
      buildQueue: newBuildQueue,
      shipyard: newShipyard,
      research: newResearch,
      fleets: newFleets,
      events: newEvents,
    );

    _onTick(_state);
  }
}
