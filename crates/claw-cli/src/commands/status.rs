//! Cluster status command implementation.
//!
//! Shows an overview of the cluster including:
//! - Node count and health
//! - GPU resources
//! - Active workloads

use std::io::Write;

use crate::error::CliError;
use crate::output::{ClusterStatus, OutputFormat};

/// Status command executor.
pub struct StatusCommand {
    gateway_url: String,
}

impl StatusCommand {
    /// Create a new status command.
    #[must_use]
    pub fn new(gateway_url: impl Into<String>) -> Self {
        Self {
            gateway_url: gateway_url.into(),
        }
    }

    /// Execute the status command.
    ///
    /// # Errors
    ///
    /// Returns an error if connection to gateway fails or output fails.
    pub async fn execute<W: Write>(&self, writer: &mut W, format: &OutputFormat) -> Result<(), CliError> {
        let status = self.fetch_status().await?;
        format.write(writer, &status)?;
        Ok(())
    }

    /// Fetch cluster status from gateway.
    ///
    /// # Errors
    ///
    /// Returns an error if connection fails.
    pub async fn fetch_status(&self) -> Result<ClusterStatus, CliError> {
        // In a real implementation, this would connect to the gateway
        // For now, we return mock data that can be replaced with real gateway calls
        
        // Validate gateway URL format
        if !self.gateway_url.starts_with("ws://") && !self.gateway_url.starts_with("wss://") {
            return Err(CliError::Config(format!(
                "invalid gateway URL: {}, must start with ws:// or wss://",
                self.gateway_url
            )));
        }

        // TODO: Replace with actual gateway connection
        // This is a placeholder that will be filled in when gateway client is implemented
        Ok(ClusterStatus {
            node_count: 0,
            healthy_nodes: 0,
            gpu_count: 0,
            active_workloads: 0,
            total_vram_mib: 0,
            gateway_version: "0.1.0".into(),
        })
    }
}

/// Gateway client for status queries.
///
/// This trait allows for testing with fake implementations.
pub trait StatusClient: Send + Sync {
    /// Fetch cluster status.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    fn fetch_status(&self) -> impl std::future::Future<Output = Result<ClusterStatus, CliError>> + Send;
}

/// Real gateway client implementation.
pub struct GatewayStatusClient {
    url: String,
}

impl GatewayStatusClient {
    /// Create a new gateway client.
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }
}

impl StatusClient for GatewayStatusClient {
    async fn fetch_status(&self) -> Result<ClusterStatus, CliError> {
        // Validate URL
        if !self.url.starts_with("ws://") && !self.url.starts_with("wss://") {
            return Err(CliError::Config(format!(
                "invalid gateway URL: {}",
                self.url
            )));
        }

        // TODO: Implement actual WebSocket connection to gateway
        // For now return placeholder
        Ok(ClusterStatus {
            node_count: 0,
            healthy_nodes: 0,
            gpu_count: 0,
            active_workloads: 0,
            total_vram_mib: 0,
            gateway_version: "0.1.0".into(),
        })
    }
}

/// Fake status client for testing.
#[cfg(test)]
pub struct FakeStatusClient {
    status: ClusterStatus,
}

#[cfg(test)]
impl FakeStatusClient {
    /// Create a fake client with the given status.
    pub fn new(status: ClusterStatus) -> Self {
        Self { status }
    }
}

#[cfg(test)]
impl StatusClient for FakeStatusClient {
    async fn fetch_status(&self) -> Result<ClusterStatus, CliError> {
        Ok(self.status.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Format;

    #[test]
    fn status_command_new() {
        let cmd = StatusCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[tokio::test]
    async fn status_command_invalid_url() {
        let cmd = StatusCommand::new("http://invalid");
        let result = cmd.fetch_status().await;
        assert!(result.is_err());
        
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid gateway URL"));
    }

    #[tokio::test]
    async fn status_command_valid_ws_url() {
        let cmd = StatusCommand::new("ws://localhost:8080");
        let result = cmd.fetch_status().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn status_command_valid_wss_url() {
        let cmd = StatusCommand::new("wss://secure.gateway:443");
        let result = cmd.fetch_status().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn status_command_execute_table() {
        let cmd = StatusCommand::new("ws://localhost:8080");
        let format = OutputFormat::new(Format::Table);
        let mut buf = Vec::new();
        
        cmd.execute(&mut buf, &format).await.expect("should execute");
        
        let output = String::from_utf8(buf).expect("valid utf8");
        assert!(output.contains("Cluster Status"));
    }

    #[tokio::test]
    async fn status_command_execute_json() {
        let cmd = StatusCommand::new("ws://localhost:8080");
        let format = OutputFormat::new(Format::Json);
        let mut buf = Vec::new();
        
        cmd.execute(&mut buf, &format).await.expect("should execute");
        
        let output = String::from_utf8(buf).expect("valid utf8");
        assert!(output.contains("\"node_count\""));
        assert!(output.contains("\"gateway_version\""));
    }

    #[tokio::test]
    async fn fake_status_client_returns_configured_status() {
        let expected = ClusterStatus {
            node_count: 5,
            healthy_nodes: 4,
            gpu_count: 12,
            active_workloads: 3,
            total_vram_mib: 245760,
            gateway_version: "1.2.3".into(),
        };

        let client = FakeStatusClient::new(expected.clone());
        let status = client.fetch_status().await.expect("should fetch");

        assert_eq!(status.node_count, 5);
        assert_eq!(status.healthy_nodes, 4);
        assert_eq!(status.gpu_count, 12);
        assert_eq!(status.gateway_version, "1.2.3");
    }

    #[tokio::test]
    async fn gateway_status_client_validates_url() {
        let client = GatewayStatusClient::new("invalid-url");
        let result = client.fetch_status().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn gateway_status_client_accepts_valid_url() {
        let client = GatewayStatusClient::new("ws://localhost:8080");
        let result = client.fetch_status().await;
        // Should succeed (returns placeholder for now)
        assert!(result.is_ok());
    }
}
