const std = @import("std");
const shared = @import("shared");

const GameEngine = @import("engine.zig").GameEngine;
const Network = @import("network.zig").Network;
const Database = @import("database.zig").Database;

const log = std.log.scoped(.server);

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
    log.info("Tick rate:  {d} Hz", .{shared.constants.TICK_RATE_HZ});
    log.info("───────────────────────────────────────────", .{});

    var db = try Database.init(allocator, config.db_path);
    defer db.deinit();

    var engine = try GameEngine.init(allocator, config.world_seed, &db);
    defer engine.deinit();

    var network = try Network.init(allocator, config.port, &engine);
    defer network.deinit();

    try network.startListening();

    log.info("Server ready.", .{});

    var tick_timer = try std.time.Timer.start();

    while (true) {
        const tick_start = tick_timer.read();

        try network.processIncoming();
        try engine.tick();
        try network.broadcastUpdates(&engine);

        if (engine.currentTick() % 30 == 0) {
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
}

const ServerConfig = struct {
    port: u16,
    world_seed: u64,
    db_path: []const u8,
};

fn parseArgs() ServerConfig {
    return .{
        .port = shared.constants.DEFAULT_PORT,
        .world_seed = shared.constants.DEFAULT_WORLD_SEED,
        .db_path = "iac_world.db",
    };
}

test {
    std.testing.refAllDecls(@This());
}
