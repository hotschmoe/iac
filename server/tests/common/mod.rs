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

/// The terminal client's binary, built into the directory the server's is
/// in. Cargo builds a package's binaries for that package's own tests only,
/// so the client's is built here, with the cargo and the profile of this
/// test run.
fn client_binary() -> PathBuf {
    let server = Path::new(env!("CARGO_BIN_EXE_iac-server"));
    let profile = server.parent().expect("the server binary stands in a profile directory");
    let mut build = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    build.args(["build", "-p", "iac-client", "--target-dir"]).arg(profile.parent().expect("the target directory"));
    if profile.ends_with("release") {
        build.arg("--release");
    }
    assert!(build.status().expect("cargo did not run").success(), "iac-client did not build");
    profile.join(format!("iac-client{}", std::env::consts::EXE_SUFFIX))
}

/// `iac-client --headless` logged in to the server on `port`: the pipe an
/// agent plays through. Lines written to it are commands; the lines it
/// prints are the server's messages, and its own `client_error` for a line
/// it could not read.
pub struct HeadlessClient {
    child: Child,
    stdin: std::process::ChildStdin,
    lines: mpsc::Receiver<Value>,
}

impl HeadlessClient {
    /// `config` is where the client may keep its token file, so that no
    /// test reads or writes the user's own.
    pub fn start(port: u16, name: &str, token: &str, config: &Path) -> Self {
        let mut child = Command::new(client_binary())
            .args(["--headless", "--host", "127.0.0.1", "--port", &port.to_string(), "--name", name, "--token", token])
            .env("XDG_CONFIG_HOME", config)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("failed to spawn the headless client");
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        let (said, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in stdout.lines().map_while(Result::ok) {
                let message = serde_json::from_str(&line).unwrap_or_else(|e| panic!("the client printed a line that is not JSON ({e}): {line}"));
                if said.send(message).is_err() {
                    break;
                }
            }
        });
        Self { child, stdin, lines }
    }

    pub fn say(&mut self, line: &str) {
        use std::io::Write;
        writeln!(self.stdin, "{line}").expect("the client took the line");
        self.stdin.flush().expect("the client took the line");
    }

    pub fn next(&self) -> Value {
        self.lines.recv_timeout(Duration::from_secs(10)).expect("the headless client printed nothing for ten seconds")
    }

    /// The next printed message `done` accepts.
    pub fn until(&self, done: impl Fn(&Value) -> bool) -> Value {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            let message = self.next();
            if done(&message) {
                return message;
            }
            assert!(std::time::Instant::now() < deadline, "never printed the message waited for; last: {message}");
        }
    }
}

impl Drop for HeadlessClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

