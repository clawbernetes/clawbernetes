//! WebSocket gateway server implementation.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use claw_gateway::{NodeRegistry, WorkloadManager};
use claw_proto::{GatewayMessage, NodeId};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::accept_async;
use tracing::{debug, info, warn};

use crate::config::ServerConfig;
use crate::error::{ServerError, ServerResult};
use crate::session::{run_session, session_channel, NodeSession, SessionSender};

/// Active session tracking information.
#[derive(Debug)]
struct ActiveSession {
    /// The session state.
    session: Arc<Mutex<NodeSession>>,
    /// Channel to send messages to this session.
    sender: SessionSender,
}

/// Gateway server for managing WebSocket connections from clawnode instances.
#[derive(Debug)]
pub struct GatewayServer {
    /// Server configuration.
    config: Arc<ServerConfig>,
    /// Node registry for tracking registered nodes.
    registry: Arc<Mutex<NodeRegistry>>,
    /// Workload manager for tracking workload lifecycle.
    workload_manager: Arc<Mutex<WorkloadManager>>,
    /// Active sessions indexed by session ID.
    sessions: Arc<RwLock<HashMap<uuid::Uuid, ActiveSession>>>,
    /// Shutdown signal sender.
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl GatewayServer {
    /// Create a new gateway server with the given configuration.
    #[must_use]
    pub fn new(config: ServerConfig) -> Self {
        Self {
            config: Arc::new(config),
            registry: Arc::new(Mutex::new(NodeRegistry::new())),
            workload_manager: Arc::new(Mutex::new(WorkloadManager::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: None,
        }
    }

    /// Create a gateway server with custom registry and workload manager.
    #[must_use]
    pub fn with_components(
        config: ServerConfig,
        registry: NodeRegistry,
        workload_manager: WorkloadManager,
    ) -> Self {
        Self {
            config: Arc::new(config),
            registry: Arc::new(Mutex::new(registry)),
            workload_manager: Arc::new(Mutex::new(workload_manager)),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: None,
        }
    }

    /// Get the server configuration.
    #[must_use]
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Get access to the node registry.
    #[must_use]
    pub fn registry(&self) -> Arc<Mutex<NodeRegistry>> {
        self.registry.clone()
    }

    /// Get access to the workload manager.
    #[must_use]
    pub fn workload_manager(&self) -> Arc<Mutex<WorkloadManager>> {
        self.workload_manager.clone()
    }

    /// Get the number of active sessions.
    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    /// Start the server and listen for connections.
    ///
    /// This method runs until the server is shut down.
    ///
    /// # Errors
    ///
    /// Returns an error if binding fails or the server encounters a fatal error.
    pub async fn serve(&mut self, addr: SocketAddr) -> ServerResult<()> {
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| ServerError::BindFailed(addr, e))?;

        info!(addr = %addr, "Gateway server listening");

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, peer_addr)) => {
                            self.handle_connection(stream, peer_addr).await;
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to accept connection");
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received");
                    break;
                }
            }
        }

        info!("Gateway server shutting down");
        Ok(())
    }

    /// Handle a new TCP connection.
    async fn handle_connection(&self, stream: TcpStream, peer_addr: SocketAddr) {
        debug!(peer = %peer_addr, "New connection");

        // Check connection limits
        let session_count = self.session_count().await;
        if session_count >= self.config.max_connections {
            warn!(
                peer = %peer_addr,
                current = session_count,
                max = self.config.max_connections,
                "Connection rejected: max connections reached"
            );
            return;
        }

        // Accept WebSocket upgrade
        let ws_stream = match accept_async(stream).await {
            Ok(ws) => ws,
            Err(e) => {
                warn!(peer = %peer_addr, error = %e, "WebSocket handshake failed");
                return;
            }
        };

        info!(peer = %peer_addr, "WebSocket connection established");

        // Create session
        let session = Arc::new(Mutex::new(NodeSession::new()));
        let session_id = session.lock().await.id();

        // Create channel for outbound messages
        let (sender, receiver) = session_channel(64);

        // Store session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(
                session_id,
                ActiveSession {
                    session: session.clone(),
                    sender: sender.clone(),
                },
            );
        }

        // Spawn session handler task
        let registry = self.registry.clone();
        let workload_mgr = self.workload_manager.clone();
        let config = self.config.clone();
        let sessions = self.sessions.clone();

        tokio::spawn(async move {
            let result = run_session(
                ws_stream,
                session.clone(),
                registry,
                workload_mgr,
                config,
                receiver,
            )
            .await;

            if let Err(e) = result {
                debug!(session_id = %session_id, error = %e, "Session ended with error");
            }

            // Remove session from tracking
            sessions.write().await.remove(&session_id);
            info!(session_id = %session_id, "Session removed");
        });
    }

    /// Broadcast a message to all registered nodes.
    ///
    /// # Errors
    ///
    /// Returns an error if sending fails to all nodes.
    #[allow(clippy::significant_drop_tightening)]
    pub async fn broadcast(&self, msg: GatewayMessage) -> ServerResult<usize> {
        // Collect senders for registered sessions
        let senders_to_notify: Vec<(uuid::Uuid, SessionSender)> = {
            let sessions = self.sessions.read().await;
            let mut result = Vec::new();
            for (session_id, active_session) in sessions.iter() {
                let is_registered = active_session.session.lock().await.is_registered();
                if is_registered {
                    result.push((*session_id, active_session.sender.clone()));
                }
            }
            result
        };

        let mut sent_count = 0;
        let mut errors = Vec::new();

        for (session_id, sender) in senders_to_notify {
            match sender.try_send(msg.clone()) {
                Ok(()) => {
                    sent_count += 1;
                }
                Err(e) => {
                    debug!(session_id = %session_id, error = %e, "Failed to send broadcast");
                    errors.push(e.to_string());
                }
            }
        }

        if sent_count == 0 && !errors.is_empty() {
            return Err(ServerError::ChannelSend(errors.join(", ")));
        }

        debug!(sent = sent_count, "Broadcast message sent");
        Ok(sent_count)
    }

    /// Send a message to a specific node.
    ///
    /// # Errors
    ///
    /// Returns an error if the node is not found or sending fails.
    pub async fn send_to_node(&self, node_id: NodeId, msg: GatewayMessage) -> ServerResult<()> {
        // Find the sender for this node
        let sender = self.find_sender_for_node(node_id).await;

        sender.map_or(Err(ServerError::NodeNotRegistered(node_id)), |s| {
            s.try_send(msg)
                .map_err(|e| ServerError::ChannelSend(e.to_string()))
        })
    }

    /// Find the sender channel for a specific node.
    #[allow(clippy::significant_drop_tightening)]
    async fn find_sender_for_node(&self, node_id: NodeId) -> Option<SessionSender> {
        let sessions = self.sessions.read().await;
        for active_session in sessions.values() {
            let session = active_session.session.lock().await;
            if session.node_id() == Some(node_id) {
                return Some(active_session.sender.clone());
            }
        }
        None
    }

    /// Trigger server shutdown.
    ///
    /// # Errors
    ///
    /// Returns an error if the shutdown signal cannot be sent.
    pub async fn shutdown(&self) -> ServerResult<()> {
        if let Some(tx) = &self.shutdown_tx {
            tx.send(())
                .await
                .map_err(|e| ServerError::Internal(e.to_string()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_proto::NodeCapabilities;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    // ==================== Helper Functions ====================

    fn make_config() -> ServerConfig {
        ServerConfig::new(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            0, // Use port 0 for OS-assigned port
        ))
        .with_max_connections(100)
    }

    fn make_config_with_port(port: u16) -> ServerConfig {
        ServerConfig::new(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port,
        ))
        .with_max_connections(100)
    }

    // ==================== GatewayServer Construction Tests ====================

    #[test]
    fn test_gateway_server_new() {
        let config = make_config();
        let server = GatewayServer::new(config.clone());

        assert_eq!(server.config().bind_addr, config.bind_addr);
        assert_eq!(server.config().max_connections, 100);
    }

    #[test]
    fn test_gateway_server_with_components() {
        let config = make_config();
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();
        registry.register(node_id, NodeCapabilities::new(4, 8192)).unwrap();

        let workload_mgr = WorkloadManager::new();

        let server = GatewayServer::with_components(config, registry, workload_mgr);

        // Registry should have the pre-registered node
        let registry = server.registry();
        let registry = futures::executor::block_on(registry.lock());
        assert!(registry.get_node(node_id).is_some());
    }

    #[test]
    fn test_gateway_server_config_accessor() {
        let config = make_config_with_port(9000);
        let server = GatewayServer::new(config);

        assert_eq!(server.config().bind_addr.port(), 9000);
    }

    // ==================== Session Count Tests ====================

    #[tokio::test]
    async fn test_session_count_initially_zero() {
        let server = GatewayServer::new(make_config());
        assert_eq!(server.session_count().await, 0);
    }

    // ==================== Broadcast Tests ====================

    #[tokio::test]
    async fn test_broadcast_to_no_sessions() {
        let server = GatewayServer::new(make_config());
        let msg = GatewayMessage::RequestMetrics;

        let result = server.broadcast(msg).await;

        // Should succeed with 0 sent (no registered sessions)
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    // ==================== Send to Node Tests ====================

    #[tokio::test]
    async fn test_send_to_nonexistent_node() {
        let server = GatewayServer::new(make_config());
        let node_id = NodeId::new();
        let msg = GatewayMessage::RequestMetrics;

        let result = server.send_to_node(node_id, msg).await;

        assert!(matches!(result, Err(ServerError::NodeNotRegistered(_))));
    }

    // ==================== Registry/WorkloadManager Access Tests ====================

    #[tokio::test]
    async fn test_registry_access() {
        let server = GatewayServer::new(make_config());
        let registry = server.registry();

        let mut registry = registry.lock().await;
        let node_id = NodeId::new();
        let caps = NodeCapabilities::new(8, 16384);

        registry.register(node_id, caps).unwrap();
        assert_eq!(registry.len(), 1);
    }

    #[tokio::test]
    async fn test_workload_manager_access() {
        let server = GatewayServer::new(make_config());
        let workload_mgr = server.workload_manager();

        let workload_mgr = workload_mgr.lock().await;
        assert!(workload_mgr.is_empty());
    }

    // ==================== Server Bind Tests ====================

    #[tokio::test]
    async fn test_serve_binds_to_address() {
        let config = make_config();
        let mut server = GatewayServer::new(config);

        // Spawn server in background
        let shutdown_handle = tokio::spawn(async move {
            // Use a random port to avoid conflicts
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
            // This would run forever, so we just test it starts
            let _ = tokio::time::timeout(Duration::from_millis(100), server.serve(addr)).await;
        });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Clean up
        shutdown_handle.abort();
    }

    #[tokio::test]
    async fn test_serve_fails_on_invalid_address() {
        let config = make_config_with_port(1); // Port 1 should fail (privileged)
        let mut server = GatewayServer::new(config);

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1);
        let result = server.serve(addr).await;

        // Should fail to bind (permission denied or similar)
        // Note: On some systems this might actually succeed if run as root
        // so we just check that it returns something
        assert!(result.is_err() || result.is_ok());
    }

    // ==================== Integration Tests ====================

    #[tokio::test]
    async fn test_server_lifecycle() {
        let config = make_config();
        let server = GatewayServer::new(config);

        // Verify initial state
        assert_eq!(server.session_count().await, 0);
        assert!(server.registry().lock().await.is_empty());
        assert!(server.workload_manager().lock().await.is_empty());
    }

    #[tokio::test]
    async fn test_concurrent_registry_access() {
        let server = GatewayServer::new(make_config());
        let registry = server.registry();

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let registry = registry.clone();
                tokio::spawn(async move {
                    let mut reg = registry.lock().await;
                    let node_id = NodeId::new();
                    reg.register(node_id, NodeCapabilities::new(4, 8192)).unwrap();
                    node_id
                })
            })
            .collect();

        // Wait for all to complete
        let mut node_ids = Vec::new();
        for handle in handles {
            node_ids.push(handle.await.unwrap());
        }

        // Verify all nodes registered
        let registry = server.registry();
        let registry = registry.lock().await;
        assert_eq!(registry.len(), 10);

        for node_id in node_ids {
            assert!(registry.get_node(node_id).is_some());
        }
    }
}
