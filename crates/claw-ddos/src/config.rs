//! `DDoS` protection configuration.

use std::collections::HashSet;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Configuration for connection-level protections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// Maximum concurrent connections per IP address.
    pub max_per_ip: u32,
    /// Timeout for incomplete handshakes (slow loris protection).
    pub handshake_timeout: Duration,
    /// Timeout for idle connections.
    pub idle_timeout: Duration,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            max_per_ip: 10,
            handshake_timeout: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(300),
        }
    }
}

/// Configuration for bandwidth limiting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthConfig {
    /// Maximum bytes per second per connection.
    pub bytes_per_second: u64,
    /// Token bucket burst size (allows short bursts).
    pub burst_size: u64,
    /// Whether to enable bandwidth limiting.
    pub enabled: bool,
}

impl Default for BandwidthConfig {
    fn default() -> Self {
        Self {
            bytes_per_second: 10 * 1024 * 1024, // 10 MB/s
            burst_size: 1024 * 1024,            // 1 MB burst
            enabled: true,
        }
    }
}

/// Configuration for request rate limiting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum requests per second per client.
    pub requests_per_second: u32,
    /// Window size for sliding window rate limiter.
    pub window_size: Duration,
    /// Whether to enable rate limiting.
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 100,
            window_size: Duration::from_secs(1),
            enabled: true,
        }
    }
}

/// Configuration for compute cost limiting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeCostConfig {
    /// Maximum compute cost units per minute.
    pub cost_per_minute: u64,
    /// Cost reset interval.
    pub reset_interval: Duration,
    /// Whether to enable compute cost limiting.
    pub enabled: bool,
}

impl Default for ComputeCostConfig {
    fn default() -> Self {
        Self {
            cost_per_minute: 10000,
            reset_interval: Duration::from_secs(60),
            enabled: true,
        }
    }
}

/// Configuration for IP blocklist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlocklistConfig {
    /// Default block duration for temporary bans.
    pub default_block_duration: Duration,
    /// Maximum block duration for escalated bans.
    pub max_block_duration: Duration,
    /// Whether to persist blocklist across restarts.
    pub persist: bool,
    /// Path to persistence file (if persist is true).
    pub persist_path: Option<String>,
}

impl Default for BlocklistConfig {
    fn default() -> Self {
        Self {
            default_block_duration: Duration::from_secs(300),  // 5 minutes
            max_block_duration: Duration::from_secs(86400),    // 24 hours
            persist: false,
            persist_path: None,
        }
    }
}

/// Configuration for geographic blocking.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct GeoConfig {
    /// Whether geographic blocking is enabled.
    pub enabled: bool,
    /// Allowed country codes (if set, only these are allowed).
    pub allowed_countries: Option<HashSet<String>>,
    /// Blocked country codes (if set, these are blocked).
    pub blocked_countries: Option<HashSet<String>>,
}


/// Configuration for reputation tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationConfig {
    /// Initial reputation score for new IPs.
    pub initial_score: i32,
    /// Minimum reputation score (below this, IP is blocked).
    pub min_score: i32,
    /// Score decay rate (points recovered per hour).
    pub decay_rate: i32,
    /// Penalty for rate limit violations.
    pub rate_limit_penalty: i32,
    /// Penalty for connection limit violations.
    pub connection_penalty: i32,
    /// Penalty for malformed requests.
    pub malformed_penalty: i32,
    /// Penalty for auth failures.
    pub auth_failure_penalty: i32,
    /// Whether to enable reputation tracking.
    pub enabled: bool,
}

impl Default for ReputationConfig {
    fn default() -> Self {
        Self {
            initial_score: 100,
            min_score: 0,
            decay_rate: 10,
            rate_limit_penalty: 5,
            connection_penalty: 10,
            malformed_penalty: 15,
            auth_failure_penalty: 20,
            enabled: true,
        }
    }
}

/// Configuration for automatic escalation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationConfig {
    /// Number of violations before temporary ban.
    pub temp_ban_threshold: u32,
    /// Duration of temporary ban.
    pub temp_ban_duration: Duration,
    /// Number of temp bans before permanent ban.
    pub permanent_ban_threshold: u32,
    /// Whether to enable automatic escalation.
    pub enabled: bool,
}

impl Default for EscalationConfig {
    fn default() -> Self {
        Self {
            temp_ban_threshold: 5,
            temp_ban_duration: Duration::from_secs(300),  // 5 minutes
            permanent_ban_threshold: 3,
            enabled: true,
        }
    }
}

/// Per-endpoint configuration overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EndpointConfig {
    /// Path pattern for the endpoint (e.g., "/api/v1/*").
    pub path_pattern: String,
    /// Override requests per second (if set).
    pub requests_per_second: Option<u32>,
    /// Override compute cost per request (if set).
    pub compute_cost: Option<u64>,
    /// Override max request size (if set).
    pub max_request_size: Option<usize>,
}

/// Main `DDoS` protection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct DdosConfig {
    /// Connection-level protection settings.
    pub connection: ConnectionConfig,
    /// Bandwidth limiting settings.
    pub bandwidth: BandwidthConfig,
    /// Rate limiting settings.
    pub rate_limit: RateLimitConfig,
    /// Compute cost limiting settings.
    pub compute_cost: ComputeCostConfig,
    /// IP blocklist settings.
    pub blocklist: BlocklistConfig,
    /// Geographic blocking settings.
    pub geo: GeoConfig,
    /// Reputation tracking settings.
    pub reputation: ReputationConfig,
    /// Escalation settings.
    pub escalation: EscalationConfig,
    /// Per-endpoint overrides.
    pub endpoints: Vec<EndpointConfig>,
    /// IPs to always allow (whitelist).
    pub whitelist: HashSet<String>,
}


impl DdosConfig {
    /// Create a new builder for `DDoS` configuration.
    #[must_use]
    pub fn builder() -> DdosConfigBuilder {
        DdosConfigBuilder::default()
    }

    /// Check if an IP is whitelisted.
    #[must_use]
    pub fn is_whitelisted(&self, ip: &str) -> bool {
        self.whitelist.contains(ip)
    }

    /// Get endpoint-specific rate limit, or default.
    #[must_use]
    pub fn rate_limit_for_endpoint(&self, path: &str) -> u32 {
        for endpoint in &self.endpoints {
            if path_matches(&endpoint.path_pattern, path) {
                if let Some(rps) = endpoint.requests_per_second {
                    return rps;
                }
            }
        }
        self.rate_limit.requests_per_second
    }

    /// Get endpoint-specific compute cost, or default.
    #[must_use]
    pub fn compute_cost_for_endpoint(&self, path: &str) -> u64 {
        for endpoint in &self.endpoints {
            if path_matches(&endpoint.path_pattern, path) {
                if let Some(cost) = endpoint.compute_cost {
                    return cost;
                }
            }
        }
        1 // Default cost of 1
    }
}

/// Builder for `DdosConfig`.
#[derive(Debug, Clone, Default)]
pub struct DdosConfigBuilder {
    config: DdosConfig,
}

impl DdosConfigBuilder {
    /// Set connection configuration.
    #[must_use]
    pub fn connection(mut self, config: ConnectionConfig) -> Self {
        self.config.connection = config;
        self
    }

    /// Set bandwidth configuration.
    #[must_use]
    pub fn bandwidth(mut self, config: BandwidthConfig) -> Self {
        self.config.bandwidth = config;
        self
    }

    /// Set rate limit configuration.
    #[must_use]
    pub fn rate_limit(mut self, config: RateLimitConfig) -> Self {
        self.config.rate_limit = config;
        self
    }

    /// Set compute cost configuration.
    #[must_use]
    pub fn compute_cost(mut self, config: ComputeCostConfig) -> Self {
        self.config.compute_cost = config;
        self
    }

    /// Set blocklist configuration.
    #[must_use]
    pub fn blocklist(mut self, config: BlocklistConfig) -> Self {
        self.config.blocklist = config;
        self
    }

    /// Set geographic blocking configuration.
    #[must_use]
    pub fn geo(mut self, config: GeoConfig) -> Self {
        self.config.geo = config;
        self
    }

    /// Set reputation configuration.
    #[must_use]
    pub fn reputation(mut self, config: ReputationConfig) -> Self {
        self.config.reputation = config;
        self
    }

    /// Set escalation configuration.
    #[must_use]
    pub fn escalation(mut self, config: EscalationConfig) -> Self {
        self.config.escalation = config;
        self
    }

    /// Add an endpoint configuration.
    #[must_use]
    pub fn endpoint(mut self, config: EndpointConfig) -> Self {
        self.config.endpoints.push(config);
        self
    }

    /// Add a whitelisted IP.
    #[must_use]
    pub fn whitelist_ip(mut self, ip: impl Into<String>) -> Self {
        self.config.whitelist.insert(ip.into());
        self
    }

    /// Build the configuration.
    #[must_use]
    pub fn build(self) -> DdosConfig {
        self.config
    }
}

/// Simple path matching with wildcard support.
fn path_matches(pattern: &str, path: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        path.starts_with(prefix)
    } else {
        pattern == path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DdosConfig::default();
        
        assert_eq!(config.connection.max_per_ip, 10);
        assert_eq!(config.rate_limit.requests_per_second, 100);
        assert!(config.rate_limit.enabled);
        assert!(!config.geo.enabled);
    }

    #[test]
    fn test_builder_pattern() {
        let config = DdosConfig::builder()
            .connection(ConnectionConfig {
                max_per_ip: 5,
                ..ConnectionConfig::default()
            })
            .whitelist_ip("127.0.0.1")
            .build();

        assert_eq!(config.connection.max_per_ip, 5);
        assert!(config.is_whitelisted("127.0.0.1"));
        assert!(!config.is_whitelisted("192.168.1.1"));
    }

    #[test]
    fn test_endpoint_overrides() {
        let config = DdosConfig::builder()
            .endpoint(EndpointConfig {
                path_pattern: "/api/expensive/*".into(),
                requests_per_second: Some(10),
                compute_cost: Some(100),
                max_request_size: None,
            })
            .build();

        // Default endpoints use global config
        assert_eq!(config.rate_limit_for_endpoint("/api/cheap"), 100);
        assert_eq!(config.compute_cost_for_endpoint("/api/cheap"), 1);

        // Configured endpoint uses override
        assert_eq!(config.rate_limit_for_endpoint("/api/expensive/foo"), 10);
        assert_eq!(config.compute_cost_for_endpoint("/api/expensive/bar"), 100);
    }

    #[test]
    fn test_path_matching() {
        assert!(path_matches("/api/*", "/api/foo"));
        assert!(path_matches("/api/*", "/api/foo/bar"));
        assert!(!path_matches("/api/*", "/other/foo"));
        assert!(path_matches("/exact", "/exact"));
        assert!(!path_matches("/exact", "/exact/more"));
    }

    #[test]
    fn test_connection_config_default() {
        let config = ConnectionConfig::default();
        assert_eq!(config.max_per_ip, 10);
        assert_eq!(config.handshake_timeout, Duration::from_secs(10));
        assert_eq!(config.idle_timeout, Duration::from_secs(300));
    }

    #[test]
    fn test_bandwidth_config_default() {
        let config = BandwidthConfig::default();
        assert_eq!(config.bytes_per_second, 10 * 1024 * 1024);
        assert_eq!(config.burst_size, 1024 * 1024);
        assert!(config.enabled);
    }

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert_eq!(config.requests_per_second, 100);
        assert_eq!(config.window_size, Duration::from_secs(1));
        assert!(config.enabled);
    }

    #[test]
    fn test_escalation_config_default() {
        let config = EscalationConfig::default();
        assert_eq!(config.temp_ban_threshold, 5);
        assert_eq!(config.permanent_ban_threshold, 3);
        assert!(config.enabled);
    }

    #[test]
    fn test_reputation_config_default() {
        let config = ReputationConfig::default();
        assert_eq!(config.initial_score, 100);
        assert_eq!(config.min_score, 0);
        assert_eq!(config.rate_limit_penalty, 5);
    }

    #[test]
    fn test_geo_config_default() {
        let config = GeoConfig::default();
        assert!(!config.enabled);
        assert!(config.allowed_countries.is_none());
        assert!(config.blocked_countries.is_none());
    }

    #[test]
    fn test_blocklist_config_default() {
        let config = BlocklistConfig::default();
        assert_eq!(config.default_block_duration, Duration::from_secs(300));
        assert!(!config.persist);
    }
}
