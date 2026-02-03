//! P2P connection management.
//!
//! This module provides QUIC-based peer-to-peer connection management:
//! - [`PeerConnection`]: Represents a connection to a single peer
//! - [`ConnectionPool`]: Manages multiple peer connections
//! - [`MessageChannel`]: Bidirectional message channel for peer communication
//! - Connection lifecycle (connect, disconnect, health monitoring)

use crate::error::P2pError;
use crate::message::P2pMessage;
use crate::protocol::PeerId;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};

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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub const fn record_ping_failure(&mut self) {
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
    #[allow(clippy::cast_precision_loss)] // Precision loss acceptable for rate calculation
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
    pub const fn mark_disconnected(&mut self) {
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
    pub const fn record_ping_failure(&mut self) {
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

/// Default channel buffer size for message passing.
const DEFAULT_CHANNEL_BUFFER: usize = 256;

/// A bidirectional message channel for peer communication.
///
/// This provides the I/O layer for sending and receiving P2P messages.
/// The channel uses bounded async queues for backpressure handling.
#[derive(Debug)]
pub struct MessageChannel {
    sender: mpsc::Sender<P2pMessage>,
    receiver: mpsc::Receiver<P2pMessage>,
}

impl MessageChannel {
    /// Creates a new message channel with the given sender and receiver.
    #[must_use]
    pub const fn new(
        sender: mpsc::Sender<P2pMessage>,
        receiver: mpsc::Receiver<P2pMessage>,
    ) -> Self {
        Self { sender, receiver }
    }

    /// Creates a connected pair of message channels for testing.
    ///
    /// Returns `(channel_a, channel_b)` where messages sent on `channel_a`
    /// are received on `channel_b` and vice versa.
    #[must_use]
    pub fn connected_pair() -> (Self, Self) {
        Self::connected_pair_with_buffer(DEFAULT_CHANNEL_BUFFER)
    }

    /// Creates a connected pair with a custom buffer size.
    #[must_use]
    pub fn connected_pair_with_buffer(buffer_size: usize) -> (Self, Self) {
        let (tx_a, rx_a) = mpsc::channel(buffer_size);
        let (tx_b, rx_b) = mpsc::channel(buffer_size);

        let channel_a = Self::new(tx_a, rx_b);
        let channel_b = Self::new(tx_b, rx_a);

        (channel_a, channel_b)
    }

    /// Sends a message through the channel.
    ///
    /// # Errors
    ///
    /// Returns an error if the channel is closed or full.
    pub async fn send(&self, msg: P2pMessage) -> Result<(), P2pError> {
        self.sender
            .send(msg)
            .await
            .map_err(|e| P2pError::Connection(format!("Channel send failed: {e}")))
    }

    /// Receives a message from the channel.
    ///
    /// # Errors
    ///
    /// Returns an error if the channel is closed.
    pub async fn recv(&mut self) -> Result<P2pMessage, P2pError> {
        self.receiver
            .recv()
            .await
            .ok_or_else(|| P2pError::Connection("Channel closed".to_string()))
    }

    /// Tries to receive a message without blocking.
    ///
    /// Returns `None` if no message is immediately available.
    ///
    /// # Errors
    ///
    /// Returns an error if the channel is closed.
    pub fn try_recv(&mut self) -> Result<Option<P2pMessage>, P2pError> {
        match self.receiver.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => {
                Err(P2pError::Connection("Channel closed".to_string()))
            }
        }
    }

    /// Returns true if the sender half is closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }
}

/// An active connection combining state tracking with message I/O.
///
/// This struct owns both the connection state and the message channel,
/// providing a unified interface for peer communication.
#[derive(Debug)]
pub struct ActiveConnection {
    connection: PeerConnection,
    channel: MessageChannel,
}

impl ActiveConnection {
    /// Creates a new active connection.
    #[must_use]
    pub const fn new(connection: PeerConnection, channel: MessageChannel) -> Self {
        Self { connection, channel }
    }

    /// Returns a reference to the underlying peer connection.
    #[must_use]
    pub const fn connection(&self) -> &PeerConnection {
        &self.connection
    }

    /// Returns a mutable reference to the underlying peer connection.
    #[must_use]
    pub const fn connection_mut(&mut self) -> &mut PeerConnection {
        &mut self.connection
    }

    /// Returns the peer ID.
    #[must_use]
    pub const fn peer_id(&self) -> PeerId {
        self.connection.peer_id()
    }

    /// Returns the remote address.
    #[must_use]
    pub const fn remote_addr(&self) -> SocketAddr {
        self.connection.remote_addr()
    }

    /// Returns the current connection state.
    #[must_use]
    pub const fn state(&self) -> ConnectionState {
        self.connection.state()
    }

    /// Returns true if the connection is usable.
    #[must_use]
    pub const fn is_usable(&self) -> bool {
        self.connection.is_usable()
    }

    /// Sends a message to the peer.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection is not usable or the channel fails.
    pub async fn send(&mut self, msg: P2pMessage) -> Result<(), P2pError> {
        if !self.connection.is_usable() {
            return Err(P2pError::Connection(format!(
                "Connection not usable (state: {:?})",
                self.connection.state()
            )));
        }

        self.channel.send(msg).await?;
        self.connection.record_message_sent();
        Ok(())
    }

    /// Receives a message from the peer.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection is not usable or the channel fails.
    pub async fn recv(&mut self) -> Result<P2pMessage, P2pError> {
        if !self.connection.is_usable() {
            return Err(P2pError::Connection(format!(
                "Connection not usable (state: {:?})",
                self.connection.state()
            )));
        }

        let msg = self.channel.recv().await?;
        self.connection.record_message_received();
        Ok(msg)
    }

    /// Tries to receive a message without blocking.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection is not usable or the channel is closed.
    pub fn try_recv(&mut self) -> Result<Option<P2pMessage>, P2pError> {
        if !self.connection.is_usable() {
            return Err(P2pError::Connection(format!(
                "Connection not usable (state: {:?})",
                self.connection.state()
            )));
        }

        let result = self.channel.try_recv()?;
        if result.is_some() {
            self.connection.record_message_received();
        }
        Ok(result)
    }

    /// Marks the connection as connected.
    pub fn mark_connected(&mut self) {
        self.connection.mark_connected();
    }

    /// Marks the connection as disconnecting.
    pub fn mark_disconnecting(&mut self) {
        self.connection.mark_disconnecting();
    }

    /// Marks the connection as disconnected.
    pub const fn mark_disconnected(&mut self) {
        self.connection.mark_disconnected();
    }

    /// Marks the connection as failed.
    pub fn mark_failed(&mut self, error: impl Into<String>) {
        self.connection.mark_failed(error);
    }

    /// Records a successful ping.
    pub fn record_ping_success(&mut self, rtt: Duration) {
        self.connection.record_ping_success(rtt);
    }

    /// Records a failed ping.
    pub const fn record_ping_failure(&mut self) {
        self.connection.record_ping_failure();
    }

    /// Returns the connection health.
    #[must_use]
    pub const fn health(&self) -> &ConnectionHealth {
        self.connection.health()
    }

    /// Creates a connected pair of active connections for testing.
    ///
    /// Both connections start in the Connected state.
    #[must_use]
    pub fn connected_pair(
        peer_a: PeerId,
        addr_a: SocketAddr,
        peer_b: PeerId,
        addr_b: SocketAddr,
    ) -> (Self, Self) {
        let (channel_a, channel_b) = MessageChannel::connected_pair();

        let mut conn_a = PeerConnection::new(peer_a, addr_a);
        let mut conn_b = PeerConnection::new(peer_b, addr_b);

        conn_a.mark_connected();
        conn_b.mark_connected();

        let active_a = Self::new(conn_a, channel_a);
        let active_b = Self::new(conn_b, channel_b);

        (active_a, active_b)
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
        self.connections
            .values()
            .filter(|c| c.is_usable())
            .collect()
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

/// Thread-safe wrapper around [`ConnectionPool`].
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
#[path = "connection_tests.rs"]
mod tests;
