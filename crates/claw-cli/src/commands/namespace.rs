//! Namespace management command implementation.
//!
//! Handles namespace CRUD operations.

use std::io::Write;

use serde::Serialize;

use crate::cli::NamespaceCommands;
use crate::error::CliError;
use crate::output::{OutputFormat, TableDisplay};

/// Handler for namespace subcommands.
pub struct NamespaceCommand<'a> {
    #[allow(dead_code)]
    gateway_url: &'a str,
}

impl<'a> NamespaceCommand<'a> {
    /// Creates a new namespace command handler.
    #[must_use]
    pub const fn new(gateway_url: &'a str) -> Self {
        Self { gateway_url }
    }

    /// Executes the namespace subcommand.
    ///
    /// # Errors
    ///
    /// Returns error if the command fails.
    pub async fn execute<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        command: &NamespaceCommands,
    ) -> Result<(), CliError> {
        match command {
            NamespaceCommands::List { tenant } => self.list(out, format, tenant.as_deref()).await,
            NamespaceCommands::Create {
                name,
                tenant,
                cpu_quota,
                gpu_quota,
                memory_quota,
            } => {
                self.create(
                    out,
                    format,
                    name,
                    tenant.as_deref(),
                    *cpu_quota,
                    *gpu_quota,
                    *memory_quota,
                )
                .await
            }
            NamespaceCommands::Delete { name, yes } => self.delete(out, format, name, *yes).await,
            NamespaceCommands::Info { name } => self.info(out, format, name).await,
        }
    }

    async fn list<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        tenant: Option<&str>,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and fetch namespaces
        let _ = tenant;
        let list = NamespaceList {
            namespaces: vec![
                NamespaceInfo {
                    name: "default".into(),
                    tenant: "default".into(),
                    workloads: 5,
                    cpu_usage: "12/24".into(),
                    gpu_usage: "4/8".into(),
                    created_at: "2024-01-01T00:00:00Z".into(),
                },
                NamespaceInfo {
                    name: "production".into(),
                    tenant: "default".into(),
                    workloads: 10,
                    cpu_usage: "48/64".into(),
                    gpu_usage: "16/32".into(),
                    created_at: "2024-01-05T12:00:00Z".into(),
                },
                NamespaceInfo {
                    name: "staging".into(),
                    tenant: "default".into(),
                    workloads: 3,
                    cpu_usage: "8/16".into(),
                    gpu_usage: "2/4".into(),
                    created_at: "2024-01-10T08:00:00Z".into(),
                },
            ],
        };

        format.write(out, &list)?;
        Ok(())
    }

    async fn create<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        name: &str,
        tenant: Option<&str>,
        cpu_quota: Option<u32>,
        gpu_quota: Option<u32>,
        memory_quota: Option<u64>,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and create namespace
        let _ = (tenant, cpu_quota, gpu_quota, memory_quota);
        let response = NamespaceResponse {
            success: true,
            name: name.to_string(),
            message: format!("Namespace '{}' created successfully", name),
        };

        format.write(out, &response)?;
        Ok(())
    }

    async fn delete<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        name: &str,
        yes: bool,
    ) -> Result<(), CliError> {
        // TODO: Add confirmation prompt if !yes
        let _ = yes;
        let response = NamespaceResponse {
            success: true,
            name: name.to_string(),
            message: format!("Namespace '{}' deleted successfully", name),
        };

        format.write(out, &response)?;
        Ok(())
    }

    async fn info<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        name: &str,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and fetch namespace details
        let detail = NamespaceDetail {
            name: name.to_string(),
            tenant: "default".into(),
            created_at: "2024-01-05T12:00:00Z".into(),
            quotas: QuotaInfo {
                cpu_limit: Some(64),
                cpu_used: 48,
                gpu_limit: Some(32),
                gpu_used: 16,
                memory_limit_mib: Some(262144),
                memory_used_mib: 196608,
            },
            workloads: vec![
                WorkloadBrief {
                    name: "training-job-1".into(),
                    state: "running".into(),
                    gpus: 4,
                },
                WorkloadBrief {
                    name: "inference-service".into(),
                    state: "running".into(),
                    gpus: 8,
                },
            ],
        };

        format.write(out, &detail)?;
        Ok(())
    }
}

// Output types

/// List of namespaces.
#[derive(Debug, Clone, Serialize)]
pub struct NamespaceList {
    /// Namespaces.
    pub namespaces: Vec<NamespaceInfo>,
}

/// Namespace information for listing.
#[derive(Debug, Clone, Serialize)]
pub struct NamespaceInfo {
    /// Namespace name.
    pub name: String,
    /// Tenant name.
    pub tenant: String,
    /// Number of workloads.
    pub workloads: usize,
    /// CPU usage (used/limit).
    pub cpu_usage: String,
    /// GPU usage (used/limit).
    pub gpu_usage: String,
    /// Creation timestamp.
    pub created_at: String,
}

/// Detailed namespace information.
#[derive(Debug, Clone, Serialize)]
pub struct NamespaceDetail {
    /// Namespace name.
    pub name: String,
    /// Tenant name.
    pub tenant: String,
    /// Creation timestamp.
    pub created_at: String,
    /// Resource quotas and usage.
    pub quotas: QuotaInfo,
    /// Workloads in this namespace.
    pub workloads: Vec<WorkloadBrief>,
}

/// Resource quota information.
#[derive(Debug, Clone, Serialize)]
pub struct QuotaInfo {
    /// CPU cores limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_limit: Option<u32>,
    /// CPU cores used.
    pub cpu_used: u32,
    /// GPU count limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_limit: Option<u32>,
    /// GPUs used.
    pub gpu_used: u32,
    /// Memory limit in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_limit_mib: Option<u64>,
    /// Memory used in MiB.
    pub memory_used_mib: u64,
}

/// Brief workload information.
#[derive(Debug, Clone, Serialize)]
pub struct WorkloadBrief {
    /// Workload name.
    pub name: String,
    /// Workload state.
    pub state: String,
    /// Number of GPUs.
    pub gpus: u32,
}

/// Namespace operation response.
#[derive(Debug, Clone, Serialize)]
pub struct NamespaceResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Namespace name.
    pub name: String,
    /// Response message.
    pub message: String,
}

impl TableDisplay for NamespaceList {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.namespaces.is_empty() {
            writeln!(writer, "No namespaces found")?;
            return Ok(());
        }

        writeln!(
            writer,
            "{:<16}  {:<16}  {:>10}  {:>10}  {:>10}",
            "NAME", "TENANT", "WORKLOADS", "CPU", "GPU"
        )?;
        writeln!(writer, "{}", "─".repeat(72))?;

        for ns in &self.namespaces {
            writeln!(
                writer,
                "{:<16}  {:<16}  {:>10}  {:>10}  {:>10}",
                ns.name, ns.tenant, ns.workloads, ns.cpu_usage, ns.gpu_usage
            )?;
        }

        writeln!(writer)?;
        writeln!(writer, "Total: {} namespace(s)", self.namespaces.len())?;
        Ok(())
    }
}

impl TableDisplay for NamespaceDetail {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        writeln!(writer, "Namespace: {}", self.name)?;
        writeln!(writer, "══════════════════════════════════")?;
        writeln!(writer)?;
        writeln!(writer, "Tenant:       {}", self.tenant)?;
        writeln!(writer, "Created:      {}", self.created_at)?;
        writeln!(writer)?;

        writeln!(writer, "Resource Usage")?;
        if let Some(cpu_limit) = self.quotas.cpu_limit {
            writeln!(writer, "  CPU:        {} / {} cores", self.quotas.cpu_used, cpu_limit)?;
        } else {
            writeln!(writer, "  CPU:        {} cores (no quota)", self.quotas.cpu_used)?;
        }
        if let Some(gpu_limit) = self.quotas.gpu_limit {
            writeln!(writer, "  GPU:        {} / {}", self.quotas.gpu_used, gpu_limit)?;
        } else {
            writeln!(writer, "  GPU:        {} (no quota)", self.quotas.gpu_used)?;
        }
        if let Some(mem_limit) = self.quotas.memory_limit_mib {
            writeln!(
                writer,
                "  Memory:     {} / {} MiB",
                self.quotas.memory_used_mib, mem_limit
            )?;
        } else {
            writeln!(
                writer,
                "  Memory:     {} MiB (no quota)",
                self.quotas.memory_used_mib
            )?;
        }
        writeln!(writer)?;

        if self.workloads.is_empty() {
            writeln!(writer, "Workloads: None")?;
        } else {
            writeln!(writer, "Workloads ({}):", self.workloads.len())?;
            for w in &self.workloads {
                writeln!(writer, "  {} ({}, {} GPUs)", w.name, w.state, w.gpus)?;
            }
        }
        Ok(())
    }
}

impl TableDisplay for NamespaceResponse {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.success {
            writeln!(writer, "✓ {}", self.message)?;
        } else {
            writeln!(writer, "✗ {}", self.message)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Format;

    #[test]
    fn namespace_command_new() {
        let cmd = NamespaceCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[test]
    fn namespace_list_table_output() {
        let list = NamespaceList {
            namespaces: vec![NamespaceInfo {
                name: "prod".into(),
                tenant: "default".into(),
                workloads: 5,
                cpu_usage: "10/20".into(),
                gpu_usage: "4/8".into(),
                created_at: "2024-01-15T10:30:00Z".into(),
            }],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("prod"));
        assert!(output.contains("10/20"));
        assert!(output.contains("Total: 1 namespace(s)"));
    }

    #[test]
    fn namespace_list_empty() {
        let list = NamespaceList { namespaces: vec![] };
        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("No namespaces found"));
    }

    #[test]
    fn namespace_detail_table_output() {
        let detail = NamespaceDetail {
            name: "production".into(),
            tenant: "ml-team".into(),
            created_at: "2024-01-01T00:00:00Z".into(),
            quotas: QuotaInfo {
                cpu_limit: Some(64),
                cpu_used: 48,
                gpu_limit: Some(32),
                gpu_used: 16,
                memory_limit_mib: Some(262144),
                memory_used_mib: 196608,
            },
            workloads: vec![WorkloadBrief {
                name: "training".into(),
                state: "running".into(),
                gpus: 8,
            }],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&detail).expect("should format");

        assert!(output.contains("Namespace: production"));
        assert!(output.contains("CPU:        48 / 64 cores"));
        assert!(output.contains("training (running, 8 GPUs)"));
    }

    #[test]
    fn namespace_list_json() {
        let list = NamespaceList {
            namespaces: vec![NamespaceInfo {
                name: "test".into(),
                tenant: "default".into(),
                workloads: 1,
                cpu_usage: "1/2".into(),
                gpu_usage: "0/0".into(),
                created_at: "2024-01-15T10:30:00Z".into(),
            }],
        };

        let fmt = OutputFormat::new(Format::Json);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("\"name\": \"test\""));
        assert!(output.contains("\"tenant\": \"default\""));
    }
}
