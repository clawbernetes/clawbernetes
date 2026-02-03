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
pub mod registry;
pub mod scheduler;
pub mod workload;

pub use dispatch::{DispatchError, WorkloadDispatcher};
pub use registry::{NodeRegistry, RegisteredNode, RegistryError};
pub use scheduler::{Scheduler, SchedulerError};
pub use workload::{TrackedWorkload, WorkloadManager, WorkloadManagerError};
