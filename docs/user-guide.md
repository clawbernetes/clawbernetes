# Clawbernetes User Guide

This guide covers everything you need to know to get Clawbernetes running in production.

## Table of Contents

1. [Overview](#overview)
2. [Installation](#installation)
3. [Cluster Setup](#cluster-setup)
4. [Deploying Workloads](#deploying-workloads)
5. [Monitoring & Observability](#monitoring--observability)
6. [Troubleshooting](#troubleshooting)

---

## Overview

Clawbernetes is an AI-native GPU orchestration platform that replaces traditional Kubernetes for GPU workloads. Instead of YAML configuration, you use natural language intents; instead of dashboard fatigue, you get AI-powered diagnostics.

### Key Concepts

| Concept | Description |
|---------|-------------|
| **Gateway** | Control plane that coordinates the cluster. Runs on any machine with network access to nodes. |
| **Node (clawnode)** | Agent that runs on each GPU machine. Detects hardware, executes workloads, reports metrics. |
| **Workload** | A containerized job with GPU requirements. Can be short-lived or long-running. |
| **MOLT** | P2P compute marketplace. Optional - lets you sell/buy GPU time. |

### Architecture Overview

```
                    ┌─────────────────────────────────┐
                    │           Gateway               │
                    │  • Node registry                │
                    │  • Workload scheduler           │
                    │  • Metrics aggregation          │
                    │  • REST API / Dashboard         │
                    └───────────────┬─────────────────┘
                                    │ WebSocket (TLS)
                    ┌───────────────┼───────────────┐
                    │               │               │
              ┌─────▼─────┐   ┌─────▼─────┐   ┌─────▼─────┐
              │  clawnode │   │  clawnode │   │  clawnode │
              │  4× A100  │   │  8× H100  │   │  M3 Ultra │
              └───────────┘   └───────────┘   └───────────┘
```

---

## Installation

### Requirements

- **Rust 1.85+** (2024 Edition)
- **Docker** (optional, for containerized workloads)
- **GPU Drivers** (CUDA, Metal, ROCm, or Vulkan)

### From Source (Recommended)

```bash
# Clone the repository
git clone https://github.com/clawbernetes/clawbernetes
cd clawbernetes

# Build release binaries
make build

# Binaries are in target/release/
ls -la target/release/{claw-gateway,clawnode,clawbernetes}
```

### Using Cargo

```bash
# Install CLI globally
cargo install --path crates/claw-cli

# Install node agent
cargo install --path crates/clawnode

# Install gateway
cargo install --path crates/claw-gateway-server
```

### Docker

```bash
# Build Docker images
make docker

# Start a complete cluster with docker-compose
make docker-up

# View logs
make docker-logs

# Stop cluster
make docker-down
```

### Verifying Installation

```bash
# Check CLI version
clawbernetes --version

# Check gateway
claw-gateway --help

# Check node agent
clawnode --help
```

---

## Cluster Setup

### Step 1: Start the Gateway

The gateway is the control plane. Run it on a machine accessible to all nodes.

```bash
# Start with defaults (binds to 0.0.0.0:8080)
./target/release/claw-gateway

# Custom port
./target/release/claw-gateway 0.0.0.0:9000

# With TLS (recommended for production)
./target/release/claw-gateway \
  --tls-cert /etc/clawbernetes/gateway.crt \
  --tls-key /etc/clawbernetes/gateway.key
```

Expected output:
```
INFO Starting Clawbernetes Gateway on 0.0.0.0:8080
INFO   WebSocket endpoint: ws://0.0.0.0:8080/
INFO   Dashboard API: http://0.0.0.0:8080/api/
INFO   Nodes connect via: CLAWNODE_GATEWAY=ws://0.0.0.0:8080/
```

### Step 2: Connect Nodes

Run `clawnode` on each GPU machine:

```bash
# Using environment variable
export CLAWNODE_GATEWAY=ws://gateway-ip:8080
./target/release/clawnode

# Using command-line argument
./target/release/clawnode --gateway ws://gateway-ip:8080 --name gpu-node-1

# Using config file (recommended for production)
./target/release/clawnode --config /etc/clawbernetes/clawnode.toml
```

#### Node Configuration File

Create `/etc/clawbernetes/clawnode.toml`:

```toml
[node]
name = "gpu-node-1"
gateway = "ws://gateway.example.com:8080"
reconnect_interval_secs = 5
max_reconnect_attempts = 10

[gpu]
# Auto-detect all GPUs (default)
auto_detect = true
# Or specify explicitly:
# gpus = ["0", "1", "2", "3"]
memory_alert_threshold = 90

[metrics]
interval_secs = 10
detailed_gpu_metrics = true

[network]
provider = "wireguard"  # or "tailscale"

[network.wireguard]
listen_port = 51820

[logging]
level = "info"
format = "pretty"  # or "json" for production
```

### Step 3: Verify Cluster

```bash
# Check cluster status
clawbernetes status

# List connected nodes
clawbernetes node list

# Get detailed node info
clawbernetes node info gpu-node-1
```

Example output:
```
CLUSTER STATUS
═══════════════════════════════════════
Gateway:       ws://localhost:8080
Nodes:         3 healthy, 0 unhealthy
Total GPUs:    16 (12 available)
Workloads:     4 running, 2 pending

NODES
┌─────────────┬──────────┬─────────┬───────────────┐
│ Name        │ GPUs     │ Status  │ Utilization   │
├─────────────┼──────────┼─────────┼───────────────┤
│ gpu-node-1  │ 4× A100  │ healthy │ 75%           │
│ gpu-node-2  │ 8× H100  │ healthy │ 50%           │
│ gpu-node-3  │ 4× RTX   │ healthy │ 25%           │
└─────────────┴──────────┴─────────┴───────────────┘
```

### Networking Options

#### Option A: WireGuard (Self-Hosted)

For full control over your network:

```toml
# clawnode.toml
[network]
provider = "wireguard"

[network.wireguard]
listen_port = 51820
# Keys are auto-generated on first run
# Or specify existing keys:
# private_key_path = "/etc/clawbernetes/wg-private.key"
```

Nodes automatically form a mesh. See [docs/wireguard-integration.md](wireguard-integration.md).

#### Option B: Tailscale (Managed)

For zero-config networking:

```toml
# clawnode.toml
[network]
provider = "tailscale"

[network.tailscale]
auth_key_env = "TS_AUTHKEY"
hostname_prefix = "clawnode"
tags = ["tag:clawbernetes"]
```

Requires a Tailscale account. See [docs/tailscale-integration.md](tailscale-integration.md).

---

## Deploying Workloads

### Basic Container Run

```bash
# Run a container with GPU access
clawbernetes run --gpus 0,1 nvidia/cuda:12.0-runtime -- nvidia-smi

# Run in background (detached)
clawbernetes run -d --gpus 0,1,2,3 pytorch/pytorch:latest -- python train.py

# With environment variables
clawbernetes run \
  --gpus 0 \
  -e MODEL=llama-70b \
  -e BATCH_SIZE=32 \
  --memory 80000 \
  vllm/vllm:latest
```

### Intent-Based Deployment

Instead of specifying exact resources, describe what you need:

```bash
# Natural language intent (parsed by the agent)
clawbernetes deploy "Run Llama 70B inference with 4 H100s, prioritize latency"

# The agent handles:
# - GPU selection (NVLink topology awareness)
# - Container configuration
# - Network setup
# - Health monitoring
# - Auto-scaling
```

### Workload Specification

For complex deployments, use a TOML spec:

```toml
# workload.toml
[workload]
name = "llm-inference"
image = "vllm/vllm:latest"

[workload.resources]
gpus = 4
gpu_type = "H100"  # or "A100", "any"
memory_mib = 80000
prefer_nvlink = true

[workload.env]
MODEL = "meta-llama/Llama-2-70b-chat-hf"
TENSOR_PARALLEL_SIZE = "4"

[workload.health]
check_interval_secs = 30
startup_timeout_secs = 300

[workload.scaling]
min_replicas = 1
max_replicas = 4
target_gpu_utilization = 70
```

Deploy with:
```bash
clawbernetes deploy --spec workload.toml
```

### Managing Workloads

```bash
# List all workloads
clawbernetes workload list

# View workload logs
clawbernetes logs llm-inference

# Stop a workload
clawbernetes stop llm-inference

# Scale a workload
clawbernetes scale llm-inference --replicas 3
```

---

## Monitoring & Observability

### AI-Native Observability

Instead of dashboards, ask questions:

```bash
# Ask about cluster health
clawbernetes ask "What's wrong with the cluster?"

# Diagnose slow training
clawbernetes ask "Why is training slow?"
# Agent: "GPU 3 thermal throttling at 89°C. Recommending migration to node-07."

# Capacity planning
clawbernetes ask "Can I run 3 more Llama-70B instances?"
```

### Dashboard API

The gateway exposes a REST API for monitoring:

```bash
# Cluster status
curl http://gateway:8080/api/status

# List nodes
curl http://gateway:8080/api/nodes

# Node metrics
curl http://gateway:8080/api/nodes/gpu-node-1/metrics

# Workload list
curl http://gateway:8080/api/workloads

# Real-time updates (WebSocket)
wscat -c ws://gateway:8080/api/ws
```

### Metrics & Logging

```bash
# View current metrics
clawbernetes metrics

# Query historical metrics
clawbernetes metrics --query "gpu_utilization > 90" --since 1h

# Stream logs from a workload
clawbernetes logs -f my-workload

# Search logs semantically
clawbernetes logs --search "CUDA out of memory"
```

### Autoscaling

Configure autoscaling policies:

```bash
# View autoscaling status
clawbernetes autoscale status

# Set utilization-based scaling
clawbernetes autoscale set-policy gpu-pool-1 \
  -t utilization \
  --min-nodes 2 \
  --max-nodes 20 \
  --target-utilization 70

# Set queue-depth scaling
clawbernetes autoscale set-policy gpu-pool-1 \
  -t queue-depth \
  --target-jobs-per-node 5 \
  --scale-up-threshold 20

# Enable/disable autoscaling
clawbernetes autoscale enable
clawbernetes autoscale disable
```

---

## Troubleshooting

### Node Won't Connect

1. **Check gateway is running:**
   ```bash
   curl http://gateway:8080/api/status
   ```

2. **Verify network connectivity:**
   ```bash
   # From node machine
   nc -zv gateway-ip 8080
   ```

3. **Check node logs:**
   ```bash
   ./clawnode --gateway ws://gateway:8080 2>&1 | head -50
   ```

4. **Verify TLS certificates (if using TLS):**
   ```bash
   openssl s_client -connect gateway:8080
   ```

### GPU Not Detected

1. **Check GPU drivers:**
   ```bash
   # NVIDIA
   nvidia-smi
   
   # AMD
   rocm-smi
   
   # Apple Silicon (Metal always available)
   system_profiler SPDisplaysDataType
   ```

2. **Verify clawnode can detect GPUs:**
   ```bash
   ./clawnode --detect-gpus
   ```

3. **Check container runtime GPU access:**
   ```bash
   docker run --gpus all nvidia/cuda:12.0-base nvidia-smi
   ```

### Workload Stuck Pending

1. **Check resource availability:**
   ```bash
   clawbernetes node list
   ```

2. **Check workload requirements vs. available resources:**
   ```bash
   clawbernetes workload info my-workload
   ```

3. **View scheduling decisions:**
   ```bash
   clawbernetes logs --scheduler my-workload
   ```

### High GPU Utilization / Thermal Issues

1. **Check thermal status:**
   ```bash
   clawbernetes ask "Are any GPUs thermal throttling?"
   ```

2. **Drain affected node:**
   ```bash
   clawbernetes node drain gpu-node-1
   ```

3. **After cooling, undrain:**
   ```bash
   clawbernetes node undrain gpu-node-1
   ```

### Getting Help

```bash
# CLI help
clawbernetes --help
clawbernetes node --help
clawbernetes molt --help

# Documentation
open docs/
```

**Community:**
- Discord: https://discord.gg/clawbernetes
- GitHub Issues: https://github.com/clawbernetes/clawbernetes/issues
- Twitter: https://twitter.com/clawbernetes

---

## Next Steps

- [CLI Reference](cli-reference.md) — Complete command reference
- [Architecture](architecture.md) — System design deep-dive
- [MOLT Network](molt-network.md) — Join the P2P compute marketplace
- [Security Guide](security.md) — Auth, secrets, and network security
