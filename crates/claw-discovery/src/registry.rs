//! Service registry for managing services and their endpoints.

use std::collections::HashMap;

use parking_lot::RwLock;
use thiserror::Error;
use tracing::{debug, info, warn};

use crate::load_balancer::LoadBalancer;
use crate::types::{
    Endpoint, EndpointId, HealthStatus, LabelSelector, Service, ServiceId, ServicePort,
};

/// Errors that can occur during registry operations.
#[derive(Debug, Error)]
pub enum RegistryError {
    /// Service already exists.
    #[error("service '{0}' already exists in namespace '{1}'")]
    ServiceExists(String, String),

    /// Service not found.
    #[error("service '{0}' not found in namespace '{1}'")]
    ServiceNotFound(String, String),

    /// Service not found by ID.
    #[error("service with ID {0} not found")]
    ServiceNotFoundById(ServiceId),

    /// Endpoint not found.
    #[error("endpoint {0} not found")]
    EndpointNotFound(EndpointId),

    /// Invalid service name.
    #[error("invalid service name: {0}")]
    InvalidServiceName(String),

    /// Invalid namespace.
    #[error("invalid namespace: {0}")]
    InvalidNamespace(String),
}

/// Result type for registry operations.
pub type Result<T> = std::result::Result<T, RegistryError>;

/// Internal structure for tracking a service with its load balancer.
#[derive(Debug)]
struct ServiceEntry {
    service: Service,
    load_balancer: LoadBalancer,
}

/// Service registry for managing services and their endpoints.
///
/// The registry provides:
/// - Service registration and deregistration
/// - Endpoint tracking for each service
/// - Load balancer management per service
/// - Label-based service discovery
#[derive(Debug, Default)]
pub struct ServiceRegistry {
    /// Services indexed by (namespace, name).
    services: RwLock<HashMap<(String, String), ServiceEntry>>,
    /// Service ID to (namespace, name) mapping for quick lookups.
    service_ids: RwLock<HashMap<ServiceId, (String, String)>>,
}

impl ServiceRegistry {
    /// Creates a new empty service registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new service.
    ///
    /// # Errors
    ///
    /// Returns an error if a service with the same name and namespace already exists.
    pub fn register(&self, service: Service) -> Result<ServiceId> {
        validate_name(&service.name)?;
        validate_namespace(&service.namespace)?;

        let key = (service.namespace.clone(), service.name.clone());
        let service_id = service.id;

        let mut services = self.services.write();

        if services.contains_key(&key) {
            return Err(RegistryError::ServiceExists(
                service.name.clone(),
                service.namespace.clone(),
            ));
        }

        let load_balancer = LoadBalancer::new(service.config.load_balancer.clone());

        info!(
            service = %service.name,
            namespace = %service.namespace,
            id = %service_id,
            "Registered service"
        );

        services.insert(
            key.clone(),
            ServiceEntry {
                service,
                load_balancer,
            },
        );

        // Update ID mapping
        drop(services);
        let mut service_ids = self.service_ids.write();
        service_ids.insert(service_id, key);

        Ok(service_id)
    }

    /// Deregisters a service by name and namespace.
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not found.
    pub fn deregister(&self, namespace: &str, name: &str) -> Result<Service> {
        let key = (namespace.to_string(), name.to_string());

        let mut services = self.services.write();

        let entry = services
            .remove(&key)
            .ok_or_else(|| RegistryError::ServiceNotFound(name.to_string(), namespace.to_string()))?;

        // Update ID mapping
        drop(services);
        let mut service_ids = self.service_ids.write();
        service_ids.remove(&entry.service.id);

        info!(
            service = %name,
            namespace = %namespace,
            "Deregistered service"
        );

        Ok(entry.service)
    }

    /// Deregisters a service by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not found.
    pub fn deregister_by_id(&self, service_id: ServiceId) -> Result<Service> {
        let service_ids = self.service_ids.read();
        let key = service_ids
            .get(&service_id)
            .cloned()
            .ok_or(RegistryError::ServiceNotFoundById(service_id))?;
        drop(service_ids);

        self.deregister(&key.0, &key.1)
    }

    /// Gets a service by name and namespace.
    #[must_use]
    pub fn get(&self, namespace: &str, name: &str) -> Option<Service> {
        let services = self.services.read();
        services
            .get(&(namespace.to_string(), name.to_string()))
            .map(|e| e.service.clone())
    }

    /// Gets a service by ID.
    #[must_use]
    pub fn get_by_id(&self, service_id: ServiceId) -> Option<Service> {
        let service_ids = self.service_ids.read();
        let key = service_ids.get(&service_id)?;

        let services = self.services.read();
        services.get(key).map(|e| e.service.clone())
    }

    /// Lists all services.
    #[must_use]
    pub fn list(&self) -> Vec<Service> {
        let services = self.services.read();
        services.values().map(|e| e.service.clone()).collect()
    }

    /// Lists all services in a namespace.
    #[must_use]
    pub fn list_in_namespace(&self, namespace: &str) -> Vec<Service> {
        let services = self.services.read();
        services
            .iter()
            .filter(|((ns, _), _)| ns == namespace)
            .map(|(_, e)| e.service.clone())
            .collect()
    }

    /// Finds services matching a label selector.
    #[must_use]
    pub fn find_by_labels(&self, selector: &LabelSelector) -> Vec<Service> {
        let services = self.services.read();
        services
            .values()
            .filter(|e| selector.matches(&e.service.labels))
            .map(|e| e.service.clone())
            .collect()
    }

    /// Returns the number of registered services.
    #[must_use]
    pub fn len(&self) -> usize {
        self.services.read().len()
    }

    /// Returns true if the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.services.read().is_empty()
    }

    // ==================== Endpoint Management ====================

    /// Adds an endpoint to a service.
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not found.
    pub fn add_endpoint(
        &self,
        namespace: &str,
        service_name: &str,
        endpoint: Endpoint,
    ) -> Result<EndpointId> {
        let key = (namespace.to_string(), service_name.to_string());
        let endpoint_id = endpoint.id;

        let services = self.services.read();

        let entry = services.get(&key).ok_or_else(|| {
            RegistryError::ServiceNotFound(service_name.to_string(), namespace.to_string())
        })?;

        debug!(
            service = %service_name,
            namespace = %namespace,
            endpoint_id = %endpoint_id,
            address = %endpoint.socket_addr(),
            "Added endpoint to service"
        );

        entry.load_balancer.add_endpoint(endpoint);

        Ok(endpoint_id)
    }

    /// Removes an endpoint from a service.
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not found.
    pub fn remove_endpoint(
        &self,
        namespace: &str,
        service_name: &str,
        endpoint_id: EndpointId,
    ) -> Result<Option<Endpoint>> {
        let key = (namespace.to_string(), service_name.to_string());

        let services = self.services.read();

        let entry = services.get(&key).ok_or_else(|| {
            RegistryError::ServiceNotFound(service_name.to_string(), namespace.to_string())
        })?;

        let removed = entry.load_balancer.remove_endpoint(endpoint_id);

        if removed.is_some() {
            debug!(
                service = %service_name,
                namespace = %namespace,
                endpoint_id = %endpoint_id,
                "Removed endpoint from service"
            );
        }

        Ok(removed)
    }

    /// Gets all endpoints for a service.
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not found.
    pub fn get_endpoints(&self, namespace: &str, service_name: &str) -> Result<Vec<Endpoint>> {
        let key = (namespace.to_string(), service_name.to_string());

        let services = self.services.read();

        let entry = services.get(&key).ok_or_else(|| {
            RegistryError::ServiceNotFound(service_name.to_string(), namespace.to_string())
        })?;

        Ok(entry.load_balancer.endpoints())
    }

    /// Gets all healthy endpoints for a service.
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not found.
    pub fn get_healthy_endpoints(
        &self,
        namespace: &str,
        service_name: &str,
    ) -> Result<Vec<Endpoint>> {
        let key = (namespace.to_string(), service_name.to_string());

        let services = self.services.read();

        let entry = services.get(&key).ok_or_else(|| {
            RegistryError::ServiceNotFound(service_name.to_string(), namespace.to_string())
        })?;

        Ok(entry.load_balancer.healthy_endpoints())
    }

    /// Updates the health status of an endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not found.
    pub fn update_endpoint_health(
        &self,
        namespace: &str,
        service_name: &str,
        endpoint_id: EndpointId,
        status: HealthStatus,
    ) -> Result<()> {
        let key = (namespace.to_string(), service_name.to_string());

        let services = self.services.read();

        let entry = services.get(&key).ok_or_else(|| {
            RegistryError::ServiceNotFound(service_name.to_string(), namespace.to_string())
        })?;

        entry.load_balancer.update_health(endpoint_id, status);

        debug!(
            service = %service_name,
            namespace = %namespace,
            endpoint_id = %endpoint_id,
            status = %status,
            "Updated endpoint health"
        );

        Ok(())
    }

    // ==================== Load Balancing ====================

    /// Selects an endpoint for a service using the configured load balancer.
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not found or no healthy endpoints are available.
    pub fn select_endpoint(
        &self,
        namespace: &str,
        service_name: &str,
        client_ip: Option<std::net::IpAddr>,
    ) -> Result<Endpoint> {
        let key = (namespace.to_string(), service_name.to_string());

        let services = self.services.read();

        let entry = services.get(&key).ok_or_else(|| {
            RegistryError::ServiceNotFound(service_name.to_string(), namespace.to_string())
        })?;

        let use_affinity = entry.service.config.session_affinity;

        if use_affinity {
            if let Some(ip) = client_ip {
                return entry
                    .load_balancer
                    .select_with_affinity(ip)
                    .map_err(|e| {
                        warn!(
                            service = %service_name,
                            namespace = %namespace,
                            error = %e,
                            "Load balancer selection failed"
                        );
                        RegistryError::ServiceNotFound(service_name.to_string(), namespace.to_string())
                    });
            }
        }

        entry.load_balancer.select(client_ip).map_err(|e| {
            warn!(
                service = %service_name,
                namespace = %namespace,
                error = %e,
                "Load balancer selection failed"
            );
            RegistryError::ServiceNotFound(service_name.to_string(), namespace.to_string())
        })
    }

    /// Records a connection to an endpoint.
    pub fn record_connection(
        &self,
        namespace: &str,
        service_name: &str,
        endpoint_id: EndpointId,
    ) {
        let key = (namespace.to_string(), service_name.to_string());

        let services = self.services.read();

        if let Some(entry) = services.get(&key) {
            entry.load_balancer.record_connection(endpoint_id);
        }
    }

    /// Records a disconnection from an endpoint.
    pub fn record_disconnection(
        &self,
        namespace: &str,
        service_name: &str,
        endpoint_id: EndpointId,
    ) {
        let key = (namespace.to_string(), service_name.to_string());

        let services = self.services.read();

        if let Some(entry) = services.get(&key) {
            entry.load_balancer.record_disconnection(endpoint_id);
        }
    }

    // ==================== Service Port Helpers ====================

    /// Gets the ports for a service.
    #[must_use]
    pub fn get_ports(&self, namespace: &str, service_name: &str) -> Option<Vec<ServicePort>> {
        let services = self.services.read();
        services
            .get(&(namespace.to_string(), service_name.to_string()))
            .map(|e| e.service.ports.clone())
    }

    /// Gets a specific port by name.
    #[must_use]
    pub fn get_port(
        &self,
        namespace: &str,
        service_name: &str,
        port_name: &str,
    ) -> Option<ServicePort> {
        let services = self.services.read();
        services
            .get(&(namespace.to_string(), service_name.to_string()))
            .and_then(|e| {
                e.service
                    .ports
                    .iter()
                    .find(|p| p.name.as_deref() == Some(port_name))
                    .cloned()
            })
    }

    // ==================== Statistics ====================

    /// Gets the number of endpoints for a service.
    #[must_use]
    pub fn endpoint_count(&self, namespace: &str, service_name: &str) -> Option<usize> {
        let services = self.services.read();
        services
            .get(&(namespace.to_string(), service_name.to_string()))
            .map(|e| e.load_balancer.endpoint_count())
    }

    /// Gets the number of healthy endpoints for a service.
    #[must_use]
    pub fn healthy_endpoint_count(&self, namespace: &str, service_name: &str) -> Option<usize> {
        let services = self.services.read();
        services
            .get(&(namespace.to_string(), service_name.to_string()))
            .map(|e| e.load_balancer.healthy_endpoint_count())
    }

    /// Gets registry statistics.
    #[must_use]
    pub fn stats(&self) -> RegistryStats {
        let services = self.services.read();

        let service_count = services.len();
        let mut total_endpoints = 0;
        let mut healthy_endpoints = 0;
        let mut namespaces = std::collections::HashSet::new();

        for ((namespace, _), entry) in services.iter() {
            namespaces.insert(namespace.clone());
            total_endpoints += entry.load_balancer.endpoint_count();
            healthy_endpoints += entry.load_balancer.healthy_endpoint_count();
        }

        RegistryStats {
            service_count,
            namespace_count: namespaces.len(),
            total_endpoints,
            healthy_endpoints,
        }
    }
}

/// Statistics about the service registry.
#[derive(Debug, Clone, Default)]
pub struct RegistryStats {
    /// Number of registered services.
    pub service_count: usize,
    /// Number of unique namespaces.
    pub namespace_count: usize,
    /// Total number of endpoints across all services.
    pub total_endpoints: usize,
    /// Number of healthy endpoints across all services.
    pub healthy_endpoints: usize,
}

/// Validates a service name.
fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(RegistryError::InvalidServiceName(
            "name cannot be empty".to_string(),
        ));
    }

    if name.len() > 253 {
        return Err(RegistryError::InvalidServiceName(
            "name cannot exceed 253 characters".to_string(),
        ));
    }

    // Must start with alphanumeric
    if !name.chars().next().is_some_and(char::is_alphanumeric) {
        return Err(RegistryError::InvalidServiceName(
            "name must start with alphanumeric character".to_string(),
        ));
    }

    // Must contain only alphanumeric, hyphens, dots
    if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.') {
        return Err(RegistryError::InvalidServiceName(
            "name can only contain alphanumeric characters, hyphens, and dots".to_string(),
        ));
    }

    Ok(())
}

/// Validates a namespace.
fn validate_namespace(namespace: &str) -> Result<()> {
    if namespace.is_empty() {
        return Err(RegistryError::InvalidNamespace(
            "namespace cannot be empty".to_string(),
        ));
    }

    if namespace.len() > 63 {
        return Err(RegistryError::InvalidNamespace(
            "namespace cannot exceed 63 characters".to_string(),
        ));
    }

    // Must start with alphanumeric
    if !namespace.chars().next().is_some_and(char::is_alphanumeric) {
        return Err(RegistryError::InvalidNamespace(
            "namespace must start with alphanumeric character".to_string(),
        ));
    }

    // Must contain only alphanumeric and hyphens
    if !namespace.chars().all(|c| c.is_alphanumeric() || c == '-') {
        return Err(RegistryError::InvalidNamespace(
            "namespace can only contain alphanumeric characters and hyphens".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{HealthStatus, LoadBalancerStrategy, ServicePort};

    // ==================== Helper Functions ====================

    fn make_service(name: &str) -> Service {
        Service::builder(name)
            .namespace("default")
            .port(ServicePort::http(80))
            .build()
    }

    fn make_service_in_namespace(name: &str, namespace: &str) -> Service {
        Service::builder(name)
            .namespace(namespace)
            .port(ServicePort::http(80))
            .build()
    }

    fn make_endpoint(ip: &str, port: u16) -> Endpoint {
        let mut endpoint = Endpoint::builder(ip.parse().ok().unwrap(), port).build();
        endpoint.health_status = HealthStatus::Healthy;
        endpoint
    }

    // ==================== Constructor Tests ====================

    #[test]
    fn test_new_registry_is_empty() {
        let registry = ServiceRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_default_registry_is_empty() {
        let registry = ServiceRegistry::default();
        assert!(registry.is_empty());
    }

    // ==================== Service Registration Tests ====================

    #[test]
    fn test_register_service() {
        let registry = ServiceRegistry::new();
        let service = make_service("api-gateway");

        let result = registry.register(service);

        assert!(result.is_ok());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_register_duplicate_service_fails() {
        let registry = ServiceRegistry::new();
        let service1 = make_service("api-gateway");
        let service2 = make_service("api-gateway");

        registry.register(service1).ok();
        let result = registry.register(service2);

        assert!(matches!(result, Err(RegistryError::ServiceExists(_, _))));
    }

    #[test]
    fn test_register_same_name_different_namespace() {
        let registry = ServiceRegistry::new();
        let service1 = make_service_in_namespace("api", "prod");
        let service2 = make_service_in_namespace("api", "staging");

        let r1 = registry.register(service1);
        let r2 = registry.register(service2);

        assert!(r1.is_ok());
        assert!(r2.is_ok());
        assert_eq!(registry.len(), 2);
    }

    // ==================== Service Name Validation Tests ====================

    #[test]
    fn test_invalid_empty_name() {
        let registry = ServiceRegistry::new();
        let mut service = make_service("temp");
        service.name = String::new();

        let result = registry.register(service);
        assert!(matches!(result, Err(RegistryError::InvalidServiceName(_))));
    }

    #[test]
    fn test_invalid_name_with_special_chars() {
        let registry = ServiceRegistry::new();
        let mut service = make_service("temp");
        service.name = "api_gateway".to_string(); // underscore not allowed

        let result = registry.register(service);
        assert!(matches!(result, Err(RegistryError::InvalidServiceName(_))));
    }

    #[test]
    fn test_invalid_name_starting_with_hyphen() {
        let registry = ServiceRegistry::new();
        let mut service = make_service("temp");
        service.name = "-api".to_string();

        let result = registry.register(service);
        assert!(matches!(result, Err(RegistryError::InvalidServiceName(_))));
    }

    #[test]
    fn test_invalid_namespace_empty() {
        let registry = ServiceRegistry::new();
        let mut service = make_service("api");
        service.namespace = String::new();

        let result = registry.register(service);
        assert!(matches!(result, Err(RegistryError::InvalidNamespace(_))));
    }

    // ==================== Service Deregistration Tests ====================

    #[test]
    fn test_deregister_service() {
        let registry = ServiceRegistry::new();
        let service = make_service("api-gateway");

        registry.register(service).ok();
        assert_eq!(registry.len(), 1);

        let result = registry.deregister("default", "api-gateway");

        assert!(result.is_ok());
        assert!(registry.is_empty());
    }

    #[test]
    fn test_deregister_nonexistent_service() {
        let registry = ServiceRegistry::new();

        let result = registry.deregister("default", "nonexistent");

        assert!(matches!(result, Err(RegistryError::ServiceNotFound(_, _))));
    }

    #[test]
    fn test_deregister_by_id() {
        let registry = ServiceRegistry::new();
        let service = make_service("api-gateway");

        let id = registry.register(service).ok().unwrap();

        let result = registry.deregister_by_id(id);

        assert!(result.is_ok());
        assert!(registry.is_empty());
    }

    #[test]
    fn test_deregister_by_invalid_id() {
        let registry = ServiceRegistry::new();
        let result = registry.deregister_by_id(ServiceId::new());
        assert!(matches!(result, Err(RegistryError::ServiceNotFoundById(_))));
    }

    // ==================== Service Lookup Tests ====================

    #[test]
    fn test_get_service() {
        let registry = ServiceRegistry::new();
        let service = make_service("api-gateway");

        registry.register(service).ok();

        let retrieved = registry.get("default", "api-gateway");

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap_or_else(|| make_service("")).name, "api-gateway");
    }

    #[test]
    fn test_get_nonexistent_service() {
        let registry = ServiceRegistry::new();
        let retrieved = registry.get("default", "nonexistent");
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_get_by_id() {
        let registry = ServiceRegistry::new();
        let service = make_service("api-gateway");

        let id = registry.register(service).ok().unwrap();

        let retrieved = registry.get_by_id(id);

        assert!(retrieved.is_some());
    }

    #[test]
    fn test_list_all_services() {
        let registry = ServiceRegistry::new();

        registry.register(make_service("service1")).ok();
        registry.register(make_service("service2")).ok();
        registry.register(make_service("service3")).ok();

        let services = registry.list();

        assert_eq!(services.len(), 3);
    }

    #[test]
    fn test_list_in_namespace() {
        let registry = ServiceRegistry::new();

        registry.register(make_service_in_namespace("api", "prod")).ok();
        registry.register(make_service_in_namespace("web", "prod")).ok();
        registry.register(make_service_in_namespace("api", "staging")).ok();

        let prod_services = registry.list_in_namespace("prod");
        let staging_services = registry.list_in_namespace("staging");

        assert_eq!(prod_services.len(), 2);
        assert_eq!(staging_services.len(), 1);
    }

    #[test]
    fn test_find_by_labels() {
        let registry = ServiceRegistry::new();

        let s1 = Service::builder("api")
            .namespace("default")
            .label("team", "platform")
            .build();
        let s2 = Service::builder("web")
            .namespace("default")
            .label("team", "platform")
            .build();
        let s3 = Service::builder("db")
            .namespace("default")
            .label("team", "data")
            .build();

        registry.register(s1).ok();
        registry.register(s2).ok();
        registry.register(s3).ok();

        let selector = LabelSelector::new().with_label("team", "platform");
        let found = registry.find_by_labels(&selector);

        assert_eq!(found.len(), 2);
    }

    // ==================== Endpoint Management Tests ====================

    #[test]
    fn test_add_endpoint() {
        let registry = ServiceRegistry::new();
        let service = make_service("api-gateway");

        registry.register(service).ok();

        let endpoint = make_endpoint("10.0.0.1", 8080);
        let result = registry.add_endpoint("default", "api-gateway", endpoint);

        assert!(result.is_ok());
        assert_eq!(registry.endpoint_count("default", "api-gateway"), Some(1));
    }

    #[test]
    fn test_add_endpoint_to_nonexistent_service() {
        let registry = ServiceRegistry::new();
        let endpoint = make_endpoint("10.0.0.1", 8080);

        let result = registry.add_endpoint("default", "nonexistent", endpoint);

        assert!(matches!(result, Err(RegistryError::ServiceNotFound(_, _))));
    }

    #[test]
    fn test_remove_endpoint() {
        let registry = ServiceRegistry::new();
        let service = make_service("api-gateway");
        registry.register(service).ok();

        let endpoint = make_endpoint("10.0.0.1", 8080);
        let endpoint_id = endpoint.id;
        registry.add_endpoint("default", "api-gateway", endpoint).ok();

        let result = registry.remove_endpoint("default", "api-gateway", endpoint_id);

        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
        assert_eq!(registry.endpoint_count("default", "api-gateway"), Some(0));
    }

    #[test]
    fn test_get_endpoints() {
        let registry = ServiceRegistry::new();
        let service = make_service("api-gateway");
        registry.register(service).ok();

        registry.add_endpoint("default", "api-gateway", make_endpoint("10.0.0.1", 8080)).ok();
        registry.add_endpoint("default", "api-gateway", make_endpoint("10.0.0.2", 8080)).ok();

        let endpoints = registry.get_endpoints("default", "api-gateway");

        assert!(endpoints.is_ok());
        assert_eq!(endpoints.unwrap().len(), 2);
    }

    #[test]
    fn test_get_healthy_endpoints() {
        let registry = ServiceRegistry::new();
        let service = make_service("api-gateway");
        registry.register(service).ok();

        let healthy = make_endpoint("10.0.0.1", 8080);
        let mut unhealthy = make_endpoint("10.0.0.2", 8080);
        unhealthy.health_status = HealthStatus::Unhealthy;

        registry.add_endpoint("default", "api-gateway", healthy).ok();
        registry.add_endpoint("default", "api-gateway", unhealthy).ok();

        let endpoints = registry.get_healthy_endpoints("default", "api-gateway");

        assert!(endpoints.is_ok());
        assert_eq!(endpoints.unwrap().len(), 1);
    }

    #[test]
    fn test_update_endpoint_health() {
        let registry = ServiceRegistry::new();
        let service = make_service("api-gateway");
        registry.register(service).ok();

        let endpoint = make_endpoint("10.0.0.1", 8080);
        let endpoint_id = endpoint.id;
        registry.add_endpoint("default", "api-gateway", endpoint).ok();

        assert_eq!(registry.healthy_endpoint_count("default", "api-gateway"), Some(1));

        registry.update_endpoint_health("default", "api-gateway", endpoint_id, HealthStatus::Unhealthy).ok();

        assert_eq!(registry.healthy_endpoint_count("default", "api-gateway"), Some(0));
    }

    // ==================== Load Balancing Tests ====================

    #[test]
    fn test_select_endpoint() {
        let registry = ServiceRegistry::new();
        let service = make_service("api-gateway");
        registry.register(service).ok();

        registry.add_endpoint("default", "api-gateway", make_endpoint("10.0.0.1", 8080)).ok();
        registry.add_endpoint("default", "api-gateway", make_endpoint("10.0.0.2", 8080)).ok();

        let result = registry.select_endpoint("default", "api-gateway", None);

        assert!(result.is_ok());
    }

    #[test]
    fn test_select_endpoint_from_nonexistent_service() {
        let registry = ServiceRegistry::new();

        let result = registry.select_endpoint("default", "nonexistent", None);

        assert!(matches!(result, Err(RegistryError::ServiceNotFound(_, _))));
    }

    #[test]
    fn test_select_endpoint_with_session_affinity() {
        let registry = ServiceRegistry::new();

        let service = Service::builder("api-gateway")
            .namespace("default")
            .session_affinity(true)
            .load_balancer(LoadBalancerStrategy::RoundRobin)
            .build();

        registry.register(service).ok();

        registry.add_endpoint("default", "api-gateway", make_endpoint("10.0.0.1", 8080)).ok();
        registry.add_endpoint("default", "api-gateway", make_endpoint("10.0.0.2", 8080)).ok();

        let client_ip: std::net::IpAddr = "192.168.1.100".parse().ok().unwrap();

        // First selection
        let first = registry.select_endpoint("default", "api-gateway", Some(client_ip)).ok();
        let first_id = first.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).id;

        // Subsequent selections should return same endpoint
        for _ in 0..5 {
            let selected = registry.select_endpoint("default", "api-gateway", Some(client_ip)).ok();
            assert_eq!(selected.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).id, first_id);
        }
    }

    #[test]
    fn test_record_connection() {
        let registry = ServiceRegistry::new();
        let service = make_service("api-gateway");
        registry.register(service).ok();

        let endpoint = make_endpoint("10.0.0.1", 8080);
        let endpoint_id = endpoint.id;
        registry.add_endpoint("default", "api-gateway", endpoint).ok();

        registry.record_connection("default", "api-gateway", endpoint_id);
        registry.record_connection("default", "api-gateway", endpoint_id);

        let endpoints = registry.get_endpoints("default", "api-gateway").unwrap();
        let ep = endpoints.iter().find(|e| e.id == endpoint_id);
        assert_eq!(ep.unwrap_or(&make_endpoint("0.0.0.0", 0)).active_connections, 2);
    }

    // ==================== Service Port Tests ====================

    #[test]
    fn test_get_ports() {
        let registry = ServiceRegistry::new();

        let service = Service::builder("api-gateway")
            .namespace("default")
            .port(ServicePort::http(80).with_name("http"))
            .port(ServicePort::tcp(443).with_name("https"))
            .build();

        registry.register(service).ok();

        let ports = registry.get_ports("default", "api-gateway");

        assert!(ports.is_some());
        assert_eq!(ports.unwrap().len(), 2);
    }

    #[test]
    fn test_get_port_by_name() {
        let registry = ServiceRegistry::new();

        let service = Service::builder("api-gateway")
            .namespace("default")
            .port(ServicePort::http(80).with_name("http"))
            .port(ServicePort::tcp(443).with_name("https"))
            .build();

        registry.register(service).ok();

        let http_port = registry.get_port("default", "api-gateway", "http");
        let https_port = registry.get_port("default", "api-gateway", "https");
        let missing_port = registry.get_port("default", "api-gateway", "grpc");

        assert!(http_port.is_some());
        assert_eq!(http_port.unwrap().port, 80);
        assert!(https_port.is_some());
        assert_eq!(https_port.unwrap().port, 443);
        assert!(missing_port.is_none());
    }

    // ==================== Statistics Tests ====================

    #[test]
    fn test_stats() {
        let registry = ServiceRegistry::new();

        registry.register(make_service_in_namespace("api", "prod")).ok();
        registry.register(make_service_in_namespace("web", "prod")).ok();
        registry.register(make_service_in_namespace("api", "staging")).ok();

        registry.add_endpoint("prod", "api", make_endpoint("10.0.0.1", 8080)).ok();
        registry.add_endpoint("prod", "api", make_endpoint("10.0.0.2", 8080)).ok();
        registry.add_endpoint("prod", "web", make_endpoint("10.0.0.3", 8080)).ok();

        let stats = registry.stats();

        assert_eq!(stats.service_count, 3);
        assert_eq!(stats.namespace_count, 2);
        assert_eq!(stats.total_endpoints, 3);
        assert_eq!(stats.healthy_endpoints, 3);
    }

    // ==================== Error Display Tests ====================

    #[test]
    fn test_error_display() {
        let err = RegistryError::ServiceExists("api".to_string(), "default".to_string());
        assert!(err.to_string().contains("already exists"));

        let err = RegistryError::ServiceNotFound("api".to_string(), "default".to_string());
        assert!(err.to_string().contains("not found"));

        let err = RegistryError::ServiceNotFoundById(ServiceId::new());
        assert!(err.to_string().contains("not found"));

        let err = RegistryError::InvalidServiceName("test".to_string());
        assert!(err.to_string().contains("invalid service name"));

        let err = RegistryError::InvalidNamespace("test".to_string());
        assert!(err.to_string().contains("invalid namespace"));
    }

    // ==================== Thread Safety Tests ====================

    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let registry = Arc::new(ServiceRegistry::new());

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let reg = Arc::clone(&registry);
                thread::spawn(move || {
                    let service = make_service(&format!("service-{i}"));
                    reg.register(service).ok();
                })
            })
            .collect();

        for handle in handles {
            handle.join().ok();
        }

        assert_eq!(registry.len(), 10);
    }
}
