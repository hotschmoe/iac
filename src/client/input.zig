const shared = @import("shared");
const zithril = @import("zithril");
const State = @import("state.zig");

const Key = zithril.Key;
const ClientState = State.ClientState;

pub const InputAction = union(enum) {
    none: void,
    quit: void,
    switch_view: State.View,
    send_command: shared.protocol.Command,
    scroll: State.ScrollDirection,
    zoom: State.ZoomLevel,
    cycle_fleet: void,
    center_fleet: void,
    toggle_info: void,
    toggle_keybinds: void,
    homeworld_build: shared.protocol.BuildingType,
    homeworld_research: shared.protocol.ResearchType,
    homeworld_ship: shared.constants.ShipClass,
    homeworld_cancel: shared.scaling.QueueType,
};

pub fn mapKey(key: Key, state: *const ClientState) InputAction {
    if (!key.isPress() and !key.isRepeat()) return .{ .none = {} };

    // Global keys
    if (key.code == .escape) return .{ .switch_view = .command_center };

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
    if (state.current_view == .homeworld) return mapHomeworldChar(c);

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

fn mapHomeworldChar(c: u21) InputAction {
    return switch (c) {
        'q' => .{ .quit = {} },
        'w' => .{ .switch_view = .windshield },
        'm' => .{ .switch_view = .star_map },
        '`' => .{ .switch_view = .command_center },
        '?' => .{ .toggle_keybinds = {} },

        // Building hotkeys: 1-8 map to BuildingType enum values
        '1' => .{ .homeworld_build = .metal_mine },
        '2' => .{ .homeworld_build = .crystal_mine },
        '3' => .{ .homeworld_build = .deuterium_synthesizer },
        '4' => .{ .homeworld_build = .shipyard },
        '5' => .{ .homeworld_build = .research_lab },
        '6' => .{ .homeworld_build = .fuel_depot },
        '7' => .{ .homeworld_build = .sensor_array },
        '8' => .{ .homeworld_build = .defense_grid },

        // Ship build: s + class letter
        'S' => .{ .homeworld_ship = .scout },
        'C' => .{ .homeworld_ship = .corvette },
        'F' => .{ .homeworld_ship = .frigate },
        'R' => .{ .homeworld_ship = .cruiser },
        'H' => .{ .homeworld_ship = .hauler },

        // Research
        'r' => .{ .homeworld_research = .fuel_efficiency },
        'e' => .{ .homeworld_research = .extended_fuel_tanks },
        'h' => .{ .homeworld_research = .reinforced_hulls },
        'a' => .{ .homeworld_research = .advanced_shields },
        'p' => .{ .homeworld_research = .weapons_research },
        'n' => .{ .homeworld_research = .navigation },
        'v' => .{ .homeworld_research = .harvesting_efficiency },

        // Cancel queues
        'x' => .{ .homeworld_cancel = .building },
        'X' => .{ .homeworld_cancel = .ship },
        'z' => .{ .homeworld_cancel = .research },

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
fn activeReadyFleet(state: *const ClientState) ?*const shared.protocol.FleetState {
    const fleet = state.activeFleet() orelse return null;
    if (fleet.ships.len == 0) return null;
    return fleet;
}
