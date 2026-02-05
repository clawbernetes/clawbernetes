//! Core types for the preemption system.
//!
//! This module provides the fundamental types used throughout claw-preemption:
//! - [`PriorityClass`]: Priority classification for workloads
//! - [`PreemptionPolicy`]: Policy controlling preemption behavior
//! - [`PreemptionCandidate`]: A workload that may be preempted
//! - [`ResourceRequirements`]: Resource needs for workloads
//! - [`EvictionResult`]: Result of an eviction operation

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{PreemptionError, Result};

/// Unique identifier for a workload.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkloadId(String);

impl WorkloadId {
    /// Creates a new workload ID.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generates a new random workload ID.
    #[must_use]
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Returns the ID as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for WorkloadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Policy controlling when a workload can be preempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PreemptionPolicy {
    /// Never preempt workloads with this policy (system-critical workloads).
    Never,
    /// Preempt workloads with lower priority only.
    #[default]
    PreemptLowerPriority,
}

impl PreemptionPolicy {
    /// Returns true if this policy allows preemption of lower priority workloads.
    #[must_use]
    pub const fn allows_preemption(&self) -> bool {
        matches!(self, Self::PreemptLowerPriority)
    }

    /// Returns true if this policy prevents the workload from being preempted.
    #[must_use]
    pub const fn is_never_preempt(&self) -> bool {
        matches!(self, Self::Never)
    }
}

/// Priority class for workloads.
///
/// Priority classes define the relative importance of workloads and how they
/// interact during preemption scenarios. Higher values indicate higher priority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorityClass {
    /// Unique name for this priority class.
    pub name: String,
    /// Priority value (0-1000). Higher is more important.
    pub value: u16,
    /// Policy controlling preemption behavior.
    pub preemption_policy: PreemptionPolicy,
    /// Human-readable description.
    pub description: String,
    /// Whether this is a built-in system priority class.
    pub is_system: bool,
}

impl PriorityClass {
    /// Creates a new priority class.
    ///
    /// # Errors
    ///
    /// Returns error if the value exceeds 1000.
    pub fn new(
        name: impl Into<String>,
        value: u16,
        preemption_policy: PreemptionPolicy,
    ) -> Result<Self> {
        let name = name.into();
        if value > 1000 {
            return Err(PreemptionError::InvalidPriorityClass {
                reason: format!("value {value} exceeds maximum of 1000"),
            });
        }

        Ok(Self {
            name,
            value,
            preemption_policy,
            description: String::new(),
            is_system: false,
        })
    }

    /// Sets the description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Marks this as a system priority class.
    #[must_use]
    pub const fn as_system(mut self) -> Self {
        self.is_system = true;
        self
    }

    /// Returns the system-critical priority class (value 1000, never preempt).
    #[must_use]
    pub fn system_critical() -> Self {
        Self {
            name: "system-critical".into(),
            value: 1000,
            preemption_policy: PreemptionPolicy::Never,
            description: "Critical system workloads that should never be preempted".into(),
            is_system: true,
        }
    }

    /// Returns the high-priority class (value 750, preempt lower).
    #[must_use]
    pub fn high_priority() -> Self {
        Self {
            name: "high-priority".into(),
            value: 750,
            preemption_policy: PreemptionPolicy::PreemptLowerPriority,
            description: "High priority workloads that can preempt lower priority".into(),
            is_system: true,
        }
    }

    /// Returns the default priority class (value 500, preempt lower).
    #[must_use]
    pub fn default_priority() -> Self {
        Self {
            name: "default".into(),
            value: 500,
            preemption_policy: PreemptionPolicy::PreemptLowerPriority,
            description: "Default priority for standard workloads".into(),
            is_system: true,
        }
    }

    /// Returns the spot priority class (value 100, preemptible).
    #[must_use]
    pub fn spot() -> Self {
        Self {
            name: "spot".into(),
            value: 100,
            preemption_policy: PreemptionPolicy::PreemptLowerPriority,
            description: "Low-cost spot workloads that can be preempted".into(),
            is_system: true,
        }
    }

    /// Returns the preemptible priority class (value 0, always preemptible).
    #[must_use]
    pub fn preemptible() -> Self {
        Self {
            name: "preemptible".into(),
            value: 0,
            preemption_policy: PreemptionPolicy::PreemptLowerPriority,
            description: "Lowest priority workloads that are always preemptible".into(),
            is_system: true,
        }
    }

    /// Returns all built-in priority classes.
    #[must_use]
    pub fn built_in_classes() -> Vec<Self> {
        vec![
            Self::system_critical(),
            Self::high_priority(),
            Self::default_priority(),
            Self::spot(),
            Self::preemptible(),
        ]
    }

    /// Checks if this priority class can preempt another.
    ///
    /// Returns true if:
    /// - This class has higher priority value
    /// - This class allows preemption (not `Never`)
    /// - The victim class is not `Never` preempt
    #[must_use]
    pub fn can_preempt(&self, victim: &Self) -> bool {
        // Cannot preempt if victim has Never policy
        if victim.preemption_policy.is_never_preempt() {
            return false;
        }

        // Must have PreemptLowerPriority policy to preempt
        if !self.preemption_policy.allows_preemption() {
            return false;
        }

        // Must have strictly higher priority
        self.value > victim.value
    }
}

impl Default for PriorityClass {
    fn default() -> Self {
        Self::default_priority()
    }
}

/// Resource requirements for a workload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResourceRequirements {
    /// Number of GPUs required.
    pub gpus: u32,
    /// Memory required in bytes.
    pub memory_bytes: u64,
    /// CPU cores required (in millicores, 1000 = 1 core).
    pub cpu_millicores: u32,
    /// GPU memory required in bytes (optional, per GPU).
    pub gpu_memory_bytes: Option<u64>,
    /// Custom resource requirements.
    pub custom: HashMap<String, u64>,
}

impl ResourceRequirements {
    /// Creates new empty resource requirements.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the GPU count.
    #[must_use]
    pub const fn with_gpus(mut self, count: u32) -> Self {
        self.gpus = count;
        self
    }

    /// Sets the memory requirement in bytes.
    #[must_use]
    pub const fn with_memory(mut self, bytes: u64) -> Self {
        self.memory_bytes = bytes;
        self
    }

    /// Sets the memory requirement in gigabytes.
    #[must_use]
    pub const fn with_memory_gb(mut self, gb: u64) -> Self {
        self.memory_bytes = gb * 1024 * 1024 * 1024;
        self
    }

    /// Sets the CPU requirement in millicores.
    #[must_use]
    pub const fn with_cpu(mut self, millicores: u32) -> Self {
        self.cpu_millicores = millicores;
        self
    }

    /// Sets the CPU requirement in cores.
    #[must_use]
    pub const fn with_cpu_cores(mut self, cores: u32) -> Self {
        self.cpu_millicores = cores * 1000;
        self
    }

    /// Sets the GPU memory requirement in bytes.
    #[must_use]
    pub const fn with_gpu_memory(mut self, bytes: u64) -> Self {
        self.gpu_memory_bytes = Some(bytes);
        self
    }

    /// Sets the GPU memory requirement in gigabytes.
    #[must_use]
    pub const fn with_gpu_memory_gb(mut self, gb: u64) -> Self {
        self.gpu_memory_bytes = Some(gb * 1024 * 1024 * 1024);
        self
    }

    /// Adds a custom resource requirement.
    #[must_use]
    pub fn with_custom(mut self, name: impl Into<String>, value: u64) -> Self {
        self.custom.insert(name.into(), value);
        self
    }

    /// Checks if these requirements are satisfied by the available resources.
    #[must_use]
    pub fn is_satisfied_by(&self, available: &Self) -> bool {
        if self.gpus > available.gpus {
            return false;
        }
        if self.memory_bytes > available.memory_bytes {
            return false;
        }
        if self.cpu_millicores > available.cpu_millicores {
            return false;
        }
        if let (Some(needed), Some(avail)) = (self.gpu_memory_bytes, available.gpu_memory_bytes) {
            if needed > avail {
                return false;
            }
        }
        for (name, needed) in &self.custom {
            let avail = available.custom.get(name).copied().unwrap_or(0);
            if *needed > avail {
                return false;
            }
        }
        true
    }

    /// Returns the resources remaining after subtracting these requirements.
    #[must_use]
    pub fn subtract_from(&self, available: &Self) -> Self {
        let mut result = available.clone();
        result.gpus = result.gpus.saturating_sub(self.gpus);
        result.memory_bytes = result.memory_bytes.saturating_sub(self.memory_bytes);
        result.cpu_millicores = result.cpu_millicores.saturating_sub(self.cpu_millicores);
        if let (Some(needed), Some(avail)) = (self.gpu_memory_bytes, available.gpu_memory_bytes) {
            result.gpu_memory_bytes = Some(avail.saturating_sub(needed));
        }
        for (name, needed) in &self.custom {
            let avail = result.custom.get(name).copied().unwrap_or(0);
            result.custom.insert(name.clone(), avail.saturating_sub(*needed));
        }
        result
    }

    /// Returns the sum of these resources and another.
    #[must_use]
    pub fn add(&self, other: &Self) -> Self {
        let mut result = self.clone();
        result.gpus = result.gpus.saturating_add(other.gpus);
        result.memory_bytes = result.memory_bytes.saturating_add(other.memory_bytes);
        result.cpu_millicores = result.cpu_millicores.saturating_add(other.cpu_millicores);
        if let Some(other_gpu_mem) = other.gpu_memory_bytes {
            result.gpu_memory_bytes = Some(
                result.gpu_memory_bytes.unwrap_or(0).saturating_add(other_gpu_mem),
            );
        }
        for (name, value) in &other.custom {
            let existing = result.custom.get(name).copied().unwrap_or(0);
            result.custom.insert(name.clone(), existing.saturating_add(*value));
        }
        result
    }

    /// Returns true if all resources are zero.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.gpus == 0
            && self.memory_bytes == 0
            && self.cpu_millicores == 0
            && self.gpu_memory_bytes.is_none_or(|v| v == 0)
            && self.custom.values().all(|v| *v == 0)
    }
}

/// State of a workload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WorkloadState {
    /// Workload is waiting to be scheduled.
    #[default]
    Pending,
    /// Workload is running.
    Running,
    /// Workload is being evicted (graceful shutdown in progress).
    Evicting,
    /// Workload has been evicted.
    Evicted,
    /// Workload completed successfully.
    Completed,
    /// Workload failed.
    Failed,
}

impl WorkloadState {
    /// Returns true if the workload is running and can be preempted.
    #[must_use]
    pub const fn is_preemptible(&self) -> bool {
        matches!(self, Self::Running)
    }

    /// Returns true if the workload is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Evicted | Self::Completed | Self::Failed)
    }
}

/// A candidate workload for preemption.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreemptionCandidate {
    /// Workload identifier.
    pub workload_id: WorkloadId,
    /// Priority class of this workload.
    pub priority_class: PriorityClass,
    /// Current state of the workload.
    pub state: WorkloadState,
    /// When this workload started running.
    pub started_at: Option<DateTime<Utc>>,
    /// Resource requirements of this workload.
    pub resources: ResourceRequirements,
    /// Estimated cost if preempted (lost work, restart cost, etc.).
    pub preemption_cost: f64,
    /// Node or location where this workload is running.
    pub node_id: Option<String>,
    /// Custom labels for selection.
    pub labels: HashMap<String, String>,
    /// Grace period for this workload during eviction.
    pub grace_period: Duration,
}

impl PreemptionCandidate {
    /// Creates a new preemption candidate.
    #[must_use]
    pub fn new(workload_id: WorkloadId, priority_class: PriorityClass) -> Self {
        Self {
            workload_id,
            priority_class,
            state: WorkloadState::Running,
            started_at: Some(Utc::now()),
            resources: ResourceRequirements::new(),
            preemption_cost: 0.0,
            node_id: None,
            labels: HashMap::new(),
            grace_period: Duration::from_secs(30),
        }
    }

    /// Sets the workload state.
    #[must_use]
    pub const fn with_state(mut self, state: WorkloadState) -> Self {
        self.state = state;
        self
    }

    /// Sets the start time.
    #[must_use]
    pub const fn with_started_at(mut self, started_at: DateTime<Utc>) -> Self {
        self.started_at = Some(started_at);
        self
    }

    /// Sets the resource requirements.
    #[must_use]
    pub fn with_resources(mut self, resources: ResourceRequirements) -> Self {
        self.resources = resources;
        self
    }

    /// Sets the preemption cost.
    #[must_use]
    pub fn with_preemption_cost(mut self, cost: f64) -> Self {
        self.preemption_cost = cost;
        self
    }

    /// Sets the node ID.
    #[must_use]
    pub fn with_node(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }

    /// Adds a label.
    #[must_use]
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Sets the grace period.
    #[must_use]
    pub const fn with_grace_period(mut self, grace_period: Duration) -> Self {
        self.grace_period = grace_period;
        self
    }

    /// Returns the running duration if the workload is running.
    #[must_use]
    pub fn running_duration(&self) -> Option<Duration> {
        self.started_at.map(|start| {
            let now = Utc::now();
            let diff = now.signed_duration_since(start);
            diff.to_std().unwrap_or(Duration::ZERO)
        })
    }

    /// Returns the priority value for sorting.
    #[must_use]
    pub fn priority_value(&self) -> u16 {
        self.priority_class.value
    }

    /// Checks if this workload can be preempted.
    #[must_use]
    pub fn can_be_preempted(&self) -> bool {
        // Only running workloads can be preempted
        if !self.state.is_preemptible() {
            return false;
        }

        // Cannot preempt if priority class has Never policy
        !self.priority_class.preemption_policy.is_never_preempt()
    }
}

/// Strategy for selecting preemption victims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VictimSelectionStrategy {
    /// Select victims with lowest priority first.
    #[default]
    LowestPriority,
    /// Select victims with shortest running time first (least work lost).
    ShortestRunning,
    /// Select victims with lowest preemption cost first.
    LowestCost,
    /// Select victims that free the most resources first.
    MostResources,
    /// Select using a combination of factors (priority, cost, running time).
    Balanced,
}

/// Result of an eviction operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvictionResult {
    /// Workloads that were evicted.
    pub evicted_workloads: Vec<WorkloadId>,
    /// Total resources freed by eviction.
    pub freed_resources: ResourceRequirements,
    /// Total preemption cost incurred.
    pub total_cost: f64,
    /// When the eviction was initiated.
    pub initiated_at: DateTime<Utc>,
    /// When the eviction completed (all workloads stopped).
    pub completed_at: Option<DateTime<Utc>>,
    /// Any failures during eviction.
    pub failures: Vec<EvictionFailure>,
}

impl EvictionResult {
    /// Creates a new eviction result.
    #[must_use]
    pub fn new() -> Self {
        Self {
            evicted_workloads: Vec::new(),
            freed_resources: ResourceRequirements::new(),
            total_cost: 0.0,
            initiated_at: Utc::now(),
            completed_at: None,
            failures: Vec::new(),
        }
    }

    /// Adds an evicted workload.
    pub fn add_evicted(&mut self, workload_id: WorkloadId, resources: &ResourceRequirements, cost: f64) {
        self.evicted_workloads.push(workload_id);
        self.freed_resources = self.freed_resources.add(resources);
        self.total_cost += cost;
    }

    /// Adds a failure.
    pub fn add_failure(&mut self, failure: EvictionFailure) {
        self.failures.push(failure);
    }

    /// Marks the eviction as completed.
    pub fn complete(&mut self) {
        self.completed_at = Some(Utc::now());
    }

    /// Returns true if all evictions were successful.
    #[must_use]
    pub fn is_successful(&self) -> bool {
        self.failures.is_empty()
    }

    /// Returns the number of evicted workloads.
    #[must_use]
    pub fn evicted_count(&self) -> usize {
        self.evicted_workloads.len()
    }
}

impl Default for EvictionResult {
    fn default() -> Self {
        Self::new()
    }
}

/// A failure during eviction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvictionFailure {
    /// Workload that failed to evict.
    pub workload_id: WorkloadId,
    /// Reason for the failure.
    pub reason: String,
    /// When the failure occurred.
    pub failed_at: DateTime<Utc>,
}

impl EvictionFailure {
    /// Creates a new eviction failure.
    #[must_use]
    pub fn new(workload_id: WorkloadId, reason: impl Into<String>) -> Self {
        Self {
            workload_id,
            reason: reason.into(),
            failed_at: Utc::now(),
        }
    }
}

/// Configuration for preemption behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreemptionConfig {
    /// Default grace period for evictions.
    pub default_grace_period: Duration,
    /// Maximum grace period allowed.
    pub max_grace_period: Duration,
    /// Victim selection strategy.
    pub victim_selection: VictimSelectionStrategy,
    /// Whether to allow preempting workloads from the same priority class.
    pub allow_same_priority_preemption: bool,
    /// Minimum priority difference required for preemption.
    pub min_priority_difference: u16,
    /// Maximum number of victims in a single preemption operation.
    pub max_victims_per_operation: usize,
    /// Whether preemption is globally enabled.
    pub enabled: bool,
}

impl Default for PreemptionConfig {
    fn default() -> Self {
        Self {
            default_grace_period: Duration::from_secs(30),
            max_grace_period: Duration::from_secs(300),
            victim_selection: VictimSelectionStrategy::default(),
            allow_same_priority_preemption: false,
            min_priority_difference: 1,
            max_victims_per_operation: 100,
            enabled: true,
        }
    }
}

impl PreemptionConfig {
    /// Creates a new preemption config with defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the default grace period.
    #[must_use]
    pub const fn with_default_grace_period(mut self, duration: Duration) -> Self {
        self.default_grace_period = duration;
        self
    }

    /// Sets the maximum grace period.
    #[must_use]
    pub const fn with_max_grace_period(mut self, duration: Duration) -> Self {
        self.max_grace_period = duration;
        self
    }

    /// Sets the victim selection strategy.
    #[must_use]
    pub const fn with_victim_selection(mut self, strategy: VictimSelectionStrategy) -> Self {
        self.victim_selection = strategy;
        self
    }

    /// Sets whether same priority preemption is allowed.
    #[must_use]
    pub const fn with_same_priority_preemption(mut self, allow: bool) -> Self {
        self.allow_same_priority_preemption = allow;
        self
    }

    /// Sets the minimum priority difference.
    #[must_use]
    pub const fn with_min_priority_difference(mut self, diff: u16) -> Self {
        self.min_priority_difference = diff;
        self
    }

    /// Sets the maximum victims per operation.
    #[must_use]
    pub const fn with_max_victims(mut self, max: usize) -> Self {
        self.max_victims_per_operation = max;
        self
    }

    /// Enables or disables preemption.
    #[must_use]
    pub const fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod workload_id_tests {
        use super::*;

        #[test]
        fn workload_id_creation() {
            let id = WorkloadId::new("job-123");
            assert_eq!(id.as_str(), "job-123");
            assert_eq!(format!("{id}"), "job-123");
        }

        #[test]
        fn workload_id_generate() {
            let id1 = WorkloadId::generate();
            let id2 = WorkloadId::generate();
            assert_ne!(id1, id2);
        }

        #[test]
        fn workload_id_equality() {
            let id1 = WorkloadId::new("job-a");
            let id2 = WorkloadId::new("job-a");
            let id3 = WorkloadId::new("job-b");

            assert_eq!(id1, id2);
            assert_ne!(id1, id3);
        }

        #[test]
        fn workload_id_serialization() {
            let id = WorkloadId::new("test-workload");
            let json = serde_json::to_string(&id);
            assert!(json.is_ok());
            let parsed: serde_json::Result<WorkloadId> = serde_json::from_str(&json.ok().unwrap_or_default());
            assert!(parsed.is_ok());
            assert_eq!(parsed.ok().unwrap_or_else(|| WorkloadId::new("")), id);
        }
    }

    mod preemption_policy_tests {
        use super::*;

        #[test]
        fn preemption_policy_default() {
            let policy = PreemptionPolicy::default();
            assert_eq!(policy, PreemptionPolicy::PreemptLowerPriority);
        }

        #[test]
        fn preemption_policy_allows_preemption() {
            assert!(PreemptionPolicy::PreemptLowerPriority.allows_preemption());
            assert!(!PreemptionPolicy::Never.allows_preemption());
        }

        #[test]
        fn preemption_policy_is_never_preempt() {
            assert!(PreemptionPolicy::Never.is_never_preempt());
            assert!(!PreemptionPolicy::PreemptLowerPriority.is_never_preempt());
        }

        #[test]
        fn preemption_policy_serialization() {
            for policy in [PreemptionPolicy::Never, PreemptionPolicy::PreemptLowerPriority] {
                let json = serde_json::to_string(&policy);
                assert!(json.is_ok());
            }
        }
    }

    mod priority_class_tests {
        use super::*;

        #[test]
        fn priority_class_creation() {
            let pc = PriorityClass::new("test", 500, PreemptionPolicy::PreemptLowerPriority);
            assert!(pc.is_ok());
            let pc = pc.ok().unwrap_or_else(PriorityClass::default);
            assert_eq!(pc.name, "test");
            assert_eq!(pc.value, 500);
            assert!(!pc.is_system);
        }

        #[test]
        fn priority_class_value_too_high() {
            let pc = PriorityClass::new("test", 1001, PreemptionPolicy::Never);
            assert!(pc.is_err());
        }

        #[test]
        fn priority_class_with_description() {
            let pc = PriorityClass::new("test", 500, PreemptionPolicy::PreemptLowerPriority)
                .ok()
                .unwrap_or_else(PriorityClass::default)
                .with_description("Test priority class");
            assert_eq!(pc.description, "Test priority class");
        }

        #[test]
        fn priority_class_as_system() {
            let pc = PriorityClass::new("test", 500, PreemptionPolicy::PreemptLowerPriority)
                .ok()
                .unwrap_or_else(PriorityClass::default)
                .as_system();
            assert!(pc.is_system);
        }

        #[test]
        fn priority_class_built_in() {
            let critical = PriorityClass::system_critical();
            assert_eq!(critical.name, "system-critical");
            assert_eq!(critical.value, 1000);
            assert!(critical.preemption_policy.is_never_preempt());
            assert!(critical.is_system);

            let high = PriorityClass::high_priority();
            assert_eq!(high.name, "high-priority");
            assert_eq!(high.value, 750);

            let default = PriorityClass::default_priority();
            assert_eq!(default.name, "default");
            assert_eq!(default.value, 500);

            let spot = PriorityClass::spot();
            assert_eq!(spot.name, "spot");
            assert_eq!(spot.value, 100);

            let preemptible = PriorityClass::preemptible();
            assert_eq!(preemptible.name, "preemptible");
            assert_eq!(preemptible.value, 0);
        }

        #[test]
        fn priority_class_built_in_list() {
            let classes = PriorityClass::built_in_classes();
            assert_eq!(classes.len(), 5);
            assert!(classes.iter().all(|c| c.is_system));
        }

        #[test]
        fn priority_class_can_preempt() {
            let high = PriorityClass::high_priority();
            let default = PriorityClass::default_priority();
            let critical = PriorityClass::system_critical();
            let preemptible = PriorityClass::preemptible();

            // High can preempt default
            assert!(high.can_preempt(&default));

            // Default can preempt preemptible
            assert!(default.can_preempt(&preemptible));

            // Cannot preempt system-critical
            assert!(!high.can_preempt(&critical));

            // Cannot preempt same or higher
            assert!(!default.can_preempt(&high));
            assert!(!default.can_preempt(&default));

            // System-critical cannot preempt anything (Never policy)
            assert!(!critical.can_preempt(&preemptible));
        }

        #[test]
        fn priority_class_default() {
            let pc = PriorityClass::default();
            assert_eq!(pc.name, "default");
            assert_eq!(pc.value, 500);
        }
    }

    mod resource_requirements_tests {
        use super::*;

        #[test]
        fn resource_requirements_creation() {
            let res = ResourceRequirements::new();
            assert_eq!(res.gpus, 0);
            assert_eq!(res.memory_bytes, 0);
            assert_eq!(res.cpu_millicores, 0);
            assert!(res.is_empty());
        }

        #[test]
        fn resource_requirements_builder() {
            let res = ResourceRequirements::new()
                .with_gpus(4)
                .with_memory_gb(32)
                .with_cpu_cores(8)
                .with_gpu_memory_gb(80);

            assert_eq!(res.gpus, 4);
            assert_eq!(res.memory_bytes, 32 * 1024 * 1024 * 1024);
            assert_eq!(res.cpu_millicores, 8000);
            assert_eq!(res.gpu_memory_bytes, Some(80 * 1024 * 1024 * 1024));
            assert!(!res.is_empty());
        }

        #[test]
        fn resource_requirements_is_satisfied_by() {
            let needed = ResourceRequirements::new()
                .with_gpus(2)
                .with_memory_gb(16);

            let available = ResourceRequirements::new()
                .with_gpus(4)
                .with_memory_gb(32);

            let insufficient = ResourceRequirements::new()
                .with_gpus(1)
                .with_memory_gb(8);

            assert!(needed.is_satisfied_by(&available));
            assert!(!needed.is_satisfied_by(&insufficient));
        }

        #[test]
        fn resource_requirements_subtract() {
            let available = ResourceRequirements::new()
                .with_gpus(8)
                .with_memory_gb(64)
                .with_cpu_cores(16);

            let used = ResourceRequirements::new()
                .with_gpus(2)
                .with_memory_gb(16)
                .with_cpu_cores(4);

            let remaining = used.subtract_from(&available);

            assert_eq!(remaining.gpus, 6);
            assert_eq!(remaining.memory_bytes, 48 * 1024 * 1024 * 1024);
            assert_eq!(remaining.cpu_millicores, 12000);
        }

        #[test]
        fn resource_requirements_add() {
            let a = ResourceRequirements::new()
                .with_gpus(2)
                .with_memory_gb(16);

            let b = ResourceRequirements::new()
                .with_gpus(3)
                .with_memory_gb(24);

            let sum = a.add(&b);

            assert_eq!(sum.gpus, 5);
            assert_eq!(sum.memory_bytes, 40 * 1024 * 1024 * 1024);
        }

        #[test]
        fn resource_requirements_custom() {
            let res = ResourceRequirements::new()
                .with_custom("network_bandwidth_mbps", 1000)
                .with_custom("storage_iops", 10000);

            assert_eq!(res.custom.get("network_bandwidth_mbps"), Some(&1000));
            assert_eq!(res.custom.get("storage_iops"), Some(&10000));
        }

        #[test]
        fn resource_requirements_subtract_saturating() {
            let available = ResourceRequirements::new()
                .with_gpus(2);

            let needed = ResourceRequirements::new()
                .with_gpus(5);

            let result = needed.subtract_from(&available);
            assert_eq!(result.gpus, 0); // Saturates at 0, doesn't go negative
        }
    }

    mod workload_state_tests {
        use super::*;

        #[test]
        fn workload_state_default() {
            let state = WorkloadState::default();
            assert_eq!(state, WorkloadState::Pending);
        }

        #[test]
        fn workload_state_is_preemptible() {
            assert!(WorkloadState::Running.is_preemptible());
            assert!(!WorkloadState::Pending.is_preemptible());
            assert!(!WorkloadState::Evicting.is_preemptible());
            assert!(!WorkloadState::Evicted.is_preemptible());
            assert!(!WorkloadState::Completed.is_preemptible());
            assert!(!WorkloadState::Failed.is_preemptible());
        }

        #[test]
        fn workload_state_is_terminal() {
            assert!(!WorkloadState::Running.is_terminal());
            assert!(!WorkloadState::Pending.is_terminal());
            assert!(!WorkloadState::Evicting.is_terminal());
            assert!(WorkloadState::Evicted.is_terminal());
            assert!(WorkloadState::Completed.is_terminal());
            assert!(WorkloadState::Failed.is_terminal());
        }
    }

    mod preemption_candidate_tests {
        use super::*;

        #[test]
        fn preemption_candidate_creation() {
            let candidate = PreemptionCandidate::new(
                WorkloadId::new("job-1"),
                PriorityClass::default_priority(),
            );

            assert_eq!(candidate.workload_id.as_str(), "job-1");
            assert_eq!(candidate.priority_class.name, "default");
            assert_eq!(candidate.state, WorkloadState::Running);
            assert!(candidate.started_at.is_some());
        }

        #[test]
        fn preemption_candidate_builder() {
            let resources = ResourceRequirements::new()
                .with_gpus(4)
                .with_memory_gb(32);

            let candidate = PreemptionCandidate::new(
                WorkloadId::new("job-2"),
                PriorityClass::spot(),
            )
            .with_resources(resources)
            .with_preemption_cost(100.0)
            .with_node("node-1")
            .with_label("team", "ml")
            .with_grace_period(Duration::from_secs(60));

            assert_eq!(candidate.resources.gpus, 4);
            assert!((candidate.preemption_cost - 100.0).abs() < f64::EPSILON);
            assert_eq!(candidate.node_id, Some("node-1".into()));
            assert_eq!(candidate.labels.get("team"), Some(&"ml".into()));
            assert_eq!(candidate.grace_period, Duration::from_secs(60));
        }

        #[test]
        fn preemption_candidate_can_be_preempted() {
            // Running with preempt-lower policy - can be preempted
            let running = PreemptionCandidate::new(
                WorkloadId::new("job-1"),
                PriorityClass::default_priority(),
            );
            assert!(running.can_be_preempted());

            // Pending - cannot be preempted
            let pending = PreemptionCandidate::new(
                WorkloadId::new("job-2"),
                PriorityClass::default_priority(),
            )
            .with_state(WorkloadState::Pending);
            assert!(!pending.can_be_preempted());

            // System-critical - cannot be preempted
            let critical = PreemptionCandidate::new(
                WorkloadId::new("job-3"),
                PriorityClass::system_critical(),
            );
            assert!(!critical.can_be_preempted());
        }

        #[test]
        fn preemption_candidate_priority_value() {
            let candidate = PreemptionCandidate::new(
                WorkloadId::new("job-1"),
                PriorityClass::high_priority(),
            );
            assert_eq!(candidate.priority_value(), 750);
        }

        #[test]
        fn preemption_candidate_running_duration() {
            let past = Utc::now() - chrono::Duration::seconds(60);
            let candidate = PreemptionCandidate::new(
                WorkloadId::new("job-1"),
                PriorityClass::default_priority(),
            )
            .with_started_at(past);

            let duration = candidate.running_duration();
            assert!(duration.is_some());
            assert!(duration.unwrap_or_default() >= Duration::from_secs(59));
        }
    }

    mod victim_selection_strategy_tests {
        use super::*;

        #[test]
        fn victim_selection_strategy_default() {
            let strategy = VictimSelectionStrategy::default();
            assert_eq!(strategy, VictimSelectionStrategy::LowestPriority);
        }

        #[test]
        fn victim_selection_strategy_serialization() {
            for strategy in [
                VictimSelectionStrategy::LowestPriority,
                VictimSelectionStrategy::ShortestRunning,
                VictimSelectionStrategy::LowestCost,
                VictimSelectionStrategy::MostResources,
                VictimSelectionStrategy::Balanced,
            ] {
                let json = serde_json::to_string(&strategy);
                assert!(json.is_ok());
            }
        }
    }

    mod eviction_result_tests {
        use super::*;

        #[test]
        fn eviction_result_creation() {
            let result = EvictionResult::new();
            assert!(result.evicted_workloads.is_empty());
            assert!(result.freed_resources.is_empty());
            assert!((result.total_cost - 0.0).abs() < f64::EPSILON);
            assert!(result.completed_at.is_none());
            assert!(result.is_successful());
        }

        #[test]
        fn eviction_result_add_evicted() {
            let mut result = EvictionResult::new();

            let resources = ResourceRequirements::new()
                .with_gpus(2)
                .with_memory_gb(16);

            result.add_evicted(WorkloadId::new("job-1"), &resources, 50.0);
            result.add_evicted(WorkloadId::new("job-2"), &resources, 75.0);

            assert_eq!(result.evicted_count(), 2);
            assert_eq!(result.freed_resources.gpus, 4);
            assert!((result.total_cost - 125.0).abs() < f64::EPSILON);
        }

        #[test]
        fn eviction_result_add_failure() {
            let mut result = EvictionResult::new();
            result.add_failure(EvictionFailure::new(
                WorkloadId::new("job-1"),
                "container not responding",
            ));

            assert!(!result.is_successful());
            assert_eq!(result.failures.len(), 1);
        }

        #[test]
        fn eviction_result_complete() {
            let mut result = EvictionResult::new();
            assert!(result.completed_at.is_none());

            result.complete();
            assert!(result.completed_at.is_some());
        }
    }

    mod eviction_failure_tests {
        use super::*;

        #[test]
        fn eviction_failure_creation() {
            let failure = EvictionFailure::new(
                WorkloadId::new("job-1"),
                "timeout waiting for graceful shutdown",
            );

            assert_eq!(failure.workload_id.as_str(), "job-1");
            assert_eq!(failure.reason, "timeout waiting for graceful shutdown");
        }
    }

    mod preemption_config_tests {
        use super::*;

        #[test]
        fn preemption_config_default() {
            let config = PreemptionConfig::default();
            assert_eq!(config.default_grace_period, Duration::from_secs(30));
            assert_eq!(config.max_grace_period, Duration::from_secs(300));
            assert_eq!(config.victim_selection, VictimSelectionStrategy::LowestPriority);
            assert!(!config.allow_same_priority_preemption);
            assert_eq!(config.min_priority_difference, 1);
            assert_eq!(config.max_victims_per_operation, 100);
            assert!(config.enabled);
        }

        #[test]
        fn preemption_config_builder() {
            let config = PreemptionConfig::new()
                .with_default_grace_period(Duration::from_secs(60))
                .with_max_grace_period(Duration::from_secs(600))
                .with_victim_selection(VictimSelectionStrategy::Balanced)
                .with_same_priority_preemption(true)
                .with_min_priority_difference(10)
                .with_max_victims(50)
                .with_enabled(false);

            assert_eq!(config.default_grace_period, Duration::from_secs(60));
            assert_eq!(config.max_grace_period, Duration::from_secs(600));
            assert_eq!(config.victim_selection, VictimSelectionStrategy::Balanced);
            assert!(config.allow_same_priority_preemption);
            assert_eq!(config.min_priority_difference, 10);
            assert_eq!(config.max_victims_per_operation, 50);
            assert!(!config.enabled);
        }
    }
}
