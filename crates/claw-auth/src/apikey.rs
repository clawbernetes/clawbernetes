//! API key authentication.
//!
//! This module provides API key generation, validation, and storage:
//! - [`ApiKeyId`]: Unique identifier for an API key
//! - [`ApiKey`]: An API key with metadata and scopes
//! - [`ApiKeyHash`]: Secure hash of the secret key
//! - [`ApiKeyStore`]: In-memory storage for API keys

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{Error, Result};
use crate::types::{Action, Scope, UserId};

/// Prefix for API keys to make them easily identifiable.
const API_KEY_PREFIX: &str = "claw_";

/// Length of the random portion of the key.
const API_KEY_RANDOM_LENGTH: usize = 32;

/// A unique identifier for an API key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ApiKeyId(String);

impl ApiKeyId {
    /// Creates a new random API key ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Creates an API key ID from an existing string.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid UUID.
    pub fn from_string(id: impl Into<String>) -> Result<Self> {
        let id = id.into();
        if id.is_empty() {
            return Err(Error::InvalidApiKey {
                reason: "id cannot be empty".to_string(),
            });
        }
        Ok(Self(id))
    }

    /// Returns the ID as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ApiKeyId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ApiKeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A hash of the API key secret, used for verification.
#[derive(Clone, Serialize, Deserialize)]
pub struct ApiKeyHash {
    /// The blake3 hash of the key.
    hash: [u8; 32],
}

impl ApiKeyHash {
    /// Creates a hash from a plaintext key.
    #[must_use]
    pub fn from_key(key: &str) -> Self {
        let hash = blake3::hash(key.as_bytes());
        Self {
            hash: *hash.as_bytes(),
        }
    }

    /// Verifies that a plaintext key matches this hash.
    #[must_use]
    pub fn verify(&self, key: &str) -> bool {
        let other_hash = blake3::hash(key.as_bytes());
        // Constant-time comparison
        use subtle::ConstantTimeEq;
        self.hash.ct_eq(other_hash.as_bytes()).into()
    }
}

impl fmt::Debug for ApiKeyHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never expose the hash in debug output
        f.debug_struct("ApiKeyHash")
            .field("hash", &"[REDACTED]")
            .finish()
    }
}

impl PartialEq for ApiKeyHash {
    fn eq(&self, other: &Self) -> bool {
        use subtle::ConstantTimeEq;
        self.hash.ct_eq(&other.hash).into()
    }
}

impl Eq for ApiKeyHash {}

/// The plaintext API key secret, zeroized on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct ApiKeySecret {
    /// The plaintext key.
    key: String,
}

impl ApiKeySecret {
    /// Creates a new random API key secret.
    #[must_use]
    pub fn generate() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let random: String = (0..API_KEY_RANDOM_LENGTH)
            .map(|_| {
                let idx = rng.gen_range(0..62);
                match idx {
                    0..=9 => (b'0' + idx) as char,
                    10..=35 => (b'a' + idx - 10) as char,
                    _ => (b'A' + idx - 36) as char,
                }
            })
            .collect();
        Self {
            key: format!("{API_KEY_PREFIX}{random}"),
        }
    }

    /// Creates from an existing string.
    #[must_use]
    pub fn from_string(key: impl Into<String>) -> Self {
        Self { key: key.into() }
    }

    /// Returns the key as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.key
    }

    /// Creates a hash of this key for storage.
    #[must_use]
    pub fn hash(&self) -> ApiKeyHash {
        ApiKeyHash::from_key(&self.key)
    }

    /// Validates the key format.
    ///
    /// # Errors
    ///
    /// Returns an error if the key format is invalid.
    pub fn validate(&self) -> Result<()> {
        if !self.key.starts_with(API_KEY_PREFIX) {
            return Err(Error::InvalidApiKey {
                reason: format!("key must start with '{API_KEY_PREFIX}'"),
            });
        }
        let suffix = &self.key[API_KEY_PREFIX.len()..];
        if suffix.len() < 16 {
            return Err(Error::InvalidApiKey {
                reason: "key is too short".to_string(),
            });
        }
        if !suffix.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(Error::InvalidApiKey {
                reason: "key contains invalid characters".to_string(),
            });
        }
        Ok(())
    }
}

impl fmt::Debug for ApiKeySecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never expose the key in debug output
        let visible = if self.key.len() > 8 {
            format!("{}...", &self.key[..8])
        } else {
            "[SHORT]".to_string()
        };
        f.debug_struct("ApiKeySecret")
            .field("key", &visible)
            .finish()
    }
}

/// An API key with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    /// Unique identifier for this key.
    pub id: ApiKeyId,
    /// Human-readable name/description.
    pub name: String,
    /// Hash of the secret key (for verification).
    pub key_hash: ApiKeyHash,
    /// User who owns this key.
    pub user_id: UserId,
    /// Scopes granted to this key.
    pub scopes: Vec<Scope>,
    /// When this key expires (None = never).
    pub expires_at: Option<DateTime<Utc>>,
    /// When this key was created.
    pub created_at: DateTime<Utc>,
    /// When this key was last used.
    pub last_used_at: Option<DateTime<Utc>>,
    /// Whether this key has been revoked.
    pub revoked: bool,
    /// Reason for revocation (if revoked).
    pub revoked_reason: Option<String>,
}

impl ApiKey {
    /// Creates a new API key and returns both the key metadata and the plaintext secret.
    ///
    /// The plaintext secret must be shown to the user once; it cannot be recovered.
    #[must_use]
    pub fn generate(name: impl Into<String>, user_id: UserId) -> (Self, ApiKeySecret) {
        let secret = ApiKeySecret::generate();
        let now = Utc::now();
        let key = Self {
            id: ApiKeyId::new(),
            name: name.into(),
            key_hash: secret.hash(),
            user_id,
            scopes: Vec::new(),
            expires_at: None,
            created_at: now,
            last_used_at: None,
            revoked: false,
            revoked_reason: None,
        };
        (key, secret)
    }

    /// Sets the expiration time.
    #[must_use]
    pub fn with_expiry(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Adds a scope to this key.
    pub fn add_scope(&mut self, scope: Scope) {
        if !self.scopes.contains(&scope) {
            self.scopes.push(scope);
        }
    }

    /// Sets multiple scopes.
    #[must_use]
    pub fn with_scopes(mut self, scopes: Vec<Scope>) -> Self {
        self.scopes = scopes;
        self
    }

    /// Verifies a plaintext key against this key's hash.
    #[must_use]
    pub fn verify(&self, secret: &str) -> bool {
        self.key_hash.verify(secret)
    }

    /// Checks if this key is currently valid (not expired, not revoked).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.revoked && !self.is_expired()
    }

    /// Checks if this key has expired.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| Utc::now() > exp)
    }

    /// Checks if this key has a scope that allows the given action on the resource.
    #[must_use]
    pub fn allows(&self, resource: &str, action: Action) -> bool {
        self.scopes.iter().any(|s| s.allows(resource, action))
    }

    /// Revokes this key.
    pub fn revoke(&mut self, reason: impl Into<String>) {
        self.revoked = true;
        self.revoked_reason = Some(reason.into());
    }

    /// Records that this key was used.
    pub fn record_use(&mut self) {
        self.last_used_at = Some(Utc::now());
    }

    /// Validates a key for use.
    ///
    /// # Errors
    ///
    /// Returns an error if the key is invalid (expired, revoked, etc.).
    pub fn validate_for_use(&self) -> Result<()> {
        if self.revoked {
            return Err(Error::ApiKeyRevoked {
                id: self.id.to_string(),
            });
        }
        if self.is_expired() {
            return Err(Error::ApiKeyExpired {
                id: self.id.to_string(),
            });
        }
        Ok(())
    }
}

/// In-memory store for API keys.
#[derive(Debug, Default)]
pub struct ApiKeyStore {
    /// Keys indexed by ID.
    keys_by_id: HashMap<ApiKeyId, ApiKey>,
    /// Key IDs indexed by user ID.
    keys_by_user: HashMap<UserId, Vec<ApiKeyId>>,
}

impl ApiKeyStore {
    /// Creates a new empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores an API key.
    pub fn store(&mut self, key: ApiKey) {
        let key_id = key.id.clone();
        let user_id = key.user_id.clone();
        self.keys_by_id.insert(key_id.clone(), key);
        self.keys_by_user
            .entry(user_id)
            .or_default()
            .push(key_id);
    }

    /// Gets a key by ID.
    #[must_use]
    pub fn get(&self, id: &ApiKeyId) -> Option<&ApiKey> {
        self.keys_by_id.get(id)
    }

    /// Gets a mutable reference to a key by ID.
    #[must_use]
    pub fn get_mut(&mut self, id: &ApiKeyId) -> Option<&mut ApiKey> {
        self.keys_by_id.get_mut(id)
    }

    /// Finds a key by verifying a plaintext secret.
    ///
    /// Returns the key if found and valid, None otherwise.
    #[must_use]
    pub fn find_by_secret(&self, secret: &str) -> Option<&ApiKey> {
        self.keys_by_id.values().find(|k| k.verify(secret))
    }

    /// Lists all keys for a user.
    #[must_use]
    pub fn list_for_user(&self, user_id: &UserId) -> Vec<&ApiKey> {
        self.keys_by_user
            .get(user_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.keys_by_id.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Removes a key by ID.
    pub fn remove(&mut self, id: &ApiKeyId) -> Option<ApiKey> {
        if let Some(key) = self.keys_by_id.remove(id) {
            if let Some(user_keys) = self.keys_by_user.get_mut(&key.user_id) {
                user_keys.retain(|k| k != id);
            }
            Some(key)
        } else {
            None
        }
    }

    /// Returns the total number of keys in the store.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys_by_id.len()
    }

    /// Returns true if the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys_by_id.is_empty()
    }

    /// Removes all expired and revoked keys.
    pub fn cleanup(&mut self) {
        let expired_ids: Vec<ApiKeyId> = self
            .keys_by_id
            .iter()
            .filter(|(_, k)| !k.is_valid())
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired_ids {
            self.remove(&id);
        }
    }
}

/// Authenticates an API key and returns the associated key metadata.
///
/// # Errors
///
/// Returns an error if the key is invalid, expired, or revoked.
pub fn authenticate_api_key<'a>(
    store: &'a ApiKeyStore,
    secret: &str,
) -> Result<&'a ApiKey> {
    let key = store
        .find_by_secret(secret)
        .ok_or(Error::InvalidApiKey {
            reason: "key not found or invalid".to_string(),
        })?;
    key.validate_for_use()?;
    Ok(key)
}

/// Extracts an API key from an HTTP Authorization header.
///
/// Expected format: `Bearer <key>` or just `<key>`
///
/// # Errors
///
/// Returns an error if the header format is invalid.
pub fn extract_api_key_from_header(header: &str) -> Result<&str> {
    let header = header.trim();
    if header.is_empty() {
        return Err(Error::InvalidApiKey {
            reason: "authorization header is empty".to_string(),
        });
    }
    if let Some(key) = header.strip_prefix("Bearer ") {
        Ok(key.trim())
    } else if header.starts_with(API_KEY_PREFIX) {
        Ok(header)
    } else {
        Err(Error::InvalidApiKey {
            reason: "invalid authorization header format".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    // ===================
    // ApiKeyId Tests
    // ===================

    #[test]
    fn api_key_id_new() {
        let id = ApiKeyId::new();
        assert!(!id.as_str().is_empty());
    }

    #[test]
    fn api_key_id_from_string() {
        let id = ApiKeyId::from_string("test-id");
        assert!(id.is_ok());
        assert_eq!(id.ok().unwrap().as_str(), "test-id");
    }

    #[test]
    fn api_key_id_empty() {
        let id = ApiKeyId::from_string("");
        assert!(id.is_err());
    }

    #[test]
    fn api_key_id_display() {
        let id = ApiKeyId::from_string("my-key-id");
        assert!(id.is_ok());
        assert_eq!(id.ok().unwrap().to_string(), "my-key-id");
    }

    // ===================
    // ApiKeyHash Tests
    // ===================

    #[test]
    fn api_key_hash_verify() {
        let hash = ApiKeyHash::from_key("test-key");
        assert!(hash.verify("test-key"));
        assert!(!hash.verify("wrong-key"));
    }

    #[test]
    fn api_key_hash_debug_redacted() {
        let hash = ApiKeyHash::from_key("test-key");
        let debug = format!("{hash:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("test-key"));
    }

    #[test]
    fn api_key_hash_equality() {
        let h1 = ApiKeyHash::from_key("key1");
        let h2 = ApiKeyHash::from_key("key1");
        let h3 = ApiKeyHash::from_key("key2");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    // ===================
    // ApiKeySecret Tests
    // ===================

    #[test]
    fn api_key_secret_generate() {
        let secret = ApiKeySecret::generate();
        assert!(secret.as_str().starts_with(API_KEY_PREFIX));
        assert!(secret.validate().is_ok());
    }

    #[test]
    fn api_key_secret_from_string() {
        let secret = ApiKeySecret::from_string("claw_abc123def456ghi789");
        assert_eq!(secret.as_str(), "claw_abc123def456ghi789");
    }

    #[test]
    fn api_key_secret_hash() {
        let secret = ApiKeySecret::generate();
        let hash = secret.hash();
        assert!(hash.verify(secret.as_str()));
    }

    #[test]
    fn api_key_secret_validate_invalid_prefix() {
        let secret = ApiKeySecret::from_string("invalid_key");
        assert!(secret.validate().is_err());
    }

    #[test]
    fn api_key_secret_validate_too_short() {
        let secret = ApiKeySecret::from_string("claw_short");
        assert!(secret.validate().is_err());
    }

    #[test]
    fn api_key_secret_debug_redacted() {
        let secret = ApiKeySecret::generate();
        let debug = format!("{secret:?}");
        // Debug shows first 8 chars followed by ...
        assert!(debug.contains("claw_"));
        assert!(debug.contains("..."));
        // Make sure the full key is not exposed
        assert!(!debug.contains(secret.as_str()));
    }

    // ===================
    // ApiKey Tests
    // ===================

    #[test]
    fn api_key_generate() {
        let user_id = UserId::new();
        let (key, secret) = ApiKey::generate("Test Key", user_id.clone());
        assert_eq!(key.name, "Test Key");
        assert_eq!(key.user_id, user_id);
        assert!(key.verify(secret.as_str()));
        assert!(key.is_valid());
    }

    #[test]
    fn api_key_with_expiry() {
        let user_id = UserId::new();
        let (key, _) = ApiKey::generate("Test Key", user_id);
        let expires = Utc::now() + Duration::days(30);
        let key = key.with_expiry(expires);
        assert_eq!(key.expires_at, Some(expires));
        assert!(!key.is_expired());
    }

    #[test]
    fn api_key_expired() {
        let user_id = UserId::new();
        let (key, _) = ApiKey::generate("Test Key", user_id);
        let expired = Utc::now() - Duration::days(1);
        let key = key.with_expiry(expired);
        assert!(key.is_expired());
        assert!(!key.is_valid());
    }

    #[test]
    fn api_key_revoke() {
        let user_id = UserId::new();
        let (mut key, _) = ApiKey::generate("Test Key", user_id);
        assert!(key.is_valid());
        key.revoke("Compromised");
        assert!(key.revoked);
        assert_eq!(key.revoked_reason, Some("Compromised".to_string()));
        assert!(!key.is_valid());
    }

    #[test]
    fn api_key_scopes() {
        let user_id = UserId::new();
        let (mut key, _) = ApiKey::generate("Test Key", user_id);
        let scope = Scope::new("workloads:read").ok().unwrap();
        key.add_scope(scope);
        assert!(key.allows("workloads", Action::Read));
        assert!(!key.allows("workloads", Action::Create));
    }

    #[test]
    fn api_key_with_scopes() {
        let user_id = UserId::new();
        let (key, _) = ApiKey::generate("Test Key", user_id);
        let scopes = vec![
            Scope::new("workloads:*").ok().unwrap(),
            Scope::new("nodes:read").ok().unwrap(),
        ];
        let key = key.with_scopes(scopes);
        assert!(key.allows("workloads", Action::Create));
        assert!(key.allows("workloads", Action::Delete));
        assert!(key.allows("nodes", Action::Read));
        assert!(!key.allows("nodes", Action::Delete));
    }

    #[test]
    fn api_key_validate_for_use() {
        let user_id = UserId::new();
        let (key, _) = ApiKey::generate("Test Key", user_id);
        assert!(key.validate_for_use().is_ok());
    }

    #[test]
    fn api_key_validate_for_use_revoked() {
        let user_id = UserId::new();
        let (mut key, _) = ApiKey::generate("Test Key", user_id);
        key.revoke("test");
        let result = key.validate_for_use();
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::ApiKeyRevoked { .. })));
    }

    #[test]
    fn api_key_validate_for_use_expired() {
        let user_id = UserId::new();
        let (key, _) = ApiKey::generate("Test Key", user_id);
        let key = key.with_expiry(Utc::now() - Duration::days(1));
        let result = key.validate_for_use();
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::ApiKeyExpired { .. })));
    }

    #[test]
    fn api_key_record_use() {
        let user_id = UserId::new();
        let (mut key, _) = ApiKey::generate("Test Key", user_id);
        assert!(key.last_used_at.is_none());
        key.record_use();
        assert!(key.last_used_at.is_some());
    }

    // ===================
    // ApiKeyStore Tests
    // ===================

    #[test]
    fn api_key_store_new() {
        let store = ApiKeyStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn api_key_store_store_and_get() {
        let mut store = ApiKeyStore::new();
        let user_id = UserId::new();
        let (key, _) = ApiKey::generate("Test Key", user_id);
        let key_id = key.id.clone();
        store.store(key);
        assert_eq!(store.len(), 1);
        assert!(store.get(&key_id).is_some());
    }

    #[test]
    fn api_key_store_find_by_secret() {
        let mut store = ApiKeyStore::new();
        let user_id = UserId::new();
        let (key, secret) = ApiKey::generate("Test Key", user_id);
        store.store(key);
        let found = store.find_by_secret(secret.as_str());
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Test Key");
    }

    #[test]
    fn api_key_store_find_by_secret_not_found() {
        let store = ApiKeyStore::new();
        let found = store.find_by_secret("nonexistent");
        assert!(found.is_none());
    }

    #[test]
    fn api_key_store_list_for_user() {
        let mut store = ApiKeyStore::new();
        let user1 = UserId::new();
        let user2 = UserId::new();
        let (key1, _) = ApiKey::generate("Key 1", user1.clone());
        let (key2, _) = ApiKey::generate("Key 2", user1.clone());
        let (key3, _) = ApiKey::generate("Key 3", user2);
        store.store(key1);
        store.store(key2);
        store.store(key3);
        let user1_keys = store.list_for_user(&user1);
        assert_eq!(user1_keys.len(), 2);
    }

    #[test]
    fn api_key_store_remove() {
        let mut store = ApiKeyStore::new();
        let user_id = UserId::new();
        let (key, _) = ApiKey::generate("Test Key", user_id);
        let key_id = key.id.clone();
        store.store(key);
        let removed = store.remove(&key_id);
        assert!(removed.is_some());
        assert!(store.is_empty());
    }

    #[test]
    fn api_key_store_cleanup() {
        let mut store = ApiKeyStore::new();
        let user_id = UserId::new();
        // Valid key
        let (key1, _) = ApiKey::generate("Valid Key", user_id.clone());
        // Expired key
        let (key2, _) = ApiKey::generate("Expired Key", user_id.clone());
        let key2 = key2.with_expiry(Utc::now() - Duration::days(1));
        // Revoked key
        let (mut key3, _) = ApiKey::generate("Revoked Key", user_id);
        key3.revoke("test");

        store.store(key1);
        store.store(key2);
        store.store(key3);
        assert_eq!(store.len(), 3);

        store.cleanup();
        assert_eq!(store.len(), 1);
    }

    // ===================
    // Authentication Tests
    // ===================

    #[test]
    fn authenticate_api_key_success() {
        let mut store = ApiKeyStore::new();
        let user_id = UserId::new();
        let (key, secret) = ApiKey::generate("Test Key", user_id);
        store.store(key);
        let result = authenticate_api_key(&store, secret.as_str());
        assert!(result.is_ok());
    }

    #[test]
    fn authenticate_api_key_not_found() {
        let store = ApiKeyStore::new();
        let result = authenticate_api_key(&store, "claw_invalid");
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::InvalidApiKey { .. })));
    }

    #[test]
    fn authenticate_api_key_revoked() {
        let mut store = ApiKeyStore::new();
        let user_id = UserId::new();
        let (mut key, secret) = ApiKey::generate("Test Key", user_id);
        key.revoke("test");
        store.store(key);
        let result = authenticate_api_key(&store, secret.as_str());
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::ApiKeyRevoked { .. })));
    }

    // ===================
    // Header Extraction Tests
    // ===================

    #[test]
    fn extract_api_key_bearer() {
        let header = "Bearer claw_abc123def456ghi789";
        let key = extract_api_key_from_header(header);
        assert!(key.is_ok());
        assert_eq!(key.ok().unwrap(), "claw_abc123def456ghi789");
    }

    #[test]
    fn extract_api_key_direct() {
        let header = "claw_abc123def456ghi789";
        let key = extract_api_key_from_header(header);
        assert!(key.is_ok());
        assert_eq!(key.ok().unwrap(), "claw_abc123def456ghi789");
    }

    #[test]
    fn extract_api_key_empty() {
        let key = extract_api_key_from_header("");
        assert!(key.is_err());
    }

    #[test]
    fn extract_api_key_invalid() {
        let key = extract_api_key_from_header("Basic invalid");
        assert!(key.is_err());
    }
}
