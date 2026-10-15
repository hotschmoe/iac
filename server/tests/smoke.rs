// End-to-end smoke test: boots the real server binary, speaks the wire
// protocol over WebSocket, and verifies the auth → state → command loop.

mod common;

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use common::*;
use futures_util::SinkExt;
use iac_server::store::Beside;
use iac_shared::constants::ECONOMY_VERSION;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn auth_state_and_command_roundtrip() {
    let file = fresh_world("smoke_roundtrip");
    let (_guard, port) = start_server(&file);

    let mut ws = connect_with_retry(&format!("ws://127.0.0.1:{port}")).await;

    // Authenticate as a new player.
    let auth = json!({"type": "auth", "player_name": "SmokeTester"});
    ws.send(Message::text(auth.to_string())).await.unwrap();

    let reply = next_json(&mut ws).await;
    assert_eq!(reply["type"], "auth_result", "unexpected reply: {reply}");
    assert_eq!(reply["success"], true, "auth failed: {reply}");
    assert!(reply["player_id"].is_u64(), "no player_id: {reply}");

    // Request full state and verify the player exists in it.
    ws.send(Message::text(json!({"type": "request_full_state"}).to_string()))
        .await
        .unwrap();
    let state = loop {
        let msg = next_json(&mut ws).await;
        if msg["type"] == "full_state" {
            break msg;
        }
    };
    assert!(
        state["player"].is_object() || state["players"].is_array(),
        "full_state has no player data: {state}"
    );
    assert_eq!(state["world"]["pace"], 1.0, "default world is persistent: {state}");
    assert_eq!(state["world"]["preset"], "persistent");
    assert_eq!(state["world"]["economy_version"], json!(ECONOMY_VERSION));

    // Server should stream tick updates (1 Hz tick loop).
    let tick = loop {
        let msg = next_json(&mut ws).await;
        if msg["type"] == "tick_update" {
            break msg;
        }
    };
    assert!(tick["tick"].is_u64(), "tick_update missing tick: {tick}");

    // Active scan from the starting fleet: expect a scan_completed event
    // and revealed sectors in a subsequent tick update.
    let fleet_id = state["fleets"][0]["id"]
        .as_u64()
        .expect("full_state has no starting fleet");
    let scan = json!({"type": "command", "action": "scan", "fleet_id": fleet_id});
    ws.send(Message::text(scan.to_string())).await.unwrap();
    let mut scan_event: Option<Value> = None;
    for _ in 0..8 {
        let msg = next_json(&mut ws).await;
        if msg["type"] != "tick_update" {
            continue;
        }
        if let Some(events) = msg["events"].as_array()
            && let Some(e) = events.iter().find(|e| e["kind"] == "ScanCompleted")
        {
            scan_event = Some(e.clone());
            break;
        }
    }
    let scan_event = scan_event.expect("no ScanCompleted event after scan command");
    assert!(
        scan_event["sectors_revealed"].as_u64().unwrap() >= 1,
        "scan revealed nothing: {scan_event}"
    );

    // A malformed scan (unknown fleet) must return an error, not kill the
    // connection.
    let bad_scan = json!({"type": "command", "action": "scan", "fleet_id": 999_999});
    ws.send(Message::text(bad_scan.to_string())).await.unwrap();
    let after = loop {
        let msg = next_json(&mut ws).await;
        if msg["type"] == "tick_update" {
            break msg;
        }
    };
    assert!(after["tick"].as_u64().unwrap() >= tick["tick"].as_u64().unwrap());

    remove_world(&file);
}

#[tokio::test]
async fn auth_issues_a_token_and_demands_it_afterwards() {
    let file = fresh_world("smoke_token");
    let (_guard, port) = start_server(&file);

    let (_ws, first) = login(port, "Returning", None).await;
    assert_eq!(first["success"], true, "registration failed: {first}");
    let token = first["token"].as_str().expect("first login returns the token").to_string();
    assert_eq!(token.len(), 64);
    assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    let player_id = first["player_id"].as_u64().unwrap();

    // Same name, no token: refused with a code and readable text.
    let (_ws, missing) = login(port, "Returning", None).await;
    assert_eq!(missing["success"], false);
    assert_eq!(missing["code"], "TokenRequired");
    assert!(missing.get("token").is_none() && missing.get("player_id").is_none(), "no data leaks on failure: {missing}");
    let text = missing["message"].as_str().unwrap();
    assert!(text.contains("Returning") && !text.contains("::") && !text.contains('{'), "message not human readable: {text}");

    // Wrong token.
    let (_ws, wrong) = login(port, "Returning", Some(&"0".repeat(64))).await;
    assert_eq!(wrong["success"], false);
    assert_eq!(wrong["code"], "InvalidToken");

    // The right token resumes the same empire and does not re-issue itself.
    let (mut ws, again) = login(port, "Returning", Some(&token)).await;
    assert_eq!(again["success"], true, "valid token refused: {again}");
    assert_eq!(again["player_id"].as_u64().unwrap(), player_id);
    assert!(again.get("token").is_none(), "token must be shown only once: {again}");
    let state = loop {
        let msg = next_json(&mut ws).await;
        if msg["type"] == "full_state" {
            break msg;
        }
    };
    assert_eq!(state["player"]["id"].as_u64().unwrap(), player_id);

    // A failed login must not have opened the session to commands.
    let (mut ws, refused) = login(port, "Returning", None).await;
    assert_eq!(refused["success"], false);
    ws.send(Message::text(json!({"type": "request_full_state"}).to_string())).await.unwrap();
    ws.send(Message::text(json!({"type": "command", "action": "scan", "fleet_id": 1}).to_string())).await.unwrap();
    let err = loop {
        let msg = next_json(&mut ws).await;
        if msg["type"] == "error" {
            break msg;
        }
        assert_ne!(msg["type"], "full_state", "unauthenticated session got state");
    };
    assert_eq!(err["code"], "AuthFailed");

    remove_world(&file);
}

#[tokio::test]
async fn auth_rejects_bad_names() {
    let file = fresh_world("smoke_names");
    let (_guard, port) = start_server(&file);

    for bad in ["ab", "has space", "-dash", "Admin", &"x".repeat(25)] {
        let (_ws, reply) = login(port, bad, None).await;
        assert_eq!(reply["success"], false, "{bad:?} accepted");
        assert_eq!(reply["code"], "InvalidName", "{bad:?}: {reply}");
        assert!(reply["message"].as_str().unwrap().starts_with("invalid player name"));
    }
    remove_world(&file);
}

#[tokio::test]
async fn a_token_outlives_a_killed_server_and_an_account_without_one_is_claimed() {
    let file = fresh_world("smoke_restart");

    // Run 1: register, and kill the server a few seconds later. It writes
    // its world every five minutes; the registration did not wait for that.
    let (player_id, token) = {
        let (server, port) = start_server(&file);
        let (_ws, reply) = login(port, "OldTimer", None).await;
        assert_eq!(reply["success"], true, "{reply}");
        tokio::time::sleep(REGISTRATION_IS_STORED).await;
        server.kill();
        (reply["player_id"].as_u64().unwrap(), reply["token"].as_str().unwrap().to_string())
    };

    // Run 2: the account was stored, so the token still works and is still required.
    {
        let (server, port) = start_server(&file);
        let (_ws, missing) = login(port, "OldTimer", None).await;
        assert_eq!(missing["code"], "TokenRequired", "{missing}");
        let (_ws, ok) = login(port, "OldTimer", Some(&token)).await;
        assert_eq!(ok["success"], true, "{ok}");
        server.stop();
    }

    // An account nobody holds a token for: what the commander of a world
    // brought here from a single-player save would be.
    {
        let mut world = stored_world(&file);
        world.players.get_mut(&player_id).expect("the account is in the file").token_hash = None;
        std::fs::write(&file, iac_sim::store::encode(&Beside {}, &world)).unwrap();
    }

    // Run 3: the first login claims the account.
    let claimed = {
        let (server, port) = start_server(&file);
        let (mut ws, reply) = login(port, "OldTimer", None).await;
        assert_eq!(reply["success"], true, "an account without a token is not claimable: {reply}");
        assert_eq!(reply["player_id"].as_u64().unwrap(), player_id, "claim must keep the same empire");
        let new_token = reply["token"].as_str().expect("claim returns a token").to_string();
        assert_ne!(new_token, token);
        let state = loop {
            let msg = next_json(&mut ws).await;
            if msg["type"] == "full_state" {
                break msg;
            }
        };
        assert!(!state["fleets"].as_array().unwrap().is_empty(), "claimed account kept its fleet");

        // From now on it is protected like any other.
        let (_ws, second) = login(port, "OldTimer", None).await;
        assert_eq!(second["code"], "TokenRequired");
        tokio::time::sleep(REGISTRATION_IS_STORED).await;
        server.kill();
        new_token
    };

    // Run 4: the claim issued a token too, and was stored as soon.
    {
        let (_guard, port) = start_server(&file);
        let (_ws, stale) = login(port, "OldTimer", Some(&token)).await;
        assert_eq!(stale["code"], "InvalidToken", "old token must not work after a claim: {stale}");
        let (_ws, ok) = login(port, "OldTimer", Some(&claimed)).await;
        assert_eq!(ok["success"], true, "{ok}");
    }

    remove_world(&file);
}

/// What a server that will not start on `file` says, and that it left the
/// file as it was.
fn refusal(file: &std::path::Path, more: &[&str]) -> String {
    let before = std::fs::read(file).expect("the world file");
    let mut server = Command::new(env!("CARGO_BIN_EXE_iac-server"))
        .args(["--port", "0", "--world", file.to_str().unwrap()])
        .args(more)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run server");
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = server.try_wait().expect("the server could not be waited for") {
            break status;
        }
        if Instant::now() > deadline {
            let _ = server.kill();
            panic!("the server started on a world it had to refuse ({more:?})");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(2), "a refusal is exit code 2 ({more:?})");
    assert_eq!(std::fs::read(file).unwrap(), before, "a refused world file is left as it was");
    let mut said = String::new();
    server.stderr.take().expect("piped stderr").read_to_string(&mut said).unwrap();
    said
}

#[tokio::test]
async fn a_world_keeps_the_pace_and_the_seed_it_was_created_with() {
    let file = fresh_world("smoke_pace");
    {
        let (server, port) = start_server_with(&file, &["--pace", "blitz", "--seed", "4242"]);
        let (mut ws, reply) = login(port, "Pacer", None).await;
        assert_eq!(reply["success"], true, "{reply}");
        let state = loop {
            let msg = next_json(&mut ws).await;
            if msg["type"] == "full_state" {
                break msg;
            }
        };
        assert_eq!(state["world"]["pace"], 600.0);
        assert_eq!(state["world"]["preset"], "blitz");
        assert!(state["world"]["estimate"]["first_cruiser_s"].is_u64());
        server.kill();
    }
    let created = stored_world(&file);
    assert_eq!((created.world_seed, created.meta.pace.value()), (4242, 600.0), "a world is in its file from its first second");

    let other_pace = refusal(&file, &["--pace", "10"]);
    assert!(
        other_pace.contains("cannot change") && other_pace.contains("600 (blitz)") && other_pace.contains("--pace 10 (season)"),
        "unclear refusal: {other_pace}",
    );
    let other_seed = refusal(&file, &["--seed", "7"]);
    assert!(
        other_seed.contains("created with seed 4242 and cannot change; refusing --seed 7") && other_seed.contains("new --world path"),
        "unclear refusal: {other_seed}",
    );

    let mut played = created.tick;
    for flags in [&[][..], &["--pace", "600", "--seed", "4242"][..]] {
        let (server, port) = start_server_with(&file, flags);
        let (_ws, reply) = login(port, &format!("Later{}", flags.len()), None).await;
        assert_eq!(reply["success"], true, "{reply}");
        server.stop();
        let kept = stored_world(&file);
        assert_eq!((kept.world_seed, kept.meta.pace.value()), (4242, 600.0), "started with {flags:?}");
        assert!(kept.tick > played, "started with {flags:?}, the world went on from tick {played}: {}", kept.tick);
        played = kept.tick;
    }
    remove_world(&file);
}

#[tokio::test]
async fn a_server_refuses_a_world_another_holds_or_this_build_does_not_play() {
    let file = fresh_world("smoke_refusals");
    let (server, _port) = start_server(&file);
    let taken = refusal(&file, &[]);
    assert!(taken.contains("another server is running on") && taken.contains("world.iac.lock"), "unclear refusal: {taken}");
    server.stop();

    let document = std::fs::read(&file).unwrap();
    let text = String::from_utf8(document.clone()).unwrap();
    let write = |bytes: &[u8]| std::fs::write(&file, bytes).unwrap();

    write(text.replacen(&format!("\"economy_version\":{ECONOMY_VERSION}"), "\"economy_version\":4", 1).as_bytes());
    let old_economy = refusal(&file, &[]);
    assert!(
        old_economy.contains(&format!(
            "this world file is economy version 4, this build runs version {ECONOMY_VERSION}; old worlds are not migrated"
        )) && old_economy.contains("new --world path"),
        "unclear refusal: {old_economy}",
    );
    write(text.replacen("\"worldgen_version\":6", "\"worldgen_version\":1", 1).as_bytes());
    assert!(refusal(&file, &[]).contains("worldgen version 1, this build runs version 6"));
    write(text.replacen("\"format\":16", "\"format\":15", 1).as_bytes());
    assert!(refusal(&file, &[]).contains("this world file is format 15, this build reads format 16"));

    write(&document[..document.len() - 40]);
    let cut_short = refusal(&file, &[]);
    assert!(cut_short.contains("cannot be read: its contents do not match its checksum"), "unclear refusal: {cut_short}");
    write(b"");
    assert!(refusal(&file, &[]).contains("cannot be read: it is not a world document"));

    write(&document);
    let (server, _port) = start_server(&file);
    server.stop();
    remove_world(&file);
}

/// The fleets of the session's player in the next full state it is sent.
async fn fleets_now(ws: &mut Ws) -> (u64, Value) {
    ws.send(Message::text(json!({"type": "request_full_state"}).to_string())).await.unwrap();
    loop {
        let msg = next_json(ws).await;
        if msg["type"] == "full_state" {
            return (msg["tick"].as_u64().unwrap(), msg["fleets"].clone());
        }
    }
}

#[tokio::test]
async fn a_killed_server_comes_back_as_its_last_document_says_and_a_stopped_one_as_it_stopped() {
    const PERIOD: u64 = 3;
    let file = fresh_world("smoke_kill");
    let hold = |fleets: &Value| (fleets[0]["cargo"]["metal"].as_f64(), fleets[0]["laden"].as_bool());

    // An order, then three periods of ticks, then a kill. By then the start
    // and the registration, which each write the file whatever the period,
    // are too far back to pass for the document a period promises.
    let (token, killed_at) = {
        let (server, port) = start_server_with(&file, &["--save-every", &PERIOD.to_string()]);
        let (mut ws, reply) = login(port, "Survivor", None).await;
        let token = reply["token"].as_str().unwrap().to_string();
        let (_, fleets) = fleets_now(&mut ws).await;
        let load = json!({"type": "command", "action": "load", "fleet_id": fleets[0]["id"], "resource": "metal", "amount": 25});
        ws.send(Message::text(load.to_string())).await.unwrap();
        let (mut loaded_at, mut seen) = (None, 0);
        while loaded_at.is_none_or(|at| seen < at + 3 * PERIOD + 2) {
            let msg = next_json(&mut ws).await;
            assert_ne!(msg["type"], "error", "{msg}");
            if msg["type"] == "tick_update" {
                seen = msg["tick"].as_u64().unwrap();
                if hold(&msg["fleets"]) == (Some(25.0), Some(true)) {
                    loaded_at.get_or_insert(seen);
                }
            }
        }
        server.kill();
        (token, seen)
    };

    // It comes back no more than the period behind, with what was ordered
    // more than a period before the kill, and plays on.
    let stopped_at = {
        let back = stored_world(&file).tick;
        assert!(
            back <= killed_at + 2 && back + PERIOD + 3 >= killed_at,
            "killed about tick {killed_at} writing every {PERIOD} s, the file holds tick {back}",
        );
        let (server, port) = start_server(&file);
        let (mut ws, reply) = login(port, "Survivor", Some(&token)).await;
        assert_eq!(reply["success"], true, "{reply}");
        let (now, fleets) = fleets_now(&mut ws).await;
        assert!(now > back, "the world plays on from its document: tick {now} after {back}");
        assert_eq!(hold(&fleets), (Some(25.0), Some(true)), "the hold loaded before the last document is aboard: {fleets}");

        // An order, and a clean stop right behind it: nothing waits for a period.
        let unload = json!({"type": "command", "action": "unload", "fleet_id": fleets[0]["id"]});
        ws.send(Message::text(unload.to_string())).await.unwrap();
        let emptied_at = loop {
            let msg = next_json(&mut ws).await;
            assert_ne!(msg["type"], "error", "{msg}");
            if msg["type"] == "tick_update" && hold(&msg["fleets"]) == (Some(0.0), Some(false)) {
                break msg["tick"].as_u64().unwrap();
            }
        };
        server.stop();
        emptied_at
    };

    let stopped = stored_world(&file);
    assert!(stopped.tick >= stopped_at, "stopped after tick {stopped_at}, the file holds tick {}", stopped.tick);
    let (server, port) = start_server(&file);
    let (mut ws, _) = login(port, "Survivor", Some(&token)).await;
    let (now, fleets) = fleets_now(&mut ws).await;
    assert!(now > stopped.tick, "tick {now} after a stop at {}", stopped.tick);
    assert_eq!(hold(&fleets), (Some(0.0), Some(false)), "the order given a moment before the stop was stored: {fleets}");
    server.stop();
    remove_world(&file);
}

#[tokio::test]
async fn leaderboard_ranks_humans_and_agents_together() {
    let file = fresh_world("smoke_leaderboard");
    let (_guard, port) = start_server(&file);

    let (_human, reply) = login(port, "Person", None).await;
    assert_eq!(reply["success"], true);

    let mut ws = connect_with_retry(&format!("ws://127.0.0.1:{port}")).await;
    let auth = json!({"type": "auth", "player_name": "Botty", "agent": true});
    ws.send(Message::text(auth.to_string())).await.unwrap();
    assert_eq!(next_json(&mut ws).await["success"], true);

    ws.send(Message::text(json!({"type": "command", "action": "leaderboard", "limit": 5}).to_string())).await.unwrap();
    let board = loop {
        let msg = next_json(&mut ws).await;
        if msg["type"] == "leaderboard" {
            break msg;
        }
    };
    let entries = board["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2, "{board}");
    let bot = entries.iter().find(|e| e["name"] == "Botty").unwrap();
    let person = entries.iter().find(|e| e["name"] == "Person").unwrap();
    assert_eq!(bot["agent"], true);
    assert_eq!(person["agent"], false);
    assert_eq!(bot["score"], person["score"], "same start, same formula, one table");
    assert_eq!(entries[0]["rank"], 1);
    assert!(board.get("you").is_none(), "{board}");

    remove_world(&file);
}

#[tokio::test]
async fn two_servers_started_at_once_each_listen_on_a_port_of_their_own() {
    let (slow_file, fast_file) = (fresh_world("smoke_pair_slow"), fresh_world("smoke_pair_fast"));
    let (_slow, slow_port) = start_server(&slow_file);
    let (_fast, fast_port) = start_server_with(&fast_file, &["--pace", "blitz"]);
    assert_ne!(slow_port, fast_port);

    for (port, pace) in [(slow_port, 1.0), (fast_port, 600.0)] {
        let (mut ws, reply) = login(port, "Twin", None).await;
        assert_eq!(reply["success"], true, "each world registers the name for itself: {reply}");
        let state = loop {
            let msg = next_json(&mut ws).await;
            if msg["type"] == "full_state" {
                break msg;
            }
        };
        assert_eq!(state["world"]["pace"], pace, "port {port} is the other world");
    }

    remove_world(&slow_file);
    remove_world(&fast_file);
}
