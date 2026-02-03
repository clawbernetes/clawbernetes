//! Solana client for MOLT token operations.
//!
//! This module provides a client for interacting with Solana to perform
//! MOLT token operations. Currently uses a simulated backend for development.

use crate::amount::Amount;
use crate::error::{MoltError, Result};
use crate::escrow::{Escrow, EscrowId, EscrowState};
use crate::transaction::{Transaction, TransactionId};
use crate::wallet::{Address, Wallet};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

/// Solana network to connect to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    /// Mainnet-beta (production).
    Mainnet,
    /// Devnet (testing).
    Devnet,
    /// Testnet.
    Testnet,
    /// Local validator.
    Localnet,
}

impl Network {
    /// Get the RPC URL for this network.
    #[must_use]
    pub fn rpc_url(&self) -> &'static str {
        match self {
            Self::Mainnet => "https://api.mainnet-beta.solana.com",
            Self::Devnet => "https://api.devnet.solana.com",
            Self::Testnet => "https://api.testnet.solana.com",
            Self::Localnet => "http://localhost:8899",
        }
    }

    /// Get the WebSocket URL for this network.
    #[must_use]
    pub fn ws_url(&self) -> &'static str {
        match self {
            Self::Mainnet => "wss://api.mainnet-beta.solana.com",
            Self::Devnet => "wss://api.devnet.solana.com",
            Self::Testnet => "wss://api.testnet.solana.com",
            Self::Localnet => "ws://localhost:8900",
        }
    }
}

/// Simulated account state for development.
#[derive(Debug, Clone)]
struct SimulatedAccount {
    balance: Amount,
}

/// Simulated Solana state for development.
#[derive(Debug, Default)]
struct SimulatedState {
    accounts: HashMap<String, SimulatedAccount>,
    transactions: HashMap<String, Transaction>,
    escrows: HashMap<String, Escrow>,
}

/// MOLT token client.
///
/// Provides methods for wallet operations, transfers, and escrow management.
/// Currently uses a simulated backend for development.
pub struct MoltClient {
    network: Network,
    state: Arc<Mutex<SimulatedState>>,
}

impl MoltClient {
    /// Create a new client for the given network.
    #[must_use]
    pub fn new(network: Network) -> Self {
        Self {
            network,
            state: Arc::new(Mutex::new(SimulatedState::default())),
        }
    }

    /// Create a devnet client.
    #[must_use]
    pub fn devnet() -> Self {
        Self::new(Network::Devnet)
    }

    /// Create a mainnet client.
    #[must_use]
    pub fn mainnet() -> Self {
        Self::new(Network::Mainnet)
    }

    /// Create a localnet client.
    #[must_use]
    pub fn localnet() -> Self {
        Self::new(Network::Localnet)
    }

    /// Get the network.
    #[must_use]
    pub fn network(&self) -> Network {
        self.network
    }

    /// Get the RPC URL.
    #[must_use]
    pub fn rpc_url(&self) -> &'static str {
        self.network.rpc_url()
    }

    /// Get the balance of an address.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    pub async fn balance(&self, address: &Address) -> Result<Amount> {
        let state = self.state.lock().await;
        let balance = state
            .accounts
            .get(address.as_str())
            .map_or(Amount::ZERO, |a| a.balance);
        Ok(balance)
    }

    /// Airdrop tokens to an address (devnet/testnet only).
    ///
    /// # Errors
    ///
    /// Returns error if not on devnet/testnet or airdrop fails.
    pub async fn airdrop(&self, address: &Address, amount: Amount) -> Result<()> {
        if self.network == Network::Mainnet {
            return Err(MoltError::network_error("airdrop not available on mainnet"));
        }

        let mut state = self.state.lock().await;
        let account = state
            .accounts
            .entry(address.as_str().to_string())
            .or_insert(SimulatedAccount {
                balance: Amount::ZERO,
            });
        account.balance = account.balance.saturating_add(amount);

        info!(
            address = %address,
            amount = %amount,
            "airdrop completed"
        );
        Ok(())
    }

    /// Transfer tokens from one wallet to another.
    ///
    /// # Errors
    ///
    /// Returns error if transfer fails.
    pub async fn transfer(
        &self,
        from: &Wallet,
        to: &Address,
        amount: Amount,
    ) -> Result<Transaction> {
        // Check balance
        let balance = self.balance(from.address()).await?;
        if balance < amount {
            return Err(MoltError::insufficient_balance(
                balance.as_molt(),
                amount.as_molt(),
            ));
        }

        // Create transaction
        let mut tx = Transaction::transfer(from.address().clone(), to.clone(), amount);

        // Simulate transfer
        {
            let mut state = self.state.lock().await;

            // Deduct from sender
            if let Some(account) = state.accounts.get_mut(from.address().as_str()) {
                account.balance = account.balance.saturating_sub(amount);
            }

            // Add to recipient
            let recipient = state
                .accounts
                .entry(to.as_str().to_string())
                .or_insert(SimulatedAccount {
                    balance: Amount::ZERO,
                });
            recipient.balance = recipient.balance.saturating_add(amount);

            // Sign and finalize
            let signature = format!("sig_{}", tx.id);
            tx.mark_submitted(signature);
            tx.mark_finalized();

            // Store transaction
            state.transactions.insert(tx.id.to_string(), tx.clone());
        }

        debug!(
            from = %from.address(),
            to = %to,
            amount = %amount,
            "transfer completed"
        );

        Ok(tx)
    }

    /// Get a transaction by ID.
    ///
    /// # Errors
    ///
    /// Returns error if transaction not found.
    pub async fn get_transaction(&self, id: &TransactionId) -> Result<Transaction> {
        let state = self.state.lock().await;
        state
            .transactions
            .get(id.as_str())
            .cloned()
            .ok_or_else(|| MoltError::TransactionNotFound {
                id: id.to_string(),
            })
    }

    /// Create an escrow for a job.
    ///
    /// # Errors
    ///
    /// Returns error if escrow creation fails.
    pub async fn create_escrow(
        &self,
        buyer: &Wallet,
        provider: &Address,
        amount: Amount,
        job_id: String,
    ) -> Result<Escrow> {
        // Check balance
        let balance = self.balance(buyer.address()).await?;
        if balance < amount {
            return Err(MoltError::insufficient_balance(
                balance.as_molt(),
                amount.as_molt(),
            ));
        }

        // Create escrow
        let mut escrow = Escrow::new(
            buyer.address().clone(),
            provider.clone(),
            amount,
            job_id,
            None,
        );

        // Simulate fund transfer to escrow
        {
            let mut state = self.state.lock().await;

            // Deduct from buyer
            if let Some(account) = state.accounts.get_mut(buyer.address().as_str()) {
                account.balance = account.balance.saturating_sub(amount);
            }

            // Activate escrow
            escrow.activate()?;

            // Store escrow
            state.escrows.insert(escrow.id.to_string(), escrow.clone());
        }

        info!(
            escrow_id = %escrow.id,
            buyer = %buyer.address(),
            provider = %provider,
            amount = %amount,
            "escrow created"
        );

        Ok(escrow)
    }

    /// Get an escrow by ID.
    ///
    /// # Errors
    ///
    /// Returns error if escrow not found.
    pub async fn get_escrow(&self, id: &EscrowId) -> Result<Escrow> {
        let state = self.state.lock().await;
        state
            .escrows
            .get(id.as_str())
            .cloned()
            .ok_or_else(|| MoltError::EscrowNotFound {
                id: id.to_string(),
            })
    }

    /// Release escrow to provider (job completed).
    ///
    /// # Errors
    ///
    /// Returns error if release fails.
    pub async fn release_escrow(&self, id: &EscrowId) -> Result<Escrow> {
        let mut state = self.state.lock().await;

        // First, get escrow info and validate
        let (payout, provider_addr) = {
            let escrow = state.escrows.get_mut(id.as_str()).ok_or_else(|| {
                MoltError::EscrowNotFound {
                    id: id.to_string(),
                }
            })?;
            escrow.start_release()?;
            (escrow.provider_payout(), escrow.provider.as_str().to_string())
        };

        // Transfer to provider
        let provider = state
            .accounts
            .entry(provider_addr)
            .or_insert(SimulatedAccount {
                balance: Amount::ZERO,
            });
        provider.balance = provider.balance.saturating_add(payout);

        // Complete release
        let escrow = state.escrows.get_mut(id.as_str()).ok_or_else(|| {
            MoltError::EscrowNotFound {
                id: id.to_string(),
            }
        })?;
        let signature = format!("release_{id}");
        escrow.complete_release(signature)?;

        info!(
            escrow_id = %id,
            payout = %payout,
            "escrow released"
        );

        Ok(escrow.clone())
    }

    /// Refund escrow to buyer (job failed/cancelled).
    ///
    /// # Errors
    ///
    /// Returns error if refund fails.
    pub async fn refund_escrow(&self, id: &EscrowId) -> Result<Escrow> {
        let mut state = self.state.lock().await;

        // First, get escrow info and validate
        let (amount, buyer_addr) = {
            let escrow = state.escrows.get_mut(id.as_str()).ok_or_else(|| {
                MoltError::EscrowNotFound {
                    id: id.to_string(),
                }
            })?;
            escrow.start_refund()?;
            (escrow.amount, escrow.buyer.as_str().to_string())
        };

        // Return full amount to buyer
        let buyer = state
            .accounts
            .entry(buyer_addr)
            .or_insert(SimulatedAccount {
                balance: Amount::ZERO,
            });
        buyer.balance = buyer.balance.saturating_add(amount);

        // Complete refund
        let escrow = state.escrows.get_mut(id.as_str()).ok_or_else(|| {
            MoltError::EscrowNotFound {
                id: id.to_string(),
            }
        })?;
        let signature = format!("refund_{id}");
        escrow.complete_refund(signature)?;

        info!(
            escrow_id = %id,
            amount = %amount,
            "escrow refunded"
        );

        Ok(escrow.clone())
    }

    /// List all escrows for an address (as buyer or provider).
    pub async fn list_escrows(&self, address: &Address) -> Vec<Escrow> {
        let state = self.state.lock().await;
        state
            .escrows
            .values()
            .filter(|e| e.buyer == *address || e.provider == *address)
            .cloned()
            .collect()
    }

    /// List active escrows.
    pub async fn list_active_escrows(&self) -> Vec<Escrow> {
        let state = self.state.lock().await;
        state
            .escrows
            .values()
            .filter(|e| e.state == EscrowState::Active)
            .cloned()
            .collect()
    }
}

#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for MoltClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MoltClient")
            .field("network", &self.network)
            .field("rpc_url", &self.rpc_url())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_client_with_balance(balance: Amount) -> (MoltClient, Wallet) {
        let client = MoltClient::devnet();
        let wallet = Wallet::generate().expect("should generate");
        client.airdrop(wallet.address(), balance).await.expect("should airdrop");
        (client, wallet)
    }

    #[tokio::test]
    async fn test_balance_zero() {
        let client = MoltClient::devnet();
        let wallet = Wallet::generate().expect("should generate");
        let balance = client.balance(wallet.address()).await.expect("should get balance");
        assert!(balance.is_zero());
    }

    #[tokio::test]
    async fn test_airdrop() {
        let client = MoltClient::devnet();
        let wallet = Wallet::generate().expect("should generate");

        client
            .airdrop(wallet.address(), Amount::molt(100.0))
            .await
            .expect("should airdrop");

        let balance = client.balance(wallet.address()).await.expect("should get balance");
        assert!((balance.as_molt() - 100.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_airdrop_mainnet_fails() {
        let client = MoltClient::mainnet();
        let wallet = Wallet::generate().expect("should generate");
        let result = client.airdrop(wallet.address(), Amount::molt(100.0)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_transfer() {
        let (client, sender) = setup_client_with_balance(Amount::molt(100.0)).await;
        let recipient = Wallet::generate().expect("should generate");

        let tx = client
            .transfer(&sender, recipient.address(), Amount::molt(30.0))
            .await
            .expect("should transfer");

        assert!(tx.status.is_success());

        let sender_balance = client.balance(sender.address()).await.unwrap();
        let recipient_balance = client.balance(recipient.address()).await.unwrap();

        assert!((sender_balance.as_molt() - 70.0).abs() < 0.001);
        assert!((recipient_balance.as_molt() - 30.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_transfer_insufficient_funds() {
        let (client, sender) = setup_client_with_balance(Amount::molt(10.0)).await;
        let recipient = Wallet::generate().expect("should generate");

        let result = client
            .transfer(&sender, recipient.address(), Amount::molt(20.0))
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), MoltError::InsufficientBalance { .. }));
    }

    #[tokio::test]
    async fn test_escrow_create_and_release() {
        let (client, buyer) = setup_client_with_balance(Amount::molt(100.0)).await;
        let provider = Wallet::generate().expect("should generate");

        // Create escrow
        let escrow = client
            .create_escrow(&buyer, provider.address(), Amount::molt(50.0), "job-1".to_string())
            .await
            .expect("should create escrow");

        assert_eq!(escrow.state, EscrowState::Active);

        // Check buyer balance decreased
        let buyer_balance = client.balance(buyer.address()).await.unwrap();
        assert!((buyer_balance.as_molt() - 50.0).abs() < 0.001);

        // Release escrow
        let released = client.release_escrow(&escrow.id).await.expect("should release");
        assert_eq!(released.state, EscrowState::Released);

        // Check provider received payout (minus 5% fee)
        let provider_balance = client.balance(provider.address()).await.unwrap();
        assert!((provider_balance.as_molt() - 47.5).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_escrow_refund() {
        let (client, buyer) = setup_client_with_balance(Amount::molt(100.0)).await;
        let provider = Wallet::generate().expect("should generate");

        // Create escrow
        let escrow = client
            .create_escrow(&buyer, provider.address(), Amount::molt(50.0), "job-1".to_string())
            .await
            .expect("should create escrow");

        // Refund escrow
        let refunded = client.refund_escrow(&escrow.id).await.expect("should refund");
        assert_eq!(refunded.state, EscrowState::Refunded);

        // Check buyer got full amount back
        let buyer_balance = client.balance(buyer.address()).await.unwrap();
        assert!((buyer_balance.as_molt() - 100.0).abs() < 0.001);

        // Provider should have nothing
        let provider_balance = client.balance(provider.address()).await.unwrap();
        assert!(provider_balance.is_zero());
    }

    #[tokio::test]
    async fn test_list_escrows() {
        let (client, buyer) = setup_client_with_balance(Amount::molt(200.0)).await;
        let provider = Wallet::generate().expect("should generate");

        // Create two escrows
        client
            .create_escrow(&buyer, provider.address(), Amount::molt(50.0), "job-1".to_string())
            .await
            .expect("should create");
        client
            .create_escrow(&buyer, provider.address(), Amount::molt(30.0), "job-2".to_string())
            .await
            .expect("should create");

        let escrows = client.list_escrows(buyer.address()).await;
        assert_eq!(escrows.len(), 2);

        let active = client.list_active_escrows().await;
        assert_eq!(active.len(), 2);
    }
}
