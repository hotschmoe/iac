const std = @import("std");
const shared = @import("shared");

const GameEngine = @import("engine.zig").GameEngine;
const Network = @import("network.zig").Network;
const Database = @import("database.zig").Database;

const log = std.log.scoped(.server);

var shutdown_requested: std.atomic.Value(bool) = std.atomic.Value(bool).init(false);

fn handleSignal(_: c_int) callconv(.c) void {
    shutdown_requested.store(true, .release);
}

fn installSignalHandlers() void {
    const handler: std.posix.Sigaction = .{
        .handler = .{ .handler = handleSignal },
        .mask = std.posix.sigemptyset(),
        .flags = 0,
    };
    std.posix.sigaction(std.posix.SIG.INT, &handler, null);
    std.posix.sigaction(std.posix.SIG.TERM, &handler, null);
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const config = parseArgs();

    log.info("═══════════════════════════════════════════", .{});
    log.info("  IN AMBER CLAD — Server v0.1.0", .{});
    log.info("═══════════════════════════════════════════", .{});
    log.info("Port:       {d}", .{config.port});
    log.info("World seed: 0x{X}", .{config.world_seed});
    log.info("Max players: {d}", .{config.max_players});
    log.info("Tick rate:  {d} Hz", .{shared.constants.TICK_RATE_HZ});
    log.info("───────────────────────────────────────────", .{});

    var db = try Database.init(allocator, config.db_path);
    defer db.deinit();

    var engine = try GameEngine.init(allocator, config.world_seed, &db);
    defer engine.deinit();

    var network = try Network.init(allocator, config.port, &engine, config.max_players);
    defer network.deinit();

    installSignalHandlers();
    try network.startListening();

    log.info("Server ready.", .{});

    const persist_every_n_ticks = 30;
    var tick_timer = try std.time.Timer.start();

    while (!shutdown_requested.load(.acquire)) {
        const tick_start = tick_timer.read();

        try network.processIncoming();
        try engine.tick();
        try network.broadcastUpdates(&engine);

        if (engine.currentTick() % persist_every_n_ticks == 0) {
            try engine.persistDirtyState();
        }

        const elapsed = tick_timer.read() - tick_start;
        const tick_ns = shared.constants.TICK_DURATION_NS;
        if (elapsed < tick_ns) {
            std.Thread.sleep(tick_ns - elapsed);
        } else {
            log.warn("Tick {d} overran by {d}ms", .{
                engine.currentTick(),
                (elapsed - tick_ns) / 1_000_000,
            });
        }
    }

    log.info("Shutdown signal received, persisting final state...", .{});
    engine.persistDirtyState() catch |err| {
        log.err("Final persist failed: {}", .{err});
    };
    log.info("Server shutdown complete.", .{});
}

const ServerConfig = struct {
    port: u16,
    world_seed: u64,
    db_path: []const u8,
    max_players: u32,
};

fn parseArgs() ServerConfig {
    var config = ServerConfig{
        .port = shared.constants.DEFAULT_PORT,
        .world_seed = shared.constants.DEFAULT_WORLD_SEED,
        .db_path = "iac_world.db",
        .max_players = shared.constants.MAX_PLAYERS,
    };

    var args = std.process.args();
    _ = args.skip(); // program name
    while (args.next()) |arg| {
        if (std.mem.eql(u8, arg, "--port")) {
            if (args.next()) |val| {
                config.port = std.fmt.parseInt(u16, val, 10) catch shared.constants.DEFAULT_PORT;
            }
        } else if (std.mem.eql(u8, arg, "--seed")) {
            if (args.next()) |val| {
                config.world_seed = std.fmt.parseInt(u64, val, 10) catch shared.constants.DEFAULT_WORLD_SEED;
            }
        } else if (std.mem.eql(u8, arg, "--db")) {
            if (args.next()) |val| {
                config.db_path = val;
            }
        } else if (std.mem.eql(u8, arg, "--max-players")) {
            if (args.next()) |val| {
                config.max_players = std.fmt.parseInt(u32, val, 10) catch shared.constants.MAX_PLAYERS;
            }
        }
    }

    return config;
}

test {
    std.testing.refAllDecls(@This());
}
