//! # claw-dashboard
//!
//! Web dashboard API for Clawbernetes cluster management and monitoring.
//!
//! This crate provides a REST API and WebSocket/SSE endpoints for real-time
//! cluster monitoring, built on top of the axum HTTP framework.
//!
//! ## Features
//!
//! - **REST API**: Query cluster status, nodes, workloads, and metrics
//! - **WebSocket**: Real-time updates stream for live monitoring
//! - **Server-Sent Events**: Alternative streaming for simpler clients
//! - **Integrated with Gateway**: Shares state with `claw-gateway-server`
//!
//! ## Example
//!
//! ```rust,no_run
//! use claw_dashboard::{DashboardServer, DashboardConfig};
//! use claw_gateway::{NodeRegistry, WorkloadManager};
//! use std::sync::Arc;
//! use tokio::sync::Mutex;
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = DashboardConfig::default();
//!     let registry = Arc::new(Mutex::new(NodeRegistry::new()));
//!     let workload_manager = Arc::new(Mutex::new(WorkloadManager::new()));
//!     
//!     let server = DashboardServer::new(config, registry, workload_manager);
//!     // server.serve("0.0.0.0:8080").await.unwrap();
//! }
//! ```
//!
//! ## API Endpoints
//!
//! | Endpoint | Method | Description |
//! |----------|--------|-------------|
//! | `/api/status` | GET | Cluster overview with node counts |
//! | `/api/nodes` | GET | List all nodes with health status |
//! | `/api/workloads` | GET | List all workloads with state |
//! | `/api/metrics` | GET | Current metrics snapshot |
//! | `/api/logs/:workload_id` | GET | Stream workload logs (SSE) |
//! | `/api/ws` | GET | WebSocket for real-time updates |

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod config;
pub mod error;
pub mod handlers;
pub mod routes;
pub mod server;
pub mod state;
pub mod types;
pub mod websocket;

// Re-export main types
pub use config::DashboardConfig;
pub use error::{DashboardError, DashboardResult};
pub use server::DashboardServer;
pub use state::DashboardState;
pub use types::{
    ClusterStatus, GpuUtilizationSummary, LiveUpdate, LogEntry, MetricsSnapshot, NodeStatus,
    WorkloadStatus,
};
