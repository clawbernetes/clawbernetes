//! Ed25519 device identity, signing, and challenge-response authentication.
//!
//! Provides [`DeviceIdentity`] for node authentication with the OpenClaw gateway,
//! including keypair generation, persistence, payload signing, and device parameter
//! construction for the WebSocket handshake.

#![forbid(unsafe_code)]

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tracing::{debug, info};

/// Device identity containing an Ed25519 keypair.
#[derive(Clone)]
pub struct DeviceIdentity {
    /// SHA-256 hex digest of the public key.
    pub device_id: String,
    /// Raw 32-byte public key.
    pub public_key_raw: Vec<u8>,
    signing_key: SigningKey,
}

/// Stored identity format (compatible with OpenClaw).
#[derive(Debug, Serialize, Deserialize)]
struct StoredIdentity {
    version: u8,
    device_id: String,
    /// Base64url-encoded raw public key (32 bytes).
    public_key: String,
    /// Base64url-encoded secret key (32 bytes).
    secret_key: String,
    created_at_ms: u64,
}

impl DeviceIdentity {
    /// Generate a new random device identity.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let public_key_raw = verifying_key.to_bytes().to_vec();

        let mut hasher = Sha256::new();
        hasher.update(&public_key_raw);
        let device_id = hex::encode(hasher.finalize());

        info!(device_id = %device_id, "generated new device identity");

        Self {
            device_id,
            public_key_raw,
            signing_key,
        }
    }

    /// Load identity from file, or generate and save if it doesn't exist.
    pub fn load_or_create(path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if path.exists() {
            debug!(path = %path.display(), "loading existing device identity");
            let content = fs::read_to_string(path)?;
            let stored: StoredIdentity = serde_json::from_str(&content)?;

            if stored.version != 1 {
                return Err(format!("unsupported identity version: {}", stored.version).into());
            }

            let secret_bytes = URL_SAFE_NO_PAD.decode(&stored.secret_key)?;
            let secret_array: [u8; 32] = secret_bytes
                .try_into()
                .map_err(|_| "invalid secret key length")?;

            let signing_key = SigningKey::from_bytes(&secret_array);
            let verifying_key = signing_key.verifying_key();
            let public_key_raw = verifying_key.to_bytes().to_vec();

            let mut hasher = Sha256::new();
            hasher.update(&public_key_raw);
            let computed_id = hex::encode(hasher.finalize());

            if computed_id != stored.device_id {
                return Err("device ID mismatch".into());
            }

            info!(device_id = %stored.device_id, "loaded device identity");

            Ok(Self {
                device_id: stored.device_id,
                public_key_raw,
                signing_key,
            })
        } else {
            debug!(path = %path.display(), "creating new device identity");
            let identity = Self::generate();
            identity.save(path)?;
            Ok(identity)
        }
    }

    /// Save identity to file with restrictive permissions.
    pub fn save(&self, path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let stored = StoredIdentity {
            version: 1,
            device_id: self.device_id.clone(),
            public_key: URL_SAFE_NO_PAD.encode(&self.public_key_raw),
            secret_key: URL_SAFE_NO_PAD.encode(self.signing_key.to_bytes()),
            created_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };

        let content = serde_json::to_string_pretty(&stored)?;
        fs::write(path, content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(path, perms)?;
        }

        info!(path = %path.display(), "saved device identity");
        Ok(())
    }

    /// Get the public key as base64url string.
    pub fn public_key_base64url(&self) -> String {
        URL_SAFE_NO_PAD.encode(&self.public_key_raw)
    }

    /// Sign a payload and return base64url-encoded signature.
    pub fn sign(&self, payload: &str) -> String {
        let signature = self.signing_key.sign(payload.as_bytes());
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    }

    /// Build the authentication payload string.
    pub fn build_auth_payload(
        &self,
        client_id: &str,
        client_mode: &str,
        role: &str,
        scopes: &[String],
        signed_at_ms: u64,
        token: Option<&str>,
        nonce: Option<&str>,
    ) -> String {
        let version = if nonce.is_some() { "v2" } else { "v1" };
        let scopes_str = scopes.join(",");
        let token_str = token.unwrap_or("");

        let mut parts = vec![
            version.to_string(),
            self.device_id.clone(),
            client_id.to_string(),
            client_mode.to_string(),
            role.to_string(),
            scopes_str,
            signed_at_ms.to_string(),
            token_str.to_string(),
        ];

        if version == "v2" {
            parts.push(nonce.unwrap_or("").to_string());
        }

        parts.join("|")
    }

    /// Create device params for the WebSocket connect message.
    pub fn device_params(
        &self,
        client_id: &str,
        client_mode: &str,
        role: &str,
        scopes: &[String],
        token: Option<&str>,
        nonce: Option<&str>,
    ) -> DeviceParams {
        let signed_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let payload = self.build_auth_payload(
            client_id, client_mode, role, scopes, signed_at, token, nonce,
        );

        let signature = self.sign(&payload);

        info!(payload = %payload, sig = %signature, "signing auth payload");

        DeviceParams {
            id: self.device_id.clone(),
            public_key: self.public_key_base64url(),
            signature,
            signed_at,
            nonce: nonce.map(|s| s.to_string()),
        }
    }
}

/// Device parameters for the WebSocket connect handshake.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceParams {
    /// Device ID (SHA-256 of public key).
    pub id: String,
    /// Base64url-encoded public key.
    pub public_key: String,
    /// Base64url-encoded signature.
    pub signature: String,
    /// Signing timestamp (ms since epoch).
    pub signed_at: u64,
    /// Optional nonce (v2 protocol).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_identity() {
        let identity = DeviceIdentity::generate();
        assert_eq!(identity.device_id.len(), 64); // SHA-256 hex
        assert_eq!(identity.public_key_raw.len(), 32);
    }

    #[test]
    fn test_sign_and_verify_consistent() {
        let identity = DeviceIdentity::generate();
        let payload = "test|payload|data";
        let sig1 = identity.sign(payload);
        let sig2 = identity.sign(payload);
        // Same key + same payload = same signature (Ed25519 is deterministic)
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_sign_different_payloads() {
        let identity = DeviceIdentity::generate();
        let sig1 = identity.sign("payload-a");
        let sig2 = identity.sign("payload-b");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_signature_length() {
        let identity = DeviceIdentity::generate();
        let sig = identity.sign("test");
        // Ed25519 signature = 64 bytes, base64url ~86 chars
        assert!(sig.len() > 80);
    }

    #[test]
    fn test_save_and_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("device.json");

        let id1 = DeviceIdentity::generate();
        id1.save(&path).expect("save");

        let id2 = DeviceIdentity::load_or_create(&path).expect("load");

        assert_eq!(id1.device_id, id2.device_id);
        assert_eq!(id1.public_key_raw, id2.public_key_raw);
    }

    #[test]
    fn test_load_or_create_new() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("new-device.json");

        let identity = DeviceIdentity::load_or_create(&path).expect("create");
        assert_eq!(identity.device_id.len(), 64);
        assert!(path.exists());
    }

    #[test]
    fn test_public_key_base64url() {
        let identity = DeviceIdentity::generate();
        let b64 = identity.public_key_base64url();
        // 32 bytes base64url = 43 chars (no padding)
        assert_eq!(b64.len(), 43);
    }

    #[test]
    fn test_build_auth_payload_v1() {
        let identity = DeviceIdentity::generate();
        let payload = identity.build_auth_payload(
            "client-1", "node", "compute", &["gpu".to_string()],
            1234567890, Some("tok"), None,
        );
        assert!(payload.starts_with("v1|"));
        assert!(payload.contains(&identity.device_id));
    }

    #[test]
    fn test_build_auth_payload_v2() {
        let identity = DeviceIdentity::generate();
        let payload = identity.build_auth_payload(
            "client-1", "node", "compute", &[],
            1234567890, None, Some("nonce-abc"),
        );
        assert!(payload.starts_with("v2|"));
        assert!(payload.contains("nonce-abc"));
    }

    #[test]
    fn test_device_params() {
        let identity = DeviceIdentity::generate();
        let params = identity.device_params(
            "c1", "node", "compute", &[], None, None,
        );
        assert_eq!(params.id, identity.device_id);
        assert!(!params.signature.is_empty());
        assert!(params.signed_at > 0);
        assert!(params.nonce.is_none());
    }

    #[test]
    fn test_device_params_with_nonce() {
        let identity = DeviceIdentity::generate();
        let params = identity.device_params(
            "c1", "node", "compute", &[], Some("token"), Some("nonce-123"),
        );
        assert_eq!(params.nonce.as_deref(), Some("nonce-123"));
    }
}
