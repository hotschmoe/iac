// Threading model:
//   webzocket Server runs in a background thread (listenInNewThread).
//   Handler.clientMessage pushes to a mutex-protected queue.
//   processIncoming() drains the queue on the main tick thread.
//   broadcastUpdates() sends to all sessions from the tick thread.

const std = @import("std");
const shared = @import("shared");
const engine_mod = @import("engine.zig");
const wz = @import("webzocket");

const GameEngine = engine_mod.GameEngine;
const protocol = shared.protocol;

const log = std.log.scoped(.network);

pub const Network = struct {
    allocator: std.mem.Allocator,
    port: u16,
    engine: *GameEngine,
    server: wz.Server(Handler),
    server_thread: ?std.Thread,

    mutex: std.Thread.Mutex,
    incoming_queue: std.ArrayList(QueuedMessage),
    sessions: std.AutoHashMap(u64, ClientSession),
    next_session_id: u64,

    pub fn init(allocator: std.mem.Allocator, port: u16, _engine: *GameEngine) !Network {
        return .{
            .allocator = allocator,
            .port = port,
            .engine = _engine,
            .server = try wz.Server(Handler).init(allocator, .{
                .port = port,
                .address = shared.constants.DEFAULT_HOST,
            }),
            .server_thread = null,
            .mutex = .{},
            .incoming_queue = std.ArrayList(QueuedMessage).empty,
            .sessions = std.AutoHashMap(u64, ClientSession).init(allocator),
            .next_session_id = 1,
        };
    }

    pub fn deinit(self: *Network) void {
        self.server.stop();
        if (self.server_thread) |t| t.join();
        self.server.deinit();
        self.incoming_queue.deinit(self.allocator);
        self.sessions.deinit();
    }

    pub fn startListening(self: *Network) !void {
        self.server_thread = try self.server.listenInNewThread(self);
        log.info("WebSocket server listening on {s}:{d}", .{ shared.constants.DEFAULT_HOST, self.port });
    }

    pub fn processIncoming(self: *Network) !void {
        var messages: std.ArrayList(QueuedMessage) = blk: {
            self.mutex.lock();
            defer self.mutex.unlock();
            const tmp = self.incoming_queue;
            self.incoming_queue = std.ArrayList(QueuedMessage).empty;
            break :blk tmp;
        };
        defer messages.deinit(self.allocator);

        for (messages.items) |queued| {
            defer queued.parsed.deinit();
            self.handleMessage(queued.session_id, queued.msg) catch |err| {
                log.warn("Error handling message from session {d}: {}", .{ queued.session_id, err });
            };
        }
    }

    pub fn broadcastUpdates(self: *Network, eng: *GameEngine) !void {
        const events = eng.drainEvents();

        self.mutex.lock();
        defer self.mutex.unlock();

        var iter = self.sessions.iterator();
        while (iter.next()) |entry| {
            const session = entry.value_ptr;
            if (!session.authenticated) continue;

            const player_id = session.player_id orelse continue;

            var arena = std.heap.ArenaAllocator.init(self.allocator);
            defer arena.deinit();
            const alloc = arena.allocator();

            const fleet_updates = try collectPlayerFleets(alloc, eng, player_id);

            var sector_update: ?protocol.SectorState = null;
            if (fleet_updates.len > 0) {
                sector_update = try buildSectorState(alloc, eng, fleet_updates[0].location);
            }

            var player_events = std.ArrayList(protocol.GameEvent).empty;
            for (events) |event| {
                if (isEventRelevant(event, player_id, eng)) {
                    try player_events.append(alloc, event);
                }
            }

            const update = protocol.ServerMessage{
                .tick_update = .{
                    .tick = eng.current_tick,
                    .fleet_updates = if (fleet_updates.len > 0) fleet_updates else null,
                    .sector_update = sector_update,
                    .events = if (player_events.items.len > 0) player_events.items else null,
                },
            };

            self.sendToSession(session, update) catch |err| {
                log.warn("Failed to send update to session {d}: {}", .{ session.id, err });
            };
        }
    }

    fn handleMessage(self: *Network, session_id: u64, msg: protocol.ClientMessage) !void {
        self.mutex.lock();
        const session = self.sessions.getPtr(session_id);
        self.mutex.unlock();

        const sess = session orelse return;

        switch (msg) {
            .auth => |auth| {
                if (sess.authenticated) {
                    try self.sendErrorToSession(sess, .already_authenticated, "Already authenticated");
                    return;
                }
                const player_id = try self.engine.registerPlayer(auth.player_name);
                self.mutex.lock();
                if (self.sessions.getPtr(session_id)) |s| {
                    s.player_id = player_id;
                    s.authenticated = true;
                    s.client_type = if (auth.token != null) .llm_agent else .tui_human;
                }
                self.mutex.unlock();

                const result = protocol.ServerMessage{
                    .auth_result = .{
                        .success = true,
                        .player_id = player_id,
                    },
                };
                try self.sendToSession(sess, result);

                try self.sendFullState(sess, player_id);
            },
            .command => |cmd| {
                if (!sess.authenticated) {
                    try self.sendErrorToSession(sess, .auth_failed, "Not authenticated");
                    return;
                }
                self.routeCommand(sess, cmd);
            },
            .policy_update => {},
            .request_full_state => {
                if (sess.authenticated) {
                    if (sess.player_id) |pid| {
                        try self.sendFullState(sess, pid);
                    }
                }
            },
        }
    }

    fn routeCommand(self: *Network, session: *const ClientSession, cmd: protocol.Command) void {
        switch (cmd) {
            .move => |m| self.engine.handleMove(m.fleet_id, m.target) catch |err| {
                self.sendCommandError(session, "Move", err);
                return;
            },
            .harvest => |h| self.engine.handleHarvest(h.fleet_id) catch |err| {
                self.sendCommandError(session, "Harvest", err);
                return;
            },
            .recall => |r| self.engine.handleRecall(r.fleet_id) catch |err| {
                self.sendCommandError(session, "Recall", err);
                return;
            },
            .attack => |_| {},
            .stop => {},
            .scan => {},
        }
    }

    fn sendCommandError(self: *Network, session: *const ClientSession, action: []const u8, err: anyerror) void {
        const code: protocol.ErrorCode = switch (err) {
            error.FleetNotFound => .fleet_not_found,
            error.NoConnection => .no_connection,
            error.InsufficientFuel => .insufficient_fuel,
            error.OnCooldown => .on_cooldown,
            error.NoResources => .no_resources,
            error.CargoFull => .cargo_full,
            error.InCombat => .on_cooldown,
            error.NoShips => .fleet_not_found,
            else => .invalid_command,
        };

        var buf: [128]u8 = undefined;
        const message = std.fmt.bufPrint(&buf, "{s} failed: {s}", .{ action, @errorName(err) }) catch "Command failed";

        self.sendErrorToSession(session, code, message) catch |send_err| {
            log.warn("{s} failed: {} (also failed to notify client: {})", .{ action, err, send_err });
        };
    }

    fn sendFullState(self: *Network, session: *const ClientSession, player_id: u64) !void {
        const player = self.engine.players.get(player_id) orelse return;

        var arena = std.heap.ArenaAllocator.init(self.allocator);
        defer arena.deinit();
        const alloc = arena.allocator();

        const fleet_states = try collectPlayerFleets(alloc, self.engine, player_id);

        var known_sectors = std.ArrayList(protocol.SectorState).empty;
        for (fleet_states) |fs| {
            try known_sectors.append(alloc, try buildSectorState(alloc, self.engine, fs.location));
        }

        const msg = protocol.ServerMessage{
            .full_state = .{
                .tick = self.engine.current_tick,
                .player = .{
                    .id = player.id,
                    .name = player.name,
                    .resources = player.resources,
                    .homeworld = player.homeworld,
                },
                .fleets = fleet_states,
                .homeworld = .{
                    .location = player.homeworld,
                    .buildings = &.{},
                    .docked_ships = &.{},
                },
                .known_sectors = known_sectors.items,
            },
        };

        try self.sendToSession(session, msg);
    }

    fn sendToSession(self: *Network, session: *const ClientSession, msg: protocol.ServerMessage) !void {
        const conn = session.conn orelse return;
        const json = try std.json.Stringify.valueAlloc(self.allocator, msg, .{});
        defer self.allocator.free(json);
        conn.writeText(json) catch |err| {
            log.warn("Write failed for session {d}: {}", .{ session.id, err });
            return err;
        };
    }

    fn sendErrorToSession(self: *Network, session: *const ClientSession, code: protocol.ErrorCode, message: []const u8) !void {
        try self.sendToSession(session, .{
            .@"error" = .{
                .code = code,
                .message = message,
            },
        });
    }

    fn collectPlayerFleets(alloc: std.mem.Allocator, eng: *GameEngine, player_id: u64) ![]const protocol.FleetState {
        var list = std.ArrayList(protocol.FleetState).empty;

        var iter = eng.fleets.iterator();
        while (iter.next()) |entry| {
            const fleet = entry.value_ptr;
            if (fleet.owner_id != player_id) continue;

            var ship_states = std.ArrayList(protocol.ShipState).empty;
            for (fleet.ships[0..fleet.ship_count]) |ship| {
                try ship_states.append(alloc, .{
                    .id = ship.id,
                    .class = ship.class,
                    .hull = ship.hull,
                    .hull_max = ship.hull_max,
                    .shield = ship.shield,
                    .shield_max = ship.shield_max,
                    .weapon_power = ship.weapon_power,
                });
            }

            try list.append(alloc, .{
                .id = fleet.id,
                .location = fleet.location,
                .state = @enumFromInt(@intFromEnum(fleet.state)),
                .ships = ship_states.items,
                .cargo = fleet.cargo,
                .fuel = fleet.fuel,
                .fuel_max = fleet.fuel_max,
                .cooldown_remaining = fleet.action_cooldown,
            });
        }

        return list.items;
    }

    fn buildSectorState(alloc: std.mem.Allocator, eng: *GameEngine, coord: shared.Hex) !protocol.SectorState {
        const template = eng.world_gen.generateSector(coord);
        const neighbors = eng.world_gen.connectedNeighbors(coord);
        const override = eng.sector_overrides.get(coord.toKey());

        // Collect hostile NPC fleets at this coord
        var hostile_list = std.ArrayList(protocol.NpcFleetInfo).empty;

        // Spawned NPC fleets (active in combat or otherwise present)
        var npc_iter = eng.npc_fleets.iterator();
        while (npc_iter.next()) |npc_entry| {
            const npc = npc_entry.value_ptr;
            if (npc.location.eql(coord)) {
                var ship_info = std.ArrayList(protocol.NpcShipInfo).empty;
                // Count ships by class
                var class_counts: [5]u16 = .{ 0, 0, 0, 0, 0 };
                for (npc.ships[0..npc.ship_count]) |ship| {
                    class_counts[@intFromEnum(ship.class)] += 1;
                }
                for (class_counts, 0..) |count, ci| {
                    if (count > 0) {
                        try ship_info.append(alloc, .{
                            .class = @enumFromInt(ci),
                            .count = count,
                        });
                    }
                }
                try hostile_list.append(alloc, .{
                    .id = npc.id,
                    .ships = ship_info.items,
                    .behavior = @enumFromInt(@intFromEnum(npc.behavior)),
                });
            }
        }

        // Template NPCs (not yet spawned but present in the world)
        if (template.npc_template) |npc_tmpl| {
            // Only show if no spawned NPC fleet already covers this sector
            if (hostile_list.items.len == 0) {
                var ship_info = try alloc.alloc(protocol.NpcShipInfo, 1);
                ship_info[0] = .{ .class = npc_tmpl.ship_class, .count = npc_tmpl.count };
                try hostile_list.append(alloc, .{
                    .id = 0,
                    .ships = ship_info,
                    .behavior = @enumFromInt(@intFromEnum(npc_tmpl.behavior)),
                });
            }
        }

        return .{
            .location = coord,
            .terrain = template.terrain,
            .resources = .{
                .metal = if (override) |o| o.metal_density orelse template.metal_density else template.metal_density,
                .crystal = if (override) |o| o.crystal_density orelse template.crystal_density else template.crystal_density,
                .deuterium = if (override) |o| o.deut_density orelse template.deut_density else template.deut_density,
            },
            .connections = try alloc.dupe(shared.Hex, neighbors.slice()),
            .hostiles = if (hostile_list.items.len > 0) hostile_list.items else null,
            .salvage = if (override) |o| o.salvage else null,
        };
    }

    fn isEventRelevant(_: protocol.GameEvent, _: u64, _: *GameEngine) bool {
        // M1: all events visible to all players
        return true;
    }
};

const Handler = struct {
    conn: *wz.Conn,
    network: *Network,
    session_id: u64,

    pub fn init(_: *wz.Handshake, conn: *wz.Conn, ctx: *Network) !Handler {
        ctx.mutex.lock();
        defer ctx.mutex.unlock();

        const session_id = ctx.next_session_id;
        ctx.next_session_id += 1;

        try ctx.sessions.put(session_id, .{
            .id = session_id,
            .conn = conn,
            .player_id = null,
            .authenticated = false,
            .client_type = .unknown,
            .last_active_tick = ctx.engine.currentTick(),
        });

        log.info("Client connected (session {d})", .{session_id});

        return .{
            .conn = conn,
            .network = ctx,
            .session_id = session_id,
        };
    }

    pub fn clientMessage(self: *Handler, data: []u8) !void {
        const parsed = std.json.parseFromSlice(
            protocol.ClientMessage,
            self.network.allocator,
            data,
            .{ .ignore_unknown_fields = true },
        ) catch |err| {
            log.warn("JSON parse error from session {d}: {}", .{ self.session_id, err });
            return;
        };

        self.network.mutex.lock();
        defer self.network.mutex.unlock();
        try self.network.incoming_queue.append(self.network.allocator, .{
            .session_id = self.session_id,
            .msg = parsed.value,
            .parsed = parsed,
        });
    }

    pub fn close(self: *Handler) void {
        self.network.mutex.lock();
        defer self.network.mutex.unlock();

        if (self.network.sessions.get(self.session_id)) |session| {
            log.info("Client disconnected (session {d}, player {?})", .{
                self.session_id,
                session.player_id,
            });
        }
        _ = self.network.sessions.remove(self.session_id);
    }
};

const QueuedMessage = struct {
    session_id: u64,
    msg: protocol.ClientMessage,
    parsed: std.json.Parsed(protocol.ClientMessage),
};

pub const ClientSession = struct {
    id: u64,
    conn: ?*wz.Conn,
    player_id: ?u64,
    authenticated: bool,
    client_type: ClientType,
    last_active_tick: u64,
};

pub const ClientType = enum {
    unknown,
    tui_human,
    llm_agent,
    web_client,
    spectator,
};
