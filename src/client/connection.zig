const std = @import("std");
const shared = @import("shared");
const wz = @import("webzocket");

const log = std.log.scoped(.connection);

pub const Connection = struct {
    allocator: std.mem.Allocator,
    client: wz.Client,
    connected: bool,

    pub fn init(allocator: std.mem.Allocator, host: []const u8, port: u16) !Connection {
        var client = try wz.Client.init(allocator, .{
            .host = host,
            .port = port,
        });
        try client.handshake("/", .{});
        try client.readTimeout(1);

        log.info("Connected to {s}:{d}", .{ host, port });

        return .{
            .allocator = allocator,
            .client = client,
            .connected = true,
        };
    }

    pub fn deinit(self: *Connection) void {
        if (self.connected) {
            self.client.close(.{}) catch {};
            self.connected = false;
        }
        self.client.deinit();
    }

    pub fn sendAuth(self: *Connection, player_name: []const u8) !void {
        const msg = shared.protocol.ClientMessage{
            .auth = .{
                .player_name = player_name,
            },
        };
        try self.send(msg);
    }

    pub fn sendCommand(self: *Connection, cmd: shared.protocol.Command) !void {
        const msg = shared.protocol.ClientMessage{
            .command = cmd,
        };
        try self.send(msg);
    }

    pub fn requestFullState(self: *Connection) !void {
        const msg = shared.protocol.ClientMessage{
            .request_full_state = {},
        };
        try self.send(msg);
    }

    pub fn poll(self: *Connection) !?shared.protocol.ServerMessage {
        if (!self.connected) return null;

        const msg = self.client.read() catch |err| {
            if (err == error.WouldBlock) return null;
            log.warn("Read error: {any}", .{err});
            self.connected = false;
            return null;
        };

        if (msg) |m| {
            defer self.client.done(m);
            const parsed = std.json.parseFromSlice(
                shared.protocol.ServerMessage,
                self.allocator,
                m.data,
                .{ .ignore_unknown_fields = true },
            ) catch |err| {
                log.warn("JSON parse error: {any}", .{err});
                return null;
            };
            return parsed.value;
        }

        return null;
    }

    fn send(self: *Connection, msg: shared.protocol.ClientMessage) !void {
        if (!self.connected) return error.NotConnected;

        const json = try std.json.Stringify.valueAlloc(self.allocator, msg, .{});
        defer self.allocator.free(json);

        // webzocket client write needs mutable slice for masking
        const buf = try self.allocator.dupe(u8, json);
        defer self.allocator.free(buf);
        try self.client.write(buf);
    }
};
