// Homeworld view: tab bar, card grid, status bar, tech tree.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::Paragraph,
    Frame,
};

use iac_shared::constants::ShipClass;
use iac_shared::scaling::{self, BuildingLevels, BuildingType, ResearchLevels, ResearchType};

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

    let bldg_levels = ClientState::building_levels_from_slice(&hw.buildings);
    let res_levels = ClientState::research_levels_from_slice(&hw.research);

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

        render_card(frame, &cols[0], state, left_idx, &bldg_levels, &res_levels);
        if right_idx < count {
            render_card(frame, &cols[1], state, right_idx, &bldg_levels, &res_levels);
        }
    }
}

fn render_card(
    frame: &mut Frame<'_>,
    area: &Rect,
    state: &ClientState,
    idx: usize,
    bldg_levels: &BuildingLevels,
    res_levels: &ResearchLevels,
) {
    let is_selected = state.homeworld_cursor == idx;
    let mut content = String::new();
    let mut maxed = false;
    // (cost, build time) for the next level/unit; colored by affordability.
    let mut cost_info: Option<(iac_shared::constants::Resources, u64)> = None;

    match state.homeworld_tab {
        HomeworldTab::Buildings => {
            let bt = match BuildingType::from_usize(idx) {
                Some(b) => b,
                None => return,
            };
            let level = bldg_levels.get(bt);
            let locked = !scaling::building_prerequisites_met(bt, bldg_levels);
            let title = format!(" {} ", bt.label());

            if level >= scaling::MAX_BUILDING_LEVEL {
                content.push_str(&format!(" MAX (Lv.{})\n", level));
                maxed = true;
            } else {
                content.push_str(&format!(" Level {}\n", level));
            }

            if let Some(prereq) = scaling::building_prerequisites(bt) {
                let met_str = if bldg_levels.get(prereq.building) >= prereq.level {
                    "[OK]"
                } else {
                    "[--]"
                };
                content.push_str(&format!(
                    " Need: {} >={} {}\n",
                    prereq.building.label(),
                    prereq.level,
                    met_str,
                ));
            }

            if level < scaling::MAX_BUILDING_LEVEL {
                cost_info = Some((
                    scaling::building_cost(bt, level + 1),
                    scaling::building_time(bt, level + 1),
                ));
            }

            render_card_box(frame, *area, state, &title, &content, cost_info, is_selected, locked, maxed);
        }
        HomeworldTab::Shipyard => {
            let classes = ShipClass::ALL;
            if idx >= classes.len() {
                return;
            }
            let sc = classes[idx];
            let locked = !scaling::ship_class_unlocked(sc, res_levels);
            let batch = state.ship_build_count;
            let title = if batch > 1 {
                format!(" {} x{} ", sc.label(), batch)
            } else {
                format!(" {} ", sc.label())
            };
            let unlocked = !locked;

            if unlocked {
                if batch > 1 {
                    content.push_str(&format!(" READY — Enter queues {}\n", batch));
                } else {
                    content.push_str(" READY\n");
                }
            } else {
                content.push_str(" LOCKED\n");
                let req_label = match sc {
                    ShipClass::Corvette => "Corvette Tech >=1",
                    ShipClass::Frigate => "Frigate Tech >=1",
                    ShipClass::Cruiser => "Cruiser Tech >=1",
                    ShipClass::Hauler => "Hauler Tech >=1",
                    ShipClass::Scout => "",
                };
                if !req_label.is_empty() {
                    content.push_str(&format!(" Need: {}\n", req_label));
                }
            }

            let cost = sc.build_cost().scale(batch as f32);
            let sy_level = bldg_levels.get(BuildingType::Shipyard);
            let ticks = scaling::ship_build_time(sc, sy_level) * batch as u64;
            cost_info = Some((cost, ticks));

            render_card_box(frame, *area, state, &title, &content, cost_info, is_selected, locked, false);
        }
        HomeworldTab::Research => {
            let rt = match ResearchType::from_usize(idx) {
                Some(r) => r,
                None => return,
            };
            let level = res_levels.get(rt);
            let max_level = scaling::research_max_level(rt);
            let met = scaling::research_prerequisites_met(rt, bldg_levels, res_levels);
            let locked = !met;
            let title = format!(" {} ", rt.label());
            maxed = level >= max_level;

            if maxed {
                content.push_str(&format!(" Lv.{}/{} MAX\n", level, max_level));
            } else {
                content.push_str(&format!(" Lv.{}/{}\n", level, max_level));
            }

            // Show prereqs
            let prereqs = scaling::research_prerequisites(rt);
            for prereq in prereqs.iter().flatten() {
                let met_str = match prereq {
                    scaling::ResearchPrereqKind::Building(b) => {
                        if bldg_levels.get(b.building) >= b.level {
                            "[OK]"
                        } else {
                            "[--]"
                        }
                    }
                    scaling::ResearchPrereqKind::Research { tech, level } => {
                        if res_levels.get(*tech) >= *level {
                            "[OK]"
                        } else {
                            "[--]"
                        }
                    }
                };
                let label = match prereq {
                    scaling::ResearchPrereqKind::Building(b) => {
                        format!(" Need: {} >={} {}", b.building.label(), b.level, met_str)
                    }
                    scaling::ResearchPrereqKind::Research { tech, level } => {
                        format!(" Need: {} >={} {}", tech.label(), level, met_str)
                    }
                };
                content.push_str(&format!("{}\n", label));
            }

            if !maxed {
                cost_info = Some((
                    scaling::research_cost(rt, level + 1),
                    scaling::research_time(rt, level + 1),
                ));
            }

            render_card_box(frame, *area, state, &title, &content, cost_info, is_selected, locked, maxed);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_card_box(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &ClientState,
    title: &str,
    content: &str,
    cost_info: Option<(iac_shared::constants::Resources, u64)>,
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
    let bldg_levels = ClientState::building_levels_from_slice(&hw.buildings);
    text.push_str(&format!(
        "\n +{:.2} Fe/t  +{:.2} Cr/t  +{:.2} De/t",
        scaling::production_per_tick(BuildingType::MetalMine, bldg_levels.get(BuildingType::MetalMine)),
        scaling::production_per_tick(BuildingType::CrystalMine, bldg_levels.get(BuildingType::CrystalMine)),
        scaling::production_per_tick(BuildingType::DeuteriumSynthesizer, bldg_levels.get(BuildingType::DeuteriumSynthesizer)),
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

    let bldg_levels = ClientState::building_levels_from_slice(&hw.buildings);
    let res_levels = ClientState::research_levels_from_slice(&hw.research);

    let cols = Layout::horizontal([Constraint::Min(1), Constraint::Min(1)]).split(inner);

    // Left column: buildings + ships
    {
        let mut lines = Vec::new();
        lines.push(Line::raw(" BUILDINGS"));
        lines.push(Line::raw(" ─────────────────────────────"));

        for bt in [
            BuildingType::MetalMine,
            BuildingType::CrystalMine,
            BuildingType::DeuteriumSynthesizer,
            BuildingType::Shipyard,
            BuildingType::ResearchLab,
            BuildingType::FuelDepot,
            BuildingType::SensorArray,
            BuildingType::DefenseGrid,
        ] {
            let level = bldg_levels.get(bt);
            if level > 0 {
                lines.push(Line::raw(format!(" {:<20} Lv.{}\n", bt.label(), level)));
            } else {
                lines.push(Line::raw(format!(" {:<20} ---\n", bt.label())));
            }
            if let Some(prereq) = scaling::building_prerequisites(bt)
                && !scaling::building_prerequisites_met(bt, &bldg_levels) {
                    let met_str = if bldg_levels.get(prereq.building) >= prereq.level {
                        "[OK]"
                    } else {
                        "[--]"
                    };
                    lines.push(Line::raw(format!(
                        "   Need: {} >= {} {}\n",
                        prereq.building.label(),
                        prereq.level,
                        met_str,
                    )));
                }
        }

        lines.push(Line::raw(""));
        lines.push(Line::raw(" SHIPS"));
        lines.push(Line::raw(" ─────────────────────────────"));

        for sc in ShipClass::ALL {
            let unlocked = scaling::ship_class_unlocked(sc, &res_levels);
            let status = if unlocked { "UNLOCKED" } else { "LOCKED" };
            lines.push(Line::raw(format!(" {:<20} {}\n", sc.label(), status)));
            if !unlocked {
                let req = match sc {
                    ShipClass::Corvette => "Corvette Tech >= 1",
                    ShipClass::Frigate => "Frigate Tech >= 1",
                    ShipClass::Cruiser => "Cruiser Tech >= 1",
                    ShipClass::Hauler => "Hauler Tech >= 1",
                    ShipClass::Scout => "",
                };
                if !req.is_empty() {
                    lines.push(Line::raw(format!("   Need: {}\n", req)));
                }
            }
        }

        frame.render_widget(Paragraph::new(lines).style(AMBER_BRIGHT), cols[0]);
    }

    // Right column: research
    {
        let mut lines = Vec::new();
        lines.push(Line::raw(" RESEARCH"));
        lines.push(Line::raw(" ─────────────────────────────"));

        for rt in [
            ResearchType::FuelEfficiency,
            ResearchType::ExtendedFuelTanks,
            ResearchType::ReinforcedHulls,
            ResearchType::AdvancedShields,
            ResearchType::WeaponsResearch,
            ResearchType::Navigation,
            ResearchType::HarvestingEfficiency,
            ResearchType::CorvetteTech,
            ResearchType::FrigateTech,
            ResearchType::CruiserTech,
            ResearchType::HaulerTech,
            ResearchType::EmergencyJump,
        ] {
            let level = res_levels.get(rt);
            let max_level = scaling::research_max_level(rt);
            lines.push(Line::raw(format!(" {:<20} {}/{}\n", rt.label(), level, max_level)));

            let prereqs = scaling::research_prerequisites(rt);
            for prereq in prereqs.iter().flatten() {
                let (label, met_str) = match prereq {
                    scaling::ResearchPrereqKind::Building(b) => {
                        let met = if bldg_levels.get(b.building) >= b.level {
                            "[OK]"
                        } else {
                            "[--]"
                        };
                        (format!("{} >= {}", b.building.label(), b.level), met)
                    }
                    scaling::ResearchPrereqKind::Research { tech, level } => {
                        let met = if res_levels.get(*tech) >= *level {
                            "[OK]"
                        } else {
                            "[--]"
                        };
                        (format!("{} >= {}", tech.label(), level), met)
                    }
                };
                lines.push(Line::raw(format!("   Need: {} {}\n", label, met_str)));
            }
        }

        lines.push(Line::raw("\n [t] Close"));
        frame.render_widget(Paragraph::new(lines).style(AMBER_BRIGHT), cols[1]);
    }
}
