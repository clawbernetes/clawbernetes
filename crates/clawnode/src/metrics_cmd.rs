//! Metrics, events, and alerts command handlers
//!
//! Wraps `claw_metrics::MetricStore` for time-series data, plus in-memory
//! event log and alert rules backed by JSON snapshots.
//!
//! Commands:
//! - `metrics.query`, `metrics.list`, `metrics.snapshot`
//! - `events.query`, `events.emit`
//! - `alerts.create`, `alerts.list`, `alerts.acknowledge`

use crate::commands::{CommandError, CommandRequest};
use crate::persist::AlertRule;
use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

/// Route a metrics/events/alerts command.
pub async fn handle_metrics_command(
    state: &SharedState,
    request: CommandRequest,
) -> Result<Value, CommandError> {
    match request.command.as_str() {
        "metrics.query" => handle_metrics_query(state, request.params).await,
        "metrics.list" => handle_metrics_list(state).await,
        "metrics.snapshot" => handle_metrics_snapshot(state).await,
        "events.query" => handle_events_query(state, request.params).await,
        "events.emit" => handle_events_emit(state, request.params).await,
        "alerts.create" => handle_alerts_create(state, request.params).await,
        "alerts.list" => handle_alerts_list(state).await,
        "alerts.acknowledge" => handle_alerts_acknowledge(state, request.params).await,
        _ => Err(format!("unknown metrics command: {}", request.command).into()),
    }
}

// ─────────────────────────────────────────────────────────────
// Metrics Commands
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct MetricsQueryParams {
    name: String,
    #[serde(rename = "rangeMinutes")]
    range_minutes: Option<i64>,
    aggregation: Option<String>,
}

async fn handle_metrics_query(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: MetricsQueryParams = serde_json::from_value(params)?;

    let name = claw_metrics::MetricName::new(&params.name)
        .map_err(|e| format!("invalid metric name: {e}"))?;

    let range = claw_metrics::TimeRange::last_minutes(params.range_minutes.unwrap_or(60));

    let aggregation = params.aggregation.as_deref().map(parse_aggregation).transpose()?;

    let points = state
        .metric_store
        .query(&name, range, aggregation)
        .map_err(|e| format!("query failed: {e}"))?;

    let data: Vec<Value> = points
        .iter()
        .map(|p| {
            json!({
                "timestamp": p.timestamp,
                "value": p.value,
                "labels": p.labels,
            })
        })
        .collect();

    Ok(json!({
        "name": params.name,
        "count": data.len(),
        "points": data,
    }))
}

fn parse_aggregation(s: &str) -> Result<claw_metrics::Aggregation, CommandError> {
    match s.to_lowercase().as_str() {
        "sum" => Ok(claw_metrics::Aggregation::Sum),
        "avg" | "average" => Ok(claw_metrics::Aggregation::Avg),
        "min" => Ok(claw_metrics::Aggregation::Min),
        "max" => Ok(claw_metrics::Aggregation::Max),
        "last" => Ok(claw_metrics::Aggregation::Last),
        "count" => Ok(claw_metrics::Aggregation::Count),
        _ => Err(format!("unknown aggregation: {s} (use sum/avg/min/max/last/count)").into()),
    }
}

async fn handle_metrics_list(state: &SharedState) -> Result<Value, CommandError> {
    let names = state.metric_store.metrics_list();

    let metrics: Vec<Value> = names
        .iter()
        .map(|n| {
            json!({
                "name": n.as_str(),
                "points": state.metric_store.metric_count(n),
            })
        })
        .collect();

    Ok(json!({
        "count": metrics.len(),
        "metrics": metrics,
    }))
}

async fn handle_metrics_snapshot(state: &SharedState) -> Result<Value, CommandError> {
    let names = state.metric_store.metrics_list();

    let mut snapshot = serde_json::Map::new();
    for name in &names {
        if let Some(val) = claw_metrics::last_value(&state.metric_store, name) {
            snapshot.insert(name.as_str().to_string(), json!(val));
        }
    }

    Ok(json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "metrics": snapshot,
    }))
}

// ─────────────────────────────────────────────────────────────
// Events Commands (in-memory log via MetricStore with special naming)
// We use the metric store with "event.*" names for simplicity,
// storing event messages as labels on zero-value points.
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct EventsQueryParams {
    severity: Option<String>,
    source: Option<String>,
    #[serde(rename = "rangeMinutes")]
    range_minutes: Option<i64>,
    limit: Option<usize>,
}

async fn handle_events_query(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: EventsQueryParams =
        serde_json::from_value(params).unwrap_or(EventsQueryParams {
            severity: None,
            source: None,
            range_minutes: None,
            limit: None,
        });

    let range = claw_metrics::TimeRange::last_minutes(params.range_minutes.unwrap_or(60));

    // Query the events metric
    let name = claw_metrics::MetricName::new("_events")
        .map_err(|e| format!("internal error: {e}"))?;

    let points = state
        .metric_store
        .query(&name, range, None)
        .unwrap_or_default();

    let mut events: Vec<Value> = points
        .iter()
        .filter(|p| {
            if let Some(ref sev) = params.severity {
                p.labels.get("severity").is_some_and(|s| s == sev)
            } else {
                true
            }
        })
        .filter(|p| {
            if let Some(ref src) = params.source {
                p.labels.get("source").is_some_and(|s| s == src)
            } else {
                true
            }
        })
        .map(|p| {
            json!({
                "timestamp": p.timestamp,
                "source": p.labels.get("source").unwrap_or(&String::new()),
                "severity": p.labels.get("severity").unwrap_or(&String::new()),
                "message": p.labels.get("message").unwrap_or(&String::new()),
            })
        })
        .collect();

    // Most recent first
    events.reverse();

    if let Some(limit) = params.limit {
        events.truncate(limit);
    }

    Ok(json!({
        "count": events.len(),
        "events": events,
    }))
}

#[derive(Debug, Deserialize)]
struct EventsEmitParams {
    source: String,
    severity: String,
    message: String,
}

async fn handle_events_emit(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: EventsEmitParams = serde_json::from_value(params)?;

    // Validate severity
    if !["info", "warning", "error"].contains(&params.severity.as_str()) {
        return Err("severity must be one of: info, warning, error".into());
    }

    info!(source = %params.source, severity = %params.severity, "emitting event");

    let name = claw_metrics::MetricName::new("_events")
        .map_err(|e| format!("internal error: {e}"))?;

    let point = claw_metrics::MetricPoint::now(0.0)
        .label("source", &params.source)
        .label("severity", &params.severity)
        .label("message", &params.message);

    state
        .metric_store
        .push(&name, point)
        .map_err(|e| format!("failed to emit event: {e}"))?;

    Ok(json!({
        "emitted": true,
        "source": params.source,
        "severity": params.severity,
    }))
}

// ─────────────────────────────────────────────────────────────
// Alerts Commands
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AlertCreateParams {
    name: String,
    metric: String,
    condition: String,
    threshold: f64,
}

async fn handle_alerts_create(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: AlertCreateParams = serde_json::from_value(params)?;

    if !["above", "below"].contains(&params.condition.as_str()) {
        return Err("condition must be 'above' or 'below'".into());
    }

    info!(name = %params.name, metric = %params.metric, "creating alert rule");

    let now = chrono::Utc::now();
    let rule = AlertRule {
        name: params.name.clone(),
        metric: params.metric.clone(),
        condition: params.condition.clone(),
        threshold: params.threshold,
        state: "ok".to_string(),
        created_at: now,
        updated_at: now,
    };

    let mut store = state.alert_store.write().await;
    store
        .create(rule)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "name": params.name,
        "metric": params.metric,
        "condition": params.condition,
        "threshold": params.threshold,
        "success": true,
    }))
}

async fn handle_alerts_list(state: &SharedState) -> Result<Value, CommandError> {
    let store = state.alert_store.read().await;
    let alerts: Vec<Value> = store
        .list()
        .iter()
        .map(|a| {
            json!({
                "name": a.name,
                "metric": a.metric,
                "condition": a.condition,
                "threshold": a.threshold,
                "state": a.state,
                "created_at": a.created_at.to_rfc3339(),
                "updated_at": a.updated_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(json!({
        "count": alerts.len(),
        "alerts": alerts,
    }))
}

#[derive(Debug, Deserialize)]
struct AlertAcknowledgeParams {
    #[serde(rename = "alertId")]
    alert_id: Option<String>,
    name: Option<String>,
}

async fn handle_alerts_acknowledge(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: AlertAcknowledgeParams = serde_json::from_value(params)?;

    let name = params
        .name
        .or(params.alert_id)
        .ok_or("name or alertId required")?;

    let mut store = state.alert_store.write().await;
    store
        .acknowledge(&name)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "name": name,
        "acknowledged": true,
    }))
}

/// Push system metrics into the store. Called from a background task.
pub fn collect_system_metrics(state: &SharedState) {
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_all();

    let store = &state.metric_store;

    // CPU usage
    let cpu_count = sys.cpus().len() as f64;
    let load = System::load_average();
    let cpu_usage = (load.one / cpu_count) * 100.0;
    push_metric(store, "cpu:usage_percent", cpu_usage);
    push_metric(store, "cpu:load_1m", load.one);
    push_metric(store, "cpu:load_5m", load.five);
    push_metric(store, "cpu:load_15m", load.fifteen);

    // Memory
    let total_mem = sys.total_memory() as f64;
    let used_mem = sys.used_memory() as f64;
    if total_mem > 0.0 {
        push_metric(store, "memory:usage_percent", (used_mem / total_mem) * 100.0);
    }
    push_metric(store, "memory:used_mb", used_mem / 1024.0 / 1024.0);
    push_metric(store, "memory:available_mb", sys.available_memory() as f64 / 1024.0 / 1024.0);

    // GPU metrics (if available)
    let inner = state.inner.blocking_read();
    if let Ok(gpu_metrics) = inner.gpu_manager.get_metrics() {
        for m in &gpu_metrics {
            let idx = m.index.to_string();
            push_metric_labeled(store, "gpu:utilization_percent", m.utilization_percent as f64, "gpu", &idx);
            push_metric_labeled(store, "gpu:memory_used_mb", m.memory_used_mb as f64, "gpu", &idx);
            push_metric_labeled(store, "gpu:temperature_c", m.temperature_c as f64, "gpu", &idx);
            if let Some(power) = m.power_draw_w {
                push_metric_labeled(store, "gpu:power_draw_w", power as f64, "gpu", &idx);
            }
        }
    }
}

fn push_metric(store: &claw_metrics::MetricStore, name: &str, value: f64) {
    if let Ok(metric_name) = claw_metrics::MetricName::new(name) {
        let point = claw_metrics::MetricPoint::now(value);
        let _ = store.push(&metric_name, point);
    }
}

fn push_metric_labeled(store: &claw_metrics::MetricStore, name: &str, value: f64, label_key: &str, label_val: &str) {
    if let Ok(metric_name) = claw_metrics::MetricName::new(name) {
        let point = claw_metrics::MetricPoint::now(value).label(label_key, label_val);
        let _ = store.push(&metric_name, point);
    }
}

/// Evaluate all alert rules against current metric values.
pub async fn evaluate_alerts(state: &SharedState) {
    let mut alert_store = state.alert_store.write().await;
    let metric_store = &state.metric_store;

    // Collect alert names and their metric references first
    let alert_metrics: Vec<(String, String)> = alert_store
        .list()
        .iter()
        .map(|a| (a.name.clone(), a.metric.clone()))
        .collect();

    for (alert_name, metric_name) in alert_metrics {
        if let Ok(name) = claw_metrics::MetricName::new(&metric_name) {
            if let Some(value) = claw_metrics::last_value(metric_store, &name) {
                alert_store.evaluate(&alert_name, value);
            }
        }
    }
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
    async fn test_metrics_push_and_query() {
        let state = test_state();

        // Push some metrics
        let name = claw_metrics::MetricName::new("test_cpu").expect("name");
        for i in 0..5 {
            let point = claw_metrics::MetricPoint::now(i as f64 * 10.0);
            state.metric_store.push(&name, point).expect("push");
        }

        let result = handle_metrics_command(
            &state,
            CommandRequest {
                command: "metrics.query".to_string(),
                params: json!({"name": "test_cpu", "rangeMinutes": 5}),
            },
        )
        .await
        .expect("query");

        assert_eq!(result["count"], 5);
        assert_eq!(result["name"], "test_cpu");
    }

    #[tokio::test]
    async fn test_metrics_query_with_aggregation() {
        let state = test_state();

        let name = claw_metrics::MetricName::new("test_mem").expect("name");
        for val in [10.0, 20.0, 30.0] {
            let point = claw_metrics::MetricPoint::now(val);
            state.metric_store.push(&name, point).expect("push");
        }

        let result = handle_metrics_command(
            &state,
            CommandRequest {
                command: "metrics.query".to_string(),
                params: json!({"name": "test_mem", "rangeMinutes": 5, "aggregation": "avg"}),
            },
        )
        .await
        .expect("query avg");

        // Aggregated query returns a single point
        assert_eq!(result["count"], 1);
    }

    #[tokio::test]
    async fn test_metrics_list() {
        let state = test_state();

        // Push metrics to two different names
        for metric in &["cpu_load", "mem_usage"] {
            let name = claw_metrics::MetricName::new(*metric).expect("name");
            state
                .metric_store
                .push(&name, claw_metrics::MetricPoint::now(42.0))
                .expect("push");
        }

        let result = handle_metrics_command(
            &state,
            CommandRequest {
                command: "metrics.list".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list");

        assert_eq!(result["count"], 2);
    }

    #[tokio::test]
    async fn test_metrics_snapshot() {
        let state = test_state();

        let name = claw_metrics::MetricName::new("snap_test").expect("name");
        state
            .metric_store
            .push(&name, claw_metrics::MetricPoint::now(99.0))
            .expect("push");

        let result = handle_metrics_command(
            &state,
            CommandRequest {
                command: "metrics.snapshot".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("snapshot");

        assert_eq!(result["metrics"]["snap_test"], 99.0);
    }

    #[tokio::test]
    async fn test_events_emit_and_query() {
        let state = test_state();

        // Emit events
        handle_metrics_command(
            &state,
            CommandRequest {
                command: "events.emit".to_string(),
                params: json!({
                    "source": "clawnode",
                    "severity": "info",
                    "message": "node started"
                }),
            },
        )
        .await
        .expect("emit");

        handle_metrics_command(
            &state,
            CommandRequest {
                command: "events.emit".to_string(),
                params: json!({
                    "source": "gpu-monitor",
                    "severity": "warning",
                    "message": "GPU temperature high"
                }),
            },
        )
        .await
        .expect("emit");

        // Query all events
        let result = handle_metrics_command(
            &state,
            CommandRequest {
                command: "events.query".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("query");
        assert_eq!(result["count"], 2);

        // Filter by severity
        let result = handle_metrics_command(
            &state,
            CommandRequest {
                command: "events.query".to_string(),
                params: json!({"severity": "warning"}),
            },
        )
        .await
        .expect("query warning");
        assert_eq!(result["count"], 1);
    }

    #[tokio::test]
    async fn test_events_invalid_severity() {
        let state = test_state();

        let result = handle_metrics_command(
            &state,
            CommandRequest {
                command: "events.emit".to_string(),
                params: json!({
                    "source": "test",
                    "severity": "critical",
                    "message": "bad"
                }),
            },
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_alerts_create_and_list() {
        let state = test_state();

        let result = handle_metrics_command(
            &state,
            CommandRequest {
                command: "alerts.create".to_string(),
                params: json!({
                    "name": "high-cpu",
                    "metric": "cpu:usage_percent",
                    "condition": "above",
                    "threshold": 90.0
                }),
            },
        )
        .await
        .expect("create");
        assert_eq!(result["success"], true);

        let result = handle_metrics_command(
            &state,
            CommandRequest {
                command: "alerts.list".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list");
        assert_eq!(result["count"], 1);
        assert_eq!(result["alerts"][0]["name"], "high-cpu");
        assert_eq!(result["alerts"][0]["state"], "ok");
    }

    #[tokio::test]
    async fn test_alerts_evaluate_and_acknowledge() {
        let state = test_state();

        // Create alert
        handle_metrics_command(
            &state,
            CommandRequest {
                command: "alerts.create".to_string(),
                params: json!({
                    "name": "mem-alert",
                    "metric": "memory:usage_percent",
                    "condition": "above",
                    "threshold": 80.0
                }),
            },
        )
        .await
        .expect("create");

        // Push a high metric value
        let name = claw_metrics::MetricName::new("memory:usage_percent").expect("name");
        state
            .metric_store
            .push(&name, claw_metrics::MetricPoint::now(95.0))
            .expect("push");

        // Evaluate alerts
        evaluate_alerts(&state).await;

        // Check it's firing
        let result = handle_metrics_command(
            &state,
            CommandRequest {
                command: "alerts.list".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list");
        assert_eq!(result["alerts"][0]["state"], "firing");

        // Acknowledge
        let result = handle_metrics_command(
            &state,
            CommandRequest {
                command: "alerts.acknowledge".to_string(),
                params: json!({"name": "mem-alert"}),
            },
        )
        .await
        .expect("ack");
        assert_eq!(result["acknowledged"], true);
    }

    #[tokio::test]
    async fn test_alerts_invalid_condition() {
        let state = test_state();

        let result = handle_metrics_command(
            &state,
            CommandRequest {
                command: "alerts.create".to_string(),
                params: json!({
                    "name": "bad",
                    "metric": "x",
                    "condition": "equals",
                    "threshold": 1.0
                }),
            },
        )
        .await;
        assert!(result.is_err());
    }
}
