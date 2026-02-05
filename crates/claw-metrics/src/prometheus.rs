//! Prometheus metrics support for Clawbernetes observability.
//!
//! This module provides Prometheus-compatible metrics for monitoring:
//! - Gateway-level metrics (nodes, workloads, scheduling)
//! - Node-level metrics (GPU utilization, memory, containers)
//!
//! # Example
//!
//! ```rust
//! use claw_metrics::prometheus::{PrometheusRegistry, GatewayMetrics, NodeMetrics};
//!
//! // Create the registry
//! let registry = PrometheusRegistry::new();
//!
//! // Get gateway metrics
//! let gateway = registry.gateway_metrics();
//! gateway.set_nodes_total(5);
//! gateway.inc_workloads_total("running");
//! gateway.observe_scheduling_duration(0.025); // 25ms
//!
//! // Get node metrics
//! let node = registry.node_metrics();
//! node.set_gpu_utilization("node-1", "gpu-0", 85.5);
//! node.set_memory_usage("node-1", 1024 * 1024 * 1024);
//! node.inc_container_restarts("node-1", "workload-123");
//!
//! // Export metrics in Prometheus format
//! let output = registry.encode();
//! assert!(output.contains("clawbernetes_nodes_total"));
//! ```

use std::io::Write;
use std::sync::Arc;

use parking_lot::RwLock;
use prometheus_client::encoding::text::encode;
use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::{exponential_buckets, Histogram};
use prometheus_client::registry::Registry;

/// Label set for workload state metrics.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct WorkloadStateLabels {
    /// The workload state (e.g., "pending", "running", "completed", "failed").
    pub state: String,
}

/// Label set for GPU metrics.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct GpuLabels {
    /// The node identifier.
    pub node_id: String,
    /// The GPU identifier within the node.
    pub gpu_id: String,
}

/// Label set for node metrics.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct NodeLabels {
    /// The node identifier.
    pub node_id: String,
}

/// Label set for container metrics.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct ContainerLabels {
    /// The node identifier.
    pub node_id: String,
    /// The workload identifier.
    pub workload_id: String,
}

/// Gateway-level metrics for Clawbernetes.
///
/// These metrics track the overall state of the gateway and fleet:
/// - Total number of registered nodes
/// - Workloads by state
/// - Scheduling latency
#[derive(Clone)]
pub struct GatewayMetrics {
    /// Total number of registered nodes.
    nodes_total: Gauge,
    /// Total workloads by state.
    workloads_total: Family<WorkloadStateLabels, Gauge>,
    /// Histogram of scheduling durations in seconds.
    scheduling_duration_seconds: Histogram,
}

impl std::fmt::Debug for GatewayMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatewayMetrics")
            .field("nodes_total", &self.nodes_total.get())
            .finish_non_exhaustive()
    }
}

impl GatewayMetrics {
    /// Creates new gateway metrics and registers them with the given registry.
    fn new(registry: &mut Registry) -> Self {
        let nodes_total = Gauge::default();
        registry.register(
            "clawbernetes_nodes_total",
            "Total number of registered nodes in the fleet",
            nodes_total.clone(),
        );

        let workloads_total = Family::<WorkloadStateLabels, Gauge>::default();
        registry.register(
            "clawbernetes_workloads_total",
            "Total number of workloads by state",
            workloads_total.clone(),
        );

        // Histogram buckets: 1ms to 10s with exponential growth
        // start=0.001, factor=2, count=14 gives us: 1ms, 2ms, 4ms, 8ms, 16ms, 32ms, 64ms,
        // 128ms, 256ms, 512ms, 1.024s, 2.048s, 4.096s, 8.192s
        let buckets = exponential_buckets(0.001, 2.0, 14);
        let scheduling_duration_seconds = Histogram::new(buckets);
        registry.register(
            "clawbernetes_scheduling_duration_seconds",
            "Time taken to schedule a workload",
            scheduling_duration_seconds.clone(),
        );

        Self {
            nodes_total,
            workloads_total,
            scheduling_duration_seconds,
        }
    }

    /// Sets the total number of registered nodes.
    #[allow(clippy::cast_possible_wrap)] // Node counts won't exceed i64::MAX
    pub fn set_nodes_total(&self, count: u64) {
        self.nodes_total.set(count as i64);
    }

    /// Gets the current total number of registered nodes.
    #[must_use]
    #[allow(clippy::cast_sign_loss)] // Value is always non-negative
    pub fn get_nodes_total(&self) -> u64 {
        self.nodes_total.get() as u64
    }

    /// Increments the node count.
    pub fn inc_nodes(&self) {
        self.nodes_total.inc();
    }

    /// Decrements the node count.
    pub fn dec_nodes(&self) {
        self.nodes_total.dec();
    }

    /// Sets the workload count for a specific state.
    #[allow(clippy::cast_possible_wrap)] // Workload counts won't exceed i64::MAX
    pub fn set_workloads_total(&self, state: &str, count: u64) {
        let labels = WorkloadStateLabels {
            state: state.to_string(),
        };
        self.workloads_total.get_or_create(&labels).set(count as i64);
    }

    /// Increments the workload count for a specific state.
    pub fn inc_workloads_total(&self, state: &str) {
        let labels = WorkloadStateLabels {
            state: state.to_string(),
        };
        self.workloads_total.get_or_create(&labels).inc();
    }

    /// Decrements the workload count for a specific state.
    pub fn dec_workloads_total(&self, state: &str) {
        let labels = WorkloadStateLabels {
            state: state.to_string(),
        };
        self.workloads_total.get_or_create(&labels).dec();
    }

    /// Gets the workload count for a specific state.
    #[must_use]
    #[allow(clippy::cast_sign_loss)] // Value is always non-negative
    pub fn get_workloads_total(&self, state: &str) -> u64 {
        let labels = WorkloadStateLabels {
            state: state.to_string(),
        };
        self.workloads_total.get_or_create(&labels).get() as u64
    }

    /// Records a scheduling duration observation in seconds.
    pub fn observe_scheduling_duration(&self, duration_seconds: f64) {
        self.scheduling_duration_seconds.observe(duration_seconds);
    }

    /// Records a scheduling duration from a `std::time::Duration`.
    pub fn observe_scheduling_duration_from(&self, duration: std::time::Duration) {
        self.scheduling_duration_seconds.observe(duration.as_secs_f64());
    }
}

/// Node-level metrics for Clawbernetes.
///
/// These metrics track individual node health and resource usage:
/// - GPU utilization per GPU
/// - Memory usage per node
/// - Container count per node
/// - Container restart counts
#[derive(Clone)]
pub struct NodeMetrics {
    /// GPU utilization percentage by node and GPU.
    gpu_utilization_percent: Family<GpuLabels, Gauge>,
    /// Memory usage in bytes by node.
    memory_usage_bytes: Family<NodeLabels, Gauge>,
    /// Container count by node.
    container_count: Family<NodeLabels, Gauge>,
    /// Container restart total by node and workload.
    container_restarts_total: Family<ContainerLabels, Counter>,
}

impl std::fmt::Debug for NodeMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeMetrics").finish()
    }
}

impl NodeMetrics {
    /// Creates new node metrics and registers them with the given registry.
    fn new(registry: &mut Registry) -> Self {
        let gpu_utilization_percent = Family::<GpuLabels, Gauge>::default();
        registry.register(
            "clawbernetes_gpu_utilization_percent",
            "GPU utilization percentage by node and GPU",
            gpu_utilization_percent.clone(),
        );

        let memory_usage_bytes = Family::<NodeLabels, Gauge>::default();
        registry.register(
            "clawbernetes_memory_usage_bytes",
            "Memory usage in bytes by node",
            memory_usage_bytes.clone(),
        );

        let container_count = Family::<NodeLabels, Gauge>::default();
        registry.register(
            "clawbernetes_container_count",
            "Number of containers running on each node",
            container_count.clone(),
        );

        let container_restarts_total = Family::<ContainerLabels, Counter>::default();
        registry.register(
            "clawbernetes_container_restarts_total",
            "Total number of container restarts by node and workload",
            container_restarts_total.clone(),
        );

        Self {
            gpu_utilization_percent,
            memory_usage_bytes,
            container_count,
            container_restarts_total,
        }
    }

    /// Sets the GPU utilization for a specific node and GPU.
    ///
    /// # Arguments
    ///
    /// * `node_id` - The node identifier
    /// * `gpu_id` - The GPU identifier within the node
    /// * `utilization` - GPU utilization percentage (0.0 - 100.0)
    #[allow(clippy::cast_possible_truncation)] // Utilization is 0-100, fits in i64
    pub fn set_gpu_utilization(&self, node_id: &str, gpu_id: &str, utilization: f64) {
        let labels = GpuLabels {
            node_id: node_id.to_string(),
            gpu_id: gpu_id.to_string(),
        };
        // Store as integer percentage * 100 for precision (e.g., 85.5% -> 8550)
        self.gpu_utilization_percent
            .get_or_create(&labels)
            .set((utilization * 100.0) as i64);
    }

    /// Gets the GPU utilization for a specific node and GPU.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // i64 to f64 is safe for percentage values
    pub fn get_gpu_utilization(&self, node_id: &str, gpu_id: &str) -> f64 {
        let labels = GpuLabels {
            node_id: node_id.to_string(),
            gpu_id: gpu_id.to_string(),
        };
        self.gpu_utilization_percent.get_or_create(&labels).get() as f64 / 100.0
    }

    /// Sets the memory usage for a specific node.
    ///
    /// # Arguments
    ///
    /// * `node_id` - The node identifier
    /// * `bytes` - Memory usage in bytes
    #[allow(clippy::cast_possible_wrap)] // Memory in bytes won't exceed i64::MAX
    pub fn set_memory_usage(&self, node_id: &str, bytes: u64) {
        let labels = NodeLabels {
            node_id: node_id.to_string(),
        };
        self.memory_usage_bytes.get_or_create(&labels).set(bytes as i64);
    }

    /// Gets the memory usage for a specific node.
    #[must_use]
    #[allow(clippy::cast_sign_loss)] // Value is always non-negative
    pub fn get_memory_usage(&self, node_id: &str) -> u64 {
        let labels = NodeLabels {
            node_id: node_id.to_string(),
        };
        self.memory_usage_bytes.get_or_create(&labels).get() as u64
    }

    /// Sets the container count for a specific node.
    #[allow(clippy::cast_possible_wrap)] // Container counts won't exceed i64::MAX
    pub fn set_container_count(&self, node_id: &str, count: u64) {
        let labels = NodeLabels {
            node_id: node_id.to_string(),
        };
        self.container_count.get_or_create(&labels).set(count as i64);
    }

    /// Gets the container count for a specific node.
    #[must_use]
    #[allow(clippy::cast_sign_loss)] // Value is always non-negative
    pub fn get_container_count(&self, node_id: &str) -> u64 {
        let labels = NodeLabels {
            node_id: node_id.to_string(),
        };
        self.container_count.get_or_create(&labels).get() as u64
    }

    /// Increments the container count for a specific node.
    pub fn inc_container_count(&self, node_id: &str) {
        let labels = NodeLabels {
            node_id: node_id.to_string(),
        };
        self.container_count.get_or_create(&labels).inc();
    }

    /// Decrements the container count for a specific node.
    pub fn dec_container_count(&self, node_id: &str) {
        let labels = NodeLabels {
            node_id: node_id.to_string(),
        };
        self.container_count.get_or_create(&labels).dec();
    }

    /// Increments the container restart counter for a specific node and workload.
    pub fn inc_container_restarts(&self, node_id: &str, workload_id: &str) {
        let labels = ContainerLabels {
            node_id: node_id.to_string(),
            workload_id: workload_id.to_string(),
        };
        self.container_restarts_total.get_or_create(&labels).inc();
    }

    /// Gets the container restart count for a specific node and workload.
    #[must_use]
    pub fn get_container_restarts(&self, node_id: &str, workload_id: &str) -> u64 {
        let labels = ContainerLabels {
            node_id: node_id.to_string(),
            workload_id: workload_id.to_string(),
        };
        self.container_restarts_total.get_or_create(&labels).get()
    }
}

/// Central Prometheus metrics registry for Clawbernetes.
///
/// This registry holds all metrics and provides methods for:
/// - Accessing gateway and node metrics
/// - Encoding metrics in Prometheus text format
/// - Creating HTTP response bodies for `/metrics` endpoints
#[derive(Clone)]
pub struct PrometheusRegistry {
    /// The underlying prometheus-client registry.
    registry: Arc<RwLock<Registry>>,
    /// Gateway-level metrics.
    gateway_metrics: GatewayMetrics,
    /// Node-level metrics.
    node_metrics: NodeMetrics,
}

impl std::fmt::Debug for PrometheusRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrometheusRegistry")
            .field("gateway_metrics", &self.gateway_metrics)
            .field("node_metrics", &self.node_metrics)
            .finish_non_exhaustive()
    }
}

impl Default for PrometheusRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PrometheusRegistry {
    /// Creates a new Prometheus metrics registry with all Clawbernetes metrics registered.
    #[must_use]
    pub fn new() -> Self {
        let mut registry = Registry::default();

        let gateway_metrics = GatewayMetrics::new(&mut registry);
        let node_metrics = NodeMetrics::new(&mut registry);

        Self {
            registry: Arc::new(RwLock::new(registry)),
            gateway_metrics,
            node_metrics,
        }
    }

    /// Returns a reference to the gateway metrics.
    #[must_use]
    pub fn gateway_metrics(&self) -> &GatewayMetrics {
        &self.gateway_metrics
    }

    /// Returns a reference to the node metrics.
    #[must_use]
    pub fn node_metrics(&self) -> &NodeMetrics {
        &self.node_metrics
    }

    /// Encodes all metrics in Prometheus text format.
    ///
    /// This output can be served directly from a `/metrics` HTTP endpoint.
    #[must_use]
    pub fn encode(&self) -> String {
        let registry = self.registry.read();
        let mut buffer = String::new();
        if encode(&mut buffer, &registry).is_err() {
            tracing::error!("failed to encode prometheus metrics");
            return String::new();
        }
        buffer
    }

    /// Encodes all metrics into a byte vector.
    ///
    /// Useful for HTTP response bodies.
    #[must_use]
    pub fn encode_to_vec(&self) -> Vec<u8> {
        self.encode().into_bytes()
    }

    /// Returns the Content-Type header value for Prometheus metrics.
    #[must_use]
    pub const fn content_type() -> &'static str {
        "text/plain; version=0.0.4; charset=utf-8"
    }
}

/// HTTP handler for serving metrics.
///
/// This struct provides methods for integrating with various HTTP frameworks.
#[derive(Clone, Debug)]
pub struct MetricsHandler {
    registry: PrometheusRegistry,
}

impl MetricsHandler {
    /// Creates a new metrics handler with the given registry.
    #[must_use]
    pub const fn new(registry: PrometheusRegistry) -> Self {
        Self { registry }
    }

    /// Returns the metrics in Prometheus text format.
    #[must_use]
    pub fn handle(&self) -> MetricsResponse {
        MetricsResponse {
            body: self.registry.encode(),
            content_type: PrometheusRegistry::content_type().to_string(),
        }
    }

    /// Returns a reference to the underlying registry.
    #[must_use]
    pub const fn registry(&self) -> &PrometheusRegistry {
        &self.registry
    }
}

/// Response from the metrics handler.
#[derive(Debug, Clone)]
pub struct MetricsResponse {
    /// The response body in Prometheus text format.
    pub body: String,
    /// The Content-Type header value.
    pub content_type: String,
}

impl MetricsResponse {
    /// Returns the body as bytes.
    #[must_use]
    pub fn body_bytes(&self) -> Vec<u8> {
        self.body.clone().into_bytes()
    }

    /// Writes the response to a writer.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails.
    pub fn write_to<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(self.body.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    mod prometheus_registry_tests {
        use super::*;

        #[test]
        fn create_registry() {
            let registry = PrometheusRegistry::new();
            assert!(registry.gateway_metrics().get_nodes_total() == 0);
        }

        #[test]
        fn registry_is_cloneable() {
            let registry1 = PrometheusRegistry::new();
            registry1.gateway_metrics().set_nodes_total(5);

            let registry2 = registry1.clone();
            assert_eq!(registry2.gateway_metrics().get_nodes_total(), 5);
        }

        #[test]
        fn cloned_registry_shares_state() {
            let registry1 = PrometheusRegistry::new();
            let registry2 = registry1.clone();

            registry1.gateway_metrics().set_nodes_total(10);
            assert_eq!(registry2.gateway_metrics().get_nodes_total(), 10);

            registry2.gateway_metrics().inc_nodes();
            assert_eq!(registry1.gateway_metrics().get_nodes_total(), 11);
        }

        #[test]
        fn encode_metrics_includes_all_registered_metrics() {
            let registry = PrometheusRegistry::new();

            // Set some values
            registry.gateway_metrics().set_nodes_total(5);
            registry.gateway_metrics().set_workloads_total("running", 10);
            registry.node_metrics().set_gpu_utilization("node-1", "gpu-0", 85.5);

            let output = registry.encode();

            assert!(output.contains("clawbernetes_nodes_total"));
            assert!(output.contains("clawbernetes_workloads_total"));
            assert!(output.contains("clawbernetes_scheduling_duration_seconds"));
            assert!(output.contains("clawbernetes_gpu_utilization_percent"));
        }

        #[test]
        fn encode_to_vec_returns_bytes() {
            let registry = PrometheusRegistry::new();
            registry.gateway_metrics().set_nodes_total(3);

            let bytes = registry.encode_to_vec();
            assert!(!bytes.is_empty());

            let as_string = String::from_utf8(bytes);
            assert!(as_string.is_ok());
            assert!(as_string.unwrap().contains("clawbernetes_nodes_total"));
        }

        #[test]
        fn content_type_is_correct() {
            let ct = PrometheusRegistry::content_type();
            assert!(ct.contains("text/plain"));
            assert!(ct.contains("0.0.4"));
        }

        #[test]
        fn default_creates_new_registry() {
            let registry = PrometheusRegistry::default();
            assert_eq!(registry.gateway_metrics().get_nodes_total(), 0);
        }
    }

    mod gateway_metrics_tests {
        use super::*;

        #[test]
        fn nodes_total_operations() {
            let registry = PrometheusRegistry::new();
            let metrics = registry.gateway_metrics();

            assert_eq!(metrics.get_nodes_total(), 0);

            metrics.set_nodes_total(5);
            assert_eq!(metrics.get_nodes_total(), 5);

            metrics.inc_nodes();
            assert_eq!(metrics.get_nodes_total(), 6);

            metrics.dec_nodes();
            assert_eq!(metrics.get_nodes_total(), 5);
        }

        #[test]
        fn workloads_total_by_state() {
            let registry = PrometheusRegistry::new();
            let metrics = registry.gateway_metrics();

            metrics.set_workloads_total("pending", 5);
            metrics.set_workloads_total("running", 10);
            metrics.set_workloads_total("completed", 100);
            metrics.set_workloads_total("failed", 2);

            assert_eq!(metrics.get_workloads_total("pending"), 5);
            assert_eq!(metrics.get_workloads_total("running"), 10);
            assert_eq!(metrics.get_workloads_total("completed"), 100);
            assert_eq!(metrics.get_workloads_total("failed"), 2);
        }

        #[test]
        fn workloads_total_inc_dec() {
            let registry = PrometheusRegistry::new();
            let metrics = registry.gateway_metrics();

            metrics.inc_workloads_total("running");
            assert_eq!(metrics.get_workloads_total("running"), 1);

            metrics.inc_workloads_total("running");
            assert_eq!(metrics.get_workloads_total("running"), 2);

            metrics.dec_workloads_total("running");
            assert_eq!(metrics.get_workloads_total("running"), 1);
        }

        #[test]
        fn scheduling_duration_observation() {
            let registry = PrometheusRegistry::new();
            let metrics = registry.gateway_metrics();

            // Observe some durations
            metrics.observe_scheduling_duration(0.001); // 1ms
            metrics.observe_scheduling_duration(0.010); // 10ms
            metrics.observe_scheduling_duration(0.100); // 100ms

            // Verify it appears in output
            let output = registry.encode();
            assert!(output.contains("clawbernetes_scheduling_duration_seconds"));
        }

        #[test]
        fn scheduling_duration_from_duration() {
            let registry = PrometheusRegistry::new();
            let metrics = registry.gateway_metrics();

            let duration = Duration::from_millis(50);
            metrics.observe_scheduling_duration_from(duration);

            let output = registry.encode();
            assert!(output.contains("clawbernetes_scheduling_duration_seconds"));
        }

        #[test]
        fn workloads_total_in_output() {
            let registry = PrometheusRegistry::new();
            let metrics = registry.gateway_metrics();

            metrics.set_workloads_total("running", 10);
            metrics.set_workloads_total("pending", 5);

            let output = registry.encode();

            assert!(output.contains("clawbernetes_workloads_total"));
            assert!(output.contains("running"));
            assert!(output.contains("pending"));
        }
    }

    mod node_metrics_tests {
        use super::*;

        #[test]
        fn gpu_utilization_operations() {
            let registry = PrometheusRegistry::new();
            let metrics = registry.node_metrics();

            metrics.set_gpu_utilization("node-1", "gpu-0", 85.5);
            metrics.set_gpu_utilization("node-1", "gpu-1", 90.0);
            metrics.set_gpu_utilization("node-2", "gpu-0", 50.0);

            assert!((metrics.get_gpu_utilization("node-1", "gpu-0") - 85.5).abs() < 0.01);
            assert!((metrics.get_gpu_utilization("node-1", "gpu-1") - 90.0).abs() < 0.01);
            assert!((metrics.get_gpu_utilization("node-2", "gpu-0") - 50.0).abs() < 0.01);
        }

        #[test]
        fn memory_usage_operations() {
            let registry = PrometheusRegistry::new();
            let metrics = registry.node_metrics();

            let mem_1gb = 1024 * 1024 * 1024;
            let mem_2gb = 2 * 1024 * 1024 * 1024;

            metrics.set_memory_usage("node-1", mem_1gb);
            metrics.set_memory_usage("node-2", mem_2gb);

            assert_eq!(metrics.get_memory_usage("node-1"), mem_1gb);
            assert_eq!(metrics.get_memory_usage("node-2"), mem_2gb);
        }

        #[test]
        fn container_count_operations() {
            let registry = PrometheusRegistry::new();
            let metrics = registry.node_metrics();

            metrics.set_container_count("node-1", 5);
            assert_eq!(metrics.get_container_count("node-1"), 5);

            metrics.inc_container_count("node-1");
            assert_eq!(metrics.get_container_count("node-1"), 6);

            metrics.dec_container_count("node-1");
            assert_eq!(metrics.get_container_count("node-1"), 5);
        }

        #[test]
        fn container_restarts_operations() {
            let registry = PrometheusRegistry::new();
            let metrics = registry.node_metrics();

            assert_eq!(metrics.get_container_restarts("node-1", "workload-1"), 0);

            metrics.inc_container_restarts("node-1", "workload-1");
            assert_eq!(metrics.get_container_restarts("node-1", "workload-1"), 1);

            metrics.inc_container_restarts("node-1", "workload-1");
            metrics.inc_container_restarts("node-1", "workload-1");
            assert_eq!(metrics.get_container_restarts("node-1", "workload-1"), 3);

            // Different workload should have its own counter
            assert_eq!(metrics.get_container_restarts("node-1", "workload-2"), 0);
            metrics.inc_container_restarts("node-1", "workload-2");
            assert_eq!(metrics.get_container_restarts("node-1", "workload-2"), 1);
            assert_eq!(metrics.get_container_restarts("node-1", "workload-1"), 3);
        }

        #[test]
        fn node_metrics_in_output() {
            let registry = PrometheusRegistry::new();
            let metrics = registry.node_metrics();

            metrics.set_gpu_utilization("node-1", "gpu-0", 85.5);
            metrics.set_memory_usage("node-1", 1024 * 1024 * 1024);
            metrics.set_container_count("node-1", 3);
            metrics.inc_container_restarts("node-1", "workload-1");

            let output = registry.encode();

            assert!(output.contains("clawbernetes_gpu_utilization_percent"));
            assert!(output.contains("clawbernetes_memory_usage_bytes"));
            assert!(output.contains("clawbernetes_container_count"));
            assert!(output.contains("clawbernetes_container_restarts_total"));
            assert!(output.contains("node_id=\"node-1\""));
        }
    }

    mod metrics_handler_tests {
        use super::*;

        #[test]
        fn create_handler() {
            let registry = PrometheusRegistry::new();
            let handler = MetricsHandler::new(registry);
            assert!(handler.registry().gateway_metrics().get_nodes_total() == 0);
        }

        #[test]
        fn handle_returns_response() {
            let registry = PrometheusRegistry::new();
            registry.gateway_metrics().set_nodes_total(5);

            let handler = MetricsHandler::new(registry);
            let response = handler.handle();

            assert!(response.body.contains("clawbernetes_nodes_total"));
            assert!(response.content_type.contains("text/plain"));
        }

        #[test]
        fn response_body_bytes() {
            let registry = PrometheusRegistry::new();
            registry.gateway_metrics().set_nodes_total(5);

            let handler = MetricsHandler::new(registry);
            let response = handler.handle();
            let bytes = response.body_bytes();

            assert!(!bytes.is_empty());
            let as_string = String::from_utf8(bytes);
            assert!(as_string.is_ok());
        }

        #[test]
        fn response_write_to() {
            let registry = PrometheusRegistry::new();
            registry.gateway_metrics().set_nodes_total(5);

            let handler = MetricsHandler::new(registry);
            let response = handler.handle();

            let mut buffer = Vec::new();
            let result = response.write_to(&mut buffer);
            assert!(result.is_ok());
            assert!(!buffer.is_empty());
        }
    }

    mod label_tests {
        use super::*;

        #[test]
        fn workload_state_labels_equality() {
            let labels1 = WorkloadStateLabels {
                state: "running".to_string(),
            };
            let labels2 = WorkloadStateLabels {
                state: "running".to_string(),
            };
            let labels3 = WorkloadStateLabels {
                state: "pending".to_string(),
            };

            assert_eq!(labels1, labels2);
            assert_ne!(labels1, labels3);
        }

        #[test]
        fn gpu_labels_equality() {
            let labels1 = GpuLabels {
                node_id: "node-1".to_string(),
                gpu_id: "gpu-0".to_string(),
            };
            let labels2 = GpuLabels {
                node_id: "node-1".to_string(),
                gpu_id: "gpu-0".to_string(),
            };
            let labels3 = GpuLabels {
                node_id: "node-1".to_string(),
                gpu_id: "gpu-1".to_string(),
            };

            assert_eq!(labels1, labels2);
            assert_ne!(labels1, labels3);
        }

        #[test]
        fn node_labels_equality() {
            let labels1 = NodeLabels {
                node_id: "node-1".to_string(),
            };
            let labels2 = NodeLabels {
                node_id: "node-1".to_string(),
            };
            let labels3 = NodeLabels {
                node_id: "node-2".to_string(),
            };

            assert_eq!(labels1, labels2);
            assert_ne!(labels1, labels3);
        }

        #[test]
        fn container_labels_equality() {
            let labels1 = ContainerLabels {
                node_id: "node-1".to_string(),
                workload_id: "workload-1".to_string(),
            };
            let labels2 = ContainerLabels {
                node_id: "node-1".to_string(),
                workload_id: "workload-1".to_string(),
            };
            let labels3 = ContainerLabels {
                node_id: "node-1".to_string(),
                workload_id: "workload-2".to_string(),
            };

            assert_eq!(labels1, labels2);
            assert_ne!(labels1, labels3);
        }
    }

    mod thread_safety_tests {
        use super::*;
        use std::thread;

        #[test]
        fn concurrent_metric_updates() {
            let registry = PrometheusRegistry::new();

            let mut handles = vec![];

            // Spawn multiple threads updating metrics
            for i in 0..10 {
                let registry_clone = registry.clone();
                let handle = thread::spawn(move || {
                    for j in 0..100 {
                        registry_clone.gateway_metrics().inc_nodes();
                        registry_clone
                            .gateway_metrics()
                            .inc_workloads_total("running");
                        registry_clone
                            .node_metrics()
                            .set_gpu_utilization(&format!("node-{i}"), "gpu-0", j as f64);
                    }
                });
                handles.push(handle);
            }

            for handle in handles {
                handle.join().unwrap();
            }

            // Should have 10 threads * 100 increments = 1000
            assert_eq!(registry.gateway_metrics().get_nodes_total(), 1000);
            assert_eq!(registry.gateway_metrics().get_workloads_total("running"), 1000);
        }

        #[test]
        fn concurrent_encode() {
            let registry = PrometheusRegistry::new();
            registry.gateway_metrics().set_nodes_total(5);

            let mut handles = vec![];

            for _ in 0..10 {
                let registry_clone = registry.clone();
                let handle = thread::spawn(move || {
                    for _ in 0..100 {
                        let output = registry_clone.encode();
                        assert!(output.contains("clawbernetes_nodes_total"));
                    }
                });
                handles.push(handle);
            }

            for handle in handles {
                handle.join().unwrap();
            }
        }
    }

    mod integration_tests {
        use super::*;

        #[test]
        fn full_workflow() {
            // Simulate a realistic workflow
            let registry = PrometheusRegistry::new();

            // Gateway starts up
            let gateway = registry.gateway_metrics();

            // Nodes register
            for _ in 0..5 {
                gateway.inc_nodes();
            }
            assert_eq!(gateway.get_nodes_total(), 5);

            // Submit some workloads
            for _ in 0..10 {
                gateway.inc_workloads_total("pending");
            }

            // Schedule workloads (with timing)
            for i in 0..10 {
                let start = std::time::Instant::now();
                // Simulate scheduling work
                std::thread::sleep(Duration::from_micros(100));
                let duration = start.elapsed();

                gateway.observe_scheduling_duration_from(duration);
                gateway.dec_workloads_total("pending");
                gateway.inc_workloads_total("running");

                // Update node metrics
                let node = registry.node_metrics();
                let node_id = format!("node-{}", i % 5);
                node.set_gpu_utilization(&node_id, "gpu-0", 50.0 + (i as f64 * 5.0));
                node.inc_container_count(&node_id);
            }

            // Verify final state
            assert_eq!(gateway.get_workloads_total("pending"), 0);
            assert_eq!(gateway.get_workloads_total("running"), 10);

            // Some workloads complete, some fail
            for _ in 0..7 {
                gateway.dec_workloads_total("running");
                gateway.inc_workloads_total("completed");
            }
            for _ in 0..2 {
                gateway.dec_workloads_total("running");
                gateway.inc_workloads_total("failed");
            }

            assert_eq!(gateway.get_workloads_total("running"), 1);
            assert_eq!(gateway.get_workloads_total("completed"), 7);
            assert_eq!(gateway.get_workloads_total("failed"), 2);

            // Export metrics
            let output = registry.encode();

            // Verify all metrics are present
            assert!(output.contains("clawbernetes_nodes_total 5"));
            assert!(output.contains("clawbernetes_workloads_total"));
            assert!(output.contains("clawbernetes_scheduling_duration_seconds"));
            assert!(output.contains("clawbernetes_gpu_utilization_percent"));
            assert!(output.contains("clawbernetes_container_count"));
        }

        #[test]
        fn metrics_output_format() {
            let registry = PrometheusRegistry::new();
            registry.gateway_metrics().set_nodes_total(5);
            registry.gateway_metrics().set_workloads_total("running", 3);
            registry.node_metrics().set_gpu_utilization("node-1", "gpu-0", 85.5);

            let output = registry.encode();

            // Check format: # HELP, # TYPE, metric_name value
            assert!(output.contains("# HELP"));
            assert!(output.contains("# TYPE"));
            assert!(output.contains("clawbernetes_nodes_total 5"));
        }
    }
}
