//! Persistence layer for node state — re-exports from dedicated crates.
//!
//! Each store is now implemented in its own crate. This module re-exports
//! all types so existing `use crate::persist::*` imports continue to work.

// Foundation
pub use claw_persist::JsonStore;

// Config
pub use claw_config::{ConfigEntry, ConfigStore};

// Deploy & Workloads
pub use claw_deploy::{DeployRecord, DeployRevision, DeployStore, WorkloadRecord, WorkloadStore};

// Secrets
pub use claw_secrets::{SecretEntry, SecretStore};

// Storage
pub use claw_storage::{BackupEntry, BackupStore, VolumeRecord, VolumeStore};

// Auth & RBAC
pub use claw_auth::{ApiKeyRecord, ApiKeyStore, AuditLogEntry, AuditLogStore};

// Autoscaling
pub use claw_autoscaler::{AutoscaleRecord, AutoscaleStore};

// Scheduling, Jobs, Namespaces, Policies, Alerts, Audit
pub use claw_scheduler::{
    AlertRule, AlertStore, AuditEntry, AuditStore, CronEntry, CronStore, JobEntry, JobStore,
    NamespaceEntry, NamespaceStore, PolicyEntry, PolicyStore, ResourceQuota, TaintEntry,
};

// Ingress & Service Discovery
pub use claw_ingress::{
    IngressEntry, IngressRule, NetworkPolicyEntry, ServiceEntry, ServiceStore,
};
