//! Alert manager for evaluating rules and managing alerts.
//!
//! This module provides the [`AlertManager`] which is the main entry point
//! for the alerting system. It evaluates rules against metrics, manages
//! alert state, and sends notifications through configured channels.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use claw_metrics::{Aggregation, MetricName, MetricStore, TimeRange};
use parking_lot::RwLock;
use tracing::{debug, info, warn};

use crate::channels::{Notification, NotificationChannel};
use crate::error::{AlertError, Result};
use crate::types::{Alert, AlertRule, AlertState, Silence};

/// Configuration for the alert manager.
#[derive(Debug, Clone)]
pub struct AlertManagerConfig {
    /// How often to evaluate rules (in seconds).
    pub evaluation_interval_secs: u64,
    /// How long to keep resolved alerts (in seconds).
    pub resolved_alert_retention_secs: u64,
    /// Maximum number of alerts to keep.
    pub max_alerts: usize,
    /// Whether to send notifications on resolve.
    pub notify_on_resolve: bool,
}

impl Default for AlertManagerConfig {
    fn default() -> Self {
        Self {
            evaluation_interval_secs: 15,
            resolved_alert_retention_secs: 3600, // 1 hour
            max_alerts: 10000,
            notify_on_resolve: true,
        }
    }
}

/// The result of an evaluation cycle.
#[derive(Debug, Clone, Default)]
pub struct EvaluationResult {
    /// Number of rules evaluated.
    pub rules_evaluated: usize,
    /// Number of rules that errored.
    pub rules_errored: usize,
    /// Alerts that started firing.
    pub alerts_fired: Vec<String>,
    /// Alerts that were resolved.
    pub alerts_resolved: Vec<String>,
    /// Notifications sent.
    pub notifications_sent: usize,
    /// Notification failures.
    pub notification_failures: usize,
}

/// The alert manager is responsible for evaluating rules and managing alerts.
///
/// It maintains the state of all alerts and handles the lifecycle:
/// - Evaluating conditions against metrics
/// - Transitioning alerts between pending, firing, and resolved states
/// - Sending notifications through configured channels
/// - Managing silences
#[derive(Debug)]
pub struct AlertManager {
    /// Configuration for the manager.
    config: AlertManagerConfig,
    /// The metric store to query for conditions.
    metrics: Option<MetricStore>,
    /// All registered alert rules.
    rules: Arc<RwLock<HashMap<String, AlertRule>>>,
    /// All active and recently resolved alerts.
    alerts: Arc<RwLock<HashMap<String, Alert>>>,
    /// Pending alert timestamps (fingerprint -> first pending time).
    pending_since: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    /// Active silences.
    silences: Arc<RwLock<HashMap<String, Silence>>>,
    /// Notification channels.
    channels: Arc<RwLock<Vec<Box<dyn NotificationChannel>>>>,
}

impl AlertManager {
    /// Creates a new alert manager with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(AlertManagerConfig::default())
    }

    /// Creates a new alert manager with custom configuration.
    #[must_use]
    pub fn with_config(config: AlertManagerConfig) -> Self {
        Self {
            config,
            metrics: None,
            rules: Arc::new(RwLock::new(HashMap::new())),
            alerts: Arc::new(RwLock::new(HashMap::new())),
            pending_since: Arc::new(RwLock::new(HashMap::new())),
            silences: Arc::new(RwLock::new(HashMap::new())),
            channels: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Sets the metric store to use for evaluating conditions.
    pub fn set_metrics(&mut self, store: MetricStore) {
        self.metrics = Some(store);
    }

    /// Returns the metric store.
    #[must_use]
    pub fn metrics(&self) -> Option<&MetricStore> {
        self.metrics.as_ref()
    }

    /// Returns the configuration.
    #[must_use]
    pub const fn config(&self) -> &AlertManagerConfig {
        &self.config
    }

    // ============ Rule Management ============

    /// Adds a new alert rule.
    ///
    /// # Errors
    ///
    /// Returns `AlertError::InvalidRule` if a rule with the same ID already exists.
    pub fn add_rule(&self, rule: AlertRule) -> Result<()> {
        let mut rules = self.rules.write();

        if rules.contains_key(&rule.id) {
            return Err(AlertError::InvalidRule {
                reason: format!("rule with ID '{}' already exists", rule.id),
            });
        }

        info!(rule_id = %rule.id, rule_name = %rule.name, "added alert rule");
        rules.insert(rule.id.clone(), rule);

        Ok(())
    }

    /// Updates an existing rule.
    ///
    /// # Errors
    ///
    /// Returns `AlertError::RuleNotFound` if the rule doesn't exist.
    pub fn update_rule(&self, rule: AlertRule) -> Result<()> {
        let mut rules = self.rules.write();

        if !rules.contains_key(&rule.id) {
            return Err(AlertError::RuleNotFound {
                name: rule.id.clone(),
            });
        }

        info!(rule_id = %rule.id, rule_name = %rule.name, "updated alert rule");
        rules.insert(rule.id.clone(), rule);

        Ok(())
    }

    /// Removes a rule by ID.
    ///
    /// Returns `true` if the rule was removed.
    pub fn remove_rule(&self, rule_id: &str) -> bool {
        let mut rules = self.rules.write();
        let removed = rules.remove(rule_id).is_some();

        if removed {
            info!(rule_id = %rule_id, "removed alert rule");
        }

        removed
    }

    /// Gets a rule by ID.
    #[must_use]
    pub fn get_rule(&self, rule_id: &str) -> Option<AlertRule> {
        let rules = self.rules.read();
        rules.get(rule_id).cloned()
    }

    /// Returns all rules.
    #[must_use]
    pub fn list_rules(&self) -> Vec<AlertRule> {
        let rules = self.rules.read();
        rules.values().cloned().collect()
    }

    /// Returns the number of rules.
    #[must_use]
    pub fn rule_count(&self) -> usize {
        let rules = self.rules.read();
        rules.len()
    }

    // ============ Alert Management ============

    /// Gets an alert by its fingerprint.
    #[must_use]
    pub fn get_alert(&self, fingerprint: &str) -> Option<Alert> {
        let alerts = self.alerts.read();
        alerts.get(fingerprint).cloned()
    }

    /// Returns all alerts.
    #[must_use]
    pub fn list_alerts(&self) -> Vec<Alert> {
        let alerts = self.alerts.read();
        alerts.values().cloned().collect()
    }

    /// Returns all active alerts (pending or firing).
    #[must_use]
    pub fn active_alerts(&self) -> Vec<Alert> {
        let alerts = self.alerts.read();
        alerts
            .values()
            .filter(|a| a.is_active())
            .cloned()
            .collect()
    }

    /// Returns all firing alerts.
    #[must_use]
    pub fn firing_alerts(&self) -> Vec<Alert> {
        let alerts = self.alerts.read();
        alerts
            .values()
            .filter(|a| a.state == AlertState::Firing)
            .cloned()
            .collect()
    }

    /// Returns the number of alerts.
    #[must_use]
    pub fn alert_count(&self) -> usize {
        let alerts = self.alerts.read();
        alerts.len()
    }

    /// Clears all alerts.
    pub fn clear_alerts(&self) {
        let mut alerts = self.alerts.write();
        let mut pending = self.pending_since.write();

        alerts.clear();
        pending.clear();

        info!("cleared all alerts");
    }

    // ============ Silence Management ============

    /// Adds a new silence.
    ///
    /// # Errors
    ///
    /// Returns an error if a silence with the same ID already exists.
    pub fn add_silence(&self, silence: Silence) -> Result<()> {
        let mut silences = self.silences.write();

        if silences.contains_key(&silence.id) {
            return Err(AlertError::InvalidRule {
                reason: format!("silence with ID '{}' already exists", silence.id),
            });
        }

        info!(
            silence_id = %silence.id,
            matchers = ?silence.matchers,
            ends_at = %silence.ends_at,
            "added silence"
        );
        silences.insert(silence.id.clone(), silence);

        Ok(())
    }

    /// Removes a silence by ID.
    ///
    /// # Errors
    ///
    /// Returns `AlertError::SilenceNotFound` if the silence doesn't exist.
    pub fn remove_silence(&self, silence_id: &str) -> Result<()> {
        let mut silences = self.silences.write();

        if silences.remove(silence_id).is_none() {
            return Err(AlertError::SilenceNotFound {
                id: silence_id.to_string(),
            });
        }

        info!(silence_id = %silence_id, "removed silence");
        Ok(())
    }

    /// Gets a silence by ID.
    #[must_use]
    pub fn get_silence(&self, silence_id: &str) -> Option<Silence> {
        let silences = self.silences.read();
        silences.get(silence_id).cloned()
    }

    /// Returns all silences.
    #[must_use]
    pub fn list_silences(&self) -> Vec<Silence> {
        let silences = self.silences.read();
        silences.values().cloned().collect()
    }

    /// Returns all active silences.
    #[must_use]
    pub fn active_silences(&self) -> Vec<Silence> {
        let silences = self.silences.read();
        silences.values().filter(|s| s.is_active()).cloned().collect()
    }

    /// Checks if an alert is silenced.
    #[must_use]
    pub fn is_silenced(&self, alert: &Alert) -> bool {
        let silences = self.silences.read();
        silences.values().any(|s| s.matches(alert))
    }

    // ============ Channel Management ============

    /// Adds a notification channel.
    pub fn add_channel(&self, channel: Box<dyn NotificationChannel>) {
        let mut channels = self.channels.write();
        info!(channel = %channel.name(), "added notification channel");
        channels.push(channel);
    }

    /// Returns the number of channels.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        let channels = self.channels.read();
        channels.len()
    }

    // ============ Evaluation ============

    /// Evaluates all rules against current metrics.
    ///
    /// This is the main evaluation loop that should be called periodically.
    ///
    /// # Errors
    ///
    /// Returns an error if the metrics store is not configured.
    pub fn evaluate(&self) -> Result<EvaluationResult> {
        let metrics = self.metrics.as_ref().ok_or_else(|| AlertError::EvaluationError {
            reason: "metrics store not configured".to_string(),
        })?;

        let mut result = EvaluationResult::default();
        let rules = self.rules.read().clone();

        for rule in rules.values() {
            if !rule.enabled {
                continue;
            }

            result.rules_evaluated += 1;

            match self.evaluate_rule(rule, metrics) {
                Ok((fired, resolved)) => {
                    result.alerts_fired.extend(fired);
                    result.alerts_resolved.extend(resolved);
                }
                Err(e) => {
                    result.rules_errored += 1;
                    warn!(
                        rule_id = %rule.id,
                        rule_name = %rule.name,
                        error = %e,
                        "failed to evaluate rule"
                    );
                }
            }
        }

        // Send notifications for fired and resolved alerts
        if !result.alerts_fired.is_empty() || !result.alerts_resolved.is_empty() {
            let (sent, failed) = self.send_notifications(&result.alerts_fired, &result.alerts_resolved);
            result.notifications_sent = sent;
            result.notification_failures = failed;
        }

        // Clean up old resolved alerts
        self.cleanup_resolved_alerts();

        debug!(
            rules_evaluated = result.rules_evaluated,
            alerts_fired = result.alerts_fired.len(),
            alerts_resolved = result.alerts_resolved.len(),
            "evaluation complete"
        );

        Ok(result)
    }

    /// Evaluates a single rule against current metrics.
    fn evaluate_rule(
        &self,
        rule: &AlertRule,
        metrics: &MetricStore,
    ) -> Result<(Vec<String>, Vec<String>)> {
        let mut fired = Vec::new();
        let mut resolved = Vec::new();

        // Query the metric value
        let metric_name = MetricName::new(&rule.condition.metric_name)
            .map_err(|e| AlertError::EvaluationError {
                reason: format!("invalid metric name: {e}"),
            })?;

        let range = TimeRange::last_minutes(1);

        // Get the latest value for the metric
        let value = match metrics.query(&metric_name, range, Some(Aggregation::Last)) {
            Ok(points) => {
                if points.is_empty() {
                    debug!(
                        rule_id = %rule.id,
                        metric = %rule.condition.metric_name,
                        "no data for metric"
                    );
                    return Ok((fired, resolved));
                }
                points[0].value
            }
            Err(claw_metrics::MetricsError::MetricNotFound { .. }) => {
                debug!(
                    rule_id = %rule.id,
                    metric = %rule.condition.metric_name,
                    "metric not found"
                );
                return Ok((fired, resolved));
            }
            Err(e) => return Err(AlertError::MetricsError(e)),
        };

        // Evaluate the condition
        let condition_met = rule.condition.evaluate(value);

        // Compute fingerprint using a temporary alert for consistency
        let temp_alert = Alert::new_pending(rule, value, HashMap::new());
        let fingerprint = temp_alert.fingerprint.clone();

        if condition_met {
            // Condition is true - handle pending/firing transition
            let (new_fired, _) = self.handle_condition_true(rule, &fingerprint, value, temp_alert);
            if let Some(fp) = new_fired {
                fired.push(fp);
            }
        } else {
            // Condition is false - handle resolution
            if let Some(fp) = self.handle_condition_false(&fingerprint) {
                resolved.push(fp);
            }
        }

        Ok((fired, resolved))
    }

    /// Handles when a condition evaluates to true.
    fn handle_condition_true(
        &self,
        rule: &AlertRule,
        fingerprint: &str,
        value: f64,
        new_alert: Alert,
    ) -> (Option<String>, bool) {
        let mut alerts = self.alerts.write();
        let mut pending_since = self.pending_since.write();
        let now = Utc::now();

        // Check if alert already exists
        if let Some(alert) = alerts.get_mut(fingerprint) {
            // Update value
            alert.update_value(value);

            if alert.state == AlertState::Pending {
                // Check if we've been pending long enough
                if let Some(pending_start) = pending_since.get(fingerprint) {
                    let pending_duration = now.signed_duration_since(*pending_start);
                    let for_duration = chrono::Duration::seconds(rule.for_duration_secs as i64);

                    if pending_duration >= for_duration {
                        // Transition to firing
                        alert.fire();
                        info!(
                            rule_id = %rule.id,
                            rule_name = %rule.name,
                            fingerprint = %fingerprint,
                            value = %value,
                            "alert fired"
                        );
                        return (Some(fingerprint.to_string()), true);
                    }
                }
            }

            return (None, false);
        }

        // Use the pre-created alert
        let fp = fingerprint.to_string();

        if rule.for_duration_secs == 0 {
            // No for_duration, fire immediately
            let mut alert = new_alert;
            alert.fire();
            alerts.insert(fp.clone(), alert);

            info!(
                rule_id = %rule.id,
                rule_name = %rule.name,
                fingerprint = %fp,
                value = %value,
                "alert fired immediately"
            );

            (Some(fp), true)
        } else {
            // Start pending
            pending_since.insert(fp.clone(), now);
            alerts.insert(fp.clone(), new_alert);

            debug!(
                rule_id = %rule.id,
                rule_name = %rule.name,
                fingerprint = %fp,
                "alert pending"
            );

            (None, false)
        }
    }

    /// Handles when a condition evaluates to false.
    fn handle_condition_false(&self, fingerprint: &str) -> Option<String> {
        let mut alerts = self.alerts.write();
        let mut pending_since = self.pending_since.write();

        // Remove from pending
        pending_since.remove(fingerprint);

        // Check if alert exists and is active
        if let Some(alert) = alerts.get_mut(fingerprint) {
            if alert.is_active() {
                let was_firing = alert.state == AlertState::Firing;
                alert.resolve();

                if was_firing {
                    info!(
                        rule_name = %alert.rule_name,
                        fingerprint = %fingerprint,
                        "alert resolved"
                    );
                    return Some(fingerprint.to_string());
                }
            }
        }

        None
    }

    /// Sends notifications for fired and resolved alerts.
    fn send_notifications(
        &self,
        fired: &[String],
        resolved: &[String],
    ) -> (usize, usize) {
        let channels = self.channels.read();
        if channels.is_empty() {
            return (0, 0);
        }

        let alerts = self.alerts.read();
        let mut sent = 0;
        let mut failed = 0;

        // Collect fired alerts
        let fired_alerts: Vec<_> = fired
            .iter()
            .filter_map(|fp| alerts.get(fp))
            .filter(|a| !self.is_silenced(a))
            .cloned()
            .collect();

        // Send firing notification
        if !fired_alerts.is_empty() {
            let notification = Notification::new(fired_alerts);

            for channel in channels.iter() {
                match channel.send(&notification) {
                    Ok(result) if result.success => sent += 1,
                    Ok(result) => {
                        warn!(channel = %result.channel, message = ?result.message, "notification failed");
                        failed += 1;
                    }
                    Err(e) => {
                        warn!(channel = %channel.name(), error = %e, "notification error");
                        failed += 1;
                    }
                }
            }
        }

        // Send resolved notification if configured
        if self.config.notify_on_resolve {
            let resolved_alerts: Vec<_> = resolved
                .iter()
                .filter_map(|fp| alerts.get(fp))
                .filter(|a| !self.is_silenced(a))
                .cloned()
                .collect();

            if !resolved_alerts.is_empty() {
                let notification = Notification::new(resolved_alerts);

                for channel in channels.iter() {
                    match channel.send(&notification) {
                        Ok(result) if result.success => sent += 1,
                        Ok(result) => {
                            warn!(channel = %result.channel, message = ?result.message, "resolve notification failed");
                            failed += 1;
                        }
                        Err(e) => {
                            warn!(channel = %channel.name(), error = %e, "resolve notification error");
                            failed += 1;
                        }
                    }
                }
            }
        }

        (sent, failed)
    }

    /// Cleans up old resolved alerts.
    fn cleanup_resolved_alerts(&self) {
        let mut alerts = self.alerts.write();
        let now = Utc::now();
        let retention = chrono::Duration::seconds(self.config.resolved_alert_retention_secs as i64);

        alerts.retain(|_, alert| {
            if let Some(resolved_at) = alert.resolved_at {
                now.signed_duration_since(resolved_at) < retention
            } else {
                true
            }
        });

        // Also enforce max alerts limit
        if alerts.len() > self.config.max_alerts {
            // Remove oldest resolved alerts first
            let mut to_remove: Vec<_> = alerts
                .iter()
                .filter(|(_, a)| a.state == AlertState::Resolved)
                .map(|(fp, a)| (fp.clone(), a.resolved_at))
                .collect();

            to_remove.sort_by_key(|(_, t)| *t);

            let remove_count = alerts.len() - self.config.max_alerts;
            for (fp, _) in to_remove.into_iter().take(remove_count) {
                alerts.remove(&fp);
            }
        }
    }

    /// Evaluates rules with custom metric values (for testing).
    ///
    /// This bypasses the metric store and uses the provided values directly.
    pub fn evaluate_with_values(&self, values: &HashMap<String, f64>) -> Result<EvaluationResult> {
        let mut result = EvaluationResult::default();
        let rules = self.rules.read().clone();

        for rule in rules.values() {
            if !rule.enabled {
                continue;
            }

            result.rules_evaluated += 1;

            // Get the value for this rule's metric
            if let Some(&value) = values.get(&rule.condition.metric_name) {
                let condition_met = rule.condition.evaluate(value);

                // Compute fingerprint using a temporary alert for consistency
                let temp_alert = Alert::new_pending(rule, value, HashMap::new());
                let fingerprint = temp_alert.fingerprint.clone();

                if condition_met {
                    let (new_fired, _) = self.handle_condition_true(rule, &fingerprint, value, temp_alert);
                    if let Some(fp) = new_fired {
                        result.alerts_fired.push(fp);
                    }
                } else if let Some(fp) = self.handle_condition_false(&fingerprint) {
                    result.alerts_resolved.push(fp);
                }
            }
        }

        // Send notifications
        if !result.alerts_fired.is_empty() || !result.alerts_resolved.is_empty() {
            let (sent, failed) = self.send_notifications(&result.alerts_fired, &result.alerts_resolved);
            result.notifications_sent = sent;
            result.notification_failures = failed;
        }

        self.cleanup_resolved_alerts();

        Ok(result)
    }
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for AlertManager {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            metrics: self.metrics.clone(),
            rules: Arc::clone(&self.rules),
            alerts: Arc::clone(&self.alerts),
            pending_since: Arc::clone(&self.pending_since),
            silences: Arc::clone(&self.silences),
            channels: Arc::clone(&self.channels),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::LogChannel;
    use crate::types::{AlertCondition, AlertSeverity, ComparisonOperator};
    use chrono::Duration as ChronoDuration;
    use claw_metrics::MetricPoint;
    use std::time::Duration;

    fn test_condition() -> AlertCondition {
        AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0).unwrap()
    }

    fn test_rule() -> AlertRule {
        AlertRule::builder("HighCPU", test_condition())
            .severity(AlertSeverity::Warning)
            .build()
            .unwrap()
    }

    fn test_rule_with_duration() -> AlertRule {
        AlertRule::builder("HighCPU", test_condition())
            .for_duration(Duration::from_secs(60))
            .severity(AlertSeverity::Warning)
            .build()
            .unwrap()
    }

    mod config_tests {
        use super::*;

        #[test]
        fn default_config() {
            let config = AlertManagerConfig::default();
            assert_eq!(config.evaluation_interval_secs, 15);
            assert_eq!(config.resolved_alert_retention_secs, 3600);
            assert_eq!(config.max_alerts, 10000);
            assert!(config.notify_on_resolve);
        }
    }

    mod manager_creation_tests {
        use super::*;

        #[test]
        fn create_manager() {
            let manager = AlertManager::new();
            assert_eq!(manager.rule_count(), 0);
            assert_eq!(manager.alert_count(), 0);
        }

        #[test]
        fn create_with_config() {
            let config = AlertManagerConfig {
                evaluation_interval_secs: 30,
                ..Default::default()
            };
            let manager = AlertManager::with_config(config);
            assert_eq!(manager.config().evaluation_interval_secs, 30);
        }

        #[test]
        fn set_metrics() {
            let mut manager = AlertManager::new();
            assert!(manager.metrics().is_none());

            let store = MetricStore::new(Duration::from_secs(3600));
            manager.set_metrics(store);
            assert!(manager.metrics().is_some());
        }

        #[test]
        fn manager_is_cloneable() {
            let manager1 = AlertManager::new();
            manager1.add_rule(test_rule()).unwrap();

            let manager2 = manager1.clone();
            assert_eq!(manager2.rule_count(), 1);
        }

        #[test]
        fn default_manager() {
            let manager = AlertManager::default();
            assert_eq!(manager.rule_count(), 0);
        }
    }

    mod rule_management_tests {
        use super::*;

        #[test]
        fn add_rule() {
            let manager = AlertManager::new();
            let rule = test_rule();
            let id = rule.id.clone();

            let result = manager.add_rule(rule);
            assert!(result.is_ok());
            assert_eq!(manager.rule_count(), 1);
            assert!(manager.get_rule(&id).is_some());
        }

        #[test]
        fn add_duplicate_rule_fails() {
            let manager = AlertManager::new();
            let rule = test_rule();
            let rule2 = rule.clone();

            manager.add_rule(rule).unwrap();
            let result = manager.add_rule(rule2);

            assert!(result.is_err());
            match result {
                Err(AlertError::InvalidRule { reason }) => {
                    assert!(reason.contains("already exists"));
                }
                _ => panic!("expected InvalidRule error"),
            }
        }

        #[test]
        fn update_rule() {
            let manager = AlertManager::new();
            let rule = test_rule();
            let id = rule.id.clone();

            manager.add_rule(rule).unwrap();

            // Create updated rule with same ID
            let mut updated =
                AlertRule::builder("UpdatedRule", test_condition())
                    .severity(AlertSeverity::Critical)
                    .build()
                    .unwrap();
            // Override the auto-generated ID
            let mut rules = manager.rules.write();
            let original = rules.remove(&id).unwrap();
            drop(rules);

            updated = AlertRule {
                id: original.id.clone(),
                ..updated
            };

            manager.add_rule(updated.clone()).unwrap();
            let result = manager.get_rule(&original.id);

            assert!(result.is_some());
            let rule = result.unwrap();
            assert_eq!(rule.name, "UpdatedRule");
            assert_eq!(rule.severity, AlertSeverity::Critical);
        }

        #[test]
        fn remove_rule() {
            let manager = AlertManager::new();
            let rule = test_rule();
            let id = rule.id.clone();

            manager.add_rule(rule).unwrap();
            assert_eq!(manager.rule_count(), 1);

            let removed = manager.remove_rule(&id);
            assert!(removed);
            assert_eq!(manager.rule_count(), 0);
        }

        #[test]
        fn remove_nonexistent_rule() {
            let manager = AlertManager::new();
            let removed = manager.remove_rule("nonexistent");
            assert!(!removed);
        }

        #[test]
        fn list_rules() {
            let manager = AlertManager::new();

            let rule1 = AlertRule::builder("Rule1", test_condition())
                .build()
                .unwrap();
            let rule2 = AlertRule::builder("Rule2", test_condition())
                .build()
                .unwrap();

            manager.add_rule(rule1).unwrap();
            manager.add_rule(rule2).unwrap();

            let rules = manager.list_rules();
            assert_eq!(rules.len(), 2);
        }
    }

    mod alert_management_tests {
        use super::*;

        #[test]
        fn active_alerts_empty() {
            let manager = AlertManager::new();
            assert!(manager.active_alerts().is_empty());
            assert!(manager.firing_alerts().is_empty());
        }

        #[test]
        fn clear_alerts() {
            let manager = AlertManager::new();
            let rule = test_rule();

            manager.add_rule(rule).unwrap();

            // Manually create an alert
            let mut values = HashMap::new();
            values.insert("cpu_usage".to_string(), 90.0);
            manager.evaluate_with_values(&values).unwrap();

            assert_eq!(manager.alert_count(), 1);

            manager.clear_alerts();
            assert_eq!(manager.alert_count(), 0);
        }

        #[test]
        fn list_alerts() {
            let manager = AlertManager::new();
            let rule = test_rule();

            manager.add_rule(rule).unwrap();

            let mut values = HashMap::new();
            values.insert("cpu_usage".to_string(), 90.0);
            manager.evaluate_with_values(&values).unwrap();

            let alerts = manager.list_alerts();
            assert_eq!(alerts.len(), 1);
        }
    }

    mod silence_management_tests {
        use super::*;

        fn test_silence() -> Silence {
            let mut matchers = HashMap::new();
            matchers.insert("alertname".to_string(), "HighCPU".to_string());

            Silence::new(
                matchers,
                Utc::now() - ChronoDuration::hours(1),
                Utc::now() + ChronoDuration::hours(1),
                "admin",
                "test silence",
            )
            .unwrap()
        }

        #[test]
        fn add_silence() {
            let manager = AlertManager::new();
            let silence = test_silence();
            let id = silence.id.clone();

            let result = manager.add_silence(silence);
            assert!(result.is_ok());
            assert!(manager.get_silence(&id).is_some());
        }

        #[test]
        fn add_duplicate_silence_fails() {
            let manager = AlertManager::new();
            let silence = test_silence();
            let silence2 = silence.clone();

            manager.add_silence(silence).unwrap();
            let result = manager.add_silence(silence2);

            assert!(result.is_err());
        }

        #[test]
        fn remove_silence() {
            let manager = AlertManager::new();
            let silence = test_silence();
            let id = silence.id.clone();

            manager.add_silence(silence).unwrap();
            let result = manager.remove_silence(&id);

            assert!(result.is_ok());
            assert!(manager.get_silence(&id).is_none());
        }

        #[test]
        fn remove_nonexistent_silence_fails() {
            let manager = AlertManager::new();
            let result = manager.remove_silence("nonexistent");

            assert!(result.is_err());
            match result {
                Err(AlertError::SilenceNotFound { .. }) => {}
                _ => panic!("expected SilenceNotFound error"),
            }
        }

        #[test]
        fn list_silences() {
            let manager = AlertManager::new();
            let silence = test_silence();

            manager.add_silence(silence).unwrap();

            let silences = manager.list_silences();
            assert_eq!(silences.len(), 1);
        }

        #[test]
        fn active_silences() {
            let manager = AlertManager::new();
            let active = test_silence();

            let mut expired_matchers = HashMap::new();
            expired_matchers.insert("alertname".to_string(), "Other".to_string());
            let expired = Silence::new(
                expired_matchers,
                Utc::now() - ChronoDuration::hours(2),
                Utc::now() - ChronoDuration::hours(1),
                "admin",
                "expired",
            )
            .unwrap();

            manager.add_silence(active).unwrap();
            manager.add_silence(expired).unwrap();

            let active_silences = manager.active_silences();
            assert_eq!(active_silences.len(), 1);
        }

        #[test]
        fn is_silenced() {
            let manager = AlertManager::new();
            let rule = test_rule();
            let silence = test_silence();

            manager.add_silence(silence).unwrap();

            let alert = Alert::new_pending(&rule, 90.0, HashMap::new());
            assert!(manager.is_silenced(&alert));
        }
    }

    mod channel_management_tests {
        use super::*;

        #[test]
        fn add_channel() {
            let manager = AlertManager::new();
            assert_eq!(manager.channel_count(), 0);

            manager.add_channel(Box::new(LogChannel::default()));
            assert_eq!(manager.channel_count(), 1);
        }
    }

    mod evaluation_tests {
        use super::*;

        #[test]
        fn evaluate_without_metrics_fails() {
            let manager = AlertManager::new();
            let result = manager.evaluate();

            assert!(result.is_err());
            match result {
                Err(AlertError::EvaluationError { reason }) => {
                    assert!(reason.contains("not configured"));
                }
                _ => panic!("expected EvaluationError"),
            }
        }

        #[test]
        fn evaluate_empty_rules() {
            let mut manager = AlertManager::new();
            let store = MetricStore::new(Duration::from_secs(3600));
            manager.set_metrics(store);

            let result = manager.evaluate().unwrap();
            assert_eq!(result.rules_evaluated, 0);
        }

        #[test]
        fn evaluate_with_values() {
            let manager = AlertManager::new();
            let rule = test_rule();

            manager.add_rule(rule).unwrap();

            let mut values = HashMap::new();
            values.insert("cpu_usage".to_string(), 90.0);

            let result = manager.evaluate_with_values(&values).unwrap();

            assert_eq!(result.rules_evaluated, 1);
            assert_eq!(result.alerts_fired.len(), 1);
        }

        #[test]
        fn evaluate_below_threshold() {
            let manager = AlertManager::new();
            let rule = test_rule();

            manager.add_rule(rule).unwrap();

            let mut values = HashMap::new();
            values.insert("cpu_usage".to_string(), 70.0);

            let result = manager.evaluate_with_values(&values).unwrap();

            assert_eq!(result.rules_evaluated, 1);
            assert!(result.alerts_fired.is_empty());
            assert_eq!(manager.alert_count(), 0);
        }

        #[test]
        fn evaluate_disabled_rule() {
            let manager = AlertManager::new();
            let rule = AlertRule::builder("DisabledRule", test_condition())
                .enabled(false)
                .build()
                .unwrap();

            manager.add_rule(rule).unwrap();

            let mut values = HashMap::new();
            values.insert("cpu_usage".to_string(), 90.0);

            let result = manager.evaluate_with_values(&values).unwrap();

            // Disabled rules are not evaluated
            assert_eq!(result.rules_evaluated, 0);
        }

        #[test]
        fn evaluate_with_for_duration_pending() {
            let manager = AlertManager::new();
            let rule = test_rule_with_duration();

            manager.add_rule(rule).unwrap();

            let mut values = HashMap::new();
            values.insert("cpu_usage".to_string(), 90.0);

            // First evaluation - should be pending, not firing
            let result = manager.evaluate_with_values(&values).unwrap();

            assert_eq!(result.rules_evaluated, 1);
            assert!(result.alerts_fired.is_empty());

            let alerts = manager.active_alerts();
            assert_eq!(alerts.len(), 1);
            assert_eq!(alerts[0].state, AlertState::Pending);
        }

        #[test]
        fn evaluate_alert_resolves() {
            let manager = AlertManager::new();
            let rule = test_rule();

            manager.add_rule(rule).unwrap();

            // First: trigger alert
            let mut values = HashMap::new();
            values.insert("cpu_usage".to_string(), 90.0);
            manager.evaluate_with_values(&values).unwrap();

            assert_eq!(manager.firing_alerts().len(), 1);

            // Second: resolve alert
            let mut values = HashMap::new();
            values.insert("cpu_usage".to_string(), 70.0);
            let result = manager.evaluate_with_values(&values).unwrap();

            assert_eq!(result.alerts_resolved.len(), 1);
            assert!(manager.firing_alerts().is_empty());
        }

        #[test]
        fn evaluate_missing_metric() {
            let manager = AlertManager::new();
            let rule = test_rule();

            manager.add_rule(rule).unwrap();

            // Evaluate with no matching metric
            let values = HashMap::new();
            let result = manager.evaluate_with_values(&values).unwrap();

            assert_eq!(result.rules_evaluated, 1);
            assert!(result.alerts_fired.is_empty());
        }

        #[test]
        fn evaluate_with_metrics_store() {
            let mut manager = AlertManager::new();
            let store = MetricStore::new(Duration::from_secs(3600));

            // Push a metric value
            let name = MetricName::new("cpu_usage").unwrap();
            store.push(&name, MetricPoint::now(90.0)).unwrap();

            manager.set_metrics(store);
            manager.add_rule(test_rule()).unwrap();

            let result = manager.evaluate().unwrap();

            assert_eq!(result.rules_evaluated, 1);
            assert_eq!(result.alerts_fired.len(), 1);
        }

        #[test]
        fn evaluate_sends_notifications() {
            let manager = AlertManager::new();
            manager.add_channel(Box::new(LogChannel::default()));
            manager.add_rule(test_rule()).unwrap();

            let mut values = HashMap::new();
            values.insert("cpu_usage".to_string(), 90.0);

            let result = manager.evaluate_with_values(&values).unwrap();

            assert_eq!(result.notifications_sent, 1);
        }

        #[test]
        fn evaluate_silenced_alert_no_notification() {
            let manager = AlertManager::new();
            manager.add_channel(Box::new(LogChannel::default()));

            let rule = test_rule();
            manager.add_rule(rule).unwrap();

            // Add silence
            let mut matchers = HashMap::new();
            matchers.insert("alertname".to_string(), "HighCPU".to_string());
            let silence = Silence::new(
                matchers,
                Utc::now() - ChronoDuration::hours(1),
                Utc::now() + ChronoDuration::hours(1),
                "admin",
                "test",
            )
            .unwrap();
            manager.add_silence(silence).unwrap();

            let mut values = HashMap::new();
            values.insert("cpu_usage".to_string(), 90.0);

            let result = manager.evaluate_with_values(&values).unwrap();

            // Alert fires but notification is suppressed
            assert_eq!(result.alerts_fired.len(), 1);
            assert_eq!(result.notifications_sent, 0);
        }
    }

    mod cleanup_tests {
        use super::*;

        #[test]
        fn cleanup_resolved_alerts() {
            let config = AlertManagerConfig {
                resolved_alert_retention_secs: 1, // Very short for testing
                ..Default::default()
            };
            let manager = AlertManager::with_config(config);
            let rule = test_rule();

            manager.add_rule(rule).unwrap();

            // Create and resolve an alert
            let mut values1 = HashMap::new();
            values1.insert("cpu_usage".to_string(), 90.0);
            manager.evaluate_with_values(&values1).unwrap();

            let mut values2 = HashMap::new();
            values2.insert("cpu_usage".to_string(), 70.0);
            manager.evaluate_with_values(&values2).unwrap();

            assert_eq!(manager.alert_count(), 1);

            // Wait for retention to expire
            std::thread::sleep(Duration::from_secs(2));

            // Trigger cleanup
            let mut values3 = HashMap::new();
            values3.insert("cpu_usage".to_string(), 50.0);
            manager.evaluate_with_values(&values3).unwrap();

            // Alert should be cleaned up
            assert_eq!(manager.alert_count(), 0);
        }
    }
}
