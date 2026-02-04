//! Peer diversity enforcement for eclipse attack mitigation.
//!
//! This module provides mechanisms to ensure peer diversity in the P2P network,
//! preventing eclipse attacks where an attacker surrounds a victim with malicious peers.
//!
//! ## Key Features
//!
//! - **Subnet limiting**: Restricts connections per IP subnet (e.g., max 2 per /24)
//! - **AS number limiting**: Restricts connections per Autonomous System (when available)
//! - **Geographic diversity**: Prefers peers from different regions
//!
//! ## Example
//!
//! ```rust
//! use molt_p2p::diversity::{PeerDiversityConfig, PeerDiversityTracker, DiversityResult};
//! use std::net::{IpAddr, Ipv4Addr};
//!
//! let config = PeerDiversityConfig::default();
//! let mut tracker = PeerDiversityTracker::new(config);
//!
//! let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
//! match tracker.check_and_add(ip, None, None) {
//!     DiversityResult::Accepted => println!("Peer accepted"),
//!     DiversityResult::RejectedSubnetLimit { subnet, current, limit } => {
//!         println!("Rejected: subnet {} has {}/{} peers", subnet, current, limit);
//!     }
//!     _ => {}
//! }
//! ```

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Configuration for peer diversity requirements.
///
/// These settings control how aggressively the network enforces peer diversity
/// to prevent eclipse attacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerDiversityConfig {
    /// Maximum connections allowed from the same IPv4 /24 subnet.
    /// Default: 2
    pub max_per_ipv4_subnet: usize,

    /// Maximum connections allowed from the same IPv6 /48 prefix.
    /// Default: 2
    pub max_per_ipv6_prefix: usize,

    /// Maximum connections allowed from the same Autonomous System.
    /// Set to 0 to disable AS-based limiting.
    /// Default: 5
    pub max_per_asn: usize,

    /// Whether to enable geographic diversity preferences.
    /// When enabled, the system prefers peers from different geographic regions.
    /// Default: true
    pub enable_geo_diversity: bool,

    /// Minimum number of distinct geographic regions to maintain.
    /// Only applies when `enable_geo_diversity` is true.
    /// Default: 3
    pub min_geo_regions: usize,

    /// Whether to allow connections from private/local IP ranges.
    /// Useful for testing but should be disabled in production.
    /// Default: true
    pub allow_private_ips: bool,

    /// Whether diversity checks are enabled at all.
    /// Useful for testing environments.
    /// Default: true
    pub enabled: bool,
}

impl Default for PeerDiversityConfig {
    fn default() -> Self {
        Self {
            max_per_ipv4_subnet: 2,
            max_per_ipv6_prefix: 2,
            max_per_asn: 5,
            enable_geo_diversity: true,
            min_geo_regions: 3,
            allow_private_ips: true,
            enabled: true,
        }
    }
}

impl PeerDiversityConfig {
    /// Creates a new configuration with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a strict configuration for production use.
    #[must_use]
    pub fn strict() -> Self {
        Self {
            max_per_ipv4_subnet: 1,
            max_per_ipv6_prefix: 1,
            max_per_asn: 3,
            enable_geo_diversity: true,
            min_geo_regions: 5,
            allow_private_ips: false,
            enabled: true,
        }
    }

    /// Creates a permissive configuration for testing.
    #[must_use]
    pub fn permissive() -> Self {
        Self {
            max_per_ipv4_subnet: 10,
            max_per_ipv6_prefix: 10,
            max_per_asn: 20,
            enable_geo_diversity: false,
            min_geo_regions: 0,
            allow_private_ips: true,
            enabled: true,
        }
    }

    /// Creates a configuration with diversity checks disabled.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }

    /// Sets the maximum connections per IPv4 /24 subnet.
    #[must_use]
    pub const fn with_max_per_ipv4_subnet(mut self, max: usize) -> Self {
        self.max_per_ipv4_subnet = max;
        self
    }

    /// Sets the maximum connections per IPv6 /48 prefix.
    #[must_use]
    pub const fn with_max_per_ipv6_prefix(mut self, max: usize) -> Self {
        self.max_per_ipv6_prefix = max;
        self
    }

    /// Sets the maximum connections per AS number.
    #[must_use]
    pub const fn with_max_per_asn(mut self, max: usize) -> Self {
        self.max_per_asn = max;
        self
    }

    /// Enables or disables geographic diversity preferences.
    #[must_use]
    pub const fn with_geo_diversity(mut self, enabled: bool) -> Self {
        self.enable_geo_diversity = enabled;
        self
    }

    /// Sets whether private IPs are allowed.
    #[must_use]
    pub const fn with_private_ips(mut self, allowed: bool) -> Self {
        self.allow_private_ips = allowed;
        self
    }

    /// Enables or disables all diversity checks.
    #[must_use]
    pub const fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Represents an IPv4 /24 subnet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ipv4Subnet {
    /// First three octets of the subnet (e.g., [192, 168, 1] for 192.168.1.0/24)
    octets: [u8; 3],
}

impl Ipv4Subnet {
    /// Creates a subnet from an IPv4 address.
    #[must_use]
    pub fn from_addr(addr: Ipv4Addr) -> Self {
        let octets = addr.octets();
        Self {
            octets: [octets[0], octets[1], octets[2]],
        }
    }

    /// Returns the subnet as a string (e.g., "192.168.1.0/24").
    #[must_use]
    pub fn to_cidr_string(&self) -> String {
        format!("{}.{}.{}.0/24", self.octets[0], self.octets[1], self.octets[2])
    }
}

impl std::fmt::Display for Ipv4Subnet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_cidr_string())
    }
}

/// Represents an IPv6 /48 prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ipv6Prefix {
    /// First 48 bits (6 bytes) of the prefix
    bytes: [u8; 6],
}

impl Ipv6Prefix {
    /// Creates a prefix from an IPv6 address.
    #[must_use]
    pub fn from_addr(addr: Ipv6Addr) -> Self {
        let segments = addr.segments();
        Self {
            bytes: [
                (segments[0] >> 8) as u8,
                segments[0] as u8,
                (segments[1] >> 8) as u8,
                segments[1] as u8,
                (segments[2] >> 8) as u8,
                segments[2] as u8,
            ],
        }
    }

    /// Returns the prefix as a string.
    #[must_use]
    pub fn to_cidr_string(&self) -> String {
        format!(
            "{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}::/48",
            self.bytes[0], self.bytes[1],
            self.bytes[2], self.bytes[3],
            self.bytes[4], self.bytes[5]
        )
    }
}

impl std::fmt::Display for Ipv6Prefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_cidr_string())
    }
}

/// Autonomous System Number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Asn(pub u32);

impl std::fmt::Display for Asn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AS{}", self.0)
    }
}

/// Geographic region identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GeoRegion(pub String);

impl std::fmt::Display for GeoRegion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Result of a diversity check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiversityResult {
    /// The peer was accepted and added to tracking.
    Accepted,

    /// Rejected: too many peers from the same IPv4 subnet.
    RejectedSubnetLimit {
        /// The subnet that exceeded the limit.
        subnet: String,
        /// Current count of peers from this subnet.
        current: usize,
        /// Maximum allowed.
        limit: usize,
    },

    /// Rejected: too many peers from the same IPv6 prefix.
    RejectedPrefixLimit {
        /// The prefix that exceeded the limit.
        prefix: String,
        /// Current count of peers from this prefix.
        current: usize,
        /// Maximum allowed.
        limit: usize,
    },

    /// Rejected: too many peers from the same AS.
    RejectedAsnLimit {
        /// The AS that exceeded the limit.
        asn: String,
        /// Current count of peers from this AS.
        current: usize,
        /// Maximum allowed.
        limit: usize,
    },

    /// Rejected: private IP not allowed.
    RejectedPrivateIp,

    /// Diversity checks are disabled; peer auto-accepted.
    Disabled,
}

impl DiversityResult {
    /// Returns true if the peer was accepted.
    #[must_use]
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted | Self::Disabled)
    }

    /// Returns true if the peer was rejected.
    #[must_use]
    pub fn is_rejected(&self) -> bool {
        !self.is_accepted()
    }

    /// Returns a human-readable rejection reason, if rejected.
    #[must_use]
    pub fn rejection_reason(&self) -> Option<String> {
        match self {
            Self::Accepted | Self::Disabled => None,
            Self::RejectedSubnetLimit { subnet, current, limit } => {
                Some(format!("subnet {} has {}/{} peers", subnet, current, limit))
            }
            Self::RejectedPrefixLimit { prefix, current, limit } => {
                Some(format!("prefix {} has {}/{} peers", prefix, current, limit))
            }
            Self::RejectedAsnLimit { asn, current, limit } => {
                Some(format!("{} has {}/{} peers", asn, current, limit))
            }
            Self::RejectedPrivateIp => Some("private IPs not allowed".to_string()),
        }
    }
}

/// Metadata about a tracked peer for diversity purposes.
#[derive(Debug, Clone)]
pub struct PeerDiversityInfo {
    /// The peer's IP address.
    pub ip: IpAddr,
    /// The peer's AS number, if known.
    pub asn: Option<Asn>,
    /// The peer's geographic region, if known.
    pub geo_region: Option<GeoRegion>,
}

/// Tracks peer diversity metrics to enforce eclipse attack mitigations.
#[derive(Debug)]
pub struct PeerDiversityTracker {
    config: PeerDiversityConfig,

    /// Count of peers per IPv4 /24 subnet.
    ipv4_subnet_counts: HashMap<Ipv4Subnet, usize>,

    /// Count of peers per IPv6 /48 prefix.
    ipv6_prefix_counts: HashMap<Ipv6Prefix, usize>,

    /// Count of peers per AS number.
    asn_counts: HashMap<Asn, usize>,

    /// Count of peers per geographic region.
    geo_region_counts: HashMap<GeoRegion, usize>,

    /// All tracked peer IPs (for removal).
    tracked_peers: HashMap<IpAddr, PeerDiversityInfo>,
}

impl PeerDiversityTracker {
    /// Creates a new diversity tracker with the given configuration.
    #[must_use]
    pub fn new(config: PeerDiversityConfig) -> Self {
        Self {
            config,
            ipv4_subnet_counts: HashMap::new(),
            ipv6_prefix_counts: HashMap::new(),
            asn_counts: HashMap::new(),
            geo_region_counts: HashMap::new(),
            tracked_peers: HashMap::new(),
        }
    }

    /// Creates a new tracker with default configuration.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(PeerDiversityConfig::default())
    }

    /// Returns the current configuration.
    #[must_use]
    pub fn config(&self) -> &PeerDiversityConfig {
        &self.config
    }

    /// Updates the configuration.
    pub fn set_config(&mut self, config: PeerDiversityConfig) {
        self.config = config;
    }

    /// Checks if a peer can be added without violating diversity constraints.
    ///
    /// This does NOT add the peer to tracking; use `check_and_add` for that.
    #[must_use]
    pub fn check(&self, ip: IpAddr, asn: Option<Asn>) -> DiversityResult {
        if !self.config.enabled {
            return DiversityResult::Disabled;
        }

        // Check private IP restriction
        if !self.config.allow_private_ips && is_private_ip(ip) {
            return DiversityResult::RejectedPrivateIp;
        }

        // Check subnet/prefix limits
        match ip {
            IpAddr::V4(v4) => {
                let subnet = Ipv4Subnet::from_addr(v4);
                let current = self.ipv4_subnet_counts.get(&subnet).copied().unwrap_or(0);
                if current >= self.config.max_per_ipv4_subnet {
                    return DiversityResult::RejectedSubnetLimit {
                        subnet: subnet.to_string(),
                        current,
                        limit: self.config.max_per_ipv4_subnet,
                    };
                }
            }
            IpAddr::V6(v6) => {
                let prefix = Ipv6Prefix::from_addr(v6);
                let current = self.ipv6_prefix_counts.get(&prefix).copied().unwrap_or(0);
                if current >= self.config.max_per_ipv6_prefix {
                    return DiversityResult::RejectedPrefixLimit {
                        prefix: prefix.to_string(),
                        current,
                        limit: self.config.max_per_ipv6_prefix,
                    };
                }
            }
        }

        // Check ASN limit
        if self.config.max_per_asn > 0 {
            if let Some(asn) = asn {
                let current = self.asn_counts.get(&asn).copied().unwrap_or(0);
                if current >= self.config.max_per_asn {
                    return DiversityResult::RejectedAsnLimit {
                        asn: asn.to_string(),
                        current,
                        limit: self.config.max_per_asn,
                    };
                }
            }
        }

        DiversityResult::Accepted
    }

    /// Checks if a peer can be added and, if so, adds it to tracking.
    ///
    /// Returns the result of the diversity check.
    pub fn check_and_add(
        &mut self,
        ip: IpAddr,
        asn: Option<Asn>,
        geo_region: Option<GeoRegion>,
    ) -> DiversityResult {
        let result = self.check(ip, asn);

        if result.is_accepted() && self.config.enabled {
            self.add_peer_internal(ip, asn, geo_region);
        }

        result
    }

    /// Adds a peer to tracking without checking diversity constraints.
    ///
    /// Use this only when you've already verified the peer is acceptable.
    pub fn add_unchecked(&mut self, ip: IpAddr, asn: Option<Asn>, geo_region: Option<GeoRegion>) {
        if self.config.enabled {
            self.add_peer_internal(ip, asn, geo_region);
        }
    }

    fn add_peer_internal(&mut self, ip: IpAddr, asn: Option<Asn>, geo_region: Option<GeoRegion>) {
        // Update subnet/prefix counts
        match ip {
            IpAddr::V4(v4) => {
                let subnet = Ipv4Subnet::from_addr(v4);
                *self.ipv4_subnet_counts.entry(subnet).or_insert(0) += 1;
            }
            IpAddr::V6(v6) => {
                let prefix = Ipv6Prefix::from_addr(v6);
                *self.ipv6_prefix_counts.entry(prefix).or_insert(0) += 1;
            }
        }

        // Update ASN count
        if let Some(asn) = asn {
            *self.asn_counts.entry(asn).or_insert(0) += 1;
        }

        // Update geo region count
        if let Some(ref region) = geo_region {
            *self.geo_region_counts.entry(region.clone()).or_insert(0) += 1;
        }

        // Track the peer
        self.tracked_peers.insert(
            ip,
            PeerDiversityInfo {
                ip,
                asn,
                geo_region,
            },
        );
    }

    /// Removes a peer from tracking.
    ///
    /// Returns `true` if the peer was being tracked.
    pub fn remove(&mut self, ip: IpAddr) -> bool {
        if let Some(info) = self.tracked_peers.remove(&ip) {
            // Decrement subnet/prefix counts
            match info.ip {
                IpAddr::V4(v4) => {
                    let subnet = Ipv4Subnet::from_addr(v4);
                    if let Some(count) = self.ipv4_subnet_counts.get_mut(&subnet) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            self.ipv4_subnet_counts.remove(&subnet);
                        }
                    }
                }
                IpAddr::V6(v6) => {
                    let prefix = Ipv6Prefix::from_addr(v6);
                    if let Some(count) = self.ipv6_prefix_counts.get_mut(&prefix) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            self.ipv6_prefix_counts.remove(&prefix);
                        }
                    }
                }
            }

            // Decrement ASN count
            if let Some(asn) = info.asn {
                if let Some(count) = self.asn_counts.get_mut(&asn) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.asn_counts.remove(&asn);
                    }
                }
            }

            // Decrement geo region count
            if let Some(region) = info.geo_region {
                if let Some(count) = self.geo_region_counts.get_mut(&region) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.geo_region_counts.remove(&region);
                    }
                }
            }

            true
        } else {
            false
        }
    }

    /// Returns the number of tracked peers.
    #[must_use]
    pub fn peer_count(&self) -> usize {
        self.tracked_peers.len()
    }

    /// Returns the number of distinct IPv4 subnets.
    #[must_use]
    pub fn subnet_count(&self) -> usize {
        self.ipv4_subnet_counts.len()
    }

    /// Returns the number of distinct IPv6 prefixes.
    #[must_use]
    pub fn prefix_count(&self) -> usize {
        self.ipv6_prefix_counts.len()
    }

    /// Returns the number of distinct AS numbers.
    #[must_use]
    pub fn asn_count(&self) -> usize {
        self.asn_counts.len()
    }

    /// Returns the number of distinct geographic regions.
    #[must_use]
    pub fn geo_region_count(&self) -> usize {
        self.geo_region_counts.len()
    }

    /// Returns the peer count for a specific IPv4 subnet.
    #[must_use]
    pub fn peers_in_subnet(&self, addr: Ipv4Addr) -> usize {
        let subnet = Ipv4Subnet::from_addr(addr);
        self.ipv4_subnet_counts.get(&subnet).copied().unwrap_or(0)
    }

    /// Returns the peer count for a specific IPv6 prefix.
    #[must_use]
    pub fn peers_in_prefix(&self, addr: Ipv6Addr) -> usize {
        let prefix = Ipv6Prefix::from_addr(addr);
        self.ipv6_prefix_counts.get(&prefix).copied().unwrap_or(0)
    }

    /// Returns the peer count for a specific AS.
    #[must_use]
    pub fn peers_in_asn(&self, asn: Asn) -> usize {
        self.asn_counts.get(&asn).copied().unwrap_or(0)
    }

    /// Returns statistics about current diversity.
    #[must_use]
    pub fn stats(&self) -> DiversityStats {
        let max_subnet_peers = self.ipv4_subnet_counts.values().max().copied().unwrap_or(0);
        let max_prefix_peers = self.ipv6_prefix_counts.values().max().copied().unwrap_or(0);
        let max_asn_peers = self.asn_counts.values().max().copied().unwrap_or(0);

        DiversityStats {
            total_peers: self.tracked_peers.len(),
            distinct_subnets: self.ipv4_subnet_counts.len(),
            distinct_prefixes: self.ipv6_prefix_counts.len(),
            distinct_asns: self.asn_counts.len(),
            distinct_regions: self.geo_region_counts.len(),
            max_peers_per_subnet: max_subnet_peers,
            max_peers_per_prefix: max_prefix_peers,
            max_peers_per_asn: max_asn_peers,
        }
    }

    /// Clears all tracked peers.
    pub fn clear(&mut self) {
        self.ipv4_subnet_counts.clear();
        self.ipv6_prefix_counts.clear();
        self.asn_counts.clear();
        self.geo_region_counts.clear();
        self.tracked_peers.clear();
    }

    /// Returns true if adding this peer would improve geographic diversity.
    ///
    /// Useful for prioritizing connections to underrepresented regions.
    #[must_use]
    pub fn would_improve_geo_diversity(&self, region: &GeoRegion) -> bool {
        if !self.config.enable_geo_diversity {
            return false;
        }

        // New region is always an improvement
        if !self.geo_region_counts.contains_key(region) {
            return true;
        }

        // Check if we're below minimum regions
        if self.geo_region_counts.len() < self.config.min_geo_regions {
            return true;
        }

        false
    }
}

/// Statistics about peer diversity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiversityStats {
    /// Total number of tracked peers.
    pub total_peers: usize,
    /// Number of distinct IPv4 /24 subnets.
    pub distinct_subnets: usize,
    /// Number of distinct IPv6 /48 prefixes.
    pub distinct_prefixes: usize,
    /// Number of distinct AS numbers.
    pub distinct_asns: usize,
    /// Number of distinct geographic regions.
    pub distinct_regions: usize,
    /// Maximum peers in any single subnet.
    pub max_peers_per_subnet: usize,
    /// Maximum peers in any single prefix.
    pub max_peers_per_prefix: usize,
    /// Maximum peers in any single AS.
    pub max_peers_per_asn: usize,
}

impl DiversityStats {
    /// Returns a diversity score from 0.0 to 1.0.
    ///
    /// Higher scores indicate better diversity.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn diversity_score(&self) -> f64 {
        if self.total_peers == 0 {
            return 1.0;
        }

        // Ideal: each peer is in a unique subnet/AS
        let subnet_ratio = self.distinct_subnets as f64 / self.total_peers as f64;
        let asn_ratio = if self.distinct_asns > 0 {
            self.distinct_asns as f64 / self.total_peers as f64
        } else {
            1.0 // No ASN data, assume diverse
        };

        // Average of subnet and ASN diversity
        (subnet_ratio + asn_ratio) / 2.0
    }

    /// Returns true if diversity looks healthy.
    #[must_use]
    pub fn is_healthy(&self, config: &PeerDiversityConfig) -> bool {
        self.max_peers_per_subnet <= config.max_per_ipv4_subnet
            && self.max_peers_per_prefix <= config.max_per_ipv6_prefix
            && (config.max_per_asn == 0 || self.max_peers_per_asn <= config.max_per_asn)
    }
}

/// Checks if an IP address is private/local.
#[must_use]
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== PeerDiversityConfig Tests ====================

    #[test]
    fn config_default_values() {
        let config = PeerDiversityConfig::default();
        assert_eq!(config.max_per_ipv4_subnet, 2);
        assert_eq!(config.max_per_ipv6_prefix, 2);
        assert_eq!(config.max_per_asn, 5);
        assert!(config.enable_geo_diversity);
        assert!(config.allow_private_ips);
        assert!(config.enabled);
    }

    #[test]
    fn config_strict_preset() {
        let config = PeerDiversityConfig::strict();
        assert_eq!(config.max_per_ipv4_subnet, 1);
        assert_eq!(config.max_per_asn, 3);
        assert!(!config.allow_private_ips);
    }

    #[test]
    fn config_permissive_preset() {
        let config = PeerDiversityConfig::permissive();
        assert_eq!(config.max_per_ipv4_subnet, 10);
        assert!(!config.enable_geo_diversity);
    }

    #[test]
    fn config_disabled_preset() {
        let config = PeerDiversityConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn config_builder_pattern() {
        let config = PeerDiversityConfig::new()
            .with_max_per_ipv4_subnet(3)
            .with_max_per_asn(10)
            .with_private_ips(false)
            .with_geo_diversity(false)
            .with_enabled(true);

        assert_eq!(config.max_per_ipv4_subnet, 3);
        assert_eq!(config.max_per_asn, 10);
        assert!(!config.allow_private_ips);
        assert!(!config.enable_geo_diversity);
        assert!(config.enabled);
    }

    // ==================== Subnet/Prefix Tests ====================

    #[test]
    fn ipv4_subnet_from_addr() {
        let addr = Ipv4Addr::new(192, 168, 1, 100);
        let subnet = Ipv4Subnet::from_addr(addr);
        assert_eq!(subnet.to_cidr_string(), "192.168.1.0/24");
    }

    #[test]
    fn ipv4_subnet_same_for_same_slash24() {
        let addr1 = Ipv4Addr::new(10, 0, 5, 1);
        let addr2 = Ipv4Addr::new(10, 0, 5, 254);
        let subnet1 = Ipv4Subnet::from_addr(addr1);
        let subnet2 = Ipv4Subnet::from_addr(addr2);
        assert_eq!(subnet1, subnet2);
    }

    #[test]
    fn ipv4_subnet_different_for_different_slash24() {
        let addr1 = Ipv4Addr::new(10, 0, 5, 1);
        let addr2 = Ipv4Addr::new(10, 0, 6, 1);
        let subnet1 = Ipv4Subnet::from_addr(addr1);
        let subnet2 = Ipv4Subnet::from_addr(addr2);
        assert_ne!(subnet1, subnet2);
    }

    #[test]
    fn ipv6_prefix_from_addr() {
        let addr = Ipv6Addr::new(0x2001, 0x0db8, 0x85a3, 0, 0, 0, 0, 1);
        let prefix = Ipv6Prefix::from_addr(addr);
        assert_eq!(prefix.to_cidr_string(), "2001:0db8:85a3::/48");
    }

    #[test]
    fn ipv6_prefix_same_for_same_slash48() {
        let addr1 = Ipv6Addr::new(0x2001, 0x0db8, 0x85a3, 0x0001, 0, 0, 0, 1);
        let addr2 = Ipv6Addr::new(0x2001, 0x0db8, 0x85a3, 0xffff, 0, 0, 0, 1);
        let prefix1 = Ipv6Prefix::from_addr(addr1);
        let prefix2 = Ipv6Prefix::from_addr(addr2);
        assert_eq!(prefix1, prefix2);
    }

    // ==================== DiversityResult Tests ====================

    #[test]
    fn diversity_result_accepted() {
        let result = DiversityResult::Accepted;
        assert!(result.is_accepted());
        assert!(!result.is_rejected());
        assert!(result.rejection_reason().is_none());
    }

    #[test]
    fn diversity_result_disabled() {
        let result = DiversityResult::Disabled;
        assert!(result.is_accepted());
        assert!(result.rejection_reason().is_none());
    }

    #[test]
    fn diversity_result_rejected_subnet() {
        let result = DiversityResult::RejectedSubnetLimit {
            subnet: "192.168.1.0/24".to_string(),
            current: 2,
            limit: 2,
        };
        assert!(result.is_rejected());
        assert!(result.rejection_reason().is_some());
        assert!(result.rejection_reason().unwrap().contains("192.168.1.0/24"));
    }

    // ==================== Tracker Basic Tests ====================

    #[test]
    fn tracker_creation() {
        let tracker = PeerDiversityTracker::with_defaults();
        assert_eq!(tracker.peer_count(), 0);
        assert_eq!(tracker.subnet_count(), 0);
    }

    #[test]
    fn tracker_add_single_peer() {
        let mut tracker = PeerDiversityTracker::with_defaults();
        let ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));

        let result = tracker.check_and_add(ip, None, None);
        assert!(result.is_accepted());
        assert_eq!(tracker.peer_count(), 1);
        assert_eq!(tracker.subnet_count(), 1);
    }

    #[test]
    fn tracker_add_peers_same_subnet() {
        let config = PeerDiversityConfig::default().with_max_per_ipv4_subnet(2);
        let mut tracker = PeerDiversityTracker::new(config);

        // Add first peer - should succeed
        let ip1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10));
        assert!(tracker.check_and_add(ip1, None, None).is_accepted());

        // Add second peer in same subnet - should succeed
        let ip2 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20));
        assert!(tracker.check_and_add(ip2, None, None).is_accepted());

        // Add third peer in same subnet - should fail
        let ip3 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 30));
        let result = tracker.check_and_add(ip3, None, None);
        assert!(result.is_rejected());
        assert!(matches!(result, DiversityResult::RejectedSubnetLimit { .. }));

        assert_eq!(tracker.peer_count(), 2);
        assert_eq!(tracker.subnet_count(), 1);
    }

    #[test]
    fn tracker_add_peers_different_subnets() {
        let mut tracker = PeerDiversityTracker::with_defaults();

        let ip1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10));
        let ip2 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 10));
        let ip3 = IpAddr::V4(Ipv4Addr::new(172, 16, 5, 10));

        assert!(tracker.check_and_add(ip1, None, None).is_accepted());
        assert!(tracker.check_and_add(ip2, None, None).is_accepted());
        assert!(tracker.check_and_add(ip3, None, None).is_accepted());

        assert_eq!(tracker.peer_count(), 3);
        assert_eq!(tracker.subnet_count(), 3);
    }

    #[test]
    fn tracker_asn_limiting() {
        let config = PeerDiversityConfig::default().with_max_per_asn(2);
        let mut tracker = PeerDiversityTracker::new(config);
        let asn = Some(Asn(12345));

        // Add peers from different subnets but same ASN
        let ip1 = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        let ip2 = IpAddr::V4(Ipv4Addr::new(5, 6, 7, 8));
        let ip3 = IpAddr::V4(Ipv4Addr::new(9, 10, 11, 12));

        assert!(tracker.check_and_add(ip1, asn, None).is_accepted());
        assert!(tracker.check_and_add(ip2, asn, None).is_accepted());

        // Third peer should be rejected due to ASN limit
        let result = tracker.check_and_add(ip3, asn, None);
        assert!(matches!(result, DiversityResult::RejectedAsnLimit { .. }));
    }

    #[test]
    fn tracker_remove_peer() {
        let mut tracker = PeerDiversityTracker::with_defaults();
        let ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));

        tracker.check_and_add(ip, None, None);
        assert_eq!(tracker.peer_count(), 1);

        assert!(tracker.remove(ip));
        assert_eq!(tracker.peer_count(), 0);
        assert_eq!(tracker.subnet_count(), 0);

        // Removing again returns false
        assert!(!tracker.remove(ip));
    }

    #[test]
    fn tracker_remove_restores_capacity() {
        let config = PeerDiversityConfig::default().with_max_per_ipv4_subnet(1);
        let mut tracker = PeerDiversityTracker::new(config);

        let ip1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10));
        let ip2 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20));

        // Add first peer
        assert!(tracker.check_and_add(ip1, None, None).is_accepted());

        // Second peer rejected
        assert!(tracker.check_and_add(ip2, None, None).is_rejected());

        // Remove first peer
        tracker.remove(ip1);

        // Now second peer can be added
        assert!(tracker.check_and_add(ip2, None, None).is_accepted());
    }

    #[test]
    fn tracker_private_ip_rejection() {
        let config = PeerDiversityConfig::default().with_private_ips(false);
        let mut tracker = PeerDiversityTracker::new(config);

        // Private IPs should be rejected
        let private_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10));
        let result = tracker.check_and_add(private_ip, None, None);
        assert!(matches!(result, DiversityResult::RejectedPrivateIp));

        // Public IP should be accepted
        let public_ip = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));
        assert!(tracker.check_and_add(public_ip, None, None).is_accepted());
    }

    #[test]
    fn tracker_disabled_accepts_all() {
        let config = PeerDiversityConfig::disabled();
        let mut tracker = PeerDiversityTracker::new(config);

        // Add many peers from same subnet - all should be accepted
        for i in 1..=10 {
            let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, i));
            let result = tracker.check_and_add(ip, None, None);
            assert!(matches!(result, DiversityResult::Disabled));
        }

        // Tracker should not have tracked any peers
        assert_eq!(tracker.peer_count(), 0);
    }

    #[test]
    fn tracker_ipv6_support() {
        let config = PeerDiversityConfig::default().with_max_per_ipv6_prefix(2);
        let mut tracker = PeerDiversityTracker::new(config);

        let ip1 = IpAddr::V6(Ipv6Addr::new(0x2001, 0x0db8, 0x85a3, 0, 0, 0, 0, 1));
        let ip2 = IpAddr::V6(Ipv6Addr::new(0x2001, 0x0db8, 0x85a3, 0xffff, 0, 0, 0, 2));
        let ip3 = IpAddr::V6(Ipv6Addr::new(0x2001, 0x0db8, 0x85a3, 0, 0, 0, 0, 3));

        assert!(tracker.check_and_add(ip1, None, None).is_accepted());
        assert!(tracker.check_and_add(ip2, None, None).is_accepted());
        assert!(tracker.check_and_add(ip3, None, None).is_rejected());

        assert_eq!(tracker.prefix_count(), 1);
    }

    // ==================== Stats Tests ====================

    #[test]
    fn tracker_stats_empty() {
        let tracker = PeerDiversityTracker::with_defaults();
        let stats = tracker.stats();

        assert_eq!(stats.total_peers, 0);
        assert_eq!(stats.distinct_subnets, 0);
        assert_eq!(stats.diversity_score(), 1.0);
    }

    #[test]
    fn tracker_stats_with_peers() {
        let mut tracker = PeerDiversityTracker::with_defaults();

        // Add peers from different subnets
        tracker.check_and_add(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), Some(Asn(100)), None);
        tracker.check_and_add(IpAddr::V4(Ipv4Addr::new(5, 6, 7, 8)), Some(Asn(200)), None);
        tracker.check_and_add(IpAddr::V4(Ipv4Addr::new(9, 10, 11, 12)), Some(Asn(300)), None);

        let stats = tracker.stats();
        assert_eq!(stats.total_peers, 3);
        assert_eq!(stats.distinct_subnets, 3);
        assert_eq!(stats.distinct_asns, 3);
        assert!((stats.diversity_score() - 1.0).abs() < 0.01);
    }

    #[test]
    fn tracker_stats_health_check() {
        let config = PeerDiversityConfig::default().with_max_per_ipv4_subnet(2);
        let mut tracker = PeerDiversityTracker::new(config.clone());

        tracker.check_and_add(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), None, None);
        tracker.check_and_add(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 5)), None, None);

        let stats = tracker.stats();
        assert!(stats.is_healthy(&config));
        assert_eq!(stats.max_peers_per_subnet, 2);
    }

    // ==================== Geo Diversity Tests ====================

    #[test]
    fn tracker_geo_diversity_improvement() {
        let config = PeerDiversityConfig::default()
            .with_geo_diversity(true)
            .with_max_per_ipv4_subnet(10);
        let mut tracker = PeerDiversityTracker::new(config);

        let region_us = GeoRegion("US".to_string());
        let region_eu = GeoRegion("EU".to_string());

        // New region is always an improvement
        assert!(tracker.would_improve_geo_diversity(&region_us));

        tracker.check_and_add(
            IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)),
            None,
            Some(region_us.clone()),
        );

        // Different region still improves diversity
        assert!(tracker.would_improve_geo_diversity(&region_eu));

        // Same region doesn't improve if we have some diversity
        tracker.check_and_add(
            IpAddr::V4(Ipv4Addr::new(5, 6, 7, 8)),
            None,
            Some(region_eu.clone()),
        );
        tracker.check_and_add(
            IpAddr::V4(Ipv4Addr::new(9, 10, 11, 12)),
            None,
            Some(GeoRegion("APAC".to_string())),
        );

        // Now we have 3 regions (min_geo_regions default)
        assert!(!tracker.would_improve_geo_diversity(&region_us));
    }

    #[test]
    fn tracker_clear() {
        let mut tracker = PeerDiversityTracker::with_defaults();

        tracker.check_and_add(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), None, None);
        tracker.check_and_add(IpAddr::V4(Ipv4Addr::new(5, 6, 7, 8)), None, None);

        assert_eq!(tracker.peer_count(), 2);

        tracker.clear();

        assert_eq!(tracker.peer_count(), 0);
        assert_eq!(tracker.subnet_count(), 0);
    }

    // ==================== Private IP Tests ====================

    #[test]
    fn is_private_ip_detection() {
        // Private IPv4
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));

        // Public IPv4
        assert!(!is_private_ip(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_private_ip(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));

        // IPv6 loopback
        assert!(is_private_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn tracker_check_without_add() {
        let config = PeerDiversityConfig::default().with_max_per_ipv4_subnet(1);
        let mut tracker = PeerDiversityTracker::new(config);

        let ip1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10));
        let ip2 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20));

        // Check without adding - should always succeed
        assert!(tracker.check(ip1, None).is_accepted());
        assert!(tracker.check(ip2, None).is_accepted());

        // Now actually add
        tracker.check_and_add(ip1, None, None);

        // Check again - ip2 should be rejected now
        assert!(tracker.check(ip2, None).is_rejected());
    }

    #[test]
    fn tracker_peers_in_subnet_query() {
        let mut tracker = PeerDiversityTracker::with_defaults();

        let addr = Ipv4Addr::new(192, 168, 1, 10);
        assert_eq!(tracker.peers_in_subnet(addr), 0);

        tracker.check_and_add(IpAddr::V4(addr), None, None);
        assert_eq!(tracker.peers_in_subnet(addr), 1);

        tracker.check_and_add(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)), None, None);
        assert_eq!(tracker.peers_in_subnet(addr), 2);
    }

    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn subnet_deterministic(a in 0u8..=255, b in 0u8..=255, c in 0u8..=255, d in 0u8..=255) {
                let addr = Ipv4Addr::new(a, b, c, d);
                let subnet1 = Ipv4Subnet::from_addr(addr);
                let subnet2 = Ipv4Subnet::from_addr(addr);
                prop_assert_eq!(subnet1, subnet2);
            }

            #[test]
            fn same_slash24_same_subnet(a in 0u8..=255, b in 0u8..=255, c in 0u8..=255, d1 in 0u8..=255, d2 in 0u8..=255) {
                let addr1 = Ipv4Addr::new(a, b, c, d1);
                let addr2 = Ipv4Addr::new(a, b, c, d2);
                let subnet1 = Ipv4Subnet::from_addr(addr1);
                let subnet2 = Ipv4Subnet::from_addr(addr2);
                prop_assert_eq!(subnet1, subnet2);
            }

            #[test]
            fn add_remove_invariant(
                ips in prop::collection::vec(
                    (0u8..=255, 0u8..=255, 0u8..=255, 0u8..=255),
                    1..10
                )
            ) {
                let mut tracker = PeerDiversityTracker::new(
                    PeerDiversityConfig::default().with_max_per_ipv4_subnet(100)
                );

                // Add all IPs
                for (a, b, c, d) in &ips {
                    let ip = IpAddr::V4(Ipv4Addr::new(*a, *b, *c, *d));
                    tracker.check_and_add(ip, None, None);
                }

                let count_after_add = tracker.peer_count();

                // Remove all IPs
                for (a, b, c, d) in &ips {
                    let ip = IpAddr::V4(Ipv4Addr::new(*a, *b, *c, *d));
                    tracker.remove(ip);
                }

                // Should be empty after removing all
                prop_assert_eq!(tracker.peer_count(), 0);
                prop_assert!(count_after_add > 0 || ips.is_empty());
            }

            #[test]
            fn diversity_score_bounded(
                num_peers in 1usize..20,
                num_subnets in 1usize..10
            ) {
                let mut tracker = PeerDiversityTracker::new(
                    PeerDiversityConfig::default().with_max_per_ipv4_subnet(100)
                );

                // Add peers distributed across subnets
                for i in 0..num_peers {
                    let subnet = (i % num_subnets) as u8;
                    let ip = IpAddr::V4(Ipv4Addr::new(10, 0, subnet, i as u8));
                    tracker.check_and_add(ip, None, None);
                }

                let stats = tracker.stats();
                prop_assert!(stats.diversity_score() >= 0.0);
                prop_assert!(stats.diversity_score() <= 1.0);
            }
        }
    }
}
