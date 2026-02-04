//! High-level gateway handle for sending messages and receiving events.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use claw_proto::{GatewayMessage, NodeCapabilities, NodeId, NodeMessage, WorkloadId, WorkloadState};
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::error::NodeError;
use crate::metrics::MetricsReport;

use super::events::GatewayEvent;
use super::state::{AtomicConnectionState, ConnectionState};

/// A handle for communicating with the gateway.
///
/// Provides a higher-level API than `GatewayClient` with methods for
/// sending registration, heartbeats, metrics, and workload updates.
pub struct GatewayHandle {
    url: String,
    node_id: NodeId,
    node_name: String,
    capabilities: NodeCapabilities,
    pub(crate) state: Arc<AtomicConnectionState>,
    pub(crate) running: Arc<AtomicBool>,
    /// Channel for sending messages to the gateway.
    tx: Arc<tokio::sync::RwLock<Option<mpsc::Sender<NodeMessage>>>>,
    /// Channel for receiving events from the gateway.
    rx: Arc<tokio::sync::Mutex<Option<mpsc::Receiver<GatewayEvent>>>>,
    /// Optional handler channel for message routing.
    handler_tx: Option<mpsc::Sender<GatewayMessage>>,
}

impl GatewayHandle {
    /// Create a new gateway handle.
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
            state: Arc::new(AtomicConnectionState::new(ConnectionState::Disconnected)),
            running: Arc::new(AtomicBool::new(false)),
            tx: Arc::new(tokio::sync::RwLock::new(None)),
            rx: Arc::new(tokio::sync::Mutex::new(None)),
            handler_tx: None,
        }
    }

    /// Add a message handler for routing incoming messages.
    #[must_use]
    pub fn with_message_handler(mut self, tx: mpsc::Sender<GatewayMessage>) -> Self {
        self.handler_tx = Some(tx);
        self
    }

    /// Get the current connection state.
    #[must_use]
    pub fn state(&self) -> ConnectionState {
        self.state.load()
    }

    /// Get the node ID.
    #[must_use]
    pub const fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Connect to the gateway and send registration.
    ///
    /// # Errors
    ///
    /// Returns an error if connection or registration fails.
    pub async fn send_registration(
        &self,
        capabilities: &NodeCapabilities,
    ) -> Result<(), NodeError> {
        self.state.store(ConnectionState::Connecting);
        self.running.store(true, Ordering::SeqCst);

        let (ws_stream, _) = tokio_tungstenite::connect_async(&self.url)
            .await
            .map_err(|e| NodeError::GatewayConnection(format!("failed to connect: {e}")))?;

        let (write, read) = ws_stream.split();

        let (tx_to_ws, rx_from_client) = mpsc::channel::<NodeMessage>(32);
        let (tx_to_client, rx_from_ws) = mpsc::channel::<GatewayEvent>(32);

        // Store channels
        {
            let mut tx_guard = self.tx.write().await;
            *tx_guard = Some(tx_to_ws.clone());
        }
        {
            let mut rx_guard = self.rx.lock().await;
            *rx_guard = Some(rx_from_ws);
        }

        // Send registration message
        let reg_msg =
            NodeMessage::register(self.node_id, &self.node_name, capabilities.clone());
        let json = reg_msg
            .to_json()
            .map_err(|e| NodeError::GatewayConnection(format!("failed to serialize: {e}")))?;

        let mut write = write;
        write.send(Message::Text(json)).await.map_err(|e| {
            NodeError::GatewayConnection(format!("failed to send registration: {e}"))
        })?;

        self.state.store(ConnectionState::Connected);

        // Spawn reader task
        let tx_events = tx_to_client;
        let state = Arc::clone(&self.state);
        let running = Arc::clone(&self.running);
        tokio::spawn(async move {
            Self::reader_loop(read, tx_events, state, running).await;
        });

        // Spawn writer task
        let running = Arc::clone(&self.running);
        tokio::spawn(async move {
            Self::writer_loop(write, rx_from_client, running).await;
        });

        Ok(())
    }

    /// Send a heartbeat message.
    ///
    /// # Errors
    ///
    /// Returns an error if not connected or send fails.
    pub async fn send_heartbeat(&self) -> Result<(), NodeError> {
        let msg = NodeMessage::heartbeat(self.node_id);
        self.send_message(msg).await
    }

    /// Send metrics to the gateway.
    ///
    /// # Errors
    ///
    /// Returns an error if not connected or send fails.
    pub async fn send_metrics(&self, report: &MetricsReport) -> Result<(), NodeError> {
        let msg = NodeMessage::metrics(self.node_id, report.gpu_metrics.clone());
        self.send_message(msg).await
    }

    /// Send a workload update to the gateway.
    ///
    /// # Errors
    ///
    /// Returns an error if not connected or send fails.
    pub async fn send_workload_update(
        &self,
        workload_id: WorkloadId,
        state: WorkloadState,
        message: Option<String>,
    ) -> Result<(), NodeError> {
        let msg = NodeMessage::workload_update(workload_id, state, message);
        self.send_message(msg).await
    }

    /// Receive the next gateway event.
    ///
    /// # Errors
    ///
    /// Returns an error if not connected or channel is closed.
    pub async fn recv(&self) -> Result<GatewayEvent, NodeError> {
        let mut rx_guard = self.rx.lock().await;
        let rx = rx_guard
            .as_mut()
            .ok_or_else(|| NodeError::GatewayConnection("not connected".into()))?;

        rx.recv()
            .await
            .ok_or_else(|| NodeError::GatewayConnection("channel closed".into()))
    }

    /// Start routing incoming messages to the handler.
    ///
    /// This spawns a background task that forwards messages to the handler channel.
    pub fn start_routing(&self) {
        if let Some(handler_tx) = &self.handler_tx {
            let rx = Arc::clone(&self.rx);
            let handler_tx = handler_tx.clone();
            let running = Arc::clone(&self.running);

            tokio::spawn(async move {
                loop {
                    if !running.load(Ordering::SeqCst) {
                        break;
                    }

                    let event = {
                        let mut rx_guard = rx.lock().await;
                        if let Some(ref mut receiver) = *rx_guard {
                            receiver.recv().await
                        } else {
                            None
                        }
                    };

                    match event {
                        Some(GatewayEvent::Message(msg)) => {
                            if handler_tx.send(msg).await.is_err() {
                                break;
                            }
                        }
                        Some(_) => {
                            // Other events - continue processing
                        }
                        None => break,
                    }
                }
            });
        }
    }

    /// Stop the gateway handle.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.state.store(ConnectionState::Disconnected);
    }

    /// Check if connected.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.state.load() == ConnectionState::Connected
    }

    /// Send a raw message to the gateway.
    async fn send_message(&self, msg: NodeMessage) -> Result<(), NodeError> {
        let tx_guard = self.tx.read().await;
        let tx = tx_guard
            .as_ref()
            .ok_or_else(|| NodeError::GatewayConnection("not connected".into()))?;

        tx.send(msg)
            .await
            .map_err(|e| NodeError::GatewayConnection(format!("send failed: {e}")))
    }

    async fn reader_loop(
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

    async fn writer_loop(
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
    fn test_gateway_handle_creation() {
        let node_id = NodeId::new();
        let handle = GatewayHandle::new(
            "wss://gateway.example.com",
            node_id,
            "test-node",
            NodeCapabilities::default(),
        );

        assert_eq!(handle.state(), ConnectionState::Disconnected);
        assert_eq!(handle.node_id(), node_id);
        assert!(!handle.is_connected());
    }

    #[test]
    fn test_gateway_handle_stop() {
        let handle = GatewayHandle::new(
            "wss://gateway.example.com",
            NodeId::new(),
            "test-node",
            NodeCapabilities::default(),
        );

        handle.running.store(true, Ordering::SeqCst);
        handle.state.store(ConnectionState::Connected);

        handle.stop();

        assert!(!handle.is_connected());
        assert_eq!(handle.state(), ConnectionState::Disconnected);
    }
}
