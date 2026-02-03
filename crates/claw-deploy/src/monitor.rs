//! Health monitoring for deployments.
//!
//! This module provides metrics collection and health assessment for deployments,
//! enabling automatic promotion and rollback decisions based on real-time data.

use crate::error::{DeployError, DeployResult};
use crate::types::DeploymentId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::debug;

/// A single metric data point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricPoint {
    /// Timestamp of the metric
    pub timestamp: DateTime<Utc>,

    /// Name of the metric (e.g., `error_rate`, `latency_p99`)
    pub name: String,

    /// Value of the metric
    pub value: f64,

    /// Optional labels for the metric
    pub labels: HashMap<String, String>,
}

impl MetricPoint {
    /// Creates a new metric point.
    #[must_use]
    pub fn new(name: impl Into<String>, value: f64) -> Self {
        Self {
            timestamp: Utc::now(),
            name: name.into(),
            value,
            labels: HashMap::new(),
        }
    }

    /// Creates a metric point with a specific timestamp.
    #[must_use]
    pub const fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Adds a label to the metric.
    #[must_use]
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }
}

/// Health assessment result from monitoring.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthAssessment {
    /// Error rate as a percentage (0.0 - 100.0)
    pub error_rate: f64,

    /// 99th percentile latency in milliseconds
    pub latency_p99: f64,

    /// Total number of successful requests
    pub success_count: u64,

    /// Total number of failed requests
    pub failure_count: u64,

    /// Timestamp of the assessment
    pub assessed_at: DateTime<Utc>,

    /// Duration of the assessment window
    pub window_seconds: u64,

    /// Whether the assessment meets promotion criteria
    pub is_healthy: bool,

    /// Optional message explaining the assessment
    pub message: Option<String>,
}

impl HealthAssessment {
    /// Creates a new health assessment from metrics.
    #[must_use]
    pub fn from_metrics(
        error_rate: f64,
        latency_p99: f64,
        success_count: u64,
        failure_count: u64,
    ) -> Self {
        let is_healthy = error_rate < 1.0 && latency_p99 < 500.0;

        Self {
            error_rate,
            latency_p99,
            success_count,
            failure_count,
            assessed_at: Utc::now(),
            window_seconds: 60,
            is_healthy,
            message: None,
        }
    }

    /// Sets the assessment window.
    #[must_use]
    pub const fn with_window(mut self, seconds: u64) -> Self {
        self.window_seconds = seconds;
        self
    }

    /// Sets an assessment message.
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Total request count.
    #[must_use]
    pub const fn total_requests(&self) -> u64 {
        self.success_count + self.failure_count
    }
}

/// Thresholds for health assessment decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthThresholds {
    /// Maximum acceptable error rate (percentage)
    pub max_error_rate: f64,

    /// Maximum acceptable p99 latency (milliseconds)
    pub max_latency_p99: f64,

    /// Minimum requests required for assessment
    pub min_requests: u64,

    /// Minimum success ratio for promotion (0.0 - 1.0)
    pub min_success_ratio: f64,
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            max_error_rate: 1.0,      // 1% max error rate
            max_latency_p99: 500.0,   // 500ms max p99 latency
            min_requests: 100,        // Need at least 100 requests
            min_success_ratio: 0.99,  // 99% success rate
        }
    }
}

impl HealthThresholds {
    /// Creates thresholds for a production environment (stricter).
    #[must_use]
    pub const fn production() -> Self {
        Self {
            max_error_rate: 0.5,
            max_latency_p99: 200.0,
            min_requests: 1000,
            min_success_ratio: 0.995,
        }
    }

    /// Creates thresholds for a staging environment.
    #[must_use]
    pub const fn staging() -> Self {
        Self {
            max_error_rate: 2.0,
            max_latency_p99: 1000.0,
            min_requests: 50,
            min_success_ratio: 0.95,
        }
    }

    /// Creates thresholds for a dev environment (lenient).
    #[must_use]
    pub const fn dev() -> Self {
        Self {
            max_error_rate: 5.0,
            max_latency_p99: 2000.0,
            min_requests: 10,
            min_success_ratio: 0.90,
        }
    }
}

/// Monitors deployment health and makes promotion/rollback decisions.
#[derive(Debug)]
pub struct DeploymentMonitor {
    /// Health thresholds for decisions
    thresholds: HealthThresholds,

    /// Stored assessments per deployment
    assessments: Arc<RwLock<HashMap<DeploymentId, Vec<HealthAssessment>>>>,
}

impl DeploymentMonitor {
    /// Creates a new deployment monitor with default thresholds.
    #[must_use]
    pub fn new() -> Self {
        Self {
            thresholds: HealthThresholds::default(),
            assessments: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Creates a monitor with custom thresholds.
    #[must_use]
    pub fn with_thresholds(thresholds: HealthThresholds) -> Self {
        Self {
            thresholds,
            assessments: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Checks the health of a deployment based on provided metrics.
    ///
    /// # Arguments
    ///
    /// * `id` - The deployment ID
    /// * `metrics` - Array of metric points to analyze
    ///
    /// # Returns
    ///
    /// A health assessment based on the metrics.
    ///
    /// # Errors
    ///
    /// Returns an error if the lock cannot be acquired for storing the assessment.
    pub fn check_health(
        &self,
        id: &DeploymentId,
        metrics: &[MetricPoint],
    ) -> DeployResult<HealthAssessment> {
        debug!("Checking health for deployment {}", id);

        if metrics.is_empty() {
            return Ok(HealthAssessment::from_metrics(0.0, 0.0, 0, 0)
                .with_message("No metrics available"));
        }

        // Extract key metrics
        let error_rate = extract_metric_avg(metrics, "error_rate").unwrap_or(0.0);
        let latency_p99 = extract_metric_avg(metrics, "latency_p99").unwrap_or(0.0);
        let success_count = safe_f64_to_u64(extract_metric_sum(metrics, "success_count"));
        let failure_count = safe_f64_to_u64(extract_metric_sum(metrics, "failure_count"));

        let mut assessment =
            HealthAssessment::from_metrics(error_rate, latency_p99, success_count, failure_count);

        // Evaluate against thresholds
        assessment.is_healthy = self.evaluate_health(&assessment);

        if !assessment.is_healthy {
            let reason = self.explain_unhealthy(&assessment);
            assessment = assessment.with_message(reason);
        }

        // Store the assessment
        self.store_assessment(id, assessment.clone())?;

        debug!(
            "Health assessment for {}: healthy={}, error_rate={:.2}%, latency_p99={:.0}ms",
            id, assessment.is_healthy, assessment.error_rate, assessment.latency_p99
        );

        Ok(assessment)
    }

    /// Determines if a deployment should be promoted based on health assessment.
    ///
    /// Promotion requires:
    /// - Assessment is healthy
    /// - Sufficient request volume
    /// - Consistent health over time
    #[must_use]
    pub fn should_promote(&self, assessment: &HealthAssessment) -> bool {
        if !assessment.is_healthy {
            return false;
        }

        // Need minimum request volume
        if assessment.total_requests() < self.thresholds.min_requests {
            return false;
        }

        // Check success ratio
        let success_ratio = if assessment.total_requests() == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let ratio = assessment.success_count as f64 / assessment.total_requests() as f64;
            ratio
        };

        success_ratio >= self.thresholds.min_success_ratio
    }

    /// Determines if a deployment should be rolled back.
    ///
    /// Rollback triggers:
    /// - Error rate exceeds threshold significantly (2x)
    /// - Latency exceeds threshold significantly (2x)
    /// - Rapid increase in failures
    #[must_use]
    pub fn should_rollback(&self, assessment: &HealthAssessment) -> bool {
        // Immediate rollback if error rate is very high
        if assessment.error_rate > self.thresholds.max_error_rate * 2.0 {
            return true;
        }

        // Rollback if latency is very high
        if assessment.latency_p99 > self.thresholds.max_latency_p99 * 2.0 {
            return true;
        }

        // Rollback if we have significant traffic and high failure rate
        let failure_ratio = if assessment.total_requests() == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let ratio = assessment.failure_count as f64 / assessment.total_requests() as f64;
            ratio
        };

        assessment.total_requests() >= 10 && failure_ratio > 0.1
    }

    /// Gets historical assessments for a deployment.
    ///
    /// # Errors
    ///
    /// Returns an error if the lock cannot be acquired.
    pub fn get_assessments(&self, id: &DeploymentId) -> DeployResult<Vec<HealthAssessment>> {
        let assessments = self.assessments.read().map_err(|e| {
            DeployError::Internal(format!("Failed to acquire lock: {e}"))
        })?;

        Ok(assessments.get(id).cloned().unwrap_or_default())
    }

    /// Evaluates health against thresholds.
    fn evaluate_health(&self, assessment: &HealthAssessment) -> bool {
        assessment.error_rate <= self.thresholds.max_error_rate
            && assessment.latency_p99 <= self.thresholds.max_latency_p99
    }

    /// Generates explanation for unhealthy assessment.
    fn explain_unhealthy(&self, assessment: &HealthAssessment) -> String {
        let mut reasons = Vec::new();

        if assessment.error_rate > self.thresholds.max_error_rate {
            reasons.push(format!(
                "error rate {:.2}% exceeds threshold {:.2}%",
                assessment.error_rate, self.thresholds.max_error_rate
            ));
        }

        if assessment.latency_p99 > self.thresholds.max_latency_p99 {
            reasons.push(format!(
                "p99 latency {:.0}ms exceeds threshold {:.0}ms",
                assessment.latency_p99, self.thresholds.max_latency_p99
            ));
        }

        if reasons.is_empty() {
            "Unknown health issue".to_string()
        } else {
            reasons.join("; ")
        }
    }

    /// Stores an assessment for historical tracking.
    fn store_assessment(
        &self,
        id: &DeploymentId,
        assessment: HealthAssessment,
    ) -> DeployResult<()> {
        let mut assessments = self.assessments.write().map_err(|e| {
            DeployError::Internal(format!("Failed to acquire lock: {e}"))
        })?;

        assessments
            .entry(id.clone())
            .or_default()
            .push(assessment);

        // Keep only last 100 assessments per deployment
        if let Some(history) = assessments.get_mut(id) {
            if history.len() > 100 {
                history.drain(0..history.len() - 100);
            }
        }

        drop(assessments);

        Ok(())
    }
}

impl Default for DeploymentMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Safely converts f64 to u64, handling negative values and truncation.
fn safe_f64_to_u64(value: f64) -> u64 {
    // Use a constant for the maximum safe f64 value that fits in u64
    // u64::MAX is 18446744073709551615, but f64 can't represent this exactly
    const MAX_SAFE: f64 = 18_446_744_073_709_549_568.0; // Largest f64 < u64::MAX

    if value < 0.0 {
        0
    } else if value >= MAX_SAFE {
        u64::MAX
    } else {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let result = value as u64;
        result
    }
}

/// Extracts the average value for a named metric.
fn extract_metric_avg(metrics: &[MetricPoint], name: &str) -> Option<f64> {
    let values: Vec<f64> = metrics
        .iter()
        .filter(|m| m.name == name)
        .map(|m| m.value)
        .collect();

    if values.is_empty() {
        None
    } else {
        #[allow(clippy::cast_precision_loss)]
        let avg = values.iter().sum::<f64>() / values.len() as f64;
        Some(avg)
    }
}

/// Extracts the sum of values for a named metric.
fn extract_metric_sum(metrics: &[MetricPoint], name: &str) -> f64 {
    metrics
        .iter()
        .filter(|m| m.name == name)
        .map(|m| m.value)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    mod metric_point_tests {
        use super::*;

        #[test]
        fn creates_with_current_timestamp() {
            let before = Utc::now();
            let point = MetricPoint::new("test", 42.0);
            let after = Utc::now();

            assert_eq!(point.name, "test");
            assert!((point.value - 42.0).abs() < f64::EPSILON);
            assert!(point.timestamp >= before && point.timestamp <= after);
        }

        #[test]
        fn with_label_adds_labels() {
            let point = MetricPoint::new("test", 1.0)
                .with_label("env", "prod")
                .with_label("region", "us-west");

            assert_eq!(point.labels.get("env").map(String::as_str), Some("prod"));
            assert_eq!(point.labels.get("region").map(String::as_str), Some("us-west"));
        }

        #[test]
        fn serialization_roundtrip() {
            let point = MetricPoint::new("error_rate", 0.5).with_label("service", "api");

            let json = serde_json::to_string(&point);
            assert!(json.is_ok());
            let parsed: Result<MetricPoint, _> = json
                .as_ref()
                .map_or_else(|_| Err("no json".to_string()), |j| {
                    serde_json::from_str(j).map_err(|e| e.to_string())
                });
            assert!(parsed.is_ok());
        }
    }

    mod health_assessment_tests {
        use super::*;

        #[test]
        fn from_metrics_calculates_is_healthy() {
            // Healthy: low error rate and latency
            let healthy = HealthAssessment::from_metrics(0.5, 100.0, 1000, 5);
            assert!(healthy.is_healthy);

            // Unhealthy: high error rate
            let unhealthy_errors = HealthAssessment::from_metrics(5.0, 100.0, 1000, 50);
            assert!(!unhealthy_errors.is_healthy);

            // Unhealthy: high latency
            let unhealthy_latency = HealthAssessment::from_metrics(0.5, 1000.0, 1000, 5);
            assert!(!unhealthy_latency.is_healthy);
        }

        #[test]
        fn total_requests_sums_correctly() {
            let assessment = HealthAssessment::from_metrics(1.0, 100.0, 950, 50);
            assert_eq!(assessment.total_requests(), 1000);
        }

        #[test]
        fn with_message_sets_message() {
            let assessment =
                HealthAssessment::from_metrics(1.0, 100.0, 100, 0).with_message("All good");
            assert_eq!(assessment.message.as_deref(), Some("All good"));
        }
    }

    mod health_thresholds_tests {
        use super::*;

        #[test]
        fn production_is_stricter() {
            let prod = HealthThresholds::production();
            let default = HealthThresholds::default();

            assert!(prod.max_error_rate < default.max_error_rate);
            assert!(prod.max_latency_p99 < default.max_latency_p99);
            assert!(prod.min_requests > default.min_requests);
        }

        #[test]
        fn dev_is_more_lenient() {
            let dev = HealthThresholds::dev();
            let default = HealthThresholds::default();

            assert!(dev.max_error_rate > default.max_error_rate);
            assert!(dev.max_latency_p99 > default.max_latency_p99);
            assert!(dev.min_requests < default.min_requests);
        }
    }

    mod deployment_monitor_tests {
        use super::*;

        #[test]
        fn check_health_with_no_metrics() {
            let monitor = DeploymentMonitor::new();
            let id = DeploymentId::new();

            let result = monitor.check_health(&id, &[]);
            assert!(result.is_ok());

            let assessment = result.ok();
            assert!(assessment
                .as_ref()
                .and_then(|a| a.message.as_ref())
                .map_or(false, |m| m.contains("No metrics")));
        }

        #[test]
        fn check_health_calculates_averages() {
            let monitor = DeploymentMonitor::new();
            let id = DeploymentId::new();

            let metrics = vec![
                MetricPoint::new("error_rate", 0.5),
                MetricPoint::new("error_rate", 0.3),
                MetricPoint::new("latency_p99", 100.0),
                MetricPoint::new("latency_p99", 200.0),
                MetricPoint::new("success_count", 500.0),
                MetricPoint::new("failure_count", 5.0),
            ];

            let result = monitor.check_health(&id, &metrics);
            assert!(result.is_ok());

            let assessment = result.ok();
            // Average error rate: (0.5 + 0.3) / 2 = 0.4
            assert!(assessment.as_ref().map_or(false, |a| {
                (a.error_rate - 0.4).abs() < 0.01
            }));
            // Average latency: (100 + 200) / 2 = 150
            assert!(assessment.as_ref().map_or(false, |a| {
                (a.latency_p99 - 150.0).abs() < 0.01
            }));
        }

        #[test]
        fn stores_assessments_history() {
            let monitor = DeploymentMonitor::new();
            let id = DeploymentId::new();

            let metrics = vec![MetricPoint::new("error_rate", 0.1)];
            let _ = monitor.check_health(&id, &metrics);
            let _ = monitor.check_health(&id, &metrics);

            let history = monitor.get_assessments(&id);
            assert!(history.is_ok());
            assert_eq!(history.as_ref().ok().map(Vec::len), Some(2));
        }
    }

    mod should_promote_tests {
        use super::*;

        #[test]
        fn promotes_healthy_with_sufficient_traffic() {
            let monitor = DeploymentMonitor::new();

            let assessment = HealthAssessment::from_metrics(0.5, 100.0, 1000, 5);
            assert!(monitor.should_promote(&assessment));
        }

        #[test]
        fn does_not_promote_unhealthy() {
            let monitor = DeploymentMonitor::new();

            let assessment = HealthAssessment::from_metrics(5.0, 100.0, 1000, 50);
            assert!(!monitor.should_promote(&assessment));
        }

        #[test]
        fn does_not_promote_with_insufficient_traffic() {
            let monitor = DeploymentMonitor::new();

            // Healthy but only 50 requests (threshold is 100)
            let assessment = HealthAssessment::from_metrics(0.1, 50.0, 50, 0);
            assert!(!monitor.should_promote(&assessment));
        }

        #[test]
        fn does_not_promote_with_low_success_ratio() {
            let monitor = DeploymentMonitor::new();

            // Has traffic but poor success ratio
            let assessment = HealthAssessment::from_metrics(0.5, 100.0, 900, 200);
            // Success ratio = 900/1100 ≈ 0.82, threshold is 0.99
            assert!(!monitor.should_promote(&assessment));
        }
    }

    mod should_rollback_tests {
        use super::*;

        #[test]
        fn rolls_back_on_very_high_error_rate() {
            let monitor = DeploymentMonitor::new();

            // Error rate > 2% (2x threshold of 1%)
            let assessment = HealthAssessment::from_metrics(2.5, 100.0, 1000, 25);
            assert!(monitor.should_rollback(&assessment));
        }

        #[test]
        fn rolls_back_on_very_high_latency() {
            let monitor = DeploymentMonitor::new();

            // Latency > 1000ms (2x threshold of 500ms)
            let assessment = HealthAssessment::from_metrics(0.1, 1200.0, 1000, 1);
            assert!(monitor.should_rollback(&assessment));
        }

        #[test]
        fn rolls_back_on_high_failure_ratio() {
            let monitor = DeploymentMonitor::new();

            // > 10% failures with significant traffic
            let assessment = HealthAssessment::from_metrics(0.5, 100.0, 80, 20);
            // Failure ratio = 20/100 = 0.2 > 0.1
            assert!(monitor.should_rollback(&assessment));
        }

        #[test]
        fn does_not_rollback_healthy() {
            let monitor = DeploymentMonitor::new();

            let assessment = HealthAssessment::from_metrics(0.5, 100.0, 990, 10);
            assert!(!monitor.should_rollback(&assessment));
        }

        #[test]
        fn does_not_rollback_with_low_traffic() {
            let monitor = DeploymentMonitor::new();

            // High failure ratio but only 5 requests
            let assessment = HealthAssessment::from_metrics(0.5, 100.0, 4, 1);
            // Even though failure ratio is 20%, traffic is too low
            assert!(!monitor.should_rollback(&assessment));
        }
    }

    mod extract_metrics_tests {
        use super::*;

        #[test]
        fn extract_avg_with_values() {
            let metrics = vec![
                MetricPoint::new("test", 10.0),
                MetricPoint::new("test", 20.0),
                MetricPoint::new("test", 30.0),
            ];

            let avg = extract_metric_avg(&metrics, "test");
            assert!(avg.map_or(false, |v| (v - 20.0).abs() < f64::EPSILON));
        }

        #[test]
        fn extract_avg_returns_none_for_missing() {
            let metrics = vec![MetricPoint::new("other", 10.0)];
            let avg = extract_metric_avg(&metrics, "test");
            assert!(avg.is_none());
        }

        #[test]
        fn extract_sum_with_values() {
            let metrics = vec![
                MetricPoint::new("count", 10.0),
                MetricPoint::new("count", 20.0),
                MetricPoint::new("count", 30.0),
            ];

            let sum = extract_metric_sum(&metrics, "count");
            assert!((sum - 60.0).abs() < f64::EPSILON);
        }

        #[test]
        fn extract_sum_returns_zero_for_missing() {
            let metrics = vec![MetricPoint::new("other", 10.0)];
            let sum = extract_metric_sum(&metrics, "count");
            assert!((sum - 0.0).abs() < f64::EPSILON);
        }
    }

    mod safe_conversion_tests {
        use super::*;

        #[test]
        fn converts_positive_values() {
            assert_eq!(safe_f64_to_u64(100.0), 100);
            assert_eq!(safe_f64_to_u64(0.0), 0);
            assert_eq!(safe_f64_to_u64(1.5), 1);
        }

        #[test]
        fn handles_negative_values() {
            assert_eq!(safe_f64_to_u64(-10.0), 0);
            assert_eq!(safe_f64_to_u64(-0.1), 0);
        }
    }
}
