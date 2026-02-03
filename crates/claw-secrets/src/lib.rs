//! # Claw Secrets
//!
//! A secrets management system for Clawbernetes that provides:
//!
//! - **Encrypted at rest**: All secrets are encrypted using ChaCha20-Poly1305
//! - **Workload identity-based access**: Access control based on workload and node identities
//! - **Automatic rotation support**: Configurable rotation policies
//! - **Audit logging**: Complete audit trail of all secret access
//!
//! ## Example
//!
//! ```rust
//! use claw_secrets::{SecretId, SecretValue, AccessPolicy, WorkloadId};
//!
//! // Create a secret identifier
//! let id = SecretId::new("database.password").expect("valid id");
//!
//! // Create an access policy
//! let policy = AccessPolicy::allow_workloads(vec![
//!     WorkloadId::new("api-server"),
//!     WorkloadId::new("worker"),
//! ]);
//!
//! // Create an encrypted secret value
//! let value = SecretValue::new(vec![1, 2, 3, 4]); // encrypted bytes
//! ```
//!
//! ## Security Considerations
//!
//! - All secret values use `zeroize` to securely clear memory on drop
//! - Constant-time comparison is used for secret values to prevent timing attacks
//! - Debug output for secrets is redacted

pub mod access;
pub mod audit;
pub mod encryption;
pub mod error;
pub mod store;
pub mod types;

// Re-export commonly used types
pub use error::{Error, Result};
pub use types::{
    AccessPolicy, Accessor, AuditAction, AuditEntry, NodeId, RotationPolicy, SecretId,
    SecretMetadata, SecretValue, WorkloadId,
};

pub use encryption::SecretKey;

pub use store::SecretStore;

pub use access::AccessController;

pub use audit::{AuditFilter, AuditLog};
