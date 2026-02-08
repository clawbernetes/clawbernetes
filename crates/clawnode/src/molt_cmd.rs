//! MOLT marketplace command handlers
//!
//! Provides 5 commands (requires `molt` feature):
//! `molt.discover`, `molt.bid`, `molt.status`, `molt.balance`, `molt.reputation`

use crate::commands::{CommandError, CommandRequest};
use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

/// Route a molt.* command.
pub async fn handle_molt_command(
    state: &SharedState,
    request: CommandRequest,
) -> Result<Value, CommandError> {
    match request.command.as_str() {
        "molt.discover" => handle_molt_discover(state, request.params).await,
        "molt.bid" => handle_molt_bid(state, request.params).await,
        "molt.status" => handle_molt_status(state, request.params).await,
        "molt.balance" => handle_molt_balance(state).await,
        "molt.reputation" => handle_molt_reputation(state, request.params).await,
        _ => Err(format!("unknown molt command: {}", request.command).into()),
    }
}

#[derive(Debug, Deserialize)]
struct MoltDiscoverParams {
    #[serde(rename = "gpuType")]
    gpu_type: Option<String>,
    #[serde(rename = "minVram")]
    min_vram: Option<u32>,
    #[serde(rename = "maxPrice")]
    max_price: Option<u64>,
}

async fn handle_molt_discover(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: MoltDiscoverParams = serde_json::from_value(params)
        .unwrap_or(MoltDiscoverParams {
            gpu_type: None,
            min_vram: None,
            max_price: None,
        });

    info!(gpu_type = ?params.gpu_type, min_vram = ?params.min_vram, "discovering MOLT providers");

    let table = state.molt_peer_table.read().await;

    // Search by capability
    let capability = params.gpu_type.as_deref().unwrap_or("gpu");
    let peers = table.find_by_capability(capability);

    let results: Vec<Value> = peers
        .iter()
        .map(|p| {
            json!({
                "peerId": p.peer_id().to_string(),
                "capabilities": p.capabilities(),
            })
        })
        .collect();

    Ok(json!({
        "count": results.len(),
        "providers": results,
        "filter": {
            "gpuType": params.gpu_type,
            "minVram": params.min_vram,
            "maxPrice": params.max_price,
        },
    }))
}

#[derive(Debug, Deserialize)]
struct MoltBidParams {
    #[serde(rename = "providerId")]
    provider_id: String,
    #[serde(rename = "jobSpec")]
    job_spec: Value,
    #[serde(rename = "maxPrice")]
    max_price: u64,
}

async fn handle_molt_bid(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: MoltBidParams = serde_json::from_value(params)?;

    info!(provider = %params.provider_id, max_price = params.max_price, "submitting MOLT bid");

    // Create an order in the order book
    let min_gpus = params
        .job_spec
        .get("gpus")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;
    let min_memory_gb = params
        .job_spec
        .get("memoryGb")
        .and_then(|v| v.as_u64())
        .unwrap_or(8) as u32;
    let max_duration_hours = params
        .job_spec
        .get("durationHours")
        .and_then(|v| v.as_u64())
        .unwrap_or(24) as u32;

    let requirements = molt_market::JobRequirements {
        min_gpus,
        gpu_model: params.job_spec.get("gpuModel").and_then(|v| v.as_str()).map(String::from),
        min_memory_gb,
        max_duration_hours,
    };

    let order = molt_market::JobOrder::new(
        "clawnode".to_string(),
        requirements,
        params.max_price,
    );

    let order_id = order.id.clone();
    let mut book = state.molt_order_book.write().await;
    book.insert_order(order);

    Ok(json!({
        "orderId": order_id,
        "providerId": params.provider_id,
        "maxPrice": params.max_price,
        "state": "submitted",
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct MoltStatusParams {
    #[serde(rename = "jobId")]
    job_id: Option<String>,
    #[serde(rename = "orderId")]
    order_id: Option<String>,
}

async fn handle_molt_status(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: MoltStatusParams = serde_json::from_value(params)?;

    let id = params
        .order_id
        .or(params.job_id)
        .ok_or("orderId or jobId required")?;

    let book = state.molt_order_book.read().await;

    if let Some(order) = book.get_order(&id) {
        Ok(json!({
            "orderId": order.id,
            "buyer": order.buyer,
            "maxPrice": order.max_price,
            "state": "active",
        }))
    } else {
        Err(format!("order/job '{id}' not found").into())
    }
}

async fn handle_molt_balance(state: &SharedState) -> Result<Value, CommandError> {
    let wallet = state.molt_wallet.read().await;
    let pubkey = wallet.public_key();

    Ok(json!({
        "publicKey": hex::encode(pubkey.as_bytes()),
        "balance": "0 MOLT",
        "message": "balance tracking requires on-chain integration",
    }))
}

#[derive(Debug, Deserialize)]
struct MoltReputationParams {
    #[serde(rename = "peerId")]
    peer_id: String,
}

async fn handle_molt_reputation(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: MoltReputationParams = serde_json::from_value(params)?;

    let table = state.molt_peer_table.read().await;

    // Look up peer
    let peer_bytes: [u8; 32] = hex::decode(&params.peer_id)
        .map_err(|e| format!("invalid peer ID hex: {e}"))?
        .try_into()
        .map_err(|_| "peer ID must be 32 bytes (64 hex chars)")?;
    let peer_id = molt_p2p::PeerId::from_bytes(peer_bytes);

    let peer = table
        .get(&peer_id)
        .ok_or_else(|| format!("peer '{}' not found", params.peer_id))?;

    Ok(json!({
        "peerId": params.peer_id,
        "capabilities": peer.capabilities(),
        "reputation": "unknown",
        "message": "reputation scoring requires attestation history",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NodeConfig;

    fn test_state() -> SharedState {
        let mut config = NodeConfig::default();
        let dir = tempfile::tempdir().expect("tempdir");
        config.state_path = dir.path().to_path_buf();
        std::mem::forget(dir);
        SharedState::new(config)
    }

    #[tokio::test]
    async fn test_molt_discover() {
        let state = test_state();

        let result = handle_molt_command(
            &state,
            CommandRequest {
                command: "molt.discover".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("discover");
        assert_eq!(result["count"], 0);
    }

    #[tokio::test]
    async fn test_molt_bid() {
        let state = test_state();

        let result = handle_molt_command(
            &state,
            CommandRequest {
                command: "molt.bid".to_string(),
                params: json!({
                    "providerId": "peer-123",
                    "jobSpec": {"gpus": 2, "memoryGb": 16, "durationHours": 8},
                    "maxPrice": 1000,
                }),
            },
        )
        .await
        .expect("bid");
        assert_eq!(result["success"], true);
        assert!(!result["orderId"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_molt_status() {
        let state = test_state();

        // Create a bid first
        let bid_result = handle_molt_command(
            &state,
            CommandRequest {
                command: "molt.bid".to_string(),
                params: json!({
                    "providerId": "peer-456",
                    "jobSpec": {"gpus": 1},
                    "maxPrice": 500,
                }),
            },
        )
        .await
        .expect("bid");

        let order_id = bid_result["orderId"].as_str().unwrap();

        let result = handle_molt_command(
            &state,
            CommandRequest {
                command: "molt.status".to_string(),
                params: json!({"orderId": order_id}),
            },
        )
        .await
        .expect("status");
        assert_eq!(result["state"], "active");
    }

    #[tokio::test]
    async fn test_molt_balance() {
        let state = test_state();

        let result = handle_molt_command(
            &state,
            CommandRequest {
                command: "molt.balance".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("balance");
        assert!(!result["publicKey"].as_str().unwrap().is_empty());
    }
}
