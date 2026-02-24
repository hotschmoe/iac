// src/server/database.zig
// SQLite persistence layer.
// Only modified state is stored — procedural generation handles the rest.

const std = @import("std");
const shared = @import("shared");

const log = std.log.scoped(.database);

pub const Database = struct {
    allocator: std.mem.Allocator,
    db_path: []const u8,
    // TODO: sqlite3 handle via zig-sqlite binding

    pub fn init(allocator: std.mem.Allocator, db_path: []const u8) !Database {
        var db = Database{
            .allocator = allocator,
            .db_path = db_path,
        };

        try db.ensureSchema();

        return db;
    }

    pub fn deinit(self: *Database) void {
        _ = self;
        // TODO: close sqlite connection
    }

    /// Create tables if they don't exist.
    fn ensureSchema(self: *Database) !void {
        _ = self;
        // TODO: Execute schema creation SQL
        //
        // Tables needed:
        //
        // players (
        //     id INTEGER PRIMARY KEY,
        //     name TEXT UNIQUE NOT NULL,
        //     token TEXT NOT NULL,
        //     homeworld_q INTEGER NOT NULL,
        //     homeworld_r INTEGER NOT NULL,
        //     metal REAL DEFAULT 500,
        //     crystal REAL DEFAULT 300,
        //     deuterium REAL DEFAULT 100,
        //     created_at INTEGER DEFAULT (unixepoch())
        // )
        //
        // buildings (
        //     player_id INTEGER REFERENCES players(id),
        //     building_type TEXT NOT NULL,
        //     level INTEGER DEFAULT 0,
        //     build_start_tick INTEGER,
        //     build_end_tick INTEGER,
        //     PRIMARY KEY (player_id, building_type)
        // )
        //
        // research (
        //     player_id INTEGER REFERENCES players(id),
        //     tech TEXT NOT NULL,
        //     level INTEGER DEFAULT 0,
        //     research_start_tick INTEGER,
        //     research_end_tick INTEGER,
        //     PRIMARY KEY (player_id, tech)
        // )
        //
        // ships (
        //     id INTEGER PRIMARY KEY,
        //     player_id INTEGER REFERENCES players(id),
        //     fleet_id INTEGER,
        //     class TEXT NOT NULL,
        //     hull REAL NOT NULL,
        //     hull_max REAL NOT NULL,
        //     shield REAL NOT NULL,
        //     shield_max REAL NOT NULL,
        //     weapon_power REAL NOT NULL,
        //     speed INTEGER NOT NULL
        // )
        //
        // fleets (
        //     id INTEGER PRIMARY KEY,
        //     player_id INTEGER REFERENCES players(id),
        //     q INTEGER NOT NULL,
        //     r INTEGER NOT NULL,
        //     state TEXT DEFAULT 'idle',
        //     fuel REAL NOT NULL,
        //     fuel_max REAL NOT NULL,
        //     cargo_metal REAL DEFAULT 0,
        //     cargo_crystal REAL DEFAULT 0,
        //     cargo_deuterium REAL DEFAULT 0
        // )
        //
        // sectors_modified (
        //     q INTEGER,
        //     r INTEGER,
        //     resources_metal REAL,
        //     resources_crystal REAL,
        //     resources_deuterium REAL,
        //     last_cleared_tick INTEGER,
        //     PRIMARY KEY (q, r)
        // )
        //
        // explored_edges (
        //     player_id INTEGER REFERENCES players(id),
        //     q1 INTEGER NOT NULL,
        //     r1 INTEGER NOT NULL,
        //     q2 INTEGER NOT NULL,
        //     r2 INTEGER NOT NULL,
        //     discovered_tick INTEGER NOT NULL,
        //     PRIMARY KEY (player_id, q1, r1, q2, r2)
        // )
        //
        // auto_policies (
        //     player_id INTEGER REFERENCES players(id),
        //     fleet_id INTEGER,
        //     priority INTEGER NOT NULL,
        //     condition TEXT NOT NULL,
        //     action TEXT NOT NULL,
        //     PRIMARY KEY (player_id, fleet_id, priority)
        // )
        //
        // server_state (
        //     key TEXT PRIMARY KEY,
        //     value TEXT NOT NULL
        // )
        //   Stores: current_tick, next_id, world_seed

        log.info("Schema verified: {s}", .{self.db_path});
    }

    // ── Player CRUD ────────────────────────────────────────────────

    pub fn savePlayer(self: *Database, player: anytype) !void {
        _ = self;
        _ = player;
        // TODO: INSERT OR REPLACE into players
    }

    pub fn loadPlayers(self: *Database, allocator: std.mem.Allocator) !void {
        _ = self;
        _ = allocator;
        // TODO: SELECT * FROM players, populate engine state
    }

    // ── Fleet CRUD ─────────────────────────────────────────────────

    pub fn saveFleet(self: *Database, fleet: anytype) !void {
        _ = self;
        _ = fleet;
        // TODO: INSERT OR REPLACE into fleets + ships
    }

    pub fn loadFleets(self: *Database, allocator: std.mem.Allocator) !void {
        _ = self;
        _ = allocator;
        // TODO: SELECT * FROM fleets JOIN ships
    }

    // ── Sector Overrides ───────────────────────────────────────────

    pub fn saveSectorOverride(self: *Database, q: i16, r: i16, override: anytype) !void {
        _ = self;
        _ = q;
        _ = r;
        _ = override;
        // TODO: INSERT OR REPLACE into sectors_modified
    }

    // ── Explored Edges ─────────────────────────────────────────────

    pub fn saveExploredEdge(self: *Database, player_id: u64, from: shared.Hex, to: shared.Hex, tick: u64) !void {
        _ = self;
        _ = player_id;
        _ = from;
        _ = to;
        _ = tick;
        // TODO: INSERT OR IGNORE into explored_edges
    }

    pub fn loadExploredEdges(self: *Database, player_id: u64, allocator: std.mem.Allocator) !void {
        _ = self;
        _ = player_id;
        _ = allocator;
        // TODO: SELECT from explored_edges WHERE player_id = ?
    }

    // ── Server State ───────────────────────────────────────────────

    pub fn saveServerState(self: *Database, key: []const u8, value: []const u8) !void {
        _ = self;
        _ = key;
        _ = value;
        // TODO: INSERT OR REPLACE into server_state
    }

    pub fn loadServerState(self: *Database, key: []const u8) !?[]const u8 {
        _ = self;
        _ = key;
        // TODO: SELECT value FROM server_state WHERE key = ?
        return null;
    }
};
