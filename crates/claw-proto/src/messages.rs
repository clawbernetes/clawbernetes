//! Protocol message definitions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::{GpuMetricsProto, NodeCapabilities, NodeId, WorkloadId, WorkloadState};
use crate::workload::WorkloadSpec;

/// Maximum number of log lines allowed in a single `WorkloadLogs` message.
/// Prevents memory exhaustion attacks from unbounded log submissions.
pub const MAX_WORKLOAD_LOG_LINES: usize = 10_000;

/// Messages sent from node to gateway.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeMessage {
    /// Node registration.
    Register {
        /// Node ID.
        node_id: NodeId,
        /// Node name.
        name: String,
        /// Capabilities.
        capabilities: NodeCapabilities,
        /// Protocol version.
        protocol_version: u32,
        /// Optional WireGuard public key for mesh networking.
        wireguard_public_key: Option<String>,
        /// Optional external endpoint for WireGuard (IP:port).
        wireguard_endpoint: Option<String>,
    },
    /// Heartbeat.
    Heartbeat {
        /// Node ID.
        node_id: NodeId,
        /// Timestamp.
        timestamp: DateTime<Utc>,
    },
    /// Metrics.
    Metrics {
        /// Node ID.
        node_id: NodeId,
        /// GPU metrics.
        gpu_metrics: Vec<GpuMetricsProto>,
        /// Timestamp.
        timestamp: DateTime<Utc>,
    },
    /// Workload update.
    WorkloadUpdate {
        /// Workload ID.
        workload_id: WorkloadId,
        /// State.
        state: WorkloadState,
        /// Message.
        message: Option<String>,
        /// Timestamp.
        timestamp: DateTime<Utc>,
    },
    /// Workload logs.
    WorkloadLogs {
        /// Workload ID.
        workload_id: WorkloadId,
        /// Lines.
        lines: Vec<String>,
        /// Is stderr.
        is_stderr: bool,
    },
    /// Mesh tunnel ready confirmation.
    ///
    /// Sent by node after configuring WireGuard mesh peers.
    MeshReady {
        /// Node ID.
        node_id: NodeId,
        /// Node's mesh IP address.
        mesh_ip: String,
        /// Number of peers successfully configured.
        peer_count: u32,
        /// Optional error message if some peers failed.
        error: Option<String>,
    },
}

/// Configuration update for nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct NodeConfig {
    /// Heartbeat interval in seconds.
    pub heartbeat_interval_secs: Option<u32>,
    /// Metrics reporting interval in seconds.
    pub metrics_interval_secs: Option<u32>,
    /// Maximum concurrent workloads.
    pub max_concurrent_workloads: Option<u32>,
    /// Log level (e.g., "debug", "info", "warn", "error").
    pub log_level: Option<String>,
}

impl NodeConfig {
    /// Create a new empty config.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            heartbeat_interval_secs: None,
            metrics_interval_secs: None,
            max_concurrent_workloads: None,
            log_level: None,
        }
    }

    /// Set heartbeat interval.
    #[must_use]
    pub const fn with_heartbeat_interval(mut self, secs: u32) -> Self {
        self.heartbeat_interval_secs = Some(secs);
        self
    }

    /// Set metrics interval.
    #[must_use]
    pub const fn with_metrics_interval(mut self, secs: u32) -> Self {
        self.metrics_interval_secs = Some(secs);
        self
    }

    /// Set max concurrent workloads.
    #[must_use]
    pub const fn with_max_concurrent_workloads(mut self, max: u32) -> Self {
        self.max_concurrent_workloads = Some(max);
        self
    }

    /// Set log level.
    #[must_use]
    pub fn with_log_level(mut self, level: impl Into<String>) -> Self {
        self.log_level = Some(level.into());
        self
    }
}

/// WireGuard mesh peer configuration sent to nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MeshPeerConfig {
    /// Public key of the peer (base64 encoded).
    pub public_key: String,
    /// Mesh IP address of the peer.
    pub mesh_ip: String,
    /// Optional endpoint (external IP:port) for direct connection.
    pub endpoint: Option<String>,
    /// Persistent keepalive interval in seconds.
    pub keepalive_secs: Option<u16>,
    /// Allowed IPs for this peer.
    pub allowed_ips: Vec<String>,
}

impl MeshPeerConfig {
    /// Creates a new mesh peer config.
    #[must_use]
    pub fn new(public_key: impl Into<String>, mesh_ip: impl Into<String>) -> Self {
        Self {
            public_key: public_key.into(),
            mesh_ip: mesh_ip.into(),
            endpoint: None,
            keepalive_secs: Some(25),
            allowed_ips: Vec::new(),
        }
    }

    /// Sets the endpoint.
    #[must_use]
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Sets the keepalive interval.
    #[must_use]
    pub fn with_keepalive(mut self, secs: u16) -> Self {
        self.keepalive_secs = Some(secs);
        self
    }

    /// Adds an allowed IP.
    #[must_use]
    pub fn with_allowed_ip(mut self, ip: impl Into<String>) -> Self {
        self.allowed_ips.push(ip.into());
        self
    }
}

/// Messages sent from gateway to node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GatewayMessage {
    /// Registered.
    Registered {
        /// Node ID.
        node_id: NodeId,
        /// Heartbeat interval.
        heartbeat_interval_secs: u32,
        /// Metrics interval.
        metrics_interval_secs: u32,
    },
    /// Heartbeat ack.
    HeartbeatAck {
        /// Server time.
        server_time: DateTime<Utc>,
    },
    /// Start workload.
    StartWorkload {
        /// Workload ID.
        workload_id: WorkloadId,
        /// Spec.
        spec: WorkloadSpec,
    },
    /// Stop workload.
    StopWorkload {
        /// Workload ID.
        workload_id: WorkloadId,
        /// Grace period.
        grace_period_secs: u32,
    },
    /// Configuration update.
    ConfigUpdate {
        /// New configuration values.
        config: NodeConfig,
    },
    /// Request metrics.
    RequestMetrics,
    /// Request capabilities.
    RequestCapabilities,
    /// Mesh peer configuration for WireGuard tunnel.
    ///
    /// Sent to nodes when they should add/update peers in their mesh.
    MeshPeerConfig {
        /// The node's assigned mesh IP address.
        node_mesh_ip: String,
        /// The node's WireGuard private key (only sent once on initial config).
        private_key: Option<String>,
        /// Peer configurations to add/update.
        peers: Vec<MeshPeerConfig>,
        /// Mesh network CIDR (e.g., "10.100.0.0/16").
        network_cidr: String,
        /// Listen port for WireGuard.
        listen_port: u16,
    },
    /// Request to remove mesh peers.
    MeshPeerRemove {
        /// Public keys of peers to remove (base64 encoded).
        peer_public_keys: Vec<String>,
    },
    /// Error.
    Error {
        /// Code.
        code: u32,
        /// Message.
        message: String,
    },
}

impl NodeMessage {
    /// Create register message.
    #[must_use]
    pub fn register(node_id: NodeId, name: impl Into<String>, capabilities: NodeCapabilities) -> Self {
        Self::Register {
            node_id,
            name: name.into(),
            capabilities,
            protocol_version: 1,
            wireguard_public_key: None,
            wireguard_endpoint: None,
        }
    }

    /// Create register message with WireGuard mesh info.
    #[must_use]
    pub fn register_with_mesh(
        node_id: NodeId,
        name: impl Into<String>,
        capabilities: NodeCapabilities,
        wireguard_public_key: impl Into<String>,
        wireguard_endpoint: Option<String>,
    ) -> Self {
        Self::Register {
            node_id,
            name: name.into(),
            capabilities,
            protocol_version: 1,
            wireguard_public_key: Some(wireguard_public_key.into()),
            wireguard_endpoint,
        }
    }

    /// Create mesh ready message.
    #[must_use]
    pub fn mesh_ready(node_id: NodeId, mesh_ip: impl Into<String>, peer_count: u32) -> Self {
        Self::MeshReady {
            node_id,
            mesh_ip: mesh_ip.into(),
            peer_count,
            error: None,
        }
    }

    /// Create mesh ready message with error.
    #[must_use]
    pub fn mesh_ready_with_error(
        node_id: NodeId,
        mesh_ip: impl Into<String>,
        peer_count: u32,
        error: impl Into<String>,
    ) -> Self {
        Self::MeshReady {
            node_id,
            mesh_ip: mesh_ip.into(),
            peer_count,
            error: Some(error.into()),
        }
    }

    /// Create heartbeat message.
    #[must_use]
    pub fn heartbeat(node_id: NodeId) -> Self {
        Self::Heartbeat {
            node_id,
            timestamp: Utc::now(),
        }
    }

    /// Create metrics message.
    #[must_use]
    pub fn metrics(node_id: NodeId, gpu_metrics: Vec<GpuMetricsProto>) -> Self {
        Self::Metrics {
            node_id,
            gpu_metrics,
            timestamp: Utc::now(),
        }
    }

    /// Create workload update message.
    #[must_use]
    pub fn workload_update(workload_id: WorkloadId, state: WorkloadState, message: Option<String>) -> Self {
        Self::WorkloadUpdate {
            workload_id,
            state,
            message,
            timestamp: Utc::now(),
        }
    }

    /// Create workload logs message.
    ///
    /// Truncates lines to [`MAX_WORKLOAD_LOG_LINES`] if exceeded.
    #[must_use]
    pub fn workload_logs(workload_id: WorkloadId, lines: Vec<String>, is_stderr: bool) -> Self {
        let truncated_lines = if lines.len() > MAX_WORKLOAD_LOG_LINES {
            lines.into_iter().take(MAX_WORKLOAD_LOG_LINES).collect()
        } else {
            lines
        };
        Self::WorkloadLogs {
            workload_id,
            lines: truncated_lines,
            is_stderr,
        }
    }

    /// Validates the message, returning an error if it violates size limits.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `WorkloadLogs` contains more than [`MAX_WORKLOAD_LOG_LINES`] lines
    pub fn validate(&self) -> Result<(), crate::ProtoError> {
        if let Self::WorkloadLogs { lines, .. } = self {
            if lines.len() > MAX_WORKLOAD_LOG_LINES {
                return Err(crate::ProtoError::Validation(format!(
                    "WorkloadLogs exceeds maximum line count: {} > {}",
                    lines.len(),
                    MAX_WORKLOAD_LOG_LINES
                )));
            }
        }
        Ok(())
    }

    /// Returns whether the message is valid according to size limits.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }

    /// Serialize to JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String, crate::ProtoError> {
        serde_json::to_string(self).map_err(|e| crate::ProtoError::Encoding(e.to_string()))
    }

    /// Deserialize from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn from_json(json: &str) -> Result<Self, crate::ProtoError> {
        serde_json::from_str(json).map_err(|e| crate::ProtoError::Decoding(e.to_string()))
    }
}

impl GatewayMessage {
    /// Create registered response.
    #[must_use]
    pub const fn registered(node_id: NodeId, heartbeat_interval_secs: u32, metrics_interval_secs: u32) -> Self {
        Self::Registered {
            node_id,
            heartbeat_interval_secs,
            metrics_interval_secs,
        }
    }

    /// Create heartbeat ack.
    #[must_use]
    pub fn heartbeat_ack() -> Self {
        Self::HeartbeatAck {
            server_time: Utc::now(),
        }
    }

    /// Create error response.
    #[must_use]
    pub fn error(code: u32, message: impl Into<String>) -> Self {
        Self::Error {
            code,
            message: message.into(),
        }
    }

    /// Create config update message.
    #[must_use]
    pub const fn config_update(config: NodeConfig) -> Self {
        Self::ConfigUpdate { config }
    }

    /// Serialize to JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String, crate::ProtoError> {
        serde_json::to_string(self).map_err(|e| crate::ProtoError::Encoding(e.to_string()))
    }

    /// Deserialize from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn from_json(json: &str) -> Result<Self, crate::ProtoError> {
        serde_json::from_str(json).map_err(|e| crate::ProtoError::Decoding(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_message() {
        let msg = NodeMessage::register(NodeId::new(), "test", NodeCapabilities::default());
        let json = msg.to_json().unwrap();
        assert!(json.contains("register"));
    }

    #[test]
    fn test_register_with_mesh_message() {
        let msg = NodeMessage::register_with_mesh(
            NodeId::new(),
            "test",
            NodeCapabilities::default(),
            "test-public-key",
            Some("192.168.1.100:51820".to_string()),
        );
        let json = msg.to_json().unwrap();
        assert!(json.contains("register"));
        assert!(json.contains("wireguard_public_key"));
        assert!(json.contains("test-public-key"));
    }

    #[test]
    fn test_mesh_ready_message() {
        let msg = NodeMessage::mesh_ready(NodeId::new(), "10.100.0.5", 3);
        let json = msg.to_json().unwrap();
        assert!(json.contains("mesh_ready"));
        assert!(json.contains("10.100.0.5"));
        assert!(json.contains("3"));
    }

    #[test]
    fn test_mesh_ready_with_error_message() {
        let msg = NodeMessage::mesh_ready_with_error(
            NodeId::new(),
            "10.100.0.5",
            2,
            "failed to add peer",
        );
        let json = msg.to_json().unwrap();
        assert!(json.contains("mesh_ready"));
        assert!(json.contains("failed to add peer"));
    }

    #[test]
    fn test_gateway_message() {
        let msg = GatewayMessage::registered(NodeId::new(), 30, 10);
        let json = msg.to_json().unwrap();
        assert!(json.contains("registered"));
    }

    #[test]
    fn test_mesh_peer_config_new() {
        let config = MeshPeerConfig::new("test-key", "10.100.0.5");
        assert_eq!(config.public_key, "test-key");
        assert_eq!(config.mesh_ip, "10.100.0.5");
        assert!(config.endpoint.is_none());
        assert_eq!(config.keepalive_secs, Some(25));
    }

    #[test]
    fn test_mesh_peer_config_builders() {
        let config = MeshPeerConfig::new("test-key", "10.100.0.5")
            .with_endpoint("192.168.1.100:51820")
            .with_keepalive(30)
            .with_allowed_ip("10.100.0.5/32");

        assert_eq!(config.endpoint, Some("192.168.1.100:51820".to_string()));
        assert_eq!(config.keepalive_secs, Some(30));
        assert_eq!(config.allowed_ips, vec!["10.100.0.5/32"]);
    }

    #[test]
    fn test_mesh_peer_config_message() {
        let msg = GatewayMessage::MeshPeerConfig {
            node_mesh_ip: "10.100.0.5".to_string(),
            private_key: Some("test-private-key".to_string()),
            peers: vec![
                MeshPeerConfig::new("peer1-key", "10.100.0.6"),
                MeshPeerConfig::new("peer2-key", "10.100.0.7"),
            ],
            network_cidr: "10.100.0.0/16".to_string(),
            listen_port: 51820,
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("mesh_peer_config"));
        assert!(json.contains("10.100.0.5"));
        assert!(json.contains("peer1-key"));
    }

    #[test]
    fn test_mesh_peer_remove_message() {
        let msg = GatewayMessage::MeshPeerRemove {
            peer_public_keys: vec!["key1".to_string(), "key2".to_string()],
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("mesh_peer_remove"));
        assert!(json.contains("key1"));
    }

    #[test]
    fn test_config_update_message() {
        let config = NodeConfig::new()
            .with_heartbeat_interval(60)
            .with_metrics_interval(30)
            .with_log_level("debug");

        let msg = GatewayMessage::config_update(config);
        let json = msg.to_json().unwrap();
        assert!(json.contains("config_update"));
        assert!(json.contains("heartbeat_interval_secs"));
        assert!(json.contains("60"));
    }

    #[test]
    fn test_node_config_builder() {
        let config = NodeConfig::new()
            .with_heartbeat_interval(30)
            .with_metrics_interval(10)
            .with_max_concurrent_workloads(5)
            .with_log_level("info");

        assert_eq!(config.heartbeat_interval_secs, Some(30));
        assert_eq!(config.metrics_interval_secs, Some(10));
        assert_eq!(config.max_concurrent_workloads, Some(5));
        assert_eq!(config.log_level, Some("info".to_string()));
    }

    #[test]
    fn test_node_config_default_is_empty() {
        let config = NodeConfig::default();
        assert!(config.heartbeat_interval_secs.is_none());
        assert!(config.metrics_interval_secs.is_none());
        assert!(config.max_concurrent_workloads.is_none());
        assert!(config.log_level.is_none());
    }

    #[test]
    fn test_config_update_serialization_roundtrip() {
        let config = NodeConfig::new()
            .with_heartbeat_interval(45)
            .with_log_level("warn");

        let msg = GatewayMessage::config_update(config);
        let json = msg.to_json().unwrap();
        let parsed = GatewayMessage::from_json(&json).unwrap();
        assert_eq!(msg, parsed);
    }

    // ========== WorkloadLogs Limit Tests ==========

    #[test]
    fn test_workload_logs_within_limit() {
        let lines: Vec<String> = (0..100).map(|i| format!("log line {i}")).collect();
        let msg = NodeMessage::workload_logs(WorkloadId::new(), lines.clone(), false);

        if let NodeMessage::WorkloadLogs { lines: result_lines, .. } = msg {
            assert_eq!(result_lines.len(), 100);
        } else {
            panic!("Expected WorkloadLogs");
        }
    }

    #[test]
    fn test_workload_logs_truncates_when_exceeding_limit() {
        let lines: Vec<String> = (0..MAX_WORKLOAD_LOG_LINES + 500)
            .map(|i| format!("log line {i}"))
            .collect();

        let msg = NodeMessage::workload_logs(WorkloadId::new(), lines, false);

        if let NodeMessage::WorkloadLogs { lines: result_lines, .. } = msg {
            assert_eq!(result_lines.len(), MAX_WORKLOAD_LOG_LINES);
            // Verify it kept the first lines (truncated from end)
            assert_eq!(result_lines[0], "log line 0");
            assert_eq!(result_lines[MAX_WORKLOAD_LOG_LINES - 1], format!("log line {}", MAX_WORKLOAD_LOG_LINES - 1));
        } else {
            panic!("Expected WorkloadLogs");
        }
    }

    #[test]
    fn test_workload_logs_validation_passes_within_limit() {
        let lines: Vec<String> = (0..100).map(|i| format!("log line {i}")).collect();
        let msg = NodeMessage::WorkloadLogs {
            workload_id: WorkloadId::new(),
            lines,
            is_stderr: false,
        };

        assert!(msg.validate().is_ok());
        assert!(msg.is_valid());
    }

    #[test]
    fn test_workload_logs_validation_fails_exceeding_limit() {
        // Directly construct a message that exceeds the limit (bypassing constructor)
        let lines: Vec<String> = (0..MAX_WORKLOAD_LOG_LINES + 1)
            .map(|i| format!("log line {i}"))
            .collect();
        let msg = NodeMessage::WorkloadLogs {
            workload_id: WorkloadId::new(),
            lines,
            is_stderr: false,
        };

        assert!(msg.validate().is_err());
        assert!(!msg.is_valid());
    }

    #[test]
    fn test_workload_logs_validation_error_message() {
        let lines: Vec<String> = (0..MAX_WORKLOAD_LOG_LINES + 100)
            .map(|i| format!("log line {i}"))
            .collect();
        let msg = NodeMessage::WorkloadLogs {
            workload_id: WorkloadId::new(),
            lines,
            is_stderr: false,
        };

        let err = msg.validate().unwrap_err();
        let err_msg = err.to_string();
        assert!(err_msg.contains("WorkloadLogs exceeds maximum"));
        assert!(err_msg.contains(&MAX_WORKLOAD_LOG_LINES.to_string()));
    }

    #[test]
    fn test_workload_logs_exactly_at_limit() {
        let lines: Vec<String> = (0..MAX_WORKLOAD_LOG_LINES)
            .map(|i| format!("log line {i}"))
            .collect();
        let msg = NodeMessage::workload_logs(WorkloadId::new(), lines.clone(), false);

        if let NodeMessage::WorkloadLogs { lines: result_lines, .. } = &msg {
            assert_eq!(result_lines.len(), MAX_WORKLOAD_LOG_LINES);
        } else {
            panic!("Expected WorkloadLogs");
        }

        assert!(msg.validate().is_ok());
    }

    #[test]
    fn test_other_messages_always_valid() {
        let msg = NodeMessage::heartbeat(NodeId::new());
        assert!(msg.validate().is_ok());
        assert!(msg.is_valid());

        let msg = NodeMessage::register(NodeId::new(), "test", NodeCapabilities::default());
        assert!(msg.validate().is_ok());
    }
}
