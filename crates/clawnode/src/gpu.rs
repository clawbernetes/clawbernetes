//! GPU detection and management.
//!
//! This module provides GPU discovery and metrics collection,
//! primarily targeting NVIDIA GPUs via nvidia-smi.

use std::process::Command;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::NodeError;

/// Information about a detected GPU.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GpuInfo {
    /// GPU index (0-based).
    pub index: u32,
    /// GPU name/model (e.g., "NVIDIA GeForce RTX 4090").
    pub name: String,
    /// Total memory in MiB.
    pub memory_total_mib: u64,
    /// GPU UUID (unique identifier).
    pub uuid: String,
}

/// Real-time GPU metrics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GpuMetrics {
    /// GPU index (0-based).
    pub index: u32,
    /// GPU utilization percentage (0-100).
    pub utilization_percent: u8,
    /// Memory used in MiB.
    pub memory_used_mib: u64,
    /// Memory total in MiB.
    pub memory_total_mib: u64,
    /// Temperature in Celsius.
    pub temperature_celsius: u32,
    /// Power usage in Watts.
    pub power_watts: Option<f32>,
}

impl GpuMetrics {
    /// Calculate memory utilization as a percentage.
    #[must_use]
    pub fn memory_utilization_percent(&self) -> f64 {
        if self.memory_total_mib == 0 {
            return 0.0;
        }
        (self.memory_used_mib as f64 / self.memory_total_mib as f64) * 100.0
    }

    /// Check if GPU is under thermal throttling risk.
    #[must_use]
    pub fn is_thermal_warning(&self, threshold: u32) -> bool {
        self.temperature_celsius >= threshold
    }
}

/// Trait for GPU detection implementations.
///
/// This allows for different backends (nvidia-smi, ROCm, etc.)
/// and enables testing with fake implementations.
pub trait GpuDetector: Send + Sync {
    /// Detect all available GPUs.
    ///
    /// # Errors
    ///
    /// Returns an error if GPU detection fails.
    fn detect_gpus(&self) -> Result<Vec<GpuInfo>, NodeError>;

    /// Collect current metrics for all GPUs.
    ///
    /// # Errors
    ///
    /// Returns an error if metrics collection fails.
    fn collect_metrics(&self) -> Result<Vec<GpuMetrics>, NodeError>;

    /// Collect metrics for a specific GPU by index.
    ///
    /// # Errors
    ///
    /// Returns an error if the GPU is not found or metrics collection fails.
    fn collect_metrics_for_gpu(&self, index: u32) -> Result<GpuMetrics, NodeError>;
}

/// NVIDIA GPU detector using nvidia-smi.
#[derive(Debug, Default)]
pub struct NvidiaDetector {
    /// Custom nvidia-smi path (for testing or non-standard installs).
    nvidia_smi_path: Option<String>,
}

impl NvidiaDetector {
    /// Create a new NVIDIA detector with default nvidia-smi path.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a detector with a custom nvidia-smi path.
    #[must_use]
    pub fn with_path(path: impl Into<String>) -> Self {
        Self {
            nvidia_smi_path: Some(path.into()),
        }
    }

    fn nvidia_smi_path(&self) -> &str {
        self.nvidia_smi_path.as_deref().unwrap_or("nvidia-smi")
    }

    /// Parse nvidia-smi CSV output for GPU info.
    ///
    /// Expected format: index, name, memory.total [MiB], uuid
    pub fn parse_gpu_info_csv(output: &str) -> Result<Vec<GpuInfo>, NodeError> {
        let mut gpus = Vec::new();

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split(", ").collect();
            if parts.len() < 4 {
                continue; // Skip malformed lines
            }

            let index = parts[0]
                .trim()
                .parse::<u32>()
                .map_err(|e| NodeError::GpuDetection(format!("invalid GPU index: {e}")))?;

            let name = parts[1].trim().to_string();

            let memory_str = parts[2].trim().replace(" MiB", "");
            let memory_total_mib = memory_str
                .parse::<u64>()
                .map_err(|e| NodeError::GpuDetection(format!("invalid memory value: {e}")))?;

            let uuid = parts[3].trim().to_string();

            gpus.push(GpuInfo {
                index,
                name,
                memory_total_mib,
                uuid,
            });
        }

        Ok(gpus)
    }

    /// Parse nvidia-smi CSV output for GPU metrics.
    ///
    /// Expected format: index, utilization.gpu [%], memory.used [MiB], memory.total [MiB], temperature.gpu, power.draw [W]
    pub fn parse_gpu_metrics_csv(output: &str) -> Result<Vec<GpuMetrics>, NodeError> {
        let mut metrics = Vec::new();

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split(", ").collect();
            if parts.len() < 5 {
                continue; // Skip malformed lines
            }

            let index = parts[0]
                .trim()
                .parse::<u32>()
                .map_err(|e| NodeError::GpuDetection(format!("invalid GPU index: {e}")))?;

            let utilization_str = parts[1].trim().replace(" %", "");
            let utilization_percent = utilization_str
                .parse::<u8>()
                .map_err(|e| NodeError::GpuDetection(format!("invalid utilization: {e}")))?;

            let memory_used_str = parts[2].trim().replace(" MiB", "");
            let memory_used_mib = memory_used_str
                .parse::<u64>()
                .map_err(|e| NodeError::GpuDetection(format!("invalid memory used: {e}")))?;

            let memory_total_str = parts[3].trim().replace(" MiB", "");
            let memory_total_mib = memory_total_str
                .parse::<u64>()
                .map_err(|e| NodeError::GpuDetection(format!("invalid memory total: {e}")))?;

            let temperature_celsius = parts[4]
                .trim()
                .parse::<u32>()
                .map_err(|e| NodeError::GpuDetection(format!("invalid temperature: {e}")))?;

            let power_watts = if parts.len() > 5 {
                let power_str = parts[5].trim().replace(" W", "");
                if power_str == "[N/A]" || power_str.is_empty() {
                    None
                } else {
                    Some(f32::from_str(&power_str).map_err(|e| {
                        NodeError::GpuDetection(format!("invalid power value: {e}"))
                    })?)
                }
            } else {
                None
            };

            metrics.push(GpuMetrics {
                index,
                utilization_percent,
                memory_used_mib,
                memory_total_mib,
                temperature_celsius,
                power_watts,
            });
        }

        Ok(metrics)
    }

    fn run_nvidia_smi(&self, args: &[&str]) -> Result<String, NodeError> {
        let output = Command::new(self.nvidia_smi_path())
            .args(args)
            .output()
            .map_err(|e| NodeError::GpuDetection(format!("failed to run nvidia-smi: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(NodeError::GpuDetection(format!(
                "nvidia-smi failed: {stderr}"
            )));
        }

        String::from_utf8(output.stdout)
            .map_err(|e| NodeError::GpuDetection(format!("invalid nvidia-smi output: {e}")))
    }
}

impl GpuDetector for NvidiaDetector {
    fn detect_gpus(&self) -> Result<Vec<GpuInfo>, NodeError> {
        let output = self.run_nvidia_smi(&[
            "--query-gpu=index,name,memory.total,uuid",
            "--format=csv,noheader",
        ])?;

        Self::parse_gpu_info_csv(&output)
    }

    fn collect_metrics(&self) -> Result<Vec<GpuMetrics>, NodeError> {
        let output = self.run_nvidia_smi(&[
            "--query-gpu=index,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw",
            "--format=csv,noheader",
        ])?;

        Self::parse_gpu_metrics_csv(&output)
    }

    fn collect_metrics_for_gpu(&self, index: u32) -> Result<GpuMetrics, NodeError> {
        let output = self.run_nvidia_smi(&[
            &format!("--id={index}"),
            "--query-gpu=index,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw",
            "--format=csv,noheader",
        ])?;

        let metrics = Self::parse_gpu_metrics_csv(&output)?;
        metrics.into_iter().next().ok_or_else(|| {
            NodeError::GpuDetection(format!("GPU with index {index} not found"))
        })
    }
}

/// A fake GPU detector for testing.
#[derive(Debug, Default)]
pub struct FakeGpuDetector {
    gpus: Vec<GpuInfo>,
    metrics: Vec<GpuMetrics>,
}

impl FakeGpuDetector {
    /// Create a new fake detector with no GPUs.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a fake GPU.
    #[must_use]
    pub fn with_gpu(mut self, info: GpuInfo, metrics: GpuMetrics) -> Self {
        self.gpus.push(info);
        self.metrics.push(metrics);
        self
    }
}

impl GpuDetector for FakeGpuDetector {
    fn detect_gpus(&self) -> Result<Vec<GpuInfo>, NodeError> {
        Ok(self.gpus.clone())
    }

    fn collect_metrics(&self) -> Result<Vec<GpuMetrics>, NodeError> {
        Ok(self.metrics.clone())
    }

    fn collect_metrics_for_gpu(&self, index: u32) -> Result<GpuMetrics, NodeError> {
        self.metrics
            .iter()
            .find(|m| m.index == index)
            .cloned()
            .ok_or_else(|| NodeError::GpuDetection(format!("GPU {index} not found")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_info_creation() {
        let gpu = GpuInfo {
            index: 0,
            name: "NVIDIA GeForce RTX 4090".to_string(),
            memory_total_mib: 24576,
            uuid: "GPU-12345678-1234-1234-1234-123456789abc".to_string(),
        };

        assert_eq!(gpu.index, 0);
        assert_eq!(gpu.name, "NVIDIA GeForce RTX 4090");
        assert_eq!(gpu.memory_total_mib, 24576);
    }

    #[test]
    fn test_gpu_metrics_creation() {
        let metrics = GpuMetrics {
            index: 0,
            utilization_percent: 75,
            memory_used_mib: 12000,
            memory_total_mib: 24576,
            temperature_celsius: 68,
            power_watts: Some(320.5),
        };

        assert_eq!(metrics.index, 0);
        assert_eq!(metrics.utilization_percent, 75);
        assert_eq!(metrics.temperature_celsius, 68);
    }

    #[test]
    fn test_memory_utilization_calculation() {
        let metrics = GpuMetrics {
            index: 0,
            utilization_percent: 50,
            memory_used_mib: 12288,
            memory_total_mib: 24576,
            temperature_celsius: 65,
            power_watts: None,
        };

        let util = metrics.memory_utilization_percent();
        assert!((util - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_memory_utilization_zero_total() {
        let metrics = GpuMetrics {
            index: 0,
            utilization_percent: 0,
            memory_used_mib: 0,
            memory_total_mib: 0,
            temperature_celsius: 0,
            power_watts: None,
        };

        assert_eq!(metrics.memory_utilization_percent(), 0.0);
    }

    #[test]
    fn test_thermal_warning() {
        let metrics = GpuMetrics {
            index: 0,
            utilization_percent: 100,
            memory_used_mib: 20000,
            memory_total_mib: 24576,
            temperature_celsius: 85,
            power_watts: Some(400.0),
        };

        assert!(metrics.is_thermal_warning(85));
        assert!(metrics.is_thermal_warning(80));
        assert!(!metrics.is_thermal_warning(90));
    }

    #[test]
    fn test_parse_single_gpu_info() {
        let csv = "0, NVIDIA GeForce RTX 4090, 24564 MiB, GPU-abc123";

        let gpus = NvidiaDetector::parse_gpu_info_csv(csv).expect("should parse");
        assert_eq!(gpus.len(), 1);

        let gpu = &gpus[0];
        assert_eq!(gpu.index, 0);
        assert_eq!(gpu.name, "NVIDIA GeForce RTX 4090");
        assert_eq!(gpu.memory_total_mib, 24564);
        assert_eq!(gpu.uuid, "GPU-abc123");
    }

    #[test]
    fn test_parse_multiple_gpus_info() {
        let csv = r"0, NVIDIA GeForce RTX 4090, 24564 MiB, GPU-abc123
1, NVIDIA GeForce RTX 4080, 16384 MiB, GPU-def456
2, NVIDIA A100, 81920 MiB, GPU-ghi789";

        let gpus = NvidiaDetector::parse_gpu_info_csv(csv).expect("should parse");
        assert_eq!(gpus.len(), 3);
        assert_eq!(gpus[0].index, 0);
        assert_eq!(gpus[1].index, 1);
        assert_eq!(gpus[2].index, 2);
        assert_eq!(gpus[2].name, "NVIDIA A100");
        assert_eq!(gpus[2].memory_total_mib, 81920);
    }

    #[test]
    fn test_parse_gpu_metrics_with_power() {
        let csv = "0, 75 %, 12000 MiB, 24564 MiB, 68, 320.5 W";

        let metrics = NvidiaDetector::parse_gpu_metrics_csv(csv).expect("should parse");
        assert_eq!(metrics.len(), 1);

        let m = &metrics[0];
        assert_eq!(m.index, 0);
        assert_eq!(m.utilization_percent, 75);
        assert_eq!(m.memory_used_mib, 12000);
        assert_eq!(m.memory_total_mib, 24564);
        assert_eq!(m.temperature_celsius, 68);
        assert!((m.power_watts.expect("power should exist") - 320.5).abs() < 0.01);
    }

    #[test]
    fn test_parse_gpu_metrics_without_power() {
        let csv = "0, 50 %, 8000 MiB, 16384 MiB, 55";

        let metrics = NvidiaDetector::parse_gpu_metrics_csv(csv).expect("should parse");
        assert_eq!(metrics.len(), 1);
        assert!(metrics[0].power_watts.is_none());
    }

    #[test]
    fn test_parse_gpu_metrics_na_power() {
        let csv = "0, 50 %, 8000 MiB, 16384 MiB, 55, [N/A]";

        let metrics = NvidiaDetector::parse_gpu_metrics_csv(csv).expect("should parse");
        assert_eq!(metrics.len(), 1);
        assert!(metrics[0].power_watts.is_none());
    }

    #[test]
    fn test_parse_empty_output() {
        let gpus = NvidiaDetector::parse_gpu_info_csv("").expect("should handle empty");
        assert!(gpus.is_empty());

        let metrics = NvidiaDetector::parse_gpu_metrics_csv("").expect("should handle empty");
        assert!(metrics.is_empty());
    }

    #[test]
    fn test_parse_whitespace_only() {
        let gpus =
            NvidiaDetector::parse_gpu_info_csv("   \n\t\n  ").expect("should handle whitespace");
        assert!(gpus.is_empty());
    }

    #[test]
    fn test_parse_invalid_index() {
        let csv = "not_a_number, GPU Name, 24564 MiB, GPU-abc123";

        let result = NvidiaDetector::parse_gpu_info_csv(csv);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid GPU index"));
    }

    #[test]
    fn test_parse_invalid_memory() {
        let csv = "0, GPU Name, not_memory MiB, GPU-abc123";

        let result = NvidiaDetector::parse_gpu_info_csv(csv);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid memory value"));
    }

    #[test]
    fn test_fake_detector_no_gpus() {
        let detector = FakeGpuDetector::new();

        let gpus = detector.detect_gpus().expect("should work");
        assert!(gpus.is_empty());

        let metrics = detector.collect_metrics().expect("should work");
        assert!(metrics.is_empty());
    }

    #[test]
    fn test_fake_detector_with_gpus() {
        let detector = FakeGpuDetector::new()
            .with_gpu(
                GpuInfo {
                    index: 0,
                    name: "Fake RTX 4090".to_string(),
                    memory_total_mib: 24576,
                    uuid: "GPU-fake-0".to_string(),
                },
                GpuMetrics {
                    index: 0,
                    utilization_percent: 42,
                    memory_used_mib: 10000,
                    memory_total_mib: 24576,
                    temperature_celsius: 60,
                    power_watts: Some(250.0),
                },
            )
            .with_gpu(
                GpuInfo {
                    index: 1,
                    name: "Fake A100".to_string(),
                    memory_total_mib: 81920,
                    uuid: "GPU-fake-1".to_string(),
                },
                GpuMetrics {
                    index: 1,
                    utilization_percent: 95,
                    memory_used_mib: 70000,
                    memory_total_mib: 81920,
                    temperature_celsius: 75,
                    power_watts: Some(400.0),
                },
            );

        let gpus = detector.detect_gpus().expect("should work");
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].name, "Fake RTX 4090");
        assert_eq!(gpus[1].name, "Fake A100");

        let metrics = detector.collect_metrics().expect("should work");
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics[0].utilization_percent, 42);
        assert_eq!(metrics[1].utilization_percent, 95);
    }

    #[test]
    fn test_fake_detector_specific_gpu() {
        let detector = FakeGpuDetector::new().with_gpu(
            GpuInfo {
                index: 0,
                name: "Test GPU".to_string(),
                memory_total_mib: 16384,
                uuid: "GPU-test".to_string(),
            },
            GpuMetrics {
                index: 0,
                utilization_percent: 50,
                memory_used_mib: 8000,
                memory_total_mib: 16384,
                temperature_celsius: 65,
                power_watts: None,
            },
        );

        let m = detector.collect_metrics_for_gpu(0).expect("should find GPU 0");
        assert_eq!(m.utilization_percent, 50);

        let err = detector.collect_metrics_for_gpu(99);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_nvidia_detector_default() {
        let detector = NvidiaDetector::new();
        assert_eq!(detector.nvidia_smi_path(), "nvidia-smi");
    }

    #[test]
    fn test_nvidia_detector_custom_path() {
        let detector = NvidiaDetector::with_path("/usr/local/bin/nvidia-smi");
        assert_eq!(detector.nvidia_smi_path(), "/usr/local/bin/nvidia-smi");
    }

    #[test]
    fn test_gpu_info_serialization() {
        let gpu = GpuInfo {
            index: 0,
            name: "RTX 4090".to_string(),
            memory_total_mib: 24576,
            uuid: "GPU-123".to_string(),
        };

        let json = serde_json::to_string(&gpu).expect("should serialize");
        let parsed: GpuInfo = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(gpu, parsed);
    }

    #[test]
    fn test_gpu_metrics_serialization() {
        let metrics = GpuMetrics {
            index: 0,
            utilization_percent: 75,
            memory_used_mib: 12000,
            memory_total_mib: 24576,
            temperature_celsius: 68,
            power_watts: Some(320.5),
        };

        let json = serde_json::to_string(&metrics).expect("should serialize");
        let parsed: GpuMetrics = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(metrics, parsed);
    }

    #[test]
    fn test_parse_multiple_gpu_metrics() {
        let csv = r"0, 75 %, 12000 MiB, 24564 MiB, 68, 320.5 W
1, 95 %, 70000 MiB, 81920 MiB, 72, 400.0 W";

        let metrics = NvidiaDetector::parse_gpu_metrics_csv(csv).expect("should parse");
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics[0].utilization_percent, 75);
        assert_eq!(metrics[1].utilization_percent, 95);
        assert_eq!(metrics[1].memory_used_mib, 70000);
    }
}
