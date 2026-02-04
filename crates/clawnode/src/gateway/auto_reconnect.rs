//! Gateway client with automatic reconnection support.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use claw_proto::{GatewayMessage, NodeCapabilities, NodeId, NodeMessage};
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use super::events::AutoReconnectEvent;
use super::heartbeat::{start_heartbeat_task, HeartbeatConfig, HeartbeatHandle};
use super::reconnect::ReconnectConfig;
use super::state::{AtomicConnectionState, ConnectionState};

/// A gateway client with automatic reconnection support.
pub struct AutoReconnectClient {
    url: String,
    node_id: NodeId,
    node_name: String,
    capabilities: NodeCapabilities,
    pub(crate) reconnect_config: ReconnectConfig,
    pub(crate) heartbeat_config: Option<HeartbeatConfig>,
    pub(crate) state: Arc<AtomicConnectionState>,
    pub(crate) running: Arc<AtomicBool>,
}

impl AutoReconnectClient {
    /// Create a new auto-reconnect client.
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
            heartbeat_config: None,
            state: Arc::new(AtomicConnectionState::new(ConnectionState::Disconnected)),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Configure reconnection behavior.
    #[must_use]
    pub fn with_reconnect_config(mut self, config: ReconnectConfig) -> Self {
        self.reconnect_config = config;
        self
    }

    /// Enable automatic heartbeats with the given configuration.
    #[must_use]
    pub fn with_heartbeat(mut self, config: HeartbeatConfig) -> Self {
        self.heartbeat_config = Some(config);
        self
    }

    /// Get the current connection state.
    #[must_use]
    pub fn state(&self) -> ConnectionState {
        self.state.load()
    }

    /// Check if the client is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Stop the client.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.state.store(ConnectionState::Disconnected);
    }

    /// Start the client with automatic reconnection.
    ///
    /// Returns channels for sending messages and receiving events.
    pub fn start(
        &self,
    ) -> (
        mpsc::Sender<NodeMessage>,
        mpsc::Receiver<AutoReconnectEvent>,
    ) {
        self.running.store(true, Ordering::SeqCst);

        let (client_tx, client_rx) = mpsc::channel::<NodeMessage>(32);
        let (event_tx, event_rx) = mpsc::channel::<AutoReconnectEvent>(32);

        let url = self.url.clone();
        let node_id = self.node_id;
        let node_name = self.node_name.clone();
        let capabilities = self.capabilities.clone();
        let reconnect_config = self.reconnect_config.clone();
        let heartbeat_config = self.heartbeat_config.clone();
        let state = Arc::clone(&self.state);
        let running = Arc::clone(&self.running);

        tokio::spawn(async move {
            Self::connection_loop(
                url,
                node_id,
                node_name,
                capabilities,
                reconnect_config,
                heartbeat_config,
                state,
                running,
                client_rx,
                event_tx,
            )
            .await;
        });

        (client_tx, event_rx)
    }

    async fn connection_loop(
        url: String,
        node_id: NodeId,
        node_name: String,
        capabilities: NodeCapabilities,
        reconnect_config: ReconnectConfig,
        heartbeat_config: Option<HeartbeatConfig>,
        state: Arc<AtomicConnectionState>,
        running: Arc<AtomicBool>,
        mut client_rx: mpsc::Receiver<NodeMessage>,
        event_tx: mpsc::Sender<AutoReconnectEvent>,
    ) {
        let mut attempt = 0u32;

        while running.load(Ordering::SeqCst) {
            state.store(ConnectionState::Connecting);

            // Attempt to connect
            match tokio_tungstenite::connect_async(&url).await {
                Ok((ws_stream, _)) => {
                    state.store(ConnectionState::Connected);
                    attempt = 0;

                    let _ = event_tx.send(AutoReconnectEvent::Connected).await;

                    let (write, read) = ws_stream.split();

                    // Send registration
                    let reg_msg =
                        NodeMessage::register(node_id, &node_name, capabilities.clone());

                    let mut write = write;
                    let json = match reg_msg.to_json() {
                        Ok(j) => j,
                        Err(_) => {
                            state.store(ConnectionState::Disconnected);
                            continue;
                        }
                    };

                    if write.send(Message::Text(json)).await.is_err() {
                        state.store(ConnectionState::Disconnected);
                        continue;
                    }

                    // Create internal channels for the connection
                    let (internal_tx, internal_rx) = mpsc::channel::<NodeMessage>(32);

                    // Start heartbeat if configured
                    let heartbeat_handle = heartbeat_config.as_ref().map(|config| {
                        start_heartbeat_task(node_id, internal_tx.clone(), config.clone())
                    });

                    // Run the connection until it breaks
                    let disconnect_reason = Self::run_connection(
                        read,
                        write,
                        internal_rx,
                        &mut client_rx,
                        &event_tx,
                        &running,
                        heartbeat_handle.as_ref(),
                    )
                    .await;

                    // Stop heartbeat
                    if let Some(hb) = heartbeat_handle {
                        hb.stop();
                    }

                    state.store(ConnectionState::Disconnected);

                    if running.load(Ordering::SeqCst) {
                        let _ = event_tx
                            .send(AutoReconnectEvent::Disconnected {
                                reason: disconnect_reason,
                            })
                            .await;
                    }
                }
                Err(e) => {
                    attempt += 1;

                    if !reconnect_config.should_reconnect(attempt) {
                        state.store(ConnectionState::Failed);
                        let _ = event_tx
                            .send(AutoReconnectEvent::ReconnectFailed {
                                attempts: attempt,
                                last_error: e.to_string(),
                            })
                            .await;
                        break;
                    }

                    state.store(ConnectionState::Reconnecting);
                    let delay = reconnect_config.delay_for_attempt(attempt);

                    let _ = event_tx
                        .send(AutoReconnectEvent::Reconnecting { attempt, delay })
                        .await;

                    sleep(delay).await;
                }
            }
        }
    }

    async fn run_connection(
        mut read: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
        mut write: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
        mut internal_rx: mpsc::Receiver<NodeMessage>,
        client_rx: &mut mpsc::Receiver<NodeMessage>,
        event_tx: &mpsc::Sender<AutoReconnectEvent>,
        running: &Arc<AtomicBool>,
        heartbeat_handle: Option<&HeartbeatHandle>,
    ) -> String {
        loop {
            if !running.load(Ordering::SeqCst) {
                return "client stopped".to_string();
            }

            tokio::select! {
                // Read from WebSocket
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            match GatewayMessage::from_json(&text) {
                                Ok(gateway_msg) => {
                                    // Check for heartbeat ack
                                    if matches!(gateway_msg, GatewayMessage::HeartbeatAck { .. }) {
                                        if let Some(hb) = heartbeat_handle {
                                            hb.ack_received();
                                        }
                                    }

                                    if event_tx
                                        .send(AutoReconnectEvent::Message(gateway_msg))
                                        .await
                                        .is_err()
                                    {
                                        return "event channel closed".to_string();
                                    }
                                }
                                Err(e) => {
                                    // Parse error, log but continue
                                    tracing::warn!("Failed to parse gateway message: {}", e);
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            return "server closed connection".to_string();
                        }
                        Some(Err(e)) => {
                            return format!("WebSocket error: {e}");
                        }
                        None => {
                            return "connection closed".to_string();
                        }
                        _ => {
                            // Ignore other message types (Ping, Pong, Binary)
                        }
                    }
                }

                // Handle internal messages (heartbeats)
                msg = internal_rx.recv() => {
                    if let Some(node_msg) = msg {
                        if let Ok(json) = node_msg.to_json() {
                            if write.send(Message::Text(json)).await.is_err() {
                                return "write failed".to_string();
                            }
                        }
                    }
                }

                // Handle client messages
                msg = client_rx.recv() => {
                    match msg {
                        Some(node_msg) => {
                            if let Ok(json) = node_msg.to_json() {
                                if write.send(Message::Text(json)).await.is_err() {
                                    return "write failed".to_string();
                                }
                            }
                        }
                        None => {
                            return "client channel closed".to_string();
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_auto_reconnect_client_creation() {
        let client = AutoReconnectClient::new(
            "wss://gateway.example.com",
            NodeId::new(),
            "test-node",
            NodeCapabilities::default(),
        );

        assert_eq!(client.state(), ConnectionState::Disconnected);
        assert!(!client.is_running());
    }

    #[test]
    fn test_auto_reconnect_client_with_reconnect_config() {
        let config = ReconnectConfig {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 1.5,
            max_attempts: Some(5),
        };

        let client = AutoReconnectClient::new(
            "wss://gateway.example.com",
            NodeId::new(),
            "test-node",
            NodeCapabilities::default(),
        )
        .with_reconnect_config(config);

        assert_eq!(
            client.reconnect_config.initial_delay,
            Duration::from_millis(100)
        );
        assert_eq!(client.reconnect_config.max_attempts, Some(5));
    }

    #[test]
    fn test_auto_reconnect_client_with_heartbeat() {
        let heartbeat_config = HeartbeatConfig {
            interval: Duration::from_secs(15),
            ack_timeout: Duration::from_secs(5),
            max_missed_acks: 2,
        };

        let client = AutoReconnectClient::new(
            "wss://gateway.example.com",
            NodeId::new(),
            "test-node",
            NodeCapabilities::default(),
        )
        .with_heartbeat(heartbeat_config);

        assert!(client.heartbeat_config.is_some());
        assert_eq!(
            client.heartbeat_config.as_ref().map(|c| c.interval),
            Some(Duration::from_secs(15))
        );
    }

    #[test]
    fn test_auto_reconnect_client_stop() {
        let client = AutoReconnectClient::new(
            "wss://gateway.example.com",
            NodeId::new(),
            "test-node",
            NodeCapabilities::default(),
        );

        client.running.store(true, Ordering::SeqCst);
        client.state.store(ConnectionState::Connected);

        client.stop();

        assert!(!client.is_running());
        assert_eq!(client.state(), ConnectionState::Disconnected);
    }
}
