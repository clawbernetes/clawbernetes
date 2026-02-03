#![allow(clippy::unnecessary_wraps, clippy::items_after_statements, clippy::missing_errors_doc)]
//! Rollback execution engine.
//!
//! This module provides functionality to plan, validate, and execute
//! rollback operations.

use crate::history::DeploymentHistory;
use crate::types::{
    DeploymentId, RollbackPlan, RollbackResult, RollbackStrategy,
    RollbackTrigger,
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::{info, warn};

/// Errors that can occur during rollback operations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RollbackError {
    /// The specified deployment was not found.
    #[error("Deployment not found: {0}")]
    DeploymentNotFound(String),

    /// No previous deployment available to rollback to.
    #[error("No previous deployment available for rollback")]
    NoPreviousDeployment,

    /// The rollback plan is invalid.
    #[error("Invalid rollback plan: {0}")]
    InvalidPlan(String),

    /// The rollback execution failed.
    #[error("Rollback execution failed: {0}")]
    ExecutionFailed(String),

    /// A validation check failed.
    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    /// The target deployment is the same as the current deployment.
    #[error("Cannot rollback to the same deployment")]
    SameDeployment,
}

/// Result type for rollback operations.
pub type Result<T> = std::result::Result<T, RollbackError>;

/// Status of a rollback execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStatus {
    /// Rollback is pending execution.
    Pending,
    /// Rollback is currently in progress.
    InProgress,
    /// Rollback completed successfully.
    Completed,
    /// Rollback failed.
    Failed,
    /// Rollback was cancelled.
    Cancelled,
}

/// Options for rollback execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionOptions {
    /// Whether to run pre-flight validation.
    pub validate: bool,
    /// Whether to perform a dry run (no actual changes).
    pub dry_run: bool,
    /// Timeout for the rollback operation.
    pub timeout: Duration,
    /// Whether to wait for the rollback to complete.
    pub wait_for_completion: bool,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            validate: true,
            dry_run: false,
            timeout: Duration::from_secs(300), // 5 minutes
            wait_for_completion: true,
        }
    }
}

impl ExecutionOptions {
    /// Creates new execution options with defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enables or disables pre-flight validation.
    #[must_use]
    pub const fn with_validation(mut self, validate: bool) -> Self {
        self.validate = validate;
        self
    }

    /// Enables or disables dry run mode.
    #[must_use]
    pub const fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Sets the execution timeout.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Enables or disables waiting for completion.
    #[must_use]
    pub const fn with_wait(mut self, wait: bool) -> Self {
        self.wait_for_completion = wait;
        self
    }
}

/// Executes rollback operations.
#[derive(Debug)]
pub struct RollbackExecutor {
    /// Deployment history for looking up snapshots.
    history: DeploymentHistory,
    /// Execution options.
    options: ExecutionOptions,
}

impl RollbackExecutor {
    /// Creates a new rollback executor with the given history.
    #[must_use]
    pub fn new(history: DeploymentHistory) -> Self {
        Self {
            history,
            options: ExecutionOptions::default(),
        }
    }

    /// Creates an executor with custom options.
    #[must_use]
    pub const fn with_options(history: DeploymentHistory, options: ExecutionOptions) -> Self {
        Self { history, options }
    }

    /// Returns a reference to the deployment history.
    #[must_use]
    pub const fn history(&self) -> &DeploymentHistory {
        &self.history
    }

    /// Returns a mutable reference to the deployment history.
    pub const fn history_mut(&mut self) -> &mut DeploymentHistory {
        &mut self.history
    }

    /// Returns a reference to the execution options.
    #[must_use]
    pub const fn options(&self) -> &ExecutionOptions {
        &self.options
    }

    /// Updates the execution options.
    pub const fn set_options(&mut self, options: ExecutionOptions) {
        self.options = options;
    }

    /// Plans a rollback from the current deployment to a target.
    ///
    /// If `target` is `None`, rolls back to the immediately previous deployment.
    ///
    /// # Arguments
    ///
    /// * `current` - The current deployment ID.
    /// * `target` - Optional target deployment ID. If `None`, uses the previous deployment.
    ///
    /// # Returns
    ///
    /// A `RollbackPlan` if successful.
    ///
    /// # Errors
    ///
    /// Returns an error if the current or target deployment is not found,
    /// or if there's no previous deployment when target is `None`.
    pub fn plan_rollback(
        &self,
        current: &DeploymentId,
        target: Option<&DeploymentId>,
    ) -> Result<RollbackPlan> {
        // Find the current deployment
        let from_snapshot = self
            .history
            .find(current)
            .ok_or_else(|| RollbackError::DeploymentNotFound(current.to_string()))?;

        // Find the target deployment
        let to_snapshot = match target {
            Some(target_id) => {
                // Check if target is the same as current
                if target_id == current {
                    return Err(RollbackError::SameDeployment);
                }
                self.history
                    .find(target_id)
                    .ok_or_else(|| RollbackError::DeploymentNotFound(target_id.to_string()))?
            }
            None => {
                // Use the previous deployment
                self.history
                    .get_previous(current)
                    .ok_or(RollbackError::NoPreviousDeployment)?
            }
        };

        let plan = RollbackPlan::new(
            from_snapshot,
            to_snapshot,
            RollbackTrigger::Manual,
            RollbackStrategy::default(),
        );

        info!(
            rollback_id = %plan.id,
            from = %plan.from.id,
            to = %plan.to.id,
            "Created rollback plan"
        );

        Ok(plan)
    }

    /// Plans a rollback with a specific trigger and strategy.
    ///
    /// # Arguments
    ///
    /// * `current` - The current deployment ID.
    /// * `target` - Optional target deployment ID.
    /// * `trigger` - The trigger that initiated the rollback.
    /// * `strategy` - The rollback strategy to use.
    ///
    /// # Returns
    ///
    /// A `RollbackPlan` if successful.
    pub fn plan_rollback_with_options(
        &self,
        current: &DeploymentId,
        target: Option<&DeploymentId>,
        trigger: RollbackTrigger,
        strategy: RollbackStrategy,
    ) -> Result<RollbackPlan> {
        let mut plan = self.plan_rollback(current, target)?;
        plan.trigger = trigger;
        plan.strategy = strategy;
        Ok(plan)
    }

    /// Validates a rollback plan before execution.
    ///
    /// Performs pre-flight checks to ensure the rollback can be executed safely.
    ///
    /// # Arguments
    ///
    /// * `plan` - The rollback plan to validate.
    ///
    /// # Returns
    ///
    /// `Ok(())` if validation passes.
    ///
    /// # Errors
    ///
    /// Returns an error describing the validation failure.
    pub fn validate_rollback(&self, plan: &RollbackPlan) -> Result<()> {
        // Check that from and to are different
        if plan.from.id == plan.to.id {
            return Err(RollbackError::SameDeployment);
        }

        // Validate the from deployment exists in history
        if self.history.find(&plan.from.id).is_none() {
            return Err(RollbackError::ValidationFailed(format!(
                "Source deployment '{}' not found in history",
                plan.from.id
            )));
        }

        // Validate the to deployment exists in history
        if self.history.find(&plan.to.id).is_none() {
            return Err(RollbackError::ValidationFailed(format!(
                "Target deployment '{}' not found in history",
                plan.to.id
            )));
        }

        // Validate strategy-specific requirements
        match &plan.strategy {
            RollbackStrategy::Rolling { batch_size } => {
                if *batch_size == 0 {
                    return Err(RollbackError::InvalidPlan(
                        "Rolling strategy batch_size must be > 0".to_string(),
                    ));
                }
            }
            RollbackStrategy::Canary {
                initial_percentage,
                increment,
            } => {
                if *initial_percentage == 0 || *initial_percentage > 100 {
                    return Err(RollbackError::InvalidPlan(
                        "Canary initial_percentage must be between 1 and 100".to_string(),
                    ));
                }
                if *increment == 0 || *increment > 100 {
                    return Err(RollbackError::InvalidPlan(
                        "Canary increment must be between 1 and 100".to_string(),
                    ));
                }
            }
            RollbackStrategy::Immediate | RollbackStrategy::BlueGreen => {
                // No additional validation needed
            }
        }

        info!(rollback_id = %plan.id, "Rollback plan validated successfully");
        Ok(())
    }

    /// Executes a rollback plan.
    ///
    /// # Arguments
    ///
    /// * `plan` - The rollback plan to execute.
    ///
    /// # Returns
    ///
    /// A `RollbackResult` describing the outcome.
    ///
    /// # Errors
    ///
    /// Returns an error if the rollback fails.
    pub fn execute(&mut self, plan: &RollbackPlan) -> Result<RollbackResult> {
        let start_time = Instant::now();

        info!(
            rollback_id = %plan.id,
            from = %plan.from.id,
            to = %plan.to.id,
            strategy = ?plan.strategy,
            "Starting rollback execution"
        );

        // Run validation if enabled
        if self.options.validate {
            self.validate_rollback(plan)?;
        }

        // If dry run, return early
        if self.options.dry_run {
            info!(rollback_id = %plan.id, "Dry run completed");
            return Ok(RollbackResult::success(start_time.elapsed())
                .with_details("Dry run - no changes made"));
        }

        // Execute based on strategy
        let execution_result = self.execute_strategy(plan, start_time);

        match &execution_result {
            Ok(result) => {
                if result.success {
                    info!(
                        rollback_id = %plan.id,
                        duration_ms = result.duration.as_millis(),
                        "Rollback completed successfully"
                    );
                } else {
                    warn!(
                        rollback_id = %plan.id,
                        details = %result.details,
                        "Rollback completed with failure"
                    );
                }
            }
            Err(e) => {
                warn!(
                    rollback_id = %plan.id,
                    error = %e,
                    "Rollback execution error"
                );
            }
        }

        execution_result
    }

    /// Executes the rollback based on the strategy.
    fn execute_strategy(
        &mut self,
        plan: &RollbackPlan,
        start_time: Instant,
    ) -> Result<RollbackResult> {
        match &plan.strategy {
            RollbackStrategy::Immediate => self.execute_immediate(plan, start_time),
            RollbackStrategy::Rolling { batch_size } => {
                self.execute_rolling(plan, *batch_size, start_time)
            }
            RollbackStrategy::BlueGreen => self.execute_blue_green(plan, start_time),
            RollbackStrategy::Canary {
                initial_percentage,
                increment,
            } => self.execute_canary(plan, *initial_percentage, *increment, start_time),
        }
    }

    /// Executes an immediate rollback.
    fn execute_immediate(
        &mut self,
        plan: &RollbackPlan,
        start_time: Instant,
    ) -> Result<RollbackResult> {
        // In a real implementation, this would:
        // 1. Stop the current deployment
        // 2. Start the target deployment
        // For now, we simulate success

        // Record the rollback target as the new current deployment
        self.history.record(plan.to.clone());

        Ok(RollbackResult::success(start_time.elapsed())
            .with_details(format!(
                "Immediate rollback from {} to {}",
                plan.from.id, plan.to.id
            )))
    }

    /// Executes a rolling rollback.
    fn execute_rolling(
        &mut self,
        plan: &RollbackPlan,
        batch_size: u32,
        start_time: Instant,
    ) -> Result<RollbackResult> {
        // Guard against zero batch size
        if batch_size == 0 {
            return Err(RollbackError::InvalidPlan(
                "Rolling strategy batch_size must be > 0".to_string(),
            ));
        }

        // In a real implementation, this would gradually shift instances
        // For now, we simulate success

        let total_replicas = plan.from.spec.replicas;
        let batches = total_replicas.div_ceil(batch_size);

        // Record the rollback target as the new current deployment
        self.history.record(plan.to.clone());

        Ok(RollbackResult::success(start_time.elapsed())
            .with_details(format!(
                "Rolling rollback from {} to {} completed in {} batches",
                plan.from.id, plan.to.id, batches
            )))
    }

    /// Executes a blue-green rollback.
    fn execute_blue_green(
        &mut self,
        plan: &RollbackPlan,
        start_time: Instant,
    ) -> Result<RollbackResult> {
        // In a real implementation, this would switch traffic atomically
        // For now, we simulate success

        // Record the rollback target as the new current deployment
        self.history.record(plan.to.clone());

        Ok(RollbackResult::success(start_time.elapsed())
            .with_details(format!(
                "Blue-green rollback from {} to {} (traffic switched)",
                plan.from.id, plan.to.id
            )))
    }

    /// Executes a canary rollback.
    fn execute_canary(
        &mut self,
        plan: &RollbackPlan,
        initial_percentage: u8,
        increment: u8,
        start_time: Instant,
    ) -> Result<RollbackResult> {
        // In a real implementation, this would gradually shift traffic
        // For now, we simulate success

        let steps = (100 - initial_percentage) / increment + 1;

        // Record the rollback target as the new current deployment
        self.history.record(plan.to.clone());

        Ok(RollbackResult::success(start_time.elapsed())
            .with_details(format!(
                "Canary rollback from {} to {} completed in {} steps ({}% -> 100%)",
                plan.from.id, plan.to.id, steps, initial_percentage
            )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DeploymentSnapshot, DeploymentSpec};

    fn create_history_with_deployments() -> DeploymentHistory {
        let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
        
        let snapshot1 = DeploymentSnapshot::new(
            DeploymentId::new("v1"),
            DeploymentSpec::new("app", "app:v1").with_replicas(3),
        );
        let snapshot2 = DeploymentSnapshot::new(
            DeploymentId::new("v2"),
            DeploymentSpec::new("app", "app:v2").with_replicas(3),
        );
        let snapshot3 = DeploymentSnapshot::new(
            DeploymentId::new("v3"),
            DeploymentSpec::new("app", "app:v3").with_replicas(3),
        );

        history.record(snapshot1);
        history.record(snapshot2);
        history.record(snapshot3);

        history
    }

    mod execution_options_tests {
        use super::*;

        #[test]
        fn default_options_have_expected_values() {
            let options = ExecutionOptions::default();
            assert!(options.validate);
            assert!(!options.dry_run);
            assert_eq!(options.timeout, Duration::from_secs(300));
            assert!(options.wait_for_completion);
        }

        #[test]
        fn builder_pattern_works() {
            let options = ExecutionOptions::new()
                .with_validation(false)
                .with_dry_run(true)
                .with_timeout(Duration::from_secs(60))
                .with_wait(false);

            assert!(!options.validate);
            assert!(options.dry_run);
            assert_eq!(options.timeout, Duration::from_secs(60));
            assert!(!options.wait_for_completion);
        }
    }

    mod plan_rollback_tests {
        use super::*;

        #[test]
        fn plan_rollback_to_previous() {
            let history = create_history_with_deployments();
            let executor = RollbackExecutor::new(history);

            let plan = executor.plan_rollback(&DeploymentId::new("v3"), None);
            
            assert!(plan.is_ok());
            let plan = plan.unwrap_or_else(|_| panic!("should be Ok"));
            assert_eq!(plan.from.id, DeploymentId::new("v3"));
            assert_eq!(plan.to.id, DeploymentId::new("v2"));
        }

        #[test]
        fn plan_rollback_to_specific_version() {
            let history = create_history_with_deployments();
            let executor = RollbackExecutor::new(history);

            let plan = executor.plan_rollback(
                &DeploymentId::new("v3"),
                Some(&DeploymentId::new("v1")),
            );
            
            assert!(plan.is_ok());
            let plan = plan.unwrap_or_else(|_| panic!("should be Ok"));
            assert_eq!(plan.from.id, DeploymentId::new("v3"));
            assert_eq!(plan.to.id, DeploymentId::new("v1"));
        }

        #[test]
        fn plan_rollback_fails_for_unknown_current() {
            let history = create_history_with_deployments();
            let executor = RollbackExecutor::new(history);

            let plan = executor.plan_rollback(&DeploymentId::new("unknown"), None);
            
            assert!(plan.is_err());
            match plan {
                Err(RollbackError::DeploymentNotFound(id)) => {
                    assert_eq!(id, "unknown");
                }
                _ => panic!("Expected DeploymentNotFound error"),
            }
        }

        #[test]
        fn plan_rollback_fails_for_unknown_target() {
            let history = create_history_with_deployments();
            let executor = RollbackExecutor::new(history);

            let plan = executor.plan_rollback(
                &DeploymentId::new("v3"),
                Some(&DeploymentId::new("unknown")),
            );
            
            assert!(plan.is_err());
            match plan {
                Err(RollbackError::DeploymentNotFound(id)) => {
                    assert_eq!(id, "unknown");
                }
                _ => panic!("Expected DeploymentNotFound error"),
            }
        }

        #[test]
        fn plan_rollback_fails_for_first_deployment() {
            let history = create_history_with_deployments();
            let executor = RollbackExecutor::new(history);

            let plan = executor.plan_rollback(&DeploymentId::new("v1"), None);
            
            assert!(plan.is_err());
            assert!(matches!(plan, Err(RollbackError::NoPreviousDeployment)));
        }

        #[test]
        fn plan_rollback_fails_for_same_deployment() {
            let history = create_history_with_deployments();
            let executor = RollbackExecutor::new(history);

            let plan = executor.plan_rollback(
                &DeploymentId::new("v3"),
                Some(&DeploymentId::new("v3")),
            );
            
            assert!(plan.is_err());
            assert!(matches!(plan, Err(RollbackError::SameDeployment)));
        }

        #[test]
        fn plan_rollback_with_options_sets_trigger_and_strategy() {
            let history = create_history_with_deployments();
            let executor = RollbackExecutor::new(history);

            let plan = executor.plan_rollback_with_options(
                &DeploymentId::new("v3"),
                None,
                RollbackTrigger::error_rate(10.0),
                RollbackStrategy::BlueGreen,
            );
            
            assert!(plan.is_ok());
            let plan = plan.unwrap_or_else(|_| panic!("should be Ok"));
            assert!(matches!(plan.trigger, RollbackTrigger::ErrorRate { .. }));
            assert!(matches!(plan.strategy, RollbackStrategy::BlueGreen));
        }
    }

    mod validate_rollback_tests {
        use super::*;

        #[test]
        fn validate_succeeds_for_valid_plan() {
            let history = create_history_with_deployments();
            let executor = RollbackExecutor::new(history);

            let plan = executor.plan_rollback(&DeploymentId::new("v3"), None)
                .unwrap_or_else(|_| panic!("should be Ok"));

            let result = executor.validate_rollback(&plan);
            assert!(result.is_ok());
        }

        #[test]
        fn validate_fails_for_same_deployment() {
            let history = create_history_with_deployments();
            let snapshot = history.find(&DeploymentId::new("v3"))
                .unwrap_or_else(|| panic!("should find v3"));
            let executor = RollbackExecutor::new(history);

            // Manually construct an invalid plan
            let plan = RollbackPlan::new(
                snapshot.clone(),
                snapshot,
                RollbackTrigger::Manual,
                RollbackStrategy::default(),
            );

            let result = executor.validate_rollback(&plan);
            assert!(result.is_err());
            assert!(matches!(result, Err(RollbackError::SameDeployment)));
        }

        #[test]
        fn validate_fails_for_missing_source() {
            let history = create_history_with_deployments();
            let executor = RollbackExecutor::new(history.clone());

            let from = DeploymentSnapshot::new(
                DeploymentId::new("nonexistent"),
                DeploymentSpec::new("app", "app:v1"),
            );
            let to = history.find(&DeploymentId::new("v1"))
                .unwrap_or_else(|| panic!("should find v1"));

            let plan = RollbackPlan::new(
                from,
                to,
                RollbackTrigger::Manual,
                RollbackStrategy::default(),
            );

            let result = executor.validate_rollback(&plan);
            assert!(result.is_err());
            match result {
                Err(RollbackError::ValidationFailed(msg)) => {
                    assert!(msg.contains("Source deployment"));
                }
                _ => panic!("Expected ValidationFailed error"),
            }
        }

        #[test]
        fn validate_fails_for_missing_target() {
            let history = create_history_with_deployments();
            let executor = RollbackExecutor::new(history.clone());

            let from = history.find(&DeploymentId::new("v3"))
                .unwrap_or_else(|| panic!("should find v3"));
            let to = DeploymentSnapshot::new(
                DeploymentId::new("nonexistent"),
                DeploymentSpec::new("app", "app:v1"),
            );

            let plan = RollbackPlan::new(
                from,
                to,
                RollbackTrigger::Manual,
                RollbackStrategy::default(),
            );

            let result = executor.validate_rollback(&plan);
            assert!(result.is_err());
            match result {
                Err(RollbackError::ValidationFailed(msg)) => {
                    assert!(msg.contains("Target deployment"));
                }
                _ => panic!("Expected ValidationFailed error"),
            }
        }

        #[test]
        fn validate_fails_for_zero_batch_size() {
            let history = create_history_with_deployments();
            let executor = RollbackExecutor::new(history.clone());

            let from = history.find(&DeploymentId::new("v3"))
                .unwrap_or_else(|| panic!("should find v3"));
            let to = history.find(&DeploymentId::new("v2"))
                .unwrap_or_else(|| panic!("should find v2"));

            let plan = RollbackPlan::new(
                from,
                to,
                RollbackTrigger::Manual,
                RollbackStrategy::Rolling { batch_size: 0 },
            );

            let result = executor.validate_rollback(&plan);
            assert!(result.is_err());
            match result {
                Err(RollbackError::InvalidPlan(msg)) => {
                    assert!(msg.contains("batch_size"));
                }
                _ => panic!("Expected InvalidPlan error"),
            }
        }

        #[test]
        fn validate_fails_for_invalid_canary_percentage() {
            let history = create_history_with_deployments();
            let executor = RollbackExecutor::new(history.clone());

            let from = history.find(&DeploymentId::new("v3"))
                .unwrap_or_else(|| panic!("should find v3"));
            let to = history.find(&DeploymentId::new("v2"))
                .unwrap_or_else(|| panic!("should find v2"));

            let plan = RollbackPlan::new(
                from,
                to,
                RollbackTrigger::Manual,
                RollbackStrategy::Canary {
                    initial_percentage: 0,
                    increment: 10,
                },
            );

            let result = executor.validate_rollback(&plan);
            assert!(result.is_err());
            match result {
                Err(RollbackError::InvalidPlan(msg)) => {
                    assert!(msg.contains("initial_percentage"));
                }
                _ => panic!("Expected InvalidPlan error"),
            }
        }

        #[test]
        fn validate_fails_for_invalid_canary_increment() {
            let history = create_history_with_deployments();
            let executor = RollbackExecutor::new(history.clone());

            let from = history.find(&DeploymentId::new("v3"))
                .unwrap_or_else(|| panic!("should find v3"));
            let to = history.find(&DeploymentId::new("v2"))
                .unwrap_or_else(|| panic!("should find v2"));

            let plan = RollbackPlan::new(
                from,
                to,
                RollbackTrigger::Manual,
                RollbackStrategy::Canary {
                    initial_percentage: 10,
                    increment: 0,
                },
            );

            let result = executor.validate_rollback(&plan);
            assert!(result.is_err());
            match result {
                Err(RollbackError::InvalidPlan(msg)) => {
                    assert!(msg.contains("increment"));
                }
                _ => panic!("Expected InvalidPlan error"),
            }
        }
    }

    mod execute_tests {
        use super::*;

        #[test]
        fn execute_immediate_rollback() {
            let history = create_history_with_deployments();
            let mut executor = RollbackExecutor::new(history);

            let plan = executor.plan_rollback_with_options(
                &DeploymentId::new("v3"),
                Some(&DeploymentId::new("v1")),
                RollbackTrigger::Manual,
                RollbackStrategy::Immediate,
            ).unwrap_or_else(|_| panic!("should be Ok"));

            let result = executor.execute(&plan);
            
            assert!(result.is_ok());
            let result = result.unwrap_or_else(|_| panic!("should be Ok"));
            assert!(result.success);
            assert!(result.details.contains("Immediate"));

            // History should be updated
            let current = executor.history().current();
            assert!(current.is_some());
            assert_eq!(current.unwrap_or_else(|| panic!("should be Some")).id, DeploymentId::new("v1"));
        }

        #[test]
        fn execute_rolling_rollback() {
            let history = create_history_with_deployments();
            let mut executor = RollbackExecutor::new(history);

            let plan = executor.plan_rollback_with_options(
                &DeploymentId::new("v3"),
                None,
                RollbackTrigger::Manual,
                RollbackStrategy::Rolling { batch_size: 2 },
            ).unwrap_or_else(|_| panic!("should be Ok"));

            let result = executor.execute(&plan);
            
            assert!(result.is_ok());
            let result = result.unwrap_or_else(|_| panic!("should be Ok"));
            assert!(result.success);
            assert!(result.details.contains("Rolling"));
            assert!(result.details.contains("batches"));
        }

        #[test]
        fn execute_blue_green_rollback() {
            let history = create_history_with_deployments();
            let mut executor = RollbackExecutor::new(history);

            let plan = executor.plan_rollback_with_options(
                &DeploymentId::new("v3"),
                None,
                RollbackTrigger::Manual,
                RollbackStrategy::BlueGreen,
            ).unwrap_or_else(|_| panic!("should be Ok"));

            let result = executor.execute(&plan);
            
            assert!(result.is_ok());
            let result = result.unwrap_or_else(|_| panic!("should be Ok"));
            assert!(result.success);
            assert!(result.details.contains("Blue-green"));
        }

        #[test]
        fn execute_canary_rollback() {
            let history = create_history_with_deployments();
            let mut executor = RollbackExecutor::new(history);

            let plan = executor.plan_rollback_with_options(
                &DeploymentId::new("v3"),
                None,
                RollbackTrigger::Manual,
                RollbackStrategy::Canary {
                    initial_percentage: 10,
                    increment: 20,
                },
            ).unwrap_or_else(|_| panic!("should be Ok"));

            let result = executor.execute(&plan);
            
            assert!(result.is_ok());
            let result = result.unwrap_or_else(|_| panic!("should be Ok"));
            assert!(result.success);
            assert!(result.details.contains("Canary"));
        }

        #[test]
        fn execute_dry_run_makes_no_changes() {
            let history = create_history_with_deployments();
            let options = ExecutionOptions::new().with_dry_run(true);
            let mut executor = RollbackExecutor::with_options(history, options);

            let plan = executor.plan_rollback(&DeploymentId::new("v3"), None)
                .unwrap_or_else(|_| panic!("should be Ok"));

            let result = executor.execute(&plan);
            
            assert!(result.is_ok());
            let result = result.unwrap_or_else(|_| panic!("should be Ok"));
            assert!(result.success);
            assert!(result.details.contains("Dry run"));

            // History should NOT be updated
            let current = executor.history().current();
            assert_eq!(current.unwrap_or_else(|| panic!("should be Some")).id, DeploymentId::new("v3"));
        }

        #[test]
        fn execute_with_validation_disabled_still_catches_execution_errors() {
            let history = create_history_with_deployments();
            let options = ExecutionOptions::new().with_validation(false);
            let mut executor = RollbackExecutor::with_options(history.clone(), options);

            let from = history.find(&DeploymentId::new("v3"))
                .unwrap_or_else(|| panic!("should find v3"));
            let to = history.find(&DeploymentId::new("v2"))
                .unwrap_or_else(|| panic!("should find v2"));

            // Plan with zero batch size (invalid)
            let plan = RollbackPlan::new(
                from,
                to,
                RollbackTrigger::Manual,
                RollbackStrategy::Rolling { batch_size: 0 },
            );

            // With validation disabled, pre-flight check is skipped
            // but execution itself will still fail for invalid batch_size
            let result = executor.execute(&plan);
            assert!(result.is_err());
            match result {
                Err(RollbackError::InvalidPlan(msg)) => {
                    assert!(msg.contains("batch_size"));
                }
                _ => panic!("Expected InvalidPlan error"),
            }
        }

        #[test]
        fn execute_with_validation_disabled_skips_preflight() {
            let history = create_history_with_deployments();
            let options = ExecutionOptions::new().with_validation(false);
            let mut executor = RollbackExecutor::with_options(history, options);

            // Execute a valid rollback with validation disabled
            let plan = executor.plan_rollback(&DeploymentId::new("v3"), None)
                .unwrap_or_else(|_| panic!("should be Ok"));

            let result = executor.execute(&plan);
            assert!(result.is_ok());
            let result = result.unwrap_or_else(|_| panic!("should be Ok"));
            assert!(result.success);
        }
    }

    mod executor_accessors_tests {
        use super::*;

        #[test]
        fn history_returns_reference() {
            let history = create_history_with_deployments();
            let executor = RollbackExecutor::new(history);
            
            assert_eq!(executor.history().len(), 3);
        }

        #[test]
        fn history_mut_allows_modification() {
            let history = create_history_with_deployments();
            let mut executor = RollbackExecutor::new(history);
            
            let new_snapshot = DeploymentSnapshot::new(
                DeploymentId::new("v4"),
                DeploymentSpec::new("app", "app:v4"),
            );
            executor.history_mut().record(new_snapshot);
            
            assert_eq!(executor.history().len(), 4);
        }

        #[test]
        fn options_returns_reference() {
            let history = create_history_with_deployments();
            let executor = RollbackExecutor::new(history);
            
            assert!(executor.options().validate);
        }

        #[test]
        fn set_options_updates_options() {
            let history = create_history_with_deployments();
            let mut executor = RollbackExecutor::new(history);
            
            let new_options = ExecutionOptions::new().with_dry_run(true);
            executor.set_options(new_options);
            
            assert!(executor.options().dry_run);
        }
    }

    mod error_tests {
        use super::*;

        #[test]
        fn error_display_messages() {
            let err = RollbackError::DeploymentNotFound("v1".to_string());
            assert!(err.to_string().contains("Deployment not found"));
            assert!(err.to_string().contains("v1"));

            let err = RollbackError::NoPreviousDeployment;
            assert!(err.to_string().contains("No previous deployment"));

            let err = RollbackError::InvalidPlan("bad plan".to_string());
            assert!(err.to_string().contains("Invalid rollback plan"));

            let err = RollbackError::ExecutionFailed("timeout".to_string());
            assert!(err.to_string().contains("Rollback execution failed"));

            let err = RollbackError::ValidationFailed("check failed".to_string());
            assert!(err.to_string().contains("Validation failed"));

            let err = RollbackError::SameDeployment;
            assert!(err.to_string().contains("same deployment"));
        }
    }
}
