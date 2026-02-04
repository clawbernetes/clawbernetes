//! # claw-gateway
//!
//! Gateway service for `OpenClaw` node fleet management and workload scheduling.
//!
//! This crate provides the core components for managing a fleet of compute nodes:
//!
//! - [`NodeRegistry`] - Track registered nodes and their capabilities
//! - [`WorkloadManager`] - Manage workload lifecycle and state
//! - [`Scheduler`] - GPU-aware workload placement
//! - [`WorkloadDispatcher`] - Coordinate workload submission, scheduling, and dispatch

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod dispatch;
pub mod logs;
pub mod registry;
pub mod scheduler;
pub mod workload;

pub use dispatch::{DispatchError, WorkloadDispatcher};
pub use logs::{WorkloadLogStore, WorkloadLogs, DEFAULT_MAX_LINES};
pub use registry::{
    HealthSummary, NodeHealthStatus, NodeRegistry, RegisteredNode, RegistryError,
    HEARTBEAT_INTERVAL_SECS, OFFLINE_THRESHOLD_MISSED, UNHEALTHY_THRESHOLD_MISSED,
};
pub use scheduler::{Scheduler, SchedulerError};
pub use workload::{TrackedWorkload, WorkloadManager, WorkloadManagerError};
