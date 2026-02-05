//! Resource quota and usage tracking.

use serde::{Deserialize, Serialize};

use crate::error::{Result, TenancyError};

/// Resource quota limits for a namespace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ResourceQuota {
    /// Maximum GPU hours allowed (billing period).
    pub gpu_hours: Option<f64>,
    /// Maximum memory in MiB.
    pub memory_mib: Option<u64>,
    /// Maximum number of concurrent workloads.
    pub max_workloads: Option<u32>,
    /// Maximum number of GPUs that can be used concurrently.
    pub max_gpus: Option<u32>,
    /// Maximum CPU cores.
    pub max_cpu_cores: Option<u32>,
    /// Maximum storage in MiB.
    pub storage_mib: Option<u64>,
}

impl ResourceQuota {
    /// Create a new empty quota (no limits).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            gpu_hours: None,
            memory_mib: None,
            max_workloads: None,
            max_gpus: None,
            max_cpu_cores: None,
            storage_mib: None,
        }
    }

    /// Set GPU hours limit.
    #[must_use]
    pub const fn with_gpu_hours(mut self, hours: f64) -> Self {
        self.gpu_hours = Some(hours);
        self
    }

    /// Set memory limit in MiB.
    #[must_use]
    pub const fn with_memory_mib(mut self, mib: u64) -> Self {
        self.memory_mib = Some(mib);
        self
    }

    /// Set maximum workloads.
    #[must_use]
    pub const fn with_max_workloads(mut self, count: u32) -> Self {
        self.max_workloads = Some(count);
        self
    }

    /// Set maximum GPUs.
    #[must_use]
    pub const fn with_max_gpus(mut self, count: u32) -> Self {
        self.max_gpus = Some(count);
        self
    }

    /// Set maximum CPU cores.
    #[must_use]
    pub const fn with_max_cpu_cores(mut self, count: u32) -> Self {
        self.max_cpu_cores = Some(count);
        self
    }

    /// Set storage limit in MiB.
    #[must_use]
    pub const fn with_storage_mib(mut self, mib: u64) -> Self {
        self.storage_mib = Some(mib);
        self
    }

    /// Validate the quota configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if any quota value is invalid.
    pub fn validate(&self) -> Result<()> {
        if let Some(hours) = self.gpu_hours {
            if hours < 0.0 {
                return Err(TenancyError::InvalidQuota(
                    "gpu_hours cannot be negative".to_string(),
                ));
            }
            if hours.is_nan() {
                return Err(TenancyError::InvalidQuota(
                    "gpu_hours cannot be NaN".to_string(),
                ));
            }
            if hours.is_infinite() {
                return Err(TenancyError::InvalidQuota(
                    "gpu_hours cannot be infinite".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Check if any quota is set.
    #[must_use]
    pub const fn has_any_limit(&self) -> bool {
        self.gpu_hours.is_some()
            || self.memory_mib.is_some()
            || self.max_workloads.is_some()
            || self.max_gpus.is_some()
            || self.max_cpu_cores.is_some()
            || self.storage_mib.is_some()
    }

    /// Merge with another quota, taking the minimum of each limit.
    #[must_use]
    pub fn merge_min(&self, other: &Self) -> Self {
        Self {
            gpu_hours: merge_option_min_f64(self.gpu_hours, other.gpu_hours),
            memory_mib: merge_option_min(self.memory_mib, other.memory_mib),
            max_workloads: merge_option_min_u32(self.max_workloads, other.max_workloads),
            max_gpus: merge_option_min_u32(self.max_gpus, other.max_gpus),
            max_cpu_cores: merge_option_min_u32(self.max_cpu_cores, other.max_cpu_cores),
            storage_mib: merge_option_min(self.storage_mib, other.storage_mib),
        }
    }
}

fn merge_option_min(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

fn merge_option_min_u32(a: Option<u32>, b: Option<u32>) -> Option<u32> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

fn merge_option_min_f64(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

/// Current resource usage for a namespace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct QuotaUsage {
    /// GPU hours consumed in current billing period.
    pub gpu_hours_used: f64,
    /// Memory currently in use (MiB).
    pub memory_mib_used: u64,
    /// Number of GPUs currently in use.
    pub gpus_in_use: u32,
    /// Number of CPU cores currently in use.
    pub cpu_cores_in_use: u32,
    /// Storage currently in use (MiB).
    pub storage_mib_used: u64,
}

impl QuotaUsage {
    /// Create a new empty usage tracker.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            gpu_hours_used: 0.0,
            memory_mib_used: 0,
            gpus_in_use: 0,
            cpu_cores_in_use: 0,
            storage_mib_used: 0,
        }
    }

    /// Reset all usage counters.
    pub fn reset(&mut self) {
        self.gpu_hours_used = 0.0;
        self.memory_mib_used = 0;
        self.gpus_in_use = 0;
        self.cpu_cores_in_use = 0;
        self.storage_mib_used = 0;
    }

    /// Reset only the billing period counters (GPU hours).
    pub fn reset_billing_period(&mut self) {
        self.gpu_hours_used = 0.0;
    }

    /// Calculate utilization percentage for each resource.
    #[must_use]
    pub fn utilization(&self, quota: &ResourceQuota) -> QuotaUtilization {
        QuotaUtilization {
            gpu_hours_percent: quota
                .gpu_hours
                .map(|max| calculate_percent(self.gpu_hours_used, max)),
            memory_percent: quota
                .memory_mib
                .map(|max| calculate_percent_u64(self.memory_mib_used, max)),
            gpus_percent: quota
                .max_gpus
                .map(|max| calculate_percent_u32(self.gpus_in_use, max)),
            cpu_cores_percent: quota
                .max_cpu_cores
                .map(|max| calculate_percent_u32(self.cpu_cores_in_use, max)),
            storage_percent: quota
                .storage_mib
                .map(|max| calculate_percent_u64(self.storage_mib_used, max)),
        }
    }
}

fn calculate_percent(used: f64, max: f64) -> f64 {
    if max <= 0.0 {
        return 0.0;
    }
    (used / max) * 100.0
}

fn calculate_percent_u64(used: u64, max: u64) -> f64 {
    if max == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let percent = (used as f64 / max as f64) * 100.0;
    percent
}

fn calculate_percent_u32(used: u32, max: u32) -> f64 {
    if max == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let percent = (f64::from(used) / f64::from(max)) * 100.0;
    percent
}

/// Utilization percentages for quotas.
#[derive(Debug, Clone, PartialEq)]
pub struct QuotaUtilization {
    /// GPU hours utilization percentage.
    pub gpu_hours_percent: Option<f64>,
    /// Memory utilization percentage.
    pub memory_percent: Option<f64>,
    /// GPU count utilization percentage.
    pub gpus_percent: Option<f64>,
    /// CPU cores utilization percentage.
    pub cpu_cores_percent: Option<f64>,
    /// Storage utilization percentage.
    pub storage_percent: Option<f64>,
}

impl QuotaUtilization {
    /// Check if any resource is over a threshold percentage.
    #[must_use]
    pub fn any_over_threshold(&self, threshold: f64) -> bool {
        self.gpu_hours_percent.is_some_and(|p| p >= threshold)
            || self.memory_percent.is_some_and(|p| p >= threshold)
            || self.gpus_percent.is_some_and(|p| p >= threshold)
            || self.cpu_cores_percent.is_some_and(|p| p >= threshold)
            || self.storage_percent.is_some_and(|p| p >= threshold)
    }

    /// Get the highest utilization percentage.
    #[must_use]
    pub fn max_utilization(&self) -> Option<f64> {
        [
            self.gpu_hours_percent,
            self.memory_percent,
            self.gpus_percent,
            self.cpu_cores_percent,
            self.storage_percent,
        ]
        .into_iter()
        .flatten()
        .reduce(f64::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== ResourceQuota Tests ====================

    #[test]
    fn test_resource_quota_new() {
        let quota = ResourceQuota::new();
        assert!(quota.gpu_hours.is_none());
        assert!(quota.memory_mib.is_none());
        assert!(quota.max_workloads.is_none());
        assert!(quota.max_gpus.is_none());
    }

    #[test]
    fn test_resource_quota_builder() {
        let quota = ResourceQuota::new()
            .with_gpu_hours(100.0)
            .with_memory_mib(16384)
            .with_max_workloads(10)
            .with_max_gpus(4)
            .with_max_cpu_cores(16)
            .with_storage_mib(102400);

        assert_eq!(quota.gpu_hours, Some(100.0));
        assert_eq!(quota.memory_mib, Some(16384));
        assert_eq!(quota.max_workloads, Some(10));
        assert_eq!(quota.max_gpus, Some(4));
        assert_eq!(quota.max_cpu_cores, Some(16));
        assert_eq!(quota.storage_mib, Some(102400));
    }

    #[test]
    fn test_resource_quota_validate_valid() {
        let quota = ResourceQuota::new()
            .with_gpu_hours(100.0)
            .with_max_workloads(10);

        assert!(quota.validate().is_ok());
    }

    #[test]
    fn test_resource_quota_validate_negative_gpu_hours() {
        let quota = ResourceQuota::new().with_gpu_hours(-1.0);
        let result = quota.validate();
        assert!(matches!(result, Err(TenancyError::InvalidQuota(_))));
    }

    #[test]
    fn test_resource_quota_validate_nan_gpu_hours() {
        let quota = ResourceQuota::new().with_gpu_hours(f64::NAN);
        let result = quota.validate();
        assert!(matches!(result, Err(TenancyError::InvalidQuota(_))));
    }

    #[test]
    fn test_resource_quota_validate_infinite_gpu_hours() {
        let quota = ResourceQuota::new().with_gpu_hours(f64::INFINITY);
        let result = quota.validate();
        assert!(matches!(result, Err(TenancyError::InvalidQuota(_))));
    }

    #[test]
    fn test_resource_quota_has_any_limit() {
        let empty = ResourceQuota::new();
        assert!(!empty.has_any_limit());

        let with_limit = ResourceQuota::new().with_max_gpus(4);
        assert!(with_limit.has_any_limit());
    }

    #[test]
    fn test_resource_quota_merge_min() {
        let quota1 = ResourceQuota::new()
            .with_gpu_hours(100.0)
            .with_max_gpus(8)
            .with_memory_mib(32768);

        let quota2 = ResourceQuota::new()
            .with_gpu_hours(50.0)
            .with_max_gpus(4);

        let merged = quota1.merge_min(&quota2);

        assert_eq!(merged.gpu_hours, Some(50.0));
        assert_eq!(merged.max_gpus, Some(4));
        assert_eq!(merged.memory_mib, Some(32768)); // Only in quota1
    }

    #[test]
    fn test_resource_quota_serialization() {
        let quota = ResourceQuota::new()
            .with_gpu_hours(100.0)
            .with_max_workloads(10);

        let json = serde_json::to_string(&quota);
        assert!(json.is_ok());

        let json = json.unwrap_or_default();
        let deserialized: std::result::Result<ResourceQuota, _> = serde_json::from_str(&json);
        assert!(deserialized.is_ok());
        assert_eq!(quota, deserialized.unwrap_or_default());
    }

    // ==================== QuotaUsage Tests ====================

    #[test]
    fn test_quota_usage_new() {
        let usage = QuotaUsage::new();
        assert_eq!(usage.gpu_hours_used, 0.0);
        assert_eq!(usage.memory_mib_used, 0);
        assert_eq!(usage.gpus_in_use, 0);
    }

    #[test]
    fn test_quota_usage_reset() {
        let mut usage = QuotaUsage {
            gpu_hours_used: 50.0,
            memory_mib_used: 8192,
            gpus_in_use: 4,
            cpu_cores_in_use: 8,
            storage_mib_used: 1024,
        };

        usage.reset();

        assert_eq!(usage.gpu_hours_used, 0.0);
        assert_eq!(usage.memory_mib_used, 0);
        assert_eq!(usage.gpus_in_use, 0);
    }

    #[test]
    fn test_quota_usage_reset_billing_period() {
        let mut usage = QuotaUsage {
            gpu_hours_used: 50.0,
            memory_mib_used: 8192,
            gpus_in_use: 4,
            cpu_cores_in_use: 8,
            storage_mib_used: 1024,
        };

        usage.reset_billing_period();

        assert_eq!(usage.gpu_hours_used, 0.0);
        assert_eq!(usage.memory_mib_used, 8192); // Unchanged
        assert_eq!(usage.gpus_in_use, 4); // Unchanged
    }

    #[test]
    fn test_quota_usage_utilization() {
        let usage = QuotaUsage {
            gpu_hours_used: 50.0,
            memory_mib_used: 8192,
            gpus_in_use: 2,
            cpu_cores_in_use: 4,
            storage_mib_used: 512,
        };

        let quota = ResourceQuota::new()
            .with_gpu_hours(100.0)
            .with_memory_mib(16384)
            .with_max_gpus(4);

        let util = usage.utilization(&quota);

        assert_eq!(util.gpu_hours_percent, Some(50.0));
        assert_eq!(util.memory_percent, Some(50.0));
        assert_eq!(util.gpus_percent, Some(50.0));
        assert!(util.cpu_cores_percent.is_none()); // No limit set
    }

    #[test]
    fn test_quota_usage_utilization_zero_quota() {
        let usage = QuotaUsage {
            gpu_hours_used: 50.0,
            memory_mib_used: 0,
            gpus_in_use: 0,
            cpu_cores_in_use: 0,
            storage_mib_used: 0,
        };

        let quota = ResourceQuota::new()
            .with_gpu_hours(0.0)
            .with_max_gpus(0);

        let util = usage.utilization(&quota);

        // Should handle division by zero gracefully
        assert_eq!(util.gpu_hours_percent, Some(0.0));
        assert_eq!(util.gpus_percent, Some(0.0));
    }

    // ==================== QuotaUtilization Tests ====================

    #[test]
    fn test_quota_utilization_any_over_threshold() {
        let util = QuotaUtilization {
            gpu_hours_percent: Some(50.0),
            memory_percent: Some(85.0),
            gpus_percent: Some(25.0),
            cpu_cores_percent: None,
            storage_percent: None,
        };

        assert!(util.any_over_threshold(80.0));
        assert!(!util.any_over_threshold(90.0));
    }

    #[test]
    fn test_quota_utilization_max_utilization() {
        let util = QuotaUtilization {
            gpu_hours_percent: Some(50.0),
            memory_percent: Some(85.0),
            gpus_percent: Some(25.0),
            cpu_cores_percent: None,
            storage_percent: Some(90.0),
        };

        assert_eq!(util.max_utilization(), Some(90.0));
    }

    #[test]
    fn test_quota_utilization_max_utilization_none() {
        let util = QuotaUtilization {
            gpu_hours_percent: None,
            memory_percent: None,
            gpus_percent: None,
            cpu_cores_percent: None,
            storage_percent: None,
        };

        assert!(util.max_utilization().is_none());
    }
}
