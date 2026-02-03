//! Protocol message definitions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::{GpuMetricsProto, NodeCapabilities, NodeId, WorkloadId, WorkloadState};
use crate::workload::WorkloadSpec;

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

/// Messages sent from gateway to node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    fn test_gateway_message() {
        let msg = GatewayMessage::registered(NodeId::new(), 30, 10);
        let json = msg.to_json().unwrap();
        assert!(json.contains("registered"));
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
}
