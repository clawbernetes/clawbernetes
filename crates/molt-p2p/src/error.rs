//! Error types for molt-p2p.

use thiserror::Error;

use crate::protocol::PeerId;

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

    /// Peer is sending messages too quickly.
    #[error("peer {peer_id} is rate limited")]
    RateLimited {
        /// The peer that exceeded the rate limit.
        peer_id: PeerId,
    },

    /// Peer has been temporarily banned due to repeated violations.
    #[error("peer {peer_id} is temporarily banned")]
    PeerBanned {
        /// The peer that was banned.
        peer_id: PeerId,
    },
}
