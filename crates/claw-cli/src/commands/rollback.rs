//! Rollback command implementation.
//!
//! Handles rolling back workloads to previous versions.

use std::io::Write;

use serde::Serialize;

use crate::cli::RollbackArgs;
use crate::error::CliError;
use crate::output::{OutputFormat, TableDisplay};

/// Handler for the rollback command.
pub struct RollbackCommand<'a> {
    #[allow(dead_code)]
    gateway_url: &'a str,
}

impl<'a> RollbackCommand<'a> {
    /// Creates a new rollback command handler.
    #[must_use]
    pub const fn new(gateway_url: &'a str) -> Self {
        Self { gateway_url }
    }

    /// Executes the rollback command.
    ///
    /// # Errors
    ///
    /// Returns error if the command fails.
    pub async fn execute<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        args: &RollbackArgs,
    ) -> Result<(), CliError> {
        // TODO: Send rollback request to gateway
        let target_revision = args.revision.unwrap_or(2); // Previous revision
        let response = RollbackResponse {
            success: true,
            workload: args.workload.clone(),
            namespace: args.namespace.clone().unwrap_or_else(|| "default".into()),
            from_revision: 3,
            to_revision: target_revision,
            message: format!(
                "Rolled back '{}' from revision 3 to revision {}",
                args.workload, target_revision
            ),
        };

        format.write(out, &response)?;
        Ok(())
    }
}

// Output types

/// Rollback response.
#[derive(Debug, Clone, Serialize)]
pub struct RollbackResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Workload name.
    pub workload: String,
    /// Namespace.
    pub namespace: String,
    /// Previous revision (before rollback).
    pub from_revision: u32,
    /// Target revision (after rollback).
    pub to_revision: u32,
    /// Response message.
    pub message: String,
}

impl TableDisplay for RollbackResponse {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        writeln!(writer, "Rollback Results")?;
        writeln!(writer, "══════════════════════════════════")?;
        writeln!(writer)?;

        if self.success {
            writeln!(writer, "✓ {}", self.message)?;
        } else {
            writeln!(writer, "✗ {}", self.message)?;
        }
        writeln!(writer)?;

        writeln!(writer, "Workload:     {}", self.workload)?;
        writeln!(writer, "Namespace:    {}", self.namespace)?;
        writeln!(
            writer,
            "Revision:     {} → {}",
            self.from_revision, self.to_revision
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Format;

    #[test]
    fn rollback_command_new() {
        let cmd = RollbackCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[test]
    fn rollback_response_table_output() {
        let response = RollbackResponse {
            success: true,
            workload: "my-app".into(),
            namespace: "production".into(),
            from_revision: 5,
            to_revision: 4,
            message: "Rolled back successfully".into(),
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&response).expect("should format");

        assert!(output.contains("Rollback Results"));
        assert!(output.contains("Workload:     my-app"));
        assert!(output.contains("Revision:     5 → 4"));
    }

    #[test]
    fn rollback_response_json() {
        let response = RollbackResponse {
            success: true,
            workload: "test".into(),
            namespace: "default".into(),
            from_revision: 3,
            to_revision: 2,
            message: "ok".into(),
        };

        let fmt = OutputFormat::new(Format::Json);
        let output = fmt.to_string(&response).expect("should format");

        assert!(output.contains("\"success\": true"));
        assert!(output.contains("\"from_revision\": 3"));
        assert!(output.contains("\"to_revision\": 2"));
    }
}
