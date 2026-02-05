//! Authentication command implementation.
//!
//! Handles login, logout, whoami, and API key management.

use std::io::Write;

use serde::Serialize;

use crate::cli::{ApikeyCommands, AuthCommands};
use crate::error::CliError;
use crate::output::{OutputFormat, TableDisplay};

/// Handler for auth subcommands.
pub struct AuthCommand<'a> {
    #[allow(dead_code)]
    gateway_url: &'a str,
}

impl<'a> AuthCommand<'a> {
    /// Creates a new auth command handler.
    #[must_use]
    pub const fn new(gateway_url: &'a str) -> Self {
        Self { gateway_url }
    }

    /// Executes the auth subcommand.
    ///
    /// # Errors
    ///
    /// Returns error if the command fails.
    pub async fn execute<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        command: &AuthCommands,
    ) -> Result<(), CliError> {
        match command {
            AuthCommands::Login {
                username,
                password,
                token,
            } => {
                self.login(out, format, username.as_deref(), password.as_deref(), token.as_deref())
                    .await
            }
            AuthCommands::Logout => self.logout(out, format).await,
            AuthCommands::Whoami => self.whoami(out, format).await,
            AuthCommands::Apikey { command } => self.apikey(out, format, command).await,
        }
    }

    async fn login<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        username: Option<&str>,
        _password: Option<&str>,
        token: Option<&str>,
    ) -> Result<(), CliError> {
        // TODO: Implement real authentication
        let response = if token.is_some() {
            AuthResponse {
                success: true,
                message: "Logged in with token".into(),
                username: Some("token-user".into()),
            }
        } else {
            AuthResponse {
                success: true,
                message: format!(
                    "Logged in as {}",
                    username.unwrap_or("interactive-user")
                ),
                username: Some(username.unwrap_or("interactive-user").into()),
            }
        };

        format.write(out, &response)?;
        Ok(())
    }

    async fn logout<W: Write>(&self, out: &mut W, format: &OutputFormat) -> Result<(), CliError> {
        // TODO: Clear credentials
        let response = AuthResponse {
            success: true,
            message: "Logged out successfully".into(),
            username: None,
        };

        format.write(out, &response)?;
        Ok(())
    }

    async fn whoami<W: Write>(&self, out: &mut W, format: &OutputFormat) -> Result<(), CliError> {
        // TODO: Read current credentials
        let info = WhoamiInfo {
            username: "admin".into(),
            email: Some("admin@example.com".into()),
            roles: vec!["cluster-admin".into(), "developer".into()],
            tenant: Some("default".into()),
            expires_at: Some("2024-12-31T23:59:59Z".into()),
        };

        format.write(out, &info)?;
        Ok(())
    }

    async fn apikey<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        command: &ApikeyCommands,
    ) -> Result<(), CliError> {
        match command {
            ApikeyCommands::Create {
                name,
                expires,
                scopes,
            } => self.apikey_create(out, format, name, *expires, scopes).await,
            ApikeyCommands::List => self.apikey_list(out, format).await,
            ApikeyCommands::Revoke { id } => self.apikey_revoke(out, format, id).await,
        }
    }

    async fn apikey_create<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        name: &str,
        expires: u32,
        scopes: &[String],
    ) -> Result<(), CliError> {
        // TODO: Create API key via gateway
        let response = ApikeyCreateResponse {
            id: "ak_a1b2c3d4e5f6".into(),
            name: name.to_string(),
            key: "clwk_1234567890abcdef1234567890abcdef".into(),
            scopes: scopes.to_vec(),
            expires_in_days: expires,
            warning: Some("Save this key securely. You won't be able to see it again.".into()),
        };

        format.write(out, &response)?;
        Ok(())
    }

    async fn apikey_list<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
    ) -> Result<(), CliError> {
        // TODO: List API keys via gateway
        let list = ApikeyList {
            keys: vec![
                ApikeyInfo {
                    id: "ak_a1b2c3d4".into(),
                    name: "ci-deploy".into(),
                    scopes: vec!["deploy".into(), "read".into()],
                    created_at: "2024-01-15T10:30:00Z".into(),
                    expires_at: Some("2024-04-15T10:30:00Z".into()),
                    last_used: Some("2024-01-20T08:00:00Z".into()),
                },
                ApikeyInfo {
                    id: "ak_e5f6g7h8".into(),
                    name: "monitoring".into(),
                    scopes: vec!["read".into()],
                    created_at: "2024-01-10T14:00:00Z".into(),
                    expires_at: None,
                    last_used: Some("2024-01-22T12:30:00Z".into()),
                },
            ],
        };

        format.write(out, &list)?;
        Ok(())
    }

    async fn apikey_revoke<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        id: &str,
    ) -> Result<(), CliError> {
        // TODO: Revoke API key via gateway
        let response = AuthResponse {
            success: true,
            message: format!("API key '{}' revoked successfully", id),
            username: None,
        };

        format.write(out, &response)?;
        Ok(())
    }
}

// Output types

/// Authentication response.
#[derive(Debug, Clone, Serialize)]
pub struct AuthResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Response message.
    pub message: String,
    /// Username (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

/// Current user information.
#[derive(Debug, Clone, Serialize)]
pub struct WhoamiInfo {
    /// Username.
    pub username: String,
    /// Email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// User roles.
    pub roles: Vec<String>,
    /// Tenant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// Token expiration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// API key creation response.
#[derive(Debug, Clone, Serialize)]
pub struct ApikeyCreateResponse {
    /// Key ID.
    pub id: String,
    /// Key name.
    pub name: String,
    /// The actual API key (only shown once).
    pub key: String,
    /// Key scopes.
    pub scopes: Vec<String>,
    /// Expiration in days.
    pub expires_in_days: u32,
    /// Warning message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// API key information.
#[derive(Debug, Clone, Serialize)]
pub struct ApikeyInfo {
    /// Key ID.
    pub id: String,
    /// Key name.
    pub name: String,
    /// Key scopes.
    pub scopes: Vec<String>,
    /// Creation timestamp.
    pub created_at: String,
    /// Expiration timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// Last used timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<String>,
}

/// List of API keys.
#[derive(Debug, Clone, Serialize)]
pub struct ApikeyList {
    /// API keys.
    pub keys: Vec<ApikeyInfo>,
}

impl TableDisplay for AuthResponse {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.success {
            writeln!(writer, "✓ {}", self.message)?;
        } else {
            writeln!(writer, "✗ {}", self.message)?;
        }
        Ok(())
    }
}

impl TableDisplay for WhoamiInfo {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        writeln!(writer, "Current User")?;
        writeln!(writer, "══════════════════════════════════")?;
        writeln!(writer)?;
        writeln!(writer, "Username:     {}", self.username)?;
        if let Some(ref email) = self.email {
            writeln!(writer, "Email:        {}", email)?;
        }
        writeln!(writer, "Roles:        {}", self.roles.join(", "))?;
        if let Some(ref tenant) = self.tenant {
            writeln!(writer, "Tenant:       {}", tenant)?;
        }
        if let Some(ref expires) = self.expires_at {
            writeln!(writer, "Expires:      {}", expires)?;
        }
        Ok(())
    }
}

impl TableDisplay for ApikeyCreateResponse {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        writeln!(writer, "API Key Created")?;
        writeln!(writer, "══════════════════════════════════")?;
        writeln!(writer)?;
        writeln!(writer, "ID:           {}", self.id)?;
        writeln!(writer, "Name:         {}", self.name)?;
        writeln!(writer, "Key:          {}", self.key)?;
        writeln!(writer, "Scopes:       {}", self.scopes.join(", "))?;
        if self.expires_in_days > 0 {
            writeln!(writer, "Expires:      {} days", self.expires_in_days)?;
        } else {
            writeln!(writer, "Expires:      Never")?;
        }
        if let Some(ref warning) = self.warning {
            writeln!(writer)?;
            writeln!(writer, "⚠ {}", warning)?;
        }
        Ok(())
    }
}

impl TableDisplay for ApikeyList {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.keys.is_empty() {
            writeln!(writer, "No API keys found")?;
            return Ok(());
        }

        writeln!(
            writer,
            "{:<16}  {:<16}  {:<24}  {:<24}  {}",
            "ID", "NAME", "CREATED", "EXPIRES", "SCOPES"
        )?;
        writeln!(writer, "{}", "─".repeat(100))?;

        for key in &self.keys {
            writeln!(
                writer,
                "{:<16}  {:<16}  {:<24}  {:<24}  {}",
                key.id,
                key.name,
                key.created_at,
                key.expires_at.as_deref().unwrap_or("Never"),
                key.scopes.join(",")
            )?;
        }

        writeln!(writer)?;
        writeln!(writer, "Total: {} key(s)", self.keys.len())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Format;

    #[test]
    fn auth_command_new() {
        let cmd = AuthCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[test]
    fn whoami_table_output() {
        let info = WhoamiInfo {
            username: "testuser".into(),
            email: Some("test@example.com".into()),
            roles: vec!["admin".into()],
            tenant: Some("default".into()),
            expires_at: None,
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&info).expect("should format");

        assert!(output.contains("Username:     testuser"));
        assert!(output.contains("Email:        test@example.com"));
        assert!(output.contains("Roles:        admin"));
    }

    #[test]
    fn apikey_list_table_output() {
        let list = ApikeyList {
            keys: vec![ApikeyInfo {
                id: "ak_test".into(),
                name: "test-key".into(),
                scopes: vec!["read".into()],
                created_at: "2024-01-15T10:30:00Z".into(),
                expires_at: None,
                last_used: None,
            }],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("ak_test"));
        assert!(output.contains("test-key"));
        assert!(output.contains("Total: 1 key(s)"));
    }

    #[test]
    fn apikey_create_response() {
        let response = ApikeyCreateResponse {
            id: "ak_123".into(),
            name: "my-key".into(),
            key: "clwk_secret".into(),
            scopes: vec!["deploy".into()],
            expires_in_days: 90,
            warning: Some("Save this key!".into()),
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&response).expect("should format");

        assert!(output.contains("API Key Created"));
        assert!(output.contains("clwk_secret"));
        assert!(output.contains("90 days"));
        assert!(output.contains("Save this key!"));
    }

    #[test]
    fn auth_response_json() {
        let response = AuthResponse {
            success: true,
            message: "Logged in".into(),
            username: Some("admin".into()),
        };

        let fmt = OutputFormat::new(Format::Json);
        let output = fmt.to_string(&response).expect("should format");

        assert!(output.contains("\"success\": true"));
        assert!(output.contains("\"username\": \"admin\""));
    }
}
