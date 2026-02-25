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

    if (state.show_keybinds) {
        renderKeybinds(frame, rows.get(1));
    } else switch (state.current_view) {
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

fn renderFooter(state: *ClientState, frame: *Frame, area: Rect) void {
    const text: []const u8 = switch (state.current_view) {
        .command_center => " CMD CENTER | [w] Windshield  [m] Map  [?] Keys",
        .windshield => " WINDSHIELD | [1-6] Move  [i] Info  [?] Keys",
        .star_map => " STAR MAP | [Arrows] Scroll  [z/x] Zoom  [?] Keys",
    };
    frame.render(Paragraph{
        .text = text,
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

    renderEventLog(state, frame, rows.get(1), " EVENT LOG ");
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

    if (state.show_sector_info)
        renderSectorInfo(state, frame, cols.get(0))
    else
        renderSectorView(state, frame, cols.get(0));
    renderFleetStatus(state, frame, cols.get(1));

    renderEventLog(state, frame, rows.get(1), " EVENTS ");
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

    if (f.ships.len == 0) {
        var buf: [256]u8 = undefined;
        const text = std.fmt.bufPrint(&buf, " Location: [{d},{d}]\n Status: NO SHIPS AVAILABLE\n\n All ships destroyed.\n [Esc] Return to Command Center", .{
            f.location.q, f.location.r,
        }) catch return;
        frame.render(Paragraph{ .text = text, .style = amber_bright }, inner);
        return;
    }

    if (inner.width < 40 or inner.height < 13) {
        renderSectorViewCompact(state, frame, inner, f);
        return;
    }

    const sector = state.currentSector() orelse {
        frame.render(Paragraph{ .text = " Sector data pending...", .style = amber_faint }, inner);
        return;
    };

    const loc = f.location;
    const came_from = findCameFrom(state, loc);

    // Center of the inner area
    const cx: i32 = @as(i32, @intCast(inner.x)) + @divTrunc(@as(i32, @intCast(inner.width)), 2);
    const cy: i32 = @as(i32, @intCast(inner.y)) + @divTrunc(@as(i32, @intCast(inner.height)), 2) - 1;

    // Node offsets: E, NE, NW, W, SW, SE
    const node_dx = [6]i32{ 17, 10, -10, -17, -10, 10 };
    const node_dy = [6]i32{ 0, -5, -5, 0, 5, 5 };

    for (shared.HexDirection.ALL, 0..) |dir, i| {
        if (sectorHasConnection(sector, loc.neighbor(dir))) {
            renderConnectionLine(frame, inner, cx, cy, node_dx[i], node_dy[i], if (came_from == i) amber else amber_dim);
        }
    }

    renderCenterNode(frame, inner, cx, cy, loc, sector.terrain.label());

    for (shared.HexDirection.ALL, 0..) |dir, i| {
        const nbr = loc.neighbor(dir);
        const nx = cx + node_dx[i];
        const ny = cy + node_dy[i];

        if (sectorHasConnection(sector, nbr)) {
            renderDirectionNode(frame, inner, nx, ny, dir, i, nbr, came_from == i, state.known_sectors.getPtr(nbr.toKey()));
        } else {
            renderDisconnectedNode(frame, inner, nx, ny, dir);
        }
    }

    if (sector.hostiles) |hostiles| {
        const hostile_y: u16 = @intCast(@as(i32, @intCast(inner.y)) + @as(i32, @intCast(inner.height)) - 2);
        var buf: [128]u8 = undefined;
        for (hostiles, 0..) |fleet_info, fi| {
            for (fleet_info.ships) |ship| {
                const text = std.fmt.bufPrint(&buf, " HOSTILE: {d}x {s} ({s})", .{
                    ship.count,
                    ship.class.label(),
                    @tagName(fleet_info.behavior),
                }) catch break;
                const row: u16 = hostile_y -| @as(u16, @intCast(fi));
                if (row >= inner.y and row < inner.y + inner.height) {
                    frame.render(Paragraph{ .text = text, .style = amber_bright }, Rect.init(inner.x, row, inner.width, 1));
                }
            }
        }
    }

    const kb_y = inner.y + inner.height -| 1;
    frame.render(Paragraph{
        .text = " [1-6] Move  [?] All Keys",
        .style = amber_dim,
    }, Rect.init(inner.x, kb_y, inner.width, 1));
}

fn renderSectorInfo(state: *ClientState, frame: *Frame, area: Rect) void {
    const block = Block{
        .title = " SECTOR INFO ",
        .border = .rounded,
        .border_style = amber_dim,
    };
    frame.render(block, area);
    const inner = block.inner(area);

    const fleet = state.activeFleet() orelse {
        frame.render(Paragraph{ .text = " No active fleet", .style = amber_faint }, inner);
        return;
    };

    const sector = state.currentSector() orelse {
        frame.render(Paragraph{ .text = " Sector data pending...", .style = amber_faint }, inner);
        return;
    };

    var buf: [1024]u8 = undefined;
    var pos: usize = 0;

    // Location + terrain
    const hdr = std.fmt.bufPrint(buf[pos..], " Location: [{d},{d}]\n Terrain:  {s}\n", .{
        fleet.location.q, fleet.location.r, sector.terrain.label(),
    }) catch return;
    pos += hdr.len;

    // Zone
    const dist = fleet.location.distFromOrigin();
    const zone_label: []const u8 = if (dist == 0) "Central Hub" else if (dist <= 8) "Inner Ring" else if (dist <= 20) "Outer Ring" else "The Wandering";
    const zone_line = std.fmt.bufPrint(buf[pos..], " Zone:     {s} (dist {d})\n", .{ zone_label, dist }) catch return;
    pos += zone_line.len;

    // Homeworld marker
    if (state.player) |p| {
        if (p.homeworld.eql(fleet.location)) {
            const hw = std.fmt.bufPrint(buf[pos..], " ** HOMEWORLD **\n", .{}) catch return;
            pos += hw.len;
        }
    }

    // Separator
    const sep = std.fmt.bufPrint(buf[pos..], "\n RESOURCES\n ─────────────────────\n", .{}) catch return;
    pos += sep.len;

    // Resources detail
    const res = sector.resources;
    const metal_line = std.fmt.bufPrint(buf[pos..], " Metal:     {s}\n", .{res.metal.label()}) catch return;
    pos += metal_line.len;
    const crystal_line = std.fmt.bufPrint(buf[pos..], " Crystal:   {s}\n", .{res.crystal.label()}) catch return;
    pos += crystal_line.len;
    const deut_line = std.fmt.bufPrint(buf[pos..], " Deuterium: {s}\n", .{res.deuterium.label()}) catch return;
    pos += deut_line.len;

    // Salvage
    if (sector.salvage) |salvage| {
        const sal_sep = std.fmt.bufPrint(buf[pos..], "\n SALVAGE\n ─────────────────────\n", .{}) catch return;
        pos += sal_sep.len;
        const sal = std.fmt.bufPrint(buf[pos..], " Fe {d:.0}  Cr {d:.0}  De {d:.0}\n", .{
            salvage.metal, salvage.crystal, salvage.deuterium,
        }) catch return;
        pos += sal.len;
    }

    // Hostiles
    if (sector.hostiles) |hostiles| {
        const h_sep = std.fmt.bufPrint(buf[pos..], "\n HOSTILES\n ─────────────────────\n", .{}) catch return;
        pos += h_sep.len;
        for (hostiles) |fleet_info| {
            for (fleet_info.ships) |ship| {
                const h_line = std.fmt.bufPrint(buf[pos..], " {d}x {s} ({s})\n", .{
                    ship.count, ship.class.label(), @tagName(fleet_info.behavior),
                }) catch break;
                pos += h_line.len;
            }
        }
    }

    // Connections
    const conn_sep = std.fmt.bufPrint(buf[pos..], "\n EXITS\n ─────────────────────\n", .{}) catch return;
    pos += conn_sep.len;
    const loc = fleet.location;
    for (shared.HexDirection.ALL, 0..) |dir, i| {
        const nbr = loc.neighbor(dir);
        if (sectorHasConnection(sector, nbr)) {
            const conn_line = std.fmt.bufPrint(buf[pos..], " [{d}] {s:>2} -> [{d},{d}]\n", .{
                i + 1, dir.label(), nbr.q, nbr.r,
            }) catch break;
            pos += conn_line.len;
        }
    }

    // Footer
    const footer = std.fmt.bufPrint(buf[pos..], "\n [i] Close Info  [1-6] Move  [h] Harvest", .{}) catch return;
    pos += footer.len;

    frame.render(Paragraph{ .text = buf[0..pos], .style = amber_bright }, inner);
}

fn renderKeybinds(frame: *Frame, area: Rect) void {
    const block = Block{
        .title = " KEYBINDS ",
        .border = .rounded,
        .border_style = amber_dim,
    };
    frame.render(block, area);
    const inner = block.inner(area);

    const text =
        " NAVIGATION\n" ++
        " ─────────────────────────────\n" ++
        " Esc       Command Center\n" ++
        " w         Windshield view\n" ++
        " m         Star Map view\n" ++
        " Tab       Cycle fleet\n" ++
        " q         Quit\n" ++
        "\n" ++
        " WINDSHIELD\n" ++
        " ─────────────────────────────\n" ++
        " 1-6       Move (hex direction)\n" ++
        " h         Harvest resources\n" ++
        " s         Collect salvage\n" ++
        " a         Attack hostile\n" ++
        " r         Recall to homeworld\n" ++
        " i         Sector info\n" ++
        "\n" ++
        " STAR MAP\n" ++
        " ─────────────────────────────\n" ++
        " Arrows    Scroll map\n" ++
        " z / x     Zoom out / in\n" ++
        " c         Center on fleet\n" ++
        "\n" ++
        " [?] Close";

    frame.render(Paragraph{ .text = text, .style = amber_bright }, inner);
}

fn renderCenterNode(frame: *Frame, inner: Rect, cx: i32, cy: i32, loc: shared.Hex, terrain_label: []const u8) void {
    var buf: [16]u8 = undefined;
    const coord_text = std.fmt.bufPrint(&buf, "[{d},{d}]", .{ loc.q, loc.r }) catch return;
    renderTextCentered(frame, inner, cx, cy - 1, coord_text, amber_bright);
    renderTextCentered(frame, inner, cx, cy, "<>", amber_full);
    renderTextCentered(frame, inner, cx, cy + 1, terrain_label, amber);
}

fn renderDirectionNode(
    frame: *Frame,
    inner: Rect,
    nx: i32,
    ny: i32,
    dir: shared.HexDirection,
    idx: usize,
    coord: shared.Hex,
    is_came_from: bool,
    sector_data: ?*const shared.protocol.SectorState,
) void {
    const explored = sector_data != null;
    const heading_style = if (is_came_from) amber_full else if (explored) amber_bright else amber;
    const detail_style = if (is_came_from or explored) amber else amber_dim;

    var heading_buf: [16]u8 = undefined;
    const heading = if (is_came_from)
        std.fmt.bufPrint(&heading_buf, "[{d}] {s} <", .{ idx + 1, dir.label() }) catch return
    else
        std.fmt.bufPrint(&heading_buf, "[{d}] {s}", .{ idx + 1, dir.label() }) catch return;

    var coord_buf: [16]u8 = undefined;
    const coord_text = std.fmt.bufPrint(&coord_buf, "[{d},{d}]", .{ coord.q, coord.r }) catch return;

    var resource_buf: [16]u8 = undefined;
    const resource_text = formatResourceSummary(&resource_buf, sector_data);

    renderTextCentered(frame, inner, nx, ny - 1, heading, heading_style);
    renderTextCentered(frame, inner, nx, ny, coord_text, heading_style);
    renderTextCentered(frame, inner, nx, ny + 1, resource_text, detail_style);
}

fn formatResourceSummary(buf: []u8, sector_data: ?*const shared.protocol.SectorState) []const u8 {
    const sd = sector_data orelse return "???";
    const res = sd.resources;
    if (res.metal == .none and res.crystal == .none and res.deuterium == .none) return "---";
    return std.fmt.bufPrint(buf, "Fe:{s} Cr:{s}", .{
        densityShort(res.metal), densityShort(res.crystal),
    }) catch "...";
}

fn renderDisconnectedNode(frame: *Frame, inner: Rect, nx: i32, ny: i32, dir: shared.HexDirection) void {
    renderTextCentered(frame, inner, nx, ny - 1, dir.label(), amber_faint);
    renderTextCentered(frame, inner, nx, ny, "---", amber_faint);
}

fn renderConnectionLine(frame: *Frame, inner: Rect, cx: i32, cy: i32, target_dx: i32, target_dy: i32, style: Style) void {
    if (target_dy == 0) {
        // Horizontal line (E or W)
        const step: i32 = if (target_dx > 0) 1 else -1;
        const start = cx + step * 3;
        const end = cx + target_dx - step * 5;
        var x = start;
        while ((step > 0 and x <= end) or (step < 0 and x >= end)) : (x += step) {
            renderTextAt(frame, inner, x, cy, "-", style);
        }
    } else {
        // Diagonal line (NE, NW, SW, SE)
        const steps: i32 = 4;
        var i: i32 = 1;
        while (i < steps) : (i += 1) {
            const lx = cx + @divTrunc(target_dx * i, steps);
            const ly = cy + @divTrunc(target_dy * i, steps);
            const ch: []const u8 = if (target_dy < 0)
                (if (target_dx > 0) "/" else "\\")
            else
                (if (target_dx > 0) "\\" else "/");
            renderTextAt(frame, inner, lx, ly, ch, style);
        }
    }
}

fn renderTextAt(frame: *Frame, inner: Rect, x: i32, y: i32, text: []const u8, style: Style) void {
    const ix: i32 = @intCast(inner.x);
    const iy: i32 = @intCast(inner.y);
    const iw: i32 = @intCast(inner.width);
    const ih: i32 = @intCast(inner.height);
    if (y < iy or y >= iy + ih) return;
    if (x >= ix + iw or x + @as(i32, @intCast(text.len)) <= ix) return;

    // Clamp to inner bounds
    const start_x: u16 = @intCast(@max(x, ix));
    const end_x: u16 = @intCast(@min(x + @as(i32, @intCast(text.len)), ix + iw));
    const text_offset: usize = @intCast(@as(i32, @intCast(start_x)) - x);
    const visible_len = end_x - start_x;

    frame.render(Paragraph{
        .text = text[text_offset..][0..visible_len],
        .style = style,
    }, Rect.init(start_x, @intCast(y), visible_len, 1));
}

fn renderTextCentered(frame: *Frame, inner: Rect, cx: i32, y: i32, text: []const u8, style: Style) void {
    renderTextAt(frame, inner, cx - @as(i32, @intCast(text.len / 2)), y, text, style);
}

fn findCameFrom(state: *const ClientState, loc: shared.Hex) ?usize {
    const prev = state.prev_fleet_location orelse return null;
    for (shared.HexDirection.ALL, 0..) |dir, i| {
        if (loc.neighbor(dir).eql(prev)) return i;
    }
    return null;
}

fn sectorHasConnection(sector: *const shared.protocol.SectorState, target: shared.Hex) bool {
    for (sector.connections) |conn| {
        if (conn.eql(target)) return true;
    }
    return false;
}

fn densityShort(d: shared.protocol.Density) []const u8 {
    return switch (d) {
        .none => "-",
        .sparse => "S",
        .moderate => "M",
        .rich => "R",
        .pristine => "P",
    };
}

fn renderSectorViewCompact(state: *const ClientState, frame: *Frame, inner: Rect, f: *const shared.protocol.FleetState) void {
    var buf: [512]u8 = undefined;
    var pos: usize = 0;

    const loc_line = std.fmt.bufPrint(buf[pos..], " Location: [{d},{d}]  Status: {s}\n", .{
        f.location.q, f.location.r, @tagName(f.state),
    }) catch return;
    pos += loc_line.len;

    if (state.currentSector()) |sector| {
        const terrain_line = std.fmt.bufPrint(buf[pos..], " Terrain: {s}\n", .{
            sector.terrain.label(),
        }) catch return;
        pos += terrain_line.len;

        const loc = f.location;
        const came_from = findCameFrom(state, loc);

        const exit_hdr = std.fmt.bufPrint(buf[pos..], " Exits:", .{}) catch return;
        pos += exit_hdr.len;

        for (shared.HexDirection.ALL, 0..) |dir, i| {
            const nbr = loc.neighbor(dir);
            if (sectorHasConnection(sector, nbr)) {
                const marker: []const u8 = if (came_from == i) "<" else " ";
                const exit_line = std.fmt.bufPrint(buf[pos..], "\n  [{d}] {s:>2} -> [{d},{d}]{s}", .{
                    i + 1, dir.label(), nbr.q, nbr.r, marker,
                }) catch break;
                pos += exit_line.len;
            }
        }
    } else {
        const no_sec = std.fmt.bufPrint(buf[pos..], " Sector data pending...\n", .{}) catch return;
        pos += no_sec.len;
    }

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

    var cargo_cap: u16 = 0;
    var hull_cur: f32 = 0;
    var hull_max: f32 = 0;
    var shield_cur: f32 = 0;
    var shield_max: f32 = 0;
    var dps: f32 = 0;
    for (fleet.ships) |ship| {
        cargo_cap += ship.class.baseStats().cargo;
        hull_cur += ship.hull;
        hull_max += ship.hull_max;
        shield_cur += ship.shield;
        shield_max += ship.shield_max;
        dps += ship.weapon_power;
    }
    const total_cargo = fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium;

    var buf: [512]u8 = undefined;
    const text = std.fmt.bufPrint(&buf,
        " Ships: {s}\n Fuel:  {d:.0}/{d:.0}\n" ++
        "\n Hull:   {d:.0}/{d:.0}" ++
        "\n Shield: {d:.0}/{d:.0}" ++
        "\n DPS:    {d:.0}" ++
        "\n\n Cargo: {d:.0}/{d}\n  Fe {d:.0}\n  Cr {d:.0}\n  De {d:.0}"
    , .{
        ships_str,
        fleet.fuel,
        fleet.fuel_max,
        hull_cur,
        hull_max,
        shield_cur,
        shield_max,
        dps,
        total_cargo,
        cargo_cap,
        fleet.cargo.metal,
        fleet.cargo.crystal,
        fleet.cargo.deuterium,
    }) catch return;

    frame.render(Paragraph{ .text = text, .style = amber_bright }, inner);
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

// -- Star Map ---------------------------------------------------------------

fn renderStarMap(state: *ClientState, frame: *Frame, area: Rect) void {
    // Layout: map area (flexible) + legend bar (3 rows)
    const rows = frame.layout(area, .vertical, &.{
        Constraint.flexible(1),
        Constraint.len(3),
    });

    renderHexGrid(state, frame, rows.get(0));
    renderMapLegend(state, frame, rows.get(1));
}

fn zoomRadius(zoom: State.ZoomLevel) u16 {
    return switch (zoom) {
        .close => 5,
        .sector => 10,
        .region => 20,
    };
}

fn renderHexGrid(state: *ClientState, frame: *Frame, area: Rect) void {
    const block = Block{
        .title = " STAR MAP ",
        .border = .rounded,
        .border_style = amber_dim,
    };
    frame.render(block, area);
    const inner = block.inner(area);

    if (inner.width < 3 or inner.height < 3) return;

    const center = state.map_center;
    const radius = zoomRadius(state.map_zoom);

    const ix: i32 = @intCast(inner.x);
    const iy: i32 = @intCast(inner.y);
    const iw: i32 = @intCast(inner.width);
    const ih: i32 = @intCast(inner.height);
    const vp_cx: i32 = ix + @divTrunc(iw, 2);
    const vp_cy: i32 = iy + @divTrunc(ih, 2);

    var spiral = shared.hex.hexSpiral(center, radius);
    while (spiral.next()) |coord| {
        const dq: i32 = @as(i32, coord.q) - @as(i32, center.q);
        const dr: i32 = @as(i32, coord.r) - @as(i32, center.r);
        const screen_x: i32 = vp_cx + dq * 2 + dr;
        const screen_y: i32 = vp_cy + dr;

        if (screen_x < ix or screen_x >= ix + iw) continue;
        if (screen_y < iy or screen_y >= iy + ih) continue;

        const cell = classifyHex(state, coord);
        frame.render(Paragraph{
            .text = cell.symbol,
            .style = cell.style,
        }, Rect.init(@intCast(screen_x), @intCast(screen_y), 1, 1));
    }
}

const HexCell = struct {
    symbol: []const u8,
    style: Style,
};

fn classifyHex(state: *ClientState, coord: shared.Hex) HexCell {
    // Active fleet position
    if (state.activeFleet()) |fleet| {
        if (fleet.location.eql(coord)) {
            return .{ .symbol = "@", .style = amber_full };
        }
    }

    // Other player fleets
    for (state.fleets.items) |fleet| {
        if (fleet.location.eql(coord)) {
            return .{ .symbol = "A", .style = amber_bright };
        }
    }

    // Homeworld
    if (state.player) |p| {
        if (p.homeworld.eql(coord)) {
            return .{ .symbol = "H", .style = amber_full };
        }
    }

    // Known sector (explored)
    if (state.known_sectors.get(coord.toKey())) |sector| {
        if (sector.hostiles != null) {
            return .{ .symbol = "!", .style = amber_bright };
        }

        // Sector with resources
        const has_resources = sector.resources.metal != .none or
            sector.resources.crystal != .none or
            sector.resources.deuterium != .none;
        if (has_resources) {
            return .{ .symbol = sector.terrain.symbol(), .style = amber };
        }

        // Explored empty sector
        return .{ .symbol = sector.terrain.symbol(), .style = amber_dim };
    }

    // Fog of war: adjacent to explored sector
    for (coord.neighbors()) |n| {
        if (state.known_sectors.contains(n.toKey())) {
            return .{ .symbol = ".", .style = amber_faint };
        }
    }

    // Completely unknown
    return .{ .symbol = " ", .style = amber_faint };
}

fn renderMapLegend(state: *ClientState, frame: *Frame, area: Rect) void {
    var buf: [256]u8 = undefined;
    const center = state.map_center;
    const zoom_label: []const u8 = switch (state.map_zoom) {
        .close => "CLOSE",
        .sector => "SECTOR",
        .region => "REGION",
    };
    const text = std.fmt.bufPrint(&buf, " [{d},{d}] Zoom:{s} | @=You H=Home !=Hostile .=Fog | [Arrows]Scroll [z/x]Zoom [c]Center [Tab]Fleet", .{
        center.q, center.r, zoom_label,
    }) catch " STAR MAP";
    frame.render(Paragraph{ .text = text, .style = amber_dim }, area);
}
