// One world on both targets: the host this test is compiled into and
// `iac_solo.wasm`, the engine a browser runs, built here by the cargo of the
// test run and executed by an interpreter. Every request goes to both and
// the two replies must be the same bytes: every frame a session is sent and,
// at the checkpoints, the whole save. When they part the failure names the
// tick and the first value that differs.
//
// The worlds are the ones that used to part (the native build took powers
// and logarithms from the system's libm, the wasm build from Rust's own): a
// blitz of hands-on bots past tick 2400 and a season day, with their fights,
// wrecks, raids and scores, and a commander who trades at the hub, founds an
// outpost and stays away while it is raided.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use iac_shared::constants::{Density, TerrainType};
use iac_shared::hex::Hex;
use iac_shared::world::{NpcBehaviorType, SectorTemplate, WorldGen};
use iac_solo::Machine;
use serde_json::{json, Value};
use wasmi::{Caller, Engine, Linker, Memory, Module, Store, TypedFunc};

const T0: u64 = 1_800_000_000_000;
const ENTROPY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// `iac_solo.wasm` as `scripts/play.sh --single-player` builds it.
fn engine_bytes() -> &'static [u8] {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    BYTES.get_or_init(|| {
        let target = Path::new(env!("CARGO_TARGET_TMPDIR")).parent().expect("the target directory");
        let built = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["build", "--release", "-p", "iac-solo", "--target", "wasm32-unknown-unknown", "--target-dir"])
            .arg(target)
            .status()
            .expect("cargo did not run");
        assert!(
            built.success(),
            "iac-solo did not build for WebAssembly; the target comes with `rustup target add wasm32-unknown-unknown`"
        );
        std::fs::read(target.join("wasm32-unknown-unknown/release/iac_solo.wasm")).expect("the engine cargo just built")
    })
}

/// The engine as a browser worker holds it: three exports and its memory.
struct Wasm {
    store: Store<Option<String>>,
    memory: Memory,
    request: TypedFunc<u32, u32>,
    call: TypedFunc<(), u32>,
    reply: TypedFunc<(), u32>,
}

impl Wasm {
    fn start() -> Wasm {
        let engine = Engine::default();
        let module = Module::new(&engine, engine_bytes()).expect("a module the interpreter accepts");
        let mut store = Store::new(&engine, None);
        let mut linker = Linker::new(&engine);
        linker
            .func_wrap("env", "iac_panic", |mut caller: Caller<'_, Option<String>>, at: u32, len: u32| {
                let memory = caller.get_export("memory").and_then(|export| export.into_memory()).expect("the engine's memory");
                let mut message = vec![0; len as usize];
                memory.read(&caller, at as usize, &mut message).expect("the panic message lies in the memory");
                *caller.data_mut() = Some(String::from_utf8_lossy(&message).into_owned());
            })
            .expect("the panic import");
        let instance = linker.instantiate_and_start(&mut store, &module).expect("the engine starts");
        Wasm {
            memory: instance.get_memory(&store, "memory").expect("the engine's memory"),
            request: instance.get_typed_func(&store, "iac_request").expect("iac_request"),
            call: instance.get_typed_func(&store, "iac_call").expect("iac_call"),
            reply: instance.get_typed_func(&store, "iac_reply").expect("iac_reply"),
            store,
        }
    }

    fn call(&mut self, request: &[u8]) -> Vec<u8> {
        let at = self.request.call(&mut self.store, request.len() as u32);
        let at = self.alive(at);
        self.memory.write(&mut self.store, at as usize, request).expect("the request buffer lies in the memory");
        let len = self.call.call(&mut self.store, ());
        let len = self.alive(len);
        let from = self.reply.call(&mut self.store, ());
        let from = self.alive(from);
        let mut reply = vec![0; len as usize];
        self.memory.read(&self.store, from as usize, &mut reply).expect("the reply lies in the memory");
        reply
    }

    fn alive(&self, outcome: Result<u32, wasmi::Error>) -> u32 {
        outcome.unwrap_or_else(|trap| {
            panic!("the wasm engine stopped: {} ({trap})", self.store.data().as_deref().unwrap_or("it left no message"))
        })
    }
}

/// Where two JSON values first differ, and the two values there.
fn first_difference(at: &str, native: &Value, wasm: &Value) -> Option<String> {
    match (native, wasm) {
        (Value::Object(a), Value::Object(b)) if a.keys().eq(b.keys()) => {
            a.iter().zip(b.values()).find_map(|((key, left), right)| first_difference(&format!("{at}/{key}"), left, right))
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            a.iter().zip(b).enumerate().find_map(|(i, (left, right))| first_difference(&format!("{at}/{i}"), left, right))
        }
        _ if native == wasm => None,
        _ => Some(format!("{at}: native {native}, wasm {wasm}")),
    }
}

/// What to say when the two replies to `asked` are not the same bytes.
fn parting(asked: &Value, native: &[u8], wasm: &[u8]) -> String {
    let lines = |bytes: &[u8]| -> Vec<Value> {
        bytes.split(|&b| b == b'\n').map(|line| serde_json::from_slice(line).unwrap_or(Value::Null)).collect()
    };
    let (native, wasm) = (lines(native), lines(wasm));
    let differences: Vec<String> = native
        .iter()
        .zip(&wasm)
        .enumerate()
        .filter_map(|(line, (left, right))| first_difference(&format!("line {line} "), left, right))
        .collect();
    let told = differences
        .iter()
        .find(|difference| !difference.contains("/digest: "))
        .or(differences.first())
        .cloned()
        .unwrap_or_else(|| format!("the same JSON in {} and {} lines, written differently", native.len(), wasm.len()));
    format!("native and wasm part at tick {}, in the reply to {asked}: {told}", native[0]["world"]["tick"])
}

/// One world hosted twice, and the wall clock both are told.
struct Twin {
    native: Machine,
    wasm: Wasm,
    now_ms: u64,
}

impl Twin {
    fn new_game(pace: &str, bots: &[&str]) -> Twin {
        let mut twin = Twin { native: Machine::default(), wasm: Wasm::start(), now_ms: T0 };
        let (reply, _) =
            twin.call(json!({"op": "new_game", "name": "Admiral", "pace": pace, "bots": bots, "entropy": ENTROPY}), &[]);
        assert_eq!(reply["world"]["tick"], 0, "{reply}");
        twin
    }

    /// The same request to both hosts. The replies must be the same bytes.
    fn call(&mut self, mut head: Value, body: &[u8]) -> (Value, Vec<u8>) {
        head["now_ms"] = json!(self.now_ms);
        let mut request = serde_json::to_vec(&head).unwrap();
        if !body.is_empty() {
            request.push(b'\n');
            request.extend(body);
        }
        let native = self.native.call(&request);
        let wasm = self.wasm.call(&request);
        assert!(native == wasm, "{}", parting(&head, &native, &wasm));
        let split = native.iter().position(|&b| b == b'\n').expect("a reply line");
        let reply: Value = serde_json::from_slice(&native[..split]).expect("reply JSON");
        assert_eq!(reply["error"], Value::Null, "{head}: {reply}");
        (reply, native[split + 1..].to_vec())
    }

    fn send(&mut self, frame: Value) -> Vec<Value> {
        let (_, body) = self.call(json!({"op": "frame"}), frame.to_string().as_bytes());
        frames(&body)
    }

    fn order(&mut self, mut command: Value) -> Vec<Value> {
        command["type"] = json!("command");
        self.send(command)
    }

    fn accepted(&mut self, command: Value) {
        let answer = self.order(command.clone());
        assert!(answer.iter().all(|frame| frame["type"] != "error"), "{command}: {answer:?}");
    }

    fn log_in(&mut self) -> Vec<Value> {
        self.call(json!({"op": "open"}), &[]);
        self.send(json!({"type": "auth", "player_name": "Admiral"}))
    }

    /// One tick, a second later.
    fn tick(&mut self) -> Vec<Value> {
        self.now_ms += 1000;
        frames(&self.call(json!({"op": "advance", "max_ticks": 100}), &[]).1)
    }

    /// Tick until a frame satisfies `done`; every frame sent on the way.
    fn play_until(&mut self, limit: u64, done: impl Fn(&Value) -> bool) -> Vec<Value> {
        let mut sent = Vec::new();
        for _ in 0..limit {
            let frames = self.tick();
            let arrived = frames.iter().any(&done);
            sent.extend(frames);
            if arrived {
                return sent;
            }
        }
        panic!("nothing of the kind within {limit} ticks: {:?}", sent.last());
    }

    /// The wall clock jumps `ticks` seconds: an absence, played without frames.
    fn stay_away(&mut self, ticks: u64) -> Vec<Value> {
        self.now_ms += ticks * 1000;
        let (reply, body) = self.call(json!({"op": "advance", "max_ticks": ticks}), &[]);
        assert_eq!(reply["world"]["behind"], 0, "{reply}");
        frames(&body)
    }

    /// Both worlds saved and found equal to the byte; the snapshot of one.
    fn checkpoint(&mut self) -> Value {
        let (_, document) = self.call(json!({"op": "save"}), &[]);
        let split = document.iter().position(|&b| b == b'\n').expect("a header line");
        serde_json::from_slice(&document[split + 1..]).expect("the snapshot")
    }

    /// Jump the one fleet to the sector next door and wait for it there.
    fn fly(&mut self, fleet: u64, hop: Hex) {
        let ready = |f: &Value| f["type"] == "tick_update" && f["fleets"][0]["state"] == "Idle" && f["fleets"][0]["cooldown_remaining"] == 0;
        self.play_until(30, ready);
        self.accepted(json!({"action": "move", "fleet_id": fleet, "target": {"q": hop.q, "r": hop.r}}));
        self.play_until(30, |f| f["type"] == "tick_update" && hex_of(&f["fleets"][0]["location"]) == hop && f["fleets"][0]["state"] == "Idle");
    }
}

fn frames(body: &[u8]) -> Vec<Value> {
    body.split(|&b| b == b'\n').filter(|line| !line.is_empty()).map(|line| serde_json::from_slice(line).unwrap()).collect()
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

fn hex_of(v: &Value) -> Hex {
    Hex { q: v["q"].as_i64().unwrap() as i16, r: v["r"].as_i64().unwrap() as i16 }
}

fn rows(of: &Value) -> Vec<&Value> {
    match of {
        Value::Object(map) => map.values().collect(),
        Value::Array(list) => list.iter().collect(),
        _ => Vec::new(),
    }
}

/// What a snapshot shows the bots to have done: players with combat points,
/// players with exploration points, homes raided, sectors a kill has heated.
fn deeds(snapshot: &Value) -> [usize; 4] {
    let count = |of: &str, field: &str| rows(&snapshot[of]).iter().filter(|row| row[field].as_f64().unwrap_or(0.0) > 0.0).count();
    [
        count("players", "combat_points"),
        count("players", "explore_points"),
        count("raid_states", "last_raid_tick"),
        count("sector_overrides", "kill_heat"),
    ]
}

/// The lanes from `home` to the nearest sector `wanted` accepts, the sector
/// itself last, through sectors where no group shoots first.
fn calm_route(world: &WorldGen, home: Hex, wanted: impl Fn(Hex, &SectorTemplate) -> bool) -> Vec<Hex> {
    let calm = |at: Hex| world.generate_sector(at).npc_template.is_none_or(|npc| npc.behavior == NpcBehaviorType::Passive);
    let mut parent = HashMap::from([(home, home)]);
    let mut frontier = VecDeque::from([home]);
    while let Some(at) = frontier.pop_front() {
        if parent[&at] != home && at != home && wanted(at, &world.generate_sector(at)) {
            let mut route = vec![at];
            while parent[route.last().unwrap()] != home {
                route.push(parent[route.last().unwrap()]);
            }
            route.reverse();
            return route;
        }
        for &next in world.connected_neighbors(at).slice() {
            if calm(next) && !parent.contains_key(&next) {
                parent.insert(next, at);
                frontier.push_back(next);
            }
        }
    }
    panic!("no such sector by calm lanes from {home}");
}

#[test]
fn a_blitz_of_hands_on_bots_is_one_world_past_the_tick_where_the_two_parted() {
    let mut twin = Twin::new_game("blitz", &["all:hands-on", "miner:hands-on:3"]);
    let mut snapshot = Value::Null;
    let (mut largest, mut hurt) = (0, 0);
    for _ in 0..10 {
        twin.stay_away(300);
        snapshot = twin.checkpoint();
        let fleets = rows(&snapshot["fleets"]);
        let ships = |fleet: &Value| rows(&fleet["ships"]).iter().map(|group| group["count"].as_u64().unwrap()).sum::<u64>();
        let damaged = |fleet: &Value| rows(&fleet["ships"]).iter().any(|group| !rows(&group["damaged"]).is_empty());
        largest = largest.max(fleets.iter().map(|fleet| ships(fleet)).max().unwrap_or(0));
        hurt += usize::from(fleets.iter().any(|fleet| damaged(fleet)));
    }
    assert_eq!(snapshot["tick"], 3000);
    let [fought, explored, raided, heated] = deeds(&snapshot);
    assert!(
        fought >= 4 && explored >= 4 && raided >= 4 && heated >= 12,
        "the world was meant to be busy: {fought} empires with combat points, {explored} with exploration points, \
         {raided} homes raided, {heated} sectors heated by a kill"
    );
    assert!(
        largest >= 8 && hurt >= 2,
        "the fights were meant to be of fleets, and to leave damage a stored world carries: the largest fleet at a \
         checkpoint was of {largest} ships, and {hurt} checkpoints of ten held a damaged ship"
    );
}

#[test]
fn a_season_day_of_sixteen_seats_is_one_world() {
    let mut twin = Twin::new_game("season", &["all:all"]);
    let mut snapshot = Value::Null;
    for _ in 0..8 {
        twin.stay_away(10_800);
        snapshot = twin.checkpoint();
    }
    assert_eq!((snapshot["tick"].as_u64(), rows(&snapshot["players"]).len()), (Some(86_400), 16));
    let [fought, explored, raided, heated] = deeds(&snapshot);
    // A season day at OGame's prices is an empire's first few Light
    // Fighters: the fighting of this world is the blitz's above.
    assert!(
        fought >= 1 && explored >= 4 && raided >= 4 && heated >= 1,
        "the world was meant to be busy: {fought} empires with combat points, {explored} with exploration points, \
         {raided} homes raided, {heated} sectors heated by a kill"
    );
}

#[test]
fn a_commanders_fight_is_the_same_rounds_on_both_targets() {
    let mut twin = Twin::new_game("test", &[]);
    let login = twin.log_in();
    let fleet = login[1]["fleets"][0]["id"].as_u64().unwrap();
    let home = hex_of(&login[1]["player"]["homeworld"]);
    let (described, _) = twin.call(json!({"op": "describe"}), &[]);
    let world = WorldGen::init(u64::from_str_radix(described["world"]["seed"].as_str().unwrap(), 16).unwrap());
    let shoots = |at: Hex| world.generate_sector(at).npc_template.is_some_and(|npc| npc.behavior != NpcBehaviorType::Passive);
    let beside = |at: Hex| world.connected_neighbors(at).slice().iter().copied().find(|&next| shoots(next));
    let approach = calm_route(&world, home, |at, _| beside(at).is_some());
    let lair = beside(*approach.last().unwrap()).unwrap();

    for hop in approach {
        twin.fly(fleet, hop);
    }
    twin.play_until(30, |f| f["type"] == "tick_update" && f["fleets"][0]["state"] == "Idle" && f["fleets"][0]["cooldown_remaining"] == 0);
    twin.accepted(json!({"action": "move", "fleet_id": fleet, "target": {"q": lair.q, "r": lair.r}}));
    let fought = twin.play_until(200, |f| !events(std::slice::from_ref(f), "CombatEnded").is_empty());

    let rounds = events(&fought, "BattleRound");
    let ended = events(&fought, "CombatEnded");
    assert!(!rounds.is_empty(), "the group at {lair} shoots first: {fought:?}");
    assert_eq!(ended[0]["rounds"].as_u64(), Some(rounds.len() as u64), "{}", ended[0]);
    let shots: u64 = rounds.iter().map(|round| round["fleets"]["shots"].as_u64().unwrap() + round["hostiles"]["shots"].as_u64().unwrap()).sum();
    assert!(shots >= 2 * rounds.len() as u64, "both sides fired in every round: {shots} shots in {} rounds", rounds.len());
    twin.checkpoint();
}

#[test]
fn a_commanders_jump_out_of_a_fight_and_the_group_it_left_are_the_same_on_both_targets() {
    let mut twin = Twin::new_game("test", &[]);
    let login = twin.log_in();
    let fleet = login[1]["fleets"][0]["id"].as_u64().unwrap();
    let home = hex_of(&login[1]["player"]["homeworld"]);
    let (described, _) = twin.call(json!({"op": "describe"}), &[]);
    let world = WorldGen::init(u64::from_str_radix(described["world"]["seed"].as_str().unwrap(), 16).unwrap());
    let shoots = |at: Hex| world.generate_sector(at).npc_template.is_some_and(|npc| npc.behavior != NpcBehaviorType::Passive);
    let beside = |at: Hex| world.connected_neighbors(at).slice().iter().copied().find(|&next| shoots(next));
    let approach = calm_route(&world, home, |at, _| beside(at).is_some());
    let back = *approach.last().unwrap();
    let lair = beside(back).unwrap();

    for hop in approach {
        twin.fly(fleet, hop);
    }
    twin.play_until(30, |f| f["type"] == "tick_update" && f["fleets"][0]["state"] == "Idle" && f["fleets"][0]["cooldown_remaining"] == 0);
    twin.accepted(json!({"action": "move", "fleet_id": fleet, "target": {"q": lair.q, "r": lair.r}}));
    let met = twin.play_until(60, |f| !events(std::slice::from_ref(f), "BattleRound").is_empty());
    let quote = &met.last().unwrap()["fleets"][0]["flight"];
    assert!(quote["rounds"].as_u64() == Some(1) && quote["refusal"].is_null(), "a fleet in a fight is quoted its flight: {quote}");

    twin.accepted(json!({"action": "move", "fleet_id": fleet, "target": {"q": back.q, "r": back.r}}));
    let left = twin.play_until(60, |f| !events(std::slice::from_ref(f), "CombatEnded").is_empty());
    let ended = events(&left, "CombatEnded");
    assert_eq!(ended[0]["outcome"], "fled", "{}", ended[0]);
    let parting: Vec<&Value> = events(&left, "BattleRound").into_iter().filter(|round| round["fleets"]["groups"][0]["fleeing"] == true).collect();
    assert_eq!(parting.len(), 1, "one round under fire: {left:?}");
    assert_eq!(parting[0]["fleets"]["shots"], 0, "in which the fleet did not fire");

    let snapshot = twin.checkpoint();
    let groups = rows(&snapshot["npc_fleets"]);
    assert_eq!(groups.len(), 1, "the group the fleet left is kept as the fight left it: {}", snapshot["npc_fleets"]);
    assert!(
        !rows(&groups[0]["ships"]).is_empty() && groups[0]["ships"] != groups[0]["muster"],
        "with its losses or its damage: {}",
        groups[0],
    );

    let mut after = Vec::new();
    for _ in 0..12 {
        after.extend(twin.tick());
    }
    assert!(events(&after, "BattleRound").is_empty(), "nothing fires on a fleet that got away: {after:?}");
    let snapshot = twin.checkpoint();
    assert!(
        rows(&snapshot["npc_fleets"]).is_empty(),
        "at this pace a group mends in ten ticks, and whole again it is its lair's template: {}",
        snapshot["npc_fleets"],
    );
}

#[test]
fn a_commander_who_trades_at_the_hub_and_founds_an_outpost_plays_one_world() {
    let amounts = |v: &Value| ["metal", "crystal", "deuterium"].map(|k| v[k].as_f64().unwrap());
    let stocked = |need: [f64; 3]| move |f: &Value| f["type"] == "tick_update" && amounts(&f["player"]["resources"]).iter().zip(need).all(|(have, need)| *have >= need);

    let mut twin = Twin::new_game("test", &["raider:hands-on", "explorer:hybrid", "turtle:check-in"]);
    let login = twin.log_in();
    let fleet = login[1]["fleets"][0]["id"].as_u64().unwrap();
    let home = hex_of(&login[1]["player"]["homeworld"]);
    let (described, _) = twin.call(json!({"op": "describe"}), &[]);
    let world = WorldGen::init(u64::from_str_radix(described["world"]["seed"].as_str().unwrap(), 16).unwrap());
    let to_hub = calm_route(&world, home, |at, _| at == Hex::ORIGIN);
    let to_site = calm_route(&world, home, |at, sector| {
        at.dist_from_origin() >= 9
            && sector.npc_template.is_none()
            && !sector.has_derelict
            && sector.terrain != TerrainType::Anomaly
            && sector.metal_density != Density::None
    });

    twin.accepted(json!({"action": "build", "building_type": "DeuteriumSynthesizer"}));
    let board = twin.order(json!({"action": "leaderboard", "limit": 10}));
    assert_eq!(board[0]["type"], "leaderboard", "{board:?}");

    twin.play_until(600, stocked([400.0, 100.0, 100.0]));
    twin.accepted(json!({"action": "load", "fleet_id": fleet, "resource": "metal", "amount": 300}));
    for hop in to_hub.iter().copied() {
        let odds = twin.order(json!({"action": "odds", "fleet_id": fleet, "sectors": world.connected_neighbors(hop).slice()}));
        assert_eq!(odds[0]["type"], "odds", "{odds:?}");
        twin.fly(fleet, hop);
    }
    let trade = json!({"fleet_id": fleet, "give": "metal", "want": "crystal", "amount": 300});
    let mut quote = trade.clone();
    quote["action"] = json!("exchange_quote");
    let quoted = twin.order(quote);
    assert_eq!((quoted[0]["type"].as_str(), quoted[0].get("refusal")), (Some("exchange_quote"), None), "{quoted:?}");
    let mut exchange = trade;
    exchange["action"] = json!("exchange");
    twin.accepted(exchange);
    let traded = twin.tick();
    let exchanged = events(&traded, "Exchanged");
    assert_eq!(exchanged.len(), 1, "{traded:?}");
    assert_eq!(exchanged[0]["receive"], quoted[0]["receive"], "the trade is the quote");
    for hop in to_hub.iter().rev().skip(1).copied().chain([home]) {
        twin.fly(fleet, hop);
    }

    twin.play_until(600, stocked([3500.0, 1900.0, 1100.0]));
    for (resource, amount) in [("metal", 3500), ("crystal", 1900), ("deuterium", 1000)] {
        twin.accepted(json!({"action": "load", "fleet_id": fleet, "resource": resource, "amount": amount}));
    }
    for hop in to_site.iter().copied() {
        twin.fly(fleet, hop);
    }
    let site = *to_site.last().unwrap();
    let survey = twin.order(json!({"action": "outpost_survey", "fleet_id": fleet}));
    assert_eq!((survey[0]["type"].as_str(), survey[0].get("refusal")), (Some("outpost_survey"), None), "{survey:?}");
    twin.accepted(json!({"action": "found_outpost", "fleet_id": fleet}));
    twin.accepted(json!({"action": "unload", "fleet_id": fleet}));
    let step_back = to_site[to_site.len() - 2];
    twin.accepted(json!({"action": "move", "fleet_id": fleet, "target": {"q": step_back.q, "r": step_back.r}}));
    let built = twin.play_until(300, |f| f["type"] == "tick_update" && f["outpost"]["level"] == 1);
    assert_eq!(events(&built, "OutpostCompleted").len(), 1);
    twin.checkpoint();

    let report = twin.stay_away(7200);
    assert_eq!(report[0]["type"], "full_state", "a session is brought up to date after an absence");
    let told: Vec<&str> = events(&report, "Alert").iter().map(|alert| alert["message"].as_str().unwrap()).collect();
    assert!(
        told.iter().any(|line| line.contains(&format!("your outpost at {site}")) && (line.contains("repelled") || line.contains("broke"))),
        "two hours of raid clock passed the outpost by: {told:?}"
    );
    let snapshot = twin.checkpoint();
    assert_eq!(rows(&snapshot["outposts"]).len(), 1, "no raid destroys an outpost");
}
