# claw-compute API Reference

Multi-platform GPU compute library powered by CubeCL.

## Overview

`claw-compute` provides GPU-accelerated compute operations that work across:

- **NVIDIA CUDA** — Data center GPUs (A100, H100, RTX)
- **Apple Metal** — M1/M2/M3/M4 chips via wgpu
- **AMD ROCm** — MI series accelerators
- **Vulkan** — Cross-platform fallback
- **CPU** — SIMD reference implementation

## Installation

```toml
[dependencies]
claw-compute = { path = "../claw-compute" }

# Enable specific backends
[features]
default = ["cpu"]
metal = ["cubecl-wgpu"]      # macOS
cuda = ["cubecl-cuda"]       # NVIDIA
all = ["metal", "cuda"]
```

## Quick Start

```rust
use claw_compute::{gpu, kernels, CpuTensor, Shape};

// GPU-accelerated operations (Metal/CUDA/Vulkan)
if gpu::is_available() {
    let a = vec![1.0, 2.0, 3.0, 4.0];
    let b = vec![5.0, 6.0, 7.0, 8.0];
    let result = gpu::gpu_add(&a, &b)?;
    // result = [6.0, 8.0, 10.0, 12.0]
}

// CPU fallback
let tensor = CpuTensor::from_f32(&[1.0, 2.0, 3.0, 4.0], Shape::new(&[4]));
let activated = kernels::gelu(&tensor)?;
```

---

## GPU Module

Real GPU compute via CubeCL wgpu backend.

### Availability Check

```rust
use claw_compute::gpu;

// Check if GPU is available
if gpu::is_available() {
    println!("GPU ready!");
}

// Get GPU info
if let Some(info) = gpu::init() {
    println!("Device: {}", info.device);
    println!("Backend: {}", info.backend);  // "Metal", "Vulkan", etc.
}

// Get backend name
let backend = gpu::backend_name();  // "Metal" on macOS
```

### Vector Operations

```rust
use claw_compute::gpu;

let a = vec![1.0f32, 2.0, 3.0, 4.0];
let b = vec![5.0f32, 6.0, 7.0, 8.0];

// Vector addition
let sum = gpu::gpu_add(&a, &b)?;
// [6.0, 8.0, 10.0, 12.0]

// Element-wise multiplication
let product = gpu::gpu_mul(&a, &b)?;
// [5.0, 12.0, 21.0, 32.0]

// Scalar multiplication
let scaled = gpu::gpu_scale(&a, 2.5)?;
// [2.5, 5.0, 7.5, 10.0]
```

### Activation Functions

```rust
use claw_compute::gpu;

let input = vec![-1.0f32, 0.0, 1.0, 2.0];

// ReLU activation
let relu_out = gpu::gpu_relu(&input)?;
// [0.0, 0.0, 1.0, 2.0]

// GELU activation (used in transformers)
let gelu_out = gpu::gpu_gelu(&input)?;
// [-0.159, 0.0, 0.841, 1.955]
```

### GPU Info

```rust
pub struct GpuInfo {
    /// Device description (e.g., "DefaultDevice")
    pub device: String,
    /// Backend type ("Metal", "Vulkan", "CUDA")
    pub backend: &'static str,
    /// Whether initialization succeeded
    pub initialized: bool,
}
```

---

## Device Discovery

Enumerate available compute devices.

### `Platform`

```rust
pub enum Platform {
    /// CPU compute (always available)
    Cpu,
    /// NVIDIA CUDA
    Cuda,
    /// Apple Metal (via wgpu)
    Metal,
    /// AMD ROCm/HIP
    Rocm,
    /// Vulkan (cross-platform)
    Vulkan,
}
```

### `ComputeDevice`

```rust
pub enum ComputeDevice {
    /// CPU device
    Cpu,
    /// GPU device with index
    Gpu {
        index: usize,
        platform: Platform,
    },
}

impl ComputeDevice {
    /// Auto-select best available device
    pub fn auto() -> Self;
    
    /// Check if this is a GPU device
    pub fn is_gpu(&self) -> bool;
    
    /// Get the platform
    pub fn platform(&self) -> Platform;
}
```

### Device Enumeration

```rust
use claw_compute::{discover_devices, DeviceInfo, Platform};

let devices = discover_devices();
for device in devices {
    println!("Device: {} ({:?})", device.name, device.platform);
    if let Some(mem) = device.memory_bytes {
        println!("  Memory: {} GB", mem / 1_073_741_824);
    }
}
```

---

## CPU Kernels

Reference implementations for all operations.

### Tensor Type

```rust
pub struct CpuTensor {
    data: Vec<f32>,
    shape: Shape,
    dtype: DType,
}

impl CpuTensor {
    /// Create from f32 slice
    pub fn from_f32(data: &[f32], shape: Shape) -> Self;
    
    /// Create zeros
    pub fn zeros(shape: Shape) -> Self;
    
    /// Create ones
    pub fn ones(shape: Shape) -> Self;
    
    /// Get shape
    pub fn shape(&self) -> &Shape;
    
    /// Get data as slice
    pub fn data(&self) -> &[f32];
}
```

### Shape

```rust
pub struct Shape {
    dims: Vec<usize>,
}

impl Shape {
    /// Create new shape
    pub fn new(dims: &[usize]) -> Self;
    
    /// Number of dimensions
    pub fn ndim(&self) -> usize;
    
    /// Total number of elements
    pub fn numel(&self) -> usize;
    
    /// Check if shapes are broadcastable
    pub fn is_broadcastable(&self, other: &Shape) -> bool;
}
```

### Activation Functions

```rust
use claw_compute::kernels;

let input = CpuTensor::from_f32(&[-1.0, 0.0, 1.0, 2.0], Shape::new(&[4]));

// GELU: Gaussian Error Linear Unit
let gelu = kernels::gelu(&input)?;

// ReLU: Rectified Linear Unit
let relu = kernels::relu(&input)?;

// SiLU: Sigmoid Linear Unit (Swish)
let silu = kernels::silu(&input)?;

// Softmax
let softmax = kernels::softmax(&input)?;
```

### Element-wise Operations

```rust
use claw_compute::kernels;

let a = CpuTensor::from_f32(&[1.0, 2.0, 3.0, 4.0], Shape::new(&[4]));
let b = CpuTensor::from_f32(&[5.0, 6.0, 7.0, 8.0], Shape::new(&[4]));

// Addition
let sum = kernels::add(&a, &b)?;

// Multiplication
let product = kernels::mul(&a, &b)?;

// Scalar multiply
let scaled = kernels::scale(&a, 2.5)?;
```

### Matrix Operations

```rust
use claw_compute::kernels;

let lhs = CpuTensor::from_f32(&[
    1.0, 2.0,
    3.0, 4.0,
], Shape::new(&[2, 2]));

let rhs = CpuTensor::from_f32(&[
    5.0, 6.0,
    7.0, 8.0,
], Shape::new(&[2, 2]));

// Matrix multiplication
let result = kernels::matmul(&lhs, &rhs)?;
// [[19.0, 22.0], [43.0, 50.0]]
```

---

## Data Types

### `DType`

Supported data types for tensors.

```rust
pub enum DType {
    /// 32-bit float (default)
    F32,
    /// 16-bit float (half precision)
    F16,
    /// 16-bit bfloat
    BF16,
    /// 32-bit integer
    I32,
    /// 64-bit integer
    I64,
}

impl DType {
    /// Size in bytes
    pub fn size(&self) -> usize;
}
```

---

## Error Handling

```rust
pub enum ComputeError {
    /// Shape mismatch in operation
    ShapeMismatch {
        expected: Shape,
        got: Shape,
    },
    /// Device not available
    DeviceNotAvailable(Platform),
    /// Out of memory
    OutOfMemory {
        requested: usize,
        available: usize,
    },
    /// Kernel execution failed
    KernelError(String),
    /// Invalid operation
    InvalidOperation(String),
}
```

### Example

```rust
use claw_compute::{kernels, ComputeError};

match kernels::matmul(&a, &b) {
    Ok(result) => println!("Success: {:?}", result.shape()),
    Err(ComputeError::ShapeMismatch { expected, got }) => {
        eprintln!("Shape mismatch: expected {:?}, got {:?}", expected, got);
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

---

## Feature Flags

| Feature | Description | Default |
|---------|-------------|---------|
| `cpu` | CPU SIMD kernels | ✓ |
| `cubecl-wgpu` | Metal/Vulkan via wgpu | |
| `metal` | Alias for `cubecl-wgpu` | |
| `cubecl-cuda` | NVIDIA CUDA | |
| `all` | All backends | |

### Conditional Compilation

```rust
#[cfg(feature = "cubecl-wgpu")]
{
    use claw_compute::gpu;
    if gpu::is_available() {
        // Use GPU
    }
}
```

---

## Performance Tips

### 1. Batch Operations

Process larger tensors when possible — GPU overhead amortizes over more data.

```rust
// Better: one call with large data
let large = gpu::gpu_add(&data_1m, &data_1m)?;

// Worse: many calls with small data
for chunk in data.chunks(100) {
    gpu::gpu_add(chunk, chunk)?;
}
```

### 2. Reuse Allocations

GPU memory allocation is expensive. Reuse buffers when possible.

### 3. Check Availability Once

```rust
// Do this once at startup
let use_gpu = gpu::is_available();

// Not every operation
if gpu::is_available() { /* ... */ }
```

---

## Workgroup Sizing

CubeCL kernels use automatic workgroup dispatch:

```rust
// Automatic sizing based on data length
const MAX_WORKGROUP_SIZE: u32 = 256;

fn compute_dispatch(len: usize) -> (CubeCount, CubeDim) {
    if len <= MAX_WORKGROUP_SIZE as usize {
        (CubeCount::Static(1, 1, 1), CubeDim::new_1d(len as u32))
    } else {
        let num_cubes = (len + MAX_WORKGROUP_SIZE - 1) / MAX_WORKGROUP_SIZE;
        (CubeCount::Static(num_cubes, 1, 1), CubeDim::new_1d(MAX_WORKGROUP_SIZE))
    }
}
```

This automatically handles vectors of any size, distributing work across multiple GPU workgroups.
