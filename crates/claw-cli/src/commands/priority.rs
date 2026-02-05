//! Priority management command implementation.
//!
//! Handles workload priority listing and setting.

use std::io::Write;

use serde::Serialize;

use crate::cli::PriorityCommands;
use crate::error::CliError;
use crate::output::{OutputFormat, TableDisplay};

/// Handler for priority subcommands.
pub struct PriorityCommand<'a> {
    #[allow(dead_code)]
    gateway_url: &'a str,
}

impl<'a> PriorityCommand<'a> {
    /// Creates a new priority command handler.
    #[must_use]
    pub const fn new(gateway_url: &'a str) -> Self {
        Self { gateway_url }
    }

    /// Executes the priority subcommand.
    ///
    /// # Errors
    ///
    /// Returns error if the command fails.
    pub async fn execute<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        command: &PriorityCommands,
    ) -> Result<(), CliError> {
        match command {
            PriorityCommands::List => self.list(out, format).await,
            PriorityCommands::Set {
                workload,
                priority,
                namespace,
            } => {
                self.set(out, format, workload, *priority, namespace.as_deref())
                    .await
            }
            PriorityCommands::Get { workload, namespace } => {
                self.get(out, format, workload, namespace.as_deref()).await
            }
        }
    }

    async fn list<W: Write>(&self, out: &mut W, format: &OutputFormat) -> Result<(), CliError> {
        // TODO: Connect to gateway and fetch priority classes
        let list = PriorityClassList {
            classes: vec![
                PriorityClass {
                    name: "system-critical".into(),
                    value: 1000,
                    description: "System-critical workloads that must not be preempted".into(),
                    preemption_policy: "Never".into(),
                },
                PriorityClass {
                    name: "high".into(),
                    value: 750,
                    description: "High priority production workloads".into(),
                    preemption_policy: "PreemptLowerPriority".into(),
                },
                PriorityClass {
                    name: "normal".into(),
                    value: 500,
                    description: "Default priority for most workloads".into(),
                    preemption_policy: "PreemptLowerPriority".into(),
                },
                PriorityClass {
                    name: "low".into(),
                    value: 250,
                    description: "Low priority batch jobs".into(),
                    preemption_policy: "PreemptLowerPriority".into(),
                },
                PriorityClass {
                    name: "background".into(),
                    value: 100,
                    description: "Background tasks that can be preempted any time".into(),
                    preemption_policy: "PreemptLowerPriority".into(),
                },
            ],
        };

        format.write(out, &list)?;
        Ok(())
    }

    async fn set<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        workload: &str,
        priority: u32,
        namespace: Option<&str>,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and set priority
        let _ = namespace;
        let response = PrioritySetResponse {
            success: true,
            workload: workload.to_string(),
            namespace: namespace.unwrap_or("default").to_string(),
            old_priority: Some(500),
            new_priority: priority,
            message: format!("Priority for '{}' set to {}", workload, priority),
        };

        format.write(out, &response)?;
        Ok(())
    }

    async fn get<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        workload: &str,
        namespace: Option<&str>,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and get priority
        let _ = namespace;
        let info = WorkloadPriority {
            workload: workload.to_string(),
            namespace: namespace.unwrap_or("default").to_string(),
            priority: 500,
            priority_class: Some("normal".into()),
            preemptible: true,
        };

        format.write(out, &info)?;
        Ok(())
    }
}

// Output types

/// List of priority classes.
#[derive(Debug, Clone, Serialize)]
pub struct PriorityClassList {
    /// Priority classes.
    pub classes: Vec<PriorityClass>,
}

/// Priority class definition.
#[derive(Debug, Clone, Serialize)]
pub struct PriorityClass {
    /// Class name.
    pub name: String,
    /// Priority value.
    pub value: u32,
    /// Description.
    pub description: String,
    /// Preemption policy.
    pub preemption_policy: String,
}

/// Priority set response.
#[derive(Debug, Clone, Serialize)]
pub struct PrioritySetResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Workload name.
    pub workload: String,
    /// Namespace.
    pub namespace: String,
    /// Previous priority.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_priority: Option<u32>,
    /// New priority.
    pub new_priority: u32,
    /// Response message.
    pub message: String,
}

/// Workload priority information.
#[derive(Debug, Clone, Serialize)]
pub struct WorkloadPriority {
    /// Workload name.
    pub workload: String,
    /// Namespace.
    pub namespace: String,
    /// Priority value.
    pub priority: u32,
    /// Priority class name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority_class: Option<String>,
    /// Whether this workload can be preempted.
    pub preemptible: bool,
}

impl TableDisplay for PriorityClassList {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.classes.is_empty() {
            writeln!(writer, "No priority classes defined")?;
            return Ok(());
        }

        writeln!(writer, "Priority Classes")?;
        writeln!(writer, "══════════════════════════════════════════════════════════════════════")?;
        writeln!(writer)?;
        writeln!(
            writer,
            "{:<20}  {:>6}  {:<24}  {}",
            "NAME", "VALUE", "PREEMPTION POLICY", "DESCRIPTION"
        )?;
        writeln!(writer, "{}", "─".repeat(80))?;

        for class in &self.classes {
            writeln!(
                writer,
                "{:<20}  {:>6}  {:<24}  {}",
                class.name,
                class.value,
                class.preemption_policy,
                truncate(&class.description, 30)
            )?;
        }

        writeln!(writer)?;
        writeln!(writer, "Total: {} class(es)", self.classes.len())?;
        Ok(())
    }
}

impl TableDisplay for PrioritySetResponse {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.success {
            writeln!(writer, "✓ {}", self.message)?;
        } else {
            writeln!(writer, "✗ {}", self.message)?;
        }
        writeln!(writer)?;

        writeln!(writer, "Workload:   {}", self.workload)?;
        writeln!(writer, "Namespace:  {}", self.namespace)?;
        if let Some(old) = self.old_priority {
            writeln!(writer, "Priority:   {} → {}", old, self.new_priority)?;
        } else {
            writeln!(writer, "Priority:   {}", self.new_priority)?;
        }
        Ok(())
    }
}

impl TableDisplay for WorkloadPriority {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        writeln!(writer, "Workload Priority")?;
        writeln!(writer, "══════════════════════════════════")?;
        writeln!(writer)?;
        writeln!(writer, "Workload:     {}", self.workload)?;
        writeln!(writer, "Namespace:    {}", self.namespace)?;
        writeln!(writer, "Priority:     {}", self.priority)?;
        if let Some(ref class) = self.priority_class {
            writeln!(writer, "Class:        {}", class)?;
        }
        writeln!(
            writer,
            "Preemptible:  {}",
            if self.preemptible { "yes" } else { "no" }
        )?;
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
    fn priority_command_new() {
        let cmd = PriorityCommand::new("ws://localhost:8080");
        assert_eq!(cmd.gateway_url, "ws://localhost:8080");
    }

    #[test]
    fn priority_class_list_table_output() {
        let list = PriorityClassList {
            classes: vec![
                PriorityClass {
                    name: "high".into(),
                    value: 750,
                    description: "High priority".into(),
                    preemption_policy: "PreemptLowerPriority".into(),
                },
                PriorityClass {
                    name: "low".into(),
                    value: 250,
                    description: "Low priority".into(),
                    preemption_policy: "PreemptLowerPriority".into(),
                },
            ],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("Priority Classes"));
        assert!(output.contains("high"));
        assert!(output.contains("750"));
        assert!(output.contains("Total: 2 class(es)"));
    }

    #[test]
    fn priority_class_list_empty() {
        let list = PriorityClassList { classes: vec![] };
        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("No priority classes defined"));
    }

    #[test]
    fn priority_set_response_table() {
        let response = PrioritySetResponse {
            success: true,
            workload: "my-job".into(),
            namespace: "default".into(),
            old_priority: Some(500),
            new_priority: 750,
            message: "Priority updated".into(),
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&response).expect("should format");

        assert!(output.contains("Priority updated"));
        assert!(output.contains("500 → 750"));
    }

    #[test]
    fn workload_priority_table() {
        let info = WorkloadPriority {
            workload: "training".into(),
            namespace: "ml".into(),
            priority: 750,
            priority_class: Some("high".into()),
            preemptible: false,
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&info).expect("should format");

        assert!(output.contains("Workload Priority"));
        assert!(output.contains("Priority:     750"));
        assert!(output.contains("Class:        high"));
        assert!(output.contains("Preemptible:  no"));
    }

    #[test]
    fn priority_class_list_json() {
        let list = PriorityClassList {
            classes: vec![PriorityClass {
                name: "test".into(),
                value: 500,
                description: "Test".into(),
                preemption_policy: "Never".into(),
            }],
        };

        let fmt = OutputFormat::new(Format::Json);
        let output = fmt.to_string(&list).expect("should format");

        assert!(output.contains("\"name\": \"test\""));
        assert!(output.contains("\"value\": 500"));
    }
}
