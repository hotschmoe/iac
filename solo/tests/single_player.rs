// The single-player host through its one door, as the browser worker uses
// it: bytes in, bytes out, the wire protocol inside. The real engine and
// the real bots run behind it; time is whatever the test says it is.

use iac_solo::{Machine, CATCH_UP_LIMIT};
use serde_json::{json, Value};

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
    (serde_json::from_slice(&reply[..split]).expect("reply JSON"), reply[split + 1..].to_vec())
}

fn frames(body: &[u8]) -> Vec<Value> {
    body.split(|&b| b == b'\n').filter(|line| !line.is_empty()).map(|line| serde_json::from_slice(line).unwrap()).collect()
}

fn send(machine: &mut Machine, now_ms: u64, frame: Value) -> Vec<Value> {
    let (reply, body) = call(machine, now_ms, json!({"op": "frame"}), frame.to_string().as_bytes());
    assert_eq!(reply["error"], Value::Null, "{reply}");
    frames(&body)
}

fn new_game(now_ms: u64, pace: &str, bots: &[&str]) -> Machine {
    let mut machine = Machine::default();
    let (reply, _) = call(
        &mut machine,
        now_ms,
        json!({"op": "new_game", "name": "Admiral", "pace": pace, "bots": bots, "entropy": ENTROPY}),
        &[],
    );
    assert_eq!(reply["error"], Value::Null, "{reply}");
    machine
}

fn log_in(machine: &mut Machine, now_ms: u64) -> Vec<Value> {
    call(machine, now_ms, json!({"op": "open"}), &[]);
    send(machine, now_ms, json!({"type": "auth", "player_name": "Admiral"}))
}

/// Advance second by second from `from_ms` for `seconds`; every frame sent.
fn play(machine: &mut Machine, from_ms: u64, seconds: u64) -> Vec<Value> {
    let mut sent = Vec::new();
    for second in 1..=seconds {
        let (reply, body) = call(machine, from_ms + second * 1000, json!({"op": "advance", "max_ticks": 100}), &[]);
        assert_eq!(reply["error"], Value::Null, "{reply}");
        sent.extend(frames(&body));
    }
    sent
}

fn save(machine: &mut Machine, now_ms: u64) -> Vec<u8> {
    call(machine, now_ms, json!({"op": "save"}), &[]).1
}

fn load(now_ms: u64, document: &[u8]) -> (Machine, Value) {
    let mut machine = Machine::default();
    let (reply, _) = call(&mut machine, now_ms, json!({"op": "load"}), document);
    (machine, reply)
}

/// The world a save document holds, without its header.
fn snapshot_of(document: &[u8]) -> &[u8] {
    let split = document.iter().position(|&b| b == b'\n').expect("a header line");
    &document[split + 1..]
}

fn events<'a>(sent: &'a [Value], kind: &str) -> Vec<&'a Value> {
    sent.iter()
        .flat_map(|frame| match frame["type"].as_str() {
            Some("tick_update") => frame["events"].as_array().map(|e| e.iter().collect()).unwrap_or_default(),
            Some("event") => vec![frame],
            _ => Vec::new(),
        })
        .filter(|event| event["kind"] == kind)
        .collect()
}

#[test]
fn a_new_world_is_played_over_the_wire_as_on_a_server() {
    let mut machine = Machine::default();
    let (described, _) = call(&mut machine, T0, json!({"op": "describe"}), &[]);
    assert_eq!(described["world"], Value::Null);
    assert_eq!(described["options"]["max_bots"], 23);
    assert!(described["options"]["presets"].as_array().unwrap().iter().any(|p| p["name"] == "blitz"));

    let (refused, _) = call(
        &mut machine,
        T0,
        json!({"op": "new_game", "name": "BOT-me", "pace": "blitz", "bots": [], "entropy": ENTROPY}),
        &[],
    );
    assert!(refused["error"].as_str().unwrap().contains("reserved for built-in bots"), "{refused}");
    assert_eq!(refused["world"], Value::Null);

    let mut machine = new_game(T0, "blitz", &["all:hands-on", "miner:check-in:2"]);
    call(&mut machine, T0, json!({"op": "open"}), &[]);

    let early = send(&mut machine, T0, json!({"type": "command", "action": "scan", "fleet_id": 1}));
    assert_eq!(early[0]["code"], "AuthFailed", "{early:?}");
    let stranger = send(&mut machine, T0, json!({"type": "auth", "player_name": "Stranger"}));
    assert_eq!(stranger[0]["success"], false);
    assert_eq!(stranger[0]["code"], "InvalidName");

    let login = send(&mut machine, T0, json!({"type": "auth", "player_name": "admiral"}));
    assert_eq!(login[0]["type"], "auth_result");
    assert_eq!(login[0]["success"], true);
    assert_eq!(login[0].get("token"), None, "a single-player account has no password to show");
    assert_eq!(login[1]["type"], "full_state");
    assert_eq!(login[1]["player"]["name"], "Admiral");
    assert_eq!(login[1]["world"]["preset"], "blitz");
    let again = send(&mut machine, T0, json!({"type": "auth", "player_name": "Admiral"}));
    assert_eq!(again[0]["code"], "AlreadyAuthenticated");

    let ordered = send(&mut machine, T0, json!({"type": "command", "action": "build", "building_type": "MetalMine"}));
    assert!(ordered.iter().all(|f| f["type"] != "error"), "{ordered:?}");

    let sent = play(&mut machine, T0, 120);
    let updates = sent.iter().filter(|f| f["type"] == "tick_update").count();
    assert_eq!(updates, 120, "one tick_update a second");
    let built = events(&sent, "BuildingCompleted");
    assert!(built.iter().any(|e| e["building_type"] == "MetalMine" && e["new_level"] == 2), "{built:?}");

    let board = send(&mut machine, T0 + 120_000, json!({"type": "command", "action": "leaderboard", "limit": 10}));
    assert_eq!(board[0]["type"], "leaderboard");
    assert_eq!(board[0]["entries"].as_array().unwrap().len(), 1, "the commander is the one ranked player");
    let bots = board[0]["bots"].as_array().unwrap();
    assert_eq!(bots.len(), 7);
    assert!(bots.iter().all(|b| b["name"].as_str().unwrap().starts_with("BOT-")));
    assert!(bots.iter().any(|b| b["score"].as_f64().unwrap() > board[0]["entries"][0]["score"].as_f64().unwrap() * 0.5));
}

#[test]
fn a_reload_plays_the_unsaved_ticks_again_and_arrives_at_the_same_world() {
    let mut watched = new_game(T0, "blitz", &["all:hands-on"]);
    log_in(&mut watched, T0);
    send(&mut watched, T0, json!({"type": "command", "action": "build", "building_type": "CrystalMine"}));
    play(&mut watched, T0, 100);
    let saved = save(&mut watched, T0 + 100_000);
    play(&mut watched, T0 + 100_000, 150);
    send(&mut watched, T0 + 250_000, json!({"type": "command", "action": "leaderboard", "limit": 10}));
    let watched_at_250 = save(&mut watched, T0 + 250_000);

    let (mut reloaded, reply) = load(T0 + 250_000, &saved);
    assert_eq!(reply["refused"], Value::Null, "{reply}");
    assert_eq!(reply["world"]["tick"], 100);
    assert_eq!(reply["world"]["behind"], 150);
    assert_eq!(reply["world"]["bots"], 5);
    let (caught_up, body) = call(&mut reloaded, T0 + 250_000, json!({"op": "advance", "max_ticks": 70}), &[]);
    assert_eq!(caught_up["world"]["catch_up"]["played"], 70);
    assert_eq!(caught_up["world"]["catch_up"]["remaining"], 80);
    assert!(body.is_empty(), "missed ticks are played without frames");
    let (arrived, _) = call(&mut reloaded, T0 + 250_000, json!({"op": "advance", "max_ticks": 1000}), &[]);
    assert_eq!(arrived["world"]["tick"], 250);
    assert_eq!(arrived["world"]["catch_up"], Value::Null);
    assert_eq!(arrived["world"]["notices"], json!([]), "a stall under five minutes is not an absence");

    let reloaded_at_250 = save(&mut reloaded, T0 + 250_000);
    assert!(snapshot_of(&reloaded_at_250) == snapshot_of(&watched_at_250), "the two worlds differ at tick 250");
}

#[test]
fn an_absence_is_played_reported_and_limited() {
    let mut machine = new_game(T0, "season", &["turtle:check-in", "raider:check-in"]);
    log_in(&mut machine, T0);
    send(&mut machine, T0, json!({"type": "command", "action": "build", "building_type": "MetalMine"}));
    play(&mut machine, T0, 10);
    let left_at = T0 + 10_000;
    let document = save(&mut machine, left_at);

    let back_at = left_at + 2 * 3_600_000;
    let (mut machine, reply) = load(back_at, &document);
    assert_eq!(reply["world"]["behind"], 7200);
    let mut rounds = 0;
    let arrived = loop {
        let (reply, body) = call(&mut machine, back_at, json!({"op": "advance", "max_ticks": 1000}), &[]);
        assert!(body.is_empty());
        rounds += 1;
        if reply["world"]["catch_up"] == Value::Null {
            break reply;
        }
        assert_eq!(reply["world"]["catch_up"]["played"], rounds * 1000);
    };
    assert_eq!(arrived["world"]["tick"], 7210);
    let notices: Vec<&str> = arrived["world"]["notices"].as_array().unwrap().iter().map(|n| n.as_str().unwrap()).collect();
    assert_eq!(notices[0], "Away 2h 00m: the world ran on without you for 2h 00m (7200 ticks).");
    assert!(notices[1].starts_with("While away: 1 building finished"), "{notices:?}");

    let kept = save(&mut machine, back_at);
    let (mut machine, reply) = load(back_at, &kept);
    assert_eq!(reply["world"]["notices"].as_array().unwrap().len(), notices.len(), "an unread report is saved");
    let login = log_in(&mut machine, back_at);
    assert_eq!(login[1]["type"], "full_state");
    assert_eq!(login[1]["tick"], 7210);
    let mine = login[1]["homeworld"]["buildings"].as_array().unwrap().iter().find(|b| b["building_type"] == "MetalMine").unwrap();
    assert_eq!(mine["level"], 2);
    let alerts = events(&login, "Alert");
    assert_eq!(alerts.len(), notices.len());
    assert_eq!(alerts[0]["message"], notices[0]);
    let (told, _) = call(&mut machine, back_at, json!({"op": "describe"}), &[]);
    assert_eq!(told["world"]["notices"], json!([]));

    let long_after = back_at + 30 * 86_400_000;
    let (first, _) = call(&mut machine, long_after, json!({"op": "advance", "max_ticks": 400}), &[]);
    assert_eq!(first["world"]["catch_up"]["played"], 400);
    assert_eq!(first["world"]["catch_up"]["remaining"], CATCH_UP_LIMIT - 400);
    let (_, body) = call(&mut machine, long_after, json!({"op": "take_command"}), &[]);
    let brought = frames(&body);
    assert_eq!(brought[0]["type"], "full_state", "a logged-in session is brought up to date");
    assert_eq!(brought[0]["tick"], 7610);
    let report = events(&brought, "Alert")[0]["message"].as_str().unwrap().to_string();
    assert!(report.starts_with("Away 30d 0h: the world ran on without you for 6m 40s (400 ticks), the most it catches up"), "{report}");
    assert!(report.contains("you took command early and the last 6d 23h were skipped"), "{report}");
    let live = play(&mut machine, long_after, 3);
    assert_eq!(live.iter().filter(|f| f["type"] == "tick_update").count(), 3, "the clock runs from where command was taken");

    let set_back = play(&mut machine, T0, 3);
    assert_eq!(set_back.iter().filter(|f| f["type"] == "tick_update").count(), 2, "a wall clock set back does not stop the world");
}

#[test]
fn a_save_this_build_cannot_read_is_refused_with_the_reason() {
    let mut machine = new_game(T0, "season", &[]);
    let document = save(&mut machine, T0);
    let text = String::from_utf8(document.clone()).unwrap();

    let other_economy = text.replacen("\"economy_version\":2", "\"economy_version\":1", 1);
    let (_, reply) = load(T0, other_economy.as_bytes());
    assert_eq!(reply["world"], Value::Null);
    assert_eq!(reply["refused"], "this save is economy version 1, this build runs version 2; start a new game");

    let other_generator = text.replacen("\"worldgen_version\":3", "\"worldgen_version\":2", 1);
    let (_, reply) = load(T0, other_generator.as_bytes());
    assert!(reply["refused"].as_str().unwrap().contains("worldgen version 2, this build runs version 3"), "{reply}");

    let other_format = text.replacen("\"format\":1", "\"format\":7", 1);
    let (_, reply) = load(T0, other_format.as_bytes());
    assert!(reply["refused"].as_str().unwrap().contains("format 7, this build reads format 1"), "{reply}");

    let (_, reply) = load(T0, &document[..document.len() - 40]);
    assert!(reply["refused"].as_str().unwrap().contains("damaged"), "{reply}");
    let (_, reply) = load(T0, b"not a save");
    assert!(reply["refused"].as_str().unwrap().contains("not a save document"), "{reply}");

    let (mut machine, reply) = load(T0, &document);
    assert_eq!(reply["refused"], Value::Null);
    assert_eq!(reply["world"]["commander"], "Admiral");
    let (abandoned, _) = call(&mut machine, T0, json!({"op": "abandon"}), &[]);
    assert_eq!(abandoned["world"], Value::Null);
    let (lost, _) = call(&mut machine, T0, json!({"op": "advance", "max_ticks": 1}), &[]);
    assert_eq!(lost["error"], "no world is loaded");
}
