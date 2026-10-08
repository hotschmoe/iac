/// Convenience launcher: run the game server serving the Flutter web build.
///
/// Usage (from repo root):
///   dart run tool/serve.dart
///   dart run tool/serve.dart --port 7777 --web-dir build/web
library;

import 'dart:io';

Future<void> main(List<String> args) async {
  final root = Directory.current.path;
  final serverDir = Directory('$root/packages/iac_server');
  if (!serverDir.existsSync()) {
    stderr.writeln('Run from monorepo root (iac_dart/).');
    exit(1);
  }

  final webDir = Directory('$root/build/web');
  final hasWeb = webDir.existsSync();

  final forwarded = <String>[
    ...args,
    if (!args.any((a) => a.startsWith('--web-dir') || a == '-w') && hasWeb)
      ...['--web-dir', webDir.path],
  ];

  stdout.writeln('Starting IAC server…');
  if (hasWeb) {
    stdout.writeln('Serving UI from ${webDir.path}');
  } else {
    stdout.writeln(
      'No build/web found. Run: flutter build web --wasm --release',
    );
  }

  final result = await Process.start(
    'dart',
    ['run', 'bin/server.dart', ...forwarded],
    workingDirectory: serverDir.path,
    mode: ProcessStartMode.inheritStdio,
  );
  exit(await result.exitCode);
}
