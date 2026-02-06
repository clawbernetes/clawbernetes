//! Device identity and signing for OpenClaw protocol
//!
//! Implements Ed25519-based device authentication compatible with OpenClaw gateway.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tracing::{debug, info};

/// Device identity containing keypair
#[derive(Clone)]
pub struct DeviceIdentity {
    pub device_id: String,
    pub public_key_raw: Vec<u8>,
    signing_key: SigningKey,
}

/// Stored identity format (compatible with OpenClaw)
#[derive(Debug, Serialize, Deserialize)]
struct StoredIdentity {
    version: u8,
    device_id: String,
    /// Base64url-encoded raw public key (32 bytes)
    public_key: String,
    /// Base64url-encoded secret key (32 bytes)
    secret_key: String,
    created_at_ms: u64,
}

impl DeviceIdentity {
    /// Generate a new device identity
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let public_key_raw = verifying_key.to_bytes().to_vec();
        
        // Device ID is SHA256 of the raw public key
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
    
    /// Load identity from file, or generate and save if not exists
    pub fn load_or_create(path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if path.exists() {
            debug!(path = %path.display(), "loading existing device identity");
            let content = fs::read_to_string(path)?;
            let stored: StoredIdentity = serde_json::from_str(&content)?;
            
            if stored.version != 1 {
                return Err(format!("unsupported identity version: {}", stored.version).into());
            }
            
            let secret_bytes = URL_SAFE_NO_PAD.decode(&stored.secret_key)?;
            let secret_array: [u8; 32] = secret_bytes.try_into()
                .map_err(|_| "invalid secret key length")?;
            
            let signing_key = SigningKey::from_bytes(&secret_array);
            let verifying_key = signing_key.verifying_key();
            let public_key_raw = verifying_key.to_bytes().to_vec();
            
            // Verify device ID matches
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
    
    /// Save identity to file
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
                .unwrap()
                .as_millis() as u64,
        };
        
        let content = serde_json::to_string_pretty(&stored)?;
        fs::write(path, content)?;
        
        // Set restrictive permissions on Unix
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
    
    /// Get the public key as base64url (for protocol)
    pub fn public_key_base64url(&self) -> String {
        URL_SAFE_NO_PAD.encode(&self.public_key_raw)
    }
    
    /// Sign a payload and return base64url signature
    pub fn sign(&self, payload: &str) -> String {
        let signature = self.signing_key.sign(payload.as_bytes());
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    }
    
    /// Build the authentication payload (v2 with nonce)
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
    
    /// Create device params for connect message
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
            .unwrap()
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

/// Device params for connect message
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceParams {
    pub id: String,
    pub public_key: String,
    pub signature: String,
    pub signed_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_generate_and_sign() {
        let identity = DeviceIdentity::generate();
        
        // Device ID should be 64 hex chars (SHA256)
        assert_eq!(identity.device_id.len(), 64);
        
        // Public key should be 32 bytes
        assert_eq!(identity.public_key_raw.len(), 32);
        
        // Test signing
        let payload = "test|payload|data";
        let sig = identity.sign(payload);
        
        // Signature should be base64url encoded (64 bytes = ~86 chars)
        assert!(sig.len() > 80);
    }
    
    #[test]
    fn test_save_and_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("device.json");
        
        let identity1 = DeviceIdentity::generate();
        identity1.save(&path).unwrap();
        
        let identity2 = DeviceIdentity::load_or_create(&path).unwrap();
        
        assert_eq!(identity1.device_id, identity2.device_id);
        assert_eq!(identity1.public_key_raw, identity2.public_key_raw);
    }
}
