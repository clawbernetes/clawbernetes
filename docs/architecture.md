# Clawbernetes Architecture

Deep dive into system design, components, and communication protocols.

## Table of Contents

1. [High-Level Overview](#high-level-overview)
2. [Component Overview](#component-overview)
3. [Communication Protocols](#communication-protocols)
4. [Data Flow](#data-flow)
5. [Crate Dependency Graph](#crate-dependency-graph)

---

## High-Level Overview

Clawbernetes is a GPU orchestration platform built on three principles:

1. **Agent-Driven** — AI agents make operational decisions, not YAML configs
2. **GPU-Native** — First-class support for CUDA, Metal, ROCm, and Vulkan
3. **Decentralized Option** — MOLT marketplace enables P2P compute trading

### System Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              CONTROL PLANE                                  │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                        claw-gateway-server                             │ │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐   │ │
│  │  │   Fleet     │  │  Workload   │  │   Intent    │  │  Dashboard  │   │ │
│  │  │   Manager   │  │  Scheduler  │  │   Parser    │  │     API     │   │ │
│  │  └──────┬──────┘  └──────┬──────┘  └─────────────┘  └──────┬──────┘   │ │
│  │         │                │                                  │          │ │
│  │  ┌──────┴────────────────┴──────────────────────────────────┴──────┐  │ │
│  │  │                        Gateway Core                              │  │ │
│  │  │  • Node Registry    • Metrics Aggregation    • Event Bus        │  │ │
│  │  └─────────────────────────────────────────────────────────────────┘  │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────┬─────────────────────────────────────────┘
                                    │
                      WebSocket + Protobuf (TLS)
                                    │
        ┌───────────────────────────┼───────────────────────────┐
        │                           │                           │
        ▼                           ▼                           ▼
┌───────────────────┐       ┌───────────────────┐       ┌───────────────────┐
│     clawnode      │       │     clawnode      │       │     clawnode      │
│  ┌─────────────┐  │       │  ┌─────────────┐  │       │  ┌─────────────┐  │
│  │ GPU Driver  │  │       │  │ GPU Driver  │  │       │  │ GPU Driver  │  │
│  │ CUDA/Metal  │  │       │  │   ROCm      │  │       │  │   Vulkan    │  │
│  └─────────────┘  │       │  └─────────────┘  │       │  └─────────────┘  │
│  ┌─────────────┐  │       │  ┌─────────────┐  │       │  ┌─────────────┐  │
│  │  Container  │  │       │  │  Container  │  │       │  │  Container  │  │
│  │   Runtime   │  │  ◄────┼──│   Runtime   │──┼────►  │  │   Runtime   │  │
│  └─────────────┘  │  Mesh │  └─────────────┘  │  Mesh │  └─────────────┘  │
│  ┌─────────────┐  │ (WG/  │  ┌─────────────┐  │ (WG/  │  ┌─────────────┐  │
│  │ MOLT Agent  │  │  TS)  │  │ MOLT Agent  │  │  TS)  │  │ MOLT Agent  │  │
│  └─────────────┘  │       │  └─────────────┘  │       │  └─────────────┘  │
└───────────────────┘       └───────────────────┘       └───────────────────┘
```

---

## Component Overview

Clawbernetes consists of **35 crates** organized into six domains:

### Core Infrastructure (6 crates)

| Crate | Purpose | Key Types |
|-------|---------|-----------|
| `claw-gateway-server` | WebSocket server for node fleet management | `GatewayServer`, `GatewayConfig` |
| `claw-gateway` | Shared gateway types and state management | `NodeRegistry`, `WorkloadManager` |
| `clawnode` | Node agent with GPU detection and workload execution | `Node`, `NodeConfig`, `GpuDetector` |
| `claw-cli` | Command-line interface | `Cli`, `Commands`, `GatewayClient` |
| `claw-proto` | Protocol definitions (Protobuf messages) | `NodeMessage`, `GatewayMessage`, `Workload` |
| `claw-compute` | Multi-platform GPU compute via CubeCL | `ComputeDevice`, `Tensor`, GPU kernels |

### Operations & Observability (7 crates)

| Crate | Purpose | Key Types |
|-------|---------|-----------|
| `claw-metrics` | Embedded time-series database | `MetricsStore`, `Metric`, `Query` |
| `claw-logs` | Structured log aggregation | `LogStore`, `LogEntry`, `LogQuery` |
| `claw-observe` | AI-native observability and diagnostics | `Observer`, `Analyzer`, `Insight` |
| `claw-deploy` | Intent-based deployment engine | `DeploymentIntent`, `Deployer` |
| `claw-rollback` | Automatic rollback with root-cause analysis | `RollbackManager`, `RollbackPlan` |
| `claw-alerts` | Alert management and notification | `AlertManager`, `AlertRule`, `Notification` |
| `claw-autoscaler` | Cluster autoscaling policies | `Autoscaler`, `ScalingPolicy` |

### Security (6 crates)

| Crate | Purpose | Key Types |
|-------|---------|-----------|
| `claw-auth` | Authentication and RBAC | `ApiKey`, `JwtManager`, `RbacPolicy`, `AuthContext` |
| `claw-secrets` | Encrypted secrets management | `SecretStore`, `SecretKey`, `AccessPolicy` |
| `claw-pki` | Certificate authority | `CertificateAuthority`, `Certificate`, `CertRequest` |
| `claw-audit` | Security audit logging | `AuditLog`, `AuditEntry`, `AuditFilter` |
| `claw-ddos` | DDoS protection | `DdosProtection`, `RateLimiter`, `IpBlocklist` |
| `claw-validation` | Input validation utilities | `Validator`, `ValidationError` |

### Networking (5 crates)

| Crate | Purpose | Key Types |
|-------|---------|-----------|
| `claw-network` | Mesh network topology management | `MeshNetwork`, `Peer`, `NetworkConfig` |
| `claw-wireguard` | WireGuard VPN integration | `WireGuardConfig`, `Peer`, `Interface` |
| `claw-tailscale` | Tailscale managed mesh | `TailscaleClient`, `TailscaleNode` |
| `claw-discovery` | Service discovery | `DiscoveryService`, `ServiceRecord` |
| `claw-tenancy` | Multi-tenancy isolation | `Tenant`, `TenantConfig`, `IsolationPolicy` |

### MOLT Marketplace (6 crates)

| Crate | Purpose | Key Types |
|-------|---------|-----------|
| `molt-core` | Token primitives and policies | `Amount`, `Policy`, `Reputation`, `Wallet` |
| `molt-token` | Solana SPL token client | `MoltClient`, `Transaction`, `Escrow` |
| `molt-market` | Decentralized orderbook | `OrderBook`, `JobOrder`, `CapacityOffer` |
| `molt-agent` | Autonomous provider/buyer agents | `ProviderAgent`, `BuyerAgent`, `AutonomyMode` |
| `molt-p2p` | Peer discovery and gossip | `P2pNetwork`, `Discovery`, `GossipProtocol` |
| `molt-attestation` | Hardware verification | `HardwareAttestation`, `ExecutionProof` |

### Other (5 crates)

| Crate | Purpose |
|-------|---------|
| `claw-dashboard` | Web dashboard REST API |
| `claw-storage` | Persistent storage abstraction |
| `claw-preemption` | Workload preemption policies |
| `claw-e2e-tests` | End-to-end integration tests |
| `molt-integration-tests` | MOLT-specific integration tests |

---

## Communication Protocols

### Node ↔ Gateway Protocol

Nodes communicate with the gateway over WebSocket using Protocol Buffers.

```
┌──────────────┐                           ┌─────────────────┐
│   clawnode   │                           │     Gateway     │
└──────┬───────┘                           └────────┬────────┘
       │                                            │
       │─────── Register(capabilities) ────────────►│
       │                                            │
       │◄────── Registered(config) ─────────────────│
       │                                            │
       │◄────── StartWorkload(spec) ────────────────│
       │                                            │
       │─────── WorkloadStatus(running) ───────────►│
       │                                            │
       │─────── Metrics(gpu_util, mem) ────────────►│
       │             (every 10s)                    │
       │                                            │
       │─────── Logs(stdout, stderr) ──────────────►│
       │                                            │
       │─────── WorkloadStatus(completed) ─────────►│
       │                                            │
       │◄────── Heartbeat ──────────────────────────│
       │─────── HeartbeatAck ──────────────────────►│
       │                                            │
```

#### Message Types

**Node → Gateway (`NodeMessage`):**

```rust
pub enum NodeMessage {
    Register {
        capabilities: NodeCapabilities,
        name: Option<String>,
    },
    WorkloadStatus {
        workload_id: WorkloadId,
        status: WorkloadStatus,
    },
    Metrics {
        timestamp: u64,
        gpus: Vec<GpuMetrics>,
        system: SystemMetrics,
    },
    Logs {
        workload_id: WorkloadId,
        lines: Vec<LogLine>,
    },
    HeartbeatAck,
    Error {
        code: ErrorCode,
        message: String,
    },
}
```

**Gateway → Node (`GatewayMessage`):**

```rust
pub enum GatewayMessage {
    Registered {
        node_id: NodeId,
        config: NodeConfig,
    },
    StartWorkload {
        workload: WorkloadSpec,
    },
    StopWorkload {
        workload_id: WorkloadId,
        graceful: bool,
    },
    UpdateConfig {
        config: NodeConfig,
    },
    Heartbeat,
    MeshPeers {
        peers: Vec<MeshPeerConfig>,
    },
}
```

### CLI ↔ Gateway Protocol

The CLI uses a separate protocol for administrative operations:

```rust
pub enum CliMessage {
    Status,
    NodeList,
    NodeInfo { id: String },
    NodeDrain { id: String, force: bool },
    RunWorkload { spec: WorkloadSpec },
    MoltStatus,
    MoltJoin { autonomy: AutonomyLevel },
    // ... etc
}

pub enum CliResponse {
    Status { nodes: u32, gpus: u32, workloads: u32 },
    NodeList { nodes: Vec<NodeInfo> },
    NodeInfo { node: NodeInfo },
    Success { message: String },
    Error { code: u32, message: String },
}
```

### Dashboard REST API

```
GET  /api/status              → ClusterStatus
GET  /api/nodes               → Vec<NodeStatus>
GET  /api/nodes/:id           → NodeStatus
GET  /api/nodes/:id/metrics   → MetricsSnapshot
GET  /api/workloads           → Vec<WorkloadStatus>
GET  /api/workloads/:id       → WorkloadStatus
GET  /api/workloads/:id/logs  → SSE stream of LogEntry
WS   /api/ws                  → Real-time LiveUpdate stream
```

### MOLT P2P Protocol

Nodes participating in MOLT communicate via libp2p gossip:

```
┌──────────────┐         Gossip          ┌──────────────┐
│   Provider   │◄───────────────────────►│    Buyer     │
└──────┬───────┘                         └──────┬───────┘
       │                                        │
       │◄─────── CapacityOffer ─────────────────│
       │                                        │
       │─────── JobRequest ────────────────────►│
       │                                        │
       │◄─────── Bid ───────────────────────────│
       │                                        │
       │─────── Accept(bid_id) ────────────────►│
       │                                        │
       │         [Escrow funded on Solana]      │
       │                                        │
       │─────── JobStarted ────────────────────►│
       │                                        │
       │─────── ExecutionProof ────────────────►│
       │                                        │
       │         [Payment released]             │
```

---

## Data Flow

### Workload Lifecycle

```
                        ┌─────────────────────────────────────────┐
                        │              User Request               │
                        │   "Run Llama 70B on 4 H100s"           │
                        └─────────────────┬───────────────────────┘
                                          │
                                          ▼
                        ┌─────────────────────────────────────────┐
                        │            Intent Parser                 │
                        │   • Parse natural language              │
                        │   • Extract requirements                │
                        │   • Generate WorkloadSpec               │
                        └─────────────────┬───────────────────────┘
                                          │
                                          ▼
                        ┌─────────────────────────────────────────┐
                        │             Scheduler                    │
                        │   • Query node registry                 │
                        │   • Match GPU requirements              │
                        │   • Consider topology (NVLink)          │
                        │   • Select best node(s)                 │
                        └─────────────────┬───────────────────────┘
                                          │
                        ┌─────────────────┼─────────────────┐
                        │                 │                 │
                        ▼                 ▼                 ▼
              ┌─────────────────┐ ┌─────────────┐ ┌─────────────────┐
              │  GPU available  │ │ GPU in use  │ │  No suitable    │
              │  → Schedule     │ │ → Queue     │ │  → Error/MOLT   │
              └────────┬────────┘ └──────┬──────┘ └─────────────────┘
                       │                 │
                       ▼                 │
              ┌─────────────────┐        │
              │ StartWorkload   │        │
              │ → clawnode      │        │
              └────────┬────────┘        │
                       │                 │
                       ▼                 │
              ┌─────────────────┐        │
              │ Container pull  │        │
              │ GPU attach      │        │
              │ Execute         │        │
              └────────┬────────┘        │
                       │                 │
                       ▼                 │
              ┌─────────────────┐        │
              │ Status updates  │◄───────┘
              │ Metrics stream  │  (When GPUs free)
              │ Logs stream     │
              └────────┬────────┘
                       │
                       ▼
              ┌─────────────────┐
              │ Completion      │
              │ Cleanup         │
              │ Report results  │
              └─────────────────┘
```

### Metrics Flow

```
┌───────────────────────────────────────────────────────────────────────────┐
│                           clawnode (per node)                             │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐                   │
│  │ nvidia-smi  │    │ rocm-smi    │    │ Metal perf  │                   │
│  │ (CUDA)      │    │ (AMD)       │    │ (Apple)     │                   │
│  └──────┬──────┘    └──────┬──────┘    └──────┬──────┘                   │
│         │                  │                  │                           │
│         └──────────────────┼──────────────────┘                           │
│                            ▼                                              │
│                   ┌─────────────────┐                                     │
│                   │  GpuMetrics     │                                     │
│                   │  • utilization  │                                     │
│                   │  • memory_used  │                                     │
│                   │  • temperature  │                                     │
│                   │  • power_draw   │                                     │
│                   └────────┬────────┘                                     │
│                            │ every 10s                                    │
└────────────────────────────┼──────────────────────────────────────────────┘
                             │
                             ▼ WebSocket
┌────────────────────────────────────────────────────────────────────────────┐
│                              Gateway                                        │
│                    ┌─────────────────────────┐                             │
│                    │      MetricsStore       │                             │
│                    │  • Time-series DB       │                             │
│                    │  • Aggregation          │                             │
│                    │  • Retention policies   │                             │
│                    └────────────┬────────────┘                             │
│                                 │                                          │
│                    ┌────────────┼────────────┐                             │
│                    ▼            ▼            ▼                             │
│             ┌──────────┐ ┌──────────┐ ┌──────────────┐                    │
│             │ Observer │ │ Alerter  │ │ Dashboard API│                    │
│             │ (AI dx)  │ │          │ │ (REST/WS)    │                    │
│             └──────────┘ └──────────┘ └──────────────┘                    │
└────────────────────────────────────────────────────────────────────────────┘
```

---

## Crate Dependency Graph

```
                                    ┌─────────────┐
                                    │   claw-cli  │
                                    └──────┬──────┘
                                           │
                                           ▼
                              ┌─────────────────────────┐
                              │   claw-gateway-server   │
                              └────────────┬────────────┘
                                           │
                    ┌──────────────────────┼──────────────────────┐
                    │                      │                      │
                    ▼                      ▼                      ▼
             ┌──────────────┐      ┌──────────────┐      ┌──────────────┐
             │ claw-gateway │      │ claw-dashboard│      │ claw-observe │
             └──────┬───────┘      └──────┬───────┘      └──────┬───────┘
                    │                      │                      │
                    ▼                      │                      │
             ┌──────────────┐              │                      │
             │  claw-proto  │◄─────────────┴──────────────────────┘
             └──────┬───────┘
                    │
        ┌───────────┼───────────┬───────────┬───────────┐
        │           │           │           │           │
        ▼           ▼           ▼           ▼           ▼
  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
  │ clawnode │ │claw-auth │ │claw-ddos │ │claw-audit│ │claw-secrets│
  └────┬─────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘
       │
       ▼
  ┌──────────────┐
  │ claw-compute │
  └──────┬───────┘
         │
         ▼
  ┌─────────────────────────────────────────────────┐
  │                    CubeCL                        │
  │  ┌─────────┐  ┌─────────┐  ┌─────────┐         │
  │  │  CUDA   │  │  Metal  │  │  Vulkan │  (HIP)  │
  │  └─────────┘  └─────────┘  └─────────┘         │
  └─────────────────────────────────────────────────┘


  MOLT Stack:
  
  ┌─────────────┐
  │ molt-agent  │
  └──────┬──────┘
         │
  ┌──────┴──────┬──────────────┐
  │             │              │
  ▼             ▼              ▼
┌────────┐ ┌──────────┐ ┌──────────────┐
│molt-p2p│ │molt-market│ │molt-attestation│
└───┬────┘ └────┬─────┘ └──────────────┘
    │           │
    └─────┬─────┘
          ▼
    ┌──────────┐
    │molt-token│
    └────┬─────┘
         ▼
    ┌──────────┐
    │molt-core │
    └──────────┘
```

---

## Security Boundaries

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            TRUST BOUNDARY 1                                 │
│                         (Authenticated Control)                             │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                        Gateway Server                                  │  │
│  │  • TLS termination           • JWT validation                        │  │
│  │  • API key validation        • RBAC enforcement                      │  │
│  │  • Rate limiting             • Audit logging                         │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                    │                                        │
│                           mTLS (node certs)                                 │
│                                    │                                        │
│  ┌─────────────────────────────────┼─────────────────────────────────────┐  │
│  │                       TRUST BOUNDARY 2                                │  │
│  │                    (Workload Isolation)                               │  │
│  │                                 │                                     │  │
│  │  ┌──────────────────────────────┼──────────────────────────────────┐ │  │
│  │  │                          clawnode                                │ │  │
│  │  │  ┌────────────────────────────────────────────────────────────┐ │ │  │
│  │  │  │                    Container Runtime                        │ │ │  │
│  │  │  │  ┌───────────┐ ┌───────────┐ ┌───────────┐                │ │ │  │
│  │  │  │  │ Workload A│ │ Workload B│ │ Workload C│                │ │ │  │
│  │  │  │  │ (isolated)│ │ (isolated)│ │ (isolated)│                │ │ │  │
│  │  │  │  └───────────┘ └───────────┘ └───────────┘                │ │ │  │
│  │  │  └────────────────────────────────────────────────────────────┘ │ │  │
│  │  └──────────────────────────────────────────────────────────────────┘ │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│                            TRUST BOUNDARY 3                                 │
│                          (MOLT P2P Network)                                 │
│                                                                             │
│  • Cryptographic identity (Ed25519 keys)                                    │
│  • Hardware attestation (TEE/TPM when available)                           │
│  • Escrow-based payments (funds locked until proof)                        │
│  • Reputation scoring (historical behavior)                                │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Crate Map (37 crates)

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

---

## Command Tiers (91 commands per node)

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

## GPU Support

Real GPU acceleration via [CubeCL](https://github.com/tracel-ai/cubecl):

| Platform | Backend | Status |
|----------|---------|--------|
| NVIDIA | CUDA | Production |
| Apple Silicon | Metal | Production |
| AMD | ROCm/HIP | Production |
| Cross-platform | Vulkan | Production |
| Fallback | CPU SIMD | Production |

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

---

## See Also

- [User Guide](user-guide.md) — Getting started
- [CLI Reference](cli-reference.md) — Command reference
- [MOLT Network](molt-network.md) — P2P marketplace architecture
- [Security Guide](security.md) — Security deep-dive
- [API Documentation](api/README.md) — Crate-level API docs
- [Skills](skills.md) — Agent skills reference
- [Use Cases](use-cases.md) — Example conversations
