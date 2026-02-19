<p align="center">
  <img src="docs/assets/logo.svg" alt="Clawbernetes" width="200"/>
</p>

<h1 align="center">Clawbernetes</h1>

<p align="center">
  <strong>Conversational GPU Infrastructure — Powered by OpenClaw</strong>
</p>

<p align="center">
  <a href="https://opensource.org/licenses/MIT">
    <img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License: MIT">
  </a>
  <a href="https://www.rust-lang.org/">
    <img src="https://img.shields.io/badge/rust-1.85%2B%20(2024%20Edition)-orange.svg" alt="Rust">
  </a>
  <a href="https://clawbernetes.com">
    <img src="https://img.shields.io/badge/web-clawbernetes.com-green.svg" alt="Website">
  </a>
</p>

---

> **Kubernetes was built for web apps. Clawbernetes is AI-native infrastructure you talk to.**

Clawbernetes turns [OpenClaw](https://github.com/openclaw/openclaw) into an intelligent GPU infrastructure manager. Instead of YAML, dashboards, and `kubectl` — you have a conversation.

```
You:   "What GPUs do we have?"
Agent: "1 node connected — morpheus (Ubuntu 24.04, 16 CPUs, 32GB RAM).
        GPU: NVIDIA RTX 3050 Ti, 4GB VRAM. No workloads running."

You:   "Deploy a vLLM server with Llama 3 8B, pick the best node."
Agent: "Deploying on morpheus (only node with GPU, 4GB VRAM available).
        Container started. Endpoint: http://morpheus:8000
        Health monitoring active."

You:   "Why is inference slow?"
Agent: "GPU 0 at 87°C — thermal throttling. Fan speed 100%.
        Ambient temp may be high. Want me to reduce batch size?"
```

## How It Works

Clawbernetes is **not** a Kubernetes replacement built from scratch. It's a thin, focused layer:

- **OpenClaw Gateway** = the control plane (already built)
- **OpenClaw Nodes** = headless agents on each GPU machine (already built)
- **`clawnode` binary** = GPU detection, metrics, container management, node identity
- **Clawbernetes Skills** = teach the agent GPU ops, deployment, scaling, diagnostics
- **Clawbernetes Plugin** = fleet-level tools for multi-node inventory and orchestration
- **MOLT Network** = P2P GPU compute marketplace (buy/sell idle compute)

```
┌──────────────────────────────────────────────────────────────┐
│                    OpenClaw Gateway                           │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │  Clawbernetes Agent                                    │  │
│  │  Skills: deploy, scale, diagnose, observe, heal, molt  │  │
│  │  Plugin: fleet status, GPU inventory, smart deploy     │  │
│  │  Memory: cluster state, incidents, decisions           │  │
│  └────────────────────────────────────────────────────────┘  │
│                          │                                   │
│             WebSocket (node.invoke)                           │
│                          │                                   │
│    ┌─────────┬───────────┼───────────┬─────────┐            │
│    ▼         ▼           ▼           ▼         ▼            │
│  Node 1   Node 2      Node 3     Node 4    Node N          │
│  8×H100   4×A100      M3 Ultra   RTX 3050  Spot GPU        │
└──────────────────────────────────────────────────────────────┘
```

## Quick Start

### 1. Install OpenClaw (the control plane)

```bash
npm install -g openclaw@latest
openclaw onboard --install-daemon
```

### 2. Build clawnode (on each GPU machine)

```bash
git clone https://github.com/clawbernetes/clawbernetes
cd clawbernetes
cargo install --path crates/clawnode
```

### 3. Connect a node to the gateway

Generate a config, then run:

```bash
# Generate config
clawnode init-config \
  --gateway ws://gateway-host:18789 \
  --output ./clawnode-config.json

# Edit config — set token, hostname, etc.
vim clawnode-config.json

# Run the node agent
clawnode run --config ./clawnode-config.json
```

Approve the node from the gateway:

```bash
openclaw nodes pending
openclaw nodes approve <requestId>
```

The node will complete the challenge-response handshake and show as `paired · connected`.

### 4. Install skills and plugin

```bash
# Skills (on the gateway machine)
cp -r skills/* ~/.openclaw/workspace/skills/

# Plugin (optional — adds fleet-level tools)
openclaw plugins install --link ./plugin/openclaw-clawbernetes
openclaw gateway restart
```

### 5. Talk to your infrastructure

```
"What GPUs do we have?"
"How's the cluster looking?"
"What's running on morpheus?"
"Deploy nginx on the node with the most free RAM."
"GPU temps across the cluster."
"Show me kernel versions on all nodes."
```

No command syntax needed. The agent translates your intent into the right API calls.

## Crate Overview

12 crates, ~70K lines of Rust, 1,770 tests passing:

### Core

| Crate | Description |
|-------|-------------|
| `clawnode` | GPU node agent — connects to OpenClaw gateway, reports capabilities, handles commands |
| `claw-cli` | Command-line interface |
| `claw-compute` | Multi-platform GPU detection and compute (CUDA, Metal, ROCm, Vulkan via CubeCL) |
| `claw-metrics` | Embedded time-series database for node metrics |
| `claw-proto` | Protobuf message definitions |
| `claw-wireguard` | WireGuard key types for P2P identity |

### MOLT Marketplace

| Crate | Description |
|-------|-------------|
| `molt-core` | Token primitives and marketplace policies |
| `molt-token` | Solana SPL token client |
| `molt-p2p` | Peer discovery, gossip protocol, tunnel management |
| `molt-agent` | Provider/buyer automation |
| `molt-market` | Order book and settlement |
| `molt-attestation` | Hardware verification (TEE/TPM) |

## Node Commands

`clawnode` exposes these commands via the OpenClaw WebSocket protocol:

| Command | Description |
|---------|-------------|
| `system.info` | OS, CPU, memory, hostname, kernel version |
| `system.run` | Execute shell commands on the node |
| `system.which` | Resolve binary paths |
| `gpu.list` | List all GPUs with model, VRAM, UUID, PCI bus |
| `gpu.metrics` | Real-time utilization, temperature, memory, power |
| `workload.run` | Start a container (Docker/Podman) |
| `workload.stop` | Stop a running container |
| `workload.list` | List running containers |
| `workload.logs` | Get container logs |
| `workload.inspect` | Detailed container info |
| `workload.stats` | Container resource usage |
| `container.exec` | Execute command inside a running container |
| `node.health` | Node health check |
| `node.capabilities` | List node capabilities |
| `config.*` | CRUD for node configuration |
| `job.*` | Create, status, logs, delete jobs |
| `cron.*` | Create, list, trigger, suspend, resume cron jobs |
| `namespace.*` | Create, quota, usage, list namespaces |
| `policy.*` | Create, validate, list policies |

## Skills

14 skills teach the agent how to manage infrastructure. Each is a `SKILL.md` the agent reads and follows.

| Skill | What It Does |
|-------|-------------|
| `clawbernetes` | Master skill — natural language query mapping, architecture overview |
| `gpu-cluster` | Fleet inventory, GPU health, topology |
| `gpu-diagnose` | Thermal, utilization, memory analysis |
| `workload-manager` | Deploy, stop, inspect containers |
| `autoscaler` | Scale workloads based on demand |
| `observability` | Aggregate logs and metrics across nodes |
| `auto-heal` | Detect failures and auto-remediate |
| `training-job` | Distributed training job management |
| `cost-optimize` | Spot instance management, right-sizing |
| `incident-response` | Automated incident diagnosis and response |
| `molt-marketplace` | Buy/sell GPU compute on the MOLT network |
| `system-admin` | Node management, labels, taints, drains |
| `job-scheduler` | Job and cron scheduling across nodes |
| `spot-migration` | Handle spot/preemptible instance evictions |

## OpenClaw Plugin

The `plugin/openclaw-clawbernetes/` directory contains a TypeScript OpenClaw plugin that adds fleet-level capabilities on top of the per-node commands:

**Tools:**
| Tool | Description |
|------|-------------|
| `claw_fleet_status` | Aggregate cluster health, GPU count, memory, workloads |
| `claw_gpu_inventory` | All GPUs across all nodes with specs |
| `claw_deploy` | Auto-place workload on the best node based on available resources |
| `claw_workloads` | Cross-node workload list |
| `claw_multi_invoke` | Fan-out any command to multiple nodes in parallel |

**Services:**
- `clawbernetes-health-monitor` — background fleet health monitoring with state transition tracking

**Gateway RPC:**
- `clawbernetes.fleet-status` — cached fleet status for the Control UI dashboard

Install:

```bash
openclaw plugins install --link ./plugin/openclaw-clawbernetes
```

## Configuration

```json
{
  "gateway": "ws://gateway.example.com:18789",
  "token": "your-gateway-auth-token",
  "hostname": "gpu-node-01",
  "labels": {},
  "state_path": "/var/lib/clawnode/state",
  "heartbeat_interval_secs": 30,
  "reconnect_delay_secs": 5,
  "container_runtime": "docker",
  "network_enabled": false,
  "region": "us-west",
  "wireguard_listen_port": 51820,
  "ingress_listen_port": 8443
}
```

Generate a starter config:

```bash
clawnode init-config --gateway ws://your-gateway:18789 --output config.json
```

## Gateway Setup

To allow clawnode commands through the OpenClaw gateway, add them to the node command allowlist in `openclaw.json`:

```json
{
  "gateway": {
    "nodes": {
      "allowCommands": [
        "system.info", "gpu.list", "gpu.metrics",
        "workload.run", "workload.stop", "workload.list",
        "workload.logs", "workload.inspect", "workload.stats",
        "container.exec", "node.health", "node.capabilities"
      ]
    }
  }
}
```

By default, only `system.run` and `system.which` are allowed for Linux nodes.

## MOLT Network

The P2P GPU compute marketplace. Providers offer idle GPUs, buyers pay in MOLT tokens (Solana SPL).

```
Provider: "I have 4×A100 idle for the next 6 hours"
Buyer:    "I need 4×A100 for distributed training"
MOLT:     Escrow → Attestation → Execute → Settle
```

## Development

```bash
# Build all crates
cargo build --workspace

# Run all tests (1,770 tests)
cargo test --workspace

# Release build
cargo build --workspace --release

# Run clawnode locally
cargo run -p clawnode -- run --config ./config.json
```

## License

MIT — see [LICENSE-MIT](LICENSE-MIT)

---

<p align="center">
  Built with 🦀 by the Clawbernetes community
</p>
