const std = @import("std");
const shared = @import("shared");
const zqlite = @import("zqlite");
const engine_mod = @import("engine.zig");

const Hex = shared.Hex;
const Resources = shared.constants.Resources;
const ShipClass = shared.constants.ShipClass;

const log = std.log.scoped(.database);

pub const Database = struct {
    allocator: std.mem.Allocator,
    db: zqlite.Database,

    pub fn init(allocator: std.mem.Allocator, db_path: []const u8) !Database {
        var db = Database{
            .allocator = allocator,
            .db = try zqlite.Database.open(allocator, db_path),
        };

        try db.ensureSchema();
        try db.applyPragmas();
        log.info("Database initialized: {s}", .{db_path});
        return db;
    }

    pub fn deinit(self: *Database) void {
        self.db.close();
    }

    fn ensureSchema(self: *Database) !void {
        try self.db.exec(
            \\CREATE TABLE IF NOT EXISTS server_state (
            \\    key TEXT PRIMARY KEY,
            \\    value TEXT NOT NULL
            \\);
        );

        try self.db.exec(
            \\CREATE TABLE IF NOT EXISTS players (
            \\    id INTEGER PRIMARY KEY,
            \\    name TEXT UNIQUE NOT NULL,
            \\    homeworld_q INTEGER NOT NULL,
            \\    homeworld_r INTEGER NOT NULL,
            \\    metal REAL DEFAULT 500,
            \\    crystal REAL DEFAULT 300,
            \\    deuterium REAL DEFAULT 100
            \\);
        );

        try self.db.exec(
            \\CREATE TABLE IF NOT EXISTS fleets (
            \\    id INTEGER PRIMARY KEY,
            \\    player_id INTEGER REFERENCES players(id),
            \\    q INTEGER NOT NULL,
            \\    r INTEGER NOT NULL,
            \\    state TEXT DEFAULT 'idle',
            \\    fuel REAL NOT NULL,
            \\    fuel_max REAL NOT NULL,
            \\    cargo_metal REAL DEFAULT 0,
            \\    cargo_crystal REAL DEFAULT 0,
            \\    cargo_deuterium REAL DEFAULT 0
            \\);
        );

        try self.db.exec(
            \\CREATE TABLE IF NOT EXISTS ships (
            \\    id INTEGER PRIMARY KEY,
            \\    fleet_id INTEGER REFERENCES fleets(id),
            \\    player_id INTEGER REFERENCES players(id),
            \\    class TEXT NOT NULL,
            \\    hull REAL NOT NULL,
            \\    hull_max REAL NOT NULL,
            \\    shield REAL NOT NULL,
            \\    shield_max REAL NOT NULL,
            \\    weapon_power REAL NOT NULL,
            \\    speed INTEGER NOT NULL
            \\);
        );

        try self.db.exec(
            \\CREATE TABLE IF NOT EXISTS sectors_modified (
            \\    q INTEGER,
            \\    r INTEGER,
            \\    metal_density INTEGER,
            \\    crystal_density INTEGER,
            \\    deut_density INTEGER,
            \\    PRIMARY KEY (q, r)
            \\);
        );

        try self.db.exec(
            \\CREATE TABLE IF NOT EXISTS explored_edges (
            \\    player_id INTEGER REFERENCES players(id),
            \\    q1 INTEGER NOT NULL,
            \\    r1 INTEGER NOT NULL,
            \\    q2 INTEGER NOT NULL,
            \\    r2 INTEGER NOT NULL,
            \\    discovered_tick INTEGER NOT NULL,
            \\    PRIMARY KEY (player_id, q1, r1, q2, r2)
            \\);
        );

        try self.db.exec("CREATE INDEX IF NOT EXISTS idx_fleets_player ON fleets(player_id)");
        try self.db.exec("CREATE INDEX IF NOT EXISTS idx_fleets_location ON fleets(q, r)");
        try self.db.exec("CREATE INDEX IF NOT EXISTS idx_ships_fleet ON ships(fleet_id)");
        try self.db.exec("CREATE INDEX IF NOT EXISTS idx_explored_player ON explored_edges(player_id)");

        // Migration: add harvest accumulator columns
        self.db.exec("ALTER TABLE sectors_modified ADD COLUMN metal_harvested REAL DEFAULT 0") catch {};
        self.db.exec("ALTER TABLE sectors_modified ADD COLUMN crystal_harvested REAL DEFAULT 0") catch {};
        self.db.exec("ALTER TABLE sectors_modified ADD COLUMN deut_harvested REAL DEFAULT 0") catch {};

        // Migration: add NPC cleared tick
        self.db.exec("ALTER TABLE sectors_modified ADD COLUMN npc_cleared_tick INTEGER") catch {};

        log.info("Schema verified", .{});
    }

    fn applyPragmas(self: *Database) !void {
        try self.db.exec("PRAGMA cache_size = -8000");
        try self.db.exec("PRAGMA wal_autocheckpoint = 1000");
        log.info("Pragmas applied: cache_size=8MB, wal_autocheckpoint=1000", .{});
    }

    pub fn saveServerState(self: *Database, key: []const u8, value: []const u8) !void {
        var stmt = try self.db.prepare(
            "INSERT OR REPLACE INTO server_state (key, value) VALUES (?1, ?2)",
        );
        defer stmt.deinit();
        try stmt.bindText(1, key);
        try stmt.bindText(2, value);
        _ = try stmt.step();
    }

    pub fn loadServerState(self: *Database, key: []const u8) !?[]const u8 {
        var stmt = try self.db.prepare(
            "SELECT value FROM server_state WHERE key = ?1",
        );
        defer stmt.deinit();
        try stmt.bindText(1, key);
        if (try stmt.step()) {
            const val = stmt.columnText(0) orelse return null;
            return try self.allocator.dupe(u8, val);
        }
        return null;
    }

    pub fn savePlayer(self: *Database, player: engine_mod.Player) !void {
        var stmt = try self.db.prepare(
            \\INSERT OR REPLACE INTO players
            \\    (id, name, homeworld_q, homeworld_r, metal, crystal, deuterium)
            \\VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        );
        defer stmt.deinit();
        try stmt.bindInt(1, @intCast(player.id));
        try stmt.bindText(2, player.name);
        try stmt.bindInt(3, @as(i64, player.homeworld.q));
        try stmt.bindInt(4, @as(i64, player.homeworld.r));
        try stmt.bindInt(5, floatToStoredInt(player.resources.metal));
        try stmt.bindInt(6, floatToStoredInt(player.resources.crystal));
        try stmt.bindInt(7, floatToStoredInt(player.resources.deuterium));
        _ = try stmt.step();
    }

    pub fn loadPlayers(self: *Database) !std.ArrayList(engine_mod.Player) {
        var players = std.ArrayList(engine_mod.Player).empty;
        var stmt = try self.db.prepare(
            "SELECT id, name, homeworld_q, homeworld_r, metal, crystal, deuterium FROM players",
        );
        defer stmt.deinit();
        while (try stmt.step()) {
            const name_raw = stmt.columnText(1) orelse continue;
            try players.append(self.allocator, .{
                .id = @intCast(stmt.columnInt(0)),
                .name = try self.allocator.dupe(u8, name_raw),
                .homeworld = .{
                    .q = @intCast(stmt.columnInt32(2)),
                    .r = @intCast(stmt.columnInt32(3)),
                },
                .resources = .{
                    .metal = storedIntToFloat(stmt.columnInt(4)),
                    .crystal = storedIntToFloat(stmt.columnInt(5)),
                    .deuterium = storedIntToFloat(stmt.columnInt(6)),
                },
            });
        }
        return players;
    }

    pub fn saveFleet(self: *Database, fleet: engine_mod.Fleet) !void {
        var stmt = try self.db.prepare(
            \\INSERT OR REPLACE INTO fleets
            \\    (id, player_id, q, r, state, fuel, fuel_max,
            \\     cargo_metal, cargo_crystal, cargo_deuterium)
            \\VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        );
        defer stmt.deinit();
        try stmt.bindInt(1, @intCast(fleet.id));
        try stmt.bindInt(2, @intCast(fleet.owner_id));
        try stmt.bindInt(3, @as(i64, fleet.location.q));
        try stmt.bindInt(4, @as(i64, fleet.location.r));
        try stmt.bindText(5, @tagName(fleet.state));
        try stmt.bindInt(6, floatToStoredInt(fleet.fuel));
        try stmt.bindInt(7, floatToStoredInt(fleet.fuel_max));
        try stmt.bindInt(8, floatToStoredInt(fleet.cargo.metal));
        try stmt.bindInt(9, floatToStoredInt(fleet.cargo.crystal));
        try stmt.bindInt(10, floatToStoredInt(fleet.cargo.deuterium));
        _ = try stmt.step();

        try self.saveShips(fleet.id, fleet.owner_id, fleet.ships[0..fleet.ship_count]);
    }

    fn saveShips(self: *Database, fleet_id: u64, player_id: u64, ships: []const engine_mod.Ship) !void {
        var del = try self.db.prepare("DELETE FROM ships WHERE fleet_id = ?1");
        defer del.deinit();
        try del.bindInt(1, @intCast(fleet_id));
        _ = try del.step();

        var ins = try self.db.prepare(
            \\INSERT INTO ships
            \\    (id, fleet_id, player_id, class, hull, hull_max, shield, shield_max, weapon_power, speed)
            \\VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        );
        defer ins.deinit();

        for (ships) |ship| {
            try ins.bindInt(1, @intCast(ship.id));
            try ins.bindInt(2, @intCast(fleet_id));
            try ins.bindInt(3, @intCast(player_id));
            try ins.bindText(4, @tagName(ship.class));
            try ins.bindInt(5, floatToStoredInt(ship.hull));
            try ins.bindInt(6, floatToStoredInt(ship.hull_max));
            try ins.bindInt(7, floatToStoredInt(ship.shield));
            try ins.bindInt(8, floatToStoredInt(ship.shield_max));
            try ins.bindInt(9, floatToStoredInt(ship.weapon_power));
            try ins.bindInt(10, @as(i64, ship.speed));
            _ = try ins.step();
            ins.reset();
        }
    }

    pub fn loadFleets(self: *Database) !std.ArrayList(engine_mod.Fleet) {
        var fleets = std.ArrayList(engine_mod.Fleet).empty;

        var stmt = try self.db.prepare(
            \\SELECT id, player_id, q, r, state, fuel, fuel_max,
            \\       cargo_metal, cargo_crystal, cargo_deuterium
            \\FROM fleets
        );
        defer stmt.deinit();

        while (try stmt.step()) {
            const fleet_id: u64 = @intCast(stmt.columnInt(0));
            const state_str = stmt.columnText(4) orelse "idle";

            var fleet = engine_mod.Fleet{
                .id = fleet_id,
                .owner_id = @intCast(stmt.columnInt(1)),
                .location = .{
                    .q = @intCast(stmt.columnInt32(2)),
                    .r = @intCast(stmt.columnInt32(3)),
                },
                .state = parseFleetStatus(state_str),
                .ships = undefined,
                .ship_count = 0,
                .cargo = .{
                    .metal = storedIntToFloat(stmt.columnInt(7)),
                    .crystal = storedIntToFloat(stmt.columnInt(8)),
                    .deuterium = storedIntToFloat(stmt.columnInt(9)),
                },
                .fuel = storedIntToFloat(stmt.columnInt(5)),
                .fuel_max = storedIntToFloat(stmt.columnInt(6)),
                .move_cooldown = 0,
                .action_cooldown = 0,
                .move_target = null,
                .idle_ticks = 0,
            };

            fleet.ship_count = try self.loadShipsInto(fleet_id, &fleet.ships);
            try fleets.append(self.allocator, fleet);
        }

        return fleets;
    }

    fn loadShipsInto(self: *Database, fleet_id: u64, ships: *[engine_mod.MAX_SHIPS_PER_FLEET]engine_mod.Ship) !usize {
        var ship_stmt = try self.db.prepare(
            \\SELECT id, class, hull, hull_max, shield, shield_max, weapon_power, speed
            \\FROM ships WHERE fleet_id = ?1
        );
        defer ship_stmt.deinit();
        try ship_stmt.bindInt(1, @intCast(fleet_id));

        var count: usize = 0;
        while (try ship_stmt.step()) {
            if (count >= engine_mod.MAX_SHIPS_PER_FLEET) break;
            const class_str = ship_stmt.columnText(1) orelse "scout";
            ships[count] = .{
                .id = @intCast(ship_stmt.columnInt(0)),
                .class = parseShipClass(class_str),
                .hull = storedIntToFloat(ship_stmt.columnInt(2)),
                .hull_max = storedIntToFloat(ship_stmt.columnInt(3)),
                .shield = storedIntToFloat(ship_stmt.columnInt(4)),
                .shield_max = storedIntToFloat(ship_stmt.columnInt(5)),
                .weapon_power = storedIntToFloat(ship_stmt.columnInt(6)),
                .speed = @intCast(ship_stmt.columnInt(7)),
            };
            count += 1;
        }
        return count;
    }

    pub fn saveSectorOverride(self: *Database, q: i16, r: i16, ov: engine_mod.SectorOverride) !void {
        var stmt = try self.db.prepare(
            \\INSERT OR REPLACE INTO sectors_modified
            \\    (q, r, metal_density, crystal_density, deut_density,
            \\     metal_harvested, crystal_harvested, deut_harvested, npc_cleared_tick)
            \\VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        );
        defer stmt.deinit();
        try stmt.bindInt(1, @as(i64, q));
        try stmt.bindInt(2, @as(i64, r));
        try stmt.bindOptionalInt(3, densityToInt(ov.metal_density));
        try stmt.bindOptionalInt(4, densityToInt(ov.crystal_density));
        try stmt.bindOptionalInt(5, densityToInt(ov.deut_density));
        try stmt.bindInt(6, floatToStoredInt(ov.metal_harvested));
        try stmt.bindInt(7, floatToStoredInt(ov.crystal_harvested));
        try stmt.bindInt(8, floatToStoredInt(ov.deut_harvested));
        try stmt.bindOptionalInt(9, if (ov.npc_cleared_tick) |t| @as(i64, @intCast(t)) else null);
        _ = try stmt.step();
    }

    pub fn loadSectorOverrides(self: *Database) !std.ArrayList(SectorOverrideRow) {
        var overrides = std.ArrayList(SectorOverrideRow).empty;
        var stmt = try self.db.prepare(
            \\SELECT q, r, metal_density, crystal_density, deut_density,
            \\       metal_harvested, crystal_harvested, deut_harvested,
            \\       npc_cleared_tick
            \\FROM sectors_modified
        );
        defer stmt.deinit();
        while (try stmt.step()) {
            const cleared_tick_raw = stmt.columnOptionalInt(8);
            try overrides.append(self.allocator, .{
                .q = @intCast(stmt.columnInt32(0)),
                .r = @intCast(stmt.columnInt32(1)),
                .override = .{
                    .metal_density = intToDensity(stmt.columnOptionalInt(2)),
                    .crystal_density = intToDensity(stmt.columnOptionalInt(3)),
                    .deut_density = intToDensity(stmt.columnOptionalInt(4)),
                    .metal_harvested = storedIntToFloat(stmt.columnInt(5)),
                    .crystal_harvested = storedIntToFloat(stmt.columnInt(6)),
                    .deut_harvested = storedIntToFloat(stmt.columnInt(7)),
                    .npc_cleared_tick = if (cleared_tick_raw) |t| @as(u64, @intCast(t)) else null,
                },
            });
        }
        return overrides;
    }

    pub fn saveExploredEdge(self: *Database, player_id: u64, from: Hex, to: Hex, tick: u64) !void {
        var stmt = try self.db.prepare(
            \\INSERT OR IGNORE INTO explored_edges
            \\    (player_id, q1, r1, q2, r2, discovered_tick)
            \\VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        );
        defer stmt.deinit();
        try stmt.bindInt(1, @intCast(player_id));
        try stmt.bindInt(2, @as(i64, from.q));
        try stmt.bindInt(3, @as(i64, from.r));
        try stmt.bindInt(4, @as(i64, to.q));
        try stmt.bindInt(5, @as(i64, to.r));
        try stmt.bindInt(6, @intCast(tick));
        _ = try stmt.step();
    }

    pub fn loadExploredEdges(self: *Database, player_id: u64) !std.ArrayList(ExploredEdgeRow) {
        var edges = std.ArrayList(ExploredEdgeRow).empty;
        var stmt = try self.db.prepare(
            "SELECT q1, r1, q2, r2, discovered_tick FROM explored_edges WHERE player_id = ?1",
        );
        defer stmt.deinit();
        try stmt.bindInt(1, @intCast(player_id));
        while (try stmt.step()) {
            try edges.append(self.allocator, .{
                .from = .{
                    .q = @intCast(stmt.columnInt32(0)),
                    .r = @intCast(stmt.columnInt32(1)),
                },
                .to = .{
                    .q = @intCast(stmt.columnInt32(2)),
                    .r = @intCast(stmt.columnInt32(3)),
                },
                .discovered_tick = @intCast(stmt.columnInt(4)),
            });
        }
        return edges;
    }
};

pub const SectorOverrideRow = struct {
    q: i16,
    r: i16,
    override: engine_mod.SectorOverride,
};

pub const ExploredEdgeRow = struct {
    from: Hex,
    to: Hex,
    discovered_tick: u64,
};

fn floatToStoredInt(val: f32) i64 {
    return @intFromFloat(val * 1000.0);
}

fn storedIntToFloat(val: i64) f32 {
    return @as(f32, @floatFromInt(val)) / 1000.0;
}

fn densityToInt(d: ?shared.constants.Density) ?i64 {
    if (d) |density| return @as(i64, @intFromEnum(density));
    return null;
}

fn intToDensity(v: ?i64) ?shared.constants.Density {
    if (v) |val| return @enumFromInt(@as(u8, @intCast(val)));
    return null;
}

fn parseFleetStatus(s: []const u8) engine_mod.FleetStatus {
    return std.meta.stringToEnum(engine_mod.FleetStatus, s) orelse .idle;
}

fn parseShipClass(s: []const u8) ShipClass {
    return std.meta.stringToEnum(ShipClass, s) orelse .scout;
}
