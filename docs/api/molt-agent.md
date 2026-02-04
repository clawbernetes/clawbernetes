# molt-agent API Reference

Autonomous provider and buyer agents for the MOLT marketplace.

## Overview

`molt-agent` provides intelligent automation for marketplace participation:

- **Provider Agent** — Manages capacity listing, job acceptance, and earnings
- **Buyer Agent** — Handles job submission, provider selection, and budget
- **Autonomy Levels** — User-selectable automation from manual to full autopilot
- **Strategies** — Configurable pricing and selection algorithms

## Installation

```toml
[dependencies]
molt-agent = { path = "../molt-agent" }
```

## Quick Start

```rust
use molt_agent::{ProviderAgent, BuyerAgent, AutonomyLevel, ProviderConfig};

// Create provider agent
let config = ProviderConfig {
    autonomy: AutonomyLevel::Moderate,
    min_price_per_hour: Amount::from_molt(5),
    max_concurrent_jobs: 4,
    ..Default::default()
};
let provider = ProviderAgent::new(config, wallet, molt_client).await?;

// Start accepting jobs
provider.start().await?;
```

---

## Autonomy Levels

Control how much the agent decides autonomously.

```rust
pub enum AutonomyLevel {
    /// Manual approval for every action
    Conservative,
    /// Agent follows policies, user approves large transactions
    Moderate,
    /// Full autopilot for maximum efficiency
    Aggressive,
}
```

### Conservative Mode

- Every job requires manual approval
- Price changes require confirmation
- Ideal for high-value workloads or new users

### Moderate Mode

- Agent follows configured policies
- Auto-approves jobs within policy bounds
- Alerts on unusual activity
- Recommended for most users

### Aggressive Mode

- Full automation for maximum earnings
- Agent optimizes pricing dynamically
- Minimal user intervention
- Best for experienced users with stable setups

---

## Provider Agent

### `ProviderAgent`

```rust
pub struct ProviderAgent {
    config: ProviderConfig,
    wallet: Wallet,
    client: MoltClient,
    state: ProviderState,
}

impl ProviderAgent {
    /// Create new provider agent
    pub async fn new(
        config: ProviderConfig,
        wallet: Wallet,
        client: MoltClient,
    ) -> Result<Self, AgentError>;
    
    /// Start the agent
    pub async fn start(&self) -> Result<(), AgentError>;
    
    /// Stop the agent
    pub async fn stop(&self) -> Result<(), AgentError>;
    
    /// Get current state
    pub fn state(&self) -> &ProviderState;
    
    /// List current capacity
    pub async fn list_capacity(&self, capacity: GpuCapacity) -> Result<OfferId, AgentError>;
    
    /// Update pricing
    pub async fn update_pricing(&self, price: Amount) -> Result<(), AgentError>;
    
    /// Get earnings summary
    pub async fn earnings(&self) -> Result<EarningsSummary, AgentError>;
}
```

### `ProviderConfig`

```rust
pub struct ProviderConfig {
    /// Autonomy level
    pub autonomy: AutonomyLevel,
    /// Minimum price per GPU-hour
    pub min_price_per_hour: Amount,
    /// Maximum price per GPU-hour
    pub max_price_per_hour: Option<Amount>,
    /// Maximum concurrent jobs
    pub max_concurrent_jobs: u32,
    /// Maximum job duration (hours)
    pub max_job_duration_hours: u32,
    /// Allowed job types
    pub allowed_job_types: Vec<JobType>,
    /// Blocked buyers (by address)
    pub blocked_buyers: Vec<Address>,
    /// Pricing strategy
    pub pricing_strategy: PricingStrategy,
}
```

### `ProviderState`

```rust
pub struct ProviderState {
    pub status: ProviderStatus,
    pub active_jobs: Vec<ActiveJob>,
    pub pending_approvals: Vec<PendingApproval>,
    pub total_earnings: Amount,
    pub available_capacity: GpuCapacity,
}

pub enum ProviderStatus {
    Offline,
    Online { since: DateTime<Utc> },
    Busy { jobs: u32 },
    Draining { reason: String },
}
```

---

## Buyer Agent

### `BuyerAgent`

```rust
pub struct BuyerAgent {
    config: BuyerConfig,
    wallet: Wallet,
    client: MoltClient,
}

impl BuyerAgent {
    /// Create new buyer agent
    pub async fn new(
        config: BuyerConfig,
        wallet: Wallet,
        client: MoltClient,
    ) -> Result<Self, AgentError>;
    
    /// Submit a job
    pub async fn submit_job(&self, spec: JobSpec) -> Result<JobSubmission, AgentError>;
    
    /// Find providers for requirements
    pub async fn find_providers(
        &self,
        requirements: &JobRequirements,
    ) -> Result<Vec<ProviderMatch>, AgentError>;
    
    /// Get job status
    pub async fn job_status(&self, job_id: &JobId) -> Result<JobStatus, AgentError>;
    
    /// Cancel a job
    pub async fn cancel_job(&self, job_id: &JobId) -> Result<(), AgentError>;
    
    /// Get spending summary
    pub async fn spending(&self) -> Result<SpendingSummary, AgentError>;
}
```

### `BuyerConfig`

```rust
pub struct BuyerConfig {
    /// Autonomy level
    pub autonomy: AutonomyLevel,
    /// Maximum price per GPU-hour
    pub max_price_per_hour: Amount,
    /// Total budget limit
    pub budget_limit: Option<Amount>,
    /// Preferred providers
    pub preferred_providers: Vec<Address>,
    /// Blocked providers
    pub blocked_providers: Vec<Address>,
    /// Provider selection strategy
    pub selection_strategy: SelectionStrategy,
    /// Auto-retry on failure
    pub auto_retry: bool,
}
```

---

## Strategies

### Pricing Strategy (Provider)

```rust
pub enum PricingStrategy {
    /// Fixed price
    Fixed { price: Amount },
    /// Time-based (peak/off-peak)
    TimeBased {
        peak_price: Amount,
        off_peak_price: Amount,
        peak_hours: Vec<u32>,
    },
    /// Demand-based (adjust based on utilization)
    DemandBased {
        base_price: Amount,
        utilization_multiplier: f64,
    },
    /// Market-following
    MarketBased {
        target_percentile: u32, // e.g., 50 for median
        min_price: Amount,
        max_price: Amount,
    },
}
```

### Selection Strategy (Buyer)

```rust
pub enum SelectionStrategy {
    /// Lowest price
    LowestPrice,
    /// Highest reputation
    HighestReputation,
    /// Balanced (price + reputation)
    Balanced { price_weight: f64, reputation_weight: f64 },
    /// Fastest availability
    FastestAvailable,
    /// Preferred providers first
    PreferredFirst,
}
```

---

## Negotiation

### Automated Negotiation

```rust
use molt_agent::{Negotiator, NegotiationConfig};

let negotiator = Negotiator::new(NegotiationConfig {
    max_rounds: 5,
    initial_discount: 0.10,  // 10% below ask
    step_size: 0.05,         // 5% steps
    walk_away_threshold: 0.95, // 95% of budget max
});

let result = negotiator.negotiate(&offer, &budget).await?;

match result {
    NegotiationResult::Agreed { price } => {
        println!("Agreed at {} MOLT/hr", price.as_f64());
    }
    NegotiationResult::WalkedAway => {
        println!("Could not reach agreement");
    }
}
```

---

## Events & Callbacks

### Provider Events

```rust
pub enum ProviderEvent {
    JobReceived { job_id: JobId, requirements: JobRequirements },
    JobApprovalNeeded { job_id: JobId, details: JobDetails },
    JobStarted { job_id: JobId },
    JobCompleted { job_id: JobId, earnings: Amount },
    JobFailed { job_id: JobId, error: String },
    EarningsReceived { amount: Amount, transaction: TransactionId },
}

// Handle events
provider.on_event(|event| {
    match event {
        ProviderEvent::JobApprovalNeeded { job_id, details } => {
            // Notify user for approval
        }
        ProviderEvent::EarningsReceived { amount, .. } => {
            println!("Received {} MOLT", amount.as_f64());
        }
        _ => {}
    }
}).await;
```

### Buyer Events

```rust
pub enum BuyerEvent {
    JobSubmitted { job_id: JobId },
    ProviderFound { job_id: JobId, provider: Address },
    JobStarted { job_id: JobId },
    JobCompleted { job_id: JobId, cost: Amount },
    JobFailed { job_id: JobId, error: String, refund: Amount },
}
```

---

## Example: Provider Agent

```rust
use molt_agent::{ProviderAgent, ProviderConfig, AutonomyLevel, PricingStrategy};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wallet = Wallet::from_file("provider.json")?;
    let client = MoltClient::new(Network::Mainnet)?;
    
    let config = ProviderConfig {
        autonomy: AutonomyLevel::Moderate,
        min_price_per_hour: Amount::from_molt(8),
        max_concurrent_jobs: 2,
        max_job_duration_hours: 48,
        pricing_strategy: PricingStrategy::DemandBased {
            base_price: Amount::from_molt(10),
            utilization_multiplier: 1.5,
        },
        ..Default::default()
    };
    
    let agent = ProviderAgent::new(config, wallet, client).await?;
    
    // List capacity
    agent.list_capacity(GpuCapacity {
        gpu_count: 8,
        gpu_model: "H100".to_string(),
        vram_gb: 80,
        ..Default::default()
    }).await?;
    
    // Run until shutdown
    agent.start().await?;
    
    Ok(())
}
```

---

## Example: Buyer Agent

```rust
use molt_agent::{BuyerAgent, BuyerConfig, AutonomyLevel, JobSpec};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wallet = Wallet::from_file("buyer.json")?;
    let client = MoltClient::new(Network::Mainnet)?;
    
    let config = BuyerConfig {
        autonomy: AutonomyLevel::Moderate,
        max_price_per_hour: Amount::from_molt(15),
        budget_limit: Some(Amount::from_molt(500)),
        selection_strategy: SelectionStrategy::Balanced {
            price_weight: 0.6,
            reputation_weight: 0.4,
        },
        auto_retry: true,
        ..Default::default()
    };
    
    let agent = BuyerAgent::new(config, wallet, client).await?;
    
    // Submit a job
    let submission = agent.submit_job(JobSpec {
        name: "llm-training".to_string(),
        requirements: JobRequirements {
            min_gpu_count: 8,
            min_vram_gb: 80,
            max_price_per_hour: Amount::from_molt(15),
            estimated_hours: 24,
            ..Default::default()
        },
        container_image: "training/llm:latest".to_string(),
        ..Default::default()
    }).await?;
    
    println!("Job submitted: {}", submission.job_id);
    println!("Provider: {}", submission.provider);
    println!("Total cost: {} MOLT", submission.total_cost.as_f64());
    
    Ok(())
}
```

---

## Error Handling

```rust
pub enum AgentError {
    /// Configuration invalid
    InvalidConfig(String),
    /// Wallet error
    WalletError(String),
    /// Network error
    NetworkError(String),
    /// No providers available
    NoProvidersAvailable,
    /// Insufficient funds
    InsufficientFunds { required: Amount, available: Amount },
    /// Job not found
    JobNotFound(JobId),
    /// Approval timeout
    ApprovalTimeout,
    /// Policy violation
    PolicyViolation(String),
}
```
