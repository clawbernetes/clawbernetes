//! Networking command handlers
//!
//! Provides 9 commands (requires `network` feature):
//! `service.create`, `service.get`, `service.delete`, `service.list`, `service.endpoints`,
//! `ingress.create`, `ingress.delete`, `network.status`, `network.policy.create`

use crate::commands::{CommandError, CommandRequest};
use crate::persist::{IngressEntry, IngressRule, NetworkPolicyEntry, ServiceEntry};
use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

/// Route a service.*, ingress.*, or network.* command.
pub async fn handle_network_command(
    state: &SharedState,
    request: CommandRequest,
) -> Result<Value, CommandError> {
    match request.command.as_str() {
        "service.create" => handle_service_create(state, request.params).await,
        "service.get" => handle_service_get(state, request.params).await,
        "service.delete" => handle_service_delete(state, request.params).await,
        "service.list" => handle_service_list(state).await,
        "service.endpoints" => handle_service_endpoints(state, request.params).await,
        "ingress.create" => handle_ingress_create(state, request.params).await,
        "ingress.delete" => handle_ingress_delete(state, request.params).await,
        "network.status" => handle_network_status(state).await,
        "network.policy.create" => handle_network_policy_create(state, request.params).await,
        "network.policy.delete" => handle_network_policy_delete(state, request.params).await,
        "network.policy.list" => handle_network_policy_list(state).await,
        _ => Err(format!("unknown network command: {}", request.command).into()),
    }
}

#[derive(Debug, Deserialize)]
struct ServiceCreateParams {
    name: String,
    #[serde(default)]
    selector: std::collections::HashMap<String, String>,
    port: u16,
    #[serde(default = "default_tcp")]
    protocol: String,
}

fn default_tcp() -> String {
    "tcp".to_string()
}

async fn handle_service_create(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: ServiceCreateParams = serde_json::from_value(params)?;

    info!(name = %params.name, port = params.port, "creating service");

    // Store in persistence layer
    let entry = ServiceEntry {
        name: params.name.clone(),
        selector: params.selector.clone(),
        port: params.port,
        protocol: params.protocol.clone(),
        endpoints: Vec::new(),
        created_at: chrono::Utc::now(),
    };

    let mut store = state.service_store.write().await;
    store
        .create_service(entry)
        .map_err(|e| -> CommandError { e.into() })?;
    drop(store);

    // Register with ServiceDiscovery for ClusterIP allocation
    let cluster_ip = {
        let mut sd_guard = state.service_discovery.write().await;
        if let Some(ref mut sd) = *sd_guard {
            match sd.register_service(&params.name, params.port, &params.protocol, params.selector) {
                Ok(vip) => Some(vip.to_string()),
                Err(e) => {
                    tracing::warn!(error = %e, "service discovery unavailable");
                    None
                }
            }
        } else {
            None
        }
    };

    let mut result = json!({
        "name": params.name,
        "port": params.port,
        "protocol": params.protocol,
        "success": true,
    });

    if let Some(ip) = cluster_ip {
        result["clusterIp"] = json!(ip);
    }

    Ok(result)
}

#[derive(Debug, Deserialize)]
struct ServiceIdentifyParams {
    name: String,
}

async fn handle_service_get(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: ServiceIdentifyParams = serde_json::from_value(params)?;

    let store = state.service_store.read().await;
    let svc = store
        .get_service(&params.name)
        .ok_or_else(|| format!("service '{}' not found", params.name))?;

    let mut result = json!({
        "name": svc.name,
        "selector": svc.selector,
        "port": svc.port,
        "protocol": svc.protocol,
        "endpoints": svc.endpoints,
        "created_at": svc.created_at.to_rfc3339(),
    });
    drop(store);

    // Include ClusterIP and live endpoints from ServiceDiscovery
    let sd_guard = state.service_discovery.read().await;
    if let Some(ref sd) = *sd_guard {
        if let Some(vip) = sd.get_cluster_ip(&params.name) {
            result["clusterIp"] = json!(vip.to_string());
        }
        if let Some(endpoints) = sd.get_endpoints(&params.name) {
            result["liveEndpoints"] = json!(endpoints.iter().map(|e| json!({
                "ip": e.ip.to_string(),
                "port": e.port,
                "containerId": e.container_id,
                "healthy": e.healthy,
            })).collect::<Vec<_>>());
        }
    }

    Ok(result)
}

async fn handle_service_delete(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: ServiceIdentifyParams = serde_json::from_value(params)?;

    info!(name = %params.name, "deleting service");

    let mut store = state.service_store.write().await;
    store
        .delete_service(&params.name)
        .map_err(|e| -> CommandError { e.into() })?;
    drop(store);

    // Remove from ServiceDiscovery (releases VIP and iptables rules)
    let mut sd_guard = state.service_discovery.write().await;
    if let Some(ref mut sd) = *sd_guard {
        let _ = sd.remove_service(&params.name); // Ignore if not in SD
    }

    Ok(json!({
        "name": params.name,
        "deleted": true,
    }))
}

async fn handle_service_list(state: &SharedState) -> Result<Value, CommandError> {
    let store = state.service_store.read().await;
    let sd_guard = state.service_discovery.read().await;

    let services: Vec<Value> = store
        .list_services()
        .iter()
        .map(|s| {
            let mut entry = json!({
                "name": s.name,
                "port": s.port,
                "protocol": s.protocol,
                "endpoints": s.endpoints.len(),
                "created_at": s.created_at.to_rfc3339(),
            });

            if let Some(ref sd) = *sd_guard {
                if let Some(vip) = sd.get_cluster_ip(&s.name) {
                    entry["clusterIp"] = json!(vip.to_string());
                }
            }

            entry
        })
        .collect();

    Ok(json!({
        "count": services.len(),
        "services": services,
    }))
}

async fn handle_service_endpoints(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: ServiceIdentifyParams = serde_json::from_value(params)?;

    let sd_guard = state.service_discovery.read().await;
    let sd = sd_guard
        .as_ref()
        .ok_or("service discovery not initialized")?;

    let (vip, endpoints) = sd
        .resolve(&params.name)
        .ok_or_else(|| format!("service '{}' not found in service discovery", params.name))?;

    let endpoint_list: Vec<Value> = endpoints
        .iter()
        .map(|e| {
            json!({
                "ip": e.ip.to_string(),
                "port": e.port,
                "containerId": e.container_id,
                "healthy": e.healthy,
            })
        })
        .collect();

    let healthy_count = endpoints.iter().filter(|e| e.healthy).count();

    Ok(json!({
        "name": params.name,
        "clusterIp": vip.to_string(),
        "total": endpoints.len(),
        "healthy": healthy_count,
        "endpoints": endpoint_list,
    }))
}

#[derive(Debug, Deserialize)]
struct IngressCreateParams {
    name: String,
    rules: Vec<IngressRuleParam>,
    #[serde(default)]
    tls: bool,
}

#[derive(Debug, Deserialize)]
struct IngressRuleParam {
    host: String,
    path: String,
    service: String,
}

async fn handle_ingress_create(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: IngressCreateParams = serde_json::from_value(params)?;

    info!(name = %params.name, rules = params.rules.len(), "creating ingress");

    let rules: Vec<IngressRule> = params
        .rules
        .into_iter()
        .map(|r| IngressRule {
            host: r.host,
            path: r.path,
            service: r.service,
        })
        .collect();

    let entry = IngressEntry {
        name: params.name.clone(),
        rules,
        tls: params.tls,
        created_at: chrono::Utc::now(),
    };

    let mut store = state.service_store.write().await;
    store
        .create_ingress(entry)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "name": params.name,
        "tls": params.tls,
        "success": true,
    }))
}

async fn handle_ingress_delete(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: ServiceIdentifyParams = serde_json::from_value(params)?;

    info!(name = %params.name, "deleting ingress");

    let mut store = state.service_store.write().await;
    store
        .delete_ingress(&params.name)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "name": params.name,
        "deleted": true,
    }))
}

async fn handle_network_status(state: &SharedState) -> Result<Value, CommandError> {
    let mesh = &state.wireguard_mesh;
    let node_count = mesh.node_count();
    let topology = mesh.get_topology();

    let mesh_nodes: Vec<Value> = topology
        .nodes
        .values()
        .map(|n| {
            json!({
                "id": n.node_id.to_string(),
                "meshIp": n.mesh_ip.to_string(),
                "region": format!("{}", n.region),
            })
        })
        .collect();

    // Include WireGuard mesh status if available
    let wireguard_status = {
        let mgr_guard = state.mesh_manager.read().await;
        if let Some(ref mgr) = *mgr_guard {
            let status = mgr.status().await;
            Some(json!({
                "interface": status.interface,
                "meshIp": status.mesh_ip,
                "publicKey": status.public_key,
                "listenPort": status.listen_port,
                "nodeId": status.node_id,
                "region": status.region,
                "peers": status.peers.iter().map(|p| json!({
                    "publicKey": p.public_key,
                    "endpoint": p.endpoint,
                    "meshIp": p.mesh_ip,
                    "lastHandshake": p.last_handshake,
                    "rxBytes": p.rx_bytes,
                    "txBytes": p.tx_bytes,
                })).collect::<Vec<_>>(),
            }))
        } else {
            None
        }
    };

    // Include service discovery summary
    let service_discovery_status = {
        let sd_guard = state.service_discovery.read().await;
        if let Some(ref sd) = *sd_guard {
            let services = sd.list_services();
            Some(json!({
                "serviceCount": services.len(),
                "iptablesAvailable": sd.iptables_available(),
                "services": services.iter().map(|s| json!({
                    "name": s.name,
                    "clusterIp": s.cluster_ip.to_string(),
                    "port": s.port,
                    "endpoints": s.endpoint_count,
                    "healthy": s.healthy_count,
                })).collect::<Vec<_>>(),
            }))
        } else {
            None
        }
    };

    Ok(json!({
        "mesh": {
            "nodeCount": node_count,
            "nodes": mesh_nodes,
        },
        "wireguard": wireguard_status,
        "serviceDiscovery": service_discovery_status,
        "allocator": {
            "stats": format!("{:?}", mesh.allocator().stats()),
        },
    }))
}

#[derive(Debug, Deserialize)]
struct NetworkPolicyCreateParams {
    name: String,
    #[serde(default)]
    selector: std::collections::HashMap<String, String>,
    #[serde(default)]
    ingress: Vec<Value>,
    #[serde(default)]
    egress: Vec<Value>,
}

async fn handle_network_policy_create(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: NetworkPolicyCreateParams = serde_json::from_value(params)?;

    info!(name = %params.name, "creating network policy");

    let entry = NetworkPolicyEntry {
        name: params.name.clone(),
        selector: params.selector.clone(),
        ingress_rules: params.ingress.clone(),
        egress_rules: params.egress.clone(),
        created_at: chrono::Utc::now(),
    };

    let mut store = state.service_store.write().await;
    store
        .create_network_policy(entry)
        .map_err(|e| -> CommandError { e.into() })?;
    drop(store);

    // Enforce via PolicyEngine if available
    let mut pe_guard = state.policy_engine.write().await;
    if let Some(ref mut pe) = *pe_guard {
        pe.add_policy(&params.name, params.selector, &params.ingress, &params.egress, &[])?;
    }

    Ok(json!({
        "name": params.name,
        "success": true,
    }))
}

async fn handle_network_policy_delete(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: ServiceIdentifyParams = serde_json::from_value(params)?;

    info!(name = %params.name, "deleting network policy");

    let mut store = state.service_store.write().await;
    store
        .delete_network_policy(&params.name)
        .map_err(|e| -> CommandError { e.into() })?;
    drop(store);

    // Remove from PolicyEngine
    let mut pe_guard = state.policy_engine.write().await;
    if let Some(ref mut pe) = *pe_guard {
        let _ = pe.remove_policy(&params.name); // Ignore if not tracked
    }

    Ok(json!({
        "name": params.name,
        "deleted": true,
    }))
}

async fn handle_network_policy_list(state: &SharedState) -> Result<Value, CommandError> {
    let store = state.service_store.read().await;
    let persisted = store.list_network_policies();

    let policies: Vec<Value> = persisted
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "selector": p.selector,
                "ingressRules": p.ingress_rules.len(),
                "egressRules": p.egress_rules.len(),
                "created_at": p.created_at.to_rfc3339(),
            })
        })
        .collect();

    // Include PolicyEngine enforcement info
    let pe_guard = state.policy_engine.read().await;
    let enforcement = if let Some(ref pe) = *pe_guard {
        Some(json!({
            "active": pe.policy_count(),
            "iptablesAvailable": pe.iptables_available(),
        }))
    } else {
        None
    };

    Ok(json!({
        "count": policies.len(),
        "policies": policies,
        "enforcement": enforcement,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NodeConfig;

    fn test_state() -> SharedState {
        let mut config = NodeConfig::default();
        let dir = tempfile::tempdir().expect("tempdir");
        config.state_path = dir.path().to_path_buf();
        std::mem::forget(dir);
        SharedState::new(config)
    }

    #[tokio::test]
    async fn test_service_create_and_get() {
        let state = test_state();

        let result = handle_network_command(
            &state,
            CommandRequest {
                command: "service.create".to_string(),
                params: json!({
                    "name": "api-svc",
                    "selector": {"app": "api"},
                    "port": 8080,
                }),
            },
        )
        .await
        .expect("create");
        assert_eq!(result["success"], true);

        let result = handle_network_command(
            &state,
            CommandRequest {
                command: "service.get".to_string(),
                params: json!({"name": "api-svc"}),
            },
        )
        .await
        .expect("get");
        assert_eq!(result["port"], 8080);
    }

    #[tokio::test]
    async fn test_service_create_with_discovery() {
        let state = test_state();

        // Initialize service discovery
        {
            let mut sd_guard = state.service_discovery.write().await;
            *sd_guard = Some(crate::service_discovery::ServiceDiscovery::new());
        }

        let result = handle_network_command(
            &state,
            CommandRequest {
                command: "service.create".to_string(),
                params: json!({
                    "name": "api-svc",
                    "selector": {"app": "api"},
                    "port": 8080,
                }),
            },
        )
        .await
        .expect("create");

        assert_eq!(result["success"], true);
        // Should have a ClusterIP allocated
        assert!(result["clusterIp"].is_string());
        assert_eq!(result["clusterIp"], "10.201.0.1");
    }

    #[tokio::test]
    async fn test_service_endpoints() {
        let state = test_state();

        // Initialize service discovery
        {
            let mut sd_guard = state.service_discovery.write().await;
            *sd_guard = Some(crate::service_discovery::ServiceDiscovery::new());
        }

        // Create a service with a selector
        handle_network_command(
            &state,
            CommandRequest {
                command: "service.create".to_string(),
                params: json!({
                    "name": "api-svc",
                    "selector": {"app": "api"},
                    "port": 8080,
                }),
            },
        )
        .await
        .expect("create");

        // Add some endpoints
        {
            let mut sd_guard = state.service_discovery.write().await;
            let sd = sd_guard.as_mut().unwrap();
            sd.update_endpoints("api-svc", vec![
                crate::service_discovery::Endpoint {
                    ip: std::net::Ipv4Addr::new(10, 200, 1, 2),
                    port: 8080,
                    container_id: "c-1".to_string(),
                    healthy: true,
                },
                crate::service_discovery::Endpoint {
                    ip: std::net::Ipv4Addr::new(10, 200, 1, 3),
                    port: 8080,
                    container_id: "c-2".to_string(),
                    healthy: true,
                },
            ])
            .expect("update");
        }

        let result = handle_network_command(
            &state,
            CommandRequest {
                command: "service.endpoints".to_string(),
                params: json!({"name": "api-svc"}),
            },
        )
        .await
        .expect("endpoints");

        assert_eq!(result["total"], 2);
        assert_eq!(result["healthy"], 2);
        assert_eq!(result["clusterIp"], "10.201.0.1");
    }

    #[tokio::test]
    async fn test_service_list_and_delete() {
        let state = test_state();

        handle_network_command(
            &state,
            CommandRequest {
                command: "service.create".to_string(),
                params: json!({"name": "svc-1", "selector": {}, "port": 80}),
            },
        )
        .await
        .expect("create");

        let result = handle_network_command(
            &state,
            CommandRequest {
                command: "service.list".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list");
        assert_eq!(result["count"], 1);

        let result = handle_network_command(
            &state,
            CommandRequest {
                command: "service.delete".to_string(),
                params: json!({"name": "svc-1"}),
            },
        )
        .await
        .expect("delete");
        assert_eq!(result["deleted"], true);
    }

    #[tokio::test]
    async fn test_ingress_create_and_delete() {
        let state = test_state();

        let result = handle_network_command(
            &state,
            CommandRequest {
                command: "ingress.create".to_string(),
                params: json!({
                    "name": "api-ingress",
                    "rules": [{"host": "api.example.com", "path": "/", "service": "api-svc"}],
                    "tls": true,
                }),
            },
        )
        .await
        .expect("create");
        assert_eq!(result["success"], true);
        assert_eq!(result["tls"], true);

        let result = handle_network_command(
            &state,
            CommandRequest {
                command: "ingress.delete".to_string(),
                params: json!({"name": "api-ingress"}),
            },
        )
        .await
        .expect("delete");
        assert_eq!(result["deleted"], true);
    }

    #[tokio::test]
    async fn test_network_status() {
        let state = test_state();

        let result = handle_network_command(
            &state,
            CommandRequest {
                command: "network.status".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("status");
        assert_eq!(result["mesh"]["nodeCount"], 0);
    }

    #[tokio::test]
    async fn test_network_policy_create() {
        let state = test_state();

        let result = handle_network_command(
            &state,
            CommandRequest {
                command: "network.policy.create".to_string(),
                params: json!({
                    "name": "deny-all",
                    "selector": {"app": "secure"},
                    "ingress": [],
                    "egress": [],
                }),
            },
        )
        .await
        .expect("create");
        assert_eq!(result["success"], true);
    }
}
