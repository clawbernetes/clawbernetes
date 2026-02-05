//! Dashboard command implementation.
//!
//! Handles starting and managing the web dashboard.

use std::io::Write;

use serde::Serialize;

use crate::cli::DashboardCommands;
use crate::error::CliError;
use crate::output::{OutputFormat, TableDisplay};

/// Handler for dashboard subcommands.
pub struct DashboardCommand<'a> {
    #[allow(dead_code)]
    gateway_url: &'a str,
}

impl<'a> DashboardCommand<'a> {
    /// Creates a new dashboard command handler.
    #[must_use]
    pub const fn new(gateway_url: &'a str) -> Self {
        Self { gateway_url }
    }

    /// Executes the dashboard subcommand.
    ///
    /// # Errors
    ///
    /// Returns error if the command fails.
    pub async fn execute<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        command: &DashboardCommands,
    ) -> Result<(), CliError> {
        match command {
            DashboardCommands::Start { port, bind, open } => {
                self.start(out, format, *port, bind, *open).await
            }
            DashboardCommands::Url => self.url(out, format).await,
        }
    }

    async fn start<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        port: u16,
        bind: &str,
        open: bool,
    ) -> Result<(), CliError> {
        // TODO: Start the dashboard server
        let response = DashboardStartResponse {
            success: true,
            url: format!("http://{}:{}", bind, port),
            port,
            bind: bind.to_string(),
            message: format!("Dashboard started at http://{}:{}", bind, port),
            browser_opened: open,
        };

        format.write(out, &response)?;

        // In a real implementation, this would block and serve the dashboard
        writeln!(out)?;
        writeln!(out, "(Dashboard server not implemented in placeholder)")?;
        writeln!(out, "Press Ctrl+C to stop")?;

        Ok(())
    }

    async fn url<W: Write>(&self, out: &mut W, format: &OutputFormat) -> Result<(), CliError> {
        // TODO: Check if dashboard is running and return URL
        let response = DashboardUrlResponse {
            running: true,
            url: Some("http://127.0.0.1:8080".into()),
        };

        format.write(out, &response)?;
        Ok(())
    }
}

// Output types

/// Dashboard start response.
#[derive(Debug, Clone, Serialize)]
pub struct DashboardStartResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Dashboard URL.
    pub url: String,
    /// Port number.
    pub port: u16,
    /// Bind address.
    pub bind: String,
    /// Response message.
    pub message: String,
    /// Whether browser was opened.
    pub browser_opened: bool,
}

/// Dashboard URL response.
#[derive(Debug, Clone, Serialize)]
pub struct DashboardUrlResponse {
    /// Whether dashboard is running.
    pub running: bool,
    /// Dashboard URL (if running).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl TableDisplay for DashboardStartResponse {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        writeln!(writer, "Dashboard")?;
        writeln!(writer, "══════════════════════════════════")?;
        writeln!(writer)?;

        if self.success {
            writeln!(writer, "✓ {}", self.message)?;
        } else {
            writeln!(writer, "✗ {}", self.message)?;
        }
        writeln!(writer)?;

        writeln!(writer, "URL:      {}", self.url)?;
        writeln!(writer, "Bind:     {}", self.bind)?;
        writeln!(writer, "Port:     {}", self.port)?;
        if self.browser_opened {
            writeln!(writer, "Browser:  Opened")?;
        }
        Ok(())
    }
}

impl TableDisplay for DashboardUrlResponse {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.running {
            if let Some(ref url) = self.url {
                writeln!(writer, "Dashboard is running at: {}", url)?;
            } else {
                writeln!(writer, "Dashboard is running")?;
            }
        } else {
            writeln!(writer, "Dashboard is not running")?;
            writeln!(writer)?;
            writeln!(writer, "Start it with: clawbernetes dashboard start")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Format;

    #[test]
    fn dashboard_command_new() {
        let cmd = DashboardCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[test]
    fn dashboard_start_response_table() {
        let response = DashboardStartResponse {
            success: true,
            url: "http://127.0.0.1:8080".into(),
            port: 8080,
            bind: "127.0.0.1".into(),
            message: "Dashboard started".into(),
            browser_opened: true,
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&response).expect("should format");

        assert!(output.contains("Dashboard started"));
        assert!(output.contains("URL:      http://127.0.0.1:8080"));
        assert!(output.contains("Browser:  Opened"));
    }

    #[test]
    fn dashboard_url_response_running() {
        let response = DashboardUrlResponse {
            running: true,
            url: Some("http://localhost:9000".into()),
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&response).expect("should format");

        assert!(output.contains("Dashboard is running at: http://localhost:9000"));
    }

    #[test]
    fn dashboard_url_response_not_running() {
        let response = DashboardUrlResponse {
            running: false,
            url: None,
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&response).expect("should format");

        assert!(output.contains("Dashboard is not running"));
        assert!(output.contains("clawbernetes dashboard start"));
    }

    #[test]
    fn dashboard_start_response_json() {
        let response = DashboardStartResponse {
            success: true,
            url: "http://127.0.0.1:8080".into(),
            port: 8080,
            bind: "127.0.0.1".into(),
            message: "ok".into(),
            browser_opened: false,
        };

        let fmt = OutputFormat::new(Format::Json);
        let output = fmt.to_string(&response).expect("should format");

        assert!(output.contains("\"success\": true"));
        assert!(output.contains("\"port\": 8080"));
    }
}
