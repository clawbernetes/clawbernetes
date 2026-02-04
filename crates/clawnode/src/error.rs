//! Error types for clawnode.

use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur in node operations.
#[derive(Debug, Error)]
pub enum NodeError {
    /// Gateway connection failed.
    #[error("gateway connection failed: {0}")]
    GatewayConnection(String),

    /// GPU detection failed.
    #[error("GPU detection failed: {0}")]
    GpuDetection(String),

    /// Container runtime error.
    #[error("container runtime error: {0}")]
    ContainerRuntime(String),

    /// Configuration error.
    #[error("configuration error: {0}")]
    Config(String),

    /// Metrics collection failed.
    #[error("metrics collection failed: {0}")]
    Metrics(String),

    /// Workload already exists.
    #[error("workload already exists: {0}")]
    WorkloadExists(Uuid),

    /// Workload not found.
    #[error("workload not found: {0}")]
    WorkloadNotFound(Uuid),

    /// Not enough GPUs available.
    #[error("insufficient GPUs: requested {requested}, available {available}")]
    InsufficientGpus {
        /// Number of GPUs requested.
        requested: u32,
        /// Number of GPUs currently available.
        available: u32,
    },

    /// Workload validation failed.
    #[error("workload validation failed: {0}")]
    WorkloadValidation(String),

    /// Resource limit configuration is invalid.
    #[error("invalid resource limit: {0}")]
    ResourceLimitInvalid(String),

    /// Requested resource exceeds node capacity.
    #[error("resource {resource} exceeds capacity: requested {requested}, available {available}")]
    ResourceExceedsCapacity {
        /// Type of resource.
        resource: String,
        /// Amount requested.
        requested: u64,
        /// Amount available.
        available: u64,
    },

    /// Maximum concurrent workloads exceeded.
    #[error("maximum workloads exceeded: current {current}, max {max}")]
    MaxWorkloadsExceeded {
        /// Current workload count.
        current: u32,
        /// Maximum allowed.
        max: u32,
    },

    /// Insufficient memory for workload.
    #[error("insufficient memory: requested {requested} bytes, available {available} bytes")]
    InsufficientMemory {
        /// Bytes requested.
        requested: u64,
        /// Bytes available.
        available: u64,
    },

    /// Insufficient CPU for workload.
    #[error("insufficient CPU: requested {requested:.2} cores, available {available:.2} cores")]
    InsufficientCpu {
        /// Cores requested.
        requested: f32,
        /// Cores available.
        available: f32,
    },

    /// Insufficient disk space for workload.
    #[error("insufficient disk: requested {requested} bytes, available {available} bytes")]
    InsufficientDisk {
        /// Bytes requested.
        requested: u64,
        /// Bytes available.
        available: u64,
    },

    /// Workload exceeded resource limits.
    #[error("workload {workload_id} exceeded {resource} limit: {message}")]
    ResourceLimitExceeded {
        /// Workload that exceeded limits.
        workload_id: Uuid,
        /// Type of resource exceeded.
        resource: String,
        /// Detailed message.
        message: String,
    },

    /// Workload execution timed out.
    #[error("workload {workload_id} timed out after {elapsed_secs} seconds")]
    ExecutionTimeout {
        /// Workload that timed out.
        workload_id: Uuid,
        /// Seconds elapsed before timeout.
        elapsed_secs: u64,
    },

    /// IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Protocol error.
    #[error("protocol error: {0}")]
    Protocol(#[from] claw_proto::ProtoError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_connection_error_display() {
        let err = NodeError::GatewayConnection("connection refused".to_string());
        assert_eq!(
            err.to_string(),
            "gateway connection failed: connection refused"
        );
    }

    #[test]
    fn test_gpu_detection_error_display() {
        let err = NodeError::GpuDetection("nvidia-smi not found".to_string());
        assert_eq!(
            err.to_string(),
            "GPU detection failed: nvidia-smi not found"
        );
    }

    #[test]
    fn test_container_runtime_error_display() {
        let err = NodeError::ContainerRuntime("docker daemon not running".to_string());
        assert_eq!(
            err.to_string(),
            "container runtime error: docker daemon not running"
        );
    }

    #[test]
    fn test_config_error_display() {
        let err = NodeError::Config("invalid gateway_url".to_string());
        assert_eq!(err.to_string(), "configuration error: invalid gateway_url");
    }

    #[test]
    fn test_metrics_error_display() {
        let err = NodeError::Metrics("failed to query GPU metrics".to_string());
        assert_eq!(
            err.to_string(),
            "metrics collection failed: failed to query GPU metrics"
        );
    }

    #[test]
    fn test_workload_exists_error_display() {
        let id = Uuid::new_v4();
        let err = NodeError::WorkloadExists(id);
        assert!(err.to_string().contains("workload already exists"));
        assert!(err.to_string().contains(&id.to_string()));
    }

    #[test]
    fn test_workload_not_found_error_display() {
        let id = Uuid::new_v4();
        let err = NodeError::WorkloadNotFound(id);
        assert!(err.to_string().contains("workload not found"));
        assert!(err.to_string().contains(&id.to_string()));
    }

    #[test]
    fn test_insufficient_gpus_error_display() {
        let err = NodeError::InsufficientGpus {
            requested: 4,
            available: 2,
        };
        let msg = err.to_string();
        assert!(msg.contains("insufficient GPUs"));
        assert!(msg.contains("requested 4"));
        assert!(msg.contains("available 2"));
    }

    #[test]
    fn test_workload_validation_error_display() {
        let err = NodeError::WorkloadValidation("image name empty".to_string());
        assert_eq!(
            err.to_string(),
            "workload validation failed: image name empty"
        );
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: NodeError = io_err.into();
        assert!(err.to_string().contains("io error"));
    }

    #[test]
    fn test_error_debug_format() {
        let err = NodeError::Config("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("Config"));
    }

    #[test]
    fn test_workload_exists_with_specific_uuid() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let err = NodeError::WorkloadExists(id);
        assert!(err.to_string().contains("550e8400-e29b-41d4-a716-446655440000"));
    }

    #[test]
    fn test_insufficient_gpus_zero_available() {
        let err = NodeError::InsufficientGpus {
            requested: 1,
            available: 0,
        };
        let msg = err.to_string();
        assert!(msg.contains("available 0"));
    }

    #[test]
    fn test_insufficient_gpus_many_requested() {
        let err = NodeError::InsufficientGpus {
            requested: 8,
            available: 4,
        };
        let msg = err.to_string();
        assert!(msg.contains("requested 8"));
        assert!(msg.contains("available 4"));
    }

    // ==================== Resource Exhaustion Error Tests ====================

    #[test]
    fn test_resource_limit_invalid_error() {
        let err = NodeError::ResourceLimitInvalid("memory cannot be zero".to_string());
        let msg = err.to_string();
        assert!(msg.contains("invalid resource limit"));
        assert!(msg.contains("memory cannot be zero"));
    }

    #[test]
    fn test_resource_exceeds_capacity_error() {
        let err = NodeError::ResourceExceedsCapacity {
            resource: "memory".to_string(),
            requested: 32 * 1024 * 1024 * 1024,
            available: 16 * 1024 * 1024 * 1024,
        };
        let msg = err.to_string();
        assert!(msg.contains("memory"));
        assert!(msg.contains("exceeds capacity"));
    }

    #[test]
    fn test_max_workloads_exceeded_error() {
        let err = NodeError::MaxWorkloadsExceeded {
            current: 64,
            max: 64,
        };
        let msg = err.to_string();
        assert!(msg.contains("maximum workloads exceeded"));
        assert!(msg.contains("current 64"));
        assert!(msg.contains("max 64"));
    }

    #[test]
    fn test_insufficient_memory_error() {
        let err = NodeError::InsufficientMemory {
            requested: 8 * 1024 * 1024 * 1024,
            available: 4 * 1024 * 1024 * 1024,
        };
        let msg = err.to_string();
        assert!(msg.contains("insufficient memory"));
        assert!(msg.contains("bytes"));
    }

    #[test]
    fn test_insufficient_cpu_error() {
        let err = NodeError::InsufficientCpu {
            requested: 16.0,
            available: 8.0,
        };
        let msg = err.to_string();
        assert!(msg.contains("insufficient CPU"));
        assert!(msg.contains("16.00"));
        assert!(msg.contains("8.00"));
    }

    #[test]
    fn test_insufficient_disk_error() {
        let err = NodeError::InsufficientDisk {
            requested: 100 * 1024 * 1024 * 1024,
            available: 50 * 1024 * 1024 * 1024,
        };
        let msg = err.to_string();
        assert!(msg.contains("insufficient disk"));
    }

    #[test]
    fn test_resource_limit_exceeded_error() {
        let id = Uuid::new_v4();
        let err = NodeError::ResourceLimitExceeded {
            workload_id: id,
            resource: "memory".to_string(),
            message: "exceeded by 500MB".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains(&id.to_string()));
        assert!(msg.contains("memory"));
        assert!(msg.contains("exceeded"));
    }

    #[test]
    fn test_execution_timeout_error() {
        let id = Uuid::new_v4();
        let err = NodeError::ExecutionTimeout {
            workload_id: id,
            elapsed_secs: 3600,
        };
        let msg = err.to_string();
        assert!(msg.contains(&id.to_string()));
        assert!(msg.contains("timed out"));
        assert!(msg.contains("3600"));
    }
}
