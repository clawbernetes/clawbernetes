# Clawbernetes API Documentation

This document provides comprehensive API documentation for all Clawbernetes crates.

## Quick Links

| Category | Crates |
|----------|--------|
| [Core](#core-infrastructure) | `claw-proto`, `claw-gateway-server`, `clawnode`, `claw-cli` |
| [Compute](#compute) | `claw-compute` |
| [Operations](#operations--observability) | `claw-metrics`, `claw-logs`, `claw-observe`, `claw-deploy`, `claw-rollback` |
| [Security](#security) | `claw-secrets`, `claw-pki` |
| [Networking](#networking) | `claw-network`, `claw-wireguard`, `claw-tailscale` |
| [MOLT Marketplace](#molt-marketplace) | `molt-core`, `molt-token`, `molt-market`, `molt-agent`, `molt-p2p`, `molt-attestation` |

---

## Core Infrastructure

### claw-proto

Protocol definitions for gateway-node communication.

```rust
use claw_proto::{NodeMessage, GatewayMessage, NodeCapabilities};
```

**[Full Documentation →](./claw-proto.md)**

### claw-gateway-server

WebSocket gateway server for managing node fleet.

```rust
use claw_gateway_server::{GatewayServer, GatewayConfig};
```

**[Full Documentation →](./claw-gateway-server.md)**

### clawnode

Node agent for GPU detection, workload execution, and metrics.

```rust
use clawnode::{Node, NodeConfig, NodeError};
```

**[Full Documentation →](./clawnode.md)**

### claw-cli

Command-line interface for cluster management.

```bash
clawbernetes node list
clawbernetes deploy "Run Llama 70B on 4 GPUs"
clawbernetes molt earnings
```

**[Full Documentation →](./claw-cli.md)**

---

## Compute

### claw-compute

Multi-platform GPU compute via CubeCL.

```rust
use claw_compute::{gpu, kernels, ComputeDevice, CpuTensor};

// GPU operations (Metal, CUDA, Vulkan)
let result = gpu::gpu_add(&a, &b)?;
let activated = gpu::gpu_gelu(&tensor)?;

// CPU reference implementations
let output = kernels::matmul(&lhs, &rhs)?;
```

**[Full Documentation →](./claw-compute.md)**

---

## Operations & Observability

### claw-metrics

Embedded time-series database for metrics storage.

```rust
use claw_metrics::{MetricsStore, Metric, Query};
```

**[Full Documentation →](./claw-metrics.md)**

### claw-logs

Structured log aggregation with semantic search.

```rust
use claw_logs::{LogStore, LogEntry, LogQuery};
```

**[Full Documentation →](./claw-logs.md)**

### claw-observe

AI-native observability combining metrics, logs, and traces.

```rust
use claw_observe::{Observer, Insight, Diagnosis};
```

**[Full Documentation →](./claw-observe.md)**

### claw-deploy

Intent-based deployment engine.

```rust
use claw_deploy::{DeploymentIntent, DeploymentPlan, Deployer};
```

**[Full Documentation →](./claw-deploy.md)**

### claw-rollback

Automatic rollback with root-cause analysis.

```rust
use claw_rollback::{RollbackManager, RollbackReason, RollbackPlan};
```

**[Full Documentation →](./claw-rollback.md)**

---

## Security

### claw-secrets

Encrypted secrets management with workload identity.

```rust
use claw_secrets::{SecretStore, Secret, AccessPolicy};
```

**[Full Documentation →](./claw-secrets.md)**

### claw-pki

Certificate authority for node/workload identity.

```rust
use claw_pki::{CertificateAuthority, Certificate, CertRequest};
```

**[Full Documentation →](./claw-pki.md)**

---

## Networking

### claw-network

Mesh network topology management.

```rust
use claw_network::{MeshNetwork, Peer, NetworkConfig};
```

**[Full Documentation →](./claw-network.md)**

### claw-wireguard

WireGuard VPN integration for self-hosted mesh.

```rust
use claw_wireguard::{WireGuardConfig, Peer, Interface};
```

**[Full Documentation →](./claw-wireguard.md)**

### claw-tailscale

Tailscale managed mesh integration.

```rust
use claw_tailscale::{TailscaleClient, TailscaleNode, TailscaleAuth};
```

**[Full Documentation →](./claw-tailscale.md)**

---

## MOLT Marketplace

### molt-core

Core primitives for the MOLT token economy.

```rust
use molt_core::{Amount, Policy, Reputation, Wallet};
```

**[Full Documentation →](./molt-core.md)**

### molt-token

Solana SPL token client for MOLT.

```rust
use molt_token::{MoltClient, Transaction, Escrow, Network};
```

**[Full Documentation →](./molt-token.md)**

### molt-market

Decentralized orderbook and settlement.

```rust
use molt_market::{OrderBook, JobOrder, CapacityOffer, PaymentService};
```

**[Full Documentation →](./molt-market.md)**

### molt-agent

Autonomous provider/buyer agents.

```rust
use molt_agent::{ProviderAgent, BuyerAgent, AutonomyLevel, Strategy};
```

**[Full Documentation →](./molt-agent.md)**

### molt-p2p

Peer discovery and gossip protocol.

```rust
use molt_p2p::{P2pNetwork, Discovery, GossipProtocol};
```

**[Full Documentation →](./molt-p2p.md)**

### molt-attestation

Hardware and execution verification.

```rust
use molt_attestation::{HardwareAttestation, ExecutionProof, Verifier};
```

**[Full Documentation →](./molt-attestation.md)**

---

## Generating Rustdoc

Generate HTML documentation for all crates:

```bash
# Generate docs
cargo doc --workspace --no-deps --open

# With private items
cargo doc --workspace --no-deps --document-private-items
```

## Version Compatibility

| Crate | Min Rust | Edition |
|-------|----------|---------|
| All crates | 1.85+ | 2024 |

## License

All APIs are available under the MIT license.
