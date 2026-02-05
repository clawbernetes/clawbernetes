//! # claw-tenancy
//!
//! Multi-tenancy and namespace isolation for Clawbernetes.
//!
//! This crate provides logical isolation between different users/organizations
//! sharing a Clawbernetes cluster through:
//!
//! - **Tenants**: Organizations or teams that own resources
//! - **Namespaces**: Logical isolation boundaries within a tenant
//! - **Resource Quotas**: Limits on GPU hours, memory, workloads, etc.
//! - **Usage Tracking**: Monitor resource consumption per namespace
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                         Tenant                               │
//! │  (organization with billing, quota, multiple namespaces)     │
//! │                                                              │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
//! │  │  Namespace   │  │  Namespace   │  │  Namespace   │       │
//! │  │  (default)   │  │   (prod)     │  │  (staging)   │       │
//! │  │              │  │              │  │              │       │
//! │  │ ┌──────────┐ │  │ ┌──────────┐ │  │ ┌──────────┐ │       │
//! │  │ │ Workload │ │  │ │ Workload │ │  │ │ Workload │ │       │
//! │  │ └──────────┘ │  │ └──────────┘ │  │ └──────────┘ │       │
//! │  │ ┌──────────┐ │  │ ┌──────────┐ │  │              │       │
//! │  │ │ Workload │ │  │ │ Workload │ │  │              │       │
//! │  │ └──────────┘ │  │ └──────────┘ │  │              │       │
//! │  │              │  │              │  │              │       │
//! │  │ Quota: 4 GPU │  │ Quota: 8 GPU │  │ Quota: 2 GPU │       │
//! │  │ Used: 2 GPU  │  │ Used: 6 GPU  │  │ Used: 0 GPU  │       │
//! │  └──────────────┘  └──────────────┘  └──────────────┘       │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Example
//!
//! ```rust
//! use claw_tenancy::{NamespaceManager, ResourceQuota};
//!
//! // Create a namespace manager
//! let manager = NamespaceManager::new();
//!
//! // Create a tenant
//! let tenant = manager.create_tenant("acme-corp")?;
//!
//! // Create a namespace with resource quotas
//! let quota = ResourceQuota::new()
//!     .with_max_gpus(4)
//!     .with_max_workloads(10)
//!     .with_gpu_hours(100.0);
//!
//! let namespace = manager.create_namespace_with_quota(
//!     tenant.id,
//!     "production",
//!     quota,
//! )?;
//!
//! // Check if a workload can be added
//! manager.check_workload_quota(namespace.id, 2, 4096)?;
//!
//! // Record workload addition
//! manager.record_workload_added(namespace.id, 2, 4096)?;
//!
//! // Track GPU hours
//! manager.record_gpu_hours(namespace.id, 1.5)?;
//!
//! # Ok::<(), claw_tenancy::TenancyError>(())
//! ```
//!
//! ## Integration with `WorkloadSpec`
//!
//! When creating a workload, include the namespace in the spec:
//!
//! ```rust,ignore
//! use claw_proto::WorkloadSpec;
//! use claw_tenancy::NamespaceId;
//!
//! let spec = WorkloadSpec::new("nvidia/cuda:12.0-base")
//!     .with_gpu_count(2)
//!     .with_memory_mb(4096);
//!
//! // The scheduler validates against namespace quota before scheduling
//! ```

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod error;
mod manager;
mod namespace;
mod quota;
mod tenant;

pub use error::{Result, TenancyError};
pub use manager::{ManagerStats, NamespaceManager};
pub use namespace::{
    validate_namespace_name, Namespace, NamespaceId, MAX_NAMESPACE_NAME_LENGTH,
    MIN_NAMESPACE_NAME_LENGTH,
};
pub use quota::{QuotaUsage, QuotaUtilization, ResourceQuota};
pub use tenant::{
    validate_tenant_name, BillingInfo, BillingPlan, Tenant, TenantId, MAX_TENANT_NAME_LENGTH,
    MIN_TENANT_NAME_LENGTH,
};

/// Library version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    /// Integration test: full workflow
    #[test]
    fn test_full_workflow() {
        let manager = NamespaceManager::new();

        // Create tenant
        let tenant = manager.create_tenant("acme-corp");
        assert!(tenant.is_ok());
        let tenant = tenant.unwrap_or_else(|_| unreachable!());

        // Create namespace with quota
        let quota = ResourceQuota::new()
            .with_max_gpus(4)
            .with_max_workloads(5)
            .with_memory_mib(16384)
            .with_gpu_hours(100.0);

        let namespace = manager.create_namespace_with_quota(tenant.id, "production", quota);
        assert!(namespace.is_ok());
        let namespace = namespace.unwrap_or_else(|_| unreachable!());

        // Check quota before adding workload
        let can_add = manager.check_workload_quota(namespace.id, 2, 4096);
        assert!(can_add.is_ok());

        // Add workload
        let added = manager.record_workload_added(namespace.id, 2, 4096);
        assert!(added.is_ok());

        // Verify usage
        let ns = manager.get_namespace(namespace.id);
        assert!(ns.is_some());
        let ns = ns.unwrap_or_else(|| unreachable!());
        assert_eq!(ns.active_workloads, 1);
        assert_eq!(ns.usage.gpus_in_use, 2);

        // Try to exceed GPU quota
        let cannot_add = manager.check_workload_quota(namespace.id, 4, 4096);
        assert!(cannot_add.is_err());

        // Record GPU hours
        let _ = manager.record_gpu_hours(namespace.id, 10.0);
        let ns = manager.get_namespace(namespace.id);
        assert!(ns.is_some());
        let ns = ns.unwrap_or_else(|| unreachable!());
        assert!((ns.usage.gpu_hours_used - 10.0).abs() < f64::EPSILON);

        // Remove workload
        let removed = manager.record_workload_removed(namespace.id, 2, 4096);
        assert!(removed.is_ok());

        // Delete namespace
        let deleted = manager.delete_namespace(namespace.id);
        assert!(deleted.is_ok());

        // Delete tenant
        let deleted = manager.delete_tenant(tenant.id);
        assert!(deleted.is_ok());
    }

    /// Test multiple tenants with isolation
    #[test]
    fn test_tenant_isolation() {
        let manager = NamespaceManager::new();

        // Create two tenants
        let tenant1 = manager.create_tenant("acme");
        let tenant2 = manager.create_tenant("globex");

        assert!(tenant1.is_ok());
        assert!(tenant2.is_ok());

        let tenant1 = tenant1.unwrap_or_else(|_| unreachable!());
        let tenant2 = tenant2.unwrap_or_else(|_| unreachable!());

        // Both can have namespace with same name
        let ns1 = manager.create_namespace(tenant1.id, "default");
        let ns2 = manager.create_namespace(tenant2.id, "default");

        assert!(ns1.is_ok());
        assert!(ns2.is_ok());

        let ns1 = ns1.unwrap_or_else(|_| unreachable!());
        let ns2 = ns2.unwrap_or_else(|_| unreachable!());

        // They are different namespaces
        assert_ne!(ns1.id, ns2.id);

        // Quota enforcement is per-namespace
        let quota1 = ResourceQuota::new().with_max_gpus(2);
        let quota2 = ResourceQuota::new().with_max_gpus(8);

        // Update namespaces with different quotas
        let ns1_updated = Namespace::with_id(ns1.id, "default", tenant1.id.as_uuid())
            .map(|n| n.with_quota(quota1));
        let ns2_updated = Namespace::with_id(ns2.id, "default", tenant2.id.as_uuid())
            .map(|n| n.with_quota(quota2));

        assert!(ns1_updated.is_ok());
        assert!(ns2_updated.is_ok());

        let _ = manager.update_namespace(ns1_updated.unwrap_or_else(|_| unreachable!()));
        let _ = manager.update_namespace(ns2_updated.unwrap_or_else(|_| unreachable!()));

        // tenant1 cannot use 4 GPUs
        let check1 = manager.check_workload_quota(ns1.id, 4, 1024);
        assert!(check1.is_err());

        // tenant2 can use 4 GPUs
        let check2 = manager.check_workload_quota(ns2.id, 4, 1024);
        assert!(check2.is_ok());
    }
}
