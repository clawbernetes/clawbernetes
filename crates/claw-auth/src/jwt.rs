//! JWT (JSON Web Token) support for session authentication.
//!
//! This module provides JWT token generation and validation:
//! - [`JwtClaims`]: Standard JWT claims with custom extensions
//! - [`JwtConfig`]: Configuration for JWT signing and validation
//! - [`JwtManager`]: High-level API for creating and validating tokens

use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::types::{Scope, UserId};

/// Default token expiration time in hours.
const DEFAULT_EXPIRY_HOURS: i64 = 24;

/// JWT claims structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Subject (user ID).
    pub sub: String,
    /// Issuer.
    pub iss: String,
    /// Audience.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    /// Expiration time (Unix timestamp).
    pub exp: i64,
    /// Issued at time (Unix timestamp).
    pub iat: i64,
    /// Not before time (Unix timestamp).
    pub nbf: i64,
    /// JWT ID (unique identifier for this token).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
    /// User's roles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<String>,
    /// Token scopes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    /// Session ID for token revocation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

impl JwtClaims {
    /// Creates new claims for a user.
    #[must_use]
    pub fn new(user_id: &UserId, issuer: impl Into<String>) -> Self {
        let now = Utc::now();
        let exp = now + Duration::hours(DEFAULT_EXPIRY_HOURS);
        Self {
            sub: user_id.to_string(),
            iss: issuer.into(),
            aud: None,
            exp: exp.timestamp(),
            iat: now.timestamp(),
            nbf: now.timestamp(),
            jti: Some(uuid::Uuid::new_v4().to_string()),
            roles: Vec::new(),
            scopes: Vec::new(),
            session_id: None,
        }
    }

    /// Sets the expiration time.
    #[must_use]
    pub fn with_expiry(mut self, exp: DateTime<Utc>) -> Self {
        self.exp = exp.timestamp();
        self
    }

    /// Sets the expiration to a duration from now.
    #[must_use]
    pub fn with_expiry_duration(mut self, duration: Duration) -> Self {
        self.exp = (Utc::now() + duration).timestamp();
        self
    }

    /// Sets the audience.
    #[must_use]
    pub fn with_audience(mut self, aud: impl Into<String>) -> Self {
        self.aud = Some(aud.into());
        self
    }

    /// Sets the roles.
    #[must_use]
    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.roles = roles;
        self
    }

    /// Sets the scopes.
    #[must_use]
    pub fn with_scopes(mut self, scopes: Vec<Scope>) -> Self {
        self.scopes = scopes.into_iter().map(String::from).collect();
        self
    }

    /// Sets the session ID.
    #[must_use]
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Returns the user ID from the subject claim.
    ///
    /// # Errors
    ///
    /// Returns an error if the subject is not a valid user ID.
    pub fn user_id(&self) -> Result<UserId> {
        UserId::from_string(&self.sub)
    }

    /// Returns the expiration time as a `DateTime`.
    #[must_use]
    pub fn expiry(&self) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp(self.exp, 0)
    }

    /// Checks if the token has expired.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() > self.exp
    }

    /// Checks if the token is not yet valid.
    #[must_use]
    pub fn is_not_yet_valid(&self) -> bool {
        Utc::now().timestamp() < self.nbf
    }

    /// Parses the scopes back into `Scope` objects.
    ///
    /// # Errors
    ///
    /// Returns an error if any scope is invalid.
    pub fn parsed_scopes(&self) -> Result<Vec<Scope>> {
        self.scopes.iter().map(Scope::new).collect()
    }
}

/// Configuration for JWT signing and validation.
#[derive(Clone)]
pub struct JwtConfig {
    /// Secret key for HMAC signing (HS256/HS384/HS512).
    secret: Vec<u8>,
    /// Algorithm to use.
    algorithm: Algorithm,
    /// Issuer string.
    issuer: String,
    /// Optional required audience.
    audience: Option<String>,
    /// Default token validity duration.
    default_expiry: Duration,
}

impl JwtConfig {
    /// Creates a new JWT configuration with HMAC-SHA256.
    ///
    /// # Errors
    ///
    /// Returns an error if the secret is too short (< 32 bytes).
    pub fn new_hs256(secret: impl AsRef<[u8]>, issuer: impl Into<String>) -> Result<Self> {
        let secret = secret.as_ref();
        if secret.len() < 32 {
            return Err(Error::JwtError {
                reason: "secret must be at least 32 bytes for HS256".to_string(),
            });
        }
        Ok(Self {
            secret: secret.to_vec(),
            algorithm: Algorithm::HS256,
            issuer: issuer.into(),
            audience: None,
            default_expiry: Duration::hours(DEFAULT_EXPIRY_HOURS),
        })
    }

    /// Creates a new JWT configuration with HMAC-SHA384.
    ///
    /// # Errors
    ///
    /// Returns an error if the secret is too short (< 48 bytes).
    pub fn new_hs384(secret: impl AsRef<[u8]>, issuer: impl Into<String>) -> Result<Self> {
        let secret = secret.as_ref();
        if secret.len() < 48 {
            return Err(Error::JwtError {
                reason: "secret must be at least 48 bytes for HS384".to_string(),
            });
        }
        Ok(Self {
            secret: secret.to_vec(),
            algorithm: Algorithm::HS384,
            issuer: issuer.into(),
            audience: None,
            default_expiry: Duration::hours(DEFAULT_EXPIRY_HOURS),
        })
    }

    /// Sets the required audience for validation.
    #[must_use]
    pub fn with_audience(mut self, audience: impl Into<String>) -> Self {
        self.audience = Some(audience.into());
        self
    }

    /// Sets the default token expiry duration.
    #[must_use]
    pub fn with_default_expiry(mut self, duration: Duration) -> Self {
        self.default_expiry = duration;
        self
    }

    /// Returns the issuer.
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// Returns the default expiry duration.
    #[must_use]
    pub fn default_expiry(&self) -> Duration {
        self.default_expiry
    }

    /// Creates an encoding key.
    fn encoding_key(&self) -> EncodingKey {
        EncodingKey::from_secret(&self.secret)
    }

    /// Creates a decoding key.
    fn decoding_key(&self) -> DecodingKey {
        DecodingKey::from_secret(&self.secret)
    }

    /// Creates validation rules.
    fn validation(&self) -> Validation {
        let mut validation = Validation::new(self.algorithm);
        validation.set_issuer(&[&self.issuer]);
        validation.set_required_spec_claims(&["exp", "iat", "nbf", "sub"]);
        if let Some(ref aud) = self.audience {
            validation.set_audience(&[aud]);
        } else {
            validation.validate_aud = false;
        }
        validation
    }
}

impl std::fmt::Debug for JwtConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtConfig")
            .field("algorithm", &self.algorithm)
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("default_expiry", &self.default_expiry)
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

/// High-level JWT manager for creating and validating tokens.
#[derive(Debug)]
pub struct JwtManager {
    config: JwtConfig,
}

impl JwtManager {
    /// Creates a new JWT manager with the given configuration.
    #[must_use]
    pub const fn new(config: JwtConfig) -> Self {
        Self { config }
    }

    /// Creates a new token for a user.
    ///
    /// # Errors
    ///
    /// Returns an error if token creation fails.
    pub fn create_token(&self, user_id: &UserId) -> Result<String> {
        let claims = JwtClaims::new(user_id, &self.config.issuer)
            .with_expiry_duration(self.config.default_expiry);
        self.encode_claims(&claims)
    }

    /// Creates a token with custom claims.
    ///
    /// # Errors
    ///
    /// Returns an error if token creation fails.
    pub fn create_token_with_claims(&self, claims: &JwtClaims) -> Result<String> {
        self.encode_claims(claims)
    }

    /// Validates a token and returns the claims.
    ///
    /// # Errors
    ///
    /// Returns an error if the token is invalid or expired.
    pub fn validate_token(&self, token: &str) -> Result<JwtClaims> {
        let token_data = decode::<JwtClaims>(
            token,
            &self.config.decoding_key(),
            &self.config.validation(),
        )
        .map_err(|e| match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => Error::TokenExpired,
            jsonwebtoken::errors::ErrorKind::InvalidToken
            | jsonwebtoken::errors::ErrorKind::InvalidSignature => Error::InvalidToken {
                reason: e.to_string(),
            },
            _ => Error::JwtError {
                reason: e.to_string(),
            },
        })?;

        Ok(token_data.claims)
    }

    /// Refreshes a token with a new expiration time.
    ///
    /// The token must still be valid (not expired) to be refreshed.
    ///
    /// # Errors
    ///
    /// Returns an error if the token is invalid or refresh fails.
    pub fn refresh_token(&self, token: &str) -> Result<String> {
        let claims = self.validate_token(token)?;
        let new_claims = JwtClaims {
            exp: (Utc::now() + self.config.default_expiry).timestamp(),
            iat: Utc::now().timestamp(),
            jti: Some(uuid::Uuid::new_v4().to_string()),
            ..claims
        };
        self.encode_claims(&new_claims)
    }

    /// Returns the configuration.
    #[must_use]
    pub const fn config(&self) -> &JwtConfig {
        &self.config
    }

    fn encode_claims(&self, claims: &JwtClaims) -> Result<String> {
        let header = Header::new(self.config.algorithm);
        encode(&header, claims, &self.config.encoding_key()).map_err(|e| Error::JwtError {
            reason: e.to_string(),
        })
    }
}

/// Extracts a JWT token from an HTTP Authorization header.
///
/// Expected format: `Bearer <token>`
///
/// # Errors
///
/// Returns an error if the header format is invalid.
pub fn extract_jwt_from_header(header: &str) -> Result<&str> {
    let header = header.trim();
    if header.is_empty() {
        return Err(Error::InvalidToken {
            reason: "authorization header is empty".to_string(),
        });
    }
    header.strip_prefix("Bearer ").map(str::trim).ok_or(Error::InvalidToken {
        reason: "invalid authorization header format, expected 'Bearer <token>'".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> JwtConfig {
        let secret = [0u8; 32];
        JwtConfig::new_hs256(secret, "test-issuer").ok().unwrap()
    }

    // ===================
    // JwtClaims Tests
    // ===================

    #[test]
    fn jwt_claims_new() {
        let user_id = UserId::new();
        let claims = JwtClaims::new(&user_id, "test-issuer");
        assert_eq!(claims.sub, user_id.to_string());
        assert_eq!(claims.iss, "test-issuer");
        assert!(claims.jti.is_some());
        assert!(!claims.is_expired());
    }

    #[test]
    fn jwt_claims_with_expiry() {
        let user_id = UserId::new();
        let exp = Utc::now() + Duration::hours(1);
        let claims = JwtClaims::new(&user_id, "test").with_expiry(exp);
        assert_eq!(claims.exp, exp.timestamp());
    }

    #[test]
    fn jwt_claims_with_expiry_duration() {
        let user_id = UserId::new();
        let claims = JwtClaims::new(&user_id, "test").with_expiry_duration(Duration::hours(2));
        let expected_exp = Utc::now() + Duration::hours(2);
        // Allow 1 second tolerance
        assert!((claims.exp - expected_exp.timestamp()).abs() <= 1);
    }

    #[test]
    fn jwt_claims_with_roles() {
        let user_id = UserId::new();
        let claims = JwtClaims::new(&user_id, "test")
            .with_roles(vec!["admin".to_string(), "operator".to_string()]);
        assert_eq!(claims.roles.len(), 2);
        assert!(claims.roles.contains(&"admin".to_string()));
    }

    #[test]
    fn jwt_claims_with_scopes() {
        let user_id = UserId::new();
        let scopes = vec![
            Scope::new("workloads:read").ok().unwrap(),
            Scope::new("nodes:list").ok().unwrap(),
        ];
        let claims = JwtClaims::new(&user_id, "test").with_scopes(scopes);
        assert_eq!(claims.scopes.len(), 2);
    }

    #[test]
    fn jwt_claims_user_id() {
        let user_id = UserId::new();
        let claims = JwtClaims::new(&user_id, "test");
        let extracted = claims.user_id();
        assert!(extracted.is_ok());
        assert_eq!(extracted.ok().unwrap(), user_id);
    }

    #[test]
    fn jwt_claims_is_expired() {
        let user_id = UserId::new();
        let claims = JwtClaims::new(&user_id, "test")
            .with_expiry(Utc::now() - Duration::hours(1));
        assert!(claims.is_expired());
    }

    #[test]
    fn jwt_claims_parsed_scopes() {
        let user_id = UserId::new();
        let scopes = vec![
            Scope::new("workloads:read").ok().unwrap(),
            Scope::new("nodes:list").ok().unwrap(),
        ];
        let claims = JwtClaims::new(&user_id, "test").with_scopes(scopes);
        let parsed = claims.parsed_scopes();
        assert!(parsed.is_ok());
        assert_eq!(parsed.ok().unwrap().len(), 2);
    }

    // ===================
    // JwtConfig Tests
    // ===================

    #[test]
    fn jwt_config_new_hs256() {
        let secret = [0u8; 32];
        let config = JwtConfig::new_hs256(secret, "test");
        assert!(config.is_ok());
        let config = config.ok().unwrap();
        assert_eq!(config.issuer(), "test");
    }

    #[test]
    fn jwt_config_new_hs256_too_short() {
        let secret = [0u8; 16];
        let config = JwtConfig::new_hs256(secret, "test");
        assert!(config.is_err());
    }

    #[test]
    fn jwt_config_new_hs384() {
        let secret = [0u8; 48];
        let config = JwtConfig::new_hs384(secret, "test");
        assert!(config.is_ok());
    }

    #[test]
    fn jwt_config_new_hs384_too_short() {
        let secret = [0u8; 32];
        let config = JwtConfig::new_hs384(secret, "test");
        assert!(config.is_err());
    }

    #[test]
    fn jwt_config_with_audience() {
        let config = test_config().with_audience("api.example.com");
        assert_eq!(config.audience, Some("api.example.com".to_string()));
    }

    #[test]
    fn jwt_config_with_default_expiry() {
        let config = test_config().with_default_expiry(Duration::hours(48));
        assert_eq!(config.default_expiry(), Duration::hours(48));
    }

    #[test]
    fn jwt_config_debug_redacted() {
        let config = test_config();
        let debug = format!("{config:?}");
        assert!(debug.contains("[REDACTED]"));
    }

    // ===================
    // JwtManager Tests
    // ===================

    #[test]
    fn jwt_manager_create_token() {
        let manager = JwtManager::new(test_config());
        let user_id = UserId::new();
        let token = manager.create_token(&user_id);
        assert!(token.is_ok());
        let token = token.ok().unwrap();
        assert!(!token.is_empty());
        assert!(token.contains('.'));
    }

    #[test]
    fn jwt_manager_validate_token() {
        let manager = JwtManager::new(test_config());
        let user_id = UserId::new();
        let token = manager.create_token(&user_id);
        assert!(token.is_ok());
        let token = token.ok().unwrap();
        let claims = manager.validate_token(&token);
        assert!(claims.is_ok());
        let claims = claims.ok().unwrap();
        assert_eq!(claims.sub, user_id.to_string());
    }

    #[test]
    fn jwt_manager_validate_expired_token() {
        let manager = JwtManager::new(test_config());
        let user_id = UserId::new();
        let claims = JwtClaims::new(&user_id, "test-issuer")
            .with_expiry(Utc::now() - Duration::hours(1));
        let token = manager.create_token_with_claims(&claims);
        assert!(token.is_ok());
        let token = token.ok().unwrap();
        let result = manager.validate_token(&token);
        assert!(result.is_err());
        assert!(matches!(result.err(), Some(Error::TokenExpired)));
    }

    #[test]
    fn jwt_manager_validate_invalid_token() {
        let manager = JwtManager::new(test_config());
        let result = manager.validate_token("invalid.token.here");
        assert!(result.is_err());
    }

    #[test]
    fn jwt_manager_validate_wrong_signature() {
        let manager1 = JwtManager::new(test_config());
        let other_secret = [1u8; 32];
        let other_config = JwtConfig::new_hs256(other_secret, "test-issuer").ok().unwrap();
        let manager2 = JwtManager::new(other_config);
        let user_id = UserId::new();
        let token = manager1.create_token(&user_id);
        assert!(token.is_ok());
        let token = token.ok().unwrap();
        let result = manager2.validate_token(&token);
        assert!(result.is_err());
    }

    #[test]
    fn jwt_manager_refresh_token() {
        let manager = JwtManager::new(test_config());
        let user_id = UserId::new();
        let token = manager.create_token(&user_id);
        assert!(token.is_ok());
        let token = token.ok().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let refreshed = manager.refresh_token(&token);
        assert!(refreshed.is_ok());
        let refreshed = refreshed.ok().unwrap();
        assert_ne!(token, refreshed);
        let claims = manager.validate_token(&refreshed);
        assert!(claims.is_ok());
        assert_eq!(claims.ok().unwrap().sub, user_id.to_string());
    }

    #[test]
    fn jwt_manager_create_token_with_claims() {
        let manager = JwtManager::new(test_config());
        let user_id = UserId::new();
        let claims = JwtClaims::new(&user_id, "test-issuer")
            .with_roles(vec!["admin".to_string()])
            .with_session_id("session-123");
        let token = manager.create_token_with_claims(&claims);
        assert!(token.is_ok());
        let token = token.ok().unwrap();
        let validated = manager.validate_token(&token);
        assert!(validated.is_ok());
        let validated = validated.ok().unwrap();
        assert_eq!(validated.roles, vec!["admin"]);
        assert_eq!(validated.session_id, Some("session-123".to_string()));
    }

    #[test]
    fn jwt_manager_with_audience() {
        let config = test_config().with_audience("api.example.com");
        let manager = JwtManager::new(config);
        let user_id = UserId::new();
        let claims = JwtClaims::new(&user_id, "test-issuer")
            .with_audience("api.example.com");
        let token = manager.create_token_with_claims(&claims);
        assert!(token.is_ok());
        let token = token.ok().unwrap();
        let result = manager.validate_token(&token);
        assert!(result.is_ok());
    }

    // ===================
    // Header Extraction Tests
    // ===================

    #[test]
    fn extract_jwt_bearer() {
        let header = "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.test";
        let token = extract_jwt_from_header(header);
        assert!(token.is_ok());
        assert_eq!(
            token.ok().unwrap(),
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.test"
        );
    }

    #[test]
    fn extract_jwt_bearer_with_whitespace() {
        let header = "  Bearer   eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.test  ";
        let token = extract_jwt_from_header(header);
        assert!(token.is_ok());
    }

    #[test]
    fn extract_jwt_empty() {
        let token = extract_jwt_from_header("");
        assert!(token.is_err());
    }

    #[test]
    fn extract_jwt_no_bearer() {
        let token = extract_jwt_from_header("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.test");
        assert!(token.is_err());
    }

    #[test]
    fn extract_jwt_wrong_scheme() {
        let token = extract_jwt_from_header("Basic abc123");
        assert!(token.is_err());
    }
}
