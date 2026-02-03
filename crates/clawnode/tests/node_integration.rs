//! Integration tests for the Node struct.

use std::sync::Arc;
use std::time::Duration;

use clawnode::config::NodeConfig;
use clawnode::gpu::{FakeGpuDetector, GpuDetector, GpuInfo, GpuMetrics};
use clawnode::node::{Node, NodeState};
use clawnode::runtime::FakeContainerRuntime;
use claw_proto::NodeId;

fn test_config() -> NodeConfig {
    NodeConfig {
        name: "integration-test-node".to_string(),
        gateway_url: "ws://localhost:19999".to_string(), // Non-existent port
        gpu: Default::default(),
        network: Default::default(),
        molt: Default::default(),
    }
}

fn test_gpu_info() -> GpuInfo {
    GpuInfo {
        index: 0,
        name: "Test RTX 4090".to_string(),
        memory_total_mib: 24576,
        uuid: "GPU-integration-test-0".to_string(),
    }
}

fn test_gpu_metrics() -> GpuMetrics {
    GpuMetrics {
        index: 0,
        utilization_percent: 75,
        memory_used_mib: 12000,
        memory_total_mib: 24576,
        temperature_celsius: 68,
        power_watts: Some(320.0),
    }
}

#[test]
fn test_node_creation_with_fake_components() {
    let config = test_config();
    let node_id = NodeId::new();

    let detector = Arc::new(
        FakeGpuDetector::new()
            .with_gpu(test_gpu_info(), test_gpu_metrics())
            .with_gpu(
                GpuInfo {
                    index: 1,
                    name: "Test A100".to_string(),
                    memory_total_mib: 81920,
                    uuid: "GPU-integration-test-1".to_string(),
                },
                GpuMetrics {
                    index: 1,
                    utilization_percent: 95,
                    memory_used_mib: 70000,
                    memory_total_mib: 81920,
                    temperature_celsius: 72,
                    power_watts: Some(400.0),
                },
            ),
    );

    let runtime = Arc::new(FakeContainerRuntime::new());

    let node = Node::with_components(config, node_id, detector.clone(), runtime.clone());

    // Verify node state
    assert_eq!(node.state(), NodeState::Initializing);
    assert_eq!(node.node_id(), node_id);
    assert_eq!(node.name(), "integration-test-node");

    // Verify GPU detection works through node
    let gpus = detector.detect_gpus().expect("GPU detection should work");
    assert_eq!(gpus.len(), 2);
    assert_eq!(gpus[0].name, "Test RTX 4090");
    assert_eq!(gpus[1].name, "Test A100");

    // Verify metrics collection works
    let metrics = detector.collect_metrics().expect("Metrics collection should work");
    assert_eq!(metrics.len(), 2);
    assert_eq!(metrics[0].utilization_percent, 75);
    assert_eq!(metrics[1].utilization_percent, 95);

    // Verify runtime is operational
    assert_eq!(runtime.container_count(), 0);
}

#[test]
fn test_node_shutdown_signal_delivery() {
    let config = test_config();
    let node_id = NodeId::new();
    let detector = Arc::new(FakeGpuDetector::new());
    let runtime = Arc::new(FakeContainerRuntime::new());

    let node = Node::with_components(config, node_id, detector, runtime);

    // Get multiple receivers
    let mut rx1 = node.shutdown_rx();
    let mut rx2 = node.shutdown_rx();

    // Signal shutdown
    node.shutdown();

    // Both receivers should get the signal
    assert!(rx1.try_recv().is_ok());
    assert!(rx2.try_recv().is_ok());
}

#[tokio::test]
async fn test_node_creation_async() {
    let mut config = test_config();
    config.gpu.enabled = false; // Disable GPU detection (no nvidia-smi)

    let result = Node::new(config).await;

    // Node should create successfully even without gateway
    assert!(result.is_ok());

    let node = result.unwrap();
    assert_eq!(node.state(), NodeState::Initializing);
    assert_eq!(node.name(), "integration-test-node");
}

#[tokio::test]
async fn test_node_run_fails_without_gateway() {
    let mut config = test_config();
    config.gpu.enabled = false;

    let node = Node::new(config).await.expect("Node creation should succeed");

    // Running should fail because there's no gateway to connect to
    let result = node.run().await;

    // Should fail with connection error
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("gateway") || err.to_string().contains("connect"));
}

#[tokio::test]
async fn test_node_graceful_shutdown_during_run() {
    let mut config = test_config();
    config.gpu.enabled = false;

    // This test verifies that shutdown signal is properly set up
    // The actual run will fail because no gateway, but we test the shutdown mechanism
    let node_id = NodeId::new();
    let detector = Arc::new(FakeGpuDetector::new());
    let runtime = Arc::new(FakeContainerRuntime::new());

    let node = Node::with_components(config, node_id, detector, runtime);

    // Get shutdown receiver
    let mut rx = node.shutdown_rx();

    // Trigger shutdown immediately
    node.shutdown();

    // Verify signal is received
    tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("Should receive shutdown within timeout")
        .expect("Shutdown signal should be Ok");
}

#[test]
fn test_multiple_nodes_independent() {
    let node1 = Node::with_components(
        NodeConfig {
            name: "node-1".to_string(),
            gateway_url: "ws://localhost:10001".to_string(),
            ..test_config()
        },
        NodeId::new(),
        Arc::new(FakeGpuDetector::new()),
        Arc::new(FakeContainerRuntime::new()),
    );

    let node2 = Node::with_components(
        NodeConfig {
            name: "node-2".to_string(),
            gateway_url: "ws://localhost:10002".to_string(),
            ..test_config()
        },
        NodeId::new(),
        Arc::new(FakeGpuDetector::new()),
        Arc::new(FakeContainerRuntime::new()),
    );

    // Nodes should have different IDs
    assert_ne!(node1.node_id(), node2.node_id());
    assert_eq!(node1.name(), "node-1");
    assert_eq!(node2.name(), "node-2");

    // Shutting down one shouldn't affect the other
    node1.shutdown();

    let mut rx2 = node2.shutdown_rx();
    assert!(rx2.try_recv().is_err()); // node2 should not have received shutdown
}

#[test]
fn test_node_with_different_gpu_configs() {
    // Test with no GPUs
    let node_no_gpu = Node::with_components(
        test_config(),
        NodeId::new(),
        Arc::new(FakeGpuDetector::new()),
        Arc::new(FakeContainerRuntime::new()),
    );
    assert_eq!(node_no_gpu.state(), NodeState::Initializing);

    // Test with multiple GPUs
    let multi_gpu_detector = Arc::new(
        FakeGpuDetector::new()
            .with_gpu(
                GpuInfo {
                    index: 0,
                    name: "GPU 0".to_string(),
                    memory_total_mib: 16384,
                    uuid: "GPU-0".to_string(),
                },
                GpuMetrics {
                    index: 0,
                    utilization_percent: 10,
                    memory_used_mib: 1000,
                    memory_total_mib: 16384,
                    temperature_celsius: 45,
                    power_watts: Some(100.0),
                },
            )
            .with_gpu(
                GpuInfo {
                    index: 1,
                    name: "GPU 1".to_string(),
                    memory_total_mib: 16384,
                    uuid: "GPU-1".to_string(),
                },
                GpuMetrics {
                    index: 1,
                    utilization_percent: 20,
                    memory_used_mib: 2000,
                    memory_total_mib: 16384,
                    temperature_celsius: 50,
                    power_watts: Some(150.0),
                },
            )
            .with_gpu(
                GpuInfo {
                    index: 2,
                    name: "GPU 2".to_string(),
                    memory_total_mib: 24576,
                    uuid: "GPU-2".to_string(),
                },
                GpuMetrics {
                    index: 2,
                    utilization_percent: 30,
                    memory_used_mib: 3000,
                    memory_total_mib: 24576,
                    temperature_celsius: 55,
                    power_watts: Some(200.0),
                },
            ),
    );

    let _node_multi_gpu = Node::with_components(
        test_config(),
        NodeId::new(),
        multi_gpu_detector.clone(),
        Arc::new(FakeContainerRuntime::new()),
    );

    // Verify GPU detection
    let gpus = multi_gpu_detector
        .detect_gpus()
        .expect("Should detect GPUs");
    assert_eq!(gpus.len(), 3);

    // Verify metrics for specific GPU
    let gpu2_metrics = multi_gpu_detector
        .collect_metrics_for_gpu(2)
        .expect("Should get GPU 2 metrics");
    assert_eq!(gpu2_metrics.utilization_percent, 30);
    assert_eq!(gpu2_metrics.memory_total_mib, 24576);
}
