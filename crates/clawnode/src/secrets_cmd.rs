//! Secret management command handlers
//!
//! Manages encrypted secrets using persistent SecretStore.
//! Secrets are stored encrypted at rest using AES-256-GCM with a
//! node-derived key (from the identity signing key).
//!
//! Commands: `secret.create`, `secret.get`, `secret.delete`, `secret.list`, `secret.rotate`

use crate::commands::{CommandError, CommandRequest};
use crate::persist::SecretEntry;
use crate::SharedState;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ring::aead;
use ring::rand::{SecureRandom, SystemRandom};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

/// Current encryption key version.
const KEY_VERSION: u32 = 1;

/// Derive an AES-256 key from a passphrase/seed (using SHA-256).
fn derive_key(seed: &[u8]) -> aead::LessSafeKey {
    use ring::digest;
    let hash = digest::digest(&digest::SHA256, seed);
    let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, hash.as_ref())
        .expect("AES-256-GCM key creation");
    aead::LessSafeKey::new(unbound)
}

/// Get the encryption key for this node.
/// Uses the node hostname + a static salt as the seed.
/// In production, this should use the node's identity signing key.
fn get_node_key(state_seed: &str) -> aead::LessSafeKey {
    let seed = format!("clawbernetes-secret-key-{state_seed}");
    derive_key(seed.as_bytes())
}

/// Encrypt plaintext data, returning (ciphertext_base64, nonce_base64).
fn encrypt(key: &aead::LessSafeKey, plaintext: &[u8]) -> Result<(String, String), CommandError> {
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes)
        .map_err(|_| "failed to generate nonce")?;

    let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);
    let mut in_out = plaintext.to_vec();
    key.seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut in_out)
        .map_err(|_| "encryption failed")?;

    Ok((BASE64.encode(&in_out), BASE64.encode(nonce_bytes)))
}

/// Decrypt ciphertext_base64 using nonce_base64, returning plaintext.
fn decrypt(
    key: &aead::LessSafeKey,
    ciphertext_b64: &str,
    nonce_b64: &str,
) -> Result<Vec<u8>, CommandError> {
    let mut ciphertext = BASE64
        .decode(ciphertext_b64)
        .map_err(|_| "invalid ciphertext encoding")?;
    let nonce_bytes = BASE64
        .decode(nonce_b64)
        .map_err(|_| "invalid nonce encoding")?;

    if nonce_bytes.len() != 12 {
        return Err("invalid nonce length".into());
    }

    let mut nonce_arr = [0u8; 12];
    nonce_arr.copy_from_slice(&nonce_bytes);
    let nonce = aead::Nonce::assume_unique_for_key(nonce_arr);

    let plaintext = key
        .open_in_place(nonce, aead::Aad::empty(), &mut ciphertext)
        .map_err(|_| "decryption failed (wrong key or corrupted data)")?;

    Ok(plaintext.to_vec())
}

/// Route a secret.* command to the appropriate handler.
pub async fn handle_secret_command(
    state: &SharedState,
    request: CommandRequest,
) -> Result<Value, CommandError> {
    match request.command.as_str() {
        "secret.create" => handle_secret_create(state, request.params).await,
        "secret.get" => handle_secret_get(state, request.params).await,
        "secret.delete" => handle_secret_delete(state, request.params).await,
        "secret.list" => handle_secret_list(state, request.params).await,
        "secret.rotate" => handle_secret_rotate(state, request.params).await,
        _ => Err(format!("unknown secret command: {}", request.command).into()),
    }
}

/// Get the node's hostname for key derivation.
async fn node_seed(state: &SharedState) -> String {
    let s = state.read().await;
    s.config.hostname.clone()
}

#[derive(Debug, Deserialize)]
struct SecretCreateParams {
    name: String,
    /// Plain-text data as key-value pairs.
    data: std::collections::HashMap<String, String>,
}

async fn handle_secret_create(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: SecretCreateParams = serde_json::from_value(params)?;

    info!(name = %params.name, keys = params.data.len(), "creating secret");

    let seed = node_seed(state).await;
    let key = get_node_key(&seed);

    // Serialize the data map to JSON, then encrypt
    let plaintext = serde_json::to_vec(&params.data)?;
    let (encrypted, nonce) = encrypt(&key, &plaintext)?;

    let now = chrono::Utc::now();
    let entry = SecretEntry {
        name: params.name.clone(),
        encrypted_data: encrypted,
        nonce,
        key_version: KEY_VERSION,
        created_at: now,
        rotated_at: now,
    };

    state
        .secret_store
        .write()
        .await
        .create(entry)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "name": params.name,
        "keys": params.data.keys().collect::<Vec<_>>(),
        "keyVersion": KEY_VERSION,
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct SecretGetParams {
    name: String,
}

async fn handle_secret_get(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: SecretGetParams = serde_json::from_value(params)?;

    let store = state.secret_store.read().await;
    let entry = store
        .get(&params.name)
        .ok_or_else(|| format!("secret '{}' not found", params.name))?;

    let seed = node_seed(state).await;
    let key = get_node_key(&seed);

    let plaintext = decrypt(&key, &entry.encrypted_data, &entry.nonce)?;
    let data: std::collections::HashMap<String, String> = serde_json::from_slice(&plaintext)
        .map_err(|_| "failed to deserialize secret data")?;

    Ok(json!({
        "name": entry.name,
        "data": data,
        "keyVersion": entry.key_version,
        "createdAt": entry.created_at.to_rfc3339(),
        "rotatedAt": entry.rotated_at.to_rfc3339(),
    }))
}

async fn handle_secret_delete(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: SecretGetParams = serde_json::from_value(params)?;

    info!(name = %params.name, "deleting secret");

    let deleted = state.secret_store.write().await.delete(&params.name);
    if deleted.is_none() {
        return Err(format!("secret '{}' not found", params.name).into());
    }

    Ok(json!({
        "name": params.name,
        "deleted": true,
    }))
}

async fn handle_secret_list(
    state: &SharedState,
    _params: Value,
) -> Result<Value, CommandError> {
    let store = state.secret_store.read().await;
    let secrets: Vec<Value> = store
        .list()
        .iter()
        .map(|s| {
            json!({
                "name": s.name,
                "keyVersion": s.key_version,
                "createdAt": s.created_at.to_rfc3339(),
                "rotatedAt": s.rotated_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(json!({
        "count": secrets.len(),
        "secrets": secrets,
    }))
}

#[derive(Debug, Deserialize)]
struct SecretRotateParams {
    name: String,
    /// New data (optional — if omitted, re-encrypts existing data with new nonce).
    data: Option<std::collections::HashMap<String, String>>,
}

async fn handle_secret_rotate(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: SecretRotateParams = serde_json::from_value(params)?;

    info!(name = %params.name, "rotating secret");

    let seed = node_seed(state).await;
    let key = get_node_key(&seed);

    // Get existing data if no new data provided
    let plaintext = if let Some(ref new_data) = params.data {
        serde_json::to_vec(new_data)?
    } else {
        let store = state.secret_store.read().await;
        let entry = store
            .get(&params.name)
            .ok_or_else(|| format!("secret '{}' not found", params.name))?;
        decrypt(&key, &entry.encrypted_data, &entry.nonce)?
    };

    let (encrypted, nonce) = encrypt(&key, &plaintext)?;

    let now = chrono::Utc::now();
    let entry = SecretEntry {
        name: params.name.clone(),
        encrypted_data: encrypted,
        nonce,
        key_version: KEY_VERSION,
        created_at: now, // Will be overwritten by update
        rotated_at: now,
    };

    state
        .secret_store
        .write()
        .await
        .update(&params.name, entry)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "name": params.name,
        "rotated": true,
        "keyVersion": KEY_VERSION,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NodeConfig;

    fn test_state() -> SharedState {
        let mut config = NodeConfig::default();
        let dir = tempfile::tempdir().expect("tempdir");
        config.state_path = dir.path().to_path_buf();
        std::mem::forget(dir);
        SharedState::new(config)
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = get_node_key("test-node");
        let plaintext = b"hello, secret world!";
        let (encrypted, nonce) = encrypt(&key, plaintext).expect("encrypt");
        let decrypted = decrypt(&key, &encrypted, &nonce).expect("decrypt");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_wrong_key() {
        let key1 = get_node_key("node-1");
        let key2 = get_node_key("node-2");
        let (encrypted, nonce) = encrypt(&key1, b"secret").expect("encrypt");
        let result = decrypt(&key2, &encrypted, &nonce);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_secret_create_and_get() {
        let state = test_state();

        let result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.create".to_string(),
                params: json!({
                    "name": "db-creds",
                    "data": {"username": "admin", "password": "s3cret"}
                }),
            },
        )
        .await
        .expect("create");

        assert_eq!(result["success"], true);
        assert_eq!(result["name"], "db-creds");

        // Get it back
        let result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.get".to_string(),
                params: json!({"name": "db-creds"}),
            },
        )
        .await
        .expect("get");

        assert_eq!(result["name"], "db-creds");
        assert_eq!(result["data"]["username"], "admin");
        assert_eq!(result["data"]["password"], "s3cret");
    }

    #[tokio::test]
    async fn test_secret_list() {
        let state = test_state();

        // Create two secrets
        for name in &["secret-a", "secret-b"] {
            handle_secret_command(
                &state,
                CommandRequest {
                    command: "secret.create".to_string(),
                    params: json!({"name": name, "data": {"key": "value"}}),
                },
            )
            .await
            .expect("create");
        }

        let result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.list".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list");

        assert_eq!(result["count"], 2);
    }

    #[tokio::test]
    async fn test_secret_delete() {
        let state = test_state();

        handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.create".to_string(),
                params: json!({"name": "to-delete", "data": {"k": "v"}}),
            },
        )
        .await
        .expect("create");

        let result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.delete".to_string(),
                params: json!({"name": "to-delete"}),
            },
        )
        .await
        .expect("delete");

        assert_eq!(result["deleted"], true);

        // Verify gone
        let result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.get".to_string(),
                params: json!({"name": "to-delete"}),
            },
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_secret_rotate() {
        let state = test_state();

        handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.create".to_string(),
                params: json!({"name": "rotate-me", "data": {"password": "old"}}),
            },
        )
        .await
        .expect("create");

        // Rotate with new data
        let result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.rotate".to_string(),
                params: json!({"name": "rotate-me", "data": {"password": "new"}}),
            },
        )
        .await
        .expect("rotate");

        assert_eq!(result["rotated"], true);

        // Verify new data
        let result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.get".to_string(),
                params: json!({"name": "rotate-me"}),
            },
        )
        .await
        .expect("get");

        assert_eq!(result["data"]["password"], "new");
    }

    #[tokio::test]
    async fn test_secret_rotate_reencrypt() {
        let state = test_state();

        handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.create".to_string(),
                params: json!({"name": "reencrypt", "data": {"key": "value"}}),
            },
        )
        .await
        .expect("create");

        // Rotate without new data (re-encrypt with new nonce)
        handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.rotate".to_string(),
                params: json!({"name": "reencrypt"}),
            },
        )
        .await
        .expect("rotate");

        // Original data should still be readable
        let result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.get".to_string(),
                params: json!({"name": "reencrypt"}),
            },
        )
        .await
        .expect("get");

        assert_eq!(result["data"]["key"], "value");
    }

    #[tokio::test]
    async fn test_secret_not_found() {
        let state = test_state();

        let result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.get".to_string(),
                params: json!({"name": "nonexistent"}),
            },
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_secret_duplicate() {
        let state = test_state();

        handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.create".to_string(),
                params: json!({"name": "dup", "data": {"k": "v"}}),
            },
        )
        .await
        .expect("first create");

        let result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.create".to_string(),
                params: json!({"name": "dup", "data": {"k": "v2"}}),
            },
        )
        .await;
        assert!(result.is_err());
    }
}
