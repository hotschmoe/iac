// src/client/main.zig
// IAC TUI Client
// Connects to the game server over WebSocket, renders the amber terminal interface.

const std = @import("std");
const shared = @import("shared");

const Connection = @import("connection.zig").Connection;
const Renderer = @import("renderer.zig").Renderer;
const Input = @import("input.zig").InputHandler;
const State = @import("state.zig").ClientState;

const log = std.log.scoped(.client);

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    // Parse args
    const config = try parseArgs(allocator);

    // Initialize subsystems
    var state = State.init(allocator);
    defer state.deinit();

    var renderer = try Renderer.init(allocator);
    defer renderer.deinit();

    var input = try Input.init();
    defer input.deinit();

    var conn = try Connection.init(allocator, config.server_host, config.server_port);
    defer conn.deinit();

    // Authenticate
    try conn.sendAuth(config.player_name);

    log.info("Connected to {s}:{d} as '{s}'", .{
        config.server_host,
        config.server_port,
        config.player_name,
    });

    // ── Main loop ──────────────────────────────────────────────────
    // The client loop runs faster than the server tick rate to keep
    // the TUI responsive. We render at ~30fps, process server updates
    // as they arrive, and handle input immediately.

    var frame_timer = try std.time.Timer.start();
    const frame_ns: u64 = 1_000_000_000 / 30; // 30 fps

    var running = true;
    while (running) {
        const frame_start = frame_timer.read();

        // 1. Process server messages
        while (try conn.poll()) |msg| {
            try state.applyServerMessage(msg);
        }

        // 2. Process user input
        if (try input.poll()) |action| {
            switch (action) {
                .quit => running = false,
                .switch_view => |view| state.setView(view),
                .command => |cmd| try conn.sendCommand(cmd),
                .scroll => |dir| state.scrollMap(dir),
                .zoom => |level| state.setZoom(level),
                else => {},
            }
        }

        // 3. Render current view
        try renderer.render(&state);

        // 4. Frame timing
        const elapsed = frame_timer.read() - frame_start;
        if (elapsed < frame_ns) {
            std.time.sleep(frame_ns - elapsed);
        }
    }

    // Cleanup: restore terminal
    try renderer.cleanup();
}

const ClientConfig = struct {
    server_host: []const u8,
    server_port: u16,
    player_name: []const u8,
};

fn parseArgs(allocator: std.mem.Allocator) !ClientConfig {
    _ = allocator;
    // TODO: proper arg parsing
    return .{
        .server_host = shared.constants.DEFAULT_HOST,
        .server_port = shared.constants.DEFAULT_PORT,
        .player_name = "Admiral",
    };
}
