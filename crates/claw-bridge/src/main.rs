//! Clawbernetes Bridge Binary
//!
//! JSON-RPC bridge for OpenClaw plugin integration.
//!
//! ## Usage
//!
//! The bridge reads JSON-RPC requests from stdin and writes responses to stdout:
//!
//! ```bash
//! echo '{"id":1,"method":"cluster_status","params":{}}' | claw-bridge
//! ```
//!
//! ## Protocol
//!
//! One JSON object per line (newline-delimited JSON).

use std::io::{self, BufRead, Write};

use claw_bridge::{handle_request, Request, Response};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> io::Result<()> {
    // Initialize logging to stderr (so stdout is clean for JSON-RPC)
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(io::stderr).with_ansi(false))
        .with(EnvFilter::from_default_env().add_directive("claw_bridge=info".parse().unwrap()))
        .init();

    tracing::info!("claw-bridge starting");

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("stdin read error: {}", e);
                break;
            }
        };

        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        // Parse request
        let request: Request = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let response = Response::error(0, claw_bridge::protocol::PARSE_ERROR, e.to_string());
                let json = serde_json::to_string(&response).unwrap_or_default();
                writeln!(stdout, "{json}")?;
                stdout.flush()?;
                continue;
            }
        };

        tracing::debug!(id = request.id, method = %request.method, "handling request");

        // Handle request
        let response = handle_request(request.id, &request.method, request.params).await;

        // Write response
        let json = serde_json::to_string(&response).unwrap_or_default();
        writeln!(stdout, "{json}")?;
        stdout.flush()?;

        tracing::debug!(id = request.id, "response sent");
    }

    tracing::info!("claw-bridge shutting down");
    Ok(())
}
