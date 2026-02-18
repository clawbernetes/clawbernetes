//! Fanout/epidemic broadcast implementation for gossip.

use super::rate_limit::{RateLimitConfig, RateLimitResult, RateLimiter};
use super::{CapacityAnnouncement, GossipMessage, GossipQuery, MessageId, QueryFilter};
use crate::error::P2pError;
use crate::protocol::PeerId;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

/// Configuration for gossip broadcast behavior.
#[derive(Debug, Clone)]
pub struct BroadcastConfig {
    /// Number of peers to forward each message to (fanout factor).
    pub fanout: usize,
    /// Maximum number of hops a message can travel.
    pub max_ttl_hops: u8,
    /// How long to remember seen message IDs for deduplication.
    pub seen_cache_ttl: Duration,
    /// Maximum number of seen message IDs to cache.
    pub max_seen_cache: usize,
    /// How long to cache announcements before expiry.
    pub announcement_cache_ttl: Duration,
    /// Maximum announcements to cache per peer.
    pub max_announcements_per_peer: usize,
    /// Maximum total announcements across all peers (prevents DoS via memory exhaustion).
    pub max_total_announcements: usize,
    /// Interval between cleanup sweeps.
    pub cleanup_interval: Duration,
    /// Rate limiting configuration.
    pub rate_limit: RateLimitConfig,
}

impl Default for BroadcastConfig {
    fn default() -> Self {
        Self {
            fanout: 3,
            max_ttl_hops: 6,
            seen_cache_ttl: Duration::from_secs(300),
            max_seen_cache: 10_000,
            announcement_cache_ttl: Duration::from_secs(600),
            max_announcements_per_peer: 5,
            max_total_announcements: 10_000,
            cleanup_interval: Duration::from_secs(60),
            rate_limit: RateLimitConfig::default(),
        }
    }
}

impl BroadcastConfig {
    /// Creates a config optimized for small networks.
    #[must_use]
    pub fn small_network() -> Self {
        Self {
            fanout: 2,
            max_ttl_hops: 4,
            ..Self::default()
        }
    }

    /// Creates a config optimized for large networks.
    #[must_use]
    pub fn large_network() -> Self {
        Self {
            fanout: 5,
            max_ttl_hops: 8,
            max_seen_cache: 50_000,
            ..Self::default()
        }
    }

    /// Sets the fanout factor.
    #[must_use]
    pub const fn with_fanout(mut self, fanout: usize) -> Self {
        self.fanout = fanout;
        self
    }

    /// Sets the maximum TTL hops.
    #[must_use]
    pub const fn with_max_ttl(mut self, ttl: u8) -> Self {
        self.max_ttl_hops = ttl;
        self
    }

    /// Sets the maximum total announcements across all peers.
    #[must_use]
    pub const fn with_max_total_announcements(mut self, max: usize) -> Self {
        self.max_total_announcements = max;
        self
    }
}

/// Result of a broadcast operation.
#[derive(Debug, Clone)]
pub struct BroadcastResult {
    /// Message ID of the broadcast.
    pub message_id: MessageId,
    /// Peers selected for forwarding.
    pub target_peers: Vec<PeerId>,
    /// Whether this was a duplicate (already seen).
    pub was_duplicate: bool,
}

/// Entry in the seen message cache.
#[derive(Debug, Clone)]
struct SeenEntry {
    /// When this entry was added.
    added_at: Instant,
    /// The peer that first sent this message to us.
    #[allow(dead_code)]
    from_peer: PeerId,
}

/// Unique key for identifying cached announcements in LRU order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct AnnouncementKey {
    /// Peer ID that owns this announcement.
    peer_id: PeerId,
    /// Sequence number for this peer's announcements (allows multiple per peer).
    seq: u64,
}

/// Cached announcement with metadata.
#[derive(Debug, Clone)]
struct CachedAnnouncement {
    /// The announcement.
    announcement: CapacityAnnouncement,
    /// When this was cached.
    cached_at: Instant,
    /// Unique key for LRU tracking.
    key: AnnouncementKey,
}

/// Gossip broadcaster handling fanout/epidemic message propagation.
#[derive(Debug)]
pub struct GossipBroadcaster {
    /// Our peer ID.
    local_peer_id: PeerId,
    /// Configuration.
    config: BroadcastConfig,
    /// Known peers available for forwarding.
    known_peers: HashSet<PeerId>,
    /// Message IDs we've already seen (for deduplication).
    seen_messages: HashMap<MessageId, SeenEntry>,
    /// Order of seen messages for LRU eviction.
    seen_order: VecDeque<MessageId>,
    /// Cached announcements by peer ID.
    announcement_cache: HashMap<PeerId, Vec<CachedAnnouncement>>,
    /// LRU order of announcement keys for eviction when total limit is reached.
    announcement_order: VecDeque<AnnouncementKey>,
    /// Next sequence number for announcement keys.
    next_announcement_seq: u64,
    /// Last cleanup time.
    last_cleanup: Instant,
    /// Per-peer rate limiter to prevent DoS attacks.
    rate_limiter: RateLimiter,
}

impl GossipBroadcaster {
    /// Creates a new gossip broadcaster.
    #[must_use]
    pub fn new(local_peer_id: PeerId, config: BroadcastConfig) -> Self {
        let rate_limiter = RateLimiter::new(config.rate_limit.clone());
        Self {
            local_peer_id,
            config,
            known_peers: HashSet::new(),
            seen_messages: HashMap::new(),
            seen_order: VecDeque::new(),
            announcement_cache: HashMap::new(),
            announcement_order: VecDeque::new(),
            next_announcement_seq: 0,
            last_cleanup: Instant::now(),
            rate_limiter,
        }
    }

    /// Creates a broadcaster with default config.
    #[must_use]
    pub fn with_defaults(local_peer_id: PeerId) -> Self {
        Self::new(local_peer_id, BroadcastConfig::default())
    }

    /// Returns the local peer ID.
    #[must_use]
    pub const fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    /// Returns the current config.
    #[must_use]
    pub const fn config(&self) -> &BroadcastConfig {
        &self.config
    }

    /// Adds a peer to the known peers set.
    pub fn add_peer(&mut self, peer_id: PeerId) {
        if peer_id != self.local_peer_id {
            self.known_peers.insert(peer_id);
        }
    }

    /// Removes a peer from the known peers set.
    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        self.known_peers.remove(peer_id);
        self.announcement_cache.remove(peer_id);
        // Clean up announcement order for this peer
        self.announcement_order.retain(|k| k.peer_id != *peer_id);
    }

    /// Returns the number of known peers.
    #[must_use]
    pub fn peer_count(&self) -> usize {
        self.known_peers.len()
    }

    /// Returns all known peer IDs.
    #[must_use]
    pub fn known_peers(&self) -> Vec<PeerId> {
        self.known_peers.iter().copied().collect()
    }

    /// Checks if a message has already been seen.
    #[must_use]
    pub fn has_seen(&self, message_id: &MessageId) -> bool {
        self.seen_messages.contains_key(message_id)
    }

    /// Returns the number of cached announcements.
    #[must_use]
    pub fn cached_announcement_count(&self) -> usize {
        self.announcement_cache.values().map(Vec::len).sum()
    }

    /// Prepares a new announcement for broadcast.
    ///
    /// Returns the message and selected target peers for fanout.
    pub fn prepare_announce(
        &mut self,
        announcement: CapacityAnnouncement,
    ) -> BroadcastResult {
        let message_id = MessageId::new();
        let target_peers = self.select_fanout_peers(None);

        // Mark as seen
        self.mark_seen(message_id, self.local_peer_id);

        // Cache the announcement
        self.cache_announcement(announcement);

        BroadcastResult {
            message_id,
            target_peers,
            was_duplicate: false,
        }
    }

    /// Handles a received gossip message.
    ///
    /// Returns the broadcast result indicating whether to forward and to whom.
    ///
    /// # Errors
    ///
    /// Returns an error if the message is malformed or the peer is rate-limited.
    pub fn handle_message(
        &mut self,
        message: &GossipMessage,
        from_peer: PeerId,
    ) -> Result<BroadcastResult, P2pError> {
        // Check rate limit for this peer
        match self.rate_limiter.check_and_record(from_peer) {
            RateLimitResult::Allowed => {}
            RateLimitResult::RateLimited => {
                return Err(P2pError::RateLimited { peer_id: from_peer });
            }
            RateLimitResult::Banned { .. } => {
                return Err(P2pError::PeerBanned { peer_id: from_peer });
            }
        }

        // Run periodic cleanup
        self.maybe_cleanup();

        match message {
            GossipMessage::Announce {
                message_id,
                announcement,
                ttl_hops,
            } => self.handle_announce(*message_id, announcement.clone(), *ttl_hops, from_peer),
            GossipMessage::Query(query) => self.handle_query(query, from_peer),
            GossipMessage::Response { query_id, .. } => {
                // Responses are not re-broadcast, just mark as seen
                self.mark_seen(*query_id, from_peer);
                Ok(BroadcastResult {
                    message_id: *query_id,
                    target_peers: vec![],
                    was_duplicate: false,
                })
            }
            GossipMessage::Heartbeat { .. }
            | GossipMessage::SyncRequest { .. }
            | GossipMessage::SyncResponse { .. } => {
                // These are not broadcast
                Ok(BroadcastResult {
                    message_id: MessageId::new(),
                    target_peers: vec![],
                    was_duplicate: false,
                })
            }
        }
    }

    /// Handles an announce message.
    fn handle_announce(
        &mut self,
        message_id: MessageId,
        announcement: CapacityAnnouncement,
        ttl_hops: u8,
        from_peer: PeerId,
    ) -> Result<BroadcastResult, P2pError> {
        // Check for duplicate
        if self.has_seen(&message_id) {
            return Ok(BroadcastResult {
                message_id,
                target_peers: vec![],
                was_duplicate: true,
            });
        }

        // Verify the announcement signature to prevent spoofing
        let verifying_key = announcement
            .peer_id()
            .to_verifying_key()
            .ok_or_else(|| P2pError::Protocol("Invalid peer ID: cannot derive verifying key".to_string()))?;

        announcement
            .verify(&verifying_key)
            .map_err(|e| P2pError::Gossip(format!("Announcement signature verification failed: {e}")))?;

        // Check TTL
        if ttl_hops == 0 {
            // TTL expired, don't forward but still cache
            self.mark_seen(message_id, from_peer);
            self.cache_announcement(announcement);
            return Ok(BroadcastResult {
                message_id,
                target_peers: vec![],
                was_duplicate: false,
            });
        }

        // Verify announcement isn't expired
        if announcement.is_expired() {
            return Err(P2pError::Gossip("Received expired announcement".to_string()));
        }

        // Mark as seen
        self.mark_seen(message_id, from_peer);

        // Cache the announcement
        self.cache_announcement(announcement);

        // Select peers for forwarding (exclude sender)
        let target_peers = self.select_fanout_peers(Some(from_peer));

        Ok(BroadcastResult {
            message_id,
            target_peers,
            was_duplicate: false,
        })
    }

    /// Handles a query message.
    fn handle_query(
        &mut self,
        query: &GossipQuery,
        from_peer: PeerId,
    ) -> Result<BroadcastResult, P2pError> {
        // Check for duplicate
        if self.has_seen(&query.query_id) {
            return Ok(BroadcastResult {
                message_id: query.query_id,
                target_peers: vec![],
                was_duplicate: true,
            });
        }

        // Mark as seen
        self.mark_seen(query.query_id, from_peer);

        // Check TTL
        if query.ttl_hops == 0 {
            return Ok(BroadcastResult {
                message_id: query.query_id,
                target_peers: vec![],
                was_duplicate: false,
            });
        }

        // Select peers for forwarding (exclude sender and original querier)
        let target_peers = self.select_fanout_peers_excluding(&[from_peer, query.from_peer]);

        Ok(BroadcastResult {
            message_id: query.query_id,
            target_peers,
            was_duplicate: false,
        })
    }

    /// Queries cached announcements matching a filter.
    #[must_use]
    pub fn query_cache(&self, filter: &QueryFilter, max_results: u32) -> Vec<CapacityAnnouncement> {
        let mut results = Vec::new();
        let max = max_results as usize;

        for cached_list in self.announcement_cache.values() {
            for cached in cached_list {
                if !cached.announcement.is_expired()
                    && cached.announcement.matches_filter(filter)
                {
                    results.push(cached.announcement.clone());
                    if results.len() >= max {
                        return results;
                    }
                }
            }
        }

        results
    }

    /// Selects peers for fanout, optionally excluding one peer.
    fn select_fanout_peers(&self, exclude: Option<PeerId>) -> Vec<PeerId> {
        let mut candidates: Vec<_> = self
            .known_peers
            .iter()
            .filter(|&p| exclude.map_or(true, |e| *p != e))
            .copied()
            .collect();

        // Deterministic selection for testability when not enough peers
        if candidates.len() <= self.config.fanout {
            return candidates;
        }

        // Random selection for fanout
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        candidates.shuffle(&mut rng);
        candidates.truncate(self.config.fanout);
        candidates
    }

    /// Selects peers for fanout, excluding multiple peers.
    fn select_fanout_peers_excluding(&self, exclude: &[PeerId]) -> Vec<PeerId> {
        let exclude_set: HashSet<_> = exclude.iter().collect();
        let mut candidates: Vec<_> = self
            .known_peers
            .iter()
            .filter(|p| !exclude_set.contains(p))
            .copied()
            .collect();

        if candidates.len() <= self.config.fanout {
            return candidates;
        }

        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        candidates.shuffle(&mut rng);
        candidates.truncate(self.config.fanout);
        candidates
    }

    /// Marks a message as seen.
    fn mark_seen(&mut self, message_id: MessageId, from_peer: PeerId) {
        // Evict oldest if at capacity
        if self.seen_messages.len() >= self.config.max_seen_cache {
            if let Some(oldest_id) = self.seen_order.pop_front() {
                self.seen_messages.remove(&oldest_id);
            }
        }

        self.seen_messages.insert(
            message_id,
            SeenEntry {
                added_at: Instant::now(),
                from_peer,
            },
        );
        self.seen_order.push_back(message_id);
    }

    /// Caches an announcement.
    fn cache_announcement(&mut self, announcement: CapacityAnnouncement) {
        let peer_id = announcement.peer_id();

        // Generate unique key for this announcement
        let key = AnnouncementKey {
            peer_id,
            seq: self.next_announcement_seq,
        };
        self.next_announcement_seq = self.next_announcement_seq.wrapping_add(1);

        let cached = CachedAnnouncement {
            announcement,
            cached_at: Instant::now(),
            key,
        };

        // Remove old announcement from same peer if exists (same created_at timestamp)
        {
            let entries = self.announcement_cache.entry(peer_id).or_default();
            let removed_keys: Vec<_> = entries
                .iter()
                .filter(|c| c.announcement.created_at() == cached.announcement.created_at())
                .map(|c| c.key)
                .collect();
            entries.retain(|c| c.announcement.created_at() != cached.announcement.created_at());
            for removed_key in &removed_keys {
                self.announcement_order.retain(|k| k != removed_key);
            }
        }

        // Evict oldest announcements if we're at the total limit
        while self.cached_announcement_count() >= self.config.max_total_announcements {
            if let Some(oldest_key) = self.announcement_order.pop_front() {
                if let Some(peer_entries) = self.announcement_cache.get_mut(&oldest_key.peer_id) {
                    peer_entries.retain(|c| c.key != oldest_key);
                    if peer_entries.is_empty() {
                        self.announcement_cache.remove(&oldest_key.peer_id);
                    }
                }
            } else {
                // No entries in order queue but cache is full - shouldn't happen, but break to avoid infinite loop
                break;
            }
        }

        // Add new announcement
        let entries = self.announcement_cache.entry(peer_id).or_default();
        entries.push(cached);
        self.announcement_order.push_back(key);

        // Trim to max per peer (remove oldest for this peer)
        while entries.len() > self.config.max_announcements_per_peer {
            if let Some(removed) = entries.first().map(|e| e.key) {
                entries.remove(0);
                self.announcement_order.retain(|k| *k != removed);
            } else {
                break;
            }
        }
    }

    /// Runs cleanup if the interval has elapsed.
    fn maybe_cleanup(&mut self) {
        if self.last_cleanup.elapsed() < self.config.cleanup_interval {
            return;
        }
        self.cleanup();
    }

    /// Cleans up expired entries.
    pub fn cleanup(&mut self) {
        let now = Instant::now();
        self.last_cleanup = now;

        // Clean seen messages
        let seen_ttl = self.config.seen_cache_ttl;
        self.seen_messages
            .retain(|_, entry| now.duration_since(entry.added_at) < seen_ttl);
        self.seen_order
            .retain(|id| self.seen_messages.contains_key(id));

        // Clean announcement cache - collect keys to remove
        let cache_ttl = self.config.announcement_cache_ttl;
        let mut removed_keys = Vec::new();
        for entries in self.announcement_cache.values_mut() {
            for entry in entries.iter() {
                if now.duration_since(entry.cached_at) >= cache_ttl || entry.announcement.is_expired() {
                    removed_keys.push(entry.key);
                }
            }
            entries.retain(|c| {
                now.duration_since(c.cached_at) < cache_ttl && !c.announcement.is_expired()
            });
        }
        self.announcement_cache.retain(|_, v| !v.is_empty());

        // Clean announcement order
        let removed_set: HashSet<_> = removed_keys.into_iter().collect();
        self.announcement_order.retain(|k| !removed_set.contains(k));
    }

    /// Returns statistics about the broadcaster state.
    #[must_use]
    pub fn stats(&self) -> BroadcasterStats {
        BroadcasterStats {
            known_peers: self.known_peers.len(),
            seen_messages: self.seen_messages.len(),
            cached_announcements: self.cached_announcement_count(),
            unique_providers: self.announcement_cache.len(),
        }
    }
}

/// Statistics about broadcaster state.
#[derive(Debug, Clone, Default)]
pub struct BroadcasterStats {
    /// Number of known peers.
    pub known_peers: usize,
    /// Number of seen message IDs cached.
    pub seen_messages: usize,
    /// Total cached announcements.
    pub cached_announcements: usize,
    /// Number of unique providers in cache.
    pub unique_providers: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gossip::{GpuInfo, Pricing};
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn make_peer_id() -> PeerId {
        let signing_key = SigningKey::generate(&mut OsRng);
        PeerId::from_public_key(&signing_key.verifying_key())
    }

    fn make_announcement(peer_id: PeerId) -> CapacityAnnouncement {
        CapacityAnnouncement::new(
            peer_id,
            vec![GpuInfo {
                model: "RTX 4090".to_string(),
                vram_gb: 24,
                count: 2,
            }],
            Pricing {
                gpu_hour_cents: 100,
                cpu_hour_cents: 10,
            },
            vec!["inference".to_string()],
            Duration::from_secs(300),
        )
    }

    fn make_signed_announcement(signing_key: &SigningKey) -> CapacityAnnouncement {
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());
        let mut announcement = make_announcement(peer_id);
        announcement.sign(signing_key);
        announcement
    }

    // ========== BroadcastConfig Tests ==========

    #[test]
    fn broadcast_config_default() {
        let config = BroadcastConfig::default();
        assert_eq!(config.fanout, 3);
        assert_eq!(config.max_ttl_hops, 6);
    }

    #[test]
    fn broadcast_config_small_network() {
        let config = BroadcastConfig::small_network();
        assert_eq!(config.fanout, 2);
        assert_eq!(config.max_ttl_hops, 4);
    }

    #[test]
    fn broadcast_config_large_network() {
        let config = BroadcastConfig::large_network();
        assert_eq!(config.fanout, 5);
        assert_eq!(config.max_ttl_hops, 8);
    }

    #[test]
    fn broadcast_config_builder() {
        let config = BroadcastConfig::default()
            .with_fanout(4)
            .with_max_ttl(10);
        assert_eq!(config.fanout, 4);
        assert_eq!(config.max_ttl_hops, 10);
    }

    // ========== GossipBroadcaster Creation Tests ==========

    #[test]
    fn broadcaster_creation() {
        let peer_id = make_peer_id();
        let broadcaster = GossipBroadcaster::with_defaults(peer_id);

        assert_eq!(broadcaster.local_peer_id(), peer_id);
        assert_eq!(broadcaster.peer_count(), 0);
        assert_eq!(broadcaster.cached_announcement_count(), 0);
    }

    #[test]
    fn broadcaster_with_custom_config() {
        let peer_id = make_peer_id();
        let config = BroadcastConfig::default().with_fanout(5);
        let broadcaster = GossipBroadcaster::new(peer_id, config);

        assert_eq!(broadcaster.config().fanout, 5);
    }

    // ========== Peer Management Tests ==========

    #[test]
    fn broadcaster_add_peer() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let peer1 = make_peer_id();
        let peer2 = make_peer_id();

        broadcaster.add_peer(peer1);
        assert_eq!(broadcaster.peer_count(), 1);

        broadcaster.add_peer(peer2);
        assert_eq!(broadcaster.peer_count(), 2);

        // Adding same peer again doesn't duplicate
        broadcaster.add_peer(peer1);
        assert_eq!(broadcaster.peer_count(), 2);
    }

    #[test]
    fn broadcaster_cannot_add_self() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        broadcaster.add_peer(local);
        assert_eq!(broadcaster.peer_count(), 0);
    }

    #[test]
    fn broadcaster_remove_peer() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let peer = make_peer_id();
        broadcaster.add_peer(peer);
        assert_eq!(broadcaster.peer_count(), 1);

        broadcaster.remove_peer(&peer);
        assert_eq!(broadcaster.peer_count(), 0);
    }

    #[test]
    fn broadcaster_known_peers() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let peer1 = make_peer_id();
        let peer2 = make_peer_id();

        broadcaster.add_peer(peer1);
        broadcaster.add_peer(peer2);

        let known = broadcaster.known_peers();
        assert_eq!(known.len(), 2);
        assert!(known.contains(&peer1));
        assert!(known.contains(&peer2));
    }

    // ========== Prepare Announce Tests ==========

    #[test]
    fn prepare_announce_returns_result() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        // Add some peers
        for _ in 0..5 {
            broadcaster.add_peer(make_peer_id());
        }

        let announcement = make_announcement(local);
        let result = broadcaster.prepare_announce(announcement);

        assert!(!result.was_duplicate);
        assert!(!result.target_peers.is_empty());
        // Should select up to fanout peers
        assert!(result.target_peers.len() <= broadcaster.config().fanout);
    }

    #[test]
    fn prepare_announce_marks_as_seen() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let announcement = make_announcement(local);
        let result = broadcaster.prepare_announce(announcement);

        assert!(broadcaster.has_seen(&result.message_id));
    }

    #[test]
    fn prepare_announce_caches_announcement() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let announcement = make_announcement(local);
        broadcaster.prepare_announce(announcement);

        assert_eq!(broadcaster.cached_announcement_count(), 1);
    }

    // ========== Handle Message Tests ==========

    #[test]
    fn handle_announce_new_message() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        // Add peers
        let peer1 = make_peer_id();
        let peer2 = make_peer_id();
        broadcaster.add_peer(peer1);
        broadcaster.add_peer(peer2);

        let signing_key = SigningKey::generate(&mut OsRng);
        let announcement = make_signed_announcement(&signing_key);
        let message = GossipMessage::announce(announcement, 3);

        let result = broadcaster
            .handle_message(&message, peer1)
            .expect("should handle");

        assert!(!result.was_duplicate);
        // Should forward to peer2 (peer1 is excluded as sender)
        assert!(result.target_peers.contains(&peer2));
        assert!(!result.target_peers.contains(&peer1));
    }

    #[test]
    fn handle_announce_duplicate() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let peer1 = make_peer_id();
        broadcaster.add_peer(peer1);

        let signing_key = SigningKey::generate(&mut OsRng);
        let announcement = make_signed_announcement(&signing_key);
        let message = GossipMessage::announce(announcement, 3);

        // First time
        let result1 = broadcaster
            .handle_message(&message, peer1)
            .expect("should handle");
        assert!(!result1.was_duplicate);

        // Second time (same message)
        let result2 = broadcaster
            .handle_message(&message, peer1)
            .expect("should handle");
        assert!(result2.was_duplicate);
        assert!(result2.target_peers.is_empty());
    }

    #[test]
    fn handle_announce_ttl_zero() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let peer1 = make_peer_id();
        broadcaster.add_peer(peer1);

        let signing_key = SigningKey::generate(&mut OsRng);
        let announcement = make_signed_announcement(&signing_key);
        let message = GossipMessage::Announce {
            message_id: MessageId::new(),
            announcement,
            ttl_hops: 0,
        };

        let result = broadcaster
            .handle_message(&message, peer1)
            .expect("should handle");

        // Should not forward (TTL expired)
        assert!(!result.was_duplicate);
        assert!(result.target_peers.is_empty());

        // But should still cache
        assert_eq!(broadcaster.cached_announcement_count(), 1);
    }

    #[test]
    fn handle_announce_expired_announcement() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let peer1 = make_peer_id();
        broadcaster.add_peer(peer1);

        let signing_key = SigningKey::generate(&mut OsRng);
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());

        // Create an already-expired announcement
        let mut announcement = CapacityAnnouncement::new(
            peer_id,
            vec![],
            Pricing {
                gpu_hour_cents: 100,
                cpu_hour_cents: 10,
            },
            vec![],
            Duration::from_millis(1), // Very short TTL
        );
        announcement.sign(&signing_key);

        std::thread::sleep(Duration::from_millis(10));

        let message = GossipMessage::announce(announcement, 3);
        let result = broadcaster.handle_message(&message, peer1);

        assert!(result.is_err());
    }

    #[test]
    fn handle_announce_rejects_unsigned_announcement() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let peer1 = make_peer_id();
        broadcaster.add_peer(peer1);

        let signing_key = SigningKey::generate(&mut OsRng);
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());

        // Create an unsigned announcement (no call to sign())
        let announcement = CapacityAnnouncement::new(
            peer_id,
            vec![],
            Pricing {
                gpu_hour_cents: 100,
                cpu_hour_cents: 10,
            },
            vec![],
            Duration::from_secs(300),
        );

        let message = GossipMessage::announce(announcement, 3);
        let result = broadcaster.handle_message(&message, peer1);

        // Should be rejected due to missing signature
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("signature"));
    }

    #[test]
    fn handle_announce_rejects_spoofed_announcement() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let peer1 = make_peer_id();
        broadcaster.add_peer(peer1);

        // Attacker's key
        let attacker_key = SigningKey::generate(&mut OsRng);

        // Victim's key (the peer being spoofed)
        let victim_key = SigningKey::generate(&mut OsRng);
        let victim_peer_id = PeerId::from_public_key(&victim_key.verifying_key());

        // Attacker creates announcement claiming to be victim but signs with attacker's key
        let mut spoofed_announcement = CapacityAnnouncement::new(
            victim_peer_id, // Claims to be victim
            vec![GpuInfo {
                model: "Fake GPU".to_string(),
                vram_gb: 999,
                count: 100,
            }],
            Pricing {
                gpu_hour_cents: 1, // Suspiciously cheap
                cpu_hour_cents: 1,
            },
            vec!["malicious".to_string()],
            Duration::from_secs(300),
        );

        // Attacker signs with their own key
        spoofed_announcement.sign(&attacker_key);

        let message = GossipMessage::announce(spoofed_announcement, 3);
        let result = broadcaster.handle_message(&message, peer1);

        // Should be rejected because signature doesn't match peer_id's key
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("signature"));
    }

    #[test]
    fn handle_announce_rejects_tampered_announcement() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let peer1 = make_peer_id();
        broadcaster.add_peer(peer1);

        let signing_key = SigningKey::generate(&mut OsRng);
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());

        // Create and sign a legitimate announcement
        let mut announcement = CapacityAnnouncement::new(
            peer_id,
            vec![GpuInfo {
                model: "RTX 4090".to_string(),
                vram_gb: 24,
                count: 2,
            }],
            Pricing {
                gpu_hour_cents: 100,
                cpu_hour_cents: 10,
            },
            vec!["inference".to_string()],
            Duration::from_secs(300),
        );
        announcement.sign(&signing_key);

        // Tamper with the announcement by serializing, modifying, deserializing
        let json = serde_json::to_value(&announcement).ok();
        let tampered: Option<CapacityAnnouncement> = json.and_then(|mut val| {
            if let Some(pricing) = val.get_mut("pricing") {
                pricing["gpu_hour_cents"] = serde_json::json!(1); // Tamper: make it super cheap
            }
            serde_json::from_value(val).ok()
        });

        if let Some(tampered_announcement) = tampered {
            let message = GossipMessage::announce(tampered_announcement, 3);
            let result = broadcaster.handle_message(&message, peer1);

            // Should be rejected due to signature mismatch
            assert!(result.is_err());
            let err = result.unwrap_err();
            assert!(err.to_string().contains("signature"));
        }
    }

    #[test]
    fn handle_query_new() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let peer1 = make_peer_id();
        let peer2 = make_peer_id();
        broadcaster.add_peer(peer1);
        broadcaster.add_peer(peer2);

        let querier = make_peer_id();
        let message = GossipMessage::query(querier, QueryFilter::any(), 10, 3);

        let result = broadcaster
            .handle_message(&message, peer1)
            .expect("should handle");

        assert!(!result.was_duplicate);
        // Should forward but exclude sender (peer1) and original querier
        assert!(!result.target_peers.contains(&peer1));
        assert!(!result.target_peers.contains(&querier));
    }

    #[test]
    fn handle_query_duplicate() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let peer1 = make_peer_id();
        broadcaster.add_peer(peer1);

        let querier = make_peer_id();
        let message = GossipMessage::query(querier, QueryFilter::any(), 10, 3);

        broadcaster
            .handle_message(&message, peer1)
            .expect("should handle");
        let result2 = broadcaster
            .handle_message(&message, peer1)
            .expect("should handle");

        assert!(result2.was_duplicate);
    }

    #[test]
    fn handle_response_marks_seen() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let peer1 = make_peer_id();
        broadcaster.add_peer(peer1);

        let query_id = MessageId::new();
        let message = GossipMessage::response(query_id, peer1, vec![]);

        let result = broadcaster
            .handle_message(&message, peer1)
            .expect("should handle");

        // Responses are not broadcast
        assert!(result.target_peers.is_empty());
        assert!(broadcaster.has_seen(&query_id));
    }

    #[test]
    fn handle_heartbeat_no_broadcast() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let peer1 = make_peer_id();
        let message = GossipMessage::heartbeat(peer1);

        let result = broadcaster
            .handle_message(&message, peer1)
            .expect("should handle");

        assert!(result.target_peers.is_empty());
    }

    // ========== Query Cache Tests ==========

    #[test]
    fn query_cache_returns_matching() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let peer1 = make_peer_id();
        broadcaster.add_peer(peer1);

        // Add some announcements
        let signing_key1 = SigningKey::generate(&mut OsRng);
        let announcement1 = make_signed_announcement(&signing_key1);
        let msg1 = GossipMessage::announce(announcement1.clone(), 3);
        broadcaster.handle_message(&msg1, peer1).ok();

        let signing_key2 = SigningKey::generate(&mut OsRng);
        let peer_id2 = PeerId::from_public_key(&signing_key2.verifying_key());
        let mut announcement2 = CapacityAnnouncement::new(
            peer_id2,
            vec![GpuInfo {
                model: "A100".to_string(),
                vram_gb: 80,
                count: 8,
            }],
            Pricing {
                gpu_hour_cents: 500,
                cpu_hour_cents: 50,
            },
            vec!["training".to_string()],
            Duration::from_secs(300),
        );
        announcement2.sign(&signing_key2);
        let msg2 = GossipMessage::announce(announcement2, 3);
        broadcaster.handle_message(&msg2, peer1).ok();

        // Query for A100
        let filter = QueryFilter::any().with_gpu_model("A100");
        let results = broadcaster.query_cache(&filter, 10);

        assert_eq!(results.len(), 1);
        assert!(results[0].gpus().iter().any(|g| g.model == "A100"));
    }

    #[test]
    fn query_cache_respects_max_results() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let peer1 = make_peer_id();
        broadcaster.add_peer(peer1);

        // Add multiple announcements
        for _ in 0..5 {
            let signing_key = SigningKey::generate(&mut OsRng);
            let announcement = make_signed_announcement(&signing_key);
            let msg = GossipMessage::announce(announcement, 3);
            broadcaster.handle_message(&msg, peer1).ok();
        }

        let filter = QueryFilter::any();
        let results = broadcaster.query_cache(&filter, 2);

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn query_cache_excludes_expired() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let signing_key = SigningKey::generate(&mut OsRng);
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());

        // Create announcement with very short TTL
        let mut announcement = CapacityAnnouncement::new(
            peer_id,
            vec![],
            Pricing {
                gpu_hour_cents: 100,
                cpu_hour_cents: 10,
            },
            vec![],
            Duration::from_millis(1),
        );
        announcement.sign(&signing_key);

        broadcaster.cache_announcement(announcement);

        std::thread::sleep(Duration::from_millis(10));

        let results = broadcaster.query_cache(&QueryFilter::any(), 10);
        assert!(results.is_empty());
    }

    // ========== Fanout Selection Tests ==========

    #[test]
    fn fanout_selects_up_to_config_limit() {
        let local = make_peer_id();
        let config = BroadcastConfig::default().with_fanout(2);
        let mut broadcaster = GossipBroadcaster::new(local, config);

        // Add 5 peers
        for _ in 0..5 {
            broadcaster.add_peer(make_peer_id());
        }

        let announcement = make_announcement(local);
        let result = broadcaster.prepare_announce(announcement);

        assert_eq!(result.target_peers.len(), 2);
    }

    #[test]
    fn fanout_returns_all_when_fewer_than_limit() {
        let local = make_peer_id();
        let config = BroadcastConfig::default().with_fanout(5);
        let mut broadcaster = GossipBroadcaster::new(local, config);

        // Add only 2 peers
        let peer1 = make_peer_id();
        let peer2 = make_peer_id();
        broadcaster.add_peer(peer1);
        broadcaster.add_peer(peer2);

        let announcement = make_announcement(local);
        let result = broadcaster.prepare_announce(announcement);

        assert_eq!(result.target_peers.len(), 2);
    }

    // ========== Cleanup Tests ==========

    #[test]
    fn cleanup_removes_expired_seen_entries() {
        let local = make_peer_id();
        let config = BroadcastConfig {
            seen_cache_ttl: Duration::from_millis(10),
            cleanup_interval: Duration::from_millis(1),
            ..BroadcastConfig::default()
        };
        let mut broadcaster = GossipBroadcaster::new(local, config);

        let message_id = MessageId::new();
        broadcaster.mark_seen(message_id, local);

        assert!(broadcaster.has_seen(&message_id));

        std::thread::sleep(Duration::from_millis(20));
        broadcaster.cleanup();

        assert!(!broadcaster.has_seen(&message_id));
    }

    #[test]
    fn seen_cache_evicts_oldest_when_full() {
        let local = make_peer_id();
        let config = BroadcastConfig {
            max_seen_cache: 3,
            ..BroadcastConfig::default()
        };
        let mut broadcaster = GossipBroadcaster::new(local, config);

        let id1 = MessageId::new();
        let id2 = MessageId::new();
        let id3 = MessageId::new();
        let id4 = MessageId::new();

        broadcaster.mark_seen(id1, local);
        broadcaster.mark_seen(id2, local);
        broadcaster.mark_seen(id3, local);

        assert!(broadcaster.has_seen(&id1));

        // Adding fourth should evict first
        broadcaster.mark_seen(id4, local);

        assert!(!broadcaster.has_seen(&id1));
        assert!(broadcaster.has_seen(&id2));
        assert!(broadcaster.has_seen(&id3));
        assert!(broadcaster.has_seen(&id4));
    }

    #[test]
    fn announcement_cache_evicts_oldest_when_total_limit_reached() {
        let local = make_peer_id();
        let config = BroadcastConfig {
            max_total_announcements: 3,
            max_announcements_per_peer: 10, // High per-peer to test total limit
            ..BroadcastConfig::default()
        };
        let mut broadcaster = GossipBroadcaster::new(local, config);

        // Create announcements from different peers
        let mut signing_keys = Vec::new();
        for _ in 0..4 {
            signing_keys.push(SigningKey::generate(&mut OsRng));
        }

        // Add first 3 announcements
        for key in &signing_keys[..3] {
            let announcement = make_signed_announcement(key);
            broadcaster.cache_announcement(announcement);
        }

        assert_eq!(broadcaster.cached_announcement_count(), 3);
        let first_peer_id = PeerId::from_public_key(&signing_keys[0].verifying_key());
        assert!(broadcaster.announcement_cache.contains_key(&first_peer_id));

        // Add 4th announcement - should evict the oldest (first)
        let announcement4 = make_signed_announcement(&signing_keys[3]);
        broadcaster.cache_announcement(announcement4);

        // Should still be at limit
        assert_eq!(broadcaster.cached_announcement_count(), 3);
        // First announcement should be evicted
        assert!(!broadcaster.announcement_cache.contains_key(&first_peer_id));
    }

    #[test]
    fn announcement_cache_eviction_across_many_peers() {
        let local = make_peer_id();
        let config = BroadcastConfig {
            max_total_announcements: 5,
            max_announcements_per_peer: 2,
            ..BroadcastConfig::default()
        };
        let mut broadcaster = GossipBroadcaster::new(local, config);

        // Add announcements from 10 different peers (only 5 should remain due to total limit)
        for _ in 0..10 {
            let signing_key = SigningKey::generate(&mut OsRng);
            let announcement = make_signed_announcement(&signing_key);
            broadcaster.cache_announcement(announcement);
        }

        // Should be capped at total limit
        assert_eq!(broadcaster.cached_announcement_count(), 5);
        // Should have exactly 5 unique providers
        assert_eq!(broadcaster.announcement_cache.len(), 5);
    }

    #[test]
    fn remove_peer_cleans_announcement_order() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let signing_key = SigningKey::generate(&mut OsRng);
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());
        let announcement = make_signed_announcement(&signing_key);

        broadcaster.add_peer(peer_id);
        broadcaster.cache_announcement(announcement);

        assert_eq!(broadcaster.cached_announcement_count(), 1);
        assert!(!broadcaster.announcement_order.is_empty());

        broadcaster.remove_peer(&peer_id);

        assert_eq!(broadcaster.cached_announcement_count(), 0);
        assert!(broadcaster.announcement_order.is_empty());
    }

    // ========== Stats Tests ==========

    #[test]
    fn stats_reflect_state() {
        let local = make_peer_id();
        let mut broadcaster = GossipBroadcaster::with_defaults(local);

        let peer1 = make_peer_id();
        broadcaster.add_peer(peer1);

        let signing_key = SigningKey::generate(&mut OsRng);
        let announcement = make_signed_announcement(&signing_key);
        let msg = GossipMessage::announce(announcement, 3);
        broadcaster.handle_message(&msg, peer1).ok();

        let stats = broadcaster.stats();

        assert_eq!(stats.known_peers, 1);
        assert_eq!(stats.seen_messages, 1);
        assert_eq!(stats.cached_announcements, 1);
        assert_eq!(stats.unique_providers, 1);
    }

    // ========== Proptest ==========

    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn fanout_never_exceeds_config(fanout in 1usize..10, num_peers in 0usize..20) {
                let local = make_peer_id();
                let config = BroadcastConfig::default().with_fanout(fanout);
                let mut broadcaster = GossipBroadcaster::new(local, config);

                for _ in 0..num_peers {
                    broadcaster.add_peer(make_peer_id());
                }

                let announcement = make_announcement(local);
                let result = broadcaster.prepare_announce(announcement);

                prop_assert!(result.target_peers.len() <= fanout);
                prop_assert!(result.target_peers.len() <= num_peers);
            }

            #[test]
            fn duplicate_detection_is_consistent(num_msgs in 1usize..10) {
                let local = make_peer_id();
                let mut broadcaster = GossipBroadcaster::with_defaults(local);

                let peer = make_peer_id();
                broadcaster.add_peer(peer);

                for _ in 0..num_msgs {
                    let signing_key = SigningKey::generate(&mut OsRng);
                    let announcement = make_signed_announcement(&signing_key);
                    let msg = GossipMessage::announce(announcement, 3);

                    let result1 = broadcaster.handle_message(&msg, peer).ok();
                    let result2 = broadcaster.handle_message(&msg, peer).ok();

                    prop_assert!(result1.is_some());
                    prop_assert!(result2.is_some());
                    prop_assert!(!result1.unwrap().was_duplicate);
                    prop_assert!(result2.unwrap().was_duplicate);
                }
            }
        }
    }
}
