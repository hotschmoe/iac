// src/client/state.zig
// Client-side game state. Mirrors server state as received via tick updates.
// Also tracks UI state: current view, scroll position, selected fleet, etc.

const std = @import("std");
const shared = @import("shared");

const Hex = shared.Hex;

pub const ClientState = struct {
    allocator: std.mem.Allocator,

    // Game state (from server)
    tick: u64,
    player: ?shared.protocol.PlayerState,
    fleets: std.ArrayList(shared.protocol.FleetState),
    homeworld: ?shared.protocol.HomeworldState,
    known_sectors: std.AutoHashMap(u32, shared.protocol.SectorState),
    event_log: EventLog,

    // UI state
    current_view: View,
    active_fleet_idx: usize, // which fleet is selected in windshield
    map_center: Hex, // star map scroll position
    map_zoom: ZoomLevel,
    command_buffer: [256]u8,
    command_len: usize,
    command_mode: bool,
    status_message: [128]u8,
    status_len: usize,

    pub fn init(allocator: std.mem.Allocator) ClientState {
        return .{
            .allocator = allocator,
            .tick = 0,
            .player = null,
            .fleets = .empty,
            .homeworld = null,
            .known_sectors = std.AutoHashMap(u32, shared.protocol.SectorState).init(allocator),
            .event_log = EventLog.init(allocator),
            .current_view = .command_center,
            .active_fleet_idx = 0,
            .map_center = Hex.ORIGIN,
            .map_zoom = .sector,
            .command_buffer = undefined,
            .command_len = 0,
            .command_mode = false,
            .status_message = undefined,
            .status_len = 0,
        };
    }

    pub fn deinit(self: *ClientState) void {
        self.fleets.deinit(self.allocator);
        self.known_sectors.deinit();
        self.event_log.deinit();
    }

    /// Apply an incoming server message to local state.
    pub fn applyServerMessage(self: *ClientState, msg: shared.protocol.ServerMessage) !void {
        switch (msg) {
            .tick_update => |update| {
                self.tick = update.tick;

                if (update.fleet_updates) |fleets| {
                    // Replace fleet state with updates
                    // TODO: merge rather than replace
                    self.fleets.clearRetainingCapacity();
                    for (fleets) |fleet| {
                        try self.fleets.append(self.allocator, fleet);
                    }
                }

                if (update.homeworld_update) |hw| {
                    self.homeworld = hw;
                }

                if (update.sector_update) |sector| {
                    try self.known_sectors.put(sector.location.toKey(), sector);
                }

                if (update.events) |events| {
                    for (events) |event| {
                        try self.event_log.push(event);
                    }
                }
            },
            .full_state => |state| {
                self.tick = state.tick;
                self.player = state.player;
                self.homeworld = state.homeworld;

                self.fleets.clearRetainingCapacity();
                for (state.fleets) |fleet| {
                    try self.fleets.append(self.allocator, fleet);
                }

                self.known_sectors.clearRetainingCapacity();
                for (state.known_sectors) |sector| {
                    try self.known_sectors.put(sector.location.toKey(), sector);
                }

                // Center map on homeworld
                self.map_center = state.player.homeworld;
            },
            .event => |event| {
                try self.event_log.push(event);
            },
            .auth_result => |result| {
                if (!result.success) {
                    // TODO: display error
                }
            },
            .@"error" => |_| {
                // TODO: display error
            },
        }
    }

    // ── UI State Management ────────────────────────────────────────

    pub fn setView(self: *ClientState, view: View) void {
        self.current_view = view;

        // When entering windshield, center on active fleet
        if (view == .windshield) {
            if (self.activeFleet()) |fleet| {
                self.map_center = fleet.location;
            }
        }
    }

    pub fn scrollMap(self: *ClientState, direction: ScrollDirection) void {
        const delta: Hex = switch (direction) {
            .up => .{ .q = 0, .r = -1 },
            .down => .{ .q = 0, .r = 1 },
            .left => .{ .q = -1, .r = 0 },
            .right => .{ .q = 1, .r = 0 },
        };
        self.map_center = self.map_center.add(delta);
    }

    pub fn setZoom(self: *ClientState, level: ZoomLevel) void {
        self.map_zoom = level;
    }

    pub fn activeFleet(self: *const ClientState) ?*const shared.protocol.FleetState {
        if (self.active_fleet_idx < self.fleets.items.len) {
            return &self.fleets.items[self.active_fleet_idx];
        }
        return null;
    }

    pub fn cycleFleet(self: *ClientState) void {
        if (self.fleets.items.len > 0) {
            self.active_fleet_idx = (self.active_fleet_idx + 1) % self.fleets.items.len;
            if (self.activeFleet()) |fleet| {
                self.map_center = fleet.location;
            }
        }
    }

    /// Get the current sector state (where active fleet is).
    pub fn currentSector(self: *const ClientState) ?*const shared.protocol.SectorState {
        if (self.activeFleet()) |fleet| {
            return self.known_sectors.getPtr(fleet.location.toKey());
        }
        return null;
    }
};

pub const View = enum {
    command_center,
    windshield,
    star_map,
};

pub const ZoomLevel = enum {
    close, // node graph, ~3 hex radius
    sector, // hybrid hex-cell, ~8 hex radius
    region, // minimal dots, ~20 hex radius
};

pub const ScrollDirection = enum {
    up,
    down,
    left,
    right,
};

/// Ring buffer for event log entries (keeps last N events).
pub const EventLog = struct {
    const MAX_EVENTS = 100;

    allocator: std.mem.Allocator,
    events: [MAX_EVENTS]shared.protocol.GameEvent,
    head: usize,
    count: usize,

    pub fn init(allocator: std.mem.Allocator) EventLog {
        return .{
            .allocator = allocator,
            .events = undefined,
            .head = 0,
            .count = 0,
        };
    }

    pub fn deinit(self: *EventLog) void {
        _ = self;
    }

    pub fn push(self: *EventLog, event: shared.protocol.GameEvent) !void {
        self.events[self.head] = event;
        self.head = (self.head + 1) % MAX_EVENTS;
        if (self.count < MAX_EVENTS) self.count += 1;
    }

    /// Get Nth most recent event (0 = newest).
    pub fn getRecent(self: *const EventLog, n: usize) ?shared.protocol.GameEvent {
        if (n >= self.count) return null;
        // head points to next write position; head-1 is most recent
        const idx = (self.head + MAX_EVENTS - 1 - n) % MAX_EVENTS;
        return self.events[idx];
    }
};
