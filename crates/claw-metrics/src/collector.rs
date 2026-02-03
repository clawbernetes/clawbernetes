//! Metric collectors for GPU and system metrics.
//!
//! This module provides trait definitions and implementations for collecting
//! metrics from various sources like GPUs and system resources.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use tracing::{debug, warn};

use crate::error::Result;
use crate::storage::MetricStore;
use crate::types::{MetricName, MetricPoint};

/// Trait for metric collectors.
///
/// Implement this trait to create custom metric collectors that can
/// gather metrics from various sources.
pub trait MetricCollector: Send + Sync {
    /// Collects metrics and returns them as a vector of (name, point) tuples.
    ///
    /// # Errors
    ///
    /// Returns an error if collection fails.
    fn collect(&self) -> Result<Vec<(MetricName, MetricPoint)>>;

    /// Returns the name of this collector for logging purposes.
    fn name(&self) -> &'static str;

    /// Collects metrics and pushes them to the given store.
    ///
    /// # Errors
    ///
    /// Returns an error if collection or storage fails.
    fn collect_and_push(&self, store: &MetricStore) -> Result<()> {
        let metrics = self.collect()?;
        store.push_batch(metrics)
    }
}

/// Collector for GPU metrics.
///
/// Collects GPU-related metrics such as:
/// - Utilization percentage
/// - Memory usage
/// - Temperature
/// - Power consumption
#[derive(Debug)]
pub struct GpuMetricCollector {
    /// The node ID this collector is associated with.
    node_id: String,
    /// Simulated GPU data for testing (in production, this would query actual GPU APIs).
    gpu_data: Arc<RwLock<HashMap<String, GpuInfo>>>,
}

/// Information about a single GPU.
#[derive(Debug, Clone)]
pub struct GpuInfo {
    /// GPU identifier (e.g., "0", "1").
    pub id: String,
    /// GPU model name.
    pub model: String,
    /// Current utilization percentage (0-100).
    pub utilization: f64,
    /// Total memory in bytes.
    pub memory_total: u64,
    /// Used memory in bytes.
    pub memory_used: u64,
    /// Temperature in Celsius.
    pub temperature: f64,
    /// Power consumption in watts.
    pub power_watts: f64,
}

impl GpuMetricCollector {
    /// Creates a new GPU metric collector for the given node.
    #[must_use]
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            gpu_data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Updates the GPU information for a specific GPU.
    ///
    /// In production, this would be called by the GPU monitoring system.
    /// For testing, this allows injecting test data.
    pub fn update_gpu(&self, info: GpuInfo) {
        let mut data = self.gpu_data.write();
        data.insert(info.id.clone(), info);
    }

    /// Removes a GPU from tracking.
    pub fn remove_gpu(&self, gpu_id: &str) {
        let mut data = self.gpu_data.write();
        data.remove(gpu_id);
    }

    /// Returns the number of tracked GPUs.
    #[must_use]
    pub fn gpu_count(&self) -> usize {
        let data = self.gpu_data.read();
        data.len()
    }

    /// Returns the node ID.
    #[must_use]
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Creates metric names for GPU metrics.
    fn metric_names() -> Result<GpuMetricNames> {
        Ok(GpuMetricNames {
            utilization: MetricName::new("gpu_utilization_percent")?,
            memory_used: MetricName::new("gpu_memory_used_bytes")?,
            memory_total: MetricName::new("gpu_memory_total_bytes")?,
            memory_percent: MetricName::new("gpu_memory_percent")?,
            temperature: MetricName::new("gpu_temperature_celsius")?,
            power: MetricName::new("gpu_power_watts")?,
        })
    }
}

struct GpuMetricNames {
    utilization: MetricName,
    memory_used: MetricName,
    memory_total: MetricName,
    memory_percent: MetricName,
    temperature: MetricName,
    power: MetricName,
}

impl MetricCollector for GpuMetricCollector {
    #[allow(clippy::cast_precision_loss)] // Memory values in bytes won't exceed f64 precision
    fn collect(&self) -> Result<Vec<(MetricName, MetricPoint)>> {
        let names = Self::metric_names()?;
        let data = self.gpu_data.read();

        if data.is_empty() {
            debug!(node = %self.node_id, "no GPUs to collect metrics from");
            return Ok(vec![]);
        }

        let timestamp = MetricPoint::now_timestamp();
        let mut metrics = Vec::new();

        for gpu in data.values() {
            let labels = |name: MetricName| {
                let point = MetricPoint::new(timestamp, 0.0)
                    .label("node_id", &self.node_id)
                    .label("gpu_id", &gpu.id)
                    .label("gpu_model", &gpu.model);
                (name, point)
            };

            // Utilization
            let (name, mut point) = labels(names.utilization.clone());
            point.value = gpu.utilization;
            metrics.push((name, point));

            // Memory used
            let (name, mut point) = labels(names.memory_used.clone());
            point.value = gpu.memory_used as f64;
            metrics.push((name, point));

            // Memory total
            let (name, mut point) = labels(names.memory_total.clone());
            point.value = gpu.memory_total as f64;
            metrics.push((name, point));

            // Memory percent
            let (name, mut point) = labels(names.memory_percent.clone());
            if gpu.memory_total > 0 {
                point.value = (gpu.memory_used as f64 / gpu.memory_total as f64) * 100.0;
            }
            metrics.push((name, point));

            // Temperature
            let (name, mut point) = labels(names.temperature.clone());
            point.value = gpu.temperature;
            metrics.push((name, point));

            // Power
            let (name, mut point) = labels(names.power.clone());
            point.value = gpu.power_watts;
            metrics.push((name, point));
        }

        debug!(
            node = %self.node_id,
            gpu_count = data.len(),
            metric_count = metrics.len(),
            "collected GPU metrics"
        );

        Ok(metrics)
    }

    fn name(&self) -> &'static str {
        "GpuMetricCollector"
    }
}

/// Collector for system-level metrics.
///
/// Collects system metrics such as:
/// - CPU usage
/// - Memory usage
/// - Disk usage
#[derive(Debug)]
pub struct SystemMetricCollector {
    /// The node ID this collector is associated with.
    node_id: String,
    /// Simulated system data for testing.
    system_data: Arc<RwLock<SystemInfo>>,
}

/// System information.
#[derive(Debug, Clone, Default)]
pub struct SystemInfo {
    /// CPU utilization percentage (0-100).
    pub cpu_percent: f64,
    /// Total memory in bytes.
    pub memory_total: u64,
    /// Used memory in bytes.
    pub memory_used: u64,
    /// Total disk space in bytes.
    pub disk_total: u64,
    /// Used disk space in bytes.
    pub disk_used: u64,
    /// System uptime in seconds.
    pub uptime_seconds: u64,
    /// Load average (1 minute).
    pub load_1m: f64,
    /// Load average (5 minutes).
    pub load_5m: f64,
    /// Load average (15 minutes).
    pub load_15m: f64,
}

impl SystemMetricCollector {
    /// Creates a new system metric collector for the given node.
    #[must_use]
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            system_data: Arc::new(RwLock::new(SystemInfo::default())),
        }
    }

    /// Updates the system information.
    ///
    /// In production, this would be called by the system monitoring component.
    pub fn update_system(&self, info: SystemInfo) {
        let mut data = self.system_data.write();
        *data = info;
    }

    /// Returns the node ID.
    #[must_use]
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Creates metric names for system metrics.
    fn metric_names() -> Result<SystemMetricNames> {
        Ok(SystemMetricNames {
            cpu_percent: MetricName::new("system_cpu_percent")?,
            memory_used: MetricName::new("system_memory_used_bytes")?,
            memory_total: MetricName::new("system_memory_total_bytes")?,
            memory_percent: MetricName::new("system_memory_percent")?,
            disk_used: MetricName::new("system_disk_used_bytes")?,
            disk_total: MetricName::new("system_disk_total_bytes")?,
            disk_percent: MetricName::new("system_disk_percent")?,
            uptime: MetricName::new("system_uptime_seconds")?,
            load_1m: MetricName::new("system_load_1m")?,
            load_5m: MetricName::new("system_load_5m")?,
            load_15m: MetricName::new("system_load_15m")?,
        })
    }
}

struct SystemMetricNames {
    cpu_percent: MetricName,
    memory_used: MetricName,
    memory_total: MetricName,
    memory_percent: MetricName,
    disk_used: MetricName,
    disk_total: MetricName,
    disk_percent: MetricName,
    uptime: MetricName,
    load_1m: MetricName,
    load_5m: MetricName,
    load_15m: MetricName,
}

impl MetricCollector for SystemMetricCollector {
    #[allow(clippy::cast_precision_loss)] // System metrics won't exceed f64 precision
    fn collect(&self) -> Result<Vec<(MetricName, MetricPoint)>> {
        let names = Self::metric_names()?;
        let timestamp = MetricPoint::now_timestamp();

        // Clone data to release lock early
        let data = self.system_data.read().clone();

        let mut metrics = Vec::new();

        let base_point = || MetricPoint::new(timestamp, 0.0).label("node_id", &self.node_id);

        // CPU
        let mut point = base_point();
        point.value = data.cpu_percent;
        metrics.push((names.cpu_percent, point));

        // Memory used
        let mut point = base_point();
        point.value = data.memory_used as f64;
        metrics.push((names.memory_used, point));

        // Memory total
        let mut point = base_point();
        point.value = data.memory_total as f64;
        metrics.push((names.memory_total, point));

        // Memory percent
        let mut point = base_point();
        if data.memory_total > 0 {
            point.value = (data.memory_used as f64 / data.memory_total as f64) * 100.0;
        }
        metrics.push((names.memory_percent, point));

        // Disk used
        let mut point = base_point();
        point.value = data.disk_used as f64;
        metrics.push((names.disk_used, point));

        // Disk total
        let mut point = base_point();
        point.value = data.disk_total as f64;
        metrics.push((names.disk_total, point));

        // Disk percent
        let mut point = base_point();
        if data.disk_total > 0 {
            point.value = (data.disk_used as f64 / data.disk_total as f64) * 100.0;
        }
        metrics.push((names.disk_percent, point));

        // Uptime
        let mut point = base_point();
        point.value = data.uptime_seconds as f64;
        metrics.push((names.uptime, point));

        // Load averages
        let mut point = base_point();
        point.value = data.load_1m;
        metrics.push((names.load_1m, point));

        let mut point = base_point();
        point.value = data.load_5m;
        metrics.push((names.load_5m, point));

        let mut point = base_point();
        point.value = data.load_15m;
        metrics.push((names.load_15m, point));

        debug!(
            node = %self.node_id,
            metric_count = metrics.len(),
            "collected system metrics"
        );

        Ok(metrics)
    }

    fn name(&self) -> &'static str {
        "SystemMetricCollector"
    }
}

/// A composite collector that aggregates multiple collectors.
#[derive(Default)]
pub struct CompositeCollector {
    collectors: Vec<Box<dyn MetricCollector>>,
}

impl std::fmt::Debug for CompositeCollector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeCollector")
            .field("collector_count", &self.collectors.len())
            .finish()
    }
}

impl CompositeCollector {
    /// Creates a new empty composite collector.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a collector to this composite.
    pub fn add(&mut self, collector: impl MetricCollector + 'static) {
        self.collectors.push(Box::new(collector));
    }

    /// Returns the number of collectors.
    #[must_use]
    pub fn collector_count(&self) -> usize {
        self.collectors.len()
    }
}

impl MetricCollector for CompositeCollector {
    fn collect(&self) -> Result<Vec<(MetricName, MetricPoint)>> {
        let mut all_metrics = Vec::new();

        for collector in &self.collectors {
            match collector.collect() {
                Ok(metrics) => all_metrics.extend(metrics),
                Err(e) => {
                    warn!(
                        collector = collector.name(),
                        error = %e,
                        "collector failed, skipping"
                    );
                }
            }
        }

        Ok(all_metrics)
    }

    fn name(&self) -> &'static str {
        "CompositeCollector"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    mod gpu_collector_tests {
        use super::*;

        fn test_gpu_info(id: &str) -> GpuInfo {
            GpuInfo {
                id: id.to_string(),
                model: "RTX 4090".to_string(),
                utilization: 85.5,
                memory_total: 24_000_000_000,
                memory_used: 18_000_000_000,
                temperature: 72.0,
                power_watts: 350.0,
            }
        }

        #[test]
        fn create_gpu_collector() {
            let collector = GpuMetricCollector::new("node-1");
            assert_eq!(collector.node_id(), "node-1");
            assert_eq!(collector.gpu_count(), 0);
        }

        #[test]
        fn add_and_remove_gpu() {
            let collector = GpuMetricCollector::new("node-1");

            collector.update_gpu(test_gpu_info("0"));
            assert_eq!(collector.gpu_count(), 1);

            collector.update_gpu(test_gpu_info("1"));
            assert_eq!(collector.gpu_count(), 2);

            collector.remove_gpu("0");
            assert_eq!(collector.gpu_count(), 1);
        }

        #[test]
        fn collect_no_gpus_returns_empty() {
            let collector = GpuMetricCollector::new("node-1");
            let metrics = collector.collect().unwrap();
            assert!(metrics.is_empty());
        }

        #[test]
        fn collect_single_gpu_metrics() {
            let collector = GpuMetricCollector::new("node-1");
            collector.update_gpu(test_gpu_info("0"));

            let metrics = collector.collect().unwrap();

            // Should have 6 metrics per GPU
            assert_eq!(metrics.len(), 6);

            // Check metric names
            let names: Vec<&str> = metrics.iter().map(|(n, _)| n.as_str()).collect();
            assert!(names.contains(&"gpu_utilization_percent"));
            assert!(names.contains(&"gpu_memory_used_bytes"));
            assert!(names.contains(&"gpu_memory_total_bytes"));
            assert!(names.contains(&"gpu_memory_percent"));
            assert!(names.contains(&"gpu_temperature_celsius"));
            assert!(names.contains(&"gpu_power_watts"));
        }

        #[test]
        fn collect_multiple_gpus() {
            let collector = GpuMetricCollector::new("node-1");
            collector.update_gpu(test_gpu_info("0"));
            collector.update_gpu(test_gpu_info("1"));

            let metrics = collector.collect().unwrap();

            // Should have 6 metrics per GPU * 2 GPUs = 12
            assert_eq!(metrics.len(), 12);
        }

        #[test]
        fn metrics_have_correct_labels() {
            let collector = GpuMetricCollector::new("node-1");
            collector.update_gpu(test_gpu_info("0"));

            let metrics = collector.collect().unwrap();
            let (_, point) = &metrics[0];

            assert_eq!(point.labels.get("node_id"), Some(&"node-1".to_string()));
            assert_eq!(point.labels.get("gpu_id"), Some(&"0".to_string()));
            assert_eq!(point.labels.get("gpu_model"), Some(&"RTX 4090".to_string()));
        }

        #[test]
        fn metrics_have_correct_values() {
            let collector = GpuMetricCollector::new("node-1");
            collector.update_gpu(test_gpu_info("0"));

            let metrics = collector.collect().unwrap();

            // Find utilization metric
            let (_, point) = metrics
                .iter()
                .find(|(n, _)| n.as_str() == "gpu_utilization_percent")
                .unwrap();
            assert!((point.value - 85.5).abs() < f64::EPSILON);

            // Find temperature metric
            let (_, point) = metrics
                .iter()
                .find(|(n, _)| n.as_str() == "gpu_temperature_celsius")
                .unwrap();
            assert!((point.value - 72.0).abs() < f64::EPSILON);
        }

        #[test]
        fn collect_and_push_to_store() {
            let store = MetricStore::new(Duration::from_secs(3600));
            let collector = GpuMetricCollector::new("node-1");
            collector.update_gpu(test_gpu_info("0"));

            collector.collect_and_push(&store).unwrap();

            let metrics = store.metrics_list();
            assert_eq!(metrics.len(), 6);
        }

        #[test]
        fn collector_name() {
            let collector = GpuMetricCollector::new("node-1");
            assert_eq!(collector.name(), "GpuMetricCollector");
        }
    }

    mod system_collector_tests {
        use super::*;

        fn test_system_info() -> SystemInfo {
            SystemInfo {
                cpu_percent: 45.0,
                memory_total: 64_000_000_000,
                memory_used: 32_000_000_000,
                disk_total: 1_000_000_000_000,
                disk_used: 500_000_000_000,
                uptime_seconds: 86400,
                load_1m: 2.5,
                load_5m: 2.0,
                load_15m: 1.5,
            }
        }

        #[test]
        fn create_system_collector() {
            let collector = SystemMetricCollector::new("node-1");
            assert_eq!(collector.node_id(), "node-1");
        }

        #[test]
        fn collect_system_metrics() {
            let collector = SystemMetricCollector::new("node-1");
            collector.update_system(test_system_info());

            let metrics = collector.collect().unwrap();

            // Should have 11 system metrics
            assert_eq!(metrics.len(), 11);

            let names: Vec<&str> = metrics.iter().map(|(n, _)| n.as_str()).collect();
            assert!(names.contains(&"system_cpu_percent"));
            assert!(names.contains(&"system_memory_used_bytes"));
            assert!(names.contains(&"system_memory_total_bytes"));
            assert!(names.contains(&"system_memory_percent"));
            assert!(names.contains(&"system_disk_used_bytes"));
            assert!(names.contains(&"system_disk_total_bytes"));
            assert!(names.contains(&"system_disk_percent"));
            assert!(names.contains(&"system_uptime_seconds"));
            assert!(names.contains(&"system_load_1m"));
            assert!(names.contains(&"system_load_5m"));
            assert!(names.contains(&"system_load_15m"));
        }

        #[test]
        fn metrics_have_correct_values() {
            let collector = SystemMetricCollector::new("node-1");
            collector.update_system(test_system_info());

            let metrics = collector.collect().unwrap();

            // Find CPU metric
            let (_, point) = metrics
                .iter()
                .find(|(n, _)| n.as_str() == "system_cpu_percent")
                .unwrap();
            assert!((point.value - 45.0).abs() < f64::EPSILON);

            // Find memory percent (should be 50%)
            let (_, point) = metrics
                .iter()
                .find(|(n, _)| n.as_str() == "system_memory_percent")
                .unwrap();
            assert!((point.value - 50.0).abs() < f64::EPSILON);
        }

        #[test]
        fn metrics_have_node_label() {
            let collector = SystemMetricCollector::new("node-1");
            collector.update_system(test_system_info());

            let metrics = collector.collect().unwrap();
            let (_, point) = &metrics[0];

            assert_eq!(point.labels.get("node_id"), Some(&"node-1".to_string()));
        }

        #[test]
        fn collect_and_push_to_store() {
            let store = MetricStore::new(Duration::from_secs(3600));
            let collector = SystemMetricCollector::new("node-1");
            collector.update_system(test_system_info());

            collector.collect_and_push(&store).unwrap();

            let metrics = store.metrics_list();
            assert_eq!(metrics.len(), 11);
        }

        #[test]
        fn collector_name() {
            let collector = SystemMetricCollector::new("node-1");
            assert_eq!(collector.name(), "SystemMetricCollector");
        }
    }

    mod composite_collector_tests {
        use super::*;

        #[test]
        fn create_empty_composite() {
            let collector = CompositeCollector::new();
            assert_eq!(collector.collector_count(), 0);

            let metrics = collector.collect().unwrap();
            assert!(metrics.is_empty());
        }

        #[test]
        fn add_collectors() {
            let mut composite = CompositeCollector::new();

            composite.add(GpuMetricCollector::new("node-1"));
            assert_eq!(composite.collector_count(), 1);

            composite.add(SystemMetricCollector::new("node-1"));
            assert_eq!(composite.collector_count(), 2);
        }

        #[test]
        fn collect_from_all_collectors() {
            let mut composite = CompositeCollector::new();

            let gpu_collector = GpuMetricCollector::new("node-1");
            gpu_collector.update_gpu(GpuInfo {
                id: "0".to_string(),
                model: "RTX 4090".to_string(),
                utilization: 80.0,
                memory_total: 24_000_000_000,
                memory_used: 12_000_000_000,
                temperature: 70.0,
                power_watts: 300.0,
            });
            composite.add(gpu_collector);

            let system_collector = SystemMetricCollector::new("node-1");
            system_collector.update_system(SystemInfo {
                cpu_percent: 50.0,
                memory_total: 64_000_000_000,
                memory_used: 32_000_000_000,
                disk_total: 1_000_000_000_000,
                disk_used: 500_000_000_000,
                uptime_seconds: 3600,
                load_1m: 1.0,
                load_5m: 0.8,
                load_15m: 0.6,
            });
            composite.add(system_collector);

            let metrics = composite.collect().unwrap();

            // Should have GPU metrics (6) + system metrics (11) = 17
            assert_eq!(metrics.len(), 17);
        }

        #[test]
        fn composite_collector_name() {
            let collector = CompositeCollector::new();
            assert_eq!(collector.name(), "CompositeCollector");
        }
    }

    mod trait_tests {
        use super::*;

        #[test]
        fn collector_is_send_sync() {
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<GpuMetricCollector>();
            assert_send_sync::<SystemMetricCollector>();
        }

        #[test]
        fn box_dyn_collector() {
            let gpu: Box<dyn MetricCollector> = Box::new(GpuMetricCollector::new("node-1"));
            let system: Box<dyn MetricCollector> = Box::new(SystemMetricCollector::new("node-1"));

            let collectors: Vec<Box<dyn MetricCollector>> = vec![gpu, system];

            for collector in &collectors {
                let _ = collector.collect();
            }
        }
    }
}
