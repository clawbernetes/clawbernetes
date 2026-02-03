//! # clawnode
//!
//! Clawbernetes node agent — the worker that runs on each compute node.
//!
//! This crate provides:
//!
//! - Hardware discovery — GPUs, CPUs, memory, network
//! - Container runtime — lifecycle management via containerd/podman
//! - Metrics streaming — GPU utilization, thermals, memory
//! - Gateway communication — WebSocket connection to control plane
//! - MOLT integration — optional P2P network participation

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod config;
pub mod error;
pub mod gateway;
pub mod gpu;
pub mod metrics;
pub mod runtime;

pub use error::NodeError;

// Re-exports will be added as types are implemented
