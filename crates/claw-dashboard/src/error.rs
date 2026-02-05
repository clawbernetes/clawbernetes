//! Error types for the dashboard server.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use thiserror::Error;

/// Result type alias for dashboard operations.
pub type DashboardResult<T> = Result<T, DashboardError>;

/// Errors that can occur in the dashboard server.
#[derive(Debug, Error)]
pub enum DashboardError {
    /// Failed to bind to the specified address.
    #[error("failed to bind to {0}: {1}")]
    BindFailed(std::net::SocketAddr, std::io::Error),

    /// Resource not found.
    #[error("{0} not found: {1}")]
    NotFound(String, String),

    /// Invalid request parameters.
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// Internal server error.
    #[error("internal error: {0}")]
    Internal(String),

    /// WebSocket error.
    #[error("websocket error: {0}")]
    WebSocket(String),

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Too many connections.
    #[error("too many connections: {0} active, limit is {1}")]
    TooManyConnections(usize, usize),

    /// Channel send error.
    #[error("channel send failed: {0}")]
    ChannelSend(String),
}

/// JSON error response body.
#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
}

impl IntoResponse for DashboardError {
    fn into_response(self) -> Response {
        let (status, error_type) = match &self {
            Self::NotFound(_, _) => (StatusCode::NOT_FOUND, "not_found"),
            Self::InvalidRequest(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
            Self::TooManyConnections(_, _) => (StatusCode::SERVICE_UNAVAILABLE, "too_many_connections"),
            Self::BindFailed(_, _) | Self::Internal(_) | Self::WebSocket(_) | Self::Serialization(_) | Self::ChannelSend(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };

        let body = ErrorResponse {
            error: error_type.to_string(),
            message: self.to_string(),
        };

        let json = serde_json::to_string(&body).unwrap_or_else(|_| {
            r#"{"error":"internal_error","message":"failed to serialize error"}"#.to_string()
        });

        (
            status,
            [("content-type", "application/json")],
            json,
        )
            .into_response()
    }
}

impl From<serde_json::Error> for DashboardError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    #[tokio::test]
    async fn test_not_found_error_response() {
        let err = DashboardError::NotFound("workload".to_string(), "abc123".to_string());
        let response = err.into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = response.into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(json["error"], "not_found");
        assert!(json["message"].as_str().unwrap().contains("workload"));
    }

    #[tokio::test]
    async fn test_invalid_request_error_response() {
        let err = DashboardError::InvalidRequest("missing field".to_string());
        let response = err.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_internal_error_response() {
        let err = DashboardError::Internal("something broke".to_string());
        let response = err.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_too_many_connections_error_response() {
        let err = DashboardError::TooManyConnections(100, 50);
        let response = err.into_response();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn test_from_serde_error() {
        let serde_err = serde_json::from_str::<i32>("invalid").unwrap_err();
        let err = DashboardError::from(serde_err);

        assert!(matches!(err, DashboardError::Serialization(_)));
    }

    #[test]
    fn test_error_display() {
        let err = DashboardError::NotFound("node".to_string(), "123".to_string());
        assert_eq!(err.to_string(), "node not found: 123");

        let err = DashboardError::InvalidRequest("bad param".to_string());
        assert_eq!(err.to_string(), "invalid request: bad param");
    }
}
