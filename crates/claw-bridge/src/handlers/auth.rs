//! Authentication and RBAC handlers
//!
//! These handlers integrate with claw-auth for user management,
//! role-based access control, and API key management.

use std::sync::Arc;

use chrono::{Duration, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use claw_auth::{
    Action, ApiKey, ApiKeyStore, Permission, RbacPolicy, Role, Scope, User, UserId,
};

use crate::error::{BridgeError, BridgeResult};
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Global State
// ─────────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref RBAC_POLICY: Arc<RwLock<RbacPolicy>> = Arc::new(RwLock::new(RbacPolicy::with_default_roles()));
    static ref API_KEY_STORE: Arc<RwLock<ApiKeyStore>> = Arc::new(RwLock::new(ApiKeyStore::new()));
}

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub name: String,
    pub roles: Vec<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoleInfo {
    pub name: String,
    pub description: Option<String>,
    pub permissions: Vec<PermissionInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PermissionInfo {
    pub resource: String,
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyInfo {
    pub id: String,
    pub name: String,
    pub user_id: String,
    pub scopes: Vec<String>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub last_used_at: Option<i64>,
}

// ─────────────────────────────────────────────────────────────
// User Management
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct UserCreateParams {
    pub name: String,
    pub roles: Option<Vec<String>>,
}

/// Create a new user
pub async fn user_create(params: Value) -> BridgeResult<Value> {
    let params: UserCreateParams = parse_params(params)?;

    let user = User::new(&params.name)
        .map_err(|e| BridgeError::InvalidParams(format!("invalid user name: {e}")))?;

    let user_id = user.id.clone();
    let mut policy = RBAC_POLICY.write();
    policy.add_user(user);

    // Assign roles if specified
    if let Some(roles) = &params.roles {
        for role_name in roles {
            policy
                .assign_role(&user_id, role_name)
                .map_err(|e| BridgeError::InvalidParams(format!("failed to assign role: {e}")))?;
        }
    }

    let user = policy
        .get_user(&user_id)
        .ok_or_else(|| BridgeError::Internal("user disappeared".to_string()))?;

    let info = user_to_info(user);
    tracing::info!(user_id = %user_id, name = %params.name, "user created");
    to_json(info)
}

#[derive(Debug, Deserialize)]
pub struct UserGetParams {
    pub user_id: String,
}

/// Get user details
pub async fn user_get(params: Value) -> BridgeResult<Value> {
    let params: UserGetParams = parse_params(params)?;

    let user_id = UserId::from_string(&params.user_id)
        .map_err(|_| BridgeError::InvalidParams(format!("invalid user_id: {}", params.user_id)))?;

    let policy = RBAC_POLICY.read();
    let user = policy
        .get_user(&user_id)
        .ok_or_else(|| BridgeError::NotFound(format!("user {} not found", params.user_id)))?;

    to_json(user_to_info(user))
}

#[derive(Debug, Deserialize)]
pub struct UserListParams {
    pub role: Option<String>,
}

/// List users
pub async fn user_list(params: Value) -> BridgeResult<Value> {
    let params: UserListParams = parse_params(params)?;
    let policy = RBAC_POLICY.read();

    let users: Vec<UserInfo> = policy
        .list_users()
        .into_iter()
        .filter(|u| {
            if let Some(role) = &params.role {
                u.roles.contains(role)
            } else {
                true
            }
        })
        .map(user_to_info)
        .collect();

    to_json(users)
}

#[derive(Debug, Deserialize)]
pub struct UserDeleteParams {
    pub user_id: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteResult {
    pub success: bool,
}

/// Delete a user
pub async fn user_delete(params: Value) -> BridgeResult<Value> {
    let params: UserDeleteParams = parse_params(params)?;

    let user_id = UserId::from_string(&params.user_id)
        .map_err(|_| BridgeError::InvalidParams(format!("invalid user_id: {}", params.user_id)))?;

    let mut policy = RBAC_POLICY.write();
    policy
        .remove_user(&user_id)
        .ok_or_else(|| BridgeError::NotFound(format!("user {} not found", params.user_id)))?;

    tracing::info!(user_id = %params.user_id, "user deleted");
    to_json(DeleteResult { success: true })
}

// ─────────────────────────────────────────────────────────────
// Role Management
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RoleAssignParams {
    pub user_id: String,
    pub role: String,
}

/// Assign a role to a user
pub async fn role_assign(params: Value) -> BridgeResult<Value> {
    let params: RoleAssignParams = parse_params(params)?;

    let user_id = UserId::from_string(&params.user_id)
        .map_err(|_| BridgeError::InvalidParams(format!("invalid user_id: {}", params.user_id)))?;

    let mut policy = RBAC_POLICY.write();
    policy
        .assign_role(&user_id, &params.role)
        .map_err(|e| BridgeError::InvalidParams(format!("failed to assign role: {e}")))?;

    tracing::info!(user_id = %params.user_id, role = %params.role, "role assigned");
    to_json(DeleteResult { success: true })
}

#[derive(Debug, Deserialize)]
pub struct RoleRevokeParams {
    pub user_id: String,
    pub role: String,
}

/// Revoke a role from a user
pub async fn role_revoke(params: Value) -> BridgeResult<Value> {
    let params: RoleRevokeParams = parse_params(params)?;

    let user_id = UserId::from_string(&params.user_id)
        .map_err(|_| BridgeError::InvalidParams(format!("invalid user_id: {}", params.user_id)))?;

    let mut policy = RBAC_POLICY.write();

    // Get user and remove role manually
    let user = policy
        .get_user_mut(&user_id)
        .ok_or_else(|| BridgeError::NotFound(format!("user {} not found", params.user_id)))?;

    user.remove_role(&params.role);

    tracing::info!(user_id = %params.user_id, role = %params.role, "role revoked");
    to_json(DeleteResult { success: true })
}

#[derive(Debug, Deserialize)]
pub struct RoleListParams {}

/// List all roles
pub async fn role_list(_params: Value) -> BridgeResult<Value> {
    let policy = RBAC_POLICY.read();

    let roles: Vec<RoleInfo> = policy
        .list_roles()
        .into_iter()
        .map(role_to_info)
        .collect();

    to_json(roles)
}

// ─────────────────────────────────────────────────────────────
// Permission Checks
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PermissionCheckParams {
    pub user_id: String,
    pub resource: String,
    pub action: String,
}

#[derive(Debug, Serialize)]
pub struct PermissionCheckResult {
    pub allowed: bool,
    pub reason: Option<String>,
}

/// Check if a user has permission for an action
pub async fn permission_check(params: Value) -> BridgeResult<Value> {
    let params: PermissionCheckParams = parse_params(params)?;

    let user_id = UserId::from_string(&params.user_id)
        .map_err(|_| BridgeError::InvalidParams(format!("invalid user_id: {}", params.user_id)))?;

    let action = parse_action(&params.action)?;

    let policy = RBAC_POLICY.read();
    let allowed = policy.check_permission(&user_id, &params.resource, action);

    to_json(PermissionCheckResult {
        allowed,
        reason: if allowed {
            None
        } else {
            Some(format!(
                "user {} lacks {} permission on {}",
                params.user_id, params.action, params.resource
            ))
        },
    })
}

// ─────────────────────────────────────────────────────────────
// API Key Management
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ApiKeyGenerateParams {
    pub name: String,
    pub user_id: String,
    pub scopes: Option<Vec<String>>,
    pub expires_in_days: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ApiKeyGenerateResult {
    pub key_id: String,
    pub secret: String,
    pub info: ApiKeyInfo,
}

/// Generate a new API key
pub async fn api_key_generate(params: Value) -> BridgeResult<Value> {
    let params: ApiKeyGenerateParams = parse_params(params)?;

    let user_id = UserId::from_string(&params.user_id)
        .map_err(|_| BridgeError::InvalidParams(format!("invalid user_id: {}", params.user_id)))?;

    let (mut key, secret) = ApiKey::generate(&params.name, user_id);

    // Add scopes if specified
    if let Some(scopes) = &params.scopes {
        for scope_str in scopes {
            if let Ok(scope) = Scope::new(scope_str) {
                key.add_scope(scope);
            }
        }
    }

    // Set expiration if specified
    if let Some(days) = params.expires_in_days {
        let expires_at = Utc::now() + Duration::days(days as i64);
        key = key.with_expiry(expires_at);
    }

    let key_id = key.id.as_str().to_string();
    let info = api_key_to_info(&key);

    let mut store = API_KEY_STORE.write();
    store.store(key);

    tracing::info!(key_id = %key_id, name = %params.name, "API key generated");

    to_json(ApiKeyGenerateResult {
        key_id,
        secret: secret.as_str().to_string(),
        info,
    })
}

#[derive(Debug, Deserialize)]
pub struct ApiKeyListParams {
    pub user_id: Option<String>,
}

/// List API keys
pub async fn api_key_list(params: Value) -> BridgeResult<Value> {
    let params: ApiKeyListParams = parse_params(params)?;

    let store = API_KEY_STORE.read();

    let keys: Vec<ApiKeyInfo> = if let Some(id_str) = &params.user_id {
        let user_id = UserId::from_string(id_str)
            .map_err(|_| BridgeError::InvalidParams(format!("invalid user_id: {}", id_str)))?;
        store.list_for_user(&user_id).into_iter().map(api_key_to_info).collect()
    } else {
        // List all would require iterating - for now return empty if no user specified
        vec![]
    };

    to_json(keys)
}

#[derive(Debug, Deserialize)]
pub struct ApiKeyRevokeParams {
    pub key_id: String,
}

/// Revoke an API key
pub async fn api_key_revoke(params: Value) -> BridgeResult<Value> {
    let params: ApiKeyRevokeParams = parse_params(params)?;

    let key_id = claw_auth::ApiKeyId::from_string(&params.key_id)
        .map_err(|_| BridgeError::InvalidParams(format!("invalid key_id: {}", params.key_id)))?;

    let mut store = API_KEY_STORE.write();
    store
        .remove(&key_id)
        .ok_or_else(|| BridgeError::NotFound(format!("key {} not found", params.key_id)))?;

    tracing::info!(key_id = %params.key_id, "API key revoked");
    to_json(DeleteResult { success: true })
}

// ─────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────

fn parse_action(s: &str) -> BridgeResult<Action> {
    match s.to_lowercase().as_str() {
        "create" => Ok(Action::Create),
        "read" => Ok(Action::Read),
        "update" => Ok(Action::Update),
        "delete" => Ok(Action::Delete),
        "list" => Ok(Action::List),
        "execute" => Ok(Action::Execute),
        "*" | "all" | "admin" => Ok(Action::Admin),
        _ => Err(BridgeError::InvalidParams(format!("unknown action: {s}"))),
    }
}

fn user_to_info(user: &User) -> UserInfo {
    UserInfo {
        id: user.id.to_string(),
        name: user.name.clone(),
        roles: user.roles.clone(),
        created_at: user.created_at.timestamp_millis(),
    }
}

fn role_to_info(role: &Role) -> RoleInfo {
    let permissions: Vec<PermissionInfo> = role
        .permissions
        .iter()
        .map(|p| PermissionInfo {
            resource: p.resource.clone(),
            actions: vec![format!("{:?}", p.action).to_lowercase()],
        })
        .collect();

    RoleInfo {
        name: role.name.clone(),
        description: Some(role.description.clone()),
        permissions,
    }
}

fn api_key_to_info(key: &ApiKey) -> ApiKeyInfo {
    ApiKeyInfo {
        id: key.id.as_str().to_string(),
        name: key.name.clone(),
        user_id: key.user_id.to_string(),
        scopes: key
            .scopes
            .iter()
            .map(|s| s.as_str().to_string())
            .collect(),
        created_at: key.created_at.timestamp_millis(),
        expires_at: key.expires_at.map(|t| t.timestamp_millis()),
        last_used_at: key.last_used_at.map(|t| t.timestamp_millis()),
    }
}
