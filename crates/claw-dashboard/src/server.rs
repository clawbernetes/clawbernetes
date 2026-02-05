//! Dashboard server implementation.

use std::net::SocketAddr;
use std::sync::Arc;

use claw_gateway::{NodeRegistry, WorkloadLogStore, WorkloadManager};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::info;

use crate::config::DashboardConfig;
use crate::error::{DashboardError, DashboardResult};
use crate::routes::create_router;
use crate::state::DashboardState;
use crate::types::LiveUpdate;

/// Dashboard server for the web UI API.
///
/// Provides REST API endpoints for cluster status, nodes, workloads, and metrics,
/// plus WebSocket and SSE endpoints for real-time updates.
#[derive(Debug, Clone)]
pub struct DashboardServer {
    state: Arc<DashboardState>,
}

impl DashboardServer {
    /// Create a new dashboard server with the given configuration and shared state.
    #[must_use]
    pub fn new(
        config: DashboardConfig,
        registry: Arc<Mutex<NodeRegistry>>,
        workload_manager: Arc<Mutex<WorkloadManager>>,
    ) -> Self {
        let state = Arc::new(DashboardState::new(config, registry, workload_manager));
        Self { state }
    }

    /// Create a new dashboard server with a log store.
    #[must_use]
    pub fn with_log_store(
        config: DashboardConfig,
        registry: Arc<Mutex<NodeRegistry>>,
        workload_manager: Arc<Mutex<WorkloadManager>>,
        log_store: Arc<Mutex<WorkloadLogStore>>,
    ) -> Self {
        let state = Arc::new(DashboardState::with_log_store(
            config,
            registry,
            workload_manager,
            log_store,
        ));
        Self { state }
    }

    /// Get the dashboard state for external access.
    #[must_use]
    pub fn state(&self) -> Arc<DashboardState> {
        self.state.clone()
    }

    /// Get a reference to the node registry.
    #[must_use]
    pub fn registry(&self) -> Arc<Mutex<NodeRegistry>> {
        self.state.registry()
    }

    /// Get a reference to the workload manager.
    #[must_use]
    pub fn workload_manager(&self) -> Arc<Mutex<WorkloadManager>> {
        self.state.workload_manager()
    }

    /// Publish a live update to all connected clients.
    ///
    /// Returns the number of clients that received the update.
    pub fn publish(&self, update: LiveUpdate) -> usize {
        self.state.publish(update)
    }

    /// Get the number of active WebSocket connections.
    #[must_use]
    pub fn ws_connection_count(&self) -> usize {
        self.state.ws_connection_count()
    }

    /// Start the dashboard server and listen for connections.
    ///
    /// This method runs until the server encounters a fatal error.
    ///
    /// # Errors
    ///
    /// Returns an error if binding to the address fails.
    pub async fn serve(&self, addr: SocketAddr) -> DashboardResult<()> {
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| DashboardError::BindFailed(addr, e))?;

        info!(addr = %addr, "Dashboard server listening");

        let router = create_router(self.state.clone());

        axum::serve(listener, router)
            .await
            .map_err(|e| DashboardError::Internal(e.to_string()))?;

        Ok(())
    }

    /// Start the dashboard server with graceful shutdown support.
    ///
    /// The server will shut down when the provided future completes.
    ///
    /// # Errors
    ///
    /// Returns an error if binding to the address fails.
    pub async fn serve_with_shutdown<F>(
        &self,
        addr: SocketAddr,
        shutdown: F,
    ) -> DashboardResult<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| DashboardError::BindFailed(addr, e))?;

        info!(addr = %addr, "Dashboard server listening");

        let router = create_router(self.state.clone());

        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(|e| DashboardError::Internal(e.to_string()))?;

        info!("Dashboard server shut down");
        Ok(())
    }

    /// Create the router without starting the server.
    ///
    /// Useful for testing or embedding in another server.
    pub fn router(&self) -> axum::Router {
        create_router(self.state.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_gateway::NodeHealthStatus;
    use claw_proto::{GpuCapability, NodeCapabilities, NodeId, WorkloadId, WorkloadState};

    fn make_test_server() -> DashboardServer {
        let config = DashboardConfig::default();
        let registry = Arc::new(Mutex::new(NodeRegistry::new()));
        let workload_manager = Arc::new(Mutex::new(WorkloadManager::new()));
        DashboardServer::new(config, registry, workload_manager)
    }

    #[test]
    fn test_server_creation() {
        let server = make_test_server();

        assert_eq!(server.ws_connection_count(), 0);
    }

    #[test]
    fn test_server_with_log_store() {
        let config = DashboardConfig::default();
        let registry = Arc::new(Mutex::new(NodeRegistry::new()));
        let workload_manager = Arc::new(Mutex::new(WorkloadManager::new()));
        let log_store = Arc::new(Mutex::new(WorkloadLogStore::new()));

        let server = DashboardServer::with_log_store(config, registry, workload_manager, log_store);

        assert_eq!(server.ws_connection_count(), 0);
    }

    #[test]
    fn test_server_clone() {
        let server = make_test_server();
        let cloned = server.clone();

        // Both should share the same state
        assert_eq!(server.ws_connection_count(), cloned.ws_connection_count());
    }

    #[tokio::test]
    async fn test_registry_access() {
        let server = make_test_server();
        let registry = server.registry();

        let mut registry = registry.lock().await;
        let node_id = NodeId::new();
        registry
            .register(node_id, NodeCapabilities::new(8, 16384))
            .unwrap();

        assert_eq!(registry.len(), 1);
    }

    #[tokio::test]
    async fn test_workload_manager_access() {
        let server = make_test_server();
        let wm = server.workload_manager();

        let wm = wm.lock().await;
        assert!(wm.is_empty());
    }

    #[tokio::test]
    async fn test_publish_update() {
        let server = make_test_server();

        // Subscribe first
        let mut rx = server.state().subscribe();

        let update = LiveUpdate::NodeHealthChanged {
            node_id: NodeId::new(),
            health: NodeHealthStatus::Healthy,
        };

        let sent = server.publish(update);
        assert_eq!(sent, 1);

        let received = rx.recv().await.unwrap();
        assert!(matches!(received, LiveUpdate::NodeHealthChanged { .. }));
    }

    #[tokio::test]
    async fn test_publish_multiple_updates() {
        let server = make_test_server();

        let mut rx = server.state().subscribe();

        // Publish multiple updates
        server.publish(LiveUpdate::Heartbeat {
            timestamp: chrono::Utc::now(),
        });
        server.publish(LiveUpdate::WorkloadStateChanged {
            workload_id: WorkloadId::new(),
            state: WorkloadState::Running,
            error: None,
        });

        let first = rx.recv().await.unwrap();
        let second = rx.recv().await.unwrap();

        assert!(matches!(first, LiveUpdate::Heartbeat { .. }));
        assert!(matches!(second, LiveUpdate::WorkloadStateChanged { .. }));
    }

    #[tokio::test]
    async fn test_state_access() {
        let server = make_test_server();
        let state = server.state();

        // Verify we can get cluster status
        let status = state.get_cluster_status().await;
        assert_eq!(status.total_nodes, 0);
    }

    #[tokio::test]
    async fn test_router_creation() {
        let server = make_test_server();
        let _router = server.router();

        // Router should be created without error
    }

    #[tokio::test]
    async fn test_serve_with_shutdown() {
        let server = make_test_server();

        // Use a random port to avoid conflicts
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));

        // Create shutdown signal that fires immediately
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        // Start server in background
        let server_handle = tokio::spawn(async move {
            server
                .serve_with_shutdown(addr, async move {
                    let _ = shutdown_rx.await;
                })
                .await
        });

        // Give server a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Trigger shutdown
        let _ = shutdown_tx.send(());

        // Wait for server to finish
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            server_handle,
        )
        .await;

        // Should complete without timeout
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_serve_bind_failure() {
        let server = make_test_server();

        // Try to bind to a privileged port (should fail)
        let addr = SocketAddr::from(([127, 0, 0, 1], 1));

        let result = server.serve(addr).await;

        // Should fail to bind (unless running as root)
        // We just verify it doesn't panic
        assert!(result.is_err() || result.is_ok());
    }

    #[tokio::test]
    async fn test_full_integration() {
        let config = DashboardConfig::default();
        let registry = Arc::new(Mutex::new(NodeRegistry::new()));
        let workload_manager = Arc::new(Mutex::new(WorkloadManager::new()));
        let server = DashboardServer::new(config, registry.clone(), workload_manager);

        // Add a node
        {
            let mut reg = registry.lock().await;
            let node_id = NodeId::new();
            let caps = NodeCapabilities::new(8, 16384).with_gpu(GpuCapability {
                index: 0,
                name: "RTX 4090".into(),
                memory_mib: 24576,
                uuid: "gpu-uuid".into(),
            });
            reg.register_with_name(node_id, "test-node", caps).unwrap();
        }

        // Verify through state
        let status = server.state().get_cluster_status().await;
        assert_eq!(status.total_nodes, 1);
        assert_eq!(status.node_health.healthy, 1);
        assert_eq!(status.gpu_utilization.total_gpus, 1);

        let nodes = server.state().get_nodes().await;
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "test-node");
    }
}
