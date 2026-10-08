// Headless NDJSON mode: the game as a pipe, so LLM agents (or scripts,
// or `jq`) can play without a terminal.
//
//   stdout — every server message, one JSON object per line, verbatim.
//   stdin  — one JSON command per line. Accepts either a full client
//            message ({"type":"command","action":"move",...}) or a bare
//            command ({"action":"move",...}), which gets wrapped.
//
// Auth happens automatically from --name/--token (or the saved token file);
// the session ends when stdin closes or the server hangs up. A refused login
// prints the auth_result on stdout, the reason on stderr, and exits non-zero.

use std::io::Write;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_tungstenite::tungstenite::protocol::Message;

use iac_shared::protocol::{ClientMessage, Command};

use crate::connection::{self, ConnectionConfig};

pub async fn run(
    config: &ConnectionConfig,
    name: &str,
    token: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (conn, auth_line) = match connection::login(config, name, token).await {
        Ok(ok) => ok,
        Err(e) => {
            if let Some(refused) = e.downcast_ref::<connection::LoginRefused>() {
                println!("{}", refused.raw);
            }
            return Err(e);
        }
    };
    println!("{auth_line}");

    let conn_tx = conn.tx.clone();

    // Server → stdout task.
    let printer = tokio::spawn(async move {
        loop {
            let msg = {
                let mut rx = conn.rx.lock().await;
                rx.recv().await
            };
            match msg {
                Some(text) => {
                    let mut out = std::io::stdout().lock();
                    if writeln!(out, "{}", text).and_then(|_| out.flush()).is_err() {
                        break; // stdout closed — reader is gone
                    }
                }
                None => {
                    // Connection closed: emit a final marker so agents can
                    // distinguish "server gone" from "quiet".
                    let mut out = std::io::stdout().lock();
                    let _ = writeln!(out, r#"{{"type":"connection_closed"}}"#);
                    let _ = out.flush();
                    break;
                }
            }
        }
    });

    // stdin → server loop.
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    loop {
        tokio::select! {
            line = lines.next_line() => {
                match line? {
                    Some(line) => {
                        let line = line.trim();
                        if line.is_empty() { continue; }
                        match parse_line(line) {
                            Ok(msg) => {
                                let json = serde_json::to_string(&msg)?;
                                if conn_tx.send(Message::Text(json.into())).is_err() {
                                    break; // connection writer gone
                                }
                            }
                            Err(e) => {
                                // Report bad input on stdout in-band, same
                                // shape as server errors.
                                let mut out = std::io::stdout().lock();
                                let _ = writeln!(
                                    out,
                                    r#"{{"type":"client_error","message":{}}}"#,
                                    serde_json::to_string(&format!("unparseable input: {e}"))?
                                );
                                let _ = out.flush();
                            }
                        }
                    }
                    None => break, // EOF: agent is done
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)), if printer.is_finished() => {
                break; // connection closed underneath us
            }
        }
    }

    printer.abort();
    Ok(())
}

/// Accept a full ClientMessage (has a `type` field) or a bare Command (has
/// an `action` field) and normalize to the former. The error is the one for
/// the shape the line claims to be, so a bad field is named as itself.
fn parse_line(line: &str) -> Result<ClientMessage, String> {
    let value: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
    let Some(object) = value.as_object() else {
        return Err("expected a JSON object".to_string());
    };
    if object.contains_key("type") {
        serde_json::from_value::<ClientMessage>(value).map_err(|e| e.to_string())
    } else if object.contains_key("action") {
        serde_json::from_value::<Command>(value)
            .map(ClientMessage::Command)
            .map_err(|e| e.to_string())
    } else {
        Err("expected a \"type\" field (full message) or an \"action\" field (bare command)".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_client_message() {
        let msg = parse_line(r#"{"type":"request_full_state"}"#).unwrap();
        assert!(matches!(msg, ClientMessage::RequestFullState));
    }

    #[test]
    fn wraps_bare_command() {
        let msg = parse_line(r#"{"action":"scan","fleet_id":7}"#).unwrap();
        match msg {
            ClientMessage::Command(Command::Scan { fleet_id }) => assert_eq!(fleet_id, 7),
            other => panic!("wrong parse: {other:?}"),
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_line("not json").is_err());
        assert!(parse_line("[1,2]").is_err());
        assert!(parse_line(r#"{"fleet_id":1}"#).unwrap_err().contains("action"));
    }

    #[test]
    fn partial_policy_params_parse() {
        let msg = parse_line(
            r#"{"type":"policy_update","fleet_id":2,"preset":"mine_and_return","params":{"max_range":3}}"#,
        ).unwrap();
        match msg {
            ClientMessage::PolicyUpdate(u) => assert_eq!(u.params.unwrap().max_range, 3),
            other => panic!("wrong parse: {other:?}"),
        }
    }

    #[test]
    fn bad_policy_field_is_reported_as_itself() {
        let err = parse_line(
            r#"{"type":"policy_update","fleet_id":2,"preset":"mine_and_return","params":{"max_range":"far"}}"#,
        ).unwrap_err();
        assert!(!err.contains("missing field `action`"), "{err}");
        assert!(err.contains("max_range") || err.contains("invalid type"), "{err}");
    }

    #[test]
    fn bad_command_field_is_reported_as_itself() {
        let err = parse_line(r#"{"action":"split","fleet_id":2}"#).unwrap_err();
        assert!(err.contains("ship_ids"), "{err}");
    }
}
