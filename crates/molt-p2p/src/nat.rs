//! NAT traversal for WireGuard connections.
//!
//! This module provides NAT traversal capabilities:
//! - STUN-based endpoint discovery
//! - UDP hole punching coordination
//! - Relay fallback when direct connection fails
//!
//! WireGuard handles most NAT cases automatically through its
//! persistent keepalive mechanism, but we still need to discover
//! our external endpoint for initial peer exchange.

use crate::error::P2pError;
use crate::protocol::PeerInfo;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::Duration;

/// Default STUN servers for endpoint discovery.
pub const DEFAULT_STUN_SERVERS: &[&str] = &[
    "stun.l.google.com:19302",
    "stun1.l.google.com:19302",
    "stun2.l.google.com:19302",
    "stun3.l.google.com:19302",
];

/// NAT type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatType {
    /// No NAT - public IP.
    None,
    /// Full cone NAT (endpoint independent mapping).
    FullCone,
    /// Restricted cone NAT (address restricted).
    RestrictedCone,
    /// Port restricted cone NAT.
    PortRestrictedCone,
    /// Symmetric NAT (different mapping per destination).
    Symmetric,
    /// Unknown NAT type.
    Unknown,
}

impl NatType {
    /// Returns true if direct P2P connection is likely to work.
    #[must_use]
    pub const fn supports_direct_connection(&self) -> bool {
        matches!(
            self,
            Self::None | Self::FullCone | Self::RestrictedCone | Self::PortRestrictedCone
        )
    }

    /// Returns true if a relay may be needed.
    #[must_use]
    pub const fn may_need_relay(&self) -> bool {
        matches!(self, Self::Symmetric | Self::Unknown)
    }
}

impl std::fmt::Display for NatType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::None => "No NAT",
            Self::FullCone => "Full Cone",
            Self::RestrictedCone => "Restricted Cone",
            Self::PortRestrictedCone => "Port Restricted Cone",
            Self::Symmetric => "Symmetric",
            Self::Unknown => "Unknown",
        };
        write!(f, "{s}")
    }
}

/// Result of endpoint discovery.
#[derive(Debug, Clone)]
pub struct EndpointDiscovery {
    /// Our external endpoint as seen by STUN servers.
    pub external_endpoint: SocketAddr,
    /// Local endpoint we're bound to.
    pub local_endpoint: SocketAddr,
    /// Detected NAT type.
    pub nat_type: NatType,
    /// STUN server that responded.
    pub stun_server: String,
}

/// NAT traversal manager.
///
/// Handles endpoint discovery and hole punching coordination.
#[derive(Debug)]
pub struct NatTraversal {
    /// STUN servers to query.
    stun_servers: Vec<SocketAddr>,
    /// Timeout for STUN requests.
    timeout: Duration,
    /// Cached external endpoint.
    cached_endpoint: Option<EndpointDiscovery>,
}

impl NatTraversal {
    /// Creates a new NAT traversal manager with the given STUN servers.
    ///
    /// # Errors
    ///
    /// Returns an error if no valid STUN server addresses are provided.
    pub fn new(stun_servers: Vec<SocketAddr>) -> Result<Self, P2pError> {
        if stun_servers.is_empty() {
            return Err(P2pError::Discovery(
                "At least one STUN server is required".to_string(),
            ));
        }

        Ok(Self {
            stun_servers,
            timeout: Duration::from_secs(5),
            cached_endpoint: None,
        })
    }

    /// Creates a NAT traversal manager with default STUN servers.
    ///
    /// # Errors
    ///
    /// Returns an error if DNS resolution fails for all default servers.
    pub fn with_defaults() -> Result<Self, P2pError> {
        let mut servers = Vec::new();

        for server in DEFAULT_STUN_SERVERS {
            // Try to resolve the server address
            if let Ok(addrs) = std::net::ToSocketAddrs::to_socket_addrs(server) {
                for addr in addrs {
                    servers.push(addr);
                    break; // Just take the first resolved address
                }
            }
        }

        if servers.is_empty() {
            return Err(P2pError::Discovery(
                "Failed to resolve any default STUN servers".to_string(),
            ));
        }

        Ok(Self {
            stun_servers: servers,
            timeout: Duration::from_secs(5),
            cached_endpoint: None,
        })
    }

    /// Sets the timeout for STUN requests.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Returns the configured STUN servers.
    #[must_use]
    pub fn stun_servers(&self) -> &[SocketAddr] {
        &self.stun_servers
    }

    /// Returns the cached endpoint discovery result.
    #[must_use]
    pub fn cached_endpoint(&self) -> Option<&EndpointDiscovery> {
        self.cached_endpoint.as_ref()
    }

    /// Discovers our external endpoint using STUN.
    ///
    /// This sends a STUN binding request to discover how our packets
    /// appear to external servers (our public IP:port).
    ///
    /// # Errors
    ///
    /// Returns an error if all STUN servers fail to respond.
    pub fn discover_external_endpoint(&mut self) -> Result<SocketAddr, P2pError> {
        // Bind to a random local port
        let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| {
            P2pError::Connection(format!("Failed to bind UDP socket: {e}"))
        })?;

        socket.set_read_timeout(Some(self.timeout)).map_err(|e| {
            P2pError::Connection(format!("Failed to set socket timeout: {e}"))
        })?;

        let local_addr = socket.local_addr().map_err(|e| {
            P2pError::Connection(format!("Failed to get local address: {e}"))
        })?;

        // Try each STUN server
        for server in &self.stun_servers {
            match self.query_stun_server(&socket, *server) {
                Ok(external_addr) => {
                    // Determine NAT type based on mapping
                    let nat_type = self.determine_nat_type(local_addr, external_addr);

                    self.cached_endpoint = Some(EndpointDiscovery {
                        external_endpoint: external_addr,
                        local_endpoint: local_addr,
                        nat_type,
                        stun_server: server.to_string(),
                    });

                    return Ok(external_addr);
                }
                Err(_) => continue, // Try next server
            }
        }

        Err(P2pError::Discovery(
            "All STUN servers failed to respond".to_string(),
        ))
    }

    /// Queries a single STUN server for our external endpoint.
    fn query_stun_server(
        &self,
        socket: &UdpSocket,
        server: SocketAddr,
    ) -> Result<SocketAddr, P2pError> {
        // STUN Binding Request (RFC 5389)
        // Message Type: Binding Request (0x0001)
        // Magic Cookie: 0x2112A442
        // Transaction ID: 12 random bytes
        let mut request = [0u8; 20];
        request[0] = 0x00; // Message Type high byte
        request[1] = 0x01; // Message Type low byte (Binding Request)
        request[2] = 0x00; // Message Length high byte
        request[3] = 0x00; // Message Length low byte (no attributes)
        // Magic Cookie
        request[4] = 0x21;
        request[5] = 0x12;
        request[6] = 0xa4;
        request[7] = 0x42;
        // Transaction ID (12 bytes) - use simple counter for now
        for (i, byte) in request[8..20].iter_mut().enumerate() {
            *byte = (i as u8).wrapping_add(1);
        }

        // Send request
        socket.send_to(&request, server).map_err(|e| {
            P2pError::Connection(format!("Failed to send STUN request: {e}"))
        })?;

        // Receive response
        let mut response = [0u8; 512];
        let (len, _) = socket.recv_from(&mut response).map_err(|e| {
            P2pError::Connection(format!("Failed to receive STUN response: {e}"))
        })?;

        // Parse STUN response
        self.parse_stun_response(&response[..len])
    }

    /// Parses a STUN binding response to extract the mapped address.
    fn parse_stun_response(&self, response: &[u8]) -> Result<SocketAddr, P2pError> {
        if response.len() < 20 {
            return Err(P2pError::Discovery("STUN response too short".to_string()));
        }

        // Check message type (should be Binding Success Response: 0x0101)
        if response[0] != 0x01 || response[1] != 0x01 {
            return Err(P2pError::Discovery(format!(
                "Unexpected STUN message type: {:02x}{:02x}",
                response[0], response[1]
            )));
        }

        // Verify magic cookie
        if response[4..8] != [0x21, 0x12, 0xa4, 0x42] {
            return Err(P2pError::Discovery("Invalid STUN magic cookie".to_string()));
        }

        // Parse message length
        let msg_len = u16::from_be_bytes([response[2], response[3]]) as usize;
        if response.len() < 20 + msg_len {
            return Err(P2pError::Discovery(
                "STUN response truncated".to_string(),
            ));
        }

        // Parse attributes to find XOR-MAPPED-ADDRESS (0x0020) or MAPPED-ADDRESS (0x0001)
        let mut offset = 20;
        while offset + 4 <= 20 + msg_len {
            let attr_type = u16::from_be_bytes([response[offset], response[offset + 1]]);
            let attr_len = u16::from_be_bytes([response[offset + 2], response[offset + 3]]) as usize;

            if offset + 4 + attr_len > response.len() {
                break;
            }

            match attr_type {
                0x0020 => {
                    // XOR-MAPPED-ADDRESS
                    return self.parse_xor_mapped_address(&response[offset + 4..offset + 4 + attr_len]);
                }
                0x0001 => {
                    // MAPPED-ADDRESS (legacy)
                    return self.parse_mapped_address(&response[offset + 4..offset + 4 + attr_len]);
                }
                _ => {}
            }

            // Move to next attribute (padded to 4 bytes)
            offset += 4 + ((attr_len + 3) & !3);
        }

        Err(P2pError::Discovery(
            "No mapped address in STUN response".to_string(),
        ))
    }

    /// Parses XOR-MAPPED-ADDRESS attribute.
    fn parse_xor_mapped_address(&self, data: &[u8]) -> Result<SocketAddr, P2pError> {
        if data.len() < 8 {
            return Err(P2pError::Discovery(
                "XOR-MAPPED-ADDRESS too short".to_string(),
            ));
        }

        let family = data[1];
        let xor_port = u16::from_be_bytes([data[2], data[3]]);
        let port = xor_port ^ 0x2112; // XOR with magic cookie high bits

        match family {
            0x01 => {
                // IPv4
                let xor_addr = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                let addr = xor_addr ^ 0x2112_a442; // XOR with magic cookie
                Ok(SocketAddr::V4(SocketAddrV4::new(
                    Ipv4Addr::from(addr),
                    port,
                )))
            }
            0x02 => {
                // IPv6 - not implemented for simplicity
                Err(P2pError::Discovery(
                    "IPv6 XOR-MAPPED-ADDRESS not supported".to_string(),
                ))
            }
            _ => Err(P2pError::Discovery(format!(
                "Unknown address family: {family}"
            ))),
        }
    }

    /// Parses MAPPED-ADDRESS attribute (legacy).
    fn parse_mapped_address(&self, data: &[u8]) -> Result<SocketAddr, P2pError> {
        if data.len() < 8 {
            return Err(P2pError::Discovery("MAPPED-ADDRESS too short".to_string()));
        }

        let family = data[1];
        let port = u16::from_be_bytes([data[2], data[3]]);

        match family {
            0x01 => {
                // IPv4
                let addr = Ipv4Addr::new(data[4], data[5], data[6], data[7]);
                Ok(SocketAddr::V4(SocketAddrV4::new(addr, port)))
            }
            _ => Err(P2pError::Discovery(format!(
                "Unknown address family: {family}"
            ))),
        }
    }

    /// Determines NAT type based on local and external addresses.
    fn determine_nat_type(&self, local: SocketAddr, external: SocketAddr) -> NatType {
        // If local IP matches external IP, we're not behind NAT
        // (simplified check - doesn't account for hairpin NAT)
        match (local, external) {
            (SocketAddr::V4(local_v4), SocketAddr::V4(external_v4)) => {
                if local_v4.ip() == external_v4.ip() && local_v4.port() == external_v4.port() {
                    NatType::None
                } else if local_v4.port() == external_v4.port() {
                    // Port preserved - likely cone NAT
                    NatType::FullCone
                } else {
                    // Port changed - could be any type
                    // Would need multiple STUN servers to determine exactly
                    NatType::Unknown
                }
            }
            _ => NatType::Unknown,
        }
    }

    /// Sets up UDP hole punching for a peer.
    ///
    /// This sends packets to the peer's endpoint to create NAT mappings
    /// that allow the peer to reach us.
    ///
    /// # Errors
    ///
    /// Returns an error if the socket operations fail.
    pub fn setup_hole_punch(&self, peer: &PeerInfo) -> Result<(), P2pError> {
        // Get peer's endpoint
        let endpoint = peer.addresses().first().ok_or_else(|| {
            P2pError::Connection("Peer has no addresses".to_string())
        })?;

        let peer_addr: SocketAddr = endpoint.parse().map_err(|e| {
            P2pError::Connection(format!("Invalid peer address: {e}"))
        })?;

        // Create socket
        let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| {
            P2pError::Connection(format!("Failed to bind socket: {e}"))
        })?;

        // Send hole punch packets
        // These are just dummy packets to create NAT mappings
        let punch_packet = b"MOLT_HOLE_PUNCH";
        for _ in 0..3 {
            let _ = socket.send_to(punch_packet, peer_addr);
            std::thread::sleep(Duration::from_millis(50));
        }

        Ok(())
    }

    /// Checks if a direct connection to a peer is likely to work.
    #[must_use]
    pub fn can_connect_directly(&self) -> bool {
        self.cached_endpoint
            .as_ref()
            .is_some_and(|e| e.nat_type.supports_direct_connection())
    }
}

impl Default for NatTraversal {
    fn default() -> Self {
        Self {
            stun_servers: Vec::new(),
            timeout: Duration::from_secs(5),
            cached_endpoint: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== NatType Tests ====================

    #[test]
    fn nat_type_supports_direct_connection() {
        assert!(NatType::None.supports_direct_connection());
        assert!(NatType::FullCone.supports_direct_connection());
        assert!(NatType::RestrictedCone.supports_direct_connection());
        assert!(NatType::PortRestrictedCone.supports_direct_connection());
        assert!(!NatType::Symmetric.supports_direct_connection());
        assert!(!NatType::Unknown.supports_direct_connection());
    }

    #[test]
    fn nat_type_may_need_relay() {
        assert!(!NatType::None.may_need_relay());
        assert!(!NatType::FullCone.may_need_relay());
        assert!(!NatType::RestrictedCone.may_need_relay());
        assert!(!NatType::PortRestrictedCone.may_need_relay());
        assert!(NatType::Symmetric.may_need_relay());
        assert!(NatType::Unknown.may_need_relay());
    }

    #[test]
    fn nat_type_display() {
        assert_eq!(NatType::None.to_string(), "No NAT");
        assert_eq!(NatType::FullCone.to_string(), "Full Cone");
        assert_eq!(NatType::Symmetric.to_string(), "Symmetric");
    }

    // ==================== NatTraversal Tests ====================

    #[test]
    fn nat_traversal_new_requires_servers() {
        let result = NatTraversal::new(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn nat_traversal_new_with_servers() {
        let servers = vec![
            "1.2.3.4:3478".parse().expect("valid"),
            "5.6.7.8:3478".parse().expect("valid"),
        ];
        let nat = NatTraversal::new(servers.clone()).expect("should create");
        assert_eq!(nat.stun_servers().len(), 2);
    }

    #[test]
    fn nat_traversal_with_timeout() {
        let servers = vec!["1.2.3.4:3478".parse().expect("valid")];
        let nat = NatTraversal::new(servers)
            .expect("should create")
            .with_timeout(Duration::from_secs(10));
        assert_eq!(nat.timeout, Duration::from_secs(10));
    }

    #[test]
    fn nat_traversal_default() {
        let nat = NatTraversal::default();
        assert!(nat.stun_servers.is_empty());
        assert!(nat.cached_endpoint.is_none());
    }

    #[test]
    fn nat_traversal_cached_endpoint_initially_none() {
        let servers = vec!["1.2.3.4:3478".parse().expect("valid")];
        let nat = NatTraversal::new(servers).expect("should create");
        assert!(nat.cached_endpoint().is_none());
    }

    // ==================== STUN Parsing Tests ====================

    #[test]
    fn parse_stun_response_too_short() {
        let nat = NatTraversal::default();
        let short_response = [0u8; 10];
        let result = nat.parse_stun_response(&short_response);
        assert!(result.is_err());
    }

    #[test]
    fn parse_stun_response_wrong_type() {
        let nat = NatTraversal::default();
        let mut response = [0u8; 20];
        response[0] = 0x00; // Wrong message type
        response[1] = 0x01;
        let result = nat.parse_stun_response(&response);
        assert!(result.is_err());
    }

    #[test]
    fn parse_stun_response_wrong_magic_cookie() {
        let nat = NatTraversal::default();
        let mut response = [0u8; 20];
        response[0] = 0x01;
        response[1] = 0x01; // Binding Success Response
        response[4..8].copy_from_slice(&[0x00, 0x00, 0x00, 0x00]); // Wrong magic
        let result = nat.parse_stun_response(&response);
        assert!(result.is_err());
    }

    #[test]
    fn parse_xor_mapped_address_ipv4() {
        let nat = NatTraversal::default();
        // XOR-MAPPED-ADDRESS for 192.0.2.1:8080
        // Family: 0x01 (IPv4)
        // X-Port: 8080 XOR 0x2112 = 0x1F90 XOR 0x2112 = 0x3E82
        // X-Address: 192.0.2.1 (0xC0000201) XOR 0x2112A442 = 0xE112A643
        let data = [
            0x00, 0x01, // Reserved + Family (IPv4)
            0x3E, 0x82, // X-Port (XORed): 8080 XOR 0x2112
            0xE1, 0x12, 0xA6, 0x43, // X-Address (XORed)
        ];

        let result = nat.parse_xor_mapped_address(&data);
        assert!(result.is_ok());
        let addr = result.expect("should parse");

        if let SocketAddr::V4(v4) = addr {
            assert_eq!(v4.ip(), &Ipv4Addr::new(192, 0, 2, 1));
            assert_eq!(v4.port(), 8080);
        } else {
            panic!("Expected IPv4 address");
        }
    }

    #[test]
    fn parse_mapped_address_ipv4() {
        let nat = NatTraversal::default();
        // MAPPED-ADDRESS for 10.0.0.1:8080
        let data = [
            0x00, 0x01, // Reserved + Family (IPv4)
            0x1F, 0x90, // Port (8080)
            10, 0, 0, 1, // Address
        ];

        let result = nat.parse_mapped_address(&data);
        assert!(result.is_ok());
        let addr = result.expect("should parse");

        if let SocketAddr::V4(v4) = addr {
            assert_eq!(v4.ip(), &Ipv4Addr::new(10, 0, 0, 1));
            assert_eq!(v4.port(), 8080);
        } else {
            panic!("Expected IPv4 address");
        }
    }

    #[test]
    fn parse_xor_mapped_address_too_short() {
        let nat = NatTraversal::default();
        let data = [0x00, 0x01, 0xA1, 0x27]; // Missing address bytes
        let result = nat.parse_xor_mapped_address(&data);
        assert!(result.is_err());
    }

    #[test]
    fn parse_mapped_address_too_short() {
        let nat = NatTraversal::default();
        let data = [0x00, 0x01, 0x1F, 0x90]; // Missing address bytes
        let result = nat.parse_mapped_address(&data);
        assert!(result.is_err());
    }

    // ==================== NAT Type Detection Tests ====================

    #[test]
    fn determine_nat_type_no_nat() {
        let nat = NatTraversal::default();
        let local = "192.168.1.1:12345".parse().expect("valid");
        let external = "192.168.1.1:12345".parse().expect("valid"); // Same
        assert_eq!(nat.determine_nat_type(local, external), NatType::None);
    }

    #[test]
    fn determine_nat_type_port_preserved() {
        let nat = NatTraversal::default();
        let local = "192.168.1.1:12345".parse().expect("valid");
        let external = "203.0.113.5:12345".parse().expect("valid"); // Different IP, same port
        assert_eq!(nat.determine_nat_type(local, external), NatType::FullCone);
    }

    #[test]
    fn determine_nat_type_port_changed() {
        let nat = NatTraversal::default();
        let local = "192.168.1.1:12345".parse().expect("valid");
        let external = "203.0.113.5:54321".parse().expect("valid"); // Different IP and port
        assert_eq!(nat.determine_nat_type(local, external), NatType::Unknown);
    }

    // ==================== EndpointDiscovery Tests ====================

    #[test]
    fn endpoint_discovery_fields() {
        let discovery = EndpointDiscovery {
            external_endpoint: "203.0.113.5:12345".parse().expect("valid"),
            local_endpoint: "192.168.1.1:54321".parse().expect("valid"),
            nat_type: NatType::FullCone,
            stun_server: "stun.example.com:3478".to_string(),
        };

        assert_eq!(
            discovery.external_endpoint,
            "203.0.113.5:12345".parse::<SocketAddr>().expect("valid")
        );
        assert_eq!(discovery.nat_type, NatType::FullCone);
    }

    // Note: Actual STUN server tests would require network access
    // and are better suited for integration tests.
}
