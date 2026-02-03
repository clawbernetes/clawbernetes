//! Authentication methods for Tailscale.
//!
//! This module provides various ways to authenticate with Tailscale:
//! - Direct auth key
//! - Auth key from file
//! - Auth key from environment variable
//! - Workload identity (for cloud environments)
//!
//! # Auth Key Format
//!
//! Tailscale auth keys follow the format: `tskey-auth-<random>-<random>`
//! or `tskey-<random>` for older formats. The prefix varies:
//! - `tskey-auth-` - Standard auth keys
//! - `tskey-client-` - Client keys (OAuth)
//! - `tskey-` - Legacy format
//!
//! # Example
//!
//! ```rust,no_run
//! use claw_tailscale::auth::{AuthMethod, resolve_auth_key, validate_auth_key_format};
//!
//! // From environment variable
//! let method = AuthMethod::AuthKeyEnv {
//!     var_name: "TS_AUTHKEY".to_string(),
//! };
//!
//! // Note: resolve_auth_key is async
//! // let key = resolve_auth_key(&method).await?;
//! ```

use crate::error::{Result, TailscaleError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Authentication methods for connecting to a Tailscale network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthMethod {
    /// Direct auth key value.
    AuthKey {
        /// The auth key string.
        key: String,
    },

    /// Auth key read from a file.
    AuthKeyFile {
        /// Path to the file containing the auth key.
        path: PathBuf,
    },

    /// Auth key from an environment variable.
    AuthKeyEnv {
        /// Name of the environment variable.
        var_name: String,
    },

    /// Workload identity for cloud environments (GCP, AWS, Azure).
    /// Uses automatic authentication based on cloud provider metadata.
    WorkloadIdentity {
        /// Optional provider hint (e.g., "gcp", "aws", "azure").
        /// If not specified, will attempt to auto-detect.
        provider: Option<String>,
    },
}

impl AuthMethod {
    /// Creates an `AuthKey` method with the given key.
    #[must_use]
    pub fn auth_key(key: impl Into<String>) -> Self {
        Self::AuthKey { key: key.into() }
    }

    /// Creates an `AuthKeyFile` method with the given path.
    #[must_use]
    pub fn auth_key_file(path: impl Into<PathBuf>) -> Self {
        Self::AuthKeyFile { path: path.into() }
    }

    /// Creates an `AuthKeyEnv` method with the given variable name.
    #[must_use]
    pub fn auth_key_env(var_name: impl Into<String>) -> Self {
        Self::AuthKeyEnv {
            var_name: var_name.into(),
        }
    }

    /// Creates a `WorkloadIdentity` method with an optional provider hint.
    #[must_use]
    pub fn workload_identity(provider: Option<String>) -> Self {
        Self::WorkloadIdentity { provider }
    }

    /// Creates a `WorkloadIdentity` method that auto-detects the provider.
    #[must_use]
    pub fn workload_identity_auto() -> Self {
        Self::WorkloadIdentity { provider: None }
    }
}

impl Default for AuthMethod {
    /// Default to reading from the `TS_AUTHKEY` environment variable.
    fn default() -> Self {
        Self::AuthKeyEnv {
            var_name: "TS_AUTHKEY".to_string(),
        }
    }
}

/// Minimum length for a valid auth key (tskey- prefix + some random chars).
const MIN_AUTH_KEY_LENGTH: usize = 10;

/// Known valid auth key prefixes.
const VALID_PREFIXES: &[&str] = &["tskey-auth-", "tskey-client-", "tskey-"];

/// Validates that an auth key has a valid format.
///
/// This performs basic format validation:
/// - Must start with a valid prefix (`tskey-auth-`, `tskey-client-`, or `tskey-`)
/// - Must have minimum length
/// - Must only contain valid characters (alphanumeric, dash)
///
/// Note: This does NOT validate that the key is actually valid with Tailscale
/// servers - that happens during authentication.
///
/// # Errors
///
/// Returns `TailscaleError::InvalidAuthKey` if the format is invalid.
pub fn validate_auth_key_format(key: &str) -> Result<()> {
    let key = key.trim();

    // Check minimum length
    if key.len() < MIN_AUTH_KEY_LENGTH {
        return Err(TailscaleError::invalid_auth_key(format!(
            "key too short (minimum {MIN_AUTH_KEY_LENGTH} characters)"
        )));
    }

    // Check for valid prefix
    let has_valid_prefix = VALID_PREFIXES.iter().any(|prefix| key.starts_with(prefix));
    if !has_valid_prefix {
        return Err(TailscaleError::invalid_auth_key(
            "key must start with 'tskey-auth-', 'tskey-client-', or 'tskey-'",
        ));
    }

    // Check for valid characters (alphanumeric and dash)
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(TailscaleError::invalid_auth_key(
            "key contains invalid characters (only alphanumeric and dash allowed)",
        ));
    }

    // Check that the key has content after the prefix
    let has_content = VALID_PREFIXES.iter().any(|prefix| {
        if let Some(suffix) = key.strip_prefix(prefix) {
            !suffix.is_empty()
        } else {
            false
        }
    });

    if !has_content {
        return Err(TailscaleError::invalid_auth_key(
            "key has no content after prefix",
        ));
    }

    Ok(())
}

/// Resolves an auth key from the given authentication method.
///
/// This function handles all the different ways to obtain an auth key:
/// - Direct key: validates and returns it
/// - File: reads the file and validates the key
/// - Environment variable: reads the env var and validates the key
/// - Workload identity: returns an error (use workload identity flow instead)
///
/// # Errors
///
/// - `TailscaleError::InvalidAuthKey` if the key format is invalid
/// - `TailscaleError::AuthKeyNotFound` if the source doesn't exist
/// - `TailscaleError::Io` for file I/O errors
pub async fn resolve_auth_key(method: &AuthMethod) -> Result<String> {
    match method {
        AuthMethod::AuthKey { key } => {
            validate_auth_key_format(key)?;
            Ok(key.trim().to_string())
        }

        AuthMethod::AuthKeyFile { path } => {
            if !path.exists() {
                return Err(TailscaleError::auth_key_not_found(format!(
                    "file not found: {}",
                    path.display()
                )));
            }

            let content = tokio::fs::read_to_string(path).await.map_err(|e| {
                TailscaleError::auth_key_not_found(format!(
                    "failed to read file {}: {}",
                    path.display(),
                    e
                ))
            })?;

            let key = content.trim().to_string();
            validate_auth_key_format(&key)?;
            Ok(key)
        }

        AuthMethod::AuthKeyEnv { var_name } => {
            let key = std::env::var(var_name).map_err(|_| {
                TailscaleError::auth_key_not_found(format!(
                    "environment variable '{var_name}' not set"
                ))
            })?;

            let key = key.trim().to_string();
            validate_auth_key_format(&key)?;
            Ok(key)
        }

        AuthMethod::WorkloadIdentity { provider } => {
            // Workload identity doesn't use auth keys in the traditional sense.
            // The tailscale CLI handles this internally when --authkey is not provided
            // and the environment supports workload identity.
            let provider_msg = provider
                .as_ref()
                .map_or("auto-detect".to_string(), std::clone::Clone::clone);

            Err(TailscaleError::auth_failed(format!(
                "workload identity ({provider_msg}) should be used via tailscale CLI, not resolved as key"
            )))
        }
    }
}

/// Checks if the given auth method requires an explicit auth key.
///
/// Workload identity does not require an explicit key - the tailscale daemon
/// handles authentication automatically.
#[must_use]
pub fn requires_explicit_key(method: &AuthMethod) -> bool {
    !matches!(method, AuthMethod::WorkloadIdentity { .. })
}

/// Returns the default auth key file path for the current platform.
#[must_use]
pub fn default_auth_key_file_path() -> PathBuf {
    if cfg!(target_os = "linux") {
        PathBuf::from("/etc/tailscale/authkey")
    } else if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Tailscale/authkey")
    } else if cfg!(target_os = "windows") {
        PathBuf::from(r"C:\ProgramData\Tailscale\authkey")
    } else {
        // Fallback for other platforms
        PathBuf::from("/etc/tailscale/authkey")
    }
}

/// Returns the default environment variable name for auth keys.
#[must_use]
pub fn default_auth_key_env_var() -> &'static str {
    "TS_AUTHKEY"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // ============= AuthMethod Tests =============

    #[test]
    fn test_auth_method_auth_key_constructor() {
        let method = AuthMethod::auth_key("tskey-auth-test123");
        assert!(matches!(method, AuthMethod::AuthKey { key } if key == "tskey-auth-test123"));
    }

    #[test]
    fn test_auth_method_auth_key_file_constructor() {
        let method = AuthMethod::auth_key_file("/path/to/key");
        assert!(matches!(method, AuthMethod::AuthKeyFile { path } if path == PathBuf::from("/path/to/key")));
    }

    #[test]
    fn test_auth_method_auth_key_env_constructor() {
        let method = AuthMethod::auth_key_env("MY_TS_KEY");
        assert!(matches!(method, AuthMethod::AuthKeyEnv { var_name } if var_name == "MY_TS_KEY"));
    }

    #[test]
    fn test_auth_method_workload_identity_constructor() {
        let method = AuthMethod::workload_identity(Some("gcp".to_string()));
        assert!(matches!(method, AuthMethod::WorkloadIdentity { provider } if provider == Some("gcp".to_string())));
    }

    #[test]
    fn test_auth_method_workload_identity_auto() {
        let method = AuthMethod::workload_identity_auto();
        assert!(matches!(
            method,
            AuthMethod::WorkloadIdentity { provider: None }
        ));
    }

    #[test]
    fn test_auth_method_default() {
        let method = AuthMethod::default();
        assert!(matches!(method, AuthMethod::AuthKeyEnv { var_name } if var_name == "TS_AUTHKEY"));
    }

    #[test]
    fn test_auth_method_serialization() {
        let method = AuthMethod::auth_key("tskey-auth-test123");
        let json = serde_json::to_string(&method).expect("should serialize");
        assert!(json.contains("auth_key"));
        assert!(json.contains("tskey-auth-test123"));
    }

    #[test]
    fn test_auth_method_deserialization() {
        let json = r#"{"type":"auth_key","key":"tskey-auth-test123"}"#;
        let method: AuthMethod = serde_json::from_str(json).expect("should deserialize");
        assert!(matches!(method, AuthMethod::AuthKey { key } if key == "tskey-auth-test123"));
    }

    #[test]
    fn test_auth_method_file_serialization() {
        let method = AuthMethod::auth_key_file("/etc/tailscale/key");
        let json = serde_json::to_string(&method).expect("should serialize");
        assert!(json.contains("auth_key_file"));
        assert!(json.contains("/etc/tailscale/key"));
    }

    #[test]
    fn test_auth_method_env_serialization() {
        let method = AuthMethod::auth_key_env("TS_AUTHKEY");
        let json = serde_json::to_string(&method).expect("should serialize");
        assert!(json.contains("auth_key_env"));
        assert!(json.contains("TS_AUTHKEY"));
    }

    #[test]
    fn test_auth_method_workload_identity_serialization() {
        let method = AuthMethod::workload_identity(Some("aws".to_string()));
        let json = serde_json::to_string(&method).expect("should serialize");
        assert!(json.contains("workload_identity"));
        assert!(json.contains("aws"));
    }

    #[test]
    fn test_auth_method_equality() {
        let method1 = AuthMethod::auth_key("tskey-auth-test123");
        let method2 = AuthMethod::auth_key("tskey-auth-test123");
        let method3 = AuthMethod::auth_key("tskey-auth-different");
        assert_eq!(method1, method2);
        assert_ne!(method1, method3);
    }

    #[test]
    fn test_auth_method_clone() {
        let method = AuthMethod::auth_key("tskey-auth-test123");
        let cloned = method.clone();
        assert_eq!(method, cloned);
    }

    // ============= validate_auth_key_format Tests =============

    #[test]
    fn test_validate_valid_auth_key() {
        assert!(validate_auth_key_format("tskey-auth-kExample12345").is_ok());
    }

    #[test]
    fn test_validate_valid_client_key() {
        assert!(validate_auth_key_format("tskey-client-kExample12345").is_ok());
    }

    #[test]
    fn test_validate_valid_legacy_key() {
        assert!(validate_auth_key_format("tskey-kExample12345").is_ok());
    }

    #[test]
    fn test_validate_key_with_dashes() {
        assert!(validate_auth_key_format("tskey-auth-abc-def-123").is_ok());
    }

    #[test]
    fn test_validate_key_with_whitespace_trimming() {
        assert!(validate_auth_key_format("  tskey-auth-test123  ").is_ok());
    }

    #[test]
    fn test_validate_key_too_short() {
        let result = validate_auth_key_format("tskey-");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TailscaleError::InvalidAuthKey { .. }));
        assert!(err.to_string().contains("too short"));
    }

    #[test]
    fn test_validate_key_wrong_prefix() {
        let result = validate_auth_key_format("wrong-auth-test123");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TailscaleError::InvalidAuthKey { .. }));
        assert!(err.to_string().contains("must start with"));
    }

    #[test]
    fn test_validate_key_invalid_characters() {
        let result = validate_auth_key_format("tskey-auth-test@123!");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TailscaleError::InvalidAuthKey { .. }));
        assert!(err.to_string().contains("invalid characters"));
    }

    #[test]
    fn test_validate_key_just_prefix() {
        // "tskey-" alone is too short (6 chars < 10)
        let result = validate_auth_key_format("tskey-");
        assert!(result.is_err());
        // "tskey-auth-" (11 chars) is valid because it also matches "tskey-" prefix
        // with content "auth-" after it, which is valid
        let result2 = validate_auth_key_format("tskey-auth-x");
        assert!(result2.is_ok());
    }

    #[test]
    fn test_validate_empty_key() {
        let result = validate_auth_key_format("");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TailscaleError::InvalidAuthKey { .. }));
    }

    // ============= resolve_auth_key Tests =============

    #[tokio::test]
    async fn test_resolve_auth_key_direct() {
        let method = AuthMethod::auth_key("tskey-auth-test123456");
        let result = resolve_auth_key(&method).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "tskey-auth-test123456");
    }

    #[tokio::test]
    async fn test_resolve_auth_key_direct_with_whitespace() {
        let method = AuthMethod::auth_key("  tskey-auth-test123456  ");
        let result = resolve_auth_key(&method).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "tskey-auth-test123456");
    }

    #[tokio::test]
    async fn test_resolve_auth_key_direct_invalid() {
        let method = AuthMethod::auth_key("invalid-key");
        let result = resolve_auth_key(&method).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TailscaleError::InvalidAuthKey { .. }
        ));
    }

    #[tokio::test]
    async fn test_resolve_auth_key_from_file() {
        // Create a temp file with the auth key
        let mut temp_file = NamedTempFile::new().expect("should create temp file");
        writeln!(temp_file, "tskey-auth-filetest123").expect("should write");

        let method = AuthMethod::auth_key_file(temp_file.path());
        let result = resolve_auth_key(&method).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "tskey-auth-filetest123");
    }

    #[tokio::test]
    async fn test_resolve_auth_key_from_file_with_whitespace() {
        let mut temp_file = NamedTempFile::new().expect("should create temp file");
        writeln!(temp_file, "  tskey-auth-filetest123  \n").expect("should write");

        let method = AuthMethod::auth_key_file(temp_file.path());
        let result = resolve_auth_key(&method).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "tskey-auth-filetest123");
    }

    #[tokio::test]
    async fn test_resolve_auth_key_file_not_found() {
        let method = AuthMethod::auth_key_file("/nonexistent/path/to/key");
        let result = resolve_auth_key(&method).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TailscaleError::AuthKeyNotFound { .. }));
        assert!(err.to_string().contains("file not found"));
    }

    #[tokio::test]
    async fn test_resolve_auth_key_file_invalid_content() {
        let mut temp_file = NamedTempFile::new().expect("should create temp file");
        writeln!(temp_file, "invalid-key-content").expect("should write");

        let method = AuthMethod::auth_key_file(temp_file.path());
        let result = resolve_auth_key(&method).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TailscaleError::InvalidAuthKey { .. }
        ));
    }

    #[tokio::test]
    async fn test_resolve_auth_key_from_env() {
        // Set a test env variable
        unsafe { std::env::set_var("TEST_TS_AUTHKEY_VALID", "tskey-auth-envtest123") };

        let method = AuthMethod::auth_key_env("TEST_TS_AUTHKEY_VALID");
        let result = resolve_auth_key(&method).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "tskey-auth-envtest123");

        // Cleanup
        unsafe { std::env::remove_var("TEST_TS_AUTHKEY_VALID") };
    }

    #[tokio::test]
    async fn test_resolve_auth_key_env_not_set() {
        // Make sure the env var doesn't exist
        unsafe { std::env::remove_var("TEST_TS_AUTHKEY_NOTSET") };

        let method = AuthMethod::auth_key_env("TEST_TS_AUTHKEY_NOTSET");
        let result = resolve_auth_key(&method).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TailscaleError::AuthKeyNotFound { .. }));
        assert!(err.to_string().contains("not set"));
    }

    #[tokio::test]
    async fn test_resolve_auth_key_env_invalid() {
        unsafe { std::env::set_var("TEST_TS_AUTHKEY_INVALID", "not-a-valid-key") };

        let method = AuthMethod::auth_key_env("TEST_TS_AUTHKEY_INVALID");
        let result = resolve_auth_key(&method).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TailscaleError::InvalidAuthKey { .. }
        ));

        unsafe { std::env::remove_var("TEST_TS_AUTHKEY_INVALID") };
    }

    #[tokio::test]
    async fn test_resolve_auth_key_workload_identity() {
        let method = AuthMethod::workload_identity(Some("gcp".to_string()));
        let result = resolve_auth_key(&method).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TailscaleError::AuthFailed { .. }));
        assert!(err.to_string().contains("workload identity"));
    }

    #[tokio::test]
    async fn test_resolve_auth_key_workload_identity_auto() {
        let method = AuthMethod::workload_identity_auto();
        let result = resolve_auth_key(&method).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("auto-detect"));
    }

    // ============= requires_explicit_key Tests =============

    #[test]
    fn test_requires_explicit_key_auth_key() {
        let method = AuthMethod::auth_key("test");
        assert!(requires_explicit_key(&method));
    }

    #[test]
    fn test_requires_explicit_key_auth_key_file() {
        let method = AuthMethod::auth_key_file("/path");
        assert!(requires_explicit_key(&method));
    }

    #[test]
    fn test_requires_explicit_key_auth_key_env() {
        let method = AuthMethod::auth_key_env("VAR");
        assert!(requires_explicit_key(&method));
    }

    #[test]
    fn test_requires_explicit_key_workload_identity() {
        let method = AuthMethod::workload_identity(Some("gcp".to_string()));
        assert!(!requires_explicit_key(&method));
    }

    #[test]
    fn test_requires_explicit_key_workload_identity_auto() {
        let method = AuthMethod::workload_identity_auto();
        assert!(!requires_explicit_key(&method));
    }

    // ============= Path/Env Helper Tests =============

    #[test]
    fn test_default_auth_key_file_path() {
        let path = default_auth_key_file_path();
        // Just verify it returns a valid path
        assert!(!path.as_os_str().is_empty());
        assert!(path.to_string_lossy().contains("tailscale") || path.to_string_lossy().contains("Tailscale"));
    }

    #[test]
    fn test_default_auth_key_env_var() {
        assert_eq!(default_auth_key_env_var(), "TS_AUTHKEY");
    }
}
