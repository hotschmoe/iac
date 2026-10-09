import 'dart:async';

import 'package:flutter/foundation.dart';

import '../models/fleet.dart';
import '../models/game_state.dart';
import '../models/homeworld.dart';
import '../models/resources.dart';
import '../protocol/protocol.dart' as proto;
import 'command_parser.dart';
import 'connection_provider.dart';
import 'demo_provider.dart';
import 'state_mapper.dart';
import 'token_store.dart';

enum MapZoom {
  close(52, 4),
  sector(30, 7),
  region(14, 16);

  final int hexSize;
  final int radius;
  const MapZoom(this.hexSize, this.radius);
}

enum HomeworldTab {
  buildings('BUILDINGS'),
  research('RESEARCH'),
  shipyard('SHIPYARD'),
  defence('DEFENCE'),
  storage('STORAGE');

  final String label;
  const HomeworldTab(this.label);
}

/// Ship batch sizes the homeworld view offers.
const shipBatches = [1, 5, 10];

enum LinkState { idle, connecting, live, reconnecting, demo }

/// The server refused the login in a way the player can fix by typing a
/// different name or token.
class AuthRejection {
  final proto.ErrorCode? code;
  final String message;
  const AuthRejection(this.code, this.message);

  bool get needsToken => code == proto.ErrorCode.tokenRequired || code == proto.ErrorCode.invalidToken;
}

/// Drives the UI from either the live Rust server or the offline demo.
/// Both feed the same protocol messages into one [StateMapper].
class GameController extends ChangeNotifier {
  GameController({TokenStore? tokens}) : _tokens = tokens ?? TokenStore.platform();

  final TokenStore _tokens;
  final GameConnection connection = GameConnection();
  final StateMapper _mapper = StateMapper();
  final StreamController<proto.GameEvent> _events = StreamController.broadcast(sync: true);

  /// Every server game event as it arrives (combat rounds included, which
  /// the log drops), for scenes that animate them.
  Stream<proto.GameEvent> get gameEvents => _events.stream;

  void _tap(proto.ServerMessage m) {
    switch (m) {
      case proto.TickUpdate(:final events):
        for (final e in events ?? const <proto.GameEvent>[]) {
          _events.add(e);
        }
      case proto.GameEvent():
        _events.add(m);
      default:
        break;
    }
  }
  DemoProvider? _demo;
  StreamSubscription<proto.ServerMessage>? _msgSub;
  StreamSubscription<ConnectionStatus>? _statusSub;
  Timer? _retry;
  int _retryDelaySec = 2;
  bool _disposed = false;

  ConnectParams? _params;
  String playerName = '';

  /// The token this session logs in with: typed by the player, from `?token=`,
  /// or remembered by [TokenStore] for this server and name.
  String? _token;

  /// Set while the server is refusing the login; the UI shows the login form
  /// again with this message.
  AuthRejection? authRejection;
  ({String token, bool saved})? _issuedToken;

  LinkState link = LinkState.idle;
  String? connectionError;
  bool _authed = false;

  GameState _state = GameState(
    tick: 0,
    clockSec: 0,
    resources: _zeroRes,
    fleets: const [],
    buildQueue: const [],
    shipyard: const [],
    docked: 'none',
    research: const ResearchState(name: 'Idle', time: '—', pct: 0, completed: []),
    events: const [],
    alerts: const [],
    sector: SectorInfo.unknown,
    homeworld: proto.Hex.origin,
    waypoints: const [],
  );

  static const _zeroRes = Resources(
    metal: ResourceStock(amount: 0, rate: 0),
    crystal: ResourceStock(amount: 0, rate: 0),
    deut: ResourceStock(amount: 0, rate: 0),
  );

  int activeFleet = 0;
  proto.Hex cursorHex = proto.Hex.origin;
  MapZoom mapZoom = MapZoom.sector;
  int mapOffsetX = 0;
  int mapOffsetY = 0;
  bool _cursorInit = false;

  HomeworldTab hwTab = HomeworldTab.buildings;
  int hwCursor = 0;
  int shipBatch = shipBatches.first;

  /// Cards per row in the homeworld grid; written by the view on layout so
  /// up/down keys know the row length.
  int hwColumns = 1;

  GameState get state => _state;
  bool get isLive => link == LinkState.live;
  bool get isDemo => link == LinkState.demo;
  bool get hasState => _mapper.hasState;

  String get linkLabel => switch (link) {
        LinkState.idle => 'OFFLINE',
        LinkState.connecting => 'CONNECTING',
        LinkState.live => 'LIVE SERVER',
        LinkState.reconnecting => 'RECONNECTING',
        LinkState.demo => 'DEMO',
      };

  /// The selected fleet, or a placeholder docked at home when there is none
  /// (e.g. every fleet was lost) so views never index an empty list.
  FleetState get currentFleet {
    if (activeFleet >= 0 && activeFleet < _state.fleets.length) return _state.fleets[activeFleet];
    return FleetState(
      id: 0,
      name: '—',
      sector: _state.homeworld,
      status: FleetStatus.docked,
      shipCount: 0,
    );
  }

  /// Server fleet id of the selected fleet (from state, never hard-coded).
  int? get activeFleetId {
    if (activeFleet < 0 || activeFleet >= _state.fleets.length) return null;
    return _state.fleets[activeFleet].id;
  }

  proto.SectorState? sectorAt(proto.Hex h) => _state.sectors[h];

  // ── Connection lifecycle ──────────────────────────────────────

  /// Connect to the server; on failure fall back to demo mode and keep
  /// retrying in the background.
  Future<void> start({required ConnectParams params, required String name, String? token}) async {
    _params = params;
    playerName = name;
    final typed = token?.trim();
    _token = (typed != null && typed.isNotEmpty) ? typed : (params.token ?? _tokens.load(params.url, name));
    authRejection = null;
    _retry?.cancel();
    link = LinkState.connecting;
    connectionError = null;
    notifyListeners();
    await _connect();
  }

  /// Skip the server entirely.
  void startDemo() {
    _retry?.cancel();
    connection.disconnect();
    connectionError = null;
    _enterDemo();
  }

  Future<void> _connect() async {
    final p = _params!;
    _authed = false;
    try {
      connection.onProtocolError = (e) {
        _mapper.pushLog('Protocol error: $e', EventLevel.bright);
        _refresh();
      };
      await connection.connect(p.url);
      await _msgSub?.cancel();
      _msgSub = connection.messages.listen(_onServerMessage);
      await _statusSub?.cancel();
      _statusSub = connection.status.listen(_onStatus);
      connection.auth(playerName, token: _token);
      // No fixed wait: the first full_state flips us to LIVE; an
      // auth_result with success=false (or a socket close) reports failure.
    } catch (e) {
      connectionError = 'Server unavailable at ${p.url}';
      _fallbackToDemo();
      _scheduleRetry();
    }
  }

  void _onStatus(ConnectionStatus s) {
    if (s != ConnectionStatus.disconnected && s != ConnectionStatus.error) return;
    switch (link) {
      case LinkState.live:
        // Keep the last state on screen, retry in the background.
        connectionError = 'Disconnected from server';
        link = LinkState.reconnecting;
        _scheduleRetry();
        notifyListeners();
      case LinkState.connecting:
        connectionError = 'Connection closed before login completed';
        _fallbackToDemo();
        _scheduleRetry();
      case LinkState.demo:
        _scheduleRetry(); // a background retry attempt was cut short
      case LinkState.reconnecting || LinkState.idle:
        break;
    }
  }

  /// Offline fallback; never clobbers a live (reconnecting) view or an
  /// already-running demo.
  void _fallbackToDemo() {
    if (link == LinkState.reconnecting || link == LinkState.demo) {
      notifyListeners();
      return;
    }
    _enterDemo();
  }

  void _scheduleRetry() {
    _retry?.cancel();
    if (_disposed || _params == null) return;
    _retry = Timer(Duration(seconds: _retryDelaySec), () async {
      if (_disposed) return;
      _retryDelaySec = (_retryDelaySec * 2).clamp(2, 15);
      if (link == LinkState.reconnecting) notifyListeners();
      await _connect();
    });
  }

  void _enterDemo() {
    _demo?.stop();
    _mapper.reset();
    link = LinkState.demo;
    _demo = DemoProvider(_onDemoMessages);
    _mapper.apply(_demo!.initial());
    _demo!.start();
    _cursorInit = false;
    _refresh();
  }

  void _leaveDemo() {
    _demo?.stop();
    _demo = null;
  }

  void _onDemoMessages(List<proto.ServerMessage> msgs) {
    for (final m in msgs) {
      _mapper.apply(m);
      _tap(m);
    }
    _refresh();
  }

  void _onServerMessage(proto.ServerMessage msg) {
    switch (msg) {
      case proto.AuthResult(:final success, :final message, :final code, :final token):
        if (!success) {
          // Not retried: a rejected login will not fix itself.
          connectionError = message ?? 'login refused';
          if (code == proto.ErrorCode.tokenRequired ||
              code == proto.ErrorCode.invalidToken ||
              code == proto.ErrorCode.invalidName) {
            authRejection = AuthRejection(code, connectionError!);
          }
          _retry?.cancel();
          connection.disconnect();
          if (link == LinkState.reconnecting) link = LinkState.connecting;
          _fallbackToDemo();
          return;
        }
        _authed = true;
        authRejection = null;
        final effective = token ?? _token;
        if (effective != null) {
          _token = effective;
          final saved = _tokens.save(_params!.url, playerName, effective);
          if (token != null) _issuedToken = (token: token, saved: saved);
        }
      case proto.GameState():
        if (!_authed) return;
        if (link != LinkState.live) {
          _leaveDemo();
          _mapper.reset();
          link = LinkState.live;
          connectionError = null;
          _retryDelaySec = 2;
          _cursorInit = false;
        }
        _mapper.apply(msg);
        final issued = _issuedToken;
        if (issued != null) {
          _issuedToken = null;
          _mapper.noticeToken(issued.token, saved: issued.saved);
        }
      default:
        if (link != LinkState.live) return; // ignore strays before full_state
        _mapper.apply(msg);
        _tap(msg);
    }
    _refresh();
  }

  void _refresh() {
    final prevId = activeFleetId;
    final n = _mapper.fleets.length;
    activeFleet = n == 0 ? 0 : activeFleet.clamp(0, n - 1);
    _state = _mapper.toUiState(activeFleet: activeFleet);
    // Keep the selected fleet selected when the list order changes.
    if (prevId != null && activeFleetId != prevId) {
      final i = _state.fleets.indexWhere((f) => f.id == prevId);
      if (i >= 0) {
        activeFleet = i;
        _state = _mapper.toUiState(activeFleet: activeFleet);
      }
    }
    if (!_cursorInit && _mapper.hasState) {
      cursorHex = currentFleet.sector;
      _cursorInit = true;
    }
    notifyListeners();
  }

  void stop() {
    _retry?.cancel();
    _demo?.stop();
    _msgSub?.cancel();
    _statusSub?.cancel();
    connection.disconnect();
  }

  @override
  void dispose() {
    _disposed = true;
    stop();
    connection.dispose();
    _events.close();
    super.dispose();
  }

  // ── Map UI ────────────────────────────────────────────────────

  void cycleFleet() {
    if (_state.fleets.isEmpty) return;
    selectFleet((activeFleet + 1) % _state.fleets.length);
  }

  void selectFleet(int index) {
    if (index < 0 || index >= _state.fleets.length) return;
    activeFleet = index;
    cursorHex = currentFleet.sector;
    mapOffsetX = 0;
    mapOffsetY = 0;
    _refresh();
  }

  void panMap(int dx, int dy) {
    mapOffsetX += dx;
    mapOffsetY += dy;
    notifyListeners();
  }

  void moveCursor(int dq, int dr) {
    cursorHex = proto.Hex(cursorHex.q + dq, cursorHex.r + dr);
    notifyListeners();
  }

  void recenterMap() {
    mapOffsetX = 0;
    mapOffsetY = 0;
    cursorHex = currentFleet.sector;
    notifyListeners();
  }

  void zoomIn() {
    final idx = MapZoom.values.indexOf(mapZoom);
    if (idx > 0) {
      mapZoom = MapZoom.values[idx - 1];
      notifyListeners();
    }
  }

  void zoomOut() {
    final idx = MapZoom.values.indexOf(mapZoom);
    if (idx < MapZoom.values.length - 1) {
      mapZoom = MapZoom.values[idx + 1];
      notifyListeners();
    }
  }

  void setZoom(MapZoom zoom) {
    mapZoom = zoom;
    notifyListeners();
  }

  // ── Homeworld ─────────────────────────────────────────────────

  int get hwCardCount {
    final c = _state.catalog;
    if (c == null) return 0;
    return switch (hwTab) {
      HomeworldTab.buildings => c.buildings.length,
      HomeworldTab.shipyard => c.ships.length,
      HomeworldTab.defence => c.defences.length,
      HomeworldTab.research => c.research.length,
      HomeworldTab.storage => 0,
    };
  }

  void selectHomeworldTab(HomeworldTab tab) {
    if (tab == hwTab) return;
    hwTab = tab;
    hwCursor = 0;
    notifyListeners();
  }

  void cycleHomeworldTab(int delta) {
    final n = HomeworldTab.values.length;
    selectHomeworldTab(HomeworldTab.values[(hwTab.index + delta + n) % n]);
  }

  /// Move the card selection by whole cards ([dx]) or whole rows ([dy]).
  void moveHomeworldCursor(int dx, int dy) {
    final n = hwCardCount;
    if (n == 0) return;
    hwCursor = (hwCursor + dx + dy * hwColumns).clamp(0, n - 1);
    notifyListeners();
  }

  void selectHomeworldCard(int index) {
    hwCursor = index;
    notifyListeners();
  }

  void setShipBatch(int n) {
    if (!shipBatches.contains(n) || n == shipBatch) return;
    shipBatch = n;
    notifyListeners();
  }

  void cycleShipBatch(int delta) {
    final i = shipBatches.indexOf(shipBatch);
    setShipBatch(shipBatches[(i + delta).clamp(0, shipBatches.length - 1)]);
  }

  /// Queue the card at [index] (the selection by default). Locked and
  /// maxed cards are answered locally; the server stays the judge of
  /// everything else, including whether the player can pay.
  void activateHomeworldCard([int? index]) {
    final c = _state.catalog;
    final i = index ?? hwCursor;
    if (c == null || i < 0 || i >= hwCardCount) return;
    hwCursor = i;

    final List<proto.Requirement> requires;
    final bool maxed;
    final String name;
    final proto.Command command;
    final String echo;
    switch (hwTab) {
      case HomeworldTab.buildings:
        final o = c.buildings[i];
        requires = o.requires;
        maxed = o.next == null;
        name = o.buildingType.label;
        command = proto.BuildCommand(buildingType: o.buildingType);
        echo = 'build ${o.buildingType.label}';
      case HomeworldTab.research:
        final o = c.research[i];
        requires = o.requires;
        maxed = o.next == null;
        name = o.tech.label;
        command = proto.ResearchCommand(tech: o.tech);
        echo = 'research ${o.tech.label}';
      case HomeworldTab.shipyard:
        final o = c.ships[i];
        requires = o.requires;
        maxed = false;
        name = o.shipClass.label;
        command = proto.BuildShipCommand(shipClass: o.shipClass, count: shipBatch);
        echo = 'ship ${o.shipClass.label} x$shipBatch';
      case HomeworldTab.defence:
        final o = c.defences[i];
        requires = o.requires;
        maxed = false;
        name = o.kind.label;
        command = proto.BuildDefenceCommand(kind: o.kind, count: shipBatch);
        echo = 'defence ${o.kind.label} x$shipBatch';
      case HomeworldTab.storage:
        return;
    }

    final unmet = [for (final r in requires) if (!r.met) r.label];
    if (unmet.isNotEmpty) {
      note('$name is locked: needs ${unmet.join(', ')}', level: EventLevel.bright);
    } else if (maxed) {
      note('$name is already at its maximum level', level: EventLevel.bright);
    } else {
      note('> $echo', level: EventLevel.full);
      sendCommand(command);
    }
  }

  // ── Fleet management ──────────────────────────────────────────

  /// Other fleets in the selected fleet's sector, which can be merged in.
  List<FleetState> get mergeCandidates {
    final cur = currentFleet;
    if (cur.id == 0) return const [];
    return [
      for (final f in _state.fleets)
        if (f.id != cur.id && f.sector == cur.sector) f,
    ];
  }

  /// Split [count] ships of [shipClass] off the selected fleet.
  void splitShips(String shipClass, int count) {
    final id = activeFleetId;
    if (id == null) {
      note('No fleet to split', level: EventLevel.bright);
      return;
    }
    final ids = [
      for (final g in currentFleet.ships)
        if (g.shipClass.toLowerCase() == shipClass.toLowerCase()) ...g.ids,
    ];
    if (count < 1 || ids.length < count) {
      note('split: ${currentFleet.name} has ${ids.length} $shipClass', level: EventLevel.bright);
      return;
    }
    if (count >= currentFleet.shipCount) {
      note('split: at least one ship must stay in the fleet', level: EventLevel.bright);
      return;
    }
    note('> split $count $shipClass off ${currentFleet.name}', level: EventLevel.full);
    sendCommand(proto.SplitCommand(fleetId: id, shipIds: ids.take(count).toList()));
  }

  /// Split exact ships (chosen by the fleet panel) off the selected fleet.
  void splitShipIds(List<int> shipIds) {
    final id = activeFleetId;
    if (id == null || shipIds.isEmpty) return;
    if (shipIds.length >= currentFleet.shipCount) {
      note('split: at least one ship must stay in the fleet', level: EventLevel.bright);
      return;
    }
    note('> split ${shipIds.length} ship(s) off ${currentFleet.name}', level: EventLevel.full);
    sendCommand(proto.SplitCommand(fleetId: id, shipIds: shipIds));
  }

  /// Fold fleet [otherFleetId] into the selected fleet.
  void mergeFleet(int otherFleetId) {
    final id = activeFleetId;
    if (id == null) return;
    note('> merge fleet $otherFleetId into ${currentFleet.name}', level: EventLevel.full);
    sendCommand(proto.MergeCommand(fleetId: id, otherFleetId: otherFleetId));
  }

  /// Cancel the [index]th running item of [queue] (half refunded), or with
  /// [waiting] the [index]th item still waiting (nothing was paid).
  void cancelHomeworldQueue(proto.QueueType queue, {int index = 0, bool waiting = false}) {
    note('> cancel ${waiting ? 'waiting ' : ''}${queue.name}', level: EventLevel.full);
    sendCommand(waiting
        ? proto.CancelQueuedCommand(queueType: queue, index: index)
        : proto.CancelBuildCommand(queueType: queue, index: index));
  }

  /// Cancel the queue that belongs to the open homeworld tab.
  void cancelCurrentHomeworldQueue() => cancelHomeworldQueue(switch (hwTab) {
        HomeworldTab.buildings => proto.QueueType.building,
        HomeworldTab.shipyard => proto.QueueType.ship,
        HomeworldTab.research => proto.QueueType.research,
        HomeworldTab.defence || HomeworldTab.storage => proto.QueueType.building,
      });

  // ── Commands ──────────────────────────────────────────────────

  /// Log a line locally (client-side feedback, not from the server).
  void note(String msg, {EventLevel level = EventLevel.normal}) {
    _mapper.pushLog(msg, level);
    _refresh();
  }

  /// Send a client message to the server (or the demo).
  void send(proto.ClientMessage msg) {
    if (isLive) {
      connection.send(msg);
    } else if (isDemo && _demo != null) {
      _onDemoMessages(_demo!.handle(msg));
    } else {
      note('Not connected -- command dropped', level: EventLevel.bright);
    }
  }

  void sendCommand(proto.Command c) => send(proto.CommandMessage(c));

  /// Windshield: fly the active fleet one hop in [direction] (0-5, same
  /// order as [proto.HexDirection]) if the server lists that lane.
  void moveFleet(int direction) {
    if (direction < 0 || direction >= 6) return;
    final id = activeFleetId;
    if (id == null) {
      note('No fleet to move', level: EventLevel.bright);
      return;
    }
    final target = currentFleet.sector.neighbor(proto.HexDirection.values[direction]);
    if (!_connected(currentFleet.sector, target)) {
      note('No lane ${proto.HexDirection.values[direction].label} from ${currentFleet.sector}');
      return;
    }
    sendCommand(proto.MoveCommand(fleetId: id, target: target));
  }

  /// Star map: fly one hop to the cursor sector (must be an adjacent lane).
  void moveToCursor() {
    final id = activeFleetId;
    if (id == null) {
      note('No fleet to move', level: EventLevel.bright);
      return;
    }
    final from = currentFleet.sector;
    if (!_connected(from, cursorHex)) {
      note('Cursor $cursorHex is not a lane from $from (moves are one hop)');
      return;
    }
    sendCommand(proto.MoveCommand(fleetId: id, target: cursorHex));
  }

  bool _connected(proto.Hex from, proto.Hex to) =>
      sectorAt(from)?.connections.contains(to) ?? false;

  /// Command-bar entry.
  void handleCommand(String text) {
    final cmd = text.trim();
    if (cmd.isEmpty) return;
    note('> $cmd', level: EventLevel.full);

    final ctx = CommandContext(
      fleetId: activeFleetId,
      fleetSector: activeFleetId == null ? null : currentFleet.sector,
      sectors: _state.sectors,
      fleets: _mapper.fleets,
      cursor: cursorHex,
      tick: _mapper.tick,
    );
    final result = parseCommand(cmd, ctx);
    switch (result) {
      case ParsedSend(:final messages):
        for (final m in messages) {
          send(m);
        }
      case ParsedLocal(:final lines):
        for (final l in lines.reversed) {
          note(l); // log is newest-first, so reverse to read top-down
        }
      case ParsedSelectFleet(:final index):
        selectFleet(index);
        note('Active fleet: ${currentFleet.name} at ${currentFleet.sector}');
      case ParsedError(:final message):
        note(message, level: EventLevel.bright);
    }
  }
}
