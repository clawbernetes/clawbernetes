//! Configuration for clawnode

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Gateway WebSocket URL (e.g., wss://gateway:18789)
    pub gateway: String,
    
    /// Bootstrap token for initial registration
    pub token: Option<String>,
    
    /// Node hostname (defaults to system hostname)
    pub hostname: String,
    
    /// Node labels for scheduling
    #[serde(default)]
    pub labels: HashMap<String, String>,
    
    /// Path to store persistent state
    #[serde(default = "default_state_path")]
    pub state_path: PathBuf,
    
    /// Heartbeat interval in seconds
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_secs: u64,
    
    /// Reconnect delay in seconds
    #[serde(default = "default_reconnect_delay")]
    pub reconnect_delay_secs: u64,
    
    /// Container runtime (docker, podman, containerd)
    #[serde(default = "default_runtime")]
    pub container_runtime: String,

    /// Enable mesh networking (requires network feature at compile time)
    #[serde(default)]
    pub network_enabled: bool,

    /// Region for mesh IP allocation (e.g., "us-west", "us-east", "eu-west")
    #[serde(default = "default_region")]
    pub region: String,

    /// WireGuard listen port
    #[serde(default = "default_wireguard_port")]
    pub wireguard_listen_port: u16,

    /// Ingress proxy listen port (0 = disabled)
    #[serde(default = "default_ingress_port")]
    pub ingress_listen_port: u16,

    /// Public endpoint for WireGuard (other nodes connect to this)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wireguard_endpoint: Option<String>,
}

fn default_state_path() -> PathBuf {
    PathBuf::from("/var/lib/clawnode")
}

fn default_heartbeat_interval() -> u64 {
    30
}

fn default_reconnect_delay() -> u64 {
    5
}

fn default_runtime() -> String {
    "docker".to_string()
}

fn default_region() -> String {
    "us-west".to_string()
}

fn default_wireguard_port() -> u16 {
    51820
}

fn default_ingress_port() -> u16 {
    8443
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            gateway: "wss://localhost:18789".to_string(),
            token: None,
            hostname: hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            labels: HashMap::new(),
            state_path: default_state_path(),
            heartbeat_interval_secs: default_heartbeat_interval(),
            reconnect_delay_secs: default_reconnect_delay(),
            container_runtime: default_runtime(),
            network_enabled: false,
            region: default_region(),
            wireguard_listen_port: default_wireguard_port(),
            ingress_listen_port: default_ingress_port(),
            wireguard_endpoint: None,
        }
    }
}

impl NodeConfig {
    /// Load config from file
    pub fn load(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&content)?;
        Ok(config)
    }
    
    /// Save config to file
    pub fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }
}
