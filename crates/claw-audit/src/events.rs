//! Security audit event types.
//!
//! This module defines all security-relevant events that can be logged.

use crate::error::{AuditError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

/// Severity level for audit events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational event (e.g., successful auth).
    Info = 0,
    /// Low severity (e.g., single rate limit hit).
    Low = 1,
    /// Medium severity (e.g., auth failure).
    Medium = 2,
    /// High severity (e.g., repeated auth failures).
    High = 3,
    /// Critical severity (e.g., signature verification failure).
    Critical = 4,
}

impl Severity {
    /// Returns the string representation of this severity.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Authentication attempt details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthAttempt {
    /// Whether authentication succeeded.
    pub success: bool,
    /// Reason for failure (if applicable).
    pub reason: Option<String>,
    /// Source IP or identifier.
    pub source: String,
    /// Authentication method used.
    pub method: Option<String>,
    /// Username or identity attempted.
    pub identity: Option<String>,
}

/// Authorization failure context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationContext {
    /// Resource being accessed.
    pub resource: String,
    /// Action attempted.
    pub action: String,
    /// Reason for denial.
    pub reason: String,
    /// Required permissions.
    pub required_permissions: Vec<String>,
    /// Actual permissions held.
    pub actual_permissions: Vec<String>,
}

/// Escrow state change details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscrowChange {
    /// Escrow contract identifier.
    pub escrow_id: String,
    /// Previous state.
    pub previous_state: String,
    /// New state.
    pub new_state: String,
    /// Amount involved (as string to preserve precision).
    pub amount: Option<String>,
    /// Parties involved.
    pub parties: Vec<String>,
}

/// Rate limit violation details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitViolation {
    /// The limit that was exceeded.
    pub limit_name: String,
    /// Current count.
    pub current_count: u64,
    /// Maximum allowed.
    pub max_allowed: u64,
    /// Time window in seconds.
    pub window_seconds: u64,
    /// Source being rate limited.
    pub source: String,
}

/// Signature verification failure details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureFailure {
    /// Type of signature (e.g., "ed25519", "transaction").
    pub signature_type: String,
    /// Reason for failure.
    pub reason: String,
    /// Public key involved (if known).
    pub public_key: Option<String>,
    /// Message hash (truncated for security).
    pub message_hash: Option<String>,
}

/// Unusual pattern detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnusualPattern {
    /// Type of pattern detected.
    pub pattern_type: String,
    /// Description of the anomaly.
    pub description: String,
    /// Confidence score (0-100).
    pub confidence: u8,
    /// Related event IDs.
    pub related_events: Vec<Uuid>,
}

/// Security audit event.
///
/// This enum covers all security-relevant events that should be logged
/// for forensics and monitoring purposes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuditEvent {
    /// Authentication attempt (success or failure).
    Authentication {
        /// Unique event identifier.
        event_id: Uuid,
        /// When the event occurred.
        timestamp: DateTime<Utc>,
        /// Severity level.
        severity: Severity,
        /// Actor identifier (user, service, etc.).
        actor_id: Option<Uuid>,
        /// Node where event occurred.
        node_id: Option<Uuid>,
        /// Authentication details.
        details: AuthAttempt,
        /// Additional metadata.
        #[serde(default)]
        metadata: HashMap<String, serde_json::Value>,
    },

    /// Authorization failure.
    AuthorizationFailure {
        /// Unique event identifier.
        event_id: Uuid,
        /// When the event occurred.
        timestamp: DateTime<Utc>,
        /// Severity level.
        severity: Severity,
        /// Actor who was denied.
        actor_id: Option<Uuid>,
        /// Node where event occurred.
        node_id: Option<Uuid>,
        /// Authorization context.
        context: AuthorizationContext,
        /// Additional metadata.
        #[serde(default)]
        metadata: HashMap<String, serde_json::Value>,
    },

    /// Escrow state change.
    EscrowStateChange {
        /// Unique event identifier.
        event_id: Uuid,
        /// When the event occurred.
        timestamp: DateTime<Utc>,
        /// Severity level.
        severity: Severity,
        /// Actor who triggered the change.
        actor_id: Option<Uuid>,
        /// Node where event occurred.
        node_id: Option<Uuid>,
        /// Escrow change details.
        change: EscrowChange,
        /// Additional metadata.
        #[serde(default)]
        metadata: HashMap<String, serde_json::Value>,
    },

    /// Rate limit violation.
    RateLimit {
        /// Unique event identifier.
        event_id: Uuid,
        /// When the event occurred.
        timestamp: DateTime<Utc>,
        /// Severity level.
        severity: Severity,
        /// Actor being rate limited.
        actor_id: Option<Uuid>,
        /// Node where event occurred.
        node_id: Option<Uuid>,
        /// Rate limit details.
        violation: RateLimitViolation,
        /// Additional metadata.
        #[serde(default)]
        metadata: HashMap<String, serde_json::Value>,
    },

    /// Signature verification failure.
    SignatureVerification {
        /// Unique event identifier.
        event_id: Uuid,
        /// When the event occurred.
        timestamp: DateTime<Utc>,
        /// Severity level.
        severity: Severity,
        /// Actor associated with the signature.
        actor_id: Option<Uuid>,
        /// Node where verification failed.
        node_id: Option<Uuid>,
        /// Failure details.
        failure: SignatureFailure,
        /// Additional metadata.
        #[serde(default)]
        metadata: HashMap<String, serde_json::Value>,
    },

    /// Unusual pattern detected.
    UnusualPatternDetected {
        /// Unique event identifier.
        event_id: Uuid,
        /// When the event occurred.
        timestamp: DateTime<Utc>,
        /// Severity level.
        severity: Severity,
        /// Actor associated with the pattern.
        actor_id: Option<Uuid>,
        /// Node where pattern was detected.
        node_id: Option<Uuid>,
        /// Pattern details.
        pattern: UnusualPattern,
        /// Additional metadata.
        #[serde(default)]
        metadata: HashMap<String, serde_json::Value>,
    },
}

impl AuditEvent {
    /// Creates a new event builder.
    #[must_use]
    pub fn builder() -> AuditEventBuilder {
        AuditEventBuilder::default()
    }

    /// Creates an authentication success event.
    #[must_use]
    pub fn authentication_success(source: impl Into<String>) -> Self {
        Self::Authentication {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            severity: Severity::Info,
            actor_id: None,
            node_id: None,
            details: AuthAttempt {
                success: true,
                reason: None,
                source: source.into(),
                method: None,
                identity: None,
            },
            metadata: HashMap::new(),
        }
    }

    /// Creates an authentication failure event.
    #[must_use]
    pub fn authentication_failure(reason: impl Into<String>, source: impl Into<String>) -> Self {
        Self::Authentication {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            severity: Severity::Medium,
            actor_id: None,
            node_id: None,
            details: AuthAttempt {
                success: false,
                reason: Some(reason.into()),
                source: source.into(),
                method: None,
                identity: None,
            },
            metadata: HashMap::new(),
        }
    }

    /// Creates a rate limit violation event.
    #[must_use]
    pub fn rate_limit_exceeded(
        limit_name: impl Into<String>,
        current: u64,
        max: u64,
        window_secs: u64,
        source: impl Into<String>,
    ) -> Self {
        Self::RateLimit {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            severity: Severity::Low,
            actor_id: None,
            node_id: None,
            violation: RateLimitViolation {
                limit_name: limit_name.into(),
                current_count: current,
                max_allowed: max,
                window_seconds: window_secs,
                source: source.into(),
            },
            metadata: HashMap::new(),
        }
    }

    /// Creates a signature verification failure event.
    #[must_use]
    pub fn signature_verification_failed(
        sig_type: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::SignatureVerification {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            severity: Severity::Critical,
            actor_id: None,
            node_id: None,
            failure: SignatureFailure {
                signature_type: sig_type.into(),
                reason: reason.into(),
                public_key: None,
                message_hash: None,
            },
            metadata: HashMap::new(),
        }
    }

    /// Returns the event ID.
    #[must_use]
    pub const fn event_id(&self) -> Uuid {
        match self {
            Self::Authentication { event_id, .. }
            | Self::AuthorizationFailure { event_id, .. }
            | Self::EscrowStateChange { event_id, .. }
            | Self::RateLimit { event_id, .. }
            | Self::SignatureVerification { event_id, .. }
            | Self::UnusualPatternDetected { event_id, .. } => *event_id,
        }
    }

    /// Returns the event timestamp.
    #[must_use]
    pub const fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::Authentication { timestamp, .. }
            | Self::AuthorizationFailure { timestamp, .. }
            | Self::EscrowStateChange { timestamp, .. }
            | Self::RateLimit { timestamp, .. }
            | Self::SignatureVerification { timestamp, .. }
            | Self::UnusualPatternDetected { timestamp, .. } => *timestamp,
        }
    }

    /// Returns the severity level.
    #[must_use]
    pub const fn severity(&self) -> Severity {
        match self {
            Self::Authentication { severity, .. }
            | Self::AuthorizationFailure { severity, .. }
            | Self::EscrowStateChange { severity, .. }
            | Self::RateLimit { severity, .. }
            | Self::SignatureVerification { severity, .. }
            | Self::UnusualPatternDetected { severity, .. } => *severity,
        }
    }

    /// Returns the event type as a string.
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::Authentication { .. } => "authentication",
            Self::AuthorizationFailure { .. } => "authorization_failure",
            Self::EscrowStateChange { .. } => "escrow_state_change",
            Self::RateLimit { .. } => "rate_limit",
            Self::SignatureVerification { .. } => "signature_verification",
            Self::UnusualPatternDetected { .. } => "unusual_pattern",
        }
    }

    /// Serializes the event to JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(AuditError::from)
    }

    /// Serializes the event to pretty-printed JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json_pretty(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(AuditError::from)
    }
}

/// Builder for creating audit events with fluent API.
#[derive(Debug, Default)]
pub struct AuditEventBuilder {
    event_type: Option<EventType>,
    event_id: Option<Uuid>,
    timestamp: Option<DateTime<Utc>>,
    severity: Option<Severity>,
    actor_id: Option<Uuid>,
    node_id: Option<Uuid>,
    metadata: HashMap<String, serde_json::Value>,
    // Type-specific fields
    auth_attempt: Option<AuthAttempt>,
    auth_context: Option<AuthorizationContext>,
    escrow_change: Option<EscrowChange>,
    rate_violation: Option<RateLimitViolation>,
    sig_failure: Option<SignatureFailure>,
    unusual_pattern: Option<UnusualPattern>,
}

#[derive(Debug, Clone, Copy)]
enum EventType {
    Authentication,
    AuthorizationFailure,
    EscrowStateChange,
    RateLimit,
    SignatureVerification,
    UnusualPattern,
}

impl AuditEventBuilder {
    /// Sets this as an authentication event.
    #[must_use]
    pub fn authentication(mut self) -> Self {
        self.event_type = Some(EventType::Authentication);
        self
    }

    /// Sets this as an authorization failure event.
    #[must_use]
    pub fn authorization_failure(mut self) -> Self {
        self.event_type = Some(EventType::AuthorizationFailure);
        if self.severity.is_none() {
            self.severity = Some(Severity::Medium);
        }
        self
    }

    /// Sets this as an escrow state change event.
    #[must_use]
    pub fn escrow_state_change(mut self) -> Self {
        self.event_type = Some(EventType::EscrowStateChange);
        if self.severity.is_none() {
            self.severity = Some(Severity::Info);
        }
        self
    }

    /// Sets this as a rate limit event.
    #[must_use]
    pub fn rate_limit(mut self) -> Self {
        self.event_type = Some(EventType::RateLimit);
        if self.severity.is_none() {
            self.severity = Some(Severity::Low);
        }
        self
    }

    /// Sets this as a signature verification event.
    #[must_use]
    pub fn signature_verification(mut self) -> Self {
        self.event_type = Some(EventType::SignatureVerification);
        if self.severity.is_none() {
            self.severity = Some(Severity::Critical);
        }
        self
    }

    /// Sets this as an unusual pattern event.
    #[must_use]
    pub fn unusual_pattern(mut self) -> Self {
        self.event_type = Some(EventType::UnusualPattern);
        if self.severity.is_none() {
            self.severity = Some(Severity::High);
        }
        self
    }

    /// Sets the event ID (auto-generated if not set).
    #[must_use]
    pub const fn event_id(mut self, id: Uuid) -> Self {
        self.event_id = Some(id);
        self
    }

    /// Sets the timestamp (defaults to now).
    #[must_use]
    pub const fn timestamp(mut self, ts: DateTime<Utc>) -> Self {
        self.timestamp = Some(ts);
        self
    }

    /// Sets the severity level.
    #[must_use]
    pub const fn severity(mut self, severity: Severity) -> Self {
        self.severity = Some(severity);
        self
    }

    /// Sets the actor ID.
    #[must_use]
    pub const fn actor_id(mut self, id: Uuid) -> Self {
        self.actor_id = Some(id);
        self
    }

    /// Sets the node ID.
    #[must_use]
    pub const fn node_id(mut self, id: Uuid) -> Self {
        self.node_id = Some(id);
        self
    }

    /// Adds metadata.
    #[must_use]
    pub fn metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    // Authentication-specific setters

    /// Sets whether authentication succeeded.
    #[must_use]
    pub fn success(mut self, success: bool) -> Self {
        let attempt = self.auth_attempt.get_or_insert_with(|| AuthAttempt {
            success: false,
            reason: None,
            source: String::new(),
            method: None,
            identity: None,
        });
        attempt.success = success;
        if success && self.severity.is_none() {
            self.severity = Some(Severity::Info);
        } else if !success && self.severity.is_none() {
            self.severity = Some(Severity::Medium);
        }
        self
    }

    /// Sets the source (IP, identifier, etc.).
    #[must_use]
    pub fn source(mut self, source: impl Into<String>) -> Self {
        let attempt = self.auth_attempt.get_or_insert_with(|| AuthAttempt {
            success: false,
            reason: None,
            source: String::new(),
            method: None,
            identity: None,
        });
        attempt.source = source.into();
        self
    }

    /// Sets the authentication method.
    #[must_use]
    pub fn method(mut self, method: impl Into<String>) -> Self {
        let attempt = self.auth_attempt.get_or_insert_with(|| AuthAttempt {
            success: false,
            reason: None,
            source: String::new(),
            method: None,
            identity: None,
        });
        attempt.method = Some(method.into());
        self
    }

    /// Sets the identity (username, etc.).
    #[must_use]
    pub fn identity(mut self, identity: impl Into<String>) -> Self {
        let attempt = self.auth_attempt.get_or_insert_with(|| AuthAttempt {
            success: false,
            reason: None,
            source: String::new(),
            method: None,
            identity: None,
        });
        attempt.identity = Some(identity.into());
        self
    }

    // Authorization-specific setters

    /// Sets the resource being accessed.
    #[must_use]
    pub fn resource(mut self, resource: impl Into<String>) -> Self {
        let ctx = self.auth_context.get_or_insert_with(|| AuthorizationContext {
            resource: String::new(),
            action: String::new(),
            reason: String::new(),
            required_permissions: Vec::new(),
            actual_permissions: Vec::new(),
        });
        ctx.resource = resource.into();
        self
    }

    /// Sets the action attempted.
    #[must_use]
    pub fn action(mut self, action: impl Into<String>) -> Self {
        let ctx = self.auth_context.get_or_insert_with(|| AuthorizationContext {
            resource: String::new(),
            action: String::new(),
            reason: String::new(),
            required_permissions: Vec::new(),
            actual_permissions: Vec::new(),
        });
        ctx.action = action.into();
        self
    }

    /// Sets the reason for denial/failure.
    #[must_use]
    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        let reason_str = reason.into();
        if let Some(ref mut ctx) = self.auth_context {
            ctx.reason.clone_from(&reason_str);
        }
        if let Some(ref mut attempt) = self.auth_attempt {
            attempt.reason = Some(reason_str);
        }
        self
    }

    // Escrow-specific setters

    /// Sets the escrow ID.
    #[must_use]
    pub fn escrow_id(mut self, id: impl Into<String>) -> Self {
        let change = self.escrow_change.get_or_insert_with(|| EscrowChange {
            escrow_id: String::new(),
            previous_state: String::new(),
            new_state: String::new(),
            amount: None,
            parties: Vec::new(),
        });
        change.escrow_id = id.into();
        self
    }

    /// Sets the state transition.
    #[must_use]
    pub fn state_transition(
        mut self,
        from: impl Into<String>,
        to: impl Into<String>,
    ) -> Self {
        let change = self.escrow_change.get_or_insert_with(|| EscrowChange {
            escrow_id: String::new(),
            previous_state: String::new(),
            new_state: String::new(),
            amount: None,
            parties: Vec::new(),
        });
        change.previous_state = from.into();
        change.new_state = to.into();
        self
    }

    /// Sets the amount involved.
    #[must_use]
    pub fn amount(mut self, amount: impl Into<String>) -> Self {
        let change = self.escrow_change.get_or_insert_with(|| EscrowChange {
            escrow_id: String::new(),
            previous_state: String::new(),
            new_state: String::new(),
            amount: None,
            parties: Vec::new(),
        });
        change.amount = Some(amount.into());
        self
    }

    // Rate limit-specific setters

    /// Sets the rate limit details.
    #[must_use]
    pub fn limit(
        mut self,
        name: impl Into<String>,
        current: u64,
        max: u64,
        window_secs: u64,
    ) -> Self {
        let violation = self.rate_violation.get_or_insert_with(|| RateLimitViolation {
            limit_name: String::new(),
            current_count: 0,
            max_allowed: 0,
            window_seconds: 0,
            source: String::new(),
        });
        violation.limit_name = name.into();
        violation.current_count = current;
        violation.max_allowed = max;
        violation.window_seconds = window_secs;
        self
    }

    /// Sets the rate limit source.
    #[must_use]
    pub fn limit_source(mut self, source: impl Into<String>) -> Self {
        let violation = self.rate_violation.get_or_insert_with(|| RateLimitViolation {
            limit_name: String::new(),
            current_count: 0,
            max_allowed: 0,
            window_seconds: 0,
            source: String::new(),
        });
        violation.source = source.into();
        self
    }

    // Signature-specific setters

    /// Sets the signature type.
    #[must_use]
    pub fn signature_type(mut self, sig_type: impl Into<String>) -> Self {
        let failure = self.sig_failure.get_or_insert_with(|| SignatureFailure {
            signature_type: String::new(),
            reason: String::new(),
            public_key: None,
            message_hash: None,
        });
        failure.signature_type = sig_type.into();
        self
    }

    /// Sets the failure reason for signature verification.
    #[must_use]
    pub fn sig_reason(mut self, reason: impl Into<String>) -> Self {
        let failure = self.sig_failure.get_or_insert_with(|| SignatureFailure {
            signature_type: String::new(),
            reason: String::new(),
            public_key: None,
            message_hash: None,
        });
        failure.reason = reason.into();
        self
    }

    /// Sets the public key involved.
    #[must_use]
    pub fn public_key(mut self, key: impl Into<String>) -> Self {
        let failure = self.sig_failure.get_or_insert_with(|| SignatureFailure {
            signature_type: String::new(),
            reason: String::new(),
            public_key: None,
            message_hash: None,
        });
        failure.public_key = Some(key.into());
        self
    }

    // Pattern-specific setters

    /// Sets the pattern type.
    #[must_use]
    pub fn pattern_type(mut self, pt: impl Into<String>) -> Self {
        let pattern = self.unusual_pattern.get_or_insert_with(|| UnusualPattern {
            pattern_type: String::new(),
            description: String::new(),
            confidence: 0,
            related_events: Vec::new(),
        });
        pattern.pattern_type = pt.into();
        self
    }

    /// Sets the pattern description.
    #[must_use]
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        let pattern = self.unusual_pattern.get_or_insert_with(|| UnusualPattern {
            pattern_type: String::new(),
            description: String::new(),
            confidence: 0,
            related_events: Vec::new(),
        });
        pattern.description = desc.into();
        self
    }

    /// Sets the confidence score.
    #[must_use]
    pub fn confidence(mut self, score: u8) -> Self {
        let pattern = self.unusual_pattern.get_or_insert_with(|| UnusualPattern {
            pattern_type: String::new(),
            description: String::new(),
            confidence: 0,
            related_events: Vec::new(),
        });
        pattern.confidence = score.min(100);
        self
    }

    /// Builds the audit event.
    ///
    /// # Errors
    ///
    /// Returns an error if required fields are missing.
    pub fn build(self) -> Result<AuditEvent> {
        let event_type = self
            .event_type
            .ok_or(AuditError::MissingField("event_type"))?;
        let event_id = self.event_id.unwrap_or_else(Uuid::new_v4);
        let timestamp = self.timestamp.unwrap_or_else(Utc::now);
        let severity = self.severity.unwrap_or(Severity::Info);

        match event_type {
            EventType::Authentication => {
                let details = self
                    .auth_attempt
                    .ok_or(AuditError::MissingField("auth_attempt details"))?;
                if details.source.is_empty() {
                    return Err(AuditError::MissingField("source"));
                }
                Ok(AuditEvent::Authentication {
                    event_id,
                    timestamp,
                    severity,
                    actor_id: self.actor_id,
                    node_id: self.node_id,
                    details,
                    metadata: self.metadata,
                })
            }
            EventType::AuthorizationFailure => {
                let context = self
                    .auth_context
                    .ok_or(AuditError::MissingField("authorization context"))?;
                if context.resource.is_empty() {
                    return Err(AuditError::MissingField("resource"));
                }
                if context.action.is_empty() {
                    return Err(AuditError::MissingField("action"));
                }
                Ok(AuditEvent::AuthorizationFailure {
                    event_id,
                    timestamp,
                    severity,
                    actor_id: self.actor_id,
                    node_id: self.node_id,
                    context,
                    metadata: self.metadata,
                })
            }
            EventType::EscrowStateChange => {
                let change = self
                    .escrow_change
                    .ok_or(AuditError::MissingField("escrow change details"))?;
                if change.escrow_id.is_empty() {
                    return Err(AuditError::MissingField("escrow_id"));
                }
                Ok(AuditEvent::EscrowStateChange {
                    event_id,
                    timestamp,
                    severity,
                    actor_id: self.actor_id,
                    node_id: self.node_id,
                    change,
                    metadata: self.metadata,
                })
            }
            EventType::RateLimit => {
                let violation = self
                    .rate_violation
                    .ok_or(AuditError::MissingField("rate limit details"))?;
                if violation.limit_name.is_empty() {
                    return Err(AuditError::MissingField("limit_name"));
                }
                Ok(AuditEvent::RateLimit {
                    event_id,
                    timestamp,
                    severity,
                    actor_id: self.actor_id,
                    node_id: self.node_id,
                    violation,
                    metadata: self.metadata,
                })
            }
            EventType::SignatureVerification => {
                let failure = self
                    .sig_failure
                    .ok_or(AuditError::MissingField("signature failure details"))?;
                if failure.signature_type.is_empty() {
                    return Err(AuditError::MissingField("signature_type"));
                }
                Ok(AuditEvent::SignatureVerification {
                    event_id,
                    timestamp,
                    severity,
                    actor_id: self.actor_id,
                    node_id: self.node_id,
                    failure,
                    metadata: self.metadata,
                })
            }
            EventType::UnusualPattern => {
                let pattern = self
                    .unusual_pattern
                    .ok_or(AuditError::MissingField("pattern details"))?;
                if pattern.pattern_type.is_empty() {
                    return Err(AuditError::MissingField("pattern_type"));
                }
                Ok(AuditEvent::UnusualPatternDetected {
                    event_id,
                    timestamp,
                    severity,
                    actor_id: self.actor_id,
                    node_id: self.node_id,
                    pattern,
                    metadata: self.metadata,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // Severity Tests
    // ===========================================

    #[test]
    fn severity_ordering() {
        assert!(Severity::Info < Severity::Low);
        assert!(Severity::Low < Severity::Medium);
        assert!(Severity::Medium < Severity::High);
        assert!(Severity::High < Severity::Critical);
    }

    #[test]
    fn severity_as_str() {
        assert_eq!(Severity::Info.as_str(), "info");
        assert_eq!(Severity::Low.as_str(), "low");
        assert_eq!(Severity::Medium.as_str(), "medium");
        assert_eq!(Severity::High.as_str(), "high");
        assert_eq!(Severity::Critical.as_str(), "critical");
    }

    #[test]
    fn severity_serialization() {
        let severity = Severity::High;
        let json = serde_json::to_string(&severity);
        assert!(json.is_ok());
        if let Ok(s) = json {
            assert_eq!(s, "\"high\"");
        }

        let parsed: std::result::Result<Severity, _> = serde_json::from_str("\"critical\"");
        assert!(parsed.is_ok());
        if let Ok(s) = parsed {
            assert_eq!(s, Severity::Critical);
        }
    }

    // ===========================================
    // AuditEvent Convenience Constructors
    // ===========================================

    #[test]
    fn authentication_success_event() {
        let event = AuditEvent::authentication_success("192.168.1.100");
        assert_eq!(event.event_type(), "authentication");
        assert_eq!(event.severity(), Severity::Info);

        if let AuditEvent::Authentication { details, .. } = event {
            assert!(details.success);
            assert_eq!(details.source, "192.168.1.100");
        } else {
            panic!("Expected Authentication event");
        }
    }

    #[test]
    fn authentication_failure_event() {
        let event = AuditEvent::authentication_failure("invalid_credentials", "10.0.0.1");
        assert_eq!(event.event_type(), "authentication");
        assert_eq!(event.severity(), Severity::Medium);

        if let AuditEvent::Authentication { details, .. } = event {
            assert!(!details.success);
            assert_eq!(details.reason, Some("invalid_credentials".to_string()));
            assert_eq!(details.source, "10.0.0.1");
        } else {
            panic!("Expected Authentication event");
        }
    }

    #[test]
    fn rate_limit_event() {
        let event = AuditEvent::rate_limit_exceeded("api_requests", 150, 100, 60, "user:123");
        assert_eq!(event.event_type(), "rate_limit");
        assert_eq!(event.severity(), Severity::Low);

        if let AuditEvent::RateLimit { violation, .. } = event {
            assert_eq!(violation.limit_name, "api_requests");
            assert_eq!(violation.current_count, 150);
            assert_eq!(violation.max_allowed, 100);
            assert_eq!(violation.window_seconds, 60);
            assert_eq!(violation.source, "user:123");
        } else {
            panic!("Expected RateLimit event");
        }
    }

    #[test]
    fn signature_verification_event() {
        let event = AuditEvent::signature_verification_failed("ed25519", "invalid_signature");
        assert_eq!(event.event_type(), "signature_verification");
        assert_eq!(event.severity(), Severity::Critical);

        if let AuditEvent::SignatureVerification { failure, .. } = event {
            assert_eq!(failure.signature_type, "ed25519");
            assert_eq!(failure.reason, "invalid_signature");
        } else {
            panic!("Expected SignatureVerification event");
        }
    }

    // ===========================================
    // AuditEvent Accessors
    // ===========================================

    #[test]
    fn event_accessors() {
        let event = AuditEvent::authentication_success("test");
        
        // Should have a valid UUID
        let _ = event.event_id();
        
        // Timestamp should be recent
        let ts = event.timestamp();
        let now = Utc::now();
        let diff = now.signed_duration_since(ts);
        assert!(diff.num_seconds() < 1);
        
        assert_eq!(event.severity(), Severity::Info);
        assert_eq!(event.event_type(), "authentication");
    }

    // ===========================================
    // AuditEvent Serialization
    // ===========================================

    #[test]
    fn event_serialization_roundtrip() {
        let event = AuditEvent::authentication_failure("test", "source");
        
        let json = event.to_json();
        assert!(json.is_ok());
        
        if let Ok(json_str) = json {
            let parsed: std::result::Result<AuditEvent, _> = serde_json::from_str(&json_str);
            assert!(parsed.is_ok());
            if let Ok(parsed_event) = parsed {
                assert_eq!(parsed_event.event_id(), event.event_id());
                assert_eq!(parsed_event.event_type(), event.event_type());
            }
        }
    }

    #[test]
    fn event_to_json_pretty() {
        let event = AuditEvent::authentication_success("test");
        let json = event.to_json_pretty();
        assert!(json.is_ok());
        if let Ok(s) = json {
            assert!(s.contains('\n')); // Pretty printed has newlines
        }
    }

    // ===========================================
    // Builder Tests
    // ===========================================

    #[test]
    fn builder_authentication_success() {
        let result = AuditEvent::builder()
            .authentication()
            .success(true)
            .source("192.168.1.1")
            .method("api_key")
            .identity("user@example.com")
            .build();

        assert!(result.is_ok());
        if let Ok(AuditEvent::Authentication { details, severity, .. }) = result {
            assert!(details.success);
            assert_eq!(details.source, "192.168.1.1");
            assert_eq!(details.method, Some("api_key".to_string()));
            assert_eq!(details.identity, Some("user@example.com".to_string()));
            assert_eq!(severity, Severity::Info);
        } else {
            panic!("Expected successful Authentication event");
        }
    }

    #[test]
    fn builder_authentication_failure() {
        let actor = Uuid::new_v4();
        let node = Uuid::new_v4();
        
        let result = AuditEvent::builder()
            .authentication()
            .success(false)
            .source("10.0.0.1")
            .reason("invalid_token")
            .actor_id(actor)
            .node_id(node)
            .metadata("attempt_count", serde_json::json!(3))
            .build();

        assert!(result.is_ok());
        if let Ok(AuditEvent::Authentication { 
            details, 
            severity, 
            actor_id, 
            node_id, 
            metadata, 
            .. 
        }) = result {
            assert!(!details.success);
            assert_eq!(details.reason, Some("invalid_token".to_string()));
            assert_eq!(severity, Severity::Medium);
            assert_eq!(actor_id, Some(actor));
            assert_eq!(node_id, Some(node));
            assert!(metadata.contains_key("attempt_count"));
        } else {
            panic!("Expected failed Authentication event");
        }
    }

    #[test]
    fn builder_authorization_failure() {
        let result = AuditEvent::builder()
            .authorization_failure()
            .actor_id(Uuid::new_v4())
            .resource("workload:abc123")
            .action("deploy")
            .reason("insufficient_permissions")
            .build();

        assert!(result.is_ok());
        if let Ok(AuditEvent::AuthorizationFailure { context, severity, .. }) = result {
            assert_eq!(context.resource, "workload:abc123");
            assert_eq!(context.action, "deploy");
            assert_eq!(context.reason, "insufficient_permissions");
            assert_eq!(severity, Severity::Medium);
        } else {
            panic!("Expected AuthorizationFailure event");
        }
    }

    #[test]
    fn builder_escrow_state_change() {
        let result = AuditEvent::builder()
            .escrow_state_change()
            .escrow_id("escrow:xyz789")
            .state_transition("pending", "funded")
            .amount("100.50")
            .build();

        assert!(result.is_ok());
        if let Ok(AuditEvent::EscrowStateChange { change, severity, .. }) = result {
            assert_eq!(change.escrow_id, "escrow:xyz789");
            assert_eq!(change.previous_state, "pending");
            assert_eq!(change.new_state, "funded");
            assert_eq!(change.amount, Some("100.50".to_string()));
            assert_eq!(severity, Severity::Info);
        } else {
            panic!("Expected EscrowStateChange event");
        }
    }

    #[test]
    fn builder_rate_limit() {
        let result = AuditEvent::builder()
            .rate_limit()
            .limit("requests_per_minute", 150, 100, 60)
            .limit_source("ip:192.168.1.1")
            .severity(Severity::Medium)
            .build();

        assert!(result.is_ok());
        if let Ok(AuditEvent::RateLimit { violation, severity, .. }) = result {
            assert_eq!(violation.limit_name, "requests_per_minute");
            assert_eq!(violation.current_count, 150);
            assert_eq!(violation.max_allowed, 100);
            assert_eq!(violation.window_seconds, 60);
            assert_eq!(violation.source, "ip:192.168.1.1");
            assert_eq!(severity, Severity::Medium);
        } else {
            panic!("Expected RateLimit event");
        }
    }

    #[test]
    fn builder_signature_verification() {
        let result = AuditEvent::builder()
            .signature_verification()
            .signature_type("ed25519")
            .sig_reason("signature_mismatch")
            .public_key("abc123def456")
            .build();

        assert!(result.is_ok());
        if let Ok(AuditEvent::SignatureVerification { failure, severity, .. }) = result {
            assert_eq!(failure.signature_type, "ed25519");
            assert_eq!(failure.reason, "signature_mismatch");
            assert_eq!(failure.public_key, Some("abc123def456".to_string()));
            assert_eq!(severity, Severity::Critical);
        } else {
            panic!("Expected SignatureVerification event");
        }
    }

    #[test]
    fn builder_unusual_pattern() {
        let result = AuditEvent::builder()
            .unusual_pattern()
            .pattern_type("brute_force")
            .description("Multiple failed auth attempts from same IP")
            .confidence(85)
            .build();

        assert!(result.is_ok());
        if let Ok(AuditEvent::UnusualPatternDetected { pattern, severity, .. }) = result {
            assert_eq!(pattern.pattern_type, "brute_force");
            assert_eq!(pattern.description, "Multiple failed auth attempts from same IP");
            assert_eq!(pattern.confidence, 85);
            assert_eq!(severity, Severity::High);
        } else {
            panic!("Expected UnusualPatternDetected event");
        }
    }

    #[test]
    fn builder_confidence_capped_at_100() {
        let result = AuditEvent::builder()
            .unusual_pattern()
            .pattern_type("test")
            .confidence(200) // Should be capped at 100
            .build();

        assert!(result.is_ok());
        if let Ok(AuditEvent::UnusualPatternDetected { pattern, .. }) = result {
            assert_eq!(pattern.confidence, 100);
        } else {
            panic!("Expected UnusualPatternDetected event");
        }
    }

    // ===========================================
    // Builder Error Cases
    // ===========================================

    #[test]
    fn builder_missing_event_type() {
        let result = AuditEvent::builder()
            .actor_id(Uuid::new_v4())
            .build();

        assert!(result.is_err());
        if let Err(AuditError::MissingField(field)) = result {
            assert_eq!(field, "event_type");
        } else {
            panic!("Expected MissingField error");
        }
    }

    #[test]
    fn builder_authentication_missing_source() {
        let result = AuditEvent::builder()
            .authentication()
            .success(true)
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn builder_authorization_missing_resource() {
        let result = AuditEvent::builder()
            .authorization_failure()
            .action("deploy")
            .reason("test")
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn builder_authorization_missing_action() {
        let result = AuditEvent::builder()
            .authorization_failure()
            .resource("test")
            .reason("test")
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn builder_escrow_missing_id() {
        let result = AuditEvent::builder()
            .escrow_state_change()
            .state_transition("a", "b")
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn builder_rate_limit_missing_name() {
        let result = AuditEvent::builder()
            .rate_limit()
            .limit_source("test")
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn builder_signature_missing_type() {
        let result = AuditEvent::builder()
            .signature_verification()
            .sig_reason("test")
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn builder_pattern_missing_type() {
        let result = AuditEvent::builder()
            .unusual_pattern()
            .description("test")
            .build();

        assert!(result.is_err());
    }

    // ===========================================
    // Detail Types Tests
    // ===========================================

    #[test]
    fn auth_attempt_serialization() {
        let attempt = AuthAttempt {
            success: false,
            reason: Some("test".to_string()),
            source: "127.0.0.1".to_string(),
            method: Some("password".to_string()),
            identity: Some("admin".to_string()),
        };

        let json = serde_json::to_string(&attempt);
        assert!(json.is_ok());

        if let Ok(s) = json {
            let parsed: std::result::Result<AuthAttempt, _> = serde_json::from_str(&s);
            assert!(parsed.is_ok());
        }
    }

    #[test]
    fn authorization_context_serialization() {
        let ctx = AuthorizationContext {
            resource: "workload:123".to_string(),
            action: "delete".to_string(),
            reason: "not_owner".to_string(),
            required_permissions: vec!["admin".to_string()],
            actual_permissions: vec!["viewer".to_string()],
        };

        let json = serde_json::to_string(&ctx);
        assert!(json.is_ok());
    }

    #[test]
    fn escrow_change_serialization() {
        let change = EscrowChange {
            escrow_id: "esc:123".to_string(),
            previous_state: "pending".to_string(),
            new_state: "active".to_string(),
            amount: Some("50.0".to_string()),
            parties: vec!["buyer".to_string(), "seller".to_string()],
        };

        let json = serde_json::to_string(&change);
        assert!(json.is_ok());
    }

    #[test]
    fn rate_limit_violation_serialization() {
        let violation = RateLimitViolation {
            limit_name: "api".to_string(),
            current_count: 100,
            max_allowed: 50,
            window_seconds: 60,
            source: "user:1".to_string(),
        };

        let json = serde_json::to_string(&violation);
        assert!(json.is_ok());
    }

    #[test]
    fn signature_failure_serialization() {
        let failure = SignatureFailure {
            signature_type: "ed25519".to_string(),
            reason: "invalid".to_string(),
            public_key: Some("abc".to_string()),
            message_hash: Some("def".to_string()),
        };

        let json = serde_json::to_string(&failure);
        assert!(json.is_ok());
    }

    #[test]
    fn unusual_pattern_serialization() {
        let pattern = UnusualPattern {
            pattern_type: "brute_force".to_string(),
            description: "test".to_string(),
            confidence: 75,
            related_events: vec![Uuid::new_v4()],
        };

        let json = serde_json::to_string(&pattern);
        assert!(json.is_ok());
    }

    // ===========================================
    // All Event Types Serialization
    // ===========================================

    #[test]
    fn all_event_types_serialize() {
        let events = vec![
            AuditEvent::authentication_success("test"),
            AuditEvent::authentication_failure("reason", "source"),
            AuditEvent::rate_limit_exceeded("limit", 10, 5, 60, "source"),
            AuditEvent::signature_verification_failed("ed25519", "reason"),
        ];

        for event in events {
            let json = event.to_json();
            assert!(json.is_ok(), "Failed to serialize {:?}", event.event_type());
        }
    }
}
