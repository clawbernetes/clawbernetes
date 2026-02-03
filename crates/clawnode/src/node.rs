//! Node orchestration.
//!
//! The main Node struct that coordinates all components:
//! - Gateway client for control plane communication
//! - GPU detector for hardware discovery
//! - Metrics collector for telemetry
//! - Container runtime for workload execution

use std::sync::Arc;
use std::time::Duration;

use claw_proto::{GatewayMessage, GpuCapability, GpuMetricsProto, NodeCapabilities, NodeId, NodeMessage};
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::config::NodeConfig;
use crate::error::NodeError;
use crate::gateway::{GatewayClient, GatewayEvent};
use crate::gpu::{FakeGpuDetector, GpuDetector, NvidiaDetector};
use crate::handlers::{handle_gateway_message, HandlerContext};
use crate::metrics::to_proto_metrics;
use crate::runtime::{ContainerRuntime, FakeContainerRuntime};
use crate::state::NodeState as NodeStateData;

/// Shutdown signal receiver.
pub type ShutdownRx = tokio::sync::broadcast::Receiver<()>;

/// Shutdown signal sender.
pub type ShutdownTx = tokio::sync::broadcast::Sender<()>;

/// Node lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeLifecycleState {
    /// Node is initializing.
    Initializing,
    /// Node is running and connected.
    Running,
    /// Node is reconnecting to gateway.
    Reconnecting,
    /// Node is shutting down.
    ShuttingDown,
    /// Node has stopped.
    Stopped,
}

/// Alias for backward compatibility.
pub type NodeState = NodeLifecycleState;

/// The main node orchestrator.
///
/// Owns all components and coordinates their operation.
pub struct Node {
    /// Node configuration.
    config: NodeConfig,
    /// Unique node identifier.
    node_id: NodeId,
    /// Current node lifecycle state.
    lifecycle_state: NodeLifecycleState,
    /// Gateway client.
    gateway: GatewayClient,
    /// GPU detector.
    gpu_detector: Arc<dyn GpuDetector>,
    /// Container runtime.
    runtime: Arc<dyn ContainerRuntime>,
    /// Node state (workloads, GPUs, etc.).
    node_state: Arc<RwLock<NodeStateData>>,
    /// Shutdown signal sender.
    shutdown_tx: ShutdownTx,
    /// Heartbeat interval in seconds (from gateway registration).
    heartbeat_interval_secs: Arc<RwLock<u32>>,
    /// Metrics interval in seconds (from gateway registration).
    metrics_interval_secs: Arc<RwLock<u32>>,
}

impl Node {
    /// Create a new node with the given configuration.
    ///
    /// This initializes all components but does not start the main loop.
    ///
    /// # Errors
    ///
    /// Returns an error if component initialization fails.
    pub async fn new(config: NodeConfig) -> Result<Self, NodeError> {
        let node_id = NodeId::new();
        info!(node_id = %node_id, name = %config.name, "initializing node");

        // Detect GPU capabilities
        let gpu_detector: Arc<dyn GpuDetector> = if config.gpu.enabled {
            if let Some(detector) = Self::try_nvidia_detector() {
                info!("NVIDIA GPU detection enabled");
                Arc::new(detector)
            } else {
                warn!("NVIDIA GPU detection failed, using stub detector");
                Arc::new(FakeGpuDetector::new())
            }
        } else {
            info!("GPU detection disabled");
            Arc::new(FakeGpuDetector::new())
        };

        // Discover GPUs
        let gpus = gpu_detector.detect_gpus().unwrap_or_default();
        let gpu_count = gpus.len();
        let total_memory_mib: u64 = gpus.iter().map(|g| g.memory_total_mib).sum();

        info!(gpu_count, total_memory_mib, "GPU discovery complete");

        // Build capabilities
        let mut capabilities = NodeCapabilities::new(num_cpus(), system_memory_mib());

        for gpu in &gpus {
            capabilities = capabilities.with_gpu(GpuCapability {
                index: gpu.index,
                name: gpu.name.clone(),
                memory_mib: gpu.memory_total_mib,
                uuid: gpu.uuid.clone(),
            });
        }

        // Create gateway client
        let gateway = GatewayClient::new(&config.gateway_url, node_id, &config.name, capabilities);

        // Create container runtime (fake for now)
        let runtime: Arc<dyn ContainerRuntime> = Arc::new(FakeContainerRuntime::new());

        // Create shutdown channel
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

        // Create node state with detected GPUs
        let node_state = Arc::new(RwLock::new(NodeStateData::with_gpus(gpu_count as u32)));

        Ok(Self {
            config,
            node_id,
            lifecycle_state: NodeLifecycleState::Initializing,
            gateway,
            gpu_detector,
            runtime,
            node_state,
            shutdown_tx,
            heartbeat_interval_secs: Arc::new(RwLock::new(30)), // Default 30s
            metrics_interval_secs: Arc::new(RwLock::new(10)),   // Default 10s
        })
    }

    /// Create a node with custom components (for testing).
    #[must_use]
    pub fn with_components(
        config: NodeConfig,
        node_id: NodeId,
        gpu_detector: Arc<dyn GpuDetector>,
        runtime: Arc<dyn ContainerRuntime>,
    ) -> Self {
        let capabilities = NodeCapabilities::new(1, 1024);

        let gateway = GatewayClient::new(&config.gateway_url, node_id, &config.name, capabilities);

        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

        // Detect GPU count for state
        let gpu_count = gpu_detector.detect_gpus().map(|g| g.len()).unwrap_or(0);
        let node_state = Arc::new(RwLock::new(NodeStateData::with_gpus(gpu_count as u32)));

        Self {
            config,
            node_id,
            lifecycle_state: NodeLifecycleState::Initializing,
            gateway,
            gpu_detector,
            runtime,
            node_state,
            shutdown_tx,
            heartbeat_interval_secs: Arc::new(RwLock::new(30)),
            metrics_interval_secs: Arc::new(RwLock::new(10)),
        }
    }

    /// Try to create an NVIDIA detector.
    fn try_nvidia_detector() -> Option<NvidiaDetector> {
        let detector = NvidiaDetector::new();
        // Test if nvidia-smi works
        match detector.detect_gpus() {
            Ok(gpus) if !gpus.is_empty() => Some(detector),
            _ => None,
        }
    }

    /// Get the node ID.
    #[must_use]
    pub const fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Get the node name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Get the current node lifecycle state.
    #[must_use]
    pub const fn state(&self) -> NodeLifecycleState {
        self.lifecycle_state
    }

    /// Get a reference to the gateway client (for testing).
    #[must_use]
    pub const fn gateway(&self) -> &GatewayClient {
        &self.gateway
    }

    /// Get a shutdown signal receiver.
    #[must_use]
    pub fn shutdown_rx(&self) -> ShutdownRx {
        self.shutdown_tx.subscribe()
    }

    /// Signal the node to shut down.
    pub fn shutdown(&self) {
        info!("shutdown signal received");
        let _ = self.shutdown_tx.send(());
    }

    /// Run the main event loop.
    ///
    /// This connects to the gateway and processes events until shutdown.
    ///
    /// # Errors
    ///
    /// Returns an error if a fatal error occurs.
    pub async fn run(mut self) -> Result<(), NodeError> {
        self.lifecycle_state = NodeLifecycleState::Running;
        info!(node_id = %self.node_id, name = %self.config.name, "node starting");

        // Connect to gateway
        let (gateway_tx, mut gateway_rx) = match self.gateway.connect().await {
            Ok((tx, rx)) => {
                info!("connected to gateway");
                (tx, rx)
            }
            Err(e) => {
                error!(error = %e, "failed to connect to gateway");
                return Err(e);
            }
        };

        // Set up heartbeat interval (default, will be updated from gateway registration)
        let heartbeat_interval = Arc::clone(&self.heartbeat_interval_secs);
        let mut heartbeat_ticker = interval(Duration::from_secs(30));

        // Set up metrics collection interval
        let metrics_interval = Duration::from_secs(self.config.gpu.poll_interval_secs);
        let mut metrics_ticker = interval(metrics_interval);

        // Set up shutdown signal
        let mut shutdown_rx = self.shutdown_rx();

        // Set up SIGTERM/SIGINT handler
        let shutdown_tx = self.shutdown_tx.clone();
        tokio::spawn(async move {
            if matches!(tokio::signal::ctrl_c().await, Ok(())) {
                info!("received SIGINT, initiating shutdown");
                let _ = shutdown_tx.send(());
            }
        });

        // Main event loop
        loop {
            tokio::select! {
                // Gateway events
                event = gateway_rx.recv() => {
                    if let Some(event) = event {
                        if let Err(e) = self.handle_gateway_event(event, &gateway_tx).await {
                            error!(error = %e, "error handling gateway event");
                        }
                    } else {
                        warn!("gateway channel closed");
                        break;
                    }
                }

                // Heartbeat timer
                _ = heartbeat_ticker.tick() => {
                    let interval_secs = *heartbeat_interval.read().await;
                    if interval_secs > 0 {
                        if let Err(e) = self.send_heartbeat(&gateway_tx).await {
                            warn!(error = %e, "failed to send heartbeat");
                        }
                    }
                }

                // Metrics collection timer
                _ = metrics_ticker.tick() => {
                    if let Err(e) = self.collect_and_send_metrics(&gateway_tx).await {
                        warn!(error = %e, "failed to collect/send metrics");
                    }
                }

                // Shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("shutdown signal received, stopping node");
                    self.lifecycle_state = NodeLifecycleState::ShuttingDown;
                    break;
                }
            }
        }

        // Cleanup
        self.gateway.stop();
        self.lifecycle_state = NodeLifecycleState::Stopped;
        info!("node stopped");

        Ok(())
    }

    /// Handle a gateway event.
    async fn handle_gateway_event(
        &mut self,
        event: GatewayEvent,
        gateway_tx: &mpsc::Sender<NodeMessage>,
    ) -> Result<(), NodeError> {
        match event {
            GatewayEvent::Connected => {
                info!("connected to gateway");
            }
            GatewayEvent::Disconnected { reason } => {
                warn!(reason = %reason, "disconnected from gateway");
                self.lifecycle_state = NodeLifecycleState::Reconnecting;
            }
            GatewayEvent::Reconnecting { attempt, delay } => {
                info!(attempt, delay_ms = delay.as_millis(), "reconnecting to gateway");
            }
            GatewayEvent::Message(msg) => {
                self.handle_gateway_message(msg, gateway_tx).await?;
            }
            GatewayEvent::Error(error) => {
                error!(error = %error, "gateway error");
            }
        }
        Ok(())
    }

    /// Handle a message from the gateway.
    async fn handle_gateway_message(
        &mut self,
        msg: GatewayMessage,
        gateway_tx: &mpsc::Sender<NodeMessage>,
    ) -> Result<(), NodeError> {
        // Update intervals from registration response
        if let GatewayMessage::Registered {
            heartbeat_interval_secs,
            metrics_interval_secs,
            ..
        } = &msg
        {
            *self.heartbeat_interval_secs.write().await = *heartbeat_interval_secs;
            *self.metrics_interval_secs.write().await = *metrics_interval_secs;
            info!(
                heartbeat_interval = heartbeat_interval_secs,
                metrics_interval = metrics_interval_secs,
                "updated intervals from gateway"
            );
        }

        // Use the handlers module for full message processing
        let mut state = self.node_state.write().await;
        let mut ctx = HandlerContext {
            state: &mut state,
            runtime: self.runtime.as_ref(),
            node_id: self.node_id,
        };

        match handle_gateway_message(msg, &mut ctx) {
            Ok(Some(response)) => {
                // Send response back to gateway
                gateway_tx
                    .send(response)
                    .await
                    .map_err(|e| NodeError::GatewayConnection(format!("send failed: {e}")))?;
            }
            Ok(None) => {
                // No response needed
            }
            Err(e) => {
                warn!(error = %e, "handler error");
                // Don't propagate non-fatal errors
            }
        }

        Ok(())
    }

    /// Send a heartbeat to the gateway.
    async fn send_heartbeat(
        &self,
        gateway_tx: &mpsc::Sender<NodeMessage>,
    ) -> Result<(), NodeError> {
        debug!("sending heartbeat");
        let msg = NodeMessage::heartbeat(self.node_id);
        gateway_tx
            .send(msg)
            .await
            .map_err(|e| NodeError::GatewayConnection(format!("heartbeat send failed: {e}")))
    }

    /// Collect and send metrics to the gateway.
    async fn collect_and_send_metrics(
        &self,
        gateway_tx: &mpsc::Sender<NodeMessage>,
    ) -> Result<(), NodeError> {
        // Collect GPU metrics
        let gpu_metrics = match self.gpu_detector.collect_metrics() {
            Ok(metrics) => metrics,
            Err(e) => {
                debug!(error = %e, "failed to collect GPU metrics");
                return Ok(()); // Non-fatal, continue running
            }
        };

        if gpu_metrics.is_empty() {
            return Ok(());
        }

        // Convert to proto metrics
        let proto_metrics: Vec<GpuMetricsProto> =
            gpu_metrics.iter().map(to_proto_metrics).collect();

        debug!(gpu_count = proto_metrics.len(), "sending metrics");

        let msg = NodeMessage::metrics(self.node_id, proto_metrics);
        gateway_tx
            .send(msg)
            .await
            .map_err(|e| NodeError::GatewayConnection(format!("send failed: {e}")))?;

        Ok(())
    }
}

/// Get number of CPU cores.
fn num_cpus() -> u32 {
    std::thread::available_parallelism()
        .map(|p| p.get() as u32)
        .unwrap_or(1)
}

/// Get system memory in MiB.
const fn system_memory_mib() -> u64 {
    // Use sysinfo crate in production; for now return a placeholder
    // This could be enhanced to read /proc/meminfo on Linux
    16384 // 16 GiB placeholder
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::ConnectionState;
    use crate::gpu::{GpuInfo, GpuMetrics};

    fn test_config() -> NodeConfig {
        NodeConfig {
            name: "test-node".to_string(),
            gateway_url: "ws://localhost:9999".to_string(),
            gpu: Default::default(),
            network: Default::default(),
            molt: Default::default(),
        }
    }

    fn test_gpu_info() -> GpuInfo {
        GpuInfo {
            index: 0,
            name: "Test GPU".to_string(),
            memory_total_mib: 24576,
            uuid: "GPU-test-0".to_string(),
        }
    }

    fn test_gpu_metrics() -> GpuMetrics {
        GpuMetrics {
            index: 0,
            utilization_percent: 50,
            memory_used_mib: 10000,
            memory_total_mib: 24576,
            temperature_celsius: 65,
            power_watts: Some(250.0),
        }
    }

    #[test]
    fn test_node_state_enum() {
        assert_ne!(NodeState::Initializing, NodeState::Running);
        assert_ne!(NodeState::Running, NodeState::Stopped);
    }

    #[test]
    fn test_node_with_components() {
        let config = test_config();
        let node_id = NodeId::new();
        let detector = Arc::new(FakeGpuDetector::new());
        let runtime = Arc::new(FakeContainerRuntime::new());

        let node = Node::with_components(config, node_id, detector, runtime);

        assert_eq!(node.node_id(), node_id);
        assert_eq!(node.name(), "test-node");
        assert_eq!(node.state(), NodeState::Initializing);
    }

    #[test]
    fn test_node_with_fake_gpus() {
        let config = test_config();
        let node_id = NodeId::new();
        let detector = Arc::new(
            FakeGpuDetector::new().with_gpu(test_gpu_info(), test_gpu_metrics()),
        );
        let runtime = Arc::new(FakeContainerRuntime::new());

        let node = Node::with_components(config, node_id, detector.clone(), runtime);

        let gpus = node.gpu_detector.detect_gpus().expect("should detect");
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].name, "Test GPU");
    }

    #[test]
    fn test_node_shutdown_signal() {
        let config = test_config();
        let node_id = NodeId::new();
        let detector = Arc::new(FakeGpuDetector::new());
        let runtime = Arc::new(FakeContainerRuntime::new());

        let node = Node::with_components(config, node_id, detector, runtime);

        // Get shutdown receiver before signaling
        let mut rx = node.shutdown_rx();

        // Signal shutdown
        node.shutdown();

        // Should receive signal
        let result = rx.try_recv();
        assert!(result.is_ok());
    }

    #[test]
    fn test_num_cpus() {
        let cpus = num_cpus();
        assert!(cpus >= 1);
    }

    #[test]
    fn test_system_memory_mib() {
        let mem = system_memory_mib();
        assert!(mem > 0);
    }

    #[tokio::test]
    async fn test_node_new_with_disabled_gpu() {
        let mut config = test_config();
        config.gpu.enabled = false;

        // This will fail to connect to gateway, but should initialize
        let result = Node::new(config).await;

        // Should succeed in creating the node (just can't connect to gateway)
        assert!(result.is_ok());
        let node = result.unwrap();
        assert_eq!(node.state(), NodeState::Initializing);
    }

    #[tokio::test]
    async fn test_node_graceful_shutdown() {
        let config = test_config();
        let node_id = NodeId::new();
        let detector = Arc::new(FakeGpuDetector::new());
        let runtime = Arc::new(FakeContainerRuntime::new());

        let node = Node::with_components(config, node_id, detector, runtime);

        // Subscribe BEFORE shutdown to receive the signal
        let mut rx = node.shutdown_rx();

        // Signal shutdown
        node.shutdown();

        // The shutdown signal should be received
        let result = rx.try_recv();
        assert!(result.is_ok());
    }

    #[test]
    fn test_gateway_connection_state() {
        let config = test_config();
        let node_id = NodeId::new();
        let detector = Arc::new(FakeGpuDetector::new());
        let runtime = Arc::new(FakeContainerRuntime::new());

        let node = Node::with_components(config, node_id, detector, runtime);

        // Gateway should start disconnected
        assert_eq!(node.gateway().state(), ConnectionState::Disconnected);
    }
}
