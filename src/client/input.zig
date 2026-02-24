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
    return switch (c) {
        'q' => .{ .quit = {} },
        'w' => .{ .switch_view = .windshield },
        'm' => .{ .switch_view = .star_map },
        '`' => .{ .switch_view = .command_center },

        // Movement keys: map 1-6 to the sector's connected exits
        '1', '2', '3', '4', '5', '6' => mapMovement(c, state),

        'h' => mapHarvest(state),
        'r' => mapRecall(state),

        else => .{ .none = {} },
    };
}

fn mapMovement(key: u21, state: *const ClientState) InputAction {
    if (state.current_view != .windshield) return .{ .none = {} };

    const fleet = state.activeFleet() orelse return .{ .none = {} };
    const sector = state.currentSector() orelse return .{ .none = {} };

    const exit_idx: usize = @intCast(key - '1');
    if (exit_idx >= sector.connections.len) return .{ .none = {} };

    return .{ .send_command = .{ .move = .{
        .fleet_id = fleet.id,
        .target = sector.connections[exit_idx],
    } } };
}

fn mapHarvest(state: *const ClientState) InputAction {
    const fleet = state.activeFleet() orelse return .{ .none = {} };
    return .{ .send_command = .{ .harvest = .{
        .fleet_id = fleet.id,
        .resource = .auto,
    } } };
}

fn mapRecall(state: *const ClientState) InputAction {
    const fleet = state.activeFleet() orelse return .{ .none = {} };
    return .{ .send_command = .{ .recall = .{
        .fleet_id = fleet.id,
    } } };
}
