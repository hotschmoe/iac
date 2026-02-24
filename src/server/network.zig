// src/server/network.zig
// WebSocket server for client connections.
// Handles both TUI and LLM/CLI clients over the same protocol.
// All messages are JSON-serialized shared.protocol types.

const std = @import("std");
const shared = @import("shared");
const engine_mod = @import("engine.zig");

const GameEngine = engine_mod.GameEngine;

const log = std.log.scoped(.network);

pub const Network = struct {
    allocator: std.mem.Allocator,
    port: u16,
    engine: *GameEngine,
    sessions: std.AutoHashMap(u64, ClientSession),
    next_session_id: u64,

    pub fn init(allocator: std.mem.Allocator, port: u16, _engine: *GameEngine) !Network {
        return .{
            .allocator = allocator,
            .port = port,
            .engine = _engine,
            .sessions = std.AutoHashMap(u64, ClientSession).init(allocator),
            .next_session_id = 1,
        };
    }

    pub fn deinit(self: *Network) void {
        self.sessions.deinit();
    }

    /// Process all incoming messages from connected clients.
    /// Called once per tick.
    pub fn processIncoming(self: *Network) !void {
        _ = self;
        // TODO: WebSocket integration
        //
        // For each connected client:
        //   1. Read any pending WebSocket frames
        //   2. Deserialize JSON to shared.protocol.ClientMessage
        //   3. Route to appropriate engine handler:
        //      - auth → engine.registerPlayer() or reconnect
        //      - command.move → engine.handleMove()
        //      - command.harvest → engine.handleHarvest()
        //      - command.attack → engine.handleAttack()
        //      - command.recall → engine.handleRecall()
        //      - policy_update → engine.updatePolicy()
        //      - request_full_state → queue full state sync
        //   4. On errors, send ErrorMessage back to client
    }

    /// Broadcast tick updates to all connected clients.
    /// Called once per tick after engine.tick().
    pub fn broadcastUpdates(self: *Network, eng: *GameEngine) !void {
        _ = self;
        const events = eng.drainEvents();
        _ = events;

        // TODO: For each connected session:
        //
        // 1. Build a TickUpdate message with:
        //    - Current tick number
        //    - Fleet state updates (for fleets this player owns)
        //    - Sector state (for the sector(s) this player's fleets are in)
        //    - Homeworld state updates
        //    - Filtered events (only events relevant to this player)
        //
        // 2. Serialize to JSON
        // 3. Send over WebSocket
        //
        // Optimization: only send deltas (fields that changed since last tick).
        // Full state sync available on request_full_state.
    }

    /// Handle a new WebSocket connection.
    fn onConnect(self: *Network) !u64 {
        const session_id = self.next_session_id;
        self.next_session_id += 1;

        try self.sessions.put(session_id, .{
            .id = session_id,
            .player_id = null,
            .authenticated = false,
            .client_type = .unknown,
            .last_active_tick = self.engine.currentTick(),
        });

        log.info("Client connected (session {d})", .{session_id});
        return session_id;
    }

    /// Handle a WebSocket disconnection.
    fn onDisconnect(self: *Network, session_id: u64) void {
        if (self.sessions.get(session_id)) |session| {
            log.info("Client disconnected (session {d}, player {?})", .{
                session_id,
                session.player_id,
            });
        }
        _ = self.sessions.remove(session_id);
    }

    /// Route a deserialized client message to the appropriate handler.
    fn handleMessage(self: *Network, session_id: u64, msg: shared.protocol.ClientMessage) !void {
        const session = self.sessions.getPtr(session_id) orelse return;

        switch (msg) {
            .auth => |auth| {
                if (session.authenticated) {
                    try self.sendError(session_id, .already_authenticated, "Already authenticated");
                    return;
                }
                // Register or reconnect
                const player_id = try self.engine.registerPlayer(auth.player_name);
                session.player_id = player_id;
                session.authenticated = true;
                session.client_type = if (auth.token != null) .llm_agent else .tui_human;

                // Send auth result + full state sync
                // TODO: serialize and send
            },
            .command => |cmd| {
                if (!session.authenticated) {
                    try self.sendError(session_id, .auth_failed, "Not authenticated");
                    return;
                }
                try self.routeCommand(session, cmd);
            },
            .policy_update => |_| {
                if (!session.authenticated) {
                    try self.sendError(session_id, .auth_failed, "Not authenticated");
                    return;
                }
                // TODO: update fleet policy
            },
            .request_full_state => {
                // TODO: build and send full GameState
            },
        }
    }

    fn routeCommand(self: *Network, session: *ClientSession, cmd: shared.protocol.Command) !void {
        _ = session;
        switch (cmd) {
            .move => |m| self.engine.handleMove(m.fleet_id, m.target) catch |err| {
                log.warn("Move failed: {}", .{err});
            },
            .harvest => |h| self.engine.handleHarvest(h.fleet_id) catch |err| {
                log.warn("Harvest failed: {}", .{err});
            },
            .recall => |r| self.engine.handleRecall(r.fleet_id) catch |err| {
                log.warn("Recall failed: {}", .{err});
            },
            .attack => |_| {
                // TODO: engine.handleAttack()
            },
            .stop => {
                // TODO: cancel current fleet action
            },
            .scan => {
                // TODO: engine.handleScan()
            },
        }
    }

    fn sendError(self: *Network, session_id: u64, code: shared.protocol.ErrorCode, message: []const u8) !void {
        _ = self;
        _ = session_id;
        _ = code;
        _ = message;
        // TODO: serialize ErrorMessage and send over WebSocket
    }
};

pub const ClientSession = struct {
    id: u64,
    player_id: ?u64,
    authenticated: bool,
    client_type: ClientType,
    last_active_tick: u64,
    // TODO: WebSocket connection handle
};

pub const ClientType = enum {
    unknown,
    tui_human,
    llm_agent,
    web_client,
    spectator,
};
