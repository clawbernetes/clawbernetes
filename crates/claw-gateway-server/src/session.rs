//! Per-node WebSocket session management.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use claw_gateway::{NodeRegistry, WorkloadManager};
use claw_proto::{GatewayMessage, NodeId, NodeMessage};
use futures::{SinkExt, StreamExt};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, error, info, warn};

use crate::config::ServerConfig;
use crate::error::{ServerError, ServerResult};
use crate::handlers::route_message;

/// State of a node session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Session is connected but node has not registered yet.
    Connected,
    /// Node has registered and is active.
    Registered,
    /// Session is disconnecting.
    Disconnecting,
    /// Session has disconnected.
    Disconnected,
}

impl SessionState {
    /// Check if the session is active (can receive/send messages).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Connected | Self::Registered)
    }
}

/// Per-node session managing a single WebSocket connection.
#[derive(Debug)]
pub struct NodeSession {
    /// Unique session identifier.
    id: uuid::Uuid,
    /// Node ID (set after registration).
    node_id: Option<NodeId>,
    /// Current session state.
    state: SessionState,
    /// When the session was created.
    connected_at: DateTime<Utc>,
    /// Last message received timestamp.
    last_message_at: DateTime<Utc>,
}

impl NodeSession {
    /// Create a new node session.
    #[must_use]
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4(),
            node_id: None,
            state: SessionState::Connected,
            connected_at: now,
            last_message_at: now,
        }
    }

    /// Get the session ID.
    #[must_use]
    pub const fn id(&self) -> uuid::Uuid {
        self.id
    }

    /// Get the node ID if registered.
    #[must_use]
    pub const fn node_id(&self) -> Option<NodeId> {
        self.node_id
    }

    /// Get the current session state.
    #[must_use]
    pub const fn state(&self) -> SessionState {
        self.state
    }

    /// Get when the session was created.
    #[must_use]
    pub const fn connected_at(&self) -> DateTime<Utc> {
        self.connected_at
    }

    /// Get when the last message was received.
    #[must_use]
    pub const fn last_message_at(&self) -> DateTime<Utc> {
        self.last_message_at
    }

    /// Check if the session is active.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.state.is_active()
    }

    /// Check if the node is registered.
    #[must_use]
    pub const fn is_registered(&self) -> bool {
        self.node_id.is_some() && matches!(self.state, SessionState::Registered)
    }

    /// Set the node ID after registration.
    pub const fn set_node_id(&mut self, node_id: NodeId) {
        self.node_id = Some(node_id);
        self.state = SessionState::Registered;
    }

    /// Update the last message timestamp.
    pub fn touch(&mut self) {
        self.last_message_at = Utc::now();
    }

    /// Mark the session as disconnecting.
    pub const fn disconnect(&mut self) {
        self.state = SessionState::Disconnecting;
    }

    /// Mark the session as fully disconnected.
    pub const fn set_disconnected(&mut self) {
        self.state = SessionState::Disconnected;
    }
}

impl Default for NodeSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle processing a raw WebSocket message.
///
/// # Errors
///
/// Returns an error if the message cannot be processed.
pub fn process_ws_message(ws_msg: &WsMessage) -> ServerResult<Option<NodeMessage>> {
    match ws_msg {
        WsMessage::Text(text) => {
            let node_msg: NodeMessage = serde_json::from_str(text)?;
            Ok(Some(node_msg))
        }
        WsMessage::Binary(data) => {
            let node_msg: NodeMessage = serde_json::from_slice(data)?;
            Ok(Some(node_msg))
        }
        WsMessage::Ping(_) | WsMessage::Pong(_) => {
            // Ping/Pong handled at WebSocket layer
            Ok(None)
        }
        WsMessage::Close(_) => {
            debug!("Received close frame");
            Err(ServerError::ConnectionClosed)
        }
        WsMessage::Frame(_) => {
            // Raw frames should not be received at this level
            Ok(None)
        }
    }
}

/// Serialize a gateway message to a WebSocket message.
///
/// # Errors
///
/// Returns an error if serialization fails.
pub fn gateway_msg_to_ws(msg: &GatewayMessage) -> ServerResult<WsMessage> {
    let json = serde_json::to_string(msg)?;
    Ok(WsMessage::Text(json))
}

/// Channel for sending outbound messages to a session.
pub type SessionSender = mpsc::Sender<GatewayMessage>;

/// Channel for receiving outbound messages in a session.
pub type SessionReceiver = mpsc::Receiver<GatewayMessage>;

/// Create a new session channel pair.
#[must_use]
pub fn session_channel(buffer_size: usize) -> (SessionSender, SessionReceiver) {
    mpsc::channel(buffer_size)
}

/// Run a session handling loop for a WebSocket connection.
///
/// This function runs two concurrent tasks:
/// 1. Reading messages from the WebSocket and processing them
/// 2. Forwarding messages from the channel to the WebSocket
///
/// # Errors
///
/// Returns an error if the session encounters a fatal error.
pub async fn run_session<S>(
    ws_stream: S,
    session: Arc<Mutex<NodeSession>>,
    registry: Arc<Mutex<NodeRegistry>>,
    workload_mgr: Arc<Mutex<WorkloadManager>>,
    config: Arc<ServerConfig>,
    mut outbound_rx: SessionReceiver,
) -> ServerResult<()>
where
    S: StreamExt<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>>
        + SinkExt<WsMessage>
        + Unpin
        + Send,
    <S as futures::Sink<WsMessage>>::Error: std::fmt::Display,
{
    let (mut ws_sink, mut ws_stream) = ws_stream.split();
    let session_id = session.lock().await.id();

    info!(session_id = %session_id, "Starting session handler");

    // Channel for sending responses from read task to write task
    let (response_tx, mut response_rx) = mpsc::channel::<GatewayMessage>(32);

    // Task for reading from WebSocket
    let read_session = session.clone();
    let read_registry = registry.clone();
    let read_workload_mgr = workload_mgr.clone();
    let read_config = config.clone();

    let read_task = async move {
        while let Some(msg_result) = ws_stream.next().await {
            let ws_msg = match msg_result {
                Ok(msg) => msg,
                Err(e) => {
                    warn!(session_id = %session_id, error = %e, "WebSocket read error");
                    break;
                }
            };

            // Process the WebSocket message
            let node_msg = match process_ws_message(&ws_msg) {
                Ok(Some(msg)) => msg,
                Ok(None) => continue,
                Err(ServerError::ConnectionClosed) => break,
                Err(e) => {
                    warn!(session_id = %session_id, error = %e, "Failed to process message");
                    continue;
                }
            };

            // Update session state
            {
                let mut session = read_session.lock().await;
                session.touch();

                // If this is a registration message, update the session
                if let NodeMessage::Register { node_id, .. } = &node_msg {
                    session.set_node_id(*node_id);
                }
            }

            // Route the message to handlers
            let response = {
                let mut registry = read_registry.lock().await;
                let mut workload_mgr = read_workload_mgr.lock().await;
                route_message(&node_msg, &mut registry, &mut workload_mgr, &read_config)
            };

            // If there's a response, send it through the channel (don't exit the loop!)
            match response {
                Ok(Some(resp)) => {
                    if response_tx.send(resp).await.is_err() {
                        warn!(session_id = %session_id, "Failed to send response to channel");
                        break;
                    }
                }
                Ok(None) => {
                    // No response needed, continue the loop
                }
                Err(e) => {
                    warn!(session_id = %session_id, error = %e, "Handler error");
                    // Don't break on handler errors, just log and continue
                }
            }
        }

        Ok::<_, ServerError>(())
    };

    // Task for writing to WebSocket (handles both outbound channel and responses)
    let write_task = async {
        loop {
            tokio::select! {
                // Handle responses from read task
                Some(msg) = response_rx.recv() => {
                    let ws_msg = gateway_msg_to_ws(&msg)?;
                    if let Err(e) = ws_sink.send(ws_msg).await {
                        error!(session_id = %session_id, error = %e, "Failed to send response");
                        return Err(ServerError::WebSocket(e.to_string()));
                    }
                }
                // Handle messages from external outbound channel
                Some(msg) = outbound_rx.recv() => {
                    let ws_msg = gateway_msg_to_ws(&msg)?;
                    if let Err(e) = ws_sink.send(ws_msg).await {
                        error!(session_id = %session_id, error = %e, "Failed to send message");
                        return Err(ServerError::WebSocket(e.to_string()));
                    }
                }
                else => break,
            }
        }
        Ok::<_, ServerError>(())
    };

    // Run both tasks concurrently
    tokio::select! {
        read_result = read_task => {
            if let Err(e) = read_result {
                warn!(session_id = %session_id, error = %e, "Read task error");
            }
        }
        write_result = write_task => {
            if let Err(e) = write_result {
                warn!(session_id = %session_id, error = %e, "Write task error");
            }
        }
    }

    // Clean up
    {
        let mut session = session.lock().await;
        session.set_disconnected();

        // If the node was registered, unregister it
        if let Some(node_id) = session.node_id() {
            let mut registry = registry.lock().await;
            if let Err(e) = registry.unregister(node_id) {
                debug!(node_id = %node_id, error = %e, "Failed to unregister node");
            } else {
                info!(node_id = %node_id, "Node unregistered on session close");
            }
        }
    }

    info!(session_id = %session_id, "Session ended");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_proto::NodeCapabilities;

    // ==================== SessionState Tests ====================

    #[test]
    fn test_session_state_is_active() {
        assert!(SessionState::Connected.is_active());
        assert!(SessionState::Registered.is_active());
        assert!(!SessionState::Disconnecting.is_active());
        assert!(!SessionState::Disconnected.is_active());
    }

    // ==================== NodeSession Tests ====================

    #[test]
    fn test_node_session_new() {
        let session = NodeSession::new();

        assert!(session.node_id().is_none());
        assert_eq!(session.state(), SessionState::Connected);
        assert!(session.is_active());
        assert!(!session.is_registered());
    }

    #[test]
    fn test_node_session_default() {
        let session = NodeSession::default();

        assert!(session.node_id().is_none());
        assert_eq!(session.state(), SessionState::Connected);
    }

    #[test]
    fn test_node_session_set_node_id() {
        let mut session = NodeSession::new();
        let node_id = NodeId::new();

        session.set_node_id(node_id);

        assert_eq!(session.node_id(), Some(node_id));
        assert_eq!(session.state(), SessionState::Registered);
        assert!(session.is_registered());
    }

    #[test]
    fn test_node_session_touch() {
        let mut session = NodeSession::new();
        let before = session.last_message_at();

        std::thread::sleep(std::time::Duration::from_millis(10));
        session.touch();

        assert!(session.last_message_at() >= before);
    }

    #[test]
    fn test_node_session_disconnect() {
        let mut session = NodeSession::new();

        session.disconnect();
        assert_eq!(session.state(), SessionState::Disconnecting);
        assert!(!session.is_active());

        session.set_disconnected();
        assert_eq!(session.state(), SessionState::Disconnected);
    }

    #[test]
    fn test_node_session_id_is_unique() {
        let session1 = NodeSession::new();
        let session2 = NodeSession::new();

        assert_ne!(session1.id(), session2.id());
    }

    #[test]
    fn test_node_session_connected_at() {
        let before = Utc::now();
        let session = NodeSession::new();
        let after = Utc::now();

        assert!(session.connected_at() >= before);
        assert!(session.connected_at() <= after);
    }

    // ==================== process_ws_message Tests ====================

    #[test]
    fn test_process_ws_message_text() {
        let node_id = NodeId::new();
        let msg = NodeMessage::heartbeat(node_id);
        let json = serde_json::to_string(&msg).unwrap();
        let ws_msg = WsMessage::Text(json);

        let result = process_ws_message(&ws_msg);

        assert!(result.is_ok());
        let parsed = result.unwrap().unwrap();
        match parsed {
            NodeMessage::Heartbeat { node_id: pid, .. } => {
                assert_eq!(pid, node_id);
            }
            _ => panic!("Expected Heartbeat message"),
        }
    }

    #[test]
    fn test_process_ws_message_binary() {
        let node_id = NodeId::new();
        let msg = NodeMessage::heartbeat(node_id);
        let data = serde_json::to_vec(&msg).unwrap();
        let ws_msg = WsMessage::Binary(data);

        let result = process_ws_message(&ws_msg);

        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn test_process_ws_message_ping() {
        let ws_msg = WsMessage::Ping(vec![1, 2, 3]);
        let result = process_ws_message(&ws_msg);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_process_ws_message_pong() {
        let ws_msg = WsMessage::Pong(vec![1, 2, 3]);
        let result = process_ws_message(&ws_msg);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_process_ws_message_close() {
        let ws_msg = WsMessage::Close(None);
        let result = process_ws_message(&ws_msg);

        assert!(matches!(result, Err(ServerError::ConnectionClosed)));
    }

    #[test]
    fn test_process_ws_message_invalid_json() {
        let ws_msg = WsMessage::Text("not valid json".to_string());
        let result = process_ws_message(&ws_msg);

        assert!(matches!(result, Err(ServerError::Serialization(_))));
    }

    // ==================== gateway_msg_to_ws Tests ====================

    #[test]
    fn test_gateway_msg_to_ws() {
        let msg = GatewayMessage::heartbeat_ack();
        let result = gateway_msg_to_ws(&msg);

        assert!(result.is_ok());
        let ws_msg = result.unwrap();
        assert!(matches!(ws_msg, WsMessage::Text(_)));
    }

    #[test]
    fn test_gateway_msg_to_ws_roundtrip() {
        let node_id = NodeId::new();
        let msg = GatewayMessage::registered(node_id, 30, 10);

        let ws_msg = gateway_msg_to_ws(&msg).unwrap();
        if let WsMessage::Text(json) = ws_msg {
            let parsed: GatewayMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, msg);
        } else {
            panic!("Expected Text message");
        }
    }

    // ==================== session_channel Tests ====================

    #[test]
    fn test_session_channel() {
        let (tx, mut rx) = session_channel(10);

        // Test non-blocking: channel should be created
        assert!(tx.try_send(GatewayMessage::heartbeat_ack()).is_ok());

        // Receive should work
        let received = rx.try_recv();
        assert!(received.is_ok());
    }

    #[tokio::test]
    async fn test_session_channel_async() {
        let (tx, mut rx) = session_channel(10);
        let msg = GatewayMessage::heartbeat_ack();

        tx.send(msg.clone()).await.unwrap();
        let received = rx.recv().await.unwrap();

        assert_eq!(received, msg);
    }

    // ==================== Integration-style Tests ====================

    #[test]
    fn test_full_registration_flow() {
        let mut registry = NodeRegistry::new();
        let mut workload_mgr = WorkloadManager::new();
        let config = ServerConfig::default();
        let node_id = NodeId::new();

        // Simulate a registration message
        let register_msg = NodeMessage::register(
            node_id,
            "test-node",
            NodeCapabilities::new(8, 16384),
        );

        let result = route_message(&register_msg, &mut registry, &mut workload_mgr, &config);

        assert!(result.is_ok());
        let response = result.unwrap().unwrap();
        
        match response {
            GatewayMessage::Registered { node_id: resp_id, .. } => {
                assert_eq!(resp_id, node_id);
            }
            _ => panic!("Expected Registered response"),
        }

        // Node should be in registry
        assert!(registry.get_node(node_id).is_some());
    }

    #[test]
    fn test_heartbeat_after_registration() {
        let mut registry = NodeRegistry::new();
        let mut workload_mgr = WorkloadManager::new();
        let config = ServerConfig::default();
        let node_id = NodeId::new();

        // Register first
        let register_msg = NodeMessage::register(
            node_id,
            "test-node",
            NodeCapabilities::new(8, 16384),
        );
        route_message(&register_msg, &mut registry, &mut workload_mgr, &config).unwrap();

        // Send heartbeat
        let heartbeat_msg = NodeMessage::heartbeat(node_id);
        let result = route_message(&heartbeat_msg, &mut registry, &mut workload_mgr, &config);

        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            Some(GatewayMessage::HeartbeatAck { .. })
        ));
    }
}
