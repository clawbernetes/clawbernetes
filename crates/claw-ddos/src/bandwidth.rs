//! Bandwidth limiting using token bucket algorithm.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Instant;

use parking_lot::RwLock;
use tracing::debug;

use crate::config::BandwidthConfig;
use crate::error::{DdosError, DdosResult};

/// Token bucket state for a single connection/IP.
#[derive(Debug)]
struct TokenBucket {
    /// Current tokens available.
    tokens: u64,
    /// Last time tokens were refilled.
    last_refill: Instant,
    /// Tokens added per second.
    rate: u64,
    /// Maximum tokens (burst size).
    capacity: u64,
}

impl TokenBucket {
    /// Create a new token bucket.
    fn new(rate: u64, capacity: u64) -> Self {
        Self {
            tokens: capacity, // Start full
            last_refill: Instant::now(),
            rate,
            capacity,
        }
    }

    /// Refill tokens based on elapsed time.
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        let elapsed_secs = elapsed.as_secs_f64();
        
        // Calculate tokens to add based on rate and elapsed time
        let tokens_to_add = (elapsed_secs * self.rate as f64) as u64;
        
        if tokens_to_add > 0 {
            self.tokens = self.tokens.saturating_add(tokens_to_add).min(self.capacity);
            self.last_refill = now;
        }
    }

    /// Try to consume tokens. Returns true if successful.
    fn try_consume(&mut self, amount: u64) -> bool {
        self.refill();
        
        if self.tokens >= amount {
            self.tokens = self.tokens.saturating_sub(amount);
            true
        } else {
            false
        }
    }

    /// Get current available tokens.
    fn available(&mut self) -> u64 {
        self.refill();
        self.tokens
    }
}

/// Per-connection bandwidth limiter using token bucket algorithm.
#[derive(Debug)]
pub struct BandwidthLimiter {
    /// Bytes allowed per second per IP.
    bytes_per_second: u64,
    /// Burst size (token bucket capacity).
    burst_size: u64,
    /// Whether limiting is enabled.
    enabled: bool,
    /// Token buckets per IP.
    buckets: RwLock<HashMap<IpAddr, TokenBucket>>,
}

impl BandwidthLimiter {
    /// Create a new bandwidth limiter.
    #[must_use]
    pub fn new(bytes_per_second: u64, burst_size: u64) -> Self {
        Self {
            bytes_per_second,
            burst_size,
            enabled: true,
            buckets: RwLock::new(HashMap::new()),
        }
    }

    /// Create from configuration.
    #[must_use]
    pub fn from_config(config: &BandwidthConfig) -> Self {
        Self {
            bytes_per_second: config.bytes_per_second,
            burst_size: config.burst_size,
            enabled: config.enabled,
            buckets: RwLock::new(HashMap::new()),
        }
    }

    /// Check if bandwidth is available and consume it if so.
    ///
    /// # Errors
    ///
    /// Returns `DdosError::BandwidthExceeded` if bandwidth limit exceeded.
    pub fn consume(&self, ip: &IpAddr, bytes: u64) -> DdosResult<()> {
        if !self.enabled {
            return Ok(());
        }

        let mut buckets = self.buckets.write();
        
        let bucket = buckets
            .entry(*ip)
            .or_insert_with(|| TokenBucket::new(self.bytes_per_second, self.burst_size));
        
        if bucket.try_consume(bytes) {
            debug!(ip = %ip, bytes = bytes, remaining = bucket.tokens, "Bandwidth consumed");
            Ok(())
        } else {
            Err(DdosError::BandwidthExceeded { ip: *ip })
        }
    }

    /// Check available bandwidth without consuming.
    #[must_use]
    pub fn available(&self, ip: &IpAddr) -> u64 {
        if !self.enabled {
            return u64::MAX;
        }

        let mut buckets = self.buckets.write();
        
        buckets
            .entry(*ip)
            .or_insert_with(|| TokenBucket::new(self.bytes_per_second, self.burst_size))
            .available()
    }

    /// Check if a certain amount of bandwidth is available.
    #[must_use]
    pub fn can_consume(&self, ip: &IpAddr, bytes: u64) -> bool {
        self.available(ip) >= bytes
    }

    /// Remove tracking for an IP (e.g., on disconnect).
    pub fn remove(&self, ip: &IpAddr) {
        self.buckets.write().remove(ip);
    }

    /// Clear all tracking.
    pub fn clear(&self) {
        self.buckets.write().clear();
    }

    /// Get the number of tracked IPs.
    #[must_use]
    pub fn tracked_count(&self) -> usize {
        self.buckets.read().len()
    }

    /// Get the configured bytes per second.
    #[must_use]
    pub const fn bytes_per_second(&self) -> u64 {
        self.bytes_per_second
    }

    /// Get the configured burst size.
    #[must_use]
    pub const fn burst_size(&self) -> u64 {
        self.burst_size
    }

    /// Check if limiting is enabled.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enable or disable bandwidth limiting.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Clean up stale entries (IPs not seen for a while).
    /// Returns number of entries removed.
    pub fn cleanup_stale(&self, max_age_secs: u64) -> usize {
        let mut buckets = self.buckets.write();
        let now = Instant::now();
        
        let initial_count = buckets.len();
        
        buckets.retain(|_, bucket| {
            let age = now.duration_since(bucket.last_refill).as_secs();
            age < max_age_secs
        });
        
        initial_count.saturating_sub(buckets.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    // ==================== TokenBucket Tests ====================

    #[test]
    fn test_token_bucket_new() {
        let bucket = TokenBucket::new(100, 1000);
        assert_eq!(bucket.rate, 100);
        assert_eq!(bucket.capacity, 1000);
        assert_eq!(bucket.tokens, 1000); // Starts full
    }

    #[test]
    fn test_token_bucket_consume() {
        let mut bucket = TokenBucket::new(100, 1000);
        
        assert!(bucket.try_consume(500));
        assert_eq!(bucket.tokens, 500);
        
        assert!(bucket.try_consume(500));
        assert_eq!(bucket.tokens, 0);
        
        // Can't consume more
        assert!(!bucket.try_consume(1));
    }

    #[test]
    fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(1000, 1000);
        
        // Consume all
        bucket.tokens = 0;
        bucket.last_refill = Instant::now();
        
        // Wait a bit and check refill
        thread::sleep(Duration::from_millis(50));
        let available = bucket.available();
        
        // Should have refilled some tokens (at least 40 at 1000/s over 50ms)
        assert!(available >= 40, "Expected at least 40 tokens, got {available}");
    }

    #[test]
    fn test_token_bucket_capacity_limit() {
        let mut bucket = TokenBucket::new(10000, 100);
        bucket.tokens = 100; // Full
        
        // Wait to accumulate more tokens
        thread::sleep(Duration::from_millis(50));
        bucket.refill();
        
        // Shouldn't exceed capacity
        assert!(bucket.tokens <= 100);
    }

    // ==================== BandwidthLimiter Tests ====================

    #[test]
    fn test_bandwidth_limiter_new() {
        let limiter = BandwidthLimiter::new(1000, 5000);
        assert_eq!(limiter.bytes_per_second(), 1000);
        assert_eq!(limiter.burst_size(), 5000);
        assert!(limiter.is_enabled());
    }

    #[test]
    fn test_bandwidth_limiter_from_config() {
        let config = BandwidthConfig {
            bytes_per_second: 500,
            burst_size: 2000,
            enabled: false,
        };
        let limiter = BandwidthLimiter::from_config(&config);
        
        assert_eq!(limiter.bytes_per_second(), 500);
        assert_eq!(limiter.burst_size(), 2000);
        assert!(!limiter.is_enabled());
    }

    #[test]
    fn test_bandwidth_limiter_consume() {
        let limiter = BandwidthLimiter::new(1000, 5000);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        // Should allow consumption up to burst size
        assert!(limiter.consume(&ip, 3000).is_ok());
        assert!(limiter.consume(&ip, 2000).is_ok());
        
        // Should fail when exhausted
        assert!(limiter.consume(&ip, 1).is_err());
    }

    #[test]
    fn test_bandwidth_limiter_available() {
        let limiter = BandwidthLimiter::new(1000, 5000);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        // Initially should have burst size available
        assert_eq!(limiter.available(&ip), 5000);
        
        limiter.consume(&ip, 1000).unwrap();
        assert_eq!(limiter.available(&ip), 4000);
    }

    #[test]
    fn test_bandwidth_limiter_can_consume() {
        let limiter = BandwidthLimiter::new(1000, 5000);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        assert!(limiter.can_consume(&ip, 5000));
        assert!(!limiter.can_consume(&ip, 5001));
        
        limiter.consume(&ip, 5000).unwrap();
        assert!(!limiter.can_consume(&ip, 1));
    }

    #[test]
    fn test_bandwidth_limiter_disabled() {
        let config = BandwidthConfig {
            bytes_per_second: 100,
            burst_size: 100,
            enabled: false,
        };
        let limiter = BandwidthLimiter::from_config(&config);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        // When disabled, everything should pass
        assert!(limiter.consume(&ip, 1_000_000).is_ok());
        assert_eq!(limiter.available(&ip), u64::MAX);
    }

    #[test]
    fn test_bandwidth_limiter_multiple_ips() {
        let limiter = BandwidthLimiter::new(1000, 5000);
        let ip1: IpAddr = "1.2.3.4".parse().unwrap();
        let ip2: IpAddr = "5.6.7.8".parse().unwrap();
        
        // Each IP has its own bucket
        limiter.consume(&ip1, 4000).unwrap();
        
        // ip2 should still have full capacity
        assert_eq!(limiter.available(&ip2), 5000);
        assert_eq!(limiter.available(&ip1), 1000);
    }

    #[test]
    fn test_bandwidth_limiter_remove() {
        let limiter = BandwidthLimiter::new(1000, 5000);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        limiter.consume(&ip, 5000).unwrap();
        assert_eq!(limiter.tracked_count(), 1);
        
        limiter.remove(&ip);
        assert_eq!(limiter.tracked_count(), 0);
        
        // After removal, should have fresh capacity
        assert_eq!(limiter.available(&ip), 5000);
    }

    #[test]
    fn test_bandwidth_limiter_clear() {
        let limiter = BandwidthLimiter::new(1000, 5000);
        let ip1: IpAddr = "1.2.3.4".parse().unwrap();
        let ip2: IpAddr = "5.6.7.8".parse().unwrap();
        
        limiter.consume(&ip1, 1000).unwrap();
        limiter.consume(&ip2, 1000).unwrap();
        assert_eq!(limiter.tracked_count(), 2);
        
        limiter.clear();
        assert_eq!(limiter.tracked_count(), 0);
    }

    #[test]
    fn test_bandwidth_limiter_set_enabled() {
        let mut limiter = BandwidthLimiter::new(1000, 5000);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        // Exhaust bandwidth
        limiter.consume(&ip, 5000).unwrap();
        assert!(limiter.consume(&ip, 1).is_err());
        
        // Disable limiting
        limiter.set_enabled(false);
        assert!(limiter.consume(&ip, 1_000_000).is_ok());
    }

    #[test]
    fn test_bandwidth_limiter_refill_over_time() {
        let limiter = BandwidthLimiter::new(10000, 1000); // 10KB/s
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        // Exhaust bandwidth
        limiter.consume(&ip, 1000).unwrap();
        assert!(limiter.consume(&ip, 1).is_err());
        
        // Wait for refill
        thread::sleep(Duration::from_millis(50));
        
        // Should have some bandwidth now (at least 400 bytes at 10KB/s over 50ms)
        let available = limiter.available(&ip);
        assert!(available >= 400, "Expected at least 400, got {available}");
    }

    #[test]
    fn test_bandwidth_limiter_cleanup_stale() {
        let limiter = BandwidthLimiter::new(1000, 5000);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        limiter.consume(&ip, 100).unwrap();
        assert_eq!(limiter.tracked_count(), 1);
        
        // Cleanup with very long max age (nothing should be removed)
        let removed = limiter.cleanup_stale(3600);
        assert_eq!(removed, 0);
        assert_eq!(limiter.tracked_count(), 1);
    }

    #[test]
    fn test_bandwidth_limiter_error_type() {
        let limiter = BandwidthLimiter::new(100, 100);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        limiter.consume(&ip, 100).unwrap();
        let result = limiter.consume(&ip, 1);
        
        assert!(matches!(result, Err(DdosError::BandwidthExceeded { .. })));
        if let Err(DdosError::BandwidthExceeded { ip: err_ip }) = result {
            assert_eq!(err_ip, ip);
        }
    }
}
