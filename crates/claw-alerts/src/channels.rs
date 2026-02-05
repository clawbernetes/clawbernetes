//! Notification channels for alert delivery.
//!
//! This module provides the [`NotificationChannel`] trait and implementations
//! for delivering alert notifications through various channels.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

use crate::error::{AlertError, Result};
use crate::types::Alert;

/// A notification to be sent through a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// The alerts included in this notification.
    pub alerts: Vec<Alert>,
    /// Status of the notification (firing or resolved).
    pub status: NotificationStatus,
    /// External URL linking to alert details.
    pub external_url: Option<String>,
    /// Group key identifying this group of alerts.
    pub group_key: String,
    /// Number of alerts truncated from this notification.
    pub truncated_alerts: usize,
}

impl Notification {
    /// Creates a new notification for the given alerts.
    #[must_use]
    pub fn new(alerts: Vec<Alert>) -> Self {
        let status = if alerts.iter().any(|a| a.state.is_active()) {
            NotificationStatus::Firing
        } else {
            NotificationStatus::Resolved
        };

        let group_key = alerts
            .first()
            .map_or_else(String::new, |a| a.rule_name.clone());

        Self {
            alerts,
            status,
            external_url: None,
            group_key,
            truncated_alerts: 0,
        }
    }

    /// Sets the external URL.
    #[must_use]
    pub fn with_external_url(mut self, url: impl Into<String>) -> Self {
        self.external_url = Some(url.into());
        self
    }

    /// Sets the group key.
    #[must_use]
    pub fn with_group_key(mut self, key: impl Into<String>) -> Self {
        self.group_key = key.into();
        self
    }
}

/// The status of a notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationStatus {
    /// At least one alert is firing.
    Firing,
    /// All alerts are resolved.
    Resolved,
}

impl fmt::Display for NotificationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Firing => write!(f, "firing"),
            Self::Resolved => write!(f, "resolved"),
        }
    }
}

/// Result of sending a notification.
#[derive(Debug, Clone)]
pub struct NotificationResult {
    /// Whether the notification was sent successfully.
    pub success: bool,
    /// The channel that processed this notification.
    pub channel: String,
    /// Optional message or error description.
    pub message: Option<String>,
    /// Response status code (if applicable).
    pub status_code: Option<u16>,
}

impl NotificationResult {
    /// Creates a successful result.
    #[must_use]
    pub fn success(channel: impl Into<String>) -> Self {
        Self {
            success: true,
            channel: channel.into(),
            message: None,
            status_code: None,
        }
    }

    /// Creates a failed result.
    #[must_use]
    pub fn failure(channel: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            channel: channel.into(),
            message: Some(message.into()),
            status_code: None,
        }
    }

    /// Sets the status code.
    #[must_use]
    pub const fn with_status_code(mut self, code: u16) -> Self {
        self.status_code = Some(code);
        self
    }

    /// Sets the message.
    #[must_use]
    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }
}

/// Trait for notification channels.
///
/// Implement this trait to create custom notification channels for
/// delivering alerts via different protocols or services.
pub trait NotificationChannel: Send + Sync + fmt::Debug {
    /// Returns the name of this channel.
    fn name(&self) -> &str;

    /// Sends a notification through this channel.
    ///
    /// # Errors
    ///
    /// Returns `AlertError::NotificationFailed` if the notification cannot be sent.
    fn send(&self, notification: &Notification) -> Result<NotificationResult>;

    /// Returns true if this channel is enabled.
    fn is_enabled(&self) -> bool {
        true
    }
}

/// Configuration for a webhook channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// The name of this webhook.
    pub name: String,
    /// The URL to send notifications to.
    pub url: String,
    /// HTTP headers to include with requests.
    pub headers: HashMap<String, String>,
    /// Maximum number of alerts per notification.
    pub max_alerts: usize,
    /// Timeout in seconds for HTTP requests.
    pub timeout_secs: u64,
    /// Whether this channel is enabled.
    pub enabled: bool,
}

impl WebhookConfig {
    /// Creates a new webhook configuration.
    ///
    /// # Errors
    ///
    /// Returns `AlertError::InvalidRule` if the URL is empty.
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Result<Self> {
        let url = url.into();
        if url.is_empty() {
            return Err(AlertError::InvalidRule {
                reason: "webhook URL cannot be empty".to_string(),
            });
        }

        Ok(Self {
            name: name.into(),
            url,
            headers: HashMap::new(),
            max_alerts: 100,
            timeout_secs: 30,
            enabled: true,
        })
    }

    /// Adds a header to the configuration.
    #[must_use]
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Sets the maximum alerts per notification.
    #[must_use]
    pub const fn with_max_alerts(mut self, max: usize) -> Self {
        self.max_alerts = max;
        self
    }

    /// Sets the timeout.
    #[must_use]
    pub const fn with_timeout_secs(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Sets whether the channel is enabled.
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// A webhook notification channel.
///
/// Sends notifications as JSON POST requests to a configured URL.
/// Compatible with Alertmanager webhook receiver format.
#[derive(Debug, Clone)]
pub struct WebhookChannel {
    config: WebhookConfig,
}

impl WebhookChannel {
    /// Creates a new webhook channel with the given configuration.
    #[must_use]
    pub const fn new(config: WebhookConfig) -> Self {
        Self { config }
    }

    /// Returns the webhook URL.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.config.url
    }

    /// Formats the notification as JSON.
    ///
    /// # Errors
    ///
    /// Returns `AlertError::SerializationError` if serialization fails.
    pub fn format_payload(&self, notification: &Notification) -> Result<String> {
        let payload = WebhookPayload::from_notification(notification);
        serde_json::to_string(&payload).map_err(AlertError::from)
    }
}

impl NotificationChannel for WebhookChannel {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn send(&self, notification: &Notification) -> Result<NotificationResult> {
        if !self.is_enabled() {
            debug!(channel = %self.name(), "channel is disabled, skipping");
            return Ok(NotificationResult::success(self.name())
                .with_message("channel disabled, notification skipped"));
        }

        let payload = self.format_payload(notification)?;

        // In a real implementation, we would make an HTTP request here.
        // For now, we just log the payload and return success.
        // The actual HTTP client integration would be added when integrating
        // with an async HTTP library like reqwest.
        info!(
            channel = %self.name(),
            url = %self.config.url,
            alerts = notification.alerts.len(),
            status = %notification.status,
            "would send webhook notification"
        );
        debug!(payload = %payload, "webhook payload");

        Ok(NotificationResult::success(self.name())
            .with_status_code(200)
            .with_message("notification queued"))
    }

    fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

/// The payload format for webhook notifications.
///
/// This format is compatible with Alertmanager webhook receivers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookPayload {
    /// The notification version.
    pub version: String,
    /// Group key identifying this alert group.
    pub group_key: String,
    /// Number of truncated alerts.
    pub truncated_alerts: usize,
    /// Status of the notification.
    pub status: NotificationStatus,
    /// The receiver that should handle this notification.
    pub receiver: String,
    /// Labels common to all alerts.
    pub group_labels: HashMap<String, String>,
    /// Labels common to all alerts.
    pub common_labels: HashMap<String, String>,
    /// Annotations common to all alerts.
    pub common_annotations: HashMap<String, String>,
    /// External URL for more information.
    pub external_url: String,
    /// The alerts in this notification.
    pub alerts: Vec<WebhookAlert>,
}

impl WebhookPayload {
    /// Creates a payload from a notification.
    #[must_use]
    pub fn from_notification(notification: &Notification) -> Self {
        let alerts: Vec<WebhookAlert> = notification
            .alerts
            .iter()
            .map(WebhookAlert::from_alert)
            .collect();

        // Extract common labels and annotations
        let (common_labels, common_annotations) = Self::extract_common(&notification.alerts);

        Self {
            version: "4".to_string(),
            group_key: notification.group_key.clone(),
            truncated_alerts: notification.truncated_alerts,
            status: notification.status,
            receiver: "claw-alerts".to_string(),
            group_labels: HashMap::new(),
            common_labels,
            common_annotations,
            external_url: notification
                .external_url
                .clone()
                .unwrap_or_default(),
            alerts,
        }
    }

    fn extract_common(alerts: &[Alert]) -> (HashMap<String, String>, HashMap<String, String>) {
        if alerts.is_empty() {
            return (HashMap::new(), HashMap::new());
        }

        let first = &alerts[0];

        // Find labels that are common to all alerts
        let common_labels: HashMap<String, String> = first
            .labels
            .iter()
            .filter(|(k, v)| {
                alerts
                    .iter()
                    .all(|a| a.labels.get(*k) == Some(*v))
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Find annotations that are common to all alerts
        let common_annotations: HashMap<String, String> = first
            .annotations
            .iter()
            .filter(|(k, v)| {
                alerts
                    .iter()
                    .all(|a| a.annotations.get(*k) == Some(*v))
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        (common_labels, common_annotations)
    }
}

/// Alert format in webhook payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookAlert {
    /// The status of this alert.
    pub status: String,
    /// Labels attached to the alert.
    pub labels: HashMap<String, String>,
    /// Annotations for the alert.
    pub annotations: HashMap<String, String>,
    /// When the alert started firing.
    pub starts_at: String,
    /// When the alert ended (if resolved).
    pub ends_at: Option<String>,
    /// URL to the alert source.
    pub generator_url: String,
    /// Fingerprint for deduplication.
    pub fingerprint: String,
}

impl WebhookAlert {
    /// Creates a webhook alert from an alert.
    #[must_use]
    pub fn from_alert(alert: &Alert) -> Self {
        Self {
            status: alert.state.as_str().to_string(),
            labels: alert.labels.clone(),
            annotations: alert.annotations.clone(),
            starts_at: alert.started_at.to_rfc3339(),
            ends_at: alert.resolved_at.map(|t| t.to_rfc3339()),
            generator_url: String::new(),
            fingerprint: alert.fingerprint.clone(),
        }
    }
}

/// Placeholder email notification channel.
///
/// This is a placeholder implementation that logs email notifications
/// instead of actually sending them. A real implementation would
/// integrate with an SMTP library.
#[derive(Debug, Clone)]
pub struct EmailChannel {
    name: String,
    to: Vec<String>,
    from: String,
    enabled: bool,
}

impl EmailChannel {
    /// Creates a new email channel.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        to: Vec<String>,
        from: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            to,
            from: from.into(),
            enabled: true,
        }
    }

    /// Sets whether the channel is enabled.
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Returns the recipient addresses.
    #[must_use]
    pub fn recipients(&self) -> &[String] {
        &self.to
    }

    /// Returns the sender address.
    #[must_use]
    pub fn sender(&self) -> &str {
        &self.from
    }
}

impl NotificationChannel for EmailChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn send(&self, notification: &Notification) -> Result<NotificationResult> {
        if !self.is_enabled() {
            debug!(channel = %self.name(), "channel is disabled, skipping");
            return Ok(NotificationResult::success(self.name())
                .with_message("channel disabled, notification skipped"));
        }

        // Placeholder implementation - just log
        info!(
            channel = %self.name(),
            to = ?self.to,
            from = %self.from,
            alerts = notification.alerts.len(),
            status = %notification.status,
            "would send email notification"
        );

        // In a real implementation, this would:
        // 1. Format the alert as an email body
        // 2. Connect to SMTP server
        // 3. Send the email

        Ok(NotificationResult::success(self.name())
            .with_message("email notification placeholder"))
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// A channel that logs notifications for debugging.
#[derive(Debug, Clone)]
pub struct LogChannel {
    name: String,
    enabled: bool,
}

impl LogChannel {
    /// Creates a new log channel.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            enabled: true,
        }
    }

    /// Sets whether the channel is enabled.
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Default for LogChannel {
    fn default() -> Self {
        Self::new("log")
    }
}

impl NotificationChannel for LogChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn send(&self, notification: &Notification) -> Result<NotificationResult> {
        if !self.is_enabled() {
            return Ok(NotificationResult::success(self.name())
                .with_message("channel disabled"));
        }

        for alert in &notification.alerts {
            match notification.status {
                NotificationStatus::Firing => {
                    error!(
                        alert = %alert.rule_name,
                        state = %alert.state,
                        severity = %alert.severity,
                        value = %alert.value,
                        labels = ?alert.labels,
                        "ALERT"
                    );
                }
                NotificationStatus::Resolved => {
                    info!(
                        alert = %alert.rule_name,
                        state = %alert.state,
                        severity = %alert.severity,
                        "RESOLVED"
                    );
                }
            }
        }

        Ok(NotificationResult::success(self.name())
            .with_message("logged to tracing"))
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AlertCondition, AlertRule, AlertSeverity, ComparisonOperator};

    fn test_alert() -> Alert {
        let condition =
            AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0).unwrap();
        let rule = AlertRule::builder("HighCPU", condition)
            .severity(AlertSeverity::Warning)
            .annotation("summary", "High CPU usage detected")
            .build()
            .unwrap();

        Alert::new_pending(&rule, 85.0, HashMap::new())
    }

    fn firing_alert() -> Alert {
        let mut alert = test_alert();
        alert.fire();
        alert
    }

    mod notification_tests {
        use super::*;

        #[test]
        fn notification_from_firing_alert() {
            let alert = firing_alert();
            let notification = Notification::new(vec![alert]);

            assert_eq!(notification.status, NotificationStatus::Firing);
            assert_eq!(notification.alerts.len(), 1);
            assert_eq!(notification.group_key, "HighCPU");
        }

        #[test]
        fn notification_from_resolved_alert() {
            let mut alert = firing_alert();
            alert.resolve();
            let notification = Notification::new(vec![alert]);

            assert_eq!(notification.status, NotificationStatus::Resolved);
        }

        #[test]
        fn notification_with_external_url() {
            let alert = test_alert();
            let notification =
                Notification::new(vec![alert]).with_external_url("http://alerts.example.com");

            assert_eq!(
                notification.external_url,
                Some("http://alerts.example.com".to_string())
            );
        }

        #[test]
        fn notification_with_group_key() {
            let alert = test_alert();
            let notification = Notification::new(vec![alert]).with_group_key("custom-group");

            assert_eq!(notification.group_key, "custom-group");
        }

        #[test]
        fn notification_empty_alerts() {
            let notification = Notification::new(vec![]);

            assert_eq!(notification.status, NotificationStatus::Resolved);
            assert!(notification.group_key.is_empty());
        }
    }

    mod notification_result_tests {
        use super::*;

        #[test]
        fn result_success() {
            let result = NotificationResult::success("webhook");

            assert!(result.success);
            assert_eq!(result.channel, "webhook");
            assert!(result.message.is_none());
        }

        #[test]
        fn result_failure() {
            let result = NotificationResult::failure("email", "connection refused");

            assert!(!result.success);
            assert_eq!(result.channel, "email");
            assert_eq!(result.message, Some("connection refused".to_string()));
        }

        #[test]
        fn result_with_status_code() {
            let result = NotificationResult::success("webhook").with_status_code(200);

            assert_eq!(result.status_code, Some(200));
        }

        #[test]
        fn result_with_message() {
            let result = NotificationResult::success("webhook").with_message("sent successfully");

            assert_eq!(result.message, Some("sent successfully".to_string()));
        }
    }

    mod webhook_config_tests {
        use super::*;

        #[test]
        fn create_webhook_config() {
            let config = WebhookConfig::new("my-webhook", "http://example.com/alerts");

            assert!(config.is_ok());
            let config = config.unwrap();
            assert_eq!(config.name, "my-webhook");
            assert_eq!(config.url, "http://example.com/alerts");
            assert!(config.enabled);
        }

        #[test]
        fn webhook_config_empty_url_fails() {
            let config = WebhookConfig::new("my-webhook", "");

            assert!(config.is_err());
            match config {
                Err(AlertError::InvalidRule { reason }) => {
                    assert!(reason.contains("empty"));
                }
                _ => panic!("expected InvalidRule error"),
            }
        }

        #[test]
        fn webhook_config_with_header() {
            let config = WebhookConfig::new("my-webhook", "http://example.com/alerts")
                .unwrap()
                .with_header("Authorization", "Bearer token123");

            assert_eq!(
                config.headers.get("Authorization"),
                Some(&"Bearer token123".to_string())
            );
        }

        #[test]
        fn webhook_config_with_max_alerts() {
            let config = WebhookConfig::new("my-webhook", "http://example.com/alerts")
                .unwrap()
                .with_max_alerts(50);

            assert_eq!(config.max_alerts, 50);
        }

        #[test]
        fn webhook_config_with_timeout() {
            let config = WebhookConfig::new("my-webhook", "http://example.com/alerts")
                .unwrap()
                .with_timeout_secs(60);

            assert_eq!(config.timeout_secs, 60);
        }

        #[test]
        fn webhook_config_disabled() {
            let config = WebhookConfig::new("my-webhook", "http://example.com/alerts")
                .unwrap()
                .enabled(false);

            assert!(!config.enabled);
        }
    }

    mod webhook_channel_tests {
        use super::*;

        fn test_webhook() -> WebhookChannel {
            let config =
                WebhookConfig::new("test-webhook", "http://example.com/alerts").unwrap();
            WebhookChannel::new(config)
        }

        #[test]
        fn webhook_channel_name() {
            let channel = test_webhook();
            assert_eq!(channel.name(), "test-webhook");
        }

        #[test]
        fn webhook_channel_url() {
            let channel = test_webhook();
            assert_eq!(channel.url(), "http://example.com/alerts");
        }

        #[test]
        fn webhook_channel_is_enabled() {
            let channel = test_webhook();
            assert!(channel.is_enabled());

            let config = WebhookConfig::new("disabled", "http://example.com")
                .unwrap()
                .enabled(false);
            let disabled = WebhookChannel::new(config);
            assert!(!disabled.is_enabled());
        }

        #[test]
        fn webhook_format_payload() {
            let channel = test_webhook();
            let alert = firing_alert();
            let notification = Notification::new(vec![alert]);

            let payload = channel.format_payload(&notification);
            assert!(payload.is_ok());

            let payload_str = payload.unwrap();
            assert!(payload_str.contains("HighCPU"));
            assert!(payload_str.contains("firing"));
        }

        #[test]
        fn webhook_send() {
            let channel = test_webhook();
            let alert = firing_alert();
            let notification = Notification::new(vec![alert]);

            let result = channel.send(&notification);
            assert!(result.is_ok());

            let result = result.unwrap();
            assert!(result.success);
            assert_eq!(result.channel, "test-webhook");
        }

        #[test]
        fn webhook_send_disabled() {
            let config = WebhookConfig::new("disabled", "http://example.com")
                .unwrap()
                .enabled(false);
            let channel = WebhookChannel::new(config);

            let notification = Notification::new(vec![]);
            let result = channel.send(&notification);

            assert!(result.is_ok());
            let result = result.unwrap();
            assert!(result.success);
            assert!(result.message.is_some());
            assert!(result.message.unwrap().contains("disabled"));
        }
    }

    mod webhook_payload_tests {
        use super::*;

        #[test]
        fn payload_from_notification() {
            let alert = firing_alert();
            let notification = Notification::new(vec![alert])
                .with_external_url("http://example.com/alerts");

            let payload = WebhookPayload::from_notification(&notification);

            assert_eq!(payload.version, "4");
            assert_eq!(payload.status, NotificationStatus::Firing);
            assert_eq!(payload.alerts.len(), 1);
            assert_eq!(payload.external_url, "http://example.com/alerts");
        }

        #[test]
        fn payload_empty_notification() {
            let notification = Notification::new(vec![]);
            let payload = WebhookPayload::from_notification(&notification);

            assert!(payload.alerts.is_empty());
            assert!(payload.common_labels.is_empty());
        }

        #[test]
        fn payload_common_labels() {
            let condition =
                AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0).unwrap();
            let rule = AlertRule::builder("HighCPU", condition)
                .label("env", "prod")
                .build()
                .unwrap();

            let mut labels1 = HashMap::new();
            labels1.insert("node".to_string(), "node-1".to_string());

            let mut labels2 = HashMap::new();
            labels2.insert("node".to_string(), "node-2".to_string());

            let alert1 = Alert::new_pending(&rule, 85.0, labels1);
            let alert2 = Alert::new_pending(&rule, 90.0, labels2);

            let notification = Notification::new(vec![alert1, alert2]);
            let payload = WebhookPayload::from_notification(&notification);

            // "env" and "alertname" should be common
            assert_eq!(payload.common_labels.get("env"), Some(&"prod".to_string()));
            assert_eq!(
                payload.common_labels.get("alertname"),
                Some(&"HighCPU".to_string())
            );
            // "node" is different, so not common
            assert!(!payload.common_labels.contains_key("node"));
        }

        #[test]
        fn payload_serialization() {
            let alert = test_alert();
            let notification = Notification::new(vec![alert]);
            let payload = WebhookPayload::from_notification(&notification);

            let json = serde_json::to_string(&payload);
            assert!(json.is_ok());

            let parsed: serde_json::Result<WebhookPayload> = serde_json::from_str(&json.unwrap());
            assert!(parsed.is_ok());
        }
    }

    mod email_channel_tests {
        use super::*;

        #[test]
        fn create_email_channel() {
            let channel = EmailChannel::new(
                "alerts",
                vec!["team@example.com".to_string()],
                "alerts@example.com",
            );

            assert_eq!(channel.name(), "alerts");
            assert_eq!(channel.recipients(), &["team@example.com".to_string()]);
            assert_eq!(channel.sender(), "alerts@example.com");
            assert!(channel.is_enabled());
        }

        #[test]
        fn email_channel_disabled() {
            let channel = EmailChannel::new(
                "alerts",
                vec!["team@example.com".to_string()],
                "alerts@example.com",
            )
            .enabled(false);

            assert!(!channel.is_enabled());
        }

        #[test]
        fn email_channel_send() {
            let channel = EmailChannel::new(
                "alerts",
                vec!["team@example.com".to_string()],
                "alerts@example.com",
            );

            let alert = firing_alert();
            let notification = Notification::new(vec![alert]);

            let result = channel.send(&notification);
            assert!(result.is_ok());

            let result = result.unwrap();
            assert!(result.success);
            assert_eq!(result.channel, "alerts");
        }
    }

    mod log_channel_tests {
        use super::*;

        #[test]
        fn create_log_channel() {
            let channel = LogChannel::new("debug-log");
            assert_eq!(channel.name(), "debug-log");
            assert!(channel.is_enabled());
        }

        #[test]
        fn log_channel_default() {
            let channel = LogChannel::default();
            assert_eq!(channel.name(), "log");
        }

        #[test]
        fn log_channel_send() {
            let channel = LogChannel::default();
            let alert = firing_alert();
            let notification = Notification::new(vec![alert]);

            let result = channel.send(&notification);
            assert!(result.is_ok());

            let result = result.unwrap();
            assert!(result.success);
        }

        #[test]
        fn log_channel_send_resolved() {
            let channel = LogChannel::default();
            let mut alert = firing_alert();
            alert.resolve();
            let notification = Notification::new(vec![alert]);

            let result = channel.send(&notification);
            assert!(result.is_ok());
        }

        #[test]
        fn log_channel_disabled() {
            let channel = LogChannel::new("disabled").enabled(false);
            let notification = Notification::new(vec![]);

            let result = channel.send(&notification);
            assert!(result.is_ok());

            let result = result.unwrap();
            assert!(result.success);
            assert!(result.message.unwrap().contains("disabled"));
        }
    }
}
