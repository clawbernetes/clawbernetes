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
//! - **Prometheus Support**: Optional Prometheus-compatible metrics (with `prometheus` feature)
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
//!
//! # Prometheus Integration
//!
//! Enable the `prometheus` feature to expose Prometheus-compatible metrics:
//!
//! ```toml
//! [dependencies]
//! claw-metrics = { version = "0.1", features = ["prometheus"] }
//! ```
//!
//! ```rust,ignore
//! use claw_metrics::prometheus::{PrometheusRegistry, MetricsHandler};
//!
//! // Create the registry
//! let registry = PrometheusRegistry::new();
//!
//! // Update metrics
//! registry.gateway_metrics().set_nodes_total(5);
//! registry.gateway_metrics().inc_workloads_total("running");
//!
//! // Create a handler for HTTP endpoints
//! let handler = MetricsHandler::new(registry.clone());
//!
//! // Handle /metrics requests
//! let response = handler.handle();
//! // response.body contains Prometheus text format
//! // response.content_type is "text/plain; version=0.0.4; charset=utf-8"
//! ```

#![doc(html_root_url = "https://docs.rs/claw-metrics/0.1.0")]
#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]

pub mod collector;
pub mod error;
pub mod query;
pub mod storage;
pub mod types;

#[cfg(feature = "prometheus")]
pub mod prometheus;

// Re-export main types at crate root
pub use collector::{GpuMetricCollector, MetricCollector, SystemMetricCollector};
pub use error::{MetricsError, Result};
pub use query::{average_over, last_value, max_over, rate};
pub use storage::MetricStore;
pub use types::{Aggregation, MetricName, MetricPoint, TimeRange};
