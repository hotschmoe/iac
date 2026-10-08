import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:iac_shared/iac_shared.dart' as shared;

import '../hex/hex_math.dart';
import '../models/game_state.dart';
import '../models/hex.dart';
import 'connection_provider.dart';
import 'demo_provider.dart';
import 'state_mapper.dart';

enum MapZoom {
  close(52, 4),
  sector(30, 7),
  region(14, 16);

  final int hexSize;
  final int radius;
  const MapZoom(this.hexSize, this.radius);
}

/// Drives the UI from either the live Dart server or offline demo mode.
class GameController extends ChangeNotifier {
  final GameConnection connection = GameConnection();
  final StateMapper _mapper = StateMapper();
  DemoProvider? _demo;
  StreamSubscription? _msgSub;
  StreamSubscription? _statusSub;

  GameState _state = createDemoState();
  bool live = false;
  bool connecting = false;
  String? connectionError;
  String playerName = 'Admiral';
  String? wsUrl;

  int activeFleet = 0;
  Hex cursorHex = const Hex(4, -2);
  MapZoom mapZoom = MapZoom.sector;
  int mapOffsetX = 0;
  int mapOffsetY = 0;

  /// Server fleet ids parallel to UI fleets list.
  List<int> _fleetIds = [];

  GameState get state => _state;
  bool get isLive => live && connection.connected;

  Future<void> start({String? url, String name = 'Admiral'}) async {
    playerName = name;
    wsUrl = url ?? GameConnection.defaultUrl();
    connecting = true;
    connectionError = null;
    notifyListeners();

    try {
      await connection.connect(wsUrl!);
      _msgSub?.cancel();
      _msgSub = connection.messages.listen(_onServerMessage);
      _statusSub?.cancel();
      _statusSub = connection.status.listen((s) {
        if (s == ConnectionStatus.disconnected || s == ConnectionStatus.error) {
          if (live) {
            connectionError = 'Disconnected from server';
            live = false;
            _fallbackDemo();
            notifyListeners();
          }
        }
      });

      connection.auth(playerName);
      // Wait briefly for full_state; if nothing, fall back to demo
      await Future<void>.delayed(const Duration(milliseconds: 800));
      if (!live) {
        // Still waiting — keep connection open; full_state may arrive late
        connecting = false;
        // Optimistic: if authenticated, mark live on first full_state
        notifyListeners();
      } else {
        connecting = false;
        notifyListeners();
      }
    } catch (e) {
      connectionError = 'Server unavailable ($e) — demo mode';
      connecting = false;
      live = false;
      _fallbackDemo();
      notifyListeners();
    }
  }

  void _fallbackDemo() {
    _demo?.stop();
    _demo = DemoProvider((s) {
      _state = s;
      notifyListeners();
    });
    _state = _demo!.state;
    _demo!.start();
  }

  void stop() {
    _demo?.stop();
    _msgSub?.cancel();
    _statusSub?.cancel();
    connection.disconnect();
  }

  @override
  void dispose() {
    stop();
    connection.dispose();
    super.dispose();
  }

  void _onServerMessage(shared.ServerMessage msg) {
    switch (msg) {
      case shared.AuthResult(:final success, :final message):
        if (!success) {
          connectionError = message ?? 'Auth failed';
          live = false;
          _fallbackDemo();
        }
      case shared.FullState():
        live = true;
        connecting = false;
        connectionError = null;
        _demo?.stop();
        _demo = null;
        _mapper.applyFull(msg);
        _syncFromMapper();
      case shared.TickUpdate():
        if (!live) {
          live = true;
          connecting = false;
          _demo?.stop();
          _demo = null;
        }
        _mapper.applyTick(msg);
        _syncFromMapper();
      case shared.ErrorMessage():
        _mapper.applyError(msg);
        _syncFromMapper();
      case shared.GameEvent():
        _mapper.ingestSharedEvent(msg);
        _syncFromMapper();
    }
  }

  void _syncFromMapper() {
    _state = _mapper.toUiState();
    _fleetIds = _mapper.fleets.map((f) => f.id).toList();
    if (_fleetIds.isNotEmpty) {
      activeFleet = activeFleet.clamp(0, _fleetIds.length - 1);
      final f = _mapper.fleets[activeFleet];
      cursorHex = Hex(f.location.q, f.location.r);
    } else if (_mapper.player != null) {
      cursorHex = Hex(_mapper.player!.homeworld.q, _mapper.player!.homeworld.r);
    }
    notifyListeners();
  }

  int? get activeFleetId {
    if (_fleetIds.isEmpty) return null;
    if (activeFleet < 0 || activeFleet >= _fleetIds.length) return null;
    return _fleetIds[activeFleet];
  }

  // ── Map UI ──────────────────────────────────────────────────────

  void panMap(int dx, int dy) {
    mapOffsetX += dx;
    mapOffsetY += dy;
    notifyListeners();
  }

  void moveCursor(int dq, int dr) {
    cursorHex = Hex(cursorHex.q + dq, cursorHex.r + dr);
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

  // ── Commands ────────────────────────────────────────────────────

  void moveFleet(int direction) {
    if (direction < 0 || direction >= 6) return;
    final fleet = _state.fleets[activeFleet.clamp(0, _state.fleets.length - 1)];
    final dir = Hex.directions[direction];
    final nq = fleet.sector.q + dir.q;
    final nr = fleet.sector.r + dir.r;
    if (!getEdge(fleet.sector.q, fleet.sector.r, nq, nr)) return;

    if (isLive) {
      final id = activeFleetId;
      if (id == null) return;
      connection.sendCommand(
        shared.MoveCommand(
          fleetId: id,
          target: shared.Hex(nq, nr),
        ),
      );
      return;
    }

    // Offline demo local move
    final newSector = Hex(nq, nr);
    final newFleets = List.of(_state.fleets);
    newFleets[activeFleet] = newFleets[activeFleet].copyWith(sector: newSector);
    cursorHex = newSector;
    _state = _state.copyWith(
      fleets: newFleets,
      events: [
        GameEvent(
          tick: _state.tick,
          message: 'Fleet ${fleet.name} moved to [$nq,$nr]',
          level: EventLevel.normal,
        ),
        ..._state.events,
      ].take(12).toList(),
    );
    notifyListeners();
  }

  void handleCommand(String cmd) {
    cmd = cmd.trim().toLowerCase();
    if (cmd.isEmpty) return;

    if (isLive) {
      _handleLiveCommand(cmd);
      return;
    }

    final responses = <String, String>{
      'help':
          'Commands: [1-3]view  [h]arvest  [a]ttack  [b]uild  [r]esearch  [f]leet  [p]olicy  help',
      'harvest': 'Fleet harvesting (demo)',
      'h': 'Fleet harvesting (demo)',
      'attack': 'No hostiles (demo)',
      'a': 'No hostiles (demo)',
      'build': 'Build queue (demo)',
      'b': 'Build queue (demo)',
      'fleet': 'Fleet status (demo)',
      'f': 'Fleet status (demo)',
      'recall': 'Emergency recall (demo)',
      'r': 'Research (demo)',
      'research': 'Research (demo)',
      'status':
          'Tick ${_state.tick} | Metal ${_state.resources.metal.amount}',
    };
    final response =
        responses[cmd] ?? 'Unknown command: "$cmd". Type "help" for commands.';
    _state = _state.copyWith(
      events: [
        GameEvent(tick: _state.tick, message: response, level: EventLevel.normal),
        GameEvent(tick: _state.tick, message: '> $cmd', level: EventLevel.full),
        ..._state.events,
      ].take(12).toList(),
    );
    notifyListeners();
  }

  void _handleLiveCommand(String cmd) {
    final id = activeFleetId;
    void note(String msg) {
      _mapper.pushLocalEvent(
        GameEvent(tick: _mapper.tick, message: msg, level: EventLevel.normal),
      );
      _syncFromMapper();
    }

    note('> $cmd');

    switch (cmd) {
      case 'help':
        note(
          'Live: harvest/h | attack/a | build metal|crystal|shipyard|… | '
          'research fuel|hull|… | ship scout|corvette | recall | stop | status',
        );
      case 'h':
      case 'harvest':
        if (id != null) {
          connection.sendCommand(shared.HarvestCommand(fleetId: id));
        }
      case 'a':
      case 'attack':
        if (id != null) {
          // Attack synthetic hostile id 0 → server spawns from template
          connection.sendCommand(
            shared.AttackCommand(fleetId: id, targetFleetId: 0),
          );
        }
      case 'recall':
        if (id != null) {
          connection.sendCommand(shared.RecallCommand(fleetId: id));
        }
      case 'stop':
        connection.sendCommand(const shared.StopCommand());
      case 'status':
        note(
          'Tick ${_mapper.tick} | '
          'M${_mapper.player?.resources.metal.toStringAsFixed(0)} '
          'C${_mapper.player?.resources.crystal.toStringAsFixed(0)} '
          'D${_mapper.player?.resources.deuterium.toStringAsFixed(0)}',
        );
      case 'f':
      case 'fleet':
        final names = _mapper.fleets
            .map((f) => '${f.name ?? f.id}@${f.location} ${f.state.wireName}')
            .join(' | ');
        note(names.isEmpty ? 'No fleets' : names);
      case 'b':
      case 'build':
        connection.sendCommand(
          const shared.BuildCommand(
            buildingType: shared.BuildingType.metalMine,
          ),
        );
      case 'build metal':
        connection.sendCommand(
          const shared.BuildCommand(
            buildingType: shared.BuildingType.metalMine,
          ),
        );
      case 'build crystal':
        connection.sendCommand(
          const shared.BuildCommand(
            buildingType: shared.BuildingType.crystalMine,
          ),
        );
      case 'build shipyard':
        connection.sendCommand(
          const shared.BuildCommand(
            buildingType: shared.BuildingType.shipyard,
          ),
        );
      case 'build lab':
      case 'build research':
        connection.sendCommand(
          const shared.BuildCommand(
            buildingType: shared.BuildingType.researchLab,
          ),
        );
      case 'build deut':
        connection.sendCommand(
          const shared.BuildCommand(
            buildingType: shared.BuildingType.deuteriumSynthesizer,
          ),
        );
      case 'r':
      case 'research':
        connection.sendCommand(
          const shared.ResearchCommand(
            tech: shared.ResearchType.fuelEfficiency,
          ),
        );
      case 'ship scout':
        connection.sendCommand(
          const shared.BuildShipCommand(shipClass: shared.ShipClass.scout),
        );
      case 'ship corvette':
        connection.sendCommand(
          const shared.BuildShipCommand(shipClass: shared.ShipClass.corvette),
        );
      default:
        if (cmd.startsWith('build ')) {
          note('Try: build metal|crystal|deut|shipyard|lab');
        } else {
          note('Unknown: "$cmd" — type help');
        }
    }
  }
}
