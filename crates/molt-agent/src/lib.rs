//! # molt-agent
//!
//! Autonomous agent logic for MOLT network participation.
//!
//! This crate provides:
//!
//! - **Autonomy modes** — [`AutonomyMode`] for Conservative, Moderate, or Aggressive behavior
//! - **Provider agent** — [`ProviderState`] for advertising capacity and accepting jobs
//! - **Buyer agent** — [`BuyerState`] for discovering providers and submitting jobs
//! - **Negotiation logic** — [`negotiate`] for bidding, counter-offers, and provider selection
//!
//! ## Example
//!
//! ```rust
//! use molt_agent::autonomy::{AutonomyMode, JobDecision};
//! use molt_agent::provider::{ProviderState, JobSpec, ProviderPolicy, evaluate_job};
//!
//! // Create provider state
//! let state = ProviderState::new(1000, 10);
//!
//! // Create a job specification
//! let job = JobSpec::new(100, 200, 3600, 80);
//!
//! // Evaluate job with moderate autonomy
//! let policy = ProviderPolicy::for_mode(AutonomyMode::Moderate);
//! let decision = evaluate_job(&job, AutonomyMode::Moderate, &policy);
//!
//! assert!(decision.is_accept());
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod autonomy;
pub mod buyer;
pub mod error;
pub mod negotiation;
pub mod provider;
pub mod strategy;

pub use error::AgentError;

// Re-exports for convenience
pub use autonomy::{AutonomyMode, Decision, DecisionThresholds, JobDecision};
pub use buyer::{BuyerState, BuyerError, BuyerPolicy, JobRequirements, ProviderOffer};
pub use buyer::{evaluate_offer, score_offer, select_best_offer};
pub use negotiation::{Bid, NegotiationJob, NegotiationState, NegotiationStrategy, SelectedBid};
pub use negotiation::{negotiate, negotiate_at, NegotiationError, NegotiationPhase};
pub use provider::{ProviderState, ProviderError, ProviderPolicy, JobSpec};
pub use provider::{evaluate_job, evaluate_job_with_state, ProviderDecisionMaker};
