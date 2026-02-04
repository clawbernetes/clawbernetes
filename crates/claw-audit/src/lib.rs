//! # claw-audit
//!
//! Security audit logging for Clawbernetes.
//!
//! This crate provides a lightweight, pluggable audit logging system for
//! security-relevant events. It's designed to integrate with the existing
//! `tracing` infrastructure while supporting custom backends.
//!
//! ## Features
//!
//! - [`AuditEvent`] — Enum covering all security-relevant events
//! - [`AuditLogger`] — Pluggable trait for audit backends
//! - [`TracingAuditLogger`] — Default implementation using `tracing`
//! - [`AuditEventBuilder`] — Fluent builder for creating events
//!
//! ## Example
//!
//! ```rust
//! use claw_audit::{AuditEvent, AuditLogger, TracingAuditLogger, Severity};
//! use uuid::Uuid;
//!
//! // Create a logger
//! let logger = TracingAuditLogger::new();
//!
//! // Log an authentication failure
//! let event = AuditEvent::authentication_failure(
//!     "invalid_credentials",
//!     "192.168.1.100",
//! );
//! logger.log(&event);
//!
//! // Log an authorization failure using the builder
//! let event = AuditEvent::builder()
//!     .authorization_failure()
//!     .actor_id(Uuid::new_v4())
//!     .resource("workload:abc123")
//!     .action("deploy")
//!     .reason("insufficient_permissions")
//!     .build();
//!
//! if let Ok(event) = event {
//!     logger.log(&event);
//! }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod events;
pub mod logger;

// Re-export main types
pub use error::{AuditError, Result};
pub use events::{
    AuditEvent, AuditEventBuilder, AuthAttempt, AuthorizationContext, EscrowChange,
    RateLimitViolation, Severity, SignatureFailure, UnusualPattern,
};
pub use logger::{AuditLogger, NoopAuditLogger, TracingAuditLogger};
