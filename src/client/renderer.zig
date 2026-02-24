// src/client/renderer.zig
// Amber TUI renderer.
// Renders game state to the terminal using ANSI escape codes.
// All output uses a single amber hue at varying brightness levels.
//
// TODO: This will be replaced/augmented with zithril + rich_zig
// once those libraries mature. For now, raw ANSI is the bootstrap.

const std = @import("std");
const shared = @import("shared");
const State = @import("state.zig");

const ClientState = State.ClientState;
const View = State.View;
const Hex = shared.Hex;

pub const Renderer = struct {
    allocator: std.mem.Allocator,
    writer: std.io.AnyWriter,
    term_width: u16,
    term_height: u16,
    // Screen buffer for double-buffering (optional optimization)
    // buffer: []Cell,

    pub fn init(allocator: std.mem.Allocator) !Renderer {
        const stdout = std.io.getStdOut();

        // Enter alternate screen, hide cursor
        try stdout.writeAll("\x1b[?1049h"); // alternate screen
        try stdout.writeAll("\x1b[?25l"); // hide cursor

        // Query terminal size
        // TODO: use ioctl or SIGWINCH. Hardcode for now.
        const width: u16 = 120;
        const height: u16 = 40;

        return .{
            .allocator = allocator,
            .writer = stdout.writer().any(),
            .term_width = width,
            .term_height = height,
        };
    }

    pub fn deinit(self: *Renderer) void {
        _ = self;
    }

    pub fn cleanup(self: *Renderer) !void {
        // Restore terminal
        try self.writer.writeAll("\x1b[?25h"); // show cursor
        try self.writer.writeAll("\x1b[?1049l"); // exit alternate screen
        try self.writer.writeAll("\x1b[0m"); // reset colors
    }

    /// Render the current game state.
    pub fn render(self: *Renderer, state: *const ClientState) !void {
        // Clear screen
        try self.writer.writeAll("\x1b[2J\x1b[H");

        // Render header
        try self.renderHeader(state);

        // Render active view
        switch (state.current_view) {
            .command_center => try self.renderCommandCenter(state),
            .windshield => try self.renderWindshield(state),
            .star_map => try self.renderStarMap(state),
        }
    }

    // ── Header ─────────────────────────────────────────────────────

    fn renderHeader(self: *Renderer, state: *const ClientState) !void {
        try self.setAmber(.full);
        try self.writer.writeAll("═══ IN AMBER CLAD v0.1.0 ");
        try self.setAmber(.dim);

        // Right-align tick counter
        try self.writer.print("│ TICK: {d} │ ", .{state.tick});

        if (state.player) |player| {
            try self.writer.print("{s} │ ", .{player.name});
        }

        const view_label: []const u8 = switch (state.current_view) {
            .command_center => "COMMAND CENTER",
            .windshield => "WINDSHIELD",
            .star_map => "STAR MAP",
        };
        try self.setAmber(.bright);
        try self.writer.print("{s}", .{view_label});

        try self.setAmber(.dim);
        try self.writer.writeAll("\n");
        try self.repeatChar('─', self.term_width);
        try self.writer.writeAll("\n");
    }

    // ── Command Center View ────────────────────────────────────────

    fn renderCommandCenter(self: *Renderer, state: *const ClientState) !void {
        // Resources panel
        try self.renderPanel("RESOURCES", 0, 2, 40, 6);
        if (state.player) |player| {
            try self.moveTo(2, 3);
            try self.setAmber(.dim);
            try self.writer.writeAll("Metal   ");
            try self.setAmber(.full);
            try self.writer.print("{d:>8.0}", .{player.resources.metal});

            try self.moveTo(2, 4);
            try self.setAmber(.dim);
            try self.writer.writeAll("Crystal ");
            try self.setAmber(.full);
            try self.writer.print("{d:>8.0}", .{player.resources.crystal});

            try self.moveTo(2, 5);
            try self.setAmber(.dim);
            try self.writer.writeAll("Deut    ");
            try self.setAmber(.full);
            try self.writer.print("{d:>8.0}", .{player.resources.deuterium});
        }

        // Fleet summary panel
        try self.renderPanel("FLEETS", 42, 2, 40, 6);
        for (state.fleets.items, 0..) |fleet, i| {
            try self.moveTo(44, @intCast(3 + i));
            try self.setAmber(.bright);
            try self.writer.print("Fleet {d}", .{fleet.id});
            try self.setAmber(.dim);
            try self.writer.print("  {} ", .{fleet.location});
            try self.setAmber(switch (fleet.state) {
                .idle => .dim,
                .in_combat => .full,
                .harvesting => .normal,
                else => .normal,
            });
            try self.writer.print("{s}", .{@tagName(fleet.state)});
        }

        // Event log at bottom
        try self.renderPanel("EVENT LOG", 0, 10, 82, 8);
        // TODO: render recent events from state.event_log

        // Command input
        try self.renderPanel("COMMAND", 0, 19, 82, 4);
        try self.moveTo(2, 20);
        try self.setAmber(.dim);
        try self.writer.writeAll("[m]ap  [w]indshield  [f]leet  [b]uild  [r]esearch  [q]uit");
        try self.moveTo(2, 21);
        try self.setAmber(.full);
        try self.writer.writeAll("▸ ");
        try self.setAmber(.bright);
        try self.writer.writeAll("_");
    }

    // ── Windshield View ────────────────────────────────────────────

    fn renderWindshield(self: *Renderer, state: *const ClientState) !void {
        const fleet = state.activeFleet() orelse {
            try self.setAmber(.dim);
            try self.writer.writeAll("  No active fleet. Press [esc] to return.");
            return;
        };

        // Center: current sector node
        const center_row: u16 = 12;
        const center_col: u16 = 30;

        // Draw current position (player node)
        try self.moveTo(center_col - 2, center_row);
        try self.setAmber(.full);
        try self.writer.writeAll("═[ ◆ ]═");

        try self.moveTo(center_col - 1, center_row + 1);
        try self.setAmber(.bright);
        try self.writer.print("{}", .{fleet.location});

        // Draw connections to neighbors
        if (state.currentSector()) |sector| {
            if (sector.connections.len > 0) {
                // TODO: Position each neighbor node around the center
                // using the direction to determine screen placement.
                // For now, list connections.
                try self.moveTo(center_col + 12, center_row - 2);
                try self.setAmber(.dim);
                try self.writer.writeAll("Exits:");
                for (sector.connections, 0..) |conn, i| {
                    try self.moveTo(center_col + 12, @intCast(center_row - 1 + i));
                    try self.setAmber(.normal);
                    try self.writer.print("  {} ", .{conn});

                    // Check if this neighbor has known content
                    if (state.known_sectors.get(conn.toKey())) |nbr| {
                        try self.setAmber(.dim);
                        try self.writer.print("{s}", .{nbr.terrain.label()});
                    } else {
                        try self.setAmber(.faint);
                        try self.writer.writeAll("unexplored");
                    }
                }
            }
        }

        // Fleet status sidebar (right side)
        try self.renderPanel("FLEET STATUS", 70, 2, 30, 10);
        try self.moveTo(72, 3);
        try self.setAmber(.dim);
        try self.writer.writeAll("Ships: ");
        try self.setAmber(.normal);
        try self.writer.print("{d}", .{fleet.ships.len});

        try self.moveTo(72, 4);
        try self.setAmber(.dim);
        try self.writer.writeAll("Fuel:  ");
        try self.setAmber(if (fleet.fuel / fleet.fuel_max < 0.25) .full else .normal);
        try self.writer.print("{d:.0}/{d:.0}", .{ fleet.fuel, fleet.fuel_max });

        try self.moveTo(72, 6);
        try self.setAmber(.dim);
        try self.writer.writeAll("Cargo:");
        try self.moveTo(72, 7);
        try self.writer.print(" Fe {d:.0}", .{fleet.cargo.metal});
        try self.moveTo(72, 8);
        try self.writer.print(" Cr {d:.0}", .{fleet.cargo.crystal});
        try self.moveTo(72, 9);
        try self.writer.print(" De {d:.0}", .{fleet.cargo.deuterium});

        // Helm controls at bottom
        try self.moveTo(0, self.term_height - 3);
        try self.setAmber(.dim);
        try self.writer.writeAll("  [1-6] move to exit  [h]arvest  [a]ttack  [r]ecall  [esc] command center");
    }

    // ── Star Map View ──────────────────────────────────────────────

    fn renderStarMap(self: *Renderer, state: *const ClientState) !void {
        _ = state;

        // Toolbar
        try self.setAmber(.dim);
        try self.writer.writeAll("  ZOOM: ");
        // TODO: highlight active zoom level
        try self.writer.writeAll("[1]CLOSE  [2]SECTOR  [3]REGION");
        try self.writer.writeAll("  │  ARROWS: scroll  │  ENTER: waypoint  │  TAB: cycle fleet\n");

        // Map viewport
        // TODO: Render hex grid based on zoom level:
        //   .close  → node graph renderer
        //   .sector → hybrid hex-cell renderer
        //   .region → minimal dot renderer
        //
        // For each hex in the viewport:
        //   1. Check if it's in state.known_sectors → render with content
        //   2. If not explored → render as ░░░ (fog of war)
        //   3. Highlight player fleet positions
        //   4. Highlight selected/cursor hex

        try self.moveTo(10, 10);
        try self.setAmber(.dim);
        try self.writer.writeAll("[ Star Map renderer — TODO ]");
        try self.moveTo(10, 12);
        try self.setAmber(.faint);
        try self.writer.writeAll("Node graph (close), hybrid hex (sector), dot map (region)");
    }

    // ── Drawing Primitives ─────────────────────────────────────────

    const AmberLevel = enum { faint, dim, normal, bright, full };

    fn setAmber(self: *Renderer, level: AmberLevel) !void {
        // Amber ≈ RGB(255, 176, 0), scaled by brightness
        const rgb: struct { r: u8, g: u8, b: u8 } = switch (level) {
            .faint => .{ .r = 30, .g = 21, .b = 0 },
            .dim => .{ .r = 77, .g = 53, .b = 0 },
            .normal => .{ .r = 153, .g = 106, .b = 0 },
            .bright => .{ .r = 217, .g = 150, .b = 0 },
            .full => .{ .r = 255, .g = 176, .b = 0 },
        };
        try self.writer.print("\x1b[38;2;{d};{d};{d}m", .{ rgb.r, rgb.g, rgb.b });
    }

    fn moveTo(self: *Renderer, col: u16, row: u16) !void {
        try self.writer.print("\x1b[{d};{d}H", .{ row + 1, col + 1 });
    }

    fn repeatChar(self: *Renderer, char: u8, count: u16) !void {
        var i: u16 = 0;
        while (i < count) : (i += 1) {
            try self.writer.writeByte(char);
        }
    }

    fn renderPanel(self: *Renderer, title: []const u8, x: u16, y: u16, w: u16, h: u16) !void {
        try self.setAmber(.dim);

        // Top border
        try self.moveTo(x, y);
        try self.writer.writeAll("┌");
        try self.setAmber(.dim);
        try self.writer.writeAll("─ ");
        try self.setAmber(.normal);
        try self.writer.print("{s}", .{title});
        try self.setAmber(.dim);
        try self.writer.writeAll(" ");

        const title_len: u16 = @intCast(title.len + 4);
        if (w > title_len + 1) {
            try self.repeatChar('─', w - title_len - 1);
        }
        try self.writer.writeAll("┐");

        // Side borders
        var row: u16 = 1;
        while (row < h - 1) : (row += 1) {
            try self.moveTo(x, y + row);
            try self.writer.writeAll("│");
            try self.moveTo(x + w - 1, y + row);
            try self.writer.writeAll("│");
        }

        // Bottom border
        try self.moveTo(x, y + h - 1);
        try self.writer.writeAll("└");
        try self.repeatChar('─', w - 2);
        try self.writer.writeAll("┘");
    }
};
