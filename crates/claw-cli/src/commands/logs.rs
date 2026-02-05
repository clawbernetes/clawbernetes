//! Logs command implementation.
//!
//! Handles viewing workload logs.

use std::io::Write;

use serde::Serialize;

use crate::cli::LogsArgs;
use crate::error::CliError;
use crate::output::{OutputFormat, TableDisplay};

/// Handler for the logs command.
pub struct LogsCommand<'a> {
    #[allow(dead_code)]
    gateway_url: &'a str,
}

impl<'a> LogsCommand<'a> {
    /// Creates a new logs command handler.
    #[must_use]
    pub const fn new(gateway_url: &'a str) -> Self {
        Self { gateway_url }
    }

    /// Executes the logs command.
    ///
    /// # Errors
    ///
    /// Returns error if the command fails.
    pub async fn execute<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        args: &LogsArgs,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and stream logs
        let _ = (args.follow, args.tail, &args.since, &args.namespace, &args.container);

        let logs = LogsOutput {
            workload: args.workload.clone(),
            namespace: args.namespace.clone().unwrap_or_else(|| "default".into()),
            container: args.container.clone(),
            lines: vec![
                LogLine {
                    timestamp: if args.timestamps {
                        Some("2024-01-22T14:00:00.123Z".into())
                    } else {
                        None
                    },
                    message: "Starting training job...".into(),
                    stream: "stdout".into(),
                },
                LogLine {
                    timestamp: if args.timestamps {
                        Some("2024-01-22T14:00:01.456Z".into())
                    } else {
                        None
                    },
                    message: "Loading model from checkpoint...".into(),
                    stream: "stdout".into(),
                },
                LogLine {
                    timestamp: if args.timestamps {
                        Some("2024-01-22T14:00:02.789Z".into())
                    } else {
                        None
                    },
                    message: "GPU 0: NVIDIA RTX 4090 detected".into(),
                    stream: "stdout".into(),
                },
                LogLine {
                    timestamp: if args.timestamps {
                        Some("2024-01-22T14:00:03.012Z".into())
                    } else {
                        None
                    },
                    message: "Epoch 1/10: loss=2.4532, accuracy=0.6234".into(),
                    stream: "stdout".into(),
                },
                LogLine {
                    timestamp: if args.timestamps {
                        Some("2024-01-22T14:05:00.345Z".into())
                    } else {
                        None
                    },
                    message: "Epoch 2/10: loss=1.8765, accuracy=0.7456".into(),
                    stream: "stdout".into(),
                },
            ],
        };

        format.write(out, &logs)?;

        if args.follow {
            writeln!(out)?;
            writeln!(out, "(Log streaming not implemented in placeholder)")?;
        }

        Ok(())
    }
}

// Output types

/// Logs output.
#[derive(Debug, Clone, Serialize)]
pub struct LogsOutput {
    /// Workload name.
    pub workload: String,
    /// Namespace.
    pub namespace: String,
    /// Container name (if specified).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// Log lines.
    pub lines: Vec<LogLine>,
}

/// A single log line.
#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    /// Timestamp (if requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Log message.
    pub message: String,
    /// Stream (stdout/stderr).
    pub stream: String,
}

impl TableDisplay for LogsOutput {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        // For logs, we don't use table format - just output the lines
        for line in &self.lines {
            if let Some(ref ts) = line.timestamp {
                write!(writer, "{} ", ts)?;
            }
            writeln!(writer, "{}", line.message)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Format;

    #[test]
    fn logs_command_new() {
        let cmd = LogsCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[test]
    fn logs_output_table_without_timestamps() {
        let logs = LogsOutput {
            workload: "training".into(),
            namespace: "default".into(),
            container: None,
            lines: vec![
                LogLine {
                    timestamp: None,
                    message: "Hello world".into(),
                    stream: "stdout".into(),
                },
                LogLine {
                    timestamp: None,
                    message: "Training started".into(),
                    stream: "stdout".into(),
                },
            ],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&logs).expect("should format");

        assert!(output.contains("Hello world"));
        assert!(output.contains("Training started"));
        assert!(!output.contains("2024")); // No timestamps
    }

    #[test]
    fn logs_output_table_with_timestamps() {
        let logs = LogsOutput {
            workload: "training".into(),
            namespace: "default".into(),
            container: None,
            lines: vec![LogLine {
                timestamp: Some("2024-01-22T14:00:00Z".into()),
                message: "Test log".into(),
                stream: "stdout".into(),
            }],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&logs).expect("should format");

        assert!(output.contains("2024-01-22T14:00:00Z"));
        assert!(output.contains("Test log"));
    }

    #[test]
    fn logs_output_json() {
        let logs = LogsOutput {
            workload: "job".into(),
            namespace: "prod".into(),
            container: Some("main".into()),
            lines: vec![LogLine {
                timestamp: Some("2024-01-22T14:00:00Z".into()),
                message: "Log message".into(),
                stream: "stderr".into(),
            }],
        };

        let fmt = OutputFormat::new(Format::Json);
        let output = fmt.to_string(&logs).expect("should format");

        assert!(output.contains("\"workload\": \"job\""));
        assert!(output.contains("\"container\": \"main\""));
        assert!(output.contains("\"stream\": \"stderr\""));
    }
}
