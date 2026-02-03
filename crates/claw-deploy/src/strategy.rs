//! Strategy selection for deployments.
//!
//! This module determines the optimal deployment strategy based on:
//! - User intent hints
//! - Environment (production vs dev)
//! - Workload criticality
//! - Cluster context and history

use crate::error::DeployResult;
use crate::types::{DeploymentIntent, DeploymentStrategy, Environment, StrategyHint};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::debug;

/// Context about the cluster for strategy selection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClusterContext {
    /// Environment classification
    pub environment: Environment,

    /// Number of recent deployment failures for this workload
    pub recent_failures: u32,

    /// Total number of active workloads in the cluster
    pub active_workloads: u32,

    /// Whether the cluster is under high load
    pub high_load: bool,

    /// Criticality level of the workload (0-100)
    pub criticality: u32,

    /// Whether this is the first deployment of this image
    pub is_first_deploy: bool,
}

impl ClusterContext {
    /// Creates a new cluster context for a given environment.
    #[must_use]
    pub fn new(environment: Environment) -> Self {
        Self {
            environment,
            ..Default::default()
        }
    }

    /// Sets the number of recent failures.
    #[must_use]
    pub const fn with_recent_failures(mut self, failures: u32) -> Self {
        self.recent_failures = failures;
        self
    }

    /// Sets the criticality level.
    #[must_use]
    pub fn with_criticality(mut self, criticality: u32) -> Self {
        self.criticality = criticality.min(100);
        self
    }

    /// Marks the cluster as under high load.
    #[must_use]
    pub const fn with_high_load(mut self, high_load: bool) -> Self {
        self.high_load = high_load;
        self
    }

    /// Marks this as a first deployment.
    #[must_use]
    pub const fn with_first_deploy(mut self, is_first: bool) -> Self {
        self.is_first_deploy = is_first;
        self
    }
}

/// Selects the optimal deployment strategy based on intent and context.
///
/// # Strategy Selection Rules
///
/// 1. If user provides an explicit hint, honor it (with safety adjustments)
/// 2. Production environments default to Canary
/// 3. Dev environments default to Immediate
/// 4. High criticality (>70) forces Canary or Blue-Green
/// 5. Recent failures increase canary duration
///
/// # Examples
///
/// ```rust
/// use claw_deploy::{select_strategy, ClusterContext, DeploymentIntent, DeploymentStrategy, Environment};
///
/// let intent = DeploymentIntent::new("myapp:v2.0");
/// let context = ClusterContext::new(Environment::Production);
///
/// let strategy = select_strategy(&intent, &context);
/// assert!(strategy.is_ok());
/// // Production defaults to canary
/// assert!(matches!(strategy.unwrap(), DeploymentStrategy::Canary { .. }));
/// ```
///
/// # Errors
///
/// Returns an error if strategy selection fails due to conflicting constraints.
pub fn select_strategy(
    intent: &DeploymentIntent,
    context: &ClusterContext,
) -> DeployResult<DeploymentStrategy> {
    debug!(
        "Selecting strategy for image={} in {:?} environment",
        intent.image, context.environment
    );

    // 1. Check for explicit user hint
    if let Some(ref hint) = intent.strategy_hint {
        let strategy = apply_hint_with_safety(hint, context);
        debug!("Applied user hint: {:?}", strategy);
        return Ok(strategy);
    }

    // 2. Select based on environment and context
    let base_strategy = select_base_strategy(intent, context);

    // 3. Adjust for failures
    let adjusted = adjust_for_failures(base_strategy, context.recent_failures);

    debug!("Selected strategy: {:?}", adjusted);
    Ok(adjusted)
}

/// Applies a user hint with safety adjustments based on context.
fn apply_hint_with_safety(hint: &StrategyHint, context: &ClusterContext) -> DeploymentStrategy {
    match hint {
        StrategyHint::Immediate => {
            // Allow immediate only in dev or if criticality is low
            if context.environment == Environment::Dev || context.criticality < 30 {
                DeploymentStrategy::Immediate
            } else {
                // Override to canary for safety in production
                debug!("Overriding immediate to canary for safety");
                DeploymentStrategy::default_canary()
            }
        }

        StrategyHint::Canary { percentage } => {
            // Ensure minimum canary percentage for high criticality
            let safe_percentage = if context.criticality > 70 {
                (*percentage).min(5) // Max 5% for very critical workloads
            } else {
                *percentage
            };

            // Increase duration if there were recent failures
            let base_duration = Duration::from_secs(300);
            let duration = if context.recent_failures > 0 {
                base_duration * (1 + context.recent_failures)
            } else {
                base_duration
            };

            DeploymentStrategy::Canary {
                percentage: safe_percentage,
                duration,
            }
        }

        StrategyHint::BlueGreen => DeploymentStrategy::BlueGreen,

        StrategyHint::Rolling { batch_size } => DeploymentStrategy::Rolling {
            batch_size: *batch_size,
        },
    }
}

/// Selects base strategy based on environment.
fn select_base_strategy(intent: &DeploymentIntent, context: &ClusterContext) -> DeploymentStrategy {
    // Use intent's environment constraint if set, otherwise use context
    let env = intent
        .constraints
        .environment
        .unwrap_or(context.environment);

    match env {
        Environment::Production => {
            // Production: default to canary
            // High criticality: blue-green for zero-downtime
            if context.criticality > 80 {
                DeploymentStrategy::BlueGreen
            } else {
                DeploymentStrategy::default_canary()
            }
        }

        Environment::Staging => {
            // Staging: smaller canary, faster
            DeploymentStrategy::Canary {
                percentage: 20,
                duration: Duration::from_secs(120),
            }
        }

        Environment::Dev => {
            // Dev: immediate unless high replica count
            if intent.replicas > 5 {
                DeploymentStrategy::default_rolling(intent.replicas)
            } else {
                DeploymentStrategy::Immediate
            }
        }
    }
}

/// Adjusts strategy based on recent failures.
fn adjust_for_failures(strategy: DeploymentStrategy, failures: u32) -> DeploymentStrategy {
    if failures == 0 {
        return strategy;
    }

    match strategy {
        DeploymentStrategy::Immediate => {
            // Failures: switch to canary
            DeploymentStrategy::Canary {
                percentage: 5,
                duration: Duration::from_secs(600), // Longer monitoring
            }
        }

        DeploymentStrategy::Canary { percentage, duration } => {
            // More failures: smaller canary, longer duration
            // Use saturating arithmetic to avoid truncation issues
            let failure_adjustment = u8::try_from(failures.min(127)).unwrap_or(127) * 2;
            let safe_percentage = percentage.saturating_sub(failure_adjustment).max(1);
            let extended_duration = duration * (1 + failures);

            DeploymentStrategy::Canary {
                percentage: safe_percentage,
                duration: extended_duration,
            }
        }

        DeploymentStrategy::Rolling { batch_size } => {
            // More failures: smaller batches
            let safe_batch = batch_size.saturating_sub(failures).max(1);
            DeploymentStrategy::Rolling {
                batch_size: safe_batch,
            }
        }

        // Blue-green is already safe
        DeploymentStrategy::BlueGreen => DeploymentStrategy::BlueGreen,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod select_strategy_tests {
        use super::*;

        #[test]
        fn production_defaults_to_canary() {
            let intent = DeploymentIntent::new("myapp:v1.0");
            let context = ClusterContext::new(Environment::Production);

            let strategy = select_strategy(&intent, &context);
            assert!(strategy.is_ok());
            assert!(matches!(
                strategy.as_ref().ok(),
                Some(DeploymentStrategy::Canary { .. })
            ));
        }

        #[test]
        fn dev_defaults_to_immediate() {
            let intent = DeploymentIntent::new("myapp:v1.0");
            let context = ClusterContext::new(Environment::Dev);

            let strategy = select_strategy(&intent, &context);
            assert!(strategy.is_ok());
            assert!(matches!(
                strategy.as_ref().ok(),
                Some(DeploymentStrategy::Immediate)
            ));
        }

        #[test]
        fn staging_uses_faster_canary() {
            let intent = DeploymentIntent::new("myapp:v1.0");
            let context = ClusterContext::new(Environment::Staging);

            let strategy = select_strategy(&intent, &context);
            assert!(strategy.is_ok());
            if let Some(DeploymentStrategy::Canary { percentage, duration }) = strategy.ok() {
                assert_eq!(percentage, 20);
                assert_eq!(duration, Duration::from_secs(120));
            } else {
                panic!("Expected Canary strategy");
            }
        }

        #[test]
        fn high_criticality_uses_blue_green() {
            let intent = DeploymentIntent::new("myapp:v1.0");
            let context = ClusterContext::new(Environment::Production).with_criticality(90);

            let strategy = select_strategy(&intent, &context);
            assert!(strategy.is_ok());
            assert!(matches!(
                strategy.as_ref().ok(),
                Some(DeploymentStrategy::BlueGreen)
            ));
        }

        #[test]
        fn honors_explicit_canary_hint() {
            let intent = DeploymentIntent::new("myapp:v1.0")
                .with_strategy_hint(StrategyHint::Canary { percentage: 25 });
            let context = ClusterContext::new(Environment::Dev);

            let strategy = select_strategy(&intent, &context);
            assert!(strategy.is_ok());
            if let Some(DeploymentStrategy::Canary { percentage, .. }) = strategy.ok() {
                assert_eq!(percentage, 25);
            } else {
                panic!("Expected Canary strategy");
            }
        }

        #[test]
        fn overrides_immediate_in_production_high_criticality() {
            let intent = DeploymentIntent::new("myapp:v1.0")
                .with_strategy_hint(StrategyHint::Immediate);
            let context = ClusterContext::new(Environment::Production).with_criticality(50);

            let strategy = select_strategy(&intent, &context);
            assert!(strategy.is_ok());
            // Should override to canary for safety
            assert!(matches!(
                strategy.as_ref().ok(),
                Some(DeploymentStrategy::Canary { .. })
            ));
        }

        #[test]
        fn allows_immediate_in_dev() {
            let intent = DeploymentIntent::new("myapp:v1.0")
                .with_strategy_hint(StrategyHint::Immediate);
            let context = ClusterContext::new(Environment::Dev);

            let strategy = select_strategy(&intent, &context);
            assert!(strategy.is_ok());
            assert!(matches!(
                strategy.as_ref().ok(),
                Some(DeploymentStrategy::Immediate)
            ));
        }

        #[test]
        fn dev_with_many_replicas_uses_rolling() {
            let intent = DeploymentIntent::new("myapp:v1.0").with_replicas(10);
            let context = ClusterContext::new(Environment::Dev);

            let strategy = select_strategy(&intent, &context);
            assert!(strategy.is_ok());
            assert!(matches!(
                strategy.as_ref().ok(),
                Some(DeploymentStrategy::Rolling { .. })
            ));
        }

        #[test]
        fn honors_rolling_hint() {
            let intent = DeploymentIntent::new("myapp:v1.0")
                .with_strategy_hint(StrategyHint::Rolling { batch_size: 5 });
            let context = ClusterContext::new(Environment::Production);

            let strategy = select_strategy(&intent, &context);
            assert!(strategy.is_ok());
            if let Some(DeploymentStrategy::Rolling { batch_size }) = strategy.ok() {
                assert_eq!(batch_size, 5);
            } else {
                panic!("Expected Rolling strategy");
            }
        }
    }

    mod adjust_for_failures_tests {
        use super::*;

        #[test]
        fn no_adjustment_without_failures() {
            let strategy = DeploymentStrategy::Immediate;
            let adjusted = adjust_for_failures(strategy, 0);
            assert!(matches!(adjusted, DeploymentStrategy::Immediate));
        }

        #[test]
        fn immediate_becomes_canary_with_failures() {
            let strategy = DeploymentStrategy::Immediate;
            let adjusted = adjust_for_failures(strategy, 1);
            assert!(matches!(adjusted, DeploymentStrategy::Canary { .. }));
        }

        #[test]
        fn canary_percentage_decreases_with_failures() {
            let strategy = DeploymentStrategy::Canary {
                percentage: 10,
                duration: Duration::from_secs(300),
            };

            let adjusted = adjust_for_failures(strategy, 2);
            if let DeploymentStrategy::Canary { percentage, .. } = adjusted {
                assert!(percentage < 10);
            } else {
                panic!("Expected Canary");
            }
        }

        #[test]
        fn canary_duration_increases_with_failures() {
            let strategy = DeploymentStrategy::Canary {
                percentage: 10,
                duration: Duration::from_secs(300),
            };

            let adjusted = adjust_for_failures(strategy, 2);
            if let DeploymentStrategy::Canary { duration, .. } = adjusted {
                assert!(duration > Duration::from_secs(300));
            } else {
                panic!("Expected Canary");
            }
        }

        #[test]
        fn rolling_batch_size_decreases_with_failures() {
            let strategy = DeploymentStrategy::Rolling { batch_size: 5 };
            let adjusted = adjust_for_failures(strategy, 2);
            if let DeploymentStrategy::Rolling { batch_size } = adjusted {
                assert!(batch_size < 5);
            } else {
                panic!("Expected Rolling");
            }
        }

        #[test]
        fn batch_size_minimum_is_one() {
            let strategy = DeploymentStrategy::Rolling { batch_size: 2 };
            let adjusted = adjust_for_failures(strategy, 10);
            if let DeploymentStrategy::Rolling { batch_size } = adjusted {
                assert_eq!(batch_size, 1);
            } else {
                panic!("Expected Rolling");
            }
        }
    }

    mod high_criticality_canary_tests {
        use super::*;

        #[test]
        fn limits_canary_percentage_for_critical_workloads() {
            let intent = DeploymentIntent::new("myapp:v1.0")
                .with_strategy_hint(StrategyHint::Canary { percentage: 20 });
            let context = ClusterContext::new(Environment::Production).with_criticality(80);

            let strategy = select_strategy(&intent, &context);
            assert!(strategy.is_ok());
            if let Some(DeploymentStrategy::Canary { percentage, .. }) = strategy.ok() {
                assert!(percentage <= 5, "Expected max 5% for critical workload");
            } else {
                panic!("Expected Canary");
            }
        }

        #[test]
        fn increases_duration_after_failures() {
            let intent = DeploymentIntent::new("myapp:v1.0")
                .with_strategy_hint(StrategyHint::Canary { percentage: 10 });
            let context = ClusterContext::new(Environment::Production).with_recent_failures(2);

            let strategy = select_strategy(&intent, &context);
            assert!(strategy.is_ok());
            if let Some(DeploymentStrategy::Canary { duration, .. }) = strategy.ok() {
                assert!(duration > Duration::from_secs(300));
            } else {
                panic!("Expected Canary");
            }
        }
    }

    mod cluster_context_tests {
        use super::*;

        #[test]
        fn builder_pattern_works() {
            let context = ClusterContext::new(Environment::Production)
                .with_criticality(80)
                .with_recent_failures(2)
                .with_high_load(true)
                .with_first_deploy(true);

            assert_eq!(context.environment, Environment::Production);
            assert_eq!(context.criticality, 80);
            assert_eq!(context.recent_failures, 2);
            assert!(context.high_load);
            assert!(context.is_first_deploy);
        }

        #[test]
        fn criticality_capped_at_100() {
            let context = ClusterContext::new(Environment::Dev).with_criticality(150);
            assert_eq!(context.criticality, 100);
        }
    }
}
