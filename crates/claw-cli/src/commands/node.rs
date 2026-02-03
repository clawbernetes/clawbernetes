//! Node management command implementation.
//!
//! Provides subcommands for:
//! - Listing all nodes
//! - Getting detailed node information
//! - Draining nodes

use std::io::Write;

use crate::cli::NodeCommands;
use crate::error::CliError;
use crate::output::{Message, NodeDetail, NodeList, OutputFormat};

#[cfg(test)]
use crate::output::{GpuDetail, NodeInfo, WorkloadSummary};

/// Node command executor.
pub struct NodeCommand {
    gateway_url: String,
}

impl NodeCommand {
    /// Create a new node command.
    #[must_use]
    pub fn new(gateway_url: impl Into<String>) -> Self {
        Self {
            gateway_url: gateway_url.into(),
        }
    }

    /// Execute a node subcommand.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    pub async fn execute<W: Write>(
        &self,
        writer: &mut W,
        format: &OutputFormat,
        command: &NodeCommands,
    ) -> Result<(), CliError> {
        self.validate_gateway_url()?;

        match command {
            NodeCommands::List => {
                let list = self.list_nodes().await?;
                format.write(writer, &list)?;
            }
            NodeCommands::Info { id } => {
                let detail = self.get_node_info(id).await?;
                format.write(writer, &detail)?;
            }
            NodeCommands::Drain { id, force } => {
                self.drain_node(id, *force).await?;
                let msg = Message::success(format!("Node {id} marked for draining"));
                format.write(writer, &msg)?;
            }
        }
        Ok(())
    }

    /// Validate the gateway URL format.
    fn validate_gateway_url(&self) -> Result<(), CliError> {
        if !self.gateway_url.starts_with("ws://") && !self.gateway_url.starts_with("wss://") {
            return Err(CliError::Config(format!(
                "invalid gateway URL: {}, must start with ws:// or wss://",
                self.gateway_url
            )));
        }
        Ok(())
    }

    /// List all nodes in the cluster.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list_nodes(&self) -> Result<NodeList, CliError> {
        // TODO: Replace with actual gateway call
        // This placeholder will be replaced when gateway client is implemented
        Ok(NodeList { nodes: vec![] })
    }

    /// Get detailed information about a specific node.
    ///
    /// # Errors
    ///
    /// Returns an error if the node is not found or the request fails.
    pub async fn get_node_info(&self, node_id: &str) -> Result<NodeDetail, CliError> {
        // Validate node ID format (should be a UUID or valid identifier)
        if node_id.is_empty() {
            return Err(CliError::InvalidArgument("node ID cannot be empty".into()));
        }

        // TODO: Replace with actual gateway call
        // For now, return a not-found error as we have no real data
        Err(CliError::NodeNotFound(node_id.to_string()))
    }

    /// Mark a node for draining.
    ///
    /// # Errors
    ///
    /// Returns an error if the node is not found or the operation fails.
    pub async fn drain_node(&self, node_id: &str, _force: bool) -> Result<(), CliError> {
        // Validate node ID
        if node_id.is_empty() {
            return Err(CliError::InvalidArgument("node ID cannot be empty".into()));
        }

        // TODO: Replace with actual gateway call
        // For now, return a not-found error
        Err(CliError::NodeNotFound(node_id.to_string()))
    }
}

/// Node client trait for testing.
pub trait NodeClient: Send + Sync {
    /// List all nodes.
    fn list_nodes(&self) -> impl std::future::Future<Output = Result<NodeList, CliError>> + Send;

    /// Get node details.
    fn get_node(&self, id: &str) -> impl std::future::Future<Output = Result<NodeDetail, CliError>> + Send;

    /// Drain a node.
    fn drain_node(&self, id: &str, force: bool) -> impl std::future::Future<Output = Result<(), CliError>> + Send;
}

/// Fake node client for testing.
#[cfg(test)]
pub struct FakeNodeClient {
    nodes: Vec<(NodeInfo, NodeDetail)>,
    drained: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

#[cfg(test)]
impl FakeNodeClient {
    /// Create a new fake client with no nodes.
    pub fn new() -> Self {
        Self {
            nodes: vec![],
            drained: std::sync::Arc::new(std::sync::Mutex::new(vec![])),
        }
    }

    /// Add a node to the fake client.
    #[must_use]
    pub fn with_node(mut self, info: NodeInfo, detail: NodeDetail) -> Self {
        self.nodes.push((info, detail));
        self
    }

    /// Check if a node was drained.
    pub fn was_drained(&self, id: &str) -> bool {
        let drained = self.drained.lock().expect("lock");
        drained.contains(&id.to_string())
    }
}

#[cfg(test)]
impl NodeClient for FakeNodeClient {
    async fn list_nodes(&self) -> Result<NodeList, CliError> {
        let nodes = self.nodes.iter().map(|(info, _)| info.clone()).collect();
        Ok(NodeList { nodes })
    }

    async fn get_node(&self, id: &str) -> Result<NodeDetail, CliError> {
        self.nodes
            .iter()
            .find(|(info, _)| info.id == id)
            .map(|(_, detail)| detail.clone())
            .ok_or_else(|| CliError::NodeNotFound(id.to_string()))
    }

    async fn drain_node(&self, id: &str, _force: bool) -> Result<(), CliError> {
        // Check if node exists
        if !self.nodes.iter().any(|(info, _)| info.id == id) {
            return Err(CliError::NodeNotFound(id.to_string()));
        }

        let mut drained = self.drained.lock().expect("lock");
        drained.push(id.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Format;

    fn make_test_node() -> (NodeInfo, NodeDetail) {
        let info = NodeInfo {
            id: "node-abc-123".into(),
            hostname: "gpu-worker-01".into(),
            status: "healthy".into(),
            gpu_count: 2,
            vram_mib: 49152,
            workloads: 1,
        };

        let detail = NodeDetail {
            id: "node-abc-123".into(),
            hostname: "gpu-worker-01".into(),
            status: "healthy".into(),
            cpu_cores: 32,
            memory_mib: 131072,
            gpus: vec![
                GpuDetail {
                    index: 0,
                    name: "NVIDIA RTX 4090".into(),
                    memory_mib: 24576,
                    uuid: "GPU-001".into(),
                    utilization_percent: Some(45),
                    temperature_celsius: Some(62),
                },
                GpuDetail {
                    index: 1,
                    name: "NVIDIA RTX 4090".into(),
                    memory_mib: 24576,
                    uuid: "GPU-002".into(),
                    utilization_percent: Some(80),
                    temperature_celsius: Some(71),
                },
            ],
            workloads: vec![WorkloadSummary {
                id: "wl-001".into(),
                image: "pytorch:latest".into(),
                state: "running".into(),
            }],
        };

        (info, detail)
    }

    #[test]
    fn node_command_new() {
        let cmd = NodeCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[test]
    fn node_command_validates_gateway_url() {
        let cmd = NodeCommand::new("http://invalid");
        let result = cmd.validate_gateway_url();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid gateway URL"));
    }

    #[test]
    fn node_command_accepts_valid_ws_url() {
        let cmd = NodeCommand::new("ws://localhost:8080");
        assert!(cmd.validate_gateway_url().is_ok());
    }

    #[test]
    fn node_command_accepts_valid_wss_url() {
        let cmd = NodeCommand::new("wss://secure:443");
        assert!(cmd.validate_gateway_url().is_ok());
    }

    #[tokio::test]
    async fn node_command_list_empty() {
        let cmd = NodeCommand::new("ws://localhost:8080");
        let list = cmd.list_nodes().await.expect("should list");
        assert!(list.nodes.is_empty());
    }

    #[tokio::test]
    async fn node_command_info_empty_id() {
        let cmd = NodeCommand::new("ws://localhost:8080");
        let result = cmd.get_node_info("").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[tokio::test]
    async fn node_command_info_not_found() {
        let cmd = NodeCommand::new("ws://localhost:8080");
        let result = cmd.get_node_info("nonexistent").await;
        assert!(matches!(result, Err(CliError::NodeNotFound(_))));
    }

    #[tokio::test]
    async fn node_command_drain_empty_id() {
        let cmd = NodeCommand::new("ws://localhost:8080");
        let result = cmd.drain_node("", false).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[tokio::test]
    async fn fake_client_list_empty() {
        let client = FakeNodeClient::new();
        let list = client.list_nodes().await.expect("should list");
        assert!(list.nodes.is_empty());
    }

    #[tokio::test]
    async fn fake_client_list_with_nodes() {
        let (info, detail) = make_test_node();
        let client = FakeNodeClient::new().with_node(info.clone(), detail);

        let list = client.list_nodes().await.expect("should list");
        assert_eq!(list.nodes.len(), 1);
        assert_eq!(list.nodes[0].id, "node-abc-123");
        assert_eq!(list.nodes[0].gpu_count, 2);
    }

    #[tokio::test]
    async fn fake_client_get_node_found() {
        let (info, detail) = make_test_node();
        let client = FakeNodeClient::new().with_node(info, detail);

        let result = client.get_node("node-abc-123").await.expect("should find");
        assert_eq!(result.id, "node-abc-123");
        assert_eq!(result.hostname, "gpu-worker-01");
        assert_eq!(result.gpus.len(), 2);
    }

    #[tokio::test]
    async fn fake_client_get_node_not_found() {
        let client = FakeNodeClient::new();
        let result = client.get_node("nonexistent").await;
        assert!(matches!(result, Err(CliError::NodeNotFound(_))));
    }

    #[tokio::test]
    async fn fake_client_drain_node_success() {
        let (info, detail) = make_test_node();
        let client = FakeNodeClient::new().with_node(info, detail);

        client.drain_node("node-abc-123", false).await.expect("should drain");
        assert!(client.was_drained("node-abc-123"));
    }

    #[tokio::test]
    async fn fake_client_drain_node_not_found() {
        let client = FakeNodeClient::new();
        let result = client.drain_node("nonexistent", false).await;
        assert!(matches!(result, Err(CliError::NodeNotFound(_))));
    }

    #[tokio::test]
    async fn node_command_execute_list_table() {
        let cmd = NodeCommand::new("ws://localhost:8080");
        let format = OutputFormat::new(Format::Table);
        let mut buf = Vec::new();

        cmd.execute(&mut buf, &format, &NodeCommands::List)
            .await
            .expect("should execute");

        let output = String::from_utf8(buf).expect("valid utf8");
        assert!(output.contains("No nodes in cluster"));
    }

    #[tokio::test]
    async fn node_command_execute_list_json() {
        let cmd = NodeCommand::new("ws://localhost:8080");
        let format = OutputFormat::new(Format::Json);
        let mut buf = Vec::new();

        cmd.execute(&mut buf, &format, &NodeCommands::List)
            .await
            .expect("should execute");

        let output = String::from_utf8(buf).expect("valid utf8");
        assert!(output.contains("\"nodes\""));
    }

    #[tokio::test]
    async fn node_command_execute_info_not_found() {
        let cmd = NodeCommand::new("ws://localhost:8080");
        let format = OutputFormat::new(Format::Table);
        let mut buf = Vec::new();

        let result = cmd
            .execute(&mut buf, &format, &NodeCommands::Info { id: "xyz".into() })
            .await;

        assert!(matches!(result, Err(CliError::NodeNotFound(_))));
    }

    #[tokio::test]
    async fn node_command_execute_drain_not_found() {
        let cmd = NodeCommand::new("ws://localhost:8080");
        let format = OutputFormat::new(Format::Table);
        let mut buf = Vec::new();

        let result = cmd
            .execute(
                &mut buf,
                &format,
                &NodeCommands::Drain {
                    id: "xyz".into(),
                    force: false,
                },
            )
            .await;

        assert!(matches!(result, Err(CliError::NodeNotFound(_))));
    }

    #[tokio::test]
    async fn node_command_execute_invalid_gateway() {
        let cmd = NodeCommand::new("http://invalid");
        let format = OutputFormat::new(Format::Table);
        let mut buf = Vec::new();

        let result = cmd.execute(&mut buf, &format, &NodeCommands::List).await;
        assert!(matches!(result, Err(CliError::Config(_))));
    }
}
