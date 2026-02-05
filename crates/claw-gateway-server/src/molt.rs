//! MOLT P2P network integration.
//!
//! This module provides integration with the MOLT compute marketplace:
//! - P2P network for peer discovery and gossip
//! - Token wallet for balance management
//!
//! The MOLT integration is optional - if not configured, the gateway
//! operates in standalone mode without P2P features.

use std::sync::Arc;

use ed25519_dalek::SigningKey;
use molt_p2p::{MoltNetwork, NetworkConfig, NetworkState, PeerId};
use molt_token::{Address, Amount, MoltClient, Wallet};
use tracing::info;

use crate::error::{ServerError, ServerResult};

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
}

impl MoltIntegration {
    /// Create a new MOLT integration with the given wallet.
    ///
    /// # Errors
    ///
    /// Returns an error if wallet creation fails.
    pub fn new(wallet: Wallet, region: Option<String>) -> ServerResult<Self> {
        // Extract signing key from wallet secret key bytes
        let signing_key = SigningKey::from_bytes(wallet.secret_key());
        let network_config = NetworkConfig::new();
        let network = MoltNetwork::new(network_config, signing_key);
        let token_client = MoltClient::devnet();

        Ok(Self {
            network: Arc::new(network),
            token_client,
            wallet,
            region,
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
