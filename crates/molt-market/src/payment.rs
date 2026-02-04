//! Payment service integrating MOLT token operations with marketplace.
//!
//! This module provides the bridge between the marketplace's escrow logic
//! and the actual Solana token operations.

use molt_token::{Address, Amount, Escrow, EscrowId, MoltClient, Network, Wallet};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::MarketError;
use crate::escrow::{EscrowAccount, EscrowState};

/// Payment service for marketplace operations.
///
/// Wraps the MOLT token client and provides high-level payment operations
/// for the marketplace.
pub struct PaymentService {
    /// MOLT token client.
    client: Arc<MoltClient>,
    /// Marketplace wallet (for collecting fees).
    marketplace_wallet: Option<Arc<Mutex<Wallet>>>,
}

impl PaymentService {
    /// Create a new payment service for the given network.
    #[must_use]
    pub fn new(network: Network) -> Self {
        Self {
            client: Arc::new(MoltClient::new(network)),
            marketplace_wallet: None,
        }
    }

    /// Create a devnet payment service.
    #[must_use]
    pub fn devnet() -> Self {
        Self::new(Network::Devnet)
    }

    /// Create a mainnet payment service.
    #[must_use]
    pub fn mainnet() -> Self {
        Self::new(Network::Mainnet)
    }

    /// Set the marketplace wallet for collecting fees.
    #[must_use]
    pub fn with_marketplace_wallet(mut self, wallet: Wallet) -> Self {
        self.marketplace_wallet = Some(Arc::new(Mutex::new(wallet)));
        self
    }

    /// Get the underlying MOLT client.
    #[must_use]
    pub fn client(&self) -> &MoltClient {
        &self.client
    }

    /// Get the balance of an address.
    ///
    /// # Errors
    ///
    /// Returns error if balance query fails.
    pub async fn get_balance(&self, address: &Address) -> Result<Amount, MarketError> {
        self.client
            .balance(address)
            .await
            .map_err(|e| MarketError::PaymentError(e.to_string()))
    }

    /// Create an escrow for a job.
    ///
    /// Transfers funds from buyer to escrow account.
    ///
    /// # Errors
    ///
    /// Returns error if escrow creation fails.
    pub async fn create_escrow(
        &self,
        buyer_wallet: &Wallet,
        provider: &Address,
        amount: Amount,
        job_id: String,
    ) -> Result<TokenEscrow, MarketError> {
        let escrow = self
            .client
            .create_escrow(buyer_wallet, provider, amount, job_id.clone())
            .await
            .map_err(|e| MarketError::PaymentError(e.to_string()))?;

        Ok(TokenEscrow {
            id: escrow.id,
            job_id,
            buyer: buyer_wallet.address().clone(),
            provider: provider.clone(),
            amount,
            state: escrow.state.into(),
        })
    }

    /// Release escrow to provider (job completed).
    ///
    /// # Errors
    ///
    /// Returns error if release fails.
    pub async fn release_escrow(&self, escrow_id: &EscrowId) -> Result<TokenEscrow, MarketError> {
        let escrow = self
            .client
            .release_escrow(escrow_id)
            .await
            .map_err(|e| MarketError::PaymentError(e.to_string()))?;

        Ok(TokenEscrow::from_token_escrow(&escrow))
    }

    /// Refund escrow to buyer (job cancelled/failed).
    ///
    /// # Errors
    ///
    /// Returns error if refund fails.
    pub async fn refund_escrow(&self, escrow_id: &EscrowId) -> Result<TokenEscrow, MarketError> {
        let escrow = self
            .client
            .refund_escrow(escrow_id)
            .await
            .map_err(|e| MarketError::PaymentError(e.to_string()))?;

        Ok(TokenEscrow::from_token_escrow(&escrow))
    }

    /// Get escrow by ID.
    ///
    /// # Errors
    ///
    /// Returns error if escrow not found.
    pub async fn get_escrow(&self, escrow_id: &EscrowId) -> Result<TokenEscrow, MarketError> {
        let escrow = self
            .client
            .get_escrow(escrow_id)
            .await
            .map_err(|e| MarketError::PaymentError(e.to_string()))?;

        Ok(TokenEscrow::from_token_escrow(&escrow))
    }

    /// Airdrop tokens (devnet/testnet only).
    ///
    /// # Errors
    ///
    /// Returns error on mainnet or if airdrop fails.
    pub async fn airdrop(&self, address: &Address, amount: Amount) -> Result<(), MarketError> {
        self.client
            .airdrop(address, amount)
            .await
            .map_err(|e| MarketError::PaymentError(e.to_string()))
    }
}

#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for PaymentService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaymentService")
            .field("network", &self.client.network())
            .finish_non_exhaustive()
    }
}

/// Token escrow state wrapper for marketplace integration.
#[derive(Debug, Clone)]
pub struct TokenEscrow {
    /// Escrow ID on chain.
    pub id: EscrowId,
    /// Job ID this escrow is for.
    pub job_id: String,
    /// Buyer address.
    pub buyer: Address,
    /// Provider address.
    pub provider: Address,
    /// Amount in escrow.
    pub amount: Amount,
    /// Current state.
    pub state: EscrowState,
}

impl TokenEscrow {
    /// Convert from molt-token Escrow.
    fn from_token_escrow(escrow: &Escrow) -> Self {
        Self {
            id: escrow.id.clone(),
            job_id: escrow.job_id.clone(),
            buyer: escrow.buyer.clone(),
            provider: escrow.provider.clone(),
            amount: escrow.amount,
            state: escrow.state.into(),
        }
    }

    /// Convert to marketplace `EscrowAccount`.
    #[must_use]
    pub fn to_escrow_account(&self) -> EscrowAccount {
        EscrowAccount::new(
            self.job_id.clone(),
            self.buyer.to_string(),
            self.provider.to_string(),
            self.amount.lamports(),
        )
    }
}

/// Convert `molt_token::EscrowState` to marketplace `EscrowState`.
impl From<molt_token::EscrowState> for EscrowState {
    fn from(state: molt_token::EscrowState) -> Self {
        match state {
            molt_token::EscrowState::Creating => EscrowState::Created,
            molt_token::EscrowState::Active => EscrowState::Funded,
            molt_token::EscrowState::Releasing => EscrowState::Funded,
            molt_token::EscrowState::Released => EscrowState::Released,
            molt_token::EscrowState::Refunding => EscrowState::Funded,
            molt_token::EscrowState::Refunded => EscrowState::Refunded,
            molt_token::EscrowState::Disputed => EscrowState::Disputed,
            molt_token::EscrowState::Expired => EscrowState::Refunded,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_payment_service_balance() {
        let service = PaymentService::devnet();
        let wallet = Wallet::generate().expect("wallet");

        // New wallet has zero balance
        let balance = service.get_balance(wallet.address()).await.unwrap();
        assert!(balance.is_zero());
    }

    #[tokio::test]
    async fn test_payment_service_airdrop() {
        let service = PaymentService::devnet();
        let wallet = Wallet::generate().expect("wallet");

        // Airdrop some tokens
        service
            .airdrop(wallet.address(), Amount::molt(100.0))
            .await
            .unwrap();

        let balance = service.get_balance(wallet.address()).await.unwrap();
        assert!((balance.as_molt() - 100.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_payment_service_escrow_flow() {
        let service = PaymentService::devnet();
        let buyer = Wallet::generate().expect("buyer");
        let provider = Wallet::generate().expect("provider");

        // Fund buyer
        service
            .airdrop(buyer.address(), Amount::molt(100.0))
            .await
            .unwrap();

        // Create escrow
        let escrow = service
            .create_escrow(&buyer, provider.address(), Amount::molt(50.0), "job-1".into())
            .await
            .unwrap();

        assert_eq!(escrow.state, EscrowState::Funded);

        // Release escrow
        let released = service.release_escrow(&escrow.id).await.unwrap();
        assert_eq!(released.state, EscrowState::Released);

        // Check provider got paid (95% after fee)
        let provider_balance = service.get_balance(provider.address()).await.unwrap();
        assert!((provider_balance.as_molt() - 47.5).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_payment_service_refund() {
        let service = PaymentService::devnet();
        let buyer = Wallet::generate().expect("buyer");
        let provider = Wallet::generate().expect("provider");

        // Fund buyer
        service
            .airdrop(buyer.address(), Amount::molt(100.0))
            .await
            .unwrap();

        // Create escrow
        let escrow = service
            .create_escrow(&buyer, provider.address(), Amount::molt(50.0), "job-2".into())
            .await
            .unwrap();

        // Refund escrow
        let refunded = service.refund_escrow(&escrow.id).await.unwrap();
        assert_eq!(refunded.state, EscrowState::Refunded);

        // Check buyer got full refund
        let buyer_balance = service.get_balance(buyer.address()).await.unwrap();
        assert!((buyer_balance.as_molt() - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_escrow_state_conversion() {
        assert_eq!(
            EscrowState::from(molt_token::EscrowState::Creating),
            EscrowState::Created
        );
        assert_eq!(
            EscrowState::from(molt_token::EscrowState::Active),
            EscrowState::Funded
        );
        assert_eq!(
            EscrowState::from(molt_token::EscrowState::Released),
            EscrowState::Released
        );
        assert_eq!(
            EscrowState::from(molt_token::EscrowState::Refunded),
            EscrowState::Refunded
        );
        assert_eq!(
            EscrowState::from(molt_token::EscrowState::Disputed),
            EscrowState::Disputed
        );
        assert_eq!(
            EscrowState::from(molt_token::EscrowState::Expired),
            EscrowState::Refunded
        );
    }
}
