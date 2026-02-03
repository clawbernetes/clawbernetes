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

    /// IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Protocol error.
    #[error("protocol error: {0}")]
    Protocol(#[from] claw_proto::ProtoError),
}
