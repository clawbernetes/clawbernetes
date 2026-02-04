//! Unified `DDoS` protection layer.

use std::net::IpAddr;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tracing::{debug, info, warn};

use crate::bandwidth::BandwidthLimiter;
use crate::blocklist::IpBlocklist;
use crate::config::DdosConfig;
use crate::connection::{ConnectionLimiter, SlowLorisProtection};
use crate::geo::GeoBlocking;
use crate::rate_limit::{ComputeCostLimiter, RequestRateLimiter};
use crate::reputation::{ReputationTracker, ViolationType};

/// Result of a protection check.
#[derive(Debug, Clone)]
pub enum ProtectionResult {
    /// Request/connection is allowed.
    Allow,
    /// Request is rate limited, retry after specified duration.
    RateLimit {
        /// Milliseconds until retry is allowed.
        retry_after_ms: u64,
    },
    /// IP is blocked.
    Block {
        /// Reason for the block.
        reason: String,
        /// When the block expires (None = permanent).
        expires_at: Option<DateTime<Utc>>,
    },
}

impl ProtectionResult {
    /// Check if the result allows the request.
    #[must_use]
    pub const fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }

    /// Check if the result is a rate limit.
    #[must_use]
    pub const fn is_rate_limited(&self) -> bool {
        matches!(self, Self::RateLimit { .. })
    }

    /// Check if the result is a block.
    #[must_use]
    pub const fn is_blocked(&self) -> bool {
        matches!(self, Self::Block { .. })
    }
}

/// Unified `DDoS` protection combining all protection layers.
#[derive(Debug)]
pub struct DdosProtection {
    /// Configuration.
    config: DdosConfig,
    /// Connection limiter.
    connection_limiter: ConnectionLimiter,
    /// Slow loris protection.
    slow_loris: SlowLorisProtection,
    /// Bandwidth limiter.
    bandwidth_limiter: BandwidthLimiter,
    /// Request rate limiter.
    rate_limiter: RequestRateLimiter,
    /// Compute cost limiter.
    compute_limiter: ComputeCostLimiter,
    /// IP blocklist.
    blocklist: IpBlocklist,
    /// Geographic blocking.
    geo_blocking: GeoBlocking,
    /// Reputation tracker.
    reputation: ReputationTracker,
}

impl DdosProtection {
    /// Create a new protection layer with the given configuration.
    #[must_use]
    pub fn new(config: DdosConfig) -> Self {
        let connection_limiter = ConnectionLimiter::from_config(&config.connection);
        let slow_loris = SlowLorisProtection::from_config(&config.connection);
        let bandwidth_limiter = BandwidthLimiter::from_config(&config.bandwidth);
        let rate_limiter = RequestRateLimiter::from_config(&config.rate_limit);
        let compute_limiter = ComputeCostLimiter::from_config(&config.compute_cost);
        let blocklist = IpBlocklist::from_config(&config.blocklist);
        let geo_blocking = GeoBlocking::from_config(&config.geo);
        let reputation = ReputationTracker::from_config(&config.reputation);

        Self {
            config,
            connection_limiter,
            slow_loris,
            bandwidth_limiter,
            rate_limiter,
            compute_limiter,
            blocklist,
            geo_blocking,
            reputation,
        }
    }

    /// Create with default configuration.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(DdosConfig::default())
    }

    // ==================== Main Check Methods ====================

    /// Check if a new connection from the given IP is allowed.
    ///
    /// This performs all connection-level checks:
    /// 1. Whitelist check
    /// 2. Blocklist check
    /// 3. Geo-blocking check
    /// 4. Reputation check
    /// 5. Connection limit check
    #[must_use]
    pub fn check_connection(&self, ip: &IpAddr) -> ProtectionResult {
        let ip_str = ip.to_string();

        // 1. Whitelist check (always allow)
        if self.config.is_whitelisted(&ip_str) {
            debug!(ip = %ip, "IP is whitelisted, allowing");
            return ProtectionResult::Allow;
        }

        // 2. Blocklist check
        if let Some(block_reason) = self.blocklist.get_block_reason(ip) {
            return ProtectionResult::Block {
                reason: block_reason.reason,
                expires_at: block_reason.expires_at,
            };
        }

        // 3. Geo-blocking check
        if let Err(e) = self.geo_blocking.check(ip) {
            return ProtectionResult::Block {
                reason: e.to_string(),
                expires_at: None,
            };
        }

        // 4. Reputation check
        if !self.reputation.has_good_reputation(ip) {
            // Bad reputation -> temporary block
            self.handle_bad_reputation(ip);
            return ProtectionResult::Block {
                reason: "Bad reputation".into(),
                expires_at: None,
            };
        }

        // 5. Connection limit check
        if let Err(_e) = self.connection_limiter.check(ip) {
            self.reputation.record_violation(ip, ViolationType::ConnectionLimit);
            self.maybe_escalate(ip);
            
            return ProtectionResult::RateLimit {
                retry_after_ms: 1000, // Try again in 1 second
            };
        }

        ProtectionResult::Allow
    }

    /// Check if a request from the given IP is allowed.
    ///
    /// This performs request-level checks:
    /// 1. Rate limit check
    /// 2. Records the request if allowed
    #[must_use]
    pub fn check_request(&self, ip: &IpAddr) -> ProtectionResult {
        // Check rate limit
        if self.rate_limiter.check_and_record(ip).is_err() {
            self.reputation.record_violation(ip, ViolationType::RateLimit);
            self.maybe_escalate(ip);
            
            let retry_after = self.rate_limiter.time_until_allowed(ip);
            return ProtectionResult::RateLimit {
                retry_after_ms: retry_after.as_millis() as u64,
            };
        }

        ProtectionResult::Allow
    }

    /// Check if bandwidth is available for the given IP.
    ///
    /// # Arguments
    ///
    /// * `ip` - The IP address
    /// * `bytes` - Number of bytes to consume
    #[must_use]
    pub fn check_bandwidth(&self, ip: &IpAddr, bytes: u64) -> ProtectionResult {
        if self.bandwidth_limiter.consume(ip, bytes).is_err() {
            self.reputation.record_violation(ip, ViolationType::BandwidthExceeded);
            
            return ProtectionResult::RateLimit {
                retry_after_ms: 100, // Try again shortly
            };
        }

        ProtectionResult::Allow
    }

    /// Check if compute cost is available for the given IP.
    ///
    /// # Arguments
    ///
    /// * `ip` - The IP address
    /// * `cost` - Compute cost units
    #[must_use]
    pub fn check_compute_cost(&self, ip: &IpAddr, cost: u64) -> ProtectionResult {
        if self.compute_limiter.consume(ip, cost).is_err() {
            self.reputation.record_violation(ip, ViolationType::ComputeCostExceeded);
            
            return ProtectionResult::RateLimit {
                retry_after_ms: 1000,
            };
        }

        ProtectionResult::Allow
    }

    /// Perform a combined check for connection and request.
    #[must_use]
    pub fn check(&self, ip: &IpAddr) -> ProtectionResult {
        // First check connection-level
        let conn_result = self.check_connection(ip);
        if !conn_result.is_allowed() {
            return conn_result;
        }

        // Then check request-level
        self.check_request(ip)
    }

    // ==================== Connection Lifecycle ====================

    /// Notify that a connection was opened from an IP.
    pub fn on_connection_opened(&self, ip: &IpAddr) {
        self.connection_limiter.add_connection(ip);
    }

    /// Notify that a connection was closed from an IP.
    pub fn on_connection_closed(&self, ip: &IpAddr) {
        self.connection_limiter.remove_connection(ip);
    }

    /// Start tracking a handshake (for slow loris protection).
    #[must_use]
    pub fn start_handshake(&self, ip: &IpAddr) -> u64 {
        self.slow_loris.start_handshake(ip)
    }

    /// Complete a handshake.
    pub fn complete_handshake(&self, connection_id: u64) {
        self.slow_loris.complete_handshake(connection_id);
    }

    /// Remove a connection from handshake tracking.
    pub fn remove_handshake(&self, connection_id: u64) {
        self.slow_loris.remove_connection(connection_id);
    }

    /// Clean up timed-out handshakes.
    ///
    /// Returns the IPs that were timed out.
    pub fn cleanup_slow_loris(&self) -> Vec<IpAddr> {
        let timed_out = self.slow_loris.cleanup_timed_out();
        
        for (_, ip) in &timed_out {
            self.reputation.record_violation(ip, ViolationType::MalformedRequest);
            self.maybe_escalate(ip);
        }
        
        timed_out.into_iter().map(|(_, ip)| ip).collect()
    }

    // ==================== Blocking ====================

    /// Block an IP address.
    pub fn block_ip(&self, ip: &IpAddr, reason: &str, duration: Option<Duration>) {
        self.blocklist.block(ip, reason, duration);
    }

    /// Block an IP with the default duration.
    pub fn block_ip_default(&self, ip: &IpAddr, reason: &str) {
        self.blocklist.block_default(ip, reason);
    }

    /// Block an IP permanently.
    pub fn block_ip_permanent(&self, ip: &IpAddr, reason: &str) {
        self.blocklist.block_permanent(ip, reason);
    }

    /// Unblock an IP address.
    pub fn unblock_ip(&self, ip: &IpAddr) -> bool {
        self.blocklist.unblock(ip)
    }

    /// Check if an IP is blocked.
    #[must_use]
    pub fn is_blocked(&self, ip: &IpAddr) -> bool {
        self.blocklist.is_blocked(ip)
    }

    // ==================== Reputation ====================

    /// Record a custom violation for an IP.
    pub fn record_violation(&self, ip: &IpAddr, violation: ViolationType) {
        self.reputation.record_violation(ip, violation);
        self.maybe_escalate(ip);
    }

    /// Get the reputation score for an IP.
    #[must_use]
    pub fn reputation_score(&self, ip: &IpAddr) -> i32 {
        self.reputation.score_value(ip)
    }

    /// Reset reputation for an IP.
    pub fn reset_reputation(&self, ip: &IpAddr) {
        self.reputation.reset(ip);
    }

    // ==================== Escalation ====================

    /// Check and apply automatic escalation for an IP.
    fn maybe_escalate(&self, ip: &IpAddr) {
        if !self.config.escalation.enabled {
            return;
        }

        // Check if reputation is below threshold
        if !self.reputation.has_good_reputation(ip) {
            self.handle_bad_reputation(ip);
        }
    }

    /// Handle an IP with bad reputation.
    fn handle_bad_reputation(&self, ip: &IpAddr) {
        let temp_ban_count = self.reputation.temp_ban_count(ip);
        
        if temp_ban_count >= self.config.escalation.permanent_ban_threshold {
            // Permanent ban
            info!(ip = %ip, temp_ban_count = temp_ban_count, "Applying permanent ban due to escalation");
            self.blocklist.block_permanent(ip, "Escalated: repeated violations");
        } else {
            // Temporary ban
            let duration = self.config.escalation.temp_ban_duration;
            info!(ip = %ip, temp_ban_count = temp_ban_count, duration_secs = duration.as_secs(), "Applying temporary ban");
            self.blocklist.block(ip, "Temporary ban: reputation threshold", Some(duration));
            self.reputation.record_temp_ban(ip);
        }
    }

    // ==================== Statistics ====================

    /// Get the number of currently blocked IPs.
    #[must_use]
    pub fn blocked_count(&self) -> usize {
        self.blocklist.blocked_count()
    }

    /// Get the number of tracked connections.
    #[must_use]
    pub fn connection_count(&self) -> u64 {
        self.connection_limiter.total_connections()
    }

    /// Get the number of unique IPs with connections.
    #[must_use]
    pub fn unique_connection_ips(&self) -> usize {
        self.connection_limiter.unique_ips()
    }

    /// Get the number of pending handshakes.
    #[must_use]
    pub fn pending_handshakes(&self) -> usize {
        self.slow_loris.pending_count()
    }

    /// Get the number of IPs being tracked for rate limiting.
    #[must_use]
    pub fn rate_limited_ips(&self) -> usize {
        self.rate_limiter.tracked_count()
    }

    /// Get the number of IPs with bad reputation.
    #[must_use]
    pub fn bad_reputation_count(&self) -> usize {
        self.reputation.bad_reputation_ips().len()
    }

    // ==================== Maintenance ====================

    /// Clean up expired blocks and stale tracking data.
    pub fn cleanup(&self) {
        // Cleanup blocklist
        self.blocklist.cleanup();
        
        // Cleanup slow loris (and record violations)
        self.cleanup_slow_loris();
        
        // Cleanup bandwidth limiter
        self.bandwidth_limiter.cleanup_stale(3600);
        
        // Cleanup reputation (remove very old entries)
        self.reputation.cleanup_stale(Duration::from_secs(86400));
    }

    /// Clear all protection state (use with caution!).
    pub fn clear_all(&self) {
        warn!("Clearing all DDoS protection state");
        self.connection_limiter.clear();
        self.bandwidth_limiter.clear();
        self.rate_limiter.clear();
        self.compute_limiter.clear();
        self.blocklist.clear();
        self.geo_blocking.clear_cache();
        self.reputation.clear();
    }

    // ==================== Configuration Access ====================

    /// Get the configuration.
    #[must_use]
    pub const fn config(&self) -> &DdosConfig {
        &self.config
    }

    /// Get a reference to the connection limiter.
    #[must_use]
    pub const fn connection_limiter(&self) -> &ConnectionLimiter {
        &self.connection_limiter
    }

    /// Get a reference to the bandwidth limiter.
    #[must_use]
    pub const fn bandwidth_limiter(&self) -> &BandwidthLimiter {
        &self.bandwidth_limiter
    }

    /// Get a reference to the rate limiter.
    #[must_use]
    pub const fn rate_limiter(&self) -> &RequestRateLimiter {
        &self.rate_limiter
    }

    /// Get a reference to the compute cost limiter.
    #[must_use]
    pub const fn compute_limiter(&self) -> &ComputeCostLimiter {
        &self.compute_limiter
    }

    /// Get a reference to the blocklist.
    #[must_use]
    pub const fn blocklist(&self) -> &IpBlocklist {
        &self.blocklist
    }

    /// Get a reference to the geo-blocking.
    #[must_use]
    pub const fn geo_blocking(&self) -> &GeoBlocking {
        &self.geo_blocking
    }

    /// Get a reference to the reputation tracker.
    #[must_use]
    pub const fn reputation(&self) -> &ReputationTracker {
        &self.reputation
    }
}

impl Default for DdosProtection {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConnectionConfig, EscalationConfig};

    // ==================== ProtectionResult Tests ====================

    #[test]
    fn test_protection_result_allow() {
        let result = ProtectionResult::Allow;
        assert!(result.is_allowed());
        assert!(!result.is_rate_limited());
        assert!(!result.is_blocked());
    }

    #[test]
    fn test_protection_result_rate_limit() {
        let result = ProtectionResult::RateLimit { retry_after_ms: 1000 };
        assert!(!result.is_allowed());
        assert!(result.is_rate_limited());
        assert!(!result.is_blocked());
    }

    #[test]
    fn test_protection_result_block() {
        let result = ProtectionResult::Block {
            reason: "test".into(),
            expires_at: None,
        };
        assert!(!result.is_allowed());
        assert!(!result.is_rate_limited());
        assert!(result.is_blocked());
    }

    // ==================== DdosProtection Tests ====================

    #[test]
    fn test_protection_new() {
        let config = DdosConfig::default();
        let protection = DdosProtection::new(config);
        
        assert_eq!(protection.blocked_count(), 0);
        assert_eq!(protection.connection_count(), 0);
    }

    #[test]
    fn test_protection_with_defaults() {
        let protection = DdosProtection::with_defaults();
        assert_eq!(protection.blocked_count(), 0);
    }

    #[test]
    fn test_protection_default() {
        let protection = DdosProtection::default();
        assert_eq!(protection.blocked_count(), 0);
    }

    #[test]
    fn test_protection_check_connection_allowed() {
        let protection = DdosProtection::with_defaults();
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        
        let result = protection.check_connection(&ip);
        assert!(result.is_allowed());
    }

    #[test]
    fn test_protection_check_connection_blocked() {
        let protection = DdosProtection::with_defaults();
        let ip: IpAddr = "10.0.0.2".parse().unwrap();
        
        protection.block_ip(&ip, "test block", None);
        
        let result = protection.check_connection(&ip);
        assert!(result.is_blocked());
    }

    #[test]
    fn test_protection_whitelist() {
        let config = DdosConfig::builder()
            .whitelist_ip("10.0.0.3")
            .build();
        let protection = DdosProtection::new(config);
        let ip: IpAddr = "10.0.0.3".parse().unwrap();
        
        // Block the IP
        protection.block_ip(&ip, "test", None);
        
        // Should still be allowed due to whitelist
        let result = protection.check_connection(&ip);
        assert!(result.is_allowed());
    }

    #[test]
    fn test_protection_connection_limit() {
        let config = DdosConfig::builder()
            .connection(ConnectionConfig {
                max_per_ip: 2,
                ..ConnectionConfig::default()
            })
            .build();
        let protection = DdosProtection::new(config);
        let ip: IpAddr = "10.0.0.4".parse().unwrap();
        
        // First two connections allowed
        assert!(protection.check_connection(&ip).is_allowed());
        protection.on_connection_opened(&ip);
        
        assert!(protection.check_connection(&ip).is_allowed());
        protection.on_connection_opened(&ip);
        
        // Third should be rate limited
        let result = protection.check_connection(&ip);
        assert!(result.is_rate_limited());
    }

    #[test]
    fn test_protection_connection_lifecycle() {
        let config = DdosConfig::builder()
            .connection(ConnectionConfig {
                max_per_ip: 1,
                ..ConnectionConfig::default()
            })
            .build();
        let protection = DdosProtection::new(config);
        let ip: IpAddr = "10.0.0.5".parse().unwrap();
        
        // Open connection
        protection.on_connection_opened(&ip);
        assert_eq!(protection.connection_count(), 1);
        
        // Can't open another
        assert!(protection.check_connection(&ip).is_rate_limited());
        
        // Close connection
        protection.on_connection_closed(&ip);
        assert_eq!(protection.connection_count(), 0);
        
        // Can open again
        assert!(protection.check_connection(&ip).is_allowed());
    }

    #[test]
    fn test_protection_check_request() {
        let config = DdosConfig::default();
        let protection = DdosProtection::new(config);
        let ip: IpAddr = "10.0.0.6".parse().unwrap();
        
        // Should allow requests within limit
        let result = protection.check_request(&ip);
        assert!(result.is_allowed());
    }

    #[test]
    fn test_protection_check_bandwidth() {
        let protection = DdosProtection::with_defaults();
        let ip: IpAddr = "10.0.0.7".parse().unwrap();
        
        // Should allow bandwidth within limit
        let result = protection.check_bandwidth(&ip, 1000);
        assert!(result.is_allowed());
    }

    #[test]
    fn test_protection_check_compute_cost() {
        let protection = DdosProtection::with_defaults();
        let ip: IpAddr = "10.0.0.8".parse().unwrap();
        
        let result = protection.check_compute_cost(&ip, 100);
        assert!(result.is_allowed());
    }

    #[test]
    fn test_protection_combined_check() {
        let protection = DdosProtection::with_defaults();
        let ip: IpAddr = "10.0.0.9".parse().unwrap();
        
        let result = protection.check(&ip);
        assert!(result.is_allowed());
    }

    #[test]
    fn test_protection_block_unblock() {
        let protection = DdosProtection::with_defaults();
        let ip: IpAddr = "10.0.0.10".parse().unwrap();
        
        assert!(!protection.is_blocked(&ip));
        
        protection.block_ip_default(&ip, "test");
        assert!(protection.is_blocked(&ip));
        
        protection.unblock_ip(&ip);
        assert!(!protection.is_blocked(&ip));
    }

    #[test]
    fn test_protection_permanent_block() {
        let protection = DdosProtection::with_defaults();
        let ip: IpAddr = "10.0.0.11".parse().unwrap();
        
        protection.block_ip_permanent(&ip, "permanent test");
        
        let result = protection.check_connection(&ip);
        assert!(result.is_blocked());
        
        if let ProtectionResult::Block { expires_at, .. } = result {
            assert!(expires_at.is_none()); // Permanent blocks have no expiry
        }
    }

    #[test]
    fn test_protection_reputation() {
        let protection = DdosProtection::with_defaults();
        let ip: IpAddr = "10.0.0.12".parse().unwrap();
        
        let initial_score = protection.reputation_score(&ip);
        
        protection.record_violation(&ip, ViolationType::RateLimit);
        
        let new_score = protection.reputation_score(&ip);
        assert!(new_score < initial_score);
    }

    #[test]
    fn test_protection_reset_reputation() {
        let protection = DdosProtection::with_defaults();
        let ip: IpAddr = "10.0.0.13".parse().unwrap();
        
        protection.record_violation(&ip, ViolationType::AuthFailure);
        let after_violation = protection.reputation_score(&ip);
        
        protection.reset_reputation(&ip);
        let after_reset = protection.reputation_score(&ip);
        
        assert!(after_reset > after_violation);
    }

    #[test]
    fn test_protection_handshake_tracking() {
        let protection = DdosProtection::with_defaults();
        let ip: IpAddr = "10.0.0.14".parse().unwrap();
        
        let conn_id = protection.start_handshake(&ip);
        assert_eq!(protection.pending_handshakes(), 1);
        
        protection.complete_handshake(conn_id);
        assert_eq!(protection.pending_handshakes(), 0);
    }

    #[test]
    fn test_protection_statistics() {
        let protection = DdosProtection::with_defaults();
        let ip: IpAddr = "10.0.0.15".parse().unwrap();
        
        assert_eq!(protection.blocked_count(), 0);
        assert_eq!(protection.connection_count(), 0);
        assert_eq!(protection.unique_connection_ips(), 0);
        assert_eq!(protection.pending_handshakes(), 0);
        
        protection.on_connection_opened(&ip);
        assert_eq!(protection.connection_count(), 1);
        assert_eq!(protection.unique_connection_ips(), 1);
        
        protection.block_ip(&ip, "test", None);
        assert_eq!(protection.blocked_count(), 1);
    }

    #[test]
    fn test_protection_cleanup() {
        let protection = DdosProtection::with_defaults();
        
        // Just ensure cleanup doesn't panic
        protection.cleanup();
    }

    #[test]
    fn test_protection_clear_all() {
        let protection = DdosProtection::with_defaults();
        let ip: IpAddr = "10.0.0.16".parse().unwrap();
        
        protection.on_connection_opened(&ip);
        protection.block_ip(&ip, "test", None);
        
        protection.clear_all();
        
        assert_eq!(protection.connection_count(), 0);
        assert_eq!(protection.blocked_count(), 0);
    }

    #[test]
    fn test_protection_accessors() {
        let config = DdosConfig::default();
        let protection = DdosProtection::new(config);
        
        // Just ensure accessors work
        let _ = protection.config();
        let _ = protection.connection_limiter();
        let _ = protection.bandwidth_limiter();
        let _ = protection.rate_limiter();
        let _ = protection.compute_limiter();
        let _ = protection.blocklist();
        let _ = protection.geo_blocking();
        let _ = protection.reputation();
    }

    #[test]
    fn test_protection_escalation_disabled() {
        let config = DdosConfig::builder()
            .escalation(EscalationConfig {
                enabled: false,
                ..EscalationConfig::default()
            })
            .build();
        let protection = DdosProtection::new(config);
        let ip: IpAddr = "10.0.0.17".parse().unwrap();
        
        // Record many violations
        for _ in 0..100 {
            protection.record_violation(&ip, ViolationType::AuthFailure);
        }
        
        // Should not be blocked because escalation is disabled
        // (reputation affects next connection check, but escalation auto-ban is off)
        assert_eq!(protection.blocked_count(), 0);
    }
}
