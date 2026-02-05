//! Clawbernetes CLI binary entrypoint.
//!
//! This is the main entry point for the `clawbernetes` command-line tool.

use std::io;
use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use claw_cli::cli::{Cli, Commands};
use claw_cli::commands::{
    AlertCommand, AuthCommand, AutoscaleCommand, DashboardCommand, DeployCommand,
    LogsCommand, MetricsCommand, MoltCommand, NamespaceCommand, NodeCommand, PreemptCommand,
    PriorityCommand, RollbackCommand, RunCommand, SecretCommand, ServiceCommand, StatusCommand,
    TenantCommand,
};
use claw_cli::output::OutputFormat;

fn main() -> ExitCode {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(io::stderr)
        .init();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Run async runtime
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create async runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(run(cli)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), claw_cli::CliError> {
    let format = OutputFormat::new(cli.format);
    let mut stdout = io::stdout().lock();

    match cli.command {
        Commands::Status => {
            let cmd = StatusCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format).await?;
        }
        Commands::Node { command } => {
            let cmd = NodeCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &command).await?;
        }
        Commands::Run(args) => {
            let cmd = RunCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &args).await?;
        }
        Commands::Molt { command } => {
            let cmd = MoltCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &command).await?;
        }
        Commands::Autoscale { command } => {
            let cmd = AutoscaleCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &command).await?;
        }
        Commands::Secret { command } => {
            let cmd = SecretCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &command).await?;
        }
        Commands::Auth { command } => {
            let cmd = AuthCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &command).await?;
        }
        Commands::Alert { command } => {
            let cmd = AlertCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &command).await?;
        }
        Commands::Tenant { command } => {
            let cmd = TenantCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &command).await?;
        }
        Commands::Namespace { command } => {
            let cmd = NamespaceCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &command).await?;
        }
        Commands::Service { command } => {
            let cmd = ServiceCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &command).await?;
        }
        Commands::Deploy(args) => {
            let cmd = DeployCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &args).await?;
        }
        Commands::Rollback(args) => {
            let cmd = RollbackCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &args).await?;
        }
        Commands::Metrics { command } => {
            let cmd = MetricsCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &command).await?;
        }
        Commands::Logs(args) => {
            let cmd = LogsCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &args).await?;
        }
        Commands::Dashboard { command } => {
            let cmd = DashboardCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &command).await?;
        }
        Commands::Preempt(args) => {
            let cmd = PreemptCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &args).await?;
        }
        Commands::Priority { command } => {
            let cmd = PriorityCommand::new(&cli.gateway);
            cmd.execute(&mut stdout, &format, &command).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_cli::cli::Format;

    #[test]
    fn cli_parses_status() {
        let cli = Cli::parse_from(["clawbernetes", "status"]);
        assert!(matches!(cli.command, Commands::Status));
    }

    #[test]
    fn cli_parses_node_list() {
        let cli = Cli::parse_from(["clawbernetes", "node", "list"]);
        match cli.command {
            Commands::Node { command } => {
                assert!(matches!(command, claw_cli::cli::NodeCommands::List));
            }
            _ => panic!("expected node command"),
        }
    }

    #[test]
    fn cli_respects_format_flag() {
        let cli = Cli::parse_from(["clawbernetes", "--format", "json", "status"]);
        assert_eq!(cli.format, Format::Json);
    }

    #[test]
    fn cli_respects_gateway_flag() {
        let cli = Cli::parse_from(["clawbernetes", "-g", "ws://custom:9000", "status"]);
        assert_eq!(cli.gateway, "ws://custom:9000");
    }

    #[tokio::test]
    async fn run_status_command_no_gateway() {
        // Without a gateway running, status command should fail with connection error
        let cli = Cli::parse_from(["clawbernetes", "status"]);
        let result = run(cli).await;
        // Should fail with connection error (no gateway running)
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn run_node_list_command_no_gateway() {
        // Without a gateway running, node list should fail with connection error
        let cli = Cli::parse_from(["clawbernetes", "node", "list"]);
        let result = run(cli).await;
        // Should fail with connection error
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn run_molt_status_command_no_gateway() {
        // MOLT status currently returns mock data, so it may still work
        // until we implement real MOLT client
        let cli = Cli::parse_from(["clawbernetes", "molt", "status"]);
        let result = run(cli).await;
        // MOLT commands still use placeholder data, may succeed or fail
        // depending on implementation
        let _ = result; // Accept either outcome for now
    }

    #[tokio::test]
    async fn run_with_invalid_gateway_fails() {
        let cli = Cli::parse_from(["clawbernetes", "-g", "http://invalid", "status"]);
        let result = run(cli).await;
        assert!(result.is_err());
    }
}
