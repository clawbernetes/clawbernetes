//! MOLT job tunnels over WireGuard.
//!
//! This module provides tunnel management for MOLT marketplace jobs:
//! - [`JobTunnel`]: A WireGuard tunnel for a specific job
//! - [`TunnelManager`]: Manages multiple job tunnels
//! - Bandwidth tracking per tunnel

use crate::error::P2pError;
use crate::protocol::{PeerId, PeerInfo};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Unique identifier for a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JobId(Uuid);

impl JobId {
    /// Creates a new random job ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates a job ID from a UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the underlying UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Bandwidth statistics for a tunnel.
#[derive(Debug, Default)]
pub struct BandwidthStats {
    /// Bytes sent through the tunnel.
    bytes_sent: AtomicU64,
    /// Bytes received through the tunnel.
    bytes_received: AtomicU64,
}

impl BandwidthStats {
    /// Creates new bandwidth statistics.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records bytes sent.
    pub fn record_sent(&self, bytes: u64) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Records bytes received.
    pub fn record_received(&self, bytes: u64) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Returns total bytes sent.
    #[must_use]
    pub fn bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }

    /// Returns total bytes received.
    #[must_use]
    pub fn bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }

    /// Returns total bytes transferred (sent + received).
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.bytes_sent() + self.bytes_received()
    }
}

impl Clone for BandwidthStats {
    fn clone(&self) -> Self {
        Self {
            bytes_sent: AtomicU64::new(self.bytes_sent.load(Ordering::Relaxed)),
            bytes_received: AtomicU64::new(self.bytes_received.load(Ordering::Relaxed)),
        }
    }
}

/// State of a job tunnel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelState {
    /// Tunnel is being established.
    Establishing,
    /// Tunnel is active and ready.
    Active,
    /// Tunnel is being closed.
    Closing,
    /// Tunnel has been closed.
    Closed,
    /// Tunnel failed to establish.
    Failed,
}

impl TunnelState {
    /// Returns true if the tunnel is usable.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// Returns true if the tunnel is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Closed | Self::Failed)
    }
}

/// A WireGuard tunnel for a MOLT job.
///
/// Each job gets its own tunnel between the buyer and provider,
/// with a unique allocated IP for routing within the mesh.
#[derive(Debug)]
pub struct JobTunnel {
    /// The job this tunnel serves.
    job_id: JobId,
    /// The provider's peer information.
    provider: PeerInfo,
    /// The buyer's peer information.
    buyer: PeerInfo,
    /// The allocated mesh IP for this tunnel.
    allocated_ip: Ipv4Addr,
    /// When the tunnel was created.
    created_at: DateTime<Utc>,
    /// Current tunnel state.
    state: TunnelState,
    /// Bandwidth statistics.
    bandwidth: Arc<BandwidthStats>,
}

impl JobTunnel {
    /// Creates a new job tunnel.
    #[must_use]
    pub fn new(job_id: JobId, provider: PeerInfo, buyer: PeerInfo, allocated_ip: Ipv4Addr) -> Self {
        Self {
            job_id,
            provider,
            buyer,
            allocated_ip,
            created_at: Utc::now(),
            state: TunnelState::Establishing,
            bandwidth: Arc::new(BandwidthStats::new()),
        }
    }

    /// Returns the job ID.
    #[must_use]
    pub const fn job_id(&self) -> JobId {
        self.job_id
    }

    /// Returns the provider's peer information.
    #[must_use]
    pub const fn provider(&self) -> &PeerInfo {
        &self.provider
    }

    /// Returns the buyer's peer information.
    #[must_use]
    pub const fn buyer(&self) -> &PeerInfo {
        &self.buyer
    }

    /// Returns the allocated mesh IP.
    #[must_use]
    pub const fn allocated_ip(&self) -> Ipv4Addr {
        self.allocated_ip
    }

    /// Returns when the tunnel was created.
    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Returns the current tunnel state.
    #[must_use]
    pub const fn state(&self) -> TunnelState {
        self.state
    }

    /// Returns the bandwidth statistics.
    #[must_use]
    pub fn bandwidth(&self) -> &BandwidthStats {
        &self.bandwidth
    }

    /// Returns a clone of the bandwidth stats Arc.
    #[must_use]
    pub fn bandwidth_handle(&self) -> Arc<BandwidthStats> {
        Arc::clone(&self.bandwidth)
    }

    /// Marks the tunnel as active.
    pub fn mark_active(&mut self) {
        if self.state == TunnelState::Establishing {
            self.state = TunnelState::Active;
        }
    }

    /// Marks the tunnel as closing.
    pub fn mark_closing(&mut self) {
        if self.state == TunnelState::Active {
            self.state = TunnelState::Closing;
        }
    }

    /// Marks the tunnel as closed.
    pub fn mark_closed(&mut self) {
        self.state = TunnelState::Closed;
    }

    /// Marks the tunnel as failed.
    pub fn mark_failed(&mut self) {
        self.state = TunnelState::Failed;
    }

    /// Returns the tunnel duration if active.
    #[must_use]
    pub fn duration(&self) -> std::time::Duration {
        Utc::now()
            .signed_duration_since(self.created_at)
            .to_std()
            .unwrap_or(std::time::Duration::ZERO)
    }
}

/// A minimal job representation for tunnel creation.
#[derive(Debug, Clone)]
pub struct Job {
    /// Job identifier.
    pub id: JobId,
    /// Provider for this job.
    pub provider_id: PeerId,
}

impl Job {
    /// Creates a new job.
    #[must_use]
    pub fn new(provider_id: PeerId) -> Self {
        Self {
            id: JobId::new(),
            provider_id,
        }
    }

    /// Creates a job with a specific ID.
    #[must_use]
    pub const fn with_id(id: JobId, provider_id: PeerId) -> Self {
        Self { id, provider_id }
    }
}

/// IP allocator for job tunnels.
#[derive(Debug)]
struct IpAllocator {
    /// Base network (e.g., 10.200.0.0).
    base: u32,
    /// Next IP to allocate.
    next: u32,
    /// Allocated IPs.
    allocated: HashMap<JobId, Ipv4Addr>,
}

impl IpAllocator {
    fn new(base: Ipv4Addr) -> Self {
        Self {
            base: u32::from(base),
            next: 1, // Start at .1
            allocated: HashMap::new(),
        }
    }

    fn allocate(&mut self, job_id: JobId) -> Ipv4Addr {
        let ip = Ipv4Addr::from(self.base + self.next);
        self.next += 1;
        self.allocated.insert(job_id, ip);
        ip
    }

    fn release(&mut self, job_id: &JobId) -> Option<Ipv4Addr> {
        self.allocated.remove(job_id)
    }

    #[allow(dead_code)]
    fn get(&self, job_id: &JobId) -> Option<Ipv4Addr> {
        self.allocated.get(job_id).copied()
    }
}

/// Manages job tunnels for the MOLT marketplace.
#[derive(Debug)]
pub struct TunnelManager {
    /// Active tunnels by job ID.
    tunnels: Arc<RwLock<HashMap<JobId, JobTunnel>>>,
    /// IP allocator.
    ip_allocator: Arc<RwLock<IpAllocator>>,
    /// Peer lookup table.
    peers: Arc<RwLock<HashMap<PeerId, PeerInfo>>>,
}

impl TunnelManager {
    /// Creates a new tunnel manager.
    ///
    /// The base IP is used to allocate mesh IPs for job tunnels.
    #[must_use]
    pub fn new() -> Self {
        Self::with_base_ip(Ipv4Addr::new(10, 200, 0, 0))
    }

    /// Creates a tunnel manager with a custom base IP.
    #[must_use]
    pub fn with_base_ip(base_ip: Ipv4Addr) -> Self {
        Self {
            tunnels: Arc::new(RwLock::new(HashMap::new())),
            ip_allocator: Arc::new(RwLock::new(IpAllocator::new(base_ip))),
            peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Registers a peer for tunnel creation.
    pub async fn register_peer(&self, info: PeerInfo) {
        let mut peers = self.peers.write().await;
        peers.insert(info.peer_id(), info);
    }

    /// Creates a job tunnel between a provider and buyer.
    ///
    /// # Errors
    ///
    /// Returns an error if the provider or buyer is not registered.
    pub async fn create_job_tunnel(
        &self,
        job: &Job,
        buyer: &PeerInfo,
    ) -> Result<JobTunnel, P2pError> {
        // Look up provider
        let peers = self.peers.read().await;
        let provider = peers.get(&job.provider_id).ok_or_else(|| {
            P2pError::PeerNotFound(format!("Provider {} not found", job.provider_id))
        })?;

        // Allocate IP
        let allocated_ip = {
            let mut allocator = self.ip_allocator.write().await;
            allocator.allocate(job.id)
        };

        // Create tunnel
        let mut tunnel = JobTunnel::new(job.id, provider.clone(), buyer.clone(), allocated_ip);

        // In a real implementation, we would:
        // 1. Configure WireGuard peer on both sides
        // 2. Establish the encrypted tunnel
        // 3. Wait for handshake confirmation
        // For now, mark as active immediately
        tunnel.mark_active();

        // Store tunnel
        {
            let mut tunnels = self.tunnels.write().await;
            tunnels.insert(job.id, tunnel.clone());
        }

        // Return a fresh instance (the stored one has moved)
        Ok(JobTunnel::new(job.id, provider.clone(), buyer.clone(), allocated_ip))
    }

    /// Closes a job tunnel.
    ///
    /// # Errors
    ///
    /// Returns an error if the tunnel doesn't exist.
    pub async fn close_tunnel(&self, job_id: &JobId) -> Result<(), P2pError> {
        let mut tunnels = self.tunnels.write().await;

        let tunnel = tunnels.get_mut(job_id).ok_or_else(|| {
            P2pError::PeerNotFound(format!("Tunnel for job {} not found", job_id))
        })?;

        // Mark closing
        tunnel.mark_closing();

        // In a real implementation, we would:
        // 1. Send close notification to peer
        // 2. Remove WireGuard peer configuration
        // 3. Wait for cleanup confirmation

        // Mark closed
        tunnel.mark_closed();

        // Release IP
        {
            let mut allocator = self.ip_allocator.write().await;
            allocator.release(job_id);
        }

        // Remove tunnel
        tunnels.remove(job_id);

        Ok(())
    }

    /// Gets a tunnel by job ID.
    pub async fn get_tunnel(&self, job_id: &JobId) -> Option<JobTunnel> {
        let tunnels = self.tunnels.read().await;
        tunnels.get(job_id).map(|t| {
            JobTunnel::new(t.job_id, t.provider.clone(), t.buyer.clone(), t.allocated_ip)
        })
    }

    /// Returns the number of active tunnels.
    pub async fn active_count(&self) -> usize {
        let tunnels = self.tunnels.read().await;
        tunnels.values().filter(|t| t.state.is_active()).count()
    }

    /// Returns all job IDs with active tunnels.
    pub async fn active_job_ids(&self) -> Vec<JobId> {
        let tunnels = self.tunnels.read().await;
        tunnels
            .iter()
            .filter(|(_, t)| t.state.is_active())
            .map(|(id, _)| *id)
            .collect()
    }

    /// Gets bandwidth stats for a tunnel.
    pub async fn get_bandwidth(&self, job_id: &JobId) -> Option<Arc<BandwidthStats>> {
        let tunnels = self.tunnels.read().await;
        tunnels.get(job_id).map(|t| t.bandwidth_handle())
    }

    /// Returns total bandwidth across all tunnels.
    pub async fn total_bandwidth(&self) -> (u64, u64) {
        let tunnels = self.tunnels.read().await;
        let mut sent = 0u64;
        let mut received = 0u64;
        for tunnel in tunnels.values() {
            sent += tunnel.bandwidth.bytes_sent();
            received += tunnel.bandwidth.bytes_received();
        }
        (sent, received)
    }
}

impl Default for TunnelManager {
    fn default() -> Self {
        Self::new()
    }
}

// Clone implementation for JobTunnel (needed for returning from manager)
impl Clone for JobTunnel {
    fn clone(&self) -> Self {
        Self {
            job_id: self.job_id,
            provider: self.provider.clone(),
            buyer: self.buyer.clone(),
            allocated_ip: self.allocated_ip,
            created_at: self.created_at,
            state: self.state,
            bandwidth: Arc::clone(&self.bandwidth),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_wireguard::KeyPair;

    fn make_peer_info() -> PeerInfo {
        let keypair = KeyPair::generate();
        PeerInfo::from_wireguard_key(keypair.public_key(), vec![], vec![])
    }

    fn make_peer_id() -> PeerId {
        let keypair = KeyPair::generate();
        PeerId::from_wireguard_key(keypair.public_key())
    }

    // ==================== JobId Tests ====================

    #[test]
    fn job_id_new_is_unique() {
        let id1 = JobId::new();
        let id2 = JobId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn job_id_display() {
        let id = JobId::new();
        let display = id.to_string();
        assert!(!display.is_empty());
    }

    #[test]
    fn job_id_from_uuid_roundtrip() {
        let uuid = Uuid::new_v4();
        let id = JobId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), &uuid);
    }

    // ==================== BandwidthStats Tests ====================

    #[test]
    fn bandwidth_stats_new_is_zero() {
        let stats = BandwidthStats::new();
        assert_eq!(stats.bytes_sent(), 0);
        assert_eq!(stats.bytes_received(), 0);
        assert_eq!(stats.total_bytes(), 0);
    }

    #[test]
    fn bandwidth_stats_record_sent() {
        let stats = BandwidthStats::new();
        stats.record_sent(100);
        stats.record_sent(50);
        assert_eq!(stats.bytes_sent(), 150);
    }

    #[test]
    fn bandwidth_stats_record_received() {
        let stats = BandwidthStats::new();
        stats.record_received(200);
        stats.record_received(100);
        assert_eq!(stats.bytes_received(), 300);
    }

    #[test]
    fn bandwidth_stats_total() {
        let stats = BandwidthStats::new();
        stats.record_sent(100);
        stats.record_received(200);
        assert_eq!(stats.total_bytes(), 300);
    }

    #[test]
    fn bandwidth_stats_clone() {
        let stats = BandwidthStats::new();
        stats.record_sent(100);
        let cloned = stats.clone();
        assert_eq!(cloned.bytes_sent(), 100);
    }

    // ==================== TunnelState Tests ====================

    #[test]
    fn tunnel_state_is_active() {
        assert!(!TunnelState::Establishing.is_active());
        assert!(TunnelState::Active.is_active());
        assert!(!TunnelState::Closing.is_active());
        assert!(!TunnelState::Closed.is_active());
        assert!(!TunnelState::Failed.is_active());
    }

    #[test]
    fn tunnel_state_is_terminal() {
        assert!(!TunnelState::Establishing.is_terminal());
        assert!(!TunnelState::Active.is_terminal());
        assert!(!TunnelState::Closing.is_terminal());
        assert!(TunnelState::Closed.is_terminal());
        assert!(TunnelState::Failed.is_terminal());
    }

    // ==================== JobTunnel Tests ====================

    #[test]
    fn job_tunnel_creation() {
        let provider = make_peer_info();
        let buyer = make_peer_info();
        let job_id = JobId::new();
        let ip = Ipv4Addr::new(10, 200, 0, 1);

        let tunnel = JobTunnel::new(job_id, provider.clone(), buyer.clone(), ip);

        assert_eq!(tunnel.job_id(), job_id);
        assert_eq!(tunnel.provider().peer_id(), provider.peer_id());
        assert_eq!(tunnel.buyer().peer_id(), buyer.peer_id());
        assert_eq!(tunnel.allocated_ip(), ip);
        assert_eq!(tunnel.state(), TunnelState::Establishing);
    }

    #[test]
    fn job_tunnel_state_transitions() {
        let provider = make_peer_info();
        let buyer = make_peer_info();
        let mut tunnel = JobTunnel::new(JobId::new(), provider, buyer, Ipv4Addr::new(10, 0, 0, 1));

        assert_eq!(tunnel.state(), TunnelState::Establishing);

        tunnel.mark_active();
        assert_eq!(tunnel.state(), TunnelState::Active);

        tunnel.mark_closing();
        assert_eq!(tunnel.state(), TunnelState::Closing);

        tunnel.mark_closed();
        assert_eq!(tunnel.state(), TunnelState::Closed);
    }

    #[test]
    fn job_tunnel_mark_failed() {
        let provider = make_peer_info();
        let buyer = make_peer_info();
        let mut tunnel = JobTunnel::new(JobId::new(), provider, buyer, Ipv4Addr::new(10, 0, 0, 1));

        tunnel.mark_failed();
        assert_eq!(tunnel.state(), TunnelState::Failed);
    }

    #[test]
    fn job_tunnel_bandwidth_tracking() {
        let provider = make_peer_info();
        let buyer = make_peer_info();
        let tunnel = JobTunnel::new(JobId::new(), provider, buyer, Ipv4Addr::new(10, 0, 0, 1));

        tunnel.bandwidth().record_sent(100);
        tunnel.bandwidth().record_received(200);

        assert_eq!(tunnel.bandwidth().bytes_sent(), 100);
        assert_eq!(tunnel.bandwidth().bytes_received(), 200);
    }

    #[test]
    fn job_tunnel_duration() {
        let provider = make_peer_info();
        let buyer = make_peer_info();
        let tunnel = JobTunnel::new(JobId::new(), provider, buyer, Ipv4Addr::new(10, 0, 0, 1));

        std::thread::sleep(std::time::Duration::from_millis(10));

        let duration = tunnel.duration();
        assert!(duration >= std::time::Duration::from_millis(10));
    }

    // ==================== Job Tests ====================

    #[test]
    fn job_creation() {
        let provider_id = make_peer_id();
        let job = Job::new(provider_id);
        assert_eq!(job.provider_id, provider_id);
    }

    #[test]
    fn job_with_id() {
        let job_id = JobId::new();
        let provider_id = make_peer_id();
        let job = Job::with_id(job_id, provider_id);
        assert_eq!(job.id, job_id);
        assert_eq!(job.provider_id, provider_id);
    }

    // ==================== TunnelManager Tests ====================

    #[tokio::test]
    async fn tunnel_manager_new() {
        let manager = TunnelManager::new();
        assert_eq!(manager.active_count().await, 0);
    }

    #[tokio::test]
    async fn tunnel_manager_create_tunnel() {
        let manager = TunnelManager::new();

        let provider = make_peer_info();
        let buyer = make_peer_info();

        // Register provider
        manager.register_peer(provider.clone()).await;

        // Create job
        let job = Job::new(provider.peer_id());

        // Create tunnel
        let tunnel = manager
            .create_job_tunnel(&job, &buyer)
            .await
            .expect("create tunnel");

        assert_eq!(tunnel.job_id(), job.id);
        assert_eq!(manager.active_count().await, 1);
    }

    #[tokio::test]
    async fn tunnel_manager_create_tunnel_provider_not_found() {
        let manager = TunnelManager::new();
        let buyer = make_peer_info();
        let job = Job::new(make_peer_id()); // Provider not registered

        let result = manager.create_job_tunnel(&job, &buyer).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn tunnel_manager_close_tunnel() {
        let manager = TunnelManager::new();

        let provider = make_peer_info();
        let buyer = make_peer_info();
        manager.register_peer(provider.clone()).await;

        let job = Job::new(provider.peer_id());
        manager.create_job_tunnel(&job, &buyer).await.expect("create");

        assert_eq!(manager.active_count().await, 1);

        manager.close_tunnel(&job.id).await.expect("close");

        assert_eq!(manager.active_count().await, 0);
    }

    #[tokio::test]
    async fn tunnel_manager_close_nonexistent() {
        let manager = TunnelManager::new();
        let result = manager.close_tunnel(&JobId::new()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn tunnel_manager_get_tunnel() {
        let manager = TunnelManager::new();

        let provider = make_peer_info();
        let buyer = make_peer_info();
        manager.register_peer(provider.clone()).await;

        let job = Job::new(provider.peer_id());
        manager.create_job_tunnel(&job, &buyer).await.expect("create");

        let tunnel = manager.get_tunnel(&job.id).await;
        assert!(tunnel.is_some());
        assert_eq!(tunnel.as_ref().map(|t| t.job_id()), Some(job.id));
    }

    #[tokio::test]
    async fn tunnel_manager_active_job_ids() {
        let manager = TunnelManager::new();

        let provider = make_peer_info();
        let buyer = make_peer_info();
        manager.register_peer(provider.clone()).await;

        let job1 = Job::new(provider.peer_id());
        let job2 = Job::new(provider.peer_id());

        manager.create_job_tunnel(&job1, &buyer).await.expect("create");
        manager.create_job_tunnel(&job2, &buyer).await.expect("create");

        let ids = manager.active_job_ids().await;
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&job1.id));
        assert!(ids.contains(&job2.id));
    }

    #[tokio::test]
    async fn tunnel_manager_bandwidth() {
        let manager = TunnelManager::new();

        let provider = make_peer_info();
        let buyer = make_peer_info();
        manager.register_peer(provider.clone()).await;

        let job = Job::new(provider.peer_id());
        manager.create_job_tunnel(&job, &buyer).await.expect("create");

        // Get bandwidth handle and record some traffic
        let bandwidth = manager.get_bandwidth(&job.id).await.expect("bandwidth");
        bandwidth.record_sent(100);
        bandwidth.record_received(200);

        let (sent, received) = manager.total_bandwidth().await;
        assert_eq!(sent, 100);
        assert_eq!(received, 200);
    }

    #[tokio::test]
    async fn tunnel_manager_ip_allocation() {
        let manager = TunnelManager::with_base_ip(Ipv4Addr::new(10, 100, 0, 0));

        let provider = make_peer_info();
        let buyer = make_peer_info();
        manager.register_peer(provider.clone()).await;

        let job1 = Job::new(provider.peer_id());
        let job2 = Job::new(provider.peer_id());

        let tunnel1 = manager.create_job_tunnel(&job1, &buyer).await.expect("create");
        let tunnel2 = manager.create_job_tunnel(&job2, &buyer).await.expect("create");

        // IPs should be allocated sequentially from base
        assert_eq!(tunnel1.allocated_ip(), Ipv4Addr::new(10, 100, 0, 1));
        assert_eq!(tunnel2.allocated_ip(), Ipv4Addr::new(10, 100, 0, 2));
    }

    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn bandwidth_stats_are_monotonic(sends in prop::collection::vec(1u64..1000, 0..10),
                                             recvs in prop::collection::vec(1u64..1000, 0..10)) {
                let stats = BandwidthStats::new();
                let mut expected_sent = 0u64;
                let mut expected_recv = 0u64;

                for s in sends {
                    expected_sent += s;
                    stats.record_sent(s);
                }
                for r in recvs {
                    expected_recv += r;
                    stats.record_received(r);
                }

                prop_assert_eq!(stats.bytes_sent(), expected_sent);
                prop_assert_eq!(stats.bytes_received(), expected_recv);
                prop_assert_eq!(stats.total_bytes(), expected_sent + expected_recv);
            }
        }
    }
}
