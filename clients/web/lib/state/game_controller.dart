import 'dart:async';

import 'package:flutter/foundation.dart';

import '../models/fleet.dart';
import '../models/game_state.dart';
import '../models/homeworld.dart';
import '../models/resources.dart';
import '../protocol/protocol.dart' as proto;
import 'command_parser.dart';
import 'connection_provider.dart';
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

enum LinkState { idle, connecting, live, reconnecting }

/// The server refused the login. The login form shows why: a different name
/// or token fixes it, or the world has no free home seat and only an
/// existing account can still log in.
class AuthRejection {
  final proto.ErrorCode? code;
  final String message;
  const AuthRejection(this.code, this.message);

  bool get needsToken => code == proto.ErrorCode.tokenRequired || code == proto.ErrorCode.invalidToken;
}

/// A world to log in to: a Rust server, or the engine running in this tab.
class Uplink {
  /// Keys what the browser remembers per world (tokens, per-account settings).
  final String id;

  /// What the console calls the link while it is up.
  final String label;

  /// What the quiet alert says feeds the console.
  final String feed;
  final Dial dial;
  const Uplink({required this.id, required this.label, required this.feed, required this.dial});

  Uplink.server(String url) : this(id: url, label: 'LIVE SERVER', feed: 'Live server feed', dial: webSocketDial(url));
}

/// Drives the UI from a world behind an [Uplink]: its protocol messages go
/// into one [StateMapper], whatever carries them.
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

  StreamSubscription<proto.ServerMessage>? _msgSub;
  StreamSubscription<ConnectionStatus>? _statusSub;
  Timer? _retry;
  int _retryDelaySec = 2;
  bool _disposed = false;

  Uplink? _uplink;
  String playerName = '';

  /// The token this session logs in with: typed by the player, from `?token=`,
  /// or remembered by [TokenStore] for this server and name.
  String? _token;

  /// Set while the server is refusing the login; the UI shows the login form
  /// again with this message.
  AuthRejection? authRejection;

  /// The token of an account created in this session, until the player says
  /// it is saved. The server shows it once; [saved] is whether the browser
  /// kept a copy.
  ({String token, bool saved})? issuedToken;
  bool _tokenNoticed = true;

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
  bool get hasState => _mapper.hasState;

  String get linkLabel => switch (link) {
        LinkState.idle => 'OFFLINE',
        LinkState.connecting => 'CONNECTING',
        LinkState.live => _uplink?.label ?? 'LIVE',
        LinkState.reconnecting => 'RECONNECTING',
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

  /// Identifies the account in this world, for per-account settings.
  String? get account => switch (link) {
        LinkState.live || LinkState.reconnecting => '${_uplink?.id}|${playerName.toLowerCase()}',
        LinkState.idle || LinkState.connecting => null,
      };

  void acknowledgeToken() {
    if (issuedToken == null) return;
    issuedToken = null;
    notifyListeners();
  }

  // ── Connection lifecycle ──────────────────────────────────────

  /// Log in to the server at [params] as [name]. While it cannot be
  /// reached the attempt is repeated in the background and
  /// [connectionError] says why.
  Future<void> start({required ConnectParams params, required String name, String? token}) =>
      open(Uplink.server(params.url), name: name, token: (token ?? '').trim().isEmpty ? params.token : token);

  /// Log in to the world behind [uplink] as [name], with [token] or the one
  /// the browser remembers for that world and name.
  Future<void> open(Uplink uplink, {required String name, String? token}) async {
    _uplink = uplink;
    _mapper.feed = uplink.feed;
    playerName = name;
    final given = token?.trim();
    _token = (given != null && given.isNotEmpty) ? given : _tokens.load(uplink.id, name);
    authRejection = null;
    _retry?.cancel();
    link = LinkState.connecting;
    connectionError = null;
    notifyListeners();
    await _connect();
  }

  Future<void> _connect() async {
    final uplink = _uplink!;
    _authed = false;
    try {
      connection.onProtocolError = (e) {
        _mapper.pushLog('Protocol error: $e', EventLevel.bright);
        _refresh();
      };
      await connection.connect(uplink.dial);
      await _msgSub?.cancel();
      _msgSub = connection.messages.listen(receive);
      await _statusSub?.cancel();
      _statusSub = connection.status.listen(_onStatus);
      connection.auth(playerName, token: _token);
      // No fixed wait: the first full_state flips us to LIVE; an
      // auth_result with success=false (or a closed line) reports failure.
    } catch (e) {
      connectionError = 'No answer from ${uplink.id}';
      notifyListeners();
      _scheduleRetry();
    }
  }

  void _onStatus(ConnectionStatus s) {
    if (s != ConnectionStatus.disconnected && s != ConnectionStatus.error) return;
    switch (link) {
      case LinkState.live:
        // Keep the last state on screen, retry in the background.
        connectionError = 'Disconnected from ${_uplink?.id}';
        link = LinkState.reconnecting;
        _scheduleRetry();
        notifyListeners();
      case LinkState.connecting:
        connectionError = 'Connection closed before login completed';
        notifyListeners();
        _scheduleRetry();
      case LinkState.reconnecting || LinkState.idle:
        break;
    }
  }

  void _scheduleRetry() {
    _retry?.cancel();
    if (_disposed || _uplink == null) return;
    _retry = Timer(Duration(seconds: _retryDelaySec), () async {
      if (_disposed) return;
      _retryDelaySec = (_retryDelaySec * 2).clamp(2, 15);
      if (link == LinkState.reconnecting) notifyListeners();
      await _connect();
    });
  }

  /// One message from the world. The connection calls this; a test may feed
  /// it a scripted world's messages directly.
  @protected
  @visibleForTesting
  void receive(proto.ServerMessage msg) {
    switch (msg) {
      case proto.AuthResult(:final success, :final message, :final code, :final token):
        if (!success) {
          // Not retried: a rejected login will not fix itself.
          connectionError = message ?? 'login refused';
          authRejection = AuthRejection(code, connectionError!);
          _retry?.cancel();
          connection.disconnect();
          link = LinkState.idle;
          notifyListeners();
          return;
        }
        _authed = true;
        authRejection = null;
        final effective = token ?? _token;
        if (effective != null) {
          _token = effective;
          final saved = _tokens.save(_uplink!.id, playerName, effective);
          if (token != null) {
            issuedToken = (token: token, saved: saved);
            _tokenNoticed = false;
          }
        }
      case proto.GameState():
        if (!_authed) return;
        if (link != LinkState.live) {
          _mapper.reset();
          link = LinkState.live;
          connectionError = null;
          _retryDelaySec = 2;
          _cursorInit = false;
        }
        _mapper.apply(msg);
        final issued = issuedToken;
        if (issued != null && !_tokenNoticed) {
          _tokenNoticed = true;
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
    for (final t in _previewTimers.values) {
      t.cancel();
    }
    _previewTimers.clear();
    for (final t in _oddsTimers.values) {
      t.cancel();
    }
    _oddsTimers.clear();
    _oddsWanted.clear();
    _retry?.cancel();
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
  /// [waiting] the [index]th item still waiting (nothing was paid). The
  /// command carries the item's stable id, so a line that moves under the
  /// click cannot change what is cancelled.
  void cancelHomeworldQueue(proto.QueueType queue, {int index = 0, bool waiting = false}) {
    note('> cancel ${waiting ? 'waiting ' : ''}${queue.name}', level: EventLevel.full);
    final ids = _mapper.queueIds(queue);
    final id = (waiting ? ids.waiting : ids.running).elementAtOrNull(index);
    if (id == null) {
      note('cancel: nothing ${waiting ? 'waiting' : 'running'} in the ${queue.name} queue', level: EventLevel.bright);
      return;
    }
    sendCommand(waiting
        ? proto.CancelQueuedCommand(queueType: queue, id: id)
        : proto.CancelBuildCommand(queueType: queue, id: id));
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

  /// Send a client message to the world.
  void send(proto.ClientMessage msg) {
    if (isLive) {
      connection.send(msg);
    } else {
      note('Not connected -- command dropped', level: EventLevel.bright);
    }
  }

  void sendCommand(proto.Command c) => send(proto.CommandMessage(c));

  /// How long a hover or a new route must rest before the server is asked.
  static const previewDebounce = Duration(milliseconds: 200);
  final Map<(int, proto.Hex), Timer> _previewTimers = {};
  final Map<(int, proto.Hex), int> _previewAsked = {};

  /// Ask the server to rate the jump of [fleetId] into the adjacent [target]
  /// (`preview_move`). Debounced, and skipped while a recent (under 3 ticks) answer or request
  /// for the same jump stands; the answer lands in [GameState.previews].
  void requestPreview(int fleetId, proto.Hex target) {
    if (fleetId == 0 || !hasState || !isLive) return;
    final key = (fleetId, target);
    if (!state.previewNeedsRefresh(fleetId, target)) return;
    final asked = _previewAsked[key];
    if (asked != null && state.tick - asked < previewRefreshTicks) return;
    _previewTimers[key]?.cancel();
    _previewTimers[key] = Timer(previewDebounce, () {
      _previewTimers.remove(key);
      if (_disposed || !isLive) return;
      _previewAsked[key] = state.tick;
      sendCommand(proto.PreviewMoveCommand(fleetId: fleetId, target: target));
    });
  }

  /// Drop a request that has not fired yet (the hover ended).
  void cancelPreview(int fleetId, proto.Hex target) => _previewTimers.remove((fleetId, target))?.cancel();

  final Map<int, Timer> _oddsTimers = {};
  final Map<int, Set<proto.Hex>> _oddsWanted = {};
  final Map<(int, proto.Hex), int> _oddsAsked = {};

  /// Ask the server how [fleetId] would fare in [sectors] (`odds`): the
  /// sector it stands in, the hops of a route, a sector under the cursor.
  /// Debounced into one request per fleet, and a sector is skipped while a
  /// recent (under 3 ticks) answer or request for it stands; the answers land
  /// in [GameState.odds].
  void requestOdds(int fleetId, Iterable<proto.Hex> sectors) {
    if (fleetId == 0 || !hasState || !isLive) return;
    final wanted = _oddsWanted.putIfAbsent(fleetId, () => {});
    for (final s in sectors) {
      if (!state.oddsNeedRefresh(fleetId, s)) continue;
      final asked = _oddsAsked[(fleetId, s)];
      if (asked != null && state.tick - asked < previewRefreshTicks) continue;
      wanted.add(s);
    }
    if (wanted.isEmpty || _oddsTimers.containsKey(fleetId)) return;
    _oddsTimers[fleetId] = Timer(previewDebounce, () {
      _oddsTimers.remove(fleetId);
      final ask = (_oddsWanted.remove(fleetId) ?? {}).take(proto.OddsCommand.maxOddsSectors).toList();
      if (_disposed || ask.isEmpty || !isLive) return;
      _oddsAsked.removeWhere((_, tick) => state.tick - tick > 60);
      for (final s in ask) {
        _oddsAsked[(fleetId, s)] = state.tick;
      }
      sendCommand(proto.OddsCommand(fleetId: fleetId, sectors: ask));
    });
  }

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
      queues: {for (final q in proto.QueueType.values) q: _mapper.queueIds(q)},
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
