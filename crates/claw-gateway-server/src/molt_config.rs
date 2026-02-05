//! MOLT network configuration.
//!
//! Configuration for bootstrap nodes, staking parameters, and network settings.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Default announce interval (5 minutes).
pub const DEFAULT_ANNOUNCE_INTERVAL_SECS: u64 = 300;

/// Default escrow timeout (24 hours).
pub const DEFAULT_ESCROW_TIMEOUT_SECS: u64 = 86400;

/// MOLT network configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoltConfig {
    /// Bootstrap nodes for initial network discovery.
    pub bootstrap_nodes: Vec<BootstrapNode>,
    /// Network region identifier (e.g., "us-west-2", "eu-central-1").
    pub region: Option<String>,
    /// Capacity announcement interval.
    #[serde(default = "default_announce_interval")]
    pub announce_interval: Duration,
    /// Minimum stake required to provide compute (in lamports).
    #[serde(default)]
    pub min_provider_stake: u64,
    /// Escrow timeout for jobs.
    #[serde(default = "default_escrow_timeout")]
    pub escrow_timeout: Duration,
    /// Enable auto-join on startup.
    #[serde(default = "default_auto_join")]
    pub auto_join: bool,
}

fn default_announce_interval() -> Duration {
    Duration::from_secs(DEFAULT_ANNOUNCE_INTERVAL_SECS)
}

fn default_escrow_timeout() -> Duration {
    Duration::from_secs(DEFAULT_ESCROW_TIMEOUT_SECS)
}

fn default_auto_join() -> bool {
    true
}

impl Default for MoltConfig {
    fn default() -> Self {
        Self {
            bootstrap_nodes: Vec::new(),
            region: None,
            announce_interval: default_announce_interval(),
            min_provider_stake: 0,
            escrow_timeout: default_escrow_timeout(),
            auto_join: true,
        }
    }
}

impl MoltConfig {
    /// Create a new MOLT configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a bootstrap node.
    #[must_use]
    pub fn with_bootstrap_node(mut self, address: impl Into<String>) -> Self {
        self.bootstrap_nodes.push(BootstrapNode::new(address));
        self
    }

    /// Add multiple bootstrap nodes.
    #[must_use]
    pub fn with_bootstrap_nodes(mut self, addresses: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for addr in addresses {
            self.bootstrap_nodes.push(BootstrapNode::new(addr));
        }
        self
    }

    /// Set the network region.
    #[must_use]
    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    /// Set the announcement interval.
    #[must_use]
    pub fn with_announce_interval(mut self, interval: Duration) -> Self {
        self.announce_interval = interval;
        self
    }

    /// Set the minimum provider stake.
    #[must_use]
    pub fn with_min_stake(mut self, lamports: u64) -> Self {
        self.min_provider_stake = lamports;
        self
    }

    /// Set auto-join behavior.
    #[must_use]
    pub fn with_auto_join(mut self, auto_join: bool) -> Self {
        self.auto_join = auto_join;
        self
    }

    /// Get bootstrap node addresses as strings.
    #[must_use]
    pub fn bootstrap_addresses(&self) -> Vec<String> {
        self.bootstrap_nodes.iter().map(|n| n.address.clone()).collect()
    }

    /// Check if any bootstrap nodes are configured.
    #[must_use]
    pub fn has_bootstrap_nodes(&self) -> bool {
        !self.bootstrap_nodes.is_empty()
    }
}

/// A bootstrap node for network discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapNode {
    /// Node address (e.g., "1.2.3.4:8080" or "node.example.com:8080").
    pub address: String,
    /// Optional human-readable name.
    pub name: Option<String>,
    /// Whether this is a trusted/official node.
    #[serde(default)]
    pub trusted: bool,
}

impl BootstrapNode {
    /// Create a new bootstrap node with the given address.
    #[must_use]
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            name: None,
            trusted: false,
        }
    }

    /// Create a trusted bootstrap node.
    #[must_use]
    pub fn trusted(address: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            name: Some(name.into()),
            trusted: true,
        }
    }
}

/// Well-known official MOLT bootstrap nodes.
pub mod official_nodes {
    use super::BootstrapNode;

    /// US West bootstrap node.
    #[must_use]
    pub fn us_west() -> BootstrapNode {
        BootstrapNode::trusted("bootstrap-us-west.molt.network:8443", "MOLT US West")
    }

    /// US East bootstrap node.
    #[must_use]
    pub fn us_east() -> BootstrapNode {
        BootstrapNode::trusted("bootstrap-us-east.molt.network:8443", "MOLT US East")
    }

    /// EU Central bootstrap node.
    #[must_use]
    pub fn eu_central() -> BootstrapNode {
        BootstrapNode::trusted("bootstrap-eu-central.molt.network:8443", "MOLT EU Central")
    }

    /// Asia Pacific bootstrap node.
    #[must_use]
    pub fn ap_southeast() -> BootstrapNode {
        BootstrapNode::trusted("bootstrap-ap-southeast.molt.network:8443", "MOLT AP Southeast")
    }

    /// Get all official bootstrap nodes.
    #[must_use]
    pub fn all() -> Vec<BootstrapNode> {
        vec![us_west(), us_east(), eu_central(), ap_southeast()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MoltConfig::default();
        assert!(config.bootstrap_nodes.is_empty());
        assert!(config.region.is_none());
        assert!(config.auto_join);
    }

    #[test]
    fn test_config_builder() {
        let config = MoltConfig::new()
            .with_bootstrap_node("node1.example.com:8080")
            .with_bootstrap_node("node2.example.com:8080")
            .with_region("us-west-2")
            .with_min_stake(1_000_000_000);

        assert_eq!(config.bootstrap_nodes.len(), 2);
        assert_eq!(config.region, Some("us-west-2".into()));
        assert_eq!(config.min_provider_stake, 1_000_000_000);
    }

    #[test]
    fn test_bootstrap_addresses() {
        let config = MoltConfig::new()
            .with_bootstrap_nodes(["a:80", "b:80", "c:80"]);

        let addrs = config.bootstrap_addresses();
        assert_eq!(addrs, vec!["a:80", "b:80", "c:80"]);
    }

    #[test]
    fn test_official_nodes() {
        let nodes = official_nodes::all();
        assert_eq!(nodes.len(), 4);
        assert!(nodes.iter().all(|n| n.trusted));
    }

    #[test]
    fn test_bootstrap_node_creation() {
        let node = BootstrapNode::new("localhost:8080");
        assert_eq!(node.address, "localhost:8080");
        assert!(!node.trusted);

        let trusted = BootstrapNode::trusted("official.molt.network:8443", "Official");
        assert!(trusted.trusted);
        assert_eq!(trusted.name, Some("Official".into()));
    }
}
