//! # claw-observe
//!
//! AI-powered observability analyzer for Clawbernetes.
//!
//! This crate provides intelligent analysis of metrics and logs, generating
//! human-readable insights and actionable recommendations. It replaces traditional
//! dashboard-based monitoring with AI-driven diagnosis.
//!
//! ## Features
//!
//! - **Health Status Tracking**: Monitor node, workload, and cluster health
//! - **Insight Generation**: Automatically detect anomalies and patterns
//! - **Cross-Signal Correlation**: Correlate metrics with logs for root cause analysis
//! - **Human-Readable Output**: Generate clear, actionable recommendations
//!
//! ## Quick Start
//!
//! ```rust
//! use claw_observe::{HealthStatus, Insight, Severity, Diagnosis};
//!
//! // Create an insight about a detected issue
//! let insight = Insight::new(
//!     Severity::Warning,
//!     "High Memory Usage",
//!     "Memory utilization has exceeded 85%"
//! )
//! .with_evidence("Current usage: 87%")
//! .with_recommendation("Consider scaling up or optimizing memory usage");
//!
//! // Create a diagnosis with the insight
//! let diagnosis = Diagnosis::new(HealthStatus::Degraded)
//!     .with_subject("node-001")
//!     .with_insight(insight);
//!
//! assert!(diagnosis.status.requires_attention());
//! ```

pub mod analyzer;
pub mod correlator;
pub mod detectors;
pub mod formatter;
pub mod types;

// Re-export core types for convenience
pub use types::{
    AnalysisScope, Diagnosis, HealthStatus, Insight, Severity, TimeRange,
};

// Re-export detector types and functions
pub use detectors::{
    detect_error_spike, detect_gpu_thermal_throttle, detect_memory_pressure,
    detect_node_offline, detect_performance_degradation, run_all_detectors,
    DetectorConfig, LogEntry, LogLevel, MetricPoint,
};

// Re-export correlator types and functions
pub use correlator::{
    build_timeline, correlate_metrics_logs, find_root_cause,
    Correlation, CorrelationType, CorrelatorConfig,
    EventSeverity, EventType, TimelineEvent,
};

// Re-export formatter functions
pub use formatter::{
    format_as_json, format_as_json_compact, format_diagnosis,
    format_for_agent, format_insight_list, format_summary,
};

// Re-export analyzer types
pub use analyzer::{Analyzer, AnalyzerConfig, AnalysisResult};
