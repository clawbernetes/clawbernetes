//! Tunnel status and health monitoring types.
//!
//! This module provides types for tracking `WireGuard` tunnel status,
//! connection health, and peer connectivity.

use std::fmt;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::keys::PublicKey;
use crate::types::Endpoint;

/// Default handshake timeout before a peer is considered stale.
pub const DEFAULT_HANDSHAKE_TIMEOUT_SECS: u64 = 180;

/// Default keepalive interval for persistent connections.
pub const DEFAULT_KEEPALIVE_SECS: u16 = 25;

/// Connection state of a `WireGuard` tunnel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ConnectionState {
    /// Tunnel is not yet established.
    #[default]
    Disconnected,
    /// Attempting to establish connection.
    Connecting,
    /// Tunnel is established and operational.
    Connected,
    /// Connection was established but is now stale.
    Stale,
    /// Tunnel is being torn down.
    Disconnecting,
    /// Tunnel encountered an error.
    Error,
}


impl fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disconnected => write!(f, "disconnected"),
            Self::Connecting => write!(f, "connecting"),
            Self::Connected => write!(f, "connected"),
            Self::Stale => write!(f, "stale"),
            Self::Disconnecting => write!(f, "disconnecting"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Health status of a tunnel peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum PeerHealth {
    /// Peer is healthy with recent handshakes.
    Healthy,
    /// Peer is degraded (handshake within timeout but not recent).
    Degraded,
    /// Peer has not had a handshake recently.
    Unhealthy,
    /// Peer is unreachable.
    #[default]
    Unreachable,
}


impl fmt::Display for PeerHealth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Degraded => write!(f, "degraded"),
            Self::Unhealthy => write!(f, "unhealthy"),
            Self::Unreachable => write!(f, "unreachable"),
        }
    }
}

/// Status of a single tunnel peer connection.
#[derive(Debug, Clone)]
pub struct TunnelPeerStatus {
    /// The peer's public key.
    pub public_key: PublicKey,
    /// Current connection state.
    pub state: ConnectionState,
    /// Peer health status.
    pub health: PeerHealth,
    /// Current endpoint (if known).
    pub endpoint: Option<Endpoint>,
    /// Assigned tunnel IP address for this peer.
    pub tunnel_ip: Option<IpAddr>,
    /// Last successful handshake time (monotonic).
    last_handshake: Option<Instant>,
    /// Bytes received from this peer.
    pub rx_bytes: u64,
    /// Bytes transmitted to this peer.
    pub tx_bytes: u64,
    /// Number of failed connection attempts.
    pub failed_attempts: u32,
    /// Handshake timeout duration.
    handshake_timeout: Duration,
}

impl TunnelPeerStatus {
    /// Creates a new peer status.
    #[must_use]
    pub fn new(public_key: PublicKey) -> Self {
        Self {
            public_key,
            state: ConnectionState::Disconnected,
            health: PeerHealth::Unreachable,
            endpoint: None,
            tunnel_ip: None,
            last_handshake: None,
            rx_bytes: 0,
            tx_bytes: 0,
            failed_attempts: 0,
            handshake_timeout: Duration::from_secs(DEFAULT_HANDSHAKE_TIMEOUT_SECS),
        }
    }

    /// Creates a peer status with a custom handshake timeout.
    #[must_use]
    pub fn with_handshake_timeout(mut self, timeout: Duration) -> Self {
        self.handshake_timeout = timeout;
        self
    }

    /// Sets the tunnel IP for this peer.
    #[must_use]
    pub fn with_tunnel_ip(mut self, ip: IpAddr) -> Self {
        self.tunnel_ip = Some(ip);
        self
    }

    /// Sets the endpoint for this peer.
    #[must_use]
    pub fn with_endpoint(mut self, endpoint: Endpoint) -> Self {
        self.endpoint = Some(endpoint);
        self
    }

    /// Records a successful handshake.
    pub fn record_handshake(&mut self) {
        self.last_handshake = Some(Instant::now());
        self.state = ConnectionState::Connected;
        self.health = PeerHealth::Healthy;
        self.failed_attempts = 0;
    }

    /// Records a failed connection attempt.
    pub fn record_failure(&mut self) {
        self.failed_attempts = self.failed_attempts.saturating_add(1);
        self.update_health();
    }

    /// Records traffic statistics.
    pub fn record_traffic(&mut self, rx: u64, tx: u64) {
        self.rx_bytes = self.rx_bytes.saturating_add(rx);
        self.tx_bytes = self.tx_bytes.saturating_add(tx);
    }

    /// Updates the health status based on current state.
    pub fn update_health(&mut self) {
        let Some(last) = self.last_handshake else {
            self.health = PeerHealth::Unreachable;
            return;
        };

        let elapsed = last.elapsed();
        let timeout = self.handshake_timeout;

        if elapsed < timeout / 3 {
            self.health = PeerHealth::Healthy;
        } else if elapsed < timeout {
            self.health = PeerHealth::Degraded;
        } else {
            self.health = PeerHealth::Unhealthy;
            self.state = ConnectionState::Stale;
        }
    }

    /// Returns the time since last handshake, if any.
    #[must_use]
    pub fn time_since_handshake(&self) -> Option<Duration> {
        self.last_handshake.map(|t| t.elapsed())
    }

    /// Returns whether this peer is connected (has had a handshake).
    #[must_use]
    pub fn is_connected(&self) -> bool {
        matches!(self.state, ConnectionState::Connected)
    }

    /// Returns whether this peer is healthy.
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        matches!(self.health, PeerHealth::Healthy)
    }

    /// Sets the connection state to connecting.
    pub fn set_connecting(&mut self) {
        self.state = ConnectionState::Connecting;
    }

    /// Sets the connection state to disconnecting.
    pub fn set_disconnecting(&mut self) {
        self.state = ConnectionState::Disconnecting;
    }

    /// Sets the connection state to disconnected.
    pub fn set_disconnected(&mut self) {
        self.state = ConnectionState::Disconnected;
        self.last_handshake = None;
        self.health = PeerHealth::Unreachable;
    }

    /// Sets the connection state to error.
    pub fn set_error(&mut self) {
        self.state = ConnectionState::Error;
    }
}

/// Overall status of a `WireGuard` tunnel.
#[derive(Debug, Clone)]
pub struct TunnelStatus {
    /// Name of the tunnel interface.
    pub interface_name: String,
    /// Our public key for this tunnel.
    pub local_public_key: PublicKey,
    /// Local listen port (if any).
    pub listen_port: Option<u16>,
    /// Local tunnel IP addresses.
    pub local_addresses: Vec<IpAddr>,
    /// Status of all connected peers.
    pub peers: Vec<TunnelPeerStatus>,
    /// When the tunnel was created.
    created_at: Instant,
    /// Overall tunnel state.
    pub state: ConnectionState,
}

impl TunnelStatus {
    /// Creates a new tunnel status.
    #[must_use]
    pub fn new(interface_name: impl Into<String>, local_public_key: PublicKey) -> Self {
        Self {
            interface_name: interface_name.into(),
            local_public_key,
            listen_port: None,
            local_addresses: Vec::new(),
            peers: Vec::new(),
            created_at: Instant::now(),
            state: ConnectionState::Disconnected,
        }
    }

    /// Sets the listen port.
    #[must_use]
    pub fn with_listen_port(mut self, port: u16) -> Self {
        self.listen_port = Some(port);
        self
    }

    /// Adds a local address.
    #[must_use]
    pub fn with_local_address(mut self, addr: IpAddr) -> Self {
        self.local_addresses.push(addr);
        self
    }

    /// Adds a peer status.
    #[must_use]
    pub fn with_peer(mut self, peer: TunnelPeerStatus) -> Self {
        self.peers.push(peer);
        self
    }

    /// Returns the number of peers.
    #[must_use]
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Returns the number of connected peers.
    #[must_use]
    pub fn connected_peer_count(&self) -> usize {
        self.peers.iter().filter(|p| p.is_connected()).count()
    }

    /// Returns the number of healthy peers.
    #[must_use]
    pub fn healthy_peer_count(&self) -> usize {
        self.peers.iter().filter(|p| p.is_healthy()).count()
    }

    /// Returns the tunnel uptime.
    #[must_use]
    pub fn uptime(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Updates the overall tunnel state based on peer states.
    pub fn update_state(&mut self) {
        if self.peers.is_empty() {
            self.state = ConnectionState::Disconnected;
            return;
        }

        let connected = self.connected_peer_count();
        let total = self.peers.len();

        if connected == total {
            self.state = ConnectionState::Connected;
        } else if connected > 0 {
            // Partially connected - use degraded connected state
            self.state = ConnectionState::Connected;
        } else {
            self.state = ConnectionState::Stale;
        }
    }

    /// Finds a peer by public key.
    #[must_use]
    pub fn find_peer(&self, public_key: &PublicKey) -> Option<&TunnelPeerStatus> {
        self.peers.iter().find(|p| &p.public_key == public_key)
    }

    /// Finds a peer by public key (mutable).
    #[must_use]
    pub fn find_peer_mut(&mut self, public_key: &PublicKey) -> Option<&mut TunnelPeerStatus> {
        self.peers.iter_mut().find(|p| &p.public_key == public_key)
    }

    /// Gets a summary of peer health.
    #[must_use]
    pub fn health_summary(&self) -> TunnelHealthSummary {
        let mut summary = TunnelHealthSummary::default();
        for peer in &self.peers {
            match peer.health {
                PeerHealth::Healthy => summary.healthy += 1,
                PeerHealth::Degraded => summary.degraded += 1,
                PeerHealth::Unhealthy => summary.unhealthy += 1,
                PeerHealth::Unreachable => summary.unreachable += 1,
            }
        }
        summary
    }
}

/// Summary of tunnel peer health.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TunnelHealthSummary {
    /// Number of healthy peers.
    pub healthy: usize,
    /// Number of degraded peers.
    pub degraded: usize,
    /// Number of unhealthy peers.
    pub unhealthy: usize,
    /// Number of unreachable peers.
    pub unreachable: usize,
}

impl TunnelHealthSummary {
    /// Total number of peers.
    #[must_use]
    pub fn total(&self) -> usize {
        self.healthy + self.degraded + self.unhealthy + self.unreachable
    }

    /// Returns whether all peers are healthy.
    #[must_use]
    pub fn all_healthy(&self) -> bool {
        self.total() > 0 && self.healthy == self.total()
    }

    /// Returns whether at least one peer is healthy.
    #[must_use]
    pub fn any_healthy(&self) -> bool {
        self.healthy > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::PrivateKey;

    fn test_public_key() -> PublicKey {
        PrivateKey::generate().public_key()
    }

    // ==================== ConnectionState Tests ====================

    #[test]
    fn connection_state_default_is_disconnected() {
        assert_eq!(ConnectionState::default(), ConnectionState::Disconnected);
    }

    #[test]
    fn connection_state_display() {
        assert_eq!(ConnectionState::Disconnected.to_string(), "disconnected");
        assert_eq!(ConnectionState::Connecting.to_string(), "connecting");
        assert_eq!(ConnectionState::Connected.to_string(), "connected");
        assert_eq!(ConnectionState::Stale.to_string(), "stale");
        assert_eq!(ConnectionState::Disconnecting.to_string(), "disconnecting");
        assert_eq!(ConnectionState::Error.to_string(), "error");
    }

    // ==================== PeerHealth Tests ====================

    #[test]
    fn peer_health_default_is_unreachable() {
        assert_eq!(PeerHealth::default(), PeerHealth::Unreachable);
    }

    #[test]
    fn peer_health_display() {
        assert_eq!(PeerHealth::Healthy.to_string(), "healthy");
        assert_eq!(PeerHealth::Degraded.to_string(), "degraded");
        assert_eq!(PeerHealth::Unhealthy.to_string(), "unhealthy");
        assert_eq!(PeerHealth::Unreachable.to_string(), "unreachable");
    }

    // ==================== TunnelPeerStatus Tests ====================

    #[test]
    fn tunnel_peer_status_new() {
        let pk = test_public_key();
        let status = TunnelPeerStatus::new(pk);

        assert_eq!(status.state, ConnectionState::Disconnected);
        assert_eq!(status.health, PeerHealth::Unreachable);
        assert!(status.endpoint.is_none());
        assert!(status.tunnel_ip.is_none());
        assert_eq!(status.rx_bytes, 0);
        assert_eq!(status.tx_bytes, 0);
        assert_eq!(status.failed_attempts, 0);
    }

    #[test]
    fn tunnel_peer_status_with_builders() {
        let pk = test_public_key();
        let endpoint: Endpoint = "192.168.1.1:51820".parse().expect("valid endpoint");
        let ip: IpAddr = "10.0.0.2".parse().expect("valid ip");

        let status = TunnelPeerStatus::new(pk)
            .with_endpoint(endpoint.clone())
            .with_tunnel_ip(ip)
            .with_handshake_timeout(Duration::from_secs(60));

        assert_eq!(status.endpoint, Some(endpoint));
        assert_eq!(status.tunnel_ip, Some(ip));
    }

    #[test]
    fn tunnel_peer_status_record_handshake() {
        let pk = test_public_key();
        let mut status = TunnelPeerStatus::new(pk);

        assert!(!status.is_connected());
        assert!(!status.is_healthy());

        status.record_handshake();

        assert!(status.is_connected());
        assert!(status.is_healthy());
        assert_eq!(status.state, ConnectionState::Connected);
        assert_eq!(status.health, PeerHealth::Healthy);
        assert!(status.time_since_handshake().is_some());
    }

    #[test]
    fn tunnel_peer_status_record_failure() {
        let pk = test_public_key();
        let mut status = TunnelPeerStatus::new(pk);

        status.record_failure();
        assert_eq!(status.failed_attempts, 1);

        status.record_failure();
        assert_eq!(status.failed_attempts, 2);
    }

    #[test]
    fn tunnel_peer_status_record_traffic() {
        let pk = test_public_key();
        let mut status = TunnelPeerStatus::new(pk);

        status.record_traffic(100, 50);
        assert_eq!(status.rx_bytes, 100);
        assert_eq!(status.tx_bytes, 50);

        status.record_traffic(200, 100);
        assert_eq!(status.rx_bytes, 300);
        assert_eq!(status.tx_bytes, 150);
    }

    #[test]
    fn tunnel_peer_status_traffic_saturation() {
        let pk = test_public_key();
        let mut status = TunnelPeerStatus::new(pk);

        status.rx_bytes = u64::MAX - 10;
        status.record_traffic(20, 0);
        assert_eq!(status.rx_bytes, u64::MAX);
    }

    #[test]
    fn tunnel_peer_status_state_transitions() {
        let pk = test_public_key();
        let mut status = TunnelPeerStatus::new(pk);

        status.set_connecting();
        assert_eq!(status.state, ConnectionState::Connecting);

        status.record_handshake();
        assert_eq!(status.state, ConnectionState::Connected);

        status.set_disconnecting();
        assert_eq!(status.state, ConnectionState::Disconnecting);

        status.set_disconnected();
        assert_eq!(status.state, ConnectionState::Disconnected);
        assert!(status.time_since_handshake().is_none());
    }

    #[test]
    fn tunnel_peer_status_set_error() {
        let pk = test_public_key();
        let mut status = TunnelPeerStatus::new(pk);

        status.set_error();
        assert_eq!(status.state, ConnectionState::Error);
    }

    #[test]
    fn tunnel_peer_status_update_health_no_handshake() {
        let pk = test_public_key();
        let mut status = TunnelPeerStatus::new(pk);

        status.update_health();
        assert_eq!(status.health, PeerHealth::Unreachable);
    }

    #[test]
    fn tunnel_peer_status_failure_resets_on_handshake() {
        let pk = test_public_key();
        let mut status = TunnelPeerStatus::new(pk);

        status.record_failure();
        status.record_failure();
        assert_eq!(status.failed_attempts, 2);

        status.record_handshake();
        assert_eq!(status.failed_attempts, 0);
    }

    // ==================== TunnelStatus Tests ====================

    #[test]
    fn tunnel_status_new() {
        let pk = test_public_key();
        let status = TunnelStatus::new("wg0", pk);

        assert_eq!(status.interface_name, "wg0");
        assert!(status.listen_port.is_none());
        assert!(status.local_addresses.is_empty());
        assert!(status.peers.is_empty());
        assert_eq!(status.state, ConnectionState::Disconnected);
    }

    #[test]
    fn tunnel_status_with_builders() {
        let pk = test_public_key();
        let peer_pk = test_public_key();
        let addr: IpAddr = "10.0.0.1".parse().expect("valid ip");

        let status = TunnelStatus::new("wg0", pk)
            .with_listen_port(51820)
            .with_local_address(addr)
            .with_peer(TunnelPeerStatus::new(peer_pk));

        assert_eq!(status.listen_port, Some(51820));
        assert_eq!(status.local_addresses.len(), 1);
        assert_eq!(status.peers.len(), 1);
    }

    #[test]
    fn tunnel_status_peer_counts() {
        let pk = test_public_key();
        let mut status = TunnelStatus::new("wg0", pk);

        // Add disconnected peer
        status.peers.push(TunnelPeerStatus::new(test_public_key()));

        // Add connected peer
        let mut connected = TunnelPeerStatus::new(test_public_key());
        connected.record_handshake();
        status.peers.push(connected);

        assert_eq!(status.peer_count(), 2);
        assert_eq!(status.connected_peer_count(), 1);
        assert_eq!(status.healthy_peer_count(), 1);
    }

    #[test]
    fn tunnel_status_update_state_empty() {
        let pk = test_public_key();
        let mut status = TunnelStatus::new("wg0", pk);

        status.update_state();
        assert_eq!(status.state, ConnectionState::Disconnected);
    }

    #[test]
    fn tunnel_status_update_state_all_connected() {
        let pk = test_public_key();
        let mut status = TunnelStatus::new("wg0", pk);

        let mut peer1 = TunnelPeerStatus::new(test_public_key());
        peer1.record_handshake();
        status.peers.push(peer1);

        let mut peer2 = TunnelPeerStatus::new(test_public_key());
        peer2.record_handshake();
        status.peers.push(peer2);

        status.update_state();
        assert_eq!(status.state, ConnectionState::Connected);
    }

    #[test]
    fn tunnel_status_update_state_partial() {
        let pk = test_public_key();
        let mut status = TunnelStatus::new("wg0", pk);

        let mut connected = TunnelPeerStatus::new(test_public_key());
        connected.record_handshake();
        status.peers.push(connected);

        status.peers.push(TunnelPeerStatus::new(test_public_key()));

        status.update_state();
        assert_eq!(status.state, ConnectionState::Connected);
    }

    #[test]
    fn tunnel_status_update_state_none_connected() {
        let pk = test_public_key();
        let mut status = TunnelStatus::new("wg0", pk);

        status.peers.push(TunnelPeerStatus::new(test_public_key()));
        status.peers.push(TunnelPeerStatus::new(test_public_key()));

        status.update_state();
        assert_eq!(status.state, ConnectionState::Stale);
    }

    #[test]
    fn tunnel_status_find_peer() {
        let pk = test_public_key();
        let peer_pk = test_public_key();
        let mut status = TunnelStatus::new("wg0", pk)
            .with_peer(TunnelPeerStatus::new(peer_pk));

        assert!(status.find_peer(&peer_pk).is_some());
        assert!(status.find_peer(&test_public_key()).is_none());

        // Test mutable find
        let peer_mut = status.find_peer_mut(&peer_pk);
        assert!(peer_mut.is_some());
    }

    #[test]
    fn tunnel_status_uptime() {
        let pk = test_public_key();
        let status = TunnelStatus::new("wg0", pk);

        std::thread::sleep(Duration::from_millis(10));
        assert!(status.uptime().as_millis() >= 10);
    }

    #[test]
    fn tunnel_status_health_summary() {
        let pk = test_public_key();
        let mut status = TunnelStatus::new("wg0", pk);

        // Add healthy peer
        let mut healthy = TunnelPeerStatus::new(test_public_key());
        healthy.record_handshake();
        status.peers.push(healthy);

        // Add unreachable peer
        status.peers.push(TunnelPeerStatus::new(test_public_key()));

        let summary = status.health_summary();
        assert_eq!(summary.healthy, 1);
        assert_eq!(summary.unreachable, 1);
        assert_eq!(summary.total(), 2);
    }

    // ==================== TunnelHealthSummary Tests ====================

    #[test]
    fn tunnel_health_summary_default() {
        let summary = TunnelHealthSummary::default();
        assert_eq!(summary.total(), 0);
        assert!(!summary.all_healthy());
        assert!(!summary.any_healthy());
    }

    #[test]
    fn tunnel_health_summary_all_healthy() {
        let summary = TunnelHealthSummary {
            healthy: 5,
            degraded: 0,
            unhealthy: 0,
            unreachable: 0,
        };
        assert!(summary.all_healthy());
        assert!(summary.any_healthy());
    }

    #[test]
    fn tunnel_health_summary_mixed() {
        let summary = TunnelHealthSummary {
            healthy: 3,
            degraded: 1,
            unhealthy: 1,
            unreachable: 0,
        };
        assert!(!summary.all_healthy());
        assert!(summary.any_healthy());
        assert_eq!(summary.total(), 5);
    }

    #[test]
    fn tunnel_health_summary_none_healthy() {
        let summary = TunnelHealthSummary {
            healthy: 0,
            degraded: 2,
            unhealthy: 1,
            unreachable: 2,
        };
        assert!(!summary.all_healthy());
        assert!(!summary.any_healthy());
    }
}
