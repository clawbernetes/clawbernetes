//! Core types for the intent-based deployment system.
//!
//! This module defines the fundamental types used throughout the deployment pipeline:
//! - `DeploymentId`: Unique identifier for deployments
//! - `DeploymentIntent`: What the user wants to deploy
//! - `DeploymentStrategy`: How to deploy (canary, blue-green, etc.)
//! - `DeploymentState`: Current state of a deployment
//! - `DeploymentStatus`: Full status including health info

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;
use uuid::Uuid;

use crate::error::{DeployError, DeployResult};

/// Unique identifier for a deployment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct DeploymentId(Uuid);

impl DeploymentId {
    /// Creates a new random deployment ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates a deployment ID from an existing UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the inner UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    /// Parses a deployment ID from a string.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid UUID.
    pub fn parse(s: &str) -> DeployResult<Self> {
        let uuid = Uuid::parse_str(s).map_err(|e| DeployError::InvalidId(e.to_string()))?;
        Ok(Self(uuid))
    }
}

impl fmt::Display for DeploymentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Represents what the user intends to deploy.
///
/// This is the parsed result of natural language deployment commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeploymentIntent {
    /// Container image to deploy (e.g., "myapp:v2.0")
    pub image: String,

    /// Number of replicas to deploy
    pub replicas: u32,

    /// Number of GPUs required per replica (0 for CPU-only workloads)
    pub gpus: u32,

    /// Additional constraints for placement and scheduling
    pub constraints: DeploymentConstraints,

    /// Hint about desired deployment strategy
    pub strategy_hint: Option<StrategyHint>,
}

impl DeploymentIntent {
    /// Creates a new deployment intent with the minimum required fields.
    #[must_use]
    pub fn new(image: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            replicas: 1,
            gpus: 0,
            constraints: DeploymentConstraints::default(),
            strategy_hint: None,
        }
    }

    /// Sets the number of replicas.
    #[must_use]
    pub const fn with_replicas(mut self, replicas: u32) -> Self {
        self.replicas = replicas;
        self
    }

    /// Sets the number of GPUs per replica.
    #[must_use]
    pub const fn with_gpus(mut self, gpus: u32) -> Self {
        self.gpus = gpus;
        self
    }

    /// Sets the constraints.
    #[must_use]
    pub fn with_constraints(mut self, constraints: DeploymentConstraints) -> Self {
        self.constraints = constraints;
        self
    }

    /// Sets the strategy hint.
    #[must_use]
    pub const fn with_strategy_hint(mut self, hint: StrategyHint) -> Self {
        self.strategy_hint = Some(hint);
        self
    }

    /// Validates the deployment intent.
    ///
    /// # Errors
    ///
    /// Returns an error if the intent is invalid (e.g., empty image name).
    pub fn validate(&self) -> DeployResult<()> {
        if self.image.is_empty() {
            return Err(DeployError::InvalidIntent("image cannot be empty".to_string()));
        }
        if self.replicas == 0 {
            return Err(DeployError::InvalidIntent(
                "replicas must be at least 1".to_string(),
            ));
        }
        Ok(())
    }
}

/// Constraints for deployment placement and scheduling.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DeploymentConstraints {
    /// Minimum memory in MB per replica
    pub min_memory_mb: Option<u64>,

    /// Minimum CPU cores per replica
    pub min_cpu_cores: Option<u32>,

    /// Required node labels
    pub node_labels: HashMap<String, String>,

    /// Environment (production, staging, dev)
    pub environment: Option<Environment>,

    /// Maximum error rate before rollback (as a percentage, e.g., 1.0 = 1%)
    pub max_error_rate: Option<f64>,

    /// Region constraints
    pub regions: Vec<String>,
}

/// Environment classification for deployments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    /// Development environment - fast deploys, less safety
    #[default]
    Dev,
    /// Staging environment - production-like but isolated
    Staging,
    /// Production environment - full safety measures
    Production,
}

/// Hint about the desired deployment strategy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyHint {
    /// Deploy immediately with no gradual rollout
    Immediate,
    /// Canary deployment with specified percentage
    Canary {
        /// Percentage of traffic to send to canary (1-100)
        percentage: u8,
    },
    /// Blue-green deployment
    BlueGreen,
    /// Rolling update with batch size
    Rolling {
        /// Number of replicas to update at a time
        batch_size: u32,
    },
}

/// The actual deployment strategy to execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeploymentStrategy {
    /// Deploy all replicas immediately
    Immediate,

    /// Canary deployment: deploy to a subset, monitor, then promote
    Canary {
        /// Percentage of traffic to send to canary (1-100)
        percentage: u8,
        /// How long to run the canary before auto-promotion
        duration: Duration,
    },

    /// Blue-green: deploy new version alongside old, then switch
    BlueGreen,

    /// Rolling update: update replicas in batches
    Rolling {
        /// Number of replicas to update at once
        batch_size: u32,
    },
}

impl DeploymentStrategy {
    /// Creates a default canary strategy (10% for 5 minutes).
    #[must_use]
    pub const fn default_canary() -> Self {
        Self::Canary {
            percentage: 10,
            duration: Duration::from_secs(300),
        }
    }

    /// Creates a default rolling strategy.
    #[must_use]
    pub fn default_rolling(total_replicas: u32) -> Self {
        let batch_size = std::cmp::max(1, total_replicas / 4);
        Self::Rolling { batch_size }
    }

    /// Returns true if this strategy requires gradual rollout monitoring.
    #[must_use]
    pub const fn requires_monitoring(&self) -> bool {
        matches!(self, Self::Canary { .. } | Self::BlueGreen | Self::Rolling { .. })
    }
}

/// Current state of a deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentState {
    /// Deployment is queued but not started
    Pending,
    /// Deployment is in progress
    Deploying,
    /// Canary is running and being monitored
    Canary,
    /// Canary is being promoted to full deployment
    Promoting,
    /// Deployment is being rolled back
    RollingBack,
    /// Deployment completed successfully
    Complete,
    /// Deployment failed
    Failed,
}

impl DeploymentState {
    /// Returns true if the deployment is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Complete | Self::Failed)
    }

    /// Returns true if the deployment can be promoted.
    #[must_use]
    pub const fn can_promote(&self) -> bool {
        matches!(self, Self::Canary)
    }

    /// Returns true if the deployment can be rolled back.
    #[must_use]
    pub const fn can_rollback(&self) -> bool {
        matches!(
            self,
            Self::Deploying | Self::Canary | Self::Promoting | Self::RollingBack
        )
    }
}

impl fmt::Display for DeploymentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Pending => "pending",
            Self::Deploying => "deploying",
            Self::Canary => "canary",
            Self::Promoting => "promoting",
            Self::RollingBack => "rolling_back",
            Self::Complete => "complete",
            Self::Failed => "failed",
        };
        write!(f, "{s}")
    }
}

/// Full status of a deployment including health information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentStatus {
    /// Current state of the deployment
    pub state: DeploymentState,

    /// Number of healthy replicas
    pub healthy_replicas: u32,

    /// Total number of replicas (target)
    pub total_replicas: u32,

    /// Optional status message
    pub message: Option<String>,

    /// When the deployment was created
    pub created_at: DateTime<Utc>,

    /// When the deployment was last updated
    pub updated_at: DateTime<Utc>,

    /// The image being deployed
    pub image: String,

    /// The strategy being used
    pub strategy: DeploymentStrategy,
}

impl DeploymentStatus {
    /// Creates a new pending deployment status.
    #[must_use]
    pub fn new_pending(image: String, total_replicas: u32, strategy: DeploymentStrategy) -> Self {
        let now = Utc::now();
        Self {
            state: DeploymentState::Pending,
            healthy_replicas: 0,
            total_replicas,
            message: None,
            created_at: now,
            updated_at: now,
            image,
            strategy,
        }
    }

    /// Transitions to a new state.
    #[must_use]
    pub fn with_state(mut self, state: DeploymentState) -> Self {
        self.state = state;
        self.updated_at = Utc::now();
        self
    }

    /// Updates the healthy replica count.
    #[must_use]
    pub fn with_healthy_replicas(mut self, count: u32) -> Self {
        self.healthy_replicas = count;
        self.updated_at = Utc::now();
        self
    }

    /// Sets a status message.
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self.updated_at = Utc::now();
        self
    }

    /// Returns the health ratio (0.0 to 1.0).
    #[must_use]
    pub fn health_ratio(&self) -> f64 {
        if self.total_replicas == 0 {
            return 0.0;
        }
        f64::from(self.healthy_replicas) / f64::from(self.total_replicas)
    }

    /// Returns true if all replicas are healthy.
    #[must_use]
    pub const fn is_fully_healthy(&self) -> bool {
        self.healthy_replicas >= self.total_replicas
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod deployment_id {
        use super::*;

        #[test]
        fn new_creates_unique_ids() {
            let id1 = DeploymentId::new();
            let id2 = DeploymentId::new();
            assert_ne!(id1, id2);
        }

        #[test]
        fn from_uuid_preserves_value() {
            let uuid = Uuid::new_v4();
            let id = DeploymentId::from_uuid(uuid);
            assert_eq!(*id.as_uuid(), uuid);
        }

        #[test]
        fn parse_valid_uuid() {
            let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
            let id = DeploymentId::parse(uuid_str);
            assert!(id.is_ok());
            assert_eq!(id.as_ref().ok().map(ToString::to_string).as_deref(), Some(uuid_str));
        }

        #[test]
        fn parse_invalid_uuid_returns_error() {
            let result = DeploymentId::parse("not-a-uuid");
            assert!(result.is_err());
        }

        #[test]
        fn display_formats_as_uuid() {
            let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").ok();
            let id = uuid.map(DeploymentId::from_uuid);
            assert_eq!(
                id.as_ref().map(ToString::to_string).as_deref(),
                Some("550e8400-e29b-41d4-a716-446655440000")
            );
        }

        #[test]
        fn serialization_roundtrip() {
            let id = DeploymentId::new();
            let json = serde_json::to_string(&id);
            assert!(json.is_ok());
            let parsed: Result<DeploymentId, _> =
                json.as_ref().map_or_else(|_| Err("no json".to_string()), |j| {
                    serde_json::from_str(j).map_err(|e| e.to_string())
                });
            assert_eq!(parsed.ok(), Some(id));
        }
    }

    mod deployment_intent {
        use super::*;

        #[test]
        fn new_creates_with_defaults() {
            let intent = DeploymentIntent::new("myapp:v1.0");
            assert_eq!(intent.image, "myapp:v1.0");
            assert_eq!(intent.replicas, 1);
            assert_eq!(intent.gpus, 0);
            assert!(intent.strategy_hint.is_none());
        }

        #[test]
        fn builder_pattern_works() {
            let intent = DeploymentIntent::new("myapp:v2.0")
                .with_replicas(5)
                .with_gpus(2)
                .with_strategy_hint(StrategyHint::Canary { percentage: 20 });

            assert_eq!(intent.image, "myapp:v2.0");
            assert_eq!(intent.replicas, 5);
            assert_eq!(intent.gpus, 2);
            assert_eq!(
                intent.strategy_hint,
                Some(StrategyHint::Canary { percentage: 20 })
            );
        }

        #[test]
        fn validate_rejects_empty_image() {
            let intent = DeploymentIntent::new("");
            let result = intent.validate();
            assert!(result.is_err());
        }

        #[test]
        fn validate_rejects_zero_replicas() {
            let intent = DeploymentIntent::new("myapp:v1.0").with_replicas(0);
            let result = intent.validate();
            assert!(result.is_err());
        }

        #[test]
        fn validate_accepts_valid_intent() {
            let intent = DeploymentIntent::new("myapp:v1.0").with_replicas(3);
            let result = intent.validate();
            assert!(result.is_ok());
        }

        #[test]
        fn serialization_roundtrip() {
            let intent = DeploymentIntent::new("myapp:v1.0")
                .with_replicas(3)
                .with_gpus(1);

            let json = serde_json::to_string(&intent);
            assert!(json.is_ok());
            let parsed: Result<DeploymentIntent, _> =
                json.as_ref().map_or_else(|_| Err("no json".to_string()), |j| {
                    serde_json::from_str(j).map_err(|e| e.to_string())
                });
            assert_eq!(parsed.ok(), Some(intent));
        }
    }

    mod deployment_strategy {
        use super::*;

        #[test]
        fn default_canary_has_expected_values() {
            let strategy = DeploymentStrategy::default_canary();
            if let DeploymentStrategy::Canary { percentage, duration } = strategy {
                assert_eq!(percentage, 10);
                assert_eq!(duration, Duration::from_secs(300));
            } else {
                panic!("Expected Canary strategy");
            }
        }

        #[test]
        fn default_rolling_calculates_batch_size() {
            let strategy = DeploymentStrategy::default_rolling(12);
            if let DeploymentStrategy::Rolling { batch_size } = strategy {
                assert_eq!(batch_size, 3); // 12 / 4 = 3
            } else {
                panic!("Expected Rolling strategy");
            }
        }

        #[test]
        fn default_rolling_minimum_batch_size_is_one() {
            let strategy = DeploymentStrategy::default_rolling(2);
            if let DeploymentStrategy::Rolling { batch_size } = strategy {
                assert_eq!(batch_size, 1); // max(1, 2/4=0) = 1
            } else {
                panic!("Expected Rolling strategy");
            }
        }

        #[test]
        fn requires_monitoring_for_gradual_strategies() {
            assert!(!DeploymentStrategy::Immediate.requires_monitoring());
            assert!(DeploymentStrategy::default_canary().requires_monitoring());
            assert!(DeploymentStrategy::BlueGreen.requires_monitoring());
            assert!(DeploymentStrategy::default_rolling(10).requires_monitoring());
        }

        #[test]
        fn serialization_roundtrip() {
            let strategies = vec![
                DeploymentStrategy::Immediate,
                DeploymentStrategy::default_canary(),
                DeploymentStrategy::BlueGreen,
                DeploymentStrategy::Rolling { batch_size: 5 },
            ];

            for strategy in strategies {
                let json = serde_json::to_string(&strategy);
                assert!(json.is_ok(), "Failed to serialize {:?}", strategy);
                let parsed: Result<DeploymentStrategy, _> =
                    json.as_ref().map_or_else(|_| Err("no json".to_string()), |j| {
                        serde_json::from_str(j).map_err(|e| e.to_string())
                    });
                assert_eq!(parsed.ok(), Some(strategy));
            }
        }
    }

    mod deployment_state {
        use super::*;

        #[test]
        fn is_terminal_for_complete_and_failed() {
            assert!(!DeploymentState::Pending.is_terminal());
            assert!(!DeploymentState::Deploying.is_terminal());
            assert!(!DeploymentState::Canary.is_terminal());
            assert!(!DeploymentState::Promoting.is_terminal());
            assert!(!DeploymentState::RollingBack.is_terminal());
            assert!(DeploymentState::Complete.is_terminal());
            assert!(DeploymentState::Failed.is_terminal());
        }

        #[test]
        fn can_promote_only_from_canary() {
            assert!(!DeploymentState::Pending.can_promote());
            assert!(!DeploymentState::Deploying.can_promote());
            assert!(DeploymentState::Canary.can_promote());
            assert!(!DeploymentState::Promoting.can_promote());
            assert!(!DeploymentState::RollingBack.can_promote());
            assert!(!DeploymentState::Complete.can_promote());
            assert!(!DeploymentState::Failed.can_promote());
        }

        #[test]
        fn can_rollback_from_active_states() {
            assert!(!DeploymentState::Pending.can_rollback());
            assert!(DeploymentState::Deploying.can_rollback());
            assert!(DeploymentState::Canary.can_rollback());
            assert!(DeploymentState::Promoting.can_rollback());
            assert!(DeploymentState::RollingBack.can_rollback());
            assert!(!DeploymentState::Complete.can_rollback());
            assert!(!DeploymentState::Failed.can_rollback());
        }

        #[test]
        fn display_formats_correctly() {
            assert_eq!(DeploymentState::Pending.to_string(), "pending");
            assert_eq!(DeploymentState::RollingBack.to_string(), "rolling_back");
            assert_eq!(DeploymentState::Complete.to_string(), "complete");
        }
    }

    mod deployment_status {
        use super::*;

        #[test]
        fn new_pending_initializes_correctly() {
            let status = DeploymentStatus::new_pending(
                "myapp:v1.0".to_string(),
                5,
                DeploymentStrategy::Immediate,
            );

            assert_eq!(status.state, DeploymentState::Pending);
            assert_eq!(status.healthy_replicas, 0);
            assert_eq!(status.total_replicas, 5);
            assert!(status.message.is_none());
            assert_eq!(status.image, "myapp:v1.0");
        }

        #[test]
        fn with_state_updates_timestamp() {
            let status = DeploymentStatus::new_pending(
                "myapp:v1.0".to_string(),
                5,
                DeploymentStrategy::Immediate,
            );
            let original_updated = status.updated_at;

            // Small delay to ensure timestamp changes
            std::thread::sleep(std::time::Duration::from_millis(1));

            let updated = status.with_state(DeploymentState::Deploying);
            assert_eq!(updated.state, DeploymentState::Deploying);
            assert!(updated.updated_at >= original_updated);
        }

        #[test]
        fn health_ratio_calculates_correctly() {
            let status = DeploymentStatus::new_pending(
                "myapp:v1.0".to_string(),
                10,
                DeploymentStrategy::Immediate,
            )
            .with_healthy_replicas(7);

            let ratio = status.health_ratio();
            assert!((ratio - 0.7).abs() < f64::EPSILON);
        }

        #[test]
        fn health_ratio_handles_zero_replicas() {
            let status = DeploymentStatus::new_pending(
                "myapp:v1.0".to_string(),
                0,
                DeploymentStrategy::Immediate,
            );
            assert!((status.health_ratio() - 0.0).abs() < f64::EPSILON);
        }

        #[test]
        fn is_fully_healthy_when_all_replicas_healthy() {
            let status = DeploymentStatus::new_pending(
                "myapp:v1.0".to_string(),
                5,
                DeploymentStrategy::Immediate,
            )
            .with_healthy_replicas(5);

            assert!(status.is_fully_healthy());
        }

        #[test]
        fn not_fully_healthy_when_some_replicas_unhealthy() {
            let status = DeploymentStatus::new_pending(
                "myapp:v1.0".to_string(),
                5,
                DeploymentStrategy::Immediate,
            )
            .with_healthy_replicas(4);

            assert!(!status.is_fully_healthy());
        }

        #[test]
        fn serialization_roundtrip() {
            let status = DeploymentStatus::new_pending(
                "myapp:v1.0".to_string(),
                5,
                DeploymentStrategy::default_canary(),
            )
            .with_state(DeploymentState::Canary)
            .with_healthy_replicas(1)
            .with_message("Canary running");

            let json = serde_json::to_string(&status);
            assert!(json.is_ok());
            let parsed: Result<DeploymentStatus, _> =
                json.as_ref().map_or_else(|_| Err("no json".to_string()), |j| {
                    serde_json::from_str(j).map_err(|e| e.to_string())
                });
            // Note: timestamps will differ slightly, so we compare key fields
            let parsed = parsed.ok();
            assert!(parsed.is_some());
            let parsed = parsed.as_ref();
            assert_eq!(parsed.map(|p| &p.state), Some(&DeploymentState::Canary));
            assert_eq!(parsed.map(|p| p.healthy_replicas), Some(1));
        }
    }
}
