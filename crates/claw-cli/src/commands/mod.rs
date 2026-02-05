//! CLI command implementations.
//!
//! Each submodule implements a specific CLI command:
//! - [`status`] - Cluster status overview
//! - [`node`] - Node management
//! - [`run`] - Workload execution
//! - [`molt`] - MOLT network participation
//! - [`autoscale`] - Autoscaling management
//! - [`secret`] - Secret management
//! - [`auth`] - Authentication and API keys
//! - [`alert`] - Alert management
//! - [`tenant`] - Tenant management
//! - [`namespace`] - Namespace management
//! - [`service`] - Service discovery
//! - [`deploy`] - Workload deployment
//! - [`rollback`] - Workload rollback
//! - [`metrics`] - Metrics querying
//! - [`logs`] - Log viewing
//! - [`dashboard`] - Dashboard management
//! - [`preempt`] - Workload preemption
//! - [`priority`] - Priority management

pub mod alert;
pub mod auth;
pub mod autoscale;
pub mod dashboard;
pub mod deploy;
pub mod logs;
pub mod metrics;
pub mod molt;
pub mod namespace;
pub mod node;
pub mod preempt;
pub mod priority;
pub mod rollback;
pub mod run;
pub mod secret;
pub mod service;
pub mod status;
pub mod tenant;

pub use alert::AlertCommand;
pub use auth::AuthCommand;
pub use autoscale::AutoscaleCommand;
pub use dashboard::DashboardCommand;
pub use deploy::DeployCommand;
pub use logs::LogsCommand;
pub use metrics::MetricsCommand;
pub use molt::MoltCommand;
pub use namespace::NamespaceCommand;
pub use node::NodeCommand;
pub use preempt::PreemptCommand;
pub use priority::PriorityCommand;
pub use rollback::RollbackCommand;
pub use run::RunCommand;
pub use secret::SecretCommand;
pub use service::ServiceCommand;
pub use status::StatusCommand;
pub use tenant::TenantCommand;
