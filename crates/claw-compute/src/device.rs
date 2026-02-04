//! Device abstraction for multi-platform GPU compute.
//!
//! Provides a unified interface over CUDA, Metal, `ROCm`, and CPU backends.

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::error::Result;
#[cfg(not(feature = "cpu"))]
use crate::error::ComputeError;

/// GPU/compute platform type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    /// NVIDIA CUDA.
    Cuda,
    /// Apple Metal (via wgpu).
    Metal,
    /// AMD `ROCm` (via HIP).
    Rocm,
    /// Vulkan (via wgpu).
    Vulkan,
    /// WebGPU (via wgpu).
    WebGpu,
    /// CPU with SIMD.
    Cpu,
}

impl Platform {
    /// Check if this platform supports tensor cores.
    #[must_use]
    pub const fn supports_tensor_cores(&self) -> bool {
        matches!(self, Self::Cuda)
    }

    /// Check if this platform is a GPU.
    #[must_use]
    pub const fn is_gpu(&self) -> bool {
        !matches!(self, Self::Cpu)
    }

    /// Get platform display name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Cuda => "CUDA",
            Self::Metal => "Metal",
            Self::Rocm => "ROCm",
            Self::Vulkan => "Vulkan",
            Self::WebGpu => "WebGPU",
            Self::Cpu => "CPU",
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Information about a compute device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Platform type.
    pub platform: Platform,
    /// Device name.
    pub name: String,
    /// Device index (for multi-GPU).
    pub index: usize,
    /// Total memory in bytes.
    pub memory_bytes: u64,
    /// Whether tensor cores are available.
    pub tensor_cores: bool,
    /// Compute capability (CUDA) or GPU family (Metal).
    pub compute_capability: Option<String>,
    /// Whether FP16 is supported.
    pub fp16_support: bool,
    /// Whether BF16 is supported.
    pub bf16_support: bool,
}

impl DeviceInfo {
    /// Get memory in GiB.
    #[must_use]
    pub fn memory_gib(&self) -> f64 {
        self.memory_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }
}

/// A compute device that can execute kernels.
///
/// This enum wraps platform-specific device types and provides
/// a unified interface for kernel execution.
#[derive(Debug)]
pub enum ComputeDevice {
    /// CUDA device.
    #[cfg(feature = "cuda")]
    Cuda {
        /// Device info.
        info: DeviceInfo,
    },
    /// Metal device (via wgpu).
    #[cfg(feature = "wgpu")]
    Metal {
        /// Device info.
        info: DeviceInfo,
    },
    /// Vulkan device (via wgpu).
    #[cfg(feature = "wgpu")]
    Vulkan {
        /// Device info.
        info: DeviceInfo,
    },
    /// ROCm/HIP device.
    #[cfg(feature = "hip")]
    Rocm {
        /// Device info.
        info: DeviceInfo,
    },
    /// CPU device with SIMD.
    #[cfg(feature = "cpu")]
    Cpu {
        /// Device info.
        info: DeviceInfo,
    },
}

impl ComputeDevice {
    /// Auto-detect the best available compute device.
    ///
    /// Priority: CUDA > `ROCm` > Metal > Vulkan > CPU
    ///
    /// # Errors
    ///
    /// Returns error if no device is available.
    pub fn auto() -> Result<Self> {
        info!("auto-detecting compute device");

        // Try CUDA first (NVIDIA GPUs)
        #[cfg(feature = "cuda")]
        {
            if let Ok(device) = Self::cuda(0) {
                info!(platform = "CUDA", name = %device.info().name, "selected device");
                return Ok(device);
            }
            debug!("CUDA not available");
        }

        // Try ROCm (AMD GPUs)
        #[cfg(feature = "hip")]
        {
            if let Ok(device) = Self::rocm(0) {
                info!(platform = "ROCm", name = %device.info().name, "selected device");
                return Ok(device);
            }
            debug!("ROCm not available");
        }

        // Try Metal on macOS
        #[cfg(all(feature = "wgpu", target_os = "macos"))]
        {
            if let Ok(device) = Self::metal(0) {
                info!(platform = "Metal", name = %device.info().name, "selected device");
                return Ok(device);
            }
            debug!("Metal not available");
        }

        // Try Vulkan on non-macOS
        #[cfg(all(feature = "wgpu", not(target_os = "macos")))]
        {
            if let Ok(device) = Self::vulkan(0) {
                info!(platform = "Vulkan", name = %device.info().name, "selected device");
                return Ok(device);
            }
            debug!("Vulkan not available");
        }

        // Fallback to CPU
        #[cfg(feature = "cpu")]
        {
            let device = Self::cpu()?;
            warn!(platform = "CPU", "falling back to CPU compute");
            Ok(device)
        }

        #[cfg(not(feature = "cpu"))]
        Err(ComputeError::NoDevice)
    }

    /// Create a CUDA device.
    #[cfg(feature = "cuda")]
    pub fn cuda(index: usize) -> Result<Self> {
        // In real implementation, query CUDA runtime
        // For now, create stub info
        let info = DeviceInfo {
            platform: Platform::Cuda,
            name: format!("CUDA Device {index}"),
            index,
            memory_bytes: 0,
            tensor_cores: true,
            compute_capability: Some("8.0".to_string()),
            fp16_support: true,
            bf16_support: true,
        };
        Ok(Self::Cuda { info })
    }

    /// Create a Metal device (macOS only).
    #[cfg(feature = "wgpu")]
    pub fn metal(index: usize) -> Result<Self> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = index;
            return Err(ComputeError::UnsupportedOperation {
                platform: "non-macOS".to_string(),
                operation: "Metal".to_string(),
            });
        }

        #[cfg(target_os = "macos")]
        {
            let info = DeviceInfo {
                platform: Platform::Metal,
                name: format!("Metal Device {index}"),
                index,
                memory_bytes: 0,
                tensor_cores: false,
                compute_capability: Some("Apple GPU".to_string()),
                fp16_support: true,
                bf16_support: false,
            };
            Ok(Self::Metal { info })
        }
    }

    /// Create a Vulkan device.
    #[cfg(feature = "wgpu")]
    pub fn vulkan(index: usize) -> Result<Self> {
        let info = DeviceInfo {
            platform: Platform::Vulkan,
            name: format!("Vulkan Device {index}"),
            index,
            memory_bytes: 0,
            tensor_cores: false,
            compute_capability: None,
            fp16_support: true,
            bf16_support: false,
        };
        Ok(Self::Vulkan { info })
    }

    /// Create a ROCm device.
    #[cfg(feature = "hip")]
    pub fn rocm(index: usize) -> Result<Self> {
        let info = DeviceInfo {
            platform: Platform::Rocm,
            name: format!("ROCm Device {index}"),
            index,
            memory_bytes: 0,
            tensor_cores: false,
            compute_capability: Some("gfx90a".to_string()),
            fp16_support: true,
            bf16_support: true,
        };
        Ok(Self::Rocm { info })
    }

    /// Create a CPU device.
    #[cfg(feature = "cpu")]
    pub fn cpu() -> Result<Self> {
        let num_cores = std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(1);

        let info = DeviceInfo {
            platform: Platform::Cpu,
            name: format!("CPU ({num_cores} cores)"),
            index: 0,
            memory_bytes: 0, // Could query system memory
            tensor_cores: false,
            compute_capability: None,
            fp16_support: false,
            bf16_support: false,
        };
        Ok(Self::Cpu { info })
    }

    /// Get device info.
    #[must_use]
    pub fn info(&self) -> &DeviceInfo {
        match self {
            #[cfg(feature = "cuda")]
            Self::Cuda { info } => info,
            #[cfg(feature = "wgpu")]
            Self::Metal { info } => info,
            #[cfg(feature = "wgpu")]
            Self::Vulkan { info } => info,
            #[cfg(feature = "hip")]
            Self::Rocm { info } => info,
            #[cfg(feature = "cpu")]
            Self::Cpu { info } => info,
        }
    }

    /// Get the platform.
    #[must_use]
    pub fn platform(&self) -> Platform {
        self.info().platform
    }

    /// Check if this is a GPU device.
    #[must_use]
    pub fn is_gpu(&self) -> bool {
        self.platform().is_gpu()
    }
}

/// Discover all available compute devices.
pub fn discover_devices() -> Vec<DeviceInfo> {
    let mut devices = vec![];

    #[cfg(feature = "cuda")]
    {
        // In real implementation, enumerate CUDA devices
        if let Ok(device) = ComputeDevice::cuda(0) {
            devices.push(device.info().clone());
        }
    }

    #[cfg(feature = "hip")]
    {
        if let Ok(device) = ComputeDevice::rocm(0) {
            devices.push(device.info().clone());
        }
    }

    #[cfg(all(feature = "wgpu", target_os = "macos"))]
    {
        if let Ok(device) = ComputeDevice::metal(0) {
            devices.push(device.info().clone());
        }
    }

    #[cfg(all(feature = "wgpu", not(target_os = "macos")))]
    {
        if let Ok(device) = ComputeDevice::vulkan(0) {
            devices.push(device.info().clone());
        }
    }

    #[cfg(feature = "cpu")]
    {
        if let Ok(device) = ComputeDevice::cpu() {
            devices.push(device.info().clone());
        }
    }

    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_display() {
        assert_eq!(Platform::Cuda.to_string(), "CUDA");
        assert_eq!(Platform::Metal.to_string(), "Metal");
        assert_eq!(Platform::Cpu.to_string(), "CPU");
    }

    #[test]
    fn test_platform_is_gpu() {
        assert!(Platform::Cuda.is_gpu());
        assert!(Platform::Metal.is_gpu());
        assert!(!Platform::Cpu.is_gpu());
    }

    #[test]
    fn test_platform_tensor_cores() {
        assert!(Platform::Cuda.supports_tensor_cores());
        assert!(!Platform::Metal.supports_tensor_cores());
        assert!(!Platform::Cpu.supports_tensor_cores());
    }

    #[test]
    fn test_device_info_memory() {
        let info = DeviceInfo {
            platform: Platform::Cuda,
            name: "Test".to_string(),
            index: 0,
            memory_bytes: 16 * 1024 * 1024 * 1024, // 16 GiB
            tensor_cores: true,
            compute_capability: None,
            fp16_support: true,
            bf16_support: true,
        };
        assert!((info.memory_gib() - 16.0).abs() < 0.001);
    }

    #[test]
    #[cfg(feature = "cpu")]
    fn test_cpu_device() {
        let device = ComputeDevice::cpu().expect("should create CPU device");
        assert_eq!(device.platform(), Platform::Cpu);
        assert!(!device.is_gpu());
    }

    #[test]
    fn test_discover_devices() {
        let devices = discover_devices();
        // At minimum, CPU should be available
        #[cfg(feature = "cpu")]
        assert!(!devices.is_empty());
    }

    #[test]
    #[cfg(feature = "cpu")]
    fn test_auto_detect() {
        // Should at least get CPU
        let device = ComputeDevice::auto().expect("should find device");
        assert!(device.info().name.len() > 0);
    }
}
