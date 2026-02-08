//! CLI protocol messages for admin/control operations.
//!
//! This module defines the protocol between `claw-cli` and `claw-gateway-server`.
//! CLI connections are distinct from node connections and support administrative
//! operations like listing nodes, managing workloads, and querying status.
//!
//! # Message Flow
//!
//! ```text
//! ┌──────────┐     CliMessage      ┌─────────────────┐
//! │ claw-cli │────────────────────►│  GatewayServer  │
//! │          │◄────────────────────│                 │
//! └──────────┘     CliResponse     └─────────────────┘
//! ```
//!
//! # Example
//!
//! ```rust
//! use claw_proto::cli::{CliMessage, CliResponse};
//!
//! // Request cluster status
//! let request = CliMessage::GetStatus;
//! let json = request.to_json().unwrap();
//! assert!(json.contains("get_status"));
//!
//! // Parse response
//! let response: CliResponse = serde_json::from_str(r#"
//!     {
//!         "type": "status",
//!         "node_count": 5,
//!         "healthy_nodes": 4,
//!         "gpu_count": 12,
//!         "active_workloads": 2,
//!         "total_vram_mib": 98304,
//!         "gateway_version": "0.1.0",
//!         "uptime_secs": 3600
//!     }
//! "#).unwrap();
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::{GpuMetricsProto, NodeCapabilities, NodeId, WorkloadId, WorkloadState};
use crate::workload::{WorkloadSpec, WorkloadStatus};
use crate::ProtoError;

/// Protocol version for CLI communication.
pub const CLI_PROTOCOL_VERSION: u32 = 1;

/// Messages sent from CLI to gateway.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CliMessage {
    /// Handshake to identify as CLI client.
    Hello {
        /// Client version.
        version: String,
        /// Protocol version.
        protocol_version: u32,
    },

    /// Request cluster status overview.
    GetStatus,

    /// List all registered nodes.
    ListNodes {
        /// Filter: only show nodes with this state.
        state_filter: Option<NodeState>,
        /// Include detailed capabilities.
        include_capabilities: bool,
    },

    /// Get detailed info for a specific node.
    GetNode {
        /// Node ID.
        node_id: NodeId,
    },

    /// List all workloads.
    ListWorkloads {
        /// Filter by node.
        node_filter: Option<NodeId>,
        /// Filter by state.
        state_filter: Option<WorkloadState>,
    },

    /// Get detailed info for a specific workload.
    GetWorkload {
        /// Workload ID.
        workload_id: WorkloadId,
    },

    /// Start a new workload.
    StartWorkload {
        /// Target node (None for auto-scheduling).
        node_id: Option<NodeId>,
        /// Workload specification.
        spec: WorkloadSpec,
    },

    /// Stop a running workload.
    StopWorkload {
        /// Workload ID.
        workload_id: WorkloadId,
        /// Force stop (no grace period).
        force: bool,
    },

    /// Get workload logs.
    GetLogs {
        /// Workload ID.
        workload_id: WorkloadId,
        /// Number of lines (None for all).
        tail: Option<u32>,
        /// Include stderr.
        include_stderr: bool,
    },

    /// Drain or undrain a node.
    ///
    /// Draining a node prevents new workloads from being scheduled on it,
    /// but allows existing workloads to continue running.
    DrainNode {
        /// Node ID.
        node_id: NodeId,
        /// Whether to drain (true) or undrain (false).
        drain: bool,
    },

    /// Get MOLT network status.
    GetMoltStatus,

    /// List MOLT peers.
    ListMoltPeers,

    /// Get MOLT wallet balance.
    GetMoltBalance,

    /// Ping to check connection.
    Ping {
        /// Timestamp for latency measurement.
        timestamp: DateTime<Utc>,
    },

    // =========================================================================
    // WireGuard Mesh Commands
    // =========================================================================

    /// Get mesh network status.
    GetMeshStatus,

    /// List mesh peers.
    ListMeshPeers,

    /// Get mesh node info for a specific node.
    GetMeshNode {
        /// Node ID.
        node_id: NodeId,
    },

    // =========================================================================
    // Advanced Scheduling Commands
    // =========================================================================

    /// Clear a scheduling gate for a workload.
    ClearGate {
        /// Workload ID.
        workload_id: WorkloadId,
        /// Gate name to clear.
        gate_name: String,
    },

    /// Get scheduling status for a workload.
    GetSchedulingStatus {
        /// Workload ID.
        workload_id: WorkloadId,
    },

    /// List workloads that are gated (waiting for conditions).
    ListGatedWorkloads,

    /// Update a node's conditions.
    UpdateNodeCondition {
        /// Node ID.
        node_id: NodeId,
        /// Condition type (e.g., "cuda-12-ready", "model-cached").
        condition_type: String,
        /// Whether the condition is satisfied.
        satisfied: bool,
        /// Optional reason.
        reason: Option<String>,
    },

    /// Set a node label.
    SetNodeLabel {
        /// Node ID.
        node_id: NodeId,
        /// Label key.
        key: String,
        /// Label value.
        value: String,
    },

    /// Remove a node label.
    RemoveNodeLabel {
        /// Node ID.
        node_id: NodeId,
        /// Label key to remove.
        key: String,
    },

    /// Get node conditions and labels.
    GetNodeConditions {
        /// Node ID.
        node_id: NodeId,
    },

    // =========================================================================
    // Node Invoke Commands
    // =========================================================================

    /// Invoke a command on a specific node.
    ///
    /// This is the primary bridge for executing any of the 91 clawnode commands
    /// from the control plane (CLI, Morpheus AI agent, OpenClaw skills).
    NodeInvoke {
        /// Target node ID.
        node_id: NodeId,
        /// Command to execute (e.g., "secret.create", "gpu.list").
        command: String,
        /// Command parameters as JSON string.
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<String>,
        /// Timeout in milliseconds (default: 30000).
        #[serde(default = "default_invoke_timeout")]
        timeout_ms: u64,
    },
}

/// State of a node.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeState {
    /// Node is healthy and accepting workloads.
    Healthy,
    /// Node is unhealthy (missed heartbeats, high temps, etc.).
    Unhealthy,
    /// Node is draining (not accepting new workloads).
    Draining,
    /// Node is offline.
    Offline,
}

impl std::fmt::Display for NodeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Unhealthy => write!(f, "unhealthy"),
            Self::Draining => write!(f, "draining"),
            Self::Offline => write!(f, "offline"),
        }
    }
}

/// Responses sent from gateway to CLI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CliResponse {
    /// Handshake accepted.
    Welcome {
        /// Server version.
        server_version: String,
        /// Protocol version.
        protocol_version: u32,
    },

    /// Cluster status overview.
    Status {
        /// Total number of nodes.
        node_count: u32,
        /// Number of healthy nodes.
        healthy_nodes: u32,
        /// Total number of GPUs.
        gpu_count: u32,
        /// Number of active workloads.
        active_workloads: u32,
        /// Total VRAM across all GPUs in MiB.
        total_vram_mib: u64,
        /// Gateway version.
        gateway_version: String,
        /// Server uptime in seconds.
        uptime_secs: u64,
    },

    /// List of nodes.
    Nodes {
        /// Node information.
        nodes: Vec<NodeInfo>,
    },

    /// Single node details.
    Node {
        /// Node information.
        node: NodeInfo,
    },

    /// List of workloads.
    Workloads {
        /// Workload information.
        workloads: Vec<WorkloadInfo>,
    },

    /// Single workload details.
    Workload {
        /// Workload information.
        workload: WorkloadInfo,
    },

    /// Workload started successfully.
    WorkloadStarted {
        /// Assigned workload ID.
        workload_id: WorkloadId,
        /// Assigned node.
        node_id: NodeId,
    },

    /// Workload stopped.
    WorkloadStopped {
        /// Workload ID.
        workload_id: WorkloadId,
    },

    /// Node drain status changed.
    NodeDrained {
        /// Node ID.
        node_id: NodeId,
        /// Whether the node is now draining.
        draining: bool,
    },

    /// Workload logs.
    Logs {
        /// Workload ID.
        workload_id: WorkloadId,
        /// Log lines (stdout).
        stdout_lines: Vec<String>,
        /// Log lines (stderr).
        stderr_lines: Vec<String>,
    },

    /// MOLT network status.
    MoltStatus {
        /// Whether connected to MOLT network.
        connected: bool,
        /// Number of peers.
        peer_count: u32,
        /// Local node ID on MOLT.
        node_id: Option<String>,
        /// Network region.
        region: Option<String>,
    },

    /// MOLT peers list.
    MoltPeers {
        /// Peer information.
        peers: Vec<MoltPeerInfo>,
    },

    /// MOLT wallet balance.
    MoltBalance {
        /// Balance in smallest unit.
        balance: u64,
        /// Pending balance.
        pending: u64,
        /// Staked amount.
        staked: u64,
    },

    /// Pong response.
    Pong {
        /// Original timestamp.
        client_timestamp: DateTime<Utc>,
        /// Server timestamp.
        server_timestamp: DateTime<Utc>,
    },

    // =========================================================================
    // WireGuard Mesh Responses
    // =========================================================================

    /// Mesh network status.
    MeshStatus {
        /// Whether mesh networking is enabled.
        enabled: bool,
        /// Number of nodes in the mesh.
        node_count: u32,
        /// Number of active connections.
        connection_count: u32,
        /// Mesh network CIDR.
        network_cidr: String,
        /// Topology type (full_mesh, hub_spoke, custom).
        topology_type: String,
    },

    /// List of mesh peers.
    MeshPeers {
        /// Peer information.
        peers: Vec<MeshPeerInfo>,
    },

    /// Single mesh node details.
    MeshNode {
        /// Node information.
        node: MeshNodeInfo,
    },

    // =========================================================================
    // Advanced Scheduling Responses
    // =========================================================================

    /// Gate cleared successfully.
    GateCleared {
        /// Workload ID.
        workload_id: WorkloadId,
        /// Gate that was cleared.
        gate_name: String,
        /// Remaining pending gates.
        pending_gates: Vec<String>,
    },

    /// Scheduling status for a workload.
    SchedulingStatus {
        /// Workload ID.
        workload_id: WorkloadId,
        /// Current workload state.
        state: WorkloadState,
        /// Pending scheduling gates.
        pending_gates: Vec<String>,
        /// Assigned node (if scheduled).
        assigned_node: Option<NodeId>,
        /// Assigned GPU indices (if scheduled).
        assigned_gpus: Vec<u32>,
        /// Worker index for parallel workloads.
        worker_index: Option<u32>,
        /// Why scheduling failed (if applicable).
        schedule_failure_reason: Option<String>,
    },

    /// List of gated workloads.
    GatedWorkloads {
        /// Workloads waiting on gates.
        workloads: Vec<GatedWorkloadInfo>,
    },

    /// Node condition updated.
    NodeConditionUpdated {
        /// Node ID.
        node_id: NodeId,
        /// Condition type.
        condition_type: String,
        /// New status.
        satisfied: bool,
    },

    /// Node label set.
    NodeLabelSet {
        /// Node ID.
        node_id: NodeId,
        /// Label key.
        key: String,
        /// Label value.
        value: String,
    },

    /// Node label removed.
    NodeLabelRemoved {
        /// Node ID.
        node_id: NodeId,
        /// Label key that was removed.
        key: String,
    },

    /// Node conditions and labels.
    NodeConditions {
        /// Node ID.
        node_id: NodeId,
        /// Current conditions.
        conditions: Vec<NodeConditionInfo>,
        /// Current labels.
        labels: Vec<NodeLabelInfo>,
    },

    // =========================================================================
    // Node Invoke Responses
    // =========================================================================

    /// Result of a node invoke command.
    NodeInvokeResult {
        /// Target node ID.
        node_id: NodeId,
        /// Command that was executed.
        command: String,
        /// Whether the command succeeded.
        ok: bool,
        /// Result payload (on success).
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<serde_json::Value>,
        /// Error message (on failure).
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    /// Error response.
    Error {
        /// Error code.
        code: u32,
        /// Error message.
        message: String,
        /// Original request type (if applicable).
        request_type: Option<String>,
    },
}

/// Information about a registered node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeInfo {
    /// Node ID.
    pub node_id: NodeId,
    /// Human-readable name.
    pub name: String,
    /// Current state.
    pub state: NodeState,
    /// Number of GPUs.
    pub gpu_count: u32,
    /// Total VRAM in MiB.
    pub total_vram_mib: u64,
    /// Number of running workloads.
    pub running_workloads: u32,
    /// Last heartbeat time.
    pub last_heartbeat: Option<DateTime<Utc>>,
    /// Detailed capabilities (if requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<NodeCapabilities>,
    /// Latest GPU metrics (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_metrics: Option<Vec<GpuMetricsProto>>,
}

/// Information about a workload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkloadInfo {
    /// Workload ID.
    pub workload_id: WorkloadId,
    /// Node running the workload.
    pub node_id: NodeId,
    /// Current state.
    pub state: WorkloadState,
    /// Container image.
    pub image: String,
    /// When the workload was created.
    pub created_at: DateTime<Utc>,
    /// When the workload started running.
    pub started_at: Option<DateTime<Utc>>,
    /// When the workload finished.
    pub finished_at: Option<DateTime<Utc>>,
    /// Detailed status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<WorkloadStatus>,
}

/// Information about a MOLT peer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MoltPeerInfo {
    /// Peer ID.
    pub peer_id: String,
    /// Region.
    pub region: Option<String>,
    /// GPU count.
    pub gpu_count: u32,
    /// Available for workloads.
    pub available: bool,
    /// Latency in milliseconds.
    pub latency_ms: Option<u32>,
}

/// Information about a mesh peer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MeshPeerInfo {
    /// Node ID.
    pub node_id: NodeId,
    /// Node name.
    pub name: String,
    /// Mesh IP address.
    pub mesh_ip: String,
    /// WireGuard public key (truncated for display).
    pub public_key: String,
    /// External endpoint (if known).
    pub endpoint: Option<String>,
    /// Connection state.
    pub state: MeshConnectionState,
    /// Last handshake time.
    pub last_handshake: Option<DateTime<Utc>>,
    /// Bytes received.
    pub rx_bytes: u64,
    /// Bytes transmitted.
    pub tx_bytes: u64,
}

/// Connection state of a mesh peer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MeshConnectionState {
    /// Connected and healthy.
    Connected,
    /// Connecting (no handshake yet).
    Connecting,
    /// Disconnected (handshake timeout).
    Disconnected,
    /// Unknown state.
    Unknown,
}

impl std::fmt::Display for MeshConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connected => write!(f, "connected"),
            Self::Connecting => write!(f, "connecting"),
            Self::Disconnected => write!(f, "disconnected"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Detailed information about a mesh node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MeshNodeInfo {
    /// Node ID.
    pub node_id: NodeId,
    /// Node name.
    pub name: String,
    /// Mesh IP address.
    pub mesh_ip: String,
    /// WireGuard public key.
    pub public_key: String,
    /// External endpoint.
    pub endpoint: Option<String>,
    /// Is this node a hub (for hub-spoke topology).
    pub is_hub: bool,
    /// Connected peer count.
    pub connected_peers: u32,
    /// Total peer count.
    pub total_peers: u32,
    /// When the node joined the mesh.
    pub joined_at: DateTime<Utc>,
}

/// Information about a workload waiting on scheduling gates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatedWorkloadInfo {
    /// Workload ID.
    pub workload_id: WorkloadId,
    /// Container image.
    pub image: String,
    /// Pending gates.
    pub pending_gates: Vec<String>,
    /// When the workload was submitted.
    pub submitted_at: DateTime<Utc>,
    /// How long it's been waiting.
    pub waiting_secs: u64,
}

/// Information about a node condition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeConditionInfo {
    /// Condition type (e.g., "cuda-12-ready").
    pub condition_type: String,
    /// Whether the condition is satisfied.
    pub satisfied: bool,
    /// Reason for the status.
    pub reason: Option<String>,
    /// When the condition was last updated.
    pub last_updated: DateTime<Utc>,
}

/// Information about a node label.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeLabelInfo {
    /// Label key.
    pub key: String,
    /// Label value.
    pub value: String,
}

// === Error codes ===

fn default_invoke_timeout() -> u64 {
    30_000
}

/// Error codes for CLI responses.
pub mod error_codes {
    /// Node not found.
    pub const NODE_NOT_FOUND: u32 = 1001;
    /// Workload not found.
    pub const WORKLOAD_NOT_FOUND: u32 = 1002;
    /// Invalid request.
    pub const INVALID_REQUEST: u32 = 1003;
    /// No capacity available.
    pub const NO_CAPACITY: u32 = 1004;
    /// Permission denied.
    pub const PERMISSION_DENIED: u32 = 1005;
    /// Internal error.
    pub const INTERNAL_ERROR: u32 = 1006;
    /// MOLT not connected.
    pub const MOLT_NOT_CONNECTED: u32 = 1007;
    /// Protocol version mismatch.
    pub const PROTOCOL_MISMATCH: u32 = 1008;
    /// Node invoke timed out.
    pub const NODE_INVOKE_TIMEOUT: u32 = 1009;
}

impl CliMessage {
    /// Create a hello message.
    #[must_use]
    pub fn hello(version: impl Into<String>) -> Self {
        Self::Hello {
            version: version.into(),
            protocol_version: CLI_PROTOCOL_VERSION,
        }
    }

    /// Create a ping message.
    #[must_use]
    pub fn ping() -> Self {
        Self::Ping {
            timestamp: Utc::now(),
        }
    }

    /// Serialize to JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String, ProtoError> {
        serde_json::to_string(self).map_err(|e| ProtoError::Encoding(e.to_string()))
    }

    /// Deserialize from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn from_json(json: &str) -> Result<Self, ProtoError> {
        serde_json::from_str(json).map_err(|e| ProtoError::Decoding(e.to_string()))
    }

    /// Get the request type name for error reporting.
    #[must_use]
    pub fn request_type(&self) -> &'static str {
        match self {
            Self::Hello { .. } => "hello",
            Self::GetStatus => "get_status",
            Self::ListNodes { .. } => "list_nodes",
            Self::GetNode { .. } => "get_node",
            Self::ListWorkloads { .. } => "list_workloads",
            Self::GetWorkload { .. } => "get_workload",
            Self::StartWorkload { .. } => "start_workload",
            Self::StopWorkload { .. } => "stop_workload",
            Self::GetLogs { .. } => "get_logs",
            Self::DrainNode { .. } => "drain_node",
            Self::GetMoltStatus => "get_molt_status",
            Self::ListMoltPeers => "list_molt_peers",
            Self::GetMoltBalance => "get_molt_balance",
            Self::Ping { .. } => "ping",
            Self::GetMeshStatus => "get_mesh_status",
            Self::ListMeshPeers => "list_mesh_peers",
            Self::GetMeshNode { .. } => "get_mesh_node",
            Self::ClearGate { .. } => "clear_gate",
            Self::GetSchedulingStatus { .. } => "get_scheduling_status",
            Self::ListGatedWorkloads => "list_gated_workloads",
            Self::UpdateNodeCondition { .. } => "update_node_condition",
            Self::SetNodeLabel { .. } => "set_node_label",
            Self::RemoveNodeLabel { .. } => "remove_node_label",
            Self::GetNodeConditions { .. } => "get_node_conditions",
            Self::NodeInvoke { .. } => "node_invoke",
        }
    }
}

impl CliResponse {
    /// Create a welcome response.
    #[must_use]
    pub fn welcome(server_version: impl Into<String>) -> Self {
        Self::Welcome {
            server_version: server_version.into(),
            protocol_version: CLI_PROTOCOL_VERSION,
        }
    }

    /// Create an error response.
    #[must_use]
    pub fn error(code: u32, message: impl Into<String>) -> Self {
        Self::Error {
            code,
            message: message.into(),
            request_type: None,
        }
    }

    /// Create an error response with request type.
    #[must_use]
    pub fn error_for_request(code: u32, message: impl Into<String>, request_type: impl Into<String>) -> Self {
        Self::Error {
            code,
            message: message.into(),
            request_type: Some(request_type.into()),
        }
    }

    /// Create a pong response.
    #[must_use]
    pub fn pong(client_timestamp: DateTime<Utc>) -> Self {
        Self::Pong {
            client_timestamp,
            server_timestamp: Utc::now(),
        }
    }

    /// Check if this is an error response.
    #[must_use]
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }

    /// Serialize to JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String, ProtoError> {
        serde_json::to_string(self).map_err(|e| ProtoError::Encoding(e.to_string()))
    }

    /// Deserialize from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn from_json(json: &str) -> Result<Self, ProtoError> {
        serde_json::from_str(json).map_err(|e| ProtoError::Decoding(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello_message() {
        let msg = CliMessage::hello("1.0.0");
        let json = msg.to_json().unwrap();
        assert!(json.contains("hello"));
        assert!(json.contains("1.0.0"));

        let parsed = CliMessage::from_json(&json).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn test_get_status_message() {
        let msg = CliMessage::GetStatus;
        let json = msg.to_json().unwrap();
        assert!(json.contains("get_status"));

        let parsed = CliMessage::from_json(&json).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn test_list_nodes_message() {
        let msg = CliMessage::ListNodes {
            state_filter: Some(NodeState::Healthy),
            include_capabilities: true,
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("list_nodes"));
        assert!(json.contains("healthy"));

        let parsed = CliMessage::from_json(&json).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn test_status_response() {
        let resp = CliResponse::Status {
            node_count: 5,
            healthy_nodes: 4,
            gpu_count: 12,
            active_workloads: 3,
            total_vram_mib: 245760,
            gateway_version: "0.1.0".into(),
            uptime_secs: 3600,
        };
        let json = resp.to_json().unwrap();
        assert!(json.contains("status"));
        assert!(json.contains("245760"));

        let parsed = CliResponse::from_json(&json).unwrap();
        assert_eq!(resp, parsed);
    }

    #[test]
    fn test_error_response() {
        let resp = CliResponse::error(error_codes::NODE_NOT_FOUND, "Node not found");
        assert!(resp.is_error());

        let json = resp.to_json().unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("1001"));
    }

    #[test]
    fn test_error_response_with_request_type() {
        let resp = CliResponse::error_for_request(
            error_codes::NODE_NOT_FOUND,
            "Node not found",
            "get_node",
        );
        let json = resp.to_json().unwrap();
        assert!(json.contains("get_node"));
    }

    #[test]
    fn test_ping_pong() {
        let ping = CliMessage::ping();
        let json = ping.to_json().unwrap();
        assert!(json.contains("ping"));

        if let CliMessage::Ping { timestamp } = ping {
            let pong = CliResponse::pong(timestamp);
            let pong_json = pong.to_json().unwrap();
            assert!(pong_json.contains("pong"));
        }
    }

    #[test]
    fn test_node_info_serialization() {
        let info = NodeInfo {
            node_id: NodeId::new(),
            name: "test-node".into(),
            state: NodeState::Healthy,
            gpu_count: 4,
            total_vram_mib: 98304,
            running_workloads: 2,
            last_heartbeat: Some(Utc::now()),
            capabilities: None,
            gpu_metrics: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("test-node"));
        assert!(json.contains("healthy"));
    }

    #[test]
    fn test_workload_info_serialization() {
        let info = WorkloadInfo {
            workload_id: WorkloadId::new(),
            node_id: NodeId::new(),
            state: WorkloadState::Running,
            image: "nvidia/cuda:12.0".into(),
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            finished_at: None,
            status: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("nvidia/cuda"));
        assert!(json.contains("running"));
    }

    #[test]
    fn test_molt_peer_info_serialization() {
        let info = MoltPeerInfo {
            peer_id: "peer-123".into(),
            region: Some("us-west".into()),
            gpu_count: 8,
            available: true,
            latency_ms: Some(25),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("peer-123"));
        assert!(json.contains("us-west"));
    }

    #[test]
    fn test_request_type() {
        assert_eq!(CliMessage::GetStatus.request_type(), "get_status");
        assert_eq!(CliMessage::hello("1.0").request_type(), "hello");
        assert_eq!(CliMessage::ping().request_type(), "ping");
    }

    #[test]
    fn test_node_state_display() {
        assert_eq!(NodeState::Healthy.to_string(), "healthy");
        assert_eq!(NodeState::Unhealthy.to_string(), "unhealthy");
        assert_eq!(NodeState::Draining.to_string(), "draining");
        assert_eq!(NodeState::Offline.to_string(), "offline");
    }

    #[test]
    fn test_welcome_response() {
        let resp = CliResponse::welcome("0.1.0");
        let json = resp.to_json().unwrap();
        assert!(json.contains("welcome"));
        assert!(json.contains("0.1.0"));
    }

    #[test]
    fn test_start_workload_message() {
        let spec = WorkloadSpec::new("nvidia/cuda:12.0")
            .with_command(vec!["nvidia-smi".into()])
            .with_gpu_count(1)
            .with_memory_mb(8192)
            .with_cpu_cores(2);
        let msg = CliMessage::StartWorkload {
            node_id: None,
            spec,
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("start_workload"));
        assert!(json.contains("nvidia/cuda"));
    }

    #[test]
    fn test_stop_workload_message() {
        let msg = CliMessage::StopWorkload {
            workload_id: WorkloadId::new(),
            force: true,
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("stop_workload"));
        assert!(json.contains("force"));
    }

    #[test]
    fn test_workload_started_response() {
        let resp = CliResponse::WorkloadStarted {
            workload_id: WorkloadId::new(),
            node_id: NodeId::new(),
        };
        let json = resp.to_json().unwrap();
        assert!(json.contains("workload_started"));
    }

    #[test]
    fn test_logs_response() {
        let resp = CliResponse::Logs {
            workload_id: WorkloadId::new(),
            stdout_lines: vec!["line 1".into(), "line 2".into()],
            stderr_lines: vec!["error 1".into()],
        };
        let json = resp.to_json().unwrap();
        assert!(json.contains("logs"));
        assert!(json.contains("line 1"));
        assert!(json.contains("error 1"));
    }

    #[test]
    fn test_molt_status_response() {
        let resp = CliResponse::MoltStatus {
            connected: true,
            peer_count: 15,
            node_id: Some("molt-node-123".into()),
            region: Some("us-west".into()),
        };
        let json = resp.to_json().unwrap();
        assert!(json.contains("molt_status"));
        assert!(json.contains("molt-node-123"));
    }

    #[test]
    fn test_molt_balance_response() {
        let resp = CliResponse::MoltBalance {
            balance: 1_000_000,
            pending: 50_000,
            staked: 500_000,
        };
        let json = resp.to_json().unwrap();
        assert!(json.contains("molt_balance"));
        assert!(json.contains("1000000"));
    }

    // ==================== Mesh CLI Tests ====================

    #[test]
    fn test_get_mesh_status_message() {
        let msg = CliMessage::GetMeshStatus;
        let json = msg.to_json().unwrap();
        assert!(json.contains("get_mesh_status"));
        assert_eq!(msg.request_type(), "get_mesh_status");
    }

    #[test]
    fn test_list_mesh_peers_message() {
        let msg = CliMessage::ListMeshPeers;
        let json = msg.to_json().unwrap();
        assert!(json.contains("list_mesh_peers"));
        assert_eq!(msg.request_type(), "list_mesh_peers");
    }

    #[test]
    fn test_get_mesh_node_message() {
        let msg = CliMessage::GetMeshNode {
            node_id: NodeId::new(),
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("get_mesh_node"));
        assert_eq!(msg.request_type(), "get_mesh_node");
    }

    #[test]
    fn test_mesh_status_response() {
        let resp = CliResponse::MeshStatus {
            enabled: true,
            node_count: 5,
            connection_count: 10,
            network_cidr: "10.100.0.0/16".to_string(),
            topology_type: "full_mesh".to_string(),
        };
        let json = resp.to_json().unwrap();
        assert!(json.contains("mesh_status"));
        assert!(json.contains("10.100.0.0/16"));
        assert!(json.contains("full_mesh"));
    }

    #[test]
    fn test_mesh_peers_response() {
        let resp = CliResponse::MeshPeers {
            peers: vec![MeshPeerInfo {
                node_id: NodeId::new(),
                name: "node-1".to_string(),
                mesh_ip: "10.100.0.5".to_string(),
                public_key: "abc123...".to_string(),
                endpoint: Some("192.168.1.100:51820".to_string()),
                state: MeshConnectionState::Connected,
                last_handshake: Some(Utc::now()),
                rx_bytes: 1024,
                tx_bytes: 2048,
            }],
        };
        let json = resp.to_json().unwrap();
        assert!(json.contains("mesh_peers"));
        assert!(json.contains("10.100.0.5"));
        assert!(json.contains("connected"));
    }

    #[test]
    fn test_mesh_node_response() {
        let resp = CliResponse::MeshNode {
            node: MeshNodeInfo {
                node_id: NodeId::new(),
                name: "node-1".to_string(),
                mesh_ip: "10.100.0.5".to_string(),
                public_key: "abc123...".to_string(),
                endpoint: Some("192.168.1.100:51820".to_string()),
                is_hub: false,
                connected_peers: 3,
                total_peers: 4,
                joined_at: Utc::now(),
            },
        };
        let json = resp.to_json().unwrap();
        assert!(json.contains("mesh_node"));
        assert!(json.contains("node-1"));
    }

    #[test]
    fn test_mesh_connection_state_display() {
        assert_eq!(MeshConnectionState::Connected.to_string(), "connected");
        assert_eq!(MeshConnectionState::Connecting.to_string(), "connecting");
        assert_eq!(MeshConnectionState::Disconnected.to_string(), "disconnected");
        assert_eq!(MeshConnectionState::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_mesh_peer_info_serialization() {
        let info = MeshPeerInfo {
            node_id: NodeId::new(),
            name: "test-node".to_string(),
            mesh_ip: "10.100.0.5".to_string(),
            public_key: "test-key".to_string(),
            endpoint: None,
            state: MeshConnectionState::Connecting,
            last_handshake: None,
            rx_bytes: 0,
            tx_bytes: 0,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("test-node"));
        assert!(json.contains("connecting"));
    }

    // ==================== Node Invoke CLI Tests ====================

    #[test]
    fn test_node_invoke_message() {
        let msg = CliMessage::NodeInvoke {
            node_id: NodeId::new(),
            command: "gpu.list".to_string(),
            params: None,
            timeout_ms: 30_000,
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("node_invoke"));
        assert!(json.contains("gpu.list"));
        assert_eq!(msg.request_type(), "node_invoke");

        let parsed = CliMessage::from_json(&json).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn test_node_invoke_with_params() {
        let msg = CliMessage::NodeInvoke {
            node_id: NodeId::new(),
            command: "secret.create".to_string(),
            params: Some(r#"{"name":"my-secret","value":"s3cret"}"#.to_string()),
            timeout_ms: 5_000,
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("secret.create"));
        assert!(json.contains("my-secret"));

        let parsed = CliMessage::from_json(&json).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn test_node_invoke_default_timeout() {
        // Deserialize without timeout_ms — should default to 30000
        let json = r#"{"type":"node_invoke","node_id":"00000000-0000-0000-0000-000000000001","command":"gpu.list"}"#;
        let msg = CliMessage::from_json(json).unwrap();
        if let CliMessage::NodeInvoke { timeout_ms, .. } = msg {
            assert_eq!(timeout_ms, 30_000);
        } else {
            panic!("Expected NodeInvoke");
        }
    }

    #[test]
    fn test_node_invoke_result_success() {
        let resp = CliResponse::NodeInvokeResult {
            node_id: NodeId::new(),
            command: "gpu.list".to_string(),
            ok: true,
            payload: Some(serde_json::json!({"gpus": []})),
            error: None,
        };
        let json = resp.to_json().unwrap();
        assert!(json.contains("node_invoke_result"));
        assert!(json.contains("gpu.list"));
        assert!(json.contains("\"ok\":true"));

        let parsed = CliResponse::from_json(&json).unwrap();
        assert_eq!(resp, parsed);
    }

    #[test]
    fn test_node_invoke_result_error() {
        let resp = CliResponse::NodeInvokeResult {
            node_id: NodeId::new(),
            command: "secret.get".to_string(),
            ok: false,
            payload: None,
            error: Some("secret not found".to_string()),
        };
        let json = resp.to_json().unwrap();
        assert!(json.contains("node_invoke_result"));
        assert!(json.contains("\"ok\":false"));
        assert!(json.contains("secret not found"));
    }

    #[test]
    fn test_node_invoke_timeout_error_code() {
        assert_eq!(error_codes::NODE_INVOKE_TIMEOUT, 1009);
    }
}
