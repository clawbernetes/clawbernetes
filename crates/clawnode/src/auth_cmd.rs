//! Auth & RBAC command handlers
//!
//! Manages API keys and audit logging using ApiKeyStore and AuditLogStore.

use crate::commands::{CommandError, CommandRequest};
use crate::persist::{ApiKeyRecord, AuditLogEntry};
use crate::SharedState;
use ring::digest;
use ring::rand::{SecureRandom, SystemRandom};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

pub async fn handle_auth_command(
    state: &SharedState,
    request: CommandRequest,
) -> Result<Value, CommandError> {
    match request.command.as_str() {
        "auth.create_key" => handle_create_key(state, request.params).await,
        "auth.revoke_key" => handle_revoke_key(state, request.params).await,
        "auth.list_keys" => handle_list_keys(state, request.params).await,
        "audit.query" => handle_audit_query(state, request.params).await,
        _ => Err(format!("unknown auth command: {}", request.command).into()),
    }
}

/// Generate a random API key (32 bytes, hex-encoded).
fn generate_api_secret() -> Result<String, CommandError> {
    let rng = SystemRandom::new();
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes).map_err(|_| "failed to generate random bytes")?;
    Ok(hex::encode(bytes))
}

/// Hash an API secret for storage.
fn hash_secret(secret: &str) -> String {
    let hash = digest::digest(&digest::SHA256, secret.as_bytes());
    hex::encode(hash.as_ref())
}

#[derive(Debug, Deserialize)]
struct CreateKeyParams {
    name: String,
    #[serde(default = "default_role")]
    role: String,
    #[serde(default)]
    scopes: Vec<String>,
}

fn default_role() -> String {
    "viewer".to_string()
}

async fn handle_create_key(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: CreateKeyParams = serde_json::from_value(params)?;

    info!(name = %params.name, role = %params.role, "creating API key");

    let secret = generate_api_secret()?;
    let secret_hash = hash_secret(&secret);
    let key_id = format!("claw-{}", &secret[..12]);

    let record = ApiKeyRecord {
        key_id: key_id.clone(),
        name: params.name.clone(),
        secret_hash,
        scopes: params.scopes.clone(),
        role: params.role.clone(),
        active: true,
        created_at: chrono::Utc::now(),
        last_used: None,
    };

    state
        .api_key_store
        .write()
        .await
        .create(record)
        .map_err(|e| -> CommandError { e.into() })?;

    // Audit log
    state.audit_log_store.write().await.append(AuditLogEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now(),
        actor: "system".to_string(),
        action: "auth.create_key".to_string(),
        resource: "api_key".to_string(),
        resource_id: Some(key_id.clone()),
        result: "success".to_string(),
        details: Some(format!("role={}, name={}", params.role, params.name)),
    });

    Ok(json!({
        "keyId": key_id,
        "name": params.name,
        "role": params.role,
        "secret": secret,
        "scopes": params.scopes,
        "success": true,
        "note": "Store the secret securely — it cannot be retrieved later."
    }))
}

#[derive(Debug, Deserialize)]
struct KeyIdParams {
    #[serde(rename = "keyId")]
    key_id: String,
}

async fn handle_revoke_key(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: KeyIdParams = serde_json::from_value(params)?;

    info!(key_id = %params.key_id, "revoking API key");

    state
        .api_key_store
        .write()
        .await
        .revoke(&params.key_id)
        .map_err(|e| -> CommandError { e.into() })?;

    state.audit_log_store.write().await.append(AuditLogEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now(),
        actor: "system".to_string(),
        action: "auth.revoke_key".to_string(),
        resource: "api_key".to_string(),
        resource_id: Some(params.key_id.clone()),
        result: "success".to_string(),
        details: None,
    });

    Ok(json!({
        "keyId": params.key_id,
        "revoked": true,
    }))
}

async fn handle_list_keys(state: &SharedState, _params: Value) -> Result<Value, CommandError> {
    let store = state.api_key_store.read().await;
    let keys: Vec<Value> = store
        .list()
        .iter()
        .map(|k| {
            json!({
                "keyId": k.key_id,
                "name": k.name,
                "role": k.role,
                "scopes": k.scopes,
                "active": k.active,
                "createdAt": k.created_at.to_rfc3339(),
                "lastUsed": k.last_used.map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(json!({
        "count": keys.len(),
        "keys": keys,
    }))
}

#[derive(Debug, Deserialize)]
struct AuditQueryParams {
    actor: Option<String>,
    action: Option<String>,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    50
}

async fn handle_audit_query(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: AuditQueryParams = serde_json::from_value(params)?;

    let store = state.audit_log_store.read().await;
    let entries = store.query(
        params.actor.as_deref(),
        params.action.as_deref(),
        params.limit,
    );

    let results: Vec<Value> = entries
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "timestamp": e.timestamp.to_rfc3339(),
                "actor": e.actor,
                "action": e.action,
                "resource": e.resource,
                "resourceId": e.resource_id,
                "result": e.result,
                "details": e.details,
            })
        })
        .collect();

    Ok(json!({
        "count": results.len(),
        "entries": results,
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

    #[tokio::test]
    async fn test_create_key() {
        let state = test_state();
        let result = handle_auth_command(
            &state,
            CommandRequest {
                command: "auth.create_key".to_string(),
                params: json!({"name": "test-key", "role": "admin"}),
            },
        )
        .await
        .expect("create");

        assert_eq!(result["success"], true);
        assert!(result["keyId"].as_str().unwrap().starts_with("claw-"));
        assert!(!result["secret"].as_str().unwrap().is_empty());
        assert_eq!(result["role"], "admin");
    }

    #[tokio::test]
    async fn test_revoke_key() {
        let state = test_state();

        let create = handle_auth_command(
            &state,
            CommandRequest {
                command: "auth.create_key".to_string(),
                params: json!({"name": "revoke-me"}),
            },
        )
        .await
        .expect("create");

        let key_id = create["keyId"].as_str().unwrap();

        let result = handle_auth_command(
            &state,
            CommandRequest {
                command: "auth.revoke_key".to_string(),
                params: json!({"keyId": key_id}),
            },
        )
        .await
        .expect("revoke");

        assert_eq!(result["revoked"], true);

        // Verify inactive
        let list = handle_auth_command(
            &state,
            CommandRequest {
                command: "auth.list_keys".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list");

        let keys = list["keys"].as_array().unwrap();
        let key = keys.iter().find(|k| k["keyId"] == key_id).unwrap();
        assert_eq!(key["active"], false);
    }

    #[tokio::test]
    async fn test_audit_log() {
        let state = test_state();

        // Create a key (generates audit entry)
        handle_auth_command(
            &state,
            CommandRequest {
                command: "auth.create_key".to_string(),
                params: json!({"name": "audit-test"}),
            },
        )
        .await
        .expect("create");

        let result = handle_auth_command(
            &state,
            CommandRequest {
                command: "audit.query".to_string(),
                params: json!({"action": "auth.create_key"}),
            },
        )
        .await
        .expect("query");

        assert!(result["count"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn test_hash_deterministic() {
        let h1 = hash_secret("test-secret");
        let h2 = hash_secret("test-secret");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_different_inputs() {
        let h1 = hash_secret("secret-1");
        let h2 = hash_secret("secret-2");
        assert_ne!(h1, h2);
    }
}
