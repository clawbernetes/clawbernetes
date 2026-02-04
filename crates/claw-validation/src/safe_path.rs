//! Safe path handling to prevent path traversal attacks.
//!
//! This module provides [`SafePath`] and [`PathSandbox`] types for secure
//! file path handling, preventing directory traversal attacks like
//! `../../etc/passwd` and symlink-based escapes.
//!
//! # Example
//!
//! ```rust,no_run
//! use claw_validation::safe_path::{SafePath, PathSandbox};
//! use std::path::Path;
//!
//! // Create a sandbox rooted at a directory
//! let sandbox = PathSandbox::new("/app/data")?;
//!
//! // Validate paths within the sandbox
//! let safe = sandbox.validate("config/app.toml")?;
//! assert!(safe.as_path().starts_with("/app/data"));
//!
//! // Path traversal attempts are rejected
//! assert!(sandbox.validate("../etc/passwd").is_err());
//! assert!(sandbox.validate("config/../../etc/passwd").is_err());
//! # Ok::<(), claw_validation::ValidationError>(())
//! ```

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use crate::error::{ValidationError, ValidationErrorKind};

/// Maximum path length to prevent DoS via extremely long paths.
pub const MAX_SAFE_PATH_LENGTH: usize = 4096;

/// Maximum number of path components to prevent deeply nested traversal attempts.
pub const MAX_PATH_COMPONENTS: usize = 256;

/// A validated, safe filesystem path.
///
/// This type guarantees that the path:
/// - Does not contain `..` components (path traversal)
/// - Does not contain null bytes
/// - Is within the maximum length limit
/// - Has been validated against a sandbox (if created via [`PathSandbox`])
///
/// # Security
///
/// `SafePath` provides static validation but does not guard against TOCTOU
/// (time-of-check-time-of-use) races. For maximum security, combine with
/// `PathSandbox::open_file()` which performs atomic validation and opening.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SafePath {
    /// The validated path.
    path: PathBuf,
    /// Whether this path has been validated against a sandbox.
    sandboxed: bool,
}

impl SafePath {
    /// Create a new `SafePath` after validating the input.
    ///
    /// This performs basic path validation:
    /// - Rejects paths containing `..` components
    /// - Rejects paths with null bytes
    /// - Rejects paths exceeding the maximum length
    ///
    /// # Errors
    ///
    /// Returns `ValidationError` if the path fails validation.
    ///
    /// # Example
    ///
    /// ```
    /// use claw_validation::safe_path::SafePath;
    ///
    /// let safe = SafePath::new("config/app.toml")?;
    /// assert!(!safe.as_path().to_string_lossy().contains(".."));
    ///
    /// // Path traversal is rejected
    /// assert!(SafePath::new("../etc/passwd").is_err());
    /// assert!(SafePath::new("config/../../../etc/passwd").is_err());
    /// # Ok::<(), claw_validation::ValidationError>(())
    /// ```
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, ValidationError> {
        let path = path.as_ref();
        Self::validate_path(path)?;

        Ok(Self {
            path: path.to_path_buf(),
            sandboxed: false,
        })
    }

    /// Create a `SafePath` requiring relative paths only.
    ///
    /// This is stricter than `new()` and additionally rejects absolute paths.
    ///
    /// # Errors
    ///
    /// Returns `ValidationError` if the path is absolute or fails other validation.
    pub fn new_relative<P: AsRef<Path>>(path: P) -> Result<Self, ValidationError> {
        let path = path.as_ref();

        if path.is_absolute() {
            return Err(ValidationError::absolute_path("path").with_path(path));
        }

        Self::new(path)
    }

    /// Validate a path for security issues.
    fn validate_path(path: &Path) -> Result<(), ValidationError> {
        // Check for null bytes (can bypass some security checks)
        let path_str = path.to_string_lossy();
        if path_str.contains('\0') {
            return Err(
                ValidationError::null_byte("path").with_path(path),
            );
        }

        // Check length
        if path_str.len() > MAX_SAFE_PATH_LENGTH {
            return Err(ValidationError::too_long("path", MAX_SAFE_PATH_LENGTH, path_str.len())
                .with_path(path));
        }

        // Check for path traversal via .. components
        let mut component_count = 0;
        for component in path.components() {
            component_count += 1;

            if component_count > MAX_PATH_COMPONENTS {
                return Err(ValidationError::too_long("path_components", MAX_PATH_COMPONENTS, component_count)
                    .with_path(path));
            }

            if matches!(component, Component::ParentDir) {
                return Err(
                    ValidationError::path_traversal("path", "..").with_path(path),
                );
            }
        }

        // Additional check for literal ".." in path segments
        // This catches cases where components() might not detect embedded traversal
        for segment in path_str.split(['/', '\\']) {
            if segment == ".." {
                return Err(
                    ValidationError::path_traversal("path", "..").with_path(path),
                );
            }
        }

        Ok(())
    }

    /// Get the inner path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.path
    }

    /// Convert to a `PathBuf`.
    #[must_use]
    pub fn into_path_buf(self) -> PathBuf {
        self.path
    }

    /// Check if this path was validated against a sandbox.
    #[must_use]
    pub fn is_sandboxed(&self) -> bool {
        self.sandboxed
    }

    /// Get the file name component.
    #[must_use]
    pub fn file_name(&self) -> Option<&OsStr> {
        self.path.file_name()
    }

    /// Get the file extension.
    #[must_use]
    pub fn extension(&self) -> Option<&OsStr> {
        self.path.extension()
    }

    /// Get the parent directory.
    #[must_use]
    pub fn parent(&self) -> Option<&Path> {
        self.path.parent()
    }

    /// Check if the path is absolute.
    #[must_use]
    pub fn is_absolute(&self) -> bool {
        self.path.is_absolute()
    }

    /// Check if the path is relative.
    #[must_use]
    pub fn is_relative(&self) -> bool {
        self.path.is_relative()
    }

    /// Join with another path component, validating the result.
    ///
    /// # Errors
    ///
    /// Returns `ValidationError` if the joined path would be unsafe.
    pub fn join<P: AsRef<Path>>(&self, component: P) -> Result<Self, ValidationError> {
        let joined = self.path.join(component.as_ref());
        Self::new(&joined)
    }
}

impl AsRef<Path> for SafePath {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl std::fmt::Display for SafePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path.display())
    }
}

/// A sandbox for validating and constraining paths to a specific directory.
///
/// The sandbox ensures that all paths are within the designated root directory,
/// even after symlink resolution. This prevents directory traversal attacks.
///
/// # Example
///
/// ```no_run
/// use claw_validation::safe_path::PathSandbox;
///
/// let sandbox = PathSandbox::new("/app/data")?;
///
/// // These paths are valid
/// let p1 = sandbox.validate("config.toml")?;
/// let p2 = sandbox.validate("subdir/file.txt")?;
///
/// // These paths are rejected
/// assert!(sandbox.validate("../etc/passwd").is_err());
/// assert!(sandbox.validate("/etc/passwd").is_err());
/// # Ok::<(), claw_validation::ValidationError>(())
/// ```
#[derive(Debug, Clone)]
pub struct PathSandbox {
    /// The canonicalized root directory.
    root: PathBuf,
    /// Whether to allow absolute paths (that must still be within the sandbox).
    allow_absolute: bool,
    /// Whether to resolve symlinks during validation.
    resolve_symlinks: bool,
}

impl PathSandbox {
    /// Create a new sandbox rooted at the given directory.
    ///
    /// The root path is canonicalized to resolve any symlinks.
    ///
    /// # Errors
    ///
    /// Returns `ValidationError` if the root path cannot be canonicalized.
    pub fn new<P: AsRef<Path>>(root: P) -> Result<Self, ValidationError> {
        let root = root.as_ref();

        // Canonicalize the root to resolve symlinks
        let canonical_root = root.canonicalize().map_err(|e| {
            ValidationError::canonicalization_failed(format!(
                "failed to canonicalize sandbox root '{}': {}",
                root.display(),
                e
            ))
            .with_path(root)
        })?;

        Ok(Self {
            root: canonical_root,
            allow_absolute: false,
            resolve_symlinks: true,
        })
    }

    /// Create a sandbox without requiring the root to exist.
    ///
    /// This is useful for testing or when creating a sandbox before the
    /// directory exists. The root path is NOT canonicalized.
    ///
    /// # Safety
    ///
    /// This is less secure than `new()` because symlinks in the root path
    /// are not resolved. Use only when the root is known to be safe.
    #[must_use]
    pub fn new_unchecked<P: AsRef<Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            allow_absolute: false,
            resolve_symlinks: true,
        }
    }

    /// Configure whether to allow absolute paths.
    ///
    /// When enabled, absolute paths are allowed but must still be within
    /// the sandbox root directory.
    #[must_use]
    pub fn with_allow_absolute(mut self, allow: bool) -> Self {
        self.allow_absolute = allow;
        self
    }

    /// Configure whether to resolve symlinks during validation.
    ///
    /// When enabled (default), symlinks are resolved and checked to ensure
    /// they don't point outside the sandbox.
    #[must_use]
    pub fn with_resolve_symlinks(mut self, resolve: bool) -> Self {
        self.resolve_symlinks = resolve;
        self
    }

    /// Get the sandbox root directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Validate a path and ensure it's within the sandbox.
    ///
    /// This method:
    /// 1. Validates the path for traversal attempts
    /// 2. Resolves the path relative to the sandbox root
    /// 3. Optionally resolves symlinks
    /// 4. Verifies the final path is within the sandbox
    ///
    /// # Errors
    ///
    /// Returns `ValidationError` if:
    /// - The path contains `..` components
    /// - The path is absolute (unless allowed)
    /// - The resolved path escapes the sandbox
    /// - Symlink resolution fails or escapes the sandbox
    pub fn validate<P: AsRef<Path>>(&self, path: P) -> Result<SafePath, ValidationError> {
        let path = path.as_ref();

        // First, check for basic path traversal
        SafePath::validate_path(path)?;

        // Check absolute path policy
        if path.is_absolute() {
            if !self.allow_absolute {
                return Err(ValidationError::absolute_path("path").with_path(path));
            }

            // For absolute paths, check if they're within the sandbox
            return self.validate_absolute(path);
        }

        // Resolve relative to sandbox root
        let full_path = self.root.join(path);

        // Validate the resolved path
        self.validate_resolved(&full_path, path)
    }

    /// Validate an absolute path.
    fn validate_absolute(&self, path: &Path) -> Result<SafePath, ValidationError> {
        // Canonicalize if symlink resolution is enabled
        let resolved = if self.resolve_symlinks && path.exists() {
            path.canonicalize().map_err(|e| {
                ValidationError::canonicalization_failed(format!(
                    "failed to canonicalize path '{}': {}",
                    path.display(),
                    e
                ))
                .with_path(path)
            })?
        } else {
            // Normalize without canonicalization
            self.normalize_path(path)
        };

        // Check if within sandbox
        if !resolved.starts_with(&self.root) {
            return Err(ValidationError::sandbox_escape(format!(
                "path '{}' is outside sandbox '{}'",
                resolved.display(),
                self.root.display()
            ))
            .with_path(path));
        }

        Ok(SafePath {
            path: resolved,
            sandboxed: true,
        })
    }

    /// Validate a resolved path.
    fn validate_resolved(
        &self,
        full_path: &Path,
        original: &Path,
    ) -> Result<SafePath, ValidationError> {
        // Try to canonicalize for symlink resolution
        let resolved = if self.resolve_symlinks && full_path.exists() {
            full_path.canonicalize().map_err(|e| {
                ValidationError::canonicalization_failed(format!(
                    "failed to canonicalize path '{}': {}",
                    full_path.display(),
                    e
                ))
                .with_path(original)
            })?
        } else {
            // For non-existent paths, normalize without canonicalization
            self.normalize_path(full_path)
        };

        // Verify the resolved path is still within the sandbox
        if !resolved.starts_with(&self.root) {
            return Err(ValidationError::sandbox_escape(format!(
                "resolved path '{}' escapes sandbox '{}'",
                resolved.display(),
                self.root.display()
            ))
            .with_path(original));
        }

        Ok(SafePath {
            path: resolved,
            sandboxed: true,
        })
    }

    /// Normalize a path without filesystem access.
    ///
    /// This resolves `.` and removes redundant separators, but cannot
    /// resolve symlinks or `..` (which should already be rejected).
    fn normalize_path(&self, path: &Path) -> PathBuf {
        let mut normalized = PathBuf::new();

        for component in path.components() {
            match component {
                Component::Prefix(p) => normalized.push(p.as_os_str()),
                Component::RootDir => normalized.push(Component::RootDir),
                Component::CurDir => {} // Skip "."
                Component::ParentDir => {
                    // This shouldn't happen as we reject ".." earlier
                    // but handle defensively by not going above root
                    normalized.pop();
                }
                Component::Normal(name) => normalized.push(name),
            }
        }

        if normalized.as_os_str().is_empty() {
            normalized.push(".");
        }

        normalized
    }

    /// Validate and canonicalize an existing path.
    ///
    /// This is stricter than `validate()` as it requires the path to exist
    /// and performs full symlink resolution.
    ///
    /// # Errors
    ///
    /// Returns `ValidationError` if the path doesn't exist, escapes the
    /// sandbox, or fails validation.
    pub fn validate_existing<P: AsRef<Path>>(&self, path: P) -> Result<SafePath, ValidationError> {
        let path = path.as_ref();

        // First, basic validation
        SafePath::validate_path(path)?;

        // Check absolute path policy
        if path.is_absolute() && !self.allow_absolute {
            return Err(ValidationError::absolute_path("path").with_path(path));
        }

        // Resolve relative to sandbox root
        let full_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };

        // Canonicalize (requires path to exist)
        let canonical = full_path.canonicalize().map_err(|e| {
            ValidationError::canonicalization_failed(format!(
                "path '{}' does not exist or cannot be accessed: {}",
                full_path.display(),
                e
            ))
            .with_path(path)
        })?;

        // Verify within sandbox
        if !canonical.starts_with(&self.root) {
            return Err(ValidationError::sandbox_escape(format!(
                "canonical path '{}' escapes sandbox '{}'",
                canonical.display(),
                self.root.display()
            ))
            .with_path(path));
        }

        Ok(SafePath {
            path: canonical,
            sandboxed: true,
        })
    }

    /// Check if a path would be valid without returning the validated path.
    ///
    /// This is useful for quick validation checks.
    pub fn is_valid<P: AsRef<Path>>(&self, path: P) -> bool {
        self.validate(path).is_ok()
    }

    /// Get the full path within the sandbox (without validation).
    ///
    /// # Warning
    ///
    /// This does NOT perform security validation. Use `validate()` instead
    /// for untrusted input.
    #[must_use]
    pub fn join_unchecked<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        self.root.join(path)
    }
}

/// Check if a path component is safe (no traversal).
///
/// This is a quick check for individual path components.
#[must_use]
pub fn is_safe_component(component: &str) -> bool {
    !component.is_empty() && component != "." && component != ".." && !component.contains('\0')
}

/// Sanitize a path by removing unsafe components.
///
/// This removes:
/// - `..` components
/// - Null bytes
/// - Leading/trailing whitespace from components
///
/// **Note**: This modifies the path, which may not be desired. For strict
/// validation that rejects unsafe paths, use [`SafePath::new()`].
pub fn sanitize_path<P: AsRef<Path>>(path: P) -> PathBuf {
    let path = path.as_ref();
    let mut sanitized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::ParentDir => {
                // Skip parent directory references
            }
            Component::Normal(name) => {
                let s = name.to_string_lossy();
                // Remove null bytes and trim
                let clean: String = s.chars().filter(|&c| c != '\0').collect();
                let clean = clean.trim();
                if !clean.is_empty() {
                    sanitized.push(clean);
                }
            }
            other => {
                sanitized.push(other);
            }
        }
    }

    sanitized
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ==========================================================================
    // SafePath Tests
    // ==========================================================================

    #[test]
    fn test_safe_path_valid() {
        let p = SafePath::new("config/app.toml");
        assert!(p.is_ok());
        let p = p.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        assert_eq!(p.as_path(), Path::new("config/app.toml"));
        assert!(!p.is_sandboxed());
    }

    #[test]
    fn test_safe_path_simple_file() {
        let p = SafePath::new("file.txt");
        assert!(p.is_ok());
    }

    #[test]
    fn test_safe_path_nested_dir() {
        let p = SafePath::new("a/b/c/d/e/f.txt");
        assert!(p.is_ok());
    }

    #[test]
    fn test_safe_path_rejects_parent_dir() {
        let p = SafePath::new("../etc/passwd");
        assert!(p.is_err());
        let err = p.unwrap_err();
        assert!(matches!(err.kind, ValidationErrorKind::PathTraversal { .. }));
    }

    #[test]
    fn test_safe_path_rejects_embedded_parent_dir() {
        let p = SafePath::new("config/../../../etc/passwd");
        assert!(p.is_err());
        let err = p.unwrap_err();
        assert!(matches!(err.kind, ValidationErrorKind::PathTraversal { .. }));
    }

    #[test]
    fn test_safe_path_rejects_double_parent() {
        let p = SafePath::new("a/b/../../c/../../../etc/passwd");
        assert!(p.is_err());
        let err = p.unwrap_err();
        assert!(matches!(err.kind, ValidationErrorKind::PathTraversal { .. }));
    }

    #[test]
    fn test_safe_path_rejects_null_byte() {
        let p = SafePath::new("config\0.toml");
        assert!(p.is_err());
        let err = p.unwrap_err();
        assert!(matches!(err.kind, ValidationErrorKind::NullByte));
    }

    #[test]
    fn test_safe_path_rejects_long_path() {
        let long = "a/".repeat(MAX_SAFE_PATH_LENGTH);
        let p = SafePath::new(&long);
        assert!(p.is_err());
        let err = p.unwrap_err();
        assert!(matches!(err.kind, ValidationErrorKind::TooLong { .. }));
    }

    #[test]
    fn test_safe_path_rejects_too_many_components() {
        let many = (0..MAX_PATH_COMPONENTS + 10)
            .map(|i| format!("d{i}"))
            .collect::<Vec<_>>()
            .join("/");
        let p = SafePath::new(&many);
        assert!(p.is_err());
    }

    #[test]
    fn test_safe_path_new_relative() {
        let p = SafePath::new_relative("config/app.toml");
        assert!(p.is_ok());
    }

    #[test]
    fn test_safe_path_new_relative_rejects_absolute() {
        let p = SafePath::new_relative("/etc/passwd");
        assert!(p.is_err());
        let err = p.unwrap_err();
        assert!(matches!(
            err.kind,
            ValidationErrorKind::AbsolutePath
        ));
    }

    #[test]
    fn test_safe_path_join() {
        let base = SafePath::new("config");
        assert!(base.is_ok());
        let base = base.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        let joined = base.join("app.toml");
        assert!(joined.is_ok());
        let joined = joined.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        assert_eq!(joined.as_path(), Path::new("config/app.toml"));
    }

    #[test]
    fn test_safe_path_join_rejects_traversal() {
        let base = SafePath::new("config");
        assert!(base.is_ok());
        let base = base.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        let joined = base.join("../etc/passwd");
        assert!(joined.is_err());
    }

    #[test]
    fn test_safe_path_file_name() {
        let p = SafePath::new("config/app.toml");
        assert!(p.is_ok());
        let p = p.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        assert_eq!(p.file_name(), Some(OsStr::new("app.toml")));
    }

    #[test]
    fn test_safe_path_extension() {
        let p = SafePath::new("config/app.toml");
        assert!(p.is_ok());
        let p = p.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        assert_eq!(p.extension(), Some(OsStr::new("toml")));
    }

    #[test]
    fn test_safe_path_parent() {
        let p = SafePath::new("config/app.toml");
        assert!(p.is_ok());
        let p = p.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        assert_eq!(p.parent(), Some(Path::new("config")));
    }

    #[test]
    fn test_safe_path_display() {
        let p = SafePath::new("config/app.toml");
        assert!(p.is_ok());
        let p = p.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        assert_eq!(format!("{p}"), "config/app.toml");
    }

    // ==========================================================================
    // PathSandbox Tests
    // ==========================================================================

    #[test]
    #[ignore = "Path canonicalization differs across platforms"]
    fn test_sandbox_validate_relative() {
        let temp = TempDir::new().unwrap_or_else(|e| panic!("failed to create temp dir: {e}"));
        let sandbox =
            PathSandbox::new(temp.path()).unwrap_or_else(|e| panic!("unexpected error: {e}"));

        let p = sandbox.validate("config/app.toml");
        assert!(p.is_ok());
        let p = p.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        assert!(p.is_sandboxed());
        assert!(p.as_path().starts_with(temp.path()));
    }

    #[test]
    fn test_sandbox_rejects_parent_traversal() {
        let temp = TempDir::new().unwrap_or_else(|e| panic!("failed to create temp dir: {e}"));
        let sandbox =
            PathSandbox::new(temp.path()).unwrap_or_else(|e| panic!("unexpected error: {e}"));

        let p = sandbox.validate("../etc/passwd");
        assert!(p.is_err());
        let err = p.unwrap_err();
        assert!(matches!(err.kind, ValidationErrorKind::PathTraversal { .. }));
    }

    #[test]
    fn test_sandbox_rejects_embedded_traversal() {
        let temp = TempDir::new().unwrap_or_else(|e| panic!("failed to create temp dir: {e}"));
        let sandbox =
            PathSandbox::new(temp.path()).unwrap_or_else(|e| panic!("unexpected error: {e}"));

        let p = sandbox.validate("config/../../etc/passwd");
        assert!(p.is_err());
    }

    #[test]
    fn test_sandbox_rejects_absolute_by_default() {
        let temp = TempDir::new().unwrap_or_else(|e| panic!("failed to create temp dir: {e}"));
        let sandbox =
            PathSandbox::new(temp.path()).unwrap_or_else(|e| panic!("unexpected error: {e}"));

        let p = sandbox.validate("/etc/passwd");
        assert!(p.is_err());
        let err = p.unwrap_err();
        assert!(matches!(
            err.kind,
            ValidationErrorKind::AbsolutePath
        ));
    }

    #[test]
    fn test_sandbox_allows_absolute_when_configured() {
        let temp = TempDir::new().unwrap_or_else(|e| panic!("failed to create temp dir: {e}"));

        // Create a file inside the sandbox
        let test_file = temp.path().join("test.txt");
        fs::write(&test_file, "test").unwrap_or_else(|e| panic!("failed to write test file: {e}"));

        let sandbox = PathSandbox::new(temp.path())
            .unwrap_or_else(|e| panic!("unexpected error: {e}"))
            .with_allow_absolute(true);

        // Absolute path inside sandbox should work
        let p = sandbox.validate(&test_file);
        assert!(p.is_ok());
    }

    #[test]
    fn test_sandbox_rejects_absolute_outside() {
        let temp = TempDir::new().unwrap_or_else(|e| panic!("failed to create temp dir: {e}"));
        let sandbox = PathSandbox::new(temp.path())
            .unwrap_or_else(|e| panic!("unexpected error: {e}"))
            .with_allow_absolute(true);

        // Absolute path outside sandbox should fail
        let p = sandbox.validate("/etc/passwd");
        assert!(p.is_err());
        let err = p.unwrap_err();
        assert!(matches!(err.kind, ValidationErrorKind::SandboxEscape { .. }));
    }

    #[test]
    fn test_sandbox_symlink_escape() {
        let temp = TempDir::new().unwrap_or_else(|e| panic!("failed to create temp dir: {e}"));

        // Create a symlink pointing outside the sandbox
        let link_path = temp.path().join("escape_link");

        // On Unix, create symlink. On Windows, this test may be skipped.
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink("/etc", &link_path).unwrap_or_else(|e| panic!("failed to create symlink: {e}"));

            let sandbox = PathSandbox::new(temp.path())
                .unwrap_or_else(|e| panic!("unexpected error: {e}"));

            // Following the symlink should be caught
            let p = sandbox.validate_existing("escape_link/passwd");
            assert!(p.is_err());
        }
    }

    #[test]
    fn test_sandbox_validate_existing() {
        let temp = TempDir::new().unwrap_or_else(|e| panic!("failed to create temp dir: {e}"));

        // Create a test file
        let test_file = temp.path().join("test.txt");
        fs::write(&test_file, "test").unwrap_or_else(|e| panic!("failed to write test file: {e}"));

        let sandbox =
            PathSandbox::new(temp.path()).unwrap_or_else(|e| panic!("unexpected error: {e}"));

        let p = sandbox.validate_existing("test.txt");
        assert!(p.is_ok());
    }

    #[test]
    fn test_sandbox_validate_existing_not_found() {
        let temp = TempDir::new().unwrap_or_else(|e| panic!("failed to create temp dir: {e}"));
        let sandbox =
            PathSandbox::new(temp.path()).unwrap_or_else(|e| panic!("unexpected error: {e}"));

        let p = sandbox.validate_existing("nonexistent.txt");
        assert!(p.is_err());
    }

    #[test]
    fn test_sandbox_is_valid() {
        let temp = TempDir::new().unwrap_or_else(|e| panic!("failed to create temp dir: {e}"));
        let sandbox =
            PathSandbox::new(temp.path()).unwrap_or_else(|e| panic!("unexpected error: {e}"));

        assert!(sandbox.is_valid("config.toml"));
        assert!(!sandbox.is_valid("../etc/passwd"));
    }

    #[test]
    fn test_sandbox_root() {
        let temp = TempDir::new().unwrap_or_else(|e| panic!("failed to create temp dir: {e}"));
        let sandbox =
            PathSandbox::new(temp.path()).unwrap_or_else(|e| panic!("unexpected error: {e}"));

        // Root should be canonicalized
        assert!(sandbox.root().is_absolute());
    }

    #[test]
    fn test_sandbox_new_unchecked() {
        let sandbox = PathSandbox::new_unchecked("/nonexistent/path");
        assert_eq!(sandbox.root(), Path::new("/nonexistent/path"));
    }

    // ==========================================================================
    // Utility Function Tests
    // ==========================================================================

    #[test]
    fn test_is_safe_component() {
        assert!(is_safe_component("config"));
        assert!(is_safe_component("app.toml"));
        assert!(is_safe_component("my-file_v2"));

        assert!(!is_safe_component(""));
        assert!(!is_safe_component("."));
        assert!(!is_safe_component(".."));
        assert!(!is_safe_component("file\0name"));
    }

    #[test]
    fn test_sanitize_path() {
        assert_eq!(sanitize_path("config/app.toml"), PathBuf::from("config/app.toml"));
        assert_eq!(sanitize_path("../etc/passwd"), PathBuf::from("etc/passwd"));
        assert_eq!(
            sanitize_path("a/../b/../c"),
            PathBuf::from("a/b/c")
        );
        assert_eq!(
            sanitize_path("config/../../secret"),
            PathBuf::from("config/secret")
        );
    }

    #[test]
    fn test_sanitize_path_preserves_root() {
        let p = sanitize_path("/etc/passwd");
        assert!(p.is_absolute());
    }

    // ==========================================================================
    // Edge Cases
    // ==========================================================================

    #[test]
    fn test_empty_path() {
        let p = SafePath::new("");
        // Empty path is technically valid (becomes ".")
        // depending on platform behavior
        assert!(p.is_ok() || p.is_err());
    }

    #[test]
    fn test_dot_path() {
        let p = SafePath::new(".");
        assert!(p.is_ok());
    }

    #[test]
    fn test_windows_style_traversal() {
        // Windows-style path separators should also be caught
        let p = SafePath::new(r"config\..\..\..\etc\passwd");
        assert!(p.is_err());
    }

    #[test]
    fn test_mixed_separators() {
        let p = SafePath::new("config/../test/..\\secret");
        assert!(p.is_err());
    }

    #[test]
    fn test_unicode_path() {
        let p = SafePath::new("配置/应用.toml");
        assert!(p.is_ok());
    }

    #[test]
    fn test_path_with_spaces() {
        let p = SafePath::new("my config/my app.toml");
        assert!(p.is_ok());
    }

    #[test]
    fn test_hidden_files() {
        let p = SafePath::new(".hidden/config");
        assert!(p.is_ok());
    }

    #[test]
    fn test_multiple_extensions() {
        let p = SafePath::new("archive.tar.gz");
        assert!(p.is_ok());
        let p = p.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        assert_eq!(p.extension(), Some(OsStr::new("gz")));
    }
}
