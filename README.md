<p align="center">
  <img src="docs/assets/logo.svg" alt="Clawbernetes" width="200"/>
</p>

<h1 align="center">Clawbernetes</h1>

<p align="center">
  <strong>AI-Native GPU Orchestration Platform</strong>
</p>

<p align="center">
  <a href="https://github.com/clawbernetes/clawbernetes/actions/workflows/ci.yml">
    <img src="https://github.com/clawbernetes/clawbernetes/actions/workflows/ci.yml/badge.svg" alt="CI">
  </a>
  <a href="https://opensource.org/licenses/MIT">
    <img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License: MIT">
  </a>
  <a href="https://www.rust-lang.org/">
    <img src="https://img.shields.io/badge/rust-1.85%2B%20(2024%20Edition)-orange.svg" alt="Rust">
  </a>
  <img src="https://img.shields.io/badge/tests-4%2C600%2B-green.svg" alt="Tests">
  <img src="https://img.shields.io/badge/lines-160K%2B-informational.svg" alt="Lines of Code">
</p>

<p align="center">
  <a href="#-quick-start">Quick Start</a> •
  <a href="#-features">Features</a> •
  <a href="#-architecture">Architecture</a> •
  <a href="#-documentation">Documentation</a> •
  <a href="#-molt-network">MOLT Network</a>
</p>

---

> **Kubernetes was built for web apps. Clawbernetes was built for AI.**

Clawbernetes replaces Kubernetes' declarative YAML-driven model with **intelligent agent-driven infrastructure**. Built on the [OpenClaw](https://github.com/openclaw/openclaw) runtime, it provides GPU-native scheduling, natural language operations, and autonomous self-healing.

## 🎯 Why Clawbernetes?

| Problem with Kubernetes | Clawbernetes Solution |
|------------------------|----------------------|
| YAML configuration hell | **Natural language intents** — "Scale training to 8 GPUs" |
| Alert fatigue from Prometheus/Grafana | **AI-native observability** — "What's wrong?" returns diagnosis |
| Complex Helm charts | **Agent-managed deployments** — describes desired state |
| Manual secret rotation | **Automatic rotation** with zero downtime |
| No GPU topology awareness | **NVLink/PCIe/VRAM-aware** scheduling |
| Vendor lock-in | **Multi-cloud + MOLT P2P** compute marketplace |

## ✨ Features

### 🖥️ Multi-Platform GPU Compute

Real GPU acceleration via [CubeCL](https://github.com/tracel-ai/cubecl):

| Platform | Backend | Status |
|----------|---------|--------|
| NVIDIA | CUDA | ✅ Ready |
| Apple Silicon | Metal | ✅ Tested |
| AMD | ROCm/HIP | ✅ Ready |
| Cross-platform | Vulkan | ✅ Ready |
| Fallback | CPU SIMD | ✅ Ready |

```rust
use claw_compute::gpu;

// Runs on Metal (macOS), CUDA (NVIDIA), or Vulkan (AMD/Intel)
let result = gpu::gpu_add(&vec_a, &vec_b)?;
let activated = gpu::gpu_gelu(&tensor)?;
```

### 🔐 Security & Secrets

- **Encrypted at rest** — AES-GCM with automatic key rotation
- **Workload identity** — Attestation-based access control
- **Built-in PKI** — Agent-managed certificate authority
- **Audit logging** — Full chain of custody with reasoning

### 🌐 Flexible Networking

Choose your networking model:

| Mode | Use Case | Complexity |
|------|----------|------------|
| **WireGuard** | Self-hosted mesh | Full control |
| **Tailscale** | Managed mesh | Zero config |
| **MOLT P2P** | Decentralized | Marketplace access |

### 📊 AI-Native Observability

Replaces: Prometheus, Grafana, Alertmanager, Loki, Jaeger

```
You: "Why is training slow?"
Agent: "GPU 3 thermal throttling at 89°C. Recommending migration to node-07."
```

- Embedded time-series database
- Semantic log search
- Automatic trace correlation
- Insight generation, not dashboards

### 🚀 Intent-Based Operations

Replaces: ArgoCD, Helm, Kustomize

```bash
# Instead of 500 lines of YAML:
clawbernetes deploy "Run Llama 70B inference with 4 H100s, prioritize latency"

# The agent handles:
# - GPU selection (NVLink topology)
# - Container configuration
# - Networking setup
# - Health monitoring
# - Auto-scaling
```

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Control Plane                                   │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │                    OpenClaw Gateway                               │  │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐  │  │
│  │  │   Fleet    │  │  Intent    │  │   Node     │  │  Workload  │  │  │
│  │  │   Agent    │  │  Parser    │  │  Registry  │  │   State    │  │  │
│  │  └────────────┘  └────────────┘  └────────────┘  └────────────┘  │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────┬────────────────────────────────────────┘
                                 │
                    WebSocket + Protobuf (TLS)
                                 │
        ┌────────────────────────┼────────────────────────┐
        │                        │                        │
        ▼                        ▼                        ▼
┌───────────────┐        ┌───────────────┐        ┌───────────────┐
│   clawnode    │        │   clawnode    │        │   clawnode    │
│  ┌─────────┐  │        │  ┌─────────┐  │        │  ┌─────────┐  │
│  │ 8x H100 │  │        │  │ 4x A100 │  │        │  │ M3 Ultra│  │
│  │ NVLink  │  │        │  │ PCIe    │  │        │  │ Metal   │  │
│  └─────────┘  │        └─────────┘  │        │  └─────────┘  │
│  ┌─────────┐  │        │  ┌─────────┐  │        │  ┌─────────┐  │
│  │Container│  │        │  │Container│  │        │  │Container│  │
│  │ Runtime │  │        │  │ Runtime │  │        │  │ Runtime │  │
│  └─────────┘  │        │  └─────────┘  │        │  └─────────┘  │
│  ┌─────────┐  │        │  ┌─────────┐  │        │  ┌─────────┐  │
│  │  MOLT   │  │        │  │  MOLT   │  │        │  │  MOLT   │  │
│  │  Agent  │  │        │  │  Agent  │  │        │  │  Agent  │  │
│  └─────────┘  │        │  └─────────┘  │        │  └─────────┘  │
└───────────────┘        └───────────────┘        └───────────────┘
```

## 🚀 Quick Start

### From Source

```bash
# Clone
git clone https://github.com/clawbernetes/clawbernetes
cd clawbernetes

# Build (requires Rust 1.85+)
make build

# Start the gateway
./target/release/claw-gateway

# Connect a node (new terminal)
./target/release/clawnode --gateway ws://localhost:8080 --name my-node

# Check cluster status
./target/release/clawbernetes node list
```

### Docker Compose

```bash
# Build images
make docker

# Start gateway + 2 simulated nodes
make docker-up

# View logs
make docker-logs

# Stop cluster
make docker-down
```

### Cargo

```bash
# Install CLI
cargo install --path crates/claw-cli

# Install node agent
cargo install --path crates/clawnode
```

## 📦 Crate Overview

Clawbernetes is organized into **35 crates** across six domains:

### Core Infrastructure

| Crate | Description | Tests |
|-------|-------------|-------|
| `claw-gateway-server` | WebSocket gateway for node fleet | ✅ |
| `clawnode` | Node agent with GPU detection | ✅ |
| `claw-cli` | Command-line interface | ✅ |
| `claw-proto` | Protobuf message definitions | ✅ |
| `claw-compute` | Multi-platform GPU compute (CubeCL) | ✅ |

### Operations & Security

| Crate | Description | Tests |
|-------|-------------|-------|
| `claw-metrics` | Embedded time-series database | ✅ |
| `claw-logs` | Structured log aggregation | ✅ |
| `claw-observe` | AI-native observability | ✅ |
| `claw-secrets` | Encrypted secrets management | ✅ |
| `claw-pki` | Certificate authority | ✅ |
| `claw-deploy` | Intent-based deployment | ✅ |
| `claw-rollback` | Automatic rollback with RCA | ✅ |

### Networking

| Crate | Description | Tests |
|-------|-------------|-------|
| `claw-network` | Mesh topology management | ✅ |
| `claw-wireguard` | WireGuard integration | ✅ |
| `claw-tailscale` | Tailscale managed mesh | ✅ |

### MOLT Marketplace

| Crate | Description | Tests |
|-------|-------------|-------|
| `molt-core` | Token primitives & policies | ✅ |
| `molt-token` | Solana SPL token client | ✅ |
| `molt-p2p` | Peer discovery & gossip | ✅ |
| `molt-agent` | Provider/buyer automation | ✅ |
| `molt-market` | Orderbook & settlement | ✅ |
| `molt-attestation` | Hardware verification | ✅ |

## 🪙 MOLT Network

Clawbernetes nodes can participate in the **MOLT P2P compute marketplace**:

```
┌─────────────────────────────────────────────────────────────────┐
│                      MOLT Network                               │
│                                                                 │
│   Provider Node                      Buyer Agent                │
│   ┌─────────────┐                   ┌─────────────┐            │
│   │ Idle GPUs   │◄── Offer ────────►│ "Need 4     │            │
│   │ H100 x 8    │                   │  H100s for  │            │
│   └─────────────┘                   │  training"  │            │
│         │                           └─────────────┘            │
│         │ Execute                          │                    │
│         ▼                                  │ MOLT Payment       │
│   ┌─────────────┐                          ▼                    │
│   │ Attestation │──── Proof ─────►┌─────────────┐              │
│   │ (TEE/TPM)   │                 │   Escrow    │              │
│   └─────────────┘                 │  (Solana)   │              │
│                                   └─────────────┘              │
└─────────────────────────────────────────────────────────────────┘
```

### Autonomy Modes

| Mode | Description | Use Case |
|------|-------------|----------|
| **Conservative** | Approve every job manually | High-value workloads |
| **Moderate** | Agent follows your policies | Balanced automation |
| **Aggressive** | Full autopilot | Maximum earnings |

```bash
# Join the marketplace
clawbernetes molt join --autonomy moderate

# Set pricing policy
clawbernetes molt policy set --min-price 0.50 --max-hours 24

# View earnings
clawbernetes molt earnings
```

## ⚙️ Configuration

```toml
# clawnode.toml
[node]
name = "gpu-node-01"
gateway = "ws://gateway.example.com:8080"

[gpu]
auto_detect = true
platforms = ["cuda", "metal"]  # or "rocm", "vulkan"

[network]
mode = "wireguard"  # or "tailscale"
mesh_cidr = "10.100.0.0/16"

[molt]
enabled = true
autonomy = "moderate"
wallet = "~/.config/clawbernetes/wallet.json"

[security]
tls_cert = "/etc/clawbernetes/node.crt"
tls_key = "/etc/clawbernetes/node.key"
```

## 📚 Documentation

| Document | Description |
|----------|-------------|
| [QUICKSTART.md](QUICKSTART.md) | 5-minute getting started guide |
| [docs/user-guide.md](docs/user-guide.md) | Complete operator guide |
| [docs/cli-reference.md](docs/cli-reference.md) | CLI command reference |
| [docs/architecture.md](docs/architecture.md) | System design deep-dive |
| [docs/molt-network.md](docs/molt-network.md) | MOLT P2P marketplace guide |
| [docs/security.md](docs/security.md) | Security & RBAC setup |
| [docs/api/README.md](docs/api/README.md) | API documentation index |
| [docs/ecosystem-replacement.md](docs/ecosystem-replacement.md) | How we replace K8s tooling |
| [docs/cubecl-integration.md](docs/cubecl-integration.md) | Multi-platform GPU support |
| [docs/wireguard-integration.md](docs/wireguard-integration.md) | Self-hosted mesh networking |
| [docs/tailscale-integration.md](docs/tailscale-integration.md) | Managed networking setup |

## 🧪 Testing

```bash
# Run all tests (2,100+)
cargo test --workspace

# Run with GPU features
cargo test --workspace --features cubecl-wgpu

# Run benchmarks
cargo bench --workspace

# Lint
cargo clippy --workspace -- -D warnings
```

## 🛠️ Development

### Requirements

- Rust 1.85+ (2024 Edition)
- Docker (for containerized testing)
- GPU drivers (optional, for hardware acceleration)

### Building

```bash
# Debug build
cargo build --workspace

# Release build
make build

# With all features
cargo build --workspace --all-features
```

### Project Stats

```
📊 160,000+ lines of Rust
📦 35 crates
🧪 4,600+ tests
🎯 0 unsafe in core (GPU crate allows for CubeCL)
```

## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Development Workflow

1. Fork the repository
2. Create a feature branch (`git checkout -b feat/amazing-feature`)
3. Write tests first (TDD)
4. Implement your changes
5. Run `cargo clippy -- -D warnings` and `cargo fmt`
6. Submit a pull request

### Code Standards

- **No `unwrap()`/`expect()`** in library code
- **No `todo!()`/`unimplemented!()`** in main branch
- **Tests required** for all new functionality
- **Documentation** for public APIs

## 📄 License

This project is dual-licensed:

- **MIT License** — see [LICENSE-MIT](LICENSE-MIT)
- **BSL 1.1** — see [LICENSE-BSL](LICENSE-BSL) (converts to MIT after 4 years)

## 🙏 Acknowledgments

- [CubeCL](https://github.com/tracel-ai/cubecl) — Multi-platform GPU compute
- [OpenClaw](https://github.com/openclaw/openclaw) — Agent runtime
- [Tailscale](https://tailscale.com) — Managed mesh networking
- [WireGuard](https://wireguard.com) — Modern VPN protocol

---

<p align="center">
  Built with 🦀 and ❤️ by the Clawbernetes community
</p>

<p align="center">
  <a href="https://discord.gg/clawbernetes">Discord</a> •
  <a href="https://twitter.com/clawbernetes">Twitter</a> •
  <a href="https://clawbernetes.dev">Website</a>
</p>
