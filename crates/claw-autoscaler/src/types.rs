//! Core types for the autoscaler system.
//!
//! This module provides the fundamental types used throughout claw-autoscaler:
//! - [`ScalingPolicy`]: Configuration for how scaling decisions are made
//! - [`NodePool`]: A group of similar nodes with shared scaling config
//! - [`ScaleRecommendation`]: A recommendation to scale up, down, or maintain
//! - [`NodeStatus`]: Current status of a node in a pool

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{AutoscalerError, Result};

/// Unique identifier for a node pool.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PoolId(String);

impl PoolId {
    /// Creates a new pool ID.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the ID as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PoolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(String);

impl NodeId {
    /// Creates a new node ID.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the ID as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Scaling policy type determining how scaling decisions are made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScalingPolicyType {
    /// Scale based on target GPU utilization percentage.
    TargetUtilization {
        /// Target GPU utilization percentage (0-100).
        target_percent: f64,
        /// Tolerance band around target before scaling (e.g., 10 means ±10%).
        tolerance_percent: f64,
    },

    /// Scale based on job queue depth.
    QueueDepth {
        /// Target number of queued jobs per node.
        target_jobs_per_node: u32,
        /// Scale up when queue exceeds this threshold.
        scale_up_threshold: u32,
        /// Scale down when queue falls below this threshold.
        scale_down_threshold: u32,
    },

    /// Scale based on a time-based schedule.
    Schedule {
        /// Scheduled scaling rules.
        rules: Vec<ScheduleRule>,
    },

    /// Combined policy using multiple signals.
    Combined {
        /// Policies to combine.
        policies: Vec<ScalingPolicyType>,
        /// How to combine the policies.
        combination: CombinationStrategy,
    },
}

/// How to combine multiple scaling policies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CombinationStrategy {
    /// Scale if any policy recommends scaling.
    #[default]
    Any,
    /// Scale only if all policies agree.
    All,
    /// Use the most aggressive (largest) recommendation.
    MostAggressive,
    /// Use the most conservative (smallest) recommendation.
    MostConservative,
}

/// A time-based scheduling rule for scaling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleRule {
    /// Name/description of this rule.
    pub name: String,
    /// Days of week this rule applies (0 = Sunday, 6 = Saturday).
    pub days_of_week: Vec<u8>,
    /// Start hour (0-23) in UTC.
    pub start_hour: u8,
    /// End hour (0-23) in UTC.
    pub end_hour: u8,
    /// Desired node count during this period.
    pub desired_nodes: u32,
}

impl ScheduleRule {
    /// Creates a new schedule rule.
    ///
    /// # Errors
    ///
    /// Returns error if hours are invalid (>23) or days are invalid (>6).
    pub fn new(
        name: impl Into<String>,
        days_of_week: Vec<u8>,
        start_hour: u8,
        end_hour: u8,
        desired_nodes: u32,
    ) -> Result<Self> {
        if start_hour > 23 || end_hour > 23 {
            return Err(AutoscalerError::InvalidSchedule {
                reason: "hours must be 0-23".into(),
            });
        }

        for day in &days_of_week {
            if *day > 6 {
                return Err(AutoscalerError::InvalidSchedule {
                    reason: "days must be 0-6 (Sunday-Saturday)".into(),
                });
            }
        }

        Ok(Self {
            name: name.into(),
            days_of_week,
            start_hour,
            end_hour,
            desired_nodes,
        })
    }

    /// Checks if this rule applies at the given time.
    #[must_use]
    pub fn applies_at(&self, time: &DateTime<Utc>) -> bool {
        use chrono::Datelike;
        use chrono::Timelike;

        let day = time.weekday().num_days_from_sunday() as u8;
        let hour = time.hour() as u8;

        if !self.days_of_week.contains(&day) {
            return false;
        }

        // Handle overnight spans (e.g., 22:00 to 06:00)
        if self.start_hour <= self.end_hour {
            hour >= self.start_hour && hour < self.end_hour
        } else {
            hour >= self.start_hour || hour < self.end_hour
        }
    }
}

/// Complete scaling policy configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScalingPolicy {
    /// Unique identifier for this policy.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Minimum number of nodes (never scale below this).
    pub min_nodes: u32,
    /// Maximum number of nodes (never scale above this).
    pub max_nodes: u32,
    /// The scaling policy type.
    pub policy_type: ScalingPolicyType,
    /// Cooldown after scale up before another scale action.
    pub scale_up_cooldown: Duration,
    /// Cooldown after scale down before another scale action.
    pub scale_down_cooldown: Duration,
    /// Whether this policy is enabled.
    pub enabled: bool,
}

impl ScalingPolicy {
    /// Creates a new scaling policy builder.
    #[must_use]
    pub fn builder(id: impl Into<String>, name: impl Into<String>) -> ScalingPolicyBuilder {
        ScalingPolicyBuilder::new(id, name)
    }

    /// Validates this policy configuration.
    ///
    /// # Errors
    ///
    /// Returns error if the policy is invalid.
    pub fn validate(&self) -> Result<()> {
        if self.min_nodes > self.max_nodes {
            return Err(AutoscalerError::InvalidPolicy {
                reason: format!(
                    "min_nodes ({}) cannot exceed max_nodes ({})",
                    self.min_nodes, self.max_nodes
                ),
            });
        }

        if self.max_nodes == 0 {
            return Err(AutoscalerError::InvalidPolicy {
                reason: "max_nodes must be at least 1".into(),
            });
        }

        self.validate_policy_type(&self.policy_type)?;

        Ok(())
    }

    fn validate_policy_type(&self, policy_type: &ScalingPolicyType) -> Result<()> {
        match policy_type {
            ScalingPolicyType::TargetUtilization {
                target_percent,
                tolerance_percent,
            } => {
                if *target_percent <= 0.0 || *target_percent > 100.0 {
                    return Err(AutoscalerError::InvalidPolicy {
                        reason: "target_percent must be between 0 and 100".into(),
                    });
                }
                if *tolerance_percent < 0.0 || *tolerance_percent > 50.0 {
                    return Err(AutoscalerError::InvalidPolicy {
                        reason: "tolerance_percent must be between 0 and 50".into(),
                    });
                }
            }
            ScalingPolicyType::QueueDepth {
                scale_up_threshold,
                scale_down_threshold,
                ..
            } => {
                if scale_down_threshold >= scale_up_threshold {
                    return Err(AutoscalerError::InvalidPolicy {
                        reason: "scale_down_threshold must be less than scale_up_threshold".into(),
                    });
                }
            }
            ScalingPolicyType::Schedule { rules } => {
                if rules.is_empty() {
                    return Err(AutoscalerError::InvalidPolicy {
                        reason: "schedule policy must have at least one rule".into(),
                    });
                }
            }
            ScalingPolicyType::Combined { policies, .. } => {
                if policies.is_empty() {
                    return Err(AutoscalerError::InvalidPolicy {
                        reason: "combined policy must have at least one sub-policy".into(),
                    });
                }
                for sub_policy in policies {
                    self.validate_policy_type(sub_policy)?;
                }
            }
        }
        Ok(())
    }
}

/// Builder for creating scaling policies.
#[derive(Debug)]
pub struct ScalingPolicyBuilder {
    id: String,
    name: String,
    min_nodes: u32,
    max_nodes: u32,
    policy_type: Option<ScalingPolicyType>,
    scale_up_cooldown: Duration,
    scale_down_cooldown: Duration,
    enabled: bool,
}

impl ScalingPolicyBuilder {
    /// Creates a new builder with required fields.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            min_nodes: 1,
            max_nodes: 10,
            policy_type: None,
            scale_up_cooldown: Duration::from_secs(300), // 5 minutes
            scale_down_cooldown: Duration::from_secs(600), // 10 minutes
            enabled: true,
        }
    }

    /// Sets the minimum number of nodes.
    #[must_use]
    pub const fn min_nodes(mut self, min: u32) -> Self {
        self.min_nodes = min;
        self
    }

    /// Sets the maximum number of nodes.
    #[must_use]
    pub const fn max_nodes(mut self, max: u32) -> Self {
        self.max_nodes = max;
        self
    }

    /// Sets the policy type.
    #[must_use]
    pub fn policy_type(mut self, policy_type: ScalingPolicyType) -> Self {
        self.policy_type = Some(policy_type);
        self
    }

    /// Sets a target utilization policy.
    #[must_use]
    pub fn target_utilization(self, target_percent: f64, tolerance_percent: f64) -> Self {
        self.policy_type(ScalingPolicyType::TargetUtilization {
            target_percent,
            tolerance_percent,
        })
    }

    /// Sets a queue depth policy.
    #[must_use]
    pub fn queue_depth(
        self,
        target_jobs_per_node: u32,
        scale_up_threshold: u32,
        scale_down_threshold: u32,
    ) -> Self {
        self.policy_type(ScalingPolicyType::QueueDepth {
            target_jobs_per_node,
            scale_up_threshold,
            scale_down_threshold,
        })
    }

    /// Sets the scale up cooldown.
    #[must_use]
    pub const fn scale_up_cooldown(mut self, cooldown: Duration) -> Self {
        self.scale_up_cooldown = cooldown;
        self
    }

    /// Sets the scale down cooldown.
    #[must_use]
    pub const fn scale_down_cooldown(mut self, cooldown: Duration) -> Self {
        self.scale_down_cooldown = cooldown;
        self
    }

    /// Sets whether the policy is enabled.
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Builds the scaling policy.
    ///
    /// # Errors
    ///
    /// Returns error if the policy is invalid.
    pub fn build(self) -> Result<ScalingPolicy> {
        let policy_type = self.policy_type.ok_or_else(|| AutoscalerError::InvalidPolicy {
            reason: "policy_type is required".into(),
        })?;

        let policy = ScalingPolicy {
            id: self.id,
            name: self.name,
            min_nodes: self.min_nodes,
            max_nodes: self.max_nodes,
            policy_type,
            scale_up_cooldown: self.scale_up_cooldown,
            scale_down_cooldown: self.scale_down_cooldown,
            enabled: self.enabled,
        };

        policy.validate()?;
        Ok(policy)
    }
}

/// Direction of a scaling recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScaleDirection {
    /// Increase the number of nodes.
    Up,
    /// Decrease the number of nodes.
    Down,
    /// Maintain current node count.
    None,
}

/// A scaling recommendation from the autoscaler.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScaleRecommendation {
    /// Direction of the recommendation.
    pub direction: ScaleDirection,
    /// Current node count.
    pub current_nodes: u32,
    /// Recommended node count.
    pub target_nodes: u32,
    /// Human-readable reason for this recommendation.
    pub reason: String,
    /// Confidence in this recommendation (0.0 - 1.0).
    pub confidence: f64,
    /// When this recommendation was made.
    pub timestamp: DateTime<Utc>,
    /// Metrics that influenced this decision.
    pub metrics: HashMap<String, f64>,
}

impl ScaleRecommendation {
    /// Creates a new scale recommendation.
    #[must_use]
    pub fn new(
        direction: ScaleDirection,
        current_nodes: u32,
        target_nodes: u32,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            direction,
            current_nodes,
            target_nodes,
            reason: reason.into(),
            confidence: 1.0,
            timestamp: Utc::now(),
            metrics: HashMap::new(),
        }
    }

    /// Creates a "no change" recommendation.
    #[must_use]
    pub fn no_change(current_nodes: u32, reason: impl Into<String>) -> Self {
        Self::new(ScaleDirection::None, current_nodes, current_nodes, reason)
    }

    /// Creates a "scale up" recommendation.
    #[must_use]
    pub fn scale_up(current_nodes: u32, target_nodes: u32, reason: impl Into<String>) -> Self {
        Self::new(ScaleDirection::Up, current_nodes, target_nodes, reason)
    }

    /// Creates a "scale down" recommendation.
    #[must_use]
    pub fn scale_down(current_nodes: u32, target_nodes: u32, reason: impl Into<String>) -> Self {
        Self::new(ScaleDirection::Down, current_nodes, target_nodes, reason)
    }

    /// Sets the confidence level.
    #[must_use]
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Adds a metric that influenced this decision.
    #[must_use]
    pub fn with_metric(mut self, name: impl Into<String>, value: f64) -> Self {
        self.metrics.insert(name.into(), value);
        self
    }

    /// Returns the delta (change in node count).
    #[must_use]
    #[allow(clippy::cast_possible_wrap)]
    pub fn delta(&self) -> i32 {
        self.target_nodes as i32 - self.current_nodes as i32
    }
}

/// Status of a node in a pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NodeStatus {
    /// Node is ready and accepting workloads.
    #[default]
    Ready,
    /// Node is starting up.
    Pending,
    /// Node is being drained before removal.
    Draining,
    /// Node has failed health checks.
    Unhealthy,
    /// Node is being removed.
    Terminating,
}

/// Information about a node in a pool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Unique identifier for this node.
    pub id: NodeId,
    /// Current status.
    pub status: NodeStatus,
    /// GPU model (e.g., "RTX 4090", "A100").
    pub gpu_model: String,
    /// Number of GPUs on this node.
    pub gpu_count: u32,
    /// When this node was added to the pool.
    pub created_at: DateTime<Utc>,
    /// Custom labels for this node.
    pub labels: HashMap<String, String>,
}

impl NodeInfo {
    /// Creates a new node info.
    #[must_use]
    pub fn new(id: impl Into<String>, gpu_model: impl Into<String>, gpu_count: u32) -> Self {
        Self {
            id: NodeId::new(id),
            status: NodeStatus::Ready,
            gpu_model: gpu_model.into(),
            gpu_count,
            created_at: Utc::now(),
            labels: HashMap::new(),
        }
    }

    /// Sets the node status.
    #[must_use]
    pub const fn with_status(mut self, status: NodeStatus) -> Self {
        self.status = status;
        self
    }

    /// Adds a label to this node.
    #[must_use]
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Checks if this node is ready for workloads.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.status == NodeStatus::Ready
    }
}

/// A group of similar nodes with shared scaling configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodePool {
    /// Unique identifier for this pool.
    pub id: PoolId,
    /// Human-readable name.
    pub name: String,
    /// Nodes in this pool.
    pub nodes: Vec<NodeInfo>,
    /// Scaling policy for this pool.
    pub policy: ScalingPolicy,
    /// Last scale action timestamp (for cooldown tracking).
    pub last_scale_up: Option<DateTime<Utc>>,
    /// Last scale down timestamp (for cooldown tracking).
    pub last_scale_down: Option<DateTime<Utc>>,
    /// Custom labels for this pool.
    pub labels: HashMap<String, String>,
}

impl NodePool {
    /// Creates a new node pool.
    ///
    /// # Errors
    ///
    /// Returns error if the policy is invalid.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        policy: ScalingPolicy,
    ) -> Result<Self> {
        policy.validate()?;

        Ok(Self {
            id: PoolId::new(id),
            name: name.into(),
            nodes: Vec::new(),
            policy,
            last_scale_up: None,
            last_scale_down: None,
            labels: HashMap::new(),
        })
    }

    /// Adds a label to this pool.
    #[must_use]
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Adds a node to this pool.
    pub fn add_node(&mut self, node: NodeInfo) {
        self.nodes.push(node);
    }

    /// Removes a node from this pool by ID.
    ///
    /// Returns the removed node if found.
    pub fn remove_node(&mut self, node_id: &NodeId) -> Option<NodeInfo> {
        if let Some(pos) = self.nodes.iter().position(|n| &n.id == node_id) {
            Some(self.nodes.remove(pos))
        } else {
            None
        }
    }

    /// Returns the current number of nodes.
    #[must_use]
    pub fn node_count(&self) -> u32 {
        self.nodes.len() as u32
    }

    /// Returns the number of ready nodes.
    #[must_use]
    pub fn ready_node_count(&self) -> u32 {
        self.nodes.iter().filter(|n| n.is_ready()).count() as u32
    }

    /// Returns the total GPU count across all nodes.
    #[must_use]
    pub fn total_gpu_count(&self) -> u32 {
        self.nodes.iter().map(|n| n.gpu_count).sum()
    }

    /// Checks if scaling up is allowed (not in cooldown).
    #[must_use]
    pub fn can_scale_up(&self, now: &DateTime<Utc>) -> bool {
        if let Some(last) = self.last_scale_up {
            let cooldown = chrono::Duration::from_std(self.policy.scale_up_cooldown)
                .unwrap_or_else(|_| chrono::Duration::seconds(300));
            *now >= last + cooldown
        } else {
            true
        }
    }

    /// Checks if scaling down is allowed (not in cooldown).
    #[must_use]
    pub fn can_scale_down(&self, now: &DateTime<Utc>) -> bool {
        if let Some(last) = self.last_scale_down {
            let cooldown = chrono::Duration::from_std(self.policy.scale_down_cooldown)
                .unwrap_or_else(|_| chrono::Duration::seconds(600));
            *now >= last + cooldown
        } else {
            true
        }
    }

    /// Records a scale up event.
    pub fn record_scale_up(&mut self, timestamp: DateTime<Utc>) {
        self.last_scale_up = Some(timestamp);
    }

    /// Records a scale down event.
    pub fn record_scale_down(&mut self, timestamp: DateTime<Utc>) {
        self.last_scale_down = Some(timestamp);
    }

    /// Updates the scaling policy.
    ///
    /// # Errors
    ///
    /// Returns error if the new policy is invalid.
    pub fn set_policy(&mut self, policy: ScalingPolicy) -> Result<()> {
        policy.validate()?;
        self.policy = policy;
        Ok(())
    }
}

/// Metrics snapshot for autoscaling decisions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MetricsSnapshot {
    /// Average GPU utilization across all nodes (0-100).
    pub avg_gpu_utilization: f64,
    /// Maximum GPU utilization among all nodes (0-100).
    pub max_gpu_utilization: f64,
    /// Minimum GPU utilization among all nodes (0-100).
    pub min_gpu_utilization: f64,
    /// Number of jobs in the queue.
    pub queue_depth: u32,
    /// Average GPU memory utilization (0-100).
    pub avg_memory_utilization: f64,
    /// Custom metrics.
    pub custom: HashMap<String, f64>,
}

impl MetricsSnapshot {
    /// Creates a new metrics snapshot.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets GPU utilization values.
    #[must_use]
    pub const fn with_gpu_utilization(mut self, avg: f64, min: f64, max: f64) -> Self {
        self.avg_gpu_utilization = avg;
        self.min_gpu_utilization = min;
        self.max_gpu_utilization = max;
        self
    }

    /// Sets queue depth.
    #[must_use]
    pub const fn with_queue_depth(mut self, depth: u32) -> Self {
        self.queue_depth = depth;
        self
    }

    /// Sets memory utilization.
    #[must_use]
    pub const fn with_memory_utilization(mut self, avg: f64) -> Self {
        self.avg_memory_utilization = avg;
        self
    }

    /// Adds a custom metric.
    #[must_use]
    pub fn with_custom(mut self, name: impl Into<String>, value: f64) -> Self {
        self.custom.insert(name.into(), value);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod pool_id_tests {
        use super::*;

        #[test]
        fn pool_id_creation_and_display() {
            let id = PoolId::new("gpu-pool-1");
            assert_eq!(id.as_str(), "gpu-pool-1");
            assert_eq!(format!("{id}"), "gpu-pool-1");
        }

        #[test]
        fn pool_id_equality() {
            let id1 = PoolId::new("pool-a");
            let id2 = PoolId::new("pool-a");
            let id3 = PoolId::new("pool-b");

            assert_eq!(id1, id2);
            assert_ne!(id1, id3);
        }

        #[test]
        fn pool_id_serialization_roundtrip() {
            let id = PoolId::new("test-pool");
            let json = serde_json::to_string(&id);
            assert!(json.is_ok());
            let parsed: serde_json::Result<PoolId> = serde_json::from_str(&json.unwrap());
            assert!(parsed.is_ok());
            assert_eq!(parsed.unwrap(), id);
        }
    }

    mod node_id_tests {
        use super::*;

        #[test]
        fn node_id_creation_and_display() {
            let id = NodeId::new("node-abc-123");
            assert_eq!(id.as_str(), "node-abc-123");
            assert_eq!(format!("{id}"), "node-abc-123");
        }
    }

    mod schedule_rule_tests {
        use super::*;

        #[test]
        fn schedule_rule_valid_creation() {
            let rule = ScheduleRule::new("weekday-high", vec![1, 2, 3, 4, 5], 9, 17, 10);
            assert!(rule.is_ok());
            let rule = rule.unwrap();
            assert_eq!(rule.name, "weekday-high");
            assert_eq!(rule.desired_nodes, 10);
        }

        #[test]
        fn schedule_rule_invalid_hour() {
            let rule = ScheduleRule::new("bad", vec![1], 24, 17, 5);
            assert!(rule.is_err());

            let rule = ScheduleRule::new("bad", vec![1], 9, 25, 5);
            assert!(rule.is_err());
        }

        #[test]
        fn schedule_rule_invalid_day() {
            let rule = ScheduleRule::new("bad", vec![7], 9, 17, 5);
            assert!(rule.is_err());
        }

        #[test]
        fn schedule_rule_applies_at() {
            let rule = ScheduleRule::new("weekday-9to5", vec![1, 2, 3, 4, 5], 9, 17, 10).unwrap();

            // Monday at 10:00 - should apply
            let monday_10am = chrono::DateTime::parse_from_rfc3339("2024-01-15T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc);
            assert!(rule.applies_at(&monday_10am));

            // Monday at 8:00 - should not apply (before start)
            let monday_8am = chrono::DateTime::parse_from_rfc3339("2024-01-15T08:00:00Z")
                .unwrap()
                .with_timezone(&Utc);
            assert!(!rule.applies_at(&monday_8am));

            // Saturday at 10:00 - should not apply (weekend)
            let saturday_10am = chrono::DateTime::parse_from_rfc3339("2024-01-13T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc);
            assert!(!rule.applies_at(&saturday_10am));
        }

        #[test]
        fn schedule_rule_overnight_span() {
            // Night shift: 22:00 to 06:00
            let rule = ScheduleRule::new("night-shift", vec![0, 1, 2, 3, 4, 5, 6], 22, 6, 5).unwrap();

            // Monday at 23:00 - should apply
            let monday_11pm = chrono::DateTime::parse_from_rfc3339("2024-01-15T23:00:00Z")
                .unwrap()
                .with_timezone(&Utc);
            assert!(rule.applies_at(&monday_11pm));

            // Tuesday at 3:00 - should apply
            let tuesday_3am = chrono::DateTime::parse_from_rfc3339("2024-01-16T03:00:00Z")
                .unwrap()
                .with_timezone(&Utc);
            assert!(rule.applies_at(&tuesday_3am));

            // Tuesday at 12:00 - should not apply (daytime)
            let tuesday_noon = chrono::DateTime::parse_from_rfc3339("2024-01-16T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc);
            assert!(!rule.applies_at(&tuesday_noon));
        }
    }

    mod scaling_policy_tests {
        use super::*;

        #[test]
        fn policy_builder_target_utilization() {
            let policy = ScalingPolicy::builder("policy-1", "GPU Utilization Policy")
                .min_nodes(2)
                .max_nodes(20)
                .target_utilization(70.0, 10.0)
                .build();

            assert!(policy.is_ok());
            let policy = policy.unwrap();
            assert_eq!(policy.id, "policy-1");
            assert_eq!(policy.min_nodes, 2);
            assert_eq!(policy.max_nodes, 20);
            assert!(policy.enabled);
        }

        #[test]
        fn policy_builder_queue_depth() {
            let policy = ScalingPolicy::builder("policy-2", "Queue Policy")
                .min_nodes(1)
                .max_nodes(50)
                .queue_depth(3, 10, 2)
                .build();

            assert!(policy.is_ok());
        }

        #[test]
        fn policy_builder_missing_type() {
            let result = ScalingPolicy::builder("policy-3", "Empty").build();
            assert!(result.is_err());
        }

        #[test]
        fn policy_validation_min_greater_than_max() {
            let result = ScalingPolicy::builder("policy-4", "Invalid")
                .min_nodes(10)
                .max_nodes(5)
                .target_utilization(70.0, 10.0)
                .build();

            assert!(result.is_err());
            if let Err(AutoscalerError::InvalidPolicy { reason }) = result {
                assert!(reason.contains("min_nodes"));
            }
        }

        #[test]
        fn policy_validation_max_zero() {
            let result = ScalingPolicy::builder("policy-5", "Invalid")
                .min_nodes(0)
                .max_nodes(0)
                .target_utilization(70.0, 10.0)
                .build();

            assert!(result.is_err());
        }

        #[test]
        fn policy_validation_invalid_target_percent() {
            let result = ScalingPolicy::builder("policy-6", "Invalid")
                .target_utilization(150.0, 10.0)
                .build();

            assert!(result.is_err());
        }

        #[test]
        fn policy_validation_invalid_queue_thresholds() {
            let result = ScalingPolicy::builder("policy-7", "Invalid")
                .queue_depth(3, 5, 10) // scale_down >= scale_up
                .build();

            assert!(result.is_err());
        }

        #[test]
        fn policy_validation_empty_schedule() {
            let policy_type = ScalingPolicyType::Schedule { rules: vec![] };
            let result = ScalingPolicy::builder("policy-8", "Invalid")
                .policy_type(policy_type)
                .build();

            assert!(result.is_err());
        }

        #[test]
        fn policy_serialization_roundtrip() {
            let policy = ScalingPolicy::builder("policy-9", "Test")
                .target_utilization(70.0, 10.0)
                .build()
                .unwrap();

            let json = serde_json::to_string(&policy);
            assert!(json.is_ok());
            let parsed: serde_json::Result<ScalingPolicy> = serde_json::from_str(&json.unwrap());
            assert!(parsed.is_ok());
            assert_eq!(parsed.unwrap(), policy);
        }
    }

    mod scale_recommendation_tests {
        use super::*;

        #[test]
        fn recommendation_no_change() {
            let rec = ScaleRecommendation::no_change(5, "cluster is balanced");
            assert_eq!(rec.direction, ScaleDirection::None);
            assert_eq!(rec.current_nodes, 5);
            assert_eq!(rec.target_nodes, 5);
            assert_eq!(rec.delta(), 0);
        }

        #[test]
        fn recommendation_scale_up() {
            let rec = ScaleRecommendation::scale_up(5, 8, "high utilization");
            assert_eq!(rec.direction, ScaleDirection::Up);
            assert_eq!(rec.current_nodes, 5);
            assert_eq!(rec.target_nodes, 8);
            assert_eq!(rec.delta(), 3);
        }

        #[test]
        fn recommendation_scale_down() {
            let rec = ScaleRecommendation::scale_down(10, 6, "low utilization");
            assert_eq!(rec.direction, ScaleDirection::Down);
            assert_eq!(rec.delta(), -4);
        }

        #[test]
        fn recommendation_with_confidence() {
            let rec = ScaleRecommendation::scale_up(5, 10, "test")
                .with_confidence(0.8);
            assert!((rec.confidence - 0.8).abs() < f64::EPSILON);
        }

        #[test]
        fn recommendation_confidence_clamped() {
            let rec = ScaleRecommendation::scale_up(5, 10, "test")
                .with_confidence(1.5);
            assert!((rec.confidence - 1.0).abs() < f64::EPSILON);

            let rec = ScaleRecommendation::scale_up(5, 10, "test")
                .with_confidence(-0.5);
            assert!(rec.confidence.abs() < f64::EPSILON);
        }

        #[test]
        fn recommendation_with_metrics() {
            let rec = ScaleRecommendation::scale_up(5, 10, "test")
                .with_metric("gpu_util", 95.0)
                .with_metric("queue_depth", 50.0);

            assert_eq!(rec.metrics.get("gpu_util"), Some(&95.0));
            assert_eq!(rec.metrics.get("queue_depth"), Some(&50.0));
        }
    }

    mod node_info_tests {
        use super::*;

        #[test]
        fn node_info_creation() {
            let node = NodeInfo::new("node-1", "RTX 4090", 2);
            assert_eq!(node.id.as_str(), "node-1");
            assert_eq!(node.gpu_model, "RTX 4090");
            assert_eq!(node.gpu_count, 2);
            assert_eq!(node.status, NodeStatus::Ready);
            assert!(node.is_ready());
        }

        #[test]
        fn node_info_with_status() {
            let node = NodeInfo::new("node-2", "A100", 8)
                .with_status(NodeStatus::Draining);
            assert_eq!(node.status, NodeStatus::Draining);
            assert!(!node.is_ready());
        }

        #[test]
        fn node_info_with_labels() {
            let node = NodeInfo::new("node-3", "H100", 8)
                .with_label("region", "us-west")
                .with_label("tier", "premium");

            assert_eq!(node.labels.get("region"), Some(&"us-west".into()));
            assert_eq!(node.labels.get("tier"), Some(&"premium".into()));
        }
    }

    mod node_pool_tests {
        use super::*;

        fn create_test_policy() -> ScalingPolicy {
            ScalingPolicy::builder("test-policy", "Test")
                .min_nodes(1)
                .max_nodes(10)
                .target_utilization(70.0, 10.0)
                .build()
                .unwrap()
        }

        #[test]
        fn pool_creation() {
            let policy = create_test_policy();
            let pool = NodePool::new("pool-1", "GPU Pool", policy);
            assert!(pool.is_ok());
            let pool = pool.unwrap();
            assert_eq!(pool.id.as_str(), "pool-1");
            assert_eq!(pool.node_count(), 0);
        }

        #[test]
        fn pool_add_and_remove_nodes() {
            let policy = create_test_policy();
            let mut pool = NodePool::new("pool-2", "Test Pool", policy).unwrap();

            pool.add_node(NodeInfo::new("node-1", "A100", 8));
            pool.add_node(NodeInfo::new("node-2", "A100", 8));

            assert_eq!(pool.node_count(), 2);
            assert_eq!(pool.total_gpu_count(), 16);

            let removed = pool.remove_node(&NodeId::new("node-1"));
            assert!(removed.is_some());
            assert_eq!(pool.node_count(), 1);

            let not_found = pool.remove_node(&NodeId::new("node-999"));
            assert!(not_found.is_none());
        }

        #[test]
        fn pool_ready_node_count() {
            let policy = create_test_policy();
            let mut pool = NodePool::new("pool-3", "Test Pool", policy).unwrap();

            pool.add_node(NodeInfo::new("node-1", "A100", 8));
            pool.add_node(NodeInfo::new("node-2", "A100", 8).with_status(NodeStatus::Draining));
            pool.add_node(NodeInfo::new("node-3", "A100", 8).with_status(NodeStatus::Pending));

            assert_eq!(pool.node_count(), 3);
            assert_eq!(pool.ready_node_count(), 1);
        }

        #[test]
        fn pool_cooldown_tracking() {
            let policy = ScalingPolicy::builder("cooldown-test", "Test")
                .min_nodes(1)
                .max_nodes(10)
                .target_utilization(70.0, 10.0)
                .scale_up_cooldown(Duration::from_secs(300))
                .scale_down_cooldown(Duration::from_secs(600))
                .build()
                .unwrap();

            let mut pool = NodePool::new("pool-4", "Test Pool", policy).unwrap();
            let now = Utc::now();

            // Initially, can scale
            assert!(pool.can_scale_up(&now));
            assert!(pool.can_scale_down(&now));

            // Record scale up
            pool.record_scale_up(now);
            assert!(!pool.can_scale_up(&now));
            assert!(pool.can_scale_down(&now));

            // After cooldown, can scale up again
            let future = now + chrono::Duration::seconds(400);
            assert!(pool.can_scale_up(&future));
        }

        #[test]
        fn pool_with_labels() {
            let policy = create_test_policy();
            let pool = NodePool::new("pool-5", "Labeled Pool", policy)
                .unwrap()
                .with_label("env", "production")
                .with_label("gpu_type", "a100");

            assert_eq!(pool.labels.get("env"), Some(&"production".into()));
            assert_eq!(pool.labels.get("gpu_type"), Some(&"a100".into()));
        }

        #[test]
        fn pool_set_policy() {
            let policy1 = create_test_policy();
            let mut pool = NodePool::new("pool-6", "Test", policy1).unwrap();

            let policy2 = ScalingPolicy::builder("new-policy", "New")
                .min_nodes(2)
                .max_nodes(20)
                .target_utilization(80.0, 5.0)
                .build()
                .unwrap();

            let result = pool.set_policy(policy2);
            assert!(result.is_ok());
            assert_eq!(pool.policy.min_nodes, 2);
            assert_eq!(pool.policy.max_nodes, 20);
        }
    }

    mod metrics_snapshot_tests {
        use super::*;

        #[test]
        fn metrics_snapshot_default() {
            let metrics = MetricsSnapshot::new();
            assert!((metrics.avg_gpu_utilization - 0.0).abs() < f64::EPSILON);
            assert_eq!(metrics.queue_depth, 0);
            assert!(metrics.custom.is_empty());
        }

        #[test]
        fn metrics_snapshot_builder_pattern() {
            let metrics = MetricsSnapshot::new()
                .with_gpu_utilization(75.0, 60.0, 90.0)
                .with_queue_depth(25)
                .with_memory_utilization(80.0)
                .with_custom("power_watts", 350.0);

            assert!((metrics.avg_gpu_utilization - 75.0).abs() < f64::EPSILON);
            assert!((metrics.min_gpu_utilization - 60.0).abs() < f64::EPSILON);
            assert!((metrics.max_gpu_utilization - 90.0).abs() < f64::EPSILON);
            assert_eq!(metrics.queue_depth, 25);
            assert!((metrics.avg_memory_utilization - 80.0).abs() < f64::EPSILON);
            assert_eq!(metrics.custom.get("power_watts"), Some(&350.0));
        }
    }

    mod combination_strategy_tests {
        use super::*;

        #[test]
        fn combination_strategy_default() {
            let strategy = CombinationStrategy::default();
            assert_eq!(strategy, CombinationStrategy::Any);
        }

        #[test]
        fn combination_strategy_serialization() {
            for strategy in [
                CombinationStrategy::Any,
                CombinationStrategy::All,
                CombinationStrategy::MostAggressive,
                CombinationStrategy::MostConservative,
            ] {
                let json = serde_json::to_string(&strategy);
                assert!(json.is_ok());
                let parsed: serde_json::Result<CombinationStrategy> =
                    serde_json::from_str(&json.unwrap());
                assert!(parsed.is_ok());
                assert_eq!(parsed.unwrap(), strategy);
            }
        }
    }

    mod node_status_tests {
        use super::*;

        #[test]
        fn node_status_default() {
            let status = NodeStatus::default();
            assert_eq!(status, NodeStatus::Ready);
        }

        #[test]
        fn node_status_serialization() {
            for status in [
                NodeStatus::Ready,
                NodeStatus::Pending,
                NodeStatus::Draining,
                NodeStatus::Unhealthy,
                NodeStatus::Terminating,
            ] {
                let json = serde_json::to_string(&status);
                assert!(json.is_ok());
                let parsed: serde_json::Result<NodeStatus> = serde_json::from_str(&json.unwrap());
                assert!(parsed.is_ok());
                assert_eq!(parsed.unwrap(), status);
            }
        }
    }
}
