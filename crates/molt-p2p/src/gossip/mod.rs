//! Gossip protocol for capacity announcements.
//!
//! This module handles broadcasting and receiving capacity announcements
//! from peers in the network using epidemic/gossip-style propagation.
//!
//! ## Architecture
//!
//! - [`CapacityAnnouncement`]: Signed announcement of compute capacity
//! - [`GossipMessage`]: Wire protocol messages for gossip
//! - [`GossipNode`]: Trait for nodes participating in gossip
//! - [`GossipBroadcaster`]: Fanout-based epidemic broadcast implementation

mod announcement;
mod broadcast;
mod message;
mod node;
mod rate_limit;

pub use announcement::{CapacityAnnouncement, GpuInfo, Pricing};
pub use broadcast::{BroadcastConfig, BroadcastResult, GossipBroadcaster};
pub use message::{GossipMessage, GossipQuery, MessageId, QueryFilter};
pub use node::{GossipEvent, GossipNode, LocalGossipNode, NodeState};
pub use rate_limit::{RateLimitConfig, RateLimitResult, RateLimiter, RateLimiterStats};
