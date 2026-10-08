// WebSocket client connection to the game server.

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{
    tungstenite::protocol::Message,
};

use iac_shared::constants::{DEFAULT_HOST, DEFAULT_PORT};
use iac_shared::protocol::{AuthRequest, AuthResult, ClientMessage, ErrorCode, ServerMessage};

use crate::token_store;

const AUTH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub struct Connection {
    pub tx: mpsc::UnboundedSender<Message>,
    pub rx: Mutex<mpsc::UnboundedReceiver<String>>,
}

pub struct ConnectionConfig {
    pub host: String,
    pub port: u16,
}

impl ConnectionConfig {
    #[allow(dead_code)]
    pub fn default_config() -> Self {
        Self {
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
        }
    }
}

impl Connection {
    pub async fn connect(config: &ConnectionConfig) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("ws://{}:{}", config.host, config.port);
        let (ws_stream, _) = tokio_tungstenite::connect_async(&url).await?;

        let (ws_tx, mut ws_rx) = ws_stream.split();
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let (recv_tx, recv_rx) = mpsc::unbounded_channel();

        // Writer task: forward channel messages to the WebSocket
        tokio::spawn(async move {
            let mut ws_tx = ws_tx;
            while let Some(msg) = msg_rx.recv().await {
                if ws_tx.send(msg).await.is_err() {
                    break;
                }
            }
        });

        // Reader task: read from WebSocket, forward to receive channel
        tokio::spawn(async move {
            while let Some(msg) = ws_rx.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if recv_tx.send(text.as_str().to_string()).is_err() {
                            break;
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            tx: msg_tx,
            rx: Mutex::new(recv_rx),
        })
    }

    /// Send the auth request and wait for the server's verdict. Returns the
    /// raw reply line along with the parsed result; the full_state that
    /// follows a success stays in the receive queue.
    pub async fn authenticate(
        &self,
        player_name: &str,
        token: Option<&str>,
    ) -> Result<(String, AuthResult), Box<dyn std::error::Error + Send + Sync>> {
        self.send_message(ClientMessage::Auth(AuthRequest {
            player_name: player_name.to_string(),
            token: token.map(str::to_string),
        }))
        .await?;

        let reply = tokio::time::timeout(AUTH_TIMEOUT, async {
            let mut rx = self.rx.lock().await;
            while let Some(text) = rx.recv().await {
                if let Ok(ServerMessage::AuthResult(result)) = serde_json::from_str(&text) {
                    return Some((text, result));
                }
            }
            None
        })
        .await
        .map_err(|_| "the server did not answer the login request")?;
        reply.ok_or_else(|| "the server closed the connection during login".into())
    }

    #[allow(dead_code)]
    pub async fn send_command(&self, cmd: ClientMessage) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.send_message(cmd).await
    }

    #[allow(dead_code)]
    pub async fn request_full_state(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let msg = ClientMessage::RequestFullState;
        self.send_message(msg).await
    }

    pub async fn send_message(&self, msg: ClientMessage) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let json = serde_json::to_string(&msg)?;
        self.tx.send(Message::Text(json.into()))?;
        Ok(())
    }

    /// Poll for incoming messages. Returns None if no messages available.
    #[allow(dead_code)]
    pub async fn poll(&self) -> Option<ServerMessage> {
        let mut rx = self.rx.lock().await;
        match rx.try_recv() {
            Ok(text) => serde_json::from_str(&text).ok(),
            Err(mpsc::error::TryRecvError::Empty) => None,
            Err(mpsc::error::TryRecvError::Disconnected) => None,
        }
    }

    /// Block until the next message arrives (used in background task).
    #[allow(dead_code)]
    pub async fn recv(&self) -> Option<ServerMessage> {
        let mut rx = self.rx.lock().await;
        match rx.recv().await {
            Some(text) => serde_json::from_str(&text).ok(),
            None => None,
        }
    }
}

/// Parse a raw WebSocket message into a ServerMessage.
pub fn parse_server_message(text: &str) -> Result<ServerMessage, Box<dyn std::error::Error + Send + Sync>> {
    serde_json::from_str(text).map_err(|e| e.into())
}

/// Connect and log in as `name`. The token comes from `--token`, else from
/// the saved token file for this server and name; a token issued by the
/// server is saved there. Returns the connection and the raw auth_result
/// line (headless mode echoes it). Notices go to stderr.
pub async fn login(
    config: &ConnectionConfig,
    name: &str,
    cli_token: Option<&str>,
) -> Result<(Connection, String), Box<dyn std::error::Error + Send + Sync>> {
    let server = token_store::server_key(&config.host, config.port);
    let token = cli_token.map(str::to_string).or_else(|| token_store::load(&server, name));

    let conn = Connection::connect(config).await?;
    let (raw, result) = conn.authenticate(name, token.as_deref()).await?;

    if !result.success {
        return Err(login_error(&result, token.is_some(), cli_token.is_some()).into());
    }
    if let Some(issued) = &result.token {
        match token_store::save(&server, name, issued) {
            Ok(path) => eprintln!("iac-client: new token for '{name}' saved to {}", path.display()),
            Err(e) => eprintln!(
                "iac-client: could not save the token ({e}); keep it and pass --token next time: {issued}"
            ),
        }
    }
    Ok((conn, raw))
}

fn login_error(result: &AuthResult, sent_token: bool, from_cli: bool) -> String {
    let reason = result.message.as_deref().unwrap_or("login refused");
    let hint = match result.code {
        Some(ErrorCode::TokenRequired) => format!(
            "\n  this name already has an owner: pass its token with --token, or restore it in {}",
            token_file_hint()
        ),
        Some(ErrorCode::InvalidToken) if sent_token => format!(
            "\n  the token {} was refused: pass the right one with --token, or fix {}",
            if from_cli { "from --token" } else { "from the saved token file" },
            token_file_hint()
        ),
        Some(ErrorCode::InvalidName) => "\n  pick another --name".to_string(),
        _ => String::new(),
    };
    format!("login failed: {reason}{hint}")
}

fn token_file_hint() -> String {
    token_store::default_path().map(|p| p.display().to_string()).unwrap_or_else(|| "the token file".to_string())
}
