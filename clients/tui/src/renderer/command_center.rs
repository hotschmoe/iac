// Command Center view: resources panel, fleet panel, event log.

use ratatui::{
    layout::{Constraint, Layout},
    text::Line,
    widgets::Paragraph,
    Frame,
};

use crate::state::ClientState;
use super::{AMBER, AMBER_BRIGHT, AMBER_DIM, AMBER_FAINT, RED_ALERT, event_lines, titled_block};

pub fn render(frame: &mut Frame<'_>, state: &ClientState, area: ratatui::layout::Rect) {
    // Split into top panels and bottom event log
    let chunks = Layout::vertical([Constraint::Length(8), Constraint::Min(1)]).split(area);

    // Top: resources + fleets side by side
    let top_cols = Layout::horizontal([Constraint::Min(1), Constraint::Min(1)]).split(chunks[0]);

    render_resource_panel(frame, state, top_cols[0]);
    render_fleet_panel(frame, state, top_cols[1]);

    render_event_log(frame, state, chunks[1], " EVENT LOG ");
}

fn render_resource_panel(frame: &mut Frame<'_>, state: &ClientState, area: ratatui::layout::Rect) {
    let block = titled_block(" RESOURCES ", AMBER_DIM);
    let inner = block.inner(area);
    frame.render_widget(&block, area);

    if let Some(player) = &state.player {
        let text = format!(
            " Metal   {:>8.0}\n Crystal {:>8.0}\n Deut    {:>8.0}",
            player.resources.metal,
            player.resources.crystal,
            player.resources.deuterium,
        );
        frame.render_widget(Paragraph::new(text).style(AMBER_BRIGHT), inner);
    } else {
        frame.render_widget(Paragraph::new(" No player data").style(AMBER_FAINT), inner);
    }
}

fn render_fleet_panel(frame: &mut Frame<'_>, state: &ClientState, area: ratatui::layout::Rect) {
    let block = titled_block(" FLEETS ", AMBER_DIM);
    let inner = block.inner(area);
    frame.render_widget(&block, area);

    if state.fleets.is_empty() {
        frame.render_widget(Paragraph::new(" No fleets").style(AMBER_FAINT), inner);
        return;
    }

    let lines: Vec<Line> = state
        .fleets
        .iter()
        .enumerate()
        .flat_map(|(i, fleet)| {
            let marker = if i == state.active_fleet_idx { ">" } else { " " };
            let in_combat = matches!(fleet.state, iac_shared::protocol::FleetStatus::InCombat);
            let status = match fleet.state {
                iac_shared::protocol::FleetStatus::Idle => "idle",
                iac_shared::protocol::FleetStatus::Moving => "moving",
                iac_shared::protocol::FleetStatus::Harvesting => "harvesting",
                iac_shared::protocol::FleetStatus::InCombat => "in_combat",
                iac_shared::protocol::FleetStatus::Returning => "returning",
                iac_shared::protocol::FleetStatus::Docked => "docked",
                iac_shared::protocol::FleetStatus::Exploring => "exploring",
            };
            let mut text = format!(
                " {}Fleet {} [{},{}] {}",
                marker, fleet.id, fleet.location.q, fleet.location.r, status
            );
            if let Some(orders) = fleet.policy {
                text.push_str(&format!(" AUTO:{}", orders.preset.label()));
            }
            if fleet.laden {
                text.push_str(" LADEN");
            }
            if fleet.cooldown_remaining > 0 {
                text.push_str(&format!(" (cd {}s)", fleet.cooldown_remaining));
            }
            let style = if in_combat {
                RED_ALERT
            } else if i == state.active_fleet_idx {
                AMBER_BRIGHT
            } else {
                AMBER
            };
            let settings = fleet.policy.map(|orders| {
                let p = orders.params;
                let fuel = if p.fuel_budget_pct >= iac_shared::constants::POLICY_FUEL_UNBOUNDED_PCT {
                    "unbounded".to_string()
                } else {
                    format!("{}%", p.fuel_budget_pct)
                };
                Line::styled(
                    format!("    range {}  engage {:.1}x  fuel {fuel}", p.max_range, f32::from(p.engage_ratio_x10) / 10.0),
                    AMBER_DIM,
                )
            });
            std::iter::once(Line::styled(text, style)).chain(settings)
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_event_log(frame: &mut Frame<'_>, state: &ClientState, area: ratatui::layout::Rect, title: &str) {
    let block = titled_block(title, AMBER_DIM);
    let inner = block.inner(area);
    frame.render_widget(&block, area);

    let lines = event_lines(state, inner.height as usize, inner.width as usize);
    frame.render_widget(Paragraph::new(lines), inner);
}
