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
        
        /// Bootstrap token
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
            hostname,
            config,
        } => {
            join_cluster(gateway, token, hostname, config).await?;
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
    
    let state = create_state(config);
    
    // Log GPU info
    {
        let s = state.read().await;
        info!(
            gpu_count = s.gpu_manager.count(),
            caps = ?s.gpu_manager.capabilities(),
            "detected hardware"
        );
    }
    
    let mut client = GatewayClient::new(state);
    
    if let Err(e) = client.run().await {
        error!(error = %e, "agent error");
        return Err(e);
    }
    
    Ok(())
}

async fn join_cluster(
    gateway: String,
    token: String,
    hostname: Option<String>,
    config_path: PathBuf,
) -> anyhow::Result<()> {
    let hostname = hostname.unwrap_or_else(|| {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    });
    
    info!(
        gateway = %gateway,
        hostname = %hostname,
        "joining cluster"
    );
    
    // Create config
    let config = NodeConfig {
        gateway,
        token: Some(token),
        hostname,
        labels: HashMap::new(),
        state_path: PathBuf::from("/var/lib/clawnode"),
        heartbeat_interval_secs: 30,
        reconnect_delay_secs: 5,
        container_runtime: "docker".to_string(),
    };
    
    // Save config
    config.save(&config_path)?;
    info!(path = %config_path.display(), "saved config");
    
    println!("Configuration saved to {}", config_path.display());
    println!();
    println!("To start the agent:");
    println!("  clawnode run --config {}", config_path.display());
    println!();
    println!("Or install as a systemd service:");
    println!("  sudo systemctl enable clawnode");
    println!("  sudo systemctl start clawnode");
    
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
    };
    
    config.save(&output)?;
    
    println!("Config written to {}", output.display());
    println!();
    println!("Edit the file to add your bootstrap token, then run:");
    println!("  clawnode run --config {}", output.display());
    
    Ok(())
}
