//! Run workload command implementation.
//!
//! Executes containers on the cluster with GPU support.

use std::io::Write;

use serde::Serialize;

use crate::cli::RunArgs;
use crate::error::CliError;
use crate::output::{OutputFormat, TableDisplay};

/// Run command executor.
pub struct RunCommand {
    gateway_url: String,
}

impl RunCommand {
    /// Create a new run command.
    #[must_use]
    pub fn new(gateway_url: impl Into<String>) -> Self {
        Self {
            gateway_url: gateway_url.into(),
        }
    }

    /// Execute the run command.
    ///
    /// # Errors
    ///
    /// Returns an error if the workload cannot be started.
    pub async fn execute<W: Write>(
        &self,
        writer: &mut W,
        format: &OutputFormat,
        args: &RunArgs,
    ) -> Result<(), CliError> {
        self.validate_gateway_url()?;
        let spec = self.build_workload_spec(args)?;
        let result = self.submit_workload(&spec).await?;

        if args.detach {
            let msg = RunResult {
                workload_id: result.workload_id,
                message: "Workload submitted".into(),
                detached: true,
            };
            format.write(writer, &msg)?;
        } else {
            // In attached mode, we would stream logs here
            // For now, just show the workload ID
            let msg = RunResult {
                workload_id: result.workload_id,
                message: "Workload running (attached mode not yet implemented)".into(),
                detached: false,
            };
            format.write(writer, &msg)?;
        }

        Ok(())
    }

    /// Validate the gateway URL format.
    fn validate_gateway_url(&self) -> Result<(), CliError> {
        if !self.gateway_url.starts_with("ws://") && !self.gateway_url.starts_with("wss://") {
            return Err(CliError::Config(format!(
                "invalid gateway URL: {}, must start with ws:// or wss://",
                self.gateway_url
            )));
        }
        Ok(())
    }

    /// Build a workload spec from run arguments.
    ///
    /// # Errors
    ///
    /// Returns an error if the arguments are invalid.
    pub fn build_workload_spec(&self, args: &RunArgs) -> Result<WorkloadSpec, CliError> {
        // Validate image name
        if args.image.is_empty() {
            return Err(CliError::InvalidArgument("image name cannot be empty".into()));
        }

        // Parse environment variables
        let mut env = Vec::new();
        for e in &args.env {
            let parts: Vec<&str> = e.splitn(2, '=').collect();
            if parts.len() != 2 {
                return Err(CliError::InvalidArgument(format!(
                    "invalid environment variable format: {e}, expected KEY=VALUE"
                )));
            }
            env.push((parts[0].to_string(), parts[1].to_string()));
        }

        Ok(WorkloadSpec {
            image: args.image.clone(),
            command: if args.command.is_empty() {
                None
            } else {
                Some(args.command.clone())
            },
            gpu_indices: args.gpus.clone(),
            env,
            memory_limit_mib: args.memory,
        })
    }

    /// Submit a workload to the cluster.
    ///
    /// # Errors
    ///
    /// Returns an error if submission fails.
    pub async fn submit_workload(&self, _spec: &WorkloadSpec) -> Result<SubmitResult, CliError> {
        // TODO: Replace with actual gateway call
        // This is a placeholder
        Ok(SubmitResult {
            workload_id: format!("wl-{}", uuid::Uuid::new_v4()),
        })
    }
}

/// Workload specification for submission.
#[derive(Debug, Clone, Serialize)]
pub struct WorkloadSpec {
    /// Container image to run.
    pub image: String,
    /// Optional command override.
    pub command: Option<Vec<String>>,
    /// GPU indices to attach.
    pub gpu_indices: Vec<u32>,
    /// Environment variables.
    pub env: Vec<(String, String)>,
    /// Memory limit in MiB.
    pub memory_limit_mib: Option<u64>,
}

/// Result of workload submission.
#[derive(Debug, Clone, Serialize)]
pub struct SubmitResult {
    /// ID of the submitted workload.
    pub workload_id: String,
}

/// Run command result for output.
#[derive(Debug, Clone, Serialize)]
pub struct RunResult {
    /// Workload ID.
    pub workload_id: String,
    /// Status message.
    pub message: String,
    /// Whether running detached.
    pub detached: bool,
}

impl TableDisplay for RunResult {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.detached {
            writeln!(writer, "✓ {}", self.message)?;
            writeln!(writer, "  Workload ID: {}", self.workload_id)?;
            writeln!(writer)?;
            writeln!(
                writer,
                "View logs: clawbernetes logs {}",
                self.workload_id
            )?;
        } else {
            writeln!(writer, "{}", self.message)?;
            writeln!(writer, "Workload ID: {}", self.workload_id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Format;

    fn make_minimal_args() -> RunArgs {
        RunArgs {
            image: "nginx:latest".into(),
            command: vec![],
            gpus: vec![],
            env: vec![],
            memory: None,
            detach: false,
        }
    }

    #[test]
    fn run_command_new() {
        let cmd = RunCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[test]
    fn run_command_validates_gateway_url() {
        let cmd = RunCommand::new("http://invalid");
        let result = cmd.validate_gateway_url();
        assert!(result.is_err());
    }

    #[test]
    fn run_command_accepts_valid_url() {
        let cmd = RunCommand::new("ws://localhost:8080");
        assert!(cmd.validate_gateway_url().is_ok());
    }

    #[test]
    fn build_workload_spec_minimal() {
        let cmd = RunCommand::new("ws://localhost:8080");
        let args = make_minimal_args();

        let spec = cmd.build_workload_spec(&args).expect("should build");

        assert_eq!(spec.image, "nginx:latest");
        assert!(spec.command.is_none());
        assert!(spec.gpu_indices.is_empty());
        assert!(spec.env.is_empty());
        assert!(spec.memory_limit_mib.is_none());
    }

    #[test]
    fn build_workload_spec_with_command() {
        let cmd = RunCommand::new("ws://localhost:8080");
        let args = RunArgs {
            image: "python:latest".into(),
            command: vec!["python".into(), "-m".into(), "http.server".into()],
            gpus: vec![],
            env: vec![],
            memory: None,
            detach: false,
        };

        let spec = cmd.build_workload_spec(&args).expect("should build");

        assert_eq!(spec.command, Some(vec!["python".into(), "-m".into(), "http.server".into()]));
    }

    #[test]
    fn build_workload_spec_with_gpus() {
        let cmd = RunCommand::new("ws://localhost:8080");
        let args = RunArgs {
            image: "pytorch:latest".into(),
            command: vec![],
            gpus: vec![0, 1, 2],
            env: vec![],
            memory: None,
            detach: false,
        };

        let spec = cmd.build_workload_spec(&args).expect("should build");

        assert_eq!(spec.gpu_indices, vec![0, 1, 2]);
    }

    #[test]
    fn build_workload_spec_with_env() {
        let cmd = RunCommand::new("ws://localhost:8080");
        let args = RunArgs {
            image: "app:latest".into(),
            command: vec![],
            gpus: vec![],
            env: vec!["FOO=bar".into(), "BAZ=qux".into()],
            memory: None,
            detach: false,
        };

        let spec = cmd.build_workload_spec(&args).expect("should build");

        assert_eq!(spec.env.len(), 2);
        assert_eq!(spec.env[0], ("FOO".into(), "bar".into()));
        assert_eq!(spec.env[1], ("BAZ".into(), "qux".into()));
    }

    #[test]
    fn build_workload_spec_with_memory() {
        let cmd = RunCommand::new("ws://localhost:8080");
        let args = RunArgs {
            image: "app:latest".into(),
            command: vec![],
            gpus: vec![],
            env: vec![],
            memory: Some(8192),
            detach: false,
        };

        let spec = cmd.build_workload_spec(&args).expect("should build");

        assert_eq!(spec.memory_limit_mib, Some(8192));
    }

    #[test]
    fn build_workload_spec_empty_image_fails() {
        let cmd = RunCommand::new("ws://localhost:8080");
        let args = RunArgs {
            image: String::new(),
            command: vec![],
            gpus: vec![],
            env: vec![],
            memory: None,
            detach: false,
        };

        let result = cmd.build_workload_spec(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("image name cannot be empty"));
    }

    #[test]
    fn build_workload_spec_invalid_env_fails() {
        let cmd = RunCommand::new("ws://localhost:8080");
        let args = RunArgs {
            image: "app:latest".into(),
            command: vec![],
            gpus: vec![],
            env: vec!["INVALID_NO_EQUALS".into()],
            memory: None,
            detach: false,
        };

        let result = cmd.build_workload_spec(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("KEY=VALUE"));
    }

    #[test]
    fn build_workload_spec_env_with_equals_in_value() {
        let cmd = RunCommand::new("ws://localhost:8080");
        let args = RunArgs {
            image: "app:latest".into(),
            command: vec![],
            gpus: vec![],
            env: vec!["CONFIG=a=b=c".into()],
            memory: None,
            detach: false,
        };

        let spec = cmd.build_workload_spec(&args).expect("should build");

        assert_eq!(spec.env[0], ("CONFIG".into(), "a=b=c".into()));
    }

    #[tokio::test]
    async fn submit_workload_returns_id() {
        let cmd = RunCommand::new("ws://localhost:8080");
        let spec = WorkloadSpec {
            image: "test:latest".into(),
            command: None,
            gpu_indices: vec![],
            env: vec![],
            memory_limit_mib: None,
        };

        let result = cmd.submit_workload(&spec).await.expect("should submit");

        assert!(result.workload_id.starts_with("wl-"));
    }

    #[tokio::test]
    async fn execute_detached() {
        let cmd = RunCommand::new("ws://localhost:8080");
        let format = OutputFormat::new(Format::Table);
        let args = RunArgs {
            image: "nginx:latest".into(),
            command: vec![],
            gpus: vec![],
            env: vec![],
            memory: None,
            detach: true,
        };
        let mut buf = Vec::new();

        cmd.execute(&mut buf, &format, &args).await.expect("should execute");

        let output = String::from_utf8(buf).expect("valid utf8");
        assert!(output.contains("Workload submitted"));
        assert!(output.contains("Workload ID:"));
    }

    #[tokio::test]
    async fn execute_attached() {
        let cmd = RunCommand::new("ws://localhost:8080");
        let format = OutputFormat::new(Format::Table);
        let args = make_minimal_args();
        let mut buf = Vec::new();

        cmd.execute(&mut buf, &format, &args).await.expect("should execute");

        let output = String::from_utf8(buf).expect("valid utf8");
        assert!(output.contains("Workload ID:"));
    }

    #[tokio::test]
    async fn execute_json_output() {
        let cmd = RunCommand::new("ws://localhost:8080");
        let format = OutputFormat::new(Format::Json);
        let args = RunArgs {
            image: "app:latest".into(),
            command: vec![],
            gpus: vec![],
            env: vec![],
            memory: None,
            detach: true,
        };
        let mut buf = Vec::new();

        cmd.execute(&mut buf, &format, &args).await.expect("should execute");

        let output = String::from_utf8(buf).expect("valid utf8");
        assert!(output.contains("\"workload_id\""));
        assert!(output.contains("\"detached\": true"));
    }

    #[tokio::test]
    async fn execute_invalid_gateway() {
        let cmd = RunCommand::new("http://invalid");
        let format = OutputFormat::new(Format::Table);
        let args = make_minimal_args();
        let mut buf = Vec::new();

        let result = cmd.execute(&mut buf, &format, &args).await;
        assert!(matches!(result, Err(CliError::Config(_))));
    }

    #[test]
    fn run_result_table_detached() {
        let result = RunResult {
            workload_id: "wl-123".into(),
            message: "Workload submitted".into(),
            detached: true,
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&result).expect("should format");

        assert!(output.contains("✓ Workload submitted"));
        assert!(output.contains("wl-123"));
        assert!(output.contains("clawbernetes logs"));
    }

    #[test]
    fn run_result_table_attached() {
        let result = RunResult {
            workload_id: "wl-456".into(),
            message: "Running".into(),
            detached: false,
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&result).expect("should format");

        assert!(output.contains("Running"));
        assert!(output.contains("wl-456"));
        assert!(!output.contains("✓")); // Not detached, no checkmark
    }

    #[test]
    fn workload_spec_serializes() {
        let spec = WorkloadSpec {
            image: "test:latest".into(),
            command: Some(vec!["echo".into(), "hello".into()]),
            gpu_indices: vec![0, 1],
            env: vec![("KEY".into(), "value".into())],
            memory_limit_mib: Some(4096),
        };

        let json = serde_json::to_string(&spec).expect("should serialize");

        assert!(json.contains("\"image\":\"test:latest\""));
        assert!(json.contains("\"gpu_indices\":[0,1]"));
    }
}
