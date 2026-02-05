//! GPU detection and metrics
//!
//! Probes NVIDIA GPUs via nvidia-smi and provides metrics.

use serde::{Deserialize, Serialize};
use std::process::Command;
use tracing::{debug, info, warn};

/// Information about a single GPU
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub index: u32,
    pub name: String,
    pub uuid: String,
    pub memory_total_mb: u64,
    pub pci_bus_id: Option<String>,
}

/// Current GPU metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuMetrics {
    pub index: u32,
    pub uuid: String,
    pub utilization_percent: u32,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub temperature_c: u32,
    pub power_draw_w: Option<f32>,
    pub power_limit_w: Option<f32>,
}

/// Manages GPU detection and metrics collection
#[derive(Debug)]
pub struct GpuManager {
    gpus: Vec<GpuInfo>,
    nvidia_available: bool,
}

impl GpuManager {
    pub fn new() -> Self {
        let mut manager = Self {
            gpus: Vec::new(),
            nvidia_available: false,
        };
        manager.detect_gpus();
        manager
    }
    
    /// Detect available GPUs
    pub fn detect_gpus(&mut self) {
        self.gpus.clear();
        
        // Try nvidia-smi
        match self.detect_nvidia_gpus() {
            Ok(gpus) => {
                self.nvidia_available = true;
                self.gpus = gpus;
                info!(count = self.gpus.len(), "detected NVIDIA GPUs");
            }
            Err(e) => {
                debug!(error = %e, "nvidia-smi not available");
                self.nvidia_available = false;
            }
        }
        
        // TODO: Add AMD ROCm support (rocm-smi)
        // TODO: Add Intel GPU support
    }
    
    /// Detect NVIDIA GPUs using nvidia-smi
    fn detect_nvidia_gpus(&self) -> anyhow::Result<Vec<GpuInfo>> {
        let output = Command::new("nvidia-smi")
            .args([
                "--query-gpu=index,name,uuid,memory.total,pci.bus_id",
                "--format=csv,noheader,nounits",
            ])
            .output()?;
        
        if !output.status.success() {
            anyhow::bail!("nvidia-smi failed: {}", String::from_utf8_lossy(&output.stderr));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut gpus = Vec::new();
        
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split(", ").collect();
            if parts.len() >= 4 {
                let index: u32 = parts[0].trim().parse().unwrap_or(0);
                let name = parts[1].trim().to_string();
                let uuid = parts[2].trim().to_string();
                let memory_total_mb: u64 = parts[3].trim().parse().unwrap_or(0);
                let pci_bus_id = parts.get(4).map(|s| s.trim().to_string());
                
                gpus.push(GpuInfo {
                    index,
                    name,
                    uuid,
                    memory_total_mb,
                    pci_bus_id,
                });
            }
        }
        
        Ok(gpus)
    }
    
    /// Get list of detected GPUs
    pub fn list(&self) -> Vec<GpuInfo> {
        self.gpus.clone()
    }
    
    /// Get GPU count
    pub fn count(&self) -> usize {
        self.gpus.len()
    }
    
    /// Check if NVIDIA drivers are available
    pub fn has_nvidia(&self) -> bool {
        self.nvidia_available
    }
    
    /// Get current metrics for all GPUs
    pub fn get_metrics(&self) -> anyhow::Result<Vec<GpuMetrics>> {
        if !self.nvidia_available {
            return Ok(Vec::new());
        }
        
        let output = Command::new("nvidia-smi")
            .args([
                "--query-gpu=index,uuid,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,power.limit",
                "--format=csv,noheader,nounits",
            ])
            .output()?;
        
        if !output.status.success() {
            anyhow::bail!("nvidia-smi metrics query failed");
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut metrics = Vec::new();
        
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split(", ").collect();
            if parts.len() >= 6 {
                let index: u32 = parts[0].trim().parse().unwrap_or(0);
                let uuid = parts[1].trim().to_string();
                let utilization_percent: u32 = parts[2].trim().parse().unwrap_or(0);
                let memory_used_mb: u64 = parts[3].trim().parse().unwrap_or(0);
                let memory_total_mb: u64 = parts[4].trim().parse().unwrap_or(0);
                let temperature_c: u32 = parts[5].trim().parse().unwrap_or(0);
                let power_draw_w: Option<f32> = parts.get(6).and_then(|s| s.trim().parse().ok());
                let power_limit_w: Option<f32> = parts.get(7).and_then(|s| s.trim().parse().ok());
                
                metrics.push(GpuMetrics {
                    index,
                    uuid,
                    utilization_percent,
                    memory_used_mb,
                    memory_total_mb,
                    temperature_c,
                    power_draw_w,
                    power_limit_w,
                });
            }
        }
        
        Ok(metrics)
    }
    
    /// Get total VRAM across all GPUs
    pub fn total_memory_gb(&self) -> u64 {
        self.gpus.iter().map(|g| g.memory_total_mb).sum::<u64>() / 1024
    }
    
    /// Build capability list for registration
    pub fn capabilities(&self) -> Vec<String> {
        let mut caps = vec!["system".to_string()];
        
        if self.nvidia_available {
            caps.push("gpu".to_string());
            caps.push("nvidia".to_string());
        }
        
        // Check for container runtimes
        if Command::new("docker").arg("--version").output().is_ok() {
            caps.push("docker".to_string());
            caps.push("container".to_string());
        }
        
        if Command::new("podman").arg("--version").output().is_ok() {
            caps.push("podman".to_string());
            if !caps.contains(&"container".to_string()) {
                caps.push("container".to_string());
            }
        }
        
        caps
    }
    
    /// Build command list for registration
    pub fn commands(&self) -> Vec<String> {
        let mut cmds = vec![
            "system.info".to_string(),
            "system.run".to_string(),
        ];
        
        if self.nvidia_available {
            cmds.push("gpu.list".to_string());
            cmds.push("gpu.metrics".to_string());
        }
        
        if self.capabilities().contains(&"container".to_string()) {
            cmds.push("workload.run".to_string());
            cmds.push("workload.stop".to_string());
            cmds.push("workload.logs".to_string());
            cmds.push("container.exec".to_string());
        }
        
        cmds
    }
}

impl Default for GpuManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_gpu_manager_creation() {
        let manager = GpuManager::new();
        // Just verify it doesn't panic
        let _ = manager.count();
        let _ = manager.capabilities();
        let _ = manager.commands();
    }
}
