//! MOLT P2P network integration.
//!
//! This module provides integration with the MOLT compute marketplace:
//! - P2P network for peer discovery and gossip
//! - Token wallet for balance management
//! - Escrow tracking for pending payments
//! - Staking for provider eligibility
//! - Capacity announcements for discoverability
//!
//! The MOLT integration is optional - if not configured, the gateway
//! operates in standalone mode without P2P features.

use std::sync::Arc;

use ed25519_dalek::SigningKey;
use molt_p2p::{MoltNetwork, NetworkConfig, NetworkState, PeerId};
use molt_token::{Address, Amount, MoltClient, Wallet};
use tokio::sync::Mutex;
use tracing::info;

use crate::error::{ServerError, ServerResult};
use crate::molt_config::MoltConfig;
use crate::molt_escrow::EscrowTracker;
use crate::molt_staking::StakingTracker;

/// MOLT integration handle.
///
/// Holds references to the P2P network and token client.
#[derive(Debug)]
pub struct MoltIntegration {
    /// P2P network instance.
    network: Arc<MoltNetwork>,
    /// Token client for balance operations.
    token_client: MoltClient,
    /// Wallet for signing and receiving payments.
    wallet: Wallet,
    /// Network region (for peer info).
    region: Option<String>,
    /// Signing key for announcements.
    signing_key: SigningKey,
    /// Configuration.
    config: MoltConfig,
    /// Escrow tracker.
    escrow_tracker: Mutex<EscrowTracker>,
    /// Staking tracker.
    staking_tracker: Mutex<StakingTracker>,
}

impl MoltIntegration {
    /// Create a new MOLT integration with the given wallet.
    ///
    /// # Errors
    ///
    /// Returns an error if wallet creation fails.
    pub fn new(wallet: Wallet, region: Option<String>) -> ServerResult<Self> {
        Self::with_config(wallet, MoltConfig::default().with_region(region.unwrap_or_default()))
    }

    /// Create with full configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if initialization fails.
    pub fn with_config(wallet: Wallet, config: MoltConfig) -> ServerResult<Self> {
        // Extract signing key from wallet secret key bytes
        let signing_key = SigningKey::from_bytes(wallet.secret_key());
        let network_config = NetworkConfig::new()
            .with_bootstrap_nodes(config.bootstrap_addresses());
        let network = MoltNetwork::new(network_config, signing_key.clone());
        let token_client = MoltClient::devnet();

        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());
        let mut escrow_tracker = EscrowTracker::new();
        escrow_tracker.set_peer_id(peer_id.to_string());

        Ok(Self {
            network: Arc::new(network),
            token_client,
            wallet,
            region: config.region.clone(),
            signing_key,
            config,
            escrow_tracker: Mutex::new(escrow_tracker),
            staking_tracker: Mutex::new(StakingTracker::new()),
        })
    }

    /// Create with a fresh wallet for development/testing.
    ///
    /// # Errors
    ///
    /// Returns an error if wallet generation fails.
    pub fn new_with_generated_wallet() -> ServerResult<Self> {
        let wallet = Wallet::generate().map_err(|e| ServerError::Internal(e.to_string()))?;
        Self::new(wallet, None)
    }

    /// Get the local peer ID.
    #[must_use]
    pub fn peer_id(&self) -> PeerId {
        self.network.local_peer_id()
    }

    /// Get the wallet address.
    #[must_use]
    pub fn address(&self) -> &Address {
        self.wallet.address()
    }

    /// Get the network region.
    #[must_use]
    pub fn region(&self) -> Option<&str> {
        self.region.as_deref()
    }

    /// Get the current network state.
    pub async fn network_state(&self) -> NetworkState {
        self.network.state().await
    }

    /// Get the number of known peers.
    pub async fn peer_count(&self) -> usize {
        self.network.peer_count().await
    }

    /// Get all known peer IDs.
    pub async fn known_peers(&self) -> Vec<PeerId> {
        self.network.known_peers().await
    }

    /// Get the token balance.
    ///
    /// # Errors
    ///
    /// Returns an error if the balance query fails.
    pub async fn balance(&self) -> ServerResult<Amount> {
        self.token_client
            .balance(self.wallet.address())
            .await
            .map_err(|e| ServerError::Internal(format!("balance query failed: {e}")))
    }

    /// Join the P2P network.
    ///
    /// # Errors
    ///
    /// Returns an error if joining fails.
    pub async fn join(&self, bootstrap_nodes: &[String]) -> ServerResult<()> {
        info!(
            peer_id = %self.peer_id(),
            bootstrap_count = bootstrap_nodes.len(),
            "Joining MOLT network"
        );

        self.network
            .join(bootstrap_nodes)
            .await
            .map_err(|e| ServerError::Internal(format!("failed to join network: {e}")))?;

        info!(peer_id = %self.peer_id(), "Successfully joined MOLT network");
        Ok(())
    }

    /// Leave the P2P network.
    ///
    /// # Errors
    ///
    /// Returns an error if leaving fails.
    pub async fn leave(&self) -> ServerResult<()> {
        info!(peer_id = %self.peer_id(), "Leaving MOLT network");

        self.network
            .leave()
            .await
            .map_err(|e| ServerError::Internal(format!("failed to leave network: {e}")))?;

        Ok(())
    }

    /// Check if connected to the network.
    pub async fn is_connected(&self) -> bool {
        matches!(self.network_state().await, NetworkState::Online)
    }

    /// Join with configured bootstrap nodes.
    ///
    /// # Errors
    ///
    /// Returns an error if joining fails.
    pub async fn join_with_config(&self) -> ServerResult<()> {
        let bootstrap_addrs = self.config.bootstrap_addresses();
        if bootstrap_addrs.is_empty() {
            info!("No bootstrap nodes configured, starting as first node");
        }
        self.join(&bootstrap_addrs).await
    }

    /// Get the configuration.
    #[must_use]
    pub fn config(&self) -> &MoltConfig {
        &self.config
    }

    /// Get the signing key (for capacity announcements).
    #[must_use]
    pub fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }

    // ========================================================================
    // Escrow Operations
    // ========================================================================

    /// Get pending escrow balance.
    pub async fn pending_balance(&self) -> u64 {
        self.escrow_tracker.lock().await.pending_balance()
    }

    /// Get escrow summary.
    pub async fn escrow_summary(&self) -> crate::molt_escrow::EscrowSummary {
        self.escrow_tracker.lock().await.summary()
    }

    /// Add a tracked escrow.
    pub async fn track_escrow(&self, escrow: crate::molt_escrow::TrackedEscrow) {
        self.escrow_tracker.lock().await.add(escrow);
    }

    /// Get escrow tracker (for advanced operations).
    pub fn escrow_tracker(&self) -> &Mutex<EscrowTracker> {
        &self.escrow_tracker
    }

    // ========================================================================
    // Staking Operations
    // ========================================================================

    /// Get staked amount.
    pub async fn staked_amount(&self) -> u64 {
        self.staking_tracker.lock().await.staked_amount()
    }

    /// Get current staking tier.
    pub async fn staking_tier(&self) -> crate::molt_staking::StakingTier {
        self.staking_tracker.lock().await.tier()
    }

    /// Update staked amount (from on-chain query).
    pub async fn update_staked(&self, amount: u64) {
        self.staking_tracker.lock().await.update_staked(amount);
    }

    /// Check if can provide compute.
    pub async fn can_provide(&self) -> bool {
        self.staking_tracker.lock().await.can_provide()
    }

    /// Get staking tracker (for advanced operations).
    pub fn staking_tracker(&self) -> &Mutex<StakingTracker> {
        &self.staking_tracker
    }

    /// Refresh staking info from on-chain.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn refresh_staking(&self) -> ServerResult<()> {
        // In a full implementation, this would query the staking contract
        // For now, we just log
        info!(peer_id = %self.peer_id(), "Would refresh staking info from chain");
        Ok(())
    }

    // ========================================================================
    // Combined Balance Info
    // ========================================================================

    /// Get full balance breakdown.
    pub async fn balance_breakdown(&self) -> ServerResult<MoltBalance> {
        let balance = self.balance().await?;
        let pending = self.pending_balance().await;
        let staked = self.staked_amount().await;

        Ok(MoltBalance {
            balance: balance.lamports(),
            pending,
            staked,
        })
    }
}

/// MOLT status information for CLI responses.
#[derive(Debug, Clone)]
pub struct MoltStatus {
    /// Whether connected to the MOLT network.
    pub connected: bool,
    /// Number of known peers.
    pub peer_count: u32,
    /// Local node ID on the MOLT network.
    pub node_id: Option<String>,
    /// Network region.
    pub region: Option<String>,
}

/// MOLT peer information for CLI responses.
#[derive(Debug, Clone)]
pub struct MoltPeerInfo {
    /// Peer ID.
    pub peer_id: String,
    /// Peer address (if known).
    pub address: Option<String>,
    /// Last seen timestamp.
    pub last_seen: Option<chrono::DateTime<chrono::Utc>>,
}

/// MOLT balance information for CLI responses.
#[derive(Debug, Clone)]
pub struct MoltBalance {
    /// Current balance in base units.
    pub balance: u64,
    /// Pending balance (in escrow).
    pub pending: u64,
    /// Staked amount.
    pub staked: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_molt_status_fields() {
        let status = MoltStatus {
            connected: true,
            peer_count: 5,
            node_id: Some("peer-123".into()),
            region: Some("us-west".into()),
        };

        assert!(status.connected);
        assert_eq!(status.peer_count, 5);
        assert_eq!(status.node_id, Some("peer-123".into()));
    }

    #[test]
    fn test_molt_balance_fields() {
        let balance = MoltBalance {
            balance: 1_000_000_000,
            pending: 500_000_000,
            staked: 0,
        };

        assert_eq!(balance.balance, 1_000_000_000);
        assert_eq!(balance.pending, 500_000_000);
    }

    #[tokio::test]
    async fn test_molt_integration_creation() {
        let result = MoltIntegration::new_with_generated_wallet();
        assert!(result.is_ok());

        let molt = result.unwrap();
        // New integration should not be connected
        assert!(!molt.is_connected().await);
    }

    #[tokio::test]
    async fn test_peer_id_generation() {
        let molt = MoltIntegration::new_with_generated_wallet().unwrap();
        let peer_id = molt.peer_id();

        // Peer ID should be deterministic from wallet
        let peer_id2 = molt.peer_id();
        assert_eq!(peer_id, peer_id2);
    }
}
