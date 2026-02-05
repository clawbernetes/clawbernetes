//! Core types for the alerting system.
//!
//! This module provides the fundamental types used throughout the claw-alerts crate:
//! - [`AlertSeverity`]: The severity level of an alert
//! - [`AlertState`]: The current state of an alert
//! - [`ComparisonOperator`]: Operators for comparing metric values
//! - [`AlertCondition`]: A condition that triggers an alert
//! - [`AlertRule`]: A rule that defines when and how to alert
//! - [`Alert`]: An active or resolved alert instance

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AlertError, Result};

/// The severity level of an alert.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    /// Informational alert, no action required.
    Info,
    /// Warning alert, should be investigated.
    #[default]
    Warning,
    /// Critical alert, requires immediate attention.
    Critical,
}

impl AlertSeverity {
    /// Returns the severity as a string.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }

    /// Returns the priority of this severity (higher = more urgent).
    #[must_use]
    pub const fn priority(&self) -> u8 {
        match self {
            Self::Info => 1,
            Self::Warning => 2,
            Self::Critical => 3,
        }
    }
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// The current state of an alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertState {
    /// The condition is true but hasn't been true long enough to fire.
    Pending,
    /// The alert is actively firing.
    Firing,
    /// The alert was firing but has been resolved.
    Resolved,
}

impl AlertState {
    /// Returns the state as a string.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Firing => "firing",
            Self::Resolved => "resolved",
        }
    }

    /// Returns true if the alert is currently active (pending or firing).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Pending | Self::Firing)
    }
}

impl std::fmt::Display for AlertState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Comparison operators for alert conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComparisonOperator {
    /// Greater than (>).
    #[serde(rename = ">")]
    GreaterThan,
    /// Greater than or equal (>=).
    #[serde(rename = ">=")]
    GreaterThanOrEqual,
    /// Less than (<).
    #[serde(rename = "<")]
    LessThan,
    /// Less than or equal (<=).
    #[serde(rename = "<=")]
    LessThanOrEqual,
    /// Equal (==).
    #[serde(rename = "==")]
    Equal,
    /// Not equal (!=).
    #[serde(rename = "!=")]
    NotEqual,
}

impl ComparisonOperator {
    /// Evaluates the comparison between two values.
    #[must_use]
    pub fn evaluate(&self, left: f64, right: f64) -> bool {
        match self {
            Self::GreaterThan => left > right,
            Self::GreaterThanOrEqual => left >= right,
            Self::LessThan => left < right,
            Self::LessThanOrEqual => left <= right,
            Self::Equal => (left - right).abs() < f64::EPSILON,
            Self::NotEqual => (left - right).abs() >= f64::EPSILON,
        }
    }

    /// Returns the operator as a string symbol.
    #[must_use]
    pub const fn as_symbol(&self) -> &'static str {
        match self {
            Self::GreaterThan => ">",
            Self::GreaterThanOrEqual => ">=",
            Self::LessThan => "<",
            Self::LessThanOrEqual => "<=",
            Self::Equal => "==",
            Self::NotEqual => "!=",
        }
    }
}

impl std::fmt::Display for ComparisonOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_symbol())
    }
}

/// A condition that triggers an alert based on a metric value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertCondition {
    /// The name of the metric to evaluate.
    pub metric_name: String,
    /// The comparison operator.
    pub operator: ComparisonOperator,
    /// The threshold value to compare against.
    pub threshold: f64,
    /// Optional label filters (metric must have matching labels).
    pub label_filters: HashMap<String, String>,
}

impl AlertCondition {
    /// Creates a new alert condition.
    ///
    /// # Errors
    ///
    /// Returns `AlertError::InvalidRule` if the metric name is empty.
    pub fn new(
        metric_name: impl Into<String>,
        operator: ComparisonOperator,
        threshold: f64,
    ) -> Result<Self> {
        let metric_name = metric_name.into();
        if metric_name.is_empty() {
            return Err(AlertError::InvalidRule {
                reason: "metric name cannot be empty".to_string(),
            });
        }

        Ok(Self {
            metric_name,
            operator,
            threshold,
            label_filters: HashMap::new(),
        })
    }

    /// Adds a label filter to this condition.
    #[must_use]
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.label_filters.insert(key.into(), value.into());
        self
    }

    /// Evaluates the condition against a metric value.
    #[must_use]
    pub fn evaluate(&self, value: f64) -> bool {
        self.operator.evaluate(value, self.threshold)
    }
}

impl std::fmt::Display for AlertCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} {}",
            self.metric_name, self.operator, self.threshold
        )
    }
}

/// A rule that defines when and how to trigger an alert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertRule {
    /// Unique identifier for the rule.
    pub id: String,
    /// Human-readable name for the rule.
    pub name: String,
    /// The condition that triggers this alert.
    pub condition: AlertCondition,
    /// How long the condition must be true before firing (in seconds).
    pub for_duration_secs: u64,
    /// The severity of alerts generated by this rule.
    pub severity: AlertSeverity,
    /// Labels to attach to alerts generated by this rule.
    pub labels: HashMap<String, String>,
    /// Annotations providing more context for the alert.
    pub annotations: HashMap<String, String>,
    /// Whether this rule is enabled.
    pub enabled: bool,
}

impl AlertRule {
    /// Maximum allowed length for rule names.
    pub const MAX_NAME_LENGTH: usize = 256;

    /// Creates a new alert rule builder.
    pub fn builder(name: impl Into<String>, condition: AlertCondition) -> AlertRuleBuilder {
        AlertRuleBuilder::new(name, condition)
    }

    /// Returns the `for_duration` as a [`Duration`].
    #[must_use]
    pub const fn for_duration(&self) -> Duration {
        Duration::from_secs(self.for_duration_secs)
    }
}

/// Builder for creating [`AlertRule`] instances.
#[derive(Debug)]
pub struct AlertRuleBuilder {
    name: String,
    condition: AlertCondition,
    for_duration_secs: u64,
    severity: AlertSeverity,
    labels: HashMap<String, String>,
    annotations: HashMap<String, String>,
    enabled: bool,
}

impl AlertRuleBuilder {
    /// Creates a new builder with required fields.
    fn new(name: impl Into<String>, condition: AlertCondition) -> Self {
        Self {
            name: name.into(),
            condition,
            for_duration_secs: 0,
            severity: AlertSeverity::Warning,
            labels: HashMap::new(),
            annotations: HashMap::new(),
            enabled: true,
        }
    }

    /// Sets the duration the condition must be true before firing.
    #[must_use]
    pub const fn for_duration(mut self, duration: Duration) -> Self {
        self.for_duration_secs = duration.as_secs();
        self
    }

    /// Sets the duration in seconds.
    #[must_use]
    pub const fn for_duration_secs(mut self, secs: u64) -> Self {
        self.for_duration_secs = secs;
        self
    }

    /// Sets the severity level.
    #[must_use]
    pub const fn severity(mut self, severity: AlertSeverity) -> Self {
        self.severity = severity;
        self
    }

    /// Adds a label to the rule.
    #[must_use]
    pub fn label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Adds multiple labels to the rule.
    #[must_use]
    pub fn labels(mut self, labels: HashMap<String, String>) -> Self {
        self.labels.extend(labels);
        self
    }

    /// Adds an annotation to the rule.
    #[must_use]
    pub fn annotation(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.annotations.insert(key.into(), value.into());
        self
    }

    /// Adds multiple annotations to the rule.
    #[must_use]
    pub fn annotations(mut self, annotations: HashMap<String, String>) -> Self {
        self.annotations.extend(annotations);
        self
    }

    /// Sets whether the rule is enabled.
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Builds the [`AlertRule`].
    ///
    /// # Errors
    ///
    /// Returns `AlertError::InvalidRule` if:
    /// - The name is empty
    /// - The name exceeds the maximum length
    pub fn build(self) -> Result<AlertRule> {
        if self.name.is_empty() {
            return Err(AlertError::InvalidRule {
                reason: "rule name cannot be empty".to_string(),
            });
        }

        if self.name.len() > AlertRule::MAX_NAME_LENGTH {
            return Err(AlertError::InvalidRule {
                reason: format!(
                    "rule name exceeds maximum length of {} characters",
                    AlertRule::MAX_NAME_LENGTH
                ),
            });
        }

        Ok(AlertRule {
            id: Uuid::new_v4().to_string(),
            name: self.name,
            condition: self.condition,
            for_duration_secs: self.for_duration_secs,
            severity: self.severity,
            labels: self.labels,
            annotations: self.annotations,
            enabled: self.enabled,
        })
    }
}

/// An active or resolved alert instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    /// Unique identifier for this alert instance.
    pub id: String,
    /// The ID of the rule that generated this alert.
    pub rule_id: String,
    /// The name of the rule that generated this alert.
    pub rule_name: String,
    /// The current state of the alert.
    pub state: AlertState,
    /// The severity of the alert.
    pub severity: AlertSeverity,
    /// The metric value that triggered the alert.
    pub value: f64,
    /// When the alert first started pending.
    pub started_at: DateTime<Utc>,
    /// When the alert started firing (None if still pending).
    pub fired_at: Option<DateTime<Utc>>,
    /// When the alert was resolved (None if still active).
    pub resolved_at: Option<DateTime<Utc>>,
    /// Labels attached to the alert.
    pub labels: HashMap<String, String>,
    /// Annotations providing more context.
    pub annotations: HashMap<String, String>,
    /// A fingerprint for deduplication (hash of rule + labels).
    pub fingerprint: String,
}

impl Alert {
    /// Creates a new pending alert from a rule.
    #[must_use]
    pub fn new_pending(rule: &AlertRule, value: f64, metric_labels: HashMap<String, String>) -> Self {
        let mut labels = rule.labels.clone();
        labels.extend(metric_labels);
        labels.insert("alertname".to_string(), rule.name.clone());
        labels.insert("severity".to_string(), rule.severity.as_str().to_string());

        let fingerprint = Self::compute_fingerprint(&rule.id, &labels);

        Self {
            id: Uuid::new_v4().to_string(),
            rule_id: rule.id.clone(),
            rule_name: rule.name.clone(),
            state: AlertState::Pending,
            severity: rule.severity,
            value,
            started_at: Utc::now(),
            fired_at: None,
            resolved_at: None,
            labels,
            annotations: rule.annotations.clone(),
            fingerprint,
        }
    }

    /// Transitions the alert to the firing state.
    pub fn fire(&mut self) {
        if self.state == AlertState::Pending {
            self.state = AlertState::Firing;
            self.fired_at = Some(Utc::now());
        }
    }

    /// Transitions the alert to the resolved state.
    pub fn resolve(&mut self) {
        if self.state != AlertState::Resolved {
            self.state = AlertState::Resolved;
            self.resolved_at = Some(Utc::now());
        }
    }

    /// Updates the metric value.
    pub fn update_value(&mut self, value: f64) {
        self.value = value;
    }

    /// Returns true if the alert is currently active (pending or firing).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.state.is_active()
    }

    /// Computes a fingerprint for deduplication.
    fn compute_fingerprint(rule_id: &str, labels: &HashMap<String, String>) -> String {
        use std::hash::{Hash, Hasher};

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        rule_id.hash(&mut hasher);

        // Sort labels for consistent hashing
        let mut sorted_labels: Vec<_> = labels.iter().collect();
        sorted_labels.sort_by_key(|(k, _)| *k);
        for (k, v) in sorted_labels {
            k.hash(&mut hasher);
            v.hash(&mut hasher);
        }

        format!("{:016x}", hasher.finish())
    }
}

/// A silence that suppresses alerts matching certain criteria.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Silence {
    /// Unique identifier for this silence.
    pub id: String,
    /// Label matchers (alerts matching all labels are silenced).
    pub matchers: HashMap<String, String>,
    /// When the silence starts.
    pub starts_at: DateTime<Utc>,
    /// When the silence ends.
    pub ends_at: DateTime<Utc>,
    /// Who created the silence.
    pub created_by: String,
    /// Comment explaining the silence.
    pub comment: String,
}

impl Silence {
    /// Creates a new silence.
    ///
    /// # Errors
    ///
    /// Returns `AlertError::InvalidDuration` if `ends_at` is before `starts_at`.
    pub fn new(
        matchers: HashMap<String, String>,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
        created_by: impl Into<String>,
        comment: impl Into<String>,
    ) -> Result<Self> {
        if ends_at <= starts_at {
            return Err(AlertError::InvalidDuration {
                reason: "silence end time must be after start time".to_string(),
            });
        }

        Ok(Self {
            id: Uuid::new_v4().to_string(),
            matchers,
            starts_at,
            ends_at,
            created_by: created_by.into(),
            comment: comment.into(),
        })
    }

    /// Checks if the silence is currently active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        let now = Utc::now();
        now >= self.starts_at && now < self.ends_at
    }

    /// Checks if an alert matches this silence.
    #[must_use]
    pub fn matches(&self, alert: &Alert) -> bool {
        if !self.is_active() {
            return false;
        }

        self.matchers
            .iter()
            .all(|(k, v)| alert.labels.get(k) == Some(v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod severity_tests {
        use super::*;

        #[test]
        fn severity_as_str() {
            assert_eq!(AlertSeverity::Info.as_str(), "info");
            assert_eq!(AlertSeverity::Warning.as_str(), "warning");
            assert_eq!(AlertSeverity::Critical.as_str(), "critical");
        }

        #[test]
        fn severity_priority() {
            assert!(AlertSeverity::Info.priority() < AlertSeverity::Warning.priority());
            assert!(AlertSeverity::Warning.priority() < AlertSeverity::Critical.priority());
        }

        #[test]
        fn severity_display() {
            assert_eq!(format!("{}", AlertSeverity::Info), "info");
            assert_eq!(format!("{}", AlertSeverity::Warning), "warning");
            assert_eq!(format!("{}", AlertSeverity::Critical), "critical");
        }

        #[test]
        fn severity_default() {
            assert_eq!(AlertSeverity::default(), AlertSeverity::Warning);
        }

        #[test]
        fn severity_serialization_roundtrip() {
            for sev in [
                AlertSeverity::Info,
                AlertSeverity::Warning,
                AlertSeverity::Critical,
            ] {
                let json = serde_json::to_string(&sev);
                assert!(json.is_ok());
                let parsed: serde_json::Result<AlertSeverity> =
                    serde_json::from_str(&json.unwrap());
                assert!(parsed.is_ok());
                assert_eq!(parsed.unwrap(), sev);
            }
        }
    }

    mod state_tests {
        use super::*;

        #[test]
        fn state_as_str() {
            assert_eq!(AlertState::Pending.as_str(), "pending");
            assert_eq!(AlertState::Firing.as_str(), "firing");
            assert_eq!(AlertState::Resolved.as_str(), "resolved");
        }

        #[test]
        fn state_is_active() {
            assert!(AlertState::Pending.is_active());
            assert!(AlertState::Firing.is_active());
            assert!(!AlertState::Resolved.is_active());
        }

        #[test]
        fn state_display() {
            assert_eq!(format!("{}", AlertState::Pending), "pending");
            assert_eq!(format!("{}", AlertState::Firing), "firing");
            assert_eq!(format!("{}", AlertState::Resolved), "resolved");
        }

        #[test]
        fn state_serialization_roundtrip() {
            for state in [AlertState::Pending, AlertState::Firing, AlertState::Resolved] {
                let json = serde_json::to_string(&state);
                assert!(json.is_ok());
                let parsed: serde_json::Result<AlertState> = serde_json::from_str(&json.unwrap());
                assert!(parsed.is_ok());
                assert_eq!(parsed.unwrap(), state);
            }
        }
    }

    mod operator_tests {
        use super::*;

        #[test]
        fn operator_greater_than() {
            let op = ComparisonOperator::GreaterThan;
            assert!(op.evaluate(10.0, 5.0));
            assert!(!op.evaluate(5.0, 10.0));
            assert!(!op.evaluate(5.0, 5.0));
        }

        #[test]
        fn operator_greater_than_or_equal() {
            let op = ComparisonOperator::GreaterThanOrEqual;
            assert!(op.evaluate(10.0, 5.0));
            assert!(!op.evaluate(5.0, 10.0));
            assert!(op.evaluate(5.0, 5.0));
        }

        #[test]
        fn operator_less_than() {
            let op = ComparisonOperator::LessThan;
            assert!(!op.evaluate(10.0, 5.0));
            assert!(op.evaluate(5.0, 10.0));
            assert!(!op.evaluate(5.0, 5.0));
        }

        #[test]
        fn operator_less_than_or_equal() {
            let op = ComparisonOperator::LessThanOrEqual;
            assert!(!op.evaluate(10.0, 5.0));
            assert!(op.evaluate(5.0, 10.0));
            assert!(op.evaluate(5.0, 5.0));
        }

        #[test]
        fn operator_equal() {
            let op = ComparisonOperator::Equal;
            assert!(!op.evaluate(10.0, 5.0));
            assert!(op.evaluate(5.0, 5.0));
            assert!(op.evaluate(0.0, 0.0));
        }

        #[test]
        fn operator_not_equal() {
            let op = ComparisonOperator::NotEqual;
            assert!(op.evaluate(10.0, 5.0));
            assert!(!op.evaluate(5.0, 5.0));
        }

        #[test]
        fn operator_as_symbol() {
            assert_eq!(ComparisonOperator::GreaterThan.as_symbol(), ">");
            assert_eq!(ComparisonOperator::GreaterThanOrEqual.as_symbol(), ">=");
            assert_eq!(ComparisonOperator::LessThan.as_symbol(), "<");
            assert_eq!(ComparisonOperator::LessThanOrEqual.as_symbol(), "<=");
            assert_eq!(ComparisonOperator::Equal.as_symbol(), "==");
            assert_eq!(ComparisonOperator::NotEqual.as_symbol(), "!=");
        }

        #[test]
        fn operator_display() {
            assert_eq!(format!("{}", ComparisonOperator::GreaterThan), ">");
            assert_eq!(format!("{}", ComparisonOperator::LessThan), "<");
        }

        #[test]
        fn operator_serialization_roundtrip() {
            for op in [
                ComparisonOperator::GreaterThan,
                ComparisonOperator::GreaterThanOrEqual,
                ComparisonOperator::LessThan,
                ComparisonOperator::LessThanOrEqual,
                ComparisonOperator::Equal,
                ComparisonOperator::NotEqual,
            ] {
                let json = serde_json::to_string(&op);
                assert!(json.is_ok());
                let parsed: serde_json::Result<ComparisonOperator> =
                    serde_json::from_str(&json.unwrap());
                assert!(parsed.is_ok());
                assert_eq!(parsed.unwrap(), op);
            }
        }
    }

    mod condition_tests {
        use super::*;

        #[test]
        fn create_condition() {
            let cond =
                AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0);
            assert!(cond.is_ok());
            let cond = cond.unwrap();
            assert_eq!(cond.metric_name, "cpu_usage");
            assert_eq!(cond.operator, ComparisonOperator::GreaterThan);
            assert!((cond.threshold - 80.0).abs() < f64::EPSILON);
        }

        #[test]
        fn condition_empty_name_fails() {
            let cond = AlertCondition::new("", ComparisonOperator::GreaterThan, 80.0);
            assert!(cond.is_err());
            match cond {
                Err(AlertError::InvalidRule { reason }) => {
                    assert!(reason.contains("empty"));
                }
                _ => panic!("expected InvalidRule error"),
            }
        }

        #[test]
        fn condition_with_label() {
            let cond = AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0)
                .unwrap()
                .with_label("node", "node-1")
                .with_label("env", "prod");

            assert_eq!(cond.label_filters.get("node"), Some(&"node-1".to_string()));
            assert_eq!(cond.label_filters.get("env"), Some(&"prod".to_string()));
        }

        #[test]
        fn condition_evaluate() {
            let cond =
                AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0).unwrap();

            assert!(cond.evaluate(90.0));
            assert!(!cond.evaluate(70.0));
            assert!(!cond.evaluate(80.0));
        }

        #[test]
        fn condition_display() {
            let cond =
                AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0).unwrap();
            assert_eq!(format!("{cond}"), "cpu_usage > 80");
        }

        #[test]
        fn condition_serialization_roundtrip() {
            let original = AlertCondition::new("memory_usage", ComparisonOperator::LessThan, 1024.0)
                .unwrap()
                .with_label("host", "server-1");

            let json = serde_json::to_string(&original);
            assert!(json.is_ok());

            let parsed: serde_json::Result<AlertCondition> = serde_json::from_str(&json.unwrap());
            assert!(parsed.is_ok());
            assert_eq!(parsed.unwrap(), original);
        }
    }

    mod rule_tests {
        use super::*;

        fn test_condition() -> AlertCondition {
            AlertCondition::new("gpu_temp", ComparisonOperator::GreaterThan, 85.0).unwrap()
        }

        #[test]
        fn create_rule_with_builder() {
            let rule = AlertRule::builder("HighGpuTemp", test_condition())
                .for_duration_secs(60)
                .severity(AlertSeverity::Critical)
                .label("team", "gpu-ops")
                .annotation("summary", "GPU temperature is too high")
                .build();

            assert!(rule.is_ok());
            let rule = rule.unwrap();
            assert_eq!(rule.name, "HighGpuTemp");
            assert_eq!(rule.for_duration_secs, 60);
            assert_eq!(rule.severity, AlertSeverity::Critical);
            assert_eq!(rule.labels.get("team"), Some(&"gpu-ops".to_string()));
            assert_eq!(
                rule.annotations.get("summary"),
                Some(&"GPU temperature is too high".to_string())
            );
            assert!(rule.enabled);
        }

        #[test]
        fn rule_empty_name_fails() {
            let rule = AlertRule::builder("", test_condition()).build();
            assert!(rule.is_err());
            match rule {
                Err(AlertError::InvalidRule { reason }) => {
                    assert!(reason.contains("empty"));
                }
                _ => panic!("expected InvalidRule error"),
            }
        }

        #[test]
        fn rule_name_too_long_fails() {
            let long_name = "a".repeat(AlertRule::MAX_NAME_LENGTH + 1);
            let rule = AlertRule::builder(long_name, test_condition()).build();
            assert!(rule.is_err());
            match rule {
                Err(AlertError::InvalidRule { reason }) => {
                    assert!(reason.contains("maximum length"));
                }
                _ => panic!("expected InvalidRule error"),
            }
        }

        #[test]
        fn rule_for_duration() {
            let rule = AlertRule::builder("test", test_condition())
                .for_duration(Duration::from_secs(300))
                .build()
                .unwrap();

            assert_eq!(rule.for_duration(), Duration::from_secs(300));
        }

        #[test]
        fn rule_disabled() {
            let rule = AlertRule::builder("test", test_condition())
                .enabled(false)
                .build()
                .unwrap();

            assert!(!rule.enabled);
        }

        #[test]
        fn rule_multiple_labels() {
            let mut labels = HashMap::new();
            labels.insert("env".to_string(), "prod".to_string());
            labels.insert("region".to_string(), "us-west".to_string());

            let rule = AlertRule::builder("test", test_condition())
                .labels(labels)
                .label("team", "sre")
                .build()
                .unwrap();

            assert_eq!(rule.labels.get("env"), Some(&"prod".to_string()));
            assert_eq!(rule.labels.get("region"), Some(&"us-west".to_string()));
            assert_eq!(rule.labels.get("team"), Some(&"sre".to_string()));
        }

        #[test]
        fn rule_serialization_roundtrip() {
            let original = AlertRule::builder("TestRule", test_condition())
                .for_duration_secs(120)
                .severity(AlertSeverity::Warning)
                .label("env", "test")
                .annotation("description", "Test alert")
                .build()
                .unwrap();

            let json = serde_json::to_string(&original);
            assert!(json.is_ok());

            let parsed: serde_json::Result<AlertRule> = serde_json::from_str(&json.unwrap());
            assert!(parsed.is_ok());
            assert_eq!(parsed.unwrap(), original);
        }
    }

    mod alert_tests {
        use super::*;

        fn test_rule() -> AlertRule {
            let condition =
                AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0).unwrap();
            AlertRule::builder("HighCPU", condition)
                .severity(AlertSeverity::Warning)
                .label("team", "platform")
                .annotation("summary", "High CPU usage")
                .build()
                .unwrap()
        }

        #[test]
        fn create_pending_alert() {
            let rule = test_rule();
            let mut metric_labels = HashMap::new();
            metric_labels.insert("node".to_string(), "node-1".to_string());

            let alert = Alert::new_pending(&rule, 85.0, metric_labels);

            assert_eq!(alert.rule_id, rule.id);
            assert_eq!(alert.rule_name, "HighCPU");
            assert_eq!(alert.state, AlertState::Pending);
            assert_eq!(alert.severity, AlertSeverity::Warning);
            assert!((alert.value - 85.0).abs() < f64::EPSILON);
            assert!(alert.fired_at.is_none());
            assert!(alert.resolved_at.is_none());
            assert_eq!(alert.labels.get("alertname"), Some(&"HighCPU".to_string()));
            assert_eq!(alert.labels.get("node"), Some(&"node-1".to_string()));
            assert_eq!(alert.labels.get("team"), Some(&"platform".to_string()));
        }

        #[test]
        fn alert_fire() {
            let rule = test_rule();
            let mut alert = Alert::new_pending(&rule, 85.0, HashMap::new());

            assert_eq!(alert.state, AlertState::Pending);
            assert!(alert.fired_at.is_none());

            alert.fire();

            assert_eq!(alert.state, AlertState::Firing);
            assert!(alert.fired_at.is_some());
        }

        #[test]
        fn alert_fire_idempotent() {
            let rule = test_rule();
            let mut alert = Alert::new_pending(&rule, 85.0, HashMap::new());

            alert.fire();
            let fired_at = alert.fired_at;

            // Fire again - should not change
            alert.fire();
            assert_eq!(alert.fired_at, fired_at);
        }

        #[test]
        fn alert_resolve() {
            let rule = test_rule();
            let mut alert = Alert::new_pending(&rule, 85.0, HashMap::new());

            alert.fire();
            alert.resolve();

            assert_eq!(alert.state, AlertState::Resolved);
            assert!(alert.resolved_at.is_some());
        }

        #[test]
        fn alert_resolve_from_pending() {
            let rule = test_rule();
            let mut alert = Alert::new_pending(&rule, 85.0, HashMap::new());

            alert.resolve();

            assert_eq!(alert.state, AlertState::Resolved);
        }

        #[test]
        fn alert_resolve_idempotent() {
            let rule = test_rule();
            let mut alert = Alert::new_pending(&rule, 85.0, HashMap::new());

            alert.resolve();
            let resolved_at = alert.resolved_at;

            alert.resolve();
            assert_eq!(alert.resolved_at, resolved_at);
        }

        #[test]
        fn alert_update_value() {
            let rule = test_rule();
            let mut alert = Alert::new_pending(&rule, 85.0, HashMap::new());

            alert.update_value(90.0);
            assert!((alert.value - 90.0).abs() < f64::EPSILON);
        }

        #[test]
        fn alert_is_active() {
            let rule = test_rule();
            let mut alert = Alert::new_pending(&rule, 85.0, HashMap::new());

            assert!(alert.is_active());

            alert.fire();
            assert!(alert.is_active());

            alert.resolve();
            assert!(!alert.is_active());
        }

        #[test]
        fn alert_fingerprint_consistent() {
            let rule = test_rule();
            let mut labels = HashMap::new();
            labels.insert("node".to_string(), "node-1".to_string());

            let alert1 = Alert::new_pending(&rule, 85.0, labels.clone());
            let alert2 = Alert::new_pending(&rule, 90.0, labels);

            // Same rule + labels = same fingerprint (different values don't matter)
            assert_eq!(alert1.fingerprint, alert2.fingerprint);
        }

        #[test]
        fn alert_fingerprint_different_labels() {
            let rule = test_rule();

            let mut labels1 = HashMap::new();
            labels1.insert("node".to_string(), "node-1".to_string());

            let mut labels2 = HashMap::new();
            labels2.insert("node".to_string(), "node-2".to_string());

            let alert1 = Alert::new_pending(&rule, 85.0, labels1);
            let alert2 = Alert::new_pending(&rule, 85.0, labels2);

            // Different labels = different fingerprint
            assert_ne!(alert1.fingerprint, alert2.fingerprint);
        }

        #[test]
        fn alert_serialization_roundtrip() {
            let rule = test_rule();
            let original = Alert::new_pending(&rule, 85.0, HashMap::new());

            let json = serde_json::to_string(&original);
            assert!(json.is_ok());

            let parsed: serde_json::Result<Alert> = serde_json::from_str(&json.unwrap());
            assert!(parsed.is_ok());
            assert_eq!(parsed.unwrap(), original);
        }
    }

    mod silence_tests {
        use super::*;
        use chrono::Duration as ChronoDuration;

        fn future_time(hours: i64) -> DateTime<Utc> {
            Utc::now() + ChronoDuration::hours(hours)
        }

        fn past_time(hours: i64) -> DateTime<Utc> {
            Utc::now() - ChronoDuration::hours(hours)
        }

        #[test]
        fn create_silence() {
            let mut matchers = HashMap::new();
            matchers.insert("alertname".to_string(), "HighCPU".to_string());

            let silence = Silence::new(
                matchers,
                Utc::now(),
                future_time(1),
                "admin",
                "Maintenance window",
            );

            assert!(silence.is_ok());
            let silence = silence.unwrap();
            assert_eq!(silence.created_by, "admin");
            assert_eq!(silence.comment, "Maintenance window");
        }

        #[test]
        fn silence_end_before_start_fails() {
            let matchers = HashMap::new();

            let silence = Silence::new(matchers, future_time(1), Utc::now(), "admin", "Bad");

            assert!(silence.is_err());
            match silence {
                Err(AlertError::InvalidDuration { .. }) => {}
                _ => panic!("expected InvalidDuration error"),
            }
        }

        #[test]
        fn silence_is_active() {
            let matchers = HashMap::new();

            // Active silence
            let active_silence =
                Silence::new(matchers.clone(), past_time(1), future_time(1), "admin", "Active")
                    .unwrap();
            assert!(active_silence.is_active());

            // Expired silence
            let expired_silence =
                Silence::new(matchers.clone(), past_time(2), past_time(1), "admin", "Expired")
                    .unwrap();
            assert!(!expired_silence.is_active());

            // Future silence
            let future_silence =
                Silence::new(matchers, future_time(1), future_time(2), "admin", "Future").unwrap();
            assert!(!future_silence.is_active());
        }

        #[test]
        fn silence_matches_alert() {
            let condition =
                AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0).unwrap();
            let rule = AlertRule::builder("HighCPU", condition)
                .severity(AlertSeverity::Warning)
                .build()
                .unwrap();

            let mut metric_labels = HashMap::new();
            metric_labels.insert("node".to_string(), "node-1".to_string());
            let alert = Alert::new_pending(&rule, 85.0, metric_labels);

            // Silence matching alertname
            let mut matchers = HashMap::new();
            matchers.insert("alertname".to_string(), "HighCPU".to_string());

            let silence =
                Silence::new(matchers, past_time(1), future_time(1), "admin", "test").unwrap();

            assert!(silence.matches(&alert));
        }

        #[test]
        fn silence_does_not_match_different_alert() {
            let condition =
                AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0).unwrap();
            let rule = AlertRule::builder("HighCPU", condition)
                .severity(AlertSeverity::Warning)
                .build()
                .unwrap();

            let alert = Alert::new_pending(&rule, 85.0, HashMap::new());

            let mut matchers = HashMap::new();
            matchers.insert("alertname".to_string(), "LowMemory".to_string());

            let silence =
                Silence::new(matchers, past_time(1), future_time(1), "admin", "test").unwrap();

            assert!(!silence.matches(&alert));
        }

        #[test]
        fn silence_inactive_does_not_match() {
            let condition =
                AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0).unwrap();
            let rule = AlertRule::builder("HighCPU", condition)
                .severity(AlertSeverity::Warning)
                .build()
                .unwrap();

            let alert = Alert::new_pending(&rule, 85.0, HashMap::new());

            let mut matchers = HashMap::new();
            matchers.insert("alertname".to_string(), "HighCPU".to_string());

            // Expired silence
            let silence =
                Silence::new(matchers, past_time(2), past_time(1), "admin", "expired").unwrap();

            assert!(!silence.matches(&alert));
        }

        #[test]
        fn silence_serialization_roundtrip() {
            let mut matchers = HashMap::new();
            matchers.insert("alertname".to_string(), "Test".to_string());

            let original =
                Silence::new(matchers, Utc::now(), future_time(1), "admin", "test").unwrap();

            let json = serde_json::to_string(&original);
            assert!(json.is_ok());

            let parsed: serde_json::Result<Silence> = serde_json::from_str(&json.unwrap());
            assert!(parsed.is_ok());
            assert_eq!(parsed.unwrap(), original);
        }
    }
}
