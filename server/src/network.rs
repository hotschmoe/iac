// HTTP/WebSocket server (axum). WebSocket upgrades are accepted on any path
// (the TUI connects to `/`, the web client to `/ws`); plain GETs are served
// from the optional static web dir with SPA fallback.
// Session management, message routing, state broadcasting.
// Ported from Zig network.zig.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use std::path::PathBuf;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::{Request, State};
use axum::http::{header, HeaderName, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tower_http::services::{ServeDir, ServeFile};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{sink::SinkExt, stream::StreamExt};
use log::{info, warn};

use iac_shared::hex::Hex;
use iac_shared::protocol::{
    ClientMessage, ServerMessage, Command, ErrorCode, HarvestResource,
    GameState, PlayerState, FleetState, ShipState, SectorState, SectorResources,
    NpcFleetInfo, NpcShipInfo, FleetBrief, ShipClassCounts,
    HomeworldState, BuildingState, ResearchState, BuildQueueItem, ShipyardQueueItem, ResearchItem,
    GameEvent, AuthResult,
};
use iac_shared::constants::{ShipClass, MAX_FLEETS_PER_PLAYER};
use iac_shared::scaling::{self, BuildingType, ResearchType};

use crate::engine::{fleet_cargo_capacity, AuthError, GameEngine, FleetStatus, SectorOverride};

pub struct Network {
    port: u16,
    engine: Arc<Mutex<GameEngine>>,
    sessions: Arc<Mutex<HashMap<u64, ClientSession>>>,
    next_session_id: Arc<Mutex<u64>>,
    incoming_tx: mpsc::UnboundedSender<QueuedMessage>,
    incoming_rx: Arc<Mutex<mpsc::UnboundedReceiver<QueuedMessage>>>,
}

#[derive(Debug, Clone)]
struct QueuedMessage {
    session_id: u64,
    msg: ClientMessage,
}

#[derive(Debug, Clone)]
pub struct ClientSession {
    pub id: u64,
    pub sender: Option<mpsc::UnboundedSender<Message>>,
    pub player_id: Option<u64>,
    pub authenticated: bool,
}

impl Network {
    pub fn init(port: u16, engine: Arc<Mutex<GameEngine>>) -> Network {
        let (tx, rx) = mpsc::unbounded_channel();
        Network {
            port,
            engine,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            next_session_id: Arc::new(Mutex::new(1)),
            incoming_tx: tx,
            incoming_rx: Arc::new(Mutex::new(rx)),
        }
    }

    pub async fn start_listening(
        &self,
        host: &str,
        web_dir: Option<PathBuf>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = format!("{}:{}", host, self.port);
        let listener = TcpListener::bind(&addr).await?;
        info!("HTTP/WebSocket server listening on {} (ws://{}/ws)", addr, addr);

        let state = HttpState {
            sessions: Arc::clone(&self.sessions),
            next_session_id: Arc::clone(&self.next_session_id),
            incoming_tx: self.incoming_tx.clone(),
            static_dir: web_dir.map(|dir| {
                let index = dir.join("index.html");
                ServeDir::new(dir).fallback(ServeFile::new(index))
            }),
        };
        let app = Router::new().fallback(handle_request).with_state(state);

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                warn!("HTTP server stopped: {}", e);
            }
        });

        Ok(())
    }

    pub fn process_incoming(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Drain the entire queue each tick, matching the Zig server.
        let mut batch = Vec::new();
        {
            let mut rx = self.incoming_rx.lock().unwrap();
            while let Ok(msg) = rx.try_recv() {
                batch.push(msg);
            }
        }

        for queued in batch {
            self.handle_message(queued.session_id, queued.msg).unwrap_or_else(|e| {
                warn!("Error handling message from session {}: {}", queued.session_id, e);
            });
        }

        Ok(())
    }

    fn handle_message(&self, session_id: u64, msg: ClientMessage) -> Result<(), Box<dyn std::error::Error>> {
        let session = {
            let sessions = self.sessions.lock().unwrap();
            sessions.get(&session_id).cloned()
        };

        let sess = session.ok_or("Session not found")?;

        match msg {
            ClientMessage::Auth(auth) => {
                if sess.authenticated {
                    self.send_error_to_session(&sess, ErrorCode::AlreadyAuthenticated, "Already authenticated")?;
                    return Ok(());
                }

                let outcome = {
                    let mut engine = self.engine.lock().unwrap();
                    engine.authenticate(&auth.player_name, auth.token.as_deref())
                };
                let granted = match outcome {
                    Ok(granted) => granted,
                    Err(err) => {
                        let (code, message) = auth_failure(&auth.player_name, err);
                        self.send_to_session(&sess, ServerMessage::AuthResult(AuthResult {
                            success: false,
                            player_id: None,
                            token: None,
                            code: Some(code),
                            message: Some(message),
                        }))?;
                        return Ok(());
                    }
                };

                {
                    let mut sessions = self.sessions.lock().unwrap();
                    if let Some(s) = sessions.get_mut(&session_id) {
                        s.player_id = Some(granted.player_id);
                        s.authenticated = true;
                    }
                }

                let message = match (&granted.new_token, granted.registered) {
                    (Some(_), true) => Some("account created; keep this token, it is your password".to_string()),
                    (Some(_), false) => Some("account claimed; keep this token, it is your password".to_string()),
                    (None, _) => None,
                };
                self.send_to_session(&sess, ServerMessage::AuthResult(AuthResult {
                    success: true,
                    player_id: Some(granted.player_id),
                    token: granted.new_token,
                    code: None,
                    message,
                }))?;

                self.send_full_state(&sess, granted.player_id)?;
            }
            ClientMessage::Command(cmd) => {
                if !sess.authenticated {
                    self.send_error_to_session(&sess, ErrorCode::AuthFailed, "Not authenticated")?;
                    return Ok(());
                }
                self.route_command(&sess, cmd)?;
            }
            ClientMessage::PolicyUpdate(update) => {
                if !sess.authenticated {
                    self.send_error_to_session(&sess, ErrorCode::AuthFailed, "Not authenticated")?;
                    return Ok(());
                }
                let Some(pid) = sess.player_id else { return Ok(()); };
                let result = {
                    let mut engine = self.engine.lock().unwrap();
                    engine.handle_policy_update(pid, update.fleet_id, update.preset, update.params)
                };
                if let Err(e) = result {
                    self.send_error_to_session(&sess, e, "PolicyUpdate failed")?;
                }
            }
            ClientMessage::RequestFullState => {
                if sess.authenticated
                    && let Some(pid) = sess.player_id
                {
                    self.send_full_state(&sess, pid)?;
                }
            }
        }

        Ok(())
    }

    fn route_command(&self, session: &ClientSession, cmd: Command) -> Result<(), Box<dyn std::error::Error>> {
        let Some(pid) = session.player_id else {
            self.send_error_to_session(session, ErrorCode::AuthFailed, "Not authenticated")?;
            return Ok(());
        };

        let mut engine = self.engine.lock().unwrap();
        let result = match cmd {
            Command::Move { fleet_id, target } => engine.handle_move(pid, fleet_id, target),
            Command::Harvest { fleet_id, resource } => engine.handle_harvest(pid, fleet_id, resource),
            Command::Recall { fleet_id } => engine.handle_recall(pid, fleet_id),
            Command::CollectSalvage { fleet_id } => engine.handle_collect_salvage(pid, fleet_id),
            Command::Attack { fleet_id, target_fleet_id } => engine.handle_attack(pid, fleet_id, target_fleet_id),
            Command::Build { building_type } => engine.handle_build(pid, building_type),
            Command::Research { tech } => engine.handle_research(pid, tech),
            Command::BuildShip { ship_class, count } => engine.handle_build_ship(pid, ship_class, count),
            Command::CancelBuild { queue_type } => engine.handle_cancel_build(pid, queue_type),
            Command::Stop { fleet_id } => engine.handle_stop(pid, fleet_id),
            Command::Scan { fleet_id } => engine.handle_scan(pid, fleet_id),
            Command::ExploreSite { fleet_id } => engine.handle_explore_site(pid, fleet_id),
        };
        let failure = result.err().map(|code| (code, command_error_message(&engine, pid, &cmd, code)));
        drop(engine);

        if let Some((code, message)) = failure {
            self.send_error_to_session(session, code, &message)?;
        }

        Ok(())
    }

    pub fn broadcast_updates(&self) -> Result<(), Box<dyn std::error::Error>> {
        let events = {
            let mut engine = self.engine.lock().unwrap();
            engine.drain_events()
        };

        let sessions = self.sessions.lock().unwrap();
        let engine = self.engine.lock().unwrap();

        for (_, session) in sessions.iter() {
            if !session.authenticated { continue; }
            let Some(player_id) = session.player_id else { continue; };

            let fleets = collect_player_fleets(&engine, player_id);
            let player_data = engine.players.get(&player_id);
            let sector_updates = collect_visible_sectors(&engine, &fleets, player_data, player_id);

            let mut player_update: Option<PlayerState> = None;
            let mut hw_update: Option<HomeworldState> = None;
            if let Some(p) = player_data {
                player_update = Some(PlayerState {
                    id: p.id,
                    name: p.name.clone(),
                    resources: p.resources,
                    homeworld: p.homeworld,
                });
                hw_update = Some(build_homeworld_state(&engine, p));
            }

            let player_events: Vec<GameEvent> = events.iter()
                .filter(|e| is_event_relevant(e, player_id, &engine))
                .cloned()
                .collect();

            let update = ServerMessage::TickUpdate(iac_shared::protocol::TickUpdate {
                tick: engine.current_tick(),
                player: player_update,
                fleets,
                sector_updates: if sector_updates.is_empty() { None } else { Some(sector_updates) },
                homeworld_update: hw_update,
                events: if player_events.is_empty() { None } else { Some(player_events) },
            });

            self.send_to_session(session, update).unwrap_or_else(|e| {
                warn!("Failed to send update to session {}: {}", session.id, e);
            });
        }

        Ok(())
    }

    fn send_to_session(&self, session: &ClientSession, msg: ServerMessage) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string(&msg)?;
        if let Some(ref sender) = session.sender {
            sender.send(Message::Text(json.into()))?;
        }
        Ok(())
    }

    fn send_error_to_session(&self, session: &ClientSession, code: ErrorCode, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.send_to_session(session, ServerMessage::Error(iac_shared::protocol::ErrorMessage {
            code,
            message: message.to_string(),
        }))
    }

    fn send_full_state(&self, session: &ClientSession, player_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let engine = self.engine.lock().unwrap();
        let player = engine.players.get(&player_id).ok_or("Player not found")?;

        let fleet_states = collect_player_fleets(&engine, player_id);
        let known_sectors = collect_visible_sectors(&engine, &fleet_states, Some(player), player_id);
        let hw_state = build_homeworld_state(&engine, player);

        let state = ServerMessage::FullState(GameState {
            tick: engine.current_tick(),
            player: PlayerState {
                id: player.id,
                name: player.name.clone(),
                resources: player.resources,
                homeworld: player.homeworld,
            },
            fleets: fleet_states,
            homeworld: hw_state,
            known_sectors,
        });

        drop(engine);
        self.send_to_session(session, state)
    }
}

fn auth_failure(name: &str, err: AuthError) -> (ErrorCode, String) {
    match err {
        AuthError::InvalidName(reason) => (ErrorCode::InvalidName, format!("invalid player name: {reason}")),
        AuthError::TokenRequired => (
            ErrorCode::TokenRequired,
            format!("player '{name}' is already registered; present its token to log in"),
        ),
        AuthError::InvalidToken => (ErrorCode::InvalidToken, format!("token rejected for player '{name}'")),
        AuthError::Server => (ErrorCode::ServerError, "could not create the account".to_string()),
    }
}

fn command_fleet_id(cmd: &Command) -> Option<u64> {
    match cmd {
        Command::Move { fleet_id, .. }
        | Command::Harvest { fleet_id, .. }
        | Command::Attack { fleet_id, .. }
        | Command::Recall { fleet_id }
        | Command::CollectSalvage { fleet_id }
        | Command::Stop { fleet_id }
        | Command::Scan { fleet_id }
        | Command::ExploreSite { fleet_id } => Some(*fleet_id),
        Command::Build { .. }
        | Command::Research { .. }
        | Command::BuildShip { .. }
        | Command::CancelBuild { .. } => None,
    }
}

fn harvest_label(r: HarvestResource) -> &'static str {
    match r {
        HarvestResource::Metal => "metal",
        HarvestResource::Crystal => "crystal",
        HarvestResource::Deuterium => "deuterium",
        HarvestResource::Auto => "resources",
    }
}

/// Human-readable explanation for a rejected command. The code stays the
/// machine-readable part; this says which fleet, sector or queue is at fault.
fn command_error_message(engine: &GameEngine, player_id: u64, cmd: &Command, code: ErrorCode) -> String {
    let fleet_id = command_fleet_id(cmd);
    let fleet = fleet_id
        .and_then(|id| engine.fleets.get(&id))
        .filter(|f| f.owner_id == player_id && f.ship_count > 0);
    let fid = fleet_id.unwrap_or(0);

    match (code, cmd) {
        (ErrorCode::FleetNotFound, _) => format!("fleet {fid} not found (or it has no ships left)"),
        (ErrorCode::OnCooldown, Command::Attack { target_fleet_id, .. })
            if fleet.is_some_and(|f| f.state != FleetStatus::InCombat && f.state != FleetStatus::Moving) =>
        {
            format!("hostile fleet {target_fleet_id} is already locked in combat")
        }
        (ErrorCode::OnCooldown, _) => match fleet {
            Some(f) if f.state == FleetStatus::InCombat => format!("fleet {fid} is in combat"),
            Some(f) if f.state == FleetStatus::Moving => format!("fleet {fid} is in transit"),
            Some(f) if f.state == FleetStatus::Exploring => format!("fleet {fid} is busy boarding a derelict"),
            Some(f) if f.action_cooldown > 0 => {
                format!("fleet {fid} is on cooldown for {} more ticks", f.action_cooldown)
            }
            _ => format!("fleet {fid} is busy"),
        },
        (ErrorCode::NoConnection, Command::Move { target, .. }) => match fleet {
            Some(f) => format!("no lane from {} to {}", f.location, target),
            None => format!("no lane to {target}"),
        },
        (ErrorCode::InsufficientFuel, Command::Recall { .. }) => {
            format!("fleet {fid} lacks the fuel for an emergency jump home")
        }
        (ErrorCode::InsufficientFuel, _) => format!("fleet {fid} lacks the fuel for this jump"),
        (ErrorCode::FleetLimitReached, _) => {
            format!("fleet limit reached: at most {MAX_FLEETS_PER_PLAYER} fleets can be deployed")
        }
        (ErrorCode::NoResources, Command::Harvest { .. }) => "this sector has nothing left to harvest".to_string(),
        (ErrorCode::NoResources, Command::CollectSalvage { .. }) => "no salvage in this sector".to_string(),
        (ErrorCode::NoResources, _) => "not enough resources".to_string(),
        (ErrorCode::ResourceNotPresent, Command::Harvest { resource, .. }) => {
            format!("this sector has no {} to harvest", harvest_label(*resource))
        }
        (ErrorCode::CargoFull, _) => format!("fleet {fid} cargo hold is full; return home to unload"),
        (ErrorCode::InvalidTarget, Command::Attack { target_fleet_id, .. }) => {
            format!("hostile fleet {target_fleet_id} is not in this sector")
        }
        (ErrorCode::InvalidTarget, Command::ExploreSite { .. }) => "no derelict to board in this sector".to_string(),
        (ErrorCode::InvalidCommand, Command::Stop { .. }) => format!("fleet {fid} has nothing to stop"),
        (ErrorCode::QueueFull, Command::Build { .. }) => "a building is already under construction".to_string(),
        (ErrorCode::QueueFull, Command::Research { .. }) => "a research project is already running".to_string(),
        (ErrorCode::QueueFull, Command::BuildShip { .. }) => "the shipyard is already building".to_string(),
        (ErrorCode::MaxLevelReached, Command::Build { building_type }) => {
            format!("{} is already at its maximum level", building_type.label())
        }
        (ErrorCode::MaxLevelReached, Command::Research { tech }) => {
            format!("{} is already at its maximum level", tech.label())
        }
        (ErrorCode::PrerequisitesNotMet, _) => "prerequisites not met".to_string(),
        (ErrorCode::NoShipyard, _) => "build a shipyard first".to_string(),
        (ErrorCode::NoResearchLab, _) => "build a research lab first".to_string(),
        (ErrorCode::ShipLocked, Command::BuildShip { ship_class, .. }) => {
            format!("{} hulls are not unlocked yet; research the matching tech", ship_class.label())
        }
        (ErrorCode::ServerError, _) => "internal server error".to_string(),
        (code, _) => format!("command rejected ({code:?})"),
    }
}

// ── WebSocket I/O Tasks ───────────────────────────────────────────

async fn writer_task(
    mut ws: SplitSink<WebSocket, Message>,
    mut rx: mpsc::UnboundedReceiver<Message>,
) {
    while let Some(msg) = rx.recv().await {
        if let Err(e) = ws.send(msg).await {
            warn!("Write error: {}", e);
            break;
        }
    }
}

async fn reader_task(
    session_id: u64,
    mut ws: SplitStream<WebSocket>,
    sessions: Arc<Mutex<HashMap<u64, ClientSession>>>,
    incoming_tx: mpsc::UnboundedSender<QueuedMessage>,
) {
    while let Some(msg) = ws.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let parsed: Result<ClientMessage, _> = serde_json::from_str(&text);
                match parsed {
                    Ok(client_msg) => {
                        if let Err(e) = incoming_tx.send(QueuedMessage {
                            session_id,
                            msg: client_msg,
                        }) {
                            warn!("Failed to queue message: {}", e);
                        }
                    }
                    Err(e) => {
                        warn!("JSON parse error from session {}: {}", session_id, e);
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(e) => {
                warn!("Read error from session {}: {}", session_id, e);
                break;
            }
        }
    }

    // Clean up session on disconnect
    {
        let mut sessions = sessions.lock().unwrap();
        if let Some(session) = sessions.remove(&session_id) {
            info!("Client disconnected (session {}, player {:?})", session_id, session.player_id);
        }
    }
}

// ── HTTP transport ────────────────────────────────────────────────

#[derive(Clone)]
struct HttpState {
    sessions: Arc<Mutex<HashMap<u64, ClientSession>>>,
    next_session_id: Arc<Mutex<u64>>,
    incoming_tx: mpsc::UnboundedSender<QueuedMessage>,
    static_dir: Option<ServeDir<ServeFile>>,
}

/// Single entry point for every path: WebSocket upgrades (any path) become
/// game sessions; everything else is static files, if a web dir is configured.
async fn handle_request(
    State(state): State<HttpState>,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    req: Request,
) -> Response {
    if let Ok(ws) = ws {
        return ws.on_upgrade(move |socket| run_session(socket, state));
    }
    let Some(mut dir) = state.static_dir else {
        return (axum::http::StatusCode::NOT_FOUND, "iac-server: WebSocket only (no web dir configured)\n")
            .into_response();
    };
    let mut resp = match dir.try_call(req).await {
        Ok(r) => r.into_response(),
        Err(e) => {
            warn!("Static file error: {}", e);
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let h = resp.headers_mut();
    // Always revalidate (ServeDir sends Last-Modified, so this is cheap) so
    // a rebuilt client is picked up without a hard refresh.
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    // Cross-origin isolation, needed by Flutter wasm (multi-threaded renderer).
    h.insert(HeaderName::from_static("cross-origin-opener-policy"), HeaderValue::from_static("same-origin"));
    h.insert(HeaderName::from_static("cross-origin-embedder-policy"), HeaderValue::from_static("require-corp"));
    resp
}

async fn run_session(socket: WebSocket, state: HttpState) {
    let session_id = {
        let mut nsid = state.next_session_id.lock().unwrap();
        let id = *nsid;
        *nsid += 1;
        id
    };
    let (ws_write, ws_read) = socket.split();
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(writer_task(ws_write, rx));

    state.sessions.lock().unwrap().insert(session_id, ClientSession {
        id: session_id,
        sender: Some(tx),
        player_id: None,
        authenticated: false,
    });
    info!("Client connected (session {})", session_id);

    reader_task(session_id, ws_read, state.sessions, state.incoming_tx).await;
}

// ── State Builders ────────────────────────────────────────────────

fn collect_player_fleets(engine: &GameEngine, player_id: u64) -> Vec<FleetState> {
    let mut list = Vec::new();
    for fleet in engine.fleets.values() {
        if fleet.owner_id != player_id || fleet.ship_count == 0 { continue; }

        let ship_states: Vec<ShipState> = fleet.ships[0..fleet.ship_count].iter().map(|ship| {
            ShipState {
                id: ship.id,
                ship_class: ship.ship_class,
                hull: ship.hull,
                hull_max: ship.hull_max,
                shield: ship.shield,
                shield_max: ship.shield_max,
                weapon_power: ship.weapon_power,
            }
        }).collect();

        let status = match fleet.state {
            FleetStatus::Idle => iac_shared::protocol::FleetStatus::Idle,
            FleetStatus::Moving => iac_shared::protocol::FleetStatus::Moving,
            FleetStatus::Harvesting => iac_shared::protocol::FleetStatus::Harvesting,
            FleetStatus::InCombat => iac_shared::protocol::FleetStatus::InCombat,
            FleetStatus::Returning => iac_shared::protocol::FleetStatus::Returning,
            FleetStatus::Docked => iac_shared::protocol::FleetStatus::Docked,
            FleetStatus::Exploring => iac_shared::protocol::FleetStatus::Exploring,
        };

        list.push(FleetState {
            id: fleet.id,
            location: fleet.location,
            state: status,
            ships: ship_states,
            cargo: fleet.cargo,
            cargo_capacity: fleet_cargo_capacity(fleet),
            fuel: fleet.fuel,
            fuel_max: fleet.fuel_max,
            cooldown_remaining: fleet.action_cooldown,
            policy: engine.policies.get(&fleet.id).map(|p| p.preset),
        });
    }
    list.sort_by_key(|f| f.id);
    list
}

fn collect_visible_sectors(
    engine: &GameEngine,
    fleet_states: &[FleetState],
    player: Option<&crate::engine::Player>,
    player_id: u64,
) -> Vec<SectorState> {
    let mut sector_list: Vec<SectorState> = Vec::new();
    let mut sector_keys: std::collections::HashSet<u32> = std::collections::HashSet::new();

    for fs in fleet_states {
        let key = fs.location.to_key();
        if sector_keys.insert(key) {
            sector_list.push(build_sector_state(engine, fs.location, Some(player_id)));
        }
    }

    if let Some(p) = player {
        let range = scaling::sensor_range(p.buildings.sensor_array);
        if range > 0 {
            let revealed = engine.get_sensor_revealed_coords(p.homeworld, range);
            for coord in revealed {
                let key = coord.to_key();
                if sector_keys.insert(key) {
                    sector_list.push(build_sector_state(engine, coord, Some(player_id)));
                }
            }
        }
    }

    // Active-scan intel: sectors the player recently pinged keep streaming
    // live updates until the reveal expires.
    for coord in engine.scan_revealed_coords(player_id) {
        let key = coord.to_key();
        if sector_keys.insert(key) {
            sector_list.push(build_sector_state(engine, coord, Some(player_id)));
        }
    }

    sector_list
}

fn build_sector_state(engine: &GameEngine, coord: Hex, exclude_player_id: Option<u64>) -> SectorState {
    let template = engine.world_gen.generate_sector(coord);
    let neighbors = engine.world_gen.connected_neighbors(coord);
    let ov = engine.sector_overrides.get(&coord.to_key());
    let (metal_d, crystal_d, deut_d) = SectorOverride::effective_densities(ov, &template);

    // Hostile NPCs
    let mut hostile_list: Vec<NpcFleetInfo> = Vec::new();
    for npc in engine.npc_fleets.values() {
        if npc.location == coord {
            let mut class_counts: [u16; 5] = [0; 5];
            for ship in &npc.ships[0..npc.ship_count as usize] {
                class_counts[ship.ship_class as usize] += 1;
            }
            let mut ship_info: Vec<NpcShipInfo> = Vec::new();
            for (ci, &count) in class_counts.iter().enumerate() {
                if count > 0 {
                    let sc: ShipClass = match ci {
                        0 => ShipClass::Scout,
                        1 => ShipClass::Corvette,
                        2 => ShipClass::Frigate,
                        3 => ShipClass::Cruiser,
                        4 => ShipClass::Hauler,
                        _ => unreachable!(),
                    };
                    ship_info.push(NpcShipInfo {
                        ship_class: sc,
                        count,
                    });
                }
            }
            let behavior = match npc.behavior {
                iac_shared::world::NpcBehaviorType::Passive => iac_shared::protocol::NpcBehavior::Passive,
                iac_shared::world::NpcBehaviorType::Patrol => iac_shared::protocol::NpcBehavior::Patrol,
                iac_shared::world::NpcBehaviorType::Aggressive => iac_shared::protocol::NpcBehavior::Aggressive,
                iac_shared::world::NpcBehaviorType::Swarm => iac_shared::protocol::NpcBehavior::Swarm,
            };
            hostile_list.push(NpcFleetInfo {
                id: npc.id,
                ships: ship_info,
                behavior,
            });
        }
    }

    // Template NPC (not yet spawned)
    if let Some(npc_tmpl) = &template.npc_template {
        let npc_cleared = ov.map(|o| o.npc_cleared_tick.is_some()).unwrap_or(false);
        if hostile_list.is_empty() && !npc_cleared {
            hostile_list.push(NpcFleetInfo {
                id: 0,
                ships: vec![NpcShipInfo {
                    ship_class: npc_tmpl.ship_class,
                    count: npc_tmpl.count as u16,
                }],
                behavior: match npc_tmpl.behavior {
                    iac_shared::world::NpcBehaviorType::Passive => iac_shared::protocol::NpcBehavior::Passive,
                    iac_shared::world::NpcBehaviorType::Patrol => iac_shared::protocol::NpcBehavior::Patrol,
                    iac_shared::world::NpcBehaviorType::Aggressive => iac_shared::protocol::NpcBehavior::Aggressive,
                    iac_shared::world::NpcBehaviorType::Swarm => iac_shared::protocol::NpcBehavior::Swarm,
                },
            });
        }
    }

    // Other player fleets
    let mut player_fleet_list: Vec<FleetBrief> = Vec::new();
    for fleet in engine.fleets.values() {
        if fleet.location != coord { continue; }
        if fleet.ship_count == 0 { continue; }
        if let Some(excl) = exclude_player_id
            && fleet.owner_id == excl
        {
            continue;
        }
        let owner = engine.players.get(&fleet.owner_id);
        let mut counts = ShipClassCounts::default();
        for ship in &fleet.ships[0..fleet.ship_count] {
            match ship.ship_class {
                ShipClass::Scout => counts.scout += 1,
                ShipClass::Corvette => counts.corvette += 1,
                ShipClass::Frigate => counts.frigate += 1,
                ShipClass::Cruiser => counts.cruiser += 1,
                ShipClass::Hauler => counts.hauler += 1,
            }
        }
        player_fleet_list.push(FleetBrief {
            id: fleet.id,
            owner_name: owner.map(|o| o.name.clone()).unwrap_or_else(|| "Unknown".to_string()),
            ship_count: fleet.ship_count as u16,
            ship_classes: counts,
        });
    }

    SectorState {
        location: coord,
        terrain: template.terrain,
        resources: SectorResources {
            metal: metal_d,
            crystal: crystal_d,
            deuterium: deut_d,
        },
        connections: neighbors.slice().to_vec(),
        hostiles: if hostile_list.is_empty() { None } else { Some(hostile_list) },
        player_fleets: if player_fleet_list.is_empty() { None } else { Some(player_fleet_list) },
        salvage: ov.and_then(|o| o.salvage),
        site: engine.derelict_site_at(coord).map(|(tier, bumps)| iac_shared::protocol::SiteBrief {
            tier,
            risk: crate::engine::site_risk_label(tier, bumps),
        }),
    }
}

fn build_homeworld_state(engine: &GameEngine, player: &crate::engine::Player) -> HomeworldState {
    let mut buildings: Vec<BuildingState> = Vec::new();
    for bt in 0..BuildingType::COUNT {
        let bt: BuildingType = match bt {
            0 => BuildingType::MetalMine,
            1 => BuildingType::CrystalMine,
            2 => BuildingType::DeuteriumSynthesizer,
            3 => BuildingType::Shipyard,
            4 => BuildingType::ResearchLab,
            5 => BuildingType::FuelDepot,
            6 => BuildingType::SensorArray,
            7 => BuildingType::DefenseGrid,
            _ => unreachable!(),
        };
        buildings.push(BuildingState {
            building_type: bt,
            level: player.buildings.get(bt),
        });
    }

    let mut research: Vec<ResearchState> = Vec::new();
    for rt in 0..ResearchType::COUNT {
        let rt: ResearchType = match rt {
            0 => ResearchType::FuelEfficiency,
            1 => ResearchType::ExtendedFuelTanks,
            2 => ResearchType::ReinforcedHulls,
            3 => ResearchType::AdvancedShields,
            4 => ResearchType::WeaponsResearch,
            5 => ResearchType::Navigation,
            6 => ResearchType::HarvestingEfficiency,
            7 => ResearchType::CorvetteTech,
            8 => ResearchType::FrigateTech,
            9 => ResearchType::CruiserTech,
            10 => ResearchType::HaulerTech,
            11 => ResearchType::EmergencyJump,
            _ => unreachable!(),
        };
        research.push(ResearchState {
            tech: rt,
            level: player.research.get(rt),
        });
    }

    let build_queue: Option<BuildQueueItem> = player.building_queue.as_ref().map(|q| BuildQueueItem {
        building_type: q.building_type,
        target_level: q.target_level,
        start_tick: q.start_tick,
        end_tick: q.end_tick,
    });

    let shipyard_queue: Option<ShipyardQueueItem> = player.ship_queue.as_ref().map(|q| ShipyardQueueItem {
        ship_class: q.ship_class,
        count: q.count,
        built: q.built,
        start_tick: q.start_tick,
        end_tick: q.end_tick,
    });

    let research_active: Option<ResearchItem> = player.research_queue.as_ref().map(|q| ResearchItem {
        tech: q.tech,
        target_level: q.target_level,
        start_tick: q.start_tick,
        end_tick: q.end_tick,
    });

    let mut docked_ships: Vec<ShipState> = Vec::new();
    for fleet in engine.fleets.values() {
        if fleet.owner_id == player.id && fleet.location == player.homeworld {
            for ship in &fleet.ships[0..fleet.ship_count] {
                docked_ships.push(ShipState {
                    id: ship.id,
                    ship_class: ship.ship_class,
                    hull: ship.hull,
                    hull_max: ship.hull_max,
                    shield: ship.shield,
                    shield_max: ship.shield_max,
                    weapon_power: ship.weapon_power,
                });
            }
        }
    }

    HomeworldState {
        location: player.homeworld,
        production: player.production_per_tick(),
        buildings,
        research,
        build_queue,
        shipyard_queue,
        research_active,
        docked_ships,
        catalog: iac_shared::protocol::HomeworldCatalog::new(&player.buildings, &player.research),
    }
}

fn is_own_fleet(engine: &GameEngine, fleet_id: u64, player_id: u64) -> bool {
    engine.fleets.get(&fleet_id).map(|f| f.owner_id == player_id).unwrap_or(false)
}

fn player_has_fleet_at_sector(engine: &GameEngine, player_id: u64, sector: Hex) -> bool {
    engine.fleets.values().any(|f| {
        f.owner_id == player_id && f.location == sector && f.ship_count > 0
    })
}

fn is_event_relevant(event: &GameEvent, player_id: u64, engine: &GameEngine) -> bool {
    match &event.kind {
        iac_shared::protocol::EventKind::SectorEntered(e) => is_own_fleet(engine, e.fleet_id, player_id),
        iac_shared::protocol::EventKind::FleetArrived(e) => is_own_fleet(engine, e.fleet_id, player_id),
        iac_shared::protocol::EventKind::ResourceHarvested(e) => is_own_fleet(engine, e.fleet_id, player_id),
        iac_shared::protocol::EventKind::SalvageCollected(e) => is_own_fleet(engine, e.fleet_id, player_id),
        iac_shared::protocol::EventKind::BuildingCompleted(e) => e.player_id.map(|p| p == player_id).unwrap_or(true),
        iac_shared::protocol::EventKind::ResearchCompleted(e) => e.player_id.map(|p| p == player_id).unwrap_or(true),
        iac_shared::protocol::EventKind::ShipBuilt(e) => e.player_id.map(|p| p == player_id).unwrap_or(true),
        iac_shared::protocol::EventKind::CombatStarted(e) => {
            is_own_fleet(engine, e.player_fleet_id, player_id) || player_has_fleet_at_sector(engine, player_id, e.sector)
        }
        iac_shared::protocol::EventKind::CombatEnded(e) => player_has_fleet_at_sector(engine, player_id, e.sector),
        iac_shared::protocol::EventKind::CombatRound(_) => true,
        iac_shared::protocol::EventKind::ShipDestroyed(e) => {
            if !e.is_npc && is_own_fleet(engine, e.owner_fleet_id, player_id) { return true; }
            if let Some(f) = engine.fleets.get(&e.owner_fleet_id)
                && player_has_fleet_at_sector(engine, player_id, f.location)
            {
                return true;
            }
            false
        }
        iac_shared::protocol::EventKind::FleetDestroyed(e) => e.is_npc || is_own_fleet(engine, e.fleet_id, player_id),
        iac_shared::protocol::EventKind::ScanCompleted(e) => is_own_fleet(engine, e.fleet_id, player_id),
        iac_shared::protocol::EventKind::RaidIncoming(e) => e.player_id == player_id,
        iac_shared::protocol::EventKind::RaidResolved(e) => e.player_id == player_id,
        iac_shared::protocol::EventKind::SiteExplorationStarted(e) => is_own_fleet(engine, e.fleet_id, player_id),
        iac_shared::protocol::EventKind::SiteExplored(e) => is_own_fleet(engine, e.fleet_id, player_id),
        iac_shared::protocol::EventKind::SiteAmbush(e) => is_own_fleet(engine, e.fleet_id, player_id),
        iac_shared::protocol::EventKind::PolicyAction(e) => is_own_fleet(engine, e.fleet_id, player_id),
        iac_shared::protocol::EventKind::Alert(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use serde_json::Value;

    fn network_with_player() -> (Network, Arc<Mutex<GameEngine>>, u64, u64, mpsc::UnboundedReceiver<Message>) {
        let engine = GameEngine::init(42, Database::init(":memory:").unwrap()).unwrap();
        let engine = Arc::new(Mutex::new(engine));
        let (pid, fid) = {
            let mut e = engine.lock().unwrap();
            let pid = e.register_player("Watcher".to_string()).unwrap();
            let fid = e.fleets.values().find(|f| f.owner_id == pid).unwrap().id;
            (pid, fid)
        };
        let net = Network::init(0, Arc::clone(&engine));
        let (tx, rx) = mpsc::unbounded_channel();
        net.sessions.lock().unwrap().insert(1, ClientSession {
            id: 1,
            sender: Some(tx),
            player_id: Some(pid),
            authenticated: true,
        });
        (net, engine, pid, fid, rx)
    }

    fn next_message(rx: &mut mpsc::UnboundedReceiver<Message>) -> Value {
        match rx.try_recv().expect("a message was sent") {
            Message::Text(t) => serde_json::from_str(&t).unwrap(),
            other => panic!("unexpected frame {other:?}"),
        }
    }

    #[test]
    fn losing_every_fleet_is_signalled_with_an_empty_list() {
        let (net, engine, _pid, fid, mut rx) = network_with_player();

        net.broadcast_updates().unwrap();
        let before = next_message(&mut rx);
        assert_eq!(before["fleets"].as_array().unwrap().len(), 1);
        assert_eq!(before["fleets"][0]["id"], fid);

        // Combat leaves a wiped-out fleet with no ships.
        engine.lock().unwrap().fleets.get_mut(&fid).unwrap().ship_count = 0;
        net.broadcast_updates().unwrap();
        let during = next_message(&mut rx);
        assert_eq!(during["fleets"], serde_json::json!([]), "dead fleet must not be listed: {during}");

        // Once reaped, the list stays an explicit empty array, never absent.
        engine.lock().unwrap().tick().unwrap();
        assert!(!engine.lock().unwrap().fleets.contains_key(&fid));
        net.broadcast_updates().unwrap();
        let after = next_message(&mut rx);
        assert_eq!(after["fleets"], serde_json::json!([]));
    }

    #[test]
    fn fleet_list_is_ordered_by_id() {
        let (net, engine, pid, fid, mut rx) = network_with_player();
        {
            let mut e = engine.lock().unwrap();
            let mut clone = e.fleets[&fid].clone();
            for id in [fid + 900, fid + 100, fid + 500] {
                clone.id = id;
                e.fleets.insert(id, clone.clone());
            }
            assert_eq!(e.fleets.values().filter(|f| f.owner_id == pid).count(), 4);
        }
        net.broadcast_updates().unwrap();
        let ids: Vec<u64> = next_message(&mut rx)["fleets"]
            .as_array().unwrap().iter().map(|f| f["id"].as_u64().unwrap()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn wrong_resource_harvest_gets_a_specific_error() {
        use iac_shared::constants::Density;
        let (net, engine, _pid, fid, mut rx) = network_with_player();
        let site = {
            let mut e = engine.lock().unwrap();
            let site = (-30i16..30)
                .flat_map(|q| (-30i16..30).map(move |r| Hex { q, r }))
                .find(|&h| {
                    let t = e.world_gen.generate_sector(h);
                    t.metal_density != Density::None && t.deut_density == Density::None
                })
                .expect("a metal-only sector");
            e.fleets.get_mut(&fid).unwrap().location = site;
            site
        };

        let session = net.sessions.lock().unwrap().get(&1).cloned().unwrap();
        net.route_command(&session, Command::Harvest { fleet_id: fid, resource: HarvestResource::Deuterium }).unwrap();

        let reply = next_message(&mut rx);
        assert_eq!(reply["type"], "error");
        assert_eq!(reply["code"], "ResourceNotPresent");
        let msg = reply["message"].as_str().unwrap();
        assert!(msg.contains("deuterium"), "message should name the resource: {msg} (sector {site})");
    }

    #[test]
    fn state_carries_cargo_capacity_and_real_production() {
        let (net, engine, pid, fid, mut rx) = network_with_player();
        {
            let mut e = engine.lock().unwrap();
            e.players.get_mut(&pid).unwrap().buildings.metal_mine = 3;
            e.players.get_mut(&pid).unwrap().buildings.deuterium_synthesizer = 2;
            // Starting scouts plus one hauler.
            let fleet = e.fleets.get_mut(&fid).unwrap();
            fleet.ships[fleet.ship_count].ship_class = ShipClass::Hauler;
            fleet.ship_count += 1;
        }
        let expected_cap: f32 = {
            let e = engine.lock().unwrap();
            e.fleets[&fid].ships[..e.fleets[&fid].ship_count].iter()
                .map(|s| s.ship_class.base_stats().cargo as f32).sum()
        };
        assert!(expected_cap > 200.0, "hauler must be counted: {expected_cap}");

        let before = engine.lock().unwrap().players[&pid].resources;
        engine.lock().unwrap().tick().unwrap();
        let grown = engine.lock().unwrap().players[&pid].resources;
        net.broadcast_updates().unwrap();
        let update = next_message(&mut rx);

        assert_eq!(update["fleets"][0]["cargo_capacity"].as_f64().unwrap() as f32, expected_cap);
        let prod = &update["homeworld_update"]["production"];
        assert!(prod["metal"].as_f64().unwrap() > 0.0);
        assert!((grown.metal - before.metal - prod["metal"].as_f64().unwrap() as f32).abs() < 1e-3,
            "advertised production must equal the stockpile change");
        assert!((grown.crystal - before.crystal - prod["crystal"].as_f64().unwrap() as f32).abs() < 1e-3);
        assert!((grown.deuterium - before.deuterium - prod["deuterium"].as_f64().unwrap() as f32).abs() < 1e-3);
    }

    #[test]
    fn homeworld_catalog_tracks_levels() {
        let (net, engine, pid, _fid, mut rx) = network_with_player();
        net.broadcast_updates().unwrap();
        let update = next_message(&mut rx);
        let mine = &update["homeworld_update"]["catalog"]["buildings"][0];
        assert_eq!(mine["building_type"], "MetalMine");
        assert_eq!(mine["level"], 1);
        assert_eq!(mine["next"]["level"], 2);
        let yard = &update["homeworld_update"]["catalog"]["buildings"][3];
        assert_eq!(yard["requires"][0]["met"], false);

        engine.lock().unwrap().players.get_mut(&pid).unwrap().buildings.metal_mine = 2;
        net.broadcast_updates().unwrap();
        let update = next_message(&mut rx);
        let catalog = &update["homeworld_update"]["catalog"];
        assert_eq!(catalog["buildings"][0]["level"], 2);
        assert_eq!(catalog["buildings"][3]["requires"][0]["met"], true);
    }
}
