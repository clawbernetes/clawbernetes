//! Service discovery, ingress routing, and network policies for Clawbernetes.
//!
//! Provides [`ServiceStore`] with unified management of services, ingress rules,
//! and network policies.

#![forbid(unsafe_code)]

use claw_persist::JsonStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::warn;

/// A service entry for service discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEntry {
    /// Service name.
    pub name: String,
    /// Label selector.
    pub selector: HashMap<String, String>,
    /// Service port.
    pub port: u16,
    /// Protocol (TCP, UDP, HTTP).
    pub protocol: String,
    /// Active endpoints.
    pub endpoints: Vec<String>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// An ingress routing rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngressRule {
    /// Hostname to match.
    pub host: String,
    /// Path prefix to match.
    pub path: String,
    /// Backend service name.
    pub service: String,
}

/// An ingress entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngressEntry {
    /// Ingress name.
    pub name: String,
    /// Routing rules.
    pub rules: Vec<IngressRule>,
    /// Whether TLS is enabled.
    pub tls: bool,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// A network policy entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicyEntry {
    /// Policy name.
    pub name: String,
    /// Pod selector.
    pub selector: HashMap<String, String>,
    /// Ingress rules (JSON).
    pub ingress_rules: Vec<serde_json::Value>,
    /// Egress rules (JSON).
    pub egress_rules: Vec<serde_json::Value>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory service store with services, ingresses, and network policies.
pub struct ServiceStore {
    services: HashMap<String, ServiceEntry>,
    ingresses: HashMap<String, IngressEntry>,
    policies: HashMap<String, NetworkPolicyEntry>,
    service_store: JsonStore,
    ingress_store: JsonStore,
    policy_store: JsonStore,
}

impl ServiceStore {
    /// Create a new service store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let service_store = JsonStore::new(state_path, "services");
        let ingress_store = JsonStore::new(state_path, "ingresses");
        let policy_store = JsonStore::new(state_path, "network_policies");
        Self {
            services: service_store.load(),
            ingresses: ingress_store.load(),
            policies: policy_store.load(),
            service_store,
            ingress_store,
            policy_store,
        }
    }

    /// Create a new service.
    pub fn create_service(&mut self, entry: ServiceEntry) -> Result<(), String> {
        if self.services.contains_key(&entry.name) {
            return Err(format!("service '{}' already exists", entry.name));
        }
        let name = entry.name.clone();
        self.services.insert(name, entry);
        self.snapshot_services();
        Ok(())
    }

    /// Get a service by name.
    pub fn get_service(&self, name: &str) -> Option<&ServiceEntry> {
        self.services.get(name)
    }

    /// Delete a service.
    pub fn delete_service(&mut self, name: &str) -> Result<(), String> {
        self.services
            .remove(name)
            .ok_or_else(|| format!("service '{name}' not found"))?;
        self.snapshot_services();
        Ok(())
    }

    /// List all services.
    pub fn list_services(&self) -> Vec<&ServiceEntry> {
        self.services.values().collect()
    }

    /// Create a new ingress.
    pub fn create_ingress(&mut self, entry: IngressEntry) -> Result<(), String> {
        if self.ingresses.contains_key(&entry.name) {
            return Err(format!("ingress '{}' already exists", entry.name));
        }
        let name = entry.name.clone();
        self.ingresses.insert(name, entry);
        self.snapshot_ingresses();
        Ok(())
    }

    /// Delete an ingress.
    pub fn delete_ingress(&mut self, name: &str) -> Result<(), String> {
        self.ingresses
            .remove(name)
            .ok_or_else(|| format!("ingress '{name}' not found"))?;
        self.snapshot_ingresses();
        Ok(())
    }

    /// Create a network policy.
    pub fn create_network_policy(&mut self, entry: NetworkPolicyEntry) -> Result<(), String> {
        if self.policies.contains_key(&entry.name) {
            return Err(format!("network policy '{}' already exists", entry.name));
        }
        let name = entry.name.clone();
        self.policies.insert(name, entry);
        self.snapshot_policies();
        Ok(())
    }

    /// Delete a network policy.
    pub fn delete_network_policy(&mut self, name: &str) -> Result<(), String> {
        self.policies
            .remove(name)
            .ok_or_else(|| format!("network policy '{name}' not found"))?;
        self.snapshot_policies();
        Ok(())
    }

    /// List all network policies.
    pub fn list_network_policies(&self) -> Vec<&NetworkPolicyEntry> {
        self.policies.values().collect()
    }

    fn snapshot_services(&self) {
        if let Err(e) = self.service_store.save(&self.services) {
            warn!(error = %e, "failed to snapshot service store");
        }
    }

    fn snapshot_ingresses(&self) {
        if let Err(e) = self.ingress_store.save(&self.ingresses) {
            warn!(error = %e, "failed to snapshot ingress store");
        }
    }

    fn snapshot_policies(&self) {
        if let Err(e) = self.policy_store.save(&self.policies) {
            warn!(error = %e, "failed to snapshot network policy store");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_crud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = ServiceStore::new(dir.path());

        store.create_service(ServiceEntry {
            name: "api-svc".to_string(),
            selector: HashMap::from([("app".to_string(), "api".to_string())]),
            port: 8080,
            protocol: "TCP".to_string(),
            endpoints: vec!["10.0.0.1:8080".to_string()],
            created_at: chrono::Utc::now(),
        }).expect("create");

        assert!(store.get_service("api-svc").is_some());
        assert_eq!(store.list_services().len(), 1);

        assert!(store.create_service(ServiceEntry {
            name: "api-svc".to_string(),
            selector: HashMap::new(),
            port: 80,
            protocol: "TCP".to_string(),
            endpoints: vec![],
            created_at: chrono::Utc::now(),
        }).is_err());

        store.delete_service("api-svc").expect("delete");
        assert!(store.get_service("api-svc").is_none());
    }

    #[test]
    fn test_ingress_crud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = ServiceStore::new(dir.path());

        store.create_ingress(IngressEntry {
            name: "web-ingress".to_string(),
            rules: vec![IngressRule {
                host: "example.com".to_string(),
                path: "/".to_string(),
                service: "web-svc".to_string(),
            }],
            tls: true,
            created_at: chrono::Utc::now(),
        }).expect("create");

        assert!(store.create_ingress(IngressEntry {
            name: "web-ingress".to_string(),
            rules: vec![],
            tls: false,
            created_at: chrono::Utc::now(),
        }).is_err());

        store.delete_ingress("web-ingress").expect("delete");
        assert!(store.delete_ingress("web-ingress").is_err());
    }

    #[test]
    fn test_network_policy_crud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = ServiceStore::new(dir.path());

        store.create_network_policy(NetworkPolicyEntry {
            name: "deny-all".to_string(),
            selector: HashMap::new(),
            ingress_rules: vec![],
            egress_rules: vec![],
            created_at: chrono::Utc::now(),
        }).expect("create");

        assert_eq!(store.list_network_policies().len(), 1);

        store.delete_network_policy("deny-all").expect("delete");
        assert_eq!(store.list_network_policies().len(), 0);
    }

    #[test]
    fn test_service_store_persistence() {
        let dir = tempfile::tempdir().expect("tempdir");
        {
            let mut store = ServiceStore::new(dir.path());
            store.create_service(ServiceEntry {
                name: "persist-svc".to_string(),
                selector: HashMap::new(),
                port: 80,
                protocol: "TCP".to_string(),
                endpoints: vec![],
                created_at: chrono::Utc::now(),
            }).expect("create");
        }
        {
            let store = ServiceStore::new(dir.path());
            assert!(store.get_service("persist-svc").is_some());
        }
    }
}
