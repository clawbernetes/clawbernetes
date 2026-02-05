//! Internal DNS resolution for service discovery.
//!
//! Provides DNS-style name resolution for services within the cluster.
//! Supports multiple DNS formats:
//!
//! - `<service>` - Service in default namespace
//! - `<service>.<namespace>` - Service in specific namespace
//! - `<service>.<namespace>.svc` - Standard service format
//! - `<service>.<namespace>.svc.cluster.local` - Full FQDN

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use thiserror::Error;
use tracing::{debug, trace};

use crate::registry::ServiceRegistry;
use crate::types::Endpoint;

/// Errors that can occur during DNS resolution.
#[derive(Debug, Error)]
pub enum DnsError {
    /// Service not found.
    #[error("service not found: {0}")]
    ServiceNotFound(String),

    /// No healthy endpoints available.
    #[error("no healthy endpoints for service: {0}")]
    NoHealthyEndpoints(String),

    /// Invalid DNS name format.
    #[error("invalid DNS name format: {0}")]
    InvalidFormat(String),

    /// Port not found.
    #[error("port '{port}' not found for service '{service}'")]
    PortNotFound {
        /// Service name.
        service: String,
        /// Port name.
        port: String,
    },

    /// Namespace not found.
    #[error("namespace not found: {0}")]
    NamespaceNotFound(String),
}

/// Result type for DNS operations.
pub type Result<T> = std::result::Result<T, DnsError>;

/// The cluster domain suffix.
pub const CLUSTER_DOMAIN: &str = "cluster.local";

/// The service subdomain.
pub const SERVICE_SUBDOMAIN: &str = "svc";

/// Configuration for the DNS resolver.
#[derive(Debug, Clone)]
pub struct DnsConfig {
    /// Default namespace for unqualified names.
    pub default_namespace: String,
    /// Cluster domain.
    pub cluster_domain: String,
    /// Whether to return all endpoints or just one.
    pub return_all_endpoints: bool,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            default_namespace: "default".to_string(),
            cluster_domain: CLUSTER_DOMAIN.to_string(),
            return_all_endpoints: false,
        }
    }
}

/// A parsed DNS name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDnsName {
    /// Service name.
    pub service: String,
    /// Namespace.
    pub namespace: String,
    /// Port name (if specified via SRV record style).
    pub port_name: Option<String>,
}

impl ParsedDnsName {
    /// Returns the full FQDN for this name.
    #[must_use]
    pub fn fqdn(&self, cluster_domain: &str) -> String {
        format!(
            "{}.{}.{}.{}",
            self.service, self.namespace, SERVICE_SUBDOMAIN, cluster_domain
        )
    }

    /// Returns the short name (service.namespace).
    #[must_use]
    pub fn short_name(&self) -> String {
        format!("{}.{}", self.service, self.namespace)
    }
}

/// Result of a DNS lookup.
#[derive(Debug, Clone)]
pub struct DnsRecord {
    /// The resolved addresses.
    pub addresses: Vec<IpAddr>,
    /// The service name that was resolved.
    pub service_name: String,
    /// The namespace.
    pub namespace: String,
    /// TTL in seconds.
    pub ttl: u32,
}

impl DnsRecord {
    /// Returns true if the record has addresses.
    #[must_use]
    pub fn has_addresses(&self) -> bool {
        !self.addresses.is_empty()
    }

    /// Returns the first address, if any.
    #[must_use]
    pub fn first_address(&self) -> Option<IpAddr> {
        self.addresses.first().copied()
    }
}

/// Result of an SRV lookup.
#[derive(Debug, Clone)]
pub struct SrvRecord {
    /// The resolved endpoints with priorities and weights.
    pub endpoints: Vec<SrvEndpoint>,
    /// The service name that was resolved.
    pub service_name: String,
    /// The namespace.
    pub namespace: String,
    /// TTL in seconds.
    pub ttl: u32,
}

/// An endpoint from an SRV record.
#[derive(Debug, Clone)]
pub struct SrvEndpoint {
    /// Hostname (usually an IP address in our case).
    pub target: IpAddr,
    /// Port number.
    pub port: u16,
    /// Priority (lower is preferred).
    pub priority: u16,
    /// Weight for load balancing among same priority.
    pub weight: u16,
}

impl SrvEndpoint {
    /// Returns the socket address.
    #[must_use]
    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.target, self.port)
    }
}

/// DNS resolver for internal service discovery.
#[derive(Debug)]
pub struct DnsResolver {
    /// Service registry to resolve names from.
    registry: Arc<ServiceRegistry>,
    /// Configuration.
    config: DnsConfig,
}

impl DnsResolver {
    /// Creates a new DNS resolver with the given registry.
    pub fn new(registry: Arc<ServiceRegistry>) -> Self {
        Self {
            registry,
            config: DnsConfig::default(),
        }
    }

    /// Creates a new DNS resolver with custom configuration.
    pub fn with_config(registry: Arc<ServiceRegistry>, config: DnsConfig) -> Self {
        Self { registry, config }
    }

    /// Returns the configuration.
    #[must_use]
    pub fn config(&self) -> &DnsConfig {
        &self.config
    }

    /// Parses a DNS name into its components.
    ///
    /// # Errors
    ///
    /// Returns an error if the name format is invalid.
    pub fn parse_name(&self, name: &str) -> Result<ParsedDnsName> {
        let name = name.trim().to_lowercase();

        if name.is_empty() {
            return Err(DnsError::InvalidFormat("empty name".to_string()));
        }

        // Split by dots
        let parts: Vec<&str> = name.split('.').collect();

        match parts.len() {
            // <service> - use default namespace
            1 => Ok(ParsedDnsName {
                service: parts[0].to_string(),
                namespace: self.config.default_namespace.clone(),
                port_name: None,
            }),

            // <service>.<namespace>
            2 => Ok(ParsedDnsName {
                service: parts[0].to_string(),
                namespace: parts[1].to_string(),
                port_name: None,
            }),

            // <service>.<namespace>.svc
            3 if parts[2] == SERVICE_SUBDOMAIN => Ok(ParsedDnsName {
                service: parts[0].to_string(),
                namespace: parts[1].to_string(),
                port_name: None,
            }),

            // <service>.<namespace>.svc.cluster.local (or custom domain)
            5 if parts[2] == SERVICE_SUBDOMAIN => {
                let domain = format!("{}.{}", parts[3], parts[4]);
                if domain != self.config.cluster_domain {
                    return Err(DnsError::InvalidFormat(format!(
                        "unknown cluster domain: {domain}"
                    )));
                }
                Ok(ParsedDnsName {
                    service: parts[0].to_string(),
                    namespace: parts[1].to_string(),
                    port_name: None,
                })
            }

            // _<port>._<proto>.<service>.<namespace>.svc... (SRV style)
            n if n >= 4 && parts[0].starts_with('_') && parts[1].starts_with('_') => {
                let port_name = parts[0].trim_start_matches('_').to_string();
                let _proto = parts[1].trim_start_matches('_'); // tcp, udp, etc.
                let service = parts[2].to_string();
                let namespace = parts[3].to_string();

                Ok(ParsedDnsName {
                    service,
                    namespace,
                    port_name: Some(port_name),
                })
            }

            _ => Err(DnsError::InvalidFormat(format!(
                "unrecognized format: {name}"
            ))),
        }
    }

    /// Resolves a DNS name to IP addresses (A record lookup).
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not found or has no healthy endpoints.
    pub fn resolve(&self, name: &str) -> Result<DnsRecord> {
        let parsed = self.parse_name(name)?;

        debug!(
            name = %name,
            service = %parsed.service,
            namespace = %parsed.namespace,
            "Resolving DNS name"
        );

        // Get healthy endpoints
        let endpoints = self
            .registry
            .get_healthy_endpoints(&parsed.namespace, &parsed.service)
            .map_err(|_| DnsError::ServiceNotFound(parsed.short_name()))?;

        if endpoints.is_empty() {
            return Err(DnsError::NoHealthyEndpoints(parsed.short_name()));
        }

        let addresses: Vec<IpAddr> = if self.config.return_all_endpoints {
            endpoints.iter().map(|e| e.address).collect()
        } else {
            // Return just the first (or use load balancer)
            let selected = self
                .registry
                .select_endpoint(&parsed.namespace, &parsed.service, None)
                .map_err(|_| DnsError::NoHealthyEndpoints(parsed.short_name()))?;
            vec![selected.address]
        };

        trace!(
            name = %name,
            addresses = ?addresses,
            "DNS resolution complete"
        );

        Ok(DnsRecord {
            addresses,
            service_name: parsed.service,
            namespace: parsed.namespace,
            ttl: 30, // Default TTL
        })
    }

    /// Resolves a DNS name to a single IP address.
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not found or has no healthy endpoints.
    pub fn resolve_one(&self, name: &str) -> Result<IpAddr> {
        let record = self.resolve(name)?;
        record
            .first_address()
            .ok_or_else(|| DnsError::NoHealthyEndpoints(name.to_string()))
    }

    /// Resolves a DNS name to a socket address (combines A lookup with port).
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not found or has no healthy endpoints.
    pub fn resolve_addr(&self, name: &str, port: u16) -> Result<SocketAddr> {
        let ip = self.resolve_one(name)?;
        Ok(SocketAddr::new(ip, port))
    }

    /// Performs an SRV record lookup.
    ///
    /// # Errors
    ///
    /// Returns an error if the service or port is not found.
    pub fn resolve_srv(&self, name: &str) -> Result<SrvRecord> {
        let parsed = self.parse_name(name)?;

        debug!(
            name = %name,
            service = %parsed.service,
            namespace = %parsed.namespace,
            port_name = ?parsed.port_name,
            "Resolving SRV record"
        );

        // Get the service to find the port
        let service = self
            .registry
            .get(&parsed.namespace, &parsed.service)
            .ok_or_else(|| DnsError::ServiceNotFound(parsed.short_name()))?;

        // Find the target port
        let target_port = match &parsed.port_name {
            Some(port_name) => {
                let port = service
                    .ports
                    .iter()
                    .find(|p| p.name.as_deref() == Some(port_name))
                    .ok_or_else(|| DnsError::PortNotFound {
                        service: parsed.short_name(),
                        port: port_name.clone(),
                    })?;
                port.effective_target_port()
            }
            None => {
                // Use first port
                service
                    .ports
                    .first()
                    .map(super::types::ServicePort::effective_target_port)
                    .ok_or_else(|| DnsError::PortNotFound {
                        service: parsed.short_name(),
                        port: "default".to_string(),
                    })?
            }
        };

        // Get healthy endpoints
        let endpoints = self
            .registry
            .get_healthy_endpoints(&parsed.namespace, &parsed.service)
            .map_err(|_| DnsError::ServiceNotFound(parsed.short_name()))?;

        if endpoints.is_empty() {
            return Err(DnsError::NoHealthyEndpoints(parsed.short_name()));
        }

        let srv_endpoints: Vec<SrvEndpoint> = endpoints
            .iter()
            .map(|e| SrvEndpoint {
                target: e.address,
                port: target_port,
                priority: 0,
                weight: e.weight.try_into().unwrap_or(u16::MAX),
            })
            .collect();

        Ok(SrvRecord {
            endpoints: srv_endpoints,
            service_name: parsed.service,
            namespace: parsed.namespace,
            ttl: 30,
        })
    }

    /// Resolves a service name to all healthy endpoints.
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not found.
    pub fn resolve_endpoints(&self, name: &str) -> Result<Vec<Endpoint>> {
        let parsed = self.parse_name(name)?;

        self.registry
            .get_healthy_endpoints(&parsed.namespace, &parsed.service)
            .map_err(|_| DnsError::ServiceNotFound(parsed.short_name()))
    }

    /// Checks if a DNS name can be resolved.
    #[must_use]
    pub fn can_resolve(&self, name: &str) -> bool {
        self.resolve(name).is_ok()
    }

    /// Lists all resolvable service names in a namespace.
    #[must_use]
    pub fn list_services(&self, namespace: &str) -> Vec<String> {
        self.registry
            .list_in_namespace(namespace)
            .iter()
            .map(|s| s.name.clone())
            .collect()
    }

    /// Lists all resolvable FQDNs in a namespace.
    #[must_use]
    pub fn list_fqdns(&self, namespace: &str) -> Vec<String> {
        self.registry
            .list_in_namespace(namespace)
            .iter()
            .map(super::types::Service::fqdn)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{HealthStatus, Service, ServicePort};

    // ==================== Helper Functions ====================

    fn make_registry() -> Arc<ServiceRegistry> {
        Arc::new(ServiceRegistry::new())
    }

    fn make_service(name: &str, namespace: &str) -> Service {
        Service::builder(name)
            .namespace(namespace)
            .port(ServicePort::http(80).with_name("http"))
            .port(ServicePort::tcp(443).with_name("https"))
            .build()
    }

    fn make_endpoint(ip: &str, port: u16) -> Endpoint {
        let mut endpoint = Endpoint::builder(ip.parse().ok().unwrap(), port).build();
        endpoint.health_status = HealthStatus::Healthy;
        endpoint
    }

    fn setup_resolver() -> DnsResolver {
        let registry = make_registry();

        // Register some services
        let api = make_service("api", "production");
        let web = make_service("web", "production");
        let db = make_service("database", "default");

        registry.register(api).ok();
        registry.register(web).ok();
        registry.register(db).ok();

        // Add endpoints
        registry.add_endpoint("production", "api", make_endpoint("10.0.0.1", 8080)).ok();
        registry.add_endpoint("production", "api", make_endpoint("10.0.0.2", 8080)).ok();
        registry.add_endpoint("production", "web", make_endpoint("10.0.1.1", 80)).ok();
        registry.add_endpoint("default", "database", make_endpoint("10.0.2.1", 5432)).ok();

        DnsResolver::new(registry)
    }

    // ==================== Parse Name Tests ====================

    #[test]
    fn test_parse_simple_name() {
        let resolver = setup_resolver();

        let parsed = resolver.parse_name("api").ok().unwrap();

        assert_eq!(parsed.service, "api");
        assert_eq!(parsed.namespace, "default");
        assert!(parsed.port_name.is_none());
    }

    #[test]
    fn test_parse_name_with_namespace() {
        let resolver = setup_resolver();

        let parsed = resolver.parse_name("api.production").ok().unwrap();

        assert_eq!(parsed.service, "api");
        assert_eq!(parsed.namespace, "production");
    }

    #[test]
    fn test_parse_name_with_svc() {
        let resolver = setup_resolver();

        let parsed = resolver.parse_name("api.production.svc").ok().unwrap();

        assert_eq!(parsed.service, "api");
        assert_eq!(parsed.namespace, "production");
    }

    #[test]
    fn test_parse_fqdn() {
        let resolver = setup_resolver();

        let parsed = resolver
            .parse_name("api.production.svc.cluster.local")
            .ok()
            .unwrap();

        assert_eq!(parsed.service, "api");
        assert_eq!(parsed.namespace, "production");
    }

    #[test]
    fn test_parse_srv_style_name() {
        let resolver = setup_resolver();

        let parsed = resolver
            .parse_name("_http._tcp.api.production")
            .ok()
            .unwrap();

        assert_eq!(parsed.service, "api");
        assert_eq!(parsed.namespace, "production");
        assert_eq!(parsed.port_name, Some("http".to_string()));
    }

    #[test]
    fn test_parse_empty_name_fails() {
        let resolver = setup_resolver();

        let result = resolver.parse_name("");
        assert!(matches!(result, Err(DnsError::InvalidFormat(_))));
    }

    #[test]
    fn test_parse_invalid_domain_fails() {
        let resolver = setup_resolver();

        let result = resolver.parse_name("api.production.svc.other.domain");
        assert!(matches!(result, Err(DnsError::InvalidFormat(_))));
    }

    #[test]
    fn test_parse_name_case_insensitive() {
        let resolver = setup_resolver();

        let parsed = resolver.parse_name("API.Production").ok().unwrap();

        assert_eq!(parsed.service, "api");
        assert_eq!(parsed.namespace, "production");
    }

    // ==================== Parsed DNS Name Tests ====================

    #[test]
    fn test_parsed_name_fqdn() {
        let name = ParsedDnsName {
            service: "api".to_string(),
            namespace: "production".to_string(),
            port_name: None,
        };

        assert_eq!(name.fqdn("cluster.local"), "api.production.svc.cluster.local");
    }

    #[test]
    fn test_parsed_name_short_name() {
        let name = ParsedDnsName {
            service: "api".to_string(),
            namespace: "production".to_string(),
            port_name: None,
        };

        assert_eq!(name.short_name(), "api.production");
    }

    // ==================== Resolve Tests ====================

    #[test]
    fn test_resolve_simple_name() {
        let resolver = setup_resolver();

        let record = resolver.resolve("database").ok().unwrap();

        assert!(record.has_addresses());
        assert_eq!(record.service_name, "database");
        assert_eq!(record.namespace, "default");
    }

    #[test]
    fn test_resolve_with_namespace() {
        let resolver = setup_resolver();

        let record = resolver.resolve("api.production").ok().unwrap();

        assert!(record.has_addresses());
        assert_eq!(record.service_name, "api");
        assert_eq!(record.namespace, "production");
    }

    #[test]
    fn test_resolve_fqdn() {
        let resolver = setup_resolver();

        let record = resolver
            .resolve("api.production.svc.cluster.local")
            .ok()
            .unwrap();

        assert!(record.has_addresses());
    }

    #[test]
    fn test_resolve_nonexistent_service() {
        let resolver = setup_resolver();

        let result = resolver.resolve("nonexistent");
        assert!(matches!(result, Err(DnsError::ServiceNotFound(_))));
    }

    #[test]
    fn test_resolve_service_with_no_endpoints() {
        let registry = make_registry();
        let service = make_service("empty", "default");
        registry.register(service).ok();

        let resolver = DnsResolver::new(registry);

        let result = resolver.resolve("empty");
        assert!(matches!(result, Err(DnsError::NoHealthyEndpoints(_))));
    }

    #[test]
    fn test_resolve_one() {
        let resolver = setup_resolver();

        let ip = resolver.resolve_one("api.production").ok().unwrap();

        // Should be one of the registered IPs
        let ip_str = ip.to_string();
        assert!(ip_str == "10.0.0.1" || ip_str == "10.0.0.2");
    }

    #[test]
    fn test_resolve_addr() {
        let resolver = setup_resolver();

        let addr = resolver.resolve_addr("api.production", 8080).ok().unwrap();

        assert_eq!(addr.port(), 8080);
    }

    #[test]
    fn test_resolve_all_endpoints() {
        let registry = make_registry();
        let service = make_service("api", "production");
        registry.register(service).ok();

        registry.add_endpoint("production", "api", make_endpoint("10.0.0.1", 8080)).ok();
        registry.add_endpoint("production", "api", make_endpoint("10.0.0.2", 8080)).ok();

        let config = DnsConfig {
            return_all_endpoints: true,
            ..Default::default()
        };

        let resolver = DnsResolver::with_config(registry, config);

        let record = resolver.resolve("api.production").ok().unwrap();

        assert_eq!(record.addresses.len(), 2);
    }

    // ==================== SRV Record Tests ====================

    #[test]
    fn test_resolve_srv() {
        let resolver = setup_resolver();

        let srv = resolver.resolve_srv("api.production").ok().unwrap();

        assert!(!srv.endpoints.is_empty());
        assert_eq!(srv.service_name, "api");
        assert_eq!(srv.namespace, "production");
    }

    #[test]
    fn test_resolve_srv_with_port_name() {
        let resolver = setup_resolver();

        let srv = resolver.resolve_srv("_http._tcp.api.production").ok().unwrap();

        assert!(!srv.endpoints.is_empty());
        // Port should be the HTTP port (80)
        assert!(srv.endpoints.iter().all(|e| e.port == 80));
    }

    #[test]
    fn test_resolve_srv_invalid_port() {
        let resolver = setup_resolver();

        let result = resolver.resolve_srv("_grpc._tcp.api.production");
        assert!(matches!(result, Err(DnsError::PortNotFound { .. })));
    }

    #[test]
    fn test_srv_endpoint_socket_addr() {
        let endpoint = SrvEndpoint {
            target: "10.0.0.1".parse().ok().unwrap(),
            port: 8080,
            priority: 0,
            weight: 100,
        };

        assert_eq!(endpoint.socket_addr().to_string(), "10.0.0.1:8080");
    }

    // ==================== Resolve Endpoints Tests ====================

    #[test]
    fn test_resolve_endpoints() {
        let resolver = setup_resolver();

        let endpoints = resolver.resolve_endpoints("api.production").ok().unwrap();

        assert_eq!(endpoints.len(), 2);
    }

    // ==================== Utility Method Tests ====================

    #[test]
    fn test_can_resolve() {
        let resolver = setup_resolver();

        assert!(resolver.can_resolve("api.production"));
        assert!(resolver.can_resolve("database"));
        assert!(!resolver.can_resolve("nonexistent"));
    }

    #[test]
    fn test_list_services() {
        let resolver = setup_resolver();

        let services = resolver.list_services("production");

        assert_eq!(services.len(), 2);
        assert!(services.contains(&"api".to_string()));
        assert!(services.contains(&"web".to_string()));
    }

    #[test]
    fn test_list_fqdns() {
        let resolver = setup_resolver();

        let fqdns = resolver.list_fqdns("production");

        assert_eq!(fqdns.len(), 2);
        assert!(fqdns.iter().any(|f| f.contains("api.production")));
    }

    // ==================== DNS Record Tests ====================

    #[test]
    fn test_dns_record_has_addresses() {
        let record = DnsRecord {
            addresses: vec!["10.0.0.1".parse().ok().unwrap()],
            service_name: "api".to_string(),
            namespace: "default".to_string(),
            ttl: 30,
        };

        assert!(record.has_addresses());
    }

    #[test]
    fn test_dns_record_empty() {
        let record = DnsRecord {
            addresses: vec![],
            service_name: "api".to_string(),
            namespace: "default".to_string(),
            ttl: 30,
        };

        assert!(!record.has_addresses());
        assert!(record.first_address().is_none());
    }

    #[test]
    fn test_dns_record_first_address() {
        let record = DnsRecord {
            addresses: vec![
                "10.0.0.1".parse().ok().unwrap(),
                "10.0.0.2".parse().ok().unwrap(),
            ],
            service_name: "api".to_string(),
            namespace: "default".to_string(),
            ttl: 30,
        };

        assert_eq!(record.first_address().unwrap().to_string(), "10.0.0.1");
    }

    // ==================== Config Tests ====================

    #[test]
    fn test_default_config() {
        let config = DnsConfig::default();

        assert_eq!(config.default_namespace, "default");
        assert_eq!(config.cluster_domain, "cluster.local");
        assert!(!config.return_all_endpoints);
    }

    #[test]
    fn test_custom_config() {
        let registry = make_registry();
        let service = make_service("api", "custom-ns");
        registry.register(service).ok();
        registry.add_endpoint("custom-ns", "api", make_endpoint("10.0.0.1", 8080)).ok();

        let config = DnsConfig {
            default_namespace: "custom-ns".to_string(),
            ..Default::default()
        };

        let resolver = DnsResolver::with_config(registry, config);

        // Simple name should resolve in custom namespace
        let record = resolver.resolve("api").ok().unwrap();
        assert_eq!(record.namespace, "custom-ns");
    }

    // ==================== Error Display Tests ====================

    #[test]
    fn test_error_display() {
        let err = DnsError::ServiceNotFound("api.default".to_string());
        assert!(err.to_string().contains("not found"));

        let err = DnsError::NoHealthyEndpoints("api.default".to_string());
        assert!(err.to_string().contains("no healthy"));

        let err = DnsError::InvalidFormat("test".to_string());
        assert!(err.to_string().contains("invalid"));

        let err = DnsError::PortNotFound {
            service: "api".to_string(),
            port: "grpc".to_string(),
        };
        assert!(err.to_string().contains("port"));
    }
}
