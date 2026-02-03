//! Error types for Tailscale operations.
//!
//! This module defines all error types that can occur when interacting with
//! Tailscale, including CLI operations, local API calls, and service management.

use std::path::PathBuf;
use thiserror::Error;

/// Result type alias for Tailscale operations.
pub type Result<T> = std::result::Result<T, TailscaleError>;

/// Errors that can occur during Tailscale operations.
#[derive(Debug, Error)]
pub enum TailscaleError {
    /// Tailscale CLI is not installed or not in PATH.
    #[error("tailscale not installed: {message}")]
    NotInstalled {
        /// Additional context about the error.
        message: String,
    },

    /// Tailscale daemon (tailscaled) is not running.
    #[error("tailscaled not running: {message}")]
    NotRunning {
        /// Additional context about the error.
        message: String,
    },

    /// Authentication failed (invalid auth key, expired, etc.).
    #[error("authentication failed: {reason}")]
    AuthFailed {
        /// Reason for the authentication failure.
        reason: String,
    },

    /// Invalid auth key format.
    #[error("invalid auth key format: {message}")]
    InvalidAuthKey {
        /// Description of the format error.
        message: String,
    },

    /// Auth key not found (file doesn't exist, env var not set, etc.).
    #[error("auth key not found: {location}")]
    AuthKeyNotFound {
        /// Where the auth key was expected.
        location: String,
    },

    /// Service-related errors (advertising, draining, discovery).
    #[error("service error: {operation} failed for '{service_name}': {reason}")]
    ServiceError {
        /// The service name involved.
        service_name: String,
        /// The operation that failed.
        operation: String,
        /// Reason for the failure.
        reason: String,
    },

    /// Tailscale Services feature not available (requires v1.94+).
    #[error("tailscale services not available: requires version 1.94 or later, found {version}")]
    ServicesNotAvailable {
        /// The current Tailscale version.
        version: String,
    },

    /// Local API error (socket communication, HTTP errors).
    #[error("local API error: {message}")]
    ApiError {
        /// Description of the API error.
        message: String,
    },

    /// Socket path does not exist or is inaccessible.
    #[error("socket not found: {path}")]
    SocketNotFound {
        /// Path to the expected socket.
        path: PathBuf,
    },

    /// Command execution error.
    #[error("command failed: {command} exited with {exit_code}: {stderr}")]
    CommandFailed {
        /// The command that was executed.
        command: String,
        /// Exit code of the command.
        exit_code: i32,
        /// Standard error output.
        stderr: String,
    },

    /// Connection to tailnet failed.
    #[error("connection failed: {reason}")]
    ConnectionFailed {
        /// Reason for the connection failure.
        reason: String,
    },

    /// Node is already connected.
    #[error("already connected: node is already connected to tailnet '{tailnet}'")]
    AlreadyConnected {
        /// The tailnet name.
        tailnet: String,
    },

    /// Node is not connected.
    #[error("not connected: node is not connected to any tailnet")]
    NotConnected,

    /// Invalid hostname format.
    #[error("invalid hostname: {message}")]
    InvalidHostname {
        /// Description of the format error.
        message: String,
    },

    /// Timeout waiting for operation to complete.
    #[error("timeout: {operation} did not complete within {timeout_secs} seconds")]
    Timeout {
        /// The operation that timed out.
        operation: String,
        /// Timeout duration in seconds.
        timeout_secs: u64,
    },

    /// JSON parsing error from API responses.
    #[error("json parse error: {message}")]
    JsonParse {
        /// Description of the parse error.
        message: String,
    },

    /// IO error (file operations, process I/O).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl TailscaleError {
    /// Creates a `NotInstalled` error with a message.
    #[must_use]
    pub fn not_installed(message: impl Into<String>) -> Self {
        Self::NotInstalled {
            message: message.into(),
        }
    }

    /// Creates a `NotRunning` error with a message.
    #[must_use]
    pub fn not_running(message: impl Into<String>) -> Self {
        Self::NotRunning {
            message: message.into(),
        }
    }

    /// Creates an `AuthFailed` error with a reason.
    #[must_use]
    pub fn auth_failed(reason: impl Into<String>) -> Self {
        Self::AuthFailed {
            reason: reason.into(),
        }
    }

    /// Creates an `InvalidAuthKey` error with a message.
    #[must_use]
    pub fn invalid_auth_key(message: impl Into<String>) -> Self {
        Self::InvalidAuthKey {
            message: message.into(),
        }
    }

    /// Creates an `AuthKeyNotFound` error with the location.
    #[must_use]
    pub fn auth_key_not_found(location: impl Into<String>) -> Self {
        Self::AuthKeyNotFound {
            location: location.into(),
        }
    }

    /// Creates a `ServiceError` with the service name, operation, and reason.
    #[must_use]
    pub fn service_error(
        service_name: impl Into<String>,
        operation: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::ServiceError {
            service_name: service_name.into(),
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    /// Creates a `ServicesNotAvailable` error with the current version.
    #[must_use]
    pub fn services_not_available(version: impl Into<String>) -> Self {
        Self::ServicesNotAvailable {
            version: version.into(),
        }
    }

    /// Creates an `ApiError` with a message.
    #[must_use]
    pub fn api_error(message: impl Into<String>) -> Self {
        Self::ApiError {
            message: message.into(),
        }
    }

    /// Creates a `SocketNotFound` error with the path.
    #[must_use]
    pub fn socket_not_found(path: impl Into<PathBuf>) -> Self {
        Self::SocketNotFound { path: path.into() }
    }

    /// Creates a `CommandFailed` error.
    #[must_use]
    pub fn command_failed(
        command: impl Into<String>,
        exit_code: i32,
        stderr: impl Into<String>,
    ) -> Self {
        Self::CommandFailed {
            command: command.into(),
            exit_code,
            stderr: stderr.into(),
        }
    }

    /// Creates an `AlreadyConnected` error with the tailnet name.
    #[must_use]
    pub fn already_connected(tailnet: impl Into<String>) -> Self {
        Self::AlreadyConnected {
            tailnet: tailnet.into(),
        }
    }

    /// Creates an `InvalidHostname` error with a message.
    #[must_use]
    pub fn invalid_hostname(message: impl Into<String>) -> Self {
        Self::InvalidHostname {
            message: message.into(),
        }
    }

    /// Creates a `Timeout` error.
    #[must_use]
    pub fn timeout(operation: impl Into<String>, timeout_secs: u64) -> Self {
        Self::Timeout {
            operation: operation.into(),
            timeout_secs,
        }
    }

    /// Creates a `JsonParse` error with a message.
    #[must_use]
    pub fn json_parse(message: impl Into<String>) -> Self {
        Self::JsonParse {
            message: message.into(),
        }
    }

    /// Returns `true` if this error indicates a recoverable condition.
    #[must_use]
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::NotRunning { .. }
                | Self::NotConnected
                | Self::Timeout { .. }
                | Self::ApiError { .. }
        )
    }

    /// Returns `true` if this error indicates a configuration problem.
    #[must_use]
    pub fn is_configuration_error(&self) -> bool {
        matches!(
            self,
            Self::NotInstalled { .. }
                | Self::InvalidAuthKey { .. }
                | Self::AuthKeyNotFound { .. }
                | Self::SocketNotFound { .. }
                | Self::InvalidHostname { .. }
        )
    }

    /// Returns `true` if this error indicates an authentication problem.
    #[must_use]
    pub fn is_auth_error(&self) -> bool {
        matches!(
            self,
            Self::AuthFailed { .. }
                | Self::InvalidAuthKey { .. }
                | Self::AuthKeyNotFound { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_installed_error_display() {
        let err = TailscaleError::not_installed("command not found");
        assert_eq!(
            err.to_string(),
            "tailscale not installed: command not found"
        );
    }

    #[test]
    fn test_not_running_error_display() {
        let err = TailscaleError::not_running("daemon not responding");
        assert_eq!(
            err.to_string(),
            "tailscaled not running: daemon not responding"
        );
    }

    #[test]
    fn test_auth_failed_error_display() {
        let err = TailscaleError::auth_failed("key expired");
        assert_eq!(err.to_string(), "authentication failed: key expired");
    }

    #[test]
    fn test_invalid_auth_key_error_display() {
        let err = TailscaleError::invalid_auth_key("missing tskey- prefix");
        assert_eq!(
            err.to_string(),
            "invalid auth key format: missing tskey- prefix"
        );
    }

    #[test]
    fn test_auth_key_not_found_error_display() {
        let err = TailscaleError::auth_key_not_found("TS_AUTHKEY environment variable");
        assert_eq!(
            err.to_string(),
            "auth key not found: TS_AUTHKEY environment variable"
        );
    }

    #[test]
    fn test_service_error_display() {
        let err = TailscaleError::service_error("my-service", "advertise", "port already in use");
        assert_eq!(
            err.to_string(),
            "service error: advertise failed for 'my-service': port already in use"
        );
    }

    #[test]
    fn test_services_not_available_error_display() {
        let err = TailscaleError::services_not_available("1.92.0");
        assert_eq!(
            err.to_string(),
            "tailscale services not available: requires version 1.94 or later, found 1.92.0"
        );
    }

    #[test]
    fn test_api_error_display() {
        let err = TailscaleError::api_error("connection refused");
        assert_eq!(err.to_string(), "local API error: connection refused");
    }

    #[test]
    fn test_socket_not_found_error_display() {
        let err = TailscaleError::socket_not_found("/var/run/tailscale/tailscaled.sock");
        assert_eq!(
            err.to_string(),
            "socket not found: /var/run/tailscale/tailscaled.sock"
        );
    }

    #[test]
    fn test_command_failed_error_display() {
        let err = TailscaleError::command_failed("tailscale up", 1, "authentication required");
        assert_eq!(
            err.to_string(),
            "command failed: tailscale up exited with 1: authentication required"
        );
    }

    #[test]
    fn test_already_connected_error_display() {
        let err = TailscaleError::already_connected("mycompany.com");
        assert_eq!(
            err.to_string(),
            "already connected: node is already connected to tailnet 'mycompany.com'"
        );
    }

    #[test]
    fn test_not_connected_error_display() {
        let err = TailscaleError::NotConnected;
        assert_eq!(
            err.to_string(),
            "not connected: node is not connected to any tailnet"
        );
    }

    #[test]
    fn test_invalid_hostname_error_display() {
        let err = TailscaleError::invalid_hostname("contains invalid characters");
        assert_eq!(
            err.to_string(),
            "invalid hostname: contains invalid characters"
        );
    }

    #[test]
    fn test_timeout_error_display() {
        let err = TailscaleError::timeout("connection", 30);
        assert_eq!(
            err.to_string(),
            "timeout: connection did not complete within 30 seconds"
        );
    }

    #[test]
    fn test_json_parse_error_display() {
        let err = TailscaleError::json_parse("unexpected end of input");
        assert_eq!(
            err.to_string(),
            "json parse error: unexpected end of input"
        );
    }

    #[test]
    fn test_io_error_from_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let ts_err: TailscaleError = io_err.into();
        assert!(matches!(ts_err, TailscaleError::Io(_)));
        assert!(ts_err.to_string().contains("file not found"));
    }

    #[test]
    fn test_is_recoverable() {
        // Recoverable errors
        assert!(TailscaleError::not_running("test").is_recoverable());
        assert!(TailscaleError::NotConnected.is_recoverable());
        assert!(TailscaleError::timeout("test", 30).is_recoverable());
        assert!(TailscaleError::api_error("test").is_recoverable());

        // Non-recoverable errors
        assert!(!TailscaleError::not_installed("test").is_recoverable());
        assert!(!TailscaleError::invalid_auth_key("test").is_recoverable());
        assert!(!TailscaleError::auth_failed("test").is_recoverable());
    }

    #[test]
    fn test_is_configuration_error() {
        // Configuration errors
        assert!(TailscaleError::not_installed("test").is_configuration_error());
        assert!(TailscaleError::invalid_auth_key("test").is_configuration_error());
        assert!(TailscaleError::auth_key_not_found("test").is_configuration_error());
        assert!(TailscaleError::socket_not_found("/path").is_configuration_error());
        assert!(TailscaleError::invalid_hostname("test").is_configuration_error());

        // Non-configuration errors
        assert!(!TailscaleError::auth_failed("test").is_configuration_error());
        assert!(!TailscaleError::NotConnected.is_configuration_error());
        assert!(!TailscaleError::timeout("test", 30).is_configuration_error());
    }

    #[test]
    fn test_is_auth_error() {
        // Auth errors
        assert!(TailscaleError::auth_failed("test").is_auth_error());
        assert!(TailscaleError::invalid_auth_key("test").is_auth_error());
        assert!(TailscaleError::auth_key_not_found("test").is_auth_error());

        // Non-auth errors
        assert!(!TailscaleError::not_installed("test").is_auth_error());
        assert!(!TailscaleError::NotConnected.is_auth_error());
        assert!(!TailscaleError::api_error("test").is_auth_error());
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TailscaleError>();
    }

    #[test]
    fn test_result_type_alias() {
        fn returns_result() -> Result<u32> {
            Ok(42)
        }
        fn returns_error() -> Result<u32> {
            Err(TailscaleError::NotConnected)
        }
        assert!(returns_result().is_ok());
        assert!(returns_error().is_err());
    }
}
