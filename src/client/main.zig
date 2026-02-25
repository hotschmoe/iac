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

    var app_state = AppState{
        .client_state = ClientState.init(allocator),
        .conn = try Connection.init(allocator, config.server_host, config.server_port),
        .allocator = allocator,
    };
    defer app_state.client_state.deinit();
    defer app_state.conn.deinit();

    // Authenticate
    try app_state.conn.sendAuth(config.player_name);

    var app = App.init(.{
        .state = &app_state,
        .update = update,
        .view = appView,
        .tick_rate_ms = 100, // Poll server at ~10Hz, render on change
    });

    try app.run(allocator);
}

const ClientConfig = struct {
    server_host: []const u8,
    server_port: u16,
    player_name: []const u8,
};

fn parseArgs() ClientConfig {
    return .{
        .server_host = shared.constants.DEFAULT_HOST,
        .server_port = shared.constants.DEFAULT_PORT,
        .player_name = "Admiral",
    };
}
