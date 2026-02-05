//! Dashboard server configuration.

use std::net::SocketAddr;
use std::time::Duration;

/// Configuration for the dashboard server.
#[derive(Debug, Clone)]
pub struct DashboardConfig {
    /// Address to bind the HTTP server to.
    pub bind_addr: SocketAddr,
    /// Maximum WebSocket connections allowed.
    pub max_ws_connections: usize,
    /// WebSocket ping interval for keepalive.
    pub ws_ping_interval: Duration,
    /// Maximum age for metrics data.
    pub metrics_max_age: Duration,
    /// CORS allowed origins (empty means all).
    pub cors_origins: Vec<String>,
    /// Enable debug endpoints.
    pub debug_endpoints: bool,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:8080".parse().unwrap_or_else(|_| {
                SocketAddr::from(([0, 0, 0, 0], 8080))
            }),
            max_ws_connections: 1000,
            ws_ping_interval: Duration::from_secs(30),
            metrics_max_age: Duration::from_secs(300),
            cors_origins: Vec::new(),
            debug_endpoints: false,
        }
    }
}

impl DashboardConfig {
    /// Create a new configuration with the specified bind address.
    #[must_use]
    pub fn new(bind_addr: SocketAddr) -> Self {
        Self {
            bind_addr,
            ..Self::default()
        }
    }

    /// Set the maximum WebSocket connections.
    #[must_use]
    pub const fn with_max_ws_connections(mut self, max: usize) -> Self {
        self.max_ws_connections = max;
        self
    }

    /// Set the WebSocket ping interval.
    #[must_use]
    pub const fn with_ws_ping_interval(mut self, interval: Duration) -> Self {
        self.ws_ping_interval = interval;
        self
    }

    /// Set the metrics maximum age.
    #[must_use]
    pub const fn with_metrics_max_age(mut self, max_age: Duration) -> Self {
        self.metrics_max_age = max_age;
        self
    }

    /// Add a CORS allowed origin.
    #[must_use]
    pub fn with_cors_origin(mut self, origin: impl Into<String>) -> Self {
        self.cors_origins.push(origin.into());
        self
    }

    /// Enable debug endpoints.
    #[must_use]
    pub const fn with_debug_endpoints(mut self, enabled: bool) -> Self {
        self.debug_endpoints = enabled;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_default_config() {
        let config = DashboardConfig::default();

        assert_eq!(config.bind_addr.port(), 8080);
        assert_eq!(config.max_ws_connections, 1000);
        assert_eq!(config.ws_ping_interval, Duration::from_secs(30));
        assert_eq!(config.metrics_max_age, Duration::from_secs(300));
        assert!(config.cors_origins.is_empty());
        assert!(!config.debug_endpoints);
    }

    #[test]
    fn test_config_new() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9000);
        let config = DashboardConfig::new(addr);

        assert_eq!(config.bind_addr, addr);
        assert_eq!(config.max_ws_connections, 1000);
    }

    #[test]
    fn test_config_builder() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9000);
        let config = DashboardConfig::new(addr)
            .with_max_ws_connections(500)
            .with_ws_ping_interval(Duration::from_secs(60))
            .with_metrics_max_age(Duration::from_secs(600))
            .with_cors_origin("http://localhost:3000")
            .with_cors_origin("https://dashboard.example.com")
            .with_debug_endpoints(true);

        assert_eq!(config.max_ws_connections, 500);
        assert_eq!(config.ws_ping_interval, Duration::from_secs(60));
        assert_eq!(config.metrics_max_age, Duration::from_secs(600));
        assert_eq!(config.cors_origins.len(), 2);
        assert!(config.cors_origins.contains(&"http://localhost:3000".to_string()));
        assert!(config.debug_endpoints);
    }
}
