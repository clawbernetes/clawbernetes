//! # claw-proto
//!
//! Protocol definitions for Clawbernetes node-gateway communication.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

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
