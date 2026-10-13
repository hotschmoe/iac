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
    let file = fresh_world("scenario_names");
    let (_guard, port) = start_server(&file);

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
    remove_world(&file);
}

#[tokio::test]
async fn a_fight_is_reported_to_the_player_in_it_and_not_to_anyone_else() {
    let file = fresh_world("scenario_fight");
    let (_guard, port) = start_server_with(&file, &["--pace", "test"]);

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
        if at != home && world.generate_sector(at).npc_template.is_some_and(|t| t.power() < 80.0) {
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
    let rounds: Vec<&Value> = mine.iter().filter(|e| e["kind"] == "BattleRound").collect();
    assert!(
        !rounds.is_empty() && rounds.iter().all(|round| round["mine"] == true && round["fleets"]["groups"][0]["mine"] == true),
        "the hunter is told every round of its own fight, and that the fleet in it is its own: {rounds:?}"
    );
    let ended = mine.iter().find(|e| e["kind"] == "CombatEnded").unwrap();
    assert_eq!(ended["rounds"].as_u64(), Some(rounds.len() as u64), "{ended}");
    let combat_kinds = ["CombatStarted", "BattleRound", "CombatEnded", "ShipsLost", "FleetDestroyed", "SalvageDespawned"];
    let leaked: Vec<_> = watched.lock().unwrap().iter().filter(|e| combat_kinds.iter().any(|k| e["kind"] == *k)).cloned().collect();
    assert!(leaked.is_empty(), "the bystander was told about a fight they were not in: {leaked:?}");
    remove_world(&file);
}

#[tokio::test]
async fn a_cancel_aimed_at_an_order_that_started_meanwhile_is_refused_and_leaves_it_running() {
    let file = fresh_world("scenario_cancel");
    let (_guard, port) = start_server_with(&file, &["--pace", "test"]);
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
    remove_world(&file);
}

#[tokio::test]
async fn a_stopped_server_comes_back_with_the_scan_live_the_boarding_under_way_and_the_orders_as_set() {
    let file = fresh_world("scenario_boarding");
    let world = WorldGen::init(DEFAULT_WORLD_SEED);
    let quiet = |at: Hex| world.generate_sector(at).npc_template.is_none();
    let orders = json!({
        "preset": "prospect",
        "params": {"min_fuel_pct": 10, "cargo_return_pct": 85, "max_range": 30, "engage_ratio_x10": 25, "fuel_budget_pct": 35, "refine": false},
    });

    let (token, fleet, site, scanned, end_tick) = {
        let (server, port) = start_server(&file);
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
        let mut ask = true;
        let mut ticks_under_orders = 0;
        while ticks_under_orders < 3 {
            assert!(Instant::now() < deadline, "the boarding never started");
            if ask && end_tick.is_none() {
                command(&mut ws, order(if scanned.is_empty() { "scan" } else { "explore_site" }, None)).await;
                ask = false;
            }
            let msg = next_json(&mut ws).await;
            if msg["type"] == "error" {
                tokio::time::sleep(Duration::from_millis(500)).await;
                ask = true;
            }
            if msg["type"] == "tick_update" && msg["fleets"][0]["policy"] == orders {
                ticks_under_orders += 1;
            }
            for e in events_of(&msg) {
                if e["kind"] == "ScanCompleted" {
                    scanned = e["threats"].as_array().unwrap().iter().map(|t| hex_of(&t["sector"])).collect();
                    ask = true;
                }
                if e["kind"] == "SiteExplorationStarted" {
                    end_tick = e["end_tick"].as_u64();
                    assert!(msg["fleets"][0].get("policy").is_none(), "a fleet flown by hand carries no orders: {msg}");
                    send(&mut ws, json!({
                        "type": "policy_update", "fleet_id": fleet, "preset": "prospect",
                        "params": {"max_range": 99, "engage_ratio_x10": 25, "fuel_budget_pct": 35},
                    })).await;
                }
            }
        }
        server.stop();
        (token, fleet, site, scanned, end_tick.unwrap())
    };

    let (_guard, port) = start_server(&file);
    let (mut ws, reply) = login(port, "Boarder", Some(&token)).await;
    assert_eq!(reply["success"], true, "{reply}");
    let state = full_state(&mut ws).await;
    let now = state["tick"].as_u64().unwrap();
    let boarder = state["fleets"].as_array().unwrap().iter().find(|f| f["id"] == fleet).expect("the fleet survived the restart");
    assert_eq!(hex_of(&boarder["location"]), site);
    assert_eq!(boarder["state"], "Exploring", "the boarding was dropped: {boarder}");
    assert_eq!(boarder["cooldown_remaining"].as_u64().unwrap(), end_tick - now, "the boarding lost its place: {boarder}");
    assert_eq!(
        boarder["policy"], orders,
        "a console that never gave the orders reads the doctrine and every setting in force, the range as the server clamped it: {boarder}",
    );

    assert!(scanned.len() > 1, "the scan reached beyond the fleet's own sector");
    let chart = state["known_sectors"].as_array().unwrap();
    for sector in scanned {
        let known = chart.iter().find(|s| hex_of(&s["location"]) == sector).expect("a scanned sector is charted");
        assert_eq!(known["live"], true, "{sector} went stale with the restart");
    }
    remove_world(&file);
}

/// The hops from `from` to the nearest sector `goal` accepts, over sectors
/// with no hostile group of their own.
fn quiet_route(world: &WorldGen, from: Hex, goal: impl Fn(Hex) -> bool) -> Vec<Hex> {
    let mut parent = std::collections::HashMap::from([(from, from)]);
    let mut frontier = std::collections::VecDeque::from([(from, 0u8)]);
    let mut found = None;
    while let Some((at, hops)) = frontier.pop_front() {
        if at != from && goal(at) {
            found = Some(at);
            break;
        }
        if hops == 10 {
            continue;
        }
        for &next in world.connected_neighbors(at).slice() {
            if world.generate_sector(next).npc_template.is_none() && !parent.contains_key(&next) {
                parent.insert(next, at);
                frontier.push_back((next, hops + 1));
            }
        }
    }
    let mut route = vec![found.expect("such a sector within ten quiet hops")];
    while *route.last().unwrap() != from {
        route.push(parent[route.last().unwrap()]);
    }
    route.reverse();
    route.split_off(1)
}

async fn fly(ws: &mut Ws, fleet: u64, route: &[Hex]) {
    let deadline = Instant::now() + Duration::from_secs(90);
    for hop in route.iter().copied() {
        'hop: loop {
            command(ws, json!({"action": "move", "fleet_id": fleet, "target": {"q": hop.q, "r": hop.r}})).await;
            loop {
                let msg = next_json(ws).await;
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
}

#[tokio::test]
async fn a_fleet_flies_its_ore_to_the_hub_trades_it_and_the_meter_outlives_the_server() {
    use iac_shared::constants::Density;
    let file = fresh_world("scenario_exchange");
    let world = WorldGen::init(DEFAULT_WORLD_SEED);
    let weight = |kind: &str| if kind == "metal" { 1.0 } else if kind == "crystal" { 1.5 } else { 2.0 };

    let (token, fleet, hub, give, want, gave, got, pressure) = {
        let (server, port) = start_server(&file);
        let (mut ws, reply) = login(port, "Trader", None).await;
        let token = reply["token"].as_str().unwrap().to_string();
        let state = full_state(&mut ws).await;
        let fleet = state["fleets"][0]["id"].as_u64().unwrap();
        let home = hex_of(&state["player"]["homeworld"]);
        let site = state["world"]["hub"].as_array().unwrap().iter().find(|s| s["services"] == json!(["exchange"])).expect("the world names its exchange");
        let hub = hex_of(&site["location"]);
        assert_eq!(hub, Hex::ORIGIN);
        assert_eq!(state["player"]["exchange"]["pressure"], 0.0, "{state}");

        let seam = quiet_route(&world, home, |at| {
            let t = world.generate_sector(at);
            [t.metal_density, t.crystal_density].iter().any(|d| *d as u8 >= Density::Moderate as u8)
        });
        fly(&mut ws, fleet, &seam).await;
        command(&mut ws, json!({"action": "exchange", "fleet_id": fleet, "give": "metal", "want": "crystal"})).await;
        let early = loop {
            let msg = next_json(&mut ws).await;
            if msg["type"] == "error" {
                break msg;
            }
        };
        assert_eq!(early["code"], "NoResources", "an empty hold has nothing to trade: {early}");

        command(&mut ws, json!({"action": "harvest", "fleet_id": fleet, "resource": "Auto"})).await;
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let msg = next_json(&mut ws).await;
            assert!(Instant::now() < deadline, "the seam gave nothing: {msg}");
            if msg["type"] == "error" {
                tokio::time::sleep(Duration::from_millis(500)).await;
                command(&mut ws, json!({"action": "harvest", "fleet_id": fleet, "resource": "Auto"})).await;
            }
            if msg["type"] != "tick_update" {
                continue;
            }
            let cargo = &msg["fleets"][0]["cargo"];
            let total: f64 = ["metal", "crystal", "deuterium"].iter().map(|k| cargo[k].as_f64().unwrap()).sum();
            if total >= 10.0 || (total >= 6.0 && msg["fleets"][0]["state"] != "Harvesting") {
                break;
            }
        }
        command(&mut ws, json!({"action": "stop", "fleet_id": fleet})).await;
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let cargo = full_state(&mut ws).await["fleets"][0]["cargo"].clone();
        let give = if cargo["metal"].as_f64().unwrap() >= cargo["crystal"].as_f64().unwrap() { "metal" } else { "crystal" };
        let want = if give == "metal" { "crystal" } else { "metal" };

        command(&mut ws, json!({"action": "exchange_quote", "fleet_id": fleet, "give": give, "want": want})).await;
        let afar = loop {
            let msg = next_json(&mut ws).await;
            if msg["type"] == "exchange_quote" {
                break msg;
            }
        };
        assert!(afar["refusal"].as_str().is_some_and(|why| why.contains("from the exchange at the hub")), "{afar}");

        let here = hex_of(&full_state(&mut ws).await["fleets"][0]["location"]);
        fly(&mut ws, fleet, &quiet_route(&world, here, |at| at == hub)).await;

        command(&mut ws, json!({"action": "exchange_quote", "fleet_id": fleet, "give": give, "want": want})).await;
        let quote = loop {
            let msg = next_json(&mut ws).await;
            assert!(msg["type"] != "error", "{msg}");
            if msg["type"] == "exchange_quote" {
                break msg;
            }
        };
        assert!(quote.get("refusal").is_none(), "{quote}");
        let (gave, got) = (quote["amount"].as_f64().unwrap(), quote["receive"].as_f64().unwrap());
        assert_eq!(Some(gave), cargo[give].as_f64(), "with no amount, all of it: {quote}");
        assert!(got >= 1.0 && got.fract() == 0.0, "{quote}");
        assert!(got * weight(want) <= 0.75 * gave * weight(give), "the exchange keeps at least a quarter: {quote}");
        assert!(quote["fee"].as_f64().unwrap() >= 0.25 && quote["meter"]["pressure"] == 0.0, "{quote}");

        command(&mut ws, json!({"action": "exchange", "fleet_id": fleet, "give": give, "want": want})).await;
        let (traded, update) = loop {
            let msg = next_json(&mut ws).await;
            assert!(msg["type"] != "error", "{msg}");
            if let Some(e) = events_of(&msg).into_iter().find(|e| e["kind"] == "Exchanged") {
                break (e, msg);
            }
        };
        assert_eq!((traded["amount"].as_f64(), traded["receive"].as_f64()), (Some(gave), Some(got)), "the trade is the quote: {traded}");
        assert_eq!((traded["fleet_id"].as_u64(), hex_of(&traded["sector"])), (Some(fleet), hub));
        let hold = &update["fleets"][0]["cargo"];
        assert_eq!(hold[give], 0.0, "{update}");
        assert!((hold[want].as_f64().unwrap() - cargo[want].as_f64().unwrap() - got).abs() < 0.001, "{update}");
        let pressure = update["player"]["exchange"]["pressure"].as_f64().unwrap();
        assert!((pressure - gave * weight(give)).abs() < 0.1, "the meter rose by the weight sold: {update}");
        assert!(update["player"]["exchange"]["clear_in"].as_u64().unwrap() > 600, "{update}");

        command(&mut ws, json!({"action": "exchange", "fleet_id": fleet, "give": want, "want": want})).await;
        let refused = loop {
            let msg = next_json(&mut ws).await;
            if msg["type"] == "error" {
                break msg;
            }
        };
        assert_eq!(refused["code"], "InvalidCommand", "{refused}");
        assert!(refused["message"].as_str().unwrap().contains("give and want are both"), "{refused}");
        server.stop();
        (token, fleet, hub, give, want, gave, got, pressure)
    };

    let (_guard, port) = start_server(&file);
    let (mut ws, reply) = login(port, "Trader", Some(&token)).await;
    assert_eq!(reply["success"], true, "{reply}");
    let state = full_state(&mut ws).await;
    let trader = state["fleets"].as_array().unwrap().iter().find(|f| f["id"] == fleet).expect("the fleet survived the restart");
    assert_eq!(hex_of(&trader["location"]), hub);
    assert_eq!(trader["cargo"][give], 0.0, "{trader}");
    assert!(trader["cargo"][want].as_f64().unwrap() >= got, "{trader}");
    let meter = &state["player"]["exchange"];
    let after = meter["pressure"].as_f64().unwrap();
    assert!(after > 0.9 * pressure && after < pressure, "the meter came back, a little lower: {after} after {pressure} ({gave} {give} sold)");
    assert!(meter["fee"].as_f64().unwrap() > 0.25, "{meter}");
    remove_world(&file);
}

async fn refused(ws: &mut Ws) -> Value {
    loop {
        let msg = next_json(ws).await;
        if msg["type"] == "error" {
            return msg;
        }
    }
}

async fn update_where(ws: &mut Ws, done: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let msg = next_json(ws).await;
        assert!(msg["type"] != "error", "{msg}");
        assert!(Instant::now() < deadline, "never saw the update: {msg}");
        if msg["type"] == "tick_update" && done(&msg) {
            return msg;
        }
    }
}

fn alerts_of(msg: &Value) -> Vec<String> {
    events_of(msg).iter().filter(|e| e["kind"] == "Alert").map(|e| e["message"].as_str().unwrap().to_string()).collect()
}

#[tokio::test]
async fn a_hold_loaded_at_the_dock_outlives_the_server_and_unloads_once_the_fleet_has_been_away() {
    let file = fresh_world("scenario_hold");
    let world = WorldGen::init(DEFAULT_WORLD_SEED);
    let hold_of = |fleet: &Value| ["metal", "crystal", "deuterium"].map(|k| fleet["cargo"][k].as_f64().unwrap());

    let (token, fleet, home, stocked) = {
        let (server, port) = start_server(&file);
        let (mut ws, reply) = login(port, "Loader", None).await;
        let token = reply["token"].as_str().unwrap().to_string();
        let state = full_state(&mut ws).await;
        let fleet = state["fleets"][0]["id"].as_u64().unwrap();
        let home = hex_of(&state["player"]["homeworld"]);
        assert_eq!((state["fleets"][0]["cargo_capacity"].as_f64(), state["fleets"][0]["laden"].as_bool()), (Some(100.0), Some(false)));
        let stocked = state["player"]["resources"]["metal"].as_f64().unwrap();

        command(&mut ws, json!({"action": "load", "fleet_id": fleet, "resource": "metal", "amount": 25})).await;
        let loaded = update_where(&mut ws, |m| m["fleets"][0]["laden"] == true).await;
        assert_eq!(hold_of(&loaded["fleets"][0]), [25.0, 0.0, 0.0], "{loaded}");
        assert_eq!(loaded["fleets"][0]["cargo_blocked"], false);
        assert!(alerts_of(&loaded).iter().any(|a| a.starts_with(&format!("fleet {fleet} loaded 25 metal from the stockpile: it carries 25 of 100"))), "{loaded}");
        let metal = loaded["player"]["resources"]["metal"].as_f64().unwrap();
        assert!((stocked - 25.0..stocked - 24.0).contains(&metal), "the stockpile paid for it: {stocked} to {metal}");

        command(&mut ws, json!({"action": "load", "fleet_id": fleet, "resource": "crystal"})).await;
        let full = update_where(&mut ws, |m| m["fleets"][0]["cargo"]["crystal"] != 0.0).await;
        assert_eq!(hold_of(&full["fleets"][0]), [25.0, 75.0, 0.0], "with no amount, all that fits: {full}");

        command(&mut ws, json!({"action": "load", "fleet_id": fleet, "resource": "deuterium"})).await;
        let no_room = refused(&mut ws).await;
        assert_eq!(no_room["code"], "CargoFull", "{no_room}");
        assert_eq!(no_room["message"], format!("the hold of fleet {fleet} is full: it carries 100 of 100"));
        command(&mut ws, json!({"action": "load", "fleet_id": fleet, "resource": "metal", "amount": 9000})).await;
        let no_stock = refused(&mut ws).await;
        assert_eq!(no_stock["code"], "NoResources", "{no_stock}");
        assert!(no_stock["message"].as_str().unwrap().ends_with(&format!("metal; fleet {fleet} cannot load 9000.0")), "{no_stock}");
        command(&mut ws, json!({"action": "recall", "fleet_id": fleet})).await;
        let at_home = refused(&mut ws).await;
        assert_eq!(at_home["code"], "InvalidCommand", "{at_home}");
        assert_eq!(
            at_home["message"],
            format!("fleet {fleet} is already at its homeworld; recall only brings a fleet home from another sector"),
        );

        for _ in 0..3 {
            let later = update_where(&mut ws, |_| true).await;
            assert_eq!(hold_of(&later["fleets"][0]), [25.0, 75.0, 0.0], "the dock leaves a laden hold alone: {later}");
        }
        server.stop();
        (token, fleet, home, stocked)
    };

    let (_guard, port) = start_server(&file);
    let (mut ws, reply) = login(port, "Loader", Some(&token)).await;
    assert_eq!(reply["success"], true, "{reply}");
    let state = full_state(&mut ws).await;
    assert_eq!(state["fleets"][0]["laden"], true, "the mark came back with the fleet: {}", state["fleets"][0]);
    for _ in 0..3 {
        let later = update_where(&mut ws, |_| true).await;
        assert_eq!(hold_of(&later["fleets"][0]), [25.0, 75.0, 0.0], "and the restarted dock leaves it alone: {later}");
        assert_eq!(later["fleets"][0]["laden"], true);
    }

    let out = quiet_route(&world, home, |at| at != home);
    fly(&mut ws, fleet, &out).await;
    let away = full_state(&mut ws).await;
    assert_eq!((hold_of(&away["fleets"][0]), away["fleets"][0]["laden"].as_bool()), ([25.0, 75.0, 0.0], Some(false)), "{away}");
    command(&mut ws, json!({"action": "unload", "fleet_id": fleet})).await;
    let afar = refused(&mut ws).await;
    assert_eq!(afar["code"], "NotInSector", "{afar}");
    assert!(afar["message"].as_str().unwrap().contains("1 hex from home"), "{afar}");

    fly(&mut ws, fleet, &[home]).await;
    let docked = update_where(&mut ws, |m| hold_of(&m["fleets"][0]) == [0.0, 0.0, 0.0]).await;
    assert_eq!(docked["fleets"][0]["laden"], false, "{docked}");
    assert!(docked["player"]["resources"]["metal"].as_f64().unwrap() >= stocked, "what it brought home is in the stockpile: {docked}");

    command(&mut ws, json!({"action": "load", "fleet_id": fleet, "resource": "metal", "amount": 10})).await;
    update_where(&mut ws, |m| m["fleets"][0]["laden"] == true).await;
    command(&mut ws, json!({"action": "unload", "fleet_id": fleet})).await;
    let emptied = update_where(&mut ws, |m| m["fleets"][0]["laden"] == false).await;
    assert_eq!(hold_of(&emptied["fleets"][0]), [0.0, 0.0, 0.0], "{emptied}");
    assert_eq!(alerts_of(&emptied), [format!("fleet {fleet} unloaded 10 metal into the stockpile")]);
    command(&mut ws, json!({"action": "unload", "fleet_id": fleet})).await;
    let again = refused(&mut ws).await;
    assert_eq!(again["code"], "InvalidCommand", "{again}");
    assert!(again["message"].as_str().unwrap().contains("was not loaded by hand"), "{again}");
    remove_world(&file);
}

/// The hops from `from` to the nearest sector `goal` accepts, over sectors
/// where nothing attacks a fleet on sight.
fn calm_route(world: &WorldGen, from: Hex, goal: impl Fn(Hex) -> bool) -> Vec<Hex> {
    use iac_shared::world::NpcBehaviorType;
    let calm = |at: Hex| world.generate_sector(at).npc_template.is_none_or(|npc| npc.behavior == NpcBehaviorType::Passive);
    let mut parent = std::collections::HashMap::from([(from, from)]);
    let mut frontier = std::collections::VecDeque::from([(from, 0u8)]);
    let mut found = None;
    while let Some((at, hops)) = frontier.pop_front() {
        if at != from && goal(at) {
            found = Some(at);
            break;
        }
        if hops == 14 {
            continue;
        }
        for &next in world.connected_neighbors(at).slice() {
            if calm(next) && !parent.contains_key(&next) {
                parent.insert(next, at);
                frontier.push_back((next, hops + 1));
            }
        }
    }
    let mut route = vec![found.expect("such a sector within fourteen calm hops")];
    while *route.last().unwrap() != from {
        route.push(parent[route.last().unwrap()]);
    }
    route.reverse();
    route.split_off(1)
}

async fn reply_of(ws: &mut Ws, kind: &str) -> Value {
    loop {
        let msg = next_json(ws).await;
        assert!(msg["type"] != "error", "{msg}");
        if msg["type"] == kind {
            return msg;
        }
    }
}

#[tokio::test]
async fn an_outpost_is_surveyed_refused_founded_outlives_a_server_stopped_while_it_is_being_built_and_is_abandoned_through_the_headless_client() {
    use iac_shared::constants::{Density, TerrainType};
    let file = fresh_world("scenario_outpost");
    let world = WorldGen::init(DEFAULT_WORLD_SEED);
    let site_for_an_outpost = |at: Hex| {
        let t = world.generate_sector(at);
        at.dist_from_origin() >= 9
            && t.npc_template.is_none()
            && !t.has_derelict
            && t.terrain != TerrainType::Anomaly
            && t.metal_density != Density::None
    };
    let amounts = |v: &Value| ["metal", "crystal", "deuterium"].map(|k| v[k].as_f64().unwrap());

    let (token, fleet, site, end_tick, store) = {
        let (server, port) = start_server_with(&file, &["--pace", "6000"]);
        let (mut ws, reply) = login(port, "Settler", None).await;
        let token = reply["token"].as_str().unwrap().to_string();
        let state = full_state(&mut ws).await;
        let fleet = state["fleets"][0]["id"].as_u64().unwrap();
        let home = hex_of(&state["player"]["homeworld"]);
        assert!(state.get("outpost").is_none() && state["fleets"][0].get("outpost_fuel").is_none(), "{state}");
        let not_here = format!("fleet {fleet} is at ring {}; an outpost stands in rings 9 to 20", home.dist_from_origin());

        command(&mut ws, json!({"action": "outpost_survey", "fleet_id": fleet})).await;
        let at_home = reply_of(&mut ws, "outpost_survey").await;
        assert_eq!((hex_of(&at_home["sector"]), at_home["refusal"].as_str()), (home, Some(not_here.as_str())), "{at_home}");
        assert_eq!((amounts(&at_home["cost"]), at_home["build_ticks"].as_u64(), at_home["site"]["hops_home"].as_u64()), ([3000.0, 1500.0, 600.0], Some(9), Some(0)));
        command(&mut ws, json!({"action": "found_outpost", "fleet_id": fleet})).await;
        let first = refused(&mut ws).await;
        assert_eq!((first["code"].as_str(), first["message"].as_str()), (Some("InvalidTarget"), Some(not_here.as_str())), "{first}");
        command(&mut ws, json!({"action": "upgrade_outpost"})).await;
        let second = refused(&mut ws).await;
        assert_eq!((second["code"].as_str(), second["message"].as_str()), (Some("InvalidCommand"), Some("you have no outpost to upgrade")), "{second}");

        command(&mut ws, json!({"action": "build", "building_type": "DeuteriumSynthesizer"})).await;
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let msg = next_json(&mut ws).await;
            assert!(Instant::now() < deadline, "the stockpile never covered an outpost: {msg}");
            if msg["type"] == "tick_update" && amounts(&msg["player"]["resources"]).iter().zip([3100.0, 1600.0, 800.0]).all(|(have, need)| *have >= need) {
                break;
            }
        }
        for (resource, amount) in [("metal", 3100), ("crystal", 1600), ("deuterium", 700)] {
            command(&mut ws, json!({"action": "load", "fleet_id": fleet, "resource": resource, "amount": amount})).await;
            update_where(&mut ws, |m| m["fleets"][0]["cargo"][resource] == f64::from(amount)).await;
        }

        let route = calm_route(&world, home, site_for_an_outpost);
        let site = *route.last().unwrap();
        fly(&mut ws, fleet, &route).await;
        command(&mut ws, json!({"action": "outpost_survey", "fleet_id": fleet})).await;
        let quote = reply_of(&mut ws, "outpost_survey").await;
        assert!(quote.get("refusal").is_none(), "{quote}");
        assert_eq!((hex_of(&quote["sector"]), quote["ring"].as_u64(), quote["site"]["hops_home"].as_u64()), (site, Some(u64::from(site.dist_from_origin())), Some(route.len() as u64)));
        assert!(quote["site"]["production"]["metal"].as_f64().unwrap() > 0.0 && quote["site"]["grade"].as_f64().unwrap() >= 0.5, "{quote}");
        assert!(quote["raid_power"]["least"].as_f64().unwrap() < quote["raid_power"]["most"].as_f64().unwrap(), "{quote}");

        command(&mut ws, json!({"action": "found_outpost", "fleet_id": fleet})).await;
        let founded = update_where(&mut ws, |m| m.get("outpost").is_some()).await;
        let post = &founded["outpost"];
        assert_eq!((hex_of(&post["location"]), post["level"].as_u64(), post["in_service"].as_bool()), (site, Some(0), Some(false)), "{post}");
        assert_eq!((post["work"]["kind"].as_str(), post["work"]["level"].as_u64()), (Some("build"), Some(1)), "{post}");
        let end_tick = post["work"]["end_tick"].as_u64().unwrap();
        assert_eq!(end_tick, post["work"]["start_tick"].as_u64().unwrap() + 9);
        assert!(alerts_of(&founded).iter().any(|a| a.starts_with(&format!("fleet {fleet} founded your outpost at {site}"))), "{founded}");
        assert_eq!(amounts(&founded["fleets"][0]["cargo"]), [100.0, 100.0, 100.0], "the hold paid the cost: {founded}");
        assert_eq!(
            (founded["fleets"][0]["at_outpost"].as_bool(), founded["fleets"][0].get("outpost_fuel")),
            (Some(true), None),
            "the fleet stands at an outpost that fills no tank yet: {founded}",
        );

        command(&mut ws, json!({"action": "unload", "fleet_id": fleet})).await;
        let stocked = update_where(&mut ws, |m| m["outpost"]["store"]["metal"] == 100.0).await;
        let store = amounts(&stocked["outpost"]["store"]);
        assert_eq!((store, amounts(&stocked["fleets"][0]["cargo"])), ([100.0, 100.0, 100.0], [0.0, 0.0, 0.0]), "{stocked}");
        let charted = full_state(&mut ws).await["known_sectors"].as_array().unwrap().iter().find(|s| hex_of(&s["location"]) == site).unwrap().clone();
        assert_eq!(charted["outpost"], json!({"owner": "Settler", "level": 0, "breached": false, "mine": true}));
        assert_eq!(charted["threat"], json!({"rating": 0, "est_power": 0.0, "basis": "observed"}), "nothing to fight where an outpost stands");

        server.stop();
        (token, fleet, site, end_tick, store)
    };
    assert!(stored_world(&file).tick < end_tick, "the build must still be under way when the server stops");

    let (_guard, port) = start_server(&file);
    let (mut ws, reply) = login(port, "Settler", Some(&token)).await;
    assert_eq!(reply["success"], true, "{reply}");
    let state = full_state(&mut ws).await;
    let post = &state["outpost"];
    assert!(state["tick"].as_u64().unwrap() < end_tick, "the restart came back inside the build: {}", state["tick"]);
    assert_eq!((hex_of(&post["location"]), post["level"].as_u64()), (site, Some(0)), "the outpost came back with the world: {post}");
    assert_eq!((post["work"]["kind"].as_str(), post["work"]["end_tick"].as_u64()), (Some("build"), Some(end_tick)), "its countdown is as it was: {post}");
    assert_eq!(amounts(&post["store"]), store, "and so is its store: {post}");
    let settler = state["fleets"].as_array().unwrap().iter().find(|f| f["id"] == fleet).expect("the fleet survived the restart");
    assert_eq!(hex_of(&settler["location"]), site);

    let opened = update_where(&mut ws, |m| m["outpost"]["level"] == 1).await;
    assert_eq!(opened["tick"].as_u64(), Some(end_tick), "level 1 is finished on the tick it was promised for");
    let done: Vec<Value> = events_of(&opened).into_iter().filter(|e| e["kind"] == "OutpostCompleted").collect();
    assert_eq!(done.len(), 1, "{opened}");
    assert_eq!((hex_of(&done[0]["sector"]), done[0]["level"].as_u64()), (site, Some(1)));
    assert_eq!((opened["outpost"]["in_service"].as_bool(), opened["fleets"][0]["outpost_fuel"].as_f64()), (Some(true), Some(0.0)), "{opened}");
    let made = opened["outpost"]["store"]["metal"].as_f64().unwrap();
    let later = update_where(&mut ws, |_| true).await;
    assert!(made > store[0] && later["outpost"]["store"]["metal"].as_f64().unwrap() > made, "the yield goes on: {later}");
    drop(ws);

    let mut pipe = HeadlessClient::start(port, "Settler", &token, file.parent().unwrap());
    let auth = pipe.next();
    assert_eq!((auth["type"].as_str(), auth["success"].as_bool()), (Some("auth_result"), Some(true)), "{auth}");
    let seen = pipe.until(|m| m["type"] == "full_state");
    assert_eq!((hex_of(&seen["outpost"]["location"]), seen["outpost"]["level"].as_u64()), (site, Some(1)), "the pipe prints the outpost as the server sent it: {seen}");
    assert_eq!(seen["fleets"][0]["outpost_fuel"].as_f64(), Some(0.0), "{seen}");
    let marked = seen["known_sectors"].as_array().unwrap().iter().find(|s| hex_of(&s["location"]) == site).unwrap();
    assert_eq!(marked["outpost"], json!({"owner": "Settler", "level": 1, "breached": false, "mine": true}));

    pipe.say(&json!({"action": "outpost_survey", "fleet_id": fleet}).to_string());
    let survey = pipe.until(|m| m["type"] == "outpost_survey");
    let taken = format!("you already have an outpost at {site}; an empire has one, and abandon_outpost gives it up");
    assert_eq!((hex_of(&survey["sector"]), survey["refusal"].as_str()), (site, Some(taken.as_str())), "a bare command is wrapped and its reply printed: {survey}");
    pipe.say(&json!({"type": "command", "action": "upgrade_outpost"}).to_string());
    let short = pipe.until(|m| m["type"] == "error");
    assert_eq!(short["code"].as_str(), Some("InsufficientResources"), "{short}");
    let costs = format!("outpost level 2 costs 7500 metal, 3750 crystal and 1500 deuterium; the store at {site} is short by ");
    assert!(short["message"].as_str().unwrap().starts_with(&costs), "the refusal passes through word for word: {short}");
    assert!(seen["outpost"]["upgrade_refusal"].as_str().unwrap().starts_with(&costs), "and the state had said it beforehand: {seen}");
    pipe.say(r#"{"action": "found_outpost"}"#);
    let unread = pipe.until(|m| m["type"] == "client_error");
    assert!(unread["message"].as_str().unwrap().contains("fleet_id"), "a line the client cannot read is answered in band and not sent: {unread}");
    assert!(pipe.until(|m| m["type"] == "tick_update").get("outpost").is_some(), "the outpost still stands");

    pipe.say(&json!({"action": "abandon_outpost"}).to_string());
    let gone = pipe.until(|m| m["type"] == "tick_update" && m.get("outpost").is_none());
    assert!(alerts_of(&gone).iter().any(|a| a.starts_with(&format!("your outpost at {site} is abandoned: "))), "{gone}");
    assert!(gone["fleets"][0].get("outpost_fuel").is_none(), "{gone}");
    drop(pipe);
    remove_world(&file);
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
async fn built_in_bots_play_tagged_and_unranked_and_carry_on_after_the_server_is_restarted() {
    let file = fresh_world("scenario_bots");
    let roster = [
        "--pace", "test", "--player-seats", "20",
        "--bot", "turtle:hands-on:2", "--bot", "explorer:hands-on", "--bot", "miner:hands-on", "--bot", "raider:hands-on:5",
    ];
    let deadline = Instant::now() + Duration::from_secs(90);

    let (token, names, scores, tick) = {
        let (server, port) = start_server_with(&file, &roster);
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
        server.stop();
        (token, names, bots, board["tick"].as_u64().unwrap())
    };

    let (_guard, port) = start_server_with(&file, &["--player-seats", "20", "--bot", "raider:hands-on:5"]);
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
    remove_world(&file);
}

#[tokio::test]
async fn a_full_world_turns_a_new_name_away_and_still_admits_its_settlers_after_it_is_killed() {
    let file = fresh_world("scenario_full_world");
    let seats = home_seats().count();

    let token = {
        let (server, port) = start_server(&file);
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
        tokio::time::sleep(REGISTRATION_IS_STORED).await;
        server.kill();
        first_token.unwrap()
    };
    assert_eq!(stored_world(&file).players.len(), seats, "every registration of the burst is in the file a few seconds on");

    let (_guard, port) = start_server(&file);
    let (_ws, refused) = login(port, "Latecomer", None).await;
    assert_eq!(refused["code"], "WorldFull", "the world is still full after a restart: {refused}");
    let (_ws, back) = login(port, "Settler0", Some(&token)).await;
    assert_eq!(back["success"], true, "{back}");
    remove_world(&file);
}
