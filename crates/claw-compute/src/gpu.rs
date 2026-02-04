//! GPU compute via CubeCL wgpu backend.
//!
//! Provides Metal (macOS), Vulkan (Linux/Windows) support.

#[cfg(feature = "cubecl-wgpu")]
pub mod wgpu_backend {
    use cubecl_wgpu::{WgpuDevice, WgpuRuntime};
    use cubecl::Runtime;
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
        { "Metal" }
        #[cfg(target_os = "windows")]
        { "Vulkan/DX12" }
        #[cfg(target_os = "linux")]
        { "Vulkan" }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        { "wgpu" }
    }

    /// Initialize wgpu and get GPU info.
    ///
    /// On macOS this uses Metal, on Linux/Windows it uses Vulkan.
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
}

#[cfg(feature = "cubecl-wgpu")]
pub use wgpu_backend::{backend_name, device, init, is_available, GpuInfo};

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
    fn test_backend_name() {
        let name = wgpu_backend::backend_name();
        println!("Backend: {name}");
        
        #[cfg(target_os = "macos")]
        assert_eq!(name, "Metal");
    }
}
