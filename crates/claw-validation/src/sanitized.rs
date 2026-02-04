//! Sanitized value wrapper types with marker traits.

use std::fmt;
use std::marker::PhantomData;

/// Marker trait for sanitization kinds.
pub trait SanitizationKind: private::Sealed {}

mod private {
    pub trait Sealed {}
}

// Marker types for different sanitized value kinds
/// Marker for node names.
#[derive(Debug, Clone, Copy)]
pub struct NodeName;
impl private::Sealed for NodeName {}
impl SanitizationKind for NodeName {}

/// Marker for paths.
#[derive(Debug, Clone, Copy)]
pub struct Path;
impl private::Sealed for Path {}
impl SanitizationKind for Path {}

/// Marker for hostnames.
#[derive(Debug, Clone, Copy)]
pub struct Hostname;
impl private::Sealed for Hostname {}
impl SanitizationKind for Hostname {}

/// Marker for service names.
#[derive(Debug, Clone, Copy)]
pub struct ServiceName;
impl private::Sealed for ServiceName {}
impl SanitizationKind for ServiceName {}

/// Marker for URLs.
#[derive(Debug, Clone, Copy)]
pub struct Url;
impl private::Sealed for Url {}
impl SanitizationKind for Url {}

/// Marker for commands.
#[derive(Debug, Clone, Copy)]
pub struct Command;
impl private::Sealed for Command {}
impl SanitizationKind for Command {}

/// Marker for command arguments.
#[derive(Debug, Clone, Copy)]
pub struct CommandArg;
impl private::Sealed for CommandArg {}
impl SanitizationKind for CommandArg {}

/// Marker for environment variable keys.
#[derive(Debug, Clone, Copy)]
pub struct EnvKey;
impl private::Sealed for EnvKey {}
impl SanitizationKind for EnvKey {}

/// Marker for environment variable values.
#[derive(Debug, Clone, Copy)]
pub struct EnvValue;
impl private::Sealed for EnvValue {}
impl SanitizationKind for EnvValue {}

/// Marker for image names.
#[derive(Debug, Clone, Copy)]
pub struct ImageName;
impl private::Sealed for ImageName {}
impl SanitizationKind for ImageName {}

/// Marker for job IDs.
#[derive(Debug, Clone, Copy)]
pub struct JobId;
impl private::Sealed for JobId {}
impl SanitizationKind for JobId {}

/// A wrapper for validated/sanitized values with type-level guarantees.
///
/// The type parameter `K` indicates what kind of sanitization was performed,
/// providing compile-time guarantees that values are used correctly.
///
/// # Example
///
/// ```
/// use claw_validation::{Sanitized, sanitize_node_name, NodeName};
///
/// let node = sanitize_node_name("my-node")?;
/// // node is Sanitized<NodeName> and cannot be confused with other types
/// # Ok::<(), claw_validation::ValidationError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Sanitized<K: SanitizationKind> {
    value: String,
    _marker: PhantomData<K>,
}

impl<K: SanitizationKind> Sanitized<K> {
    /// Create a new sanitized value.
    ///
    /// This should only be called after the value has been validated.
    #[must_use]
    pub fn new(value: String) -> Self {
        Self {
            value,
            _marker: PhantomData,
        }
    }

    /// Get a reference to the inner value.
    #[must_use]
    pub fn inner(&self) -> &str {
        &self.value
    }

    /// Get the sanitized string as a slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Consume the wrapper and return the inner value.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.value
    }

    /// Get the length of the sanitized string.
    #[must_use]
    pub fn len(&self) -> usize {
        self.value.len()
    }

    /// Check if the sanitized string is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }
}

impl<K: SanitizationKind> AsRef<str> for Sanitized<K> {
    fn as_ref(&self) -> &str {
        &self.value
    }
}

impl<K: SanitizationKind> fmt::Display for Sanitized<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

/// A validated numeric value with bounds information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValidatedValue<T> {
    value: T,
}

impl<T: Copy> ValidatedValue<T> {
    /// Create a new validated value.
    #[must_use]
    pub const fn new(value: T) -> Self {
        Self { value }
    }

    /// Get the inner value.
    #[must_use]
    pub const fn value(&self) -> T {
        self.value
    }
}

impl<T: fmt::Display> fmt::Display for ValidatedValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitized_new() {
        let s: Sanitized<NodeName> = Sanitized::new("test".to_string());
        assert_eq!(s.inner(), "test");
    }

    #[test]
    fn test_sanitized_into_inner() {
        let s: Sanitized<Path> = Sanitized::new("test".to_string());
        let inner = s.into_inner();
        assert_eq!(inner, "test");
    }

    #[test]
    fn test_sanitized_as_str() {
        let s: Sanitized<Hostname> = Sanitized::new("test".to_string());
        assert_eq!(s.as_str(), "test");
    }

    #[test]
    fn test_sanitized_len() {
        let s: Sanitized<Command> = Sanitized::new("test".to_string());
        assert_eq!(s.len(), 4);
    }

    #[test]
    fn test_sanitized_is_empty() {
        let s: Sanitized<ServiceName> = Sanitized::new(String::new());
        assert!(s.is_empty());

        let s: Sanitized<ServiceName> = Sanitized::new("test".to_string());
        assert!(!s.is_empty());
    }

    #[test]
    fn test_sanitized_display() {
        let s: Sanitized<Url> = Sanitized::new("test".to_string());
        assert_eq!(format!("{s}"), "test");
    }

    #[test]
    fn test_validated_value() {
        let v = ValidatedValue::new(42u16);
        assert_eq!(v.value(), 42);
    }

    #[test]
    fn test_validated_value_display() {
        let v = ValidatedValue::new(8080u16);
        assert_eq!(format!("{v}"), "8080");
    }

    #[test]
    fn test_type_safety() {
        // These are different types and can't be mixed
        fn takes_node(_: Sanitized<NodeName>) {}
        fn takes_path(_: Sanitized<Path>) {}

        let node: Sanitized<NodeName> = Sanitized::new("node".to_string());
        let path: Sanitized<Path> = Sanitized::new("path".to_string());

        takes_node(node);
        takes_path(path);
    }
}
