/// Lightweight multi-client fuzzer against a live IAC WebSocket server.
///
/// Usage:
///   dart run tool/fuzz_clients.dart --url ws://127.0.0.1:7777/ws --clients 8 --seconds 15
///
/// Reports: connects, auths, messages, commands, errors, tick samples.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:args/args.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

Future<void> main(List<String> args) async {
  final parser = ArgParser()
    ..addOption('url', defaultsTo: 'ws://127.0.0.1:7777/ws')
    ..addOption('clients', abbr: 'c', defaultsTo: '6')
    ..addOption('seconds', abbr: 't', defaultsTo: '12')
    ..addOption('health', defaultsTo: 'http://127.0.0.1:7777/health')
    ..addFlag('help', negatable: false);

  final results = parser.parse(args);
  if (results['help'] as bool) {
    stdout.writeln(parser.usage);
    exit(0);
  }

  final url = results['url'] as String;
  final nClients = int.parse(results['clients'] as String);
  final seconds = int.parse(results['seconds'] as String);
  final healthUrl = results['health'] as String;

  stdout.writeln('Fuzz: $nClients clients → $url for ${seconds}s');

  final stats = FuzzStats();
  final rng = Random(42);
  final clients = <FuzzClient>[];

  for (var i = 0; i < nClients; i++) {
    final c = FuzzClient(
      name: 'FuzzBot_$i',
      url: url,
      rng: Random(rng.nextInt(1 << 30)),
      stats: stats,
    );
    clients.add(c);
    unawaited(c.run());
  }

  final healthTimer = Timer.periodic(const Duration(seconds: 2), (_) async {
    try {
      final client = HttpClient();
      final req = await client.getUrl(Uri.parse(healthUrl));
      final res = await req.close().timeout(const Duration(seconds: 2));
      final body = await res.transform(utf8.decoder).join();
      client.close(force: true);
      final json = jsonDecode(body) as Map<String, dynamic>;
      stats.healthSamples.add(json['tick'] as int? ?? -1);
      stats.healthPlayers = json['players'] as int? ?? stats.healthPlayers;
    } catch (_) {
      stats.healthErrors++;
    }
  });

  await Future<void>.delayed(Duration(seconds: seconds));
  for (final c in clients) {
    await c.stop();
  }
  healthTimer.cancel();

  stdout.writeln('');
  stdout.writeln('═══ Fuzz results ═══');
  stdout.writeln('Connects OK:     ${stats.connects}');
  stdout.writeln('Auth OK:         ${stats.auths}');
  stdout.writeln('Connect fails:   ${stats.connectFails}');
  stdout.writeln('Msgs received:   ${stats.msgsIn}');
  stdout.writeln('Cmds sent:       ${stats.cmdsOut}');
  stdout.writeln('Server errors:   ${stats.serverErrors}');
  stdout.writeln('Parse errors:    ${stats.parseErrors}');
  stdout.writeln('Health errors:   ${stats.healthErrors}');
  stdout.writeln('Health players:  ${stats.healthPlayers}');
  if (stats.healthSamples.isNotEmpty) {
    stdout.writeln(
      'Tick samples:    ${stats.healthSamples.first} → ${stats.healthSamples.last} '
      '(${stats.healthSamples.length} samples)',
    );
  }
  stdout.writeln(
    'Full states: ${stats.fullStates}  Tick updates: ${stats.tickUpdates}',
  );

  final ok = stats.connects > 0 &&
      stats.auths > 0 &&
      stats.connectFails == 0 &&
      stats.msgsIn > 0;
  stdout.writeln(ok ? 'RESULT: PASS' : 'RESULT: FAIL');
  exit(ok ? 0 : 1);
}

class FuzzStats {
  int connects = 0;
  int connectFails = 0;
  int auths = 0;
  int msgsIn = 0;
  int cmdsOut = 0;
  int serverErrors = 0;
  int parseErrors = 0;
  int healthErrors = 0;
  int healthPlayers = 0;
  int fullStates = 0;
  int tickUpdates = 0;
  final List<int> healthSamples = [];
}

class FuzzClient {
  final String name;
  final String url;
  final Random rng;
  final FuzzStats stats;

  WebSocketChannel? _ch;
  StreamSubscription? _sub;
  bool _running = true;
  bool _authed = false;
  int? _fleetId;
  Map<String, dynamic>? _location;
  final List<Map<String, dynamic>> _connections = [];
  Timer? _cmdTimer;

  FuzzClient({
    required this.name,
    required this.url,
    required this.rng,
    required this.stats,
  });

  Future<void> run() async {
    try {
      _ch = WebSocketChannel.connect(Uri.parse(url));
      await _ch!.ready.timeout(const Duration(seconds: 5));
      stats.connects++;
    } catch (e) {
      stats.connectFails++;
      return;
    }

    _sub = _ch!.stream.listen(
      (data) {
        stats.msgsIn++;
        try {
          final raw = data is String ? data : utf8.decode(data as List<int>);
          final json = jsonDecode(raw) as Map<String, dynamic>;
          _handle(json);
        } catch (_) {
          stats.parseErrors++;
        }
      },
      onError: (_) {},
      onDone: () {},
    );

    // Auth (both flat and nested forms occasionally)
    if (rng.nextBool()) {
      _send({'type': 'auth', 'player_name': name});
    } else {
      _send({
        'auth': {'player_name': name},
      });
    }

    // Command loop after short delay
    await Future<void>.delayed(Duration(milliseconds: 200 + rng.nextInt(400)));
    _cmdTimer = Timer.periodic(Duration(milliseconds: 400 + rng.nextInt(600)), (
      _,
    ) {
      if (!_running || !_authed) return;
      _randomCommand();
    });
  }

  void _handle(Map<String, dynamic> json) {
    if (json.containsKey('auth_result')) {
      final ar = json['auth_result'] as Map<String, dynamic>;
      if (ar['success'] == true) {
        _authed = true;
        stats.auths++;
      }
      return;
    }
    if (json.containsKey('error')) {
      stats.serverErrors++;
      return;
    }
    if (json.containsKey('full_state')) {
      stats.fullStates++;
      final fs = json['full_state'] as Map<String, dynamic>;
      _ingestState(fs);
      return;
    }
    if (json.containsKey('tick_update')) {
      stats.tickUpdates++;
      final tu = json['tick_update'] as Map<String, dynamic>;
      if (tu['fleet_updates'] != null) {
        _ingestFleets(tu['fleet_updates'] as List);
      }
      if (tu['sector_updates'] != null) {
        _ingestSectors(tu['sector_updates'] as List);
      }
      return;
    }
    // Flat type form
    switch (json['type']) {
      case 'auth_result':
        if (json['success'] == true) {
          _authed = true;
          stats.auths++;
        }
      case 'error':
        stats.serverErrors++;
      case 'full_state':
        stats.fullStates++;
        _ingestState(json);
      case 'tick_update':
        stats.tickUpdates++;
    }
  }

  void _ingestState(Map<String, dynamic> fs) {
    final fleets = fs['fleets'] as List? ?? [];
    _ingestFleets(fleets);
    final sectors = fs['known_sectors'] as List? ?? [];
    _ingestSectors(sectors);
  }

  void _ingestFleets(List fleets) {
    if (fleets.isEmpty) return;
    final f = fleets.first as Map<String, dynamic>;
    _fleetId = (f['id'] as num?)?.toInt();
    _location = f['location'] as Map<String, dynamic>?;
  }

  void _ingestSectors(List sectors) {
    _connections.clear();
    for (final s in sectors) {
      final m = s as Map<String, dynamic>;
      final conns = m['connections'] as List? ?? [];
      for (final c in conns) {
        if (c is Map<String, dynamic>) _connections.add(c);
      }
    }
  }

  void _randomCommand() {
    final roll = rng.nextInt(100);
    if (_fleetId == null) {
      _send({'request_full_state': null});
      stats.cmdsOut++;
      return;
    }

    if (roll < 35 && _connections.isNotEmpty) {
      final target = _connections[rng.nextInt(_connections.length)];
      _send({
        'move': {'fleet_id': _fleetId, 'target': target},
      });
    } else if (roll < 55) {
      _send({
        'harvest': {'fleet_id': _fleetId, 'resource': 'auto'},
      });
    } else if (roll < 65) {
      _send({
        'build': {'building_type': 'metal_mine'},
      });
    } else if (roll < 72) {
      _send({
        'build': {'building_type': 'crystal_mine'},
      });
    } else if (roll < 78) {
      _send({
        'attack': {'fleet_id': _fleetId, 'target_fleet_id': 0},
      });
    } else if (roll < 84) {
      _send({
        'recall': {'fleet_id': _fleetId},
      });
    } else if (roll < 90) {
      _send({'stop': null});
    } else if (roll < 94) {
      // Malformed / unknown — server should error, not crash
      _send({'type': 'not_a_real_command', 'fleet_id': _fleetId});
    } else if (roll < 97) {
      _send('this is not json');
      stats.cmdsOut++;
      return;
    } else {
      _send({'request_full_state': null});
    }
    stats.cmdsOut++;
  }

  void _send(Object msg) {
    final ch = _ch;
    if (ch == null) return;
    try {
      if (msg is String) {
        ch.sink.add(msg);
      } else {
        ch.sink.add(jsonEncode(msg));
      }
    } catch (_) {}
  }

  Future<void> stop() async {
    _running = false;
    _cmdTimer?.cancel();
    await _sub?.cancel();
    await _ch?.sink.close();
  }
}
