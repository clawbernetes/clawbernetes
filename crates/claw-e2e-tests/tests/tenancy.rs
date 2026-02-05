//! End-to-end tests for Multi-tenancy (claw-tenancy).
//!
//! These tests verify:
//! 1. Tenant creation and management
//! 2. Namespace isolation
//! 3. Resource quotas and enforcement
//! 4. Usage tracking
//! 5. Cross-tenant isolation

use claw_tenancy::{
    BillingInfo, BillingPlan, NamespaceManager, ResourceQuota,
    validate_namespace_name, validate_tenant_name,
};

// ============================================================================
// Tenant Management Tests
// ============================================================================

#[test]
fn test_tenant_creation() {
    let manager = NamespaceManager::new();

    let tenant = manager.create_tenant("acme-corp");
    assert!(tenant.is_ok());

    let tenant = tenant.unwrap();
    assert_eq!(tenant.name, "acme-corp");
    assert!(!tenant.id.as_uuid().is_nil());
}

#[test]
fn test_duplicate_tenant_rejected() {
    let manager = NamespaceManager::new();

    // Create first tenant
    let tenant1 = manager.create_tenant("duplicate");
    assert!(tenant1.is_ok());

    // Try to create duplicate
    let tenant2 = manager.create_tenant("duplicate");
    assert!(tenant2.is_err());
}

#[test]
fn test_tenant_name_validation() {
    // Valid names (start with letter, alphanumeric + hyphen/underscore, not ending with -/_)
    assert!(validate_tenant_name("acme").is_ok());
    assert!(validate_tenant_name("acme-corp").is_ok());
    assert!(validate_tenant_name("company123").is_ok());
    assert!(validate_tenant_name("my-org-name").is_ok());
    assert!(validate_tenant_name("Acme").is_ok()); // Uppercase is allowed

    // Invalid names
    assert!(validate_tenant_name("").is_err());       // Empty
    assert!(validate_tenant_name("-invalid").is_err()); // Starts with dash
    assert!(validate_tenant_name("1invalid").is_err()); // Starts with number
    assert!(validate_tenant_name("has space").is_err()); // Spaces not allowed
    assert!(validate_tenant_name("invalid-").is_err()); // Ends with dash
}

#[test]
fn test_tenant_deletion() {
    let manager = NamespaceManager::new();

    // Create tenant
    let tenant = manager.create_tenant("deletable").unwrap();
    let tenant_id = tenant.id;

    // Verify it exists
    assert!(manager.get_tenant(tenant_id).is_some());

    // Delete tenant
    let deleted = manager.delete_tenant(tenant_id);
    assert!(deleted.is_ok());

    // Should no longer exist
    assert!(manager.get_tenant(tenant_id).is_none());
}

#[test]
fn test_tenant_with_billing_info() {
    let manager = NamespaceManager::new();

    let mut tenant = manager.create_tenant("billing-test").unwrap();

    // Add billing information
    let billing = BillingInfo::free()
        .with_plan(BillingPlan::Enterprise)
        .with_billing_email("billing@company.com");

    tenant.billing = billing;
    let updated = manager.update_tenant(tenant.clone());
    assert!(updated.is_ok());

    // Verify billing was set
    let retrieved = manager.get_tenant(tenant.id).unwrap();
    assert_eq!(retrieved.billing.plan, BillingPlan::Enterprise);
    assert!(retrieved.billing.billing_email.is_some());
}

// ============================================================================
// Namespace Management Tests
// ============================================================================

#[test]
fn test_namespace_creation() {
    let manager = NamespaceManager::new();

    // Create tenant first
    let tenant = manager.create_tenant("test-org").unwrap();

    // Create namespace
    let namespace = manager.create_namespace(tenant.id, "production");
    assert!(namespace.is_ok());

    let namespace = namespace.unwrap();
    assert_eq!(namespace.name, "production");
}

#[test]
fn test_namespace_name_validation() {
    // Valid names (lowercase, alphanumeric + hyphen, not ending with -)
    assert!(validate_namespace_name("default").is_ok());
    assert!(validate_namespace_name("production").is_ok());
    assert!(validate_namespace_name("staging-env").is_ok());
    assert!(validate_namespace_name("dev-1").is_ok());
    assert!(validate_namespace_name("a").is_ok()); // Single char is allowed

    // Invalid names
    assert!(validate_namespace_name("").is_err());
    assert!(validate_namespace_name("-invalid").is_err());
    assert!(validate_namespace_name("Production").is_err()); // Uppercase not allowed
    assert!(validate_namespace_name("invalid-").is_err()); // Ends with hyphen
}

#[test]
fn test_multiple_namespaces_per_tenant() {
    let manager = NamespaceManager::new();
    let tenant = manager.create_tenant("multi-ns-org").unwrap();

    // Create multiple namespaces
    let ns1 = manager.create_namespace(tenant.id, "production");
    let ns2 = manager.create_namespace(tenant.id, "staging");
    let ns3 = manager.create_namespace(tenant.id, "development");

    assert!(ns1.is_ok());
    assert!(ns2.is_ok());
    assert!(ns3.is_ok());

    // List namespaces for tenant
    let namespaces = manager.list_namespaces(tenant.id);
    assert_eq!(namespaces.len(), 3);
}

#[test]
fn test_same_namespace_name_different_tenants() {
    let manager = NamespaceManager::new();

    // Create two tenants
    let tenant1 = manager.create_tenant("org-one").unwrap();
    let tenant2 = manager.create_tenant("org-two").unwrap();

    // Both can have "default" namespace
    let ns1 = manager.create_namespace(tenant1.id, "default");
    let ns2 = manager.create_namespace(tenant2.id, "default");

    assert!(ns1.is_ok());
    assert!(ns2.is_ok());

    // Different namespace IDs
    assert_ne!(ns1.unwrap().id, ns2.unwrap().id);
}

#[test]
fn test_namespace_deletion() {
    let manager = NamespaceManager::new();
    let tenant = manager.create_tenant("delete-ns-org").unwrap();

    let namespace = manager.create_namespace(tenant.id, "temporary").unwrap();
    let ns_id = namespace.id;

    // Verify exists
    assert!(manager.get_namespace(ns_id).is_some());

    // Delete
    let deleted = manager.delete_namespace(ns_id);
    assert!(deleted.is_ok());

    // Should not exist
    assert!(manager.get_namespace(ns_id).is_none());
}

// ============================================================================
// Resource Quota Tests
// ============================================================================

#[test]
fn test_resource_quota_creation() {
    let quota = ResourceQuota::new()
        .with_max_gpus(8)
        .with_max_workloads(100)
        .with_memory_mib(65536)  // 64 GB
        .with_gpu_hours(1000.0);

    assert_eq!(quota.max_gpus, Some(8));
    assert_eq!(quota.max_workloads, Some(100));
    assert_eq!(quota.memory_mib, Some(65536));
    assert!((quota.gpu_hours.unwrap() - 1000.0).abs() < f64::EPSILON);
}

#[test]
fn test_namespace_with_quota() {
    let manager = NamespaceManager::new();
    let tenant = manager.create_tenant("quota-org").unwrap();

    let quota = ResourceQuota::new()
        .with_max_gpus(4)
        .with_max_workloads(10)
        .with_gpu_hours(100.0);

    let namespace = manager.create_namespace_with_quota(
        tenant.id,
        "limited",
        quota,
    );

    assert!(namespace.is_ok());

    let ns = namespace.unwrap();
    assert_eq!(ns.quota.max_gpus, Some(4));
    assert_eq!(ns.quota.max_workloads, Some(10));
}

#[test]
fn test_quota_check_workload() {
    let manager = NamespaceManager::new();
    let tenant = manager.create_tenant("check-org").unwrap();

    let quota = ResourceQuota::new()
        .with_max_gpus(4)
        .with_max_workloads(5)
        .with_memory_mib(16384);

    let namespace = manager.create_namespace_with_quota(
        tenant.id,
        "checked",
        quota,
    ).unwrap();

    // Should allow workload within quota
    let can_add = manager.check_workload_quota(namespace.id, 2, 4096);
    assert!(can_add.is_ok());

    // Should reject workload exceeding GPU quota
    let cannot_add = manager.check_workload_quota(namespace.id, 8, 4096);
    assert!(cannot_add.is_err());

    // Should reject workload exceeding memory quota
    let cannot_add = manager.check_workload_quota(namespace.id, 1, 65536);
    assert!(cannot_add.is_err());
}

#[test]
fn test_quota_enforcement() {
    let manager = NamespaceManager::new();
    let tenant = manager.create_tenant("enforce-org").unwrap();

    let quota = ResourceQuota::new()
        .with_max_gpus(4)
        .with_max_workloads(2);

    let namespace = manager.create_namespace_with_quota(
        tenant.id,
        "enforced",
        quota,
    ).unwrap();

    // Add first workload (2 GPUs)
    assert!(manager.record_workload_added(namespace.id, 2, 4096).is_ok());

    // Add second workload (2 GPUs) - total now 4 GPUs
    assert!(manager.record_workload_added(namespace.id, 2, 4096).is_ok());

    // Try to add third workload - should fail (workload limit)
    assert!(manager.check_workload_quota(namespace.id, 0, 0).is_err());

    // Try to add more GPUs - should fail (GPU limit)
    manager.record_workload_removed(namespace.id, 0, 0).unwrap(); // Remove one workload
    assert!(manager.check_workload_quota(namespace.id, 2, 4096).is_err()); // Would exceed GPU quota
}

// ============================================================================
// Usage Tracking Tests
// ============================================================================

#[test]
fn test_usage_tracking() {
    let manager = NamespaceManager::new();
    let tenant = manager.create_tenant("usage-org").unwrap();

    let quota = ResourceQuota::new()
        .with_max_gpus(8)
        .with_max_workloads(10);

    let namespace = manager.create_namespace_with_quota(
        tenant.id,
        "tracked",
        quota,
    ).unwrap();

    // Add workloads
    manager.record_workload_added(namespace.id, 2, 4096).unwrap();
    manager.record_workload_added(namespace.id, 4, 8192).unwrap();

    // Check usage
    let ns = manager.get_namespace(namespace.id).unwrap();
    assert_eq!(ns.active_workloads, 2);
    assert_eq!(ns.usage.gpus_in_use, 6);
    assert_eq!(ns.usage.memory_mib_used, 12288);
}

#[test]
fn test_gpu_hours_tracking() {
    let manager = NamespaceManager::new();
    let tenant = manager.create_tenant("gpu-hours-org").unwrap();

    let quota = ResourceQuota::new()
        .with_max_gpus(4)
        .with_gpu_hours(100.0);

    let namespace = manager.create_namespace_with_quota(
        tenant.id,
        "gpu-tracked",
        quota,
    ).unwrap();

    // Record GPU usage
    manager.record_gpu_hours(namespace.id, 10.0).unwrap();
    manager.record_gpu_hours(namespace.id, 25.0).unwrap();
    manager.record_gpu_hours(namespace.id, 15.0).unwrap();

    // Check usage
    let ns = manager.get_namespace(namespace.id).unwrap();
    assert!((ns.usage.gpu_hours_used - 50.0).abs() < f64::EPSILON);
}

#[test]
fn test_workload_removal_updates_usage() {
    let manager = NamespaceManager::new();
    let tenant = manager.create_tenant("removal-org").unwrap();

    let quota = ResourceQuota::new()
        .with_max_gpus(8);

    let namespace = manager.create_namespace_with_quota(
        tenant.id,
        "removal-test",
        quota,
    ).unwrap();

    // Add workloads
    manager.record_workload_added(namespace.id, 4, 8192).unwrap();
    manager.record_workload_added(namespace.id, 2, 4096).unwrap();

    let ns = manager.get_namespace(namespace.id).unwrap();
    assert_eq!(ns.usage.gpus_in_use, 6);

    // Remove one workload
    manager.record_workload_removed(namespace.id, 4, 8192).unwrap();

    let ns = manager.get_namespace(namespace.id).unwrap();
    assert_eq!(ns.usage.gpus_in_use, 2);
    assert_eq!(ns.active_workloads, 1);
}

#[test]
fn test_quota_utilization() {
    let manager = NamespaceManager::new();
    let tenant = manager.create_tenant("util-org").unwrap();

    let quota = ResourceQuota::new()
        .with_max_gpus(10)
        .with_max_workloads(20)
        .with_memory_mib(32768)
        .with_gpu_hours(100.0);

    let namespace = manager.create_namespace_with_quota(
        tenant.id,
        "utilization",
        quota,
    ).unwrap();

    // Use 50% of resources
    manager.record_workload_added(namespace.id, 5, 16384).unwrap();
    manager.record_gpu_hours(namespace.id, 50.0).unwrap();

    let ns = manager.get_namespace(namespace.id).unwrap();
    let utilization = ns.usage.utilization(&ns.quota);

    // All should be around 50%
    assert!((utilization.gpus_percent.unwrap() - 50.0).abs() < 1.0);
    assert!((utilization.memory_percent.unwrap() - 50.0).abs() < 1.0);
    assert!((utilization.gpu_hours_percent.unwrap() - 50.0).abs() < 1.0);
}

// ============================================================================
// Tenant Isolation Tests
// ============================================================================

#[test]
fn test_tenant_isolation() {
    let manager = NamespaceManager::new();

    // Create two tenants
    let tenant1 = manager.create_tenant("isolated-org-1").unwrap();
    let tenant2 = manager.create_tenant("isolated-org-2").unwrap();

    // Each has their own namespace with same name
    let quota1 = ResourceQuota::new().with_max_gpus(2);
    let quota2 = ResourceQuota::new().with_max_gpus(8);

    let ns1 = manager.create_namespace_with_quota(
        tenant1.id,
        "production",
        quota1,
    ).unwrap();

    let ns2 = manager.create_namespace_with_quota(
        tenant2.id,
        "production",
        quota2,
    ).unwrap();

    // Tenant 1's quota is independent
    assert!(manager.check_workload_quota(ns1.id, 2, 4096).is_ok());
    assert!(manager.check_workload_quota(ns1.id, 4, 4096).is_err()); // Exceeds ns1 quota

    // Tenant 2 has more quota
    assert!(manager.check_workload_quota(ns2.id, 4, 4096).is_ok());
    assert!(manager.check_workload_quota(ns2.id, 8, 4096).is_ok());
}

#[test]
fn test_namespace_cannot_access_other_tenant() {
    let manager = NamespaceManager::new();

    let tenant1 = manager.create_tenant("owner").unwrap();
    let tenant2 = manager.create_tenant("other").unwrap();

    let ns1 = manager.create_namespace(tenant1.id, "private").unwrap();

    // List namespaces only returns tenant's own
    let tenant1_namespaces = manager.list_namespaces(tenant1.id);
    let tenant2_namespaces = manager.list_namespaces(tenant2.id);

    assert_eq!(tenant1_namespaces.len(), 1);
    assert_eq!(tenant2_namespaces.len(), 0);

    // Can't delete another tenant's namespace directly
    // (The namespace ID itself is the lookup key, but we validate ownership)
    let ns1_belongs_to_tenant1 = tenant1_namespaces.iter()
        .any(|ns| ns.id == ns1.id);
    assert!(ns1_belongs_to_tenant1);
}

// ============================================================================
// Manager Statistics Tests
// ============================================================================

#[test]
fn test_manager_stats() {
    let manager = NamespaceManager::new();

    // Create multiple tenants with namespaces
    for i in 1..=3 {
        let tenant = manager.create_tenant(format!("tenant-{}", i)).unwrap();
        for j in 1..=2 {
            let quota = ResourceQuota::new().with_max_gpus(4);
            manager.create_namespace_with_quota(
                tenant.id,
                format!("ns-{}", j),
                quota,
            ).unwrap();
        }
    }

    let stats = manager.stats();
    assert_eq!(stats.total_tenants, 3);
    assert_eq!(stats.total_namespaces, 6);
}

// ============================================================================
// Integration: Full Multi-tenancy Workflow
// ============================================================================

#[test]
fn test_full_multi_tenancy_workflow() {
    let manager = NamespaceManager::new();

    // 1. Onboard a new organization
    let mut tenant = manager.create_tenant("enterprise-corp").unwrap();

    // 2. Set up billing
    let billing = BillingInfo::free()
        .with_plan(BillingPlan::Enterprise)
        .with_billing_email("finance@enterprise-corp.com");
    tenant.billing = billing;
    manager.update_tenant(tenant.clone()).unwrap();

    // 3. Create namespaces with different quotas
    let prod_quota = ResourceQuota::new()
        .with_max_gpus(16)
        .with_max_workloads(100)
        .with_memory_mib(131072)  // 128 GB
        .with_gpu_hours(10000.0);

    let staging_quota = ResourceQuota::new()
        .with_max_gpus(8)
        .with_max_workloads(50)
        .with_memory_mib(65536)  // 64 GB
        .with_gpu_hours(5000.0);

    let dev_quota = ResourceQuota::new()
        .with_max_gpus(4)
        .with_max_workloads(20)
        .with_memory_mib(32768)  // 32 GB
        .with_gpu_hours(1000.0);

    let prod_ns = manager.create_namespace_with_quota(
        tenant.id,
        "production",
        prod_quota,
    ).unwrap();

    let staging_ns = manager.create_namespace_with_quota(
        tenant.id,
        "staging",
        staging_quota,
    ).unwrap();

    let dev_ns = manager.create_namespace_with_quota(
        tenant.id,
        "development",
        dev_quota,
    ).unwrap();

    // 4. Deploy workloads
    // Production: 4 GPUs, 16 GB
    assert!(manager.check_workload_quota(prod_ns.id, 4, 16384).is_ok());
    manager.record_workload_added(prod_ns.id, 4, 16384).unwrap();

    // More production workloads
    manager.record_workload_added(prod_ns.id, 8, 32768).unwrap();
    manager.record_workload_added(prod_ns.id, 2, 8192).unwrap();

    // Staging
    manager.record_workload_added(staging_ns.id, 4, 16384).unwrap();

    // Development
    manager.record_workload_added(dev_ns.id, 2, 8192).unwrap();

    // 5. Track GPU usage
    manager.record_gpu_hours(prod_ns.id, 500.0).unwrap();
    manager.record_gpu_hours(staging_ns.id, 100.0).unwrap();
    manager.record_gpu_hours(dev_ns.id, 50.0).unwrap();

    // 6. Verify usage
    let prod = manager.get_namespace(prod_ns.id).unwrap();
    let staging = manager.get_namespace(staging_ns.id).unwrap();
    let dev = manager.get_namespace(dev_ns.id).unwrap();

    // Production usage
    assert_eq!(prod.active_workloads, 3);
    assert_eq!(prod.usage.gpus_in_use, 14);
    assert!((prod.usage.gpu_hours_used - 500.0).abs() < f64::EPSILON);

    // Staging usage
    assert_eq!(staging.active_workloads, 1);
    assert_eq!(staging.usage.gpus_in_use, 4);

    // Development usage
    assert_eq!(dev.active_workloads, 1);
    assert_eq!(dev.usage.gpus_in_use, 2);

    // 7. Check utilization
    let prod_util = prod.usage.utilization(&prod.quota);
    assert!(prod_util.gpus_percent.unwrap() > 80.0); // 14/16 = 87.5%

    // 8. Try to exceed quota (should fail)
    let cannot_add = manager.check_workload_quota(prod_ns.id, 4, 16384);
    assert!(cannot_add.is_err()); // Would exceed 16 GPU limit

    // 9. Remove a workload to free capacity
    manager.record_workload_removed(prod_ns.id, 4, 16384).unwrap();

    // 10. Now we can add
    let can_add = manager.check_workload_quota(prod_ns.id, 4, 16384);
    assert!(can_add.is_ok());

    // 11. Verify manager stats
    let stats = manager.stats();
    assert_eq!(stats.total_tenants, 1);
    assert_eq!(stats.total_namespaces, 3);
}

#[test]
fn test_multi_organization_isolation() {
    let manager = NamespaceManager::new();

    // Organization 1: Small startup
    let startup = manager.create_tenant("ai-startup").unwrap();
    let startup_quota = ResourceQuota::new()
        .with_max_gpus(4)
        .with_max_workloads(10);

    let startup_ns = manager.create_namespace_with_quota(
        startup.id,
        "default",
        startup_quota,
    ).unwrap();

    // Organization 2: Enterprise
    let enterprise = manager.create_tenant("big-corp").unwrap();
    let enterprise_quota = ResourceQuota::new()
        .with_max_gpus(100)
        .with_max_workloads(1000);

    let enterprise_ns = manager.create_namespace_with_quota(
        enterprise.id,
        "default",
        enterprise_quota,
    ).unwrap();

    // Startup uses their full quota
    manager.record_workload_added(startup_ns.id, 4, 16384).unwrap();

    // Enterprise uses only a fraction
    manager.record_workload_added(enterprise_ns.id, 10, 40960).unwrap();

    // Startup can't add more GPUs
    assert!(manager.check_workload_quota(startup_ns.id, 2, 4096).is_err());

    // Enterprise can add much more
    assert!(manager.check_workload_quota(enterprise_ns.id, 50, 204800).is_ok());

    // They don't affect each other's quotas
    let startup_status = manager.get_namespace(startup_ns.id).unwrap();
    let enterprise_status = manager.get_namespace(enterprise_ns.id).unwrap();

    assert_eq!(startup_status.usage.gpus_in_use, 4);
    assert_eq!(enterprise_status.usage.gpus_in_use, 10);
}
