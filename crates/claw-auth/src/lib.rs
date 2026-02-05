//! # Claw Auth
#![forbid(unsafe_code)]
//!
//! Authentication and Role-Based Access Control (RBAC) for Clawbernetes.
//!
//! This crate provides:
//!
//! - **API Key Authentication**: Generate, validate, and manage API keys
//! - **JWT Token Support**: Create and validate JWT tokens for sessions
//! - **RBAC**: Role-based access control with permissions and policies
//! - **CLI Protocol Integration**: HTTP header utilities for auth
//!
//! ## Quick Start
//!
//! ```rust
//! use claw_auth::{
//!     ApiKey, ApiKeyStore, JwtConfig, JwtManager,
//!     RbacPolicy, AuthContext, User, UserId, Role, Permission, Action, Scope,
//! };
//!
//! // Create RBAC policy with default roles
//! let mut policy = RbacPolicy::with_default_roles();
//!
//! // Create a user
//! let user = User::new("alice").expect("valid name");
//! let user_id = user.id.clone();
//! policy.add_user(user);
//!
//! // Assign a role
//! policy.assign_role(&user_id, "operator").expect("role exists");
//!
//! // Check permissions
//! assert!(policy.check_permission(&user_id, "workloads", Action::Create));
//! assert!(policy.check_permission(&user_id, "nodes", Action::Read));
//! assert!(!policy.check_permission(&user_id, "nodes", Action::Delete));
//! ```
//!
//! ## API Key Authentication
//!
//! ```rust
//! use claw_auth::{ApiKey, ApiKeyStore, UserId, Scope, Action, authenticate_api_key};
//!
//! // Create an API key store
//! let mut store = ApiKeyStore::new();
//!
//! // Generate an API key for a user
//! let user_id = UserId::new();
//! let (mut key, secret) = ApiKey::generate("My API Key", user_id);
//!
//! // Add scopes
//! key.add_scope(Scope::new("workloads:*").expect("valid scope"));
//!
//! // Store the key (only the hash is stored)
//! store.store(key);
//!
//! // Authenticate with the secret
//! let authenticated = authenticate_api_key(&store, secret.as_str())
//!     .expect("valid key");
//! assert!(authenticated.allows("workloads", Action::Create));
//! ```
//!
//! ## JWT Tokens
//!
//! ```rust
//! use claw_auth::{JwtConfig, JwtManager, JwtClaims, UserId};
//! use chrono::Duration;
//!
//! // Create JWT configuration (use a real secret in production!)
//! let secret = [0u8; 32]; // Use a real secret!
//! let config = JwtConfig::new_hs256(secret, "clawbernetes")
//!     .expect("valid config");
//! let manager = JwtManager::new(config);
//!
//! // Create a token for a user
//! let user_id = UserId::new();
//! let token = manager.create_token(&user_id).expect("creates token");
//!
//! // Validate and extract claims
//! let claims = manager.validate_token(&token).expect("valid token");
//! assert_eq!(claims.sub, user_id.to_string());
//! ```
//!
//! ## Auth Context
//!
//! ```rust
//! use claw_auth::{AuthContext, RbacPolicy, User, UserId, Action, context_for_user};
//!
//! let mut policy = RbacPolicy::with_default_roles();
//!
//! // Create an admin user
//! let admin = User::new("admin").expect("valid name");
//! let admin_id = admin.id.clone();
//! policy.add_user(admin);
//! policy.assign_role(&admin_id, "admin").expect("role exists");
//!
//! // Create an auth context for the user
//! let ctx = context_for_user(&policy, &admin_id).expect("user exists");
//!
//! // Check permissions through the context
//! assert!(ctx.is_authenticated());
//! assert!(ctx.has_role("admin"));
//!
//! // Use require_permission for access control
//! ctx.require_permission(&policy, "nodes", Action::Delete)
//!     .expect("admin can delete nodes");
//! ```
//!
//! ## Security Considerations
//!
//! - API key secrets are hashed using BLAKE3 before storage
//! - Constant-time comparison is used for key verification
//! - JWT secrets should be at least 32 bytes for HS256
//! - Sensitive data is zeroized on drop where appropriate
//! - Debug output redacts secrets and hashes

pub mod apikey;
pub mod error;
pub mod headers;
pub mod jwt;
pub mod rbac;
pub mod types;

// Re-export commonly used types
pub use error::{Error, Result};

// Types
pub use types::{Action, Permission, Role, Scope, User, UserId};

// API Keys
pub use apikey::{
    authenticate_api_key, extract_api_key_from_header, ApiKey, ApiKeyHash, ApiKeyId,
    ApiKeySecret, ApiKeyStore,
};

// JWT
pub use jwt::{extract_jwt_from_header, JwtClaims, JwtConfig, JwtManager};

// RBAC
pub use rbac::{context_for_user, AuthContext, RbacPolicy};

// Headers/CLI integration
pub use headers::{
    authenticate_request, authenticate_request_optional, extract_credentials,
    AuthMiddlewareConfig, AuthResult, Credential, AUTHORIZATION_HEADER, X_API_KEY_HEADER,
};

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn full_workflow_api_key() {
        // Setup RBAC policy
        let mut policy = RbacPolicy::with_default_roles();

        // Create a user
        let user = User::new("api-user").ok().unwrap();
        let user_id = user.id.clone();
        policy.add_user(user);
        policy.assign_role(&user_id, "operator").ok();

        // Create API key store
        let mut store = ApiKeyStore::new();

        // Generate an API key
        let (mut key, secret) = ApiKey::generate("Automation Key", user_id.clone());
        key.add_scope(Scope::new("workloads:*").ok().unwrap());
        key.add_scope(Scope::new("nodes:read").ok().unwrap());
        store.store(key);

        // Authenticate
        let authenticated = authenticate_api_key(&store, secret.as_str()).ok().unwrap();
        assert_eq!(authenticated.user_id, user_id);
        assert!(authenticated.allows("workloads", Action::Create));
        assert!(authenticated.allows("nodes", Action::Read));
        assert!(!authenticated.allows("nodes", Action::Delete));

        // Create auth context
        let user = policy.get_user(&user_id).unwrap();
        let ctx = AuthContext::authenticated(user_id.clone(), user.roles.clone())
            .with_scopes(authenticated.scopes.clone());

        // Check permissions
        assert!(ctx.require_scope("workloads", Action::Create).is_ok());
        assert!(ctx.require_permission(&policy, "workloads", Action::Create).is_ok());
        // Scope allows nodes:read
        assert!(ctx.require_scope("nodes", Action::Read).is_ok());
        // But not nodes:delete
        assert!(ctx.require_scope("nodes", Action::Delete).is_err());
    }

    #[test]
    fn full_workflow_jwt() {
        // Setup RBAC policy
        let mut policy = RbacPolicy::with_default_roles();

        // Create a user
        let user = User::new("jwt-user").ok().unwrap();
        let user_id = user.id.clone();
        policy.add_user(user);
        policy.assign_role(&user_id, "admin").ok();

        // Setup JWT
        let secret = [42u8; 32];
        let config = JwtConfig::new_hs256(secret, "test-issuer")
            .ok()
            .unwrap()
            .with_default_expiry(Duration::hours(1));
        let manager = JwtManager::new(config);

        // Create token with custom claims
        let claims = JwtClaims::new(&user_id, "test-issuer")
            .with_roles(vec!["admin".to_string()])
            .with_scopes(vec![Scope::new("*").ok().unwrap()])
            .with_session_id("session-123");
        let token = manager.create_token_with_claims(&claims).ok().unwrap();

        // Validate token
        let validated = manager.validate_token(&token).ok().unwrap();
        assert_eq!(validated.sub, user_id.to_string());
        assert_eq!(validated.roles, vec!["admin"]);

        // Create auth context from claims
        let scopes = validated.parsed_scopes().ok().unwrap();
        let ctx = AuthContext::authenticated(user_id.clone(), validated.roles.clone())
            .with_scopes(scopes);

        // Admin has all permissions
        assert!(ctx.require_permission(&policy, "anything", Action::Admin).is_ok());
    }

    #[test]
    fn full_workflow_rbac_only() {
        // Setup RBAC policy with custom role
        let mut policy = RbacPolicy::with_default_roles();

        // Create a custom role
        let mut viewer_role = Role::new("viewer", "Can view workloads").ok().unwrap();
        viewer_role.add_permission(Permission::new("workloads", Action::Read).ok().unwrap());
        viewer_role.add_permission(Permission::new("workloads", Action::List).ok().unwrap());
        policy.add_role(viewer_role);

        // Create users with different roles
        let admin = User::new("admin").ok().unwrap();
        let admin_id = admin.id.clone();
        policy.add_user(admin);
        policy.assign_role(&admin_id, "admin").ok();

        let viewer = User::new("viewer").ok().unwrap();
        let viewer_id = viewer.id.clone();
        policy.add_user(viewer);
        policy.assign_role(&viewer_id, "viewer").ok();

        // Admin can do everything
        assert!(policy.check_permission(&admin_id, "workloads", Action::Create));
        assert!(policy.check_permission(&admin_id, "workloads", Action::Delete));
        assert!(policy.check_permission(&admin_id, "nodes", Action::Admin));

        // Viewer can only view workloads
        assert!(policy.check_permission(&viewer_id, "workloads", Action::Read));
        assert!(policy.check_permission(&viewer_id, "workloads", Action::List));
        assert!(!policy.check_permission(&viewer_id, "workloads", Action::Create));
        assert!(!policy.check_permission(&viewer_id, "workloads", Action::Delete));
        assert!(!policy.check_permission(&viewer_id, "nodes", Action::Read));
    }

    #[test]
    fn full_workflow_header_auth() {
        // Setup everything
        let mut policy = RbacPolicy::with_default_roles();
        let user = User::new("header-user").ok().unwrap();
        let user_id = user.id.clone();
        policy.add_user(user);
        policy.assign_role(&user_id, "operator").ok();

        let mut api_key_store = ApiKeyStore::new();
        let (key, secret) = ApiKey::generate("Header Key", user_id.clone());
        let key = key.with_scopes(vec![Scope::new("workloads:*").ok().unwrap()]);
        api_key_store.store(key);

        let jwt_config = JwtConfig::new_hs256([0u8; 32], "test").ok().unwrap();
        let jwt_manager = JwtManager::new(jwt_config);

        // Test X-API-Key header
        let result = authenticate_request(
            None,
            Some(secret.as_str()),
            &api_key_store,
            &jwt_manager,
            &policy,
        );
        assert!(result.is_ok());
        let result = result.ok().unwrap();
        assert!(result.context.is_authenticated());
        assert!(result.context.has_role("operator"));

        // Test Bearer API key
        let bearer = format!("Bearer {}", secret.as_str());
        let result = authenticate_request(
            Some(&bearer),
            None,
            &api_key_store,
            &jwt_manager,
            &policy,
        );
        assert!(result.is_ok());

        // Test JWT token
        let token = jwt_manager.create_token(&user_id).ok().unwrap();
        let bearer = format!("Bearer {token}");
        let result = authenticate_request(
            Some(&bearer),
            None,
            &api_key_store,
            &jwt_manager,
            &policy,
        );
        assert!(result.is_ok());
        let result = result.ok().unwrap();
        assert!(result.claims.is_some());

        // Test anonymous (optional auth)
        let result = authenticate_request_optional(
            None,
            None,
            &api_key_store,
            &jwt_manager,
            &policy,
        );
        assert!(!result.context.is_authenticated());
    }

    #[test]
    fn api_key_lifecycle() {
        let mut store = ApiKeyStore::new();
        let user_id = UserId::new();

        // Generate key
        let (key, secret) = ApiKey::generate("Test Key", user_id.clone());
        let key_id = key.id.clone();
        store.store(key);

        // Key works
        assert!(store.find_by_secret(secret.as_str()).is_some());

        // Revoke key
        let key = store.get_mut(&key_id).unwrap();
        key.revoke("No longer needed");

        // Key no longer valid
        let key = store.get(&key_id).unwrap();
        assert!(!key.is_valid());

        // Cleanup removes invalid keys
        store.cleanup();
        assert!(store.is_empty());
    }

    #[test]
    fn jwt_token_lifecycle() {
        let secret = [1u8; 32];
        let config = JwtConfig::new_hs256(secret, "lifecycle-test")
            .ok()
            .unwrap();
        let manager = JwtManager::new(config);

        let user_id = UserId::new();

        // Create token
        let token = manager.create_token(&user_id).ok().unwrap();
        assert!(!token.is_empty());

        // Validate token
        let claims = manager.validate_token(&token).ok().unwrap();
        assert_eq!(claims.sub, user_id.to_string());
        assert!(!claims.is_expired());

        // Refresh token
        let refreshed = manager.refresh_token(&token).ok().unwrap();
        assert_ne!(token, refreshed);

        // Refreshed token is valid
        let claims = manager.validate_token(&refreshed).ok().unwrap();
        assert_eq!(claims.sub, user_id.to_string());
    }

    #[test]
    fn permission_hierarchy() {
        // Test that admin permission implies all actions
        let mut role = Role::new("test", "Test").ok().unwrap();
        role.add_permission(Permission::admin("workloads"));

        assert!(role.has_permission("workloads", Action::Create));
        assert!(role.has_permission("workloads", Action::Read));
        assert!(role.has_permission("workloads", Action::Update));
        assert!(role.has_permission("workloads", Action::Delete));
        assert!(role.has_permission("workloads", Action::List));
        assert!(role.has_permission("workloads", Action::Execute));
        assert!(role.has_permission("workloads", Action::Admin));

        // But not other resources
        assert!(!role.has_permission("nodes", Action::Read));
    }

    #[test]
    fn scope_hierarchy() {
        // Test scope patterns
        let wildcard = Scope::new("*").ok().unwrap();
        assert!(wildcard.allows("anything", Action::Admin));

        let resource_wildcard = Scope::new("workloads:*").ok().unwrap();
        assert!(resource_wildcard.allows("workloads", Action::Create));
        assert!(resource_wildcard.allows("workloads", Action::Delete));
        assert!(!resource_wildcard.allows("nodes", Action::Read));

        let specific = Scope::new("workloads:read").ok().unwrap();
        assert!(specific.allows("workloads", Action::Read));
        assert!(!specific.allows("workloads", Action::Create));

        // Child resources
        assert!(specific.allows("workloads:logs", Action::Read));
    }
}
