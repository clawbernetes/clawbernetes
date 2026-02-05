//! Clawbernetes Bridge
//!
//! JSON-RPC bridge for OpenClaw plugin integration. This crate provides a stdio-based
//! interface for the TypeScript plugin to communicate with Clawbernetes Rust crates.
//!
//! ## Protocol
//!
//! The bridge uses a simple JSON-RPC-like protocol over stdin/stdout:
//!
//! Request:
//! ```json
//! {"id": 1, "method": "cluster_status", "params": {}}
//! ```
//!
//! Response:
//! ```json
//! {"id": 1, "result": {...}}
//! ```
//!
//! Error:
//! ```json
//! {"id": 1, "error": {"code": -32000, "message": "..."}}
//! ```

pub mod error;
pub mod handlers;
pub mod protocol;

pub use error::{BridgeError, BridgeResult};
pub use handlers::handle_request;
pub use protocol::{ErrorResponse, Request, Response};
