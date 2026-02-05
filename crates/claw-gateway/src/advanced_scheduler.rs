//! Advanced workload scheduling with K8s-inspired features.
//!
//! This scheduler extends the basic scheduler with:
//! - CEL-based GPU selection with fallback chains
//! - Scheduling gates (workload holds)
//! - Node condition requirements
//! - Node label selectors
//! - Drain-aware scheduling

use std::collections::HashSet;

use claw_proto::{
    node_satisfies_requirements, CompletionMode, GpuSelector, MatchResult, NodeId,
    SchedulingRequirements, WorkloadSpec,
};
use thiserror::Error;

use crate::registry::{NodeHealthStatus, NodeRegistry, RegisteredNode};

/// Errors that can occur during advanced scheduling.
#[derive(Debug, Error)]
pub enum AdvancedSchedulerError {
    /// Workload is gated and cannot be scheduled yet.
    #[error("workload is gated: {pending_gates:?}")]
    Gated {
        /// Names of pending gates.
        pending_gates: Vec<String>,
    },

    /// No suitable node found for the workload.
    #[error("no suitable node: {reason}")]
    NoSuitableNode {
        /// Human-readable reason.
        reason: String,
        /// Nodes that were considered but rejected.
        rejected_nodes: Vec<RejectedNode>,
    },

    /// Registry is empty.
    #[error("no nodes registered")]
    NoNodes,

    /// Specific node requested but not found.
    #[error("node {0} not found")]
    NodeNotFound(NodeId),

    /// Specific node requested but not available.
    #[error("node {node_id} not available: {reason}")]
    NodeNotAvailable {
        /// The node ID.
        node_id: NodeId,
        /// Reason it's not available.
        reason: String,
    },
}

/// Information about why a node was rejected during scheduling.
#[derive(Debug, Clone)]
pub struct RejectedNode {
    /// The node ID.
    pub node_id: NodeId,
    /// The node name.
    pub name: String,
    /// Reason for rejection.
    pub reason: String,
}

/// Result of successful scheduling.
#[derive(Debug, Clone)]
pub struct ScheduleResult {
    /// The node selected for the workload.
    pub node_id: NodeId,
    /// GPU indices to use on the node.
    pub gpu_indices: Vec<u32>,
    /// Priority of the matched GPU requirement (for fallback tracking).
    pub gpu_priority: u32,
    /// Worker index for indexed parallel workloads.
    pub worker_index: Option<u32>,
}

/// Advanced GPU-aware workload scheduler.
#[derive(Debug)]
pub struct AdvancedScheduler {
    /// Minimum memory headroom (MiB) to leave on nodes.
    memory_headroom_mib: u64,
    /// GPU selector for CEL evaluation.
    gpu_selector: GpuSelector,
    /// Cleared scheduling gates (by workload).
    cleared_gates: HashSet<(claw_proto::WorkloadId, String)>,
}

impl Default for AdvancedScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl AdvancedScheduler {
    /// Create a new advanced scheduler.
    #[must_use]
    pub fn new() -> Self {
        Self {
            memory_headroom_mib: 0,
            gpu_selector: GpuSelector::new(),
            cleared_gates: HashSet::new(),
        }
    }

    /// Create a scheduler with memory headroom.
    #[must_use]
    pub fn with_memory_headroom(mut self, memory_headroom_mib: u64) -> Self {
        self.memory_headroom_mib = memory_headroom_mib;
        self
    }

    /// Clear a scheduling gate for a workload.
    pub fn clear_gate(&mut self, workload_id: claw_proto::WorkloadId, gate_name: &str) {
        self.cleared_gates.insert((workload_id, gate_name.to_string()));
    }

    /// Check if a gate is cleared for a workload.
    #[must_use]
    pub fn is_gate_cleared(&self, workload_id: claw_proto::WorkloadId, gate_name: &str) -> bool {
        self.cleared_gates.contains(&(workload_id, gate_name.to_string()))
    }

    /// Schedule a workload using advanced matching.
    ///
    /// # Algorithm
    ///
    /// 1. Check scheduling gates (return Gated if any pending)
    /// 2. Filter nodes by health status (only Healthy)
    /// 3. Filter nodes by label selector
    /// 4. Filter nodes by required conditions
    /// 5. Filter nodes by GPU requirements (CEL, fallback chain)
    /// 6. Filter nodes by basic resources (CPU, memory)
    /// 7. Score and select best node
    ///
    /// # Errors
    ///
    /// Returns an error if scheduling cannot proceed.
    pub fn schedule(
        &self,
        workload_id: claw_proto::WorkloadId,
        spec: &WorkloadSpec,
        registry: &NodeRegistry,
    ) -> Result<ScheduleResult, AdvancedSchedulerError> {
        // Step 1: Check scheduling gates
        self.check_gates(workload_id, spec)?;

        // Step 2: Get available nodes
        let available_nodes = registry.available_nodes();
        if available_nodes.is_empty() {
            if registry.is_empty() {
                return Err(AdvancedSchedulerError::NoNodes);
            }
            return Err(AdvancedSchedulerError::NoSuitableNode {
                reason: "all nodes are unhealthy or draining".to_string(),
                rejected_nodes: self.build_rejected_list(registry),
            });
        }

        // Step 3-6: Filter and score nodes
        let mut candidates: Vec<ScoredNode> = Vec::new();
        let mut rejected: Vec<RejectedNode> = Vec::new();

        for node in &available_nodes {
            match self.evaluate_node(node, spec) {
                Ok(scored) => candidates.push(scored),
                Err(reason) => rejected.push(RejectedNode {
                    node_id: node.id,
                    name: node.name.clone(),
                    reason,
                }),
            }
        }

        if candidates.is_empty() {
            return Err(AdvancedSchedulerError::NoSuitableNode {
                reason: self.summarize_rejections(&rejected),
                rejected_nodes: rejected,
            });
        }

        // Step 7: Select best node (highest score)
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        let best = &candidates[0];

        Ok(ScheduleResult {
            node_id: best.node_id,
            gpu_indices: best.gpu_indices.clone(),
            gpu_priority: best.gpu_priority,
            worker_index: None,
        })
    }

    /// Schedule to a specific node (for targeted placement).
    ///
    /// # Errors
    ///
    /// Returns an error if the node is not found or not suitable.
    pub fn schedule_to_node(
        &self,
        workload_id: claw_proto::WorkloadId,
        spec: &WorkloadSpec,
        target_node_id: NodeId,
        registry: &NodeRegistry,
    ) -> Result<ScheduleResult, AdvancedSchedulerError> {
        // Check gates first
        self.check_gates(workload_id, spec)?;

        // Find the target node
        let node = registry
            .get_node(target_node_id)
            .ok_or(AdvancedSchedulerError::NodeNotFound(target_node_id))?;

        // Check if node is available
        if !node.is_available() {
            return Err(AdvancedSchedulerError::NodeNotAvailable {
                node_id: target_node_id,
                reason: format!("node is {}", node.health_status()),
            });
        }

        // Evaluate the node
        match self.evaluate_node(node, spec) {
            Ok(scored) => Ok(ScheduleResult {
                node_id: scored.node_id,
                gpu_indices: scored.gpu_indices,
                gpu_priority: scored.gpu_priority,
                worker_index: None,
            }),
            Err(reason) => Err(AdvancedSchedulerError::NodeNotAvailable {
                node_id: target_node_id,
                reason,
            }),
        }
    }

    /// Schedule an indexed parallel workload.
    ///
    /// Returns a schedule result for each worker index.
    ///
    /// # Errors
    ///
    /// Returns an error if not all workers can be scheduled.
    pub fn schedule_parallel(
        &self,
        workload_id: claw_proto::WorkloadId,
        spec: &WorkloadSpec,
        registry: &NodeRegistry,
    ) -> Result<Vec<ScheduleResult>, AdvancedSchedulerError> {
        // Check gates first
        self.check_gates(workload_id, spec)?;

        let parallel_config = spec.parallel_config().ok_or_else(|| {
            AdvancedSchedulerError::NoSuitableNode {
                reason: "workload is not configured for parallel execution".to_string(),
                rejected_nodes: vec![],
            }
        })?;

        let worker_count = parallel_config.completions;
        let is_indexed = parallel_config.completion_mode == CompletionMode::Indexed;

        // Get available nodes
        let available_nodes = registry.available_nodes();
        if available_nodes.is_empty() {
            return Err(AdvancedSchedulerError::NoNodes);
        }

        // Score all nodes
        let mut candidates: Vec<ScoredNode> = Vec::new();
        for node in &available_nodes {
            if let Ok(scored) = self.evaluate_node(node, spec) {
                candidates.push(scored);
            }
        }

        if candidates.len() < worker_count as usize {
            return Err(AdvancedSchedulerError::NoSuitableNode {
                reason: format!(
                    "need {} workers, only {} suitable nodes",
                    worker_count,
                    candidates.len()
                ),
                rejected_nodes: vec![],
            });
        }

        // Sort by score and assign workers
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        let mut results = Vec::with_capacity(worker_count as usize);
        for (i, candidate) in candidates.into_iter().take(worker_count as usize).enumerate() {
            results.push(ScheduleResult {
                node_id: candidate.node_id,
                gpu_indices: candidate.gpu_indices,
                gpu_priority: candidate.gpu_priority,
                worker_index: if is_indexed { Some(i as u32) } else { None },
            });
        }

        Ok(results)
    }

    /// Check if scheduling gates are satisfied.
    fn check_gates(
        &self,
        workload_id: claw_proto::WorkloadId,
        spec: &WorkloadSpec,
    ) -> Result<(), AdvancedSchedulerError> {
        let pending: Vec<String> = spec
            .scheduling
            .scheduling_gates
            .iter()
            .filter(|gate| !self.is_gate_cleared(workload_id, &gate.name))
            .map(|gate| gate.name.clone())
            .collect();

        if pending.is_empty() {
            Ok(())
        } else {
            Err(AdvancedSchedulerError::Gated {
                pending_gates: pending,
            })
        }
    }

    /// Evaluate a single node against the workload spec.
    fn evaluate_node(
        &self,
        node: &RegisteredNode,
        spec: &WorkloadSpec,
    ) -> Result<ScoredNode, String> {
        // Check scheduling requirements (labels, conditions)
        if !node_satisfies_requirements(&node.capabilities, &spec.scheduling) {
            return Err(self.explain_requirement_failure(node, &spec.scheduling));
        }

        // Check basic resources
        let required_memory = spec.memory_mb + self.memory_headroom_mib;
        if node.capabilities.memory_mib < required_memory {
            return Err(format!(
                "insufficient memory: need {} MiB, have {} MiB",
                required_memory, node.capabilities.memory_mib
            ));
        }

        if node.capabilities.cpu_cores < spec.cpu_cores {
            return Err(format!(
                "insufficient CPU: need {} cores, have {} cores",
                spec.cpu_cores, node.capabilities.cpu_cores
            ));
        }

        // Check GPU requirements
        let (gpu_indices, gpu_priority) = self.evaluate_gpu_requirement(node, spec)?;

        // Calculate score
        let score = self.calculate_score(node, spec, gpu_priority);

        Ok(ScoredNode {
            node_id: node.id,
            score,
            gpu_indices,
            gpu_priority,
        })
    }

    /// Evaluate GPU requirements against a node.
    fn evaluate_gpu_requirement(
        &self,
        node: &RegisteredNode,
        spec: &WorkloadSpec,
    ) -> Result<(Vec<u32>, u32), String> {
        // If there's an advanced GPU requirement, use the selector
        if let Some(gpu_req) = &spec.scheduling.gpu_requirement {
            let result = self.gpu_selector.match_requirement(gpu_req, &node.capabilities);
            match result {
                MatchResult::Match {
                    matched_gpus,
                    priority,
                } => Ok((matched_gpus, priority)),
                MatchResult::NoMatch { reason } => Err(reason),
            }
        } else if spec.gpu_count > 0 {
            // Fall back to simple GPU count
            if node.gpu_count() >= spec.gpu_count as usize {
                let indices: Vec<u32> = (0..spec.gpu_count).collect();
                Ok((indices, 0))
            } else {
                Err(format!(
                    "insufficient GPUs: need {}, have {}",
                    spec.gpu_count,
                    node.gpu_count()
                ))
            }
        } else {
            // No GPU requirement
            Ok((vec![], 0))
        }
    }

    /// Calculate a scheduling score for a node.
    fn calculate_score(&self, node: &RegisteredNode, spec: &WorkloadSpec, gpu_priority: u32) -> f64 {
        let mut score = 100.0;

        // Prefer nodes with exact GPU match (avoid wasting resources)
        let gpu_diff = node.gpu_count().abs_diff(spec.effective_gpu_count() as usize);
        score -= (gpu_diff as f64) * 5.0;

        // Prefer higher GPU priority (from fallback chain)
        score += (gpu_priority as f64) * 10.0;

        // Prefer nodes with more available memory (spread load)
        let memory_ratio = node.capabilities.memory_mib as f64 / spec.memory_mb.max(1) as f64;
        score += memory_ratio.min(2.0) * 5.0;

        // Prefer nodes with matching labels (affinity)
        let label_matches = spec
            .scheduling
            .node_selector
            .iter()
            .filter(|(k, v)| node.capabilities.labels.get(*k) == Some(*v))
            .count();
        score += (label_matches as f64) * 3.0;

        score
    }

    /// Explain why a node failed requirement checks.
    fn explain_requirement_failure(
        &self,
        node: &RegisteredNode,
        requirements: &SchedulingRequirements,
    ) -> String {
        // Check labels
        for (key, value) in &requirements.node_selector {
            if node.capabilities.labels.get(key) != Some(value) {
                return format!(
                    "label mismatch: need {}={}, have {:?}",
                    key,
                    value,
                    node.capabilities.labels.get(key)
                );
            }
        }

        // Check conditions
        for cond_req in &requirements.required_conditions {
            if !node.capabilities.is_condition_satisfied(&cond_req.condition_type) {
                return format!("condition not satisfied: {}", cond_req.condition_type);
            }
        }

        "requirements not satisfied".to_string()
    }

    /// Build a list of rejected nodes for error reporting.
    fn build_rejected_list(&self, registry: &NodeRegistry) -> Vec<RejectedNode> {
        registry
            .list_nodes()
            .into_iter()
            .filter(|n| !n.is_available())
            .map(|n| RejectedNode {
                node_id: n.id,
                name: n.name.clone(),
                reason: format!("status: {}", n.health_status()),
            })
            .collect()
    }

    /// Summarize rejection reasons for error messages.
    fn summarize_rejections(&self, rejected: &[RejectedNode]) -> String {
        if rejected.is_empty() {
            return "no nodes available".to_string();
        }

        // Group by reason
        let mut reasons: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for r in rejected {
            *reasons.entry(r.reason.as_str()).or_insert(0) += 1;
        }

        let summary: Vec<String> = reasons
            .into_iter()
            .map(|(reason, count)| format!("{} nodes: {}", count, reason))
            .collect();

        summary.join("; ")
    }
}

/// A node that passed initial filtering with a score.
#[derive(Debug)]
struct ScoredNode {
    node_id: NodeId,
    score: f64,
    gpu_indices: Vec<u32>,
    gpu_priority: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_proto::{
        ConditionRequirement, ConditionStatus, GpuCapability, GpuRequirement, NodeCapabilities,
        NodeCondition, ParallelConfig, SchedulingGate, WorkloadId,
    };

    // ========================================================================
    // Helper Functions
    // ========================================================================

    fn make_gpu(index: u32, name: &str, memory_mib: u64) -> GpuCapability {
        GpuCapability {
            index,
            name: name.to_string(),
            memory_mib,
            uuid: format!("GPU-{}", index),
        }
    }

    fn make_capabilities(cpu: u32, memory: u64, gpus: Vec<GpuCapability>) -> NodeCapabilities {
        let mut caps = NodeCapabilities::new(cpu, memory);
        for gpu in gpus {
            caps = caps.with_gpu(gpu);
        }
        caps
    }

    fn register_node(
        registry: &mut NodeRegistry,
        name: &str,
        cpu: u32,
        memory: u64,
        gpus: Vec<GpuCapability>,
    ) -> NodeId {
        let node_id = NodeId::new();
        let caps = make_capabilities(cpu, memory, gpus);
        registry.register_with_name(node_id, name, caps).unwrap();
        node_id
    }

    // ========================================================================
    // Basic Scheduling Tests
    // ========================================================================

    #[test]
    fn test_schedule_simple_workload() {
        let scheduler = AdvancedScheduler::new();
        let mut registry = NodeRegistry::new();

        let node_id = register_node(&mut registry, "node-1", 8, 16384, vec![]);
        let workload_id = WorkloadId::new();
        let spec = WorkloadSpec::new("nginx:latest")
            .with_cpu_cores(2)
            .with_memory_mb(1024);

        let result = scheduler.schedule(workload_id, &spec, &registry);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().node_id, node_id);
    }

    #[test]
    fn test_schedule_no_nodes() {
        let scheduler = AdvancedScheduler::new();
        let registry = NodeRegistry::new();
        let workload_id = WorkloadId::new();
        let spec = WorkloadSpec::new("nginx:latest");

        let result = scheduler.schedule(workload_id, &spec, &registry);

        assert!(matches!(result, Err(AdvancedSchedulerError::NoNodes)));
    }

    // ========================================================================
    // Scheduling Gates Tests
    // ========================================================================

    #[test]
    fn test_schedule_gated_workload_blocked() {
        let scheduler = AdvancedScheduler::new();
        let mut registry = NodeRegistry::new();

        register_node(&mut registry, "node-1", 8, 16384, vec![]);
        let workload_id = WorkloadId::new();
        let spec = WorkloadSpec::new("pytorch:latest")
            .with_scheduling_gate(SchedulingGate::new("model-loaded"));

        let result = scheduler.schedule(workload_id, &spec, &registry);

        match result {
            Err(AdvancedSchedulerError::Gated { pending_gates }) => {
                assert_eq!(pending_gates, vec!["model-loaded"]);
            }
            _ => panic!("Expected Gated error"),
        }
    }

    #[test]
    fn test_schedule_gate_cleared() {
        let mut scheduler = AdvancedScheduler::new();
        let mut registry = NodeRegistry::new();

        register_node(&mut registry, "node-1", 8, 16384, vec![]);
        let workload_id = WorkloadId::new();
        let spec = WorkloadSpec::new("pytorch:latest")
            .with_scheduling_gate(SchedulingGate::new("model-loaded"));

        // Clear the gate
        scheduler.clear_gate(workload_id, "model-loaded");

        let result = scheduler.schedule(workload_id, &spec, &registry);

        assert!(result.is_ok());
    }

    #[test]
    fn test_schedule_multiple_gates() {
        let mut scheduler = AdvancedScheduler::new();
        let mut registry = NodeRegistry::new();

        register_node(&mut registry, "node-1", 8, 16384, vec![]);
        let workload_id = WorkloadId::new();
        let spec = WorkloadSpec::new("pytorch:latest")
            .with_scheduling_gate(SchedulingGate::new("model-loaded"))
            .with_scheduling_gate(SchedulingGate::new("vram-warm"));

        // Clear only one gate
        scheduler.clear_gate(workload_id, "model-loaded");

        let result = scheduler.schedule(workload_id, &spec, &registry);

        match result {
            Err(AdvancedSchedulerError::Gated { pending_gates }) => {
                assert_eq!(pending_gates, vec!["vram-warm"]);
            }
            _ => panic!("Expected Gated error"),
        }
    }

    // ========================================================================
    // GPU Selection Tests
    // ========================================================================

    #[test]
    fn test_schedule_with_gpu_requirement() {
        let scheduler = AdvancedScheduler::new();
        let mut registry = NodeRegistry::new();

        let node_id = register_node(
            &mut registry,
            "gpu-node",
            8,
            32768,
            vec![make_gpu(0, "NVIDIA A100-80GB", 81920)],
        );

        let workload_id = WorkloadId::new();
        let spec = WorkloadSpec::new("pytorch:latest").with_gpu_requirement(
            GpuRequirement::new(1).with_min_memory_mib(40960),
        );

        let result = scheduler.schedule(workload_id, &spec, &registry);

        assert!(result.is_ok());
        let schedule = result.unwrap();
        assert_eq!(schedule.node_id, node_id);
        assert_eq!(schedule.gpu_indices, vec![0]);
    }

    #[test]
    fn test_schedule_gpu_fallback() {
        let scheduler = AdvancedScheduler::new();
        let mut registry = NodeRegistry::new();

        // Node has RTX 4090, not A100
        let node_id = register_node(
            &mut registry,
            "rtx-node",
            8,
            32768,
            vec![
                make_gpu(0, "NVIDIA RTX 4090", 24576),
                make_gpu(1, "NVIDIA RTX 4090", 24576),
            ],
        );

        let workload_id = WorkloadId::new();
        let spec = WorkloadSpec::new("pytorch:latest").with_gpu_requirement(
            GpuRequirement::new(1)
                .with_model_pattern("A100")
                .with_priority(10)
                .with_fallback(GpuRequirement::new(2).with_priority(5)),
        );

        let result = scheduler.schedule(workload_id, &spec, &registry);

        assert!(result.is_ok());
        let schedule = result.unwrap();
        assert_eq!(schedule.node_id, node_id);
        assert_eq!(schedule.gpu_priority, 5); // Used fallback
        assert_eq!(schedule.gpu_indices.len(), 2);
    }

    // ========================================================================
    // Node Condition Tests
    // ========================================================================

    #[test]
    fn test_schedule_requires_condition() {
        let scheduler = AdvancedScheduler::new();
        let mut registry = NodeRegistry::new();

        // Node without condition
        register_node(&mut registry, "no-cuda", 8, 16384, vec![]);

        // Node with condition
        let caps_with_condition = make_capabilities(8, 16384, vec![])
            .with_condition(NodeCondition::new("cuda-12-ready", ConditionStatus::True));
        let node_id = NodeId::new();
        registry
            .register_with_name(node_id, "cuda-node", caps_with_condition)
            .unwrap();

        let workload_id = WorkloadId::new();
        let spec = WorkloadSpec::new("cuda-app:latest")
            .with_required_condition(ConditionRequirement::must_be_true("cuda-12-ready"));

        let result = scheduler.schedule(workload_id, &spec, &registry);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().node_id, node_id);
    }

    // ========================================================================
    // Node Selector Tests
    // ========================================================================

    #[test]
    fn test_schedule_with_node_selector() {
        let scheduler = AdvancedScheduler::new();
        let mut registry = NodeRegistry::new();

        // Node without label
        register_node(&mut registry, "no-label", 8, 16384, vec![]);

        // Node with label
        let caps_with_label =
            make_capabilities(8, 16384, vec![]).with_label("gpu-type", "nvidia");
        let node_id = NodeId::new();
        registry
            .register_with_name(node_id, "nvidia-node", caps_with_label)
            .unwrap();

        let workload_id = WorkloadId::new();
        let spec =
            WorkloadSpec::new("cuda-app:latest").with_node_selector("gpu-type", "nvidia");

        let result = scheduler.schedule(workload_id, &spec, &registry);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().node_id, node_id);
    }

    // ========================================================================
    // Parallel Scheduling Tests
    // ========================================================================

    #[test]
    fn test_schedule_parallel_indexed() {
        let scheduler = AdvancedScheduler::new();
        let mut registry = NodeRegistry::new();

        // Register 4 nodes
        for i in 0..4 {
            register_node(&mut registry, &format!("node-{}", i), 8, 16384, vec![]);
        }

        let workload_id = WorkloadId::new();
        let spec = WorkloadSpec::new("pytorch-ddp:latest")
            .with_parallel(ParallelConfig::indexed(4));

        let result = scheduler.schedule_parallel(workload_id, &spec, &registry);

        assert!(result.is_ok());
        let schedules = result.unwrap();
        assert_eq!(schedules.len(), 4);

        // Check worker indices are assigned
        let indices: Vec<_> = schedules.iter().map(|s| s.worker_index).collect();
        assert_eq!(indices, vec![Some(0), Some(1), Some(2), Some(3)]);
    }

    #[test]
    fn test_schedule_parallel_not_enough_nodes() {
        let scheduler = AdvancedScheduler::new();
        let mut registry = NodeRegistry::new();

        // Only 2 nodes
        register_node(&mut registry, "node-0", 8, 16384, vec![]);
        register_node(&mut registry, "node-1", 8, 16384, vec![]);

        let workload_id = WorkloadId::new();
        let spec = WorkloadSpec::new("pytorch-ddp:latest")
            .with_parallel(ParallelConfig::indexed(4)); // Needs 4

        let result = scheduler.schedule_parallel(workload_id, &spec, &registry);

        assert!(matches!(
            result,
            Err(AdvancedSchedulerError::NoSuitableNode { .. })
        ));
    }

    // ========================================================================
    // Targeted Scheduling Tests
    // ========================================================================

    #[test]
    fn test_schedule_to_specific_node() {
        let scheduler = AdvancedScheduler::new();
        let mut registry = NodeRegistry::new();

        register_node(&mut registry, "node-1", 8, 16384, vec![]);
        let target_id = register_node(&mut registry, "node-2", 8, 16384, vec![]);

        let workload_id = WorkloadId::new();
        let spec = WorkloadSpec::new("nginx:latest");

        let result = scheduler.schedule_to_node(workload_id, &spec, target_id, &registry);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().node_id, target_id);
    }

    #[test]
    fn test_schedule_to_nonexistent_node() {
        let scheduler = AdvancedScheduler::new();
        let mut registry = NodeRegistry::new();

        register_node(&mut registry, "node-1", 8, 16384, vec![]);
        let fake_id = NodeId::new();

        let workload_id = WorkloadId::new();
        let spec = WorkloadSpec::new("nginx:latest");

        let result = scheduler.schedule_to_node(workload_id, &spec, fake_id, &registry);

        assert!(matches!(
            result,
            Err(AdvancedSchedulerError::NodeNotFound(_))
        ));
    }

    // ========================================================================
    // Error Reporting Tests
    // ========================================================================

    #[test]
    fn test_rejection_reasons_in_error() {
        let scheduler = AdvancedScheduler::new();
        let mut registry = NodeRegistry::new();

        // Node with insufficient memory
        register_node(&mut registry, "small-node", 8, 1024, vec![]);

        let workload_id = WorkloadId::new();
        let spec = WorkloadSpec::new("big-app:latest").with_memory_mb(8192);

        let result = scheduler.schedule(workload_id, &spec, &registry);

        match result {
            Err(AdvancedSchedulerError::NoSuitableNode {
                rejected_nodes, ..
            }) => {
                assert_eq!(rejected_nodes.len(), 1);
                assert!(rejected_nodes[0].reason.contains("memory"));
            }
            _ => panic!("Expected NoSuitableNode error"),
        }
    }
}
