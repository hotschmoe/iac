const std = @import("std");
const shared = @import("shared");
const zithril = @import("zithril");
const State = @import("state.zig");
const protocol = shared.protocol;
const scaling = shared.scaling;
const ShipClass = shared.constants.ShipClass;
const Resources = shared.constants.Resources;
const Zone = shared.constants.Zone;

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
    } else if (state.show_tech_tree and state.current_view == .homeworld) {
        renderTechTree(state, frame, rows.get(1));
    } else switch (state.current_view) {
        .command_center => renderCommandCenter(state, frame, rows.get(1)),
        .windshield => renderWindshield(state, frame, rows.get(1)),
        .star_map => renderStarMap(state, frame, rows.get(1)),
        .homeworld => renderHomeworld(state, frame, rows.get(1)),
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
            .homeworld => "HOMEWORLD",
        },
    }) catch " IN AMBER CLAD v0.1";

    frame.render(Paragraph{
        .text = header_text,
        .style = amber_full,
    }, area);
}

// -- Footer -----------------------------------------------------------------

fn renderFooter(state: *ClientState, frame: *Frame, area: Rect) void {
    const text: []const u8 = if (state.show_tech_tree and state.current_view == .homeworld)
        " TECH TREE | [t] Close  [?] Keys"
    else switch (state.current_view) {
        .command_center => " CMD CENTER | [w] Windshield  [m] Map  [b] Base  [?] Keys",
        .windshield => " WINDSHIELD | [1-6] Move  [i] Info  [b] Base  [?] Keys",
        .star_map => " STAR MAP | [Arrows] Scroll  [z/x] Zoom  [b] Base  [?] Keys",
        .homeworld => switch (state.homeworld_tab) {
            .fleets => " FLEETS | Tab Switch  Up/Down Select  [n] New  [d] Dock  [1-3] Assign  [x] Dissolve  [?] Keys",
            .inventory => " INVENTORY | Tab Switch  [?] Keys",
            else => " HOMEWORLD | Tab Switch  Arrows Select  Enter Build  [x/X/z] Cancel  [t] Tree  [?] Keys",
        },
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

fn formatEvent(buf: []u8, pos: usize, event: protocol.GameEvent) !usize {
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
        .building_completed => |e| std.fmt.bufPrint(slice, " T{d}: {s} upgraded to Lv{d}\n", .{ event.tick, e.building_type.label(), e.new_level }),
        .research_completed => |e| std.fmt.bufPrint(slice, " T{d}: {s} {d} researched\n", .{ event.tick, e.tech.label(), e.new_level }),
        .ship_built => |e| std.fmt.bufPrint(slice, " T{d}: {s} built\n", .{ event.tick, e.ship_class.label() }),
        .loot_acquired => |e| switch (e.loot_type) {
            .component => |c| std.fmt.bufPrint(slice, " T{d}: Found {s} ({s})\n", .{ event.tick, c.component_type.label(), c.rarity.label() }),
            .data_fragment => |f| std.fmt.bufPrint(slice, " T{d}: +{d} {s}\n", .{ event.tick, f.count, f.fragment_type.label() }),
        },
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

    // Allied fleets
    if (sector.player_fleets) |allies| {
        if (allies.len > 0) {
            const a_sep = std.fmt.bufPrint(buf[pos..], "\n ALLIED FLEETS\n ─────────────────────\n", .{}) catch return;
            pos += a_sep.len;
            for (allies) |ally| {
                const a_line = std.fmt.bufPrint(buf[pos..], " {s} -- {d} ships\n", .{
                    ally.owner_name, ally.ship_count,
                }) catch break;
                pos += a_line.len;
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
        " b         Homeworld base\n" ++
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
        " HOMEWORLD\n" ++
        " ─────────────────────────────\n" ++
        " Tab       Cycle panel\n" ++
        " Arrows    Navigate cards\n" ++
        " Enter     Build/Research\n" ++
        " x/X/z     Cancel bld/ship/res\n" ++
        " t         Tech tree\n" ++
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
    sector_data: ?*const protocol.SectorState,
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

fn formatResourceSummary(buf: []u8, sector_data: ?*const protocol.SectorState) []const u8 {
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

fn sectorHasConnection(sector: *const protocol.SectorState, target: shared.Hex) bool {
    for (sector.connections) |conn| {
        if (conn.eql(target)) return true;
    }
    return false;
}

fn queueProgress(tick: u64, start: u64, end: u64) struct { elapsed: u64, total: u64 } {
    return .{
        .elapsed = if (tick > start) tick - start else 0,
        .total = if (end > start) end - start else 1,
    };
}

fn densityShort(d: protocol.Density) []const u8 {
    return switch (d) {
        .none => "-",
        .sparse => "S",
        .moderate => "M",
        .rich => "R",
        .pristine => "P",
    };
}

fn renderSectorViewCompact(state: *const ClientState, frame: *Frame, inner: Rect, f: *const protocol.FleetState) void {
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

// -- Homeworld View ---------------------------------------------------------

fn renderHomeworld(state: *ClientState, frame: *Frame, area: Rect) void {
    const rows = frame.layout(area, .vertical, &.{
        Constraint.len(1), // tab bar
        Constraint.flexible(1), // content
        Constraint.len(3), // status bar
    });

    renderTabBar(state, frame, rows.get(0));
    switch (state.homeworld_tab) {
        .fleets => renderFleetManager(state, frame, rows.get(1)),
        .inventory => renderInventory(state, frame, rows.get(1)),
        else => renderCardGrid(state, frame, rows.get(1)),
    }
    renderStatusBar(state, frame, rows.get(2));
}

fn renderTabBar(state: *ClientState, frame: *Frame, area: Rect) void {
    const tabs = [_]struct { tab: State.HomeworldTab, label: []const u8 }{
        .{ .tab = .buildings, .label = "Buildings" },
        .{ .tab = .shipyard, .label = "Shipyard" },
        .{ .tab = .research, .label = "Research" },
        .{ .tab = .fleets, .label = "Fleets" },
        .{ .tab = .inventory, .label = "Inventory" },
    };

    var buf: [128]u8 = undefined;
    var pos: usize = 0;
    for (tabs) |t| {
        const active = state.homeworld_tab == t.tab;
        const segment = if (active)
            std.fmt.bufPrint(buf[pos..], " [*{s}] ", .{t.label})
        else
            std.fmt.bufPrint(buf[pos..], " [ {s} ] ", .{t.label});
        pos += (segment catch break).len;
    }
    frame.render(Paragraph{ .text = buf[0..pos], .style = amber_full }, area);
}

fn renderFleetManager(state: *ClientState, frame: *Frame, area: Rect) void {
    const block = Block{
        .title = " FLEET MANAGER ",
        .border = .rounded,
        .border_style = amber_dim,
    };
    frame.render(block, area);
    const inner = block.inner(area);

    if (inner.height < 2) return;

    const rows_data = state.buildFleetRows();
    const total = state.fleetRowCount();
    if (total == 0) {
        frame.render(Paragraph{ .text = " No fleet data", .style = amber_faint }, inner);
        return;
    }

    // Calculate visible window (scrolling)
    const visible: usize = @intCast(inner.height - 1); // reserve 1 for legend
    const scroll_offset: usize = if (state.fleet_cursor >= visible)
        state.fleet_cursor - visible + 1
    else
        0;

    var buf: [2048]u8 = undefined;
    var pos: usize = 0;
    var rendered: usize = 0;

    for (0..total) |i| {
        if (i < scroll_offset) continue;
        if (rendered >= visible) break;

        const is_selected = (i == state.fleet_cursor);
        const cursor_char: u8 = if (is_selected) '>' else ' ';

        switch (rows_data[i]) {
            .docked_header => |count| {
                const l = std.fmt.bufPrint(buf[pos..], "{c} DOCKED ({d} ships)\n", .{ cursor_char, count }) catch break;
                pos += l.len;
            },
            .docked_ship => |ship| {
                const l = std.fmt.bufPrint(buf[pos..], "{c}   {s: <10} {d:.0}/{d:.0} hull  {d:.0}/{d:.0} shld\n", .{
                    cursor_char,
                    ship.class.label(),
                    ship.hull,
                    ship.hull_max,
                    ship.shield,
                    ship.shield_max,
                }) catch break;
                pos += l.len;
            },
            .fleet_header => |fh| {
                const state_str: []const u8 = switch (fh.state) {
                    .idle => "idle",
                    .moving => "moving",
                    .harvesting => "harvesting",
                    .in_combat => "COMBAT",
                    .returning => "returning",
                    .docked => "docked",
                };
                const l = std.fmt.bufPrint(buf[pos..], "{c} FLEET {d} [{s} @ home] ({d} ships)\n", .{
                    cursor_char,
                    fh.fleet_idx + 1,
                    state_str,
                    fh.ship_count,
                }) catch break;
                pos += l.len;
            },
            .fleet_ship => |fs| {
                const l = std.fmt.bufPrint(buf[pos..], "{c}   {s: <10} {d:.0}/{d:.0} hull  {d:.0}/{d:.0} shld\n", .{
                    cursor_char,
                    fs.ship.class.label(),
                    fs.ship.hull,
                    fs.ship.hull_max,
                    fs.ship.shield,
                    fs.ship.shield_max,
                }) catch break;
                pos += l.len;
            },
            .deployed_header => |fh| {
                const l = std.fmt.bufPrint(buf[pos..], "{c} FLEET {d} [deployed {d},{d}] ({d} ships)\n", .{
                    cursor_char,
                    fh.fleet_idx + 1,
                    fh.location.q,
                    fh.location.r,
                    fh.ship_count,
                }) catch break;
                pos += l.len;
            },
        }
        rendered += 1;
    }

    // Legend
    const legend = "[n] New Fleet  [d] Dock Ship  [1-3] Assign  [x] Dissolve";
    const ll = std.fmt.bufPrint(buf[pos..], "{s}", .{legend}) catch "";
    pos += ll.len;

    frame.render(Paragraph{ .text = buf[0..pos], .style = amber_bright }, inner);
}

fn renderInventory(state: *ClientState, frame: *Frame, area: Rect) void {
    const block = Block{
        .title = " INVENTORY ",
        .border = .rounded,
        .border_style = amber_dim,
    };
    frame.render(block, area);
    const inner = block.inner(area);

    const hw = state.homeworld orelse {
        frame.render(Paragraph{ .text = " No homeworld data", .style = amber_faint }, inner);
        return;
    };

    var buf: [2048]u8 = undefined;
    var pos: usize = 0;

    // Components section
    const comp_hdr = std.fmt.bufPrint(buf[pos..], " COMPONENTS\n", .{}) catch "";
    pos += comp_hdr.len;

    if (hw.components.len == 0) {
        const l = std.fmt.bufPrint(buf[pos..], "   (none acquired)\n", .{}) catch "";
        pos += l.len;
    } else {
        for (hw.components) |comp| {
            const bonus_pct: f32 = comp.component_type.bonusPerLevel() * @as(f32, @floatFromInt(comp.level)) * 100.0;
            const l = std.fmt.bufPrint(buf[pos..], "   {s: <22} Lv.{d}  +{d:.0}%\n", .{
                comp.component_type.label(),
                comp.level,
                bonus_pct,
            }) catch break;
            pos += l.len;
        }
    }

    // Separator
    const sep = std.fmt.bufPrint(buf[pos..], "\n DATA FRAGMENTS\n", .{}) catch "";
    pos += sep.len;

    if (hw.fragments.len == 0) {
        const l = std.fmt.bufPrint(buf[pos..], "   (none collected)\n", .{}) catch "";
        pos += l.len;
    } else {
        for (hw.fragments) |frag| {
            const l = std.fmt.bufPrint(buf[pos..], "   {s: <22} x{d}\n", .{
                frag.fragment_type.label(),
                frag.count,
            }) catch break;
            pos += l.len;
        }
    }

    frame.render(Paragraph{ .text = buf[0..pos], .style = amber_bright }, inner);
}

fn renderCardGrid(state: *ClientState, frame: *Frame, area: Rect) void {
    const hw = state.homeworld orelse {
        frame.render(Paragraph{ .text = " No homeworld data", .style = amber_faint }, area);
        return;
    };

    const bldg_levels = ClientState.buildingLevelsFromSlice(hw.buildings);
    const res_levels = ClientState.researchLevelsFromSlice(hw.research);

    const count = state.homeworld_tab.itemCount();
    const num_rows: u16 = @intCast((count + 1) / 2);
    const card_height: u16 = if (num_rows > 0 and area.height >= num_rows)
        area.height / num_rows
    else
        6;

    var row_constraints: [6]Constraint = undefined;
    for (0..num_rows) |i| {
        row_constraints[i] = Constraint.len(card_height);
    }
    const grid_rows = frame.layout(area, .vertical, row_constraints[0..num_rows]);

    for (0..num_rows) |ri| {
        const cols = frame.layout(grid_rows.get(@intCast(ri)), .horizontal, &.{
            Constraint.flexible(1),
            Constraint.flexible(1),
        });

        const left_idx = ri * 2;
        const right_idx = ri * 2 + 1;

        renderCard(state, frame, cols.get(0), left_idx, bldg_levels, res_levels);
        if (right_idx < count) {
            renderCard(state, frame, cols.get(1), right_idx, bldg_levels, res_levels);
        }
    }
}

fn renderCard(
    state: *ClientState,
    frame: *Frame,
    area: Rect,
    idx: usize,
    bldg_levels: scaling.BuildingLevels,
    res_levels: scaling.ResearchLevels,
) void {
    const is_selected = state.homeworld_cursor == idx;

    var title_buf: [32]u8 = undefined;
    var content_buf: [256]u8 = undefined;
    var cpos: usize = 0;
    var locked = false;

    switch (state.homeworld_tab) {
        .buildings => {
            if (idx >= scaling.BuildingType.COUNT) return;
            const bt: scaling.BuildingType = @enumFromInt(idx);
            const level = bldg_levels.get(bt);
            const met = scaling.buildingPrerequisitesMet(bt, bldg_levels);
            locked = !met;

            const title = std.fmt.bufPrint(&title_buf, " {s} ", .{bt.label()}) catch return;

            if (level >= scaling.MAX_BUILDING_LEVEL) {
                const l = std.fmt.bufPrint(content_buf[cpos..], " MAX (Lv.{d})\n", .{level}) catch return;
                cpos += l.len;
            } else {
                const l = std.fmt.bufPrint(content_buf[cpos..], " Level {d}\n", .{level}) catch return;
                cpos += l.len;
            }

            if (scaling.buildingPrerequisites(bt)) |prereq| {
                const met_str: []const u8 = if (bldg_levels.get(prereq.building) >= prereq.level) "[OK]" else "[--]";
                const l = std.fmt.bufPrint(content_buf[cpos..], " Need: {s} >={d} {s}\n", .{ prereq.building.label(), prereq.level, met_str }) catch return;
                cpos += l.len;
            }

            if (level < scaling.MAX_BUILDING_LEVEL) {
                const cost = scaling.buildingCost(bt, level + 1);
                cpos += fmtCostLine(content_buf[cpos..], cost, scaling.buildingTime(bt, level + 1));
            }

            renderCardBox(frame, area, title, content_buf[0..cpos], is_selected, locked, level >= scaling.MAX_BUILDING_LEVEL);
        },
        .shipyard => {
            const classes = ShipClass.ALL;
            if (idx >= classes.len) return;
            const sc = classes[idx];
            const unlocked = scaling.shipClassUnlocked(sc, res_levels);
            locked = !unlocked;

            const title = std.fmt.bufPrint(&title_buf, " {s} ", .{sc.label()}) catch return;

            if (unlocked) {
                const l = std.fmt.bufPrint(content_buf[cpos..], " READY\n", .{}) catch return;
                cpos += l.len;
            } else {
                const l = std.fmt.bufPrint(content_buf[cpos..], " LOCKED\n", .{}) catch return;
                cpos += l.len;
                // Show unlock requirement
                const req_label: []const u8 = switch (sc) {
                    .corvette => "Corvette Tech >=1",
                    .frigate => "Frigate Tech >=1",
                    .cruiser => "Cruiser Tech >=1",
                    .hauler => "Hauler Tech >=1",
                    .scout => "",
                };
                if (req_label.len > 0) {
                    const rl = std.fmt.bufPrint(content_buf[cpos..], " Need: {s}\n", .{req_label}) catch return;
                    cpos += rl.len;
                }
            }

            const cost = sc.buildCost();
            const sy_level = bldg_levels.get(.shipyard);
            cpos += fmtCostLine(content_buf[cpos..], cost, scaling.shipBuildTime(sc, sy_level));

            renderCardBox(frame, area, title, content_buf[0..cpos], is_selected, locked, false);
        },
        .research => {
            if (idx >= scaling.ResearchType.COUNT) return;
            const rt: scaling.ResearchType = @enumFromInt(idx);
            const level = res_levels.get(rt);
            const max_level = scaling.researchMaxLevel(rt);
            const met = scaling.researchPrerequisitesMet(rt, bldg_levels, res_levels);
            locked = !met;
            const maxed = level >= max_level;

            const title = std.fmt.bufPrint(&title_buf, " {s} ", .{rt.label()}) catch return;

            if (maxed) {
                const l = std.fmt.bufPrint(content_buf[cpos..], " Lv.{d}/{d} MAX\n", .{ level, max_level }) catch return;
                cpos += l.len;
            } else {
                const l = std.fmt.bufPrint(content_buf[cpos..], " Lv.{d}/{d}\n", .{ level, max_level }) catch return;
                cpos += l.len;
            }

            // Show prereqs
            const prereqs = scaling.researchPrerequisites(rt);
            for (prereqs) |maybe_prereq| {
                const prereq = maybe_prereq orelse continue;
                switch (prereq.kind) {
                    .building => |b| {
                        const met_str: []const u8 = if (bldg_levels.get(b.building) >= b.level) "[OK]" else "[--]";
                        const l = std.fmt.bufPrint(content_buf[cpos..], " Need: {s} >={d} {s}\n", .{ b.building.label(), b.level, met_str }) catch break;
                        cpos += l.len;
                    },
                    .research => |r| {
                        const met_str: []const u8 = if (res_levels.get(r.tech) >= r.level) "[OK]" else "[--]";
                        const l = std.fmt.bufPrint(content_buf[cpos..], " Need: {s} >={d} {s}\n", .{ r.tech.label(), r.level, met_str }) catch break;
                        cpos += l.len;
                    },
                }
            }

            if (!maxed) {
                const cost = scaling.researchCost(rt, level + 1);
                cpos += fmtCostLine(content_buf[cpos..], cost, scaling.researchTime(rt, level + 1));

                // Fragment cost for level III+
                if (scaling.researchFragmentCost(rt, level + 1)) |fc| {
                    if (fc.inner > 0) {
                        const fl = std.fmt.bufPrint(content_buf[cpos..], " Req: {d} Inner frags\n", .{fc.inner}) catch "";
                        cpos += fl.len;
                    }
                    if (fc.outer > 0) {
                        const fl = std.fmt.bufPrint(content_buf[cpos..], " Req: {d} Outer frags\n", .{fc.outer}) catch "";
                        cpos += fl.len;
                    }
                    if (fc.wandering > 0) {
                        const fl = std.fmt.bufPrint(content_buf[cpos..], " Req: {d} Deep frags\n", .{fc.wandering}) catch "";
                        cpos += fl.len;
                    }
                }
            }

            renderCardBox(frame, area, title, content_buf[0..cpos], is_selected, locked, maxed);
        },
        .fleets, .inventory => return, // handled separately
    }
}

fn fmtCostLine(buf: []u8, cost: Resources, ticks: u64) usize {
    var pos: usize = 0;
    const cl = std.fmt.bufPrint(buf[pos..], " {d:.0} Fe  {d:.0} Cr", .{ cost.metal, cost.crystal }) catch return pos;
    pos += cl.len;
    if (cost.deuterium > 0) {
        const dl = std.fmt.bufPrint(buf[pos..], "  {d:.0} De", .{cost.deuterium}) catch return pos;
        pos += dl.len;
    }
    const tl = std.fmt.bufPrint(buf[pos..], "\n {d} ticks\n", .{ticks}) catch return pos;
    pos += tl.len;
    return pos;
}

fn renderCardBox(
    frame: *Frame,
    area: Rect,
    title: []const u8,
    content: []const u8,
    is_selected: bool,
    is_locked: bool,
    is_maxed: bool,
) void {
    const border_style = if (is_selected)
        amber_full
    else if (is_locked)
        amber_faint
    else
        amber_dim;

    const block = Block{
        .title = title,
        .border = .rounded,
        .border_style = border_style,
    };
    frame.render(block, area);
    const inner = block.inner(area);

    const text_style = if (is_selected and !is_locked)
        amber_bright
    else if (is_locked)
        amber_dim
    else if (is_maxed)
        amber
    else
        amber_bright;

    frame.render(Paragraph{ .text = content, .style = text_style }, inner);
}

fn renderStatusBar(state: *ClientState, frame: *Frame, area: Rect) void {
    const block = Block{
        .title = " STATUS ",
        .border = .rounded,
        .border_style = amber_dim,
    };
    frame.render(block, area);
    const inner = block.inner(area);

    const hw = state.homeworld orelse {
        frame.render(Paragraph{ .text = " ---", .style = amber_faint }, inner);
        return;
    };

    var buf: [512]u8 = undefined;
    var pos: usize = 0;

    // Queue line
    if (hw.build_queue) |q| {
        const p = queueProgress(state.tick, q.start_tick, q.end_tick);
        const l = std.fmt.bufPrint(buf[pos..], " Bld: {s} Lv{d} {d}/{d}t [x]", .{
            q.building_type.label(), q.target_level, p.elapsed, p.total,
        }) catch "";
        pos += l.len;
    } else {
        const l = std.fmt.bufPrint(buf[pos..], " Bld: idle", .{}) catch "";
        pos += l.len;
    }

    if (hw.shipyard_queue) |q| {
        const p = queueProgress(state.tick, q.start_tick, q.end_tick);
        const l = std.fmt.bufPrint(buf[pos..], " | Ship: {s} {d}/{d}b {d}/{d}t [X]", .{
            q.ship_class.label(), q.built, q.count, p.elapsed, p.total,
        }) catch "";
        pos += l.len;
    } else {
        const l = std.fmt.bufPrint(buf[pos..], " | Ship: idle", .{}) catch "";
        pos += l.len;
    }

    if (hw.research_active) |q| {
        const p = queueProgress(state.tick, q.start_tick, q.end_tick);
        const l = std.fmt.bufPrint(buf[pos..], " | Res: {s} {d}/{d}t [z]", .{
            q.tech.label(), p.elapsed, p.total,
        }) catch "";
        pos += l.len;
    } else {
        const l = std.fmt.bufPrint(buf[pos..], " | Res: idle", .{}) catch "";
        pos += l.len;
    }

    // Production line
    const bldg_levels = ClientState.buildingLevelsFromSlice(hw.buildings);
    const nl = std.fmt.bufPrint(buf[pos..], "\n +{d:.2} Fe/t  +{d:.2} Cr/t  +{d:.2} De/t", .{
        scaling.productionPerTick(.metal_mine, bldg_levels.get(.metal_mine)),
        scaling.productionPerTick(.crystal_mine, bldg_levels.get(.crystal_mine)),
        scaling.productionPerTick(.deuterium_synthesizer, bldg_levels.get(.deuterium_synthesizer)),
    }) catch "";
    pos += nl.len;

    frame.render(Paragraph{ .text = buf[0..pos], .style = amber_bright }, inner);
}

fn renderTechTree(state: *ClientState, frame: *Frame, area: Rect) void {
    const block = Block{
        .title = " TECH TREE ",
        .border = .rounded,
        .border_style = amber_dim,
    };
    frame.render(block, area);
    const inner = block.inner(area);

    const hw = state.homeworld orelse {
        frame.render(Paragraph{ .text = " No homeworld data", .style = amber_faint }, inner);
        return;
    };

    const bldg_levels = ClientState.buildingLevelsFromSlice(hw.buildings);
    const res_levels = ClientState.researchLevelsFromSlice(hw.research);

    // Two columns: left = buildings+ships, right = research
    const cols = frame.layout(inner, .horizontal, &.{
        Constraint.flexible(1),
        Constraint.flexible(1),
    });

    // Left column: buildings + ships
    {
        var buf: [1536]u8 = undefined;
        var pos: usize = 0;

        const hdr = std.fmt.bufPrint(buf[pos..], " BUILDINGS\n ─────────────────────────────\n", .{}) catch "";
        pos += hdr.len;

        const bt_fields = @typeInfo(scaling.BuildingType).@"enum".fields;
        inline for (bt_fields, 0..) |_, i| {
            const bt: scaling.BuildingType = @enumFromInt(i);
            const level = bldg_levels.get(bt);
            if (level > 0) {
                const l = std.fmt.bufPrint(buf[pos..], " {s: <20} Lv.{d}\n", .{ bt.label(), level }) catch break;
                pos += l.len;
            } else {
                const l = std.fmt.bufPrint(buf[pos..], " {s: <20} ---\n", .{bt.label()}) catch break;
                pos += l.len;
            }
            if (scaling.buildingPrerequisites(bt)) |prereq| {
                if (!scaling.buildingPrerequisitesMet(bt, bldg_levels)) {
                    const met_str: []const u8 = if (bldg_levels.get(prereq.building) >= prereq.level) "[OK]" else "[--]";
                    const pl = std.fmt.bufPrint(buf[pos..], "   Need: {s} >= {d} {s}\n", .{ prereq.building.label(), prereq.level, met_str }) catch break;
                    pos += pl.len;
                }
            }
        }

        const ship_hdr = std.fmt.bufPrint(buf[pos..], "\n SHIPS\n ─────────────────────────────\n", .{}) catch "";
        pos += ship_hdr.len;

        for (ShipClass.ALL) |sc| {
            const unlocked = scaling.shipClassUnlocked(sc, res_levels);
            const status: []const u8 = if (unlocked) "UNLOCKED" else "LOCKED";
            const sl = std.fmt.bufPrint(buf[pos..], " {s: <20} {s}\n", .{ sc.label(), status }) catch break;
            pos += sl.len;
            if (!unlocked) {
                const req: []const u8 = switch (sc) {
                    .corvette => "Corvette Tech >= 1",
                    .frigate => "Frigate Tech >= 1",
                    .cruiser => "Cruiser Tech >= 1",
                    .hauler => "Hauler Tech >= 1",
                    .scout => "",
                };
                if (req.len > 0) {
                    const rl = std.fmt.bufPrint(buf[pos..], "   Need: {s}\n", .{req}) catch break;
                    pos += rl.len;
                }
            }
        }

        frame.render(Paragraph{ .text = buf[0..pos], .style = amber_bright }, cols.get(0));
    }

    // Right column: research
    {
        var buf: [1536]u8 = undefined;
        var pos: usize = 0;

        const hdr = std.fmt.bufPrint(buf[pos..], " RESEARCH\n ─────────────────────────────\n", .{}) catch "";
        pos += hdr.len;

        const rt_fields = @typeInfo(scaling.ResearchType).@"enum".fields;
        inline for (rt_fields, 0..) |_, i| {
            const rt: scaling.ResearchType = @enumFromInt(i);
            const level = res_levels.get(rt);
            const max_level = scaling.researchMaxLevel(rt);
            const rl = std.fmt.bufPrint(buf[pos..], " {s: <20} {d}/{d}\n", .{ rt.label(), level, max_level }) catch break;
            pos += rl.len;

            const prereqs = scaling.researchPrerequisites(rt);
            for (prereqs) |maybe_prereq| {
                const prereq = maybe_prereq orelse continue;
                switch (prereq.kind) {
                    .building => |b| {
                        const met_str: []const u8 = if (bldg_levels.get(b.building) >= b.level) "[OK]" else "[--]";
                        const pl = std.fmt.bufPrint(buf[pos..], "   Need: {s} >= {d} {s}\n", .{ b.building.label(), b.level, met_str }) catch break;
                        pos += pl.len;
                    },
                    .research => |r| {
                        const met_str: []const u8 = if (res_levels.get(r.tech) >= r.level) "[OK]" else "[--]";
                        const pl = std.fmt.bufPrint(buf[pos..], "   Need: {s} >= {d} {s}\n", .{ r.tech.label(), r.level, met_str }) catch break;
                        pos += pl.len;
                    },
                }
            }
        }

        const footer = std.fmt.bufPrint(buf[pos..], "\n [t] Close", .{}) catch "";
        pos += footer.len;

        frame.render(Paragraph{ .text = buf[0..pos], .style = amber_bright }, cols.get(1));
    }
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

    // Other own fleets
    for (state.fleets.items) |fleet| {
        if (fleet.location.eql(coord)) {
            return .{ .symbol = "A", .style = amber_bright };
        }
    }

    // Allied player fleets (from sector data)
    if (state.known_sectors.get(coord.toKey())) |sector| {
        if (sector.player_fleets) |pf| {
            if (pf.len > 0) return .{ .symbol = "A", .style = amber };
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

    // Zone boundary markers visible as navigation aid
    const dist = coord.distFromOrigin();
    const boundary_symbol: ?[]const u8 = if (dist == Zone.inner_ring_radius)
        "-"
    else if (dist == Zone.outer_ring_radius)
        "~"
    else
        null;

    // Fog of war: adjacent to explored sector (boundaries brighter near known space)
    const is_fog = for (coord.neighbors()) |n| {
        if (state.known_sectors.contains(n.toKey())) break true;
    } else false;

    if (boundary_symbol) |sym| {
        return .{ .symbol = sym, .style = if (is_fog) amber_dim else amber_faint };
    }
    return .{ .symbol = if (is_fog) "." else " ", .style = amber_faint };
}

fn renderMapLegend(state: *ClientState, frame: *Frame, area: Rect) void {
    var buf: [256]u8 = undefined;
    const center = state.map_center;
    const zoom_label: []const u8 = switch (state.map_zoom) {
        .close => "CLOSE",
        .sector => "SECTOR",
        .region => "REGION",
    };
    const text = std.fmt.bufPrint(&buf, " [{d},{d}] Zoom:{s} | @=You A=Ally H=Home !=Hostile -=Ring ~=Wander .=Fog | [Arrows]Scroll [z/x]Zoom [c]Center [Tab]Fleet", .{
        center.q, center.r, zoom_label,
    }) catch " STAR MAP";
    frame.render(Paragraph{ .text = text, .style = amber_dim }, area);
}
