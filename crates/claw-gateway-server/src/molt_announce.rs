//! Capacity announcement for MOLT network.
//!
//! Broadcasts our compute capacity to the P2P network so buyers can discover us.

use std::sync::Arc;
use std::time::Duration;

use claw_gateway::NodeRegistry;
use ed25519_dalek::SigningKey;
use molt_p2p::gossip::{CapacityAnnouncement, GpuInfo, Pricing};
use molt_p2p::PeerId;
use tokio::sync::Mutex;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::molt_staking::StakingTracker;

/// Capacity announcer that periodically broadcasts our capabilities.
pub struct CapacityAnnouncer {
    /// Our peer ID.
    peer_id: PeerId,
    /// Signing key for announcements.
    signing_key: SigningKey,
    /// Node registry to get capabilities.
    registry: Arc<Mutex<NodeRegistry>>,
    /// Staking tracker to include stake info.
    staking: Arc<Mutex<StakingTracker>>,
    /// Announcement interval.
    interval: Duration,
    /// Network region.
    region: Option<String>,
    /// Base pricing (per GPU-hour in lamports).
    base_price_per_gpu_hour: u64,
}

impl CapacityAnnouncer {
    /// Default price per GPU-hour (0.1 MOLT).
    pub const DEFAULT_PRICE_PER_GPU_HOUR: u64 = 100_000_000; // 0.1 MOLT

    /// Create a new capacity announcer.
    #[must_use]
    pub fn new(
        peer_id: PeerId,
        signing_key: SigningKey,
        registry: Arc<Mutex<NodeRegistry>>,
        staking: Arc<Mutex<StakingTracker>>,
        interval: Duration,
    ) -> Self {
        Self {
            peer_id,
            signing_key,
            registry,
            staking,
            interval,
            region: None,
            base_price_per_gpu_hour: Self::DEFAULT_PRICE_PER_GPU_HOUR,
        }
    }

    /// Set the network region.
    pub fn set_region(&mut self, region: impl Into<String>) {
        self.region = Some(region.into());
    }

    /// Set the base price per GPU-hour.
    pub fn set_base_price(&mut self, lamports_per_gpu_hour: u64) {
        self.base_price_per_gpu_hour = lamports_per_gpu_hour;
    }

    /// Build a capacity announcement from current state.
    pub async fn build_announcement(&self) -> Option<CapacityAnnouncement> {
        let registry = self.registry.lock().await;
        let staking = self.staking.lock().await;

        // Check if we can provide compute
        if !staking.can_provide() {
            debug!("Not announcing: insufficient stake");
            return None;
        }

        // Aggregate GPU capacity from all healthy nodes
        let healthy_nodes = registry.healthy_nodes();
        if healthy_nodes.is_empty() {
            debug!("Not announcing: no healthy nodes");
            return None;
        }

        let mut total_gpus = Vec::new();
        let mut total_vram_mib = 0u64;

        for node in healthy_nodes {
            for gpu in &node.capabilities.gpus {
                total_gpus.push(GpuInfo {
                    model: gpu.name.clone(),
                    vram_gb: (gpu.memory_mib / 1024) as u32,
                    count: 1,
                });
                total_vram_mib += gpu.memory_mib;
            }
        }

        if total_gpus.is_empty() {
            debug!("Not announcing: no GPUs available");
            return None;
        }

        // Build pricing based on GPU count
        let gpu_count = total_gpus.len() as u32;
        let pricing = Pricing {
            gpu_hour_cents: self.base_price_per_gpu_hour / 10_000_000, // Convert lamports to cents (approx)
            cpu_hour_cents: 10, // 10 cents per CPU hour default
        };

        // Create announcement
        let mut announcement = CapacityAnnouncement::new(
            self.peer_id,
            total_gpus,
            pricing,
            vec!["container".into(), "inference".into()],
            std::time::Duration::from_secs(600), // 10 minute TTL
        );

        // Sign the announcement
        announcement.sign(&self.signing_key);

        info!(
            peer_id = %self.peer_id,
            gpu_count,
            total_vram_mib,
            region = ?self.region,
            "Built capacity announcement"
        );

        Some(announcement)
    }

    /// Run the announcement loop.
    ///
    /// Returns a future that broadcasts announcements periodically.
    pub async fn run(&self, mut shutdown: tokio::sync::broadcast::Receiver<()>) {
        let mut ticker = interval(self.interval);

        info!(
            interval_secs = self.interval.as_secs(),
            "Starting capacity announcer"
        );

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Some(announcement) = self.build_announcement().await {
                        // In a full implementation, we would broadcast this
                        // via the P2P network gossip layer
                        debug!(
                            peer_id = %announcement.peer_id(),
                            "Would broadcast announcement"
                        );
                    }
                }
                _ = shutdown.recv() => {
                    info!("Capacity announcer shutting down");
                    break;
                }
            }
        }
    }
}

/// Summary of our announced capacity.
#[derive(Debug, Clone, Default)]
pub struct CapacitySummary {
    /// Total GPU count.
    pub gpu_count: u32,
    /// Total VRAM in MiB.
    pub total_vram_mib: u64,
    /// GPU models available.
    pub gpu_models: Vec<String>,
    /// Current pricing.
    pub price_per_gpu_hour: u64,
    /// Whether we're actively announcing.
    pub is_announcing: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_gateway::NodeRegistry;
    use claw_proto::{GpuCapability, NodeCapabilities, NodeId};
    use rand::rngs::OsRng;

    fn make_signing_key() -> SigningKey {
        SigningKey::generate(&mut OsRng)
    }

    fn make_peer_id(key: &SigningKey) -> PeerId {
        PeerId::from_public_key(&key.verifying_key())
    }

    fn make_node_with_gpu() -> (NodeId, NodeCapabilities) {
        let caps = NodeCapabilities::new(8, 16384).with_gpu(GpuCapability {
            index: 0,
            name: "NVIDIA RTX 4090".into(),
            memory_mib: 24576,
            uuid: "GPU-001".into(),
        });
        (NodeId::new(), caps)
    }

    #[tokio::test]
    async fn test_build_announcement_no_stake() {
        let key = make_signing_key();
        let peer_id = make_peer_id(&key);
        let registry = Arc::new(Mutex::new(NodeRegistry::new()));
        let staking = Arc::new(Mutex::new(StakingTracker::new()));

        let announcer = CapacityAnnouncer::new(
            peer_id,
            key,
            registry,
            staking,
            Duration::from_secs(60),
        );

        // No stake = no announcement
        let announcement = announcer.build_announcement().await;
        assert!(announcement.is_none());
    }

    #[tokio::test]
    async fn test_build_announcement_no_nodes() {
        let key = make_signing_key();
        let peer_id = make_peer_id(&key);
        let registry = Arc::new(Mutex::new(NodeRegistry::new()));
        let staking = Arc::new(Mutex::new(StakingTracker::new()));

        // Add stake
        staking.lock().await.update_staked(100_000_000_000);

        let announcer = CapacityAnnouncer::new(
            peer_id,
            key,
            registry,
            staking,
            Duration::from_secs(60),
        );

        // No nodes = no announcement
        let announcement = announcer.build_announcement().await;
        assert!(announcement.is_none());
    }

    #[tokio::test]
    async fn test_build_announcement_success() {
        let key = make_signing_key();
        let peer_id = make_peer_id(&key);
        let mut reg = NodeRegistry::new();

        // Add a node with GPU
        let (node_id, caps) = make_node_with_gpu();
        reg.register(node_id, caps).unwrap();

        let registry = Arc::new(Mutex::new(reg));
        let staking = Arc::new(Mutex::new(StakingTracker::new()));

        // Add stake
        staking.lock().await.update_staked(100_000_000_000);

        let mut announcer = CapacityAnnouncer::new(
            peer_id,
            key,
            registry,
            staking,
            Duration::from_secs(60),
        );
        announcer.set_region("us-west");

        let announcement = announcer.build_announcement().await;
        assert!(announcement.is_some());

        let ann = announcement.unwrap();
        assert_eq!(ann.peer_id(), peer_id);
        assert!(ann.is_signed());
    }
}
