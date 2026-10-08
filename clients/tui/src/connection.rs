// WebSocket client connection to the game server.

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{
    tungstenite::protocol::Message,
};

use iac_shared::constants::{DEFAULT_HOST, DEFAULT_PORT};
use iac_shared::protocol::{ClientMessage, ServerMessage};

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

    pub async fn send_auth(&self, player_name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let msg = ClientMessage::Auth(iac_shared::protocol::AuthRequest {
            player_name: player_name.to_string(),
            token: None,
        });
        self.send_message(msg).await
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
