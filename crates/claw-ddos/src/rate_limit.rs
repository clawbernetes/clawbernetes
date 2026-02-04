//! Request-level rate limiting.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use tracing::debug;

use crate::config::{ComputeCostConfig, RateLimitConfig};
use crate::error::{DdosError, DdosResult};

/// Sliding window request timestamps for rate limiting.
#[derive(Debug)]
struct SlidingWindow {
    /// Request timestamps within the window.
    timestamps: VecDeque<Instant>,
    /// Window duration.
    window_size: Duration,
    /// Maximum requests allowed in window.
    max_requests: u32,
}

impl SlidingWindow {
    /// Create a new sliding window.
    fn new(max_requests: u32, window_size: Duration) -> Self {
        Self {
            timestamps: VecDeque::with_capacity(max_requests as usize + 1),
            window_size,
            max_requests,
        }
    }

    /// Clean up expired timestamps and check if a new request is allowed.
    #[allow(clippy::unchecked_time_subtraction)] // window_size is always small enough
    fn try_request(&mut self) -> bool {
        let now = Instant::now();
        let cutoff = now - self.window_size;
        
        // Remove expired timestamps
        while self.timestamps.front().is_some_and(|t| *t < cutoff) {
            self.timestamps.pop_front();
        }
        
        // Check if under limit
        if (self.timestamps.len() as u32) < self.max_requests {
            self.timestamps.push_back(now);
            true
        } else {
            false
        }
    }

    /// Get current request count in the window.
    #[allow(clippy::unchecked_time_subtraction)] // window_size is always small enough
    fn current_count(&mut self) -> u32 {
        let now = Instant::now();
        let cutoff = now - self.window_size;
        
        // Remove expired timestamps
        while self.timestamps.front().is_some_and(|t| *t < cutoff) {
            self.timestamps.pop_front();
        }
        
        self.timestamps.len() as u32
    }

    /// Get time until next request would be allowed (if rate limited).
    fn time_until_available(&mut self) -> Option<Duration> {
        if self.try_request() {
            // We were able to make a request, undo it
            self.timestamps.pop_back();
            None
        } else if let Some(oldest) = self.timestamps.front() {
            let now = Instant::now();
            let expires = *oldest + self.window_size;
            if expires > now {
                Some(expires - now)
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Request rate limiter using sliding window algorithm.
#[derive(Debug)]
pub struct RequestRateLimiter {
    /// Maximum requests per window.
    max_requests: u32,
    /// Window size.
    window_size: Duration,
    /// Whether limiting is enabled.
    enabled: bool,
    /// Sliding windows per IP.
    windows: RwLock<HashMap<IpAddr, SlidingWindow>>,
}

impl RequestRateLimiter {
    /// Create a new rate limiter.
    #[must_use]
    pub fn new(requests_per_second: u32) -> Self {
        Self {
            max_requests: requests_per_second,
            window_size: Duration::from_secs(1),
            enabled: true,
            windows: RwLock::new(HashMap::new()),
        }
    }

    /// Create with custom window size.
    #[must_use]
    pub fn with_window(max_requests: u32, window_size: Duration) -> Self {
        Self {
            max_requests,
            window_size,
            enabled: true,
            windows: RwLock::new(HashMap::new()),
        }
    }

    /// Create from configuration.
    #[must_use]
    pub fn from_config(config: &RateLimitConfig) -> Self {
        Self {
            max_requests: config.requests_per_second,
            window_size: config.window_size,
            enabled: config.enabled,
            windows: RwLock::new(HashMap::new()),
        }
    }

    /// Check if a request is allowed and record it if so.
    ///
    /// # Errors
    ///
    /// Returns `DdosError::RateLimitExceeded` if rate limit exceeded.
    pub fn check_and_record(&self, ip: &IpAddr) -> DdosResult<()> {
        if !self.enabled {
            return Ok(());
        }

        let mut windows = self.windows.write();
        
        let window = windows
            .entry(*ip)
            .or_insert_with(|| SlidingWindow::new(self.max_requests, self.window_size));
        
        if window.try_request() {
            debug!(ip = %ip, count = window.timestamps.len(), "Request allowed");
            Ok(())
        } else {
            Err(DdosError::RateLimitExceeded {
                ip: *ip,
                limit_type: format!("{}/s", self.max_requests),
            })
        }
    }

    /// Check if a request would be allowed (without recording).
    #[must_use]
    pub fn would_allow(&self, ip: &IpAddr) -> bool {
        if !self.enabled {
            return true;
        }

        let mut windows = self.windows.write();
        
        let window = windows
            .entry(*ip)
            .or_insert_with(|| SlidingWindow::new(self.max_requests, self.window_size));
        
        window.current_count() < self.max_requests
    }

    /// Get time until next request would be allowed (0 if allowed now).
    #[must_use]
    pub fn time_until_allowed(&self, ip: &IpAddr) -> Duration {
        if !self.enabled {
            return Duration::ZERO;
        }

        let mut windows = self.windows.write();
        
        let window = windows
            .entry(*ip)
            .or_insert_with(|| SlidingWindow::new(self.max_requests, self.window_size));
        
        window.time_until_available().unwrap_or(Duration::ZERO)
    }

    /// Get current request count for an IP.
    #[must_use]
    pub fn current_count(&self, ip: &IpAddr) -> u32 {
        let mut windows = self.windows.write();
        
        windows
            .get_mut(ip)
            .map_or(0, SlidingWindow::current_count)
    }

    /// Remove tracking for an IP.
    pub fn remove(&self, ip: &IpAddr) {
        self.windows.write().remove(ip);
    }

    /// Clear all tracking.
    pub fn clear(&self) {
        self.windows.write().clear();
    }

    /// Get number of tracked IPs.
    #[must_use]
    pub fn tracked_count(&self) -> usize {
        self.windows.read().len()
    }

    /// Get the max requests per window.
    #[must_use]
    pub const fn max_requests(&self) -> u32 {
        self.max_requests
    }

    /// Get the window size.
    #[must_use]
    pub const fn window_size(&self) -> Duration {
        self.window_size
    }

    /// Check if rate limiting is enabled.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Compute cost tracking for an IP.
#[derive(Debug)]
struct CostTracker {
    /// Total cost accumulated in current period.
    total_cost: u64,
    /// When the current period started.
    period_start: Instant,
    /// Period duration.
    period_duration: Duration,
    /// Cost budget per period.
    budget: u64,
}

impl CostTracker {
    /// Create a new cost tracker.
    fn new(budget: u64, period_duration: Duration) -> Self {
        Self {
            total_cost: 0,
            period_start: Instant::now(),
            budget,
            period_duration,
        }
    }

    /// Reset if period has elapsed.
    fn maybe_reset(&mut self) {
        if self.period_start.elapsed() >= self.period_duration {
            self.total_cost = 0;
            self.period_start = Instant::now();
        }
    }

    /// Try to add cost. Returns true if within budget.
    fn try_add(&mut self, cost: u64) -> bool {
        self.maybe_reset();
        
        let new_total = self.total_cost.saturating_add(cost);
        if new_total <= self.budget {
            self.total_cost = new_total;
            true
        } else {
            false
        }
    }

    /// Get remaining budget.
    fn remaining(&mut self) -> u64 {
        self.maybe_reset();
        self.budget.saturating_sub(self.total_cost)
    }

    /// Get current usage.
    fn used(&mut self) -> u64 {
        self.maybe_reset();
        self.total_cost
    }
}

/// Limits expensive operations per client based on compute cost.
#[derive(Debug)]
pub struct ComputeCostLimiter {
    /// Cost budget per reset interval.
    cost_per_period: u64,
    /// Reset interval.
    reset_interval: Duration,
    /// Whether limiting is enabled.
    enabled: bool,
    /// Cost trackers per IP.
    trackers: RwLock<HashMap<IpAddr, CostTracker>>,
}

impl ComputeCostLimiter {
    /// Create a new compute cost limiter.
    #[must_use]
    pub fn new(cost_per_minute: u64) -> Self {
        Self {
            cost_per_period: cost_per_minute,
            reset_interval: Duration::from_secs(60),
            enabled: true,
            trackers: RwLock::new(HashMap::new()),
        }
    }

    /// Create with custom reset interval.
    #[must_use]
    pub fn with_interval(cost_per_period: u64, reset_interval: Duration) -> Self {
        Self {
            cost_per_period,
            reset_interval,
            enabled: true,
            trackers: RwLock::new(HashMap::new()),
        }
    }

    /// Create from configuration.
    #[must_use]
    pub fn from_config(config: &ComputeCostConfig) -> Self {
        Self {
            cost_per_period: config.cost_per_minute,
            reset_interval: config.reset_interval,
            enabled: config.enabled,
            trackers: RwLock::new(HashMap::new()),
        }
    }

    /// Try to consume compute cost.
    ///
    /// # Errors
    ///
    /// Returns `DdosError::ComputeCostExceeded` if budget exceeded.
    pub fn consume(&self, ip: &IpAddr, cost: u64) -> DdosResult<()> {
        if !self.enabled {
            return Ok(());
        }

        let mut trackers = self.trackers.write();
        
        let tracker = trackers
            .entry(*ip)
            .or_insert_with(|| CostTracker::new(self.cost_per_period, self.reset_interval));
        
        if tracker.try_add(cost) {
            debug!(ip = %ip, cost = cost, remaining = tracker.remaining(), "Compute cost consumed");
            Ok(())
        } else {
            Err(DdosError::ComputeCostExceeded {
                ip: *ip,
                used: tracker.used(),
                budget: self.cost_per_period,
            })
        }
    }

    /// Check if cost can be consumed without actually consuming.
    #[must_use]
    pub fn can_consume(&self, ip: &IpAddr, cost: u64) -> bool {
        if !self.enabled {
            return true;
        }

        let mut trackers = self.trackers.write();
        
        let tracker = trackers
            .entry(*ip)
            .or_insert_with(|| CostTracker::new(self.cost_per_period, self.reset_interval));
        
        tracker.remaining() >= cost
    }

    /// Get remaining budget for an IP.
    #[must_use]
    pub fn remaining(&self, ip: &IpAddr) -> u64 {
        if !self.enabled {
            return u64::MAX;
        }

        let mut trackers = self.trackers.write();
        
        trackers
            .get_mut(ip)
            .map_or(self.cost_per_period, CostTracker::remaining)
    }

    /// Get current usage for an IP.
    #[must_use]
    pub fn used(&self, ip: &IpAddr) -> u64 {
        let mut trackers = self.trackers.write();
        
        trackers
            .get_mut(ip)
            .map_or(0, CostTracker::used)
    }

    /// Remove tracking for an IP.
    pub fn remove(&self, ip: &IpAddr) {
        self.trackers.write().remove(ip);
    }

    /// Clear all tracking.
    pub fn clear(&self) {
        self.trackers.write().clear();
    }

    /// Get number of tracked IPs.
    #[must_use]
    pub fn tracked_count(&self) -> usize {
        self.trackers.read().len()
    }

    /// Get the cost budget per period.
    #[must_use]
    pub const fn cost_per_period(&self) -> u64 {
        self.cost_per_period
    }

    /// Get the reset interval.
    #[must_use]
    pub const fn reset_interval(&self) -> Duration {
        self.reset_interval
    }

    /// Check if limiting is enabled.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    // ==================== SlidingWindow Tests ====================

    #[test]
    fn test_sliding_window_allows_under_limit() {
        let mut window = SlidingWindow::new(5, Duration::from_secs(1));
        
        for _ in 0..5 {
            assert!(window.try_request());
        }
        assert_eq!(window.current_count(), 5);
    }

    #[test]
    fn test_sliding_window_blocks_over_limit() {
        let mut window = SlidingWindow::new(3, Duration::from_secs(1));
        
        assert!(window.try_request());
        assert!(window.try_request());
        assert!(window.try_request());
        assert!(!window.try_request()); // Should be blocked
    }

    #[test]
    fn test_sliding_window_expires_old_requests() {
        let mut window = SlidingWindow::new(2, Duration::from_millis(50));
        
        assert!(window.try_request());
        assert!(window.try_request());
        assert!(!window.try_request());
        
        // Wait for expiration
        thread::sleep(Duration::from_millis(60));
        
        // Should be allowed again
        assert!(window.try_request());
    }

    #[test]
    fn test_sliding_window_time_until_available() {
        let mut window = SlidingWindow::new(1, Duration::from_millis(100));
        
        // First request allowed
        assert!(window.time_until_available().is_none());
        window.try_request();
        
        // Now we're blocked, should have wait time
        let wait = window.time_until_available();
        assert!(wait.is_some());
        assert!(wait.unwrap() <= Duration::from_millis(100));
    }

    // ==================== RequestRateLimiter Tests ====================

    #[test]
    fn test_rate_limiter_new() {
        let limiter = RequestRateLimiter::new(100);
        assert_eq!(limiter.max_requests(), 100);
        assert_eq!(limiter.window_size(), Duration::from_secs(1));
        assert!(limiter.is_enabled());
    }

    #[test]
    fn test_rate_limiter_from_config() {
        let config = RateLimitConfig {
            requests_per_second: 50,
            window_size: Duration::from_secs(2),
            enabled: true,
        };
        let limiter = RequestRateLimiter::from_config(&config);
        
        assert_eq!(limiter.max_requests(), 50);
        assert_eq!(limiter.window_size(), Duration::from_secs(2));
    }

    #[test]
    fn test_rate_limiter_allows_under_limit() {
        let limiter = RequestRateLimiter::new(5);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        for _ in 0..5 {
            assert!(limiter.check_and_record(&ip).is_ok());
        }
    }

    #[test]
    fn test_rate_limiter_blocks_over_limit() {
        let limiter = RequestRateLimiter::new(3);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        assert!(limiter.check_and_record(&ip).is_ok());
        assert!(limiter.check_and_record(&ip).is_ok());
        assert!(limiter.check_and_record(&ip).is_ok());
        
        let result = limiter.check_and_record(&ip);
        assert!(matches!(result, Err(DdosError::RateLimitExceeded { .. })));
    }

    #[test]
    fn test_rate_limiter_would_allow() {
        let limiter = RequestRateLimiter::new(2);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        assert!(limiter.would_allow(&ip));
        limiter.check_and_record(&ip).unwrap();
        
        assert!(limiter.would_allow(&ip));
        limiter.check_and_record(&ip).unwrap();
        
        assert!(!limiter.would_allow(&ip));
    }

    #[test]
    fn test_rate_limiter_time_until_allowed() {
        let limiter = RequestRateLimiter::with_window(1, Duration::from_millis(100));
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        // Not rate limited initially
        assert_eq!(limiter.time_until_allowed(&ip), Duration::ZERO);
        
        limiter.check_and_record(&ip).unwrap();
        
        // Should have wait time now
        let wait = limiter.time_until_allowed(&ip);
        assert!(wait > Duration::ZERO);
        assert!(wait <= Duration::from_millis(100));
    }

    #[test]
    fn test_rate_limiter_current_count() {
        let limiter = RequestRateLimiter::new(10);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        assert_eq!(limiter.current_count(&ip), 0);
        
        limiter.check_and_record(&ip).unwrap();
        limiter.check_and_record(&ip).unwrap();
        
        assert_eq!(limiter.current_count(&ip), 2);
    }

    #[test]
    fn test_rate_limiter_multiple_ips() {
        let limiter = RequestRateLimiter::new(2);
        let ip1: IpAddr = "1.2.3.4".parse().unwrap();
        let ip2: IpAddr = "5.6.7.8".parse().unwrap();
        
        // Exhaust ip1's limit
        limiter.check_and_record(&ip1).unwrap();
        limiter.check_and_record(&ip1).unwrap();
        assert!(limiter.check_and_record(&ip1).is_err());
        
        // ip2 should still have full capacity
        assert!(limiter.check_and_record(&ip2).is_ok());
        assert!(limiter.check_and_record(&ip2).is_ok());
    }

    #[test]
    fn test_rate_limiter_remove_and_clear() {
        let limiter = RequestRateLimiter::new(10);
        let ip1: IpAddr = "1.2.3.4".parse().unwrap();
        let ip2: IpAddr = "5.6.7.8".parse().unwrap();
        
        limiter.check_and_record(&ip1).unwrap();
        limiter.check_and_record(&ip2).unwrap();
        assert_eq!(limiter.tracked_count(), 2);
        
        limiter.remove(&ip1);
        assert_eq!(limiter.tracked_count(), 1);
        
        limiter.clear();
        assert_eq!(limiter.tracked_count(), 0);
    }

    #[test]
    fn test_rate_limiter_disabled() {
        let config = RateLimitConfig {
            requests_per_second: 1,
            window_size: Duration::from_secs(1),
            enabled: false,
        };
        let limiter = RequestRateLimiter::from_config(&config);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        // Should allow unlimited when disabled
        for _ in 0..100 {
            assert!(limiter.check_and_record(&ip).is_ok());
        }
    }

    // ==================== ComputeCostLimiter Tests ====================

    #[test]
    fn test_compute_cost_limiter_new() {
        let limiter = ComputeCostLimiter::new(1000);
        assert_eq!(limiter.cost_per_period(), 1000);
        assert_eq!(limiter.reset_interval(), Duration::from_secs(60));
        assert!(limiter.is_enabled());
    }

    #[test]
    fn test_compute_cost_limiter_from_config() {
        let config = ComputeCostConfig {
            cost_per_minute: 500,
            reset_interval: Duration::from_secs(30),
            enabled: true,
        };
        let limiter = ComputeCostLimiter::from_config(&config);
        
        assert_eq!(limiter.cost_per_period(), 500);
        assert_eq!(limiter.reset_interval(), Duration::from_secs(30));
    }

    #[test]
    fn test_compute_cost_limiter_consume() {
        let limiter = ComputeCostLimiter::new(1000);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        assert!(limiter.consume(&ip, 500).is_ok());
        assert_eq!(limiter.used(&ip), 500);
        assert_eq!(limiter.remaining(&ip), 500);
        
        assert!(limiter.consume(&ip, 500).is_ok());
        assert_eq!(limiter.used(&ip), 1000);
        assert_eq!(limiter.remaining(&ip), 0);
        
        // Should fail now
        let result = limiter.consume(&ip, 1);
        assert!(matches!(result, Err(DdosError::ComputeCostExceeded { .. })));
    }

    #[test]
    fn test_compute_cost_limiter_can_consume() {
        let limiter = ComputeCostLimiter::new(100);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        assert!(limiter.can_consume(&ip, 100));
        assert!(!limiter.can_consume(&ip, 101));
        
        limiter.consume(&ip, 50).unwrap();
        
        assert!(limiter.can_consume(&ip, 50));
        assert!(!limiter.can_consume(&ip, 51));
    }

    #[test]
    fn test_compute_cost_limiter_reset() {
        let limiter = ComputeCostLimiter::with_interval(100, Duration::from_millis(50));
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        limiter.consume(&ip, 100).unwrap();
        assert!(limiter.consume(&ip, 1).is_err());
        
        // Wait for reset
        thread::sleep(Duration::from_millis(60));
        
        // Should have full budget again
        assert!(limiter.consume(&ip, 100).is_ok());
    }

    #[test]
    fn test_compute_cost_limiter_multiple_ips() {
        let limiter = ComputeCostLimiter::new(100);
        let ip1: IpAddr = "1.2.3.4".parse().unwrap();
        let ip2: IpAddr = "5.6.7.8".parse().unwrap();
        
        limiter.consume(&ip1, 100).unwrap();
        
        // ip2 should have full budget
        assert_eq!(limiter.remaining(&ip2), 100);
        assert!(limiter.consume(&ip2, 100).is_ok());
    }

    #[test]
    fn test_compute_cost_limiter_disabled() {
        let config = ComputeCostConfig {
            cost_per_minute: 10,
            reset_interval: Duration::from_secs(60),
            enabled: false,
        };
        let limiter = ComputeCostLimiter::from_config(&config);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        // Should allow unlimited when disabled
        assert!(limiter.consume(&ip, 1_000_000).is_ok());
        assert_eq!(limiter.remaining(&ip), u64::MAX);
    }

    #[test]
    fn test_compute_cost_limiter_remove_and_clear() {
        let limiter = ComputeCostLimiter::new(1000);
        let ip1: IpAddr = "1.2.3.4".parse().unwrap();
        let ip2: IpAddr = "5.6.7.8".parse().unwrap();
        
        limiter.consume(&ip1, 100).unwrap();
        limiter.consume(&ip2, 100).unwrap();
        assert_eq!(limiter.tracked_count(), 2);
        
        limiter.remove(&ip1);
        assert_eq!(limiter.tracked_count(), 1);
        
        limiter.clear();
        assert_eq!(limiter.tracked_count(), 0);
    }

    #[test]
    fn test_compute_cost_error_details() {
        let limiter = ComputeCostLimiter::new(100);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        limiter.consume(&ip, 100).unwrap();
        let result = limiter.consume(&ip, 50);
        
        if let Err(DdosError::ComputeCostExceeded { used, budget, .. }) = result {
            assert_eq!(used, 100);
            assert_eq!(budget, 100);
        } else {
            panic!("Expected ComputeCostExceeded error");
        }
    }
}
