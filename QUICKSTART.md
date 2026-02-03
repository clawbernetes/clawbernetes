# Clawbernetes Quick Start

## Build

```bash
cargo build --release
```

Binaries are in `target/release/`:
- `claw-gateway` — Control plane (1.5 MB)
- `clawnode` — Node agent (2.1 MB)
- `clawbernetes` — CLI (1.4 MB)

## 1. Start the Gateway

```bash
./target/release/claw-gateway
# Or with custom address:
./target/release/claw-gateway 0.0.0.0:9000
```

Output:
```
INFO Starting Clawbernetes Gateway on 0.0.0.0:8080
INFO   WebSocket endpoint: ws://0.0.0.0:8080/
INFO   Nodes connect via:  CLAWNODE_GATEWAY=ws://0.0.0.0:8080/
```

## 2. Connect Nodes

On each GPU machine:

```bash
# Option A: Environment variable
export CLAWNODE_GATEWAY=ws://gateway-ip:8080
./clawnode

# Option B: Command line
./clawnode --gateway ws://gateway-ip:8080 --name gpu-node-1

# Option C: Config file
./clawnode --config /etc/clawbernetes/clawnode.toml
```

See `examples/clawnode.toml` for configuration options.

## 3. Use the CLI

```bash
# Check cluster status
./clawbernetes status

# List nodes
./clawbernetes node list

# Get node details
./clawbernetes node info gpu-node-1

# Run a workload
./clawbernetes run --gpus 2 --image nvidia/cuda:12.0

# MOLT marketplace (if enabled)
./clawbernetes molt status
./clawbernetes molt join
./clawbernetes molt earnings
```

## Network Configuration

### Option A: WireGuard (Self-Hosted)

```toml
# clawnode.toml
[network]
provider = "wireguard"

[network.wireguard]
listen_port = 51820
```

Nodes form a mesh automatically. All traffic encrypted.

### Option B: Tailscale (Managed)

```toml
# clawnode.toml
[network]
provider = "tailscale"

[network.tailscale]
auth_key_env = "TS_AUTHKEY"
tags = ["tag:clawbernetes"]
```

Zero config networking via Tailscale.

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `CLAWNODE_GATEWAY` | Gateway WebSocket URL | (required) |
| `CLAWNODE_NAME` | Node identifier | hostname |
| `CLAWNODE_CONFIG` | Config file path | none |
| `TS_AUTHKEY` | Tailscale auth key | none |
| `RUST_LOG` | Log level | info |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         Gateway                              │
│  • Node registry        • Workload scheduling               │
│  • WebSocket server     • Metrics aggregation               │
│  • MOLT coordination    • Observability (AI diagnosis)      │
└─────────────────────────────────────────────────────────────┘
           ▲ WebSocket                    ▲ WebSocket
           │                              │
    ┌──────┴───────┐              ┌──────┴───────┐
    │   clawnode   │   WireGuard  │   clawnode   │
    │  (GPU host)  │◄────────────►│  (GPU host)  │
    │  • 4x A100   │    mesh      │  • 8x H100   │
    └──────────────┘              └──────────────┘
```

## What Clawbernetes Replaces

| Traditional | Clawbernetes |
|-------------|--------------|
| Kubernetes | Single binary orchestrator |
| Prometheus/Grafana | AI-native observability |
| Vault/cert-manager | Built-in secrets & PKI |
| ArgoCD/Helm | Intent-based deployment |
| Calico/Flannel | WireGuard mesh |
| Istio mTLS | Encryption by default |

## MOLT P2P Marketplace

Enable MOLT in node config to sell GPU compute:

```toml
[molt]
enabled = true
min_price = 1.0  # MOLT tokens per GPU-hour
max_jobs = 2
```

Buyers find providers automatically. Payments via MOLT token.
