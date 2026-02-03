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
}
