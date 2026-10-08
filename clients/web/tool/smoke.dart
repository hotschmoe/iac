// ignore_for_file: avoid_print, avoid_relative_lib_imports
// Live smoke test against a running Rust server, reusing the app's protocol
// classes (no Flutter needed):
//
//   dart run tool/smoke.dart [ws://127.0.0.1:7777/ws] [player-name] [token]
//
// Connects, authenticates, expects auth_result + full_state, a few
// tick_updates, sends scan and move (when a lane exists), and fails on any
// decode error or unexpected server error.

import 'dart:async';
import 'dart:io';

import '../lib/protocol/protocol.dart';

Never fail(String msg) {
  stderr.writeln('SMOKE FAIL: $msg');
  exit(1);
}

Future<void> main(List<String> args) async {
  final url = args.isNotEmpty ? args[0] : 'ws://127.0.0.1:7777/ws';
  final name = args.length > 1 ? args[1] : 'smoke-${DateTime.now().millisecondsSinceEpoch % 100000}';
  final token = args.length > 2 ? args[2] : null;

  final ws = await WebSocket.connect(url).timeout(const Duration(seconds: 5));
  final inbox = StreamController<ServerMessage>();
  var decoded = 0;
  var decodeErrors = 0;
  var maxGapMs = 0;
  final clock = Stopwatch()..start();
  var lastTickAt = 0;
  ws.listen(
    (data) {
      try {
        final m = ServerMessage.decode(data as String);
        if (m is TickUpdate) {
          final now = clock.elapsedMilliseconds;
          if (lastTickAt > 0 && now - lastTickAt > maxGapMs) maxGapMs = now - lastTickAt;
          lastTickAt = now;
        }
        inbox.add(m);
        decoded++;
      } catch (e) {
        decodeErrors++;
        stderr.writeln('DECODE ERROR: $e\n  frame: $data');
      }
    },
    onDone: inbox.close,
  );
  final it = StreamIterator(inbox.stream);
  void send(ClientMessage m) => ws.add(m.encode());

  Future<T> next<T extends ServerMessage>() async {
    while (await it.moveNext().timeout(const Duration(seconds: 6))) {
      final m = it.current;
      if (m is ErrorMessage) fail('server error ${m.code.wire}: ${m.message}');
      if (m is T) return m;
    }
    fail('stream closed waiting for $T');
  }

  send(AuthRequest(playerName: name, token: token));
  final auth = await next<AuthResult>();
  if (!auth.success) fail('auth rejected (${auth.code?.wire}): ${auth.message}');
  print('auth ok: player_id=${auth.playerId}${auth.token == null ? '' : ' issued token=${auth.token}'}');

  final full = await next<GameState>();
  print('full_state: tick=${full.tick} player=${full.player.name} fleets=${full.fleets.length} '
      'sectors=${full.knownSectors.length} buildings=${full.homeworld.buildings.length}');
  if (full.fleets.isEmpty) fail('no fleets in full_state');
  final fleet = full.fleets.first;
  final home = full.knownSectors.firstWhere((s) => s.location == fleet.location,
      orElse: () => fail('fleet sector missing from known_sectors'));
  print('fleet ${fleet.id} at ${fleet.location}, ${home.connections.length} lanes: ${home.connections}');

  var ticks = 0;
  Hex? lastLoc;
  send(CommandMessage(ScanCommand(fleetId: fleet.id)));
  var scanSeen = false;
  var moveSent = false;
  var cooldown = 99;
  final deadline = DateTime.now().add(const Duration(seconds: 60));
  while (DateTime.now().isBefore(deadline) && (ticks < 5 || !scanSeen || !moveSent)) {
    if (!await it.moveNext().timeout(const Duration(seconds: 6))) fail('stream closed early');
    final m = it.current;
    switch (m) {
      case ErrorMessage():
        print('server error (reported, not fatal for scan/move): ${m.code.wire}: ${m.message}');
        if (m.code == ErrorCode.authFailed) fail('auth lost');
      case TickUpdate():
        ticks++;
        final mine = m.fleets.where((f) => f.id == fleet.id);
        if (mine.isNotEmpty) {
          lastLoc = mine.first.location;
          cooldown = mine.first.cooldownRemaining;
        }
        for (final e in m.events ?? const <GameEvent>[]) {
          if (e.kind is ScanCompletedEvent) {
            scanSeen = true;
            final k = e.kind as ScanCompletedEvent;
            print('scan_completed: revealed=${k.sectorsRevealed} hostiles=${k.hostilesDetected} signals=${k.signals.length}');
          }
        }
        if (!moveSent && scanSeen && cooldown == 0 && home.connections.isNotEmpty) {
          send(CommandMessage(MoveCommand(fleetId: fleet.id, target: home.connections.first)));
          moveSent = true;
          print('sent move -> ${home.connections.first}');
        }
      default:
        break;
    }
    if (moveSent && ticks >= 5 && scanSeen) break;
  }
  // Give the move a few ticks to land.
  for (var i = 0; i < 12 && lastLoc != home.connections.first; i++) {
    if (!await it.moveNext().timeout(const Duration(seconds: 6))) break;
    final m = it.current;
    if (m is TickUpdate) {
      ticks++;
      final f = m.fleets.where((f) => f.id == fleet.id);
      if (f.isNotEmpty) lastLoc = f.first.location;
    }
  }
  if (lastLoc != home.connections.first) fail('fleet did not arrive at ${home.connections.first} (at $lastLoc)');
  print('ticks=$ticks scan_seen=$scanSeen moved_to=$lastLoc decoded=$decoded decode_errors=$decodeErrors max_tick_gap_ms=$maxGapMs');

  send(const RequestFullState());
  final again = await next<GameState>();
  print('re-sync full_state tick=${again.tick} sectors=${again.knownSectors.length}');

  await ws.close();
  if (decodeErrors > 0) fail('$decodeErrors decode errors');
  if (ticks < 3) fail('fewer than 3 tick_updates ($ticks)');
  if (!scanSeen) fail('no scan_completed event');
  print('SMOKE OK');
  exit(0);
}
