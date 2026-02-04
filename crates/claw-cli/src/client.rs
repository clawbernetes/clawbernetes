//! Gateway WebSocket client for CLI operations.
//!
//! This module provides a WebSocket client that connects to the gateway
//! and sends CLI protocol messages.
//!
//! # Example
//!
//! ```rust,no_run
//! use claw_cli::client::GatewayClient;
//!
//! # async fn example() -> Result<(), claw_cli::CliError> {
//! let mut client = GatewayClient::connect("ws://localhost:8080").await?;
//! let status = client.get_status().await?;
//! println!("Nodes: {}", status.node_count);
//! # Ok(())
//! # }
//! ```

use std::time::Duration;

use claw_proto::cli::{
    self, CliMessage, CliResponse, MoltPeerInfo, NodeInfo, NodeState, WorkloadInfo,
    CLI_PROTOCOL_VERSION,
};
use claw_proto::{NodeId, WorkloadId, WorkloadSpec, WorkloadState};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::{
    connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, trace, warn};

use crate::error::CliError;

/// Default connection timeout.
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Default request timeout.
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Gateway WebSocket client.
pub struct GatewayClient {
    /// WebSocket stream.
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    /// Server version.
    server_version: String,
    /// Request timeout.
    request_timeout: Duration,
}

impl std::fmt::Debug for GatewayClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatewayClient")
            .field("server_version", &self.server_version)
            .field("request_timeout", &self.request_timeout)
            .finish_non_exhaustive()
    }
}

impl GatewayClient {
    /// Connect to the gateway at the given URL.
    ///
    /// Performs the handshake to identify as a CLI client.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The URL is invalid (must start with `ws://` or `wss://`)
    /// - Connection fails
    /// - Handshake fails
    pub async fn connect(url: &str) -> Result<Self, CliError> {
        Self::connect_with_timeout(url, DEFAULT_CONNECT_TIMEOUT).await
    }

    /// Connect with a custom timeout.
    ///
    /// # Errors
    ///
    /// Returns an error if connection or handshake fails.
    pub async fn connect_with_timeout(url: &str, connect_timeout: Duration) -> Result<Self, CliError> {
        // Validate URL
        if !url.starts_with("ws://") && !url.starts_with("wss://") {
            return Err(CliError::Config(format!(
                "invalid gateway URL: {url}, must start with ws:// or wss://"
            )));
        }

        debug!(url = %url, "Connecting to gateway");

        // Connect with timeout
        let (ws, _response) = timeout(connect_timeout, connect_async(url))
            .await
            .map_err(|_| CliError::Timeout("connection timed out".into()))?
            .map_err(|e| CliError::Connection(e.to_string()))?;

        debug!("WebSocket connected, sending handshake");

        let mut client = Self {
            ws,
            server_version: String::new(),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        };

        // Perform handshake
        let hello = CliMessage::hello(env!("CARGO_PKG_VERSION"));
        let response = client.send_request(hello).await?;

        match response {
            CliResponse::Welcome {
                server_version,
                protocol_version,
            } => {
                if protocol_version != CLI_PROTOCOL_VERSION {
                    warn!(
                        server = protocol_version,
                        client = CLI_PROTOCOL_VERSION,
                        "Protocol version mismatch"
                    );
                }
                client.server_version = server_version;
                debug!(version = %client.server_version, "Handshake complete");
                Ok(client)
            }
            CliResponse::Error { code, message, .. } => {
                Err(CliError::Gateway { code, message })
            }
            other => Err(CliError::Protocol(format!(
                "unexpected response to hello: {other:?}"
            ))),
        }
    }

    /// Set the request timeout.
    pub fn set_request_timeout(&mut self, timeout: Duration) {
        self.request_timeout = timeout;
    }

    /// Get the server version.
    #[must_use]
    pub fn server_version(&self) -> &str {
        &self.server_version
    }

    /// Send a request and wait for a response.
    async fn send_request(&mut self, request: CliMessage) -> Result<CliResponse, CliError> {
        let request_type = request.request_type();
        let json = request.to_json().map_err(|e| CliError::Protocol(e.to_string()))?;

        trace!(request_type, "Sending request");
        self.ws
            .send(Message::Text(json))
            .await
            .map_err(|e| CliError::Connection(e.to_string()))?;

        // Wait for response
        let response = timeout(self.request_timeout, self.ws.next())
            .await
            .map_err(|_| CliError::Timeout(format!("request '{request_type}' timed out")))?
            .ok_or_else(|| CliError::Connection("connection closed".into()))?
            .map_err(|e| CliError::Connection(e.to_string()))?;

        match response {
            Message::Text(text) => {
                let response = CliResponse::from_json(&text)
                    .map_err(|e| CliError::Protocol(e.to_string()))?;

                if let CliResponse::Error { code, message, .. } = &response {
                    return Err(CliError::Gateway {
                        code: *code,
                        message: message.clone(),
                    });
                }

                trace!(request_type, "Received response");
                Ok(response)
            }
            Message::Binary(_) => Err(CliError::Protocol("unexpected binary message".into())),
            Message::Close(_) => Err(CliError::Connection("connection closed by server".into())),
            _ => Err(CliError::Protocol("unexpected message type".into())),
        }
    }

    /// Close the connection gracefully.
    pub async fn close(mut self) -> Result<(), CliError> {
        self.ws
            .close(None)
            .await
            .map_err(|e| CliError::Connection(e.to_string()))
    }

    // ========================================================================
    // Status Operations
    // ========================================================================

    /// Get cluster status overview.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn get_status(&mut self) -> Result<ClusterStatus, CliError> {
        let response = self.send_request(CliMessage::GetStatus).await?;

        match response {
            CliResponse::Status {
                node_count,
                healthy_nodes,
                gpu_count,
                active_workloads,
                total_vram_mib,
                gateway_version,
                uptime_secs,
            } => Ok(ClusterStatus {
                node_count,
                healthy_nodes,
                gpu_count,
                active_workloads,
                total_vram_mib,
                gateway_version,
                uptime_secs,
            }),
            other => Err(CliError::Protocol(format!(
                "unexpected response: {other:?}"
            ))),
        }
    }

    // ========================================================================
    // Node Operations
    // ========================================================================

    /// List all nodes.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list_nodes(
        &mut self,
        state_filter: Option<NodeState>,
        include_capabilities: bool,
    ) -> Result<Vec<NodeInfo>, CliError> {
        let response = self
            .send_request(CliMessage::ListNodes {
                state_filter,
                include_capabilities,
            })
            .await?;

        match response {
            CliResponse::Nodes { nodes } => Ok(nodes),
            other => Err(CliError::Protocol(format!(
                "unexpected response: {other:?}"
            ))),
        }
    }

    /// Get detailed info for a specific node.
    ///
    /// # Errors
    ///
    /// Returns an error if the node is not found or the request fails.
    pub async fn get_node(&mut self, node_id: NodeId) -> Result<NodeInfo, CliError> {
        let response = self.send_request(CliMessage::GetNode { node_id }).await?;

        match response {
            CliResponse::Node { node } => Ok(node),
            other => Err(CliError::Protocol(format!(
                "unexpected response: {other:?}"
            ))),
        }
    }

    // ========================================================================
    // Workload Operations
    // ========================================================================

    /// List workloads.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list_workloads(
        &mut self,
        node_filter: Option<NodeId>,
        state_filter: Option<WorkloadState>,
    ) -> Result<Vec<WorkloadInfo>, CliError> {
        let response = self
            .send_request(CliMessage::ListWorkloads {
                node_filter,
                state_filter,
            })
            .await?;

        match response {
            CliResponse::Workloads { workloads } => Ok(workloads),
            other => Err(CliError::Protocol(format!(
                "unexpected response: {other:?}"
            ))),
        }
    }

    /// Get detailed info for a specific workload.
    ///
    /// # Errors
    ///
    /// Returns an error if the workload is not found or the request fails.
    pub async fn get_workload(&mut self, workload_id: WorkloadId) -> Result<WorkloadInfo, CliError> {
        let response = self
            .send_request(CliMessage::GetWorkload { workload_id })
            .await?;

        match response {
            CliResponse::Workload { workload } => Ok(workload),
            other => Err(CliError::Protocol(format!(
                "unexpected response: {other:?}"
            ))),
        }
    }

    /// Start a new workload.
    ///
    /// # Errors
    ///
    /// Returns an error if there's no capacity or the request fails.
    pub async fn start_workload(
        &mut self,
        node_id: Option<NodeId>,
        spec: WorkloadSpec,
    ) -> Result<(WorkloadId, NodeId), CliError> {
        let response = self
            .send_request(CliMessage::StartWorkload { node_id, spec })
            .await?;

        match response {
            CliResponse::WorkloadStarted {
                workload_id,
                node_id,
            } => Ok((workload_id, node_id)),
            other => Err(CliError::Protocol(format!(
                "unexpected response: {other:?}"
            ))),
        }
    }

    /// Stop a workload.
    ///
    /// # Errors
    ///
    /// Returns an error if the workload is not found or the request fails.
    pub async fn stop_workload(
        &mut self,
        workload_id: WorkloadId,
        force: bool,
    ) -> Result<(), CliError> {
        let response = self
            .send_request(CliMessage::StopWorkload { workload_id, force })
            .await?;

        match response {
            CliResponse::WorkloadStopped { .. } => Ok(()),
            other => Err(CliError::Protocol(format!(
                "unexpected response: {other:?}"
            ))),
        }
    }

    /// Get workload logs.
    ///
    /// # Errors
    ///
    /// Returns an error if the workload is not found or the request fails.
    pub async fn get_logs(
        &mut self,
        workload_id: WorkloadId,
        tail: Option<u32>,
        include_stderr: bool,
    ) -> Result<WorkloadLogs, CliError> {
        let response = self
            .send_request(CliMessage::GetLogs {
                workload_id,
                tail,
                include_stderr,
            })
            .await?;

        match response {
            CliResponse::Logs {
                workload_id,
                stdout_lines,
                stderr_lines,
            } => Ok(WorkloadLogs {
                workload_id,
                stdout_lines,
                stderr_lines,
            }),
            other => Err(CliError::Protocol(format!(
                "unexpected response: {other:?}"
            ))),
        }
    }

    // ========================================================================
    // MOLT Operations
    // ========================================================================

    /// Get MOLT network status.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn get_molt_status(&mut self) -> Result<MoltStatus, CliError> {
        let response = self.send_request(CliMessage::GetMoltStatus).await?;

        match response {
            CliResponse::MoltStatus {
                connected,
                peer_count,
                node_id,
                region,
            } => Ok(MoltStatus {
                connected,
                peer_count,
                node_id,
                region,
            }),
            other => Err(CliError::Protocol(format!(
                "unexpected response: {other:?}"
            ))),
        }
    }

    /// List MOLT peers.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list_molt_peers(&mut self) -> Result<Vec<MoltPeerInfo>, CliError> {
        let response = self.send_request(CliMessage::ListMoltPeers).await?;

        match response {
            CliResponse::MoltPeers { peers } => Ok(peers),
            other => Err(CliError::Protocol(format!(
                "unexpected response: {other:?}"
            ))),
        }
    }

    /// Get MOLT wallet balance.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn get_molt_balance(&mut self) -> Result<MoltBalance, CliError> {
        let response = self.send_request(CliMessage::GetMoltBalance).await?;

        match response {
            CliResponse::MoltBalance {
                balance,
                pending,
                staked,
            } => Ok(MoltBalance {
                balance,
                pending,
                staked,
            }),
            other => Err(CliError::Protocol(format!(
                "unexpected response: {other:?}"
            ))),
        }
    }

    /// Ping the gateway.
    ///
    /// Returns the round-trip time in milliseconds.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn ping(&mut self) -> Result<u64, CliError> {
        let start = std::time::Instant::now();
        let request = CliMessage::ping();

        let response = self.send_request(request).await?;

        match response {
            CliResponse::Pong { .. } => Ok(start.elapsed().as_millis() as u64),
            other => Err(CliError::Protocol(format!(
                "unexpected response: {other:?}"
            ))),
        }
    }
}

// ============================================================================
// Response Types
// ============================================================================

/// Cluster status information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterStatus {
    /// Total number of nodes.
    pub node_count: u32,
    /// Number of healthy nodes.
    pub healthy_nodes: u32,
    /// Total number of GPUs.
    pub gpu_count: u32,
    /// Number of active workloads.
    pub active_workloads: u32,
    /// Total VRAM across all GPUs in MiB.
    pub total_vram_mib: u64,
    /// Gateway version.
    pub gateway_version: String,
    /// Server uptime in seconds.
    pub uptime_secs: u64,
}

/// Workload logs.
#[derive(Debug, Clone)]
pub struct WorkloadLogs {
    /// Workload ID.
    pub workload_id: WorkloadId,
    /// Standard output lines.
    pub stdout_lines: Vec<String>,
    /// Standard error lines.
    pub stderr_lines: Vec<String>,
}

/// MOLT network status.
#[derive(Debug, Clone)]
pub struct MoltStatus {
    /// Whether connected to MOLT network.
    pub connected: bool,
    /// Number of peers.
    pub peer_count: u32,
    /// Local node ID on MOLT.
    pub node_id: Option<String>,
    /// Network region.
    pub region: Option<String>,
}

/// MOLT wallet balance.
#[derive(Debug, Clone, Copy)]
pub struct MoltBalance {
    /// Balance in smallest unit.
    pub balance: u64,
    /// Pending balance.
    pub pending: u64,
    /// Staked amount.
    pub staked: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_status_equality() {
        let s1 = ClusterStatus {
            node_count: 5,
            healthy_nodes: 4,
            gpu_count: 12,
            active_workloads: 3,
            total_vram_mib: 245760,
            gateway_version: "0.1.0".into(),
            uptime_secs: 3600,
        };
        let s2 = s1.clone();
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_molt_balance_copy() {
        let b1 = MoltBalance {
            balance: 1000,
            pending: 100,
            staked: 500,
        };
        let b2 = b1;
        assert_eq!(b1.balance, b2.balance);
    }

    #[tokio::test]
    async fn test_invalid_url_rejected() {
        let result = GatewayClient::connect("http://invalid").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid gateway URL"));
    }

    #[tokio::test]
    async fn test_connection_timeout() {
        // Try to connect to a non-routable address
        let result = GatewayClient::connect_with_timeout(
            "ws://10.255.255.1:9999",
            Duration::from_millis(100),
        )
        .await;

        assert!(result.is_err());
    }
}
