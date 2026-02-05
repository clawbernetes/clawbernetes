//! MOLT marketplace handlers
//!
//! These handlers integrate with molt-market for P2P GPU marketplace operations.

use std::collections::HashMap;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use molt_market::{
    CapacityOffer, EscrowAccount, GpuCapacity, JobOrder, JobRequirements, OrderBook,
};

use crate::error::{BridgeError, BridgeResult};
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Global State
// ─────────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref ORDER_BOOK: RwLock<OrderBook> = RwLock::new(OrderBook::new());
    static ref ESCROWS: RwLock<HashMap<String, EscrowAccount>> = RwLock::new(HashMap::new());
}

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct OfferInfo {
    pub id: String,
    pub provider: String,
    pub gpu_count: u32,
    pub gpu_model: String,
    pub memory_gb: u32,
    pub price_per_hour: u64,
    pub reputation: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderInfo {
    pub id: String,
    pub buyer: String,
    pub min_gpus: u32,
    pub gpu_model: Option<String>,
    pub min_memory_gb: u32,
    pub max_price: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchInfo {
    pub order_id: String,
    pub offer_id: String,
    pub score: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct EscrowInfo {
    pub id: String,
    pub job_id: String,
    pub buyer: String,
    pub provider: String,
    pub amount: u64,
    pub state: String,
    pub created_at: i64,
    pub expires_at: i64,
}

// ─────────────────────────────────────────────────────────────
// Offer Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct OffersParams {
    pub min_gpus: Option<u32>,
    pub max_price_per_hour: Option<u64>,
    pub gpu_model: Option<String>,
}

/// List available offers on the marketplace
pub async fn offers(params: Value) -> BridgeResult<Value> {
    let _params: OffersParams = parse_params(params)?;

    // OrderBook doesn't expose iteration, so we return empty for now
    // A real impl would track offer IDs separately
    let results: Vec<OfferInfo> = vec![];

    to_json(results)
}

#[derive(Debug, Deserialize)]
pub struct OfferCreateParams {
    pub provider: String,
    pub gpu_count: u32,
    pub gpu_model: String,
    pub memory_gb: u32,
    pub price_per_hour: u64,
    pub reputation: Option<u32>,
}

/// Create a new offer on the marketplace
pub async fn offer_create(params: Value) -> BridgeResult<Value> {
    let params: OfferCreateParams = parse_params(params)?;

    let gpus = GpuCapacity {
        count: params.gpu_count,
        model: params.gpu_model.clone(),
        memory_gb: params.memory_gb,
    };

    let offer = CapacityOffer::new(
        params.provider.clone(),
        gpus,
        params.price_per_hour,
        params.reputation.unwrap_or(100),
    );

    let offer_id = offer.id.clone();

    let mut book = ORDER_BOOK.write();
    book.insert_offer(offer);

    tracing::info!(
        offer_id = %offer_id,
        provider = %params.provider,
        gpus = params.gpu_count,
        "MOLT offer created"
    );

    to_json(OfferInfo {
        id: offer_id,
        provider: params.provider,
        gpu_count: params.gpu_count,
        gpu_model: params.gpu_model,
        memory_gb: params.memory_gb,
        price_per_hour: params.price_per_hour,
        reputation: params.reputation.unwrap_or(100),
    })
}

// ─────────────────────────────────────────────────────────────
// Order Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct OrderCreateParams {
    pub buyer: String,
    pub min_gpus: u32,
    pub gpu_model: Option<String>,
    pub min_memory_gb: Option<u32>,
    pub max_price: u64,
    pub max_duration_hours: Option<u32>,
}

/// Create a job order (bid request)
pub async fn order_create(params: Value) -> BridgeResult<Value> {
    let params: OrderCreateParams = parse_params(params)?;

    let requirements = JobRequirements {
        min_gpus: params.min_gpus,
        gpu_model: params.gpu_model.clone(),
        min_memory_gb: params.min_memory_gb.unwrap_or(0),
        max_duration_hours: params.max_duration_hours.unwrap_or(24),
    };

    let order = JobOrder::new(params.buyer.clone(), requirements, params.max_price);
    let order_id = order.id.clone();

    let mut book = ORDER_BOOK.write();
    book.insert_order(order);

    tracing::info!(
        order_id = %order_id,
        buyer = %params.buyer,
        gpus = params.min_gpus,
        "MOLT order created"
    );

    to_json(OrderInfo {
        id: order_id,
        buyer: params.buyer,
        min_gpus: params.min_gpus,
        gpu_model: params.gpu_model,
        min_memory_gb: params.min_memory_gb.unwrap_or(0),
        max_price: params.max_price,
    })
}

#[derive(Debug, Deserialize)]
pub struct MatchParams {
    pub order_id: String,
}

/// Find matching offers for an order
pub async fn find_matches(params: Value) -> BridgeResult<Value> {
    let params: MatchParams = parse_params(params)?;

    let book = ORDER_BOOK.read();
    let matches = book
        .find_matches(&params.order_id)
        .map_err(|e| BridgeError::NotFound(format!("order not found or no matches: {e}")))?;

    let infos: Vec<MatchInfo> = matches
        .iter()
        .map(|m| MatchInfo {
            order_id: m.order_id.clone(),
            offer_id: m.offer_id.clone(),
            score: m.score,
        })
        .collect();

    to_json(infos)
}

// ─────────────────────────────────────────────────────────────
// Escrow Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct EscrowCreateParams {
    pub job_id: String,
    pub buyer: String,
    pub provider: String,
    pub amount: u64,
}

/// Create an escrow for a job
pub async fn escrow_create(params: Value) -> BridgeResult<Value> {
    let params: EscrowCreateParams = parse_params(params)?;

    let escrow = EscrowAccount::new(
        params.job_id.clone(),
        params.buyer.clone(),
        params.provider.clone(),
        params.amount,
    );

    let escrow_id = format!("escrow-{}", params.job_id);

    let info = EscrowInfo {
        id: escrow_id.clone(),
        job_id: params.job_id.clone(),
        buyer: params.buyer.clone(),
        provider: params.provider.clone(),
        amount: params.amount,
        state: format!("{:?}", escrow.state),
        created_at: escrow.created_at.timestamp_millis(),
        expires_at: (escrow.created_at + escrow.timeout_duration).timestamp_millis(),
    };

    let mut escrows = ESCROWS.write();
    escrows.insert(escrow_id.clone(), escrow);

    tracing::info!(
        escrow_id = %escrow_id,
        job_id = %params.job_id,
        amount = params.amount,
        "escrow created"
    );

    to_json(info)
}

#[derive(Debug, Deserialize)]
pub struct EscrowFundParams {
    pub escrow_id: String,
    pub caller: String,
}

/// Fund an escrow
pub async fn escrow_fund(params: Value) -> BridgeResult<Value> {
    let params: EscrowFundParams = parse_params(params)?;

    let mut escrows = ESCROWS.write();
    let escrow = escrows
        .get_mut(&params.escrow_id)
        .ok_or_else(|| BridgeError::NotFound("escrow not found".to_string()))?;

    escrow
        .fund(&params.caller)
        .map_err(|e| BridgeError::Internal(format!("failed to fund escrow: {e}")))?;

    tracing::info!(escrow_id = %params.escrow_id, "escrow funded");

    to_json(serde_json::json!({ "success": true, "state": format!("{:?}", escrow.state) }))
}

#[derive(Debug, Deserialize)]
pub struct EscrowReleaseParams {
    pub escrow_id: String,
    pub caller: String,
}

/// Release escrow funds to provider
pub async fn escrow_release(params: Value) -> BridgeResult<Value> {
    let params: EscrowReleaseParams = parse_params(params)?;

    let mut escrows = ESCROWS.write();
    let escrow = escrows
        .get_mut(&params.escrow_id)
        .ok_or_else(|| BridgeError::NotFound("escrow not found".to_string()))?;

    escrow
        .release(&params.caller)
        .map_err(|e| BridgeError::Internal(format!("failed to release escrow: {e}")))?;

    tracing::info!(escrow_id = %params.escrow_id, "escrow released");

    to_json(serde_json::json!({ "success": true, "state": format!("{:?}", escrow.state) }))
}

#[derive(Debug, Deserialize)]
pub struct EscrowRefundParams {
    pub escrow_id: String,
    pub caller: String,
}

/// Refund escrow to buyer
pub async fn escrow_refund(params: Value) -> BridgeResult<Value> {
    let params: EscrowRefundParams = parse_params(params)?;

    let mut escrows = ESCROWS.write();
    let escrow = escrows
        .get_mut(&params.escrow_id)
        .ok_or_else(|| BridgeError::NotFound("escrow not found".to_string()))?;

    escrow
        .refund(&params.caller)
        .map_err(|e| BridgeError::Internal(format!("failed to refund escrow: {e}")))?;

    tracing::info!(escrow_id = %params.escrow_id, "escrow refunded");

    to_json(serde_json::json!({ "success": true, "state": format!("{:?}", escrow.state) }))
}

// ─────────────────────────────────────────────────────────────
// Legacy Handlers (kept for backward compatibility)
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BidParams {
    pub offer_id: String,
    pub price_per_hour: f64,
    pub duration_hours: u32,
}

/// Place a bid on an offer (legacy - use order_create instead)
pub async fn bid(params: Value) -> BridgeResult<Value> {
    let params: BidParams = parse_params(params)?;

    tracing::info!(
        offer_id = %params.offer_id,
        price = params.price_per_hour,
        duration = params.duration_hours,
        "MOLT bid placed (legacy)"
    );

    to_json(serde_json::json!({
        "status": "pending",
        "offer_id": params.offer_id,
        "note": "Use order_create + find_matches for the new flow"
    }))
}

#[derive(Debug, Deserialize)]
pub struct SpotPricesParams {
    pub gpu_model: Option<String>,
}

/// Get current spot prices (computed from order book)
pub async fn spot_prices(params: Value) -> BridgeResult<Value> {
    let _params: SpotPricesParams = parse_params(params)?;

    // In a real impl, we'd compute average prices from the order book
    let prices: Vec<serde_json::Value> = vec![];
    to_json(prices)
}
