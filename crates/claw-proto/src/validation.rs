//! Input validation for Clawbernetes protocol types.

use crate::error::ProtoError;

/// Maximum memory limit in MB (1 TB).
pub const MAX_MEMORY_MB: u64 = 1_024_000;

/// Maximum CPU cores limit.
pub const MAX_CPU_CORES: u32 = 1024;

/// Maximum GPU count limit.
pub const MAX_GPU_COUNT: u32 = 64;

/// Validation error with detailed information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// The field that failed validation.
    pub field: String,
    /// Description of the validation failure.
    pub message: String,
}

impl ValidationError {
    /// Create a new validation error.
    #[must_use]
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for ValidationError {}

impl From<ValidationError> for ProtoError {
    fn from(e: ValidationError) -> Self {
        Self::Validation(e.to_string())
    }
}

/// Result of validation that may contain multiple errors.
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    errors: Vec<ValidationError>,
}

impl ValidationResult {
    /// Create a new empty validation result.
    #[must_use]
    pub const fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Add an error to the result.
    pub fn add_error(&mut self, error: ValidationError) {
        self.errors.push(error);
    }

    /// Add an error with field and message.
    pub fn error(&mut self, field: impl Into<String>, message: impl Into<String>) {
        self.errors.push(ValidationError::new(field, message));
    }

    /// Check if validation passed (no errors).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Get all errors.
    #[must_use]
    pub fn errors(&self) -> &[ValidationError] {
        &self.errors
    }

    /// Convert to Result, returning Err if there are any errors.
    ///
    /// # Errors
    ///
    /// Returns the first validation error if validation failed.
    pub fn into_result(self) -> Result<(), ValidationError> {
        self.errors
            .into_iter()
            .next()
            .map_or(Ok(()), Err)
    }

    /// Merge another validation result into this one.
    pub fn merge(&mut self, other: Self) {
        self.errors.extend(other.errors);
    }
}

/// Validate a container image reference.
///
/// Valid formats:
/// - `name:tag` (e.g., `nginx:latest`)
/// - `registry/name:tag` (e.g., `docker.io/nginx:latest`)
/// - `registry:port/name:tag` (e.g., `localhost:5000/myapp:v1`)
/// - `name@sha256:digest` (e.g., `nginx@sha256:abc123...`)
///
/// # Errors
///
/// Returns an error if the image reference is invalid.
pub fn validate_image(image: &str) -> Result<(), ValidationError> {
    if image.is_empty() {
        return Err(ValidationError::new("image", "image cannot be empty"));
    }

    // Check for invalid characters
    if image.contains(char::is_whitespace) {
        return Err(ValidationError::new(
            "image",
            "image cannot contain whitespace",
        ));
    }

    // Very basic structure validation
    // Image must have at least a name component
    let parts: Vec<&str> = image.split('/').collect();
    let name_tag = parts.last().unwrap_or(&"");

    if name_tag.is_empty() {
        return Err(ValidationError::new(
            "image",
            "image must have a name component",
        ));
    }

    // Check for obviously invalid patterns
    if image.starts_with(':') || image.starts_with('@') {
        return Err(ValidationError::new(
            "image",
            "image cannot start with ':' or '@'",
        ));
    }

    if image.ends_with(':') || image.ends_with('/') {
        return Err(ValidationError::new(
            "image",
            "image cannot end with ':' or '/'",
        ));
    }

    // Validate tag or digest if present
    if let Some(at_pos) = name_tag.find('@') {
        let digest = &name_tag[at_pos + 1..];
        if !digest.starts_with("sha256:") && !digest.starts_with("sha512:") {
            return Err(ValidationError::new(
                "image",
                "digest must start with 'sha256:' or 'sha512:'",
            ));
        }
    }

    Ok(())
}

/// Validate resource limits.
///
/// # Errors
///
/// Returns an error if any resource limit is invalid.
pub fn validate_resources(
    memory_mb: u64,
    cpu_cores: u32,
    gpu_count: u32,
) -> Result<(), ValidationError> {
    if memory_mb > MAX_MEMORY_MB {
        return Err(ValidationError::new(
            "memory_mb",
            format!("memory_mb cannot exceed {MAX_MEMORY_MB}"),
        ));
    }

    if cpu_cores > MAX_CPU_CORES {
        return Err(ValidationError::new(
            "cpu_cores",
            format!("cpu_cores cannot exceed {MAX_CPU_CORES}"),
        ));
    }

    if gpu_count > MAX_GPU_COUNT {
        return Err(ValidationError::new(
            "gpu_count",
            format!("gpu_count cannot exceed {MAX_GPU_COUNT}"),
        ));
    }

    Ok(())
}

/// Validate environment variable key.
///
/// # Errors
///
/// Returns an error if the key is invalid.
pub fn validate_env_key(key: &str) -> Result<(), ValidationError> {
    if key.is_empty() {
        return Err(ValidationError::new(
            "env",
            "environment variable key cannot be empty",
        ));
    }

    // Environment variable names should start with a letter or underscore
    let first = key.chars().next().unwrap_or('0');
    if !first.is_ascii_alphabetic() && first != '_' {
        return Err(ValidationError::new(
            "env",
            format!("environment variable key '{key}' must start with a letter or underscore"),
        ));
    }

    // Rest should be alphanumeric or underscore
    for c in key.chars() {
        if !c.is_ascii_alphanumeric() && c != '_' {
            return Err(ValidationError::new(
                "env",
                format!("environment variable key '{key}' contains invalid character '{c}'"),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Image Validation Tests ====================

    #[test]
    fn test_validate_image_simple_name() {
        assert!(validate_image("nginx").is_ok());
    }

    #[test]
    fn test_validate_image_with_tag() {
        assert!(validate_image("nginx:latest").is_ok());
        assert!(validate_image("python:3.11-slim").is_ok());
    }

    #[test]
    fn test_validate_image_with_registry() {
        assert!(validate_image("docker.io/library/nginx:latest").is_ok());
        assert!(validate_image("gcr.io/my-project/my-image:v1.0").is_ok());
    }

    #[test]
    fn test_validate_image_with_port() {
        assert!(validate_image("localhost:5000/myapp:latest").is_ok());
    }

    #[test]
    fn test_validate_image_with_digest() {
        assert!(validate_image("nginx@sha256:abc123def456").is_ok());
    }

    #[test]
    fn test_validate_image_empty_fails() {
        let result = validate_image("");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().field, "image");
    }

    #[test]
    fn test_validate_image_whitespace_fails() {
        assert!(validate_image("nginx latest").is_err());
        assert!(validate_image("nginx\tlatest").is_err());
    }

    #[test]
    fn test_validate_image_invalid_start_fails() {
        assert!(validate_image(":latest").is_err());
        assert!(validate_image("@sha256:abc").is_err());
    }

    #[test]
    fn test_validate_image_invalid_end_fails() {
        assert!(validate_image("nginx:").is_err());
        assert!(validate_image("nginx/").is_err());
    }

    #[test]
    fn test_validate_image_invalid_digest_fails() {
        assert!(validate_image("nginx@md5:abc123").is_err());
    }

    // ==================== Resource Validation Tests ====================

    #[test]
    fn test_validate_resources_valid() {
        assert!(validate_resources(1024, 4, 1).is_ok());
        assert!(validate_resources(0, 0, 0).is_ok());
        assert!(validate_resources(MAX_MEMORY_MB, MAX_CPU_CORES, MAX_GPU_COUNT).is_ok());
    }

    #[test]
    fn test_validate_resources_memory_exceeds_limit() {
        let result = validate_resources(MAX_MEMORY_MB + 1, 4, 1);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().field, "memory_mb");
    }

    #[test]
    fn test_validate_resources_cpu_exceeds_limit() {
        let result = validate_resources(1024, MAX_CPU_CORES + 1, 1);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().field, "cpu_cores");
    }

    #[test]
    fn test_validate_resources_gpu_exceeds_limit() {
        let result = validate_resources(1024, 4, MAX_GPU_COUNT + 1);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().field, "gpu_count");
    }

    // ==================== Environment Key Validation Tests ====================

    #[test]
    fn test_validate_env_key_valid() {
        assert!(validate_env_key("PATH").is_ok());
        assert!(validate_env_key("MY_VAR").is_ok());
        assert!(validate_env_key("_PRIVATE").is_ok());
        assert!(validate_env_key("var123").is_ok());
    }

    #[test]
    fn test_validate_env_key_empty_fails() {
        assert!(validate_env_key("").is_err());
    }

    #[test]
    fn test_validate_env_key_starts_with_number_fails() {
        assert!(validate_env_key("123VAR").is_err());
    }

    #[test]
    fn test_validate_env_key_invalid_char_fails() {
        assert!(validate_env_key("MY-VAR").is_err());
        assert!(validate_env_key("MY.VAR").is_err());
        assert!(validate_env_key("MY VAR").is_err());
    }

    // ==================== ValidationResult Tests ====================

    #[test]
    fn test_validation_result_empty_is_valid() {
        let result = ValidationResult::new();
        assert!(result.is_valid());
        assert!(result.errors().is_empty());
    }

    #[test]
    fn test_validation_result_with_errors() {
        let mut result = ValidationResult::new();
        result.error("field1", "error1");
        result.error("field2", "error2");

        assert!(!result.is_valid());
        assert_eq!(result.errors().len(), 2);
    }

    #[test]
    fn test_validation_result_into_result() {
        let result = ValidationResult::new();
        assert!(result.into_result().is_ok());

        let mut result = ValidationResult::new();
        result.error("test", "failed");
        let err = result.into_result().unwrap_err();
        assert_eq!(err.field, "test");
    }

    #[test]
    fn test_validation_result_merge() {
        let mut result1 = ValidationResult::new();
        result1.error("field1", "error1");

        let mut result2 = ValidationResult::new();
        result2.error("field2", "error2");

        result1.merge(result2);
        assert_eq!(result1.errors().len(), 2);
    }

    #[test]
    fn test_validation_error_display() {
        let err = ValidationError::new("image", "cannot be empty");
        assert_eq!(err.to_string(), "image: cannot be empty");
    }
}
