//! Human-readable output formatting for diagnoses.
//!
//! This module provides formatters to convert diagnoses into various output formats
//! suitable for humans, AI agents, and summary displays.

// Allow format string pushing for readability in formatter functions
#![allow(clippy::format_push_string)]
#![allow(clippy::uninlined_format_args)]

use crate::types::{Diagnosis, HealthStatus, Insight, Severity};

/// Formats a diagnosis into a detailed human-readable string.
///
/// This format includes the full details of the diagnosis, all insights,
/// evidence, and recommendations.
#[must_use]
pub fn format_diagnosis(diagnosis: &Diagnosis) -> String {
    let mut output = String::new();

    // Header
    output.push_str(&format_header(diagnosis));
    output.push('\n');

    // Status section
    output.push_str(&format_status_section(diagnosis));
    output.push('\n');

    // Insights section
    if diagnosis.insights.is_empty() {
        output.push_str("No issues detected.\n");
    } else {
        output.push_str(&format_insights_section(&diagnosis.insights));
    }

    // Footer
    output.push_str(&format_footer(diagnosis));

    output
}

fn format_header(diagnosis: &Diagnosis) -> String {
    let mut header = String::new();
    header.push_str("═══════════════════════════════════════════════════════════════\n");
    header.push_str("                    CLAWBERNETES DIAGNOSIS\n");
    header.push_str("═══════════════════════════════════════════════════════════════\n");

    if let Some(ref subject) = diagnosis.subject {
        header.push_str(&format!("Subject: {subject}\n"));
    }

    header.push_str(&format!(
        "Analyzed: {}\n",
        diagnosis.analyzed_at.format("%Y-%m-%d %H:%M:%S UTC")
    ));

    if diagnosis.analysis_duration_ms > 0 {
        header.push_str(&format!("Duration: {}ms\n", diagnosis.analysis_duration_ms));
    }

    header
}

fn format_status_section(diagnosis: &Diagnosis) -> String {
    let status_emoji = match diagnosis.status {
        HealthStatus::Healthy => "✅",
        HealthStatus::Degraded => "⚠️",
        HealthStatus::Critical => "🚨",
        HealthStatus::Unknown => "❓",
    };

    let mut section = String::new();
    section.push_str("───────────────────────────────────────────────────────────────\n");
    section.push_str(&format!(
        "Status: {} {}\n",
        status_emoji, diagnosis.status
    ));
    section.push_str(&format!("        {}\n", diagnosis.status.description()));
    section.push_str("───────────────────────────────────────────────────────────────\n");

    // Summary counts
    let critical_count = diagnosis.count_by_severity(Severity::Critical);
    let error_count = diagnosis.count_by_severity(Severity::Error);
    let warning_count = diagnosis.count_by_severity(Severity::Warning);
    let info_count = diagnosis.count_by_severity(Severity::Info);

    if !diagnosis.insights.is_empty() {
        section.push_str(&format!(
            "Summary: {} Critical, {} Error, {} Warning, {} Info\n",
            critical_count, error_count, warning_count, info_count
        ));
    }

    section
}

fn format_insights_section(insights: &[Insight]) -> String {
    let mut section = String::new();
    section.push_str("\n📋 INSIGHTS\n");
    section.push_str("───────────────────────────────────────────────────────────────\n");

    // Sort by severity (most severe first)
    let mut sorted_insights: Vec<_> = insights.iter().collect();
    sorted_insights.sort_by(|a, b| b.severity.as_level().cmp(&a.severity.as_level()));

    for (i, insight) in sorted_insights.iter().enumerate() {
        section.push_str(&format_single_insight(i + 1, insight));
        section.push('\n');
    }

    section
}

fn format_single_insight(index: usize, insight: &Insight) -> String {
    let mut output = String::new();

    output.push_str(&format!(
        "\n{} [{index}] {}: {}\n",
        insight.severity.emoji(),
        insight.severity,
        insight.title
    ));

    output.push_str(&format!("   {}\n", insight.description));

    if !insight.evidence.is_empty() {
        output.push_str("   Evidence:\n");
        for evidence in &insight.evidence {
            output.push_str(&format!("   • {evidence}\n"));
        }
    }

    if let Some(ref recommendation) = insight.recommendation {
        output.push_str(&format!("   💡 Recommendation: {recommendation}\n"));
    }

    if !insight.tags.is_empty() {
        output.push_str(&format!("   Tags: {}\n", insight.tags.join(", ")));
    }

    output
}

fn format_footer(diagnosis: &Diagnosis) -> String {
    let mut footer = String::new();
    footer.push_str("\n═══════════════════════════════════════════════════════════════\n");
    footer.push_str(&format!("Diagnosis ID: {}\n", diagnosis.id));
    footer.push_str("═══════════════════════════════════════════════════════════════\n");
    footer
}

/// Formats a diagnosis in a structured format suitable for AI agents.
///
/// This format uses a clear structure that's easy for AI systems to parse
/// and reason about.
#[must_use]
pub fn format_for_agent(diagnosis: &Diagnosis) -> String {
    let mut output = String::new();

    // Structured header
    output.push_str("## Diagnosis Report\n\n");

    // Metadata
    output.push_str("### Metadata\n");
    output.push_str(&format!("- **ID**: {}\n", diagnosis.id));
    output.push_str(&format!("- **Status**: {}\n", diagnosis.status));
    output.push_str(&format!(
        "- **Analyzed At**: {}\n",
        diagnosis.analyzed_at.to_rfc3339()
    ));

    if let Some(ref subject) = diagnosis.subject {
        output.push_str(&format!("- **Subject**: {subject}\n"));
    }

    output.push_str(&format!(
        "- **Analysis Duration**: {}ms\n",
        diagnosis.analysis_duration_ms
    ));
    output.push('\n');

    // Status interpretation
    output.push_str("### Status Interpretation\n");
    output.push_str(&format!(
        "The system is in **{}** state. {}\n\n",
        diagnosis.status,
        diagnosis.status.description()
    ));

    // Insights
    output.push_str("### Insights\n\n");

    if diagnosis.insights.is_empty() {
        output.push_str("No issues detected. System appears healthy.\n\n");
    } else {
        for (i, insight) in diagnosis.insights.iter().enumerate() {
            output.push_str(&format_insight_for_agent(i + 1, insight));
        }
    }

    // Action items
    output.push_str("### Recommended Actions\n\n");

    let actionable: Vec<_> = diagnosis
        .insights
        .iter()
        .filter(|i| i.recommendation.is_some())
        .collect();

    if actionable.is_empty() {
        output.push_str("No immediate actions required.\n");
    } else {
        for (i, insight) in actionable.iter().enumerate() {
            if let Some(ref rec) = insight.recommendation {
                output.push_str(&format!(
                    "{}. **{}**: {}\n",
                    i + 1,
                    insight.title,
                    rec
                ));
            }
        }
    }

    output
}

fn format_insight_for_agent(index: usize, insight: &Insight) -> String {
    let mut output = String::new();

    output.push_str(&format!("#### {index}. {} ({})\n\n", insight.title, insight.severity));
    output.push_str(&format!("{}\n\n", insight.description));

    if !insight.evidence.is_empty() {
        output.push_str("**Evidence:**\n");
        for evidence in &insight.evidence {
            output.push_str(&format!("- {evidence}\n"));
        }
        output.push('\n');
    }

    if let Some(ref recommendation) = insight.recommendation {
        output.push_str(&format!("**Recommendation:** {recommendation}\n\n"));
    }

    output
}

/// Formats a diagnosis as a one-line summary.
///
/// Useful for logging, notifications, or quick status checks.
#[must_use]
pub fn format_summary(diagnosis: &Diagnosis) -> String {
    let status_indicator = match diagnosis.status {
        HealthStatus::Healthy => "[OK]",
        HealthStatus::Degraded => "[WARN]",
        HealthStatus::Critical => "[CRIT]",
        HealthStatus::Unknown => "[???]",
    };

    let subject = diagnosis
        .subject
        .as_deref()
        .unwrap_or("system");

    let critical_count = diagnosis.count_by_severity(Severity::Critical);
    let error_count = diagnosis.count_by_severity(Severity::Error);
    let warning_count = diagnosis.count_by_severity(Severity::Warning);

    let issues = if diagnosis.insights.is_empty() {
        "no issues".to_string()
    } else {
        let mut parts = Vec::new();
        if critical_count > 0 {
            parts.push(format!("{critical_count} critical"));
        }
        if error_count > 0 {
            parts.push(format!("{error_count} error"));
        }
        if warning_count > 0 {
            parts.push(format!("{warning_count} warning"));
        }
        if parts.is_empty() {
            format!("{} info", diagnosis.count_by_severity(Severity::Info))
        } else {
            parts.join(", ")
        }
    };

    format!("{status_indicator} {subject}: {issues}")
}

/// Formats a list of insights as a bullet list.
#[must_use]
pub fn format_insight_list(insights: &[Insight]) -> String {
    let mut output = String::new();

    for insight in insights {
        output.push_str(&format!(
            "• {} {}: {}\n",
            insight.severity.emoji(),
            insight.title,
            truncate_string(&insight.description, 60)
        ));
    }

    output
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Formats a diagnosis as JSON for programmatic consumption.
///
/// # Errors
///
/// Returns an error if JSON serialization fails.
pub fn format_as_json(diagnosis: &Diagnosis) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(diagnosis)
}

/// Formats a diagnosis as compact JSON (single line).
///
/// # Errors
///
/// Returns an error if JSON serialization fails.
pub fn format_as_json_compact(diagnosis: &Diagnosis) -> Result<String, serde_json::Error> {
    serde_json::to_string(diagnosis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn create_test_diagnosis() -> Diagnosis {
        Diagnosis::with_id(Uuid::nil(), HealthStatus::Degraded)
            .with_subject("node-001")
            .with_duration(150)
            .with_insight(
                Insight::new(
                    Severity::Warning,
                    "High CPU Usage",
                    "CPU utilization has exceeded 90%",
                )
                .with_evidence("Current usage: 92%")
                .with_evidence("Average over 5min: 88%")
                .with_recommendation("Consider scaling up or reducing workload")
                .with_tag("cpu")
                .with_tag("resource"),
            )
            .with_insight(
                Insight::new(
                    Severity::Error,
                    "Memory Pressure",
                    "Memory usage is critically high",
                )
                .with_evidence("Used: 95%")
                .with_recommendation("Free up memory or scale up"),
            )
    }

    mod format_diagnosis_tests {
        use super::*;

        #[test]
        fn test_format_empty_diagnosis() {
            let diagnosis = Diagnosis::new(HealthStatus::Healthy);
            let output = format_diagnosis(&diagnosis);

            assert!(output.contains("CLAWBERNETES DIAGNOSIS"));
            assert!(output.contains("Healthy"));
            assert!(output.contains("No issues detected"));
        }

        #[test]
        fn test_format_diagnosis_with_insights() {
            let diagnosis = create_test_diagnosis();
            let output = format_diagnosis(&diagnosis);

            assert!(output.contains("node-001"));
            assert!(output.contains("Degraded"));
            assert!(output.contains("High CPU Usage"));
            assert!(output.contains("Memory Pressure"));
            assert!(output.contains("Current usage: 92%"));
            assert!(output.contains("Consider scaling up"));
        }

        #[test]
        fn test_format_shows_severity_counts() {
            let diagnosis = create_test_diagnosis();
            let output = format_diagnosis(&diagnosis);

            // Should show summary with counts
            assert!(output.contains("1 Error"));
            assert!(output.contains("1 Warning"));
        }

        #[test]
        fn test_format_shows_status_emoji() {
            let healthy = Diagnosis::new(HealthStatus::Healthy);
            assert!(format_diagnosis(&healthy).contains("✅"));

            let degraded = Diagnosis::new(HealthStatus::Degraded);
            assert!(format_diagnosis(&degraded).contains("⚠️"));

            let critical = Diagnosis::new(HealthStatus::Critical);
            assert!(format_diagnosis(&critical).contains("🚨"));
        }

        #[test]
        fn test_format_shows_duration() {
            let diagnosis = Diagnosis::new(HealthStatus::Healthy).with_duration(250);
            let output = format_diagnosis(&diagnosis);

            assert!(output.contains("250ms"));
        }

        #[test]
        fn test_format_shows_tags() {
            let diagnosis = create_test_diagnosis();
            let output = format_diagnosis(&diagnosis);

            assert!(output.contains("cpu"));
            assert!(output.contains("resource"));
        }
    }

    mod format_for_agent_tests {
        use super::*;

        #[test]
        fn test_agent_format_structure() {
            let diagnosis = create_test_diagnosis();
            let output = format_for_agent(&diagnosis);

            // Should have markdown structure
            assert!(output.contains("## Diagnosis Report"));
            assert!(output.contains("### Metadata"));
            assert!(output.contains("### Status Interpretation"));
            assert!(output.contains("### Insights"));
            assert!(output.contains("### Recommended Actions"));
        }

        #[test]
        fn test_agent_format_includes_metadata() {
            let diagnosis = create_test_diagnosis();
            let output = format_for_agent(&diagnosis);

            assert!(output.contains("**ID**"));
            assert!(output.contains("**Status**: Degraded"));
            assert!(output.contains("**Subject**: node-001"));
        }

        #[test]
        fn test_agent_format_includes_insights() {
            let diagnosis = create_test_diagnosis();
            let output = format_for_agent(&diagnosis);

            assert!(output.contains("High CPU Usage"));
            assert!(output.contains("Memory Pressure"));
            assert!(output.contains("**Evidence:**"));
        }

        #[test]
        fn test_agent_format_includes_actions() {
            let diagnosis = create_test_diagnosis();
            let output = format_for_agent(&diagnosis);

            assert!(output.contains("Consider scaling up"));
            assert!(output.contains("Free up memory"));
        }

        #[test]
        fn test_agent_format_empty_diagnosis() {
            let diagnosis = Diagnosis::new(HealthStatus::Healthy);
            let output = format_for_agent(&diagnosis);

            assert!(output.contains("No issues detected"));
            assert!(output.contains("No immediate actions required"));
        }
    }

    mod format_summary_tests {
        use super::*;

        #[test]
        fn test_summary_healthy() {
            let diagnosis = Diagnosis::new(HealthStatus::Healthy);
            let summary = format_summary(&diagnosis);

            assert!(summary.contains("[OK]"));
            assert!(summary.contains("no issues"));
        }

        #[test]
        fn test_summary_with_issues() {
            let diagnosis = create_test_diagnosis();
            let summary = format_summary(&diagnosis);

            assert!(summary.contains("[WARN]"));
            assert!(summary.contains("node-001"));
            assert!(summary.contains("1 error"));
            assert!(summary.contains("1 warning"));
        }

        #[test]
        fn test_summary_critical() {
            let diagnosis = Diagnosis::new(HealthStatus::Critical)
                .with_subject("cluster")
                .with_insight(Insight::new(
                    Severity::Critical,
                    "System Down",
                    "Total failure",
                ));
            let summary = format_summary(&diagnosis);

            assert!(summary.contains("[CRIT]"));
            assert!(summary.contains("cluster"));
            assert!(summary.contains("1 critical"));
        }

        #[test]
        fn test_summary_unknown_status() {
            let diagnosis = Diagnosis::new(HealthStatus::Unknown);
            let summary = format_summary(&diagnosis);

            assert!(summary.contains("[???]"));
        }

        #[test]
        fn test_summary_info_only() {
            let diagnosis = Diagnosis::new(HealthStatus::Healthy)
                .with_insight(Insight::new(
                    Severity::Info,
                    "FYI",
                    "Just information",
                ));
            let summary = format_summary(&diagnosis);

            assert!(summary.contains("1 info"));
        }
    }

    mod format_insight_list_tests {
        use super::*;

        #[test]
        fn test_empty_list() {
            let insights: Vec<Insight> = vec![];
            let output = format_insight_list(&insights);

            assert!(output.is_empty());
        }

        #[test]
        fn test_list_with_insights() {
            let insights = vec![
                Insight::new(Severity::Warning, "Warning 1", "Description 1"),
                Insight::new(Severity::Error, "Error 1", "Description 2"),
            ];
            let output = format_insight_list(&insights);

            assert!(output.contains("• ⚠️ Warning 1"));
            assert!(output.contains("• ❌ Error 1"));
        }

        #[test]
        fn test_truncates_long_descriptions() {
            let long_desc = "A".repeat(100);
            let insights = vec![Insight::new(Severity::Info, "Title", &long_desc)];
            let output = format_insight_list(&insights);

            assert!(output.contains("..."));
            assert!(output.len() < long_desc.len() + 50); // Should be truncated
        }
    }

    mod format_json_tests {
        use super::*;

        #[test]
        fn test_json_format() {
            let diagnosis = create_test_diagnosis();
            let json = format_as_json(&diagnosis);

            assert!(json.is_ok());
            let json_str = json.expect("JSON should be valid");
            assert!(json_str.contains("\"status\""));
            assert!(json_str.contains("\"insights\""));
            assert!(json_str.contains("node-001"));
        }

        #[test]
        fn test_json_compact_format() {
            let diagnosis = create_test_diagnosis();
            let json = format_as_json_compact(&diagnosis);

            assert!(json.is_ok());
            let json_str = json.expect("JSON should be valid");
            assert!(!json_str.contains('\n')); // Single line
        }

        #[test]
        fn test_json_roundtrip() {
            let original = create_test_diagnosis();
            let json = format_as_json(&original).expect("serialization should work");
            let parsed: Diagnosis =
                serde_json::from_str(&json).expect("deserialization should work");

            assert_eq!(original.id, parsed.id);
            assert_eq!(original.status, parsed.status);
            assert_eq!(original.insights.len(), parsed.insights.len());
        }
    }

    mod truncate_string_tests {
        use super::*;

        #[test]
        fn test_short_string_unchanged() {
            let result = truncate_string("hello", 10);
            assert_eq!(result, "hello");
        }

        #[test]
        fn test_long_string_truncated() {
            let result = truncate_string("hello world", 8);
            assert_eq!(result, "hello...");
        }

        #[test]
        fn test_exact_length() {
            let result = truncate_string("hello", 5);
            assert_eq!(result, "hello");
        }
    }
}
