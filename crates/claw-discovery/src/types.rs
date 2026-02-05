//! Core types for service discovery and load balancing.

use std::collections::HashMap;
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ServiceId(Uuid);

impl ServiceId {
    /// Creates a new unique service ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates a service ID from a UUID.
    #[must_use]
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the underlying UUID.
    #[must_use]
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for ServiceId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ServiceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EndpointId(Uuid);

impl EndpointId {
    /// Creates a new unique endpoint ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates an endpoint ID from a UUID.
    #[must_use]
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the underlying UUID.
    #[must_use]
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for EndpointId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EndpointId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Protocol for service communication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    /// TCP protocol.
    #[default]
    Tcp,
    /// UDP protocol.
    Udp,
    /// HTTP protocol (over TCP).
    Http,
    /// HTTPS protocol (over TCP).
    Https,
    /// gRPC protocol (over HTTP/2).
    Grpc,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tcp => write!(f, "tcp"),
            Self::Udp => write!(f, "udp"),
            Self::Http => write!(f, "http"),
            Self::Https => write!(f, "https"),
            Self::Grpc => write!(f, "grpc"),
        }
    }
}

/// Health status of an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// Endpoint is healthy and ready to receive traffic.
    #[default]
    Healthy,
    /// Endpoint is unhealthy and should not receive traffic.
    Unhealthy,
    /// Endpoint health is unknown (initial state or health check pending).
    Unknown,
    /// Endpoint is draining (finishing existing requests, no new traffic).
    Draining,
}

impl fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Unhealthy => write!(f, "unhealthy"),
            Self::Unknown => write!(f, "unknown"),
            Self::Draining => write!(f, "draining"),
        }
    }
}

/// Load balancer strategy for distributing traffic across endpoints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum LoadBalancerStrategy {
    /// Round-robin: distribute requests evenly in order.
    #[default]
    RoundRobin,
    /// Least connections: send to endpoint with fewest active connections.
    LeastConnections,
    /// Random: randomly select an endpoint.
    Random,
    /// Weighted random: select randomly weighted by endpoint weights.
    WeightedRandom,
    /// IP hash: consistent hashing based on client IP.
    IpHash,
}


impl fmt::Display for LoadBalancerStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RoundRobin => write!(f, "round_robin"),
            Self::LeastConnections => write!(f, "least_connections"),
            Self::Random => write!(f, "random"),
            Self::WeightedRandom => write!(f, "weighted_random"),
            Self::IpHash => write!(f, "ip_hash"),
        }
    }
}

/// Health check configuration for endpoints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Interval between health checks.
    #[serde(with = "serde_duration")]
    pub interval: Duration,
    /// Timeout for each health check.
    #[serde(with = "serde_duration")]
    pub timeout: Duration,
    /// Number of consecutive failures before marking unhealthy.
    pub unhealthy_threshold: u32,
    /// Number of consecutive successes before marking healthy.
    pub healthy_threshold: u32,
    /// Optional HTTP path for HTTP health checks.
    pub http_path: Option<String>,
    /// Expected HTTP status codes for healthy endpoint.
    pub expected_status_codes: Vec<u16>,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(10),
            timeout: Duration::from_secs(5),
            unhealthy_threshold: 3,
            healthy_threshold: 2,
            http_path: None,
            expected_status_codes: vec![200],
        }
    }
}

/// Label selector for matching workloads to services.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LabelSelector {
    /// Labels that must match exactly.
    pub match_labels: HashMap<String, String>,
}

impl LabelSelector {
    /// Creates an empty selector that matches nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a selector with the given label.
    #[must_use]
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.match_labels.insert(key.into(), value.into());
        self
    }

    /// Checks if the given labels match this selector.
    #[must_use]
    pub fn matches(&self, labels: &HashMap<String, String>) -> bool {
        self.match_labels
            .iter()
            .all(|(k, v)| labels.get(k) == Some(v))
    }

    /// Returns true if the selector is empty (matches nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.match_labels.is_empty()
    }
}

/// Service port configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServicePort {
    /// Port name (for multi-port services).
    pub name: Option<String>,
    /// Protocol for this port.
    pub protocol: Protocol,
    /// Port exposed by the service.
    pub port: u16,
    /// Target port on the endpoints (defaults to port if not specified).
    pub target_port: Option<u16>,
}

impl ServicePort {
    /// Creates a new TCP service port.
    #[must_use]
    pub fn tcp(port: u16) -> Self {
        Self {
            name: None,
            protocol: Protocol::Tcp,
            port,
            target_port: None,
        }
    }

    /// Creates a new HTTP service port.
    #[must_use]
    pub fn http(port: u16) -> Self {
        Self {
            name: None,
            protocol: Protocol::Http,
            port,
            target_port: None,
        }
    }

    /// Creates a new HTTPS service port.
    #[must_use]
    pub fn https(port: u16) -> Self {
        Self {
            name: None,
            protocol: Protocol::Https,
            port,
            target_port: None,
        }
    }

    /// Creates a new gRPC service port.
    #[must_use]
    pub fn grpc(port: u16) -> Self {
        Self {
            name: None,
            protocol: Protocol::Grpc,
            port,
            target_port: None,
        }
    }

    /// Sets the port name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the target port.
    #[must_use]
    pub fn with_target_port(mut self, target_port: u16) -> Self {
        self.target_port = Some(target_port);
        self
    }

    /// Gets the effective target port.
    #[must_use]
    pub fn effective_target_port(&self) -> u16 {
        self.target_port.unwrap_or(self.port)
    }
}

/// Configuration for a service.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Load balancer strategy.
    pub load_balancer: LoadBalancerStrategy,
    /// Health check configuration.
    pub health_check: HealthCheckConfig,
    /// Session affinity (sticky sessions).
    pub session_affinity: bool,
    /// Session affinity timeout.
    #[serde(with = "serde_duration")]
    pub session_affinity_timeout: Duration,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            load_balancer: LoadBalancerStrategy::default(),
            health_check: HealthCheckConfig::default(),
            session_affinity: false,
            session_affinity_timeout: Duration::from_secs(3600),
        }
    }
}

/// A service that exposes workloads to the network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    /// Unique service identifier.
    pub id: ServiceId,
    /// Service name (used for DNS resolution).
    pub name: String,
    /// Namespace for the service.
    pub namespace: String,
    /// Ports exposed by this service.
    pub ports: Vec<ServicePort>,
    /// Label selector for matching endpoints.
    pub selector: LabelSelector,
    /// Service configuration.
    pub config: ServiceConfig,
    /// Cluster IP assigned to this service (if any).
    pub cluster_ip: Option<IpAddr>,
    /// Labels attached to this service.
    pub labels: HashMap<String, String>,
    /// Annotations attached to this service.
    pub annotations: HashMap<String, String>,
    /// When the service was created.
    pub created_at: DateTime<Utc>,
    /// When the service was last updated.
    pub updated_at: DateTime<Utc>,
}

impl Service {
    /// Creates a new service builder.
    #[must_use]
    pub fn builder(name: impl Into<String>) -> ServiceBuilder {
        ServiceBuilder::new(name)
    }

    /// Returns the fully qualified domain name for this service.
    #[must_use]
    pub fn fqdn(&self) -> String {
        format!("{}.{}.svc.cluster.local", self.name, self.namespace)
    }

    /// Returns the short DNS name for this service.
    #[must_use]
    pub fn dns_name(&self) -> String {
        format!("{}.{}", self.name, self.namespace)
    }
}

/// Builder for creating services.
#[derive(Debug)]
pub struct ServiceBuilder {
    name: String,
    namespace: String,
    ports: Vec<ServicePort>,
    selector: LabelSelector,
    config: ServiceConfig,
    cluster_ip: Option<IpAddr>,
    labels: HashMap<String, String>,
    annotations: HashMap<String, String>,
}

impl ServiceBuilder {
    /// Creates a new service builder with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            namespace: "default".to_string(),
            ports: Vec::new(),
            selector: LabelSelector::new(),
            config: ServiceConfig::default(),
            cluster_ip: None,
            labels: HashMap::new(),
            annotations: HashMap::new(),
        }
    }

    /// Sets the namespace.
    #[must_use]
    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }

    /// Adds a port to the service.
    #[must_use]
    pub fn port(mut self, port: ServicePort) -> Self {
        self.ports.push(port);
        self
    }

    /// Sets the selector.
    #[must_use]
    pub fn selector(mut self, selector: LabelSelector) -> Self {
        self.selector = selector;
        self
    }

    /// Sets the load balancer strategy.
    #[must_use]
    pub fn load_balancer(mut self, strategy: LoadBalancerStrategy) -> Self {
        self.config.load_balancer = strategy;
        self
    }

    /// Sets the health check configuration.
    #[must_use]
    pub fn health_check(mut self, config: HealthCheckConfig) -> Self {
        self.config.health_check = config;
        self
    }

    /// Enables session affinity.
    #[must_use]
    pub fn session_affinity(mut self, enabled: bool) -> Self {
        self.config.session_affinity = enabled;
        self
    }

    /// Sets the cluster IP.
    #[must_use]
    pub fn cluster_ip(mut self, ip: IpAddr) -> Self {
        self.cluster_ip = Some(ip);
        self
    }

    /// Adds a label.
    #[must_use]
    pub fn label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Adds an annotation.
    #[must_use]
    pub fn annotation(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.annotations.insert(key.into(), value.into());
        self
    }

    /// Builds the service.
    #[must_use]
    pub fn build(self) -> Service {
        let now = Utc::now();
        Service {
            id: ServiceId::new(),
            name: self.name,
            namespace: self.namespace,
            ports: self.ports,
            selector: self.selector,
            config: self.config,
            cluster_ip: self.cluster_ip,
            labels: self.labels,
            annotations: self.annotations,
            created_at: now,
            updated_at: now,
        }
    }
}

/// An endpoint representing a backend server for a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    /// Unique endpoint identifier.
    pub id: EndpointId,
    /// IP address of the endpoint.
    pub address: IpAddr,
    /// Port of the endpoint.
    pub port: u16,
    /// Current health status.
    pub health_status: HealthStatus,
    /// Weight for weighted load balancing (default: 100).
    pub weight: u32,
    /// Number of active connections (for least-connections LB).
    pub active_connections: u32,
    /// Labels attached to this endpoint.
    pub labels: HashMap<String, String>,
    /// Node ID where this endpoint runs.
    pub node_id: Option<String>,
    /// When the endpoint was registered.
    pub registered_at: DateTime<Utc>,
    /// When the endpoint was last seen healthy.
    pub last_healthy: Option<DateTime<Utc>>,
    /// Number of consecutive failed health checks.
    pub failed_checks: u32,
    /// Number of consecutive successful health checks.
    pub successful_checks: u32,
}

impl Endpoint {
    /// Creates a new endpoint builder.
    #[must_use]
    pub fn builder(address: IpAddr, port: u16) -> EndpointBuilder {
        EndpointBuilder::new(address, port)
    }

    /// Returns the socket address of this endpoint.
    #[must_use]
    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.address, self.port)
    }

    /// Checks if the endpoint is ready to receive traffic.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.health_status == HealthStatus::Healthy
    }

    /// Increments the active connection count.
    pub fn connect(&mut self) {
        self.active_connections = self.active_connections.saturating_add(1);
    }

    /// Decrements the active connection count.
    pub fn disconnect(&mut self) {
        self.active_connections = self.active_connections.saturating_sub(1);
    }

    /// Records a successful health check.
    pub fn record_healthy(&mut self, healthy_threshold: u32) {
        self.successful_checks = self.successful_checks.saturating_add(1);
        self.failed_checks = 0;
        self.last_healthy = Some(Utc::now());

        if self.successful_checks >= healthy_threshold {
            self.health_status = HealthStatus::Healthy;
        }
    }

    /// Records a failed health check.
    pub fn record_unhealthy(&mut self, unhealthy_threshold: u32) {
        self.failed_checks = self.failed_checks.saturating_add(1);
        self.successful_checks = 0;

        if self.failed_checks >= unhealthy_threshold {
            self.health_status = HealthStatus::Unhealthy;
        }
    }

    /// Sets the endpoint to draining state.
    pub fn drain(&mut self) {
        self.health_status = HealthStatus::Draining;
    }
}

/// Builder for creating endpoints.
#[derive(Debug)]
pub struct EndpointBuilder {
    address: IpAddr,
    port: u16,
    weight: u32,
    labels: HashMap<String, String>,
    node_id: Option<String>,
}

impl EndpointBuilder {
    /// Creates a new endpoint builder.
    #[must_use]
    pub fn new(address: IpAddr, port: u16) -> Self {
        Self {
            address,
            port,
            weight: 100,
            labels: HashMap::new(),
            node_id: None,
        }
    }

    /// Sets the weight for weighted load balancing.
    #[must_use]
    pub fn weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }

    /// Adds a label.
    #[must_use]
    pub fn label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Sets the node ID.
    #[must_use]
    pub fn node_id(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }

    /// Builds the endpoint.
    #[must_use]
    pub fn build(self) -> Endpoint {
        Endpoint {
            id: EndpointId::new(),
            address: self.address,
            port: self.port,
            health_status: HealthStatus::Unknown,
            weight: self.weight,
            active_connections: 0,
            labels: self.labels,
            node_id: self.node_id,
            registered_at: Utc::now(),
            last_healthy: None,
            failed_checks: 0,
            successful_checks: 0,
        }
    }
}

/// Helper module for serializing Duration with serde.
mod serde_duration {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    #[derive(Serialize, Deserialize)]
    struct DurationHelper {
        secs: u64,
        nanos: u32,
    }

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let helper = DurationHelper {
            secs: duration.as_secs(),
            nanos: duration.subsec_nanos(),
        };
        helper.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = DurationHelper::deserialize(deserializer)?;
        Ok(Duration::new(helper.secs, helper.nanos))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== ServiceId Tests ====================

    #[test]
    fn test_service_id_new_creates_unique_ids() {
        let id1 = ServiceId::new();
        let id2 = ServiceId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_service_id_from_uuid() {
        let uuid = Uuid::new_v4();
        let id = ServiceId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), &uuid);
    }

    #[test]
    fn test_service_id_display() {
        let id = ServiceId::new();
        let display = id.to_string();
        assert!(!display.is_empty());
    }

    #[test]
    fn test_service_id_default() {
        let id1 = ServiceId::default();
        let id2 = ServiceId::default();
        assert_ne!(id1, id2);
    }

    // ==================== EndpointId Tests ====================

    #[test]
    fn test_endpoint_id_new_creates_unique_ids() {
        let id1 = EndpointId::new();
        let id2 = EndpointId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_endpoint_id_from_uuid() {
        let uuid = Uuid::new_v4();
        let id = EndpointId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), &uuid);
    }

    // ==================== Protocol Tests ====================

    #[test]
    fn test_protocol_default_is_tcp() {
        assert_eq!(Protocol::default(), Protocol::Tcp);
    }

    #[test]
    fn test_protocol_display() {
        assert_eq!(Protocol::Tcp.to_string(), "tcp");
        assert_eq!(Protocol::Udp.to_string(), "udp");
        assert_eq!(Protocol::Http.to_string(), "http");
        assert_eq!(Protocol::Https.to_string(), "https");
        assert_eq!(Protocol::Grpc.to_string(), "grpc");
    }

    // ==================== HealthStatus Tests ====================

    #[test]
    fn test_health_status_default_is_healthy() {
        assert_eq!(HealthStatus::default(), HealthStatus::Healthy);
    }

    #[test]
    fn test_health_status_display() {
        assert_eq!(HealthStatus::Healthy.to_string(), "healthy");
        assert_eq!(HealthStatus::Unhealthy.to_string(), "unhealthy");
        assert_eq!(HealthStatus::Unknown.to_string(), "unknown");
        assert_eq!(HealthStatus::Draining.to_string(), "draining");
    }

    // ==================== LoadBalancerStrategy Tests ====================

    #[test]
    fn test_lb_strategy_default_is_round_robin() {
        assert_eq!(LoadBalancerStrategy::default(), LoadBalancerStrategy::RoundRobin);
    }

    #[test]
    fn test_lb_strategy_display() {
        assert_eq!(LoadBalancerStrategy::RoundRobin.to_string(), "round_robin");
        assert_eq!(LoadBalancerStrategy::LeastConnections.to_string(), "least_connections");
        assert_eq!(LoadBalancerStrategy::Random.to_string(), "random");
        assert_eq!(LoadBalancerStrategy::WeightedRandom.to_string(), "weighted_random");
        assert_eq!(LoadBalancerStrategy::IpHash.to_string(), "ip_hash");
    }

    // ==================== LabelSelector Tests ====================

    #[test]
    fn test_label_selector_empty() {
        let selector = LabelSelector::new();
        assert!(selector.is_empty());
    }

    #[test]
    fn test_label_selector_with_label() {
        let selector = LabelSelector::new()
            .with_label("app", "nginx")
            .with_label("version", "v1");

        assert!(!selector.is_empty());
        assert_eq!(selector.match_labels.len(), 2);
    }

    #[test]
    fn test_label_selector_matches() {
        let selector = LabelSelector::new()
            .with_label("app", "nginx");

        let mut matching = HashMap::new();
        matching.insert("app".to_string(), "nginx".to_string());
        matching.insert("version".to_string(), "v1".to_string());

        let mut non_matching = HashMap::new();
        non_matching.insert("app".to_string(), "apache".to_string());

        assert!(selector.matches(&matching));
        assert!(!selector.matches(&non_matching));
    }

    #[test]
    fn test_label_selector_empty_matches_nothing() {
        let selector = LabelSelector::new();
        let labels = HashMap::new();
        // Empty selector matches empty labels (vacuous truth)
        assert!(selector.matches(&labels));
    }

    // ==================== ServicePort Tests ====================

    #[test]
    fn test_service_port_tcp() {
        let port = ServicePort::tcp(8080);
        assert_eq!(port.port, 8080);
        assert_eq!(port.protocol, Protocol::Tcp);
        assert!(port.name.is_none());
        assert!(port.target_port.is_none());
    }

    #[test]
    fn test_service_port_http() {
        let port = ServicePort::http(80);
        assert_eq!(port.port, 80);
        assert_eq!(port.protocol, Protocol::Http);
    }

    #[test]
    fn test_service_port_with_name() {
        let port = ServicePort::tcp(8080).with_name("api");
        assert_eq!(port.name, Some("api".to_string()));
    }

    #[test]
    fn test_service_port_with_target_port() {
        let port = ServicePort::tcp(80).with_target_port(8080);
        assert_eq!(port.target_port, Some(8080));
        assert_eq!(port.effective_target_port(), 8080);
    }

    #[test]
    fn test_service_port_effective_target_port_default() {
        let port = ServicePort::tcp(8080);
        assert_eq!(port.effective_target_port(), 8080);
    }

    // ==================== HealthCheckConfig Tests ====================

    #[test]
    fn test_health_check_config_default() {
        let config = HealthCheckConfig::default();
        assert_eq!(config.interval, Duration::from_secs(10));
        assert_eq!(config.timeout, Duration::from_secs(5));
        assert_eq!(config.unhealthy_threshold, 3);
        assert_eq!(config.healthy_threshold, 2);
        assert!(config.http_path.is_none());
        assert_eq!(config.expected_status_codes, vec![200]);
    }

    // ==================== ServiceConfig Tests ====================

    #[test]
    fn test_service_config_default() {
        let config = ServiceConfig::default();
        assert_eq!(config.load_balancer, LoadBalancerStrategy::RoundRobin);
        assert!(!config.session_affinity);
    }

    // ==================== Service Tests ====================

    #[test]
    fn test_service_builder_basic() {
        let service = Service::builder("api-gateway")
            .namespace("production")
            .port(ServicePort::http(80))
            .build();

        assert_eq!(service.name, "api-gateway");
        assert_eq!(service.namespace, "production");
        assert_eq!(service.ports.len(), 1);
    }

    #[test]
    fn test_service_builder_with_all_options() {
        let ip: std::net::IpAddr = "10.0.0.1".parse().ok().unwrap();
        let service = Service::builder("web-server")
            .namespace("default")
            .port(ServicePort::http(80))
            .port(ServicePort::https(443))
            .selector(LabelSelector::new().with_label("app", "web"))
            .load_balancer(LoadBalancerStrategy::LeastConnections)
            .session_affinity(true)
            .cluster_ip(ip)
            .label("team", "platform")
            .annotation("description", "Main web server")
            .build();

        assert_eq!(service.ports.len(), 2);
        assert!(service.config.session_affinity);
        assert_eq!(service.config.load_balancer, LoadBalancerStrategy::LeastConnections);
        assert!(service.cluster_ip.is_some());
        assert_eq!(service.labels.get("team"), Some(&"platform".to_string()));
    }

    #[test]
    fn test_service_fqdn() {
        let service = Service::builder("api")
            .namespace("production")
            .build();

        assert_eq!(service.fqdn(), "api.production.svc.cluster.local");
    }

    #[test]
    fn test_service_dns_name() {
        let service = Service::builder("api")
            .namespace("production")
            .build();

        assert_eq!(service.dns_name(), "api.production");
    }

    // ==================== Endpoint Tests ====================

    #[test]
    fn test_endpoint_builder_basic() {
        let endpoint = Endpoint::builder("10.0.0.1".parse().ok().unwrap(), 8080)
            .build();

        assert_eq!(endpoint.address.to_string(), "10.0.0.1");
        assert_eq!(endpoint.port, 8080);
        assert_eq!(endpoint.health_status, HealthStatus::Unknown);
        assert_eq!(endpoint.weight, 100);
        assert_eq!(endpoint.active_connections, 0);
    }

    #[test]
    fn test_endpoint_builder_with_options() {
        let endpoint = Endpoint::builder("192.168.1.100".parse().ok().unwrap(), 9000)
            .weight(200)
            .label("zone", "us-west-1")
            .node_id("node-123")
            .build();

        assert_eq!(endpoint.weight, 200);
        assert_eq!(endpoint.labels.get("zone"), Some(&"us-west-1".to_string()));
        assert_eq!(endpoint.node_id, Some("node-123".to_string()));
    }

    #[test]
    fn test_endpoint_socket_addr() {
        let endpoint = Endpoint::builder("10.0.0.1".parse().ok().unwrap(), 8080)
            .build();

        assert_eq!(endpoint.socket_addr().to_string(), "10.0.0.1:8080");
    }

    #[test]
    fn test_endpoint_is_ready() {
        let mut endpoint = Endpoint::builder("10.0.0.1".parse().ok().unwrap(), 8080)
            .build();

        assert!(!endpoint.is_ready()); // Unknown status

        endpoint.health_status = HealthStatus::Healthy;
        assert!(endpoint.is_ready());

        endpoint.health_status = HealthStatus::Unhealthy;
        assert!(!endpoint.is_ready());
    }

    #[test]
    fn test_endpoint_connect_disconnect() {
        let mut endpoint = Endpoint::builder("10.0.0.1".parse().ok().unwrap(), 8080)
            .build();

        assert_eq!(endpoint.active_connections, 0);

        endpoint.connect();
        endpoint.connect();
        assert_eq!(endpoint.active_connections, 2);

        endpoint.disconnect();
        assert_eq!(endpoint.active_connections, 1);

        endpoint.disconnect();
        endpoint.disconnect(); // Should not go negative
        assert_eq!(endpoint.active_connections, 0);
    }

    #[test]
    fn test_endpoint_record_healthy() {
        let mut endpoint = Endpoint::builder("10.0.0.1".parse().ok().unwrap(), 8080)
            .build();

        // Simulate health checks
        endpoint.record_healthy(2);
        assert_eq!(endpoint.successful_checks, 1);
        assert_eq!(endpoint.health_status, HealthStatus::Unknown);

        endpoint.record_healthy(2);
        assert_eq!(endpoint.successful_checks, 2);
        assert_eq!(endpoint.health_status, HealthStatus::Healthy);
        assert!(endpoint.last_healthy.is_some());
    }

    #[test]
    fn test_endpoint_record_unhealthy() {
        let mut endpoint = Endpoint::builder("10.0.0.1".parse().ok().unwrap(), 8080)
            .build();

        endpoint.health_status = HealthStatus::Healthy;

        endpoint.record_unhealthy(3);
        assert_eq!(endpoint.failed_checks, 1);
        assert_eq!(endpoint.health_status, HealthStatus::Healthy);

        endpoint.record_unhealthy(3);
        endpoint.record_unhealthy(3);
        assert_eq!(endpoint.failed_checks, 3);
        assert_eq!(endpoint.health_status, HealthStatus::Unhealthy);
    }

    #[test]
    fn test_endpoint_record_healthy_resets_failures() {
        let mut endpoint = Endpoint::builder("10.0.0.1".parse().ok().unwrap(), 8080)
            .build();

        endpoint.record_unhealthy(3);
        endpoint.record_unhealthy(3);
        assert_eq!(endpoint.failed_checks, 2);

        endpoint.record_healthy(2);
        assert_eq!(endpoint.failed_checks, 0);
        assert_eq!(endpoint.successful_checks, 1);
    }

    #[test]
    fn test_endpoint_drain() {
        let mut endpoint = Endpoint::builder("10.0.0.1".parse().ok().unwrap(), 8080)
            .build();

        endpoint.health_status = HealthStatus::Healthy;
        endpoint.drain();
        assert_eq!(endpoint.health_status, HealthStatus::Draining);
    }

    // ==================== Serialization Tests ====================

    #[test]
    fn test_service_serialization() {
        let service = Service::builder("test-service")
            .namespace("test-ns")
            .port(ServicePort::http(80))
            .build();

        let json = serde_json::to_string(&service).ok();
        assert!(json.is_some());

        let deserialized: Result<Service, _> = serde_json::from_str(&json.unwrap_or_default());
        assert!(deserialized.is_ok());
    }

    #[test]
    fn test_endpoint_serialization() {
        let endpoint = Endpoint::builder("10.0.0.1".parse().ok().unwrap(), 8080)
            .weight(150)
            .build();

        let json = serde_json::to_string(&endpoint).ok();
        assert!(json.is_some());

        let deserialized: Result<Endpoint, _> = serde_json::from_str(&json.unwrap_or_default());
        assert!(deserialized.is_ok());
    }
}
