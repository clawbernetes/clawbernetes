//! Observability handlers
//!
//! These handlers integrate with claw-metrics, claw-logs, and claw-alerts.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use claw_alerts::{
    AlertCondition, AlertManager, AlertRule, AlertSeverity as ClawAlertSeverity,
    ComparisonOperator, Silence,
};
use claw_logs::{LogFilter, LogLevel, LogStore, LogStoreConfig};
use claw_metrics::{MetricName, MetricStore, TimeRange};

use crate::error::{BridgeError, BridgeResult};
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Global State (would be injected in production)
// ─────────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref METRIC_STORE: MetricStore = MetricStore::new(Duration::from_secs(3600));
    static ref LOG_STORE: Arc<LogStore> = Arc::new(LogStore::with_config(LogStoreConfig::default()));
    static ref ALERT_MANAGER: AlertManager = AlertManager::new();
}

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct MetricPoint {
    pub timestamp: i64,
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
    pub timestamp: i64,
    pub level: String,
    pub message: String,
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
    pub fired_at: i64,
    pub resolved_at: Option<i64>,
    pub labels: Option<HashMap<String, String>>,
}

// ─────────────────────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct MetricsQueryParams {
    pub name: String,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub step: Option<i64>,
    pub labels: Option<HashMap<String, String>>,
}

/// Query metrics from claw-metrics
pub async fn metrics_query(params: Value) -> BridgeResult<Value> {
    let params: MetricsQueryParams = parse_params(params)?;

    let metric_name = MetricName::new(&params.name)
        .map_err(|e| BridgeError::InvalidParams(format!("invalid metric name: {e}")))?;

    let now = Utc::now().timestamp_millis();
    let start = params.start_time.unwrap_or(now - 3600_000); // Default 1 hour ago
    let end = params.end_time.unwrap_or(now);

    let range = TimeRange::new(start, end)
        .map_err(|e| BridgeError::InvalidParams(format!("invalid time range: {e}")))?;

    let points = METRIC_STORE
        .query(&metric_name, range, None)
        .map_err(|e| BridgeError::Internal(format!("query failed: {e}")))?;

    let series = vec![MetricSeries {
        name: params.name,
        points: points
            .into_iter()
            .map(|p| MetricPoint {
                timestamp: p.timestamp,
                value: p.value,
                labels: if p.labels.is_empty() {
                    None
                } else {
                    Some(p.labels)
                },
            })
            .collect(),
    }];

    to_json(series)
}

#[derive(Debug, Deserialize)]
pub struct LogsSearchParams {
    pub text: Option<String>,
    pub level: Option<String>,
    pub workload_id: Option<String>,
    pub node_id: Option<String>,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub limit: Option<u32>,
}

/// Search logs from claw-logs
pub async fn logs_search(params: Value) -> BridgeResult<Value> {
    let params: LogsSearchParams = parse_params(params)?;

    let mut filter = LogFilter::new();

    if let Some(text) = &params.text {
        filter = filter.with_contains(text);
    }

    if let Some(level) = &params.level {
        let log_level = match level.to_lowercase().as_str() {
            "trace" => LogLevel::Trace,
            "debug" => LogLevel::Debug,
            "info" => LogLevel::Info,
            "warn" | "warning" => LogLevel::Warn,
            "error" => LogLevel::Error,
            _ => {
                return Err(BridgeError::InvalidParams(format!(
                    "unknown log level: {level}"
                )))
            }
        };
        filter = filter.with_level(log_level);
    }

    if let Some(workload_id) = &params.workload_id {
        if let Ok(uuid) = uuid::Uuid::parse_str(workload_id) {
            filter = filter.with_workload(uuid);
        }
    }

    if let Some(node_id) = &params.node_id {
        if let Ok(uuid) = uuid::Uuid::parse_str(node_id) {
            filter = filter.with_node(uuid);
        }
    }

    let limit = params.limit.unwrap_or(100) as usize;

    let entries = LOG_STORE.query(&filter, limit);

    let logs: Vec<LogEntry> = entries
        .into_iter()
        .map(|e| LogEntry {
            timestamp: e.timestamp.timestamp_millis(),
            level: format!("{:?}", e.level).to_lowercase(),
            message: e.message.clone(),
            workload_id: Some(e.workload_id.to_string()),
            node_id: Some(e.node_id.to_string()),
            fields: if e.fields.is_empty() {
                None
            } else {
                Some(
                    e.fields
                        .iter()
                        .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or(Value::Null)))
                        .collect(),
                )
            },
        })
        .collect();

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

/// Create an alert rule in claw-alerts
pub async fn alert_create(params: Value) -> BridgeResult<Value> {
    let params: AlertCreateParams = parse_params(params)?;

    // Parse the condition (simple format: "metric_name > value")
    let parts: Vec<&str> = params.condition.split_whitespace().collect();
    if parts.len() != 3 {
        return Err(BridgeError::InvalidParams(
            "condition must be in format: 'metric_name operator value'".to_string(),
        ));
    }

    let metric = parts[0];
    let op = match parts[1] {
        ">" => ComparisonOperator::GreaterThan,
        ">=" => ComparisonOperator::GreaterThanOrEqual,
        "<" => ComparisonOperator::LessThan,
        "<=" => ComparisonOperator::LessThanOrEqual,
        "==" | "=" => ComparisonOperator::Equal,
        "!=" => ComparisonOperator::NotEqual,
        _ => {
            return Err(BridgeError::InvalidParams(format!(
                "unknown operator: {}",
                parts[1]
            )))
        }
    };
    let value: f64 = parts[2]
        .parse()
        .map_err(|_| BridgeError::InvalidParams(format!("invalid threshold: {}", parts[2])))?;

    let severity = match params.severity.to_lowercase().as_str() {
        "info" => ClawAlertSeverity::Info,
        "warning" | "warn" => ClawAlertSeverity::Warning,
        "critical" | "error" => ClawAlertSeverity::Critical,
        _ => {
            return Err(BridgeError::InvalidParams(format!(
                "unknown severity: {}",
                params.severity
            )))
        }
    };

    let condition = AlertCondition::new(metric, op, value)
        .map_err(|e| BridgeError::InvalidParams(format!("invalid condition: {e}")))?;

    let mut rule_builder = AlertRule::builder(&params.name, condition)
        .severity(severity)
        .annotation("message", &params.message);

    if let Some(labels) = &params.labels {
        for (k, v) in labels {
            rule_builder = rule_builder.label(k, v);
        }
    }

    let rule = rule_builder
        .build()
        .map_err(|e| BridgeError::InvalidParams(format!("invalid rule: {e}")))?;

    let rule_id = rule.name.clone();

    ALERT_MANAGER
        .add_rule(rule)
        .map_err(|e| BridgeError::Internal(format!("failed to add rule: {e}")))?;

    let now = Utc::now().timestamp_millis();

    let alert = Alert {
        id: rule_id.clone(),
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

/// List alerts from claw-alerts
pub async fn alert_list(params: Value) -> BridgeResult<Value> {
    let _params: AlertListParams = parse_params(params)?;

    // Get firing alerts from the manager
    let firing = ALERT_MANAGER.firing_alerts();

    let alerts: Vec<Alert> = firing
        .iter()
        .map(|a| {
            let severity = match a.severity {
                ClawAlertSeverity::Info => "info",
                ClawAlertSeverity::Warning => "warning",
                ClawAlertSeverity::Critical => "critical",
            };

            Alert {
                id: a.fingerprint.clone(),
                name: a.rule_name.clone(),
                severity: severity.to_string(),
                message: a
                    .annotations
                    .get("message")
                    .cloned()
                    .unwrap_or_else(|| a.rule_name.clone()),
                source: "clawbernetes".to_string(),
                fired_at: a.started_at.timestamp_millis(),
                resolved_at: a.resolved_at.map(|t| t.timestamp_millis()),
                labels: if a.labels.is_empty() {
                    None
                } else {
                    Some(a.labels.clone())
                },
            }
        })
        .collect();

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

    let now = Utc::now();
    let ends_at = now + chrono::Duration::seconds(params.duration_seconds as i64);

    // Create a silence
    let mut matchers = HashMap::new();
    matchers.insert("alertname".to_string(), params.alert_id.clone());

    let silence = Silence::new(
        matchers,
        now,
        ends_at,
        "bridge".to_string(),
        format!("Silenced via bridge for {} seconds", params.duration_seconds),
    )
    .map_err(|e| BridgeError::Internal(format!("failed to create silence: {e}")))?;

    ALERT_MANAGER
        .add_silence(silence)
        .map_err(|e| BridgeError::Internal(format!("failed to add silence: {e}")))?;

    tracing::info!(
        alert_id = %params.alert_id,
        duration_seconds = params.duration_seconds,
        "silenced alert"
    );

    to_json(AlertSilenceResult { success: true })
}
