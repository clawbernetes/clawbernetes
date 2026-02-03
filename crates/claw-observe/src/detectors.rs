//! Anomaly detectors for various system conditions.
//!
//! This module provides specialized detectors that analyze metrics and logs
//! to identify specific conditions like GPU thermal throttling, memory pressure,
//! error spikes, and performance degradation.

// Allow precision loss for statistical calculations - this is acceptable
// as we're dealing with counts and rates, not precise financial calculations
#![allow(clippy::cast_precision_loss)]

use crate::types::{Insight, Severity};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Threshold configuration for detectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectorConfig {
    /// GPU temperature threshold for thermal throttle detection (Celsius).
    pub gpu_thermal_threshold: f64,
    /// Memory usage percentage threshold for pressure detection.
    pub memory_pressure_threshold: f64,
    /// Error rate threshold (errors per minute) for spike detection.
    pub error_spike_threshold: f64,
    /// Performance degradation threshold (percentage of baseline).
    pub performance_degradation_threshold: f64,
    /// Time window for metrics staleness detection (seconds).
    pub offline_timeout_seconds: u64,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            gpu_thermal_threshold: 85.0,
            memory_pressure_threshold: 90.0,
            error_spike_threshold: 10.0,
            performance_degradation_threshold: 30.0,
            offline_timeout_seconds: 60,
        }
    }
}

/// A single metric data point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPoint {
    /// Metric name.
    pub name: String,
    /// Metric value.
    pub value: f64,
    /// Timestamp when the metric was recorded.
    pub timestamp: DateTime<Utc>,
    /// Optional labels/tags for the metric.
    pub labels: HashMap<String, String>,
}

impl MetricPoint {
    /// Creates a new metric point.
    #[must_use]
    pub fn new(name: impl Into<String>, value: f64, timestamp: DateTime<Utc>) -> Self {
        Self {
            name: name.into(),
            value,
            timestamp,
            labels: HashMap::new(),
        }
    }

    /// Adds a label to the metric point.
    #[must_use]
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Creates a metric point with the current timestamp.
    #[must_use]
    pub fn now(name: impl Into<String>, value: f64) -> Self {
        Self::new(name, value, Utc::now())
    }
}

/// A log entry for analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Log level (error, warn, info, debug, trace).
    pub level: LogLevel,
    /// Log message content.
    pub message: String,
    /// Timestamp of the log entry.
    pub timestamp: DateTime<Utc>,
    /// Source of the log (component, service, etc.).
    pub source: Option<String>,
    /// Optional structured fields.
    pub fields: HashMap<String, String>,
}

impl LogEntry {
    /// Creates a new log entry.
    #[must_use]
    pub fn new(level: LogLevel, message: impl Into<String>, timestamp: DateTime<Utc>) -> Self {
        Self {
            level,
            message: message.into(),
            timestamp,
            source: None,
            fields: HashMap::new(),
        }
    }

    /// Sets the source of the log entry.
    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Adds a field to the log entry.
    #[must_use]
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// Returns true if this is an error-level log.
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self.level, LogLevel::Error)
    }
}

/// Log severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Trace-level log.
    Trace,
    /// Debug-level log.
    Debug,
    /// Informational log.
    Info,
    /// Warning-level log.
    Warn,
    /// Error-level log.
    Error,
}

/// Detects GPU thermal throttling from temperature metrics.
///
/// Returns an insight if GPU temperature exceeds the configured threshold,
/// indicating potential thermal throttling.
#[must_use]
pub fn detect_gpu_thermal_throttle(
    metrics: &[MetricPoint],
    config: &DetectorConfig,
) -> Option<Insight> {
    let gpu_temps: Vec<&MetricPoint> = metrics
        .iter()
        .filter(|m| m.name == "gpu_temperature" || m.name == "gpu.temperature")
        .collect();

    if gpu_temps.is_empty() {
        return None;
    }

    // Find the maximum GPU temperature
    let max_temp = gpu_temps
        .iter()
        .map(|m| m.value)
        .fold(f64::NEG_INFINITY, f64::max);

    if max_temp >= config.gpu_thermal_threshold {
        let gpu_id = gpu_temps
            .iter()
            .find(|m| (m.value - max_temp).abs() < f64::EPSILON)
            .and_then(|m| m.labels.get("gpu_id").cloned())
            .unwrap_or_else(|| "unknown".to_string());

        Some(
            Insight::new(
                Severity::Critical,
                "GPU Thermal Throttling Detected",
                format!(
                    "GPU temperature ({max_temp:.1}°C) has exceeded the thermal threshold ({:.1}°C). \
                     Performance may be degraded due to thermal throttling.",
                    config.gpu_thermal_threshold
                ),
            )
            .with_evidence(format!("GPU {gpu_id} temperature: {max_temp:.1}°C"))
            .with_evidence(format!("Threshold: {:.1}°C", config.gpu_thermal_threshold))
            .with_recommendation(
                "Check GPU cooling, clean dust from heatsinks, ensure adequate airflow, \
                 or reduce workload intensity.",
            )
            .with_tag("gpu")
            .with_tag("thermal"),
        )
    } else {
        None
    }
}

/// Detects memory pressure from usage metrics.
///
/// Returns an insight if memory usage exceeds the configured threshold.
#[must_use]
pub fn detect_memory_pressure(
    metrics: &[MetricPoint],
    config: &DetectorConfig,
) -> Option<Insight> {
    let memory_metrics: Vec<&MetricPoint> = metrics
        .iter()
        .filter(|m| {
            m.name == "memory_usage_percent"
                || m.name == "memory.usage_percent"
                || m.name == "mem_used_percent"
        })
        .collect();

    if memory_metrics.is_empty() {
        return None;
    }

    // Get the most recent memory reading
    let latest = memory_metrics
        .iter()
        .max_by_key(|m| m.timestamp)
        .copied();

    let latest = latest?;

    if latest.value >= config.memory_pressure_threshold {
        let severity = if latest.value >= 95.0 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        Some(
            Insight::new(
                severity,
                "Memory Pressure Detected",
                format!(
                    "Memory utilization ({:.1}%) has exceeded the threshold ({:.1}%). \
                     System may experience slowdowns or OOM conditions.",
                    latest.value, config.memory_pressure_threshold
                ),
            )
            .with_evidence(format!("Current memory usage: {:.1}%", latest.value))
            .with_evidence(format!("Threshold: {:.1}%", config.memory_pressure_threshold))
            .with_recommendation(
                "Consider scaling up memory, terminating unused processes, \
                 or optimizing memory-intensive workloads.",
            )
            .with_tag("memory")
            .with_tag("resource"),
        )
    } else {
        None
    }
}

/// Detects error spikes in log entries.
///
/// Returns an insight if the rate of error logs exceeds the configured threshold.
#[must_use]
pub fn detect_error_spike(logs: &[LogEntry], config: &DetectorConfig) -> Option<Insight> {
    if logs.is_empty() {
        return None;
    }

    let error_logs: Vec<&LogEntry> = logs.iter().filter(|l| l.is_error()).collect();

    if error_logs.is_empty() {
        return None;
    }

    // Calculate the time window from the logs
    let min_time = logs
        .iter()
        .map(|l| l.timestamp)
        .min()?;
    let max_time = logs
        .iter()
        .map(|l| l.timestamp)
        .max()?;

    let duration = max_time - min_time;
    let minutes = duration.num_minutes().max(1) as f64;

    let errors_per_minute = error_logs.len() as f64 / minutes;

    if errors_per_minute >= config.error_spike_threshold {
        let severity = if errors_per_minute >= config.error_spike_threshold * 2.0 {
            Severity::Critical
        } else {
            Severity::Error
        };

        // Collect sample error messages directly into evidence
        let sample_evidence: Vec<String> = error_logs
            .iter()
            .take(3)
            .map(|l| format!("Sample: {}", l.message))
            .collect();

        Some(
            Insight::new(
                severity,
                "Error Spike Detected",
                format!(
                    "Detected {:.1} errors/minute, which exceeds the threshold of {:.1}/minute. \
                     {} total errors in the analysis window.",
                    errors_per_minute,
                    config.error_spike_threshold,
                    error_logs.len()
                ),
            )
            .with_evidence(format!("Error rate: {errors_per_minute:.1}/minute"))
            .with_evidence(format!("Total errors: {}", error_logs.len()))
            .with_evidence_list(sample_evidence)
            .with_recommendation(
                "Review error logs to identify the root cause. Check for failing dependencies, \
                 misconfigurations, or resource exhaustion.",
            )
            .with_tag("errors")
            .with_tag("logs"),
        )
    } else {
        None
    }
}

/// Detects performance degradation by comparing current metrics to baseline.
///
/// Returns an insight if throughput or latency metrics show significant degradation.
#[must_use]
pub fn detect_performance_degradation(
    metrics: &[MetricPoint],
    config: &DetectorConfig,
) -> Option<Insight> {
    // Look for throughput or latency metrics with baseline comparisons
    let throughput_metrics: Vec<&MetricPoint> = metrics
        .iter()
        .filter(|m| {
            m.name.contains("throughput")
                || m.name.contains("requests_per_second")
                || m.name.contains("ops_per_second")
        })
        .collect();

    let latency_metrics: Vec<&MetricPoint> = metrics
        .iter()
        .filter(|m| {
            m.name.contains("latency")
                || m.name.contains("response_time")
                || m.name.contains("duration")
        })
        .collect();

    // Check throughput degradation
    if let Some(degradation) = detect_throughput_drop(&throughput_metrics, config) {
        return Some(degradation);
    }

    // Check latency degradation
    if let Some(degradation) = detect_latency_increase(&latency_metrics, config) {
        return Some(degradation);
    }

    None
}

fn detect_throughput_drop(metrics: &[&MetricPoint], config: &DetectorConfig) -> Option<Insight> {
    if metrics.len() < 2 {
        return None;
    }

    // Sort by timestamp
    let mut sorted: Vec<_> = metrics.to_vec();
    sorted.sort_by_key(|m| m.timestamp);

    // Compare first half average to second half average
    let mid = sorted.len() / 2;
    let first_half_avg: f64 = sorted[..mid].iter().map(|m| m.value).sum::<f64>() / mid as f64;
    let second_half_avg: f64 =
        sorted[mid..].iter().map(|m| m.value).sum::<f64>() / (sorted.len() - mid) as f64;

    if first_half_avg <= 0.0 {
        return None;
    }

    let drop_percent = ((first_half_avg - second_half_avg) / first_half_avg) * 100.0;

    if drop_percent >= config.performance_degradation_threshold {
        Some(
            Insight::new(
                Severity::Warning,
                "Throughput Degradation Detected",
                format!(
                    "Throughput has decreased by {drop_percent:.1}% over the analysis window. \
                     This may indicate resource contention or system issues."
                ),
            )
            .with_evidence(format!("Initial throughput: {first_half_avg:.2}"))
            .with_evidence(format!("Current throughput: {second_half_avg:.2}"))
            .with_evidence(format!("Degradation: {drop_percent:.1}%"))
            .with_recommendation(
                "Check for resource bottlenecks, failing nodes, or increased load. \
                 Review recent deployments or configuration changes.",
            )
            .with_tag("performance")
            .with_tag("throughput"),
        )
    } else {
        None
    }
}

fn detect_latency_increase(metrics: &[&MetricPoint], config: &DetectorConfig) -> Option<Insight> {
    if metrics.len() < 2 {
        return None;
    }

    // Sort by timestamp
    let mut sorted: Vec<_> = metrics.to_vec();
    sorted.sort_by_key(|m| m.timestamp);

    // Compare first half average to second half average
    let mid = sorted.len() / 2;
    let first_half_avg: f64 = sorted[..mid].iter().map(|m| m.value).sum::<f64>() / mid as f64;
    let second_half_avg: f64 =
        sorted[mid..].iter().map(|m| m.value).sum::<f64>() / (sorted.len() - mid) as f64;

    if first_half_avg <= 0.0 {
        return None;
    }

    let increase_percent = ((second_half_avg - first_half_avg) / first_half_avg) * 100.0;

    if increase_percent >= config.performance_degradation_threshold {
        Some(
            Insight::new(
                Severity::Warning,
                "Latency Increase Detected",
                format!(
                    "Response latency has increased by {increase_percent:.1}% over the analysis window. \
                     This may indicate resource contention or system issues."
                ),
            )
            .with_evidence(format!("Initial latency: {first_half_avg:.2}ms"))
            .with_evidence(format!("Current latency: {second_half_avg:.2}ms"))
            .with_evidence(format!("Increase: {increase_percent:.1}%"))
            .with_recommendation(
                "Check for resource bottlenecks, failing nodes, or increased load. \
                 Review recent deployments or configuration changes.",
            )
            .with_tag("performance")
            .with_tag("latency"),
        )
    } else {
        None
    }
}

/// Detects if a node appears to be offline based on metrics staleness.
///
/// Returns an insight if no recent metrics are found for a node.
#[must_use]
pub fn detect_node_offline(
    metrics: &[MetricPoint],
    node_id: &str,
    config: &DetectorConfig,
    current_time: DateTime<Utc>,
) -> Option<Insight> {
    let node_metrics: Vec<&MetricPoint> = metrics
        .iter()
        .filter(|m| m.labels.get("node_id").is_some_and(|n| n == node_id))
        .collect();

    if node_metrics.is_empty() {
        return Some(
            Insight::new(
                Severity::Critical,
                "Node Offline - No Metrics",
                format!(
                    "No metrics found for node '{node_id}'. \
                     The node may be offline or experiencing connectivity issues."
                ),
            )
            .with_evidence(format!("Node ID: {node_id}"))
            .with_evidence("No metrics received in analysis window")
            .with_recommendation(
                "Check node connectivity, verify the node agent is running, \
                 and review network configuration.",
            )
            .with_tag("node")
            .with_tag("offline"),
        );
    }

    // Find the most recent metric
    let latest_metric = node_metrics.iter().max_by_key(|m| m.timestamp)?;

    let age = current_time - latest_metric.timestamp;
    let threshold = Duration::seconds(config.offline_timeout_seconds as i64);

    if age > threshold {
        Some(
            Insight::new(
                Severity::Critical,
                "Node Offline - Stale Metrics",
                format!(
                    "Last metric from node '{node_id}' was {} seconds ago, \
                     exceeding the offline threshold of {} seconds.",
                    age.num_seconds(),
                    config.offline_timeout_seconds
                ),
            )
            .with_evidence(format!("Node ID: {node_id}"))
            .with_evidence(format!("Last metric: {} seconds ago", age.num_seconds()))
            .with_evidence(format!("Threshold: {} seconds", config.offline_timeout_seconds))
            .with_recommendation(
                "Check node connectivity, verify the node agent is running, \
                 and review network configuration.",
            )
            .with_tag("node")
            .with_tag("offline"),
        )
    } else {
        None
    }
}

/// Runs all detectors on the provided metrics and logs.
///
/// Returns a vector of all detected insights.
#[must_use]
pub fn run_all_detectors(
    metrics: &[MetricPoint],
    logs: &[LogEntry],
    config: &DetectorConfig,
) -> Vec<Insight> {
    let mut insights = Vec::new();

    if let Some(insight) = detect_gpu_thermal_throttle(metrics, config) {
        insights.push(insight);
    }

    if let Some(insight) = detect_memory_pressure(metrics, config) {
        insights.push(insight);
    }

    if let Some(insight) = detect_error_spike(logs, config) {
        insights.push(insight);
    }

    if let Some(insight) = detect_performance_degradation(metrics, config) {
        insights.push(insight);
    }

    insights
}

#[cfg(test)]
mod tests {
    use super::*;

    mod detector_config_tests {
        use super::*;

        #[test]
        fn test_default_config() {
            let config = DetectorConfig::default();

            assert!((config.gpu_thermal_threshold - 85.0).abs() < f64::EPSILON);
            assert!((config.memory_pressure_threshold - 90.0).abs() < f64::EPSILON);
            assert!((config.error_spike_threshold - 10.0).abs() < f64::EPSILON);
            assert!((config.performance_degradation_threshold - 30.0).abs() < f64::EPSILON);
            assert_eq!(config.offline_timeout_seconds, 60);
        }
    }

    mod metric_point_tests {
        use super::*;

        #[test]
        fn test_metric_point_creation() {
            let timestamp = Utc::now();
            let metric = MetricPoint::new("cpu_usage", 75.5, timestamp);

            assert_eq!(metric.name, "cpu_usage");
            assert!((metric.value - 75.5).abs() < f64::EPSILON);
            assert_eq!(metric.timestamp, timestamp);
            assert!(metric.labels.is_empty());
        }

        #[test]
        fn test_metric_point_with_labels() {
            let metric = MetricPoint::now("gpu_temperature", 85.0)
                .with_label("gpu_id", "0")
                .with_label("node_id", "node-001");

            assert_eq!(metric.labels.len(), 2);
            assert_eq!(metric.labels.get("gpu_id"), Some(&"0".to_string()));
            assert_eq!(metric.labels.get("node_id"), Some(&"node-001".to_string()));
        }
    }

    mod log_entry_tests {
        use super::*;

        #[test]
        fn test_log_entry_creation() {
            let timestamp = Utc::now();
            let log = LogEntry::new(LogLevel::Error, "Connection failed", timestamp);

            assert_eq!(log.level, LogLevel::Error);
            assert_eq!(log.message, "Connection failed");
            assert!(log.source.is_none());
            assert!(log.fields.is_empty());
        }

        #[test]
        fn test_log_entry_is_error() {
            let timestamp = Utc::now();

            let error_log = LogEntry::new(LogLevel::Error, "Error message", timestamp);
            let warn_log = LogEntry::new(LogLevel::Warn, "Warning message", timestamp);
            let info_log = LogEntry::new(LogLevel::Info, "Info message", timestamp);

            assert!(error_log.is_error());
            assert!(!warn_log.is_error());
            assert!(!info_log.is_error());
        }

        #[test]
        fn test_log_entry_with_source_and_fields() {
            let log = LogEntry::new(LogLevel::Info, "Request processed", Utc::now())
                .with_source("api-server")
                .with_field("request_id", "abc123")
                .with_field("latency_ms", "45");

            assert_eq!(log.source, Some("api-server".to_string()));
            assert_eq!(log.fields.len(), 2);
        }
    }

    mod gpu_thermal_tests {
        use super::*;

        #[test]
        fn test_no_gpu_metrics_returns_none() {
            let metrics = vec![
                MetricPoint::now("cpu_usage", 50.0),
                MetricPoint::now("memory_usage", 60.0),
            ];
            let config = DetectorConfig::default();

            let result = detect_gpu_thermal_throttle(&metrics, &config);
            assert!(result.is_none());
        }

        #[test]
        fn test_gpu_below_threshold_returns_none() {
            let metrics = vec![
                MetricPoint::now("gpu_temperature", 70.0).with_label("gpu_id", "0"),
                MetricPoint::now("gpu_temperature", 75.0).with_label("gpu_id", "1"),
            ];
            let config = DetectorConfig::default(); // threshold is 85.0

            let result = detect_gpu_thermal_throttle(&metrics, &config);
            assert!(result.is_none());
        }

        #[test]
        fn test_gpu_above_threshold_returns_insight() {
            let metrics = vec![
                MetricPoint::now("gpu_temperature", 90.0).with_label("gpu_id", "0"),
                MetricPoint::now("gpu_temperature", 70.0).with_label("gpu_id", "1"),
            ];
            let config = DetectorConfig::default();

            let result = detect_gpu_thermal_throttle(&metrics, &config);
            assert!(result.is_some());

            let insight = result.expect("should have insight");
            assert_eq!(insight.severity, Severity::Critical);
            assert!(insight.title.contains("Thermal"));
            assert!(insight.tags.contains(&"gpu".to_string()));
        }

        #[test]
        fn test_gpu_at_exact_threshold_returns_insight() {
            let metrics = vec![MetricPoint::now("gpu_temperature", 85.0)];
            let config = DetectorConfig::default();

            let result = detect_gpu_thermal_throttle(&metrics, &config);
            assert!(result.is_some());
        }
    }

    mod memory_pressure_tests {
        use super::*;

        #[test]
        fn test_no_memory_metrics_returns_none() {
            let metrics = vec![MetricPoint::now("cpu_usage", 50.0)];
            let config = DetectorConfig::default();

            let result = detect_memory_pressure(&metrics, &config);
            assert!(result.is_none());
        }

        #[test]
        fn test_memory_below_threshold_returns_none() {
            let metrics = vec![MetricPoint::now("memory_usage_percent", 80.0)];
            let config = DetectorConfig::default(); // threshold is 90.0

            let result = detect_memory_pressure(&metrics, &config);
            assert!(result.is_none());
        }

        #[test]
        fn test_memory_above_threshold_returns_warning() {
            let metrics = vec![MetricPoint::now("memory_usage_percent", 92.0)];
            let config = DetectorConfig::default();

            let result = detect_memory_pressure(&metrics, &config);
            assert!(result.is_some());

            let insight = result.expect("should have insight");
            assert_eq!(insight.severity, Severity::Warning);
            assert!(insight.tags.contains(&"memory".to_string()));
        }

        #[test]
        fn test_memory_above_95_returns_critical() {
            let metrics = vec![MetricPoint::now("memory_usage_percent", 97.0)];
            let config = DetectorConfig::default();

            let result = detect_memory_pressure(&metrics, &config);
            assert!(result.is_some());

            let insight = result.expect("should have insight");
            assert_eq!(insight.severity, Severity::Critical);
        }

        #[test]
        fn test_uses_most_recent_memory_metric() {
            let old_timestamp = Utc::now() - Duration::minutes(5);
            let new_timestamp = Utc::now();

            let metrics = vec![
                MetricPoint::new("memory_usage_percent", 95.0, old_timestamp),
                MetricPoint::new("memory_usage_percent", 85.0, new_timestamp), // below threshold
            ];
            let config = DetectorConfig::default();

            let result = detect_memory_pressure(&metrics, &config);
            // Should use the most recent (85.0) which is below threshold
            assert!(result.is_none());
        }
    }

    mod error_spike_tests {
        use super::*;

        #[test]
        fn test_empty_logs_returns_none() {
            let logs: Vec<LogEntry> = vec![];
            let config = DetectorConfig::default();

            let result = detect_error_spike(&logs, &config);
            assert!(result.is_none());
        }

        #[test]
        fn test_no_errors_returns_none() {
            let logs = vec![
                LogEntry::new(LogLevel::Info, "Request processed", Utc::now()),
                LogEntry::new(LogLevel::Warn, "Slow response", Utc::now()),
            ];
            let config = DetectorConfig::default();

            let result = detect_error_spike(&logs, &config);
            assert!(result.is_none());
        }

        #[test]
        fn test_low_error_rate_returns_none() {
            let now = Utc::now();
            let logs: Vec<LogEntry> = (0..5)
                .map(|i| {
                    LogEntry::new(
                        LogLevel::Error,
                        format!("Error {i}"),
                        now - Duration::minutes(10 - i),
                    )
                })
                .collect();
            let config = DetectorConfig::default(); // threshold is 10/min

            // 5 errors over 10 minutes = 0.5 errors/min
            let result = detect_error_spike(&logs, &config);
            assert!(result.is_none());
        }

        #[test]
        fn test_high_error_rate_returns_insight() {
            let now = Utc::now();
            let logs: Vec<LogEntry> = (0..20)
                .map(|i| {
                    LogEntry::new(
                        LogLevel::Error,
                        format!("Error {i}"),
                        now - Duration::seconds(30 - i),
                    )
                })
                .collect();
            let config = DetectorConfig::default();

            // 20 errors in ~30 seconds = ~40 errors/min
            let result = detect_error_spike(&logs, &config);
            assert!(result.is_some());

            let insight = result.expect("should have insight");
            assert!(matches!(insight.severity, Severity::Error | Severity::Critical));
            assert!(insight.tags.contains(&"errors".to_string()));
        }

        #[test]
        fn test_very_high_error_rate_returns_critical() {
            let now = Utc::now();
            let logs: Vec<LogEntry> = (0..50)
                .map(|i| {
                    LogEntry::new(
                        LogLevel::Error,
                        format!("Error {i}"),
                        now - Duration::seconds(30 - (i % 30)),
                    )
                })
                .collect();
            let config = DetectorConfig::default();

            let result = detect_error_spike(&logs, &config);
            assert!(result.is_some());

            let insight = result.expect("should have insight");
            assert_eq!(insight.severity, Severity::Critical);
        }
    }

    mod performance_degradation_tests {
        use super::*;

        #[test]
        fn test_no_performance_metrics_returns_none() {
            let metrics = vec![MetricPoint::now("cpu_usage", 50.0)];
            let config = DetectorConfig::default();

            let result = detect_performance_degradation(&metrics, &config);
            assert!(result.is_none());
        }

        #[test]
        fn test_insufficient_metrics_returns_none() {
            let metrics = vec![MetricPoint::now("throughput", 100.0)];
            let config = DetectorConfig::default();

            let result = detect_performance_degradation(&metrics, &config);
            assert!(result.is_none());
        }

        #[test]
        fn test_stable_throughput_returns_none() {
            let now = Utc::now();
            let metrics: Vec<MetricPoint> = (0..10)
                .map(|i| {
                    MetricPoint::new(
                        "throughput",
                        100.0 + (i % 5) as f64,
                        now - Duration::minutes(10 - i),
                    )
                })
                .collect();
            let config = DetectorConfig::default();

            let result = detect_performance_degradation(&metrics, &config);
            assert!(result.is_none());
        }

        #[test]
        fn test_significant_throughput_drop_returns_insight() {
            let now = Utc::now();
            let mut metrics: Vec<MetricPoint> = (0..5)
                .map(|i| {
                    MetricPoint::new("throughput", 100.0, now - Duration::minutes(10 - i))
                })
                .collect();
            metrics.extend((0..5).map(|i| {
                MetricPoint::new("throughput", 60.0, now - Duration::minutes(5 - i))
            }));
            let config = DetectorConfig::default(); // threshold is 30%

            // 100 -> 60 = 40% drop
            let result = detect_performance_degradation(&metrics, &config);
            assert!(result.is_some());

            let insight = result.expect("should have insight");
            assert!(insight.title.contains("Throughput") || insight.title.contains("Degradation"));
            assert!(insight.tags.contains(&"performance".to_string()));
        }

        #[test]
        fn test_significant_latency_increase_returns_insight() {
            let now = Utc::now();
            let mut metrics: Vec<MetricPoint> = (0..5)
                .map(|i| {
                    MetricPoint::new("latency", 10.0, now - Duration::minutes(10 - i))
                })
                .collect();
            metrics.extend((0..5).map(|i| {
                MetricPoint::new("latency", 15.0, now - Duration::minutes(5 - i))
            }));
            let config = DetectorConfig::default(); // threshold is 30%

            // 10 -> 15 = 50% increase
            let result = detect_performance_degradation(&metrics, &config);
            assert!(result.is_some());

            let insight = result.expect("should have insight");
            assert!(insight.title.contains("Latency"));
        }
    }

    mod node_offline_tests {
        use super::*;

        #[test]
        fn test_no_metrics_for_node_returns_offline() {
            let metrics = vec![
                MetricPoint::now("cpu_usage", 50.0).with_label("node_id", "node-001"),
            ];
            let config = DetectorConfig::default();

            let result = detect_node_offline(&metrics, "node-002", &config, Utc::now());
            assert!(result.is_some());

            let insight = result.expect("should have insight");
            assert_eq!(insight.severity, Severity::Critical);
            assert!(insight.title.contains("Offline"));
        }

        #[test]
        fn test_recent_metrics_returns_none() {
            let metrics = vec![
                MetricPoint::now("cpu_usage", 50.0).with_label("node_id", "node-001"),
            ];
            let config = DetectorConfig::default();

            let result = detect_node_offline(&metrics, "node-001", &config, Utc::now());
            assert!(result.is_none());
        }

        #[test]
        fn test_stale_metrics_returns_offline() {
            let old_timestamp = Utc::now() - Duration::minutes(5);
            let metrics = vec![
                MetricPoint::new("cpu_usage", 50.0, old_timestamp)
                    .with_label("node_id", "node-001"),
            ];
            let config = DetectorConfig::default(); // offline_timeout is 60 seconds

            let result = detect_node_offline(&metrics, "node-001", &config, Utc::now());
            assert!(result.is_some());

            let insight = result.expect("should have insight");
            assert_eq!(insight.severity, Severity::Critical);
            assert!(insight.description.contains("seconds ago"));
        }
    }

    mod run_all_detectors_tests {
        use super::*;

        #[test]
        fn test_empty_inputs_returns_empty() {
            let metrics: Vec<MetricPoint> = vec![];
            let logs: Vec<LogEntry> = vec![];
            let config = DetectorConfig::default();

            let insights = run_all_detectors(&metrics, &logs, &config);
            assert!(insights.is_empty());
        }

        #[test]
        fn test_multiple_issues_detected() {
            let now = Utc::now();

            let metrics = vec![
                MetricPoint::now("gpu_temperature", 95.0),
                MetricPoint::now("memory_usage_percent", 92.0),
            ];

            let logs: Vec<LogEntry> = (0..50)
                .map(|i| {
                    LogEntry::new(LogLevel::Error, format!("Error {i}"), now)
                })
                .collect();

            let config = DetectorConfig::default();

            let insights = run_all_detectors(&metrics, &logs, &config);

            // Should detect GPU thermal, memory pressure, and error spike
            assert!(insights.len() >= 2);
        }
    }
}
