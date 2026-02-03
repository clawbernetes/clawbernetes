//! Core types for the secrets management system.
//!
//! This module defines the fundamental types used throughout the secrets system:
//! - [`SecretId`]: A validated identifier for secrets
//! - [`SecretValue`]: Encrypted secret data that zeroizes on drop
//! - [`SecretMetadata`]: Metadata about a secret (timestamps, version, rotation)
//! - [`AccessPolicy`]: Who can access a secret and when
//! - [`AuditEntry`]: Record of secret access for audit trail

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{Error, Result};

/// A validated identifier for a secret.
///
/// Secret IDs must:
/// - Be between 1 and 253 characters
/// - Contain only lowercase alphanumeric characters, hyphens, underscores, and periods
/// - Start with an alphanumeric character
/// - Not end with a hyphen or period
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SecretId(String);

impl SecretId {
    /// Maximum length of a secret identifier.
    pub const MAX_LENGTH: usize = 253;

    /// Minimum length of a secret identifier.
    pub const MIN_LENGTH: usize = 1;

    /// Creates a new `SecretId` after validating the input.
    ///
    /// # Errors
    ///
    /// Returns an error if the identifier is invalid.
    pub fn new(id: impl Into<String>) -> Result<Self> {
        let id = id.into();
        Self::validate(&id)?;
        Ok(Self(id))
    }

    /// Returns the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validates a secret identifier string.
    fn validate(id: &str) -> Result<()> {
        if id.len() < Self::MIN_LENGTH {
            return Err(Error::InvalidSecretId {
                reason: "identifier cannot be empty".to_string(),
            });
        }

        if id.len() > Self::MAX_LENGTH {
            return Err(Error::InvalidSecretId {
                reason: format!(
                    "identifier exceeds maximum length of {} characters",
                    Self::MAX_LENGTH
                ),
            });
        }

        let first_char = id.chars().next().ok_or_else(|| Error::InvalidSecretId {
            reason: "identifier cannot be empty".to_string(),
        })?;

        if !first_char.is_ascii_alphanumeric() {
            return Err(Error::InvalidSecretId {
                reason: "identifier must start with an alphanumeric character".to_string(),
            });
        }

        let last_char = id.chars().last().ok_or_else(|| Error::InvalidSecretId {
            reason: "identifier cannot be empty".to_string(),
        })?;

        if last_char == '-' || last_char == '.' {
            return Err(Error::InvalidSecretId {
                reason: "identifier cannot end with a hyphen or period".to_string(),
            });
        }

        for c in id.chars() {
            if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' && c != '_' && c != '.' {
                return Err(Error::InvalidSecretId {
                    reason: format!(
                        "identifier contains invalid character '{}'; only lowercase alphanumeric, hyphens, underscores, and periods are allowed",
                        c
                    ),
                });
            }
        }

        Ok(())
    }
}

impl fmt::Display for SecretId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for SecretId {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<SecretId> for String {
    fn from(id: SecretId) -> Self {
        id.0
    }
}

impl AsRef<str> for SecretId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Encrypted secret value that securely zeroizes memory on drop.
///
/// This type holds the encrypted bytes of a secret and ensures that
/// the memory is securely cleared when the value is dropped.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretValue {
    /// The encrypted bytes of the secret.
    data: Vec<u8>,
}

impl SecretValue {
    /// Creates a new `SecretValue` from encrypted bytes.
    #[must_use]
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Returns the encrypted data as a byte slice.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Returns the length of the encrypted data.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns true if the encrypted data is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Consumes the `SecretValue` and returns the encrypted bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        // Note: We use ManuallyDrop to prevent double-zeroize
        // The caller takes ownership and responsibility for the data
        let mut this = std::mem::ManuallyDrop::new(self);
        std::mem::take(&mut this.data)
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never expose the actual bytes in debug output
        f.debug_struct("SecretValue")
            .field("len", &self.data.len())
            .field("data", &"[REDACTED]")
            .finish()
    }
}

impl PartialEq for SecretValue {
    fn eq(&self, other: &Self) -> bool {
        // Constant-time comparison to prevent timing attacks
        use subtle::ConstantTimeEq;
        self.data.ct_eq(&other.data).into()
    }
}

impl Eq for SecretValue {}

/// Rotation policy for secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotationPolicy {
    /// How often the secret should be rotated, in seconds.
    pub interval_seconds: u64,
    /// Whether automatic rotation is enabled.
    pub auto_rotate: bool,
    /// Maximum number of old versions to keep.
    pub max_versions: u32,
}

impl Default for RotationPolicy {
    fn default() -> Self {
        Self {
            interval_seconds: 86400 * 30, // 30 days
            auto_rotate: false,
            max_versions: 3,
        }
    }
}

/// Metadata about a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretMetadata {
    /// When the secret was created.
    pub created_at: DateTime<Utc>,
    /// When the secret was last updated.
    pub updated_at: DateTime<Utc>,
    /// The current version of the secret.
    pub version: u64,
    /// The rotation policy for this secret.
    pub rotation_policy: RotationPolicy,
}

impl SecretMetadata {
    /// Creates new metadata with the current timestamp.
    #[must_use]
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            created_at: now,
            updated_at: now,
            version: 1,
            rotation_policy: RotationPolicy::default(),
        }
    }

    /// Creates new metadata with a custom rotation policy.
    #[must_use]
    pub fn with_rotation_policy(rotation_policy: RotationPolicy) -> Self {
        let mut meta = Self::new();
        meta.rotation_policy = rotation_policy;
        meta
    }

    /// Increments the version and updates the timestamp.
    pub fn bump_version(&mut self) {
        self.version += 1;
        self.updated_at = Utc::now();
    }
}

impl Default for SecretMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// A workload identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkloadId(String);

impl WorkloadId {
    /// Creates a new workload identifier.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WorkloadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A node identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(String);

impl NodeId {
    /// Creates a new node identifier.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Access policy for a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessPolicy {
    /// Workloads allowed to access this secret.
    pub allowed_workloads: Vec<WorkloadId>,
    /// Nodes allowed to access this secret.
    pub allowed_nodes: Vec<NodeId>,
    /// When this policy expires (if ever).
    pub expires_at: Option<DateTime<Utc>>,
}

impl AccessPolicy {
    /// Creates a new empty access policy.
    #[must_use]
    pub fn new() -> Self {
        Self {
            allowed_workloads: Vec::new(),
            allowed_nodes: Vec::new(),
            expires_at: None,
        }
    }

    /// Creates a policy that allows the specified workloads.
    #[must_use]
    pub fn allow_workloads(workloads: Vec<WorkloadId>) -> Self {
        Self {
            allowed_workloads: workloads,
            allowed_nodes: Vec::new(),
            expires_at: None,
        }
    }

    /// Creates a policy that allows the specified nodes.
    #[must_use]
    pub fn allow_nodes(nodes: Vec<NodeId>) -> Self {
        Self {
            allowed_workloads: Vec::new(),
            allowed_nodes: nodes,
            expires_at: None,
        }
    }

    /// Sets the expiration time for this policy.
    #[must_use]
    pub fn with_expiry(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Checks if the policy has expired.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| Utc::now() > exp)
    }

    /// Checks if a workload is allowed by this policy.
    #[must_use]
    pub fn allows_workload(&self, workload: &WorkloadId) -> bool {
        self.allowed_workloads.contains(workload)
    }

    /// Checks if a node is allowed by this policy.
    #[must_use]
    pub fn allows_node(&self, node: &NodeId) -> bool {
        self.allowed_nodes.contains(node)
    }
}

impl Default for AccessPolicy {
    fn default() -> Self {
        Self::new()
    }
}

/// Actions that can be performed on secrets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditAction {
    /// Secret was created.
    Created,
    /// Secret was read.
    Read,
    /// Secret was updated.
    Updated,
    /// Secret was deleted.
    Deleted,
    /// Secret was rotated.
    Rotated,
    /// Access was denied.
    AccessDenied,
}

impl fmt::Display for AuditAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Read => write!(f, "read"),
            Self::Updated => write!(f, "updated"),
            Self::Deleted => write!(f, "deleted"),
            Self::Rotated => write!(f, "rotated"),
            Self::AccessDenied => write!(f, "access_denied"),
        }
    }
}

/// Who is accessing a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Accessor {
    /// A workload is accessing the secret.
    Workload(WorkloadId),
    /// A node is accessing the secret.
    Node(NodeId),
    /// An administrator is accessing the secret.
    Admin(String),
    /// The system itself (for internal operations).
    System,
}

impl fmt::Display for Accessor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Workload(id) => write!(f, "workload:{id}"),
            Self::Node(id) => write!(f, "node:{id}"),
            Self::Admin(id) => write!(f, "admin:{id}"),
            Self::System => write!(f, "system"),
        }
    }
}

/// An entry in the audit log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// When the action occurred.
    pub timestamp: DateTime<Utc>,
    /// The secret that was accessed.
    pub secret_id: SecretId,
    /// Who accessed the secret.
    pub accessor: Accessor,
    /// What action was performed.
    pub action: AuditAction,
    /// Human-readable reason for the action.
    pub reason: String,
}

impl AuditEntry {
    /// Creates a new audit entry with the current timestamp.
    #[must_use]
    pub fn new(
        secret_id: SecretId,
        accessor: Accessor,
        action: AuditAction,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            secret_id,
            accessor,
            action,
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    // ===================
    // SecretId Tests
    // ===================

    #[test]
    fn secret_id_valid_simple() {
        let id = SecretId::new("my-secret").expect("should be valid");
        assert_eq!(id.as_str(), "my-secret");
    }

    #[test]
    fn secret_id_valid_with_dots() {
        let id = SecretId::new("database.password").expect("should be valid");
        assert_eq!(id.as_str(), "database.password");
    }

    #[test]
    fn secret_id_valid_with_underscores() {
        let id = SecretId::new("api_key_production").expect("should be valid");
        assert_eq!(id.as_str(), "api_key_production");
    }

    #[test]
    fn secret_id_valid_single_char() {
        let id = SecretId::new("a").expect("should be valid");
        assert_eq!(id.as_str(), "a");
    }

    #[test]
    fn secret_id_valid_numbers() {
        let id = SecretId::new("secret123").expect("should be valid");
        assert_eq!(id.as_str(), "secret123");
    }

    #[test_case("" ; "empty string")]
    #[test_case("-secret" ; "starts with hyphen")]
    #[test_case(".secret" ; "starts with period")]
    #[test_case("_secret" ; "starts with underscore")]
    #[test_case("secret-" ; "ends with hyphen")]
    #[test_case("secret." ; "ends with period")]
    #[test_case("Secret" ; "contains uppercase")]
    #[test_case("my secret" ; "contains space")]
    #[test_case("my@secret" ; "contains at sign")]
    #[test_case("my/secret" ; "contains slash")]
    fn secret_id_invalid(input: &str) {
        let result = SecretId::new(input);
        assert!(result.is_err(), "expected '{}' to be invalid", input);
    }

    #[test]
    fn secret_id_max_length() {
        let long_id = "a".repeat(SecretId::MAX_LENGTH);
        let id = SecretId::new(&long_id).expect("max length should be valid");
        assert_eq!(id.as_str().len(), SecretId::MAX_LENGTH);
    }

    #[test]
    fn secret_id_exceeds_max_length() {
        let too_long = "a".repeat(SecretId::MAX_LENGTH + 1);
        let result = SecretId::new(&too_long);
        assert!(result.is_err());
    }

    #[test]
    fn secret_id_display() {
        let id = SecretId::new("my-secret").expect("should be valid");
        assert_eq!(format!("{id}"), "my-secret");
    }

    #[test]
    fn secret_id_serde_roundtrip() {
        let original = SecretId::new("my-secret").expect("should be valid");
        let json = serde_json::to_string(&original).expect("serialize");
        let restored: SecretId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, restored);
    }

    #[test]
    fn secret_id_serde_rejects_invalid() {
        let json = r#""Invalid-ID""#;
        let result: std::result::Result<SecretId, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    // ===================
    // SecretValue Tests
    // ===================

    #[test]
    fn secret_value_creation() {
        let data = vec![1, 2, 3, 4, 5];
        let value = SecretValue::new(data.clone());
        assert_eq!(value.as_bytes(), &data);
    }

    #[test]
    fn secret_value_len() {
        let value = SecretValue::new(vec![1, 2, 3]);
        assert_eq!(value.len(), 3);
        assert!(!value.is_empty());
    }

    #[test]
    fn secret_value_empty() {
        let value = SecretValue::new(vec![]);
        assert!(value.is_empty());
        assert_eq!(value.len(), 0);
    }

    #[test]
    fn secret_value_debug_redacts_data() {
        let value = SecretValue::new(vec![1, 2, 3, 4, 5]);
        let debug_str = format!("{value:?}");
        assert!(debug_str.contains("[REDACTED]"));
        assert!(!debug_str.contains("1, 2, 3"));
    }

    #[test]
    fn secret_value_into_bytes() {
        let data = vec![1, 2, 3, 4, 5];
        let value = SecretValue::new(data.clone());
        let extracted = value.into_bytes();
        assert_eq!(extracted, data);
    }

    #[test]
    fn secret_value_equality() {
        let v1 = SecretValue::new(vec![1, 2, 3]);
        let v2 = SecretValue::new(vec![1, 2, 3]);
        let v3 = SecretValue::new(vec![1, 2, 4]);

        assert_eq!(v1, v2);
        assert_ne!(v1, v3);
    }

    // ===================
    // AccessPolicy Tests
    // ===================

    #[test]
    fn access_policy_default_empty() {
        let policy = AccessPolicy::new();
        assert!(policy.allowed_workloads.is_empty());
        assert!(policy.allowed_nodes.is_empty());
        assert!(policy.expires_at.is_none());
    }

    #[test]
    fn access_policy_allows_workload() {
        let workload = WorkloadId::new("my-workload");
        let policy = AccessPolicy::allow_workloads(vec![workload.clone()]);

        assert!(policy.allows_workload(&workload));
        assert!(!policy.allows_workload(&WorkloadId::new("other")));
    }

    #[test]
    fn access_policy_allows_node() {
        let node = NodeId::new("my-node");
        let policy = AccessPolicy::allow_nodes(vec![node.clone()]);

        assert!(policy.allows_node(&node));
        assert!(!policy.allows_node(&NodeId::new("other")));
    }

    #[test]
    fn access_policy_expiry() {
        use chrono::Duration;

        let future = Utc::now() + Duration::hours(1);
        let policy = AccessPolicy::new().with_expiry(future);
        assert!(!policy.is_expired());

        let past = Utc::now() - Duration::hours(1);
        let expired_policy = AccessPolicy::new().with_expiry(past);
        assert!(expired_policy.is_expired());
    }

    #[test]
    fn access_policy_serde_roundtrip() {
        let policy = AccessPolicy {
            allowed_workloads: vec![WorkloadId::new("w1"), WorkloadId::new("w2")],
            allowed_nodes: vec![NodeId::new("n1")],
            expires_at: Some(Utc::now()),
        };

        let json = serde_json::to_string(&policy).expect("serialize");
        let restored: AccessPolicy = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(policy.allowed_workloads, restored.allowed_workloads);
        assert_eq!(policy.allowed_nodes, restored.allowed_nodes);
    }

    // ===================
    // SecretMetadata Tests
    // ===================

    #[test]
    fn secret_metadata_new() {
        let meta = SecretMetadata::new();
        assert_eq!(meta.version, 1);
        assert!(!meta.rotation_policy.auto_rotate);
    }

    #[test]
    fn secret_metadata_bump_version() {
        let mut meta = SecretMetadata::new();
        let original_updated = meta.updated_at;

        // Small delay to ensure time difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        meta.bump_version();
        assert_eq!(meta.version, 2);
        assert!(meta.updated_at > original_updated);
    }

    #[test]
    fn secret_metadata_with_rotation_policy() {
        let policy = RotationPolicy {
            interval_seconds: 3600,
            auto_rotate: true,
            max_versions: 5,
        };

        let meta = SecretMetadata::with_rotation_policy(policy.clone());
        assert_eq!(meta.rotation_policy, policy);
    }

    // ===================
    // AuditEntry Tests
    // ===================

    #[test]
    fn audit_entry_creation() {
        let secret_id = SecretId::new("my-secret").expect("valid id");
        let accessor = Accessor::Workload(WorkloadId::new("workload-1"));

        let entry = AuditEntry::new(
            secret_id.clone(),
            accessor.clone(),
            AuditAction::Read,
            "routine access",
        );

        assert_eq!(entry.secret_id, secret_id);
        assert_eq!(entry.accessor, accessor);
        assert_eq!(entry.action, AuditAction::Read);
        assert_eq!(entry.reason, "routine access");
    }

    #[test]
    fn accessor_display() {
        assert_eq!(
            format!("{}", Accessor::Workload(WorkloadId::new("w1"))),
            "workload:w1"
        );
        assert_eq!(
            format!("{}", Accessor::Node(NodeId::new("n1"))),
            "node:n1"
        );
        assert_eq!(
            format!("{}", Accessor::Admin("alice".to_string())),
            "admin:alice"
        );
        assert_eq!(format!("{}", Accessor::System), "system");
    }

    #[test]
    fn audit_action_display() {
        assert_eq!(format!("{}", AuditAction::Created), "created");
        assert_eq!(format!("{}", AuditAction::Read), "read");
        assert_eq!(format!("{}", AuditAction::Updated), "updated");
        assert_eq!(format!("{}", AuditAction::Deleted), "deleted");
        assert_eq!(format!("{}", AuditAction::Rotated), "rotated");
        assert_eq!(format!("{}", AuditAction::AccessDenied), "access_denied");
    }

    #[test]
    fn audit_entry_serde_roundtrip() {
        let entry = AuditEntry::new(
            SecretId::new("test-secret").expect("valid"),
            Accessor::System,
            AuditAction::Created,
            "initial creation",
        );

        let json = serde_json::to_string(&entry).expect("serialize");
        let restored: AuditEntry = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(entry.secret_id, restored.secret_id);
        assert_eq!(entry.action, restored.action);
        assert_eq!(entry.reason, restored.reason);
    }
}
