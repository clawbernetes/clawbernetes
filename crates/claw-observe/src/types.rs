//! Core types for the observability analyzer.
//!
//! This module defines the fundamental data structures used throughout the claw-observe crate,
//! including health status indicators, insights, diagnoses, and analysis scopes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::ops::Range;
use uuid::Uuid;

/// Represents the health status of a node, workload, or cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    /// System is operating normally with no issues detected.
    Healthy,
    /// System is experiencing minor issues but still functional.
    Degraded,
    /// System is experiencing critical issues requiring immediate attention.
    Critical,
    /// Health status cannot be determined due to insufficient data.
    #[default]
    Unknown,
}

impl HealthStatus {
    /// Returns true if the status indicates the system is operational.
    #[must_use]
    pub const fn is_operational(&self) -> bool {
        matches!(self, Self::Healthy | Self::Degraded)
    }

    /// Returns true if the status requires attention.
    #[must_use]
    pub const fn requires_attention(&self) -> bool {
        matches!(self, Self::Degraded | Self::Critical)
    }

    /// Returns the severity level as a numeric value (0-3).
    /// Higher values indicate more severe status.
    #[must_use]
    pub const fn severity_level(&self) -> u8 {
        match self {
            Self::Healthy => 0,
            Self::Degraded => 1,
            Self::Critical => 2,
            Self::Unknown => 3,
        }
    }

    /// Returns a human-readable description of the status.
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::Healthy => "System is operating normally",
            Self::Degraded => "System is experiencing minor issues",
            Self::Critical => "System requires immediate attention",
            Self::Unknown => "Unable to determine system status",
        }
    }
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "Healthy"),
            Self::Degraded => write!(f, "Degraded"),
            Self::Critical => write!(f, "Critical"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Severity level for insights and alerts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational insight, no action required.
    Info,
    /// Warning that should be monitored.
    Warning,
    /// Error that needs attention.
    Error,
    /// Critical issue requiring immediate action.
    Critical,
}

impl Severity {
    /// Returns the severity as a numeric value for comparison.
    #[must_use]
    pub const fn as_level(&self) -> u8 {
        match self {
            Self::Info => 0,
            Self::Warning => 1,
            Self::Error => 2,
            Self::Critical => 3,
        }
    }

    /// Returns an emoji representation for display purposes.
    #[must_use]
    pub const fn emoji(&self) -> &'static str {
        match self {
            Self::Info => "ℹ️",
            Self::Warning => "⚠️",
            Self::Error => "❌",
            Self::Critical => "🚨",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARNING"),
            Self::Error => write!(f, "ERROR"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

impl Default for Severity {
    fn default() -> Self {
        Self::Info
    }
}

/// An insight generated from analyzing metrics and logs.
///
/// Insights represent actionable observations about the system's state,
/// including evidence supporting the observation and recommendations for resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Insight {
    /// Unique identifier for this insight.
    pub id: Uuid,
    /// Severity level of this insight.
    pub severity: Severity,
    /// Short, descriptive title.
    pub title: String,
    /// Detailed description of the observation.
    pub description: String,
    /// Evidence supporting this insight (metrics values, log excerpts, etc.).
    pub evidence: Vec<String>,
    /// Recommended actions to resolve or address this insight.
    pub recommendation: Option<String>,
    /// When this insight was generated.
    pub generated_at: DateTime<Utc>,
    /// Optional tags for categorization.
    pub tags: Vec<String>,
}

impl Insight {
    /// Creates a new insight with the given parameters.
    #[must_use]
    pub fn new(severity: Severity, title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            severity,
            title: title.into(),
            description: description.into(),
            evidence: Vec::new(),
            recommendation: None,
            generated_at: Utc::now(),
            tags: Vec::new(),
        }
    }

    /// Creates a new insight with a specific ID (useful for testing).
    #[must_use]
    pub fn with_id(
        id: Uuid,
        severity: Severity,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id,
            severity,
            title: title.into(),
            description: description.into(),
            evidence: Vec::new(),
            recommendation: None,
            generated_at: Utc::now(),
            tags: Vec::new(),
        }
    }

    /// Adds evidence to this insight.
    #[must_use]
    pub fn with_evidence(mut self, evidence: impl Into<String>) -> Self {
        self.evidence.push(evidence.into());
        self
    }

    /// Adds multiple pieces of evidence to this insight.
    #[must_use]
    pub fn with_evidence_list(mut self, evidence: Vec<String>) -> Self {
        self.evidence.extend(evidence);
        self
    }

    /// Sets the recommendation for this insight.
    #[must_use]
    pub fn with_recommendation(mut self, recommendation: impl Into<String>) -> Self {
        self.recommendation = Some(recommendation.into());
        self
    }

    /// Adds a tag to this insight.
    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Sets the generation timestamp (useful for testing).
    #[must_use]
    pub fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.generated_at = timestamp;
        self
    }

    /// Returns true if this insight requires immediate attention.
    #[must_use]
    pub const fn is_critical(&self) -> bool {
        matches!(self.severity, Severity::Critical)
    }

    /// Returns true if this insight has actionable recommendations.
    #[must_use]
    pub fn has_recommendation(&self) -> bool {
        self.recommendation.is_some()
    }
}

/// A complete diagnosis containing health status and insights.
///
/// Represents the result of analyzing a node, workload, or cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnosis {
    /// Unique identifier for this diagnosis.
    pub id: Uuid,
    /// Overall health status based on analysis.
    pub status: HealthStatus,
    /// List of insights discovered during analysis.
    pub insights: Vec<Insight>,
    /// When the analysis was performed.
    pub analyzed_at: DateTime<Utc>,
    /// Optional subject identifier (node_id, workload_id, or "cluster").
    pub subject: Option<String>,
    /// Duration of the analysis in milliseconds.
    pub analysis_duration_ms: u64,
}

impl Diagnosis {
    /// Creates a new diagnosis with the given status.
    #[must_use]
    pub fn new(status: HealthStatus) -> Self {
        Self {
            id: Uuid::new_v4(),
            status,
            insights: Vec::new(),
            analyzed_at: Utc::now(),
            subject: None,
            analysis_duration_ms: 0,
        }
    }

    /// Creates a new diagnosis with a specific ID (useful for testing).
    #[must_use]
    pub fn with_id(id: Uuid, status: HealthStatus) -> Self {
        Self {
            id,
            status,
            insights: Vec::new(),
            analyzed_at: Utc::now(),
            subject: None,
            analysis_duration_ms: 0,
        }
    }

    /// Adds an insight to this diagnosis.
    #[must_use]
    pub fn with_insight(mut self, insight: Insight) -> Self {
        self.insights.push(insight);
        self
    }

    /// Adds multiple insights to this diagnosis.
    #[must_use]
    pub fn with_insights(mut self, insights: Vec<Insight>) -> Self {
        self.insights.extend(insights);
        self
    }

    /// Sets the subject of this diagnosis.
    #[must_use]
    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// Sets the analysis timestamp (useful for testing).
    #[must_use]
    pub fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.analyzed_at = timestamp;
        self
    }

    /// Sets the analysis duration.
    #[must_use]
    pub const fn with_duration(mut self, duration_ms: u64) -> Self {
        self.analysis_duration_ms = duration_ms;
        self
    }

    /// Returns the count of insights by severity.
    #[must_use]
    pub fn count_by_severity(&self, severity: Severity) -> usize {
        self.insights.iter().filter(|i| i.severity == severity).count()
    }

    /// Returns all critical insights.
    #[must_use]
    pub fn critical_insights(&self) -> Vec<&Insight> {
        self.insights.iter().filter(|i| i.is_critical()).collect()
    }

    /// Returns true if there are any critical insights.
    #[must_use]
    pub fn has_critical_insights(&self) -> bool {
        self.insights.iter().any(Insight::is_critical)
    }

    /// Returns the most severe status based on insights.
    #[must_use]
    pub fn computed_status(&self) -> HealthStatus {
        if self.insights.is_empty() {
            return HealthStatus::Unknown;
        }

        let max_severity = self
            .insights
            .iter()
            .map(|i| i.severity.as_level())
            .max()
            .unwrap_or(0);

        match max_severity {
            0 => HealthStatus::Healthy,
            1 => HealthStatus::Degraded,
            2 | 3 => HealthStatus::Critical,
            _ => HealthStatus::Unknown,
        }
    }
}

impl Default for Diagnosis {
    fn default() -> Self {
        Self::new(HealthStatus::Unknown)
    }
}

/// Defines the scope of an analysis operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AnalysisScope {
    /// Specific workload IDs to analyze (empty means all).
    pub workload_ids: Vec<String>,
    /// Specific node IDs to analyze (empty means all).
    pub node_ids: Vec<String>,
    /// Time range for the analysis.
    pub time_range: Option<TimeRange>,
    /// Whether to include historical data in the analysis.
    pub include_historical: bool,
    /// Maximum number of insights to generate.
    pub max_insights: Option<usize>,
}

impl AnalysisScope {
    /// Creates a new empty scope (analyzes everything).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a scope for a specific node.
    #[must_use]
    pub fn for_node(node_id: impl Into<String>) -> Self {
        Self {
            node_ids: vec![node_id.into()],
            ..Self::default()
        }
    }

    /// Creates a scope for a specific workload.
    #[must_use]
    pub fn for_workload(workload_id: impl Into<String>) -> Self {
        Self {
            workload_ids: vec![workload_id.into()],
            ..Self::default()
        }
    }

    /// Creates a scope for the entire cluster.
    #[must_use]
    pub fn for_cluster() -> Self {
        Self::default()
    }

    /// Adds a workload ID to the scope.
    #[must_use]
    pub fn with_workload(mut self, workload_id: impl Into<String>) -> Self {
        self.workload_ids.push(workload_id.into());
        self
    }

    /// Adds a node ID to the scope.
    #[must_use]
    pub fn with_node(mut self, node_id: impl Into<String>) -> Self {
        self.node_ids.push(node_id.into());
        self
    }

    /// Sets the time range for the analysis.
    #[must_use]
    pub fn with_time_range(mut self, time_range: TimeRange) -> Self {
        self.time_range = Some(time_range);
        self
    }

    /// Enables historical data inclusion.
    #[must_use]
    pub const fn with_historical(mut self) -> Self {
        self.include_historical = true;
        self
    }

    /// Sets the maximum number of insights.
    #[must_use]
    pub const fn with_max_insights(mut self, max: usize) -> Self {
        self.max_insights = Some(max);
        self
    }

    /// Returns true if this scope targets specific nodes.
    #[must_use]
    pub fn has_node_filter(&self) -> bool {
        !self.node_ids.is_empty()
    }

    /// Returns true if this scope targets specific workloads.
    #[must_use]
    pub fn has_workload_filter(&self) -> bool {
        !self.workload_ids.is_empty()
    }

    /// Returns true if this is a cluster-wide scope.
    #[must_use]
    pub fn is_cluster_scope(&self) -> bool {
        self.node_ids.is_empty() && self.workload_ids.is_empty()
    }
}

/// Represents a time range for analysis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    /// Start of the time range (inclusive).
    pub start: DateTime<Utc>,
    /// End of the time range (exclusive).
    pub end: DateTime<Utc>,
}

impl TimeRange {
    /// Creates a new time range.
    ///
    /// # Panics
    /// Panics if start is after end in debug builds.
    #[must_use]
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        debug_assert!(start <= end, "start must be before or equal to end");
        Self { start, end }
    }

    /// Creates a time range for the last N minutes.
    #[must_use]
    pub fn last_minutes(minutes: i64) -> Self {
        let end = Utc::now();
        let start = end - chrono::Duration::minutes(minutes);
        Self { start, end }
    }

    /// Creates a time range for the last N hours.
    #[must_use]
    pub fn last_hours(hours: i64) -> Self {
        let end = Utc::now();
        let start = end - chrono::Duration::hours(hours);
        Self { start, end }
    }

    /// Creates a time range for the last N days.
    #[must_use]
    pub fn last_days(days: i64) -> Self {
        let end = Utc::now();
        let start = end - chrono::Duration::days(days);
        Self { start, end }
    }

    /// Returns the duration of this time range.
    #[must_use]
    pub fn duration(&self) -> chrono::Duration {
        self.end - self.start
    }

    /// Returns true if the given timestamp is within this range.
    #[must_use]
    pub fn contains(&self, timestamp: &DateTime<Utc>) -> bool {
        *timestamp >= self.start && *timestamp < self.end
    }

    /// Converts to a standard Range for iteration patterns.
    #[must_use]
    pub fn as_range(&self) -> Range<DateTime<Utc>> {
        self.start..self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod health_status_tests {
        use super::*;

        #[test]
        fn test_health_status_default_is_unknown() {
            let status = HealthStatus::default();
            assert_eq!(status, HealthStatus::Unknown);
        }

        #[test]
        fn test_health_status_is_operational() {
            assert!(HealthStatus::Healthy.is_operational());
            assert!(HealthStatus::Degraded.is_operational());
            assert!(!HealthStatus::Critical.is_operational());
            assert!(!HealthStatus::Unknown.is_operational());
        }

        #[test]
        fn test_health_status_requires_attention() {
            assert!(!HealthStatus::Healthy.requires_attention());
            assert!(HealthStatus::Degraded.requires_attention());
            assert!(HealthStatus::Critical.requires_attention());
            assert!(!HealthStatus::Unknown.requires_attention());
        }

        #[test]
        fn test_health_status_severity_level() {
            assert_eq!(HealthStatus::Healthy.severity_level(), 0);
            assert_eq!(HealthStatus::Degraded.severity_level(), 1);
            assert_eq!(HealthStatus::Critical.severity_level(), 2);
            assert_eq!(HealthStatus::Unknown.severity_level(), 3);
        }

        #[test]
        fn test_health_status_display() {
            assert_eq!(format!("{}", HealthStatus::Healthy), "Healthy");
            assert_eq!(format!("{}", HealthStatus::Degraded), "Degraded");
            assert_eq!(format!("{}", HealthStatus::Critical), "Critical");
            assert_eq!(format!("{}", HealthStatus::Unknown), "Unknown");
        }

        #[test]
        fn test_health_status_description() {
            assert_eq!(HealthStatus::Healthy.description(), "System is operating normally");
            assert_eq!(HealthStatus::Degraded.description(), "System is experiencing minor issues");
            assert_eq!(HealthStatus::Critical.description(), "System requires immediate attention");
            assert_eq!(HealthStatus::Unknown.description(), "Unable to determine system status");
        }

        #[test]
        fn test_health_status_serialization() {
            let status = HealthStatus::Critical;
            let json = serde_json::to_string(&status).expect("serialization should work in test");
            assert_eq!(json, "\"critical\"");

            let deserialized: HealthStatus =
                serde_json::from_str(&json).expect("deserialization should work in test");
            assert_eq!(deserialized, status);
        }
    }

    mod severity_tests {
        use super::*;

        #[test]
        fn test_severity_ordering() {
            assert!(Severity::Info < Severity::Warning);
            assert!(Severity::Warning < Severity::Error);
            assert!(Severity::Error < Severity::Critical);
        }

        #[test]
        fn test_severity_as_level() {
            assert_eq!(Severity::Info.as_level(), 0);
            assert_eq!(Severity::Warning.as_level(), 1);
            assert_eq!(Severity::Error.as_level(), 2);
            assert_eq!(Severity::Critical.as_level(), 3);
        }

        #[test]
        fn test_severity_emoji() {
            assert_eq!(Severity::Info.emoji(), "ℹ️");
            assert_eq!(Severity::Warning.emoji(), "⚠️");
            assert_eq!(Severity::Error.emoji(), "❌");
            assert_eq!(Severity::Critical.emoji(), "🚨");
        }

        #[test]
        fn test_severity_display() {
            assert_eq!(format!("{}", Severity::Info), "INFO");
            assert_eq!(format!("{}", Severity::Warning), "WARNING");
            assert_eq!(format!("{}", Severity::Error), "ERROR");
            assert_eq!(format!("{}", Severity::Critical), "CRITICAL");
        }

        #[test]
        fn test_severity_default() {
            assert_eq!(Severity::default(), Severity::Info);
        }
    }

    mod insight_tests {
        use super::*;

        #[test]
        fn test_insight_creation() {
            let insight = Insight::new(
                Severity::Warning,
                "High CPU Usage",
                "CPU utilization exceeded 90%",
            );

            assert_eq!(insight.severity, Severity::Warning);
            assert_eq!(insight.title, "High CPU Usage");
            assert_eq!(insight.description, "CPU utilization exceeded 90%");
            assert!(insight.evidence.is_empty());
            assert!(insight.recommendation.is_none());
            assert!(insight.tags.is_empty());
        }

        #[test]
        fn test_insight_with_evidence() {
            let insight = Insight::new(Severity::Error, "Memory Leak", "Detected memory growth")
                .with_evidence("Memory increased from 1GB to 4GB over 2 hours")
                .with_evidence("No garbage collection observed");

            assert_eq!(insight.evidence.len(), 2);
            assert!(insight.evidence[0].contains("1GB to 4GB"));
        }

        #[test]
        fn test_insight_with_recommendation() {
            let insight = Insight::new(Severity::Critical, "Disk Full", "Disk space exhausted")
                .with_recommendation("Delete old logs or expand storage");

            assert!(insight.has_recommendation());
            assert_eq!(
                insight.recommendation,
                Some("Delete old logs or expand storage".to_string())
            );
        }

        #[test]
        fn test_insight_with_tags() {
            let insight = Insight::new(Severity::Info, "Test", "Description")
                .with_tag("gpu")
                .with_tag("thermal");

            assert_eq!(insight.tags.len(), 2);
            assert!(insight.tags.contains(&"gpu".to_string()));
            assert!(insight.tags.contains(&"thermal".to_string()));
        }

        #[test]
        fn test_insight_is_critical() {
            let critical = Insight::new(Severity::Critical, "Critical Issue", "Details");
            let warning = Insight::new(Severity::Warning, "Warning Issue", "Details");

            assert!(critical.is_critical());
            assert!(!warning.is_critical());
        }

        #[test]
        fn test_insight_serialization() {
            let id = Uuid::nil();
            let insight = Insight::with_id(id, Severity::Warning, "Test", "Description")
                .with_evidence("Evidence 1")
                .with_recommendation("Do something");

            let json = serde_json::to_string(&insight).expect("serialization should work in test");
            assert!(json.contains("\"severity\":\"warning\""));
            assert!(json.contains("\"title\":\"Test\""));

            let deserialized: Insight =
                serde_json::from_str(&json).expect("deserialization should work in test");
            assert_eq!(deserialized.id, id);
            assert_eq!(deserialized.severity, Severity::Warning);
        }
    }

    mod diagnosis_tests {
        use super::*;

        #[test]
        fn test_diagnosis_creation() {
            let diagnosis = Diagnosis::new(HealthStatus::Healthy);

            assert_eq!(diagnosis.status, HealthStatus::Healthy);
            assert!(diagnosis.insights.is_empty());
            assert!(diagnosis.subject.is_none());
            assert_eq!(diagnosis.analysis_duration_ms, 0);
        }

        #[test]
        fn test_diagnosis_with_insights() {
            let insight1 = Insight::new(Severity::Warning, "Warning 1", "Details");
            let insight2 = Insight::new(Severity::Error, "Error 1", "Details");

            let diagnosis = Diagnosis::new(HealthStatus::Degraded)
                .with_insight(insight1)
                .with_insight(insight2);

            assert_eq!(diagnosis.insights.len(), 2);
        }

        #[test]
        fn test_diagnosis_with_subject() {
            let diagnosis = Diagnosis::new(HealthStatus::Healthy).with_subject("node-001");

            assert_eq!(diagnosis.subject, Some("node-001".to_string()));
        }

        #[test]
        fn test_diagnosis_count_by_severity() {
            let diagnosis = Diagnosis::new(HealthStatus::Degraded)
                .with_insight(Insight::new(Severity::Warning, "W1", "D"))
                .with_insight(Insight::new(Severity::Warning, "W2", "D"))
                .with_insight(Insight::new(Severity::Error, "E1", "D"));

            assert_eq!(diagnosis.count_by_severity(Severity::Warning), 2);
            assert_eq!(diagnosis.count_by_severity(Severity::Error), 1);
            assert_eq!(diagnosis.count_by_severity(Severity::Critical), 0);
        }

        #[test]
        fn test_diagnosis_critical_insights() {
            let diagnosis = Diagnosis::new(HealthStatus::Critical)
                .with_insight(Insight::new(Severity::Warning, "W1", "D"))
                .with_insight(Insight::new(Severity::Critical, "C1", "D"))
                .with_insight(Insight::new(Severity::Critical, "C2", "D"));

            let critical = diagnosis.critical_insights();
            assert_eq!(critical.len(), 2);
            assert!(diagnosis.has_critical_insights());
        }

        #[test]
        fn test_diagnosis_computed_status() {
            // Empty insights -> Unknown
            let empty = Diagnosis::new(HealthStatus::Healthy);
            assert_eq!(empty.computed_status(), HealthStatus::Unknown);

            // Only info -> Healthy
            let info_only = Diagnosis::new(HealthStatus::Unknown)
                .with_insight(Insight::new(Severity::Info, "I1", "D"));
            assert_eq!(info_only.computed_status(), HealthStatus::Healthy);

            // Warning present -> Degraded
            let with_warning = Diagnosis::new(HealthStatus::Unknown)
                .with_insight(Insight::new(Severity::Info, "I1", "D"))
                .with_insight(Insight::new(Severity::Warning, "W1", "D"));
            assert_eq!(with_warning.computed_status(), HealthStatus::Degraded);

            // Error/Critical present -> Critical
            let with_error = Diagnosis::new(HealthStatus::Unknown)
                .with_insight(Insight::new(Severity::Error, "E1", "D"));
            assert_eq!(with_error.computed_status(), HealthStatus::Critical);
        }

        #[test]
        fn test_diagnosis_default() {
            let diagnosis = Diagnosis::default();
            assert_eq!(diagnosis.status, HealthStatus::Unknown);
            assert!(diagnosis.insights.is_empty());
        }

        #[test]
        fn test_diagnosis_serialization() {
            let id = Uuid::nil();
            let diagnosis = Diagnosis::with_id(id, HealthStatus::Healthy)
                .with_subject("test-node")
                .with_duration(100);

            let json = serde_json::to_string(&diagnosis).expect("serialization should work in test");
            assert!(json.contains("\"status\":\"healthy\""));
            assert!(json.contains("\"subject\":\"test-node\""));

            let deserialized: Diagnosis =
                serde_json::from_str(&json).expect("deserialization should work in test");
            assert_eq!(deserialized.id, id);
            assert_eq!(deserialized.status, HealthStatus::Healthy);
        }
    }

    mod analysis_scope_tests {
        use super::*;

        #[test]
        fn test_scope_default_is_cluster_wide() {
            let scope = AnalysisScope::new();
            assert!(scope.is_cluster_scope());
            assert!(!scope.has_node_filter());
            assert!(!scope.has_workload_filter());
        }

        #[test]
        fn test_scope_for_node() {
            let scope = AnalysisScope::for_node("node-001");
            assert!(scope.has_node_filter());
            assert!(!scope.is_cluster_scope());
            assert_eq!(scope.node_ids, vec!["node-001"]);
        }

        #[test]
        fn test_scope_for_workload() {
            let scope = AnalysisScope::for_workload("workload-abc");
            assert!(scope.has_workload_filter());
            assert!(!scope.is_cluster_scope());
            assert_eq!(scope.workload_ids, vec!["workload-abc"]);
        }

        #[test]
        fn test_scope_builder_pattern() {
            let scope = AnalysisScope::new()
                .with_node("node-001")
                .with_node("node-002")
                .with_workload("workload-abc")
                .with_historical()
                .with_max_insights(10);

            assert_eq!(scope.node_ids.len(), 2);
            assert_eq!(scope.workload_ids.len(), 1);
            assert!(scope.include_historical);
            assert_eq!(scope.max_insights, Some(10));
        }

        #[test]
        fn test_scope_with_time_range() {
            let start = Utc::now() - chrono::Duration::hours(1);
            let end = Utc::now();
            let time_range = TimeRange::new(start, end);

            let scope = AnalysisScope::new().with_time_range(time_range.clone());

            assert!(scope.time_range.is_some());
            let range = scope.time_range.as_ref().expect("time_range should be set");
            assert_eq!(range.start, start);
            assert_eq!(range.end, end);
        }
    }

    mod time_range_tests {
        use super::*;

        #[test]
        fn test_time_range_creation() {
            let start = Utc::now() - chrono::Duration::hours(1);
            let end = Utc::now();
            let range = TimeRange::new(start, end);

            assert_eq!(range.start, start);
            assert_eq!(range.end, end);
        }

        #[test]
        fn test_time_range_last_minutes() {
            let range = TimeRange::last_minutes(30);
            let duration = range.duration();

            // Allow 1 second tolerance for test execution time
            assert!(duration.num_minutes() >= 29 && duration.num_minutes() <= 30);
        }

        #[test]
        fn test_time_range_last_hours() {
            let range = TimeRange::last_hours(2);
            let duration = range.duration();

            assert!(duration.num_hours() >= 1 && duration.num_hours() <= 2);
        }

        #[test]
        fn test_time_range_last_days() {
            let range = TimeRange::last_days(7);
            let duration = range.duration();

            assert!(duration.num_days() >= 6 && duration.num_days() <= 7);
        }

        #[test]
        fn test_time_range_contains() {
            let start = Utc::now() - chrono::Duration::hours(2);
            let end = Utc::now();
            let range = TimeRange::new(start, end);

            let middle = start + chrono::Duration::hours(1);
            let before = start - chrono::Duration::hours(1);
            let after = end + chrono::Duration::hours(1);

            assert!(range.contains(&middle));
            assert!(range.contains(&start)); // inclusive start
            assert!(!range.contains(&end)); // exclusive end
            assert!(!range.contains(&before));
            assert!(!range.contains(&after));
        }

        #[test]
        fn test_time_range_as_range() {
            let start = Utc::now() - chrono::Duration::hours(1);
            let end = Utc::now();
            let time_range = TimeRange::new(start, end);

            let range = time_range.as_range();
            assert_eq!(range.start, start);
            assert_eq!(range.end, end);
        }
    }
}
