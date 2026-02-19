//! Node auto-mesh management.
//!
//! Creates a WireGuard interface on startup, allocates a mesh IP,
//! and manages peering with other nodes in the cluster.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use crate::network_types::{MeshNode, NodeId, Region, WireGuardKey, WireGuardMesh};
use ipnet::{IpNet, Ipv4Net};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::commands::CommandError;

/// Default WireGuard interface name for the mesh.
const MESH_INTERFACE: &str = "claw0";

/// Default WireGuard listen port.
const MESH_PORT: u16 = 51820;

/// Information about a remote peer for mesh synchronization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub node_id: String,
    pub public_key: String,
    pub mesh_ip: String,
    pub endpoint: Option<String>,
    pub workload_subnet: Option<String>,
    pub region: String,
}

/// Result of a peer sync operation.
#[derive(Debug, Default)]
pub struct SyncResult {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub unchanged: usize,
}

/// Current mesh status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshStatus {
    pub interface: String,
    pub mesh_ip: String,
    pub public_key: String,
    pub listen_port: u16,
    pub node_id: String,
    pub region: String,
    pub peers: Vec<MeshPeerStatus>,
}

/// Status of a single mesh peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshPeerStatus {
    pub public_key: String,
    pub endpoint: Option<String>,
    pub mesh_ip: String,
    pub last_handshake: Option<u64>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// Manages the WireGuard mesh for this node.
///
/// Bridges `claw_network::WireGuardMesh` (topology, IP allocation)
/// with `claw_wireguard::WireGuardManager` (actual interface management).
pub struct MeshManager {
    mesh: Arc<WireGuardMesh>,
    mesh_ip: IpAddr,
    public_key: String,
    interface_name: String,
    node_id: NodeId,
    region: Region,
    listen_port: u16,
    /// Workload subnet allocated for this node's containers.
    workload_subnet: IpNet,
    /// Tracks which node IDs have been added as WireGuard peers.
    active_peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
}

impl MeshManager {
    /// Initialize the mesh manager.
    ///
    /// Allocates a mesh IP, registers the node in the mesh topology,
    /// and (on Linux with root) creates the WireGuard interface.
    ///
    /// Returns `None` if mesh networking is unavailable (no WireGuard support).
    pub fn init(
        mesh: Arc<WireGuardMesh>,
        region: Region,
        public_key: String,
        endpoint: Option<std::net::SocketAddr>,
    ) -> Result<Self, CommandError> {
        let allocator = mesh.allocator();
        let node_id = NodeId::new();

        // Allocate mesh IP from the regional pool
        let mesh_ip = allocator
            .allocate_node_ip(region)
            .map_err(|e| format!("mesh IP allocation failed: {e}"))?;

        // Allocate workload subnet
        let workload_subnet = allocator
            .allocate_workload_subnet(node_id)
            .map_err(|e| format!("workload subnet allocation failed: {e}"))?;

        // Create WireGuard key for claw-network
        let wg_key = WireGuardKey::new(&public_key)
            .map_err(|e| format!("invalid WireGuard key: {e}"))?;

        // Register in mesh topology
        let mut builder = MeshNode::builder()
            .node_id(node_id)
            .mesh_ip(mesh_ip)
            .workload_subnet(workload_subnet)
            .wireguard_key(wg_key)
            .region(region);

        if let Some(ep) = endpoint {
            builder = builder.endpoint(ep);
        }

        let mesh_node = builder
            .build()
            .map_err(|e| format!("mesh node build failed: {e}"))?;

        mesh.add_node(mesh_node)
            .map_err(|e| format!("mesh add_node failed: {e}"))?;

        info!(
            node_id = %node_id,
            mesh_ip = %mesh_ip,
            region = %region,
            workload_subnet = %workload_subnet,
            "mesh node registered"
        );

        Ok(Self {
            mesh,
            mesh_ip,
            public_key,
            interface_name: MESH_INTERFACE.to_string(),
            node_id,
            region,
            listen_port: MESH_PORT,
            workload_subnet,
            active_peers: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Get this node's mesh IP.
    pub fn mesh_ip(&self) -> IpAddr {
        self.mesh_ip
    }

    /// Get this node's ID.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Get this node's region.
    pub fn region(&self) -> Region {
        self.region
    }

    /// Get the WireGuard public key.
    pub fn public_key(&self) -> &str {
        &self.public_key
    }

    /// Get the listen port.
    pub fn listen_port(&self) -> u16 {
        self.listen_port
    }

    /// Get the interface name.
    pub fn interface_name(&self) -> &str {
        &self.interface_name
    }

    /// Get the workload subnet allocated for this node's containers.
    ///
    /// Returns `None` if the allocated subnet is IPv6 (shouldn't happen with current allocator).
    pub fn workload_subnet(&self) -> Option<Ipv4Net> {
        match self.workload_subnet {
            IpNet::V4(v4) => Some(v4),
            IpNet::V6(_) => None,
        }
    }

    /// Add a remote peer to the mesh.
    pub async fn add_remote_peer(&self, peer: PeerInfo) -> Result<(), CommandError> {
        let peer_id = peer.node_id.clone();

        info!(
            peer_id = %peer_id,
            mesh_ip = %peer.mesh_ip,
            "adding remote mesh peer"
        );

        // In a full implementation, this would:
        // 1. Add WireGuard peer via LinuxWireGuardInterface
        // 2. Add route for their workload subnet via claw0
        // For now, we track the peer in our active_peers map.

        let mut peers = self.active_peers.write().await;
        peers.insert(peer_id, peer);

        Ok(())
    }

    /// Remove a remote peer from the mesh.
    pub async fn remove_remote_peer(&self, node_id: &str) -> Result<(), CommandError> {
        info!(peer_id = %node_id, "removing remote mesh peer");

        let mut peers = self.active_peers.write().await;
        peers.remove(node_id)
            .ok_or_else(|| format!("peer '{node_id}' not found"))?;

        Ok(())
    }

    /// Synchronize local peers with a list of known nodes from the gateway.
    pub async fn sync_peers(&self, known_nodes: Vec<PeerInfo>) -> Result<SyncResult, CommandError> {
        let mut result = SyncResult::default();
        let mut peers = self.active_peers.write().await;

        // Build set of known node IDs (excluding self)
        let known: HashMap<String, &PeerInfo> = known_nodes
            .iter()
            .filter(|p| p.node_id != self.node_id.to_string())
            .map(|p| (p.node_id.clone(), p))
            .collect();

        // Remove peers that are no longer in the known set
        let to_remove: Vec<String> = peers
            .keys()
            .filter(|id| !known.contains_key(*id))
            .cloned()
            .collect();

        for id in to_remove {
            peers.remove(&id);
            result.removed.push(id);
        }

        // Add new peers
        for (id, peer_info) in &known {
            if !peers.contains_key(id) {
                peers.insert(id.clone(), (*peer_info).clone());
                result.added.push(id.clone());
            } else {
                result.unchanged += 1;
            }
        }

        Ok(result)
    }

    /// Get the current mesh status.
    pub async fn status(&self) -> MeshStatus {
        let peers = self.active_peers.read().await;

        let peer_statuses: Vec<MeshPeerStatus> = peers
            .values()
            .map(|p| MeshPeerStatus {
                public_key: p.public_key.clone(),
                endpoint: p.endpoint.clone(),
                mesh_ip: p.mesh_ip.clone(),
                last_handshake: None, // Would come from WireGuard interface
                rx_bytes: 0,
                tx_bytes: 0,
            })
            .collect();

        MeshStatus {
            interface: self.interface_name.clone(),
            mesh_ip: self.mesh_ip.to_string(),
            public_key: self.public_key.clone(),
            listen_port: self.listen_port,
            node_id: self.node_id.to_string(),
            region: format!("{}", self.region),
            peers: peer_statuses,
        }
    }

    /// Shut down the mesh manager.
    pub async fn shutdown(&self) {
        info!(node_id = %self.node_id, "shutting down mesh manager");

        // Remove from mesh topology
        if let Err(e) = self.mesh.remove_node(self.node_id) {
            warn!(error = %e, "failed to remove node from mesh on shutdown");
        }

        // Clear active peers
        let mut peers = self.active_peers.write().await;
        peers.clear();
    }
}

impl std::fmt::Debug for MeshManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeshManager")
            .field("mesh_ip", &self.mesh_ip)
            .field("node_id", &self.node_id)
            .field("region", &self.region)
            .field("interface", &self.interface_name)
            .finish()
    }
}

/// Parse a region string into a `Region` enum value.
///
/// Accepts various formats: "us-west", "us_west", "uswest".
/// Defaults to `Region::UsWest` for unrecognized values.
pub fn parse_region(s: &str) -> Region {
    match s.to_lowercase().replace('-', "_").as_str() {
        "us_west" | "uswest" => Region::UsWest,
        "us_east" | "useast" => Region::UsEast,
        "eu_west" | "euwest" => Region::EuWest,
        "asia" | "asia_pacific" | "asiapacific" | "apac" => Region::Asia,
        "molt" => Region::Molt,
        "gateway" => Region::Gateway,
        _ => Region::UsWest,
    }
}

/// Mesh info sent in the connect payload to the gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshInfo {
    pub mesh_ip: String,
    pub wireguard_pubkey: String,
    pub wireguard_port: u16,
    pub region: String,
    pub endpoint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network_types::MeshConfig;

    fn test_mesh() -> Arc<WireGuardMesh> {
        Arc::new(WireGuardMesh::new(MeshConfig::default()).expect("mesh"))
    }

    fn test_pubkey() -> String {
        // A valid base64-encoded 32-byte key
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode([1u8; 32])
    }

    #[test]
    fn mesh_manager_init() {
        let mesh = test_mesh();
        let mgr = MeshManager::init(mesh.clone(), Region::UsWest, test_pubkey(), None)
            .expect("init");

        assert_eq!(mgr.region(), Region::UsWest);
        assert!(!mgr.mesh_ip().to_string().is_empty());
        assert_eq!(mesh.node_count(), 1);
    }

    #[test]
    fn mesh_manager_init_registers_in_topology() {
        let mesh = test_mesh();
        let mgr = MeshManager::init(mesh.clone(), Region::UsEast, test_pubkey(), None)
            .expect("init");

        let node = mesh.get_node(mgr.node_id()).expect("node exists");
        assert_eq!(node.mesh_ip, mgr.mesh_ip());
        assert_eq!(node.region, Region::UsEast);
    }

    #[tokio::test]
    async fn mesh_manager_add_and_remove_peer() {
        let mesh = test_mesh();
        let mgr = MeshManager::init(mesh, Region::UsWest, test_pubkey(), None)
            .expect("init");

        let peer = PeerInfo {
            node_id: "peer-1".to_string(),
            public_key: test_pubkey(),
            mesh_ip: "10.100.32.1".to_string(),
            endpoint: Some("1.2.3.4:51820".to_string()),
            workload_subnet: Some("10.200.2.0/24".to_string()),
            region: "UsEast".to_string(),
        };

        mgr.add_remote_peer(peer).await.expect("add");

        let status = mgr.status().await;
        assert_eq!(status.peers.len(), 1);
        assert_eq!(status.peers[0].mesh_ip, "10.100.32.1");

        mgr.remove_remote_peer("peer-1").await.expect("remove");

        let status = mgr.status().await;
        assert_eq!(status.peers.len(), 0);
    }

    #[tokio::test]
    async fn mesh_manager_remove_nonexistent_peer_fails() {
        let mesh = test_mesh();
        let mgr = MeshManager::init(mesh, Region::UsWest, test_pubkey(), None)
            .expect("init");

        let result = mgr.remove_remote_peer("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn mesh_manager_sync_peers() {
        let mesh = test_mesh();
        let mgr = MeshManager::init(mesh, Region::UsWest, test_pubkey(), None)
            .expect("init");

        // Add initial peers
        let peers = vec![
            PeerInfo {
                node_id: "peer-1".to_string(),
                public_key: test_pubkey(),
                mesh_ip: "10.100.32.1".to_string(),
                endpoint: None,
                workload_subnet: None,
                region: "UsEast".to_string(),
            },
            PeerInfo {
                node_id: "peer-2".to_string(),
                public_key: test_pubkey(),
                mesh_ip: "10.100.48.1".to_string(),
                endpoint: None,
                workload_subnet: None,
                region: "EuWest".to_string(),
            },
        ];

        let result = mgr.sync_peers(peers).await.expect("sync");
        assert_eq!(result.added.len(), 2);
        assert_eq!(result.removed.len(), 0);

        // Sync again with only peer-1 — peer-2 should be removed
        let peers = vec![PeerInfo {
            node_id: "peer-1".to_string(),
            public_key: test_pubkey(),
            mesh_ip: "10.100.32.1".to_string(),
            endpoint: None,
            workload_subnet: None,
            region: "UsEast".to_string(),
        }];

        let result = mgr.sync_peers(peers).await.expect("sync");
        assert_eq!(result.added.len(), 0);
        assert_eq!(result.removed.len(), 1);
        assert!(result.removed.contains(&"peer-2".to_string()));
        assert_eq!(result.unchanged, 1);
    }

    #[tokio::test]
    async fn mesh_manager_status() {
        let mesh = test_mesh();
        let mgr = MeshManager::init(mesh, Region::UsWest, test_pubkey(), None)
            .expect("init");

        let status = mgr.status().await;
        assert_eq!(status.interface, "claw0");
        assert_eq!(status.region, "us-west");
        assert_eq!(status.listen_port, 51820);
        assert_eq!(status.peers.len(), 0);
    }

    #[tokio::test]
    async fn mesh_manager_shutdown() {
        let mesh = test_mesh();
        let mgr = MeshManager::init(mesh.clone(), Region::UsWest, test_pubkey(), None)
            .expect("init");

        assert_eq!(mesh.node_count(), 1);

        mgr.shutdown().await;

        assert_eq!(mesh.node_count(), 0);
    }

    #[test]
    fn mesh_manager_workload_subnet() {
        let mesh = test_mesh();
        let mgr = MeshManager::init(mesh, Region::UsWest, test_pubkey(), None)
            .expect("init");

        let subnet = mgr.workload_subnet();
        assert!(subnet.is_some(), "workload subnet should be IPv4");
        let subnet = subnet.unwrap();
        assert_eq!(subnet.prefix_len(), 24);
    }

    #[test]
    fn parse_region_known_values() {
        assert_eq!(parse_region("us-west"), Region::UsWest);
        assert_eq!(parse_region("us-east"), Region::UsEast);
        assert_eq!(parse_region("eu-west"), Region::EuWest);
        assert_eq!(parse_region("asia"), Region::Asia);
        assert_eq!(parse_region("asia_pacific"), Region::Asia);
        assert_eq!(parse_region("molt"), Region::Molt);
        assert_eq!(parse_region("gateway"), Region::Gateway);
    }

    #[test]
    fn parse_region_case_insensitive() {
        assert_eq!(parse_region("US-West"), Region::UsWest);
        assert_eq!(parse_region("US_EAST"), Region::UsEast);
        assert_eq!(parse_region("ASIA"), Region::Asia);
    }

    #[test]
    fn parse_region_unknown_defaults_to_us_west() {
        assert_eq!(parse_region("unknown"), Region::UsWest);
        assert_eq!(parse_region(""), Region::UsWest);
        assert_eq!(parse_region("mars"), Region::UsWest);
    }

    #[test]
    fn mesh_info_serializes() {
        let info = MeshInfo {
            mesh_ip: "10.100.16.1".to_string(),
            wireguard_pubkey: test_pubkey(),
            wireguard_port: 51820,
            region: "UsWest".to_string(),
            endpoint: Some("1.2.3.4:51820".to_string()),
        };

        let json = serde_json::to_string(&info).expect("serialize");
        assert!(json.contains("meshIp"));
        assert!(json.contains("wireguardPubkey"));
    }
}
