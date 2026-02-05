//! Workload preemption and priority scheduling for Clawbernetes.
//!
//! `claw-preemption` provides cost optimization through intelligent workload
//! preemption, supporting spot instances and priority-based scheduling.
//!
//! # Features
//!
//! - **Priority Classes**: Built-in and custom priority classes (0-1000)
//! - **Preemption Policies**: Control which workloads can be preempted
//! - **Victim Selection**: Multiple strategies for selecting preemption targets
//! - **Graceful Eviction**: Configurable grace periods for clean shutdown
//! - **Cost Tracking**: Track preemption costs for optimization decisions
//!
//! # Built-in Priority Classes
//!
//! | Name | Value | Policy | Description |
//! |------|-------|--------|-------------|
//! | `system-critical` | 1000 | Never | Critical system workloads |
//! | `high-priority` | 750 | `PreemptLower` | High priority workloads |
//! | `default` | 500 | `PreemptLower` | Standard workloads |
//! | `spot` | 100 | `PreemptLower` | Low-cost spot workloads |
//! | `preemptible` | 0 | `PreemptLower` | Always preemptible |
//!
//! # Example
//!
//! ```rust
//! use claw_preemption::{
//!     PreemptionCandidate, PreemptionConfig, PreemptionManager,
//!     PreemptionRequest, PriorityClass, ResourceRequirements,
//!     WorkloadId, NoOpEvictionHandler,
//! };
//!
//! // Create a preemption manager
//! let handler = NoOpEvictionHandler::new();
//! let manager = PreemptionManager::with_defaults(handler);
//!
//! // Register some workloads
//! let spot_workload = PreemptionCandidate::new(
//!     WorkloadId::new("ml-training-1"),
//!     PriorityClass::spot(),
//! )
//! .with_resources(
//!     ResourceRequirements::new()
//!         .with_gpus(4)
//!         .with_memory_gb(32)
//! );
//!
//! manager.register_workload(spot_workload);
//!
//! // Request preemption when a high-priority job needs resources
//! let request = PreemptionRequest::new(
//!     ResourceRequirements::new().with_gpus(4),
//!     PriorityClass::high_priority(),
//! );
//!
//! match manager.request_preemption(&request) {
//!     Ok(result) => {
//!         println!("Evicted {} workloads", result.evicted_count());
//!         println!("Freed {} GPUs", result.freed_resources.gpus);
//!     }
//!     Err(e) => println!("Preemption failed: {}", e),
//! }
//! ```
//!
//! # Custom Priority Classes
//!
//! You can register custom priority classes for your specific needs:
//!
//! ```rust
//! use claw_preemption::{
//!     Preemptor, PreemptionConfig, PriorityClass, PreemptionPolicy,
//!     NoOpEvictionHandler,
//! };
//!
//! let handler = NoOpEvictionHandler::new();
//! let preemptor = Preemptor::with_defaults(handler);
//!
//! // Register a custom priority class
//! let custom_class = PriorityClass::new(
//!     "batch-processing",
//!     300,
//!     PreemptionPolicy::PreemptLowerPriority,
//! )
//! .unwrap()
//! .with_description("Batch processing jobs that can wait");
//!
//! preemptor.register_priority_class(custom_class).unwrap();
//! ```
//!
//! # Victim Selection Strategies
//!
//! Configure how victims are selected when preemption is needed:
//!
//! ```rust
//! use claw_preemption::{PreemptionConfig, VictimSelectionStrategy};
//!
//! let config = PreemptionConfig::new()
//!     .with_victim_selection(VictimSelectionStrategy::Balanced)
//!     .with_min_priority_difference(100)
//!     .with_max_victims(50);
//! ```
//!
//! Available strategies:
//! - `LowestPriority`: Prefer lowest priority workloads (default)
//! - `ShortestRunning`: Prefer workloads with shortest runtime (least work lost)
//! - `LowestCost`: Prefer workloads with lowest preemption cost
//! - `MostResources`: Prefer workloads that free the most resources
//! - `Balanced`: Weighted combination of all factors
//!
//! # Graceful Eviction
//!
//! Workloads are given a configurable grace period for clean shutdown:
//!
//! ```rust
//! use claw_preemption::{
//!     PreemptionCandidate, PriorityClass, WorkloadId,
//! };
//! use std::time::Duration;
//!
//! let workload = PreemptionCandidate::new(
//!     WorkloadId::new("training-job"),
//!     PriorityClass::spot(),
//! )
//! .with_grace_period(Duration::from_secs(60)); // 60 second shutdown
//! ```
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │           PreemptionManager             │
//! │  ┌─────────────────────────────────────┐│
//! │  │          Preemptor                  ││
//! │  │  ┌───────────┐  ┌────────────────┐ ││
//! │  │  │ Priority  │  │    Victim      │ ││
//! │  │  │ Classes   │  │   Selection    │ ││
//! │  │  └───────────┘  └────────────────┘ ││
//! │  └─────────────────────────────────────┘│
//! │  ┌─────────────────────────────────────┐│
//! │  │        Eviction Handler             ││
//! │  │  (evict, kill, is_terminated)       ││
//! │  └─────────────────────────────────────┘│
//! └─────────────────────────────────────────┘
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

pub mod error;
pub mod preemptor;
pub mod types;

// Re-export main types
pub use error::{PreemptionError, Result};
pub use preemptor::{
    EvictionHandler, NoOpEvictionHandler, PreemptionManager, PreemptionRequest, Preemptor,
    VictimSet,
};
pub use types::{
    EvictionFailure, EvictionResult, PreemptionCandidate, PreemptionConfig, PreemptionPolicy,
    PriorityClass, ResourceRequirements, VictimSelectionStrategy, WorkloadId, WorkloadState,
};

/// Prelude for convenient imports.
pub mod prelude {
    pub use crate::error::{PreemptionError, Result};
    pub use crate::preemptor::{
        EvictionHandler, NoOpEvictionHandler, PreemptionManager, PreemptionRequest, Preemptor,
        VictimSet,
    };
    pub use crate::types::{
        EvictionResult, PreemptionCandidate, PreemptionConfig, PreemptionPolicy, PriorityClass,
        ResourceRequirements, VictimSelectionStrategy, WorkloadId, WorkloadState,
    };
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn full_preemption_workflow() {
        // Create manager with default config
        let handler = NoOpEvictionHandler::new();
        let manager = PreemptionManager::with_defaults(handler);

        // Register workloads with different priorities
        let critical_workload = PreemptionCandidate::new(
            WorkloadId::new("critical-system"),
            PriorityClass::system_critical(),
        )
        .with_resources(ResourceRequirements::new().with_gpus(2).with_memory_gb(16));

        let high_workload = PreemptionCandidate::new(
            WorkloadId::new("important-job"),
            PriorityClass::high_priority(),
        )
        .with_resources(ResourceRequirements::new().with_gpus(4).with_memory_gb(32));

        let spot_workload1 = PreemptionCandidate::new(
            WorkloadId::new("spot-training-1"),
            PriorityClass::spot(),
        )
        .with_resources(ResourceRequirements::new().with_gpus(4).with_memory_gb(32))
        .with_preemption_cost(50.0);

        let spot_workload2 = PreemptionCandidate::new(
            WorkloadId::new("spot-training-2"),
            PriorityClass::spot(),
        )
        .with_resources(ResourceRequirements::new().with_gpus(4).with_memory_gb(32))
        .with_preemption_cost(75.0);

        manager.register_workload(critical_workload);
        manager.register_workload(high_workload);
        manager.register_workload(spot_workload1);
        manager.register_workload(spot_workload2);

        assert_eq!(manager.workloads().len(), 4);
        // All except system-critical are preemptible (high-priority, spot1, spot2)
        assert_eq!(manager.preemptible_workloads().len(), 3);

        // Request preemption for a high-priority job that needs 4 GPUs
        let request = PreemptionRequest::new(
            ResourceRequirements::new().with_gpus(4),
            PriorityClass::high_priority(),
        );

        let result = manager.request_preemption(&request);
        assert!(result.is_ok());

        let result = result.ok().unwrap_or_else(EvictionResult::new);
        assert!(result.is_successful());
        assert!(result.freed_resources.gpus >= 4);
        assert!(!result.evicted_workloads.is_empty());

        // Verify eviction history
        let history = manager.eviction_history();
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn priority_class_hierarchy() {
        let critical = PriorityClass::system_critical();
        let high = PriorityClass::high_priority();
        let default = PriorityClass::default_priority();
        let spot = PriorityClass::spot();
        let preemptible = PriorityClass::preemptible();

        // Verify priority values
        assert_eq!(critical.value, 1000);
        assert_eq!(high.value, 750);
        assert_eq!(default.value, 500);
        assert_eq!(spot.value, 100);
        assert_eq!(preemptible.value, 0);

        // Verify can_preempt relationships
        assert!(high.can_preempt(&default));
        assert!(high.can_preempt(&spot));
        assert!(high.can_preempt(&preemptible));

        assert!(default.can_preempt(&spot));
        assert!(default.can_preempt(&preemptible));

        assert!(spot.can_preempt(&preemptible));

        // Cannot preempt system-critical
        assert!(!high.can_preempt(&critical));
        assert!(!default.can_preempt(&critical));

        // Cannot preempt same or higher
        assert!(!default.can_preempt(&high));
        assert!(!spot.can_preempt(&default));

        // System-critical cannot preempt (Never policy)
        assert!(!critical.can_preempt(&preemptible));
    }

    #[test]
    fn resource_based_victim_selection() {
        let handler = NoOpEvictionHandler::new();
        let config = PreemptionConfig::new()
            .with_victim_selection(VictimSelectionStrategy::MostResources);
        let preemptor = Preemptor::new(config, handler);

        let candidates = vec![
            PreemptionCandidate::new(WorkloadId::new("small"), PriorityClass::spot())
                .with_resources(ResourceRequirements::new().with_gpus(1)),
            PreemptionCandidate::new(WorkloadId::new("large"), PriorityClass::spot())
                .with_resources(ResourceRequirements::new().with_gpus(8)),
            PreemptionCandidate::new(WorkloadId::new("medium"), PriorityClass::spot())
                .with_resources(ResourceRequirements::new().with_gpus(4)),
        ];

        let request = PreemptionRequest::new(
            ResourceRequirements::new().with_gpus(8),
            PriorityClass::high_priority(),
        );

        let victim_set = preemptor.find_victims(&request, &candidates);

        assert!(victim_set.satisfies_request);
        // Should select "large" first as it has most resources
        assert_eq!(victim_set.victims[0].workload_id.as_str(), "large");
    }

    #[test]
    fn graceful_eviction_with_custom_period() {
        let handler = NoOpEvictionHandler::new();
        let config = PreemptionConfig::new()
            .with_default_grace_period(Duration::from_secs(30))
            .with_max_grace_period(Duration::from_secs(120));
        let preemptor = Preemptor::new(config, handler);

        // Create workload with custom grace period
        let workload = PreemptionCandidate::new(
            WorkloadId::new("custom-grace"),
            PriorityClass::spot(),
        )
        .with_resources(ResourceRequirements::new().with_gpus(4))
        .with_grace_period(Duration::from_secs(60));

        let result = preemptor.evict(&[workload]);
        assert!(result.is_ok());

        let result = result.ok().unwrap_or_else(EvictionResult::new);
        assert!(result.is_successful());
        assert_eq!(result.evicted_count(), 1);
    }

    #[test]
    fn cost_limited_preemption() {
        let handler = NoOpEvictionHandler::new();
        let manager = PreemptionManager::with_defaults(handler);

        // Register workloads with varying costs
        let cheap = PreemptionCandidate::new(WorkloadId::new("cheap"), PriorityClass::spot())
            .with_resources(ResourceRequirements::new().with_gpus(2))
            .with_preemption_cost(10.0);

        let expensive = PreemptionCandidate::new(WorkloadId::new("expensive"), PriorityClass::spot())
            .with_resources(ResourceRequirements::new().with_gpus(2))
            .with_preemption_cost(1000.0);

        manager.register_workload(cheap);
        manager.register_workload(expensive);

        // Request with cost limit
        let request = PreemptionRequest::new(
            ResourceRequirements::new().with_gpus(2),
            PriorityClass::high_priority(),
        )
        .with_max_cost(50.0);

        let result = manager.request_preemption(&request);
        assert!(result.is_ok());

        let result = result.ok().unwrap_or_else(EvictionResult::new);
        // Should only evict the cheap workload
        assert_eq!(result.evicted_count(), 1);
        assert!(result.total_cost <= 50.0);
    }

    #[test]
    fn node_specific_preemption() {
        let handler = NoOpEvictionHandler::new();
        let manager = PreemptionManager::with_defaults(handler);

        // Register workloads on different nodes
        let node1_workload = PreemptionCandidate::new(
            WorkloadId::new("node1-job"),
            PriorityClass::spot(),
        )
        .with_resources(ResourceRequirements::new().with_gpus(4))
        .with_node("node-1");

        let node2_workload = PreemptionCandidate::new(
            WorkloadId::new("node2-job"),
            PriorityClass::spot(),
        )
        .with_resources(ResourceRequirements::new().with_gpus(4))
        .with_node("node-2");

        manager.register_workload(node1_workload);
        manager.register_workload(node2_workload);

        // Request preemption specifically on node-1
        let request = PreemptionRequest::new(
            ResourceRequirements::new().with_gpus(4),
            PriorityClass::high_priority(),
        )
        .with_node("node-1");

        let candidates = manager.preemptible_workloads();
        let victim_set = manager.preemptor().find_victims(&request, &candidates);

        assert!(victim_set.satisfies_request);
        assert!(victim_set
            .victims
            .iter()
            .all(|v| v.node_id == Some("node-1".into())));
    }

    #[test]
    fn custom_priority_class_registration() {
        let handler = NoOpEvictionHandler::new();
        let preemptor = Preemptor::with_defaults(handler);

        // Register custom priority classes
        let batch = PriorityClass::new("batch", 250, PreemptionPolicy::PreemptLowerPriority)
            .ok()
            .unwrap_or_else(PriorityClass::default)
            .with_description("Batch processing jobs");

        let interactive = PriorityClass::new("interactive", 800, PreemptionPolicy::PreemptLowerPriority)
            .ok()
            .unwrap_or_else(PriorityClass::default)
            .with_description("User-facing interactive jobs");

        assert!(preemptor.register_priority_class(batch).is_ok());
        assert!(preemptor.register_priority_class(interactive).is_ok());

        // Verify they're registered
        let batch_class = preemptor.get_priority_class("batch");
        assert!(batch_class.is_some());
        assert_eq!(batch_class.unwrap_or_else(PriorityClass::default).value, 250);

        let interactive_class = preemptor.get_priority_class("interactive");
        assert!(interactive_class.is_some());
        assert_eq!(interactive_class.unwrap_or_else(PriorityClass::default).value, 800);

        // Verify preemption works with custom classes
        let interactive_pc = preemptor
            .get_priority_class("interactive")
            .unwrap_or_else(PriorityClass::default);
        let batch_pc = preemptor
            .get_priority_class("batch")
            .unwrap_or_else(PriorityClass::default);

        assert!(interactive_pc.can_preempt(&batch_pc));
    }

    #[test]
    fn workload_state_transitions() {
        let handler = NoOpEvictionHandler::new();
        let manager = PreemptionManager::with_defaults(handler);

        let workload = PreemptionCandidate::new(
            WorkloadId::new("stateful-job"),
            PriorityClass::spot(),
        )
        .with_resources(ResourceRequirements::new().with_gpus(2));

        manager.register_workload(workload);

        // Verify initial state
        let w = manager
            .get_workload(&WorkloadId::new("stateful-job"))
            .unwrap_or_else(|| PreemptionCandidate::new(WorkloadId::new(""), PriorityClass::default_priority()));
        assert_eq!(w.state, WorkloadState::Running);
        assert!(w.can_be_preempted());

        // Transition to evicting
        manager.update_workload_state(&WorkloadId::new("stateful-job"), WorkloadState::Evicting);

        let w = manager
            .get_workload(&WorkloadId::new("stateful-job"))
            .unwrap_or_else(|| PreemptionCandidate::new(WorkloadId::new(""), PriorityClass::default_priority()));
        assert_eq!(w.state, WorkloadState::Evicting);
        assert!(!w.can_be_preempted()); // Cannot preempt while evicting

        // Transition to evicted
        manager.update_workload_state(&WorkloadId::new("stateful-job"), WorkloadState::Evicted);

        let w = manager
            .get_workload(&WorkloadId::new("stateful-job"))
            .unwrap_or_else(|| PreemptionCandidate::new(WorkloadId::new(""), PriorityClass::default_priority()));
        assert_eq!(w.state, WorkloadState::Evicted);
        assert!(w.state.is_terminal());
    }

    #[test]
    fn preemption_disabled_config() {
        let handler = NoOpEvictionHandler::new();
        let config = PreemptionConfig::new().with_enabled(false);
        let preemptor = Preemptor::new(config, handler);

        let victim = PreemptionCandidate::new(
            WorkloadId::new("victim"),
            PriorityClass::preemptible(),
        );

        // Cannot preempt when disabled
        assert!(!preemptor.can_preempt(&PriorityClass::high_priority(), &victim));

        // Eviction fails when disabled
        let result = preemptor.evict(&[victim]);
        assert!(result.is_err());
    }
}
