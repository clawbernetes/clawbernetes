//! claw-tui - Clawbernetes Terminal UI
//!
//! Read-only visualization of the Clawbernetes cluster.
//! No mutations possible - purely observational.

mod app;
mod data;
mod events;
mod ui;

use std::io;
use std::time::Duration;

use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use app::App;
use data::DataClient;
use events::{handle_key, AppEvent, EventHandler};

#[derive(Parser)]
#[command(name = "claw-tui")]
#[command(about = "Clawbernetes Terminal UI - Read-only cluster visualization")]
#[command(version)]
struct Cli {
    /// Path to claw-bridge binary
    #[arg(long, default_value = "claw-bridge")]
    bridge: String,
    
    /// Data poll interval in seconds
    #[arg(long, default_value = "2")]
    poll_interval: u64,
    
    /// Enable demo mode with fake data
    #[arg(long)]
    demo: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging (to file, not terminal)
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env().add_directive("claw_tui=debug".parse()?))
        .init();
    
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    // Run application
    let result = run_app(&mut terminal, cli).await;
    
    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    
    if let Err(err) = result {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
    
    Ok(())
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, cli: Cli) -> anyhow::Result<()> {
    let mut app = App::new();
    let tick_rate = Duration::from_millis(100);
    let poll_interval = Duration::from_secs(cli.poll_interval);
    
    let mut event_handler = EventHandler::new(tick_rate);
    
    // Start data polling in background
    if cli.demo {
        // Demo mode - inject fake data
        let tx = event_handler.sender();
        tokio::spawn(async move {
            run_demo_mode(tx).await;
        });
    } else {
        // Real mode - connect to bridge
        let tx = event_handler.sender();
        let bridge_path = cli.bridge.clone();
        tokio::spawn(async move {
            let client = DataClient::new(bridge_path, tx);
            client.run(poll_interval).await;
        });
    }
    
    // Main loop
    while app.running {
        // Draw UI
        terminal.draw(|frame| ui::draw(frame, &app))?;
        
        // Handle events
        if let Some(event) = event_handler.next().await {
            match event {
                AppEvent::Key(key) => {
                    handle_key(&mut app, key);
                }
                AppEvent::Resize(_, _) => {
                    // Terminal will redraw automatically
                }
                AppEvent::Tick => {
                    // Periodic tick - could update animations here
                }
                AppEvent::DataUpdate(data_event) => {
                    data::apply_data_event(&mut app, data_event);
                }
            }
        }
    }
    
    Ok(())
}

/// Demo mode - generates fake data for testing
async fn run_demo_mode(tx: tokio::sync::mpsc::UnboundedSender<AppEvent>) {
    use app::*;
    use events::DataEvent;
    use rand::Rng;
    use rand::SeedableRng;
    use serde_json::json;
    
    let mut rng = rand::rngs::StdRng::from_entropy();
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    
    // Initial demo data
    let demo_nodes = vec![
        ("gpu-node-1", "H100", 4),
        ("gpu-node-2", "H100", 4),
        ("gpu-node-3", "A100", 8),
        ("gpu-node-4", "A100", 8),
        ("gpu-node-5", "L40S", 4),
    ];
    
    let activity_messages = vec![
        (ActivityType::Deploy, "Deployed pytorch/pytorch:2.1-cuda12"),
        (ActivityType::Scale, "Scaling pool-prod: 3→5 nodes"),
        (ActivityType::Trade, "MOLT bid accepted: 4x H100 @ $2.41/hr"),
        (ActivityType::NodeJoin, "Node gpu-node-6 joined cluster"),
        (ActivityType::Preempt, "Preempting low-priority job j-abc123"),
        (ActivityType::Alert, "GPU temp >85°C on gpu-node-2"),
        (ActivityType::Info, "Autoscaler evaluation complete"),
    ];
    
    loop {
        interval.tick().await;
        
        // Update cluster state
        let running = rng.gen_range(10..20);
        let pending = rng.gen_range(0..5);
        let _ = tx.send(AppEvent::DataUpdate(DataEvent::ClusterUpdate(json!({
            "total_nodes": 5,
            "healthy_nodes": rng.gen_range(4..6),
            "total_gpus": 28,
            "available_gpus": rng.gen_range(5..15),
            "total_memory_gb": 560,
            "used_memory_gb": rng.gen_range(200..400),
            "running_workloads": running,
            "pending_workloads": pending,
            "completed_workloads": rng.gen_range(100..200),
            "failed_workloads": rng.gen_range(0..5),
        }))));
        
        // Update nodes
        let nodes: Vec<serde_json::Value> = demo_nodes.iter().map(|(name, model, gpu_count)| {
            let gpus: Vec<serde_json::Value> = (0..*gpu_count).map(|i| {
                json!({
                    "index": i,
                    "name": model,
                    "utilization_percent": rng.gen_range(0..100),
                    "memory_used_mb": rng.gen_range(10000..80000),
                    "memory_total_mb": 80000,
                    "temperature_c": rng.gen_range(45..85),
                    "power_draw_w": rng.gen_range(200.0..400.0),
                })
            }).collect();
            
            json!({
                "id": name,
                "name": name,
                "status": if rng.gen_bool(0.9) { "healthy" } else { "unhealthy" },
                "gpus": gpus,
                "cpu_percent": rng.gen_range(10.0..90.0),
                "memory_used_mb": rng.gen_range(32000..128000),
                "memory_total_mb": 256000,
                "workload_count": rng.gen_range(1..5),
            })
        }).collect();
        
        let _ = tx.send(AppEvent::DataUpdate(DataEvent::NodeUpdate(json!(nodes))));
        
        // Update workloads
        let workloads: Vec<serde_json::Value> = (0..rng.gen_range(5..15)).map(|i| {
            let states = ["pending", "running", "running", "running", "completed"];
            json!({
                "id": format!("wl-{:08x}", rng.r#gen::<u32>()),
                "name": format!("training-job-{}", i),
                "image": "pytorch/pytorch:2.1-cuda12",
                "state": states[rng.gen_range(0..states.len())],
                "gpu_count": rng.gen_range(1..8),
                "assigned_node": demo_nodes[rng.gen_range(0..demo_nodes.len())].0,
                "progress_percent": rng.gen_range(0..100),
            })
        }).collect();
        
        let _ = tx.send(AppEvent::DataUpdate(DataEvent::WorkloadUpdate(json!(workloads))));
        
        // Update market
        let prices = vec![
            json!({ "gpu_model": "H100", "price_per_hour": 2.41 + rng.gen_range(-0.2..0.2), "change_percent": rng.gen_range(-5.0..5.0), "available_count": rng.gen_range(10..50) }),
            json!({ "gpu_model": "A100", "price_per_hour": 1.85 + rng.gen_range(-0.1..0.1), "change_percent": rng.gen_range(-3.0..3.0), "available_count": rng.gen_range(20..100) }),
            json!({ "gpu_model": "L40S", "price_per_hour": 0.92 + rng.gen_range(-0.05..0.05), "change_percent": rng.gen_range(-2.0..2.0), "available_count": rng.gen_range(50..200) }),
            json!({ "gpu_model": "4090", "price_per_hour": 0.45 + rng.gen_range(-0.02..0.02), "change_percent": rng.gen_range(-1.0..1.0), "available_count": rng.gen_range(100..500) }),
        ];
        let _ = tx.send(AppEvent::DataUpdate(DataEvent::MarketUpdate(json!(prices))));
        
        // Add random activity
        if rng.gen_bool(0.7) {
            let (event_type, message) = activity_messages[rng.gen_range(0..activity_messages.len())];
            let _ = tx.send(AppEvent::DataUpdate(DataEvent::Activity(json!({
                "type": format!("{:?}", event_type).to_lowercase(),
                "message": message,
            }))));
        }
    }
}
