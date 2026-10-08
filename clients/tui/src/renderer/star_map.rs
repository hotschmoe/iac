// Star Map view: the tactical plot. An amber plotting table with a fixed
// crosshair at screen center — the chart scrolls beneath it.
//
// Close zoom lays hexes out loosely (4 columns per q, 2 rows per r) so the
// sparse connection graph renders as actual lanes (─ ╱ ╲): chokepoints and
// dead ends become visible, and plotted courses light up their edges.
// Wider zooms pack one hex per cell and stay abstract.
//
// Intel has age: live-streamed sectors burn bright, remembered charts sit
// dim, scan pings blink cyan until they expire, and a fresh scan throws a
// brief expanding ripple.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::Paragraph,
    Frame,
};

use iac_shared::hex::{self, Hex};
use iac_shared::constants::Zone;
use iac_shared::protocol::SignalKind;

use crate::state::{ClientState, ZoomLevel, SCAN_RIPPLE_TICKS};
use super::{
    AMBER, AMBER_BRIGHT, AMBER_DIM, AMBER_FAINT, AMBER_FULL,
    CYAN_DIM, CYAN_INTEL, GREEN_DIM, GREEN_GOOD, RED_ALERT, RED_DIM,
    titled_block,
};

pub fn render(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(4)]).split(area);

    render_hex_grid(frame, state, rows[0]);
    render_map_footer(frame, state, rows[1]);
}

fn zoom_radius(zoom: ZoomLevel) -> u16 {
    match zoom {
        ZoomLevel::Close => 6,
        ZoomLevel::Sector => 10,
        ZoomLevel::Region => 20,
    }
}

/// Screen offset of a hex relative to the view center, per zoom.
fn cell_offset(zoom: ZoomLevel, dq: i32, dr: i32) -> (i32, i32) {
    match zoom {
        // Tactical plot: room between nodes for connection glyphs.
        ZoomLevel::Close => (dq * 4 + dr * 2, dr * 2),
        // Tight packing: one hex per cell.
        _ => (dq * 2 + dr, dr),
    }
}

fn render_hex_grid(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let title = match state.map_zoom {
        ZoomLevel::Close => " TACTICAL PLOT ",
        ZoomLevel::Sector => " STAR MAP ",
        ZoomLevel::Region => " REGION CHART ",
    };
    let block = titled_block(title, AMBER_DIM);
    let inner = block.inner(area);
    frame.render_widget(&block, area);

    if inner.width < 5 || inner.height < 5 {
        return;
    }

    let center = state.map_center;
    let radius = zoom_radius(state.map_zoom);

    let ix = inner.x as i32;
    let iy = inner.y as i32;
    let iw = inner.width as i32;
    let ih = inner.height as i32;
    let vp_cx = ix + iw / 2;
    let vp_cy = iy + ih / 2;

    let in_view = |x: i32, y: i32| x >= ix && x < ix + iw && y >= iy && y < iy + ih;
    let put = |frame: &mut Frame<'_>, x: i32, y: i32, sym: &str, style: Style| {
        if in_view(x, y) {
            frame.render_widget(
                Paragraph::new(sym.to_string()).style(style),
                Rect::new(x as u16, y as u16, 1, 1),
            );
        }
    };

    // Route edges (fleet position → hop1 → hop2 …) get bright lanes.
    let route_edges = route_edge_set(state);

    // Pass 1 (Close zoom only): lanes between known sectors.
    if state.map_zoom == ZoomLevel::Close {
        for coord in hex::hex_spiral(center, radius + 1) {
            let Some(sector) = state.known_sectors.get(&coord.to_key()) else { continue; };
            let (dx, dy) = cell_offset(state.map_zoom, (coord.q - center.q) as i32, (coord.r - center.r) as i32);
            let (x, y) = (vp_cx + dx, vp_cy + dy);

            for conn in &sector.connections {
                let on_route = route_edges.contains(&edge_key(coord, *conn));
                let style = if on_route { AMBER_FULL } else { lane_style(state, coord, *conn) };
                let ddq = (conn.q - coord.q) as i32;
                let ddr = (conn.r - coord.r) as i32;
                match (ddq, ddr) {
                    (1, 0) => {
                        for step in 1..=3 {
                            put(frame, x + step, y, "─", style);
                        }
                    }
                    (-1, 0) => {
                        for step in 1..=3 {
                            put(frame, x - step, y, "─", style);
                        }
                    }
                    (1, -1) => put(frame, x + 1, y - 1, "╱", style),
                    (-1, 1) => put(frame, x - 1, y + 1, "╱", style),
                    (0, -1) => put(frame, x - 1, y - 1, "╲", style),
                    (0, 1) => put(frame, x + 1, y + 1, "╲", style),
                    _ => {}
                }
            }
        }
    }

    // Pass 2: nodes.
    for coord in hex::hex_spiral(center, radius) {
        let (dx, dy) = cell_offset(state.map_zoom, (coord.q - center.q) as i32, (coord.r - center.r) as i32);
        let (x, y) = (vp_cx + dx, vp_cy + dy);
        if !in_view(x, y) { continue; }

        let mut cell = classify_hex(state, coord);

        // Plotted course nodes glow.
        if state.route_hops().contains(&coord) {
            cell.style = AMBER_FULL;
        }

        // Scan ripple: an expanding cyan ring for a few beats.
        for (origin, t) in &state.scan_ripples {
            let age = state.tick.saturating_sub(*t);
            if age <= SCAN_RIPPLE_TICKS
                && Hex::distance(origin, &coord) as u64 == age + 1
                && cell.symbol == " "
            {
                cell = HexCell { symbol: "·", style: CYAN_DIM };
            }
        }

        // The crosshair: fixed at screen center, inverse video.
        let style = if coord == center {
            cell.style.add_modifier(Modifier::REVERSED)
        } else {
            cell.style
        };

        put(frame, x, y, cell.symbol, style);
    }
}

/// Lanes read dimmer when they lead into fog.
fn lane_style(state: &ClientState, _from: Hex, to: Hex) -> Style {
    if state.known_sectors.contains_key(&to.to_key()) {
        AMBER_DIM
    } else {
        AMBER_FAINT
    }
}

/// Undirected edge identity.
fn edge_key(a: Hex, b: Hex) -> (u32, u32) {
    let (ka, kb) = (a.to_key(), b.to_key());
    if ka < kb { (ka, kb) } else { (kb, ka) }
}

/// Edges along the plotted course, including the leg the fleet is on.
fn route_edge_set(state: &ClientState) -> std::collections::HashSet<(u32, u32)> {
    let mut set = std::collections::HashSet::new();
    let hops = state.route_hops();
    if hops.is_empty() { return set; }
    if let Some((fleet_id, _)) = state.nav_route
        && let Some(fleet) = state.fleets.iter().find(|f| f.id == fleet_id)
    {
        set.insert(edge_key(fleet.location, hops[0]));
    }
    for pair in hops.windows(2) {
        set.insert(edge_key(pair[0], pair[1]));
    }
    set
}

struct HexCell {
    symbol: &'static str,
    style: Style,
}

/// Dim a live style down to its remembered-chart sibling.
fn fade(style: Style) -> Style {
    if style == AMBER_FULL || style == AMBER_BRIGHT || style == AMBER {
        AMBER_DIM
    } else if style == RED_ALERT {
        RED_DIM
    } else if style == GREEN_GOOD {
        GREEN_DIM
    } else if style == CYAN_INTEL {
        CYAN_DIM
    } else {
        style
    }
}

fn classify_hex(state: &ClientState, coord: Hex) -> HexCell {
    let key = coord.to_key();

    // Active fleet position
    if let Some(fleet) = state.active_fleet()
        && fleet.location == coord {
            return HexCell { symbol: "@", style: AMBER_FULL };
        }

    // Other own fleets
    for fleet in &state.fleets {
        if fleet.location == coord {
            return HexCell { symbol: "A", style: AMBER_BRIGHT };
        }
    }

    // Allied player fleets (from sector data)
    if let Some(sector) = state.known_sectors.get(&key)
        && sector.player_fleets.as_ref().map(|pf| !pf.is_empty()).unwrap_or(false) {
            return HexCell { symbol: "A", style: AMBER };
        }

    // Homeworld
    if let Some(player) = &state.player
        && player.homeworld == coord {
            return HexCell { symbol: "H", style: AMBER_FULL };
        }

    // Known sector: live intel renders at full heat, memory fades.
    if let Some(sector) = state.known_sectors.get(&key) {
        let live = state.sector_is_live(key);
        let cell = classify_known(sector);
        return if live { cell } else { HexCell { symbol: cell.symbol, style: fade(cell.style) } };
    }

    // Scan pings: faint contacts in the fog, blinking until they expire.
    if let Some((kind, _)) = state.signal_pings.get(&key) {
        let (symbol, bright) = match kind {
            SignalKind::RichOre => ("%", GREEN_GOOD),
            SignalKind::HostileMass => ("!", RED_ALERT),
            SignalKind::Derelict => ("D", CYAN_INTEL),
            SignalKind::Anomaly => ("?", CYAN_INTEL),
        };
        let style = if state.tick.is_multiple_of(2) { bright } else { fade(bright) };
        return HexCell { symbol, style };
    }

    // Zone boundary markers
    let dist = coord.dist_from_origin();
    let boundary_symbol = if dist == Zone::INNER_RING_RADIUS {
        Some("-")
    } else if dist == Zone::OUTER_RING_RADIUS {
        Some("~")
    } else {
        None
    };

    // Fog of war: adjacent to explored sector
    let is_fog = coord.neighbors().iter().any(|n| state.known_sectors.contains_key(&n.to_key()));

    if let Some(sym) = boundary_symbol {
        return HexCell {
            symbol: sym,
            style: if is_fog { AMBER_DIM } else { AMBER_FAINT },
        };
    }

    HexCell {
        symbol: if is_fog { "." } else { " " },
        style: AMBER_FAINT,
    }
}

fn classify_known(sector: &iac_shared::protocol::SectorState) -> HexCell {
    if sector.hostiles.is_some() {
        return HexCell { symbol: "!", style: RED_ALERT };
    }
    if sector.salvage.is_some() {
        return HexCell { symbol: "$", style: GREEN_GOOD };
    }
    if sector.site.is_some() {
        return HexCell { symbol: "D", style: CYAN_INTEL };
    }

    let rich = [sector.resources.metal, sector.resources.crystal, sector.resources.deuterium]
        .iter()
        .any(|&d| d as u8 >= iac_shared::constants::Density::Rich as u8);
    if rich {
        return HexCell { symbol: sector.terrain.symbol(), style: GREEN_GOOD };
    }

    let has_resources = sector.resources.metal != iac_shared::constants::Density::None
        || sector.resources.crystal != iac_shared::constants::Density::None
        || sector.resources.deuterium != iac_shared::constants::Density::None;
    if has_resources {
        return HexCell { symbol: sector.terrain.symbol(), style: AMBER };
    }

    HexCell { symbol: sector.terrain.symbol(), style: AMBER_BRIGHT }
}

// ── Footer: crosshair inspect + legend + controls ─────────────────

fn render_map_footer(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ]).split(area);

    // Line 1: what's under the crosshair.
    let (inspect, inspect_style) = inspect_line(state);
    frame.render_widget(Paragraph::new(inspect).style(inspect_style), rows[0]);

    // Line 2: course status, if one is plotted.
    let course = match &state.nav_route {
        Some((fleet_id, hops)) if !hops.is_empty() => {
            let dest = hops[hops.len() - 1];
            format!(
                " COURSE F{} → [{},{}]  {} hops remaining  [Bksp] clear",
                fleet_id, dest.q, dest.r, hops.len()
            )
        }
        _ => String::new(),
    };
    frame.render_widget(Paragraph::new(course).style(AMBER_BRIGHT), rows[1]);

    // Line 3: legend.
    frame.render_widget(
        Paragraph::new(" @You AFleet HHome !Hostile $Salvage DDerelict %OrePing ?Anomaly ·Fog  bright=live dim=memory")
            .style(AMBER_DIM),
        rows[2],
    );

    // Line 4: controls.
    frame.render_widget(
        Paragraph::new(" [Arrows] Scroll  [Enter] Plot Course  [z/x] Zoom  [c] Center  [Tab] Fleet")
            .style(AMBER_DIM),
        rows[3],
    );
}

fn inspect_line(state: &ClientState) -> (String, Style) {
    let coord = state.map_center;
    let key = coord.to_key();
    let dist = coord.dist_from_origin();

    if let Some(sector) = state.known_sectors.get(&key) {
        let freshness = if state.sector_is_live(key) { "LIVE" } else { "CHART" };
        let mut line = format!(
            " ▣ [{},{}] {} d{}  {}  M:{} C:{} D:{}",
            coord.q, coord.r, freshness, dist,
            sector.terrain.label(),
            super::density_short(sector.resources.metal),
            super::density_short(sector.resources.crystal),
            super::density_short(sector.resources.deuterium),
        );
        if let Some(h) = &sector.hostiles {
            let ships: u16 = h.iter().flat_map(|f| f.ships.iter().map(|s| s.count)).sum();
            line.push_str(&format!("  HOSTILES:{}", ships));
        }
        if sector.salvage.is_some() {
            line.push_str("  SALVAGE");
        }
        if let Some(site) = &sector.site {
            line.push_str(&format!("  DERELICT T{} ({})", site.tier, site.risk.label()));
        }
        line.push_str(&format!("  exits:{}", sector.connections.len()));
        let style = if sector.hostiles.is_some() { RED_ALERT } else { AMBER_BRIGHT };
        return (line, style);
    }

    if let Some((kind, t)) = state.signal_pings.get(&key) {
        let age = state.tick.saturating_sub(*t);
        return (
            format!(" ▣ [{},{}] SIGNAL — {} ({}t old)", coord.q, coord.r, kind.label(), age),
            CYAN_INTEL,
        );
    }

    (
        format!(" ▣ [{},{}] d{}  UNCHARTED", coord.q, coord.r, dist),
        AMBER_DIM,
    )
}
