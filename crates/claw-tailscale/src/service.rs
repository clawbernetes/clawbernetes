//! Tailscale Services support.
//!
//! Tailscale Services (v1.94+) allow publishing internal resources as named
//! services in your tailnet, decoupling resources from specific devices.
//!
//! # Example
//!
//! ```rust,no_run
//! use claw_tailscale::service::{ServiceConfig, ServiceMode, advertise_service};
//!
//! # async fn example() -> claw_tailscale::Result<()> {
//! let config = ServiceConfig {
//!     name: "my-api".to_string(),
//!     port: 8080,
//!     mode: ServiceMode::Tcp,
//! };
//!
//! advertise_service(&config, "http://localhost:8080").await?;
//! # Ok(())
//! # }
//! ```

use crate::error::{Result, TailscaleError};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info};

/// Service mode for Tailscale Services.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ServiceMode {
    /// TCP-level proxying.
    #[default]
    Tcp,
    /// HTTP with optional TLS termination.
    Http {
        /// Whether to use TLS (HTTPS).
        tls: bool,
    },
}

impl ServiceMode {
    /// Get the protocol prefix for tailscale serve.
    #[must_use]
    pub fn protocol(&self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Http { tls: true } => "https",
            Self::Http { tls: false } => "http",
        }
    }
}


/// Configuration for a Tailscale Service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Service name (used in `MagicDNS`).
    pub name: String,

    /// Port to expose.
    pub port: u16,

    /// Service mode (TCP or HTTP).
    #[serde(default)]
    pub mode: ServiceMode,
}

/// Status of an advertised service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    /// Service name.
    pub name: String,
    /// Port being served.
    pub port: u16,
    /// Whether the service is active.
    pub active: bool,
    /// Backend target.
    pub target: String,
}

/// Discovered service endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEndpoint {
    /// Service name.
    pub name: String,
    /// Node providing the service.
    pub node: String,
    /// IP addresses for the service.
    pub ips: Vec<IpAddr>,
    /// Whether the node is online.
    pub online: bool,
}

/// Advertise a service via Tailscale Services.
///
/// This uses `tailscale serve --service` to advertise the service.
///
/// # Arguments
///
/// * `config` - Service configuration
/// * `target` - Backend target (e.g., <http://localhost:8080> or <tcp://localhost:5432>)
///
/// # Errors
///
/// Returns error if tailscale CLI fails or service cannot be advertised.
pub async fn advertise_service(config: &ServiceConfig, target: &str) -> Result<()> {
    let port_spec = format!("{}:{}", config.mode.protocol(), config.port);

    let output = Command::new("tailscale")
        .args(["serve", "--service", &config.name, &port_spec, target])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| TailscaleError::NotInstalled {
            message: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TailscaleError::ServiceError {
            service_name: config.name.clone(),
            operation: "advertise".to_string(),
            reason: stderr.to_string(),
        });
    }

    info!(service = %config.name, port = config.port, "advertised service");
    Ok(())
}

/// Drain a service (stop accepting new connections but let existing ones finish).
///
/// # Errors
///
/// Returns error if tailscale CLI fails.
pub async fn drain_service(name: &str) -> Result<()> {
    let output = Command::new("tailscale")
        .args(["serve", "drain", &format!("svc:{name}")])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| TailscaleError::NotInstalled {
            message: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TailscaleError::ServiceError {
            service_name: name.to_string(),
            operation: "serve".to_string(), reason: stderr.to_string(),
        });
    }

    info!(service = %name, "draining service");
    Ok(())
}

/// Remove a service advertisement.
///
/// # Errors
///
/// Returns error if tailscale CLI fails.
pub async fn remove_service(name: &str, port: u16, mode: ServiceMode) -> Result<()> {
    let port_spec = format!("{}:{}", mode.protocol(), port);

    let output = Command::new("tailscale")
        .args(["serve", "--service", name, &port_spec, "off"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| TailscaleError::NotInstalled {
            message: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TailscaleError::ServiceError {
            service_name: name.to_string(),
            operation: "serve".to_string(), reason: stderr.to_string(),
        });
    }

    info!(service = %name, "removed service");
    Ok(())
}

/// Get the status of served content.
///
/// # Errors
///
/// Returns error if tailscale CLI fails.
pub async fn serve_status() -> Result<String> {
    let output = Command::new("tailscale")
        .args(["serve", "status"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| TailscaleError::NotInstalled {
            message: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TailscaleError::ServiceError {
            service_name: "status".to_string(),
            operation: "serve".to_string(), reason: stderr.to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Service manager for handling multiple services.
#[derive(Debug, Default)]
pub struct ServiceManager {
    services: Vec<ServiceConfig>,
}

impl ServiceManager {
    /// Create a new service manager.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add and advertise a service.
    ///
    /// # Errors
    ///
    /// Returns error if service cannot be advertised.
    pub async fn add(&mut self, config: ServiceConfig, target: &str) -> Result<()> {
        advertise_service(&config, target).await?;
        self.services.push(config);
        Ok(())
    }

    /// Drain and remove a service.
    ///
    /// # Errors
    ///
    /// Returns error if service cannot be removed.
    pub async fn remove(&mut self, name: &str) -> Result<()> {
        if let Some(idx) = self.services.iter().position(|s| s.name == name) {
            let config = self.services.remove(idx);
            drain_service(name).await?;
            remove_service(name, config.port, config.mode).await?;
        }
        Ok(())
    }

    /// Drain and remove all services.
    ///
    /// # Errors
    ///
    /// Returns error if any service cannot be removed.
    pub async fn remove_all(&mut self) -> Result<()> {
        let services = std::mem::take(&mut self.services);
        for config in services {
            if let Err(e) = drain_service(&config.name).await {
                debug!(service = %config.name, error = %e, "failed to drain service");
            }
            if let Err(e) = remove_service(&config.name, config.port, config.mode).await {
                debug!(service = %config.name, error = %e, "failed to remove service");
            }
        }
        Ok(())
    }

    /// Get list of managed services.
    #[must_use]
    pub fn services(&self) -> &[ServiceConfig] {
        &self.services
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_mode_protocol() {
        assert_eq!(ServiceMode::Tcp.protocol(), "tcp");
        assert_eq!(ServiceMode::Http { tls: false }.protocol(), "http");
        assert_eq!(ServiceMode::Http { tls: true }.protocol(), "https");
    }

    #[test]
    fn test_service_config_serialization() {
        let config = ServiceConfig {
            name: "my-service".to_string(),
            port: 8080,
            mode: ServiceMode::Http { tls: true },
        };

        let json = serde_json::to_string(&config).expect("serialize");
        let parsed: ServiceConfig = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.name, "my-service");
        assert_eq!(parsed.port, 8080);
        assert_eq!(parsed.mode, ServiceMode::Http { tls: true });
    }

    #[test]
    fn test_service_manager_new() {
        let manager = ServiceManager::new();
        assert!(manager.services().is_empty());
    }

    #[test]
    fn test_default_service_mode() {
        let mode = ServiceMode::default();
        assert_eq!(mode, ServiceMode::Tcp);
    }
}
