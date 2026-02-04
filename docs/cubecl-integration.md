# CubeCL Integration for Multi-Platform GPU Support

## Overview

CubeCL enables Clawbernetes to support multiple GPU platforms with a single codebase:
- **NVIDIA CUDA** - Data center GPUs (A100, H100, RTX)
- **Apple Metal** - M1/M2/M3 chips via wgpu
- **AMD ROCm** - MI series accelerators
- **Vulkan** - Cross-platform fallback
- **CPU SIMD** - Development/testing fallback

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Clawbernetes Workloads                      │
│           (Training, Inference, Data Processing)                │
└─────────────────────────────────┬───────────────────────────────┘
                                  │
┌─────────────────────────────────▼───────────────────────────────┐
│                      claw-compute (new crate)                   │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌───────────┐ │
│  │   Kernels  │  │   Memory   │  │   Device   │  │  Autotune │ │
│  │   Library  │  │   Manager  │  │  Discovery │  │   Engine  │ │
│  └────────────┘  └────────────┘  └────────────┘  └───────────┘ │
└─────────────────────────────────┬───────────────────────────────┘
                                  │
┌─────────────────────────────────▼───────────────────────────────┐
│                         CubeCL 0.9                              │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────┐ │
│  │ cubecl-  │  │ cubecl-  │  │ cubecl-  │  │ cubecl-cpu      │ │
│  │ cuda     │  │ wgpu     │  │ hip      │  │ (SIMD fallback) │ │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────────┬─────────┘ │
└───────┼─────────────┼─────────────┼─────────────────┼───────────┘
        │             │             │                 │
   ┌────▼────┐   ┌────▼────┐   ┌────▼────┐      ┌────▼────┐
   │  CUDA   │   │  Metal  │   │  ROCm   │      │   CPU   │
   │ Runtime │   │  wgpu   │   │   HIP   │      │  SIMD   │
   └─────────┘   └─────────┘   └─────────┘      └─────────┘
        │             │             │                 │
   ┌────▼────┐   ┌────▼────┐   ┌────▼────┐      ┌────▼────┐
   │ NVIDIA  │   │  Apple  │   │   AMD   │      │   Any   │
   │  GPUs   │   │ Silicon │   │  GPUs   │      │   CPU   │
   └─────────┘   └─────────┘   └─────────┘      └─────────┘
```

## Implementation Plan

### Phase 1: claw-compute Crate (Foundation)

```toml
# crates/claw-compute/Cargo.toml
[package]
name = "claw-compute"
version = "0.1.0"

[features]
default = ["cpu"]
cpu = ["cubecl-cpu"]
cuda = ["cubecl-cuda"]
metal = ["cubecl-wgpu"]  # wgpu handles Metal
rocm = ["cubecl-hip"]
all = ["cpu", "cuda", "metal", "rocm"]

[dependencies]
cubecl = "0.9"
cubecl-cpu = { version = "0.9", optional = true }
cubecl-cuda = { version = "0.9", optional = true }
cubecl-wgpu = { version = "0.9", optional = true }
cubecl-hip = { version = "0.9", optional = true }
```

### Phase 2: Device Abstraction

```rust
use cubecl::prelude::*;

/// Unified compute device abstraction.
pub enum ComputeDevice {
    Cuda(cubecl_cuda::CudaDevice),
    Metal(cubecl_wgpu::WgpuDevice),  // wgpu on macOS = Metal
    Rocm(cubecl_hip::HipDevice),
    Cpu(cubecl_cpu::CpuDevice),
}

impl ComputeDevice {
    /// Auto-detect best available device.
    pub fn auto() -> Self {
        #[cfg(feature = "cuda")]
        if let Ok(device) = cubecl_cuda::CudaDevice::default() {
            return Self::Cuda(device);
        }
        
        #[cfg(feature = "rocm")]
        if let Ok(device) = cubecl_hip::HipDevice::default() {
            return Self::Rocm(device);
        }
        
        #[cfg(feature = "metal")]
        if cfg!(target_os = "macos") {
            if let Ok(device) = cubecl_wgpu::WgpuDevice::default() {
                return Self::Metal(device);
            }
        }
        
        // Fallback to CPU
        Self::Cpu(cubecl_cpu::CpuDevice::default())
    }
    
    /// Get device info for scheduling.
    pub fn info(&self) -> DeviceInfo {
        match self {
            Self::Cuda(d) => DeviceInfo::from_cuda(d),
            Self::Metal(d) => DeviceInfo::from_wgpu(d),
            Self::Rocm(d) => DeviceInfo::from_hip(d),
            Self::Cpu(d) => DeviceInfo::from_cpu(d),
        }
    }
}
```

### Phase 3: Kernel Library

Common ML/compute kernels that run on any platform:

```rust
/// Matrix multiplication kernel (uses Tensor Cores when available).
#[cube(launch)]
pub fn matmul<F: Float>(
    a: &Tensor<F>,
    b: &Tensor<F>,
    out: &mut Tensor<F>,
) {
    // CubeCL handles platform-specific optimization
    cubecl_linalg::matmul::launch(a, b, out);
}

/// Element-wise operations
#[cube(launch)]
pub fn gelu<F: Float>(input: &Array<Line<F>>, output: &mut Array<Line<F>>) {
    if ABSOLUTE_POS < input.len() {
        let x = input[ABSOLUTE_POS];
        let sqrt2 = F::new(comptime!(2.0f32.sqrt()));
        output[ABSOLUTE_POS] = x * (Line::erf(x / sqrt2) + 1.0) / 2.0;
    }
}

/// Attention mechanism
#[cube(launch)]
pub fn attention<F: Float>(
    q: &Tensor<F>,
    k: &Tensor<F>,
    v: &Tensor<F>,
    out: &mut Tensor<F>,
) {
    // Flash attention when possible
    cubecl_attention::flash_attention(q, k, v, out);
}
```

### Phase 4: Node Integration

Update clawnode to report CubeCL device capabilities:

```rust
// In clawnode/src/gpu.rs

pub struct GpuCapabilities {
    pub platform: GpuPlatform,
    pub compute_capability: Option<String>,  // CUDA compute capability
    pub metal_family: Option<String>,        // Apple GPU family
    pub memory_gb: f64,
    pub tensor_cores: bool,
    pub fp16_support: bool,
    pub bf16_support: bool,
}

pub enum GpuPlatform {
    Cuda,
    Metal,
    Rocm,
    Vulkan,
    Cpu,
}

impl GpuDetector for CubeclDetector {
    fn detect_gpus(&self) -> Vec<GpuCapabilities> {
        let mut gpus = vec![];
        
        #[cfg(feature = "cuda")]
        gpus.extend(detect_cuda_devices());
        
        #[cfg(all(feature = "metal", target_os = "macos"))]
        gpus.extend(detect_metal_devices());
        
        #[cfg(feature = "rocm")]
        gpus.extend(detect_rocm_devices());
        
        gpus
    }
}
```

### Phase 5: Workload Scheduling

Gateway schedules workloads based on device capabilities:

```rust
pub struct WorkloadRequirements {
    pub min_memory_gb: f64,
    pub tensor_cores_required: bool,
    pub platforms: Vec<GpuPlatform>,  // Acceptable platforms
}

impl Scheduler {
    pub fn find_node(&self, req: &WorkloadRequirements) -> Option<NodeId> {
        self.nodes
            .iter()
            .filter(|n| n.gpu.memory_gb >= req.min_memory_gb)
            .filter(|n| !req.tensor_cores_required || n.gpu.tensor_cores)
            .filter(|n| req.platforms.contains(&n.gpu.platform))
            .min_by_key(|n| n.current_load)
            .map(|n| n.id)
    }
}
```

## Benefits

1. **Single Codebase** - Write kernels once, run on NVIDIA/Apple/AMD
2. **Optimal Performance** - Tensor Cores, Metal Performance Shaders, etc.
3. **Mac Development** - Engineers can develop/test on MacBooks
4. **Cloud Flexibility** - Run on any cloud provider's GPUs
5. **Future-Proof** - New backends added by CubeCL team

## Example: ML Training on Any GPU

```rust
use claw_compute::{ComputeDevice, kernels};

// Auto-detect GPU (CUDA on server, Metal on Mac)
let device = ComputeDevice::auto();
println!("Using: {:?}", device.info().platform);

// Same code works everywhere
let weights = Tensor::zeros(&device, [1024, 1024]);
let input = Tensor::rand(&device, [32, 1024]);

// Forward pass (uses Tensor Cores on A100, ANE on M3)
let output = kernels::matmul(&weights, &input);
let activated = kernels::gelu(&output);
```

## Timeline

| Phase | Description | Effort |
|-------|-------------|--------|
| 1 | claw-compute crate setup | 2 hr |
| 2 | Device abstraction | 3 hr |
| 3 | Core kernel library | 4 hr |
| 4 | Node integration | 2 hr |
| 5 | Scheduler updates | 2 hr |
| **Total** | | **~13 hr** |

## Dependencies

```toml
cubecl = "0.9"
cubecl-cuda = { version = "0.9", optional = true }
cubecl-wgpu = { version = "0.9", optional = true }
cubecl-hip = { version = "0.9", optional = true }
cubecl-cpu = { version = "0.9", optional = true }
cubecl-linalg = "0.9"        # Matrix operations
cubecl-attention = "0.9"     # Attention kernels
```

## References

- [CubeCL GitHub](https://github.com/tracel-ai/cubecl)
- [Burn Framework](https://burn.dev) - Uses CubeCL internally
- [CubeCL Book](https://burn.dev/cubecl-book/)
