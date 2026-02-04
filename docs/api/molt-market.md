# molt-market API Reference

Decentralized orderbook and settlement for GPU compute marketplace.

## Overview

`molt-market` provides the marketplace infrastructure for MOLT:

- **Orderbook** — Matching buyers and providers
- **Job Orders** — Compute requirements specification
- **Capacity Offers** — Provider listings
- **Escrow** — Secure payment handling
- **Settlement** — Job completion and payment release

## Installation

```toml
[dependencies]
molt-market = { path = "../molt-market" }
```

## Quick Start

```rust
use molt_market::{OrderBook, JobOrder, CapacityOffer, PaymentService};

// Create orderbook
let mut orderbook = OrderBook::new();

// Provider lists capacity
let offer = CapacityOffer {
    provider_id: "provider-123".to_string(),
    gpu_capacity: GpuCapacity {
        gpu_count: 8,
        gpu_model: "H100".to_string(),
        vram_gb: 80,
    },
    price_per_hour: Amount::from_molt(10),
    available_hours: 24,
    ..Default::default()
};
orderbook.add_offer(offer)?;

// Buyer submits job
let order = JobOrder {
    buyer_id: "buyer-456".to_string(),
    requirements: JobRequirements {
        min_gpu_count: 4,
        min_vram_gb: 40,
        max_price_per_hour: Amount::from_molt(15),
        estimated_hours: 8,
    },
    ..Default::default()
};

// Match and execute
let matches = orderbook.match_order(&order)?;
```

---

## OrderBook

Central matching engine for compute orders.

```rust
pub struct OrderBook {
    offers: Vec<CapacityOffer>,
    orders: Vec<JobOrder>,
}

impl OrderBook {
    /// Create empty orderbook
    pub fn new() -> Self;
    
    /// Add a capacity offer
    pub fn add_offer(&mut self, offer: CapacityOffer) -> Result<OfferId, MarketError>;
    
    /// Remove an offer
    pub fn remove_offer(&mut self, offer_id: &OfferId) -> Result<(), MarketError>;
    
    /// Submit a job order
    pub fn submit_order(&mut self, order: JobOrder) -> Result<OrderId, MarketError>;
    
    /// Cancel an order
    pub fn cancel_order(&mut self, order_id: &OrderId) -> Result<(), MarketError>;
    
    /// Match an order against offers
    pub fn match_order(&self, order: &JobOrder) -> Result<Vec<OrderMatch>, MarketError>;
    
    /// Get best offers for requirements
    pub fn best_offers(
        &self,
        requirements: &JobRequirements,
        limit: usize,
    ) -> Vec<&CapacityOffer>;
    
    /// Get all active offers
    pub fn active_offers(&self) -> &[CapacityOffer];
    
    /// Get all pending orders
    pub fn pending_orders(&self) -> &[JobOrder];
}
```

---

## Job Orders

### `JobOrder`

Buyer's compute request.

```rust
pub struct JobOrder {
    /// Unique order ID
    pub id: OrderId,
    /// Buyer identifier
    pub buyer_id: String,
    /// Compute requirements
    pub requirements: JobRequirements,
    /// Maximum total budget
    pub max_budget: Amount,
    /// Order creation time
    pub created_at: DateTime<Utc>,
    /// Order expiration
    pub expires_at: DateTime<Utc>,
    /// Order status
    pub status: OrderStatus,
    /// Priority level
    pub priority: Priority,
}
```

### `JobRequirements`

```rust
pub struct JobRequirements {
    /// Minimum GPU count
    pub min_gpu_count: u32,
    /// Minimum VRAM per GPU (GB)
    pub min_vram_gb: u32,
    /// Preferred GPU models (optional)
    pub preferred_models: Vec<String>,
    /// Maximum price per GPU-hour
    pub max_price_per_hour: Amount,
    /// Estimated duration (hours)
    pub estimated_hours: u32,
    /// Required features
    pub required_features: Vec<Feature>,
    /// Geographic requirements
    pub location: Option<Location>,
}

pub enum Feature {
    NvLink,
    Infiniband,
    FP8,
    HighBandwidthMemory,
    DedicatedHost,
}
```

### Example: Create Job Order

```rust
use molt_market::{JobOrder, JobRequirements, Priority, Amount};
use chrono::{Utc, Duration};

let order = JobOrder {
    id: OrderId::new(),
    buyer_id: "buyer-123".to_string(),
    requirements: JobRequirements {
        min_gpu_count: 8,
        min_vram_gb: 80,
        preferred_models: vec!["H100".to_string()],
        max_price_per_hour: Amount::from_molt(12),
        estimated_hours: 24,
        required_features: vec![Feature::NvLink],
        location: None,
    },
    max_budget: Amount::from_molt(300),
    created_at: Utc::now(),
    expires_at: Utc::now() + Duration::hours(1),
    status: OrderStatus::Pending,
    priority: Priority::Normal,
};
```

---

## Capacity Offers

### `CapacityOffer`

Provider's available compute resources.

```rust
pub struct CapacityOffer {
    /// Unique offer ID
    pub id: OfferId,
    /// Provider identifier
    pub provider_id: String,
    /// GPU capacity details
    pub gpu_capacity: GpuCapacity,
    /// Price per GPU-hour
    pub price_per_hour: Amount,
    /// Maximum hours available
    pub available_hours: u32,
    /// Minimum commitment (hours)
    pub min_commitment_hours: u32,
    /// Offer creation time
    pub created_at: DateTime<Utc>,
    /// Offer expiration
    pub expires_at: DateTime<Utc>,
    /// Offer status
    pub status: OfferStatus,
    /// Available features
    pub features: Vec<Feature>,
    /// Geographic location
    pub location: Option<Location>,
}
```

### `GpuCapacity`

```rust
pub struct GpuCapacity {
    /// Number of GPUs available
    pub gpu_count: u32,
    /// GPU model name
    pub gpu_model: String,
    /// VRAM per GPU (GB)
    pub vram_gb: u32,
    /// Compute capability
    pub compute_capability: Option<String>,
    /// Memory bandwidth (GB/s)
    pub memory_bandwidth: Option<u32>,
    /// Current utilization (0-100)
    pub current_utilization: Option<u32>,
}
```

### Example: Create Capacity Offer

```rust
use molt_market::{CapacityOffer, GpuCapacity, Feature, Amount};
use chrono::{Utc, Duration};

let offer = CapacityOffer {
    id: OfferId::new(),
    provider_id: "provider-456".to_string(),
    gpu_capacity: GpuCapacity {
        gpu_count: 8,
        gpu_model: "NVIDIA H100 80GB".to_string(),
        vram_gb: 80,
        compute_capability: Some("9.0".to_string()),
        memory_bandwidth: Some(3350),
        current_utilization: Some(0),
    },
    price_per_hour: Amount::from_molt(10),
    available_hours: 168, // 1 week
    min_commitment_hours: 1,
    created_at: Utc::now(),
    expires_at: Utc::now() + Duration::days(7),
    status: OfferStatus::Active,
    features: vec![Feature::NvLink, Feature::Infiniband],
    location: Some(Location {
        region: "us-east-1".to_string(),
        country: "US".to_string(),
    }),
};
```

---

## Order Matching

### `OrderMatch`

Result of matching an order to an offer.

```rust
pub struct OrderMatch {
    /// The matched order
    pub order_id: OrderId,
    /// The matched offer
    pub offer_id: OfferId,
    /// Provider ID
    pub provider_id: String,
    /// Allocated GPU count
    pub gpu_count: u32,
    /// Agreed price per hour
    pub price_per_hour: Amount,
    /// Allocated hours
    pub hours: u32,
    /// Total cost
    pub total_cost: Amount,
    /// Match score (0.0-1.0)
    pub score: f64,
}
```

### Matching Algorithm

```rust
use molt_market::{OrderBook, JobOrder};

let orderbook = OrderBook::new();
// ... add offers ...

let order = JobOrder { /* ... */ };

// Get all matches sorted by score
let matches = orderbook.match_order(&order)?;

for m in matches {
    println!("Provider: {} | GPUs: {} | ${}/hr | Score: {:.2}",
        m.provider_id, m.gpu_count, m.price_per_hour.as_f64(), m.score);
}

// Take best match
if let Some(best) = matches.first() {
    println!("Best match: {} for {} MOLT total",
        best.provider_id, best.total_cost.as_f64());
}
```

---

## Payment Service

Bridge between marketplace and on-chain payments.

```rust
pub struct PaymentService {
    molt_client: MoltClient,
}

impl PaymentService {
    /// Create new payment service
    pub fn new(molt_client: MoltClient) -> Self;
    
    /// Create escrow for matched order
    pub async fn create_job_escrow(
        &self,
        match_result: &OrderMatch,
        buyer_wallet: &Wallet,
    ) -> Result<EscrowAccount, MarketError>;
    
    /// Release payment on job completion
    pub async fn release_payment(
        &self,
        escrow: &EscrowAccount,
        proof: &ExecutionProof,
    ) -> Result<TransactionId, MarketError>;
    
    /// Refund on job failure
    pub async fn refund_payment(
        &self,
        escrow: &EscrowAccount,
        reason: &str,
    ) -> Result<TransactionId, MarketError>;
    
    /// Handle dispute
    pub async fn handle_dispute(
        &self,
        escrow: &EscrowAccount,
        dispute: &Dispute,
    ) -> Result<DisputeResolution, MarketError>;
}
```

### `EscrowAccount`

```rust
pub struct EscrowAccount {
    pub id: EscrowId,
    pub state: EscrowState,
    pub order_id: OrderId,
    pub offer_id: OfferId,
    pub buyer: Address,
    pub provider: Address,
    pub amount: Amount,
    pub created_at: DateTime<Utc>,
    pub locked_until: DateTime<Utc>,
    pub released_at: Option<DateTime<Utc>>,
    pub transaction_id: Option<TransactionId>,
}

pub enum EscrowState {
    Created,
    Locked,
    Released,
    Refunded,
    Disputed,
}
```

---

## Settlement

### `JobSettlementInput`

Input for settling a completed job.

```rust
pub struct JobSettlementInput {
    /// Escrow account
    pub escrow_id: EscrowId,
    /// Actual GPU-hours used
    pub actual_hours: f64,
    /// Execution proof
    pub proof: ExecutionProof,
    /// Provider attestation
    pub provider_attestation: Attestation,
    /// Buyer confirmation (optional)
    pub buyer_confirmation: Option<Confirmation>,
}
```

### `SettlementResult`

```rust
pub struct SettlementResult {
    /// Settlement status
    pub status: SettlementStatus,
    /// Amount paid to provider
    pub provider_payment: Amount,
    /// Amount refunded to buyer
    pub buyer_refund: Amount,
    /// Platform fee
    pub platform_fee: Amount,
    /// Transaction IDs
    pub transactions: Vec<TransactionId>,
}

pub enum SettlementStatus {
    Success,
    PartialRefund { reason: String },
    FullRefund { reason: String },
    Disputed { dispute_id: DisputeId },
}
```

### Settlement Function

```rust
use molt_market::{settle_job, JobSettlementInput, ExecutionProof};

let input = JobSettlementInput {
    escrow_id: escrow.id.clone(),
    actual_hours: 23.5,
    proof: ExecutionProof { /* ... */ },
    provider_attestation: attestation,
    buyer_confirmation: Some(confirmation),
};

let result = settle_job(&input)?;

match result.status {
    SettlementStatus::Success => {
        println!("Paid provider: {} MOLT", result.provider_payment.as_f64());
    }
    SettlementStatus::PartialRefund { reason } => {
        println!("Partial refund: {}", reason);
        println!("  Provider: {} MOLT", result.provider_payment.as_f64());
        println!("  Refund: {} MOLT", result.buyer_refund.as_f64());
    }
    _ => { /* handle other cases */ }
}
```

---

## Error Handling

```rust
pub enum MarketError {
    /// Order not found
    OrderNotFound(OrderId),
    /// Offer not found
    OfferNotFound(OfferId),
    /// No matching offers
    NoMatchingOffers,
    /// Insufficient capacity
    InsufficientCapacity { required: u32, available: u32 },
    /// Price exceeds budget
    PriceExceedsBudget { price: Amount, budget: Amount },
    /// Escrow error
    EscrowError(String),
    /// Payment failed
    PaymentFailed(String),
    /// Invalid state transition
    InvalidStateTransition { from: String, to: String },
    /// Expired
    Expired(String),
}
```

---

## Order Status Flow

```
Pending → Matched → Escrowed → Running → Completed → Settled
                                   ↓
                               Failed → Refunded
                                   ↓
                               Disputed → Resolved
```

---

## Example: Full Flow

```rust
use molt_market::{OrderBook, JobOrder, PaymentService, settle_job};

async fn marketplace_flow() -> Result<(), MarketError> {
    let mut orderbook = OrderBook::new();
    let payment = PaymentService::new(MoltClient::new(Network::Mainnet)?);
    
    // 1. Provider lists capacity
    let offer = CapacityOffer { /* ... */ };
    orderbook.add_offer(offer)?;
    
    // 2. Buyer submits order
    let order = JobOrder { /* ... */ };
    let matches = orderbook.match_order(&order)?;
    let best_match = matches.first().ok_or(MarketError::NoMatchingOffers)?;
    
    // 3. Create escrow
    let buyer_wallet = Wallet::from_file("buyer.json")?;
    let escrow = payment.create_job_escrow(&best_match, &buyer_wallet).await?;
    
    // 4. Job executes on provider...
    
    // 5. Settle
    let result = settle_job(&JobSettlementInput {
        escrow_id: escrow.id,
        actual_hours: 24.0,
        proof: execution_proof,
        provider_attestation: attestation,
        buyer_confirmation: Some(confirmation),
    })?;
    
    println!("Settlement: {:?}", result.status);
    
    Ok(())
}
```
