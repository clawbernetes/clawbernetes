//! Async message handlers for gateway communication.
//!
//! This module handles incoming messages from the gateway using async container
//! runtime operations. It's designed to work with real Docker or containerd.
//!
//! ## Key differences from sync handlers
//!
//! - Uses `AsyncContainerRuntime` trait instead of sync `ContainerRuntime`
//! - All workload operations are async
//! - Supports log streaming to gateway

use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

use claw_proto::messages::{GatewayMessage, NodeMessage};
use claw_proto::types::{NodeId, WorkloadId, WorkloadState};
use claw_proto::workload::WorkloadSpec;

use crate::docker::AsyncContainerRuntime;
use crate::error::NodeError;
use crate::runtime::ContainerSpec;
use crate::state::{NodeState, WorkloadInfo};

/// Async context for handling gateway messages.
pub struct AsyncHandlerContext<R: AsyncContainerRuntime> {
    /// Current node state.
    pub state: Arc<RwLock<NodeState>>,
    /// Container runtime.
    pub runtime: Arc<R>,
    /// Node ID for response messages.
    pub node_id: NodeId,
    /// Channel to send messages back to gateway.
    pub gateway_tx: mpsc::Sender<NodeMessage>,
}

impl<R: AsyncContainerRuntime> AsyncHandlerContext<R> {
    /// Create a new async handler context.
    pub fn new(
        state: Arc<RwLock<NodeState>>,
        runtime: Arc<R>,
        node_id: NodeId,
        gateway_tx: mpsc::Sender<NodeMessage>,
    ) -> Self {
        Self {
            state,
            runtime,
            node_id,
            gateway_tx,
        }
    }
}

/// Handle an incoming gateway message asynchronously.
///
/// # Errors
///
/// Returns an error if the message cannot be processed.
pub async fn handle_gateway_message_async<R: AsyncContainerRuntime>(
    msg: GatewayMessage,
    ctx: &AsyncHandlerContext<R>,
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
            handle_start_workload_async(workload_id, &spec, ctx).await
        }

        GatewayMessage::StopWorkload {
            workload_id,
            grace_period_secs,
        } => handle_stop_workload_async(workload_id, grace_period_secs, ctx).await,

        GatewayMessage::RequestMetrics => {
            debug!("received metrics request");
            Ok(None)
        }

        GatewayMessage::RequestCapabilities => {
            debug!("received capabilities request");
            Ok(None)
        }

        GatewayMessage::Error { code, message } => {
            warn!(code, message = %message, "received error from gateway");
            Ok(None)
        }

        GatewayMessage::ConfigUpdate { config } => {
            info!(?config, "received config update from gateway");
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
    }
}

/// Handle a workload start request asynchronously.
#[allow(clippy::cast_precision_loss)]
async fn handle_start_workload_async<R: AsyncContainerRuntime + 'static>(
    workload_id: WorkloadId,
    spec: &WorkloadSpec,
    ctx: &AsyncHandlerContext<R>,
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

    // Lock state for the duration of GPU allocation
    let mut state = ctx.state.write().await;

    // Check if workload already exists
    if state.get_workload(id).is_some() {
        return Err(NodeError::WorkloadExists(id));
    }

    // Allocate GPUs if needed
    let gpu_ids = if spec.gpu_count > 0 {
        match state.allocate_gpus(spec.gpu_count) {
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

    // Send "starting" status update
    let _ = ctx
        .gateway_tx
        .send(NodeMessage::workload_update(
            workload_id,
            WorkloadState::Starting,
            Some("Allocating resources".to_string()),
        ))
        .await;

    // Release state lock before container operations
    drop(state);

    // Build container spec
    let mut container_spec = ContainerSpec::new(&spec.image)
        .with_gpus(gpu_ids.clone())
        .with_label("workload-id", workload_id.to_string())
        .with_label("managed-by", "clawbernetes");

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
    let container = match ctx.runtime.create(&container_spec).await {
        Ok(c) => c,
        Err(e) => {
            // Release GPUs on failure
            let mut state = ctx.state.write().await;
            state.release_gpus(&gpu_ids);
            warn!(workload_id = %workload_id, error = %e, "container creation failed");
            return Ok(Some(NodeMessage::workload_update(
                workload_id,
                WorkloadState::Failed,
                Some(format!("container creation failed: {e}")),
            )));
        }
    };

    // Add workload to state
    let workload_info =
        WorkloadInfo::new(id, &spec.image, gpu_ids.clone()).with_container_id(&container.id);

    let mut state = ctx.state.write().await;
    if let Err(e) = state.add_workload(workload_info) {
        // Cleanup on failure
        drop(state);
        let _ = ctx.runtime.remove(&container.id).await;
        return Err(e);
    }
    drop(state);

    info!(
        workload_id = %workload_id,
        container_id = %container.id,
        "workload started successfully"
    );

    // Start log streaming in background
    let container_id = container.id.clone();
    let runtime = Arc::clone(&ctx.runtime);
    let gateway_tx = ctx.gateway_tx.clone();
    let wid = workload_id;

    tokio::spawn(async move {
        if let Err(e) = stream_container_logs(wid, &container_id, runtime, gateway_tx).await {
            warn!(workload_id = %wid, error = %e, "log streaming failed");
        }
    });

    // Return "running" status update
    Ok(Some(NodeMessage::workload_update(
        workload_id,
        WorkloadState::Running,
        Some(format!("Container {} started", container.id)),
    )))
}

/// Handle a workload stop request asynchronously.
async fn handle_stop_workload_async<R: AsyncContainerRuntime>(
    workload_id: WorkloadId,
    grace_period_secs: u32,
    ctx: &AsyncHandlerContext<R>,
) -> Result<Option<NodeMessage>, NodeError> {
    let id = workload_id.as_uuid();

    info!(
        workload_id = %workload_id,
        grace_period = grace_period_secs,
        "stopping workload"
    );

    // Get the workload info
    let workload = {
        let state = ctx.state.read().await;
        state.get_workload(id).cloned()
    };

    let Some(workload) = workload else {
        warn!(workload_id = %workload_id, "workload not found for stop request");
        return Err(NodeError::WorkloadNotFound(id));
    };

    // Send "stopping" status update
    let _ = ctx
        .gateway_tx
        .send(NodeMessage::workload_update(
            workload_id,
            WorkloadState::Stopping,
            Some("Graceful shutdown initiated".to_string()),
        ))
        .await;

    // Stop the container if it exists
    if let Some(container_id) = &workload.container_id {
        if let Err(e) = ctx.runtime.stop(container_id, grace_period_secs).await {
            warn!(
                workload_id = %workload_id,
                container_id = %container_id,
                error = %e,
                "error stopping container"
            );
            // Continue with cleanup even if stop fails
        }

        // Remove the container
        if let Err(e) = ctx.runtime.remove(container_id).await {
            warn!(
                workload_id = %workload_id,
                container_id = %container_id,
                error = %e,
                "error removing container"
            );
        }
    }

    // Remove workload from state (this also releases GPUs)
    {
        let mut state = ctx.state.write().await;
        state.remove_workload(id)?;
    }

    info!(workload_id = %workload_id, "workload stopped successfully");

    // Return "stopped" status update
    Ok(Some(NodeMessage::workload_update(
        workload_id,
        WorkloadState::Stopped,
        Some("Workload stopped".to_string()),
    )))
}

/// Stream container logs to the gateway.
async fn stream_container_logs<R: AsyncContainerRuntime>(
    workload_id: WorkloadId,
    container_id: &str,
    runtime: Arc<R>,
    gateway_tx: mpsc::Sender<NodeMessage>,
) -> Result<(), NodeError> {
    debug!(workload_id = %workload_id, container_id, "starting log streaming");

    let mut log_rx = runtime.stream_logs(container_id).await?;
    let mut buffer = Vec::with_capacity(100);

    loop {
        // Use select to batch logs or timeout
        let result = tokio::time::timeout(tokio::time::Duration::from_millis(500), log_rx.recv())
            .await;

        match result {
            Ok(Some(line)) => {
                buffer.push(line);

                // Send batch if buffer is getting large
                if buffer.len() >= 50 {
                    let lines = std::mem::take(&mut buffer);
                    let msg = NodeMessage::workload_logs(workload_id, lines, false);
                    if gateway_tx.send(msg).await.is_err() {
                        break; // Gateway channel closed
                    }
                }
            }
            Ok(None) => {
                // Stream ended
                break;
            }
            Err(_) => {
                // Timeout - send whatever we have buffered
                if !buffer.is_empty() {
                    let lines = std::mem::take(&mut buffer);
                    let msg = NodeMessage::workload_logs(workload_id, lines, false);
                    if gateway_tx.send(msg).await.is_err() {
                        break;
                    }
                }
            }
        }
    }

    // Send any remaining buffered logs
    if !buffer.is_empty() {
        let msg = NodeMessage::workload_logs(workload_id, buffer, false);
        let _ = gateway_tx.send(msg).await;
    }

    debug!(workload_id = %workload_id, "log streaming ended");
    Ok(())
}

/// Get the status of a workload.
///
/// # Errors
///
/// Returns an error if the workload is not found.
pub async fn get_workload_status_async(
    workload_id: WorkloadId,
    state: &RwLock<NodeState>,
) -> Result<WorkloadState, NodeError> {
    let id = workload_id.as_uuid();
    let state_guard = state.read().await;

    match state_guard.get_workload(id) {
        Some(_) => Ok(WorkloadState::Running),
        None => Err(NodeError::WorkloadNotFound(id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::FakeAsyncContainerRuntime;

    fn test_node_id() -> NodeId {
        NodeId::new()
    }

    fn test_workload_id() -> WorkloadId {
        WorkloadId::new()
    }

    async fn make_context(
        state: Arc<RwLock<NodeState>>,
        runtime: Arc<FakeAsyncContainerRuntime>,
    ) -> (
        AsyncHandlerContext<FakeAsyncContainerRuntime>,
        mpsc::Receiver<NodeMessage>,
    ) {
        let (tx, rx) = mpsc::channel(100);
        let ctx = AsyncHandlerContext::new(state, runtime, test_node_id(), tx);
        (ctx, rx)
    }

    // ==================== Registered Message Tests ====================

    #[tokio::test]
    async fn test_handle_registered_message() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(4)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, _rx) = make_context(state, runtime).await;

        let msg = GatewayMessage::Registered {
            node_id: NodeId::new(),
            heartbeat_interval_secs: 30,
            metrics_interval_secs: 10,
        };

        let result = handle_gateway_message_async(msg, &ctx).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // ==================== HeartbeatAck Message Tests ====================

    #[tokio::test]
    async fn test_handle_heartbeat_ack() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(4)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, _rx) = make_context(state, runtime).await;

        let msg = GatewayMessage::HeartbeatAck {
            server_time: chrono::Utc::now(),
        };

        let result = handle_gateway_message_async(msg, &ctx).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // ==================== StartWorkload Message Tests ====================

    #[tokio::test]
    async fn test_handle_start_workload_success() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(4)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, _rx) = make_context(Arc::clone(&state), Arc::clone(&runtime)).await;

        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new("nginx:latest")
            .with_gpu_count(2)
            .with_memory_mb(1024);

        let msg = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };

        let result = handle_gateway_message_async(msg, &ctx).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.is_some());

        // Check response is a running update
        match response.unwrap() {
            NodeMessage::WorkloadUpdate {
                workload_id: id,
                state: ws,
                ..
            } => {
                assert_eq!(id, workload_id);
                assert_eq!(ws, WorkloadState::Running);
            }
            _ => panic!("expected WorkloadUpdate message"),
        }

        // Check state was updated
        let state_guard = state.read().await;
        assert_eq!(state_guard.workload_count(), 1);
        assert_eq!(state_guard.available_gpu_count(), 2);

        // Check container was created
        assert_eq!(runtime.container_count().await, 1);
    }

    #[tokio::test]
    async fn test_handle_start_workload_no_gpus() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(4)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, _rx) = make_context(Arc::clone(&state), runtime).await;

        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new("nginx:latest")
            .with_gpu_count(0)
            .with_memory_mb(512);

        let msg = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };

        let result = handle_gateway_message_async(msg, &ctx).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.is_some());

        // All GPUs should still be available
        let state_guard = state.read().await;
        assert_eq!(state_guard.available_gpu_count(), 4);
        assert_eq!(state_guard.workload_count(), 1);
    }

    #[tokio::test]
    async fn test_handle_start_workload_insufficient_gpus() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(2)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, _rx) = make_context(Arc::clone(&state), runtime).await;

        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new("nvidia/cuda:12.0").with_gpu_count(4); // More than available

        let msg = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };

        let result = handle_gateway_message_async(msg, &ctx).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.is_some());

        // Check response is a failed update
        match response.unwrap() {
            NodeMessage::WorkloadUpdate {
                state: ws,
                message,
                ..
            } => {
                assert_eq!(ws, WorkloadState::Failed);
                assert!(message.unwrap().contains("GPU allocation failed"));
            }
            _ => panic!("expected WorkloadUpdate message"),
        }

        // State should be unchanged
        let state_guard = state.read().await;
        assert_eq!(state_guard.workload_count(), 0);
        assert_eq!(state_guard.available_gpu_count(), 2);
    }

    #[tokio::test]
    async fn test_handle_start_workload_invalid_spec() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(4)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, _rx) = make_context(Arc::clone(&state), Arc::clone(&runtime)).await;

        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new(""); // Empty image is invalid

        let msg = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };

        let result = handle_gateway_message_async(msg, &ctx).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.is_some());

        // Check response is a failed update
        match response.unwrap() {
            NodeMessage::WorkloadUpdate {
                state: ws,
                message,
                ..
            } => {
                assert_eq!(ws, WorkloadState::Failed);
                assert!(message.unwrap().contains("validation failed"));
            }
            _ => panic!("expected WorkloadUpdate message"),
        }

        // No container should be created
        assert_eq!(runtime.container_count().await, 0);
    }

    #[tokio::test]
    async fn test_handle_start_workload_duplicate() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(4)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, _rx) = make_context(Arc::clone(&state), runtime).await;

        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new("nginx:latest");

        // Start first workload
        let msg1 = GatewayMessage::StartWorkload {
            workload_id,
            spec: spec.clone(),
        };
        handle_gateway_message_async(msg1, &ctx)
            .await
            .expect("first start should succeed");

        // Try to start same workload again
        let msg2 = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };
        let result = handle_gateway_message_async(msg2, &ctx).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), NodeError::WorkloadExists(_)));
    }

    #[tokio::test]
    async fn test_handle_start_workload_with_env() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(4)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, _rx) = make_context(Arc::clone(&state), runtime).await;

        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new("python:3.11")
            .with_command(vec!["python".to_string(), "app.py".to_string()])
            .with_env("DEBUG", "true")
            .with_env("PORT", "8080");

        let msg = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };

        let result = handle_gateway_message_async(msg, &ctx).await;

        assert!(result.is_ok());
        let state_guard = state.read().await;
        assert_eq!(state_guard.workload_count(), 1);
    }

    // ==================== StopWorkload Message Tests ====================

    #[tokio::test]
    async fn test_handle_stop_workload_success() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(4)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, _rx) = make_context(Arc::clone(&state), Arc::clone(&runtime)).await;

        // First, start a workload
        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new("nginx:latest").with_gpu_count(2);

        let start_msg = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };
        handle_gateway_message_async(start_msg, &ctx)
            .await
            .expect("start should succeed");

        {
            let state_guard = state.read().await;
            assert_eq!(state_guard.workload_count(), 1);
            assert_eq!(state_guard.available_gpu_count(), 2);
        }

        // Now stop it
        let stop_msg = GatewayMessage::StopWorkload {
            workload_id,
            grace_period_secs: 30,
        };

        let result = handle_gateway_message_async(stop_msg, &ctx).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.is_some());

        // Check response is a stopped update
        match response.unwrap() {
            NodeMessage::WorkloadUpdate {
                workload_id: id,
                state: ws,
                ..
            } => {
                assert_eq!(id, workload_id);
                assert_eq!(ws, WorkloadState::Stopped);
            }
            _ => panic!("expected WorkloadUpdate message"),
        }

        // Check state was updated
        let state_guard = state.read().await;
        assert_eq!(state_guard.workload_count(), 0);
        assert_eq!(state_guard.available_gpu_count(), 4); // GPUs released
    }

    #[tokio::test]
    async fn test_handle_stop_workload_not_found() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(4)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, _rx) = make_context(state, runtime).await;

        let workload_id = test_workload_id();

        let msg = GatewayMessage::StopWorkload {
            workload_id,
            grace_period_secs: 30,
        };

        let result = handle_gateway_message_async(msg, &ctx).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), NodeError::WorkloadNotFound(_)));
    }

    #[tokio::test]
    async fn test_handle_stop_workload_releases_gpus() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(4)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, _rx) = make_context(Arc::clone(&state), runtime).await;

        // Start two workloads
        let workload_id1 = test_workload_id();
        let workload_id2 = test_workload_id();

        let msg1 = GatewayMessage::StartWorkload {
            workload_id: workload_id1,
            spec: WorkloadSpec::new("nginx:latest").with_gpu_count(2),
        };
        handle_gateway_message_async(msg1, &ctx)
            .await
            .expect("start should succeed");

        let msg2 = GatewayMessage::StartWorkload {
            workload_id: workload_id2,
            spec: WorkloadSpec::new("redis:latest").with_gpu_count(1),
        };
        handle_gateway_message_async(msg2, &ctx)
            .await
            .expect("start should succeed");

        {
            let state_guard = state.read().await;
            assert_eq!(state_guard.available_gpu_count(), 1);
        }

        // Stop first workload
        let stop_msg = GatewayMessage::StopWorkload {
            workload_id: workload_id1,
            grace_period_secs: 10,
        };
        handle_gateway_message_async(stop_msg, &ctx)
            .await
            .expect("stop should succeed");

        // 2 GPUs should be released, 1 still in use
        let state_guard = state.read().await;
        assert_eq!(state_guard.available_gpu_count(), 3);
        assert_eq!(state_guard.workload_count(), 1);
    }

    // ==================== RequestMetrics Message Tests ====================

    #[tokio::test]
    async fn test_handle_request_metrics() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(4)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, _rx) = make_context(state, runtime).await;

        let msg = GatewayMessage::RequestMetrics;

        let result = handle_gateway_message_async(msg, &ctx).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // ==================== Error Message Tests ====================

    #[tokio::test]
    async fn test_handle_error_message() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(4)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, _rx) = make_context(state, runtime).await;

        let msg = GatewayMessage::Error {
            code: 500,
            message: "Internal server error".to_string(),
        };

        let result = handle_gateway_message_async(msg, &ctx).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // ==================== get_workload_status_async Tests ====================

    #[tokio::test]
    async fn test_get_workload_status_running() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(4)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, _rx) = make_context(Arc::clone(&state), runtime).await;

        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new("nginx:latest");

        let msg = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };
        handle_gateway_message_async(msg, &ctx)
            .await
            .expect("start should succeed");

        let status = get_workload_status_async(workload_id, &state).await;

        assert!(status.is_ok());
        assert_eq!(status.unwrap(), WorkloadState::Running);
    }

    #[tokio::test]
    async fn test_get_workload_status_not_found() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(4)));
        let workload_id = test_workload_id();

        let status = get_workload_status_async(workload_id, &state).await;

        assert!(status.is_err());
        assert!(matches!(status.unwrap_err(), NodeError::WorkloadNotFound(_)));
    }

    // ==================== Integration Tests ====================

    #[tokio::test]
    async fn test_full_workload_lifecycle_via_handlers() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(8)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, _rx) = make_context(Arc::clone(&state), runtime).await;

        let workload_id = test_workload_id();

        // Start workload
        let start_msg = GatewayMessage::StartWorkload {
            workload_id,
            spec: WorkloadSpec::new("training:v1")
                .with_gpu_count(4)
                .with_memory_mb(16384)
                .with_env("CUDA_VISIBLE_DEVICES", "0,1,2,3"),
        };

        let start_result = handle_gateway_message_async(start_msg, &ctx).await;
        assert!(start_result.is_ok());

        // Verify running
        let status = get_workload_status_async(workload_id, &state).await;
        assert_eq!(status.unwrap(), WorkloadState::Running);
        {
            let state_guard = state.read().await;
            assert_eq!(state_guard.available_gpu_count(), 4);
        }

        // Stop workload
        let stop_msg = GatewayMessage::StopWorkload {
            workload_id,
            grace_period_secs: 60,
        };

        let stop_result = handle_gateway_message_async(stop_msg, &ctx).await;
        assert!(stop_result.is_ok());

        // Verify stopped
        let status = get_workload_status_async(workload_id, &state).await;
        assert!(status.is_err()); // Not found = stopped
        {
            let state_guard = state.read().await;
            assert_eq!(state_guard.available_gpu_count(), 8); // All GPUs released
        }
    }

    #[tokio::test]
    async fn test_multiple_workloads_lifecycle() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(8)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, _rx) = make_context(Arc::clone(&state), runtime).await;

        let id1 = test_workload_id();
        let id2 = test_workload_id();
        let id3 = test_workload_id();

        // Start three workloads
        for (id, gpu_count) in [(id1, 2), (id2, 3), (id3, 1)] {
            let msg = GatewayMessage::StartWorkload {
                workload_id: id,
                spec: WorkloadSpec::new("worker:latest").with_gpu_count(gpu_count),
            };
            handle_gateway_message_async(msg, &ctx)
                .await
                .expect("start should succeed");
        }

        {
            let state_guard = state.read().await;
            assert_eq!(state_guard.workload_count(), 3);
            assert_eq!(state_guard.available_gpu_count(), 2); // 8 - 2 - 3 - 1 = 2
        }

        // Stop middle workload
        let stop_msg = GatewayMessage::StopWorkload {
            workload_id: id2,
            grace_period_secs: 10,
        };
        handle_gateway_message_async(stop_msg, &ctx)
            .await
            .expect("stop should succeed");

        {
            let state_guard = state.read().await;
            assert_eq!(state_guard.workload_count(), 2);
            assert_eq!(state_guard.available_gpu_count(), 5); // 2 + 3 = 5
        }

        // Stop remaining workloads
        for id in [id1, id3] {
            let msg = GatewayMessage::StopWorkload {
                workload_id: id,
                grace_period_secs: 10,
            };
            handle_gateway_message_async(msg, &ctx)
                .await
                .expect("stop should succeed");
        }

        let state_guard = state.read().await;
        assert_eq!(state_guard.workload_count(), 0);
        assert_eq!(state_guard.available_gpu_count(), 8);
    }

    // ==================== Log Streaming Tests ====================

    #[tokio::test]
    async fn test_log_messages_sent_during_start() {
        let state = Arc::new(RwLock::new(NodeState::with_gpus(4)));
        let runtime = Arc::new(FakeAsyncContainerRuntime::new());
        let (ctx, mut rx) = make_context(Arc::clone(&state), runtime).await;

        let workload_id = test_workload_id();
        let spec = WorkloadSpec::new("nginx:latest");

        let msg = GatewayMessage::StartWorkload {
            workload_id,
            spec,
        };

        let _result = handle_gateway_message_async(msg, &ctx).await;

        // Should receive a "starting" status message
        let first_msg = tokio::time::timeout(tokio::time::Duration::from_millis(100), rx.recv())
            .await
            .expect("should receive message")
            .expect("channel should not be closed");

        match first_msg {
            NodeMessage::WorkloadUpdate { state: ws, .. } => {
                assert_eq!(ws, WorkloadState::Starting);
            }
            _ => panic!("expected WorkloadUpdate message"),
        }
    }
}
