//! Metrics command implementation.
//!
//! Handles querying and listing metrics.

use std::io::Write;

use serde::Serialize;

use crate::cli::MetricsCommands;
use crate::error::CliError;
use crate::output::{OutputFormat, TableDisplay};

/// Handler for metrics subcommands.
pub struct MetricsCommand<'a> {
    #[allow(dead_code)]
    gateway_url: &'a str,
}

impl<'a> MetricsCommand<'a> {
    /// Creates a new metrics command handler.
    #[must_use]
    pub const fn new(gateway_url: &'a str) -> Self {
        Self { gateway_url }
    }

    /// Executes the metrics subcommand.
    ///
    /// # Errors
    ///
    /// Returns error if the command fails.
    pub async fn execute<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        command: &MetricsCommands,
    ) -> Result<(), CliError> {
        match command {
            MetricsCommands::Query {
                expr,
                start,
                end,
                step,
            } => {
                self.query(out, format, expr, start.as_deref(), end.as_deref(), step.as_deref())
                    .await
            }
            MetricsCommands::List { prefix } => self.list(out, format, prefix.as_deref()).await,
        }
    }

    async fn query<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        expr: &str,
        start: Option<&str>,
        end: Option<&str>,
        step: Option<&str>,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and query metrics
        let _ = (start, end, step);
        let result = MetricsQueryResult {
            query: expr.to_string(),
            result_type: "vector".into(),
            samples: vec![
                MetricSample {
                    labels: vec![
                        ("instance".into(), "node-1".into()),
                        ("gpu".into(), "0".into()),
                    ],
                    value: 85.5,
                    timestamp: "2024-01-22T14:00:00Z".into(),
                },
                MetricSample {
                    labels: vec![
                        ("instance".into(), "node-1".into()),
                        ("gpu".into(), "1".into()),
                    ],
                    value: 72.3,
                    timestamp: "2024-01-22T14:00:00Z".into(),
                },
                MetricSample {
                    labels: vec![
                        ("instance".into(), "node-2".into()),
                        ("gpu".into(), "0".into()),
                    ],
                    value: 91.2,
                    timestamp: "2024-01-22T14:00:00Z".into(),
                },
            ],
        };

        format.write(out, &result)?;
        Ok(())
    }

    async fn list<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        prefix: Option<&str>,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and list metrics
        let _ = prefix;
        let list = MetricsList {
            metrics: vec![
                MetricMeta {
                    name: "clawbernetes_gpu_utilization".into(),
                    description: "GPU utilization percentage".into(),
                    metric_type: "gauge".into(),
                },
                MetricMeta {
                    name: "clawbernetes_gpu_memory_used_bytes".into(),
                    description: "GPU memory used in bytes".into(),
                    metric_type: "gauge".into(),
                },
                MetricMeta {
                    name: "clawbernetes_workload_count".into(),
                    description: "Number of active workloads".into(),
                    metric_type: "gauge".into(),
                },
                MetricMeta {
                    name: "clawbernetes_job_duration_seconds".into(),
                    description: "Job execution duration".into(),
                    metric_type: "histogram".into(),
                },
                MetricMeta {
                    name: "clawbernetes_node_heartbeat_total".into(),
                    description: "Total node heartbeats".into(),
                    metric_type: "counter".into(),
                },
            ],
        };

        format.write(out, &list)?;
        Ok(())
    }
}

// Output types

/// Metrics query result.
#[derive(Debug, Clone, Serialize)]
pub struct MetricsQueryResult {
    /// Original query.
    pub query: String,
    /// Result type (vector, matrix, scalar).
    pub result_type: String,
    /// Sample data.
    pub samples: Vec<MetricSample>,
}

/// A single metric sample.
#[derive(Debug, Clone, Serialize)]
pub struct MetricSample {
    /// Metric labels.
    pub labels: Vec<(String, String)>,
    /// Metric value.
    pub value: f64,
    /// Timestamp.
    pub timestamp: String,
}

/// List of available metrics.
#[derive(Debug, Clone, Serialize)]
pub struct MetricsList {
    /// Metrics.
    pub metrics: Vec<MetricMeta>,
}

/// Metric metadata.
#[derive(Debug, Clone, Serialize)]
pub struct MetricMeta {
    /// Metric name.
    pub name: String,
    /// Metric description.
    pub description: String,
    /// Metric type (gauge, counter, histogram, summary).
    pub metric_type: String,
}

impl TableDisplay for MetricsQueryResult {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        writeln!(writer, "Query: {}", self.query)?;
        writeln!(writer, "Type:  {}", self.result_type)?;
        writeln!(writer, "══════════════════════════════════════════════════")?;
        writeln!(writer)?;

        if self.samples.is_empty() {
            writeln!(writer, "No data")?;
            return Ok(());
        }

        for sample in &self.samples {
            let labels_str: Vec<String> = sample
                .labels
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            writeln!(writer, "{{{}}}", labels_str.join(", "))?;
            writeln!(writer, "  Value:     {:.4}", sample.value)?;
            writeln!(writer, "  Timestamp: {}", sample.timestamp)?;
            writeln!(writer)?;
        }

        writeln!(writer, "Total: {} sample(s)", self.samples.len())?;
        Ok(())
    }
}

impl TableDisplay for MetricsList {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.metrics.is_empty() {
            writeln!(writer, "No metrics found")?;
            return Ok(());
        }

        writeln!(
            writer,
            "{:<40}  {:<12}  {}",
            "NAME", "TYPE", "DESCRIPTION"
        )?;
        writeln!(writer, "{}", "─".repeat(100))?;

        for metric in &self.metrics {
            writeln!(
                writer,
                "{:<40}  {:<12}  {}",
                metric.name,
                metric.metric_type,
                truncate(&metric.description, 40)
            )?;
        }

        writeln!(writer)?;
        writeln!(writer, "Total: {} metric(s)", self.metrics.len())?;
        Ok(())
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len > 3 {
        format!("{}...", &s[..max_len - 3])
    } else {
        s[..max_len].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Format;

    #[test]
    fn metrics_command_new() {
        let cmd = MetricsCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[test]
    fn metrics_query_result_table_output() {
        let result = MetricsQueryResult {
            query: "gpu_utilization".into(),
            result_type: "vector".into(),
            samples: vec![MetricSample {
                labels: vec![("node".into(), "1".into())],
                value: 75.5,
                timestamp: "2024-01-22T14:00:00Z".into(),
            }],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&result).expect("should format");

        assert!(output.contains("Query: gpu_utilization"));
        assert!(output.contains("node=1"));
        assert!(output.contains("75.5"));
    }

    #[test]
    fn metrics_query_empty() {
        let result = MetricsQueryResult {
            query: "nonexistent".into(),
            result_type: "vector".into(),
            samples: vec![],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&result).expect("should format");

        assert!(output.contains("No data"));
    }

    #[test]
    fn metrics_list_table_output() {
        let list = MetricsList {
            metrics: vec![MetricMeta {
                name: "gpu_temp".into(),
                description: "GPU temperature".into(),
                metric_type: "gauge".into(),
            }],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("gpu_temp"));
        assert!(output.contains("gauge"));
        assert!(output.contains("Total: 1 metric(s)"));
    }

    #[test]
    fn metrics_list_json() {
        let list = MetricsList {
            metrics: vec![MetricMeta {
                name: "test".into(),
                description: "Test metric".into(),
                metric_type: "counter".into(),
            }],
        };

        let fmt = OutputFormat::new(Format::Json);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("\"name\": \"test\""));
        assert!(output.contains("\"metric_type\": \"counter\""));
    }
}
