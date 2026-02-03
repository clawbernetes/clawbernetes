//! # claw-proto
//!
//! Protocol definitions for Clawbernetes node-gateway communication.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod messages;
pub mod types;

pub use error::ProtoError;
pub use messages::{GatewayMessage, NodeMessage};
pub use types::{
    GpuCapability, GpuMetricsProto, NodeCapabilities, NodeId, WorkloadId, WorkloadSpec,
    WorkloadState,
};
