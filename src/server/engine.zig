const std = @import("std");
const shared = @import("shared");
const combat = @import("combat.zig");
const Database = @import("database.zig").Database;

const Hex = shared.Hex;
const Resources = shared.constants.Resources;
const ShipClass = shared.constants.ShipClass;
const WorldGen = shared.world.WorldGen;
const scaling = shared.scaling;
const BuildingType = scaling.BuildingType;
const ResearchType = scaling.ResearchType;

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
    deleted_fleet_ids: std.AutoHashMap(u64, void),

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
            .deleted_fleet_ids = std.AutoHashMap(u64, void).init(allocator),
        };

        try engine.loadState();
        try engine.persistWorldSeed();

        return engine;
    }

    pub fn deinit(self: *GameEngine) void {
        var player_iter = self.players.iterator();
        while (player_iter.next()) |entry| {
            self.allocator.free(entry.value_ptr.name);
            if (entry.value_ptr.token_hash) |th| self.allocator.free(th);
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
        self.deleted_fleet_ids.deinit();
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
        try self.processBuildQueues();
        try self.processSalvageDespawn();
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

                const first_visit = !(self.db.hasExploredSector(fleet.owner_id, target) catch false);
                try self.recordExplored(fleet.owner_id, fleet.location);
                try self.pending_events.append(self.allocator, .{
                    .tick = self.current_tick,
                    .kind = .{ .sector_entered = .{
                        .fleet_id = fleet.id,
                        .sector = target,
                        .first_visit = first_visit,
                    } },
                });

                try self.checkHomeworldDocking(fleet);
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

            // Collect player fleet pointers
            var pf_ptrs: [MAX_COMBAT_FLEETS]*Fleet = undefined;
            var pf_count: usize = 0;
            for (active_combat.player_fleet_ids[0..active_combat.player_fleet_count]) |fid| {
                if (self.fleets.getPtr(fid)) |fp| {
                    pf_ptrs[pf_count] = fp;
                    pf_count += 1;
                }
            }

            // Collect NPC fleet pointers
            var npc_ptrs: [MAX_COMBAT_FLEETS]*NpcFleet = undefined;
            var npc_count: usize = 0;
            for (active_combat.npc_fleet_ids[0..active_combat.npc_fleet_count]) |nid| {
                if (self.npc_fleets.getPtr(nid)) |np| {
                    npc_ptrs[npc_count] = np;
                    npc_count += 1;
                }
            }

            if (pf_count == 0 or npc_count == 0) {
                try to_remove.append(self.allocator, combat_id);
                continue;
            }

            const result = try combat.resolveCombatRound(
                self.allocator,
                active_combat,
                pf_ptrs[0..pf_count],
                npc_ptrs[0..npc_count],
                self.current_tick,
            );
            defer self.allocator.free(result.events);

            for (result.events) |event| {
                try self.pending_events.append(self.allocator, event);
            }

            // Mark all participating player fleets dirty
            for (active_combat.player_fleet_ids[0..active_combat.player_fleet_count]) |fid| {
                try self.dirty_fleets.put(fid, {});
            }

            if (result.concluded) {
                try to_remove.append(self.allocator, combat_id);

                // Update state for each player fleet
                for (pf_ptrs[0..pf_count]) |pf| {
                    pf.state = if (pf.ship_count == 0) .docked else .idle;
                    pf.idle_ticks = 0;
                }

                try self.pending_events.append(self.allocator, .{
                    .tick = self.current_tick,
                    .kind = .{ .combat_ended = .{
                        .sector = active_combat.sector,
                        .player_victory = result.player_won,
                    } },
                });

                if (result.player_won) {
                    try self.dropSalvage(active_combat.sector, active_combat.npc_value);
                    try self.generateLootDrops(active_combat, pf_ptrs[0..pf_count]);

                    const cleared_key = active_combat.sector.toKey();
                    const cleared_ov = try self.ensureOverride(cleared_key);
                    cleared_ov.npc_cleared_tick = self.current_tick;
                    try self.dirty_sectors.put(cleared_key, {});
                }

                // Remove destroyed NPCs
                for (active_combat.npc_fleet_ids[0..active_combat.npc_fleet_count]) |nid| {
                    _ = self.npc_fleets.remove(nid);
                }
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

            const player_data = self.players.get(fleet.owner_id);
            const research = if (player_data) |p| p.research else null;
            const harvest_power = fleetHarvestPower(fleet, research);
            const max_cargo = fleetCargoCapacity(fleet);
            var remaining = max_cargo - (fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium);

            if (remaining <= 0) {
                fleet.state = .idle;
                continue;
            }

            const HarvestTarget = struct { density: shared.constants.Density, cargo: *f32, accum: ResourceType, event: shared.protocol.HarvestResource };
            const targets = [_]HarvestTarget{
                .{ .density = densities.metal, .cargo = &fleet.cargo.metal, .accum = .metal, .event = .metal },
                .{ .density = densities.crystal, .cargo = &fleet.cargo.crystal, .accum = .crystal, .event = .crystal },
                .{ .density = densities.deut, .cargo = &fleet.cargo.deuterium, .accum = .deut, .event = .deuterium },
            };

            var harvested_any = false;
            for (targets) |t| {
                const amount = t.density.harvestMultiplier() * harvest_power;
                if (amount > 0 and remaining > 0) {
                    const actual = @min(amount, remaining);
                    t.cargo.* += actual;
                    remaining -= actual;
                    harvested_any = true;
                    try self.accumulateHarvest(sector_key, t.accum, actual, t.density);
                    try self.pending_events.append(self.allocator, .{
                        .tick = self.current_tick,
                        .kind = .{ .resource_harvested = .{ .fleet_id = fleet.id, .resource_type = t.event, .amount = actual } },
                    });
                }
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

        const ov_ptr = try self.ensureOverride(sector_key);

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

            if (!isDepleted(ov.metal_density) and !isDepleted(ov.crystal_density) and !isDepleted(ov.deut_density)) continue;

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

    fn isDepleted(density: ?shared.constants.Density) bool {
        const d = density orelse return false;
        return d != .pristine;
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

    fn processSalvageDespawn(self: *GameEngine) !void {
        var iter = self.sector_overrides.iterator();
        while (iter.next()) |entry| {
            const ov = entry.value_ptr;
            const despawn_tick = ov.salvage_despawn_tick orelse continue;
            if (self.current_tick >= despawn_tick) {
                ov.salvage = null;
                ov.salvage_despawn_tick = null;
                try self.dirty_sectors.put(entry.key_ptr.*, {});
            }
        }
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
                    try self.startCombat(fleet, npc);
                    break;
                }
            }
        }
    }

    fn processHomeworlds(self: *GameEngine) !void {
        var iter = self.players.iterator();
        while (iter.next()) |entry| {
            var player = entry.value_ptr;

            player.resources.metal += scaling.productionPerTick(.metal_mine, player.buildings.metal_mine);
            player.resources.crystal += scaling.productionPerTick(.crystal_mine, player.buildings.crystal_mine);
            player.resources.deuterium += scaling.productionPerTick(.deuterium_synthesizer, player.buildings.deuterium_synthesizer);
            try self.dirty_players.put(player.id, {});
        }
    }

    fn processBuildQueues(self: *GameEngine) !void {
        var iter = self.players.iterator();
        while (iter.next()) |entry| {
            var player = entry.value_ptr;

            // Building queue
            if (player.building_queue) |q| {
                if (self.current_tick >= q.end_tick) {
                    player.buildings.set(q.building, q.target_level);
                    player.building_queue = null;
                    try self.dirty_players.put(player.id, {});
                    try self.pending_events.append(self.allocator, .{
                        .tick = self.current_tick,
                        .kind = .{ .building_completed = .{
                            .building_type = q.building,
                            .new_level = q.target_level,
                            .player_id = player.id,
                        } },
                    });
                    if (q.building == .fuel_depot) {
                        try self.recalculatePlayerFleetFuel(player);
                    }
                }
            }

            // Ship queue
            if (player.ship_queue) |*q| {
                if (self.current_tick >= q.end_tick) {
                    try self.addShipToHomeworld(player, q.ship_class);
                    q.built += 1;
                    try self.dirty_players.put(player.id, {});
                    try self.pending_events.append(self.allocator, .{
                        .tick = self.current_tick,
                        .kind = .{ .ship_built = .{
                            .ship_class = q.ship_class,
                            .count = 1,
                            .player_id = player.id,
                        } },
                    });

                    if (q.built >= q.count) {
                        player.ship_queue = null;
                    } else {
                        // Advance end_tick for next ship
                        const per_ship = scaling.shipBuildTime(q.ship_class, player.buildings.shipyard);
                        q.end_tick = self.current_tick + per_ship;
                    }
                }
            }

            // Research queue
            if (player.research_queue) |q| {
                if (self.current_tick >= q.end_tick) {
                    player.research.set(q.tech, q.target_level);
                    player.research_queue = null;
                    try self.dirty_players.put(player.id, {});
                    try self.pending_events.append(self.allocator, .{
                        .tick = self.current_tick,
                        .kind = .{ .research_completed = .{
                            .tech = q.tech,
                            .new_level = q.target_level,
                            .player_id = player.id,
                        } },
                    });
                    if (q.tech == .extended_fuel_tanks) {
                        try self.recalculatePlayerFleetFuel(player);
                    }
                }
            }
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

                if (fleet.idle_ticks >= shared.constants.SHIELD_REGEN_IDLE_TICKS) {
                    for (fleet.ships[0..fleet.ship_count]) |*ship| {
                        if (ship.shield < ship.shield_max) {
                            ship.shield = @min(ship.shield + ship.shield_max * 0.1, ship.shield_max);
                            try self.dirty_fleets.put(fleet.id, {});
                        }
                    }
                }
            }
        }
    }

    pub const RegisterResult = struct {
        player_id: u64,
        token_hex: [shared.constants.TOKEN_HEX_LEN]u8,
    };

    pub fn registerNewPlayer(self: *GameEngine, raw_name: []const u8, max_players: u32) !RegisterResult {
        var name_buf: [shared.constants.PLAYER_NAME_MAX_LEN]u8 = undefined;
        const name = normalizeName(raw_name, &name_buf) orelse return error.NameInvalid;
        try validatePlayerName(name);

        if (self.players.count() >= max_players) {
            const db_count = self.db.countPlayers() catch 0;
            if (db_count >= max_players) return error.PlayerCapReached;
        }

        var iter = self.players.iterator();
        while (iter.next()) |entry| {
            if (std.mem.eql(u8, entry.value_ptr.name, name)) {
                if (entry.value_ptr.token_hash == null) {
                    const token = generateToken();
                    var hash_buf: [shared.constants.HASH_BYTES]u8 = undefined;
                    hashToken(&token, &hash_buf);

                    entry.value_ptr.token_hash = try self.allocator.dupe(u8, &hash_buf);
                    entry.value_ptr.last_login_at = currentTimestamp();

                    self.db.savePlayerAuth(entry.key_ptr.*, &hash_buf, entry.value_ptr.created_at) catch |err| {
                        log.warn("Failed to save auth for claimed player: {}", .{err});
                    };

                    log.info("Player '{s}' claimed legacy account (id={d})", .{ name, entry.key_ptr.* });
                    return .{ .player_id = entry.key_ptr.*, .token_hex = token };
                }
                return error.NameTaken;
            }
        }

        const token = generateToken();
        var hash_buf: [shared.constants.HASH_BYTES]u8 = undefined;
        hashToken(&token, &hash_buf);

        const now = currentTimestamp();
        const player_id = self.nextId();
        const homeworld = self.findHomeworldLocation() orelse return error.RegistrationFailed;

        const player = Player{
            .id = player_id,
            .name = try self.allocator.dupe(u8, name),
            .resources = shared.constants.STARTING_RESOURCES,
            .homeworld = homeworld,
            .token_hash = try self.allocator.dupe(u8, &hash_buf),
            .created_at = now,
            .last_login_at = now,
        };

        try self.players.put(player_id, player);
        try self.db.savePlayer(player);
        try self.createStartingFleet(player_id, homeworld, player.research);

        log.info("Player '{s}' registered (id={d}) at homeworld {any}", .{ name, player_id, homeworld });

        return .{ .player_id = player_id, .token_hex = token };
    }

    pub fn loginPlayer(self: *GameEngine, raw_name: []const u8, token: []const u8) !u64 {
        var name_buf: [shared.constants.PLAYER_NAME_MAX_LEN]u8 = undefined;
        const name = normalizeName(raw_name, &name_buf) orelse return error.AuthFailed;

        var iter = self.players.iterator();
        while (iter.next()) |entry| {
            if (std.mem.eql(u8, entry.value_ptr.name, name)) {
                const stored_hash = entry.value_ptr.token_hash orelse return error.AuthFailed;
                var provided_hash: [shared.constants.HASH_BYTES]u8 = undefined;
                hashToken(token, &provided_hash);
                if (!constantTimeEql(&provided_hash, stored_hash)) return error.AuthFailed;

                entry.value_ptr.last_login_at = currentTimestamp();
                self.db.updateLastLogin(entry.key_ptr.*, entry.value_ptr.last_login_at) catch |err| {
                    log.warn("Failed to update last_login_at: {}", .{err});
                };

                log.info("Player '{s}' logged in (id={d})", .{ name, entry.key_ptr.* });
                return entry.key_ptr.*;
            }
        }

        return error.AuthFailed;
    }

    fn createStartingFleet(self: *GameEngine, player_id: u64, homeworld: Hex, research: scaling.ResearchLevels) !void {
        const fleet_id = self.nextId();
        const scout_base = ShipClass.scout.baseStats();
        const scout_stats = scaling.applyResearchToStats(scout_base, research);

        var fleet = Fleet{
            .id = fleet_id,
            .owner_id = player_id,
            .location = homeworld,
            .state = .idle,
            .ships = undefined,
            .ship_count = shared.constants.STARTING_SCOUTS,
            .cargo = .{},
            .fuel = 50000,
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
    }

    pub fn handleMove(self: *GameEngine, fleet_id: u64, target: Hex) !void {
        const fleet = self.fleets.getPtr(fleet_id) orelse return error.FleetNotFound;

        if (fleet.ship_count == 0) return error.NoShips;
        if (fleet.state == .in_combat) return error.InCombat;
        if (fleet.action_cooldown > 0) return error.OnCooldown;

        const player = self.players.get(fleet.owner_id);

        const connections = self.world_gen.connectedNeighbors(fleet.location);
        var valid = false;
        for (connections.slice()) |conn| {
            if (conn.eql(target)) {
                valid = true;
                break;
            }
        }
        if (!valid) return error.NoConnection;

        const research = if (player) |p| p.research else null;
        const fuel_cost = fleetFuelCost(fleet, research);
        if (fleet.fuel < fuel_cost) return error.InsufficientFuel;

        fleet.fuel -= fuel_cost;
        fleet.state = .moving;
        fleet.move_target = target;
        fleet.move_cooldown = fleetMoveCooldown(fleet, research);
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

    pub fn handleCollectSalvage(self: *GameEngine, fleet_id: u64) !void {
        const fleet = self.fleets.getPtr(fleet_id) orelse return error.FleetNotFound;
        if (fleet.ship_count == 0) return error.NoShips;
        if (fleet.state == .in_combat) return error.InCombat;
        if (fleet.state == .moving) return error.OnCooldown;

        const key = fleet.location.toKey();
        const ov = self.sector_overrides.getPtr(key) orelse return error.NoResources;
        if (ov.salvage == null) return error.NoResources;

        try self.collectSalvage(fleet);
    }

    pub fn handleAttack(self: *GameEngine, fleet_id: u64, target_fleet_id: u64) !void {
        const fleet = self.fleets.getPtr(fleet_id) orelse return error.FleetNotFound;
        if (fleet.ship_count == 0) return error.NoShips;
        if (fleet.state == .in_combat) return error.InCombat;
        if (fleet.state == .moving) return error.OnCooldown;

        const npc = self.npc_fleets.getPtr(target_fleet_id) orelse {
            // Target might be a template NPC not yet spawned -- check sector
            const sector_key = fleet.location.toKey();
            if (self.sector_overrides.get(sector_key)) |ov| {
                if (ov.npc_cleared_tick != null) return error.InvalidTarget;
            }
            const template = self.world_gen.generateSector(fleet.location);
            if (template.npc_template) |npc_tmpl| {
                const spawned = try self.spawnNpcFleet(fleet.location, npc_tmpl);
                try self.startCombat(fleet, spawned);
                return;
            }
            return error.InvalidTarget;
        };

        if (!npc.location.eql(fleet.location)) return error.InvalidTarget;
        if (npc.in_combat) return error.InCombat;
        try self.startCombat(fleet, npc);
    }

    pub fn handleRecall(self: *GameEngine, fleet_id: u64) !void {
        const fleet = self.fleets.getPtr(fleet_id) orelse return error.FleetNotFound;
        if (fleet.ship_count == 0) return error.NoShips;
        const player = self.players.getPtr(fleet.owner_id) orelse return error.PlayerNotFound;

        const dist = Hex.distance(fleet.location, player.homeworld);

        const fuel_cost = fleetFuelCost(fleet, player.research) * @as(f32, @floatFromInt(dist)) * shared.constants.RECALL_FUEL_MULTIPLIER;
        if (fleet.fuel < fuel_cost) return error.InsufficientFuel;

        fleet.fuel -= fuel_cost;

        const base_damage_chance = shared.constants.RECALL_DAMAGE_CHANCE_PER_HEX * @as(f32, @floatFromInt(dist));
        const ej_reduction = scaling.recallDamageReduction(player.research.emergency_jump);
        const damage_chance = @min(
            shared.constants.RECALL_DAMAGE_CHANCE_CAP,
            @max(0, base_damage_chance - ej_reduction),
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

        try self.dockFleet(fleet, player);
    }

    pub fn handleBuild(self: *GameEngine, player_id: u64, building_type: BuildingType) !void {
        const player = self.players.getPtr(player_id) orelse return error.PlayerNotFound;

        if (player.building_queue != null) return error.QueueFull;

        const current_level = player.buildings.get(building_type);
        if (current_level >= scaling.MAX_BUILDING_LEVEL) return error.MaxLevelReached;

        if (!scaling.buildingPrerequisitesMet(building_type, player.buildings)) return error.PrerequisitesNotMet;

        const target_level = current_level + 1;
        const cost = scaling.buildingCost(building_type, target_level);
        if (!player.resources.canAfford(cost)) return error.InsufficientResources;

        player.resources = player.resources.sub(cost);
        const duration = scaling.buildingTime(building_type, target_level);
        player.building_queue = .{
            .building = building_type,
            .target_level = target_level,
            .start_tick = self.current_tick,
            .end_tick = self.current_tick + duration,
        };
        try self.dirty_players.put(player_id, {});
    }

    pub fn handleResearch(self: *GameEngine, player_id: u64, tech: ResearchType) !void {
        const player = self.players.getPtr(player_id) orelse return error.PlayerNotFound;

        if (player.research_queue != null) return error.QueueFull;
        if (player.buildings.research_lab == 0) return error.NoResearchLab;

        const current_level = player.research.get(tech);
        if (current_level >= scaling.researchMaxLevel(tech)) return error.MaxLevelReached;

        if (!scaling.researchPrerequisitesMet(tech, player.buildings, player.research)) return error.PrerequisitesNotMet;

        const target_level = current_level + 1;
        const cost = scaling.researchCost(tech, target_level);
        if (!player.resources.canAfford(cost)) return error.InsufficientResources;

        // Check fragment cost for level III+
        const frag_cost = scaling.researchFragmentCost(tech, target_level);
        if (frag_cost) |fc| {
            if (!player.fragments.canAfford(fc)) return error.InsufficientFragments;
        }

        player.resources = player.resources.sub(cost);
        if (frag_cost) |fc| {
            player.fragments.sub(fc);
        }
        const duration = scaling.researchTime(tech, target_level);
        player.research_queue = .{
            .tech = tech,
            .target_level = target_level,
            .start_tick = self.current_tick,
            .end_tick = self.current_tick + duration,
        };
        try self.dirty_players.put(player_id, {});
    }

    pub fn handleBuildShip(self: *GameEngine, player_id: u64, ship_class: ShipClass, count: u16) !void {
        const player = self.players.getPtr(player_id) orelse return error.PlayerNotFound;

        if (player.ship_queue != null) return error.QueueFull;
        if (player.buildings.shipyard == 0) return error.NoShipyard;

        if (!scaling.shipClassUnlocked(ship_class, player.research)) return error.ShipLocked;

        const unit_cost = ship_class.buildCost();
        const total_cost = Resources{
            .metal = unit_cost.metal * @as(f32, @floatFromInt(count)),
            .crystal = unit_cost.crystal * @as(f32, @floatFromInt(count)),
            .deuterium = unit_cost.deuterium * @as(f32, @floatFromInt(count)),
        };
        if (!player.resources.canAfford(total_cost)) return error.InsufficientResources;

        player.resources = player.resources.sub(total_cost);
        const per_ship = scaling.shipBuildTime(ship_class, player.buildings.shipyard);
        player.ship_queue = .{
            .ship_class = ship_class,
            .count = count,
            .built = 0,
            .start_tick = self.current_tick,
            .end_tick = self.current_tick + per_ship,
        };
        try self.dirty_players.put(player_id, {});
    }

    pub fn handleCancelBuild(self: *GameEngine, player_id: u64, queue_type: scaling.QueueType) !void {
        const player = self.players.getPtr(player_id) orelse return error.PlayerNotFound;

        switch (queue_type) {
            .building => {
                const q = player.building_queue orelse return error.NoResources;
                const cost = scaling.buildingCost(q.building, q.target_level);
                player.resources = player.resources.add(cost.scale(scaling.CANCEL_REFUND_FRACTION));
                player.building_queue = null;
            },
            .ship => {
                const q = player.ship_queue orelse return error.NoResources;
                const remaining: f32 = @floatFromInt(q.count - q.built);
                const refund = q.ship_class.buildCost().scale(remaining).scale(scaling.CANCEL_REFUND_FRACTION);
                player.resources = player.resources.add(refund);
                player.ship_queue = null;
            },
            .research => {
                const q = player.research_queue orelse return error.NoResources;
                const cost = scaling.researchCost(q.tech, q.target_level);
                player.resources = player.resources.add(cost.scale(scaling.CANCEL_REFUND_FRACTION));
                player.research_queue = null;
            },
        }
        try self.dirty_players.put(player_id, {});
    }

    fn addShipToHomeworld(self: *GameEngine, player: *Player, ship_class: ShipClass) !void {
        if (player.docked_ship_count >= MAX_SHIPS_PER_FLEET) return;

        const base_stats = ship_class.baseStats();
        const research_stats = scaling.applyResearchToStats(base_stats, player.research);
        const stats = scaling.applyComponentBonus(research_stats, ship_class, player.components);

        player.docked_ships[player.docked_ship_count] = Ship{
            .id = self.nextId(),
            .class = ship_class,
            .hull = stats.hull,
            .hull_max = stats.hull,
            .shield = stats.shield,
            .shield_max = stats.shield,
            .weapon_power = stats.weapon,
            .speed = stats.speed,
        };
        player.docked_ship_count += 1;

        try self.dirty_players.put(player.id, {});
    }

    fn countPlayerFleets(self: *const GameEngine, player_id: u64) usize {
        var count: usize = 0;
        var iter = self.fleets.iterator();
        while (iter.next()) |entry| {
            const f = entry.value_ptr;
            if (f.owner_id == player_id and f.ship_count > 0) {
                count += 1;
            }
        }
        return count;
    }

    fn findHomeworldLocation(self: *GameEngine) ?Hex {
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

            var player_iter = self.players.iterator();
            const too_close = while (player_iter.next()) |entry| {
                if (Hex.distance(entry.value_ptr.homeworld, candidate) <= 1) break true;
            } else false;
            if (!too_close) return candidate;
        }

        return null;
    }

    fn checkHomeworldDocking(self: *GameEngine, fleet: *Fleet) !void {
        const player = self.players.getPtr(fleet.owner_id) orelse return;
        if (!fleet.location.eql(player.homeworld)) return;
        try self.dockFleet(fleet, player);
    }

    fn dockFleet(self: *GameEngine, fleet: *Fleet, player: *Player) !void {
        // Deposit cargo to player resources
        player.resources = player.resources.add(fleet.cargo);
        fleet.cargo = .{};

        // Refuel
        fleet.fuel_max = fleetFuelMax(fleet, player);
        fleet.fuel = fleet.fuel_max;

        try self.dirty_players.put(player.id, {});
        try self.dirty_fleets.put(fleet.id, {});
    }

    fn checkNpcEncounter(self: *GameEngine, fleet: *Fleet) !void {
        // Check if an existing patrol NPC is at this location
        var npc_iter = self.npc_fleets.iterator();
        while (npc_iter.next()) |npc_entry| {
            const npc = npc_entry.value_ptr;
            if (npc.location.eql(fleet.location) and !npc.in_combat) {
                if (npc.behavior == .passive) continue;
                try self.startCombat(fleet, npc);
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
            const spawned = try self.spawnNpcFleet(fleet.location, npc);
            try self.startCombat(fleet, spawned);
        }
    }

    fn spawnNpcFleet(self: *GameEngine, location: Hex, npc: shared.world.NpcTemplate) !*NpcFleet {
        const npc_fleet_id = self.nextId();
        var npc_fleet = NpcFleet{
            .id = npc_fleet_id,
            .location = location,
            .ships = undefined,
            .ship_count = npc.count,
            .behavior = npc.behavior,
            .home_sector = location,
        };

        const stats = npc.ship_class.baseStats();
        const m = npc.stat_multiplier;
        var i: u8 = 0;
        while (i < @min(npc.count, 32)) : (i += 1) {
            const hull = stats.hull * m;
            const shield = stats.shield * m;
            npc_fleet.ships[i] = Ship{
                .id = self.nextId(),
                .class = npc.ship_class,
                .hull = hull,
                .hull_max = hull,
                .shield = shield,
                .shield_max = shield,
                .weapon_power = stats.weapon * m,
                .speed = stats.speed,
            };
        }

        try self.npc_fleets.put(npc_fleet_id, npc_fleet);
        return self.npc_fleets.getPtr(npc_fleet_id).?;
    }

    fn ensureOverride(self: *GameEngine, sector_key: u32) !*SectorOverride {
        if (self.sector_overrides.getPtr(sector_key)) |ptr| return ptr;
        try self.sector_overrides.put(sector_key, .{});
        return self.sector_overrides.getPtr(sector_key).?;
    }

    fn startCombat(self: *GameEngine, fleet: *Fleet, npc: *NpcFleet) !void {
        // Check for existing combat in same sector -- join it
        var combat_iter = self.active_combats.iterator();
        while (combat_iter.next()) |c_entry| {
            const existing = c_entry.value_ptr;
            if (existing.sector.eql(fleet.location)) {
                if (!existing.hasPlayerFleet(fleet.id)) {
                    existing.addPlayerFleet(fleet.id);
                    fleet.state = .in_combat;
                }
                if (!existing.hasNpcFleet(npc.id)) {
                    existing.addNpcFleet(npc.id);
                    npc.in_combat = true;
                    existing.npc_value = existing.npc_value.add(npcFleetValue(npc));
                }
                try self.pending_events.append(self.allocator, .{
                    .tick = self.current_tick,
                    .kind = .{ .combat_started = .{
                        .player_fleet_id = fleet.id,
                        .enemy_fleet_id = npc.id,
                        .sector = fleet.location,
                    } },
                });
                try self.enrollAllPlayerFleetsInSectorCombat(existing);
                return;
            }
        }

        // No existing combat -- create new
        const combat_id = self.nextId();
        var new_combat = Combat{
            .id = combat_id,
            .sector = fleet.location,
            .npc_value = npcFleetValue(npc),
            .npc_ship_class = npc.ships[0].class,
            .npc_ship_count = npc.ship_count,
            .round = 0,
        };
        new_combat.addPlayerFleet(fleet.id);
        new_combat.addNpcFleet(npc.id);
        try self.active_combats.put(combat_id, new_combat);
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

        try self.enrollAllPlayerFleetsInSectorCombat(self.active_combats.getPtr(combat_id).?);
    }

    fn enrollAllPlayerFleetsInSectorCombat(self: *GameEngine, active_combat: *Combat) !void {
        var fleet_iter = self.fleets.iterator();
        while (fleet_iter.next()) |f_entry| {
            const f = f_entry.value_ptr;
            if (!f.location.eql(active_combat.sector)) continue;
            if (f.state == .in_combat) continue;
            if (f.ship_count == 0) continue;
            if (active_combat.hasPlayerFleet(f.id)) continue;
            active_combat.addPlayerFleet(f.id);
            f.state = .in_combat;
        }
    }

    pub fn handleCreateFleet(self: *GameEngine, player_id: u64) !void {
        const player = self.players.getPtr(player_id) orelse return error.PlayerNotFound;

        if (self.countPlayerFleets(player_id) >= shared.constants.MAX_FLEETS_PER_PLAYER) {
            return error.FleetLimitReached;
        }

        // Must be at homeworld: player has a fleet at homeworld or has docked ships
        const at_home = player.docked_ship_count > 0 or self.playerHasFleetAtHomeworld(player_id, player.homeworld);
        if (!at_home) return error.NotAtHomeworld;

        const fleet_id = self.nextId();
        const new_fleet = Fleet{
            .id = fleet_id,
            .owner_id = player_id,
            .location = player.homeworld,
            .state = .idle,
            .ships = undefined,
            .ship_count = 0,
            .cargo = .{},
            .fuel = 0,
            .fuel_max = 0,
            .move_cooldown = 0,
            .action_cooldown = 0,
            .move_target = null,
            .idle_ticks = 0,
        };
        try self.fleets.put(fleet_id, new_fleet);
        try self.dirty_fleets.put(fleet_id, {});
    }

    pub fn handleDissolveFleet(self: *GameEngine, player_id: u64, fleet_id: u64) !void {
        const player = self.players.getPtr(player_id) orelse return error.PlayerNotFound;
        const fleet = self.fleets.getPtr(fleet_id) orelse return error.FleetNotFound;

        if (fleet.owner_id != player_id) return error.FleetNotFound;
        if (!fleet.location.eql(player.homeworld)) return error.NotAtHomeworld;
        if (fleet.state == .in_combat) return error.InCombat;

        // Move all ships to docked pool
        for (fleet.ships[0..fleet.ship_count]) |ship| {
            if (player.docked_ship_count >= MAX_SHIPS_PER_FLEET) return error.DockFull;
            player.docked_ships[player.docked_ship_count] = ship;
            player.docked_ship_count += 1;
        }

        // Deposit fleet cargo
        player.resources = player.resources.add(fleet.cargo);

        _ = self.fleets.remove(fleet_id);
        try self.deleted_fleet_ids.put(fleet_id, {});
        try self.dirty_players.put(player_id, {});
    }

    pub fn handleTransferShip(self: *GameEngine, player_id: u64, ship_id: u64, fleet_id: u64) !void {
        const player = self.players.getPtr(player_id) orelse return error.PlayerNotFound;
        const fleet = self.fleets.getPtr(fleet_id) orelse return error.FleetNotFound;

        if (fleet.owner_id != player_id) return error.FleetNotFound;
        if (!fleet.location.eql(player.homeworld)) return error.NotAtHomeworld;
        if (fleet.state == .in_combat) return error.InCombat;
        if (fleet.ship_count >= MAX_SHIPS_PER_FLEET) return error.CargoFull;

        const ship = removeDockedShip(player, ship_id) orelse return error.ShipNotFound;

        fleet.ships[fleet.ship_count] = ship;
        fleet.ship_count += 1;

        fleet.fuel_max = fleetFuelMax(fleet, player);
        fleet.fuel = fleet.fuel_max;

        try self.dirty_fleets.put(fleet_id, {});
        try self.dirty_players.put(player_id, {});
    }

    pub fn handleDockShip(self: *GameEngine, player_id: u64, ship_id: u64) !void {
        const player = self.players.getPtr(player_id) orelse return error.PlayerNotFound;
        if (player.docked_ship_count >= MAX_SHIPS_PER_FLEET) return error.DockFull;

        // Find which fleet contains this ship
        var target_fleet: ?*Fleet = null;
        var iter = self.fleets.iterator();
        while (iter.next()) |entry| {
            const f = entry.value_ptr;
            if (f.owner_id != player_id) continue;
            if (!f.location.eql(player.homeworld)) continue;
            if (f.state == .in_combat or f.state == .moving) continue;
            for (f.ships[0..f.ship_count]) |ship| {
                if (ship.id == ship_id) {
                    target_fleet = f;
                    break;
                }
            }
            if (target_fleet != null) break;
        }

        const fleet = target_fleet orelse return error.ShipNotFound;
        const ship = removeShipFromFleet(fleet, ship_id) orelse return error.ShipNotFound;

        player.docked_ships[player.docked_ship_count] = ship;
        player.docked_ship_count += 1;

        fleet.fuel_max = fleetFuelMax(fleet, player);
        if (fleet.fuel > fleet.fuel_max) fleet.fuel = fleet.fuel_max;

        try self.dirty_fleets.put(fleet.id, {});
        try self.dirty_players.put(player_id, {});
    }

    fn playerHasFleetAtHomeworld(self: *const GameEngine, player_id: u64, homeworld: Hex) bool {
        var iter = self.fleets.iterator();
        while (iter.next()) |entry| {
            const f = entry.value_ptr;
            if (f.owner_id == player_id and f.location.eql(homeworld)) return true;
        }
        return false;
    }

    fn collectSalvage(self: *GameEngine, fleet: *Fleet) !void {
        const key = fleet.location.toKey();
        const ov = self.sector_overrides.getPtr(key) orelse return;
        const salvage = ov.salvage orelse return;

        const max_cargo = fleetCargoCapacity(fleet);
        var remaining = max_cargo - (fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium);
        if (remaining <= 0) return;

        var collected = Resources{};
        collected.metal = @min(salvage.metal, remaining);
        remaining -= collected.metal;
        collected.crystal = @min(salvage.crystal, remaining);
        remaining -= collected.crystal;
        collected.deuterium = @min(salvage.deuterium, remaining);

        fleet.cargo.metal += collected.metal;
        fleet.cargo.crystal += collected.crystal;
        fleet.cargo.deuterium += collected.deuterium;

        ov.salvage = null;
        ov.salvage_despawn_tick = null;
        try self.dirty_fleets.put(fleet.id, {});
        try self.dirty_sectors.put(key, {});

        try self.pending_events.append(self.allocator, .{
            .tick = self.current_tick,
            .kind = .{ .salvage_collected = .{
                .fleet_id = fleet.id,
                .resources = collected,
            } },
        });
    }

    fn generateLootDrops(self: *GameEngine, active_combat: *const Combat, player_fleets: []*Fleet) !void {
        const zone = shared.constants.Zone.fromDistance(active_combat.sector.distFromOrigin());

        var rng = std.Random.DefaultPrng.init(self.current_tick *% 0x517CC1B727220A95);
        const random = rng.random();

        const loot = scaling.rollLoot(zone, active_combat.npc_ship_class, active_combat.npc_ship_count, random);

        // Deduplicate player owners
        var owner_ids: [MAX_COMBAT_FLEETS]u64 = @splat(0);
        var owner_count: usize = 0;
        for (player_fleets) |pf| {
            var found = false;
            for (owner_ids[0..owner_count]) |oid| {
                if (oid == pf.owner_id) {
                    found = true;
                    break;
                }
            }
            if (!found and owner_count < MAX_COMBAT_FLEETS) {
                owner_ids[owner_count] = pf.owner_id;
                owner_count += 1;
            }
        }

        // Award loot to each participating player
        for (owner_ids[0..owner_count]) |pid| {
            const player = self.players.getPtr(pid) orelse continue;

            // Fragments
            if (loot.fragment_type) |ft| {
                player.fragments.add(ft, loot.fragment_count);
                try self.pending_events.append(self.allocator, .{
                    .tick = self.current_tick,
                    .kind = .{ .loot_acquired = .{
                        .player_id = pid,
                        .loot_type = .{ .data_fragment = .{
                            .fragment_type = ft,
                            .count = loot.fragment_count,
                        } },
                    } },
                });
            }

            // Component
            if (loot.component) |comp| {
                const current = player.components.get(comp.component_type);
                if (current < scaling.MAX_COMPONENT_LEVEL) {
                    player.components.set(comp.component_type, current + 1);
                    try self.pending_events.append(self.allocator, .{
                        .tick = self.current_tick,
                        .kind = .{ .loot_acquired = .{
                            .player_id = pid,
                            .loot_type = .{ .component = .{
                                .component_type = comp.component_type,
                                .rarity = comp.rarity,
                            } },
                        } },
                    });
                }
            }

            try self.dirty_players.put(pid, {});
        }
    }

    fn dropSalvage(self: *GameEngine, sector: Hex, fleet_value: Resources) !void {
        const key = sector.toKey();
        const ov = try self.ensureOverride(key);
        ov.salvage = fleet_value.scale(shared.constants.SALVAGE_FRACTION);
        ov.salvage_despawn_tick = self.current_tick + shared.constants.SALVAGE_DESPAWN_TICKS;
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
            var p = player;
            p.buildings = try self.db.loadBuildings(player.id);
            p.research = try self.db.loadResearch(player.id);
            p.components = try self.db.loadComponents(player.id);
            p.fragments = try self.db.loadFragments(player.id);
            const queues = try self.db.loadBuildQueues(player.id);
            p.building_queue = queues.building;
            p.ship_queue = queues.ship;
            p.research_queue = queues.research;
            try self.players.put(p.id, p);
        }

        var fleets = try self.db.loadFleets();
        defer fleets.deinit(self.allocator);
        for (fleets.items) |fleet| {
            try self.fleets.put(fleet.id, fleet);
        }

        // Load docked ships for each player
        var dock_iter = self.players.iterator();
        while (dock_iter.next()) |entry| {
            const docked = try self.db.loadDockedShips(entry.key_ptr.*);
            entry.value_ptr.docked_ship_count = docked.count;
            if (docked.count > 0) {
                @memcpy(entry.value_ptr.docked_ships[0..docked.count], docked.ships[0..docked.count]);
            }
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
                try self.db.saveBuildings(player_id, player.buildings);
                try self.db.saveResearch(player_id, player.research);
                try self.db.saveComponents(player_id, player.components);
                try self.db.saveFragments(player_id, player.fragments);
                try self.db.saveBuildQueue(player_id, player);
                try self.db.saveDockedShips(player_id, player.docked_ships[0..player.docked_ship_count]);
            }
        }

        var fleet_iter = self.dirty_fleets.iterator();
        while (fleet_iter.next()) |entry| {
            const fid = entry.key_ptr.*;
            if (self.fleets.get(fid)) |fleet| {
                try self.db.saveFleet(fleet);
            }
        }

        var del_iter = self.deleted_fleet_ids.iterator();
        while (del_iter.next()) |entry| {
            try self.db.deleteFleet(entry.key_ptr.*);
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
        self.deleted_fleet_ids.clearRetainingCapacity();
    }

    fn recalculatePlayerFleetFuel(self: *GameEngine, player: *const Player) !void {
        var iter = self.fleets.iterator();
        while (iter.next()) |entry| {
            var fleet = entry.value_ptr;
            if (fleet.owner_id != player.id) continue;
            if (fleet.ship_count == 0) continue;

            const new_max = fleetFuelMax(fleet, player);
            if (new_max > fleet.fuel_max) {
                const bonus = new_max - fleet.fuel_max;
                fleet.fuel += bonus;
                fleet.fuel_max = new_max;
                try self.dirty_fleets.put(fleet.id, {});
            }
        }
    }

    pub fn getSensorRevealedCoords(self: *GameEngine, origin: Hex, max_hops: u8, alloc: std.mem.Allocator) ![]const Hex {
        if (max_hops == 0) return &.{};

        var visited = std.AutoHashMap(u32, void).init(alloc);
        defer visited.deinit();

        var current_frontier: std.ArrayList(Hex) = .empty;
        defer current_frontier.deinit(alloc);
        var next_frontier: std.ArrayList(Hex) = .empty;
        defer next_frontier.deinit(alloc);

        var result: std.ArrayList(Hex) = .empty;

        try visited.put(origin.toKey(), {});
        try current_frontier.append(alloc, origin);

        for (0..max_hops) |_| {
            for (current_frontier.items) |coord| {
                const neighbors = self.world_gen.connectedNeighbors(coord);
                for (neighbors.slice()) |n| {
                    const key = n.toKey();
                    if (visited.contains(key)) continue;
                    try visited.put(key, {});
                    try next_frontier.append(alloc, n);
                    try result.append(alloc, n);
                }
            }
            current_frontier.clearRetainingCapacity();
            std.mem.swap(std.ArrayList(Hex), &current_frontier, &next_frontier);
        }

        return try result.toOwnedSlice(alloc);
    }

    pub fn drainEvents(self: *GameEngine) []const shared.protocol.GameEvent {
        return self.pending_events.items;
    }
};

fn fleetMoveCooldown(fleet: *const Fleet, research: ?scaling.ResearchLevels) u16 {
    var min_speed: u8 = 255;
    for (fleet.ships[0..fleet.ship_count]) |ship| {
        if (ship.speed < min_speed) min_speed = ship.speed;
    }
    if (min_speed == 0) return shared.constants.MOVE_BASE_COOLDOWN;
    const base: u16 = @intCast(@as(u32, shared.constants.MOVE_BASE_COOLDOWN) * 10 / @as(u32, min_speed));
    if (research) |r| {
        const reduction = scaling.navigationCooldownReduction(r.navigation);
        return base -| reduction;
    }
    return base;
}

fn fleetFuelCost(fleet: *const Fleet, research: ?scaling.ResearchLevels) f32 {
    var total_mass: f32 = 0;
    for (fleet.ships[0..fleet.ship_count]) |ship| {
        total_mass += ship.hull_max;
    }
    const base = total_mass * shared.constants.FUEL_RATE_PER_MASS;
    if (research) |r| {
        return base * scaling.fuelRateModifier(r.fuel_efficiency);
    }
    return base;
}

fn fleetHarvestPower(fleet: *const Fleet, research: ?scaling.ResearchLevels) f32 {
    var power: f32 = 0;
    for (fleet.ships[0..fleet.ship_count]) |ship| {
        power += switch (ship.class) {
            .hauler => 5.0,
            .scout => 1.0,
            else => 0.5,
        };
    }
    if (research) |r| {
        return power * scaling.harvestRateModifier(r.harvesting_efficiency);
    }
    return power;
}

fn fleetFuelMax(fleet: *const Fleet, player: *const Player) f32 {
    var total_fuel: f32 = 0;
    for (fleet.ships[0..fleet.ship_count]) |ship| {
        total_fuel += @floatFromInt(ship.class.baseStats().fuel);
    }
    return total_fuel * scaling.fuelCapacityModifier(player.research.extended_fuel_tanks) * scaling.fuelDepotModifier(player.buildings.fuel_depot);
}

fn removeShipFromFleet(fleet: *Fleet, ship_id: u64) ?Ship {
    for (0..fleet.ship_count) |i| {
        if (fleet.ships[i].id == ship_id) {
            const ship = fleet.ships[i];
            fleet.ships[i] = fleet.ships[fleet.ship_count - 1];
            fleet.ship_count -= 1;
            return ship;
        }
    }
    return null;
}

fn removeDockedShip(player: *Player, ship_id: u64) ?Ship {
    for (0..player.docked_ship_count) |i| {
        if (player.docked_ships[i].id == ship_id) {
            const ship = player.docked_ships[i];
            player.docked_ships[i] = player.docked_ships[player.docked_ship_count - 1];
            player.docked_ship_count -= 1;
            return ship;
        }
    }
    return null;
}

fn npcFleetValue(npc: *const NpcFleet) Resources {
    var total = Resources{};
    for (npc.ships[0..npc.ship_count]) |ship| {
        total = total.add(ship.class.buildCost());
    }
    return total;
}

fn fleetCargoCapacity(fleet: *const Fleet) f32 {
    var cap: f32 = 0;
    for (fleet.ships[0..fleet.ship_count]) |ship| {
        const stats = ship.class.baseStats();
        cap += @floatFromInt(stats.cargo);
    }
    return cap;
}

fn normalizeName(raw: []const u8, buf: *[shared.constants.PLAYER_NAME_MAX_LEN]u8) ?[]const u8 {
    if (raw.len < shared.constants.PLAYER_NAME_MIN_LEN or raw.len > shared.constants.PLAYER_NAME_MAX_LEN) return null;
    for (raw, 0..) |c, i| {
        buf[i] = std.ascii.toLower(c);
    }
    return buf[0..raw.len];
}

fn validatePlayerName(name: []const u8) !void {
    for (name) |c| {
        const valid = (c >= 'a' and c <= 'z') or (c >= '0' and c <= '9') or c == '_' or c == '-';
        if (!valid) return error.NameInvalid;
    }

    if (name[0] == '-' or name[0] == '_') return error.NameInvalid;
    if (name[name.len - 1] == '-' or name[name.len - 1] == '_') return error.NameInvalid;

    const reserved = [_][]const u8{
        "admin", "administrator", "server", "system", "moderator",
        "mod",   "npc",           "mlm",    "gm",     "gamemaster",
    };
    for (reserved) |r| {
        if (std.mem.eql(u8, name, r)) return error.NameInvalid;
    }
}

fn generateToken() [shared.constants.TOKEN_HEX_LEN]u8 {
    var raw_bytes: [shared.constants.TOKEN_BYTES]u8 = undefined;
    std.crypto.random.bytes(&raw_bytes);
    return std.fmt.bytesToHex(raw_bytes, .lower);
}

fn hashToken(token: []const u8, out: *[shared.constants.HASH_BYTES]u8) void {
    var hasher = std.crypto.hash.sha2.Sha256.init(.{});
    hasher.update(token);
    hasher.final(out);
}

fn constantTimeEql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var diff: u8 = 0;
    for (a, b) |x, y| {
        diff |= x ^ y;
    }
    return diff == 0;
}

fn currentTimestamp() u64 {
    return @intCast(std.time.timestamp());
}

pub const MAX_SHIPS_PER_FLEET: usize = 64;
pub const MAX_NPC_SHIPS: usize = 32;

pub const Player = struct {
    id: u64,
    name: []const u8,
    resources: Resources,
    homeworld: Hex,
    buildings: scaling.BuildingLevels = .{},
    research: scaling.ResearchLevels = .{},
    components: scaling.ComponentLevels = .{},
    fragments: scaling.FragmentCounts = .{},
    building_queue: ?BuildQueueEntry = null,
    ship_queue: ?ShipQueueEntry = null,
    research_queue: ?ResearchQueueEntry = null,
    token_hash: ?[]const u8 = null,
    created_at: u64 = 0,
    last_login_at: u64 = 0,
    docked_ships: [MAX_SHIPS_PER_FLEET]Ship = undefined,
    docked_ship_count: usize = 0,
};

pub const BuildQueueEntry = struct {
    building: BuildingType,
    target_level: u8,
    start_tick: u64,
    end_tick: u64,
};

pub const ShipQueueEntry = struct {
    ship_class: ShipClass,
    count: u16,
    built: u16 = 0,
    start_tick: u64,
    end_tick: u64,
};

pub const ResearchQueueEntry = struct {
    tech: ResearchType,
    target_level: u8,
    start_tick: u64,
    end_tick: u64,
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

pub const MAX_COMBAT_FLEETS: usize = 8;

pub const Combat = struct {
    id: u64,
    sector: Hex,
    player_fleet_ids: [MAX_COMBAT_FLEETS]u64 = @splat(0),
    player_fleet_count: u8 = 0,
    npc_fleet_ids: [MAX_COMBAT_FLEETS]u64 = @splat(0),
    npc_fleet_count: u8 = 0,
    npc_value: Resources,
    npc_ship_class: ShipClass = .scout,
    npc_ship_count: u8 = 0,
    round: u16,

    pub fn addPlayerFleet(self: *Combat, fleet_id: u64) void {
        if (self.hasPlayerFleet(fleet_id)) return;
        if (self.player_fleet_count >= MAX_COMBAT_FLEETS) return;
        self.player_fleet_ids[self.player_fleet_count] = fleet_id;
        self.player_fleet_count += 1;
    }

    pub fn addNpcFleet(self: *Combat, fleet_id: u64) void {
        if (self.hasNpcFleet(fleet_id)) return;
        if (self.npc_fleet_count >= MAX_COMBAT_FLEETS) return;
        self.npc_fleet_ids[self.npc_fleet_count] = fleet_id;
        self.npc_fleet_count += 1;
    }

    pub fn hasPlayerFleet(self: *const Combat, fleet_id: u64) bool {
        for (self.player_fleet_ids[0..self.player_fleet_count]) |id| {
            if (id == fleet_id) return true;
        }
        return false;
    }

    pub fn hasNpcFleet(self: *const Combat, fleet_id: u64) bool {
        for (self.npc_fleet_ids[0..self.npc_fleet_count]) |id| {
            if (id == fleet_id) return true;
        }
        return false;
    }
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
