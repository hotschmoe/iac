// IAC Client — async TUI game client.
//
// Architecture:
//   - Tokio runtime drives WebSocket I/O in a background task.
//   - Main thread runs the ratatui event loop (crossterm input + render).
//   - Channels (mpsc) carry server messages and commands between the two.

use std::sync::Arc;

use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc;
use tokio::sync::Mutex as TokioMutex;

use iac_shared::constants::{DEFAULT_HOST, DEFAULT_PORT};
use iac_shared::protocol::ClientMessage;

mod connection;
mod headless;
mod input;
mod renderer;
mod state;
mod token_store;

use connection::Connection;
use state::ClientState;

/// IAC Game Client
#[derive(Parser, Debug)]
#[command(name = "iac-client", version, about)]
struct Cli {
    /// Server host
    #[arg(long, default_value = DEFAULT_HOST)]
    host: String,

    /// Server port
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,

    /// Player name
    #[arg(long, default_value = "Admiral")]
    name: String,

    /// Headless NDJSON mode: server messages on stdout (one JSON per
    /// line), commands from stdin. Built for LLM agents and scripts.
    #[arg(long)]
    headless: bool,

    /// Auth token for this name. Defaults to the one saved for this server
    /// and name; a token issued on first login is saved automatically.
    #[arg(long)]
    token: Option<String>,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    if let Err(e) = run_client().await {
        eprintln!("iac-client: {e}");
        std::process::exit(1);
    }
}

async fn run_client() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();

    let config = connection::ConnectionConfig {
        host: cli.host.clone(),
        port: cli.port,
    };
    if cli.headless {
        return headless::run(&config, &cli.name, cli.token.as_deref()).await;
    }

    // Log in before taking over the terminal so a refusal prints normally.
    let (conn, _) = connection::login(&config, &cli.name, cli.token.as_deref(), false).await?;

    // Initialize terminal
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    crossterm::execute!(stdout, crossterm::cursor::Hide)?;

    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;

    // Restore the terminal even if run() panics, so a crash doesn't leave
    // the shell stuck in raw mode / alternate screen.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let _ = crossterm::execute!(std::io::stdout(), crossterm::cursor::Show);
        default_hook(info);
    }));

    let result = run(terminal, conn).await;

    // Restore terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;
    crossterm::execute!(std::io::stdout(), crossterm::cursor::Show)?;

    result
}

async fn run(
    mut terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
    conn: Connection,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Shared state
    let state = Arc::new(TokioMutex::new(ClientState::new()));

    // Channels:
    //   - server_tx / server_rx: WebSocket task → TUI loop (server messages)
    //   - cmd_tx / cmd_rx: TUI loop → WebSocket task (outgoing commands)
    let (server_tx, mut server_rx) = mpsc::unbounded_channel();
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();

    // Extract conn.tx before moving conn.rx into Arc
    let conn_tx = conn.tx.clone();
    let conn_rx = Arc::new(conn.rx);

    // Spawn WebSocket reader task
    let reader_handle = tokio::spawn({
        let conn_rx = conn_rx.clone();
        async move {
            loop {
                let msg = {
                    let mut rx = conn_rx.lock().await;
                    rx.recv().await
                };
                match msg {
                    Some(text) => {
                        if let Ok(server_msg) = connection::parse_server_message(&text)
                            && server_tx.send(server_msg).is_err() {
                                break;
                            }
                    }
                    None => break, // connection closed
                }
            }
        }
    });

    // Main TUI loop
    let mut running = true;
    let tick_rate = std::time::Duration::from_millis(100);
    let mut last_tick = std::time::Instant::now();

    while running {
        // Drain any pending server messages
        while let Ok(server_msg) = server_rx.try_recv() {
            let mut st = state.lock().await;
            if st.apply_server_message(&server_msg).is_err() {
                log::warn!("Failed to apply server message");
            }
        }

        // Render
        {
            let st = state.lock().await;
            terminal.draw(|frame| {
                renderer::view(frame, &st);
            })?;
        }

        // Poll for input with timeout
        let timeout = if last_tick.elapsed() >= tick_rate {
            std::time::Duration::from_millis(1)
        } else {
            tick_rate - last_tick.elapsed()
        };

        if event::poll(timeout)?
            && let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // Lock state, map key, then handle action
                let action = {
                    let st = state.lock().await;
                    input::map_key(key, &st)
                };

                match action {
                    input::InputAction::Quit => {
                        running = false;
                    }
                    input::InputAction::SwitchView(view) => {
                        let mut st = state.lock().await;
                        st.set_view(view);
                    }
                    input::InputAction::SendCommand(cmd) => {
                        // Manual fleet orders override a plotted course.
                        {
                            use iac_shared::protocol::Command as C;
                            let mut st = state.lock().await;
                            let manual_nav = matches!(
                                cmd, C::Move { .. } | C::Stop { .. } | C::Recall { .. }
                            );
                            if manual_nav && st.nav_route.is_some() {
                                st.nav_route = None;
                            }
                        }
                        let msg = ClientMessage::Command(cmd);
                        let _ = cmd_tx.send(msg);
                    }
                    input::InputAction::Scroll(dir) => {
                        let mut st = state.lock().await;
                        st.scroll_map(dir);
                    }
                    input::InputAction::Zoom(level) => {
                        let mut st = state.lock().await;
                        st.set_zoom(level);
                    }
                    input::InputAction::CycleFleet => {
                        let mut st = state.lock().await;
                        st.cycle_fleet();
                    }
                    input::InputAction::CycleTarget => {
                        let mut st = state.lock().await;
                        st.cycle_target();
                    }
                    input::InputAction::AdjustBuildCount(delta) => {
                        let mut st = state.lock().await;
                        st.adjust_build_count(delta);
                    }
                    input::InputAction::CenterFleet => {
                        let mut st = state.lock().await;
                        if let Some(fleet) = st.active_fleet() {
                            st.map_center = fleet.location;
                        }
                    }
                    input::InputAction::ToggleInfo => {
                        let mut st = state.lock().await;
                        st.show_sector_info = !st.show_sector_info;
                        st.show_keybinds = false;
                    }
                    input::InputAction::CloseLeaderboard => {
                        state.lock().await.show_leaderboard = false;
                    }
                    input::InputAction::ToggleKeybinds => {
                        let mut st = state.lock().await;
                        st.show_keybinds = !st.show_keybinds;
                        st.show_sector_info = false;
                        st.show_tech_tree = false;
                    }
                    input::InputAction::HomeworldNav(nav) => {
                        let mut st = state.lock().await;
                        if let Some(cmd) = st.homeworld_nav(nav) {
                            let msg = ClientMessage::Command(cmd);
                            let _ = cmd_tx.send(msg);
                        }
                    }
                    input::InputAction::ToggleTechTree => {
                        let mut st = state.lock().await;
                        st.show_tech_tree = !st.show_tech_tree;
                        st.show_keybinds = false;
                    }
                    input::InputAction::CyclePolicy => {
                        // Advance the active fleet to the next doctrine;
                        // wraps back to manual (clears orders).
                        let st = state.lock().await;
                        if let Some(fleet) = st.active_fleet() {
                            use iac_shared::protocol::PolicyPreset;
                            let presets = PolicyPreset::ALL;
                            let current = fleet.policy.unwrap_or(PolicyPreset::Manual);
                            let idx = presets.iter().position(|p| *p == current).unwrap_or(0);
                            let next = presets[(idx + 1) % presets.len()];
                            let msg = ClientMessage::PolicyUpdate(iac_shared::protocol::PolicyUpdate {
                                fleet_id: fleet.id,
                                preset: next,
                                params: None,
                            });
                            let _ = cmd_tx.send(msg);
                        }
                    }
                    input::InputAction::ToggleFleetPanel => {
                        state.lock().await.toggle_fleet_panel();
                    }
                    input::InputAction::FleetPanelMove(delta) => {
                        state.lock().await.fleet_panel_move(delta);
                    }
                    input::InputAction::FleetPanelMark => {
                        state.lock().await.fleet_panel_mark();
                    }
                    input::InputAction::FleetPanelSplit => {
                        let cmd = state.lock().await.fleet_panel_split();
                        if let Some(cmd) = cmd {
                            let _ = cmd_tx.send(ClientMessage::Command(cmd));
                        }
                    }
                    input::InputAction::FleetPanelMerge => {
                        let cmd = state.lock().await.fleet_panel_merge();
                        if let Some(cmd) = cmd {
                            let _ = cmd_tx.send(ClientMessage::Command(cmd));
                        }
                    }
                    input::InputAction::PlotRoute => {
                        let mut st = state.lock().await;
                        let target = st.map_center;
                        match st.plot_route(target) {
                            Some((fleet_id, hops)) => {
                                st.status_message = format!(
                                    "course plotted: {} hops to [{},{}]",
                                    hops.len(), target.q, target.r
                                );
                                st.status_set_tick = st.tick;
                                st.nav_route = Some((fleet_id, hops));
                            }
                            None => {
                                st.status_message = "NO KNOWN ROUTE".to_string();
                                st.status_set_tick = st.tick;
                            }
                        }
                    }
                    input::InputAction::ClearRoute => {
                        let mut st = state.lock().await;
                        if st.nav_route.take().is_some() {
                            st.status_message = "course cleared".to_string();
                            st.status_set_tick = st.tick;
                        }
                    }
                    input::InputAction::None => {}
                }
            }

        // Fly the plotted course: one hop per tick as the fleet idles.
        {
            let mut st = state.lock().await;
            if let Some(cmd) = st.next_route_move() {
                let _ = cmd_tx.send(ClientMessage::Command(cmd));
            }
        }

        // Process outgoing commands
        while let Ok(msg) = cmd_rx.try_recv() {
            let json = serde_json::to_string(&msg).unwrap_or_default();
            let _ = conn_tx.send(tokio_tungstenite::tungstenite::protocol::Message::Text(json.into()));
        }

        last_tick = std::time::Instant::now();
    }

    // Cleanup
    reader_handle.abort();

    Ok(())
}
