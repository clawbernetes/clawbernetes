//! Workload dispatch and lifecycle management.
//!
//! The [`WorkloadDispatcher`] coordinates workload submission, scheduling,
//! and dispatch to nodes in the fleet.

use std::collections::HashSet;

use claw_proto::{GatewayMessage, NodeCapabilities, NodeId, WorkloadId, WorkloadSpec, WorkloadState};
use thiserror::Error;

use crate::registry::{NodeRegistry, RegisteredNode, RegistryError};
use crate::scheduler::Scheduler;
use crate::workload::{TrackedWorkload, WorkloadManager, WorkloadManagerError};

/// Errors that can occur during dispatch operations.
#[derive(Debug, Error)]
pub enum DispatchError {
    /// Workload spec validation failed.
    #[error("validation failed: {0}")]
    ValidationFailed(String),

    /// Workload was not found.
    #[error("workload {0} not found")]
    WorkloadNotFound(WorkloadId),

    /// Node is not registered or is offline.
    #[error("node {0} is offline")]
    NodeOffline(NodeId),

    /// Invalid state transition.
    #[error("invalid state transition for workload {0}: cannot transition from {1} to {2}")]
    InvalidTransition(WorkloadId, WorkloadState, WorkloadState),

    /// Scheduling failed.
    #[error("scheduling failed: {0}")]
    SchedulingFailed(String),

    /// Registry operation failed.
    #[error("registry error: {0}")]
    Registry(#[from] RegistryError),

    /// Workload manager error.
    #[error("workload manager error: {0}")]
    WorkloadManager(#[from] WorkloadManagerError),

    /// Cannot stop workload in current state.
    #[error("cannot stop workload {0} in state {1}")]
    CannotStop(WorkloadId, WorkloadState),
}

/// Coordinates workload submission, scheduling, and dispatch.
///
/// The dispatcher is the central coordination point for workload lifecycle:
///
/// 1. **Submit** - Accept workload specs and create tracked workloads
/// 2. **Schedule** - Find suitable nodes using the scheduler
/// 3. **Dispatch** - Send workload commands to nodes
/// 4. **Track** - Process state updates from nodes
#[derive(Debug)]
pub struct WorkloadDispatcher {
    registry: NodeRegistry,
    workload_mgr: WorkloadManager,
    scheduler: Scheduler,
    /// Workloads waiting for a node assignment.
    pending_dispatch: HashSet<WorkloadId>,
}

impl WorkloadDispatcher {
    /// Create a new workload dispatcher.
    #[must_use]
    pub fn new(registry: NodeRegistry, workload_mgr: WorkloadManager, scheduler: Scheduler) -> Self {
        Self {
            registry,
            workload_mgr,
            scheduler,
            pending_dispatch: HashSet::new(),
        }
    }

    /// Submit a workload for scheduling and dispatch.
    ///
    /// If a suitable node is available, the workload is immediately assigned.
    /// Otherwise, it remains pending until a node becomes available.
    ///
    /// # Errors
    ///
    /// Returns an error if the workload spec is invalid.
    #[allow(clippy::needless_pass_by_value)] // spec is used for both submit and schedule
    pub fn submit(&mut self, spec: WorkloadSpec) -> Result<WorkloadId, DispatchError> {
        // Submit to workload manager (validates spec)
        let workload_id = self
            .workload_mgr
            .submit(spec.clone())
            .map_err(|e| DispatchError::ValidationFailed(e.to_string()))?;

        // Try to schedule immediately
        match self.scheduler.schedule(&spec, &self.registry) {
            Ok(node_id) => {
                // Assign to node
                self.workload_mgr
                    .assign_to_node(workload_id, node_id)
                    .map_err(DispatchError::WorkloadManager)?;
            }
            Err(_) => {
                // No suitable node - mark as pending
                self.pending_dispatch.insert(workload_id);
            }
        }

        Ok(workload_id)
    }

    /// Get a reference to a tracked workload.
    #[must_use]
    pub fn get_workload(&self, workload_id: WorkloadId) -> Option<&TrackedWorkload> {
        self.workload_mgr.get_workload(workload_id)
    }

    /// Get a reference to a registered node.
    #[must_use]
    pub fn get_node(&self, node_id: NodeId) -> Option<&RegisteredNode> {
        self.registry.get_node(node_id)
    }

    /// Get the count of workloads pending dispatch.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.pending_dispatch.len()
    }

    /// Dispatch a workload to a specific node.
    ///
    /// Creates a `StartWorkload` message and transitions the workload to `Starting`.
    ///
    /// # Errors
    ///
    /// Returns an error if the workload or node is not found.
    pub fn dispatch_to_node(
        &mut self,
        workload_id: WorkloadId,
        node_id: NodeId,
    ) -> Result<GatewayMessage, DispatchError> {
        // Verify node exists
        if self.registry.get_node(node_id).is_none() {
            return Err(DispatchError::NodeOffline(node_id));
        }

        // Get workload
        let workload = self
            .workload_mgr
            .get_workload(workload_id)
            .ok_or(DispatchError::WorkloadNotFound(workload_id))?;

        let spec = workload.workload.spec.clone();

        // Transition to Starting
        self.workload_mgr
            .update_state(workload_id, WorkloadState::Starting)
            .map_err(|e| match e {
                WorkloadManagerError::InvalidTransition(id, from, to) => {
                    DispatchError::InvalidTransition(id, from, to)
                }
                other => DispatchError::WorkloadManager(other),
            })?;

        // Remove from pending if it was there
        self.pending_dispatch.remove(&workload_id);

        // Create StartWorkload message
        Ok(GatewayMessage::StartWorkload {
            workload_id,
            spec,
        })
    }

    /// Handle a workload state update from a node.
    ///
    /// # Errors
    ///
    /// Returns an error if the workload is not found or the transition is invalid.
    pub fn handle_workload_update(
        &mut self,
        workload_id: WorkloadId,
        new_state: WorkloadState,
        _message: Option<String>,
    ) -> Result<(), DispatchError> {
        // Verify workload exists
        if self.workload_mgr.get_workload(workload_id).is_none() {
            return Err(DispatchError::WorkloadNotFound(workload_id));
        }

        // Update state
        self.workload_mgr
            .update_state(workload_id, new_state)
            .map_err(|e| match e {
                WorkloadManagerError::InvalidTransition(id, from, to) => {
                    DispatchError::InvalidTransition(id, from, to)
                }
                WorkloadManagerError::NotFound(id) => DispatchError::WorkloadNotFound(id),
                other => DispatchError::WorkloadManager(other),
            })?;

        // If terminal, remove from pending (in case it was there)
        if new_state.is_terminal() {
            self.pending_dispatch.remove(&workload_id);
        }

        Ok(())
    }

    /// Register a node with the dispatcher.
    ///
    /// # Errors
    ///
    /// Returns an error if the node is already registered.
    pub fn register_node(
        &mut self,
        node_id: NodeId,
        capabilities: NodeCapabilities,
    ) -> Result<NodeId, DispatchError> {
        self.registry.register(node_id, capabilities)?;
        Ok(node_id)
    }

    /// Unregister a node from the dispatcher.
    ///
    /// Any running workloads on the node are marked as failed.
    ///
    /// # Errors
    ///
    /// Returns an error if the node is not found.
    pub fn unregister_node(&mut self, node_id: NodeId) -> Result<(), DispatchError> {
        // Find workloads assigned to this node
        let affected_workloads: Vec<WorkloadId> = self
            .workload_mgr
            .list_by_node(node_id)
            .iter()
            .filter(|w| !w.state().is_terminal())
            .map(|w| w.id())
            .collect();

        // Fail all non-terminal workloads on this node
        for workload_id in affected_workloads {
            // Try to transition to Failed - ignore if already terminal
            let _ = self.workload_mgr.update_state(workload_id, WorkloadState::Failed);
        }

        // Unregister the node
        self.registry.unregister(node_id)?;
        Ok(())
    }

    /// Try to dispatch pending workloads to available nodes.
    ///
    /// Returns the list of workload IDs that were successfully dispatched.
    ///
    /// # Errors
    ///
    /// Returns an error if dispatch fails for reasons other than scheduling.
    pub fn try_dispatch_pending(&mut self) -> Result<Vec<WorkloadId>, DispatchError> {
        let mut dispatched = Vec::new();
        let pending: Vec<WorkloadId> = self.pending_dispatch.iter().copied().collect();

        for workload_id in pending {
            // Get the workload spec
            let Some(workload) = self.workload_mgr.get_workload(workload_id) else {
                // Workload no longer exists, remove from pending
                self.pending_dispatch.remove(&workload_id);
                continue;
            };
            let spec = workload.workload.spec.clone();

            // Try to schedule
            if let Ok(node_id) = self.scheduler.schedule(&spec, &self.registry) {
                // Assign to node
                if self
                    .workload_mgr
                    .assign_to_node(workload_id, node_id)
                    .is_ok()
                {
                    self.pending_dispatch.remove(&workload_id);
                    dispatched.push(workload_id);
                }
            }
        }

        Ok(dispatched)
    }

    /// Stop a running workload.
    ///
    /// Creates a `StopWorkload` message and transitions the workload to `Stopping`.
    ///
    /// # Errors
    ///
    /// Returns an error if the workload is not found or cannot be stopped.
    pub fn stop_workload(
        &mut self,
        workload_id: WorkloadId,
        grace_period_secs: u32,
    ) -> Result<GatewayMessage, DispatchError> {
        // Get current state
        let current_state = self
            .workload_mgr
            .get_workload(workload_id)
            .ok_or(DispatchError::WorkloadNotFound(workload_id))?
            .state();

        // Can only stop Running or Starting workloads
        if !matches!(current_state, WorkloadState::Running | WorkloadState::Starting) {
            return Err(DispatchError::CannotStop(workload_id, current_state));
        }

        // Transition to Stopping
        self.workload_mgr
            .update_state(workload_id, WorkloadState::Stopping)
            .map_err(|e| match e {
                WorkloadManagerError::InvalidTransition(id, from, to) => {
                    DispatchError::InvalidTransition(id, from, to)
                }
                other => DispatchError::WorkloadManager(other),
            })?;

        // Create StopWorkload message
        Ok(GatewayMessage::StopWorkload {
            workload_id,
            grace_period_secs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_proto::GpuCapability;

    // ==================== Helper Functions ====================

    fn make_capabilities(cpu: u32, memory: u64, gpus: Vec<GpuCapability>) -> NodeCapabilities {
        let mut caps = NodeCapabilities::new(cpu, memory);
        for gpu in gpus {
            caps = caps.with_gpu(gpu);
        }
        caps
    }

    fn make_valid_spec() -> WorkloadSpec {
        WorkloadSpec::new("nginx:latest")
            .with_memory_mb(1024)
            .with_cpu_cores(2)
    }

    // ==================== Basic Tests ====================

    #[test]
    fn test_dispatcher_new_has_no_pending() {
        let dispatcher = WorkloadDispatcher::new(
            NodeRegistry::new(),
            WorkloadManager::new(),
            Scheduler::new(),
        );
        assert_eq!(dispatcher.pending_count(), 0);
    }

    #[test]
    fn test_submit_with_no_nodes_adds_to_pending() {
        let mut dispatcher = WorkloadDispatcher::new(
            NodeRegistry::new(),
            WorkloadManager::new(),
            Scheduler::new(),
        );

        let id = dispatcher.submit(make_valid_spec()).expect("should succeed");
        assert_eq!(dispatcher.pending_count(), 1);
        assert!(dispatcher.get_workload(id).is_some());
    }

    #[test]
    fn test_submit_with_available_node_assigns() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();
        registry
            .register(node_id, make_capabilities(8, 16384, vec![]))
            .expect("should register");

        let mut dispatcher =
            WorkloadDispatcher::new(registry, WorkloadManager::new(), Scheduler::new());

        let id = dispatcher.submit(make_valid_spec()).expect("should succeed");
        assert_eq!(dispatcher.pending_count(), 0);

        let workload = dispatcher.get_workload(id).expect("should exist");
        assert_eq!(workload.assigned_node, Some(node_id));
    }
}
