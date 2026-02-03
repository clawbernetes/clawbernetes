//! Basic workload scheduling with GPU-aware placement.

use claw_proto::{NodeId, WorkloadSpec};
use thiserror::Error;

use crate::registry::{NodeRegistry, RegisteredNode};

/// Errors that can occur during scheduling.
#[derive(Debug, Error)]
pub enum SchedulerError {
    /// No suitable node found for the workload.
    #[error("no suitable node found: {0}")]
    NoSuitableNode(String),

    /// Registry is empty.
    #[error("no nodes registered")]
    NoNodes,
}

/// GPU-aware workload scheduler.
#[derive(Debug, Default)]
pub struct Scheduler {
    /// Minimum memory headroom (MiB) to leave on nodes.
    memory_headroom_mib: u64,
}

impl Scheduler {
    /// Create a new scheduler.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            memory_headroom_mib: 0,
        }
    }

    /// Create a scheduler with memory headroom.
    #[must_use]
    pub const fn with_memory_headroom(memory_headroom_mib: u64) -> Self {
        Self { memory_headroom_mib }
    }

    /// Schedule a workload to a node based on resource requirements.
    ///
    /// # Algorithm
    ///
    /// 1. Filter nodes that have enough GPUs (if required)
    /// 2. Filter nodes that have enough memory
    /// 3. Filter nodes that have enough CPU cores
    /// 4. Return the first suitable node (simple first-fit)
    ///
    /// # Errors
    ///
    /// Returns an error if no suitable node is found.
    pub fn schedule(
        &self,
        spec: &WorkloadSpec,
        registry: &NodeRegistry,
    ) -> Result<NodeId, SchedulerError> {
        if registry.is_empty() {
            return Err(SchedulerError::NoNodes);
        }

        let suitable_node = registry
            .list_nodes()
            .into_iter()
            .find(|node| self.node_fits_spec(node, spec));

        suitable_node.map_or_else(
            || {
                Err(SchedulerError::NoSuitableNode(format!(
                    "need {} GPUs, {} MB memory, {} CPU cores",
                    spec.gpu_count, spec.memory_mb, spec.cpu_cores
                )))
            },
            |node| Ok(node.id),
        )
    }

    /// Check if a node can accommodate the workload spec.
    fn node_fits_spec(&self, node: &RegisteredNode, spec: &WorkloadSpec) -> bool {
        // Check GPU requirements
        if spec.gpu_count > 0 && !Self::has_enough_gpus(node, spec.gpu_count) {
            return false;
        }

        // Check memory requirements (convert MB to MiB, roughly equivalent for our purposes)
        let required_memory_mib = spec.memory_mb + self.memory_headroom_mib;
        if node.capabilities.memory_mib < required_memory_mib {
            return false;
        }

        // Check CPU requirements
        if node.capabilities.cpu_cores < spec.cpu_cores {
            return false;
        }

        true
    }

    /// Check if a node has enough GPUs.
    fn has_enough_gpus(node: &RegisteredNode, required: u32) -> bool {
        node.gpu_count() >= required as usize
    }

    /// Find the best node for a workload (GPU-aware).
    ///
    /// Prefers nodes with:
    /// 1. Exact GPU count match (to avoid wasting resources)
    /// 2. Most available memory
    ///
    /// # Errors
    ///
    /// Returns an error if no suitable node is found.
    pub fn schedule_best_fit(
        &self,
        spec: &WorkloadSpec,
        registry: &NodeRegistry,
    ) -> Result<NodeId, SchedulerError> {
        if registry.is_empty() {
            return Err(SchedulerError::NoNodes);
        }

        let suitable_nodes: Vec<_> = registry
            .list_nodes()
            .into_iter()
            .filter(|node| self.node_fits_spec(node, spec))
            .collect();

        if suitable_nodes.is_empty() {
            return Err(SchedulerError::NoSuitableNode(format!(
                "need {} GPUs, {} MB memory, {} CPU cores",
                spec.gpu_count, spec.memory_mb, spec.cpu_cores
            )));
        }

        // Sort by best fit:
        // 1. Prefer exact GPU match
        // 2. Then by most memory (to spread load)
        let best_node = suitable_nodes
            .into_iter()
            .min_by(|a, b| {
                let a_gpu_diff = a.gpu_count().abs_diff(spec.gpu_count as usize);
                let b_gpu_diff = b.gpu_count().abs_diff(spec.gpu_count as usize);

                // First compare by GPU difference (prefer closer match)
                match a_gpu_diff.cmp(&b_gpu_diff) {
                    std::cmp::Ordering::Equal => {
                        // If equal, prefer node with more memory (descending)
                        b.capabilities.memory_mib.cmp(&a.capabilities.memory_mib)
                    }
                    other => other,
                }
            });

        best_node.map_or_else(
            || Err(SchedulerError::NoSuitableNode("no suitable node found".to_string())),
            |node| Ok(node.id),
        )
    }

    /// Find nodes by GPU type.
    #[must_use]
    pub fn find_nodes_by_gpu_type<'a>(
        &self,
        gpu_type: &str,
        registry: &'a NodeRegistry,
    ) -> Vec<&'a RegisteredNode> {
        registry.find_by_gpu(gpu_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_proto::{GpuCapability, NodeCapabilities};

    // ==================== Helper Functions ====================

    fn make_gpu(name: &str, memory_mib: u64) -> GpuCapability {
        GpuCapability {
            index: 0,
            name: name.to_string(),
            memory_mib,
            uuid: format!("gpu-uuid-{name}"),
        }
    }

    fn make_node_capabilities(cpu_cores: u32, memory_mib: u64, gpus: Vec<GpuCapability>) -> NodeCapabilities {
        let mut caps = NodeCapabilities::new(cpu_cores, memory_mib);
        for gpu in gpus {
            caps = caps.with_gpu(gpu);
        }
        caps
    }

    fn register_node(registry: &mut NodeRegistry, cpu: u32, memory: u64, gpus: Vec<GpuCapability>) -> NodeId {
        let node_id = NodeId::new();
        let caps = make_node_capabilities(cpu, memory, gpus);
        registry.register(node_id, caps).unwrap();
        node_id
    }

    // ==================== Scheduler Basic Tests ====================

    #[test]
    fn test_scheduler_new() {
        let scheduler = Scheduler::new();
        assert_eq!(scheduler.memory_headroom_mib, 0);
    }

    #[test]
    fn test_scheduler_with_memory_headroom() {
        let scheduler = Scheduler::with_memory_headroom(1024);
        assert_eq!(scheduler.memory_headroom_mib, 1024);
    }

    #[test]
    fn test_scheduler_default() {
        let scheduler = Scheduler::default();
        assert_eq!(scheduler.memory_headroom_mib, 0);
    }

    // ==================== Schedule Tests - Empty Registry ====================

    #[test]
    fn test_schedule_empty_registry_fails() {
        let scheduler = Scheduler::new();
        let registry = NodeRegistry::new();
        let spec = WorkloadSpec::new("nginx:latest");

        let result = scheduler.schedule(&spec, &registry);

        assert!(matches!(result, Err(SchedulerError::NoNodes)));
    }

    // ==================== Schedule Tests - Basic Resource Matching ====================

    #[test]
    fn test_schedule_simple_workload() {
        let scheduler = Scheduler::new();
        let mut registry = NodeRegistry::new();

        let node_id = register_node(&mut registry, 4, 8192, vec![]);
        let spec = WorkloadSpec::new("nginx:latest")
            .with_cpu_cores(2)
            .with_memory_mb(1024);

        let result = scheduler.schedule(&spec, &registry);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), node_id);
    }

    #[test]
    fn test_schedule_insufficient_cpu() {
        let scheduler = Scheduler::new();
        let mut registry = NodeRegistry::new();

        register_node(&mut registry, 2, 8192, vec![]); // Only 2 cores

        let spec = WorkloadSpec::new("nginx:latest")
            .with_cpu_cores(4); // Needs 4 cores

        let result = scheduler.schedule(&spec, &registry);

        assert!(matches!(result, Err(SchedulerError::NoSuitableNode(_))));
    }

    #[test]
    fn test_schedule_insufficient_memory() {
        let scheduler = Scheduler::new();
        let mut registry = NodeRegistry::new();

        register_node(&mut registry, 8, 2048, vec![]); // Only 2 GB

        let spec = WorkloadSpec::new("nginx:latest")
            .with_memory_mb(4096); // Needs 4 GB

        let result = scheduler.schedule(&spec, &registry);

        assert!(matches!(result, Err(SchedulerError::NoSuitableNode(_))));
    }

    // ==================== Schedule Tests - GPU Requirements ====================

    #[test]
    fn test_schedule_gpu_workload() {
        let scheduler = Scheduler::new();
        let mut registry = NodeRegistry::new();

        let gpu = make_gpu("RTX 4090", 24576);
        let node_id = register_node(&mut registry, 8, 32768, vec![gpu]);

        let spec = WorkloadSpec::new("pytorch:latest")
            .with_gpu_count(1)
            .with_memory_mb(8192);

        let result = scheduler.schedule(&spec, &registry);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), node_id);
    }

    #[test]
    fn test_schedule_multi_gpu_workload() {
        let scheduler = Scheduler::new();
        let mut registry = NodeRegistry::new();

        let gpus = vec![
            make_gpu("RTX 4090 #0", 24576),
            make_gpu("RTX 4090 #1", 24576),
            make_gpu("RTX 4090 #2", 24576),
            make_gpu("RTX 4090 #3", 24576),
        ];
        let node_id = register_node(&mut registry, 32, 131072, gpus);

        let spec = WorkloadSpec::new("pytorch:latest")
            .with_gpu_count(4)
            .with_memory_mb(32768);

        let result = scheduler.schedule(&spec, &registry);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), node_id);
    }

    #[test]
    fn test_schedule_insufficient_gpus() {
        let scheduler = Scheduler::new();
        let mut registry = NodeRegistry::new();

        let gpu = make_gpu("RTX 4090", 24576);
        register_node(&mut registry, 8, 32768, vec![gpu]); // Only 1 GPU

        let spec = WorkloadSpec::new("pytorch:latest")
            .with_gpu_count(2); // Needs 2 GPUs

        let result = scheduler.schedule(&spec, &registry);

        assert!(matches!(result, Err(SchedulerError::NoSuitableNode(_))));
    }

    #[test]
    fn test_schedule_no_gpu_when_required() {
        let scheduler = Scheduler::new();
        let mut registry = NodeRegistry::new();

        register_node(&mut registry, 8, 32768, vec![]); // No GPUs

        let spec = WorkloadSpec::new("pytorch:latest")
            .with_gpu_count(1);

        let result = scheduler.schedule(&spec, &registry);

        assert!(matches!(result, Err(SchedulerError::NoSuitableNode(_))));
    }

    // ==================== Schedule Tests - Multiple Nodes ====================

    #[test]
    fn test_schedule_selects_first_suitable_node() {
        let scheduler = Scheduler::new();
        let mut registry = NodeRegistry::new();

        // Node 1: Small, no GPU
        let _node1 = register_node(&mut registry, 2, 4096, vec![]);

        // Node 2: Large with GPU
        let gpu = make_gpu("A100", 40960);
        let _node2 = register_node(&mut registry, 16, 65536, vec![gpu]);

        // Simple workload that fits node1
        let spec = WorkloadSpec::new("nginx:latest")
            .with_cpu_cores(1)
            .with_memory_mb(512);

        let result = scheduler.schedule(&spec, &registry);

        // Should find a suitable node (either one works)
        assert!(result.is_ok());
    }

    #[test]
    fn test_schedule_skips_unsuitable_nodes() {
        let scheduler = Scheduler::new();
        let mut registry = NodeRegistry::new();

        // Node 1: Too small
        register_node(&mut registry, 2, 2048, vec![]);

        // Node 2: Just right
        let node2 = register_node(&mut registry, 8, 16384, vec![]);

        let spec = WorkloadSpec::new("nginx:latest")
            .with_cpu_cores(4)
            .with_memory_mb(8192);

        let result = scheduler.schedule(&spec, &registry);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), node2);
    }

    // ==================== Memory Headroom Tests ====================

    #[test]
    fn test_schedule_with_memory_headroom() {
        let scheduler = Scheduler::with_memory_headroom(1024);
        let mut registry = NodeRegistry::new();

        // Node has 8192 MiB
        register_node(&mut registry, 4, 8192, vec![]);

        // Workload needs 8000 MiB, but with 1024 headroom, total is 9024
        let spec = WorkloadSpec::new("nginx:latest")
            .with_memory_mb(8000);

        let result = scheduler.schedule(&spec, &registry);

        // Should fail because 8192 < 8000 + 1024
        assert!(matches!(result, Err(SchedulerError::NoSuitableNode(_))));
    }

    #[test]
    fn test_schedule_headroom_passes_when_enough_memory() {
        let scheduler = Scheduler::with_memory_headroom(1024);
        let mut registry = NodeRegistry::new();

        // Node has 10240 MiB
        let node_id = register_node(&mut registry, 4, 10240, vec![]);

        // Workload needs 8000 MiB + 1024 headroom = 9024 MiB (fits in 10240)
        let spec = WorkloadSpec::new("nginx:latest")
            .with_memory_mb(8000);

        let result = scheduler.schedule(&spec, &registry);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), node_id);
    }

    // ==================== Best Fit Tests ====================

    #[test]
    fn test_schedule_best_fit_prefers_exact_gpu_match() {
        let scheduler = Scheduler::new();
        let mut registry = NodeRegistry::new();

        // Node 1: 2 GPUs
        let _node1 = register_node(
            &mut registry,
            8,
            32768,
            vec![make_gpu("A100 #0", 40960), make_gpu("A100 #1", 40960)],
        );

        // Node 2: 1 GPU (exact match)
        let node2 = register_node(&mut registry, 8, 32768, vec![make_gpu("A100", 40960)]);

        // Node 3: 4 GPUs
        let _node3 = register_node(
            &mut registry,
            8,
            32768,
            vec![
                make_gpu("A100 #0", 40960),
                make_gpu("A100 #1", 40960),
                make_gpu("A100 #2", 40960),
                make_gpu("A100 #3", 40960),
            ],
        );

        let spec = WorkloadSpec::new("pytorch:latest")
            .with_gpu_count(1)
            .with_memory_mb(8192);

        let result = scheduler.schedule_best_fit(&spec, &registry);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), node2);
    }

    #[test]
    fn test_schedule_best_fit_empty_registry() {
        let scheduler = Scheduler::new();
        let registry = NodeRegistry::new();
        let spec = WorkloadSpec::new("nginx:latest");

        let result = scheduler.schedule_best_fit(&spec, &registry);

        assert!(matches!(result, Err(SchedulerError::NoNodes)));
    }

    #[test]
    fn test_schedule_best_fit_no_suitable() {
        let scheduler = Scheduler::new();
        let mut registry = NodeRegistry::new();

        register_node(&mut registry, 2, 2048, vec![]);

        let spec = WorkloadSpec::new("pytorch:latest")
            .with_gpu_count(1);

        let result = scheduler.schedule_best_fit(&spec, &registry);

        assert!(matches!(result, Err(SchedulerError::NoSuitableNode(_))));
    }

    // ==================== Find By GPU Type Tests ====================

    #[test]
    fn test_find_nodes_by_gpu_type() {
        let scheduler = Scheduler::new();
        let mut registry = NodeRegistry::new();

        register_node(&mut registry, 8, 32768, vec![make_gpu("NVIDIA RTX 4090", 24576)]);
        register_node(&mut registry, 8, 32768, vec![make_gpu("NVIDIA A100", 40960)]);
        register_node(&mut registry, 8, 32768, vec![make_gpu("NVIDIA RTX 4090", 24576)]);

        let rtx_nodes = scheduler.find_nodes_by_gpu_type("RTX 4090", &registry);
        let a100_nodes = scheduler.find_nodes_by_gpu_type("A100", &registry);
        let h100_nodes = scheduler.find_nodes_by_gpu_type("H100", &registry);

        assert_eq!(rtx_nodes.len(), 2);
        assert_eq!(a100_nodes.len(), 1);
        assert_eq!(h100_nodes.len(), 0);
    }

    #[test]
    fn test_find_nodes_by_gpu_type_empty_registry() {
        let scheduler = Scheduler::new();
        let registry = NodeRegistry::new();

        let nodes = scheduler.find_nodes_by_gpu_type("RTX 4090", &registry);

        assert!(nodes.is_empty());
    }
}
