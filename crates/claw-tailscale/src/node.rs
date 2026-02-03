//! Tailscale node management.
//!
//! This module provides the main `TailscaleNode` type for connecting to
//! and managing a Tailscale network connection.

use crate::auth::{resolve_auth_key, AuthMethod};
use crate::client::{LocalClient, SelfNode, Status};
use crate::error::{Result, TailscaleError};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info, warn};

/// Configuration for a Tailscale node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct TailscaleConfig {
    /// Authentication method.
    pub auth: AuthMethod,

    /// Hostname to use on the tailnet.
    pub hostname: String,

    /// Tags to apply to this node (e.g., "tag:server").
    #[serde(default)]
    pub tags: Vec<String>,

    /// Whether to advertise as an exit node.
    #[serde(default)]
    pub exit_node: bool,

    /// Whether to accept routes from other nodes.
    #[serde(default)]
    pub accept_routes: bool,

    /// Whether to accept DNS configuration from the tailnet.
    #[serde(default = "default_true")]
    pub accept_dns: bool,

    /// Subnets to advertise (CIDR notation).
    #[serde(default)]
    pub advertise_routes: Vec<String>,

    /// Whether this node should be ephemeral (removed when offline).
    #[serde(default)]
    pub ephemeral: bool,
}

fn default_true() -> bool {
    true
}

impl Default for TailscaleConfig {
    fn default() -> Self {
        Self {
            auth: AuthMethod::AuthKeyEnv {
                var_name: "TS_AUTHKEY".to_string(),
            },
            hostname: String::new(),
            tags: Vec::new(),
            exit_node: false,
            accept_routes: false,
            accept_dns: true,
            advertise_routes: Vec::new(),
            ephemeral: false,
        }
    }
}

/// A Tailscale node instance.
///
/// Wraps the `tailscale` CLI to provide programmatic control over
/// the Tailscale connection.
pub struct TailscaleNode {
    config: TailscaleConfig,
    client: LocalClient,
    connected: bool,
}

impl TailscaleNode {
    /// Create a new Tailscale node with the given configuration.
    ///
    /// This does not connect to the network; call `connect()` to do that.
    ///
    /// # Errors
    ///
    /// Returns error if the configuration is invalid.
    pub async fn new(config: TailscaleConfig) -> Result<Self> {
        // Validate auth method can be resolved
        let _ = resolve_auth_key(&config.auth).await?;

        Ok(Self {
            config,
            client: LocalClient::new(),
            connected: false,
        })
    }

    /// Connect to the Tailscale network.
    ///
    /// # Errors
    ///
    /// Returns error if connection fails.
    pub async fn connect(&mut self) -> Result<()> {
        let auth_key = resolve_auth_key(&self.config.auth).await?;

        let mut cmd = Command::new("tailscale");
        cmd.arg("up");
        cmd.arg("--authkey").arg(&auth_key);

        if !self.config.hostname.is_empty() {
            cmd.arg("--hostname").arg(&self.config.hostname);
        }

        if !self.config.tags.is_empty() {
            let tags = self.config.tags.join(",");
            cmd.arg("--advertise-tags").arg(&tags);
        }

        if self.config.exit_node {
            cmd.arg("--advertise-exit-node");
        }

        if self.config.accept_routes {
            cmd.arg("--accept-routes");
        }

        if !self.config.accept_dns {
            cmd.arg("--accept-dns=false");
        }

        if !self.config.advertise_routes.is_empty() {
            let routes = self.config.advertise_routes.join(",");
            cmd.arg("--advertise-routes").arg(&routes);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        debug!(?cmd, "running tailscale up");

        let output = cmd.output().await.map_err(|e| TailscaleError::NotInstalled {
            message: e.to_string(),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TailscaleError::ConnectionFailed {
                reason: stderr.to_string(),
            });
        }

        self.connected = true;
        info!(hostname = %self.config.hostname, "connected to tailnet");
        Ok(())
    }

    /// Disconnect from the Tailscale network.
    ///
    /// # Errors
    ///
    /// Returns error if disconnect fails.
    pub async fn disconnect(&mut self) -> Result<()> {
        let output = Command::new("tailscale")
            .arg("down")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| TailscaleError::NotInstalled {
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(%stderr, "tailscale down warning");
        }

        self.connected = false;
        info!("disconnected from tailnet");
        Ok(())
    }

    /// Get the current status.
    ///
    /// # Errors
    ///
    /// Returns error if status query fails.
    pub async fn status(&self) -> Result<Status> {
        self.client.status().await
    }

    /// Get this node's information.
    ///
    /// # Errors
    ///
    /// Returns error if not connected or status query fails.
    pub async fn self_node(&self) -> Result<SelfNode> {
        let status = self.status().await?;
        status.self_node.ok_or_else(|| TailscaleError::NotRunning {
            message: "no self node in status".to_string(),
        })
    }

    /// Get the Tailscale IPv4 address for this node.
    ///
    /// # Errors
    ///
    /// Returns error if not connected or no IPv4 assigned.
    pub async fn ipv4(&self) -> Result<Ipv4Addr> {
        let self_node = self.self_node().await?;
        self_node
            .tailscale_ips
            .iter()
            .find_map(|ip| match ip {
                IpAddr::V4(v4) => Some(*v4),
                IpAddr::V6(_) => None,
            })
            .ok_or_else(|| TailscaleError::NotRunning {
                message: "no IPv4 address assigned".to_string(),
            })
    }

    /// Get the Tailscale IPv6 address for this node.
    ///
    /// # Errors
    ///
    /// Returns error if not connected or no IPv6 assigned.
    pub async fn ipv6(&self) -> Result<Ipv6Addr> {
        let self_node = self.self_node().await?;
        self_node
            .tailscale_ips
            .iter()
            .find_map(|ip| match ip {
                IpAddr::V4(_) => None,
                IpAddr::V6(v6) => Some(*v6),
            })
            .ok_or_else(|| TailscaleError::NotRunning {
                message: "no IPv6 address assigned".to_string(),
            })
    }

    /// Get the hostname.
    #[must_use]
    pub fn hostname(&self) -> &str {
        &self.config.hostname
    }

    /// Check if connected to the tailnet.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Get the local API client.
    #[must_use]
    pub fn client(&self) -> &LocalClient {
        &self.client
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TailscaleConfig::default();
        assert!(config.hostname.is_empty());
        assert!(config.tags.is_empty());
        assert!(!config.exit_node);
        assert!(config.accept_dns);
    }

    #[test]
    fn test_config_serialization() {
        let config = TailscaleConfig {
            auth: AuthMethod::AuthKey {
                key: "test-key".to_string(),
            },
            hostname: "test-node".to_string(),
            tags: vec!["tag:server".to_string()],
            exit_node: false,
            accept_routes: true,
            accept_dns: true,
            advertise_routes: vec!["10.0.0.0/8".to_string()],
            ephemeral: true,
        };

        let json = serde_json::to_string(&config).expect("serialize");
        let parsed: TailscaleConfig = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.hostname, "test-node");
        assert_eq!(parsed.tags, vec!["tag:server"]);
        assert!(parsed.accept_routes);
        assert!(parsed.ephemeral);
    }
}
