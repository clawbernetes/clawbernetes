//! Server configuration.

use std::net::SocketAddr;
use std::time::Duration;

/// Configuration for the gateway server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Address to bind the WebSocket server to.
    pub bind_addr: SocketAddr,
    /// Interval for heartbeat expectations from nodes.
    pub heartbeat_interval: Duration,
    /// Interval for metrics collection from nodes.
    pub metrics_interval: Duration,
    /// Maximum number of concurrent connections.
    pub max_connections: usize,
    /// Connection timeout for handshake.
    pub connection_timeout: Duration,
    /// Time after which a node is considered stale if no heartbeat received.
    pub node_stale_timeout: Duration,
}

impl ServerConfig {
    /// Create a new server configuration with the specified bind address.
    #[must_use]
    pub const fn new(bind_addr: SocketAddr) -> Self {
        Self {
            bind_addr,
            heartbeat_interval: Duration::from_secs(30),
            metrics_interval: Duration::from_secs(10),
            max_connections: 1000,
            connection_timeout: Duration::from_secs(10),
            node_stale_timeout: Duration::from_secs(90),
        }
    }

    /// Set the heartbeat interval.
    #[must_use]
    pub const fn with_heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = interval;
        self
    }

    /// Set the metrics interval.
    #[must_use]
    pub const fn with_metrics_interval(mut self, interval: Duration) -> Self {
        self.metrics_interval = interval;
        self
    }

    /// Set the maximum number of connections.
    #[must_use]
    pub const fn with_max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }

    /// Set the connection timeout.
    #[must_use]
    pub const fn with_connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = timeout;
        self
    }

    /// Set the node stale timeout.
    #[must_use]
    pub const fn with_node_stale_timeout(mut self, timeout: Duration) -> Self {
        self.node_stale_timeout = timeout;
        self
    }

    /// Get the heartbeat interval in seconds.
    ///
    /// # Panics
    ///
    /// Panics if the interval exceeds `u32::MAX` seconds (unlikely in practice).
    #[must_use]
    pub fn heartbeat_interval_secs(&self) -> u32 {
        u32::try_from(self.heartbeat_interval.as_secs())
            .unwrap_or(u32::MAX)
    }

    /// Get the metrics interval in seconds.
    ///
    /// # Panics
    ///
    /// Panics if the interval exceeds `u32::MAX` seconds (unlikely in practice).
    #[must_use]
    pub fn metrics_interval_secs(&self) -> u32 {
        u32::try_from(self.metrics_interval.as_secs())
            .unwrap_or(u32::MAX)
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self::new(([0, 0, 0, 0], 8080).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    // ==================== Construction Tests ====================

    #[test]
    fn test_server_config_new() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9000);
        let config = ServerConfig::new(addr);

        assert_eq!(config.bind_addr, addr);
        assert_eq!(config.heartbeat_interval, Duration::from_secs(30));
        assert_eq!(config.metrics_interval, Duration::from_secs(10));
        assert_eq!(config.max_connections, 1000);
        assert_eq!(config.connection_timeout, Duration::from_secs(10));
        assert_eq!(config.node_stale_timeout, Duration::from_secs(90));
    }

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();

        assert_eq!(config.bind_addr, SocketAddr::from(([0, 0, 0, 0], 8080)));
        assert_eq!(config.heartbeat_interval, Duration::from_secs(30));
    }

    // ==================== Builder Pattern Tests ====================

    #[test]
    fn test_with_heartbeat_interval() {
        let config = ServerConfig::default()
            .with_heartbeat_interval(Duration::from_secs(60));

        assert_eq!(config.heartbeat_interval, Duration::from_secs(60));
    }

    #[test]
    fn test_with_metrics_interval() {
        let config = ServerConfig::default()
            .with_metrics_interval(Duration::from_secs(5));

        assert_eq!(config.metrics_interval, Duration::from_secs(5));
    }

    #[test]
    fn test_with_max_connections() {
        let config = ServerConfig::default()
            .with_max_connections(500);

        assert_eq!(config.max_connections, 500);
    }

    #[test]
    fn test_with_connection_timeout() {
        let config = ServerConfig::default()
            .with_connection_timeout(Duration::from_secs(30));

        assert_eq!(config.connection_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_with_node_stale_timeout() {
        let config = ServerConfig::default()
            .with_node_stale_timeout(Duration::from_secs(120));

        assert_eq!(config.node_stale_timeout, Duration::from_secs(120));
    }

    #[test]
    fn test_builder_chaining() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 3000);
        let config = ServerConfig::new(addr)
            .with_heartbeat_interval(Duration::from_secs(45))
            .with_metrics_interval(Duration::from_secs(15))
            .with_max_connections(2000)
            .with_connection_timeout(Duration::from_secs(20))
            .with_node_stale_timeout(Duration::from_secs(180));

        assert_eq!(config.bind_addr, addr);
        assert_eq!(config.heartbeat_interval, Duration::from_secs(45));
        assert_eq!(config.metrics_interval, Duration::from_secs(15));
        assert_eq!(config.max_connections, 2000);
        assert_eq!(config.connection_timeout, Duration::from_secs(20));
        assert_eq!(config.node_stale_timeout, Duration::from_secs(180));
    }

    // ==================== Accessor Tests ====================

    #[test]
    fn test_heartbeat_interval_secs() {
        let config = ServerConfig::default()
            .with_heartbeat_interval(Duration::from_secs(45));

        assert_eq!(config.heartbeat_interval_secs(), 45);
    }

    #[test]
    fn test_metrics_interval_secs() {
        let config = ServerConfig::default()
            .with_metrics_interval(Duration::from_secs(20));

        assert_eq!(config.metrics_interval_secs(), 20);
    }

    // ==================== Clone Tests ====================

    #[test]
    fn test_config_clone() {
        let config = ServerConfig::default()
            .with_max_connections(500);
        let cloned = config.clone();

        assert_eq!(config.max_connections, cloned.max_connections);
        assert_eq!(config.bind_addr, cloned.bind_addr);
    }

    // ==================== Debug Tests ====================

    #[test]
    fn test_config_debug() {
        let config = ServerConfig::default();
        let debug_str = format!("{config:?}");

        assert!(debug_str.contains("ServerConfig"));
        assert!(debug_str.contains("bind_addr"));
    }
}
