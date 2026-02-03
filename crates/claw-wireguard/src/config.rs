//! WireGuard configuration file generation and parsing.
//!
//! This module handles the INI-style configuration format used by WireGuard.

use std::fmt::Write as FmtWrite;
use std::net::IpAddr;

use crate::error::{Result, WireGuardError};
use crate::keys::{PrivateKey, PublicKey, KEY_SIZE};
use crate::types::{AllowedIp, Endpoint, PresharedKey};

/// Configuration for a WireGuard interface.
#[derive(Clone, Debug)]
pub struct InterfaceConfig {
    /// The interface's private key.
    pub private_key: PrivateKey,
    /// Optional listen port.
    pub listen_port: Option<u16>,
    /// IP addresses assigned to this interface.
    pub addresses: Vec<AllowedIp>,
    /// Configured peers.
    pub peers: Vec<PeerConfig>,
    /// Optional DNS servers.
    pub dns: Vec<IpAddr>,
    /// Optional MTU.
    pub mtu: Option<u16>,
}

impl InterfaceConfig {
    /// Creates a new interface configuration with the given private key.
    #[must_use]
    pub fn new(private_key: PrivateKey) -> Self {
        Self {
            private_key,
            listen_port: None,
            addresses: Vec::new(),
            peers: Vec::new(),
            dns: Vec::new(),
            mtu: None,
        }
    }

    /// Sets the listen port.
    #[must_use]
    pub fn with_listen_port(mut self, port: u16) -> Self {
        self.listen_port = Some(port);
        self
    }

    /// Adds an address.
    #[must_use]
    pub fn with_address(mut self, address: AllowedIp) -> Self {
        self.addresses.push(address);
        self
    }

    /// Adds a peer.
    #[must_use]
    pub fn with_peer(mut self, peer: PeerConfig) -> Self {
        self.peers.push(peer);
        self
    }

    /// Adds a DNS server.
    #[must_use]
    pub fn with_dns(mut self, dns: IpAddr) -> Self {
        self.dns.push(dns);
        self
    }

    /// Sets the MTU.
    #[must_use]
    pub fn with_mtu(mut self, mtu: u16) -> Self {
        self.mtu = Some(mtu);
        self
    }
}

/// Builder for creating `InterfaceConfig`.
#[derive(Default)]
pub struct InterfaceConfigBuilder {
    private_key: Option<PrivateKey>,
    listen_port: Option<u16>,
    addresses: Vec<AllowedIp>,
    peers: Vec<PeerConfig>,
    dns: Vec<IpAddr>,
    mtu: Option<u16>,
}

impl InterfaceConfigBuilder {
    /// Creates a new builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the private key.
    #[must_use]
    pub fn private_key(mut self, key: PrivateKey) -> Self {
        self.private_key = Some(key);
        self
    }

    /// Generates a new private key.
    #[must_use]
    pub fn generate_private_key(mut self) -> Self {
        let (private, _) = crate::keys::generate_keypair();
        self.private_key = Some(private);
        self
    }

    /// Sets the listen port.
    #[must_use]
    pub fn listen_port(mut self, port: u16) -> Self {
        self.listen_port = Some(port);
        self
    }

    /// Adds an address from CIDR notation.
    pub fn address_cidr(mut self, cidr: &str) -> Result<Self> {
        self.addresses.push(AllowedIp::from_cidr(cidr)?);
        Ok(self)
    }

    /// Adds a peer.
    #[must_use]
    pub fn peer(mut self, peer: PeerConfig) -> Self {
        self.peers.push(peer);
        self
    }

    /// Adds a DNS server.
    #[must_use]
    pub fn dns(mut self, server: IpAddr) -> Self {
        self.dns.push(server);
        self
    }

    /// Sets the MTU.
    #[must_use]
    pub fn mtu(mut self, mtu: u16) -> Self {
        self.mtu = Some(mtu);
        self
    }

    /// Builds the `InterfaceConfig`.
    pub fn build(self) -> Result<InterfaceConfig> {
        let private_key = self.private_key.ok_or_else(|| {
            WireGuardError::InvalidConfig("private key is required".to_string())
        })?;

        Ok(InterfaceConfig {
            private_key,
            listen_port: self.listen_port,
            addresses: self.addresses,
            peers: self.peers,
            dns: self.dns,
            mtu: self.mtu,
        })
    }
}

/// Configuration for a WireGuard peer.
#[derive(Clone, Debug)]
pub struct PeerConfig {
    /// The peer's public key.
    pub public_key: PublicKey,
    /// Optional preshared key.
    pub preshared_key: Option<PresharedKey>,
    /// Allowed IPs for this peer.
    pub allowed_ips: Vec<AllowedIp>,
    /// Optional endpoint.
    pub endpoint: Option<Endpoint>,
    /// Optional persistent keepalive interval.
    pub persistent_keepalive: Option<u16>,
}

impl PeerConfig {
    /// Creates a new peer config with the given public key.
    #[must_use]
    pub fn new(public_key: PublicKey) -> Self {
        Self {
            public_key,
            preshared_key: None,
            allowed_ips: Vec::new(),
            endpoint: None,
            persistent_keepalive: None,
        }
    }
}

/// Builder for creating `PeerConfig`.
#[derive(Default)]
pub struct PeerConfigBuilder {
    public_key: Option<PublicKey>,
    preshared_key: Option<PresharedKey>,
    allowed_ips: Vec<AllowedIp>,
    endpoint: Option<Endpoint>,
    persistent_keepalive: Option<u16>,
}

impl PeerConfigBuilder {
    /// Creates a new builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the public key.
    #[must_use]
    pub fn public_key(mut self, key: PublicKey) -> Self {
        self.public_key = Some(key);
        self
    }

    /// Sets the preshared key.
    #[must_use]
    pub fn preshared_key(mut self, key: PresharedKey) -> Self {
        self.preshared_key = Some(key);
        self
    }

    /// Adds an allowed IP from CIDR notation.
    pub fn allowed_ip(mut self, cidr: &str) -> Result<Self> {
        self.allowed_ips.push(AllowedIp::from_cidr(cidr)?);
        Ok(self)
    }

    /// Sets the endpoint.
    pub fn endpoint(mut self, endpoint: &str) -> Result<Self> {
        self.endpoint = Some(endpoint.parse()?);
        Ok(self)
    }

    /// Sets the persistent keepalive interval.
    #[must_use]
    pub fn persistent_keepalive(mut self, seconds: u16) -> Self {
        self.persistent_keepalive = Some(seconds);
        self
    }

    /// Builds the `PeerConfig`.
    pub fn build(self) -> Result<PeerConfig> {
        let public_key = self.public_key.ok_or_else(|| {
            WireGuardError::InvalidConfig("public key is required".to_string())
        })?;

        Ok(PeerConfig {
            public_key,
            preshared_key: self.preshared_key,
            allowed_ips: self.allowed_ips,
            endpoint: self.endpoint,
            persistent_keepalive: self.persistent_keepalive,
        })
    }
}

/// Generates a WireGuard configuration file from an `InterfaceConfig`.
#[must_use]
pub fn generate_wg_config(config: &InterfaceConfig) -> String {
    let mut output = String::new();

    output.push_str("[Interface]\n");
    let _ = writeln!(output, "PrivateKey = {}", config.private_key.to_base64());

    if let Some(port) = config.listen_port {
        let _ = writeln!(output, "ListenPort = {port}");
    }

    for addr in &config.addresses {
        let _ = writeln!(output, "Address = {}", addr.to_cidr());
    }

    if !config.dns.is_empty() {
        let dns_str: Vec<String> = config.dns.iter().map(ToString::to_string).collect();
        let _ = writeln!(output, "DNS = {}", dns_str.join(", "));
    }

    if let Some(mtu) = config.mtu {
        let _ = writeln!(output, "MTU = {mtu}");
    }

    for peer in &config.peers {
        output.push('\n');
        output.push_str("[Peer]\n");
        let _ = writeln!(output, "PublicKey = {}", peer.public_key.to_base64());

        if let Some(ref psk) = peer.preshared_key {
            let _ = writeln!(output, "PresharedKey = {}", psk.to_base64());
        }

        if !peer.allowed_ips.is_empty() {
            let ips: Vec<String> = peer.allowed_ips.iter().map(AllowedIp::to_cidr).collect();
            let _ = writeln!(output, "AllowedIPs = {}", ips.join(", "));
        }

        if let Some(ref endpoint) = peer.endpoint {
            let _ = writeln!(output, "Endpoint = {endpoint}");
        }

        if let Some(keepalive) = peer.persistent_keepalive {
            let _ = writeln!(output, "PersistentKeepalive = {keepalive}");
        }
    }

    output
}

/// Parser state for configuration files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Interface,
    Peer,
}

/// Parses a WireGuard configuration file.
pub fn parse_wg_config(config_str: &str) -> Result<InterfaceConfig> {
    let mut section = Section::None;
    let mut private_key: Option<PrivateKey> = None;
    let mut listen_port: Option<u16> = None;
    let mut addresses: Vec<AllowedIp> = Vec::new();
    let mut dns: Vec<IpAddr> = Vec::new();
    let mut mtu: Option<u16> = None;
    let mut peers: Vec<PeerConfig> = Vec::new();
    let mut current_peer: Option<ParsedPeer> = None;

    for (line_num, line) in config_str.lines().enumerate() {
        let line = line.trim();
        let line_number = line_num + 1;

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            if let Some(peer) = current_peer.take() {
                peers.push(peer.build(line_number)?);
            }

            let section_name = &line[1..line.len() - 1];
            section = match section_name {
                "Interface" => Section::Interface,
                "Peer" => {
                    current_peer = Some(ParsedPeer::new());
                    Section::Peer
                }
                _ => {
                    return Err(WireGuardError::ParseError {
                        line: line_number,
                        message: format!("unknown section: {section_name}"),
                    });
                }
            };
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(WireGuardError::ParseError {
                line: line_number,
                message: format!("invalid line format: {line}"),
            });
        };

        let key = key.trim();
        let value = value.trim();

        match section {
            Section::None => {
                return Err(WireGuardError::ParseError {
                    line: line_number,
                    message: "key-value pair outside of section".to_string(),
                });
            }
            Section::Interface => {
                parse_interface_key(
                    key,
                    value,
                    line_number,
                    &mut private_key,
                    &mut listen_port,
                    &mut addresses,
                    &mut dns,
                    &mut mtu,
                )?;
            }
            Section::Peer => {
                if let Some(ref mut peer) = current_peer {
                    peer.parse_key(key, value, line_number)?;
                }
            }
        }
    }

    if let Some(peer) = current_peer {
        let last_line = config_str.lines().count();
        peers.push(peer.build(last_line)?);
    }

    let private_key = private_key.ok_or_else(|| WireGuardError::ParseError {
        line: 0,
        message: "missing PrivateKey in [Interface] section".to_string(),
    })?;

    Ok(InterfaceConfig {
        private_key,
        listen_port,
        addresses,
        peers,
        dns,
        mtu,
    })
}

fn parse_interface_key(
    key: &str,
    value: &str,
    line_number: usize,
    private_key: &mut Option<PrivateKey>,
    listen_port: &mut Option<u16>,
    addresses: &mut Vec<AllowedIp>,
    dns: &mut Vec<IpAddr>,
    mtu: &mut Option<u16>,
) -> Result<()> {
    match key {
        "PrivateKey" => {
            *private_key = Some(PrivateKey::from_base64(value).map_err(|_| {
                WireGuardError::ParseError {
                    line: line_number,
                    message: "invalid PrivateKey".to_string(),
                }
            })?);
        }
        "ListenPort" => {
            *listen_port = Some(value.parse().map_err(|_| WireGuardError::ParseError {
                line: line_number,
                message: "invalid ListenPort".to_string(),
            })?);
        }
        "Address" => {
            for addr in value.split(',') {
                addresses.push(AllowedIp::from_cidr(addr.trim()).map_err(|_| {
                    WireGuardError::ParseError {
                        line: line_number,
                        message: format!("invalid Address: {addr}"),
                    }
                })?);
            }
        }
        "DNS" => {
            for addr in value.split(',') {
                dns.push(addr.trim().parse().map_err(|_| WireGuardError::ParseError {
                    line: line_number,
                    message: format!("invalid DNS address: {addr}"),
                })?);
            }
        }
        "MTU" => {
            *mtu = Some(value.parse().map_err(|_| WireGuardError::ParseError {
                line: line_number,
                message: "invalid MTU".to_string(),
            })?);
        }
        _ => {}
    }
    Ok(())
}

/// Builder for peer configuration during parsing.
#[derive(Default)]
struct ParsedPeer {
    public_key: Option<PublicKey>,
    preshared_key: Option<PresharedKey>,
    allowed_ips: Vec<AllowedIp>,
    endpoint: Option<Endpoint>,
    persistent_keepalive: Option<u16>,
}

impl ParsedPeer {
    fn new() -> Self {
        Self::default()
    }

    fn parse_key(&mut self, key: &str, value: &str, line_number: usize) -> Result<()> {
        match key {
            "PublicKey" => {
                self.public_key =
                    Some(PublicKey::from_base64(value).map_err(|_| WireGuardError::ParseError {
                        line: line_number,
                        message: "invalid PublicKey".to_string(),
                    })?);
            }
            "PresharedKey" => {
                self.preshared_key = Some(
                    PresharedKey::from_base64(value).map_err(|_| WireGuardError::ParseError {
                        line: line_number,
                        message: "invalid PresharedKey".to_string(),
                    })?,
                );
            }
            "AllowedIPs" => {
                for ip in value.split(',') {
                    self.allowed_ips
                        .push(AllowedIp::from_cidr(ip.trim()).map_err(|_| {
                            WireGuardError::ParseError {
                                line: line_number,
                                message: format!("invalid AllowedIPs: {ip}"),
                            }
                        })?);
                }
            }
            "Endpoint" => {
                self.endpoint = Some(value.parse().map_err(|_| WireGuardError::ParseError {
                    line: line_number,
                    message: format!("invalid Endpoint: {value}"),
                })?);
            }
            "PersistentKeepalive" => {
                self.persistent_keepalive =
                    Some(value.parse().map_err(|_| WireGuardError::ParseError {
                        line: line_number,
                        message: "invalid PersistentKeepalive".to_string(),
                    })?);
            }
            _ => {}
        }
        Ok(())
    }

    fn build(self, line_number: usize) -> Result<PeerConfig> {
        let public_key = self.public_key.ok_or_else(|| WireGuardError::ParseError {
            line: line_number,
            message: "missing PublicKey in [Peer] section".to_string(),
        })?;

        Ok(PeerConfig {
            public_key,
            preshared_key: self.preshared_key,
            allowed_ips: self.allowed_ips,
            endpoint: self.endpoint,
            persistent_keepalive: self.persistent_keepalive,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::generate_keypair;

    fn test_private_key() -> PrivateKey {
        PrivateKey::from_bytes(&[1u8; KEY_SIZE]).expect("valid key")
    }

    fn test_public_key() -> PublicKey {
        PublicKey::from_bytes(&[2u8; KEY_SIZE]).expect("valid key")
    }

    #[test]
    fn generate_config_minimal() {
        let config = InterfaceConfig::new(test_private_key());
        let output = generate_wg_config(&config);

        assert!(output.contains("[Interface]"));
        assert!(output.contains("PrivateKey = "));
    }

    #[test]
    fn generate_config_with_port() {
        let config = InterfaceConfig::new(test_private_key()).with_listen_port(51820);
        let output = generate_wg_config(&config);

        assert!(output.contains("ListenPort = 51820"));
    }

    #[test]
    fn generate_config_with_peer() {
        let mut peer = PeerConfig::new(test_public_key());
        peer.allowed_ips.push(AllowedIp::from_cidr("10.0.0.2/32").expect("valid cidr"));
        peer.endpoint = Some("192.168.1.1:51820".parse().expect("valid endpoint"));
        peer.persistent_keepalive = Some(25);

        let config = InterfaceConfig::new(test_private_key()).with_peer(peer);
        let output = generate_wg_config(&config);

        assert!(output.contains("[Peer]"));
        assert!(output.contains("AllowedIPs = 10.0.0.2/32"));
        assert!(output.contains("Endpoint = 192.168.1.1:51820"));
        assert!(output.contains("PersistentKeepalive = 25"));
    }

    #[test]
    fn parse_config_minimal() {
        let private_key = test_private_key();
        let config_str = format!(
            "[Interface]\nPrivateKey = {}\n",
            private_key.to_base64()
        );

        let config = parse_wg_config(&config_str).expect("valid config");
        assert_eq!(config.private_key.to_base64(), private_key.to_base64());
    }

    #[test]
    fn parse_config_with_peer() {
        let private_key = test_private_key();
        let public_key = test_public_key();
        let config_str = format!(
            "[Interface]\n\
             PrivateKey = {}\n\
             \n\
             [Peer]\n\
             PublicKey = {}\n\
             AllowedIPs = 10.0.0.2/32\n",
            private_key.to_base64(),
            public_key.to_base64()
        );

        let config = parse_wg_config(&config_str).expect("valid config");
        assert_eq!(config.peers.len(), 1);
    }

    #[test]
    fn config_roundtrip() {
        let (private_key, _) = generate_keypair();
        let (_, peer_public) = generate_keypair();

        let mut peer = PeerConfig::new(peer_public);
        peer.allowed_ips.push(AllowedIp::from_cidr("10.0.0.0/24").expect("valid cidr"));

        let original = InterfaceConfig::new(private_key)
            .with_listen_port(51820)
            .with_address(AllowedIp::from_cidr("10.0.0.1/24").expect("valid cidr"))
            .with_peer(peer);

        let config_str = generate_wg_config(&original);
        let parsed = parse_wg_config(&config_str).expect("valid config");

        assert_eq!(original.listen_port, parsed.listen_port);
        assert_eq!(original.addresses.len(), parsed.addresses.len());
        assert_eq!(original.peers.len(), parsed.peers.len());
    }

    #[test]
    fn builder_generates_key() {
        let config = InterfaceConfigBuilder::new()
            .generate_private_key()
            .listen_port(51820)
            .build()
            .expect("valid config");

        assert_eq!(config.listen_port, Some(51820));
    }

    #[test]
    fn builder_without_key_fails() {
        let result = InterfaceConfigBuilder::new()
            .listen_port(51820)
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn peer_builder_works() {
        let (_, public_key) = generate_keypair();

        let peer = PeerConfigBuilder::new()
            .public_key(public_key)
            .allowed_ip("10.0.0.2/32")
            .expect("valid cidr")
            .endpoint("192.168.1.1:51820")
            .expect("valid endpoint")
            .persistent_keepalive(25)
            .build()
            .expect("valid peer");

        assert_eq!(peer.allowed_ips.len(), 1);
        assert!(peer.endpoint.is_some());
        assert_eq!(peer.persistent_keepalive, Some(25));
    }
}
