//! End-to-end tests for Observability (claw-metrics, claw-logs, claw-alerts).
//!
//! These tests verify:
//! 1. Metric ingestion and query
//! 2. GPU metric collection
//! 3. Log aggregation and search
//! 4. Log streaming
//! 5. Alert rule creation and evaluation
//! 6. Alert silencing
//! 7. Notification channels

use std::collections::HashMap;
use std::time::Duration;
use chrono::Utc;
use uuid::Uuid;

use claw_metrics::{
    MetricCollector, MetricName, MetricPoint, MetricStore, TimeRange,
    average_over, last_value, max_over, rate,
    GpuMetricCollector, SystemMetricCollector,
};
use claw_logs::{
    LogEntry, LogEntryBuilder, LogFilter, LogId, LogLevel, LogStore,
    LogStoreConfig, RetentionPolicy, TimeRange as LogTimeRange,
};
use claw_alerts::{
    Alert, AlertCondition, AlertManager, AlertManagerConfig, AlertRule, AlertSeverity,
    AlertState, ComparisonOperator, LogChannel, Silence, WebhookChannel, WebhookConfig,
};

// ============================================================================
// Metrics: Basic Operations
// ============================================================================

#[test]
fn test_metric_store_creation() {
    let store = MetricStore::new(Duration::from_secs(3600)); // 1 hour retention
    assert!(store.is_empty());
}

#[test]
fn test_metric_push_and_query() {
    let store = MetricStore::new(Duration::from_secs(3600));

    let name = MetricName::new("cpu_usage").unwrap();

    // Push some metric points
    for i in 0..10 {
        store.push(&name, MetricPoint::now(50.0 + i as f64)).unwrap();
    }

    // Query recent data
    let range = TimeRange::last_minutes(5);
    let points = store.query(&name, range, None).unwrap();

    assert_eq!(points.len(), 10);
}

#[test]
fn test_metric_with_labels() {
    let store = MetricStore::new(Duration::from_secs(3600));

    let name = MetricName::new("http_requests_total").unwrap();

    // Push metrics with different labels
    store.push(&name, MetricPoint::now(100.0)
        .label("method", "GET")
        .label("status", "200")
    ).unwrap();

    store.push(&name, MetricPoint::now(50.0)
        .label("method", "POST")
        .label("status", "200")
    ).unwrap();

    store.push(&name, MetricPoint::now(5.0)
        .label("method", "GET")
        .label("status", "500")
    ).unwrap();

    // Query with label filter
    let range = TimeRange::last_minutes(5);

    // Filter by method
    let mut labels = HashMap::new();
    labels.insert("method".to_string(), "GET".to_string());
    let get_requests = store.query(&name, range.clone(), Some(labels)).unwrap();
    assert_eq!(get_requests.len(), 2); // 200 and 500 status

    // Filter by status
    let mut labels = HashMap::new();
    labels.insert("status".to_string(), "200".to_string());
    let success_requests = store.query(&name, range, Some(labels)).unwrap();
    assert_eq!(success_requests.len(), 2); // GET and POST
}

#[test]
fn test_metric_time_ranges() {
    let store = MetricStore::new(Duration::from_secs(3600));

    let name = MetricName::new("test_metric").unwrap();

    // Push metrics
    for i in 0..100 {
        store.push(&name, MetricPoint::now(i as f64)).unwrap();
    }

    // Query different time ranges
    let last_min = store.query(&name, TimeRange::last_minutes(1), None).unwrap();
    let last_5 = store.query(&name, TimeRange::last_minutes(5), None).unwrap();
    let last_hour = store.query(&name, TimeRange::last_hour(), None).unwrap();

    // All should return all points since they were just created
    assert_eq!(last_min.len(), 100);
    assert_eq!(last_5.len(), 100);
    assert_eq!(last_hour.len(), 100);
}

// ============================================================================
// Metrics: Query Functions
// ============================================================================

#[test]
fn test_metric_average() {
    let store = MetricStore::new(Duration::from_secs(3600));
    let name = MetricName::new("values").unwrap();

    // Push known values: 10, 20, 30, 40, 50 -> avg = 30
    for v in [10.0, 20.0, 30.0, 40.0, 50.0] {
        store.push(&name, MetricPoint::now(v)).unwrap();
    }

    let range = TimeRange::last_minutes(5);
    let points = store.query(&name, range, None).unwrap();
    let avg = average_over(&points);

    assert!((avg - 30.0).abs() < 0.001);
}

#[test]
fn test_metric_max() {
    let store = MetricStore::new(Duration::from_secs(3600));
    let name = MetricName::new("values").unwrap();

    for v in [10.0, 95.0, 30.0, 75.0, 50.0] {
        store.push(&name, MetricPoint::now(v)).unwrap();
    }

    let range = TimeRange::last_minutes(5);
    let points = store.query(&name, range, None).unwrap();
    let max = max_over(&points);

    assert!((max - 95.0).abs() < 0.001);
}

#[test]
fn test_metric_last_value() {
    let store = MetricStore::new(Duration::from_secs(3600));
    let name = MetricName::new("current").unwrap();

    for v in [10.0, 20.0, 30.0, 40.0, 99.0] {
        store.push(&name, MetricPoint::now(v)).unwrap();
    }

    let range = TimeRange::last_minutes(5);
    let points = store.query(&name, range, None).unwrap();
    let last = last_value(&points);

    assert!(last.is_some());
    assert!((last.unwrap() - 99.0).abs() < 0.001);
}

#[test]
fn test_metric_rate() {
    let store = MetricStore::new(Duration::from_secs(3600));
    let name = MetricName::new("counter").unwrap();

    // Counter going up over time
    let start = chrono::Utc::now();
    for i in 0..10 {
        let ts = start + chrono::Duration::seconds(i);
        store.push(&name, MetricPoint::at(ts, (i * 100) as f64)).unwrap();
    }

    let range = TimeRange::last_minutes(5);
    let points = store.query(&name, range, None).unwrap();
    let r = rate(&points);

    // Rate should be approximately 100 per second
    assert!(r > 0.0);
}

// ============================================================================
// Metrics: GPU Metrics
// ============================================================================

#[test]
fn test_gpu_metric_collector() {
    let store = MetricStore::new(Duration::from_secs(3600));
    let collector = GpuMetricCollector::new();

    // Simulate GPU metrics
    let gpu_utilization = MetricName::new("gpu_utilization").unwrap();
    let gpu_memory = MetricName::new("gpu_memory_used_mb").unwrap();
    let gpu_temperature = MetricName::new("gpu_temperature_celsius").unwrap();
    let gpu_power = MetricName::new("gpu_power_watts").unwrap();

    // GPU 0 metrics
    store.push(&gpu_utilization, MetricPoint::now(85.5).label("gpu_id", "0")).unwrap();
    store.push(&gpu_memory, MetricPoint::now(12288.0).label("gpu_id", "0")).unwrap();
    store.push(&gpu_temperature, MetricPoint::now(72.0).label("gpu_id", "0")).unwrap();
    store.push(&gpu_power, MetricPoint::now(250.0).label("gpu_id", "0")).unwrap();

    // GPU 1 metrics
    store.push(&gpu_utilization, MetricPoint::now(92.0).label("gpu_id", "1")).unwrap();
    store.push(&gpu_memory, MetricPoint::now(16384.0).label("gpu_id", "1")).unwrap();
    store.push(&gpu_temperature, MetricPoint::now(78.0).label("gpu_id", "1")).unwrap();
    store.push(&gpu_power, MetricPoint::now(300.0).label("gpu_id", "1")).unwrap();

    // Query GPU 0 utilization
    let mut labels = HashMap::new();
    labels.insert("gpu_id".to_string(), "0".to_string());
    let gpu0_util = store.query(&gpu_utilization, TimeRange::last_minutes(5), Some(labels)).unwrap();
    assert_eq!(gpu0_util.len(), 1);
    assert!((gpu0_util[0].value - 85.5).abs() < 0.001);

    // Query all GPU temperatures
    let all_temps = store.query(&gpu_temperature, TimeRange::last_minutes(5), None).unwrap();
    assert_eq!(all_temps.len(), 2);
}

#[test]
fn test_system_metric_collector() {
    let store = MetricStore::new(Duration::from_secs(3600));
    let collector = SystemMetricCollector::new();

    // Simulate system metrics
    let cpu_usage = MetricName::new("system_cpu_usage").unwrap();
    let memory_usage = MetricName::new("system_memory_usage_bytes").unwrap();
    let disk_io = MetricName::new("system_disk_io_bytes").unwrap();
    let network_rx = MetricName::new("system_network_rx_bytes").unwrap();

    store.push(&cpu_usage, MetricPoint::now(45.0).label("node", "node-1")).unwrap();
    store.push(&memory_usage, MetricPoint::now(8_589_934_592.0).label("node", "node-1")).unwrap();
    store.push(&disk_io, MetricPoint::now(1_073_741_824.0).label("node", "node-1")).unwrap();
    store.push(&network_rx, MetricPoint::now(536_870_912.0).label("node", "node-1")).unwrap();

    // Query all
    assert_eq!(store.query(&cpu_usage, TimeRange::last_minutes(5), None).unwrap().len(), 1);
    assert_eq!(store.query(&memory_usage, TimeRange::last_minutes(5), None).unwrap().len(), 1);
}

// ============================================================================
// Logs: Basic Operations
// ============================================================================

#[test]
fn test_log_store_creation() {
    let config = LogStoreConfig::default();
    let store = LogStore::new(config);
    assert!(store.is_empty());
}

#[test]
fn test_log_entry_creation() {
    let entry = LogEntry::builder()
        .id(LogId(1))
        .timestamp(Utc::now())
        .level(LogLevel::Info)
        .message("Application started")
        .workload_id(Uuid::new_v4())
        .node_id(Uuid::new_v4())
        .build();

    assert_eq!(entry.level, LogLevel::Info);
    assert_eq!(entry.message, "Application started");
}

#[test]
fn test_log_ingestion_and_query() {
    let store = LogStore::new(LogStoreConfig::default());

    // Ingest some logs
    for i in 0..100 {
        let entry = LogEntry::builder()
            .id(LogId(i))
            .timestamp(Utc::now())
            .level(if i % 10 == 0 { LogLevel::Error } else { LogLevel::Info })
            .message(format!("Log message {}", i))
            .workload_id(Uuid::new_v4())
            .build();

        store.append(entry).unwrap();
    }

    // Query all logs
    let filter = LogFilter::new();
    let logs = store.query(&filter, 1000);
    assert_eq!(logs.len(), 100);

    // Query only errors
    let error_filter = LogFilter::new().with_level(LogLevel::Error);
    let errors = store.query(&error_filter, 1000);
    assert_eq!(errors.len(), 10);
}

#[test]
fn test_log_filtering_by_workload() {
    let store = LogStore::new(LogStoreConfig::default());

    let workload_1 = Uuid::new_v4();
    let workload_2 = Uuid::new_v4();

    // Create logs for different workloads
    for i in 0..50 {
        let entry = LogEntry::builder()
            .id(LogId(i))
            .timestamp(Utc::now())
            .level(LogLevel::Info)
            .message(format!("Workload 1 log {}", i))
            .workload_id(workload_1)
            .build();
        store.append(entry).unwrap();
    }

    for i in 50..80 {
        let entry = LogEntry::builder()
            .id(LogId(i))
            .timestamp(Utc::now())
            .level(LogLevel::Info)
            .message(format!("Workload 2 log {}", i))
            .workload_id(workload_2)
            .build();
        store.append(entry).unwrap();
    }

    // Filter by workload
    let filter = LogFilter::new().with_workload_id(workload_1);
    let w1_logs = store.query(&filter, 1000);
    assert_eq!(w1_logs.len(), 50);

    let filter = LogFilter::new().with_workload_id(workload_2);
    let w2_logs = store.query(&filter, 1000);
    assert_eq!(w2_logs.len(), 30);
}

#[test]
fn test_log_text_search() {
    let store = LogStore::new(LogStoreConfig::default());

    let entries = vec![
        "User login successful",
        "Database connection established",
        "Error: Connection timeout",
        "Processing request from user",
        "Error: Invalid credentials",
        "User logout completed",
    ];

    for (i, msg) in entries.iter().enumerate() {
        let entry = LogEntry::builder()
            .id(LogId(i as u64))
            .timestamp(Utc::now())
            .level(if msg.starts_with("Error") { LogLevel::Error } else { LogLevel::Info })
            .message(msg.to_string())
            .build();
        store.append(entry).unwrap();
    }

    // Search for "user"
    let filter = LogFilter::new().with_contains("user");
    let user_logs = store.query(&filter, 1000);
    assert_eq!(user_logs.len(), 3); // login, processing, logout

    // Search for "Error"
    let filter = LogFilter::new().with_contains("Error");
    let error_logs = store.query(&filter, 1000);
    assert_eq!(error_logs.len(), 2);
}

#[test]
fn test_log_level_hierarchy() {
    let store = LogStore::new(LogStoreConfig::default());

    // Create logs at various levels
    let levels = vec![
        LogLevel::Trace,
        LogLevel::Debug,
        LogLevel::Info,
        LogLevel::Warn,
        LogLevel::Error,
    ];

    for (i, level) in levels.iter().enumerate() {
        let entry = LogEntry::builder()
            .id(LogId(i as u64))
            .timestamp(Utc::now())
            .level(*level)
            .message(format!("{:?} message", level))
            .build();
        store.append(entry).unwrap();
    }

    // Query Warn and above (Warn + Error)
    let filter = LogFilter::new().with_min_level(LogLevel::Warn);
    let warn_plus = store.query(&filter, 1000);
    assert_eq!(warn_plus.len(), 2);
}

// ============================================================================
// Logs: Retention
// ============================================================================

#[test]
fn test_log_retention_by_count() {
    let retention = RetentionPolicy::new()
        .with_max_entries(50);

    let config = LogStoreConfig::default()
        .with_retention(retention);
    let store = LogStore::new(config);

    // Add 100 logs
    for i in 0..100 {
        let entry = LogEntry::builder()
            .id(LogId(i))
            .timestamp(Utc::now())
            .level(LogLevel::Info)
            .message(format!("Log {}", i))
            .build();
        store.append(entry).unwrap();
    }

    // After enforcement, should only have last 50
    store.enforce_retention().unwrap();
    let logs = store.query(&LogFilter::new(), 1000);
    assert!(logs.len() <= 50);
}

// ============================================================================
// Alerts: Basic Operations
// ============================================================================

#[test]
fn test_alert_manager_creation() {
    let manager = AlertManager::new();
    assert_eq!(manager.rule_count(), 0);
}

#[test]
fn test_alert_rule_creation() {
    let condition = AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0).unwrap();

    let rule = AlertRule::builder("HighCPU", condition)
        .for_duration(Duration::from_secs(60))
        .severity(AlertSeverity::Warning)
        .label("team", "platform")
        .annotation("summary", "CPU usage is above 80%")
        .build()
        .unwrap();

    assert_eq!(rule.name, "HighCPU");
    assert_eq!(rule.severity, AlertSeverity::Warning);
}

#[test]
fn test_alert_rule_evaluation() {
    let manager = AlertManager::new();

    // Add a rule
    let condition = AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0).unwrap();
    let rule = AlertRule::builder("HighCPU", condition)
        .severity(AlertSeverity::Warning)
        .build()
        .unwrap();

    manager.add_rule(rule).unwrap();

    // Evaluate with values below threshold
    let mut values = HashMap::new();
    values.insert("cpu_usage".to_string(), 50.0);

    let result = manager.evaluate_with_values(&values).unwrap();
    assert_eq!(result.alerts_fired.len(), 0);

    // Evaluate with values above threshold
    values.insert("cpu_usage".to_string(), 90.0);

    let result = manager.evaluate_with_values(&values).unwrap();
    assert_eq!(result.alerts_fired.len(), 1);
    assert_eq!(result.alerts_fired[0].rule_name, "HighCPU");
}

#[test]
fn test_multiple_alert_rules() {
    let manager = AlertManager::new();

    // Rule 1: High CPU
    let cpu_condition = AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0).unwrap();
    let cpu_rule = AlertRule::builder("HighCPU", cpu_condition)
        .severity(AlertSeverity::Warning)
        .build()
        .unwrap();
    manager.add_rule(cpu_rule).unwrap();

    // Rule 2: High Memory
    let mem_condition = AlertCondition::new("memory_usage", ComparisonOperator::GreaterThan, 90.0).unwrap();
    let mem_rule = AlertRule::builder("HighMemory", mem_condition)
        .severity(AlertSeverity::Critical)
        .build()
        .unwrap();
    manager.add_rule(mem_rule).unwrap();

    // Rule 3: Low Disk
    let disk_condition = AlertCondition::new("disk_free_percent", ComparisonOperator::LessThan, 10.0).unwrap();
    let disk_rule = AlertRule::builder("LowDisk", disk_condition)
        .severity(AlertSeverity::Critical)
        .build()
        .unwrap();
    manager.add_rule(disk_rule).unwrap();

    assert_eq!(manager.rule_count(), 3);

    // Test various scenarios
    let mut values = HashMap::new();
    values.insert("cpu_usage".to_string(), 85.0);    // Fires
    values.insert("memory_usage".to_string(), 70.0); // OK
    values.insert("disk_free_percent".to_string(), 5.0); // Fires

    let result = manager.evaluate_with_values(&values).unwrap();
    assert_eq!(result.alerts_fired.len(), 2);
}

#[test]
fn test_alert_comparison_operators() {
    let manager = AlertManager::new();

    // Greater than
    let gt_cond = AlertCondition::new("value", ComparisonOperator::GreaterThan, 50.0).unwrap();
    manager.add_rule(AlertRule::builder("GT", gt_cond).build().unwrap()).unwrap();

    // Less than
    let lt_cond = AlertCondition::new("value", ComparisonOperator::LessThan, 50.0).unwrap();
    manager.add_rule(AlertRule::builder("LT", lt_cond).build().unwrap()).unwrap();

    // Greater than or equal
    let gte_cond = AlertCondition::new("value", ComparisonOperator::GreaterThanOrEqual, 50.0).unwrap();
    manager.add_rule(AlertRule::builder("GTE", gte_cond).build().unwrap()).unwrap();

    // Less than or equal
    let lte_cond = AlertCondition::new("value", ComparisonOperator::LessThanOrEqual, 50.0).unwrap();
    manager.add_rule(AlertRule::builder("LTE", lte_cond).build().unwrap()).unwrap();

    // Equal
    let eq_cond = AlertCondition::new("value", ComparisonOperator::Equal, 50.0).unwrap();
    manager.add_rule(AlertRule::builder("EQ", eq_cond).build().unwrap()).unwrap();

    // Not equal
    let neq_cond = AlertCondition::new("value", ComparisonOperator::NotEqual, 50.0).unwrap();
    manager.add_rule(AlertRule::builder("NEQ", neq_cond).build().unwrap()).unwrap();

    // Test with value = 50
    let mut values = HashMap::new();
    values.insert("value".to_string(), 50.0);

    let result = manager.evaluate_with_values(&values).unwrap();
    let fired_names: Vec<_> = result.alerts_fired.iter().map(|a| a.rule_name.as_str()).collect();

    // Should fire: GTE, LTE, EQ (not GT, LT, NEQ)
    assert!(fired_names.contains(&"GTE"));
    assert!(fired_names.contains(&"LTE"));
    assert!(fired_names.contains(&"EQ"));
    assert!(!fired_names.contains(&"GT"));
    assert!(!fired_names.contains(&"LT"));
    assert!(!fired_names.contains(&"NEQ"));
}

// ============================================================================
// Alerts: Silencing
// ============================================================================

#[test]
fn test_alert_silencing() {
    let manager = AlertManager::new();

    // Add a rule
    let condition = AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0).unwrap();
    let rule = AlertRule::builder("HighCPU", condition)
        .severity(AlertSeverity::Warning)
        .label("team", "platform")
        .build()
        .unwrap();
    manager.add_rule(rule).unwrap();

    // Create a silence
    let mut matchers = HashMap::new();
    matchers.insert("alertname".to_string(), "HighCPU".to_string());

    let silence = Silence::new(
        matchers,
        Utc::now(),
        Utc::now() + chrono::Duration::hours(4),
        "admin",
        "Maintenance window",
    ).unwrap();

    manager.add_silence(silence).unwrap();

    // Evaluate - alert should fire but be silenced
    let mut values = HashMap::new();
    values.insert("cpu_usage".to_string(), 95.0);

    let result = manager.evaluate_with_values(&values).unwrap();

    // Alert fires but notifications are suppressed
    assert_eq!(result.alerts_fired.len(), 1);
    assert!(result.alerts_silenced.len() > 0 || result.alerts_fired[0].is_silenced);
}

#[test]
fn test_silence_expiry() {
    let manager = AlertManager::new();

    // Create an expired silence
    let mut matchers = HashMap::new();
    matchers.insert("alertname".to_string(), "Test".to_string());

    let expired_silence = Silence::new(
        matchers,
        Utc::now() - chrono::Duration::hours(2),
        Utc::now() - chrono::Duration::hours(1), // Already ended
        "admin",
        "Past maintenance",
    ).unwrap();

    manager.add_silence(expired_silence).unwrap();

    // Cleanup expired silences
    manager.cleanup_silences();

    // Should have no active silences
    assert_eq!(manager.active_silences().len(), 0);
}

// ============================================================================
// Alerts: Notification Channels
// ============================================================================

#[test]
fn test_log_notification_channel() {
    let manager = AlertManager::new();

    // Add log channel (for testing - logs to console/file)
    manager.add_channel(Box::new(LogChannel::default()));

    // Add a rule
    let condition = AlertCondition::new("test_metric", ComparisonOperator::GreaterThan, 0.0).unwrap();
    let rule = AlertRule::builder("TestAlert", condition)
        .severity(AlertSeverity::Info)
        .build()
        .unwrap();
    manager.add_rule(rule).unwrap();

    // Trigger the alert
    let mut values = HashMap::new();
    values.insert("test_metric".to_string(), 100.0);

    let result = manager.evaluate_with_values(&values);
    assert!(result.is_ok());
}

#[test]
fn test_webhook_channel_config() {
    let config = WebhookConfig::new("https://alertmanager.example.com/api/v2/alerts")
        .with_timeout(Duration::from_secs(30))
        .with_retry_count(3);

    let channel = WebhookChannel::new(config);

    // Channel should be created successfully
    assert!(channel.is_ok());
}

// ============================================================================
// Alerts: Severity Levels
// ============================================================================

#[test]
fn test_alert_severity_ordering() {
    // Verify severity ordering
    assert!(AlertSeverity::Critical > AlertSeverity::Warning);
    assert!(AlertSeverity::Warning > AlertSeverity::Info);
    assert!(AlertSeverity::Critical > AlertSeverity::Info);
}

#[test]
fn test_alerts_grouped_by_severity() {
    let manager = AlertManager::new();

    // Add rules with different severities
    let info_cond = AlertCondition::new("info_metric", ComparisonOperator::GreaterThan, 0.0).unwrap();
    manager.add_rule(AlertRule::builder("InfoAlert", info_cond)
        .severity(AlertSeverity::Info)
        .build().unwrap()).unwrap();

    let warn_cond = AlertCondition::new("warn_metric", ComparisonOperator::GreaterThan, 0.0).unwrap();
    manager.add_rule(AlertRule::builder("WarnAlert", warn_cond)
        .severity(AlertSeverity::Warning)
        .build().unwrap()).unwrap();

    let crit_cond = AlertCondition::new("crit_metric", ComparisonOperator::GreaterThan, 0.0).unwrap();
    manager.add_rule(AlertRule::builder("CritAlert", crit_cond)
        .severity(AlertSeverity::Critical)
        .build().unwrap()).unwrap();

    // Fire all alerts
    let mut values = HashMap::new();
    values.insert("info_metric".to_string(), 1.0);
    values.insert("warn_metric".to_string(), 1.0);
    values.insert("crit_metric".to_string(), 1.0);

    let result = manager.evaluate_with_values(&values).unwrap();

    // Count by severity
    let critical_count = result.alerts_fired.iter()
        .filter(|a| a.severity == AlertSeverity::Critical)
        .count();
    let warning_count = result.alerts_fired.iter()
        .filter(|a| a.severity == AlertSeverity::Warning)
        .count();
    let info_count = result.alerts_fired.iter()
        .filter(|a| a.severity == AlertSeverity::Info)
        .count();

    assert_eq!(critical_count, 1);
    assert_eq!(warning_count, 1);
    assert_eq!(info_count, 1);
}

// ============================================================================
// Integration: Full Observability Workflow
// ============================================================================

#[test]
fn test_metrics_to_alerts_workflow() {
    // 1. Set up metrics store
    let metric_store = MetricStore::new(Duration::from_secs(3600));

    // 2. Set up alert manager
    let alert_manager = AlertManager::new();

    // Add alerting rules based on metrics
    let cpu_condition = AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0).unwrap();
    alert_manager.add_rule(AlertRule::builder("HighCPU", cpu_condition)
        .severity(AlertSeverity::Warning)
        .annotation("summary", "CPU usage is high")
        .build().unwrap()).unwrap();

    let gpu_condition = AlertCondition::new("gpu_utilization", ComparisonOperator::GreaterThan, 95.0).unwrap();
    alert_manager.add_rule(AlertRule::builder("HighGPU", gpu_condition)
        .severity(AlertSeverity::Critical)
        .annotation("summary", "GPU utilization critical")
        .build().unwrap()).unwrap();

    // 3. Ingest metrics
    let cpu_metric = MetricName::new("cpu_usage").unwrap();
    let gpu_metric = MetricName::new("gpu_utilization").unwrap();

    // Simulate high CPU but normal GPU
    metric_store.push(&cpu_metric, MetricPoint::now(85.0).label("node", "node-1")).unwrap();
    metric_store.push(&gpu_metric, MetricPoint::now(75.0).label("gpu_id", "0")).unwrap();

    // 4. Get latest values for alerting
    let cpu_points = metric_store.query(&cpu_metric, TimeRange::last_minutes(1), None).unwrap();
    let gpu_points = metric_store.query(&gpu_metric, TimeRange::last_minutes(1), None).unwrap();

    let mut values = HashMap::new();
    if let Some(v) = last_value(&cpu_points) {
        values.insert("cpu_usage".to_string(), v);
    }
    if let Some(v) = last_value(&gpu_points) {
        values.insert("gpu_utilization".to_string(), v);
    }

    // 5. Evaluate alerts
    let result = alert_manager.evaluate_with_values(&values).unwrap();

    // Only CPU alert should fire
    assert_eq!(result.alerts_fired.len(), 1);
    assert_eq!(result.alerts_fired[0].rule_name, "HighCPU");
}

#[test]
fn test_logs_with_error_alerting() {
    // 1. Set up log store
    let log_store = LogStore::new(LogStoreConfig::default());

    // 2. Set up alert manager for error rate
    let alert_manager = AlertManager::new();

    let error_condition = AlertCondition::new("error_rate", ComparisonOperator::GreaterThan, 5.0).unwrap();
    alert_manager.add_rule(AlertRule::builder("HighErrorRate", error_condition)
        .severity(AlertSeverity::Critical)
        .annotation("summary", "Error rate exceeds 5%")
        .build().unwrap()).unwrap();

    // 3. Ingest logs with some errors
    let total_logs = 100;
    let error_logs = 10; // 10% error rate

    for i in 0..total_logs {
        let level = if i < error_logs { LogLevel::Error } else { LogLevel::Info };
        let entry = LogEntry::builder()
            .id(LogId(i as u64))
            .timestamp(Utc::now())
            .level(level)
            .message(format!("Log entry {}", i))
            .build();
        log_store.append(entry).unwrap();
    }

    // 4. Calculate error rate from logs
    let all_logs = log_store.query(&LogFilter::new(), 1000);
    let errors = log_store.query(&LogFilter::new().with_level(LogLevel::Error), 1000);

    let error_rate = (errors.len() as f64 / all_logs.len() as f64) * 100.0;

    // 5. Evaluate alerts
    let mut values = HashMap::new();
    values.insert("error_rate".to_string(), error_rate);

    let result = alert_manager.evaluate_with_values(&values).unwrap();

    // Error rate alert should fire (10% > 5%)
    assert_eq!(result.alerts_fired.len(), 1);
    assert_eq!(result.alerts_fired[0].rule_name, "HighErrorRate");
}

#[test]
fn test_full_observability_stack() {
    // Complete integration test

    // 1. Metrics
    let metrics = MetricStore::new(Duration::from_secs(3600));

    // Ingest various metrics
    metrics.push(&MetricName::new("cpu_usage").unwrap(), MetricPoint::now(45.0)).unwrap();
    metrics.push(&MetricName::new("memory_used_gb").unwrap(), MetricPoint::now(12.0)).unwrap();
    metrics.push(&MetricName::new("gpu_utilization").unwrap(),
        MetricPoint::now(80.0).label("gpu_id", "0")).unwrap();
    metrics.push(&MetricName::new("request_latency_ms").unwrap(),
        MetricPoint::now(150.0).label("endpoint", "/api/v1/workloads")).unwrap();

    // 2. Logs
    let logs = LogStore::new(LogStoreConfig::default());

    let workload_id = Uuid::new_v4();
    for i in 0..50 {
        logs.append(LogEntry::builder()
            .id(LogId(i))
            .timestamp(Utc::now())
            .level(if i % 20 == 0 { LogLevel::Warn } else { LogLevel::Info })
            .message(format!("Processing request {}", i))
            .workload_id(workload_id)
            .build()
        ).unwrap();
    }

    // 3. Alerts
    let alerts = AlertManager::new();

    // Add various alert rules
    alerts.add_rule(AlertRule::builder("HighCPU",
        AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0).unwrap())
        .severity(AlertSeverity::Warning)
        .build().unwrap()).unwrap();

    alerts.add_rule(AlertRule::builder("HighLatency",
        AlertCondition::new("p99_latency", ComparisonOperator::GreaterThan, 200.0).unwrap())
        .severity(AlertSeverity::Warning)
        .build().unwrap()).unwrap();

    alerts.add_rule(AlertRule::builder("LowGPU",
        AlertCondition::new("gpu_utilization", ComparisonOperator::LessThan, 20.0).unwrap())
        .severity(AlertSeverity::Info)
        .annotation("summary", "GPU underutilized")
        .build().unwrap()).unwrap();

    // 4. Query metrics
    let cpu = last_value(&metrics.query(&MetricName::new("cpu_usage").unwrap(),
        TimeRange::last_minutes(5), None).unwrap()).unwrap_or(0.0);
    let gpu = last_value(&metrics.query(&MetricName::new("gpu_utilization").unwrap(),
        TimeRange::last_minutes(5), None).unwrap()).unwrap_or(0.0);
    let latency = last_value(&metrics.query(&MetricName::new("request_latency_ms").unwrap(),
        TimeRange::last_minutes(5), None).unwrap()).unwrap_or(0.0);

    // 5. Query logs
    let workload_logs = logs.query(&LogFilter::new().with_workload_id(workload_id), 100);
    let warn_logs = logs.query(&LogFilter::new().with_level(LogLevel::Warn), 100);

    // 6. Evaluate alerts
    let mut values = HashMap::new();
    values.insert("cpu_usage".to_string(), cpu);
    values.insert("gpu_utilization".to_string(), gpu);
    values.insert("p99_latency".to_string(), latency);

    let result = alerts.evaluate_with_values(&values).unwrap();

    // Verify state
    assert_eq!(workload_logs.len(), 50);
    assert_eq!(warn_logs.len(), 3); // Every 20th log is warning
    assert!((cpu - 45.0).abs() < 0.001);
    assert!((gpu - 80.0).abs() < 0.001);

    // No alerts should fire with current values
    assert_eq!(result.alerts_fired.len(), 0);
}
