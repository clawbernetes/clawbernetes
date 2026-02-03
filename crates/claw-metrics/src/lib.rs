//! Embedded time-series database for GPU workload monitoring.
#![forbid(unsafe_code)]
//!
//! `claw-metrics` is a lightweight, high-performance metrics system designed
//! specifically for Clawbernetes GPU workload monitoring. It provides a simple
//! push-based API without the complexity of Prometheus query language.
//!
//! # Features
//!
//! - **Simple Push API**: No complex query languages, just push metrics and query them
//! - **GPU-optimized**: Designed for GPU metrics like utilization, memory, temperature, power
//! - **Retention Policies**: Automatic downsampling and expiry of old data
//! - **Fast Queries**: Optimized for recent data access (last hour)
//!
//! # Example
//!
//! ```rust
//! use claw_metrics::{MetricPoint, MetricName, MetricStore, TimeRange};
//! use std::time::Duration;
//!
//! // Create a store with 1 hour retention
//! let store = MetricStore::new(Duration::from_secs(3600));
//!
//! // Create a metric name
//! let name = MetricName::new("gpu_utilization").unwrap();
//!
//! // Push a metric point
//! store.push(&name, MetricPoint::now(85.5).label("gpu_id", "0")).unwrap();
//!
//! // Query recent data
//! let range = TimeRange::last_minutes(5);
//! let points = store.query(&name, range, None).unwrap();
//! ```

#![doc(html_root_url = "https://docs.rs/claw-metrics/0.1.0")]
#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]

pub mod error;
pub mod types;
pub mod storage;
pub mod query;
pub mod collector;

// Re-export main types at crate root
pub use error::{MetricsError, Result};
pub use types::{Aggregation, MetricName, MetricPoint, TimeRange};
pub use storage::MetricStore;
pub use query::{average_over, last_value, max_over, rate};
pub use collector::{GpuMetricCollector, SystemMetricCollector, MetricCollector};
