//! # claw-gateway-server
//!
//! WebSocket gateway server for Clawbernetes node fleet communication.
//!
//! This crate provides the WebSocket server that `clawnode` instances connect to
//! for registration, heartbeats, metrics reporting, and workload management.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────┐     WebSocket      ┌─────────────────┐
//! │   clawnode   │◄──────────────────►│  GatewayServer  │
//! │   instance   │                    │                 │
//! └──────────────┘                    │  ┌───────────┐  │
//!                                     │  │ NodeSess- │  │
//! ┌──────────────┐                    │  │   ion     │  │
//! │   clawnode   │◄──────────────────►│  └───────────┘  │
//! │   instance   │                    │                 │
//! └──────────────┘                    │  ┌───────────┐  │
//!                                     │  │ NodeReg-  │  │
//!                                     │  │  istry    │  │
//!                                     │  └───────────┘  │
//!                                     │                 │
//!                                     │  ┌───────────┐  │
//!                                     │  │ Workload- │  │
//!                                     │  │  Manager  │  │
//!                                     │  └───────────┘  │
//!                                     └─────────────────┘
//! ```
//!
//! ## Example Usage
//!
//! ```rust,no_run
//! use claw_gateway_server::{GatewayServer, ServerConfig};
//! use std::net::SocketAddr;
//!
//! #[tokio::main]
//! async fn main() {
//!     let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
//!     let config = ServerConfig::new(addr)
//!         .with_max_connections(1000);
//!     
//!     let mut server = GatewayServer::new(config);
//!     server.serve(addr).await.unwrap();
//! }
//! ```
//!
//! ## Message Protocol
//!
//! The server handles the following message types from nodes (see `claw-proto`):
//!
//! - **Register**: Node announces itself with capabilities
//! - **Heartbeat**: Periodic health check
//! - **Metrics**: GPU and system metrics
//! - `WorkloadUpdate`: State changes for running workloads
//! - `WorkloadLogs`: Log output from workloads
//!
//! The server sends these messages to nodes:
//!
//! - **Registered**: Acknowledgment with heartbeat/metrics intervals
//! - `HeartbeatAck`: Response to heartbeat
//! - `StartWorkload`: Command to start a workload
//! - `StopWorkload`: Command to stop a workload
//! - `RequestMetrics`: Request immediate metrics report
//! - **Error**: Error notification

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli_handler;
pub mod config;
pub mod error;
pub mod handlers;
pub mod server;
pub mod session;

// Re-export main types
pub use config::{
    ServerConfig, WebSocketConfig, DEFAULT_MAX_FRAME_SIZE, DEFAULT_MAX_MESSAGE_SIZE,
    DEFAULT_MAX_VIOLATIONS,
};
pub use error::{ServerError, ServerResult};
pub use handlers::{
    handle_heartbeat, handle_metrics, handle_register, handle_workload_update, route_message,
};
pub use server::GatewayServer;
pub use session::{
    gateway_msg_to_ws, process_ws_message, process_ws_message_with_limits, run_session,
    session_channel, validate_message_size, ws_message_size, NodeSession, SessionReceiver,
    SessionSender, SessionState, ViolationTracker,
};
