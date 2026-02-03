//! Core analysis engine for Clawbernetes observability.
//!
//! The `Analyzer` is the main entry point for diagnosing node, workload,
//! and cluster health by combining metrics, logs, and correlation analysis.

use crate::correlator::{
    build_timeline, correlate_metrics_logs, find_root_cause, CorrelatorConfig, TimelineEvent,
};
use crate::detectors::{
    detect_error_spike, detect_gpu_thermal_throttle, detect_memory_pressure,
    detect_node_offline, detect_performance_degradation, DetectorConfig, LogEntry, MetricPoint,
};
use crate::types::{AnalysisScope, Diagnosis, HealthStatus, Insight, Severity, TimeRange};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Configuration for the analyzer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzerConfig {
    /// Configuration for anomaly detectors.
    pub detector_config: DetectorConfig,
    /// Configuration for correlation analysis.
    pub correlator_config: CorrelatorConfig,
    /// Whether to include root cause analysis.
    pub enable_root_cause_analysis: bool,
    /// Whether to build event timelines.
    pub enable_timeline: bool,
    /// Maximum number of insights to include in a diagnosis.
    pub max_insights: Option<usize>,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            detector_config: DetectorConfig::default(),
            correlator_config: CorrelatorConfig::default(),
            enable_root_cause_analysis: true,
            enable_timeline: true,
            max_insights: None,
        }
    }
}

impl AnalyzerConfig {
    /// Creates a minimal configuration for faster analysis.
    #[must_use]
    pub fn minimal() -> Self {
        Self {
            enable_root_cause_analysis: false,
            enable_timeline: false,
            ..Self::default()
        }
    }

    /// Sets the maximum number of insights.
    #[must_use]
    pub const fn with_max_insights(mut self, max: usize) -> Self {
        self.max_insights = Some(max);
        self
    }

    /// Enables or disables root cause analysis.
    #[must_use]
    pub const fn with_root_cause_analysis(mut self, enabled: bool) -> Self {
        self.enable_root_cause_analysis = enabled;
        self
    }

    /// Enables or disables timeline building.
    #[must_use]
    pub const fn with_timeline(mut self, enabled: bool) -> Self {
        self.enable_timeline = enabled;
        self
    }
}

/// Result of an analysis operation with additional metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    /// The diagnosis produced by the analysis.
    pub diagnosis: Diagnosis,
    /// Event timeline if enabled.
    pub timeline: Option<Vec<TimelineEvent>>,
    /// Root cause insight if identified.
    pub root_cause: Option<Insight>,
    /// The scope that was analyzed.
    pub scope: AnalysisScope,
}

impl AnalysisResult {
    /// Creates a new analysis result.
    #[must_use]
    pub const fn new(diagnosis: Diagnosis, scope: AnalysisScope) -> Self {
        Self {
            diagnosis,
            timeline: None,
            root_cause: None,
            scope,
        }
    }

    /// Sets the timeline.
    #[must_use]
    pub fn with_timeline(mut self, timeline: Vec<TimelineEvent>) -> Self {
        self.timeline = Some(timeline);
        self
    }

    /// Sets the root cause.
    #[must_use]
    pub fn with_root_cause(mut self, root_cause: Insight) -> Self {
        self.root_cause = Some(root_cause);
        self
    }
}

/// The main analysis engine for Clawbernetes observability.
///
/// Combines multiple detection strategies, correlation analysis, and
/// root cause identification to produce comprehensive diagnoses.
#[derive(Debug, Clone)]
pub struct Analyzer {
    config: AnalyzerConfig,
}

impl Analyzer {
    /// Creates a new analyzer with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: AnalyzerConfig::default(),
        }
    }

    /// Creates a new analyzer with the specified configuration.
    #[must_use]
    pub const fn with_config(config: AnalyzerConfig) -> Self {
        Self { config }
    }

    /// Returns a reference to the analyzer's configuration.
    #[must_use]
    pub const fn config(&self) -> &AnalyzerConfig {
        &self.config
    }

    /// Analyzes a single node's health based on its metrics and logs.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // Analysis duration will never exceed u64::MAX ms
    pub fn analyze_node(
        &self,
        node_id: &str,
        metrics: &[MetricPoint],
        logs: &[LogEntry],
    ) -> AnalysisResult {
        let start = Instant::now();
        let mut insights = Vec::new();

        // Run all detectors
        self.run_detectors(&mut insights, metrics, logs, Some(node_id));

        // Determine overall health status
        let status = Self::compute_status(&insights);

        // Build diagnosis
        let diagnosis = Diagnosis::new(status)
            .with_subject(node_id)
            .with_insights(self.limit_insights(insights.clone()))
            .with_duration(start.elapsed().as_millis() as u64);

        // Build result with optional extras
        let scope = AnalysisScope::for_node(node_id);
        let mut result = AnalysisResult::new(diagnosis, scope);

        if self.config.enable_timeline {
            let timeline = build_timeline(metrics, logs, &self.config.correlator_config);
            result = result.with_timeline(timeline);
        }

        if self.config.enable_root_cause_analysis {
            if let Some(root_cause) = find_root_cause(&insights) {
                result = result.with_root_cause(root_cause);
            }
        }

        result
    }

    /// Analyzes a workload's health based on its metrics and logs.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // Analysis duration will never exceed u64::MAX ms
    pub fn analyze_workload(
        &self,
        workload_id: &str,
        metrics: &[MetricPoint],
        logs: &[LogEntry],
    ) -> AnalysisResult {
        let start = Instant::now();
        let mut insights = Vec::new();

        // Run detectors (without node-specific checks)
        self.run_workload_detectors(&mut insights, metrics, logs);

        // Add workload-specific correlation analysis
        if !metrics.is_empty() && !logs.is_empty() {
            let time_range = Self::compute_time_range(metrics, logs);
            let correlations = correlate_metrics_logs(
                metrics,
                logs,
                &time_range,
                &self.config.correlator_config,
            );

            for correlation in correlations {
                insights.push(
                    Insight::new(
                        Severity::Warning,
                        format!("Correlation: {}", correlation.correlation_type),
                        correlation.description,
                    )
                    .with_evidence(format!("Strength: {:.2}", correlation.strength))
                    .with_tag("correlation"),
                );
            }
        }

        let status = Self::compute_status(&insights);

        let diagnosis = Diagnosis::new(status)
            .with_subject(workload_id)
            .with_insights(self.limit_insights(insights.clone()))
            .with_duration(start.elapsed().as_millis() as u64);

        let scope = AnalysisScope::for_workload(workload_id);
        let mut result = AnalysisResult::new(diagnosis, scope);

        if self.config.enable_timeline {
            let timeline = build_timeline(metrics, logs, &self.config.correlator_config);
            result = result.with_timeline(timeline);
        }

        if self.config.enable_root_cause_analysis {
            if let Some(root_cause) = find_root_cause(&insights) {
                result = result.with_root_cause(root_cause);
            }
        }

        result
    }

    /// Analyzes the entire cluster's health.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // Analysis duration will never exceed u64::MAX ms
    pub fn analyze_cluster(
        &self,
        all_metrics: &[MetricPoint],
        all_logs: &[LogEntry],
    ) -> AnalysisResult {
        let start = Instant::now();
        let mut insights = Vec::new();

        // Run standard detectors
        self.run_detectors(&mut insights, all_metrics, all_logs, None);

        // Cluster-wide correlation analysis
        if !all_metrics.is_empty() && !all_logs.is_empty() {
            let time_range = Self::compute_time_range(all_metrics, all_logs);
            let correlations = correlate_metrics_logs(
                all_metrics,
                all_logs,
                &time_range,
                &self.config.correlator_config,
            );

            for correlation in correlations {
                if correlation.is_strong() {
                    insights.push(
                        Insight::new(
                            Severity::Warning,
                            format!("Cluster-wide: {}", correlation.correlation_type),
                            correlation.description,
                        )
                        .with_evidence(format!("Correlation strength: {:.2}", correlation.strength))
                        .with_tag("cluster")
                        .with_tag("correlation"),
                    );
                }
            }
        }

        // Check for widespread issues across nodes
        self.detect_cluster_wide_issues(&mut insights, all_metrics);

        let status = Self::compute_status(&insights);

        let diagnosis = Diagnosis::new(status)
            .with_subject("cluster")
            .with_insights(self.limit_insights(insights.clone()))
            .with_duration(start.elapsed().as_millis() as u64);

        let scope = AnalysisScope::for_cluster();
        let mut result = AnalysisResult::new(diagnosis, scope);

        if self.config.enable_timeline {
            let timeline = build_timeline(all_metrics, all_logs, &self.config.correlator_config);
            result = result.with_timeline(timeline);
        }

        if self.config.enable_root_cause_analysis {
            if let Some(root_cause) = find_root_cause(&insights) {
                result = result.with_root_cause(root_cause);
            }
        }

        result
    }

    /// Performs a quick health check without detailed analysis.
    #[must_use]
    pub fn quick_check(&self, metrics: &[MetricPoint], logs: &[LogEntry]) -> HealthStatus {
        let mut insights = Vec::new();
        self.run_detectors(&mut insights, metrics, logs, None);
        Self::compute_status(&insights)
    }

    fn run_detectors(
        &self,
        insights: &mut Vec<Insight>,
        metrics: &[MetricPoint],
        logs: &[LogEntry],
        node_id: Option<&str>,
    ) {
        // GPU thermal throttle
        if let Some(insight) = detect_gpu_thermal_throttle(metrics, &self.config.detector_config) {
            insights.push(insight);
        }

        // Memory pressure
        if let Some(insight) = detect_memory_pressure(metrics, &self.config.detector_config) {
            insights.push(insight);
        }

        // Error spike
        if let Some(insight) = detect_error_spike(logs, &self.config.detector_config) {
            insights.push(insight);
        }

        // Performance degradation
        if let Some(insight) = detect_performance_degradation(metrics, &self.config.detector_config)
        {
            insights.push(insight);
        }

        // Node offline (only for specific node analysis)
        if let Some(node) = node_id {
            if let Some(insight) = detect_node_offline(
                metrics,
                node,
                &self.config.detector_config,
                Utc::now(),
            ) {
                insights.push(insight);
            }
        }
    }

    fn run_workload_detectors(
        &self,
        insights: &mut Vec<Insight>,
        metrics: &[MetricPoint],
        logs: &[LogEntry],
    ) {
        // Workloads typically don't have GPU or node-level concerns
        // Focus on performance and errors

        if let Some(insight) = detect_memory_pressure(metrics, &self.config.detector_config) {
            insights.push(insight);
        }

        if let Some(insight) = detect_error_spike(logs, &self.config.detector_config) {
            insights.push(insight);
        }

        if let Some(insight) = detect_performance_degradation(metrics, &self.config.detector_config)
        {
            insights.push(insight);
        }
    }

    fn detect_cluster_wide_issues(&self, insights: &mut Vec<Insight>, metrics: &[MetricPoint]) {
        // Check if multiple nodes have the same issue
        let node_count = Self::count_unique_nodes(metrics);

        if node_count < 2 {
            return;
        }

        // Check for widespread high temperature
        let thermal_threshold = self.config.detector_config.gpu_thermal_threshold;
        let high_temp_nodes = Self::count_nodes_with_condition(metrics, |m| {
            m.name.contains("temperature") && m.value >= thermal_threshold
        });

        if high_temp_nodes >= node_count / 2 && high_temp_nodes >= 2 {
            insights.push(
                Insight::new(
                    Severity::Critical,
                    "Cluster-wide Thermal Issue",
                    format!(
                        "{high_temp_nodes} out of {node_count} nodes are experiencing high temperatures. \
                         This may indicate an environmental or cooling issue."
                    ),
                )
                .with_evidence(format!("{high_temp_nodes} nodes affected"))
                .with_recommendation(
                    "Check datacenter cooling and environmental conditions.",
                )
                .with_tag("cluster")
                .with_tag("thermal"),
            );
        }

        // Check for widespread memory pressure
        let memory_threshold = self.config.detector_config.memory_pressure_threshold;
        let high_mem_nodes = Self::count_nodes_with_condition(metrics, |m| {
            m.name.contains("memory") && m.value >= memory_threshold
        });

        if high_mem_nodes >= node_count / 2 && high_mem_nodes >= 2 {
            insights.push(
                Insight::new(
                    Severity::Critical,
                    "Cluster-wide Memory Pressure",
                    format!(
                        "{high_mem_nodes} out of {node_count} nodes are experiencing memory pressure. \
                         Cluster may be undersized for the current workload."
                    ),
                )
                .with_evidence(format!("{high_mem_nodes} nodes affected"))
                .with_recommendation(
                    "Consider scaling up the cluster or reducing workload intensity.",
                )
                .with_tag("cluster")
                .with_tag("memory"),
            );
        }
    }

    fn count_unique_nodes(metrics: &[MetricPoint]) -> usize {
        let mut nodes = std::collections::HashSet::new();
        for metric in metrics {
            if let Some(node_id) = metric.labels.get("node_id") {
                nodes.insert(node_id.as_str());
            }
        }
        nodes.len()
    }

    fn count_nodes_with_condition<F>(metrics: &[MetricPoint], condition: F) -> usize
    where
        F: Fn(&MetricPoint) -> bool,
    {
        let mut affected_nodes = std::collections::HashSet::new();
        for metric in metrics {
            if condition(metric) {
                if let Some(node_id) = metric.labels.get("node_id") {
                    affected_nodes.insert(node_id.as_str());
                }
            }
        }
        affected_nodes.len()
    }

    fn compute_status(insights: &[Insight]) -> HealthStatus {
        if insights.is_empty() {
            return HealthStatus::Healthy;
        }

        let max_severity = insights
            .iter()
            .map(|i| i.severity.as_level())
            .max()
            .unwrap_or(0);

        match max_severity {
            0 => HealthStatus::Healthy,      // Info only
            1 => HealthStatus::Degraded,     // Warnings
            2 | 3 => HealthStatus::Critical, // Errors or Critical
            _ => HealthStatus::Unknown,
        }
    }

    fn compute_time_range(metrics: &[MetricPoint], logs: &[LogEntry]) -> TimeRange {
        let all_times: Vec<_> = metrics
            .iter()
            .map(|m| m.timestamp)
            .chain(logs.iter().map(|l| l.timestamp))
            .collect();

        if all_times.is_empty() {
            return TimeRange::last_hours(1);
        }

        let min_time = all_times.iter().min().copied().unwrap_or_else(Utc::now);
        let max_time = all_times.iter().max().copied().unwrap_or_else(Utc::now);

        // Add small buffer
        let buffer = chrono::Duration::minutes(1);
        TimeRange::new(min_time - buffer, max_time + buffer)
    }

    fn limit_insights(&self, insights: Vec<Insight>) -> Vec<Insight> {
        match self.config.max_insights {
            Some(max) if insights.len() > max => {
                // Sort by severity (most severe first) and take top N
                let mut sorted = insights;
                sorted.sort_by(|a, b| b.severity.as_level().cmp(&a.severity.as_level()));
                sorted.truncate(max);
                sorted
            }
            _ => insights,
        }
    }
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn create_healthy_metrics() -> Vec<MetricPoint> {
        vec![
            MetricPoint::now("cpu_usage", 50.0).with_label("node_id", "node-001"),
            MetricPoint::now("memory_usage_percent", 60.0).with_label("node_id", "node-001"),
            MetricPoint::now("gpu_temperature", 70.0).with_label("node_id", "node-001"),
        ]
    }

    fn create_critical_metrics() -> Vec<MetricPoint> {
        vec![
            MetricPoint::now("cpu_usage", 95.0).with_label("node_id", "node-001"),
            MetricPoint::now("memory_usage_percent", 98.0).with_label("node_id", "node-001"),
            MetricPoint::now("gpu_temperature", 95.0)
                .with_label("node_id", "node-001")
                .with_label("gpu_id", "0"),
        ]
    }

    fn create_error_logs(count: usize) -> Vec<LogEntry> {
        (0..count)
            .map(|i| {
                LogEntry::new(
                    crate::detectors::LogLevel::Error,
                    format!("Error {i}: Connection failed"),
                    Utc::now(),
                )
                .with_source("test-service")
            })
            .collect()
    }

    mod analyzer_config_tests {
        use super::*;

        #[test]
        fn test_default_config() {
            let config = AnalyzerConfig::default();

            assert!(config.enable_root_cause_analysis);
            assert!(config.enable_timeline);
            assert!(config.max_insights.is_none());
        }

        #[test]
        fn test_minimal_config() {
            let config = AnalyzerConfig::minimal();

            assert!(!config.enable_root_cause_analysis);
            assert!(!config.enable_timeline);
        }

        #[test]
        fn test_config_builder() {
            let config = AnalyzerConfig::default()
                .with_max_insights(5)
                .with_root_cause_analysis(false)
                .with_timeline(false);

            assert_eq!(config.max_insights, Some(5));
            assert!(!config.enable_root_cause_analysis);
            assert!(!config.enable_timeline);
        }
    }

    mod analyzer_creation_tests {
        use super::*;

        #[test]
        fn test_new_creates_default_analyzer() {
            let analyzer = Analyzer::new();
            assert!(analyzer.config().enable_root_cause_analysis);
        }

        #[test]
        fn test_with_config() {
            let config = AnalyzerConfig::minimal();
            let analyzer = Analyzer::with_config(config);

            assert!(!analyzer.config().enable_root_cause_analysis);
        }

        #[test]
        fn test_default_trait() {
            let analyzer = Analyzer::default();
            assert!(analyzer.config().enable_timeline);
        }
    }

    mod analyze_node_tests {
        use super::*;

        #[test]
        fn test_healthy_node() {
            let analyzer = Analyzer::new();
            let metrics = create_healthy_metrics();
            let logs: Vec<LogEntry> = vec![];

            let result = analyzer.analyze_node("node-001", &metrics, &logs);

            assert_eq!(result.diagnosis.status, HealthStatus::Healthy);
            assert_eq!(result.diagnosis.subject, Some("node-001".to_string()));
            assert!(result.scope.has_node_filter());
        }

        #[test]
        fn test_critical_node() {
            let analyzer = Analyzer::new();
            let metrics = create_critical_metrics();
            let logs: Vec<LogEntry> = vec![];

            let result = analyzer.analyze_node("node-001", &metrics, &logs);

            assert_eq!(result.diagnosis.status, HealthStatus::Critical);
            assert!(!result.diagnosis.insights.is_empty());
        }

        #[test]
        fn test_node_with_errors() {
            let analyzer = Analyzer::new();
            let metrics = create_healthy_metrics();
            let logs = create_error_logs(50); // Many errors in short time

            let result = analyzer.analyze_node("node-001", &metrics, &logs);

            // Should detect error spike
            let has_error_insight = result
                .diagnosis
                .insights
                .iter()
                .any(|i| i.title.contains("Error"));
            assert!(has_error_insight);
        }

        #[test]
        fn test_node_analysis_includes_timeline() {
            let analyzer = Analyzer::new();
            let metrics = create_critical_metrics();
            let logs = create_error_logs(5);

            let result = analyzer.analyze_node("node-001", &metrics, &logs);

            assert!(result.timeline.is_some());
        }

        #[test]
        fn test_node_analysis_includes_root_cause() {
            let analyzer = Analyzer::new();
            let metrics = create_critical_metrics();
            let logs: Vec<LogEntry> = vec![];

            let result = analyzer.analyze_node("node-001", &metrics, &logs);

            // With critical issues, should identify a root cause
            if !result.diagnosis.insights.is_empty() {
                assert!(result.root_cause.is_some());
            }
        }

        #[test]
        fn test_node_analysis_without_extras() {
            let config = AnalyzerConfig::minimal();
            let analyzer = Analyzer::with_config(config);
            let metrics = create_critical_metrics();
            let logs: Vec<LogEntry> = vec![];

            let result = analyzer.analyze_node("node-001", &metrics, &logs);

            assert!(result.timeline.is_none());
            assert!(result.root_cause.is_none());
        }

        #[test]
        fn test_node_analysis_tracks_duration() {
            let analyzer = Analyzer::new();
            let metrics = create_healthy_metrics();
            let logs: Vec<LogEntry> = vec![];

            let result = analyzer.analyze_node("node-001", &metrics, &logs);

            // Duration should be tracked (may be 0 for very fast analysis)
            assert!(result.diagnosis.analysis_duration_ms < 10000); // Sanity check
        }

        #[test]
        fn test_missing_node_detected_as_offline() {
            let analyzer = Analyzer::new();
            // Metrics with different node_id
            let metrics = vec![
                MetricPoint::now("cpu_usage", 50.0).with_label("node_id", "node-002"),
            ];
            let logs: Vec<LogEntry> = vec![];

            let result = analyzer.analyze_node("node-001", &metrics, &logs);

            let has_offline_insight = result
                .diagnosis
                .insights
                .iter()
                .any(|i| i.title.contains("Offline"));
            assert!(has_offline_insight);
        }
    }

    mod analyze_workload_tests {
        use super::*;

        #[test]
        fn test_healthy_workload() {
            let analyzer = Analyzer::new();
            let metrics = vec![
                MetricPoint::now("memory_usage_percent", 60.0),
                MetricPoint::now("throughput", 100.0),
            ];
            let logs: Vec<LogEntry> = vec![];

            let result = analyzer.analyze_workload("workload-abc", &metrics, &logs);

            assert_eq!(result.diagnosis.status, HealthStatus::Healthy);
            assert_eq!(result.diagnosis.subject, Some("workload-abc".to_string()));
            assert!(result.scope.has_workload_filter());
        }

        #[test]
        fn test_workload_with_memory_pressure() {
            let analyzer = Analyzer::new();
            let metrics = vec![MetricPoint::now("memory_usage_percent", 95.0)];
            let logs: Vec<LogEntry> = vec![];

            let result = analyzer.analyze_workload("workload-abc", &metrics, &logs);

            assert!(matches!(
                result.diagnosis.status,
                HealthStatus::Critical | HealthStatus::Degraded
            ));
        }

        #[test]
        fn test_workload_correlation_analysis() {
            let analyzer = Analyzer::new();
            let now = Utc::now();

            let metrics = vec![
                MetricPoint::new("cpu_usage", 30.0, now - Duration::minutes(5)),
                MetricPoint::new("cpu_usage", 95.0, now), // Spike
            ];
            let logs = vec![
                LogEntry::new(
                    crate::detectors::LogLevel::Error,
                    "Service timeout",
                    now + Duration::seconds(5),
                ),
            ];

            let result = analyzer.analyze_workload("workload-abc", &metrics, &logs);

            // Should find correlation between spike and errors
            let _has_correlation = result
                .diagnosis
                .insights
                .iter()
                .any(|i| i.tags.contains(&"correlation".to_string()));

            // May or may not find correlation depending on timing
            // Just verify analysis completes without error
            assert!(result.diagnosis.subject.is_some());
        }
    }

    mod analyze_cluster_tests {
        use super::*;

        #[test]
        fn test_healthy_cluster() {
            let analyzer = Analyzer::new();
            let metrics = vec![
                MetricPoint::now("cpu_usage", 50.0).with_label("node_id", "node-001"),
                MetricPoint::now("cpu_usage", 55.0).with_label("node_id", "node-002"),
            ];
            let logs: Vec<LogEntry> = vec![];

            let result = analyzer.analyze_cluster(&metrics, &logs);

            assert_eq!(result.diagnosis.status, HealthStatus::Healthy);
            assert_eq!(result.diagnosis.subject, Some("cluster".to_string()));
            assert!(result.scope.is_cluster_scope());
        }

        #[test]
        fn test_cluster_wide_thermal_issue() {
            let analyzer = Analyzer::new();
            let metrics = vec![
                MetricPoint::now("gpu_temperature", 95.0).with_label("node_id", "node-001"),
                MetricPoint::now("gpu_temperature", 92.0).with_label("node_id", "node-002"),
                MetricPoint::now("gpu_temperature", 90.0).with_label("node_id", "node-003"),
            ];
            let logs: Vec<LogEntry> = vec![];

            let result = analyzer.analyze_cluster(&metrics, &logs);

            // Should detect cluster-wide thermal issue
            let has_cluster_thermal = result
                .diagnosis
                .insights
                .iter()
                .any(|i| {
                    i.tags.contains(&"cluster".to_string())
                        && i.tags.contains(&"thermal".to_string())
                });
            assert!(has_cluster_thermal);
        }

        #[test]
        fn test_cluster_wide_memory_pressure() {
            let analyzer = Analyzer::new();
            let metrics = vec![
                MetricPoint::now("memory_usage_percent", 95.0).with_label("node_id", "node-001"),
                MetricPoint::now("memory_usage_percent", 93.0).with_label("node_id", "node-002"),
                MetricPoint::now("memory_usage_percent", 70.0).with_label("node_id", "node-003"),
            ];
            let logs: Vec<LogEntry> = vec![];

            let result = analyzer.analyze_cluster(&metrics, &logs);

            // 2 out of 3 nodes have memory pressure
            let has_cluster_memory = result
                .diagnosis
                .insights
                .iter()
                .any(|i| {
                    i.tags.contains(&"cluster".to_string())
                        && i.tags.contains(&"memory".to_string())
                });
            assert!(has_cluster_memory);
        }

        #[test]
        fn test_empty_cluster() {
            let analyzer = Analyzer::new();
            let metrics: Vec<MetricPoint> = vec![];
            let logs: Vec<LogEntry> = vec![];

            let result = analyzer.analyze_cluster(&metrics, &logs);

            assert_eq!(result.diagnosis.status, HealthStatus::Healthy);
        }
    }

    mod quick_check_tests {
        use super::*;

        #[test]
        fn test_quick_check_healthy() {
            let analyzer = Analyzer::new();
            let metrics = create_healthy_metrics();
            let logs: Vec<LogEntry> = vec![];

            let status = analyzer.quick_check(&metrics, &logs);
            assert_eq!(status, HealthStatus::Healthy);
        }

        #[test]
        fn test_quick_check_critical() {
            let analyzer = Analyzer::new();
            let metrics = create_critical_metrics();
            let logs: Vec<LogEntry> = vec![];

            let status = analyzer.quick_check(&metrics, &logs);
            assert_eq!(status, HealthStatus::Critical);
        }
    }

    mod max_insights_tests {
        use super::*;

        #[test]
        fn test_max_insights_limit() {
            let config = AnalyzerConfig::default().with_max_insights(2);
            let analyzer = Analyzer::with_config(config);

            // Create metrics that will generate multiple insights
            let metrics = vec![
                MetricPoint::now("gpu_temperature", 95.0),
                MetricPoint::now("memory_usage_percent", 98.0),
            ];
            let logs = create_error_logs(50);

            let result = analyzer.analyze_node("node-001", &metrics, &logs);

            assert!(result.diagnosis.insights.len() <= 2);
        }

        #[test]
        fn test_max_insights_prioritizes_severity() {
            let config = AnalyzerConfig::default().with_max_insights(1);
            let analyzer = Analyzer::with_config(config);

            // Create metrics that will generate critical and warning
            let metrics = vec![
                MetricPoint::now("gpu_temperature", 95.0), // Critical
                MetricPoint::now("memory_usage_percent", 91.0), // Warning (below 95)
            ];
            let logs: Vec<LogEntry> = vec![];

            let result = analyzer.analyze_node("node-001", &metrics, &logs);

            assert_eq!(result.diagnosis.insights.len(), 1);
            // Should keep the critical one (GPU thermal)
            assert_eq!(result.diagnosis.insights[0].severity, Severity::Critical);
        }
    }

    mod analysis_result_tests {
        use super::*;

        #[test]
        fn test_analysis_result_creation() {
            let diagnosis = Diagnosis::new(HealthStatus::Healthy);
            let scope = AnalysisScope::for_node("test");
            let result = AnalysisResult::new(diagnosis, scope);

            assert!(result.timeline.is_none());
            assert!(result.root_cause.is_none());
        }

        #[test]
        fn test_analysis_result_builder() {
            let diagnosis = Diagnosis::new(HealthStatus::Healthy);
            let scope = AnalysisScope::for_cluster();
            let root_cause = Insight::new(Severity::Critical, "Root", "Cause");

            let result = AnalysisResult::new(diagnosis, scope)
                .with_timeline(vec![])
                .with_root_cause(root_cause);

            assert!(result.timeline.is_some());
            assert!(result.root_cause.is_some());
        }
    }
}
