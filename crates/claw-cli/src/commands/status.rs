//! Cluster status command implementation.
//!
//! Shows an overview of the cluster including:
//! - Node count and health
//! - GPU resources
//! - Active workloads

use std::io::Write;

use crate::client::GatewayClient;
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
    pub async fn execute<W: Write>(
        &self,
        writer: &mut W,
        format: &OutputFormat,
    ) -> Result<(), CliError> {
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
        let mut client = GatewayClient::connect(&self.gateway_url).await?;
        let status = client.get_status().await?;

        Ok(ClusterStatus {
            node_count: status.node_count as usize,
            healthy_nodes: status.healthy_nodes as usize,
            gpu_count: status.gpu_count as usize,
            active_workloads: status.active_workloads as usize,
            total_vram_mib: status.total_vram_mib,
            gateway_version: status.gateway_version,
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
    fn fetch_status(
        &self,
    ) -> impl std::future::Future<Output = Result<ClusterStatus, CliError>> + Send;
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
    async fn status_command_connection_refused() {
        // Try to connect to a port that's not listening
        let cmd = StatusCommand::new("ws://127.0.0.1:59999");
        let result = cmd.fetch_status().await;
        // Should fail with connection error (no server running)
        assert!(result.is_err());
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

    #[test]
    fn cluster_status_display() {
        let status = ClusterStatus {
            node_count: 3,
            healthy_nodes: 2,
            gpu_count: 8,
            active_workloads: 1,
            total_vram_mib: 65536,
            gateway_version: "0.1.0".into(),
        };

        // Test JSON output (pretty-printed with spaces)
        let format = OutputFormat::new(Format::Json);
        let mut buf = Vec::new();
        format.write(&mut buf, &status).expect("should write");
        let json = String::from_utf8(buf).expect("valid utf8");
        assert!(json.contains("\"node_count\": 3"));
        assert!(json.contains("\"gateway_version\": \"0.1.0\""));
    }

    #[test]
    fn cluster_status_table_output() {
        let status = ClusterStatus {
            node_count: 3,
            healthy_nodes: 2,
            gpu_count: 8,
            active_workloads: 1,
            total_vram_mib: 65536,
            gateway_version: "0.1.0".into(),
        };

        let format = OutputFormat::new(Format::Table);
        let mut buf = Vec::new();
        format.write(&mut buf, &status).expect("should write");
        let output = String::from_utf8(buf).expect("valid utf8");
        assert!(output.contains("Cluster Status"));
    }
}
