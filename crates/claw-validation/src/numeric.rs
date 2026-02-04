//! Numeric validation functions.

use crate::error::ValidationError;
use crate::sanitized::ValidatedValue;
use crate::{MAX_MEMORY_LIMIT, MAX_TIMEOUT_SECONDS, MIN_MEMORY_LIMIT};

/// Type alias for validated port numbers.
pub type ValidatedPort = ValidatedValue<u16>;

/// Type alias for validated memory limits.
pub type ValidatedMemoryLimit = ValidatedValue<u64>;

/// Type alias for validated timeouts.
pub type ValidatedTimeout = ValidatedValue<u64>;

/// Validate a port number.
///
/// Ports must be in the range 1-65535.
///
/// # Errors
///
/// Returns `ValidationError` if the port is out of range.
pub fn validate_port(port: u16) -> Result<ValidatedPort, ValidationError> {
    if port == 0 {
        return Err(ValidationError::out_of_range("port", 1, 65535, 0));
    }
    Ok(ValidatedValue::new(port))
}

/// Validate a memory limit in bytes.
///
/// Memory limits must be between 1 MiB and 1 TiB.
///
/// # Errors
///
/// Returns `ValidationError` if the limit is out of range.
pub fn validate_memory_limit(bytes: u64) -> Result<ValidatedMemoryLimit, ValidationError> {
    if bytes < MIN_MEMORY_LIMIT {
        return Err(ValidationError::out_of_range(
            "memory_limit",
            MIN_MEMORY_LIMIT,
            MAX_MEMORY_LIMIT,
            bytes,
        ));
    }

    if bytes > MAX_MEMORY_LIMIT {
        return Err(ValidationError::out_of_range(
            "memory_limit",
            MIN_MEMORY_LIMIT,
            MAX_MEMORY_LIMIT,
            bytes,
        ));
    }

    Ok(ValidatedValue::new(bytes))
}

/// Validate a timeout in seconds.
///
/// Timeouts must be between 1 second and 7 days.
///
/// # Errors
///
/// Returns `ValidationError` if the timeout is out of range.
pub fn validate_timeout(seconds: u64) -> Result<ValidatedTimeout, ValidationError> {
    if seconds == 0 {
        return Err(ValidationError::out_of_range(
            "timeout",
            1,
            MAX_TIMEOUT_SECONDS,
            0,
        ));
    }

    if seconds > MAX_TIMEOUT_SECONDS {
        return Err(ValidationError::out_of_range(
            "timeout",
            1,
            MAX_TIMEOUT_SECONDS,
            seconds,
        ));
    }

    Ok(ValidatedValue::new(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Port Tests
    // =========================================================================

    #[test]
    fn test_valid_port() {
        assert!(validate_port(80).is_ok());
        assert!(validate_port(443).is_ok());
        assert!(validate_port(8080).is_ok());
        assert!(validate_port(1).is_ok());
        assert!(validate_port(65535).is_ok());
    }

    #[test]
    fn test_port_zero() {
        assert!(validate_port(0).is_err());
    }

    #[test]
    fn test_port_value() {
        let port = validate_port(8080).unwrap_or_else(|_| ValidatedValue::new(0));
        assert_eq!(port.value(), 8080);
    }

    // =========================================================================
    // Memory Limit Tests
    // =========================================================================

    #[test]
    fn test_valid_memory_limit() {
        assert!(validate_memory_limit(MIN_MEMORY_LIMIT).is_ok());
        assert!(validate_memory_limit(MAX_MEMORY_LIMIT).is_ok());
        assert!(validate_memory_limit(1024 * 1024 * 1024).is_ok()); // 1 GiB
    }

    #[test]
    fn test_memory_limit_too_small() {
        assert!(validate_memory_limit(MIN_MEMORY_LIMIT - 1).is_err());
        assert!(validate_memory_limit(0).is_err());
    }

    #[test]
    fn test_memory_limit_too_large() {
        assert!(validate_memory_limit(MAX_MEMORY_LIMIT + 1).is_err());
    }

    // =========================================================================
    // Timeout Tests
    // =========================================================================

    #[test]
    fn test_valid_timeout() {
        assert!(validate_timeout(1).is_ok());
        assert!(validate_timeout(60).is_ok());
        assert!(validate_timeout(3600).is_ok());
        assert!(validate_timeout(MAX_TIMEOUT_SECONDS).is_ok());
    }

    #[test]
    fn test_timeout_zero() {
        assert!(validate_timeout(0).is_err());
    }

    #[test]
    fn test_timeout_too_large() {
        assert!(validate_timeout(MAX_TIMEOUT_SECONDS + 1).is_err());
    }
}
