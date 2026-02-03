//! Local API client for communicating with tailscaled.
//!
//! The local API is available via Unix socket (Linux/macOS) or named pipe (Windows).
//! It provides status queries, peer information, and some control operations.

use crate::error::{Result, TailscaleError};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

/// Default socket path on Linux.
#[cfg(target_os = "linux")]
pub const DEFAULT_SOCKET_PATH: &str = "/var/run/tailscale/tailscaled.sock";

/// Default socket path on macOS.
#[cfg(target_os = "macos")]
pub const DEFAULT_SOCKET_PATH: &str = "/var/run/tailscaled.socket";

/// Default socket path for other platforms.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub const DEFAULT_SOCKET_PATH: &str = "/var/run/tailscale/tailscaled.sock";

/// Tailscale status information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Status {
    /// Backend state (e.g., "Running", "`NeedsLogin`").
    pub backend_state: String,

    /// Current tailnet name.
    #[serde(default)]
    pub current_tailnet: Option<TailnetInfo>,

    /// This node's information.
    #[serde(rename = "Self")]
    pub self_node: Option<SelfNode>,

    /// Connected peers.
    #[serde(default)]
    pub peer: std::collections::HashMap<String, PeerStatus>,
}

/// Information about the current tailnet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TailnetInfo {
    /// Tailnet name.
    pub name: String,
    /// Magic DNS suffix.
    #[serde(default)]
    pub magic_dns_suffix: String,
}

/// Information about this node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SelfNode {
    /// Node ID.
    #[serde(rename = "ID")]
    pub id: String,
    /// Public key.
    pub public_key: String,
    /// Hostname.
    pub host_name: String,
    /// DNS name.
    #[serde(rename = "DNSName")]
    pub dns_name: String,
    /// Tailscale IPs.
    #[serde(rename = "TailscaleIPs")]
    pub tailscale_ips: Vec<IpAddr>,
    /// Whether this node is online.
    pub online: bool,
}

/// Status of a peer node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PeerStatus {
    /// Node ID.
    #[serde(rename = "ID")]
    pub id: String,
    /// Public key.
    pub public_key: String,
    /// Hostname.
    pub host_name: String,
    /// DNS name.
    #[serde(rename = "DNSName")]
    pub dns_name: String,
    /// Tailscale IPs.
    #[serde(rename = "TailscaleIPs")]
    pub tailscale_ips: Vec<IpAddr>,
    /// Whether the peer is online.
    pub online: bool,
    /// Last seen timestamp.
    #[serde(default)]
    pub last_seen: Option<String>,
}

/// `WhoIs` response for identifying a connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct WhoIsResponse {
    /// Node information.
    pub node: WhoIsNode,
    /// User profile.
    pub user_profile: UserProfile,
}

/// Node information from `WhoIs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct WhoIsNode {
    /// Node ID.
    #[serde(rename = "ID")]
    pub id: String,
    /// Computed name.
    pub computed_name: String,
}

/// User profile from `WhoIs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserProfile {
    /// Login name.
    pub login_name: String,
    /// Display name.
    pub display_name: String,
}

/// Client for the Tailscale local API.
///
/// Uses the `tailscale` CLI for most operations as it handles
/// socket communication internally.
#[derive(Debug, Clone)]
pub struct LocalClient {
    socket_path: PathBuf,
}

impl LocalClient {
    /// Create a new local client with the default socket path.
    #[must_use]
    pub fn new() -> Self {
        Self {
            socket_path: PathBuf::from(DEFAULT_SOCKET_PATH),
        }
    }

    /// Create a new local client with a custom socket path.
    #[must_use]
    pub fn with_socket_path(path: impl AsRef<Path>) -> Self {
        Self {
            socket_path: path.as_ref().to_path_buf(),
        }
    }

    /// Check if tailscaled is running.
    pub async fn is_running(&self) -> bool {
        self.status().await.is_ok()
    }

    /// Get the current Tailscale status.
    ///
    /// # Errors
    ///
    /// Returns error if tailscale CLI is not available or tailscaled is not running.
    pub async fn status(&self) -> Result<Status> {
        let output = Command::new("tailscale")
            .args(["status", "--json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| TailscaleError::NotInstalled {
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TailscaleError::NotRunning {
                message: stderr.to_string(),
            });
        }

        let status: Status = serde_json::from_slice(&output.stdout).map_err(|e| {
            TailscaleError::ApiError {
                message: format!("failed to parse status: {e}"),
            }
        })?;

        Ok(status)
    }

    /// Get information about who is connecting from an IP address.
    ///
    /// # Errors
    ///
    /// Returns error if the IP is not a Tailscale peer.
    pub async fn whois(&self, ip: IpAddr) -> Result<WhoIsResponse> {
        let output = Command::new("tailscale")
            .args(["whois", "--json", &ip.to_string()])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| TailscaleError::NotInstalled {
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TailscaleError::ApiError {
                message: format!("whois failed: {stderr}"),
            });
        }

        let response: WhoIsResponse =
            serde_json::from_slice(&output.stdout).map_err(|e| TailscaleError::ApiError {
                message: format!("failed to parse whois response: {e}"),
            })?;

        Ok(response)
    }

    /// Get the socket path.
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

impl Default for LocalClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_socket_path() {
        let client = LocalClient::new();
        assert!(!client.socket_path().as_os_str().is_empty());
    }

    #[test]
    fn test_custom_socket_path() {
        let client = LocalClient::with_socket_path("/custom/path.sock");
        assert_eq!(client.socket_path(), Path::new("/custom/path.sock"));
    }

    #[test]
    fn test_status_deserialization() {
        let json = r#"{
            "BackendState": "Running",
            "CurrentTailnet": {
                "Name": "example.com",
                "MagicDNSSuffix": "ts.net"
            },
            "Self": {
                "ID": "12345",
                "PublicKey": "abc123",
                "HostName": "mynode",
                "DNSName": "mynode.example.ts.net",
                "TailscaleIPs": ["100.64.0.1"],
                "Online": true
            },
            "Peer": {}
        }"#;

        let status: Status = serde_json::from_str(json).expect("should parse");
        assert_eq!(status.backend_state, "Running");
        assert!(status.self_node.is_some());
        let self_node = status.self_node.as_ref().expect("self node");
        assert_eq!(self_node.host_name, "mynode");
    }

    #[test]
    fn test_whois_deserialization() {
        let json = r#"{
            "Node": {
                "ID": "12345",
                "ComputedName": "peer-node"
            },
            "UserProfile": {
                "LoginName": "user@example.com",
                "DisplayName": "Test User"
            }
        }"#;

        let response: WhoIsResponse = serde_json::from_str(json).expect("should parse");
        assert_eq!(response.node.computed_name, "peer-node");
        assert_eq!(response.user_profile.login_name, "user@example.com");
    }
}
