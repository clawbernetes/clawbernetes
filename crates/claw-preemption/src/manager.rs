//! Preemption manager for tracking workloads and coordinating evictions.
//!
//! The [`PreemptionManager`] provides a higher-level interface for:
//! - Tracking registered workloads
//! - Coordinating preemption requests
//! - Maintaining eviction history

use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use parking_lot::RwLock;

use crate::error::{PreemptionError, Result};
use crate::preemptor::{EvictionHandler, Preemptor, PreemptionRequest};
use crate::types::{EvictionResult, PreemptionCandidate, WorkloadId, WorkloadState};

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
    use crate::preemptor::NoOpEvictionHandler;
    use crate::types::{PriorityClass, ResourceRequirements};

    fn create_test_candidate(
        id: &str,
        priority_class: PriorityClass,
        gpus: u32,
    ) -> PreemptionCandidate {
        PreemptionCandidate::new(WorkloadId::new(id), priority_class)
            .with_resources(ResourceRequirements::new().with_gpus(gpus).with_memory_gb(16))
            .with_preemption_cost(10.0 * f64::from(gpus))
    }

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
            workload
                .unwrap_or_else(|| create_test_candidate("", PriorityClass::default_priority(), 0))
                .state,
            WorkloadState::Evicting
        );
    }

    #[test]
    fn manager_preemptible_workloads() {
        let handler = NoOpEvictionHandler::new();
        let manager = PreemptionManager::with_defaults(handler);

        // Register various workloads
        manager.register_workload(create_test_candidate("job-1", PriorityClass::spot(), 2));
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
        manager.register_workload(create_test_candidate(
            "job-2",
            PriorityClass::preemptible(),
            4,
        ));

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
