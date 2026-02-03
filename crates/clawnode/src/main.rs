#![allow(clippy::expect_used)]
//! Clawnode binary entrypoint.
//!
//! The Clawbernetes node agent that runs on compute nodes.

use std::path::PathBuf;

use clap::Parser;
use tracing::info;

use clawnode::config::NodeConfig;
use clawnode::error::NodeError;
use clawnode::node::Node;

/// Clawnode - Clawbernetes Node Agent
///
/// Runs on compute nodes to provide GPU resources to the cluster.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[command(name = "clawnode")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Gateway WebSocket URL to connect to.
    ///
    /// Must be a ws:// or wss:// URL.
    #[arg(short, long, env = "CLAWNODE_GATEWAY")]
    pub gateway: Option<String>,

    /// Path to configuration file (TOML format).
    #[arg(short, long, env = "CLAWNODE_CONFIG")]
    pub config: Option<PathBuf>,

    /// Node name (identifier for this node).
    ///
    /// Must be alphanumeric with hyphens and underscores only.
    #[arg(short, long, env = "CLAWNODE_NAME")]
    pub name: Option<String>,

    /// Enable verbose logging.
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,
}

impl Cli {
    /// Build a `NodeConfig` from CLI arguments, optionally loading from file.
    ///
    /// Priority: CLI args > config file > defaults
    ///
    /// # Errors
    ///
    /// Returns an error if required fields are missing or validation fails.
    pub fn build_config(&self) -> Result<NodeConfig, NodeError> {
        // Start with config from file if provided, otherwise use defaults
        let mut config = match &self.config {
            Some(path) => NodeConfig::from_file(path)?,
            None => NodeConfig {
                name: String::new(),
                gateway_url: String::new(),
                gpu: Default::default(),
                molt: Default::default(),
            },
        };

        // Override with CLI arguments
        if let Some(gateway) = &self.gateway {
            config.gateway_url.clone_from(gateway);
        }

        if let Some(name) = &self.name {
            config.name.clone_from(name);
        }

        // Validate required fields
        if config.name.is_empty() {
            return Err(NodeError::Config(
                "node name is required (use --name or config file)".to_string(),
            ));
        }

        if config.gateway_url.is_empty() {
            return Err(NodeError::Config(
                "gateway URL is required (use --gateway or config file)".to_string(),
            ));
        }

        // Run full validation
        config.validate()?;

        Ok(config)
    }
}

/// Initialize tracing/logging based on verbosity.
fn init_tracing(verbose: bool) {
    use tracing_subscriber::EnvFilter;

    let filter = if verbose {
        EnvFilter::new("clawnode=debug,info")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("clawnode=info,warn"))
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .init();
}

/// Run the node agent.
///
/// # Errors
///
/// Returns an error if the node fails to start or run.
async fn run(cli: Cli) -> Result<(), NodeError> {
    init_tracing(cli.verbose);

    info!("clawnode starting...");

    let config = cli.build_config()?;
    info!(name = %config.name, gateway = %config.gateway_url, "configuration loaded");

    let node = Node::new(config).await?;
    info!("node initialized, starting main loop");

    node.run().await
}

fn main() {
    let cli = Cli::parse();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    if let Err(e) = runtime.block_on(run(cli)) {
        eprintln!("clawnode error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Helper to parse args without the binary name
    fn parse_args(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut full_args = vec!["clawnode"];
        full_args.extend(args);
        Cli::try_parse_from(full_args)
    }

    #[test]
    fn test_parse_no_args() {
        let cli = parse_args(&[]).expect("should parse empty args");
        assert!(cli.gateway.is_none());
        assert!(cli.config.is_none());
        assert!(cli.name.is_none());
        assert!(!cli.verbose);
    }

    #[test]
    fn test_parse_gateway_short() {
        let cli = parse_args(&["-g", "wss://gateway.example.com"]).expect("should parse");
        assert_eq!(cli.gateway, Some("wss://gateway.example.com".to_string()));
    }

    #[test]
    fn test_parse_gateway_long() {
        let cli = parse_args(&["--gateway", "ws://localhost:8080"]).expect("should parse");
        assert_eq!(cli.gateway, Some("ws://localhost:8080".to_string()));
    }

    #[test]
    fn test_parse_config_short() {
        let cli = parse_args(&["-c", "/etc/clawnode/config.toml"]).expect("should parse");
        assert_eq!(cli.config, Some(PathBuf::from("/etc/clawnode/config.toml")));
    }

    #[test]
    fn test_parse_config_long() {
        let cli = parse_args(&["--config", "./config.toml"]).expect("should parse");
        assert_eq!(cli.config, Some(PathBuf::from("./config.toml")));
    }

    #[test]
    fn test_parse_name_short() {
        let cli = parse_args(&["-n", "gpu-node-01"]).expect("should parse");
        assert_eq!(cli.name, Some("gpu-node-01".to_string()));
    }

    #[test]
    fn test_parse_name_long() {
        let cli = parse_args(&["--name", "my_node"]).expect("should parse");
        assert_eq!(cli.name, Some("my_node".to_string()));
    }

    #[test]
    fn test_parse_verbose_short() {
        let cli = parse_args(&["-v"]).expect("should parse");
        assert!(cli.verbose);
    }

    #[test]
    fn test_parse_verbose_long() {
        let cli = parse_args(&["--verbose"]).expect("should parse");
        assert!(cli.verbose);
    }

    #[test]
    fn test_parse_all_args() {
        let cli = parse_args(&[
            "--gateway",
            "wss://gateway.example.com:8080",
            "--config",
            "/etc/clawnode.toml",
            "--name",
            "gpu-node-01",
            "--verbose",
        ])
        .expect("should parse");

        assert_eq!(
            cli.gateway,
            Some("wss://gateway.example.com:8080".to_string())
        );
        assert_eq!(cli.config, Some(PathBuf::from("/etc/clawnode.toml")));
        assert_eq!(cli.name, Some("gpu-node-01".to_string()));
        assert!(cli.verbose);
    }

    #[test]
    fn test_parse_combined_short_args() {
        let cli = parse_args(&["-g", "ws://localhost", "-n", "node1", "-v"]).expect("should parse");

        assert_eq!(cli.gateway, Some("ws://localhost".to_string()));
        assert_eq!(cli.name, Some("node1".to_string()));
        assert!(cli.verbose);
    }

    #[test]
    fn test_build_config_from_cli_args() {
        let cli = Cli {
            gateway: Some("wss://gateway.example.com".to_string()),
            config: None,
            name: Some("test-node".to_string()),
            verbose: false,
        };

        let config = cli.build_config().expect("should build config");

        assert_eq!(config.name, "test-node");
        assert_eq!(config.gateway_url, "wss://gateway.example.com");
    }

    #[test]
    fn test_build_config_missing_name() {
        let cli = Cli {
            gateway: Some("wss://gateway.example.com".to_string()),
            config: None,
            name: None,
            verbose: false,
        };

        let result = cli.build_config();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("node name is required"));
    }

    #[test]
    fn test_build_config_missing_gateway() {
        let cli = Cli {
            gateway: None,
            config: None,
            name: Some("test-node".to_string()),
            verbose: false,
        };

        let result = cli.build_config();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("gateway URL is required"));
    }

    #[test]
    fn test_build_config_from_file() {
        let toml = r#"
            name = "file-node"
            gateway_url = "wss://file-gateway.example.com"
        "#;

        let mut file = NamedTempFile::new().expect("create temp file");
        file.write_all(toml.as_bytes()).expect("write temp file");

        let cli = Cli {
            gateway: None,
            config: Some(file.path().to_path_buf()),
            name: None,
            verbose: false,
        };

        let config = cli.build_config().expect("should build config");

        assert_eq!(config.name, "file-node");
        assert_eq!(config.gateway_url, "wss://file-gateway.example.com");
    }

    #[test]
    fn test_build_config_cli_overrides_file() {
        let toml = r#"
            name = "file-node"
            gateway_url = "wss://file-gateway.example.com"
        "#;

        let mut file = NamedTempFile::new().expect("create temp file");
        file.write_all(toml.as_bytes()).expect("write temp file");

        let cli = Cli {
            gateway: Some("wss://cli-gateway.example.com".to_string()),
            config: Some(file.path().to_path_buf()),
            name: Some("cli-node".to_string()),
            verbose: false,
        };

        let config = cli.build_config().expect("should build config");

        // CLI args should override file values
        assert_eq!(config.name, "cli-node");
        assert_eq!(config.gateway_url, "wss://cli-gateway.example.com");
    }

    #[test]
    fn test_build_config_partial_cli_override() {
        let toml = r#"
            name = "file-node"
            gateway_url = "wss://file-gateway.example.com"
        "#;

        let mut file = NamedTempFile::new().expect("create temp file");
        file.write_all(toml.as_bytes()).expect("write temp file");

        let cli = Cli {
            gateway: None,
            config: Some(file.path().to_path_buf()),
            name: Some("cli-node".to_string()), // Only override name
            verbose: false,
        };

        let config = cli.build_config().expect("should build config");

        assert_eq!(config.name, "cli-node"); // From CLI
        assert_eq!(config.gateway_url, "wss://file-gateway.example.com"); // From file
    }

    #[test]
    fn test_build_config_invalid_name() {
        let cli = Cli {
            gateway: Some("wss://gateway.example.com".to_string()),
            config: None,
            name: Some("node with spaces".to_string()),
            verbose: false,
        };

        let result = cli.build_config();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("alphanumeric"));
    }

    #[test]
    fn test_build_config_invalid_gateway_scheme() {
        let cli = Cli {
            gateway: Some("http://gateway.example.com".to_string()),
            config: None,
            name: Some("test-node".to_string()),
            verbose: false,
        };

        let result = cli.build_config();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("ws://"));
    }

    #[test]
    fn test_build_config_nonexistent_file() {
        let cli = Cli {
            gateway: None,
            config: Some(PathBuf::from("/nonexistent/path/config.toml")),
            name: None,
            verbose: false,
        };

        let result = cli.build_config();
        assert!(result.is_err());
    }

    #[test]
    fn test_cli_equality() {
        let cli1 = Cli {
            gateway: Some("ws://test".to_string()),
            config: None,
            name: Some("node1".to_string()),
            verbose: true,
        };

        let cli2 = Cli {
            gateway: Some("ws://test".to_string()),
            config: None,
            name: Some("node1".to_string()),
            verbose: true,
        };

        assert_eq!(cli1, cli2);
    }

    #[test]
    fn test_cli_debug() {
        let cli = Cli {
            gateway: Some("ws://test".to_string()),
            config: None,
            name: Some("node1".to_string()),
            verbose: false,
        };

        let debug_str = format!("{cli:?}");
        assert!(debug_str.contains("gateway"));
        assert!(debug_str.contains("ws://test"));
    }
}
