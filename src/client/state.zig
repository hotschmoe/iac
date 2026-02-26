const std = @import("std");
const shared = @import("shared");

const Hex = shared.Hex;
const protocol = shared.protocol;
const scaling = shared.scaling;
const ShipClass = shared.constants.ShipClass;

pub const HomeworldTab = enum {
    buildings,
    shipyard,
    research,
    fleets,

    pub fn next(self: HomeworldTab) HomeworldTab {
        return switch (self) {
            .buildings => .shipyard,
            .shipyard => .research,
            .research => .fleets,
            .fleets => .buildings,
        };
    }

    pub fn itemCount(self: HomeworldTab) usize {
        return switch (self) {
            .buildings => scaling.BuildingType.COUNT,
            .shipyard => ShipClass.COUNT,
            .research => scaling.ResearchType.COUNT,
            .fleets => 0, // dynamic, handled by fleet manager
        };
    }
};

pub const HomeworldNav = enum {
    cursor_up,
    cursor_down,
    cursor_left,
    cursor_right,
    tab_next,
    select,
};

pub const FleetManagerNav = union(enum) {
    cursor_up: void,
    cursor_down: void,
    create_fleet: void,
    dock_ship: void,
    assign_to_fleet: usize, // fleet index (0-based)
    dissolve_fleet: void,
};

pub const ClientState = struct {
    allocator: std.mem.Allocator,

    // Game state (from server)
    tick: u64,
    player: ?protocol.PlayerState,
    fleets: std.ArrayList(protocol.FleetState),
    homeworld: ?protocol.HomeworldState,
    known_sectors: std.AutoHashMap(u32, protocol.SectorState),
    event_log: EventLog,

    pending_token: ?[128]u8 = null,
    pending_token_len: usize = 0,
    config_player_name: []const u8 = "",

    // UI state
    current_view: View,
    active_fleet_idx: usize,
    prev_fleet_location: ?Hex = null,
    map_center: Hex,
    map_zoom: ZoomLevel,
    command_buffer: [256]u8,
    command_len: usize,
    show_sector_info: bool,
    show_keybinds: bool,
    show_tech_tree: bool,
    command_mode: bool,
    homeworld_tab: HomeworldTab,
    homeworld_cursor: usize,
    fleet_cursor: usize,
    status_message: [128]u8,
    status_len: usize,

    pub fn init(allocator: std.mem.Allocator) ClientState {
        return .{
            .allocator = allocator,
            .tick = 0,
            .player = null,
            .fleets = .empty,
            .homeworld = null,
            .known_sectors = std.AutoHashMap(u32, protocol.SectorState).init(allocator),
            .event_log = EventLog.init(allocator),
            .current_view = .command_center,
            .active_fleet_idx = 0,
            .map_center = Hex.ORIGIN,
            .map_zoom = .sector,
            .command_buffer = undefined,
            .command_len = 0,
            .show_sector_info = false,
            .show_keybinds = false,
            .show_tech_tree = false,
            .command_mode = false,
            .homeworld_tab = .buildings,
            .homeworld_cursor = 0,
            .fleet_cursor = 0,
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
            self.allocator.free(hw.research);
            self.allocator.free(hw.docked_ships);
            self.homeworld = null;
        }
    }

    fn freeOwnedSectors(self: *ClientState) void {
        var iter = self.known_sectors.iterator();
        while (iter.next()) |entry| {
            freeSectorOwned(self.allocator, entry.value_ptr.*);
        }
    }

    fn freeSectorOwned(alloc: std.mem.Allocator, sector: protocol.SectorState) void {
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

    fn replaceSector(self: *ClientState, sector: protocol.SectorState) !void {
        if (self.known_sectors.getPtr(sector.location.toKey())) |existing| {
            freeSectorOwned(self.allocator, existing.*);
        }
        var owned = sector;
        owned.connections = try self.allocator.dupe(Hex, sector.connections);
        if (sector.hostiles) |hostiles| {
            const duped = try self.allocator.alloc(protocol.NpcFleetInfo, hostiles.len);
            for (hostiles, 0..) |h, i| {
                duped[i] = h;
                duped[i].ships = try self.allocator.dupe(protocol.NpcShipInfo, h.ships);
            }
            owned.hostiles = duped;
        }
        if (sector.player_fleets) |pf| {
            const duped = try self.allocator.alloc(protocol.FleetBrief, pf.len);
            for (pf, 0..) |f, i| {
                duped[i] = f;
                duped[i].owner_name = try self.allocator.dupe(u8, f.owner_name);
            }
            owned.player_fleets = duped;
        }
        try self.known_sectors.put(sector.location.toKey(), owned);
    }

    pub fn applyServerMessage(self: *ClientState, msg: protocol.ServerMessage) !void {
        switch (msg) {
            .tick_update => |update| {
                self.tick = update.tick;

                if (update.player) |player| {
                    try self.replacePlayer(player);
                }

                if (update.fleet_updates) |fleets| {
                    try self.replaceFleets(fleets);
                }

                if (update.homeworld_update) |hw| {
                    try self.replaceHomeworld(hw);
                }

                if (update.sector_updates) |sectors| {
                    for (sectors) |sector| {
                        try self.replaceSector(sector);
                    }
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

                    if (self.pending_token) |tok| {
                        const main_mod = @import("main.zig");
                        const name = if (self.config_player_name.len > 0) self.config_player_name else p.name;
                        main_mod.saveToken(name, tok[0..self.pending_token_len]);
                        self.pending_token = null;
                        self.pending_token_len = 0;
                    }
                }
            },
            .event => |event| {
                try self.event_log.push(event);
            },
            .auth_result => |result| {
                if (result.success) {
                    if (result.token) |token| {
                        const len = @min(token.len, @as(usize, 128));
                        var buf: [128]u8 = undefined;
                        @memcpy(buf[0..len], token[0..len]);
                        self.pending_token = buf;
                        self.pending_token_len = len;
                    }
                } else {
                    if (result.message) |err_msg| {
                        const len = @min(err_msg.len, self.status_message.len);
                        @memcpy(self.status_message[0..len], err_msg[0..len]);
                        self.status_len = len;
                    }
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

    fn replaceFleets(self: *ClientState, fleets: []const protocol.FleetState) !void {
        const old_loc: ?Hex = if (self.activeFleet()) |f| f.location else null;
        self.freeOwnedFleets();
        self.fleets.clearRetainingCapacity();
        for (fleets) |fleet| {
            var owned = fleet;
            owned.ships = try self.allocator.dupe(protocol.ShipState, fleet.ships);
            try self.fleets.append(self.allocator, owned);
        }
        self.updatePrevFleetLocation(old_loc);
    }

    fn updatePrevFleetLocation(self: *ClientState, old_loc: ?Hex) void {
        const prev = old_loc orelse return;
        const cur = self.activeFleet() orelse return;
        if (!cur.location.eql(prev)) {
            self.prev_fleet_location = prev;
        }
    }

    fn replacePlayer(self: *ClientState, player: protocol.PlayerState) !void {
        self.freeOwnedPlayer();
        var owned = player;
        owned.name = try self.allocator.dupe(u8, player.name);
        self.player = owned;
    }

    fn replaceHomeworld(self: *ClientState, hw: protocol.HomeworldState) !void {
        self.freeOwnedHomeworld();
        var owned = hw;
        owned.buildings = try self.allocator.dupe(protocol.BuildingState, hw.buildings);
        owned.research = try self.allocator.dupe(protocol.ResearchState, hw.research);
        owned.docked_ships = try self.allocator.dupe(protocol.ShipState, hw.docked_ships);
        self.homeworld = owned;
    }

    pub fn buildingLevelsFromSlice(buildings: []const protocol.BuildingState) scaling.BuildingLevels {
        var levels = scaling.BuildingLevels{};
        for (buildings) |b| levels.set(b.building_type, b.level);
        return levels;
    }

    pub fn researchLevelsFromSlice(research: []const protocol.ResearchState) scaling.ResearchLevels {
        var levels = scaling.ResearchLevels{};
        for (research) |r| levels.set(r.tech, r.level);
        return levels;
    }

    pub fn homeworldNav(self: *ClientState, nav: HomeworldNav) ?protocol.Command {
        const hw = self.homeworld orelse return null;
        const count = self.homeworld_tab.itemCount();

        switch (nav) {
            .cursor_up => {
                if (self.homeworld_cursor >= 2)
                    self.homeworld_cursor -= 2;
            },
            .cursor_down => {
                if (self.homeworld_cursor + 2 < count)
                    self.homeworld_cursor += 2;
            },
            .cursor_left => {
                if (self.homeworld_cursor > 0)
                    self.homeworld_cursor -= 1;
            },
            .cursor_right => {
                if (self.homeworld_cursor + 1 < count)
                    self.homeworld_cursor += 1;
            },
            .tab_next => {
                self.homeworld_tab = self.homeworld_tab.next();
                self.homeworld_cursor = 0;
                self.fleet_cursor = 0;
            },
            .select => {
                const bldg_levels = buildingLevelsFromSlice(hw.buildings);
                const res_levels = researchLevelsFromSlice(hw.research);

                switch (self.homeworld_tab) {
                    .buildings => {
                        if (self.homeworld_cursor >= scaling.BuildingType.COUNT) return null;
                        const bt: scaling.BuildingType = @enumFromInt(self.homeworld_cursor);
                        if (!scaling.buildingPrerequisitesMet(bt, bldg_levels)) return null;
                        if (bldg_levels.get(bt) >= scaling.MAX_BUILDING_LEVEL) return null;
                        return .{ .build = .{ .building_type = bt } };
                    },
                    .research => {
                        if (self.homeworld_cursor >= scaling.ResearchType.COUNT) return null;
                        const rt: scaling.ResearchType = @enumFromInt(self.homeworld_cursor);
                        if (!scaling.researchPrerequisitesMet(rt, bldg_levels, res_levels)) return null;
                        if (res_levels.get(rt) >= scaling.researchMaxLevel(rt)) return null;
                        return .{ .research = .{ .tech = rt } };
                    },
                    .shipyard => {
                        const classes = ShipClass.ALL;
                        if (self.homeworld_cursor >= classes.len) return null;
                        const sc = classes[self.homeworld_cursor];
                        if (!scaling.shipClassUnlocked(sc, res_levels)) return null;
                        return .{ .build_ship = .{ .ship_class = sc, .count = 1 } };
                    },
                    .fleets => return null, // handled by fleetManagerNav
                }
            },
        }
        return null;
    }

    // Builds a list of selectable rows for the fleet manager.
    // Row kinds: docked ship, fleet header, fleet ship (homeworld only), deployed header (not selectable).
    pub const FleetRow = union(enum) {
        docked_header: usize, // count of docked ships
        docked_ship: protocol.ShipState,
        fleet_header: FleetHeaderInfo,
        fleet_ship: FleetShipInfo,
        deployed_header: FleetHeaderInfo,
    };

    pub const FleetHeaderInfo = struct {
        fleet_id: u64,
        fleet_idx: usize, // 0-based index among player's fleets
        state: protocol.FleetStatus,
        location: Hex,
        ship_count: usize,
    };

    pub const FleetShipInfo = struct {
        ship: protocol.ShipState,
        fleet_id: u64,
    };

    pub fn buildFleetRows(self: *const ClientState) [128]FleetRow {
        var rows: [128]FleetRow = undefined;
        var count: usize = 0;
        const hw = self.homeworld orelse return rows;
        const player = self.player orelse return rows;

        // Docked ships header + ships
        if (count < rows.len) {
            rows[count] = .{ .docked_header = hw.docked_ships.len };
            count += 1;
        }
        for (hw.docked_ships) |ship| {
            if (count >= rows.len) break;
            rows[count] = .{ .docked_ship = ship };
            count += 1;
        }

        // Fleets
        for (self.fleets.items, 0..) |fleet, fi| {
            if (count >= rows.len) break;
            const at_home = fleet.location.eql(player.homeworld);
            const header = FleetHeaderInfo{
                .fleet_id = fleet.id,
                .fleet_idx = fi,
                .state = fleet.state,
                .location = fleet.location,
                .ship_count = fleet.ships.len,
            };

            if (at_home and fleet.state != .in_combat) {
                rows[count] = .{ .fleet_header = header };
                count += 1;
                for (fleet.ships) |ship| {
                    if (count >= rows.len) break;
                    rows[count] = .{ .fleet_ship = .{ .ship = ship, .fleet_id = fleet.id } };
                    count += 1;
                }
            } else {
                rows[count] = .{ .deployed_header = header };
                count += 1;
            }
        }

        return rows;
    }

    pub fn fleetRowCount(self: *const ClientState) usize {
        const hw = self.homeworld orelse return 0;
        const player = self.player orelse return 0;

        var count: usize = 1; // docked header
        count += hw.docked_ships.len;

        for (self.fleets.items) |fleet| {
            count += 1; // fleet header
            const at_home = fleet.location.eql(player.homeworld);
            if (at_home and fleet.state != .in_combat) {
                count += fleet.ships.len;
            }
        }

        return count;
    }

    pub fn fleetManagerNav(self: *ClientState, nav: FleetManagerNav) ?protocol.Command {
        const total = self.fleetRowCount();
        if (total == 0) return null;

        switch (nav) {
            .cursor_up => {
                if (self.fleet_cursor > 0) self.fleet_cursor -= 1;
            },
            .cursor_down => {
                if (self.fleet_cursor + 1 < total) self.fleet_cursor += 1;
            },
            .create_fleet => {
                return .{ .create_fleet = {} };
            },
            .dock_ship => {
                const rows = self.buildFleetRows();
                if (self.fleet_cursor < rows.len) {
                    switch (rows[self.fleet_cursor]) {
                        .fleet_ship => |fs| return .{ .dock_ship = .{ .ship_id = fs.ship.id } },
                        else => {},
                    }
                }
                return null;
            },
            .assign_to_fleet => |fleet_idx| {
                // Find the fleet at this index
                if (fleet_idx >= self.fleets.items.len) return null;
                const target_fleet = self.fleets.items[fleet_idx];

                const rows = self.buildFleetRows();
                if (self.fleet_cursor < rows.len) {
                    switch (rows[self.fleet_cursor]) {
                        .docked_ship => |ship| return .{ .transfer_ship = .{
                            .ship_id = ship.id,
                            .fleet_id = target_fleet.id,
                        } },
                        else => {},
                    }
                }
                return null;
            },
            .dissolve_fleet => {
                const rows = self.buildFleetRows();
                if (self.fleet_cursor < rows.len) {
                    switch (rows[self.fleet_cursor]) {
                        .fleet_header => |fh| return .{ .dissolve_fleet = .{ .fleet_id = fh.fleet_id } },
                        else => {},
                    }
                }
                return null;
            },
        }
        return null;
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

    pub fn activeFleet(self: *const ClientState) ?*const protocol.FleetState {
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

    pub fn currentSector(self: *const ClientState) ?*const protocol.SectorState {
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
    homeworld,
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
    events: [MAX_EVENTS]protocol.GameEvent = undefined,
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

    pub fn push(self: *EventLog, event: protocol.GameEvent) !void {
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

    fn freeSlices(self: *EventLog, event: protocol.GameEvent) void {
        switch (event.kind) {
            .alert => |a| self.allocator.free(a.message),
            else => {},
        }
    }

    pub fn getRecent(self: *const EventLog, n: usize) ?protocol.GameEvent {
        if (n >= self.count) return null;
        const idx = (self.head + MAX_EVENTS - 1 - n) % MAX_EVENTS;
        return self.events[idx];
    }
};
