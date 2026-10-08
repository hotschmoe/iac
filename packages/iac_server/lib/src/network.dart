/// WebSocket + optional static web serving + tick loop.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:iac_shared/iac_shared.dart';
import 'package:shelf/shelf.dart';
import 'package:shelf/shelf_io.dart' as shelf_io;
import 'package:shelf_static/shelf_static.dart';
import 'package:shelf_web_socket/shelf_web_socket.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import 'engine.dart';

class ClientSession {
  final int id;
  final WebSocketChannel channel;
  int? playerId;
  bool authenticated = false;
  StreamSubscription? subscription;

  ClientSession({required this.id, required this.channel});
}

class QueuedMessage {
  final int sessionId;
  final ClientMessage message;

  QueuedMessage(this.sessionId, this.message);
}

class GameServer {
  final GameEngine engine;
  final String host;
  final int port;
  final String? webDir;

  HttpServer? _httpServer;
  Timer? _tickTimer;
  int _nextSessionId = 1;

  final Map<int, ClientSession> sessions = {};
  final List<QueuedMessage> _incoming = [];

  GameServer({
    required this.engine,
    this.host = '0.0.0.0',
    this.port = defaultPort,
    this.webDir,
  });

  Future<void> start() async {
    final wsHandler = webSocketHandler((WebSocketChannel channel, String? _) {
      _onWebSocket(channel);
    });

    Handler handler = (Request request) async {
      if (request.url.path == 'ws' || request.url.pathSegments.isNotEmpty && request.url.pathSegments.first == 'ws') {
        return wsHandler(request);
      }
      // Health
      if (request.url.path == 'health' || request.url.path.isEmpty && request.method == 'GET' && webDir == null) {
        if (request.url.path == 'health') {
          return Response.ok(
            jsonEncode({
              'ok': true,
              'tick': engine.currentTick,
              'players': engine.players.length,
            }),
            headers: {'content-type': 'application/json'},
          );
        }
      }
      return Response.notFound('Not found. Connect WebSocket at /ws');
    };

    final resolvedWeb = _resolveWebDir();
    if (resolvedWeb != null) {
      final staticHandler = createStaticHandler(
        resolvedWeb,
        defaultDocument: 'index.html',
        listDirectories: false,
      );
      final cascade = Cascade()
          .add((Request request) {
            if (request.url.path == 'ws' ||
                (request.url.pathSegments.isNotEmpty &&
                    request.url.pathSegments.first == 'ws')) {
              return wsHandler(request);
            }
            if (request.url.path == 'health') {
              return Response.ok(
                jsonEncode({
                  'ok': true,
                  'tick': engine.currentTick,
                  'players': engine.players.length,
                }),
                headers: {'content-type': 'application/json'},
              );
            }
            return Response.notFound('skip');
          })
          .add(staticHandler);
      handler = cascade.handler;
    } else {
      handler = (Request request) async {
        if (request.url.path == 'ws' ||
            (request.url.pathSegments.isNotEmpty &&
                request.url.pathSegments.first == 'ws')) {
          return wsHandler(request);
        }
        if (request.url.path == 'health') {
          return Response.ok(
            jsonEncode({
              'ok': true,
              'tick': engine.currentTick,
              'players': engine.players.length,
            }),
            headers: {'content-type': 'application/json'},
          );
        }
        return Response.ok(
          _fallbackHtml(),
          headers: {'content-type': 'text/html; charset=utf-8'},
        );
      };
    }

    // CORS-friendly wrapper for Flutter web-server on another port
    final pipeline = Pipeline().addMiddleware(_corsMiddleware()).addHandler(handler);

    _httpServer = await shelf_io.serve(pipeline, host, port);
    // Increase idle timeout for long-lived WS
    _httpServer!.autoCompress = true;

    _tickTimer = Timer.periodic(const Duration(seconds: 1), (_) {
      _onTick();
    });

    final lanIp = await _guessLanIp();
    stdout.writeln('═══════════════════════════════════════════');
    stdout.writeln('  IN AMBER CLAD — Server v0.1.0');
    stdout.writeln('═══════════════════════════════════════════');
    stdout.writeln('Listening:  http://$host:$port');
    stdout.writeln('WebSocket:  ws://$host:$port/ws');
    if (lanIp != null) {
      stdout.writeln('LAN URL:    http://$lanIp:$port');
      stdout.writeln('LAN WS:     ws://$lanIp:$port/ws');
    }
    if (resolvedWeb != null) {
      stdout.writeln('Web dir:    $resolvedWeb');
    }
    final seedHex = engine.worldGen.worldSeed
        .toUnsigned(64)
        .toRadixString(16)
        .toUpperCase()
        .padLeft(16, '0');
    stdout.writeln('World seed: 0x$seedHex');
    stdout.writeln('Tick rate:  $tickRateHz Hz');
    stdout.writeln('───────────────────────────────────────────');
    stdout.writeln('Ready. Auth with: {"auth":{"player_name":"Admiral"}}');
  }

  Future<void> stop() async {
    _tickTimer?.cancel();
    _tickTimer = null;
    for (final s in sessions.values) {
      await s.subscription?.cancel();
      await s.channel.sink.close();
    }
    sessions.clear();
    await _httpServer?.close(force: true);
    _httpServer = null;
  }

  String? _resolveWebDir() {
    if (webDir != null && webDir!.isNotEmpty) {
      final d = Directory(webDir!);
      if (d.existsSync()) return d.absolute.path;
      stderr.writeln('Warning: --web-dir not found: $webDir');
    }
    final env = Platform.environment['IAC_WEB_DIR'];
    if (env != null && env.isNotEmpty) {
      final d = Directory(env);
      if (d.existsSync()) return d.absolute.path;
    }
    // Default relative to package: ../../build/web
    final candidates = [
      Directory('../../build/web'),
      Directory('../../../build/web'),
      Directory('build/web'),
    ];
    for (final c in candidates) {
      if (c.existsSync()) return c.absolute.path;
    }
    return null;
  }

  void _onWebSocket(WebSocketChannel channel) {
    final sessionId = _nextSessionId++;
    final session = ClientSession(id: sessionId, channel: channel);
    sessions[sessionId] = session;

    stdout.writeln('[session $sessionId] connected');

    session.subscription = channel.stream.listen(
      (dynamic data) {
        try {
          final raw = data is String ? data : utf8.decode(data as List<int>);
          final msg = ClientMessage.decode(raw);
          _incoming.add(QueuedMessage(sessionId, msg));
        } catch (e) {
          _send(session, ErrorMessage(
            code: ErrorCode.invalidCommand,
            message: 'Bad message: $e',
          ));
        }
      },
      onDone: () {
        stdout.writeln('[session $sessionId] disconnected');
        sessions.remove(sessionId);
      },
      onError: (Object e) {
        stderr.writeln('[session $sessionId] error: $e');
        sessions.remove(sessionId);
      },
      cancelOnError: true,
    );
  }

  void _onTick() {
    // Drain incoming messages
    final batch = List<QueuedMessage>.from(_incoming);
    _incoming.clear();
    for (final q in batch) {
      try {
        _handleMessage(q.sessionId, q.message);
      } catch (e, st) {
        stderr.writeln('Error handling message: $e\n$st');
      }
    }

    engine.tick();
    _broadcastUpdates();
  }

  void _handleMessage(int sessionId, ClientMessage msg) {
    final session = sessions[sessionId];
    if (session == null) return;

    switch (msg) {
      case AuthRequest(:final playerName):
        if (session.authenticated) {
          _send(
            session,
            ErrorMessage(
              code: ErrorCode.alreadyAuthenticated,
              message: 'Already authenticated',
            ),
          );
          return;
        }
        try {
          final playerId = engine.registerPlayer(playerName);
          session.playerId = playerId;
          session.authenticated = true;
          _send(
            session,
            AuthResult(success: true, playerId: playerId),
          );
          _send(session, engine.buildFullState(playerId));
          stdout.writeln(
            '[session $sessionId] auth ok player=$playerName id=$playerId',
          );
        } on EngineException catch (e) {
          _send(
            session,
            AuthResult(success: false, message: e.message),
          );
        } catch (e) {
          _send(
            session,
            AuthResult(success: false, message: 'Registration failed: $e'),
          );
        }

      case final Command command:
        if (!session.authenticated || session.playerId == null) {
          _send(
            session,
            ErrorMessage(
              code: ErrorCode.authFailed,
              message: 'Not authenticated',
            ),
          );
          return;
        }
        _routeCommand(session, command);

      case RequestFullState():
        if (session.authenticated && session.playerId != null) {
          _send(session, engine.buildFullState(session.playerId!));
        }

      case PolicyUpdate():
        // Not implemented in demo server
        break;
    }
  }

  void _routeCommand(ClientSession session, Command cmd) {
    final playerId = session.playerId!;
    try {
      switch (cmd) {
        case MoveCommand(:final fleetId, :final target):
          _assertOwnsFleet(playerId, fleetId);
          engine.handleMove(fleetId, target);
        case HarvestCommand(:final fleetId):
          _assertOwnsFleet(playerId, fleetId);
          engine.handleHarvest(fleetId);
        case RecallCommand(:final fleetId):
          _assertOwnsFleet(playerId, fleetId);
          engine.handleRecall(fleetId);
        case CollectSalvageCommand(:final fleetId):
          _assertOwnsFleet(playerId, fleetId);
          engine.handleCollectSalvage(fleetId);
        case AttackCommand(:final fleetId, :final targetFleetId):
          _assertOwnsFleet(playerId, fleetId);
          engine.handleAttack(fleetId, targetFleetId);
        case BuildCommand(:final buildingType):
          engine.handleBuild(playerId, buildingType);
        case ResearchCommand(:final tech):
          engine.handleResearch(playerId, tech);
        case BuildShipCommand(:final shipClass, :final count):
          engine.handleBuildShip(playerId, shipClass, count);
        case CancelBuildCommand(:final queueType):
          engine.handleCancelBuild(playerId, queueType);
        case StopCommand():
          engine.handleStop(playerId);
        case ScanCommand():
          // Demo: no-op beyond full state
          _send(session, engine.buildFullState(playerId));
      }
    } on EngineException catch (e) {
      _send(session, ErrorMessage(code: e.code, message: e.message));
    } catch (e) {
      _send(
        session,
        ErrorMessage(code: ErrorCode.invalidCommand, message: '$e'),
      );
    }
  }

  void _assertOwnsFleet(int playerId, int fleetId) {
    final fleet = engine.fleets[fleetId];
    if (fleet == null || fleet.ownerId != playerId) {
      throw EngineException(ErrorCode.fleetNotFound, 'Fleet not found');
    }
  }

  void _broadcastUpdates() {
    for (final session in sessions.values) {
      if (!session.authenticated || session.playerId == null) continue;
      try {
        final update = engine.buildTickUpdate(session.playerId!);
        _send(session, update);
      } catch (e) {
        stderr.writeln(
          'Failed to send tick to session ${session.id}: $e',
        );
      }
    }
  }

  void _send(ClientSession session, ServerMessage msg) {
    try {
      session.channel.sink.add(msg.encode());
    } catch (e) {
      stderr.writeln('Write failed session ${session.id}: $e');
    }
  }

  Future<String?> _guessLanIp() async {
    try {
      for (final iface in await NetworkInterface.list(
        type: InternetAddressType.IPv4,
        includeLinkLocal: false,
      )) {
        for (final addr in iface.addresses) {
          if (!addr.isLoopback) return addr.address;
        }
      }
    } catch (_) {}
    return null;
  }

  String _fallbackHtml() => '''
<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8"/>
  <title>IN AMBER CLAD Server</title>
  <style>
    body { background:#060500; color:#FFB000; font-family: monospace; padding:2rem; }
    code { background:#1a1500; padding:0.2rem 0.4rem; }
  </style>
</head>
<body>
  <h1>IN AMBER CLAD — Server</h1>
  <p>WebSocket endpoint: <code>ws://HOST:$port/ws</code></p>
  <p>Auth example:</p>
  <pre>{"auth":{"player_name":"Admiral"}}</pre>
  <p>Serve Flutter build with <code>--web-dir ../../build/web</code></p>
</body>
</html>
''';
}

Middleware _corsMiddleware() {
  return (Handler inner) {
    return (Request request) async {
      if (request.method == 'OPTIONS') {
        return Response.ok('', headers: _corsHeaders);
      }
      final response = await inner(request);
      return response.change(headers: {...response.headers, ..._corsHeaders});
    };
  };
}

const _corsHeaders = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
  'Access-Control-Allow-Headers': 'Origin, Content-Type, Accept',
};
