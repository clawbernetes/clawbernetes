//! Tenant management command implementation.
//!
//! Handles tenant CRUD operations.

use std::io::Write;

use serde::Serialize;

use crate::cli::TenantCommands;
use crate::error::CliError;
use crate::output::{OutputFormat, TableDisplay};

/// Handler for tenant subcommands.
pub struct TenantCommand<'a> {
    #[allow(dead_code)]
    gateway_url: &'a str,
}

impl<'a> TenantCommand<'a> {
    /// Creates a new tenant command handler.
    #[must_use]
    pub const fn new(gateway_url: &'a str) -> Self {
        Self { gateway_url }
    }

    /// Executes the tenant subcommand.
    ///
    /// # Errors
    ///
    /// Returns error if the command fails.
    pub async fn execute<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        command: &TenantCommands,
    ) -> Result<(), CliError> {
        match command {
            TenantCommands::List => self.list(out, format).await,
            TenantCommands::Create {
                name,
                display_name,
                admin_email,
            } => {
                self.create(out, format, name, display_name.as_deref(), admin_email.as_deref())
                    .await
            }
            TenantCommands::Delete { name, yes } => self.delete(out, format, name, *yes).await,
            TenantCommands::Info { name } => self.info(out, format, name).await,
        }
    }

    async fn list<W: Write>(&self, out: &mut W, format: &OutputFormat) -> Result<(), CliError> {
        // TODO: Connect to gateway and fetch tenants
        let list = TenantList {
            tenants: vec![
                TenantInfo {
                    name: "default".into(),
                    display_name: Some("Default Tenant".into()),
                    namespaces: 3,
                    workloads: 12,
                    created_at: "2024-01-01T00:00:00Z".into(),
                },
                TenantInfo {
                    name: "ml-team".into(),
                    display_name: Some("Machine Learning Team".into()),
                    namespaces: 2,
                    workloads: 8,
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
        display_name: Option<&str>,
        admin_email: Option<&str>,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and create tenant
        let _ = (display_name, admin_email);
        let response = TenantResponse {
            success: true,
            name: name.to_string(),
            message: format!("Tenant '{}' created successfully", name),
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
        let response = TenantResponse {
            success: true,
            name: name.to_string(),
            message: format!("Tenant '{}' deleted successfully", name),
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
        // TODO: Connect to gateway and fetch tenant details
        let detail = TenantDetail {
            name: name.to_string(),
            display_name: Some("Example Tenant".into()),
            admin_email: Some("admin@example.com".into()),
            namespaces: vec!["default".into(), "staging".into(), "production".into()],
            workload_count: 12,
            gpu_quota: Some(100),
            gpu_used: 45,
            created_at: "2024-01-01T00:00:00Z".into(),
        };

        format.write(out, &detail)?;
        Ok(())
    }
}

// Output types

/// List of tenants.
#[derive(Debug, Clone, Serialize)]
pub struct TenantList {
    /// Tenants.
    pub tenants: Vec<TenantInfo>,
}

/// Tenant information for listing.
#[derive(Debug, Clone, Serialize)]
pub struct TenantInfo {
    /// Tenant name.
    pub name: String,
    /// Display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Number of namespaces.
    pub namespaces: usize,
    /// Number of workloads.
    pub workloads: usize,
    /// Creation timestamp.
    pub created_at: String,
}

/// Detailed tenant information.
#[derive(Debug, Clone, Serialize)]
pub struct TenantDetail {
    /// Tenant name.
    pub name: String,
    /// Display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Admin email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub admin_email: Option<String>,
    /// Namespaces in this tenant.
    pub namespaces: Vec<String>,
    /// Number of workloads.
    pub workload_count: usize,
    /// GPU quota.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_quota: Option<u32>,
    /// GPUs currently used.
    pub gpu_used: u32,
    /// Creation timestamp.
    pub created_at: String,
}

/// Tenant operation response.
#[derive(Debug, Clone, Serialize)]
pub struct TenantResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Tenant name.
    pub name: String,
    /// Response message.
    pub message: String,
}

impl TableDisplay for TenantList {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.tenants.is_empty() {
            writeln!(writer, "No tenants found")?;
            return Ok(());
        }

        writeln!(
            writer,
            "{:<16}  {:<24}  {:>10}  {:>10}  {:<24}",
            "NAME", "DISPLAY NAME", "NAMESPACES", "WORKLOADS", "CREATED"
        )?;
        writeln!(writer, "{}", "─".repeat(96))?;

        for tenant in &self.tenants {
            writeln!(
                writer,
                "{:<16}  {:<24}  {:>10}  {:>10}  {:<24}",
                tenant.name,
                tenant.display_name.as_deref().unwrap_or("-"),
                tenant.namespaces,
                tenant.workloads,
                tenant.created_at
            )?;
        }

        writeln!(writer)?;
        writeln!(writer, "Total: {} tenant(s)", self.tenants.len())?;
        Ok(())
    }
}

impl TableDisplay for TenantDetail {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        writeln!(writer, "Tenant: {}", self.name)?;
        writeln!(writer, "══════════════════════════════════")?;
        writeln!(writer)?;
        if let Some(ref display_name) = self.display_name {
            writeln!(writer, "Display Name:   {}", display_name)?;
        }
        if let Some(ref email) = self.admin_email {
            writeln!(writer, "Admin Email:    {}", email)?;
        }
        writeln!(writer, "Created:        {}", self.created_at)?;
        writeln!(writer)?;

        writeln!(writer, "Resources")?;
        writeln!(writer, "  Workloads:    {}", self.workload_count)?;
        if let Some(quota) = self.gpu_quota {
            writeln!(writer, "  GPU Usage:    {} / {}", self.gpu_used, quota)?;
        } else {
            writeln!(writer, "  GPU Usage:    {} (no quota)", self.gpu_used)?;
        }
        writeln!(writer)?;

        writeln!(writer, "Namespaces ({}):", self.namespaces.len())?;
        for ns in &self.namespaces {
            writeln!(writer, "  - {}", ns)?;
        }
        Ok(())
    }
}

impl TableDisplay for TenantResponse {
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
    fn tenant_command_new() {
        let cmd = TenantCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[test]
    fn tenant_list_table_output() {
        let list = TenantList {
            tenants: vec![TenantInfo {
                name: "ml-team".into(),
                display_name: Some("ML Team".into()),
                namespaces: 2,
                workloads: 5,
                created_at: "2024-01-15T10:30:00Z".into(),
            }],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("ml-team"));
        assert!(output.contains("ML Team"));
        assert!(output.contains("Total: 1 tenant(s)"));
    }

    #[test]
    fn tenant_list_empty() {
        let list = TenantList { tenants: vec![] };
        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("No tenants found"));
    }

    #[test]
    fn tenant_detail_table_output() {
        let detail = TenantDetail {
            name: "production".into(),
            display_name: Some("Production Tenant".into()),
            admin_email: Some("admin@prod.com".into()),
            namespaces: vec!["app".into(), "data".into()],
            workload_count: 10,
            gpu_quota: Some(50),
            gpu_used: 25,
            created_at: "2024-01-01T00:00:00Z".into(),
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&detail).expect("should format");

        assert!(output.contains("Tenant: production"));
        assert!(output.contains("GPU Usage:    25 / 50"));
        assert!(output.contains("- app"));
    }

    #[test]
    fn tenant_list_json() {
        let list = TenantList {
            tenants: vec![TenantInfo {
                name: "test".into(),
                display_name: None,
                namespaces: 1,
                workloads: 0,
                created_at: "2024-01-15T10:30:00Z".into(),
            }],
        };

        let fmt = OutputFormat::new(Format::Json);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("\"name\": \"test\""));
        assert!(output.contains("\"namespaces\": 1"));
    }
}
