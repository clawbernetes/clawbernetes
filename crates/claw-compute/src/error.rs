//! Error types for claw-compute.

use thiserror::Error;

/// Errors that can occur in compute operations.
#[derive(Debug, Error)]
pub enum ComputeError {
    /// No GPU device available.
    #[error("no GPU device available")]
    NoDevice,

    /// Device initialization failed.
    #[error("device initialization failed: {0}")]
    DeviceInit(String),

    /// Kernel compilation failed.
    #[error("kernel compilation failed: {0}")]
    KernelCompilation(String),

    /// Kernel launch failed.
    #[error("kernel launch failed: {0}")]
    KernelLaunch(String),

    /// Memory allocation failed.
    #[error("memory allocation failed: {size} bytes")]
    MemoryAllocation {
        /// Requested allocation size.
        size: usize,
    },

    /// Invalid tensor shape.
    #[error("invalid tensor shape: expected {expected:?}, got {actual:?}")]
    InvalidShape {
        /// Expected shape.
        expected: Vec<usize>,
        /// Actual shape.
        actual: Vec<usize>,
    },

    /// Unsupported operation on this platform.
    #[error("unsupported operation on {platform}: {operation}")]
    UnsupportedOperation {
        /// Platform name.
        platform: String,
        /// Operation name.
        operation: String,
    },

    /// Device synchronization failed.
    #[error("device synchronization failed: {0}")]
    Sync(String),
}

/// Result type for compute operations.
pub type Result<T> = std::result::Result<T, ComputeError>;
