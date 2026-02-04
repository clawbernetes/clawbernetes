# molt-token API Reference

Solana SPL token client for the MOLT compute marketplace.

## Overview

`molt-token` provides the on-chain integration for the MOLT token economy:

- Wallet management
- Token transfers
- Escrow for compute jobs
- Transaction history
- Network selection (devnet/mainnet)

## Installation

```toml
[dependencies]
molt-token = { path = "../molt-token" }
```

## Quick Start

```rust
use molt_token::{MoltClient, Network, Amount, Wallet};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client for devnet
    let client = MoltClient::new(Network::Devnet)?;
    
    // Load wallet
    let wallet = Wallet::from_file("~/.config/clawbernetes/wallet.json")?;
    
    // Check balance
    let balance = client.get_balance(&wallet.address()).await?;
    println!("Balance: {} MOLT", balance);
    
    Ok(())
}
```

---

## Amount

Precise token amount representation.

```rust
pub struct Amount {
    /// Amount in lamports (1 MOLT = 1_000_000_000 lamports)
    lamports: u64,
}

impl Amount {
    /// Create from MOLT (whole tokens)
    pub fn from_molt(molt: u64) -> Self;
    
    /// Create from lamports
    pub fn from_lamports(lamports: u64) -> Self;
    
    /// Get as MOLT (whole tokens)
    pub fn as_molt(&self) -> u64;
    
    /// Get as lamports
    pub fn as_lamports(&self) -> u64;
    
    /// Get as f64 for display
    pub fn as_f64(&self) -> f64;
    
    /// Zero amount
    pub const fn zero() -> Self;
    
    /// Check if zero
    pub fn is_zero(&self) -> bool;
}
```

### Amount Arithmetic

```rust
use molt_token::Amount;

let a = Amount::from_molt(100);
let b = Amount::from_molt(50);

// Addition
let sum = a.checked_add(&b).unwrap();  // 150 MOLT

// Subtraction
let diff = a.checked_sub(&b).unwrap(); // 50 MOLT

// Comparison
assert!(a > b);
assert!(sum == Amount::from_molt(150));
```

---

## Wallet

Local wallet management.

```rust
pub struct Wallet {
    keypair: Keypair,
}

impl Wallet {
    /// Generate a new wallet
    pub fn generate() -> Self;
    
    /// Load from file (JSON format)
    pub fn from_file(path: &str) -> Result<Self, MoltError>;
    
    /// Load from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MoltError>;
    
    /// Save to file
    pub fn save(&self, path: &str) -> Result<(), MoltError>;
    
    /// Get public address
    pub fn address(&self) -> Address;
    
    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> Signature;
}
```

### Example

```rust
use molt_token::Wallet;

// Generate new wallet
let wallet = Wallet::generate();
println!("Address: {}", wallet.address());

// Save for later
wallet.save("my-wallet.json")?;

// Load existing
let loaded = Wallet::from_file("my-wallet.json")?;
```

---

## Address

Public address representation.

```rust
pub struct Address(String);

impl Address {
    /// Create from base58 string
    pub fn from_str(s: &str) -> Result<Self, MoltError>;
    
    /// Get as base58 string
    pub fn as_str(&self) -> &str;
    
    /// Validate address format
    pub fn is_valid(s: &str) -> bool;
}

impl Display for Address {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result;
}
```

---

## Network

Solana network selection.

```rust
pub enum Network {
    /// Development network (free tokens)
    Devnet,
    /// Test network
    Testnet,
    /// Production network (real tokens)
    Mainnet,
    /// Local validator
    Localnet,
    /// Custom RPC endpoint
    Custom(String),
}

impl Network {
    /// Get RPC URL
    pub fn rpc_url(&self) -> &str;
    
    /// Get WebSocket URL
    pub fn ws_url(&self) -> &str;
    
    /// Check if this is a production network
    pub fn is_production(&self) -> bool;
}
```

---

## MoltClient

Main client for MOLT token operations.

```rust
pub struct MoltClient {
    network: Network,
    // Internal RPC client
}

impl MoltClient {
    /// Create new client
    pub fn new(network: Network) -> Result<Self, MoltError>;
    
    /// Get token balance
    pub async fn get_balance(&self, address: &Address) -> Result<Amount, MoltError>;
    
    /// Transfer tokens
    pub async fn transfer(
        &self,
        from: &Wallet,
        to: &Address,
        amount: Amount,
    ) -> Result<TransactionId, MoltError>;
    
    /// Get transaction status
    pub async fn get_transaction(
        &self,
        tx_id: &TransactionId,
    ) -> Result<Transaction, MoltError>;
    
    /// Create escrow for compute job
    pub async fn create_escrow(
        &self,
        payer: &Wallet,
        provider: &Address,
        amount: Amount,
        job_id: &str,
    ) -> Result<Escrow, MoltError>;
    
    /// Release escrow to provider
    pub async fn release_escrow(
        &self,
        escrow_id: &EscrowId,
        authority: &Wallet,
    ) -> Result<TransactionId, MoltError>;
    
    /// Refund escrow to payer
    pub async fn refund_escrow(
        &self,
        escrow_id: &EscrowId,
        authority: &Wallet,
    ) -> Result<TransactionId, MoltError>;
    
    /// Request airdrop (devnet only)
    pub async fn request_airdrop(
        &self,
        address: &Address,
        amount: Amount,
    ) -> Result<TransactionId, MoltError>;
}
```

---

## Transactions

### `TransactionId`

```rust
pub struct TransactionId(String);

impl TransactionId {
    /// Get as base58 string
    pub fn as_str(&self) -> &str;
    
    /// Create explorer URL
    pub fn explorer_url(&self, network: &Network) -> String;
}
```

### `Transaction`

```rust
pub struct Transaction {
    pub id: TransactionId,
    pub status: TransactionStatus,
    pub tx_type: TransactionType,
    pub amount: Amount,
    pub from: Address,
    pub to: Address,
    pub timestamp: DateTime<Utc>,
    pub fee: Amount,
    pub memo: Option<String>,
}

pub enum TransactionStatus {
    Pending,
    Confirmed { slot: u64 },
    Failed { error: String },
}

pub enum TransactionType {
    Transfer,
    EscrowCreate,
    EscrowRelease,
    EscrowRefund,
    Airdrop,
}
```

---

## Escrow

Secure compute job payments.

### `Escrow`

```rust
pub struct Escrow {
    pub id: EscrowId,
    pub state: EscrowState,
    pub payer: Address,
    pub provider: Address,
    pub amount: Amount,
    pub job_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

pub enum EscrowState {
    /// Funds locked, awaiting completion
    Locked,
    /// Released to provider
    Released,
    /// Refunded to payer
    Refunded,
    /// Expired (can be refunded)
    Expired,
    /// Disputed
    Disputed,
}
```

### Escrow Flow

```rust
use molt_token::{MoltClient, Network, Wallet, Amount};

async fn escrow_example() -> Result<(), MoltError> {
    let client = MoltClient::new(Network::Devnet)?;
    let buyer = Wallet::from_file("buyer.json")?;
    let provider_addr = Address::from_str("Provider...")?;
    
    // 1. Create escrow for job
    let escrow = client.create_escrow(
        &buyer,
        &provider_addr,
        Amount::from_molt(100),
        "job-12345",
    ).await?;
    
    println!("Escrow created: {}", escrow.id);
    
    // 2. Job completes successfully...
    
    // 3. Release to provider
    let tx = client.release_escrow(&escrow.id, &buyer).await?;
    println!("Released: {}", tx);
    
    Ok(())
}
```

---

## Error Handling

```rust
pub enum MoltError {
    /// Network/RPC error
    Network(String),
    /// Insufficient balance
    InsufficientBalance { required: Amount, available: Amount },
    /// Invalid address format
    InvalidAddress(String),
    /// Transaction failed
    TransactionFailed { tx_id: TransactionId, error: String },
    /// Escrow error
    EscrowError(String),
    /// Wallet error
    WalletError(String),
    /// Serialization error
    Serialization(String),
}
```

---

## Examples

### Check Balance

```rust
use molt_token::{MoltClient, Network, Address};

let client = MoltClient::new(Network::Mainnet)?;
let address = Address::from_str("7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU")?;

let balance = client.get_balance(&address).await?;
println!("Balance: {} MOLT", balance.as_f64());
```

### Send Tokens

```rust
use molt_token::{MoltClient, Network, Wallet, Address, Amount};

let client = MoltClient::new(Network::Devnet)?;
let wallet = Wallet::from_file("my-wallet.json")?;
let recipient = Address::from_str("RecipientAddress...")?;

let tx = client.transfer(
    &wallet,
    &recipient,
    Amount::from_molt(50),
).await?;

println!("Transaction: {}", tx);
println!("Explorer: {}", tx.explorer_url(&Network::Devnet));
```

### Airdrop (Devnet)

```rust
use molt_token::{MoltClient, Network, Wallet, Amount};

let client = MoltClient::new(Network::Devnet)?;
let wallet = Wallet::generate();

// Request free tokens on devnet
let tx = client.request_airdrop(
    &wallet.address(),
    Amount::from_molt(1000),
).await?;

println!("Airdrop received: {}", tx);
```

---

## Security Considerations

1. **Never commit wallet files** — Add `*.json` to `.gitignore`
2. **Use hardware wallets** — For mainnet operations
3. **Validate addresses** — Always validate before sending
4. **Check network** — Verify you're on the intended network
5. **Escrow timeouts** — Set reasonable expiration times

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `MOLT_NETWORK` | Default network (devnet, mainnet) |
| `MOLT_RPC_URL` | Custom RPC endpoint |
| `MOLT_WALLET_PATH` | Default wallet file path |
