//! Observability handlers
//!
//! These handlers integrate with claw-metrics, claw-logs, and claw-alerts.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::error::BridgeResult;
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct MetricPoint {
    pub timestamp: u64,
    pub value: f64,
    pub labels: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricSeries {
    pub name: String,
    pub points: Vec<MetricPoint>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: u64,
    pub level: String,
    pub message: String,
    pub source: Option<String>,
    pub workload_id: Option<String>,
    pub node_id: Option<String>,
    pub fields: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Alert {
    pub id: String,
    pub name: String,
    pub severity: String,
    pub message: String,
    pub source: String,
    pub fired_at: u64,
    pub resolved_at: Option<u64>,
    pub labels: Option<HashMap<String, String>>,
}

// ─────────────────────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct MetricsQueryParams {
    pub name: String,
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,
    pub step: Option<u64>,
    pub labels: Option<HashMap<String, String>>,
}

/// Query metrics
pub async fn metrics_query(params: Value) -> BridgeResult<Value> {
    let params: MetricsQueryParams = parse_params(params)?;

    // TODO: Query from claw-metrics
    tracing::debug!(
        metric = %params.name,
        start = params.start_time,
        "querying metrics"
    );

    let series: Vec<MetricSeries> = vec![];

    to_json(series)
}

#[derive(Debug, Deserialize)]
pub struct LogsSearchParams {
    pub text: Option<String>,
    pub level: Option<String>,
    pub workload_id: Option<String>,
    pub node_id: Option<String>,
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,
    pub limit: Option<u32>,
}

/// Search logs
pub async fn logs_search(params: Value) -> BridgeResult<Value> {
    let params: LogsSearchParams = parse_params(params)?;

    // TODO: Query from claw-logs
    tracing::debug!(
        text = params.text.as_deref().unwrap_or("*"),
        limit = params.limit.unwrap_or(100),
        "searching logs"
    );

    let logs: Vec<LogEntry> = vec![];

    to_json(logs)
}

#[derive(Debug, Deserialize)]
pub struct AlertCreateParams {
    pub name: String,
    pub severity: String,
    pub condition: String,
    pub message: String,
    pub labels: Option<HashMap<String, String>>,
}

/// Create an alert rule
pub async fn alert_create(params: Value) -> BridgeResult<Value> {
    let params: AlertCreateParams = parse_params(params)?;

    // TODO: Create in claw-alerts
    tracing::info!(
        name = %params.name,
        severity = %params.severity,
        "creating alert"
    );

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let alert = Alert {
        id: format!("alert-{}", now),
        name: params.name,
        severity: params.severity,
        message: params.message,
        source: "clawbernetes-bridge".to_string(),
        fired_at: now,
        resolved_at: None,
        labels: params.labels,
    };

    to_json(alert)
}

#[derive(Debug, Deserialize)]
pub struct AlertListParams {
    pub severity: Option<String>,
    pub resolved: Option<bool>,
}

/// List alerts
pub async fn alert_list(params: Value) -> BridgeResult<Value> {
    let _params: AlertListParams = parse_params(params)?;

    // TODO: Query from claw-alerts
    let alerts: Vec<Alert> = vec![];

    to_json(alerts)
}

#[derive(Debug, Deserialize)]
pub struct AlertSilenceParams {
    pub alert_id: String,
    pub duration_seconds: u64,
}

#[derive(Debug, Serialize)]
pub struct AlertSilenceResult {
    pub success: bool,
}

/// Silence an alert
pub async fn alert_silence(params: Value) -> BridgeResult<Value> {
    let params: AlertSilenceParams = parse_params(params)?;

    // TODO: Silence in claw-alerts
    tracing::info!(
        alert_id = %params.alert_id,
        duration_seconds = params.duration_seconds,
        "silencing alert"
    );

    to_json(AlertSilenceResult { success: true })
}
