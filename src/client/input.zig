const shared = @import("shared");
const zithril = @import("zithril");
const State = @import("state.zig");

const Key = zithril.Key;
const ClientState = State.ClientState;
const protocol = shared.protocol;

pub const InputAction = union(enum) {
    none: void,
    quit: void,
    switch_view: State.View,
    send_command: protocol.Command,
    scroll: State.ScrollDirection,
    zoom: State.ZoomLevel,
    cycle_fleet: void,
    center_fleet: void,
    toggle_info: void,
    toggle_keybinds: void,
    homeworld_nav: State.HomeworldNav,
    fleet_nav: State.FleetManagerNav,
    toggle_tech_tree: void,
};

pub fn mapKey(key: Key, state: *const ClientState) InputAction {
    if (!key.isPress() and !key.isRepeat()) return .{ .none = {} };

    // Global keys
    if (key.code == .escape) return .{ .switch_view = .command_center };

    // Homeworld-specific: arrow/tab/enter routing for card navigation
    if (state.current_view == .homeworld and !state.show_keybinds and !state.show_tech_tree) {
        if (state.homeworld_tab == .fleets) {
            switch (key.code) {
                .tab => return .{ .homeworld_nav = .tab_next },
                .up => return .{ .fleet_nav = .{ .cursor_up = {} } },
                .down => return .{ .fleet_nav = .{ .cursor_down = {} } },
                else => {},
            }
        } else if (state.homeworld_tab == .inventory) {
            if (key.code == .tab) return .{ .homeworld_nav = .tab_next };
        } else {
            switch (key.code) {
                .tab => return .{ .homeworld_nav = .tab_next },
                .up => return .{ .homeworld_nav = .cursor_up },
                .down => return .{ .homeworld_nav = .cursor_down },
                .left => return .{ .homeworld_nav = .cursor_left },
                .right => return .{ .homeworld_nav = .cursor_right },
                .enter => return .{ .homeworld_nav = .select },
                else => {},
            }
        }
    }

    switch (key.code) {
        .char => |c| return mapChar(c, state),
        .tab => return .{ .cycle_fleet = {} },
        .up => return .{ .scroll = .up },
        .down => return .{ .scroll = .down },
        .left => return .{ .scroll = .left },
        .right => return .{ .scroll = .right },
        else => {},
    }

    return .{ .none = {} };
}

fn mapChar(c: u21, state: *const ClientState) InputAction {
    if (state.current_view == .homeworld) {
        if (state.homeworld_tab == .fleets) return mapFleetManagerChar(c);
        return mapHomeworldChar(c);
    }

    return switch (c) {
        'q' => .{ .quit = {} },
        'w' => .{ .switch_view = .windshield },
        'm' => .{ .switch_view = .star_map },
        'b' => .{ .switch_view = .homeworld },
        '`' => .{ .switch_view = .command_center },

        // Movement keys: map 1-6 to the sector's connected exits
        '1', '2', '3', '4', '5', '6' => mapMovement(c, state),

        'h' => mapHarvest(state),
        'r' => mapRecall(state),
        'a' => mapAttack(state),
        's' => mapCollectSalvage(state),
        'i' => if (state.current_view == .windshield) .{ .toggle_info = {} } else .{ .none = {} },
        '?' => .{ .toggle_keybinds = {} },

        'z' => mapZoomOut(state),
        'x' => mapZoomIn(state),
        'c' => .{ .center_fleet = {} },

        else => .{ .none = {} },
    };
}

fn mapFleetManagerChar(c: u21) InputAction {
    return switch (c) {
        'q' => .{ .quit = {} },
        'w' => .{ .switch_view = .windshield },
        'm' => .{ .switch_view = .star_map },
        '`' => .{ .switch_view = .command_center },
        '?' => .{ .toggle_keybinds = {} },
        'n' => .{ .fleet_nav = .{ .create_fleet = {} } },
        'd' => .{ .fleet_nav = .{ .dock_ship = {} } },
        'x' => .{ .fleet_nav = .{ .dissolve_fleet = {} } },
        '1' => .{ .fleet_nav = .{ .assign_to_fleet = 0 } },
        '2' => .{ .fleet_nav = .{ .assign_to_fleet = 1 } },
        '3' => .{ .fleet_nav = .{ .assign_to_fleet = 2 } },
        else => .{ .none = {} },
    };
}

fn mapHomeworldChar(c: u21) InputAction {
    return switch (c) {
        'q' => .{ .quit = {} },
        'w' => .{ .switch_view = .windshield },
        'm' => .{ .switch_view = .star_map },
        '`' => .{ .switch_view = .command_center },
        '?' => .{ .toggle_keybinds = {} },
        't' => .{ .toggle_tech_tree = {} },

        // Cancel queues
        'x' => .{ .send_command = .{ .cancel_build = .{ .queue_type = .building } } },
        'X' => .{ .send_command = .{ .cancel_build = .{ .queue_type = .ship } } },
        'z' => .{ .send_command = .{ .cancel_build = .{ .queue_type = .research } } },

        else => .{ .none = {} },
    };
}

fn mapMovement(key: u21, state: *const ClientState) InputAction {
    if (state.current_view != .windshield) return .{ .none = {} };

    const fleet = activeReadyFleet(state) orelse return .{ .none = {} };
    const sector = state.currentSector() orelse return .{ .none = {} };

    const dir_idx: usize = @intCast(key - '1');
    const dir = shared.HexDirection.ALL[dir_idx];
    const target = fleet.location.neighbor(dir);

    for (sector.connections) |conn| {
        if (conn.eql(target)) {
            return .{ .send_command = .{ .move = .{
                .fleet_id = fleet.id,
                .target = target,
            } } };
        }
    }

    return .{ .none = {} };
}

fn mapHarvest(state: *const ClientState) InputAction {
    const fleet = activeReadyFleet(state) orelse return .{ .none = {} };
    return .{ .send_command = .{ .harvest = .{
        .fleet_id = fleet.id,
        .resource = .auto,
    } } };
}

fn mapRecall(state: *const ClientState) InputAction {
    const fleet = activeReadyFleet(state) orelse return .{ .none = {} };
    return .{ .send_command = .{ .recall = .{
        .fleet_id = fleet.id,
    } } };
}

fn mapAttack(state: *const ClientState) InputAction {
    if (state.current_view != .windshield) return .{ .none = {} };
    const fleet = activeReadyFleet(state) orelse return .{ .none = {} };
    const sector = state.currentSector() orelse return .{ .none = {} };
    const hostiles = sector.hostiles orelse return .{ .none = {} };
    if (hostiles.len == 0) return .{ .none = {} };
    return .{ .send_command = .{ .attack = .{
        .fleet_id = fleet.id,
        .target_fleet_id = hostiles[0].id,
    } } };
}

fn mapCollectSalvage(state: *const ClientState) InputAction {
    if (state.current_view != .windshield) return .{ .none = {} };
    const fleet = activeReadyFleet(state) orelse return .{ .none = {} };
    return .{ .send_command = .{ .collect_salvage = .{
        .fleet_id = fleet.id,
    } } };
}

fn mapZoomOut(state: *const ClientState) InputAction {
    if (state.current_view != .star_map) return .{ .none = {} };
    return switch (state.map_zoom) {
        .close => .{ .zoom = .sector },
        .sector => .{ .zoom = .region },
        .region => .{ .none = {} },
    };
}

fn mapZoomIn(state: *const ClientState) InputAction {
    if (state.current_view != .star_map) return .{ .none = {} };
    return switch (state.map_zoom) {
        .region => .{ .zoom = .sector },
        .sector => .{ .zoom = .close },
        .close => .{ .none = {} },
    };
}

/// Returns the active fleet only if it exists and has ships.
fn activeReadyFleet(state: *const ClientState) ?*const protocol.FleetState {
    const fleet = state.activeFleet() orelse return null;
    if (fleet.ships.len == 0) return null;
    return fleet;
}
