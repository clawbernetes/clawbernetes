//! Error types for molt-agent.

use thiserror::Error;

/// Errors that can occur in agent operations.
#[derive(Debug, Error)]
pub enum AgentError {
    /// Policy rejected the action.
    #[error("policy rejected: {0}")]
    PolicyRejected(String),

    /// Negotiation failed.
    #[error("negotiation failed: {0}")]
    NegotiationFailed(String),

    /// Job execution failed.
    #[error("job execution failed: {0}")]
    ExecutionFailed(String),

    /// Provider not available.
    #[error("provider not available: {0}")]
    ProviderUnavailable(String),

    /// Market error.
    #[error("market error: {0}")]
    Market(String),

    /// P2P error.
    #[error("p2p error: {0}")]
    P2p(String),
}
