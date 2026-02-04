//! # claw-proto
//!
//! Protocol definitions for Clawbernetes node-gateway communication.
//!
//! This crate provides message types for two protocols:
//!
//! ## Node Protocol
//!
//! Communication between `clawnode` instances and the gateway:
//! - [`NodeMessage`] — Messages from nodes to gateway
//! - [`GatewayMessage`] — Messages from gateway to nodes
//!
//! ## CLI Protocol
//!
//! Communication between `claw-cli` and the gateway for administration:
//! - [`cli::CliMessage`] — Requests from CLI to gateway
//! - [`cli::CliResponse`] — Responses from gateway to CLI

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;
pub mod error;
pub mod events;
pub mod messages;
pub mod types;
pub mod validation;
pub mod workload;

pub use error::ProtoError;
pub use events::{EventLog, EventMetadata, WorkloadEvent, WorkloadEventKind};
pub use messages::{GatewayMessage, NodeConfig, NodeMessage, MAX_WORKLOAD_LOG_LINES};
pub use types::{
    GpuCapability, GpuMetricsProto, NodeCapabilities, NodeId, WorkloadId,
    WorkloadState,
};
pub use validation::{
    validate_env_key, validate_image, validate_resources, ValidationError, ValidationResult,
};
pub use workload::{is_valid_transition, Workload, WorkloadSpec, WorkloadStatus};

// Re-export CLI types for convenience
pub use cli::{CliMessage, CliResponse, NodeInfo, NodeState, WorkloadInfo};
