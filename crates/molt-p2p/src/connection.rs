//! P2P connection management.
//!
//! This module provides QUIC-based peer-to-peer connection management:
//! - [`PeerConnection`]: Represents a connection to a single peer
//! - [`ConnectionPool`]: Manages multiple peer connections
//! - Connection lifecycle (connect, disconnect, health monitoring)

use crate::error::P2pError;
use crate::protocol::PeerId;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Connection state for a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Connection is being established.
    Connecting,
    /// Connection is active and healthy.
    Connected,
    /// Connection is being gracefully closed.
    Disconnecting,
    /// Connection has been closed.
    Disconnected,
    /// Connection failed.
    Failed,
}

impl ConnectionState {
    /// Returns true if the connection is in a usable state.
    #[must_use]
    pub const fn is_usable(&self) -> bool {
        matches!(self, Self::Connected)
    }

    /// Returns true if the connection is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Disconnected | Self::Failed)
    }
}

/// Health status of a connection.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionHealth {
    /// Last successful ping round-trip time.
    pub last_rtt: Option<Duration>,
    /// Number of successful pings.
    pub successful_pings: u64,
    /// Number of failed pings.
    pub failed_pings: u64,
    /// Last time the peer was seen (sent or received a message).
    pub last_seen: DateTime<Utc>,
    /// Number of messages sent.
    pub messages_sent: u64,
    /// Number of messages received.
    pub messages_received: u64,
}

impl ConnectionHealth {
    /// Creates a new connection health tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            last_rtt: None,
            successful_pings: 0,
            failed_pings: 0,
            last_seen: Utc::now(),
            messages_sent: 0,
            messages_received: 0,
        }
    }

    /// Records a successful ping with the given round-trip time.
    pub fn record_ping_success(&mut self, rtt: Duration) {
        self.last_rtt = Some(rtt);
        self.successful_pings += 1;
        self.last_seen = Utc::now();
    }

    /// Records a failed ping.
    pub fn record_ping_failure(&mut self) {
        self.failed_pings += 1;
    }

    /// Records a message sent.
    pub fn record_message_sent(&mut self) {
        self.messages_sent += 1;
        self.last_seen = Utc::now();
    }

    /// Records a message received.
    pub fn record_message_received(&mut self) {
        self.messages_received += 1;
        self.last_seen = Utc::now();
    }

    /// Returns the ping success rate (0.0 to 1.0).
    #[must_use]
    pub fn ping_success_rate(&self) -> f64 {
        let total = self.successful_pings + self.failed_pings;
        if total == 0 {
            1.0 // No pings yet, assume healthy
        } else {
            self.successful_pings as f64 / total as f64
        }
    }

    /// Returns true if the connection appears healthy.
    #[must_use]
    pub fn is_healthy(&self, max_silence: Duration) -> bool {
        let silence = Utc::now()
            .signed_duration_since(self.last_seen)
            .to_std()
            .unwrap_or(Duration::MAX);

        silence < max_silence && self.ping_success_rate() > 0.5
    }
}

impl Default for ConnectionHealth {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents a connection to a single peer.
#[derive(Debug)]
pub struct PeerConnection {
    peer_id: PeerId,
    remote_addr: SocketAddr,
    state: ConnectionState,
    health: ConnectionHealth,
    established_at: Option<DateTime<Utc>>,
    error_message: Option<String>,
}

impl PeerConnection {
    /// Creates a new peer connection in the Connecting state.
    #[must_use]
    pub fn new(peer_id: PeerId, remote_addr: SocketAddr) -> Self {
        Self {
            peer_id,
            remote_addr,
            state: ConnectionState::Connecting,
            health: ConnectionHealth::new(),
            established_at: None,
            error_message: None,
        }
    }

    /// Returns the peer ID.
    #[must_use]
    pub const fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    /// Returns the remote address.
    #[must_use]
    pub const fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    /// Returns the current connection state.
    #[must_use]
    pub const fn state(&self) -> ConnectionState {
        self.state
    }

    /// Returns the connection health.
    #[must_use]
    pub const fn health(&self) -> &ConnectionHealth {
        &self.health
    }

    /// Returns when the connection was established, if it has been.
    #[must_use]
    pub const fn established_at(&self) -> Option<DateTime<Utc>> {
        self.established_at
    }

    /// Returns the error message if the connection failed.
    #[must_use]
    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    /// Marks the connection as successfully established.
    pub fn mark_connected(&mut self) {
        self.state = ConnectionState::Connected;
        self.established_at = Some(Utc::now());
        self.error_message = None;
    }

    /// Marks the connection as disconnecting (graceful close initiated).
    pub fn mark_disconnecting(&mut self) {
        if self.state == ConnectionState::Connected {
            self.state = ConnectionState::Disconnecting;
        }
    }

    /// Marks the connection as disconnected.
    pub fn mark_disconnected(&mut self) {
        self.state = ConnectionState::Disconnected;
    }

    /// Marks the connection as failed with an error message.
    pub fn mark_failed(&mut self, error: impl Into<String>) {
        self.state = ConnectionState::Failed;
        self.error_message = Some(error.into());
    }

    /// Returns true if the connection is usable for sending messages.
    #[must_use]
    pub const fn is_usable(&self) -> bool {
        self.state.is_usable()
    }

    /// Records a successful ping.
    pub fn record_ping_success(&mut self, rtt: Duration) {
        self.health.record_ping_success(rtt);
    }

    /// Records a failed ping.
    pub fn record_ping_failure(&mut self) {
        self.health.record_ping_failure();
    }

    /// Records a message sent.
    pub fn record_message_sent(&mut self) {
        self.health.record_message_sent();
    }

    /// Records a message received.
    pub fn record_message_received(&mut self) {
        self.health.record_message_received();
    }

    /// Returns the connection duration if established.
    #[must_use]
    pub fn connection_duration(&self) -> Option<Duration> {
        self.established_at.map(|t| {
            Utc::now()
                .signed_duration_since(t)
                .to_std()
                .unwrap_or(Duration::ZERO)
        })
    }
}

/// Configuration for the connection pool.
#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    /// Maximum number of concurrent connections.
    pub max_connections: usize,
    /// Timeout for establishing a connection.
    pub connect_timeout: Duration,
    /// Interval between health check pings.
    pub ping_interval: Duration,
    /// Maximum time without activity before considering connection unhealthy.
    pub max_silence: Duration,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 100,
            connect_timeout: Duration::from_secs(10),
            ping_interval: Duration::from_secs(30),
            max_silence: Duration::from_secs(90),
        }
    }
}

impl ConnectionPoolConfig {
    /// Creates a new configuration with the given max connections.
    #[must_use]
    pub const fn new(max_connections: usize) -> Self {
        Self {
            max_connections,
            connect_timeout: Duration::from_secs(10),
            ping_interval: Duration::from_secs(30),
            max_silence: Duration::from_secs(90),
        }
    }

    /// Sets the connect timeout.
    #[must_use]
    pub const fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Sets the ping interval.
    #[must_use]
    pub const fn with_ping_interval(mut self, interval: Duration) -> Self {
        self.ping_interval = interval;
        self
    }

    /// Sets the max silence duration.
    #[must_use]
    pub const fn with_max_silence(mut self, duration: Duration) -> Self {
        self.max_silence = duration;
        self
    }
}

/// Manages a pool of peer connections.
#[derive(Debug)]
pub struct ConnectionPool {
    config: ConnectionPoolConfig,
    connections: HashMap<PeerId, PeerConnection>,
}

impl ConnectionPool {
    /// Creates a new connection pool with the given configuration.
    #[must_use]
    pub fn new(config: ConnectionPoolConfig) -> Self {
        Self {
            config,
            connections: HashMap::new(),
        }
    }

    /// Creates a new connection pool with default configuration.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(ConnectionPoolConfig::default())
    }

    /// Returns the pool configuration.
    #[must_use]
    pub const fn config(&self) -> &ConnectionPoolConfig {
        &self.config
    }

    /// Returns the number of connections in the pool.
    #[must_use]
    pub fn len(&self) -> usize {
        self.connections.len()
    }

    /// Returns true if the pool is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }

    /// Returns the number of active (usable) connections.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.connections.values().filter(|c| c.is_usable()).count()
    }

    /// Returns true if the pool has reached its connection limit.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.connections.len() >= self.config.max_connections
    }

    /// Adds a new connection to the pool.
    ///
    /// # Errors
    ///
    /// Returns an error if the pool is full or if a connection to this peer already exists.
    pub fn add_connection(&mut self, conn: PeerConnection) -> Result<(), P2pError> {
        if self.is_full() {
            return Err(P2pError::Connection(format!(
                "Connection pool is full (max: {})",
                self.config.max_connections
            )));
        }

        if self.connections.contains_key(&conn.peer_id) {
            return Err(P2pError::Connection(format!(
                "Connection to peer {} already exists",
                conn.peer_id
            )));
        }

        self.connections.insert(conn.peer_id, conn);
        Ok(())
    }

    /// Gets a reference to a connection by peer ID.
    #[must_use]
    pub fn get(&self, peer_id: &PeerId) -> Option<&PeerConnection> {
        self.connections.get(peer_id)
    }

    /// Gets a mutable reference to a connection by peer ID.
    #[must_use]
    pub fn get_mut(&mut self, peer_id: &PeerId) -> Option<&mut PeerConnection> {
        self.connections.get_mut(peer_id)
    }

    /// Removes a connection from the pool.
    pub fn remove(&mut self, peer_id: &PeerId) -> Option<PeerConnection> {
        self.connections.remove(peer_id)
    }

    /// Returns true if the pool contains a connection to the given peer.
    #[must_use]
    pub fn contains(&self, peer_id: &PeerId) -> bool {
        self.connections.contains_key(peer_id)
    }

    /// Returns all peer IDs in the pool.
    #[must_use]
    pub fn peer_ids(&self) -> Vec<PeerId> {
        self.connections.keys().copied().collect()
    }

    /// Returns all active (usable) connections.
    #[must_use]
    pub fn active_connections(&self) -> Vec<&PeerConnection> {
        self.connections.values().filter(|c| c.is_usable()).collect()
    }

    /// Returns all connections in a specific state.
    #[must_use]
    pub fn connections_in_state(&self, state: ConnectionState) -> Vec<&PeerConnection> {
        self.connections
            .values()
            .filter(|c| c.state() == state)
            .collect()
    }

    /// Removes all connections in terminal states (Disconnected, Failed).
    pub fn cleanup_terminal(&mut self) -> usize {
        let to_remove: Vec<PeerId> = self
            .connections
            .iter()
            .filter(|(_, c)| c.state().is_terminal())
            .map(|(id, _)| *id)
            .collect();

        let count = to_remove.len();
        for peer_id in to_remove {
            self.connections.remove(&peer_id);
        }
        count
    }

    /// Returns connections that appear unhealthy based on configuration.
    #[must_use]
    pub fn unhealthy_connections(&self) -> Vec<&PeerConnection> {
        self.connections
            .values()
            .filter(|c| c.is_usable() && !c.health().is_healthy(self.config.max_silence))
            .collect()
    }

    /// Iterates over all connections.
    pub fn iter(&self) -> impl Iterator<Item = (&PeerId, &PeerConnection)> {
        self.connections.iter()
    }

    /// Iterates mutably over all connections.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&PeerId, &mut PeerConnection)> {
        self.connections.iter_mut()
    }
}

/// Thread-safe wrapper around ConnectionPool.
#[derive(Debug, Clone)]
pub struct SharedConnectionPool {
    inner: Arc<RwLock<ConnectionPool>>,
}

impl SharedConnectionPool {
    /// Creates a new shared connection pool.
    #[must_use]
    pub fn new(config: ConnectionPoolConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(ConnectionPool::new(config))),
        }
    }

    /// Creates a new shared connection pool with default configuration.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(ConnectionPoolConfig::default())
    }

    /// Returns the inner pool for read access.
    pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, ConnectionPool> {
        self.inner.read().await
    }

    /// Returns the inner pool for write access.
    pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, ConnectionPool> {
        self.inner.write().await
    }

    /// Adds a connection to the pool.
    ///
    /// # Errors
    ///
    /// Returns an error if the pool is full or connection already exists.
    pub async fn add_connection(&self, conn: PeerConnection) -> Result<(), P2pError> {
        self.inner.write().await.add_connection(conn)
    }

    /// Removes a connection from the pool.
    pub async fn remove(&self, peer_id: &PeerId) -> Option<PeerConnection> {
        self.inner.write().await.remove(peer_id)
    }

    /// Returns the number of connections.
    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }

    /// Returns true if empty.
    pub async fn is_empty(&self) -> bool {
        self.inner.read().await.is_empty()
    }

    /// Returns the number of active connections.
    pub async fn active_count(&self) -> usize {
        self.inner.read().await.active_count()
    }

    /// Returns all peer IDs.
    pub async fn peer_ids(&self) -> Vec<PeerId> {
        self.inner.read().await.peer_ids()
    }

    /// Checks if pool contains a peer.
    pub async fn contains(&self, peer_id: &PeerId) -> bool {
        self.inner.read().await.contains(peer_id)
    }

    /// Cleans up terminal connections.
    pub async fn cleanup_terminal(&self) -> usize {
        self.inner.write().await.cleanup_terminal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use std::net::{IpAddr, Ipv4Addr};

    fn make_peer_id() -> PeerId {
        let signing_key = SigningKey::generate(&mut OsRng);
        PeerId::from_public_key(&signing_key.verifying_key())
    }

    fn make_socket_addr(port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, port as u8)), port)
    }

    fn make_connection(port: u16) -> PeerConnection {
        PeerConnection::new(make_peer_id(), make_socket_addr(port))
    }

    // ========== ConnectionState Tests ==========

    #[test]
    fn connection_state_is_usable() {
        assert!(!ConnectionState::Connecting.is_usable());
        assert!(ConnectionState::Connected.is_usable());
        assert!(!ConnectionState::Disconnecting.is_usable());
        assert!(!ConnectionState::Disconnected.is_usable());
        assert!(!ConnectionState::Failed.is_usable());
    }

    #[test]
    fn connection_state_is_terminal() {
        assert!(!ConnectionState::Connecting.is_terminal());
        assert!(!ConnectionState::Connected.is_terminal());
        assert!(!ConnectionState::Disconnecting.is_terminal());
        assert!(ConnectionState::Disconnected.is_terminal());
        assert!(ConnectionState::Failed.is_terminal());
    }

    // ========== ConnectionHealth Tests ==========

    #[test]
    fn connection_health_new() {
        let health = ConnectionHealth::new();
        assert!(health.last_rtt.is_none());
        assert_eq!(health.successful_pings, 0);
        assert_eq!(health.failed_pings, 0);
        assert_eq!(health.messages_sent, 0);
        assert_eq!(health.messages_received, 0);
    }

    #[test]
    fn connection_health_record_ping_success() {
        let mut health = ConnectionHealth::new();
        let rtt = Duration::from_millis(50);

        health.record_ping_success(rtt);

        assert_eq!(health.last_rtt, Some(rtt));
        assert_eq!(health.successful_pings, 1);
        assert_eq!(health.failed_pings, 0);
    }

    #[test]
    fn connection_health_record_ping_failure() {
        let mut health = ConnectionHealth::new();

        health.record_ping_failure();

        assert!(health.last_rtt.is_none());
        assert_eq!(health.successful_pings, 0);
        assert_eq!(health.failed_pings, 1);
    }

    #[test]
    fn connection_health_ping_success_rate() {
        let mut health = ConnectionHealth::new();

        // No pings yet - assume 100% healthy
        assert!((health.ping_success_rate() - 1.0).abs() < f64::EPSILON);

        // 3 successes, 1 failure = 75%
        health.record_ping_success(Duration::from_millis(10));
        health.record_ping_success(Duration::from_millis(10));
        health.record_ping_success(Duration::from_millis(10));
        health.record_ping_failure();

        assert!((health.ping_success_rate() - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn connection_health_message_tracking() {
        let mut health = ConnectionHealth::new();

        health.record_message_sent();
        health.record_message_sent();
        health.record_message_received();

        assert_eq!(health.messages_sent, 2);
        assert_eq!(health.messages_received, 1);
    }

    #[test]
    fn connection_health_is_healthy() {
        let mut health = ConnectionHealth::new();
        let max_silence = Duration::from_secs(60);

        // Fresh connection should be healthy
        assert!(health.is_healthy(max_silence));

        // After many failures, should be unhealthy
        for _ in 0..10 {
            health.record_ping_failure();
        }
        health.record_ping_success(Duration::from_millis(10)); // Touch last_seen
        assert!(!health.is_healthy(max_silence)); // < 50% success rate
    }

    // ========== PeerConnection Tests ==========

    #[test]
    fn peer_connection_new() {
        let peer_id = make_peer_id();
        let addr = make_socket_addr(8080);

        let conn = PeerConnection::new(peer_id, addr);

        assert_eq!(conn.peer_id(), peer_id);
        assert_eq!(conn.remote_addr(), addr);
        assert_eq!(conn.state(), ConnectionState::Connecting);
        assert!(conn.established_at().is_none());
        assert!(conn.error_message().is_none());
    }

    #[test]
    fn peer_connection_mark_connected() {
        let mut conn = make_connection(8080);

        assert_eq!(conn.state(), ConnectionState::Connecting);
        assert!(conn.established_at().is_none());

        conn.mark_connected();

        assert_eq!(conn.state(), ConnectionState::Connected);
        assert!(conn.established_at().is_some());
        assert!(conn.is_usable());
    }

    #[test]
    fn peer_connection_mark_disconnecting() {
        let mut conn = make_connection(8080);

        // Disconnecting from Connecting state does nothing
        conn.mark_disconnecting();
        assert_eq!(conn.state(), ConnectionState::Connecting);

        // Disconnecting from Connected state works
        conn.mark_connected();
        conn.mark_disconnecting();
        assert_eq!(conn.state(), ConnectionState::Disconnecting);
    }

    #[test]
    fn peer_connection_mark_disconnected() {
        let mut conn = make_connection(8080);
        conn.mark_connected();
        conn.mark_disconnecting();
        conn.mark_disconnected();

        assert_eq!(conn.state(), ConnectionState::Disconnected);
        assert!(!conn.is_usable());
    }

    #[test]
    fn peer_connection_mark_failed() {
        let mut conn = make_connection(8080);

        conn.mark_failed("Connection refused");

        assert_eq!(conn.state(), ConnectionState::Failed);
        assert_eq!(conn.error_message(), Some("Connection refused"));
        assert!(!conn.is_usable());
    }

    #[test]
    fn peer_connection_health_tracking() {
        let mut conn = make_connection(8080);
        conn.mark_connected();

        conn.record_ping_success(Duration::from_millis(50));
        conn.record_message_sent();
        conn.record_message_received();

        assert_eq!(conn.health().successful_pings, 1);
        assert_eq!(conn.health().messages_sent, 1);
        assert_eq!(conn.health().messages_received, 1);
    }

    #[test]
    fn peer_connection_duration() {
        let mut conn = make_connection(8080);

        // Not established yet
        assert!(conn.connection_duration().is_none());

        conn.mark_connected();
        std::thread::sleep(Duration::from_millis(10));

        let duration = conn.connection_duration();
        assert!(duration.is_some());
        assert!(duration.unwrap() >= Duration::from_millis(10));
    }

    // ========== ConnectionPoolConfig Tests ==========

    #[test]
    fn connection_pool_config_default() {
        let config = ConnectionPoolConfig::default();

        assert_eq!(config.max_connections, 100);
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.ping_interval, Duration::from_secs(30));
        assert_eq!(config.max_silence, Duration::from_secs(90));
    }

    #[test]
    fn connection_pool_config_builder() {
        let config = ConnectionPoolConfig::new(50)
            .with_connect_timeout(Duration::from_secs(5))
            .with_ping_interval(Duration::from_secs(15))
            .with_max_silence(Duration::from_secs(45));

        assert_eq!(config.max_connections, 50);
        assert_eq!(config.connect_timeout, Duration::from_secs(5));
        assert_eq!(config.ping_interval, Duration::from_secs(15));
        assert_eq!(config.max_silence, Duration::from_secs(45));
    }

    // ========== ConnectionPool Tests ==========

    #[test]
    fn connection_pool_new() {
        let pool = ConnectionPool::with_defaults();

        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
        assert_eq!(pool.active_count(), 0);
    }

    #[test]
    fn connection_pool_add_connection() {
        let mut pool = ConnectionPool::with_defaults();
        let conn = make_connection(8080);
        let peer_id = conn.peer_id();

        pool.add_connection(conn).expect("should add connection");

        assert_eq!(pool.len(), 1);
        assert!(pool.contains(&peer_id));
    }

    #[test]
    fn connection_pool_add_duplicate_fails() {
        let mut pool = ConnectionPool::with_defaults();
        let peer_id = make_peer_id();
        let addr = make_socket_addr(8080);

        let conn1 = PeerConnection::new(peer_id, addr);
        let conn2 = PeerConnection::new(peer_id, addr);

        pool.add_connection(conn1).expect("first add should succeed");
        let result = pool.add_connection(conn2);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), P2pError::Connection(_)));
    }

    #[test]
    fn connection_pool_max_connections() {
        let config = ConnectionPoolConfig::new(2);
        let mut pool = ConnectionPool::new(config);

        pool.add_connection(make_connection(8081))
            .expect("first add should succeed");
        pool.add_connection(make_connection(8082))
            .expect("second add should succeed");

        assert!(pool.is_full());

        let result = pool.add_connection(make_connection(8083));
        assert!(result.is_err());
    }

    #[test]
    fn connection_pool_get_and_get_mut() {
        let mut pool = ConnectionPool::with_defaults();
        let conn = make_connection(8080);
        let peer_id = conn.peer_id();

        pool.add_connection(conn).expect("should add connection");

        // Get immutable reference
        let conn_ref = pool.get(&peer_id);
        assert!(conn_ref.is_some());
        assert_eq!(conn_ref.unwrap().state(), ConnectionState::Connecting);

        // Get mutable reference and modify
        let conn_mut = pool.get_mut(&peer_id);
        assert!(conn_mut.is_some());
        conn_mut.unwrap().mark_connected();

        // Verify modification
        assert_eq!(pool.get(&peer_id).unwrap().state(), ConnectionState::Connected);
    }

    #[test]
    fn connection_pool_remove() {
        let mut pool = ConnectionPool::with_defaults();
        let conn = make_connection(8080);
        let peer_id = conn.peer_id();

        pool.add_connection(conn).expect("should add connection");
        assert!(pool.contains(&peer_id));

        let removed = pool.remove(&peer_id);
        assert!(removed.is_some());
        assert!(!pool.contains(&peer_id));
        assert!(pool.is_empty());
    }

    #[test]
    fn connection_pool_active_count() {
        let mut pool = ConnectionPool::with_defaults();

        let mut conn1 = make_connection(8081);
        let mut conn2 = make_connection(8082);
        let conn3 = make_connection(8083);

        conn1.mark_connected();
        conn2.mark_connected();
        // conn3 is still Connecting

        pool.add_connection(conn1).expect("add conn1");
        pool.add_connection(conn2).expect("add conn2");
        pool.add_connection(conn3).expect("add conn3");

        assert_eq!(pool.len(), 3);
        assert_eq!(pool.active_count(), 2);
    }

    #[test]
    fn connection_pool_peer_ids() {
        let mut pool = ConnectionPool::with_defaults();

        let conn1 = make_connection(8081);
        let conn2 = make_connection(8082);
        let id1 = conn1.peer_id();
        let id2 = conn2.peer_id();

        pool.add_connection(conn1).expect("add conn1");
        pool.add_connection(conn2).expect("add conn2");

        let ids = pool.peer_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
    }

    #[test]
    fn connection_pool_active_connections() {
        let mut pool = ConnectionPool::with_defaults();

        let mut conn1 = make_connection(8081);
        let mut conn2 = make_connection(8082);
        let mut conn3 = make_connection(8083);

        conn1.mark_connected();
        conn2.mark_failed("error");
        conn3.mark_connected();

        pool.add_connection(conn1).expect("add");
        pool.add_connection(conn2).expect("add");
        pool.add_connection(conn3).expect("add");

        let active = pool.active_connections();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn connection_pool_connections_in_state() {
        let mut pool = ConnectionPool::with_defaults();

        let mut conn1 = make_connection(8081);
        let mut conn2 = make_connection(8082);
        let conn3 = make_connection(8083);

        conn1.mark_connected();
        conn2.mark_connected();
        // conn3 is Connecting

        pool.add_connection(conn1).expect("add");
        pool.add_connection(conn2).expect("add");
        pool.add_connection(conn3).expect("add");

        let connected = pool.connections_in_state(ConnectionState::Connected);
        let connecting = pool.connections_in_state(ConnectionState::Connecting);

        assert_eq!(connected.len(), 2);
        assert_eq!(connecting.len(), 1);
    }

    #[test]
    fn connection_pool_cleanup_terminal() {
        let mut pool = ConnectionPool::with_defaults();

        let mut conn1 = make_connection(8081);
        let mut conn2 = make_connection(8082);
        let mut conn3 = make_connection(8083);

        conn1.mark_connected();
        conn2.mark_failed("error");
        conn3.mark_connected();
        conn3.mark_disconnecting();
        conn3.mark_disconnected();

        pool.add_connection(conn1).expect("add");
        pool.add_connection(conn2).expect("add");
        pool.add_connection(conn3).expect("add");

        assert_eq!(pool.len(), 3);

        let removed = pool.cleanup_terminal();
        assert_eq!(removed, 2); // conn2 (Failed) and conn3 (Disconnected)
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn connection_pool_iter() {
        let mut pool = ConnectionPool::with_defaults();

        pool.add_connection(make_connection(8081)).expect("add");
        pool.add_connection(make_connection(8082)).expect("add");

        let count = pool.iter().count();
        assert_eq!(count, 2);
    }

    #[test]
    fn connection_pool_iter_mut() {
        let mut pool = ConnectionPool::with_defaults();

        pool.add_connection(make_connection(8081)).expect("add");
        pool.add_connection(make_connection(8082)).expect("add");

        // Mark all as connected using iter_mut
        for (_, conn) in pool.iter_mut() {
            conn.mark_connected();
        }

        assert_eq!(pool.active_count(), 2);
    }

    // ========== SharedConnectionPool Tests ==========

    #[tokio::test]
    async fn shared_connection_pool_basic_operations() {
        let pool = SharedConnectionPool::with_defaults();

        assert!(pool.is_empty().await);

        let conn = make_connection(8080);
        let peer_id = conn.peer_id();

        pool.add_connection(conn).await.expect("should add");

        assert_eq!(pool.len().await, 1);
        assert!(pool.contains(&peer_id).await);

        pool.remove(&peer_id).await;
        assert!(pool.is_empty().await);
    }

    #[tokio::test]
    async fn shared_connection_pool_concurrent_access() {
        let pool = SharedConnectionPool::with_defaults();

        let pool1 = pool.clone();
        let pool2 = pool.clone();

        let handle1 = tokio::spawn(async move {
            for i in 0..10 {
                let conn = make_connection(8000 + i);
                let _ = pool1.add_connection(conn).await;
            }
        });

        let handle2 = tokio::spawn(async move {
            for _ in 0..50 {
                let _ = pool2.len().await;
                tokio::task::yield_now().await;
            }
        });

        handle1.await.expect("task 1 should complete");
        handle2.await.expect("task 2 should complete");

        // Should have added some connections (exact count depends on timing)
        assert!(pool.len().await > 0);
    }

    #[tokio::test]
    async fn shared_connection_pool_cleanup_terminal() {
        let pool = SharedConnectionPool::with_defaults();

        let mut conn1 = make_connection(8081);
        let mut conn2 = make_connection(8082);

        conn1.mark_connected();
        conn2.mark_failed("test error");

        pool.add_connection(conn1).await.expect("add");
        pool.add_connection(conn2).await.expect("add");

        assert_eq!(pool.len().await, 2);

        let removed = pool.cleanup_terminal().await;
        assert_eq!(removed, 1);
        assert_eq!(pool.len().await, 1);
    }

    // ========== Proptest ==========

    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn connection_health_ping_rate_bounds(
                successes in 0u64..1000,
                failures in 0u64..1000
            ) {
                let mut health = ConnectionHealth::new();
                
                for _ in 0..successes {
                    health.record_ping_success(Duration::from_millis(10));
                }
                for _ in 0..failures {
                    health.record_ping_failure();
                }
                
                let rate = health.ping_success_rate();
                prop_assert!(rate >= 0.0 && rate <= 1.0);
            }

            #[test]
            fn connection_pool_add_up_to_max(max_conns in 1usize..20) {
                let config = ConnectionPoolConfig::new(max_conns);
                let mut pool = ConnectionPool::new(config);
                
                for i in 0..max_conns {
                    let result = pool.add_connection(make_connection(8000 + i as u16));
                    prop_assert!(result.is_ok());
                }
                
                prop_assert!(pool.is_full());
                prop_assert_eq!(pool.len(), max_conns);
            }

            #[test]
            fn connection_state_transitions_valid(
                should_connect in any::<bool>(),
                should_fail in any::<bool>()
            ) {
                let mut conn = make_connection(8080);
                prop_assert_eq!(conn.state(), ConnectionState::Connecting);
                
                if should_fail {
                    conn.mark_failed("test");
                    prop_assert_eq!(conn.state(), ConnectionState::Failed);
                } else if should_connect {
                    conn.mark_connected();
                    prop_assert_eq!(conn.state(), ConnectionState::Connected);
                    
                    conn.mark_disconnecting();
                    prop_assert_eq!(conn.state(), ConnectionState::Disconnecting);
                    
                    conn.mark_disconnected();
                    prop_assert_eq!(conn.state(), ConnectionState::Disconnected);
                }
            }
        }
    }
}
