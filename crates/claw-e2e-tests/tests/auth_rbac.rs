//! End-to-end tests for Authentication and RBAC (claw-auth).
//!
//! These tests verify:
//! 1. API key generation and validation
//! 2. JWT token creation, validation, and refresh
//! 3. RBAC permission checks across roles
//! 4. Scope-based access control
//! 5. Header-based authentication flow

use chrono::Duration;
use claw_auth::{
    authenticate_api_key, authenticate_request, authenticate_request_optional,
    context_for_user, ApiKey, ApiKeyStore, AuthContext, JwtClaims, JwtConfig, JwtManager,
    Permission, RbacPolicy, Role, Scope, User, UserId, Action,
};

// ============================================================================
// API Key Tests
// ============================================================================

#[test]
fn test_api_key_generation_and_validation() {
    let mut store = ApiKeyStore::new();
    let user_id = UserId::new();

    // Generate an API key
    let (key, secret) = ApiKey::generate("Test Key", user_id.clone());
    let key_id = key.id.clone();
    store.store(key);

    // Authenticate with the secret
    let authenticated = authenticate_api_key(&store, secret.as_str());
    assert!(authenticated.is_ok(), "Should authenticate with valid secret");
    let authenticated = authenticated.unwrap();
    assert_eq!(authenticated.user_id, user_id);

    // Invalid secret should fail
    let invalid = authenticate_api_key(&store, "invalid-secret-12345");
    assert!(invalid.is_err(), "Should reject invalid secret");

    // Can retrieve key by ID
    let retrieved = store.get(&key_id);
    assert!(retrieved.is_some());
}

#[test]
fn test_api_key_with_scopes() {
    let mut store = ApiKeyStore::new();
    let user_id = UserId::new();

    let (mut key, secret) = ApiKey::generate("Scoped Key", user_id);
    key.add_scope(Scope::new("workloads:*").unwrap());
    key.add_scope(Scope::new("nodes:read").unwrap());
    store.store(key);

    let authenticated = authenticate_api_key(&store, secret.as_str()).unwrap();

    // Should have workload permissions
    assert!(authenticated.allows("workloads", Action::Create));
    assert!(authenticated.allows("workloads", Action::Delete));
    assert!(authenticated.allows("workloads", Action::Read));

    // Should have only read access to nodes
    assert!(authenticated.allows("nodes", Action::Read));
    assert!(!authenticated.allows("nodes", Action::Create));
    assert!(!authenticated.allows("nodes", Action::Delete));
}

#[test]
fn test_api_key_revocation() {
    let mut store = ApiKeyStore::new();
    let user_id = UserId::new();

    let (key, secret) = ApiKey::generate("Revokable Key", user_id);
    let key_id = key.id.clone();
    store.store(key);

    // Key works initially
    assert!(authenticate_api_key(&store, secret.as_str()).is_ok());

    // Revoke the key
    let key = store.get_mut(&key_id).unwrap();
    key.revoke("No longer needed");

    // Key should be invalid now
    let key = store.get(&key_id).unwrap();
    assert!(!key.is_valid());

    // Cleanup removes revoked keys
    store.cleanup();
    assert!(store.is_empty());
}

#[test]
fn test_multiple_api_keys_per_user() {
    let mut store = ApiKeyStore::new();
    let user_id = UserId::new();

    // Create multiple keys
    let (key1, secret1) = ApiKey::generate("Key 1", user_id.clone());
    let (key2, secret2) = ApiKey::generate("Key 2", user_id.clone());
    let (key3, secret3) = ApiKey::generate("Key 3", user_id.clone());

    store.store(key1);
    store.store(key2);
    store.store(key3);

    // All keys should work
    assert!(authenticate_api_key(&store, secret1.as_str()).is_ok());
    assert!(authenticate_api_key(&store, secret2.as_str()).is_ok());
    assert!(authenticate_api_key(&store, secret3.as_str()).is_ok());

    // All should map to the same user
    let auth1 = authenticate_api_key(&store, secret1.as_str()).unwrap();
    let auth2 = authenticate_api_key(&store, secret2.as_str()).unwrap();
    let auth3 = authenticate_api_key(&store, secret3.as_str()).unwrap();

    assert_eq!(auth1.user_id, auth2.user_id);
    assert_eq!(auth2.user_id, auth3.user_id);
}

// ============================================================================
// JWT Token Tests
// ============================================================================

#[test]
fn test_jwt_token_creation_and_validation() {
    let secret = [42u8; 32];
    let config = JwtConfig::new_hs256(secret, "clawbernetes").unwrap();
    let manager = JwtManager::new(config);

    let user_id = UserId::new();

    // Create a token
    let token = manager.create_token(&user_id).unwrap();
    assert!(!token.is_empty());

    // Validate the token
    let claims = manager.validate_token(&token).unwrap();
    assert_eq!(claims.sub, user_id.to_string());
    assert!(!claims.is_expired());
}

#[test]
fn test_jwt_token_with_custom_claims() {
    let secret = [99u8; 32];
    let config = JwtConfig::new_hs256(secret, "test-issuer").unwrap();
    let manager = JwtManager::new(config);

    let user_id = UserId::new();

    // Create token with custom claims
    let claims = JwtClaims::new(&user_id, "test-issuer")
        .with_roles(vec!["admin".to_string(), "operator".to_string()])
        .with_scopes(vec![Scope::new("*").unwrap()])
        .with_session_id("session-abc123");

    let token = manager.create_token_with_claims(&claims).unwrap();
    let validated = manager.validate_token(&token).unwrap();

    assert_eq!(validated.roles, vec!["admin", "operator"]);
    assert!(validated.session_id.is_some());
    assert_eq!(validated.session_id.unwrap(), "session-abc123");
}

#[test]
fn test_jwt_token_refresh() {
    let secret = [1u8; 32];
    let config = JwtConfig::new_hs256(secret, "refresh-test")
        .unwrap()
        .with_default_expiry(Duration::hours(1));
    let manager = JwtManager::new(config);

    let user_id = UserId::new();
    let original_token = manager.create_token(&user_id).unwrap();

    // Refresh the token
    let refreshed_token = manager.refresh_token(&original_token).unwrap();
    assert_ne!(original_token, refreshed_token);

    // Refreshed token should be valid
    let claims = manager.validate_token(&refreshed_token).unwrap();
    assert_eq!(claims.sub, user_id.to_string());
}

#[test]
fn test_jwt_invalid_token_rejected() {
    let secret = [50u8; 32];
    let config = JwtConfig::new_hs256(secret, "test").unwrap();
    let manager = JwtManager::new(config);

    // Completely invalid token
    let result = manager.validate_token("not-a-valid-token");
    assert!(result.is_err());

    // Token signed with different secret
    let other_secret = [99u8; 32];
    let other_config = JwtConfig::new_hs256(other_secret, "test").unwrap();
    let other_manager = JwtManager::new(other_config);

    let user_id = UserId::new();
    let other_token = other_manager.create_token(&user_id).unwrap();

    // Should fail validation with our manager
    let result = manager.validate_token(&other_token);
    assert!(result.is_err());
}

// ============================================================================
// RBAC Permission Tests
// ============================================================================

#[test]
fn test_rbac_default_roles() {
    let policy = RbacPolicy::with_default_roles();

    // Create users with different roles
    let admin = User::new("admin-user").unwrap();
    let admin_id = admin.id.clone();
    let mut policy = policy;
    policy.add_user(admin);
    policy.assign_role(&admin_id, "admin").unwrap();

    let operator = User::new("operator-user").unwrap();
    let operator_id = operator.id.clone();
    policy.add_user(operator);
    policy.assign_role(&operator_id, "operator").unwrap();

    let viewer = User::new("viewer-user").unwrap();
    let viewer_id = viewer.id.clone();
    policy.add_user(viewer);
    policy.assign_role(&viewer_id, "viewer").unwrap();

    // Admin can do everything
    assert!(policy.check_permission(&admin_id, "workloads", Action::Create));
    assert!(policy.check_permission(&admin_id, "workloads", Action::Delete));
    assert!(policy.check_permission(&admin_id, "nodes", Action::Admin));
    assert!(policy.check_permission(&admin_id, "secrets", Action::Read));

    // Operator can manage workloads and read nodes
    assert!(policy.check_permission(&operator_id, "workloads", Action::Create));
    assert!(policy.check_permission(&operator_id, "workloads", Action::Update));
    assert!(policy.check_permission(&operator_id, "nodes", Action::Read));
    assert!(!policy.check_permission(&operator_id, "nodes", Action::Delete));

    // Viewer can only read
    assert!(policy.check_permission(&viewer_id, "workloads", Action::Read));
    assert!(policy.check_permission(&viewer_id, "workloads", Action::List));
    assert!(!policy.check_permission(&viewer_id, "workloads", Action::Create));
    assert!(!policy.check_permission(&viewer_id, "workloads", Action::Delete));
}

#[test]
fn test_rbac_custom_role() {
    let mut policy = RbacPolicy::with_default_roles();

    // Create a custom role for CI/CD automation
    let mut ci_role = Role::new("ci-deployer", "CI/CD deployment automation").unwrap();
    ci_role.add_permission(Permission::new("workloads", Action::Create).unwrap());
    ci_role.add_permission(Permission::new("workloads", Action::Update).unwrap());
    ci_role.add_permission(Permission::new("workloads", Action::Read).unwrap());
    ci_role.add_permission(Permission::new("deployments", Action::Create).unwrap());
    ci_role.add_permission(Permission::new("deployments", Action::Read).unwrap());
    policy.add_role(ci_role);

    // Create a user with the custom role
    let ci_user = User::new("github-actions").unwrap();
    let ci_user_id = ci_user.id.clone();
    policy.add_user(ci_user);
    policy.assign_role(&ci_user_id, "ci-deployer").unwrap();

    // Can deploy workloads
    assert!(policy.check_permission(&ci_user_id, "workloads", Action::Create));
    assert!(policy.check_permission(&ci_user_id, "deployments", Action::Create));

    // Cannot delete or manage nodes
    assert!(!policy.check_permission(&ci_user_id, "workloads", Action::Delete));
    assert!(!policy.check_permission(&ci_user_id, "nodes", Action::Read));
}

#[test]
fn test_rbac_multiple_roles() {
    let mut policy = RbacPolicy::with_default_roles();

    // Create a user with multiple roles
    let power_user = User::new("power-user").unwrap();
    let user_id = power_user.id.clone();
    policy.add_user(power_user);

    // Assign both operator and viewer roles
    policy.assign_role(&user_id, "operator").unwrap();
    policy.assign_role(&user_id, "viewer").unwrap();

    // Should have combined permissions
    let user = policy.get_user(&user_id).unwrap();
    assert!(user.roles.contains(&"operator".to_string()));
    assert!(user.roles.contains(&"viewer".to_string()));

    // Can do operator things
    assert!(policy.check_permission(&user_id, "workloads", Action::Create));
}

#[test]
fn test_admin_permission_implies_all() {
    let mut role = Role::new("resource-admin", "Admin for a resource").unwrap();
    role.add_permission(Permission::admin("workloads"));

    // Admin permission should imply all actions
    assert!(role.has_permission("workloads", Action::Create));
    assert!(role.has_permission("workloads", Action::Read));
    assert!(role.has_permission("workloads", Action::Update));
    assert!(role.has_permission("workloads", Action::Delete));
    assert!(role.has_permission("workloads", Action::List));
    assert!(role.has_permission("workloads", Action::Execute));
    assert!(role.has_permission("workloads", Action::Admin));

    // But not for other resources
    assert!(!role.has_permission("nodes", Action::Read));
}

// ============================================================================
// Auth Context Tests
// ============================================================================

#[test]
fn test_auth_context_creation() {
    let mut policy = RbacPolicy::with_default_roles();

    let user = User::new("context-test").unwrap();
    let user_id = user.id.clone();
    policy.add_user(user);
    policy.assign_role(&user_id, "operator").unwrap();

    // Create context from policy
    let ctx = context_for_user(&policy, &user_id).unwrap();

    assert!(ctx.is_authenticated());
    assert!(ctx.has_role("operator"));
    assert!(!ctx.has_role("admin"));

    // Check permissions through context
    assert!(ctx.require_permission(&policy, "workloads", Action::Create).is_ok());
    assert!(ctx.require_permission(&policy, "nodes", Action::Delete).is_err());
}

#[test]
fn test_auth_context_with_scopes() {
    let user_id = UserId::new();

    // Create context with limited scopes
    let ctx = AuthContext::authenticated(user_id, vec!["operator".to_string()])
        .with_scopes(vec![
            Scope::new("workloads:read").unwrap(),
            Scope::new("workloads:list").unwrap(),
        ]);

    // Scope check should limit access
    assert!(ctx.require_scope("workloads", Action::Read).is_ok());
    assert!(ctx.require_scope("workloads", Action::List).is_ok());
    assert!(ctx.require_scope("workloads", Action::Create).is_err());
    assert!(ctx.require_scope("nodes", Action::Read).is_err());
}

#[test]
fn test_anonymous_auth_context() {
    let ctx = AuthContext::anonymous();

    assert!(!ctx.is_authenticated());
    assert!(!ctx.has_role("admin"));
    assert!(ctx.user_id.is_none());
}

// ============================================================================
// Header Authentication Tests
// ============================================================================

#[test]
fn test_authenticate_with_api_key_header() {
    // Setup
    let mut policy = RbacPolicy::with_default_roles();
    let user = User::new("api-key-user").unwrap();
    let user_id = user.id.clone();
    policy.add_user(user);
    policy.assign_role(&user_id, "operator").unwrap();

    let mut store = ApiKeyStore::new();
    let (key, secret) = ApiKey::generate("Test", user_id);
    store.store(key);

    let jwt_config = JwtConfig::new_hs256([0u8; 32], "test").unwrap();
    let jwt_manager = JwtManager::new(jwt_config);

    // Test X-API-Key header
    let result = authenticate_request(
        None,
        Some(secret.as_str()),
        &store,
        &jwt_manager,
        &policy,
    );

    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(result.context.is_authenticated());
}

#[test]
fn test_authenticate_with_bearer_token() {
    let mut policy = RbacPolicy::with_default_roles();
    let user = User::new("jwt-user").unwrap();
    let user_id = user.id.clone();
    policy.add_user(user);
    policy.assign_role(&user_id, "admin").unwrap();

    let store = ApiKeyStore::new();
    let jwt_config = JwtConfig::new_hs256([42u8; 32], "test").unwrap();
    let jwt_manager = JwtManager::new(jwt_config);

    // Create JWT token
    let token = jwt_manager.create_token(&user_id).unwrap();
    let bearer = format!("Bearer {}", token);

    // Authenticate with Bearer header
    let result = authenticate_request(
        Some(&bearer),
        None,
        &store,
        &jwt_manager,
        &policy,
    );

    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(result.context.is_authenticated());
    assert!(result.claims.is_some());
}

#[test]
fn test_optional_auth_allows_anonymous() {
    let policy = RbacPolicy::with_default_roles();
    let store = ApiKeyStore::new();
    let jwt_config = JwtConfig::new_hs256([0u8; 32], "test").unwrap();
    let jwt_manager = JwtManager::new(jwt_config);

    // No auth headers provided
    let result = authenticate_request_optional(
        None,
        None,
        &store,
        &jwt_manager,
        &policy,
    );

    // Should return anonymous context, not error
    assert!(!result.context.is_authenticated());
}

// ============================================================================
// Scope Pattern Tests
// ============================================================================

#[test]
fn test_wildcard_scope_allows_everything() {
    let wildcard = Scope::new("*").unwrap();

    assert!(wildcard.allows("anything", Action::Admin));
    assert!(wildcard.allows("workloads", Action::Create));
    assert!(wildcard.allows("nodes", Action::Delete));
    assert!(wildcard.allows("secrets", Action::Read));
}

#[test]
fn test_resource_wildcard_scope() {
    let workload_scope = Scope::new("workloads:*").unwrap();

    assert!(workload_scope.allows("workloads", Action::Create));
    assert!(workload_scope.allows("workloads", Action::Delete));
    assert!(workload_scope.allows("workloads", Action::Read));
    assert!(workload_scope.allows("workloads", Action::Admin));

    // Should not allow other resources
    assert!(!workload_scope.allows("nodes", Action::Read));
    assert!(!workload_scope.allows("secrets", Action::Read));
}

#[test]
fn test_specific_action_scope() {
    let read_only = Scope::new("workloads:read").unwrap();

    assert!(read_only.allows("workloads", Action::Read));
    assert!(!read_only.allows("workloads", Action::Create));
    assert!(!read_only.allows("workloads", Action::Delete));

    // Child resources should also work
    assert!(read_only.allows("workloads:logs", Action::Read));
}

// ============================================================================
// Integration: Full Auth Workflow
// ============================================================================

#[test]
fn test_full_auth_workflow_api_key() {
    // 1. Setup RBAC policy
    let mut policy = RbacPolicy::with_default_roles();
    let user = User::new("automation-user").unwrap();
    let user_id = user.id.clone();
    policy.add_user(user);
    policy.assign_role(&user_id, "operator").unwrap();

    // 2. Setup API key store
    let mut store = ApiKeyStore::new();
    let (mut key, secret) = ApiKey::generate("Automation Key", user_id.clone());
    key.add_scope(Scope::new("workloads:*").unwrap());
    key.add_scope(Scope::new("nodes:read").unwrap());
    store.store(key);

    // 3. Setup JWT (not used for API key auth but required)
    let jwt_config = JwtConfig::new_hs256([0u8; 32], "test").unwrap();
    let jwt_manager = JwtManager::new(jwt_config);

    // 4. Authenticate
    let result = authenticate_request(
        None,
        Some(secret.as_str()),
        &store,
        &jwt_manager,
        &policy,
    ).unwrap();

    // 5. Verify permissions
    let ctx = result.context;
    assert!(ctx.is_authenticated());
    assert!(ctx.has_role("operator"));

    // 6. Check scope-limited access
    assert!(ctx.require_scope("workloads", Action::Create).is_ok());
    assert!(ctx.require_scope("workloads", Action::Delete).is_ok());
    assert!(ctx.require_scope("nodes", Action::Read).is_ok());
    assert!(ctx.require_scope("nodes", Action::Delete).is_err());

    // 7. Check RBAC permissions
    assert!(ctx.require_permission(&policy, "workloads", Action::Create).is_ok());
}

#[test]
fn test_full_auth_workflow_jwt() {
    // 1. Setup RBAC policy
    let mut policy = RbacPolicy::with_default_roles();
    let user = User::new("web-user").unwrap();
    let user_id = user.id.clone();
    policy.add_user(user);
    policy.assign_role(&user_id, "admin").unwrap();

    // 2. Setup JWT
    let jwt_config = JwtConfig::new_hs256([99u8; 32], "clawbernetes")
        .unwrap()
        .with_default_expiry(Duration::hours(8));
    let jwt_manager = JwtManager::new(jwt_config);

    // 3. Create token with claims
    let claims = JwtClaims::new(&user_id, "clawbernetes")
        .with_roles(vec!["admin".to_string()])
        .with_scopes(vec![Scope::new("*").unwrap()]);
    let token = jwt_manager.create_token_with_claims(&claims).unwrap();

    // 4. Authenticate
    let store = ApiKeyStore::new();
    let bearer = format!("Bearer {}", token);
    let result = authenticate_request(
        Some(&bearer),
        None,
        &store,
        &jwt_manager,
        &policy,
    ).unwrap();

    // 5. Verify
    let ctx = result.context;
    assert!(ctx.is_authenticated());
    assert!(ctx.has_role("admin"));

    // Admin can do everything
    assert!(ctx.require_permission(&policy, "workloads", Action::Admin).is_ok());
    assert!(ctx.require_permission(&policy, "nodes", Action::Delete).is_ok());
    assert!(ctx.require_permission(&policy, "secrets", Action::Create).is_ok());

    // 6. Verify claims were extracted
    let claims = result.claims.unwrap();
    assert_eq!(claims.sub, user_id.to_string());
}
