//! End-to-end tests for Gateway ↔ CLI ↔ Node integration.
//!
//! These tests verify:
//! 1. Gateway startup and CLI connection
//! 2. Node registration and heartbeats
//! 3. Status queries reflect registered nodes
//! 4. Health tracking (healthy/unhealthy/draining)
//! 5. Log streaming from nodes
//! 6. Drain command

mod helpers;

use std::time::Duration;

use claw_proto::cli::{CliMessage, CliResponse, NodeState};
use claw_proto::{GatewayMessage, WorkloadId};
use helpers::*;
use tokio::time::sleep;

// ============================================================================
// Gateway Startup Tests
// ============================================================================

#[tokio::test]
async fn test_gateway_starts_and_accepts_cli_connection() {
    let gateway = TestGateway::start().await;
    
    // Connect CLI client
    let client = TestCliClient::connect(&gateway.ws_url()).await;
    assert!(client.is_ok(), "CLI should connect successfully");
    
    gateway.shutdown().await;
}

#[tokio::test]
async fn test_gateway_status_empty() {
    let gateway = TestGateway::start().await;
    let mut client = TestCliClient::connect(&gateway.ws_url()).await.unwrap();
    
    // Get status
    let response = client.get_status().await.unwrap();
    
    match response {
        CliResponse::Status {
            node_count,
            healthy_nodes,
            gpu_count,
            active_workloads,
            ..
        } => {
            assert_eq!(node_count, 0, "No nodes registered yet");
            assert_eq!(healthy_nodes, 0);
            assert_eq!(gpu_count, 0);
            assert_eq!(active_workloads, 0);
        }
        other => panic!("Expected Status, got {:?}", other),
    }
    
    gateway.shutdown().await;
}

// ============================================================================
// Node Registration Tests
// ============================================================================

#[tokio::test]
async fn test_node_registration() {
    let gateway = TestGateway::start().await;
    
    // Register a node
    let node = TestNode::connect_and_register(
        &gateway.ws_url(),
        "test-node-1",
        test_capabilities(),
    ).await;
    
    assert!(node.is_ok(), "Node should register successfully");
    
    gateway.shutdown().await;
}

#[tokio::test]
async fn test_node_registration_with_gpu() {
    let gateway = TestGateway::start().await;
    
    // Register a node with GPU
    let node = TestNode::connect_and_register(
        &gateway.ws_url(),
        "gpu-node-1",
        test_capabilities_with_gpu(),
    ).await;
    
    assert!(node.is_ok(), "GPU node should register successfully");
    
    gateway.shutdown().await;
}

#[tokio::test]
async fn test_status_reflects_registered_nodes() {
    let gateway = TestGateway::start().await;
    
    // Register multiple nodes
    let _node1 = TestNode::connect_and_register(
        &gateway.ws_url(),
        "node-1",
        test_capabilities(),
    ).await.unwrap();
    
    let _node2 = TestNode::connect_and_register(
        &gateway.ws_url(),
        "node-2",
        test_capabilities_with_gpu(),
    ).await.unwrap();
    
    // Give gateway time to process
    sleep(Duration::from_millis(100)).await;
    
    // Connect CLI and check status
    let mut client = TestCliClient::connect(&gateway.ws_url()).await.unwrap();
    let response = client.get_status().await.unwrap();
    
    match response {
        CliResponse::Status {
            node_count,
            healthy_nodes,
            gpu_count,
            ..
        } => {
            assert_eq!(node_count, 2, "Should have 2 nodes");
            assert_eq!(healthy_nodes, 2, "Both should be healthy");
            assert_eq!(gpu_count, 1, "One GPU from node-2");
        }
        other => panic!("Expected Status, got {:?}", other),
    }
    
    gateway.shutdown().await;
}

// ============================================================================
// Node Listing Tests
// ============================================================================

#[tokio::test]
async fn test_list_nodes_empty() {
    let gateway = TestGateway::start().await;
    let mut client = TestCliClient::connect(&gateway.ws_url()).await.unwrap();
    
    let response = client.list_nodes().await.unwrap();
    
    match response {
        CliResponse::Nodes { nodes } => {
            assert!(nodes.is_empty(), "No nodes registered");
        }
        other => panic!("Expected Nodes, got {:?}", other),
    }
    
    gateway.shutdown().await;
}

#[tokio::test]
async fn test_list_nodes_with_registered() {
    let gateway = TestGateway::start().await;
    
    // Register nodes
    let node1 = TestNode::connect_and_register(
        &gateway.ws_url(),
        "worker-1",
        test_capabilities_with_gpu(),
    ).await.unwrap();
    
    sleep(Duration::from_millis(100)).await;
    
    // List nodes via CLI
    let mut client = TestCliClient::connect(&gateway.ws_url()).await.unwrap();
    let response = client.list_nodes().await.unwrap();
    
    match response {
        CliResponse::Nodes { nodes } => {
            assert_eq!(nodes.len(), 1);
            assert_eq!(nodes[0].node_id, node1.node_id);
            assert_eq!(nodes[0].gpu_count, 1);
            assert_eq!(nodes[0].state, NodeState::Healthy);
        }
        other => panic!("Expected Nodes, got {:?}", other),
    }
    
    gateway.shutdown().await;
}

// ============================================================================
// Heartbeat Tests
// ============================================================================

#[tokio::test]
async fn test_node_heartbeat() {
    let gateway = TestGateway::start().await;
    
    let mut node = TestNode::connect_and_register(
        &gateway.ws_url(),
        "heartbeat-test",
        test_capabilities(),
    ).await.unwrap();
    
    // Send heartbeat
    let response = node.send_heartbeat().await.unwrap();
    
    match response {
        GatewayMessage::HeartbeatAck { .. } => {
            // Success
        }
        other => panic!("Expected HeartbeatAck, got {:?}", other),
    }
    
    gateway.shutdown().await;
}

#[tokio::test]
async fn test_multiple_heartbeats() {
    let gateway = TestGateway::start().await;
    
    let mut node = TestNode::connect_and_register(
        &gateway.ws_url(),
        "multi-heartbeat",
        test_capabilities(),
    ).await.unwrap();
    
    // Send multiple heartbeats
    for i in 0..5 {
        let response = node.send_heartbeat().await.unwrap();
        assert!(
            matches!(response, GatewayMessage::HeartbeatAck { .. }),
            "Heartbeat {} should be acknowledged",
            i
        );
        sleep(Duration::from_millis(50)).await;
    }
    
    gateway.shutdown().await;
}

// ============================================================================
// Drain Command Tests
// ============================================================================

#[tokio::test]
async fn test_drain_node() {
    let gateway = TestGateway::start().await;
    
    // Register a node
    let node = TestNode::connect_and_register(
        &gateway.ws_url(),
        "drain-test",
        test_capabilities(),
    ).await.unwrap();
    
    sleep(Duration::from_millis(100)).await;
    
    // Connect CLI and drain the node
    let mut client = TestCliClient::connect(&gateway.ws_url()).await.unwrap();
    
    let response = client.request(CliMessage::DrainNode {
        node_id: node.node_id,
        drain: true,
    }).await.unwrap();
    
    match response {
        CliResponse::NodeDrained { node_id, draining } => {
            assert_eq!(node_id, node.node_id);
            assert!(draining, "Node should be draining");
        }
        other => panic!("Expected NodeDrained, got {:?}", other),
    }
    
    // Verify node is draining in list
    let response = client.request(CliMessage::ListNodes {
        state_filter: None,
        include_capabilities: false,
    }).await.unwrap();
    
    match response {
        CliResponse::Nodes { nodes } => {
            assert_eq!(nodes.len(), 1);
            assert_eq!(nodes[0].state, NodeState::Draining);
        }
        other => panic!("Expected Nodes, got {:?}", other),
    }
    
    gateway.shutdown().await;
}

#[tokio::test]
async fn test_undrain_node() {
    let gateway = TestGateway::start().await;
    
    let node = TestNode::connect_and_register(
        &gateway.ws_url(),
        "undrain-test",
        test_capabilities(),
    ).await.unwrap();
    
    sleep(Duration::from_millis(100)).await;
    
    let mut client = TestCliClient::connect(&gateway.ws_url()).await.unwrap();
    
    // Drain then undrain
    client.request(CliMessage::DrainNode {
        node_id: node.node_id,
        drain: true,
    }).await.unwrap();
    
    let response = client.request(CliMessage::DrainNode {
        node_id: node.node_id,
        drain: false,
    }).await.unwrap();
    
    match response {
        CliResponse::NodeDrained { draining, .. } => {
            assert!(!draining, "Node should not be draining");
        }
        other => panic!("Expected NodeDrained, got {:?}", other),
    }
    
    // Verify node is healthy again
    let response = client.list_nodes().await.unwrap();
    match response {
        CliResponse::Nodes { nodes } => {
            assert_eq!(nodes[0].state, NodeState::Healthy);
        }
        other => panic!("Expected Nodes, got {:?}", other),
    }
    
    gateway.shutdown().await;
}

// ============================================================================
// Log Streaming Tests
// ============================================================================

#[tokio::test]
async fn test_node_sends_logs() {
    let gateway = TestGateway::start().await;
    
    let mut node = TestNode::connect_and_register(
        &gateway.ws_url(),
        "log-test",
        test_capabilities(),
    ).await.unwrap();
    
    let workload_id = WorkloadId::new();
    
    // Send some logs
    node.send_logs(workload_id, vec![
        "Starting container...".into(),
        "Container started".into(),
        "Running workload".into(),
    ], false).await.unwrap();
    
    // Send stderr logs
    node.send_logs(workload_id, vec![
        "Warning: something happened".into(),
    ], true).await.unwrap();
    
    sleep(Duration::from_millis(100)).await;
    
    // Query logs via CLI
    let mut client = TestCliClient::connect(&gateway.ws_url()).await.unwrap();
    let response = client.request(CliMessage::GetLogs {
        workload_id,
        tail: None,
        include_stderr: true,
    }).await.unwrap();
    
    match response {
        CliResponse::Logs {
            workload_id: resp_id,
            stdout_lines,
            stderr_lines,
        } => {
            assert_eq!(resp_id, workload_id);
            assert_eq!(stdout_lines.len(), 3);
            assert_eq!(stderr_lines.len(), 1);
            assert_eq!(stdout_lines[0], "Starting container...");
            assert_eq!(stderr_lines[0], "Warning: something happened");
        }
        other => panic!("Expected Logs, got {:?}", other),
    }
    
    gateway.shutdown().await;
}

#[tokio::test]
async fn test_log_tail() {
    let gateway = TestGateway::start().await;
    
    let mut node = TestNode::connect_and_register(
        &gateway.ws_url(),
        "log-tail-test",
        test_capabilities(),
    ).await.unwrap();
    
    let workload_id = WorkloadId::new();
    
    // Send many logs
    let logs: Vec<String> = (0..100).map(|i| format!("Log line {}", i)).collect();
    node.send_logs(workload_id, logs, false).await.unwrap();
    
    sleep(Duration::from_millis(100)).await;
    
    // Query with tail
    let mut client = TestCliClient::connect(&gateway.ws_url()).await.unwrap();
    let response = client.request(CliMessage::GetLogs {
        workload_id,
        tail: Some(10),
        include_stderr: false,
    }).await.unwrap();
    
    match response {
        CliResponse::Logs { stdout_lines, .. } => {
            assert_eq!(stdout_lines.len(), 10, "Should return last 10 lines");
            assert_eq!(stdout_lines[0], "Log line 90");
            assert_eq!(stdout_lines[9], "Log line 99");
        }
        other => panic!("Expected Logs, got {:?}", other),
    }
    
    gateway.shutdown().await;
}

// ============================================================================
// MOLT Status Tests
// ============================================================================

#[tokio::test]
async fn test_molt_status_without_integration() {
    let gateway = TestGateway::start().await;
    let mut client = TestCliClient::connect(&gateway.ws_url()).await.unwrap();
    
    let response = client.request(CliMessage::GetMoltStatus).await.unwrap();
    
    match response {
        CliResponse::MoltStatus {
            connected,
            peer_count,
            node_id,
            region,
        } => {
            assert!(!connected, "MOLT not configured");
            assert_eq!(peer_count, 0);
            assert!(node_id.is_none());
            assert!(region.is_none());
        }
        other => panic!("Expected MoltStatus, got {:?}", other),
    }
    
    gateway.shutdown().await;
}

#[tokio::test]
async fn test_molt_balance_without_integration() {
    let gateway = TestGateway::start().await;
    let mut client = TestCliClient::connect(&gateway.ws_url()).await.unwrap();
    
    let response = client.request(CliMessage::GetMoltBalance).await.unwrap();
    
    match response {
        CliResponse::MoltBalance {
            balance,
            pending,
            staked,
        } => {
            assert_eq!(balance, 0);
            assert_eq!(pending, 0);
            assert_eq!(staked, 0);
        }
        other => panic!("Expected MoltBalance, got {:?}", other),
    }
    
    gateway.shutdown().await;
}

// ============================================================================
// Ping/Pong Tests
// ============================================================================

#[tokio::test]
async fn test_ping_pong() {
    let gateway = TestGateway::start().await;
    let mut client = TestCliClient::connect(&gateway.ws_url()).await.unwrap();
    
    let timestamp = chrono::Utc::now();
    let response = client.request(CliMessage::Ping { timestamp }).await.unwrap();
    
    match response {
        CliResponse::Pong {
            client_timestamp,
            server_timestamp,
        } => {
            assert_eq!(client_timestamp, timestamp);
            assert!(server_timestamp >= timestamp);
        }
        other => panic!("Expected Pong, got {:?}", other),
    }
    
    gateway.shutdown().await;
}

// ============================================================================
// Concurrent Connections Tests
// ============================================================================

#[tokio::test]
async fn test_multiple_cli_clients() {
    let gateway = TestGateway::start().await;
    
    // Connect multiple CLI clients
    let mut clients = Vec::new();
    for _ in 0..5 {
        let client = TestCliClient::connect(&gateway.ws_url()).await.unwrap();
        clients.push(client);
    }
    
    // All clients should be able to get status
    for client in &mut clients {
        let response = client.get_status().await.unwrap();
        assert!(matches!(response, CliResponse::Status { .. }));
    }
    
    gateway.shutdown().await;
}

#[tokio::test]
async fn test_multiple_nodes() {
    let gateway = TestGateway::start().await;
    
    // Register multiple nodes
    let mut nodes = Vec::new();
    for i in 0..10 {
        let node = TestNode::connect_and_register(
            &gateway.ws_url(),
            &format!("node-{}", i),
            test_capabilities(),
        ).await.unwrap();
        nodes.push(node);
    }
    
    sleep(Duration::from_millis(200)).await;
    
    // Verify all nodes are registered
    let mut client = TestCliClient::connect(&gateway.ws_url()).await.unwrap();
    let response = client.get_status().await.unwrap();
    
    match response {
        CliResponse::Status { node_count, healthy_nodes, .. } => {
            assert_eq!(node_count, 10);
            assert_eq!(healthy_nodes, 10);
        }
        other => panic!("Expected Status, got {:?}", other),
    }
    
    gateway.shutdown().await;
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_get_nonexistent_node() {
    let gateway = TestGateway::start().await;
    let mut client = TestCliClient::connect(&gateway.ws_url()).await.unwrap();
    
    let fake_id = claw_proto::NodeId::new();
    let response = client.request(CliMessage::GetNode { node_id: fake_id }).await.unwrap();
    
    match response {
        CliResponse::Error { code, .. } => {
            assert_eq!(code, claw_proto::cli::error_codes::NODE_NOT_FOUND);
        }
        other => panic!("Expected Error, got {:?}", other),
    }
    
    gateway.shutdown().await;
}

#[tokio::test]
async fn test_drain_nonexistent_node() {
    let gateway = TestGateway::start().await;
    let mut client = TestCliClient::connect(&gateway.ws_url()).await.unwrap();
    
    let fake_id = claw_proto::NodeId::new();
    let response = client.request(CliMessage::DrainNode {
        node_id: fake_id,
        drain: true,
    }).await.unwrap();
    
    match response {
        CliResponse::Error { code, .. } => {
            assert_eq!(code, claw_proto::cli::error_codes::NODE_NOT_FOUND);
        }
        other => panic!("Expected Error, got {:?}", other),
    }
    
    gateway.shutdown().await;
}
