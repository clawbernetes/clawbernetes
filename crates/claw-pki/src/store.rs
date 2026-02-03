//! Certificate storage implementation.

// Allow significant drop tightening warnings - we intentionally hold RwLock guards
// while accessing the stored data, which is the correct pattern.
#![allow(clippy::significant_drop_tightening)]

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

use chrono::Utc;
use tracing::{debug, info};

use crate::error::{Error, Result};
use crate::types::{Certificate, CertificateId, PrivateKey};

/// In-memory certificate store.
pub struct CertStore {
    /// Storage for certificates and keys.
    store: RwLock<HashMap<CertificateId, StoredCertificate>>,
}

/// A certificate with its associated private key.
struct StoredCertificate {
    certificate: Certificate,
    private_key: PrivateKey,
}

impl CertStore {
    /// Creates a new empty certificate store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
        }
    }

    /// Stores a certificate and its private key.
    ///
    /// # Arguments
    ///
    /// * `cert` - The certificate to store.
    /// * `key` - The private key.
    ///
    /// # Returns
    ///
    /// The unique identifier assigned to the certificate.
    ///
    /// # Errors
    ///
    /// Returns an error if the store cannot be locked.
    pub fn store(&self, cert: Certificate, key: PrivateKey) -> Result<CertificateId> {
        let id = CertificateId::new();
        info!("Storing certificate: {} (subject: {})", id, cert.subject());

        let stored = StoredCertificate {
            certificate: cert,
            private_key: key,
        };

        let mut store = self
            .store
            .write()
            .map_err(|e| Error::Storage(format!("failed to acquire write lock: {e}")))?;

        store.insert(id.clone(), stored);

        debug!("Certificate stored successfully: {}", id);

        Ok(id)
    }

    /// Stores a certificate and key with a specific ID.
    ///
    /// # Arguments
    ///
    /// * `id` - The certificate ID to use.
    /// * `cert` - The certificate to store.
    /// * `key` - The private key.
    ///
    /// # Errors
    ///
    /// Returns an error if the store cannot be locked.
    pub fn store_with_id(
        &self,
        id: &CertificateId,
        cert: Certificate,
        key: PrivateKey,
    ) -> Result<()> {
        info!(
            "Storing certificate with ID: {} (subject: {})",
            id,
            cert.subject()
        );

        let stored = StoredCertificate {
            certificate: cert,
            private_key: key,
        };

        let mut store = self
            .store
            .write()
            .map_err(|e| Error::Storage(format!("failed to acquire write lock: {e}")))?;

        store.insert(id.clone(), stored);

        debug!("Certificate stored successfully: {}", id);

        Ok(())
    }

    /// Retrieves a certificate and its private key.
    ///
    /// # Arguments
    ///
    /// * `id` - The certificate ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the certificate is not found.
    pub fn get(&self, id: &CertificateId) -> Result<(Certificate, PrivateKey)> {
        let store = self
            .store
            .read()
            .map_err(|e| Error::Storage(format!("failed to acquire read lock: {e}")))?;

        let stored = store
            .get(id)
            .ok_or_else(|| Error::NotFound(id.to_string()))?;

        Ok((stored.certificate.clone(), stored.private_key.clone()))
    }

    /// Retrieves only the certificate (without the private key).
    ///
    /// # Arguments
    ///
    /// * `id` - The certificate ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the certificate is not found.
    pub fn get_certificate(&self, id: &CertificateId) -> Result<Certificate> {
        let store = self
            .store
            .read()
            .map_err(|e| Error::Storage(format!("failed to acquire read lock: {e}")))?;

        let stored = store
            .get(id)
            .ok_or_else(|| Error::NotFound(id.to_string()))?;

        Ok(stored.certificate.clone())
    }

    /// Lists certificates expiring within the specified duration.
    ///
    /// # Arguments
    ///
    /// * `within` - The duration to check for expiring certificates.
    ///
    /// # Returns
    ///
    /// A vector of certificate IDs for certificates expiring within the duration.
    #[must_use]
    pub fn list_expiring(&self, within: Duration) -> Vec<CertificateId> {
        let Ok(store) = self.store.read() else {
            return Vec::new();
        };

        let now = Utc::now();
        let threshold = now + chrono::Duration::from_std(within).unwrap_or(chrono::Duration::zero());

        store
            .iter()
            .filter(|(_, stored)| stored.certificate.not_after() <= threshold)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Lists all certificate IDs in the store.
    #[must_use]
    pub fn list_all(&self) -> Vec<CertificateId> {
        let Ok(store) = self.store.read() else {
            return Vec::new();
        };

        store.keys().cloned().collect()
    }

    /// Deletes a certificate from the store.
    ///
    /// # Arguments
    ///
    /// * `id` - The certificate ID to delete.
    ///
    /// # Errors
    ///
    /// Returns an error if the certificate is not found.
    pub fn delete(&self, id: &CertificateId) -> Result<()> {
        info!("Deleting certificate: {}", id);

        let mut store = self
            .store
            .write()
            .map_err(|e| Error::Storage(format!("failed to acquire write lock: {e}")))?;

        if store.remove(id).is_none() {
            return Err(Error::NotFound(id.to_string()));
        }

        debug!("Certificate deleted: {}", id);

        Ok(())
    }

    /// Returns the number of certificates in the store.
    #[must_use]
    pub fn len(&self) -> usize {
        self.store.read().map(|s| s.len()).unwrap_or(0)
    }

    /// Returns true if the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Checks if a certificate exists in the store.
    #[must_use]
    pub fn contains(&self, id: &CertificateId) -> bool {
        self.store.read().map(|s| s.contains_key(id)).unwrap_or(false)
    }
}

impl Default for CertStore {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for CertStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.len();
        f.debug_struct("CertStore")
            .field("count", &count)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration as ChronoDuration, Utc};

    fn create_test_cert(subject: &str, validity_days: i64) -> Certificate {
        let now = Utc::now();
        Certificate::new(
            vec![1, 2, 3],
            now,
            now + ChronoDuration::days(validity_days),
            subject.into(),
            "Test CA".into(),
            vec![],
        )
    }

    fn create_test_key() -> PrivateKey {
        PrivateKey::new(vec![4, 5, 6])
    }

    #[test]
    fn store_new_is_empty() {
        let store = CertStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn store_and_retrieve_certificate() {
        let store = CertStore::new();
        let cert = create_test_cert("test.example.com", 30);
        let key = create_test_key();

        let id = store.store(cert.clone(), key.clone()).unwrap();

        let (retrieved_cert, retrieved_key) = store.get(&id).unwrap();
        assert_eq!(retrieved_cert.subject(), cert.subject());
        assert_eq!(retrieved_key.der(), key.der());
    }

    #[test]
    fn store_with_specific_id() {
        let store = CertStore::new();
        let cert = create_test_cert("test.example.com", 30);
        let key = create_test_key();
        let id = CertificateId::new();

        store.store_with_id(&id, cert.clone(), key).unwrap();

        let (retrieved_cert, _) = store.get(&id).unwrap();
        assert_eq!(retrieved_cert.subject(), cert.subject());
    }

    #[test]
    fn get_certificate_only() {
        let store = CertStore::new();
        let cert = create_test_cert("test.example.com", 30);
        let key = create_test_key();

        let id = store.store(cert.clone(), key).unwrap();

        let retrieved_cert = store.get_certificate(&id).unwrap();
        assert_eq!(retrieved_cert.subject(), cert.subject());
    }

    #[test]
    fn get_nonexistent_certificate() {
        let store = CertStore::new();
        let id = CertificateId::new();

        let result = store.get(&id);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::NotFound(_)));
    }

    #[test]
    fn delete_certificate() {
        let store = CertStore::new();
        let cert = create_test_cert("test.example.com", 30);
        let key = create_test_key();

        let id = store.store(cert, key).unwrap();
        assert!(store.contains(&id));

        store.delete(&id).unwrap();
        assert!(!store.contains(&id));
    }

    #[test]
    fn delete_nonexistent_certificate() {
        let store = CertStore::new();
        let id = CertificateId::new();

        let result = store.delete(&id);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::NotFound(_)));
    }

    #[test]
    fn list_expiring_certificates() {
        let store = CertStore::new();

        // Certificate expiring in 10 days
        let cert1 = create_test_cert("expiring-soon.example.com", 10);
        let id1 = store.store(cert1, create_test_key()).unwrap();

        // Certificate expiring in 100 days
        let cert2 = create_test_cert("not-expiring.example.com", 100);
        let _id2 = store.store(cert2, create_test_key()).unwrap();

        // List certificates expiring in the next 30 days
        let expiring = store.list_expiring(Duration::from_secs(30 * 24 * 60 * 60));

        assert_eq!(expiring.len(), 1);
        assert!(expiring.contains(&id1));
    }

    #[test]
    fn list_all_certificates() {
        let store = CertStore::new();

        let cert1 = create_test_cert("test1.example.com", 30);
        let id1 = store.store(cert1, create_test_key()).unwrap();

        let cert2 = create_test_cert("test2.example.com", 30);
        let id2 = store.store(cert2, create_test_key()).unwrap();

        let all = store.list_all();
        assert_eq!(all.len(), 2);
        assert!(all.contains(&id1));
        assert!(all.contains(&id2));
    }

    #[test]
    fn store_count() {
        let store = CertStore::new();
        assert_eq!(store.len(), 0);

        let cert = create_test_cert("test.example.com", 30);
        store.store(cert, create_test_key()).unwrap();
        assert_eq!(store.len(), 1);

        let cert = create_test_cert("test2.example.com", 30);
        store.store(cert, create_test_key()).unwrap();
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn store_contains() {
        let store = CertStore::new();
        let cert = create_test_cert("test.example.com", 30);
        let key = create_test_key();

        let id = store.store(cert, key).unwrap();
        let nonexistent_id = CertificateId::new();

        assert!(store.contains(&id));
        assert!(!store.contains(&nonexistent_id));
    }

    #[test]
    fn store_debug_format() {
        let store = CertStore::new();
        let debug = format!("{:?}", store);
        assert!(debug.contains("CertStore"));
        assert!(debug.contains("count"));
    }

    #[test]
    fn store_default() {
        let store = CertStore::default();
        assert!(store.is_empty());
    }
}
