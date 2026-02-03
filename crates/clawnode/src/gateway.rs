//! Gateway WebSocket client.
//!
//! Manages the connection between a node and the Clawbernetes gateway,
//! including automatic reconnection with exponential backoff.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use claw_proto::{GatewayMessage, NodeCapabilities, NodeId, NodeMessage};
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::error::NodeError;

/// Configuration for reconnection behavior.
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// Initial delay before first reconnection attempt.
    pub initial_delay: Duration,
    /// Maximum delay between reconnection attempts.
    pub max_delay: Duration,
    /// Multiplier for exponential backoff.
    pub backoff_multiplier: f64,
    /// Maximum number of reconnection attempts (None = infinite).
    pub max_attempts: Option<u32>,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            max_attempts: None,
        }
    }
}

impl ReconnectConfig {
    /// Calculate delay for the given attempt number.
    #[must_use]
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let multiplier = self.backoff_multiplier.powi(attempt.saturating_sub(1) as i32);
        let delay_millis = (self.initial_delay.as_millis() as f64 * multiplier) as u64;
        Duration::from_millis(delay_millis).min(self.max_delay)
    }

    /// Check if we should attempt reconnection.
    #[must_use]
    pub fn should_reconnect(&self, attempt: u32) -> bool {
        match self.max_attempts {
            Some(max) => attempt < max,
            None => true,
        }
    }
}

/// State of the gateway connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected.
    Disconnected,
    /// Attempting to connect.
    Connecting,
    /// Connected and registered.
    Connected,
    /// Connection failed, will retry.
    Reconnecting,
    /// Permanently failed (max retries exceeded).
    Failed,
}

/// Events emitted by the gateway client.
#[derive(Debug, Clone)]
pub enum GatewayEvent {
    /// Connection established.
    Connected,
    /// Connection lost.
    Disconnected {
        /// Reason for disconnection.
        reason: String,
    },
    /// Attempting reconnection.
    Reconnecting {
        /// Attempt number.
        attempt: u32,
        /// Delay before next attempt.
        delay: Duration,
    },
    /// Message received from gateway.
    Message(GatewayMessage),
    /// Error occurred.
    Error(String),
}

/// Gateway client for WebSocket communication.
pub struct GatewayClient {
    url: String,
    node_id: NodeId,
    node_name: String,
    capabilities: NodeCapabilities,
    reconnect_config: ReconnectConfig,
    state: Arc<AtomicConnectionState>,
    reconnect_attempts: Arc<AtomicU32>,
    running: Arc<AtomicBool>,
}

/// Atomic wrapper for connection state.
#[derive(Debug)]
pub struct AtomicConnectionState(AtomicU32);

impl AtomicConnectionState {
    /// Create a new atomic state.
    #[must_use]
    pub fn new(state: ConnectionState) -> Self {
        Self(AtomicU32::new(state as u32))
    }

    /// Load the current state.
    #[must_use]
    pub fn load(&self) -> ConnectionState {
        match self.0.load(Ordering::SeqCst) {
            0 => ConnectionState::Disconnected,
            1 => ConnectionState::Connecting,
            2 => ConnectionState::Connected,
            3 => ConnectionState::Reconnecting,
            _ => ConnectionState::Failed,
        }
    }

    /// Store a new state.
    pub fn store(&self, state: ConnectionState) {
        self.0.store(state as u32, Ordering::SeqCst);
    }
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
        let reg_msg = NodeMessage::register(self.node_id, &self.node_name, self.capabilities.clone());

        let mut write = write;
        let json = reg_msg
            .to_json()
            .map_err(|e| NodeError::GatewayConnection(format!("failed to serialize: {e}")))?;

        write
            .send(Message::Text(json.into()))
            .await
            .map_err(|e| {
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
                        if write.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                }
                None => break,
            }
        }
    }
}

/// Calculate reconnection delay using exponential backoff.
#[must_use]
pub fn calculate_backoff(
    attempt: u32,
    initial_delay: Duration,
    max_delay: Duration,
    multiplier: f64,
) -> Duration {
    let factor = multiplier.powi(attempt.saturating_sub(1) as i32);
    let delay_millis = (initial_delay.as_millis() as f64 * factor) as u64;
    Duration::from_millis(delay_millis).min(max_delay)
}

/// Attempt to reconnect with exponential backoff.
pub async fn reconnect_with_backoff<F, Fut, T, E>(
    config: &ReconnectConfig,
    mut connect_fn: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut attempt = 0;

    loop {
        attempt += 1;

        match connect_fn().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if !config.should_reconnect(attempt) {
                    return Err(e);
                }

                let delay = config.delay_for_attempt(attempt);
                sleep(delay).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconnect_config_default() {
        let config = ReconnectConfig::default();
        assert_eq!(config.initial_delay, Duration::from_secs(1));
        assert_eq!(config.max_delay, Duration::from_secs(60));
        assert_eq!(config.backoff_multiplier, 2.0);
        assert!(config.max_attempts.is_none());
    }

    #[test]
    fn test_delay_for_attempt() {
        let config = ReconnectConfig {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            max_attempts: None,
        };

        assert_eq!(config.delay_for_attempt(1), Duration::from_secs(1));
        assert_eq!(config.delay_for_attempt(2), Duration::from_secs(2));
        assert_eq!(config.delay_for_attempt(3), Duration::from_secs(4));
        assert_eq!(config.delay_for_attempt(4), Duration::from_secs(8));
        assert_eq!(config.delay_for_attempt(5), Duration::from_secs(16));
        assert_eq!(config.delay_for_attempt(6), Duration::from_secs(32));
        assert_eq!(config.delay_for_attempt(7), Duration::from_secs(60)); // capped
    }

    #[test]
    fn test_should_reconnect_infinite() {
        let config = ReconnectConfig {
            max_attempts: None,
            ..Default::default()
        };

        assert!(config.should_reconnect(1));
        assert!(config.should_reconnect(100));
        assert!(config.should_reconnect(1000));
    }

    #[test]
    fn test_should_reconnect_limited() {
        let config = ReconnectConfig {
            max_attempts: Some(5),
            ..Default::default()
        };

        assert!(config.should_reconnect(1));
        assert!(config.should_reconnect(4));
        assert!(!config.should_reconnect(5));
        assert!(!config.should_reconnect(6));
    }

    #[test]
    fn test_calculate_backoff() {
        let initial = Duration::from_millis(100);
        let max = Duration::from_secs(10);

        assert_eq!(
            calculate_backoff(1, initial, max, 2.0),
            Duration::from_millis(100)
        );
        assert_eq!(
            calculate_backoff(2, initial, max, 2.0),
            Duration::from_millis(200)
        );
        assert_eq!(
            calculate_backoff(3, initial, max, 2.0),
            Duration::from_millis(400)
        );
        assert_eq!(calculate_backoff(10, initial, max, 2.0), Duration::from_secs(10)); // capped
    }

    #[test]
    fn test_connection_state_enum() {
        assert_eq!(ConnectionState::Disconnected as u32, 0);
        assert_eq!(ConnectionState::Connecting as u32, 1);
        assert_eq!(ConnectionState::Connected as u32, 2);
        assert_eq!(ConnectionState::Reconnecting as u32, 3);
        assert_eq!(ConnectionState::Failed as u32, 4);
    }

    #[test]
    fn test_atomic_connection_state() {
        let state = AtomicConnectionState::new(ConnectionState::Disconnected);
        assert_eq!(state.load(), ConnectionState::Disconnected);

        state.store(ConnectionState::Connecting);
        assert_eq!(state.load(), ConnectionState::Connecting);

        state.store(ConnectionState::Connected);
        assert_eq!(state.load(), ConnectionState::Connected);
    }

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

    #[test]
    fn test_gateway_event_variants() {
        let connected = GatewayEvent::Connected;
        assert!(matches!(connected, GatewayEvent::Connected));

        let disconnected = GatewayEvent::Disconnected {
            reason: "timeout".to_string(),
        };
        if let GatewayEvent::Disconnected { reason } = disconnected {
            assert_eq!(reason, "timeout");
        } else {
            panic!("expected Disconnected");
        }

        let reconnecting = GatewayEvent::Reconnecting {
            attempt: 3,
            delay: Duration::from_secs(4),
        };
        if let GatewayEvent::Reconnecting { attempt, delay } = reconnecting {
            assert_eq!(attempt, 3);
            assert_eq!(delay, Duration::from_secs(4));
        } else {
            panic!("expected Reconnecting");
        }

        let error = GatewayEvent::Error("test error".to_string());
        if let GatewayEvent::Error(msg) = error {
            assert_eq!(msg, "test error");
        } else {
            panic!("expected Error");
        }
    }

    #[tokio::test]
    async fn test_reconnect_with_backoff_success_first_try() {
        let config = ReconnectConfig {
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            max_attempts: Some(3),
        };

        let mut attempts = 0;
        let result: Result<i32, &str> = reconnect_with_backoff(&config, || {
            attempts += 1;
            async move { Ok(42) }
        })
        .await;

        assert_eq!(result, Ok(42));
        assert_eq!(attempts, 1);
    }

    #[tokio::test]
    async fn test_reconnect_with_backoff_success_after_retries() {
        let config = ReconnectConfig {
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            backoff_multiplier: 2.0,
            max_attempts: Some(5),
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let result: Result<i32, &str> = reconnect_with_backoff(&config, || {
            let a = attempts_clone.fetch_add(1, Ordering::SeqCst);
            async move {
                if a < 2 {
                    Err("not yet")
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result, Ok(42));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_reconnect_with_backoff_max_attempts_exceeded() {
        let config = ReconnectConfig {
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            backoff_multiplier: 2.0,
            max_attempts: Some(3),
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let result: Result<i32, &str> = reconnect_with_backoff(&config, || {
            attempts_clone.fetch_add(1, Ordering::SeqCst);
            async move { Err("always fail") }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_delay_with_zero_attempt() {
        let config = ReconnectConfig::default();
        let delay = config.delay_for_attempt(0);
        assert_eq!(delay, Duration::from_secs(1));
    }

    #[test]
    fn test_backoff_with_different_multipliers() {
        let initial = Duration::from_millis(100);
        let max = Duration::from_secs(60);

        // multiplier 1.5
        assert_eq!(
            calculate_backoff(2, initial, max, 1.5),
            Duration::from_millis(150)
        );
        assert_eq!(
            calculate_backoff(3, initial, max, 1.5),
            Duration::from_millis(225)
        );

        // multiplier 3.0
        assert_eq!(
            calculate_backoff(2, initial, max, 3.0),
            Duration::from_millis(300)
        );
        assert_eq!(
            calculate_backoff(3, initial, max, 3.0),
            Duration::from_millis(900)
        );
    }
}
