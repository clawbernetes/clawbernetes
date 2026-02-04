//! Connection-level protections.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use tracing::{debug, warn};

use crate::config::ConnectionConfig;
use crate::error::{DdosError, DdosResult};

/// Tracks active connections per IP address.
#[derive(Debug)]
struct ConnectionCount {
    /// Number of active connections.
    count: u32,
    /// When the first connection was opened.
    first_seen: Instant,
}

/// Limits concurrent connections per IP address.
#[derive(Debug)]
pub struct ConnectionLimiter {
    /// Maximum connections per IP.
    max_per_ip: u32,
    /// Active connections by IP.
    connections: RwLock<HashMap<IpAddr, ConnectionCount>>,
    /// Total connections tracked.
    total_connections: AtomicU64,
}

impl ConnectionLimiter {
    /// Create a new connection limiter.
    #[must_use]
    pub fn new(max_per_ip: u32) -> Self {
        Self {
            max_per_ip,
            connections: RwLock::new(HashMap::new()),
            total_connections: AtomicU64::new(0),
        }
    }

    /// Create from configuration.
    #[must_use]
    pub fn from_config(config: &ConnectionConfig) -> Self {
        Self::new(config.max_per_ip)
    }

    /// Check if a new connection from the given IP is allowed.
    ///
    /// # Errors
    ///
    /// Returns `DdosError::ConnectionLimitExceeded` if the IP has too many connections.
    pub fn check(&self, ip: &IpAddr) -> DdosResult<()> {
        let connections = self.connections.read();
        
        if let Some(conn) = connections.get(ip) {
            if conn.count >= self.max_per_ip {
                return Err(DdosError::ConnectionLimitExceeded {
                    ip: *ip,
                    current: conn.count,
                    max: self.max_per_ip,
                });
            }
        }
        
        Ok(())
    }

    /// Record a new connection from an IP.
    pub fn add_connection(&self, ip: &IpAddr) {
        let mut connections = self.connections.write();
        
        let entry = connections.entry(*ip).or_insert(ConnectionCount {
            count: 0,
            first_seen: Instant::now(),
        });
        
        entry.count = entry.count.saturating_add(1);
        self.total_connections.fetch_add(1, Ordering::Relaxed);
        
        debug!(ip = %ip, count = entry.count, "Connection added");
    }

    /// Record a connection closure from an IP.
    pub fn remove_connection(&self, ip: &IpAddr) {
        let mut connections = self.connections.write();
        
        if let Some(conn) = connections.get_mut(ip) {
            conn.count = conn.count.saturating_sub(1);
            self.total_connections.fetch_sub(1, Ordering::Relaxed);
            
            debug!(ip = %ip, count = conn.count, "Connection removed");
            
            // Clean up if no connections left
            if conn.count == 0 {
                connections.remove(ip);
            }
        }
    }

    /// Get current connection count for an IP.
    #[must_use]
    pub fn connection_count(&self, ip: &IpAddr) -> u32 {
        self.connections
            .read()
            .get(ip)
            .map_or(0, |c| c.count)
    }

    /// Get total tracked connections.
    #[must_use]
    pub fn total_connections(&self) -> u64 {
        self.total_connections.load(Ordering::Relaxed)
    }

    /// Get number of unique IPs with active connections.
    #[must_use]
    pub fn unique_ips(&self) -> usize {
        self.connections.read().len()
    }

    /// Clear all connection tracking.
    pub fn clear(&self) {
        let mut connections = self.connections.write();
        connections.clear();
        self.total_connections.store(0, Ordering::Relaxed);
    }
}

/// Tracks handshake state for slow loris protection.
#[derive(Debug)]
struct HandshakeState {
    /// When the handshake started.
    started_at: Instant,
    /// Whether the handshake has completed.
    completed: bool,
}

/// Protection against slow loris attacks (slow/incomplete handshakes).
#[derive(Debug)]
pub struct SlowLorisProtection {
    /// Timeout for handshake completion.
    handshake_timeout: Duration,
    /// Timeout for idle connections.
    idle_timeout: Duration,
    /// Pending handshakes by connection ID.
    pending: RwLock<HashMap<u64, (IpAddr, HandshakeState)>>,
    /// Next connection ID.
    next_id: AtomicU64,
}

impl SlowLorisProtection {
    /// Create new slow loris protection.
    #[must_use]
    pub fn new(handshake_timeout: Duration, idle_timeout: Duration) -> Self {
        Self {
            handshake_timeout,
            idle_timeout,
            pending: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Create from configuration.
    #[must_use]
    pub fn from_config(config: &ConnectionConfig) -> Self {
        Self::new(config.handshake_timeout, config.idle_timeout)
    }

    /// Start tracking a new connection handshake.
    /// Returns a connection ID for later reference.
    #[must_use]
    pub fn start_handshake(&self, ip: &IpAddr) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        
        let state = HandshakeState {
            started_at: Instant::now(),
            completed: false,
        };
        
        self.pending.write().insert(id, (*ip, state));
        debug!(connection_id = id, ip = %ip, "Handshake started");
        
        id
    }

    /// Mark a handshake as completed.
    pub fn complete_handshake(&self, connection_id: u64) {
        let mut pending = self.pending.write();
        
        if let Some((ip, state)) = pending.get_mut(&connection_id) {
            state.completed = true;
            debug!(connection_id = connection_id, ip = %ip, "Handshake completed");
        }
    }

    /// Remove a connection from tracking.
    pub fn remove_connection(&self, connection_id: u64) {
        self.pending.write().remove(&connection_id);
    }

    /// Check if a handshake has timed out.
    #[must_use]
    pub fn is_timed_out(&self, connection_id: u64) -> bool {
        let pending = self.pending.read();
        
        if let Some((_, state)) = pending.get(&connection_id) {
            if !state.completed && state.started_at.elapsed() > self.handshake_timeout {
                return true;
            }
        }
        
        false
    }

    /// Clean up timed-out handshakes and return their IPs.
    pub fn cleanup_timed_out(&self) -> Vec<(u64, IpAddr)> {
        let mut pending = self.pending.write();
        let now = Instant::now();
        
        let timed_out: Vec<_> = pending
            .iter()
            .filter(|(_, (_, state))| {
                !state.completed && now.duration_since(state.started_at) > self.handshake_timeout
            })
            .map(|(id, (ip, _))| (*id, *ip))
            .collect();
        
        for (id, ip) in &timed_out {
            warn!(connection_id = id, ip = %ip, "Handshake timed out (slow loris detected)");
            pending.remove(id);
        }
        
        timed_out
    }

    /// Get count of pending handshakes.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.pending
            .read()
            .values()
            .filter(|(_, state)| !state.completed)
            .count()
    }

    /// Get total tracked connections.
    #[must_use]
    pub fn tracked_count(&self) -> usize {
        self.pending.read().len()
    }

    /// Get the handshake timeout duration.
    #[must_use]
    pub const fn handshake_timeout(&self) -> Duration {
        self.handshake_timeout
    }

    /// Get the idle timeout duration.
    #[must_use]
    pub const fn idle_timeout(&self) -> Duration {
        self.idle_timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== ConnectionLimiter Tests ====================

    #[test]
    fn test_connection_limiter_new() {
        let limiter = ConnectionLimiter::new(5);
        assert_eq!(limiter.total_connections(), 0);
        assert_eq!(limiter.unique_ips(), 0);
    }

    #[test]
    fn test_connection_limiter_from_config() {
        let config = ConnectionConfig {
            max_per_ip: 20,
            ..ConnectionConfig::default()
        };
        let limiter = ConnectionLimiter::from_config(&config);
        
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        for _ in 0..20 {
            assert!(limiter.check(&ip).is_ok());
            limiter.add_connection(&ip);
        }
        assert!(limiter.check(&ip).is_err());
    }

    #[test]
    fn test_connection_limiter_allows_under_limit() {
        let limiter = ConnectionLimiter::new(3);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();

        // First connection
        assert!(limiter.check(&ip).is_ok());
        limiter.add_connection(&ip);
        assert_eq!(limiter.connection_count(&ip), 1);

        // Second connection
        assert!(limiter.check(&ip).is_ok());
        limiter.add_connection(&ip);
        assert_eq!(limiter.connection_count(&ip), 2);

        // Third connection
        assert!(limiter.check(&ip).is_ok());
        limiter.add_connection(&ip);
        assert_eq!(limiter.connection_count(&ip), 3);
    }

    #[test]
    fn test_connection_limiter_blocks_over_limit() {
        let limiter = ConnectionLimiter::new(2);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();

        limiter.add_connection(&ip);
        limiter.add_connection(&ip);

        // Third should be blocked
        let result = limiter.check(&ip);
        assert!(matches!(
            result,
            Err(DdosError::ConnectionLimitExceeded { current: 2, max: 2, .. })
        ));
    }

    #[test]
    fn test_connection_limiter_remove_connection() {
        let limiter = ConnectionLimiter::new(2);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();

        limiter.add_connection(&ip);
        limiter.add_connection(&ip);
        assert_eq!(limiter.connection_count(&ip), 2);

        limiter.remove_connection(&ip);
        assert_eq!(limiter.connection_count(&ip), 1);

        // Should be able to add one more now
        assert!(limiter.check(&ip).is_ok());
    }

    #[test]
    fn test_connection_limiter_cleanup_on_zero() {
        let limiter = ConnectionLimiter::new(5);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();

        limiter.add_connection(&ip);
        assert_eq!(limiter.unique_ips(), 1);

        limiter.remove_connection(&ip);
        assert_eq!(limiter.unique_ips(), 0);
        assert_eq!(limiter.connection_count(&ip), 0);
    }

    #[test]
    fn test_connection_limiter_multiple_ips() {
        let limiter = ConnectionLimiter::new(2);
        let ip1: IpAddr = "1.2.3.4".parse().unwrap();
        let ip2: IpAddr = "5.6.7.8".parse().unwrap();

        limiter.add_connection(&ip1);
        limiter.add_connection(&ip1);
        limiter.add_connection(&ip2);

        assert_eq!(limiter.connection_count(&ip1), 2);
        assert_eq!(limiter.connection_count(&ip2), 1);
        assert_eq!(limiter.total_connections(), 3);
        assert_eq!(limiter.unique_ips(), 2);

        // ip1 is at limit
        assert!(limiter.check(&ip1).is_err());
        // ip2 still has room
        assert!(limiter.check(&ip2).is_ok());
    }

    #[test]
    fn test_connection_limiter_clear() {
        let limiter = ConnectionLimiter::new(5);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();

        limiter.add_connection(&ip);
        limiter.add_connection(&ip);
        
        limiter.clear();
        
        assert_eq!(limiter.total_connections(), 0);
        assert_eq!(limiter.unique_ips(), 0);
        assert_eq!(limiter.connection_count(&ip), 0);
    }

    #[test]
    fn test_connection_limiter_saturating_operations() {
        let limiter = ConnectionLimiter::new(u32::MAX);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();

        // Remove from non-existent IP shouldn't panic
        limiter.remove_connection(&ip);
        assert_eq!(limiter.connection_count(&ip), 0);
    }

    // ==================== SlowLorisProtection Tests ====================

    #[test]
    fn test_slow_loris_new() {
        let protection = SlowLorisProtection::new(
            Duration::from_secs(10),
            Duration::from_secs(300),
        );
        
        assert_eq!(protection.pending_count(), 0);
        assert_eq!(protection.tracked_count(), 0);
    }

    #[test]
    fn test_slow_loris_from_config() {
        let config = ConnectionConfig {
            handshake_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(60),
            ..ConnectionConfig::default()
        };
        let protection = SlowLorisProtection::from_config(&config);
        
        assert_eq!(protection.handshake_timeout(), Duration::from_secs(5));
        assert_eq!(protection.idle_timeout(), Duration::from_secs(60));
    }

    #[test]
    fn test_slow_loris_start_handshake() {
        let protection = SlowLorisProtection::new(
            Duration::from_secs(10),
            Duration::from_secs(300),
        );
        let ip: IpAddr = "1.2.3.4".parse().unwrap();

        let id = protection.start_handshake(&ip);
        assert!(id > 0);
        assert_eq!(protection.pending_count(), 1);
        assert_eq!(protection.tracked_count(), 1);
    }

    #[test]
    fn test_slow_loris_complete_handshake() {
        let protection = SlowLorisProtection::new(
            Duration::from_secs(10),
            Duration::from_secs(300),
        );
        let ip: IpAddr = "1.2.3.4".parse().unwrap();

        let id = protection.start_handshake(&ip);
        assert_eq!(protection.pending_count(), 1);

        protection.complete_handshake(id);
        assert_eq!(protection.pending_count(), 0);
        assert_eq!(protection.tracked_count(), 1); // Still tracked
    }

    #[test]
    fn test_slow_loris_remove_connection() {
        let protection = SlowLorisProtection::new(
            Duration::from_secs(10),
            Duration::from_secs(300),
        );
        let ip: IpAddr = "1.2.3.4".parse().unwrap();

        let id = protection.start_handshake(&ip);
        protection.remove_connection(id);
        
        assert_eq!(protection.pending_count(), 0);
        assert_eq!(protection.tracked_count(), 0);
    }

    #[test]
    fn test_slow_loris_timeout_detection() {
        let protection = SlowLorisProtection::new(
            Duration::from_millis(1), // Very short timeout for testing
            Duration::from_secs(300),
        );
        let ip: IpAddr = "1.2.3.4".parse().unwrap();

        let id = protection.start_handshake(&ip);
        
        // Sleep to exceed timeout
        std::thread::sleep(Duration::from_millis(10));
        
        assert!(protection.is_timed_out(id));
    }

    #[test]
    fn test_slow_loris_no_timeout_if_completed() {
        let protection = SlowLorisProtection::new(
            Duration::from_millis(1),
            Duration::from_secs(300),
        );
        let ip: IpAddr = "1.2.3.4".parse().unwrap();

        let id = protection.start_handshake(&ip);
        protection.complete_handshake(id);
        
        std::thread::sleep(Duration::from_millis(10));
        
        // Completed handshakes don't time out
        assert!(!protection.is_timed_out(id));
    }

    #[test]
    fn test_slow_loris_cleanup() {
        let protection = SlowLorisProtection::new(
            Duration::from_millis(1),
            Duration::from_secs(300),
        );
        let ip1: IpAddr = "1.2.3.4".parse().unwrap();
        let ip2: IpAddr = "5.6.7.8".parse().unwrap();

        let id1 = protection.start_handshake(&ip1);
        let id2 = protection.start_handshake(&ip2);
        protection.complete_handshake(id2); // Complete one

        std::thread::sleep(Duration::from_millis(10));

        let timed_out = protection.cleanup_timed_out();
        
        // Only incomplete handshake should be cleaned up
        assert_eq!(timed_out.len(), 1);
        assert_eq!(timed_out[0].0, id1);
        assert_eq!(timed_out[0].1, ip1);
        
        // Completed one should still be tracked
        assert_eq!(protection.tracked_count(), 1);
    }

    #[test]
    fn test_slow_loris_unique_ids() {
        let protection = SlowLorisProtection::new(
            Duration::from_secs(10),
            Duration::from_secs(300),
        );
        let ip: IpAddr = "1.2.3.4".parse().unwrap();

        let id1 = protection.start_handshake(&ip);
        let id2 = protection.start_handshake(&ip);
        let id3 = protection.start_handshake(&ip);

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_slow_loris_nonexistent_connection() {
        let protection = SlowLorisProtection::new(
            Duration::from_secs(10),
            Duration::from_secs(300),
        );

        // Operations on non-existent connections shouldn't panic
        protection.complete_handshake(999);
        protection.remove_connection(999);
        assert!(!protection.is_timed_out(999));
    }
}
