//! Namespace types for logical isolation.

use std::collections::HashMap;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Result, TenancyError};
use crate::quota::{QuotaUsage, ResourceQuota};

/// Unique identifier for a namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NamespaceId(Uuid);

impl NamespaceId {
    /// Create a new random `NamespaceId`.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a `NamespaceId` from a UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Parse a `NamespaceId` from a string.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid UUID.
    pub fn parse(s: &str) -> Result<Self> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|e| TenancyError::InvalidNamespaceName(format!("invalid UUID: {e}")))
    }

    /// Get the underlying UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for NamespaceId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for NamespaceId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl fmt::Display for NamespaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Minimum length for namespace names.
pub const MIN_NAMESPACE_NAME_LENGTH: usize = 1;

/// Maximum length for namespace names.
pub const MAX_NAMESPACE_NAME_LENGTH: usize = 63;

/// Validate a namespace name.
///
/// Namespace names must:
/// - Be 1-63 characters long
/// - Start with a lowercase letter
/// - Contain only lowercase letters, numbers, and hyphens
/// - Not end with a hyphen
///
/// # Errors
///
/// Returns an error if the name is invalid.
pub fn validate_namespace_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(TenancyError::InvalidNamespaceName(
            "name cannot be empty".to_string(),
        ));
    }

    if name.len() > MAX_NAMESPACE_NAME_LENGTH {
        return Err(TenancyError::InvalidNamespaceName(format!(
            "name too long: {} > {}",
            name.len(),
            MAX_NAMESPACE_NAME_LENGTH
        )));
    }

    let mut chars = name.chars();

    // First character must be a lowercase letter
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        Some(c) => {
            return Err(TenancyError::InvalidNamespaceName(format!(
                "must start with lowercase letter, got '{c}'"
            )));
        }
        None => {
            return Err(TenancyError::InvalidNamespaceName(
                "name cannot be empty".to_string(),
            ));
        }
    }

    // Remaining characters must be lowercase letters, numbers, or hyphens
    for c in chars {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' {
            return Err(TenancyError::InvalidNamespaceName(format!(
                "invalid character '{c}': only lowercase letters, numbers, and hyphens allowed"
            )));
        }
    }

    // Cannot end with a hyphen
    if name.ends_with('-') {
        return Err(TenancyError::InvalidNamespaceName(
            "cannot end with a hyphen".to_string(),
        ));
    }

    Ok(())
}

/// A namespace provides logical isolation for workloads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Namespace {
    /// Unique namespace identifier.
    pub id: NamespaceId,
    /// Human-readable namespace name (unique within tenant).
    pub name: String,
    /// Tenant that owns this namespace.
    pub tenant_id: Uuid,
    /// Labels for organization and selection.
    pub labels: HashMap<String, String>,
    /// Resource quota limits for this namespace.
    pub quota: ResourceQuota,
    /// Current resource usage.
    pub usage: QuotaUsage,
    /// When the namespace was created.
    pub created_at: DateTime<Utc>,
    /// When the namespace was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optional description.
    pub description: Option<String>,
    /// Number of active workloads in this namespace.
    pub active_workloads: u32,
}

impl Namespace {
    /// Create a new namespace with the given name and tenant.
    ///
    /// # Errors
    ///
    /// Returns an error if the name is invalid.
    pub fn new(name: impl Into<String>, tenant_id: Uuid) -> Result<Self> {
        let name = name.into();
        validate_namespace_name(&name)?;

        let now = Utc::now();
        Ok(Self {
            id: NamespaceId::new(),
            name,
            tenant_id,
            labels: HashMap::new(),
            quota: ResourceQuota::default(),
            usage: QuotaUsage::default(),
            created_at: now,
            updated_at: now,
            description: None,
            active_workloads: 0,
        })
    }

    /// Create a namespace with a specific ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the name is invalid.
    pub fn with_id(id: NamespaceId, name: impl Into<String>, tenant_id: Uuid) -> Result<Self> {
        let name = name.into();
        validate_namespace_name(&name)?;

        let now = Utc::now();
        Ok(Self {
            id,
            name,
            tenant_id,
            labels: HashMap::new(),
            quota: ResourceQuota::default(),
            usage: QuotaUsage::default(),
            created_at: now,
            updated_at: now,
            description: None,
            active_workloads: 0,
        })
    }

    /// Set the resource quota.
    #[must_use]
    pub fn with_quota(mut self, quota: ResourceQuota) -> Self {
        self.quota = quota;
        self
    }

    /// Add a label.
    #[must_use]
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Set the description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Check if a workload can be added without exceeding quota.
    ///
    /// # Errors
    ///
    /// Returns an error describing which quota would be exceeded.
    pub fn can_add_workload(&self, gpu_count: u32, memory_mib: u64) -> Result<()> {
        // Check workload count
        if let Some(max) = self.quota.max_workloads {
            if self.active_workloads >= max {
                return Err(TenancyError::QuotaExceeded {
                    resource: "workloads".to_string(),
                    used: u64::from(self.active_workloads),
                    limit: u64::from(max),
                });
            }
        }

        // Check GPU count
        if let Some(max) = self.quota.max_gpus {
            let new_gpus = self.usage.gpus_in_use.saturating_add(gpu_count);
            if new_gpus > max {
                return Err(TenancyError::QuotaExceeded {
                    resource: "gpus".to_string(),
                    used: u64::from(new_gpus),
                    limit: u64::from(max),
                });
            }
        }

        // Check memory
        if let Some(max) = self.quota.memory_mib {
            let new_memory = self.usage.memory_mib_used.saturating_add(memory_mib);
            if new_memory > max {
                return Err(TenancyError::QuotaExceeded {
                    resource: "memory_mib".to_string(),
                    used: new_memory,
                    limit: max,
                });
            }
        }

        Ok(())
    }

    /// Record that a workload was added.
    pub fn record_workload_added(&mut self, gpu_count: u32, memory_mib: u64) {
        self.active_workloads = self.active_workloads.saturating_add(1);
        self.usage.gpus_in_use = self.usage.gpus_in_use.saturating_add(gpu_count);
        self.usage.memory_mib_used = self.usage.memory_mib_used.saturating_add(memory_mib);
        self.updated_at = Utc::now();
    }

    /// Record that a workload was removed.
    pub fn record_workload_removed(&mut self, gpu_count: u32, memory_mib: u64) {
        self.active_workloads = self.active_workloads.saturating_sub(1);
        self.usage.gpus_in_use = self.usage.gpus_in_use.saturating_sub(gpu_count);
        self.usage.memory_mib_used = self.usage.memory_mib_used.saturating_sub(memory_mib);
        self.updated_at = Utc::now();
    }

    /// Record GPU hours used.
    pub fn record_gpu_hours(&mut self, hours: f64) {
        self.usage.gpu_hours_used += hours;
        self.updated_at = Utc::now();
    }

    /// Check if GPU hours quota is exceeded.
    #[must_use]
    pub fn is_gpu_hours_exceeded(&self) -> bool {
        self.quota
            .gpu_hours
            .is_some_and(|max| self.usage.gpu_hours_used >= max)
    }

    /// Get remaining GPU hours, if quota is set.
    #[must_use]
    pub fn remaining_gpu_hours(&self) -> Option<f64> {
        self.quota.gpu_hours.map(|max| {
            let remaining = max - self.usage.gpu_hours_used;
            if remaining < 0.0 { 0.0 } else { remaining }
        })
    }

    /// Check if the namespace has any active workloads.
    #[must_use]
    pub const fn has_active_workloads(&self) -> bool {
        self.active_workloads > 0
    }

    /// Check if a label matches.
    #[must_use]
    pub fn matches_label(&self, key: &str, value: &str) -> bool {
        self.labels.get(key).is_some_and(|v| v == value)
    }

    /// Check if all selector labels match.
    #[must_use]
    pub fn matches_selector(&self, selector: &HashMap<String, String>) -> bool {
        selector
            .iter()
            .all(|(k, v)| self.labels.get(k).is_some_and(|label_v| label_v == v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== NamespaceId Tests ====================

    #[test]
    fn test_namespace_id_new() {
        let id = NamespaceId::new();
        assert_eq!(id.as_uuid().get_version_num(), 4);
    }

    #[test]
    fn test_namespace_id_parse_valid() {
        let uuid = Uuid::new_v4();
        let id = NamespaceId::parse(&uuid.to_string());
        assert!(id.is_ok());
        assert_eq!(id.unwrap_or_default().as_uuid(), uuid);
    }

    #[test]
    fn test_namespace_id_parse_invalid() {
        let result = NamespaceId::parse("not-a-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn test_namespace_id_display() {
        let uuid = Uuid::new_v4();
        let id = NamespaceId::from_uuid(uuid);
        assert_eq!(id.to_string(), uuid.to_string());
    }

    // ==================== Namespace Name Validation Tests ====================

    #[test]
    fn test_validate_namespace_name_valid() {
        assert!(validate_namespace_name("default").is_ok());
        assert!(validate_namespace_name("my-namespace").is_ok());
        assert!(validate_namespace_name("team1").is_ok());
        assert!(validate_namespace_name("prod-us-east-1").is_ok());
        assert!(validate_namespace_name("a").is_ok());
        assert!(validate_namespace_name("abc123").is_ok());
    }

    #[test]
    fn test_validate_namespace_name_empty() {
        let result = validate_namespace_name("");
        assert!(matches!(result, Err(TenancyError::InvalidNamespaceName(_))));
    }

    #[test]
    fn test_validate_namespace_name_too_long() {
        let name = "a".repeat(64);
        let result = validate_namespace_name(&name);
        assert!(matches!(result, Err(TenancyError::InvalidNamespaceName(_))));
    }

    #[test]
    fn test_validate_namespace_name_starts_with_number() {
        let result = validate_namespace_name("1namespace");
        assert!(matches!(result, Err(TenancyError::InvalidNamespaceName(_))));
    }

    #[test]
    fn test_validate_namespace_name_starts_with_hyphen() {
        let result = validate_namespace_name("-namespace");
        assert!(matches!(result, Err(TenancyError::InvalidNamespaceName(_))));
    }

    #[test]
    fn test_validate_namespace_name_ends_with_hyphen() {
        let result = validate_namespace_name("namespace-");
        assert!(matches!(result, Err(TenancyError::InvalidNamespaceName(_))));
    }

    #[test]
    fn test_validate_namespace_name_uppercase() {
        let result = validate_namespace_name("Namespace");
        assert!(matches!(result, Err(TenancyError::InvalidNamespaceName(_))));
    }

    #[test]
    fn test_validate_namespace_name_special_chars() {
        assert!(validate_namespace_name("namespace_test").is_err());
        assert!(validate_namespace_name("namespace.test").is_err());
        assert!(validate_namespace_name("namespace/test").is_err());
    }

    // ==================== Namespace Tests ====================

    #[test]
    fn test_namespace_new() {
        let tenant_id = Uuid::new_v4();
        let ns = Namespace::new("default", tenant_id);
        assert!(ns.is_ok());

        let ns = ns.unwrap_or_else(|_| unreachable!());
        assert_eq!(ns.name, "default");
        assert_eq!(ns.tenant_id, tenant_id);
        assert!(ns.labels.is_empty());
        assert_eq!(ns.active_workloads, 0);
    }

    #[test]
    fn test_namespace_new_invalid_name() {
        let tenant_id = Uuid::new_v4();
        let result = Namespace::new("Invalid", tenant_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_namespace_with_quota() {
        let tenant_id = Uuid::new_v4();
        let quota = ResourceQuota::new()
            .with_max_workloads(10)
            .with_max_gpus(4);

        let ns = Namespace::new("test", tenant_id)
            .map(|n| n.with_quota(quota.clone()));

        assert!(ns.is_ok());
        let ns = ns.unwrap_or_else(|_| unreachable!());
        assert_eq!(ns.quota.max_workloads, Some(10));
        assert_eq!(ns.quota.max_gpus, Some(4));
    }

    #[test]
    fn test_namespace_with_labels() {
        let tenant_id = Uuid::new_v4();
        let ns = Namespace::new("test", tenant_id)
            .map(|n| n.with_label("env", "production").with_label("team", "ml"));

        assert!(ns.is_ok());
        let ns = ns.unwrap_or_else(|_| unreachable!());
        assert_eq!(ns.labels.get("env"), Some(&"production".to_string()));
        assert_eq!(ns.labels.get("team"), Some(&"ml".to_string()));
    }

    #[test]
    fn test_namespace_can_add_workload_no_quota() {
        let tenant_id = Uuid::new_v4();
        let ns = Namespace::new("test", tenant_id);
        assert!(ns.is_ok());

        let ns = ns.unwrap_or_else(|_| unreachable!());
        assert!(ns.can_add_workload(4, 8192).is_ok());
    }

    #[test]
    fn test_namespace_can_add_workload_within_quota() {
        let tenant_id = Uuid::new_v4();
        let quota = ResourceQuota::new()
            .with_max_workloads(10)
            .with_max_gpus(8)
            .with_memory_mib(16384);

        let ns = Namespace::new("test", tenant_id)
            .map(|n| n.with_quota(quota));

        assert!(ns.is_ok());
        let ns = ns.unwrap_or_else(|_| unreachable!());
        assert!(ns.can_add_workload(4, 8192).is_ok());
    }

    #[test]
    fn test_namespace_can_add_workload_exceeds_workload_quota() {
        let tenant_id = Uuid::new_v4();
        let quota = ResourceQuota::new().with_max_workloads(0);

        let ns = Namespace::new("test", tenant_id)
            .map(|n| n.with_quota(quota));

        assert!(ns.is_ok());
        let ns = ns.unwrap_or_else(|_| unreachable!());
        let result = ns.can_add_workload(0, 0);
        assert!(matches!(result, Err(TenancyError::QuotaExceeded { resource, .. }) if resource == "workloads"));
    }

    #[test]
    fn test_namespace_can_add_workload_exceeds_gpu_quota() {
        let tenant_id = Uuid::new_v4();
        let quota = ResourceQuota::new()
            .with_max_workloads(10)
            .with_max_gpus(2);

        let ns = Namespace::new("test", tenant_id)
            .map(|n| n.with_quota(quota));

        assert!(ns.is_ok());
        let ns = ns.unwrap_or_else(|_| unreachable!());
        let result = ns.can_add_workload(4, 1024);
        assert!(matches!(result, Err(TenancyError::QuotaExceeded { resource, .. }) if resource == "gpus"));
    }

    #[test]
    fn test_namespace_can_add_workload_exceeds_memory_quota() {
        let tenant_id = Uuid::new_v4();
        let quota = ResourceQuota::new()
            .with_max_workloads(10)
            .with_memory_mib(1024);

        let ns = Namespace::new("test", tenant_id)
            .map(|n| n.with_quota(quota));

        assert!(ns.is_ok());
        let ns = ns.unwrap_or_else(|_| unreachable!());
        let result = ns.can_add_workload(0, 2048);
        assert!(matches!(result, Err(TenancyError::QuotaExceeded { resource, .. }) if resource == "memory_mib"));
    }

    #[test]
    fn test_namespace_record_workload_added() {
        let tenant_id = Uuid::new_v4();
        let ns = Namespace::new("test", tenant_id);
        assert!(ns.is_ok());

        let mut ns = ns.unwrap_or_else(|_| unreachable!());
        ns.record_workload_added(2, 4096);

        assert_eq!(ns.active_workloads, 1);
        assert_eq!(ns.usage.gpus_in_use, 2);
        assert_eq!(ns.usage.memory_mib_used, 4096);
    }

    #[test]
    fn test_namespace_record_workload_removed() {
        let tenant_id = Uuid::new_v4();
        let ns = Namespace::new("test", tenant_id);
        assert!(ns.is_ok());

        let mut ns = ns.unwrap_or_else(|_| unreachable!());
        ns.record_workload_added(4, 8192);
        ns.record_workload_removed(2, 4096);

        assert_eq!(ns.active_workloads, 0);
        assert_eq!(ns.usage.gpus_in_use, 2);
        assert_eq!(ns.usage.memory_mib_used, 4096);
    }

    #[test]
    fn test_namespace_gpu_hours_tracking() {
        let tenant_id = Uuid::new_v4();
        let quota = ResourceQuota::new().with_gpu_hours(100.0);
        let ns = Namespace::new("test", tenant_id)
            .map(|n| n.with_quota(quota));

        assert!(ns.is_ok());
        let mut ns = ns.unwrap_or_else(|_| unreachable!());

        assert!(!ns.is_gpu_hours_exceeded());
        assert_eq!(ns.remaining_gpu_hours(), Some(100.0));

        ns.record_gpu_hours(50.0);
        assert!(!ns.is_gpu_hours_exceeded());
        assert_eq!(ns.remaining_gpu_hours(), Some(50.0));

        ns.record_gpu_hours(60.0);
        assert!(ns.is_gpu_hours_exceeded());
        assert_eq!(ns.remaining_gpu_hours(), Some(0.0));
    }

    #[test]
    fn test_namespace_matches_selector() {
        let tenant_id = Uuid::new_v4();
        let ns = Namespace::new("test", tenant_id)
            .map(|n| n.with_label("env", "prod").with_label("team", "ml"));

        assert!(ns.is_ok());
        let ns = ns.unwrap_or_else(|_| unreachable!());

        // Match all labels
        let mut selector = HashMap::new();
        selector.insert("env".to_string(), "prod".to_string());
        assert!(ns.matches_selector(&selector));

        // Match multiple labels
        selector.insert("team".to_string(), "ml".to_string());
        assert!(ns.matches_selector(&selector));

        // No match
        selector.insert("region".to_string(), "us-east".to_string());
        assert!(!ns.matches_selector(&selector));

        // Empty selector matches everything
        assert!(ns.matches_selector(&HashMap::new()));
    }

    #[test]
    fn test_namespace_serialization() {
        let tenant_id = Uuid::new_v4();
        let ns = Namespace::new("test", tenant_id)
            .map(|n| n.with_label("env", "prod"));

        assert!(ns.is_ok());
        let ns = ns.unwrap_or_else(|_| unreachable!());

        let json = serde_json::to_string(&ns);
        assert!(json.is_ok());

        let json = json.unwrap_or_default();
        let deserialized: Result<Namespace> = serde_json::from_str(&json)
            .map_err(|e| TenancyError::InvalidNamespaceName(e.to_string()));
        assert!(deserialized.is_ok());

        let deserialized = deserialized.unwrap_or_else(|_| unreachable!());
        assert_eq!(ns.id, deserialized.id);
        assert_eq!(ns.name, deserialized.name);
    }
}
