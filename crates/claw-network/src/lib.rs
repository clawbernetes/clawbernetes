//! WireGuard mesh networking, IP allocation, and topology for Clawbernetes.
//!
//! Provides mesh node registration, regional IP allocation, workload subnet
//! assignment, and topology management for the WireGuard overlay network.

#![forbid(unsafe_code)]

use ipnet::{IpNet, Ipv4Net};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

// ─────────────────────────────────────────────────────────────
// Node Identity
// ─────────────────────────────────────────────────────────────

/// A unique node identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(u64);

impl NodeId {
    /// Create a new random node ID.
    pub fn new() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Self((nanos as u64) ^ rand_u64())
    }

    /// Create from a raw value (for testing).
    pub fn from_raw(val: u64) -> Self {
        Self(val)
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node-{:016x}", self.0)
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple non-crypto random u64.
fn rand_u64() -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    std::thread::current().id().hash(&mut hasher);
    std::time::Instant::now().hash(&mut hasher);
    hasher.finish()
}

// ─────────────────────────────────────────────────────────────
// Region
// ─────────────────────────────────────────────────────────────

/// Geographic region for IP allocation and topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Region {
    /// US West Coast.
    UsWest,
    /// US East Coast.
    UsEast,
    /// EU West.
    EuWest,
    /// Asia Pacific.
    Asia,
    /// MOLT marketplace.
    Molt,
    /// Gateway / control plane.
    Gateway,
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UsWest => write!(f, "us-west"),
            Self::UsEast => write!(f, "us-east"),
            Self::EuWest => write!(f, "eu-west"),
            Self::Asia => write!(f, "asia"),
            Self::Molt => write!(f, "molt"),
            Self::Gateway => write!(f, "gateway"),
        }
    }
}

// ─────────────────────────────────────────────────────────────
// WireGuard Key
// ─────────────────────────────────────────────────────────────

/// A WireGuard public key (base64-encoded 32 bytes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireGuardKey(String);

impl WireGuardKey {
    /// Create from a base64-encoded public key string.
    pub fn new(key: &str) -> Result<Self, String> {
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(key)
            .map_err(|e| format!("invalid base64: {e}"))?;
        if decoded.len() != 32 {
            return Err(format!("expected 32 bytes, got {}", decoded.len()));
        }
        Ok(Self(key.to_string()))
    }

    /// Get the base64-encoded key string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WireGuardKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ─────────────────────────────────────────────────────────────
// Mesh Node
// ─────────────────────────────────────────────────────────────

/// A node registered in the mesh topology.
#[derive(Debug, Clone)]
pub struct MeshNode {
    /// Node identifier.
    pub node_id: NodeId,
    /// Mesh overlay IP address.
    pub mesh_ip: IpAddr,
    /// Workload subnet assigned to this node.
    pub workload_subnet: IpNet,
    /// WireGuard public key.
    pub wireguard_key: WireGuardKey,
    /// Geographic region.
    pub region: Region,
    /// WireGuard endpoint (external IP:port).
    pub endpoint: Option<std::net::SocketAddr>,
}

/// Builder for [`MeshNode`].
pub struct MeshNodeBuilder {
    node_id: Option<NodeId>,
    mesh_ip: Option<IpAddr>,
    workload_subnet: Option<IpNet>,
    wireguard_key: Option<WireGuardKey>,
    region: Option<Region>,
    endpoint: Option<std::net::SocketAddr>,
}

impl MeshNodeBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            node_id: None,
            mesh_ip: None,
            workload_subnet: None,
            wireguard_key: None,
            region: None,
            endpoint: None,
        }
    }

    /// Set the node ID.
    pub fn node_id(mut self, id: NodeId) -> Self {
        self.node_id = Some(id);
        self
    }

    /// Set the mesh IP.
    pub fn mesh_ip(mut self, ip: IpAddr) -> Self {
        self.mesh_ip = Some(ip);
        self
    }

    /// Set the workload subnet.
    pub fn workload_subnet(mut self, subnet: IpNet) -> Self {
        self.workload_subnet = Some(subnet);
        self
    }

    /// Set the WireGuard key.
    pub fn wireguard_key(mut self, key: WireGuardKey) -> Self {
        self.wireguard_key = Some(key);
        self
    }

    /// Set the region.
    pub fn region(mut self, region: Region) -> Self {
        self.region = Some(region);
        self
    }

    /// Set the endpoint.
    pub fn endpoint(mut self, endpoint: std::net::SocketAddr) -> Self {
        self.endpoint = Some(endpoint);
        self
    }

    /// Build the mesh node.
    pub fn build(self) -> Result<MeshNode, String> {
        Ok(MeshNode {
            node_id: self.node_id.ok_or("node_id required")?,
            mesh_ip: self.mesh_ip.ok_or("mesh_ip required")?,
            workload_subnet: self.workload_subnet.ok_or("workload_subnet required")?,
            wireguard_key: self.wireguard_key.ok_or("wireguard_key required")?,
            region: self.region.ok_or("region required")?,
            endpoint: self.endpoint,
        })
    }
}

impl Default for MeshNodeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MeshNode {
    /// Create a builder for MeshNode.
    pub fn builder() -> MeshNodeBuilder {
        MeshNodeBuilder::new()
    }
}

// ─────────────────────────────────────────────────────────────
// IP Allocator
// ─────────────────────────────────────────────────────────────

/// Allocates mesh IPs and workload subnets from regional pools.
pub struct IpAllocator {
    region_counters: Mutex<HashMap<Region, u32>>,
    workload_counter: Mutex<u32>,
}

impl IpAllocator {
    /// Create a new IP allocator.
    pub fn new() -> Self {
        Self {
            region_counters: Mutex::new(HashMap::new()),
            workload_counter: Mutex::new(0),
        }
    }

    /// Allocate a mesh IP for a node in the given region.
    ///
    /// Regional pools:
    /// - Gateway: 10.100.0.0/20
    /// - UsWest:  10.100.16.0/20
    /// - UsEast:  10.100.32.0/20
    /// - EuWest:  10.100.48.0/20
    /// - Asia:    10.100.64.0/20
    /// - Molt:    10.100.80.0/20
    pub fn allocate_node_ip(&self, region: Region) -> Result<IpAddr, String> {
        let base = match region {
            Region::Gateway => [10, 100, 0, 0],
            Region::UsWest => [10, 100, 16, 0],
            Region::UsEast => [10, 100, 32, 0],
            Region::EuWest => [10, 100, 48, 0],
            Region::Asia => [10, 100, 64, 0],
            Region::Molt => [10, 100, 80, 0],
        };

        let mut counters = self.region_counters.lock().map_err(|_| "lock poisoned")?;
        let counter = counters.entry(region).or_insert(1);
        if *counter >= 4094 {
            return Err(format!("region {region} IP pool exhausted"));
        }

        let offset = *counter;
        *counter += 1;

        let ip = std::net::Ipv4Addr::new(
            base[0],
            base[1],
            base[2] + ((offset >> 8) as u8),
            (offset & 0xFF) as u8,
        );

        Ok(IpAddr::V4(ip))
    }

    /// Allocate a /24 workload subnet from the 10.200.0.0/16 pool.
    pub fn allocate_workload_subnet(&self, _node_id: NodeId) -> Result<IpNet, String> {
        let mut counter = self.workload_counter.lock().map_err(|_| "lock poisoned")?;
        if *counter >= 255 {
            return Err("workload subnet pool exhausted".to_string());
        }

        let third_octet = *counter as u8;
        *counter += 1;

        let subnet: Ipv4Net = format!("10.200.{third_octet}.0/24")
            .parse()
            .map_err(|e| format!("subnet parse error: {e}"))?;

        Ok(IpNet::V4(subnet))
    }

    /// Get allocator statistics.
    pub fn stats(&self) -> AllocatorStats {
        let regions = self
            .region_counters
            .lock()
            .map(|c| c.iter().map(|(r, n)| (r.to_string(), *n)).collect())
            .unwrap_or_default();
        let workload_subnets_allocated = self
            .workload_counter
            .lock()
            .map(|c| *c)
            .unwrap_or(0);
        AllocatorStats {
            regions,
            workload_subnets_allocated,
        }
    }
}

impl Default for IpAllocator {
    fn default() -> Self {
        Self::new()
    }
}

/// Allocator statistics snapshot.
#[derive(Debug, Clone)]
pub struct AllocatorStats {
    /// Per-region allocation counts.
    pub regions: HashMap<String, u32>,
    /// Total workload subnets allocated.
    pub workload_subnets_allocated: u32,
}

// ─────────────────────────────────────────────────────────────
// Mesh Config & WireGuardMesh
// ─────────────────────────────────────────────────────────────

/// Configuration for the WireGuard mesh.
#[derive(Debug, Clone)]
pub struct MeshConfig {
    /// Mesh CIDR block.
    pub mesh_cidr: String,
    /// Workload CIDR block.
    pub workload_cidr: String,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            mesh_cidr: "10.100.0.0/16".to_string(),
            workload_cidr: "10.200.0.0/16".to_string(),
        }
    }
}

/// Topology snapshot.
#[derive(Debug, Clone)]
pub struct MeshTopology {
    /// All registered nodes.
    pub nodes: HashMap<NodeId, MeshNode>,
}

/// The WireGuard mesh topology manager.
pub struct WireGuardMesh {
    #[allow(dead_code)]
    config: MeshConfig,
    allocator: Arc<IpAllocator>,
    nodes: Arc<Mutex<HashMap<NodeId, MeshNode>>>,
}

impl WireGuardMesh {
    /// Create a new mesh with the given configuration.
    pub fn new(config: MeshConfig) -> Result<Self, String> {
        Ok(Self {
            config,
            allocator: Arc::new(IpAllocator::new()),
            nodes: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Get the IP allocator.
    pub fn allocator(&self) -> &IpAllocator {
        &self.allocator
    }

    /// Register a node in the mesh.
    pub fn add_node(&self, node: MeshNode) -> Result<(), String> {
        let mut nodes = self.nodes.lock().map_err(|_| "lock poisoned")?;
        if nodes.contains_key(&node.node_id) {
            return Err(format!("node {} already registered", node.node_id));
        }
        nodes.insert(node.node_id, node);
        Ok(())
    }

    /// Remove a node from the mesh.
    pub fn remove_node(&self, node_id: NodeId) -> Result<(), String> {
        let mut nodes = self.nodes.lock().map_err(|_| "lock poisoned")?;
        nodes
            .remove(&node_id)
            .ok_or_else(|| format!("node {node_id} not found"))?;
        Ok(())
    }

    /// Get a node by ID.
    pub fn get_node(&self, node_id: NodeId) -> Option<MeshNode> {
        let nodes = self.nodes.lock().ok()?;
        nodes.get(&node_id).cloned()
    }

    /// Get the number of registered nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.lock().map(|n| n.len()).unwrap_or(0)
    }

    /// List all registered nodes.
    pub fn list_nodes(&self) -> Vec<MeshNode> {
        self.nodes
            .lock()
            .map(|n| n.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Get a snapshot of the mesh topology.
    pub fn get_topology(&self) -> MeshTopology {
        let nodes = self
            .nodes
            .lock()
            .map(|n| n.clone())
            .unwrap_or_default();
        MeshTopology { nodes }
    }
}

impl fmt::Debug for WireGuardMesh {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WireGuardMesh")
            .field("config", &self.config)
            .field("node_count", &self.node_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pubkey() -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode([1u8; 32])
    }

    #[test]
    fn test_node_id_unique() {
        let a = NodeId::new();
        let b = NodeId::new();
        assert_ne!(format!("{a}"), format!("{b}"));
    }

    #[test]
    fn test_node_id_display() {
        let id = NodeId::from_raw(0xDEADBEEF);
        assert!(id.to_string().starts_with("node-"));
    }

    #[test]
    fn test_node_id_default() {
        let id = NodeId::default();
        assert!(id.to_string().starts_with("node-"));
    }

    #[test]
    fn test_region_display() {
        assert_eq!(Region::UsWest.to_string(), "us-west");
        assert_eq!(Region::UsEast.to_string(), "us-east");
        assert_eq!(Region::EuWest.to_string(), "eu-west");
        assert_eq!(Region::Asia.to_string(), "asia");
        assert_eq!(Region::Molt.to_string(), "molt");
        assert_eq!(Region::Gateway.to_string(), "gateway");
    }

    #[test]
    fn test_wireguard_key_valid() {
        assert!(WireGuardKey::new(&test_pubkey()).is_ok());
    }

    #[test]
    fn test_wireguard_key_invalid_base64() {
        assert!(WireGuardKey::new("not-base64!!!").is_err());
    }

    #[test]
    fn test_wireguard_key_wrong_length() {
        use base64::Engine;
        let short = base64::engine::general_purpose::STANDARD.encode([1u8; 16]);
        assert!(WireGuardKey::new(&short).is_err());
    }

    #[test]
    fn test_wireguard_key_display() {
        let key = WireGuardKey::new(&test_pubkey()).expect("key");
        assert_eq!(key.to_string(), key.as_str());
    }

    #[test]
    fn test_ip_allocator_node_ips() {
        let alloc = IpAllocator::new();
        let ip1 = alloc.allocate_node_ip(Region::UsWest).expect("alloc 1");
        let ip2 = alloc.allocate_node_ip(Region::UsWest).expect("alloc 2");
        assert_ne!(ip1, ip2);
        assert!(ip1.to_string().starts_with("10.100.16."));
    }

    #[test]
    fn test_ip_allocator_different_regions() {
        let alloc = IpAllocator::new();
        let west = alloc.allocate_node_ip(Region::UsWest).expect("west");
        let east = alloc.allocate_node_ip(Region::UsEast).expect("east");
        assert!(west.to_string().starts_with("10.100.16."));
        assert!(east.to_string().starts_with("10.100.32."));
    }

    #[test]
    fn test_ip_allocator_gateway_region() {
        let alloc = IpAllocator::new();
        let ip = alloc.allocate_node_ip(Region::Gateway).expect("gw");
        assert!(ip.to_string().starts_with("10.100.0."));
    }

    #[test]
    fn test_ip_allocator_workload_subnets() {
        let alloc = IpAllocator::new();
        let s1 = alloc.allocate_workload_subnet(NodeId::new()).expect("subnet 1");
        let s2 = alloc.allocate_workload_subnet(NodeId::new()).expect("subnet 2");
        assert_ne!(s1, s2);
        assert!(s1.to_string().starts_with("10.200."));
        assert!(s1.to_string().ends_with("/24"));
    }

    #[test]
    fn test_ip_allocator_stats() {
        let alloc = IpAllocator::new();
        alloc.allocate_node_ip(Region::UsWest).expect("ip");
        alloc.allocate_node_ip(Region::UsWest).expect("ip");
        alloc.allocate_workload_subnet(NodeId::new()).expect("subnet");

        let stats = alloc.stats();
        assert_eq!(stats.regions.get("us-west"), Some(&3)); // counter starts at 1, 2 allocs -> 3
        assert_eq!(stats.workload_subnets_allocated, 1);
    }

    #[test]
    fn test_mesh_node_builder() {
        let node = MeshNode::builder()
            .node_id(NodeId::new())
            .mesh_ip("10.100.16.1".parse().unwrap())
            .workload_subnet("10.200.0.0/24".parse().unwrap())
            .wireguard_key(WireGuardKey::new(&test_pubkey()).unwrap())
            .region(Region::UsWest)
            .build()
            .expect("build");
        assert_eq!(node.region, Region::UsWest);
        assert!(node.endpoint.is_none());
    }

    #[test]
    fn test_mesh_node_builder_with_endpoint() {
        let node = MeshNode::builder()
            .node_id(NodeId::new())
            .mesh_ip("10.100.16.1".parse().unwrap())
            .workload_subnet("10.200.0.0/24".parse().unwrap())
            .wireguard_key(WireGuardKey::new(&test_pubkey()).unwrap())
            .region(Region::UsWest)
            .endpoint("1.2.3.4:51820".parse().unwrap())
            .build()
            .expect("build");
        assert!(node.endpoint.is_some());
    }

    #[test]
    fn test_mesh_node_builder_missing_field() {
        assert!(MeshNode::builder().node_id(NodeId::new()).build().is_err());
    }

    #[test]
    fn test_mesh_config_default() {
        let cfg = MeshConfig::default();
        assert_eq!(cfg.mesh_cidr, "10.100.0.0/16");
        assert_eq!(cfg.workload_cidr, "10.200.0.0/16");
    }

    #[test]
    fn test_wireguard_mesh_lifecycle() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("mesh");
        let alloc = mesh.allocator();
        let id = NodeId::new();
        let ip = alloc.allocate_node_ip(Region::UsWest).expect("ip");
        let subnet = alloc.allocate_workload_subnet(id).expect("subnet");

        let node = MeshNode::builder()
            .node_id(id)
            .mesh_ip(ip)
            .workload_subnet(subnet)
            .wireguard_key(WireGuardKey::new(&test_pubkey()).unwrap())
            .region(Region::UsWest)
            .build()
            .expect("build");

        mesh.add_node(node).expect("add");
        assert_eq!(mesh.node_count(), 1);

        let fetched = mesh.get_node(id).expect("get");
        assert_eq!(fetched.mesh_ip, ip);

        mesh.remove_node(id).expect("remove");
        assert_eq!(mesh.node_count(), 0);
        assert!(mesh.get_node(id).is_none());
    }

    #[test]
    fn test_wireguard_mesh_duplicate_node() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("mesh");
        let alloc = mesh.allocator();
        let id = NodeId::new();

        let make_node = || {
            let ip = alloc.allocate_node_ip(Region::UsWest).expect("ip");
            let subnet = alloc.allocate_workload_subnet(NodeId::new()).expect("subnet");
            MeshNode::builder()
                .node_id(id)
                .mesh_ip(ip)
                .workload_subnet(subnet)
                .wireguard_key(WireGuardKey::new(&test_pubkey()).unwrap())
                .region(Region::UsWest)
                .build()
                .expect("build")
        };

        mesh.add_node(make_node()).expect("first add");
        assert!(mesh.add_node(make_node()).is_err());
    }

    #[test]
    fn test_wireguard_mesh_remove_not_found() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("mesh");
        assert!(mesh.remove_node(NodeId::new()).is_err());
    }

    #[test]
    fn test_wireguard_mesh_list_and_topology() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("mesh");
        let alloc = mesh.allocator();

        for _ in 0..3 {
            let id = NodeId::new();
            let ip = alloc.allocate_node_ip(Region::UsWest).expect("ip");
            let subnet = alloc.allocate_workload_subnet(id).expect("subnet");
            mesh.add_node(
                MeshNode::builder()
                    .node_id(id)
                    .mesh_ip(ip)
                    .workload_subnet(subnet)
                    .wireguard_key(WireGuardKey::new(&test_pubkey()).unwrap())
                    .region(Region::UsWest)
                    .build()
                    .expect("build"),
            )
            .expect("add");
        }

        assert_eq!(mesh.list_nodes().len(), 3);
        assert_eq!(mesh.get_topology().nodes.len(), 3);
    }

    #[test]
    fn test_wireguard_mesh_debug() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("mesh");
        let debug = format!("{mesh:?}");
        assert!(debug.contains("WireGuardMesh"));
        assert!(debug.contains("node_count"));
    }
}
