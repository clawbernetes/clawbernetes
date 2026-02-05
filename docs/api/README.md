# Clawbernetes API Documentation

Comprehensive API documentation for all Clawbernetes crates.

## Quick Links

| Category | Crates |
|----------|--------|
| [Core Infrastructure](#core-infrastructure) | `claw-proto`, `claw-gateway-server`, `claw-gateway`, `clawnode`, `claw-cli`, `claw-compute` |
| [Operations & Observability](#operations--observability) | `claw-metrics`, `claw-logs`, `claw-observe`, `claw-deploy`, `claw-rollback`, `claw-alerts`, `claw-autoscaler` |
| [Security](#security) | `claw-auth`, `claw-secrets`, `claw-pki`, `claw-audit`, `claw-ddos`, `claw-validation` |
| [Networking](#networking) | `claw-network`, `claw-wireguard`, `claw-tailscale`, `claw-discovery`, `claw-tenancy` |
| [MOLT Marketplace](#molt-marketplace) | `molt-core`, `molt-token`, `molt-market`, `molt-agent`, `molt-p2p`, `molt-attestation` |
| [Dashboard & Storage](#dashboard--storage) | `claw-dashboard`, `claw-storage`, `claw-preemption` |

---

## Core Infrastructure

### claw-proto

Protocol definitions for gateway-node communication using Protocol Buffers.

```rust
use claw_proto::{
    NodeMessage, GatewayMessage, NodeCapabilities,
    Workload, WorkloadSpec, WorkloadState,
    cli::{CliMessage, CliResponse},
};
```

**Key Types:**
- `NodeMessage` — Messages from nodes to gateway
- `GatewayMessage` — Messages from gateway to nodes
- `Workload` — Workload specification and state
- `NodeCapabilities` — Node hardware capabilities

**[Full Documentation →](./claw-proto.md)**

---

### claw-gateway-server

WebSocket gateway server for managing node fleet.

```rust
use claw_gateway_server::{GatewayServer, GatewayConfig};

#[tokio::main]
async fn main() {
    let config = GatewayConfig::default();
    let server = GatewayServer::new(config);
    server.serve("0.0.0.0:8080").await.unwrap();
}
```

**Key Types:**
- `GatewayServer` — Main server instance
- `GatewayConfig` — Server configuration

**[Full Documentation →](./claw-gateway-server.md)**

---

### claw-gateway

Shared gateway types and state management.

```rust
use claw_gateway::{NodeRegistry, WorkloadManager, FleetState};
```

**Key Types:**
- `NodeRegistry` — Connected node registry
- `WorkloadManager` — Workload lifecycle management
- `FleetState` — Cluster state snapshot

---

### clawnode

Node agent with GPU detection and workload execution.

```rust
use clawnode::{Node, NodeConfig, GpuDetector};
```

**Key Types:**
- `Node` — Node agent instance
- `NodeConfig` — Node configuration
- `GpuDetector` — Multi-platform GPU detection

**[Full Documentation →](./clawnode.md)**

---

### claw-cli

Command-line interface for cluster management.

```rust
use claw_cli::{Cli, Commands, GatewayClient, OutputFormat};
```

**Key Types:**
- `Cli` — CLI argument parser
- `Commands` — Command enum
- `GatewayClient` — WebSocket client

---

### claw-compute

Multi-platform GPU compute via CubeCL.

```rust
use claw_compute::{gpu, kernels, ComputeDevice, CpuTensor};

// GPU operations (Metal, CUDA, Vulkan, ROCm)
let result = gpu::gpu_add(&vec_a, &vec_b)?;
let activated = gpu::gpu_gelu(&tensor)?;

// CPU reference implementations
let output = kernels::matmul(&lhs, &rhs)?;
```

**Supported Platforms:**
| Platform | Backend | Feature Flag |
|----------|---------|--------------|
| NVIDIA | CUDA | `cubecl-cuda` |
| Apple Silicon | Metal | `cubecl-metal` |
| AMD | ROCm/HIP | `cubecl-hip` |
| Cross-platform | Vulkan | `cubecl-wgpu` |
| Fallback | CPU SIMD | default |

**[Full Documentation →](./claw-compute.md)**

---

## Operations & Observability

### claw-metrics

Embedded time-series database for metrics storage.

```rust
use claw_metrics::{MetricsStore, Metric, MetricValue, Query, Aggregation};

let store = MetricsStore::new();
store.record("gpu_utilization", MetricValue::Gauge(0.75), labels!{"gpu" => "0"});

let results = store.query(Query::new("gpu_utilization")
    .with_aggregation(Aggregation::Avg)
    .since(Duration::hours(1)))?;
```

---

### claw-logs

Structured log aggregation with semantic search.

```rust
use claw_logs::{LogStore, LogEntry, LogQuery, LogLevel};

let store = LogStore::new();
store.append(LogEntry::new(LogLevel::Info, "Workload started"));

let logs = store.search(LogQuery::new()
    .workload("my-workload")
    .level(LogLevel::Error)
    .since(Duration::hours(1)))?;
```

---

### claw-observe

AI-native observability combining metrics, logs, and traces.

```rust
use claw_observe::{Observer, Analyzer, Insight, Diagnosis};

let observer = Observer::new(metrics_store, log_store);
let diagnosis = observer.diagnose("Why is GPU 3 slow?")?;
// Returns: Insight { cause: "Thermal throttling at 89°C", recommendation: "..." }
```

---

### claw-deploy

Intent-based deployment engine.

```rust
use claw_deploy::{DeploymentIntent, DeploymentPlan, Deployer};

let intent = DeploymentIntent::parse("Run Llama 70B on 4 H100s")?;
let plan = Deployer::plan(&intent, &cluster_state)?;
let deployment = Deployer::execute(plan).await?;
```

---

### claw-rollback

Automatic rollback with root-cause analysis.

```rust
use claw_rollback::{RollbackManager, RollbackTrigger, RollbackPlan};

let manager = RollbackManager::new();
manager.set_trigger(RollbackTrigger::HealthCheckFailed { threshold: 3 });

// Automatic rollback on failure
let plan = manager.analyze_failure(deployment_id)?;
manager.execute_rollback(plan).await?;
```

---

### claw-alerts

Alert management and notification routing.

```rust
use claw_alerts::{AlertManager, AlertRule, Severity, NotificationChannel};

let mut manager = AlertManager::new();
manager.add_rule(AlertRule::new("high_gpu_temp")
    .condition("gpu_temperature > 85")
    .severity(Severity::Warning)
    .notify(NotificationChannel::Slack("gpu-alerts")));
```

---

### claw-autoscaler

Cluster autoscaling policies.

```rust
use claw_autoscaler::{Autoscaler, ScalingPolicy, PolicyType};

let policy = ScalingPolicy::new("gpu-pool-1", PolicyType::Utilization)
    .target(0.70)
    .min_nodes(2)
    .max_nodes(20);

let autoscaler = Autoscaler::new(vec![policy]);
autoscaler.evaluate(&cluster_metrics).await?;
```

---

## Security

### claw-auth

Authentication and Role-Based Access Control (RBAC).

```rust
use claw_auth::{
    ApiKey, ApiKeyStore, JwtConfig, JwtManager,
    RbacPolicy, AuthContext, User, Role, Permission, Action,
};

// API Key authentication
let mut store = ApiKeyStore::new();
let (key, secret) = ApiKey::generate("CI/CD", user_id);
store.store(key);
let auth = authenticate_api_key(&store, secret.as_str())?;

// JWT tokens
let config = JwtConfig::new_hs256(secret, "clawbernetes")?;
let manager = JwtManager::new(config);
let token = manager.create_token(&user_id)?;

// RBAC
let mut policy = RbacPolicy::with_default_roles();
policy.assign_role(&user_id, "operator")?;
assert!(policy.check_permission(&user_id, "workloads", Action::Create));
```

---

### claw-secrets

Encrypted secrets management with workload identity.

```rust
use claw_secrets::{SecretStore, SecretKey, SecretId, AccessPolicy, WorkloadId};

let key = SecretKey::generate();
let store = SecretStore::new(key);

let id = SecretId::new("database-password")?;
let policy = AccessPolicy::allow_workloads(vec![
    WorkloadId::new("api-server"),
    WorkloadId::new("worker"),
]);

store.set(&id, b"super-secret", policy)?;
let secret = store.get(&id, &workload_identity)?;
```

---

### claw-pki

Certificate authority for node and workload identity.

```rust
use claw_pki::{CertificateAuthority, CertRequest, KeyUsage};

let ca = CertificateAuthority::init("/etc/clawbernetes/pki")?;

let request = CertRequest::new("gpu-node-1")
    .with_dns("gpu-node-1.example.com")
    .with_usage(KeyUsage::ClientAuth);

let cert = ca.issue(request)?;
cert.save("/etc/clawbernetes/pki/node.crt")?;
```

---

### claw-audit

Security audit logging.

```rust
use claw_audit::{AuditLog, AuditEntry, AuditAction, AuditFilter};

let log = AuditLog::new("/var/log/clawbernetes/audit")?;

log.record(AuditEntry::new(AuditAction::SecretAccessed)
    .with_user(&user_id)
    .with_resource("database-password"))?;

let entries = log.query(AuditFilter::new()
    .action(AuditAction::SecretAccessed)
    .since(Duration::days(7)))?;
```

---

### claw-ddos

Comprehensive DDoS protection.

```rust
use claw_ddos::{DdosProtection, DdosConfig, ProtectionResult};

let config = DdosConfig::builder()
    .connection(ConnectionConfig { max_per_ip: 100, ..Default::default() })
    .rate_limit(RateLimitConfig { requests_per_second: 100, ..Default::default() })
    .build();

let protection = DdosProtection::new(config);

match protection.check_connection(&client_ip) {
    ProtectionResult::Allow => handle_request(),
    ProtectionResult::RateLimit { retry_after_ms } => return_429(retry_after_ms),
    ProtectionResult::Block { reason, .. } => return_403(reason),
}
```

---

### claw-validation

Input validation utilities.

```rust
use claw_validation::{validate_image, validate_env_key, validate_resources};

validate_image("pytorch/pytorch:2.0")?;  // OK
validate_image("../escape")?;            // Error: InvalidImage

validate_env_key("MODEL_NAME")?;         // OK
validate_env_key("1INVALID")?;           // Error: InvalidEnvKey
```

---

## Networking

### claw-network

Mesh network topology management.

```rust
use claw_network::{MeshNetwork, Peer, NetworkConfig, NetworkProvider};

let config = NetworkConfig::new(NetworkProvider::WireGuard);
let network = MeshNetwork::new(config)?;

network.add_peer(Peer::new("node-2", "10.100.0.2"))?;
network.connect().await?;
```

---

### claw-wireguard

WireGuard VPN integration.

```rust
use claw_wireguard::{WireGuardConfig, Interface, Peer};

let config = WireGuardConfig::new()
    .listen_port(51820)
    .private_key_path("/etc/clawbernetes/wg-private.key");

let interface = Interface::new(config)?;
interface.add_peer(Peer::new(public_key, "10.100.0.2/32"))?;
interface.up()?;
```

**[Full Documentation → wireguard-integration.md](../wireguard-integration.md)**

---

### claw-tailscale

Tailscale managed mesh integration.

```rust
use claw_tailscale::{TailscaleClient, TailscaleConfig};

let config = TailscaleConfig::new()
    .auth_key_env("TS_AUTHKEY")
    .hostname("clawnode-gpu-1")
    .tags(vec!["tag:clawbernetes"]);

let client = TailscaleClient::new(config)?;
client.connect().await?;
```

**[Full Documentation → tailscale-integration.md](../tailscale-integration.md)**

---

### claw-discovery

Service discovery for cluster components.

```rust
use claw_discovery::{DiscoveryService, ServiceRecord, ServiceQuery};

let discovery = DiscoveryService::new();
discovery.register(ServiceRecord::new("gateway", "10.100.0.1:8080"))?;

let services = discovery.query(ServiceQuery::new("gateway"))?;
```

---

### claw-tenancy

Multi-tenancy isolation.

```rust
use claw_tenancy::{Tenant, TenantConfig, IsolationPolicy};

let tenant = Tenant::new("team-ml", TenantConfig::new()
    .with_quota(GpuQuota::new(16))
    .with_isolation(IsolationPolicy::Strict))?;
```

---

## MOLT Marketplace

### molt-core

Core primitives for the MOLT token economy.

```rust
use molt_core::{Amount, Policy, AutonomyLevel, Reputation, Wallet};

// Token amounts with fixed-point precision
let price = Amount::from_float(1.5)?;
let total = price.checked_mul(&Amount::from_u64(4))?;

// Autonomy policies
let policy = Policy::builder()
    .autonomy(AutonomyLevel::Moderate)
    .max_spend_per_job(Amount::from_float(100.0)?)
    .build();
```

---

### molt-token

Solana SPL token client for MOLT.

```rust
use molt_token::{MoltClient, Network, Transaction, Escrow};

let client = MoltClient::new(Network::Mainnet, wallet)?;

// Check balance
let balance = client.balance().await?;

// Create escrow for job
let escrow = client.create_escrow(
    job_id,
    provider_pubkey,
    Amount::from_float(50.0)?,
).await?;

// Release on completion
client.release_escrow(escrow.id, execution_proof).await?;
```

**[Full Documentation →](./molt-token.md)**

---

### molt-market

Decentralized orderbook and settlement.

```rust
use molt_market::{OrderBook, JobOrder, CapacityOffer, Match};

let mut orderbook = OrderBook::new();

// Provider posts capacity
orderbook.post_offer(CapacityOffer::new(provider_id)
    .gpus(8, GpuType::H100)
    .price_per_hour(Amount::from_float(5.0)?))?;

// Buyer posts job request
orderbook.post_order(JobOrder::new(buyer_id)
    .gpus(4, GpuType::H100)
    .duration_hours(8)
    .max_price(Amount::from_float(200.0)?))?;

// Match orders
let matches = orderbook.match_orders()?;
```

**[Full Documentation →](./molt-market.md)**

---

### molt-agent

Autonomous provider/buyer agents.

```rust
use molt_agent::{
    ProviderAgent, BuyerAgent, AutonomyMode,
    ProviderPolicy, JobSpec, evaluate_job,
};

// Provider agent
let policy = ProviderPolicy::for_mode(AutonomyMode::Moderate);
let decision = evaluate_job(&job_spec, AutonomyMode::Moderate, &policy);

if decision.is_accept() {
    accept_job(job_spec).await?;
}

// Buyer agent
let buyer = BuyerAgent::new(AutonomyMode::Moderate);
let offers = buyer.find_providers(&requirements).await?;
let best = buyer.select_best_offer(&offers)?;
```

**[Full Documentation →](./molt-agent.md)**

---

### molt-p2p

Peer discovery and gossip protocol.

```rust
use molt_p2p::{P2pNetwork, Discovery, GossipMessage};

let network = P2pNetwork::new(config)?;
network.start().await?;

// Broadcast capacity offer
network.broadcast(GossipMessage::CapacityOffer(offer)).await?;

// Listen for job requests
network.subscribe(|msg| {
    if let GossipMessage::JobRequest(req) = msg {
        handle_request(req);
    }
}).await?;
```

---

### molt-attestation

Hardware and execution verification.

```rust
use molt_attestation::{
    HardwareAttestation, ExecutionProof, Verifier, Challenge,
};

// Generate attestation
let challenge = Challenge::new();
let attestation = HardwareAttestation::generate(&challenge)?;

// Verify
let verifier = Verifier::new();
verifier.verify(&attestation, &challenge)?;

// Execution proof
let proof = ExecutionProof::new(job_id)
    .with_metrics(gpu_metrics)
    .with_output_hash(output_hash)
    .sign(&private_key)?;
```

---

## Dashboard & Storage

### claw-dashboard

Web dashboard REST API and WebSocket streaming.

```rust
use claw_dashboard::{DashboardServer, DashboardConfig, DashboardState};

let config = DashboardConfig::default();
let state = DashboardState::new(node_registry, workload_manager);
let server = DashboardServer::new(config, state);

server.serve("0.0.0.0:8080").await?;
```

**REST API Endpoints:**

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/status` | GET | Cluster overview |
| `/api/nodes` | GET | List all nodes |
| `/api/nodes/:id` | GET | Node details |
| `/api/nodes/:id/metrics` | GET | Node metrics |
| `/api/workloads` | GET | List workloads |
| `/api/workloads/:id` | GET | Workload details |
| `/api/workloads/:id/logs` | GET | Workload logs (SSE) |
| `/api/ws` | WS | Real-time updates |

---

### claw-storage

Persistent storage abstraction.

```rust
use claw_storage::{Storage, StorageConfig, StorageBackend};

let config = StorageConfig::new(StorageBackend::Sqlite)
    .path("/var/lib/clawbernetes/data.db");

let storage = Storage::new(config)?;
storage.put("key", &value).await?;
let value: T = storage.get("key").await?;
```

---

### claw-preemption

Workload preemption policies.

```rust
use claw_preemption::{PreemptionPolicy, Priority, PreemptionManager};

let policy = PreemptionPolicy::new()
    .priority_based(true)
    .grace_period_secs(30);

let manager = PreemptionManager::new(policy);
let to_preempt = manager.evaluate(&pending_workloads, &running_workloads)?;
```

---

## Generating Documentation

Generate HTML documentation for all crates:

```bash
# Generate docs (opens in browser)
cargo doc --workspace --no-deps --open

# With private items
cargo doc --workspace --no-deps --document-private-items

# Generate without opening
cargo doc --workspace --no-deps
# Output: target/doc/
```

---

## Version Compatibility

| Crate | Min Rust | Edition |
|-------|----------|---------|
| All crates | 1.85+ | 2024 |

---

## License

All APIs are available under the MIT license. See [LICENSE-MIT](../../LICENSE-MIT).
