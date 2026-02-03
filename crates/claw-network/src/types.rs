//! Core types for the `WireGuard` mesh network.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use chrono::{DateTime, Utc};
use ipnet::{IpNet, Ipv4Net};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a mesh node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(Uuid);

impl NodeId {
    /// Creates a new random `NodeId`.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates a `NodeId` from a UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the underlying UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Geographic or logical region for a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Region {
    /// US West Coast (10.100.2.0/20)
    UsWest,
    /// US East Coast (10.100.18.0/20)
    UsEast,
    /// Europe West (10.100.34.0/20)
    EuWest,
    /// Asia Pacific (10.100.50.0/20)
    Asia,
    /// MOLT marketplace providers (10.100.128.0/17)
    Molt,
    /// Gateway/control plane (10.100.0.0/24)
    Gateway,
}

impl Region {
    /// Returns all regions except Gateway.
    #[must_use]
    pub const fn all_compute() -> &'static [Region] {
        &[
            Region::UsWest,
            Region::UsEast,
            Region::EuWest,
            Region::Asia,
            Region::Molt,
        ]
    }
}

impl std::fmt::Display for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Region::UsWest => write!(f, "us-west"),
            Region::UsEast => write!(f, "us-east"),
            Region::EuWest => write!(f, "eu-west"),
            Region::Asia => write!(f, "asia"),
            Region::Molt => write!(f, "molt"),
            Region::Gateway => write!(f, "gateway"),
        }
    }
}

/// `WireGuard` public key (base64-encoded 32-byte key).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WireGuardKey(String);

impl WireGuardKey {
    /// Creates a new `WireGuardKey` from a base64-encoded string.
    ///
    /// # Errors
    ///
    /// Returns an error if the key is not valid base64 or not 32 bytes.
    pub fn new(key: impl Into<String>) -> Result<Self, WireGuardKeyError> {
        use base64::Engine;
        let key = key.into();

        // Validate base64 and length
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&key)
            .map_err(|_| WireGuardKeyError::InvalidBase64)?;

        if decoded.len() != 32 {
            return Err(WireGuardKeyError::InvalidLength {
                expected: 32,
                actual: decoded.len(),
            });
        }

        Ok(Self(key))
    }

    /// Returns the base64-encoded key string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Errors that can occur when creating a `WireGuardKey`.
#[derive(Debug, Clone, thiserror::Error)]
pub enum WireGuardKeyError {
    /// The key is not valid base64.
    #[error("invalid base64 encoding")]
    InvalidBase64,
    /// The key is not 32 bytes.
    #[error("invalid key length: expected {expected}, got {actual}")]
    InvalidLength {
        /// Expected length.
        expected: usize,
        /// Actual length.
        actual: usize,
    },
}

/// A node in the `WireGuard` mesh network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshNode {
    /// Unique identifier for this node.
    pub node_id: NodeId,
    /// IP address within the mesh network.
    pub mesh_ip: IpAddr,
    /// Subnet allocated for workloads on this node.
    pub workload_subnet: IpNet,
    /// `WireGuard` public key.
    pub wireguard_key: WireGuardKey,
    /// External endpoint for `WireGuard` connections.
    pub endpoint: Option<SocketAddr>,
    /// Region this node belongs to.
    pub region: Region,
    /// When this node was added to the mesh.
    pub joined_at: DateTime<Utc>,
    /// When this node was last seen.
    pub last_seen: DateTime<Utc>,
}

impl MeshNode {
    /// Creates a new `MeshNodeBuilder`.
    #[must_use]
    pub fn builder() -> MeshNodeBuilder {
        MeshNodeBuilder::default()
    }
}

/// Builder for `MeshNode`.
#[derive(Debug, Default)]
pub struct MeshNodeBuilder {
    node_id: Option<NodeId>,
    mesh_ip: Option<IpAddr>,
    workload_subnet: Option<IpNet>,
    wireguard_key: Option<WireGuardKey>,
    endpoint: Option<SocketAddr>,
    region: Option<Region>,
}

impl MeshNodeBuilder {
    /// Sets the node ID.
    #[must_use]
    pub fn node_id(mut self, id: NodeId) -> Self {
        self.node_id = Some(id);
        self
    }

    /// Sets the mesh IP address.
    #[must_use]
    pub fn mesh_ip(mut self, ip: IpAddr) -> Self {
        self.mesh_ip = Some(ip);
        self
    }

    /// Sets the workload subnet.
    #[must_use]
    pub fn workload_subnet(mut self, subnet: IpNet) -> Self {
        self.workload_subnet = Some(subnet);
        self
    }

    /// Sets the `WireGuard` key.
    #[must_use]
    pub fn wireguard_key(mut self, key: WireGuardKey) -> Self {
        self.wireguard_key = Some(key);
        self
    }

    /// Sets the external endpoint.
    #[must_use]
    pub fn endpoint(mut self, endpoint: SocketAddr) -> Self {
        self.endpoint = Some(endpoint);
        self
    }

    /// Sets the region.
    #[must_use]
    pub fn region(mut self, region: Region) -> Self {
        self.region = Some(region);
        self
    }

    /// Builds the `MeshNode`.
    ///
    /// # Errors
    ///
    /// Returns an error if required fields are missing.
    pub fn build(self) -> Result<MeshNode, MeshNodeBuilderError> {
        let now = Utc::now();

        Ok(MeshNode {
            node_id: self.node_id.unwrap_or_default(),
            mesh_ip: self.mesh_ip.ok_or(MeshNodeBuilderError::MissingMeshIp)?,
            workload_subnet: self
                .workload_subnet
                .ok_or(MeshNodeBuilderError::MissingWorkloadSubnet)?,
            wireguard_key: self
                .wireguard_key
                .ok_or(MeshNodeBuilderError::MissingWireGuardKey)?,
            endpoint: self.endpoint,
            region: self.region.ok_or(MeshNodeBuilderError::MissingRegion)?,
            joined_at: now,
            last_seen: now,
        })
    }
}

/// Errors that can occur when building a `MeshNode`.
#[derive(Debug, Clone, thiserror::Error)]
pub enum MeshNodeBuilderError {
    /// Missing mesh IP address.
    #[error("missing mesh IP address")]
    MissingMeshIp,
    /// Missing workload subnet.
    #[error("missing workload subnet")]
    MissingWorkloadSubnet,
    /// Missing `WireGuard` key.
    #[error("missing WireGuard key")]
    MissingWireGuardKey,
    /// Missing region.
    #[error("missing region")]
    MissingRegion,
}

/// The complete topology of the mesh network.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MeshTopology {
    /// All nodes in the mesh, indexed by their ID.
    pub nodes: HashMap<NodeId, MeshNode>,
    /// The gateway node ID, if present.
    pub gateway: Option<NodeId>,
}

impl MeshTopology {
    /// Creates a new empty topology.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of nodes in the mesh.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns an iterator over all nodes.
    pub fn iter(&self) -> impl Iterator<Item = (&NodeId, &MeshNode)> {
        self.nodes.iter()
    }

    /// Returns nodes in a specific region.
    pub fn nodes_in_region(&self, region: Region) -> impl Iterator<Item = &MeshNode> {
        self.nodes.values().filter(move |n| n.region == region)
    }
}

/// Configuration for the mesh network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshConfig {
    /// Base CIDR for the mesh network.
    pub mesh_cidr: IpNet,
    /// Base CIDR for workload subnets.
    pub workload_base_cidr: IpNet,
    /// Gateway endpoint address.
    pub gateway_endpoint: Option<SocketAddr>,
    /// `WireGuard` listen port.
    pub wireguard_port: u16,
}

impl MeshConfig {
    /// Default mesh CIDR: 10.100.0.0/16
    const DEFAULT_MESH_CIDR: Ipv4Net = Ipv4Net::new_assert(Ipv4Addr::new(10, 100, 0, 0), 16);

    /// Default workload base CIDR: 10.200.0.0/16
    const DEFAULT_WORKLOAD_CIDR: Ipv4Net = Ipv4Net::new_assert(Ipv4Addr::new(10, 200, 0, 0), 16);
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            mesh_cidr: IpNet::V4(MeshConfig::DEFAULT_MESH_CIDR),
            workload_base_cidr: IpNet::V4(MeshConfig::DEFAULT_WORKLOAD_CIDR),
            gateway_endpoint: None,
            wireguard_port: 51820,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_generates_unique_ids() {
        let id1 = NodeId::new();
        let id2 = NodeId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn node_id_roundtrips_through_uuid() {
        let uuid = Uuid::new_v4();
        let id = NodeId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn wireguard_key_validates_base64() {
        // Valid 32-byte key in base64
        let valid_key = "YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=";
        assert!(WireGuardKey::new(valid_key).is_ok());

        // Invalid base64
        let invalid_base64 = "not valid base64!!!";
        assert!(matches!(
            WireGuardKey::new(invalid_base64),
            Err(WireGuardKeyError::InvalidBase64)
        ));

        // Wrong length
        let wrong_length = "YWFh"; // 3 bytes
        assert!(matches!(
            WireGuardKey::new(wrong_length),
            Err(WireGuardKeyError::InvalidLength { .. })
        ));
    }

    #[test]
    fn mesh_node_builder_requires_fields() {
        let result = MeshNode::builder().build();
        assert!(result.is_err());
    }

    #[test]
    fn mesh_node_builder_creates_node() {
        let key = WireGuardKey::new("YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=")
            .expect("valid test key");

        let node = MeshNode::builder()
            .mesh_ip("10.100.2.1".parse().expect("valid IP"))
            .workload_subnet("10.200.1.0/24".parse().expect("valid subnet"))
            .wireguard_key(key)
            .region(Region::UsWest)
            .build()
            .expect("should build");

        assert_eq!(
            node.mesh_ip,
            "10.100.2.1".parse::<IpAddr>().expect("valid IP")
        );
        assert_eq!(node.region, Region::UsWest);
    }

    #[test]
    fn mesh_topology_counts_nodes() {
        let mut topology = MeshTopology::new();
        assert_eq!(topology.node_count(), 0);

        let key = WireGuardKey::new("YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=")
            .expect("valid test key");

        let node = MeshNode::builder()
            .mesh_ip("10.100.2.1".parse().expect("valid IP"))
            .workload_subnet("10.200.1.0/24".parse().expect("valid subnet"))
            .wireguard_key(key)
            .region(Region::UsWest)
            .build()
            .expect("should build");

        topology.nodes.insert(node.node_id, node);
        assert_eq!(topology.node_count(), 1);
    }

    #[test]
    fn region_display_formats_correctly() {
        assert_eq!(format!("{}", Region::UsWest), "us-west");
        assert_eq!(format!("{}", Region::UsEast), "us-east");
        assert_eq!(format!("{}", Region::EuWest), "eu-west");
        assert_eq!(format!("{}", Region::Asia), "asia");
        assert_eq!(format!("{}", Region::Molt), "molt");
        assert_eq!(format!("{}", Region::Gateway), "gateway");
    }
}
