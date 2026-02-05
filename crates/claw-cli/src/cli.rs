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

    /// Secret management commands.
    Secret {
        /// Secret subcommand to execute.
        #[command(subcommand)]
        command: SecretCommands,
    },

    /// Authentication commands.
    Auth {
        /// Auth subcommand to execute.
        #[command(subcommand)]
        command: AuthCommands,
    },

    /// Alert management commands.
    Alert {
        /// Alert subcommand to execute.
        #[command(subcommand)]
        command: AlertCommands,
    },

    /// Tenant management commands.
    Tenant {
        /// Tenant subcommand to execute.
        #[command(subcommand)]
        command: TenantCommands,
    },

    /// Namespace management commands.
    Namespace {
        /// Namespace subcommand to execute.
        #[command(subcommand)]
        command: NamespaceCommands,
    },

    /// Service discovery commands.
    Service {
        /// Service subcommand to execute.
        #[command(subcommand)]
        command: ServiceCommands,
    },

    /// Deploy a workload from an intent file.
    Deploy(DeployArgs),

    /// Rollback a workload to a previous version.
    Rollback(RollbackArgs),

    /// Query metrics from the cluster.
    Metrics {
        /// Metrics subcommand to execute.
        #[command(subcommand)]
        command: MetricsCommands,
    },

    /// View logs for a workload.
    Logs(LogsArgs),

    /// Dashboard management commands.
    Dashboard {
        /// Dashboard subcommand to execute.
        #[command(subcommand)]
        command: DashboardCommands,
    },

    /// Preempt a workload.
    Preempt(PreemptArgs),

    /// Priority management commands.
    Priority {
        /// Priority subcommand to execute.
        #[command(subcommand)]
        command: PriorityCommands,
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

// ============================================================================
// Secret Commands
// ============================================================================

/// Secret management subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum SecretCommands {
    /// List all secrets.
    List {
        /// Filter by namespace.
        #[arg(short, long)]
        namespace: Option<String>,
    },

    /// Get a secret value.
    Get {
        /// Secret name.
        name: String,

        /// Namespace.
        #[arg(short, long)]
        namespace: Option<String>,
    },

    /// Set a secret value.
    Set {
        /// Secret name.
        name: String,

        /// Secret value (or use --file).
        value: Option<String>,

        /// Read value from file.
        #[arg(short, long)]
        file: Option<String>,

        /// Namespace.
        #[arg(short, long)]
        namespace: Option<String>,
    },

    /// Delete a secret.
    Delete {
        /// Secret name.
        name: String,

        /// Namespace.
        #[arg(short, long)]
        namespace: Option<String>,

        /// Skip confirmation prompt.
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Rotate a secret.
    Rotate {
        /// Secret name.
        name: String,

        /// Namespace.
        #[arg(short, long)]
        namespace: Option<String>,
    },
}

// ============================================================================
// Auth Commands
// ============================================================================

/// Authentication subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum AuthCommands {
    /// Login to the cluster.
    Login {
        /// Username.
        #[arg(short, long)]
        username: Option<String>,

        /// Password (not recommended, use interactive prompt).
        #[arg(short, long)]
        password: Option<String>,

        /// Use token-based authentication.
        #[arg(long)]
        token: Option<String>,
    },

    /// Logout from the cluster.
    Logout,

    /// Show current authentication status.
    Whoami,

    /// API key management.
    Apikey {
        /// API key subcommand.
        #[command(subcommand)]
        command: ApikeyCommands,
    },
}

/// API key subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum ApikeyCommands {
    /// Create a new API key.
    Create {
        /// Name for the API key.
        #[arg(short, long)]
        name: String,

        /// Expiration in days (0 = never).
        #[arg(short, long, default_value = "90")]
        expires: u32,

        /// Scopes for the API key (comma-separated).
        #[arg(short, long, value_delimiter = ',')]
        scopes: Vec<String>,
    },

    /// List all API keys.
    List,

    /// Revoke an API key.
    Revoke {
        /// API key ID to revoke.
        id: String,
    },
}

// ============================================================================
// Alert Commands
// ============================================================================

/// Alert management subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum AlertCommands {
    /// List all alerts.
    List {
        /// Filter by state (firing, pending, resolved).
        #[arg(short, long)]
        state: Option<String>,

        /// Filter by severity (critical, warning, info).
        #[arg(long)]
        severity: Option<String>,
    },

    /// Create an alert rule.
    Create(CreateAlertArgs),

    /// Delete an alert rule.
    Delete {
        /// Alert rule name.
        name: String,

        /// Skip confirmation.
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Silence an alert.
    Silence {
        /// Alert name or matcher pattern.
        matcher: String,

        /// Duration (e.g., "2h", "1d").
        #[arg(short, long, default_value = "1h")]
        duration: String,

        /// Comment/reason for silence.
        #[arg(short, long)]
        comment: Option<String>,
    },

    /// Remove a silence.
    Unsilence {
        /// Silence ID to remove.
        id: String,
    },
}

/// Arguments for creating an alert.
#[derive(Parser, Debug, Clone)]
pub struct CreateAlertArgs {
    /// Alert rule name.
    #[arg(required = true)]
    pub name: String,

    /// PromQL expression for the alert.
    #[arg(short, long)]
    pub expr: String,

    /// Duration before firing (e.g., "5m").
    #[arg(short, long, default_value = "5m")]
    pub for_duration: String,

    /// Alert severity (critical, warning, info).
    #[arg(short, long, default_value = "warning")]
    pub severity: String,

    /// Alert summary message.
    #[arg(long)]
    pub summary: Option<String>,

    /// Alert description.
    #[arg(long)]
    pub description: Option<String>,
}

// ============================================================================
// Tenant Commands
// ============================================================================

/// Tenant management subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum TenantCommands {
    /// List all tenants.
    List,

    /// Create a new tenant.
    Create {
        /// Tenant name.
        name: String,

        /// Display name.
        #[arg(short, long)]
        display_name: Option<String>,

        /// Admin email.
        #[arg(short, long)]
        admin_email: Option<String>,
    },

    /// Delete a tenant.
    Delete {
        /// Tenant name.
        name: String,

        /// Skip confirmation.
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Show tenant details.
    Info {
        /// Tenant name.
        name: String,
    },
}

// ============================================================================
// Namespace Commands
// ============================================================================

/// Namespace management subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum NamespaceCommands {
    /// List all namespaces.
    List {
        /// Filter by tenant.
        #[arg(short, long)]
        tenant: Option<String>,
    },

    /// Create a new namespace.
    Create {
        /// Namespace name.
        name: String,

        /// Tenant this namespace belongs to.
        #[arg(short, long)]
        tenant: Option<String>,

        /// Resource quota (CPU cores).
        #[arg(long)]
        cpu_quota: Option<u32>,

        /// Resource quota (GPU count).
        #[arg(long)]
        gpu_quota: Option<u32>,

        /// Resource quota (memory in MiB).
        #[arg(long)]
        memory_quota: Option<u64>,
    },

    /// Delete a namespace.
    Delete {
        /// Namespace name.
        name: String,

        /// Skip confirmation.
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Show namespace details.
    Info {
        /// Namespace name.
        name: String,
    },
}

// ============================================================================
// Service Discovery Commands
// ============================================================================

/// Service discovery subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum ServiceCommands {
    /// List all registered services.
    List {
        /// Filter by namespace.
        #[arg(short, long)]
        namespace: Option<String>,
    },

    /// Register a new service.
    Register {
        /// Service name.
        name: String,

        /// Service address (host:port).
        #[arg(short, long)]
        address: String,

        /// Service port.
        #[arg(short, long)]
        port: u16,

        /// Health check endpoint.
        #[arg(long)]
        health_check: Option<String>,

        /// Namespace.
        #[arg(short, long)]
        namespace: Option<String>,

        /// Service tags (comma-separated).
        #[arg(short, long, value_delimiter = ',')]
        tags: Vec<String>,
    },

    /// Deregister a service.
    Deregister {
        /// Service name or ID.
        name: String,

        /// Skip confirmation.
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Show service details.
    Info {
        /// Service name.
        name: String,
    },
}

// ============================================================================
// Deploy Commands
// ============================================================================

/// Arguments for the deploy command.
#[derive(Parser, Debug, Clone)]
pub struct DeployArgs {
    /// Path to intent file (YAML/JSON).
    #[arg(required = true)]
    pub intent: String,

    /// Dry run without applying.
    #[arg(long)]
    pub dry_run: bool,

    /// Wait for deployment to complete.
    #[arg(short, long)]
    pub wait: bool,

    /// Timeout for wait (e.g., "5m").
    #[arg(long, default_value = "5m")]
    pub timeout: String,

    /// Namespace.
    #[arg(short, long)]
    pub namespace: Option<String>,
}

/// Arguments for the rollback command.
#[derive(Parser, Debug, Clone)]
pub struct RollbackArgs {
    /// Workload name to rollback.
    #[arg(required = true)]
    pub workload: String,

    /// Revision to rollback to (default: previous).
    #[arg(short, long)]
    pub revision: Option<u32>,

    /// Namespace.
    #[arg(short, long)]
    pub namespace: Option<String>,
}

// ============================================================================
// Metrics Commands
// ============================================================================

/// Metrics subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum MetricsCommands {
    /// Query metrics using PromQL.
    Query {
        /// PromQL expression.
        expr: String,

        /// Time range start (RFC3339 or relative like "-1h").
        #[arg(long)]
        start: Option<String>,

        /// Time range end (RFC3339 or relative).
        #[arg(long)]
        end: Option<String>,

        /// Step interval for range queries (e.g., "15s").
        #[arg(long)]
        step: Option<String>,
    },

    /// List available metrics.
    List {
        /// Filter by metric name prefix.
        #[arg(short, long)]
        prefix: Option<String>,
    },
}

// ============================================================================
// Logs Commands
// ============================================================================

/// Arguments for the logs command.
#[derive(Parser, Debug, Clone)]
pub struct LogsArgs {
    /// Workload name.
    #[arg(required = true)]
    pub workload: String,

    /// Follow log output.
    #[arg(short, long)]
    pub follow: bool,

    /// Number of lines to show (default: 100).
    #[arg(short, long, default_value = "100")]
    pub tail: u32,

    /// Show timestamps.
    #[arg(short = 'T', long)]
    pub timestamps: bool,

    /// Filter by time (e.g., "--since 1h").
    #[arg(long)]
    pub since: Option<String>,

    /// Namespace.
    #[arg(short, long)]
    pub namespace: Option<String>,

    /// Container name (if multiple containers).
    #[arg(short, long)]
    pub container: Option<String>,
}

// ============================================================================
// Dashboard Commands
// ============================================================================

/// Dashboard subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum DashboardCommands {
    /// Start the dashboard server.
    Start {
        /// Port to listen on.
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Bind address.
        #[arg(short, long, default_value = "127.0.0.1")]
        bind: String,

        /// Open browser automatically.
        #[arg(long)]
        open: bool,
    },

    /// Show dashboard URL (if running).
    Url,
}

// ============================================================================
// Preemption Commands
// ============================================================================

/// Arguments for the preempt command.
#[derive(Parser, Debug, Clone)]
pub struct PreemptArgs {
    /// Workload name to preempt.
    #[arg(required = true)]
    pub workload: String,

    /// Reason for preemption.
    #[arg(short, long)]
    pub reason: Option<String>,

    /// Skip confirmation.
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Namespace.
    #[arg(short, long)]
    pub namespace: Option<String>,
}

/// Priority management subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum PriorityCommands {
    /// List priority classes.
    List,

    /// Set priority for a workload.
    Set {
        /// Workload name.
        workload: String,

        /// Priority value (0-1000, higher = more important).
        priority: u32,

        /// Namespace.
        #[arg(short, long)]
        namespace: Option<String>,
    },

    /// Get priority for a workload.
    Get {
        /// Workload name.
        workload: String,

        /// Namespace.
        #[arg(short, long)]
        namespace: Option<String>,
    },
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

    // ========================================================================
    // Secret command tests
    // ========================================================================

    #[test]
    fn parse_secret_list() {
        let cli = Cli::parse_from(["clawbernetes", "secret", "list"]);
        match cli.command {
            Commands::Secret { command: SecretCommands::List { namespace } } => {
                assert!(namespace.is_none());
            }
            _ => panic!("expected secret list command"),
        }
    }

    #[test]
    fn parse_secret_list_with_namespace() {
        let cli = Cli::parse_from(["clawbernetes", "secret", "list", "-n", "production"]);
        match cli.command {
            Commands::Secret { command: SecretCommands::List { namespace } } => {
                assert_eq!(namespace, Some("production".into()));
            }
            _ => panic!("expected secret list command"),
        }
    }

    #[test]
    fn parse_secret_get() {
        let cli = Cli::parse_from(["clawbernetes", "secret", "get", "my-secret"]);
        match cli.command {
            Commands::Secret { command: SecretCommands::Get { name, namespace } } => {
                assert_eq!(name, "my-secret");
                assert!(namespace.is_none());
            }
            _ => panic!("expected secret get command"),
        }
    }

    #[test]
    fn parse_secret_set() {
        let cli = Cli::parse_from(["clawbernetes", "secret", "set", "db-pass", "secret123"]);
        match cli.command {
            Commands::Secret { command: SecretCommands::Set { name, value, .. } } => {
                assert_eq!(name, "db-pass");
                assert_eq!(value, Some("secret123".into()));
            }
            _ => panic!("expected secret set command"),
        }
    }

    #[test]
    fn parse_secret_delete() {
        let cli = Cli::parse_from(["clawbernetes", "secret", "delete", "old-secret", "-y"]);
        match cli.command {
            Commands::Secret { command: SecretCommands::Delete { name, yes, .. } } => {
                assert_eq!(name, "old-secret");
                assert!(yes);
            }
            _ => panic!("expected secret delete command"),
        }
    }

    #[test]
    fn parse_secret_rotate() {
        let cli = Cli::parse_from(["clawbernetes", "secret", "rotate", "api-key"]);
        match cli.command {
            Commands::Secret { command: SecretCommands::Rotate { name, .. } } => {
                assert_eq!(name, "api-key");
            }
            _ => panic!("expected secret rotate command"),
        }
    }

    // ========================================================================
    // Auth command tests
    // ========================================================================

    #[test]
    fn parse_auth_login() {
        let cli = Cli::parse_from(["clawbernetes", "auth", "login", "-u", "admin"]);
        match cli.command {
            Commands::Auth { command: AuthCommands::Login { username, .. } } => {
                assert_eq!(username, Some("admin".into()));
            }
            _ => panic!("expected auth login command"),
        }
    }

    #[test]
    fn parse_auth_logout() {
        let cli = Cli::parse_from(["clawbernetes", "auth", "logout"]);
        assert!(matches!(cli.command, Commands::Auth { command: AuthCommands::Logout }));
    }

    #[test]
    fn parse_auth_whoami() {
        let cli = Cli::parse_from(["clawbernetes", "auth", "whoami"]);
        assert!(matches!(cli.command, Commands::Auth { command: AuthCommands::Whoami }));
    }

    #[test]
    fn parse_auth_apikey_create() {
        let cli = Cli::parse_from([
            "clawbernetes", "auth", "apikey", "create",
            "-n", "ci-key",
            "-e", "30",
            "-s", "deploy,read"
        ]);
        match cli.command {
            Commands::Auth { command: AuthCommands::Apikey { command: ApikeyCommands::Create { name, expires, scopes } } } => {
                assert_eq!(name, "ci-key");
                assert_eq!(expires, 30);
                assert_eq!(scopes, vec!["deploy", "read"]);
            }
            _ => panic!("expected auth apikey create command"),
        }
    }

    #[test]
    fn parse_auth_apikey_list() {
        let cli = Cli::parse_from(["clawbernetes", "auth", "apikey", "list"]);
        assert!(matches!(
            cli.command,
            Commands::Auth { command: AuthCommands::Apikey { command: ApikeyCommands::List } }
        ));
    }

    #[test]
    fn parse_auth_apikey_revoke() {
        let cli = Cli::parse_from(["clawbernetes", "auth", "apikey", "revoke", "ak_123"]);
        match cli.command {
            Commands::Auth { command: AuthCommands::Apikey { command: ApikeyCommands::Revoke { id } } } => {
                assert_eq!(id, "ak_123");
            }
            _ => panic!("expected auth apikey revoke command"),
        }
    }

    // ========================================================================
    // Alert command tests
    // ========================================================================

    #[test]
    fn parse_alert_list() {
        let cli = Cli::parse_from(["clawbernetes", "alert", "list"]);
        match cli.command {
            Commands::Alert { command: AlertCommands::List { state, severity } } => {
                assert!(state.is_none());
                assert!(severity.is_none());
            }
            _ => panic!("expected alert list command"),
        }
    }

    #[test]
    fn parse_alert_list_with_filters() {
        let cli = Cli::parse_from(["clawbernetes", "alert", "list", "-s", "firing", "--severity", "critical"]);
        match cli.command {
            Commands::Alert { command: AlertCommands::List { state, severity } } => {
                assert_eq!(state, Some("firing".into()));
                assert_eq!(severity, Some("critical".into()));
            }
            _ => panic!("expected alert list command"),
        }
    }

    #[test]
    fn parse_alert_create() {
        let cli = Cli::parse_from([
            "clawbernetes", "alert", "create", "HighCPU",
            "-e", "cpu_usage > 90",
            "-f", "5m",
            "-s", "warning"
        ]);
        match cli.command {
            Commands::Alert { command: AlertCommands::Create(args) } => {
                assert_eq!(args.name, "HighCPU");
                assert_eq!(args.expr, "cpu_usage > 90");
                assert_eq!(args.for_duration, "5m");
                assert_eq!(args.severity, "warning");
            }
            _ => panic!("expected alert create command"),
        }
    }

    #[test]
    fn parse_alert_silence() {
        let cli = Cli::parse_from([
            "clawbernetes", "alert", "silence", "HighCPU",
            "-d", "2h",
            "-c", "Scheduled maintenance"
        ]);
        match cli.command {
            Commands::Alert { command: AlertCommands::Silence { matcher, duration, comment } } => {
                assert_eq!(matcher, "HighCPU");
                assert_eq!(duration, "2h");
                assert_eq!(comment, Some("Scheduled maintenance".into()));
            }
            _ => panic!("expected alert silence command"),
        }
    }

    // ========================================================================
    // Tenant command tests
    // ========================================================================

    #[test]
    fn parse_tenant_list() {
        let cli = Cli::parse_from(["clawbernetes", "tenant", "list"]);
        assert!(matches!(cli.command, Commands::Tenant { command: TenantCommands::List }));
    }

    #[test]
    fn parse_tenant_create() {
        let cli = Cli::parse_from([
            "clawbernetes", "tenant", "create", "ml-team",
            "-d", "Machine Learning Team",
            "-a", "admin@ml.com"
        ]);
        match cli.command {
            Commands::Tenant { command: TenantCommands::Create { name, display_name, admin_email } } => {
                assert_eq!(name, "ml-team");
                assert_eq!(display_name, Some("Machine Learning Team".into()));
                assert_eq!(admin_email, Some("admin@ml.com".into()));
            }
            _ => panic!("expected tenant create command"),
        }
    }

    #[test]
    fn parse_tenant_delete() {
        let cli = Cli::parse_from(["clawbernetes", "tenant", "delete", "old-tenant", "-y"]);
        match cli.command {
            Commands::Tenant { command: TenantCommands::Delete { name, yes } } => {
                assert_eq!(name, "old-tenant");
                assert!(yes);
            }
            _ => panic!("expected tenant delete command"),
        }
    }

    // ========================================================================
    // Namespace command tests
    // ========================================================================

    #[test]
    fn parse_namespace_list() {
        let cli = Cli::parse_from(["clawbernetes", "namespace", "list"]);
        match cli.command {
            Commands::Namespace { command: NamespaceCommands::List { tenant } } => {
                assert!(tenant.is_none());
            }
            _ => panic!("expected namespace list command"),
        }
    }

    #[test]
    fn parse_namespace_create() {
        let cli = Cli::parse_from([
            "clawbernetes", "namespace", "create", "production",
            "-t", "ml-team",
            "--gpu-quota", "32"
        ]);
        match cli.command {
            Commands::Namespace { command: NamespaceCommands::Create { name, tenant, gpu_quota, .. } } => {
                assert_eq!(name, "production");
                assert_eq!(tenant, Some("ml-team".into()));
                assert_eq!(gpu_quota, Some(32));
            }
            _ => panic!("expected namespace create command"),
        }
    }

    // ========================================================================
    // Service command tests
    // ========================================================================

    #[test]
    fn parse_service_list() {
        let cli = Cli::parse_from(["clawbernetes", "service", "list"]);
        match cli.command {
            Commands::Service { command: ServiceCommands::List { namespace } } => {
                assert!(namespace.is_none());
            }
            _ => panic!("expected service list command"),
        }
    }

    #[test]
    fn parse_service_register() {
        let cli = Cli::parse_from([
            "clawbernetes", "service", "register", "my-api",
            "-a", "10.0.1.50",
            "-p", "8080",
            "--health-check", "/health",
            "-t", "web,api"
        ]);
        match cli.command {
            Commands::Service { command: ServiceCommands::Register { name, address, port, health_check, tags, .. } } => {
                assert_eq!(name, "my-api");
                assert_eq!(address, "10.0.1.50");
                assert_eq!(port, 8080);
                assert_eq!(health_check, Some("/health".into()));
                assert_eq!(tags, vec!["web", "api"]);
            }
            _ => panic!("expected service register command"),
        }
    }

    // ========================================================================
    // Deploy command tests
    // ========================================================================

    #[test]
    fn parse_deploy_minimal() {
        let cli = Cli::parse_from(["clawbernetes", "deploy", "workload.yaml"]);
        match cli.command {
            Commands::Deploy(args) => {
                assert_eq!(args.intent, "workload.yaml");
                assert!(!args.dry_run);
                assert!(!args.wait);
            }
            _ => panic!("expected deploy command"),
        }
    }

    #[test]
    fn parse_deploy_with_options() {
        let cli = Cli::parse_from([
            "clawbernetes", "deploy", "intent.yaml",
            "--dry-run",
            "-w",
            "--timeout", "10m",
            "-n", "production"
        ]);
        match cli.command {
            Commands::Deploy(args) => {
                assert_eq!(args.intent, "intent.yaml");
                assert!(args.dry_run);
                assert!(args.wait);
                assert_eq!(args.timeout, "10m");
                assert_eq!(args.namespace, Some("production".into()));
            }
            _ => panic!("expected deploy command"),
        }
    }

    // ========================================================================
    // Rollback command tests
    // ========================================================================

    #[test]
    fn parse_rollback() {
        let cli = Cli::parse_from(["clawbernetes", "rollback", "my-app"]);
        match cli.command {
            Commands::Rollback(args) => {
                assert_eq!(args.workload, "my-app");
                assert!(args.revision.is_none());
            }
            _ => panic!("expected rollback command"),
        }
    }

    #[test]
    fn parse_rollback_with_revision() {
        let cli = Cli::parse_from(["clawbernetes", "rollback", "my-app", "-r", "5"]);
        match cli.command {
            Commands::Rollback(args) => {
                assert_eq!(args.workload, "my-app");
                assert_eq!(args.revision, Some(5));
            }
            _ => panic!("expected rollback command"),
        }
    }

    // ========================================================================
    // Metrics command tests
    // ========================================================================

    #[test]
    fn parse_metrics_query() {
        let cli = Cli::parse_from(["clawbernetes", "metrics", "query", "gpu_utilization{node=\"1\"}"]);
        match cli.command {
            Commands::Metrics { command: MetricsCommands::Query { expr, .. } } => {
                assert_eq!(expr, "gpu_utilization{node=\"1\"}");
            }
            _ => panic!("expected metrics query command"),
        }
    }

    #[test]
    fn parse_metrics_list() {
        let cli = Cli::parse_from(["clawbernetes", "metrics", "list", "-p", "gpu"]);
        match cli.command {
            Commands::Metrics { command: MetricsCommands::List { prefix } } => {
                assert_eq!(prefix, Some("gpu".into()));
            }
            _ => panic!("expected metrics list command"),
        }
    }

    // ========================================================================
    // Logs command tests
    // ========================================================================

    #[test]
    fn parse_logs_minimal() {
        let cli = Cli::parse_from(["clawbernetes", "logs", "my-job"]);
        match cli.command {
            Commands::Logs(args) => {
                assert_eq!(args.workload, "my-job");
                assert!(!args.follow);
                assert_eq!(args.tail, 100);
            }
            _ => panic!("expected logs command"),
        }
    }

    #[test]
    fn parse_logs_with_options() {
        let cli = Cli::parse_from([
            "clawbernetes", "logs", "my-job",
            "-f",
            "-t", "50",
            "-T",
            "--since", "1h"
        ]);
        match cli.command {
            Commands::Logs(args) => {
                assert_eq!(args.workload, "my-job");
                assert!(args.follow);
                assert_eq!(args.tail, 50);
                assert!(args.timestamps);
                assert_eq!(args.since, Some("1h".into()));
            }
            _ => panic!("expected logs command"),
        }
    }

    // ========================================================================
    // Dashboard command tests
    // ========================================================================

    #[test]
    fn parse_dashboard_start() {
        let cli = Cli::parse_from(["clawbernetes", "dashboard", "start"]);
        match cli.command {
            Commands::Dashboard { command: DashboardCommands::Start { port, bind, open } } => {
                assert_eq!(port, 8080);
                assert_eq!(bind, "127.0.0.1");
                assert!(!open);
            }
            _ => panic!("expected dashboard start command"),
        }
    }

    #[test]
    fn parse_dashboard_start_with_options() {
        let cli = Cli::parse_from([
            "clawbernetes", "dashboard", "start",
            "-p", "9000",
            "-b", "0.0.0.0",
            "--open"
        ]);
        match cli.command {
            Commands::Dashboard { command: DashboardCommands::Start { port, bind, open } } => {
                assert_eq!(port, 9000);
                assert_eq!(bind, "0.0.0.0");
                assert!(open);
            }
            _ => panic!("expected dashboard start command"),
        }
    }

    #[test]
    fn parse_dashboard_url() {
        let cli = Cli::parse_from(["clawbernetes", "dashboard", "url"]);
        assert!(matches!(
            cli.command,
            Commands::Dashboard { command: DashboardCommands::Url }
        ));
    }

    // ========================================================================
    // Preempt command tests
    // ========================================================================

    #[test]
    fn parse_preempt() {
        let cli = Cli::parse_from(["clawbernetes", "preempt", "low-priority-job", "-y"]);
        match cli.command {
            Commands::Preempt(args) => {
                assert_eq!(args.workload, "low-priority-job");
                assert!(args.yes);
            }
            _ => panic!("expected preempt command"),
        }
    }

    #[test]
    fn parse_preempt_with_reason() {
        let cli = Cli::parse_from([
            "clawbernetes", "preempt", "batch-job",
            "-r", "High priority job needs resources",
            "-y"
        ]);
        match cli.command {
            Commands::Preempt(args) => {
                assert_eq!(args.workload, "batch-job");
                assert_eq!(args.reason, Some("High priority job needs resources".into()));
            }
            _ => panic!("expected preempt command"),
        }
    }

    // ========================================================================
    // Priority command tests
    // ========================================================================

    #[test]
    fn parse_priority_list() {
        let cli = Cli::parse_from(["clawbernetes", "priority", "list"]);
        assert!(matches!(
            cli.command,
            Commands::Priority { command: PriorityCommands::List }
        ));
    }

    #[test]
    fn parse_priority_set() {
        let cli = Cli::parse_from(["clawbernetes", "priority", "set", "my-job", "750"]);
        match cli.command {
            Commands::Priority { command: PriorityCommands::Set { workload, priority, namespace } } => {
                assert_eq!(workload, "my-job");
                assert_eq!(priority, 750);
                assert!(namespace.is_none());
            }
            _ => panic!("expected priority set command"),
        }
    }

    #[test]
    fn parse_priority_get() {
        let cli = Cli::parse_from(["clawbernetes", "priority", "get", "my-job", "-n", "prod"]);
        match cli.command {
            Commands::Priority { command: PriorityCommands::Get { workload, namespace } } => {
                assert_eq!(workload, "my-job");
                assert_eq!(namespace, Some("prod".into()));
            }
            _ => panic!("expected priority get command"),
        }
    }
}
