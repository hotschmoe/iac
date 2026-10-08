import 'dart:io';

import 'package:args/args.dart';
import 'package:iac_server/iac_server.dart';
import 'package:iac_shared/iac_shared.dart';

Future<void> main(List<String> args) async {
  final parser = ArgParser()
    ..addOption('port', abbr: 'p', defaultsTo: '$defaultPort', help: 'HTTP/WS port')
    ..addOption('host', abbr: 'h', defaultsTo: '0.0.0.0', help: 'Bind address')
    ..addOption(
      'seed',
      abbr: 's',
      defaultsTo: '0x${defaultWorldSeed.toRadixString(16)}',
      help: 'World seed (int or 0xHEX)',
    )
    ..addOption('web-dir', abbr: 'w', help: 'Static Flutter web build directory')
    ..addOption(
      'db',
      abbr: 'd',
      defaultsTo: 'iac.db',
      help: 'SQLite path (use :memory: for no disk)',
    )
    ..addFlag('help', help: 'Show usage', negatable: false);

  late ArgResults results;
  try {
    results = parser.parse(args);
  } on FormatException catch (e) {
    stderr.writeln(e.message);
    stderr.writeln(parser.usage);
    exit(64);
  }

  if (results['help'] as bool) {
    stdout.writeln('IN AMBER CLAD game server\n');
    stdout.writeln(parser.usage);
    stdout.writeln('\nExample:');
    stdout.writeln(
      '  dart run bin/server.dart --port 7777 --host 0.0.0.0 --web-dir ../../build/web --db iac.db',
    );
    exit(0);
  }

  final port = int.tryParse(results['port'] as String) ?? defaultPort;
  final host = results['host'] as String;
  final seed = _parseSeed(results['seed'] as String);
  final webDir = results['web-dir'] as String?;
  final dbPath = results['db'] as String;

  final db = GameDatabase.open(dbPath);
  stdout.writeln('Database:   $dbPath');

  final engine = GameEngine(worldSeed: seed, db: db);
  final server = GameServer(
    engine: engine,
    host: host,
    port: port,
    webDir: webDir,
  );

  await server.start();

  Future<void> shutdown() async {
    stdout.writeln('\nShutting down...');
    engine.persistDirtyState();
    await server.stop();
    db.close();
    exit(0);
  }

  ProcessSignal.sigint.watch().listen((_) => shutdown());
  try {
    ProcessSignal.sigterm.watch().listen((_) => shutdown());
  } catch (_) {
    // SIGTERM may be unavailable on some platforms
  }
}

int _parseSeed(String raw) {
  final s = raw.trim();
  if (s.startsWith('0x') || s.startsWith('0X')) {
    return int.tryParse(s.substring(2), radix: 16) ?? defaultWorldSeed;
  }
  return int.tryParse(s) ?? defaultWorldSeed;
}
