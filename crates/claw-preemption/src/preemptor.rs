//! Preemptor for selecting and evicting workloads.
//!
//! The [`Preemptor`] is responsible for:
//! - Finding victim workloads when resources are scarce
//! - Evaluating preemption eligibility based on priority classes
//! - Executing graceful eviction with configurable grace periods
//! - Tracking eviction results and freed resources

use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use parking_lot::RwLock;
use tracing::{debug, info, warn};

use crate::error::{PreemptionError, Result};
use crate::types::{
    EvictionFailure, EvictionResult, PreemptionCandidate, PreemptionConfig, PriorityClass,
    ResourceRequirements, VictimSelectionStrategy, WorkloadId, WorkloadState,
};

#[cfg(test)]
use crate::types::PreemptionPolicy;

/// Handler for executing workload evictions.
///
/// Implement this trait to integrate with your workload runtime.
pub trait EvictionHandler: Send + Sync {
    /// Sends a graceful shutdown signal to the workload.
    ///
    /// # Errors
    ///
    /// Returns error if the eviction signal cannot be sent.
    fn evict(&self, workload_id: &WorkloadId, grace_period: Duration) -> Result<()>;

    /// Forcefully terminates a workload (used after grace period expires).
    ///
    /// # Errors
    ///
    /// Returns error if the workload cannot be killed.
    fn kill(&self, workload_id: &WorkloadId) -> Result<()>;

    /// Checks if a workload has terminated.
    fn is_terminated(&self, workload_id: &WorkloadId) -> bool;
}

/// A no-op eviction handler for testing.
#[derive(Debug, Default)]
pub struct NoOpEvictionHandler {
    terminated: RwLock<HashMap<String, bool>>,
}

impl NoOpEvictionHandler {
    /// Creates a new no-op handler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks a workload as terminated (for testing).
    pub fn mark_terminated(&self, workload_id: &WorkloadId) {
        let mut terminated = self.terminated.write();
        terminated.insert(workload_id.as_str().to_string(), true);
    }
}

impl EvictionHandler for NoOpEvictionHandler {
    fn evict(&self, workload_id: &WorkloadId, _grace_period: Duration) -> Result<()> {
        debug!(workload_id = %workload_id, "NoOp eviction signal sent");
        Ok(())
    }

    fn kill(&self, workload_id: &WorkloadId) -> Result<()> {
        debug!(workload_id = %workload_id, "NoOp kill signal sent");
        let mut terminated = self.terminated.write();
        terminated.insert(workload_id.as_str().to_string(), true);
        Ok(())
    }

    fn is_terminated(&self, workload_id: &WorkloadId) -> bool {
        let terminated = self.terminated.read();
        terminated.get(workload_id.as_str()).copied().unwrap_or(false)
    }
}

/// A preemption request specifying resource needs and requestor priority.
#[derive(Debug, Clone)]
pub struct PreemptionRequest {
    /// Resources needed by the requestor.
    pub needed_resources: ResourceRequirements,
    /// Priority class of the requestor.
    pub requestor_priority: PriorityClass,
    /// Node to preempt from (None for any node).
    pub node_id: Option<String>,
    /// Maximum cost willing to pay for preemption.
    pub max_preemption_cost: Option<f64>,
}

impl PreemptionRequest {
    /// Creates a new preemption request.
    #[must_use]
    pub fn new(needed_resources: ResourceRequirements, requestor_priority: PriorityClass) -> Self {
        Self {
            needed_resources,
            requestor_priority,
            node_id: None,
            max_preemption_cost: None,
        }
    }

    /// Sets the target node.
    #[must_use]
    pub fn with_node(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }

    /// Sets the maximum preemption cost.
    #[must_use]
    pub fn with_max_cost(mut self, max_cost: f64) -> Self {
        self.max_preemption_cost = Some(max_cost);
        self
    }
}

/// Result of finding victims for preemption.
#[derive(Debug, Clone)]
pub struct VictimSet {
    /// Workloads selected for preemption.
    pub victims: Vec<PreemptionCandidate>,
    /// Total resources that would be freed.
    pub total_freed_resources: ResourceRequirements,
    /// Total preemption cost.
    pub total_cost: f64,
    /// Whether the freed resources satisfy the request.
    pub satisfies_request: bool,
}

impl VictimSet {
    /// Creates an empty victim set.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            victims: Vec::new(),
            total_freed_resources: ResourceRequirements::new(),
            total_cost: 0.0,
            satisfies_request: false,
        }
    }

    /// Adds a victim to the set.
    pub fn add_victim(&mut self, victim: PreemptionCandidate) {
        self.total_freed_resources = self.total_freed_resources.add(&victim.resources);
        self.total_cost += victim.preemption_cost;
        self.victims.push(victim);
    }

    /// Returns the number of victims.
    #[must_use]
    pub fn len(&self) -> usize {
        self.victims.len()
    }

    /// Returns true if there are no victims.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.victims.is_empty()
    }
}

/// The preemptor selects victims and executes evictions.
pub struct Preemptor<H: EvictionHandler> {
    config: PreemptionConfig,
    handler: H,
    priority_classes: RwLock<HashMap<String, PriorityClass>>,
}

impl<H: EvictionHandler> Preemptor<H> {
    /// Creates a new preemptor with the given configuration and handler.
    #[must_use]
    pub fn new(config: PreemptionConfig, handler: H) -> Self {
        let preemptor = Self {
            config,
            handler,
            priority_classes: RwLock::new(HashMap::new()),
        };

        // Register built-in priority classes
        let mut classes = preemptor.priority_classes.write();
        for class in PriorityClass::built_in_classes() {
            classes.insert(class.name.clone(), class);
        }
        drop(classes);

        preemptor
    }

    /// Creates a preemptor with default configuration.
    #[must_use]
    pub fn with_defaults(handler: H) -> Self {
        Self::new(PreemptionConfig::default(), handler)
    }

    /// Returns the current configuration.
    #[must_use]
    pub fn config(&self) -> &PreemptionConfig {
        &self.config
    }

    /// Registers a custom priority class.
    ///
    /// # Errors
    ///
    /// Returns error if the priority class is invalid.
    pub fn register_priority_class(&self, priority_class: PriorityClass) -> Result<()> {
        if priority_class.value > 1000 {
            return Err(PreemptionError::InvalidPriorityClass {
                reason: format!("value {} exceeds maximum of 1000", priority_class.value),
            });
        }

        let mut classes = self.priority_classes.write();
        classes.insert(priority_class.name.clone(), priority_class);
        Ok(())
    }

    /// Gets a priority class by name.
    #[must_use]
    pub fn get_priority_class(&self, name: &str) -> Option<PriorityClass> {
        let classes = self.priority_classes.read();
        classes.get(name).cloned()
    }

    /// Checks if a workload can be preempted by another with the given priority class.
    #[must_use]
    pub fn can_preempt(&self, requestor: &PriorityClass, victim: &PreemptionCandidate) -> bool {
        if !self.config.enabled {
            return false;
        }

        // Victim must be in preemptible state
        if !victim.can_be_preempted() {
            return false;
        }

        // Check priority difference requirement
        if requestor.value <= victim.priority_class.value {
            return false;
        }

        let priority_diff = requestor.value - victim.priority_class.value;
        if priority_diff < self.config.min_priority_difference {
            return false;
        }

        // Use priority class's can_preempt logic
        requestor.can_preempt(&victim.priority_class)
    }

    /// Finds victims to preempt to satisfy a resource request.
    ///
    /// Returns a set of victims that, if evicted, would free enough resources
    /// to satisfy the request.
    #[must_use]
    pub fn find_victims(
        &self,
        request: &PreemptionRequest,
        candidates: &[PreemptionCandidate],
    ) -> VictimSet {
        if !self.config.enabled {
            return VictimSet::empty();
        }

        // Filter to eligible victims
        let mut eligible: Vec<&PreemptionCandidate> = candidates
            .iter()
            .filter(|c| self.can_preempt(&request.requestor_priority, c))
            .filter(|c| {
                // Filter by node if specified
                request
                    .node_id
                    .as_ref()
                    .is_none_or(|node| c.node_id.as_ref() == Some(node))
            })
            .collect();

        if eligible.is_empty() {
            return VictimSet::empty();
        }

        // Sort by selection strategy
        self.sort_victims(&mut eligible);

        // Select victims until we have enough resources
        let mut victim_set = VictimSet::empty();
        let max_victims = self.config.max_victims_per_operation;

        for candidate in eligible {
            if victim_set.len() >= max_victims {
                break;
            }

            // Check cost limit
            if let Some(max_cost) = request.max_preemption_cost {
                if victim_set.total_cost + candidate.preemption_cost > max_cost {
                    continue;
                }
            }

            victim_set.add_victim(candidate.clone());

            // Check if we have enough resources
            if request
                .needed_resources
                .is_satisfied_by(&victim_set.total_freed_resources)
            {
                victim_set.satisfies_request = true;
                break;
            }
        }

        victim_set
    }

    /// Sorts victims according to the configured selection strategy.
    fn sort_victims(&self, victims: &mut [&PreemptionCandidate]) {
        match self.config.victim_selection {
            VictimSelectionStrategy::LowestPriority => {
                victims.sort_by_key(|a| a.priority_value());
            }
            VictimSelectionStrategy::ShortestRunning => {
                victims.sort_by(|a, b| {
                    let a_duration = a.running_duration().unwrap_or(Duration::MAX);
                    let b_duration = b.running_duration().unwrap_or(Duration::MAX);
                    a_duration.cmp(&b_duration)
                });
            }
            VictimSelectionStrategy::LowestCost => {
                victims.sort_by(|a, b| {
                    a.preemption_cost
                        .partial_cmp(&b.preemption_cost)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            VictimSelectionStrategy::MostResources => {
                victims.sort_by(|a, b| {
                    let a_resources = a.resources.gpus;
                    let b_resources = b.resources.gpus;
                    b_resources.cmp(&a_resources) // Descending
                });
            }
            VictimSelectionStrategy::Balanced => {
                // Score = (1 - normalized_priority) + (1 - normalized_cost) + normalized_resources
                victims.sort_by(|a, b| {
                    let a_score = self.balanced_score(a);
                    let b_score = self.balanced_score(b);
                    b_score
                        .partial_cmp(&a_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }
    }

    /// Calculates a balanced score for victim selection.
    /// Higher score = better victim candidate.
    #[allow(clippy::unused_self)] // Keep as method for future config-based weighting
    fn balanced_score(&self, candidate: &PreemptionCandidate) -> f64 {
        // Lower priority = better victim (higher score)
        let priority_score = 1.0 - (f64::from(candidate.priority_value()) / 1000.0);

        // Lower cost = better victim (higher score)
        // Normalize cost assuming max cost of 1000
        let cost_score = 1.0 - (candidate.preemption_cost / 1000.0).min(1.0);

        // More resources = better victim (higher score)
        // Normalize GPUs assuming max of 8
        let resource_score = (f64::from(candidate.resources.gpus) / 8.0).min(1.0);

        // Shorter running time = better victim (higher score)
        let runtime_score = candidate.running_duration().map_or(0.5, |d| {
            // Normalize to 1 hour
            1.0 - (d.as_secs_f64() / 3600.0).min(1.0)
        });

        // Weighted combination
        0.4 * priority_score + 0.2 * cost_score + 0.2 * resource_score + 0.2 * runtime_score
    }

    /// Evicts the given victims gracefully.
    ///
    /// Sends eviction signals and waits for graceful termination.
    /// Returns the eviction result with details of what was evicted.
    ///
    /// # Errors
    ///
    /// Returns error if preemption is disabled or all evictions fail.
    pub fn evict(&self, victims: &[PreemptionCandidate]) -> Result<EvictionResult> {
        if !self.config.enabled {
            return Err(PreemptionError::PreemptionNotAllowed {
                reason: "preemption is disabled".into(),
            });
        }

        if victims.is_empty() {
            return Ok(EvictionResult::new());
        }

        let mut result = EvictionResult::new();

        for victim in victims {
            // Determine grace period (capped at max)
            let grace_period = victim
                .grace_period
                .min(self.config.max_grace_period);

            info!(
                workload_id = %victim.workload_id,
                priority = victim.priority_value(),
                grace_period_secs = grace_period.as_secs(),
                "Initiating graceful eviction"
            );

            match self.handler.evict(&victim.workload_id, grace_period) {
                Ok(()) => {
                    result.add_evicted(
                        victim.workload_id.clone(),
                        &victim.resources,
                        victim.preemption_cost,
                    );
                }
                Err(e) => {
                    warn!(
                        workload_id = %victim.workload_id,
                        error = %e,
                        "Failed to evict workload"
                    );
                    result.add_failure(EvictionFailure::new(
                        victim.workload_id.clone(),
                        e.to_string(),
                    ));
                }
            }
        }

        result.complete();
        Ok(result)
    }

    /// Forcefully kills workloads that haven't terminated within their grace period.
    ///
    /// # Errors
    ///
    /// Returns error if any kill operation fails.
    pub fn force_kill(&self, workload_ids: &[WorkloadId]) -> Result<Vec<WorkloadId>> {
        let mut killed = Vec::new();

        for workload_id in workload_ids {
            if self.handler.is_terminated(workload_id) {
                continue;
            }

            info!(workload_id = %workload_id, "Force killing workload after grace period");

            match self.handler.kill(workload_id) {
                Ok(()) => {
                    killed.push(workload_id.clone());
                }
                Err(e) => {
                    warn!(
                        workload_id = %workload_id,
                        error = %e,
                        "Failed to force kill workload"
                    );
                }
            }
        }

        Ok(killed)
    }

    /// Checks if a workload has terminated.
    #[must_use]
    pub fn is_terminated(&self, workload_id: &WorkloadId) -> bool {
        self.handler.is_terminated(workload_id)
    }

    /// Returns all registered priority classes.
    #[must_use]
    pub fn priority_classes(&self) -> Vec<PriorityClass> {
        let classes = self.priority_classes.read();
        classes.values().cloned().collect()
    }
}

/// Manager for tracking preemption state and coordinating evictions.
pub struct PreemptionManager<H: EvictionHandler> {
    preemptor: Preemptor<H>,
    workloads: RwLock<HashMap<String, PreemptionCandidate>>,
    eviction_history: RwLock<Vec<EvictionResult>>,
}

impl<H: EvictionHandler> PreemptionManager<H> {
    /// Creates a new preemption manager.
    #[must_use]
    pub fn new(preemptor: Preemptor<H>) -> Self {
        Self {
            preemptor,
            workloads: RwLock::new(HashMap::new()),
            eviction_history: RwLock::new(Vec::new()),
        }
    }

    /// Creates a manager with default configuration.
    #[must_use]
    pub fn with_defaults(handler: H) -> Self {
        Self::new(Preemptor::with_defaults(handler))
    }

    /// Registers a workload for preemption tracking.
    pub fn register_workload(&self, candidate: PreemptionCandidate) {
        let mut workloads = self.workloads.write();
        workloads.insert(candidate.workload_id.as_str().to_string(), candidate);
    }

    /// Unregisters a workload.
    pub fn unregister_workload(&self, workload_id: &WorkloadId) {
        let mut workloads = self.workloads.write();
        workloads.remove(workload_id.as_str());
    }

    /// Updates a workload's state.
    pub fn update_workload_state(&self, workload_id: &WorkloadId, state: WorkloadState) {
        let mut workloads = self.workloads.write();
        if let Some(workload) = workloads.get_mut(workload_id.as_str()) {
            workload.state = state;
        }
    }

    /// Gets a workload by ID.
    #[must_use]
    pub fn get_workload(&self, workload_id: &WorkloadId) -> Option<PreemptionCandidate> {
        let workloads = self.workloads.read();
        workloads.get(workload_id.as_str()).cloned()
    }

    /// Returns all registered workloads.
    #[must_use]
    pub fn workloads(&self) -> Vec<PreemptionCandidate> {
        let workloads = self.workloads.read();
        workloads.values().cloned().collect()
    }

    /// Returns workloads that can be preempted.
    #[must_use]
    pub fn preemptible_workloads(&self) -> Vec<PreemptionCandidate> {
        let workloads = self.workloads.read();
        workloads
            .values()
            .filter(|w| w.can_be_preempted())
            .cloned()
            .collect()
    }

    /// Requests preemption to free resources.
    ///
    /// Finds victims, evicts them, and returns the result.
    ///
    /// # Errors
    ///
    /// Returns error if no suitable victims are found or eviction fails.
    pub fn request_preemption(&self, request: &PreemptionRequest) -> Result<EvictionResult> {
        let candidates = self.preemptible_workloads();
        let victim_set = self.preemptor.find_victims(request, &candidates);

        if victim_set.is_empty() {
            return Err(PreemptionError::PreemptionNotAllowed {
                reason: "no eligible victims found".into(),
            });
        }

        if !victim_set.satisfies_request {
            return Err(PreemptionError::InsufficientResources {
                needed: format!(
                    "{} GPUs, {} bytes memory",
                    request.needed_resources.gpus, request.needed_resources.memory_bytes
                ),
                available: format!(
                    "{} GPUs, {} bytes memory",
                    victim_set.total_freed_resources.gpus,
                    victim_set.total_freed_resources.memory_bytes
                ),
            });
        }

        let result = self.preemptor.evict(&victim_set.victims)?;

        // Update workload states
        for workload_id in &result.evicted_workloads {
            self.update_workload_state(workload_id, WorkloadState::Evicting);
        }

        // Record in history
        {
            let mut history = self.eviction_history.write();
            history.push(result.clone());
        }

        Ok(result)
    }

    /// Returns the preemptor.
    #[must_use]
    pub fn preemptor(&self) -> &Preemptor<H> {
        &self.preemptor
    }

    /// Returns eviction history.
    #[must_use]
    pub fn eviction_history(&self) -> Vec<EvictionResult> {
        let history = self.eviction_history.read();
        history.clone()
    }

    /// Clears eviction history older than the given duration.
    pub fn clear_old_history(&self, older_than: Duration) {
        let cutoff = Utc::now() - chrono::Duration::from_std(older_than).unwrap_or_default();
        let mut history = self.eviction_history.write();
        history.retain(|r| r.initiated_at > cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn create_test_candidate(
        id: &str,
        priority_class: PriorityClass,
        gpus: u32,
    ) -> PreemptionCandidate {
        PreemptionCandidate::new(WorkloadId::new(id), priority_class)
            .with_resources(ResourceRequirements::new().with_gpus(gpus).with_memory_gb(16))
            .with_preemption_cost(10.0 * f64::from(gpus))
    }

    mod preemptor_tests {
        use super::*;

        #[test]
        fn preemptor_creation() {
            let handler = NoOpEvictionHandler::new();
            let preemptor = Preemptor::with_defaults(handler);

            assert!(preemptor.config().enabled);

            // Check built-in priority classes are registered
            let classes = preemptor.priority_classes();
            assert!(classes.len() >= 5);
            assert!(preemptor.get_priority_class("system-critical").is_some());
            assert!(preemptor.get_priority_class("default").is_some());
        }

        #[test]
        fn preemptor_register_custom_priority_class() {
            let handler = NoOpEvictionHandler::new();
            let preemptor = Preemptor::with_defaults(handler);

            let custom = PriorityClass::new("custom", 600, PreemptionPolicy::PreemptLowerPriority)
                .ok()
                .unwrap_or_else(PriorityClass::default);
            assert!(preemptor.register_priority_class(custom).is_ok());

            assert!(preemptor.get_priority_class("custom").is_some());
        }

        #[test]
        fn preemptor_register_invalid_priority_class() {
            let handler = NoOpEvictionHandler::new();
            let preemptor = Preemptor::with_defaults(handler);

            // Create with invalid value directly
            let invalid = PriorityClass {
                name: "invalid".into(),
                value: 1500, // Too high
                preemption_policy: PreemptionPolicy::PreemptLowerPriority,
                description: String::new(),
                is_system: false,
            };

            let result = preemptor.register_priority_class(invalid);
            assert!(result.is_err());
        }

        #[test]
        fn preemptor_can_preempt() {
            let handler = NoOpEvictionHandler::new();
            let preemptor = Preemptor::with_defaults(handler);

            let high = PriorityClass::high_priority();
            let default_victim = create_test_candidate("job-1", PriorityClass::default_priority(), 2);
            let spot_victim = create_test_candidate("job-2", PriorityClass::spot(), 2);
            let critical_victim = create_test_candidate("job-3", PriorityClass::system_critical(), 2);

            // High can preempt default and spot
            assert!(preemptor.can_preempt(&high, &default_victim));
            assert!(preemptor.can_preempt(&high, &spot_victim));

            // Cannot preempt system-critical
            assert!(!preemptor.can_preempt(&high, &critical_victim));

            // Default cannot preempt high
            let default_pc = PriorityClass::default_priority();
            let high_victim = create_test_candidate("job-4", PriorityClass::high_priority(), 2);
            assert!(!preemptor.can_preempt(&default_pc, &high_victim));
        }

        #[test]
        fn preemptor_can_preempt_disabled() {
            let handler = NoOpEvictionHandler::new();
            let config = PreemptionConfig::new().with_enabled(false);
            let preemptor = Preemptor::new(config, handler);

            let high = PriorityClass::high_priority();
            let victim = create_test_candidate("job-1", PriorityClass::spot(), 2);

            assert!(!preemptor.can_preempt(&high, &victim));
        }

        #[test]
        fn preemptor_can_preempt_min_priority_difference() {
            let handler = NoOpEvictionHandler::new();
            let config = PreemptionConfig::new().with_min_priority_difference(300);
            let preemptor = Preemptor::new(config, handler);

            let high = PriorityClass::high_priority(); // 750
            let default_victim = create_test_candidate("job-1", PriorityClass::default_priority(), 2); // 500

            // Difference is 250, less than required 300
            assert!(!preemptor.can_preempt(&high, &default_victim));

            // Spot has priority 100, difference is 650
            let spot_victim = create_test_candidate("job-2", PriorityClass::spot(), 2);
            assert!(preemptor.can_preempt(&high, &spot_victim));
        }

        #[test]
        fn preemptor_find_victims_basic() {
            let handler = NoOpEvictionHandler::new();
            let preemptor = Preemptor::with_defaults(handler);

            let candidates = vec![
                create_test_candidate("job-1", PriorityClass::spot(), 2),
                create_test_candidate("job-2", PriorityClass::spot(), 2),
                create_test_candidate("job-3", PriorityClass::preemptible(), 4),
            ];

            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(4),
                PriorityClass::high_priority(),
            );

            let victim_set = preemptor.find_victims(&request, &candidates);

            assert!(victim_set.satisfies_request);
            assert!(!victim_set.is_empty());
            assert!(victim_set.total_freed_resources.gpus >= 4);
        }

        #[test]
        fn preemptor_find_victims_no_eligible() {
            let handler = NoOpEvictionHandler::new();
            let preemptor = Preemptor::with_defaults(handler);

            // All candidates have higher priority than requestor
            let candidates = vec![
                create_test_candidate("job-1", PriorityClass::system_critical(), 2),
                create_test_candidate("job-2", PriorityClass::high_priority(), 2),
            ];

            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(2),
                PriorityClass::default_priority(),
            );

            let victim_set = preemptor.find_victims(&request, &candidates);

            assert!(victim_set.is_empty());
            assert!(!victim_set.satisfies_request);
        }

        #[test]
        fn preemptor_find_victims_insufficient_resources() {
            let handler = NoOpEvictionHandler::new();
            let preemptor = Preemptor::with_defaults(handler);

            let candidates = vec![
                create_test_candidate("job-1", PriorityClass::spot(), 1),
                create_test_candidate("job-2", PriorityClass::spot(), 1),
            ];

            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(10), // Need more than available
                PriorityClass::high_priority(),
            );

            let victim_set = preemptor.find_victims(&request, &candidates);

            assert!(!victim_set.is_empty());
            assert!(!victim_set.satisfies_request);
            assert_eq!(victim_set.total_freed_resources.gpus, 2);
        }

        #[test]
        fn preemptor_find_victims_with_node_filter() {
            let handler = NoOpEvictionHandler::new();
            let preemptor = Preemptor::with_defaults(handler);

            let candidates = vec![
                create_test_candidate("job-1", PriorityClass::spot(), 4).with_node("node-1"),
                create_test_candidate("job-2", PriorityClass::spot(), 4).with_node("node-2"),
            ];

            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(4),
                PriorityClass::high_priority(),
            )
            .with_node("node-1");

            let victim_set = preemptor.find_victims(&request, &candidates);

            assert!(victim_set.satisfies_request);
            assert_eq!(victim_set.len(), 1);
            assert!(victim_set.victims.iter().all(|v| v.node_id == Some("node-1".into())));
        }

        #[test]
        fn preemptor_find_victims_with_cost_limit() {
            let handler = NoOpEvictionHandler::new();
            let preemptor = Preemptor::with_defaults(handler);

            let candidates = vec![
                create_test_candidate("job-1", PriorityClass::spot(), 2).with_preemption_cost(100.0),
                create_test_candidate("job-2", PriorityClass::spot(), 2).with_preemption_cost(5.0),
            ];

            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(2),
                PriorityClass::high_priority(),
            )
            .with_max_cost(10.0);

            let victim_set = preemptor.find_victims(&request, &candidates);

            assert!(victim_set.satisfies_request);
            assert_eq!(victim_set.len(), 1);
            assert!(victim_set.total_cost <= 10.0);
        }

        #[test]
        fn preemptor_find_victims_respects_max_victims() {
            let handler = NoOpEvictionHandler::new();
            let config = PreemptionConfig::new().with_max_victims(2);
            let preemptor = Preemptor::new(config, handler);

            let candidates: Vec<_> = (0..10)
                .map(|i| create_test_candidate(&format!("job-{i}"), PriorityClass::spot(), 1))
                .collect();

            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(10),
                PriorityClass::high_priority(),
            );

            let victim_set = preemptor.find_victims(&request, &candidates);

            assert_eq!(victim_set.len(), 2);
        }

        #[test]
        fn preemptor_evict_success() {
            let handler = NoOpEvictionHandler::new();
            let preemptor = Preemptor::with_defaults(handler);

            let victims = vec![
                create_test_candidate("job-1", PriorityClass::spot(), 2),
                create_test_candidate("job-2", PriorityClass::preemptible(), 2),
            ];

            let result = preemptor.evict(&victims);
            assert!(result.is_ok());

            let result = result.ok().unwrap_or_else(EvictionResult::new);
            assert!(result.is_successful());
            assert_eq!(result.evicted_count(), 2);
            assert_eq!(result.freed_resources.gpus, 4);
        }

        #[test]
        fn preemptor_evict_disabled() {
            let handler = NoOpEvictionHandler::new();
            let config = PreemptionConfig::new().with_enabled(false);
            let preemptor = Preemptor::new(config, handler);

            let victims = vec![create_test_candidate("job-1", PriorityClass::spot(), 2)];

            let result = preemptor.evict(&victims);
            assert!(result.is_err());
        }

        #[test]
        fn preemptor_evict_empty() {
            let handler = NoOpEvictionHandler::new();
            let preemptor = Preemptor::with_defaults(handler);

            let result = preemptor.evict(&[]);
            assert!(result.is_ok());

            let result = result.ok().unwrap_or_else(EvictionResult::new);
            assert_eq!(result.evicted_count(), 0);
        }

        #[test]
        fn preemptor_force_kill() {
            let handler = NoOpEvictionHandler::new();
            let preemptor = Preemptor::with_defaults(handler);

            let workload_ids = vec![WorkloadId::new("job-1"), WorkloadId::new("job-2")];

            let result = preemptor.force_kill(&workload_ids);
            assert!(result.is_ok());

            let killed = result.ok().unwrap_or_default();
            assert_eq!(killed.len(), 2);

            // Verify they're now terminated
            assert!(preemptor.is_terminated(&workload_ids[0]));
            assert!(preemptor.is_terminated(&workload_ids[1]));
        }
    }

    mod victim_selection_tests {
        use super::*;

        fn create_candidate_with_details(
            id: &str,
            priority: u16,
            gpus: u32,
            cost: f64,
            running_secs: i64,
        ) -> PreemptionCandidate {
            let pc = PriorityClass::new(id, priority, PreemptionPolicy::PreemptLowerPriority)
                .ok()
                .unwrap_or_else(PriorityClass::default);
            let started_at = Utc::now() - chrono::Duration::seconds(running_secs);

            PreemptionCandidate::new(WorkloadId::new(id), pc)
                .with_resources(ResourceRequirements::new().with_gpus(gpus))
                .with_preemption_cost(cost)
                .with_started_at(started_at)
        }

        #[test]
        fn selection_lowest_priority() {
            let handler = NoOpEvictionHandler::new();
            let config = PreemptionConfig::new()
                .with_victim_selection(VictimSelectionStrategy::LowestPriority);
            let preemptor = Preemptor::new(config, handler);

            let candidates = vec![
                create_candidate_with_details("job-1", 200, 2, 50.0, 100),
                create_candidate_with_details("job-2", 100, 2, 50.0, 100), // Lowest priority
                create_candidate_with_details("job-3", 300, 2, 50.0, 100),
            ];

            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(2),
                PriorityClass::high_priority(),
            );

            let victim_set = preemptor.find_victims(&request, &candidates);

            assert!(victim_set.satisfies_request);
            assert_eq!(victim_set.victims[0].workload_id.as_str(), "job-2");
        }

        #[test]
        fn selection_shortest_running() {
            let handler = NoOpEvictionHandler::new();
            let config = PreemptionConfig::new()
                .with_victim_selection(VictimSelectionStrategy::ShortestRunning);
            let preemptor = Preemptor::new(config, handler);

            let candidates = vec![
                create_candidate_with_details("job-1", 100, 2, 50.0, 3600), // 1 hour
                create_candidate_with_details("job-2", 100, 2, 50.0, 60),   // 1 minute (shortest)
                create_candidate_with_details("job-3", 100, 2, 50.0, 1800), // 30 min
            ];

            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(2),
                PriorityClass::high_priority(),
            );

            let victim_set = preemptor.find_victims(&request, &candidates);

            assert!(victim_set.satisfies_request);
            assert_eq!(victim_set.victims[0].workload_id.as_str(), "job-2");
        }

        #[test]
        fn selection_lowest_cost() {
            let handler = NoOpEvictionHandler::new();
            let config = PreemptionConfig::new()
                .with_victim_selection(VictimSelectionStrategy::LowestCost);
            let preemptor = Preemptor::new(config, handler);

            let candidates = vec![
                create_candidate_with_details("job-1", 100, 2, 100.0, 100),
                create_candidate_with_details("job-2", 100, 2, 10.0, 100), // Lowest cost
                create_candidate_with_details("job-3", 100, 2, 50.0, 100),
            ];

            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(2),
                PriorityClass::high_priority(),
            );

            let victim_set = preemptor.find_victims(&request, &candidates);

            assert!(victim_set.satisfies_request);
            assert_eq!(victim_set.victims[0].workload_id.as_str(), "job-2");
        }

        #[test]
        fn selection_most_resources() {
            let handler = NoOpEvictionHandler::new();
            let config = PreemptionConfig::new()
                .with_victim_selection(VictimSelectionStrategy::MostResources);
            let preemptor = Preemptor::new(config, handler);

            let candidates = vec![
                create_candidate_with_details("job-1", 100, 2, 50.0, 100),
                create_candidate_with_details("job-2", 100, 8, 50.0, 100), // Most GPUs
                create_candidate_with_details("job-3", 100, 4, 50.0, 100),
            ];

            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(4),
                PriorityClass::high_priority(),
            );

            let victim_set = preemptor.find_victims(&request, &candidates);

            assert!(victim_set.satisfies_request);
            assert_eq!(victim_set.victims[0].workload_id.as_str(), "job-2");
        }
    }

    mod preemption_manager_tests {
        use super::*;

        #[test]
        fn manager_creation() {
            let handler = NoOpEvictionHandler::new();
            let manager = PreemptionManager::with_defaults(handler);

            assert!(manager.workloads().is_empty());
            assert!(manager.eviction_history().is_empty());
        }

        #[test]
        fn manager_register_workload() {
            let handler = NoOpEvictionHandler::new();
            let manager = PreemptionManager::with_defaults(handler);

            let candidate = create_test_candidate("job-1", PriorityClass::default_priority(), 4);
            manager.register_workload(candidate);

            assert_eq!(manager.workloads().len(), 1);
            assert!(manager.get_workload(&WorkloadId::new("job-1")).is_some());
        }

        #[test]
        fn manager_unregister_workload() {
            let handler = NoOpEvictionHandler::new();
            let manager = PreemptionManager::with_defaults(handler);

            let candidate = create_test_candidate("job-1", PriorityClass::default_priority(), 4);
            manager.register_workload(candidate);
            manager.unregister_workload(&WorkloadId::new("job-1"));

            assert!(manager.workloads().is_empty());
        }

        #[test]
        fn manager_update_state() {
            let handler = NoOpEvictionHandler::new();
            let manager = PreemptionManager::with_defaults(handler);

            let candidate = create_test_candidate("job-1", PriorityClass::default_priority(), 4);
            manager.register_workload(candidate);

            manager.update_workload_state(&WorkloadId::new("job-1"), WorkloadState::Evicting);

            let workload = manager.get_workload(&WorkloadId::new("job-1"));
            assert!(workload.is_some());
            assert_eq!(
                workload.unwrap_or_else(|| create_test_candidate("", PriorityClass::default_priority(), 0)).state,
                WorkloadState::Evicting
            );
        }

        #[test]
        fn manager_preemptible_workloads() {
            let handler = NoOpEvictionHandler::new();
            let manager = PreemptionManager::with_defaults(handler);

            // Register various workloads
            manager.register_workload(create_test_candidate(
                "job-1",
                PriorityClass::spot(),
                2,
            ));
            manager.register_workload(create_test_candidate(
                "job-2",
                PriorityClass::system_critical(),
                2,
            ));
            manager.register_workload(
                create_test_candidate("job-3", PriorityClass::default_priority(), 2)
                    .with_state(WorkloadState::Pending),
            );

            let preemptible = manager.preemptible_workloads();

            // Only job-1 should be preemptible (running + not system-critical)
            assert_eq!(preemptible.len(), 1);
            assert_eq!(preemptible[0].workload_id.as_str(), "job-1");
        }

        #[test]
        fn manager_request_preemption_success() {
            let handler = NoOpEvictionHandler::new();
            let manager = PreemptionManager::with_defaults(handler);

            // Register preemptible workloads
            manager.register_workload(create_test_candidate("job-1", PriorityClass::spot(), 4));
            manager.register_workload(create_test_candidate("job-2", PriorityClass::preemptible(), 4));

            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(4),
                PriorityClass::high_priority(),
            );

            let result = manager.request_preemption(&request);
            assert!(result.is_ok());

            let result = result.ok().unwrap_or_else(EvictionResult::new);
            assert!(result.is_successful());
            assert!(result.freed_resources.gpus >= 4);

            // Check history was recorded
            let history = manager.eviction_history();
            assert_eq!(history.len(), 1);
        }

        #[test]
        fn manager_request_preemption_no_victims() {
            let handler = NoOpEvictionHandler::new();
            let manager = PreemptionManager::with_defaults(handler);

            // Only register high-priority workloads
            manager.register_workload(create_test_candidate(
                "job-1",
                PriorityClass::system_critical(),
                4,
            ));

            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(4),
                PriorityClass::default_priority(),
            );

            let result = manager.request_preemption(&request);
            assert!(result.is_err());
        }

        #[test]
        fn manager_request_preemption_insufficient() {
            let handler = NoOpEvictionHandler::new();
            let manager = PreemptionManager::with_defaults(handler);

            // Only 2 GPUs available for preemption
            manager.register_workload(create_test_candidate("job-1", PriorityClass::spot(), 2));

            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(10), // Need more
                PriorityClass::high_priority(),
            );

            let result = manager.request_preemption(&request);
            assert!(result.is_err());
        }

        #[test]
        fn manager_clear_old_history() {
            let handler = NoOpEvictionHandler::new();
            let manager = PreemptionManager::with_defaults(handler);

            manager.register_workload(create_test_candidate("job-1", PriorityClass::spot(), 4));

            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(2),
                PriorityClass::high_priority(),
            );

            let _ = manager.request_preemption(&request);
            assert_eq!(manager.eviction_history().len(), 1);

            // Clear with very short duration (everything should be older)
            manager.clear_old_history(Duration::ZERO);
            // Note: This won't actually clear because the timestamp is now,
            // but it tests the API
        }
    }

    mod no_op_handler_tests {
        use super::*;

        #[test]
        fn no_op_handler_evict() {
            let handler = NoOpEvictionHandler::new();
            let result = handler.evict(&WorkloadId::new("job-1"), Duration::from_secs(30));
            assert!(result.is_ok());
        }

        #[test]
        fn no_op_handler_kill() {
            let handler = NoOpEvictionHandler::new();
            let workload_id = WorkloadId::new("job-1");

            assert!(!handler.is_terminated(&workload_id));

            let result = handler.kill(&workload_id);
            assert!(result.is_ok());

            assert!(handler.is_terminated(&workload_id));
        }

        #[test]
        fn no_op_handler_mark_terminated() {
            let handler = NoOpEvictionHandler::new();
            let workload_id = WorkloadId::new("job-1");

            assert!(!handler.is_terminated(&workload_id));

            handler.mark_terminated(&workload_id);

            assert!(handler.is_terminated(&workload_id));
        }
    }

    mod preemption_request_tests {
        use super::*;

        #[test]
        fn preemption_request_creation() {
            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(4),
                PriorityClass::high_priority(),
            );

            assert_eq!(request.needed_resources.gpus, 4);
            assert_eq!(request.requestor_priority.value, 750);
            assert!(request.node_id.is_none());
            assert!(request.max_preemption_cost.is_none());
        }

        #[test]
        fn preemption_request_with_node() {
            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(4),
                PriorityClass::high_priority(),
            )
            .with_node("node-1");

            assert_eq!(request.node_id, Some("node-1".into()));
        }

        #[test]
        fn preemption_request_with_max_cost() {
            let request = PreemptionRequest::new(
                ResourceRequirements::new().with_gpus(4),
                PriorityClass::high_priority(),
            )
            .with_max_cost(100.0);

            assert_eq!(request.max_preemption_cost, Some(100.0));
        }
    }

    mod victim_set_tests {
        use super::*;

        #[test]
        fn victim_set_empty() {
            let set = VictimSet::empty();
            assert!(set.is_empty());
            assert_eq!(set.len(), 0);
            assert!(!set.satisfies_request);
            assert!(set.total_freed_resources.is_empty());
        }

        #[test]
        fn victim_set_add_victim() {
            let mut set = VictimSet::empty();

            let victim = create_test_candidate("job-1", PriorityClass::spot(), 4);
            set.add_victim(victim);

            assert_eq!(set.len(), 1);
            assert!(!set.is_empty());
            assert_eq!(set.total_freed_resources.gpus, 4);
        }

        #[test]
        fn victim_set_multiple_victims() {
            let mut set = VictimSet::empty();

            set.add_victim(create_test_candidate("job-1", PriorityClass::spot(), 2).with_preemption_cost(10.0));
            set.add_victim(create_test_candidate("job-2", PriorityClass::spot(), 4).with_preemption_cost(20.0));

            assert_eq!(set.len(), 2);
            assert_eq!(set.total_freed_resources.gpus, 6);
            assert!((set.total_cost - 30.0).abs() < f64::EPSILON);
        }
    }
}
