# Clawbernetes OpenClaw Plugin

Fleet-level GPU cluster management for OpenClaw. Provides aggregate tools that fan out across all connected clawnodes, a background health monitor, and 20 bundled skills.

## What This Plugin Adds

Beyond skills + `node.invoke`:

1. **Fleet-level aggregate tools** — one tool call instead of N `node.invoke` calls
2. **Auto node selection** — `claw_deploy` scores nodes and picks the best one
3. **Background health service** — continuous fleet monitoring without agent polling
4. **Gateway RPC methods** — expose fleet status to Control UI dashboard

## Installation

```bash
# Install plugin
cd plugin/openclaw-clawbernetes && npm install

# Link into OpenClaw
openclaw plugins install -l ./plugin/openclaw-clawbernetes

# Enable + start
openclaw plugins enable clawbernetes
openclaw gateway --verbose
```

## Fleet Tools

| Tool | Description |
|------|-------------|
| `claw_fleet_status` | Aggregate cluster state — nodes, GPUs, memory, workload counts |
| `claw_gpu_inventory` | All GPUs across all nodes with model, VRAM, utilization, temperature |
| `claw_deploy` | Deploy a workload to the best available node (auto-placement) |
| `claw_workloads` | Cross-node workload list with filtering |
| `claw_multi_invoke` | Fan-out any of the 91 commands to multiple nodes in parallel |

## Bundled Skills (20)

### Core (always available)
- `gpu-cluster` — GPU discovery and metrics
- `workload-manager` — Container lifecycle
- `system-admin` — System info, node management
- `secrets-config` — Secrets + configuration
- `observability` — Metrics, events, alerts

### Feature-gated
- `auth-rbac` — Auth/RBAC/audit
- `job-scheduler` — Jobs + cron
- `storage` — Volumes + backups
- `networking` — Services, ingress, mesh
- `autoscaler` — GPU-aware autoscaling
- `molt-marketplace` — P2P GPU trading

### Meta
- `clawbernetes` — Fleet management overview

### Workflow (reference, not slash-invocable)
- `auto-heal` — Auto-recovery
- `canary-release` — Gradual rollouts
- `blue-green-deploy` — Zero-downtime deploys
- `spot-migration` — Cost optimization via MOLT
- `cost-optimize` — Cluster cost analysis
- `training-job` — ML job orchestration
- `incident-response` — SRE response
- `gpu-diagnose` — GPU troubleshooting

## Architecture

```
OpenClaw Agent (Morpheus)
      │
      ▼
[@clawbernetes/openclaw-plugin]   TypeScript, runs in gateway
      │ HTTP node.invoke
      ▼
[claw-gateway-server]             Routes to connected nodes
      │ WebSocket JSON-RPC
      ▼
[clawnode]  [clawnode]  ...       Rust nodes, 91 commands each
```

The plugin talks to clawnodes via `node.invoke` over the gateway. Zero coupling to Rust code.

## Configuration

In `openclaw.plugin.json`:

| Option | Default | Description |
|--------|---------|-------------|
| `gatewayUrl` | `http://127.0.0.1:18789` | Gateway URL for node.invoke calls |
| `healthIntervalMs` | `60000` | Fleet health check interval (ms) |
| `invokeTimeoutMs` | `30000` | Default timeout for node.invoke calls |

## Usage

```
You: "What's the fleet status?"
Agent: claw_fleet_status() → nodes, GPUs, memory, workloads

You: "List all GPUs"
Agent: claw_gpu_inventory() → flat list with utilization

You: "Deploy pytorch with 4 GPUs"
Agent: claw_deploy({image: "pytorch/pytorch:2.0", gpus: 4})

You: "List all workloads"
Agent: claw_workloads() → cross-node list

You: "Run gpu.metrics on all nodes"
Agent: claw_multi_invoke({command: "gpu.metrics"})
```

## Development

```bash
# Watch mode
cd plugin/openclaw-clawbernetes && npm run dev

# Type check
npm run typecheck

# Build
npm run build
```

## License

MIT
