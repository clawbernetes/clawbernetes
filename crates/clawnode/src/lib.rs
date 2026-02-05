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
//! - Resource exhaustion prevention — limits, tracking, alerts

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod config;
pub mod docker;
pub mod error;
pub mod gateway;
pub mod gpu;
pub mod handlers;
pub mod handlers_async;
pub mod metrics;
pub mod network;
pub mod node;
pub mod resources;
pub mod runtime;
pub mod state;

pub use docker::{AsyncContainerRuntime, FakeAsyncContainerRuntime};
#[cfg(feature = "docker")]
pub use docker::DockerContainerRuntime;
pub use error::NodeError;
pub use handlers::{get_workload_status, handle_gateway_message, HandlerContext};
pub use handlers_async::{
    get_workload_status_async, handle_gateway_message_async, AsyncHandlerContext,
};
pub use node::Node;
pub use resources::{
    EffectiveResourceLimits, ExecutionWatchdog, NodeCapacity, ResourceAlert, ResourceLimits,
    ResourceTracker, ResourceType, ResourceUsage,
};
pub use state::{GatewayConnectionState, NodeState, WorkloadInfo};
