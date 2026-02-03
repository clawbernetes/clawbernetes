//! Rollback system for Clawbernetes deployments.
//!
//! This crate provides comprehensive rollback functionality for Clawbernetes,
//! including:
//!
//! - **Deployment History Tracking**: Record and query deployment snapshots
//! - **Automatic Triggers**: Configure conditions that trigger automatic rollbacks
//! - **Rollback Execution**: Plan and execute rollbacks with various strategies
//! - **Root Cause Analysis**: Analyze failures to determine likely causes
//!
//! # Quick Start
//!
//! ```rust
//! use claw_rollback::{
//!     DeploymentHistory, DeploymentSnapshot, DeploymentId, DeploymentSpec,
//!     RollbackExecutor, RollbackTrigger, TriggerEvaluator, Metrics,
//!     DefaultTriggers, configure_triggers, TriggerConfig,
//! };
//!
//! // Create a deployment history
//! let mut history = DeploymentHistory::new(10).unwrap();
//!
//! // Record deployments
//! let v1 = DeploymentSnapshot::new(
//!     DeploymentId::new("v1"),
//!     DeploymentSpec::new("my-app", "my-app:v1.0"),
//! );
//! history.record(v1);
//!
//! let v2 = DeploymentSnapshot::new(
//!     DeploymentId::new("v2"),
//!     DeploymentSpec::new("my-app", "my-app:v2.0"),
//! );
//! history.record(v2);
//!
//! // Set up automatic triggers
//! let triggers = DefaultTriggers::all(100.0); // 100ms baseline latency
//!
//! // Evaluate current metrics against triggers
//! let evaluator = TriggerEvaluator::new();
//! let current_metrics = Metrics::new()
//!     .with_error_rate(10.0)  // 10% error rate - exceeds 5% threshold!
//!     .with_p99_latency_ms(150.0);
//!
//! if let Some(triggered) = evaluator.evaluate_all(&triggers, &current_metrics) {
//!     println!("Rollback triggered: {}", triggered.description());
//!     
//!     // Plan and execute rollback
//!     let mut executor = RollbackExecutor::new(history);
//!     let plan = executor.plan_rollback(&DeploymentId::new("v2"), None).unwrap();
//!     let result = executor.execute(&plan).unwrap();
//!     
//!     assert!(result.success);
//! }
//! ```
//!
//! # Modules
//!
//! - [`types`]: Core types for the rollback system
//! - [`history`]: Deployment history tracking
//! - [`triggers`]: Automatic rollback triggers
//! - [`executor`]: Rollback planning and execution
//! - [`analysis`]: Root cause analysis

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![deny(clippy::todo)]
#![deny(clippy::unimplemented)]

pub mod analysis;
pub mod executor;
pub mod history;
pub mod triggers;
pub mod types;

// Re-export commonly used types for convenience
pub use analysis::{analyze_failure, AnalysisConfig, FailureAnalyzer};
pub use executor::{ExecutionOptions, ExecutionStatus, RollbackError, RollbackExecutor};
pub use history::DeploymentHistory;
pub use triggers::{
    configure_triggers, ComparisonOperator, CustomTriggerConfig, DefaultTriggers, TriggerConfig,
    TriggerEvaluator,
};
pub use types::{
    DeploymentId, DeploymentSnapshot, DeploymentSpec, LogEntry, LogLevel, Metrics, ResourceLimits,
    RollbackId, RollbackPlan, RollbackResult, RollbackStrategy, RollbackTrigger, RootCause,
    RootCauseCategory,
};

/// Prelude module for convenient imports.
///
/// # Example
///
/// ```rust
/// use claw_rollback::prelude::*;
/// ```
pub mod prelude {
    pub use crate::analysis::{analyze_failure, AnalysisConfig, FailureAnalyzer};
    pub use crate::executor::{ExecutionOptions, RollbackError, RollbackExecutor};
    pub use crate::history::DeploymentHistory;
    pub use crate::triggers::{
        configure_triggers, DefaultTriggers, TriggerConfig, TriggerEvaluator,
    };
    pub use crate::types::{
        DeploymentId, DeploymentSnapshot, DeploymentSpec, Metrics, RollbackPlan, RollbackResult,
        RollbackStrategy, RollbackTrigger, RootCause, RootCauseCategory,
    };
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Creates a test deployment history with several versions.
    fn setup_test_history() -> DeploymentHistory {
        let mut history = DeploymentHistory::new(10).unwrap_or_else(|| {
            // Fallback for tests - this shouldn't happen with 10
            DeploymentHistory::new(1).unwrap_or_else(|| panic!("Failed to create history"))
        });

        let v1 = DeploymentSnapshot::new(
            DeploymentId::new("v1"),
            DeploymentSpec::new("test-app", "test-app:v1.0")
                .with_replicas(3)
                .with_env("ENV", "production"),
        );

        let v2 = DeploymentSnapshot::new(
            DeploymentId::new("v2"),
            DeploymentSpec::new("test-app", "test-app:v2.0")
                .with_replicas(3)
                .with_env("ENV", "production"),
        );

        let v3 = DeploymentSnapshot::new(
            DeploymentId::new("v3"),
            DeploymentSpec::new("test-app", "test-app:v3.0")
                .with_replicas(3)
                .with_env("ENV", "production"),
        );

        history.record(v1);
        history.record(v2);
        history.record(v3);

        history
    }

    #[test]
    fn full_rollback_workflow() {
        // Setup
        let history = setup_test_history();
        let mut executor = RollbackExecutor::new(history);

        // Configure triggers
        let triggers = DefaultTriggers::all(100.0);
        let evaluator = TriggerEvaluator::new();

        // Simulate unhealthy metrics
        let current_metrics = Metrics::new()
            .with_error_rate(10.0) // Exceeds 5% threshold
            .with_p99_latency_ms(150.0)
            .with_health_check_failures(0);

        // Evaluate triggers
        let triggered = evaluator.evaluate_all(&triggers, &current_metrics);
        assert!(triggered.is_some());

        // Plan rollback
        let current_id = DeploymentId::new("v3");
        let plan = executor.plan_rollback(&current_id, None);
        assert!(plan.is_ok());
        let plan = plan.unwrap_or_else(|_| panic!("Plan should succeed"));

        // Verify plan
        assert_eq!(plan.from.id.as_str(), "v3");
        assert_eq!(plan.to.id.as_str(), "v2");

        // Execute rollback
        let result = executor.execute(&plan);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("Execution should succeed"));
        assert!(result.success);

        // Verify history updated
        let current = executor.history().current();
        assert!(current.is_some());
        assert_eq!(current.unwrap_or_else(|| panic!("Should have current")).id.as_str(), "v2");
    }

    #[test]
    fn rollback_with_root_cause_analysis() {
        // Setup
        let history = setup_test_history();
        let mut executor = RollbackExecutor::new(history.clone());

        // Simulate failure scenario
        let failing_snapshot = history.find(&DeploymentId::new("v3"))
            .unwrap_or_else(|| panic!("Should find v3"));
        
        let metrics = Metrics::new()
            .with_error_rate(25.0)
            .with_p99_latency_ms(500.0)
            .with_health_check_failures(5);

        let logs = vec![
            LogEntry::new(LogLevel::Error, "Connection timeout to database"),
            LogEntry::new(LogLevel::Error, "Service unavailable"),
        ];

        // Analyze root cause
        let root_cause = analyze_failure(&failing_snapshot, &metrics, &logs);
        assert_eq!(root_cause.category, RootCauseCategory::DependencyFailure);
        assert!(!root_cause.evidence.is_empty());

        // Plan rollback with the trigger
        let plan = executor.plan_rollback_with_options(
            &DeploymentId::new("v3"),
            None,
            RollbackTrigger::error_rate(5.0),
            RollbackStrategy::BlueGreen,
        );
        assert!(plan.is_ok());

        // Execute
        let result = executor.execute(&plan.unwrap_or_else(|_| panic!("Should have plan")));
        assert!(result.is_ok());
        
        // Attach root cause to result
        let result = result
            .unwrap_or_else(|_| panic!("Should have result"))
            .with_root_cause(root_cause);
        
        assert!(result.root_cause.is_some());
    }

    #[test]
    fn custom_trigger_configuration() {
        let config = TriggerConfig::new()
            .with_error_rate_threshold(3.0) // Stricter than default
            .with_baseline_latency_ms(50.0) // Will use 100ms as 2x threshold
            .with_health_check_failure_threshold(2)
            .with_custom_trigger(CustomTriggerConfig::new(
                "queue_depth",
                ComparisonOperator::GreaterThan,
                1000.0,
                "Queue backlog exceeded",
            ));

        let triggers = configure_triggers(&config);
        assert_eq!(triggers.len(), 4); // error_rate, latency, health_check, custom

        let evaluator = TriggerEvaluator::new();

        // Test custom trigger
        let metrics = Metrics::new()
            .with_error_rate(1.0)
            .with_p99_latency_ms(50.0)
            .with_custom("queue_depth", 1500.0);

        let custom_config = &config.custom_triggers[0];
        assert!(evaluator.evaluate_custom(custom_config, &metrics));
    }

    #[test]
    fn rollback_to_specific_version() {
        let history = setup_test_history();
        let mut executor = RollbackExecutor::new(history);

        // Rollback from v3 directly to v1, skipping v2
        let plan = executor.plan_rollback(
            &DeploymentId::new("v3"),
            Some(&DeploymentId::new("v1")),
        );
        assert!(plan.is_ok());

        let plan = plan.unwrap_or_else(|_| panic!("Should have plan"));
        assert_eq!(plan.from.id.as_str(), "v3");
        assert_eq!(plan.to.id.as_str(), "v1");

        let result = executor.execute(&plan);
        assert!(result.is_ok());
        assert!(result.unwrap_or_else(|_| panic!("Should have result")).success);
    }

    #[test]
    fn dry_run_makes_no_changes() {
        let history = setup_test_history();
        let options = ExecutionOptions::new().with_dry_run(true);
        let mut executor = RollbackExecutor::with_options(history, options);

        let plan = executor.plan_rollback(&DeploymentId::new("v3"), None);
        assert!(plan.is_ok());

        let result = executor.execute(&plan.unwrap_or_else(|_| panic!("Should have plan")));
        assert!(result.is_ok());
        
        let result = result.unwrap_or_else(|_| panic!("Should have result"));
        assert!(result.success);
        assert!(result.details.contains("Dry run"));

        // History should be unchanged
        let current = executor.history().current();
        assert_eq!(
            current.unwrap_or_else(|| panic!("Should have current")).id.as_str(),
            "v3"
        );
    }

    #[test]
    fn history_operations() {
        let mut history = DeploymentHistory::new(5).unwrap_or_else(|| panic!("Should create"));

        // Record several deployments
        for i in 1..=7 {
            let snapshot = DeploymentSnapshot::new(
                DeploymentId::new(format!("v{i}")),
                DeploymentSpec::new("app", format!("app:v{i}")),
            );
            history.record(snapshot);
        }

        // Only last 5 should be retained (capacity is 5)
        assert_eq!(history.len(), 5);

        // v1 and v2 should be evicted
        assert!(history.find(&DeploymentId::new("v1")).is_none());
        assert!(history.find(&DeploymentId::new("v2")).is_none());

        // v3-v7 should exist
        assert!(history.find(&DeploymentId::new("v3")).is_some());
        assert!(history.find(&DeploymentId::new("v7")).is_some());

        // Test get_version
        let current = history.get_version(0);
        assert!(current.is_some());
        assert_eq!(current.unwrap_or_else(|| panic!("Should have")).id.as_str(), "v7");

        let two_back = history.get_version(2);
        assert!(two_back.is_some());
        assert_eq!(two_back.unwrap_or_else(|| panic!("Should have")).id.as_str(), "v5");

        // Test list_recent
        let recent = history.list_recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].id.as_str(), "v7");
        assert_eq!(recent[1].id.as_str(), "v6");
        assert_eq!(recent[2].id.as_str(), "v5");
    }

    #[test]
    fn trigger_evaluation() {
        let evaluator = TriggerEvaluator::with_baseline(
            Metrics::new().with_p99_latency_ms(100.0),
        );

        // Test each trigger type
        let triggers = vec![
            RollbackTrigger::error_rate(5.0),
            RollbackTrigger::latency(200.0),
            RollbackTrigger::health_check(3),
        ];

        // Healthy metrics - no triggers should fire
        let healthy = Metrics::new()
            .with_error_rate(2.0)
            .with_p99_latency_ms(100.0)
            .with_health_check_failures(1);

        assert!(evaluator.evaluate_all(&triggers, &healthy).is_none());

        // Unhealthy error rate
        let high_errors = Metrics::new().with_error_rate(10.0);
        let triggered = evaluator.evaluate_all(&triggers, &high_errors);
        assert!(matches!(
            triggered,
            Some(RollbackTrigger::ErrorRate { .. })
        ));

        // Unhealthy latency
        let high_latency = Metrics::new().with_p99_latency_ms(300.0);
        let triggered = evaluator.evaluate_all(&triggers, &high_latency);
        assert!(matches!(
            triggered,
            Some(RollbackTrigger::Latency { .. })
        ));

        // Unhealthy health checks
        let failing_health = Metrics::new().with_health_check_failures(5);
        let triggered = evaluator.evaluate_all(&triggers, &failing_health);
        assert!(matches!(
            triggered,
            Some(RollbackTrigger::HealthCheck { .. })
        ));
    }

    #[test]
    fn analysis_categorization() {
        let analyzer = FailureAnalyzer::new();
        let snapshot = DeploymentSnapshot::new(
            DeploymentId::new("test"),
            DeploymentSpec::new("app", "app:v1"),
        );

        // Resource exhaustion scenario
        let mut resource_metrics = Metrics::new();
        resource_metrics.memory_utilization = 95.0;
        resource_metrics.cpu_utilization = 98.0;

        let result = analyzer.analyze_failure(&snapshot, &resource_metrics, &[]);
        assert_eq!(result.category, RootCauseCategory::ResourceExhaustion);

        // Dependency failure scenario
        let dep_logs = vec![
            LogEntry::new(LogLevel::Error, "Connection refused to redis"),
            LogEntry::new(LogLevel::Error, "Database timeout after 30s"),
        ];

        let result = analyzer.analyze_failure(&snapshot, &Metrics::new(), &dep_logs);
        assert_eq!(result.category, RootCauseCategory::DependencyFailure);

        // Config error scenario
        let config_snapshot = DeploymentSnapshot::new(
            DeploymentId::new("test"),
            DeploymentSpec::new("app", "app:v1")
                .with_env("API_KEY", "")
                .with_env("SECRET", "${MISSING}"),
        );

        let result = analyzer.analyze_failure(&config_snapshot, &Metrics::new(), &[]);
        assert_eq!(result.category, RootCauseCategory::ConfigError);
    }
}
