//! Deploy command implementation.
//!
//! Handles deploying workloads from intent files.

use std::io::Write;

use serde::Serialize;

use crate::cli::DeployArgs;
use crate::error::CliError;
use crate::output::{OutputFormat, TableDisplay};

/// Handler for the deploy command.
pub struct DeployCommand<'a> {
    #[allow(dead_code)]
    gateway_url: &'a str,
}

impl<'a> DeployCommand<'a> {
    /// Creates a new deploy command handler.
    #[must_use]
    pub const fn new(gateway_url: &'a str) -> Self {
        Self { gateway_url }
    }

    /// Executes the deploy command.
    ///
    /// # Errors
    ///
    /// Returns error if the command fails.
    pub async fn execute<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        args: &DeployArgs,
    ) -> Result<(), CliError> {
        // TODO: Read and parse intent file, send to gateway
        let _ = (args.dry_run, args.wait, &args.timeout, &args.namespace);

        let response = DeployResponse {
            success: true,
            workload_name: "my-training-job".into(),
            namespace: args.namespace.clone().unwrap_or_else(|| "default".into()),
            intent_file: args.intent.clone(),
            dry_run: args.dry_run,
            message: if args.dry_run {
                "Dry run completed successfully. No changes applied.".into()
            } else {
                "Deployment initiated successfully".into()
            },
            resources: DeployedResources {
                gpus: 4,
                cpu_cores: 8,
                memory_mib: 32768,
            },
            workloads_created: vec![
                WorkloadCreated {
                    name: "my-training-job".into(),
                    kind: "Job".into(),
                    replicas: 1,
                },
            ],
        };

        format.write(out, &response)?;
        Ok(())
    }
}

// Output types

/// Deploy response.
#[derive(Debug, Clone, Serialize)]
pub struct DeployResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Workload name.
    pub workload_name: String,
    /// Namespace.
    pub namespace: String,
    /// Intent file path.
    pub intent_file: String,
    /// Whether this was a dry run.
    pub dry_run: bool,
    /// Response message.
    pub message: String,
    /// Resources allocated.
    pub resources: DeployedResources,
    /// Workloads created.
    pub workloads_created: Vec<WorkloadCreated>,
}

/// Resources allocated for deployment.
#[derive(Debug, Clone, Serialize)]
pub struct DeployedResources {
    /// Number of GPUs.
    pub gpus: u32,
    /// Number of CPU cores.
    pub cpu_cores: u32,
    /// Memory in MiB.
    pub memory_mib: u64,
}

/// Information about a created workload.
#[derive(Debug, Clone, Serialize)]
pub struct WorkloadCreated {
    /// Workload name.
    pub name: String,
    /// Workload kind (Job, Service, etc.).
    pub kind: String,
    /// Number of replicas.
    pub replicas: u32,
}

impl TableDisplay for DeployResponse {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.dry_run {
            writeln!(writer, "Dry Run Results")?;
        } else {
            writeln!(writer, "Deployment Results")?;
        }
        writeln!(writer, "══════════════════════════════════")?;
        writeln!(writer)?;

        if self.success {
            writeln!(writer, "✓ {}", self.message)?;
        } else {
            writeln!(writer, "✗ {}", self.message)?;
        }
        writeln!(writer)?;

        writeln!(writer, "Intent File:  {}", self.intent_file)?;
        writeln!(writer, "Namespace:    {}", self.namespace)?;
        writeln!(writer)?;

        writeln!(writer, "Resources")?;
        writeln!(writer, "  GPUs:       {}", self.resources.gpus)?;
        writeln!(writer, "  CPU Cores:  {}", self.resources.cpu_cores)?;
        writeln!(writer, "  Memory:     {} MiB", self.resources.memory_mib)?;
        writeln!(writer)?;

        if !self.workloads_created.is_empty() {
            writeln!(writer, "Workloads ({}):", self.workloads_created.len())?;
            for w in &self.workloads_created {
                writeln!(writer, "  {} ({}, {} replica(s))", w.name, w.kind, w.replicas)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Format;

    #[test]
    fn deploy_command_new() {
        let cmd = DeployCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[test]
    fn deploy_response_table_output() {
        let response = DeployResponse {
            success: true,
            workload_name: "training-job".into(),
            namespace: "production".into(),
            intent_file: "deploy.yaml".into(),
            dry_run: false,
            message: "Deployment successful".into(),
            resources: DeployedResources {
                gpus: 8,
                cpu_cores: 16,
                memory_mib: 65536,
            },
            workloads_created: vec![WorkloadCreated {
                name: "training-job".into(),
                kind: "Job".into(),
                replicas: 1,
            }],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&response).expect("should format");

        assert!(output.contains("Deployment Results"));
        assert!(output.contains("Intent File:  deploy.yaml"));
        assert!(output.contains("GPUs:       8"));
        assert!(output.contains("training-job (Job, 1 replica(s))"));
    }

    #[test]
    fn deploy_response_dry_run() {
        let response = DeployResponse {
            success: true,
            workload_name: "test".into(),
            namespace: "default".into(),
            intent_file: "test.yaml".into(),
            dry_run: true,
            message: "Dry run completed".into(),
            resources: DeployedResources {
                gpus: 1,
                cpu_cores: 2,
                memory_mib: 4096,
            },
            workloads_created: vec![],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&response).expect("should format");

        assert!(output.contains("Dry Run Results"));
    }

    #[test]
    fn deploy_response_json() {
        let response = DeployResponse {
            success: true,
            workload_name: "job".into(),
            namespace: "ns".into(),
            intent_file: "f.yaml".into(),
            dry_run: false,
            message: "ok".into(),
            resources: DeployedResources {
                gpus: 2,
                cpu_cores: 4,
                memory_mib: 8192,
            },
            workloads_created: vec![],
        };

        let fmt = OutputFormat::new(Format::Json);
        let output = fmt.to_string(&response).expect("should format");

        assert!(output.contains("\"success\": true"));
        assert!(output.contains("\"gpus\": 2"));
    }
}
