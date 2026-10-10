// Protocol-level scenarios against the real server binary: several clients
// at once, one world, only the wire between them and the engine.

mod common;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::*;
use futures_util::SinkExt;
use iac_shared::constants::DEFAULT_WORLD_SEED;
use iac_shared::hex::Hex;
use iac_shared::world::{WorldGen, home_seats};
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

#[tokio::test]
async fn a_killed_server_comes_back_with_the_scan_live_and_the_boarding_under_way() {
    let port = 17954;
    let db = fresh_db(port);
    let world = WorldGen::init(DEFAULT_WORLD_SEED);
    let quiet = |at: Hex| world.generate_sector(at).npc_template.is_none();

    let (token, fleet, site, scanned, end_tick) = {
        let _guard = start_server(port, &db);
        let (mut ws, reply) = login(port, "Boarder", None).await;
        let token = reply["token"].as_str().unwrap().to_string();
        let state = full_state(&mut ws).await;
        let fleet = state["fleets"][0]["id"].as_u64().unwrap();
        let home = hex_of(&state["player"]["homeworld"]);

        let mut parent = std::collections::HashMap::from([(home, home)]);
        let mut frontier = std::collections::VecDeque::from([(home, 0u8)]);
        let mut site = None;
        while let Some((at, hops)) = frontier.pop_front() {
            if at != home && world.generate_sector(at).has_derelict {
                site = Some(at);
                break;
            }
            if hops == 8 {
                continue;
            }
            for &next in world.connected_neighbors(at).slice() {
                if quiet(next) && !parent.contains_key(&next) {
                    parent.insert(next, at);
                    frontier.push_back((next, hops + 1));
                }
            }
        }
        let site = site.expect("a derelict within eight quiet hops");
        let mut route = vec![site];
        while *route.last().unwrap() != home {
            route.push(parent[route.last().unwrap()]);
        }
        route.reverse();

        let deadline = Instant::now() + Duration::from_secs(90);
        let order = |action: &str, target: Option<Hex>| {
            let mut body = json!({"action": action, "fleet_id": fleet});
            if let Some(hex) = target {
                body["target"] = json!({"q": hex.q, "r": hex.r});
            }
            body
        };
        for hop in route[1..].iter().copied() {
            'hop: loop {
                command(&mut ws, order("move", Some(hop))).await;
                loop {
                    let msg = next_json(&mut ws).await;
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

        let mut scanned = Vec::<Hex>::new();
        let mut end_tick = None;
        let mut send = true;
        let mut ticks_saved = 0;
        while ticks_saved < 3 {
            assert!(Instant::now() < deadline, "the boarding never started");
            if send && end_tick.is_none() {
                command(&mut ws, order(if scanned.is_empty() { "scan" } else { "explore_site" }, None)).await;
                send = false;
            }
            let msg = next_json(&mut ws).await;
            if msg["type"] == "error" {
                tokio::time::sleep(Duration::from_millis(500)).await;
                send = true;
            }
            if msg["type"] == "tick_update" && end_tick.is_some() {
                ticks_saved += 1;
            }
            for e in events_of(&msg) {
                if e["kind"] == "ScanCompleted" {
                    scanned = e["threats"].as_array().unwrap().iter().map(|t| hex_of(&t["sector"])).collect();
                    send = true;
                }
                if e["kind"] == "SiteExplorationStarted" {
                    end_tick = e["end_tick"].as_u64();
                }
            }
        }
        (token, fleet, site, scanned, end_tick.unwrap())
    };

    let _guard = start_server(port, &db);
    let (mut ws, reply) = login(port, "Boarder", Some(&token)).await;
    assert_eq!(reply["success"], true, "{reply}");
    let state = full_state(&mut ws).await;
    let now = state["tick"].as_u64().unwrap();
    let boarder = state["fleets"].as_array().unwrap().iter().find(|f| f["id"] == fleet).expect("the fleet survived the restart");
    assert_eq!(hex_of(&boarder["location"]), site);
    assert_eq!(boarder["state"], "Exploring", "the boarding was dropped: {boarder}");
    assert_eq!(boarder["cooldown_remaining"].as_u64().unwrap(), end_tick - now, "the boarding lost its place: {boarder}");

    assert!(scanned.len() > 1, "the scan reached beyond the fleet's own sector");
    let chart = state["known_sectors"].as_array().unwrap();
    for sector in scanned {
        let known = chart.iter().find(|s| hex_of(&s["location"]) == sector).expect("a scanned sector is charted");
        assert_eq!(known["live"], true, "{sector} went stale with the restart");
    }
    remove_db(&db);
}

async fn leaderboard(ws: &mut Ws) -> Value {
    command(ws, json!({"action": "leaderboard", "limit": 20})).await;
    loop {
        let msg = next_json(ws).await;
        if msg["type"] == "leaderboard" {
            return msg;
        }
    }
}

fn rows(board: &Value, list: &str) -> Vec<(String, f64)> {
    let rows = board[list].as_array().cloned().unwrap_or_default();
    rows.iter().map(|row| (row["name"].as_str().unwrap().to_string(), row["score"].as_f64().unwrap())).collect()
}

#[tokio::test]
async fn built_in_bots_play_tagged_and_unranked_and_carry_on_after_the_server_is_killed() {
    let port = 17956;
    let db = fresh_db(port);
    let roster = [
        "--pace", "test", "--player-seats", "20",
        "--bot", "turtle:hands-on:2", "--bot", "explorer:hands-on", "--bot", "miner:hands-on", "--bot", "raider:hands-on:5",
    ];
    let deadline = Instant::now() + Duration::from_secs(90);

    let (token, names, scores, tick) = {
        let _guard = start_server_with(port, &db, &roster);
        let (mut ws, reply) = login(port, "Watcher", None).await;
        let token = reply["token"].as_str().unwrap().to_string();

        let board = loop {
            let board = leaderboard(&mut ws).await;
            let building = rows(&board, "bots").iter().filter(|(_, score)| *score > 2.0).count();
            if building == 4 && board["tick"].as_u64().unwrap() >= 8 {
                break board;
            }
            assert!(Instant::now() < deadline, "the bots never got going: {board}");
            tokio::time::sleep(Duration::from_millis(500)).await;
        };
        assert_eq!(rows(&board, "entries").iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["Watcher"]);
        assert_eq!(board["entries"][0]["rank"], 1, "bots take no rank: {board}");
        let bots = rows(&board, "bots");
        assert_eq!(bots.len(), 4, "20 of the 24 home seats are kept for players, so the raiders stay out: {bots:?}");
        assert!(bots.iter().all(|(name, _)| name.starts_with("BOT-")), "every bot is tagged in its name: {bots:?}");
        assert!(bots.iter().all(|(_, score)| *score > rows(&board, "entries")[0].1), "{board}");
        let ranks: Vec<u64> = board["bots"].as_array().unwrap().iter().map(|row| row["rank"].as_u64().unwrap()).collect();
        assert_eq!(ranks, [1, 2, 3, 4], "bots are ranked among themselves");

        let (_ws, taken) = login(port, &bots[0].0, None).await;
        assert_eq!((taken["success"].clone(), taken["code"].clone()), (json!(false), json!("TokenRequired")), "{taken}");
        let (_ws, reserved) = login(port, "bot-impostor", None).await;
        assert_eq!((reserved["success"].clone(), reserved["code"].clone()), (json!(false), json!("InvalidName")), "{reserved}");
        assert!(reserved["message"].as_str().unwrap().contains("reserved for built-in bots"), "{reserved}");

        let mut names: Vec<String> = bots.iter().map(|(name, _)| name.clone()).collect();
        names.sort();
        (token, names, bots, board["tick"].as_u64().unwrap())
    };

    let _guard = start_server_with(port, &db, &["--player-seats", "20", "--bot", "raider:hands-on:5"]);
    let (mut ws, reply) = login(port, "Watcher", Some(&token)).await;
    assert_eq!(reply["success"], true, "{reply}");
    let board = loop {
        let board = leaderboard(&mut ws).await;
        let grown = rows(&board, "bots")
            .iter()
            .filter(|(name, score)| scores.iter().any(|(was, before)| was == name && score > before))
            .count();
        if grown == 4 && board["tick"].as_u64().unwrap() > tick + 5 {
            break board;
        }
        assert!(Instant::now() < deadline, "the bots did not carry on after the restart: {board} (before: {scores:?})");
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    let mut after: Vec<String> = rows(&board, "bots").into_iter().map(|(name, _)| name).collect();
    after.sort();
    assert_eq!(after, names, "the same bots though --bot no longer lists them, and no raider: the seats left are the players'");
    assert_eq!(rows(&board, "entries").len(), 1);
    remove_db(&db);
}

#[tokio::test]
async fn a_full_world_turns_a_new_name_away_and_still_admits_its_settlers_after_a_restart() {
    let port = 17955;
    let db = fresh_db(port);
    let seats = home_seats().count();

    let token = {
        let _guard = start_server(port, &db);
        let mut homes = Vec::new();
        let mut first_token = None;
        for i in 0..seats {
            let (mut ws, reply) = login(port, &format!("Settler{i}"), None).await;
            assert_eq!(reply["success"], true, "seat {i}: {reply}");
            first_token.get_or_insert_with(|| reply["token"].as_str().unwrap().to_string());
            homes.push(hex_of(&full_state(&mut ws).await["player"]["homeworld"]));
        }
        homes.sort_by_key(|h| h.to_key());
        homes.dedup();
        assert_eq!(homes.len(), seats, "every settler has a seat of their own");

        let (_ws, refused) = login(port, "Latecomer", None).await;
        assert_eq!((refused["success"].clone(), refused["code"].clone()), (json!(false), json!("WorldFull")), "{refused}");
        assert!(refused["message"].as_str().unwrap().contains("world is full"), "{refused}");
        assert!(refused.get("token").is_none() && refused.get("player_id").is_none(), "{refused}");
        tokio::time::sleep(Duration::from_millis(2500)).await;
        first_token.unwrap()
    };

    let _guard = start_server(port, &db);
    let (_ws, refused) = login(port, "Latecomer", None).await;
    assert_eq!(refused["code"], "WorldFull", "the world is still full after a restart: {refused}");
    let (_ws, back) = login(port, "Settler0", Some(&token)).await;
    assert_eq!(back["success"], true, "{back}");
    remove_db(&db);
}
