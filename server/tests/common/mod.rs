#![allow(dead_code)]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
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

/// An empty directory of this test's own, named for the test and for this
/// process, so no other checkout or run of the tests shares it.
pub fn fresh_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("iac_{test}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temporary directory");
    dir
}

pub fn fresh_db(test: &str) -> PathBuf {
    fresh_dir(test).join("world.db")
}

pub fn remove_db(db: &Path) {
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", db.display()));
    }
    if let Some(dir) = db.parent() {
        let _ = std::fs::remove_dir(dir);
    }
}

pub fn start_server(db: &Path) -> (ServerGuard, u16) {
    start_server_with(db, &[])
}

/// Start the server on a port the system picks and return it with that
/// port, read from the line the server prints once it listens.
pub fn start_server_with(db: &Path, more: &[&str]) -> (ServerGuard, u16) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_iac-server"))
        .args(["--port", "0", "--db", db.to_str().unwrap()])
        .args(more)
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn server");
    let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
    let guard = ServerGuard(child);
    let (said, hears) = mpsc::channel();
    std::thread::spawn(move || {
        for line in stdout.lines().map_while(Result::ok) {
            let port = line.strip_prefix("listening on ").and_then(|addr| addr.rsplit(':').next()?.parse::<u16>().ok());
            if let Some(port) = port {
                let _ = said.send(port);
            }
        }
    });
    let port = hears.recv_timeout(Duration::from_secs(30)).expect("the server did not say where it listens");
    (guard, port)
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

