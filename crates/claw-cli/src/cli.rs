//! Command-line argument parsing with clap.

use clap::{Parser, Subcommand, ValueEnum};

/// Clawbernetes CLI - AI-Native GPU Orchestration.
#[derive(Parser, Debug, Clone)]
#[command(name = "clawbernetes")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Gateway URL to connect to.
    #[arg(short, long, env = "CLAWBERNETES_GATEWAY", default_value = "ws://localhost:8080")]
    pub gateway: String,

    /// Output format.
    #[arg(short, long, value_enum, default_value_t = Format::Table)]
    pub format: Format,

    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Commands,
}

/// Output format options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[derive(Default)]
pub enum Format {
    /// Human-readable table format.
    #[default]
    Table,
    /// JSON output for scripting.
    Json,
}


/// Top-level subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Show cluster status.
    Status,
    
    /// Node management commands.
    Node {
        /// Node subcommand to execute.
        #[command(subcommand)]
        command: NodeCommands,
    },
    
    /// Run a workload on the cluster.
    Run(RunArgs),
    
    /// MOLT network participation.
    Molt {
        /// MOLT subcommand to execute.
        #[command(subcommand)]
        command: MoltCommands,
    },

    /// Autoscaling management commands.
    Autoscale {
        /// Autoscale subcommand to execute.
        #[command(subcommand)]
        command: AutoscaleCommands,
    },
}

/// Node subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum NodeCommands {
    /// List all nodes in the cluster.
    List,
    
    /// Show detailed information about a node.
    Info {
        /// Node ID to inspect.
        id: String,
    },
    
    /// Mark a node for draining.
    ///
    /// Draining prevents new workloads from being scheduled on the node,
    /// but allows existing workloads to continue running.
    Drain {
        /// Node ID to drain.
        id: String,

        /// Force drain even with running workloads.
        #[arg(short, long)]
        force: bool,
    },

    /// Remove drain status from a node.
    ///
    /// Allows the node to accept new workloads again.
    Undrain {
        /// Node ID to undrain.
        id: String,
    },
}

/// Arguments for the run command.
#[derive(Parser, Debug, Clone)]
pub struct RunArgs {
    /// Container image to run.
    #[arg(required = true)]
    pub image: String,

    /// Command to execute in the container.
    #[arg(last = true)]
    pub command: Vec<String>,

    /// GPU indices to attach (comma-separated).
    #[arg(short, long, value_delimiter = ',')]
    pub gpus: Vec<u32>,

    /// Environment variables (KEY=VALUE).
    #[arg(short, long, value_name = "KEY=VALUE")]
    pub env: Vec<String>,

    /// Memory limit in MiB.
    #[arg(short, long)]
    pub memory: Option<u64>,

    /// Detach and run in background.
    #[arg(short, long)]
    pub detach: bool,
}

/// MOLT subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum MoltCommands {
    /// Show MOLT participation status.
    Status,
    
    /// Join the MOLT network.
    Join {
        /// Autonomy level for agent participation.
        #[arg(short, long, value_enum, default_value_t = AutonomyArg::Conservative)]
        autonomy: AutonomyArg,
        
        /// Maximum spend per job in MOLT tokens.
        #[arg(long)]
        max_spend: Option<String>,
    },
    
    /// Leave the MOLT network.
    Leave,
    
    /// Show earnings summary.
    Earnings {
        /// Show detailed breakdown.
        #[arg(short, long)]
        detailed: bool,
    },
}

/// Autonomy level argument for MOLT join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[derive(Default)]
pub enum AutonomyArg {
    /// Minimal autonomy. Only low-risk, pre-approved jobs.
    #[default]
    Conservative,
    /// Balanced autonomy. Most jobs within budget.
    Moderate,
    /// Maximum autonomy. Any job within capability.
    Aggressive,
}

/// Autoscaling subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum AutoscaleCommands {
    /// Show autoscaling status for all pools.
    Status,

    /// List all node pools with their scaling configuration.
    Pools,

    /// Show detailed information about a specific pool.
    Pool {
        /// Pool ID to inspect.
        id: String,
    },

    /// Set or update a scaling policy for a pool.
    SetPolicy(SetScalingPolicyArgs),

    /// Enable autoscaling.
    Enable,

    /// Disable autoscaling.
    Disable,

    /// Trigger an immediate scaling evaluation.
    Evaluate,
}

/// Arguments for setting a scaling policy.
#[derive(Parser, Debug, Clone)]
pub struct SetScalingPolicyArgs {
    /// Pool ID to configure.
    #[arg(required = true)]
    pub pool_id: String,

    /// Policy type.
    #[arg(short = 't', long, value_enum)]
    pub policy_type: PolicyTypeArg,

    /// Minimum number of nodes.
    #[arg(long, default_value = "1")]
    pub min_nodes: u32,

    /// Maximum number of nodes.
    #[arg(long, default_value = "10")]
    pub max_nodes: u32,

    /// Target GPU utilization percentage (for utilization policy).
    #[arg(long)]
    pub target_utilization: Option<f64>,

    /// Tolerance percentage around target (for utilization policy).
    #[arg(long, default_value = "10")]
    pub tolerance: f64,

    /// Target jobs per node (for queue depth policy).
    #[arg(long)]
    pub target_jobs_per_node: Option<u32>,

    /// Scale up threshold (for queue depth policy).
    #[arg(long)]
    pub scale_up_threshold: Option<u32>,

    /// Scale down threshold (for queue depth policy).
    #[arg(long)]
    pub scale_down_threshold: Option<u32>,

    /// Scale up cooldown in seconds.
    #[arg(long, default_value = "300")]
    pub scale_up_cooldown: u64,

    /// Scale down cooldown in seconds.
    #[arg(long, default_value = "600")]
    pub scale_down_cooldown: u64,
}

/// Policy type argument for autoscaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PolicyTypeArg {
    /// Scale based on GPU utilization percentage.
    Utilization,
    /// Scale based on job queue depth.
    QueueDepth,
    /// Scale based on time schedule.
    Schedule,
}


#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    // Test that the CLI can be constructed and help works
    #[test]
    fn cli_help_does_not_panic() {
        Cli::command().debug_assert();
    }

    // Test parsing status command
    #[test]
    fn parse_status_command() {
        let cli = Cli::parse_from(["clawbernetes", "status"]);
        assert!(matches!(cli.command, Commands::Status));
        assert_eq!(cli.gateway, "ws://localhost:8080");
        assert_eq!(cli.format, Format::Table);
    }

    // Test parsing status with custom gateway
    #[test]
    fn parse_status_with_gateway() {
        let cli = Cli::parse_from(["clawbernetes", "-g", "ws://192.168.1.100:9000", "status"]);
        assert!(matches!(cli.command, Commands::Status));
        assert_eq!(cli.gateway, "ws://192.168.1.100:9000");
    }

    // Test parsing status with json format
    #[test]
    fn parse_status_with_json_format() {
        let cli = Cli::parse_from(["clawbernetes", "--format", "json", "status"]);
        assert!(matches!(cli.command, Commands::Status));
        assert_eq!(cli.format, Format::Json);
    }

    // Test parsing node list command
    #[test]
    fn parse_node_list_command() {
        let cli = Cli::parse_from(["clawbernetes", "node", "list"]);
        match cli.command {
            Commands::Node { command: NodeCommands::List } => {}
            _ => panic!("expected node list command"),
        }
    }

    // Test parsing node info command
    #[test]
    fn parse_node_info_command() {
        let cli = Cli::parse_from(["clawbernetes", "node", "info", "node-abc-123"]);
        match cli.command {
            Commands::Node { command: NodeCommands::Info { id } } => {
                assert_eq!(id, "node-abc-123");
            }
            _ => panic!("expected node info command"),
        }
    }

    // Test parsing node drain command
    #[test]
    fn parse_node_drain_command() {
        let cli = Cli::parse_from(["clawbernetes", "node", "drain", "node-xyz"]);
        match cli.command {
            Commands::Node { command: NodeCommands::Drain { id, force } } => {
                assert_eq!(id, "node-xyz");
                assert!(!force);
            }
            _ => panic!("expected node drain command"),
        }
    }

    // Test parsing node drain with force flag
    #[test]
    fn parse_node_drain_with_force() {
        let cli = Cli::parse_from(["clawbernetes", "node", "drain", "--force", "node-xyz"]);
        match cli.command {
            Commands::Node { command: NodeCommands::Drain { id, force } } => {
                assert_eq!(id, "node-xyz");
                assert!(force);
            }
            _ => panic!("expected node drain command"),
        }
    }

    // Test parsing run command with minimal args
    #[test]
    fn parse_run_command_minimal() {
        let cli = Cli::parse_from(["clawbernetes", "run", "nginx:latest"]);
        match cli.command {
            Commands::Run(args) => {
                assert_eq!(args.image, "nginx:latest");
                assert!(args.command.is_empty());
                assert!(args.gpus.is_empty());
            }
            _ => panic!("expected run command"),
        }
    }

    // Test parsing run command with GPUs
    #[test]
    fn parse_run_command_with_gpus() {
        let cli = Cli::parse_from(["clawbernetes", "run", "-g", "0,1,2", "pytorch:latest"]);
        match cli.command {
            Commands::Run(args) => {
                assert_eq!(args.image, "pytorch:latest");
                assert_eq!(args.gpus, vec![0, 1, 2]);
            }
            _ => panic!("expected run command"),
        }
    }

    // Test parsing run command with environment variables
    #[test]
    fn parse_run_command_with_env() {
        let cli = Cli::parse_from([
            "clawbernetes", "run", 
            "-e", "MODEL=gpt2",
            "-e", "BATCH_SIZE=32",
            "transformer:latest"
        ]);
        match cli.command {
            Commands::Run(args) => {
                assert_eq!(args.image, "transformer:latest");
                assert_eq!(args.env, vec!["MODEL=gpt2", "BATCH_SIZE=32"]);
            }
            _ => panic!("expected run command"),
        }
    }

    // Test parsing run command with memory limit
    #[test]
    fn parse_run_command_with_memory() {
        let cli = Cli::parse_from(["clawbernetes", "run", "--memory", "8192", "app:latest"]);
        match cli.command {
            Commands::Run(args) => {
                assert_eq!(args.image, "app:latest");
                assert_eq!(args.memory, Some(8192));
            }
            _ => panic!("expected run command"),
        }
    }

    // Test parsing run command with detach
    #[test]
    fn parse_run_command_with_detach() {
        let cli = Cli::parse_from(["clawbernetes", "run", "-d", "worker:latest"]);
        match cli.command {
            Commands::Run(args) => {
                assert!(args.detach);
            }
            _ => panic!("expected run command"),
        }
    }

    // Test parsing run command with trailing command
    #[test]
    fn parse_run_command_with_trailing_command() {
        let cli = Cli::parse_from([
            "clawbernetes", "run", "python:latest", "--", "python", "-m", "http.server"
        ]);
        match cli.command {
            Commands::Run(args) => {
                assert_eq!(args.image, "python:latest");
                assert_eq!(args.command, vec!["python", "-m", "http.server"]);
            }
            _ => panic!("expected run command"),
        }
    }

    // Test parsing molt status command
    #[test]
    fn parse_molt_status_command() {
        let cli = Cli::parse_from(["clawbernetes", "molt", "status"]);
        match cli.command {
            Commands::Molt { command: MoltCommands::Status } => {}
            _ => panic!("expected molt status command"),
        }
    }

    // Test parsing molt join command with default autonomy
    #[test]
    fn parse_molt_join_default() {
        let cli = Cli::parse_from(["clawbernetes", "molt", "join"]);
        match cli.command {
            Commands::Molt { command: MoltCommands::Join { autonomy, max_spend } } => {
                assert_eq!(autonomy, AutonomyArg::Conservative);
                assert!(max_spend.is_none());
            }
            _ => panic!("expected molt join command"),
        }
    }

    // Test parsing molt join with aggressive autonomy
    #[test]
    fn parse_molt_join_aggressive() {
        let cli = Cli::parse_from(["clawbernetes", "molt", "join", "--autonomy", "aggressive"]);
        match cli.command {
            Commands::Molt { command: MoltCommands::Join { autonomy, .. } } => {
                assert_eq!(autonomy, AutonomyArg::Aggressive);
            }
            _ => panic!("expected molt join command"),
        }
    }

    // Test parsing molt join with max spend
    #[test]
    fn parse_molt_join_with_max_spend() {
        let cli = Cli::parse_from([
            "clawbernetes", "molt", "join", 
            "--autonomy", "moderate",
            "--max-spend", "100.5"
        ]);
        match cli.command {
            Commands::Molt { command: MoltCommands::Join { autonomy, max_spend } } => {
                assert_eq!(autonomy, AutonomyArg::Moderate);
                assert_eq!(max_spend, Some("100.5".into()));
            }
            _ => panic!("expected molt join command"),
        }
    }

    // Test parsing molt leave command
    #[test]
    fn parse_molt_leave_command() {
        let cli = Cli::parse_from(["clawbernetes", "molt", "leave"]);
        match cli.command {
            Commands::Molt { command: MoltCommands::Leave } => {}
            _ => panic!("expected molt leave command"),
        }
    }

    // Test parsing molt earnings command
    #[test]
    fn parse_molt_earnings_command() {
        let cli = Cli::parse_from(["clawbernetes", "molt", "earnings"]);
        match cli.command {
            Commands::Molt { command: MoltCommands::Earnings { detailed } } => {
                assert!(!detailed);
            }
            _ => panic!("expected molt earnings command"),
        }
    }

    // Test parsing molt earnings with detailed flag
    #[test]
    fn parse_molt_earnings_detailed() {
        let cli = Cli::parse_from(["clawbernetes", "molt", "earnings", "--detailed"]);
        match cli.command {
            Commands::Molt { command: MoltCommands::Earnings { detailed } } => {
                assert!(detailed);
            }
            _ => panic!("expected molt earnings command"),
        }
    }

    // Test format default
    #[test]
    fn format_default_is_table() {
        assert_eq!(Format::default(), Format::Table);
    }

    // Test autonomy arg default
    #[test]
    fn autonomy_arg_default_is_conservative() {
        assert_eq!(AutonomyArg::default(), AutonomyArg::Conservative);
    }

    // Test long form gateway flag
    #[test]
    fn parse_long_gateway_flag() {
        let cli = Cli::parse_from(["clawbernetes", "--gateway", "ws://custom:8080", "status"]);
        assert_eq!(cli.gateway, "ws://custom:8080");
    }

    // Test combined flags
    #[test]
    fn parse_combined_flags() {
        let cli = Cli::parse_from([
            "clawbernetes",
            "-g", "ws://prod:8080",
            "-f", "json",
            "node", "list"
        ]);
        assert_eq!(cli.gateway, "ws://prod:8080");
        assert_eq!(cli.format, Format::Json);
        assert!(matches!(cli.command, Commands::Node { command: NodeCommands::List }));
    }

    // Test parsing autoscale status command
    #[test]
    fn parse_autoscale_status_command() {
        let cli = Cli::parse_from(["clawbernetes", "autoscale", "status"]);
        match cli.command {
            Commands::Autoscale { command: AutoscaleCommands::Status } => {}
            _ => panic!("expected autoscale status command"),
        }
    }

    // Test parsing autoscale pools command
    #[test]
    fn parse_autoscale_pools_command() {
        let cli = Cli::parse_from(["clawbernetes", "autoscale", "pools"]);
        match cli.command {
            Commands::Autoscale { command: AutoscaleCommands::Pools } => {}
            _ => panic!("expected autoscale pools command"),
        }
    }

    // Test parsing autoscale pool info command
    #[test]
    fn parse_autoscale_pool_command() {
        let cli = Cli::parse_from(["clawbernetes", "autoscale", "pool", "gpu-pool-1"]);
        match cli.command {
            Commands::Autoscale { command: AutoscaleCommands::Pool { id } } => {
                assert_eq!(id, "gpu-pool-1");
            }
            _ => panic!("expected autoscale pool command"),
        }
    }

    // Test parsing autoscale set-policy with utilization
    #[test]
    fn parse_autoscale_set_policy_utilization() {
        let cli = Cli::parse_from([
            "clawbernetes", "autoscale", "set-policy",
            "gpu-pool-1",
            "-t", "utilization",
            "--min-nodes", "2",
            "--max-nodes", "20",
            "--target-utilization", "70",
        ]);
        match cli.command {
            Commands::Autoscale { command: AutoscaleCommands::SetPolicy(args) } => {
                assert_eq!(args.pool_id, "gpu-pool-1");
                assert_eq!(args.policy_type, PolicyTypeArg::Utilization);
                assert_eq!(args.min_nodes, 2);
                assert_eq!(args.max_nodes, 20);
                assert_eq!(args.target_utilization, Some(70.0));
            }
            _ => panic!("expected autoscale set-policy command"),
        }
    }

    // Test parsing autoscale set-policy with queue depth
    #[test]
    fn parse_autoscale_set_policy_queue_depth() {
        let cli = Cli::parse_from([
            "clawbernetes", "autoscale", "set-policy",
            "gpu-pool-1",
            "-t", "queue-depth",
            "--target-jobs-per-node", "5",
            "--scale-up-threshold", "20",
            "--scale-down-threshold", "2",
        ]);
        match cli.command {
            Commands::Autoscale { command: AutoscaleCommands::SetPolicy(args) } => {
                assert_eq!(args.policy_type, PolicyTypeArg::QueueDepth);
                assert_eq!(args.target_jobs_per_node, Some(5));
                assert_eq!(args.scale_up_threshold, Some(20));
                assert_eq!(args.scale_down_threshold, Some(2));
            }
            _ => panic!("expected autoscale set-policy command"),
        }
    }

    // Test parsing autoscale enable command
    #[test]
    fn parse_autoscale_enable_command() {
        let cli = Cli::parse_from(["clawbernetes", "autoscale", "enable"]);
        match cli.command {
            Commands::Autoscale { command: AutoscaleCommands::Enable } => {}
            _ => panic!("expected autoscale enable command"),
        }
    }

    // Test parsing autoscale disable command
    #[test]
    fn parse_autoscale_disable_command() {
        let cli = Cli::parse_from(["clawbernetes", "autoscale", "disable"]);
        match cli.command {
            Commands::Autoscale { command: AutoscaleCommands::Disable } => {}
            _ => panic!("expected autoscale disable command"),
        }
    }

    // Test parsing autoscale evaluate command
    #[test]
    fn parse_autoscale_evaluate_command() {
        let cli = Cli::parse_from(["clawbernetes", "autoscale", "evaluate"]);
        match cli.command {
            Commands::Autoscale { command: AutoscaleCommands::Evaluate } => {}
            _ => panic!("expected autoscale evaluate command"),
        }
    }

    // Test policy type arg values
    #[test]
    fn policy_type_arg_values() {
        // Verify all values can be parsed
        let cli = Cli::parse_from([
            "clawbernetes", "autoscale", "set-policy",
            "pool-1", "-t", "utilization",
        ]);
        assert!(matches!(
            cli.command,
            Commands::Autoscale { command: AutoscaleCommands::SetPolicy(args) }
            if args.policy_type == PolicyTypeArg::Utilization
        ));

        let cli = Cli::parse_from([
            "clawbernetes", "autoscale", "set-policy",
            "pool-1", "-t", "queue-depth",
        ]);
        assert!(matches!(
            cli.command,
            Commands::Autoscale { command: AutoscaleCommands::SetPolicy(args) }
            if args.policy_type == PolicyTypeArg::QueueDepth
        ));

        let cli = Cli::parse_from([
            "clawbernetes", "autoscale", "set-policy",
            "pool-1", "-t", "schedule",
        ]);
        assert!(matches!(
            cli.command,
            Commands::Autoscale { command: AutoscaleCommands::SetPolicy(args) }
            if args.policy_type == PolicyTypeArg::Schedule
        ));
    }

    // Test set-policy with custom cooldowns
    #[test]
    fn parse_autoscale_set_policy_with_cooldowns() {
        let cli = Cli::parse_from([
            "clawbernetes", "autoscale", "set-policy",
            "gpu-pool-1",
            "-t", "utilization",
            "--scale-up-cooldown", "120",
            "--scale-down-cooldown", "300",
        ]);
        match cli.command {
            Commands::Autoscale { command: AutoscaleCommands::SetPolicy(args) } => {
                assert_eq!(args.scale_up_cooldown, 120);
                assert_eq!(args.scale_down_cooldown, 300);
            }
            _ => panic!("expected autoscale set-policy command"),
        }
    }
}
