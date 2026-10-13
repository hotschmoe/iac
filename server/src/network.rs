// HTTP/WebSocket server (axum). WebSocket upgrades are accepted on any path
// (the TUI connects to `/`, the web client to `/ws`); plain GETs are served
// from the optional static web dir with SPA fallback.
//
// This is the transport and nothing above it: a session is a socket with a
// number. Who is logged in and what a message is answered with is the host
// core's (`iac_sim::host`); messages wait here until the tick hands them over.

use std::collections::HashMap;
use std::net::SocketAddr;
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

use iac_shared::protocol::{ClientMessage, ServerMessage};
use iac_sim::host::{Door, SessionId};
use iac_sim::Host;

type Sockets = Arc<Mutex<HashMap<SessionId, mpsc::UnboundedSender<Message>>>>;

pub struct Network {
    port: u16,
    sockets: Sockets,
    next_session_id: Arc<Mutex<SessionId>>,
    incoming_tx: mpsc::UnboundedSender<Incoming>,
    incoming_rx: Mutex<mpsc::UnboundedReceiver<Incoming>>,
}

#[derive(Debug)]
enum Incoming {
    Message(SessionId, ClientMessage),
    Closed(SessionId),
}

impl Network {
    pub fn init(port: u16) -> Network {
        let (tx, rx) = mpsc::unbounded_channel();
        Network {
            port,
            sockets: Arc::new(Mutex::new(HashMap::new())),
            next_session_id: Arc::new(Mutex::new(1)),
            incoming_tx: tx,
            incoming_rx: Mutex::new(rx),
        }
    }

    /// Bind and serve. Returns the address bound, which names the port the
    /// system chose when the server was started on port 0.
    pub async fn start_listening(
        &self,
        host: &str,
        web_dir: Option<PathBuf>,
    ) -> Result<SocketAddr, Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind((host, self.port)).await?;
        let addr = listener.local_addr()?;
        info!("HTTP/WebSocket server listening on {} (ws://{}/ws)", addr, addr);

        let state = HttpState {
            sockets: Arc::clone(&self.sockets),
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

        Ok(addr)
    }

    /// Hand `host` everything that arrived since the last tick, in the order
    /// it arrived, and send each session what it is answered. A message
    /// whose session has gone meanwhile is dropped. Returns whether an
    /// account was issued a token: from then on someone outside holds a
    /// password to this world.
    pub fn deliver<D: Door>(&self, host: &mut Host<D>) -> bool {
        let mut issued = false;
        let mut batch = Vec::new();
        {
            let mut rx = self.incoming_rx.lock().unwrap();
            while let Ok(incoming) = rx.try_recv() {
                batch.push(incoming);
            }
        }

        let mut replies = Vec::new();
        for incoming in batch {
            match incoming {
                Incoming::Closed(session) => {
                    info!("Client disconnected (session {}, player {:?})", session, host.player_of(session));
                    host.close(session);
                }
                Incoming::Message(session, msg) => {
                    if !self.sockets.lock().unwrap().contains_key(&session) {
                        warn!("Error handling message from session {}: Session not found", session);
                        continue;
                    }
                    host.receive(session, msg, &mut replies);
                    for reply in replies.drain(..) {
                        issued |= matches!(&reply, ServerMessage::AuthResult(result) if result.token.is_some());
                        self.send(session, &reply);
                    }
                }
            }
        }
        issued
    }

    pub fn send(&self, session: SessionId, msg: &ServerMessage) {
        let json = match serde_json::to_string(msg) {
            Ok(json) => json,
            Err(e) => return warn!("Failed to encode a message for session {}: {}", session, e),
        };
        let sockets = self.sockets.lock().unwrap();
        if let Some(sender) = sockets.get(&session)
            && let Err(e) = sender.send(Message::Text(json.into()))
        {
            warn!("Failed to send to session {}: {}", session, e);
        }
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
    session_id: SessionId,
    mut ws: SplitStream<WebSocket>,
    sockets: Sockets,
    incoming_tx: mpsc::UnboundedSender<Incoming>,
) {
    while let Some(msg) = ws.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let parsed: Result<ClientMessage, _> = serde_json::from_str(&text);
                match parsed {
                    Ok(client_msg) => {
                        if let Err(e) = incoming_tx.send(Incoming::Message(session_id, client_msg)) {
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

    sockets.lock().unwrap().remove(&session_id);
    let _ = incoming_tx.send(Incoming::Closed(session_id));
}

// ── HTTP transport ────────────────────────────────────────────────

#[derive(Clone)]
struct HttpState {
    sockets: Sockets,
    next_session_id: Arc<Mutex<SessionId>>,
    incoming_tx: mpsc::UnboundedSender<Incoming>,
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

    state.sockets.lock().unwrap().insert(session_id, tx);
    info!("Client connected (session {})", session_id);

    reader_task(session_id, ws_read, state.sockets, state.incoming_tx).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use iac_shared::constants::ShipClass;
    use iac_shared::hex::Hex;
    use iac_shared::pace::Pace;
    use iac_shared::protocol::{AuthRequest, Command, ErrorCode, GameEvent, HarvestResource};
    use iac_shared::scaling::{BuildingType, Refit, ResearchType};
    use iac_sim::commands::command_error_message;
    use iac_sim::engine::Ships;
    use iac_sim::host::Tokens;
    use iac_sim::views::{self, event_for};
    use iac_sim::GameEngine;
    use serde_json::Value;

    type Mint = fn() -> String;

    /// A world hosted as the server hosts it, and one player logged in over
    /// session 1 of a network whose socket is a channel.
    struct Table {
        net: Network,
        host: Host<Tokens<Mint>>,
        pid: u64,
        fid: u64,
        rx: mpsc::UnboundedReceiver<Message>,
    }

    fn table() -> Table {
        let mut host = Host::new(GameEngine::new(42, Pace::default()), Tokens(crate::auth::generate_token as Mint));
        let net = Network::init(0);
        let (tx, mut rx) = mpsc::unbounded_channel();
        net.sockets.lock().unwrap().insert(1, tx);
        let auth = AuthRequest { player_name: "Watcher".to_string(), token: None, agent: false };
        net.incoming_tx.send(Incoming::Message(1, ClientMessage::Auth(auth))).unwrap();
        net.deliver(&mut host);
        let pid = next_message(&mut rx)["player_id"].as_u64().expect("logged in");
        assert_eq!(next_message(&mut rx)["type"], "full_state");
        let fid = host.engine().fleets.values().find(|f| f.owner_id == pid).unwrap().id;
        Table { net, host, pid, fid, rx }
    }

    impl Table {
        fn engine(&mut self) -> &mut GameEngine {
            self.host.engine_mut()
        }

        /// One message from the session, through the queue, and its first answer.
        fn say(&mut self, msg: ClientMessage) -> Value {
            self.net.incoming_tx.send(Incoming::Message(1, msg)).unwrap();
            self.net.deliver(&mut self.host);
            next_message(&mut self.rx)
        }

        /// One tick of the host, and the update the session is sent for it.
        fn tick(&mut self) -> Value {
            let net = &self.net;
            self.host.step(&mut (), |session, frame| net.send(session, &frame)).unwrap();
            next_message(&mut self.rx)
        }

        /// The update the session would be sent for the world as it stands,
        /// where a test looks between two ticks.
        fn update(&mut self) -> Value {
            let events = self.host.engine_mut().drain_events();
            let update = views::tick_update(self.host.engine(), self.pid, &events);
            self.net.send(1, &ServerMessage::TickUpdate(update));
            next_message(&mut self.rx)
        }
    }

    fn next_message(rx: &mut mpsc::UnboundedReceiver<Message>) -> Value {
        match rx.try_recv().expect("a message was sent") {
            Message::Text(t) => serde_json::from_str(&t).unwrap(),
            other => panic!("unexpected frame {other:?}"),
        }
    }

    #[test]
    fn losing_every_fleet_is_signalled_with_an_empty_list() {
        let mut t = table();
        let fid = t.fid;

        let before = t.update();
        assert_eq!(before["fleets"].as_array().unwrap().len(), 1);
        assert_eq!(before["fleets"][0]["id"], fid);

        // Combat leaves a wiped-out fleet with no ships.
        t.engine().fleets.get_mut(&fid).unwrap().ships = Ships::default();
        let during = t.update();
        assert_eq!(during["fleets"], serde_json::json!([]), "dead fleet must not be listed: {during}");

        // Once reaped, the list stays an explicit empty array, never absent.
        let after = t.tick();
        assert!(!t.engine().fleets.contains_key(&fid));
        assert_eq!(after["fleets"], serde_json::json!([]));
    }

    #[test]
    fn fleet_list_is_ordered_by_id() {
        let mut t = table();
        let (pid, fid) = (t.pid, t.fid);
        {
            let e = t.engine();
            let mut clone = e.fleets[&fid].clone();
            for id in [fid + 900, fid + 100, fid + 500] {
                clone.id = id;
                e.fleets.insert(id, clone.clone());
            }
            assert_eq!(e.fleets.values().filter(|f| f.owner_id == pid).count(), 4);
        }
        let ids: Vec<u64> = t.update()["fleets"]
            .as_array().unwrap().iter().map(|f| f["id"].as_u64().unwrap()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn wrong_resource_harvest_gets_a_specific_error() {
        use iac_shared::constants::Density;
        let mut t = table();
        let fid = t.fid;
        let site = {
            let e = t.engine();
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

        let harvest = Command::Harvest { fleet_id: fid, resource: HarvestResource::Deuterium };
        let reply = t.say(ClientMessage::Command(harvest));
        assert_eq!(reply["type"], "error");
        assert_eq!(reply["code"], "ResourceNotPresent");
        let msg = reply["message"].as_str().unwrap();
        assert!(msg.contains("deuterium"), "message should name the resource: {msg} (sector {site})");
    }

    #[test]
    fn state_carries_cargo_capacity_and_real_production() {
        let mut t = table();
        let (pid, fid) = (t.pid, t.fid);
        {
            let e = t.engine();
            e.players.get_mut(&pid).unwrap().buildings.metal_mine = 3;
            e.players.get_mut(&pid).unwrap().buildings.deuterium_synthesizer = 2;
            // The starting fighters plus one cargo ship.
            e.fleets.get_mut(&fid).unwrap().ships.add(ShipClass::SmallCargo, 1);
        }
        let expected_cap: f64 = {
            let e = t.engine();
            e.fleets[&fid].ships.counts()
                .map(|(class, count)| f64::from(count) * f64::from(class.base_stats().cargo)).sum()
        };
        assert!(expected_cap > 5000.0, "the cargo ship must be counted: {expected_cap}");

        let before = t.engine().players[&pid].resources;
        let update = t.tick();
        let grown = t.engine().players[&pid].resources;

        assert_eq!(update["fleets"][0]["cargo_capacity"].as_f64().unwrap(), expected_cap);
        let prod = &update["homeworld_update"]["production"];
        assert!(prod["metal"].as_f64().unwrap() > 0.0);
        assert!((grown.metal - before.metal - prod["metal"].as_f64().unwrap()).abs() < 1e-3,
            "advertised production must equal the stockpile change");
        assert!((grown.crystal - before.crystal - prod["crystal"].as_f64().unwrap()).abs() < 1e-3);
        assert!((grown.deuterium - before.deuterium - prod["deuterium"].as_f64().unwrap()).abs() < 1e-3);
    }

    #[test]
    fn homeworld_catalog_tracks_levels() {
        let mut t = table();
        let pid = t.pid;
        let update = t.update();
        let mine = &update["homeworld_update"]["catalog"]["buildings"][0];
        assert_eq!(mine["building_type"], "MetalMine");
        assert_eq!(mine["level"], 1);
        assert_eq!(mine["next"]["level"], 2);
        assert_eq!(mine["effects"][1], serde_json::json!({"kind": "energy_used", "now": 11.0, "next": 25.0}));
        let yard = &update["homeworld_update"]["catalog"]["buildings"][7];
        assert_eq!((&yard["building_type"], &yard["requires"][0]["name"]), (&"Shipyard".into(), &"Robotics Factory".into()));
        assert_eq!(yard["requires"][0]["met"], false);
        let energy = &update["homeworld_update"]["energy"];
        assert_eq!((&energy["made"], &energy["needed"], &energy["fraction"]), (&22.into(), &22.into(), &1.0.into()));

        let buildings = &mut t.engine().players.get_mut(&pid).unwrap().buildings;
        (buildings.metal_mine, buildings.robotics_factory) = (2, 2);
        let update = t.update();
        let catalog = &update["homeworld_update"]["catalog"];
        assert_eq!(catalog["buildings"][0]["level"], 2);
        assert_eq!(catalog["buildings"][7]["requires"][0]["met"], true);
        let energy = &update["homeworld_update"]["energy"];
        assert_eq!((&energy["made"], &energy["needed"]), (&22.into(), &36.into()), "a mine raised past its power");
        assert!(energy["fraction"].as_f64().unwrap() < 1.0);
    }

    #[test]
    fn prerequisite_errors_name_what_is_missing() {
        let mut t = table();
        let pid = t.pid;
        t.engine().players.get_mut(&pid).unwrap().buildings.shipyard = 2;
        let e = t.host.engine();

        let cmd = Command::BuildShip { ship_class: ShipClass::Cruiser, count: 1 };
        assert_eq!(
            command_error_message(e, pid, &cmd, ErrorCode::ShipLocked),
            "Cruiser needs Shipyard level 5 (you have 2), Impulse Drive level 4 (you have 0) and Ion Technology level 2 (you have 0)"
        );
        let cmd = Command::Build { building_type: BuildingType::FusionReactor, reserve: false };
        assert_eq!(
            command_error_message(e, pid, &cmd, ErrorCode::PrerequisitesNotMet),
            "Fusion Reactor needs Deuterium Synthesizer level 5 (you have 0) and Energy Technology level 3 (you have 0)"
        );
        let cmd = Command::Research { tech: ResearchType::CombustionDrive, reserve: false };
        assert_eq!(
            command_error_message(e, pid, &cmd, ErrorCode::PrerequisitesNotMet),
            "Combustion Drive needs Research Lab level 1 (you have 0) and Energy Technology level 1 (you have 0)"
        );
        let cmd = Command::Research { tech: ResearchType::GravitonTechnology, reserve: false };
        assert_eq!(
            command_error_message(e, pid, &cmd, ErrorCode::PrerequisitesNotMet),
            "Graviton Technology needs Research Lab level 12 (you have 0) and 300000 energy made (you make 22)"
        );
        let cmd = Command::BuildDefence { kind: iac_shared::scaling::DefenceKind::LightLaser, count: 1 };
        assert_eq!(
            command_error_message(e, pid, &cmd, ErrorCode::DefenceLocked),
            "Light Laser needs Energy Technology level 1 (you have 0) and Laser Technology level 3 (you have 0)"
        );
    }

    #[test]
    fn cancel_refusals_say_where_the_order_actually_is() {
        use iac_shared::protocol::QueueType;
        let mut t = table();
        let pid = t.pid;
        let e = t.engine();
        e.players.get_mut(&pid).unwrap().resources = iac_shared::constants::Resources { metal: 9e3, crystal: 9e3, deuterium: 9e3 };
        e.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        e.handle_build(pid, BuildingType::CrystalMine, false).unwrap();
        let running = e.players[&pid].building_queue[0].id;
        let waiting = e.players[&pid].building_pending[0].id;
        let e = t.host.engine();

        let msg = |cmd: Command| command_error_message(e, pid, &cmd, ErrorCode::InvalidTarget);
        let cmd = Command::CancelQueued { queue_type: QueueType::Ship, id: waiting };
        assert_eq!(msg(cmd), format!("order {waiting} is in the building queue, not the shipyard queue; nothing was cancelled"));
        let cmd = Command::CancelQueued { queue_type: QueueType::Building, id: running };
        assert_eq!(msg(cmd), format!("order {running} is not waiting in the building queue; use cancel_build for it"));
        let cmd = Command::CancelBuild { queue_type: QueueType::Building, id: waiting };
        assert_eq!(msg(cmd), format!("order {waiting} is not under way in the building queue; use cancel_queued for it"));
        let cmd = Command::CancelQueued { queue_type: QueueType::Building, id: 999_999 };
        assert!(msg(cmd).starts_with("no order 999999 in any queue"));
    }

    fn fight_events(engine: &mut GameEngine, pid: u64, fid: u64, at: Hex) -> Vec<GameEvent> {
        let fleet = engine.fleets.get_mut(&fid).unwrap();
        fleet.location = at;
        fleet.refit = Refit { weapons: 250, shielding: 250, armour: 250 };
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
            K::BattleRound(_) | K::CombatStarted(_) | K::CombatEnded(_) | K::FleetDestroyed(_)
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
        assert!(events.iter().any(|e| matches!(e.kind, K::BattleRound(_))), "a fight happened");
        engine.fleets.get_mut(&fa).unwrap().location = engine.players[&a].homeworld;

        let seen = |p: u64| -> Vec<GameEvent> {
            let player = &engine.players[&p];
            events.iter().filter_map(|e| event_for(e, player, &engine)).collect()
        };

        let alpha = seen(a);
        assert!(alpha.iter().any(is_combat_detail));
        for e in alpha.iter().filter(|e| is_combat_detail(e)) {
            match &e.kind {
                K::BattleRound(r) => {
                    let whose: Vec<(Option<&str>, bool)> = r.fleets.groups.iter().map(|g| (g.owner.as_deref(), g.mine)).collect();
                    assert!(r.mine && whose == [(Some("Alpha"), true), (Some("Charlie"), false)], "{whose:?}");
                    assert!(r.hostiles.groups.iter().all(|g| !g.mine && g.owner.is_none() && !g.label.is_empty()));
                }
                K::CombatStarted(s) => assert!(s.mine && s.owner == "Alpha"),
                K::CombatEnded(s) => {
                    assert!(s.mine);
                    let whose: Vec<(Option<&str>, bool, bool)> =
                        s.losses.iter().map(|l| (l.owner.as_deref(), l.mine, l.destroyed)).collect();
                    assert_eq!(whose, [(Some("Alpha"), true, false), (Some("Charlie"), false, false), (None, false, true)]);
                }
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
            K::BattleRound(r) => !r.mine && r.fleets.groups.iter().all(|g| !g.mine),
            K::CombatStarted(s) => !s.mine,
            K::CombatEnded(s) => !s.mine && s.losses.iter().all(|l| !l.mine),
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
        // Two Espionage Probes, which have no weapon, each a hit from breaking up.
        let fleet = engine.fleets.get_mut(&fa).unwrap();
        fleet.location = at;
        fleet.ships = Ships::default();
        for _ in 0..2 {
            fleet.ships.add_holding(ShipClass::EspionageProbe, 1, 100);
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
        let mut t = table();
        let (pid, fid) = (t.pid, t.fid);
        t.engine().handle_scan(pid, fid).unwrap();
        let live = t.tick();
        assert!(live["sector_updates"].as_array().unwrap().iter().all(|s| s["live"] == true));
        let learned = live["sector_updates"].as_array().unwrap().len();

        for _ in 0..(iac_shared::constants::SCAN_REVEAL_TICKS + 2) {
            t.engine().tick().unwrap();
        }
        let full = t.say(ClientMessage::RequestFullState);
        let known = full["known_sectors"].as_array().unwrap();
        assert_eq!(known.len(), learned.max(known.len()));
        let stale: Vec<_> = known.iter().filter(|s| s["live"] == false).collect();
        assert!(!stale.is_empty(), "scan-only sectors persist as stale");
        assert!(stale.iter().all(|s| s["last_seen"].as_u64().unwrap() < full["tick"].as_u64().unwrap()));
        assert!(known.len() >= learned, "nothing forgotten: {} < {learned}", known.len());

        let tick = t.update();
        let updates = tick["sector_updates"].as_array().unwrap();
        assert!(updates.iter().all(|s| s["live"] == true), "settled stale sectors are not repeated");
    }

    #[test]
    fn storage_errors_name_the_storage_level_needed() {
        let mut t = table();
        let pid = t.pid;
        let e = t.engine();
        e.players.get_mut(&pid).unwrap().resources = iac_shared::Resources { metal: 100.0, crystal: 40.0, deuterium: 0.0 };

        e.players.get_mut(&pid).unwrap().buildings.research_lab = 4;
        e.players.get_mut(&pid).unwrap().research.weapons_technology = 8;
        let cmd = Command::Research { tech: ResearchType::WeaponsTechnology, reserve: false };
        assert_eq!(
            command_error_message(e, pid, &cmd, ErrorCode::StorageTooSmall),
            "Weapons Technology Lv.9 costs 204800 metal, more than the 100000 metal your stockpile holds; needs Metal Storage level 3"
        );
        let cmd = Command::BuildShip { ship_class: ShipClass::Deathstar, count: 1 };
        assert_eq!(
            command_error_message(e, pid, &cmd, ErrorCode::StorageTooSmall),
            "Deathstar x1 costs 5000000 metal, more than the 100000 metal your stockpile holds; \
             needs Metal Storage level 10, Crystal Storage level 10 and Deuterium Tank level 7"
        );
    }

    #[test]
    fn preview_move_answers_with_fuel_numbers_and_fleets_carry_range() {
        let mut t = table();
        let (pid, fid) = (t.pid, t.fid);
        let (home, neighbour) = {
            let e = t.host.engine();
            let home = e.players[&pid].homeworld;
            (home, e.world_gen.connected_neighbors(home).slice()[0])
        };
        let msg = t.say(ClientMessage::Command(Command::PreviewMove { fleet_id: fid, target: neighbour }));
        assert_eq!(msg["type"], "preview_move", "{msg}");
        assert_eq!(msg["fleet_id"], fid);
        assert_eq!(msg["can_jump"], true);
        assert_eq!(msg["hops_home"], 1);
        assert_eq!(msg["route_unexplored"], false);
        assert!(msg["fuel_cost"].as_f64().unwrap() > 0.0);
        let _ = home;

        let tick = t.update();
        let fleet = &tick["fleets"][0];
        // 2 Light Fighters: 400 fuel, 40 a jump -> 5 hops out and back.
        assert_eq!(fleet["range_hops"], 5, "{fleet}");
        assert!(fleet["power"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn the_homeworld_reports_defences_and_the_expected_raid() {
        let mut t = table();
        let pid = t.pid;
        {
            let p = t.engine().players.get_mut(&pid).unwrap();
            p.research.weapons_technology = 1;
            p.defences.count[0] = 5;
            p.satellites = 3;
        }
        let tick = t.update();
        let hw = &tick["homeworld_update"];
        assert_eq!(hw["defences"][0]["kind"], "RocketLauncher");
        assert_eq!(hw["defences"][0]["count"], 5);
        let launcher = 80.0 * 1.1 + (200.0 + 20.0) / 10.0;
        assert!((hw["defences"][0]["power"].as_f64().unwrap() - launcher).abs() < 1e-3, "Weapons Technology reaches a defence");
        let defence = hw["home_defence_power"].as_f64().unwrap();
        let in_all = t.host.engine().home_defense_power(pid) as f64;
        assert!((defence - in_all).abs() < 1e-3);
        let satellites = hw["satellite_power"].as_f64().unwrap();
        assert!(
            (satellites - 3.0 * (1.0 + 20.1)).abs() < 1e-3,
            "three Solar Satellites stand in the defence, and a weapon of 1 is still 1 a level up, cut off as the round cuts it: {satellites}"
        );
        assert!(defence > 5.0 * launcher + satellites, "defences and satellites count toward home defence: {defence}");
        assert!(hw["next_raid_estimate_power"].as_f64().unwrap() > 50.0);
        let raid = &hw["raid"];
        assert_eq!(raid["power"], hw["next_raid_estimate_power"], "with none announced the forecast is the typical raid");
        assert!(raid["arrival_tick"].is_null());
        assert_eq!(raid["ships"][0]["class"], "LightFighter", "a new empire's raid is Light Fighters: {raid}");
        assert_eq!(
            raid["odds"]["label"], "repelled",
            "two Light Fighters, five Rocket Launchers and three satellites destroy it every time: {raid}"
        );
        assert_eq!(hw["catalog"]["defences"][0]["count"], 5);
        assert_eq!((&hw["satellites"], &hw["energy"]["made"]), (&3.into(), &(22 + 3 * 30).into()));
    }
}
