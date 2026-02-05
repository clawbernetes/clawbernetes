//! Container runtime error types.

use std::fmt;
use thiserror::Error;

/// Container runtime errors.
#[derive(Debug, Error)]
pub enum ContainerError {
    /// Failed to connect to Docker daemon.
    #[error("failed to connect to Docker daemon: {0}")]
    ConnectionFailed(String),

    /// Container not found.
    #[error("container not found: {id}")]
    NotFound {
        /// Container ID.
        id: String,
    },

    /// Image not found.
    #[error("image not found: {image}")]
    ImageNotFound {
        /// Image name.
        image: String,
    },

    /// Container creation failed.
    #[error("container creation failed: {0}")]
    CreateFailed(String),

    /// Container start failed.
    #[error("container start failed: {id}: {reason}")]
    StartFailed {
        /// Container ID.
        id: String,
        /// Failure reason.
        reason: String,
    },

    /// Container stop failed.
    #[error("container stop failed: {id}: {reason}")]
    StopFailed {
        /// Container ID.
        id: String,
        /// Failure reason.
        reason: String,
    },

    /// Container remove failed.
    #[error("container remove failed: {id}: {reason}")]
    RemoveFailed {
        /// Container ID.
        id: String,
        /// Failure reason.
        reason: String,
    },

    /// GPU passthrough not available.
    #[error("GPU passthrough not available: {0}")]
    GpuNotAvailable(String),

    /// Invalid configuration.
    #[error("invalid container configuration: {0}")]
    InvalidConfig(String),

    /// Resource limit exceeded.
    #[error("resource limit exceeded: {resource}: requested {requested}, available {available}")]
    ResourceLimitExceeded {
        /// Resource type (memory, CPU, GPU).
        resource: String,
        /// Requested amount.
        requested: String,
        /// Available amount.
        available: String,
    },

    /// Operation timeout.
    #[error("operation timed out after {seconds} seconds")]
    Timeout {
        /// Timeout duration in seconds.
        seconds: u64,
    },

    /// Internal runtime error.
    #[error("internal runtime error: {0}")]
    Internal(String),
}

/// Result type for container operations.
pub type ContainerResult<T> = std::result::Result<T, ContainerError>;

/// Container ID wrapper for type safety.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContainerId(String);

impl ContainerId {
    /// Create a new container ID from a string.
    ///
    /// # Errors
    ///
    /// Returns error if ID is empty or contains invalid characters.
    pub fn new(id: impl Into<String>) -> ContainerResult<Self> {
        let id = id.into();
        if id.is_empty() {
            return Err(ContainerError::InvalidConfig(
                "container ID cannot be empty".to_string(),
            ));
        }
        // Docker IDs are hex strings
        if !id.chars().all(|c| c.is_ascii_hexdigit()) {
            // Could be a name with more allowed chars
            if !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
            {
                return Err(ContainerError::InvalidConfig(format!(
                    "invalid container ID: {id}"
                )));
            }
        }
        Ok(Self(id))
    }

    /// Create a container ID without validation (for internal use).
    #[must_use]
    pub(crate) fn new_unchecked(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the ID as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the short form (first 12 chars) of the container ID.
    #[must_use]
    pub fn short(&self) -> &str {
        if self.0.len() >= 12 {
            &self.0[..12]
        } else {
            &self.0
        }
    }
}

impl fmt::Display for ContainerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.short())
    }
}

impl AsRef<str> for ContainerId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<ContainerId> for String {
    fn from(id: ContainerId) -> Self {
        id.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_id_valid_hex() {
        let id = ContainerId::new("abc123def456").expect("valid hex ID");
        assert_eq!(id.as_str(), "abc123def456");
        assert_eq!(id.short(), "abc123def456");
    }

    #[test]
    fn test_container_id_long_hex() {
        let id =
            ContainerId::new("abc123def456789012345678").expect("valid long hex ID");
        assert_eq!(id.short(), "abc123def456");
    }

    #[test]
    fn test_container_id_valid_name() {
        let id = ContainerId::new("my-container_name.1").expect("valid name");
        assert_eq!(id.as_str(), "my-container_name.1");
    }

    #[test]
    fn test_container_id_empty() {
        let result = ContainerId::new("");
        assert!(result.is_err());
    }

    #[test]
    fn test_container_id_invalid_chars() {
        let result = ContainerId::new("my container!");
        assert!(result.is_err());
    }

    #[test]
    fn test_container_id_display() {
        let id = ContainerId::new_unchecked("abc123def456789012345678");
        assert_eq!(format!("{id}"), "abc123def456");
    }

    #[test]
    fn test_container_id_into_string() {
        let id = ContainerId::new_unchecked("test123");
        let s: String = id.into();
        assert_eq!(s, "test123");
    }

    #[test]
    fn test_container_error_display() {
        let err = ContainerError::NotFound {
            id: "abc123".to_string(),
        };
        assert!(err.to_string().contains("abc123"));

        let err = ContainerError::Timeout { seconds: 30 };
        assert!(err.to_string().contains("30"));
    }

    #[test]
    fn test_container_error_resource_limit() {
        let err = ContainerError::ResourceLimitExceeded {
            resource: "memory".to_string(),
            requested: "8GB".to_string(),
            available: "4GB".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("memory"));
        assert!(msg.contains("8GB"));
        assert!(msg.contains("4GB"));
    }
}
