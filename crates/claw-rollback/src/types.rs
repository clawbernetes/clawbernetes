//! Core types for the rollback system.
//!
//! This module defines the fundamental data structures used throughout
//! the rollback system, including identifiers, snapshots, triggers, and results.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

/// Unique identifier for a rollback operation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RollbackId(Uuid);

impl RollbackId {
    /// Creates a new random rollback ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates a rollback ID from an existing UUID.
    #[must_use]
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the inner UUID.
    #[must_use]
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for RollbackId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RollbackId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a deployment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeploymentId(String);

impl DeploymentId {
    /// Creates a new deployment ID from a string.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DeploymentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Deployment specification containing the configuration for a deployment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeploymentSpec {
    /// Name of the deployment.
    pub name: String,
    /// Image/version being deployed.
    pub image: String,
    /// Number of replicas.
    pub replicas: u32,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// Resource limits.
    pub resources: ResourceLimits,
}

impl DeploymentSpec {
    /// Creates a new deployment spec with minimal configuration.
    #[must_use]
    pub fn new(name: impl Into<String>, image: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            image: image.into(),
            replicas: 1,
            env: HashMap::new(),
            resources: ResourceLimits::default(),
        }
    }

    /// Sets the number of replicas.
    #[must_use]
    pub fn with_replicas(mut self, replicas: u32) -> Self {
        self.replicas = replicas;
        self
    }

    /// Adds an environment variable.
    #[must_use]
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Sets the resource limits.
    #[must_use]
    pub fn with_resources(mut self, resources: ResourceLimits) -> Self {
        self.resources = resources;
        self
    }
}

/// Resource limits for a deployment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// CPU limit in millicores.
    pub cpu_millis: u64,
    /// Memory limit in bytes.
    pub memory_bytes: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_millis: 1000,      // 1 CPU core
            memory_bytes: 512_000_000, // 512MB
        }
    }
}

/// Metrics collected at deployment time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Metrics {
    /// Error rate as a percentage (0.0 - 100.0).
    pub error_rate: f64,
    /// P50 latency in milliseconds.
    pub p50_latency_ms: f64,
    /// P99 latency in milliseconds.
    pub p99_latency_ms: f64,
    /// Requests per second.
    pub requests_per_second: f64,
    /// CPU utilization as a percentage (0.0 - 100.0).
    pub cpu_utilization: f64,
    /// Memory utilization as a percentage (0.0 - 100.0).
    pub memory_utilization: f64,
    /// Number of consecutive health check failures.
    pub health_check_failures: u32,
    /// Custom metrics.
    pub custom: HashMap<String, f64>,
}

impl Metrics {
    /// Creates new empty metrics.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the error rate.
    #[must_use]
    pub fn with_error_rate(mut self, rate: f64) -> Self {
        self.error_rate = rate;
        self
    }

    /// Sets the P99 latency.
    #[must_use]
    pub fn with_p99_latency_ms(mut self, latency: f64) -> Self {
        self.p99_latency_ms = latency;
        self
    }

    /// Sets the P50 latency.
    #[must_use]
    pub fn with_p50_latency_ms(mut self, latency: f64) -> Self {
        self.p50_latency_ms = latency;
        self
    }

    /// Sets the health check failures count.
    #[must_use]
    pub fn with_health_check_failures(mut self, failures: u32) -> Self {
        self.health_check_failures = failures;
        self
    }

    /// Adds a custom metric.
    #[must_use]
    pub fn with_custom(mut self, key: impl Into<String>, value: f64) -> Self {
        self.custom.insert(key.into(), value);
        self
    }
}

/// A snapshot of a deployment at a specific point in time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeploymentSnapshot {
    /// Unique identifier for this deployment.
    pub id: DeploymentId,
    /// The deployment specification.
    pub spec: DeploymentSpec,
    /// When this deployment was created/recorded.
    pub timestamp: DateTime<Utc>,
    /// Metrics captured at deployment time.
    pub metrics_at_deploy: Metrics,
}

impl DeploymentSnapshot {
    /// Creates a new deployment snapshot.
    #[must_use]
    pub fn new(id: DeploymentId, spec: DeploymentSpec) -> Self {
        Self {
            id,
            spec,
            timestamp: Utc::now(),
            metrics_at_deploy: Metrics::default(),
        }
    }

    /// Creates a snapshot with a specific timestamp.
    #[must_use]
    pub fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Attaches metrics captured at deployment time.
    #[must_use]
    pub fn with_metrics(mut self, metrics: Metrics) -> Self {
        self.metrics_at_deploy = metrics;
        self
    }
}

/// Trigger conditions that can initiate a rollback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RollbackTrigger {
    /// Manual rollback initiated by an operator.
    Manual,
    /// Automatic rollback when error rate exceeds threshold.
    ErrorRate {
        /// Error rate threshold as a percentage (0.0 - 100.0).
        threshold: f64,
    },
    /// Automatic rollback when latency exceeds threshold.
    Latency {
        /// P99 latency threshold in milliseconds.
        threshold_ms: f64,
    },
    /// Automatic rollback when health checks fail.
    HealthCheck {
        /// Number of consecutive failures before triggering.
        failure_threshold: u32,
    },
    /// Custom trigger with a reason.
    Custom {
        /// Description of the custom trigger condition.
        reason: String,
    },
}

impl RollbackTrigger {
    /// Creates a new error rate trigger with the given threshold.
    #[must_use]
    pub fn error_rate(threshold: f64) -> Self {
        Self::ErrorRate { threshold }
    }

    /// Creates a new latency trigger with the given threshold.
    #[must_use]
    pub fn latency(threshold_ms: f64) -> Self {
        Self::Latency { threshold_ms }
    }

    /// Creates a new health check trigger with the given failure threshold.
    #[must_use]
    pub fn health_check(failure_threshold: u32) -> Self {
        Self::HealthCheck { failure_threshold }
    }

    /// Creates a new custom trigger with the given reason.
    #[must_use]
    pub fn custom(reason: impl Into<String>) -> Self {
        Self::Custom {
            reason: reason.into(),
        }
    }

    /// Returns a human-readable description of the trigger.
    #[must_use]
    pub fn description(&self) -> String {
        match self {
            Self::Manual => "Manual rollback".to_string(),
            Self::ErrorRate { threshold } => format!("Error rate exceeded {threshold}%"),
            Self::Latency { threshold_ms } => format!("P99 latency exceeded {threshold_ms}ms"),
            Self::HealthCheck { failure_threshold } => {
                format!("Health check failed {failure_threshold} consecutive times")
            }
            Self::Custom { reason } => format!("Custom: {reason}"),
        }
    }
}

/// Strategy for executing a rollback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RollbackStrategy {
    /// Immediate rollback - stop current and start previous.
    Immediate,
    /// Rolling rollback - gradually shift traffic.
    Rolling {
        /// Number of instances to roll back at a time.
        batch_size: u32,
    },
    /// Blue-green style rollback.
    BlueGreen,
    /// Canary rollback - gradual traffic shift with monitoring.
    Canary {
        /// Initial percentage of traffic to shift.
        initial_percentage: u8,
        /// Increment for each step.
        increment: u8,
    },
}

impl Default for RollbackStrategy {
    fn default() -> Self {
        Self::Rolling { batch_size: 1 }
    }
}

/// A plan for executing a rollback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RollbackPlan {
    /// Unique identifier for this rollback operation.
    pub id: RollbackId,
    /// The deployment to rollback from.
    pub from: DeploymentSnapshot,
    /// The deployment to rollback to.
    pub to: DeploymentSnapshot,
    /// The trigger that caused this rollback.
    pub trigger: RollbackTrigger,
    /// The strategy to use for the rollback.
    pub strategy: RollbackStrategy,
    /// When this plan was created.
    pub created_at: DateTime<Utc>,
}

impl RollbackPlan {
    /// Creates a new rollback plan.
    #[must_use]
    pub fn new(
        from: DeploymentSnapshot,
        to: DeploymentSnapshot,
        trigger: RollbackTrigger,
        strategy: RollbackStrategy,
    ) -> Self {
        Self {
            id: RollbackId::new(),
            from,
            to,
            trigger,
            strategy,
            created_at: Utc::now(),
        }
    }

    /// Creates a plan with a specific ID.
    #[must_use]
    pub fn with_id(mut self, id: RollbackId) -> Self {
        self.id = id;
        self
    }
}

/// The result of executing a rollback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RollbackResult {
    /// Whether the rollback succeeded.
    pub success: bool,
    /// How long the rollback took.
    pub duration: Duration,
    /// Root cause analysis (if available).
    pub root_cause: Option<RootCause>,
    /// When the rollback completed.
    pub completed_at: DateTime<Utc>,
    /// Additional details about the rollback.
    pub details: String,
}

impl RollbackResult {
    /// Creates a successful rollback result.
    #[must_use]
    pub fn success(duration: Duration) -> Self {
        Self {
            success: true,
            duration,
            root_cause: None,
            completed_at: Utc::now(),
            details: String::new(),
        }
    }

    /// Creates a failed rollback result.
    #[must_use]
    pub fn failure(duration: Duration, details: impl Into<String>) -> Self {
        Self {
            success: false,
            duration,
            root_cause: None,
            completed_at: Utc::now(),
            details: details.into(),
        }
    }

    /// Attaches root cause analysis.
    #[must_use]
    pub fn with_root_cause(mut self, root_cause: RootCause) -> Self {
        self.root_cause = Some(root_cause);
        self
    }

    /// Adds details to the result.
    #[must_use]
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = details.into();
        self
    }
}

/// Categories of root causes for deployment failures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootCauseCategory {
    /// Configuration error (bad env vars, missing secrets, etc.).
    ConfigError,
    /// Resource exhaustion (OOM, CPU throttling, etc.).
    ResourceExhaustion,
    /// Dependency failure (database down, API unavailable, etc.).
    DependencyFailure,
    /// Bug in the deployed code.
    CodeBug,
    /// Unable to determine root cause.
    Unknown,
}

impl std::fmt::Display for RootCauseCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfigError => write!(f, "Configuration Error"),
            Self::ResourceExhaustion => write!(f, "Resource Exhaustion"),
            Self::DependencyFailure => write!(f, "Dependency Failure"),
            Self::CodeBug => write!(f, "Code Bug"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Root cause analysis result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RootCause {
    /// Category of the root cause.
    pub category: RootCauseCategory,
    /// Human-readable description of the root cause.
    pub description: String,
    /// Evidence supporting this root cause analysis.
    pub evidence: Vec<String>,
    /// Recommended actions to fix the issue.
    pub recommendation: String,
}

impl RootCause {
    /// Creates a new root cause analysis.
    #[must_use]
    pub fn new(
        category: RootCauseCategory,
        description: impl Into<String>,
        recommendation: impl Into<String>,
    ) -> Self {
        Self {
            category,
            description: description.into(),
            evidence: Vec::new(),
            recommendation: recommendation.into(),
        }
    }

    /// Adds evidence to the root cause.
    #[must_use]
    pub fn with_evidence(mut self, evidence: impl Into<String>) -> Self {
        self.evidence.push(evidence.into());
        self
    }

    /// Adds multiple pieces of evidence.
    #[must_use]
    pub fn with_evidence_list(mut self, evidence: Vec<String>) -> Self {
        self.evidence.extend(evidence);
        self
    }
}

/// Log entry for root cause analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    /// Timestamp of the log entry.
    pub timestamp: DateTime<Utc>,
    /// Log level.
    pub level: LogLevel,
    /// Log message.
    pub message: String,
    /// Optional structured fields.
    pub fields: HashMap<String, String>,
}

impl LogEntry {
    /// Creates a new log entry.
    #[must_use]
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            level,
            message: message.into(),
            fields: HashMap::new(),
        }
    }

    /// Sets the timestamp.
    #[must_use]
    pub fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Adds a field to the log entry.
    #[must_use]
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }
}

/// Log levels for log entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    /// Trace level.
    Trace,
    /// Debug level.
    Debug,
    /// Info level.
    Info,
    /// Warning level.
    Warn,
    /// Error level.
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    mod rollback_id_tests {
        use super::*;

        #[test]
        fn new_creates_unique_ids() {
            let id1 = RollbackId::new();
            let id2 = RollbackId::new();
            assert_ne!(id1, id2);
        }

        #[test]
        fn from_uuid_preserves_uuid() {
            let uuid = Uuid::new_v4();
            let id = RollbackId::from_uuid(uuid);
            assert_eq!(id.as_uuid(), &uuid);
        }

        #[test]
        fn display_shows_uuid() {
            let uuid = Uuid::new_v4();
            let id = RollbackId::from_uuid(uuid);
            assert_eq!(id.to_string(), uuid.to_string());
        }

        #[test]
        fn default_creates_new_id() {
            let id1 = RollbackId::default();
            let id2 = RollbackId::default();
            assert_ne!(id1, id2);
        }

        #[test]
        fn serialization_roundtrip() {
            let id = RollbackId::new();
            let json = serde_json::to_string(&id).unwrap_or_default();
            let deserialized: RollbackId = serde_json::from_str(&json).unwrap_or_default();
            assert_eq!(id, deserialized);
        }
    }

    mod deployment_id_tests {
        use super::*;

        #[test]
        fn new_stores_string() {
            let id = DeploymentId::new("my-deployment-v1");
            assert_eq!(id.as_str(), "my-deployment-v1");
        }

        #[test]
        fn display_shows_id() {
            let id = DeploymentId::new("test-id");
            assert_eq!(id.to_string(), "test-id");
        }

        #[test]
        fn equality_works() {
            let id1 = DeploymentId::new("same");
            let id2 = DeploymentId::new("same");
            let id3 = DeploymentId::new("different");
            assert_eq!(id1, id2);
            assert_ne!(id1, id3);
        }
    }

    mod deployment_spec_tests {
        use super::*;

        #[test]
        fn new_creates_minimal_spec() {
            let spec = DeploymentSpec::new("my-app", "my-app:v1.0");
            assert_eq!(spec.name, "my-app");
            assert_eq!(spec.image, "my-app:v1.0");
            assert_eq!(spec.replicas, 1);
            assert!(spec.env.is_empty());
        }

        #[test]
        fn with_replicas_sets_count() {
            let spec = DeploymentSpec::new("app", "img").with_replicas(3);
            assert_eq!(spec.replicas, 3);
        }

        #[test]
        fn with_env_adds_variable() {
            let spec = DeploymentSpec::new("app", "img")
                .with_env("KEY1", "value1")
                .with_env("KEY2", "value2");
            assert_eq!(spec.env.get("KEY1"), Some(&"value1".to_string()));
            assert_eq!(spec.env.get("KEY2"), Some(&"value2".to_string()));
        }

        #[test]
        fn with_resources_sets_limits() {
            let resources = ResourceLimits {
                cpu_millis: 2000,
                memory_bytes: 1_000_000_000,
            };
            let spec = DeploymentSpec::new("app", "img").with_resources(resources.clone());
            assert_eq!(spec.resources, resources);
        }
    }

    mod metrics_tests {
        use super::*;

        #[test]
        fn new_creates_zeroed_metrics() {
            let metrics = Metrics::new();
            assert!((metrics.error_rate - 0.0).abs() < f64::EPSILON);
            assert!((metrics.p99_latency_ms - 0.0).abs() < f64::EPSILON);
            assert_eq!(metrics.health_check_failures, 0);
        }

        #[test]
        fn builder_pattern_works() {
            let metrics = Metrics::new()
                .with_error_rate(5.5)
                .with_p99_latency_ms(150.0)
                .with_health_check_failures(2)
                .with_custom("queue_depth", 100.0);

            assert!((metrics.error_rate - 5.5).abs() < f64::EPSILON);
            assert!((metrics.p99_latency_ms - 150.0).abs() < f64::EPSILON);
            assert_eq!(metrics.health_check_failures, 2);
            assert_eq!(metrics.custom.get("queue_depth"), Some(&100.0));
        }
    }

    mod deployment_snapshot_tests {
        use super::*;

        #[test]
        fn new_creates_snapshot_with_current_time() {
            let before = Utc::now();
            let snapshot = DeploymentSnapshot::new(
                DeploymentId::new("deploy-1"),
                DeploymentSpec::new("app", "img:v1"),
            );
            let after = Utc::now();

            assert!(snapshot.timestamp >= before);
            assert!(snapshot.timestamp <= after);
        }

        #[test]
        fn with_timestamp_overrides_time() {
            let fixed_time = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                .map(|dt| dt.to_utc())
                .unwrap_or_else(|_| Utc::now());

            let snapshot = DeploymentSnapshot::new(
                DeploymentId::new("deploy-1"),
                DeploymentSpec::new("app", "img"),
            )
            .with_timestamp(fixed_time);

            assert_eq!(snapshot.timestamp, fixed_time);
        }

        #[test]
        fn with_metrics_attaches_metrics() {
            let metrics = Metrics::new().with_error_rate(1.5);
            let snapshot = DeploymentSnapshot::new(
                DeploymentId::new("deploy-1"),
                DeploymentSpec::new("app", "img"),
            )
            .with_metrics(metrics.clone());

            assert_eq!(snapshot.metrics_at_deploy, metrics);
        }

        #[test]
        fn serialization_roundtrip() {
            let snapshot = DeploymentSnapshot::new(
                DeploymentId::new("deploy-1"),
                DeploymentSpec::new("app", "img:v1").with_replicas(3),
            )
            .with_metrics(Metrics::new().with_error_rate(2.0));

            let json = serde_json::to_string(&snapshot).unwrap_or_default();
            let deserialized: Result<DeploymentSnapshot, _> = serde_json::from_str(&json);
            assert!(deserialized.is_ok());
        }
    }

    mod rollback_trigger_tests {
        use super::*;

        #[test]
        fn error_rate_creates_trigger() {
            let trigger = RollbackTrigger::error_rate(5.0);
            match trigger {
                RollbackTrigger::ErrorRate { threshold } => {
                    assert!((threshold - 5.0).abs() < f64::EPSILON);
                }
                _ => panic!("Expected ErrorRate trigger"),
            }
        }

        #[test]
        fn latency_creates_trigger() {
            let trigger = RollbackTrigger::latency(200.0);
            match trigger {
                RollbackTrigger::Latency { threshold_ms } => {
                    assert!((threshold_ms - 200.0).abs() < f64::EPSILON);
                }
                _ => panic!("Expected Latency trigger"),
            }
        }

        #[test]
        fn health_check_creates_trigger() {
            let trigger = RollbackTrigger::health_check(3);
            match trigger {
                RollbackTrigger::HealthCheck { failure_threshold } => {
                    assert_eq!(failure_threshold, 3);
                }
                _ => panic!("Expected HealthCheck trigger"),
            }
        }

        #[test]
        fn custom_creates_trigger() {
            let trigger = RollbackTrigger::custom("Memory leak detected");
            match trigger {
                RollbackTrigger::Custom { reason } => {
                    assert_eq!(reason, "Memory leak detected");
                }
                _ => panic!("Expected Custom trigger"),
            }
        }

        #[test]
        fn description_formats_correctly() {
            assert_eq!(RollbackTrigger::Manual.description(), "Manual rollback");
            assert_eq!(
                RollbackTrigger::error_rate(5.0).description(),
                "Error rate exceeded 5%"
            );
            assert_eq!(
                RollbackTrigger::latency(200.0).description(),
                "P99 latency exceeded 200ms"
            );
            assert_eq!(
                RollbackTrigger::health_check(3).description(),
                "Health check failed 3 consecutive times"
            );
            assert_eq!(
                RollbackTrigger::custom("test").description(),
                "Custom: test"
            );
        }

        #[test]
        fn serialization_roundtrip() {
            let triggers = vec![
                RollbackTrigger::Manual,
                RollbackTrigger::error_rate(5.0),
                RollbackTrigger::latency(200.0),
                RollbackTrigger::health_check(3),
                RollbackTrigger::custom("reason"),
            ];

            for trigger in triggers {
                let json = serde_json::to_string(&trigger).unwrap_or_default();
                let deserialized: Result<RollbackTrigger, _> = serde_json::from_str(&json);
                assert!(deserialized.is_ok());
            }
        }
    }

    mod rollback_strategy_tests {
        use super::*;

        #[test]
        fn default_is_rolling_with_batch_1() {
            let strategy = RollbackStrategy::default();
            match strategy {
                RollbackStrategy::Rolling { batch_size } => assert_eq!(batch_size, 1),
                _ => panic!("Expected Rolling strategy"),
            }
        }

        #[test]
        fn all_variants_serialize() {
            let strategies = vec![
                RollbackStrategy::Immediate,
                RollbackStrategy::Rolling { batch_size: 2 },
                RollbackStrategy::BlueGreen,
                RollbackStrategy::Canary {
                    initial_percentage: 10,
                    increment: 10,
                },
            ];

            for strategy in strategies {
                let json = serde_json::to_string(&strategy).unwrap_or_default();
                assert!(!json.is_empty());
            }
        }
    }

    mod rollback_plan_tests {
        use super::*;

        fn create_snapshot(id: &str, image: &str) -> DeploymentSnapshot {
            DeploymentSnapshot::new(
                DeploymentId::new(id),
                DeploymentSpec::new("app", image),
            )
        }

        #[test]
        fn new_creates_plan_with_unique_id() {
            let from = create_snapshot("v2", "img:v2");
            let to = create_snapshot("v1", "img:v1");

            let plan1 = RollbackPlan::new(
                from.clone(),
                to.clone(),
                RollbackTrigger::Manual,
                RollbackStrategy::default(),
            );
            let plan2 = RollbackPlan::new(
                from,
                to,
                RollbackTrigger::Manual,
                RollbackStrategy::default(),
            );

            assert_ne!(plan1.id, plan2.id);
        }

        #[test]
        fn with_id_overrides_id() {
            let from = create_snapshot("v2", "img:v2");
            let to = create_snapshot("v1", "img:v1");
            let custom_id = RollbackId::new();

            let plan = RollbackPlan::new(
                from,
                to,
                RollbackTrigger::Manual,
                RollbackStrategy::default(),
            )
            .with_id(custom_id.clone());

            assert_eq!(plan.id, custom_id);
        }

        #[test]
        fn plan_contains_all_fields() {
            let from = create_snapshot("v2", "img:v2");
            let to = create_snapshot("v1", "img:v1");
            let trigger = RollbackTrigger::error_rate(10.0);
            let strategy = RollbackStrategy::BlueGreen;

            let plan = RollbackPlan::new(from.clone(), to.clone(), trigger.clone(), strategy.clone());

            assert_eq!(plan.from.id, from.id);
            assert_eq!(plan.to.id, to.id);
            assert_eq!(plan.trigger, trigger);
            assert_eq!(plan.strategy, strategy);
        }
    }

    mod rollback_result_tests {
        use super::*;

        #[test]
        fn success_creates_successful_result() {
            let result = RollbackResult::success(Duration::from_secs(30));
            assert!(result.success);
            assert_eq!(result.duration, Duration::from_secs(30));
            assert!(result.root_cause.is_none());
        }

        #[test]
        fn failure_creates_failed_result() {
            let result = RollbackResult::failure(Duration::from_secs(10), "Connection timeout");
            assert!(!result.success);
            assert_eq!(result.duration, Duration::from_secs(10));
            assert_eq!(result.details, "Connection timeout");
        }

        #[test]
        fn with_root_cause_attaches_analysis() {
            let root_cause = RootCause::new(
                RootCauseCategory::ConfigError,
                "Missing API key",
                "Add API_KEY to environment",
            );

            let result = RollbackResult::failure(Duration::from_secs(5), "Startup failed")
                .with_root_cause(root_cause.clone());

            assert_eq!(result.root_cause, Some(root_cause));
        }
    }

    mod root_cause_tests {
        use super::*;

        #[test]
        fn new_creates_root_cause() {
            let cause = RootCause::new(
                RootCauseCategory::ResourceExhaustion,
                "Container OOM killed",
                "Increase memory limit",
            );

            assert_eq!(cause.category, RootCauseCategory::ResourceExhaustion);
            assert_eq!(cause.description, "Container OOM killed");
            assert_eq!(cause.recommendation, "Increase memory limit");
            assert!(cause.evidence.is_empty());
        }

        #[test]
        fn with_evidence_adds_items() {
            let cause = RootCause::new(
                RootCauseCategory::CodeBug,
                "Null pointer exception",
                "Fix null check",
            )
            .with_evidence("Stack trace at line 42")
            .with_evidence("Error log at 12:00:00");

            assert_eq!(cause.evidence.len(), 2);
            assert_eq!(cause.evidence[0], "Stack trace at line 42");
        }

        #[test]
        fn with_evidence_list_adds_multiple() {
            let evidence = vec!["Evidence 1".to_string(), "Evidence 2".to_string()];
            let cause = RootCause::new(
                RootCauseCategory::DependencyFailure,
                "Database unavailable",
                "Check DB connection",
            )
            .with_evidence_list(evidence);

            assert_eq!(cause.evidence.len(), 2);
        }

        #[test]
        fn category_display_formats_correctly() {
            assert_eq!(RootCauseCategory::ConfigError.to_string(), "Configuration Error");
            assert_eq!(RootCauseCategory::ResourceExhaustion.to_string(), "Resource Exhaustion");
            assert_eq!(RootCauseCategory::DependencyFailure.to_string(), "Dependency Failure");
            assert_eq!(RootCauseCategory::CodeBug.to_string(), "Code Bug");
            assert_eq!(RootCauseCategory::Unknown.to_string(), "Unknown");
        }
    }

    mod log_entry_tests {
        use super::*;

        #[test]
        fn new_creates_entry_with_current_time() {
            let before = Utc::now();
            let entry = LogEntry::new(LogLevel::Error, "Something went wrong");
            let after = Utc::now();

            assert!(entry.timestamp >= before);
            assert!(entry.timestamp <= after);
            assert_eq!(entry.level, LogLevel::Error);
            assert_eq!(entry.message, "Something went wrong");
        }

        #[test]
        fn with_field_adds_structured_data() {
            let entry = LogEntry::new(LogLevel::Info, "Request processed")
                .with_field("user_id", "123")
                .with_field("duration_ms", "50");

            assert_eq!(entry.fields.get("user_id"), Some(&"123".to_string()));
            assert_eq!(entry.fields.get("duration_ms"), Some(&"50".to_string()));
        }

        #[test]
        fn with_timestamp_overrides_time() {
            let fixed_time = DateTime::parse_from_rfc3339("2024-06-15T10:30:00Z")
                .map(|dt| dt.to_utc())
                .unwrap_or_else(|_| Utc::now());

            let entry = LogEntry::new(LogLevel::Warn, "Warning").with_timestamp(fixed_time);
            assert_eq!(entry.timestamp, fixed_time);
        }
    }
}
