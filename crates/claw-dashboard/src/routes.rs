//! Route configuration for the dashboard API.

use std::sync::Arc;

use axum::routing::{get, Router};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::handlers::{
    get_metrics, get_node, get_status, get_workload, health_check, list_nodes, list_workloads,
    stream_events, stream_logs,
};
use crate::state::DashboardState;
use crate::websocket::ws_upgrade;

/// Create the dashboard API router.
pub fn create_router(state: Arc<DashboardState>) -> Router {
    let cors = build_cors_layer(state.config());

    let api_routes = Router::new()
        // Health check
        .route("/health", get(health_check))
        // Cluster status
        .route("/status", get(get_status))
        // Node endpoints
        .route("/nodes", get(list_nodes))
        .route("/nodes/{id}", get(get_node))
        // Workload endpoints
        .route("/workloads", get(list_workloads))
        .route("/workloads/{id}", get(get_workload))
        // Metrics endpoint
        .route("/metrics", get(get_metrics))
        // Log streaming
        .route("/logs/{workload_id}", get(stream_logs))
        // SSE events stream
        .route("/events", get(stream_events))
        // WebSocket endpoint
        .route("/ws", get(ws_upgrade));

    Router::new()
        .nest("/api", api_routes)
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

/// Build the CORS layer based on configuration.
fn build_cors_layer(config: &crate::config::DashboardConfig) -> CorsLayer {
    if config.cors_origins.is_empty() {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        let origins: Vec<_> = config
            .cors_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();

        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(Any)
            .allow_headers(Any)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use claw_gateway::{NodeRegistry, WorkloadManager};
    use http_body_util::BodyExt;
    use tokio::sync::Mutex;
    use tower::ServiceExt;

    fn make_test_state() -> Arc<DashboardState> {
        let config = crate::config::DashboardConfig::default();
        let registry = Arc::new(Mutex::new(NodeRegistry::new()));
        let workload_manager = Arc::new(Mutex::new(WorkloadManager::new()));
        Arc::new(DashboardState::new(config, registry, workload_manager))
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let state = make_test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/health")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["status"], "ok");
    }

    #[tokio::test]
    async fn test_status_endpoint() {
        let state = make_test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/status")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["total_nodes"], 0);
        assert_eq!(json["total_workloads"], 0);
    }

    #[tokio::test]
    async fn test_nodes_endpoint() {
        let state = make_test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/nodes")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

        assert!(json.is_empty());
    }

    #[tokio::test]
    async fn test_node_not_found() {
        let state = make_test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri(&format!("/api/nodes/{}", uuid::Uuid::new_v4()))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_workloads_endpoint() {
        let state = make_test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/workloads")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

        assert!(json.is_empty());
    }

    #[tokio::test]
    async fn test_workloads_with_state_filter() {
        let state = make_test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/workloads?state=running")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_workloads_with_invalid_state() {
        let state = make_test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/workloads?state=invalid")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_workload_not_found() {
        let state = make_test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri(&format!("/api/workloads/{}", uuid::Uuid::new_v4()))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_metrics_endpoint() {
        let state = make_test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/metrics")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(json["nodes"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_logs_workload_not_found() {
        let state = make_test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri(&format!("/api/logs/{}", uuid::Uuid::new_v4()))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_cors_any_origin() {
        let state = make_test_state();
        let app = create_router(state);

        let request = Request::builder()
            .method("OPTIONS")
            .uri("/api/health")
            .header("Origin", "http://example.com")
            .header("Access-Control-Request-Method", "GET")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        // Should allow the request (might be 200 or 204 depending on axum version)
        assert!(response.status().is_success() || response.status() == StatusCode::NO_CONTENT || response.status() == StatusCode::OK);
    }

    #[tokio::test]
    async fn test_cors_specific_origins() {
        let config = crate::config::DashboardConfig::default()
            .with_cors_origin("http://localhost:3000");
        let registry = Arc::new(Mutex::new(NodeRegistry::new()));
        let workload_manager = Arc::new(Mutex::new(WorkloadManager::new()));
        let state = Arc::new(DashboardState::new(config, registry, workload_manager));
        let _app = create_router(state);

        // Router created successfully with specific CORS origins
    }

    #[tokio::test]
    async fn test_unknown_endpoint() {
        let state = make_test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/unknown")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_root_path() {
        let state = make_test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        // Root path is not configured, should return 404
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
