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
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::config::NodeConfig;
use crate::error::NodeError;
use crate::gateway::{ConnectionState, GatewayClient, GatewayEvent};
use crate::gpu::{FakeGpuDetector, GpuDetector, GpuInfo, GpuMetrics, NvidiaDetector};
use crate::metrics::to_proto_metrics;
use crate::runtime::{ContainerRuntime, FakeContainerRuntime};

/// Shutdown signal receiver.
pub type ShutdownRx = tokio::sync::broadcast::Receiver<()>;

/// Shutdown signal sender.
pub type ShutdownTx = tokio::sync::broadcast::Sender<()>;

/// Node state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeState {
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

/// The main node orchestrator.
///
/// Owns all components and coordinates their operation.
pub struct Node {
    /// Node configuration.
    config: NodeConfig,
    /// Unique node identifier.
    node_id: NodeId,
    /// Current node state.
    state: NodeState,
    /// Gateway client.
    gateway: GatewayClient,
    /// GPU detector.
    gpu_detector: Arc<dyn GpuDetector>,
    /// Container runtime.
    #[allow(dead_code)]
    runtime: Arc<dyn ContainerRuntime>,
    /// Shutdown signal sender.
    shutdown_tx: ShutdownTx,
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
            match Self::try_nvidia_detector() {
                Some(detector) => {
                    info!("NVIDIA GPU detection enabled");
                    Arc::new(detector)
                }
                None => {
                    warn!("NVIDIA GPU detection failed, using stub detector");
                    Arc::new(FakeGpuDetector::new())
                }
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

        Ok(Self {
            config,
            node_id,
            state: NodeState::Initializing,
            gateway,
            gpu_detector,
            runtime,
            shutdown_tx,
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

        Self {
            config,
            node_id,
            state: NodeState::Initializing,
            gateway,
            gpu_detector,
            runtime,
            shutdown_tx,
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
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Get the node name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Get the current node state.
    #[must_use]
    pub fn state(&self) -> NodeState {
        self.state
    }

    /// Get a reference to the gateway client (for testing).
    #[must_use]
    pub fn gateway(&self) -> &GatewayClient {
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
        self.state = NodeState::Running;
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

        // Set up metrics collection interval
        let metrics_interval = Duration::from_secs(self.config.gpu.poll_interval_secs);
        let mut metrics_ticker = interval(metrics_interval);

        // Set up shutdown signal
        let mut shutdown_rx = self.shutdown_rx();

        // Set up SIGTERM/SIGINT handler
        let shutdown_tx = self.shutdown_tx.clone();
        tokio::spawn(async move {
            if let Ok(()) = tokio::signal::ctrl_c().await {
                info!("received SIGINT, initiating shutdown");
                let _ = shutdown_tx.send(());
            }
        });

        // Main event loop
        loop {
            tokio::select! {
                // Gateway events
                event = gateway_rx.recv() => {
                    match event {
                        Some(event) => {
                            if let Err(e) = self.handle_gateway_event(event, &gateway_tx).await {
                                error!(error = %e, "error handling gateway event");
                            }
                        }
                        None => {
                            warn!("gateway channel closed");
                            break;
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
                    self.state = NodeState::ShuttingDown;
                    break;
                }
            }
        }

        // Cleanup
        self.gateway.stop();
        self.state = NodeState::Stopped;
        info!("node stopped");

        Ok(())
    }

    /// Handle a gateway event.
    async fn handle_gateway_event(
        &mut self,
        event: GatewayEvent,
        _gateway_tx: &mpsc::Sender<NodeMessage>,
    ) -> Result<(), NodeError> {
        match event {
            GatewayEvent::Connected => {
                info!("connected to gateway");
            }
            GatewayEvent::Disconnected { reason } => {
                warn!(reason = %reason, "disconnected from gateway");
                self.state = NodeState::Reconnecting;
            }
            GatewayEvent::Reconnecting { attempt, delay } => {
                info!(attempt, delay_ms = delay.as_millis(), "reconnecting to gateway");
            }
            GatewayEvent::Message(msg) => {
                self.handle_gateway_message(msg).await?;
            }
            GatewayEvent::Error(error) => {
                error!(error = %error, "gateway error");
            }
        }
        Ok(())
    }

    /// Handle a message from the gateway.
    async fn handle_gateway_message(&mut self, msg: GatewayMessage) -> Result<(), NodeError> {
        match &msg {
            GatewayMessage::Registered { node_id, .. } => {
                info!(node_id = %node_id, "node registered with gateway");
            }
            GatewayMessage::HeartbeatAck { server_time } => {
                debug!(server_time = %server_time, "heartbeat acknowledged");
            }
            GatewayMessage::StartWorkload { workload_id, spec } => {
                info!(workload_id = %workload_id, image = %spec.image, "received workload start request");
                // TODO: Implement workload scheduling
            }
            GatewayMessage::StopWorkload {
                workload_id,
                grace_period_secs,
            } => {
                info!(workload_id = %workload_id, grace_period = grace_period_secs, "received workload stop request");
                // TODO: Implement workload stopping
            }
            GatewayMessage::RequestMetrics => {
                debug!("received metrics request");
            }
            GatewayMessage::RequestCapabilities => {
                debug!("received capabilities request");
            }
            GatewayMessage::Error { code, message } => {
                error!(code, message = %message, "received error from gateway");
            }
        }

        Ok(())
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
fn system_memory_mib() -> u64 {
    // Use sysinfo crate in production; for now return a placeholder
    // This could be enhanced to read /proc/meminfo on Linux
    16384 // 16 GiB placeholder
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::ConnectionState;

    fn test_config() -> NodeConfig {
        NodeConfig {
            name: "test-node".to_string(),
            gateway_url: "ws://localhost:9999".to_string(),
            gpu: Default::default(),
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
