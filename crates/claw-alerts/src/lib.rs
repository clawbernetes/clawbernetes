//! Alerting system with Prometheus Alertmanager-style rules for Clawbernetes.
//!
//! `claw-alerts` provides a comprehensive alerting system that evaluates
//! conditions against metrics and sends notifications through various channels.
//!
//! # Features
//!
//! - **Alert Rules**: Define conditions that trigger alerts with severity levels
//! - **For Duration**: Require conditions to be true for a duration before firing
//! - **Silences**: Suppress notifications for specific alerts
//! - **Notification Channels**: Send alerts via webhooks, email, or custom channels
//! - **Prometheus Compatible**: Webhook payload format compatible with Alertmanager receivers
//!
//! # Example
//!
//! ```rust
//! use claw_alerts::{
//!     AlertCondition, AlertManager, AlertRule, AlertSeverity, ComparisonOperator,
//!     channels::LogChannel,
//! };
//! use std::collections::HashMap;
//! use std::time::Duration;
//!
//! // Create an alert manager
//! let manager = AlertManager::new();
//!
//! // Add a notification channel
//! manager.add_channel(Box::new(LogChannel::default()));
//!
//! // Create a condition: cpu_usage > 80
//! let condition = AlertCondition::new("cpu_usage", ComparisonOperator::GreaterThan, 80.0).unwrap();
//!
//! // Create a rule that fires after 1 minute above threshold
//! let rule = AlertRule::builder("HighCPU", condition)
//!     .for_duration(Duration::from_secs(60))
//!     .severity(AlertSeverity::Warning)
//!     .label("team", "platform")
//!     .annotation("summary", "CPU usage is above 80%")
//!     .build()
//!     .unwrap();
//!
//! // Add the rule
//! manager.add_rule(rule).unwrap();
//!
//! // Evaluate rules with current metric values
//! let mut values = HashMap::new();
//! values.insert("cpu_usage".to_string(), 85.0);
//!
//! let result = manager.evaluate_with_values(&values).unwrap();
//! println!("Alerts fired: {:?}", result.alerts_fired);
//! ```
//!
//! # Integration with claw-metrics
//!
//! The alert manager can be configured with a `MetricStore` from `claw-metrics`
//! to automatically query metric values:
//!
//! ```rust,ignore
//! use claw_alerts::AlertManager;
//! use claw_metrics::MetricStore;
//! use std::time::Duration;
//!
//! let mut manager = AlertManager::new();
//! let store = MetricStore::new(Duration::from_secs(3600));
//!
//! // ... push metrics to store ...
//!
//! manager.set_metrics(store);
//! manager.evaluate()?; // Will query metrics from the store
//! ```
//!
//! # Silencing Alerts
//!
//! Silences can be created to suppress notifications for matching alerts:
//!
//! ```rust
//! use claw_alerts::Silence;
//! use chrono::{Utc, Duration};
//! use std::collections::HashMap;
//!
//! let mut matchers = HashMap::new();
//! matchers.insert("alertname".to_string(), "HighCPU".to_string());
//!
//! let silence = Silence::new(
//!     matchers,
//!     Utc::now(),
//!     Utc::now() + Duration::hours(4),
//!     "admin",
//!     "Maintenance window",
//! ).unwrap();
//! ```

#![forbid(unsafe_code)]
#![doc(html_root_url = "https://docs.rs/claw-alerts/0.1.0")]
#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]

pub mod channels;
pub mod error;
pub mod manager;
pub mod types;

// Re-export main types at crate root
pub use channels::{
    EmailChannel, LogChannel, Notification, NotificationChannel, NotificationResult,
    NotificationStatus, WebhookChannel, WebhookConfig,
};
pub use error::{AlertError, Result};
pub use manager::{AlertManager, AlertManagerConfig, EvaluationResult};
pub use types::{
    Alert, AlertCondition, AlertRule, AlertRuleBuilder, AlertSeverity, AlertState,
    ComparisonOperator, Silence,
};
