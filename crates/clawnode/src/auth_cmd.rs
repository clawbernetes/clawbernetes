//! Auth, RBAC, and audit command handlers
//!
//! Provides 7 commands (requires `auth` feature):
//! `auth.create_key`, `auth.revoke_key`, `auth.list_keys`,
//! `rbac.create_role`, `rbac.bind`, `rbac.check`,
//! `audit.query`

use crate::commands::{CommandError, CommandRequest};
use crate::persist::AuditEntry;
use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

/// Route an auth.*, rbac.*, or audit.* command.
pub async fn handle_auth_command(
    state: &SharedState,
    request: CommandRequest,
) -> Result<Value, CommandError> {
    match request.command.as_str() {
        "auth.create_key" => handle_create_key(state, request.params).await,
        "auth.revoke_key" => handle_revoke_key(state, request.params).await,
        "auth.list_keys" => handle_list_keys(state).await,
        "rbac.create_role" => handle_create_role(state, request.params).await,
        "rbac.bind" => handle_rbac_bind(state, request.params).await,
        "rbac.check" => handle_rbac_check(state, request.params).await,
        "audit.query" => handle_audit_query(state, request.params).await,
        _ => Err(format!("unknown auth command: {}", request.command).into()),
    }
}

// ─────────────────────────────────────────────────────────────
// API Key Commands
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateKeyParams {
    name: String,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(rename = "expiresIn")]
    expires_in: Option<String>,
}

async fn handle_create_key(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: CreateKeyParams = serde_json::from_value(params)?;

    info!(name = %params.name, "creating API key");

    let user_id = claw_auth::UserId::new();
    let (mut key, secret) = claw_auth::ApiKey::generate(&params.name, user_id.clone());

    // Add scopes
    let mut scope_list = Vec::new();
    for scope_str in &params.scopes {
        let scope = claw_auth::Scope::new(scope_str)
            .map_err(|e| format!("invalid scope '{scope_str}': {e}"))?;
        scope_list.push(scope);
    }
    if !scope_list.is_empty() {
        key = key.with_scopes(scope_list);
    }

    let key_id = key.id.to_string();
    let mut store = state.api_key_store.write().await;
    store.store(key);

    // Record audit
    let mut audit = state.audit_store.write().await;
    audit.record(AuditEntry {
        principal: "system".to_string(),
        action: "auth.create_key".to_string(),
        resource: params.name.clone(),
        result: "allowed".to_string(),
        timestamp: chrono::Utc::now(),
    });

    Ok(json!({
        "keyId": key_id,
        "name": params.name,
        "secret": secret.as_str(),
        "scopes": params.scopes,
        "userId": user_id.to_string(),
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct RevokeKeyParams {
    #[serde(rename = "keyId")]
    key_id: String,
    reason: Option<String>,
}

async fn handle_revoke_key(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: RevokeKeyParams = serde_json::from_value(params)?;

    info!(key_id = %params.key_id, "revoking API key");

    let key_id = claw_auth::ApiKeyId::from_string(&params.key_id)
        .map_err(|e| format!("invalid key ID: {e}"))?;

    let mut store = state.api_key_store.write().await;
    let key = store
        .get_mut(&key_id)
        .ok_or_else(|| format!("key '{}' not found", params.key_id))?;

    key.revoke(params.reason.as_deref().unwrap_or("manual revocation"));

    // Record audit
    let mut audit = state.audit_store.write().await;
    audit.record(AuditEntry {
        principal: "system".to_string(),
        action: "auth.revoke_key".to_string(),
        resource: params.key_id.clone(),
        result: "allowed".to_string(),
        timestamp: chrono::Utc::now(),
    });

    Ok(json!({
        "keyId": params.key_id,
        "revoked": true,
    }))
}

async fn handle_list_keys(state: &SharedState) -> Result<Value, CommandError> {
    let store = state.api_key_store.read().await;
    let count = store.len();

    Ok(json!({
        "count": count,
    }))
}

// ─────────────────────────────────────────────────────────────
// RBAC Commands
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateRoleParams {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    permissions: Vec<PermissionParam>,
}

#[derive(Debug, Deserialize)]
struct PermissionParam {
    resource: String,
    action: String,
}

async fn handle_create_role(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: CreateRoleParams = serde_json::from_value(params)?;

    info!(name = %params.name, "creating RBAC role");

    let mut role = claw_auth::Role::new(&params.name, &params.description)
        .map_err(|e| format!("invalid role: {e}"))?;

    for perm in &params.permissions {
        let action = parse_action(&perm.action)?;
        let permission = claw_auth::Permission::new(&perm.resource, action)
            .map_err(|e| format!("invalid permission: {e}"))?;
        role.add_permission(permission);
    }

    let mut policy = state.rbac_policy.write().await;
    policy.add_role(role);

    Ok(json!({
        "name": params.name,
        "permissions": params.permissions.len(),
        "success": true,
    }))
}

fn parse_action(s: &str) -> Result<claw_auth::Action, CommandError> {
    match s.to_lowercase().as_str() {
        "create" => Ok(claw_auth::Action::Create),
        "read" | "get" => Ok(claw_auth::Action::Read),
        "update" => Ok(claw_auth::Action::Update),
        "delete" => Ok(claw_auth::Action::Delete),
        "list" => Ok(claw_auth::Action::List),
        "execute" | "exec" => Ok(claw_auth::Action::Execute),
        "admin" | "*" => Ok(claw_auth::Action::Admin),
        _ => Err(format!("unknown action: {s} (use create/read/update/delete/list/execute/admin)")
            .into()),
    }
}

#[derive(Debug, Deserialize)]
struct RbacBindParams {
    #[serde(rename = "userId")]
    user_id: String,
    role: String,
}

async fn handle_rbac_bind(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: RbacBindParams = serde_json::from_value(params)?;

    info!(user = %params.user_id, role = %params.role, "binding role");

    let user_id = claw_auth::UserId::from_string(&params.user_id)
        .map_err(|e| format!("invalid user ID: {e}"))?;

    let mut policy = state.rbac_policy.write().await;

    // Ensure user exists (create if not)
    if policy.get_user(&user_id).is_none() {
        let user = claw_auth::User::with_id(user_id.clone(), &params.user_id);
        policy.add_user(user);
    }

    policy
        .assign_role(&user_id, &params.role)
        .map_err(|e| format!("assign failed: {e}"))?;

    // Record audit
    let mut audit = state.audit_store.write().await;
    audit.record(AuditEntry {
        principal: "system".to_string(),
        action: "rbac.bind".to_string(),
        resource: format!("{}:{}", params.user_id, params.role),
        result: "allowed".to_string(),
        timestamp: chrono::Utc::now(),
    });

    Ok(json!({
        "userId": params.user_id,
        "role": params.role,
        "bound": true,
    }))
}

#[derive(Debug, Deserialize)]
struct RbacCheckParams {
    #[serde(rename = "userId")]
    user_id: String,
    action: String,
    resource: String,
}

async fn handle_rbac_check(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: RbacCheckParams = serde_json::from_value(params)?;

    let user_id = claw_auth::UserId::from_string(&params.user_id)
        .map_err(|e| format!("invalid user ID: {e}"))?;
    let action = parse_action(&params.action)?;

    let policy = state.rbac_policy.read().await;
    let allowed = policy.check_permission(&user_id, &params.resource, action);

    // Record audit
    let mut audit = state.audit_store.write().await;
    audit.record(AuditEntry {
        principal: params.user_id.clone(),
        action: params.action.clone(),
        resource: params.resource.clone(),
        result: if allowed { "allowed" } else { "denied" }.to_string(),
        timestamp: chrono::Utc::now(),
    });

    Ok(json!({
        "userId": params.user_id,
        "action": params.action,
        "resource": params.resource,
        "allowed": allowed,
    }))
}

// ─────────────────────────────────────────────────────────────
// Audit Commands
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AuditQueryParams {
    principal: Option<String>,
    action: Option<String>,
    #[serde(rename = "rangeMinutes")]
    range_minutes: Option<i64>,
}

async fn handle_audit_query(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: AuditQueryParams = serde_json::from_value(params)
        .unwrap_or(AuditQueryParams {
            principal: None,
            action: None,
            range_minutes: None,
        });

    let store = state.audit_store.read().await;
    let entries = store.query(
        params.principal.as_deref(),
        params.action.as_deref(),
        params.range_minutes,
    );

    let events: Vec<Value> = entries
        .iter()
        .map(|e| {
            json!({
                "principal": e.principal,
                "action": e.action,
                "resource": e.resource,
                "result": e.result,
                "timestamp": e.timestamp.to_rfc3339(),
            })
        })
        .collect();

    Ok(json!({
        "count": events.len(),
        "events": events,
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
    async fn test_create_key_and_list() {
        let state = test_state();

        let result = handle_auth_command(
            &state,
            CommandRequest {
                command: "auth.create_key".to_string(),
                params: json!({
                    "name": "test-key",
                    "scopes": ["workloads:*"],
                }),
            },
        )
        .await
        .expect("create");
        assert_eq!(result["success"], true);
        assert!(!result["secret"].as_str().unwrap().is_empty());

        let result = handle_auth_command(
            &state,
            CommandRequest {
                command: "auth.list_keys".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list");
        assert_eq!(result["count"], 1);
    }

    #[tokio::test]
    async fn test_revoke_key() {
        let state = test_state();

        let create_result = handle_auth_command(
            &state,
            CommandRequest {
                command: "auth.create_key".to_string(),
                params: json!({"name": "revoke-me", "scopes": []}),
            },
        )
        .await
        .expect("create");

        let key_id = create_result["keyId"].as_str().unwrap();

        let result = handle_auth_command(
            &state,
            CommandRequest {
                command: "auth.revoke_key".to_string(),
                params: json!({"keyId": key_id, "reason": "no longer needed"}),
            },
        )
        .await
        .expect("revoke");
        assert_eq!(result["revoked"], true);

        // After revocation, key is still in store (just marked revoked)
        let result = handle_auth_command(
            &state,
            CommandRequest {
                command: "auth.list_keys".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list");
        // Key still counted (but is revoked)
        assert_eq!(result["count"], 1);
    }

    #[tokio::test]
    async fn test_rbac_create_role_and_check() {
        let state = test_state();

        // Create a role
        handle_auth_command(
            &state,
            CommandRequest {
                command: "rbac.create_role".to_string(),
                params: json!({
                    "name": "gpu-operator",
                    "description": "Can manage GPU workloads",
                    "permissions": [
                        {"resource": "workloads", "action": "create"},
                        {"resource": "workloads", "action": "read"},
                    ],
                }),
            },
        )
        .await
        .expect("create role");

        // Create a user and bind
        let user_id = claw_auth::UserId::new().to_string();

        handle_auth_command(
            &state,
            CommandRequest {
                command: "rbac.bind".to_string(),
                params: json!({"userId": user_id, "role": "gpu-operator"}),
            },
        )
        .await
        .expect("bind");

        // Check permission (allowed)
        let result = handle_auth_command(
            &state,
            CommandRequest {
                command: "rbac.check".to_string(),
                params: json!({"userId": user_id, "action": "create", "resource": "workloads"}),
            },
        )
        .await
        .expect("check");
        assert_eq!(result["allowed"], true);

        // Check permission (denied)
        let result = handle_auth_command(
            &state,
            CommandRequest {
                command: "rbac.check".to_string(),
                params: json!({"userId": user_id, "action": "delete", "resource": "workloads"}),
            },
        )
        .await
        .expect("check");
        assert_eq!(result["allowed"], false);
    }

    #[tokio::test]
    async fn test_audit_query() {
        let state = test_state();

        // Create a key (generates audit entries)
        handle_auth_command(
            &state,
            CommandRequest {
                command: "auth.create_key".to_string(),
                params: json!({"name": "audit-test", "scopes": []}),
            },
        )
        .await
        .expect("create");

        let result = handle_auth_command(
            &state,
            CommandRequest {
                command: "audit.query".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("query");
        assert!(result["count"].as_u64().unwrap() >= 1);
    }
}
