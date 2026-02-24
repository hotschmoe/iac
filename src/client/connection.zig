// src/client/connection.zig
// WebSocket client connection to the game server.
// Handles JSON serialization/deserialization of protocol messages.

const std = @import("std");
const shared = @import("shared");

const log = std.log.scoped(.connection);

pub const Connection = struct {
    allocator: std.mem.Allocator,
    host: []const u8,
    port: u16,
    connected: bool,
    // TODO: WebSocket handle from zig websocket library

    pub fn init(allocator: std.mem.Allocator, host: []const u8, port: u16) !Connection {
        var conn = Connection{
            .allocator = allocator,
            .host = host,
            .port = port,
            .connected = false,
        };

        try conn.connect();

        return conn;
    }

    pub fn deinit(self: *Connection) void {
        if (self.connected) {
            self.disconnect();
        }
    }

    fn connect(self: *Connection) !void {
        // TODO: Establish WebSocket connection
        //
        // 1. TCP connect to host:port
        // 2. WebSocket handshake
        // 3. Set connected = true
        //
        // Using zig WebSocket library:
        //   const ws = try websocket.connect(self.host, self.port, "/");
        //   self.ws_handle = ws;

        log.info("Connecting to {s}:{d}...", .{ self.host, self.port });
        self.connected = true;
        log.info("Connected.", .{});
    }

    fn disconnect(self: *Connection) void {
        // TODO: close WebSocket gracefully
        self.connected = false;
    }

    /// Send authentication request.
    pub fn sendAuth(self: *Connection, player_name: []const u8) !void {
        const msg = shared.protocol.ClientMessage{
            .auth = .{
                .player_name = player_name,
            },
        };
        try self.send(msg);
    }

    /// Send a game command.
    pub fn sendCommand(self: *Connection, cmd: shared.protocol.Command) !void {
        const msg = shared.protocol.ClientMessage{
            .command = cmd,
        };
        try self.send(msg);
    }

    /// Send a policy update.
    pub fn sendPolicyUpdate(self: *Connection, fleet_id: u64, rules: []const shared.protocol.PolicyRule) !void {
        const msg = shared.protocol.ClientMessage{
            .policy_update = .{
                .fleet_id = fleet_id,
                .rules = rules,
            },
        };
        try self.send(msg);
    }

    /// Request full state sync from server.
    pub fn requestFullState(self: *Connection) !void {
        const msg = shared.protocol.ClientMessage{
            .request_full_state = {},
        };
        try self.send(msg);
    }

    /// Non-blocking poll for server messages.
    /// Returns null if no message available.
    pub fn poll(self: *Connection) !?shared.protocol.ServerMessage {
        if (!self.connected) return null;

        // TODO: Read from WebSocket non-blocking
        //
        // 1. Check if data available (non-blocking read)
        // 2. If frame complete, deserialize JSON → ServerMessage
        // 3. Return parsed message
        //
        // const frame = try self.ws_handle.readNonBlocking();
        // if (frame) |f| {
        //     return try std.json.parseFromSlice(
        //         shared.protocol.ServerMessage,
        //         self.allocator,
        //         f.data,
        //         .{},
        //     );
        // }

        return null;
    }

    /// Serialize and send a client message as JSON over WebSocket.
    fn send(self: *Connection, msg: shared.protocol.ClientMessage) !void {
        if (!self.connected) return error.NotConnected;

        // TODO: JSON serialization and WebSocket send
        //
        // var buf = std.ArrayList(u8).init(self.allocator);
        // defer buf.deinit();
        // try std.json.stringify(msg, .{}, buf.writer());
        // try self.ws_handle.send(.text, buf.items);

        _ = msg;
    }
};
