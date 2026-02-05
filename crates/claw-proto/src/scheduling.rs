//! Kubernetes-inspired scheduling primitives for AI workloads.
//!
//! This module provides advanced scheduling features:
//! - **Scheduling Gates**: Hold workloads until dependencies are ready
//! - **CEL-Based GPU Selection**: Fine-grained GPU matching with fallbacks
//! - **Indexed Parallel Jobs**: Distributed training with deterministic indices
//! - **Custom Node Conditions**: Beyond binary Ready/NotReady

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// Scheduling Gates
// ============================================================================

/// A gate that must be cleared before a workload can be scheduled.
///
/// Gates allow holding workloads until external conditions are met:
/// - Model loaded into VRAM
/// - Dependencies deployed
/// - Resource quotas available
/// - Manual approval
///
/// # Example
///
/// ```rust
/// use claw_proto::scheduling::SchedulingGate;
///
/// let gate = SchedulingGate::new("model-loaded")
///     .with_reason("Waiting for Llama-70B to be cached on node");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchedulingGate {
    /// Unique name for this gate (e.g., "model-loaded", "vram-warm").
    pub name: String,
    /// Human-readable reason for the gate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Controller responsible for clearing this gate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller: Option<String>,
}

impl SchedulingGate {
    /// Create a new scheduling gate.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            reason: None,
            controller: None,
        }
    }

    /// Set the reason for this gate.
    #[must_use]
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Set the controller responsible for this gate.
    #[must_use]
    pub fn with_controller(mut self, controller: impl Into<String>) -> Self {
        self.controller = Some(controller.into());
        self
    }
}

/// Result of evaluating scheduling gates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateStatus {
    /// All gates cleared, workload can be scheduled.
    Ready,
    /// Workload is gated, waiting for conditions.
    Gated {
        /// Names of gates that are still blocking.
        pending_gates: Vec<String>,
    },
}

impl GateStatus {
    /// Check if the workload is ready to schedule.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }
}

// ============================================================================
// CEL-Based GPU Selection
// ============================================================================

/// GPU selection requirement with optional CEL expression and fallback chain.
///
/// Allows fine-grained GPU selection beyond simple count:
/// - Filter by VRAM, model name, compute capability
/// - Prioritized fallback if primary selection unavailable
///
/// # Example
///
/// ```rust
/// use claw_proto::scheduling::GpuRequirement;
///
/// // Primary: A100 with 80GB, fallback to any GPU with 40GB+
/// let req = GpuRequirement::new(1)
///     .with_selector("device.memory_mib >= 81920 && device.name.contains('A100')")
///     .with_fallback(
///         GpuRequirement::new(1)
///             .with_selector("device.memory_mib >= 40960")
///     );
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GpuRequirement {
    /// Number of GPUs required.
    pub count: u32,
    /// CEL expression for GPU selection.
    ///
    /// Available variables:
    /// - `device.index`: GPU index (u32)
    /// - `device.name`: GPU model name (string)
    /// - `device.memory_mib`: Total VRAM in MiB (u64)
    /// - `device.uuid`: GPU UUID (string)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// Minimum VRAM per GPU in MiB (shorthand for CEL).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_memory_mib: Option<u64>,
    /// Required GPU model name substring (shorthand for CEL).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_pattern: Option<String>,
    /// Fallback requirement if primary cannot be satisfied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Box<GpuRequirement>>,
    /// Priority of this requirement (higher = preferred).
    #[serde(default)]
    pub priority: u32,
}

impl GpuRequirement {
    /// Create a new GPU requirement.
    #[must_use]
    pub const fn new(count: u32) -> Self {
        Self {
            count,
            selector: None,
            min_memory_mib: None,
            model_pattern: None,
            fallback: None,
            priority: 0,
        }
    }

    /// Set a CEL selector expression.
    #[must_use]
    pub fn with_selector(mut self, selector: impl Into<String>) -> Self {
        self.selector = Some(selector.into());
        self
    }

    /// Set minimum VRAM requirement.
    #[must_use]
    pub const fn with_min_memory_mib(mut self, memory_mib: u64) -> Self {
        self.min_memory_mib = Some(memory_mib);
        self
    }

    /// Set model name pattern (e.g., "A100", "H100", "RTX").
    #[must_use]
    pub fn with_model_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.model_pattern = Some(pattern.into());
        self
    }

    /// Add a fallback requirement.
    #[must_use]
    pub fn with_fallback(mut self, fallback: GpuRequirement) -> Self {
        self.fallback = Some(Box::new(fallback));
        self
    }

    /// Set priority (higher = preferred when scoring nodes).
    #[must_use]
    pub const fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    /// Check if this requirement has any selection criteria beyond count.
    #[must_use]
    pub fn has_selection_criteria(&self) -> bool {
        self.selector.is_some() || self.min_memory_mib.is_some() || self.model_pattern.is_some()
    }

    /// Get the fallback chain as an iterator.
    pub fn fallback_chain(&self) -> FallbackChainIter<'_> {
        FallbackChainIter {
            current: Some(self),
        }
    }
}

impl Default for GpuRequirement {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Iterator over the fallback chain of GPU requirements.
pub struct FallbackChainIter<'a> {
    current: Option<&'a GpuRequirement>,
}

impl<'a> Iterator for FallbackChainIter<'a> {
    type Item = &'a GpuRequirement;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.current?;
        self.current = result.fallback.as_deref();
        Some(result)
    }
}

// ============================================================================
// Indexed Parallel Jobs
// ============================================================================

/// Completion mode for parallel workloads.
///
/// Inspired by Kubernetes Jobs completion modes for distributed training.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CompletionMode {
    /// Non-indexed: any N successful completions (default).
    #[default]
    NonIndexed,
    /// Indexed: exactly N workers with indices 0..N-1.
    ///
    /// Each worker gets:
    /// - `WORKLOAD_COMPLETION_INDEX` env var
    /// - Deterministic hostname for peer discovery
    Indexed,
}

/// Parallel workload configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParallelConfig {
    /// Total number of completions required.
    pub completions: u32,
    /// Maximum concurrent workers.
    pub parallelism: u32,
    /// Completion mode (indexed or non-indexed).
    #[serde(default)]
    pub completion_mode: CompletionMode,
    /// Backoff limit for failed workers.
    #[serde(default = "default_backoff_limit")]
    pub backoff_limit: u32,
    /// Per-index backoff limit (for indexed mode).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backoff_limit_per_index: Option<u32>,
}

const fn default_backoff_limit() -> u32 {
    6
}

impl ParallelConfig {
    /// Create a simple parallel config.
    #[must_use]
    pub const fn new(completions: u32, parallelism: u32) -> Self {
        Self {
            completions,
            parallelism,
            completion_mode: CompletionMode::NonIndexed,
            backoff_limit: 6,
            backoff_limit_per_index: None,
        }
    }

    /// Create an indexed parallel config for distributed training.
    #[must_use]
    pub const fn indexed(workers: u32) -> Self {
        Self {
            completions: workers,
            parallelism: workers,
            completion_mode: CompletionMode::Indexed,
            backoff_limit: 6,
            backoff_limit_per_index: Some(1),
        }
    }

    /// Set completion mode.
    #[must_use]
    pub const fn with_completion_mode(mut self, mode: CompletionMode) -> Self {
        self.completion_mode = mode;
        self
    }

    /// Set backoff limit.
    #[must_use]
    pub const fn with_backoff_limit(mut self, limit: u32) -> Self {
        self.backoff_limit = limit;
        self
    }
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self::new(1, 1)
    }
}

// ============================================================================
// Custom Node Conditions
// ============================================================================

/// Status of a node condition.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConditionStatus {
    /// Condition is true.
    True,
    /// Condition is false.
    False,
    /// Condition status is unknown.
    #[default]
    Unknown,
}

impl ConditionStatus {
    /// Check if condition is satisfied.
    #[must_use]
    pub const fn is_true(&self) -> bool {
        matches!(self, Self::True)
    }
}

/// A custom condition reported by a node.
///
/// Extends beyond binary Ready/NotReady to express:
/// - CUDA driver version compatibility
/// - Model cache status
/// - Network agent readiness
/// - Custom health checks
///
/// # Example
///
/// ```rust
/// use claw_proto::scheduling::{NodeCondition, ConditionStatus};
/// use chrono::Utc;
///
/// let condition = NodeCondition::new("model-llama-70b-cached", ConditionStatus::True)
///     .with_reason("CacheHit")
///     .with_message("Model cached at /models/llama-70b");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeCondition {
    /// Condition type (e.g., "cuda-ready", "model-cached").
    pub condition_type: String,
    /// Current status of the condition.
    pub status: ConditionStatus,
    /// Last time the condition transitioned.
    pub last_transition_time: DateTime<Utc>,
    /// Last time the condition was probed.
    pub last_probe_time: DateTime<Utc>,
    /// Machine-readable reason for the status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Human-readable message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Observed generation (for cache invalidation).
    #[serde(default)]
    pub observed_generation: u64,
}

impl NodeCondition {
    /// Create a new node condition.
    #[must_use]
    pub fn new(condition_type: impl Into<String>, status: ConditionStatus) -> Self {
        let now = Utc::now();
        Self {
            condition_type: condition_type.into(),
            status,
            last_transition_time: now,
            last_probe_time: now,
            reason: None,
            message: None,
            observed_generation: 0,
        }
    }

    /// Set the reason.
    #[must_use]
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Set the message.
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Update the condition status.
    pub fn update_status(&mut self, status: ConditionStatus) {
        if self.status != status {
            self.status = status;
            self.last_transition_time = Utc::now();
        }
        self.last_probe_time = Utc::now();
    }

    /// Check if this condition is satisfied.
    #[must_use]
    pub const fn is_satisfied(&self) -> bool {
        self.status.is_true()
    }
}

/// A requirement that a node condition must be satisfied.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConditionRequirement {
    /// Condition type to check.
    pub condition_type: String,
    /// Required status (defaults to True).
    #[serde(default = "default_required_status")]
    pub required_status: ConditionStatus,
}

const fn default_required_status() -> ConditionStatus {
    ConditionStatus::True
}

impl ConditionRequirement {
    /// Create a requirement for a condition to be true.
    #[must_use]
    pub fn must_be_true(condition_type: impl Into<String>) -> Self {
        Self {
            condition_type: condition_type.into(),
            required_status: ConditionStatus::True,
        }
    }

    /// Create a requirement for a condition to be false.
    #[must_use]
    pub fn must_be_false(condition_type: impl Into<String>) -> Self {
        Self {
            condition_type: condition_type.into(),
            required_status: ConditionStatus::False,
        }
    }

    /// Check if a condition satisfies this requirement.
    #[must_use]
    pub fn is_satisfied_by(&self, condition: &NodeCondition) -> bool {
        condition.condition_type == self.condition_type && condition.status == self.required_status
    }
}

// ============================================================================
// Workload Scheduling Requirements
// ============================================================================

/// Complete scheduling requirements for a workload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SchedulingRequirements {
    /// Gates that must be cleared before scheduling.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scheduling_gates: Vec<SchedulingGate>,
    /// GPU requirements with CEL selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_requirement: Option<GpuRequirement>,
    /// Node conditions that must be satisfied.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_conditions: Vec<ConditionRequirement>,
    /// Parallel execution configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel: Option<ParallelConfig>,
    /// Node selector labels.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub node_selector: std::collections::HashMap<String, String>,
}

impl SchedulingRequirements {
    /// Create empty scheduling requirements.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a scheduling gate.
    #[must_use]
    pub fn with_gate(mut self, gate: SchedulingGate) -> Self {
        self.scheduling_gates.push(gate);
        self
    }

    /// Set GPU requirement.
    #[must_use]
    pub fn with_gpu_requirement(mut self, requirement: GpuRequirement) -> Self {
        self.gpu_requirement = Some(requirement);
        self
    }

    /// Add a required node condition.
    #[must_use]
    pub fn with_condition(mut self, requirement: ConditionRequirement) -> Self {
        self.required_conditions.push(requirement);
        self
    }

    /// Set parallel configuration.
    #[must_use]
    pub fn with_parallel(mut self, config: ParallelConfig) -> Self {
        self.parallel = Some(config);
        self
    }

    /// Add a node selector label.
    #[must_use]
    pub fn with_node_selector(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.node_selector.insert(key.into(), value.into());
        self
    }

    /// Check if this workload has any gates.
    #[must_use]
    pub fn is_gated(&self) -> bool {
        !self.scheduling_gates.is_empty()
    }

    /// Check if this is a parallel workload.
    #[must_use]
    pub fn is_parallel(&self) -> bool {
        self.parallel.as_ref().is_some_and(|p| p.completions > 1)
    }

    /// Check if this is an indexed workload.
    #[must_use]
    pub fn is_indexed(&self) -> bool {
        self.parallel
            .as_ref()
            .is_some_and(|p| p.completion_mode == CompletionMode::Indexed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Scheduling Gate Tests
    // ========================================================================

    #[test]
    fn test_scheduling_gate_new() {
        let gate = SchedulingGate::new("model-loaded");
        assert_eq!(gate.name, "model-loaded");
        assert!(gate.reason.is_none());
        assert!(gate.controller.is_none());
    }

    #[test]
    fn test_scheduling_gate_with_reason() {
        let gate = SchedulingGate::new("vram-warm")
            .with_reason("Waiting for VRAM preallocation")
            .with_controller("vram-controller");

        assert_eq!(gate.name, "vram-warm");
        assert_eq!(gate.reason.as_deref(), Some("Waiting for VRAM preallocation"));
        assert_eq!(gate.controller.as_deref(), Some("vram-controller"));
    }

    #[test]
    fn test_gate_status_is_ready() {
        assert!(GateStatus::Ready.is_ready());
        assert!(!GateStatus::Gated {
            pending_gates: vec!["test".into()]
        }
        .is_ready());
    }

    // ========================================================================
    // GPU Requirement Tests
    // ========================================================================

    #[test]
    fn test_gpu_requirement_new() {
        let req = GpuRequirement::new(2);
        assert_eq!(req.count, 2);
        assert!(req.selector.is_none());
        assert!(req.fallback.is_none());
    }

    #[test]
    fn test_gpu_requirement_with_selector() {
        let req = GpuRequirement::new(1)
            .with_selector("device.memory_mib >= 81920")
            .with_min_memory_mib(40960)
            .with_model_pattern("A100");

        assert_eq!(req.count, 1);
        assert_eq!(
            req.selector.as_deref(),
            Some("device.memory_mib >= 81920")
        );
        assert_eq!(req.min_memory_mib, Some(40960));
        assert_eq!(req.model_pattern.as_deref(), Some("A100"));
    }

    #[test]
    fn test_gpu_requirement_fallback_chain() {
        let req = GpuRequirement::new(1)
            .with_model_pattern("A100")
            .with_priority(10)
            .with_fallback(
                GpuRequirement::new(1)
                    .with_model_pattern("H100")
                    .with_priority(5)
                    .with_fallback(GpuRequirement::new(2).with_min_memory_mib(24000)),
            );

        let chain: Vec<_> = req.fallback_chain().collect();
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[0].priority, 10);
        assert_eq!(chain[1].priority, 5);
        assert_eq!(chain[2].count, 2);
    }

    #[test]
    fn test_gpu_requirement_has_selection_criteria() {
        assert!(!GpuRequirement::new(1).has_selection_criteria());
        assert!(GpuRequirement::new(1)
            .with_selector("test")
            .has_selection_criteria());
        assert!(GpuRequirement::new(1)
            .with_min_memory_mib(1000)
            .has_selection_criteria());
    }

    // ========================================================================
    // Parallel Config Tests
    // ========================================================================

    #[test]
    fn test_parallel_config_new() {
        let config = ParallelConfig::new(10, 5);
        assert_eq!(config.completions, 10);
        assert_eq!(config.parallelism, 5);
        assert_eq!(config.completion_mode, CompletionMode::NonIndexed);
    }

    #[test]
    fn test_parallel_config_indexed() {
        let config = ParallelConfig::indexed(8);
        assert_eq!(config.completions, 8);
        assert_eq!(config.parallelism, 8);
        assert_eq!(config.completion_mode, CompletionMode::Indexed);
        assert_eq!(config.backoff_limit_per_index, Some(1));
    }

    #[test]
    fn test_completion_mode_default() {
        assert_eq!(CompletionMode::default(), CompletionMode::NonIndexed);
    }

    // ========================================================================
    // Node Condition Tests
    // ========================================================================

    #[test]
    fn test_node_condition_new() {
        let condition = NodeCondition::new("cuda-ready", ConditionStatus::True);
        assert_eq!(condition.condition_type, "cuda-ready");
        assert!(condition.status.is_true());
        assert!(condition.is_satisfied());
    }

    #[test]
    fn test_node_condition_with_details() {
        let condition = NodeCondition::new("model-cached", ConditionStatus::True)
            .with_reason("CacheHit")
            .with_message("Model available at /models/llama");

        assert_eq!(condition.reason.as_deref(), Some("CacheHit"));
        assert_eq!(
            condition.message.as_deref(),
            Some("Model available at /models/llama")
        );
    }

    #[test]
    fn test_node_condition_update_status() {
        let mut condition = NodeCondition::new("test", ConditionStatus::False);
        let original_time = condition.last_transition_time;

        // Update to same status - no transition
        condition.update_status(ConditionStatus::False);
        assert_eq!(condition.last_transition_time, original_time);

        // Update to different status - transition
        std::thread::sleep(std::time::Duration::from_millis(10));
        condition.update_status(ConditionStatus::True);
        assert!(condition.last_transition_time > original_time);
    }

    #[test]
    fn test_condition_status_is_true() {
        assert!(ConditionStatus::True.is_true());
        assert!(!ConditionStatus::False.is_true());
        assert!(!ConditionStatus::Unknown.is_true());
    }

    // ========================================================================
    // Condition Requirement Tests
    // ========================================================================

    #[test]
    fn test_condition_requirement_must_be_true() {
        let req = ConditionRequirement::must_be_true("gpu-ready");
        assert_eq!(req.required_status, ConditionStatus::True);
    }

    #[test]
    fn test_condition_requirement_is_satisfied_by() {
        let req = ConditionRequirement::must_be_true("cuda-ready");
        
        let satisfied = NodeCondition::new("cuda-ready", ConditionStatus::True);
        let not_satisfied = NodeCondition::new("cuda-ready", ConditionStatus::False);
        let wrong_type = NodeCondition::new("other", ConditionStatus::True);

        assert!(req.is_satisfied_by(&satisfied));
        assert!(!req.is_satisfied_by(&not_satisfied));
        assert!(!req.is_satisfied_by(&wrong_type));
    }

    // ========================================================================
    // Scheduling Requirements Tests
    // ========================================================================

    #[test]
    fn test_scheduling_requirements_new() {
        let reqs = SchedulingRequirements::new();
        assert!(!reqs.is_gated());
        assert!(!reqs.is_parallel());
        assert!(!reqs.is_indexed());
    }

    #[test]
    fn test_scheduling_requirements_with_gate() {
        let reqs = SchedulingRequirements::new()
            .with_gate(SchedulingGate::new("model-loaded"));
        assert!(reqs.is_gated());
    }

    #[test]
    fn test_scheduling_requirements_with_parallel() {
        let reqs = SchedulingRequirements::new()
            .with_parallel(ParallelConfig::indexed(4));
        
        assert!(reqs.is_parallel());
        assert!(reqs.is_indexed());
    }

    #[test]
    fn test_scheduling_requirements_combined() {
        let reqs = SchedulingRequirements::new()
            .with_gate(SchedulingGate::new("dependencies-ready"))
            .with_gpu_requirement(GpuRequirement::new(2).with_model_pattern("A100"))
            .with_condition(ConditionRequirement::must_be_true("cuda-12"))
            .with_parallel(ParallelConfig::indexed(8))
            .with_node_selector("gpu-type", "nvidia");

        assert!(reqs.is_gated());
        assert!(reqs.is_parallel());
        assert!(reqs.is_indexed());
        assert!(reqs.gpu_requirement.is_some());
        assert_eq!(reqs.required_conditions.len(), 1);
        assert_eq!(reqs.node_selector.get("gpu-type").map(String::as_str), Some("nvidia"));
    }

    // ========================================================================
    // Serialization Tests
    // ========================================================================

    #[test]
    fn test_scheduling_gate_serialization() {
        let gate = SchedulingGate::new("test")
            .with_reason("Testing");

        let json = serde_json::to_string(&gate).unwrap();
        let deserialized: SchedulingGate = serde_json::from_str(&json).unwrap();
        assert_eq!(gate, deserialized);
    }

    #[test]
    fn test_gpu_requirement_serialization() {
        let req = GpuRequirement::new(2)
            .with_selector("device.memory_mib >= 40000")
            .with_fallback(GpuRequirement::new(4));

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: GpuRequirement = serde_json::from_str(&json).unwrap();
        assert_eq!(req, deserialized);
    }

    #[test]
    fn test_parallel_config_serialization() {
        let config = ParallelConfig::indexed(4);
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ParallelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_node_condition_serialization() {
        let condition = NodeCondition::new("test", ConditionStatus::True)
            .with_reason("TestReason");

        let json = serde_json::to_string(&condition).unwrap();
        let deserialized: NodeCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(condition, deserialized);
    }

    #[test]
    fn test_scheduling_requirements_serialization() {
        let reqs = SchedulingRequirements::new()
            .with_gate(SchedulingGate::new("test"))
            .with_gpu_requirement(GpuRequirement::new(1))
            .with_parallel(ParallelConfig::indexed(2));

        let json = serde_json::to_string(&reqs).unwrap();
        let deserialized: SchedulingRequirements = serde_json::from_str(&json).unwrap();
        assert_eq!(reqs, deserialized);
    }
}
