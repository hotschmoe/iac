/// SQLite persistence for the IAC game server (lite port of Zig database.zig).
library;

import 'dart:io';

import 'package:iac_shared/iac_shared.dart';
import 'package:sqlite3/sqlite3.dart';

import 'entities.dart';

/// Lightweight SQLite layer: players, fleets, ships, buildings, research, meta.
class GameDatabase {
  final Database db;
  final String path;

  GameDatabase._(this.db, this.path);

  /// Open (or create) a database file. Use `:memory:` for tests.
  factory GameDatabase.open(String path) {
    final db = path == ':memory:'
        ? sqlite3.openInMemory()
        : sqlite3.open(path);
    final g = GameDatabase._(db, path);
    g._ensureSchema();
    g._applyPragmas();
    return g;
  }

  void close() => db.close();

  void _applyPragmas() {
    db.execute('PRAGMA journal_mode = WAL');
    db.execute('PRAGMA synchronous = NORMAL');
    db.execute('PRAGMA cache_size = -8000');
  }

  void _ensureSchema() {
    db.execute('''
CREATE TABLE IF NOT EXISTS server_state (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
''');
    db.execute('''
CREATE TABLE IF NOT EXISTS players (
  id INTEGER PRIMARY KEY,
  name TEXT UNIQUE NOT NULL,
  homeworld_q INTEGER NOT NULL,
  homeworld_r INTEGER NOT NULL,
  metal REAL DEFAULT 500,
  crystal REAL DEFAULT 300,
  deuterium REAL DEFAULT 100,
  fleet_name_index INTEGER DEFAULT 0
);
''');
    db.execute('''
CREATE TABLE IF NOT EXISTS fleets (
  id INTEGER PRIMARY KEY,
  player_id INTEGER NOT NULL,
  name TEXT NOT NULL,
  q INTEGER NOT NULL,
  r INTEGER NOT NULL,
  state TEXT DEFAULT 'idle',
  fuel REAL NOT NULL,
  fuel_max REAL NOT NULL,
  cargo_metal REAL DEFAULT 0,
  cargo_crystal REAL DEFAULT 0,
  cargo_deuterium REAL DEFAULT 0
);
''');
    db.execute('''
CREATE TABLE IF NOT EXISTS ships (
  id INTEGER PRIMARY KEY,
  fleet_id INTEGER NOT NULL,
  player_id INTEGER NOT NULL,
  class TEXT NOT NULL,
  hull REAL NOT NULL,
  hull_max REAL NOT NULL,
  shield REAL NOT NULL,
  shield_max REAL NOT NULL,
  weapon_power REAL NOT NULL,
  speed INTEGER NOT NULL
);
''');
    db.execute('''
CREATE TABLE IF NOT EXISTS buildings (
  player_id INTEGER NOT NULL,
  building_type TEXT NOT NULL,
  level INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (player_id, building_type)
);
''');
    db.execute('''
CREATE TABLE IF NOT EXISTS research (
  player_id INTEGER NOT NULL,
  tech_type TEXT NOT NULL,
  level INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (player_id, tech_type)
);
''');
    db.execute('''
CREATE TABLE IF NOT EXISTS explored (
  player_id INTEGER NOT NULL,
  q INTEGER NOT NULL,
  r INTEGER NOT NULL,
  PRIMARY KEY (player_id, q, r)
);
''');
  }

  // ── Meta ────────────────────────────────────────────────────────

  void saveMeta(String key, String value) {
    db.execute(
      'INSERT OR REPLACE INTO server_state (key, value) VALUES (?, ?)',
      [key, value],
    );
  }

  String? loadMeta(String key) {
    final rows = db.select(
      'SELECT value FROM server_state WHERE key = ?',
      [key],
    );
    if (rows.isEmpty) return null;
    return rows.first['value'] as String;
  }

  int? loadTick() {
    final v = loadMeta('tick');
    return v == null ? null : int.tryParse(v);
  }

  int? loadNextId() {
    final v = loadMeta('next_id');
    return v == null ? null : int.tryParse(v);
  }

  // ── Players ─────────────────────────────────────────────────────

  void savePlayer(Player player) {
    db.execute(
      '''
INSERT OR REPLACE INTO players
  (id, name, homeworld_q, homeworld_r, metal, crystal, deuterium, fleet_name_index)
VALUES (?, ?, ?, ?, ?, ?, ?, ?)
''',
      [
        player.id,
        player.name,
        player.homeworld.q,
        player.homeworld.r,
        player.resources.metal,
        player.resources.crystal,
        player.resources.deuterium,
        player.fleetNameIndex,
      ],
    );

    for (final bt in BuildingType.values) {
      final level = player.buildings.get(bt);
      db.execute(
        '''
INSERT OR REPLACE INTO buildings (player_id, building_type, level)
VALUES (?, ?, ?)
''',
        [player.id, bt.wireName, level],
      );
    }
    for (final rt in ResearchType.values) {
      final level = player.research.get(rt);
      if (level <= 0) continue;
      db.execute(
        '''
INSERT OR REPLACE INTO research (player_id, tech_type, level)
VALUES (?, ?, ?)
''',
        [player.id, rt.wireName, level],
      );
    }
  }

  List<Player> loadPlayers() {
    final rows = db.select(
      'SELECT id, name, homeworld_q, homeworld_r, metal, crystal, deuterium, fleet_name_index FROM players',
    );
    final players = <Player>[];
    for (final row in rows) {
      final id = row['id'] as int;
      final player = Player(
        id: id,
        name: row['name'] as String,
        resources: ResourceAmounts(
          metal: (row['metal'] as num).toDouble(),
          crystal: (row['crystal'] as num).toDouble(),
          deuterium: (row['deuterium'] as num).toDouble(),
        ),
        homeworld: Hex(row['homeworld_q'] as int, row['homeworld_r'] as int),
      );
      player.fleetNameIndex = (row['fleet_name_index'] as int?) ?? 0;

      final bRows = db.select(
        'SELECT building_type, level FROM buildings WHERE player_id = ?',
        [id],
      );
      for (final b in bRows) {
        final bt = BuildingType.fromWire(b['building_type'] as String?);
        player.buildings.set(bt, b['level'] as int);
      }

      final rRows = db.select(
        'SELECT tech_type, level FROM research WHERE player_id = ?',
        [id],
      );
      for (final r in rRows) {
        final rt = ResearchType.fromWire(r['tech_type'] as String?);
        player.research.set(rt, r['level'] as int);
      }

      players.add(player);
    }
    return players;
  }

  // ── Fleets / ships ──────────────────────────────────────────────

  void saveFleet(Fleet fleet) {
    db.execute(
      '''
INSERT OR REPLACE INTO fleets
  (id, player_id, name, q, r, state, fuel, fuel_max,
   cargo_metal, cargo_crystal, cargo_deuterium)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
''',
      [
        fleet.id,
        fleet.ownerId,
        fleet.name,
        fleet.location.q,
        fleet.location.r,
        fleet.status.wireName,
        fleet.fuel,
        fleet.fuelMax,
        fleet.cargo.metal,
        fleet.cargo.crystal,
        fleet.cargo.deuterium,
      ],
    );

    db.execute('DELETE FROM ships WHERE fleet_id = ?', [fleet.id]);
    for (final ship in fleet.ships) {
      db.execute(
        '''
INSERT INTO ships
  (id, fleet_id, player_id, class, hull, hull_max, shield, shield_max, weapon_power, speed)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
''',
        [
          ship.id,
          fleet.id,
          fleet.ownerId,
          ship.class_.wireName,
          ship.hull,
          ship.hullMax,
          ship.shield,
          ship.shieldMax,
          ship.weaponPower,
          ship.speed,
        ],
      );
    }
  }

  void deleteFleet(int fleetId) {
    db.execute('DELETE FROM ships WHERE fleet_id = ?', [fleetId]);
    db.execute('DELETE FROM fleets WHERE id = ?', [fleetId]);
  }

  List<Fleet> loadFleets() {
    final rows = db.select(
      '''
SELECT id, player_id, name, q, r, state, fuel, fuel_max,
       cargo_metal, cargo_crystal, cargo_deuterium
FROM fleets
''',
    );
    final fleets = <Fleet>[];
    for (final row in rows) {
      final fleetId = row['id'] as int;
      final ships = <Ship>[];
      final sRows = db.select(
        '''
SELECT id, class, hull, hull_max, shield, shield_max, weapon_power, speed
FROM ships WHERE fleet_id = ?
''',
        [fleetId],
      );
      for (final s in sRows) {
        ships.add(Ship(
          id: s['id'] as int,
          class_: ShipClass.fromWire(s['class'] as String?),
          hull: (s['hull'] as num).toDouble(),
          hullMax: (s['hull_max'] as num).toDouble(),
          shield: (s['shield'] as num).toDouble(),
          shieldMax: (s['shield_max'] as num).toDouble(),
          weaponPower: (s['weapon_power'] as num).toDouble(),
          speed: s['speed'] as int,
        ));
      }

      fleets.add(Fleet(
        id: fleetId,
        ownerId: row['player_id'] as int,
        name: row['name'] as String? ?? 'Fleet',
        location: Hex(row['q'] as int, row['r'] as int),
        status: FleetStatus.fromWire(row['state'] as String?),
        ships: ships,
        cargo: ResourceAmounts(
          metal: (row['cargo_metal'] as num).toDouble(),
          crystal: (row['cargo_crystal'] as num).toDouble(),
          deuterium: (row['cargo_deuterium'] as num).toDouble(),
        ),
        fuel: (row['fuel'] as num).toDouble(),
        fuelMax: (row['fuel_max'] as num).toDouble(),
      ));
    }
    return fleets;
  }

  // ── Explored ────────────────────────────────────────────────────

  void saveExplored(int playerId, Hex hex) {
    db.execute(
      'INSERT OR IGNORE INTO explored (player_id, q, r) VALUES (?, ?, ?)',
      [playerId, hex.q, hex.r],
    );
  }

  Map<int, Set<int>> loadExplored() {
    final result = <int, Set<int>>{};
    final rows = db.select('SELECT player_id, q, r FROM explored');
    for (final row in rows) {
      final pid = row['player_id'] as int;
      final key = Hex(row['q'] as int, row['r'] as int).toKey();
      result.putIfAbsent(pid, () => {}).add(key);
    }
    return result;
  }

  /// Persist dirty entities in one transaction.
  void persistDirty({
    required int tick,
    required int nextId,
    required Iterable<Player> players,
    required Iterable<Fleet> fleets,
    required Iterable<int> deletedFleetIds,
    required Map<int, Set<int>> exploredSectors,
  }) {
    db.execute('BEGIN');
    try {
      saveMeta('tick', '$tick');
      saveMeta('next_id', '$nextId');
      for (final p in players) {
        savePlayer(p);
      }
      for (final f in fleets) {
        saveFleet(f);
      }
      for (final id in deletedFleetIds) {
        deleteFleet(id);
      }
      for (final entry in exploredSectors.entries) {
        for (final key in entry.value) {
          final h = Hex.fromKey(key);
          saveExplored(entry.key, h);
        }
      }
      db.execute('COMMIT');
    } catch (e) {
      db.execute('ROLLBACK');
      rethrow;
    }
  }

  /// File size if on disk.
  int? fileBytes() {
    if (path == ':memory:') return null;
    final f = File(path);
    if (!f.existsSync()) return null;
    return f.lengthSync();
  }
}
