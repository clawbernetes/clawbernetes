//! clawnode - Clawbernetes GPU Node Agent
//!
//! This binary runs on GPU servers and connects to the OpenClaw gateway,
//! registering as a node with GPU capabilities.

use clap::{Parser, Subcommand};
use clawnode::{config::NodeConfig, create_state, GatewayClient, GpuManager};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{error, info};
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive("clawnode=info".parse()?))
        .init();
    
    let cli = Cli::parse();
    
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
    
    // Log capabilities
    info!(
        caps = ?state.capabilities,
        commands = ?state.commands,
        "node capabilities"
    );
    
    let mut client = GatewayClient::new(state);
    
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
    
    // Create minimal config for joining
    let config = NodeConfig {
        gateway: gateway.clone(),
        token: Some(token.clone()),
        hostname,
        labels: HashMap::new(),
        state_path: PathBuf::from("/var/lib/clawnode"),
        heartbeat_interval_secs: 30,
        reconnect_delay_secs: 5,
        container_runtime: "docker".to_string(),
    };
    
    let state = create_state(config);
    
    // Log capabilities
    info!(
        caps = ?state.capabilities,
        commands = ?state.commands,
        "node capabilities"
    );
    
    let mut client = GatewayClient::new(state);
    
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
    };
    
    config.save(&output)?;
    
    println!("Config written to {}", output.display());
    println!();
    println!("Edit the file to add your bootstrap token, then run:");
    println!("  clawnode run --config {}", output.display());
    
    Ok(())
}
