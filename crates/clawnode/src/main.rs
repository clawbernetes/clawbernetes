//! clawnode - Clawbernetes GPU Node Agent
//!
//! This binary runs on GPU servers and connects to the OpenClaw gateway,
//! registering as a node with GPU capabilities.

use clap::{Parser, Subcommand};
use clawnode::{config::NodeConfig, create_state, GatewayClient, GpuManager};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{error, info};
#[cfg(feature = "network")]
use tracing::warn;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Parser)]
#[command(name = "clawnode")]
#[command(about = "Clawbernetes GPU Node Agent")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the node agent
    Run {
        /// Path to config file
        #[arg(short, long, default_value = "/etc/clawnode/config.json")]
        config: PathBuf,
    },
    
    /// Join a cluster using a bootstrap token
    Join {
        /// Gateway WebSocket URL
        #[arg(long)]
        gateway: String,
        
        /// Bootstrap token or auth token
        #[arg(long)]
        token: String,
        
        /// Node hostname (defaults to system hostname)
        #[arg(long)]
        hostname: Option<String>,
        
        /// Path to save config
        #[arg(long, default_value = "/etc/clawnode/config.json")]
        config: PathBuf,
    },
    
    /// Detect and list GPUs on this system
    GpuList,
    
    /// Get GPU metrics
    GpuMetrics,
    
    /// Show system information
    Info,
    
    /// Generate a sample config file
    InitConfig {
        /// Path to write config
        #[arg(short, long, default_value = "/etc/clawnode/config.json")]
        output: PathBuf,
        
        /// Gateway URL
        #[arg(long, default_value = "wss://localhost:18789")]
        gateway: String,
    },

    /// Execute an internal command (for use via system.run)
    ///
    /// Dispatches to the same handlers used by the WebSocket protocol,
    /// allowing all node commands to work through system.run.
    ///
    /// Examples:
    ///   clawnode exec gpu.list
    ///   clawnode exec gpu.metrics
    ///   clawnode exec system.info
    ///   clawnode exec node.health
    ///   clawnode exec node.capabilities
    ///   clawnode exec workload.list
    ///   clawnode exec workload.run --params '{"image":"nginx"}'
    Exec {
        /// Command name (e.g. gpu.list, gpu.metrics, system.info, workload.list)
        command: String,

        /// JSON parameters for the command (default: {})
        #[arg(long, default_value = "{}")]
        params: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // For exec commands, suppress tracing to keep stdout as clean JSON
    if !matches!(cli.command, Commands::Exec { .. }) {
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(EnvFilter::from_default_env().add_directive("clawnode=info".parse()?))
            .init();
    }
    
    match cli.command {
        Commands::Run { config } => {
            run_agent(config).await?;
        }
        
        Commands::Join {
            gateway,
            token,
            hostname: _hostname,
            config: _config,
        } => {
            join_cluster(gateway, token).await?;
        }
        
        Commands::GpuList => {
            gpu_list()?;
        }
        
        Commands::GpuMetrics => {
            gpu_metrics()?;
        }
        
        Commands::Info => {
            system_info()?;
        }
        
        Commands::InitConfig { output, gateway } => {
            init_config(output, gateway)?;
        }

        Commands::Exec { command, params } => {
            exec_command(&command, &params).await?;
        }
    }
    
    Ok(())
}

async fn exec_command(command: &str, params_str: &str) -> anyhow::Result<()> {
    use clawnode::commands::{handle_command, CommandRequest};

    let params: serde_json::Value = serde_json::from_str(params_str)
        .map_err(|e| anyhow::anyhow!("invalid JSON params: {e}"))?;

    // Build a lightweight state for local execution
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let config = NodeConfig {
        gateway: String::new(),
        token: None,
        hostname,
        labels: HashMap::new(),
        state_path: dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".clawnode"),
        heartbeat_interval_secs: 30,
        reconnect_delay_secs: 5,
        container_runtime: "docker".to_string(),
        network_enabled: false,
        region: "us-west".to_string(),
        wireguard_listen_port: 51820,
        ingress_listen_port: 8443,
        wireguard_endpoint: None,
    };

    let state = create_state(config);

    let request = CommandRequest {
        command: command.to_string(),
        params,
    };

    match handle_command(&state, request).await {
        Ok(result) => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Err(e) => {
            let err = serde_json::json!({
                "ok": false,
                "error": e.to_string(),
                "command": command,
            });
            println!("{}", serde_json::to_string_pretty(&err)?);
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn run_agent(config_path: PathBuf) -> anyhow::Result<()> {
    info!(config = %config_path.display(), "starting clawnode");
    
    let config = NodeConfig::load(&config_path)?;
    info!(
        gateway = %config.gateway,
        hostname = %config.hostname,
        "loaded config"
    );
    
    let state = create_state(config.clone());

    // Initialize networking if enabled and compiled in
    #[cfg(feature = "network")]
    if config.network_enabled {
        if let Err(e) = init_networking(&state, &config).await {
            warn!(error = %e, "networking unavailable, continuing without mesh");
        }
    }

    let identity_path = config.state_path.join("device.json");

    // Log capabilities
    info!(
        caps = ?state.capabilities,
        commands = ?state.commands,
        "node capabilities"
    );
    
    let mut client = GatewayClient::new(state, identity_path);
    
    // Connect with token if available
    let token = config.token.as_deref();
    
    loop {
        if let Err(e) = client.connect(&config.gateway, token).await {
            error!(error = %e, "connection error");
        }
        
        info!(delay = config.reconnect_delay_secs, "reconnecting in {} seconds", config.reconnect_delay_secs);
        tokio::time::sleep(std::time::Duration::from_secs(config.reconnect_delay_secs)).await;
    }
}

async fn join_cluster(gateway: String, token: String) -> anyhow::Result<()> {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    
    info!(
        gateway = %gateway,
        hostname = %hostname,
        "joining cluster"
    );
    
    // Use home directory for state on join
    let state_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".clawnode");
    
    std::fs::create_dir_all(&state_path)?;
    
    // Create minimal config for joining
    let config = NodeConfig {
        gateway: gateway.clone(),
        token: Some(token.clone()),
        hostname,
        labels: HashMap::new(),
        state_path: state_path.clone(),
        heartbeat_interval_secs: 30,
        reconnect_delay_secs: 5,
        container_runtime: "docker".to_string(),
        network_enabled: false,
        region: "us-west".to_string(),
        wireguard_listen_port: 51820,
        ingress_listen_port: 8443,
        wireguard_endpoint: None,
    };
    
    let state = create_state(config);
    let identity_path = state_path.join("device.json");
    
    // Log capabilities
    info!(
        caps = ?state.capabilities,
        commands = ?state.commands,
        "node capabilities"
    );
    
    let mut client = GatewayClient::new(state, identity_path);
    
    // Try to connect
    match client.connect(&gateway, Some(&token)).await {
        Ok(_) => {
            info!("joined cluster successfully");
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "failed to join cluster");
            anyhow::bail!("{}", e)
        }
    }
}

/// Initialize the networking stack: mesh, workload networking, service discovery,
/// network policies, and optionally the ingress proxy.
///
/// Any step that fails logs a warning and the function returns early with an error.
/// The node continues to operate without networking in that case.
#[cfg(feature = "network")]
async fn init_networking(
    state: &clawnode::SharedState,
    config: &NodeConfig,
) -> anyhow::Result<()> {
    use clawnode::{
        ingress_proxy::{IngressProxyConfig, start_proxy},
        mesh::{parse_region, MeshManager},
        netpolicy::PolicyEngine,
        service_discovery::ServiceDiscovery,
        workload_net::WorkloadNetManager,
    };

    // 1. Parse region
    let region = parse_region(&config.region);

    // 2. Generate WireGuard keypair
    let (_private_key, public_key) = claw_wireguard::generate_keypair();
    let pubkey_b64 = public_key.to_base64();

    // 3. Parse endpoint
    let endpoint = config
        .wireguard_endpoint
        .as_ref()
        .and_then(|ep| ep.parse::<std::net::SocketAddr>().ok());

    // 4. Init MeshManager
    let mesh_mgr = MeshManager::init(
        state.wireguard_mesh.clone(),
        region,
        pubkey_b64,
        endpoint,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    info!(
        mesh_ip = %mesh_mgr.mesh_ip(),
        region = %mesh_mgr.region(),
        "mesh networking initialized"
    );

    // 5. Init WorkloadNetManager from the mesh subnet
    let workload_subnet = mesh_mgr
        .workload_subnet()
        .ok_or_else(|| anyhow::anyhow!("workload subnet is not IPv4"))?;

    let wn_mgr = WorkloadNetManager::init(workload_subnet, mesh_mgr.interface_name())
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    info!(subnet = %workload_subnet, "workload networking initialized");

    // 6. Init ServiceDiscovery
    let sd = ServiceDiscovery::new();
    info!("service discovery initialized");

    // 7. Init PolicyEngine
    let pe = PolicyEngine::new();
    info!("network policy engine initialized");

    // Store all in shared state
    *state.mesh_manager.write().await = Some(mesh_mgr);
    *state.workload_net.write().await = Some(wn_mgr);
    *state.service_discovery.write().await = Some(sd);
    *state.policy_engine.write().await = Some(pe);

    // 8. Start ingress proxy (if port > 0)
    if config.ingress_listen_port > 0 {
        let proxy_config = IngressProxyConfig {
            listen_addr: ([0, 0, 0, 0], config.ingress_listen_port).into(),
        };
        let routes = state.ingress_routes.clone();
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            if let Err(e) = start_proxy(proxy_config, routes, shutdown_rx).await {
                warn!(error = %e, "ingress proxy stopped");
            }
        });
        info!(port = config.ingress_listen_port, "ingress proxy started");
    }

    Ok(())
}

fn gpu_list() -> anyhow::Result<()> {
    let manager = GpuManager::new();
    let gpus = manager.list();
    
    if gpus.is_empty() {
        println!("No GPUs detected");
        if !manager.has_nvidia() {
            println!("nvidia-smi not found - ensure NVIDIA drivers are installed");
        }
        return Ok(());
    }
    
    println!("Detected {} GPU(s):", gpus.len());
    println!();
    
    for gpu in &gpus {
        println!("  GPU {}: {}", gpu.index, gpu.name);
        println!("    UUID: {}", gpu.uuid);
        println!("    Memory: {} MB", gpu.memory_total_mb);
        if let Some(pci) = &gpu.pci_bus_id {
            println!("    PCI Bus: {}", pci);
        }
        println!();
    }
    
    println!("Total VRAM: {} GB", manager.total_memory_gb());
    
    Ok(())
}

fn gpu_metrics() -> anyhow::Result<()> {
    let manager = GpuManager::new();
    
    if !manager.has_nvidia() {
        println!("nvidia-smi not found - ensure NVIDIA drivers are installed");
        return Ok(());
    }
    
    let metrics = manager.get_metrics()?;
    
    if metrics.is_empty() {
        println!("No GPU metrics available");
        return Ok(());
    }
    
    println!("GPU Metrics:");
    println!();
    
    for m in &metrics {
        println!("  GPU {}: ", m.index);
        println!("    Utilization: {}%", m.utilization_percent);
        println!("    Memory: {} / {} MB ({:.1}%)", 
            m.memory_used_mb, 
            m.memory_total_mb,
            (m.memory_used_mb as f64 / m.memory_total_mb as f64) * 100.0
        );
        println!("    Temperature: {}°C", m.temperature_c);
        if let Some(power) = m.power_draw_w {
            print!("    Power: {:.1}W", power);
            if let Some(limit) = m.power_limit_w {
                print!(" / {:.1}W", limit);
            }
            println!();
        }
        println!();
    }
    
    Ok(())
}

fn system_info() -> anyhow::Result<()> {
    use sysinfo::System;
    
    let mut sys = System::new_all();
    sys.refresh_all();
    
    let gpu_manager = GpuManager::new();
    
    println!("System Information:");
    println!();
    println!("  Hostname: {}", hostname::get()?.to_string_lossy());
    println!("  OS: {} {}", 
        System::name().unwrap_or_default(),
        System::os_version().unwrap_or_default()
    );
    println!("  Kernel: {}", System::kernel_version().unwrap_or_default());
    println!();
    println!("  CPUs: {}", sys.cpus().len());
    println!("  Memory: {} / {} MB", 
        sys.used_memory() / 1024 / 1024,
        sys.total_memory() / 1024 / 1024
    );
    println!();
    println!("  GPUs: {}", gpu_manager.count());
    println!("  GPU Memory: {} GB", gpu_manager.total_memory_gb());
    println!();
    println!("  Capabilities: {:?}", gpu_manager.capabilities());
    println!("  Commands: {:?}", gpu_manager.commands());
    
    Ok(())
}

fn init_config(output: PathBuf, gateway: String) -> anyhow::Result<()> {
    let config = NodeConfig {
        gateway,
        token: None,
        hostname: hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string()),
        labels: HashMap::new(),
        state_path: PathBuf::from("/var/lib/clawnode"),
        heartbeat_interval_secs: 30,
        reconnect_delay_secs: 5,
        container_runtime: "docker".to_string(),
        network_enabled: false,
        region: "us-west".to_string(),
        wireguard_listen_port: 51820,
        ingress_listen_port: 8443,
        wireguard_endpoint: None,
    };
    
    config.save(&output)?;
    
    println!("Config written to {}", output.display());
    println!();
    println!("Edit the file to add your bootstrap token, then run:");
    println!("  clawnode run --config {}", output.display());
    
    Ok(())
}
