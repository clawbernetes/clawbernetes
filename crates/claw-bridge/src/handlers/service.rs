//! Service discovery handlers
//!
//! These handlers integrate with claw-discovery for service mesh capabilities.

use std::collections::HashMap;
use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use claw_discovery::{
    EndpointBuilder, HealthStatus, LabelSelector, Service, ServiceBuilder, ServicePort,
    ServiceRegistry,
};

use crate::error::{BridgeError, BridgeResult};
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Global State
// ─────────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref SERVICE_REGISTRY: ServiceRegistry = ServiceRegistry::new();
}

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ServiceInfo {
    pub id: String,
    pub namespace: String,
    pub name: String,
    pub labels: HashMap<String, String>,
    pub ports: Vec<PortInfo>,
    pub endpoint_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortInfo {
    pub name: Option<String>,
    pub port: u16,
    pub target_port: u16,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EndpointInfo {
    pub id: String,
    pub address: String,
    pub port: u16,
    pub healthy: bool,
    pub weight: u32,
}

// ─────────────────────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ServiceRegisterParams {
    pub name: String,
    pub namespace: Option<String>,
    pub ports: Vec<PortParams>,
    pub labels: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct PortParams {
    pub port: u16,
    pub name: Option<String>,
    pub target_port: Option<u16>,
    pub protocol: Option<String>,
}

/// Register a service
pub async fn service_register(params: Value) -> BridgeResult<Value> {
    let params: ServiceRegisterParams = parse_params(params)?;

    let mut builder = ServiceBuilder::new(&params.name);

    if let Some(ns) = &params.namespace {
        builder = builder.namespace(ns);
    }

    for p in &params.ports {
        let mut port = match p.protocol.as_deref() {
            Some("tcp") | Some("TCP") => ServicePort::tcp(p.port),
            Some("grpc") | Some("GRPC") => ServicePort::grpc(p.port),
            Some("https") | Some("HTTPS") => ServicePort::https(p.port),
            _ => ServicePort::http(p.port),
        };

        if let Some(name) = &p.name {
            port = port.with_name(name);
        }
        if let Some(target) = p.target_port {
            port = port.with_target_port(target);
        }

        builder = builder.port(port);
    }

    if let Some(labels) = &params.labels {
        for (k, v) in labels {
            builder = builder.label(k, v);
        }
    }

    let service = builder.build();
    let service_id = SERVICE_REGISTRY
        .register(service)
        .map_err(|e| BridgeError::Internal(format!("failed to register service: {e}")))?;

    tracing::info!(name = %params.name, service_id = %service_id, "service registered");

    to_json(serde_json::json!({
        "service_id": service_id.to_string(),
        "name": params.name,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ServiceGetParams {
    pub namespace: String,
    pub name: String,
}

/// Get a service
pub async fn service_get(params: Value) -> BridgeResult<Value> {
    let params: ServiceGetParams = parse_params(params)?;

    let service = SERVICE_REGISTRY
        .get(&params.namespace, &params.name)
        .ok_or_else(|| BridgeError::NotFound("service not found".to_string()))?;

    let endpoints = SERVICE_REGISTRY
        .get_endpoints(&params.namespace, &params.name)
        .unwrap_or_default();

    to_json(service_to_info(&service, endpoints.len()))
}

#[derive(Debug, Deserialize)]
pub struct ServiceListParams {
    pub namespace: Option<String>,
    pub labels: Option<HashMap<String, String>>,
}

/// List services
pub async fn service_list(params: Value) -> BridgeResult<Value> {
    let params: ServiceListParams = parse_params(params)?;

    let services = if let Some(labels) = params.labels {
        let mut selector = LabelSelector::new();
        for (k, v) in labels {
            selector = selector.with_label(k, v);
        }
        SERVICE_REGISTRY.find_by_labels(&selector)
    } else if let Some(ns) = &params.namespace {
        SERVICE_REGISTRY.list_in_namespace(ns)
    } else {
        SERVICE_REGISTRY.list()
    };

    let infos: Vec<ServiceInfo> = services
        .iter()
        .map(|s| {
            let endpoints = SERVICE_REGISTRY
                .get_endpoints(&s.namespace, &s.name)
                .unwrap_or_default();
            service_to_info(s, endpoints.len())
        })
        .collect();

    to_json(infos)
}

#[derive(Debug, Deserialize)]
pub struct ServiceDeregisterParams {
    pub namespace: String,
    pub name: String,
}

/// Deregister a service
pub async fn service_deregister(params: Value) -> BridgeResult<Value> {
    let params: ServiceDeregisterParams = parse_params(params)?;

    SERVICE_REGISTRY
        .deregister(&params.namespace, &params.name)
        .map_err(|e| BridgeError::NotFound(format!("failed to deregister: {e}")))?;

    tracing::info!(namespace = %params.namespace, name = %params.name, "service deregistered");

    to_json(serde_json::json!({ "success": true }))
}

// ─────────────────────────────────────────────────────────────
// Endpoint Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct EndpointAddParams {
    pub namespace: String,
    pub service_name: String,
    pub address: String,
    pub port: u16,
    pub weight: Option<u32>,
}

/// Add an endpoint to a service
pub async fn endpoint_add(params: Value) -> BridgeResult<Value> {
    let params: EndpointAddParams = parse_params(params)?;

    let ip: IpAddr = params
        .address
        .parse()
        .map_err(|_| BridgeError::InvalidParams(format!("invalid IP: {}", params.address)))?;

    let mut builder = EndpointBuilder::new(ip, params.port);
    if let Some(weight) = params.weight {
        builder = builder.weight(weight);
    }
    let endpoint = builder.build();

    SERVICE_REGISTRY
        .add_endpoint(&params.namespace, &params.service_name, endpoint)
        .map_err(|e| BridgeError::Internal(format!("failed to add endpoint: {e}")))?;

    tracing::info!(
        namespace = %params.namespace,
        service = %params.service_name,
        address = %params.address,
        "endpoint added"
    );

    to_json(serde_json::json!({ "success": true }))
}

#[derive(Debug, Deserialize)]
pub struct EndpointListParams {
    pub namespace: String,
    pub service_name: String,
    pub healthy_only: Option<bool>,
}

/// List endpoints for a service
pub async fn endpoint_list(params: Value) -> BridgeResult<Value> {
    let params: EndpointListParams = parse_params(params)?;

    let endpoints = if params.healthy_only.unwrap_or(false) {
        SERVICE_REGISTRY
            .get_healthy_endpoints(&params.namespace, &params.service_name)
            .map_err(|e| BridgeError::Internal(format!("failed to get endpoints: {e}")))?
    } else {
        SERVICE_REGISTRY
            .get_endpoints(&params.namespace, &params.service_name)
            .map_err(|e| BridgeError::Internal(format!("failed to get endpoints: {e}")))?
    };

    let infos: Vec<EndpointInfo> = endpoints
        .iter()
        .map(|e| EndpointInfo {
            id: e.id.to_string(),
            address: e.address.to_string(),
            port: e.port,
            healthy: matches!(e.health_status, HealthStatus::Healthy),
            weight: e.weight,
        })
        .collect();

    to_json(infos)
}

#[derive(Debug, Deserialize)]
pub struct EndpointSelectParams {
    pub namespace: String,
    pub service_name: String,
}

/// Select an endpoint using load balancing
pub async fn endpoint_select(params: Value) -> BridgeResult<Value> {
    let params: EndpointSelectParams = parse_params(params)?;

    let endpoint = SERVICE_REGISTRY
        .select_endpoint(&params.namespace, &params.service_name, None)
        .map_err(|e| BridgeError::Internal(format!("failed to select endpoint: {e}")))?;

    to_json(EndpointInfo {
        id: endpoint.id.to_string(),
        address: endpoint.address.to_string(),
        port: endpoint.port,
        healthy: matches!(endpoint.health_status, HealthStatus::Healthy),
        weight: endpoint.weight,
    })
}

// ─────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────

fn service_to_info(service: &Service, endpoint_count: usize) -> ServiceInfo {
    let ports: Vec<PortInfo> = service
        .ports
        .iter()
        .map(|p| PortInfo {
            name: p.name.clone(),
            port: p.port,
            target_port: p.effective_target_port(),
            protocol: format!("{:?}", p.protocol),
        })
        .collect();

    ServiceInfo {
        id: service.id.to_string(),
        namespace: service.namespace.clone(),
        name: service.name.clone(),
        labels: service.labels.clone(),
        ports,
        endpoint_count,
    }
}
