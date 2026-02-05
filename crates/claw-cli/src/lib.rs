//! # claw-cli
//!
//! Clawbernetes command-line interface.
//!
//! Provides commands for:
//! - Cluster status monitoring
//! - Node management  
//! - MOLT network participation
//! - Workload execution
//!
//! # Architecture
//!
//! The CLI connects to a `claw-gateway-server` via WebSocket using the
//! CLI protocol defined in `claw-proto::cli`. The [`client::GatewayClient`]
//! handles connection management and request/response serialization.
//!
//! ```text
//! ┌───────────┐     CLI Protocol      ┌─────────────────┐
//! │  claw-cli │◄─────────────────────►│  claw-gateway   │
//! └───────────┘     (WebSocket)       └─────────────────┘
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;
pub mod client;
pub mod commands;
pub mod error;
pub mod output;

pub use cli::{
    AlertCommands, ApikeyCommands, AuthCommands, AutoscaleCommands, Cli, Commands,
    CreateAlertArgs, DashboardCommands, DeployArgs, Format, LogsArgs, MetricsCommands,
    MoltCommands, NamespaceCommands, NodeCommands, PreemptArgs, PriorityCommands, RollbackArgs,
    RunArgs, SecretCommands, ServiceCommands, TenantCommands,
};
pub use client::GatewayClient;
pub use error::CliError;
pub use output::OutputFormat;
