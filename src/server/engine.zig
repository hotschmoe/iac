const std = @import("std");
const shared = @import("shared");
const combat = @import("combat.zig");
const Database = @import("database.zig").Database;

const Hex = shared.Hex;
const Resources = shared.constants.Resources;
const ShipClass = shared.constants.ShipClass;
const WorldGen = shared.world.WorldGen;

const log = std.log.scoped(.engine);

pub const GameEngine = struct {
    allocator: std.mem.Allocator,
    world_gen: WorldGen,
    db: *Database,
    current_tick: u64,

    players: std.AutoHashMap(u64, Player),
    fleets: std.AutoHashMap(u64, Fleet),
    npc_fleets: std.AutoHashMap(u64, NpcFleet),
    active_combats: std.AutoHashMap(u64, Combat),

    sector_overrides: std.AutoHashMap(u32, SectorOverride),

    pending_events: std.ArrayList(shared.protocol.GameEvent),

    next_id: u64,

    dirty_players: std.AutoHashMap(u64, void),
    dirty_fleets: std.AutoHashMap(u64, void),
    dirty_sectors: std.AutoHashMap(u32, void),

    pub fn init(allocator: std.mem.Allocator, world_seed: u64, db: *Database) !GameEngine {
        var engine = GameEngine{
            .allocator = allocator,
            .world_gen = WorldGen.init(world_seed),
            .db = db,
            .current_tick = 0,
            .players = std.AutoHashMap(u64, Player).init(allocator),
            .fleets = std.AutoHashMap(u64, Fleet).init(allocator),
            .npc_fleets = std.AutoHashMap(u64, NpcFleet).init(allocator),
            .active_combats = std.AutoHashMap(u64, Combat).init(allocator),
            .sector_overrides = std.AutoHashMap(u32, SectorOverride).init(allocator),
            .pending_events = .empty,
            .next_id = 1,
            .dirty_players = std.AutoHashMap(u64, void).init(allocator),
            .dirty_fleets = std.AutoHashMap(u64, void).init(allocator),
            .dirty_sectors = std.AutoHashMap(u32, void).init(allocator),
        };

        try engine.loadState();
        try engine.persistWorldSeed();

        return engine;
    }

    pub fn deinit(self: *GameEngine) void {
        var player_iter = self.players.iterator();
        while (player_iter.next()) |entry| {
            self.allocator.free(entry.value_ptr.name);
        }
        self.players.deinit();
        self.fleets.deinit();
        self.npc_fleets.deinit();
        self.active_combats.deinit();
        self.sector_overrides.deinit();
        self.pending_events.deinit(self.allocator);
        self.dirty_players.deinit();
        self.dirty_fleets.deinit();
        self.dirty_sectors.deinit();
    }

    pub fn currentTick(self: *const GameEngine) u64 {
        return self.current_tick;
    }

    pub fn nextId(self: *GameEngine) u64 {
        const id = self.next_id;
        self.next_id += 1;
        return id;
    }

    pub fn tick(self: *GameEngine) !void {
        self.current_tick += 1;
        self.pending_events.clearRetainingCapacity();

        try self.processMovement();
        try self.processCombat();
        try self.processHarvesting();
        try self.processNpcBehavior();
        try self.processHomeworlds();
        try self.processCooldowns();
    }

    fn processMovement(self: *GameEngine) !void {
        var iter = self.fleets.iterator();
        while (iter.next()) |entry| {
            var fleet = entry.value_ptr;
            if (fleet.state != .moving) continue;

            if (fleet.move_cooldown > 0) {
                fleet.move_cooldown -= 1;
                continue;
            }

            if (fleet.move_target) |target| {
                fleet.location = target;
                fleet.state = .idle;
                fleet.move_target = null;
                try self.dirty_fleets.put(fleet.id, {});

                try self.recordExplored(fleet.owner_id, fleet.location);
                try self.pending_events.append(self.allocator, .{
                    .tick = self.current_tick,
                    .kind = .{ .sector_entered = .{
                        .fleet_id = fleet.id,
                        .sector = target,
                        .first_visit = true, // TODO: check explored set
                    } },
                });

                try self.checkNpcEncounter(fleet);
            }
        }
    }

    fn processCombat(self: *GameEngine) !void {
        var to_remove: std.ArrayList(u64) = .empty;
        defer to_remove.deinit(self.allocator);

        var iter = self.active_combats.iterator();
        while (iter.next()) |entry| {
            const combat_id = entry.key_ptr.*;
            const active_combat = entry.value_ptr;

            const player_fleet = self.fleets.getPtr(active_combat.player_fleet_id) orelse {
                try to_remove.append(self.allocator, combat_id);
                continue;
            };
            const npc_fleet = self.npc_fleets.getPtr(active_combat.npc_fleet_id) orelse {
                try to_remove.append(self.allocator, combat_id);
                continue;
            };

            const result = try combat.resolveCombatRound(
                self.allocator,
                active_combat,
                player_fleet,
                npc_fleet,
                self.current_tick,
            );
            defer self.allocator.free(result.events);

            for (result.events) |event| {
                try self.pending_events.append(self.allocator, event);
            }

            try self.dirty_fleets.put(active_combat.player_fleet_id, {});

            if (result.concluded) {
                try to_remove.append(self.allocator, combat_id);

                player_fleet.state = if (player_fleet.ship_count == 0) .docked else .idle;
                player_fleet.idle_ticks = 0;

                try self.pending_events.append(self.allocator, .{
                    .tick = self.current_tick,
                    .kind = .{ .combat_ended = .{
                        .sector = active_combat.sector,
                        .player_victory = result.player_won,
                    } },
                });

                if (result.player_won) {
                    try self.dropSalvage(active_combat.sector, active_combat.npc_value);
                }

                _ = self.npc_fleets.remove(active_combat.npc_fleet_id);
            }
        }

        for (to_remove.items) |id| {
            _ = self.active_combats.remove(id);
        }
    }

    fn processHarvesting(self: *GameEngine) !void {
        var iter = self.fleets.iterator();
        while (iter.next()) |entry| {
            var fleet = entry.value_ptr;
            if (fleet.state != .harvesting) continue;

            const template = self.world_gen.generateSector(fleet.location);
            const override = self.sector_overrides.get(fleet.location.toKey());

            const metal_density = if (override) |ov| ov.metal_density orelse template.metal_density else template.metal_density;
            const crystal_density = if (override) |ov| ov.crystal_density orelse template.crystal_density else template.crystal_density;
            const deut_density = if (override) |ov| ov.deut_density orelse template.deut_density else template.deut_density;

            const harvest_power = fleetHarvestPower(fleet);
            const max_cargo = fleetCargoCapacity(fleet);
            var remaining = max_cargo - (fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium);

            if (remaining <= 0) {
                fleet.state = .idle;
                continue;
            }

            var harvested_any = false;

            const metal_amount = metal_density.harvestMultiplier() * harvest_power;
            if (metal_amount > 0 and remaining > 0) {
                const actual = @min(metal_amount, remaining);
                fleet.cargo.metal += actual;
                remaining -= actual;
                harvested_any = true;
                try self.pending_events.append(self.allocator, .{
                    .tick = self.current_tick,
                    .kind = .{ .resource_harvested = .{ .fleet_id = fleet.id, .resource_type = .metal, .amount = actual } },
                });
            }

            const crystal_amount = crystal_density.harvestMultiplier() * harvest_power;
            if (crystal_amount > 0 and remaining > 0) {
                const actual = @min(crystal_amount, remaining);
                fleet.cargo.crystal += actual;
                remaining -= actual;
                harvested_any = true;
                try self.pending_events.append(self.allocator, .{
                    .tick = self.current_tick,
                    .kind = .{ .resource_harvested = .{ .fleet_id = fleet.id, .resource_type = .crystal, .amount = actual } },
                });
            }

            const deut_amount = deut_density.harvestMultiplier() * harvest_power;
            if (deut_amount > 0 and remaining > 0) {
                const actual = @min(deut_amount, remaining);
                fleet.cargo.deuterium += actual;
                remaining -= actual;
                harvested_any = true;
                try self.pending_events.append(self.allocator, .{
                    .tick = self.current_tick,
                    .kind = .{ .resource_harvested = .{ .fleet_id = fleet.id, .resource_type = .deuterium, .amount = actual } },
                });
            }

            if (harvested_any) {
                try self.dirty_fleets.put(fleet.id, {});
            } else {
                fleet.state = .idle;
            }
        }
    }

    fn processNpcBehavior(self: *GameEngine) !void {
        // TODO: NPC movement, aggro detection, spawning
        _ = self;
    }

    fn processHomeworlds(self: *GameEngine) !void {
        var iter = self.players.iterator();
        while (iter.next()) |entry| {
            var player = entry.value_ptr;

            // TODO: calculate from building levels
            player.resources.metal += 0.5;
            player.resources.crystal += 0.3;
            try self.dirty_players.put(player.id, {});
        }
    }

    fn processCooldowns(self: *GameEngine) !void {
        var iter = self.fleets.iterator();
        while (iter.next()) |entry| {
            var fleet = entry.value_ptr;
            if (fleet.action_cooldown > 0) {
                fleet.action_cooldown -= 1;
            }
            if (fleet.state == .idle) {
                fleet.idle_ticks += 1;
            }
        }
    }

    pub fn registerPlayer(self: *GameEngine, name: []const u8) !u64 {
        // Reconnect if player already exists
        var iter = self.players.iterator();
        while (iter.next()) |entry| {
            if (std.mem.eql(u8, entry.value_ptr.name, name)) {
                log.info("Player '{s}' reconnected (id={d})", .{ name, entry.key_ptr.* });
                return entry.key_ptr.*;
            }
        }

        const player_id = self.nextId();
        const homeworld = self.findHomeworldLocation();

        const player = Player{
            .id = player_id,
            .name = try self.allocator.dupe(u8, name),
            .resources = shared.constants.STARTING_RESOURCES,
            .homeworld = homeworld,
        };

        try self.players.put(player_id, player);
        try self.db.savePlayer(player);

        const fleet_id = self.nextId();
        const ship_id = self.nextId();

        const scout_stats = ShipClass.scout.baseStats();
        var fleet = Fleet{
            .id = fleet_id,
            .owner_id = player_id,
            .location = homeworld,
            .state = .idle,
            .ships = undefined,
            .ship_count = 1,
            .cargo = .{},
            .fuel = 50000, // testing: high starting fuel
            .fuel_max = 50000,
            .move_cooldown = 0,
            .action_cooldown = 0,
            .move_target = null,
            .idle_ticks = 0,
        };

        fleet.ships[0] = Ship{
            .id = ship_id,
            .class = .scout,
            .hull = scout_stats.hull,
            .hull_max = scout_stats.hull,
            .shield = scout_stats.shield,
            .shield_max = scout_stats.shield,
            .weapon_power = scout_stats.weapon,
            .speed = scout_stats.speed,
        };

        try self.fleets.put(fleet_id, fleet);
        try self.dirty_players.put(player_id, {});
        try self.dirty_fleets.put(fleet_id, {});

        log.info("Player '{s}' registered (id={d}) at homeworld {any}", .{ name, player_id, homeworld });

        return player_id;
    }

    pub fn handleMove(self: *GameEngine, fleet_id: u64, target: Hex) !void {
        const fleet = self.fleets.getPtr(fleet_id) orelse return error.FleetNotFound;

        if (fleet.ship_count == 0) return error.NoShips;
        if (fleet.state == .in_combat) return error.InCombat;
        if (fleet.action_cooldown > 0) return error.OnCooldown;

        const connections = self.world_gen.connectedNeighbors(fleet.location);
        var valid = false;
        for (connections.slice()) |conn| {
            if (conn.eql(target)) {
                valid = true;
                break;
            }
        }
        if (!valid) return error.NoConnection;

        const fuel_cost = fleetFuelCost(fleet);
        if (fleet.fuel < fuel_cost) return error.InsufficientFuel;

        fleet.fuel -= fuel_cost;
        fleet.state = .moving;
        fleet.move_target = target;
        fleet.move_cooldown = fleetMoveCooldown(fleet);
        fleet.action_cooldown = fleet.move_cooldown;
        try self.dirty_fleets.put(fleet_id, {});
    }

    pub fn handleHarvest(self: *GameEngine, fleet_id: u64) !void {
        const fleet = self.fleets.getPtr(fleet_id) orelse return error.FleetNotFound;

        if (fleet.ship_count == 0) return error.NoShips;
        if (fleet.state == .in_combat) return error.InCombat;
        if (fleet.action_cooldown > 0) return error.OnCooldown;

        fleet.state = .harvesting;
        fleet.action_cooldown = shared.constants.HARVEST_COOLDOWN;
        try self.dirty_fleets.put(fleet_id, {});
    }

    pub fn handleRecall(self: *GameEngine, fleet_id: u64) !void {
        const fleet = self.fleets.getPtr(fleet_id) orelse return error.FleetNotFound;
        if (fleet.ship_count == 0) return error.NoShips;
        const player = self.players.getPtr(fleet.owner_id) orelse return error.PlayerNotFound;

        const dist = Hex.distance(fleet.location, player.homeworld);

        const fuel_cost = fleetFuelCost(fleet) * @as(f32, @floatFromInt(dist)) * shared.constants.RECALL_FUEL_MULTIPLIER;
        if (fleet.fuel < fuel_cost) return error.InsufficientFuel;

        fleet.fuel -= fuel_cost;

        const damage_chance = @min(
            shared.constants.RECALL_DAMAGE_CHANCE_CAP,
            shared.constants.RECALL_DAMAGE_CHANCE_PER_HEX * @as(f32, @floatFromInt(dist)),
        );

        var rng = std.Random.DefaultPrng.init(@as(u64, @truncate(@as(u128, @bitCast(std.time.nanoTimestamp())))));
        const random = rng.random();

        var i: usize = 0;
        while (i < fleet.ship_count) {
            const roll = random.float(f32);
            if (roll < damage_chance) {
                const damage_pct = shared.constants.RECALL_HULL_DAMAGE_MIN +
                    random.float(f32) * (shared.constants.RECALL_HULL_DAMAGE_MAX - shared.constants.RECALL_HULL_DAMAGE_MIN);
                fleet.ships[i].hull -= fleet.ships[i].hull_max * damage_pct;

                if (fleet.ships[i].hull <= 0) {
                    try self.pending_events.append(self.allocator, .{
                        .tick = self.current_tick,
                        .kind = .{ .ship_destroyed = .{
                            .ship_id = fleet.ships[i].id,
                            .ship_class = fleet.ships[i].class,
                            .owner_fleet_id = fleet.id,
                            .is_npc = false,
                        } },
                    });
                    fleet.ships[i] = fleet.ships[fleet.ship_count - 1];
                    fleet.ship_count -= 1;
                    continue;
                }
            }
            i += 1;
        }

        fleet.location = player.homeworld;
        fleet.state = if (fleet.ship_count == 0) .docked else .idle;
        fleet.move_target = null;
        try self.dirty_fleets.put(fleet.id, {});
    }

    fn findHomeworldLocation(self: *GameEngine) Hex {
        var rng = std.Random.DefaultPrng.init(@as(u64, @truncate(@as(u128, @bitCast(std.time.nanoTimestamp())))));
        const random = rng.random();

        const min = shared.constants.HOMEWORLD_MIN_DIST;
        const max = shared.constants.HOMEWORLD_MAX_DIST;

        for (0..100) |_| {
            const dist = random.intRangeAtMost(u16, min, max);
            const q: i16 = @intCast(random.intRangeAtMost(i32, -@as(i32, dist), @as(i32, dist)));
            const r_min: i16 = @intCast(@max(-@as(i32, dist), -@as(i32, q) - @as(i32, dist)));
            const r_max: i16 = @intCast(@min(@as(i32, dist), -@as(i32, q) + @as(i32, dist)));
            const r: i16 = @intCast(random.intRangeAtMost(i32, r_min, r_max));

            const candidate = Hex{ .q = q, .r = r };
            if (candidate.distFromOrigin() < min or candidate.distFromOrigin() > max) continue;

            var taken = false;
            var player_iter = self.players.iterator();
            while (player_iter.next()) |entry| {
                if (entry.value_ptr.homeworld.eql(candidate)) {
                    taken = true;
                    break;
                }
            }
            if (!taken) return candidate;
        }

        // Fallback
        return Hex{ .q = @intCast(min), .r = 0 };
    }

    fn checkNpcEncounter(self: *GameEngine, fleet: *Fleet) !void {
        const template = self.world_gen.generateSector(fleet.location);
        if (template.npc_template) |npc| {
            if (npc.behavior == .passive) return; // passive NPCs don't auto-aggro

            const combat_id = self.nextId();
            const npc_fleet_id = self.nextId();

            var npc_fleet = NpcFleet{
                .id = npc_fleet_id,
                .location = fleet.location,
                .ships = undefined,
                .ship_count = npc.count,
                .behavior = npc.behavior,
            };

            const stats = npc.ship_class.baseStats();
            var i: u8 = 0;
            while (i < @min(npc.count, 32)) : (i += 1) {
                npc_fleet.ships[i] = Ship{
                    .id = self.nextId(),
                    .class = npc.ship_class,
                    .hull = stats.hull,
                    .hull_max = stats.hull,
                    .shield = stats.shield,
                    .shield_max = stats.shield,
                    .weapon_power = stats.weapon,
                    .speed = stats.speed,
                };
            }

            try self.npc_fleets.put(npc_fleet_id, npc_fleet);
            try self.active_combats.put(combat_id, Combat{
                .id = combat_id,
                .sector = fleet.location,
                .player_fleet_id = fleet.id,
                .npc_fleet_id = npc_fleet_id,
                .npc_value = ShipClass.scout.buildCost(), // TODO: calculate actual value
                .round = 0,
            });

            fleet.state = .in_combat;

            try self.pending_events.append(self.allocator, .{
                .tick = self.current_tick,
                .kind = .{ .combat_started = .{
                    .player_fleet_id = fleet.id,
                    .enemy_fleet_id = npc_fleet_id,
                    .sector = fleet.location,
                } },
            });
        }
    }

    fn dropSalvage(self: *GameEngine, sector: Hex, fleet_value: Resources) !void {
        const salvage = Resources{
            .metal = fleet_value.metal * shared.constants.SALVAGE_FRACTION,
            .crystal = fleet_value.crystal * shared.constants.SALVAGE_FRACTION,
            .deuterium = fleet_value.deuterium * shared.constants.SALVAGE_FRACTION,
        };

        const key = sector.toKey();
        if (self.sector_overrides.getPtr(key)) |ov| {
            ov.salvage = salvage;
            ov.salvage_despawn_tick = self.current_tick + shared.constants.SALVAGE_DESPAWN_TICKS;
        } else {
            try self.sector_overrides.put(key, .{
                .salvage = salvage,
                .salvage_despawn_tick = self.current_tick + shared.constants.SALVAGE_DESPAWN_TICKS,
            });
        }
    }

    fn recordExplored(self: *GameEngine, player_id: u64, coord: Hex) !void {
        const connections = self.world_gen.connectedNeighbors(coord);
        for (connections.slice()) |neighbor| {
            self.db.saveExploredEdge(player_id, coord, neighbor, self.current_tick) catch |err| {
                log.warn("Failed to save explored edge: {}", .{err});
            };
        }
    }

    fn loadState(self: *GameEngine) !void {
        if (try self.db.loadServerState("current_tick")) |tick_str| {
            defer self.allocator.free(tick_str);
            self.current_tick = std.fmt.parseInt(u64, tick_str, 10) catch 0;
        }
        if (try self.db.loadServerState("next_id")) |id_str| {
            defer self.allocator.free(id_str);
            self.next_id = std.fmt.parseInt(u64, id_str, 10) catch 1;
        }
        if (try self.db.loadServerState("world_seed")) |seed_str| {
            defer self.allocator.free(seed_str);
            const stored_seed = std.fmt.parseInt(u64, seed_str, 10) catch 0;
            if (stored_seed != self.world_gen.world_seed) {
                log.warn("World seed mismatch: DB has {d}, config has {d}. Using config seed.", .{
                    stored_seed, self.world_gen.world_seed,
                });
            }
        }

        var players = try self.db.loadPlayers();
        defer players.deinit(self.allocator);
        for (players.items) |player| {
            try self.players.put(player.id, player);
        }

        var fleets = try self.db.loadFleets();
        defer fleets.deinit(self.allocator);
        for (fleets.items) |fleet| {
            try self.fleets.put(fleet.id, fleet);
        }

        var overrides = try self.db.loadSectorOverrides();
        defer overrides.deinit(self.allocator);
        for (overrides.items) |row| {
            const key = (Hex{ .q = row.q, .r = row.r }).toKey();
            try self.sector_overrides.put(key, row.override);
        }

        const player_count = self.players.count();
        const fleet_count = self.fleets.count();
        if (player_count > 0) {
            log.info("State loaded: {d} players, {d} fleets, tick {d}", .{
                player_count,
                fleet_count,
                self.current_tick,
            });
        } else {
            log.info("State loaded (empty -- fresh world)", .{});
        }
    }

    fn persistWorldSeed(self: *GameEngine) !void {
        var seed_buf: [20]u8 = undefined;
        const seed_str = std.fmt.bufPrint(&seed_buf, "{d}", .{self.world_gen.world_seed}) catch unreachable;
        try self.db.saveServerState("world_seed", seed_str);
    }

    pub fn persistDirtyState(self: *GameEngine) !void {
        try self.db.db.exec("BEGIN IMMEDIATE");
        errdefer self.db.db.exec("ROLLBACK") catch {};

        var tick_buf: [20]u8 = undefined;
        const tick_str = std.fmt.bufPrint(&tick_buf, "{d}", .{self.current_tick}) catch unreachable;
        try self.db.saveServerState("current_tick", tick_str);

        var id_buf: [20]u8 = undefined;
        const id_str = std.fmt.bufPrint(&id_buf, "{d}", .{self.next_id}) catch unreachable;
        try self.db.saveServerState("next_id", id_str);

        var dirty_iter = self.dirty_players.iterator();
        while (dirty_iter.next()) |entry| {
            const player_id = entry.key_ptr.*;
            if (self.players.get(player_id)) |player| {
                try self.db.savePlayer(player);
            }
        }

        var fleet_iter = self.dirty_fleets.iterator();
        while (fleet_iter.next()) |entry| {
            const fid = entry.key_ptr.*;
            if (self.fleets.get(fid)) |fleet| {
                try self.db.saveFleet(fleet);
            }
        }

        var sector_iter = self.dirty_sectors.iterator();
        while (sector_iter.next()) |entry| {
            const key = entry.key_ptr.*;
            if (self.sector_overrides.get(key)) |ov| {
                const hex_val = Hex.fromKey(key);
                try self.db.saveSectorOverride(hex_val.q, hex_val.r, ov);
            }
        }

        try self.db.db.exec("COMMIT");

        self.dirty_players.clearRetainingCapacity();
        self.dirty_fleets.clearRetainingCapacity();
        self.dirty_sectors.clearRetainingCapacity();
    }

    pub fn drainEvents(self: *GameEngine) []const shared.protocol.GameEvent {
        return self.pending_events.items;
    }
};

fn fleetMoveCooldown(fleet: *const Fleet) u16 {
    var min_speed: u8 = 255;
    for (fleet.ships[0..fleet.ship_count]) |ship| {
        if (ship.speed < min_speed) min_speed = ship.speed;
    }
    if (min_speed == 0) return shared.constants.MOVE_BASE_COOLDOWN;
    return @intCast(@as(u32, shared.constants.MOVE_BASE_COOLDOWN) * 10 / @as(u32, min_speed));
}

fn fleetFuelCost(fleet: *const Fleet) f32 {
    var total_mass: f32 = 0;
    for (fleet.ships[0..fleet.ship_count]) |ship| {
        total_mass += ship.hull_max;
    }
    return total_mass * shared.constants.FUEL_RATE_PER_MASS;
}

fn fleetHarvestPower(fleet: *const Fleet) f32 {
    var power: f32 = 0;
    for (fleet.ships[0..fleet.ship_count]) |ship| {
        power += switch (ship.class) {
            .hauler => 5.0,
            .scout => 1.0,
            else => 0.5,
        };
    }
    return power;
}

fn fleetCargoCapacity(fleet: *const Fleet) f32 {
    var cap: f32 = 0;
    for (fleet.ships[0..fleet.ship_count]) |ship| {
        const stats = ship.class.baseStats();
        cap += @floatFromInt(stats.cargo);
    }
    return cap;
}

pub const MAX_SHIPS_PER_FLEET: usize = 64;
pub const MAX_NPC_SHIPS: usize = 32;

pub const Player = struct {
    id: u64,
    name: []const u8,
    resources: Resources,
    homeworld: Hex,
    // TODO: building levels, research levels
};

pub const Fleet = struct {
    id: u64,
    owner_id: u64,
    location: Hex,
    state: FleetStatus,
    ships: [MAX_SHIPS_PER_FLEET]Ship,
    ship_count: usize,
    cargo: Resources,
    fuel: f32,
    fuel_max: f32,
    move_cooldown: u16,
    action_cooldown: u16,
    move_target: ?Hex,
    idle_ticks: u16,
};

pub const FleetStatus = enum {
    idle,
    moving,
    harvesting,
    in_combat,
    returning,
    docked,
};

pub const Ship = struct {
    id: u64,
    class: ShipClass,
    hull: f32,
    hull_max: f32,
    shield: f32,
    shield_max: f32,
    weapon_power: f32,
    speed: u8,
};

pub const NpcFleet = struct {
    id: u64,
    location: Hex,
    ships: [MAX_NPC_SHIPS]Ship,
    ship_count: u8,
    behavior: shared.world.NpcBehaviorType,
};

pub const Combat = struct {
    id: u64,
    sector: Hex,
    player_fleet_id: u64,
    npc_fleet_id: u64,
    npc_value: Resources, // for salvage calculation
    round: u16,
};

pub const SectorOverride = struct {
    metal_density: ?shared.constants.Density = null,
    crystal_density: ?shared.constants.Density = null,
    deut_density: ?shared.constants.Density = null,
    salvage: ?Resources = null,
    salvage_despawn_tick: ?u64 = null,
};
