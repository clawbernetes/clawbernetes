//! Secret management command implementation.
//!
//! Handles secret CRUD operations.

use std::io::Write;

use serde::Serialize;

use crate::cli::SecretCommands;
use crate::error::CliError;
use crate::output::{OutputFormat, TableDisplay};

/// Handler for secret subcommands.
pub struct SecretCommand<'a> {
    #[allow(dead_code)]
    gateway_url: &'a str,
}

impl<'a> SecretCommand<'a> {
    /// Creates a new secret command handler.
    #[must_use]
    pub const fn new(gateway_url: &'a str) -> Self {
        Self { gateway_url }
    }

    /// Executes the secret subcommand.
    ///
    /// # Errors
    ///
    /// Returns error if the command fails.
    pub async fn execute<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        command: &SecretCommands,
    ) -> Result<(), CliError> {
        match command {
            SecretCommands::List { namespace } => self.list(out, format, namespace.as_deref()).await,
            SecretCommands::Get { name, namespace } => {
                self.get(out, format, name, namespace.as_deref()).await
            }
            SecretCommands::Set {
                name,
                value,
                file,
                namespace,
            } => {
                self.set(out, format, name, value.as_deref(), file.as_deref(), namespace.as_deref())
                    .await
            }
            SecretCommands::Delete { name, namespace, yes } => {
                self.delete(out, format, name, namespace.as_deref(), *yes).await
            }
            SecretCommands::Rotate { name, namespace } => {
                self.rotate(out, format, name, namespace.as_deref()).await
            }
        }
    }

    async fn list<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        namespace: Option<&str>,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and fetch secrets
        let _ = namespace;
        let list = SecretList {
            secrets: vec![
                SecretInfo {
                    name: "db-password".into(),
                    namespace: "default".into(),
                    created_at: "2024-01-15T10:30:00Z".into(),
                    updated_at: "2024-01-15T10:30:00Z".into(),
                },
                SecretInfo {
                    name: "api-key".into(),
                    namespace: "default".into(),
                    created_at: "2024-01-14T08:00:00Z".into(),
                    updated_at: "2024-01-20T12:00:00Z".into(),
                },
            ],
        };

        format.write(out, &list)?;
        Ok(())
    }

    async fn get<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        name: &str,
        namespace: Option<&str>,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and fetch secret
        let _ = namespace;
        let detail = SecretDetail {
            name: name.to_string(),
            namespace: "default".into(),
            created_at: "2024-01-15T10:30:00Z".into(),
            updated_at: "2024-01-15T10:30:00Z".into(),
            version: 1,
            // Value is masked for security
            value_masked: "********".into(),
        };

        format.write(out, &detail)?;
        Ok(())
    }

    async fn set<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        name: &str,
        value: Option<&str>,
        file: Option<&str>,
        namespace: Option<&str>,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and set secret
        let _ = (value, file, namespace);
        let response = SecretResponse {
            success: true,
            name: name.to_string(),
            message: format!("Secret '{}' set successfully", name),
        };

        format.write(out, &response)?;
        Ok(())
    }

    async fn delete<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        name: &str,
        namespace: Option<&str>,
        yes: bool,
    ) -> Result<(), CliError> {
        // TODO: Add confirmation prompt if !yes
        let _ = (namespace, yes);
        let response = SecretResponse {
            success: true,
            name: name.to_string(),
            message: format!("Secret '{}' deleted successfully", name),
        };

        format.write(out, &response)?;
        Ok(())
    }

    async fn rotate<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        name: &str,
        namespace: Option<&str>,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and rotate secret
        let _ = namespace;
        let response = SecretResponse {
            success: true,
            name: name.to_string(),
            message: format!("Secret '{}' rotated successfully (version 2)", name),
        };

        format.write(out, &response)?;
        Ok(())
    }
}

// Output types

/// List of secrets.
#[derive(Debug, Clone, Serialize)]
pub struct SecretList {
    /// Secrets.
    pub secrets: Vec<SecretInfo>,
}

/// Secret information for listing.
#[derive(Debug, Clone, Serialize)]
pub struct SecretInfo {
    /// Secret name.
    pub name: String,
    /// Namespace.
    pub namespace: String,
    /// Creation timestamp.
    pub created_at: String,
    /// Last update timestamp.
    pub updated_at: String,
}

/// Detailed secret information.
#[derive(Debug, Clone, Serialize)]
pub struct SecretDetail {
    /// Secret name.
    pub name: String,
    /// Namespace.
    pub namespace: String,
    /// Creation timestamp.
    pub created_at: String,
    /// Last update timestamp.
    pub updated_at: String,
    /// Secret version.
    pub version: u32,
    /// Masked value.
    pub value_masked: String,
}

/// Secret operation response.
#[derive(Debug, Clone, Serialize)]
pub struct SecretResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Secret name.
    pub name: String,
    /// Response message.
    pub message: String,
}

impl TableDisplay for SecretList {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.secrets.is_empty() {
            writeln!(writer, "No secrets found")?;
            return Ok(());
        }

        writeln!(
            writer,
            "{:<24}  {:<16}  {:<24}  {:<24}",
            "NAME", "NAMESPACE", "CREATED", "UPDATED"
        )?;
        writeln!(writer, "{}", "─".repeat(96))?;

        for secret in &self.secrets {
            writeln!(
                writer,
                "{:<24}  {:<16}  {:<24}  {:<24}",
                secret.name, secret.namespace, secret.created_at, secret.updated_at
            )?;
        }

        writeln!(writer)?;
        writeln!(writer, "Total: {} secret(s)", self.secrets.len())?;
        Ok(())
    }
}

impl TableDisplay for SecretDetail {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        writeln!(writer, "Secret: {}", self.name)?;
        writeln!(writer, "══════════════════════════════════")?;
        writeln!(writer)?;
        writeln!(writer, "Namespace:    {}", self.namespace)?;
        writeln!(writer, "Version:      {}", self.version)?;
        writeln!(writer, "Created:      {}", self.created_at)?;
        writeln!(writer, "Updated:      {}", self.updated_at)?;
        writeln!(writer, "Value:        {}", self.value_masked)?;
        Ok(())
    }
}

impl TableDisplay for SecretResponse {
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
    fn secret_command_new() {
        let cmd = SecretCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[test]
    fn secret_list_table_output() {
        let list = SecretList {
            secrets: vec![SecretInfo {
                name: "my-secret".into(),
                namespace: "default".into(),
                created_at: "2024-01-15T10:30:00Z".into(),
                updated_at: "2024-01-15T10:30:00Z".into(),
            }],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("my-secret"));
        assert!(output.contains("default"));
        assert!(output.contains("Total: 1 secret(s)"));
    }

    #[test]
    fn secret_list_empty() {
        let list = SecretList { secrets: vec![] };
        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("No secrets found"));
    }

    #[test]
    fn secret_detail_table_output() {
        let detail = SecretDetail {
            name: "db-password".into(),
            namespace: "production".into(),
            created_at: "2024-01-15T10:30:00Z".into(),
            updated_at: "2024-01-20T12:00:00Z".into(),
            version: 3,
            value_masked: "********".into(),
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&detail).expect("should format");

        assert!(output.contains("Secret: db-password"));
        assert!(output.contains("Version:      3"));
        assert!(output.contains("********"));
    }

    #[test]
    fn secret_response_success() {
        let response = SecretResponse {
            success: true,
            name: "test".into(),
            message: "Secret set successfully".into(),
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&response).expect("should format");

        assert!(output.contains("✓"));
        assert!(output.contains("Secret set successfully"));
    }

    #[test]
    fn secret_list_json_output() {
        let list = SecretList {
            secrets: vec![SecretInfo {
                name: "my-secret".into(),
                namespace: "default".into(),
                created_at: "2024-01-15T10:30:00Z".into(),
                updated_at: "2024-01-15T10:30:00Z".into(),
            }],
        };

        let fmt = OutputFormat::new(Format::Json);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("\"name\": \"my-secret\""));
        assert!(output.contains("\"namespace\": \"default\""));
    }
}
