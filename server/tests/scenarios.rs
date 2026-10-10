// Protocol-level scenarios against the real server binary: several clients
// at once, one world, only the wire between them and the engine.

mod common;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::*;
use futures_util::SinkExt;
use iac_shared::constants::DEFAULT_WORLD_SEED;
use iac_shared::hex::Hex;
use iac_shared::world::WorldGen;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

async fn send(ws: &mut Ws, msg: Value) {
    ws.send(Message::text(msg.to_string())).await.unwrap();
}

async fn command(ws: &mut Ws, body: Value) {
    let mut msg = body;
    msg["type"] = json!("command");
    send(ws, msg).await;
}

async fn full_state(ws: &mut Ws) -> Value {
    send(ws, json!({"type": "request_full_state"})).await;
    loop {
        let msg = next_json(ws).await;
        if msg["type"] == "full_state" {
            return msg;
        }
    }
}

fn events_of(msg: &Value) -> Vec<Value> {
    msg["events"].as_array().cloned().unwrap_or_default()
}

fn hex_of(v: &Value) -> Hex {
    Hex { q: v["q"].as_i64().unwrap() as i16, r: v["r"].as_i64().unwrap() as i16 }
}

#[tokio::test]
async fn names_are_one_account_whatever_the_case() {
    let port = 17951;
    let db = fresh_db(port);
    let _guard = start_server(port, &db);

    let (_ws, first) = login(port, "Casey", None).await;
    let token = first["token"].as_str().unwrap().to_string();

    let (_ws, bare) = login(port, "cASEY", None).await;
    assert_eq!((bare["success"].clone(), bare["code"].clone()), (json!(false), json!("TokenRequired")), "{bare}");

    let (_ws, resumed) = login(port, "CASEY", Some(&token)).await;
    assert_eq!(resumed["success"], true, "{resumed}");
    assert_eq!(resumed["player_id"], first["player_id"], "one empire under every spelling");

    let (mut ws, board) = login(port, "Other", None).await;
    assert_eq!(board["success"], true);
    command(&mut ws, json!({"action": "leaderboard", "limit": 10})).await;
    let table = loop {
        let msg = next_json(&mut ws).await;
        if msg["type"] == "leaderboard" {
            break msg;
        }
    };
    assert_eq!(table["entries"].as_array().unwrap().len(), 2, "{table}");
    remove_db(&db);
}

#[tokio::test]
async fn a_fight_is_reported_to_the_player_in_it_and_not_to_anyone_else() {
    let port = 17952;
    let db = fresh_db(port);
    let child = std::process::Command::new(env!("CARGO_BIN_EXE_iac-server"))
        .args(["--port", &port.to_string(), "--db", db.to_str().unwrap(), "--pace", "test"])
        .spawn()
        .expect("failed to spawn server");
    let _guard = ServerGuard(child);

    let (mut hunter, _) = login(port, "Hunter", None).await;
    let (mut bystander, _) = login(port, "Bystander", None).await;
    let watched = Arc::new(Mutex::new(Vec::<Value>::new()));
    let watcher = {
        let watched = Arc::clone(&watched);
        tokio::spawn(async move {
            loop {
                let msg = next_json(&mut bystander).await;
                watched.lock().unwrap().extend(events_of(&msg));
            }
        })
    };

    let state = full_state(&mut hunter).await;
    let fleet = state["fleets"][0]["id"].as_u64().unwrap();
    let home = hex_of(&state["player"]["homeworld"]);
    let world = WorldGen::init(DEFAULT_WORLD_SEED);
    let mut parent = std::collections::HashMap::from([(home, home)]);
    let mut frontier = std::collections::VecDeque::from([(home, 0u8)]);
    let mut lair = None;
    while let Some((at, hops)) = frontier.pop_front() {
        if at != home && world.generate_sector(at).npc_template.is_some_and(|t| t.power() < 8.0) {
            lair = Some(at);
            break;
        }
        if hops == 8 {
            continue;
        }
        for &next in world.connected_neighbors(at).slice() {
            if let std::collections::hash_map::Entry::Vacant(slot) = parent.entry(next) {
                slot.insert(at);
                frontier.push_back((next, hops + 1));
            }
        }
    }
    let lair = lair.expect("a weak lair within eight hops");
    let mut route = vec![lair];
    while *route.last().unwrap() != home {
        route.push(parent[route.last().unwrap()]);
    }
    route.reverse();

    let mut mine = Vec::<Value>::new();
    for hop in route[1..].iter().copied() {
        let deadline = Instant::now() + Duration::from_secs(60);
        'hop: loop {
            command(&mut hunter, json!({"action": "move", "fleet_id": fleet, "target": {"q": hop.q, "r": hop.r}})).await;
            loop {
                let msg = next_json(&mut hunter).await;
                mine.extend(events_of(&msg));
                if msg["type"] == "error" {
                    assert!(Instant::now() < deadline, "never allowed to move: {msg}");
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue 'hop;
                }
                if msg["type"] == "tick_update" && hex_of(&msg["fleets"][0]["location"]) == hop && msg["fleets"][0]["state"] != "Moving" {
                    break 'hop;
                }
            }
        }
    }

    let chart = full_state(&mut hunter).await;
    let sector = chart["known_sectors"].as_array().unwrap().iter().find(|s| hex_of(&s["location"]) == lair).expect("the lair is charted");
    let target = sector["hostiles"][0]["id"].as_u64().expect("the lair lists a hostile with an id");

    let deadline = Instant::now() + Duration::from_secs(90);
    let mut attacked = false;
    while !mine.iter().any(|e| e["kind"] == "CombatEnded") {
        assert!(Instant::now() < deadline, "the fight never ended: {mine:?}");
        if !attacked && !mine.iter().any(|e| e["kind"] == "CombatStarted") {
            command(&mut hunter, json!({"action": "attack", "fleet_id": fleet, "target_fleet_id": target})).await;
            attacked = true;
        }
        mine.extend(events_of(&next_json(&mut hunter).await));
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    watcher.abort();

    assert!(mine.iter().any(|e| e["kind"] == "CombatStarted"));
    let combat_kinds = ["CombatStarted", "CombatRound", "CombatEnded", "ShipDestroyed", "FleetDestroyed", "SalvageDespawned"];
    let leaked: Vec<_> = watched.lock().unwrap().iter().filter(|e| combat_kinds.iter().any(|k| e["kind"] == *k)).cloned().collect();
    assert!(leaked.is_empty(), "the bystander was told about a fight they were not in: {leaked:?}");
    remove_db(&db);
}

#[tokio::test]
async fn a_cancel_aimed_at_an_order_that_started_meanwhile_is_refused_and_leaves_it_running() {
    let port = 17953;
    let db = fresh_db(port);
    let child = std::process::Command::new(env!("CARGO_BIN_EXE_iac-server"))
        .args(["--port", &port.to_string(), "--db", db.to_str().unwrap(), "--pace", "test"])
        .spawn()
        .expect("failed to spawn server");
    let _guard = ServerGuard(child);
    let (mut ws, _) = login(port, "Racer", None).await;
    full_state(&mut ws).await;

    for building in ["MetalMine", "CrystalMine", "DeuteriumSynthesizer"] {
        command(&mut ws, json!({"action": "build", "building_type": building})).await;
    }
    let mut waiting_id = None;
    let mut seen = Vec::<Value>::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    while waiting_id.is_none() || !seen.iter().any(|e| e["kind"] == "Queue" && e["action"] == "Started" && Some(&e["id"]) == waiting_id.as_ref()) {
        assert!(Instant::now() < deadline, "the waiting order never started: {seen:?}");
        let msg = next_json(&mut ws).await;
        for e in events_of(&msg) {
            if e["kind"] == "Queue" && e["action"] == "Waiting" && e["item"].as_str().unwrap().starts_with("Crystal") {
                waiting_id = Some(e["id"].clone());
            }
            seen.push(e);
        }
    }
    let id = waiting_id.unwrap();

    command(&mut ws, json!({"action": "cancel_queued", "queue_type": "Building", "id": id})).await;
    let refusal = loop {
        let msg = next_json(&mut ws).await;
        seen.extend(events_of(&msg));
        if msg["type"] == "error" {
            break msg;
        }
    };
    assert_eq!(refusal["code"], "InvalidTarget", "{refusal}");
    let text = refusal["message"].as_str().unwrap();
    assert!(text.contains(&id.to_string()) && (text.contains("cancel_build") || text.contains("finished")), "{text}");

    let deadline = Instant::now() + Duration::from_secs(30);
    while !seen.iter().any(|e| e["kind"] == "BuildingCompleted" && e["building_type"] == "CrystalMine") {
        assert!(Instant::now() < deadline, "the crystal mine was lost with the refused cancel: {seen:?}");
        seen.extend(events_of(&next_json(&mut ws).await));
    }
    assert!(!seen.iter().any(|e| e["kind"] == "Queue" && e["action"] == "Cancelled"), "{seen:?}");
    remove_db(&db);
}
