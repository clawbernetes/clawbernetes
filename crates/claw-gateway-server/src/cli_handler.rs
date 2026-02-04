//! CLI connection handler.
//!
//! Handles WebSocket connections from `claw-cli` clients, distinct from
//! node connections. CLI connections use the CLI protocol for administrative
//! operations like listing nodes, managing workloads, and querying status.

use std::sync::Arc;
use std::time::Instant;

use claw_gateway::{NodeHealthStatus, NodeRegistry, WorkloadLogStore, WorkloadManager};
use claw_proto::cli::{
    self, CliMessage, CliResponse, NodeInfo, NodeState, WorkloadInfo, CLI_PROTOCOL_VERSION,
};
use claw_proto::{NodeId, WorkloadId, WorkloadSpec, WorkloadState};
use futures::{Sink, SinkExt, Stream, StreamExt};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, info, warn};

use crate::config::ServerConfig;
use crate::error::{ServerError, ServerResult};

/// Handle a CLI WebSocket connection.
///
/// This function takes over after the initial Hello message identifies
/// the connection as a CLI client.
pub async fn handle_cli_connection<S>(
    mut ws: S,
    registry: Arc<Mutex<NodeRegistry>>,
    workload_manager: Arc<Mutex<WorkloadManager>>,
    log_store: Arc<Mutex<WorkloadLogStore>>,
    config: Arc<ServerConfig>,
    start_time: Instant,
) -> ServerResult<()>
where
    S: Stream<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>>
        + Sink<WsMessage>
        + Unpin
        + Send,
    <S as Sink<WsMessage>>::Error: std::fmt::Display,
{
    // Send welcome response
    let welcome = CliResponse::welcome(env!("CARGO_PKG_VERSION"));
    send_response(&mut ws, &welcome).await?;

    info!("CLI client connected");

    // Process CLI messages
    loop {
        let msg = match ws.next().await {
            Some(Ok(WsMessage::Text(text))) => text,
            Some(Ok(WsMessage::Close(_))) => {
                info!("CLI client disconnected");
                break;
            }
            Some(Ok(WsMessage::Ping(data))) => {
                if let Err(e) = ws.send(WsMessage::Pong(data)).await {
                    warn!(error = %e, "Failed to send pong");
                }
                continue;
            }
            Some(Ok(_)) => continue,
            Some(Err(e)) => {
                warn!(error = %e, "WebSocket error");
                break;
            }
            None => {
                info!("CLI connection closed");
                break;
            }
        };

        // Parse CLI message
        let request = match CliMessage::from_json(&msg) {
            Ok(msg) => msg,
            Err(e) => {
                let response = CliResponse::error(
                    cli::error_codes::INVALID_REQUEST,
                    format!("invalid message: {e}"),
                );
                send_response(&mut ws, &response).await?;
                continue;
            }
        };

        debug!(request_type = request.request_type(), "Processing CLI request");

        // Handle the request
        let response = handle_request(
            request,
            &registry,
            &workload_manager,
            &log_store,
            &config,
            start_time,
        )
        .await;

        send_response(&mut ws, &response).await?;
    }

    Ok(())
}

/// Handle a single CLI request.
async fn handle_request(
    request: CliMessage,
    registry: &Arc<Mutex<NodeRegistry>>,
    workload_manager: &Arc<Mutex<WorkloadManager>>,
    log_store: &Arc<Mutex<WorkloadLogStore>>,
    _config: &Arc<ServerConfig>,
    start_time: Instant,
) -> CliResponse {
    match request {
        CliMessage::Hello {
            protocol_version, ..
        } => {
            // Already handled in handshake, but respond anyway
            if protocol_version != CLI_PROTOCOL_VERSION {
                return CliResponse::error(
                    cli::error_codes::PROTOCOL_MISMATCH,
                    format!(
                        "protocol version mismatch: expected {}, got {}",
                        CLI_PROTOCOL_VERSION, protocol_version
                    ),
                );
            }
            CliResponse::welcome(env!("CARGO_PKG_VERSION"))
        }

        CliMessage::GetStatus => handle_get_status(registry, workload_manager, start_time).await,

        CliMessage::ListNodes {
            state_filter,
            include_capabilities,
        } => handle_list_nodes(registry, state_filter, include_capabilities).await,

        CliMessage::GetNode { node_id } => handle_get_node(registry, node_id).await,

        CliMessage::ListWorkloads {
            node_filter,
            state_filter,
        } => handle_list_workloads(workload_manager, node_filter, state_filter).await,

        CliMessage::GetWorkload { workload_id } => {
            handle_get_workload(workload_manager, workload_id).await
        }

        CliMessage::StartWorkload { node_id, spec } => {
            handle_start_workload(workload_manager, registry, node_id, spec).await
        }

        CliMessage::StopWorkload { workload_id, force } => {
            handle_stop_workload(workload_manager, workload_id, force).await
        }

        CliMessage::GetLogs {
            workload_id,
            tail,
            include_stderr,
        } => handle_get_logs(log_store, workload_id, tail, include_stderr).await,

        CliMessage::DrainNode { node_id, drain } => {
            handle_drain_node(registry, node_id, drain).await
        }

        CliMessage::GetMoltStatus => handle_get_molt_status().await,

        CliMessage::ListMoltPeers => handle_list_molt_peers().await,

        CliMessage::GetMoltBalance => handle_get_molt_balance().await,

        CliMessage::Ping { timestamp } => CliResponse::pong(timestamp),
    }
}

/// Send a CLI response.
async fn send_response<S>(ws: &mut S, response: &CliResponse) -> ServerResult<()>
where
    S: Sink<WsMessage> + Unpin,
    <S as Sink<WsMessage>>::Error: std::fmt::Display,
{
    let json = response
        .to_json()
        .map_err(|e| ServerError::Protocol(e.to_string()))?;

    ws.send(WsMessage::Text(json))
        .await
        .map_err(|e| ServerError::Protocol(e.to_string()))?;

    Ok(())
}

// ============================================================================
// Request Handlers
// ============================================================================

async fn handle_get_status(
    registry: &Arc<Mutex<NodeRegistry>>,
    workload_manager: &Arc<Mutex<WorkloadManager>>,
    start_time: Instant,
) -> CliResponse {
    let registry = registry.lock().await;
    let workload_mgr = workload_manager.lock().await;

    let nodes = registry.list_nodes();
    let health_summary = registry.health_summary();

    let node_count = nodes.len() as u32;
    let healthy_nodes = health_summary.healthy as u32;

    let mut gpu_count = 0u32;
    let mut total_vram_mib = 0u64;

    for node in &nodes {
        gpu_count += node.capabilities.gpus.len() as u32;
        total_vram_mib += node.capabilities.total_vram_mib();
    }

    let active_workloads = workload_mgr.len() as u32;
    let uptime_secs = start_time.elapsed().as_secs();

    CliResponse::Status {
        node_count,
        healthy_nodes,
        gpu_count,
        active_workloads,
        total_vram_mib,
        gateway_version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs,
    }
}

/// Map internal health status to CLI NodeState.
fn health_to_node_state(health: NodeHealthStatus) -> NodeState {
    match health {
        NodeHealthStatus::Healthy => NodeState::Healthy,
        NodeHealthStatus::Unhealthy => NodeState::Unhealthy,
        NodeHealthStatus::Draining => NodeState::Draining,
        NodeHealthStatus::Offline => NodeState::Offline,
    }
}

async fn handle_list_nodes(
    registry: &Arc<Mutex<NodeRegistry>>,
    state_filter: Option<NodeState>,
    include_capabilities: bool,
) -> CliResponse {
    let registry = registry.lock().await;
    let nodes = registry.list_nodes();

    let node_infos: Vec<NodeInfo> = nodes
        .into_iter()
        .filter(|n| {
            if let Some(ref filter) = state_filter {
                health_to_node_state(n.health_status()) == *filter
            } else {
                true
            }
        })
        .map(|n| NodeInfo {
            node_id: n.id,
            name: n.name.clone(),
            state: health_to_node_state(n.health_status()),
            gpu_count: n.capabilities.gpus.len() as u32,
            total_vram_mib: n.capabilities.total_vram_mib(),
            running_workloads: 0, // Would need cross-reference with workload manager
            last_heartbeat: Some(n.last_heartbeat),
            capabilities: if include_capabilities {
                Some(n.capabilities.clone())
            } else {
                None
            },
            gpu_metrics: None,
        })
        .collect();

    CliResponse::Nodes { nodes: node_infos }
}

async fn handle_get_node(registry: &Arc<Mutex<NodeRegistry>>, node_id: NodeId) -> CliResponse {
    let registry = registry.lock().await;

    match registry.get_node(node_id) {
        Some(node) => CliResponse::Node {
            node: NodeInfo {
                node_id: node.id,
                name: node.name.clone(),
                state: health_to_node_state(node.health_status()),
                gpu_count: node.capabilities.gpus.len() as u32,
                total_vram_mib: node.capabilities.total_vram_mib(),
                running_workloads: 0,
                last_heartbeat: Some(node.last_heartbeat),
                capabilities: Some(node.capabilities.clone()),
                gpu_metrics: None,
            },
        },
        None => CliResponse::error(
            cli::error_codes::NODE_NOT_FOUND,
            format!("node not found: {node_id}"),
        ),
    }
}

async fn handle_list_workloads(
    workload_manager: &Arc<Mutex<WorkloadManager>>,
    node_filter: Option<NodeId>,
    state_filter: Option<WorkloadState>,
) -> CliResponse {
    let mgr = workload_manager.lock().await;
    let workloads = mgr.list_workloads();

    let workload_infos: Vec<WorkloadInfo> = workloads
        .into_iter()
        .filter(|w| {
            // Apply node filter
            if let Some(ref filter_node) = node_filter {
                if w.assigned_node != Some(*filter_node) {
                    return false;
                }
            }
            // Apply state filter
            if let Some(ref filter_state) = state_filter {
                if w.state() != *filter_state {
                    return false;
                }
            }
            true
        })
        .map(|w| WorkloadInfo {
            workload_id: w.id(),
            node_id: w.assigned_node.unwrap_or_else(NodeId::new),
            state: w.state(),
            image: w.workload.spec.image.clone(),
            created_at: w.submitted_at,
            started_at: w.workload.status.started_at,
            finished_at: w.workload.status.finished_at,
            status: None,
        })
        .collect();

    CliResponse::Workloads {
        workloads: workload_infos,
    }
}

async fn handle_get_workload(
    workload_manager: &Arc<Mutex<WorkloadManager>>,
    workload_id: WorkloadId,
) -> CliResponse {
    let mgr = workload_manager.lock().await;

    match mgr.get_workload(workload_id) {
        Some(w) => CliResponse::Workload {
            workload: WorkloadInfo {
                workload_id: w.id(),
                node_id: w.assigned_node.unwrap_or_else(NodeId::new),
                state: w.state(),
                image: w.workload.spec.image.clone(),
                created_at: w.submitted_at,
                started_at: w.workload.status.started_at,
                finished_at: w.workload.status.finished_at,
                status: Some(w.workload.status.clone()),
            },
        },
        None => CliResponse::error(
            cli::error_codes::WORKLOAD_NOT_FOUND,
            format!("workload not found: {workload_id}"),
        ),
    }
}

async fn handle_start_workload(
    workload_manager: &Arc<Mutex<WorkloadManager>>,
    registry: &Arc<Mutex<NodeRegistry>>,
    target_node: Option<NodeId>,
    spec: WorkloadSpec,
) -> CliResponse {
    let mut mgr = workload_manager.lock().await;
    let registry = registry.lock().await;

    // Find a node to run the workload
    let node_id = if let Some(id) = target_node {
        // Verify the node exists and is available
        match registry.get_node(id) {
            Some(node) => {
                if !node.is_available() {
                    return CliResponse::error(
                        cli::error_codes::NO_CAPACITY,
                        format!("node {id} is not available (status: {})", node.health_status()),
                    );
                }
                id
            }
            None => {
                return CliResponse::error(
                    cli::error_codes::NODE_NOT_FOUND,
                    format!("target node not found: {id}"),
                );
            }
        }
    } else {
        // Auto-select an available node with capacity
        let candidates: Vec<_> = registry
            .available_nodes()
            .into_iter()
            .filter(|n| n.capabilities.gpus.len() as u32 >= spec.gpu_count)
            .collect();

        if candidates.is_empty() {
            return CliResponse::error(
                cli::error_codes::NO_CAPACITY,
                "no available nodes with sufficient capacity",
            );
        }

        // Pick first available (could be smarter)
        candidates[0].id
    };

    // Submit the workload
    let workload_id = match mgr.submit(spec) {
        Ok(id) => id,
        Err(e) => {
            return CliResponse::error(
                cli::error_codes::INTERNAL_ERROR,
                format!("failed to submit workload: {e}"),
            )
        }
    };

    // Assign to node
    if let Err(e) = mgr.assign_to_node(workload_id, node_id) {
        return CliResponse::error(
            cli::error_codes::INTERNAL_ERROR,
            format!("failed to assign workload to node: {e}"),
        );
    }

    CliResponse::WorkloadStarted {
        workload_id,
        node_id,
    }
}

async fn handle_stop_workload(
    workload_manager: &Arc<Mutex<WorkloadManager>>,
    workload_id: WorkloadId,
    _force: bool,
) -> CliResponse {
    let mut mgr = workload_manager.lock().await;

    if mgr.get_workload(workload_id).is_none() {
        return CliResponse::error(
            cli::error_codes::WORKLOAD_NOT_FOUND,
            format!("workload not found: {workload_id}"),
        );
    }

    if let Err(e) = mgr.cancel(workload_id) {
        return CliResponse::error(
            cli::error_codes::INTERNAL_ERROR,
            format!("failed to stop workload: {e}"),
        );
    }

    CliResponse::WorkloadStopped { workload_id }
}

async fn handle_get_logs(
    log_store: &Arc<Mutex<WorkloadLogStore>>,
    workload_id: WorkloadId,
    tail: Option<u32>,
    include_stderr: bool,
) -> CliResponse {
    let store = log_store.lock().await;

    // Convert tail from u32 to usize
    let tail_usize = tail.map(|t| t as usize);

    match store.get_logs_with_tail(workload_id, tail_usize) {
        Some((stdout_lines, stderr_lines)) => CliResponse::Logs {
            workload_id,
            stdout_lines,
            stderr_lines: if include_stderr {
                stderr_lines
            } else {
                Vec::new()
            },
        },
        None => {
            // If no logs exist for this workload, return empty logs
            // (workload might not have produced output yet)
            CliResponse::Logs {
                workload_id,
                stdout_lines: Vec::new(),
                stderr_lines: Vec::new(),
            }
        }
    }
}

// ============================================================================
// Node Drain Operations
// ============================================================================

async fn handle_drain_node(
    registry: &Arc<Mutex<NodeRegistry>>,
    node_id: NodeId,
    drain: bool,
) -> CliResponse {
    let mut reg = registry.lock().await;

    match reg.set_draining(node_id, drain) {
        Ok(()) => CliResponse::NodeDrained {
            node_id,
            draining: drain,
        },
        Err(_) => CliResponse::error(
            cli::error_codes::NODE_NOT_FOUND,
            format!("node not found: {node_id}"),
        ),
    }
}

// ============================================================================
// MOLT Operations (Placeholder)
// ============================================================================

async fn handle_get_molt_status() -> CliResponse {
    // TODO: Integrate with actual MOLT P2P network
    CliResponse::MoltStatus {
        connected: false,
        peer_count: 0,
        node_id: None,
        region: None,
    }
}

async fn handle_list_molt_peers() -> CliResponse {
    // TODO: Integrate with actual MOLT P2P network
    CliResponse::MoltPeers { peers: Vec::new() }
}

async fn handle_get_molt_balance() -> CliResponse {
    // TODO: Integrate with actual MOLT wallet
    CliResponse::MoltBalance {
        balance: 0,
        pending: 0,
        staked: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version() {
        assert_eq!(CLI_PROTOCOL_VERSION, 1);
    }
}
