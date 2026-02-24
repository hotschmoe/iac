// src/client/renderer.zig
// Amber TUI renderer.
// Renders game state to the terminal using ANSI escape codes.
// All output uses a single amber hue at varying brightness levels.
//
// TODO: This will be replaced/augmented with zithril
// once those libraries mature. For now, raw ANSI is the bootstrap.

const std = @import("std");
const shared = @import("shared");
const State = @import("state.zig");

const ClientState = State.ClientState;
const View = State.View;
const Hex = shared.Hex;

pub const Renderer = struct {
    allocator: std.mem.Allocator,
    file: std.fs.File,
    term_width: u16,
    term_height: u16,

    pub fn init(allocator: std.mem.Allocator) !Renderer {
        const stdout = std.fs.File.stdout();

        // Enter alternate screen, hide cursor
        try stdout.writeAll("\x1b[?1049h"); // alternate screen
        try stdout.writeAll("\x1b[?25l"); // hide cursor

        // Query terminal size
        // TODO: use ioctl or SIGWINCH. Hardcode for now.
        const width: u16 = 120;
        const height: u16 = 40;

        return .{
            .allocator = allocator,
            .file = stdout,
            .term_width = width,
            .term_height = height,
        };
    }

    pub fn deinit(self: *Renderer) void {
        _ = self;
    }

    pub fn cleanup(self: *Renderer) !void {
        try self.file.writeAll("\x1b[?25h"); // show cursor
        try self.file.writeAll("\x1b[?1049l"); // exit alternate screen
        try self.file.writeAll("\x1b[0m"); // reset colors
    }

    // ── Output Helpers ──────────────────────────────────────────────

    fn out(self: *Renderer, bytes: []const u8) !void {
        try self.file.writeAll(bytes);
    }

    fn outFmt(self: *Renderer, comptime fmt: []const u8, args: anytype) !void {
        var buf: [1024]u8 = undefined;
        const slice = std.fmt.bufPrint(&buf, fmt, args) catch |err| switch (err) {
            error.NoSpaceLeft => {
                try self.file.writeAll("[fmt overflow]");
                return;
            },
        };
        try self.file.writeAll(slice);
    }

    fn outByte(self: *Renderer, byte: u8) !void {
        try self.file.writeAll(&.{byte});
    }

    /// Render the current game state.
    pub fn render(self: *Renderer, state: *const ClientState) !void {
        try self.out("\x1b[2J\x1b[H");
        try self.renderHeader(state);

        switch (state.current_view) {
            .command_center => try self.renderCommandCenter(state),
            .windshield => try self.renderWindshield(state),
            .star_map => try self.renderStarMap(state),
        }
    }

    // ── Header ─────────────────────────────────────────────────────

    fn renderHeader(self: *Renderer, state: *const ClientState) !void {
        try self.setAmber(.full);
        try self.out("=== IN AMBER CLAD v0.1.0 ");
        try self.setAmber(.dim);

        try self.outFmt("| TICK: {d} | ", .{state.tick});

        if (state.player) |player| {
            try self.outFmt("{s} | ", .{player.name});
        }

        const view_label: []const u8 = switch (state.current_view) {
            .command_center => "COMMAND CENTER",
            .windshield => "WINDSHIELD",
            .star_map => "STAR MAP",
        };
        try self.setAmber(.bright);
        try self.outFmt("{s}", .{view_label});

        try self.setAmber(.dim);
        try self.out("\n");
        try self.repeatChar('-', self.term_width);
        try self.out("\n");
    }

    // ── Command Center View ────────────────────────────────────────

    fn renderCommandCenter(self: *Renderer, state: *const ClientState) !void {
        try self.renderPanel("RESOURCES", 0, 2, 40, 6);
        if (state.player) |player| {
            try self.moveTo(2, 3);
            try self.setAmber(.dim);
            try self.out("Metal   ");
            try self.setAmber(.full);
            try self.outFmt("{d:>8.0}", .{player.resources.metal});

            try self.moveTo(2, 4);
            try self.setAmber(.dim);
            try self.out("Crystal ");
            try self.setAmber(.full);
            try self.outFmt("{d:>8.0}", .{player.resources.crystal});

            try self.moveTo(2, 5);
            try self.setAmber(.dim);
            try self.out("Deut    ");
            try self.setAmber(.full);
            try self.outFmt("{d:>8.0}", .{player.resources.deuterium});
        }

        try self.renderPanel("FLEETS", 42, 2, 40, 6);
        for (state.fleets.items, 0..) |fleet, i| {
            try self.moveTo(44, @intCast(3 + i));
            try self.setAmber(.bright);
            try self.outFmt("Fleet {d}", .{fleet.id});
            try self.setAmber(.dim);
            try self.outFmt("  {any} ", .{fleet.location});
            try self.setAmber(switch (fleet.state) {
                .idle => .dim,
                .in_combat => .full,
                .harvesting => .normal,
                else => .normal,
            });
            try self.outFmt("{s}", .{@tagName(fleet.state)});
        }

        try self.renderPanel("EVENT LOG", 0, 10, 82, 8);

        try self.renderPanel("COMMAND", 0, 19, 82, 4);
        try self.moveTo(2, 20);
        try self.setAmber(.dim);
        try self.out("[m]ap  [w]indshield  [f]leet  [b]uild  [r]esearch  [q]uit");
        try self.moveTo(2, 21);
        try self.setAmber(.full);
        try self.out("> ");
        try self.setAmber(.bright);
        try self.out("_");
    }

    // ── Windshield View ────────────────────────────────────────────

    fn renderWindshield(self: *Renderer, state: *const ClientState) !void {
        const fleet = state.activeFleet() orelse {
            try self.setAmber(.dim);
            try self.out("  No active fleet. Press [esc] to return.");
            return;
        };

        const center_row: u16 = 12;
        const center_col: u16 = 30;

        try self.moveTo(center_col - 2, center_row);
        try self.setAmber(.full);
        try self.out("=[ * ]=");

        try self.moveTo(center_col - 1, center_row + 1);
        try self.setAmber(.bright);
        try self.outFmt("{any}", .{fleet.location});

        if (state.currentSector()) |sector| {
            if (sector.connections.len > 0) {
                try self.moveTo(center_col + 12, center_row - 2);
                try self.setAmber(.dim);
                try self.out("Exits:");
                for (sector.connections, 0..) |conn, i| {
                    try self.moveTo(center_col + 12, @intCast(center_row - 1 + i));
                    try self.setAmber(.normal);
                    try self.outFmt("  {any} ", .{conn});

                    if (state.known_sectors.get(conn.toKey())) |nbr| {
                        try self.setAmber(.dim);
                        try self.outFmt("{s}", .{nbr.terrain.label()});
                    } else {
                        try self.setAmber(.faint);
                        try self.out("unexplored");
                    }
                }
            }
        }

        try self.renderPanel("FLEET STATUS", 70, 2, 30, 10);
        try self.moveTo(72, 3);
        try self.setAmber(.dim);
        try self.out("Ships: ");
        try self.setAmber(.normal);
        try self.outFmt("{d}", .{fleet.ships.len});

        try self.moveTo(72, 4);
        try self.setAmber(.dim);
        try self.out("Fuel:  ");
        try self.setAmber(if (fleet.fuel / fleet.fuel_max < 0.25) .full else .normal);
        try self.outFmt("{d:.0}/{d:.0}", .{ fleet.fuel, fleet.fuel_max });

        try self.moveTo(72, 6);
        try self.setAmber(.dim);
        try self.out("Cargo:");
        try self.moveTo(72, 7);
        try self.outFmt(" Fe {d:.0}", .{fleet.cargo.metal});
        try self.moveTo(72, 8);
        try self.outFmt(" Cr {d:.0}", .{fleet.cargo.crystal});
        try self.moveTo(72, 9);
        try self.outFmt(" De {d:.0}", .{fleet.cargo.deuterium});

        try self.moveTo(0, self.term_height - 3);
        try self.setAmber(.dim);
        try self.out("  [1-6] move to exit  [h]arvest  [a]ttack  [r]ecall  [esc] command center");
    }

    // ── Star Map View ──────────────────────────────────────────────

    fn renderStarMap(self: *Renderer, state: *const ClientState) !void {
        _ = state;

        try self.setAmber(.dim);
        try self.out("  ZOOM: ");
        try self.out("[1]CLOSE  [2]SECTOR  [3]REGION");
        try self.out("  |  ARROWS: scroll  |  ENTER: waypoint  |  TAB: cycle fleet\n");

        try self.moveTo(10, 10);
        try self.setAmber(.dim);
        try self.out("[ Star Map renderer -- TODO ]");
        try self.moveTo(10, 12);
        try self.setAmber(.faint);
        try self.out("Node graph (close), hybrid hex (sector), dot map (region)");
    }

    // ── Drawing Primitives ─────────────────────────────────────────

    const AmberLevel = enum { faint, dim, normal, bright, full };

    fn setAmber(self: *Renderer, level: AmberLevel) !void {
        const rgb: struct { r: u8, g: u8, b: u8 } = switch (level) {
            .faint => .{ .r = 30, .g = 21, .b = 0 },
            .dim => .{ .r = 77, .g = 53, .b = 0 },
            .normal => .{ .r = 153, .g = 106, .b = 0 },
            .bright => .{ .r = 217, .g = 150, .b = 0 },
            .full => .{ .r = 255, .g = 176, .b = 0 },
        };
        try self.outFmt("\x1b[38;2;{d};{d};{d}m", .{ rgb.r, rgb.g, rgb.b });
    }

    fn moveTo(self: *Renderer, col: u16, row: u16) !void {
        try self.outFmt("\x1b[{d};{d}H", .{ row + 1, col + 1 });
    }

    fn repeatChar(self: *Renderer, char: u8, count: u16) !void {
        var i: u16 = 0;
        while (i < count) : (i += 1) {
            try self.outByte(char);
        }
    }

    fn renderPanel(self: *Renderer, title: []const u8, x: u16, y: u16, w: u16, h: u16) !void {
        try self.setAmber(.dim);

        try self.moveTo(x, y);
        try self.out("+");
        try self.setAmber(.dim);
        try self.out("- ");
        try self.setAmber(.normal);
        try self.outFmt("{s}", .{title});
        try self.setAmber(.dim);
        try self.out(" ");

        const title_len: u16 = @intCast(title.len + 4);
        if (w > title_len + 1) {
            try self.repeatChar('-', w - title_len - 1);
        }
        try self.out("+");

        var row: u16 = 1;
        while (row < h - 1) : (row += 1) {
            try self.moveTo(x, y + row);
            try self.out("|");
            try self.moveTo(x + w - 1, y + row);
            try self.out("|");
        }

        try self.moveTo(x, y + h - 1);
        try self.out("+");
        try self.repeatChar('-', w - 2);
        try self.out("+");
    }
};
