<p align="center">
  <img src="docs/assets/logo.svg" alt="Clawbernetes" width="200"/>
</p>

<h1 align="center">Clawbernetes</h1>

<p align="center">
  <strong>AI-Native GPU Orchestration Platform</strong><br>
  Deploy and manage GPU workloads across any machine using natural language.
</p>

<p align="center">
  <a href="https://github.com/clawbernetes/clawbernetes/actions/workflows/ci.yml">
    <img src="https://github.com/clawbernetes/clawbernetes/actions/workflows/ci.yml/badge.svg" alt="CI">
  </a>
  <a href="https://opensource.org/licenses/MIT">
    <img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License: MIT">
  </a>
  <a href="https://www.rust-lang.org/">
    <img src="https://img.shields.io/badge/rust-1.85%2B-orange.svg" alt="Rust">
  </a>
  <img src="https://img.shields.io/badge/tests-4%2C600%2B-green.svg" alt="Tests">
  <img src="https://img.shields.io/badge/crates-37-informational.svg" alt="Crates">
</p>

<p align="center">
  <a href="#-quick-start">Quick Start</a> &bull;
  <a href="#-how-it-works">How It Works</a> &bull;
  <a href="#-use-cases">Use Cases</a> &bull;
  <a href="#%EF%B8%8F-architecture">Architecture</a> &bull;
  <a href="#-documentation">Documentation</a>
</p>

---

Kubernetes was built for web apps. **Clawbernetes was built for AI.**

Instead of writing YAML manifests and configuring Helm charts, you tell an AI assistant what you need. It handles GPU selection, container configuration, networking, monitoring, and scaling across your machines — local or remote.

```
You:   "Deploy a Llama 70B inference server with 4 H100s, prioritize latency"
Agent:  Found 3 nodes with available H100s. Deploying to node-07
        (8x H100 NVLink, 3 GPUs free, lowest latency score).
        Workload llama-70b-serve running. Allocated GPUs 4-7.
```

## Quick Start

Three steps. Five minutes. No YAML.

### 1. Install OpenClaw

[OpenClaw](https://github.com/openclaw/openclaw) is the AI runtime that Clawbernetes plugs into. It provides the agent, the web UI, and the chat interfaces (Slack, Discord, WhatsApp, Telegram, CLI).

```bash
npm install -g openclaw
```

### 2. Install Clawbernetes

```bash
# Clone the repo
git clone https://github.com/clawbernetes/clawbernetes
cd clawbernetes

# Build the Rust binaries (requires Rust 1.85+)
make build

# Install the OpenClaw plugin
cd plugin/openclaw-clawbernetes
npm install && npm run build
openclaw plugins install -l .
openclaw plugins enable clawbernetes

# Start the gateway (control plane)
cd ../..
./target/release/claw-gateway &

# Connect this machine as a node
./target/release/clawnode --gateway ws://localhost:8080 --name my-machine &
```

### 3. Open the Web UI and Start Deploying

```bash
openclaw gateway --verbose
```

Open the OpenClaw web interface. Your Clawbernetes cluster is live. Talk to the agent:

```
You:   "What do I have?"
Agent:  Fleet status: 1 node (my-machine), healthy.
        GPUs: 1x NVIDIA RTX 4090 (24GB VRAM, 2% utilization, 38C).
        No running workloads.

You:   "Run a pytorch training container with 1 GPU"
Agent:  Deployed pytorch/pytorch:2.4-cuda12.4 to my-machine.
        Workload ID: a3f8c2. 1 GPU allocated.

You:   "How's it doing?"
Agent:  Workload a3f8c2 running. GPU utilization 94%, temp 71C, memory 18.2/24GB.
```

That's it. No manifests, no kubectl, no dashboards to configure. The agent handles everything through the 91 commands available on each node.

---

## Adding More Machines

Every machine you want to use runs `clawnode`. It connects to your gateway over WebSocket and registers itself.

**Same network:**
```bash
./clawnode --gateway ws://gateway-ip:8080 --name gpu-server-2
```

**Remote machine (with WireGuard mesh):**
```bash
# On remote machine — clawnode handles the WireGuard tunnel
./clawnode --config /etc/clawnode/config.toml
```

```toml
# /etc/clawnode/config.toml
[node]
name = "cloud-gpu-01"
gateway = "wss://gateway.yourcompany.com:8080"

[network]
provider = "wireguard"

[network.wireguard]
listen_port = 51820
```

**Docker (quick test with simulated nodes):**
```bash
make docker-up    # gateway + 2 simulated nodes
make docker-logs  # watch them connect
make docker-down  # tear down
```

Once nodes are connected, the agent sees them all. "Deploy X with 4 GPUs" will automatically pick the best node based on available GPUs, memory, temperature, and current load.

---

## How It Works

```
You (chat, web UI, Slack, CLI)
 |
 v
OpenClaw Agent ---- reads 20 skills (how to use Clawbernetes)
 |                  calls 5 fleet tools (aggregate across nodes)
 v
Clawbernetes Plugin (TypeScript, runs in OpenClaw gateway)
 |
 | HTTP node.invoke
 v
claw-gateway (Rust, WebSocket server)
 |
 | WebSocket JSON-RPC
 v
clawnode        clawnode        clawnode
(91 commands)   (91 commands)   (91 commands)
4x A100         8x H100         M3 Ultra
```

**The plugin provides 5 fleet-level tools** so the agent can work across all nodes in a single call:

| Tool | What it does |
|------|-------------|
| `claw_fleet_status` | Aggregate health, GPU count, memory, workloads across all nodes |
| `claw_gpu_inventory` | Flat list of every GPU in the cluster with utilization and temperature |
| `claw_deploy` | Score all nodes and deploy to the best one automatically |
| `claw_workloads` | List all workloads across all nodes |
| `claw_multi_invoke` | Run any of the 91 commands on all nodes in parallel |

**20 bundled skills** teach the agent how to handle complex workflows — canary deployments, incident response, GPU troubleshooting, cost optimization, and more. The agent reads these as context, not as code.

---

## Use Cases

### ML Training

```
You:   "I need to fine-tune Llama 3 8B on my custom dataset. I have it in /data/training."
Agent:  Found 2 nodes with available GPUs. node-01 has 4x A100 (80GB each).
        Setting up distributed training with torchrun across 4 GPUs.
        Mounting /data/training as a volume.
        Deploying... workload fine-tune-llama running.

You:   "Set up a cron to checkpoint every 2 hours"
Agent:  Created cron job: saves checkpoint to /data/checkpoints every 2h.

You:   "GPU 2 seems slow"
Agent:  Checking GPU metrics on node-01...
        GPU 2: utilization 67% (vs 94% on GPUs 0,1,3), temp 91C — thermal throttling.
        Recommend: migrate workload to node-02 (8x H100, 42C ambient).
        Should I migrate?
```

### Inference Serving

```
You:   "Deploy a vLLM server for Mixtral 8x7B, make it accessible on port 8000"
Agent:  Mixtral 8x7B needs ~90GB VRAM. node-03 has 2x A100-80GB free.
        Deploying vllm/vllm-openai with tensor parallelism across 2 GPUs.
        Creating ingress route: https://mixtral.internal:8000
        Health check configured. Workload running.

You:   "Scale to handle more traffic"
Agent:  Current: 1 replica on node-03 (2 GPUs).
        node-01 has 4x A100 available. Deploying second replica.
        Load balancing across both endpoints.
```

### Multi-Node Batch Processing

```
You:   "Run this data processing job on every node in the cluster"
Agent:  Running on 5 nodes in parallel...
        node-01: completed (42s)
        node-02: completed (38s)
        node-03: completed (45s)
        node-04: completed (41s)
        node-05: failed — out of memory
        4/5 succeeded. node-05 has 16GB RAM, job needs 32GB. Skip or retry with lower batch size?
```

### GPU Cluster Monitoring

```
You:   "Give me a daily health report"
Agent:  Fleet: 8 nodes, 7 healthy, 1 degraded.
        GPUs: 32 total (24 A100, 8 H100), 18 allocated, 14 available.
        Alerts: node-06 GPU 3 memory errors (ECC count 47, was 12 yesterday).
        Workloads: 12 running, 3 pending (waiting for GPU).
        Recommendation: node-06 GPU 3 should be taken offline for diagnostics.

You:   "Do it"
Agent:  Draining node-06... migrating 2 workloads to node-04.
        Workloads migrated. node-06 cordoned. GPU 3 isolated.
```

### Cost Optimization with MOLT

The MOLT marketplace lets you buy and sell GPU compute peer-to-peer.

```
You:   "My training job only runs at night. Can I rent out GPUs during the day?"
Agent:  Your cluster has 24 GPUs. Daytime utilization averages 15%.
        Creating MOLT offer: 20 GPUs available 8am-6pm, $0.80/GPU-hour.
        Estimated daily revenue: ~$160 from marketplace demand.

You:   "Find me cheap H100s for a 4-hour batch job tonight"
Agent:  Found 3 MOLT offers for H100s:
        - provider-aws-7: 8x H100, $1.20/hr, 98% reputation
        - provider-lambda-3: 4x H100, $0.95/hr, 94% reputation
        - provider-coreweave-1: 8x H100, $1.50/hr, 99% reputation
        Recommend provider-aws-7. Total cost: $38.40 for 4 hours.
        Place bid?
```

### Canary Deployments

```
You:   "Update the inference server to the new model version, but do it safely"
Agent:  Using canary strategy. Deploying v2 alongside v1.
        Routing 10% of traffic to v2.
        Monitoring latency and error rates for 10 minutes...

        5min: v2 p99 latency 45ms (v1: 42ms), error rate 0.1% — looks good.
        10min: v2 stable. Promoting to 50%.
        20min: No regressions. Promoting to 100%. Removing v1.
        Canary complete. All traffic on v2.
```

### Incident Response

```
You:   "GPU utilization dropped to 0 on node-03, training is stuck"
Agent:  Investigating node-03...
        - Node healthy, network OK
        - GPU driver responding, but CUDA context hung on GPU 0
        - Workload fine-tune-7b stuck in CUDA_ERROR_LAUNCH_FAILED
        Root cause: OOM during backward pass (tried to allocate 78GB on 80GB card).
        Fix: restart workload with gradient checkpointing enabled (halves memory).
        Should I restart with --gradient-checkpointing?
```

---

## What Clawbernetes Replaces

| Traditional Stack | Clawbernetes |
|-------------------|-------------|
| Kubernetes + kubectl + YAML | Single binary + natural language |
| Prometheus + Grafana + Alertmanager | AI-native observability ("what's wrong?" gets an answer) |
| Vault + cert-manager | Built-in encrypted secrets + automatic rotation |
| ArgoCD + Helm + Kustomize | Intent-based deployment ("deploy X with Y GPUs") |
| Calico + Flannel + Istio | WireGuard mesh or Tailscale (automatic) |
| SLURM + PBS | GPU-aware scheduling with workload priorities |
| Cloud marketplace (AWS/GCP) | MOLT P2P compute marketplace |

---

## Architecture

### System Overview

```
                        +-----------------------+
                        |     OpenClaw Agent    |
                        |  (Morpheus / Web UI)  |
                        +----------+------------+
                                   |
                    reads skills   |   calls fleet tools
                                   |
                        +----------v------------+
                        |   Clawbernetes Plugin |
                        |     (TypeScript)      |
                        |  5 fleet tools        |
                        |  20 skills            |
                        |  health monitor       |
                        +----------+------------+
                                   |
                          HTTP node.invoke
                                   |
                        +----------v------------+
                        |    claw-gateway       |
                        |   (Rust, WebSocket)   |
                        |   port 8080           |
                        +--+--------+--------+--+
                           |        |        |
                     WS    |   WS   |   WS   |
                           |        |        |
                    +------v--+ +---v----+ +-v-------+
                    | clawnode | |clawnode| |clawnode |
                    | 4x A100 | |8x H100 | |M3 Ultra |
                    | Linux   | |Linux   | |macOS    |
                    +---------+ +--------+ +---------+
```

### Crate Map (37 crates)

**Core**
| Crate | Purpose |
|-------|---------|
| `claw-gateway-server` | WebSocket gateway, node registry, workload routing |
| `clawnode` | Node agent — 91 commands, GPU detection, container runtime |
| `claw-cli` | CLI (`clawbernetes` binary) |
| `claw-proto` | Protocol messages (NodeMessage, GatewayMessage, WorkloadSpec) |
| `claw-bridge` | JSON-RPC stdio bridge for plugin integration |
| `claw-compute` | Multi-platform GPU compute via CubeCL |

**Operations**
| Crate | Purpose |
|-------|---------|
| `claw-deploy` | Deployment strategies (canary, blue-green, rolling) |
| `claw-secrets` | ChaCha20-Poly1305 encrypted secrets with audit trail |
| `claw-metrics` | In-memory time-series database (24h retention) |
| `claw-auth` | API keys, RBAC, audit logging |
| `claw-autoscaler` | GPU-aware autoscaling policies |
| `claw-storage` | Volume management and backups |

**Networking**
| Crate | Purpose |
|-------|---------|
| `claw-network` | Mesh topology, service discovery, ingress |
| `claw-wireguard` | WireGuard tunnel management |
| `claw-discovery` | Network scanning, node bootstrap |
| `claw-pki` | Certificate authority |

**MOLT Marketplace**
| Crate | Purpose |
|-------|---------|
| `molt-core` | Token primitives and policies |
| `molt-p2p` | Peer discovery and gossip protocol |
| `molt-market` | Order book and settlement |
| `molt-agent` | Provider/buyer automation |
| `molt-token` | Solana SPL token integration |
| `molt-attestation` | Hardware verification (TEE/TPM) |

### Command Tiers (91 commands per node)

Each `clawnode` exposes commands organized into feature-gated tiers:

| Tier | Commands | Feature Flag | Examples |
|------|----------|-------------|----------|
| 0 - Core | 13 | always | `system.info`, `gpu.list`, `gpu.metrics`, `workload.run`, `workload.list` |
| Config | 5 | always | `config.create`, `config.get`, `config.update`, `config.delete`, `config.list` |
| 1 - Secrets | 5 | `secrets` | `secret.create`, `secret.get`, `secret.rotate` |
| 2 - Metrics | 8 | `metrics` | `metrics.query`, `events.emit`, `alerts.create` |
| 3 - Deploy | 8 | `deploy` | `deploy.create`, `deploy.rollback`, `deploy.promote` |
| 4 - Jobs | 9 | always | `job.create`, `cron.create`, `cron.trigger` |
| 5 - Network | 8 | `network` | `service.create`, `ingress.create`, `network.status` |
| 6 - Storage | 9 | `storage` | `volume.create`, `volume.snapshot`, `backup.create` |
| 7 - Auth | 7 | `auth` | `auth.create_key`, `rbac.create_role`, `audit.query` |
| 8 - Namespaces | 7 | always | `namespace.create`, `node.label`, `node.drain` |
| 9 - Autoscale | 4 | `autoscaler` | `autoscale.create`, `autoscale.status` |
| 10 - MOLT | 5 | `molt` | `molt.discover`, `molt.bid`, `molt.reputation` |
| 11 - Policy | 3 | always | `policy.create`, `policy.validate`, `policy.list` |

Build with the features you need:

```bash
cargo build -p clawnode --features full              # all 91 commands
cargo build -p clawnode --features docker,secrets     # core + docker + secrets
cargo build -p clawnode                               # core only (13 commands)
```

### GPU Support

Real GPU acceleration via [CubeCL](https://github.com/tracel-ai/cubecl):

| Platform | Backend | Status |
|----------|---------|--------|
| NVIDIA | CUDA | Production |
| Apple Silicon | Metal | Production |
| AMD | ROCm/HIP | Production |
| Cross-platform | Vulkan | Production |
| Fallback | CPU SIMD | Production |

---

## Configuration

### Gateway

The gateway accepts a single argument — the bind address:

```bash
./claw-gateway                    # binds to 0.0.0.0:8080
./claw-gateway 0.0.0.0:18789     # custom port
```

### Node

Nodes can be configured via CLI flags, environment variables, or a config file:

```toml
# /etc/clawnode/config.toml

[node]
name = "gpu-node-01"
gateway = "ws://gateway.example.com:8080"
reconnect_interval_secs = 5

[gpu]
memory_alert_threshold = 90

[metrics]
interval_secs = 10
detailed_gpu_metrics = true

[network]
provider = "wireguard"    # or "tailscale"

[network.wireguard]
listen_port = 51820

[molt]
enabled = false
# min_price = 1.0
# max_jobs = 2

[logging]
level = "info"
format = "pretty"
```

**Environment variables:**

| Variable | Description | Default |
|----------|-------------|---------|
| `CLAWNODE_GATEWAY` | Gateway WebSocket URL | required |
| `CLAWNODE_NAME` | Node identifier | system hostname |
| `CLAWNODE_CONFIG` | Config file path | `/etc/clawnode/config.json` |
| `RUST_LOG` | Log level | `info` |

### Plugin

The OpenClaw plugin is configured in your OpenClaw settings:

```json
{
  "plugins": {
    "clawbernetes": {
      "enabled": true,
      "gatewayUrl": "http://127.0.0.1:8080",
      "healthIntervalMs": 60000,
      "invokeTimeoutMs": 30000
    }
  },
  "tools": {
    "alsoAllow": ["clawbernetes"]
  }
}
```

See `plugin/examples/openclaw-config.json` for a full example with agent configurations and tool permissions.

---

## Docker Deployment

### Quick Start

```bash
make docker-up      # gateway + 2 simulated nodes
make docker-logs    # follow logs
make docker-down    # tear down
```

### GPU Nodes

For real GPU passthrough, uncomment the `gpu-node` service in `docker-compose.yml`. Requires [nvidia-container-toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html).

```bash
make docker-gpu     # build GPU node image
```

### Building Images

```bash
docker build --target gateway -t clawbernetes/gateway:latest .
docker build --target node -t clawbernetes/node:latest .
docker build -f Dockerfile.gpu -t clawbernetes/node-gpu:latest .
```

---

## Skills

The plugin bundles 20 skills that teach the agent how to use Clawbernetes. Skills are markdown documents — the agent reads them as context to understand what commands are available and how to combine them.

### Core Skills (always available)

| Skill | What the agent learns |
|-------|----------------------|
| `gpu-cluster` | Discover nodes, list GPUs, read metrics, check health |
| `workload-manager` | Run containers, monitor workloads, view logs, stop/restart |
| `system-admin` | System info, node management, cluster administration |
| `secrets-config` | Create/rotate encrypted secrets, manage config maps |
| `observability` | Query metrics, emit events, create alert rules |

### Feature Skills (available when node features are enabled)

| Skill | Feature | What the agent learns |
|-------|---------|----------------------|
| `auth-rbac` | `auth` | API keys, RBAC roles, audit logs |
| `job-scheduler` | always | One-shot jobs, cron schedules |
| `storage` | `storage` | Volumes, snapshots, backups |
| `networking` | `network` | Services, ingress, WireGuard mesh |
| `autoscaler` | `autoscaler` | Auto-scaling policies |
| `molt-marketplace` | `molt` | P2P GPU trading |

### Workflow Skills (reference guides for complex operations)

| Skill | When the agent uses it |
|-------|----------------------|
| `canary-release` | "Deploy safely" or "canary rollout" |
| `blue-green-deploy` | "Zero-downtime deployment" |
| `auto-heal` | Node failures, crash loops, resource exhaustion |
| `incident-response` | "Production is down", critical alerts |
| `gpu-diagnose` | "GPU is slow", memory errors, thermal issues |
| `training-job` | "Train a model", distributed training setup |
| `cost-optimize` | "Reduce costs", underutilization analysis |
| `spot-migration` | "Use cheaper GPUs", spot/on-demand migration |

---

## MOLT P2P Marketplace

Clawbernetes nodes can participate in the MOLT decentralized GPU marketplace:

```
Provider Node                         Buyer Agent
+-------------+                      +-------------+
| Idle GPUs   |<-- Offer ----------->| "Need 4     |
| H100 x 8   |                      |  H100s for  |
+-------------+                      |  training"  |
      |                              +-------------+
      | Execute                             |
      v                                     | MOLT Payment
+-------------+                             v
| Attestation |---- Proof -------->+-------------+
| (TEE/TPM)   |                    |   Escrow    |
+-------------+                    |  (Solana)   |
                                   +-------------+
```

**Autonomy modes:**

| Mode | Behavior |
|------|----------|
| Conservative | Approve every job manually |
| Moderate | Agent follows your pricing/duration policies |
| Aggressive | Full autopilot — maximize earnings |

```bash
clawbernetes molt join --autonomy moderate
clawbernetes molt policy set --min-price 0.50 --max-hours 24
clawbernetes molt earnings
```

---

## Development

### Prerequisites

- **Rust 1.85+** (2024 Edition)
- **Node.js 20+** (for the OpenClaw plugin)
- **Docker** (optional, for containerized testing)
- GPU drivers (optional, for hardware acceleration)

### Building

```bash
make build            # release build (all binaries)
make build-gateway    # gateway only
make build-node       # node only
make build-cli        # CLI only
```

### Testing

```bash
make test             # all tests (4,600+)
make test-fast        # unit tests only
make clippy           # lint
make check            # fmt + clippy + test
```

### Plugin Development

```bash
cd plugin/openclaw-clawbernetes
npm run dev           # watch mode (recompile on save)
npm run typecheck     # type check without emit
npm run build         # production build
```

### Project Stats

- 160,000+ lines of Rust
- 37 crates across 6 domains
- 4,600+ tests
- 0 `unsafe` in core libraries

### Code Standards

- No `unwrap()`/`expect()` in library code
- Tests required for all new functionality
- `cargo clippy -- -D warnings` must pass
- `cargo fmt` enforced

---

## Documentation

| Document | Description |
|----------|-------------|
| [QUICKSTART.md](QUICKSTART.md) | 5-minute getting started |
| [docs/user-guide.md](docs/user-guide.md) | Complete operator guide |
| [docs/cli-reference.md](docs/cli-reference.md) | CLI command reference |
| [docs/architecture.md](docs/architecture.md) | System design deep-dive |
| [docs/molt-network.md](docs/molt-network.md) | MOLT marketplace guide |
| [docs/security.md](docs/security.md) | Security and RBAC |
| [docs/api/README.md](docs/api/README.md) | API documentation |
| [docs/wireguard-integration.md](docs/wireguard-integration.md) | Self-hosted mesh networking |
| [docs/tailscale-integration.md](docs/tailscale-integration.md) | Managed networking |
| [docs/cubecl-integration.md](docs/cubecl-integration.md) | Multi-platform GPU compute |

---

## Contributing

We welcome contributions. Please see our [Contributing Guide](CONTRIBUTING.md) for details.

1. Fork the repository
2. Create a feature branch (`git checkout -b feat/my-feature`)
3. Write tests first
4. Implement your changes
5. Run `make check`
6. Submit a pull request

---

## License

Dual-licensed:

- **MIT License** — [LICENSE-MIT](LICENSE-MIT)
- **BSL 1.1** — [LICENSE-BSL](LICENSE-BSL) (converts to MIT after 4 years)

---

## Acknowledgments

- [OpenClaw](https://github.com/openclaw/openclaw) — Agent runtime
- [CubeCL](https://github.com/tracel-ai/cubecl) — Multi-platform GPU compute
- [WireGuard](https://wireguard.com) — Modern VPN protocol
- [Tailscale](https://tailscale.com) — Managed mesh networking

---

<p align="center">
  <a href="https://discord.gg/clawbernetes">Discord</a> &bull;
  <a href="https://twitter.com/clawbernetes">Twitter</a> &bull;
  <a href="https://clawbernetes.dev">Website</a>
</p>
