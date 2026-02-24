const std = @import("std");
const shared = @import("shared");
const zithril = @import("zithril");
const State = @import("state.zig");

const ClientState = State.ClientState;

const Frame = zithril.Frame(zithril.App(ClientState).DefaultMaxWidgets);
const Constraint = zithril.Constraint;
const Block = zithril.Block;
const Paragraph = zithril.Paragraph;
const Rect = zithril.Rect;
const Style = zithril.Style;
const Color = zithril.Color;

// -- Amber color palette ----------------------------------------------------

pub const amber_faint = Style.init().fg(Color.fromRgb(30, 21, 0));
pub const amber_dim = Style.init().fg(Color.fromRgb(77, 53, 0));
pub const amber = Style.init().fg(Color.fromRgb(153, 106, 0));
pub const amber_bright = Style.init().fg(Color.fromRgb(217, 150, 0));
pub const amber_full = Style.init().fg(Color.fromRgb(255, 176, 0));

// -- View dispatch ----------------------------------------------------------

pub fn view(state: *ClientState, frame: *Frame) void {
    const area = frame.size();
    if (area.width < 10 or area.height < 5) return;

    // Header (2 rows) + body + footer (1 row)
    const rows = frame.layout(area, .vertical, &.{
        Constraint.len(2),
        Constraint.flexible(1),
        Constraint.len(1),
    });

    renderHeader(state, frame, rows.get(0));

    switch (state.current_view) {
        .command_center => renderCommandCenter(state, frame, rows.get(1)),
        .windshield => renderWindshield(state, frame, rows.get(1)),
        .star_map => renderStarMap(state, frame, rows.get(1)),
    }

    renderFooter(state, frame, rows.get(2));
}

// -- Header -----------------------------------------------------------------

fn renderHeader(state: *ClientState, frame: *Frame, area: Rect) void {
    var buf: [256]u8 = undefined;
    const header_text = std.fmt.bufPrint(&buf, " IN AMBER CLAD v0.1 | TICK: {d} | {s} | {s}", .{
        state.tick,
        if (state.player) |p| p.name else "---",
        switch (state.current_view) {
            .command_center => "COMMAND CENTER",
            .windshield => "WINDSHIELD",
            .star_map => "STAR MAP",
        },
    }) catch " IN AMBER CLAD v0.1";

    frame.render(Paragraph{
        .text = header_text,
        .style = amber_full,
    }, area);
}

// -- Footer -----------------------------------------------------------------

fn renderFooter(_: *ClientState, frame: *Frame, area: Rect) void {
    frame.render(Paragraph{
        .text = " [Esc] Cmd Center  [w] Windshield  [m] Map  [Tab] Cycle Fleet  [q] Quit",
        .style = amber_dim,
    }, area);
}

// -- Command Center ---------------------------------------------------------

fn renderCommandCenter(state: *ClientState, frame: *Frame, area: Rect) void {
    // Split into top panels and bottom event log
    const rows = frame.layout(area, .vertical, &.{
        Constraint.len(8),
        Constraint.flexible(1),
    });

    // Top: resources + fleets side by side
    const top_cols = frame.layout(rows.get(0), .horizontal, &.{
        Constraint.flexible(1),
        Constraint.flexible(1),
    });

    renderResourcePanel(state, frame, top_cols.get(0));
    renderFleetPanel(state, frame, top_cols.get(1));

    renderEventPanel(state, frame, rows.get(1));
}

fn renderResourcePanel(state: *ClientState, frame: *Frame, area: Rect) void {
    const block = Block{
        .title = " RESOURCES ",
        .border = .rounded,
        .border_style = amber_dim,
    };
    frame.render(block, area);
    const inner = block.inner(area);

    if (state.player) |player| {
        var buf: [128]u8 = undefined;
        const text = std.fmt.bufPrint(&buf, " Metal   {d:>8.0}\n Crystal {d:>8.0}\n Deut    {d:>8.0}", .{
            player.resources.metal,
            player.resources.crystal,
            player.resources.deuterium,
        }) catch return;
        frame.render(Paragraph{ .text = text, .style = amber_bright }, inner);
    } else {
        frame.render(Paragraph{ .text = " No player data", .style = amber_faint }, inner);
    }
}

fn renderFleetPanel(state: *ClientState, frame: *Frame, area: Rect) void {
    const block = Block{
        .title = " FLEETS ",
        .border = .rounded,
        .border_style = amber_dim,
    };
    frame.render(block, area);
    const inner = block.inner(area);

    if (state.fleets.items.len == 0) {
        frame.render(Paragraph{ .text = " No fleets", .style = amber_faint }, inner);
        return;
    }

    var buf: [512]u8 = undefined;
    var pos: usize = 0;
    for (state.fleets.items, 0..) |fleet, i| {
        const marker: []const u8 = if (i == state.active_fleet_idx) ">" else " ";
        const line = std.fmt.bufPrint(buf[pos..], " {s}Fleet {d} [{d},{d}] {s}\n", .{
            marker,
            fleet.id,
            fleet.location.q,
            fleet.location.r,
            @tagName(fleet.state),
        }) catch break;
        pos += line.len;
    }

    frame.render(Paragraph{ .text = buf[0..pos], .style = amber }, inner);
}

fn renderEventPanel(state: *ClientState, frame: *Frame, area: Rect) void {
    renderEventLog(state, frame, area, " EVENT LOG ");
}

fn formatEvent(buf: []u8, pos: usize, event: shared.protocol.GameEvent) !usize {
    const slice = buf[pos..];
    const text = switch (event.kind) {
        .combat_started => |e| std.fmt.bufPrint(slice, " T{d}: Combat at [{d},{d}]\n", .{ event.tick, e.sector.q, e.sector.r }),
        .combat_ended => |e| std.fmt.bufPrint(slice, " T{d}: Combat {s} at [{d},{d}]\n", .{
            event.tick,
            if (e.player_victory) "WON" else "LOST",
            e.sector.q,
            e.sector.r,
        }),
        .sector_entered => |e| std.fmt.bufPrint(slice, " T{d}: Entered [{d},{d}]\n", .{ event.tick, e.sector.q, e.sector.r }),
        .resource_harvested => |e| std.fmt.bufPrint(slice, " T{d}: Harvested {d:.1} {s}\n", .{ event.tick, e.amount, @tagName(e.resource_type) }),
        .ship_destroyed => |e| std.fmt.bufPrint(slice, " T{d}: {s} destroyed\n", .{ event.tick, e.ship_class.label() }),
        .combat_round => |e| std.fmt.bufPrint(slice, " T{d}: Hit for {d:.0} dmg ({d:.0} absorbed)\n", .{ event.tick, e.hull_damage, e.shield_absorbed }),
        .fleet_destroyed => |e| std.fmt.bufPrint(slice, " T{d}: {s} fleet destroyed!\n", .{ event.tick, if (e.is_npc) "Enemy" else "Your" }),
        .alert => |e| std.fmt.bufPrint(slice, " T{d}: [{s}] {s}\n", .{ event.tick, @tagName(e.level), e.message }),
        else => std.fmt.bufPrint(slice, " T{d}: Event\n", .{event.tick}),
    } catch return error.NoSpaceLeft;
    return text.len;
}

// -- Windshield View --------------------------------------------------------

fn renderWindshield(state: *ClientState, frame: *Frame, area: Rect) void {
    // Top section + bottom event log
    const rows = frame.layout(area, .vertical, &.{
        Constraint.flexible(1),
        Constraint.len(7),
    });

    // Top: sector view + fleet sidebar
    const cols = frame.layout(rows.get(0), .horizontal, &.{
        Constraint.flexible(1),
        Constraint.len(30),
    });

    renderSectorView(state, frame, cols.get(0));
    renderFleetStatus(state, frame, cols.get(1));

    renderWindshieldEvents(state, frame, rows.get(1));
}

fn renderSectorView(state: *ClientState, frame: *Frame, area: Rect) void {
    const fleet = state.activeFleet();
    const in_combat = if (fleet) |f| f.state == .in_combat else false;

    const block = Block{
        .title = if (in_combat) " !! COMBAT !! " else " SECTOR VIEW ",
        .border = .rounded,
        .border_style = if (in_combat) amber_full else amber_dim,
    };
    frame.render(block, area);
    const inner = block.inner(area);

    const f = fleet orelse {
        frame.render(Paragraph{ .text = " No active fleet", .style = amber_faint }, inner);
        return;
    };

    // Death state: all ships destroyed
    if (f.ships.len == 0) {
        var buf: [256]u8 = undefined;
        const text = std.fmt.bufPrint(&buf, " Location: [{d},{d}]\n Status: NO SHIPS AVAILABLE\n\n All ships destroyed.\n [Esc] Return to Command Center", .{
            f.location.q, f.location.r,
        }) catch return;
        frame.render(Paragraph{ .text = text, .style = amber_bright }, inner);
        return;
    }

    var buf: [1024]u8 = undefined;
    var pos: usize = 0;

    // Current position
    const loc_line = std.fmt.bufPrint(buf[pos..], " Location: [{d},{d}]\n", .{
        f.location.q, f.location.r,
    }) catch return;
    pos += loc_line.len;

    const status_line = std.fmt.bufPrint(buf[pos..], " Status: {s}\n", .{
        @tagName(f.state),
    }) catch return;
    pos += status_line.len;

    if (state.currentSector()) |sector| {
        // Sector info
        const terrain_line = std.fmt.bufPrint(buf[pos..], " Terrain: {s}\n", .{
            sector.terrain.label(),
        }) catch return;
        pos += terrain_line.len;

        const res = sector.resources;
        if (res.metal != .none or res.crystal != .none or res.deuterium != .none) {
            const res_line = std.fmt.bufPrint(buf[pos..], " Resources: Fe:{s} Cr:{s} De:{s}\n", .{
                res.metal.label(), res.crystal.label(), res.deuterium.label(),
            }) catch return;
            pos += res_line.len;
        }

        if (sector.hostiles) |hostiles| {
            for (hostiles) |fleet_info| {
                for (fleet_info.ships) |ship| {
                    const hostile_line = std.fmt.bufPrint(buf[pos..], " HOSTILE: {d}x {s} ({s})\n", .{
                        ship.count,
                        ship.class.label(),
                        @tagName(fleet_info.behavior),
                    }) catch break;
                    pos += hostile_line.len;
                }
            }
        }

        // Hex compass + exits
        const HexDir = shared.HexDirection;
        const loc = f.location;

        var connected: [6]bool = .{ false, false, false, false, false, false };
        var nbrs: [6]shared.Hex = undefined;
        for (HexDir.ALL, 0..) |dir, i| {
            nbrs[i] = loc.neighbor(dir);
            for (sector.connections) |conn| {
                if (conn.eql(nbrs[i])) {
                    connected[i] = true;
                    break;
                }
            }
        }

        const compass = std.fmt.bufPrint(buf[pos..],
            \\
            \\        {c}  NE
            \\     {c} / \ {c}  NW / \ E
            \\       |*|
            \\     {c} \ / {c}  W  \ / SE
            \\        {c}  SW
            \\
            \\
        , .{
            dirKey(connected[1], 1),
            dirKey(connected[2], 2),
            dirKey(connected[0], 0),
            dirKey(connected[3], 3),
            dirKey(connected[5], 5),
            dirKey(connected[4], 4),
        }) catch return;
        pos += compass.len;

        const exit_hdr = std.fmt.bufPrint(buf[pos..], " Exits:\n", .{}) catch return;
        pos += exit_hdr.len;

        for (HexDir.ALL, 0..) |dir, i| {
            if (connected[i]) {
                const exit_line = std.fmt.bufPrint(buf[pos..], "  [{d}] {s:>2} -> [{d},{d}]\n", .{
                    i + 1,
                    dir.label(),
                    nbrs[i].q,
                    nbrs[i].r,
                }) catch break;
                pos += exit_line.len;
            }
        }
    } else {
        const no_sec = std.fmt.bufPrint(buf[pos..], " Sector data pending...\n", .{}) catch return;
        pos += no_sec.len;
    }

    // Keybinds
    const keys = std.fmt.bufPrint(buf[pos..], "\n [1-6] Move  [h] Harvest  [r] Recall", .{}) catch return;
    pos += keys.len;

    frame.render(Paragraph{ .text = buf[0..pos], .style = amber }, inner);
}

fn renderFleetStatus(state: *ClientState, frame: *Frame, area: Rect) void {
    const block = Block{
        .title = " FLEET ",
        .border = .rounded,
        .border_style = amber_dim,
    };
    frame.render(block, area);
    const inner = block.inner(area);

    const fleet = state.activeFleet() orelse {
        frame.render(Paragraph{ .text = " ---", .style = amber_faint }, inner);
        return;
    };

    var ships_buf: [20]u8 = undefined;
    const ships_str: []const u8 = if (fleet.ships.len == 0)
        "0 (DESTROYED)"
    else
        std.fmt.bufPrint(&ships_buf, "{d}", .{fleet.ships.len}) catch "?";

    var buf: [256]u8 = undefined;
    const text = std.fmt.bufPrint(&buf, " Ships: {s}\n Fuel:  {d:.0}/{d:.0}\n\n Cargo:\n  Fe {d:.0}\n  Cr {d:.0}\n  De {d:.0}", .{
        ships_str,
        fleet.fuel,
        fleet.fuel_max,
        fleet.cargo.metal,
        fleet.cargo.crystal,
        fleet.cargo.deuterium,
    }) catch return;

    frame.render(Paragraph{ .text = text, .style = amber_bright }, inner);
}

fn renderWindshieldEvents(state: *ClientState, frame: *Frame, area: Rect) void {
    renderEventLog(state, frame, area, " EVENTS ");
}

fn renderEventLog(state: *ClientState, frame: *Frame, area: Rect, title: []const u8) void {
    const block = Block{
        .title = title,
        .border = .rounded,
        .border_style = amber_dim,
    };
    frame.render(block, area);
    const inner = block.inner(area);

    var buf: [1024]u8 = undefined;
    var pos: usize = 0;
    const max_lines: usize = if (inner.height > 0) inner.height else 1;

    var i: usize = 0;
    while (i < max_lines) : (i += 1) {
        if (state.event_log.getRecent(i)) |event| {
            const line = formatEvent(&buf, pos, event) catch break;
            pos += line;
        } else break;
    }

    frame.render(Paragraph{ .text = buf[0..pos], .style = amber_dim }, inner);
}

fn dirKey(is_connected: bool, idx: usize) u8 {
    return if (is_connected) "123456"[idx] else '.';
}

// -- Star Map ---------------------------------------------------------------

fn renderStarMap(_: *ClientState, frame: *Frame, area: Rect) void {
    const block = Block{
        .title = " STAR MAP ",
        .border = .rounded,
        .border_style = amber_dim,
    };
    frame.render(block, area);
    const inner = block.inner(area);

    frame.render(Paragraph{
        .text = " Star map rendering -- use arrows to scroll\n\n [Arrows] Scroll  [Tab] Cycle fleet",
        .style = amber_faint,
    }, inner);
}
