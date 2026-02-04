//! Server configuration.

use std::net::SocketAddr;
use std::time::Duration;

/// Default maximum WebSocket message size: 1MB.
pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 1024 * 1024;

/// Default maximum WebSocket frame size: 64KB.
pub const DEFAULT_MAX_FRAME_SIZE: usize = 64 * 1024;

/// Default maximum violations before connection termination.
pub const DEFAULT_MAX_VIOLATIONS: u32 = 3;

/// Configuration for WebSocket message handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebSocketConfig {
    /// Maximum allowed message size in bytes.
    /// Messages larger than this will be rejected.
    pub max_message_size: usize,
    /// Maximum allowed frame size in bytes.
    /// Frames larger than this will be rejected.
    pub max_frame_size: usize,
    /// Maximum number of size violations before terminating the connection.
    /// Set to 0 to close immediately on first violation.
    pub max_violations: u32,
}

impl WebSocketConfig {
    /// Create a new WebSocket configuration with default values.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
            max_frame_size: DEFAULT_MAX_FRAME_SIZE,
            max_violations: DEFAULT_MAX_VIOLATIONS,
        }
    }

    /// Set the maximum message size.
    #[must_use]
    pub const fn with_max_message_size(mut self, size: usize) -> Self {
        self.max_message_size = size;
        self
    }

    /// Set the maximum frame size.
    #[must_use]
    pub const fn with_max_frame_size(mut self, size: usize) -> Self {
        self.max_frame_size = size;
        self
    }

    /// Set the maximum number of violations before termination.
    #[must_use]
    pub const fn with_max_violations(mut self, max: u32) -> Self {
        self.max_violations = max;
        self
    }

    /// Check if a message size is within the allowed limit.
    #[must_use]
    pub const fn is_message_size_valid(&self, size: usize) -> bool {
        size <= self.max_message_size
    }

    /// Check if a frame size is within the allowed limit.
    #[must_use]
    pub const fn is_frame_size_valid(&self, size: usize) -> bool {
        size <= self.max_frame_size
    }
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self::new()
    }
}

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
    /// WebSocket configuration for message size limits.
    pub websocket: WebSocketConfig,
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
            websocket: WebSocketConfig::new(),
        }
    }

    /// Set the WebSocket configuration.
    #[must_use]
    pub const fn with_websocket_config(mut self, config: WebSocketConfig) -> Self {
        self.websocket = config;
        self
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

    // ==================== WebSocketConfig Tests ====================

    #[test]
    fn test_websocket_config_new() {
        let config = WebSocketConfig::new();

        assert_eq!(config.max_message_size, DEFAULT_MAX_MESSAGE_SIZE);
        assert_eq!(config.max_frame_size, DEFAULT_MAX_FRAME_SIZE);
        assert_eq!(config.max_violations, DEFAULT_MAX_VIOLATIONS);
    }

    #[test]
    fn test_websocket_config_default() {
        let config = WebSocketConfig::default();

        assert_eq!(config.max_message_size, 1024 * 1024);
        assert_eq!(config.max_frame_size, 64 * 1024);
        assert_eq!(config.max_violations, 3);
    }

    #[test]
    fn test_websocket_config_builder() {
        let config = WebSocketConfig::new()
            .with_max_message_size(2 * 1024 * 1024)
            .with_max_frame_size(128 * 1024)
            .with_max_violations(5);

        assert_eq!(config.max_message_size, 2 * 1024 * 1024);
        assert_eq!(config.max_frame_size, 128 * 1024);
        assert_eq!(config.max_violations, 5);
    }

    #[test]
    fn test_websocket_config_message_size_validation() {
        let config = WebSocketConfig::new().with_max_message_size(1000);

        assert!(config.is_message_size_valid(500));
        assert!(config.is_message_size_valid(1000));
        assert!(!config.is_message_size_valid(1001));
    }

    #[test]
    fn test_websocket_config_frame_size_validation() {
        let config = WebSocketConfig::new().with_max_frame_size(500);

        assert!(config.is_frame_size_valid(250));
        assert!(config.is_frame_size_valid(500));
        assert!(!config.is_frame_size_valid(501));
    }

    #[test]
    fn test_websocket_config_zero_violations() {
        let config = WebSocketConfig::new().with_max_violations(0);
        assert_eq!(config.max_violations, 0);
    }

    #[test]
    fn test_websocket_config_equality() {
        let config1 = WebSocketConfig::new();
        let config2 = WebSocketConfig::new();
        let config3 = WebSocketConfig::new().with_max_message_size(500);

        assert_eq!(config1, config2);
        assert_ne!(config1, config3);
    }

    #[test]
    fn test_websocket_config_copy() {
        let config1 = WebSocketConfig::new().with_max_message_size(500);
        let config2 = config1;

        assert_eq!(config1, config2);
    }

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
        assert_eq!(config.websocket, WebSocketConfig::new());
    }

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();

        assert_eq!(config.bind_addr, SocketAddr::from(([0, 0, 0, 0], 8080)));
        assert_eq!(config.heartbeat_interval, Duration::from_secs(30));
        assert_eq!(config.websocket.max_message_size, DEFAULT_MAX_MESSAGE_SIZE);
    }

    #[test]
    fn test_server_config_with_websocket() {
        let ws_config = WebSocketConfig::new()
            .with_max_message_size(512 * 1024)
            .with_max_violations(10);

        let config = ServerConfig::default()
            .with_websocket_config(ws_config);

        assert_eq!(config.websocket.max_message_size, 512 * 1024);
        assert_eq!(config.websocket.max_violations, 10);
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
