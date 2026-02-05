//! Error types for the claw-tenancy crate.

use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur during tenancy operations.
#[derive(Debug, Error)]
pub enum TenancyError {
    /// Namespace was not found.
    #[error("namespace not found: {0}")]
    NamespaceNotFound(Uuid),

    /// Namespace name already exists.
    #[error("namespace name already exists: {0}")]
    NamespaceNameExists(String),

    /// Tenant was not found.
    #[error("tenant not found: {0}")]
    TenantNotFound(Uuid),

    /// Tenant name already exists.
    #[error("tenant name already exists: {0}")]
    TenantNameExists(String),

    /// Quota exceeded for a resource.
    #[error("quota exceeded for {resource}: used {used}, limit {limit}")]
    QuotaExceeded {
        /// The resource that exceeded quota.
        resource: String,
        /// Current usage.
        used: u64,
        /// Maximum limit.
        limit: u64,
    },

    /// Invalid namespace name.
    #[error("invalid namespace name: {0}")]
    InvalidNamespaceName(String),

    /// Invalid tenant name.
    #[error("invalid tenant name: {0}")]
    InvalidTenantName(String),

    /// Namespace has active workloads.
    #[error("namespace {0} has {1} active workloads")]
    NamespaceHasWorkloads(Uuid, u32),

    /// Tenant has active namespaces.
    #[error("tenant {0} has {1} active namespaces")]
    TenantHasNamespaces(Uuid, usize),

    /// Invalid quota configuration.
    #[error("invalid quota: {0}")]
    InvalidQuota(String),
}

/// Result type for tenancy operations.
pub type Result<T> = std::result::Result<T, TenancyError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_not_found_display() {
        let id = Uuid::new_v4();
        let err = TenancyError::NamespaceNotFound(id);
        assert!(err.to_string().contains(&id.to_string()));
    }

    #[test]
    fn test_quota_exceeded_display() {
        let err = TenancyError::QuotaExceeded {
            resource: "gpu_hours".to_string(),
            used: 100,
            limit: 80,
        };
        let msg = err.to_string();
        assert!(msg.contains("gpu_hours"));
        assert!(msg.contains("100"));
        assert!(msg.contains("80"));
    }

    #[test]
    fn test_namespace_has_workloads_display() {
        let id = Uuid::new_v4();
        let err = TenancyError::NamespaceHasWorkloads(id, 5);
        let msg = err.to_string();
        assert!(msg.contains("5"));
        assert!(msg.contains("active workloads"));
    }
}
