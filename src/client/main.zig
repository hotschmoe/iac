const std = @import("std");
const shared = @import("shared");
const zithril = @import("zithril");

const Connection = @import("connection.zig").Connection;
const renderer = @import("renderer.zig");
const input = @import("input.zig");
const State = @import("state.zig");

const ClientState = State.ClientState;
const App = zithril.App(AppState);

const log = std.log.scoped(.client);


const AppState = struct {
    client_state: ClientState,
    conn: Connection,
    allocator: std.mem.Allocator,
    config: ClientConfig,
};

fn update(state: *AppState, event: zithril.Event) zithril.Action {
    switch (event) {
        .key => |key| {
            const action = input.mapKey(key, &state.client_state);
            switch (action) {
                .quit => return zithril.Action.quit_action,
                .switch_view => |v| state.client_state.setView(v),
                .send_command => |cmd| {
                    state.conn.sendCommand(cmd) catch |err| {
                        log.warn("Send failed: {any}", .{err});
                    };
                },
                .scroll => |dir| state.client_state.scrollMap(dir),
                .zoom => |level| state.client_state.setZoom(level),
                .cycle_fleet => state.client_state.cycleFleet(),
                .center_fleet => {
                    if (state.client_state.activeFleet()) |fleet| {
                        state.client_state.map_center = fleet.location;
                    }
                },
                .toggle_info => {
                    state.client_state.show_sector_info = !state.client_state.show_sector_info;
                    state.client_state.show_keybinds = false;
                },
                .toggle_keybinds => {
                    state.client_state.show_keybinds = !state.client_state.show_keybinds;
                    state.client_state.show_sector_info = false;
                    state.client_state.show_tech_tree = false;
                },
                .homeworld_nav => |nav| {
                    if (state.client_state.homeworldNav(nav)) |cmd| {
                        state.conn.sendCommand(cmd) catch |err| {
                            log.warn("Send failed: {any}", .{err});
                        };
                    }
                },
                .fleet_nav => |nav| {
                    if (state.client_state.fleetManagerNav(nav)) |cmd| {
                        state.conn.sendCommand(cmd) catch |err| {
                            log.warn("Send failed: {any}", .{err});
                        };
                    }
                },
                .toggle_tech_tree => {
                    state.client_state.show_tech_tree = !state.client_state.show_tech_tree;
                    state.client_state.show_keybinds = false;
                },
                .none => {},
            }
        },
        .tick => {
            for (0..20) |_| {
                const parsed = state.conn.poll() catch break;
                if (parsed) |p| {
                    defer p.deinit();
                    state.client_state.applyServerMessage(p.value) catch |err| {
                        log.warn("Apply message failed: {any}", .{err});
                    };
                } else break;
            }
        },
        else => {},
    }
    return zithril.Action.none_action;
}

fn appView(state: *AppState, frame: *zithril.Frame(App.DefaultMaxWidgets)) void {
    renderer.view(&state.client_state, frame);
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const config = parseArgs();

    var client_state = ClientState.init(allocator);
    client_state.config_player_name = config.player_name;

    var app_state = AppState{
        .client_state = client_state,
        .conn = try Connection.init(allocator, config.server_host, config.server_port),
        .allocator = allocator,
        .config = config,
    };
    defer app_state.client_state.deinit();
    defer app_state.conn.deinit();

    const token = loadToken(allocator, config.player_name);
    if (token) |t| {
        defer allocator.free(t);
        try app_state.conn.sendAuthLogin(config.player_name, t);
    } else {
        try app_state.conn.sendAuthRegister(config.player_name);
    }

    var app = App.init(.{
        .state = &app_state,
        .update = update,
        .view = appView,
        .tick_rate_ms = 100,
    });

    try app.run(allocator);
}

const ClientConfig = struct {
    server_host: []const u8,
    server_port: u16,
    player_name: []const u8,
};

fn parseArgs() ClientConfig {
    var config = ClientConfig{
        .server_host = shared.constants.DEFAULT_HOST,
        .server_port = shared.constants.DEFAULT_PORT,
        .player_name = "Admiral",
    };

    var args = std.process.args();
    _ = args.skip();
    while (args.next()) |arg| {
        if (std.mem.eql(u8, arg, "--host")) {
            if (args.next()) |val| config.server_host = val;
        } else if (std.mem.eql(u8, arg, "--port")) {
            if (args.next()) |val| {
                config.server_port = std.fmt.parseInt(u16, val, 10) catch shared.constants.DEFAULT_PORT;
            }
        } else if (std.mem.eql(u8, arg, "--name")) {
            if (args.next()) |val| config.player_name = val;
        }
    }

    return config;
}

const CREDS_DIR = ".iac";

fn loadToken(allocator: std.mem.Allocator, player_name: []const u8) ?[]const u8 {
    const home = std.posix.getenv("HOME") orelse return null;
    var path_buf: [std.fs.max_path_bytes]u8 = undefined;
    const path = std.fmt.bufPrint(&path_buf, "{s}/{s}/{s}.token", .{ home, CREDS_DIR, player_name }) catch return null;

    const contents = std.fs.cwd().readFileAlloc(allocator, path, 4096) catch return null;
    const trimmed = std.mem.trim(u8, contents, &std.ascii.whitespace);
    if (trimmed.len == 0) {
        allocator.free(contents);
        return null;
    }
    if (trimmed.ptr == contents.ptr and trimmed.len == contents.len) return contents;
    defer allocator.free(contents);
    return allocator.dupe(u8, trimmed) catch null;
}

pub fn saveToken(player_name: []const u8, token: []const u8) void {
    const home = std.posix.getenv("HOME") orelse return;

    var dir_buf: [std.fs.max_path_bytes]u8 = undefined;
    const dir_path = std.fmt.bufPrint(&dir_buf, "{s}/{s}", .{ home, CREDS_DIR }) catch return;
    std.fs.cwd().makeDir(dir_path) catch |err| {
        if (err != error.PathAlreadyExists) return;
    };

    var path_buf: [std.fs.max_path_bytes]u8 = undefined;
    const path = std.fmt.bufPrint(&path_buf, "{s}/{s}/{s}.token", .{ home, CREDS_DIR, player_name }) catch return;

    const file = std.fs.cwd().createFile(path, .{ .mode = 0o600 }) catch return;
    defer file.close();
    file.writeAll(token) catch return;

    log.info("Token saved to {s}", .{path});
}
