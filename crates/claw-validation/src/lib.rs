//! Centralized input validation and sanitization for Clawbernetes.
//!
//! This crate provides a comprehensive set of validators for all user input
//! entering the system, ensuring security and consistency across all crates.
//!
//! # Security Features
//!
//! - **Command injection prevention**: Safe command builder that validates all arguments
//! - **Path traversal protection**: Blocks `..` and absolute paths via [`safe_path::SafePath`]
//! - **Shell metacharacter rejection**: Blocks `;`, `|`, `&`, `$`, etc.
//! - **Type-safe wrappers**: `Sanitized<T>` types prove validation was performed
//!
//! # Path Security
//!
//! For secure file path handling, use [`safe_path::SafePath`] and
//! [`safe_path::PathSandbox`] to prevent path traversal attacks:
//!
//! ```
//! use claw_validation::safe_path::{SafePath, PathSandbox};
//!
//! // Basic path validation
//! let safe = SafePath::new("config/app.toml")?;
//!
//! // Path traversal is blocked
//! assert!(SafePath::new("../etc/passwd").is_err());
//! # Ok::<(), claw_validation::ValidationError>(())
//! ```
//!
//! # String Validation
//!
//! ```
//! use claw_validation::{sanitize_node_name, validate_port};
//!
//! // Validate a node name
//! let node = sanitize_node_name("my-node-01")?;
//! assert_eq!(node.as_str(), "my-node-01");
//!
//! // Validate a port
//! let port = validate_port(8080)?;
//! assert_eq!(port.value(), 8080);
//! # Ok::<(), claw_validation::ValidationError>(())
//! ```
//!
//! # Safe Command Execution
//!
//! For executing external commands safely, use the `command` module (requires `command` feature):
//!
//! ```rust,no_run
//! # #[cfg(feature = "command")]
//! # async fn example() -> Result<(), claw_validation::command::CommandError> {
//! use claw_validation::command::{SafeCommand, AllowedProgram};
//!
//! let output = SafeCommand::new(AllowedProgram::Tailscale)
//!     .arg("status")
//!     .arg("--json")
//!     .execute()
//!     .await?;
//! # Ok(())
//! # }
//! ```

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod builder;
#[cfg(feature = "command")]
pub mod command;
mod error;
mod numeric;
pub mod safe_path;
mod sanitized;
mod strings;

pub use builder::ValidationBuilder;
#[cfg(feature = "command")]
pub use command::{AllowedProgram, CommandError, CommandOutput, SafeCommand};
pub use error::{ValidationError, ValidationErrorKind};
pub use numeric::{
    validate_memory_limit, validate_port, validate_timeout, ValidatedMemoryLimit, ValidatedPort,
    ValidatedTimeout,
};
pub use safe_path::{PathSandbox, SafePath};
pub use sanitized::{
    Command as CommandMarker, CommandArg, EnvKey, EnvValue, Hostname, ImageName, JobId, NodeName,
    Path, Sanitized, SanitizationKind, ServiceName, Url, ValidatedValue,
};
pub use strings::{
    sanitize_command, sanitize_command_arg, sanitize_env_var, sanitize_hostname,
    sanitize_image_name, sanitize_job_id, sanitize_node_name, sanitize_path, sanitize_service_name,
    sanitize_url,
};

/// Maximum length for node names.
pub const MAX_NODE_NAME_LENGTH: usize = 64;

/// Maximum length for environment variable keys.
pub const MAX_ENV_KEY_LENGTH: usize = 256;

/// Maximum length for environment variable values.
pub const MAX_ENV_VALUE_LENGTH: usize = 32768;

/// Maximum length for image names.
pub const MAX_IMAGE_NAME_LENGTH: usize = 256;

/// Maximum length for commands.
pub const MAX_COMMAND_LENGTH: usize = 4096;

/// Maximum length for paths.
pub const MAX_PATH_LENGTH: usize = 4096;

/// Maximum memory limit (1 TiB in bytes).
pub const MAX_MEMORY_LIMIT: u64 = 1024 * 1024 * 1024 * 1024;

/// Minimum memory limit (1 MiB in bytes).
pub const MIN_MEMORY_LIMIT: u64 = 1024 * 1024;

/// Maximum timeout duration (7 days in seconds).
pub const MAX_TIMEOUT_SECONDS: u64 = 7 * 24 * 60 * 60;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_name_sanitization() {
        // Valid names
        assert!(sanitize_node_name("node1").is_ok());
        assert!(sanitize_node_name("my-node").is_ok());
        assert!(sanitize_node_name("node_123").is_ok());

        // Invalid names
        assert!(sanitize_node_name("").is_err());
        assert!(sanitize_node_name("; rm -rf /").is_err());
        assert!(sanitize_node_name("node\0name").is_err());
    }

    #[test]
    fn test_path_sanitization() {
        // Valid paths
        assert!(sanitize_path("data/file.txt").is_ok());
        assert!(sanitize_path("config.yaml").is_ok());

        // Invalid paths (traversal attempts)
        assert!(sanitize_path("../../../etc/passwd").is_err());
        assert!(sanitize_path("/etc/passwd").is_err());
    }

    #[test]
    fn test_command_injection_prevention() {
        // Valid command
        assert!(sanitize_command("python").is_ok());

        // Command with path traversal
        assert!(sanitize_command("../bin/sh").is_err());
    }

    #[test]
    fn test_hostname_validation() {
        assert!(sanitize_hostname("localhost").is_ok());
        assert!(sanitize_hostname("my-host.example.com").is_ok());
        assert!(sanitize_hostname("host; cat /etc/passwd").is_err());
    }

    #[test]
    fn test_service_name_validation() {
        assert!(sanitize_service_name("my-api").is_ok());
        assert!(sanitize_service_name("api-v2").is_ok());
        assert!(sanitize_service_name("API").is_err()); // uppercase
        assert!(sanitize_service_name("-api").is_err()); // starts with dash
    }

    #[test]
    fn test_url_validation() {
        assert!(sanitize_url("http://localhost:8080").is_ok());
        assert!(sanitize_url("https://example.com").is_ok());
        assert!(sanitize_url("tcp://localhost:5432").is_ok());

        // Invalid URLs
        assert!(sanitize_url("not-a-url").is_err());
        assert!(sanitize_url("http://example.com; rm -rf /").is_err());
    }

    #[test]
    fn test_port_validation() {
        assert!(validate_port(80).is_ok());
        assert!(validate_port(443).is_ok());
        assert!(validate_port(8080).is_ok());
        assert!(validate_port(0).is_err());
    }

    #[test]
    fn test_validation_builder() {
        let result = ValidationBuilder::new()
            .validate_not_empty("name", "test")
            .validate_no_shell_chars("name", "test")
            .finish();

        assert!(result.is_ok());
    }

    #[test]
    fn test_validation_builder_with_errors() {
        let result = ValidationBuilder::new()
            .validate_not_empty("name", "")
            .validate_no_shell_chars("cmd", "ls; rm")
            .finish();

        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn test_sanitized_type_safety() {
        let node = sanitize_node_name("test").unwrap_or_else(|_| Sanitized::new(String::new()));
        let path = sanitize_path("some/path").unwrap_or_else(|_| Sanitized::new(String::new()));

        // These are different types and can't be mixed
        fn takes_node(_: Sanitized<NodeName>) {}
        fn takes_path(_: Sanitized<Path>) {}

        takes_node(node);
        takes_path(path);
    }

    #[test]
    fn test_image_name_validation() {
        assert!(sanitize_image_name("nginx").is_ok());
        assert!(sanitize_image_name("nginx:latest").is_ok());
        assert!(sanitize_image_name("gcr.io/project/image:v1").is_ok());

        // Injection attempts
        assert!(sanitize_image_name("image; rm -rf /").is_err());
        assert!(sanitize_image_name("$(whoami)").is_err());
    }

    #[test]
    fn test_env_var_validation() {
        assert!(sanitize_env_var("PATH", "/usr/bin").is_ok());
        assert!(sanitize_env_var("MY_VAR", "value").is_ok());

        // Invalid key
        assert!(sanitize_env_var("123INVALID", "value").is_err());
        assert!(sanitize_env_var("", "value").is_err());
    }

    #[test]
    fn test_safe_path_integration() {
        // SafePath rejects traversal
        assert!(SafePath::new("../etc/passwd").is_err());
        assert!(SafePath::new("config/../../../etc/passwd").is_err());

        // SafePath accepts valid paths
        assert!(SafePath::new("config/app.toml").is_ok());
        assert!(SafePath::new("data/file.txt").is_ok());
    }
}
