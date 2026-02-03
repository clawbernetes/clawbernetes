# 🦀 Clawbernetes

**AI-Native GPU Orchestration Platform**

[![CI](https://github.com/clawbernetes/clawbernetes/actions/workflows/ci.yml/badge.svg)](https://github.com/clawbernetes/clawbernetes/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)

> **Kubernetes was built for web apps. Clawbernetes was built for AI.**

Clawbernetes replaces Kubernetes' declarative reconciliation model with intelligent agent-driven infrastructure management. Built on the [OpenClaw](https://github.com/openclaw/openclaw) agent runtime, it provides GPU-native scheduling, intent-based operations, and autonomous self-healing.

## ✨ Key Features

- **Intent over YAML** — Tell the agent what you want, not how to configure it
- **GPU-Native Scheduling** — Understands NVLink topology, VRAM, thermals, and PCIe lanes
- **Agent-Driven Operations** — Manage clusters from WhatsApp, Slack, Discord, or CLI
- **Self-Healing** — Root-cause analysis and autonomous remediation
- **MOLT Network** — Optional P2P compute marketplace with token incentives

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    OpenClaw Gateway (Control Plane)             │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌───────────┐  │
│  │Fleet Agent │  │Scheduler   │  │ Node       │  │ Workload  │  │
│  │ (Skills)   │  │ Agent      │  │ Registry   │  │ State     │  │
│  └────────────┘  └────────────┘  └────────────┘  └───────────┘  │
└────────────────────────────┬────────────────────────────────────┘
                             │ WebSocket + Protobuf
┌────────────────────────────▼────────────────────────────────────┐
│                         clawnode                                │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────────────────┐ │
│  │ GPU     │  │Container│  │ Metrics │  │ MOLT P2P            │ │
│  │ Manager │  │ Runtime │  │ Agent   │  │ (Optional)          │ │
│  └─────────┘  └─────────┘  └─────────┘  └─────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

## 📦 Crates

| Crate | Description | Status |
|-------|-------------|--------|
| `clawnode` | Node agent — GPU detection, container runtime, metrics | 🚧 In Progress |
| `claw-proto` | Protobuf definitions for node ↔ gateway communication | 🚧 In Progress |
| `claw-cli` | Command-line interface | 🚧 In Progress |
| `molt-core` | MOLT token types and primitives | 🚧 In Progress |
| `molt-p2p` | P2P discovery and gossip protocol | 🚧 In Progress |
| `molt-agent` | Autonomous provider/buyer agent logic | 🚧 In Progress |
| `molt-market` | Decentralized marketplace protocol | 🚧 In Progress |
| `molt-attestation` | Hardware and execution attestation | 🚧 In Progress |

## 🚀 Quick Start

```bash
# Clone the repository
git clone https://github.com/clawbernetes/clawbernetes
cd clawbernetes

# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Start a node agent (connects to Gateway)
cargo run -p clawnode -- --gateway ws://localhost:18789
```

## 🔧 Configuration

```toml
# clawbernetes.toml
[node]
name = "gpu-node-01"
gateway = "ws://localhost:18789"

[gpu]
auto_detect = true
allow_mig = true

[molt]
enabled = false  # Set true to join MOLT network
autonomy = "moderate"  # conservative | moderate | aggressive
```

## 🪙 MOLT Network (Optional)

Clawbernetes nodes can optionally participate in the MOLT P2P compute network:

- **Earn MOLT** for providing GPU compute to the network
- **Spend MOLT** to access distributed GPU capacity
- **Choose your autonomy level:**
  - **Conservative** — Approve every job manually
  - **Moderate** — Agent follows your policies
  - **Aggressive** — Full autopilot for maximum earnings

```bash
# Join the MOLT network
clawbernetes molt join --autonomy moderate
```

## 📄 License

MIT License — see [LICENSE-MIT](LICENSE-MIT) for details.

## 🤝 Contributing

Contributions welcome! Please read our [Contributing Guide](CONTRIBUTING.md) first.

---

Built with 🦀 by the Clawbernetes community
