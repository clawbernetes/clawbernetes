//! Integration tests for workload dispatch.
//!
//! Tests follow TDD methodology - these tests were written FIRST before implementation.

use claw_gateway::{
    dispatch::{DispatchError, WorkloadDispatcher},
    NodeRegistry, Scheduler, WorkloadManager,
};
use claw_proto::{GpuCapability, NodeCapabilities, NodeId, WorkloadSpec, WorkloadState};

// ==================== Helper Functions ====================

fn make_gpu(name: &str, memory_mib: u64) -> GpuCapability {
    GpuCapability {
        index: 0,
        name: name.to_string(),
        memory_mib,
        uuid: format!("gpu-uuid-{name}"),
    }
}

fn make_capabilities(cpu: u32, memory: u64, gpus: Vec<GpuCapability>) -> NodeCapabilities {
    let mut caps = NodeCapabilities::new(cpu, memory);
    for gpu in gpus {
        caps = caps.with_gpu(gpu);
    }
    caps
}

fn register_node(
    registry: &mut NodeRegistry,
    cpu: u32,
    memory: u64,
    gpus: Vec<GpuCapability>,
) -> NodeId {
    let node_id = NodeId::new();
    let caps = make_capabilities(cpu, memory, gpus);
    registry
        .register(node_id, caps)
        .expect("failed to register node in test setup");
    node_id
}

fn make_valid_spec() -> WorkloadSpec {
    WorkloadSpec::new("nginx:latest")
        .with_memory_mb(1024)
        .with_cpu_cores(2)
}

fn make_gpu_spec(gpu_count: u32) -> WorkloadSpec {
    WorkloadSpec::new("pytorch:latest")
        .with_gpu_count(gpu_count)
        .with_memory_mb(8192)
        .with_cpu_cores(4)
}

// ==================== WorkloadDispatcher Construction Tests ====================

#[test]
fn test_dispatcher_new() {
    let registry = NodeRegistry::new();
    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();

    let dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    assert!(dispatcher.pending_count() == 0);
}

// ==================== Submit Tests ====================

#[test]
fn test_submit_returns_workload_id() {
    let registry = NodeRegistry::new();
    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let spec = make_valid_spec();
    let result = dispatcher.submit(spec);

    assert!(result.is_ok());
}

#[test]
fn test_submit_schedules_to_available_node() {
    let mut registry = NodeRegistry::new();
    let node_id = register_node(&mut registry, 8, 16384, vec![]);

    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let spec = make_valid_spec();
    let workload_id = dispatcher.submit(spec).expect("submit should succeed");

    // Check that workload was assigned to the node
    let workload = dispatcher
        .get_workload(workload_id)
        .expect("workload should exist");
    assert_eq!(workload.assigned_node, Some(node_id));
}

#[test]
fn test_submit_with_gpu_requirement_schedules_to_gpu_node() {
    let mut registry = NodeRegistry::new();

    // Node without GPU
    let _cpu_node = register_node(&mut registry, 8, 16384, vec![]);

    // Node with GPU
    let gpu = make_gpu("RTX 4090", 24576);
    let gpu_node = register_node(&mut registry, 8, 32768, vec![gpu]);

    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let spec = make_gpu_spec(1);
    let workload_id = dispatcher.submit(spec).expect("submit should succeed");

    let workload = dispatcher
        .get_workload(workload_id)
        .expect("workload should exist");
    assert_eq!(workload.assigned_node, Some(gpu_node));
}

#[test]
fn test_submit_without_nodes_keeps_pending() {
    let registry = NodeRegistry::new(); // Empty registry
    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let spec = make_valid_spec();
    let workload_id = dispatcher.submit(spec).expect("submit should succeed");

    // Workload should be pending with no node assignment
    let workload = dispatcher
        .get_workload(workload_id)
        .expect("workload should exist");
    assert!(workload.assigned_node.is_none());
    assert_eq!(workload.state(), WorkloadState::Pending);
    assert_eq!(dispatcher.pending_count(), 1);
}

#[test]
fn test_submit_invalid_spec_fails() {
    let registry = NodeRegistry::new();
    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let spec = WorkloadSpec::new(""); // Invalid empty image
    let result = dispatcher.submit(spec);

    assert!(matches!(result, Err(DispatchError::ValidationFailed(_))));
}

// ==================== Dispatch to Node Tests ====================

#[test]
fn test_dispatch_to_node_creates_workload_command() {
    let mut registry = NodeRegistry::new();
    let node_id = register_node(&mut registry, 8, 16384, vec![]);

    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let spec = make_valid_spec();
    let workload_id = dispatcher.submit(spec.clone()).expect("submit should succeed");

    let message = dispatcher
        .dispatch_to_node(workload_id, node_id)
        .expect("dispatch should succeed");

    // Should create a StartWorkload message with correct workload
    match message {
        claw_proto::GatewayMessage::StartWorkload {
            workload_id: msg_id,
            spec: msg_spec,
        } => {
            assert_eq!(msg_id, workload_id);
            assert_eq!(msg_spec.image, spec.image);
        }
        other => panic!("expected StartWorkload message, got {other:?}"),
    }
}

#[test]
fn test_dispatch_to_node_transitions_to_starting() {
    let mut registry = NodeRegistry::new();
    let node_id = register_node(&mut registry, 8, 16384, vec![]);

    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let spec = make_valid_spec();
    let workload_id = dispatcher.submit(spec).expect("submit should succeed");

    dispatcher
        .dispatch_to_node(workload_id, node_id)
        .expect("dispatch should succeed");

    let workload = dispatcher
        .get_workload(workload_id)
        .expect("workload should exist");
    assert_eq!(workload.state(), WorkloadState::Starting);
}

#[test]
fn test_dispatch_to_offline_node_fails() {
    let mut registry = NodeRegistry::new();
    let node_id = register_node(&mut registry, 8, 16384, vec![]);

    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let spec = make_valid_spec();
    let workload_id = dispatcher.submit(spec).expect("submit should succeed");

    // Unregister the node to simulate it going offline
    dispatcher.unregister_node(node_id).expect("should unregister");

    let result = dispatcher.dispatch_to_node(workload_id, node_id);

    assert!(matches!(result, Err(DispatchError::NodeOffline(_))));
}

#[test]
fn test_dispatch_nonexistent_workload_fails() {
    let mut registry = NodeRegistry::new();
    let node_id = register_node(&mut registry, 8, 16384, vec![]);

    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let fake_workload_id = claw_proto::WorkloadId::new();
    let result = dispatcher.dispatch_to_node(fake_workload_id, node_id);

    assert!(matches!(result, Err(DispatchError::WorkloadNotFound(_))));
}

// ==================== Handle Workload Update Tests ====================

#[test]
fn test_handle_workload_update_running() {
    let mut registry = NodeRegistry::new();
    let node_id = register_node(&mut registry, 8, 16384, vec![]);

    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let spec = make_valid_spec();
    let workload_id = dispatcher.submit(spec).expect("submit should succeed");

    // Dispatch to node (transitions to Starting)
    dispatcher
        .dispatch_to_node(workload_id, node_id)
        .expect("dispatch should succeed");

    // Simulate node reporting running state
    dispatcher
        .handle_workload_update(workload_id, WorkloadState::Running, None)
        .expect("update should succeed");

    let workload = dispatcher
        .get_workload(workload_id)
        .expect("workload should exist");
    assert_eq!(workload.state(), WorkloadState::Running);
}

#[test]
fn test_handle_workload_update_completed() {
    let mut registry = NodeRegistry::new();
    let node_id = register_node(&mut registry, 8, 16384, vec![]);

    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let spec = make_valid_spec();
    let workload_id = dispatcher.submit(spec).expect("submit should succeed");

    // Simulate full lifecycle
    dispatcher
        .dispatch_to_node(workload_id, node_id)
        .expect("dispatch should succeed");
    dispatcher
        .handle_workload_update(workload_id, WorkloadState::Running, None)
        .expect("should transition to running");
    dispatcher
        .handle_workload_update(workload_id, WorkloadState::Completed, None)
        .expect("should transition to completed");

    let workload = dispatcher
        .get_workload(workload_id)
        .expect("workload should exist");
    assert_eq!(workload.state(), WorkloadState::Completed);
    assert!(workload.state().is_terminal());
}

#[test]
fn test_handle_workload_update_failed_with_message() {
    let mut registry = NodeRegistry::new();
    let node_id = register_node(&mut registry, 8, 16384, vec![]);

    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let spec = make_valid_spec();
    let workload_id = dispatcher.submit(spec).expect("submit should succeed");

    dispatcher
        .dispatch_to_node(workload_id, node_id)
        .expect("dispatch should succeed");
    dispatcher
        .handle_workload_update(workload_id, WorkloadState::Running, None)
        .expect("should transition to running");

    let error_msg = "Out of memory".to_string();
    dispatcher
        .handle_workload_update(workload_id, WorkloadState::Failed, Some(error_msg.clone()))
        .expect("should transition to failed");

    let workload = dispatcher
        .get_workload(workload_id)
        .expect("workload should exist");
    assert_eq!(workload.state(), WorkloadState::Failed);
}

#[test]
fn test_handle_workload_update_invalid_transition() {
    let registry = NodeRegistry::new();
    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let spec = make_valid_spec();
    let workload_id = dispatcher.submit(spec).expect("submit should succeed");

    // Cannot go directly from Pending to Completed
    let result = dispatcher.handle_workload_update(workload_id, WorkloadState::Completed, None);

    assert!(matches!(result, Err(DispatchError::InvalidTransition(_, _, _))));
}

#[test]
fn test_handle_workload_update_nonexistent_workload() {
    let registry = NodeRegistry::new();
    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let fake_id = claw_proto::WorkloadId::new();
    let result = dispatcher.handle_workload_update(fake_id, WorkloadState::Running, None);

    assert!(matches!(result, Err(DispatchError::WorkloadNotFound(_))));
}

// ==================== Pending Dispatch Management Tests ====================

#[test]
fn test_pending_count_tracks_unscheduled_workloads() {
    let registry = NodeRegistry::new(); // Empty - no nodes
    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    assert_eq!(dispatcher.pending_count(), 0);

    dispatcher.submit(make_valid_spec()).expect("should succeed");
    assert_eq!(dispatcher.pending_count(), 1);

    dispatcher.submit(make_valid_spec()).expect("should succeed");
    assert_eq!(dispatcher.pending_count(), 2);
}

#[test]
fn test_try_dispatch_pending_when_node_registers() {
    let registry = NodeRegistry::new(); // Start empty
    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    // Submit workloads with no available nodes
    let wl1 = dispatcher.submit(make_valid_spec()).expect("should succeed");
    let wl2 = dispatcher.submit(make_valid_spec()).expect("should succeed");

    assert_eq!(dispatcher.pending_count(), 2);

    // Register a node
    let caps = make_capabilities(8, 16384, vec![]);
    let _node_id = dispatcher
        .register_node(NodeId::new(), caps)
        .expect("should register");

    // Try to dispatch pending workloads
    let dispatched = dispatcher
        .try_dispatch_pending()
        .expect("should not error");

    // Should have dispatched at least one workload
    assert!(!dispatched.is_empty());

    // Check workloads got assigned
    let w1 = dispatcher.get_workload(wl1).expect("should exist");
    let w2 = dispatcher.get_workload(wl2).expect("should exist");

    // At least one should be assigned (both if scheduler allows)
    assert!(w1.assigned_node.is_some() || w2.assigned_node.is_some());
}

// ==================== Node Registration/Unregistration Tests ====================

#[test]
fn test_register_node_through_dispatcher() {
    let registry = NodeRegistry::new();
    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let node_id = NodeId::new();
    let caps = make_capabilities(8, 16384, vec![make_gpu("RTX 4090", 24576)]);

    let result = dispatcher.register_node(node_id, caps);

    assert!(result.is_ok());
    assert!(dispatcher.get_node(node_id).is_some());
}

#[test]
fn test_unregister_node_fails_workloads_on_node() {
    let mut registry = NodeRegistry::new();
    let node_id = register_node(&mut registry, 8, 16384, vec![]);

    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    // Submit and dispatch a workload
    let spec = make_valid_spec();
    let workload_id = dispatcher.submit(spec).expect("should succeed");
    dispatcher
        .dispatch_to_node(workload_id, node_id)
        .expect("should dispatch");
    dispatcher
        .handle_workload_update(workload_id, WorkloadState::Running, None)
        .expect("should update");

    // Unregister the node - should fail running workloads
    dispatcher.unregister_node(node_id).expect("should unregister");

    let workload = dispatcher
        .get_workload(workload_id)
        .expect("should exist");

    // Workload should be marked as failed
    assert_eq!(workload.state(), WorkloadState::Failed);
}

// ==================== Stop Workload Tests ====================

#[test]
fn test_stop_workload_creates_stop_message() {
    let mut registry = NodeRegistry::new();
    let node_id = register_node(&mut registry, 8, 16384, vec![]);

    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let spec = make_valid_spec();
    let workload_id = dispatcher.submit(spec).expect("should succeed");
    dispatcher
        .dispatch_to_node(workload_id, node_id)
        .expect("should dispatch");
    dispatcher
        .handle_workload_update(workload_id, WorkloadState::Running, None)
        .expect("should update");

    let message = dispatcher
        .stop_workload(workload_id, 30)
        .expect("should create stop message");

    match message {
        claw_proto::GatewayMessage::StopWorkload {
            workload_id: msg_id,
            grace_period_secs,
        } => {
            assert_eq!(msg_id, workload_id);
            assert_eq!(grace_period_secs, 30);
        }
        other => panic!("expected StopWorkload message, got {other:?}"),
    }
}

#[test]
fn test_stop_workload_transitions_to_stopping() {
    let mut registry = NodeRegistry::new();
    let node_id = register_node(&mut registry, 8, 16384, vec![]);

    let workload_mgr = WorkloadManager::new();
    let scheduler = Scheduler::new();
    let mut dispatcher = WorkloadDispatcher::new(registry, workload_mgr, scheduler);

    let spec = make_valid_spec();
    let workload_id = dispatcher.submit(spec).expect("should succeed");
    dispatcher
        .dispatch_to_node(workload_id, node_id)
        .expect("should dispatch");
    dispatcher
        .handle_workload_update(workload_id, WorkloadState::Running, None)
        .expect("should update");

    dispatcher
        .stop_workload(workload_id, 30)
        .expect("should succeed");

    let workload = dispatcher
        .get_workload(workload_id)
        .expect("should exist");
    assert_eq!(workload.state(), WorkloadState::Stopping);
}
