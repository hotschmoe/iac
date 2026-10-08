import 'dart:convert';
import 'dart:io';

/// Repo-level golden fixtures written by shared/tests/golden_fixtures.rs.
/// `flutter test` runs with the package root (clients/web) as cwd.
Directory fixturesDir() {
  var dir = Directory.current;
  for (var i = 0; i < 4; i++) {
    final candidate = Directory('${dir.path}/fixtures');
    if (candidate.existsSync()) return candidate;
    dir = dir.parent;
  }
  throw StateError('fixtures/ not found from ${Directory.current.path}');
}

/// All *.json files directly under fixtures/<sub>, sorted by name.
List<File> fixtureFiles(String sub) {
  final files = Directory('${fixturesDir().path}/$sub')
      .listSync()
      .whereType<File>()
      .where((f) => f.path.endsWith('.json'))
      .toList()
    ..sort((a, b) => a.path.compareTo(b.path));
  if (files.isEmpty) throw StateError('no fixtures in $sub');
  return files;
}

String baseName(File f) => f.uri.pathSegments.last.replaceAll('.json', '');

dynamic readJson(File f) => jsonDecode(f.readAsStringSync());
