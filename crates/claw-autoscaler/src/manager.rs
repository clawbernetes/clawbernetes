//! Autoscaler manager for orchestrating multiple node pools.
//!
//! This module provides a centralized manager for handling scaling across
//! multiple node pools with different policies.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use tracing::{debug, info, warn};

use crate::autoscaler::{Autoscaler, AutoscalerConfig, MetricsProvider};
use crate::error::{AutoscalerError, Result};
use crate::types::{
    NodeId, NodeInfo, NodePool, PoolId, ScaleDirection, ScaleRecommendation, ScalingPolicy,
};

/// Status of the autoscaler manager.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AutoscalerStatus {
    /// Whether the autoscaler is enabled.
    pub enabled: bool,
    /// Number of managed pools.
    pub pool_count: usize,
    /// Total number of nodes across all pools.
    pub total_nodes: u32,
    /// Total number of GPUs across all pools.
    pub total_gpus: u32,
    /// Last evaluation time.
    pub last_evaluation: Option<DateTime<Utc>>,
    /// Number of pending scale actions.
    pub pending_actions: usize,
}

/// A pending scaling action to be executed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScaleAction {
    /// Pool to scale.
    pub pool_id: PoolId,
    /// Scaling recommendation.
    pub recommendation: ScaleRecommendation,
    /// When this action was created.
    pub created_at: DateTime<Utc>,
    /// Action status.
    pub status: ActionStatus,
}

/// Status of a scaling action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum ActionStatus {
    /// Action is pending execution.
    #[default]
    Pending,
    /// Action is in progress.
    InProgress,
    /// Action completed successfully.
    Completed,
    /// Action failed.
    Failed,
    /// Action was cancelled.
    Cancelled,
}

/// Manager for autoscaling multiple node pools.
pub struct AutoscalerManager<M: MetricsProvider> {
    autoscaler: Autoscaler<M>,
    pools: RwLock<HashMap<PoolId, NodePool>>,
    pending_actions: RwLock<Vec<ScaleAction>>,
    enabled: RwLock<bool>,
    last_evaluation: RwLock<Option<DateTime<Utc>>>,
}

impl<M: MetricsProvider> AutoscalerManager<M> {
    /// Creates a new autoscaler manager.
    #[must_use]
    pub fn new(metrics_provider: M) -> Self {
        Self {
            autoscaler: Autoscaler::new(metrics_provider),
            pools: RwLock::new(HashMap::new()),
            pending_actions: RwLock::new(Vec::new()),
            enabled: RwLock::new(true),
            last_evaluation: RwLock::new(None),
        }
    }

    /// Creates a new autoscaler manager with custom configuration.
    #[must_use]
    pub fn with_config(metrics_provider: M, config: AutoscalerConfig) -> Self {
        Self {
            autoscaler: Autoscaler::with_config(metrics_provider, config),
            pools: RwLock::new(HashMap::new()),
            pending_actions: RwLock::new(Vec::new()),
            enabled: RwLock::new(true),
            last_evaluation: RwLock::new(None),
        }
    }

    /// Enables or disables the autoscaler.
    pub fn set_enabled(&self, enabled: bool) {
        *self.enabled.write() = enabled;
        info!(enabled, "autoscaler enabled state changed");
    }

    /// Checks if the autoscaler is enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        *self.enabled.read()
    }

    /// Registers a new node pool.
    ///
    /// # Errors
    ///
    /// Returns error if a pool with the same ID already exists.
    pub fn register_pool(&self, pool: NodePool) -> Result<()> {
        let mut pools = self.pools.write();
        if pools.contains_key(&pool.id) {
            return Err(AutoscalerError::InvalidNodePool {
                reason: format!("pool {} already exists", pool.id),
            });
        }

        info!(pool = %pool.id, "registered node pool");
        pools.insert(pool.id.clone(), pool);
        Ok(())
    }

    /// Unregisters a node pool.
    ///
    /// # Errors
    ///
    /// Returns error if the pool does not exist.
    pub fn unregister_pool(&self, pool_id: &PoolId) -> Result<NodePool> {
        let mut pools = self.pools.write();
        pools.remove(pool_id).ok_or_else(|| AutoscalerError::PoolNotFound {
            pool_id: pool_id.to_string(),
        })
    }

    /// Gets a pool by ID.
    #[must_use]
    pub fn get_pool(&self, pool_id: &PoolId) -> Option<NodePool> {
        self.pools.read().get(pool_id).cloned()
    }

    /// Lists all registered pools.
    #[must_use]
    pub fn list_pools(&self) -> Vec<NodePool> {
        self.pools.read().values().cloned().collect()
    }

    /// Adds a node to a pool.
    ///
    /// # Errors
    ///
    /// Returns error if the pool does not exist.
    pub fn add_node(&self, pool_id: &PoolId, node: NodeInfo) -> Result<()> {
        let mut pools = self.pools.write();
        let pool = pools.get_mut(pool_id).ok_or_else(|| AutoscalerError::PoolNotFound {
            pool_id: pool_id.to_string(),
        })?;

        info!(pool = %pool_id, node = %node.id, "added node to pool");
        pool.add_node(node);
        Ok(())
    }

    /// Removes a node from a pool.
    ///
    /// # Errors
    ///
    /// Returns error if the pool or node does not exist.
    pub fn remove_node(&self, pool_id: &PoolId, node_id: &NodeId) -> Result<NodeInfo> {
        let mut pools = self.pools.write();
        let pool = pools.get_mut(pool_id).ok_or_else(|| AutoscalerError::PoolNotFound {
            pool_id: pool_id.to_string(),
        })?;

        pool.remove_node(node_id)
            .ok_or_else(|| AutoscalerError::NodeNotFound {
                node_id: node_id.to_string(),
            })
    }

    /// Updates the scaling policy for a pool.
    ///
    /// # Errors
    ///
    /// Returns error if the pool does not exist or the policy is invalid.
    pub fn set_policy(&self, pool_id: &PoolId, policy: ScalingPolicy) -> Result<()> {
        let mut pools = self.pools.write();
        let pool = pools.get_mut(pool_id).ok_or_else(|| AutoscalerError::PoolNotFound {
            pool_id: pool_id.to_string(),
        })?;

        pool.set_policy(policy)?;
        info!(pool = %pool_id, "updated scaling policy");
        Ok(())
    }

    /// Evaluates all pools and generates scaling recommendations.
    ///
    /// # Errors
    ///
    /// Returns error if evaluation fails for any pool.
    pub fn evaluate(&self) -> Result<Vec<ScaleAction>> {
        self.evaluate_at(Utc::now())
    }

    /// Evaluates all pools at a specific time.
    ///
    /// # Errors
    ///
    /// Returns error if evaluation fails for any pool.
    pub fn evaluate_at(&self, now: DateTime<Utc>) -> Result<Vec<ScaleAction>> {
        if !self.is_enabled() {
            debug!("autoscaler is disabled, skipping evaluation");
            return Ok(Vec::new());
        }

        let pools: Vec<NodePool> = self.pools.read().values().cloned().collect();
        let mut actions = Vec::new();

        for pool in pools {
            match self.autoscaler.evaluate_at(&pool, now) {
                Ok(recommendation) if recommendation.direction != ScaleDirection::None => {
                    info!(
                        pool = %pool.id,
                        direction = ?recommendation.direction,
                        current = recommendation.current_nodes,
                        target = recommendation.target_nodes,
                        "scaling recommended"
                    );

                    let action = ScaleAction {
                        pool_id: pool.id.clone(),
                        recommendation,
                        created_at: now,
                        status: ActionStatus::Pending,
                    };
                    actions.push(action);
                }
                Ok(_) => {
                    debug!(pool = %pool.id, "no scaling needed");
                }
                Err(e) => {
                    warn!(pool = %pool.id, error = %e, "failed to evaluate pool");
                }
            }
        }

        // Store pending actions
        {
            let mut pending = self.pending_actions.write();
            pending.extend(actions.clone());
        }

        // Update last evaluation time
        *self.last_evaluation.write() = Some(now);

        Ok(actions)
    }

    /// Gets all pending scaling actions.
    #[must_use]
    pub fn pending_actions(&self) -> Vec<ScaleAction> {
        self.pending_actions.read().clone()
    }

    /// Marks an action as completed.
    pub fn complete_action(&self, pool_id: &PoolId) {
        let mut pending = self.pending_actions.write();
        if let Some(action) = pending.iter_mut().find(|a| &a.pool_id == pool_id) {
            action.status = ActionStatus::Completed;
        }

        // Also update the pool's last scale time
        let mut pools = self.pools.write();
        if let Some(pool) = pools.get_mut(pool_id) {
            let now = Utc::now();
            // Determine direction from the action
            if let Some(action) = pending.iter().find(|a| &a.pool_id == pool_id) {
                match action.recommendation.direction {
                    ScaleDirection::Up => pool.record_scale_up(now),
                    ScaleDirection::Down => pool.record_scale_down(now),
                    ScaleDirection::None => {}
                }
            }
        }
    }

    /// Marks an action as failed.
    pub fn fail_action(&self, pool_id: &PoolId) {
        let mut pending = self.pending_actions.write();
        if let Some(action) = pending.iter_mut().find(|a| &a.pool_id == pool_id) {
            action.status = ActionStatus::Failed;
        }
    }

    /// Cancels an action.
    pub fn cancel_action(&self, pool_id: &PoolId) {
        let mut pending = self.pending_actions.write();
        if let Some(action) = pending.iter_mut().find(|a| &a.pool_id == pool_id) {
            action.status = ActionStatus::Cancelled;
        }
    }

    /// Clears completed/failed/cancelled actions.
    pub fn clear_finished_actions(&self) {
        let mut pending = self.pending_actions.write();
        pending.retain(|a| a.status == ActionStatus::Pending || a.status == ActionStatus::InProgress);
    }

    /// Gets the current status of the autoscaler.
    #[must_use]
    pub fn status(&self) -> AutoscalerStatus {
        let pools = self.pools.read();
        let pending = self.pending_actions.read();

        let total_nodes: u32 = pools.values().map(|p| p.node_count()).sum();
        let total_gpus: u32 = pools.values().map(|p| p.total_gpu_count()).sum();
        let pending_count = pending
            .iter()
            .filter(|a| a.status == ActionStatus::Pending || a.status == ActionStatus::InProgress)
            .count();

        AutoscalerStatus {
            enabled: *self.enabled.read(),
            pool_count: pools.len(),
            total_nodes,
            total_gpus,
            last_evaluation: *self.last_evaluation.read(),
            pending_actions: pending_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoscaler::InMemoryMetricsProvider;
    use crate::types::{MetricsSnapshot, ScalingPolicy};

    fn create_test_pool(id: &str, name: &str) -> NodePool {
        let policy = ScalingPolicy::builder(format!("{id}-policy"), format!("{name} Policy"))
            .min_nodes(1)
            .max_nodes(20)
            .target_utilization(70.0, 10.0)
            .build()
            .unwrap();

        let mut pool = NodePool::new(id, name, policy).unwrap();
        pool.add_node(NodeInfo::new("node-1", "A100", 8));
        pool.add_node(NodeInfo::new("node-2", "A100", 8));
        pool
    }

    #[test]
    fn register_and_list_pools() {
        let provider = InMemoryMetricsProvider::new();
        let manager = AutoscalerManager::new(provider);

        let pool1 = create_test_pool("pool-1", "Pool 1");
        let pool2 = create_test_pool("pool-2", "Pool 2");

        assert!(manager.register_pool(pool1).is_ok());
        assert!(manager.register_pool(pool2).is_ok());

        let pools = manager.list_pools();
        assert_eq!(pools.len(), 2);
    }

    #[test]
    fn register_duplicate_pool_fails() {
        let provider = InMemoryMetricsProvider::new();
        let manager = AutoscalerManager::new(provider);

        let pool1 = create_test_pool("pool-1", "Pool 1");
        let pool2 = create_test_pool("pool-1", "Pool 1 Duplicate");

        assert!(manager.register_pool(pool1).is_ok());
        assert!(manager.register_pool(pool2).is_err());
    }

    #[test]
    fn unregister_pool() {
        let provider = InMemoryMetricsProvider::new();
        let manager = AutoscalerManager::new(provider);

        let pool = create_test_pool("pool-1", "Pool 1");
        assert!(manager.register_pool(pool).is_ok());

        let removed = manager.unregister_pool(&PoolId::new("pool-1"));
        assert!(removed.is_ok());
        assert!(manager.get_pool(&PoolId::new("pool-1")).is_none());
    }

    #[test]
    fn unregister_nonexistent_pool_fails() {
        let provider = InMemoryMetricsProvider::new();
        let manager = AutoscalerManager::new(provider);

        let result = manager.unregister_pool(&PoolId::new("nonexistent"));
        assert!(result.is_err());
    }

    #[test]
    fn add_and_remove_nodes() {
        let provider = InMemoryMetricsProvider::new();
        let manager = AutoscalerManager::new(provider);

        let pool = create_test_pool("pool-1", "Pool 1");
        assert!(manager.register_pool(pool).is_ok());

        let pool_id = PoolId::new("pool-1");

        // Add a node
        let node = NodeInfo::new("node-3", "A100", 8);
        assert!(manager.add_node(&pool_id, node).is_ok());

        let pool = manager.get_pool(&pool_id).unwrap();
        assert_eq!(pool.node_count(), 3);

        // Remove a node
        let removed = manager.remove_node(&pool_id, &NodeId::new("node-3"));
        assert!(removed.is_ok());

        let pool = manager.get_pool(&pool_id).unwrap();
        assert_eq!(pool.node_count(), 2);
    }

    #[test]
    fn add_node_to_nonexistent_pool_fails() {
        let provider = InMemoryMetricsProvider::new();
        let manager = AutoscalerManager::new(provider);

        let pool_id = PoolId::new("nonexistent");
        let node = NodeInfo::new("node-1", "A100", 8);

        let result = manager.add_node(&pool_id, node);
        assert!(result.is_err());
    }

    #[test]
    fn set_policy() {
        let provider = InMemoryMetricsProvider::new();
        let manager = AutoscalerManager::new(provider);

        let pool = create_test_pool("pool-1", "Pool 1");
        assert!(manager.register_pool(pool).is_ok());

        let pool_id = PoolId::new("pool-1");

        let new_policy = ScalingPolicy::builder("new-policy", "New Policy")
            .min_nodes(5)
            .max_nodes(50)
            .target_utilization(80.0, 5.0)
            .build()
            .unwrap();

        assert!(manager.set_policy(&pool_id, new_policy).is_ok());

        let pool = manager.get_pool(&pool_id).unwrap();
        assert_eq!(pool.policy.min_nodes, 5);
        assert_eq!(pool.policy.max_nodes, 50);
    }

    #[test]
    fn evaluate_generates_actions() {
        let provider = InMemoryMetricsProvider::new();

        let pool = create_test_pool("pool-1", "Pool 1");
        let pool_id = pool.id.clone();

        // Set high utilization to trigger scale up
        provider.set_metrics(
            &pool_id,
            MetricsSnapshot::new().with_gpu_utilization(95.0, 90.0, 100.0),
        );

        let manager = AutoscalerManager::new(provider);
        assert!(manager.register_pool(pool).is_ok());

        let actions = manager.evaluate().unwrap();

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].pool_id, pool_id);
        assert_eq!(actions[0].recommendation.direction, ScaleDirection::Up);
        assert_eq!(actions[0].status, ActionStatus::Pending);
    }

    #[test]
    fn disabled_autoscaler_skips_evaluation() {
        let provider = InMemoryMetricsProvider::new();

        let pool = create_test_pool("pool-1", "Pool 1");
        let pool_id = pool.id.clone();

        provider.set_metrics(
            &pool_id,
            MetricsSnapshot::new().with_gpu_utilization(95.0, 90.0, 100.0),
        );

        let manager = AutoscalerManager::new(provider);
        assert!(manager.register_pool(pool).is_ok());

        manager.set_enabled(false);
        let actions = manager.evaluate().unwrap();

        assert!(actions.is_empty());
    }

    #[test]
    fn action_lifecycle() {
        let provider = InMemoryMetricsProvider::new();

        let pool = create_test_pool("pool-1", "Pool 1");
        let pool_id = pool.id.clone();

        provider.set_metrics(
            &pool_id,
            MetricsSnapshot::new().with_gpu_utilization(95.0, 90.0, 100.0),
        );

        let manager = AutoscalerManager::new(provider);
        assert!(manager.register_pool(pool).is_ok());

        // Generate action
        let actions = manager.evaluate().unwrap();
        assert_eq!(actions.len(), 1);

        // Check pending
        let pending = manager.pending_actions();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].status, ActionStatus::Pending);

        // Complete action
        manager.complete_action(&pool_id);
        let pending = manager.pending_actions();
        assert_eq!(pending[0].status, ActionStatus::Completed);

        // Clear finished
        manager.clear_finished_actions();
        let pending = manager.pending_actions();
        assert!(pending.is_empty());
    }

    #[test]
    fn fail_and_cancel_actions() {
        let provider = InMemoryMetricsProvider::new();

        let pool1 = create_test_pool("pool-1", "Pool 1");
        let pool2 = create_test_pool("pool-2", "Pool 2");
        let pool1_id = pool1.id.clone();
        let pool2_id = pool2.id.clone();

        provider.set_metrics(
            &pool1_id,
            MetricsSnapshot::new().with_gpu_utilization(95.0, 90.0, 100.0),
        );
        provider.set_metrics(
            &pool2_id,
            MetricsSnapshot::new().with_gpu_utilization(95.0, 90.0, 100.0),
        );

        let manager = AutoscalerManager::new(provider);
        assert!(manager.register_pool(pool1).is_ok());
        assert!(manager.register_pool(pool2).is_ok());

        manager.evaluate().unwrap();

        manager.fail_action(&pool1_id);
        manager.cancel_action(&pool2_id);

        let pending = manager.pending_actions();
        let pool1_action = pending.iter().find(|a| a.pool_id == pool1_id).unwrap();
        let pool2_action = pending.iter().find(|a| a.pool_id == pool2_id).unwrap();

        assert_eq!(pool1_action.status, ActionStatus::Failed);
        assert_eq!(pool2_action.status, ActionStatus::Cancelled);
    }

    #[test]
    fn status_reflects_state() {
        let provider = InMemoryMetricsProvider::new();

        let pool1 = create_test_pool("pool-1", "Pool 1");
        let pool2 = create_test_pool("pool-2", "Pool 2");
        let pool1_id = pool1.id.clone();
        let pool2_id = pool2.id.clone();

        // Pool 1: high utilization (will scale)
        provider.set_metrics(
            &pool1_id,
            MetricsSnapshot::new().with_gpu_utilization(95.0, 90.0, 100.0),
        );
        // Pool 2: normal utilization (won't scale)
        provider.set_metrics(
            &pool2_id,
            MetricsSnapshot::new().with_gpu_utilization(70.0, 65.0, 75.0),
        );

        let manager = AutoscalerManager::new(provider);
        assert!(manager.register_pool(pool1).is_ok());
        assert!(manager.register_pool(pool2).is_ok());

        let status = manager.status();
        assert!(status.enabled);
        assert_eq!(status.pool_count, 2);
        assert_eq!(status.total_nodes, 4); // 2 nodes per pool
        assert_eq!(status.total_gpus, 32); // 8 GPUs per node * 4 nodes
        assert!(status.last_evaluation.is_none());
        assert_eq!(status.pending_actions, 0);

        manager.evaluate().unwrap();

        let status = manager.status();
        assert!(status.last_evaluation.is_some());
        assert_eq!(status.pending_actions, 1); // Only pool-1 scales
    }

    #[test]
    fn with_config() {
        let provider = InMemoryMetricsProvider::new();
        let config = AutoscalerConfig {
            min_confidence: 0.8,
            ignore_disabled_policies: false,
            max_scale_delta: 2,
        };

        let manager = AutoscalerManager::with_config(provider, config);
        assert!(manager.is_enabled());
    }

    mod action_status_tests {
        use super::*;

        #[test]
        fn action_status_default() {
            let status = ActionStatus::default();
            assert_eq!(status, ActionStatus::Pending);
        }

        #[test]
        fn action_status_serialization() {
            for status in [
                ActionStatus::Pending,
                ActionStatus::InProgress,
                ActionStatus::Completed,
                ActionStatus::Failed,
                ActionStatus::Cancelled,
            ] {
                let json = serde_json::to_string(&status);
                assert!(json.is_ok());
                let parsed: serde_json::Result<ActionStatus> = serde_json::from_str(&json.unwrap());
                assert!(parsed.is_ok());
                assert_eq!(parsed.unwrap(), status);
            }
        }
    }

    mod autoscaler_status_tests {
        use super::*;

        #[test]
        fn status_serialization() {
            let status = AutoscalerStatus {
                enabled: true,
                pool_count: 3,
                total_nodes: 15,
                total_gpus: 120,
                last_evaluation: Some(Utc::now()),
                pending_actions: 2,
            };

            let json = serde_json::to_string(&status);
            assert!(json.is_ok());
            let parsed: serde_json::Result<AutoscalerStatus> = serde_json::from_str(&json.unwrap());
            assert!(parsed.is_ok());
            let parsed = parsed.unwrap();
            assert_eq!(parsed.pool_count, 3);
            assert_eq!(parsed.total_gpus, 120);
        }
    }
}
