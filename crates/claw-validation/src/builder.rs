//! Validation builder for fluent validation chains.

use crate::error::ValidationError;

/// Shell metacharacters that could be dangerous.
const SHELL_METACHARACTERS: &[char] = &[
    ';', '|', '&', '$', '`', '(', ')', '{', '}', '[', ']', '<', '>', '!', '#', '*', '?', '~', '\\',
    '\n', '\r',
];

/// A builder for performing multiple validations with error collection.
///
/// # Example
///
/// ```
/// use claw_validation::ValidationBuilder;
///
/// let result = ValidationBuilder::new()
///     .validate_not_empty("name", "test")
///     .validate_no_shell_chars("name", "test")
///     .finish();
///
/// assert!(result.is_ok());
/// ```
#[derive(Debug, Default)]
pub struct ValidationBuilder {
    errors: Vec<ValidationError>,
}

impl ValidationBuilder {
    /// Create a new validation builder.
    #[must_use]
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Add a validation check.
    ///
    /// The closure should return `Ok(())` if validation passes,
    /// or `Err(ValidationError)` if it fails.
    #[must_use]
    pub fn validate<F>(mut self, check: F) -> Self
    where
        F: FnOnce() -> Result<(), ValidationError>,
    {
        if let Err(e) = check() {
            self.errors.push(e);
        }
        self
    }

    /// Validate that a value is not empty.
    #[must_use]
    pub fn validate_not_empty(mut self, field: &str, value: &str) -> Self {
        if value.trim().is_empty() {
            self.errors.push(ValidationError::empty(field));
        }
        self
    }

    /// Validate that a value contains no shell metacharacters.
    #[must_use]
    pub fn validate_no_shell_chars(mut self, field: &str, value: &str) -> Self {
        for ch in value.chars() {
            if SHELL_METACHARACTERS.contains(&ch) {
                self.errors.push(ValidationError::shell_injection(field, ch));
                return self;
            }
        }
        self
    }

    /// Validate that a value has no path traversal.
    #[must_use]
    pub fn validate_no_traversal(mut self, field: &str, value: &str) -> Self {
        if value.contains("..") {
            self.errors.push(ValidationError::path_traversal(field, ".."));
        }
        self
    }

    /// Validate that a value is within a maximum length.
    #[must_use]
    pub fn validate_max_length(mut self, field: &str, value: &str, max: usize) -> Self {
        if value.len() > max {
            self.errors.push(ValidationError::too_long(field, max, value.len()));
        }
        self
    }

    /// Validate that a value contains no null bytes.
    #[must_use]
    pub fn validate_no_null_bytes(mut self, field: &str, value: &str) -> Self {
        if value.contains('\0') {
            self.errors.push(ValidationError::null_byte(field));
        }
        self
    }

    /// Validate that a numeric value is within a range.
    #[must_use]
    pub fn validate_in_range(mut self, field: &str, value: u64, min: u64, max: u64) -> Self {
        if value < min || value > max {
            self.errors.push(ValidationError::out_of_range(field, min, max, value));
        }
        self
    }

    /// Check if any errors have been collected.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Get the number of errors collected.
    #[must_use]
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// Finish validation and return result.
    ///
    /// Returns `Ok(())` if all validations passed, or `Err` with all errors
    /// if any validation failed.
    pub fn finish(self) -> Result<(), Vec<ValidationError>> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors)
        }
    }

    /// Finish validation and return only the first error.
    ///
    /// Returns `Ok(())` if all validations passed, or `Err` with the
    /// first error if any validation failed.
    pub fn finish_first(mut self) -> Result<(), ValidationError> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.remove(0))
        }
    }

    /// Get all collected errors.
    #[must_use]
    pub fn errors(&self) -> &[ValidationError] {
        &self.errors
    }

    /// Consume and return all collected errors.
    #[must_use]
    pub fn into_errors(self) -> Vec<ValidationError> {
        self.errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_all_pass() {
        let result = ValidationBuilder::new()
            .validate_not_empty("name", "test")
            .validate_no_shell_chars("name", "test")
            .finish();

        assert!(result.is_ok());
    }

    #[test]
    fn test_builder_one_fails() {
        let result = ValidationBuilder::new()
            .validate_not_empty("name", "test")
            .validate_no_shell_chars("cmd", "ls; rm")
            .finish();

        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_builder_multiple_failures() {
        let result = ValidationBuilder::new()
            .validate_not_empty("name", "")
            .validate_no_shell_chars("cmd", "ls; rm")
            .finish();

        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn test_builder_has_errors() {
        let builder = ValidationBuilder::new().validate_not_empty("name", "");

        assert!(builder.has_errors());
        assert_eq!(builder.error_count(), 1);
    }

    #[test]
    fn test_builder_no_traversal() {
        let result = ValidationBuilder::new()
            .validate_no_traversal("path", "../etc/passwd")
            .finish();

        assert!(result.is_err());
    }

    #[test]
    fn test_builder_max_length() {
        let result = ValidationBuilder::new()
            .validate_max_length("name", "test", 2)
            .finish();

        assert!(result.is_err());
    }

    #[test]
    fn test_builder_no_null_bytes() {
        let result = ValidationBuilder::new()
            .validate_no_null_bytes("data", "test\0data")
            .finish();

        assert!(result.is_err());
    }

    #[test]
    fn test_builder_in_range() {
        assert!(ValidationBuilder::new()
            .validate_in_range("port", 8080, 1, 65535)
            .finish()
            .is_ok());

        assert!(ValidationBuilder::new()
            .validate_in_range("port", 0, 1, 65535)
            .finish()
            .is_err());
    }
}
