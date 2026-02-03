//! Error types for molt-p2p.

use thiserror::Error;

/// Errors that can occur in P2P operations.
#[derive(Debug, Error)]
pub enum P2pError {
    /// Connection failed.
    #[error("connection failed: {0}")]
    Connection(String),

    /// Discovery failed.
    #[error("discovery failed: {0}")]
    Discovery(String),

    /// Gossip protocol error.
    #[error("gossip error: {0}")]
    Gossip(String),

    /// Peer not found.
    #[error("peer not found: {0}")]
    PeerNotFound(String),

    /// Protocol error.
    #[error("protocol error: {0}")]
    Protocol(String),

    /// IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
