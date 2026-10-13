// The two hosts of `iac-sim` play one world alike: the real server, over a
// WebSocket and its world file, and the single-player host, through
// `Machine::call`. One stored world is resumed by both, one commander gives
// both the same orders on the same ticks, and they must send that commander
// the same frames, one for one, and end with the same world.
//
// How one world reaches both. A world is made by the server (built-in bots
// seated at tick 0, then an account registered over the wire) and stopped.
// The file it leaves is the document both hosts store: the single-player
// host loads those bytes, its world and its checksum as the server wrote
// them, once the header's own part names a commander and a clock, which is
// all a server's file lacks. The server is then started again on the file.
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

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::Path;

use common::*;
use futures_util::SinkExt;
use iac_shared::constants::ShipClass;
use iac_shared::hex::Hex;
use iac_shared::protocol::{ClientMessage, Command, HarvestResource, QueueType};
use iac_shared::scaling::{BuildingType, ResearchType};
use iac_shared::world::{NpcBehaviorType, WorldGen};
use iac_solo::Machine;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

const SEED: u64 = 20_261_013;
const NAME: &str = "Admiral";
const WORLD: [&str; 12] = [
    "--seed", "20261013", "--pace", "test", "--bot", "all:hands-on", "--bot", "explorer:check-in", "--bot",
    "turtle:hybrid", "--bot-sessions", "24",
];
/// Ticks the bots play before the commander's account is stored with them.
const SETTLE_TICKS: u64 = 4;
/// Ticks the commander plays on both hosts before it flies into a fight.
const PLAY_TICKS: u64 = 40;
/// Ticks between two hops of that flight: a jump of Light Fighters and the
/// wait after it.
const HOP_TICKS: u64 = 5;
/// Ticks the commander stays for the fight it flew into.
const FIGHT_TICKS: u64 = 8;
const T0: u64 = 1_800_000_000_000;
const ENTROPY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

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

fn header_of(document: &[u8]) -> (Value, &[u8]) {
    let split = document.iter().position(|&b| b == b'\n').expect("a header line");
    (serde_json::from_slice(&document[..split]).expect("a header of JSON"), &document[split..])
}

/// The server's file as the single-player host's save: the same world and
/// checksum, byte for byte, under the server's header, to which is added
/// what the single-player host keeps beside a world. That part is the one
/// the host itself writes for a new game, naming `commander` and a clock
/// that stands at `T0` at the world's tick.
fn as_a_save(file: &Path, commander: u64, tick: u64) -> Vec<u8> {
    let mut scratch = Machine::default();
    let new_game = json!({"op": "new_game", "name": NAME, "pace": "test", "bots": [], "entropy": ENTROPY});
    call(&mut scratch, T0, new_game, &[]);
    let (mut own, _) = header_of(&call(&mut scratch, T0, json!({"op": "save"}), &[]).1);
    let host = &mut own["host"];
    for (field, value) in [
        ("commander", json!(commander)),
        ("clock", json!({"anchor_ms": T0, "anchor_tick": tick})),
        ("saved_at_ms", json!(T0)),
    ] {
        assert!(host.get(field).is_some(), "the single-player host keeps no `{field}` beside its world any more: {host}");
        host[field] = value;
    }

    let written = std::fs::read(file).expect("the server's world file");
    let (mut header, world) = header_of(&written);
    assert_eq!(header["host"], json!({}), "the server keeps nothing beside its world: {header}");
    header["host"] = host.take();
    [header.to_string().as_bytes(), world].concat()
}

/// The single-player host and the wall clock it is told.
struct Solo {
    machine: Machine,
    /// The tick the world stood at when it was loaded, at `T0`.
    loaded_at: u64,
}

impl Solo {
    fn load(file: &Path, commander: u64, tick: u64) -> Solo {
        let mut machine = Machine::default();
        let (reply, _) = call(&mut machine, T0, json!({"op": "load"}), &as_a_save(file, commander, tick));
        assert_eq!(reply["refused"], Value::Null, "the single-player host refused the server's world: {reply}");
        assert_eq!(reply["world"]["tick"], tick, "{reply}");
        assert_eq!(reply["world"]["commander"], NAME, "{reply}");
        Solo { machine, loaded_at: tick }
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

    /// The world's part of the host's save: the snapshot's bytes.
    fn world(&mut self, at_tick: u64) -> Vec<u8> {
        let document = self.call(at_tick, json!({"op": "save"}), &[]).1;
        header_of(&document).1[1..].to_vec()
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

/// The lanes from `home` to the nearest sector whose group shoots first, the
/// sector itself last.
fn into_a_fight(world: &WorldGen, home: Hex) -> Vec<Hex> {
    let mut parent = HashMap::from([(home, home)]);
    let mut frontier = VecDeque::from([home]);
    while let Some(at) = frontier.pop_front() {
        let shoots = world.generate_sector(at).npc_template.is_some_and(|npc| npc.behavior != NpcBehaviorType::Passive);
        if at != home && shoots {
            let mut route = vec![at];
            while parent[route.last().unwrap()] != home {
                route.push(parent[route.last().unwrap()]);
            }
            route.reverse();
            return route;
        }
        for &next in world.connected_neighbors(at).slice() {
            if let Entry::Vacant(unseen) = parent.entry(next) {
                unseen.insert(at);
                frontier.push_back(next);
            }
        }
    }
    panic!("no group that shoots first by any lane from {home}");
}

/// What the commander orders, by ticks after logging in: things that are
/// accepted, things that are refused, questions about the whole world, and
/// at the end a flight into a group that shoots first.
fn orders(fleet_id: u64, home: Hex) -> Vec<(u64, Vec<Value>)> {
    let world = WorldGen::init(SEED);
    let next = world.connected_neighbors(home).slice()[0];
    let build = |building_type| command(Command::Build { building_type, reserve: false });
    let board = || command(Command::Leaderboard { limit: 50 });
    let standing = |preset: &str| {
        json!({
            "type": "policy_update", "fleet_id": fleet_id, "preset": preset,
            "params": {"max_range": 6, "engage_ratio_x10": 25, "fuel_budget_pct": 35},
        })
    };
    let mut script = vec![
        (1, vec![build(BuildingType::MetalMine), build(BuildingType::CrystalMine), command(Command::Scan { fleet_id }), board()]),
        (3, vec![
            command(Command::BuildShip { ship_class: ShipClass::LightFighter, count: 2 }),
            command(Command::PreviewMove { fleet_id, target: next }),
            command(Command::Odds { fleet_id, sectors: vec![next, home] }),
        ]),
        (5, vec![command(Command::Move { fleet_id, target: next })]),
        (9, vec![command(Command::Harvest { fleet_id, resource: HarvestResource::Auto }), board()]),
        (13, vec![
            build(BuildingType::SolarPlant),
            build(BuildingType::Shipyard),
            command(Command::Research { tech: ResearchType::EnergyTechnology, reserve: false }),
            command(Command::CancelQueued { queue_type: QueueType::Building, id: 999_999 }),
        ]),
        (18, vec![standing("prospect"), board()]),
        (26, vec![build(BuildingType::DeuteriumSynthesizer), command(Command::Scan { fleet_id: 999_999 })]),
        (32, vec![standing("manual"), command(Command::Recall { fleet_id }), board()]),
        (38, vec![board()]),
    ];
    let mut after = PLAY_TICKS;
    for target in into_a_fight(&world, home) {
        script.push((after, vec![command(Command::Move { fleet_id, target })]));
        after += HOP_TICKS;
    }
    script.push((after + FIGHT_TICKS, vec![board()]));
    script
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
    let file = fresh_world("hosts_agree");

    let (token, commander) = {
        let (server, port) = start_server_with(&file, &WORLD);
        let (mut ws, auth) = login(port, NAME, None).await;
        let token = auth["token"].as_str().expect("a new account is given its token").to_string();
        let mut seen = 0;
        while seen < SETTLE_TICKS {
            seen += u64::from(next_json(&mut ws).await["type"] == "tick_update");
        }
        server.stop();
        (token, auth["player_id"].as_u64().unwrap())
    };

    let stored = stored_world(&file);
    assert!(stored.tick >= SETTLE_TICKS && stored.players.contains_key(&commander), "tick {}", stored.tick);
    assert_eq!((stored.world_seed, stored.bots.len()), (SEED, 7), "five hands-on bots, a check-in explorer and a hybrid turtle");
    let mut solo = Solo::load(&file, commander, stored.tick);

    let (server, port) = start_server_with(&file, &WORLD);
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
    assert!(kinds("preview_move") == 1 && kinds("odds") == 1 && kinds("leaderboard") == 6);
    let boards: Vec<&Value> = played.got.iter().filter(|frame| frame["type"] == "leaderboard").collect();
    let scores = |board: &Value| -> f64 { board["bots"].as_array().unwrap().iter().map(|row| row["score"].as_f64().unwrap()).sum() };
    assert_eq!(boards[0]["bots"].as_array().unwrap().len(), 7);
    assert!(scores(boards[4]) > scores(boards[0]) + 20.0, "the bots played on: {} to {}", scores(boards[0]), scores(boards[4]));
    let events = played.got.iter().filter(|frame| frame["type"] == "tick_update" && frame["events"].is_array()).count();
    assert!(events >= 5, "the commander's orders had consequences: {events} updates carried events");
    let told = |kind: &str| -> Vec<&Value> {
        let updates = played.got.iter().filter(|frame| frame["type"] == "tick_update");
        updates.flat_map(|frame| frame["events"].as_array().into_iter().flatten()).filter(|event| event["kind"] == kind).collect()
    };
    let (rounds, ended) = (told("BattleRound"), told("CombatEnded"));
    assert!(
        !rounds.is_empty() && rounds.iter().all(|round| round["fleets"]["shots"].as_u64().unwrap() > 0) && ended.len() == 1,
        "the commander flew into a group that shoots first and both hosts fought it: {} rounds, {} fights ended",
        rounds.len(),
        ended.len(),
    );

    server.stop();
    let ended = stored_world(&file).tick;
    assert!(ended >= played.end);
    for tick in solo.tick() + 1..=ended {
        solo.advance_to(tick);
    }
    let written = std::fs::read(&file).unwrap();
    let (on_server, on_solo) = (header_of(&written).1[1..].to_vec(), solo.world(ended));
    if on_server != on_solo {
        let (a, b): (Value, Value) = (serde_json::from_slice(&on_server).unwrap(), serde_json::from_slice(&on_solo).unwrap());
        let difference = first_difference("world", &a, &b);
        panic!("the worlds part by tick {ended}: {}", difference.unwrap_or_else(|| "in their bytes, not in their JSON".to_string()));
    }
    remove_world(&file);
}

/// The other way round: a save of the single-player host, put where a
/// server keeps its world, is a world that server plays. Nothing is
/// converted. What does not come across is the commander's password: nobody
/// has it, so the account is there and cannot be logged in to.
#[tokio::test]
async fn a_server_plays_on_a_world_the_single_player_host_saved() {
    const PLAYED: u64 = 30;
    let mut machine = Machine::default();
    let bots = ["miner:hands-on", "turtle:check-in"];
    let new_game = json!({"op": "new_game", "name": NAME, "pace": "test", "bots": bots, "entropy": ENTROPY});
    call(&mut machine, T0, new_game, &[]);
    for second in 1..=PLAYED {
        call(&mut machine, T0 + second * 1000, json!({"op": "advance", "max_ticks": 1}), &[]);
    }
    let (saved, save) = call(&mut machine, T0 + PLAYED * 1000, json!({"op": "save"}), &[]);
    assert_eq!(saved["world"]["tick"], PLAYED, "{saved}");
    assert!(header_of(&save).0["host"]["commander"].is_u64(), "a single-player save names its commander");

    let file = fresh_world("hosts_agree_solo_save");
    std::fs::write(&file, &save).unwrap();
    let (server, port) = start_server(&file);
    let (_ws, commander) = login(port, NAME, None).await;
    assert_eq!(commander["code"], "TokenRequired", "the commander's account came with the world, its token did not: {commander}");
    let (mut ws, visitor) = login(port, "Visitor", None).await;
    assert_eq!(visitor["success"], true, "{visitor}");
    ws.send(Message::text(command(Command::Leaderboard { limit: 50 }).to_string())).await.unwrap();
    let board = loop {
        let frame = next_json(&mut ws).await;
        if frame["type"] == "leaderboard" {
            break frame;
        }
    };
    let names = |rows: &str| -> Vec<String> {
        board[rows].as_array().unwrap().iter().map(|row| row["name"].as_str().unwrap().to_string()).collect()
    };
    assert_eq!(names("bots").len(), bots.len(), "the bots of the save play on: {board}");
    assert!(names("entries").contains(&NAME.to_string()) && names("entries").contains(&"Visitor".to_string()), "{board}");
    assert!(board["tick"].as_u64().unwrap() > PLAYED, "the world went on from the save's tick: {board}");
    server.stop();

    let kept = stored_world(&file);
    assert!(kept.tick > PLAYED && kept.bots.len() == bots.len() && kept.meta.pace.preset().is_some_and(|p| p.name == "test"));
    assert_eq!(header_of(&std::fs::read(&file).unwrap()).0["host"], json!({}), "and the file is a server's now");
    remove_world(&file);
}
