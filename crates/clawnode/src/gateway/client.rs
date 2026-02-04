//! Basic gateway WebSocket client.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use claw_proto::{GatewayMessage, NodeCapabilities, NodeId, NodeMessage};
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::error::NodeError;

use super::events::GatewayEvent;
use super::reconnect::ReconnectConfig;
use super::state::{AtomicConnectionState, ConnectionState};

/// Gateway client for WebSocket communication.
pub struct GatewayClient {
    url: String,
    node_id: NodeId,
    node_name: String,
    capabilities: NodeCapabilities,
    pub(crate) reconnect_config: ReconnectConfig,
    pub(crate) state: Arc<AtomicConnectionState>,
    reconnect_attempts: Arc<AtomicU32>,
    pub(crate) running: Arc<AtomicBool>,
}

impl GatewayClient {
    /// Create a new gateway client.
    #[must_use]
    pub fn new(
        url: impl Into<String>,
        node_id: NodeId,
        node_name: impl Into<String>,
        capabilities: NodeCapabilities,
    ) -> Self {
        Self {
            url: url.into(),
            node_id,
            node_name: node_name.into(),
            capabilities,
            reconnect_config: ReconnectConfig::default(),
            state: Arc::new(AtomicConnectionState::new(ConnectionState::Disconnected)),
            reconnect_attempts: Arc::new(AtomicU32::new(0)),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Set reconnection configuration.
    #[must_use]
    pub fn with_reconnect_config(mut self, config: ReconnectConfig) -> Self {
        self.reconnect_config = config;
        self
    }

    /// Get the current connection state.
    #[must_use]
    pub fn state(&self) -> ConnectionState {
        self.state.load()
    }

    /// Get the current reconnection attempt count.
    #[must_use]
    pub fn reconnect_attempts(&self) -> u32 {
        self.reconnect_attempts.load(Ordering::SeqCst)
    }

    /// Check if the client is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Connect to the gateway.
    ///
    /// Returns channels for sending messages and receiving events.
    ///
    /// # Errors
    ///
    /// Returns an error if the initial connection fails.
    pub async fn connect(
        &self,
    ) -> Result<(mpsc::Sender<NodeMessage>, mpsc::Receiver<GatewayEvent>), NodeError> {
        self.state.store(ConnectionState::Connecting);
        self.running.store(true, Ordering::SeqCst);

        let (ws_stream, _) = tokio_tungstenite::connect_async(&self.url)
            .await
            .map_err(|e| NodeError::GatewayConnection(format!("failed to connect: {e}")))?;

        self.state.store(ConnectionState::Connected);
        self.reconnect_attempts.store(0, Ordering::SeqCst);

        let (write, read) = ws_stream.split();

        let (tx_to_ws, rx_from_client) = mpsc::channel::<NodeMessage>(32);
        let (tx_to_client, rx_from_ws) = mpsc::channel::<GatewayEvent>(32);

        // Send registration message
        let reg_msg =
            NodeMessage::register(self.node_id, &self.node_name, self.capabilities.clone());

        let mut write = write;
        let json = reg_msg
            .to_json()
            .map_err(|e| NodeError::GatewayConnection(format!("failed to serialize: {e}")))?;

        write.send(Message::Text(json)).await.map_err(|e| {
            NodeError::GatewayConnection(format!("failed to send registration: {e}"))
        })?;

        // Spawn reader task
        let tx_events = tx_to_client.clone();
        let state = Arc::clone(&self.state);
        let running = Arc::clone(&self.running);
        tokio::spawn(async move {
            Self::reader_task(read, tx_events, state, running).await;
        });

        // Spawn writer task
        let running = Arc::clone(&self.running);
        tokio::spawn(async move {
            Self::writer_task(write, rx_from_client, running).await;
        });

        // Send connected event
        let _ = tx_to_client.send(GatewayEvent::Connected).await;

        Ok((tx_to_ws, rx_from_ws))
    }

    /// Stop the client.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.state.store(ConnectionState::Disconnected);
    }

    async fn reader_task(
        mut read: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
        tx: mpsc::Sender<GatewayEvent>,
        state: Arc<AtomicConnectionState>,
        running: Arc<AtomicBool>,
    ) {
        while running.load(Ordering::SeqCst) {
            match read.next().await {
                Some(Ok(msg)) => {
                    if let Message::Text(text) = msg {
                        match GatewayMessage::from_json(&text) {
                            Ok(gateway_msg) => {
                                if tx.send(GatewayEvent::Message(gateway_msg)).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                let _ = tx
                                    .send(GatewayEvent::Error(format!("parse error: {e}")))
                                    .await;
                            }
                        }
                    }
                }
                Some(Err(e)) => {
                    state.store(ConnectionState::Disconnected);
                    let _ = tx
                        .send(GatewayEvent::Disconnected {
                            reason: e.to_string(),
                        })
                        .await;
                    break;
                }
                None => {
                    state.store(ConnectionState::Disconnected);
                    let _ = tx
                        .send(GatewayEvent::Disconnected {
                            reason: "connection closed".to_string(),
                        })
                        .await;
                    break;
                }
            }
        }
    }

    async fn writer_task(
        mut write: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
        mut rx: mpsc::Receiver<NodeMessage>,
        running: Arc<AtomicBool>,
    ) {
        while running.load(Ordering::SeqCst) {
            match rx.recv().await {
                Some(msg) => {
                    if let Ok(json) = msg.to_json() {
                        if write.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_client_creation() {
        let client = GatewayClient::new(
            "wss://gateway.example.com:8080",
            NodeId::new(),
            "test-node",
            NodeCapabilities::default(),
        );

        assert_eq!(client.state(), ConnectionState::Disconnected);
        assert_eq!(client.reconnect_attempts(), 0);
        assert!(!client.is_running());
    }

    #[test]
    fn test_gateway_client_with_config() {
        use std::time::Duration;

        let config = ReconnectConfig {
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 1.5,
            max_attempts: Some(10),
        };

        let client = GatewayClient::new(
            "wss://gateway.example.com",
            NodeId::new(),
            "test-node",
            NodeCapabilities::default(),
        )
        .with_reconnect_config(config);

        assert_eq!(
            client.reconnect_config.initial_delay,
            Duration::from_millis(500)
        );
        assert_eq!(client.reconnect_config.max_attempts, Some(10));
    }

    #[test]
    fn test_gateway_client_stop() {
        let client = GatewayClient::new(
            "wss://gateway.example.com",
            NodeId::new(),
            "test-node",
            NodeCapabilities::default(),
        );

        client.running.store(true, Ordering::SeqCst);
        client.state.store(ConnectionState::Connected);

        assert!(client.is_running());
        assert_eq!(client.state(), ConnectionState::Connected);

        client.stop();

        assert!(!client.is_running());
        assert_eq!(client.state(), ConnectionState::Disconnected);
    }
}
