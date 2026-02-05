//! Tenant types for multi-tenancy.

use std::collections::HashMap;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Result, TenancyError};
use crate::namespace::NamespaceId;
use crate::quota::ResourceQuota;

/// Unique identifier for a tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TenantId(Uuid);

impl TenantId {
    /// Create a new random `TenantId`.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a `TenantId` from a UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Parse a `TenantId` from a string.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid UUID.
    pub fn parse(s: &str) -> Result<Self> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|e| TenancyError::InvalidTenantName(format!("invalid UUID: {e}")))
    }

    /// Get the underlying UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for TenantId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for TenantId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl fmt::Display for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Minimum length for tenant names.
pub const MIN_TENANT_NAME_LENGTH: usize = 1;

/// Maximum length for tenant names.
pub const MAX_TENANT_NAME_LENGTH: usize = 128;

/// Validate a tenant name.
///
/// Tenant names must:
/// - Be 1-128 characters long
/// - Start with a letter (uppercase or lowercase)
/// - Contain only letters, numbers, hyphens, and underscores
/// - Not end with a hyphen or underscore
///
/// # Errors
///
/// Returns an error if the name is invalid.
pub fn validate_tenant_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(TenancyError::InvalidTenantName(
            "name cannot be empty".to_string(),
        ));
    }

    if name.len() > MAX_TENANT_NAME_LENGTH {
        return Err(TenancyError::InvalidTenantName(format!(
            "name too long: {} > {}",
            name.len(),
            MAX_TENANT_NAME_LENGTH
        )));
    }

    let mut chars = name.chars();

    // First character must be a letter
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        Some(c) => {
            return Err(TenancyError::InvalidTenantName(format!(
                "must start with a letter, got '{c}'"
            )));
        }
        None => {
            return Err(TenancyError::InvalidTenantName(
                "name cannot be empty".to_string(),
            ));
        }
    }

    // Remaining characters must be letters, numbers, hyphens, or underscores
    for c in chars {
        if !c.is_ascii_alphanumeric() && c != '-' && c != '_' {
            return Err(TenancyError::InvalidTenantName(format!(
                "invalid character '{c}': only letters, numbers, hyphens, and underscores allowed"
            )));
        }
    }

    // Cannot end with a hyphen or underscore
    if name.ends_with('-') || name.ends_with('_') {
        return Err(TenancyError::InvalidTenantName(
            "cannot end with a hyphen or underscore".to_string(),
        ));
    }

    Ok(())
}

/// Billing plan for a tenant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BillingPlan {
    /// Free tier with limited resources.
    #[default]
    Free,
    /// Starter plan for small teams.
    Starter,
    /// Professional plan for growing teams.
    Professional,
    /// Enterprise plan with custom limits.
    Enterprise,
    /// Custom plan with negotiated terms.
    Custom,
}

impl fmt::Display for BillingPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Free => "free",
            Self::Starter => "starter",
            Self::Professional => "professional",
            Self::Enterprise => "enterprise",
            Self::Custom => "custom",
        };
        write!(f, "{s}")
    }
}

/// Billing information for a tenant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BillingInfo {
    /// Billing plan.
    pub plan: BillingPlan,
    /// External billing ID (e.g., Stripe customer ID).
    pub external_id: Option<String>,
    /// Email for billing notifications.
    pub billing_email: Option<String>,
    /// Start of current billing period.
    pub billing_period_start: Option<DateTime<Utc>>,
    /// End of current billing period.
    pub billing_period_end: Option<DateTime<Utc>>,
}

impl BillingInfo {
    /// Create new billing info with the free plan.
    #[must_use]
    pub const fn free() -> Self {
        Self {
            plan: BillingPlan::Free,
            external_id: None,
            billing_email: None,
            billing_period_start: None,
            billing_period_end: None,
        }
    }

    /// Set the billing plan.
    #[must_use]
    pub const fn with_plan(mut self, plan: BillingPlan) -> Self {
        self.plan = plan;
        self
    }

    /// Set the external billing ID.
    #[must_use]
    pub fn with_external_id(mut self, id: impl Into<String>) -> Self {
        self.external_id = Some(id.into());
        self
    }

    /// Set the billing email.
    #[must_use]
    pub fn with_billing_email(mut self, email: impl Into<String>) -> Self {
        self.billing_email = Some(email.into());
        self
    }

    /// Set the billing period.
    #[must_use]
    pub const fn with_billing_period(
        mut self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Self {
        self.billing_period_start = Some(start);
        self.billing_period_end = Some(end);
        self
    }

    /// Check if currently in a billing period.
    #[must_use]
    pub fn is_in_billing_period(&self) -> bool {
        let now = Utc::now();
        match (self.billing_period_start, self.billing_period_end) {
            (Some(start), Some(end)) => now >= start && now <= end,
            _ => false,
        }
    }
}

/// A tenant represents an organization or team with multiple namespaces.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tenant {
    /// Unique tenant identifier.
    pub id: TenantId,
    /// Human-readable tenant name (unique).
    pub name: String,
    /// Display name (can contain any characters).
    pub display_name: Option<String>,
    /// Namespaces owned by this tenant.
    pub namespaces: Vec<NamespaceId>,
    /// Default quota for new namespaces.
    pub default_namespace_quota: ResourceQuota,
    /// Tenant-level quota (aggregate across all namespaces).
    pub quota: ResourceQuota,
    /// Billing information.
    pub billing: BillingInfo,
    /// Labels for organization.
    pub labels: HashMap<String, String>,
    /// When the tenant was created.
    pub created_at: DateTime<Utc>,
    /// When the tenant was last updated.
    pub updated_at: DateTime<Utc>,
    /// Whether the tenant is active.
    pub active: bool,
}

impl Tenant {
    /// Create a new tenant with the given name.
    ///
    /// # Errors
    ///
    /// Returns an error if the name is invalid.
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        validate_tenant_name(&name)?;

        let now = Utc::now();
        Ok(Self {
            id: TenantId::new(),
            name,
            display_name: None,
            namespaces: Vec::new(),
            default_namespace_quota: ResourceQuota::default(),
            quota: ResourceQuota::default(),
            billing: BillingInfo::free(),
            labels: HashMap::new(),
            created_at: now,
            updated_at: now,
            active: true,
        })
    }

    /// Create a tenant with a specific ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the name is invalid.
    pub fn with_id(id: TenantId, name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        validate_tenant_name(&name)?;

        let now = Utc::now();
        Ok(Self {
            id,
            name,
            display_name: None,
            namespaces: Vec::new(),
            default_namespace_quota: ResourceQuota::default(),
            quota: ResourceQuota::default(),
            billing: BillingInfo::free(),
            labels: HashMap::new(),
            created_at: now,
            updated_at: now,
            active: true,
        })
    }

    /// Set the display name.
    #[must_use]
    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// Set the default namespace quota.
    #[must_use]
    pub fn with_default_namespace_quota(mut self, quota: ResourceQuota) -> Self {
        self.default_namespace_quota = quota;
        self
    }

    /// Set the tenant-level quota.
    #[must_use]
    pub fn with_quota(mut self, quota: ResourceQuota) -> Self {
        self.quota = quota;
        self
    }

    /// Set the billing info.
    #[must_use]
    pub fn with_billing(mut self, billing: BillingInfo) -> Self {
        self.billing = billing;
        self
    }

    /// Add a label.
    #[must_use]
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Add a namespace to this tenant.
    pub fn add_namespace(&mut self, namespace_id: NamespaceId) {
        if !self.namespaces.contains(&namespace_id) {
            self.namespaces.push(namespace_id);
            self.updated_at = Utc::now();
        }
    }

    /// Remove a namespace from this tenant.
    pub fn remove_namespace(&mut self, namespace_id: NamespaceId) {
        self.namespaces.retain(|&id| id != namespace_id);
        self.updated_at = Utc::now();
    }

    /// Check if the tenant has any namespaces.
    #[must_use]
    pub fn has_namespaces(&self) -> bool {
        !self.namespaces.is_empty()
    }

    /// Get the number of namespaces.
    #[must_use]
    pub fn namespace_count(&self) -> usize {
        self.namespaces.len()
    }

    /// Deactivate the tenant.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.updated_at = Utc::now();
    }

    /// Reactivate the tenant.
    pub fn activate(&mut self) {
        self.active = true;
        self.updated_at = Utc::now();
    }

    /// Get the effective display name.
    #[must_use]
    pub fn effective_display_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== TenantId Tests ====================

    #[test]
    fn test_tenant_id_new() {
        let id = TenantId::new();
        assert_eq!(id.as_uuid().get_version_num(), 4);
    }

    #[test]
    fn test_tenant_id_parse_valid() {
        let uuid = Uuid::new_v4();
        let id = TenantId::parse(&uuid.to_string());
        assert!(id.is_ok());
        assert_eq!(id.unwrap_or_default().as_uuid(), uuid);
    }

    #[test]
    fn test_tenant_id_parse_invalid() {
        let result = TenantId::parse("not-a-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn test_tenant_id_display() {
        let uuid = Uuid::new_v4();
        let id = TenantId::from_uuid(uuid);
        assert_eq!(id.to_string(), uuid.to_string());
    }

    // ==================== Tenant Name Validation Tests ====================

    #[test]
    fn test_validate_tenant_name_valid() {
        assert!(validate_tenant_name("acme").is_ok());
        assert!(validate_tenant_name("Acme").is_ok());
        assert!(validate_tenant_name("acme-corp").is_ok());
        assert!(validate_tenant_name("acme_corp").is_ok());
        assert!(validate_tenant_name("Acme_Corp_123").is_ok());
        assert!(validate_tenant_name("a").is_ok());
    }

    #[test]
    fn test_validate_tenant_name_empty() {
        let result = validate_tenant_name("");
        assert!(matches!(result, Err(TenancyError::InvalidTenantName(_))));
    }

    #[test]
    fn test_validate_tenant_name_too_long() {
        let name = "a".repeat(129);
        let result = validate_tenant_name(&name);
        assert!(matches!(result, Err(TenancyError::InvalidTenantName(_))));
    }

    #[test]
    fn test_validate_tenant_name_starts_with_number() {
        let result = validate_tenant_name("1acme");
        assert!(matches!(result, Err(TenancyError::InvalidTenantName(_))));
    }

    #[test]
    fn test_validate_tenant_name_starts_with_hyphen() {
        let result = validate_tenant_name("-acme");
        assert!(matches!(result, Err(TenancyError::InvalidTenantName(_))));
    }

    #[test]
    fn test_validate_tenant_name_ends_with_hyphen() {
        let result = validate_tenant_name("acme-");
        assert!(matches!(result, Err(TenancyError::InvalidTenantName(_))));
    }

    #[test]
    fn test_validate_tenant_name_ends_with_underscore() {
        let result = validate_tenant_name("acme_");
        assert!(matches!(result, Err(TenancyError::InvalidTenantName(_))));
    }

    #[test]
    fn test_validate_tenant_name_special_chars() {
        assert!(validate_tenant_name("acme.corp").is_err());
        assert!(validate_tenant_name("acme/corp").is_err());
        assert!(validate_tenant_name("acme corp").is_err());
    }

    // ==================== BillingInfo Tests ====================

    #[test]
    fn test_billing_info_free() {
        let billing = BillingInfo::free();
        assert_eq!(billing.plan, BillingPlan::Free);
        assert!(billing.external_id.is_none());
    }

    #[test]
    fn test_billing_info_builder() {
        let start = Utc::now();
        let end = start + chrono::Duration::days(30);

        let billing = BillingInfo::free()
            .with_plan(BillingPlan::Professional)
            .with_external_id("cus_123")
            .with_billing_email("billing@acme.com")
            .with_billing_period(start, end);

        assert_eq!(billing.plan, BillingPlan::Professional);
        assert_eq!(billing.external_id, Some("cus_123".to_string()));
        assert_eq!(billing.billing_email, Some("billing@acme.com".to_string()));
        assert_eq!(billing.billing_period_start, Some(start));
        assert_eq!(billing.billing_period_end, Some(end));
    }

    #[test]
    fn test_billing_info_in_billing_period() {
        let start = Utc::now() - chrono::Duration::days(1);
        let end = Utc::now() + chrono::Duration::days(29);

        let billing = BillingInfo::free().with_billing_period(start, end);
        assert!(billing.is_in_billing_period());
    }

    #[test]
    fn test_billing_info_not_in_billing_period() {
        let start = Utc::now() + chrono::Duration::days(1);
        let end = Utc::now() + chrono::Duration::days(31);

        let billing = BillingInfo::free().with_billing_period(start, end);
        assert!(!billing.is_in_billing_period());
    }

    #[test]
    fn test_billing_plan_display() {
        assert_eq!(BillingPlan::Free.to_string(), "free");
        assert_eq!(BillingPlan::Professional.to_string(), "professional");
        assert_eq!(BillingPlan::Enterprise.to_string(), "enterprise");
    }

    // ==================== Tenant Tests ====================

    #[test]
    fn test_tenant_new() {
        let tenant = Tenant::new("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        assert_eq!(tenant.name, "acme");
        assert!(tenant.namespaces.is_empty());
        assert!(tenant.active);
    }

    #[test]
    fn test_tenant_new_invalid_name() {
        let result = Tenant::new("123invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_tenant_with_display_name() {
        let tenant = Tenant::new("acme")
            .map(|t| t.with_display_name("ACME Corporation"));

        assert!(tenant.is_ok());
        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        assert_eq!(tenant.display_name, Some("ACME Corporation".to_string()));
        assert_eq!(tenant.effective_display_name(), "ACME Corporation");
    }

    #[test]
    fn test_tenant_effective_display_name_fallback() {
        let tenant = Tenant::new("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        assert_eq!(tenant.effective_display_name(), "acme");
    }

    #[test]
    fn test_tenant_add_namespace() {
        let tenant = Tenant::new("acme");
        assert!(tenant.is_ok());

        let mut tenant = tenant.unwrap_or_else(|_| unreachable!());
        let ns_id = NamespaceId::new();

        tenant.add_namespace(ns_id);
        assert_eq!(tenant.namespaces.len(), 1);
        assert!(tenant.namespaces.contains(&ns_id));

        // Adding same namespace again should not duplicate
        tenant.add_namespace(ns_id);
        assert_eq!(tenant.namespaces.len(), 1);
    }

    #[test]
    fn test_tenant_remove_namespace() {
        let tenant = Tenant::new("acme");
        assert!(tenant.is_ok());

        let mut tenant = tenant.unwrap_or_else(|_| unreachable!());
        let ns_id1 = NamespaceId::new();
        let ns_id2 = NamespaceId::new();

        tenant.add_namespace(ns_id1);
        tenant.add_namespace(ns_id2);
        assert_eq!(tenant.namespaces.len(), 2);

        tenant.remove_namespace(ns_id1);
        assert_eq!(tenant.namespaces.len(), 1);
        assert!(!tenant.namespaces.contains(&ns_id1));
        assert!(tenant.namespaces.contains(&ns_id2));
    }

    #[test]
    fn test_tenant_has_namespaces() {
        let tenant = Tenant::new("acme");
        assert!(tenant.is_ok());

        let mut tenant = tenant.unwrap_or_else(|_| unreachable!());
        assert!(!tenant.has_namespaces());

        tenant.add_namespace(NamespaceId::new());
        assert!(tenant.has_namespaces());
    }

    #[test]
    fn test_tenant_deactivate_activate() {
        let tenant = Tenant::new("acme");
        assert!(tenant.is_ok());

        let mut tenant = tenant.unwrap_or_else(|_| unreachable!());
        assert!(tenant.active);

        tenant.deactivate();
        assert!(!tenant.active);

        tenant.activate();
        assert!(tenant.active);
    }

    #[test]
    fn test_tenant_with_quota() {
        let quota = ResourceQuota::new()
            .with_max_gpus(16)
            .with_gpu_hours(1000.0);

        let tenant = Tenant::new("acme")
            .map(|t| t.with_quota(quota.clone()));

        assert!(tenant.is_ok());
        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        assert_eq!(tenant.quota.max_gpus, Some(16));
        assert_eq!(tenant.quota.gpu_hours, Some(1000.0));
    }

    #[test]
    fn test_tenant_with_labels() {
        let tenant = Tenant::new("acme")
            .map(|t| t.with_label("env", "production").with_label("region", "us-east"));

        assert!(tenant.is_ok());
        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        assert_eq!(tenant.labels.get("env"), Some(&"production".to_string()));
        assert_eq!(tenant.labels.get("region"), Some(&"us-east".to_string()));
    }

    #[test]
    fn test_tenant_serialization() {
        let tenant = Tenant::new("acme")
            .map(|t| t.with_display_name("ACME Corp"));

        assert!(tenant.is_ok());
        let tenant = tenant.unwrap_or_else(|_| unreachable!());

        let json = serde_json::to_string(&tenant);
        assert!(json.is_ok());

        let json = json.unwrap_or_default();
        let deserialized: std::result::Result<Tenant, _> = serde_json::from_str(&json);
        assert!(deserialized.is_ok());

        let deserialized = deserialized.unwrap_or_else(|_| unreachable!());
        assert_eq!(tenant.id, deserialized.id);
        assert_eq!(tenant.name, deserialized.name);
    }
}
