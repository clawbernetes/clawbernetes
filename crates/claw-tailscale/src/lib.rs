//! Tailscale integration for Clawbernetes.
//!
//! This crate provides Tailscale as an alternative network provider to raw `WireGuard`.
//! It wraps the `tailscale` CLI and local socket API for programmatic control.
//!
//! # Features
//!
//! - Multiple authentication methods (auth key, file, env, workload identity)
//! - Service advertisement via Tailscale Services (v1.94+)
//! - Local API client for status queries
//! - `NetworkProvider` trait implementation for unified interface
//!
//! # Example
//!
//! ```rust,no_run
//! use claw_tailscale::{TailscaleNode, TailscaleConfig};
//! use claw_tailscale::auth::AuthMethod;
//!
//! # async fn example() -> claw_tailscale::Result<()> {
//! let config = TailscaleConfig {
//!     auth: AuthMethod::AuthKeyEnv { var_name: "TS_AUTHKEY".into() },
//!     hostname: "clawnode-1".into(),
//!     ..Default::default()
//! };
//!
//! let mut node = TailscaleNode::new(config).await?;
//! node.connect().await?;
//! # Ok(())
//! # }
//! ```

#![deny(unsafe_code)]
// Tests need unsafe for env var manipulation in Rust 2024
#![cfg_attr(test, allow(unsafe_code))]
#![warn(missing_docs)]

pub mod auth;
pub mod client;
pub mod error;
pub mod node;
pub mod service;

pub use auth::AuthMethod;
pub use client::LocalClient;
pub use error::{Result, TailscaleError};
pub use node::{TailscaleConfig, TailscaleNode};
pub use service::{ServiceConfig, ServiceMode};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_api_exports() {
        // Verify public types are accessible
        let _: fn() -> Result<()> = || Ok(());
    }
}
