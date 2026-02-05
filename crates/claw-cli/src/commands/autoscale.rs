//! Autoscale command implementation.
//!
//! Handles autoscaling management commands.

use std::io::Write;

use crate::cli::AutoscaleCommands;
use crate::error::CliError;
use crate::output::{OutputFormat, TableDisplay};

/// Handler for autoscale subcommands.
pub struct AutoscaleCommand<'a> {
    #[allow(dead_code)]
    gateway_url: &'a str,
}

impl<'a> AutoscaleCommand<'a> {
    /// Creates a new autoscale command handler.
    #[must_use]
    pub const fn new(gateway_url: &'a str) -> Self {
        Self { gateway_url }
    }

    /// Executes the autoscale subcommand.
    ///
    /// # Errors
    ///
    /// Returns error if the command fails.
    pub async fn execute<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        command: &AutoscaleCommands,
    ) -> Result<(), CliError> {
        match command {
            AutoscaleCommands::Status => self.show_status(out, format).await,
            AutoscaleCommands::Pools => self.list_pools(out, format).await,
            AutoscaleCommands::Pool { id } => self.show_pool(out, format, id).await,
            AutoscaleCommands::SetPolicy(args) => self.set_policy(out, format, args).await,
            AutoscaleCommands::Enable => self.set_enabled(out, format, true).await,
            AutoscaleCommands::Disable => self.set_enabled(out, format, false).await,
            AutoscaleCommands::Evaluate => self.evaluate(out, format).await,
        }
    }

    async fn show_status<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and fetch real status
        // For now, return placeholder data
        let status = AutoscalerStatusResponse {
            enabled: true,
            pool_count: 2,
            total_nodes: 10,
            total_gpus: 80,
            last_evaluation: Some("2024-01-15T10:30:00Z".to_string()),
            pending_actions: 1,
        };

        format.output(out, &status)?;
        Ok(())
    }

    async fn list_pools<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and fetch real pool list
        let pools = vec![
            PoolSummary {
                id: "gpu-pool-1".to_string(),
                name: "GPU Pool 1".to_string(),
                node_count: 5,
                gpu_count: 40,
                policy_type: "utilization".to_string(),
                enabled: true,
            },
            PoolSummary {
                id: "gpu-pool-2".to_string(),
                name: "GPU Pool 2".to_string(),
                node_count: 5,
                gpu_count: 40,
                policy_type: "queue_depth".to_string(),
                enabled: true,
            },
        ];

        format.output(out, &pools)?;
        Ok(())
    }

    async fn show_pool<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        id: &str,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and fetch real pool details
        let pool = PoolDetails {
            id: id.to_string(),
            name: format!("Pool {id}"),
            node_count: 5,
            ready_node_count: 5,
            gpu_count: 40,
            policy: PolicyDetails {
                policy_type: "utilization".to_string(),
                min_nodes: 2,
                max_nodes: 20,
                target_utilization: Some(70.0),
                tolerance: Some(10.0),
                scale_up_cooldown_secs: 300,
                scale_down_cooldown_secs: 600,
                enabled: true,
            },
            last_scale_up: None,
            last_scale_down: None,
        };

        format.output(out, &pool)?;
        Ok(())
    }

    async fn set_policy<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        args: &crate::cli::SetScalingPolicyArgs,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and set the policy
        let response = SetPolicyResponse {
            success: true,
            pool_id: args.pool_id.clone(),
            message: format!("Policy updated for pool {}", args.pool_id),
        };

        format.output(out, &response)?;
        Ok(())
    }

    async fn set_enabled<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
        enabled: bool,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and enable/disable autoscaler
        let response = EnableResponse {
            success: true,
            enabled,
            message: if enabled {
                "Autoscaling enabled".to_string()
            } else {
                "Autoscaling disabled".to_string()
            },
        };

        format.output(out, &response)?;
        Ok(())
    }

    async fn evaluate<W: Write>(
        &self,
        out: &mut W,
        format: &OutputFormat,
    ) -> Result<(), CliError> {
        // TODO: Connect to gateway and trigger evaluation
        let response = EvaluateResponse {
            success: true,
            actions_generated: 1,
            message: "Evaluation complete, 1 scaling action generated".to_string(),
        };

        format.output(out, &response)?;
        Ok(())
    }
}

/// Autoscaler status response.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct AutoscalerStatusResponse {
    enabled: bool,
    pool_count: usize,
    total_nodes: u32,
    total_gpus: u32,
    last_evaluation: Option<String>,
    pending_actions: usize,
}

/// Pool summary for listing.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PoolSummary {
    id: String,
    name: String,
    node_count: u32,
    gpu_count: u32,
    policy_type: String,
    enabled: bool,
}

/// Pool details response.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PoolDetails {
    id: String,
    name: String,
    node_count: u32,
    ready_node_count: u32,
    gpu_count: u32,
    policy: PolicyDetails,
    last_scale_up: Option<String>,
    last_scale_down: Option<String>,
}

/// Policy details.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PolicyDetails {
    policy_type: String,
    min_nodes: u32,
    max_nodes: u32,
    target_utilization: Option<f64>,
    tolerance: Option<f64>,
    scale_up_cooldown_secs: u64,
    scale_down_cooldown_secs: u64,
    enabled: bool,
}

/// Set policy response.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SetPolicyResponse {
    success: bool,
    pool_id: String,
    message: String,
}

/// Enable/disable response.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct EnableResponse {
    success: bool,
    enabled: bool,
    message: String,
}

/// Evaluate response.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct EvaluateResponse {
    success: bool,
    actions_generated: usize,
    message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn autoscale_status_returns_ok() {
        let cmd = AutoscaleCommand::new("ws://localhost:8080");
        let mut output = Vec::new();
        let format = OutputFormat::new(crate::cli::Format::Json);

        let result = cmd
            .execute(&mut output, &format, &AutoscaleCommands::Status)
            .await;

        assert!(result.is_ok());
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("enabled"));
        assert!(output_str.contains("pool_count"));
    }

    #[tokio::test]
    async fn autoscale_pools_returns_list() {
        let cmd = AutoscaleCommand::new("ws://localhost:8080");
        let mut output = Vec::new();
        let format = OutputFormat::new(crate::cli::Format::Json);

        let result = cmd
            .execute(&mut output, &format, &AutoscaleCommands::Pools)
            .await;

        assert!(result.is_ok());
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("gpu-pool-1"));
        assert!(output_str.contains("gpu-pool-2"));
    }

    #[tokio::test]
    async fn autoscale_pool_shows_details() {
        let cmd = AutoscaleCommand::new("ws://localhost:8080");
        let mut output = Vec::new();
        let format = OutputFormat::new(crate::cli::Format::Json);

        let result = cmd
            .execute(
                &mut output,
                &format,
                &AutoscaleCommands::Pool {
                    id: "test-pool".to_string(),
                },
            )
            .await;

        assert!(result.is_ok());
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("test-pool"));
        assert!(output_str.contains("policy"));
    }

    #[tokio::test]
    async fn autoscale_enable_returns_success() {
        let cmd = AutoscaleCommand::new("ws://localhost:8080");
        let mut output = Vec::new();
        let format = OutputFormat::new(crate::cli::Format::Json);

        let result = cmd
            .execute(&mut output, &format, &AutoscaleCommands::Enable)
            .await;

        assert!(result.is_ok());
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("enabled"));
        assert!(output_str.contains("true"));
    }

    #[tokio::test]
    async fn autoscale_disable_returns_success() {
        let cmd = AutoscaleCommand::new("ws://localhost:8080");
        let mut output = Vec::new();
        let format = OutputFormat::new(crate::cli::Format::Json);

        let result = cmd
            .execute(&mut output, &format, &AutoscaleCommands::Disable)
            .await;

        assert!(result.is_ok());
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("disabled"));
    }

    #[tokio::test]
    async fn autoscale_evaluate_returns_success() {
        let cmd = AutoscaleCommand::new("ws://localhost:8080");
        let mut output = Vec::new();
        let format = OutputFormat::new(crate::cli::Format::Json);

        let result = cmd
            .execute(&mut output, &format, &AutoscaleCommands::Evaluate)
            .await;

        assert!(result.is_ok());
        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("success"));
        assert!(output_str.contains("actions_generated"));
    }
}
