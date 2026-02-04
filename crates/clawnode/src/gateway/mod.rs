//! Gateway WebSocket client.
//!
//! Manages the connection between a node and the Clawbernetes gateway,
//! including automatic reconnection with exponential backoff.

#![allow(dead_code)]

mod auto_reconnect;
mod client;
mod events;
mod handle;
mod heartbeat;
mod reconnect;
mod state;

// Re-export public types
pub use auto_reconnect::AutoReconnectClient;
pub use client::GatewayClient;
pub use events::{AutoReconnectEvent, GatewayEvent};
pub use handle::GatewayHandle;
pub use heartbeat::{start_heartbeat_task, HeartbeatConfig, HeartbeatHandle};
pub use reconnect::{calculate_backoff, reconnect_with_backoff, ReconnectConfig};
pub use state::{AtomicConnectionState, ConnectionState};
