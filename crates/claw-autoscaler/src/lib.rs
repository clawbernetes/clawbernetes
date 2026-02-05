//! Intelligent GPU cluster autoscaling for Clawbernetes.
//!
//! `claw-autoscaler` provides automatic scaling of GPU node pools based on
//! various signals including utilization, queue depth, and time-based schedules.
//!
//! # Features
//!
//! - **Target Utilization**: Scale based on GPU utilization percentage
//! - **Queue Depth**: Scale based on job queue depth per node
//! - **Schedule-based**: Scale based on time-of-day and day-of-week
//! - **Combined Policies**: Combine multiple signals with different strategies
//! - **Cooldown Protection**: Prevent thrashing with configurable cooldowns
//! - **Node Pool Management**: Group similar nodes with shared scaling configs
//!
//! # Example
//!
//! ```rust
//! use claw_autoscaler::{
//!     Autoscaler, AutoscalerManager, InMemoryMetricsProvider,
//!     MetricsSnapshot, NodeInfo, NodePool, PoolId, ScaleDirection,
//!     ScalingPolicy, ScalingPolicyType,
//! };
//!
//! // Create a metrics provider
//! let provider = InMemoryMetricsProvider::new();
//!
//! // Create a scaling policy
//! let policy = ScalingPolicy::builder("gpu-policy", "GPU Utilization Policy")
//!     .min_nodes(2)
//!     .max_nodes(20)
//!     .target_utilization(70.0, 10.0)
//!     .build()
//!     .unwrap();
//!
//! // Create a node pool
//! let mut pool = NodePool::new("gpu-pool", "GPU Pool", policy).unwrap();
//! pool.add_node(NodeInfo::new("node-1", "A100", 8));
//! pool.add_node(NodeInfo::new("node-2", "A100", 8));
//!
//! // Set current metrics
//! provider.set_metrics(
//!     &pool.id,
//!     MetricsSnapshot::new().with_gpu_utilization(85.0, 80.0, 90.0),
//! );
//!
//! // Create the autoscaler and evaluate
//! let autoscaler = Autoscaler::new(provider);
//! let recommendation = autoscaler.evaluate(&pool).unwrap();
//!
//! match recommendation.direction {
//!     ScaleDirection::Up => println!("Scale up to {} nodes", recommendation.target_nodes),
//!     ScaleDirection::Down => println!("Scale down to {} nodes", recommendation.target_nodes),
//!     ScaleDirection::None => println!("No scaling needed"),
//! }
//! ```
//!
//! # Manager Usage
//!
//! For managing multiple pools, use the `AutoscalerManager`:
//!
//! ```rust
//! use claw_autoscaler::{
//!     AutoscalerManager, InMemoryMetricsProvider,
//!     MetricsSnapshot, NodeInfo, NodePool, PoolId,
//!     ScalingPolicy,
//! };
//!
//! let provider = InMemoryMetricsProvider::new();
//! let manager = AutoscalerManager::new(provider);
//!
//! // Register pools
//! let policy = ScalingPolicy::builder("p1", "Policy 1")
//!     .min_nodes(1)
//!     .max_nodes(10)
//!     .target_utilization(70.0, 10.0)
//!     .build()
//!     .unwrap();
//!
//! let pool = NodePool::new("pool-1", "Pool 1", policy).unwrap();
//! manager.register_pool(pool).unwrap();
//!
//! // Evaluate all pools
//! // let actions = manager.evaluate().unwrap();
//! // for action in actions {
//! //     println!("Pool {}: scale {:?}", action.pool_id, action.recommendation.direction);
//! // }
//! ```
//!
//! # Policy Types
//!
//! ## Target Utilization
//!
//! Scales based on average GPU utilization with a tolerance band:
//!
//! ```rust
//! use claw_autoscaler::ScalingPolicy;
//!
//! let policy = ScalingPolicy::builder("util", "Utilization")
//!     .min_nodes(2)
//!     .max_nodes(20)
//!     .target_utilization(70.0, 10.0) // 70% ± 10%
//!     .build()
//!     .unwrap();
//! ```
//!
//! ## Queue Depth
//!
//! Scales based on job queue depth per node:
//!
//! ```rust
//! use claw_autoscaler::ScalingPolicy;
//!
//! let policy = ScalingPolicy::builder("queue", "Queue")
//!     .min_nodes(1)
//!     .max_nodes(50)
//!     .queue_depth(
//!         5,   // target jobs per node
//!         20,  // scale up threshold
//!         2,   // scale down threshold
//!     )
//!     .build()
//!     .unwrap();
//! ```
//!
//! ## Schedule-based
//!
//! Scales based on time schedules:
//!
//! ```rust
//! use claw_autoscaler::{ScalingPolicy, ScalingPolicyType, ScheduleRule};
//!
//! let business_hours = ScheduleRule::new(
//!     "business-hours",
//!     vec![1, 2, 3, 4, 5], // Monday-Friday
//!     9, 17,               // 9 AM to 5 PM UTC
//!     10,                  // 10 nodes
//! ).unwrap();
//!
//! let policy = ScalingPolicy::builder("schedule", "Schedule")
//!     .min_nodes(2)
//!     .max_nodes(20)
//!     .policy_type(ScalingPolicyType::Schedule {
//!         rules: vec![business_hours],
//!     })
//!     .build()
//!     .unwrap();
//! ```

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![deny(clippy::todo)]
#![deny(clippy::unimplemented)]

pub mod autoscaler;
pub mod error;
pub mod manager;
pub mod types;

// Re-export main types
pub use autoscaler::{Autoscaler, AutoscalerConfig, InMemoryMetricsProvider, MetricsProvider};
pub use error::{AutoscalerError, Result};
pub use manager::{ActionStatus, AutoscalerManager, AutoscalerStatus, ScaleAction};
pub use types::{
    CombinationStrategy, MetricsSnapshot, NodeId, NodeInfo, NodePool, NodeStatus, PoolId,
    ScaleDirection, ScaleRecommendation, ScalingPolicy, ScalingPolicyBuilder, ScalingPolicyType,
    ScheduleRule,
};

/// Prelude for convenient imports.
pub mod prelude {
    pub use crate::autoscaler::{Autoscaler, AutoscalerConfig, MetricsProvider};
    pub use crate::error::{AutoscalerError, Result};
    pub use crate::manager::{AutoscalerManager, AutoscalerStatus, ScaleAction};
    pub use crate::types::{
        MetricsSnapshot, NodeInfo, NodePool, PoolId, ScaleDirection, ScaleRecommendation,
        ScalingPolicy, ScalingPolicyType, ScheduleRule,
    };
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn full_scaling_workflow() {
        // Set up the manager with metrics provider
        let provider = InMemoryMetricsProvider::new();
        let manager = AutoscalerManager::new(provider);

        // Create and register a pool with utilization-based scaling
        let policy = ScalingPolicy::builder("gpu-policy", "GPU Utilization Policy")
            .min_nodes(2)
            .max_nodes(20)
            .target_utilization(70.0, 10.0)
            .build()
            .unwrap();

        let mut pool = NodePool::new("gpu-pool", "GPU Pool", policy).unwrap();
        pool.add_node(NodeInfo::new("node-1", "A100", 8));
        pool.add_node(NodeInfo::new("node-2", "A100", 8));
        pool.add_node(NodeInfo::new("node-3", "A100", 8));

        let pool_id = pool.id.clone();
        manager.register_pool(pool).unwrap();

        // Simulate high utilization
        let provider = InMemoryMetricsProvider::new();
        provider.set_metrics(
            &pool_id,
            MetricsSnapshot::new().with_gpu_utilization(95.0, 90.0, 100.0),
        );

        // Re-create manager with the provider that has metrics
        let manager = AutoscalerManager::new(provider);
        let mut pool = NodePool::new(
            "gpu-pool",
            "GPU Pool",
            ScalingPolicy::builder("gpu-policy", "GPU Utilization Policy")
                .min_nodes(2)
                .max_nodes(20)
                .target_utilization(70.0, 10.0)
                .build()
                .unwrap(),
        )
        .unwrap();
        pool.add_node(NodeInfo::new("node-1", "A100", 8));
        pool.add_node(NodeInfo::new("node-2", "A100", 8));
        pool.add_node(NodeInfo::new("node-3", "A100", 8));
        manager.register_pool(pool).unwrap();

        // Evaluate and get actions
        let actions = manager.evaluate().unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].recommendation.direction, ScaleDirection::Up);

        // Verify status
        let status = manager.status();
        assert!(status.enabled);
        assert_eq!(status.pool_count, 1);
        assert_eq!(status.pending_actions, 1);

        // Simulate completing the action
        manager.complete_action(&pool_id);

        let pending = manager.pending_actions();
        assert_eq!(pending[0].status, ActionStatus::Completed);

        // Clear finished actions
        manager.clear_finished_actions();
        assert!(manager.pending_actions().is_empty());
    }

    #[test]
    fn multi_pool_scaling() {
        let provider = InMemoryMetricsProvider::new();

        // Set up three pools
        let pool1_id = PoolId::new("pool-1");
        let pool2_id = PoolId::new("pool-2");
        let pool3_id = PoolId::new("pool-3");

        // Pool 1: High utilization (should scale up)
        provider.set_metrics(
            &pool1_id,
            MetricsSnapshot::new().with_gpu_utilization(95.0, 90.0, 100.0),
        );

        // Pool 2: Normal utilization (no scaling)
        provider.set_metrics(
            &pool2_id,
            MetricsSnapshot::new().with_gpu_utilization(70.0, 65.0, 75.0),
        );

        // Pool 3: Low utilization (should scale down)
        provider.set_metrics(
            &pool3_id,
            MetricsSnapshot::new().with_gpu_utilization(30.0, 25.0, 35.0),
        );

        let manager = AutoscalerManager::new(provider);

        // Create and register pools
        for (id, name) in [
            ("pool-1", "Pool 1"),
            ("pool-2", "Pool 2"),
            ("pool-3", "Pool 3"),
        ] {
            let policy = ScalingPolicy::builder(format!("{id}-policy"), format!("{name} Policy"))
                .min_nodes(2)
                .max_nodes(20)
                .target_utilization(70.0, 10.0)
                .build()
                .unwrap();

            let mut pool = NodePool::new(id, name, policy).unwrap();
            for i in 1..=5 {
                pool.add_node(NodeInfo::new(format!("{id}-node-{i}"), "A100", 8));
            }
            manager.register_pool(pool).unwrap();
        }

        // Evaluate all pools
        let actions = manager.evaluate().unwrap();

        // Should have 2 actions (pool-1 up, pool-3 down)
        assert_eq!(actions.len(), 2);

        let pool1_action = actions.iter().find(|a| a.pool_id == pool1_id);
        let pool3_action = actions.iter().find(|a| a.pool_id == pool3_id);

        assert!(pool1_action.is_some());
        assert_eq!(
            pool1_action.unwrap().recommendation.direction,
            ScaleDirection::Up
        );

        assert!(pool3_action.is_some());
        assert_eq!(
            pool3_action.unwrap().recommendation.direction,
            ScaleDirection::Down
        );

        // Pool 2 should not have an action
        assert!(actions.iter().find(|a| a.pool_id == pool2_id).is_none());
    }

    #[test]
    fn schedule_based_scaling_workflow() {
        let provider = InMemoryMetricsProvider::new();

        // Create a schedule-based policy
        let weekday_rule = ScheduleRule::new("weekday-peak", vec![1, 2, 3, 4, 5], 9, 17, 10).unwrap();

        // 0 to 23 covers all hours on weekends
        let weekend_rule = ScheduleRule::new("weekend-low", vec![0, 6], 0, 23, 2).unwrap();

        let policy = ScalingPolicy::builder("schedule-policy", "Schedule Policy")
            .min_nodes(2)
            .max_nodes(20)
            .policy_type(ScalingPolicyType::Schedule {
                rules: vec![weekday_rule, weekend_rule],
            })
            .build()
            .unwrap();

        let mut pool = NodePool::new("schedule-pool", "Schedule Pool", policy).unwrap();
        pool.add_node(NodeInfo::new("node-1", "A100", 8));
        pool.add_node(NodeInfo::new("node-2", "A100", 8));
        pool.add_node(NodeInfo::new("node-3", "A100", 8));

        let pool_id = pool.id.clone();

        provider.set_metrics(&pool_id, MetricsSnapshot::new());

        // Use a config with larger max_scale_delta for schedule-based tests
        let config = AutoscalerConfig {
            max_scale_delta: 10,
            ..Default::default()
        };
        let autoscaler = Autoscaler::with_config(provider, config);

        // Test Monday at 10 AM (should scale to 10 nodes)
        let monday_10am = chrono::DateTime::parse_from_rfc3339("2024-01-15T10:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let rec = autoscaler.evaluate_at(&pool, monday_10am).unwrap();
        assert_eq!(rec.direction, ScaleDirection::Up);
        assert_eq!(rec.target_nodes, 10);

        // Test Saturday at noon (should scale to 2 nodes)
        let saturday_noon = chrono::DateTime::parse_from_rfc3339("2024-01-13T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let rec = autoscaler.evaluate_at(&pool, saturday_noon).unwrap();
        assert_eq!(rec.direction, ScaleDirection::Down);
        assert_eq!(rec.target_nodes, 2);
    }

    #[test]
    fn combined_policy_scaling() {
        let provider = InMemoryMetricsProvider::new();

        // Create a combined policy
        let policy = ScalingPolicy::builder("combined-policy", "Combined Policy")
            .min_nodes(2)
            .max_nodes(30)
            .policy_type(ScalingPolicyType::Combined {
                policies: vec![
                    ScalingPolicyType::TargetUtilization {
                        target_percent: 70.0,
                        tolerance_percent: 10.0,
                    },
                    ScalingPolicyType::QueueDepth {
                        target_jobs_per_node: 5,
                        scale_up_threshold: 15,
                        scale_down_threshold: 2,
                    },
                ],
                combination: CombinationStrategy::Any,
            })
            .build()
            .unwrap();

        let mut pool = NodePool::new("combined-pool", "Combined Pool", policy).unwrap();
        for i in 1..=5 {
            pool.add_node(NodeInfo::new(format!("node-{i}"), "A100", 8));
        }

        let pool_id = pool.id.clone();

        // Test case: Normal utilization but high queue (queue policy triggers)
        provider.set_metrics(
            &pool_id,
            MetricsSnapshot::new()
                .with_gpu_utilization(70.0, 65.0, 75.0)
                .with_queue_depth(100),
        );

        let autoscaler = Autoscaler::new(provider);
        let rec = autoscaler.evaluate(&pool).unwrap();

        assert_eq!(rec.direction, ScaleDirection::Up);
    }

    #[test]
    fn node_pool_operations() {
        let provider = InMemoryMetricsProvider::new();
        let manager = AutoscalerManager::new(provider);

        let policy = ScalingPolicy::builder("ops-policy", "Ops Policy")
            .min_nodes(1)
            .max_nodes(10)
            .target_utilization(70.0, 10.0)
            .build()
            .unwrap();

        let pool = NodePool::new("ops-pool", "Ops Pool", policy).unwrap();
        let pool_id = pool.id.clone();
        manager.register_pool(pool).unwrap();

        // Add nodes dynamically
        for i in 1..=5 {
            manager
                .add_node(&pool_id, NodeInfo::new(format!("node-{i}"), "A100", 8))
                .unwrap();
        }

        let pool = manager.get_pool(&pool_id).unwrap();
        assert_eq!(pool.node_count(), 5);
        assert_eq!(pool.total_gpu_count(), 40);

        // Remove a node
        manager
            .remove_node(&pool_id, &NodeId::new("node-3"))
            .unwrap();

        let pool = manager.get_pool(&pool_id).unwrap();
        assert_eq!(pool.node_count(), 4);

        // Update policy
        let new_policy = ScalingPolicy::builder("new-ops-policy", "New Ops Policy")
            .min_nodes(2)
            .max_nodes(20)
            .target_utilization(80.0, 5.0)
            .build()
            .unwrap();

        manager.set_policy(&pool_id, new_policy).unwrap();

        let pool = manager.get_pool(&pool_id).unwrap();
        assert_eq!(pool.policy.min_nodes, 2);
        assert_eq!(pool.policy.max_nodes, 20);
    }
}
