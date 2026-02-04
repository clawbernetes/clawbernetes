//! Tailscale node management.
//!
//! This module provides the main `TailscaleNode` type for connecting to
//! and managing a Tailscale network connection.
//!
//! # Security
//!
//! All configuration values (hostname, tags, routes) are validated before
//! being passed to the tailscale CLI to prevent command injection.

use crate::auth::{resolve_auth_key, AuthMethod};
use crate::client::{LocalClient, SelfNode, Status};
use crate::error::{Result, TailscaleError};
use claw_validation::command::{AllowedProgram, SafeCommand};
use claw_validation::sanitize_hostname;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
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

/// Validate a tag format (e.g., "tag:server").
fn validate_tag(tag: &str) -> Result<()> {
    // Tags must start with "tag:" and contain only safe characters
    if !tag.starts_with("tag:") {
        return Err(TailscaleError::InvalidHostname {
            message: format!("tag must start with 'tag:': {tag}"),
        });
    }

    let tag_name = &tag[4..];
    if tag_name.is_empty() {
        return Err(TailscaleError::InvalidHostname {
            message: "tag name cannot be empty".to_string(),
        });
    }

    // Check for dangerous characters
    for c in tag_name.chars() {
        if !c.is_alphanumeric() && c != '-' && c != '_' {
            return Err(TailscaleError::InvalidHostname {
                message: format!("tag contains invalid character '{c}'"),
            });
        }
    }

    Ok(())
}

/// Validate a CIDR route (e.g., "10.0.0.0/8").
fn validate_route(route: &str) -> Result<()> {
    // Basic CIDR validation
    let parts: Vec<&str> = route.split('/').collect();
    if parts.len() != 2 {
        return Err(TailscaleError::InvalidHostname {
            message: format!("invalid CIDR format: {route}"),
        });
    }

    // Validate IP part
    if parts[0].parse::<IpAddr>().is_err() {
        return Err(TailscaleError::InvalidHostname {
            message: format!("invalid IP in route: {route}"),
        });
    }

    // Validate prefix length
    let prefix: u8 = parts[1].parse().map_err(|_| TailscaleError::InvalidHostname {
        message: format!("invalid prefix length in route: {route}"),
    })?;

    // Check reasonable prefix lengths
    if parts[0].contains(':') {
        // IPv6
        if prefix > 128 {
            return Err(TailscaleError::InvalidHostname {
                message: format!("IPv6 prefix too large: {prefix}"),
            });
        }
    } else {
        // IPv4
        if prefix > 32 {
            return Err(TailscaleError::InvalidHostname {
                message: format!("IPv4 prefix too large: {prefix}"),
            });
        }
    }

    Ok(())
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
    /// # Security
    ///
    /// All configuration values are validated before being passed to tailscale:
    /// - Hostname is validated per RFC 1123
    /// - Tags must start with "tag:" and contain only safe characters
    /// - Routes must be valid CIDR notation
    ///
    /// # Errors
    ///
    /// Returns error if validation or connection fails.
    pub async fn connect(&mut self) -> Result<()> {
        let auth_key = resolve_auth_key(&self.config.auth).await?;

        // Build command with validated arguments
        let mut cmd = SafeCommand::new(AllowedProgram::Tailscale)
            .arg("up")
            .arg("--authkey")
            .arg(&auth_key);

        // Validate and add hostname
        if !self.config.hostname.is_empty() {
            let validated_hostname = sanitize_hostname(&self.config.hostname).map_err(|e| {
                TailscaleError::InvalidHostname {
                    message: e.to_string(),
                }
            })?;
            cmd = cmd.arg("--hostname").arg(validated_hostname.as_str());
        }

        // Validate and add tags
        if !self.config.tags.is_empty() {
            for tag in &self.config.tags {
                validate_tag(tag)?;
            }
            let tags = self.config.tags.join(",");
            cmd = cmd.arg("--advertise-tags").arg(&tags);
        }

        if self.config.exit_node {
            cmd = cmd.arg("--advertise-exit-node");
        }

        if self.config.accept_routes {
            cmd = cmd.arg("--accept-routes");
        }

        if !self.config.accept_dns {
            cmd = cmd.arg("--accept-dns=false");
        }

        // Validate and add routes
        if !self.config.advertise_routes.is_empty() {
            for route in &self.config.advertise_routes {
                validate_route(route)?;
            }
            let routes = self.config.advertise_routes.join(",");
            cmd = cmd.arg("--advertise-routes").arg(&routes);
        }

        debug!("running tailscale up");

        let output = cmd.execute().await.map_err(|e| TailscaleError::NotInstalled {
            message: e.to_string(),
        })?;

        if !output.success() {
            return Err(TailscaleError::ConnectionFailed {
                reason: output.stderr_lossy(),
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
        let output = SafeCommand::new(AllowedProgram::Tailscale)
            .arg("down")
            .execute_unchecked()
            .await
            .map_err(|e| TailscaleError::NotInstalled {
                message: e.to_string(),
            })?;

        if !output.success() {
            warn!(stderr = %output.stderr_lossy(), "tailscale down warning");
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

    // Security tests
    #[test]
    fn test_validate_tag_valid() {
        assert!(validate_tag("tag:server").is_ok());
        assert!(validate_tag("tag:web-server").is_ok());
        assert!(validate_tag("tag:api_v2").is_ok());
    }

    #[test]
    fn test_validate_tag_invalid() {
        assert!(validate_tag("server").is_err()); // missing tag: prefix
        assert!(validate_tag("tag:").is_err()); // empty name
        assert!(validate_tag("tag:server;rm").is_err()); // injection attempt
        assert!(validate_tag("tag:$(whoami)").is_err()); // injection attempt
    }

    #[test]
    fn test_validate_route_valid() {
        assert!(validate_route("10.0.0.0/8").is_ok());
        assert!(validate_route("192.168.1.0/24").is_ok());
        assert!(validate_route("::1/128").is_ok());
        assert!(validate_route("fd00::/8").is_ok());
    }

    #[test]
    fn test_validate_route_invalid() {
        assert!(validate_route("10.0.0.0").is_err()); // missing prefix
        assert!(validate_route("invalid/24").is_err()); // invalid IP
        assert!(validate_route("10.0.0.0/33").is_err()); // prefix too large for IPv4
    }

    #[test]
    fn test_hostname_injection_blocked() {
        assert!(sanitize_hostname("valid-hostname").is_ok());
        assert!(sanitize_hostname("host.example.com").is_ok());
        
        // These should fail
        assert!(sanitize_hostname("host;rm -rf /").is_err());
        assert!(sanitize_hostname("host$(whoami)").is_err());
        assert!(sanitize_hostname("host`id`").is_err());
    }
}
