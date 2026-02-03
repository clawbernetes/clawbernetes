//! Error types for the gateway server.

use std::net::SocketAddr;

use claw_proto::NodeId;
use thiserror::Error;

/// Errors that can occur in the gateway server.
#[derive(Debug, Error)]
pub enum ServerError {
    /// Failed to bind to the specified address.
    #[error("failed to bind to {0}: {1}")]
    BindFailed(SocketAddr, std::io::Error),

    /// WebSocket error occurred.
    #[error("websocket error: {0}")]
    WebSocket(String),

    /// Failed to serialize or deserialize a message.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Node is not registered.
    #[error("node {0} is not registered")]
    NodeNotRegistered(NodeId),

    /// Protocol error.
    #[error("protocol error: {0}")]
    Protocol(String),

    /// Internal server error.
    #[error("internal error: {0}")]
    Internal(String),

    /// Connection closed.
    #[error("connection closed")]
    ConnectionClosed,

    /// Channel send error.
    #[error("channel send error: {0}")]
    ChannelSend(String),
}

impl From<serde_json::Error> for ServerError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

impl From<tokio_tungstenite::tungstenite::Error> for ServerError {
    fn from(err: tokio_tungstenite::tungstenite::Error) -> Self {
        Self::WebSocket(err.to_string())
    }
}

/// Result type for server operations.
pub type ServerResult<T> = Result<T, ServerError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_bind_failed_error_display() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let io_err = std::io::Error::new(std::io::ErrorKind::AddrInUse, "address in use");
        let err = ServerError::BindFailed(addr, io_err);
        
        let msg = err.to_string();
        assert!(msg.contains("127.0.0.1:8080"));
        assert!(msg.contains("address in use"));
    }

    #[test]
    fn test_websocket_error_display() {
        let err = ServerError::WebSocket("connection reset".to_string());
        assert!(err.to_string().contains("connection reset"));
    }

    #[test]
    fn test_serialization_error_display() {
        let err = ServerError::Serialization("invalid json".to_string());
        assert!(err.to_string().contains("invalid json"));
    }

    #[test]
    fn test_node_not_registered_display() {
        let node_id = NodeId::new();
        let err = ServerError::NodeNotRegistered(node_id);
        assert!(err.to_string().contains(&node_id.to_string()));
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_err = serde_json::from_str::<String>("invalid").unwrap_err();
        let err: ServerError = json_err.into();
        assert!(matches!(err, ServerError::Serialization(_)));
    }

    #[test]
    fn test_connection_closed_display() {
        let err = ServerError::ConnectionClosed;
        assert_eq!(err.to_string(), "connection closed");
    }
}
