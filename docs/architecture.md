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

## See Also

- [User Guide](user-guide.md) — Getting started
- [CLI Reference](cli-reference.md) — Command reference
- [MOLT Network](molt-network.md) — P2P marketplace architecture
- [Security Guide](security.md) — Security deep-dive
- [API Documentation](api/README.md) — Crate-level API docs
