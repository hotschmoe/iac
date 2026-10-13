// The single-player host through its one door, as the browser worker uses
// it: bytes in, bytes out, the wire protocol inside. The real engine and
// the real bots run behind it; time is whatever the test says it is.

use iac_shared::constants::ECONOMY_VERSION;
use iac_sim::store::SAVE_FORMAT;
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
    let login = log_in(&mut watched, T0);
    let fleet = login[1]["fleets"][0]["id"].as_u64().unwrap();
    assert_eq!(login[1]["fleets"][0].get("policy"), None, "a fleet flown by hand carries no orders");
    send(&mut watched, T0, json!({"type": "command", "action": "build", "building_type": "CrystalMine"}));
    let given = send(&mut watched, T0, json!({
        "type": "policy_update", "fleet_id": fleet, "preset": "patrol_home",
        "params": {"max_range": 2, "engage_ratio_x10": 100, "fuel_budget_pct": 50},
    }));
    assert!(given.iter().all(|f| f["type"] != "error"), "{given:?}");
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

    let back = log_in(&mut reloaded, T0 + 250_000);
    let flown = back[1]["fleets"].as_array().unwrap().iter().find(|f| f["id"] == fleet).expect("the fleet is in the reloaded world");
    assert_eq!(
        flown["policy"],
        json!({
            "preset": "patrol_home",
            "params": {"min_fuel_pct": 10, "cargo_return_pct": 85, "max_range": 2, "engage_ratio_x10": 100, "fuel_budget_pct": 50},
        }),
        "the console that opens the save reads the orders and every setting in force: {flown}",
    );
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
fn a_ship_batch_the_stockpile_does_not_cover_is_refused_and_costs_nothing() {
    const SCOUT: [f64; 3] = [200.0, 50.0, 30.0];
    let stock_of = |state: &Value| ["metal", "crystal", "deuterium"].map(|k| state["player"]["resources"][k].as_f64().unwrap());
    let covered = |stock: [f64; 3]| (0..3).map(|k| (stock[k] / SCOUT[k]).floor() as u64).min().unwrap();

    let mut machine = new_game(T0, "blitz", &[]);
    log_in(&mut machine, T0);
    for building in ["MetalMine", "Shipyard"] {
        let queued = send(&mut machine, T0, json!({"type": "command", "action": "build", "building_type": building}));
        assert!(queued.iter().all(|f| f["type"] != "error"), "a building queues ahead of what it needs: {queued:?}");
    }

    let mut now = T0;
    let state = loop {
        now += 1000;
        play(&mut machine, now - 1000, 1);
        let state = send(&mut machine, now, json!({"type": "request_full_state"})).remove(0);
        let yard = state["homeworld"]["buildings"].as_array().unwrap().iter().any(|b| b["building_type"] == "Shipyard" && b["level"] == 1);
        if yard && covered(stock_of(&state)) >= 1 {
            break state;
        }
        assert!(now < T0 + 600_000, "no shipyard and a scout's worth of stock within ten minutes of blitz");
    };
    let stock = stock_of(&state);
    let fits = covered(stock);

    let too_many = send(&mut machine, now, json!({"type": "command", "action": "build_ship", "ship_class": "Scout", "count": fits + 1}));
    assert_eq!(too_many.len(), 1, "{too_many:?}");
    assert_eq!((too_many[0]["type"].as_str(), too_many[0]["code"].as_str()), (Some("error"), Some("InsufficientResources")));
    let message = too_many[0]["message"].as_str().unwrap();
    assert!(
        message.starts_with(&format!("Scout x{} costs {} metal, ", fits + 1, 200 * (fits + 1)))
            && message.contains("; the stockpile is short by ")
            && message.ends_with(&format!("Nothing was ordered: a batch is paid in full when it is ordered. The stockpile covers {fits}.")),
        "{message}"
    );
    let after = send(&mut machine, now, json!({"type": "request_full_state"})).remove(0);
    assert_eq!(stock_of(&after), stock, "the refused batch cost nothing");
    assert_eq!(after["homeworld"]["shipyard_queue"], Value::Null);

    let ordered = send(&mut machine, now, json!({"type": "command", "action": "build_ship", "ship_class": "Scout", "count": fits}));
    assert!(ordered.iter().all(|f| f["type"] != "error"), "{ordered:?}");
    let paid = send(&mut machine, now, json!({"type": "request_full_state"})).remove(0);
    let left = stock_of(&paid);
    for k in 0..3 {
        assert!((stock[k] - SCOUT[k] * fits as f64 - left[k]).abs() < 0.01, "the whole batch is paid when it is ordered: {stock:?} to {left:?}");
    }
    assert_eq!((paid["homeworld"]["shipyard_queue"]["count"].as_u64(), paid["homeworld"]["shipyard_queue"]["built"].as_u64()), (Some(fits), Some(0)));
}

#[test]
fn a_hold_loaded_at_the_dock_is_in_the_save_and_stays_aboard_after_a_reload() {
    let fleet_of = |frames: &[Value]| frames.iter().rev().find_map(|f| f["fleets"].get(0).cloned()).expect("a frame with the fleet");
    let hold_of = |fleet: &Value| ["metal", "crystal", "deuterium"].map(|k| fleet["cargo"][k].as_f64().unwrap());
    let put_aboard = |resource: &str, amount: Value, fleet: u64| {
        let mut order = json!({"type": "command", "action": "load", "fleet_id": fleet, "resource": resource});
        if !amount.is_null() {
            order["amount"] = amount;
        }
        order
    };

    let mut watched = new_game(T0, "season", &[]);
    let login = log_in(&mut watched, T0);
    let fleet = login[1]["fleets"][0]["id"].as_u64().unwrap();
    let hold = login[1]["fleets"][0]["cargo_capacity"].as_f64().unwrap();
    assert!((hold - 126.49).abs() < 0.01 && login[1]["fleets"][0]["laden"] == false, "{}", login[1]["fleets"][0]);

    let ordered = send(&mut watched, T0, put_aboard("metal", json!(100), fleet));
    assert!(ordered.iter().all(|f| f["type"] != "error"), "{ordered:?}");
    let sent = play(&mut watched, T0, 5);
    let told = events(&sent, "Alert");
    assert!(
        told.iter().any(|a| a["message"].as_str().unwrap().starts_with(&format!("fleet {fleet} loaded 100 metal from the stockpile"))),
        "{told:?}"
    );
    let aboard = fleet_of(&sent);
    assert_eq!((hold_of(&aboard), aboard["laden"].as_bool()), ([100.0, 0.0, 0.0], Some(true)), "five ticks at the dock: {aboard}");

    let saved = save(&mut watched, T0 + 5_000);
    play(&mut watched, T0 + 5_000, 60);
    let watched_at_65 = save(&mut watched, T0 + 65_000);

    let (mut reloaded, reply) = load(T0 + 65_000, &saved);
    assert_eq!((reply["refused"].clone(), reply["world"]["behind"].as_u64()), (Value::Null, Some(60)), "{reply}");
    let (arrived, _) = call(&mut reloaded, T0 + 65_000, json!({"op": "advance", "max_ticks": 1000}), &[]);
    assert_eq!(arrived["world"]["tick"], 65);
    assert!(snapshot_of(&save(&mut reloaded, T0 + 65_000)) == snapshot_of(&watched_at_65), "the two worlds differ at tick 65");
    let back = log_in(&mut reloaded, T0 + 65_000);
    let aboard = &back[1]["fleets"][0];
    assert_eq!((hold_of(aboard), aboard["laden"].as_bool()), ([100.0, 0.0, 0.0], Some(true)), "sixty ticks played from the save: {aboard}");

    let now = T0 + 65_000;
    assert!(send(&mut reloaded, now, put_aboard("crystal", Value::Null, fleet)).iter().all(|f| f["type"] != "error"));
    let no_room = send(&mut reloaded, now, put_aboard("deuterium", json!(5), fleet));
    assert_eq!((no_room[0]["type"].as_str(), no_room[0]["code"].as_str()), (Some("error"), Some("CargoFull")), "{no_room:?}");
    assert_eq!(no_room[0]["message"], format!("the hold of fleet {fleet} is full: it carries 126 of 126"));
    let full = fleet_of(&play(&mut reloaded, now, 1));
    assert!((hold_of(&full)[1] - (hold - 100.0)).abs() < 0.01, "with no amount, all that fits: {full}");

    assert!(send(&mut reloaded, now + 1000, json!({"type": "command", "action": "unload", "fleet_id": fleet})).is_empty());
    let sent = play(&mut reloaded, now + 1000, 1);
    let emptied = fleet_of(&sent);
    assert_eq!((hold_of(&emptied), emptied["laden"].as_bool()), ([0.0, 0.0, 0.0], Some(false)), "{emptied}");
    assert_eq!(events(&sent, "Alert")[0]["message"], format!("fleet {fleet} unloaded 100 metal and 27 crystal into the stockpile"));
}

/// Play second by second from `*now` until a frame satisfies `done`.
fn play_until(machine: &mut Machine, now: &mut u64, limit: u64, done: impl Fn(&Value) -> bool) -> Vec<Value> {
    let mut sent = Vec::new();
    for _ in 0..limit {
        let frames = play(machine, *now, 1);
        *now += 1000;
        let arrived = frames.iter().any(&done);
        sent.extend(frames);
        if arrived {
            return sent;
        }
    }
    panic!("nothing of the kind within {limit} ticks: {:?}", sent.last());
}

fn order(machine: &mut Machine, now: u64, body: Value) -> Vec<Value> {
    let mut frame = body;
    frame["type"] = json!("command");
    send(machine, now, frame)
}

#[test]
fn an_outpost_is_founded_saved_replayed_and_raided_while_the_commander_is_away() {
    use iac_shared::constants::{Density, TerrainType};
    use iac_shared::hex::Hex;
    use iac_shared::world::{NpcBehaviorType, WorldGen};
    let hex_of = |v: &Value| Hex { q: v["q"].as_i64().unwrap() as i16, r: v["r"].as_i64().unwrap() as i16 };
    let amounts = |v: &Value| ["metal", "crystal", "deuterium"].map(|k| v[k].as_f64().unwrap());
    let accepted = |frames: Vec<Value>| assert!(frames.iter().all(|f| f["type"] != "error"), "{frames:?}");

    let mut machine = new_game(T0, "test", &[]);
    let login = log_in(&mut machine, T0);
    let fleet = login[1]["fleets"][0]["id"].as_u64().unwrap();
    let home = hex_of(&login[1]["player"]["homeworld"]);
    assert!(login[1].get("outpost").is_none());
    let (described, _) = call(&mut machine, T0, json!({"op": "describe"}), &[]);
    let world = WorldGen::init(u64::from_str_radix(described["world"]["seed"].as_str().unwrap(), 16).unwrap());

    let calm = |at: Hex| world.generate_sector(at).npc_template.is_none_or(|npc| npc.behavior == NpcBehaviorType::Passive);
    let mut parent = std::collections::HashMap::from([(home, home)]);
    let mut frontier = std::collections::VecDeque::from([home]);
    let mut site = None;
    while let Some(at) = frontier.pop_front() {
        let t = world.generate_sector(at);
        let open = t.npc_template.is_none() && !t.has_derelict && t.terrain != TerrainType::Anomaly;
        if at.dist_from_origin() >= 9 && open && t.metal_density != Density::None && parent[&at] != home {
            site = Some(at);
            break;
        }
        for &next in world.connected_neighbors(at).slice() {
            if calm(next) && !parent.contains_key(&next) {
                parent.insert(next, at);
                frontier.push_back(next);
            }
        }
    }
    let site = site.expect("an ore site in the Outer Ring by calm lanes");
    let mut route = vec![site];
    while parent[route.last().unwrap()] != home {
        route.push(parent[route.last().unwrap()]);
    }
    route.reverse();
    let step_back = route[route.len() - 2];

    let mut now = T0;
    accepted(order(&mut machine, now, json!({"action": "build", "building_type": "DeuteriumSynthesizer"})));
    play_until(&mut machine, &mut now, 600, |f| {
        f["type"] == "tick_update" && amounts(&f["player"]["resources"]).iter().zip([1500.0, 900.0, 700.0]).all(|(have, need)| *have >= need)
    });
    for (resource, amount) in [("metal", 1500), ("crystal", 900), ("deuterium", 600)] {
        accepted(order(&mut machine, now, json!({"action": "load", "fleet_id": fleet, "resource": resource, "amount": amount})));
    }
    let refused = order(&mut machine, now, json!({"action": "found_outpost", "fleet_id": fleet}));
    assert_eq!(refused[0]["code"], "InvalidTarget", "{refused:?}");
    assert_eq!(refused[0]["message"], format!("fleet {fleet} is at ring {}; an outpost stands in rings 9 to 20", home.dist_from_origin()));

    let fly = |machine: &mut Machine, now: &mut u64, hop: Hex| {
        play_until(machine, now, 30, |f| f["type"] == "tick_update" && f["fleets"][0]["state"] == "Idle" && f["fleets"][0]["cooldown_remaining"] == 0);
        accepted(order(machine, *now, json!({"action": "move", "fleet_id": fleet, "target": {"q": hop.q, "r": hop.r}})));
        play_until(machine, now, 30, |f| f["type"] == "tick_update" && hex_of(&f["fleets"][0]["location"]) == hop && f["fleets"][0]["state"] == "Idle");
    };
    for hop in route.iter().copied() {
        fly(&mut machine, &mut now, hop);
    }

    let quote = order(&mut machine, now, json!({"action": "outpost_survey", "fleet_id": fleet}));
    assert_eq!((quote[0]["type"].as_str(), quote[0].get("refusal")), (Some("outpost_survey"), None), "{quote:?}");
    assert_eq!((hex_of(&quote[0]["sector"]), quote[0]["build_ticks"].as_u64(), quote[0]["site"]["hops_home"].as_u64()), (site, Some(7), Some(route.len() as u64)));
    accepted(order(&mut machine, now, json!({"action": "found_outpost", "fleet_id": fleet})));
    accepted(order(&mut machine, now, json!({"action": "unload", "fleet_id": fleet})));
    accepted(order(&mut machine, now, json!({"action": "move", "fleet_id": fleet, "target": {"q": step_back.q, "r": step_back.r}})));
    let founded = play(&mut machine, now, 1);
    now += 1000;
    let post = &founded.last().unwrap()["outpost"];
    assert_eq!((hex_of(&post["location"]), post["level"].as_u64(), post["work"]["kind"].as_str()), (site, Some(0), Some("build")), "{post}");
    assert_eq!(amounts(&post["store"]), [500.0, 400.0, 400.0], "what the hold carried beyond the cost is in the store");
    let told = events(&founded, "Alert");
    assert!(told.iter().any(|a| a["message"].as_str().unwrap().starts_with(&format!("fleet {fleet} founded your outpost at {site}"))), "{told:?}");

    let saved = save(&mut machine, now);
    let watched = play(&mut machine, now, 150);
    let finished = events(&watched, "OutpostCompleted");
    assert_eq!((finished.len(), finished[0]["level"].as_u64()), (1, Some(1)), "{finished:?}");
    let watched_at = save(&mut machine, now + 150_000);
    let (mut reloaded, reply) = load(now + 150_000, &saved);
    assert_eq!((reply["refused"].clone(), reply["world"]["behind"].as_u64()), (Value::Null, Some(150)), "{reply}");
    let (arrived, body) = call(&mut reloaded, now + 150_000, json!({"op": "advance", "max_ticks": 1000}), &[]);
    assert!(body.is_empty() && arrived["world"]["catch_up"] == Value::Null, "{arrived}");
    assert!(
        snapshot_of(&save(&mut reloaded, now + 150_000)) == snapshot_of(&watched_at),
        "the outpost's build, its extractor and its raid clock replay from the save to the same world",
    );

    let back_at = now + 2 * 3_600_000;
    let (mut away, reply) = load(back_at, &saved);
    assert_eq!(reply["world"]["behind"], 7200);
    let arrived = loop {
        let (reply, body) = call(&mut away, back_at, json!({"op": "advance", "max_ticks": 2000}), &[]);
        assert!(body.is_empty());
        if reply["world"]["catch_up"] == Value::Null {
            break reply;
        }
    };
    let notices: Vec<&str> = arrived["world"]["notices"].as_array().unwrap().iter().map(|n| n.as_str().unwrap()).collect();
    assert_eq!(notices[0], "Away 2h 00m: the world ran on without you for 2h 00m (7200 ticks).");
    assert_eq!(notices[1], "While away: 1 outpost level finished.");
    let named = format!("your outpost at {site}");
    let repelled = notices.iter().find(|n| n.contains("repelled") && n.contains(&named));
    let broken = notices.iter().find(|n| n.contains(&format!("broke {named} and took")));
    assert!(repelled.is_some() || broken.is_some(), "two hours of raid clock passed the outpost by: {notices:?}");

    let back = log_in(&mut away, back_at);
    let post = &back[1]["outpost"];
    assert_eq!((hex_of(&post["location"]), post["level"].as_u64()), (site, Some(1)), "no raid destroys an outpost: {post}");
    let alerts: Vec<&str> = events(&back, "Alert").iter().map(|a| a["message"].as_str().unwrap()).collect();
    assert_eq!(alerts, notices, "the report is what the commander reads on return");
    let Some(broken) = broken else { panic!("this world's raids were all repelled; the report of a lost one is untested: {notices:?}") };
    let words: Vec<&str> = broken.split(' ').collect();
    assert!(words[2].parse::<u32>().unwrap() >= 1 && broken.contains(" Fe, ") && broken.contains(" Cr, ") && broken.ends_with('.'), "{broken}");
    let standing = match (post["breached"].as_bool().unwrap(), post["work"]["kind"].as_str()) {
        (false, _) => "It has repaired itself and is whole.".to_string(),
        (true, Some("repair")) => format!("It is breached and under repair until tick {}.", post["work"]["end_tick"]),
        (true, _) => "It is breached, and its store does not cover the repair: it makes nothing until a fleet unloads the rest.".to_string(),
    };
    assert!(broken.ends_with(&standing), "the report says how the outpost stands now ({standing}): {broken}");
    assert!(!broken.contains("there"), "the fleet stood a sector away: no ship was at the outpost to lose: {broken}");
}

#[test]
fn a_save_this_build_cannot_read_is_refused_with_the_reason() {
    let mut machine = new_game(T0, "season", &[]);
    let document = save(&mut machine, T0);
    let text = String::from_utf8(document.clone()).unwrap();

    let other_economy = text.replacen(&format!("\"economy_version\":{ECONOMY_VERSION}"), "\"economy_version\":4", 1);
    let (_, reply) = load(T0, other_economy.as_bytes());
    assert_eq!(reply["world"], Value::Null);
    assert_eq!(
        reply["refused"],
        format!("this save is economy version 4, this build runs version {ECONOMY_VERSION}; start a new game"),
    );

    let other_generator = text.replacen("\"worldgen_version\":3", "\"worldgen_version\":2", 1);
    let (_, reply) = load(T0, other_generator.as_bytes());
    assert!(reply["refused"].as_str().unwrap().contains("worldgen version 2, this build runs version 3"), "{reply}");

    assert_eq!(SAVE_FORMAT, 6);
    let before_the_refine_switch = text.replacen("\"format\":6", "\"format\":5", 1);
    let (_, reply) = load(T0, before_the_refine_switch.as_bytes());
    assert_eq!(reply["world"], Value::Null);
    assert_eq!(reply["refused"], "this save is format 5, this build reads format 6; start a new game");

    let other_format = text.replacen(&format!("\"format\":{SAVE_FORMAT}"), "\"format\":7", 1);
    let (_, reply) = load(T0, other_format.as_bytes());
    assert_eq!(reply["world"], Value::Null);
    assert!(reply["refused"].as_str().unwrap().contains(&format!("format 7, this build reads format {SAVE_FORMAT}")), "{reply}");

    let (_, reply) = load(T0, &document[..document.len() - 40]);
    assert_eq!(reply["refused"], "this save cannot be read: its contents do not match its checksum");
    let (_, reply) = load(T0, b"not a save");
    assert_eq!(reply["refused"], "this save cannot be read: it is not a world document");

    let split = document.iter().position(|&b| b == b'\n').unwrap();
    let mut header: Value = serde_json::from_slice(&document[..split]).unwrap();
    assert_eq!(header["host"]["commander"], 1, "the header's own part names the commander: {header}");
    header["host"] = json!({});
    let of_a_server = [header.to_string().as_bytes(), &document[split..]].concat();
    let (_, reply) = load(T0, &of_a_server);
    assert_eq!(reply["world"], Value::Null);
    let refused = reply["refused"].as_str().unwrap();
    assert!(refused.contains("does not hold what this host keeps beside a world") && refused.contains("commander"), "{reply}");

    let (mut machine, reply) = load(T0, &document);
    assert_eq!(reply["refused"], Value::Null);
    assert_eq!(reply["world"]["commander"], "Admiral");
    let (abandoned, _) = call(&mut machine, T0, json!({"op": "abandon"}), &[]);
    assert_eq!(abandoned["world"], Value::Null);
    let (lost, _) = call(&mut machine, T0, json!({"op": "advance", "max_ticks": 1}), &[]);
    assert_eq!(lost["error"], "no world is loaded");
}
