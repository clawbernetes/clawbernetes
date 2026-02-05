//! MOLT marketplace handlers
//!
//! These handlers integrate with molt-core, molt-market, and molt-p2p for
//! P2P GPU marketplace operations.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::BridgeResult;
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct MoltOffer {
    pub id: String,
    pub node_id: String,
    pub gpus: u32,
    pub gpu_model: String,
    pub price_per_hour: f64,
    pub min_duration_hours: Option<u32>,
    pub max_duration_hours: Option<u32>,
    pub region: String,
    pub available_at: u64,
    pub expires_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MoltBid {
    pub id: String,
    pub offer_id: String,
    pub bidder_id: String,
    pub price_per_hour: f64,
    pub duration_hours: u32,
    pub status: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpotPrice {
    pub region: String,
    pub gpu_model: String,
    pub price_per_hour: f64,
}

// ─────────────────────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct OffersParams {
    pub min_gpus: Option<u32>,
    pub max_price_per_hour: Option<f64>,
    pub region: Option<String>,
    pub gpu_model: Option<String>,
}

/// List available offers on the marketplace
pub async fn offers(params: Value) -> BridgeResult<Value> {
    let _params: OffersParams = parse_params(params)?;

    // TODO: Query from molt-market
    let offers: Vec<MoltOffer> = vec![];

    to_json(offers)
}

#[derive(Debug, Deserialize)]
pub struct OfferCreateParams {
    pub gpus: u32,
    pub gpu_model: String,
    pub price_per_hour: f64,
    pub min_duration_hours: Option<u32>,
    pub max_duration_hours: Option<u32>,
}

/// Create a new offer on the marketplace
pub async fn offer_create(params: Value) -> BridgeResult<Value> {
    let params: OfferCreateParams = parse_params(params)?;

    // TODO: Create via molt-market
    tracing::info!(
        gpus = params.gpus,
        gpu_model = %params.gpu_model,
        price = params.price_per_hour,
        "creating MOLT offer"
    );

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let offer = MoltOffer {
        id: format!("offer-{}", now),
        node_id: "local".to_string(),
        gpus: params.gpus,
        gpu_model: params.gpu_model,
        price_per_hour: params.price_per_hour,
        min_duration_hours: params.min_duration_hours,
        max_duration_hours: params.max_duration_hours,
        region: "unknown".to_string(),
        available_at: now,
        expires_at: None,
    };

    to_json(offer)
}

#[derive(Debug, Deserialize)]
pub struct BidParams {
    pub offer_id: String,
    pub price_per_hour: f64,
    pub duration_hours: u32,
}

/// Place a bid on an offer
pub async fn bid(params: Value) -> BridgeResult<Value> {
    let params: BidParams = parse_params(params)?;

    // TODO: Place bid via molt-market
    tracing::info!(
        offer_id = %params.offer_id,
        price = params.price_per_hour,
        duration = params.duration_hours,
        "placing MOLT bid"
    );

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let bid = MoltBid {
        id: format!("bid-{}", now),
        offer_id: params.offer_id,
        bidder_id: "self".to_string(),
        price_per_hour: params.price_per_hour,
        duration_hours: params.duration_hours,
        status: "pending".to_string(),
        created_at: now,
    };

    to_json(bid)
}

#[derive(Debug, Deserialize)]
pub struct SpotPricesParams {
    pub region: Option<String>,
    pub gpu_model: Option<String>,
}

/// Get current spot prices
pub async fn spot_prices(params: Value) -> BridgeResult<Value> {
    let _params: SpotPricesParams = parse_params(params)?;

    // TODO: Query from molt-market
    let prices: Vec<SpotPrice> = vec![];

    to_json(prices)
}
