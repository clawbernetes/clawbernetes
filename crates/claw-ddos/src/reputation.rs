//! Reputation tracking for bad behavior patterns.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use tracing::{debug, info, warn};

use crate::config::ReputationConfig;

/// Types of violations that affect reputation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViolationType {
    /// Rate limit was exceeded.
    RateLimit,
    /// Connection limit was exceeded.
    ConnectionLimit,
    /// Malformed request was received.
    MalformedRequest,
    /// Authentication failure.
    AuthFailure,
    /// Bandwidth limit exceeded.
    BandwidthExceeded,
    /// Compute cost exceeded.
    ComputeCostExceeded,
    /// Custom violation with specified penalty.
    Custom(i32),
}

impl ViolationType {
    /// Get the default penalty for this violation type.
    #[must_use]
    pub const fn default_penalty(&self) -> i32 {
        match self {
            Self::RateLimit => 5,
            Self::ConnectionLimit => 10,
            Self::MalformedRequest => 15,
            Self::AuthFailure => 20,
            Self::BandwidthExceeded => 5,
            Self::ComputeCostExceeded => 10,
            Self::Custom(penalty) => *penalty,
        }
    }
}

/// Reputation score for an IP address.
#[derive(Debug, Clone)]
pub struct ReputationScore {
    /// Current score (higher is better).
    pub score: i32,
    /// Total violations recorded.
    pub total_violations: u32,
    /// When this IP was first seen.
    pub first_seen: Instant,
    /// When the score was last updated.
    pub last_updated: Instant,
    /// Violation counts by type.
    pub violation_counts: HashMap<String, u32>,
}

impl ReputationScore {
    /// Create a new reputation score with the given initial value.
    #[must_use]
    pub fn new(initial_score: i32) -> Self {
        let now = Instant::now();
        Self {
            score: initial_score,
            total_violations: 0,
            first_seen: now,
            last_updated: now,
            violation_counts: HashMap::new(),
        }
    }

    /// Apply a penalty to the score.
    pub fn apply_penalty(&mut self, penalty: i32, violation_type: &str) {
        self.score = self.score.saturating_sub(penalty);
        self.total_violations = self.total_violations.saturating_add(1);
        self.last_updated = Instant::now();
        
        *self.violation_counts.entry(violation_type.to_string()).or_insert(0) += 1;
    }

    /// Apply decay (increase score over time).
    pub fn apply_decay(&mut self, decay_rate: i32, max_score: i32) {
        let elapsed = self.last_updated.elapsed();
        let hours = elapsed.as_secs() / 3600;
        
        if hours > 0 {
            let decay_amount = decay_rate.saturating_mul(hours as i32);
            self.score = self.score.saturating_add(decay_amount).min(max_score);
            self.last_updated = Instant::now();
        }
    }

    /// Get time since first seen.
    #[must_use]
    pub fn age(&self) -> Duration {
        self.first_seen.elapsed()
    }

    /// Get time since last update.
    #[must_use]
    pub fn time_since_update(&self) -> Duration {
        self.last_updated.elapsed()
    }
}

/// Tracks reputation scores for IP addresses.
#[derive(Debug)]
pub struct ReputationTracker {
    /// Initial score for new IPs.
    initial_score: i32,
    /// Minimum score before blocking.
    min_score: i32,
    /// Decay rate (points recovered per hour).
    decay_rate: i32,
    /// Penalty amounts by violation type.
    penalties: HashMap<ViolationType, i32>,
    /// Whether tracking is enabled.
    enabled: bool,
    /// Reputation scores by IP.
    scores: RwLock<HashMap<IpAddr, ReputationScore>>,
    /// Temporary ban counts by IP.
    temp_ban_counts: RwLock<HashMap<IpAddr, u32>>,
}

impl ReputationTracker {
    /// Create a new reputation tracker with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::with_initial_score(100)
    }

    /// Create with a specific initial score.
    #[must_use]
    pub fn with_initial_score(initial_score: i32) -> Self {
        let mut penalties = HashMap::new();
        penalties.insert(ViolationType::RateLimit, 5);
        penalties.insert(ViolationType::ConnectionLimit, 10);
        penalties.insert(ViolationType::MalformedRequest, 15);
        penalties.insert(ViolationType::AuthFailure, 20);
        penalties.insert(ViolationType::BandwidthExceeded, 5);
        penalties.insert(ViolationType::ComputeCostExceeded, 10);

        Self {
            initial_score,
            min_score: 0,
            decay_rate: 10,
            penalties,
            enabled: true,
            scores: RwLock::new(HashMap::new()),
            temp_ban_counts: RwLock::new(HashMap::new()),
        }
    }

    /// Create from configuration.
    #[must_use]
    pub fn from_config(config: &ReputationConfig) -> Self {
        let mut penalties = HashMap::new();
        penalties.insert(ViolationType::RateLimit, config.rate_limit_penalty);
        penalties.insert(ViolationType::ConnectionLimit, config.connection_penalty);
        penalties.insert(ViolationType::MalformedRequest, config.malformed_penalty);
        penalties.insert(ViolationType::AuthFailure, config.auth_failure_penalty);
        penalties.insert(ViolationType::BandwidthExceeded, config.rate_limit_penalty);
        penalties.insert(ViolationType::ComputeCostExceeded, config.connection_penalty);

        Self {
            initial_score: config.initial_score,
            min_score: config.min_score,
            decay_rate: config.decay_rate,
            penalties,
            enabled: config.enabled,
            scores: RwLock::new(HashMap::new()),
            temp_ban_counts: RwLock::new(HashMap::new()),
        }
    }

    /// Record a violation for an IP address.
    pub fn record_violation(&self, ip: &IpAddr, violation: ViolationType) {
        if !self.enabled {
            return;
        }

        let penalty = self.penalties.get(&violation)
            .copied()
            .unwrap_or_else(|| violation.default_penalty());
        
        let violation_name = format!("{violation:?}");
        
        let mut scores = self.scores.write();
        let score = scores
            .entry(*ip)
            .or_insert_with(|| ReputationScore::new(self.initial_score));
        
        score.apply_penalty(penalty, &violation_name);
        
        debug!(
            ip = %ip,
            violation = %violation_name,
            penalty = penalty,
            new_score = score.score,
            "Recorded violation"
        );
        
        if score.score <= self.min_score {
            warn!(ip = %ip, score = score.score, "IP reputation below minimum threshold");
        }
    }

    /// Check if an IP has good reputation (above minimum threshold).
    #[must_use]
    pub fn has_good_reputation(&self, ip: &IpAddr) -> bool {
        if !self.enabled {
            return true;
        }

        let mut scores = self.scores.write();
        
        if let Some(score) = scores.get_mut(ip) {
            // Apply decay before checking
            score.apply_decay(self.decay_rate, self.initial_score);
            score.score > self.min_score
        } else {
            // Unknown IPs have good reputation
            true
        }
    }

    /// Get the reputation score for an IP.
    #[must_use]
    pub fn get_score(&self, ip: &IpAddr) -> Option<ReputationScore> {
        let mut scores = self.scores.write();
        
        if let Some(score) = scores.get_mut(ip) {
            score.apply_decay(self.decay_rate, self.initial_score);
            Some(score.clone())
        } else {
            None
        }
    }

    /// Get the current numeric score for an IP (returns initial score if unknown).
    #[must_use]
    pub fn score_value(&self, ip: &IpAddr) -> i32 {
        self.get_score(ip)
            .map_or(self.initial_score, |s| s.score)
    }

    /// Reset the reputation for an IP.
    pub fn reset(&self, ip: &IpAddr) {
        self.scores.write().remove(ip);
        self.temp_ban_counts.write().remove(ip);
        info!(ip = %ip, "Reputation reset");
    }

    /// Clear all reputation data.
    pub fn clear(&self) {
        self.scores.write().clear();
        self.temp_ban_counts.write().clear();
        info!("All reputation data cleared");
    }

    /// Record a temporary ban for escalation tracking.
    pub fn record_temp_ban(&self, ip: &IpAddr) {
        let mut counts = self.temp_ban_counts.write();
        let count = counts.entry(*ip).or_insert(0);
        *count = count.saturating_add(1);
        
        debug!(ip = %ip, temp_ban_count = *count, "Temporary ban recorded");
    }

    /// Get the number of temporary bans for an IP.
    #[must_use]
    pub fn temp_ban_count(&self, ip: &IpAddr) -> u32 {
        self.temp_ban_counts.read().get(ip).copied().unwrap_or(0)
    }

    /// Check if an IP should be permanently banned based on temp ban count.
    #[must_use]
    pub fn should_permanent_ban(&self, ip: &IpAddr, threshold: u32) -> bool {
        self.temp_ban_count(ip) >= threshold
    }

    /// Get the number of tracked IPs.
    #[must_use]
    pub fn tracked_count(&self) -> usize {
        self.scores.read().len()
    }

    /// Get IPs with reputation below threshold.
    #[must_use]
    pub fn bad_reputation_ips(&self) -> Vec<IpAddr> {
        self.scores
            .read()
            .iter()
            .filter(|(_, score)| score.score <= self.min_score)
            .map(|(ip, _)| *ip)
            .collect()
    }

    /// Get the initial score.
    #[must_use]
    pub const fn initial_score(&self) -> i32 {
        self.initial_score
    }

    /// Get the minimum score threshold.
    #[must_use]
    pub const fn min_score(&self) -> i32 {
        self.min_score
    }

    /// Check if tracking is enabled.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Clean up old entries (IPs not updated in the specified duration).
    pub fn cleanup_stale(&self, max_age: Duration) -> usize {
        let mut scores = self.scores.write();
        let initial_count = scores.len();
        
        scores.retain(|_, score| score.time_since_update() < max_age);
        
        initial_count.saturating_sub(scores.len())
    }
}

impl Default for ReputationTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    // ==================== ViolationType Tests ====================

    #[test]
    fn test_violation_type_penalties() {
        assert_eq!(ViolationType::RateLimit.default_penalty(), 5);
        assert_eq!(ViolationType::ConnectionLimit.default_penalty(), 10);
        assert_eq!(ViolationType::MalformedRequest.default_penalty(), 15);
        assert_eq!(ViolationType::AuthFailure.default_penalty(), 20);
        assert_eq!(ViolationType::Custom(42).default_penalty(), 42);
    }

    // ==================== ReputationScore Tests ====================

    #[test]
    fn test_reputation_score_new() {
        let score = ReputationScore::new(100);
        
        assert_eq!(score.score, 100);
        assert_eq!(score.total_violations, 0);
        assert!(score.violation_counts.is_empty());
    }

    #[test]
    fn test_reputation_score_apply_penalty() {
        let mut score = ReputationScore::new(100);
        
        score.apply_penalty(10, "test");
        
        assert_eq!(score.score, 90);
        assert_eq!(score.total_violations, 1);
        assert_eq!(*score.violation_counts.get("test").unwrap_or(&0), 1);
    }

    #[test]
    fn test_reputation_score_multiple_penalties() {
        let mut score = ReputationScore::new(100);
        
        score.apply_penalty(10, "type_a");
        score.apply_penalty(10, "type_a");
        score.apply_penalty(20, "type_b");
        
        assert_eq!(score.score, 60);
        assert_eq!(score.total_violations, 3);
        assert_eq!(*score.violation_counts.get("type_a").unwrap_or(&0), 2);
        assert_eq!(*score.violation_counts.get("type_b").unwrap_or(&0), 1);
    }

    #[test]
    fn test_reputation_score_saturating_sub() {
        let mut score = ReputationScore::new(10);
        
        score.apply_penalty(100, "test");
        
        // Should not go negative
        assert!(score.score <= 10);
        // Score should be 10 - 100 = -90, but saturating means it stops at i32::MIN
        // Actually saturating_sub for i32 goes negative, let me check...
        // saturating_sub doesn't stop at 0 for signed integers
        assert_eq!(score.score, -90);
    }

    #[test]
    fn test_reputation_score_age() {
        let score = ReputationScore::new(100);
        thread::sleep(Duration::from_millis(10));
        
        let age = score.age();
        assert!(age >= Duration::from_millis(10));
    }

    // ==================== ReputationTracker Tests ====================

    #[test]
    fn test_tracker_new() {
        let tracker = ReputationTracker::new();
        
        assert_eq!(tracker.initial_score(), 100);
        assert_eq!(tracker.tracked_count(), 0);
        assert!(tracker.is_enabled());
    }

    #[test]
    fn test_tracker_with_initial_score() {
        let tracker = ReputationTracker::with_initial_score(50);
        
        assert_eq!(tracker.initial_score(), 50);
    }

    #[test]
    fn test_tracker_from_config() {
        let config = ReputationConfig {
            initial_score: 200,
            min_score: 50,
            decay_rate: 20,
            rate_limit_penalty: 10,
            connection_penalty: 20,
            malformed_penalty: 30,
            auth_failure_penalty: 40,
            enabled: true,
        };
        let tracker = ReputationTracker::from_config(&config);
        
        assert_eq!(tracker.initial_score(), 200);
        assert_eq!(tracker.min_score(), 50);
    }

    #[test]
    fn test_tracker_record_violation() {
        let tracker = ReputationTracker::new();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        tracker.record_violation(&ip, ViolationType::RateLimit);
        
        let score = tracker.get_score(&ip).unwrap();
        assert!(score.score < 100);
        assert_eq!(score.total_violations, 1);
    }

    #[test]
    fn test_tracker_has_good_reputation() {
        let tracker = ReputationTracker::with_initial_score(100);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        // Unknown IP has good reputation
        assert!(tracker.has_good_reputation(&ip));
        
        // After many violations, reputation is bad
        for _ in 0..25 {
            tracker.record_violation(&ip, ViolationType::AuthFailure); // 20 penalty each
        }
        
        assert!(!tracker.has_good_reputation(&ip));
    }

    #[test]
    fn test_tracker_score_value() {
        let tracker = ReputationTracker::with_initial_score(100);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        // Unknown IP returns initial score
        assert_eq!(tracker.score_value(&ip), 100);
        
        tracker.record_violation(&ip, ViolationType::RateLimit);
        assert!(tracker.score_value(&ip) < 100);
    }

    #[test]
    fn test_tracker_reset() {
        let tracker = ReputationTracker::new();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        tracker.record_violation(&ip, ViolationType::RateLimit);
        assert!(tracker.get_score(&ip).is_some());
        
        tracker.reset(&ip);
        assert!(tracker.get_score(&ip).is_none());
    }

    #[test]
    fn test_tracker_clear() {
        let tracker = ReputationTracker::new();
        let ip1: IpAddr = "1.2.3.4".parse().unwrap();
        let ip2: IpAddr = "5.6.7.8".parse().unwrap();
        
        tracker.record_violation(&ip1, ViolationType::RateLimit);
        tracker.record_violation(&ip2, ViolationType::RateLimit);
        assert_eq!(tracker.tracked_count(), 2);
        
        tracker.clear();
        assert_eq!(tracker.tracked_count(), 0);
    }

    #[test]
    fn test_tracker_temp_ban_tracking() {
        let tracker = ReputationTracker::new();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        assert_eq!(tracker.temp_ban_count(&ip), 0);
        
        tracker.record_temp_ban(&ip);
        assert_eq!(tracker.temp_ban_count(&ip), 1);
        
        tracker.record_temp_ban(&ip);
        tracker.record_temp_ban(&ip);
        assert_eq!(tracker.temp_ban_count(&ip), 3);
    }

    #[test]
    fn test_tracker_should_permanent_ban() {
        let tracker = ReputationTracker::new();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        assert!(!tracker.should_permanent_ban(&ip, 3));
        
        tracker.record_temp_ban(&ip);
        tracker.record_temp_ban(&ip);
        assert!(!tracker.should_permanent_ban(&ip, 3));
        
        tracker.record_temp_ban(&ip);
        assert!(tracker.should_permanent_ban(&ip, 3));
    }

    #[test]
    fn test_tracker_bad_reputation_ips() {
        let tracker = ReputationTracker::with_initial_score(50);
        let ip1: IpAddr = "1.2.3.4".parse().unwrap();
        let ip2: IpAddr = "5.6.7.8".parse().unwrap();
        
        // Drive ip1 reputation to zero
        for _ in 0..10 {
            tracker.record_violation(&ip1, ViolationType::RateLimit);
        }
        
        // ip2 has only one violation
        tracker.record_violation(&ip2, ViolationType::RateLimit);
        
        let bad_ips = tracker.bad_reputation_ips();
        assert!(bad_ips.contains(&ip1));
        assert!(!bad_ips.contains(&ip2));
    }

    #[test]
    fn test_tracker_disabled() {
        let config = ReputationConfig {
            enabled: false,
            ..ReputationConfig::default()
        };
        let tracker = ReputationTracker::from_config(&config);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        // When disabled, violations don't affect score
        tracker.record_violation(&ip, ViolationType::AuthFailure);
        assert!(tracker.get_score(&ip).is_none());
        
        // Always has good reputation when disabled
        assert!(tracker.has_good_reputation(&ip));
    }

    #[test]
    fn test_tracker_default() {
        let tracker = ReputationTracker::default();
        assert_eq!(tracker.initial_score(), 100);
    }

    #[test]
    fn test_tracker_multiple_violation_types() {
        let tracker = ReputationTracker::new();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        tracker.record_violation(&ip, ViolationType::RateLimit);
        tracker.record_violation(&ip, ViolationType::ConnectionLimit);
        tracker.record_violation(&ip, ViolationType::AuthFailure);
        
        let score = tracker.get_score(&ip).unwrap();
        assert_eq!(score.total_violations, 3);
        
        // Check penalty amounts
        // RateLimit: 5, ConnectionLimit: 10, AuthFailure: 20 = 35 total
        assert_eq!(score.score, 100 - 5 - 10 - 20);
    }

    #[test]
    fn test_tracker_cleanup_stale() {
        let tracker = ReputationTracker::new();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        tracker.record_violation(&ip, ViolationType::RateLimit);
        
        // Cleanup with long max age (nothing removed)
        let removed = tracker.cleanup_stale(Duration::from_secs(3600));
        assert_eq!(removed, 0);
        
        // Cleanup with very short max age
        thread::sleep(Duration::from_millis(10));
        let removed = tracker.cleanup_stale(Duration::from_millis(1));
        assert_eq!(removed, 1);
    }

    #[test]
    fn test_tracker_custom_violation() {
        let tracker = ReputationTracker::new();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        tracker.record_violation(&ip, ViolationType::Custom(50));
        
        let score = tracker.get_score(&ip).unwrap();
        assert_eq!(score.score, 50); // 100 - 50
    }
}
