// WebSocket server with tokio-tungstenite.
// Session management, message routing, state broadcasting.
// Ported from Zig network.zig.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};
use tokio::net::TcpStream;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{sink::SinkExt, stream::StreamExt};
use log::{info, warn};

use iac_shared::hex::Hex;
use iac_shared::protocol::{
    ClientMessage, ServerMessage, Command, ErrorCode,
    GameState, PlayerState, FleetState, ShipState, SectorState, SectorResources,
    NpcFleetInfo, NpcShipInfo, FleetBrief, ShipClassCounts,
    HomeworldState, BuildingState, ResearchState, BuildQueueItem, ShipyardQueueItem, ResearchItem,
    GameEvent,
};
use iac_shared::constants::{ShipClass, DEFAULT_HOST};
use iac_shared::scaling::{self, BuildingType, ResearchType};

use crate::engine::{GameEngine, FleetStatus, SectorOverride};

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
    pub client_type: ClientType,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientType {
    Unknown,
    TuiHuman,
    LlmAgent,
    WebClient,
    Spectator,
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

    pub async fn start_listening(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = format!("{}:{}", DEFAULT_HOST, self.port);
        let listener = TcpListener::bind(&addr).await?;
        info!("WebSocket server listening on {}", addr);

        let sessions = Arc::clone(&self.sessions);
        let next_session_id = Arc::clone(&self.next_session_id);
        let incoming_tx = self.incoming_tx.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let session_id = {
                            let mut nsid = next_session_id.lock().unwrap();
                            let id = *nsid;
                            *nsid += 1;
                            id
                        };

                        let ws_stream = match tokio_tungstenite::accept_async(stream).await {
                            Ok(ws) => ws,
                            Err(e) => {
                                warn!("WebSocket handshake failed: {}", e);
                                continue;
                            }
                        };
                        let (ws_write, ws_read) = ws_stream.split();

                        let (tx, rx) = mpsc::unbounded_channel();
                        tokio::spawn(writer_task(ws_write, rx));

                        let session = ClientSession {
                            id: session_id,
                            sender: Some(tx),
                            player_id: None,
                            authenticated: false,
                            client_type: ClientType::Unknown,
                        };

                        sessions.lock().unwrap().insert(session_id, session);
                        info!("Client connected (session {})", session_id);

                        // Spawn reader task
                        let sessions_clone = Arc::clone(&sessions);
                        let incoming_tx_clone = incoming_tx.clone();
                        tokio::spawn(reader_task(
                            session_id,
                            ws_read,
                            sessions_clone,
                            incoming_tx_clone,
                        ));
                    }
                    Err(e) => {
                        warn!("Accept error: {}", e);
                    }
                }
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

                let registration = {
                    let mut engine = self.engine.lock().unwrap();
                    engine.register_player(auth.player_name.clone())
                };
                let player_id = match registration {
                    Ok(id) => id,
                    Err(code) => {
                        // Tell the client auth failed instead of going silent.
                        self.send_to_session(&sess, ServerMessage::AuthResult(iac_shared::protocol::AuthResult {
                            success: false,
                            player_id: None,
                            token: None,
                            message: Some(format!("Registration failed: {:?}", code)),
                        }))?;
                        return Ok(());
                    }
                };

                {
                    let mut sessions = self.sessions.lock().unwrap();
                    if let Some(s) = sessions.get_mut(&session_id) {
                        s.player_id = Some(player_id);
                        s.authenticated = true;
                        s.client_type = if auth.token.is_some() {
                            ClientType::LlmAgent
                        } else {
                            ClientType::TuiHuman
                        };
                    }
                }

                let result = ServerMessage::AuthResult(iac_shared::protocol::AuthResult {
                    success: true,
                    player_id: Some(player_id),
                    token: None,
                    message: None,
                });
                self.send_to_session(&sess, result)?;

                self.send_full_state(&sess, player_id)?;
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
            Command::Harvest { fleet_id, resource: _ } => engine.handle_harvest(pid, fleet_id),
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
        drop(engine);

        if let Err(e) = result {
            let action = match cmd {
                Command::Move { .. } => "Move",
                Command::Harvest { .. } => "Harvest",
                Command::Recall { .. } => "Recall",
                Command::CollectSalvage { .. } => "Salvage",
                Command::Attack { .. } => "Attack",
                Command::Build { .. } => "Build",
                Command::Research { .. } => "Research",
                Command::BuildShip { .. } => "BuildShip",
                Command::CancelBuild { .. } => "Cancel",
                Command::Stop { .. } => "Stop",
                Command::Scan { .. } => "Scan",
                Command::ExploreSite { .. } => "ExploreSite",
            };
            self.send_error_to_session(session, e, &format!("{} failed", action))?;
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

            let fleet_updates = collect_player_fleets(&engine, player_id);
            let player_data = engine.players.get(&player_id);
            let sector_updates = collect_visible_sectors(&engine, &fleet_updates, player_data, player_id);

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
                fleet_updates: if fleet_updates.is_empty() { None } else { Some(fleet_updates) },
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

// ── WebSocket I/O Tasks ───────────────────────────────────────────

async fn writer_task(
    mut ws: SplitSink<WebSocketStream<TcpStream>, Message>,
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
    mut ws: SplitStream<WebSocketStream<TcpStream>>,
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

// ── State Builders ────────────────────────────────────────────────

fn collect_player_fleets(engine: &GameEngine, player_id: u64) -> Vec<FleetState> {
    let mut list = Vec::new();
    for fleet in engine.fleets.values() {
        if fleet.owner_id != player_id { continue; }

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
            fuel: fleet.fuel,
            fuel_max: fleet.fuel_max,
            cooldown_remaining: fleet.action_cooldown,
            policy: engine.policies.get(&fleet.id).map(|p| p.preset),
        });
    }
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
        buildings,
        research,
        build_queue,
        shipyard_queue,
        research_active,
        docked_ships,
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
