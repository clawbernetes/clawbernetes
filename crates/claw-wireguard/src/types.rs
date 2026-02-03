//! Core types for WireGuard configuration.

use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use ipnet::IpNet;
use serde::{Deserialize, Serialize};

use crate::error::{Result, WireGuardError};
use crate::keys::{PublicKey, KEY_SIZE};

/// A WireGuard preshared key (optional, 32 bytes).
#[derive(Clone, Serialize, Deserialize)]
pub struct PresharedKey([u8; KEY_SIZE]);

impl PresharedKey {
    /// Creates from raw bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != KEY_SIZE {
            return Err(WireGuardError::InvalidKeyLength(bytes.len()));
        }
        let mut key = [0u8; KEY_SIZE];
        key.copy_from_slice(bytes);
        Ok(Self(key))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; KEY_SIZE] { &self.0 }

    #[must_use]
    pub fn to_base64(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(self.0)
    }

    pub fn from_base64(s: &str) -> Result<Self> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(s)?;
        Self::from_bytes(&bytes)
    }

    #[must_use]
    pub fn generate() -> Self {
        use rand::RngCore;
        let mut key = [0u8; KEY_SIZE];
        rand::thread_rng().fill_bytes(&mut key);
        Self(key)
    }
}

impl fmt::Debug for PresharedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PresharedKey").field("key", &"[REDACTED]").finish()
    }
}

impl PartialEq for PresharedKey {
    fn eq(&self, other: &Self) -> bool {
        use subtle::ConstantTimeEq;
        self.0.ct_eq(&other.0).into()
    }
}

impl Eq for PresharedKey {}

/// An allowed IP address or network in CIDR notation.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AllowedIp { network: IpNet }

impl AllowedIp {
    #[must_use]
    pub fn new(network: IpNet) -> Self { Self { network } }

    #[must_use]
    pub fn network(&self) -> &IpNet { &self.network }

    pub fn from_cidr(s: &str) -> Result<Self> {
        let network = s.parse::<IpNet>().map_err(|e| WireGuardError::InvalidCidr(e.to_string()))?;
        Ok(Self { network })
    }

    #[must_use]
    pub fn to_cidr(&self) -> String { self.network.to_string() }
}

impl FromStr for AllowedIp {
    type Err = WireGuardError;
    fn from_str(s: &str) -> Result<Self> { Self::from_cidr(s) }
}

impl fmt::Display for AllowedIp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.network) }
}

/// A WireGuard peer endpoint.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Endpoint { address: SocketAddr }

impl Endpoint {
    #[must_use]
    pub fn new(address: SocketAddr) -> Self { Self { address } }

    #[must_use]
    pub fn from_ip_port(ip: IpAddr, port: u16) -> Self { Self { address: SocketAddr::new(ip, port) } }

    #[must_use]
    pub fn address(&self) -> &SocketAddr { &self.address }

    #[must_use]
    pub fn ip(&self) -> IpAddr { self.address.ip() }

    #[must_use]
    pub fn port(&self) -> u16 { self.address.port() }
}

impl FromStr for Endpoint {
    type Err = WireGuardError;
    fn from_str(s: &str) -> Result<Self> {
        let address = s.parse::<SocketAddr>().map_err(|e| WireGuardError::InvalidEndpoint(e.to_string()))?;
        Ok(Self { address })
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.address) }
}

/// Configuration for a WireGuard peer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WireGuardPeer {
    pub public_key: PublicKey,
    pub preshared_key: Option<PresharedKey>,
    pub allowed_ips: Vec<AllowedIp>,
    pub endpoint: Option<Endpoint>,
    pub persistent_keepalive: Option<u16>,
}

impl WireGuardPeer {
    #[must_use]
    pub fn new(public_key: PublicKey) -> Self {
        Self { public_key, preshared_key: None, allowed_ips: Vec::new(), endpoint: None, persistent_keepalive: None }
    }

    #[must_use]
    pub fn with_preshared_key(mut self, key: PresharedKey) -> Self { self.preshared_key = Some(key); self }

    #[must_use]
    pub fn with_allowed_ip(mut self, ip: AllowedIp) -> Self { self.allowed_ips.push(ip); self }

    #[must_use]
    pub fn with_endpoint(mut self, endpoint: Endpoint) -> Self { self.endpoint = Some(endpoint); self }

    #[must_use]
    pub fn with_persistent_keepalive(mut self, seconds: u16) -> Self { self.persistent_keepalive = Some(seconds); self }
}

/// Status of a WireGuard interface.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InterfaceStatus {
    pub name: String,
    pub public_key: PublicKey,
    pub listen_port: Option<u16>,
    pub peers: Vec<PeerStatus>,
}

/// Status of a WireGuard peer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerStatus {
    pub public_key: PublicKey,
    pub endpoint: Option<Endpoint>,
    pub allowed_ips: Vec<AllowedIp>,
    pub last_handshake: Option<u64>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}
