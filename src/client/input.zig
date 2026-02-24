// src/client/input.zig
// Terminal input handler.
// Puts terminal in raw mode, reads keypresses, maps to game actions.

const std = @import("std");
const shared = @import("shared");
const State = @import("state.zig");

pub const InputHandler = struct {
    stdin: std.fs.File,
    original_termios: ?std.posix.termios,

    pub fn init() !InputHandler {
        const stdin = std.fs.File.stdin();

        // Enter raw mode
        var raw = try std.posix.tcgetattr(stdin.handle);
        const original = raw;

        // Disable canonical mode, echo, signals
        raw.lflag.ICANON = false;
        raw.lflag.ECHO = false;
        raw.lflag.ISIG = false;

        // Disable input processing
        raw.iflag.IXON = false;
        raw.iflag.ICRNL = false;

        // Read returns immediately with whatever is available
        raw.cc[@intFromEnum(std.posix.system.V.MIN)] = 0;
        raw.cc[@intFromEnum(std.posix.system.V.TIME)] = 0;

        try std.posix.tcsetattr(stdin.handle, .FLUSH, raw);

        return .{
            .stdin = stdin,
            .original_termios = original,
        };
    }

    pub fn deinit(self: *InputHandler) void {
        // Restore original terminal settings
        if (self.original_termios) |orig| {
            std.posix.tcsetattr(self.stdin.handle, .FLUSH, orig) catch {};
        }
    }

    /// Non-blocking poll for input. Returns null if no input available.
    pub fn poll(self: *InputHandler) !?InputAction {
        var buf: [8]u8 = undefined;
        const n = self.stdin.read(&buf) catch |err| {
            if (err == error.WouldBlock) return null;
            return err;
        };
        if (n == 0) return null;

        return self.mapKey(buf[0..n]);
    }

    fn mapKey(self: *InputHandler, key: []const u8) ?InputAction {
        _ = self;

        if (key.len == 1) {
            return switch (key[0]) {
                'q' => .quit,

                // View switching
                '`', 27 => .{ .switch_view = .command_center }, // ESC
                'w' => .{ .switch_view = .windshield },
                'm' => .{ .switch_view = .star_map },

                // Windshield movement (hex directions)
                '1' => .{ .command = .{ .move = .{
                    .fleet_id = 0, // TODO: active fleet
                    .target = .{ .q = 0, .r = -1 }, // NW - placeholder
                } } },
                '2' => .{ .command = .{ .move = .{
                    .fleet_id = 0,
                    .target = .{ .q = 1, .r = -1 }, // NE
                } } },
                '3' => .{ .command = .{ .move = .{
                    .fleet_id = 0,
                    .target = .{ .q = 1, .r = 0 }, // E
                } } },
                '4' => .{ .command = .{ .move = .{
                    .fleet_id = 0,
                    .target = .{ .q = 0, .r = 1 }, // SE
                } } },
                '5' => .{ .command = .{ .move = .{
                    .fleet_id = 0,
                    .target = .{ .q = -1, .r = 1 }, // SW
                } } },

                // Actions
                'h' => .{ .command = .{ .harvest = .{
                    .fleet_id = 0,
                    .resource = .auto,
                } } },
                'r' => .{ .command = .{ .recall = .{
                    .fleet_id = 0,
                } } },

                // Star map zoom
                '[' => .{ .zoom = .close },
                ']' => .{ .zoom = .sector },
                '\\' => .{ .zoom = .region },

                // Fleet cycling
                '\t' => .cycle_fleet,

                else => null,
            };
        }

        // Arrow keys (escape sequences)
        if (key.len == 3 and key[0] == 27 and key[1] == '[') {
            return switch (key[2]) {
                'A' => .{ .scroll = .up },
                'B' => .{ .scroll = .down },
                'C' => .{ .scroll = .right },
                'D' => .{ .scroll = .left },
                else => null,
            };
        }

        return null;
    }
};

pub const InputAction = union(enum) {
    quit: void,
    switch_view: State.View,
    command: shared.protocol.Command,
    scroll: State.ScrollDirection,
    zoom: State.ZoomLevel,
    cycle_fleet: void,
};
