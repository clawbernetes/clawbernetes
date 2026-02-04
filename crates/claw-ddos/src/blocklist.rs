//! IP blocklist with expiry support.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use tracing::{debug, info};

use crate::config::BlocklistConfig;
use crate::error::{DdosError, DdosResult};

/// Reason for blocking an IP.
#[derive(Debug, Clone)]
pub struct BlockReason {
    /// Human-readable reason.
    pub reason: String,
    /// When the block was created.
    pub created_at: DateTime<Utc>,
    /// When the block expires (None = permanent).
    pub expires_at: Option<DateTime<Utc>>,
    /// Number of times this IP has been blocked.
    pub block_count: u32,
}

impl BlockReason {
    /// Create a new block reason.
    #[must_use]
    pub fn new(reason: impl Into<String>, duration: Option<Duration>) -> Self {
        let now = Utc::now();
        let expires_at = duration.map(|d| {
            now + chrono::Duration::milliseconds(d.as_millis() as i64)
        });
        
        Self {
            reason: reason.into(),
            created_at: now,
            expires_at,
            block_count: 1,
        }
    }

    /// Create a permanent block.
    #[must_use]
    pub fn permanent(reason: impl Into<String>) -> Self {
        Self::new(reason, None)
    }

    /// Create a temporary block.
    #[must_use]
    pub fn temporary(reason: impl Into<String>, duration: Duration) -> Self {
        Self::new(reason, Some(duration))
    }

    /// Check if this block has expired.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| Utc::now() > exp)
    }

    /// Check if this block is permanent.
    #[must_use]
    pub const fn is_permanent(&self) -> bool {
        self.expires_at.is_none()
    }

    /// Get remaining duration (None if permanent or expired).
    #[must_use]
    pub fn remaining(&self) -> Option<Duration> {
        self.expires_at.and_then(|exp| {
            let now = Utc::now();
            if exp > now {
                let diff = exp - now;
                Some(Duration::from_millis(diff.num_milliseconds() as u64))
            } else {
                None
            }
        })
    }
}

/// Entry in the blocklist with internal timing.
#[derive(Debug)]
struct BlockEntry {
    /// Block reason and metadata.
    reason: BlockReason,
    /// Internal timing for fast expiry checks.
    expires_instant: Option<Instant>,
}

impl BlockEntry {
    fn new(reason: BlockReason, duration: Option<Duration>) -> Self {
        let expires_instant = duration.map(|d| Instant::now() + d);
        Self {
            reason,
            expires_instant,
        }
    }

    fn is_expired(&self) -> bool {
        self.expires_instant.is_some_and(|exp| Instant::now() > exp)
    }
}

/// IP blocklist with automatic expiry.
#[derive(Debug)]
pub struct IpBlocklist {
    /// Blocked IPs and reasons.
    blocked: RwLock<HashMap<IpAddr, BlockEntry>>,
    /// Default block duration for temporary bans.
    default_duration: Duration,
    /// Maximum block duration.
    max_duration: Duration,
    /// Last cleanup time.
    last_cleanup: RwLock<Instant>,
    /// Cleanup interval.
    cleanup_interval: Duration,
}

impl IpBlocklist {
    /// Create a new blocklist with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            blocked: RwLock::new(HashMap::new()),
            default_duration: Duration::from_secs(300),   // 5 minutes
            max_duration: Duration::from_secs(86400),     // 24 hours
            last_cleanup: RwLock::new(Instant::now()),
            cleanup_interval: Duration::from_secs(60),    // Cleanup every minute
        }
    }

    /// Create from configuration.
    #[must_use]
    pub fn from_config(config: &BlocklistConfig) -> Self {
        Self {
            blocked: RwLock::new(HashMap::new()),
            default_duration: config.default_block_duration,
            max_duration: config.max_block_duration,
            last_cleanup: RwLock::new(Instant::now()),
            cleanup_interval: Duration::from_secs(60),
        }
    }

    /// Block an IP address.
    pub fn block(&self, ip: &IpAddr, reason: impl Into<String>, duration: Option<Duration>) {
        let reason_str = reason.into();
        let actual_duration = duration.map(|d| d.min(self.max_duration));
        
        let block_reason = match actual_duration {
            Some(d) => BlockReason::temporary(&reason_str, d),
            None => BlockReason::permanent(&reason_str),
        };
        
        let entry = BlockEntry::new(block_reason.clone(), actual_duration);
        
        let mut blocked = self.blocked.write();
        
        // If already blocked, increment the counter
        if let Some(existing) = blocked.get_mut(ip) {
            existing.reason.block_count = existing.reason.block_count.saturating_add(1);
            existing.reason.reason.clone_from(&reason_str);
            existing.reason.created_at = Utc::now();
            existing.reason.expires_at = block_reason.expires_at;
            existing.expires_instant = actual_duration.map(|d| Instant::now() + d);
            
            info!(
                ip = %ip,
                reason = %reason_str,
                block_count = existing.reason.block_count,
                "IP re-blocked"
            );
        } else {
            blocked.insert(*ip, entry);
            info!(ip = %ip, reason = %reason_str, "IP blocked");
        }
        
        self.maybe_cleanup();
    }

    /// Block an IP with the default duration.
    pub fn block_default(&self, ip: &IpAddr, reason: impl Into<String>) {
        self.block(ip, reason, Some(self.default_duration));
    }

    /// Block an IP permanently.
    pub fn block_permanent(&self, ip: &IpAddr, reason: impl Into<String>) {
        self.block(ip, reason, None);
    }

    /// Unblock an IP address.
    pub fn unblock(&self, ip: &IpAddr) -> bool {
        let removed = self.blocked.write().remove(ip).is_some();
        if removed {
            info!(ip = %ip, "IP unblocked");
        }
        removed
    }

    /// Check if an IP is blocked.
    ///
    /// # Errors
    ///
    /// Returns `DdosError::Blocked` if the IP is blocked.
    pub fn check(&self, ip: &IpAddr) -> DdosResult<()> {
        self.maybe_cleanup();
        
        let blocked = self.blocked.read();
        
        if let Some(entry) = blocked.get(ip) {
            if !entry.is_expired() {
                return Err(DdosError::Blocked {
                    ip: *ip,
                    reason: entry.reason.reason.clone(),
                });
            }
        }
        
        Ok(())
    }

    /// Check if an IP is blocked (returns bool instead of Result).
    #[must_use]
    pub fn is_blocked(&self, ip: &IpAddr) -> bool {
        self.check(ip).is_err()
    }

    /// Get the block reason for an IP (if blocked).
    #[must_use]
    pub fn get_block_reason(&self, ip: &IpAddr) -> Option<BlockReason> {
        let blocked = self.blocked.read();
        
        blocked.get(ip).and_then(|entry| {
            if entry.is_expired() {
                None
            } else {
                Some(entry.reason.clone())
            }
        })
    }

    /// Get the number of times an IP has been blocked.
    #[must_use]
    pub fn block_count(&self, ip: &IpAddr) -> u32 {
        self.blocked
            .read()
            .get(ip)
            .map_or(0, |e| e.reason.block_count)
    }

    /// Get number of currently blocked IPs.
    #[must_use]
    pub fn blocked_count(&self) -> usize {
        self.blocked.read().len()
    }

    /// Get all currently blocked IPs.
    #[must_use]
    pub fn list_blocked(&self) -> Vec<(IpAddr, BlockReason)> {
        self.blocked
            .read()
            .iter()
            .filter(|(_, entry)| !entry.is_expired())
            .map(|(ip, entry)| (*ip, entry.reason.clone()))
            .collect()
    }

    /// Clear all blocks.
    pub fn clear(&self) {
        self.blocked.write().clear();
        info!("Blocklist cleared");
    }

    /// Manually trigger cleanup of expired entries.
    pub fn cleanup(&self) -> usize {
        let mut blocked = self.blocked.write();
        let initial_count = blocked.len();
        
        blocked.retain(|ip, entry| {
            let keep = !entry.is_expired();
            if !keep {
                debug!(ip = %ip, "Block expired, removing");
            }
            keep
        });
        
        let removed = initial_count.saturating_sub(blocked.len());
        if removed > 0 {
            debug!(removed = removed, "Cleaned up expired blocks");
        }
        
        *self.last_cleanup.write() = Instant::now();
        removed
    }

    /// Maybe cleanup if enough time has passed.
    fn maybe_cleanup(&self) {
        let should_cleanup = {
            let last = *self.last_cleanup.read();
            last.elapsed() >= self.cleanup_interval
        };
        
        if should_cleanup {
            self.cleanup();
        }
    }

    /// Get the default block duration.
    #[must_use]
    pub const fn default_duration(&self) -> Duration {
        self.default_duration
    }

    /// Get the maximum block duration.
    #[must_use]
    pub const fn max_duration(&self) -> Duration {
        self.max_duration
    }
}

impl Default for IpBlocklist {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    // ==================== BlockReason Tests ====================

    #[test]
    fn test_block_reason_temporary() {
        let reason = BlockReason::temporary("test", Duration::from_secs(60));
        
        assert_eq!(reason.reason, "test");
        assert!(!reason.is_permanent());
        assert!(!reason.is_expired());
        assert!(reason.remaining().is_some());
        assert_eq!(reason.block_count, 1);
    }

    #[test]
    fn test_block_reason_permanent() {
        let reason = BlockReason::permanent("test");
        
        assert!(reason.is_permanent());
        assert!(!reason.is_expired());
        assert!(reason.remaining().is_none());
    }

    #[test]
    fn test_block_reason_expiry() {
        let reason = BlockReason::temporary("test", Duration::from_millis(10));
        
        assert!(!reason.is_expired());
        thread::sleep(Duration::from_millis(20));
        assert!(reason.is_expired());
    }

    #[test]
    fn test_block_reason_remaining() {
        let reason = BlockReason::temporary("test", Duration::from_secs(1));
        
        let remaining = reason.remaining().unwrap();
        assert!(remaining <= Duration::from_secs(1));
        assert!(remaining > Duration::from_millis(900));
    }

    // ==================== IpBlocklist Tests ====================

    #[test]
    fn test_blocklist_new() {
        let blocklist = IpBlocklist::new();
        assert_eq!(blocklist.blocked_count(), 0);
    }

    #[test]
    fn test_blocklist_from_config() {
        let config = BlocklistConfig {
            default_block_duration: Duration::from_secs(120),
            max_block_duration: Duration::from_secs(3600),
            persist: false,
            persist_path: None,
        };
        let blocklist = IpBlocklist::from_config(&config);
        
        assert_eq!(blocklist.default_duration(), Duration::from_secs(120));
        assert_eq!(blocklist.max_duration(), Duration::from_secs(3600));
    }

    #[test]
    fn test_blocklist_block_and_check() {
        let blocklist = IpBlocklist::new();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        // Not blocked initially
        assert!(blocklist.check(&ip).is_ok());
        assert!(!blocklist.is_blocked(&ip));
        
        // Block it
        blocklist.block(&ip, "test reason", Some(Duration::from_secs(60)));
        
        // Should be blocked now
        assert!(blocklist.is_blocked(&ip));
        
        let result = blocklist.check(&ip);
        assert!(matches!(result, Err(DdosError::Blocked { .. })));
    }

    #[test]
    fn test_blocklist_block_default() {
        let blocklist = IpBlocklist::new();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        blocklist.block_default(&ip, "test");
        assert!(blocklist.is_blocked(&ip));
    }

    #[test]
    fn test_blocklist_block_permanent() {
        let blocklist = IpBlocklist::new();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        blocklist.block_permanent(&ip, "permanent block");
        
        let reason = blocklist.get_block_reason(&ip).unwrap();
        assert!(reason.is_permanent());
    }

    #[test]
    fn test_blocklist_unblock() {
        let blocklist = IpBlocklist::new();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        blocklist.block(&ip, "test", Some(Duration::from_secs(60)));
        assert!(blocklist.is_blocked(&ip));
        
        let removed = blocklist.unblock(&ip);
        assert!(removed);
        assert!(!blocklist.is_blocked(&ip));
        
        // Unblocking non-blocked IP returns false
        let removed = blocklist.unblock(&ip);
        assert!(!removed);
    }

    #[test]
    fn test_blocklist_get_block_reason() {
        let blocklist = IpBlocklist::new();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        // No reason for non-blocked IP
        assert!(blocklist.get_block_reason(&ip).is_none());
        
        blocklist.block(&ip, "suspicious activity", Some(Duration::from_secs(60)));
        
        let reason = blocklist.get_block_reason(&ip).unwrap();
        assert_eq!(reason.reason, "suspicious activity");
    }

    #[test]
    fn test_blocklist_block_count_increment() {
        let blocklist = IpBlocklist::new();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        blocklist.block(&ip, "first", Some(Duration::from_secs(60)));
        assert_eq!(blocklist.block_count(&ip), 1);
        
        blocklist.block(&ip, "second", Some(Duration::from_secs(60)));
        assert_eq!(blocklist.block_count(&ip), 2);
        
        blocklist.block(&ip, "third", Some(Duration::from_secs(60)));
        assert_eq!(blocklist.block_count(&ip), 3);
        
        // Reason should be updated
        let reason = blocklist.get_block_reason(&ip).unwrap();
        assert_eq!(reason.reason, "third");
    }

    #[test]
    fn test_blocklist_expiry() {
        let blocklist = IpBlocklist::new();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        blocklist.block(&ip, "test", Some(Duration::from_millis(10)));
        assert!(blocklist.is_blocked(&ip));
        
        thread::sleep(Duration::from_millis(20));
        
        // Should no longer be blocked after expiry
        assert!(!blocklist.is_blocked(&ip));
    }

    #[test]
    fn test_blocklist_max_duration_cap() {
        let config = BlocklistConfig {
            default_block_duration: Duration::from_secs(60),
            max_block_duration: Duration::from_secs(100),
            persist: false,
            persist_path: None,
        };
        let blocklist = IpBlocklist::from_config(&config);
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        // Try to block for longer than max
        blocklist.block(&ip, "test", Some(Duration::from_secs(1000)));
        
        let reason = blocklist.get_block_reason(&ip).unwrap();
        // Duration should be capped at max
        let remaining = reason.remaining().unwrap();
        assert!(remaining <= Duration::from_secs(100));
    }

    #[test]
    fn test_blocklist_list_blocked() {
        let blocklist = IpBlocklist::new();
        let ip1: IpAddr = "1.2.3.4".parse().unwrap();
        let ip2: IpAddr = "5.6.7.8".parse().unwrap();
        
        blocklist.block(&ip1, "reason1", Some(Duration::from_secs(60)));
        blocklist.block(&ip2, "reason2", Some(Duration::from_secs(60)));
        
        let blocked = blocklist.list_blocked();
        assert_eq!(blocked.len(), 2);
        
        let ips: Vec<_> = blocked.iter().map(|(ip, _)| *ip).collect();
        assert!(ips.contains(&ip1));
        assert!(ips.contains(&ip2));
    }

    #[test]
    fn test_blocklist_cleanup() {
        let blocklist = IpBlocklist::new();
        let ip1: IpAddr = "1.2.3.4".parse().unwrap();
        let ip2: IpAddr = "5.6.7.8".parse().unwrap();
        
        blocklist.block(&ip1, "expires soon", Some(Duration::from_millis(10)));
        blocklist.block(&ip2, "expires later", Some(Duration::from_secs(60)));
        
        assert_eq!(blocklist.blocked_count(), 2);
        
        thread::sleep(Duration::from_millis(20));
        
        let removed = blocklist.cleanup();
        assert_eq!(removed, 1);
        assert_eq!(blocklist.blocked_count(), 1);
        assert!(!blocklist.is_blocked(&ip1));
        assert!(blocklist.is_blocked(&ip2));
    }

    #[test]
    fn test_blocklist_clear() {
        let blocklist = IpBlocklist::new();
        let ip1: IpAddr = "1.2.3.4".parse().unwrap();
        let ip2: IpAddr = "5.6.7.8".parse().unwrap();
        
        blocklist.block(&ip1, "test", Some(Duration::from_secs(60)));
        blocklist.block(&ip2, "test", Some(Duration::from_secs(60)));
        assert_eq!(blocklist.blocked_count(), 2);
        
        blocklist.clear();
        assert_eq!(blocklist.blocked_count(), 0);
    }

    #[test]
    fn test_blocklist_multiple_ips() {
        let blocklist = IpBlocklist::new();
        let ip1: IpAddr = "1.2.3.4".parse().unwrap();
        let ip2: IpAddr = "5.6.7.8".parse().unwrap();
        let ip3: IpAddr = "10.0.0.1".parse().unwrap();
        
        blocklist.block(&ip1, "blocked", Some(Duration::from_secs(60)));
        
        assert!(blocklist.is_blocked(&ip1));
        assert!(!blocklist.is_blocked(&ip2));
        assert!(!blocklist.is_blocked(&ip3));
    }

    #[test]
    fn test_blocklist_default() {
        let blocklist = IpBlocklist::default();
        assert_eq!(blocklist.blocked_count(), 0);
    }

    #[test]
    fn test_block_count_non_blocked_ip() {
        let blocklist = IpBlocklist::new();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        assert_eq!(blocklist.block_count(&ip), 0);
    }
}
