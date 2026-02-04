//! Gateway integration tests.
//!
//! Tests end-to-end integration between clawnode and gateway protocols.
//! Uses a mock WebSocket server to verify message flows.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use claw_proto::{
    GatewayMessage, GpuCapability, GpuMetricsProto, NodeCapabilities, NodeId, NodeMessage,
    WorkloadId, WorkloadSpec, WorkloadState,
};
use clawnode::error::NodeError;
use clawnode::gateway::{
    AutoReconnectClient, AutoReconnectEvent, ConnectionState, GatewayEvent, GatewayHandle,
    HeartbeatConfig, ReconnectConfig,
};
use clawnode::metrics::MetricsReport;
use futures::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_async, WebSocketStream};

// ============================================================================
// Test Helpers - Mock Gateway Server
// ============================================================================

/// A mock gateway server for testing.
struct MockGateway {
    listener: TcpListener,
    addr: SocketAddr,
}

impl MockGateway {
    /// Create a new mock gateway bound to an available port.
    async fn new() -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        Ok(Self { listener, addr })
    }

    /// Get the WebSocket URL for this gateway.
    fn url(&self) -> String {
        format!("ws://{}", self.addr)
    }

    /// Accept a single connection and return the WebSocket stream.
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

    /// Accept and handle registration, sending back a Registered response.
    async fn accept_and_register(&self) -> Result<(WebSocketStream<TcpStream>, NodeId), NodeError> {
        let mut ws = self.accept().await?;

        // Read registration message
        let msg = ws
            .next()
            .await
            .ok_or_else(|| NodeError::GatewayConnection("no message received".into()))?
            .map_err(|e| NodeError::GatewayConnection(e.to_string()))?;

        let node_msg: NodeMessage = match msg {
            Message::Text(text) => NodeMessage::from_json(&text)
                .map_err(|e| NodeError::GatewayConnection(e.to_string()))?,
            _ => return Err(NodeError::GatewayConnection("expected text message".into())),
        };

        let node_id = match node_msg {
            NodeMessage::Register { node_id, .. } => node_id,
            _ => return Err(NodeError::GatewayConnection("expected Register message".into())),
        };

        // Send Registered response
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
// Test Helpers - Configuration
// ============================================================================

fn test_capabilities() -> NodeCapabilities {
    NodeCapabilities::new(4, 16384)
        .with_gpu(GpuCapability {
            index: 0,
            name: "Test GPU".to_string(),
            memory_mib: 24576,
            uuid: "GPU-test-0".to_string(),
        })
        .with_runtime("docker")
}

// ============================================================================
// Registration Flow Tests
// ============================================================================

#[tokio::test]
async fn test_registration_sends_correct_message() {
    // Arrange: Start mock gateway
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    // Create gateway handle
    let handle = GatewayHandle::new(url.clone(), node_id, "test-node", capabilities.clone());

    // Act: Connect and register in background, capture the registration message
    let gateway_task = tokio::spawn(async move {
        let mut ws = gateway.accept().await.expect("failed to accept connection");

        // Read registration message
        let msg = ws.next().await.expect("no message").expect("ws error");
        let text = match msg {
            Message::Text(t) => t.to_string(),
            _ => panic!("expected text message"),
        };
        NodeMessage::from_json(&text).expect("failed to parse")
    });

    let connect_result = handle.send_registration(&capabilities).await;

    // Get the captured registration message
    let received = timeout(Duration::from_secs(5), gateway_task)
        .await
        .expect("timeout waiting for gateway")
        .expect("gateway task failed");

    // Assert: Registration message has correct fields
    match received {
        NodeMessage::Register {
            node_id: recv_node_id,
            name,
            capabilities: recv_caps,
            protocol_version,
        } => {
            assert_eq!(recv_node_id, node_id);
            assert_eq!(name, "test-node");
            assert_eq!(recv_caps.cpu_cores, 4);
            assert_eq!(recv_caps.memory_mib, 16384);
            assert_eq!(recv_caps.gpus.len(), 1);
            assert_eq!(recv_caps.gpus[0].name, "Test GPU");
            assert_eq!(protocol_version, 1);
        }
        _ => panic!("expected Register message, got {:?}", received),
    }

    assert!(connect_result.is_ok());
}

#[tokio::test]
async fn test_registration_receives_registered_response() {
    // Arrange
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let handle = GatewayHandle::new(url.clone(), node_id, "test-node", capabilities.clone());

    // Gateway task: accept, read registration, send Registered response
    let gateway_task = tokio::spawn(async move {
        let (ws, recv_node_id) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept and register");
        (ws, recv_node_id)
    });

    // Act: Send registration and wait for response
    let result = handle.send_registration(&capabilities).await;

    // Wait for gateway to complete
    let _ = timeout(Duration::from_secs(5), gateway_task)
        .await
        .expect("timeout waiting for gateway");

    // Assert: Registration succeeded
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_registration_failure_when_gateway_unavailable() {
    // Arrange: Use URL that doesn't exist
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let handle = GatewayHandle::new(
        "ws://127.0.0.1:1".to_string(), // Non-routable address
        node_id,
        "test-node",
        capabilities.clone(),
    );

    // Act: Try to register
    let result = handle.send_registration(&capabilities).await;

    // Assert: Should fail
    assert!(result.is_err());
}

// ============================================================================
// Heartbeat Tests
// ============================================================================

#[tokio::test]
async fn test_heartbeat_sends_correct_message() {
    // Arrange
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let handle = GatewayHandle::new(url.clone(), node_id, "test-node", capabilities.clone());

    // Gateway task: accept, register, then read heartbeat
    let gateway_task = tokio::spawn(async move {
        let (mut ws, _) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept and register");

        // Read heartbeat message
        let msg = ws.next().await.expect("no message").expect("ws error");
        let text = match msg {
            Message::Text(t) => t.to_string(),
            _ => panic!("expected text message"),
        };
        NodeMessage::from_json(&text).expect("failed to parse")
    });

    // Connect first
    handle
        .send_registration(&capabilities)
        .await
        .expect("registration failed");

    // Act: Send heartbeat
    let before_heartbeat = Utc::now();
    handle.send_heartbeat().await.expect("heartbeat failed");
    let after_heartbeat = Utc::now();

    // Get captured heartbeat
    let received = timeout(Duration::from_secs(5), gateway_task)
        .await
        .expect("timeout waiting for gateway")
        .expect("gateway task failed");

    // Assert
    match received {
        NodeMessage::Heartbeat {
            node_id: recv_node_id,
            timestamp,
        } => {
            assert_eq!(recv_node_id, node_id);
            assert!(timestamp >= before_heartbeat);
            assert!(timestamp <= after_heartbeat);
        }
        _ => panic!("expected Heartbeat message, got {:?}", received),
    }
}

#[tokio::test]
async fn test_heartbeat_receives_ack() {
    // Arrange
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let handle = GatewayHandle::new(url.clone(), node_id, "test-node", capabilities.clone());

    // Gateway task: accept, register, read heartbeat, send ack
    let gateway_task = tokio::spawn(async move {
        let (mut ws, _) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept and register");

        // Read heartbeat
        let _ = ws.next().await.expect("no message").expect("ws error");

        // Send HeartbeatAck
        let ack = GatewayMessage::heartbeat_ack();
        let json = ack.to_json().expect("serialize failed");
        ws.send(Message::Text(json.into()))
            .await
            .expect("send failed");
    });

    // Connect
    handle
        .send_registration(&capabilities)
        .await
        .expect("registration failed");

    // Act
    let result = handle.send_heartbeat().await;

    // Wait for gateway
    let _ = timeout(Duration::from_secs(5), gateway_task).await;

    // Assert
    assert!(result.is_ok());
}

// ============================================================================
// Metrics Reporting Tests
// ============================================================================

#[tokio::test]
async fn test_send_metrics_correct_format() {
    // Arrange
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let handle = GatewayHandle::new(url.clone(), node_id, "test-node", capabilities.clone());

    let report = MetricsReport::new(
        node_id,
        vec![GpuMetricsProto {
            index: 0,
            utilization_percent: 75,
            memory_used_mib: 12000,
            memory_total_mib: 24576,
            temperature_celsius: 68,
            power_watts: Some(320.0),
        }],
    );

    // Gateway task
    let gateway_task = tokio::spawn(async move {
        let (mut ws, _) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept and register");

        // Read metrics message
        let msg = ws.next().await.expect("no message").expect("ws error");
        let text = match msg {
            Message::Text(t) => t.to_string(),
            _ => panic!("expected text message"),
        };
        NodeMessage::from_json(&text).expect("failed to parse")
    });

    // Connect
    handle
        .send_registration(&capabilities)
        .await
        .expect("registration failed");

    // Act
    handle.send_metrics(&report).await.expect("send metrics failed");

    // Get captured metrics
    let received = timeout(Duration::from_secs(5), gateway_task)
        .await
        .expect("timeout waiting for gateway")
        .expect("gateway task failed");

    // Assert
    match received {
        NodeMessage::Metrics {
            node_id: recv_node_id,
            gpu_metrics,
            timestamp: _,
        } => {
            assert_eq!(recv_node_id, node_id);
            assert_eq!(gpu_metrics.len(), 1);
            assert_eq!(gpu_metrics[0].utilization_percent, 75);
            assert_eq!(gpu_metrics[0].memory_used_mib, 12000);
            assert_eq!(gpu_metrics[0].temperature_celsius, 68);
        }
        _ => panic!("expected Metrics message, got {:?}", received),
    }
}

// ============================================================================
// Workload Command Handling Tests
// ============================================================================

#[tokio::test]
async fn test_receive_start_workload_command() {
    // Arrange
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let handle = GatewayHandle::new(url.clone(), node_id, "test-node", capabilities.clone());

    let workload_id = WorkloadId::new();
    let spec = WorkloadSpec::new("nginx:latest")
        .with_gpu_count(1)
        .with_memory_mb(4096);

    // Gateway task: accept, register, send StartWorkload command
    let gateway_task = tokio::spawn(async move {
        let (mut ws, _) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept and register");

        // Send StartWorkload command
        let cmd = GatewayMessage::StartWorkload {
            workload_id,
            spec: spec.clone(),
        };
        let json = cmd.to_json().expect("serialize failed");
        ws.send(Message::Text(json.into()))
            .await
            .expect("send failed");

        (ws, workload_id, spec)
    });

    // Connect
    handle
        .send_registration(&capabilities)
        .await
        .expect("registration failed");

    // First, receive the Registered response
    let reg_event = timeout(Duration::from_secs(5), handle.recv())
        .await
        .expect("timeout waiting for registered")
        .expect("recv failed");
    assert!(matches!(reg_event, GatewayEvent::Message(GatewayMessage::Registered { .. })));

    // Act: Receive the StartWorkload command
    let event = timeout(Duration::from_secs(5), handle.recv())
        .await
        .expect("timeout waiting for event")
        .expect("recv failed");

    // Get gateway results
    let (_, expected_workload_id, expected_spec) = timeout(Duration::from_secs(5), gateway_task)
        .await
        .expect("timeout")
        .expect("gateway failed");

    // Assert
    match event {
        GatewayEvent::Message(GatewayMessage::StartWorkload { workload_id, spec }) => {
            assert_eq!(workload_id, expected_workload_id);
            assert_eq!(spec.image, expected_spec.image);
            assert_eq!(spec.gpu_count, 1);
            assert_eq!(spec.memory_mb, 4096);
        }
        _ => panic!("expected StartWorkload message, got {:?}", event),
    }
}

#[tokio::test]
async fn test_receive_stop_workload_command() {
    // Arrange
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let handle = GatewayHandle::new(url.clone(), node_id, "test-node", capabilities.clone());

    let workload_id = WorkloadId::new();

    // Gateway task
    let gateway_task = tokio::spawn(async move {
        let (mut ws, _) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept and register");

        // Send StopWorkload command
        let cmd = GatewayMessage::StopWorkload {
            workload_id,
            grace_period_secs: 30,
        };
        let json = cmd.to_json().expect("serialize failed");
        ws.send(Message::Text(json.into()))
            .await
            .expect("send failed");

        workload_id
    });

    // Connect
    handle
        .send_registration(&capabilities)
        .await
        .expect("registration failed");

    // First, receive the Registered response
    let reg_event = timeout(Duration::from_secs(5), handle.recv())
        .await
        .expect("timeout waiting for registered")
        .expect("recv failed");
    assert!(matches!(reg_event, GatewayEvent::Message(GatewayMessage::Registered { .. })));

    // Act: Receive the StopWorkload command
    let event = timeout(Duration::from_secs(5), handle.recv())
        .await
        .expect("timeout waiting for event")
        .expect("recv failed");

    let expected_workload_id = timeout(Duration::from_secs(5), gateway_task)
        .await
        .expect("timeout")
        .expect("gateway failed");

    // Assert
    match event {
        GatewayEvent::Message(GatewayMessage::StopWorkload {
            workload_id,
            grace_period_secs,
        }) => {
            assert_eq!(workload_id, expected_workload_id);
            assert_eq!(grace_period_secs, 30);
        }
        _ => panic!("expected StopWorkload message, got {:?}", event),
    }
}

// ============================================================================
// Workload Status Reporting Tests
// ============================================================================

#[tokio::test]
async fn test_send_workload_update() {
    // Arrange
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let handle = GatewayHandle::new(url.clone(), node_id, "test-node", capabilities.clone());

    let workload_id = WorkloadId::new();

    // Gateway task
    let gateway_task = tokio::spawn(async move {
        let (mut ws, _) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept and register");

        // Read workload update
        let msg = ws.next().await.expect("no message").expect("ws error");
        let text = match msg {
            Message::Text(t) => t.to_string(),
            _ => panic!("expected text message"),
        };
        NodeMessage::from_json(&text).expect("failed to parse")
    });

    // Connect
    handle
        .send_registration(&capabilities)
        .await
        .expect("registration failed");

    // Act
    handle
        .send_workload_update(workload_id, WorkloadState::Running, Some("Started successfully".into()))
        .await
        .expect("send update failed");

    // Get captured message
    let received = timeout(Duration::from_secs(5), gateway_task)
        .await
        .expect("timeout waiting for gateway")
        .expect("gateway task failed");

    // Assert
    match received {
        NodeMessage::WorkloadUpdate {
            workload_id: recv_id,
            state,
            message,
            timestamp: _,
        } => {
            assert_eq!(recv_id, workload_id);
            assert_eq!(state, WorkloadState::Running);
            assert_eq!(message, Some("Started successfully".to_string()));
        }
        _ => panic!("expected WorkloadUpdate message, got {:?}", received),
    }
}

// ============================================================================
// Message Routing Tests
// ============================================================================

#[tokio::test]
async fn test_message_routing_to_handlers() {
    // Arrange
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let (handler_tx, mut handler_rx) = mpsc::channel::<GatewayMessage>(32);

    let handle = GatewayHandle::new(url.clone(), node_id, "test-node", capabilities.clone())
        .with_message_handler(handler_tx);

    let workload_id = WorkloadId::new();
    let spec = WorkloadSpec::new("nginx:latest");

    // Gateway task
    let gateway_task = tokio::spawn(async move {
        let (mut ws, _) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept and register");

        // Send multiple commands
        let start = GatewayMessage::StartWorkload {
            workload_id,
            spec: spec.clone(),
        };
        ws.send(Message::Text(start.to_json().expect("json").into()))
            .await
            .expect("send failed");

        let stop = GatewayMessage::StopWorkload {
            workload_id,
            grace_period_secs: 10,
        };
        ws.send(Message::Text(stop.to_json().expect("json").into()))
            .await
            .expect("send failed");
    });

    // Connect and start routing
    handle
        .send_registration(&capabilities)
        .await
        .expect("registration failed");

    // Start message routing BEFORE waiting for messages
    handle.start_routing();

    // Act: Receive routed messages (Registered will be first, then StartWorkload, then StopWorkload)
    let msg1 = timeout(Duration::from_secs(5), handler_rx.recv())
        .await
        .expect("timeout")
        .expect("recv failed");

    // First message should be the Registered response
    assert!(matches!(msg1, GatewayMessage::Registered { .. }), "expected Registered, got {:?}", msg1);

    let msg2 = timeout(Duration::from_secs(5), handler_rx.recv())
        .await
        .expect("timeout")
        .expect("recv failed");

    let msg3 = timeout(Duration::from_secs(5), handler_rx.recv())
        .await
        .expect("timeout")
        .expect("recv failed");

    // Wait for gateway
    let _ = timeout(Duration::from_secs(5), gateway_task).await;

    // Assert: msg2 should be StartWorkload, msg3 should be StopWorkload
    assert!(matches!(msg2, GatewayMessage::StartWorkload { .. }), "expected StartWorkload, got {:?}", msg2);
    assert!(matches!(msg3, GatewayMessage::StopWorkload { .. }), "expected StopWorkload, got {:?}", msg3);
}

// ============================================================================
// Connection State Tests
// ============================================================================

#[tokio::test]
async fn test_connection_state_transitions() {
    // Arrange
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let handle = GatewayHandle::new(url.clone(), node_id, "test-node", capabilities.clone());

    // Assert: Initial state is disconnected
    assert_eq!(handle.state(), ConnectionState::Disconnected);

    // Gateway task
    let gateway_task = tokio::spawn(async move {
        let (ws, _) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept and register");
        ws
    });

    // Act: Connect
    handle
        .send_registration(&capabilities)
        .await
        .expect("registration failed");

    // Wait for gateway
    let _ = timeout(Duration::from_secs(5), gateway_task).await;

    // Assert: State is now connected
    assert_eq!(handle.state(), ConnectionState::Connected);

    // Act: Stop
    handle.stop();

    // Assert: State is now disconnected
    assert_eq!(handle.state(), ConnectionState::Disconnected);
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_handles_gateway_error_message() {
    // Arrange
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let handle = GatewayHandle::new(url.clone(), node_id, "test-node", capabilities.clone());

    // Gateway task: send error message
    let gateway_task = tokio::spawn(async move {
        let (mut ws, _) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept and register");

        let err = GatewayMessage::error(500, "Internal server error");
        ws.send(Message::Text(err.to_json().expect("json").into()))
            .await
            .expect("send failed");
    });

    // Connect
    handle
        .send_registration(&capabilities)
        .await
        .expect("registration failed");

    // First, receive the Registered response
    let reg_event = timeout(Duration::from_secs(5), handle.recv())
        .await
        .expect("timeout waiting for registered")
        .expect("recv failed");
    assert!(matches!(reg_event, GatewayEvent::Message(GatewayMessage::Registered { .. })));

    // Act: Receive the Error message
    let event = timeout(Duration::from_secs(5), handle.recv())
        .await
        .expect("timeout")
        .expect("recv failed");

    // Wait for gateway
    let _ = timeout(Duration::from_secs(5), gateway_task).await;

    // Assert
    match event {
        GatewayEvent::Message(GatewayMessage::Error { code, message }) => {
            assert_eq!(code, 500);
            assert_eq!(message, "Internal server error");
        }
        _ => panic!("expected Error message, got {:?}", event),
    }
}

// ============================================================================
// AutoReconnectClient Integration Tests
// ============================================================================

#[tokio::test]
async fn test_auto_reconnect_client_connects_and_registers() {
    // Arrange
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let client = AutoReconnectClient::new(url.clone(), node_id, "test-node", capabilities.clone());

    // Gateway task: accept and verify registration
    let gateway_task = tokio::spawn(async move {
        let (ws, recv_node_id) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept and register");
        (ws, recv_node_id)
    });

    // Act: Start the client
    let (_tx, mut rx) = client.start();

    // Should receive Connected event
    let event = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timeout waiting for Connected")
        .expect("channel closed");

    assert!(
        matches!(event, AutoReconnectEvent::Connected),
        "expected Connected, got {:?}",
        event
    );

    // Should receive Registered message
    let event = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timeout waiting for Registered")
        .expect("channel closed");

    assert!(
        matches!(
            event,
            AutoReconnectEvent::Message(GatewayMessage::Registered { .. })
        ),
        "expected Registered message, got {:?}",
        event
    );

    // Verify gateway received correct node_id
    let (_, recv_node_id) = timeout(Duration::from_secs(5), gateway_task)
        .await
        .expect("timeout")
        .expect("gateway failed");

    assert_eq!(recv_node_id, node_id);

    client.stop();
}

#[tokio::test]
async fn test_auto_reconnect_client_sends_messages() {
    // Arrange
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let client = AutoReconnectClient::new(url.clone(), node_id, "test-node", capabilities.clone());

    // Gateway task: accept, register, then read a message
    let gateway_task = tokio::spawn(async move {
        let (mut ws, _) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept and register");

        // Read additional message (workload update)
        let msg = ws.next().await.expect("no message").expect("ws error");
        let text = match msg {
            Message::Text(t) => t.to_string(),
            _ => panic!("expected text message"),
        };
        NodeMessage::from_json(&text).expect("failed to parse")
    });

    // Start client
    let (tx, mut rx) = client.start();

    // Wait for Connected
    let _ = timeout(Duration::from_secs(5), rx.recv()).await;
    // Wait for Registered
    let _ = timeout(Duration::from_secs(5), rx.recv()).await;

    // Act: Send a workload update
    let workload_id = WorkloadId::new();
    let msg = NodeMessage::workload_update(workload_id, WorkloadState::Running, None);
    tx.send(msg).await.expect("send failed");

    // Verify gateway received the message
    let received = timeout(Duration::from_secs(5), gateway_task)
        .await
        .expect("timeout")
        .expect("gateway failed");

    match received {
        NodeMessage::WorkloadUpdate {
            workload_id: recv_id,
            state,
            ..
        } => {
            assert_eq!(recv_id, workload_id);
            assert_eq!(state, WorkloadState::Running);
        }
        _ => panic!("expected WorkloadUpdate, got {:?}", received),
    }

    client.stop();
}

#[tokio::test]
async fn test_auto_reconnect_client_receives_commands() {
    // Arrange
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let client = AutoReconnectClient::new(url.clone(), node_id, "test-node", capabilities.clone());

    let workload_id = WorkloadId::new();
    let spec = WorkloadSpec::new("nginx:latest").with_gpu_count(2);

    // Gateway task: accept, register, send StartWorkload
    let gateway_task = tokio::spawn(async move {
        let (mut ws, _) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept and register");

        // Send StartWorkload command
        let cmd = GatewayMessage::StartWorkload {
            workload_id,
            spec: spec.clone(),
        };
        ws.send(Message::Text(cmd.to_json().expect("json").into()))
            .await
            .expect("send failed");

        (workload_id, spec)
    });

    // Start client
    let (_tx, mut rx) = client.start();

    // Skip Connected event
    let _ = timeout(Duration::from_secs(5), rx.recv()).await;
    // Skip Registered message
    let _ = timeout(Duration::from_secs(5), rx.recv()).await;

    // Act: Receive StartWorkload command
    let event = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timeout waiting for StartWorkload")
        .expect("channel closed");

    let (expected_workload_id, expected_spec) = timeout(Duration::from_secs(5), gateway_task)
        .await
        .expect("timeout")
        .expect("gateway failed");

    // Assert
    match event {
        AutoReconnectEvent::Message(GatewayMessage::StartWorkload { workload_id, spec }) => {
            assert_eq!(workload_id, expected_workload_id);
            assert_eq!(spec.image, expected_spec.image);
            assert_eq!(spec.gpu_count, 2);
        }
        _ => panic!("expected StartWorkload message, got {:?}", event),
    }

    client.stop();
}

#[tokio::test]
async fn test_auto_reconnect_client_reconnects_on_disconnect() {
    // Arrange: Create a gateway that will accept one connection then close it
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let reconnect_config = ReconnectConfig {
        initial_delay: Duration::from_millis(10),
        max_delay: Duration::from_millis(100),
        backoff_multiplier: 2.0,
        max_attempts: Some(3),
    };

    let client = AutoReconnectClient::new(url.clone(), node_id, "test-node", capabilities.clone())
        .with_reconnect_config(reconnect_config);

    // Gateway task: accept first connection, close it, accept second connection
    let gateway_task = tokio::spawn(async move {
        // First connection - close immediately after registration
        let (ws, _) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept first connection");
        drop(ws); // Close connection

        // Second connection - keep open
        let (ws, node_id) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept second connection");
        (ws, node_id)
    });

    // Start client
    let (_tx, mut rx) = client.start();

    // First connection: Connected
    let event = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timeout")
        .expect("closed");
    assert!(matches!(event, AutoReconnectEvent::Connected));

    // First connection: Registered
    let event = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timeout")
        .expect("closed");
    assert!(matches!(
        event,
        AutoReconnectEvent::Message(GatewayMessage::Registered { .. })
    ));

    // Should receive Disconnected after server closes
    let event = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timeout waiting for Disconnected")
        .expect("closed");
    assert!(
        matches!(event, AutoReconnectEvent::Disconnected { .. }),
        "expected Disconnected, got {:?}",
        event
    );

    // Should receive Connected again after reconnect
    let event = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timeout waiting for reconnect Connected")
        .expect("closed");
    assert!(
        matches!(event, AutoReconnectEvent::Connected),
        "expected Connected after reconnect, got {:?}",
        event
    );

    // Should receive Registered again
    let event = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timeout")
        .expect("closed");
    assert!(matches!(
        event,
        AutoReconnectEvent::Message(GatewayMessage::Registered { .. })
    ));

    // Verify gateway accepted both connections
    let _ = timeout(Duration::from_secs(5), gateway_task).await;

    client.stop();
}

#[tokio::test]
async fn test_auto_reconnect_client_fails_after_max_attempts() {
    // Arrange: Use non-existent URL
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let reconnect_config = ReconnectConfig {
        initial_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(10),
        backoff_multiplier: 2.0,
        max_attempts: Some(3),
    };

    let client = AutoReconnectClient::new(
        "ws://127.0.0.1:1", // Non-routable
        node_id,
        "test-node",
        capabilities.clone(),
    )
    .with_reconnect_config(reconnect_config);

    // Start client
    let (_tx, mut rx) = client.start();

    // Should receive Reconnecting events
    let mut reconnect_count = 0;
    loop {
        let event = timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("timeout")
            .expect("closed");

        match event {
            AutoReconnectEvent::Reconnecting { attempt, .. } => {
                reconnect_count = attempt;
            }
            AutoReconnectEvent::ReconnectFailed { attempts, .. } => {
                assert_eq!(attempts, 3);
                break;
            }
            _ => {}
        }
    }

    assert!(reconnect_count >= 2, "should have made reconnect attempts");
    assert_eq!(client.state(), ConnectionState::Failed);
}

#[tokio::test]
async fn test_auto_reconnect_client_with_heartbeat() {
    // Arrange
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let heartbeat_config = HeartbeatConfig {
        interval: Duration::from_millis(50),
        ack_timeout: Duration::from_secs(1),
        max_missed_acks: 10,
    };

    let client = AutoReconnectClient::new(url.clone(), node_id, "test-node", capabilities.clone())
        .with_heartbeat(heartbeat_config);

    // Gateway task: accept, register, read heartbeats, send acks
    let heartbeat_count = Arc::new(AtomicU32::new(0));
    let heartbeat_count_clone = Arc::clone(&heartbeat_count);

    let gateway_task = tokio::spawn(async move {
        let (mut ws, _) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept and register");

        // Read heartbeats and send acks
        for _ in 0..3 {
            if let Some(Ok(Message::Text(text))) = ws.next().await {
                if let Ok(NodeMessage::Heartbeat { .. }) = NodeMessage::from_json(&text) {
                    heartbeat_count_clone.fetch_add(1, Ordering::SeqCst);

                    // Send ack
                    let ack = GatewayMessage::heartbeat_ack();
                    let _ = ws.send(Message::Text(ack.to_json().expect("json").into())).await;
                }
            }
        }
    });

    // Start client
    let (_tx, mut rx) = client.start();

    // Wait for connection
    let _ = timeout(Duration::from_secs(5), rx.recv()).await; // Connected
    let _ = timeout(Duration::from_secs(5), rx.recv()).await; // Registered

    // Wait for heartbeats to be exchanged
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify heartbeats were received
    let _ = timeout(Duration::from_secs(5), gateway_task).await;

    let count = heartbeat_count.load(Ordering::SeqCst);
    assert!(count >= 2, "expected at least 2 heartbeats, got {}", count);

    client.stop();
}

#[tokio::test]
async fn test_auto_reconnect_client_heartbeat_ack_resets_counter() {
    // Arrange
    let gateway = MockGateway::new().await.expect("failed to create mock gateway");
    let url = gateway.url();
    let node_id = NodeId::new();
    let capabilities = test_capabilities();

    let heartbeat_config = HeartbeatConfig {
        interval: Duration::from_millis(20),
        ack_timeout: Duration::from_secs(1),
        max_missed_acks: 2, // Will stop quickly if acks not received
    };

    let client = AutoReconnectClient::new(url.clone(), node_id, "test-node", capabilities.clone())
        .with_heartbeat(heartbeat_config);

    // Gateway task: accept, register, send acks for heartbeats
    // Keep connection open longer to allow state check
    let gateway_task = tokio::spawn(async move {
        let (mut ws, _) = gateway
            .accept_and_register()
            .await
            .expect("failed to accept and register");

        // Keep connection alive by acking heartbeats
        for _ in 0..10 {
            if let Some(Ok(Message::Text(text))) = ws.next().await {
                if NodeMessage::from_json(&text)
                    .map(|m| matches!(m, NodeMessage::Heartbeat { .. }))
                    .unwrap_or(false)
                {
                    let ack = GatewayMessage::heartbeat_ack();
                    let _ = ws.send(Message::Text(ack.to_json().expect("json").into())).await;
                }
            }
        }
        // Keep connection open briefly after acks
        tokio::time::sleep(Duration::from_millis(50)).await;
    });

    // Start client
    let (_tx, mut rx) = client.start();

    // Wait for connection
    let _ = timeout(Duration::from_secs(5), rx.recv()).await; // Connected
    let _ = timeout(Duration::from_secs(5), rx.recv()).await; // Registered

    // Receive heartbeat acks and verify connection stays up
    let mut ack_count = 0;
    for _ in 0..15 {
        if let Ok(Some(event)) = timeout(Duration::from_millis(50), rx.recv()).await {
            if matches!(
                event,
                AutoReconnectEvent::Message(GatewayMessage::HeartbeatAck { .. })
            ) {
                ack_count += 1;
                // Check state while acks are still coming - connection should be up
                if ack_count == 3 {
                    assert_eq!(
                        client.state(),
                        ConnectionState::Connected,
                        "connection should stay up while receiving acks"
                    );
                }
            }
        }
    }

    assert!(ack_count >= 3, "expected at least 3 acks, got {}", ack_count);

    // Wait for gateway task to complete
    let _ = timeout(Duration::from_secs(5), gateway_task).await;

    client.stop();
}
