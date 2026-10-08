// Windshield view: sector node graph, fleet status sidebar, event log.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::Paragraph,
    Frame,
};

use iac_shared::hex::HexDirection;
use iac_shared::protocol::SectorState;

use crate::state::ClientState;
use super::{
    AMBER, AMBER_BRIGHT, AMBER_DIM, AMBER_FAINT, AMBER_FULL,
    CYAN_INTEL, GREEN_GOOD, RED_ALERT, RED_DIM,
    density_short, event_lines, titled_block,
};

pub fn render(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    // Top section + bottom event log
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(7)]).split(area);

    // Top: sector view + fleet sidebar
    let cols = Layout::horizontal([Constraint::Min(1), Constraint::Length(30)]).split(rows[0]);

    if state.show_sector_info {
        render_sector_info(frame, state, cols[0]);
    } else {
        render_sector_view(frame, state, cols[0]);
    }
    render_fleet_status(frame, state, cols[1]);

    // Event log
    let log_block = titled_block(" EVENTS ", AMBER_DIM);
    let log_inner = log_block.inner(rows[1]);
    frame.render_widget(&log_block, rows[1]);

    let lines = event_lines(state, log_inner.height as usize);
    frame.render_widget(Paragraph::new(lines), log_inner);
}

fn render_sector_view(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let fleet = state.active_fleet();
    let in_combat = fleet.map(|f| matches!(f.state, iac_shared::protocol::FleetStatus::InCombat)).unwrap_or(false);

    let block = titled_block(
        if in_combat { " !! COMBAT !! " } else { " SECTOR VIEW " },
        if in_combat { RED_ALERT } else { AMBER_DIM },
    );
    let inner = block.inner(area);
    frame.render_widget(&block, area);

    let fleet = match fleet {
        Some(f) => f,
        None => {
            frame.render_widget(Paragraph::new(" No active fleet").style(AMBER_FAINT), inner);
            return;
        }
    };

    if fleet.ships.is_empty() {
        let text = format!(
            " Location: [{},{}]\n Status: NO SHIPS AVAILABLE\n\n All ships destroyed.\n [Esc] Return to Command Center",
            fleet.location.q, fleet.location.r,
        );
        frame.render_widget(Paragraph::new(text).style(AMBER_BRIGHT), inner);
        return;
    }

    if inner.width < 40 || inner.height < 13 {
        render_sector_view_compact(frame, state, inner, fleet);
        return;
    }

    let sector = match state.current_sector() {
        Some(s) => s,
        None => {
            frame.render_widget(Paragraph::new(" Sector data pending...").style(AMBER_FAINT), inner);
            return;
        }
    };

    let loc = fleet.location;
    let came_from = find_came_from(state, loc);

    // Center of the inner area
    let cx = inner.x as i32 + (inner.width as i32) / 2;
    let cy = inner.y as i32 + (inner.height as i32) / 2 - 1;

    // Node offsets: E, NE, NW, W, SW, SE
    let node_dx = [17, 10, -10, -17, -10, 10];
    let node_dy = [0, -5, -5, 0, 5, 5];

    // Draw connection lines
    for (i, &dir) in HexDirection::ALL.iter().enumerate() {
        if sector_has_connection(sector, loc.neighbor(dir)) {
            let style = if Some(i) == came_from { AMBER } else { AMBER_DIM };
            render_connection_line(frame, inner, cx, cy, node_dx[i], node_dy[i], style);
        }
    }

    // Center node
    render_center_node(frame, inner, cx, cy, loc, sector.terrain.label());

    // Direction nodes
    for (i, &dir) in HexDirection::ALL.iter().enumerate() {
        let nbr = loc.neighbor(dir);
        let nx = cx + node_dx[i];
        let ny = cy + node_dy[i];

        if sector_has_connection(sector, nbr) {
            let sector_data = state.known_sectors.get(&nbr.to_key());
            render_direction_node(frame, inner, nx, ny, dir, i, nbr, came_from == Some(i), sector_data);
        } else {
            render_disconnected_node(frame, inner, nx, ny, dir);
        }
    }

    // Hostile fleet info at bottom; [n] moves the » marker, [a] fires.
    if let Some(hostiles) = &sector.hostiles {
        let target_idx = if hostiles.is_empty() { 0 } else { state.target_idx % hostiles.len() };
        let hostile_y = inner.y + inner.height.saturating_sub(2);
        for (fi, fleet_info) in hostiles.iter().enumerate() {
            let targeted = fi == target_idx;
            for ship in &fleet_info.ships {
                let text = format!(
                    " {} HOSTILE: {}x {} ({})",
                    if targeted { "»" } else { " " },
                    ship.count,
                    ship.ship_class.label(),
                    behavior_label(fleet_info.behavior),
                );
                let row = hostile_y.saturating_sub(fi as u16);
                if row >= inner.y && row < inner.y + inner.height {
                    let rect = Rect::new(inner.x, row, inner.width, 1);
                    let style = if targeted { RED_ALERT } else { RED_DIM };
                    frame.render_widget(Paragraph::new(text).style(style), rect);
                }
            }
        }
    }

    // Keybinds hint
    let kb_y = inner.y + inner.height.saturating_sub(1);
    let rect = Rect::new(inner.x, kb_y, inner.width, 1);
    frame.render_widget(Paragraph::new(" [1-6] Move  [v] Scan  [x] Board  [n] Target  [a] Attack  [p] Orders  [?] All Keys").style(AMBER_DIM), rect);
}

fn render_sector_view_compact(
    frame: &mut Frame<'_>,
    state: &ClientState,
    inner: Rect,
    fleet: &iac_shared::protocol::FleetState,
) {
    let mut lines = Vec::new();
    let status = match fleet.state {
        iac_shared::protocol::FleetStatus::Idle => "idle",
        iac_shared::protocol::FleetStatus::Moving => "moving",
        iac_shared::protocol::FleetStatus::Harvesting => "harvesting",
        iac_shared::protocol::FleetStatus::InCombat => "in_combat",
        iac_shared::protocol::FleetStatus::Returning => "returning",
        iac_shared::protocol::FleetStatus::Docked => "docked",
        iac_shared::protocol::FleetStatus::Exploring => "exploring",
    };
    lines.push(Line::raw(format!(
        " Location: [{},{}]  Status: {}",
        fleet.location.q, fleet.location.r, status
    )));

    if let Some(sector) = state.current_sector() {
        lines.push(Line::raw(format!(" Terrain: {}", sector.terrain.label())));
        let loc = fleet.location;
        let came_from = find_came_from(state, loc);

        lines.push(Line::raw(" Exits:"));
        for (i, &dir) in HexDirection::ALL.iter().enumerate() {
            let nbr = loc.neighbor(dir);
            if sector_has_connection(sector, nbr) {
                let marker = if came_from == Some(i) { "<" } else { " " };
                lines.push(Line::raw(format!(
                    "  [{}] {:>2} -> [{},{}]{}",
                    i + 1,
                    dir.label(),
                    nbr.q,
                    nbr.r,
                    marker,
                )));
            }
        }
    } else {
        lines.push(Line::raw(" Sector data pending..."));
    }

    lines.push(Line::raw(" [1-6] Move  [h] Harvest  [r] Recall"));
    frame.render_widget(Paragraph::new(lines).style(AMBER), inner);
}

fn render_sector_info(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let block = titled_block(" SECTOR INFO ", AMBER_DIM);
    let inner = block.inner(area);
    frame.render_widget(&block, area);

    let fleet = match state.active_fleet() {
        Some(f) => f,
        None => {
            frame.render_widget(Paragraph::new(" No active fleet").style(AMBER_FAINT), inner);
            return;
        }
    };

    let sector = match state.current_sector() {
        Some(s) => s,
        None => {
            frame.render_widget(Paragraph::new(" Sector data pending...").style(AMBER_FAINT), inner);
            return;
        }
    };

    let mut lines = Vec::new();
    lines.push(Line::raw(format!(
        " Location: [{},{}]\n Terrain:  {}",
        fleet.location.q, fleet.location.r, sector.terrain.label()
    )));

    // Zone
    let dist = fleet.location.dist_from_origin();
    let zone_label = if dist == 0 {
        "Central Hub"
    } else if dist <= 8 {
        "Inner Ring"
    } else if dist <= 20 {
        "Outer Ring"
    } else {
        "The Wandering"
    };
    lines.push(Line::raw(format!(" Zone:     {} (dist {})\n", zone_label, dist)));

    // Homeworld marker
    if let Some(player) = &state.player
        && player.homeworld == fleet.location {
            lines.push(Line::raw(" ** HOMEWORLD **"));
        }

    // Resources
    lines.push(Line::raw("\n RESOURCES"));
    lines.push(Line::raw(" ─────────────────────"));
    lines.push(Line::raw(format!(" Metal:     {}", sector.resources.metal.label())));
    lines.push(Line::raw(format!(" Crystal:   {}", sector.resources.crystal.label())));
    lines.push(Line::raw(format!(" Deuterium: {}", sector.resources.deuterium.label())));

    // Salvage
    if let Some(salvage) = &sector.salvage {
        lines.push(Line::raw("\n SALVAGE"));
        lines.push(Line::raw(" ─────────────────────"));
        lines.push(Line::raw(format!(" Fe {:.0}  Cr {:.0}  De {:.0}", salvage.metal, salvage.crystal, salvage.deuterium)));
    }

    // Derelict site
    if let Some(site) = &sector.site {
        let risk_style = match site.risk {
            iac_shared::protocol::SiteRisk::Quiet => CYAN_INTEL,
            iac_shared::protocol::SiteRisk::Uneasy => AMBER_FULL,
            iac_shared::protocol::SiteRisk::Hot => RED_ALERT,
        };
        lines.push(Line::styled("\n DERELICT HULK", CYAN_INTEL));
        lines.push(Line::raw(" ─────────────────────"));
        lines.push(Line::styled(
            format!(" Tier {}  risk: {}   [x] board", site.tier, site.risk.label()),
            risk_style,
        ));
    }

    // Hostiles
    if let Some(hostiles) = &sector.hostiles {
        let target_idx = if hostiles.is_empty() { 0 } else { state.target_idx % hostiles.len() };
        lines.push(Line::styled("\n HOSTILES", RED_ALERT));
        lines.push(Line::raw(" ─────────────────────"));
        for (fi, fleet_info) in hostiles.iter().enumerate() {
            let targeted = fi == target_idx;
            for ship in &fleet_info.ships {
                lines.push(Line::styled(
                    format!(
                        " {} {}x {} ({})",
                        if targeted { "»" } else { " " },
                        ship.count,
                        ship.ship_class.label(),
                        behavior_label(fleet_info.behavior),
                    ),
                    if targeted { RED_ALERT } else { RED_DIM },
                ));
            }
        }
    }

    // Allied fleets
    if let Some(allies) = &sector.player_fleets
        && !allies.is_empty() {
            lines.push(Line::raw("\n ALLIED FLEETS"));
            lines.push(Line::raw(" ─────────────────────"));
            for ally in allies {
                lines.push(Line::raw(format!(" {} -- {} ships", ally.owner_name, ally.ship_count)));
            }
        }

    // Exits
    lines.push(Line::raw("\n EXITS"));
    lines.push(Line::raw(" ─────────────────────"));
    let loc = fleet.location;
    for (i, &dir) in HexDirection::ALL.iter().enumerate() {
        let nbr = loc.neighbor(dir);
        if sector_has_connection(sector, nbr) {
            lines.push(Line::raw(format!(
                " [{}] {:>2} -> [{},{}]",
                i + 1,
                dir.label(),
                nbr.q,
                nbr.r,
            )));
        }
    }

    lines.push(Line::raw("\n [i] Close Info  [1-6] Move  [h] Harvest"));
    frame.render_widget(Paragraph::new(lines).style(AMBER_BRIGHT), inner);
}

fn render_fleet_status(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let block = titled_block(" FLEET ", AMBER_DIM);
    let inner = block.inner(area);
    frame.render_widget(&block, area);

    let fleet = match state.active_fleet() {
        Some(f) => f,
        None => {
            frame.render_widget(Paragraph::new(" ---").style(AMBER_FAINT), inner);
            return;
        }
    };

    let ships_str = fleet.ships.len().to_string();

    let mut hull_cur: f32 = 0.0;
    let mut hull_max: f32 = 0.0;
    let mut shield_cur: f32 = 0.0;
    let mut shield_max: f32 = 0.0;
    let mut dps: f32 = 0.0;

    for ship in &fleet.ships {
        hull_cur += ship.hull;
        hull_max += ship.hull_max;
        shield_cur += ship.shield;
        shield_max += ship.shield_max;
        dps += ship.weapon_power;
    }
    let total_cargo = fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium;

    let status = match fleet.state {
        iac_shared::protocol::FleetStatus::Idle => "idle",
        iac_shared::protocol::FleetStatus::Moving => "moving",
        iac_shared::protocol::FleetStatus::Harvesting => "harvesting",
        iac_shared::protocol::FleetStatus::InCombat => "IN COMBAT",
        iac_shared::protocol::FleetStatus::Returning => "returning",
        iac_shared::protocol::FleetStatus::Docked => "docked",
        iac_shared::protocol::FleetStatus::Exploring => "BOARDING",
    };
    let status_style = match fleet.state {
        iac_shared::protocol::FleetStatus::InCombat => RED_ALERT,
        iac_shared::protocol::FleetStatus::Harvesting => GREEN_GOOD,
        iac_shared::protocol::FleetStatus::Exploring => CYAN_INTEL,
        _ => AMBER_BRIGHT,
    };
    let mut status_line = format!(" Status: {}", status);
    if fleet.cooldown_remaining > 0 {
        status_line.push_str(&format!("  cd {}s", fleet.cooldown_remaining));
    }
    if let Some(preset) = fleet.policy {
        status_line.push_str(&format!("  AUTO:{}", preset.label()));
    }

    // Hull turns red when the fleet is under 50% — glanceable danger.
    let hull_style = if hull_max > 0.0 && hull_cur / hull_max < 0.5 {
        RED_ALERT
    } else {
        AMBER_BRIGHT
    };

    let lines = vec![
        Line::styled(status_line, status_style),
        Line::styled(format!(" Ships: {}", ships_str), AMBER_BRIGHT),
        Line::styled(format!(" Fuel:  {:.0}/{:.0}", fleet.fuel, fleet.fuel_max), AMBER_BRIGHT),
        Line::raw(""),
        Line::styled(format!(" Hull:   {:.0}/{:.0}", hull_cur, hull_max), hull_style),
        Line::styled(format!(" Shield: {:.0}/{:.0}", shield_cur, shield_max), CYAN_INTEL),
        Line::styled(format!(" DPS:    {:.0}", dps), AMBER_BRIGHT),
        Line::raw(""),
        Line::styled(format!(" Cargo: {:.0}/{:.0}", total_cargo, fleet.cargo_capacity), AMBER_BRIGHT),
        Line::styled(format!("  Fe {:.0}", fleet.cargo.metal), AMBER),
        Line::styled(format!("  Cr {:.0}", fleet.cargo.crystal), AMBER),
        Line::styled(format!("  De {:.0}", fleet.cargo.deuterium), AMBER),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

// ── Node graph helpers ─────────────────────────────────────────────

fn render_center_node(frame: &mut Frame<'_>, inner: Rect, cx: i32, cy: i32, loc: iac_shared::hex::Hex, terrain_label: &str) {
    let coord_text = format!("[{},{}]", loc.q, loc.r);
    render_text_centered(frame, inner, cx, cy - 1, &coord_text, AMBER_BRIGHT);
    render_text_centered(frame, inner, cx, cy, "<>", AMBER_FULL);
    render_text_centered(frame, inner, cx, cy + 1, terrain_label, AMBER);
}

#[allow(clippy::too_many_arguments)]
fn render_direction_node(
    frame: &mut Frame<'_>,
    inner: Rect,
    nx: i32,
    ny: i32,
    dir: HexDirection,
    idx: usize,
    coord: iac_shared::hex::Hex,
    is_came_from: bool,
    sector_data: Option<&SectorState>,
) {
    let explored = sector_data.is_some();
    let heading_style = if is_came_from {
        AMBER_FULL
    } else if explored {
        AMBER_BRIGHT
    } else {
        AMBER
    };
    let detail_style = if is_came_from || explored { AMBER } else { AMBER_DIM };

    let heading = if is_came_from {
        format!("[{}] {} <", idx + 1, dir.label())
    } else {
        format!("[{}] {}", idx + 1, dir.label())
    };

    let coord_text = format!("[{},{}]", coord.q, coord.r);
    let resource_text = format_resource_summary(sector_data);

    render_text_centered(frame, inner, nx, ny - 1, &heading, heading_style);
    render_text_centered(frame, inner, nx, ny, &coord_text, heading_style);
    render_text_centered(frame, inner, nx, ny + 1, &resource_text, detail_style);
}

fn render_disconnected_node(frame: &mut Frame<'_>, inner: Rect, nx: i32, ny: i32, dir: HexDirection) {
    render_text_centered(frame, inner, nx, ny - 1, dir.label(), AMBER_FAINT);
    render_text_centered(frame, inner, nx, ny, "---", AMBER_FAINT);
}

fn render_connection_line(frame: &mut Frame<'_>, inner: Rect, cx: i32, cy: i32, target_dx: i32, target_dy: i32, style: Style) {
    if target_dy == 0 {
        // Horizontal line
        let step = if target_dx > 0 { 1 } else { -1 };
        let start = cx + step * 3;
        let end = cx + target_dx - step * 5;
        let mut x = start;
        while (step > 0 && x <= end) || (step < 0 && x >= end) {
            render_text_at(frame, inner, x, cy, "-", style);
            x += step;
        }
    } else {
        // Diagonal line
        let steps: i32 = 4;
        for i in 1..steps {
            let lx = cx + (target_dx * i) / steps;
            let ly = cy + (target_dy * i) / steps;
            let ch = if target_dy < 0 {
                if target_dx > 0 { "/" } else { "\\" }
            } else {
                if target_dx > 0 { "\\" } else { "/" }
            };
            render_text_at(frame, inner, lx, ly, ch, style);
        }
    }
}

fn render_text_at(frame: &mut Frame<'_>, inner: Rect, x: i32, y: i32, text: &str, style: Style) {
    let ix = inner.x as i32;
    let iy = inner.y as i32;
    let iw = inner.width as i32;
    let ih = inner.height as i32;

    if y < iy || y >= iy + ih {
        return;
    }
    if x >= ix + iw || x + text.len() as i32 <= ix {
        return;
    }

    let start_x = x.max(ix) as u16;
    let end_x = (x + text.len() as i32).min(ix + iw) as u16;
    let text_offset = (start_x as i32 - x) as usize;
    let visible_len = (end_x - start_x) as usize;

    let visible = &text[text_offset..text_offset + visible_len];
    let rect = Rect::new(start_x, y as u16, visible_len as u16, 1);
    frame.render_widget(Paragraph::new(visible).style(style), rect);
}

fn render_text_centered(frame: &mut Frame<'_>, inner: Rect, cx: i32, y: i32, text: &str, style: Style) {
    let x = cx - (text.len() as i32) / 2;
    render_text_at(frame, inner, x, y, text, style);
}

fn find_came_from(state: &ClientState, loc: iac_shared::hex::Hex) -> Option<usize> {
    let prev = state.prev_fleet_location?;
    for (i, &dir) in HexDirection::ALL.iter().enumerate() {
        if loc.neighbor(dir) == prev {
            return Some(i);
        }
    }
    None
}

fn sector_has_connection(sector: &SectorState, target: iac_shared::hex::Hex) -> bool {
    sector.connections.contains(&target)
}

fn format_resource_summary(sector_data: Option<&SectorState>) -> String {
    let sd = match sector_data {
        Some(s) => s,
        None => return "???".to_string(),
    };
    let res = &sd.resources;
    if res.metal == iac_shared::constants::Density::None
        && res.crystal == iac_shared::constants::Density::None
        && res.deuterium == iac_shared::constants::Density::None
    {
        return "---".to_string();
    }
    format!(
        "Fe:{} Cr:{}",
        density_short(res.metal),
        density_short(res.crystal),
    )
}

fn behavior_label(behavior: iac_shared::protocol::NpcBehavior) -> &'static str {
    match behavior {
        iac_shared::protocol::NpcBehavior::Passive => "passive",
        iac_shared::protocol::NpcBehavior::Patrol => "patrol",
        iac_shared::protocol::NpcBehavior::Aggressive => "aggressive",
        iac_shared::protocol::NpcBehavior::Swarm => "swarm",
    }
}
