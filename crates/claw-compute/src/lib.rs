//! # claw-compute
//!
//! Multi-platform GPU compute for Clawbernetes using `CubeCL`.
//!
//! This crate provides a unified interface for GPU compute across:
//! - **CUDA** - NVIDIA GPUs (data center, gaming)
//! - **Metal** - Apple Silicon (M1, M2, M3)
//! - **`ROCm`** - AMD GPUs (MI series)
//! - **Vulkan** - Cross-platform fallback
//! - **CPU** - SIMD-optimized fallback
//!
//! ## Features
//!
//! Enable backends via cargo features:
//! - `cpu` (default) - CPU with SIMD
//! - `cuda` - NVIDIA CUDA
//! - `wgpu` / `metal` - Apple Metal via wgpu
//! - `hip` / `rocm` - AMD `ROCm` via HIP
//! - `all` - All backends
//!
//! ## Example
//!
//! ```rust,no_run
//! use claw_compute::{ComputeDevice, Platform, CpuTensor, Shape};
//!
//! // Auto-detect best device
//! let device = ComputeDevice::auto().expect("no device");
//! println!("Using: {} ({})", device.info().name, device.platform());
//!
//! // Create tensor
//! let input = CpuTensor::from_f32([4], &[1.0, 2.0, 3.0, 4.0]);
//! println!("Shape: {}", input.shape());
//! ```
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │           Application Code              │
//! └─────────────────────┬───────────────────┘
//!                       │
//! ┌─────────────────────▼───────────────────┐
//! │            claw-compute                  │
//! │  ┌─────────┐  ┌────────┐  ┌──────────┐ │
//! │  │ Device  │  │Kernels │  │ Tensor   │ │
//! │  └─────────┘  └────────┘  └──────────┘ │
//! └─────────────────────┬───────────────────┘
//!                       │
//! ┌─────────────────────▼───────────────────┐
//! │              CubeCL 0.9                  │
//! └─────────────────────┬───────────────────┘
//!                       │
//!     ┌─────────┬───────┼───────┬─────────┐
//!     ▼         ▼       ▼       ▼         ▼
//!   CUDA     Metal    ROCm   Vulkan     CPU
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod device;
pub mod error;
#[cfg(any(feature = "cubecl-wgpu", feature = "cubecl-cuda"))]
pub mod gpu;
pub mod kernels;
pub mod tensor;

// Re-exports
pub use device::{discover_devices, ComputeDevice, DeviceInfo, Platform};
pub use error::{ComputeError, Result};
pub use kernels::{add, gelu, matmul, mul, relu, scale, silu, softmax};
pub use tensor::{CpuTensor, DType, Shape, TensorMeta};

/// Library version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Supported `CubeCL` version.
pub const CUBECL_VERSION: &str = "0.9";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_cubecl_version() {
        assert_eq!(CUBECL_VERSION, "0.9");
    }
}
