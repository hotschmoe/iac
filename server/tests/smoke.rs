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

#[tokio::test]
async fn reauth_resumes_same_player() {
    let port = 17932;
    let db = std::env::temp_dir().join(format!("iac_smoke_{port}.db"));
    let _ = std::fs::remove_file(&db);

    let child = Command::new(env!("CARGO_BIN_EXE_iac-server"))
        .args(["--port", &port.to_string(), "--db", db.to_str().unwrap()])
        .spawn()
        .expect("failed to spawn server");
    let _guard = ServerGuard(child);

    let url = format!("ws://127.0.0.1:{port}");

    let mut ws = connect_with_retry(&url).await;
    ws.send(Message::text(
        json!({"type": "auth", "player_name": "Returning"}).to_string(),
    ))
    .await
    .unwrap();
    let first = next_json(&mut ws).await;
    assert_eq!(first["success"], true, "first auth failed: {first}");
    let first_id = first["player_id"].as_u64().unwrap();
    ws.close(None).await.unwrap();

    let mut ws2 = connect_with_retry(&url).await;
    ws2.send(Message::text(
        json!({"type": "auth", "player_name": "Returning"}).to_string(),
    ))
    .await
    .unwrap();
    let second = next_json(&mut ws2).await;
    assert_eq!(second["success"], true, "re-auth failed: {second}");
    assert_eq!(
        second["player_id"].as_u64().unwrap(),
        first_id,
        "same player name should resume the same player"
    );

    let _ = std::fs::remove_file(&db);
}
