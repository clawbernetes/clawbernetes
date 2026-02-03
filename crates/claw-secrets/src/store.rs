//! Secret storage with encryption at rest.
//!
//! This module provides the main secret store that manages encrypted
//! secrets with access control and audit logging.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::access::AccessController;
use crate::encryption::{decrypt, encrypt, SecretKey};
use crate::error::{Error, Result};
use crate::types::{
    AccessPolicy, Accessor, SecretId, SecretMetadata, SecretValue, WorkloadId,
};

/// A stored secret with its metadata and access policy.
#[derive(Clone)]
struct StoredSecret {
    /// The encrypted secret value.
    encrypted_value: Vec<u8>,
    /// Metadata about the secret.
    metadata: SecretMetadata,
    /// Access policy for the secret.
    policy: AccessPolicy,
}

/// A secure secret store with encryption at rest.
///
/// All secrets are encrypted using ChaCha20-Poly1305 with keys derived
/// from a master key. Access is controlled via policies and all operations
/// are logged for audit purposes.
pub struct SecretStore {
    /// The master key used for key derivation.
    master_key: SecretKey,
    /// In-memory storage for secrets.
    secrets: RwLock<HashMap<SecretId, StoredSecret>>,
    /// Access controller for policy enforcement.
    access_controller: AccessController,
}

impl SecretStore {
    /// Creates a new secret store with the given master key.
    #[must_use]
    pub fn new(master_key: SecretKey) -> Self {
        Self {
            master_key,
            secrets: RwLock::new(HashMap::new()),
            access_controller: AccessController::with_new_audit_log(),
        }
    }

    /// Creates a new secret store with an existing access controller.
    ///
    /// This allows sharing an audit log between multiple components.
    #[must_use]
    pub fn with_access_controller(master_key: SecretKey, access_controller: AccessController) -> Self {
        Self {
            master_key,
            secrets: RwLock::new(HashMap::new()),
            access_controller,
        }
    }

    /// Returns a reference to the access controller.
    #[must_use]
    pub const fn access_controller(&self) -> &AccessController {
        &self.access_controller
    }

    /// Stores a secret with the given access policy.
    ///
    /// # Arguments
    ///
    /// * `id` - The identifier for the secret
    /// * `value` - The plaintext secret value
    /// * `policy` - The access policy for the secret
    ///
    /// # Errors
    ///
    /// Returns an error if encryption fails.
    pub fn put(&self, id: &SecretId, value: &[u8], policy: AccessPolicy) -> Result<()> {
        // Derive a key specific to this secret
        let secret_key = self.master_key.derive_for_secret(id);

        // Encrypt the value
        let encrypted_value = encrypt(&secret_key, value)?;

        let stored = StoredSecret {
            encrypted_value,
            metadata: SecretMetadata::new(),
            policy,
        };

        // Store the secret
        {
            let mut secrets = self
                .secrets
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            secrets.insert(id.clone(), stored);
        }

        // Record the creation in the audit log
        self.access_controller
            .record_created(id, &Accessor::System, "secret stored");

        Ok(())
    }

    /// Retrieves a secret if the accessor is allowed.
    ///
    /// # Arguments
    ///
    /// * `id` - The identifier of the secret
    /// * `accessor` - Who is accessing the secret
    /// * `reason` - Human-readable reason for the access
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The secret does not exist
    /// - The accessor is not allowed
    /// - Decryption fails
    pub fn get(
        &self,
        id: &SecretId,
        accessor: &Accessor,
        reason: &str,
    ) -> Result<SecretValue> {
        let secrets = self
            .secrets
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let stored = secrets.get(id).ok_or_else(|| Error::SecretNotFound {
            id: id.to_string(),
        })?;

        // Check access policy
        self.access_controller
            .check(id, &stored.policy, accessor, reason)?;

        // Derive the key and decrypt
        let secret_key = self.master_key.derive_for_secret(id);
        let plaintext = decrypt(&secret_key, &stored.encrypted_value)?;

        Ok(SecretValue::new(plaintext))
    }

    /// Retrieves a secret by workload ID.
    ///
    /// This is a convenience method that wraps the workload ID in an accessor.
    ///
    /// # Errors
    ///
    /// Returns an error if access is denied or the secret doesn't exist.
    pub fn get_for_workload(
        &self,
        id: &SecretId,
        workload: &WorkloadId,
        reason: &str,
    ) -> Result<SecretValue> {
        self.get(id, &Accessor::Workload(workload.clone()), reason)
    }

    /// Deletes a secret.
    ///
    /// # Errors
    ///
    /// Returns an error if the secret does not exist.
    pub fn delete(&self, id: &SecretId) -> Result<()> {
        let mut secrets = self
            .secrets
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if secrets.remove(id).is_none() {
            return Err(Error::SecretNotFound {
                id: id.to_string(),
            });
        }

        // Record deletion in audit log
        self.access_controller
            .record_deleted(id, &Accessor::System, "secret deleted");

        Ok(())
    }

    /// Lists all secret identifiers in the store.
    #[must_use]
    pub fn list(&self) -> Vec<SecretId> {
        let secrets = self
            .secrets
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        secrets.keys().cloned().collect()
    }

    /// Rotates a secret to a new value.
    ///
    /// This updates the secret value while preserving metadata (with
    /// incremented version) and the access policy.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The secret does not exist
    /// - Encryption fails
    pub fn rotate(&self, id: &SecretId, new_value: &[u8]) -> Result<()> {
        let mut secrets = self
            .secrets
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let stored = secrets.get_mut(id).ok_or_else(|| Error::SecretNotFound {
            id: id.to_string(),
        })?;

        // Derive the key and encrypt the new value
        let secret_key = self.master_key.derive_for_secret(id);
        let encrypted_value = encrypt(&secret_key, new_value)?;

        // Update the stored secret
        stored.encrypted_value = encrypted_value;
        stored.metadata.bump_version();

        // Record rotation in audit log (need to drop lock first)
        drop(secrets);
        self.access_controller
            .record_rotated(id, &Accessor::System, "secret rotated");

        Ok(())
    }

    /// Updates the access policy for a secret.
    ///
    /// # Errors
    ///
    /// Returns an error if the secret does not exist.
    pub fn update_policy(&self, id: &SecretId, policy: AccessPolicy) -> Result<()> {
        let mut secrets = self
            .secrets
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let stored = secrets.get_mut(id).ok_or_else(|| Error::SecretNotFound {
            id: id.to_string(),
        })?;

        stored.policy = policy;
        stored.metadata.bump_version();

        // Record update in audit log
        drop(secrets);
        self.access_controller
            .record_updated(id, &Accessor::System, "policy updated");

        Ok(())
    }

    /// Returns the metadata for a secret.
    ///
    /// # Errors
    ///
    /// Returns an error if the secret does not exist.
    pub fn metadata(&self, id: &SecretId) -> Result<SecretMetadata> {
        let secrets = self
            .secrets
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let stored = secrets.get(id).ok_or_else(|| Error::SecretNotFound {
            id: id.to_string(),
        })?;

        Ok(stored.metadata.clone())
    }

    /// Returns the number of secrets in the store.
    #[must_use]
    pub fn len(&self) -> usize {
        self.secrets
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Returns true if the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Checks if a secret exists in the store.
    #[must_use]
    pub fn contains(&self, id: &SecretId) -> bool {
        self.secrets
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(id)
    }
}

impl std::fmt::Debug for SecretStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.len();
        f.debug_struct("SecretStore")
            .field("secrets_count", &len)
            .field("master_key", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::AuditFilter;
    use crate::types::{AuditAction, NodeId};

    fn test_store() -> SecretStore {
        SecretStore::new(SecretKey::generate())
    }

    fn test_secret_id(name: &str) -> SecretId {
        SecretId::new(name).expect("valid id")
    }

    #[test]
    fn secret_store_new_is_empty() {
        let store = test_store();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn secret_store_put_and_get() {
        let store = test_store();
        let id = test_secret_id("my-secret");
        let value = b"super secret value";
        let workload = WorkloadId::new("my-workload");
        let policy = AccessPolicy::allow_workloads(vec![workload.clone()]);

        // Store the secret
        store.put(&id, value, policy).expect("put should succeed");

        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
        assert!(store.contains(&id));

        // Retrieve it
        let retrieved = store
            .get_for_workload(&id, &workload, "test access")
            .expect("get should succeed");

        assert_eq!(retrieved.as_bytes(), value);
    }

    #[test]
    fn secret_store_get_nonexistent() {
        let store = test_store();
        let id = test_secret_id("nonexistent");

        let result = store.get(&id, &Accessor::System, "test");

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::SecretNotFound { .. }));
    }

    #[test]
    fn secret_store_get_access_denied() {
        let store = test_store();
        let id = test_secret_id("restricted");
        let allowed = WorkloadId::new("allowed");
        let denied = WorkloadId::new("denied");
        let policy = AccessPolicy::allow_workloads(vec![allowed]);

        store
            .put(&id, b"secret", policy)
            .expect("put should succeed");

        let result = store.get_for_workload(&id, &denied, "unauthorized access");

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::AccessDenied { .. }));
    }

    #[test]
    fn secret_store_delete() {
        let store = test_store();
        let id = test_secret_id("to-delete");
        let policy = AccessPolicy::new();

        store
            .put(&id, b"secret", policy)
            .expect("put should succeed");
        assert!(store.contains(&id));

        store.delete(&id).expect("delete should succeed");

        assert!(!store.contains(&id));
        assert!(store.is_empty());
    }

    #[test]
    fn secret_store_delete_nonexistent() {
        let store = test_store();
        let id = test_secret_id("nonexistent");

        let result = store.delete(&id);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::SecretNotFound { .. }));
    }

    #[test]
    fn secret_store_list() {
        let store = test_store();
        let policy = AccessPolicy::new();

        store
            .put(&test_secret_id("secret1"), b"value1", policy.clone())
            .expect("put 1");
        store
            .put(&test_secret_id("secret2"), b"value2", policy.clone())
            .expect("put 2");
        store
            .put(&test_secret_id("secret3"), b"value3", policy)
            .expect("put 3");

        let list = store.list();

        assert_eq!(list.len(), 3);
        // Check all secrets are present (order not guaranteed)
        let ids: Vec<&str> = list.iter().map(SecretId::as_str).collect();
        assert!(ids.contains(&"secret1"));
        assert!(ids.contains(&"secret2"));
        assert!(ids.contains(&"secret3"));
    }

    #[test]
    fn secret_store_rotate() {
        let store = test_store();
        let id = test_secret_id("rotating");
        let workload = WorkloadId::new("worker");
        let policy = AccessPolicy::allow_workloads(vec![workload.clone()]);

        // Store initial value
        store
            .put(&id, b"initial value", policy)
            .expect("put should succeed");

        let meta_before = store.metadata(&id).expect("metadata");
        assert_eq!(meta_before.version, 1);

        // Rotate to new value
        store
            .rotate(&id, b"rotated value")
            .expect("rotate should succeed");

        // Check new value
        let retrieved = store
            .get_for_workload(&id, &workload, "after rotation")
            .expect("get should succeed");
        assert_eq!(retrieved.as_bytes(), b"rotated value");

        // Check version incremented
        let meta_after = store.metadata(&id).expect("metadata");
        assert_eq!(meta_after.version, 2);
    }

    #[test]
    fn secret_store_rotate_nonexistent() {
        let store = test_store();
        let id = test_secret_id("nonexistent");

        let result = store.rotate(&id, b"new value");

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::SecretNotFound { .. }));
    }

    #[test]
    fn secret_store_update_policy() {
        let store = test_store();
        let id = test_secret_id("policy-update");
        let workload1 = WorkloadId::new("worker1");
        let workload2 = WorkloadId::new("worker2");

        // Store with initial policy
        let initial_policy = AccessPolicy::allow_workloads(vec![workload1.clone()]);
        store
            .put(&id, b"secret", initial_policy)
            .expect("put should succeed");

        // workload1 can access, workload2 cannot
        assert!(store
            .get_for_workload(&id, &workload1, "test")
            .is_ok());
        assert!(store
            .get_for_workload(&id, &workload2, "test")
            .is_err());

        // Update policy
        let new_policy = AccessPolicy::allow_workloads(vec![workload2.clone()]);
        store
            .update_policy(&id, new_policy)
            .expect("update should succeed");

        // Now workload2 can access, workload1 cannot
        assert!(store
            .get_for_workload(&id, &workload1, "test")
            .is_err());
        assert!(store
            .get_for_workload(&id, &workload2, "test")
            .is_ok());
    }

    #[test]
    fn secret_store_metadata() {
        let store = test_store();
        let id = test_secret_id("with-metadata");
        let policy = AccessPolicy::new();

        store
            .put(&id, b"secret", policy)
            .expect("put should succeed");

        let meta = store.metadata(&id).expect("metadata should exist");

        assert_eq!(meta.version, 1);
        assert!(!meta.rotation_policy.auto_rotate);
    }

    #[test]
    fn secret_store_metadata_nonexistent() {
        let store = test_store();
        let id = test_secret_id("nonexistent");

        let result = store.metadata(&id);

        assert!(result.is_err());
    }

    #[test]
    fn secret_store_audit_logging() {
        let store = test_store();
        let id = test_secret_id("audited");
        let workload = WorkloadId::new("worker");
        let policy = AccessPolicy::allow_workloads(vec![workload.clone()]);

        // Perform operations
        store.put(&id, b"secret", policy).expect("put");
        let _ = store.get_for_workload(&id, &workload, "read access");
        store.rotate(&id, b"new value").expect("rotate");
        store.delete(&id).expect("delete");

        // Check audit log
        let entries = store
            .access_controller()
            .audit_log()
            .query(&AuditFilter::new());

        assert_eq!(entries.len(), 4);

        // Entries should be in reverse chronological order
        let actions: Vec<AuditAction> = entries.iter().map(|e| e.action).collect();
        assert!(actions.contains(&AuditAction::Created));
        assert!(actions.contains(&AuditAction::Read));
        assert!(actions.contains(&AuditAction::Rotated));
        assert!(actions.contains(&AuditAction::Deleted));
    }

    #[test]
    fn secret_store_different_secrets_different_keys() {
        let store = test_store();
        let id1 = test_secret_id("secret1");
        let id2 = test_secret_id("secret2");
        let policy = AccessPolicy::new();

        // Store same value under different IDs
        store
            .put(&id1, b"same value", policy.clone())
            .expect("put 1");
        store.put(&id2, b"same value", policy).expect("put 2");

        // Both should decrypt correctly
        let v1 = store
            .get(&id1, &Accessor::System, "test")
            .expect("get 1");
        let v2 = store
            .get(&id2, &Accessor::System, "test")
            .expect("get 2");

        assert_eq!(v1.as_bytes(), v2.as_bytes());
    }

    #[test]
    fn secret_store_system_accessor_always_allowed() {
        let store = test_store();
        let id = test_secret_id("system-access");
        let policy = AccessPolicy::new(); // Empty policy

        store.put(&id, b"secret", policy).expect("put");

        // System should still be able to access
        let result = store.get(&id, &Accessor::System, "system access");
        assert!(result.is_ok());
    }

    #[test]
    fn secret_store_admin_accessor_always_allowed() {
        let store = test_store();
        let id = test_secret_id("admin-access");
        let policy = AccessPolicy::new(); // Empty policy

        store.put(&id, b"secret", policy).expect("put");

        // Admin should still be able to access
        let admin = Accessor::Admin("root".to_string());
        let result = store.get(&id, &admin, "admin access");
        assert!(result.is_ok());
    }

    #[test]
    fn secret_store_node_accessor() {
        let store = test_store();
        let id = test_secret_id("node-access");
        let node = NodeId::new("node-1");
        let policy = AccessPolicy::allow_nodes(vec![node.clone()]);

        store.put(&id, b"secret", policy).expect("put");

        // Node should be able to access
        let result = store.get(&id, &Accessor::Node(node), "node access");
        assert!(result.is_ok());
    }

    #[test]
    fn secret_store_debug_redacts_key() {
        let store = test_store();
        let debug = format!("{store:?}");

        assert!(debug.contains("[REDACTED]"));
        assert!(debug.contains("SecretStore"));
    }

    #[test]
    fn secret_store_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let store = Arc::new(test_store());
        let mut handles = vec![];

        // Writer thread
        let store_writer = Arc::clone(&store);
        let writer = thread::spawn(move || {
            for i in 0..10 {
                let id = test_secret_id(&format!("secret{i}"));
                let policy = AccessPolicy::new();
                store_writer
                    .put(&id, format!("value{i}").as_bytes(), policy)
                    .expect("put should succeed");
            }
        });
        handles.push(writer);

        // Reader threads
        for _ in 0..3 {
            let store_reader = Arc::clone(&store);
            let reader = thread::spawn(move || {
                for _ in 0..20 {
                    let _ = store_reader.list();
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            });
            handles.push(reader);
        }

        for handle in handles {
            handle.join().expect("thread should complete");
        }

        assert_eq!(store.len(), 10);
    }
}
