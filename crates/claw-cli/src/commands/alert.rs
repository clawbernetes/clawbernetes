//! Alert management command implementation.
//!
//! Handles alert listing, creation, deletion, and silencing.

use std::io::Write;

use serde::Serialize;

use crate::cli::{AlertCommands, CreateAlertArgs};
use crate::error::CliError;
use crate::output::{OutputFormat, TableDisplay};

/// Handler for alert subcommands.
pub struct AlertCommand<'a> {
    #[allow(dead_code)]
    gateway_url: &'a str,
}

impl<'a> AlertCommand<'a> {
    /// Creates a new alert command handler.
    #[must_use]
    pub const fn new(gateway_url: &'a str) -> Self {
        Self { gateway_url }
    }

    /// Executes the alert subcommand.
    ///
    /// # Errors
    ///
    /// Returns error if the command fails.
    pub async fn execute<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        command: &AlertCommands,
    ) -> Result<(), CliError> {
        match command {
            AlertCommands::List { state, severity } => {
                self.list(out, format, state.as_deref(), severity.as_deref())
                    .await
            }
            AlertCommands::Create(args) => self.create(out, format, args).await,
            AlertCommands::Delete { name, yes } => self.delete(out, format, name, *yes).await,
            AlertCommands::Silence {
                matcher,
                duration,
                comment,
            } => {
                self.silence(out, format, matcher, duration, comment.as_deref())
                    .await
            }
            AlertCommands::Unsilence { id } => self.unsilence(out, format, id).await,
        }
    }

    async fn list<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        state: Option<&str>,
        severity: Option<&str>,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and fetch alerts
        let _ = (state, severity);
        let list = AlertList {
            alerts: vec![
                AlertInfo {
                    name: "HighGpuUtilization".into(),
                    state: "firing".into(),
                    severity: "warning".into(),
                    summary: "GPU utilization above 90%".into(),
                    active_since: Some("2024-01-22T10:30:00Z".into()),
                    silenced: false,
                },
                AlertInfo {
                    name: "NodeDiskFull".into(),
                    state: "pending".into(),
                    severity: "critical".into(),
                    summary: "Node disk usage above 85%".into(),
                    active_since: Some("2024-01-22T14:00:00Z".into()),
                    silenced: false,
                },
                AlertInfo {
                    name: "HighMemoryUsage".into(),
                    state: "firing".into(),
                    severity: "warning".into(),
                    summary: "Memory usage above 80%".into(),
                    active_since: Some("2024-01-21T08:00:00Z".into()),
                    silenced: true,
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
        args: &CreateAlertArgs,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and create alert
        let response = AlertResponse {
            success: true,
            name: args.name.clone(),
            message: format!("Alert rule '{}' created successfully", args.name),
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
        let response = AlertResponse {
            success: true,
            name: name.to_string(),
            message: format!("Alert rule '{}' deleted successfully", name),
        };

        format.write(out, &response)?;
        Ok(())
    }

    async fn silence<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        matcher: &str,
        duration: &str,
        comment: Option<&str>,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and create silence
        let response = SilenceResponse {
            success: true,
            silence_id: "sil_abc123".into(),
            matcher: matcher.to_string(),
            duration: duration.to_string(),
            comment: comment.map(String::from),
            message: format!("Silence created for '{}'", matcher),
        };

        format.write(out, &response)?;
        Ok(())
    }

    async fn unsilence<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        id: &str,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and remove silence
        let response = AlertResponse {
            success: true,
            name: id.to_string(),
            message: format!("Silence '{}' removed successfully", id),
        };

        format.write(out, &response)?;
        Ok(())
    }
}

// Output types

/// List of alerts.
#[derive(Debug, Clone, Serialize)]
pub struct AlertList {
    /// Alerts.
    pub alerts: Vec<AlertInfo>,
}

/// Alert information.
#[derive(Debug, Clone, Serialize)]
pub struct AlertInfo {
    /// Alert name.
    pub name: String,
    /// Alert state (firing, pending, resolved).
    pub state: String,
    /// Severity level.
    pub severity: String,
    /// Summary message.
    pub summary: String,
    /// When the alert became active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_since: Option<String>,
    /// Whether the alert is silenced.
    pub silenced: bool,
}

/// Alert operation response.
#[derive(Debug, Clone, Serialize)]
pub struct AlertResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Alert name.
    pub name: String,
    /// Response message.
    pub message: String,
}

/// Silence creation response.
#[derive(Debug, Clone, Serialize)]
pub struct SilenceResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Silence ID.
    pub silence_id: String,
    /// Matcher pattern.
    pub matcher: String,
    /// Duration.
    pub duration: String,
    /// Comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Response message.
    pub message: String,
}

impl TableDisplay for AlertList {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.alerts.is_empty() {
            writeln!(writer, "No alerts found")?;
            return Ok(());
        }

        writeln!(
            writer,
            "{:<24}  {:<10}  {:<10}  {:<8}  {}",
            "NAME", "STATE", "SEVERITY", "SILENCED", "SUMMARY"
        )?;
        writeln!(writer, "{}", "─".repeat(100))?;

        for alert in &self.alerts {
            let state_icon = match alert.state.as_str() {
                "firing" => "🔴",
                "pending" => "🟡",
                "resolved" => "🟢",
                _ => "⚪",
            };
            writeln!(
                writer,
                "{:<24}  {} {:<7}  {:<10}  {:<8}  {}",
                alert.name,
                state_icon,
                alert.state,
                alert.severity,
                if alert.silenced { "yes" } else { "no" },
                truncate(&alert.summary, 40)
            )?;
        }

        let firing = self.alerts.iter().filter(|a| a.state == "firing").count();
        let pending = self.alerts.iter().filter(|a| a.state == "pending").count();
        writeln!(writer)?;
        writeln!(
            writer,
            "Total: {} alert(s) ({} firing, {} pending)",
            self.alerts.len(),
            firing,
            pending
        )?;
        Ok(())
    }
}

impl TableDisplay for AlertResponse {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.success {
            writeln!(writer, "✓ {}", self.message)?;
        } else {
            writeln!(writer, "✗ {}", self.message)?;
        }
        Ok(())
    }
}

impl TableDisplay for SilenceResponse {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        writeln!(writer, "Silence Created")?;
        writeln!(writer, "══════════════════════════════════")?;
        writeln!(writer)?;
        writeln!(writer, "ID:           {}", self.silence_id)?;
        writeln!(writer, "Matcher:      {}", self.matcher)?;
        writeln!(writer, "Duration:     {}", self.duration)?;
        if let Some(ref comment) = self.comment {
            writeln!(writer, "Comment:      {}", comment)?;
        }
        Ok(())
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len > 3 {
        format!("{}...", &s[..max_len - 3])
    } else {
        s[..max_len].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Format;

    #[test]
    fn alert_command_new() {
        let cmd = AlertCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[test]
    fn alert_list_table_output() {
        let list = AlertList {
            alerts: vec![AlertInfo {
                name: "TestAlert".into(),
                state: "firing".into(),
                severity: "critical".into(),
                summary: "Test alert summary".into(),
                active_since: Some("2024-01-22T10:30:00Z".into()),
                silenced: false,
            }],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("TestAlert"));
        assert!(output.contains("firing"));
        assert!(output.contains("critical"));
        assert!(output.contains("Total: 1 alert(s)"));
    }

    #[test]
    fn alert_list_empty() {
        let list = AlertList { alerts: vec![] };
        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("No alerts found"));
    }

    #[test]
    fn silence_response_table() {
        let response = SilenceResponse {
            success: true,
            silence_id: "sil_123".into(),
            matcher: "alertname=TestAlert".into(),
            duration: "2h".into(),
            comment: Some("Scheduled maintenance".into()),
            message: "Silence created".into(),
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&response).expect("should format");

        assert!(output.contains("sil_123"));
        assert!(output.contains("2h"));
        assert!(output.contains("Scheduled maintenance"));
    }

    #[test]
    fn alert_list_json() {
        let list = AlertList {
            alerts: vec![AlertInfo {
                name: "TestAlert".into(),
                state: "firing".into(),
                severity: "critical".into(),
                summary: "Test".into(),
                active_since: None,
                silenced: true,
            }],
        };

        let fmt = OutputFormat::new(Format::Json);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("\"name\": \"TestAlert\""));
        assert!(output.contains("\"silenced\": true"));
    }
}
