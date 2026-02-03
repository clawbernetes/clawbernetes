//! Network orchestration for the MOLT P2P network.
//!
//! This module provides the main network interface:
//! - [`MoltNetwork`]: Main network orchestration
//! - [`NetworkConfig`]: Network configuration
//! - [`NetworkState`]: Current network state

use crate::connection::{ConnectionPoolConfig, SharedConnectionPool};
use crate::discovery::{BootstrapNode, PeerTable};
use crate::error::P2pError;
use crate::gossip::CapacityAnnouncement;
use crate::message::CapacityRequirements;
use crate::protocol::{PeerId, PeerInfo};
use chrono::{DateTime, Utc};
use ed25519_dalek::SigningKey;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// State of the network node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkState {
    /// Not connected to any peers.
    Offline,
    /// Attempting to join the network.
    Joining,
    /// Connected and operational.
    Online,
    /// Gracefully leaving the network.
    Leaving,
}

impl NetworkState {
    /// Returns true if the network is operational.
    #[must_use]
    pub const fn is_operational(&self) -> bool {
        matches!(self, Self::Online)
    }

    /// Returns true if the network can accept new operations.
    #[must_use]
    pub const fn can_operate(&self) -> bool {
        matches!(self, Self::Online | Self::Joining)
    }
}

/// Configuration for the MOLT network.
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Local addresses to bind to.
    pub bind_addresses: Vec<SocketAddr>,
    /// Bootstrap nodes to connect to initially.
    pub bootstrap_nodes: Vec<String>,
    /// Maximum number of peer connections.
    pub max_peers: usize,
    /// Interval for broadcasting capacity announcements.
    pub announce_interval: Duration,
    /// Timeout for finding providers.
    pub provider_timeout: Duration,
    /// Connection pool configuration.
    pub connection_config: ConnectionPoolConfig,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            bind_addresses: vec![],
            bootstrap_nodes: vec![],
            max_peers: 50,
            announce_interval: Duration::from_secs(60),
            provider_timeout: Duration::from_secs(10),
            connection_config: ConnectionPoolConfig::default(),
        }
    }
}

impl NetworkConfig {
    /// Creates a new network configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the bind addresses.
    #[must_use]
    pub fn with_bind_addresses(mut self, addresses: Vec<SocketAddr>) -> Self {
        self.bind_addresses = addresses;
        self
    }

    /// Sets the bootstrap nodes.
    #[must_use]
    pub fn with_bootstrap_nodes(mut self, nodes: Vec<String>) -> Self {
        self.bootstrap_nodes = nodes;
        self
    }

    /// Sets the maximum number of peers.
    #[must_use]
    pub const fn with_max_peers(mut self, max: usize) -> Self {
        self.max_peers = max;
        self
    }

    /// Sets the announcement interval.
    #[must_use]
    pub const fn with_announce_interval(mut self, interval: Duration) -> Self {
        self.announce_interval = interval;
        self
    }

    /// Sets the provider timeout.
    #[must_use]
    pub const fn with_provider_timeout(mut self, timeout: Duration) -> Self {
        self.provider_timeout = timeout;
        self
    }
}

/// Statistics about the network.
#[derive(Debug, Clone, Default)]
pub struct NetworkStats {
    /// Number of messages sent.
    pub messages_sent: u64,
    /// Number of messages received.
    pub messages_received: u64,
    /// Number of successful joins.
    pub successful_joins: u64,
    /// Number of failed joins.
    pub failed_joins: u64,
    /// Number of capacity announcements broadcast.
    pub announcements_broadcast: u64,
    /// Number of provider searches.
    pub provider_searches: u64,
    /// Time when the network came online.
    pub online_since: Option<DateTime<Utc>>,
}

impl NetworkStats {
    /// Creates new empty statistics.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            messages_sent: 0,
            messages_received: 0,
            successful_joins: 0,
            failed_joins: 0,
            announcements_broadcast: 0,
            provider_searches: 0,
            online_since: None,
        }
    }

    /// Returns how long the network has been online.
    #[must_use]
    pub fn uptime(&self) -> Option<Duration> {
        self.online_since.map(|t| {
            Utc::now()
                .signed_duration_since(t)
                .to_std()
                .unwrap_or(Duration::ZERO)
        })
    }

    /// Records that the network came online.
    pub fn mark_online(&mut self) {
        self.online_since = Some(Utc::now());
    }

    /// Records that the network went offline.
    pub const fn mark_offline(&mut self) {
        self.online_since = None;
    }

    /// Records a message sent.
    pub const fn record_message_sent(&mut self) {
        self.messages_sent += 1;
    }

    /// Records a message received.
    pub const fn record_message_received(&mut self) {
        self.messages_received += 1;
    }

    /// Records a successful join.
    pub const fn record_successful_join(&mut self) {
        self.successful_joins += 1;
    }

    /// Records a failed join.
    pub const fn record_failed_join(&mut self) {
        self.failed_joins += 1;
    }

    /// Records a capacity broadcast.
    pub const fn record_announcement_broadcast(&mut self) {
        self.announcements_broadcast += 1;
    }

    /// Records a provider search.
    pub const fn record_provider_search(&mut self) {
        self.provider_searches += 1;
    }
}

/// Result of a provider search.
#[derive(Debug, Clone)]
pub struct ProviderSearchResult {
    /// Providers that matched the requirements.
    pub providers: Vec<CapacityAnnouncement>,
    /// How long the search took.
    pub duration: Duration,
    /// Number of peers queried.
    pub peers_queried: usize,
}

impl ProviderSearchResult {
    /// Creates a new search result.
    #[must_use]
    pub const fn new(
        providers: Vec<CapacityAnnouncement>,
        duration: Duration,
        peers_queried: usize,
    ) -> Self {
        Self {
            providers,
            duration,
            peers_queried,
        }
    }

    /// Returns true if any providers were found.
    #[must_use]
    pub fn has_providers(&self) -> bool {
        !self.providers.is_empty()
    }

    /// Returns the number of providers found.
    #[must_use]
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }
}

/// Inner state for [`MoltNetwork`] (behind [`RwLock`]).
#[derive(Debug)]
struct NetworkInner {
    state: NetworkState,
    peer_table: PeerTable,
    known_capacities: HashMap<PeerId, CapacityAnnouncement>,
    stats: NetworkStats,
    our_announcement: Option<CapacityAnnouncement>,
}

impl NetworkInner {
    fn new() -> Self {
        Self {
            state: NetworkState::Offline,
            peer_table: PeerTable::new(),
            known_capacities: HashMap::new(),
            stats: NetworkStats::new(),
            our_announcement: None,
        }
    }
}

/// Main network interface for the MOLT P2P network.
///
/// This provides high-level operations for participating in the network:
/// - Joining and leaving
/// - Broadcasting capacity announcements
/// - Finding compute providers
#[derive(Debug)]
pub struct MoltNetwork {
    config: NetworkConfig,
    local_peer_id: PeerId,
    /// Signing key for authenticating messages (used in future network operations).
    #[allow(dead_code)]
    signing_key: SigningKey,
    connections: SharedConnectionPool,
    inner: Arc<RwLock<NetworkInner>>,
}

impl MoltNetwork {
    /// Creates a new MOLT network instance.
    #[must_use]
    pub fn new(config: NetworkConfig, signing_key: SigningKey) -> Self {
        let verifying_key = signing_key.verifying_key();
        let local_peer_id = PeerId::from_public_key(&verifying_key);
        let connection_config = ConnectionPoolConfig::new(config.max_peers);

        Self {
            config,
            local_peer_id,
            signing_key,
            connections: SharedConnectionPool::new(connection_config),
            inner: Arc::new(RwLock::new(NetworkInner::new())),
        }
    }

    /// Returns the local peer ID.
    #[must_use]
    pub const fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    /// Returns the network configuration.
    #[must_use]
    pub const fn config(&self) -> &NetworkConfig {
        &self.config
    }

    /// Returns the current network state.
    pub async fn state(&self) -> NetworkState {
        self.inner.read().await.state
    }

    /// Returns the current network statistics.
    pub async fn stats(&self) -> NetworkStats {
        self.inner.read().await.stats.clone()
    }

    /// Returns the number of known peers.
    pub async fn peer_count(&self) -> usize {
        self.inner.read().await.peer_table.len()
    }

    /// Returns the number of active connections.
    pub async fn connection_count(&self) -> usize {
        self.connections.active_count().await
    }

    /// Returns all known peer IDs.
    pub async fn known_peers(&self) -> Vec<PeerId> {
        self.inner
            .read()
            .await
            .peer_table
            .all_peers()
            .iter()
            .map(|p| p.peer_id())
            .collect()
    }

    /// Joins the network by connecting to bootstrap nodes.
    ///
    /// # Errors
    ///
    /// Returns an error if already online or if joining fails.
    #[allow(clippy::significant_drop_tightening)] // Simplified impl; guard usage is intentional
    pub async fn join(&self, bootstrap_nodes: &[String]) -> Result<(), P2pError> {
        let mut inner = self.inner.write().await;

        // Check current state
        match inner.state {
            NetworkState::Online => {
                return Err(P2pError::Protocol("Already online".to_string()));
            }
            NetworkState::Joining => {
                return Err(P2pError::Protocol("Already joining".to_string()));
            }
            NetworkState::Leaving => {
                return Err(P2pError::Protocol("Currently leaving".to_string()));
            }
            NetworkState::Offline => {}
        }

        inner.state = NetworkState::Joining;

        // If no bootstrap nodes provided, we become the first node
        if bootstrap_nodes.is_empty() {
            inner.state = NetworkState::Online;
            inner.stats.mark_online();
            inner.stats.record_successful_join();
            return Ok(());
        }

        // In a real implementation, we would:
        // 1. Parse bootstrap node addresses
        // 2. Attempt QUIC connections to each
        // 3. Send Join messages
        // 4. Wait for JoinAck responses
        // 5. Populate peer table with known peers

        // For now, we simulate a successful join
        for node_addr in bootstrap_nodes {
            let bootstrap = BootstrapNode::new(node_addr.clone());
            // In real impl: attempt connection here
            let _ = bootstrap;
        }

        inner.state = NetworkState::Online;
        inner.stats.mark_online();
        inner.stats.record_successful_join();

        Ok(())
    }

    /// Gracefully leaves the network.
    ///
    /// # Errors
    ///
    /// Returns an error if not currently online.
    #[allow(clippy::significant_drop_tightening)] // Multiple lock acquisitions needed
    pub async fn leave(&self) -> Result<(), P2pError> {
        let mut inner = self.inner.write().await;

        if inner.state != NetworkState::Online {
            return Err(P2pError::Protocol(format!(
                "Cannot leave: not online (state: {:?})",
                inner.state
            )));
        }

        inner.state = NetworkState::Leaving;

        // In a real implementation, we would:
        // 1. Send Leave messages to all connected peers
        // 2. Wait for acknowledgments (with timeout)
        // 3. Close all connections gracefully

        // For now, just clean up
        drop(inner);

        // Clear all connections
        let peer_ids = self.connections.peer_ids().await;
        for peer_id in peer_ids {
            self.connections.remove(&peer_id).await;
        }

        let mut inner = self.inner.write().await;
        inner.state = NetworkState::Offline;
        inner.stats.mark_offline();
        inner.known_capacities.clear();

        Ok(())
    }

    /// Broadcasts our capacity announcement to all connected peers.
    ///
    /// # Errors
    ///
    /// Returns an error if not online.
    #[allow(clippy::significant_drop_tightening)] // Guard held for state consistency
    pub async fn broadcast_capacity(
        &self,
        announcement: CapacityAnnouncement,
    ) -> Result<usize, P2pError> {
        let mut inner = self.inner.write().await;

        if !inner.state.is_operational() {
            return Err(P2pError::Protocol(format!(
                "Cannot broadcast: not operational (state: {:?})",
                inner.state
            )));
        }

        // Store our own announcement
        inner.our_announcement = Some(announcement.clone());
        inner.stats.record_announcement_broadcast();

        // Get active peer count
        let active_peers = self.connections.active_count().await;

        // In a real implementation, we would:
        // 1. Create CapacityAnnounce message
        // 2. Send to all connected peers
        // 3. Handle gossip propagation

        // Record messages sent
        for _ in 0..active_peers {
            inner.stats.record_message_sent();
        }

        Ok(active_peers)
    }

    /// Finds providers matching the given requirements.
    ///
    /// # Errors
    ///
    /// Returns an error if not online.
    #[allow(clippy::significant_drop_tightening)] // Guard held for atomic search
    pub async fn find_providers(
        &self,
        requirements: &CapacityRequirements,
        max_results: usize,
    ) -> Result<ProviderSearchResult, P2pError> {
        let start = std::time::Instant::now();

        let mut inner = self.inner.write().await;

        if !inner.state.is_operational() {
            return Err(P2pError::Protocol(format!(
                "Cannot search: not operational (state: {:?})",
                inner.state
            )));
        }

        inner.stats.record_provider_search();

        // Search locally known capacities first
        let mut matching: Vec<CapacityAnnouncement> = inner
            .known_capacities
            .values()
            .filter(|ann| !ann.is_expired() && matches_requirements(ann, requirements))
            .cloned()
            .collect();

        // In a real implementation, we would also:
        // 1. Send CapacityRequest messages to connected peers
        // 2. Wait for CapacityResponse messages (with timeout)
        // 3. Aggregate and deduplicate results

        // Limit results
        matching.truncate(max_results);

        let peers_queried = self.connections.active_count().await;
        let duration = start.elapsed();

        Ok(ProviderSearchResult::new(matching, duration, peers_queried))
    }

    /// Stores a received capacity announcement.
    pub async fn store_capacity(&self, announcement: CapacityAnnouncement) {
        let mut inner = self.inner.write().await;

        // Don't store expired announcements
        if announcement.is_expired() {
            return;
        }

        // Store or update
        inner
            .known_capacities
            .insert(announcement.peer_id(), announcement);
    }

    /// Returns all known capacity announcements.
    pub async fn known_capacities(&self) -> Vec<CapacityAnnouncement> {
        let inner = self.inner.read().await;
        inner.known_capacities.values().cloned().collect()
    }

    /// Removes expired capacity announcements.
    pub async fn cleanup_expired_capacities(&self) -> usize {
        let mut inner = self.inner.write().await;
        let initial = inner.known_capacities.len();

        inner.known_capacities.retain(|_, ann| !ann.is_expired());

        initial - inner.known_capacities.len()
    }

    /// Adds a peer to the peer table.
    pub async fn add_peer(&self, info: PeerInfo) {
        let mut inner = self.inner.write().await;
        inner.peer_table.insert(info);
    }

    /// Removes a peer from the peer table.
    pub async fn remove_peer(&self, peer_id: &PeerId) {
        let mut inner = self.inner.write().await;
        inner.peer_table.remove(peer_id);
        inner.known_capacities.remove(peer_id);
    }

    /// Gets information about a specific peer.
    pub async fn get_peer(&self, peer_id: &PeerId) -> Option<PeerInfo> {
        let inner = self.inner.read().await;
        inner.peer_table.get(peer_id).cloned()
    }

    /// Returns our current capacity announcement if set.
    pub async fn our_announcement(&self) -> Option<CapacityAnnouncement> {
        self.inner.read().await.our_announcement.clone()
    }
}

/// Checks if a capacity announcement matches the given requirements.
fn matches_requirements(ann: &CapacityAnnouncement, reqs: &CapacityRequirements) -> bool {
    // Check VRAM requirement
    if let Some(min_vram) = reqs.min_vram_gb {
        let has_sufficient_vram = ann.gpus().iter().any(|gpu| gpu.vram_gb >= min_vram);
        if !has_sufficient_vram {
            return false;
        }
    }

    // Check GPU model requirement
    if let Some(ref model) = reqs.gpu_model {
        let has_model = ann
            .gpus()
            .iter()
            .any(|gpu| gpu.model.to_lowercase().contains(&model.to_lowercase()));
        if !has_model {
            return false;
        }
    }

    // Check GPU count requirement
    if let Some(min_count) = reqs.min_gpu_count {
        let total_gpus: u32 = ann.gpus().iter().map(|g| g.count).sum();
        if total_gpus < min_count {
            return false;
        }
    }

    // Check job type requirement
    if let Some(ref job_type) = reqs.job_type {
        let has_job_type = ann.job_types().iter().any(|jt| jt == job_type);
        if !has_job_type {
            return false;
        }
    }

    // Check price requirement
    if let Some(max_price) = reqs.max_gpu_hour_cents {
        if ann.pricing().gpu_hour_cents > max_price {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gossip::{GpuInfo, Pricing};
    use rand::rngs::OsRng;

    fn make_signing_key() -> SigningKey {
        SigningKey::generate(&mut OsRng)
    }

    fn make_announcement(peer_id: PeerId, gpus: Vec<GpuInfo>, pricing: Pricing) -> CapacityAnnouncement {
        CapacityAnnouncement::new(peer_id, gpus, pricing, vec!["inference".to_string(), "training".to_string()], Duration::from_secs(300))
    }

    fn make_gpu(model: &str, vram: u32, count: u32) -> GpuInfo {
        GpuInfo { model: model.to_string(), vram_gb: vram, count }
    }

    fn make_pricing(gpu: u64, cpu: u64) -> Pricing {
        Pricing { gpu_hour_cents: gpu, cpu_hour_cents: cpu }
    }

    fn make_network() -> MoltNetwork {
        MoltNetwork::new(NetworkConfig::new(), make_signing_key())
    }

    #[test]
    fn network_state_flags() {
        assert!(!NetworkState::Offline.is_operational());
        assert!(!NetworkState::Joining.is_operational());
        assert!(NetworkState::Online.is_operational());
        assert!(!NetworkState::Leaving.is_operational());
        assert!(!NetworkState::Offline.can_operate());
        assert!(NetworkState::Joining.can_operate());
        assert!(NetworkState::Online.can_operate());
        assert!(!NetworkState::Leaving.can_operate());
    }

    #[test]
    fn network_config_defaults_and_builder() {
        let def = NetworkConfig::default();
        assert!(def.bind_addresses.is_empty() && def.bootstrap_nodes.is_empty());
        assert_eq!(def.max_peers, 50);

        let cfg = NetworkConfig::new()
            .with_max_peers(100)
            .with_announce_interval(Duration::from_secs(120))
            .with_provider_timeout(Duration::from_secs(30))
            .with_bootstrap_nodes(vec!["n1".into(), "n2".into()]);
        assert_eq!(cfg.max_peers, 100);
        assert_eq!(cfg.bootstrap_nodes.len(), 2);
    }

    #[test]
    fn network_stats_tracking_and_uptime() {
        let mut stats = NetworkStats::new();
        assert!(stats.uptime().is_none());

        stats.record_message_sent();
        stats.record_message_received();
        stats.record_successful_join();
        stats.record_announcement_broadcast();
        stats.record_provider_search();
        assert_eq!((stats.messages_sent, stats.messages_received), (1, 1));
        assert_eq!(stats.successful_joins, 1);

        stats.mark_online();
        std::thread::sleep(Duration::from_millis(10));
        assert!(stats.uptime().unwrap() >= Duration::from_millis(10));
        stats.mark_offline();
        assert!(stats.uptime().is_none());
    }

    #[test]
    fn provider_search_result_accessors() {
        let empty = ProviderSearchResult::new(vec![], Duration::from_millis(50), 5);
        assert!(!empty.has_providers() && empty.provider_count() == 0);

        let sk = make_signing_key();
        let pid = PeerId::from_public_key(&sk.verifying_key());
        let ann = make_announcement(pid, vec![make_gpu("A100", 80, 4)], make_pricing(200, 20));
        let with = ProviderSearchResult::new(vec![ann], Duration::from_millis(100), 10);
        assert!(with.has_providers() && with.provider_count() == 1);
    }

    #[tokio::test]
    async fn network_lifecycle() {
        let net = make_network();
        assert_eq!(net.state().await, NetworkState::Offline);

        // Join without bootstrap
        net.join(&[]).await.expect("join");
        assert_eq!(net.state().await, NetworkState::Online);
        assert!(net.stats().await.online_since.is_some());

        // Double join fails
        assert!(net.join(&[]).await.is_err());

        // Leave
        net.leave().await.expect("leave");
        assert_eq!(net.state().await, NetworkState::Offline);

        // Leave when offline fails
        assert!(net.leave().await.is_err());
    }

    #[tokio::test]
    async fn network_join_with_bootstrap() {
        let net = make_network();
        net.join(&["/ip4/1.2.3.4/tcp/8080".into()]).await.expect("join");
        assert_eq!(net.state().await, NetworkState::Online);
    }

    #[tokio::test]
    async fn network_broadcast_and_offline_errors() {
        let net = make_network();
        let sk = make_signing_key();
        let pid = PeerId::from_public_key(&sk.verifying_key());
        let ann = make_announcement(pid, vec![make_gpu("RTX 4090", 24, 2)], make_pricing(100, 10));

        // Broadcast when offline fails
        assert!(net.broadcast_capacity(ann.clone()).await.is_err());

        net.join(&[]).await.expect("join");
        let count = net.broadcast_capacity(ann).await.expect("broadcast");
        assert_eq!(count, 0); // No peers
        assert!(net.our_announcement().await.is_some());
    }

    #[tokio::test]
    async fn network_find_providers() {
        let net = make_network();
        assert!(net.find_providers(&CapacityRequirements::any(), 10).await.is_err());

        net.join(&[]).await.expect("join");
        assert!(!net.find_providers(&CapacityRequirements::any(), 10).await.unwrap().has_providers());

        // Store A100 and RTX 4090
        let (sk1, sk2) = (make_signing_key(), make_signing_key());
        let (pid1, pid2) = (PeerId::from_public_key(&sk1.verifying_key()), PeerId::from_public_key(&sk2.verifying_key()));
        net.store_capacity(make_announcement(pid1, vec![make_gpu("A100", 80, 8)], make_pricing(200, 20))).await;
        net.store_capacity(make_announcement(pid2, vec![make_gpu("RTX 4090", 24, 2)], make_pricing(100, 10))).await;

        // Various requirement filters
        assert_eq!(net.find_providers(&CapacityRequirements::any(), 10).await.unwrap().provider_count(), 2);
        assert_eq!(net.find_providers(&CapacityRequirements::any().with_gpu_model("A100"), 10).await.unwrap().provider_count(), 1);
        assert_eq!(net.find_providers(&CapacityRequirements::any().with_min_vram(48), 10).await.unwrap().provider_count(), 1);
        assert_eq!(net.find_providers(&CapacityRequirements::any().with_max_price(150), 10).await.unwrap().provider_count(), 1);
    }

    #[tokio::test]
    async fn network_peer_and_capacity_management() {
        let net = make_network();
        net.join(&[]).await.expect("join");

        let sk = make_signing_key();
        let pid = PeerId::from_public_key(&sk.verifying_key());
        let info = PeerInfo::new(pid, vec!["/ip4/1.1.1.1/tcp:8080".into()], vec!["gpu".into()]);

        net.add_peer(info).await;
        assert_eq!(net.peer_count().await, 1);
        assert!(net.known_peers().await.contains(&pid));
        assert!(net.get_peer(&pid).await.is_some());

        net.remove_peer(&pid).await;
        assert_eq!(net.peer_count().await, 0);

        let ann = make_announcement(pid, vec![make_gpu("H100", 80, 4)], make_pricing(300, 30));
        net.store_capacity(ann).await;
        assert_eq!(net.known_capacities().await.len(), 1);
    }

    #[test]
    fn matches_requirements_filters() {
        let sk = make_signing_key();
        let pid = PeerId::from_public_key(&sk.verifying_key());

        // Basic announcement: RTX 4090, 24GB, count 2, price 100
        let ann = make_announcement(pid, vec![make_gpu("RTX 4090", 24, 2)], make_pricing(100, 10));
        assert!(matches_requirements(&ann, &CapacityRequirements::any()));
        assert!(matches_requirements(&ann, &CapacityRequirements::any().with_min_vram(16)));
        assert!(!matches_requirements(&ann, &CapacityRequirements::any().with_min_vram(32)));
        assert!(matches_requirements(&ann, &CapacityRequirements::any().with_gpu_model("4090")));
        assert!(matches_requirements(&ann, &CapacityRequirements::any().with_gpu_model("rtx")));
        assert!(!matches_requirements(&ann, &CapacityRequirements::any().with_gpu_model("A100")));
        assert!(matches_requirements(&ann, &CapacityRequirements::any().with_job_type("inference")));
        assert!(!matches_requirements(&ann, &CapacityRequirements::any().with_job_type("rendering")));
        assert!(matches_requirements(&ann, &CapacityRequirements::any().with_max_price(100)));
        assert!(!matches_requirements(&ann, &CapacityRequirements::any().with_max_price(50)));

        // Multi-GPU count test
        let ann2 = make_announcement(pid, vec![make_gpu("RTX 4090", 24, 2), make_gpu("RTX 3090", 24, 2)], make_pricing(100, 10));
        assert!(matches_requirements(&ann2, &CapacityRequirements::any().with_min_gpu_count(4)));
        assert!(!matches_requirements(&ann2, &CapacityRequirements::any().with_min_gpu_count(5)));

        // Combined requirements
        let ann3 = make_announcement(pid, vec![make_gpu("A100", 80, 8)], make_pricing(200, 20));
        let reqs = CapacityRequirements::any().with_min_vram(48).with_gpu_model("A100").with_min_gpu_count(4).with_job_type("inference").with_max_price(300);
        assert!(matches_requirements(&ann3, &reqs));
        assert!(!matches_requirements(&ann3, &CapacityRequirements::any().with_max_price(100)));
    }

    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn stats_uptime_monotonic(sleep_ms in 1u64..50) {
                let mut stats = NetworkStats::new();
                stats.mark_online();
                std::thread::sleep(Duration::from_millis(sleep_ms));
                prop_assert!(stats.uptime().unwrap() >= Duration::from_millis(sleep_ms));
            }

            #[test]
            fn vram_threshold(vram in 1u32..256, required in 1u32..256) {
                let sk = SigningKey::generate(&mut OsRng);
                let pid = PeerId::from_public_key(&sk.verifying_key());
                let ann = make_announcement(pid, vec![make_gpu("GPU", vram, 1)], make_pricing(100, 10));
                prop_assert_eq!(matches_requirements(&ann, &CapacityRequirements::any().with_min_vram(required)), vram >= required);
            }
        }
    }
}
