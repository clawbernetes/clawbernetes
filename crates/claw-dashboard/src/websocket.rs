//! WebSocket handler for real-time updates.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tokio::time::interval;
use tracing::{debug, info, warn};

use crate::error::DashboardError;
use crate::state::DashboardState;
use crate::types::LiveUpdate;

/// Handle WebSocket upgrade request for /api/ws.
pub async fn ws_upgrade(
    State(state): State<Arc<DashboardState>>,
    ws: WebSocketUpgrade,
) -> Result<Response, DashboardError> {
    // Check connection limit
    if !state.add_ws_connection() {
        let current = state.ws_connection_count();
        let max = state.config().max_ws_connections;
        return Err(DashboardError::TooManyConnections(current, max));
    }

    Ok(ws.on_upgrade(move |socket| handle_ws_connection(socket, state)))
}

/// Handle an active WebSocket connection.
async fn handle_ws_connection(socket: WebSocket, state: Arc<DashboardState>) {
    let (mut sender, mut receiver) = socket.split();

    let update_rx = state.subscribe();
    let ping_interval = state.config().ws_ping_interval;

    // Spawn task to forward updates to WebSocket
    let send_task = tokio::spawn(async move {
        forward_updates_to_ws(&mut sender, update_rx, ping_interval).await;
    });

    // Handle incoming messages (for ping/pong and potential commands)
    let recv_task = tokio::spawn(async move {
        handle_incoming_messages(&mut receiver).await;
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {
            debug!("WebSocket send task completed");
        }
        _ = recv_task => {
            debug!("WebSocket receive task completed");
        }
    }

    // Clean up connection tracking
    state.remove_ws_connection();
    info!("WebSocket connection closed");
}

/// Forward live updates to the WebSocket.
async fn forward_updates_to_ws(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    mut update_rx: broadcast::Receiver<LiveUpdate>,
    ping_interval: Duration,
) {
    let mut ping_timer = interval(ping_interval);

    loop {
        tokio::select! {
            // Send live updates
            update_result = update_rx.recv() => {
                match update_result {
                    Ok(update) => {
                        match serde_json::to_string(&update) {
                            Ok(json) => {
                                if sender.send(Message::Text(json.into())).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!(error = %e, "Failed to serialize update");
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(count = n, "WebSocket receiver lagged, dropped messages");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }

            // Send periodic heartbeats
            _ = ping_timer.tick() => {
                let heartbeat = LiveUpdate::Heartbeat {
                    timestamp: Utc::now(),
                };
                match serde_json::to_string(&heartbeat) {
                    Ok(json) => {
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to serialize heartbeat");
                    }
                }
            }
        }
    }
}

/// Handle incoming WebSocket messages.
async fn handle_incoming_messages(
    receiver: &mut futures::stream::SplitStream<WebSocket>,
) {
    while let Some(msg_result) = receiver.next().await {
        match msg_result {
            Ok(Message::Ping(data)) => {
                debug!("Received ping");
                // Pong is handled automatically by axum
                let _ = data;
            }
            Ok(Message::Pong(_)) => {
                debug!("Received pong");
            }
            Ok(Message::Close(_)) => {
                debug!("Received close frame");
                break;
            }
            Ok(Message::Text(text)) => {
                // Could handle commands here in the future
                debug!(message = %text, "Received text message");
            }
            Ok(Message::Binary(_)) => {
                debug!("Received binary message (ignored)");
            }
            Err(e) => {
                warn!(error = %e, "WebSocket receive error");
                break;
            }
        }
    }
}

/// Client message for potential future commands.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// Subscribe to specific update types.
    Subscribe {
        /// Update types to subscribe to.
        events: Vec<String>,
    },
    /// Unsubscribe from specific update types.
    Unsubscribe {
        /// Update types to unsubscribe from.
        events: Vec<String>,
    },
    /// Request current status.
    GetStatus,
    /// Ping message for keepalive.
    Ping,
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_gateway::{NodeRegistry, WorkloadManager};
    use tokio::sync::Mutex;

    fn make_test_state() -> Arc<DashboardState> {
        let config = crate::config::DashboardConfig::default();
        let registry = Arc::new(Mutex::new(NodeRegistry::new()));
        let workload_manager = Arc::new(Mutex::new(WorkloadManager::new()));
        Arc::new(DashboardState::new(config, registry, workload_manager))
    }

    #[test]
    fn test_client_message_subscribe_deserialization() {
        let json = r#"{"type":"Subscribe","events":["node_health_changed","workload_state_changed"]}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();

        match msg {
            ClientMessage::Subscribe { events } => {
                assert_eq!(events.len(), 2);
                assert!(events.contains(&"node_health_changed".to_string()));
            }
            _ => panic!("Expected Subscribe message"),
        }
    }

    #[test]
    fn test_client_message_get_status_deserialization() {
        let json = r#"{"type":"GetStatus"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();

        assert!(matches!(msg, ClientMessage::GetStatus));
    }

    #[test]
    fn test_client_message_ping_deserialization() {
        let json = r#"{"type":"Ping"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();

        assert!(matches!(msg, ClientMessage::Ping));
    }

    #[tokio::test]
    async fn test_ws_connection_tracking() {
        let state = make_test_state();

        // Add connection
        assert!(state.add_ws_connection());
        assert_eq!(state.ws_connection_count(), 1);

        // Remove connection
        state.remove_ws_connection();
        assert_eq!(state.ws_connection_count(), 0);
    }

    #[tokio::test]
    async fn test_ws_connection_limit() {
        let config = crate::config::DashboardConfig::default().with_max_ws_connections(2);
        let registry = Arc::new(Mutex::new(NodeRegistry::new()));
        let workload_manager = Arc::new(Mutex::new(WorkloadManager::new()));
        let state = Arc::new(DashboardState::new(config, registry, workload_manager));

        assert!(state.add_ws_connection());
        assert!(state.add_ws_connection());
        assert!(!state.add_ws_connection()); // Should fail

        state.remove_ws_connection();
        assert!(state.add_ws_connection()); // Should succeed now
    }

    #[tokio::test]
    async fn test_live_update_serialization() {
        let update = LiveUpdate::Heartbeat {
            timestamp: Utc::now(),
        };

        let json = serde_json::to_string(&update).unwrap();
        // The serde tag serialization uses PascalCase by default
        assert!(json.contains("Heartbeat"));

        let deserialized: LiveUpdate = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, LiveUpdate::Heartbeat { .. }));
    }
}
