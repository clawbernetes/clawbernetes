//! Cross-signal correlation for root cause analysis.
//!
//! This module provides functionality to correlate metrics with logs,
//! identify patterns across multiple signals, and find root causes.

use crate::detectors::{LogEntry, LogLevel, MetricPoint};
use crate::types::{Insight, Severity, TimeRange};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// A correlation between metrics and logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correlation {
    /// Unique identifier for this correlation.
    pub id: String,
    /// Description of the correlation.
    pub description: String,
    /// Metric points involved in this correlation.
    pub metric_names: Vec<String>,
    /// Log sources involved in this correlation.
    pub log_sources: Vec<String>,
    /// Strength of the correlation (0.0 to 1.0).
    pub strength: f64,
    /// Time window over which the correlation was observed.
    pub time_window: TimeRange,
    /// Correlation type.
    pub correlation_type: CorrelationType,
}

impl Correlation {
    /// Creates a new correlation.
    #[must_use]
    pub fn new(
        description: impl Into<String>,
        time_window: TimeRange,
        correlation_type: CorrelationType,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            metric_names: Vec::new(),
            log_sources: Vec::new(),
            strength: 0.0,
            time_window,
            correlation_type,
        }
    }

    /// Adds a metric name to the correlation.
    #[must_use]
    pub fn with_metric(mut self, metric_name: impl Into<String>) -> Self {
        self.metric_names.push(metric_name.into());
        self
    }

    /// Adds a log source to the correlation.
    #[must_use]
    pub fn with_log_source(mut self, source: impl Into<String>) -> Self {
        self.log_sources.push(source.into());
        self
    }

    /// Sets the correlation strength.
    #[must_use]
    pub const fn with_strength(mut self, strength: f64) -> Self {
        self.strength = strength;
        self
    }

    /// Returns true if this is a strong correlation (>= 0.7).
    #[must_use]
    pub fn is_strong(&self) -> bool {
        self.strength >= 0.7
    }
}

/// Type of correlation observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrelationType {
    /// Metric spike coincides with error logs.
    MetricSpikeWithErrors,
    /// Metric drop coincides with error logs.
    MetricDropWithErrors,
    /// Multiple metrics moving together.
    MetricComovement,
    /// Error logs from multiple sources.
    ErrorPropagation,
    /// Resource exhaustion pattern.
    ResourceExhaustion,
    /// Cascade failure pattern.
    CascadeFailure,
}

impl std::fmt::Display for CorrelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MetricSpikeWithErrors => write!(f, "Metric Spike with Errors"),
            Self::MetricDropWithErrors => write!(f, "Metric Drop with Errors"),
            Self::MetricComovement => write!(f, "Metric Co-movement"),
            Self::ErrorPropagation => write!(f, "Error Propagation"),
            Self::ResourceExhaustion => write!(f, "Resource Exhaustion"),
            Self::CascadeFailure => write!(f, "Cascade Failure"),
        }
    }
}

/// An event in the diagnostic timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    /// When this event occurred.
    pub timestamp: DateTime<Utc>,
    /// Type of event.
    pub event_type: EventType,
    /// Description of the event.
    pub description: String,
    /// Severity of the event.
    pub severity: EventSeverity,
    /// Source of the event (metric name or log source).
    pub source: String,
    /// Associated value if applicable.
    pub value: Option<f64>,
}

impl TimelineEvent {
    /// Creates a new timeline event.
    #[must_use]
    pub fn new(
        timestamp: DateTime<Utc>,
        event_type: EventType,
        description: impl Into<String>,
        severity: EventSeverity,
        source: impl Into<String>,
    ) -> Self {
        Self {
            timestamp,
            event_type,
            description: description.into(),
            severity,
            source: source.into(),
            value: None,
        }
    }

    /// Sets the value for this event.
    #[must_use]
    pub const fn with_value(mut self, value: f64) -> Self {
        self.value = Some(value);
        self
    }
}

/// Type of timeline event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// Metric threshold exceeded.
    MetricThresholdExceeded,
    /// Metric returned to normal.
    MetricNormalized,
    /// Error log recorded.
    ErrorLogged,
    /// Warning log recorded.
    WarningLogged,
    /// State change detected.
    StateChange,
    /// Anomaly detected.
    AnomalyDetected,
}

/// Severity level for timeline events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum EventSeverity {
    /// Low severity.
    Low,
    /// Medium severity.
    Medium,
    /// High severity.
    High,
    /// Critical severity.
    Critical,
}

/// Configuration for correlation analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelatorConfig {
    /// Time window tolerance for correlating events (seconds).
    pub time_tolerance_seconds: i64,
    /// Minimum correlation strength to report.
    pub min_correlation_strength: f64,
    /// Thresholds for metric spike detection.
    pub spike_threshold_percent: f64,
}

impl Default for CorrelatorConfig {
    fn default() -> Self {
        Self {
            time_tolerance_seconds: 60,
            min_correlation_strength: 0.5,
            spike_threshold_percent: 50.0,
        }
    }
}

/// Correlates metrics with logs to find patterns.
///
/// Identifies cases where metric anomalies coincide with error logs
/// within the specified time window.
#[must_use]
pub fn correlate_metrics_logs(
    metrics: &[MetricPoint],
    logs: &[LogEntry],
    time_window: &TimeRange,
    config: &CorrelatorConfig,
) -> Vec<Correlation> {
    let mut correlations = Vec::new();

    // Filter to relevant time window
    let relevant_metrics: Vec<_> = metrics
        .iter()
        .filter(|m| time_window.contains(&m.timestamp))
        .collect();

    let error_logs: Vec<_> = logs
        .iter()
        .filter(|l| time_window.contains(&l.timestamp) && l.is_error())
        .collect();

    if relevant_metrics.is_empty() || error_logs.is_empty() {
        return correlations;
    }

    // Detect metric spikes that coincide with errors
    let metric_spikes = detect_metric_spikes(&relevant_metrics, config);

    for spike in metric_spikes {
        let tolerance = Duration::seconds(config.time_tolerance_seconds);
        let spike_start = spike.timestamp - tolerance;
        let spike_end = spike.timestamp + tolerance;

        let concurrent_errors: Vec<_> = error_logs
            .iter()
            .filter(|l| l.timestamp >= spike_start && l.timestamp <= spike_end)
            .collect();

        if !concurrent_errors.is_empty() {
            let strength = calculate_correlation_strength(
                concurrent_errors.len(),
                error_logs.len(),
                config,
            );

            if strength >= config.min_correlation_strength {
                let mut correlation = Correlation::new(
                    format!(
                        "Spike in {} ({:.1}% increase) coincides with {} error(s)",
                        spike.name,
                        spike.value,
                        concurrent_errors.len()
                    ),
                    time_window.clone(),
                    CorrelationType::MetricSpikeWithErrors,
                )
                .with_metric(&spike.name)
                .with_strength(strength);

                // Add unique log sources
                for log in concurrent_errors {
                    if let Some(ref source) = log.source {
                        if !correlation.log_sources.contains(source) {
                            correlation = correlation.with_log_source(source);
                        }
                    }
                }

                correlations.push(correlation);
            }
        }
    }

    // Detect resource exhaustion patterns
    if let Some(exhaustion) = detect_resource_exhaustion(&relevant_metrics, &error_logs, time_window, config) {
        correlations.push(exhaustion);
    }

    correlations
}

#[derive(Debug)]
struct MetricSpike {
    name: String,
    timestamp: DateTime<Utc>,
    value: f64, // percentage increase
}

fn detect_metric_spikes(metrics: &[&MetricPoint], config: &CorrelatorConfig) -> Vec<MetricSpike> {
    let mut spikes = Vec::new();

    // Group metrics by name
    let mut grouped: std::collections::HashMap<&str, Vec<&&MetricPoint>> =
        std::collections::HashMap::new();
    for metric in metrics {
        grouped.entry(metric.name.as_str()).or_default().push(metric);
    }

    for (name, mut points) in grouped {
        if points.len() < 2 {
            continue;
        }

        points.sort_by_key(|m| m.timestamp);

        // Calculate running average and detect spikes
        for window in points.windows(2) {
            let prev = window[0];
            let curr = window[1];

            if prev.value > 0.0 {
                let change_percent = ((curr.value - prev.value) / prev.value) * 100.0;

                if change_percent >= config.spike_threshold_percent {
                    spikes.push(MetricSpike {
                        name: name.to_string(),
                        timestamp: curr.timestamp,
                        value: change_percent,
                    });
                }
            }
        }
    }

    spikes
}

fn calculate_correlation_strength(
    concurrent_count: usize,
    total_count: usize,
    _config: &CorrelatorConfig,
) -> f64 {
    if total_count == 0 {
        return 0.0;
    }

    // Base strength on proportion of errors that are concurrent
    #[allow(clippy::cast_precision_loss)] // Precision loss acceptable for statistical calculations
    let proportion = concurrent_count as f64 / total_count as f64;

    // Apply sigmoid-like transformation to avoid extremes
    // Scale to 0.2-1.0 range
    let raw_strength = proportion.mul_add(0.8, 0.2);

    raw_strength.min(1.0)
}

fn detect_resource_exhaustion(
    metrics: &[&MetricPoint],
    error_logs: &[&LogEntry],
    time_window: &TimeRange,
    _config: &CorrelatorConfig,
) -> Option<Correlation> {
    // Look for high memory + OOM-related errors
    let high_memory = metrics.iter().any(|m| {
        (m.name.contains("memory") || m.name.contains("mem")) && m.value >= 90.0
    });

    let oom_errors = error_logs.iter().any(|l| {
        let msg_lower = l.message.to_lowercase();
        msg_lower.contains("oom")
            || msg_lower.contains("out of memory")
            || msg_lower.contains("memory exhausted")
            || msg_lower.contains("cannot allocate")
    });

    if high_memory && oom_errors {
        return Some(
            Correlation::new(
                "High memory usage correlated with OOM errors",
                time_window.clone(),
                CorrelationType::ResourceExhaustion,
            )
            .with_metric("memory_usage")
            .with_strength(0.9),
        );
    }

    None
}

/// Attempts to identify the root cause from a set of symptoms (insights).
///
/// Analyzes patterns in the insights to determine the most likely root cause.
#[must_use]
pub fn find_root_cause(symptoms: &[Insight]) -> Option<Insight> {
    if symptoms.is_empty() {
        return None;
    }

    // Priority order for root cause identification:
    // 1. Node offline (everything else could be a consequence)
    // 2. Resource exhaustion (memory, disk)
    // 3. Thermal throttling (affects GPU performance)
    // 4. Error spikes (could be symptom or cause)

    // Check for node offline
    if let Some(offline) = symptoms.iter().find(|i| {
        i.tags.contains(&"offline".to_string()) || i.title.to_lowercase().contains("offline")
    }) {
        return Some(
            Insight::new(
                Severity::Critical,
                "Root Cause: Node Failure",
                format!(
                    "The primary issue appears to be a node failure. \
                     Original insight: {}",
                    offline.title
                ),
            )
            .with_evidence("Node offline or unreachable")
            .with_evidence_list(offline.evidence.clone())
            .with_recommendation(
                "Investigate node connectivity and health. Check network, power, and hardware status.",
            )
            .with_tag("root_cause"),
        );
    }

    // Check for resource exhaustion
    if let Some(resource) = symptoms.iter().find(|i| {
        i.tags.contains(&"memory".to_string()) && i.severity == Severity::Critical
    }) {
        return Some(
            Insight::new(
                Severity::Critical,
                "Root Cause: Memory Exhaustion",
                format!(
                    "The primary issue appears to be memory exhaustion. \
                     Other symptoms may be cascading effects. Original insight: {}",
                    resource.title
                ),
            )
            .with_evidence_list(resource.evidence.clone())
            .with_recommendation(
                "Address memory exhaustion first. Scale up memory or identify and terminate \
                 memory-intensive processes.",
            )
            .with_tag("root_cause"),
        );
    }

    // Check for thermal throttling
    if let Some(thermal) = symptoms.iter().find(|i| {
        i.tags.contains(&"thermal".to_string())
    }) {
        return Some(
            Insight::new(
                Severity::Critical,
                "Root Cause: Thermal Throttling",
                format!(
                    "GPU thermal throttling detected. Performance degradation may be caused by \
                     overheating. Original insight: {}",
                    thermal.title
                ),
            )
            .with_evidence_list(thermal.evidence.clone())
            .with_recommendation(
                "Address cooling issues first. Check GPU fans, airflow, and ambient temperature.",
            )
            .with_tag("root_cause"),
        );
    }

    // If no clear root cause, return the most severe symptom
    symptoms
        .iter()
        .max_by_key(|i| i.severity.as_level())
        .map(|most_severe| {
            Insight::new(
                most_severe.severity,
                format!("Likely Root Cause: {}", most_severe.title),
                format!(
                    "Based on severity analysis, this appears to be the primary issue: {}",
                    most_severe.description
                ),
            )
            .with_evidence_list(most_severe.evidence.clone())
            .with_recommendation(
                most_severe
                    .recommendation
                    .clone()
                    .unwrap_or_else(|| "Review and address this issue first.".to_string()),
            )
            .with_tag("root_cause")
        })
}

/// Builds a chronological timeline of events from metrics and logs.
///
/// Useful for understanding the sequence of events leading to an issue.
#[must_use]
pub fn build_timeline(
    metrics: &[MetricPoint],
    logs: &[LogEntry],
    config: &CorrelatorConfig,
) -> Vec<TimelineEvent> {
    let mut events = Vec::new();

    // Add significant metric events
    for metric in metrics {
        if is_significant_metric(metric) {
            let severity = metric_to_severity(metric);
            let event = TimelineEvent::new(
                metric.timestamp,
                EventType::MetricThresholdExceeded,
                format!("{}: {:.2}", metric.name, metric.value),
                severity,
                &metric.name,
            )
            .with_value(metric.value);
            events.push(event);
        }
    }

    // Add error and warning logs
    for log in logs {
        let (event_type, severity) = match log.level {
            LogLevel::Error => (EventType::ErrorLogged, EventSeverity::High),
            LogLevel::Warn => (EventType::WarningLogged, EventSeverity::Medium),
            _ => continue, // Skip info/debug/trace
        };

        let source = log.source.clone().unwrap_or_else(|| "unknown".to_string());
        let event = TimelineEvent::new(
            log.timestamp,
            event_type,
            log.message.clone(),
            severity,
            source,
        );
        events.push(event);
    }

    // Detect state changes (metric normalization after spike)
    add_state_change_events(&mut events, metrics, config);

    // Sort by timestamp
    events.sort_by_key(|e| e.timestamp);

    events
}

fn is_significant_metric(metric: &MetricPoint) -> bool {
    // Check for common threshold exceedances
    let name_lower = metric.name.to_lowercase();

    if name_lower.contains("temperature") || name_lower.contains("temp") {
        return metric.value >= 80.0;
    }

    if name_lower.contains("memory") || name_lower.contains("mem") {
        return metric.value >= 85.0;
    }

    if name_lower.contains("cpu") {
        return metric.value >= 90.0;
    }

    if name_lower.contains("disk") || name_lower.contains("storage") {
        return metric.value >= 90.0;
    }

    // Default: consider high values significant
    metric.value >= 90.0
}

fn metric_to_severity(metric: &MetricPoint) -> EventSeverity {
    if metric.value >= 95.0 {
        EventSeverity::Critical
    } else if metric.value >= 90.0 {
        EventSeverity::High
    } else if metric.value >= 80.0 {
        EventSeverity::Medium
    } else {
        EventSeverity::Low
    }
}

fn add_state_change_events(
    events: &mut Vec<TimelineEvent>,
    metrics: &[MetricPoint],
    _config: &CorrelatorConfig,
) {
    // Group metrics by name
    let mut grouped: std::collections::HashMap<&str, Vec<&MetricPoint>> =
        std::collections::HashMap::new();
    for metric in metrics {
        grouped.entry(metric.name.as_str()).or_default().push(metric);
    }

    for (name, mut points) in grouped {
        if points.len() < 2 {
            continue;
        }

        points.sort_by_key(|m| m.timestamp);

        for window in points.windows(2) {
            let prev = window[0];
            let curr = window[1];

            // Check for normalization (high to normal)
            let prev_significant = is_significant_metric(prev);
            let curr_significant = is_significant_metric(curr);

            if prev_significant && !curr_significant {
                events.push(TimelineEvent::new(
                    curr.timestamp,
                    EventType::MetricNormalized,
                    format!("{} returned to normal: {:.2} -> {:.2}", name, prev.value, curr.value),
                    EventSeverity::Low,
                    name,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod correlation_tests {
        use super::*;

        #[test]
        fn test_correlation_creation() {
            let time_range = TimeRange::last_hours(1);
            let correlation = Correlation::new(
                "Test correlation",
                time_range,
                CorrelationType::MetricSpikeWithErrors,
            );

            assert_eq!(correlation.description, "Test correlation");
            assert!(!correlation.id.is_empty());
            assert!(correlation.metric_names.is_empty());
            assert!(correlation.log_sources.is_empty());
        }

        #[test]
        fn test_correlation_builder() {
            let time_range = TimeRange::last_hours(1);
            let correlation = Correlation::new(
                "Test",
                time_range,
                CorrelationType::ResourceExhaustion,
            )
            .with_metric("memory_usage")
            .with_metric("cpu_usage")
            .with_log_source("api-server")
            .with_strength(0.85);

            assert_eq!(correlation.metric_names.len(), 2);
            assert_eq!(correlation.log_sources.len(), 1);
            assert!((correlation.strength - 0.85).abs() < f64::EPSILON);
            assert!(correlation.is_strong());
        }

        #[test]
        fn test_correlation_strength_threshold() {
            let time_range = TimeRange::last_hours(1);

            let strong = Correlation::new("Strong", time_range.clone(), CorrelationType::MetricComovement)
                .with_strength(0.75);
            let weak = Correlation::new("Weak", time_range, CorrelationType::MetricComovement)
                .with_strength(0.5);

            assert!(strong.is_strong());
            assert!(!weak.is_strong());
        }
    }

    mod correlation_type_tests {
        use super::*;

        #[test]
        fn test_correlation_type_display() {
            assert_eq!(
                format!("{}", CorrelationType::MetricSpikeWithErrors),
                "Metric Spike with Errors"
            );
            assert_eq!(
                format!("{}", CorrelationType::ResourceExhaustion),
                "Resource Exhaustion"
            );
        }
    }

    mod timeline_event_tests {
        use super::*;

        #[test]
        fn test_timeline_event_creation() {
            let timestamp = Utc::now();
            let event = TimelineEvent::new(
                timestamp,
                EventType::ErrorLogged,
                "Connection timeout",
                EventSeverity::High,
                "api-server",
            );

            assert_eq!(event.timestamp, timestamp);
            assert_eq!(event.event_type, EventType::ErrorLogged);
            assert_eq!(event.description, "Connection timeout");
            assert_eq!(event.severity, EventSeverity::High);
            assert_eq!(event.source, "api-server");
            assert!(event.value.is_none());
        }

        #[test]
        fn test_timeline_event_with_value() {
            let event = TimelineEvent::new(
                Utc::now(),
                EventType::MetricThresholdExceeded,
                "CPU at 95%",
                EventSeverity::Critical,
                "cpu_usage",
            )
            .with_value(95.0);

            assert_eq!(event.value, Some(95.0));
        }
    }

    mod correlate_metrics_logs_tests {
        use super::*;

        #[test]
        fn test_empty_inputs_returns_empty() {
            let metrics: Vec<MetricPoint> = vec![];
            let logs: Vec<LogEntry> = vec![];
            let time_window = TimeRange::last_hours(1);
            let config = CorrelatorConfig::default();

            let correlations = correlate_metrics_logs(&metrics, &logs, &time_window, &config);
            assert!(correlations.is_empty());
        }

        #[test]
        fn test_no_errors_returns_empty() {
            let now = Utc::now();
            let metrics = vec![
                MetricPoint::new("cpu_usage", 50.0, now),
                MetricPoint::new("cpu_usage", 90.0, now + Duration::seconds(10)),
            ];
            let logs = vec![
                LogEntry::new(LogLevel::Info, "Request processed", now),
            ];
            let time_window = TimeRange::last_hours(1);
            let config = CorrelatorConfig::default();

            let correlations = correlate_metrics_logs(&metrics, &logs, &time_window, &config);
            assert!(correlations.is_empty());
        }

        #[test]
        fn test_spike_with_concurrent_errors_correlates() {
            let now = Utc::now();
            let metrics = vec![
                MetricPoint::new("cpu_usage", 30.0, now - Duration::minutes(2)),
                MetricPoint::new("cpu_usage", 95.0, now), // 200%+ spike
            ];
            let logs = vec![
                LogEntry::new(LogLevel::Error, "Service timeout", now + Duration::seconds(5))
                    .with_source("api-server"),
            ];
            let time_window = TimeRange::new(now - Duration::hours(1), now + Duration::hours(1));
            let config = CorrelatorConfig::default();

            let correlations = correlate_metrics_logs(&metrics, &logs, &time_window, &config);
            assert!(!correlations.is_empty());

            let corr = &correlations[0];
            assert_eq!(corr.correlation_type, CorrelationType::MetricSpikeWithErrors);
            assert!(corr.metric_names.contains(&"cpu_usage".to_string()));
        }

        #[test]
        fn test_resource_exhaustion_correlation() {
            let now = Utc::now();
            let metrics = vec![
                MetricPoint::new("memory_usage_percent", 95.0, now),
            ];
            let logs = vec![
                LogEntry::new(LogLevel::Error, "Out of memory: Cannot allocate buffer", now),
            ];
            let time_window = TimeRange::new(now - Duration::hours(1), now + Duration::hours(1));
            let config = CorrelatorConfig::default();

            let correlations = correlate_metrics_logs(&metrics, &logs, &time_window, &config);

            let exhaustion = correlations
                .iter()
                .find(|c| c.correlation_type == CorrelationType::ResourceExhaustion);
            assert!(exhaustion.is_some());
        }
    }

    mod find_root_cause_tests {
        use super::*;

        #[test]
        fn test_empty_symptoms_returns_none() {
            let symptoms: Vec<Insight> = vec![];
            let result = find_root_cause(&symptoms);
            assert!(result.is_none());
        }

        #[test]
        fn test_node_offline_is_root_cause() {
            let symptoms = vec![
                Insight::new(Severity::Warning, "High latency", "Latency increased")
                    .with_tag("performance"),
                Insight::new(Severity::Critical, "Node Offline", "Node unreachable")
                    .with_tag("offline")
                    .with_tag("node"),
            ];

            let result = find_root_cause(&symptoms);
            assert!(result.is_some());

            let root = result.expect("should have root cause");
            assert!(root.title.contains("Node Failure"));
            assert!(root.tags.contains(&"root_cause".to_string()));
        }

        #[test]
        fn test_memory_exhaustion_is_root_cause() {
            let symptoms = vec![
                Insight::new(Severity::Warning, "Slow queries", "DB queries slow")
                    .with_tag("database"),
                Insight::new(Severity::Critical, "Memory Critical", "Memory at 98%")
                    .with_tag("memory"),
            ];

            let result = find_root_cause(&symptoms);
            assert!(result.is_some());

            let root = result.expect("should have root cause");
            assert!(root.title.contains("Memory Exhaustion"));
        }

        #[test]
        fn test_thermal_throttling_is_root_cause() {
            let symptoms = vec![
                Insight::new(Severity::Warning, "Performance drop", "50% slower")
                    .with_tag("performance"),
                Insight::new(Severity::Critical, "GPU Thermal", "GPU at 95C")
                    .with_tag("thermal")
                    .with_tag("gpu"),
            ];

            let result = find_root_cause(&symptoms);
            assert!(result.is_some());

            let root = result.expect("should have root cause");
            assert!(root.title.contains("Thermal Throttling"));
        }

        #[test]
        fn test_fallback_to_most_severe() {
            let symptoms = vec![
                Insight::new(Severity::Info, "Normal traffic", "Expected levels"),
                Insight::new(Severity::Error, "Connection errors", "Multiple timeouts"),
            ];

            let result = find_root_cause(&symptoms);
            assert!(result.is_some());

            let root = result.expect("should have root cause");
            assert!(root.title.contains("Connection errors"));
        }
    }

    mod build_timeline_tests {
        use super::*;

        #[test]
        fn test_empty_inputs_returns_empty() {
            let metrics: Vec<MetricPoint> = vec![];
            let logs: Vec<LogEntry> = vec![];
            let config = CorrelatorConfig::default();

            let timeline = build_timeline(&metrics, &logs, &config);
            assert!(timeline.is_empty());
        }

        #[test]
        fn test_significant_metrics_appear_in_timeline() {
            let now = Utc::now();
            let metrics = vec![
                MetricPoint::new("cpu_usage", 95.0, now),
                MetricPoint::new("cpu_usage", 50.0, now + Duration::seconds(10)), // not significant
            ];
            let logs: Vec<LogEntry> = vec![];
            let config = CorrelatorConfig::default();

            let timeline = build_timeline(&metrics, &logs, &config);

            // Only the 95% should appear
            let threshold_events: Vec<_> = timeline
                .iter()
                .filter(|e| e.event_type == EventType::MetricThresholdExceeded)
                .collect();
            assert_eq!(threshold_events.len(), 1);
            assert!((threshold_events[0].value.expect("should have value") - 95.0).abs() < f64::EPSILON);
        }

        #[test]
        fn test_error_logs_appear_in_timeline() {
            let now = Utc::now();
            let metrics: Vec<MetricPoint> = vec![];
            let logs = vec![
                LogEntry::new(LogLevel::Error, "Connection failed", now)
                    .with_source("database"),
                LogEntry::new(LogLevel::Info, "Request processed", now), // should be filtered
                LogEntry::new(LogLevel::Warn, "Slow query", now + Duration::seconds(5))
                    .with_source("database"),
            ];
            let config = CorrelatorConfig::default();

            let timeline = build_timeline(&metrics, &logs, &config);

            assert_eq!(timeline.len(), 2); // Only error and warning

            let error_event = timeline
                .iter()
                .find(|e| e.event_type == EventType::ErrorLogged);
            assert!(error_event.is_some());
            assert_eq!(
                error_event.expect("should have error").description,
                "Connection failed"
            );
        }

        #[test]
        fn test_timeline_is_sorted_chronologically() {
            let now = Utc::now();
            let metrics = vec![
                MetricPoint::new("cpu_usage", 95.0, now + Duration::seconds(30)),
            ];
            let logs = vec![
                LogEntry::new(LogLevel::Error, "Error 1", now),
                LogEntry::new(LogLevel::Error, "Error 2", now + Duration::minutes(1)),
            ];
            let config = CorrelatorConfig::default();

            let timeline = build_timeline(&metrics, &logs, &config);

            // Verify chronological order
            for window in timeline.windows(2) {
                assert!(window[0].timestamp <= window[1].timestamp);
            }
        }

        #[test]
        fn test_metric_normalization_event() {
            let now = Utc::now();
            let metrics = vec![
                MetricPoint::new("cpu_usage", 95.0, now),
                MetricPoint::new("cpu_usage", 50.0, now + Duration::seconds(10)),
            ];
            let logs: Vec<LogEntry> = vec![];
            let config = CorrelatorConfig::default();

            let timeline = build_timeline(&metrics, &logs, &config);

            let normalized = timeline
                .iter()
                .find(|e| e.event_type == EventType::MetricNormalized);
            assert!(normalized.is_some());
        }
    }

    mod event_severity_tests {
        use super::*;

        #[test]
        fn test_event_severity_ordering() {
            assert!(EventSeverity::Low < EventSeverity::Medium);
            assert!(EventSeverity::Medium < EventSeverity::High);
            assert!(EventSeverity::High < EventSeverity::Critical);
        }
    }
}
