//! Preempt command implementation.
//!
//! Handles preempting workloads.

use std::io::Write;

use serde::Serialize;

use crate::cli::PreemptArgs;
use crate::error::CliError;
use crate::output::{OutputFormat, TableDisplay};

/// Handler for the preempt command.
pub struct PreemptCommand<'a> {
    #[allow(dead_code)]
    gateway_url: &'a str,
}

impl<'a> PreemptCommand<'a> {
    /// Creates a new preempt command handler.
    #[must_use]
    pub const fn new(gateway_url: &'a str) -> Self {
        Self { gateway_url }
    }

    /// Executes the preempt command.
    ///
    /// # Errors
    ///
    /// Returns error if the command fails.
    pub async fn execute<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        args: &PreemptArgs,
    ) -> Result<(), CliError> {
        // TODO: Add confirmation prompt if !yes
        // TODO: Send preemption request to gateway
        let _ = args.yes;

        let response = PreemptResponse {
            success: true,
            workload: args.workload.clone(),
            namespace: args.namespace.clone().unwrap_or_else(|| "default".into()),
            reason: args.reason.clone(),
            message: format!("Workload '{}' preempted successfully", args.workload),
            freed_resources: FreedResources {
                gpus: 4,
                cpu_cores: 8,
                memory_mib: 32768,
            },
        };

        format.write(out, &response)?;
        Ok(())
    }
}

// Output types

/// Preempt response.
#[derive(Debug, Clone, Serialize)]
pub struct PreemptResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Workload name.
    pub workload: String,
    /// Namespace.
    pub namespace: String,
    /// Reason for preemption.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Response message.
    pub message: String,
    /// Resources freed.
    pub freed_resources: FreedResources,
}

/// Resources freed by preemption.
#[derive(Debug, Clone, Serialize)]
pub struct FreedResources {
    /// Number of GPUs freed.
    pub gpus: u32,
    /// Number of CPU cores freed.
    pub cpu_cores: u32,
    /// Memory freed in MiB.
    pub memory_mib: u64,
}

impl TableDisplay for PreemptResponse {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        writeln!(writer, "Preemption Results")?;
        writeln!(writer, "══════════════════════════════════")?;
        writeln!(writer)?;

        if self.success {
            writeln!(writer, "✓ {}", self.message)?;
        } else {
            writeln!(writer, "✗ {}", self.message)?;
        }
        writeln!(writer)?;

        writeln!(writer, "Workload:   {}", self.workload)?;
        writeln!(writer, "Namespace:  {}", self.namespace)?;
        if let Some(ref reason) = self.reason {
            writeln!(writer, "Reason:     {}", reason)?;
        }
        writeln!(writer)?;

        writeln!(writer, "Freed Resources")?;
        writeln!(writer, "  GPUs:      {}", self.freed_resources.gpus)?;
        writeln!(writer, "  CPU Cores: {}", self.freed_resources.cpu_cores)?;
        writeln!(writer, "  Memory:    {} MiB", self.freed_resources.memory_mib)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Format;

    #[test]
    fn preempt_command_new() {
        let cmd = PreemptCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[test]
    fn preempt_response_table_output() {
        let response = PreemptResponse {
            success: true,
            workload: "low-priority-job".into(),
            namespace: "default".into(),
            reason: Some("High-priority job needs resources".into()),
            message: "Preempted successfully".into(),
            freed_resources: FreedResources {
                gpus: 2,
                cpu_cores: 4,
                memory_mib: 16384,
            },
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&response).expect("should format");

        assert!(output.contains("Preemption Results"));
        assert!(output.contains("low-priority-job"));
        assert!(output.contains("High-priority job needs resources"));
        assert!(output.contains("GPUs:      2"));
    }

    #[test]
    fn preempt_response_without_reason() {
        let response = PreemptResponse {
            success: true,
            workload: "job".into(),
            namespace: "ns".into(),
            reason: None,
            message: "ok".into(),
            freed_resources: FreedResources {
                gpus: 1,
                cpu_cores: 2,
                memory_mib: 4096,
            },
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&response).expect("should format");

        assert!(!output.contains("Reason:"));
    }

    #[test]
    fn preempt_response_json() {
        let response = PreemptResponse {
            success: true,
            workload: "test".into(),
            namespace: "default".into(),
            reason: None,
            message: "ok".into(),
            freed_resources: FreedResources {
                gpus: 4,
                cpu_cores: 8,
                memory_mib: 32768,
            },
        };

        let fmt = OutputFormat::new(Format::Json);
        let output = fmt.to_string(&response).expect("should format");

        assert!(output.contains("\"success\": true"));
        assert!(output.contains("\"gpus\": 4"));
    }
}
