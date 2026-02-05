//! Service discovery command implementation.
//!
//! Handles service registration and discovery operations.

use std::io::Write;

use serde::Serialize;

use crate::cli::ServiceCommands;
use crate::error::CliError;
use crate::output::{OutputFormat, TableDisplay};

/// Handler for service subcommands.
pub struct ServiceCommand<'a> {
    #[allow(dead_code)]
    gateway_url: &'a str,
}

impl<'a> ServiceCommand<'a> {
    /// Creates a new service command handler.
    #[must_use]
    pub const fn new(gateway_url: &'a str) -> Self {
        Self { gateway_url }
    }

    /// Executes the service subcommand.
    ///
    /// # Errors
    ///
    /// Returns error if the command fails.
    pub async fn execute<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        command: &ServiceCommands,
    ) -> Result<(), CliError> {
        match command {
            ServiceCommands::List { namespace } => self.list(out, format, namespace.as_deref()).await,
            ServiceCommands::Register {
                name,
                address,
                port,
                health_check,
                namespace,
                tags,
            } => {
                self.register(
                    out,
                    format,
                    name,
                    address,
                    *port,
                    health_check.as_deref(),
                    namespace.as_deref(),
                    tags,
                )
                .await
            }
            ServiceCommands::Deregister { name, yes } => {
                self.deregister(out, format, name, *yes).await
            }
            ServiceCommands::Info { name } => self.info(out, format, name).await,
        }
    }

    async fn list<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        namespace: Option<&str>,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and fetch services
        let _ = namespace;
        let list = ServiceList {
            services: vec![
                ServiceInfo {
                    name: "model-serving".into(),
                    address: "10.0.1.50".into(),
                    port: 8080,
                    namespace: "production".into(),
                    status: "healthy".into(),
                    tags: vec!["ml".into(), "inference".into()],
                },
                ServiceInfo {
                    name: "training-api".into(),
                    address: "10.0.1.51".into(),
                    port: 9000,
                    namespace: "production".into(),
                    status: "healthy".into(),
                    tags: vec!["ml".into(), "training".into()],
                },
                ServiceInfo {
                    name: "metrics-collector".into(),
                    address: "10.0.1.52".into(),
                    port: 9090,
                    namespace: "monitoring".into(),
                    status: "degraded".into(),
                    tags: vec!["metrics".into()],
                },
            ],
        };

        format.write(out, &list)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn register<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        name: &str,
        address: &str,
        port: u16,
        health_check: Option<&str>,
        namespace: Option<&str>,
        tags: &[String],
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and register service
        let _ = (address, port, health_check, namespace, tags);
        let response = ServiceResponse {
            success: true,
            name: name.to_string(),
            message: format!("Service '{}' registered successfully", name),
        };

        format.write(out, &response)?;
        Ok(())
    }

    async fn deregister<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        name: &str,
        yes: bool,
    ) -> Result<(), CliError> {
        // TODO: Add confirmation prompt if !yes
        let _ = yes;
        let response = ServiceResponse {
            success: true,
            name: name.to_string(),
            message: format!("Service '{}' deregistered successfully", name),
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
        // TODO: Connect to gateway and fetch service details
        let detail = ServiceDetail {
            name: name.to_string(),
            address: "10.0.1.50".into(),
            port: 8080,
            namespace: "production".into(),
            status: "healthy".into(),
            tags: vec!["ml".into(), "inference".into()],
            health_check: Some("/health".into()),
            registered_at: "2024-01-15T10:30:00Z".into(),
            last_health_check: Some("2024-01-22T14:00:00Z".into()),
            endpoints: vec![
                EndpointInfo {
                    address: "10.0.1.50:8080".into(),
                    status: "healthy".into(),
                },
                EndpointInfo {
                    address: "10.0.1.51:8080".into(),
                    status: "healthy".into(),
                },
            ],
        };

        format.write(out, &detail)?;
        Ok(())
    }
}

// Output types

/// List of services.
#[derive(Debug, Clone, Serialize)]
pub struct ServiceList {
    /// Services.
    pub services: Vec<ServiceInfo>,
}

/// Service information for listing.
#[derive(Debug, Clone, Serialize)]
pub struct ServiceInfo {
    /// Service name.
    pub name: String,
    /// Service address.
    pub address: String,
    /// Service port.
    pub port: u16,
    /// Namespace.
    pub namespace: String,
    /// Health status.
    pub status: String,
    /// Service tags.
    pub tags: Vec<String>,
}

/// Detailed service information.
#[derive(Debug, Clone, Serialize)]
pub struct ServiceDetail {
    /// Service name.
    pub name: String,
    /// Service address.
    pub address: String,
    /// Service port.
    pub port: u16,
    /// Namespace.
    pub namespace: String,
    /// Health status.
    pub status: String,
    /// Service tags.
    pub tags: Vec<String>,
    /// Health check endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_check: Option<String>,
    /// Registration timestamp.
    pub registered_at: String,
    /// Last health check timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_health_check: Option<String>,
    /// Service endpoints.
    pub endpoints: Vec<EndpointInfo>,
}

/// Endpoint information.
#[derive(Debug, Clone, Serialize)]
pub struct EndpointInfo {
    /// Endpoint address.
    pub address: String,
    /// Endpoint status.
    pub status: String,
}

/// Service operation response.
#[derive(Debug, Clone, Serialize)]
pub struct ServiceResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Service name.
    pub name: String,
    /// Response message.
    pub message: String,
}

impl TableDisplay for ServiceList {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.services.is_empty() {
            writeln!(writer, "No services found")?;
            return Ok(());
        }

        writeln!(
            writer,
            "{:<20}  {:<16}  {:>6}  {:<12}  {:<10}  {}",
            "NAME", "ADDRESS", "PORT", "NAMESPACE", "STATUS", "TAGS"
        )?;
        writeln!(writer, "{}", "─".repeat(96))?;

        for svc in &self.services {
            let status_icon = match svc.status.as_str() {
                "healthy" => "🟢",
                "degraded" => "🟡",
                "unhealthy" => "🔴",
                _ => "⚪",
            };
            writeln!(
                writer,
                "{:<20}  {:<16}  {:>6}  {:<12}  {} {:<7}  {}",
                svc.name,
                svc.address,
                svc.port,
                svc.namespace,
                status_icon,
                svc.status,
                svc.tags.join(",")
            )?;
        }

        let healthy = self
            .services
            .iter()
            .filter(|s| s.status == "healthy")
            .count();
        writeln!(writer)?;
        writeln!(
            writer,
            "Total: {} service(s) ({} healthy)",
            self.services.len(),
            healthy
        )?;
        Ok(())
    }
}

impl TableDisplay for ServiceDetail {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        let status_icon = match self.status.as_str() {
            "healthy" => "🟢",
            "degraded" => "🟡",
            "unhealthy" => "🔴",
            _ => "⚪",
        };

        writeln!(writer, "Service: {}", self.name)?;
        writeln!(writer, "══════════════════════════════════")?;
        writeln!(writer)?;
        writeln!(writer, "Address:      {}:{}", self.address, self.port)?;
        writeln!(writer, "Namespace:    {}", self.namespace)?;
        writeln!(writer, "Status:       {} {}", status_icon, self.status)?;
        writeln!(writer, "Tags:         {}", self.tags.join(", "))?;
        if let Some(ref hc) = self.health_check {
            writeln!(writer, "Health Check: {}", hc)?;
        }
        writeln!(writer, "Registered:   {}", self.registered_at)?;
        if let Some(ref lhc) = self.last_health_check {
            writeln!(writer, "Last Check:   {}", lhc)?;
        }
        writeln!(writer)?;

        if self.endpoints.is_empty() {
            writeln!(writer, "Endpoints: None")?;
        } else {
            writeln!(writer, "Endpoints ({}):", self.endpoints.len())?;
            for ep in &self.endpoints {
                writeln!(writer, "  {} ({})", ep.address, ep.status)?;
            }
        }
        Ok(())
    }
}

impl TableDisplay for ServiceResponse {
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
    fn service_command_new() {
        let cmd = ServiceCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[test]
    fn service_list_table_output() {
        let list = ServiceList {
            services: vec![ServiceInfo {
                name: "api".into(),
                address: "10.0.0.1".into(),
                port: 8080,
                namespace: "default".into(),
                status: "healthy".into(),
                tags: vec!["web".into()],
            }],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("api"));
        assert!(output.contains("10.0.0.1"));
        assert!(output.contains("8080"));
        assert!(output.contains("Total: 1 service(s)"));
    }

    #[test]
    fn service_list_empty() {
        let list = ServiceList { services: vec![] };
        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("No services found"));
    }

    #[test]
    fn service_detail_table_output() {
        let detail = ServiceDetail {
            name: "model-api".into(),
            address: "10.0.1.50".into(),
            port: 8080,
            namespace: "production".into(),
            status: "healthy".into(),
            tags: vec!["ml".into()],
            health_check: Some("/health".into()),
            registered_at: "2024-01-15T10:30:00Z".into(),
            last_health_check: Some("2024-01-22T14:00:00Z".into()),
            endpoints: vec![EndpointInfo {
                address: "10.0.1.50:8080".into(),
                status: "healthy".into(),
            }],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&detail).expect("should format");

        assert!(output.contains("Service: model-api"));
        assert!(output.contains("Address:      10.0.1.50:8080"));
        assert!(output.contains("Health Check: /health"));
    }

    #[test]
    fn service_list_json() {
        let list = ServiceList {
            services: vec![ServiceInfo {
                name: "test".into(),
                address: "localhost".into(),
                port: 3000,
                namespace: "dev".into(),
                status: "healthy".into(),
                tags: vec![],
            }],
        };

        let fmt = OutputFormat::new(Format::Json);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("\"name\": \"test\""));
        assert!(output.contains("\"port\": 3000"));
    }
}
