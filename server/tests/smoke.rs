// End-to-end smoke test: boots the real server binary, speaks the wire
// protocol over WebSocket, and verifies the auth → state → command loop.

use std::process::{Child, Command};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

struct ServerGuard(Child);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn connect_with_retry(
    url: &str,
) -> tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
> {
    for _ in 0..50 {
        if let Ok((ws, _)) = tokio_tungstenite::connect_async(url).await {
            return ws;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("server did not accept a connection at {url}");
}

async fn next_json<S>(ws: &mut S) -> Value
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(10), ws.next())
            .await
            .expect("timed out waiting for server message")
            .expect("connection closed")
            .expect("websocket error");
        if let Message::Text(text) = msg {
            return serde_json::from_str(&text).expect("server sent invalid JSON");
        }
    }
}

#[tokio::test]
async fn auth_state_and_command_roundtrip() {
    let port = 17931;
    let db = std::env::temp_dir().join(format!("iac_smoke_{port}.db"));
    let _ = std::fs::remove_file(&db);

    let child = Command::new(env!("CARGO_BIN_EXE_iac-server"))
        .args(["--port", &port.to_string(), "--db", db.to_str().unwrap()])
        .spawn()
        .expect("failed to spawn server");
    let _guard = ServerGuard(child);

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
    assert_eq!(state["world"]["economy_version"], 2);

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

    let _ = std::fs::remove_file(&db);
}

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn fresh_db(port: u16) -> std::path::PathBuf {
    let db = std::env::temp_dir().join(format!("iac_smoke_{port}.db"));
    remove_db(&db);
    db
}

fn remove_db(db: &std::path::Path) {
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", db.display()));
    }
}

fn start_server(port: u16, db: &std::path::Path) -> ServerGuard {
    let child = Command::new(env!("CARGO_BIN_EXE_iac-server"))
        .args(["--port", &port.to_string(), "--db", db.to_str().unwrap()])
        .spawn()
        .expect("failed to spawn server");
    ServerGuard(child)
}

/// Connect, send an auth request, and return the connection with the reply.
async fn login(port: u16, name: &str, token: Option<&str>) -> (Ws, Value) {
    let mut ws = connect_with_retry(&format!("ws://127.0.0.1:{port}")).await;
    let mut req = json!({"type": "auth", "player_name": name});
    if let Some(t) = token {
        req["token"] = json!(t);
    }
    ws.send(Message::text(req.to_string())).await.unwrap();
    let reply = next_json(&mut ws).await;
    assert_eq!(reply["type"], "auth_result", "unexpected reply: {reply}");
    (ws, reply)
}

#[tokio::test]
async fn auth_issues_a_token_and_demands_it_afterwards() {
    let port = 17932;
    let db = fresh_db(port);
    let _guard = start_server(port, &db);

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

    remove_db(&db);
}

#[tokio::test]
async fn auth_rejects_bad_names() {
    let port = 17933;
    let db = fresh_db(port);
    let _guard = start_server(port, &db);

    for bad in ["ab", "has space", "-dash", "Admin", &"x".repeat(25)] {
        let (_ws, reply) = login(port, bad, None).await;
        assert_eq!(reply["success"], false, "{bad:?} accepted");
        assert_eq!(reply["code"], "InvalidName", "{bad:?}: {reply}");
        assert!(reply["message"].as_str().unwrap().starts_with("invalid player name"));
    }
    remove_db(&db);
}

#[tokio::test]
async fn token_survives_restart_and_pre_token_databases_are_claimed() {
    let port = 17934;
    let db = fresh_db(port);

    // Run 1: register, let the persist thread commit it.
    let (player_id, token) = {
        let _guard = start_server(port, &db);
        let (_ws, reply) = login(port, "OldTimer", None).await;
        assert_eq!(reply["success"], true, "{reply}");
        tokio::time::sleep(Duration::from_millis(2500)).await;
        (reply["player_id"].as_u64().unwrap(), reply["token"].as_str().unwrap().to_string())
    };

    // Run 2: the hash was stored, so the token still works and is still required.
    {
        let _guard = start_server(port, &db);
        let (_ws, missing) = login(port, "OldTimer", None).await;
        assert_eq!(missing["code"], "TokenRequired", "{missing}");
        let (_ws, ok) = login(port, "OldTimer", Some(&token)).await;
        assert_eq!(ok["success"], true, "{ok}");
    }

    // Turn the database into one written before tokens existed.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("ALTER TABLE players DROP COLUMN token_hash").unwrap();
    }

    // Run 3: the column is migrated back in, and the first login claims the account.
    let claimed = {
        let _guard = start_server(port, &db);
        let (mut ws, reply) = login(port, "OldTimer", None).await;
        assert_eq!(reply["success"], true, "legacy account not claimable: {reply}");
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
        tokio::time::sleep(Duration::from_millis(2500)).await;
        new_token
    };

    // Run 4: the claim itself was persisted.
    {
        let _guard = start_server(port, &db);
        let (_ws, stale) = login(port, "OldTimer", Some(&token)).await;
        assert_eq!(stale["code"], "InvalidToken", "old token must not work after a claim: {stale}");
        let (_ws, ok) = login(port, "OldTimer", Some(&claimed)).await;
        assert_eq!(ok["success"], true, "{ok}");
    }

    remove_db(&db);
}

#[tokio::test]
async fn a_world_refuses_a_different_pace() {
    let port = 17935;
    let db = fresh_db(port);
    {
        let child = Command::new(env!("CARGO_BIN_EXE_iac-server"))
            .args(["--port", &port.to_string(), "--db", db.to_str().unwrap(), "--pace", "blitz"])
            .spawn()
            .expect("failed to spawn server");
        let _guard = ServerGuard(child);
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
    }

    let out = Command::new(env!("CARGO_BIN_EXE_iac-server"))
        .args(["--port", &port.to_string(), "--db", db.to_str().unwrap(), "--pace", "10"])
        .output()
        .expect("failed to run server");
    assert!(!out.status.success(), "a different pace must stop the server");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("cannot change") && err.contains("600 (blitz)"), "unclear refusal: {err}");

    remove_db(&db);
}

#[tokio::test]
async fn leaderboard_ranks_humans_and_agents_together() {
    let port = 17936;
    let db = fresh_db(port);
    let _guard = start_server(port, &db);

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

    remove_db(&db);
}
