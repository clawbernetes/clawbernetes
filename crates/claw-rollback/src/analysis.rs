//! Root cause analysis for deployment failures.
//!
//! This module provides functionality to analyze deployment failures
//! and determine the likely root cause based on metrics, logs, and
//! deployment configuration.

use crate::types::{
    DeploymentSnapshot, LogEntry, LogLevel, Metrics, RootCause, RootCauseCategory,
};
use serde::{Deserialize, Serialize};

/// Configuration for root cause analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    /// Threshold for considering error rate as high.
    pub high_error_rate_threshold: f64,
    /// Threshold for considering memory utilization as high.
    pub high_memory_threshold: f64,
    /// Threshold for considering CPU utilization as high.
    pub high_cpu_threshold: f64,
    /// Keywords that indicate configuration errors.
    pub config_error_keywords: Vec<String>,
    /// Keywords that indicate dependency failures.
    pub dependency_keywords: Vec<String>,
    /// Keywords that indicate code bugs.
    pub code_bug_keywords: Vec<String>,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            high_error_rate_threshold: 10.0,
            high_memory_threshold: 90.0,
            high_cpu_threshold: 95.0,
            config_error_keywords: vec![
                "config".to_string(),
                "configuration".to_string(),
                "environment".to_string(),
                "env".to_string(),
                "secret".to_string(),
                "key".to_string(),
                "missing".to_string(),
                "invalid".to_string(),
                "undefined".to_string(),
            ],
            dependency_keywords: vec![
                "connection".to_string(),
                "timeout".to_string(),
                "refused".to_string(),
                "unreachable".to_string(),
                "database".to_string(),
                "redis".to_string(),
                "kafka".to_string(),
                "api".to_string(),
                "service".to_string(),
                "upstream".to_string(),
                "downstream".to_string(),
            ],
            code_bug_keywords: vec![
                "null".to_string(),
                "undefined".to_string(),
                "panic".to_string(),
                "exception".to_string(),
                "error".to_string(),
                "stack".to_string(),
                "trace".to_string(),
                "assertion".to_string(),
                "failed".to_string(),
            ],
        }
    }
}

impl AnalysisConfig {
    /// Creates a new analysis configuration with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the high error rate threshold.
    #[must_use]
    pub fn with_high_error_rate_threshold(mut self, threshold: f64) -> Self {
        self.high_error_rate_threshold = threshold;
        self
    }

    /// Sets the high memory threshold.
    #[must_use]
    pub fn with_high_memory_threshold(mut self, threshold: f64) -> Self {
        self.high_memory_threshold = threshold;
        self
    }

    /// Sets the high CPU threshold.
    #[must_use]
    pub fn with_high_cpu_threshold(mut self, threshold: f64) -> Self {
        self.high_cpu_threshold = threshold;
        self
    }

    /// Adds config error keywords.
    #[must_use]
    pub fn with_config_keywords(mut self, keywords: Vec<String>) -> Self {
        self.config_error_keywords.extend(keywords);
        self
    }

    /// Adds dependency keywords.
    #[must_use]
    pub fn with_dependency_keywords(mut self, keywords: Vec<String>) -> Self {
        self.dependency_keywords.extend(keywords);
        self
    }
}

/// Analyzes deployment failures to determine root cause.
#[derive(Debug, Clone)]
pub struct FailureAnalyzer {
    /// Configuration for analysis.
    config: AnalysisConfig,
}

impl Default for FailureAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl FailureAnalyzer {
    /// Creates a new failure analyzer with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: AnalysisConfig::default(),
        }
    }

    /// Creates an analyzer with custom configuration.
    #[must_use]
    pub fn with_config(config: AnalysisConfig) -> Self {
        Self { config }
    }

    /// Returns a reference to the configuration.
    #[must_use]
    pub fn config(&self) -> &AnalysisConfig {
        &self.config
    }

    /// Analyzes a failure and returns the likely root cause.
    ///
    /// # Arguments
    ///
    /// * `snapshot` - The deployment snapshot that failed.
    /// * `metrics` - Current metrics at the time of failure.
    /// * `logs` - Log entries around the time of failure.
    ///
    /// # Returns
    ///
    /// A `RootCause` describing the likely cause of the failure.
    #[must_use]
    pub fn analyze_failure(
        &self,
        snapshot: &DeploymentSnapshot,
        metrics: &Metrics,
        logs: &[LogEntry],
    ) -> RootCause {
        let mut evidence = Vec::new();
        let mut scores = CategoryScores::default();

        // Analyze metrics
        self.analyze_metrics(metrics, &mut evidence, &mut scores);

        // Analyze logs
        self.analyze_logs(logs, &mut evidence, &mut scores);

        // Analyze deployment configuration
        self.analyze_config(snapshot, &mut evidence, &mut scores);

        // Determine the most likely category
        let category = scores.highest_category();

        // Generate description and recommendation
        let (description, recommendation) = self.generate_analysis(&category, &evidence, snapshot);

        RootCause {
            category,
            description,
            evidence,
            recommendation,
        }
    }

    /// Analyzes metrics for signs of failure.
    fn analyze_metrics(
        &self,
        metrics: &Metrics,
        evidence: &mut Vec<String>,
        scores: &mut CategoryScores,
    ) {
        // Check memory utilization
        if metrics.memory_utilization > self.config.high_memory_threshold {
            evidence.push(format!(
                "High memory utilization: {:.1}%",
                metrics.memory_utilization
            ));
            scores.resource_exhaustion += 2.0;
        }

        // Check CPU utilization
        if metrics.cpu_utilization > self.config.high_cpu_threshold {
            evidence.push(format!(
                "High CPU utilization: {:.1}%",
                metrics.cpu_utilization
            ));
            scores.resource_exhaustion += 2.0;
        }

        // Check error rate
        if metrics.error_rate > self.config.high_error_rate_threshold {
            evidence.push(format!("High error rate: {:.1}%", metrics.error_rate));
            scores.code_bug += 1.0;
        }

        // Check health check failures
        if metrics.health_check_failures > 0 {
            evidence.push(format!(
                "Health check failures: {}",
                metrics.health_check_failures
            ));
            if metrics.health_check_failures >= 3 {
                scores.dependency_failure += 1.0;
            }
        }

        // Check latency spike
        if metrics.p99_latency_ms > 0.0 && metrics.p50_latency_ms > 0.0 {
            let ratio = metrics.p99_latency_ms / metrics.p50_latency_ms;
            if ratio > 10.0 {
                evidence.push(format!(
                    "Latency spike: P99 ({:.0}ms) is {:.1}x P50 ({:.0}ms)",
                    metrics.p99_latency_ms, ratio, metrics.p50_latency_ms
                ));
                scores.dependency_failure += 1.0;
            }
        }
    }

    /// Analyzes logs for signs of failure.
    fn analyze_logs(
        &self,
        logs: &[LogEntry],
        evidence: &mut Vec<String>,
        scores: &mut CategoryScores,
    ) {
        let error_logs: Vec<_> = logs
            .iter()
            .filter(|l| l.level == LogLevel::Error)
            .collect();

        // Count error logs
        if !error_logs.is_empty() {
            evidence.push(format!("Found {} error log entries", error_logs.len()));
        }

        // Analyze log messages for keywords
        for log in logs {
            let message_lower = log.message.to_lowercase();

            // Check for config errors
            let config_matches: Vec<_> = self
                .config
                .config_error_keywords
                .iter()
                .filter(|k| message_lower.contains(&k.to_lowercase()))
                .collect();
            if !config_matches.is_empty() {
                scores.config_error += 1.0;
                if log.level == LogLevel::Error {
                    scores.config_error += 0.5;
                }
            }

            // Check for dependency failures
            let dependency_matches: Vec<_> = self
                .config
                .dependency_keywords
                .iter()
                .filter(|k| message_lower.contains(&k.to_lowercase()))
                .collect();
            if !dependency_matches.is_empty() {
                scores.dependency_failure += 1.0;
                if log.level == LogLevel::Error {
                    scores.dependency_failure += 0.5;
                }
            }

            // Check for code bugs
            let bug_matches: Vec<_> = self
                .config
                .code_bug_keywords
                .iter()
                .filter(|k| message_lower.contains(&k.to_lowercase()))
                .collect();
            if !bug_matches.is_empty() && log.level == LogLevel::Error {
                scores.code_bug += 1.0;
            }

            // Add specific error messages as evidence
            if log.level == LogLevel::Error && evidence.len() < 5 {
                let truncated = if log.message.len() > 100 {
                    format!("{}...", &log.message[..100])
                } else {
                    log.message.clone()
                };
                evidence.push(format!("Error log: {truncated}"));
            }
        }
    }

    /// Analyzes deployment configuration for issues.
    fn analyze_config(
        &self,
        snapshot: &DeploymentSnapshot,
        evidence: &mut Vec<String>,
        scores: &mut CategoryScores,
    ) {
        // Check for missing or suspicious environment variables
        let env = &snapshot.spec.env;
        
        // Look for common missing config patterns
        let suspicious_values: Vec<_> = env
            .iter()
            .filter(|(_, v)| {
                v.is_empty()
                    || v.starts_with("${")
                    || v.as_str() == "null"
                    || v.as_str() == "undefined"
                    || v.as_str() == "TODO"
            })
            .collect();

        if !suspicious_values.is_empty() {
            for (key, value) in &suspicious_values {
                evidence.push(format!("Suspicious env var: {key}={value}"));
            }
            scores.config_error += suspicious_values.len() as f64;
        }

        // Check for low resource limits
        let resources = &snapshot.spec.resources;
        if resources.memory_bytes < 128_000_000 {
            // Less than 128MB
            evidence.push("Low memory limit: <128MB".to_string());
            scores.resource_exhaustion += 0.5;
        }
        if resources.cpu_millis < 100 {
            // Less than 0.1 CPU
            evidence.push("Low CPU limit: <100m".to_string());
            scores.resource_exhaustion += 0.5;
        }
    }

    /// Generates a description and recommendation based on the analysis.
    fn generate_analysis(
        &self,
        category: &RootCauseCategory,
        evidence: &[String],
        snapshot: &DeploymentSnapshot,
    ) -> (String, String) {
        match category {
            RootCauseCategory::ConfigError => (
                format!(
                    "Configuration issue detected in deployment '{}'",
                    snapshot.spec.name
                ),
                "Review environment variables and configuration. Check for missing secrets or invalid values.".to_string(),
            ),
            RootCauseCategory::ResourceExhaustion => (
                format!(
                    "Resource exhaustion detected in deployment '{}'",
                    snapshot.spec.name
                ),
                format!(
                    "Consider increasing resource limits. Current: {}m CPU, {}MB memory",
                    snapshot.spec.resources.cpu_millis,
                    snapshot.spec.resources.memory_bytes / 1_000_000
                ),
            ),
            RootCauseCategory::DependencyFailure => (
                format!(
                    "Dependency failure detected affecting deployment '{}'",
                    snapshot.spec.name
                ),
                "Check connectivity to dependent services (databases, APIs, message queues). Verify network policies and service endpoints.".to_string(),
            ),
            RootCauseCategory::CodeBug => (
                format!(
                    "Potential code bug detected in deployment '{}'",
                    snapshot.spec.name
                ),
                "Review recent code changes. Check error logs for stack traces and exceptions. Consider rolling back to a known-good version.".to_string(),
            ),
            RootCauseCategory::Unknown => {
                if evidence.is_empty() {
                    (
                        format!(
                            "Unable to determine root cause for deployment '{}'",
                            snapshot.spec.name
                        ),
                        "Insufficient data for analysis. Enable detailed logging and monitoring.".to_string(),
                    )
                } else {
                    (
                        format!(
                            "Inconclusive analysis for deployment '{}'",
                            snapshot.spec.name
                        ),
                        "Multiple potential causes identified. Manual investigation recommended.".to_string(),
                    )
                }
            }
        }
    }
}

/// Scores for each root cause category.
#[derive(Debug, Default)]
struct CategoryScores {
    config_error: f64,
    resource_exhaustion: f64,
    dependency_failure: f64,
    code_bug: f64,
}

impl CategoryScores {
    /// Returns the category with the highest score.
    fn highest_category(&self) -> RootCauseCategory {
        let max_score = self
            .config_error
            .max(self.resource_exhaustion)
            .max(self.dependency_failure)
            .max(self.code_bug);

        if max_score == 0.0 {
            return RootCauseCategory::Unknown;
        }

        // Use a small epsilon for float comparison
        const EPSILON: f64 = 0.001;

        if (self.config_error - max_score).abs() < EPSILON {
            RootCauseCategory::ConfigError
        } else if (self.resource_exhaustion - max_score).abs() < EPSILON {
            RootCauseCategory::ResourceExhaustion
        } else if (self.dependency_failure - max_score).abs() < EPSILON {
            RootCauseCategory::DependencyFailure
        } else if (self.code_bug - max_score).abs() < EPSILON {
            RootCauseCategory::CodeBug
        } else {
            RootCauseCategory::Unknown
        }
    }
}

/// Analyzes a failure and returns the likely root cause.
///
/// This is a convenience function that uses the default analyzer configuration.
///
/// # Arguments
///
/// * `snapshot` - The deployment snapshot that failed.
/// * `metrics` - Current metrics at the time of failure.
/// * `logs` - Log entries around the time of failure.
///
/// # Returns
///
/// A `RootCause` describing the likely cause of the failure.
#[must_use]
pub fn analyze_failure(
    snapshot: &DeploymentSnapshot,
    metrics: &Metrics,
    logs: &[LogEntry],
) -> RootCause {
    FailureAnalyzer::new().analyze_failure(snapshot, metrics, logs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DeploymentId, DeploymentSpec, ResourceLimits};

    fn create_snapshot() -> DeploymentSnapshot {
        DeploymentSnapshot::new(
            DeploymentId::new("test-v1"),
            DeploymentSpec::new("test-app", "test-app:v1"),
        )
    }

    fn create_snapshot_with_env(env: Vec<(&str, &str)>) -> DeploymentSnapshot {
        let mut spec = DeploymentSpec::new("test-app", "test-app:v1");
        for (key, value) in env {
            spec = spec.with_env(key, value);
        }
        DeploymentSnapshot::new(DeploymentId::new("test-v1"), spec)
    }

    fn create_snapshot_with_resources(cpu_millis: u64, memory_bytes: u64) -> DeploymentSnapshot {
        let resources = ResourceLimits {
            cpu_millis,
            memory_bytes,
        };
        DeploymentSnapshot::new(
            DeploymentId::new("test-v1"),
            DeploymentSpec::new("test-app", "test-app:v1").with_resources(resources),
        )
    }

    mod analyzer_construction_tests {
        use super::*;

        #[test]
        fn new_creates_analyzer_with_defaults() {
            let analyzer = FailureAnalyzer::new();
            assert!((analyzer.config().high_error_rate_threshold - 10.0).abs() < f64::EPSILON);
            assert!((analyzer.config().high_memory_threshold - 90.0).abs() < f64::EPSILON);
        }

        #[test]
        fn with_config_uses_custom_config() {
            let config = AnalysisConfig::new().with_high_error_rate_threshold(5.0);
            let analyzer = FailureAnalyzer::with_config(config);
            assert!((analyzer.config().high_error_rate_threshold - 5.0).abs() < f64::EPSILON);
        }

        #[test]
        fn default_trait_works() {
            let analyzer = FailureAnalyzer::default();
            assert!((analyzer.config().high_error_rate_threshold - 10.0).abs() < f64::EPSILON);
        }
    }

    mod config_tests {
        use super::*;

        #[test]
        fn default_config_has_expected_values() {
            let config = AnalysisConfig::default();
            assert!((config.high_error_rate_threshold - 10.0).abs() < f64::EPSILON);
            assert!((config.high_memory_threshold - 90.0).abs() < f64::EPSILON);
            assert!((config.high_cpu_threshold - 95.0).abs() < f64::EPSILON);
            assert!(!config.config_error_keywords.is_empty());
            assert!(!config.dependency_keywords.is_empty());
        }

        #[test]
        fn builder_pattern_works() {
            let config = AnalysisConfig::new()
                .with_high_error_rate_threshold(15.0)
                .with_high_memory_threshold(85.0)
                .with_high_cpu_threshold(90.0);

            assert!((config.high_error_rate_threshold - 15.0).abs() < f64::EPSILON);
            assert!((config.high_memory_threshold - 85.0).abs() < f64::EPSILON);
            assert!((config.high_cpu_threshold - 90.0).abs() < f64::EPSILON);
        }

        #[test]
        fn with_keywords_extends_lists() {
            let config = AnalysisConfig::new()
                .with_config_keywords(vec!["custom_config".to_string()])
                .with_dependency_keywords(vec!["custom_dep".to_string()]);

            assert!(config.config_error_keywords.contains(&"custom_config".to_string()));
            assert!(config.dependency_keywords.contains(&"custom_dep".to_string()));
        }
    }

    mod resource_exhaustion_tests {
        use super::*;

        #[test]
        fn high_memory_indicates_resource_exhaustion() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let metrics = Metrics::new()
                .with_error_rate(1.0)
                .with_custom("memory_utilization", 95.0);
            // Set memory_utilization directly
            let mut metrics = metrics;
            metrics.memory_utilization = 95.0;

            let result = analyzer.analyze_failure(&snapshot, &metrics, &[]);

            assert_eq!(result.category, RootCauseCategory::ResourceExhaustion);
            assert!(result.evidence.iter().any(|e| e.contains("memory")));
        }

        #[test]
        fn high_cpu_indicates_resource_exhaustion() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let mut metrics = Metrics::new();
            metrics.cpu_utilization = 98.0;

            let result = analyzer.analyze_failure(&snapshot, &metrics, &[]);

            assert_eq!(result.category, RootCauseCategory::ResourceExhaustion);
            assert!(result.evidence.iter().any(|e| e.contains("CPU")));
        }

        #[test]
        fn low_resource_limits_indicate_resource_exhaustion() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot_with_resources(50, 64_000_000);
            let metrics = Metrics::new();

            let result = analyzer.analyze_failure(&snapshot, &metrics, &[]);

            assert_eq!(result.category, RootCauseCategory::ResourceExhaustion);
            assert!(result.evidence.iter().any(|e| e.contains("Low")));
        }
    }

    mod config_error_tests {
        use super::*;

        #[test]
        fn empty_env_var_indicates_config_error() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot_with_env(vec![("API_KEY", "")]);
            let metrics = Metrics::new();

            let result = analyzer.analyze_failure(&snapshot, &metrics, &[]);

            assert_eq!(result.category, RootCauseCategory::ConfigError);
            assert!(result.evidence.iter().any(|e| e.contains("API_KEY")));
        }

        #[test]
        fn unresolved_env_var_indicates_config_error() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot_with_env(vec![("DATABASE_URL", "${DB_URL}")]);
            let metrics = Metrics::new();

            let result = analyzer.analyze_failure(&snapshot, &metrics, &[]);

            assert_eq!(result.category, RootCauseCategory::ConfigError);
        }

        #[test]
        fn config_keyword_in_log_indicates_config_error() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let metrics = Metrics::new();
            let logs = vec![
                LogEntry::new(LogLevel::Error, "Missing configuration key: SECRET_KEY"),
            ];

            let result = analyzer.analyze_failure(&snapshot, &metrics, &logs);

            assert_eq!(result.category, RootCauseCategory::ConfigError);
        }
    }

    mod dependency_failure_tests {
        use super::*;

        #[test]
        fn connection_timeout_indicates_dependency_failure() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let metrics = Metrics::new();
            let logs = vec![
                LogEntry::new(LogLevel::Error, "Connection timeout to database"),
            ];

            let result = analyzer.analyze_failure(&snapshot, &metrics, &logs);

            assert_eq!(result.category, RootCauseCategory::DependencyFailure);
        }

        #[test]
        fn service_unreachable_indicates_dependency_failure() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let metrics = Metrics::new();
            let logs = vec![
                LogEntry::new(LogLevel::Error, "Service unreachable: api.example.com"),
            ];

            let result = analyzer.analyze_failure(&snapshot, &metrics, &logs);

            assert_eq!(result.category, RootCauseCategory::DependencyFailure);
        }

        #[test]
        fn latency_spike_indicates_dependency_failure() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let metrics = Metrics::new()
                .with_p50_latency_ms(10.0)
                .with_p99_latency_ms(500.0); // 50x ratio

            let result = analyzer.analyze_failure(&snapshot, &metrics, &[]);

            assert_eq!(result.category, RootCauseCategory::DependencyFailure);
            assert!(result.evidence.iter().any(|e| e.contains("Latency spike")));
        }

        #[test]
        fn multiple_health_check_failures_indicate_dependency() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let metrics = Metrics::new().with_health_check_failures(5);

            let result = analyzer.analyze_failure(&snapshot, &metrics, &[]);

            assert!(result.evidence.iter().any(|e| e.contains("Health check")));
        }
    }

    mod code_bug_tests {
        use super::*;

        #[test]
        fn exception_in_log_indicates_code_bug() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let metrics = Metrics::new().with_error_rate(50.0);
            let logs = vec![
                LogEntry::new(LogLevel::Error, "Unhandled exception in handler"),
                LogEntry::new(LogLevel::Error, "Stack trace: at main.rs:42"),
            ];

            let result = analyzer.analyze_failure(&snapshot, &metrics, &logs);

            assert_eq!(result.category, RootCauseCategory::CodeBug);
        }

        #[test]
        fn panic_in_log_indicates_code_bug() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let metrics = Metrics::new();
            let logs = vec![
                LogEntry::new(LogLevel::Error, "thread 'main' panicked at 'assertion failed'"),
            ];

            let result = analyzer.analyze_failure(&snapshot, &metrics, &logs);

            assert_eq!(result.category, RootCauseCategory::CodeBug);
        }

        #[test]
        fn high_error_rate_contributes_to_code_bug() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let metrics = Metrics::new().with_error_rate(25.0);
            let logs = vec![
                LogEntry::new(LogLevel::Error, "Request failed with error"),
            ];

            let result = analyzer.analyze_failure(&snapshot, &metrics, &logs);

            assert!(result.evidence.iter().any(|e| e.contains("error rate")));
        }
    }

    mod unknown_category_tests {
        use super::*;

        #[test]
        fn no_evidence_returns_unknown() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let metrics = Metrics::new();

            let result = analyzer.analyze_failure(&snapshot, &metrics, &[]);

            assert_eq!(result.category, RootCauseCategory::Unknown);
            assert!(result.recommendation.contains("Insufficient data"));
        }

        #[test]
        fn mixed_signals_may_return_unknown() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let metrics = Metrics::new();
            let logs = vec![
                LogEntry::new(LogLevel::Info, "Everything looks fine"),
            ];

            let result = analyzer.analyze_failure(&snapshot, &metrics, &logs);

            // With minimal signals, should be unknown
            assert_eq!(result.category, RootCauseCategory::Unknown);
        }
    }

    mod evidence_collection_tests {
        use super::*;

        #[test]
        fn error_logs_added_as_evidence() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let metrics = Metrics::new();
            let logs = vec![
                LogEntry::new(LogLevel::Error, "First error message"),
                LogEntry::new(LogLevel::Error, "Second error message"),
            ];

            let result = analyzer.analyze_failure(&snapshot, &metrics, &logs);

            assert!(result.evidence.iter().any(|e| e.contains("error log entries")));
        }

        #[test]
        fn long_messages_are_truncated() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let metrics = Metrics::new();
            let long_message = "A".repeat(200);
            let logs = vec![LogEntry::new(LogLevel::Error, &long_message)];

            let result = analyzer.analyze_failure(&snapshot, &metrics, &logs);

            // Should have truncated message in evidence
            let has_truncated = result.evidence.iter().any(|e| e.ends_with("..."));
            assert!(has_truncated);
        }

        #[test]
        fn evidence_limit_respected() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let metrics = Metrics::new();
            let logs: Vec<_> = (0..20)
                .map(|i| LogEntry::new(LogLevel::Error, format!("Error {i}")))
                .collect();

            let result = analyzer.analyze_failure(&snapshot, &metrics, &logs);

            // Should not have more than ~5-10 evidence items to avoid noise
            // The exact count depends on implementation, but should be bounded
            assert!(result.evidence.len() < 15);
        }
    }

    mod recommendation_tests {
        use super::*;

        #[test]
        fn config_error_recommendation_mentions_env_vars() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot_with_env(vec![("KEY", "")]);
            let metrics = Metrics::new();

            let result = analyzer.analyze_failure(&snapshot, &metrics, &[]);

            assert!(result.recommendation.to_lowercase().contains("environment")
                || result.recommendation.to_lowercase().contains("config"));
        }

        #[test]
        fn resource_exhaustion_recommendation_mentions_limits() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let mut metrics = Metrics::new();
            metrics.memory_utilization = 95.0;

            let result = analyzer.analyze_failure(&snapshot, &metrics, &[]);

            assert!(result.recommendation.to_lowercase().contains("limit")
                || result.recommendation.to_lowercase().contains("resource"));
        }

        #[test]
        fn dependency_failure_recommendation_mentions_connectivity() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let metrics = Metrics::new();
            let logs = vec![LogEntry::new(LogLevel::Error, "Connection refused to database")];

            let result = analyzer.analyze_failure(&snapshot, &metrics, &logs);

            assert!(result.recommendation.to_lowercase().contains("connect")
                || result.recommendation.to_lowercase().contains("service"));
        }

        #[test]
        fn code_bug_recommendation_mentions_rollback() {
            let analyzer = FailureAnalyzer::new();
            let snapshot = create_snapshot();
            let metrics = Metrics::new().with_error_rate(50.0);
            let logs = vec![LogEntry::new(LogLevel::Error, "panic in handler")];

            let result = analyzer.analyze_failure(&snapshot, &metrics, &logs);

            assert!(result.recommendation.to_lowercase().contains("rollback")
                || result.recommendation.to_lowercase().contains("code"));
        }
    }

    mod convenience_function_tests {
        use super::*;

        #[test]
        fn analyze_failure_function_works() {
            let snapshot = create_snapshot();
            let metrics = Metrics::new();
            let logs: Vec<LogEntry> = vec![];

            let result = analyze_failure(&snapshot, &metrics, &logs);

            // Should return a valid RootCause
            assert!(!result.description.is_empty());
            assert!(!result.recommendation.is_empty());
        }
    }

    mod serialization_tests {
        use super::*;

        #[test]
        fn analysis_config_serialization_roundtrip() {
            let config = AnalysisConfig::new()
                .with_high_error_rate_threshold(15.0)
                .with_config_keywords(vec!["custom".to_string()]);

            let json = serde_json::to_string(&config).unwrap_or_default();
            let deserialized: Result<AnalysisConfig, _> = serde_json::from_str(&json);

            assert!(deserialized.is_ok());
            let deserialized = deserialized.unwrap_or_else(|_| panic!("should deserialize"));
            assert!((deserialized.high_error_rate_threshold - 15.0).abs() < f64::EPSILON);
        }
    }
}
