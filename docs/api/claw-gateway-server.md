# claw-gateway-server API Reference

WebSocket gateway server for managing the Clawbernetes node fleet.

## Overview

`claw-gateway-server` is the control plane component that:

- Accepts WebSocket connections from nodes
- Manages node registration and heartbeats
- Dispatches workloads to appropriate nodes
- Aggregates metrics and logs
- Handles node lifecycle events

## Installation

```bash
# From source
cargo install --path crates/claw-gateway-server

# Run
claw-gateway --bind 0.0.0.0:8080
```

## Quick Start

```rust
use claw_gateway_server::{GatewayServer, GatewayConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = GatewayConfig {
        bind_address: "0.0.0.0:8080".parse()?,
        heartbeat_interval_secs: 30,
        metrics_interval_secs: 60,
        ..Default::default()
    };
    
    let server = GatewayServer::new(config)?;
    server.run().await?;
    
    Ok(())
}
```

---

## Gateway Configuration

### `GatewayConfig`

```rust
pub struct GatewayConfig {
    /// Bind address for WebSocket server
    pub bind_address: SocketAddr,
    /// Heartbeat interval for nodes
    pub heartbeat_interval_secs: u32,
    /// Metrics reporting interval
    pub metrics_interval_secs: u32,
    /// Maximum nodes allowed
    pub max_nodes: usize,
    /// Node timeout before disconnect
    pub node_timeout_secs: u32,
    /// TLS configuration
    pub tls: Option<TlsConfig>,
    /// Authentication configuration
    pub auth: Option<AuthConfig>,
}
```

### Configuration File

```toml
# gateway.toml
[server]
bind_address = "0.0.0.0:8080"
max_nodes = 1000
node_timeout_secs = 120

[timing]
heartbeat_interval_secs = 30
metrics_interval_secs = 60

[tls]
cert_path = "/etc/clawbernetes/server.crt"
key_path = "/etc/clawbernetes/server.key"

[auth]
enabled = true
token_secret = "${GATEWAY_TOKEN_SECRET}"
```

---

## GatewayServer

Main server struct.

```rust
pub struct GatewayServer {
    config: GatewayConfig,
    state: Arc<RwLock<GatewayState>>,
}

impl GatewayServer {
    /// Create new server
    pub fn new(config: GatewayConfig) -> Result<Self, GatewayError>;
    
    /// Run the server
    pub async fn run(&self) -> Result<(), GatewayError>;
    
    /// Shutdown gracefully
    pub async fn shutdown(&self) -> Result<(), GatewayError>;
    
    /// Get current state
    pub async fn state(&self) -> GatewayState;
    
    /// Get connected nodes
    pub async fn nodes(&self) -> Vec<NodeInfo>;
    
    /// Get node by ID
    pub async fn get_node(&self, id: &NodeId) -> Option<NodeInfo>;
    
    /// Deploy workload to specific node
    pub async fn deploy_workload(
        &self,
        node_id: &NodeId,
        workload: Workload,
    ) -> Result<WorkloadId, GatewayError>;
    
    /// Deploy workload with auto-scheduling
    pub async fn schedule_workload(
        &self,
        workload: Workload,
    ) -> Result<(NodeId, WorkloadId), GatewayError>;
    
    /// Stop a workload
    pub async fn stop_workload(
        &self,
        workload_id: &WorkloadId,
        force: bool,
    ) -> Result<(), GatewayError>;
}
```

---

## Gateway State

### `GatewayState`

```rust
pub struct GatewayState {
    /// Connected nodes
    pub nodes: HashMap<NodeId, NodeInfo>,
    /// Active workloads
    pub workloads: HashMap<WorkloadId, WorkloadState>,
    /// Total capacity
    pub total_capacity: ClusterCapacity,
    /// Available capacity
    pub available_capacity: ClusterCapacity,
}
```

### `NodeInfo`

```rust
pub struct NodeInfo {
    pub id: NodeId,
    pub name: String,
    pub capabilities: NodeCapabilities,
    pub status: NodeStatus,
    pub connected_at: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
    pub workloads: Vec<WorkloadId>,
    pub metrics: Option<NodeMetrics>,
}

pub enum NodeStatus {
    Connecting,
    Connected,
    Healthy,
    Degraded { reason: String },
    Draining,
    Disconnected,
}
```

### `ClusterCapacity`

```rust
pub struct ClusterCapacity {
    pub total_gpus: u32,
    pub total_vram_gb: u64,
    pub node_count: u32,
    pub by_model: HashMap<String, u32>,
}
```

---

## Session Management

### `Session`

Represents a connected node session.

```rust
pub struct Session {
    pub id: SessionId,
    pub node_id: NodeId,
    pub node_name: String,
    pub connected_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub state: SessionState,
}

pub enum SessionState {
    Registering,
    Active,
    Draining,
    Closing,
}
```

### Session Events

```rust
pub enum SessionEvent {
    Connected { session_id: SessionId },
    Registered { node_id: NodeId, name: String },
    Heartbeat { node_id: NodeId },
    Metrics { node_id: NodeId, metrics: NodeMetrics },
    Disconnected { node_id: NodeId, reason: String },
    Error { node_id: Option<NodeId>, error: String },
}
```

---

## Scheduling

### GPU-Aware Scheduling

```rust
use claw_gateway_server::{Scheduler, SchedulingPolicy, WorkloadSpec};

let scheduler = Scheduler::new(SchedulingPolicy::BestFit);

// Schedule based on GPU requirements
let result = scheduler.schedule(&workload, &available_nodes)?;

match result {
    ScheduleResult::Placed { node_id } => {
        println!("Scheduled on node {}", node_id);
    }
    ScheduleResult::Pending { reason } => {
        println!("Waiting for capacity: {}", reason);
    }
    ScheduleResult::Failed { error } => {
        eprintln!("Cannot schedule: {}", error);
    }
}
```

### `SchedulingPolicy`

```rust
pub enum SchedulingPolicy {
    /// Pack workloads onto fewest nodes
    BinPack,
    /// Spread workloads across nodes
    Spread,
    /// Best fit for GPU requirements
    BestFit,
    /// Random selection
    Random,
    /// Custom scoring function
    Custom(Box<dyn ScoringFn>),
}
```

---

## Workload Management

### Deploy Workload

```rust
use claw_gateway_server::{GatewayServer, Workload, WorkloadSpec};

let workload = Workload {
    id: WorkloadId::new(),
    name: "training-job".to_string(),
    spec: WorkloadSpec {
        image: "pytorch/pytorch:latest".to_string(),
        gpu_count: 4,
        gpu_memory_mb: Some(40960),
        ..Default::default()
    },
    ..Default::default()
};

// Auto-schedule
let (node_id, workload_id) = server.schedule_workload(workload).await?;
println!("Deployed {} on {}", workload_id, node_id);

// Or deploy to specific node
let workload_id = server.deploy_workload(&node_id, workload).await?;
```

### Monitor Workload

```rust
// Get workload status
let status = server.get_workload_status(&workload_id).await?;
println!("State: {:?}", status.state);

// Stream logs
let mut logs = server.stream_logs(&workload_id).await?;
while let Some(line) = logs.next().await {
    println!("{}", line);
}
```

### Stop Workload

```rust
// Graceful stop
server.stop_workload(&workload_id, false).await?;

// Force stop
server.stop_workload(&workload_id, true).await?;
```

---

## Metrics Aggregation

### Cluster Metrics

```rust
use claw_gateway_server::metrics;

// Get cluster-wide metrics
let cluster = server.cluster_metrics().await;
println!("GPU utilization: {}%", cluster.avg_gpu_utilization);
println!("Memory used: {} GB", cluster.total_memory_used_gb);
println!("Active workloads: {}", cluster.active_workloads);

// Per-node metrics
for node in server.nodes().await {
    if let Some(m) = &node.metrics {
        println!("{}: {}% GPU, {}°C", 
            node.name, 
            m.gpu_utilization, 
            m.gpu_temperature
        );
    }
}
```

---

## Error Handling

```rust
pub enum GatewayError {
    /// Bind failed
    BindFailed(String),
    /// TLS configuration error
    TlsError(String),
    /// Node not found
    NodeNotFound(NodeId),
    /// Workload not found
    WorkloadNotFound(WorkloadId),
    /// No capacity available
    NoCapacity { required: ResourceRequest, available: ResourceRequest },
    /// Node communication failed
    NodeCommunication { node_id: NodeId, error: String },
    /// Internal error
    Internal(String),
}
```

---

## CLI Reference

```bash
claw-gateway [OPTIONS]

Options:
  -b, --bind <ADDR>         Bind address [default: 0.0.0.0:8080]
  -c, --config <FILE>       Configuration file
      --tls-cert <FILE>     TLS certificate path
      --tls-key <FILE>      TLS key path
      --log-level <LEVEL>   Log level [default: info]
      --log-format <FMT>    Log format (json, pretty)
  -h, --help                Print help
  -V, --version             Print version

Examples:
  claw-gateway --bind 0.0.0.0:8080
  claw-gateway --config /etc/clawbernetes/gateway.toml
  claw-gateway --bind 0.0.0.0:8443 --tls-cert server.crt --tls-key server.key
```

---

## Health Endpoints

When running, the gateway exposes HTTP health endpoints:

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Basic health check |
| `GET /ready` | Readiness check (has connected nodes) |
| `GET /metrics` | Prometheus metrics |

```bash
# Health check
curl http://localhost:8080/health
# {"status":"healthy","nodes":5,"workloads":12}

# Prometheus metrics
curl http://localhost:8080/metrics
# gateway_connected_nodes 5
# gateway_active_workloads 12
# gateway_total_gpus 40
```

---

## Signals

| Signal | Behavior |
|--------|----------|
| `SIGTERM` | Graceful shutdown (drain workloads) |
| `SIGINT` | Graceful shutdown |
| `SIGHUP` | Reload configuration |
