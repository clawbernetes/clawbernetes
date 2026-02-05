//! Container runtime with GPU passthrough support.
//!
//! This module provides container execution capabilities for Clawbernetes,
//! enabling workloads to run in isolated containers with optional GPU support.
//!
//! ## Features
//!
//! - **Container Lifecycle**: Create, start, stop, and remove containers
//! - **GPU Passthrough**: NVIDIA GPU support via nvidia-container-runtime
//! - **Resource Isolation**: Memory limits, CPU pinning, and more
//! - **Docker Integration**: Full Docker API support via bollard
//!
//! ## Example
//!
//! ```rust,ignore
//! // Requires the `container-runtime` feature
//! use claw_compute::container::{
//!     ContainerConfig, ContainerRuntime, DockerRuntime,
//!     GpuRequirements, MemoryConfig, CpuConfig,
//! };
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Connect to Docker
//! let runtime = DockerRuntime::connect()?
//!     .with_gpu_runtime("nvidia");
//!
//! // Configure container with GPU
//! let config = ContainerConfig::new("ml-training", "nvidia/cuda:12.0-base")
//!     .with_command(vec!["python".into(), "train.py".into()])
//!     .with_memory(MemoryConfig::limit_gb(32))
//!     .with_cpu(CpuConfig::cpus(8.0))
//!     .with_gpu(GpuRequirements::count(2))
//!     .with_env("CUDA_VISIBLE_DEVICES", "0,1")
//!     .auto_remove();
//!
//! // Create and start
//! let id = runtime.create(&config).await?;
//! runtime.start(&id).await?;
//!
//! // Wait for completion
//! let exit_code = runtime.wait(&id).await?;
//! println!("Container exited with code: {exit_code}");
//! # Ok(())
//! # }
//! ```
//!
//! ## GPU Passthrough
//!
//! GPU passthrough requires:
//! - NVIDIA driver installed on host
//! - nvidia-container-runtime installed
//! - Docker configured to use nvidia runtime
//!
//! ```rust,no_run
//! use claw_compute::container::{GpuDevice, GpuRequirements};
//!
//! // Request all GPUs
//! let all_gpus = GpuRequirements::all();
//!
//! // Request specific count
//! let two_gpus = GpuRequirements::count(2);
//!
//! // Request specific devices by index
//! let specific = GpuRequirements::devices(vec![
//!     GpuDevice::by_index(0),
//!     GpuDevice::by_index(2),
//! ]);
//!
//! // Request by UUID with memory limit
//! let by_uuid = GpuRequirements::devices(vec![
//!     GpuDevice::by_uuid("GPU-12345678-abcd-efgh-ijkl")
//!         .with_memory_limit_gb(8),
//! ]);
//! ```
//!
//! ## Resource Isolation
//!
//! ```rust,no_run
//! use claw_compute::container::{MemoryConfig, CpuConfig};
//!
//! // Memory limits
//! let mem = MemoryConfig::limit_gb(16)
//!     .with_reservation_bytes(8 * 1024 * 1024 * 1024)
//!     .with_no_swap()
//!     .with_oom_kill_disabled();
//!
//! // CPU limits - fractional CPUs
//! let cpu = CpuConfig::cpus(4.5);
//!
//! // CPU pinning to specific cores
//! let pinned = CpuConfig::pinned("0-7")
//!     .with_numa_nodes("0");
//! ```

pub mod config;
#[cfg(feature = "container-runtime")]
pub mod docker;
pub mod error;
pub mod runtime;
pub mod status;

// Re-exports
pub use config::{
    ContainerConfig, CpuConfig, GpuDevice, GpuRequirements, HealthCheck, MemoryConfig,
    MountType, NetworkMode, RestartPolicy, VolumeMount,
};
#[cfg(feature = "container-runtime")]
pub use docker::DockerRuntime;
pub use error::{ContainerError, ContainerId, ContainerResult};
pub use runtime::{
    ContainerRuntime, ContainerRuntimeExt, ExecOptions, ExecResult, ListOptions,
    LogsOptions, RemoveOptions, RuntimeInfo, StopOptions,
};
pub use status::{
    ContainerState, ContainerStatus, ContainerSummary, HealthStatus, NetworkEndpoint,
    NetworkSettings, PortBinding, ResourceStats,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Just verify types are accessible
        let _ = ContainerConfig::new("test", "alpine");
        let _ = GpuRequirements::none();
        let _ = MemoryConfig::limit_gb(8);
        let _ = CpuConfig::unlimited();
        let _ = ContainerState::Running;
    }

    #[test]
    fn test_gpu_workflow() {
        // Demonstrate GPU configuration workflow
        let config = ContainerConfig::new("gpu-workload", "nvidia/cuda:12.0-base")
            .with_command(vec!["nvidia-smi".to_string()])
            .with_env("CUDA_VISIBLE_DEVICES", "0")
            .with_memory(MemoryConfig::limit_gb(16))
            .with_cpu(CpuConfig::cpus(4.0))
            .with_gpu(
                GpuRequirements::count(1)
                    .with_capabilities(vec!["compute".to_string(), "utility".to_string()])
                    .with_cuda_version("12.0"),
            )
            .auto_remove();

        assert!(config.validate().is_ok());
        assert!(config.gpu.is_enabled());
        assert_eq!(config.gpu.devices.len(), 1);
    }

    #[test]
    fn test_resource_isolation_workflow() {
        // Demonstrate resource isolation configuration
        let config = ContainerConfig::new("isolated-workload", "alpine")
            .with_memory(
                MemoryConfig::limit_gb(32)
                    .with_reservation_bytes(16 * 1024 * 1024 * 1024)
                    .with_no_swap()
                    .with_swappiness(0),
            )
            .with_cpu(
                CpuConfig::pinned("0-7")
                    .with_shares(2048)
                    .with_numa_nodes("0"),
            );

        assert!(config.validate().is_ok());
        assert_eq!(config.memory.limit, 32 * 1024 * 1024 * 1024);
        assert_eq!(config.cpu.cpuset_cpus, Some("0-7".to_string()));
    }
}
