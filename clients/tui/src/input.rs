// Key mapping: crossterm events → InputAction.

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

use iac_shared::hex::HexDirection;
use iac_shared::protocol::{Command, HarvestResource};

use crate::state::{ClientState, HomeworldNav, ScrollDirection, View, ZoomLevel};

#[derive(Debug, Clone)]
pub enum InputAction {
    None,
    Quit,
    SwitchView(View),
    SendCommand(Command),
    Scroll(ScrollDirection),
    Zoom(ZoomLevel),
    CycleFleet,
    CycleTarget,
    AdjustBuildCount(i32),
    CenterFleet,
    ToggleInfo,
    ToggleKeybinds,
    HomeworldNav(HomeworldNav),
    ToggleTechTree,
    /// Cycle the active fleet's standing orders to the next preset.
    CyclePolicy,
    /// Plot a course from the active fleet to the map crosshair.
    PlotRoute,
    /// Clear the plotted course.
    ClearRoute,
    ToggleFleetPanel,
    FleetPanelMove(i32),
    FleetPanelMark,
    FleetPanelSplit,
    FleetPanelMerge,
}

pub fn map_key(key: KeyEvent, state: &ClientState) -> InputAction {
    // Allow SHIFT — crossterm reports '?' and 'X' with SHIFT set.
    if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
        return InputAction::None;
    }

    // Global keys
    if key.code == KeyCode::Esc {
        return InputAction::SwitchView(View::CommandCenter);
    }

    // The fleet panel captures the keyboard while it is open.
    if state.fleet_panel.is_some() {
        return match key.code {
            KeyCode::Esc | KeyCode::Char('f') => InputAction::ToggleFleetPanel,
            KeyCode::Up => InputAction::FleetPanelMove(-1),
            KeyCode::Down => InputAction::FleetPanelMove(1),
            KeyCode::Char(' ') => InputAction::FleetPanelMark,
            KeyCode::Char('s') | KeyCode::Enter => InputAction::FleetPanelSplit,
            KeyCode::Char('g') => InputAction::FleetPanelMerge,
            KeyCode::Tab => InputAction::CycleFleet,
            KeyCode::Char('q') => InputAction::Quit,
            _ => InputAction::None,
        };
    }

    // Homeworld-specific: arrow/tab/enter routing for card navigation
    if state.current_view == View::Homeworld && !state.show_keybinds && !state.show_tech_tree {
        match key.code {
            KeyCode::Tab => return InputAction::HomeworldNav(HomeworldNav::TabNext),
            KeyCode::Up => return InputAction::HomeworldNav(HomeworldNav::CursorUp),
            KeyCode::Down => return InputAction::HomeworldNav(HomeworldNav::CursorDown),
            KeyCode::Left => return InputAction::HomeworldNav(HomeworldNav::CursorLeft),
            KeyCode::Right => return InputAction::HomeworldNav(HomeworldNav::CursorRight),
            KeyCode::Enter => return InputAction::HomeworldNav(HomeworldNav::Select),
            _ => {}
        }
    }

    // Star map: Enter plots a course to the crosshair, Backspace clears it.
    if state.current_view == View::StarMap {
        match key.code {
            KeyCode::Enter => return InputAction::PlotRoute,
            KeyCode::Backspace => return InputAction::ClearRoute,
            _ => {}
        }
    }

    match key.code {
        KeyCode::Char(c) => map_char(c, state),
        KeyCode::Tab => InputAction::CycleFleet,
        KeyCode::Up => InputAction::Scroll(ScrollDirection::Up),
        KeyCode::Down => InputAction::Scroll(ScrollDirection::Down),
        KeyCode::Left => InputAction::Scroll(ScrollDirection::Left),
        KeyCode::Right => InputAction::Scroll(ScrollDirection::Right),
        _ => InputAction::None,
    }
}

fn map_char(c: char, state: &ClientState) -> InputAction {
    // Route unconditionally in Homeworld view so 't' can close the tech tree.
    if state.current_view == View::Homeworld {
        return map_homeworld_char(c);
    }

    match c {
        'q' => InputAction::Quit,
        'w' => InputAction::SwitchView(View::Windshield),
        'm' => InputAction::SwitchView(View::StarMap),
        'b' => InputAction::SwitchView(View::Homeworld),
        '`' => InputAction::SwitchView(View::CommandCenter),

        // Movement keys: map 1-6 to the sector's connected exits
        '1'..='6' => map_movement(c, state),

        'h' => map_harvest(state, HarvestResource::Auto),
        'M' => map_harvest(state, HarvestResource::Metal),
        'C' => map_harvest(state, HarvestResource::Crystal),
        'D' => map_harvest(state, HarvestResource::Deuterium),
        'r' => map_recall(state),
        'a' => map_attack(state),
        's' => map_collect_salvage(state),
        'v' => map_scan(state),
        'S' => map_stop(state),
        'n' => if state.current_view == View::Windshield {
            InputAction::CycleTarget
        } else {
            InputAction::None
        },
        'i' => if state.current_view == View::Windshield {
            InputAction::ToggleInfo
        } else {
            InputAction::None
        },
        '?' => InputAction::ToggleKeybinds,

        'z' => map_zoom_out(state),
        'x' => if state.current_view == View::Windshield {
            map_explore_site(state)
        } else {
            map_zoom_in(state)
        },
        'c' => InputAction::CenterFleet,
        'p' => InputAction::CyclePolicy,
        'f' => InputAction::ToggleFleetPanel,

        _ => InputAction::None,
    }
}

/// Board the derelict in the current sector, if the client knows of one.
fn map_explore_site(state: &ClientState) -> InputAction {
    let Some(fleet) = active_ready_fleet(state) else { return InputAction::None };
    let Some(sector) = state.current_sector() else { return InputAction::None };
    if sector.site.is_none() {
        return InputAction::None;
    }
    InputAction::SendCommand(Command::ExploreSite {
        fleet_id: fleet.id,
    })
}

fn map_homeworld_char(c: char) -> InputAction {
    match c {
        'q' => InputAction::Quit,
        'w' => InputAction::SwitchView(View::Windshield),
        'm' => InputAction::SwitchView(View::StarMap),
        '`' => InputAction::SwitchView(View::CommandCenter),
        '?' => InputAction::ToggleKeybinds,
        't' => InputAction::ToggleTechTree,
        'f' => InputAction::ToggleFleetPanel,

        // Cancel the oldest running item (half refunded) ...
        'x' => InputAction::SendCommand(Command::CancelBuild {
            queue_type: iac_shared::protocol::QueueType::Building,
            index: 0,
        }),
        'X' => InputAction::SendCommand(Command::CancelBuild {
            queue_type: iac_shared::protocol::QueueType::Ship,
            index: 0,
        }),
        'z' => InputAction::SendCommand(Command::CancelBuild {
            queue_type: iac_shared::protocol::QueueType::Research,
            index: 0,
        }),
        // ... or the next waiting one (nothing was paid)
        'c' => InputAction::SendCommand(Command::CancelQueued {
            queue_type: iac_shared::protocol::QueueType::Building,
            index: 0,
        }),
        'C' => InputAction::SendCommand(Command::CancelQueued {
            queue_type: iac_shared::protocol::QueueType::Ship,
            index: 0,
        }),
        'Z' => InputAction::SendCommand(Command::CancelQueued {
            queue_type: iac_shared::protocol::QueueType::Research,
            index: 0,
        }),

        // Shipyard batch size
        '+' | '=' => InputAction::AdjustBuildCount(1),
        '-' | '_' => InputAction::AdjustBuildCount(-1),

        _ => InputAction::None,
    }
}

fn map_movement(key: char, state: &ClientState) -> InputAction {
    if state.current_view != View::Windshield {
        return InputAction::None;
    }

    let Some(fleet) = active_ready_fleet(state) else { return InputAction::None };
    let Some(sector) = state.current_sector() else { return InputAction::None };

    let dir_idx = (key as u8 - b'1') as usize;
    let dir = HexDirection::ALL[dir_idx];
    let target = fleet.location.neighbor(dir);

    for conn in &sector.connections {
        if *conn == target {
            return InputAction::SendCommand(Command::Move {
                fleet_id: fleet.id,
                target,
            });
        }
    }

    InputAction::None
}

fn map_harvest(state: &ClientState, resource: HarvestResource) -> InputAction {
    let Some(fleet) = active_ready_fleet(state) else { return InputAction::None };
    InputAction::SendCommand(Command::Harvest {
        fleet_id: fleet.id,
        resource,
    })
}

fn map_recall(state: &ClientState) -> InputAction {
    let Some(fleet) = active_ready_fleet(state) else { return InputAction::None };
    InputAction::SendCommand(Command::Recall {
        fleet_id: fleet.id,
    })
}

fn map_attack(state: &ClientState) -> InputAction {
    if state.current_view != View::Windshield {
        return InputAction::None;
    }
    let Some(fleet) = active_ready_fleet(state) else { return InputAction::None };
    let Some(sector) = state.current_sector() else { return InputAction::None };
    let Some(hostiles) = sector.hostiles.as_ref() else { return InputAction::None };
    if hostiles.is_empty() {
        return InputAction::None;
    }
    // [n] cycles the target; clamp in case hostiles changed since.
    let target = &hostiles[state.target_idx % hostiles.len()];
    InputAction::SendCommand(Command::Attack {
        fleet_id: fleet.id,
        target_fleet_id: target.id,
    })
}

fn map_scan(state: &ClientState) -> InputAction {
    if state.current_view != View::Windshield {
        return InputAction::None;
    }
    let Some(fleet) = active_ready_fleet(state) else { return InputAction::None };
    InputAction::SendCommand(Command::Scan {
        fleet_id: fleet.id,
    })
}

fn map_stop(state: &ClientState) -> InputAction {
    let Some(fleet) = active_ready_fleet(state) else { return InputAction::None };
    InputAction::SendCommand(Command::Stop {
        fleet_id: fleet.id,
    })
}

fn map_collect_salvage(state: &ClientState) -> InputAction {
    if state.current_view != View::Windshield {
        return InputAction::None;
    }
    let Some(fleet) = active_ready_fleet(state) else { return InputAction::None };
    InputAction::SendCommand(Command::CollectSalvage {
        fleet_id: fleet.id,
    })
}

fn map_zoom_out(state: &ClientState) -> InputAction {
    if state.current_view != View::StarMap {
        return InputAction::None;
    }
    match state.map_zoom {
        ZoomLevel::Close => InputAction::Zoom(ZoomLevel::Sector),
        ZoomLevel::Sector => InputAction::Zoom(ZoomLevel::Region),
        ZoomLevel::Region => InputAction::None,
    }
}

fn map_zoom_in(state: &ClientState) -> InputAction {
    if state.current_view != View::StarMap {
        return InputAction::None;
    }
    match state.map_zoom {
        ZoomLevel::Region => InputAction::Zoom(ZoomLevel::Sector),
        ZoomLevel::Sector => InputAction::Zoom(ZoomLevel::Close),
        ZoomLevel::Close => InputAction::None,
    }
}

/// Returns the active fleet only if it exists and has ships.
fn active_ready_fleet(state: &ClientState) -> Option<&iac_shared::protocol::FleetState> {
    let fleet = state.active_fleet()?;
    if fleet.ships.is_empty() {
        return None;
    }
    Some(fleet)
}

/// Poll for a key event with timeout.
    #[allow(dead_code)]
pub fn poll_event(timeout: std::time::Duration) -> Option<Event> {
    if event::poll(timeout).unwrap_or(false) {
        event::read().ok()
    } else {
        None
    }
}
