// The two hosts of `iac-sim` play one world alike: the real server, over a
// WebSocket and SQLite, and the single-player host, through `Machine::call`
// and its save document. One stored world is resumed by both, one commander
// gives both the same orders on the same ticks, and they must send that
// commander the same frames, one for one, and end with the same world.
//
// How one world reaches both. A world is made by the server (built-in bots
// seated at tick 0, then an account registered over the wire) and stopped.
// What its database holds is read as a `Snapshot`, and that snapshot is put
// before the single-player host as a save document. The server is then
// started again on the database. From there both resume the same bytes.
//
// How the orders land on known ticks although the server ticks by the wall
// clock. Nothing is assumed about when the server takes a message: it is
// read off its answers. Orders go out in batches, one right after a tick's
// update, each between two `request_full_state`. The server answers a
// session's messages when it takes them in, before that tick's update, so
// the update that follows the two full states names the tick the batch
// landed in, and the single-player host is given the batch before the same
// tick. A batch the server took in two ticks (its two full states on either
// side of an update: the machine stalled about a second in the middle of
// three small frames) cannot be placed, and fails the test saying so.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use common::*;
use futures_util::SinkExt;
use iac_server::database::Database;
use iac_shared::constants::ShipClass;
use iac_shared::hex::Hex;
use iac_shared::protocol::{ClientMessage, Command, HarvestResource, QueueType};
use iac_shared::scaling::{BuildingType, ResearchType};
use iac_shared::world::WorldGen;
use iac_sim::Snapshot;
use iac_solo::Machine;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use xxhash_rust::xxh3::xxh3_64;

const SEED: u64 = 20_261_013;
const NAME: &str = "Admiral";
const WORLD: [&str; 12] = [
    "--seed", "20261013", "--pace", "test", "--bot", "all:hands-on", "--bot", "explorer:check-in", "--bot",
    "turtle:hybrid", "--bot-sessions", "24",
];
/// Ticks the bots play before the commander's account is stored with them.
const SETTLE_TICKS: u64 = 4;
/// Ticks the commander plays on both hosts.
const PLAY_TICKS: u64 = 40;
const T0: u64 = 1_800_000_000_000;
const ENTROPY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn stored_world(db: &Path) -> Snapshot {
    let database = Database::init(db.to_str().unwrap()).expect("the server's database opens");
    database.load_snapshot(SEED, None).expect("the server's database holds a world")
}

fn call(machine: &mut Machine, now_ms: u64, mut head: Value, body: &[u8]) -> (Value, Vec<u8>) {
    head["now_ms"] = json!(now_ms);
    let mut request = serde_json::to_vec(&head).unwrap();
    if !body.is_empty() {
        request.push(b'\n');
        request.extend(body);
    }
    let reply = machine.call(&request);
    let split = reply.iter().position(|&b| b == b'\n').expect("a reply line");
    let head: Value = serde_json::from_slice(&reply[..split]).expect("reply JSON");
    assert_eq!(head["error"], Value::Null, "the single-player host failed: {head}");
    (head, reply[split + 1..].to_vec())
}

fn frames(body: &[u8]) -> Vec<Value> {
    body.split(|&b| b == b'\n').filter(|line| !line.is_empty()).map(|line| serde_json::from_slice(line).unwrap()).collect()
}

/// The single-player host's save document for `world`, played by
/// `commander` and standing at `T0`. Its header is the one the host itself
/// writes for a new game, with what names the world replaced.
fn save_document(world: &Snapshot, commander: u64) -> Vec<u8> {
    let mut scratch = Machine::default();
    let new_game = json!({"op": "new_game", "name": NAME, "pace": "test", "bots": [], "entropy": ENTROPY});
    call(&mut scratch, T0, new_game, &[]);
    let written = call(&mut scratch, T0, json!({"op": "save"}), &[]).1;
    let split = written.iter().position(|&b| b == b'\n').expect("a header line");
    let mut header: Value = serde_json::from_slice(&written[..split]).expect("a header of JSON");

    let body = serde_json::to_vec(world).unwrap();
    for (field, value) in [
        ("digest", json!(xxh3_64(&body))),
        ("commander", json!(commander)),
        ("clock", json!({"anchor_ms": T0, "anchor_tick": world.tick})),
        ("saved_at_ms", json!(T0)),
    ] {
        assert!(header.get(field).is_some(), "the save header has no `{field}` any more: {header}");
        header[field] = value;
    }
    let mut document = serde_json::to_vec(&header).unwrap();
    document.push(b'\n');
    document.extend(body);
    document
}

/// The single-player host and the wall clock it is told.
struct Solo {
    machine: Machine,
    /// The tick the world stood at when it was loaded, at `T0`.
    loaded_at: u64,
}

impl Solo {
    fn load(world: &Snapshot, commander: u64) -> Solo {
        let mut machine = Machine::default();
        let (reply, _) = call(&mut machine, T0, json!({"op": "load"}), &save_document(world, commander));
        assert_eq!(reply["refused"], Value::Null, "the single-player host refused the server's world: {reply}");
        assert_eq!(reply["world"]["tick"], world.tick, "{reply}");
        assert_eq!(reply["world"]["commander"], NAME, "{reply}");
        Solo { machine, loaded_at: world.tick }
    }

    /// One request, made when the wall clock says the world is at `tick`.
    fn call(&mut self, tick: u64, head: Value, body: &[u8]) -> (Value, Vec<u8>) {
        call(&mut self.machine, T0 + (tick - self.loaded_at) * 1000, head, body)
    }

    fn tick(&mut self) -> u64 {
        self.call(self.loaded_at, json!({"op": "describe"}), &[]).0["world"]["tick"].as_u64().expect("a world")
    }

    /// Play the one tick that leads to `tick`.
    fn advance_to(&mut self, tick: u64) -> Vec<Value> {
        let (reply, body) = self.call(tick, json!({"op": "advance", "max_ticks": 1}), &[]);
        assert_eq!(reply["world"]["tick"], tick, "the single-player host is not where its clock says: {reply}");
        assert_eq!(reply["world"]["catch_up"], Value::Null, "one tick a call is never an absence: {reply}");
        frames(&body)
    }

    fn send(&mut self, at_tick: u64, message: &Value) -> Vec<Value> {
        frames(&self.call(at_tick, json!({"op": "frame"}), message.to_string().as_bytes()).1)
    }

    fn world(&mut self, at_tick: u64) -> Snapshot {
        let document = self.call(at_tick, json!({"op": "save"}), &[]).1;
        let split = document.iter().position(|&b| b == b'\n').expect("a header line");
        serde_json::from_slice(&document[split + 1..]).expect("a save document holds a snapshot")
    }
}

fn first_difference(at: &str, server: &Value, solo: &Value) -> Option<String> {
    match (server, solo) {
        (Value::Object(a), Value::Object(b)) if a.keys().eq(b.keys()) => {
            a.iter().zip(b.values()).find_map(|((key, left), right)| first_difference(&format!("{at}/{key}"), left, right))
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            a.iter().zip(b).enumerate().find_map(|(i, (left, right))| first_difference(&format!("{at}/{i}"), left, right))
        }
        _ if server == solo => None,
        _ => Some(format!("{at}: the server has {server}, the single-player host {solo}")),
    }
}

fn command(command: Command) -> Value {
    serde_json::to_value(ClientMessage::Command(command)).unwrap()
}

/// What the commander orders, by ticks after logging in: things that are
/// accepted, things that are refused, questions about the whole world.
fn orders(fleet_id: u64, home: Hex) -> Vec<(u64, Vec<Value>)> {
    let next = WorldGen::init(SEED).connected_neighbors(home).slice()[0];
    let build = |building_type| command(Command::Build { building_type, reserve: false });
    let board = || command(Command::Leaderboard { limit: 50 });
    let standing = |preset: &str| {
        json!({
            "type": "policy_update", "fleet_id": fleet_id, "preset": preset,
            "params": {"max_range": 6, "engage_ratio_x10": 25, "fuel_budget_pct": 35},
        })
    };
    vec![
        (1, vec![build(BuildingType::MetalMine), build(BuildingType::CrystalMine), command(Command::Scan { fleet_id }), board()]),
        (3, vec![
            command(Command::BuildShip { ship_class: ShipClass::Scout, count: 2 }),
            command(Command::PreviewMove { fleet_id, target: next }),
            command(Command::Odds { fleet_id, sectors: vec![next, home] }),
        ]),
        (5, vec![command(Command::Move { fleet_id, target: next })]),
        (9, vec![command(Command::Harvest { fleet_id, resource: HarvestResource::Auto }), board()]),
        (13, vec![
            build(BuildingType::Shipyard),
            command(Command::Research { tech: ResearchType::CorvetteTech, reserve: false }),
            command(Command::CancelQueued { queue_type: QueueType::Building, id: 999_999 }),
        ]),
        (18, vec![standing("prospect"), board()]),
        (26, vec![build(BuildingType::DeuteriumSynthesizer), command(Command::Scan { fleet_id: 999_999 })]),
        (32, vec![standing("manual"), command(Command::Recall { fleet_id }), board()]),
        (38, vec![board()]),
    ]
}

/// A batch on its way: what it holds, and the tick its first full state
/// said it landed in.
struct Batch {
    messages: Vec<Value>,
    landed: Option<u64>,
}

/// What a session lived through on the server.
struct Played {
    /// The tick the session logged in at.
    start: u64,
    /// The last tick it was sent.
    end: u64,
    /// How many batches its script held.
    batches: usize,
    /// Every frame it was sent, in order, from the answer to its login.
    got: Vec<Value>,
    /// What it sent, under the tick the server took it in before.
    sent: BTreeMap<u64, Vec<Value>>,
}

async fn play_on_the_server(port: u16, token: &str) -> Played {
    let (mut ws, auth) = login(port, NAME, Some(token)).await;
    assert_eq!(auth["success"], true, "{auth}");
    let state = next_json(&mut ws).await;
    assert_eq!(state["type"], "full_state", "{state}");
    let start = state["tick"].as_u64().unwrap();
    let fleet_id = state["fleets"][0]["id"].as_u64().expect("a starting fleet");
    let home = &state["player"]["homeworld"];
    let home = Hex { q: home["q"].as_i64().unwrap() as i16, r: home["r"].as_i64().unwrap() as i16 };

    let script = orders(fleet_id, home);
    let mut played = Played { start, end: start, batches: script.len(), got: vec![auth, state], sent: BTreeMap::new() };
    let mut script = script.into_iter().peekable();
    let mut on_its_way: Option<Batch> = None;
    loop {
        let frame = next_json(&mut ws).await;
        match frame["type"].as_str() {
            Some("tick_update") => {
                played.end = frame["tick"].as_u64().unwrap();
                played.got.push(frame);
                if on_its_way.is_some() {
                    continue;
                }
                if script.peek().is_none() && played.end >= start + PLAY_TICKS {
                    return played;
                }
                if let Some((_, orders)) = script.next_if(|(after, _)| start + after <= played.end) {
                    let marker = json!({"type": "request_full_state"});
                    let messages: Vec<Value> =
                        std::iter::once(marker.clone()).chain(orders).chain(std::iter::once(marker)).collect();
                    for message in &messages {
                        ws.feed(Message::text(message.to_string())).await.unwrap();
                    }
                    ws.flush().await.unwrap();
                    on_its_way = Some(Batch { messages, landed: None });
                }
            }
            Some("full_state") => {
                let landing = played.end + 1;
                played.got.push(frame);
                let batch = on_its_way.as_mut().expect("a full state nobody asked for");
                match batch.landed.replace(landing) {
                    None => {}
                    Some(first) => {
                        assert_eq!(
                            first, landing,
                            "the server took the batch sent after tick {} in two ticks ({first} and {landing}): \
                             it stalled about a second in the middle of it, and where each order landed \
                             cannot be told. This run proves nothing; run the test again",
                            first - 1,
                        );
                        played.sent.insert(landing, on_its_way.take().expect("the batch").messages);
                    }
                }
            }
            _ => played.got.push(frame),
        }
    }
}

/// The same session on the single-player host: logged in at the same tick,
/// each batch given before the tick the server took it in.
fn play_solo(solo: &mut Solo, played: &Played) -> Vec<Value> {
    for tick in solo.loaded_at + 1..=played.start {
        assert!(solo.advance_to(tick).is_empty(), "a session that is not logged in was sent a frame");
    }
    solo.call(played.start, json!({"op": "open"}), &[]);
    let mut got = solo.send(played.start, &json!({"type": "auth", "player_name": NAME}));
    for tick in played.start + 1..=played.end {
        for message in played.sent.get(&tick).into_iter().flatten() {
            got.extend(solo.send(tick - 1, message));
        }
        got.extend(solo.advance_to(tick));
    }
    got
}

fn tick_of(frames: &[Value], upto: usize) -> u64 {
    frames[..=upto].iter().rev().find_map(|frame| frame["tick"].as_u64()).unwrap_or(0)
}

#[tokio::test]
async fn the_server_and_the_single_player_host_play_one_world_alike() {
    let db = fresh_db("hosts_agree");

    let (token, commander) = {
        let (server, port) = start_server_with(&db, &WORLD);
        let (mut ws, auth) = login(port, NAME, None).await;
        let token = auth["token"].as_str().expect("a new account is given its token").to_string();
        let mut seen = 0;
        while seen < SETTLE_TICKS {
            seen += u64::from(next_json(&mut ws).await["type"] == "tick_update");
        }
        server.stop();
        (token, auth["player_id"].as_u64().unwrap())
    };

    let stored = stored_world(&db);
    assert!(stored.tick >= SETTLE_TICKS && stored.players.contains_key(&commander), "tick {}", stored.tick);
    assert_eq!(stored.bots.len(), 7, "five hands-on bots, a check-in explorer and a hybrid turtle");
    let mut solo = Solo::load(&stored, commander);

    let (server, port) = start_server_with(&db, &WORLD);
    let played = play_on_the_server(port, &token).await;
    let on_solo = play_solo(&mut solo, &played);

    let batches = played.batches;
    assert_eq!(played.sent.len(), batches, "every batch landed in a tick of its own");
    for (at, (server, solo)) in played.got.iter().zip(&on_solo).enumerate() {
        if let Some(difference) = first_difference("", server, solo) {
            panic!(
                "the hosts part at frame {at}, a {} around tick {} (the session logged in at tick {}): {difference}",
                server["type"],
                tick_of(&played.got, at),
                played.start,
            );
        }
    }
    assert_eq!(played.got.len(), on_solo.len(), "one host sent frames the other did not");

    let kinds = |kind: &str| played.got.iter().filter(|frame| frame["type"] == kind).count();
    assert!(kinds("tick_update") as u64 >= PLAY_TICKS && kinds("full_state") == 1 + 2 * batches);
    assert!(kinds("error") >= 3, "the script has orders that are refused: {} were", kinds("error"));
    assert!(kinds("preview_move") == 1 && kinds("odds") == 1 && kinds("leaderboard") == 5);
    let boards: Vec<&Value> = played.got.iter().filter(|frame| frame["type"] == "leaderboard").collect();
    let scores = |board: &Value| -> f64 { board["bots"].as_array().unwrap().iter().map(|row| row["score"].as_f64().unwrap()).sum() };
    assert_eq!(boards[0]["bots"].as_array().unwrap().len(), 7);
    assert!(scores(boards[4]) > scores(boards[0]) + 20.0, "the bots played on: {} to {}", scores(boards[0]), scores(boards[4]));
    let events = played.got.iter().filter(|frame| frame["type"] == "tick_update" && frame["events"].is_array()).count();
    assert!(events >= 5, "the commander's orders had consequences: {events} updates carried events");

    server.stop();
    let ended = stored_world(&db);
    assert!(ended.tick >= played.end);
    for tick in solo.tick() + 1..=ended.tick {
        solo.advance_to(tick);
    }
    let on_solo = solo.world(ended.tick);
    let (a, b) = (serde_json::to_value(&ended).unwrap(), serde_json::to_value(&on_solo).unwrap());
    if let Some(difference) = first_difference("world", &a, &b) {
        panic!("the worlds part by tick {}: {difference}", ended.tick);
    }
    assert_eq!(ended.hash(), on_solo.hash());
    remove_db(&db);
}
