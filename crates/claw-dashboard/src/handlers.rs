//! HTTP request handlers for the dashboard API.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use claw_proto::{NodeId, WorkloadId, WorkloadState};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::error::{DashboardError, DashboardResult};
use crate::state::DashboardState;
use crate::types::{ClusterStatus, LiveUpdate, MetricsSnapshot, NodeStatus, WorkloadStatus};

/// Query parameters for workload filtering.
#[derive(Debug, Deserialize)]
pub struct WorkloadQuery {
    /// Filter by state.
    pub state: Option<String>,
    /// Filter by node ID.
    pub node_id: Option<String>,
    /// Limit number of results.
    pub limit: Option<usize>,
}

/// Query parameters for log streaming.
#[derive(Debug, Deserialize)]
pub struct LogQuery {
    /// Number of lines to return.
    pub lines: Option<usize>,
    /// Follow logs in real-time.
    pub follow: Option<bool>,
}

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Status message.
    pub status: String,
    /// Server uptime in seconds.
    pub uptime_secs: u64,
}

/// Handle GET /api/health - health check endpoint.
pub async fn health_check(State(state): State<Arc<DashboardState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        uptime_secs: state.uptime_secs(),
    })
}

/// Handle GET /api/status - cluster overview.
pub async fn get_status(
    State(state): State<Arc<DashboardState>>,
) -> DashboardResult<Json<ClusterStatus>> {
    let status = state.get_cluster_status().await;
    Ok(Json(status))
}

/// Handle GET /api/nodes - list all nodes.
pub async fn list_nodes(
    State(state): State<Arc<DashboardState>>,
) -> DashboardResult<Json<Vec<NodeStatus>>> {
    let nodes = state.get_nodes().await;
    Ok(Json(nodes))
}

/// Handle GET /api/nodes/:id - get a specific node.
pub async fn get_node(
    State(state): State<Arc<DashboardState>>,
    Path(id): Path<String>,
) -> DashboardResult<Json<NodeStatus>> {
    let node_id = NodeId::parse(&id)
        .map_err(|_| DashboardError::InvalidRequest(format!("invalid node ID: {id}")))?;

    state
        .get_node(node_id)
        .await
        .map(Json)
        .ok_or_else(|| DashboardError::NotFound("node".to_string(), id))
}

/// Handle GET /api/workloads - list all workloads.
pub async fn list_workloads(
    State(state): State<Arc<DashboardState>>,
    Query(query): Query<WorkloadQuery>,
) -> DashboardResult<Json<Vec<WorkloadStatus>>> {
    let mut workloads = if let Some(state_filter) = &query.state {
        let state_enum = parse_workload_state(state_filter)?;
        state.get_workloads_by_state(state_enum).await
    } else {
        state.get_workloads().await
    };

    // Filter by node_id if provided
    if let Some(node_id_str) = &query.node_id {
        let node_id = NodeId::parse(node_id_str)
            .map_err(|_| DashboardError::InvalidRequest(format!("invalid node ID: {node_id_str}")))?;
        workloads.retain(|w| w.node_id == Some(node_id));
    }

    // Apply limit if provided
    if let Some(limit) = query.limit {
        workloads.truncate(limit);
    }

    Ok(Json(workloads))
}

/// Handle GET /api/workloads/:id - get a specific workload.
pub async fn get_workload(
    State(state): State<Arc<DashboardState>>,
    Path(id): Path<String>,
) -> DashboardResult<Json<WorkloadStatus>> {
    let workload_id = WorkloadId::parse(&id)
        .map_err(|_| DashboardError::InvalidRequest(format!("invalid workload ID: {id}")))?;

    state
        .get_workload(workload_id)
        .await
        .map(Json)
        .ok_or_else(|| DashboardError::NotFound("workload".to_string(), id))
}

/// Handle GET /api/metrics - current metrics snapshot.
pub async fn get_metrics(
    State(state): State<Arc<DashboardState>>,
) -> DashboardResult<Json<MetricsSnapshot>> {
    let metrics = state.get_metrics().await;
    Ok(Json(metrics))
}

/// Handle GET `/api/logs/:workload_id` - stream workload logs via SSE.
pub async fn stream_logs(
    State(state): State<Arc<DashboardState>>,
    Path(workload_id): Path<String>,
    Query(query): Query<LogQuery>,
) -> DashboardResult<Response> {
    let id = WorkloadId::parse(&workload_id)
        .map_err(|_| DashboardError::InvalidRequest(format!("invalid workload ID: {workload_id}")))?;

    // Verify workload exists
    if state.get_workload(id).await.is_none() {
        return Err(DashboardError::NotFound(
            "workload".to_string(),
            workload_id,
        ));
    }

    let lines = query.lines.unwrap_or(100);
    let logs = state.get_workload_logs(id, Some(lines)).await;

    // If follow is not requested, return logs as JSON
    if !query.follow.unwrap_or(false) {
        return Ok(Json(logs).into_response());
    }

    // Otherwise, stream logs via SSE
    let initial_logs = logs;
    let update_rx = state.subscribe();

    let stream = create_log_stream(id, initial_logs, update_rx);

    Ok(Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response())
}

/// Create a stream of log events for SSE.
fn create_log_stream(
    workload_id: WorkloadId,
    initial_logs: Vec<String>,
    update_rx: tokio::sync::broadcast::Receiver<LiveUpdate>,
) -> impl Stream<Item = Result<Event, std::convert::Infallible>> {
    // Send initial logs
    let initial_stream = stream::iter(initial_logs.into_iter().map(|line| {
        Ok(Event::default().event("log").data(line))
    }));

    // Then stream new log lines
    let update_stream = BroadcastStream::new(update_rx)
        .filter_map(move |result| {
            match result {
                Ok(LiveUpdate::LogLine { entry }) if entry.workload_id == workload_id => {
                    Some(Ok(Event::default().event("log").data(entry.message)))
                }
                _ => None,
            }
        });

    initial_stream.chain(update_stream)
}

/// Handle GET /api/events - SSE stream of all live updates.
pub async fn stream_events(
    State(state): State<Arc<DashboardState>>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let update_rx = state.subscribe();

    let stream = BroadcastStream::new(update_rx).filter_map(|result| {
        match result {
            Ok(update) => {
                let event_type = update.event_type();
                match serde_json::to_string(&update) {
                    Ok(data) => Some(Ok(Event::default().event(event_type).data(data))),
                    Err(_) => None,
                }
            }
            Err(_) => None,
        }
    });

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

/// Parse a workload state string into the enum.
fn parse_workload_state(s: &str) -> DashboardResult<WorkloadState> {
    match s.to_lowercase().as_str() {
        "pending" => Ok(WorkloadState::Pending),
        "starting" => Ok(WorkloadState::Starting),
        "running" => Ok(WorkloadState::Running),
        "stopping" => Ok(WorkloadState::Stopping),
        "stopped" => Ok(WorkloadState::Stopped),
        "completed" => Ok(WorkloadState::Completed),
        "failed" => Ok(WorkloadState::Failed),
        _ => Err(DashboardError::InvalidRequest(format!(
            "invalid workload state: {s}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_gateway::{NodeRegistry, WorkloadManager};
    use claw_proto::NodeCapabilities;
    use tokio::sync::Mutex;

    fn make_test_state() -> Arc<DashboardState> {
        let config = crate::config::DashboardConfig::default();
        let registry = Arc::new(Mutex::new(NodeRegistry::new()));
        let workload_manager = Arc::new(Mutex::new(WorkloadManager::new()));
        Arc::new(DashboardState::new(config, registry, workload_manager))
    }

    #[test]
    fn test_parse_workload_state_valid() {
        assert_eq!(parse_workload_state("pending").unwrap(), WorkloadState::Pending);
        assert_eq!(parse_workload_state("RUNNING").unwrap(), WorkloadState::Running);
        assert_eq!(parse_workload_state("Failed").unwrap(), WorkloadState::Failed);
    }

    #[test]
    fn test_parse_workload_state_invalid() {
        let result = parse_workload_state("invalid");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_health_check() {
        let state = make_test_state();
        let response = health_check(State(state)).await;

        assert_eq!(response.status, "ok");
    }

    #[tokio::test]
    async fn test_get_status_empty() {
        let state = make_test_state();
        let response = get_status(State(state)).await.unwrap();

        assert_eq!(response.total_nodes, 0);
        assert_eq!(response.total_workloads, 0);
    }

    #[tokio::test]
    async fn test_list_nodes_empty() {
        let state = make_test_state();
        let response = list_nodes(State(state)).await.unwrap();

        assert!(response.0.is_empty());
    }

    #[tokio::test]
    async fn test_list_nodes_with_data() {
        let state = make_test_state();

        // Add a node
        {
            let registry = state.registry();
            let mut registry = registry.lock().await;
            registry
                .register_with_name(NodeId::new(), "test-node", NodeCapabilities::new(8, 16384))
                .unwrap();
        }

        let response = list_nodes(State(state)).await.unwrap();

        assert_eq!(response.0.len(), 1);
        assert_eq!(response.0[0].name, "test-node");
    }

    #[tokio::test]
    async fn test_get_node_not_found() {
        let state = make_test_state();
        let id = uuid::Uuid::new_v4().to_string();

        let result = get_node(State(state), Path(id.clone())).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, DashboardError::NotFound(_, _)));
    }

    #[tokio::test]
    async fn test_get_node_invalid_id() {
        let state = make_test_state();

        let result = get_node(State(state), Path("invalid".to_string())).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, DashboardError::InvalidRequest(_)));
    }

    #[tokio::test]
    async fn test_list_workloads_empty() {
        let state = make_test_state();
        let query = WorkloadQuery {
            state: None,
            node_id: None,
            limit: None,
        };

        let response = list_workloads(State(state), Query(query)).await.unwrap();

        assert!(response.0.is_empty());
    }

    #[tokio::test]
    async fn test_list_workloads_with_state_filter() {
        let state = make_test_state();
        let query = WorkloadQuery {
            state: Some("running".to_string()),
            node_id: None,
            limit: None,
        };

        let response = list_workloads(State(state), Query(query)).await.unwrap();

        assert!(response.0.is_empty()); // No workloads yet
    }

    #[tokio::test]
    async fn test_list_workloads_with_invalid_state() {
        let state = make_test_state();
        let query = WorkloadQuery {
            state: Some("invalid".to_string()),
            node_id: None,
            limit: None,
        };

        let result = list_workloads(State(state), Query(query)).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_workload_not_found() {
        let state = make_test_state();
        let id = uuid::Uuid::new_v4().to_string();

        let result = get_workload(State(state), Path(id.clone())).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_metrics() {
        let state = make_test_state();
        let response = get_metrics(State(state)).await.unwrap();

        assert!(response.nodes.is_empty());
        assert_eq!(response.gpu_summary.total_gpus, 0);
    }

    #[tokio::test]
    async fn test_stream_logs_workload_not_found() {
        let state = make_test_state();
        let query = LogQuery {
            lines: Some(100),
            follow: Some(false),
        };

        let result = stream_logs(
            State(state),
            Path(uuid::Uuid::new_v4().to_string()),
            Query(query),
        )
        .await;

        assert!(result.is_err());
    }

    #[test]
    fn test_health_response_serialization() {
        let response = HealthResponse {
            status: "ok".to_string(),
            uptime_secs: 3600,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("ok"));
        assert!(json.contains("3600"));
    }
}
