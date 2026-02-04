//! Tailscale Services support.
//!
//! Tailscale Services (v1.94+) allow publishing internal resources as named
//! services in your tailnet, decoupling resources from specific devices.
//!
//! # Security
//!
//! All service names, ports, and targets are validated before being passed
//! to the tailscale CLI to prevent command injection.
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
use claw_validation::command::{AllowedProgram, SafeCommand};
use claw_validation::{sanitize_service_name, sanitize_url, validate_port};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
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
/// # Security
///
/// The service name is validated to prevent command injection:
/// - Must be lowercase alphanumeric with hyphens
/// - Must start and end with alphanumeric
/// - Maximum 64 characters
///
/// The target URL is validated to ensure it starts with a valid scheme.
///
/// # Arguments
///
/// * `config` - Service configuration
/// * `target` - Backend target (e.g., <http://localhost:8080> or <tcp://localhost:5432>)
///
/// # Errors
///
/// Returns error if validation fails, tailscale CLI fails, or service cannot be advertised.
pub async fn advertise_service(config: &ServiceConfig, target: &str) -> Result<()> {
    // Validate service name (security critical)
    let validated_name = sanitize_service_name(&config.name).map_err(|e| {
        TailscaleError::ServiceError {
            service_name: config.name.clone(),
            operation: "validate".to_string(),
            reason: e.to_string(),
        }
    })?;

    // Validate port
    let _validated_port = validate_port(config.port).map_err(|e| {
        TailscaleError::ServiceError {
            service_name: config.name.clone(),
            operation: "validate".to_string(),
            reason: e.to_string(),
        }
    })?;

    // Validate target URL (security critical)
    let validated_target = sanitize_url(target).map_err(|e| {
        TailscaleError::ServiceError {
            service_name: config.name.clone(),
            operation: "validate".to_string(),
            reason: e.to_string(),
        }
    })?;

    let port_spec = format!("{}:{}", config.mode.protocol(), config.port);

    let output = SafeCommand::new(AllowedProgram::Tailscale)
        .args(["serve", "--service", validated_name.as_str(), &port_spec, validated_target.as_str()])
        .execute()
        .await
        .map_err(|e| TailscaleError::ServiceError {
            service_name: config.name.clone(),
            operation: "advertise".to_string(),
            reason: e.to_string(),
        })?;

    if !output.success() {
        return Err(TailscaleError::ServiceError {
            service_name: config.name.clone(),
            operation: "advertise".to_string(),
            reason: output.stderr_lossy(),
        });
    }

    info!(service = %config.name, port = config.port, "advertised service");
    Ok(())
}

/// Drain a service (stop accepting new connections but let existing ones finish).
///
/// # Errors
///
/// Returns error if validation fails or tailscale CLI fails.
pub async fn drain_service(name: &str) -> Result<()> {
    // Validate service name
    let validated_name = sanitize_service_name(name).map_err(|e| {
        TailscaleError::ServiceError {
            service_name: name.to_string(),
            operation: "validate".to_string(),
            reason: e.to_string(),
        }
    })?;

    let svc_spec = format!("svc:{}", validated_name.as_str());

    let output = SafeCommand::new(AllowedProgram::Tailscale)
        .args(["serve", "drain", &svc_spec])
        .execute()
        .await
        .map_err(|e| TailscaleError::ServiceError {
            service_name: name.to_string(),
            operation: "drain".to_string(),
            reason: e.to_string(),
        })?;

    if !output.success() {
        return Err(TailscaleError::ServiceError {
            service_name: name.to_string(),
            operation: "drain".to_string(),
            reason: output.stderr_lossy(),
        });
    }

    info!(service = %name, "draining service");
    Ok(())
}

/// Remove a service advertisement.
///
/// # Errors
///
/// Returns error if validation fails or tailscale CLI fails.
pub async fn remove_service(name: &str, port: u16, mode: ServiceMode) -> Result<()> {
    // Validate service name
    let validated_name = sanitize_service_name(name).map_err(|e| {
        TailscaleError::ServiceError {
            service_name: name.to_string(),
            operation: "validate".to_string(),
            reason: e.to_string(),
        }
    })?;

    // Validate port
    let _validated_port = validate_port(port).map_err(|e| {
        TailscaleError::ServiceError {
            service_name: name.to_string(),
            operation: "validate".to_string(),
            reason: e.to_string(),
        }
    })?;

    let port_spec = format!("{}:{}", mode.protocol(), port);

    let output = SafeCommand::new(AllowedProgram::Tailscale)
        .args(["serve", "--service", validated_name.as_str(), &port_spec, "off"])
        .execute()
        .await
        .map_err(|e| TailscaleError::ServiceError {
            service_name: name.to_string(),
            operation: "remove".to_string(),
            reason: e.to_string(),
        })?;

    if !output.success() {
        return Err(TailscaleError::ServiceError {
            service_name: name.to_string(),
            operation: "remove".to_string(),
            reason: output.stderr_lossy(),
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
    let output = SafeCommand::new(AllowedProgram::Tailscale)
        .args(["serve", "status"])
        .execute()
        .await
        .map_err(|e| TailscaleError::ServiceError {
            service_name: "status".to_string(),
            operation: "serve".to_string(),
            reason: e.to_string(),
        })?;

    Ok(output.stdout_lossy())
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

    // Security tests - these validate that injection attempts are caught
    #[test]
    fn test_service_name_injection_blocked() {
        // These should all fail validation
        assert!(sanitize_service_name("; rm -rf /").is_err());
        assert!(sanitize_service_name("service$(whoami)").is_err());
        assert!(sanitize_service_name("service`id`").is_err());
        assert!(sanitize_service_name("service|cat /etc/passwd").is_err());
    }

    #[test]
    fn test_target_url_injection_blocked() {
        // These should all fail validation
        assert!(sanitize_url("http://localhost; rm -rf /").is_err());
        assert!(sanitize_url("http://localhost`id`").is_err());
        assert!(sanitize_url("not-a-url").is_err());
    }

    #[test]
    fn test_valid_service_name_accepted() {
        assert!(sanitize_service_name("my-api").is_ok());
        assert!(sanitize_service_name("api-v2").is_ok());
        assert!(sanitize_service_name("service123").is_ok());
    }

    #[test]
    fn test_valid_target_url_accepted() {
        assert!(sanitize_url("http://localhost:8080").is_ok());
        assert!(sanitize_url("https://example.com/path").is_ok());
        assert!(sanitize_url("tcp://localhost:5432").is_ok());
    }
}
