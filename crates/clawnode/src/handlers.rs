//! Message handlers for gateway communication.
//!
//! This module handles incoming messages from the gateway and produces responses.

use tracing::{debug, info, warn};

use claw_proto::messages::{GatewayMessage, NodeMessage};
use claw_proto::types::{NodeId, WorkloadId, WorkloadState};
use claw_proto::workload::WorkloadSpec;

use crate::error::NodeError;
use crate::runtime::{ContainerRuntime, ContainerSpec};
use crate::state::{NodeState, WorkloadInfo};

/// Context required for handling gateway messages.
pub struct HandlerContext<'a, R: ContainerRuntime + ?Sized> {
    /// Current node state.
    pub state: &'a mut NodeState,
    /// Container runtime.
    pub runtime: &'a R,
    /// Node ID for response messages.
    pub node_id: NodeId,
}

/// Handle an incoming gateway message and optionally produce a response.
///
/// # Errors
///
/// Returns an error if the message cannot be processed.
pub fn handle_gateway_message<R: ContainerRuntime + ?Sized>(
    msg: GatewayMessage,
    ctx: &mut HandlerContext<'_, R>,
) -> Result<Option<NodeMessage>, NodeError> {
    match msg {
        GatewayMessage::Registered {
            node_id,
            heartbeat_interval_secs,
            metrics_interval_secs,
        } => {
            info!(
                node_id = %node_id,
                heartbeat_interval = heartbeat_interval_secs,
                metrics_interval = metrics_interval_secs,
                "node registered with gateway"
            );
            Ok(None)
        }

        GatewayMessage::HeartbeatAck { server_time } => {
            debug!(server_time = %server_time, "heartbeat acknowledged");
            Ok(None)
        }

        GatewayMessage::StartWorkload { workload_id, spec } => {
            handle_start_workload(workload_id, &spec, ctx)
        }

        GatewayMessage::StopWorkload {
            workload_id,
            grace_period_secs,
        } => handle_stop_workload(workload_id, grace_period_secs, ctx),

        GatewayMessage::RequestMetrics => {
            debug!("received metrics request");
            // Metrics are sent by a separate task, just acknowledge
            Ok(None)
        }

        GatewayMessage::RequestCapabilities => {
            debug!("received capabilities request");
            // Capabilities are sent during registration, just acknowledge
            Ok(None)
        }

        GatewayMessage::Error { code, message } => {
            warn!(code, message = %message, "received error from gateway");
            Ok(None)
        }

        GatewayMessage::ConfigUpdate { config } => {
            info!(?config, "received config update from gateway");
            // TODO: Apply config updates (intervals, etc.)
            Ok(None)
        }

        GatewayMessage::MeshPeerConfig { .. } => {
            debug!("received mesh peer config (not implemented)");
            Ok(None)
        }

        GatewayMessage::MeshPeerRemove { .. } => {
            debug!("received mesh peer remove (not implemented)");
            Ok(None)
        }

        GatewayMessage::RawEvent { event, .. } => {
            debug!(event = %event, "received raw event (handled in client event loop)");
            Ok(None)
        }
    }
}

/// Handle a workload start request.
#[allow(clippy::cast_precision_loss)] // cpu_cores fits in f32
fn handle_start_workload<R: ContainerRuntime + ?Sized>(
    workload_id: WorkloadId,
    spec: &WorkloadSpec,
    ctx: &mut HandlerContext<'_, R>,
) -> Result<Option<NodeMessage>, NodeError> {
    let id = workload_id.as_uuid();

    info!(
        workload_id = %workload_id,
        image = %spec.image,
        gpu_count = spec.gpu_count,
        "starting workload"
    );

    // Validate the workload spec
    if let Err(e) = spec.validate() {
        warn!(workload_id = %workload_id, error = %e, "workload validation failed");
        return Ok(Some(NodeMessage::workload_update(
            workload_id,
            WorkloadState::Failed,
            Some(format!("validation failed: {e}")),
        )));
    }

    // Check if workload already exists
    if ctx.state.get_workload(id).is_some() {
        return Err(NodeError::WorkloadExists(id));
    }

    // Allocate GPUs if needed
    let gpu_ids = if spec.gpu_count > 0 {
        match ctx.state.allocate_gpus(spec.gpu_count) {
            Ok(ids) => ids,
            Err(e) => {
                warn!(workload_id = %workload_id, error = %e, "GPU allocation failed");
                return Ok(Some(NodeMessage::workload_update(
                    workload_id,
                    WorkloadState::Failed,
                    Some(format!("GPU allocation failed: {e}")),
                )));
            }
        }
    } else {
        Vec::new()
    };

    // Send "starting" status update (could be sent async in real implementation)
    let _starting_msg = NodeMessage::workload_update(
        workload_id,
        WorkloadState::Starting,
        Some("Allocating resources".to_string()),
    );

    // Build container spec
    let mut container_spec = ContainerSpec::new(&spec.image)
        .with_gpus(gpu_ids.clone())
        .with_label("workload-id", workload_id.to_string());

    if !spec.command.is_empty() {
        container_spec = container_spec.with_command(spec.command.clone());
    }

    for (key, value) in &spec.env {
        container_spec = container_spec.with_env(key, value);
    }

    if spec.memory_mb > 0 {
        container_spec = container_spec.with_memory_limit(spec.memory_mb * 1024 * 1024);
    }

    if spec.cpu_cores > 0 {
        container_spec = container_spec.with_cpu_limit(spec.cpu_cores as f32);
    }

    // Create and start container
    let container = match ctx.runtime.create(&container_spec) {
        Ok(c) => c,
        Err(e) => {
            // Release GPUs on failure
            ctx.state.release_gpus(&gpu_ids);
            warn!(workload_id = %workload_id, error = %e, "container creation failed");
            return Ok(Some(NodeMessage::workload_update(
                workload_id,
                WorkloadState::Failed,
                Some(format!("container creation failed: {e}")),
            )));
        }
    };

    // Add workload to state
    let workload_info = WorkloadInfo::new(id, &spec.image, gpu_ids)
        .with_container_id(&container.id);

    if let Err(e) = ctx.state.add_workload(workload_info) {
        // Cleanup on failure
        let _ = ctx.runtime.remove(&container.id);
        return Err(e);
    }

    info!(
        workload_id = %workload_id,
        container_id = %container.id,
        "workload started successfully"
    );

    // Return "running" status update
    Ok(Some(NodeMessage::workload_update(
        workload_id,
        WorkloadState::Running,
        Some(format!("Container {} started", container.id)),
    )))
}

/// Handle a workload stop request.
fn handle_stop_workload<R: ContainerRuntime + ?Sized>(
    workload_id: WorkloadId,
    grace_period_secs: u32,
    ctx: &mut HandlerContext<'_, R>,
) -> Result<Option<NodeMessage>, NodeError> {
    let id = workload_id.as_uuid();

    info!(
        workload_id = %workload_id,
        grace_period = grace_period_secs,
        "stopping workload"
    );

    // Get the workload info
    let Some(workload) = ctx.state.get_workload(id).cloned() else {
        warn!(workload_id = %workload_id, "workload not found for stop request");
        return Err(NodeError::WorkloadNotFound(id));
    };

    // Send "stopping" status update (could be sent async in real implementation)
    let _stopping_msg = NodeMessage::workload_update(
        workload_id,
        WorkloadState::Stopping,
        Some("Graceful shutdown initiated".to_string()),
    );

    // Stop the container if it exists
    if let Some(container_id) = &workload.container_id {
        if let Err(e) = ctx.runtime.stop(container_id, grace_period_secs) {
            warn!(
                workload_id = %workload_id,
                container_id = %container_id,
                error = %e,
                "error stopping container"
            );
            // Continue with cleanup even if stop fails
        }

        // Remove the container
        if let Err(e) = ctx.runtime.remove(container_id) {
            warn!(
                workload_id = %workload_id,
                container_id = %container_id,
                error = %e,
                "error removing container"
            );
        }
    }

    // Remove workload from state (this also releases GPUs)
    ctx.state.remove_workload(id)?;

    info!(workload_id = %workload_id, "workload stopped successfully");

    // Return "stopped" status update
    Ok(Some(NodeMessage::workload_update(
        workload_id,
        WorkloadState::Stopped,
        Some("Workload stopped".to_string()),
    )))
}

/// Get the status of a workload.
///
/// # Errors
///
/// Returns an error if the workload is not found.
pub fn get_workload_status(
    workload_id: WorkloadId,
    state: &NodeState,
) -> Result<WorkloadState, NodeError> {
    let id = workload_id.as_uuid();

    match state.get_workload(id) {
        Some(_) => Ok(WorkloadState::Running),
        None => Err(NodeError::WorkloadNotFound(id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::FakeContainerRuntime;
    use chrono::Utc;

    fn test_node_id() -> NodeId {
        NodeId::new()
    }

    fn test_workload_id() -> WorkloadId {
        WorkloadId::new()
    }

    fn make_context<'a>(
        state: &'a mut NodeState,
        runtime: &'a FakeContainerRuntime,
    ) -> HandlerContext<'a, FakeContainerRuntime> {
        HandlerContext {
            state,
            runtime,
            node_id: test_node_id(),
        }
    }

    // ==================== Registered Message Tests ====================

    #[test]
    fn test_handle_registered_message() {
        let mut state = NodeState::with_gpus(4);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        let msg = GatewayMessage::Registered {
            node_id: NodeId::new(),
            heartbeat_interval_secs: 30,
            metrics_interval_secs: 10,
        };

        let result = handle_gateway_message(msg, &mut ctx);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // ==================== HeartbeatAck Message Tests ====================

    #[test]
    fn test_handle_heartbeat_ack() {
        let mut state = NodeState::with_gpus(4);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        let msg = GatewayMessage::HeartbeatAck {
            server_time: Utc::now(),
        };

        let result = handle_gateway_message(msg, &mut ctx);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // ==================== StartWorkload Message Tests ====================

    #[test]
    fn test_handle_start_workload_success() {
        let mut state = NodeState::with_gpus(4);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new("nginx:latest")
            .with_gpu_count(2)
            .with_memory_mb(1024);

        let msg = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };

        let result = handle_gateway_message(msg, &mut ctx);

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.is_some());

        // Check response is a running update
        match response.unwrap() {
            NodeMessage::WorkloadUpdate { workload_id: id, state: ws, .. } => {
                assert_eq!(id, workload_id);
                assert_eq!(ws, WorkloadState::Running);
            }
            _ => panic!("expected WorkloadUpdate message"),
        }

        // Check state was updated
        assert_eq!(ctx.state.workload_count(), 1);
        assert_eq!(ctx.state.available_gpu_count(), 2);

        // Check container was created
        assert_eq!(runtime.container_count(), 1);
    }

    #[test]
    fn test_handle_start_workload_no_gpus() {
        let mut state = NodeState::with_gpus(4);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new("nginx:latest")
            .with_gpu_count(0)
            .with_memory_mb(512);

        let msg = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };

        let result = handle_gateway_message(msg, &mut ctx);

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.is_some());

        // All GPUs should still be available
        assert_eq!(ctx.state.available_gpu_count(), 4);
        assert_eq!(ctx.state.workload_count(), 1);
    }

    #[test]
    fn test_handle_start_workload_insufficient_gpus() {
        let mut state = NodeState::with_gpus(2);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new("nvidia/cuda:12.0")
            .with_gpu_count(4); // More than available

        let msg = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };

        let result = handle_gateway_message(msg, &mut ctx);

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.is_some());

        // Check response is a failed update
        match response.unwrap() {
            NodeMessage::WorkloadUpdate { state: ws, message, .. } => {
                assert_eq!(ws, WorkloadState::Failed);
                assert!(message.unwrap().contains("GPU allocation failed"));
            }
            _ => panic!("expected WorkloadUpdate message"),
        }

        // State should be unchanged
        assert_eq!(ctx.state.workload_count(), 0);
        assert_eq!(ctx.state.available_gpu_count(), 2);
    }

    #[test]
    fn test_handle_start_workload_invalid_spec() {
        let mut state = NodeState::with_gpus(4);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new(""); // Empty image is invalid

        let msg = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };

        let result = handle_gateway_message(msg, &mut ctx);

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.is_some());

        // Check response is a failed update
        match response.unwrap() {
            NodeMessage::WorkloadUpdate { state: ws, message, .. } => {
                assert_eq!(ws, WorkloadState::Failed);
                assert!(message.unwrap().contains("validation failed"));
            }
            _ => panic!("expected WorkloadUpdate message"),
        }

        // No container should be created
        assert_eq!(runtime.container_count(), 0);
    }

    #[test]
    fn test_handle_start_workload_duplicate() {
        let mut state = NodeState::with_gpus(4);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new("nginx:latest");

        // Start first workload
        let msg1 = GatewayMessage::StartWorkload {
            workload_id,
            spec: spec.clone(),
        };
        handle_gateway_message(msg1, &mut ctx).expect("first start should succeed");

        // Try to start same workload again
        let msg2 = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };
        let result = handle_gateway_message(msg2, &mut ctx);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), NodeError::WorkloadExists(_)));
    }

    #[test]
    fn test_handle_start_workload_with_env() {
        let mut state = NodeState::with_gpus(4);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new("python:3.11")
            .with_command(vec!["python".to_string(), "app.py".to_string()])
            .with_env("DEBUG", "true")
            .with_env("PORT", "8080");

        let msg = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };

        let result = handle_gateway_message(msg, &mut ctx);

        assert!(result.is_ok());
        assert_eq!(ctx.state.workload_count(), 1);
    }

    // ==================== StopWorkload Message Tests ====================

    #[test]
    fn test_handle_stop_workload_success() {
        let mut state = NodeState::with_gpus(4);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        // First, start a workload
        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new("nginx:latest").with_gpu_count(2);

        let start_msg = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };
        handle_gateway_message(start_msg, &mut ctx).expect("start should succeed");

        assert_eq!(ctx.state.workload_count(), 1);
        assert_eq!(ctx.state.available_gpu_count(), 2);

        // Now stop it
        let stop_msg = GatewayMessage::StopWorkload {
            workload_id,
            grace_period_secs: 30,
        };

        let result = handle_gateway_message(stop_msg, &mut ctx);

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.is_some());

        // Check response is a stopped update
        match response.unwrap() {
            NodeMessage::WorkloadUpdate { workload_id: id, state: ws, .. } => {
                assert_eq!(id, workload_id);
                assert_eq!(ws, WorkloadState::Stopped);
            }
            _ => panic!("expected WorkloadUpdate message"),
        }

        // Check state was updated
        assert_eq!(ctx.state.workload_count(), 0);
        assert_eq!(ctx.state.available_gpu_count(), 4); // GPUs released
    }

    #[test]
    fn test_handle_stop_workload_not_found() {
        let mut state = NodeState::with_gpus(4);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        let workload_id = test_workload_id();

        let msg = GatewayMessage::StopWorkload {
            workload_id,
            grace_period_secs: 30,
        };

        let result = handle_gateway_message(msg, &mut ctx);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), NodeError::WorkloadNotFound(_)));
    }

    #[test]
    fn test_handle_stop_workload_releases_gpus() {
        let mut state = NodeState::with_gpus(4);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        // Start two workloads
        let workload_id1 = test_workload_id();
        let workload_id2 = test_workload_id();

        let msg1 = GatewayMessage::StartWorkload {
            workload_id: workload_id1,
            spec: WorkloadSpec::new("nginx:latest").with_gpu_count(2),
        };
        handle_gateway_message(msg1, &mut ctx).expect("start should succeed");

        let msg2 = GatewayMessage::StartWorkload {
            workload_id: workload_id2,
            spec: WorkloadSpec::new("redis:latest").with_gpu_count(1),
        };
        handle_gateway_message(msg2, &mut ctx).expect("start should succeed");

        assert_eq!(ctx.state.available_gpu_count(), 1);

        // Stop first workload
        let stop_msg = GatewayMessage::StopWorkload {
            workload_id: workload_id1,
            grace_period_secs: 10,
        };
        handle_gateway_message(stop_msg, &mut ctx).expect("stop should succeed");

        // 2 GPUs should be released, 1 still in use
        assert_eq!(ctx.state.available_gpu_count(), 3);
        assert_eq!(ctx.state.workload_count(), 1);
    }

    // ==================== RequestMetrics Message Tests ====================

    #[test]
    fn test_handle_request_metrics() {
        let mut state = NodeState::with_gpus(4);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        let msg = GatewayMessage::RequestMetrics;

        let result = handle_gateway_message(msg, &mut ctx);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // ==================== RequestCapabilities Message Tests ====================

    #[test]
    fn test_handle_request_capabilities() {
        let mut state = NodeState::with_gpus(4);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        let msg = GatewayMessage::RequestCapabilities;

        let result = handle_gateway_message(msg, &mut ctx);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // ==================== Error Message Tests ====================

    #[test]
    fn test_handle_error_message() {
        let mut state = NodeState::with_gpus(4);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        let msg = GatewayMessage::Error {
            code: 500,
            message: "Internal server error".to_string(),
        };

        let result = handle_gateway_message(msg, &mut ctx);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // ==================== get_workload_status Tests ====================

    #[test]
    fn test_get_workload_status_running() {
        let mut state = NodeState::with_gpus(4);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new("nginx:latest");

        let msg = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };
        handle_gateway_message(msg, &mut ctx).expect("start should succeed");

        let status = get_workload_status(workload_id, ctx.state);

        assert!(status.is_ok());
        assert_eq!(status.unwrap(), WorkloadState::Running);
    }

    #[test]
    fn test_get_workload_status_not_found() {
        let state = NodeState::with_gpus(4);
        let workload_id = test_workload_id();

        let status = get_workload_status(workload_id, &state);

        assert!(status.is_err());
        assert!(matches!(status.unwrap_err(), NodeError::WorkloadNotFound(_)));
    }

    // ==================== Integration Tests ====================

    #[test]
    fn test_full_workload_lifecycle_via_handlers() {
        let mut state = NodeState::with_gpus(8);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        let workload_id = test_workload_id();

        // Start workload
        let start_msg = GatewayMessage::StartWorkload {
            workload_id,
            spec: WorkloadSpec::new("training:v1")
                .with_gpu_count(4)
                .with_memory_mb(16384)
                .with_env("CUDA_VISIBLE_DEVICES", "0,1,2,3"),
        };

        let start_result = handle_gateway_message(start_msg, &mut ctx);
        assert!(start_result.is_ok());

        // Verify running
        let status = get_workload_status(workload_id, ctx.state);
        assert_eq!(status.unwrap(), WorkloadState::Running);
        assert_eq!(ctx.state.available_gpu_count(), 4);

        // Stop workload
        let stop_msg = GatewayMessage::StopWorkload {
            workload_id,
            grace_period_secs: 60,
        };

        let stop_result = handle_gateway_message(stop_msg, &mut ctx);
        assert!(stop_result.is_ok());

        // Verify stopped
        let status = get_workload_status(workload_id, ctx.state);
        assert!(status.is_err()); // Not found = stopped
        assert_eq!(ctx.state.available_gpu_count(), 8); // All GPUs released
    }

    #[test]
    fn test_multiple_workloads_lifecycle() {
        let mut state = NodeState::with_gpus(8);
        let runtime = FakeContainerRuntime::new();
        let mut ctx = make_context(&mut state, &runtime);

        let id1 = test_workload_id();
        let id2 = test_workload_id();
        let id3 = test_workload_id();

        // Start three workloads
        for (id, gpu_count) in [(id1, 2), (id2, 3), (id3, 1)] {
            let msg = GatewayMessage::StartWorkload {
                workload_id: id,
                spec: WorkloadSpec::new("worker:latest").with_gpu_count(gpu_count),
            };
            handle_gateway_message(msg, &mut ctx).expect("start should succeed");
        }

        assert_eq!(ctx.state.workload_count(), 3);
        assert_eq!(ctx.state.available_gpu_count(), 2); // 8 - 2 - 3 - 1 = 2

        // Stop middle workload
        let stop_msg = GatewayMessage::StopWorkload {
            workload_id: id2,
            grace_period_secs: 10,
        };
        handle_gateway_message(stop_msg, &mut ctx).expect("stop should succeed");

        assert_eq!(ctx.state.workload_count(), 2);
        assert_eq!(ctx.state.available_gpu_count(), 5); // 2 + 3 = 5

        // Stop remaining workloads
        for id in [id1, id3] {
            let msg = GatewayMessage::StopWorkload {
                workload_id: id,
                grace_period_secs: 10,
            };
            handle_gateway_message(msg, &mut ctx).expect("stop should succeed");
        }

        assert_eq!(ctx.state.workload_count(), 0);
        assert_eq!(ctx.state.available_gpu_count(), 8);
    }
}
