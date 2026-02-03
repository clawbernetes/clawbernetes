# Tailscale Integration for Clawbernetes

## Why Tailscale as an Option

Tailscale provides a **managed WireGuard control plane** with enterprise features:

| Feature | Raw WireGuard | Tailscale |
|---------|---------------|-----------|
| Key exchange | Manual/gateway | Automatic (DERP) |
| NAT traversal | DIY STUN | Battle-tested |
| Identity | DIY PKI | SSO/OIDC/SAML |
| Access control | Static configs | ACL policies |
| Audit logs | DIY | Built-in |
| Multi-cloud | Complex | Zero config |

**Offer both:**
- `claw-wireguard` — Self-hosted, zero dependencies
- `claw-tailscale` — Managed, enterprise features

---

## Tailscale v1.94 Features We Can Leverage

### 1. Tailscale Services (NEW!)

Services decouple resources from specific devices:

```
clawbernetes-gateway.tailnet → Any gateway node
clawbernetes-gpu-pool.tailnet → Any GPU provider
```

**Why this matters:**
- GPU pools advertise as a single service
- Failover automatic
- MagicDNS names stable across migrations
- Built-in load balancing

### 2. tsnet — Embed Tailscale in Clawnode

```go
// Go example (Rust equivalent via FFI or port)
srv := &tsnet.Server{
    Hostname: "clawnode-gpu-1",
    AuthKey:  os.Getenv("TS_AUTHKEY"),
}
srv.Start()

// Listen for MOLT job requests
ln, _ := srv.ListenService("molt-provider", tsnet.ServiceModeTCP{Port: 8443})
```

**Benefits:**
- No separate tailscaled daemon
- Single binary deployment
- Programmatic control

### 3. ListenService — Service-Oriented Networking

```rust
// Conceptual Rust API
let tailscale = TailscaleNode::new(config)?;
tailscale.listen_service("clawbernetes-gateway", ServiceMode::TCP { port: 443 })?;
```

Nodes register as service backends automatically.

### 4. Workload Identity Federation

Authenticate to Tailscale using:
- AWS IAM roles
- GCP service accounts  
- Azure managed identities
- Kubernetes ServiceAccount tokens

**No long-lived credentials needed!**

### 5. Peer Relays

Improved throughput for relay traffic when direct connections fail.

---

## Architecture

### Dual-Mode Networking

```
┌─────────────────────────────────────────────────────────────────┐
│                    claw-networking (unified API)                │
│                                                                 │
│   trait NetworkProvider {                                       │
│       fn join_mesh(&self, config: &MeshConfig) -> Result<()>;  │
│       fn get_mesh_ip(&self) -> IpAddr;                         │
│       fn add_peer(&mut self, peer: &PeerInfo) -> Result<()>;   │
│       fn advertise_service(&self, svc: &Service) -> Result<()>;│
│   }                                                             │
└─────────────────────────────────────────────────────────────────┘
              │                           │
              ▼                           ▼
┌─────────────────────────┐   ┌─────────────────────────┐
│     claw-wireguard      │   │     claw-tailscale      │
│                         │   │                         │
│  • Self-hosted mesh     │   │  • Tailscale control    │
│  • Ed25519 keys         │   │  • SSO integration      │
│  • Gateway coordination │   │  • Tailscale Services   │
│  • Full control         │   │  • MagicDNS             │
└─────────────────────────┘   └─────────────────────────┘
```

### Configuration

```toml
# clawbernetes.toml

[network]
# Choose: "wireguard" | "tailscale"
provider = "tailscale"

[network.wireguard]
# Self-hosted WireGuard mesh
listen_port = 51820
mesh_cidr = "10.100.0.0/16"

[network.tailscale]
# Tailscale managed mesh
auth_key_env = "TS_AUTHKEY"  # Or use workload identity
# auth_key_file = "/etc/clawbernetes/ts-authkey"
hostname_prefix = "clawnode"
tailnet = "your-org.tailscale.net"
tags = ["tag:clawbernetes", "tag:gpu-provider"]

# Advertise as Tailscale Service
[network.tailscale.services]
gateway = { port = 443, mode = "tcp" }
molt-provider = { port = 8443, mode = "tcp" }
```

---

## claw-tailscale Crate Design

### Files

```
crates/claw-tailscale/
├── Cargo.toml
└── src/
    ├── lib.rs           # Public API
    ├── node.rs          # TailscaleNode wrapper
    ├── service.rs       # Service advertisement
    ├── auth.rs          # Auth key / workload identity
    ├── acl.rs           # ACL policy helpers
    └── error.rs         # Error types
```

### Core Types

```rust
// claw-tailscale/src/lib.rs

pub struct TailscaleConfig {
    pub auth: AuthMethod,
    pub hostname: String,
    pub tailnet: String,
    pub tags: Vec<String>,
    pub services: Vec<ServiceConfig>,
}

pub enum AuthMethod {
    /// Pre-generated auth key
    AuthKey(String),
    /// Read from file
    AuthKeyFile(PathBuf),
    /// Environment variable
    AuthKeyEnv(String),
    /// Workload identity federation (AWS/GCP/Azure/K8s)
    WorkloadIdentity {
        issuer: String,
        audience: Option<String>,
    },
}

pub struct ServiceConfig {
    pub name: String,
    pub port: u16,
    pub mode: ServiceMode,
}

pub enum ServiceMode {
    Tcp,
    Http { tls: bool },
}
```

### Implementation Strategy

**Option A: Shell out to `tailscale` CLI**
```rust
// Simple but requires tailscale installed
Command::new("tailscale")
    .args(["serve", "--service", &service_name, &format!("tcp:{}", port)])
    .spawn()?;
```

**Option B: tsnet via CGO (if we had Go)**
```go
// Direct embedding, best performance
srv := &tsnet.Server{Hostname: hostname}
srv.ListenService(name, mode)
```

**Option C: tailscale daemon socket API**
```rust
// Talk to tailscaled over local socket
let client = LocalClient::connect("/var/run/tailscale/tailscaled.sock")?;
client.serve_service(name, config).await?;
```

**Recommendation: Option C** — Most Rust-native, works with existing tailscaled.

---

## Integration Points

### 1. clawnode Startup

```rust
// clawnode/src/main.rs

async fn main() -> Result<()> {
    let config = load_config()?;
    
    // Initialize network based on config
    let network: Box<dyn NetworkProvider> = match config.network.provider {
        NetworkProvider::WireGuard => {
            Box::new(WireGuardMesh::new(&config.network.wireguard)?)
        }
        NetworkProvider::Tailscale => {
            Box::new(TailscaleNode::new(&config.network.tailscale).await?)
        }
    };
    
    network.join_mesh(&mesh_config).await?;
    
    // Advertise GPU services
    if config.molt.enabled {
        network.advertise_service(&Service {
            name: "molt-provider".into(),
            port: 8443,
        }).await?;
    }
    
    // ... rest of node startup
}
```

### 2. MOLT Provider Discovery

With Tailscale Services, buyers can discover providers via MagicDNS:

```rust
// molt-p2p with Tailscale
let providers = tailscale.discover_service("molt-provider").await?;
// Returns all nodes advertising the molt-provider service
```

### 3. Gateway HA with Services

```rust
// Multiple gateways advertise same service
// Tailscale handles failover automatically

gateway1.advertise_service("clawbernetes-gateway", 443).await?;
gateway2.advertise_service("clawbernetes-gateway", 443).await?;

// Clients connect to: clawbernetes-gateway.tailnet.ts.net
// Traffic routes to any healthy gateway
```

---

## Comparison: When to Use Which

### Use Raw WireGuard When:
- ✅ Full control required
- ✅ No external dependencies
- ✅ Air-gapped environments
- ✅ Custom identity/PKI integration
- ✅ Cost-sensitive (Tailscale has pricing tiers)

### Use Tailscale When:
- ✅ SSO/OIDC identity integration needed
- ✅ Enterprise audit requirements
- ✅ Multi-cloud without VPN complexity
- ✅ Team collaboration features
- ✅ Quick setup preferred
- ✅ Tailscale Services load balancing

---

## Implementation Plan

### Phase 1: Basic Integration
```
- [ ] claw-tailscale crate skeleton
- [ ] TailscaleNode struct with connect/disconnect
- [ ] Auth key support
- [ ] NetworkProvider trait implementation
```

### Phase 2: Service Support
```
- [ ] Service advertisement via tailscale CLI
- [ ] Service discovery
- [ ] Health monitoring
```

### Phase 3: Advanced Features
```
- [ ] Workload identity federation
- [ ] ACL policy helpers
- [ ] Peer Relay optimization hints
- [ ] tsnet direct embedding (if Go bridge feasible)
```

---

## MOLT + Tailscale Vision

```
┌─────────────────────────────────────────────────────────────────┐
│                    MOLT Marketplace + Tailscale                 │
│                                                                 │
│  Buyer's Tailnet                    Provider's Tailnet          │
│       │                                   │                     │
│       │      Tailscale Peer Relay        │                     │
│       └───────────── ◄═══► ──────────────┘                     │
│                                                                 │
│  • Buyer searches "molt-provider" service                      │
│  • Tailscale routes to available GPU providers                 │
│  • Job tunnel established automatically                         │
│  • Billing via MOLT tokens                                      │
│  • Zero networking config for either party                      │
│                                                                 │
│  "Sell your GPU: install clawnode, add to Tailscale, done"     │
└─────────────────────────────────────────────────────────────────┘
```

---

## Questions to Decide

1. **Default provider?**
   - Tailscale: Better UX, external dependency
   - WireGuard: Self-contained, more setup
   - Recommendation: WireGuard default, Tailscale as upgrade

2. **tailscaled or tsnet?**
   - tailscaled: Stable, CLI-based, daemon
   - tsnet: Embedded, Go-only (for now)
   - Recommendation: tailscaled for v1, explore tsnet later

3. **Tailscale pricing tier?**
   - Free: 100 devices, 3 users
   - Starter: $6/user/mo
   - Premium: $18/user/mo
   - Document tier requirements for different scales

4. **Hybrid mode?**
   - Some nodes on Tailscale, some on raw WireGuard?
   - Recommendation: Single provider per cluster for simplicity
