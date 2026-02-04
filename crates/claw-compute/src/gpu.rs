//! GPU compute via CubeCL wgpu backend.
//!
//! Provides Metal (macOS), Vulkan (Linux/Windows) GPU acceleration.

#[cfg(feature = "cubecl-wgpu")]
// CubeCL requires unsafe for raw buffer access in kernel launches
#[allow(unsafe_code)]
pub mod wgpu_backend {
    use cubecl::prelude::*;
    use cubecl_wgpu::{WgpuDevice, WgpuRuntime};
    use tracing::info;

    /// GPU device information.
    #[derive(Debug, Clone)]
    pub struct GpuInfo {
        /// Device description.
        pub device: String,
        /// Backend type.
        pub backend: &'static str,
        /// Whether initialization succeeded.
        pub initialized: bool,
    }

    /// Get the backend name based on platform.
    #[must_use]
    pub const fn backend_name() -> &'static str {
        #[cfg(target_os = "macos")]
        {
            "Metal"
        }
        #[cfg(target_os = "windows")]
        {
            "Vulkan/DX12"
        }
        #[cfg(target_os = "linux")]
        {
            "Vulkan"
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            "wgpu"
        }
    }

    /// Initialize wgpu and get GPU info.
    pub fn init() -> Option<GpuInfo> {
        let device = WgpuDevice::default();
        let _client = WgpuRuntime::client(&device);

        let backend = backend_name();

        let info = GpuInfo {
            device: format!("{device:?}"),
            backend,
            initialized: true,
        };

        info!(backend = backend, "GPU initialized via wgpu");

        Some(info)
    }

    /// Check if GPU is available.
    #[must_use]
    pub fn is_available() -> bool {
        init().is_some()
    }

    /// Get the wgpu device for compute operations.
    #[must_use]
    pub fn device() -> WgpuDevice {
        WgpuDevice::default()
    }

    // =========================================================================
    // GPU Kernels
    // =========================================================================

    /// Vector addition kernel.
    #[cube(launch)]
    fn vector_add_kernel(a: &Array<f32>, b: &Array<f32>, out: &mut Array<f32>) {
        if ABSOLUTE_POS < a.len() {
            out[ABSOLUTE_POS] = a[ABSOLUTE_POS] + b[ABSOLUTE_POS];
        }
    }

    /// Vector multiplication kernel.
    #[cube(launch)]
    fn vector_mul_kernel(a: &Array<f32>, b: &Array<f32>, out: &mut Array<f32>) {
        if ABSOLUTE_POS < a.len() {
            out[ABSOLUTE_POS] = a[ABSOLUTE_POS] * b[ABSOLUTE_POS];
        }
    }

    /// Scalar multiply kernel.
    #[cube(launch)]
    fn scalar_mul_kernel(input: &Array<f32>, scalar: f32, out: &mut Array<f32>) {
        if ABSOLUTE_POS < input.len() {
            out[ABSOLUTE_POS] = input[ABSOLUTE_POS] * scalar;
        }
    }

    /// GELU activation kernel.
    #[cube(launch)]
    fn gelu_kernel(input: &Array<f32>, out: &mut Array<f32>) {
        if ABSOLUTE_POS < input.len() {
            let x = input[ABSOLUTE_POS];
            // GELU(x) ≈ x * 0.5 * (1 + tanh(sqrt(2/π) * (x + 0.044715 * x³)))
            // Using tanh approximation for GPU efficiency
            let sqrt_2_pi = 0.7978845608f32; // sqrt(2/π)
            let coeff = 0.044715f32;
            let x3 = x * x * x;
            let inner = sqrt_2_pi * (x + coeff * x3);
            let tanh_inner = f32::tanh(inner);
            out[ABSOLUTE_POS] = x * 0.5f32 * (1.0f32 + tanh_inner);
        }
    }

    /// ReLU activation kernel.
    #[cube(launch)]
    fn relu_kernel(input: &Array<f32>, out: &mut Array<f32>) {
        if ABSOLUTE_POS < input.len() {
            let x = input[ABSOLUTE_POS];
            out[ABSOLUTE_POS] = f32::max(x, 0.0f32);
        }
    }

    // =========================================================================
    // Public GPU Operations
    // =========================================================================

    /// Maximum workgroup size for GPU dispatch.
    const MAX_WORKGROUP_SIZE: u32 = 256;

    /// Calculate cube count and dimension for a given length.
    fn compute_dispatch(len: usize) -> (CubeCount, CubeDim) {
        let len = len as u32;
        if len <= MAX_WORKGROUP_SIZE {
            (CubeCount::Static(1, 1, 1), CubeDim::new_1d(len))
        } else {
            let num_cubes = (len + MAX_WORKGROUP_SIZE - 1) / MAX_WORKGROUP_SIZE;
            (
                CubeCount::Static(num_cubes, 1, 1),
                CubeDim::new_1d(MAX_WORKGROUP_SIZE),
            )
        }
    }

    /// Add two vectors on GPU.
    ///
    /// # Safety
    /// Uses unsafe CubeCL API for raw buffer access.
    #[allow(unsafe_code)]
    pub fn gpu_add(a: &[f32], b: &[f32]) -> Result<Vec<f32>, String> {
        if a.len() != b.len() {
            return Err("Vector lengths must match".to_string());
        }

        let device = WgpuDevice::default();
        let client = WgpuRuntime::client(&device);
        let len = a.len();

        let handle_a = client.create_from_slice(f32::as_bytes(a));
        let handle_b = client.create_from_slice(f32::as_bytes(b));
        let handle_out = client.empty(len * core::mem::size_of::<f32>());

        let (cube_count, cube_dim) = compute_dispatch(len);

        vector_add_kernel::launch::<WgpuRuntime>(
            &client,
            cube_count,
            cube_dim,
            unsafe { ArrayArg::from_raw_parts::<f32>(&handle_a, len, 1) },
            unsafe { ArrayArg::from_raw_parts::<f32>(&handle_b, len, 1) },
            unsafe { ArrayArg::from_raw_parts::<f32>(&handle_out, len, 1) },
        )
        .map_err(|e| format!("Kernel launch failed: {e:?}"))?;

        let bytes = client.read_one(handle_out);
        Ok(f32::from_bytes(&bytes).to_vec())
    }

    /// Multiply two vectors element-wise on GPU.
    #[allow(unsafe_code)]
    pub fn gpu_mul(a: &[f32], b: &[f32]) -> Result<Vec<f32>, String> {
        if a.len() != b.len() {
            return Err("Vector lengths must match".to_string());
        }

        let device = WgpuDevice::default();
        let client = WgpuRuntime::client(&device);
        let len = a.len();

        let handle_a = client.create_from_slice(f32::as_bytes(a));
        let handle_b = client.create_from_slice(f32::as_bytes(b));
        let handle_out = client.empty(len * core::mem::size_of::<f32>());

        let (cube_count, cube_dim) = compute_dispatch(len);

        vector_mul_kernel::launch::<WgpuRuntime>(
            &client,
            cube_count,
            cube_dim,
            unsafe { ArrayArg::from_raw_parts::<f32>(&handle_a, len, 1) },
            unsafe { ArrayArg::from_raw_parts::<f32>(&handle_b, len, 1) },
            unsafe { ArrayArg::from_raw_parts::<f32>(&handle_out, len, 1) },
        )
        .map_err(|e| format!("Kernel launch failed: {e:?}"))?;

        let bytes = client.read_one(handle_out);
        Ok(f32::from_bytes(&bytes).to_vec())
    }

    /// Scale a vector by a scalar on GPU.
    #[allow(unsafe_code)]
    pub fn gpu_scale(input: &[f32], scalar: f32) -> Result<Vec<f32>, String> {
        let device = WgpuDevice::default();
        let client = WgpuRuntime::client(&device);
        let len = input.len();

        let handle_in = client.create_from_slice(f32::as_bytes(input));
        let handle_out = client.empty(len * core::mem::size_of::<f32>());

        let (cube_count, cube_dim) = compute_dispatch(len);

        scalar_mul_kernel::launch::<WgpuRuntime>(
            &client,
            cube_count,
            cube_dim,
            unsafe { ArrayArg::from_raw_parts::<f32>(&handle_in, len, 1) },
            ScalarArg::new(scalar),
            unsafe { ArrayArg::from_raw_parts::<f32>(&handle_out, len, 1) },
        )
        .map_err(|e| format!("Kernel launch failed: {e:?}"))?;

        let bytes = client.read_one(handle_out);
        Ok(f32::from_bytes(&bytes).to_vec())
    }

    /// Apply GELU activation on GPU.
    #[allow(unsafe_code)]
    pub fn gpu_gelu(input: &[f32]) -> Result<Vec<f32>, String> {
        let device = WgpuDevice::default();
        let client = WgpuRuntime::client(&device);
        let len = input.len();

        let handle_in = client.create_from_slice(f32::as_bytes(input));
        let handle_out = client.empty(len * core::mem::size_of::<f32>());

        let (cube_count, cube_dim) = compute_dispatch(len);

        gelu_kernel::launch::<WgpuRuntime>(
            &client,
            cube_count,
            cube_dim,
            unsafe { ArrayArg::from_raw_parts::<f32>(&handle_in, len, 1) },
            unsafe { ArrayArg::from_raw_parts::<f32>(&handle_out, len, 1) },
        )
        .map_err(|e| format!("Kernel launch failed: {e:?}"))?;

        let bytes = client.read_one(handle_out);
        Ok(f32::from_bytes(&bytes).to_vec())
    }

    /// Apply ReLU activation on GPU.
    #[allow(unsafe_code)]
    pub fn gpu_relu(input: &[f32]) -> Result<Vec<f32>, String> {
        let device = WgpuDevice::default();
        let client = WgpuRuntime::client(&device);
        let len = input.len();

        let handle_in = client.create_from_slice(f32::as_bytes(input));
        let handle_out = client.empty(len * core::mem::size_of::<f32>());

        let (cube_count, cube_dim) = compute_dispatch(len);

        relu_kernel::launch::<WgpuRuntime>(
            &client,
            cube_count,
            cube_dim,
            unsafe { ArrayArg::from_raw_parts::<f32>(&handle_in, len, 1) },
            unsafe { ArrayArg::from_raw_parts::<f32>(&handle_out, len, 1) },
        )
        .map_err(|e| format!("Kernel launch failed: {e:?}"))?;

        let bytes = client.read_one(handle_out);
        Ok(f32::from_bytes(&bytes).to_vec())
    }
}

#[cfg(feature = "cubecl-wgpu")]
pub use wgpu_backend::{
    backend_name, device, gpu_add, gpu_gelu, gpu_mul, gpu_relu, gpu_scale, init, is_available,
    GpuInfo,
};

#[cfg(test)]
#[cfg(feature = "cubecl-wgpu")]
mod tests {
    use super::wgpu_backend;

    #[test]
    fn test_gpu_available() {
        let available = wgpu_backend::is_available();
        println!("GPU available: {available}");
        assert!(available, "GPU should be available on this machine");
    }

    #[test]
    fn test_gpu_info() {
        if let Some(info) = wgpu_backend::init() {
            println!("GPU Device: {}", info.device);
            println!("Backend: {}", info.backend);
            assert!(info.initialized);

            #[cfg(target_os = "macos")]
            assert_eq!(info.backend, "Metal");
        }
    }

    #[test]
    fn test_gpu_add() {
        let a = vec![1.0f32, 2.0, 3.0, 4.0];
        let b = vec![5.0f32, 6.0, 7.0, 8.0];

        let result = wgpu_backend::gpu_add(&a, &b).expect("GPU add failed");

        println!("GPU add result: {result:?}");
        assert_eq!(result.len(), 4);
        assert!((result[0] - 6.0).abs() < 0.001);
        assert!((result[1] - 8.0).abs() < 0.001);
        assert!((result[2] - 10.0).abs() < 0.001);
        assert!((result[3] - 12.0).abs() < 0.001);
    }

    #[test]
    fn test_gpu_mul() {
        let a = vec![1.0f32, 2.0, 3.0, 4.0];
        let b = vec![2.0f32, 3.0, 4.0, 5.0];

        let result = wgpu_backend::gpu_mul(&a, &b).expect("GPU mul failed");

        println!("GPU mul result: {result:?}");
        assert_eq!(result.len(), 4);
        assert!((result[0] - 2.0).abs() < 0.001);
        assert!((result[1] - 6.0).abs() < 0.001);
        assert!((result[2] - 12.0).abs() < 0.001);
        assert!((result[3] - 20.0).abs() < 0.001);
    }

    #[test]
    fn test_gpu_scale() {
        let input = vec![1.0f32, 2.0, 3.0, 4.0];
        let scalar = 2.5f32;

        let result = wgpu_backend::gpu_scale(&input, scalar).expect("GPU scale failed");

        println!("GPU scale result: {result:?}");
        assert_eq!(result.len(), 4);
        assert!((result[0] - 2.5).abs() < 0.001);
        assert!((result[1] - 5.0).abs() < 0.001);
        assert!((result[2] - 7.5).abs() < 0.001);
        assert!((result[3] - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_gpu_relu() {
        let input = vec![-2.0f32, -1.0, 0.0, 1.0, 2.0];

        let result = wgpu_backend::gpu_relu(&input).expect("GPU relu failed");

        println!("GPU relu result: {result:?}");
        assert_eq!(result.len(), 5);
        assert!((result[0] - 0.0).abs() < 0.001);
        assert!((result[1] - 0.0).abs() < 0.001);
        assert!((result[2] - 0.0).abs() < 0.001);
        assert!((result[3] - 1.0).abs() < 0.001);
        assert!((result[4] - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_gpu_gelu() {
        let input = vec![-1.0f32, 0.0, 1.0, 2.0];

        let result = wgpu_backend::gpu_gelu(&input).expect("GPU gelu failed");

        println!("GPU gelu result: {result:?}");
        assert_eq!(result.len(), 4);
        // GELU(-1) ≈ -0.159
        assert!((result[0] - (-0.159)).abs() < 0.05);
        // GELU(0) = 0
        assert!((result[1] - 0.0).abs() < 0.001);
        // GELU(1) ≈ 0.841
        assert!((result[2] - 0.841).abs() < 0.05);
        // GELU(2) ≈ 1.955
        assert!((result[3] - 1.955).abs() < 0.05);
    }

    #[test]
    fn test_gpu_large_vector() {
        // Test with a larger vector to verify GPU parallelism
        let size = 10000;
        let a: Vec<f32> = (0..size).map(|i| i as f32).collect();
        let b: Vec<f32> = (0..size).map(|i| (size - i) as f32).collect();

        let result = wgpu_backend::gpu_add(&a, &b).expect("GPU add large failed");

        assert_eq!(result.len(), size);
        // All elements should equal size (i + (size - i) = size)
        for (i, &val) in result.iter().enumerate() {
            assert!(
                (val - size as f32).abs() < 0.001,
                "Element {i} = {val}, expected {size}"
            );
        }
        println!("GPU processed {size} elements successfully");
    }
}
