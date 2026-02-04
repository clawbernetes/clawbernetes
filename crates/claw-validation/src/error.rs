//! Validation error types with detailed rejection reasons.

use std::fmt;
use std::path::PathBuf;
use thiserror::Error;

/// The kind of validation error that occurred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationErrorKind {
    /// Input was empty when a value was required.
    Empty,
    /// Input exceeded maximum allowed length.
    TooLong {
        /// Maximum allowed length.
        max: usize,
        /// Actual length of input.
        actual: usize,
    },
    /// Input was shorter than minimum required length.
    TooShort {
        /// Minimum required length.
        min: usize,
        /// Actual length of input.
        actual: usize,
    },
    /// Input contained invalid characters.
    InvalidCharacters {
        /// Description of invalid characters found.
        found: String,
        /// Description of allowed characters.
        allowed: String,
    },
    /// Input contained dangerous shell metacharacters.
    ShellInjection {
        /// The dangerous character found.
        found: char,
    },
    /// Input contained path traversal sequences.
    PathTraversal {
        /// The traversal pattern found.
        pattern: String,
    },
    /// Input contained an absolute path when not allowed.
    AbsolutePath,
    /// Input did not match expected format.
    InvalidFormat {
        /// Expected format description.
        expected: String,
        /// What was actually provided.
        actual: String,
    },
    /// Numeric value was out of allowed range.
    OutOfRange {
        /// Minimum allowed value.
        min: u64,
        /// Maximum allowed value.
        max: u64,
        /// Actual value provided.
        actual: u64,
    },
    /// UUID was malformed or invalid.
    InvalidUuid {
        /// The parsing error message.
        reason: String,
    },
    /// Docker image name was invalid.
    InvalidImageName {
        /// Reason the image name is invalid.
        reason: String,
    },
    /// Environment variable validation failed.
    InvalidEnvVar {
        /// Reason the environment variable is invalid.
        reason: String,
    },
    /// Input contained null bytes.
    NullByte,
    /// Input contained control characters.
    ControlCharacters,
    /// Path escapes sandbox directory.
    SandboxEscape,
    /// Symlink resolution failed or points outside sandbox.
    SymlinkEscape,
    /// Path canonicalization failed.
    CanonicalizationFailed,
}

impl fmt::Display for ValidationErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "input cannot be empty"),
            Self::TooLong { max, actual } => {
                write!(f, "input too long: {actual} chars exceeds max of {max}")
            }
            Self::TooShort { min, actual } => {
                write!(f, "input too short: {actual} chars below min of {min}")
            }
            Self::InvalidCharacters { found, allowed } => {
                write!(f, "invalid characters '{found}': allowed: {allowed}")
            }
            Self::ShellInjection { found } => {
                write!(f, "shell metacharacter '{found}' not allowed")
            }
            Self::PathTraversal { pattern } => {
                write!(f, "path traversal pattern '{pattern}' detected")
            }
            Self::AbsolutePath => write!(f, "absolute paths are not allowed"),
            Self::InvalidFormat { expected, actual } => {
                write!(f, "invalid format: expected {expected}, got '{actual}'")
            }
            Self::OutOfRange { min, max, actual } => {
                write!(f, "value {actual} out of range [{min}, {max}]")
            }
            Self::InvalidUuid { reason } => write!(f, "invalid UUID: {reason}"),
            Self::InvalidImageName { reason } => write!(f, "invalid image name: {reason}"),
            Self::InvalidEnvVar { reason } => write!(f, "invalid environment variable: {reason}"),
            Self::NullByte => write!(f, "input contains null byte"),
            Self::ControlCharacters => write!(f, "input contains control characters"),
            Self::SandboxEscape => write!(f, "path escapes sandbox"),
            Self::SymlinkEscape => write!(f, "symlink points outside sandbox"),
            Self::CanonicalizationFailed => write!(f, "path canonicalization failed"),
        }
    }
}

/// Error returned when validation fails.
#[derive(Debug, Clone, Error)]
#[error("validation failed for '{field}': {kind}")]
pub struct ValidationError {
    /// The name of the field that failed validation.
    pub field: String,
    /// The kind of validation error.
    pub kind: ValidationErrorKind,
    /// The offending path (if applicable).
    pub path: Option<PathBuf>,
}

impl ValidationError {
    /// Create a new validation error.
    #[must_use]
    pub fn new(field: impl Into<String>, kind: ValidationErrorKind) -> Self {
        Self {
            field: field.into(),
            kind,
            path: None,
        }
    }

    /// Create an "empty" validation error.
    #[must_use]
    pub fn empty(field: impl Into<String>) -> Self {
        Self::new(field, ValidationErrorKind::Empty)
    }

    /// Create a "too long" validation error.
    #[must_use]
    pub fn too_long(field: impl Into<String>, max: usize, actual: usize) -> Self {
        Self::new(field, ValidationErrorKind::TooLong { max, actual })
    }

    /// Create a "too short" validation error.
    #[must_use]
    pub fn too_short(field: impl Into<String>, min: usize, actual: usize) -> Self {
        Self::new(field, ValidationErrorKind::TooShort { min, actual })
    }

    /// Create an "invalid characters" validation error.
    #[must_use]
    pub fn invalid_characters(
        field: impl Into<String>,
        found: impl Into<String>,
        allowed: impl Into<String>,
    ) -> Self {
        Self::new(
            field,
            ValidationErrorKind::InvalidCharacters {
                found: found.into(),
                allowed: allowed.into(),
            },
        )
    }

    /// Create a "shell injection" validation error.
    #[must_use]
    pub fn shell_injection(field: impl Into<String>, found: char) -> Self {
        Self::new(field, ValidationErrorKind::ShellInjection { found })
    }

    /// Create a "path traversal" validation error.
    #[must_use]
    pub fn path_traversal(field: impl Into<String>, pattern: impl Into<String>) -> Self {
        Self::new(
            field,
            ValidationErrorKind::PathTraversal {
                pattern: pattern.into(),
            },
        )
    }

    /// Create an "absolute path" validation error.
    #[must_use]
    pub fn absolute_path(field: impl Into<String>) -> Self {
        Self::new(field, ValidationErrorKind::AbsolutePath)
    }

    /// Create an "invalid format" validation error.
    #[must_use]
    pub fn invalid_format(
        field: impl Into<String>,
        expected: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        Self::new(
            field,
            ValidationErrorKind::InvalidFormat {
                expected: expected.into(),
                actual: actual.into(),
            },
        )
    }

    /// Create an "out of range" validation error.
    #[must_use]
    pub fn out_of_range(field: impl Into<String>, min: u64, max: u64, actual: u64) -> Self {
        Self::new(field, ValidationErrorKind::OutOfRange { min, max, actual })
    }

    /// Create an "invalid UUID" validation error.
    #[must_use]
    pub fn invalid_uuid(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::new(
            field,
            ValidationErrorKind::InvalidUuid {
                reason: reason.into(),
            },
        )
    }

    /// Create an "invalid image name" validation error.
    #[must_use]
    pub fn invalid_image_name(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::new(
            field,
            ValidationErrorKind::InvalidImageName {
                reason: reason.into(),
            },
        )
    }

    /// Create an "invalid env var" validation error.
    #[must_use]
    pub fn invalid_env_var(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::new(
            field,
            ValidationErrorKind::InvalidEnvVar {
                reason: reason.into(),
            },
        )
    }

    /// Create a "null byte" validation error.
    #[must_use]
    pub fn null_byte(field: impl Into<String>) -> Self {
        Self::new(field, ValidationErrorKind::NullByte)
    }

    /// Create a "control characters" validation error.
    #[must_use]
    pub fn control_characters(field: impl Into<String>) -> Self {
        Self::new(field, ValidationErrorKind::ControlCharacters)
    }

    /// Create a "sandbox escape" validation error.
    #[must_use]
    pub fn sandbox_escape(field: impl Into<String>) -> Self {
        Self::new(field, ValidationErrorKind::SandboxEscape)
    }

    /// Create a "symlink escape" validation error.
    #[must_use]
    pub fn symlink_escape(field: impl Into<String>) -> Self {
        Self::new(field, ValidationErrorKind::SymlinkEscape)
    }

    /// Create a "canonicalization failed" validation error.
    #[must_use]
    pub fn canonicalization_failed(field: impl Into<String>) -> Self {
        Self::new(field, ValidationErrorKind::CanonicalizationFailed)
    }

    /// Set the path.
    #[must_use]
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Check if this is an empty error.
    #[must_use]
    pub fn is_empty_error(&self) -> bool {
        matches!(self.kind, ValidationErrorKind::Empty)
    }

    /// Check if this is a security-related error (injection, traversal).
    #[must_use]
    pub fn is_security_error(&self) -> bool {
        matches!(
            self.kind,
            ValidationErrorKind::ShellInjection { .. }
                | ValidationErrorKind::PathTraversal { .. }
                | ValidationErrorKind::NullByte
                | ValidationErrorKind::ControlCharacters
                | ValidationErrorKind::SandboxEscape
                | ValidationErrorKind::SymlinkEscape
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_error() {
        let err = ValidationError::empty("field");
        assert_eq!(err.field, "field");
        assert!(err.is_empty_error());
    }

    #[test]
    fn test_too_long_error() {
        let err = ValidationError::too_long("name", 10, 20);
        assert!(err.to_string().contains("too long"));
    }

    #[test]
    fn test_shell_injection_error() {
        let err = ValidationError::shell_injection("cmd", ';');
        assert!(err.is_security_error());
        assert!(err.to_string().contains("';'"));
    }

    #[test]
    fn test_path_traversal_error() {
        let err = ValidationError::path_traversal("path", "..");
        assert!(err.is_security_error());
        assert!(err.to_string().contains(".."));
    }

    #[test]
    fn test_out_of_range_error() {
        let err = ValidationError::out_of_range("port", 1, 65535, 0);
        assert!(err.to_string().contains("out of range"));
    }

    #[test]
    fn test_with_path() {
        let err = ValidationError::path_traversal("path", "..")
            .with_path("/tmp/test");
        assert!(err.path.is_some());
    }
}
