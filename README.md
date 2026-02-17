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
</p>

---

> **Kubernetes was built for web apps. Clawbernetes is AI-native infrastructure you talk to.**

Clawbernetes turns [OpenClaw](https://github.com/openclaw/openclaw) into an intelligent GPU infrastructure manager. Instead of YAML, dashboards, and `kubectl` — you have a conversation.

```
You:   "Deploy Llama 70B inference, optimize for latency"
Agent: "Found 8×H100 on gpu-node-01 with NVLink. Deploying container...
        Model loaded in 47s. Endpoint: https://llama.cluster.local
        Health monitoring active (5-min interval)."

You:   "Why is inference slow?"
Agent: "GPU 3 thermal throttling at 89°C on node-01.
        node-02 has 4×A100 at 62°C. Want me to migrate?"
```

## How It Works

Clawbernetes is **not** a Kubernetes replacement built from scratch. It's a thin, focused layer:

- **OpenClaw Gateway** = the control plane (already built)
- **OpenClaw Nodes** = headless agents on each GPU machine (already built)
- **Clawbernetes Skills** = teach the agent GPU ops, deployment, scaling
- **Clawbernetes Plugin** = custom tools for fleet inventory and workload state
- **`clawnode` binary** = GPU detection, metrics collection, node identity
- **MOLT Network** = P2P GPU compute marketplace (buy/sell idle compute)

```
┌──────────────────────────────────────────────────────────────┐
│                    OpenClaw Gateway                           │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │  Clawbernetes Agent                                    │  │
│  │  Skills: deploy, scale, diagnose, observe, molt        │  │
│  │  Memory: cluster state, incidents, decisions           │  │
│  └────────────────────────────────────────────────────────┘  │
│                          │                                   │
│             WebSocket (exec host=node)                       │
│                          │                                   │
│    ┌─────────┬───────────┼───────────┬─────────┐            │
│    ▼         ▼           ▼           ▼         ▼            │
│  Node 1   Node 2      Node 3     Node 4    Node N          │
│  8×H100   4×A100      M3 Ultra   CPU only  Spot GPU        │
└──────────────────────────────────────────────────────────────┘
```

## Quick Start

### 1. Install OpenClaw (the control plane)

```bash
npm install -g openclaw@latest
openclaw onboard --install-daemon
```

### 2. Build and install clawnode (on each GPU machine)

```bash
git clone https://github.com/clawbernetes/clawbernetes
cd clawbernetes
cargo install --path crates/clawnode
```

### 3. Connect nodes to the gateway

On each GPU machine:

```bash
clawnode --gateway ws://gateway-host:18789 --name "gpu-node-01"
```

Approve the node from the gateway:

```bash
openclaw nodes pending
openclaw nodes approve <requestId>
```

### 4. Install the Clawbernetes skills

```bash
cp -r skills/* ~/.openclaw/workspace/skills/
```

### 5. Talk to your infrastructure

```
You: "What GPUs do I have?"
You: "Deploy a vLLM server with Llama 3 70B"
You: "Scale inference to handle 2x traffic"
You: "What's the health of my cluster?"
```

## Crate Overview

12 focused crates (~73K lines of Rust):

### Core

| Crate | Description |
|-------|-------------|
| `clawnode` | GPU node agent — connects to OpenClaw gateway, reports capabilities |
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

## Skills

Skills teach the agent how to manage infrastructure. Each is a `SKILL.md` that the agent reads and follows.

| Skill | What It Does |
|-------|-------------|
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

The `plugin/openclaw-clawbernetes/` directory contains a TypeScript OpenClaw plugin that adds:

- Custom agent tools (fleet status, GPU inventory, workload management)
- Background health monitoring service
- Webhook handlers for external alerts

Install:

```bash
openclaw plugins install ./plugin/openclaw-clawbernetes
```

## MOLT Network

The P2P GPU compute marketplace. Providers offer idle GPUs, buyers pay in MOLT tokens (Solana SPL).

```
Provider: "I have 4×A100 idle for the next 6 hours"
Buyer:    "I need 4×A100 for distributed training"
MOLT:     Escrow → Attestation → Execute → Settle
```

## Configuration

```toml
# clawnode.toml
[node]
name = "gpu-node-01"
gateway = "ws://gateway.example.com:18789"

[gpu]
auto_detect = true

[molt]
enabled = false
autonomy = "moderate"
```

## Development

```bash
# Build all
cargo build --workspace

# Test
cargo test --workspace

# Release build
cargo build --workspace --release
```

## License

MIT — see [LICENSE-MIT](LICENSE-MIT)

---

<p align="center">
  Built with 🦀 by the Clawbernetes community
</p>
