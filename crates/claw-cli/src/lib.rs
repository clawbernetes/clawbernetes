//! # claw-cli
//!
//! Clawbernetes command-line interface.
//!
//! Provides commands for:
//! - Cluster status monitoring
//! - Node management  
//! - MOLT network participation
//! - Workload execution

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;
pub mod commands;
pub mod error;
pub mod output;

pub use cli::{Cli, Commands, Format, NodeCommands, MoltCommands, RunArgs};
pub use error::CliError;
pub use output::OutputFormat;
