//! Bridge protocol types

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC request
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    /// Request ID for correlation
    pub id: u64,
    /// Method name (e.g., "cluster_status", "workload_submit")
    pub method: String,
    /// Method parameters
    #[serde(default)]
    pub params: Value,
}

/// JSON-RPC response
#[derive(Debug, Clone, Serialize)]
pub struct Response {
    /// Request ID for correlation
    pub id: u64,
    /// Result on success
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error on failure
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorResponse>,
}

impl Response {
    /// Create a success response
    pub fn success(id: u64, result: impl Serialize) -> Self {
        Self {
            id,
            result: Some(serde_json::to_value(result).unwrap_or(Value::Null)),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(id: u64, code: i32, message: impl Into<String>) -> Self {
        Self {
            id,
            result: None,
            error: Some(ErrorResponse {
                code,
                message: message.into(),
            }),
        }
    }
}

/// JSON-RPC error
#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
}

// Standard JSON-RPC error codes
pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

// Custom error codes
pub const NOT_FOUND: i32 = -32000;
pub const PERMISSION_DENIED: i32 = -32001;
pub const RESOURCE_EXHAUSTED: i32 = -32002;
pub const UNAVAILABLE: i32 = -32003;
