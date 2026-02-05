//! End-to-end tests for Service Discovery (claw-discovery).
//!
//! These tests verify:
//! 1. Service registration and deregistration
//! 2. Endpoint management
//! 3. Health status tracking
//! 4. DNS resolution
//! 5. Load balancing strategies
//! 6. Service mesh workflows

use std::collections::HashMap;
use std::sync::Arc;
use claw_discovery::{
    DnsConfig, DnsRecord, DnsResolver, Endpoint, EndpointId, HealthCheckConfig,
    HealthStatus, LabelSelector, LoadBalancer, LoadBalancerStrategy, Protocol,
    Service, ServicePort, ServiceRegistry, SrvRecord,
};

// ============================================================================
// Service Registry: Basic Operations
// ============================================================================

#[test]
fn test_service_registry_creation() {
    let registry = ServiceRegistry::new();
    assert_eq!(registry.len(), 0);
}

#[test]
fn test_service_registration() {
    let registry = Arc::new(ServiceRegistry::new());

    let service = Service::builder("api-gateway")
        .namespace("production")
        .port(ServicePort::http(80))
        .build();

    let result = registry.register(service);
    assert!(result.is_ok());
    assert_eq!(registry.len(), 1);
}

#[test]
fn test_service_with_multiple_ports() {
    let registry = Arc::new(ServiceRegistry::new());

    let service = Service::builder("web-server")
        .namespace("default")
        .port(ServicePort::http(80).with_name("http"))
        .port(ServicePort::tcp(443).with_name("https"))
        .port(ServicePort::tcp(9090).with_name("metrics"))
        .build();

    registry.register(service).unwrap();

    let retrieved = registry.get("default", "web-server");
    assert!(retrieved.is_some());
}

#[test]
fn test_service_with_labels() {
    let registry = Arc::new(ServiceRegistry::new());

    let service = Service::builder("labeled-service")
        .namespace("default")
        .port(ServicePort::http(8080))
        .label("team", "platform")
        .label("tier", "backend")
        .label("env", "production")
        .build();

    registry.register(service).unwrap();

    // Find by labels
    let selector = LabelSelector::new()
        .with_label("team", "platform");

    let services = registry.find_by_labels(&selector);
    assert_eq!(services.len(), 1);
    assert_eq!(services[0].name, "labeled-service");
}

#[test]
fn test_service_deregistration() {
    let registry = Arc::new(ServiceRegistry::new());

    let service = Service::builder("temporary")
        .namespace("default")
        .port(ServicePort::http(80))
        .build();

    registry.register(service).unwrap();
    assert_eq!(registry.len(), 1);

    // Deregister
    let removed = registry.deregister("default", "temporary");
    assert!(removed.is_ok());
    assert_eq!(registry.len(), 0);
}

#[test]
fn test_duplicate_service_rejected() {
    let registry = Arc::new(ServiceRegistry::new());

    let service1 = Service::builder("unique-service")
        .namespace("default")
        .port(ServicePort::http(80))
        .build();

    let service2 = Service::builder("unique-service")
        .namespace("default")
        .port(ServicePort::http(8080))
        .build();

    assert!(registry.register(service1).is_ok());
    assert!(registry.register(service2).is_err()); // Duplicate
}

// ============================================================================
// Endpoint Management
// ============================================================================

#[test]
fn test_endpoint_addition() {
    let registry = Arc::new(ServiceRegistry::new());

    // Register service
    let service = Service::builder("backend")
        .namespace("default")
        .port(ServicePort::http(8080))
        .build();
    registry.register(service).unwrap();

    // Add endpoint
    let mut endpoint = Endpoint::builder("10.0.0.1".parse().unwrap(), 8080).build();
    endpoint.health_status = HealthStatus::Healthy;

    let result = registry.add_endpoint("default", "backend", endpoint);
    assert!(result.is_ok());
    assert_eq!(registry.endpoint_count("default", "backend"), Some(1));
}

#[test]
fn test_multiple_endpoints() {
    let registry = Arc::new(ServiceRegistry::new());

    let service = Service::builder("scalable")
        .namespace("default")
        .port(ServicePort::http(8080))
        .build();
    registry.register(service).unwrap();

    // Add multiple endpoints
    for i in 1..=5 {
        let mut endpoint = Endpoint::builder(
            format!("10.0.0.{}", i).parse().unwrap(),
            8080,
        ).build();
        endpoint.health_status = HealthStatus::Healthy;
        registry.add_endpoint("default", "scalable", endpoint).unwrap();
    }

    assert_eq!(registry.endpoint_count("default", "scalable"), Some(5));
    assert_eq!(registry.healthy_endpoint_count("default", "scalable"), Some(5));
}

#[test]
fn test_endpoint_with_metadata() {
    let registry = Arc::new(ServiceRegistry::new());

    let service = Service::builder("metadata-service")
        .namespace("default")
        .port(ServicePort::http(8080))
        .build();
    registry.register(service).unwrap();

    let endpoint = Endpoint::builder("10.0.0.1".parse().unwrap(), 8080)
        .node_id("node-1")
        .weight(100)
        .build();

    registry.add_endpoint("default", "metadata-service", endpoint).unwrap();
}

#[test]
fn test_endpoint_removal() {
    let registry = Arc::new(ServiceRegistry::new());

    let service = Service::builder("removable")
        .namespace("default")
        .port(ServicePort::http(8080))
        .build();
    registry.register(service).unwrap();

    let endpoint = Endpoint::builder("10.0.0.1".parse().unwrap(), 8080).build();
    let ep_id = endpoint.id;
    registry.add_endpoint("default", "removable", endpoint).unwrap();

    assert_eq!(registry.endpoint_count("default", "removable"), Some(1));

    // Remove endpoint
    registry.remove_endpoint("default", "removable", ep_id).unwrap();
    assert_eq!(registry.endpoint_count("default", "removable"), Some(0));
}

// ============================================================================
// Health Status Tracking
// ============================================================================

#[test]
fn test_health_status_updates() {
    let registry = Arc::new(ServiceRegistry::new());

    let service = Service::builder("health-tracked")
        .namespace("default")
        .port(ServicePort::http(8080))
        .build();
    registry.register(service).unwrap();

    let endpoint = Endpoint::builder("10.0.0.1".parse().unwrap(), 8080).build();
    let ep_id = endpoint.id;
    registry.add_endpoint("default", "health-tracked", endpoint).unwrap();

    // Initially unknown
    assert_eq!(registry.healthy_endpoint_count("default", "health-tracked"), Some(0));

    // Mark healthy
    registry.update_endpoint_health("default", "health-tracked", ep_id, HealthStatus::Healthy).unwrap();
    assert_eq!(registry.healthy_endpoint_count("default", "health-tracked"), Some(1));

    // Mark unhealthy
    registry.update_endpoint_health("default", "health-tracked", ep_id, HealthStatus::Unhealthy).unwrap();
    assert_eq!(registry.healthy_endpoint_count("default", "health-tracked"), Some(0));
}

#[test]
fn test_draining_status() {
    let registry = Arc::new(ServiceRegistry::new());

    let service = Service::builder("draining-test")
        .namespace("default")
        .port(ServicePort::http(8080))
        .build();
    registry.register(service).unwrap();

    let mut endpoint = Endpoint::builder("10.0.0.1".parse().unwrap(), 8080).build();
    endpoint.health_status = HealthStatus::Healthy;
    let ep_id = endpoint.id;
    registry.add_endpoint("default", "draining-test", endpoint).unwrap();

    assert_eq!(registry.healthy_endpoint_count("default", "draining-test"), Some(1));

    // Set to draining
    registry.update_endpoint_health("default", "draining-test", ep_id, HealthStatus::Draining).unwrap();

    // Draining endpoints are not considered healthy for new connections
    assert_eq!(registry.healthy_endpoint_count("default", "draining-test"), Some(0));
}

#[test]
fn test_mixed_health_states() {
    let registry = Arc::new(ServiceRegistry::new());

    let service = Service::builder("mixed-health")
        .namespace("default")
        .port(ServicePort::http(8080))
        .build();
    registry.register(service).unwrap();

    // Add endpoints with different states
    let states = vec![
        HealthStatus::Healthy,
        HealthStatus::Healthy,
        HealthStatus::Unhealthy,
        HealthStatus::Unknown,
        HealthStatus::Draining,
    ];

    for (i, state) in states.iter().enumerate() {
        let mut endpoint = Endpoint::builder(
            format!("10.0.0.{}", i + 1).parse().unwrap(),
            8080,
        ).build();
        endpoint.health_status = *state;
        registry.add_endpoint("default", "mixed-health", endpoint).unwrap();
    }

    // Only 2 healthy
    assert_eq!(registry.endpoint_count("default", "mixed-health"), Some(5));
    assert_eq!(registry.healthy_endpoint_count("default", "mixed-health"), Some(2));
}

// ============================================================================
// DNS Resolution
// ============================================================================

#[test]
fn test_dns_resolver_creation() {
    let registry = Arc::new(ServiceRegistry::new());
    let resolver = DnsResolver::new(registry);
    // Resolver should be created successfully
}

#[test]
fn test_simple_dns_resolution() {
    let registry = Arc::new(ServiceRegistry::new());

    // Register service with healthy endpoint
    let service = Service::builder("api")
        .namespace("default")
        .port(ServicePort::http(8080))
        .build();
    registry.register(service).unwrap();

    let mut endpoint = Endpoint::builder("10.0.0.1".parse().unwrap(), 8080).build();
    endpoint.health_status = HealthStatus::Healthy;
    registry.add_endpoint("default", "api", endpoint).unwrap();

    let resolver = DnsResolver::new(Arc::clone(&registry));

    // Resolve simple name
    let record = resolver.resolve("api");
    assert!(record.is_ok());
    let record = record.unwrap();
    assert!(!record.addresses.is_empty());
}

#[test]
fn test_qualified_dns_resolution() {
    let registry = Arc::new(ServiceRegistry::new());

    let service = Service::builder("api")
        .namespace("production")
        .port(ServicePort::http(8080))
        .build();
    registry.register(service).unwrap();

    let mut endpoint = Endpoint::builder("10.0.0.100".parse().unwrap(), 8080).build();
    endpoint.health_status = HealthStatus::Healthy;
    registry.add_endpoint("production", "api", endpoint).unwrap();

    let resolver = DnsResolver::new(Arc::clone(&registry));

    // Resolve with namespace
    let record = resolver.resolve("api.production");
    assert!(record.is_ok());

    // Resolve with .svc suffix
    let record = resolver.resolve("api.production.svc");
    assert!(record.is_ok());

    // Resolve with full FQDN
    let record = resolver.resolve("api.production.svc.cluster.local");
    assert!(record.is_ok());
}

#[test]
fn test_dns_name_parsing() {
    let registry = Arc::new(ServiceRegistry::new());
    let resolver = DnsResolver::new(registry);

    // Parse different formats
    let simple = resolver.parse_name("api");
    assert!(simple.is_ok());
    let simple = simple.unwrap();
    assert_eq!(simple.service, "api");
    assert_eq!(simple.namespace, "default");

    let qualified = resolver.parse_name("api.production");
    assert!(qualified.is_ok());
    let qualified = qualified.unwrap();
    assert_eq!(qualified.service, "api");
    assert_eq!(qualified.namespace, "production");
}

#[test]
fn test_srv_record_lookup() {
    let registry = Arc::new(ServiceRegistry::new());

    let service = Service::builder("web")
        .namespace("default")
        .port(ServicePort::http(80).with_name("http"))
        .build();
    registry.register(service).unwrap();

    // Add healthy endpoints
    for i in 1..=3 {
        let mut endpoint = Endpoint::builder(
            format!("10.0.0.{}", i).parse().unwrap(),
            80,
        ).build();
        endpoint.health_status = HealthStatus::Healthy;
        registry.add_endpoint("default", "web", endpoint).unwrap();
    }

    let resolver = DnsResolver::new(Arc::clone(&registry));

    // SRV lookup
    let srv = resolver.resolve_srv("_http._tcp.web.default");
    assert!(srv.is_ok());
    let srv = srv.unwrap();
    assert_eq!(srv.endpoints.len(), 3);
}

#[test]
fn test_dns_resolution_no_healthy_endpoints() {
    let registry = Arc::new(ServiceRegistry::new());

    let service = Service::builder("unhealthy")
        .namespace("default")
        .port(ServicePort::http(8080))
        .build();
    registry.register(service).unwrap();

    // Add only unhealthy endpoints
    let mut endpoint = Endpoint::builder("10.0.0.1".parse().unwrap(), 8080).build();
    endpoint.health_status = HealthStatus::Unhealthy;
    registry.add_endpoint("default", "unhealthy", endpoint).unwrap();

    let resolver = DnsResolver::new(Arc::clone(&registry));

    // Should not resolve (no healthy endpoints)
    assert!(!resolver.can_resolve("unhealthy"));
}

// ============================================================================
// Load Balancing
// ============================================================================

#[test]
fn test_round_robin_load_balancer() {
    let lb = LoadBalancer::new(LoadBalancerStrategy::RoundRobin);

    // Add endpoints
    for i in 1..=3 {
        let mut endpoint = Endpoint::builder(
            format!("10.0.0.{}", i).parse().unwrap(),
            8080,
        ).build();
        endpoint.health_status = HealthStatus::Healthy;
        lb.add_endpoint(endpoint);
    }

    // Make selections
    let mut selection_counts: HashMap<String, usize> = HashMap::new();
    for _ in 0..30 {
        let selected = lb.select(None).unwrap();
        let key = selected.addr.to_string();
        *selection_counts.entry(key).or_insert(0) += 1;
    }

    // Should be evenly distributed
    for count in selection_counts.values() {
        assert_eq!(*count, 10);
    }
}

#[test]
fn test_random_load_balancer() {
    let lb = LoadBalancer::new(LoadBalancerStrategy::Random);

    for i in 1..=3 {
        let mut endpoint = Endpoint::builder(
            format!("10.0.0.{}", i).parse().unwrap(),
            8080,
        ).build();
        endpoint.health_status = HealthStatus::Healthy;
        lb.add_endpoint(endpoint);
    }

    // Make many selections - should pick different endpoints
    let mut selected_addresses = std::collections::HashSet::new();
    for _ in 0..100 {
        let selected = lb.select(None).unwrap();
        selected_addresses.insert(selected.addr.to_string());
    }

    // Should have selected from multiple endpoints
    assert!(selected_addresses.len() > 1);
}

#[test]
fn test_least_connections_load_balancer() {
    let lb = LoadBalancer::new(LoadBalancerStrategy::LeastConnections);

    let mut endpoints = Vec::new();
    for i in 1..=3 {
        let mut endpoint = Endpoint::builder(
            format!("10.0.0.{}", i).parse().unwrap(),
            8080,
        ).build();
        endpoint.health_status = HealthStatus::Healthy;
        endpoints.push(endpoint.id);
        lb.add_endpoint(endpoint);
    }

    // Add connections to first two endpoints
    lb.record_connection(endpoints[0]);
    lb.record_connection(endpoints[0]);
    lb.record_connection(endpoints[1]);

    // Should prefer endpoint 3 (0 connections)
    let selected = lb.select(None).unwrap();
    assert_eq!(selected.id, endpoints[2]);
}

#[test]
fn test_ip_hash_load_balancer() {
    let lb = LoadBalancer::new(LoadBalancerStrategy::IpHash);

    for i in 1..=3 {
        let mut endpoint = Endpoint::builder(
            format!("10.0.0.{}", i).parse().unwrap(),
            8080,
        ).build();
        endpoint.health_status = HealthStatus::Healthy;
        lb.add_endpoint(endpoint);
    }

    // Same client IP should always get same endpoint
    let client_ip: std::net::IpAddr = "192.168.1.100".parse().unwrap();

    let first_selection = lb.select(Some(client_ip)).unwrap();
    for _ in 0..10 {
        let selection = lb.select(Some(client_ip)).unwrap();
        assert_eq!(selection.id, first_selection.id);
    }
}

#[test]
fn test_weighted_random_load_balancer() {
    let lb = LoadBalancer::new(LoadBalancerStrategy::WeightedRandom);

    // Endpoint with high weight
    let mut heavy = Endpoint::builder("10.0.0.1".parse().unwrap(), 8080)
        .weight(300)
        .build();
    heavy.health_status = HealthStatus::Healthy;
    let heavy_id = heavy.id;
    lb.add_endpoint(heavy);

    // Endpoint with low weight
    let mut light = Endpoint::builder("10.0.0.2".parse().unwrap(), 8080)
        .weight(100)
        .build();
    light.health_status = HealthStatus::Healthy;
    let light_id = light.id;
    lb.add_endpoint(light);

    // Make selections
    let mut heavy_count = 0;
    let mut light_count = 0;
    for _ in 0..1000 {
        let selected = lb.select(None).unwrap();
        if selected.id == heavy_id {
            heavy_count += 1;
        } else {
            light_count += 1;
        }
    }

    // Heavy should be selected roughly 3x more than light
    let ratio = heavy_count as f64 / light_count as f64;
    assert!(ratio > 2.0 && ratio < 4.0, "Ratio was {}", ratio);
}

// ============================================================================
// Load Balancing via Registry
// ============================================================================

#[test]
fn test_registry_endpoint_selection() {
    let registry = Arc::new(ServiceRegistry::new());

    let service = Service::builder("lb-service")
        .namespace("default")
        .port(ServicePort::http(8080))
        .load_balancer(LoadBalancerStrategy::RoundRobin)
        .build();
    registry.register(service).unwrap();

    for i in 1..=3 {
        let mut endpoint = Endpoint::builder(
            format!("10.0.0.{}", i).parse().unwrap(),
            8080,
        ).build();
        endpoint.health_status = HealthStatus::Healthy;
        registry.add_endpoint("default", "lb-service", endpoint).unwrap();
    }

    // Select endpoints
    let selected = registry.select_endpoint("default", "lb-service", None);
    assert!(selected.is_ok());
}

#[test]
fn test_session_affinity() {
    let registry = Arc::new(ServiceRegistry::new());

    let service = Service::builder("session-service")
        .namespace("default")
        .port(ServicePort::http(8080))
        .session_affinity(true)
        .build();
    registry.register(service).unwrap();

    for i in 1..=3 {
        let mut endpoint = Endpoint::builder(
            format!("10.0.0.{}", i).parse().unwrap(),
            8080,
        ).build();
        endpoint.health_status = HealthStatus::Healthy;
        registry.add_endpoint("default", "session-service", endpoint).unwrap();
    }

    let client_ip: std::net::IpAddr = "192.168.1.50".parse().unwrap();

    // Same client should get same endpoint
    let first = registry.select_endpoint("default", "session-service", Some(client_ip)).unwrap();
    for _ in 0..10 {
        let selected = registry.select_endpoint("default", "session-service", Some(client_ip)).unwrap();
        assert_eq!(selected.id, first.id);
    }
}

// ============================================================================
// Multiple Namespaces
// ============================================================================

#[test]
fn test_multiple_namespaces() {
    let registry = Arc::new(ServiceRegistry::new());

    // Same service name in different namespaces
    for ns in ["production", "staging", "development"] {
        let service = Service::builder("api")
            .namespace(ns)
            .port(ServicePort::http(8080))
            .build();
        registry.register(service).unwrap();

        let mut endpoint = Endpoint::builder(
            format!("10.{}.0.1", match ns {
                "production" => 1,
                "staging" => 2,
                _ => 3,
            }).parse().unwrap(),
            8080,
        ).build();
        endpoint.health_status = HealthStatus::Healthy;
        registry.add_endpoint(ns, "api", endpoint).unwrap();
    }

    // DNS resolves to correct namespace
    let resolver = DnsResolver::new(Arc::clone(&registry));

    let prod = resolver.resolve("api.production").unwrap();
    let staging = resolver.resolve("api.staging").unwrap();
    let dev = resolver.resolve("api.development").unwrap();

    // All should have different IPs
    assert_ne!(prod.first_address(), staging.first_address());
    assert_ne!(staging.first_address(), dev.first_address());
}

// ============================================================================
// Integration: Full Service Mesh Workflow
// ============================================================================

#[test]
fn test_full_service_mesh_workflow() {
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

    registry.register(api_service).unwrap();
    registry.register(web_service).unwrap();
    registry.register(db_service).unwrap();

    // 2. Add endpoints
    for i in 1..=3 {
        let mut ep = Endpoint::builder(
            format!("10.0.0.{}", i).parse().unwrap(),
            8080,
        )
        .node_id(&format!("node-{}", i))
        .build();
        ep.health_status = HealthStatus::Healthy;
        registry.add_endpoint("production", "api-gateway", ep).unwrap();
    }

    let mut web_ep = Endpoint::builder("10.0.1.1".parse().unwrap(), 80).build();
    web_ep.health_status = HealthStatus::Healthy;
    registry.add_endpoint("production", "web-frontend", web_ep).unwrap();

    let mut db_ep = Endpoint::builder("10.0.2.1".parse().unwrap(), 5432).build();
    db_ep.health_status = HealthStatus::Healthy;
    registry.add_endpoint("production", "postgres", db_ep).unwrap();

    // 3. Verify registry state
    assert_eq!(registry.len(), 3);
    assert_eq!(registry.endpoint_count("production", "api-gateway"), Some(3));

    // 4. DNS resolution
    let resolver = DnsResolver::new(Arc::clone(&registry));

    let api_record = resolver.resolve("api-gateway.production").unwrap();
    assert_eq!(api_record.addresses.len(), 3);

    let db_record = resolver.resolve("postgres.production").unwrap();
    assert_eq!(db_record.addresses.len(), 1);

    // 5. Load balancing
    let ep1 = registry.select_endpoint("production", "api-gateway", None).unwrap();
    let ep1_id = ep1.id;

    // Record connection
    registry.record_connection("production", "api-gateway", ep1_id);
    registry.record_connection("production", "api-gateway", ep1_id);

    // Next selection should prefer other endpoints (least connections)
    let ep2 = registry.select_endpoint("production", "api-gateway", None).unwrap();
    assert_ne!(ep2.id, ep1_id); // Should pick a different one

    // 6. Health updates
    registry.update_endpoint_health(
        "production",
        "api-gateway",
        ep1_id,
        HealthStatus::Unhealthy,
    ).unwrap();

    assert_eq!(registry.healthy_endpoint_count("production", "api-gateway"), Some(2));

    // 7. SRV lookup
    let srv = resolver.resolve_srv("_http._tcp.api-gateway.production").unwrap();
    assert_eq!(srv.endpoints.len(), 2); // Only healthy ones

    // 8. Find services by label
    let selector = LabelSelector::new().with_label("team", "platform");
    let platform_services = registry.find_by_labels(&selector);
    assert_eq!(platform_services.len(), 1);
    assert_eq!(platform_services[0].name, "api-gateway");

    // 9. Verify stats
    let stats = registry.stats();
    assert_eq!(stats.service_count, 3);
    assert_eq!(stats.namespace_count, 1);
    assert_eq!(stats.total_endpoints, 5);
    assert_eq!(stats.healthy_endpoints, 4); // 2 api (1 unhealthy) + 1 web + 1 db

    // 10. Deregister
    registry.deregister("production", "web-frontend").unwrap();
    assert_eq!(registry.len(), 2);
}

#[test]
fn test_health_check_driven_workflow() {
    let registry = Arc::new(ServiceRegistry::new());

    // Register service
    let service = Service::builder("health-driven")
        .namespace("default")
        .port(ServicePort::http(8080))
        .build();
    registry.register(service).unwrap();

    // Add endpoint starting as unknown
    let endpoint = Endpoint::builder("10.0.0.1".parse().unwrap(), 8080).build();
    let ep_id = endpoint.id;
    registry.add_endpoint("default", "health-driven", endpoint).unwrap();

    let resolver = DnsResolver::new(Arc::clone(&registry));

    // Initially can't resolve (no healthy endpoints)
    assert!(!resolver.can_resolve("health-driven"));

    // Health check passes
    registry.update_endpoint_health("default", "health-driven", ep_id, HealthStatus::Healthy).unwrap();
    assert!(resolver.can_resolve("health-driven"));

    // Health check fails
    registry.update_endpoint_health("default", "health-driven", ep_id, HealthStatus::Unhealthy).unwrap();
    assert!(!resolver.can_resolve("health-driven"));

    // Recovered
    registry.update_endpoint_health("default", "health-driven", ep_id, HealthStatus::Healthy).unwrap();
    assert!(resolver.can_resolve("health-driven"));

    // Draining (graceful shutdown)
    registry.update_endpoint_health("default", "health-driven", ep_id, HealthStatus::Draining).unwrap();
    assert!(!resolver.can_resolve("health-driven"));
}

#[test]
fn test_cross_namespace_resolution() {
    let registry = Arc::new(ServiceRegistry::new());

    // Services in different namespaces
    for ns in ["app", "data", "monitoring"] {
        let service = Service::builder("service")
            .namespace(ns)
            .port(ServicePort::http(8080))
            .build();
        registry.register(service).unwrap();

        let mut endpoint = Endpoint::builder(
            format!("10.0.{}.1", match ns {
                "app" => 1,
                "data" => 2,
                _ => 3,
            }).parse().unwrap(),
            8080,
        ).build();
        endpoint.health_status = HealthStatus::Healthy;
        registry.add_endpoint(ns, "service", endpoint).unwrap();
    }

    let resolver = DnsResolver::new(Arc::clone(&registry));

    // Each namespace resolves independently
    let app = resolver.resolve("service.app").unwrap();
    let data = resolver.resolve("service.data").unwrap();
    let mon = resolver.resolve("service.monitoring").unwrap();

    assert_ne!(app.first_address(), data.first_address());
    assert_ne!(data.first_address(), mon.first_address());
}
