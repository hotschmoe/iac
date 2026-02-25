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
    active_fleet_idx: usize,
    prev_fleet_location: ?Hex = null,
    map_center: Hex,
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
        self.freeOwnedFleets();
        self.fleets.deinit(self.allocator);
        self.freeOwnedPlayer();
        self.freeOwnedHomeworld();
        self.freeOwnedSectors();
        self.known_sectors.deinit();
        self.event_log.deinit();
    }

    fn freeOwnedFleets(self: *ClientState) void {
        for (self.fleets.items) |fleet| {
            self.allocator.free(fleet.ships);
        }
    }

    fn freeOwnedPlayer(self: *ClientState) void {
        if (self.player) |player| {
            self.allocator.free(player.name);
            self.player = null;
        }
    }

    fn freeOwnedHomeworld(self: *ClientState) void {
        if (self.homeworld) |hw| {
            self.allocator.free(hw.buildings);
            self.allocator.free(hw.docked_ships);
            self.homeworld = null;
        }
    }

    fn freeOwnedSectors(self: *ClientState) void {
        var iter = self.known_sectors.iterator();
        while (iter.next()) |entry| {
            freeSectorSlices(self.allocator, entry.value_ptr.*);
        }
    }

    fn freeSectorSlices(alloc: std.mem.Allocator, sector: shared.protocol.SectorState) void {
        alloc.free(sector.connections);
        if (sector.hostiles) |hostiles| {
            for (hostiles) |h| alloc.free(h.ships);
            alloc.free(hostiles);
        }
        if (sector.player_fleets) |pf| {
            for (pf) |f| alloc.free(f.owner_name);
            alloc.free(pf);
        }
    }

    fn replaceSector(self: *ClientState, sector: shared.protocol.SectorState) !void {
        if (self.known_sectors.getPtr(sector.location.toKey())) |existing| {
            freeSectorSlices(self.allocator, existing.*);
        }
        var owned = sector;
        owned.connections = try self.allocator.dupe(Hex, sector.connections);
        if (sector.hostiles) |hostiles| {
            const duped = try self.allocator.alloc(shared.protocol.NpcFleetInfo, hostiles.len);
            for (hostiles, 0..) |h, i| {
                duped[i] = h;
                duped[i].ships = try self.allocator.dupe(shared.protocol.NpcShipInfo, h.ships);
            }
            owned.hostiles = duped;
        }
        if (sector.player_fleets) |pf| {
            const duped = try self.allocator.alloc(shared.protocol.FleetBrief, pf.len);
            for (pf, 0..) |f, i| {
                duped[i] = f;
                duped[i].owner_name = try self.allocator.dupe(u8, f.owner_name);
            }
            owned.player_fleets = duped;
        }
        try self.known_sectors.put(sector.location.toKey(), owned);
    }

    pub fn applyServerMessage(self: *ClientState, msg: shared.protocol.ServerMessage) !void {
        switch (msg) {
            .tick_update => |update| {
                self.tick = update.tick;

                if (update.fleet_updates) |fleets| {
                    try self.replaceFleets(fleets);
                }

                if (update.homeworld_update) |hw| {
                    try self.replaceHomeworld(hw);
                }

                if (update.sector_update) |sector| {
                    try self.replaceSector(sector);
                }

                if (update.events) |events| {
                    for (events) |event| {
                        try self.event_log.push(event);
                    }
                }
            },
            .full_state => |state| {
                self.tick = state.tick;
                try self.replacePlayer(state.player);
                try self.replaceHomeworld(state.homeworld);
                try self.replaceFleets(state.fleets);

                self.freeOwnedSectors();
                self.known_sectors.clearRetainingCapacity();
                for (state.known_sectors) |sector| {
                    try self.replaceSector(sector);
                }

                if (self.player) |p| {
                    self.map_center = p.homeworld;
                }
            },
            .event => |event| {
                try self.event_log.push(event);
            },
            .auth_result => |result| {
                if (!result.success) {
                    // TODO: display error
                }
            },
            .@"error" => |err| {
                const len = @min(err.message.len, self.status_message.len);
                @memcpy(self.status_message[0..len], err.message[0..len]);
                self.status_len = len;

                try self.event_log.push(.{
                    .tick = self.tick,
                    .kind = .{ .alert = .{
                        .level = .warning,
                        .message = self.status_message[0..self.status_len],
                    } },
                });
            },
        }
    }

    fn replaceFleets(self: *ClientState, fleets: []const shared.protocol.FleetState) !void {
        const old_loc: ?Hex = if (self.activeFleet()) |f| f.location else null;
        self.freeOwnedFleets();
        self.fleets.clearRetainingCapacity();
        for (fleets) |fleet| {
            var owned = fleet;
            owned.ships = try self.allocator.dupe(shared.protocol.ShipState, fleet.ships);
            try self.fleets.append(self.allocator, owned);
        }
        if (old_loc) |prev| {
            if (self.activeFleet()) |cur| {
                if (!cur.location.eql(prev)) {
                    self.prev_fleet_location = prev;
                }
            }
        }
    }

    fn replacePlayer(self: *ClientState, player: shared.protocol.PlayerState) !void {
        self.freeOwnedPlayer();
        var owned = player;
        owned.name = try self.allocator.dupe(u8, player.name);
        self.player = owned;
    }

    fn replaceHomeworld(self: *ClientState, hw: shared.protocol.HomeworldState) !void {
        self.freeOwnedHomeworld();
        var owned = hw;
        owned.buildings = try self.allocator.dupe(shared.protocol.BuildingState, hw.buildings);
        owned.docked_ships = try self.allocator.dupe(shared.protocol.ShipState, hw.docked_ships);
        self.homeworld = owned;
    }

    pub fn setView(self: *ClientState, view: View) void {
        self.current_view = view;
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
            self.prev_fleet_location = null;
            if (self.activeFleet()) |fleet| {
                self.map_center = fleet.location;
            }
        }
    }

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
/// Owns duped copies of any heap-allocated slices in events (AlertEvent.message).
pub const EventLog = struct {
    const MAX_EVENTS = 100;

    allocator: std.mem.Allocator,
    events: [MAX_EVENTS]shared.protocol.GameEvent = undefined,
    head: usize = 0,
    count: usize = 0,

    pub fn init(allocator: std.mem.Allocator) EventLog {
        return .{ .allocator = allocator };
    }

    pub fn deinit(self: *EventLog) void {
        const start = (self.head + MAX_EVENTS - self.count) % MAX_EVENTS;
        for (0..self.count) |i| {
            self.freeSlices(self.events[(start + i) % MAX_EVENTS]);
        }
    }

    pub fn push(self: *EventLog, event: shared.protocol.GameEvent) !void {
        if (self.count == MAX_EVENTS) {
            self.freeSlices(self.events[self.head]);
        }

        var owned = event;
        switch (owned.kind) {
            .alert => |a| owned.kind.alert.message = try self.allocator.dupe(u8, a.message),
            else => {},
        }

        self.events[self.head] = owned;
        self.head = (self.head + 1) % MAX_EVENTS;
        if (self.count < MAX_EVENTS) self.count += 1;
    }

    fn freeSlices(self: *EventLog, event: shared.protocol.GameEvent) void {
        switch (event.kind) {
            .alert => |a| self.allocator.free(a.message),
            else => {},
        }
    }

    pub fn getRecent(self: *const EventLog, n: usize) ?shared.protocol.GameEvent {
        if (n >= self.count) return null;
        const idx = (self.head + MAX_EVENTS - 1 - n) % MAX_EVENTS;
        return self.events[idx];
    }
};
