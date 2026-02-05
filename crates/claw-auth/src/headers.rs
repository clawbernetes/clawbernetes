//! HTTP header utilities for authentication.
//!
//! This module provides utilities for working with authentication headers
//! in HTTP requests, integrating with the CLI protocol.

use crate::apikey::{authenticate_api_key, ApiKeyStore};
use crate::error::{Error, Result};
use crate::jwt::{JwtClaims, JwtManager};
use crate::rbac::{AuthContext, RbacPolicy};
use crate::types::Scope;

/// The standard HTTP Authorization header name.
pub const AUTHORIZATION_HEADER: &str = "Authorization";

/// Custom header for API key authentication (alternative to Authorization).
pub const X_API_KEY_HEADER: &str = "X-API-Key";

/// Credential type extracted from headers.
#[derive(Debug)]
pub enum Credential<'a> {
    /// API key credential.
    ApiKey(&'a str),
    /// JWT bearer token.
    BearerToken(&'a str),
}

/// Extracts credentials from HTTP headers.
///
/// Checks in order:
/// 1. `X-API-Key` header for direct API key
/// 2. `Authorization: Bearer <token>` for JWT
/// 3. `Authorization: <api_key>` for API key (if starts with "claw_")
///
/// # Errors
///
/// Returns an error if no valid credentials are found.
pub fn extract_credentials<'a>(
    authorization: Option<&'a str>,
    x_api_key: Option<&'a str>,
) -> Result<Credential<'a>> {
    // Check X-API-Key header first
    if let Some(api_key) = x_api_key {
        let api_key = api_key.trim();
        if !api_key.is_empty() {
            return Ok(Credential::ApiKey(api_key));
        }
    }

    // Check Authorization header
    if let Some(auth) = authorization {
        let auth = auth.trim();
        if auth.is_empty() {
            return Err(Error::AuthenticationRequired);
        }

        // Check if it's a Bearer token
        if let Some(token) = auth.strip_prefix("Bearer ") {
            let token = token.trim();
            // If it starts with "claw_", it's an API key in Bearer format
            if token.starts_with("claw_") {
                return Ok(Credential::ApiKey(token));
            }
            return Ok(Credential::BearerToken(token));
        }

        // Check if it's a direct API key
        if auth.starts_with("claw_") {
            return Ok(Credential::ApiKey(auth));
        }

        return Err(Error::InvalidToken {
            reason: "unsupported authorization scheme".to_string(),
        });
    }

    Err(Error::AuthenticationRequired)
}

/// Result of authentication containing the context and optional claims.
#[derive(Debug)]
pub struct AuthResult {
    /// The authenticated context.
    pub context: AuthContext,
    /// JWT claims if authenticated via token.
    pub claims: Option<JwtClaims>,
}

/// Authenticates a request using API key store and JWT manager.
///
/// # Errors
///
/// Returns an error if authentication fails.
pub fn authenticate_request(
    authorization: Option<&str>,
    x_api_key: Option<&str>,
    api_key_store: &ApiKeyStore,
    jwt_manager: &JwtManager,
    rbac_policy: &RbacPolicy,
) -> Result<AuthResult> {
    let credential = extract_credentials(authorization, x_api_key)?;

    match credential {
        Credential::ApiKey(key) => {
            let api_key = authenticate_api_key(api_key_store, key)?;
            let user = rbac_policy
                .get_user(&api_key.user_id)
                .ok_or_else(|| Error::UserNotFound {
                    id: api_key.user_id.to_string(),
                })?;

            if !user.active {
                return Err(Error::PermissionDenied {
                    reason: "user is deactivated".to_string(),
                });
            }

            // Convert API key scopes to auth context scopes
            let scopes = api_key.scopes.clone();
            let context = AuthContext::authenticated(api_key.user_id.clone(), user.roles.clone())
                .with_scopes(scopes);

            Ok(AuthResult {
                context,
                claims: None,
            })
        }
        Credential::BearerToken(token) => {
            let claims = jwt_manager.validate_token(token)?;
            let user_id = claims.user_id()?;

            let user = rbac_policy
                .get_user(&user_id)
                .ok_or_else(|| Error::UserNotFound {
                    id: user_id.to_string(),
                })?;

            if !user.active {
                return Err(Error::PermissionDenied {
                    reason: "user is deactivated".to_string(),
                });
            }

            // Parse scopes from claims
            let scopes: Vec<Scope> = claims
                .scopes
                .iter()
                .filter_map(|s| Scope::new(s).ok())
                .collect();

            let context =
                AuthContext::authenticated(user_id, user.roles.clone()).with_scopes(scopes);

            Ok(AuthResult {
                context,
                claims: Some(claims),
            })
        }
    }
}

/// Authenticates a request, returning anonymous context if no credentials provided.
///
/// Use this for endpoints that support both authenticated and anonymous access.
pub fn authenticate_request_optional(
    authorization: Option<&str>,
    x_api_key: Option<&str>,
    api_key_store: &ApiKeyStore,
    jwt_manager: &JwtManager,
    rbac_policy: &RbacPolicy,
) -> AuthResult {
    match authenticate_request(
        authorization,
        x_api_key,
        api_key_store,
        jwt_manager,
        rbac_policy,
    ) {
        Ok(result) => result,
        Err(_) => AuthResult {
            context: AuthContext::anonymous(),
            claims: None,
        },
    }
}

/// Builder for authentication middleware configuration.
#[derive(Debug, Clone)]
pub struct AuthMiddlewareConfig {
    /// Whether to allow anonymous access.
    pub allow_anonymous: bool,
    /// Required roles (any of these).
    pub required_roles: Vec<String>,
    /// Required scopes.
    pub required_scopes: Vec<String>,
}

impl Default for AuthMiddlewareConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthMiddlewareConfig {
    /// Creates a new config requiring authentication.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            allow_anonymous: false,
            required_roles: Vec::new(),
            required_scopes: Vec::new(),
        }
    }

    /// Allows anonymous access.
    #[must_use]
    pub const fn allow_anonymous(mut self) -> Self {
        self.allow_anonymous = true;
        self
    }

    /// Requires one of the specified roles.
    #[must_use]
    pub fn require_roles(mut self, roles: Vec<String>) -> Self {
        self.required_roles = roles;
        self
    }

    /// Requires the specified scopes.
    #[must_use]
    pub fn require_scopes(mut self, scopes: Vec<String>) -> Self {
        self.required_scopes = scopes;
        self
    }

    /// Validates an auth context against this config.
    ///
    /// # Errors
    ///
    /// Returns an error if the context doesn't meet the requirements.
    pub fn validate(&self, context: &AuthContext) -> Result<()> {
        // Check authentication requirement
        if !self.allow_anonymous && !context.is_authenticated() {
            return Err(Error::AuthenticationRequired);
        }

        // If anonymous is allowed and we're not authenticated, skip other checks
        if !context.is_authenticated() {
            return Ok(());
        }

        // Check role requirements
        if !self.required_roles.is_empty() {
            let has_required_role = self
                .required_roles
                .iter()
                .any(|r| context.has_role(r));
            if !has_required_role {
                return Err(Error::PermissionDenied {
                    reason: format!(
                        "required one of roles: {}",
                        self.required_roles.join(", ")
                    ),
                });
            }
        }

        // Check scope requirements
        for scope_str in &self.required_scopes {
            let parts: Vec<&str> = scope_str.splitn(2, ':').collect();
            if parts.len() != 2 {
                continue;
            }
            let resource = parts[0];
            let action_str = parts[1];
            if let Ok(action) = action_str.parse() {
                context.require_scope(resource, action)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apikey::{extract_api_key_from_header, ApiKey};
    use crate::jwt::{extract_jwt_from_header, JwtConfig};
    use crate::types::{User, UserId};

    fn test_jwt_config() -> JwtConfig {
        let secret = [0u8; 32];
        JwtConfig::new_hs256(secret, "test-issuer").ok().unwrap()
    }

    fn setup_test_env() -> (ApiKeyStore, JwtManager, RbacPolicy, UserId, String) {
        let mut api_key_store = ApiKeyStore::new();
        let jwt_manager = JwtManager::new(test_jwt_config());
        let mut rbac_policy = RbacPolicy::with_default_roles();

        // Create a test user
        let user_id = UserId::new();
        let mut user = User::with_id(user_id.clone(), "test-user");
        user.add_role("operator");
        rbac_policy.add_user(user);

        // Create an API key for the user
        let (api_key, secret) = ApiKey::generate("Test Key", user_id.clone());
        let api_key = api_key.with_scopes(vec![
            Scope::new("workloads:*").ok().unwrap(),
            Scope::new("nodes:read").ok().unwrap(),
        ]);
        api_key_store.store(api_key);

        (api_key_store, jwt_manager, rbac_policy, user_id, secret.as_str().to_string())
    }

    // ===================
    // Credential Extraction Tests
    // ===================

    #[test]
    fn extract_credentials_x_api_key() {
        let result = extract_credentials(None, Some("claw_test123456789012345678"));
        assert!(result.is_ok());
        assert!(matches!(result.ok(), Some(Credential::ApiKey(_))));
    }

    #[test]
    fn extract_credentials_bearer_jwt() {
        let result = extract_credentials(Some("Bearer eyJhbGciOiJIUzI1NiJ9.test"), None);
        assert!(result.is_ok());
        assert!(matches!(result.ok(), Some(Credential::BearerToken(_))));
    }

    #[test]
    fn extract_credentials_bearer_api_key() {
        let result = extract_credentials(Some("Bearer claw_test123456789012345678"), None);
        assert!(result.is_ok());
        assert!(matches!(result.ok(), Some(Credential::ApiKey(_))));
    }

    #[test]
    fn extract_credentials_direct_api_key() {
        let result = extract_credentials(Some("claw_test123456789012345678"), None);
        assert!(result.is_ok());
        assert!(matches!(result.ok(), Some(Credential::ApiKey(_))));
    }

    #[test]
    fn extract_credentials_none() {
        let result = extract_credentials(None, None);
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::AuthenticationRequired)));
    }

    #[test]
    fn extract_credentials_empty() {
        let result = extract_credentials(Some(""), None);
        assert!(result.is_err());
    }

    #[test]
    fn extract_credentials_invalid_scheme() {
        let result = extract_credentials(Some("Basic dXNlcjpwYXNz"), None);
        assert!(result.is_err());
    }

    #[test]
    fn extract_credentials_x_api_key_priority() {
        // X-API-Key takes priority over Authorization
        let result = extract_credentials(
            Some("Bearer jwt-token"),
            Some("claw_priority123456789012345"),
        );
        assert!(result.is_ok());
        if let Some(Credential::ApiKey(key)) = result.ok() {
            assert!(key.starts_with("claw_priority"));
        } else {
            panic!("Expected ApiKey credential");
        }
    }

    // ===================
    // Authentication Tests
    // ===================

    #[test]
    fn authenticate_request_api_key() {
        let (api_key_store, jwt_manager, rbac_policy, user_id, secret) = setup_test_env();
        let result = authenticate_request(
            Some(&secret),
            None,
            &api_key_store,
            &jwt_manager,
            &rbac_policy,
        );
        assert!(result.is_ok());
        let result = result.ok().unwrap();
        assert!(result.context.is_authenticated());
        assert_eq!(result.context.user_id(), &user_id);
        assert!(result.claims.is_none());
    }

    #[test]
    fn authenticate_request_jwt() {
        let (api_key_store, jwt_manager, rbac_policy, user_id, _) = setup_test_env();
        let token = jwt_manager.create_token(&user_id).ok().unwrap();
        let auth_header = format!("Bearer {token}");
        let result = authenticate_request(
            Some(&auth_header),
            None,
            &api_key_store,
            &jwt_manager,
            &rbac_policy,
        );
        assert!(result.is_ok());
        let result = result.ok().unwrap();
        assert!(result.context.is_authenticated());
        assert!(result.claims.is_some());
    }

    #[test]
    fn authenticate_request_invalid_key() {
        let (api_key_store, jwt_manager, rbac_policy, _, _) = setup_test_env();
        let result = authenticate_request(
            Some("claw_invalid123456789012345678"),
            None,
            &api_key_store,
            &jwt_manager,
            &rbac_policy,
        );
        assert!(result.is_err());
    }

    #[test]
    fn authenticate_request_optional_no_creds() {
        let (api_key_store, jwt_manager, rbac_policy, _, _) = setup_test_env();
        let result = authenticate_request_optional(
            None,
            None,
            &api_key_store,
            &jwt_manager,
            &rbac_policy,
        );
        assert!(!result.context.is_authenticated());
    }

    #[test]
    fn authenticate_request_inactive_user() {
        let (api_key_store, jwt_manager, mut rbac_policy, user_id, secret) = setup_test_env();
        let user = rbac_policy.get_user_mut(&user_id).unwrap();
        user.deactivate();
        let result = authenticate_request(
            Some(&secret),
            None,
            &api_key_store,
            &jwt_manager,
            &rbac_policy,
        );
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::PermissionDenied { .. })));
    }

    // ===================
    // AuthMiddlewareConfig Tests
    // ===================

    #[test]
    fn auth_middleware_config_default() {
        let config = AuthMiddlewareConfig::new();
        assert!(!config.allow_anonymous);
        assert!(config.required_roles.is_empty());
    }

    #[test]
    fn auth_middleware_config_allow_anonymous() {
        let config = AuthMiddlewareConfig::new().allow_anonymous();
        assert!(config.allow_anonymous);
    }

    #[test]
    fn auth_middleware_config_require_roles() {
        let config = AuthMiddlewareConfig::new()
            .require_roles(vec!["admin".to_string(), "operator".to_string()]);
        assert_eq!(config.required_roles.len(), 2);
    }

    #[test]
    fn auth_middleware_config_validate_authenticated() {
        let config = AuthMiddlewareConfig::new();
        let user_id = UserId::new();
        let ctx = AuthContext::authenticated(user_id, vec![]);
        assert!(config.validate(&ctx).is_ok());
    }

    #[test]
    fn auth_middleware_config_validate_anonymous_not_allowed() {
        let config = AuthMiddlewareConfig::new();
        let ctx = AuthContext::anonymous();
        let result = config.validate(&ctx);
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::AuthenticationRequired)));
    }

    #[test]
    fn auth_middleware_config_validate_anonymous_allowed() {
        let config = AuthMiddlewareConfig::new().allow_anonymous();
        let ctx = AuthContext::anonymous();
        assert!(config.validate(&ctx).is_ok());
    }

    #[test]
    fn auth_middleware_config_validate_required_role_success() {
        let config = AuthMiddlewareConfig::new()
            .require_roles(vec!["admin".to_string(), "operator".to_string()]);
        let user_id = UserId::new();
        let ctx = AuthContext::authenticated(user_id, vec!["operator".to_string()]);
        assert!(config.validate(&ctx).is_ok());
    }

    #[test]
    fn auth_middleware_config_validate_required_role_failure() {
        let config = AuthMiddlewareConfig::new()
            .require_roles(vec!["admin".to_string()]);
        let user_id = UserId::new();
        let ctx = AuthContext::authenticated(user_id, vec!["operator".to_string()]);
        let result = config.validate(&ctx);
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::PermissionDenied { .. })));
    }

    #[test]
    fn auth_middleware_config_validate_required_scope() {
        let config = AuthMiddlewareConfig::new()
            .require_scopes(vec!["workloads:read".to_string()]);
        let user_id = UserId::new();
        let scopes = vec![Scope::new("workloads:read").ok().unwrap()];
        let ctx = AuthContext::authenticated(user_id, vec![]).with_scopes(scopes);
        assert!(config.validate(&ctx).is_ok());
    }

    // ===================
    // Header Constant Tests
    // ===================

    #[test]
    fn header_constants() {
        assert_eq!(AUTHORIZATION_HEADER, "Authorization");
        assert_eq!(X_API_KEY_HEADER, "X-API-Key");
    }

    #[test]
    fn extract_api_key_from_header_works() {
        let key = extract_api_key_from_header("Bearer claw_test123456789012");
        assert!(key.is_ok());
    }

    #[test]
    fn extract_jwt_from_header_works() {
        let token = extract_jwt_from_header("Bearer eyJhbGciOiJIUzI1NiJ9.test");
        assert!(token.is_ok());
    }
}
