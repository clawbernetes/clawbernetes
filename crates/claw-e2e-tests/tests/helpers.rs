//! Test helpers for E2E tests.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use claw_gateway_server::{GatewayServer, ServerConfig};
use claw_proto::cli::{CliMessage, CliResponse};
use claw_proto::{GatewayMessage, NodeCapabilities, NodeId, NodeMessage};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage, MaybeTlsStream, WebSocketStream};

/// Default test timeout.
pub const TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Find an available port for testing.
pub async fn find_available_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

/// Test gateway that manages its own lifecycle.
pub struct TestGateway {
    pub addr: SocketAddr,
    pub server: Arc<Mutex<GatewayServer>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl TestGateway {
    /// Start a new test gateway on an available port.
    pub async fn start() -> Self {
        let port = find_available_port().await;
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
        
        let config = ServerConfig::new(addr)
            .with_max_connections(100)
            .with_heartbeat_interval(Duration::from_secs(5));
        
        let mut server = GatewayServer::new(config);
        let server = Arc::new(Mutex::new(server));
        
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        
        let server_clone = server.clone();
        let handle = tokio::spawn(async move {
            let mut srv = server_clone.lock().await;
            tokio::select! {
                result = srv.serve(addr) => {
                    if let Err(e) = result {
                        eprintln!("Server error: {}", e);
                    }
                }
                _ = shutdown_rx => {
                    // Graceful shutdown
                }
            }
        });
        
        // Wait for server to be ready
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        Self {
            addr,
            server,
            shutdown_tx: Some(shutdown_tx),
            handle: Some(handle),
        }
    }
    
    /// Get the WebSocket URL for this gateway.
    pub fn ws_url(&self) -> String {
        format!("ws://{}", self.addr)
    }
    
    /// Shutdown the gateway.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = timeout(Duration::from_secs(2), handle).await;
        }
    }
}

/// CLI client for testing.
pub struct TestCliClient {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl TestCliClient {
    /// Connect to a gateway as a CLI client.
    pub async fn connect(url: &str) -> Result<Self, String> {
        let (ws, _) = connect_async(url)
            .await
            .map_err(|e| format!("Failed to connect: {}", e))?;
        
        let mut client = Self { ws };
        
        // Send Hello
        client.send_cli_message(&CliMessage::Hello {
            version: "test-1.0".into(),
            protocol_version: 1,
        }).await?;
        
        // Wait for Welcome
        let response = client.recv_cli_response().await?;
        match response {
            CliResponse::Welcome { .. } => Ok(client),
            other => Err(format!("Expected Welcome, got {:?}", other)),
        }
    }
    
    /// Send a CLI message.
    pub async fn send_cli_message(&mut self, msg: &CliMessage) -> Result<(), String> {
        let json = msg.to_json().map_err(|e| e.to_string())?;
        self.ws.send(WsMessage::Text(json)).await.map_err(|e| e.to_string())
    }
    
    /// Receive a CLI response.
    pub async fn recv_cli_response(&mut self) -> Result<CliResponse, String> {
        let msg = timeout(TEST_TIMEOUT, self.ws.next())
            .await
            .map_err(|_| "Timeout waiting for response")?
            .ok_or("Connection closed")?
            .map_err(|e| e.to_string())?;
        
        match msg {
            WsMessage::Text(text) => {
                CliResponse::from_json(&text).map_err(|e| e.to_string())
            }
            _ => Err("Expected text message".into()),
        }
    }
    
    /// Send a request and get the response.
    pub async fn request(&mut self, msg: CliMessage) -> Result<CliResponse, String> {
        self.send_cli_message(&msg).await?;
        self.recv_cli_response().await
    }
    
    /// Get gateway status.
    pub async fn get_status(&mut self) -> Result<CliResponse, String> {
        self.request(CliMessage::GetStatus).await
    }
    
    /// List nodes.
    pub async fn list_nodes(&mut self) -> Result<CliResponse, String> {
        self.request(CliMessage::ListNodes {
            state_filter: None,
            include_capabilities: false,
        }).await
    }
}

/// Simulated node for testing.
pub struct TestNode {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    pub node_id: NodeId,
    pub name: String,
}

impl TestNode {
    /// Connect and register a node.
    pub async fn connect_and_register(
        url: &str,
        name: &str,
        capabilities: NodeCapabilities,
    ) -> Result<Self, String> {
        let (ws, _) = connect_async(url)
            .await
            .map_err(|e| format!("Failed to connect: {}", e))?;
        
        let node_id = NodeId::new();
        let mut node = Self {
            ws,
            node_id,
            name: name.into(),
        };
        
        // Send Register
        node.send_node_message(&NodeMessage::register(
            node_id,
            name,
            capabilities,
        )).await?;
        
        // Wait for Registered
        let response = node.recv_gateway_message().await?;
        match response {
            GatewayMessage::Registered { node_id: resp_id, .. } => {
                if resp_id != node_id {
                    return Err("Node ID mismatch".into());
                }
                Ok(node)
            }
            GatewayMessage::Error { message, .. } => {
                Err(format!("Registration failed: {}", message))
            }
            other => Err(format!("Expected Registered, got {:?}", other)),
        }
    }
    
    /// Send a node message.
    pub async fn send_node_message(&mut self, msg: &NodeMessage) -> Result<(), String> {
        let json = msg.to_json().map_err(|e| e.to_string())?;
        self.ws.send(WsMessage::Text(json)).await.map_err(|e| e.to_string())
    }
    
    /// Receive a gateway message.
    pub async fn recv_gateway_message(&mut self) -> Result<GatewayMessage, String> {
        let msg = timeout(TEST_TIMEOUT, self.ws.next())
            .await
            .map_err(|_| "Timeout waiting for response")?
            .ok_or("Connection closed")?
            .map_err(|e| e.to_string())?;
        
        match msg {
            WsMessage::Text(text) => {
                GatewayMessage::from_json(&text).map_err(|e| e.to_string())
            }
            _ => Err("Expected text message".into()),
        }
    }
    
    /// Send a heartbeat.
    pub async fn send_heartbeat(&mut self) -> Result<GatewayMessage, String> {
        self.send_node_message(&NodeMessage::heartbeat(self.node_id)).await?;
        self.recv_gateway_message().await
    }
    
    /// Send workload logs.
    pub async fn send_logs(
        &mut self,
        workload_id: claw_proto::WorkloadId,
        lines: Vec<String>,
        is_stderr: bool,
    ) -> Result<(), String> {
        self.send_node_message(&NodeMessage::WorkloadLogs {
            workload_id,
            lines,
            is_stderr,
        }).await
    }
}

/// Create test node capabilities.
pub fn test_capabilities() -> NodeCapabilities {
    NodeCapabilities::new(8, 16384)
}

/// Create test capabilities with GPUs.
pub fn test_capabilities_with_gpu() -> NodeCapabilities {
    use claw_proto::GpuCapability;
    
    NodeCapabilities::new(8, 16384)
        .with_gpu(GpuCapability {
            index: 0,
            name: "NVIDIA RTX 4090".into(),
            memory_mib: 24576,
            uuid: "GPU-TEST-001".into(),
        })
}
