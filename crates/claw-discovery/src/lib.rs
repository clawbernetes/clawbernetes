//! # claw-discovery
//!
//! Service discovery, load balancing, and internal DNS for Clawbernetes.
//!
//! This crate provides the service mesh infrastructure for Clawbernetes, including:
//!
//! - **Service Registry** - Register and discover services
//! - **Endpoint Tracking** - Track healthy backends for each service
//! - **Load Balancing** - Distribute traffic using multiple strategies
//! - **Internal DNS** - Resolve service names to endpoints
//!
//! ## Overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    Service Mesh                          │
//! │                                                          │
//! │  ┌──────────────┐   ┌──────────────┐   ┌─────────────┐  │
//! │  │   Service    │   │   Endpoint   │   │     DNS     │  │
//! │  │   Registry   │──▶│   Tracking   │──▶│   Resolver  │  │
//! │  └──────────────┘   └──────────────┘   └─────────────┘  │
//! │         │                  │                  │          │
//! │         └──────────┬───────┘                  │          │
//! │                    ▼                          │          │
//! │           ┌──────────────┐                   │          │
//! │           │    Load      │◀──────────────────┘          │
//! │           │   Balancer   │                              │
//! │           └──────────────┘                              │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Quick Start
//!
//! ```rust
//! use std::sync::Arc;
//! use claw_discovery::{
//!     ServiceRegistry, DnsResolver, Service, ServicePort, Endpoint, HealthStatus,
//! };
//!
//! // Create the service registry
//! let registry = Arc::new(ServiceRegistry::new());
//!
//! // Register a service
//! let service = Service::builder("api-gateway")
//!     .namespace("production")
//!     .port(ServicePort::http(80))
//!     .build();
//!
//! registry.register(service).expect("register service");
//!
//! // Add endpoints
//! let mut endpoint = Endpoint::builder("10.0.0.1".parse().unwrap(), 8080).build();
//! endpoint.health_status = HealthStatus::Healthy;
//! registry.add_endpoint("production", "api-gateway", endpoint).expect("add endpoint");
//!
//! // Create DNS resolver
//! let resolver = DnsResolver::new(Arc::clone(&registry));
//!
//! // Resolve service name
//! let record = resolver.resolve("api-gateway.production").expect("resolve");
//! println!("Resolved to: {:?}", record.addresses);
//! ```
//!
//! ## Load Balancer Strategies
//!
//! The crate supports multiple load balancing strategies:
//!
//! - **Round Robin** - Distribute requests evenly in order
//! - **Least Connections** - Send to endpoint with fewest active connections
//! - **Random** - Randomly select an endpoint
//! - **Weighted Random** - Select randomly weighted by endpoint weights
//! - **IP Hash** - Consistent hashing based on client IP
//!
//! ```rust
//! use claw_discovery::{LoadBalancer, LoadBalancerStrategy, Endpoint, HealthStatus};
//!
//! // Create load balancer with strategy
//! let lb = LoadBalancer::new(LoadBalancerStrategy::LeastConnections);
//!
//! // Add endpoints
//! let mut e1 = Endpoint::builder("10.0.0.1".parse().unwrap(), 8080).build();
//! e1.health_status = HealthStatus::Healthy;
//! lb.add_endpoint(e1);
//!
//! // Select endpoint
//! let selected = lb.select(None).expect("select endpoint");
//! println!("Selected: {}", selected.socket_addr());
//! ```
//!
//! ## DNS Resolution
//!
//! The DNS resolver supports multiple name formats:
//!
//! - `<service>` - Service in default namespace
//! - `<service>.<namespace>` - Service in specific namespace
//! - `<service>.<namespace>.svc` - Standard service format
//! - `<service>.<namespace>.svc.cluster.local` - Full FQDN
//!
//! ```rust
//! use std::sync::Arc;
//! use claw_discovery::{DnsResolver, ServiceRegistry, DnsConfig};
//!
//! let registry = Arc::new(ServiceRegistry::new());
//! let resolver = DnsResolver::new(registry);
//!
//! // Parse different name formats
//! let simple = resolver.parse_name("api").expect("parse");
//! let qualified = resolver.parse_name("api.production").expect("parse");
//! let fqdn = resolver.parse_name("api.production.svc.cluster.local").expect("parse");
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod dns;
pub mod load_balancer;
pub mod registry;
pub mod types;

// Re-export main types for convenience
pub use dns::{DnsConfig, DnsError, DnsRecord, DnsResolver, ParsedDnsName, SrvEndpoint, SrvRecord};
pub use load_balancer::{LoadBalancer, LoadBalancerError};
pub use registry::{RegistryError, RegistryStats, ServiceRegistry};
pub use types::{
    Endpoint, EndpointBuilder, EndpointId, HealthCheckConfig, HealthStatus, LabelSelector,
    LoadBalancerStrategy, Protocol, Service, ServiceBuilder, ServiceConfig, ServiceId,
    ServicePort,
};

/// Prelude module for convenient imports.
pub mod prelude {
    pub use crate::dns::{DnsConfig, DnsResolver};
    pub use crate::load_balancer::LoadBalancer;
    pub use crate::registry::ServiceRegistry;
    pub use crate::types::{
        Endpoint, HealthStatus, LabelSelector, LoadBalancerStrategy, Protocol, Service,
        ServicePort,
    };
}

/// Library version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    /// Integration test: Full service mesh workflow.
    #[test]
    fn test_full_service_mesh_workflow() {
        // Create the service registry
        let registry = Arc::new(ServiceRegistry::new());

        // 1. Register services
        let api_service = Service::builder("api-gateway")
            .namespace("production")
            .port(ServicePort::http(80).with_name("http"))
            .port(ServicePort::tcp(443).with_name("https"))
            .load_balancer(LoadBalancerStrategy::LeastConnections)
            .label("team", "platform")
            .build();

        let web_service = Service::builder("web-frontend")
            .namespace("production")
            .port(ServicePort::http(80))
            .load_balancer(LoadBalancerStrategy::RoundRobin)
            .build();

        let db_service = Service::builder("postgres")
            .namespace("production")
            .port(ServicePort::tcp(5432).with_name("postgres"))
            .session_affinity(true)
            .build();

        let api_id = registry.register(api_service).ok();
        let web_id = registry.register(web_service).ok();
        let db_id = registry.register(db_service).ok();

        assert!(api_id.is_some());
        assert!(web_id.is_some());
        assert!(db_id.is_some());

        // 2. Add endpoints for api-gateway
        let mut ep1 = Endpoint::builder("10.0.0.1".parse().ok().unwrap(), 8080)
            .node_id("node-1")
            .build();
        ep1.health_status = HealthStatus::Healthy;

        let mut ep2 = Endpoint::builder("10.0.0.2".parse().ok().unwrap(), 8080)
            .node_id("node-2")
            .build();
        ep2.health_status = HealthStatus::Healthy;

        let mut ep3 = Endpoint::builder("10.0.0.3".parse().ok().unwrap(), 8080)
            .node_id("node-3")
            .build();
        ep3.health_status = HealthStatus::Healthy;

        let ep1_id = ep1.id;
        registry.add_endpoint("production", "api-gateway", ep1).ok();
        registry.add_endpoint("production", "api-gateway", ep2).ok();
        registry.add_endpoint("production", "api-gateway", ep3).ok();

        // Add endpoints for web-frontend
        let mut web_ep = Endpoint::builder("10.0.1.1".parse().ok().unwrap(), 80)
            .build();
        web_ep.health_status = HealthStatus::Healthy;
        registry.add_endpoint("production", "web-frontend", web_ep).ok();

        // Add endpoint for postgres (single instance)
        let mut db_ep = Endpoint::builder("10.0.2.1".parse().ok().unwrap(), 5432)
            .build();
        db_ep.health_status = HealthStatus::Healthy;
        registry.add_endpoint("production", "postgres", db_ep).ok();

        // 3. Verify registry state
        assert_eq!(registry.len(), 3);
        assert_eq!(registry.endpoint_count("production", "api-gateway"), Some(3));
        assert_eq!(registry.healthy_endpoint_count("production", "api-gateway"), Some(3));

        // 4. Create DNS resolver
        let resolver = DnsResolver::new(Arc::clone(&registry));

        // 5. Test DNS resolution with different formats
        let simple = resolver.resolve("postgres.production");
        assert!(simple.is_ok());

        let fqdn = resolver.resolve("api-gateway.production.svc.cluster.local");
        assert!(fqdn.is_ok());

        // 6. Test load balancing
        let client_ip: std::net::IpAddr = "192.168.1.100".parse().ok().unwrap();

        // Least connections for api-gateway
        let selected = registry.select_endpoint("production", "api-gateway", Some(client_ip));
        assert!(selected.is_ok());

        // Record connections to verify least-connections works
        registry.record_connection("production", "api-gateway", ep1_id);
        registry.record_connection("production", "api-gateway", ep1_id);

        // Next selection should prefer other endpoints
        let selected2 = registry.select_endpoint("production", "api-gateway", None);
        assert!(selected2.is_ok());

        // 7. Test session affinity for database
        let db_selected1 = registry.select_endpoint("production", "postgres", Some(client_ip));
        let db_selected2 = registry.select_endpoint("production", "postgres", Some(client_ip));

        // Should get same endpoint due to session affinity
        assert_eq!(
            db_selected1.ok().unwrap().id,
            db_selected2.ok().unwrap().id
        );

        // 8. Test health updates
        registry.update_endpoint_health("production", "api-gateway", ep1_id, HealthStatus::Unhealthy).ok();
        assert_eq!(registry.healthy_endpoint_count("production", "api-gateway"), Some(2));

        // 9. Test SRV record lookup
        let srv = resolver.resolve_srv("_http._tcp.api-gateway.production");
        assert!(srv.is_ok());
        let srv_record = srv.ok().unwrap();
        assert_eq!(srv_record.endpoints.len(), 2); // Only 2 healthy now

        // 10. Verify stats
        let stats = registry.stats();
        assert_eq!(stats.service_count, 3);
        assert_eq!(stats.namespace_count, 1);
        assert_eq!(stats.total_endpoints, 5);
        assert_eq!(stats.healthy_endpoints, 4); // 2 api + 1 web + 1 db

        // 11. Find services by label
        let selector = LabelSelector::new().with_label("team", "platform");
        let platform_services = registry.find_by_labels(&selector);
        assert_eq!(platform_services.len(), 1);

        // 12. Deregister a service
        let removed = registry.deregister("production", "web-frontend");
        assert!(removed.is_ok());
        assert_eq!(registry.len(), 2);
    }

    /// Integration test: Multiple namespaces.
    #[test]
    fn test_multiple_namespaces() {
        let registry = Arc::new(ServiceRegistry::new());

        // Same service name in different namespaces
        let prod_api = Service::builder("api")
            .namespace("production")
            .port(ServicePort::http(80))
            .build();

        let staging_api = Service::builder("api")
            .namespace("staging")
            .port(ServicePort::http(80))
            .build();

        let dev_api = Service::builder("api")
            .namespace("development")
            .port(ServicePort::http(80))
            .build();

        registry.register(prod_api).ok();
        registry.register(staging_api).ok();
        registry.register(dev_api).ok();

        // Add endpoints
        let mut prod_ep = Endpoint::builder("10.0.0.1".parse().ok().unwrap(), 80).build();
        prod_ep.health_status = HealthStatus::Healthy;

        let mut staging_ep = Endpoint::builder("10.0.1.1".parse().ok().unwrap(), 80).build();
        staging_ep.health_status = HealthStatus::Healthy;

        let mut dev_ep = Endpoint::builder("10.0.2.1".parse().ok().unwrap(), 80).build();
        dev_ep.health_status = HealthStatus::Healthy;

        registry.add_endpoint("production", "api", prod_ep).ok();
        registry.add_endpoint("staging", "api", staging_ep).ok();
        registry.add_endpoint("development", "api", dev_ep).ok();

        // DNS resolver should distinguish between namespaces
        let resolver = DnsResolver::new(Arc::clone(&registry));

        let prod_record = resolver.resolve("api.production").ok().unwrap();
        let staging_record = resolver.resolve("api.staging").ok().unwrap();
        let dev_record = resolver.resolve("api.development").ok().unwrap();

        // All should resolve to different IPs
        assert_ne!(
            prod_record.first_address(),
            staging_record.first_address()
        );
        assert_ne!(
            staging_record.first_address(),
            dev_record.first_address()
        );
    }

    /// Integration test: Health check workflow.
    #[test]
    fn test_health_check_workflow() {
        let registry = Arc::new(ServiceRegistry::new());

        let service = Service::builder("backend")
            .namespace("default")
            .port(ServicePort::http(8080))
            .build();

        registry.register(service).ok();

        // Add endpoint in Unknown state
        let endpoint = Endpoint::builder("10.0.0.1".parse().ok().unwrap(), 8080).build();
        let ep_id = endpoint.id;
        registry.add_endpoint("default", "backend", endpoint).ok();

        // Initially unknown - not considered healthy
        assert_eq!(registry.healthy_endpoint_count("default", "backend"), Some(0));

        // Simulate health check passing
        registry.update_endpoint_health("default", "backend", ep_id, HealthStatus::Healthy).ok();
        assert_eq!(registry.healthy_endpoint_count("default", "backend"), Some(1));

        // Can now resolve
        let resolver = DnsResolver::new(Arc::clone(&registry));
        assert!(resolver.can_resolve("backend"));

        // Simulate health check failing
        registry.update_endpoint_health("default", "backend", ep_id, HealthStatus::Unhealthy).ok();
        assert_eq!(registry.healthy_endpoint_count("default", "backend"), Some(0));

        // Cannot resolve when no healthy endpoints
        assert!(!resolver.can_resolve("backend"));

        // Set to draining
        registry.update_endpoint_health("default", "backend", ep_id, HealthStatus::Draining).ok();
        assert!(!resolver.can_resolve("backend"));
    }

    /// Integration test: Load balancer strategies.
    #[test]
    fn test_load_balancer_strategies() {
        use std::collections::HashMap;

        // Test each strategy
        let strategies = vec![
            LoadBalancerStrategy::RoundRobin,
            LoadBalancerStrategy::Random,
            LoadBalancerStrategy::LeastConnections,
            LoadBalancerStrategy::WeightedRandom,
            LoadBalancerStrategy::IpHash,
        ];

        for strategy in strategies {
            let lb = LoadBalancer::new(strategy.clone());

            // Add endpoints
            for i in 1..=3 {
                let mut ep = Endpoint::builder(
                    format!("10.0.0.{i}").parse().ok().unwrap(),
                    8080,
                )
                .weight(100)
                .build();
                ep.health_status = HealthStatus::Healthy;
                lb.add_endpoint(ep);
            }

            // Make selections
            let mut selections: HashMap<EndpointId, usize> = HashMap::new();
            for _ in 0..100 {
                let selected = lb.select(Some("192.168.1.1".parse().ok().unwrap())).ok().unwrap();
                *selections.entry(selected.id).or_insert(0) += 1;
            }

            // All strategies should work
            assert!(!selections.is_empty(), "Strategy {:?} failed to select", strategy);

            // IP hash should be consistent
            if matches!(strategy, LoadBalancerStrategy::IpHash) {
                // All selections should be the same endpoint
                assert_eq!(selections.len(), 1, "IP hash should be consistent");
            }
        }
    }

    /// Integration test: Service discovery with selector.
    #[test]
    fn test_service_selector_matching() {
        let registry = Arc::new(ServiceRegistry::new());

        // Create services with different labels
        let frontend = Service::builder("frontend")
            .namespace("default")
            .port(ServicePort::http(80))
            .label("tier", "frontend")
            .label("env", "production")
            .build();

        let backend = Service::builder("backend")
            .namespace("default")
            .port(ServicePort::http(8080))
            .label("tier", "backend")
            .label("env", "production")
            .build();

        let cache = Service::builder("cache")
            .namespace("default")
            .port(ServicePort::tcp(6379))
            .label("tier", "cache")
            .label("env", "production")
            .build();

        registry.register(frontend).ok();
        registry.register(backend).ok();
        registry.register(cache).ok();

        // Find by single label
        let prod_services = registry.find_by_labels(
            &LabelSelector::new().with_label("env", "production")
        );
        assert_eq!(prod_services.len(), 3);

        // Find by multiple labels
        let backend_services = registry.find_by_labels(
            &LabelSelector::new()
                .with_label("tier", "backend")
                .with_label("env", "production")
        );
        assert_eq!(backend_services.len(), 1);
        assert_eq!(backend_services[0].name, "backend");
    }
}
