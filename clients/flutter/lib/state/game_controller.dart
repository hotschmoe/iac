import 'package:flutter/foundation.dart';

import '../hex/hex_math.dart';
import '../models/game_state.dart';
import '../models/hex.dart';
import 'demo_provider.dart';

enum MapZoom {
  close(52, 4),
  sector(30, 7),
  region(14, 16);

  final int hexSize;
  final int radius;
  const MapZoom(this.hexSize, this.radius);
}

class GameController extends ChangeNotifier {
  late GameState _state;
  late DemoProvider _demo;

  int activeFleet = 0;
  Hex cursorHex = const Hex(14, -6);
  MapZoom mapZoom = MapZoom.sector;
  int mapOffsetX = 0;
  int mapOffsetY = 0;

  GameController() {
    _demo = DemoProvider(_onTick);
    _state = _demo.state;
  }

  GameState get state => _state;

  void start() => _demo.start();
  void stop() => _demo.stop();

  void _onTick(GameState newState) {
    _state = newState;
    notifyListeners();
  }

  void panMap(int dx, int dy) {
    mapOffsetX += dx;
    mapOffsetY += dy;
    notifyListeners();
  }

  void recenterMap() {
    mapOffsetX = 0;
    mapOffsetY = 0;
    notifyListeners();
  }

  void zoomIn() {
    const zooms = MapZoom.values;
    final idx = zooms.indexOf(mapZoom);
    if (idx > 0) {
      mapZoom = zooms[idx - 1];
      notifyListeners();
    }
  }

  void zoomOut() {
    const zooms = MapZoom.values;
    final idx = zooms.indexOf(mapZoom);
    if (idx < zooms.length - 1) {
      mapZoom = zooms[idx + 1];
      notifyListeners();
    }
  }

  void setZoom(MapZoom zoom) {
    mapZoom = zoom;
    notifyListeners();
  }

  void moveFleet(int direction) {
    if (direction < 0 || direction >= 6) return;
    final fleet = _state.fleets[activeFleet];
    final dir = Hex.directions[direction];
    final nq = fleet.sector.q + dir.q;
    final nr = fleet.sector.r + dir.r;
    if (!getEdge(fleet.sector.q, fleet.sector.r, nq, nr)) return;

    final newSector = Hex(nq, nr);
    final newFleets = List.of(_state.fleets);
    newFleets[activeFleet] = newFleets[activeFleet].copyWith(sector: newSector);

    cursorHex = newSector;

    final newEvents = List.of(_state.events);
    newEvents.insert(
      0,
      GameEvent(
        tick: _state.tick,
        message: 'Fleet ${fleet.name} moved to [$nq,$nr]',
        level: EventLevel.normal,
      ),
    );
    if (newEvents.length > 12) newEvents.removeRange(12, newEvents.length);

    _state = _state.copyWith(fleets: newFleets, events: newEvents);
    notifyListeners();
  }

  void handleCommand(String cmd) {
    final responses = <String, String>{
      'help': 'Commands: [1-3]view  [h]arvest  [a]ttack  [b]uild  [r]esearch  [f]leet  [p]olicy  help',
      'harvest': 'Fleet Alpha: harvesting initiated in sector [14,-6]',
      'h': 'Fleet Alpha: harvesting initiated in sector [14,-6]',
      'attack': 'No hostile targets in current sector. Use windshield view for combat.',
      'a': 'No hostile targets in current sector.',
      'build': 'Build queue: Crystal Mine Lv.5 (62%), Fuel Depot Lv.3, Sensor Array Lv.2',
      'b': 'Build queue: Crystal Mine Lv.5 (62%), Fuel Depot Lv.3, Sensor Array Lv.2',
      'fleet': 'Active fleet: Alpha [14,-6] HARVESTING | Beta [3,-1] EN ROUTE | Gamma DOCKED',
      'f': 'Active fleet: Alpha [14,-6] HARVESTING | Beta [3,-1] EN ROUTE | Gamma DOCKED',
      'recall': 'Emergency recall initiated! Fleet Alpha jumping to homeworld...',
      'r': 'Research: Fuel Efficiency II -- 24% complete, 8/10 fragments',
      'research': 'Research: Fuel Efficiency II -- 24% complete, 8/10 fragments',
      'policy': 'Policy table: 4 rules active. Use "policy edit" to modify.',
      'p': 'Policy table: 4 rules active. Use "policy edit" to modify.',
      'status': 'Tick ${_state.tick} | Metal ${_state.resources.metal.amount} | Crystal ${_state.resources.crystal.amount} | Deut ${_state.resources.deut.amount}',
    };

    final response = responses[cmd] ?? 'Unknown command: "$cmd". Type "help" for commands.';
    final newEvents = List.of(_state.events);
    newEvents.insert(
      0,
      GameEvent(tick: _state.tick, message: '> $cmd', level: EventLevel.full),
    );
    newEvents.insert(
      0,
      GameEvent(tick: _state.tick, message: response, level: EventLevel.normal),
    );
    if (newEvents.length > 12) newEvents.removeRange(12, newEvents.length);

    _state = _state.copyWith(events: newEvents);
    notifyListeners();
  }
}
