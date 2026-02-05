//! Shared state for the dashboard server.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use claw_gateway::{NodeRegistry, WorkloadLogStore, WorkloadManager};
use claw_proto::{NodeId, WorkloadId, WorkloadState};
use chrono::Utc;
use tokio::sync::{broadcast, Mutex, RwLock};

use crate::config::DashboardConfig;
use crate::types::{
    ClusterStatus, GpuUtilizationSummary, LiveUpdate, MetricsSnapshot, NodeHealthSummary,
    NodeMetrics, NodeStatus, WorkloadStateSummary, WorkloadStatus,
};

/// Shared state for the dashboard server.
#[derive(Debug)]
pub struct DashboardState {
    /// Dashboard configuration.
    config: Arc<DashboardConfig>,
    /// Node registry (shared with gateway).
    registry: Arc<Mutex<NodeRegistry>>,
    /// Workload manager (shared with gateway).
    workload_manager: Arc<Mutex<WorkloadManager>>,
    /// Workload log store (shared with gateway).
    log_store: Arc<Mutex<WorkloadLogStore>>,
    /// Broadcast channel for live updates.
    update_tx: broadcast::Sender<LiveUpdate>,
    /// Number of active WebSocket connections.
    ws_connections: AtomicUsize,
    /// Server start time.
    start_time: Instant,
    /// Cached metrics (`node_id` -> last metrics).
    cached_metrics: Arc<RwLock<HashMap<NodeId, NodeMetrics>>>,
}

impl DashboardState {
    /// Create a new dashboard state.
    pub fn new(
        config: DashboardConfig,
        registry: Arc<Mutex<NodeRegistry>>,
        workload_manager: Arc<Mutex<WorkloadManager>>,
    ) -> Self {
        let (update_tx, _) = broadcast::channel(1024);
        Self {
            config: Arc::new(config),
            registry,
            workload_manager,
            log_store: Arc::new(Mutex::new(WorkloadLogStore::new())),
            update_tx,
            ws_connections: AtomicUsize::new(0),
            start_time: Instant::now(),
            cached_metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new dashboard state with a log store.
    pub fn with_log_store(
        config: DashboardConfig,
        registry: Arc<Mutex<NodeRegistry>>,
        workload_manager: Arc<Mutex<WorkloadManager>>,
        log_store: Arc<Mutex<WorkloadLogStore>>,
    ) -> Self {
        let (update_tx, _) = broadcast::channel(1024);
        Self {
            config: Arc::new(config),
            registry,
            workload_manager,
            log_store,
            update_tx,
            ws_connections: AtomicUsize::new(0),
            start_time: Instant::now(),
            cached_metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the configuration.
    #[must_use]
    pub fn config(&self) -> &DashboardConfig {
        &self.config
    }

    /// Get a reference to the node registry.
    #[must_use]
    pub fn registry(&self) -> Arc<Mutex<NodeRegistry>> {
        self.registry.clone()
    }

    /// Get a reference to the workload manager.
    #[must_use]
    pub fn workload_manager(&self) -> Arc<Mutex<WorkloadManager>> {
        self.workload_manager.clone()
    }

    /// Get a reference to the log store.
    #[must_use]
    pub fn log_store(&self) -> Arc<Mutex<WorkloadLogStore>> {
        self.log_store.clone()
    }

    /// Subscribe to live updates.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<LiveUpdate> {
        self.update_tx.subscribe()
    }

    /// Publish a live update to all subscribers.
    ///
    /// Returns the number of subscribers that received the update.
    pub fn publish(&self, update: LiveUpdate) -> usize {
        self.update_tx.send(update).unwrap_or(0)
    }

    /// Get the number of active WebSocket connections.
    #[must_use]
    pub fn ws_connection_count(&self) -> usize {
        self.ws_connections.load(Ordering::Relaxed)
    }

    /// Increment the WebSocket connection count.
    ///
    /// Returns `true` if the connection was allowed, `false` if limit reached.
    pub fn add_ws_connection(&self) -> bool {
        let current = self.ws_connections.fetch_add(1, Ordering::Relaxed);
        if current >= self.config.max_ws_connections {
            self.ws_connections.fetch_sub(1, Ordering::Relaxed);
            return false;
        }
        true
    }

    /// Decrement the WebSocket connection count.
    pub fn remove_ws_connection(&self) {
        self.ws_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get server uptime in seconds.
    #[must_use]
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Get the complete cluster status.
    pub async fn get_cluster_status(&self) -> ClusterStatus {
        let registry = self.registry.lock().await;
        let workload_manager = self.workload_manager.lock().await;

        let health_summary = registry.health_summary();
        let node_health = NodeHealthSummary::from(health_summary);

        let mut workload_states = WorkloadStateSummary::default();
        for workload in workload_manager.list_workloads() {
            workload_states.add(workload.state());
        }

        let gpu_utilization = self.calculate_gpu_utilization(&registry).await;

        ClusterStatus {
            total_nodes: registry.len(),
            node_health,
            total_workloads: workload_manager.len(),
            workload_states,
            gpu_utilization,
            timestamp: Utc::now(),
            uptime_secs: self.uptime_secs(),
        }
    }

    /// Get the status of all nodes.
    pub async fn get_nodes(&self) -> Vec<NodeStatus> {
        let registry = self.registry.lock().await;
        let workload_manager = self.workload_manager.lock().await;

        registry
            .list_nodes()
            .into_iter()
            .map(|node| {
                let workload_count = workload_manager.list_by_node(node.id).len();
                NodeStatus::from_registered(node, workload_count)
            })
            .collect()
    }

    /// Get the status of a specific node.
    pub async fn get_node(&self, node_id: NodeId) -> Option<NodeStatus> {
        let registry = self.registry.lock().await;
        let workload_manager = self.workload_manager.lock().await;

        registry.get_node(node_id).map(|node| {
            let workload_count = workload_manager.list_by_node(node.id).len();
            NodeStatus::from_registered(node, workload_count)
        })
    }

    /// Get the status of all workloads.
    pub async fn get_workloads(&self) -> Vec<WorkloadStatus> {
        let workload_manager = self.workload_manager.lock().await;

        workload_manager
            .list_workloads()
            .into_iter()
            .map(WorkloadStatus::from_tracked)
            .collect()
    }

    /// Get the status of a specific workload.
    pub async fn get_workload(&self, workload_id: WorkloadId) -> Option<WorkloadStatus> {
        let workload_manager = self.workload_manager.lock().await;

        workload_manager.get_workload(workload_id).map(WorkloadStatus::from_tracked)
    }

    /// Get workloads filtered by state.
    pub async fn get_workloads_by_state(&self, state: WorkloadState) -> Vec<WorkloadStatus> {
        let workload_manager = self.workload_manager.lock().await;

        workload_manager
            .list_by_state(state)
            .into_iter()
            .map(WorkloadStatus::from_tracked)
            .collect()
    }

    /// Get the current metrics snapshot.
    pub async fn get_metrics(&self) -> MetricsSnapshot {
        let registry = self.registry.lock().await;
        let cached = self.cached_metrics.read().await;

        let nodes: Vec<NodeMetrics> = registry
            .list_nodes()
            .into_iter()
            .map(|node| {
                cached.get(&node.id).cloned().unwrap_or_else(|| NodeMetrics {
                    node_id: node.id,
                    name: node.name.clone(),
                    health: node.health_status(),
                    gpus: Vec::new(),
                })
            })
            .collect();

        let gpu_summary = self.calculate_gpu_utilization(&registry).await;

        MetricsSnapshot {
            timestamp: Utc::now(),
            nodes,
            gpu_summary,
        }
    }

    /// Update cached metrics for a node.
    pub async fn update_node_metrics(&self, metrics: NodeMetrics) {
        let mut cached = self.cached_metrics.write().await;
        cached.insert(metrics.node_id, metrics);
    }

    /// Calculate GPU utilization summary from the registry.
    async fn calculate_gpu_utilization(&self, registry: &NodeRegistry) -> GpuUtilizationSummary {
        let cached = self.cached_metrics.read().await;

        let mut total_gpus = 0usize;
        let mut total_vram: u64 = 0;
        let mut used_vram: u64 = 0;
        let mut total_utilization: u32 = 0;

        for node in registry.list_nodes() {
            total_gpus += node.capabilities.gpus.len();
            for gpu in &node.capabilities.gpus {
                total_vram += gpu.memory_mib;
            }

            if let Some(metrics) = cached.get(&node.id) {
                for gpu in &metrics.gpus {
                    used_vram += gpu.memory_used_mib;
                    total_utilization += u32::from(gpu.utilization_percent);
                }
            }
        }

        let avg_utilization = if total_gpus > 0 {
            #[allow(clippy::cast_precision_loss)]
            let avg = total_utilization as f32 / total_gpus as f32;
            avg
        } else {
            0.0
        };

        GpuUtilizationSummary {
            total_gpus,
            total_vram_mib: total_vram,
            used_vram_mib: used_vram,
            avg_utilization_percent: avg_utilization,
        }
    }

    /// Get logs for a workload.
    ///
    /// Returns combined stdout and stderr logs, interleaved in the order they were received.
    pub async fn get_workload_logs(&self, workload_id: WorkloadId, limit: Option<usize>) -> Vec<String> {
        let log_store = self.log_store.lock().await;

        if let Some((stdout, stderr)) = log_store.get_logs_with_tail(workload_id, limit) {
            // Combine stdout and stderr - in a real implementation we'd interleave by timestamp
            // For now, just append stderr after stdout
            let mut combined = stdout;
            combined.extend(stderr);
            combined
        } else {
            Vec::new()
        }
    }
}

impl Clone for DashboardState {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            registry: self.registry.clone(),
            workload_manager: self.workload_manager.clone(),
            log_store: self.log_store.clone(),
            update_tx: self.update_tx.clone(),
            ws_connections: AtomicUsize::new(self.ws_connections.load(Ordering::Relaxed)),
            start_time: self.start_time,
            cached_metrics: self.cached_metrics.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_proto::{GpuCapability, NodeCapabilities};

    fn make_test_state() -> DashboardState {
        let config = DashboardConfig::default();
        let registry = Arc::new(Mutex::new(NodeRegistry::new()));
        let workload_manager = Arc::new(Mutex::new(WorkloadManager::new()));
        DashboardState::new(config, registry, workload_manager)
    }

    #[tokio::test]
    async fn test_state_creation() {
        let state = make_test_state();

        assert_eq!(state.ws_connection_count(), 0);
        assert!(state.uptime_secs() < 2);
    }

    #[tokio::test]
    async fn test_ws_connection_tracking() {
        let state = make_test_state();

        assert!(state.add_ws_connection());
        assert_eq!(state.ws_connection_count(), 1);

        assert!(state.add_ws_connection());
        assert_eq!(state.ws_connection_count(), 2);

        state.remove_ws_connection();
        assert_eq!(state.ws_connection_count(), 1);
    }

    #[tokio::test]
    async fn test_ws_connection_limit() {
        let config = DashboardConfig::default().with_max_ws_connections(2);
        let registry = Arc::new(Mutex::new(NodeRegistry::new()));
        let workload_manager = Arc::new(Mutex::new(WorkloadManager::new()));
        let state = DashboardState::new(config, registry, workload_manager);

        assert!(state.add_ws_connection());
        assert!(state.add_ws_connection());
        assert!(!state.add_ws_connection()); // Should fail, limit reached

        assert_eq!(state.ws_connection_count(), 2);

        state.remove_ws_connection();
        assert!(state.add_ws_connection()); // Should succeed now
    }

    #[tokio::test]
    async fn test_publish_subscribe() {
        let state = make_test_state();

        let mut rx = state.subscribe();

        let update = LiveUpdate::Heartbeat {
            timestamp: Utc::now(),
        };

        let sent = state.publish(update.clone());
        assert_eq!(sent, 1);

        let received = rx.recv().await.unwrap();
        assert!(matches!(received, LiveUpdate::Heartbeat { .. }));
    }

    #[tokio::test]
    async fn test_get_cluster_status_empty() {
        let state = make_test_state();

        let status = state.get_cluster_status().await;

        assert_eq!(status.total_nodes, 0);
        assert_eq!(status.total_workloads, 0);
        assert_eq!(status.node_health.healthy, 0);
    }

    #[tokio::test]
    async fn test_get_cluster_status_with_nodes() {
        let state = make_test_state();

        // Add a node
        {
            let mut registry = state.registry.lock().await;
            let node_id = NodeId::new();
            let caps = NodeCapabilities::new(8, 16384).with_gpu(GpuCapability {
                index: 0,
                name: "RTX 4090".into(),
                memory_mib: 24576,
                uuid: "gpu-uuid".into(),
            });
            registry.register(node_id, caps).unwrap();
        }

        let status = state.get_cluster_status().await;

        assert_eq!(status.total_nodes, 1);
        assert_eq!(status.node_health.healthy, 1);
        assert_eq!(status.gpu_utilization.total_gpus, 1);
        assert_eq!(status.gpu_utilization.total_vram_mib, 24576);
    }

    #[tokio::test]
    async fn test_get_nodes() {
        let state = make_test_state();
        let node_id = NodeId::new();

        {
            let mut registry = state.registry.lock().await;
            registry
                .register_with_name(node_id, "test-node", NodeCapabilities::new(8, 16384))
                .unwrap();
        }

        let nodes = state.get_nodes().await;

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, node_id);
        assert_eq!(nodes[0].name, "test-node");
    }

    #[tokio::test]
    async fn test_get_node_not_found() {
        let state = make_test_state();

        let node = state.get_node(NodeId::new()).await;

        assert!(node.is_none());
    }

    #[tokio::test]
    async fn test_get_workloads_empty() {
        let state = make_test_state();

        let workloads = state.get_workloads().await;

        assert!(workloads.is_empty());
    }

    #[tokio::test]
    async fn test_get_metrics_empty() {
        let state = make_test_state();

        let metrics = state.get_metrics().await;

        assert!(metrics.nodes.is_empty());
        assert_eq!(metrics.gpu_summary.total_gpus, 0);
    }

    #[tokio::test]
    async fn test_update_node_metrics() {
        let state = make_test_state();
        let node_id = NodeId::new();

        // Add node to registry
        {
            let mut registry = state.registry.lock().await;
            registry
                .register(node_id, NodeCapabilities::new(8, 16384))
                .unwrap();
        }

        // Update metrics
        let metrics = NodeMetrics {
            node_id,
            name: "test-node".into(),
            health: claw_gateway::NodeHealthStatus::Healthy,
            gpus: vec![crate::types::GpuMetrics {
                index: 0,
                name: "RTX 4090".into(),
                utilization_percent: 85,
                memory_used_mib: 20000,
                memory_total_mib: 24576,
                temperature_celsius: 65,
            }],
        };

        state.update_node_metrics(metrics).await;

        // Verify metrics are cached
        let snapshot = state.get_metrics().await;
        assert_eq!(snapshot.nodes.len(), 1);
        assert_eq!(snapshot.nodes[0].gpus.len(), 1);
        assert_eq!(snapshot.nodes[0].gpus[0].utilization_percent, 85);
    }

    #[tokio::test]
    async fn test_clone_preserves_connections() {
        let state = make_test_state();
        state.add_ws_connection();
        state.add_ws_connection();

        let cloned = state.clone();

        // Connection count should be preserved
        assert_eq!(cloned.ws_connection_count(), 2);
    }

    #[tokio::test]
    async fn test_get_workload_logs_empty() {
        let state = make_test_state();
        let workload_id = WorkloadId::new();

        let logs = state.get_workload_logs(workload_id, None).await;

        assert!(logs.is_empty());
    }
}
