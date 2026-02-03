//! Clawbernetes Gateway Server binary.
//!
//! The gateway is the control plane for a Clawbernetes cluster.

use claw_gateway_server::{GatewayServer, ServerConfig};
use std::net::SocketAddr;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Parse command line args
    let args: Vec<String> = std::env::args().collect();

    let bind_addr: SocketAddr = args
        .get(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| "0.0.0.0:8080".parse().expect("valid default addr"));

    info!("Starting Clawbernetes Gateway on {}", bind_addr);
    info!("  WebSocket endpoint: ws://{}/", bind_addr);
    info!("  Nodes connect via:  CLAWNODE_GATEWAY=ws://{}/", bind_addr);

    // Create gateway config
    let config = ServerConfig::new(bind_addr).with_max_connections(1000);

    // Create and run gateway
    let mut server = GatewayServer::new(config);

    // Run until error or shutdown
    if let Err(e) = server.serve(bind_addr).await {
        error!("Gateway error: {}", e);
        std::process::exit(1);
    }
}
