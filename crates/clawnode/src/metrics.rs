//! Metrics collection and streaming.
//!
//! Periodically collects GPU metrics and generates reports for the gateway.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use claw_proto::{GpuMetricsProto, NodeId};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::time::interval;

use crate::error::NodeError;
use crate::gpu::{GpuDetector, GpuMetrics};

/// A metrics report containing GPU telemetry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsReport {
    /// Node ID.
    pub node_id: NodeId,
    /// Timestamp of the report.
    pub timestamp: DateTime<Utc>,
    /// GPU metrics.
    pub gpu_metrics: Vec<GpuMetricsProto>,
}

impl MetricsReport {
    /// Create a new metrics report.
    #[must_use]
    pub fn new(node_id: NodeId, gpu_metrics: Vec<GpuMetricsProto>) -> Self {
        Self {
            node_id,
            timestamp: Utc::now(),
            gpu_metrics,
        }
    }

    /// Check if the report has any metrics.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.gpu_metrics.is_empty()
    }

    /// Get the number of GPUs in the report.
    #[must_use]
    pub fn gpu_count(&self) -> usize {
        self.gpu_metrics.len()
    }

    /// Calculate average GPU utilization.
    #[must_use]
    pub fn average_utilization(&self) -> Option<f64> {
        if self.gpu_metrics.is_empty() {
            return None;
        }
        let sum: u32 = self
            .gpu_metrics
            .iter()
            .map(|m| u32::from(m.utilization_percent))
            .sum();
        Some(f64::from(sum) / self.gpu_metrics.len() as f64)
    }

    /// Check if any GPU is above a temperature threshold.
    #[must_use]
    pub fn any_thermal_warning(&self, threshold: u32) -> bool {
        self.gpu_metrics
            .iter()
            .any(|m| m.temperature_celsius >= threshold)
    }
}

/// Convert internal `GpuMetrics` to protocol `GpuMetricsProto`.
#[must_use]
pub const fn to_proto_metrics(metrics: &GpuMetrics) -> GpuMetricsProto {
    GpuMetricsProto {
        index: metrics.index,
        utilization_percent: metrics.utilization_percent,
        memory_used_mib: metrics.memory_used_mib,
        memory_total_mib: metrics.memory_total_mib,
        temperature_celsius: metrics.temperature_celsius,
        power_watts: metrics.power_watts,
    }
}

/// Configuration for the metrics collector.
#[derive(Debug, Clone)]
pub struct MetricsCollectorConfig {
    /// Interval between metrics collection.
    pub collection_interval: Duration,
    /// Whether to include power metrics.
    pub include_power: bool,
    /// Temperature threshold for warnings.
    pub thermal_warning_threshold: Option<u32>,
}

impl Default for MetricsCollectorConfig {
    fn default() -> Self {
        Self {
            collection_interval: Duration::from_secs(5),
            include_power: true,
            thermal_warning_threshold: Some(85),
        }
    }
}

/// Collects GPU metrics periodically.
pub struct MetricsCollector<D: GpuDetector + ?Sized> {
    /// GPU detector.
    detector: Arc<D>,
    /// Node ID.
    node_id: NodeId,
    /// Configuration.
    config: MetricsCollectorConfig,
}

impl<D: GpuDetector + ?Sized + 'static> MetricsCollector<D> {
    /// Create a new metrics collector.
    #[must_use]
    pub const fn new(detector: Arc<D>, node_id: NodeId, config: MetricsCollectorConfig) -> Self {
        Self {
            detector,
            node_id,
            config,
        }
    }

    /// Collect metrics once.
    ///
    /// # Errors
    ///
    /// Returns an error if metrics collection fails.
    pub fn collect_once(&self) -> Result<MetricsReport, NodeError> {
        let gpu_metrics = self.detector.collect_metrics()?;
        let proto_metrics: Vec<GpuMetricsProto> =
            gpu_metrics.iter().map(to_proto_metrics).collect();

        Ok(MetricsReport::new(self.node_id, proto_metrics))
    }

    /// Start periodic metrics collection.
    ///
    /// Returns a receiver that will receive metrics reports.
    #[must_use] 
    pub fn start_periodic(&self) -> mpsc::Receiver<MetricsReport> {
        let (tx, rx) = mpsc::channel(32);
        let detector = Arc::clone(&self.detector);
        let node_id = self.node_id;
        let interval_duration = self.config.collection_interval;

        tokio::spawn(async move {
            let mut ticker = interval(interval_duration);
            loop {
                ticker.tick().await;
                match detector.collect_metrics() {
                    Ok(gpu_metrics) => {
                        let proto_metrics: Vec<GpuMetricsProto> =
                            gpu_metrics.iter().map(to_proto_metrics).collect();
                        let report = MetricsReport::new(node_id, proto_metrics);
                        if tx.send(report).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        // Log error but continue collecting
                        continue;
                    }
                }
            }
        });

        rx
    }
}

/// Aggregated metrics over a time window.
#[derive(Debug, Clone, Default)]
pub struct AggregatedMetrics {
    /// Number of samples.
    pub sample_count: u32,
    /// Average utilization per GPU.
    pub avg_utilization: Vec<f64>,
    /// Maximum utilization per GPU.
    pub max_utilization: Vec<u8>,
    /// Average temperature per GPU.
    pub avg_temperature: Vec<f64>,
    /// Maximum temperature per GPU.
    pub max_temperature: Vec<u32>,
    /// Average power per GPU (if available).
    pub avg_power: Vec<Option<f64>>,
}

impl AggregatedMetrics {
    /// Create a new aggregated metrics container for N GPUs.
    #[must_use]
    pub fn new(gpu_count: usize) -> Self {
        Self {
            sample_count: 0,
            avg_utilization: vec![0.0; gpu_count],
            max_utilization: vec![0; gpu_count],
            avg_temperature: vec![0.0; gpu_count],
            max_temperature: vec![0; gpu_count],
            avg_power: vec![None; gpu_count],
        }
    }

    /// Add a sample to the aggregation.
    pub fn add_sample(&mut self, metrics: &[GpuMetricsProto]) {
        if self.avg_utilization.len() != metrics.len() {
            // GPU count mismatch, reset
            *self = Self::new(metrics.len());
        }

        self.sample_count += 1;

        for (i, m) in metrics.iter().enumerate() {
            // Update running averages using incremental formula
            let n = f64::from(self.sample_count);

            // Utilization
            self.avg_utilization[i] +=
                (f64::from(m.utilization_percent) - self.avg_utilization[i]) / n;
            if m.utilization_percent > self.max_utilization[i] {
                self.max_utilization[i] = m.utilization_percent;
            }

            // Temperature
            self.avg_temperature[i] +=
                (f64::from(m.temperature_celsius) - self.avg_temperature[i]) / n;
            if m.temperature_celsius > self.max_temperature[i] {
                self.max_temperature[i] = m.temperature_celsius;
            }

            // Power
            if let Some(power) = m.power_watts {
                let current_avg = self.avg_power[i].unwrap_or(0.0);
                self.avg_power[i] = Some(current_avg + (f64::from(power) - current_avg) / n);
            }
        }
    }

    /// Reset the aggregation.
    pub fn reset(&mut self) {
        let gpu_count = self.avg_utilization.len();
        *self = Self::new(gpu_count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::{FakeGpuDetector, GpuInfo};

    fn create_test_metrics() -> GpuMetricsProto {
        GpuMetricsProto {
            index: 0,
            utilization_percent: 75,
            memory_used_mib: 12000,
            memory_total_mib: 24576,
            temperature_celsius: 68,
            power_watts: Some(320.0),
        }
    }

    #[test]
    fn test_metrics_report_creation() {
        let node_id = NodeId::new();
        let metrics = vec![create_test_metrics()];

        let report = MetricsReport::new(node_id, metrics);

        assert_eq!(report.node_id, node_id);
        assert_eq!(report.gpu_count(), 1);
        assert!(!report.is_empty());
    }

    #[test]
    fn test_metrics_report_empty() {
        let report = MetricsReport::new(NodeId::new(), vec![]);

        assert!(report.is_empty());
        assert_eq!(report.gpu_count(), 0);
    }

    #[test]
    fn test_average_utilization() {
        let metrics = vec![
            GpuMetricsProto {
                index: 0,
                utilization_percent: 60,
                memory_used_mib: 10000,
                memory_total_mib: 24576,
                temperature_celsius: 65,
                power_watts: None,
            },
            GpuMetricsProto {
                index: 1,
                utilization_percent: 80,
                memory_used_mib: 20000,
                memory_total_mib: 24576,
                temperature_celsius: 70,
                power_watts: None,
            },
        ];

        let report = MetricsReport::new(NodeId::new(), metrics);
        let avg = report.average_utilization();

        assert!(avg.is_some());
        assert!((avg.unwrap() - 70.0).abs() < 0.01);
    }

    #[test]
    fn test_average_utilization_empty() {
        let report = MetricsReport::new(NodeId::new(), vec![]);
        assert!(report.average_utilization().is_none());
    }

    #[test]
    fn test_thermal_warning() {
        let metrics = vec![
            GpuMetricsProto {
                index: 0,
                utilization_percent: 100,
                memory_used_mib: 24000,
                memory_total_mib: 24576,
                temperature_celsius: 90,
                power_watts: Some(450.0),
            },
        ];

        let report = MetricsReport::new(NodeId::new(), metrics);

        assert!(report.any_thermal_warning(85));
        assert!(report.any_thermal_warning(90));
        assert!(!report.any_thermal_warning(91));
    }

    #[test]
    fn test_to_proto_metrics() {
        let gpu_metrics = GpuMetrics {
            index: 0,
            utilization_percent: 75,
            memory_used_mib: 12000,
            memory_total_mib: 24576,
            temperature_celsius: 68,
            power_watts: Some(320.0),
        };

        let proto = to_proto_metrics(&gpu_metrics);

        assert_eq!(proto.index, 0);
        assert_eq!(proto.utilization_percent, 75);
        assert_eq!(proto.memory_used_mib, 12000);
        assert_eq!(proto.temperature_celsius, 68);
    }

    #[test]
    fn test_metrics_collector_config_default() {
        let config = MetricsCollectorConfig::default();
        assert_eq!(config.collection_interval, Duration::from_secs(5));
        assert!(config.include_power);
        assert_eq!(config.thermal_warning_threshold, Some(85));
    }

    #[test]
    fn test_metrics_collector_collect_once() {
        let detector = Arc::new(
            FakeGpuDetector::new().with_gpu(
                GpuInfo {
                    index: 0,
                    name: "Test GPU".to_string(),
                    memory_total_mib: 24576,
                    uuid: "GPU-test".to_string(),
                },
                GpuMetrics {
                    index: 0,
                    utilization_percent: 50,
                    memory_used_mib: 10000,
                    memory_total_mib: 24576,
                    temperature_celsius: 65,
                    power_watts: Some(250.0),
                },
            ),
        );

        let collector = MetricsCollector::new(
            detector,
            NodeId::new(),
            MetricsCollectorConfig::default(),
        );

        let report = collector.collect_once().expect("should collect");

        assert_eq!(report.gpu_count(), 1);
        assert_eq!(report.gpu_metrics[0].utilization_percent, 50);
    }

    #[test]
    fn test_aggregated_metrics_creation() {
        let agg = AggregatedMetrics::new(2);
        assert_eq!(agg.sample_count, 0);
        assert_eq!(agg.avg_utilization.len(), 2);
        assert_eq!(agg.max_utilization.len(), 2);
    }

    #[test]
    fn test_aggregated_metrics_single_sample() {
        let mut agg = AggregatedMetrics::new(1);
        let metrics = vec![create_test_metrics()];

        agg.add_sample(&metrics);

        assert_eq!(agg.sample_count, 1);
        assert!((agg.avg_utilization[0] - 75.0).abs() < 0.01);
        assert_eq!(agg.max_utilization[0], 75);
        assert!((agg.avg_temperature[0] - 68.0).abs() < 0.01);
        assert_eq!(agg.max_temperature[0], 68);
    }

    #[test]
    fn test_aggregated_metrics_multiple_samples() {
        let mut agg = AggregatedMetrics::new(1);

        // Sample 1: 60% utilization, 60C
        agg.add_sample(&[GpuMetricsProto {
            index: 0,
            utilization_percent: 60,
            memory_used_mib: 10000,
            memory_total_mib: 24576,
            temperature_celsius: 60,
            power_watts: Some(200.0),
        }]);

        // Sample 2: 80% utilization, 70C
        agg.add_sample(&[GpuMetricsProto {
            index: 0,
            utilization_percent: 80,
            memory_used_mib: 15000,
            memory_total_mib: 24576,
            temperature_celsius: 70,
            power_watts: Some(300.0),
        }]);

        assert_eq!(agg.sample_count, 2);
        assert!((agg.avg_utilization[0] - 70.0).abs() < 0.01);
        assert_eq!(agg.max_utilization[0], 80);
        assert!((agg.avg_temperature[0] - 65.0).abs() < 0.01);
        assert_eq!(agg.max_temperature[0], 70);
        assert!((agg.avg_power[0].unwrap() - 250.0).abs() < 0.01);
    }

    #[test]
    fn test_aggregated_metrics_reset() {
        let mut agg = AggregatedMetrics::new(1);
        agg.add_sample(&[create_test_metrics()]);

        assert_eq!(agg.sample_count, 1);

        agg.reset();

        assert_eq!(agg.sample_count, 0);
        assert!((agg.avg_utilization[0] - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_aggregated_metrics_gpu_count_change() {
        let mut agg = AggregatedMetrics::new(1);
        agg.add_sample(&[create_test_metrics()]);

        assert_eq!(agg.sample_count, 1);

        // Add sample with different GPU count - should reset
        agg.add_sample(&[create_test_metrics(), create_test_metrics()]);

        assert_eq!(agg.sample_count, 1); // Reset happened
        assert_eq!(agg.avg_utilization.len(), 2);
    }

    #[test]
    fn test_metrics_report_serialization() {
        let report = MetricsReport::new(NodeId::new(), vec![create_test_metrics()]);

        let json = serde_json::to_string(&report).expect("should serialize");
        let parsed: MetricsReport = serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(report.node_id, parsed.node_id);
        assert_eq!(report.gpu_metrics, parsed.gpu_metrics);
    }

    #[tokio::test]
    async fn test_metrics_collector_periodic() {
        let detector = Arc::new(
            FakeGpuDetector::new().with_gpu(
                GpuInfo {
                    index: 0,
                    name: "Test GPU".to_string(),
                    memory_total_mib: 24576,
                    uuid: "GPU-test".to_string(),
                },
                GpuMetrics {
                    index: 0,
                    utilization_percent: 50,
                    memory_used_mib: 10000,
                    memory_total_mib: 24576,
                    temperature_celsius: 65,
                    power_watts: Some(250.0),
                },
            ),
        );

        let config = MetricsCollectorConfig {
            collection_interval: Duration::from_millis(10),
            ..Default::default()
        };

        let collector = MetricsCollector::new(detector, NodeId::new(), config);
        let mut rx = collector.start_periodic();

        // Should receive at least one report within timeout
        let report = tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("should receive within timeout")
            .expect("should have report");

        assert_eq!(report.gpu_count(), 1);
    }
}
