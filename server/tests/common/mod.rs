use std::process::{Child, Command};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

pub struct ServerGuard(Child);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub async fn connect_with_retry(
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

pub async fn next_json<S>(ws: &mut S) -> Value
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


pub type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub fn fresh_db(port: u16) -> std::path::PathBuf {
    let db = std::env::temp_dir().join(format!("iac_smoke_{port}.db"));
    remove_db(&db);
    db
}

pub fn remove_db(db: &std::path::Path) {
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", db.display()));
    }
}

pub fn start_server(port: u16, db: &std::path::Path) -> ServerGuard {
    let child = Command::new(env!("CARGO_BIN_EXE_iac-server"))
        .args(["--port", &port.to_string(), "--db", db.to_str().unwrap()])
        .spawn()
        .expect("failed to spawn server");
    ServerGuard(child)
}

/// Connect, send an auth request, and return the connection with the reply.
pub async fn login(port: u16, name: &str, token: Option<&str>) -> (Ws, Value) {
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

