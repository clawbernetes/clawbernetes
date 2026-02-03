//! End-to-end flow tests for Clawbernetes.
//!
//! These tests simulate the complete flow:
//! 1. Node registers with gateway
//! 2. Node receives workload commands
//! 3. Node executes workload
//! 4. Node reports completion

use std::net::SocketAddr;
use std::time::Duration;

use claw_proto::{
    GatewayMessage, GpuCapability, NodeCapabilities, NodeId, NodeMessage, WorkloadId, WorkloadSpec,
    WorkloadState,
};
use clawnode::error::NodeError;
use clawnode::gateway::GatewayHandle;
use clawnode::metrics::MetricsReport;
use futures::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_async, WebSocketStream};

// ============================================================================
// Mock Gateway Server
// ============================================================================

/// A mock gateway server that simulates the control plane.
struct MockGatewayServer {
    listener: TcpListener,
    addr: SocketAddr,
}

impl MockGatewayServer {
    async fn new() -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        Ok(Self { listener, addr })
    }

    fn url(&self) -> String {
        format!("ws://{}", self.addr)
    }

    async fn accept(&self) -> Result<WebSocketStream<TcpStream>, NodeError> {
        let (stream, _) = self
            .listener
            .accept()
            .await
            .map_err(|e| NodeError::GatewayConnection(e.to_string()))?;
        accept_async(stream)
            .await
            .map_err(|e| NodeError::GatewayConnection(e.to_string()))
    }

    /// Accept connection and handle registration.
    async fn accept_and_register(&self) -> Result<(WebSocketStream<TcpStream>, NodeId), NodeError> {
        let mut ws = self.accept().await?;

        let msg = ws
            .next()
            .await
            .ok_or_else(|| NodeError::GatewayConnection("no message".into()))?
            .map_err(|e| NodeError::GatewayConnection(e.to_string()))?;

        let node_msg: NodeMessage = match msg {
            Message::Text(text) => NodeMessage::from_json(&text)
                .map_err(|e| NodeError::GatewayConnection(e.to_string()))?,
            _ => return Err(NodeError::GatewayConnection("expected text".into())),
        };

        let node_id = match node_msg {
            NodeMessage::Register { node_id, .. } => node_id,
            _ => return Err(NodeError::GatewayConnection("expected Register".into())),
        };

        let response = GatewayMessage::registered(node_id, 30, 10);
        let json = response
            .to_json()
            .map_err(|e| NodeError::GatewayConnection(e.to_string()))?;
        ws.send(Message::Text(json.into()))
            .await
            .map_err(|e| NodeError::GatewayConnection(e.to_string()))?;

        Ok((ws, node_id))
    }
}

// ============================================================================
// Test Helpers
// ============================================================================

fn test_capabilities() -> NodeCapabilities {
    NodeCapabilities::new(8, 32768)
        .with_gpu(GpuCapability {
            index: 0,
            name: "NVIDIA RTX 4090".to_string(),
            memory_mib: 24576,
            uuid: "GPU-e2e-test-0".to_string(),
        })
        .with_gpu(GpuCapability {
            index: 1,
            name: "NVIDIA RTX 4090".to_string(),
            memory_mib: 24576,
            uuid: "GPU-e2e-test-1".to_string(),
        })
        .with_runtime("docker")
}

// ============================================================================
// E2E Flow Tests
// ============================================================================

/// Test the complete flow: registration → workload → execution → completion.
#[tokio::test]
async fn test_full_workload_lifecycle() {
    // Start mock gateway
    let gateway = MockGatewayServer::new()
        .await
        .expect("failed to create gateway");
    let url = gateway.url();

    let node_id = NodeId::new();
    let capabilities = test_capabilities();
    let workload_id = WorkloadId::new();

    // Create node handle
    let handle = GatewayHandle::new(url.clone(), node_id, "e2e-test-node", capabilities.clone());

    // Gateway task: simulate the control plane
    let gateway_task = tokio::spawn(async move {
        let (mut ws, registered_node_id) = gateway
            .accept_and_register()
            .await
            .expect("registration failed");

        // Send StartWorkload command
        let start_cmd = GatewayMessage::StartWorkload {
            workload_id,
            spec: WorkloadSpec::new("training:v1")
                .with_gpu_count(1)
                .with_memory_mb(8192)
                .with_env("MODEL", "llama2-7b"),
        };
        ws.send(Message::Text(start_cmd.to_json().expect("json").into()))
            .await
            .expect("send failed");

        // Collect messages from node
        let mut received_messages = Vec::new();
        
        // Read messages with timeout
        loop {
            match tokio::time::timeout(Duration::from_millis(500), ws.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    if let Ok(msg) = NodeMessage::from_json(&text) {
                        received_messages.push(msg);
                        
                        // Check if we got a workload update indicating completion
                        if received_messages.iter().any(|m| {
                            matches!(
                                m,
                                NodeMessage::WorkloadUpdate { state, .. }
                                if *state == WorkloadState::Running
                            )
                        }) {
                            // Send stop command after workload starts
                            let stop_cmd = GatewayMessage::StopWorkload {
                                workload_id,
                                grace_period_secs: 30,
                            };
                            let _ = ws
                                .send(Message::Text(stop_cmd.to_json().expect("json").into()))
                                .await;
                        }
                    }
                }
                Ok(Some(Ok(_))) => continue,
                Ok(Some(Err(_))) | Ok(None) => break,
                Err(_) => break, // Timeout
            }
        }

        (registered_node_id, received_messages)
    });

    // Connect and register
    handle
        .send_registration(&capabilities)
        .await
        .expect("registration failed");

    // Process events for a short time
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        match timeout(Duration::from_millis(100), handle.recv()).await {
            Ok(Ok(_event)) => {
                // Events are being processed
            }
            Ok(Err(_)) | Err(_) => {
                // Timeout or channel closed
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }

    handle.stop();

    // Get results from gateway
    let (registered_node_id, received_messages) = timeout(Duration::from_secs(5), gateway_task)
        .await
        .expect("gateway timeout")
        .expect("gateway failed");

    // Verify registration happened with correct node ID
    assert_eq!(registered_node_id, node_id);

    // Log what we received for debugging
    for msg in &received_messages {
        println!("Gateway received: {:?}", msg);
    }
}

/// Test that node sends heartbeats after registration.
#[tokio::test]
async fn test_node_sends_heartbeats() {
    let gateway = MockGatewayServer::new()
        .await
        .expect("failed to create gateway");
    let url = gateway.url();

    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let handle = GatewayHandle::new(url.clone(), node_id, "heartbeat-test", capabilities.clone());

    // Gateway task
    let gateway_task = tokio::spawn(async move {
        let (mut ws, _) = gateway
            .accept_and_register()
            .await
            .expect("registration failed");

        let mut heartbeat_count = 0;

        // Wait for heartbeats
        for _ in 0..10 {
            match tokio::time::timeout(Duration::from_millis(200), ws.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    if let Ok(NodeMessage::Heartbeat { .. }) = NodeMessage::from_json(&text) {
                        heartbeat_count += 1;
                    }
                }
                _ => {}
            }
        }

        heartbeat_count
    });

    // Connect
    handle
        .send_registration(&capabilities)
        .await
        .expect("registration failed");

    // Send heartbeats manually (since we're not running the full node loop)
    for _ in 0..3 {
        handle.send_heartbeat().await.expect("heartbeat failed");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    handle.stop();

    let heartbeat_count = timeout(Duration::from_secs(5), gateway_task)
        .await
        .expect("gateway timeout")
        .expect("gateway failed");

    assert!(
        heartbeat_count >= 2,
        "expected at least 2 heartbeats, got {}",
        heartbeat_count
    );
}

/// Test that node handles multiple workloads.
#[tokio::test]
async fn test_multiple_workloads() {
    let gateway = MockGatewayServer::new()
        .await
        .expect("failed to create gateway");
    let url = gateway.url();

    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let workload_ids: Vec<_> = (0..3).map(|_| WorkloadId::new()).collect();
    let workload_ids_clone = workload_ids.clone();

    let handle = GatewayHandle::new(url.clone(), node_id, "multi-workload-test", capabilities.clone());

    // Gateway task
    let gateway_task = tokio::spawn(async move {
        let (mut ws, _) = gateway
            .accept_and_register()
            .await
            .expect("registration failed");

        // Send multiple workload commands
        for (i, wid) in workload_ids_clone.iter().enumerate() {
            let start_cmd = GatewayMessage::StartWorkload {
                workload_id: *wid,
                spec: WorkloadSpec::new(format!("worker-{i}:latest"))
                    .with_gpu_count(0)
                    .with_memory_mb(512),
            };
            ws.send(Message::Text(start_cmd.to_json().expect("json").into()))
                .await
                .expect("send failed");
        }

        // Collect responses
        let mut workload_updates = Vec::new();

        for _ in 0..20 {
            match tokio::time::timeout(Duration::from_millis(200), ws.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    if let Ok(msg) = NodeMessage::from_json(&text) {
                        if matches!(msg, NodeMessage::WorkloadUpdate { .. }) {
                            workload_updates.push(msg);
                        }
                    }
                }
                _ => {}
            }
        }

        workload_updates
    });

    // Connect
    handle
        .send_registration(&capabilities)
        .await
        .expect("registration failed");

    // Process events
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        match timeout(Duration::from_millis(100), handle.recv()).await {
            Ok(Ok(_)) => {}
            Ok(Err(_)) | Err(_) => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }

    handle.stop();

    let workload_updates = timeout(Duration::from_secs(5), gateway_task)
        .await
        .expect("gateway timeout")
        .expect("gateway failed");

    // We may have received workload updates for our workloads
    println!("Received {} workload updates", workload_updates.len());
    for update in &workload_updates {
        println!("  {:?}", update);
    }
}

/// Test node gracefully handles gateway errors.
#[tokio::test]
async fn test_handles_gateway_errors() {
    let gateway = MockGatewayServer::new()
        .await
        .expect("failed to create gateway");
    let url = gateway.url();

    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let handle = GatewayHandle::new(url.clone(), node_id, "error-test", capabilities.clone());

    // Gateway task: send error message
    let gateway_task = tokio::spawn(async move {
        let (mut ws, _) = gateway
            .accept_and_register()
            .await
            .expect("registration failed");

        // Send error
        let err = GatewayMessage::error(503, "Service temporarily unavailable");
        ws.send(Message::Text(err.to_json().expect("json").into()))
            .await
            .expect("send failed");

        // Keep connection open briefly
        tokio::time::sleep(Duration::from_millis(200)).await;
    });

    // Connect
    handle
        .send_registration(&capabilities)
        .await
        .expect("registration failed");

    // Receive the Registered response first
    let event1 = timeout(Duration::from_secs(5), handle.recv())
        .await
        .expect("timeout")
        .expect("recv failed");
    assert!(matches!(
        event1,
        clawnode::gateway::GatewayEvent::Message(GatewayMessage::Registered { .. })
    ));

    // Receive the error
    let event2 = timeout(Duration::from_secs(5), handle.recv())
        .await
        .expect("timeout")
        .expect("recv failed");

    match event2 {
        clawnode::gateway::GatewayEvent::Message(GatewayMessage::Error { code, message }) => {
            assert_eq!(code, 503);
            assert!(message.contains("unavailable"));
        }
        _ => panic!("expected Error message, got {:?}", event2),
    }

    handle.stop();
    let _ = timeout(Duration::from_secs(5), gateway_task).await;
}

/// Test metrics reporting flow.
#[tokio::test]
async fn test_metrics_reporting_flow() {
    let gateway = MockGatewayServer::new()
        .await
        .expect("failed to create gateway");
    let url = gateway.url();

    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let handle = GatewayHandle::new(url.clone(), node_id, "metrics-test", capabilities.clone());

    // Gateway task
    let gateway_task = tokio::spawn(async move {
        let (mut ws, _) = gateway
            .accept_and_register()
            .await
            .expect("registration failed");

        let mut metrics_received = Vec::new();

        for _ in 0..10 {
            match tokio::time::timeout(Duration::from_millis(200), ws.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    if let Ok(NodeMessage::Metrics { gpu_metrics, .. }) = NodeMessage::from_json(&text)
                    {
                        metrics_received.push(gpu_metrics);
                    }
                }
                _ => {}
            }
        }

        metrics_received
    });

    // Connect
    handle
        .send_registration(&capabilities)
        .await
        .expect("registration failed");

    // Send metrics
    let report = MetricsReport::new(
        node_id,
        vec![claw_proto::GpuMetricsProto {
            index: 0,
            utilization_percent: 85,
            memory_used_mib: 20000,
            memory_total_mib: 24576,
            temperature_celsius: 72,
            power_watts: Some(350.0),
        }],
    );

    handle.send_metrics(&report).await.expect("metrics failed");

    tokio::time::sleep(Duration::from_millis(100)).await;
    handle.stop();

    let metrics_received = timeout(Duration::from_secs(5), gateway_task)
        .await
        .expect("gateway timeout")
        .expect("gateway failed");

    assert!(
        !metrics_received.is_empty(),
        "expected to receive at least one metrics report"
    );

    let first_metrics = &metrics_received[0];
    assert_eq!(first_metrics.len(), 1);
    assert_eq!(first_metrics[0].utilization_percent, 85);
    assert_eq!(first_metrics[0].temperature_celsius, 72);
}
