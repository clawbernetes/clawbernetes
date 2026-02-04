# claw-proto API Reference

Protocol definitions for Clawbernetes gateway-node communication.

## Overview

`claw-proto` defines the message types, data structures, and validation rules used for communication between the gateway and nodes over WebSocket connections.

## Installation

```toml
[dependencies]
claw-proto = { path = "../claw-proto" }
```

## Quick Start

```rust
use claw_proto::{NodeMessage, GatewayMessage, NodeCapabilities, NodeId};

// Create a registration message
let msg = NodeMessage::Register {
    node_id: NodeId::new(),
    name: "gpu-node-01".to_string(),
    capabilities: NodeCapabilities::default(),
    protocol_version: 1,
};

// Serialize to JSON
let json = serde_json::to_string(&msg)?;
```

---

## Node Messages

Messages sent from nodes to the gateway.

### `NodeMessage`

```rust
pub enum NodeMessage {
    /// Node registration on connect
    Register {
        node_id: NodeId,
        name: String,
        capabilities: NodeCapabilities,
        protocol_version: u32,
    },
    
    /// Periodic heartbeat
    Heartbeat {
        node_id: NodeId,
        timestamp: DateTime<Utc>,
    },
    
    /// GPU and system metrics
    Metrics {
        node_id: NodeId,
        gpu_metrics: Vec<GpuMetricsProto>,
        timestamp: DateTime<Utc>,
    },
    
    /// Workload state change
    WorkloadUpdate {
        workload_id: WorkloadId,
        state: WorkloadState,
        message: Option<String>,
        timestamp: DateTime<Utc>,
    },
    
    /// Workload log output
    WorkloadLogs {
        workload_id: WorkloadId,
        lines: Vec<String>,
        is_stderr: bool,
    },
}
```

### Example: Registration

```rust
use claw_proto::{NodeMessage, NodeId, NodeCapabilities, GpuCapability};

let capabilities = NodeCapabilities {
    total_vram_mb: 81920,  // 80GB
    gpu_count: 8,
    gpus: vec![
        GpuCapability {
            index: 0,
            name: "NVIDIA H100".to_string(),
            vram_mb: 81920,
            compute_capability: "9.0".to_string(),
            driver_version: "535.104.05".to_string(),
        },
        // ... more GPUs
    ],
    max_concurrent_workloads: 16,
    supported_runtimes: vec!["docker".to_string(), "containerd".to_string()],
};

let register = NodeMessage::Register {
    node_id: NodeId::new(),
    name: "h100-cluster-01".to_string(),
    capabilities,
    protocol_version: 1,
};
```

---

## Gateway Messages

Messages sent from the gateway to nodes.

### `GatewayMessage`

```rust
pub enum GatewayMessage {
    /// Registration acknowledgment
    Registered {
        heartbeat_interval_secs: u32,
        metrics_interval_secs: u32,
        config: Option<NodeConfig>,
    },
    
    /// Heartbeat acknowledgment
    HeartbeatAck {
        timestamp: DateTime<Utc>,
    },
    
    /// Deploy a workload
    DeployWorkload {
        workload: Workload,
    },
    
    /// Stop a workload
    StopWorkload {
        workload_id: WorkloadId,
        force: bool,
    },
    
    /// Request workload status
    GetWorkloadStatus {
        workload_id: WorkloadId,
    },
    
    /// Update node configuration
    UpdateConfig {
        config: NodeConfig,
    },
    
    /// Ping for latency measurement
    Ping {
        timestamp: DateTime<Utc>,
    },
    
    /// Request node to shutdown
    Shutdown {
        reason: String,
        graceful: bool,
    },
}
```

### Example: Deploy Workload

```rust
use claw_proto::{GatewayMessage, Workload, WorkloadSpec, WorkloadId};

let workload = Workload {
    id: WorkloadId::new(),
    name: "llama-70b-inference".to_string(),
    spec: WorkloadSpec {
        image: "clawbernetes/llama:70b".to_string(),
        gpu_count: 4,
        gpu_memory_mb: Some(40960),
        env: vec![
            ("MODEL_PATH".to_string(), "/models/llama-70b".to_string()),
        ],
        ports: vec![8080],
        ..Default::default()
    },
    ..Default::default()
};

let deploy = GatewayMessage::DeployWorkload { workload };
```

---

## Types

### `NodeId`

Unique identifier for nodes (UUID v4).

```rust
pub struct NodeId(Uuid);

impl NodeId {
    pub fn new() -> Self;                    // Generate new UUID
    pub fn from_str(s: &str) -> Result<Self>;
}
```

### `WorkloadId`

Unique identifier for workloads (UUID v4).

```rust
pub struct WorkloadId(Uuid);

impl WorkloadId {
    pub fn new() -> Self;
    pub fn from_str(s: &str) -> Result<Self>;
}
```

### `NodeCapabilities`

Hardware capabilities reported by nodes.

```rust
pub struct NodeCapabilities {
    pub total_vram_mb: u64,
    pub gpu_count: u32,
    pub gpus: Vec<GpuCapability>,
    pub max_concurrent_workloads: u32,
    pub supported_runtimes: Vec<String>,
}
```

### `GpuCapability`

Individual GPU information.

```rust
pub struct GpuCapability {
    pub index: u32,
    pub name: String,
    pub vram_mb: u64,
    pub compute_capability: String,
    pub driver_version: String,
}
```

### `GpuMetricsProto`

GPU runtime metrics.

```rust
pub struct GpuMetricsProto {
    pub index: u32,
    pub utilization_percent: u32,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub temperature_celsius: u32,
    pub power_watts: u32,
}
```

### `WorkloadState`

Workload lifecycle states.

```rust
pub enum WorkloadState {
    Pending,
    Pulling,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
    Completed,
}
```

---

## Workload Specification

### `Workload`

Full workload definition.

```rust
pub struct Workload {
    pub id: WorkloadId,
    pub name: String,
    pub spec: WorkloadSpec,
    pub status: WorkloadStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### `WorkloadSpec`

Workload specification (container config).

```rust
pub struct WorkloadSpec {
    pub image: String,
    pub command: Option<Vec<String>>,
    pub args: Option<Vec<String>>,
    pub env: Vec<(String, String)>,
    pub gpu_count: u32,
    pub gpu_memory_mb: Option<u64>,
    pub cpu_millicores: Option<u32>,
    pub memory_mb: Option<u64>,
    pub ports: Vec<u16>,
    pub volumes: Vec<VolumeMount>,
    pub restart_policy: RestartPolicy,
}
```

---

## Configuration

### `NodeConfig`

Runtime configuration updates.

```rust
pub struct NodeConfig {
    pub heartbeat_interval_secs: Option<u32>,
    pub metrics_interval_secs: Option<u32>,
    pub max_concurrent_workloads: Option<u32>,
    pub log_level: Option<String>,
}
```

---

## Validation

Built-in validation helpers.

```rust
use claw_proto::validation::{validate_workload_spec, ValidationError};

let spec = WorkloadSpec { /* ... */ };
validate_workload_spec(&spec)?;  // Returns ValidationError if invalid
```

### Validation Rules

| Field | Rule |
|-------|------|
| `image` | Non-empty, valid container image format |
| `gpu_count` | 0-64 |
| `gpu_memory_mb` | 0-1,048,576 (1TB max) |
| `cpu_millicores` | 0-1,024,000 |
| `memory_mb` | 0-4,194,304 (4TB max) |
| `ports` | Valid port range (1-65535) |

---

## Events

### `WorkloadEvent`

Workload lifecycle events for audit logging.

```rust
pub struct WorkloadEvent {
    pub kind: WorkloadEventKind,
    pub workload_id: WorkloadId,
    pub metadata: EventMetadata,
    pub timestamp: DateTime<Utc>,
}

pub enum WorkloadEventKind {
    Created,
    Scheduled { node_id: NodeId },
    Started,
    Stopped { reason: String },
    Failed { error: String },
    Completed { exit_code: i32 },
}
```

---

## Serialization

All types implement `Serialize` and `Deserialize` via serde:

```rust
// JSON serialization
let json = serde_json::to_string(&message)?;
let parsed: NodeMessage = serde_json::from_str(&json)?;

// Binary serialization (with bincode)
let bytes = bincode::serialize(&message)?;
let parsed: NodeMessage = bincode::deserialize(&bytes)?;
```

## Error Handling

```rust
use claw_proto::ProtoError;

pub enum ProtoError {
    /// Invalid message format
    InvalidMessage(String),
    /// Validation failed
    ValidationError(String),
    /// Serialization error
    SerializationError(String),
    /// Unknown message type
    UnknownMessageType(String),
}
```

---

## Protocol Version

Current protocol version: **1**

Version negotiation happens during registration. Nodes and gateways must agree on the major version.
