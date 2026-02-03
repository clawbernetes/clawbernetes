# WireGuard Deep Integration for Clawbernetes

## The Vision

**WireGuard becomes the fundamental network layer for Clawbernetes.**

Not just "another VPN option" — WireGuard IS the cluster network. Every node, every workload, every GPU communicates through WireGuard tunnels.

```
┌─────────────────────────────────────────────────────────────────┐
│                    Traditional K8s Networking                    │
│  CNI → Calico/Flannel → iptables → VXLAN/IPIP → pain           │
│  Service mesh → Istio/Linkerd → sidecars → complexity          │
│  Network policy → OPA → YAML → confusion                       │
└─────────────────────────────────────────────────────────────────┘
                              vs
┌─────────────────────────────────────────────────────────────────┐
│                    Clawbernetes + WireGuard                     │
│  Every node has a WireGuard interface                          │
│  Identity = WireGuard public key = MOLT identity               │
│  Encryption built-in, no sidecars, no complexity               │
│  GPU-to-GPU communication across datacenters, encrypted        │
└─────────────────────────────────────────────────────────────────┘
```

---

## Why WireGuard Changes Everything

### 1. Identity Unification

```
PeerId = MOLT Identity = WireGuard Public Key = Ed25519

One key to rule them all:
- Node authentication
- MOLT marketplace identity
- Workload attestation signing
- Network encryption
```

### 2. NAT Traversal for MOLT

```
MOLT Provider (behind NAT)              MOLT Buyer
       │                                    │
       │ WireGuard UDP hole punch          │
       └──────────── ◄───► ────────────────┘
                 Encrypted tunnel
                 
Provider can sell GPU compute from home/office/anywhere
No port forwarding, no static IPs required
```

### 3. Zero-Trust by Default

```
Every packet between clawnodes is:
- Authenticated (WireGuard key)
- Encrypted (ChaCha20-Poly1305)
- Replay-protected
- Perfect forward secrecy

No "trust the network" assumptions
```

### 4. Multi-Cloud Native

```
AWS Node ─────┐
              │
GCP Node ─────┼───► WireGuard Mesh ───► Single Cluster
              │
Home GPU ─────┘

All nodes appear on same L3 network (10.100.x.x)
```

---

## Architecture

### Core Components

```
crates/
├── claw-wireguard/          # WireGuard integration
│   ├── src/
│   │   ├── interface.rs     # WireGuard interface management
│   │   ├── peer.rs          # Peer configuration
│   │   ├── config.rs        # wg0.conf generation
│   │   ├── nat.rs           # NAT traversal helpers
│   │   ├── mesh.rs          # Full mesh topology
│   │   └── lib.rs
│   └── Cargo.toml
│
├── claw-network/            # Cluster networking
│   ├── src/
│   │   ├── allocation.rs    # IP allocation (10.100.x.x)
│   │   ├── routing.rs       # Inter-node routing
│   │   ├── dns.rs           # Service discovery
│   │   └── lib.rs
│   └── Cargo.toml
```

### IP Allocation Scheme

```
10.100.0.0/16 - Clawbernetes mesh

10.100.0.0/24    - Gateway/control plane
10.100.1.0/24    - Reserved
10.100.2.0/20    - Region: us-west (4094 nodes)
10.100.18.0/20   - Region: us-east (4094 nodes)
10.100.34.0/20   - Region: eu-west (4094 nodes)
10.100.50.0/20   - Region: asia (4094 nodes)
10.100.128.0/17  - MOLT marketplace (32766 providers)

Per-node workload subnet: 10.200.{node_id}.0/24
```

### Node Registration Flow

```
1. Node generates Ed25519 keypair (or uses existing MOLT key)
2. Node contacts gateway with public key
3. Gateway allocates:
   - WireGuard IP (10.100.x.x)
   - Workload subnet (10.200.x.0/24)
4. Gateway returns peer configs for existing nodes
5. Node establishes WireGuard tunnels to peers
6. Node is now part of the mesh
```

---

## Key Features

### 1. Automatic Mesh Formation

```rust
// claw-wireguard/src/mesh.rs

pub struct WireGuardMesh {
    interface: WireGuardInterface,
    peers: HashMap<PeerId, WireGuardPeer>,
    topology: MeshTopology,
}

impl WireGuardMesh {
    /// Add a peer to the mesh
    pub async fn add_peer(&mut self, peer: PeerInfo) -> Result<()> {
        let wg_peer = WireGuardPeer {
            public_key: peer.wireguard_key,
            allowed_ips: vec![
                peer.mesh_ip,           // Node IP
                peer.workload_subnet,   // Workload subnet
            ],
            endpoint: peer.endpoint,
            persistent_keepalive: Some(25),
        };
        
        self.interface.add_peer(&wg_peer).await?;
        self.peers.insert(peer.id, wg_peer);
        Ok(())
    }
    
    /// Automatically discover and connect to peers
    pub async fn auto_mesh(&mut self, discovery: &PeerDiscovery) -> Result<()> {
        for peer in discovery.get_peers().await? {
            if !self.peers.contains_key(&peer.id) {
                self.add_peer(peer).await?;
            }
        }
        Ok(())
    }
}
```

### 2. MOLT Provider Tunnel

```rust
// MOLT provider exposes GPU via WireGuard

pub struct MoltProviderTunnel {
    /// WireGuard interface for MOLT traffic
    interface: WireGuardInterface,
    
    /// Active job tunnels
    job_tunnels: HashMap<JobId, JobTunnel>,
}

impl MoltProviderTunnel {
    /// Accept a job and create dedicated tunnel
    pub async fn accept_job(&mut self, job: Job, buyer: PeerInfo) -> Result<JobTunnel> {
        // Create WireGuard peer for this specific buyer
        let tunnel = JobTunnel {
            job_id: job.id,
            buyer_key: buyer.wireguard_key,
            allocated_ip: self.allocate_job_ip()?,
            gpu_access: job.gpu_requirements,
        };
        
        // Buyer can now directly access GPU over WireGuard
        self.interface.add_peer(&tunnel.to_wg_peer()).await?;
        self.job_tunnels.insert(job.id, tunnel.clone());
        
        Ok(tunnel)
    }
}
```

### 3. GPU-Direct Over WireGuard

```
Buyer's Training Job                Provider's GPU
       │                                  │
       │     WireGuard Tunnel            │
       └──────────────────────────────────┘
       
       10.100.x.x ←───────→ 10.100.y.y
       
CUDA operations tunneled through WireGuard:
- cuMemcpy (GPU memory transfers)
- NCCL (multi-GPU communication)
- Tensor streaming
```

### 4. Agent-Managed Networking

```
User: "Add my home GPU to the cluster"

Agent:
1. Generates WireGuard keypair on home machine
2. Contacts gateway for mesh config
3. Configures WireGuard interface
4. Tests connectivity to existing nodes
5. Registers GPU capabilities

User: "My home GPU is now visible as node-47"
```

---

## Integration Points

### 1. clawnode Integration

```rust
// clawnode/src/network.rs

pub struct NodeNetwork {
    /// WireGuard mesh interface
    wireguard: WireGuardMesh,
    
    /// Node's mesh IP
    mesh_ip: IpAddr,
    
    /// Workload subnet for this node
    workload_subnet: IpNet,
}

impl Node {
    pub async fn initialize_network(&mut self) -> Result<()> {
        // Get WireGuard config from gateway
        let config = self.gateway.get_wireguard_config().await?;
        
        // Initialize WireGuard interface
        self.network.wireguard.configure(&config).await?;
        
        // Connect to existing peers
        self.network.wireguard.auto_mesh(&self.discovery).await?;
        
        info!(mesh_ip = %self.network.mesh_ip, "node joined mesh");
        Ok(())
    }
}
```

### 2. MOLT Network Integration

```rust
// molt-p2p becomes molt-wireguard for transport

pub struct MoltNetwork {
    /// WireGuard-based peer connections
    mesh: WireGuardMesh,
    
    /// Peer discovery (can use WireGuard keys for identity)
    discovery: PeerDiscovery,
    
    /// Job tunnels for active compute jobs
    job_tunnels: HashMap<JobId, JobTunnel>,
}

impl MoltNetwork {
    /// Join the MOLT network
    pub async fn join(&mut self, bootstrap: &[BootstrapNode]) -> Result<()> {
        // WireGuard handles the secure connection
        for node in bootstrap {
            self.mesh.add_peer(node.to_peer_info()).await?;
        }
        
        // Announce our capacity over the mesh
        self.announce_capacity().await?;
        Ok(())
    }
}
```

### 3. Gateway Integration

```rust
// claw-gateway-server manages the mesh

pub struct GatewayNetwork {
    /// All nodes in the mesh
    nodes: HashMap<NodeId, NodeNetworkInfo>,
    
    /// IP allocator
    allocator: IpAllocator,
    
    /// WireGuard config generator
    config_gen: WireGuardConfigGenerator,
}

impl GatewayServer {
    /// Handle node registration with WireGuard
    pub async fn register_node(&mut self, 
        node_id: NodeId,
        wireguard_key: PublicKey,
        endpoint: Option<SocketAddr>,
    ) -> Result<NodeNetworkConfig> {
        // Allocate IPs
        let mesh_ip = self.network.allocator.allocate_node_ip()?;
        let workload_subnet = self.network.allocator.allocate_workload_subnet()?;
        
        // Generate WireGuard config for this node
        let wg_config = self.network.config_gen.generate_node_config(
            wireguard_key,
            mesh_ip,
            workload_subnet,
            &self.network.nodes,  // Existing peers
        )?;
        
        // Store node info
        self.network.nodes.insert(node_id, NodeNetworkInfo {
            mesh_ip,
            workload_subnet,
            wireguard_key,
            endpoint,
        });
        
        // Notify existing nodes of new peer
        self.broadcast_peer_update(node_id).await?;
        
        Ok(NodeNetworkConfig {
            mesh_ip,
            workload_subnet,
            wireguard_config: wg_config,
            peers: self.get_peer_configs(&node_id),
        })
    }
}
```

---

## Security Model

### Key Hierarchy

```
Cluster Root Key (held by gateway)
       │
       ├── Node Keys (Ed25519 = WireGuard key)
       │       │
       │       └── Workload Keys (derived, short-lived)
       │
       └── MOLT Keys (same as Node keys for providers)
```

### Trust Model

```
1. Gateway is trusted root
2. Nodes trust gateway's peer announcements
3. Nodes verify each other via WireGuard handshake
4. Workloads inherit node's network identity
5. MOLT jobs get scoped, temporary tunnel access
```

### Zero-Trust Principles

```
- No implicit trust based on network location
- Every connection authenticated via WireGuard
- Workload-to-workload traffic encrypted
- MOLT jobs isolated to specific tunnels
- Automatic key rotation supported
```

---

## Implementation Plan

### Phase 1: Core WireGuard Crate (claw-wireguard)

```
- WireGuard interface management
- Peer configuration
- Config file generation
- Cross-platform support (Linux, macOS, Windows)
```

### Phase 2: Mesh Networking (claw-network)

```
- IP allocation
- Mesh topology management
- Peer discovery integration
- DNS/service discovery
```

### Phase 3: MOLT Integration

```
- Replace QUIC with WireGuard tunnels
- Job-specific tunnel creation
- Provider NAT traversal
- Bandwidth metering
```

### Phase 4: Advanced Features

```
- Split tunneling for hybrid workloads
- Multi-path WireGuard (bonding)
- Geographic routing optimization
- Bandwidth QoS per job
```

---

## Comparison: Before & After

### Before (Traditional)

```
Node A                          Node B
  │                               │
  └── TCP/QUIC ──────────────────┘
      - Manual TLS setup
      - Certificate management
      - Port forwarding needed
      - Complex NAT traversal
```

### After (WireGuard)

```
Node A                          Node B
  │                               │
  └── WireGuard ─────────────────┘
      - Single key exchange
      - Built-in encryption
      - UDP hole punching
      - Works everywhere
```

---

## The Game-Changer for MOLT

```
Traditional P2P Compute:
- Complex NAT traversal (STUN/TURN/ICE)
- TLS certificate management
- Firewall configuration
- Often requires relay servers

MOLT + WireGuard:
- Provider starts clawnode
- WireGuard auto-connects to mesh
- Buyer purchases compute
- Direct encrypted tunnel established
- GPU accessible as if local

"Sell your GPU from your bedroom, no IT degree required"
```

---

## Questions to Decide

1. **Userspace vs Kernel WireGuard?**
   - Kernel: Better performance, requires root
   - Userspace (wireguard-go): Portable, slower
   - Recommendation: Kernel by default, userspace fallback

2. **Full mesh or hub-and-spoke?**
   - Full mesh: Better latency, O(n²) connections
   - Hub: Gateway as relay, simpler, single point of failure
   - Recommendation: Full mesh for small clusters, regional hubs for large

3. **WireGuard key = MOLT key?**
   - Yes: Simpler, single identity
   - No: Flexibility, separate concerns
   - Recommendation: Yes, use Ed25519 for both

4. **Workload networking?**
   - NAT workloads behind node IP
   - Bridge workloads to mesh directly
   - Recommendation: NAT by default, bridge for trusted workloads
