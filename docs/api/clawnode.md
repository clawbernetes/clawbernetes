# clawnode API Reference

Node agent for GPU detection, workload execution, and metrics collection.

## Overview

`clawnode` is the node agent that runs on each machine in your Clawbernetes cluster. It:

- Connects to the gateway via WebSocket
- Detects GPU hardware capabilities
- Executes container workloads
- Reports metrics and health status
- Handles workload lifecycle management

## Installation

```bash
# From source
cargo install --path crates/clawnode

# Run
clawnode --gateway ws://gateway:8080 --name my-node
```

## Quick Start

```rust
use clawnode::{Node, NodeConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = NodeConfig::from_file("clawnode.toml")?;
    let mut node = Node::new(config).await?;
    node.run().await?;
    Ok(())
}
```

---

## Node Configuration

### `NodeConfig`

```rust
pub struct NodeConfig {
    /// Node display name
    pub name: String,
    /// Gateway WebSocket URL
    pub gateway_url: String,
    /// GPU detection settings
    pub gpu: GpuConfig,
    /// Runtime settings
    pub runtime: RuntimeConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
}
```

### Configuration File

```toml
# clawnode.toml
[node]
name = "gpu-node-01"
gateway_url = "ws://gateway.example.com:8080"

[gpu]
enabled = true
auto_detect = true
allow_mig = true

[runtime]
type = "docker"  # or "containerd"
socket = "/var/run/docker.sock"
default_gpu_memory_mb = 8192

[logging]
level = "info"
format = "json"
```

### Loading Configuration

```rust
use clawnode::NodeConfig;

// From file
let config = NodeConfig::from_file("clawnode.toml")?;

// From environment
let config = NodeConfig::from_env()?;

// Programmatic
let config = NodeConfig {
    name: "my-node".to_string(),
    gateway_url: "ws://localhost:8080".to_string(),
    gpu: GpuConfig::default(),
    runtime: RuntimeConfig::default(),
    logging: LoggingConfig::default(),
};
```

---

## Node Lifecycle

### `Node`

The main node orchestrator.

```rust
pub struct Node {
    // Internal state
}

impl Node {
    /// Create a new node with configuration
    pub async fn new(config: NodeConfig) -> Result<Self, NodeError>;
    
    /// Start the main run loop
    pub async fn run(&mut self) -> Result<(), NodeError>;
    
    /// Get current lifecycle state
    pub fn state(&self) -> NodeLifecycleState;
    
    /// Initiate graceful shutdown
    pub async fn shutdown(&mut self) -> Result<(), NodeError>;
}
```

### `NodeLifecycleState`

```rust
pub enum NodeLifecycleState {
    /// Node is initializing
    Initializing,
    /// Node is running and connected
    Running,
    /// Node is reconnecting to gateway
    Reconnecting,
    /// Node is shutting down
    ShuttingDown,
    /// Node has stopped
    Stopped,
}
```

### Lifecycle Example

```rust
use clawnode::{Node, NodeConfig, NodeLifecycleState};
use tokio::signal;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = NodeConfig::from_env()?;
    let mut node = Node::new(config).await?;
    
    // Run until shutdown signal
    tokio::select! {
        result = node.run() => {
            if let Err(e) = result {
                eprintln!("Node error: {}", e);
            }
        }
        _ = signal::ctrl_c() => {
            println!("Shutting down...");
            node.shutdown().await?;
        }
    }
    
    Ok(())
}
```

---

## GPU Detection

### `GpuDetector` Trait

```rust
pub trait GpuDetector: Send + Sync {
    /// Detect available GPUs
    fn detect(&self) -> Result<Vec<GpuInfo>, NodeError>;
    
    /// Get real-time metrics for a GPU
    fn get_metrics(&self, index: u32) -> Result<GpuMetrics, NodeError>;
}
```

### `GpuInfo`

```rust
pub struct GpuInfo {
    pub index: u32,
    pub name: String,
    pub vram_mb: u64,
    pub compute_capability: String,
    pub driver_version: String,
    pub pci_bus_id: Option<String>,
    pub uuid: Option<String>,
}
```

### `GpuMetrics`

```rust
pub struct GpuMetrics {
    pub index: u32,
    pub utilization_percent: u32,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub temperature_celsius: u32,
    pub power_watts: u32,
    pub fan_speed_percent: Option<u32>,
}
```

### NVIDIA Detection

```rust
use clawnode::gpu::{NvidiaDetector, GpuDetector};

let detector = NvidiaDetector::new()?;
let gpus = detector.detect()?;

for gpu in gpus {
    println!("GPU {}: {} ({} MB)", gpu.index, gpu.name, gpu.vram_mb);
    
    let metrics = detector.get_metrics(gpu.index)?;
    println!("  Utilization: {}%", metrics.utilization_percent);
    println!("  Temperature: {}°C", metrics.temperature_celsius);
}
```

---

## Container Runtime

### `ContainerRuntime` Trait

```rust
pub trait ContainerRuntime: Send + Sync {
    /// Pull a container image
    async fn pull_image(&self, image: &str) -> Result<(), NodeError>;
    
    /// Start a container
    async fn start_container(&self, spec: &ContainerSpec) -> Result<ContainerId, NodeError>;
    
    /// Stop a container
    async fn stop_container(&self, id: &ContainerId, force: bool) -> Result<(), NodeError>;
    
    /// Get container status
    async fn get_status(&self, id: &ContainerId) -> Result<ContainerStatus, NodeError>;
    
    /// Get container logs
    async fn get_logs(&self, id: &ContainerId, tail: usize) -> Result<Vec<String>, NodeError>;
}
```

### `ContainerSpec`

```rust
pub struct ContainerSpec {
    pub image: String,
    pub name: String,
    pub command: Option<Vec<String>>,
    pub args: Option<Vec<String>>,
    pub env: Vec<(String, String)>,
    pub gpu_ids: Vec<u32>,
    pub memory_limit_mb: Option<u64>,
    pub cpu_limit_millicores: Option<u32>,
    pub ports: Vec<PortMapping>,
    pub volumes: Vec<VolumeMount>,
}
```

---

## Gateway Communication

### `GatewayClient`

```rust
pub struct GatewayClient {
    // Internal WebSocket state
}

impl GatewayClient {
    /// Connect to gateway
    pub async fn connect(url: &str) -> Result<Self, NodeError>;
    
    /// Send a message
    pub async fn send(&self, msg: NodeMessage) -> Result<(), NodeError>;
    
    /// Receive next message
    pub async fn recv(&mut self) -> Result<GatewayMessage, NodeError>;
    
    /// Check connection status
    pub fn is_connected(&self) -> bool;
    
    /// Reconnect with backoff
    pub async fn reconnect(&mut self) -> Result<(), NodeError>;
}
```

### Message Handling

```rust
use clawnode::{handle_gateway_message, HandlerContext};

async fn process_messages(
    client: &mut GatewayClient,
    ctx: &HandlerContext,
) -> Result<(), NodeError> {
    loop {
        match client.recv().await? {
            msg @ GatewayMessage::DeployWorkload { .. } => {
                let response = handle_gateway_message(msg, ctx).await?;
                client.send(response).await?;
            }
            GatewayMessage::HeartbeatAck { .. } => {
                // Acknowledge received
            }
            GatewayMessage::Shutdown { reason, graceful } => {
                tracing::info!("Shutdown requested: {}", reason);
                break;
            }
            _ => {}
        }
    }
    Ok(())
}
```

---

## Node State

### `NodeState`

Internal state tracking for workloads and resources.

```rust
pub struct NodeState {
    /// Active workloads
    pub workloads: HashMap<WorkloadId, WorkloadInfo>,
    /// GPU allocations
    pub gpu_allocations: HashMap<u32, WorkloadId>,
    /// Connection state
    pub gateway_state: GatewayConnectionState,
}
```

### `WorkloadInfo`

```rust
pub struct WorkloadInfo {
    pub id: WorkloadId,
    pub name: String,
    pub spec: WorkloadSpec,
    pub state: WorkloadState,
    pub container_id: Option<ContainerId>,
    pub allocated_gpus: Vec<u32>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}
```

### `GatewayConnectionState`

```rust
pub enum GatewayConnectionState {
    Disconnected,
    Connecting,
    Connected {
        connected_at: DateTime<Utc>,
        heartbeat_interval: Duration,
        metrics_interval: Duration,
    },
    Reconnecting {
        attempt: u32,
        last_error: String,
    },
}
```

---

## Metrics Collection

### Built-in Metrics

The node automatically collects and reports:

| Metric | Type | Description |
|--------|------|-------------|
| `node_gpu_utilization` | Gauge | GPU utilization % |
| `node_gpu_memory_used` | Gauge | GPU memory used (MB) |
| `node_gpu_temperature` | Gauge | GPU temperature (°C) |
| `node_gpu_power` | Gauge | GPU power draw (W) |
| `node_workloads_active` | Gauge | Active workload count |
| `node_workloads_total` | Counter | Total workloads run |

### Custom Metrics

```rust
use clawnode::metrics::{MetricsCollector, Metric};

let collector = MetricsCollector::new();

// Add custom metric
collector.record(Metric::gauge(
    "custom_metric",
    42.0,
    vec![("label", "value")],
));
```

---

## Error Handling

### `NodeError`

```rust
pub enum NodeError {
    /// Configuration error
    Config(String),
    /// Gateway connection failed
    GatewayConnection(String),
    /// GPU detection failed
    GpuDetection(String),
    /// Container runtime error
    Runtime(String),
    /// Workload execution failed
    WorkloadExecution { workload_id: WorkloadId, error: String },
    /// Resource exhausted
    ResourceExhausted(String),
    /// Internal error
    Internal(String),
}
```

---

## CLI Reference

```bash
clawnode [OPTIONS]

Options:
  -g, --gateway <URL>       Gateway WebSocket URL
  -n, --name <NAME>         Node display name
  -c, --config <FILE>       Configuration file path
      --gpu-enabled         Enable GPU detection [default: true]
      --log-level <LEVEL>   Log level (trace, debug, info, warn, error)
      --log-format <FMT>    Log format (json, pretty)
  -h, --help                Print help
  -V, --version             Print version

Examples:
  clawnode --gateway ws://localhost:8080 --name my-node
  clawnode --config /etc/clawnode/config.toml
  clawnode --gateway wss://gateway.prod.example.com:8443 --name prod-gpu-01
```

---

## Signals

The node handles Unix signals gracefully:

| Signal | Behavior |
|--------|----------|
| `SIGTERM` | Graceful shutdown |
| `SIGINT` | Graceful shutdown (Ctrl+C) |
| `SIGHUP` | Reload configuration |

```bash
# Graceful shutdown
kill -TERM $(pidof clawnode)

# Reload config
kill -HUP $(pidof clawnode)
```
