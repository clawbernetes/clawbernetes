//! Core autoscaler logic for GPU cluster scaling.
//!
//! This module implements the autoscaler that monitors metrics and generates
//! scaling recommendations based on configured policies.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use tracing::{debug, info, warn};

use crate::error::{AutoscalerError, Result};
use crate::types::{
    CombinationStrategy, MetricsSnapshot, NodePool, PoolId, ScaleDirection, ScaleRecommendation,
    ScalingPolicy, ScalingPolicyType, ScheduleRule,
};

/// Provides metrics for autoscaling decisions.
pub trait MetricsProvider: Send + Sync {
    /// Fetches current metrics for a node pool.
    ///
    /// # Errors
    ///
    /// Returns error if metrics cannot be retrieved.
    fn get_metrics(&self, pool_id: &PoolId) -> Result<MetricsSnapshot>;
}

/// Simple in-memory metrics provider for testing.
#[derive(Debug, Default)]
pub struct InMemoryMetricsProvider {
    metrics: parking_lot::RwLock<HashMap<String, MetricsSnapshot>>,
}

impl InMemoryMetricsProvider {
    /// Creates a new in-memory metrics provider.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets metrics for a pool.
    pub fn set_metrics(&self, pool_id: &PoolId, metrics: MetricsSnapshot) {
        self.metrics
            .write()
            .insert(pool_id.as_str().to_string(), metrics);
    }
}

impl MetricsProvider for InMemoryMetricsProvider {
    fn get_metrics(&self, pool_id: &PoolId) -> Result<MetricsSnapshot> {
        self.metrics
            .read()
            .get(pool_id.as_str())
            .cloned()
            .ok_or_else(|| AutoscalerError::MetricsError {
                message: format!("no metrics for pool {pool_id}"),
            })
    }
}

/// Configuration for the autoscaler.
#[derive(Debug, Clone)]
pub struct AutoscalerConfig {
    /// Minimum confidence threshold for recommendations.
    pub min_confidence: f64,
    /// Whether to allow scaling disabled pools.
    pub ignore_disabled_policies: bool,
    /// Maximum scale delta per evaluation (to prevent aggressive scaling).
    pub max_scale_delta: u32,
}

impl Default for AutoscalerConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.5,
            ignore_disabled_policies: true,
            max_scale_delta: 5,
        }
    }
}

/// The autoscaler that monitors metrics and recommends scaling actions.
pub struct Autoscaler<M: MetricsProvider> {
    metrics_provider: M,
    config: AutoscalerConfig,
}

impl<M: MetricsProvider> Autoscaler<M> {
    /// Creates a new autoscaler with the given metrics provider.
    #[must_use]
    pub fn new(metrics_provider: M) -> Self {
        Self {
            metrics_provider,
            config: AutoscalerConfig::default(),
        }
    }

    /// Creates a new autoscaler with custom configuration.
    #[must_use]
    pub fn with_config(metrics_provider: M, config: AutoscalerConfig) -> Self {
        Self {
            metrics_provider,
            config,
        }
    }

    /// Evaluates a node pool and returns a scaling recommendation.
    ///
    /// # Errors
    ///
    /// Returns error if metrics cannot be retrieved.
    pub fn evaluate(&self, pool: &NodePool) -> Result<ScaleRecommendation> {
        self.evaluate_at(pool, Utc::now())
    }

    /// Evaluates a node pool at a specific time (useful for testing schedule-based policies).
    ///
    /// # Errors
    ///
    /// Returns error if metrics cannot be retrieved.
    pub fn evaluate_at(&self, pool: &NodePool, now: DateTime<Utc>) -> Result<ScaleRecommendation> {
        // Check if policy is enabled
        if !pool.policy.enabled && self.config.ignore_disabled_policies {
            debug!(pool = %pool.id, "skipping disabled policy");
            return Ok(ScaleRecommendation::no_change(
                pool.node_count(),
                "policy is disabled",
            ));
        }

        // Get current metrics
        let metrics = self.metrics_provider.get_metrics(&pool.id)?;
        let current_nodes = pool.node_count();

        // Evaluate based on policy type
        let mut recommendation = self.evaluate_policy_type(
            &pool.policy,
            &pool.policy.policy_type,
            &metrics,
            current_nodes,
            &now,
        );

        // Apply constraints
        recommendation = self.apply_constraints(pool, recommendation, &now);

        // Apply max delta
        recommendation = self.apply_max_delta(recommendation);

        // Ensure within bounds
        recommendation = self.apply_bounds(&pool.policy, recommendation);

        info!(
            pool = %pool.id,
            direction = ?recommendation.direction,
            current = recommendation.current_nodes,
            target = recommendation.target_nodes,
            "scaling recommendation"
        );

        Ok(recommendation)
    }

    fn evaluate_policy_type(
        &self,
        policy: &ScalingPolicy,
        policy_type: &ScalingPolicyType,
        metrics: &MetricsSnapshot,
        current_nodes: u32,
        now: &DateTime<Utc>,
    ) -> ScaleRecommendation {
        match policy_type {
            ScalingPolicyType::TargetUtilization {
                target_percent,
                tolerance_percent,
            } => self.evaluate_utilization(
                metrics,
                current_nodes,
                *target_percent,
                *tolerance_percent,
            ),

            ScalingPolicyType::QueueDepth {
                target_jobs_per_node,
                scale_up_threshold,
                scale_down_threshold,
            } => self.evaluate_queue_depth(
                metrics,
                current_nodes,
                *target_jobs_per_node,
                *scale_up_threshold,
                *scale_down_threshold,
            ),

            ScalingPolicyType::Schedule { rules } => {
                self.evaluate_schedule(rules, current_nodes, now)
            }

            ScalingPolicyType::Combined {
                policies,
                combination,
            } => self.evaluate_combined(policy, policies, metrics, current_nodes, now, *combination),
        }
    }

    fn evaluate_utilization(
        &self,
        metrics: &MetricsSnapshot,
        current_nodes: u32,
        target_percent: f64,
        tolerance_percent: f64,
    ) -> ScaleRecommendation {
        let utilization = metrics.avg_gpu_utilization;
        let upper_bound = target_percent + tolerance_percent;
        let lower_bound = target_percent - tolerance_percent;

        if utilization > upper_bound {
            // Scale up: utilization is too high
            // Calculate how many nodes needed to bring utilization to target
            let current_capacity = current_nodes as f64;
            let needed_capacity = (current_capacity * utilization) / target_percent;
            let target_nodes = needed_capacity.ceil() as u32;

            ScaleRecommendation::scale_up(
                current_nodes,
                target_nodes,
                format!(
                    "GPU utilization {utilization:.1}% exceeds upper threshold {upper_bound:.1}%"
                ),
            )
            .with_metric("gpu_utilization", utilization)
            .with_confidence(self.calculate_utilization_confidence(utilization, upper_bound))
        } else if utilization < lower_bound && current_nodes > 1 {
            // Scale down: utilization is too low
            let current_capacity = current_nodes as f64;
            let needed_capacity = (current_capacity * utilization) / target_percent;
            let target_nodes = needed_capacity.ceil().max(1.0) as u32;

            ScaleRecommendation::scale_down(
                current_nodes,
                target_nodes,
                format!(
                    "GPU utilization {utilization:.1}% below lower threshold {lower_bound:.1}%"
                ),
            )
            .with_metric("gpu_utilization", utilization)
            .with_confidence(self.calculate_utilization_confidence(lower_bound, utilization))
        } else {
            ScaleRecommendation::no_change(
                current_nodes,
                format!("GPU utilization {utilization:.1}% within target range"),
            )
            .with_metric("gpu_utilization", utilization)
        }
    }

    fn calculate_utilization_confidence(&self, actual: f64, threshold: f64) -> f64 {
        // Confidence increases with distance from threshold
        let distance = (actual - threshold).abs();
        (distance / 20.0).min(1.0).max(0.5)
    }

    fn evaluate_queue_depth(
        &self,
        metrics: &MetricsSnapshot,
        current_nodes: u32,
        target_jobs_per_node: u32,
        scale_up_threshold: u32,
        scale_down_threshold: u32,
    ) -> ScaleRecommendation {
        let queue_depth = metrics.queue_depth;
        let jobs_per_node = if current_nodes > 0 {
            queue_depth / current_nodes
        } else {
            queue_depth
        };

        if jobs_per_node > scale_up_threshold {
            // Scale up to handle queue
            let target_nodes = (queue_depth / target_jobs_per_node).max(current_nodes + 1);

            ScaleRecommendation::scale_up(
                current_nodes,
                target_nodes,
                format!(
                    "queue depth {queue_depth} ({jobs_per_node} per node) exceeds threshold {scale_up_threshold}"
                ),
            )
            .with_metric("queue_depth", f64::from(queue_depth))
            .with_metric("jobs_per_node", f64::from(jobs_per_node))
        } else if jobs_per_node < scale_down_threshold && current_nodes > 1 {
            // Scale down
            let target_nodes = (queue_depth / target_jobs_per_node).max(1);

            ScaleRecommendation::scale_down(
                current_nodes,
                target_nodes,
                format!(
                    "queue depth {queue_depth} ({jobs_per_node} per node) below threshold {scale_down_threshold}"
                ),
            )
            .with_metric("queue_depth", f64::from(queue_depth))
            .with_metric("jobs_per_node", f64::from(jobs_per_node))
        } else {
            ScaleRecommendation::no_change(
                current_nodes,
                format!("queue depth {queue_depth} within acceptable range"),
            )
            .with_metric("queue_depth", f64::from(queue_depth))
        }
    }

    fn evaluate_schedule(
        &self,
        rules: &[ScheduleRule],
        current_nodes: u32,
        now: &DateTime<Utc>,
    ) -> ScaleRecommendation {
        // Find the first applicable rule
        for rule in rules {
            if rule.applies_at(now) {
                let target_nodes = rule.desired_nodes;

                return if target_nodes > current_nodes {
                    ScaleRecommendation::scale_up(
                        current_nodes,
                        target_nodes,
                        format!("schedule rule '{}' requires {} nodes", rule.name, target_nodes),
                    )
                    .with_confidence(1.0) // Schedule-based scaling is deterministic
                } else if target_nodes < current_nodes {
                    ScaleRecommendation::scale_down(
                        current_nodes,
                        target_nodes,
                        format!("schedule rule '{}' requires {} nodes", rule.name, target_nodes),
                    )
                    .with_confidence(1.0)
                } else {
                    ScaleRecommendation::no_change(
                        current_nodes,
                        format!("schedule rule '{}' satisfied", rule.name),
                    )
                };
            }
        }

        // No rule applies - maintain current state
        ScaleRecommendation::no_change(
            current_nodes,
            "no schedule rule applies at current time",
        )
    }

    fn evaluate_combined(
        &self,
        policy: &ScalingPolicy,
        policies: &[ScalingPolicyType],
        metrics: &MetricsSnapshot,
        current_nodes: u32,
        now: &DateTime<Utc>,
        combination: CombinationStrategy,
    ) -> ScaleRecommendation {
        let recommendations: Vec<ScaleRecommendation> = policies
            .iter()
            .map(|p| self.evaluate_policy_type(policy, p, metrics, current_nodes, now))
            .collect();

        if recommendations.is_empty() {
            return ScaleRecommendation::no_change(current_nodes, "no sub-policies");
        }

        match combination {
            CombinationStrategy::Any => {
                // Return the first non-None recommendation
                recommendations
                    .into_iter()
                    .find(|r| r.direction != ScaleDirection::None)
                    .unwrap_or_else(|| {
                        ScaleRecommendation::no_change(current_nodes, "no policy triggered")
                    })
            }

            CombinationStrategy::All => {
                // All must agree on direction
                let first_direction = recommendations
                    .first()
                    .map(|r| r.direction)
                    .unwrap_or(ScaleDirection::None);

                if recommendations.iter().all(|r| r.direction == first_direction) {
                    recommendations.into_iter().next().unwrap_or_else(|| {
                        ScaleRecommendation::no_change(current_nodes, "no recommendation")
                    })
                } else {
                    ScaleRecommendation::no_change(current_nodes, "policies disagree on direction")
                }
            }

            CombinationStrategy::MostAggressive => {
                // Return the recommendation with the largest absolute delta
                recommendations
                    .into_iter()
                    .max_by_key(|r| r.delta().unsigned_abs())
                    .unwrap_or_else(|| {
                        ScaleRecommendation::no_change(current_nodes, "no recommendation")
                    })
            }

            CombinationStrategy::MostConservative => {
                // Return the recommendation with the smallest absolute delta
                recommendations
                    .into_iter()
                    .min_by_key(|r| r.delta().unsigned_abs())
                    .unwrap_or_else(|| {
                        ScaleRecommendation::no_change(current_nodes, "no recommendation")
                    })
            }
        }
    }

    fn apply_constraints(
        &self,
        pool: &NodePool,
        recommendation: ScaleRecommendation,
        now: &DateTime<Utc>,
    ) -> ScaleRecommendation {
        match recommendation.direction {
            ScaleDirection::Up if !pool.can_scale_up(now) => {
                warn!(pool = %pool.id, "scale up blocked by cooldown");
                ScaleRecommendation::no_change(
                    recommendation.current_nodes,
                    "scale up cooldown active",
                )
            }
            ScaleDirection::Down if !pool.can_scale_down(now) => {
                warn!(pool = %pool.id, "scale down blocked by cooldown");
                ScaleRecommendation::no_change(
                    recommendation.current_nodes,
                    "scale down cooldown active",
                )
            }
            _ => recommendation,
        }
    }

    #[allow(clippy::cast_possible_wrap)]
    fn apply_max_delta(&self, mut recommendation: ScaleRecommendation) -> ScaleRecommendation {
        let delta = recommendation.delta();
        let max_delta = self.config.max_scale_delta as i32;

        if delta.abs() > max_delta {
            let capped_delta = if delta > 0 { max_delta } else { -max_delta };
            let new_target = (recommendation.current_nodes as i32 + capped_delta).max(1) as u32;

            recommendation.target_nodes = new_target;
            recommendation.reason = format!(
                "{} (capped from {} to {} delta)",
                recommendation.reason,
                delta,
                capped_delta
            );
        }

        recommendation
    }

    fn apply_bounds(
        &self,
        policy: &ScalingPolicy,
        mut recommendation: ScaleRecommendation,
    ) -> ScaleRecommendation {
        let original_target = recommendation.target_nodes;

        recommendation.target_nodes = recommendation
            .target_nodes
            .clamp(policy.min_nodes, policy.max_nodes);

        if recommendation.target_nodes != original_target {
            recommendation.reason = format!(
                "{} (bounded to [{}, {}])",
                recommendation.reason, policy.min_nodes, policy.max_nodes
            );
        }

        // Update direction based on final target
        recommendation.direction = if recommendation.target_nodes > recommendation.current_nodes {
            ScaleDirection::Up
        } else if recommendation.target_nodes < recommendation.current_nodes {
            ScaleDirection::Down
        } else {
            ScaleDirection::None
        };

        recommendation
    }

    /// Evaluates multiple pools and returns all recommendations.
    ///
    /// # Errors
    ///
    /// Returns error if any pool evaluation fails.
    pub fn evaluate_all(
        &self,
        pools: &[NodePool],
    ) -> Result<HashMap<PoolId, ScaleRecommendation>> {
        let mut results = HashMap::new();
        for pool in pools {
            let recommendation = self.evaluate(pool)?;
            results.insert(pool.id.clone(), recommendation);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{NodeInfo, ScalingPolicy};
    use std::time::Duration;

    fn create_test_pool_with_policy(policy: ScalingPolicy) -> NodePool {
        let mut pool = NodePool::new("test-pool", "Test Pool", policy).unwrap();
        pool.add_node(NodeInfo::new("node-1", "A100", 8));
        pool.add_node(NodeInfo::new("node-2", "A100", 8));
        pool.add_node(NodeInfo::new("node-3", "A100", 8));
        pool.add_node(NodeInfo::new("node-4", "A100", 8));
        pool.add_node(NodeInfo::new("node-5", "A100", 8));
        pool
    }

    fn create_utilization_policy() -> ScalingPolicy {
        ScalingPolicy::builder("util-policy", "Utilization Policy")
            .min_nodes(2)
            .max_nodes(20)
            .target_utilization(70.0, 10.0)
            .build()
            .unwrap()
    }

    mod utilization_tests {
        use super::*;

        #[test]
        fn high_utilization_scales_up() {
            let pool = create_test_pool_with_policy(create_utilization_policy());
            let provider = InMemoryMetricsProvider::new();
            provider.set_metrics(
                &pool.id,
                MetricsSnapshot::new().with_gpu_utilization(95.0, 90.0, 100.0),
            );

            let autoscaler = Autoscaler::new(provider);
            let recommendation = autoscaler.evaluate(&pool).unwrap();

            assert_eq!(recommendation.direction, ScaleDirection::Up);
            assert!(recommendation.target_nodes > 5);
        }

        #[test]
        fn low_utilization_scales_down() {
            let pool = create_test_pool_with_policy(create_utilization_policy());
            let provider = InMemoryMetricsProvider::new();
            provider.set_metrics(
                &pool.id,
                MetricsSnapshot::new().with_gpu_utilization(40.0, 30.0, 50.0),
            );

            let autoscaler = Autoscaler::new(provider);
            let recommendation = autoscaler.evaluate(&pool).unwrap();

            assert_eq!(recommendation.direction, ScaleDirection::Down);
            assert!(recommendation.target_nodes < 5);
        }

        #[test]
        fn normal_utilization_no_change() {
            let pool = create_test_pool_with_policy(create_utilization_policy());
            let provider = InMemoryMetricsProvider::new();
            provider.set_metrics(
                &pool.id,
                MetricsSnapshot::new().with_gpu_utilization(70.0, 65.0, 75.0),
            );

            let autoscaler = Autoscaler::new(provider);
            let recommendation = autoscaler.evaluate(&pool).unwrap();

            assert_eq!(recommendation.direction, ScaleDirection::None);
            assert_eq!(recommendation.target_nodes, 5);
        }

        #[test]
        fn utilization_within_tolerance_no_change() {
            let pool = create_test_pool_with_policy(create_utilization_policy());
            let provider = InMemoryMetricsProvider::new();

            // 75% is within the 70% ± 10% tolerance (60-80%)
            provider.set_metrics(
                &pool.id,
                MetricsSnapshot::new().with_gpu_utilization(75.0, 70.0, 80.0),
            );

            let autoscaler = Autoscaler::new(provider);
            let recommendation = autoscaler.evaluate(&pool).unwrap();

            assert_eq!(recommendation.direction, ScaleDirection::None);
        }
    }

    mod queue_depth_tests {
        use super::*;

        fn create_queue_policy() -> ScalingPolicy {
            ScalingPolicy::builder("queue-policy", "Queue Policy")
                .min_nodes(1)
                .max_nodes(50)
                .queue_depth(5, 20, 2)
                .build()
                .unwrap()
        }

        #[test]
        fn high_queue_scales_up() {
            let pool = create_test_pool_with_policy(create_queue_policy());
            let provider = InMemoryMetricsProvider::new();
            provider.set_metrics(&pool.id, MetricsSnapshot::new().with_queue_depth(150));

            let autoscaler = Autoscaler::new(provider);
            let recommendation = autoscaler.evaluate(&pool).unwrap();

            assert_eq!(recommendation.direction, ScaleDirection::Up);
        }

        #[test]
        fn low_queue_scales_down() {
            let pool = create_test_pool_with_policy(create_queue_policy());
            let provider = InMemoryMetricsProvider::new();
            provider.set_metrics(&pool.id, MetricsSnapshot::new().with_queue_depth(5));

            let autoscaler = Autoscaler::new(provider);
            let recommendation = autoscaler.evaluate(&pool).unwrap();

            assert_eq!(recommendation.direction, ScaleDirection::Down);
        }

        #[test]
        fn normal_queue_no_change() {
            let pool = create_test_pool_with_policy(create_queue_policy());
            let provider = InMemoryMetricsProvider::new();
            provider.set_metrics(&pool.id, MetricsSnapshot::new().with_queue_depth(50));

            let autoscaler = Autoscaler::new(provider);
            let recommendation = autoscaler.evaluate(&pool).unwrap();

            assert_eq!(recommendation.direction, ScaleDirection::None);
        }
    }

    mod schedule_tests {
        use super::*;
        use crate::types::ScheduleRule;

        fn create_schedule_policy() -> ScalingPolicy {
            let weekday_rule =
                ScheduleRule::new("weekday-high", vec![1, 2, 3, 4, 5], 9, 17, 10).unwrap();
            // Using overnight span (0 to 0 means all 24 hours - start==end is special case)
            let weekend_rule =
                ScheduleRule::new("weekend-low", vec![0, 6], 0, 23, 2).unwrap();

            ScalingPolicy::builder("schedule-policy", "Schedule Policy")
                .min_nodes(1)
                .max_nodes(20)
                .policy_type(ScalingPolicyType::Schedule {
                    rules: vec![weekday_rule, weekend_rule],
                })
                .build()
                .unwrap()
        }

        #[test]
        fn schedule_scales_up_during_business_hours() {
            let pool = create_test_pool_with_policy(create_schedule_policy());
            let provider = InMemoryMetricsProvider::new();
            provider.set_metrics(&pool.id, MetricsSnapshot::new());

            // Monday at 10:00 AM
            let now = chrono::DateTime::parse_from_rfc3339("2024-01-15T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc);

            let autoscaler = Autoscaler::new(provider);
            let recommendation = autoscaler.evaluate_at(&pool, now).unwrap();

            assert_eq!(recommendation.direction, ScaleDirection::Up);
            assert_eq!(recommendation.target_nodes, 10);
        }

        #[test]
        fn schedule_scales_down_on_weekend() {
            let pool = create_test_pool_with_policy(create_schedule_policy());
            let provider = InMemoryMetricsProvider::new();
            provider.set_metrics(&pool.id, MetricsSnapshot::new());

            // Saturday at noon
            let now = chrono::DateTime::parse_from_rfc3339("2024-01-13T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc);

            let autoscaler = Autoscaler::new(provider);
            let recommendation = autoscaler.evaluate_at(&pool, now).unwrap();

            assert_eq!(recommendation.direction, ScaleDirection::Down);
            assert_eq!(recommendation.target_nodes, 2);
        }
    }

    mod cooldown_tests {
        use super::*;

        #[test]
        fn cooldown_blocks_scale_up() {
            let policy = ScalingPolicy::builder("cooldown-test", "Cooldown Test")
                .min_nodes(1)
                .max_nodes(20)
                .target_utilization(70.0, 10.0)
                .scale_up_cooldown(Duration::from_secs(300))
                .build()
                .unwrap();

            let mut pool = create_test_pool_with_policy(policy);
            let now = Utc::now();
            pool.record_scale_up(now);

            let provider = InMemoryMetricsProvider::new();
            provider.set_metrics(
                &pool.id,
                MetricsSnapshot::new().with_gpu_utilization(95.0, 90.0, 100.0),
            );

            let autoscaler = Autoscaler::new(provider);
            let recommendation = autoscaler.evaluate_at(&pool, now).unwrap();

            assert_eq!(recommendation.direction, ScaleDirection::None);
            assert!(recommendation.reason.contains("cooldown"));
        }

        #[test]
        fn cooldown_allows_scale_after_period() {
            let policy = ScalingPolicy::builder("cooldown-test", "Cooldown Test")
                .min_nodes(1)
                .max_nodes(20)
                .target_utilization(70.0, 10.0)
                .scale_up_cooldown(Duration::from_secs(300))
                .build()
                .unwrap();

            let mut pool = create_test_pool_with_policy(policy);
            let scale_time = Utc::now();
            pool.record_scale_up(scale_time);

            let provider = InMemoryMetricsProvider::new();
            provider.set_metrics(
                &pool.id,
                MetricsSnapshot::new().with_gpu_utilization(95.0, 90.0, 100.0),
            );

            // Evaluate 10 minutes later
            let later = scale_time + chrono::Duration::seconds(600);
            let autoscaler = Autoscaler::new(provider);
            let recommendation = autoscaler.evaluate_at(&pool, later).unwrap();

            assert_eq!(recommendation.direction, ScaleDirection::Up);
        }
    }

    mod bounds_tests {
        use super::*;

        #[test]
        fn respects_min_nodes() {
            let policy = ScalingPolicy::builder("bounds-test", "Bounds Test")
                .min_nodes(3)
                .max_nodes(20)
                .target_utilization(70.0, 10.0)
                .build()
                .unwrap();

            let pool = create_test_pool_with_policy(policy);
            let provider = InMemoryMetricsProvider::new();
            provider.set_metrics(
                &pool.id,
                MetricsSnapshot::new().with_gpu_utilization(10.0, 5.0, 15.0),
            );

            let autoscaler = Autoscaler::new(provider);
            let recommendation = autoscaler.evaluate(&pool).unwrap();

            // Should not go below min_nodes
            assert!(recommendation.target_nodes >= 3);
        }

        #[test]
        fn respects_max_nodes() {
            let policy = ScalingPolicy::builder("bounds-test", "Bounds Test")
                .min_nodes(1)
                .max_nodes(8)
                .target_utilization(70.0, 10.0)
                .build()
                .unwrap();

            let pool = create_test_pool_with_policy(policy);
            let provider = InMemoryMetricsProvider::new();
            provider.set_metrics(
                &pool.id,
                MetricsSnapshot::new().with_gpu_utilization(99.0, 98.0, 100.0),
            );

            let autoscaler = Autoscaler::new(provider);
            let recommendation = autoscaler.evaluate(&pool).unwrap();

            // Should not exceed max_nodes
            assert!(recommendation.target_nodes <= 8);
        }
    }

    mod max_delta_tests {
        use super::*;

        #[test]
        fn caps_large_scale_up() {
            let policy = ScalingPolicy::builder("delta-test", "Delta Test")
                .min_nodes(1)
                .max_nodes(100)
                .target_utilization(70.0, 10.0)
                .build()
                .unwrap();

            let pool = create_test_pool_with_policy(policy);
            let provider = InMemoryMetricsProvider::new();
            provider.set_metrics(
                &pool.id,
                MetricsSnapshot::new().with_gpu_utilization(99.0, 98.0, 100.0),
            );

            let config = AutoscalerConfig {
                max_scale_delta: 3,
                ..Default::default()
            };

            let autoscaler = Autoscaler::with_config(provider, config);
            let recommendation = autoscaler.evaluate(&pool).unwrap();

            // Should cap at 5 + 3 = 8 nodes
            assert!(recommendation.target_nodes <= 8);
        }
    }

    mod disabled_policy_tests {
        use super::*;

        #[test]
        fn disabled_policy_returns_no_change() {
            let policy = ScalingPolicy::builder("disabled-test", "Disabled Test")
                .min_nodes(1)
                .max_nodes(20)
                .target_utilization(70.0, 10.0)
                .enabled(false)
                .build()
                .unwrap();

            let pool = create_test_pool_with_policy(policy);
            let provider = InMemoryMetricsProvider::new();
            provider.set_metrics(
                &pool.id,
                MetricsSnapshot::new().with_gpu_utilization(99.0, 98.0, 100.0),
            );

            let autoscaler = Autoscaler::new(provider);
            let recommendation = autoscaler.evaluate(&pool).unwrap();

            assert_eq!(recommendation.direction, ScaleDirection::None);
            assert!(recommendation.reason.contains("disabled"));
        }
    }

    mod evaluate_all_tests {
        use super::*;

        #[test]
        fn evaluates_multiple_pools() {
            let policy1 = create_utilization_policy();
            let policy2 = ScalingPolicy::builder("policy-2", "Policy 2")
                .min_nodes(1)
                .max_nodes(10)
                .target_utilization(80.0, 5.0)
                .build()
                .unwrap();

            let mut pool1 = NodePool::new("pool-1", "Pool 1", policy1).unwrap();
            pool1.add_node(NodeInfo::new("node-1", "A100", 8));
            pool1.add_node(NodeInfo::new("node-2", "A100", 8));

            let mut pool2 = NodePool::new("pool-2", "Pool 2", policy2).unwrap();
            pool2.add_node(NodeInfo::new("node-3", "A100", 8));

            let provider = InMemoryMetricsProvider::new();
            provider.set_metrics(
                &pool1.id,
                MetricsSnapshot::new().with_gpu_utilization(90.0, 85.0, 95.0),
            );
            provider.set_metrics(
                &pool2.id,
                MetricsSnapshot::new().with_gpu_utilization(50.0, 45.0, 55.0),
            );

            let autoscaler = Autoscaler::new(provider);
            let recommendations = autoscaler.evaluate_all(&[pool1, pool2]).unwrap();

            assert_eq!(recommendations.len(), 2);
            assert!(recommendations.contains_key(&PoolId::new("pool-1")));
            assert!(recommendations.contains_key(&PoolId::new("pool-2")));
        }
    }

    mod combined_policy_tests {
        use super::*;

        #[test]
        fn combined_any_returns_first_trigger() {
            let policy = ScalingPolicy::builder("combined-test", "Combined Test")
                .min_nodes(1)
                .max_nodes(50)
                .policy_type(ScalingPolicyType::Combined {
                    policies: vec![
                        ScalingPolicyType::TargetUtilization {
                            target_percent: 70.0,
                            tolerance_percent: 10.0,
                        },
                        ScalingPolicyType::QueueDepth {
                            target_jobs_per_node: 5,
                            scale_up_threshold: 20,
                            scale_down_threshold: 2,
                        },
                    ],
                    combination: CombinationStrategy::Any,
                })
                .build()
                .unwrap();

            let pool = create_test_pool_with_policy(policy);
            let provider = InMemoryMetricsProvider::new();
            // High utilization but normal queue
            provider.set_metrics(
                &pool.id,
                MetricsSnapshot::new()
                    .with_gpu_utilization(95.0, 90.0, 100.0)
                    .with_queue_depth(25),
            );

            let autoscaler = Autoscaler::new(provider);
            let recommendation = autoscaler.evaluate(&pool).unwrap();

            assert_eq!(recommendation.direction, ScaleDirection::Up);
        }

        #[test]
        fn combined_all_requires_agreement() {
            let policy = ScalingPolicy::builder("combined-test", "Combined Test")
                .min_nodes(1)
                .max_nodes(50)
                .policy_type(ScalingPolicyType::Combined {
                    policies: vec![
                        ScalingPolicyType::TargetUtilization {
                            target_percent: 70.0,
                            tolerance_percent: 10.0,
                        },
                        ScalingPolicyType::QueueDepth {
                            target_jobs_per_node: 5,
                            scale_up_threshold: 20,
                            scale_down_threshold: 2,
                        },
                    ],
                    combination: CombinationStrategy::All,
                })
                .build()
                .unwrap();

            let pool = create_test_pool_with_policy(policy);
            let provider = InMemoryMetricsProvider::new();
            // High utilization (wants scale up) but low queue (wants scale down)
            provider.set_metrics(
                &pool.id,
                MetricsSnapshot::new()
                    .with_gpu_utilization(95.0, 90.0, 100.0)
                    .with_queue_depth(5),
            );

            let autoscaler = Autoscaler::new(provider);
            let recommendation = autoscaler.evaluate(&pool).unwrap();

            // Should not scale because policies disagree
            assert_eq!(recommendation.direction, ScaleDirection::None);
            assert!(recommendation.reason.contains("disagree"));
        }
    }

    mod metrics_provider_tests {
        use super::*;

        #[test]
        fn in_memory_provider_set_and_get() {
            let provider = InMemoryMetricsProvider::new();
            let pool_id = PoolId::new("test-pool");

            let metrics = MetricsSnapshot::new()
                .with_gpu_utilization(75.0, 70.0, 80.0)
                .with_queue_depth(100);

            provider.set_metrics(&pool_id, metrics.clone());
            let retrieved = provider.get_metrics(&pool_id).unwrap();

            assert!((retrieved.avg_gpu_utilization - 75.0).abs() < f64::EPSILON);
            assert_eq!(retrieved.queue_depth, 100);
        }

        #[test]
        fn in_memory_provider_missing_pool() {
            let provider = InMemoryMetricsProvider::new();
            let pool_id = PoolId::new("missing-pool");

            let result = provider.get_metrics(&pool_id);
            assert!(result.is_err());
        }
    }
}
