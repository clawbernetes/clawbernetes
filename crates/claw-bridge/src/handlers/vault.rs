//! AI-Native Vault - Reasoning Engine Guarded Secrets
//!
//! This vault has NO external API. Secrets can only be accessed through
//! the AI agent (reasoning engine), which decides whether access is appropriate
//! based on context, session, and purpose.
//!
//! Key principles:
//! 1. No REST/GraphQL/gRPC endpoints - only tool invocations
//! 2. Access requires justification (reason parameter)
//! 3. All access is logged with session context
//! 4. Agent can deny access based on context
//! 5. Secrets can have access policies evaluated by the agent

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{BridgeError, BridgeResult};
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Global State
// ─────────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    /// AI-native vault storage
    static ref VAULT: RwLock<VaultStore> = RwLock::new(VaultStore::new());
}

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

/// A secret stored in the vault
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultSecret {
    /// Unique identifier
    pub id: String,
    
    /// Encrypted/stored value (in production, would be encrypted at rest)
    #[serde(skip_serializing)]
    pub value: String,
    
    /// Human-readable description
    pub description: Option<String>,
    
    /// Secret type for agent context
    pub secret_type: SecretType,
    
    /// Access policy - evaluated by the agent
    pub access_policy: AccessPolicy,
    
    /// Creation timestamp
    pub created_at: i64,
    
    /// Last accessed timestamp
    pub last_accessed_at: Option<i64>,
    
    /// Access count
    pub access_count: u64,
    
    /// Labels for categorization
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecretType {
    /// SSH private key
    SshKey,
    /// API token/key
    ApiKey,
    /// Password/credential
    Password,
    /// TLS certificate/key
    Certificate,
    /// Generic secret
    Generic,
    /// Bootstrap token (one-time use)
    BootstrapToken,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessPolicy {
    /// Allowed purposes (e.g., "node-bootstrap", "workload-secret")
    pub allowed_purposes: Vec<String>,
    
    /// Require explicit user confirmation
    pub require_confirmation: bool,
    
    /// Maximum access count (None = unlimited)
    pub max_access_count: Option<u64>,
    
    /// Expire after timestamp (None = never)
    pub expires_at: Option<i64>,
    
    /// Allowed session types
    pub allowed_sessions: Vec<String>,
    
    /// Custom policy description for agent to evaluate
    pub custom_policy: Option<String>,
}

impl Default for AccessPolicy {
    fn default() -> Self {
        Self {
            allowed_purposes: vec!["*".to_string()],
            require_confirmation: false,
            max_access_count: None,
            expires_at: None,
            allowed_sessions: vec!["*".to_string()],
            custom_policy: None,
        }
    }
}

/// Access log entry
#[derive(Debug, Clone, Serialize)]
pub struct VaultAccessLog {
    pub secret_id: String,
    pub timestamp: i64,
    pub session_id: Option<String>,
    pub purpose: String,
    pub reason: String,
    pub granted: bool,
    pub denial_reason: Option<String>,
}

/// The vault store
struct VaultStore {
    secrets: HashMap<String, VaultSecret>,
    access_logs: Vec<VaultAccessLog>,
}

impl VaultStore {
    fn new() -> Self {
        Self {
            secrets: HashMap::new(),
            access_logs: Vec::new(),
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Vault Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct VaultStoreParams {
    pub id: String,
    pub value: String,
    pub description: Option<String>,
    pub secret_type: Option<String>,
    pub allowed_purposes: Option<Vec<String>>,
    pub require_confirmation: Option<bool>,
    pub max_access_count: Option<u64>,
    pub expires_in_minutes: Option<u64>,
    pub labels: Option<HashMap<String, String>>,
    pub custom_policy: Option<String>,
}

/// Store a secret in the AI-native vault
pub async fn vault_store(params: Value) -> BridgeResult<Value> {
    let params: VaultStoreParams = parse_params(params)?;
    
    let now = chrono::Utc::now().timestamp_millis();
    
    let secret_type = match params.secret_type.as_deref() {
        Some("ssh_key") => SecretType::SshKey,
        Some("api_key") => SecretType::ApiKey,
        Some("password") => SecretType::Password,
        Some("certificate") => SecretType::Certificate,
        Some("bootstrap_token") => SecretType::BootstrapToken,
        _ => SecretType::Generic,
    };
    
    let expires_at = params.expires_in_minutes.map(|m| {
        now + (m as i64 * 60 * 1000)
    });
    
    let secret = VaultSecret {
        id: params.id.clone(),
        value: params.value,
        description: params.description,
        secret_type,
        access_policy: AccessPolicy {
            allowed_purposes: params.allowed_purposes.unwrap_or_else(|| vec!["*".to_string()]),
            require_confirmation: params.require_confirmation.unwrap_or(false),
            max_access_count: params.max_access_count,
            expires_at,
            allowed_sessions: vec!["*".to_string()],
            custom_policy: params.custom_policy,
        },
        created_at: now,
        last_accessed_at: None,
        access_count: 0,
        labels: params.labels.unwrap_or_default(),
    };
    
    let mut vault = VAULT.write();
    vault.secrets.insert(params.id.clone(), secret);
    
    tracing::info!(id = %params.id, "stored secret in AI-native vault");
    
    to_json(serde_json::json!({
        "id": params.id,
        "stored": true,
        "message": "Secret stored in AI-native vault. Access requires agent justification."
    }))
}

#[derive(Debug, Deserialize)]
pub struct VaultRetrieveParams {
    pub id: String,
    /// Why the agent needs this secret
    pub reason: String,
    /// Purpose/use case
    pub purpose: String,
    /// Session context
    pub session_id: Option<String>,
    /// Agent acknowledges policy
    pub policy_acknowledged: Option<bool>,
}

/// Retrieve a secret from the AI-native vault
/// 
/// This is the ONLY way to access secrets - through agent justification.
/// The agent must provide:
/// - reason: Why it needs this secret
/// - purpose: What it will be used for
/// - session_id: Current session context
pub async fn vault_retrieve(params: Value) -> BridgeResult<Value> {
    let params: VaultRetrieveParams = parse_params(params)?;
    
    let now = chrono::Utc::now().timestamp_millis();
    
    let mut vault = VAULT.write();
    
    // First, check if secret exists and gather read-only data
    let secret = vault.secrets.get(&params.id)
        .ok_or_else(|| BridgeError::NotFound(format!("secret not found: {}", params.id)))?;
    
    // Check expiration
    if let Some(expires_at) = secret.access_policy.expires_at {
        if now > expires_at {
            vault.access_logs.push(VaultAccessLog {
                secret_id: params.id.clone(),
                timestamp: now,
                session_id: params.session_id.clone(),
                purpose: params.purpose.clone(),
                reason: params.reason.clone(),
                granted: false,
                denial_reason: Some("Secret has expired".to_string()),
            });
            return Err(BridgeError::InvalidParams("secret has expired".to_string()));
        }
    }
    
    // Check max access count
    if let Some(max) = secret.access_policy.max_access_count {
        if secret.access_count >= max {
            vault.access_logs.push(VaultAccessLog {
                secret_id: params.id.clone(),
                timestamp: now,
                session_id: params.session_id.clone(),
                purpose: params.purpose.clone(),
                reason: params.reason.clone(),
                granted: false,
                denial_reason: Some("Maximum access count exceeded".to_string()),
            });
            return Err(BridgeError::InvalidParams("maximum access count exceeded".to_string()));
        }
    }
    
    // Check purpose
    let purpose_allowed = secret.access_policy.allowed_purposes.contains(&"*".to_string())
        || secret.access_policy.allowed_purposes.contains(&params.purpose);
    
    if !purpose_allowed {
        vault.access_logs.push(VaultAccessLog {
            secret_id: params.id.clone(),
            timestamp: now,
            session_id: params.session_id.clone(),
            purpose: params.purpose.clone(),
            reason: params.reason.clone(),
            granted: false,
            denial_reason: Some(format!("Purpose '{}' not allowed", params.purpose)),
        });
        return Err(BridgeError::InvalidParams(format!(
            "purpose '{}' not allowed for this secret", params.purpose
        )));
    }
    
    // Check confirmation requirement
    let requires_confirmation = secret.access_policy.require_confirmation;
    let custom_policy = secret.access_policy.custom_policy.clone();
    
    if requires_confirmation && !params.policy_acknowledged.unwrap_or(false) {
        vault.access_logs.push(VaultAccessLog {
            secret_id: params.id.clone(),
            timestamp: now,
            session_id: params.session_id.clone(),
            purpose: params.purpose.clone(),
            reason: params.reason.clone(),
            granted: false,
            denial_reason: Some("Confirmation required but not acknowledged".to_string()),
        });
        return to_json(serde_json::json!({
            "id": params.id,
            "requires_confirmation": true,
            "custom_policy": custom_policy,
            "message": "This secret requires explicit confirmation. Set policy_acknowledged=true to proceed."
        }));
    }
    
    // Gather data before mutable borrow
    let value = secret.value.clone();
    let description = secret.description.clone();
    
    // Now get mutable reference and update
    if let Some(secret) = vault.secrets.get_mut(&params.id) {
        secret.last_accessed_at = Some(now);
        secret.access_count += 1;
    }
    
    let access_count = vault.secrets.get(&params.id)
        .map(|s| s.access_count)
        .unwrap_or(1);
    
    // Log successful access
    vault.access_logs.push(VaultAccessLog {
        secret_id: params.id.clone(),
        timestamp: now,
        session_id: params.session_id,
        purpose: params.purpose,
        reason: params.reason,
        granted: true,
        denial_reason: None,
    });
    
    tracing::info!(id = %params.id, "vault secret accessed by agent");
    
    to_json(serde_json::json!({
        "id": params.id,
        "value": value,
        "description": description,
        "access_count": access_count,
    }))
}

#[derive(Debug, Deserialize)]
pub struct VaultListParams {
    pub include_expired: Option<bool>,
}

/// List secrets in the vault (metadata only, no values)
pub async fn vault_list(params: Value) -> BridgeResult<Value> {
    let params: VaultListParams = serde_json::from_value(params).unwrap_or(VaultListParams {
        include_expired: None,
    });
    
    let now = chrono::Utc::now().timestamp_millis();
    let vault = VAULT.read();
    
    let secrets: Vec<_> = vault.secrets.values()
        .filter(|s| {
            if params.include_expired.unwrap_or(false) {
                true
            } else if let Some(expires_at) = s.access_policy.expires_at {
                now < expires_at
            } else {
                true
            }
        })
        .map(|s| serde_json::json!({
            "id": s.id,
            "description": s.description,
            "secret_type": format!("{:?}", s.secret_type),
            "created_at": s.created_at,
            "last_accessed_at": s.last_accessed_at,
            "access_count": s.access_count,
            "labels": s.labels,
            "requires_confirmation": s.access_policy.require_confirmation,
            "allowed_purposes": s.access_policy.allowed_purposes,
            "expires_at": s.access_policy.expires_at,
        }))
        .collect();
    
    to_json(serde_json::json!({
        "count": secrets.len(),
        "secrets": secrets,
    }))
}

#[derive(Debug, Deserialize)]
pub struct VaultDeleteParams {
    pub id: String,
    pub reason: String,
}

/// Delete a secret from the vault
pub async fn vault_delete(params: Value) -> BridgeResult<Value> {
    let params: VaultDeleteParams = parse_params(params)?;
    
    let mut vault = VAULT.write();
    
    if vault.secrets.remove(&params.id).is_some() {
        tracing::info!(id = %params.id, reason = %params.reason, "deleted secret from vault");
        
        to_json(serde_json::json!({
            "id": params.id,
            "deleted": true,
        }))
    } else {
        Err(BridgeError::NotFound(format!("secret not found: {}", params.id)))
    }
}

#[derive(Debug, Deserialize)]
pub struct VaultAuditParams {
    pub secret_id: Option<String>,
    pub limit: Option<usize>,
}

/// Get access audit logs
pub async fn vault_audit(params: Value) -> BridgeResult<Value> {
    let params: VaultAuditParams = serde_json::from_value(params).unwrap_or(VaultAuditParams {
        secret_id: None,
        limit: None,
    });
    
    let vault = VAULT.read();
    let limit = params.limit.unwrap_or(100);
    
    let logs: Vec<_> = vault.access_logs.iter()
        .rev()
        .filter(|log| {
            params.secret_id.as_ref()
                .map(|id| &log.secret_id == id)
                .unwrap_or(true)
        })
        .take(limit)
        .cloned()
        .collect();
    
    to_json(serde_json::json!({
        "count": logs.len(),
        "logs": logs,
    }))
}
