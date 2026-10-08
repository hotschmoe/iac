// Static hosting + WebSocket on one port: boots the real server binary with a
// temp web dir and checks HTTP responses and WS upgrades at `/` and `/ws`.

use std::process::{Child, Command};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;

struct ServerGuard(Child);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start(port: u16, with_web: bool) -> (ServerGuard, std::path::PathBuf) {
    let base = std::env::temp_dir().join(format!("iac_web_test_{port}"));
    let _ = std::fs::remove_dir_all(&base);
    let web = base.join("web");
    std::fs::create_dir_all(web.join("assets")).unwrap();
    std::fs::write(web.join("index.html"), "<html>INDEX</html>").unwrap();
    std::fs::write(web.join("main.dart.wasm"), [0u8, 0x61, 0x73, 0x6d]).unwrap();
    std::fs::write(web.join("main.dart.mjs"), "export {};").unwrap();
    std::fs::write(web.join("assets/AssetManifest.json"), "{}").unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_iac-server"));
    cmd.args(["--port", &port.to_string(), "--db", base.join("w.db").to_str().unwrap()]);
    if with_web {
        cmd.args(["--web-dir", web.to_str().unwrap()]);
    }
    (ServerGuard(cmd.spawn().expect("spawn server")), base)
}

/// Minimal HTTP/1.1 GET; returns (status, lowercased headers, body).
async fn http_get(port: u16, path: &str) -> (u16, String, String) {
    for _ in 0..50 {
        if let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)).await {
            let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
            s.write_all(req.as_bytes()).await.unwrap();
            let mut buf = Vec::new();
            s.read_to_end(&mut buf).await.unwrap();
            let text = String::from_utf8_lossy(&buf).to_string();
            let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
            let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
            return (status, head.to_lowercase(), body.to_string());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("server did not accept a connection");
}

#[tokio::test]
async fn serves_static_files_with_spa_fallback() {
    let port = 17941;
    let (_g, _dir) = start(port, true);

    let (status, head, body) = http_get(port, "/").await;
    assert_eq!(status, 200);
    assert!(body.contains("INDEX"), "body: {body}");
    assert!(head.contains("cross-origin-opener-policy: same-origin"), "{head}");
    assert!(head.contains("cross-origin-embedder-policy: require-corp"), "{head}");
    assert!(head.contains("cache-control: no-cache"), "{head}");

    let (status, head, _) = http_get(port, "/main.dart.wasm").await;
    assert_eq!(status, 200);
    assert!(head.contains("content-type: application/wasm"), "{head}");

    let (_, head, _) = http_get(port, "/main.dart.mjs").await;
    assert!(head.contains("javascript"), "{head}");
    let (_, head, _) = http_get(port, "/assets/AssetManifest.json").await;
    assert!(head.contains("application/json"), "{head}");

    // Deep link (not a file) falls back to index.html with 200.
    let (status, _, body) = http_get(port, "/some/deep/link").await;
    assert_eq!(status, 200);
    assert!(body.contains("INDEX"), "body: {body}");
}

#[tokio::test]
async fn no_web_dir_returns_404_for_http() {
    let port = 17942;
    let (_g, _dir) = start(port, false);
    // Run from a CWD without clients/web/build/web (cargo test CWD is server/).
    let (status, _, _) = http_get(port, "/").await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn websocket_at_ws_and_legacy_root() {
    let port = 17943;
    let (_g, _dir) = start(port, true);
    http_get(port, "/").await; // wait until up

    for (i, path) in ["/ws", "", "/"].iter().enumerate() {
        let url = format!("ws://127.0.0.1:{port}{path}");
        let (mut ws, _) = tokio_tungstenite::connect_async(&url).await
            .unwrap_or_else(|e| panic!("connect {url}: {e}"));
        let name = format!("WebTester{i}");
        ws.send(Message::text(json!({"type": "auth", "player_name": name}).to_string()))
            .await
            .unwrap();
        let mut got_auth = false;
        let mut got_state = false;
        for _ in 0..20 {
            let msg = tokio::time::timeout(Duration::from_secs(10), ws.next())
                .await
                .expect("timeout")
                .expect("closed")
                .expect("ws error");
            let Message::Text(t) = msg else { continue };
            let v: Value = serde_json::from_str(&t).unwrap();
            match v["type"].as_str() {
                Some("auth_result") => {
                    assert_eq!(v["success"], true, "{v}");
                    got_auth = true;
                    ws.send(Message::text(json!({"type": "request_full_state"}).to_string()))
                        .await
                        .unwrap();
                }
                Some("full_state") => {
                    got_state = true;
                    break;
                }
                _ => {}
            }
        }
        assert!(got_auth && got_state, "path {path:?}: auth={got_auth} state={got_state}");
    }
}
