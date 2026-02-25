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
        try self.processSectorRegen();
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

                    // Mark sector NPC as cleared for respawn timer
                    const cleared_key = active_combat.sector.toKey();
                    const cleared_ov = self.sector_overrides.getPtr(cleared_key) orelse blk: {
                        try self.sector_overrides.put(cleared_key, .{});
                        break :blk self.sector_overrides.getPtr(cleared_key).?;
                    };
                    cleared_ov.npc_cleared_tick = self.current_tick;
                    try self.dirty_sectors.put(cleared_key, {});
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
            const sector_key = fleet.location.toKey();
            const densities = SectorOverride.effectiveDensities(self.sector_overrides.get(sector_key), template);
            const metal_density = densities.metal;
            const crystal_density = densities.crystal;
            const deut_density = densities.deut;

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
                try self.accumulateHarvest(sector_key, .metal, actual, metal_density);
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
                try self.accumulateHarvest(sector_key, .crystal, actual, crystal_density);
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
                try self.accumulateHarvest(sector_key, .deut, actual, deut_density);
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

    const ResourceType = enum { metal, crystal, deut };

    fn accumulateHarvest(
        self: *GameEngine,
        sector_key: u32,
        resource: ResourceType,
        amount: f32,
        current_density: shared.constants.Density,
    ) !void {
        if (current_density == .none) return;

        const ov_ptr = self.sector_overrides.getPtr(sector_key) orelse blk: {
            try self.sector_overrides.put(sector_key, .{});
            break :blk self.sector_overrides.getPtr(sector_key).?;
        };

        const harvested_ptr = switch (resource) {
            .metal => &ov_ptr.metal_harvested,
            .crystal => &ov_ptr.crystal_harvested,
            .deut => &ov_ptr.deut_harvested,
        };

        harvested_ptr.* += amount;

        const threshold = current_density.depletionThreshold();
        if (threshold > 0 and harvested_ptr.* >= threshold) {
            harvested_ptr.* = 0;
            const new_density = current_density.downgrade();
            switch (resource) {
                .metal => ov_ptr.metal_density = new_density,
                .crystal => ov_ptr.crystal_density = new_density,
                .deut => ov_ptr.deut_density = new_density,
            }
            try self.dirty_sectors.put(sector_key, {});
        }
    }

    fn processSectorRegen(self: *GameEngine) !void {
        var iter = self.sector_overrides.iterator();
        while (iter.next()) |entry| {
            const sector_key = entry.key_ptr.*;
            const ov = entry.value_ptr;

            const has_depleted = (ov.metal_density != null and ov.metal_density.? != .pristine) or
                (ov.crystal_density != null and ov.crystal_density.? != .pristine) or
                (ov.deut_density != null and ov.deut_density.? != .pristine);
            if (!has_depleted) continue;

            // Skip regen if any player fleet is present
            const coord = Hex.fromKey(sector_key);
            var fleet_present = false;
            var fleet_iter = self.fleets.iterator();
            while (fleet_iter.next()) |f_entry| {
                if (f_entry.value_ptr.location.eql(coord)) {
                    fleet_present = true;
                    break;
                }
            }
            if (fleet_present) continue;

            const template = self.world_gen.generateSector(coord);
            var changed = false;

            changed = regenResource(&ov.metal_harvested, &ov.metal_density, template.metal_density) or changed;
            changed = regenResource(&ov.crystal_harvested, &ov.crystal_density, template.crystal_density) or changed;
            changed = regenResource(&ov.deut_harvested, &ov.deut_density, template.deut_density) or changed;

            if (changed) {
                try self.dirty_sectors.put(sector_key, {});
            }
        }
    }

    fn regenResource(harvested: *f32, override_density: *?shared.constants.Density, template_density: shared.constants.Density) bool {
        const current = override_density.* orelse return false;
        if (@intFromEnum(current) >= @intFromEnum(template_density)) return false;

        const regen_amount = shared.constants.SECTOR_REGEN_RATE * current.depletionThreshold();
        harvested.* -= regen_amount;
        if (harvested.* < 0) {
            const new_density = current.upgrade();
            if (@intFromEnum(new_density) >= @intFromEnum(template_density)) {
                override_density.* = null;
            } else {
                override_density.* = new_density;
            }
            harvested.* = 0;
        }
        return true;
    }

    fn processNpcBehavior(self: *GameEngine) !void {
        // Respawn check: clear npc_cleared_tick after zone-based delay
        var sector_iter = self.sector_overrides.iterator();
        while (sector_iter.next()) |entry| {
            const ov = entry.value_ptr;
            const cleared_tick = ov.npc_cleared_tick orelse continue;
            const coord = Hex.fromKey(entry.key_ptr.*);
            const zone = shared.constants.Zone.fromDistance(coord.distFromOrigin());
            const delay = shared.constants.npcRespawnDelay(zone);
            if (self.current_tick >= cleared_tick + delay) {
                ov.npc_cleared_tick = null;
                try self.dirty_sectors.put(entry.key_ptr.*, {});
            }
        }

        // Patrol movement: iterate npc_fleets not in combat
        var rng = std.Random.DefaultPrng.init(self.current_tick *% 0x9E3779B97F4A7C15);
        const random = rng.random();

        var npc_iter = self.npc_fleets.iterator();
        while (npc_iter.next()) |entry| {
            const npc = entry.value_ptr;
            if (npc.in_combat) continue;
            if (npc.behavior != .patrol and npc.behavior != .aggressive) continue;

            if (npc.patrol_timer > 0) {
                npc.patrol_timer -= 1;
                continue;
            }

            // Move to a random connected neighbor
            const connections = self.world_gen.connectedNeighbors(npc.location);
            if (connections.len == 0) continue;

            const idx = random.intRangeLessThan(u8, 0, connections.len);
            npc.location = connections.items[idx];
            npc.patrol_timer = shared.constants.NPC_PATROL_INTERVAL;

            // Check if player fleet at new location -> trigger combat
            var fleet_check = self.fleets.iterator();
            while (fleet_check.next()) |f_entry| {
                const fleet = f_entry.value_ptr;
                if (fleet.location.eql(npc.location) and fleet.state != .in_combat and fleet.ship_count > 0) {
                    const combat_id = self.nextId();
                    try self.active_combats.put(combat_id, Combat{
                        .id = combat_id,
                        .sector = npc.location,
                        .player_fleet_id = fleet.id,
                        .npc_fleet_id = npc.id,
                        .npc_value = npc.ships[0].class.buildCost(),
                        .round = 0,
                    });
                    fleet.state = .in_combat;
                    npc.in_combat = true;

                    try self.pending_events.append(self.allocator, .{
                        .tick = self.current_tick,
                        .kind = .{ .combat_started = .{
                            .player_fleet_id = fleet.id,
                            .enemy_fleet_id = npc.id,
                            .sector = npc.location,
                        } },
                    });
                    break;
                }
            }
        }
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

        const scout_stats = ShipClass.scout.baseStats();
        var fleet = Fleet{
            .id = fleet_id,
            .owner_id = player_id,
            .location = homeworld,
            .state = .idle,
            .ships = undefined,
            .ship_count = shared.constants.STARTING_SCOUTS,
            .cargo = .{},
            .fuel = 50000, // testing: high starting fuel
            .fuel_max = 50000,
            .move_cooldown = 0,
            .action_cooldown = 0,
            .move_target = null,
            .idle_ticks = 0,
        };

        for (0..shared.constants.STARTING_SCOUTS) |i| {
            fleet.ships[i] = Ship{
                .id = self.nextId(),
                .class = .scout,
                .hull = scout_stats.hull,
                .hull_max = scout_stats.hull,
                .shield = scout_stats.shield,
                .shield_max = scout_stats.shield,
                .weapon_power = scout_stats.weapon,
                .speed = scout_stats.speed,
            };
        }

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
        if (fleet.state == .moving) return error.OnCooldown;
        if (fleet.action_cooldown > 0) return error.OnCooldown;

        const template = self.world_gen.generateSector(fleet.location);
        const densities = SectorOverride.effectiveDensities(self.sector_overrides.get(fleet.location.toKey()), template);

        if (densities.metal == .none and densities.crystal == .none and densities.deut == .none) return error.NoResources;

        const max_cargo = fleetCargoCapacity(fleet);
        const current_cargo = fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium;
        if (current_cargo >= max_cargo) return error.CargoFull;

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
        // Check if an existing patrol NPC is at this location
        var npc_iter = self.npc_fleets.iterator();
        while (npc_iter.next()) |npc_entry| {
            const npc = npc_entry.value_ptr;
            if (npc.location.eql(fleet.location) and !npc.in_combat) {
                if (npc.behavior == .passive) continue;

                const combat_id = self.nextId();
                try self.active_combats.put(combat_id, Combat{
                    .id = combat_id,
                    .sector = fleet.location,
                    .player_fleet_id = fleet.id,
                    .npc_fleet_id = npc.id,
                    .npc_value = npc.ships[0].class.buildCost(),
                    .round = 0,
                });
                fleet.state = .in_combat;
                npc.in_combat = true;

                try self.pending_events.append(self.allocator, .{
                    .tick = self.current_tick,
                    .kind = .{ .combat_started = .{
                        .player_fleet_id = fleet.id,
                        .enemy_fleet_id = npc.id,
                        .sector = fleet.location,
                    } },
                });
                return;
            }
        }

        // Check if sector NPC was cleared recently
        const sector_key = fleet.location.toKey();
        if (self.sector_overrides.get(sector_key)) |ov| {
            if (ov.npc_cleared_tick != null) return;
        }

        const template = self.world_gen.generateSector(fleet.location);
        if (template.npc_template) |npc| {
            if (npc.behavior == .passive) return;

            const combat_id = self.nextId();
            const npc_fleet_id = self.nextId();

            var npc_fleet = NpcFleet{
                .id = npc_fleet_id,
                .location = fleet.location,
                .ships = undefined,
                .ship_count = npc.count,
                .behavior = npc.behavior,
                .home_sector = fleet.location,
                .in_combat = true,
            };

            const stats = npc.ship_class.baseStats();
            const m = npc.stat_multiplier;
            var i: u8 = 0;
            while (i < @min(npc.count, 32)) : (i += 1) {
                const hull = stats.hull * m;
                const shield = stats.shield * m;
                const weapon = stats.weapon * m;
                npc_fleet.ships[i] = Ship{
                    .id = self.nextId(),
                    .class = npc.ship_class,
                    .hull = hull,
                    .hull_max = hull,
                    .shield = shield,
                    .shield_max = shield,
                    .weapon_power = weapon,
                    .speed = stats.speed,
                };
            }

            try self.npc_fleets.put(npc_fleet_id, npc_fleet);
            try self.active_combats.put(combat_id, Combat{
                .id = combat_id,
                .sector = fleet.location,
                .player_fleet_id = fleet.id,
                .npc_fleet_id = npc_fleet_id,
                .npc_value = npc.ship_class.buildCost(),
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
    home_sector: Hex = Hex.ORIGIN,
    patrol_timer: u16 = 0,
    in_combat: bool = false,
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
    metal_harvested: f32 = 0,
    crystal_harvested: f32 = 0,
    deut_harvested: f32 = 0,
    salvage: ?Resources = null,
    salvage_despawn_tick: ?u64 = null,
    npc_cleared_tick: ?u64 = null,

    const Density = shared.constants.Density;

    pub fn effectiveDensities(override: ?SectorOverride, template: shared.world.SectorTemplate) struct { metal: Density, crystal: Density, deut: Density } {
        const ov = override orelse return .{ .metal = template.metal_density, .crystal = template.crystal_density, .deut = template.deut_density };
        return .{
            .metal = ov.metal_density orelse template.metal_density,
            .crystal = ov.crystal_density orelse template.crystal_density,
            .deut = ov.deut_density orelse template.deut_density,
        };
    }
};
