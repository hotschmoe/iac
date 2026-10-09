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

use iac_shared::protocol::{AuthResult, ClientMessage, Command, ErrorCode, ServerMessage};
use iac_sim::{commands, views, GameEngine};

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
                    engine.authenticate(&auth.player_name, auth.token.as_deref(), auth.agent, crate::auth::generate_token)
                };
                let granted = match outcome {
                    Ok(granted) => granted,
                    Err(err) => {
                        let (code, message) = commands::auth_failure(&auth.player_name, err);
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

        let outcome = self.engine.lock().unwrap().execute(pid, cmd);

        if let Some(reply) = outcome.reply {
            self.send_to_session(session, reply)?;
        }
        if let Some((code, message)) = outcome.error {
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

            let update = ServerMessage::TickUpdate(views::tick_update(&engine, player_id, &events));

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
        let state = {
            let engine = self.engine.lock().unwrap();
            ServerMessage::FullState(views::full_state(&engine, player_id).ok_or("Player not found")?)
        };
        self.send_to_session(session, state)
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

#[cfg(test)]
mod tests {
    use super::*;
    use iac_shared::constants::ShipClass;
    use iac_shared::hex::Hex;
    use iac_shared::pace::Pace;
    use iac_shared::protocol::{GameEvent, HarvestResource};
    use iac_shared::scaling::{BuildingType, ResearchType};
    use iac_sim::commands::command_error_message;
    use iac_sim::views::event_for;
    use serde_json::Value;

    fn network_with_player() -> (Network, Arc<Mutex<GameEngine>>, u64, u64, mpsc::UnboundedReceiver<Message>) {
        let engine = GameEngine::new(42, Pace::default());
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

    #[test]
    fn prerequisite_errors_name_what_is_missing() {
        let (_net, engine, pid, _fid, _rx) = network_with_player();
        let mut e = engine.lock().unwrap();
        e.players.get_mut(&pid).unwrap().buildings.shipyard = 2;

        let cmd = Command::BuildShip { ship_class: ShipClass::Frigate, count: 1, reserve: false };
        assert_eq!(
            command_error_message(&e, pid, &cmd, ErrorCode::ShipLocked),
            "Frigate needs Frigate Tech level 1 (you have 0)"
        );
        let cmd = Command::Build { building_type: BuildingType::DefenseGrid, reserve: false };
        assert_eq!(
            command_error_message(&e, pid, &cmd, ErrorCode::PrerequisitesNotMet),
            "Defense Grid needs Shipyard level 3 (you have 2)"
        );
        let cmd = Command::Research { tech: ResearchType::FrigateTech, reserve: false };
        assert_eq!(
            command_error_message(&e, pid, &cmd, ErrorCode::PrerequisitesNotMet),
            "Frigate Tech needs Research Lab level 1 (you have 0), Corvette Tech level 1 (you have 0) and Shipyard level 4 (you have 2)"
        );
    }

    #[test]
    fn cancel_refusals_say_where_the_order_actually_is() {
        use iac_shared::protocol::QueueType;
        let (_net, engine, pid, _fid, _rx) = network_with_player();
        let mut e = engine.lock().unwrap();
        e.players.get_mut(&pid).unwrap().resources = iac_shared::constants::Resources { metal: 9e3, crystal: 9e3, deuterium: 9e3 };
        e.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        e.handle_build(pid, BuildingType::CrystalMine, false).unwrap();
        let running = e.players[&pid].building_queue[0].id;
        let waiting = e.players[&pid].building_pending[0].id;

        let msg = |e: &GameEngine, cmd: Command| command_error_message(e, pid, &cmd, ErrorCode::InvalidTarget);
        let cmd = Command::CancelQueued { queue_type: QueueType::Ship, id: waiting };
        assert_eq!(msg(&e, cmd), format!("order {waiting} is in the building queue, not the shipyard queue; nothing was cancelled"));
        let cmd = Command::CancelQueued { queue_type: QueueType::Building, id: running };
        assert_eq!(msg(&e, cmd), format!("order {running} is not waiting in the building queue; use cancel_build for it"));
        let cmd = Command::CancelBuild { queue_type: QueueType::Building, id: waiting };
        assert_eq!(msg(&e, cmd), format!("order {waiting} is not under way in the building queue; use cancel_queued for it"));
        let cmd = Command::CancelQueued { queue_type: QueueType::Building, id: 999_999 };
        assert!(msg(&e, cmd).starts_with("no order 999999 in any queue"));
    }

    fn fight_events(engine: &mut GameEngine, pid: u64, fid: u64, at: Hex) -> Vec<GameEvent> {
        engine.fleets.get_mut(&fid).unwrap().location = at;
        for ship in &mut engine.fleets.get_mut(&fid).unwrap().ships[..] {
            ship.weapon_power = 5000.0;
            ship.hull_max = 1e6;
            ship.hull = 1e6;
        }
        engine.handle_attack(pid, fid, iac_sim::engine::template_npc_id(at)).unwrap();
        let mut events = engine.drain_events();
        for _ in 0..5 {
            engine.tick().unwrap();
            events.extend(engine.drain_events());
        }
        events
    }

    fn is_combat_detail(e: &GameEvent) -> bool {
        use iac_shared::protocol::EventKind as K;
        matches!(
            e.kind,
            K::CombatRound(_) | K::CombatStarted(_) | K::CombatEnded(_) | K::ShipDestroyed(_) | K::FleetDestroyed(_)
        )
    }

    #[test]
    fn combat_reaches_only_the_players_involved_or_present() {
        use iac_shared::protocol::EventKind as K;
        let mut engine = GameEngine::new(42, Pace::default());
        let a = engine.register_player("Alpha".to_string()).unwrap();
        let b = engine.register_player("Bravo".to_string()).unwrap();
        let c = engine.register_player("Charlie".to_string()).unwrap();
        let d = engine.register_player("Delta".to_string()).unwrap();
        let fleet_of = |e: &GameEngine, p: u64| e.fleets.values().find(|f| f.owner_id == p).unwrap().id;
        let (fa, fc) = (fleet_of(&engine, a), fleet_of(&engine, c));

        let at = (-20i16..20)
            .flat_map(|q| (-20i16..20).map(move |r| Hex { q, r }))
            .find(|&h| {
                engine.world_gen.generate_sector(h).npc_template.is_some_and(|t| t.count == 1)
                    && engine.derelict_site_at(h).is_none()
                    && ![a, b, c, d].iter().any(|p| engine.players[p].homeworld == h)
            })
            .unwrap();
        engine.fleets.get_mut(&fc).unwrap().location = at;
        engine.players.get_mut(&d).unwrap().homeworld = at;
        let events = fight_events(&mut engine, a, fa, at);
        assert!(events.iter().any(|e| matches!(e.kind, K::CombatRound(_))), "a fight happened");
        engine.fleets.get_mut(&fa).unwrap().location = engine.players[&a].homeworld;

        let seen = |p: u64| -> Vec<GameEvent> {
            let player = &engine.players[&p];
            events.iter().filter_map(|e| event_for(e, player, &engine)).collect()
        };

        let alpha = seen(a);
        assert!(alpha.iter().any(is_combat_detail));
        for e in alpha.iter().filter(|e| is_combat_detail(e)) {
            match &e.kind {
                K::CombatRound(r) => assert!(r.mine && (r.attacker_owner.as_deref() == Some("Alpha") || r.target_owner.as_deref() == Some("Alpha"))),
                K::CombatStarted(s) => assert!(s.mine && s.owner == "Alpha"),
                K::CombatEnded(s) => assert!(s.mine),
                K::ShipDestroyed(s) => assert!(!s.mine && s.owner.is_none(), "the destroyed ship was an NPC's"),
                K::FleetDestroyed(s) => assert!(!s.mine && s.owner.is_none()),
                _ => unreachable!(),
            }
        }

        assert!(seen(b).iter().all(|e| !is_combat_detail(e)), "uninvolved player gets no combat detail");
        assert!(seen(b).is_empty() || seen(b).iter().all(|e| !matches!(e.kind, K::Alert(_))));

        // Charlie's fleet was in the sector, so the fight swept it in: the outcome is his too.
        let charlie = seen(c);
        assert!(charlie.iter().any(|e| matches!(&e.kind, K::CombatEnded(s) if s.mine && s.owners.len() == 2)));
        assert!(charlie.iter().any(|e| matches!(&e.kind, K::CombatStarted(s) if !s.mine && s.owner == "Alpha")));

        // Delta's homeworld sits in the sector with no fleet: a pure witness.
        let delta = seen(d);
        assert!(delta.iter().any(is_combat_detail), "homeworld presence is enough to see the fight");
        assert!(delta.iter().filter(|e| is_combat_detail(e)).all(|e| match &e.kind {
            K::CombatRound(r) => !r.mine,
            K::CombatStarted(s) => !s.mine,
            K::CombatEnded(s) => !s.mine,
            K::ShipDestroyed(s) => !s.mine,
            K::FleetDestroyed(s) => !s.mine,
            _ => unreachable!(),
        }), "witnessed fights are never flagged mine");
    }

    #[test]
    fn a_players_own_loss_reaches_them_even_when_nobody_else_is_there() {
        use iac_shared::protocol::{AlertLevel, EventKind as K};
        let mut engine = GameEngine::new(42, Pace::default());
        let a = engine.register_player("Alpha".to_string()).unwrap();
        let b = engine.register_player("Bravo".to_string()).unwrap();
        let fa = engine.fleets.values().find(|f| f.owner_id == a).unwrap().id;
        let at = (-20i16..20)
            .flat_map(|q| (-20i16..20).map(move |r| Hex { q, r }))
            .find(|&h| {
                engine.world_gen.generate_sector(h).npc_template.is_some_and(|t| {
                    t.count == 1 && t.behavior != iac_shared::world::NpcBehaviorType::Passive
                })
            })
            .unwrap();
        engine.fleets.get_mut(&fa).unwrap().location = at;
        for ship in &mut engine.fleets.get_mut(&fa).unwrap().ships[..] {
            ship.hull = 0.5;
            ship.shield = 0.0;
        }
        engine.handle_attack(a, fa, iac_sim::engine::template_npc_id(at)).unwrap();
        let mut events = engine.drain_events();
        for _ in 0..8 {
            engine.tick().unwrap();
            events.extend(engine.drain_events());
        }
        // The fleet is reaped by now: routing must not need it.
        assert!(!engine.fleets.contains_key(&fa));
        let for_a: Vec<_> = events.iter().filter_map(|e| event_for(e, &engine.players[&a], &engine)).collect();
        let lost = for_a.iter().find_map(|e| match &e.kind {
            K::FleetDestroyed(d) if !d.is_npc => Some(d.clone()),
            _ => None,
        }).expect("owner is told");
        assert!(lost.mine);
        assert!(for_a.iter().any(|e| matches!(&e.kind, K::Alert(al) if matches!(al.level, AlertLevel::Critical))));
        let id = iac_sim::engine::template_npc_id(at).to_string();
        let alert = for_a.iter().find_map(|e| match &e.kind {
            K::Alert(al) if matches!(al.level, AlertLevel::Critical) => Some(al.message.clone()),
            _ => None,
        }).unwrap();
        assert!(!alert.contains(&id), "{alert}");
        assert!(alert.contains("by hostile ") && (alert.contains("patrol") || alert.contains("pack") || alert.contains("swarm") || alert.contains("convoy")), "{alert}");
        assert!(events.iter().any(|e| matches!(&e.kind, K::CombatStarted(c) if !c.enemy.is_empty() && c.enemy_fleet_id.to_string() == id)));
        let for_b: Vec<_> = events.iter().filter_map(|e| event_for(e, &engine.players[&b], &engine)).collect();
        assert!(for_b.is_empty(), "{for_b:?}");
    }

    #[test]
    fn full_state_carries_stale_sectors_and_the_tick_only_new_ones() {
        let (net, engine, pid, fid, mut rx) = network_with_player();
        {
            let mut e = engine.lock().unwrap();
            e.handle_scan(pid, fid).unwrap();
            e.tick().unwrap();
        }
        net.broadcast_updates().unwrap();
        let live = next_message(&mut rx);
        assert!(live["sector_updates"].as_array().unwrap().iter().all(|s| s["live"] == true));
        let learned = live["sector_updates"].as_array().unwrap().len();

        for _ in 0..(iac_shared::constants::SCAN_REVEAL_TICKS + 2) {
            engine.lock().unwrap().tick().unwrap();
        }
        net.send_full_state(&net.sessions.lock().unwrap().get(&1).cloned().unwrap(), pid).unwrap();
        let full = next_message(&mut rx);
        let known = full["known_sectors"].as_array().unwrap();
        assert_eq!(known.len(), learned.max(known.len()));
        let stale: Vec<_> = known.iter().filter(|s| s["live"] == false).collect();
        assert!(!stale.is_empty(), "scan-only sectors persist as stale");
        assert!(stale.iter().all(|s| s["last_seen"].as_u64().unwrap() < full["tick"].as_u64().unwrap()));
        assert!(known.len() >= learned, "nothing forgotten: {} < {learned}", known.len());

        net.broadcast_updates().unwrap();
        let tick = next_message(&mut rx);
        let updates = tick["sector_updates"].as_array().unwrap();
        assert!(updates.iter().all(|s| s["live"] == true), "settled stale sectors are not repeated");
    }

    #[test]
    fn storage_errors_name_the_vault_level_needed() {
        let (_net, engine, pid, _fid, _rx) = network_with_player();
        let mut e = engine.lock().unwrap();
        e.players.get_mut(&pid).unwrap().resources = iac_shared::Resources { metal: 100.0, crystal: 40.0, deuterium: 0.0 };

        e.players.get_mut(&pid).unwrap().buildings.research_lab = 4;
        let cmd = Command::Research { tech: ResearchType::CruiserTech, reserve: false };
        assert_eq!(
            command_error_message(&e, pid, &cmd, ErrorCode::StorageTooSmall),
            "Cruiser Tech Lv.1 costs 8000 metal, more than the 5000 metal your stockpile holds; needs Storage Vault level 2"
        );
    }

    #[test]
    fn preview_move_answers_with_fuel_numbers_and_fleets_carry_range() {
        let (net, engine, pid, fid, mut rx) = network_with_player();
        let (home, neighbour) = {
            let e = engine.lock().unwrap();
            let home = e.players[&pid].homeworld;
            (home, e.world_gen.connected_neighbors(home).slice()[0])
        };
        let session = net.sessions.lock().unwrap().get(&1).cloned().unwrap();
        net.route_command(&session, Command::PreviewMove { fleet_id: fid, target: neighbour }).unwrap();
        let msg = next_message(&mut rx);
        assert_eq!(msg["type"], "preview_move", "{msg}");
        assert_eq!(msg["fleet_id"], fid);
        assert_eq!(msg["can_jump"], true);
        assert_eq!(msg["hops_home"], 1);
        assert_eq!(msg["route_unexplored"], false);
        assert!(msg["fuel_cost"].as_f64().unwrap() > 0.0);
        let _ = home;

        net.broadcast_updates().unwrap();
        let tick = next_message(&mut rx);
        let fleet = &tick["fleets"][0];
        // 2 Scouts: 120 fuel, 6 per jump -> 10 round trips out and back.
        assert_eq!(fleet["range_hops"], 10, "{fleet}");
        assert!(fleet["power"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn the_homeworld_reports_defences_and_the_expected_raid() {
        let (net, engine, pid, _fid, mut rx) = network_with_player();
        {
            let mut e = engine.lock().unwrap();
            let p = e.players.get_mut(&pid).unwrap();
            p.buildings.defense_grid = 2;
            p.defences.count[0] = 5;
            p.defences.restoring[0] = 1;
        }
        net.broadcast_updates().unwrap();
        let tick = next_message(&mut rx);
        let hw = &tick["homeworld_update"];
        assert_eq!(hw["defences"][0]["kind"], "PulseTurret");
        assert_eq!(hw["defences"][0]["count"], 5);
        assert_eq!(hw["defences"][0]["restoring"], 1);
        assert!((hw["defences"][0]["power"].as_f64().unwrap() - 17.0 * 1.08).abs() < 1e-3);
        let defence = hw["home_defence_power"].as_f64().unwrap();
        let ships_and_grid = engine.lock().unwrap().home_defense_power(pid) as f64;
        assert!((defence - ships_and_grid).abs() < 1e-3);
        assert!(defence > 5.0 * 17.0 * 1.08, "structures count toward home defence: {defence}");
        assert!(hw["next_raid_estimate_power"].as_f64().unwrap() > 5.0);
        assert_eq!(hw["catalog"]["defences"][0]["count"], 5);
    }
}
