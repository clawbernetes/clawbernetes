//! Message handlers for gateway server.

use claw_gateway::{
    NodeRegistry, RegistryError, WorkloadLogStore, WorkloadManager, WorkloadManagerError,
};
use claw_proto::{
    GatewayMessage, GpuMetricsProto, NodeCapabilities, NodeId, NodeMessage, WorkloadId,
    WorkloadState,
};
use chrono::Utc;
use tracing::{debug, info, warn};

use crate::config::ServerConfig;
use crate::error::{ServerError, ServerResult};

/// Handle a node registration request.
///
/// # Errors
///
/// Returns an error if registration fails.
pub fn handle_register(
    node_id: NodeId,
    name: &str,
    capabilities: NodeCapabilities,
    registry: &mut NodeRegistry,
    config: &ServerConfig,
) -> ServerResult<GatewayMessage> {
    info!(node_id = %node_id, name = %name, "Node registration request");
    debug!(
        "Node capabilities: {} CPUs, {} MiB RAM, {} GPUs",
        capabilities.cpu_cores,
        capabilities.memory_mib,
        capabilities.gpus.len()
    );

    match registry.register(node_id, capabilities) {
        Ok(()) => {
            info!(node_id = %node_id, "Node registered successfully");
            Ok(GatewayMessage::registered(
                node_id,
                config.heartbeat_interval_secs(),
                config.metrics_interval_secs(),
            ))
        }
        Err(RegistryError::AlreadyRegistered(_)) => {
            warn!(node_id = %node_id, "Node already registered, sending updated config");
            // For reconnections, just update and acknowledge
            if let Some(node) = registry.get_node_mut(node_id) {
                node.update_heartbeat();
            }
            Ok(GatewayMessage::registered(
                node_id,
                config.heartbeat_interval_secs(),
                config.metrics_interval_secs(),
            ))
        }
        Err(e) => Err(ServerError::Protocol(e.to_string())),
    }
}

/// Handle a heartbeat from a node.
///
/// # Errors
///
/// Returns an error if the node is not registered.
pub fn handle_heartbeat(node_id: NodeId, registry: &mut NodeRegistry) -> ServerResult<GatewayMessage> {
    debug!(node_id = %node_id, "Heartbeat received");

    match registry.heartbeat(node_id) {
        Ok(()) => Ok(GatewayMessage::HeartbeatAck {
            server_time: Utc::now(),
        }),
        Err(RegistryError::NotFound(_)) => {
            warn!(node_id = %node_id, "Heartbeat from unregistered node");
            Err(ServerError::NodeNotRegistered(node_id))
        }
        Err(e) => Err(ServerError::Protocol(e.to_string())),
    }
}

/// Handle metrics from a node.
///
/// # Errors
///
/// Returns an error if the node is not registered.
pub fn handle_metrics(
    node_id: NodeId,
    gpu_metrics: &[GpuMetricsProto],
    registry: &NodeRegistry,
) -> ServerResult<()> {
    debug!(node_id = %node_id, gpu_count = gpu_metrics.len(), "Metrics received");

    // Verify node is registered
    if registry.get_node(node_id).is_none() {
        warn!(node_id = %node_id, "Metrics from unregistered node");
        return Err(ServerError::NodeNotRegistered(node_id));
    }

    // Log metrics for now (in production, this would go to a metrics store)
    for metric in gpu_metrics {
        debug!(
            node_id = %node_id,
            gpu_index = metric.index,
            utilization = metric.utilization_percent,
            memory_used = metric.memory_used_mib,
            temperature = metric.temperature_celsius,
            "GPU metrics"
        );
    }

    Ok(())
}

/// Handle a workload state update from a node.
///
/// # Errors
///
/// Returns an error if the workload is not found or the state transition is invalid.
pub fn handle_workload_update(
    workload_id: WorkloadId,
    state: WorkloadState,
    message: Option<&str>,
    workload_mgr: &mut WorkloadManager,
) -> ServerResult<()> {
    info!(
        workload_id = %workload_id,
        state = ?state,
        message = message,
        "Workload update received"
    );

    match workload_mgr.update_state(workload_id, state) {
        Ok(()) => {
            info!(workload_id = %workload_id, state = ?state, "Workload state updated");
            Ok(())
        }
        Err(WorkloadManagerError::NotFound(_)) => {
            warn!(workload_id = %workload_id, "Update for unknown workload");
            Err(ServerError::Protocol(format!(
                "workload {workload_id} not found"
            )))
        }
        Err(WorkloadManagerError::InvalidTransition(_, from, to)) => {
            warn!(
                workload_id = %workload_id,
                from = ?from,
                to = ?to,
                "Invalid workload state transition"
            );
            Err(ServerError::Protocol(format!(
                "invalid state transition from {from} to {to}"
            )))
        }
        Err(e) => Err(ServerError::Protocol(e.to_string())),
    }
}

/// Route an incoming node message to the appropriate handler.
///
/// # Errors
///
/// Returns an error if handling fails.
pub fn route_message(
    msg: &NodeMessage,
    registry: &mut NodeRegistry,
    workload_mgr: &mut WorkloadManager,
    log_store: &mut WorkloadLogStore,
    config: &ServerConfig,
) -> ServerResult<Option<GatewayMessage>> {
    match msg {
        NodeMessage::Register {
            node_id,
            name,
            capabilities,
            protocol_version: _,
            wireguard_public_key: _,
            wireguard_endpoint: _,
        } => {
            let response = handle_register(*node_id, name, capabilities.clone(), registry, config)?;
            Ok(Some(response))
        }
        NodeMessage::Heartbeat { node_id, .. } => {
            let response = handle_heartbeat(*node_id, registry)?;
            Ok(Some(response))
        }
        NodeMessage::Metrics {
            node_id,
            gpu_metrics,
            ..
        } => {
            handle_metrics(*node_id, gpu_metrics, registry)?;
            Ok(None)
        }
        NodeMessage::WorkloadUpdate {
            workload_id,
            state,
            message,
            ..
        } => {
            handle_workload_update(*workload_id, *state, message.as_deref(), workload_mgr)?;
            Ok(None)
        }
        NodeMessage::WorkloadLogs {
            workload_id,
            lines,
            is_stderr,
        } => {
            handle_workload_logs(*workload_id, lines, *is_stderr, log_store);
            Ok(None)
        }
        NodeMessage::MeshReady {
            node_id,
            mesh_ip,
            peer_count,
            error,
        } => {
            handle_mesh_ready(*node_id, mesh_ip, *peer_count, error.as_deref());
            Ok(None)
        }
    }
}

/// Handle mesh ready notification from a node.
fn handle_mesh_ready(
    node_id: NodeId,
    mesh_ip: &str,
    peer_count: u32,
    error: Option<&str>,
) {
    if let Some(err) = error {
        warn!(
            node_id = %node_id,
            mesh_ip = %mesh_ip,
            peer_count = peer_count,
            error = %err,
            "Node mesh ready with errors"
        );
    } else {
        info!(
            node_id = %node_id,
            mesh_ip = %mesh_ip,
            peer_count = peer_count,
            "Node mesh ready"
        );
    }
}

/// Handle workload log lines from a node.
fn handle_workload_logs(
    workload_id: WorkloadId,
    lines: &[String],
    is_stderr: bool,
    log_store: &mut WorkloadLogStore,
) {
    debug!(
        workload_id = %workload_id,
        lines = lines.len(),
        is_stderr = is_stderr,
        "Received workload logs"
    );

    if is_stderr {
        log_store.append_stderr(workload_id, lines.iter().cloned());
    } else {
        log_store.append_stdout(workload_id, lines.iter().cloned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_proto::{GpuCapability, WorkloadSpec};

    // ==================== Helper Functions ====================

    fn make_config() -> ServerConfig {
        ServerConfig::default()
            .with_heartbeat_interval(std::time::Duration::from_secs(30))
            .with_metrics_interval(std::time::Duration::from_secs(10))
    }

    fn make_capabilities() -> NodeCapabilities {
        NodeCapabilities::new(8, 16384)
    }

    fn make_capabilities_with_gpu() -> NodeCapabilities {
        NodeCapabilities::new(8, 16384).with_gpu(GpuCapability {
            index: 0,
            name: "NVIDIA RTX 4090".to_string(),
            memory_mib: 24576,
            uuid: "gpu-uuid-test".to_string(),
        })
    }

    fn make_gpu_metrics() -> Vec<GpuMetricsProto> {
        vec![GpuMetricsProto {
            index: 0,
            utilization_percent: 75,
            memory_used_mib: 8000,
            memory_total_mib: 24576,
            temperature_celsius: 65,
            power_watts: Some(250.0),
        }]
    }

    // ==================== Register Handler Tests ====================

    #[test]
    fn test_handle_register_success() {
        let mut registry = NodeRegistry::new();
        let config = make_config();
        let node_id = NodeId::new();

        let result = handle_register(
            node_id,
            "test-node",
            make_capabilities(),
            &mut registry,
            &config,
        );

        assert!(result.is_ok());
        let response = result.unwrap();
        match response {
            GatewayMessage::Registered {
                node_id: resp_id,
                heartbeat_interval_secs,
                metrics_interval_secs,
            } => {
                assert_eq!(resp_id, node_id);
                assert_eq!(heartbeat_interval_secs, 30);
                assert_eq!(metrics_interval_secs, 10);
            }
            _ => panic!("Expected Registered response"),
        }
    }

    #[test]
    fn test_handle_register_node_stored_in_registry() {
        let mut registry = NodeRegistry::new();
        let config = make_config();
        let node_id = NodeId::new();
        let caps = make_capabilities_with_gpu();

        handle_register(node_id, "test-node", caps.clone(), &mut registry, &config).unwrap();

        let node = registry.get_node(node_id);
        assert!(node.is_some());
        let node = node.unwrap();
        assert_eq!(node.capabilities.cpu_cores, 8);
        assert_eq!(node.capabilities.gpus.len(), 1);
    }

    #[test]
    fn test_handle_register_duplicate_node_succeeds() {
        let mut registry = NodeRegistry::new();
        let config = make_config();
        let node_id = NodeId::new();

        // First registration
        handle_register(node_id, "test-node", make_capabilities(), &mut registry, &config).unwrap();

        // Second registration (reconnection) should also succeed
        let result = handle_register(
            node_id,
            "test-node",
            make_capabilities(),
            &mut registry,
            &config,
        );

        assert!(result.is_ok());
    }

    // ==================== Heartbeat Handler Tests ====================

    #[test]
    fn test_handle_heartbeat_success() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();
        registry.register(node_id, make_capabilities()).unwrap();

        let result = handle_heartbeat(node_id, &mut registry);

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(matches!(response, GatewayMessage::HeartbeatAck { .. }));
    }

    #[test]
    fn test_handle_heartbeat_unregistered_node() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();

        let result = handle_heartbeat(node_id, &mut registry);

        assert!(matches!(result, Err(ServerError::NodeNotRegistered(_))));
    }

    #[test]
    fn test_handle_heartbeat_updates_timestamp() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();
        registry.register(node_id, make_capabilities()).unwrap();

        let before = registry.get_node(node_id).unwrap().last_heartbeat;
        std::thread::sleep(std::time::Duration::from_millis(10));

        handle_heartbeat(node_id, &mut registry).unwrap();

        let after = registry.get_node(node_id).unwrap().last_heartbeat;
        assert!(after >= before);
    }

    // ==================== Metrics Handler Tests ====================

    #[test]
    fn test_handle_metrics_success() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();
        registry.register(node_id, make_capabilities_with_gpu()).unwrap();

        let result = handle_metrics(node_id, &make_gpu_metrics(), &registry);

        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_metrics_unregistered_node() {
        let registry = NodeRegistry::new();
        let node_id = NodeId::new();

        let result = handle_metrics(node_id, &make_gpu_metrics(), &registry);

        assert!(matches!(result, Err(ServerError::NodeNotRegistered(_))));
    }

    #[test]
    fn test_handle_metrics_empty_metrics() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();
        registry.register(node_id, make_capabilities()).unwrap();

        let result = handle_metrics(node_id, &[], &registry);

        assert!(result.is_ok());
    }

    // ==================== Workload Update Handler Tests ====================

    #[test]
    fn test_handle_workload_update_success() {
        let mut workload_mgr = WorkloadManager::new();
        let spec = WorkloadSpec::new("nginx:latest")
            .with_memory_mb(1024)
            .with_cpu_cores(2);
        let workload_id = workload_mgr.submit(spec).unwrap();

        let result = handle_workload_update(
            workload_id,
            WorkloadState::Starting,
            Some("Container starting"),
            &mut workload_mgr,
        );

        assert!(result.is_ok());
        assert_eq!(
            workload_mgr.get_status(workload_id).unwrap().state,
            WorkloadState::Starting
        );
    }

    #[test]
    fn test_handle_workload_update_not_found() {
        let mut workload_mgr = WorkloadManager::new();
        let fake_id = WorkloadId::new();

        let result = handle_workload_update(
            fake_id,
            WorkloadState::Running,
            None,
            &mut workload_mgr,
        );

        assert!(matches!(result, Err(ServerError::Protocol(_))));
    }

    #[test]
    fn test_handle_workload_update_invalid_transition() {
        let mut workload_mgr = WorkloadManager::new();
        let spec = WorkloadSpec::new("nginx:latest")
            .with_memory_mb(1024)
            .with_cpu_cores(2);
        let workload_id = workload_mgr.submit(spec).unwrap();

        // Try to jump directly from Pending to Completed (invalid)
        let result = handle_workload_update(
            workload_id,
            WorkloadState::Completed,
            None,
            &mut workload_mgr,
        );

        assert!(matches!(result, Err(ServerError::Protocol(_))));
    }

    // ==================== Route Message Tests ====================

    #[test]
    fn test_route_register_message() {
        let mut registry = NodeRegistry::new();
        let mut workload_mgr = WorkloadManager::new();
        let mut log_store = WorkloadLogStore::new();
        let config = make_config();
        let node_id = NodeId::new();

        let msg = NodeMessage::register(node_id, "test-node", make_capabilities());

        let result = route_message(&msg, &mut registry, &mut workload_mgr, &mut log_store, &config);

        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
        assert!(registry.get_node(node_id).is_some());
    }

    #[test]
    fn test_route_heartbeat_message() {
        let mut registry = NodeRegistry::new();
        let mut workload_mgr = WorkloadManager::new();
        let mut log_store = WorkloadLogStore::new();
        let config = make_config();
        let node_id = NodeId::new();

        registry.register(node_id, make_capabilities()).unwrap();

        let msg = NodeMessage::heartbeat(node_id);
        let result = route_message(&msg, &mut registry, &mut workload_mgr, &mut log_store, &config);

        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn test_route_metrics_message() {
        let mut registry = NodeRegistry::new();
        let mut workload_mgr = WorkloadManager::new();
        let mut log_store = WorkloadLogStore::new();
        let config = make_config();
        let node_id = NodeId::new();

        registry.register(node_id, make_capabilities_with_gpu()).unwrap();

        let msg = NodeMessage::metrics(node_id, make_gpu_metrics());
        let result = route_message(&msg, &mut registry, &mut workload_mgr, &mut log_store, &config);

        assert!(result.is_ok());
        // Metrics don't have a response
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_route_workload_update_message() {
        let mut registry = NodeRegistry::new();
        let mut workload_mgr = WorkloadManager::new();
        let mut log_store = WorkloadLogStore::new();
        let config = make_config();

        let spec = WorkloadSpec::new("nginx:latest")
            .with_memory_mb(1024)
            .with_cpu_cores(2);
        let workload_id = workload_mgr.submit(spec).unwrap();

        let msg = NodeMessage::workload_update(workload_id, WorkloadState::Starting, None);
        let result = route_message(&msg, &mut registry, &mut workload_mgr, &mut log_store, &config);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_route_workload_logs_message() {
        let mut registry = NodeRegistry::new();
        let mut workload_mgr = WorkloadManager::new();
        let mut log_store = WorkloadLogStore::new();
        let config = make_config();
        let workload_id = WorkloadId::new();

        let msg = NodeMessage::WorkloadLogs {
            workload_id,
            lines: vec!["log line 1".to_string(), "log line 2".to_string()],
            is_stderr: false,
        };

        let result = route_message(&msg, &mut registry, &mut workload_mgr, &mut log_store, &config);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());

        // Verify logs were stored
        let logs = log_store.get_logs(workload_id).unwrap();
        assert_eq!(logs.get_stdout(None), vec!["log line 1", "log line 2"]);
    }

    #[test]
    fn test_route_workload_logs_stderr() {
        let mut registry = NodeRegistry::new();
        let mut workload_mgr = WorkloadManager::new();
        let mut log_store = WorkloadLogStore::new();
        let config = make_config();
        let workload_id = WorkloadId::new();

        let msg = NodeMessage::WorkloadLogs {
            workload_id,
            lines: vec!["error line".to_string()],
            is_stderr: true,
        };

        let result = route_message(&msg, &mut registry, &mut workload_mgr, &mut log_store, &config);

        assert!(result.is_ok());

        // Verify stderr was stored
        let logs = log_store.get_logs(workload_id).unwrap();
        assert_eq!(logs.get_stderr(None), vec!["error line"]);
        assert!(logs.get_stdout(None).is_empty());
    }
}
