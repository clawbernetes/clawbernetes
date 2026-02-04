//! Audit logging backends.
//!
//! This module provides the [`AuditLogger`] trait and default implementations.

use crate::events::{AuditEvent, Severity};

/// Trait for audit logging backends.
///
/// Implement this trait to create custom audit log destinations
/// (e.g., file, database, external service).
pub trait AuditLogger: Send + Sync {
    /// Logs an audit event.
    fn log(&self, event: &AuditEvent);

    /// Logs an audit event if the severity is at or above the minimum.
    fn log_if_severe(&self, event: &AuditEvent, min_severity: Severity) {
        if event.severity() >= min_severity {
            self.log(event);
        }
    }
}

/// Audit logger that uses the `tracing` infrastructure.
///
/// Events are logged at appropriate tracing levels based on severity:
/// - Info, Low → `tracing::info!`
/// - Medium → `tracing::warn!`
/// - High, Critical → `tracing::error!`
#[derive(Debug, Clone, Default)]
pub struct TracingAuditLogger {
    /// Optional prefix for all log messages.
    prefix: Option<String>,
}

impl TracingAuditLogger {
    /// Creates a new tracing-based audit logger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new tracing-based audit logger with a prefix.
    #[must_use]
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: Some(prefix.into()),
        }
    }
}

impl AuditLogger for TracingAuditLogger {
    fn log(&self, event: &AuditEvent) {
        let event_id = event.event_id();
        let event_type = event.event_type();
        let severity = event.severity();
        let timestamp = event.timestamp();

        // Serialize to JSON for structured logging (ignore errors)
        let json = event.to_json().unwrap_or_else(|_| "{}".to_string());

        let prefix = self.prefix.as_deref().unwrap_or("AUDIT");

        match severity {
            Severity::Info | Severity::Low => {
                tracing::info!(
                    target: "claw_audit",
                    %event_id,
                    %event_type,
                    %severity,
                    %timestamp,
                    event_json = %json,
                    "[{prefix}] {event_type}"
                );
            }
            Severity::Medium => {
                tracing::warn!(
                    target: "claw_audit",
                    %event_id,
                    %event_type,
                    %severity,
                    %timestamp,
                    event_json = %json,
                    "[{prefix}] {event_type}"
                );
            }
            Severity::High | Severity::Critical => {
                tracing::error!(
                    target: "claw_audit",
                    %event_id,
                    %event_type,
                    %severity,
                    %timestamp,
                    event_json = %json,
                    "[{prefix}] {event_type}"
                );
            }
        }
    }
}

/// A no-op audit logger for testing or disabled scenarios.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopAuditLogger;

impl NoopAuditLogger {
    /// Creates a new no-op audit logger.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl AuditLogger for NoopAuditLogger {
    fn log(&self, _event: &AuditEvent) {
        // Intentionally does nothing
    }
}

/// A boxed audit logger for dynamic dispatch.
pub type BoxedAuditLogger = Box<dyn AuditLogger>;

impl AuditLogger for BoxedAuditLogger {
    fn log(&self, event: &AuditEvent) {
        (**self).log(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A test logger that counts calls.
    #[derive(Debug, Default)]
    struct CountingLogger {
        count: AtomicUsize,
    }

    impl AuditLogger for CountingLogger {
        fn log(&self, _event: &AuditEvent) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn tracing_logger_creation() {
        let logger = TracingAuditLogger::new();
        assert!(logger.prefix.is_none());

        let logger = TracingAuditLogger::with_prefix("SECURITY");
        assert_eq!(logger.prefix, Some("SECURITY".to_string()));
    }

    #[test]
    fn tracing_logger_logs_events() {
        // This test just verifies the logger doesn't panic
        let logger = TracingAuditLogger::new();
        
        let event = AuditEvent::authentication_success("test");
        logger.log(&event);

        let event = AuditEvent::authentication_failure("reason", "source");
        logger.log(&event);

        let event = AuditEvent::signature_verification_failed("ed25519", "invalid");
        logger.log(&event);
    }

    #[test]
    fn tracing_logger_with_prefix() {
        let logger = TracingAuditLogger::with_prefix("CUSTOM");
        let event = AuditEvent::authentication_success("test");
        logger.log(&event); // Should not panic
    }

    #[test]
    fn noop_logger_does_nothing() {
        let logger = NoopAuditLogger::new();
        let event = AuditEvent::authentication_success("test");
        logger.log(&event); // Should not panic
    }

    #[test]
    fn counting_logger_tracks_calls() {
        let logger = CountingLogger::default();
        
        assert_eq!(logger.count.load(Ordering::SeqCst), 0);
        
        logger.log(&AuditEvent::authentication_success("test"));
        assert_eq!(logger.count.load(Ordering::SeqCst), 1);
        
        logger.log(&AuditEvent::authentication_failure("reason", "source"));
        assert_eq!(logger.count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn log_if_severe_filters_correctly() {
        let logger = CountingLogger::default();
        
        // Info severity should be logged when min is Info
        let info_event = AuditEvent::authentication_success("test");
        logger.log_if_severe(&info_event, Severity::Info);
        assert_eq!(logger.count.load(Ordering::SeqCst), 1);
        
        // Info severity should NOT be logged when min is Medium
        logger.log_if_severe(&info_event, Severity::Medium);
        assert_eq!(logger.count.load(Ordering::SeqCst), 1); // Still 1
        
        // Critical should be logged when min is Medium
        let critical_event = AuditEvent::signature_verification_failed("ed25519", "invalid");
        logger.log_if_severe(&critical_event, Severity::Medium);
        assert_eq!(logger.count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn boxed_logger_works() {
        let boxed: BoxedAuditLogger = Box::new(TracingAuditLogger::new());
        let event = AuditEvent::authentication_success("test");
        boxed.log(&event); // Should not panic
    }

    #[test]
    fn logger_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TracingAuditLogger>();
        assert_send_sync::<NoopAuditLogger>();
    }

    #[test]
    fn logger_in_arc() {
        let logger: Arc<dyn AuditLogger> = Arc::new(TracingAuditLogger::new());
        let event = AuditEvent::authentication_success("test");
        logger.log(&event);
    }

    #[test]
    fn all_severity_levels_log() {
        let logger = TracingAuditLogger::new();
        
        // Info level (authentication success)
        logger.log(&AuditEvent::authentication_success("test"));
        
        // Low level (rate limit)
        logger.log(&AuditEvent::rate_limit_exceeded("limit", 10, 5, 60, "source"));
        
        // Medium level (authentication failure)
        logger.log(&AuditEvent::authentication_failure("reason", "source"));
        
        // Critical level (signature verification)
        logger.log(&AuditEvent::signature_verification_failed("ed25519", "reason"));
        
        // All should complete without panic
    }

    #[test]
    fn noop_default() {
        let logger = NoopAuditLogger::default();
        logger.log(&AuditEvent::authentication_success("test"));
    }

    #[test]
    fn tracing_logger_clone() {
        let logger = TracingAuditLogger::with_prefix("TEST");
        let cloned = logger.clone();
        assert_eq!(logger.prefix, cloned.prefix);
    }
}
