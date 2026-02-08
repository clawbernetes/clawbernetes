//! CLI connection handler.
//!
//! Handles WebSocket connections from `claw-cli` clients, distinct from
//! node connections. CLI connections use the CLI protocol for administrative
//! operations like listing nodes, managing workloads, and querying status.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use claw_gateway::{
    AdvancedScheduler, NodeHealthStatus, NodeRegistry, WorkloadLogStore, WorkloadManager,
};
use claw_proto::cli::{
    self, CliMessage, CliResponse, GatedWorkloadInfo, NodeConditionInfo, NodeInfo, NodeLabelInfo,
    NodeState, WorkloadInfo, CLI_PROTOCOL_VERSION,
};
use claw_proto::scheduling::{ConditionStatus, NodeCondition};
use claw_proto::{GatewayMessage, NodeId, WorkloadId, WorkloadSpec, WorkloadState};
use futures::{Sink, SinkExt, Stream, StreamExt};
use tokio::sync::{Mutex, RwLock};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, info, warn};

use crate::config::ServerConfig;
use crate::error::{ServerError, ServerResult};
use crate::molt::MoltIntegration;
use crate::server::{ActiveSession, PendingInvokes};

/// Handle a CLI WebSocket connection.
///
/// This function takes over after the initial Hello message identifies
/// the connection as a CLI client.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_cli_connection<S>(
    mut ws: S,
    registry: Arc<Mutex<NodeRegistry>>,
    workload_manager: Arc<Mutex<WorkloadManager>>,
    log_store: Arc<Mutex<WorkloadLogStore>>,
    scheduler: Arc<Mutex<AdvancedScheduler>>,
    molt: Option<Arc<MoltIntegration>>,
    mesh: Option<Arc<crate::mesh::MeshIntegration>>,
    config: Arc<ServerConfig>,
    sessions: Arc<RwLock<HashMap<uuid::Uuid, ActiveSession>>>,
    pending_invokes: PendingInvokes,
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
            &scheduler,
            &molt,
            &mesh,
            &config,
            &sessions,
            &pending_invokes,
            start_time,
        )
        .await;

        send_response(&mut ws, &response).await?;
    }

    Ok(())
}

/// Handle a single CLI request.
#[allow(clippy::too_many_arguments)]
async fn handle_request(
    request: CliMessage,
    registry: &Arc<Mutex<NodeRegistry>>,
    workload_manager: &Arc<Mutex<WorkloadManager>>,
    log_store: &Arc<Mutex<WorkloadLogStore>>,
    scheduler: &Arc<Mutex<AdvancedScheduler>>,
    molt: &Option<Arc<MoltIntegration>>,
    mesh: &Option<Arc<crate::mesh::MeshIntegration>>,
    _config: &Arc<ServerConfig>,
    sessions: &Arc<RwLock<HashMap<uuid::Uuid, ActiveSession>>>,
    pending_invokes: &PendingInvokes,
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
            handle_start_workload(workload_manager, registry, scheduler, sessions, node_id, spec)
                .await
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

        CliMessage::GetMoltStatus => handle_get_molt_status(molt).await,

        CliMessage::ListMoltPeers => handle_list_molt_peers(molt).await,

        CliMessage::GetMoltBalance => handle_get_molt_balance(molt).await,

        CliMessage::Ping { timestamp } => CliResponse::pong(timestamp),

        // =====================================================================
        // WireGuard Mesh Commands
        // =====================================================================

        CliMessage::GetMeshStatus => handle_get_mesh_status(mesh).await,

        CliMessage::ListMeshPeers => handle_list_mesh_peers(mesh).await,

        CliMessage::GetMeshNode { node_id } => handle_get_mesh_node(mesh, node_id).await,

        // =====================================================================
        // Advanced Scheduling Commands
        // =====================================================================

        CliMessage::ClearGate {
            workload_id,
            gate_name,
        } => handle_clear_gate(scheduler, workload_manager, workload_id, &gate_name).await,

        CliMessage::GetSchedulingStatus { workload_id } => {
            handle_get_scheduling_status(workload_manager, scheduler, workload_id).await
        }

        CliMessage::ListGatedWorkloads => {
            handle_list_gated_workloads(workload_manager, scheduler).await
        }

        CliMessage::UpdateNodeCondition {
            node_id,
            condition_type,
            satisfied,
            reason,
        } => {
            handle_update_node_condition(registry, node_id, &condition_type, satisfied, reason)
                .await
        }

        CliMessage::SetNodeLabel {
            node_id,
            key,
            value,
        } => handle_set_node_label(registry, node_id, &key, &value).await,

        CliMessage::RemoveNodeLabel { node_id, key } => {
            handle_remove_node_label(registry, node_id, &key).await
        }

        CliMessage::GetNodeConditions { node_id } => {
            handle_get_node_conditions(registry, node_id).await
        }

        // =====================================================================
        // Node Invoke Commands
        // =====================================================================

        CliMessage::NodeInvoke {
            node_id,
            command,
            params,
            timeout_ms,
        } => {
            handle_node_invoke(sessions, pending_invokes, node_id, command, params, timeout_ms)
                .await
        }
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
    scheduler: &Arc<Mutex<AdvancedScheduler>>,
    sessions: &Arc<RwLock<HashMap<uuid::Uuid, ActiveSession>>>,
    target_node: Option<NodeId>,
    spec: WorkloadSpec,
) -> CliResponse {
    let mut mgr = workload_manager.lock().await;

    // Submit the workload first to get an ID
    let workload_id = match mgr.submit(spec.clone()) {
        Ok(id) => id,
        Err(e) => {
            return CliResponse::error(
                cli::error_codes::INTERNAL_ERROR,
                format!("failed to submit workload: {e}"),
            )
        }
    };

    // Use the advanced scheduler
    let scheduler = scheduler.lock().await;
    let registry = registry.lock().await;

    let schedule_result = if let Some(target_id) = target_node {
        scheduler.schedule_to_node(workload_id, &spec, target_id, &registry)
    } else {
        scheduler.schedule(workload_id, &spec, &registry)
    };

    drop(scheduler);
    drop(registry);

    match schedule_result {
        Ok(result) => {
            // Assign to node
            if let Err(e) = mgr.assign_to_node(workload_id, result.node_id) {
                return CliResponse::error(
                    cli::error_codes::INTERNAL_ERROR,
                    format!("failed to assign workload to node: {e}"),
                );
            }

            // Dispatch StartWorkload to the target node
            let dispatch_msg = GatewayMessage::StartWorkload {
                workload_id,
                spec: spec.clone(),
            };

            if let Err(e) = dispatch_to_node(sessions, result.node_id, dispatch_msg).await {
                warn!(
                    workload_id = %workload_id,
                    node_id = %result.node_id,
                    error = %e,
                    "failed to dispatch workload to node"
                );
                return CliResponse::error(
                    cli::error_codes::INTERNAL_ERROR,
                    format!("workload scheduled but dispatch to node failed: {e}"),
                );
            }

            info!(
                workload_id = %workload_id,
                node_id = %result.node_id,
                "workload dispatched to node"
            );

            CliResponse::WorkloadStarted {
                workload_id,
                node_id: result.node_id,
            }
        }
        Err(claw_gateway::AdvancedSchedulerError::Gated { pending_gates }) => {
            // Workload is gated - keep it pending
            CliResponse::SchedulingStatus {
                workload_id,
                state: WorkloadState::Pending,
                pending_gates,
                assigned_node: None,
                assigned_gpus: vec![],
                worker_index: None,
                schedule_failure_reason: Some("workload is gated".to_string()),
            }
        }
        Err(claw_gateway::AdvancedSchedulerError::NoNodes) => {
            CliResponse::error(cli::error_codes::NO_CAPACITY, "no nodes registered")
        }
        Err(claw_gateway::AdvancedSchedulerError::NodeNotFound(id)) => {
            CliResponse::error(
                cli::error_codes::NODE_NOT_FOUND,
                format!("target node not found: {id}"),
            )
        }
        Err(claw_gateway::AdvancedSchedulerError::NodeNotAvailable { node_id, reason }) => {
            CliResponse::error(
                cli::error_codes::NO_CAPACITY,
                format!("node {node_id} not available: {reason}"),
            )
        }
        Err(claw_gateway::AdvancedSchedulerError::NoSuitableNode { reason, .. }) => {
            CliResponse::error(cli::error_codes::NO_CAPACITY, reason)
        }
    }
}

/// Dispatch a gateway message to a specific node via its session sender.
async fn dispatch_to_node(
    sessions: &Arc<RwLock<HashMap<uuid::Uuid, ActiveSession>>>,
    node_id: NodeId,
    msg: GatewayMessage,
) -> Result<(), String> {
    let sessions_guard = sessions.read().await;
    for active_session in sessions_guard.values() {
        let session = active_session.session.lock().await;
        if session.node_id() == Some(node_id) {
            return active_session
                .sender
                .try_send(msg)
                .map_err(|e| format!("send failed: {e}"));
        }
    }
    Err(format!("no active session found for node {node_id}"))
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
// MOLT Operations
// ============================================================================

async fn handle_get_molt_status(molt: &Option<Arc<MoltIntegration>>) -> CliResponse {
    match molt {
        Some(m) => {
            let connected = m.is_connected().await;
            let peer_count = m.peer_count().await as u32;
            let node_id = Some(m.peer_id().to_string());
            let region = m.region().map(String::from);

            CliResponse::MoltStatus {
                connected,
                peer_count,
                node_id,
                region,
            }
        }
        None => {
            // MOLT not configured
            CliResponse::MoltStatus {
                connected: false,
                peer_count: 0,
                node_id: None,
                region: None,
            }
        }
    }
}

async fn handle_list_molt_peers(molt: &Option<Arc<MoltIntegration>>) -> CliResponse {
    match molt {
        Some(m) => {
            let peer_ids = m.known_peers().await;
            let peers: Vec<cli::MoltPeerInfo> = peer_ids
                .into_iter()
                .map(|peer_id| cli::MoltPeerInfo {
                    peer_id: peer_id.to_string(),
                    region: None,
                    gpu_count: 0, // Would need to query gossip for capabilities
                    available: true,
                    latency_ms: None,
                })
                .collect();

            CliResponse::MoltPeers { peers }
        }
        None => CliResponse::MoltPeers { peers: Vec::new() },
    }
}

async fn handle_get_molt_balance(molt: &Option<Arc<MoltIntegration>>) -> CliResponse {
    match molt {
        Some(m) => {
            match m.balance_breakdown().await {
                Ok(breakdown) => CliResponse::MoltBalance {
                    balance: breakdown.balance,
                    pending: breakdown.pending,
                    staked: breakdown.staked,
                },
                Err(e) => {
                    warn!(error = %e, "Failed to get MOLT balance breakdown");
                    CliResponse::MoltBalance {
                        balance: 0,
                        pending: 0,
                        staked: 0,
                    }
                }
            }
        }
        None => CliResponse::MoltBalance {
            balance: 0,
            pending: 0,
            staked: 0,
        },
    }
}

// ============================================================================
// WireGuard Mesh Operations
// ============================================================================

async fn handle_get_mesh_status(
    mesh: &Option<Arc<crate::mesh::MeshIntegration>>,
) -> CliResponse {
    match mesh {
        Some(m) => {
            let status = m.status().await;
            CliResponse::MeshStatus {
                enabled: status.enabled,
                node_count: status.node_count,
                connection_count: status.connection_count,
                network_cidr: status.network_cidr,
                topology_type: status.topology_type,
            }
        }
        None => CliResponse::MeshStatus {
            enabled: false,
            node_count: 0,
            connection_count: 0,
            network_cidr: String::new(),
            topology_type: "none".to_string(),
        },
    }
}

async fn handle_list_mesh_peers(
    mesh: &Option<Arc<crate::mesh::MeshIntegration>>,
) -> CliResponse {
    match mesh {
        Some(m) => {
            let nodes = m.list_nodes().await;
            let peers: Vec<cli::MeshPeerInfo> = nodes
                .into_iter()
                .map(|n| cli::MeshPeerInfo {
                    node_id: n.node_id,
                    name: n.name,
                    mesh_ip: n.mesh_ip.to_string(),
                    public_key: truncate_key(&n.public_key),
                    endpoint: n.endpoint,
                    state: if n.last_mesh_ready.is_some() {
                        cli::MeshConnectionState::Connected
                    } else {
                        cli::MeshConnectionState::Connecting
                    },
                    last_handshake: n.last_mesh_ready,
                    rx_bytes: 0, // Would need WireGuard stats
                    tx_bytes: 0,
                })
                .collect();
            CliResponse::MeshPeers { peers }
        }
        None => CliResponse::MeshPeers { peers: Vec::new() },
    }
}

async fn handle_get_mesh_node(
    mesh: &Option<Arc<crate::mesh::MeshIntegration>>,
    node_id: NodeId,
) -> CliResponse {
    match mesh {
        Some(m) => {
            match m.get_node(node_id).await {
                Some(n) => CliResponse::MeshNode {
                    node: cli::MeshNodeInfo {
                        node_id: n.node_id,
                        name: n.name,
                        mesh_ip: n.mesh_ip.to_string(),
                        public_key: n.public_key,
                        endpoint: n.endpoint,
                        is_hub: n.is_hub,
                        connected_peers: n.connected_peers,
                        total_peers: m.node_count().await.saturating_sub(1) as u32,
                        joined_at: n.joined_at,
                    },
                },
                None => CliResponse::error(
                    cli::error_codes::NODE_NOT_FOUND,
                    format!("node {node_id} not in mesh"),
                ),
            }
        }
        None => CliResponse::error(
            cli::error_codes::INTERNAL_ERROR,
            "mesh networking not enabled",
        ),
    }
}

/// Truncates a public key for display.
fn truncate_key(key: &str) -> String {
    if key.len() > 12 {
        format!("{}...", &key[..12])
    } else {
        key.to_string()
    }
}

// ============================================================================
// Advanced Scheduling Operations
// ============================================================================

async fn handle_clear_gate(
    scheduler: &Arc<Mutex<AdvancedScheduler>>,
    workload_manager: &Arc<Mutex<WorkloadManager>>,
    workload_id: WorkloadId,
    gate_name: &str,
) -> CliResponse {
    // Verify workload exists
    let mgr = workload_manager.lock().await;
    let workload = match mgr.get_workload(workload_id) {
        Some(w) => w,
        None => {
            return CliResponse::error(
                cli::error_codes::WORKLOAD_NOT_FOUND,
                format!("workload not found: {workload_id}"),
            )
        }
    };

    // Get pending gates
    let pending_gates: Vec<String> = workload
        .workload
        .spec
        .scheduling
        .scheduling_gates
        .iter()
        .map(|g| g.name.clone())
        .collect();

    drop(mgr);

    // Clear the gate
    let mut scheduler = scheduler.lock().await;
    scheduler.clear_gate(workload_id, gate_name);

    // Calculate remaining pending gates
    let remaining: Vec<String> = pending_gates
        .into_iter()
        .filter(|g| g != gate_name && !scheduler.is_gate_cleared(workload_id, g))
        .collect();

    CliResponse::GateCleared {
        workload_id,
        gate_name: gate_name.to_string(),
        pending_gates: remaining,
    }
}

async fn handle_get_scheduling_status(
    workload_manager: &Arc<Mutex<WorkloadManager>>,
    scheduler: &Arc<Mutex<AdvancedScheduler>>,
    workload_id: WorkloadId,
) -> CliResponse {
    let mgr = workload_manager.lock().await;
    let workload = match mgr.get_workload(workload_id) {
        Some(w) => w,
        None => {
            return CliResponse::error(
                cli::error_codes::WORKLOAD_NOT_FOUND,
                format!("workload not found: {workload_id}"),
            )
        }
    };

    let scheduler = scheduler.lock().await;

    // Get pending gates (those not yet cleared)
    let pending_gates: Vec<String> = workload
        .workload
        .spec
        .scheduling
        .scheduling_gates
        .iter()
        .filter(|g| !scheduler.is_gate_cleared(workload_id, &g.name))
        .map(|g| g.name.clone())
        .collect();

    CliResponse::SchedulingStatus {
        workload_id,
        state: workload.state(),
        pending_gates,
        assigned_node: workload.assigned_node,
        assigned_gpus: workload.assigned_gpus.clone(),
        worker_index: workload.worker_index,
        schedule_failure_reason: workload.schedule_failure.clone(),
    }
}

async fn handle_list_gated_workloads(
    workload_manager: &Arc<Mutex<WorkloadManager>>,
    scheduler: &Arc<Mutex<AdvancedScheduler>>,
) -> CliResponse {
    let mgr = workload_manager.lock().await;
    let scheduler = scheduler.lock().await;
    let now = chrono::Utc::now();

    let gated: Vec<GatedWorkloadInfo> = mgr
        .list_workloads()
        .into_iter()
        .filter(|w| {
            // Check if workload has any pending gates
            w.workload
                .spec
                .scheduling
                .scheduling_gates
                .iter()
                .any(|g| !scheduler.is_gate_cleared(w.id(), &g.name))
        })
        .map(|w| {
            let pending: Vec<String> = w
                .workload
                .spec
                .scheduling
                .scheduling_gates
                .iter()
                .filter(|g| !scheduler.is_gate_cleared(w.id(), &g.name))
                .map(|g| g.name.clone())
                .collect();

            let waiting_secs = (now - w.submitted_at).num_seconds().max(0) as u64;

            GatedWorkloadInfo {
                workload_id: w.id(),
                image: w.workload.spec.image.clone(),
                pending_gates: pending,
                submitted_at: w.submitted_at,
                waiting_secs,
            }
        })
        .collect();

    CliResponse::GatedWorkloads { workloads: gated }
}

async fn handle_update_node_condition(
    registry: &Arc<Mutex<NodeRegistry>>,
    node_id: NodeId,
    condition_type: &str,
    satisfied: bool,
    reason: Option<String>,
) -> CliResponse {
    let mut reg = registry.lock().await;

    let node = match reg.get_node_mut(node_id) {
        Some(n) => n,
        None => {
            return CliResponse::error(
                cli::error_codes::NODE_NOT_FOUND,
                format!("node not found: {node_id}"),
            )
        }
    };

    // Update or add the condition
    let status = if satisfied {
        ConditionStatus::True
    } else {
        ConditionStatus::False
    };

    let mut condition = NodeCondition::new(condition_type, status);
    if let Some(r) = reason {
        condition = condition.with_reason(r);
    }

    // Find existing condition or add new one
    if let Some(existing) = node
        .capabilities
        .conditions
        .iter_mut()
        .find(|c| c.condition_type == condition_type)
    {
        existing.update_status(status);
        if condition.reason.is_some() {
            existing.reason = condition.reason;
        }
    } else {
        node.capabilities.conditions.push(condition);
    }

    CliResponse::NodeConditionUpdated {
        node_id,
        condition_type: condition_type.to_string(),
        satisfied,
    }
}

async fn handle_set_node_label(
    registry: &Arc<Mutex<NodeRegistry>>,
    node_id: NodeId,
    key: &str,
    value: &str,
) -> CliResponse {
    let mut reg = registry.lock().await;

    let node = match reg.get_node_mut(node_id) {
        Some(n) => n,
        None => {
            return CliResponse::error(
                cli::error_codes::NODE_NOT_FOUND,
                format!("node not found: {node_id}"),
            )
        }
    };

    node.capabilities.labels.insert(key.to_string(), value.to_string());

    CliResponse::NodeLabelSet {
        node_id,
        key: key.to_string(),
        value: value.to_string(),
    }
}

async fn handle_remove_node_label(
    registry: &Arc<Mutex<NodeRegistry>>,
    node_id: NodeId,
    key: &str,
) -> CliResponse {
    let mut reg = registry.lock().await;

    let node = match reg.get_node_mut(node_id) {
        Some(n) => n,
        None => {
            return CliResponse::error(
                cli::error_codes::NODE_NOT_FOUND,
                format!("node not found: {node_id}"),
            )
        }
    };

    node.capabilities.labels.remove(key);

    CliResponse::NodeLabelRemoved {
        node_id,
        key: key.to_string(),
    }
}

async fn handle_get_node_conditions(
    registry: &Arc<Mutex<NodeRegistry>>,
    node_id: NodeId,
) -> CliResponse {
    let reg = registry.lock().await;

    let node = match reg.get_node(node_id) {
        Some(n) => n,
        None => {
            return CliResponse::error(
                cli::error_codes::NODE_NOT_FOUND,
                format!("node not found: {node_id}"),
            )
        }
    };

    let conditions: Vec<NodeConditionInfo> = node
        .capabilities
        .conditions
        .iter()
        .map(|c| NodeConditionInfo {
            condition_type: c.condition_type.clone(),
            satisfied: c.is_satisfied(),
            reason: c.reason.clone(),
            last_updated: c.last_probe_time,
        })
        .collect();

    let labels: Vec<NodeLabelInfo> = node
        .capabilities
        .labels
        .iter()
        .map(|(k, v)| NodeLabelInfo {
            key: k.clone(),
            value: v.clone(),
        })
        .collect();

    CliResponse::NodeConditions {
        node_id,
        conditions,
        labels,
    }
}

// ============================================================================
// Node Invoke Operations
// ============================================================================

async fn handle_node_invoke(
    sessions: &Arc<RwLock<HashMap<uuid::Uuid, ActiveSession>>>,
    pending_invokes: &PendingInvokes,
    node_id: NodeId,
    command: String,
    params: Option<String>,
    timeout_ms: u64,
) -> CliResponse {
    info!(
        node_id = %node_id,
        command = %command,
        timeout_ms = timeout_ms,
        "node invoke request"
    );

    // Generate a unique invoke ID for correlation
    let invoke_id = uuid::Uuid::new_v4().to_string();

    // Create oneshot channel for the response
    let (tx, rx) = tokio::sync::oneshot::channel();

    // Store in pending invokes map
    pending_invokes.write().await.insert(invoke_id.clone(), tx);

    // Build the node.invoke.request event payload
    let event_payload = serde_json::json!({
        "id": invoke_id,
        "nodeId": node_id.to_string(),
        "command": command,
        "paramsJSON": params,
        "timeoutMs": timeout_ms,
    });

    // Dispatch to node as a RawEvent (serialized as OpenClaw EventFrame)
    let msg = GatewayMessage::RawEvent {
        event: "node.invoke.request".to_string(),
        payload: event_payload,
    };

    if let Err(e) = dispatch_to_node(sessions, node_id, msg).await {
        // Clean up pending invoke on dispatch failure
        pending_invokes.write().await.remove(&invoke_id);
        return CliResponse::error(
            cli::error_codes::NODE_NOT_FOUND,
            format!("failed to invoke on node {node_id}: {e}"),
        );
    }

    // Await response with timeout
    let timeout_duration = std::time::Duration::from_millis(timeout_ms);
    match tokio::time::timeout(timeout_duration, rx).await {
        Ok(Ok(result)) => {
            info!(
                node_id = %node_id,
                command = %command,
                ok = result.ok,
                "node invoke completed"
            );
            CliResponse::NodeInvokeResult {
                node_id,
                command,
                ok: result.ok,
                payload: result.payload,
                error: result.error,
            }
        }
        Ok(Err(_)) => {
            // Sender dropped — node disconnected during invoke
            warn!(
                node_id = %node_id,
                command = %command,
                "node disconnected during invoke"
            );
            CliResponse::error(
                cli::error_codes::INTERNAL_ERROR,
                format!("node {node_id} disconnected during invoke"),
            )
        }
        Err(_) => {
            // Timeout — clean up pending invoke
            pending_invokes.write().await.remove(&invoke_id);
            warn!(
                node_id = %node_id,
                command = %command,
                timeout_ms = timeout_ms,
                "node invoke timed out"
            );
            CliResponse::error(
                cli::error_codes::NODE_INVOKE_TIMEOUT,
                format!("node invoke timed out after {timeout_ms}ms"),
            )
        }
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
