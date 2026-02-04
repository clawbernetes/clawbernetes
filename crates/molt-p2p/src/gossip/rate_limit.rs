//! Per-peer rate limiting for gossip messages.
//!
//! Provides sliding window rate limiting to prevent DoS attacks from peers
//! flooding the network with unique messages.

use crate::protocol::PeerId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Configuration for rate limiting behavior.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum messages allowed per peer within the time window.
    pub max_messages_per_window: u32,
    /// Time window for counting messages (sliding window).
    pub window_duration: Duration,
    /// Number of rate limit violations before a peer is temporarily banned.
    pub violations_before_ban: u32,
    /// How long a peer is banned after exceeding violation threshold.
    pub ban_duration: Duration,
    /// Whether rate limiting is enabled.
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_messages_per_window: 100,
            window_duration: Duration::from_secs(1),
            violations_before_ban: 5,
            ban_duration: Duration::from_secs(60),
            enabled: true,
        }
    }
}

impl RateLimitConfig {
    /// Creates a permissive config for testing or trusted networks.
    #[must_use]
    pub fn permissive() -> Self {
        Self {
            max_messages_per_window: 1000,
            window_duration: Duration::from_secs(1),
            violations_before_ban: 100,
            ban_duration: Duration::from_secs(10),
            enabled: true,
        }
    }

    /// Creates a strict config for hostile networks.
    #[must_use]
    pub fn strict() -> Self {
        Self {
            max_messages_per_window: 50,
            window_duration: Duration::from_secs(1),
            violations_before_ban: 3,
            ban_duration: Duration::from_secs(300),
            enabled: true,
        }
    }

    /// Disables rate limiting entirely.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            max_messages_per_window: 0,
            window_duration: Duration::from_secs(0),
            violations_before_ban: 0,
            ban_duration: Duration::from_secs(0),
            enabled: false,
        }
    }

    /// Builder: set max messages per window.
    #[must_use]
    pub const fn with_max_messages(mut self, max: u32) -> Self {
        self.max_messages_per_window = max;
        self
    }

    /// Builder: set window duration.
    #[must_use]
    pub const fn with_window_duration(mut self, duration: Duration) -> Self {
        self.window_duration = duration;
        self
    }

    /// Builder: set violations before ban.
    #[must_use]
    pub const fn with_violations_before_ban(mut self, violations: u32) -> Self {
        self.violations_before_ban = violations;
        self
    }

    /// Builder: set ban duration.
    #[must_use]
    pub const fn with_ban_duration(mut self, duration: Duration) -> Self {
        self.ban_duration = duration;
        self
    }
}

/// Result of a rate limit check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RateLimitResult {
    /// Message is allowed.
    Allowed,
    /// Message is rate-limited (dropped).
    RateLimited,
    /// Peer is temporarily banned.
    Banned {
        /// When the ban will expire.
        until: Instant,
    },
}

impl RateLimitResult {
    /// Returns true if the message should be processed.
    #[must_use]
    pub const fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }
}

/// Tracks message timestamps for sliding window rate limiting.
#[derive(Debug, Clone)]
struct PeerRateState {
    /// Timestamps of recent messages within the window.
    message_times: Vec<Instant>,
    /// Number of rate limit violations.
    violation_count: u32,
    /// If banned, when the ban expires.
    banned_until: Option<Instant>,
}

impl PeerRateState {
    fn new() -> Self {
        Self {
            message_times: Vec::new(),
            violation_count: 0,
            banned_until: None,
        }
    }
}

/// Per-peer rate limiter using sliding window algorithm.
#[derive(Debug)]
pub struct RateLimiter {
    config: RateLimitConfig,
    peer_states: HashMap<PeerId, PeerRateState>,
    last_cleanup: Instant,
}

impl RateLimiter {
    /// Creates a new rate limiter with the given configuration.
    #[must_use]
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            peer_states: HashMap::new(),
            last_cleanup: Instant::now(),
        }
    }

    /// Creates a rate limiter with default configuration.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(RateLimitConfig::default())
    }

    /// Returns the current configuration.
    #[must_use]
    pub const fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Checks if a message from a peer should be allowed.
    ///
    /// This also records the message for rate tracking.
    pub fn check_and_record(&mut self, peer_id: PeerId) -> RateLimitResult {
        // If rate limiting is disabled, always allow
        if !self.config.enabled {
            return RateLimitResult::Allowed;
        }

        let now = Instant::now();

        // Periodic cleanup
        self.maybe_cleanup(now);

        let state = self.peer_states.entry(peer_id).or_insert_with(PeerRateState::new);

        // Check if peer is banned
        if let Some(banned_until) = state.banned_until {
            if now < banned_until {
                return RateLimitResult::Banned { until: banned_until };
            }
            // Ban expired, reset state
            state.banned_until = None;
            state.violation_count = 0;
            state.message_times.clear();
        }

        // Remove messages outside the window
        let window_start = now.checked_sub(self.config.window_duration).unwrap_or(now);
        state.message_times.retain(|&t| t >= window_start);

        // Check if over limit
        if state.message_times.len() >= self.config.max_messages_per_window as usize {
            state.violation_count = state.violation_count.saturating_add(1);

            // Check if should ban
            if state.violation_count >= self.config.violations_before_ban {
                let banned_until = now + self.config.ban_duration;
                state.banned_until = Some(banned_until);
                return RateLimitResult::Banned { until: banned_until };
            }

            return RateLimitResult::RateLimited;
        }

        // Record this message
        state.message_times.push(now);
        RateLimitResult::Allowed
    }

    /// Checks if a peer is currently banned without recording a message.
    #[must_use]
    pub fn is_banned(&self, peer_id: &PeerId) -> bool {
        self.peer_states
            .get(peer_id)
            .and_then(|s| s.banned_until)
            .map_or(false, |until| Instant::now() < until)
    }

    /// Manually bans a peer for the configured duration.
    pub fn ban_peer(&mut self, peer_id: PeerId) {
        let state = self.peer_states.entry(peer_id).or_insert_with(PeerRateState::new);
        state.banned_until = Some(Instant::now() + self.config.ban_duration);
    }

    /// Manually unbans a peer.
    pub fn unban_peer(&mut self, peer_id: &PeerId) {
        if let Some(state) = self.peer_states.get_mut(peer_id) {
            state.banned_until = None;
            state.violation_count = 0;
            // Clear message history to give the peer a fresh start
            state.message_times.clear();
        }
    }

    /// Returns the number of tracked peers.
    #[must_use]
    pub fn tracked_peer_count(&self) -> usize {
        self.peer_states.len()
    }

    /// Returns the current message count for a peer in the window.
    #[must_use]
    pub fn message_count(&self, peer_id: &PeerId) -> usize {
        let now = Instant::now();
        let window_start = now.checked_sub(self.config.window_duration).unwrap_or(now);

        self.peer_states
            .get(peer_id)
            .map(|s| s.message_times.iter().filter(|&&t| t >= window_start).count())
            .unwrap_or(0)
    }

    /// Returns the violation count for a peer.
    #[must_use]
    pub fn violation_count(&self, peer_id: &PeerId) -> u32 {
        self.peer_states
            .get(peer_id)
            .map(|s| s.violation_count)
            .unwrap_or(0)
    }

    /// Removes tracking state for a peer.
    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        self.peer_states.remove(peer_id);
    }

    /// Runs cleanup if enough time has passed.
    fn maybe_cleanup(&mut self, now: Instant) {
        // Run cleanup every 10 seconds
        if now.duration_since(self.last_cleanup) < Duration::from_secs(10) {
            return;
        }
        self.cleanup(now);
    }

    /// Cleans up expired state.
    fn cleanup(&mut self, now: Instant) {
        self.last_cleanup = now;
        let window = self.config.window_duration;

        // Remove peers with no recent activity and no active bans
        self.peer_states.retain(|_, state| {
            // Keep if banned
            if state.banned_until.map_or(false, |until| now < until) {
                return true;
            }

            // Keep if has recent messages
            let window_start = now.checked_sub(window).unwrap_or(now);
            state.message_times.retain(|&t| t >= window_start);
            !state.message_times.is_empty() || state.violation_count > 0
        });
    }

    /// Returns statistics about the rate limiter state.
    #[must_use]
    pub fn stats(&self) -> RateLimiterStats {
        let now = Instant::now();
        let banned_count = self.peer_states.values()
            .filter(|s| s.banned_until.map_or(false, |until| now < until))
            .count();

        RateLimiterStats {
            tracked_peers: self.peer_states.len(),
            banned_peers: banned_count,
            total_violations: self.peer_states.values().map(|s| s.violation_count as usize).sum(),
        }
    }
}

/// Statistics about rate limiter state.
#[derive(Debug, Clone, Default)]
pub struct RateLimiterStats {
    /// Number of peers being tracked.
    pub tracked_peers: usize,
    /// Number of currently banned peers.
    pub banned_peers: usize,
    /// Total violations across all peers.
    pub total_violations: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn make_peer_id() -> PeerId {
        let signing_key = SigningKey::generate(&mut OsRng);
        PeerId::from_public_key(&signing_key.verifying_key())
    }

    // ========== RateLimitConfig Tests ==========

    #[test]
    fn config_default_values() {
        let config = RateLimitConfig::default();
        assert_eq!(config.max_messages_per_window, 100);
        assert_eq!(config.window_duration, Duration::from_secs(1));
        assert_eq!(config.violations_before_ban, 5);
        assert_eq!(config.ban_duration, Duration::from_secs(60));
        assert!(config.enabled);
    }

    #[test]
    fn config_permissive() {
        let config = RateLimitConfig::permissive();
        assert_eq!(config.max_messages_per_window, 1000);
        assert!(config.enabled);
    }

    #[test]
    fn config_strict() {
        let config = RateLimitConfig::strict();
        assert_eq!(config.max_messages_per_window, 50);
        assert_eq!(config.violations_before_ban, 3);
    }

    #[test]
    fn config_disabled() {
        let config = RateLimitConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn config_builder_methods() {
        let config = RateLimitConfig::default()
            .with_max_messages(200)
            .with_window_duration(Duration::from_secs(5))
            .with_violations_before_ban(10)
            .with_ban_duration(Duration::from_secs(120));

        assert_eq!(config.max_messages_per_window, 200);
        assert_eq!(config.window_duration, Duration::from_secs(5));
        assert_eq!(config.violations_before_ban, 10);
        assert_eq!(config.ban_duration, Duration::from_secs(120));
    }

    // ========== RateLimitResult Tests ==========

    #[test]
    fn rate_limit_result_is_allowed() {
        assert!(RateLimitResult::Allowed.is_allowed());
        assert!(!RateLimitResult::RateLimited.is_allowed());
        assert!(!RateLimitResult::Banned { until: Instant::now() }.is_allowed());
    }

    // ========== RateLimiter Creation Tests ==========

    #[test]
    fn rate_limiter_creation() {
        let limiter = RateLimiter::with_defaults();
        assert_eq!(limiter.tracked_peer_count(), 0);
        assert!(limiter.config().enabled);
    }

    #[test]
    fn rate_limiter_custom_config() {
        let config = RateLimitConfig::strict();
        let limiter = RateLimiter::new(config.clone());
        assert_eq!(limiter.config().max_messages_per_window, config.max_messages_per_window);
    }

    // ========== Normal Traffic Tests ==========

    #[test]
    fn normal_traffic_passes_through() {
        let config = RateLimitConfig::default().with_max_messages(10);
        let mut limiter = RateLimiter::new(config);
        let peer = make_peer_id();

        // Send 5 messages (below limit of 10)
        for _ in 0..5 {
            let result = limiter.check_and_record(peer);
            assert!(result.is_allowed(), "Normal traffic should pass");
        }

        assert_eq!(limiter.message_count(&peer), 5);
        assert_eq!(limiter.violation_count(&peer), 0);
    }

    #[test]
    fn messages_at_exact_limit_pass() {
        let config = RateLimitConfig::default().with_max_messages(5);
        let mut limiter = RateLimiter::new(config);
        let peer = make_peer_id();

        // Send exactly 5 messages (at limit)
        for i in 0..5 {
            let result = limiter.check_and_record(peer);
            assert!(result.is_allowed(), "Message {} should pass", i);
        }
    }

    // ========== Rate Limiting Tests ==========

    #[test]
    fn excessive_traffic_gets_rate_limited() {
        let config = RateLimitConfig::default()
            .with_max_messages(5)
            .with_violations_before_ban(100); // High to prevent ban
        let mut limiter = RateLimiter::new(config);
        let peer = make_peer_id();

        // Fill up the limit
        for _ in 0..5 {
            limiter.check_and_record(peer);
        }

        // Next message should be rate limited
        let result = limiter.check_and_record(peer);
        assert_eq!(result, RateLimitResult::RateLimited);
        assert_eq!(limiter.violation_count(&peer), 1);
    }

    #[test]
    fn rate_limited_messages_increment_violations() {
        let config = RateLimitConfig::default()
            .with_max_messages(5)
            .with_violations_before_ban(100);
        let mut limiter = RateLimiter::new(config);
        let peer = make_peer_id();

        // Fill up the limit
        for _ in 0..5 {
            limiter.check_and_record(peer);
        }

        // Send 3 more (all rate limited)
        for _ in 0..3 {
            limiter.check_and_record(peer);
        }

        assert_eq!(limiter.violation_count(&peer), 3);
    }

    // ========== Independent Peer Limits Tests ==========

    #[test]
    fn different_peers_have_independent_limits() {
        let config = RateLimitConfig::default().with_max_messages(5);
        let mut limiter = RateLimiter::new(config);

        let peer1 = make_peer_id();
        let peer2 = make_peer_id();

        // Fill peer1's limit
        for _ in 0..5 {
            limiter.check_and_record(peer1);
        }

        // peer1 should be rate limited
        let result1 = limiter.check_and_record(peer1);
        assert_eq!(result1, RateLimitResult::RateLimited);

        // peer2 should still be allowed
        let result2 = limiter.check_and_record(peer2);
        assert!(result2.is_allowed(), "peer2 should not be affected by peer1's limit");

        // Verify counts
        assert_eq!(limiter.message_count(&peer1), 5);
        assert_eq!(limiter.message_count(&peer2), 1);
    }

    #[test]
    fn banning_one_peer_does_not_affect_others() {
        let config = RateLimitConfig::default()
            .with_max_messages(5)
            .with_violations_before_ban(2);
        let mut limiter = RateLimiter::new(config);

        let peer1 = make_peer_id();
        let peer2 = make_peer_id();

        // Get peer1 banned (exceed limit + 2 violations)
        for _ in 0..5 {
            limiter.check_and_record(peer1);
        }
        limiter.check_and_record(peer1); // violation 1
        let result = limiter.check_and_record(peer1); // violation 2 -> ban
        assert!(matches!(result, RateLimitResult::Banned { .. }));

        // peer2 should be unaffected
        assert!(!limiter.is_banned(&peer2));
        let result2 = limiter.check_and_record(peer2);
        assert!(result2.is_allowed());
    }

    // ========== Ban/Unban Behavior Tests ==========

    #[test]
    fn peer_gets_banned_after_violations() {
        let config = RateLimitConfig::default()
            .with_max_messages(5)
            .with_violations_before_ban(3);
        let mut limiter = RateLimiter::new(config);
        let peer = make_peer_id();

        // Fill limit
        for _ in 0..5 {
            limiter.check_and_record(peer);
        }

        // Violation 1 & 2: rate limited but not banned
        for i in 0..2 {
            let result = limiter.check_and_record(peer);
            assert_eq!(result, RateLimitResult::RateLimited, "violation {}", i + 1);
            assert!(!limiter.is_banned(&peer));
        }

        // Violation 3: should trigger ban
        let result = limiter.check_and_record(peer);
        assert!(matches!(result, RateLimitResult::Banned { .. }));
        assert!(limiter.is_banned(&peer));
    }

    #[test]
    fn banned_peer_messages_rejected() {
        let config = RateLimitConfig::default()
            .with_max_messages(5)
            .with_violations_before_ban(1);
        let mut limiter = RateLimiter::new(config);
        let peer = make_peer_id();

        // Get banned
        for _ in 0..5 {
            limiter.check_and_record(peer);
        }
        limiter.check_and_record(peer); // trigger ban

        // All subsequent messages should be rejected as banned
        for _ in 0..10 {
            let result = limiter.check_and_record(peer);
            assert!(matches!(result, RateLimitResult::Banned { .. }));
        }
    }

    #[test]
    fn manual_ban_works() {
        let mut limiter = RateLimiter::with_defaults();
        let peer = make_peer_id();

        assert!(!limiter.is_banned(&peer));
        limiter.ban_peer(peer);
        assert!(limiter.is_banned(&peer));

        let result = limiter.check_and_record(peer);
        assert!(matches!(result, RateLimitResult::Banned { .. }));
    }

    #[test]
    fn manual_unban_works() {
        let config = RateLimitConfig::default()
            .with_max_messages(5)
            .with_violations_before_ban(1);
        let mut limiter = RateLimiter::new(config);
        let peer = make_peer_id();

        // Get banned
        for _ in 0..5 {
            limiter.check_and_record(peer);
        }
        limiter.check_and_record(peer);
        assert!(limiter.is_banned(&peer));

        // Unban
        limiter.unban_peer(&peer);
        assert!(!limiter.is_banned(&peer));
        assert_eq!(limiter.violation_count(&peer), 0);

        // Should be allowed again
        let result = limiter.check_and_record(peer);
        assert!(result.is_allowed());
    }

    #[test]
    fn ban_expires_after_duration() {
        let config = RateLimitConfig::default()
            .with_max_messages(5)
            .with_violations_before_ban(1)
            .with_ban_duration(Duration::from_millis(50));
        let mut limiter = RateLimiter::new(config);
        let peer = make_peer_id();

        // Get banned
        for _ in 0..5 {
            limiter.check_and_record(peer);
        }
        limiter.check_and_record(peer);
        assert!(limiter.is_banned(&peer));

        // Wait for ban to expire
        std::thread::sleep(Duration::from_millis(100));

        // Should be allowed again
        let result = limiter.check_and_record(peer);
        assert!(result.is_allowed(), "Ban should have expired");
        assert!(!limiter.is_banned(&peer));
        assert_eq!(limiter.violation_count(&peer), 0); // Reset after ban expires
    }

    // ========== Sliding Window Tests ==========

    #[test]
    fn sliding_window_expires_old_messages() {
        let config = RateLimitConfig::default()
            .with_max_messages(5)
            .with_window_duration(Duration::from_millis(50));
        let mut limiter = RateLimiter::new(config);
        let peer = make_peer_id();

        // Fill limit
        for _ in 0..5 {
            limiter.check_and_record(peer);
        }

        // Should be rate limited now
        let result = limiter.check_and_record(peer);
        assert_eq!(result, RateLimitResult::RateLimited);

        // Wait for window to expire
        std::thread::sleep(Duration::from_millis(100));

        // Should be allowed again
        let result = limiter.check_and_record(peer);
        assert!(result.is_allowed(), "Old messages should have expired from window");
    }

    // ========== Disabled Rate Limiting Tests ==========

    #[test]
    fn disabled_rate_limiting_allows_all() {
        let config = RateLimitConfig::disabled();
        let mut limiter = RateLimiter::new(config);
        let peer = make_peer_id();

        // Should allow unlimited messages
        for _ in 0..1000 {
            let result = limiter.check_and_record(peer);
            assert!(result.is_allowed());
        }
    }

    // ========== Remove Peer Tests ==========

    #[test]
    fn remove_peer_clears_state() {
        let config = RateLimitConfig::default().with_max_messages(5);
        let mut limiter = RateLimiter::new(config);
        let peer = make_peer_id();

        // Build up state
        for _ in 0..5 {
            limiter.check_and_record(peer);
        }
        limiter.check_and_record(peer); // violation

        assert_eq!(limiter.message_count(&peer), 5);
        assert_eq!(limiter.violation_count(&peer), 1);

        // Remove peer
        limiter.remove_peer(&peer);

        assert_eq!(limiter.message_count(&peer), 0);
        assert_eq!(limiter.violation_count(&peer), 0);
        assert!(!limiter.is_banned(&peer));
    }

    // ========== Stats Tests ==========

    #[test]
    fn stats_reflect_state() {
        let config = RateLimitConfig::default()
            .with_max_messages(5)
            .with_violations_before_ban(1);
        let mut limiter = RateLimiter::new(config);

        let peer1 = make_peer_id();
        let peer2 = make_peer_id();
        let peer3 = make_peer_id();

        // peer1: normal traffic
        for _ in 0..3 {
            limiter.check_and_record(peer1);
        }

        // peer2: gets banned
        for _ in 0..5 {
            limiter.check_and_record(peer2);
        }
        limiter.check_and_record(peer2);

        // peer3: just one message
        limiter.check_and_record(peer3);

        let stats = limiter.stats();
        assert_eq!(stats.tracked_peers, 3);
        assert_eq!(stats.banned_peers, 1);
        assert_eq!(stats.total_violations, 1);
    }

    // ========== Proptest ==========

    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn messages_under_limit_always_pass(
                limit in 10u32..100,
                message_count in 1usize..10
            ) {
                let config = RateLimitConfig::default().with_max_messages(limit);
                let mut limiter = RateLimiter::new(config);
                let peer = make_peer_id();

                // Ensure we're under the limit
                let count = message_count.min(limit as usize - 1);

                for _ in 0..count {
                    let result = limiter.check_and_record(peer);
                    prop_assert!(result.is_allowed());
                }
            }

            #[test]
            fn messages_over_limit_get_limited(
                limit in 5u32..20,
                excess in 1usize..10
            ) {
                let config = RateLimitConfig::default()
                    .with_max_messages(limit)
                    .with_violations_before_ban(100); // Prevent banning
                let mut limiter = RateLimiter::new(config);
                let peer = make_peer_id();

                // Fill limit
                for _ in 0..limit {
                    limiter.check_and_record(peer);
                }

                // Excess should be rate limited
                for _ in 0..excess {
                    let result = limiter.check_and_record(peer);
                    prop_assert_eq!(result, RateLimitResult::RateLimited);
                }
            }

            #[test]
            fn peer_independence(num_peers in 2usize..10) {
                let config = RateLimitConfig::default().with_max_messages(10);
                let mut limiter = RateLimiter::new(config);

                let peers: Vec<_> = (0..num_peers).map(|_| make_peer_id()).collect();

                // Each peer sends 5 messages
                for peer in &peers {
                    for _ in 0..5 {
                        let result = limiter.check_and_record(*peer);
                        prop_assert!(result.is_allowed());
                    }
                }

                // Verify counts
                for peer in &peers {
                    prop_assert_eq!(limiter.message_count(peer), 5);
                }
            }
        }
    }
}
