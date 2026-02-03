//! Automatic rollback trigger evaluation.
//!
//! This module provides functionality to evaluate conditions that should
//! trigger automatic rollbacks based on metrics and health checks.

use crate::types::{Metrics, RollbackTrigger};
use serde::{Deserialize, Serialize};

/// Configuration for automatic rollback triggers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerConfig {
    /// Error rate threshold (percentage, 0.0 - 100.0).
    pub error_rate_threshold: Option<f64>,
    /// P99 latency threshold in milliseconds.
    pub latency_threshold_ms: Option<f64>,
    /// Baseline latency to compare against (for 2x threshold).
    pub baseline_latency_ms: Option<f64>,
    /// Number of consecutive health check failures before triggering.
    pub health_check_failure_threshold: Option<u32>,
    /// Custom trigger conditions.
    pub custom_triggers: Vec<CustomTriggerConfig>,
}

impl Default for TriggerConfig {
    fn default() -> Self {
        Self {
            error_rate_threshold: Some(5.0),          // 5% error rate
            latency_threshold_ms: None,               // Use 2x baseline by default
            baseline_latency_ms: Some(100.0),         // 100ms baseline
            health_check_failure_threshold: Some(3), // 3 consecutive failures
            custom_triggers: Vec::new(),
        }
    }
}

impl TriggerConfig {
    /// Creates a new trigger configuration with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the error rate threshold.
    #[must_use]
    pub fn with_error_rate_threshold(mut self, threshold: f64) -> Self {
        self.error_rate_threshold = Some(threshold);
        self
    }

    /// Sets the latency threshold.
    #[must_use]
    pub fn with_latency_threshold_ms(mut self, threshold: f64) -> Self {
        self.latency_threshold_ms = Some(threshold);
        self
    }

    /// Sets the baseline latency for 2x threshold calculations.
    #[must_use]
    pub fn with_baseline_latency_ms(mut self, baseline: f64) -> Self {
        self.baseline_latency_ms = Some(baseline);
        self
    }

    /// Sets the health check failure threshold.
    #[must_use]
    pub fn with_health_check_failure_threshold(mut self, threshold: u32) -> Self {
        self.health_check_failure_threshold = Some(threshold);
        self
    }

    /// Adds a custom trigger.
    #[must_use]
    pub fn with_custom_trigger(mut self, trigger: CustomTriggerConfig) -> Self {
        self.custom_triggers.push(trigger);
        self
    }

    /// Disables the error rate trigger.
    #[must_use]
    pub fn without_error_rate_trigger(mut self) -> Self {
        self.error_rate_threshold = None;
        self
    }

    /// Disables the latency trigger.
    #[must_use]
    pub fn without_latency_trigger(mut self) -> Self {
        self.latency_threshold_ms = None;
        self.baseline_latency_ms = None;
        self
    }

    /// Disables the health check trigger.
    #[must_use]
    pub fn without_health_check_trigger(mut self) -> Self {
        self.health_check_failure_threshold = None;
        self
    }
}

/// Configuration for a custom trigger based on a metric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomTriggerConfig {
    /// Name of the custom metric to evaluate.
    pub metric_name: String,
    /// Threshold value.
    pub threshold: f64,
    /// Comparison operator.
    pub comparison: ComparisonOperator,
    /// Human-readable reason for this trigger.
    pub reason: String,
}

impl CustomTriggerConfig {
    /// Creates a new custom trigger configuration.
    #[must_use]
    pub fn new(
        metric_name: impl Into<String>,
        comparison: ComparisonOperator,
        threshold: f64,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            metric_name: metric_name.into(),
            threshold,
            comparison,
            reason: reason.into(),
        }
    }
}

/// Comparison operators for trigger evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComparisonOperator {
    /// Greater than.
    GreaterThan,
    /// Greater than or equal.
    GreaterThanOrEqual,
    /// Less than.
    LessThan,
    /// Less than or equal.
    LessThanOrEqual,
    /// Equal (within epsilon for floats).
    Equal,
}

impl ComparisonOperator {
    /// Evaluates the comparison.
    #[must_use]
    pub fn evaluate(self, value: f64, threshold: f64) -> bool {
        const EPSILON: f64 = 1e-10;
        match self {
            Self::GreaterThan => value > threshold,
            Self::GreaterThanOrEqual => value >= threshold - EPSILON,
            Self::LessThan => value < threshold,
            Self::LessThanOrEqual => value <= threshold + EPSILON,
            Self::Equal => (value - threshold).abs() < EPSILON,
        }
    }
}

/// Evaluates trigger conditions against current metrics.
#[derive(Debug, Clone)]
pub struct TriggerEvaluator {
    /// Baseline metrics for comparison.
    baseline: Option<Metrics>,
}

impl Default for TriggerEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerEvaluator {
    /// Creates a new trigger evaluator.
    #[must_use]
    pub fn new() -> Self {
        Self { baseline: None }
    }

    /// Creates an evaluator with baseline metrics for comparison.
    #[must_use]
    pub fn with_baseline(baseline: Metrics) -> Self {
        Self {
            baseline: Some(baseline),
        }
    }

    /// Sets the baseline metrics.
    pub fn set_baseline(&mut self, baseline: Metrics) {
        self.baseline = Some(baseline);
    }

    /// Returns the current baseline, if set.
    #[must_use]
    pub fn baseline(&self) -> Option<&Metrics> {
        self.baseline.as_ref()
    }

    /// Evaluates a single trigger against the current metrics.
    ///
    /// Returns `true` if the trigger condition is met (rollback should occur).
    #[must_use]
    pub fn evaluate(&self, trigger: &RollbackTrigger, current_metrics: &Metrics) -> bool {
        match trigger {
            RollbackTrigger::Manual => true,
            RollbackTrigger::ErrorRate { threshold } => current_metrics.error_rate > *threshold,
            RollbackTrigger::Latency { threshold_ms } => {
                current_metrics.p99_latency_ms > *threshold_ms
            }
            RollbackTrigger::HealthCheck { failure_threshold } => {
                current_metrics.health_check_failures >= *failure_threshold
            }
            RollbackTrigger::Custom { reason } => {
                // Custom triggers need explicit evaluation via custom metrics
                // Check if there's a matching custom metric
                current_metrics
                    .custom
                    .get(reason)
                    .map_or(false, |v| *v > 0.0)
            }
        }
    }

    /// Evaluates all provided triggers, returning the first one that fires.
    #[must_use]
    pub fn evaluate_all<'a>(
        &self,
        triggers: &'a [RollbackTrigger],
        current_metrics: &Metrics,
    ) -> Option<&'a RollbackTrigger> {
        triggers.iter().find(|t| self.evaluate(t, current_metrics))
    }

    /// Evaluates a custom trigger configuration.
    #[must_use]
    pub fn evaluate_custom(
        &self,
        config: &CustomTriggerConfig,
        current_metrics: &Metrics,
    ) -> bool {
        current_metrics
            .custom
            .get(&config.metric_name)
            .map_or(false, |value| {
                config.comparison.evaluate(*value, config.threshold)
            })
    }
}

/// Generates triggers from a configuration.
///
/// # Arguments
///
/// * `config` - The trigger configuration.
///
/// # Returns
///
/// A vector of `RollbackTrigger` instances based on the configuration.
#[must_use]
pub fn configure_triggers(config: &TriggerConfig) -> Vec<RollbackTrigger> {
    let mut triggers = Vec::new();

    if let Some(threshold) = config.error_rate_threshold {
        triggers.push(RollbackTrigger::error_rate(threshold));
    }

    // If explicit latency threshold is set, use it
    // Otherwise, if baseline is set, use 2x baseline
    if let Some(threshold) = config.latency_threshold_ms {
        triggers.push(RollbackTrigger::latency(threshold));
    } else if let Some(baseline) = config.baseline_latency_ms {
        triggers.push(RollbackTrigger::latency(baseline * 2.0));
    }

    if let Some(threshold) = config.health_check_failure_threshold {
        triggers.push(RollbackTrigger::health_check(threshold));
    }

    for custom in &config.custom_triggers {
        triggers.push(RollbackTrigger::custom(&custom.reason));
    }

    triggers
}

/// Provides default trigger configurations.
pub struct DefaultTriggers;

impl DefaultTriggers {
    /// Returns the default error rate trigger (> 5%).
    #[must_use]
    pub fn error_rate() -> RollbackTrigger {
        RollbackTrigger::error_rate(5.0)
    }

    /// Returns a latency trigger at 2x the provided baseline.
    #[must_use]
    pub fn latency_2x_baseline(baseline_ms: f64) -> RollbackTrigger {
        RollbackTrigger::latency(baseline_ms * 2.0)
    }

    /// Returns the default health check trigger (3 consecutive failures).
    #[must_use]
    pub fn health_check() -> RollbackTrigger {
        RollbackTrigger::health_check(3)
    }

    /// Returns all default triggers with the specified baseline latency.
    #[must_use]
    pub fn all(baseline_latency_ms: f64) -> Vec<RollbackTrigger> {
        vec![
            Self::error_rate(),
            Self::latency_2x_baseline(baseline_latency_ms),
            Self::health_check(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod comparison_operator_tests {
        use super::*;

        #[test]
        fn greater_than_evaluates_correctly() {
            assert!(ComparisonOperator::GreaterThan.evaluate(10.0, 5.0));
            assert!(!ComparisonOperator::GreaterThan.evaluate(5.0, 10.0));
            assert!(!ComparisonOperator::GreaterThan.evaluate(5.0, 5.0));
        }

        #[test]
        fn greater_than_or_equal_evaluates_correctly() {
            assert!(ComparisonOperator::GreaterThanOrEqual.evaluate(10.0, 5.0));
            assert!(!ComparisonOperator::GreaterThanOrEqual.evaluate(5.0, 10.0));
            assert!(ComparisonOperator::GreaterThanOrEqual.evaluate(5.0, 5.0));
        }

        #[test]
        fn less_than_evaluates_correctly() {
            assert!(!ComparisonOperator::LessThan.evaluate(10.0, 5.0));
            assert!(ComparisonOperator::LessThan.evaluate(5.0, 10.0));
            assert!(!ComparisonOperator::LessThan.evaluate(5.0, 5.0));
        }

        #[test]
        fn less_than_or_equal_evaluates_correctly() {
            assert!(!ComparisonOperator::LessThanOrEqual.evaluate(10.0, 5.0));
            assert!(ComparisonOperator::LessThanOrEqual.evaluate(5.0, 10.0));
            assert!(ComparisonOperator::LessThanOrEqual.evaluate(5.0, 5.0));
        }

        #[test]
        fn equal_evaluates_correctly() {
            assert!(ComparisonOperator::Equal.evaluate(5.0, 5.0));
            assert!(!ComparisonOperator::Equal.evaluate(5.1, 5.0));
            assert!(!ComparisonOperator::Equal.evaluate(4.9, 5.0));
        }
    }

    mod trigger_config_tests {
        use super::*;

        #[test]
        fn default_config_has_expected_values() {
            let config = TriggerConfig::default();
            assert_eq!(config.error_rate_threshold, Some(5.0));
            assert!(config.latency_threshold_ms.is_none());
            assert_eq!(config.baseline_latency_ms, Some(100.0));
            assert_eq!(config.health_check_failure_threshold, Some(3));
            assert!(config.custom_triggers.is_empty());
        }

        #[test]
        fn builder_pattern_works() {
            let config = TriggerConfig::new()
                .with_error_rate_threshold(10.0)
                .with_latency_threshold_ms(500.0)
                .with_health_check_failure_threshold(5);

            assert_eq!(config.error_rate_threshold, Some(10.0));
            assert_eq!(config.latency_threshold_ms, Some(500.0));
            assert_eq!(config.health_check_failure_threshold, Some(5));
        }

        #[test]
        fn can_disable_triggers() {
            let config = TriggerConfig::new()
                .without_error_rate_trigger()
                .without_latency_trigger()
                .without_health_check_trigger();

            assert!(config.error_rate_threshold.is_none());
            assert!(config.latency_threshold_ms.is_none());
            assert!(config.baseline_latency_ms.is_none());
            assert!(config.health_check_failure_threshold.is_none());
        }

        #[test]
        fn can_add_custom_triggers() {
            let custom = CustomTriggerConfig::new(
                "queue_depth",
                ComparisonOperator::GreaterThan,
                1000.0,
                "Queue too deep",
            );
            let config = TriggerConfig::new().with_custom_trigger(custom);

            assert_eq!(config.custom_triggers.len(), 1);
            assert_eq!(config.custom_triggers[0].metric_name, "queue_depth");
        }
    }

    mod trigger_evaluator_tests {
        use super::*;

        #[test]
        fn new_creates_evaluator_without_baseline() {
            let evaluator = TriggerEvaluator::new();
            assert!(evaluator.baseline().is_none());
        }

        #[test]
        fn with_baseline_sets_baseline() {
            let baseline = Metrics::new().with_p99_latency_ms(100.0);
            let evaluator = TriggerEvaluator::with_baseline(baseline.clone());
            assert!(evaluator.baseline().is_some());
            assert!((evaluator.baseline().unwrap_or(&Metrics::new()).p99_latency_ms - 100.0).abs() < f64::EPSILON);
        }

        #[test]
        fn set_baseline_updates_baseline() {
            let mut evaluator = TriggerEvaluator::new();
            let baseline = Metrics::new().with_p99_latency_ms(50.0);
            evaluator.set_baseline(baseline);
            assert!(evaluator.baseline().is_some());
        }

        #[test]
        fn evaluate_manual_always_returns_true() {
            let evaluator = TriggerEvaluator::new();
            let metrics = Metrics::new();
            assert!(evaluator.evaluate(&RollbackTrigger::Manual, &metrics));
        }

        #[test]
        fn evaluate_error_rate_trigger() {
            let evaluator = TriggerEvaluator::new();
            let trigger = RollbackTrigger::error_rate(5.0);

            let healthy = Metrics::new().with_error_rate(2.0);
            let unhealthy = Metrics::new().with_error_rate(10.0);
            let boundary = Metrics::new().with_error_rate(5.0);

            assert!(!evaluator.evaluate(&trigger, &healthy));
            assert!(evaluator.evaluate(&trigger, &unhealthy));
            assert!(!evaluator.evaluate(&trigger, &boundary)); // Not greater than
        }

        #[test]
        fn evaluate_latency_trigger() {
            let evaluator = TriggerEvaluator::new();
            let trigger = RollbackTrigger::latency(200.0);

            let healthy = Metrics::new().with_p99_latency_ms(100.0);
            let unhealthy = Metrics::new().with_p99_latency_ms(300.0);

            assert!(!evaluator.evaluate(&trigger, &healthy));
            assert!(evaluator.evaluate(&trigger, &unhealthy));
        }

        #[test]
        fn evaluate_health_check_trigger() {
            let evaluator = TriggerEvaluator::new();
            let trigger = RollbackTrigger::health_check(3);

            let healthy = Metrics::new().with_health_check_failures(1);
            let unhealthy = Metrics::new().with_health_check_failures(3);
            let very_unhealthy = Metrics::new().with_health_check_failures(5);

            assert!(!evaluator.evaluate(&trigger, &healthy));
            assert!(evaluator.evaluate(&trigger, &unhealthy)); // Equal to threshold
            assert!(evaluator.evaluate(&trigger, &very_unhealthy));
        }

        #[test]
        fn evaluate_custom_trigger_with_metric() {
            let evaluator = TriggerEvaluator::new();
            let trigger = RollbackTrigger::custom("memory_pressure");

            let no_pressure = Metrics::new();
            let with_pressure = Metrics::new().with_custom("memory_pressure", 1.0);

            assert!(!evaluator.evaluate(&trigger, &no_pressure));
            assert!(evaluator.evaluate(&trigger, &with_pressure));
        }

        #[test]
        fn evaluate_all_returns_first_triggered() {
            let evaluator = TriggerEvaluator::new();
            let triggers = vec![
                RollbackTrigger::error_rate(5.0),
                RollbackTrigger::latency(200.0),
                RollbackTrigger::health_check(3),
            ];

            // Only latency trigger should fire
            let metrics = Metrics::new()
                .with_error_rate(2.0)
                .with_p99_latency_ms(300.0)
                .with_health_check_failures(1);

            let triggered = evaluator.evaluate_all(&triggers, &metrics);
            assert!(triggered.is_some());
            match triggered {
                Some(RollbackTrigger::Latency { .. }) => {}
                _ => panic!("Expected latency trigger"),
            }
        }

        #[test]
        fn evaluate_all_returns_none_when_healthy() {
            let evaluator = TriggerEvaluator::new();
            let triggers = vec![
                RollbackTrigger::error_rate(5.0),
                RollbackTrigger::latency(200.0),
                RollbackTrigger::health_check(3),
            ];

            let healthy_metrics = Metrics::new()
                .with_error_rate(1.0)
                .with_p99_latency_ms(50.0)
                .with_health_check_failures(0);

            let triggered = evaluator.evaluate_all(&triggers, &healthy_metrics);
            assert!(triggered.is_none());
        }

        #[test]
        fn evaluate_custom_config() {
            let evaluator = TriggerEvaluator::new();
            let config = CustomTriggerConfig::new(
                "queue_depth",
                ComparisonOperator::GreaterThan,
                100.0,
                "Queue too deep",
            );

            let normal = Metrics::new().with_custom("queue_depth", 50.0);
            let overloaded = Metrics::new().with_custom("queue_depth", 200.0);

            assert!(!evaluator.evaluate_custom(&config, &normal));
            assert!(evaluator.evaluate_custom(&config, &overloaded));
        }

        #[test]
        fn evaluate_custom_config_missing_metric() {
            let evaluator = TriggerEvaluator::new();
            let config = CustomTriggerConfig::new(
                "nonexistent_metric",
                ComparisonOperator::GreaterThan,
                100.0,
                "Test",
            );

            let metrics = Metrics::new();
            assert!(!evaluator.evaluate_custom(&config, &metrics));
        }
    }

    mod configure_triggers_tests {
        use super::*;

        #[test]
        fn default_config_creates_three_triggers() {
            let config = TriggerConfig::default();
            let triggers = configure_triggers(&config);

            assert_eq!(triggers.len(), 3);
        }

        #[test]
        fn creates_error_rate_trigger_from_config() {
            let config = TriggerConfig::new()
                .with_error_rate_threshold(10.0)
                .without_latency_trigger()
                .without_health_check_trigger();

            let triggers = configure_triggers(&config);
            assert_eq!(triggers.len(), 1);
            match &triggers[0] {
                RollbackTrigger::ErrorRate { threshold } => {
                    assert!((*threshold - 10.0).abs() < f64::EPSILON);
                }
                _ => panic!("Expected error rate trigger"),
            }
        }

        #[test]
        fn creates_latency_trigger_from_explicit_threshold() {
            let config = TriggerConfig::new()
                .without_error_rate_trigger()
                .with_latency_threshold_ms(500.0)
                .without_health_check_trigger();

            let triggers = configure_triggers(&config);
            assert_eq!(triggers.len(), 1);
            match &triggers[0] {
                RollbackTrigger::Latency { threshold_ms } => {
                    assert!((*threshold_ms - 500.0).abs() < f64::EPSILON);
                }
                _ => panic!("Expected latency trigger"),
            }
        }

        #[test]
        fn creates_latency_trigger_from_2x_baseline() {
            let config = TriggerConfig::new()
                .without_error_rate_trigger()
                .with_baseline_latency_ms(100.0)
                .without_health_check_trigger();

            let triggers = configure_triggers(&config);
            assert_eq!(triggers.len(), 1);
            match &triggers[0] {
                RollbackTrigger::Latency { threshold_ms } => {
                    assert!((*threshold_ms - 200.0).abs() < f64::EPSILON); // 2x baseline
                }
                _ => panic!("Expected latency trigger"),
            }
        }

        #[test]
        fn explicit_latency_overrides_baseline() {
            let config = TriggerConfig::new()
                .without_error_rate_trigger()
                .with_baseline_latency_ms(100.0)
                .with_latency_threshold_ms(300.0)
                .without_health_check_trigger();

            let triggers = configure_triggers(&config);
            assert_eq!(triggers.len(), 1);
            match &triggers[0] {
                RollbackTrigger::Latency { threshold_ms } => {
                    assert!((*threshold_ms - 300.0).abs() < f64::EPSILON);
                }
                _ => panic!("Expected latency trigger"),
            }
        }

        #[test]
        fn creates_health_check_trigger_from_config() {
            let config = TriggerConfig::new()
                .without_error_rate_trigger()
                .without_latency_trigger()
                .with_health_check_failure_threshold(5);

            let triggers = configure_triggers(&config);
            assert_eq!(triggers.len(), 1);
            match &triggers[0] {
                RollbackTrigger::HealthCheck { failure_threshold } => {
                    assert_eq!(*failure_threshold, 5);
                }
                _ => panic!("Expected health check trigger"),
            }
        }

        #[test]
        fn includes_custom_triggers() {
            let custom = CustomTriggerConfig::new(
                "test_metric",
                ComparisonOperator::GreaterThan,
                50.0,
                "Test trigger",
            );
            let config = TriggerConfig::new()
                .without_error_rate_trigger()
                .without_latency_trigger()
                .without_health_check_trigger()
                .with_custom_trigger(custom);

            let triggers = configure_triggers(&config);
            assert_eq!(triggers.len(), 1);
            match &triggers[0] {
                RollbackTrigger::Custom { reason } => {
                    assert_eq!(reason, "Test trigger");
                }
                _ => panic!("Expected custom trigger"),
            }
        }

        #[test]
        fn empty_config_creates_no_triggers() {
            let config = TriggerConfig::new()
                .without_error_rate_trigger()
                .without_latency_trigger()
                .without_health_check_trigger();

            let triggers = configure_triggers(&config);
            assert!(triggers.is_empty());
        }
    }

    mod default_triggers_tests {
        use super::*;

        #[test]
        fn error_rate_returns_5_percent() {
            match DefaultTriggers::error_rate() {
                RollbackTrigger::ErrorRate { threshold } => {
                    assert!((threshold - 5.0).abs() < f64::EPSILON);
                }
                _ => panic!("Expected error rate trigger"),
            }
        }

        #[test]
        fn latency_2x_baseline_doubles_value() {
            match DefaultTriggers::latency_2x_baseline(100.0) {
                RollbackTrigger::Latency { threshold_ms } => {
                    assert!((threshold_ms - 200.0).abs() < f64::EPSILON);
                }
                _ => panic!("Expected latency trigger"),
            }
        }

        #[test]
        fn health_check_returns_3_failures() {
            match DefaultTriggers::health_check() {
                RollbackTrigger::HealthCheck { failure_threshold } => {
                    assert_eq!(failure_threshold, 3);
                }
                _ => panic!("Expected health check trigger"),
            }
        }

        #[test]
        fn all_returns_three_triggers() {
            let triggers = DefaultTriggers::all(100.0);
            assert_eq!(triggers.len(), 3);
        }
    }

    mod serialization_tests {
        use super::*;

        #[test]
        fn trigger_config_serialization_roundtrip() {
            let config = TriggerConfig::new()
                .with_error_rate_threshold(7.5)
                .with_latency_threshold_ms(300.0)
                .with_custom_trigger(CustomTriggerConfig::new(
                    "test",
                    ComparisonOperator::LessThan,
                    10.0,
                    "Test reason",
                ));

            let json = serde_json::to_string(&config).unwrap_or_default();
            let deserialized: Result<TriggerConfig, _> = serde_json::from_str(&json);
            
            assert!(deserialized.is_ok());
            let deserialized = deserialized.unwrap_or_else(|_| panic!("should deserialize"));
            assert_eq!(deserialized.error_rate_threshold, Some(7.5));
            assert_eq!(deserialized.latency_threshold_ms, Some(300.0));
            assert_eq!(deserialized.custom_triggers.len(), 1);
        }
    }
}
