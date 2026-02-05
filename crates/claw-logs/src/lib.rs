//! # claw-logs
//!
//! Structured logging system for Clawbernetes workloads.
//!
//! This crate provides:
//!
//! - [`LogEntry`] — Structured log entries with metadata
//! - [`LogLevel`] — Severity levels (Trace, Debug, Info, Warn, Error)
//! - [`LogFilter`] — Query filters for searching logs
//! - [`RetentionPolicy`] — Configurable retention rules
//! - [`LogStore`] — In-memory log storage with retention
//! - [`FileLogStore`] — File-based log storage with rotation
//! - [`LogStoreTrait`] — Abstract trait for log backends
//! - [`LogStream`] — Async streaming (tail -f equivalent)
//!
//! ## Example
//!
//! ```rust
//! use claw_logs::{LogEntry, LogLevel, LogFilter, LogId};
//! use chrono::Utc;
//! use uuid::Uuid;
//!
//! // Create a log entry
//! let entry = LogEntry::builder()
//!     .id(LogId(1))
//!     .timestamp(Utc::now())
//!     .level(LogLevel::Info)
//!     .message("Application started")
//!     .workload_id(Uuid::new_v4())
//!     .node_id(Uuid::new_v4())
//!     .build();
//!
//! // Create a filter
//! let filter = LogFilter::new()
//!     .with_level(LogLevel::Info)
//!     .with_contains("started");
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod collector;
pub mod error;
pub mod file_store;
pub mod index;
pub mod store;
pub mod traits;
pub mod types;

// Re-export main types
pub use collector::{
    LineParser, LogEntryFactory, LogSource, NodeLogCollector, ParsedLine, WorkloadLogCollector,
};
pub use error::{LogError, Result};
pub use file_store::{FileLogStore, FileLogStoreConfig};
pub use index::LogIndex;
pub use store::{shared_store, LogStore, LogStoreConfig, LogStream, SharedLogStore};
pub use traits::{LogStoreTrait, RotatableStore};
pub use types::{LogEntry, LogEntryBuilder, LogFilter, LogId, LogLevel, RetentionPolicy, TimeRange};
