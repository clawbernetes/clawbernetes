//! Namespace and tenant management.

use std::collections::HashMap;

use parking_lot::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::error::{Result, TenancyError};
use crate::namespace::{Namespace, NamespaceId};
use crate::quota::ResourceQuota;
use crate::tenant::{Tenant, TenantId};

/// Manager for namespaces and tenants.
///
/// This provides the core API for creating, managing, and enforcing
/// isolation between tenants and their namespaces.
#[derive(Debug, Default)]
pub struct NamespaceManager {
    /// All tenants by ID.
    tenants: RwLock<HashMap<TenantId, Tenant>>,
    /// All namespaces by ID.
    namespaces: RwLock<HashMap<NamespaceId, Namespace>>,
    /// Index: tenant name -> tenant ID.
    tenant_names: RwLock<HashMap<String, TenantId>>,
    /// Index: (`tenant_id`, `namespace_name`) -> namespace ID.
    namespace_names: RwLock<HashMap<(Uuid, String), NamespaceId>>,
}

impl NamespaceManager {
    /// Create a new namespace manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tenants: RwLock::new(HashMap::new()),
            namespaces: RwLock::new(HashMap::new()),
            tenant_names: RwLock::new(HashMap::new()),
            namespace_names: RwLock::new(HashMap::new()),
        }
    }

    // ==================== Tenant Operations ====================

    /// Create a new tenant.
    ///
    /// # Errors
    ///
    /// Returns an error if the tenant name already exists or is invalid.
    pub fn create_tenant(&self, name: impl Into<String>) -> Result<Tenant> {
        let name = name.into();

        // Check for existing name
        {
            let names = self.tenant_names.read();
            if names.contains_key(&name) {
                return Err(TenancyError::TenantNameExists(name));
            }
        }

        let tenant = Tenant::new(&name)?;

        // Insert into storage
        {
            let mut tenants = self.tenants.write();
            let mut names = self.tenant_names.write();

            tenants.insert(tenant.id, tenant.clone());
            names.insert(name.clone(), tenant.id);
        }

        info!(tenant_id = %tenant.id, tenant_name = %name, "created tenant");
        Ok(tenant)
    }

    /// Create a tenant with specific configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the tenant name already exists or is invalid.
    pub fn create_tenant_with_config(
        &self,
        name: impl Into<String>,
        quota: ResourceQuota,
        default_namespace_quota: ResourceQuota,
    ) -> Result<Tenant> {
        let name = name.into();

        // Check for existing name
        {
            let names = self.tenant_names.read();
            if names.contains_key(&name) {
                return Err(TenancyError::TenantNameExists(name));
            }
        }

        let tenant = Tenant::new(&name)?
            .with_quota(quota)
            .with_default_namespace_quota(default_namespace_quota);

        // Insert into storage
        {
            let mut tenants = self.tenants.write();
            let mut names = self.tenant_names.write();

            tenants.insert(tenant.id, tenant.clone());
            names.insert(name.clone(), tenant.id);
        }

        info!(tenant_id = %tenant.id, tenant_name = %name, "created tenant with config");
        Ok(tenant)
    }

    /// Get a tenant by ID.
    #[must_use]
    pub fn get_tenant(&self, id: TenantId) -> Option<Tenant> {
        let tenants = self.tenants.read();
        tenants.get(&id).cloned()
    }

    /// Get a tenant by name.
    #[must_use]
    pub fn get_tenant_by_name(&self, name: &str) -> Option<Tenant> {
        let names = self.tenant_names.read();
        let id = names.get(name)?;

        let tenants = self.tenants.read();
        tenants.get(id).cloned()
    }

    /// List all tenants.
    #[must_use]
    pub fn list_tenants(&self) -> Vec<Tenant> {
        let tenants = self.tenants.read();
        tenants.values().cloned().collect()
    }

    /// Update a tenant.
    ///
    /// # Errors
    ///
    /// Returns an error if the tenant is not found.
    pub fn update_tenant(&self, tenant: Tenant) -> Result<()> {
        let mut tenants = self.tenants.write();

        if !tenants.contains_key(&tenant.id) {
            return Err(TenancyError::TenantNotFound(tenant.id.as_uuid()));
        }

        tenants.insert(tenant.id, tenant);
        Ok(())
    }

    /// Delete a tenant.
    ///
    /// # Errors
    ///
    /// Returns an error if the tenant has active namespaces or is not found.
    pub fn delete_tenant(&self, id: TenantId) -> Result<()> {
        let tenant = self
            .get_tenant(id)
            .ok_or(TenancyError::TenantNotFound(id.as_uuid()))?;

        if !tenant.namespaces.is_empty() {
            return Err(TenancyError::TenantHasNamespaces(
                id.as_uuid(),
                tenant.namespaces.len(),
            ));
        }

        // Remove from storage
        {
            let mut tenants = self.tenants.write();
            let mut names = self.tenant_names.write();

            tenants.remove(&id);
            names.remove(&tenant.name);
        }

        info!(tenant_id = %id, tenant_name = %tenant.name, "deleted tenant");
        Ok(())
    }

    // ==================== Namespace Operations ====================

    /// Create a new namespace for a tenant.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The tenant is not found
    /// - The namespace name already exists for this tenant
    /// - The namespace name is invalid
    pub fn create_namespace(
        &self,
        tenant_id: TenantId,
        name: impl Into<String>,
    ) -> Result<Namespace> {
        let name = name.into();

        // Verify tenant exists and get default quota
        let (tenant_uuid, default_quota) = {
            let tenants = self.tenants.read();
            let tenant = tenants
                .get(&tenant_id)
                .ok_or(TenancyError::TenantNotFound(tenant_id.as_uuid()))?;
            (tenant.id.as_uuid(), tenant.default_namespace_quota.clone())
        };

        // Check for existing name
        {
            let names = self.namespace_names.read();
            if names.contains_key(&(tenant_uuid, name.clone())) {
                return Err(TenancyError::NamespaceNameExists(name));
            }
        }

        let namespace = Namespace::new(&name, tenant_uuid)?.with_quota(default_quota);

        // Insert namespace and update tenant
        {
            let mut namespaces = self.namespaces.write();
            let mut names = self.namespace_names.write();
            let mut tenants = self.tenants.write();

            namespaces.insert(namespace.id, namespace.clone());
            names.insert((tenant_uuid, name.clone()), namespace.id);

            // Update tenant's namespace list
            if let Some(tenant) = tenants.get_mut(&tenant_id) {
                tenant.add_namespace(namespace.id);
            }
        }

        info!(
            namespace_id = %namespace.id,
            namespace_name = %name,
            tenant_id = %tenant_id,
            "created namespace"
        );
        Ok(namespace)
    }

    /// Create a namespace with specific quota.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The tenant is not found
    /// - The namespace name already exists for this tenant
    /// - The namespace name is invalid
    pub fn create_namespace_with_quota(
        &self,
        tenant_id: TenantId,
        name: impl Into<String>,
        quota: ResourceQuota,
    ) -> Result<Namespace> {
        let name = name.into();

        // Verify tenant exists
        let tenant_uuid = {
            let tenants = self.tenants.read();
            let tenant = tenants
                .get(&tenant_id)
                .ok_or(TenancyError::TenantNotFound(tenant_id.as_uuid()))?;
            tenant.id.as_uuid()
        };

        // Check for existing name
        {
            let names = self.namespace_names.read();
            if names.contains_key(&(tenant_uuid, name.clone())) {
                return Err(TenancyError::NamespaceNameExists(name));
            }
        }

        let namespace = Namespace::new(&name, tenant_uuid)?.with_quota(quota);

        // Insert namespace and update tenant
        {
            let mut namespaces = self.namespaces.write();
            let mut names = self.namespace_names.write();
            let mut tenants = self.tenants.write();

            namespaces.insert(namespace.id, namespace.clone());
            names.insert((tenant_uuid, name.clone()), namespace.id);

            // Update tenant's namespace list
            if let Some(tenant) = tenants.get_mut(&tenant_id) {
                tenant.add_namespace(namespace.id);
            }
        }

        info!(
            namespace_id = %namespace.id,
            namespace_name = %name,
            tenant_id = %tenant_id,
            "created namespace with quota"
        );
        Ok(namespace)
    }

    /// Get a namespace by ID.
    #[must_use]
    pub fn get_namespace(&self, id: NamespaceId) -> Option<Namespace> {
        let namespaces = self.namespaces.read();
        namespaces.get(&id).cloned()
    }

    /// Get a namespace by tenant and name.
    #[must_use]
    pub fn get_namespace_by_name(&self, tenant_id: TenantId, name: &str) -> Option<Namespace> {
        let names = self.namespace_names.read();
        let id = names.get(&(tenant_id.as_uuid(), name.to_string()))?;

        let namespaces = self.namespaces.read();
        namespaces.get(id).cloned()
    }

    /// List all namespaces for a tenant.
    #[must_use]
    pub fn list_namespaces(&self, tenant_id: TenantId) -> Vec<Namespace> {
        let tenants = self.tenants.read();
        let Some(tenant) = tenants.get(&tenant_id) else {
            return Vec::new();
        };

        let namespaces = self.namespaces.read();
        tenant
            .namespaces
            .iter()
            .filter_map(|id| namespaces.get(id).cloned())
            .collect()
    }

    /// List all namespaces (across all tenants).
    #[must_use]
    pub fn list_all_namespaces(&self) -> Vec<Namespace> {
        let namespaces = self.namespaces.read();
        namespaces.values().cloned().collect()
    }

    /// Update a namespace.
    ///
    /// # Errors
    ///
    /// Returns an error if the namespace is not found.
    pub fn update_namespace(&self, namespace: Namespace) -> Result<()> {
        let mut namespaces = self.namespaces.write();

        if !namespaces.contains_key(&namespace.id) {
            return Err(TenancyError::NamespaceNotFound(namespace.id.as_uuid()));
        }

        namespaces.insert(namespace.id, namespace);
        Ok(())
    }

    /// Delete a namespace.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The namespace is not found
    /// - The namespace has active workloads
    pub fn delete_namespace(&self, id: NamespaceId) -> Result<()> {
        let namespace = self
            .get_namespace(id)
            .ok_or(TenancyError::NamespaceNotFound(id.as_uuid()))?;

        if namespace.has_active_workloads() {
            return Err(TenancyError::NamespaceHasWorkloads(
                id.as_uuid(),
                namespace.active_workloads,
            ));
        }

        // Remove from storage
        {
            let mut namespaces = self.namespaces.write();
            let mut names = self.namespace_names.write();
            let mut tenants = self.tenants.write();

            namespaces.remove(&id);
            names.remove(&(namespace.tenant_id, namespace.name.clone()));

            // Update tenant's namespace list
            let tenant_id = TenantId::from_uuid(namespace.tenant_id);
            if let Some(tenant) = tenants.get_mut(&tenant_id) {
                tenant.remove_namespace(id);
            }
        }

        info!(
            namespace_id = %id,
            namespace_name = %namespace.name,
            "deleted namespace"
        );
        Ok(())
    }

    // ==================== Quota Enforcement ====================

    /// Check if a workload can be added to a namespace.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The namespace is not found
    /// - Adding the workload would exceed quota
    pub fn check_workload_quota(
        &self,
        namespace_id: NamespaceId,
        gpu_count: u32,
        memory_mib: u64,
    ) -> Result<()> {
        let namespaces = self.namespaces.read();
        let namespace = namespaces
            .get(&namespace_id)
            .ok_or(TenancyError::NamespaceNotFound(namespace_id.as_uuid()))?;

        namespace.can_add_workload(gpu_count, memory_mib)
    }

    /// Record that a workload was added to a namespace.
    ///
    /// # Errors
    ///
    /// Returns an error if the namespace is not found.
    pub fn record_workload_added(
        &self,
        namespace_id: NamespaceId,
        gpu_count: u32,
        memory_mib: u64,
    ) -> Result<()> {
        let mut namespaces = self.namespaces.write();
        let namespace = namespaces
            .get_mut(&namespace_id)
            .ok_or(TenancyError::NamespaceNotFound(namespace_id.as_uuid()))?;

        namespace.record_workload_added(gpu_count, memory_mib);
        debug!(
            namespace_id = %namespace_id,
            gpu_count,
            memory_mib,
            active_workloads = namespace.active_workloads,
            "recorded workload added"
        );
        Ok(())
    }

    /// Record that a workload was removed from a namespace.
    ///
    /// # Errors
    ///
    /// Returns an error if the namespace is not found.
    pub fn record_workload_removed(
        &self,
        namespace_id: NamespaceId,
        gpu_count: u32,
        memory_mib: u64,
    ) -> Result<()> {
        let mut namespaces = self.namespaces.write();
        let namespace = namespaces
            .get_mut(&namespace_id)
            .ok_or(TenancyError::NamespaceNotFound(namespace_id.as_uuid()))?;

        namespace.record_workload_removed(gpu_count, memory_mib);
        debug!(
            namespace_id = %namespace_id,
            gpu_count,
            memory_mib,
            active_workloads = namespace.active_workloads,
            "recorded workload removed"
        );
        Ok(())
    }

    /// Record GPU hours used by a namespace.
    ///
    /// # Errors
    ///
    /// Returns an error if the namespace is not found.
    pub fn record_gpu_hours(&self, namespace_id: NamespaceId, hours: f64) -> Result<()> {
        let mut namespaces = self.namespaces.write();
        let namespace = namespaces
            .get_mut(&namespace_id)
            .ok_or(TenancyError::NamespaceNotFound(namespace_id.as_uuid()))?;

        namespace.record_gpu_hours(hours);

        if namespace.is_gpu_hours_exceeded() {
            warn!(
                namespace_id = %namespace_id,
                used = namespace.usage.gpu_hours_used,
                limit = ?namespace.quota.gpu_hours,
                "namespace GPU hours quota exceeded"
            );
        }

        Ok(())
    }

    /// Reset GPU hours for a billing period.
    ///
    /// # Errors
    ///
    /// Returns an error if the namespace is not found.
    pub fn reset_billing_period(&self, namespace_id: NamespaceId) -> Result<()> {
        let mut namespaces = self.namespaces.write();
        let namespace = namespaces
            .get_mut(&namespace_id)
            .ok_or(TenancyError::NamespaceNotFound(namespace_id.as_uuid()))?;

        namespace.usage.reset_billing_period();
        info!(namespace_id = %namespace_id, "reset billing period");
        Ok(())
    }

    /// Reset billing period for all namespaces of a tenant.
    ///
    /// # Errors
    ///
    /// Returns an error if the tenant is not found.
    pub fn reset_tenant_billing_period(&self, tenant_id: TenantId) -> Result<()> {
        let tenants = self.tenants.read();
        let tenant = tenants
            .get(&tenant_id)
            .ok_or(TenancyError::TenantNotFound(tenant_id.as_uuid()))?;

        let namespace_ids = tenant.namespaces.clone();
        drop(tenants);

        let mut namespaces = self.namespaces.write();
        for ns_id in namespace_ids {
            if let Some(namespace) = namespaces.get_mut(&ns_id) {
                namespace.usage.reset_billing_period();
            }
        }

        info!(tenant_id = %tenant_id, "reset tenant billing period");
        Ok(())
    }

    // ==================== Query Operations ====================

    /// Find namespaces matching a label selector.
    #[must_use]
    pub fn find_namespaces_by_labels(
        &self,
        selector: &HashMap<String, String>,
    ) -> Vec<Namespace> {
        let namespaces = self.namespaces.read();
        namespaces
            .values()
            .filter(|ns| ns.matches_selector(selector))
            .cloned()
            .collect()
    }

    /// Find namespaces over a utilization threshold.
    #[must_use]
    pub fn find_namespaces_over_threshold(&self, threshold_percent: f64) -> Vec<Namespace> {
        let namespaces = self.namespaces.read();
        namespaces
            .values()
            .filter(|ns| {
                let util = ns.usage.utilization(&ns.quota);
                util.any_over_threshold(threshold_percent)
            })
            .cloned()
            .collect()
    }

    /// Get statistics about the manager.
    #[must_use]
    pub fn stats(&self) -> ManagerStats {
        let tenants = self.tenants.read();
        let namespaces = self.namespaces.read();

        let active_tenants = tenants.values().filter(|t| t.active).count();
        let total_active_workloads: u32 = namespaces.values().map(|ns| ns.active_workloads).sum();
        let total_gpus_in_use: u32 = namespaces.values().map(|ns| ns.usage.gpus_in_use).sum();

        ManagerStats {
            total_tenants: tenants.len(),
            active_tenants,
            total_namespaces: namespaces.len(),
            total_active_workloads,
            total_gpus_in_use,
        }
    }
}

/// Statistics about the namespace manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagerStats {
    /// Total number of tenants.
    pub total_tenants: usize,
    /// Number of active tenants.
    pub active_tenants: usize,
    /// Total number of namespaces.
    pub total_namespaces: usize,
    /// Total active workloads across all namespaces.
    pub total_active_workloads: u32,
    /// Total GPUs in use across all namespaces.
    pub total_gpus_in_use: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Tenant Management Tests ====================

    #[test]
    fn test_create_tenant() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");

        assert!(tenant.is_ok());
        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        assert_eq!(tenant.name, "acme");
    }

    #[test]
    fn test_create_tenant_duplicate_name() {
        let manager = NamespaceManager::new();
        let _ = manager.create_tenant("acme");

        let result = manager.create_tenant("acme");
        assert!(matches!(result, Err(TenancyError::TenantNameExists(_))));
    }

    #[test]
    fn test_create_tenant_invalid_name() {
        let manager = NamespaceManager::new();
        let result = manager.create_tenant("123invalid");
        assert!(matches!(result, Err(TenancyError::InvalidTenantName(_))));
    }

    #[test]
    fn test_get_tenant() {
        let manager = NamespaceManager::new();
        let created = manager.create_tenant("acme");
        assert!(created.is_ok());

        let created = created.unwrap_or_else(|_| unreachable!());
        let retrieved = manager.get_tenant(created.id);

        assert!(retrieved.is_some());
        assert_eq!(retrieved.map(|t| t.name), Some("acme".to_string()));
    }

    #[test]
    fn test_get_tenant_by_name() {
        let manager = NamespaceManager::new();
        let _ = manager.create_tenant("acme");

        let retrieved = manager.get_tenant_by_name("acme");
        assert!(retrieved.is_some());

        let not_found = manager.get_tenant_by_name("unknown");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_list_tenants() {
        let manager = NamespaceManager::new();
        let _ = manager.create_tenant("acme");
        let _ = manager.create_tenant("globex");

        let tenants = manager.list_tenants();
        assert_eq!(tenants.len(), 2);
    }

    #[test]
    fn test_delete_tenant() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let result = manager.delete_tenant(tenant.id);
        assert!(result.is_ok());

        assert!(manager.get_tenant(tenant.id).is_none());
    }

    #[test]
    fn test_delete_tenant_with_namespaces() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let _ = manager.create_namespace(tenant.id, "default");

        let result = manager.delete_tenant(tenant.id);
        assert!(matches!(result, Err(TenancyError::TenantHasNamespaces(_, 1))));
    }

    // ==================== Namespace Management Tests ====================

    #[test]
    fn test_create_namespace() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let namespace = manager.create_namespace(tenant.id, "default");

        assert!(namespace.is_ok());
        let namespace = namespace.unwrap_or_else(|_| unreachable!());
        assert_eq!(namespace.name, "default");
        assert_eq!(namespace.tenant_id, tenant.id.as_uuid());
    }

    #[test]
    fn test_create_namespace_inherits_default_quota() {
        let manager = NamespaceManager::new();

        let default_quota = ResourceQuota::new()
            .with_max_gpus(4)
            .with_max_workloads(10);

        let tenant = manager.create_tenant_with_config(
            "acme",
            ResourceQuota::new(),
            default_quota.clone(),
        );
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let namespace = manager.create_namespace(tenant.id, "default");

        assert!(namespace.is_ok());
        let namespace = namespace.unwrap_or_else(|_| unreachable!());
        assert_eq!(namespace.quota.max_gpus, Some(4));
        assert_eq!(namespace.quota.max_workloads, Some(10));
    }

    #[test]
    fn test_create_namespace_duplicate_name() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let _ = manager.create_namespace(tenant.id, "default");

        let result = manager.create_namespace(tenant.id, "default");
        assert!(matches!(result, Err(TenancyError::NamespaceNameExists(_))));
    }

    #[test]
    fn test_create_namespace_same_name_different_tenants() {
        let manager = NamespaceManager::new();

        let tenant1 = manager.create_tenant("acme");
        let tenant2 = manager.create_tenant("globex");

        assert!(tenant1.is_ok());
        assert!(tenant2.is_ok());

        let tenant1 = tenant1.unwrap_or_else(|_| unreachable!());
        let tenant2 = tenant2.unwrap_or_else(|_| unreachable!());

        let ns1 = manager.create_namespace(tenant1.id, "default");
        let ns2 = manager.create_namespace(tenant2.id, "default");

        assert!(ns1.is_ok());
        assert!(ns2.is_ok());
    }

    #[test]
    fn test_create_namespace_tenant_not_found() {
        let manager = NamespaceManager::new();
        let fake_id = TenantId::new();

        let result = manager.create_namespace(fake_id, "default");
        assert!(matches!(result, Err(TenancyError::TenantNotFound(_))));
    }

    #[test]
    fn test_get_namespace() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let created = manager.create_namespace(tenant.id, "default");
        assert!(created.is_ok());

        let created = created.unwrap_or_else(|_| unreachable!());
        let retrieved = manager.get_namespace(created.id);

        assert!(retrieved.is_some());
        assert_eq!(retrieved.map(|n| n.name), Some("default".to_string()));
    }

    #[test]
    fn test_get_namespace_by_name() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let _ = manager.create_namespace(tenant.id, "default");

        let retrieved = manager.get_namespace_by_name(tenant.id, "default");
        assert!(retrieved.is_some());

        let not_found = manager.get_namespace_by_name(tenant.id, "unknown");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_list_namespaces() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let _ = manager.create_namespace(tenant.id, "default");
        let _ = manager.create_namespace(tenant.id, "production");

        let namespaces = manager.list_namespaces(tenant.id);
        assert_eq!(namespaces.len(), 2);
    }

    #[test]
    fn test_delete_namespace() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let namespace = manager.create_namespace(tenant.id, "default");
        assert!(namespace.is_ok());

        let namespace = namespace.unwrap_or_else(|_| unreachable!());
        let result = manager.delete_namespace(namespace.id);
        assert!(result.is_ok());

        assert!(manager.get_namespace(namespace.id).is_none());
    }

    #[test]
    fn test_delete_namespace_with_workloads() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let namespace = manager.create_namespace(tenant.id, "default");
        assert!(namespace.is_ok());

        let namespace = namespace.unwrap_or_else(|_| unreachable!());
        // Add a workload
        let _ = manager.record_workload_added(namespace.id, 1, 1024);

        let result = manager.delete_namespace(namespace.id);
        assert!(matches!(
            result,
            Err(TenancyError::NamespaceHasWorkloads(_, 1))
        ));
    }

    // ==================== Quota Enforcement Tests ====================

    #[test]
    fn test_check_workload_quota_no_limits() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let namespace = manager.create_namespace(tenant.id, "default");
        assert!(namespace.is_ok());

        let namespace = namespace.unwrap_or_else(|_| unreachable!());
        let result = manager.check_workload_quota(namespace.id, 4, 8192);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_workload_quota_within_limits() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let quota = ResourceQuota::new()
            .with_max_workloads(10)
            .with_max_gpus(8)
            .with_memory_mib(16384);

        let namespace = manager.create_namespace_with_quota(tenant.id, "default", quota);
        assert!(namespace.is_ok());

        let namespace = namespace.unwrap_or_else(|_| unreachable!());
        let result = manager.check_workload_quota(namespace.id, 4, 8192);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_workload_quota_exceeds_gpus() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let quota = ResourceQuota::new().with_max_gpus(2);

        let namespace = manager.create_namespace_with_quota(tenant.id, "default", quota);
        assert!(namespace.is_ok());

        let namespace = namespace.unwrap_or_else(|_| unreachable!());
        let result = manager.check_workload_quota(namespace.id, 4, 1024);
        assert!(matches!(
            result,
            Err(TenancyError::QuotaExceeded { resource, .. }) if resource == "gpus"
        ));
    }

    #[test]
    fn test_record_workload_added() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let namespace = manager.create_namespace(tenant.id, "default");
        assert!(namespace.is_ok());

        let namespace = namespace.unwrap_or_else(|_| unreachable!());
        let result = manager.record_workload_added(namespace.id, 2, 4096);
        assert!(result.is_ok());

        let updated = manager.get_namespace(namespace.id);
        assert!(updated.is_some());

        let updated = updated.unwrap_or_else(|| unreachable!());
        assert_eq!(updated.active_workloads, 1);
        assert_eq!(updated.usage.gpus_in_use, 2);
        assert_eq!(updated.usage.memory_mib_used, 4096);
    }

    #[test]
    fn test_record_workload_removed() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let namespace = manager.create_namespace(tenant.id, "default");
        assert!(namespace.is_ok());

        let namespace = namespace.unwrap_or_else(|_| unreachable!());
        let _ = manager.record_workload_added(namespace.id, 4, 8192);
        let result = manager.record_workload_removed(namespace.id, 4, 8192);
        assert!(result.is_ok());

        let updated = manager.get_namespace(namespace.id);
        assert!(updated.is_some());

        let updated = updated.unwrap_or_else(|| unreachable!());
        assert_eq!(updated.active_workloads, 0);
        assert_eq!(updated.usage.gpus_in_use, 0);
        assert_eq!(updated.usage.memory_mib_used, 0);
    }

    #[test]
    fn test_record_gpu_hours() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let namespace = manager.create_namespace(tenant.id, "default");
        assert!(namespace.is_ok());

        let namespace = namespace.unwrap_or_else(|_| unreachable!());
        let result = manager.record_gpu_hours(namespace.id, 10.5);
        assert!(result.is_ok());

        let updated = manager.get_namespace(namespace.id);
        assert!(updated.is_some());

        let updated = updated.unwrap_or_else(|| unreachable!());
        assert!((updated.usage.gpu_hours_used - 10.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_reset_billing_period() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let namespace = manager.create_namespace(tenant.id, "default");
        assert!(namespace.is_ok());

        let namespace = namespace.unwrap_or_else(|_| unreachable!());
        let _ = manager.record_gpu_hours(namespace.id, 50.0);
        let _ = manager.record_workload_added(namespace.id, 2, 4096);

        let result = manager.reset_billing_period(namespace.id);
        assert!(result.is_ok());

        let updated = manager.get_namespace(namespace.id);
        assert!(updated.is_some());

        let updated = updated.unwrap_or_else(|| unreachable!());
        assert!(updated.usage.gpu_hours_used < f64::EPSILON);
        // Workloads should still be tracked
        assert_eq!(updated.active_workloads, 1);
    }

    // ==================== Query Tests ====================

    #[test]
    fn test_find_namespaces_by_labels() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());

        // Create namespaces with different labels
        let ns1 = manager.create_namespace(tenant.id, "prod");
        let ns2 = manager.create_namespace(tenant.id, "staging");

        assert!(ns1.is_ok());
        assert!(ns2.is_ok());

        let ns1 = ns1.unwrap_or_else(|_| unreachable!());
        let ns2 = ns2.unwrap_or_else(|_| unreachable!());

        // Update with labels
        let ns1_updated = Namespace::with_id(ns1.id, "prod", tenant.id.as_uuid())
            .map(|n| n.with_label("env", "production"));
        let ns2_updated = Namespace::with_id(ns2.id, "staging", tenant.id.as_uuid())
            .map(|n| n.with_label("env", "staging"));

        assert!(ns1_updated.is_ok());
        assert!(ns2_updated.is_ok());

        let _ = manager.update_namespace(ns1_updated.unwrap_or_else(|_| unreachable!()));
        let _ = manager.update_namespace(ns2_updated.unwrap_or_else(|_| unreachable!()));

        let mut selector = HashMap::new();
        selector.insert("env".to_string(), "production".to_string());

        let results = manager.find_namespaces_by_labels(&selector);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "prod");
    }

    #[test]
    fn test_stats() {
        let manager = NamespaceManager::new();
        let tenant = manager.create_tenant("acme");
        assert!(tenant.is_ok());

        let tenant = tenant.unwrap_or_else(|_| unreachable!());
        let namespace = manager.create_namespace(tenant.id, "default");
        assert!(namespace.is_ok());

        let namespace = namespace.unwrap_or_else(|_| unreachable!());
        let _ = manager.record_workload_added(namespace.id, 2, 4096);

        let stats = manager.stats();
        assert_eq!(stats.total_tenants, 1);
        assert_eq!(stats.active_tenants, 1);
        assert_eq!(stats.total_namespaces, 1);
        assert_eq!(stats.total_active_workloads, 1);
        assert_eq!(stats.total_gpus_in_use, 2);
    }
}
