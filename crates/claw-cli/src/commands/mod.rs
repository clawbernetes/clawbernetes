//! CLI command implementations.
//!
//! Each submodule implements a specific CLI command:
//! - [`status`] - Cluster status overview
//! - [`node`] - Node management
//! - [`run`] - Workload execution
//! - [`molt`] - MOLT network participation
//! - [`autoscale`] - Autoscaling management

pub mod autoscale;
pub mod molt;
pub mod node;
pub mod run;
pub mod status;

pub use autoscale::AutoscaleCommand;
pub use molt::MoltCommand;
pub use node::NodeCommand;
pub use run::RunCommand;
pub use status::StatusCommand;
