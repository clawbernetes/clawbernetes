//! Deployment execution engine.
//!
//! This module handles the actual execution of deployments, managing the
//! lifecycle from start to completion (or rollback).

use crate::error::{DeployError, DeployResult};
use crate::strategy::{select_strategy, ClusterContext};
use crate::types::{DeploymentId, DeploymentIntent, DeploymentState, DeploymentStatus};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

/// Executor for managing deployments.
///
/// The executor maintains the state of all deployments and provides methods
/// for starting, promoting, and rolling back deployments.
#[derive(Debug)]
pub struct DeploymentExecutor {
    /// Active deployments keyed by ID
    deployments: Arc<RwLock<HashMap<DeploymentId, DeploymentStatus>>>,

    /// Default cluster context for strategy selection
    default_context: ClusterContext,
}

impl DeploymentExecutor {
    /// Creates a new deployment executor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            deployments: Arc::new(RwLock::new(HashMap::new())),
            default_context: ClusterContext::default(),
        }
    }

    /// Creates a new executor with a specific cluster context.
    #[must_use]
    pub fn with_context(context: ClusterContext) -> Self {
        Self {
            deployments: Arc::new(RwLock::new(HashMap::new())),
            default_context: context,
        }
    }

    /// Starts a new deployment.
    ///
    /// # Arguments
    ///
    /// * `intent` - The deployment intent describing what to deploy
    ///
    /// # Returns
    ///
    /// The unique ID of the started deployment.
    ///
    /// # Errors
    ///
    /// Returns an error if the intent is invalid or deployment cannot be started.
    pub fn start(&self, intent: &DeploymentIntent) -> DeployResult<DeploymentId> {
        // Validate the intent
        intent.validate()?;

        info!("Starting deployment for image: {}", intent.image);

        // Select strategy
        let context = self.build_context(intent);
        let strategy = select_strategy(intent, &context)?;

        debug!(
            "Selected strategy {:?} for deployment of {}",
            strategy, intent.image
        );

        // Create deployment ID and initial status
        let id = DeploymentId::new();
        let status = DeploymentStatus::new_pending(intent.image.clone(), intent.replicas, strategy);

        // Store the deployment
        {
            let mut deployments = self.deployments.write().map_err(|e| {
                DeployError::Internal(format!("Failed to acquire lock: {e}"))
            })?;
            deployments.insert(id.clone(), status);
        }

        info!("Created deployment {} for {}", id, intent.image);

        // Transition to deploying state
        self.transition_state(&id, DeploymentState::Deploying)?;

        Ok(id)
    }

    /// Gets the status of a deployment.
    ///
    /// # Errors
    ///
    /// Returns an error if the deployment is not found.
    pub fn get_status(&self, id: &DeploymentId) -> DeployResult<DeploymentStatus> {
        let deployments = self.deployments.read().map_err(|e| {
            DeployError::Internal(format!("Failed to acquire lock: {e}"))
        })?;

        deployments
            .get(id)
            .cloned()
            .ok_or_else(|| DeployError::not_found(id))
    }

    /// Promotes a canary deployment to full rollout.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The deployment is not found
    /// - The deployment is not in the Canary state
    pub fn promote(&self, id: &DeploymentId) -> DeployResult<()> {
        let status = self.get_status(id)?;

        if !status.state.can_promote() {
            return Err(DeployError::Promotion(format!(
                "Cannot promote deployment in {} state",
                status.state
            )));
        }

        info!("Promoting deployment {} from canary to full rollout", id);

        // Transition through promoting to complete
        self.transition_state(id, DeploymentState::Promoting)?;

        // Simulate promotion completing
        self.update_healthy_replicas(id, status.total_replicas)?;
        self.transition_state(id, DeploymentState::Complete)?;

        info!("Deployment {} promoted successfully", id);
        Ok(())
    }

    /// Rolls back a deployment.
    ///
    /// # Arguments
    ///
    /// * `id` - The deployment ID
    /// * `reason` - The reason for rollback
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The deployment is not found
    /// - The deployment cannot be rolled back (terminal state)
    pub fn rollback(&self, id: &DeploymentId, reason: &str) -> DeployResult<()> {
        let status = self.get_status(id)?;

        if !status.state.can_rollback() {
            return Err(DeployError::Rollback(format!(
                "Cannot rollback deployment in {} state",
                status.state
            )));
        }

        warn!(
            "Rolling back deployment {} for image {}: {}",
            id, status.image, reason
        );

        // Transition to rolling back
        self.transition_state(id, DeploymentState::RollingBack)?;
        self.set_message(id, format!("Rollback: {reason}"))?;

        // Simulate rollback completing (in real impl, this would restore previous version)
        self.update_healthy_replicas(id, 0)?;
        self.transition_state(id, DeploymentState::Failed)?;

        info!("Deployment {} rolled back", id);
        Ok(())
    }

    /// Lists all deployments.
    ///
    /// # Errors
    ///
    /// Returns an error if the lock cannot be acquired.
    pub fn list(&self) -> DeployResult<Vec<(DeploymentId, DeploymentStatus)>> {
        let deployments = self.deployments.read().map_err(|e| {
            DeployError::Internal(format!("Failed to acquire lock: {e}"))
        })?;

        Ok(deployments
            .iter()
            .map(|(id, status)| (id.clone(), status.clone()))
            .collect())
    }

    /// Lists deployments by state.
    ///
    /// # Errors
    ///
    /// Returns an error if the lock cannot be acquired.
    pub fn list_by_state(
        &self,
        state: DeploymentState,
    ) -> DeployResult<Vec<(DeploymentId, DeploymentStatus)>> {
        let deployments = self.deployments.read().map_err(|e| {
            DeployError::Internal(format!("Failed to acquire lock: {e}"))
        })?;

        Ok(deployments
            .iter()
            .filter(|(_, status)| status.state == state)
            .map(|(id, status)| (id.clone(), status.clone()))
            .collect())
    }

    /// Transitions a deployment to a new state.
    fn transition_state(&self, id: &DeploymentId, new_state: DeploymentState) -> DeployResult<()> {
        let mut deployments = self.deployments.write().map_err(|e| {
            DeployError::Internal(format!("Failed to acquire lock: {e}"))
        })?;

        let status = deployments
            .get_mut(id)
            .ok_or_else(|| DeployError::not_found(id))?;

        debug!(
            "Transitioning deployment {} from {} to {}",
            id, status.state, new_state
        );

        // Update state and timestamp
        *status = status.clone().with_state(new_state);

        drop(deployments);
        Ok(())
    }

    /// Updates the healthy replica count.
    fn update_healthy_replicas(&self, id: &DeploymentId, count: u32) -> DeployResult<()> {
        let mut deployments = self.deployments.write().map_err(|e| {
            DeployError::Internal(format!("Failed to acquire lock: {e}"))
        })?;

        let status = deployments
            .get_mut(id)
            .ok_or_else(|| DeployError::not_found(id))?;

        *status = status.clone().with_healthy_replicas(count);

        drop(deployments);
        Ok(())
    }

    /// Sets a status message.
    fn set_message(&self, id: &DeploymentId, message: String) -> DeployResult<()> {
        let mut deployments = self.deployments.write().map_err(|e| {
            DeployError::Internal(format!("Failed to acquire lock: {e}"))
        })?;

        let status = deployments
            .get_mut(id)
            .ok_or_else(|| DeployError::not_found(id))?;

        *status = status.clone().with_message(message);

        drop(deployments);
        Ok(())
    }

    /// Builds context for strategy selection.
    fn build_context(&self, intent: &DeploymentIntent) -> ClusterContext {
        let env = intent
            .constraints
            .environment
            .unwrap_or(self.default_context.environment);

        ClusterContext::new(env)
            .with_criticality(self.default_context.criticality)
            .with_recent_failures(self.default_context.recent_failures)
    }

    /// Moves a deployment to canary state (for testing and internal use).
    ///
    /// # Errors
    ///
    /// Returns an error if the deployment is not found or already in canary.
    pub fn enter_canary(&self, id: &DeploymentId, healthy_count: u32) -> DeployResult<()> {
        let status = self.get_status(id)?;

        if status.state != DeploymentState::Deploying {
            return Err(DeployError::invalid_transition(
                status.state.to_string(),
                "canary",
            ));
        }

        self.transition_state(id, DeploymentState::Canary)?;
        self.update_healthy_replicas(id, healthy_count)?;

        Ok(())
    }
}

impl Default for DeploymentExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DeploymentStrategy, StrategyHint};

    mod executor_new {
        use super::*;

        #[test]
        fn creates_empty_executor() {
            let executor = DeploymentExecutor::new();
            let list = executor.list();
            assert!(list.is_ok());
            assert!(list.as_ref().ok().map_or(false, Vec::is_empty));
        }

        #[test]
        fn with_context_sets_defaults() {
            use crate::types::Environment;
            let context = ClusterContext::new(Environment::Production).with_criticality(80);
            let executor = DeploymentExecutor::with_context(context);

            // Start a deployment - it should use production defaults
            let intent = DeploymentIntent::new("myapp:v1.0");
            let id = executor.start(&intent);
            assert!(id.is_ok());

            let status = executor.get_status(&id.ok().as_ref().cloned().unwrap_or_default());
            // Should have canary strategy in production
            assert!(status.is_ok());
        }
    }

    mod executor_start {
        use super::*;

        #[test]
        fn starts_valid_deployment() {
            let executor = DeploymentExecutor::new();
            let intent = DeploymentIntent::new("myapp:v1.0").with_replicas(3);

            let id = executor.start(&intent);
            assert!(id.is_ok());
        }

        #[test]
        fn rejects_invalid_intent() {
            let executor = DeploymentExecutor::new();
            let intent = DeploymentIntent::new(""); // Empty image

            let result = executor.start(&intent);
            assert!(result.is_err());
        }

        #[test]
        fn creates_unique_ids() {
            let executor = DeploymentExecutor::new();

            let id1 = executor.start(&DeploymentIntent::new("app1:v1"));
            let id2 = executor.start(&DeploymentIntent::new("app2:v1"));

            assert!(id1.is_ok());
            assert!(id2.is_ok());
            assert_ne!(id1.ok(), id2.ok());
        }

        #[test]
        fn starts_in_deploying_state() {
            let executor = DeploymentExecutor::new();
            let intent = DeploymentIntent::new("myapp:v1.0");

            let id = executor.start(&intent);
            assert!(id.is_ok());

            let status = id.ok().as_ref().and_then(|id| executor.get_status(id).ok());
            assert_eq!(status.map(|s| s.state), Some(DeploymentState::Deploying));
        }

        #[test]
        fn uses_specified_strategy() {
            let executor = DeploymentExecutor::new();
            let intent = DeploymentIntent::new("myapp:v1.0")
                .with_strategy_hint(StrategyHint::BlueGreen);

            let id = executor.start(&intent);
            assert!(id.is_ok());

            let status = id.ok().as_ref().and_then(|id| executor.get_status(id).ok());
            assert!(matches!(
                status.map(|s| s.strategy),
                Some(DeploymentStrategy::BlueGreen)
            ));
        }
    }

    mod executor_get_status {
        use super::*;

        #[test]
        fn returns_status_for_existing_deployment() {
            let executor = DeploymentExecutor::new();
            let intent = DeploymentIntent::new("myapp:v1.0").with_replicas(5);

            let id = executor.start(&intent);
            assert!(id.is_ok());

            let status = id.ok().as_ref().and_then(|id| executor.get_status(id).ok());
            assert!(status.is_some());
            assert_eq!(status.as_ref().map(|s| s.total_replicas), Some(5));
            assert_eq!(status.as_ref().map(|s| &s.image).map(String::as_str), Some("myapp:v1.0"));
        }

        #[test]
        fn returns_error_for_nonexistent_deployment() {
            let executor = DeploymentExecutor::new();
            let fake_id = DeploymentId::new();

            let result = executor.get_status(&fake_id);
            assert!(result.is_err());
        }
    }

    mod executor_promote {
        use super::*;

        #[test]
        fn promotes_canary_deployment() {
            let executor = DeploymentExecutor::new();
            let intent = DeploymentIntent::new("myapp:v1.0")
                .with_replicas(10)
                .with_strategy_hint(StrategyHint::Canary { percentage: 10 });

            let id = executor.start(&intent);
            assert!(id.is_ok());
            let id = id.ok();

            // Move to canary state
            let enter_result = id.as_ref().map(|id| executor.enter_canary(id, 1));
            assert!(enter_result.map_or(false, |r| r.is_ok()));

            // Promote
            let promote_result = id.as_ref().map(|id| executor.promote(id));
            assert!(promote_result.map_or(false, |r| r.is_ok()));

            // Should be complete
            let status = id.as_ref().and_then(|id| executor.get_status(id).ok());
            assert_eq!(status.as_ref().map(|s| s.state), Some(DeploymentState::Complete));
            assert_eq!(status.as_ref().map(|s| s.healthy_replicas), Some(10));
        }

        #[test]
        fn fails_to_promote_non_canary() {
            let executor = DeploymentExecutor::new();
            let intent = DeploymentIntent::new("myapp:v1.0");

            let id = executor.start(&intent);
            assert!(id.is_ok());
            let id = id.ok();

            // Try to promote from deploying state
            let result = id.as_ref().map(|id| executor.promote(id));
            assert!(result.map_or(true, |r| r.is_err()));
        }
    }

    mod executor_rollback {
        use super::*;

        #[test]
        fn rolls_back_deploying_deployment() {
            let executor = DeploymentExecutor::new();
            let intent = DeploymentIntent::new("myapp:v1.0");

            let id = executor.start(&intent);
            assert!(id.is_ok());
            let id = id.ok();

            let result = id.as_ref().map(|id| executor.rollback(id, "test failure"));
            assert!(result.map_or(false, |r| r.is_ok()));

            let status = id.as_ref().and_then(|id| executor.get_status(id).ok());
            assert_eq!(status.as_ref().map(|s| s.state), Some(DeploymentState::Failed));
            assert!(status
                .as_ref()
                .and_then(|s| s.message.as_ref())
                .map_or(false, |m| m.contains("test failure")));
        }

        #[test]
        fn rolls_back_canary_deployment() {
            let executor = DeploymentExecutor::new();
            let intent = DeploymentIntent::new("myapp:v1.0")
                .with_strategy_hint(StrategyHint::Canary { percentage: 10 });

            let id = executor.start(&intent);
            assert!(id.is_ok());
            let id = id.ok();

            let enter_result = id.as_ref().map(|id| executor.enter_canary(id, 1));
            assert!(enter_result.map_or(false, |r| r.is_ok()));

            let result = id.as_ref().map(|id| executor.rollback(id, "metrics degraded"));
            assert!(result.map_or(false, |r| r.is_ok()));

            let status = id.as_ref().and_then(|id| executor.get_status(id).ok());
            assert_eq!(status.map(|s| s.state), Some(DeploymentState::Failed));
        }

        #[test]
        fn fails_to_rollback_completed() {
            let executor = DeploymentExecutor::new();
            let intent = DeploymentIntent::new("myapp:v1.0")
                .with_strategy_hint(StrategyHint::Canary { percentage: 10 });

            let id = executor.start(&intent);
            assert!(id.is_ok());
            let id = id.ok();

            let enter_result = id.as_ref().map(|id| executor.enter_canary(id, 1));
            assert!(enter_result.map_or(false, |r| r.is_ok()));

            let promote_result = id.as_ref().map(|id| executor.promote(id));
            assert!(promote_result.map_or(false, |r| r.is_ok()));

            // Now try to rollback
            let result = id.as_ref().map(|id| executor.rollback(id, "too late"));
            assert!(result.map_or(true, |r| r.is_err()));
        }
    }

    mod executor_list {
        use super::*;

        #[test]
        fn lists_all_deployments() {
            let executor = DeploymentExecutor::new();

            let _ = executor.start(&DeploymentIntent::new("app1:v1"));
            let _ = executor.start(&DeploymentIntent::new("app2:v1"));
            let _ = executor.start(&DeploymentIntent::new("app3:v1"));

            let list = executor.list();
            assert!(list.is_ok());
            assert_eq!(list.as_ref().ok().map(Vec::len), Some(3));
        }

        #[test]
        fn lists_by_state() {
            let executor = DeploymentExecutor::new();

            let id1 = executor.start(
                &DeploymentIntent::new("app1:v1").with_strategy_hint(StrategyHint::Canary {
                    percentage: 10,
                }),
            );
            let _ = executor.start(&DeploymentIntent::new("app2:v1"));

            // Move first to canary
            if let Some(id) = id1.ok().as_ref() {
                let _ = executor.enter_canary(id, 1);
            }

            let canary_list = executor.list_by_state(DeploymentState::Canary);
            let deploying_list = executor.list_by_state(DeploymentState::Deploying);

            assert_eq!(canary_list.as_ref().ok().map(Vec::len), Some(1));
            assert_eq!(deploying_list.as_ref().ok().map(Vec::len), Some(1));
        }
    }
}
