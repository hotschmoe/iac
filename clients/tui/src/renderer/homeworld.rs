// Homeworld view: tab bar, card grid, status bar, tech tree.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::Paragraph,
    Frame,
};

use iac_shared::constants::Resources;
use iac_shared::protocol::{HomeworldState, Requirement};

use crate::state::{ClientState, HomeworldTab};
use super::{
    AMBER, AMBER_BRIGHT, AMBER_DIM, AMBER_FAINT, AMBER_FULL,
    GREEN_GOOD, RED_DIM,
    queue_progress, titled_block,
};

pub fn render(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(1), Constraint::Length(3)]).split(area);

    render_tab_bar(frame, state, rows[0]);
    render_card_grid(frame, state, rows[1]);
    render_status_bar(frame, state, rows[2]);
}

fn render_tab_bar(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let tabs = [
        (HomeworldTab::Buildings, "Buildings"),
        (HomeworldTab::Shipyard, "Shipyard"),
        (HomeworldTab::Research, "Research"),
    ];

    let mut parts = String::new();
    for (tab, label) in &tabs {
        if *tab == state.homeworld_tab {
            parts.push_str(&format!(" [*{}] ", label));
        } else {
            parts.push_str(&format!(" [ {} ] ", label));
        }
    }
    frame.render_widget(Paragraph::new(parts.as_str()).style(AMBER_FULL), area);
}

fn render_card_grid(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let hw = match &state.homeworld {
        Some(h) => h,
        None => {
            frame.render_widget(Paragraph::new(" No homeworld data").style(AMBER_FAINT), area);
            return;
        }
    };

    let count = state.homeworld_tab.item_count();
    let num_rows = count.div_ceil(2);
    let card_height = if num_rows > 0 && area.height >= num_rows as u16 {
        area.height / num_rows as u16
    } else {
        6
    };

    let row_constraints: Vec<Constraint> = (0..num_rows)
        .map(|_| Constraint::Length(card_height))
        .collect();
    let grid_rows = Layout::vertical(row_constraints).split(area);

    for ri in 0..num_rows {
        let cols = Layout::horizontal([Constraint::Min(1), Constraint::Min(1)]).split(grid_rows[ri]);

        let left_idx = ri * 2;
        let right_idx = ri * 2 + 1;

        render_card(frame, &cols[0], state, hw, left_idx);
        if right_idx < count {
            render_card(frame, &cols[1], state, hw, right_idx);
        }
    }
}

fn render_card(frame: &mut Frame<'_>, area: &Rect, state: &ClientState, hw: &HomeworldState, idx: usize) {
    let is_selected = state.homeworld_cursor == idx;
    let catalog = &hw.catalog;
    let mut content = String::new();
    let maxed;
    // (cost, build time) for the next level/unit; colored by affordability.
    let cost_info: Option<(Resources, u64)>;
    let title: String;
    let requires: &[Requirement];

    match state.homeworld_tab {
        HomeworldTab::Buildings => {
            let Some(o) = catalog.buildings.get(idx) else { return };
            title = format!(" {} ", o.building_type.label());
            requires = &o.requires;
            maxed = o.next.is_none();
            if maxed {
                content.push_str(&format!(" MAX (Lv.{})\n", o.level));
            } else {
                content.push_str(&format!(" Level {}\n", o.level));
            }
            cost_info = o.next.map(|n| (n.cost, n.ticks));
        }
        HomeworldTab::Shipyard => {
            let Some(o) = catalog.ships.get(idx) else { return };
            let batch = state.ship_build_count;
            title = if batch > 1 {
                format!(" {} x{} ", o.ship_class.label(), batch)
            } else {
                format!(" {} ", o.ship_class.label())
            };
            requires = &o.requires;
            if requires.iter().all(|r| r.met) {
                if batch > 1 {
                    content.push_str(&format!(" READY - Enter queues {}\n", batch));
                } else {
                    content.push_str(" READY\n");
                }
            } else {
                content.push_str(" LOCKED\n");
            }
            maxed = false;
            cost_info = Some((o.unit_cost.scale(batch as f32), o.ticks_per_ship * batch as u64));
        }
        HomeworldTab::Research => {
            let Some(o) = catalog.research.get(idx) else { return };
            title = format!(" {} ", o.tech.label());
            requires = &o.requires;
            maxed = o.next.is_none();
            if maxed {
                content.push_str(&format!(" Lv.{}/{} MAX\n", o.level, o.max_level));
            } else {
                content.push_str(&format!(" Lv.{}/{}\n", o.level, o.max_level));
            }
            cost_info = o.next.map(|n| (n.cost, n.ticks));
        }
    }

    for r in requires {
        content.push_str(&format!(" Need: {} {}\n", r.label(), if r.met { "[OK]" } else { "[--]" }));
    }
    let locked = requires.iter().any(|r| !r.met);

    render_card_box(frame, *area, state, &title, &content, cost_info, is_selected, locked, maxed);
}

#[allow(clippy::too_many_arguments)]
fn render_card_box(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &ClientState,
    title: &str,
    content: &str,
    cost_info: Option<(Resources, u64)>,
    is_selected: bool,
    is_locked: bool,
    is_maxed: bool,
) {
    let border_style = if is_selected {
        AMBER_FULL
    } else if is_locked {
        AMBER_FAINT
    } else {
        AMBER_DIM
    };

    let block = titled_block(title, border_style);
    let inner = block.inner(area);
    frame.render_widget(&block, area);

    let text_style = if is_selected && !is_locked {
        AMBER_BRIGHT
    } else if is_locked {
        AMBER_DIM
    } else if is_maxed {
        AMBER
    } else {
        AMBER_BRIGHT
    };

    let mut lines: Vec<Line> = content
        .trim_end_matches('\n')
        .split('\n')
        .map(|s| Line::styled(s.to_string(), text_style))
        .collect();

    // Cost line: green if you can pay for it right now, dim red if not.
    if let Some((cost, ticks)) = cost_info {
        let affordable = state.player.as_ref()
            .map(|p| p.resources.can_afford(cost))
            .unwrap_or(false);
        let cost_style = if is_locked {
            AMBER_DIM
        } else if affordable {
            GREEN_GOOD
        } else {
            RED_DIM
        };
        let mut s = format!(" {:.0} Fe  {:.0} Cr", cost.metal, cost.crystal);
        if cost.deuterium > 0.0 {
            s.push_str(&format!("  {:.0} De", cost.deuterium));
        }
        lines.push(Line::styled(s, cost_style));
        lines.push(Line::styled(format!(" {} ticks", ticks), text_style));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_status_bar(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let block = titled_block(" STATUS ", AMBER_DIM);
    let inner = block.inner(area);
    frame.render_widget(&block, area);

    let hw = match &state.homeworld {
        Some(h) => h,
        None => {
            frame.render_widget(Paragraph::new(" ---").style(AMBER_FAINT), inner);
            return;
        }
    };

    let mut text = String::new();

    // Building queue
    if let Some(q) = &hw.build_queue {
        let (elapsed, total) = queue_progress(state.tick, q.start_tick, q.end_tick);
        text.push_str(&format!(
            " Bld: {} Lv{} {}/{}t [x]",
            q.building_type.label(),
            q.target_level,
            elapsed,
            total,
        ));
    } else {
        text.push_str(" Bld: idle");
    }

    // Shipyard queue
    if let Some(q) = &hw.shipyard_queue {
        let (elapsed, total) = queue_progress(state.tick, q.start_tick, q.end_tick);
        text.push_str(&format!(
            " | Ship: {} {}/{}b {}/{}t [X]",
            q.ship_class.label(),
            q.built,
            q.count,
            elapsed,
            total,
        ));
    } else {
        text.push_str(" | Ship: idle");
    }

    // Research queue
    if let Some(q) = &hw.research_active {
        let (elapsed, total) = queue_progress(state.tick, q.start_tick, q.end_tick);
        text.push_str(&format!(
            " | Res: {} {}/{}t [z]",
            q.tech.label(),
            elapsed,
            total,
        ));
    } else {
        text.push_str(" | Res: idle");
    }

    if state.homeworld_tab == HomeworldTab::Shipyard {
        text.push_str(&format!(" | Batch: x{} [+/-]", state.ship_build_count));
    }

    // Production line
    text.push_str(&format!(
        "\n +{:.2} Fe/t  +{:.2} Cr/t  +{:.2} De/t",
        hw.production.metal, hw.production.crystal, hw.production.deuterium,
    ));

    frame.render_widget(Paragraph::new(text).style(AMBER_BRIGHT), inner);
}

// ── Tech Tree ──────────────────────────────────────────────────────

pub fn render_tech_tree(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let block = titled_block(" TECH TREE ", AMBER_DIM);
    let inner = block.inner(area);
    frame.render_widget(&block, area);

    let hw = match &state.homeworld {
        Some(h) => h,
        None => {
            frame.render_widget(Paragraph::new(" No homeworld data").style(AMBER_FAINT), inner);
            return;
        }
    };

    let cols = Layout::horizontal([Constraint::Min(1), Constraint::Min(1)]).split(inner);
    let catalog = &hw.catalog;

    let need_lines = |lines: &mut Vec<Line<'_>>, requires: &[Requirement], only_unmet: bool| {
        for r in requires.iter().filter(|r| !only_unmet || !r.met) {
            let met = if r.met { "[OK]" } else { "[--]" };
            lines.push(Line::raw(format!("   Need: {} {}", r.label(), met)));
        }
    };

    // Left column: buildings + ships
    {
        let mut lines = Vec::new();
        lines.push(Line::raw(" BUILDINGS"));
        lines.push(Line::raw(" ─────────────────────────────"));

        for o in &catalog.buildings {
            if o.level > 0 {
                lines.push(Line::raw(format!(" {:<20} Lv.{}", o.building_type.label(), o.level)));
            } else {
                lines.push(Line::raw(format!(" {:<20} ---", o.building_type.label())));
            }
            need_lines(&mut lines, &o.requires, true);
        }

        lines.push(Line::raw(""));
        lines.push(Line::raw(" SHIPS"));
        lines.push(Line::raw(" ─────────────────────────────"));

        for o in &catalog.ships {
            let unlocked = o.requires.iter().all(|r| r.met);
            let status = if unlocked { "UNLOCKED" } else { "LOCKED" };
            lines.push(Line::raw(format!(" {:<20} {}", o.ship_class.label(), status)));
            need_lines(&mut lines, &o.requires, true);
        }

        frame.render_widget(Paragraph::new(lines).style(AMBER_BRIGHT), cols[0]);
    }

    // Right column: research
    {
        let mut lines = Vec::new();
        lines.push(Line::raw(" RESEARCH"));
        lines.push(Line::raw(" ─────────────────────────────"));

        for o in &catalog.research {
            lines.push(Line::raw(format!(" {:<20} {}/{}", o.tech.label(), o.level, o.max_level)));
            need_lines(&mut lines, &o.requires, false);
        }

        lines.push(Line::raw("\n [t] Close"));
        frame.render_widget(Paragraph::new(lines).style(AMBER_BRIGHT), cols[1]);
    }
}
