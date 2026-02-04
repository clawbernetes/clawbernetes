//! String validation and sanitization functions.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::error::ValidationError;
use crate::sanitized::{
    Command, CommandArg, EnvKey, EnvValue, Hostname, ImageName, JobId, NodeName, Path,
    Sanitized, ServiceName, Url,
};
use crate::{
    MAX_COMMAND_LENGTH, MAX_ENV_KEY_LENGTH, MAX_ENV_VALUE_LENGTH, MAX_IMAGE_NAME_LENGTH,
    MAX_NODE_NAME_LENGTH, MAX_PATH_LENGTH,
};

/// Shell metacharacters that could enable command injection.
const SHELL_METACHARACTERS: &[char] = &[';', '|', '&', '$', '`', '(', ')', '{', '}', '<', '>', '\n', '\r', '\0'];

/// Maximum length for hostnames (RFC 1035).
const MAX_HOSTNAME_LENGTH: usize = 253;

/// Maximum length for service names (Kubernetes convention).
const MAX_SERVICE_NAME_LENGTH: usize = 63;

/// Maximum length for URLs.
const MAX_URL_LENGTH: usize = 2048;

/// Regex for valid node names (alphanumeric, hyphens, underscores).
static NODE_NAME_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_-]*$").unwrap_or_else(|_| unreachable!()));

/// Regex for valid environment variable keys.
static ENV_KEY_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap_or_else(|_| unreachable!()));

/// Regex for valid image names (Docker format).
static IMAGE_NAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-z0-9][a-z0-9._/-]*(?::[a-zA-Z0-9._-]+)?(?:@sha256:[a-f0-9]{64})?$")
        .unwrap_or_else(|_| unreachable!())
});

/// Regex for valid job IDs (UUID or alphanumeric).
static JOB_ID_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}$").unwrap_or_else(|_| unreachable!())
});

/// Regex for valid hostnames (RFC 1123).
static HOSTNAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?)*$")
        .unwrap_or_else(|_| unreachable!())
});

/// Regex for valid service names (Kubernetes DNS-1035).
static SERVICE_NAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-z][a-z0-9-]*[a-z0-9]$|^[a-z]$").unwrap_or_else(|_| unreachable!())
});

/// Regex for valid URLs.
static URL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z][a-zA-Z0-9+.-]*://[^\s]+$").unwrap_or_else(|_| unreachable!())
});

/// Check for null bytes in input.
fn check_null_bytes(field: &str, input: &str) -> Result<(), ValidationError> {
    if input.contains('\0') {
        return Err(ValidationError::null_byte(field));
    }
    Ok(())
}

/// Check for shell metacharacters.
fn check_shell_chars(field: &str, input: &str) -> Result<(), ValidationError> {
    for ch in input.chars() {
        if SHELL_METACHARACTERS.contains(&ch) {
            return Err(ValidationError::shell_injection(field, ch));
        }
    }
    Ok(())
}

/// Check for path traversal attempts.
fn check_path_traversal(field: &str, input: &str) -> Result<(), ValidationError> {
    // Check for ".." as a path component
    let components: Vec<&str> = input
        .split(|c| c == '/' || c == '\\')
        .filter(|s| !s.is_empty())
        .collect();

    for component in components {
        if component == ".." {
            return Err(ValidationError::path_traversal(field, ".."));
        }
    }
    Ok(())
}

/// Sanitize and validate a node name.
///
/// Node names must:
/// - Be 1-64 characters
/// - Start with alphanumeric
/// - Contain only alphanumeric, hyphens, and underscores
///
/// # Errors
///
/// Returns `ValidationError` if the name is invalid.
///
/// # Example
///
/// ```
/// use claw_validation::sanitize_node_name;
///
/// let node = sanitize_node_name("my-node-01")?;
/// assert_eq!(node.as_str(), "my-node-01");
/// # Ok::<(), claw_validation::ValidationError>(())
/// ```
pub fn sanitize_node_name(name: &str) -> Result<Sanitized<NodeName>, ValidationError> {
    let field = "node_name";
    let name = name.trim();

    if name.is_empty() {
        return Err(ValidationError::empty(field));
    }

    if name.len() > MAX_NODE_NAME_LENGTH {
        return Err(ValidationError::too_long(field, MAX_NODE_NAME_LENGTH, name.len()));
    }

    check_null_bytes(field, name)?;

    if !NODE_NAME_REGEX.is_match(name) {
        return Err(ValidationError::invalid_format(
            field,
            "alphanumeric start, then alphanumeric/hyphens/underscores",
            name,
        ));
    }

    Ok(Sanitized::new(name.to_string()))
}

/// Sanitize and validate an environment variable key.
fn sanitize_env_key(key: &str) -> Result<Sanitized<EnvKey>, ValidationError> {
    let field = "env_key";
    let key = key.trim();

    if key.is_empty() {
        return Err(ValidationError::empty(field));
    }

    if key.len() > MAX_ENV_KEY_LENGTH {
        return Err(ValidationError::too_long(field, MAX_ENV_KEY_LENGTH, key.len()));
    }

    check_null_bytes(field, key)?;

    if !ENV_KEY_REGEX.is_match(key) {
        return Err(ValidationError::invalid_format(
            field,
            "letter/underscore start, then alphanumeric/underscores",
            key,
        ));
    }

    Ok(Sanitized::new(key.to_string()))
}

/// Sanitize and validate an environment variable value.
fn sanitize_env_value(value: &str) -> Result<Sanitized<EnvValue>, ValidationError> {
    let field = "env_value";

    if value.len() > MAX_ENV_VALUE_LENGTH {
        return Err(ValidationError::too_long(field, MAX_ENV_VALUE_LENGTH, value.len()));
    }

    check_null_bytes(field, value)?;

    Ok(Sanitized::new(value.to_string()))
}

/// Sanitize and validate an environment variable.
///
/// # Arguments
///
/// * `key` - The environment variable name
/// * `value` - The environment variable value
///
/// # Errors
///
/// Returns `ValidationError` if the key or value is invalid.
pub fn sanitize_env_var(
    key: &str,
    value: &str,
) -> Result<(Sanitized<EnvKey>, Sanitized<EnvValue>), ValidationError> {
    let sanitized_key = sanitize_env_key(key)?;
    let sanitized_value = sanitize_env_value(value)?;
    Ok((sanitized_key, sanitized_value))
}

/// Sanitize and validate a container image name.
///
/// Image names follow Docker conventions:
/// - registry/namespace/name:tag
/// - registry/namespace/name@sha256:digest
///
/// # Errors
///
/// Returns `ValidationError` if the image name is invalid.
pub fn sanitize_image_name(name: &str) -> Result<Sanitized<ImageName>, ValidationError> {
    let field = "image_name";
    let name = name.trim();

    if name.is_empty() {
        return Err(ValidationError::empty(field));
    }

    if name.len() > MAX_IMAGE_NAME_LENGTH {
        return Err(ValidationError::too_long(field, MAX_IMAGE_NAME_LENGTH, name.len()));
    }

    check_null_bytes(field, name)?;
    check_shell_chars(field, name)?;

    // Convert to lowercase for validation
    let lower = name.to_lowercase();

    if !IMAGE_NAME_REGEX.is_match(&lower) {
        return Err(ValidationError::invalid_format(
            field,
            "Docker image format: [registry/][namespace/]name[:tag][@digest]",
            name,
        ));
    }

    Ok(Sanitized::new(name.to_string()))
}

/// Sanitize and validate a job ID.
///
/// Job IDs must be 1-64 characters, alphanumeric with hyphens/underscores,
/// or valid UUIDs.
///
/// # Errors
///
/// Returns `ValidationError` if the job ID is invalid.
pub fn sanitize_job_id(id: &str) -> Result<Sanitized<JobId>, ValidationError> {
    let field = "job_id";
    let id = id.trim();

    if id.is_empty() {
        return Err(ValidationError::empty(field));
    }

    check_null_bytes(field, id)?;

    // Try parsing as UUID first
    if let Ok(uuid) = uuid::Uuid::try_parse(id) {
        return Ok(Sanitized::new(uuid.to_string()));
    }

    // Otherwise validate as alphanumeric ID
    if !JOB_ID_REGEX.is_match(id) {
        return Err(ValidationError::invalid_format(
            field,
            "UUID or alphanumeric with hyphens/underscores, max 64 chars",
            id,
        ));
    }

    Ok(Sanitized::new(id.to_string()))
}

/// Sanitize and validate a command (program name).
///
/// Rejects:
/// - Empty commands
/// - Path traversal attempts
/// - Null bytes
///
/// Note: This validates the program name only. For full command
/// safety, use the `command` module's `SafeCommand`.
///
/// # Errors
///
/// Returns `ValidationError` if the command is invalid.
pub fn sanitize_command(cmd: &str) -> Result<Sanitized<Command>, ValidationError> {
    let field = "command";
    let cmd = cmd.trim();

    if cmd.is_empty() {
        return Err(ValidationError::empty(field));
    }

    if cmd.len() > MAX_COMMAND_LENGTH {
        return Err(ValidationError::too_long(field, MAX_COMMAND_LENGTH, cmd.len()));
    }

    check_null_bytes(field, cmd)?;
    check_path_traversal(field, cmd)?;

    Ok(Sanitized::new(cmd.to_string()))
}

/// Sanitize and validate a command argument.
///
/// Rejects dangerous shell metacharacters that could enable injection.
///
/// # Errors
///
/// Returns `ValidationError` if the argument contains dangerous characters.
pub fn sanitize_command_arg(arg: &str) -> Result<Sanitized<CommandArg>, ValidationError> {
    let field = "command_arg";

    if arg.len() > MAX_COMMAND_LENGTH {
        return Err(ValidationError::too_long(field, MAX_COMMAND_LENGTH, arg.len()));
    }

    check_null_bytes(field, arg)?;
    check_shell_chars(field, arg)?;

    Ok(Sanitized::new(arg.to_string()))
}

/// Sanitize and validate a filesystem path.
///
/// Rejects:
/// - Empty paths
/// - Path traversal (`..`)
/// - Absolute paths (when not allowed)
/// - Null bytes
///
/// # Errors
///
/// Returns `ValidationError` if the path is invalid.
pub fn sanitize_path(path: &str) -> Result<Sanitized<Path>, ValidationError> {
    let field = "path";
    let path = path.trim();

    if path.is_empty() {
        return Err(ValidationError::empty(field));
    }

    if path.len() > MAX_PATH_LENGTH {
        return Err(ValidationError::too_long(field, MAX_PATH_LENGTH, path.len()));
    }

    check_null_bytes(field, path)?;
    check_path_traversal(field, path)?;

    // Reject absolute paths by default
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(ValidationError::absolute_path(field));
    }

    // Windows-style absolute paths
    if path.len() >= 2 && path.chars().nth(1) == Some(':') {
        return Err(ValidationError::absolute_path(field));
    }

    Ok(Sanitized::new(path.to_string()))
}

/// Sanitize and validate a hostname.
///
/// Hostnames must conform to RFC 1123:
/// - Max 253 characters
/// - Labels separated by dots
/// - Each label: alphanumeric, may contain hyphens (not at start/end)
///
/// # Errors
///
/// Returns `ValidationError` if the hostname is invalid.
pub fn sanitize_hostname(hostname: &str) -> Result<Sanitized<Hostname>, ValidationError> {
    let field = "hostname";
    let hostname = hostname.trim();

    if hostname.is_empty() {
        return Err(ValidationError::empty(field));
    }

    if hostname.len() > MAX_HOSTNAME_LENGTH {
        return Err(ValidationError::too_long(field, MAX_HOSTNAME_LENGTH, hostname.len()));
    }

    check_null_bytes(field, hostname)?;
    check_shell_chars(field, hostname)?;

    if !HOSTNAME_REGEX.is_match(hostname) {
        return Err(ValidationError::invalid_format(
            field,
            "RFC 1123 hostname",
            hostname,
        ));
    }

    Ok(Sanitized::new(hostname.to_lowercase()))
}

/// Sanitize and validate a service name.
///
/// Service names follow Kubernetes DNS-1035 conventions:
/// - Start with lowercase letter
/// - Contain only lowercase alphanumeric and hyphens
/// - Max 63 characters
/// - Cannot end with hyphen
///
/// # Errors
///
/// Returns `ValidationError` if the service name is invalid.
pub fn sanitize_service_name(name: &str) -> Result<Sanitized<ServiceName>, ValidationError> {
    let field = "service_name";
    let name = name.trim();

    if name.is_empty() {
        return Err(ValidationError::empty(field));
    }

    if name.len() > MAX_SERVICE_NAME_LENGTH {
        return Err(ValidationError::too_long(field, MAX_SERVICE_NAME_LENGTH, name.len()));
    }

    check_null_bytes(field, name)?;

    // Must be lowercase
    if name != name.to_lowercase() {
        return Err(ValidationError::invalid_format(
            field,
            "lowercase letters, numbers, and hyphens",
            name,
        ));
    }

    if !SERVICE_NAME_REGEX.is_match(name) {
        return Err(ValidationError::invalid_format(
            field,
            "DNS-1035: start with letter, alphanumeric and hyphens, no trailing hyphen",
            name,
        ));
    }

    Ok(Sanitized::new(name.to_string()))
}

/// Sanitize and validate a URL.
///
/// URLs must:
/// - Have a valid scheme (http, https, tcp, etc.)
/// - Not contain shell metacharacters
/// - Not exceed maximum length
///
/// # Errors
///
/// Returns `ValidationError` if the URL is invalid.
pub fn sanitize_url(url: &str) -> Result<Sanitized<Url>, ValidationError> {
    let field = "url";
    let url = url.trim();

    if url.is_empty() {
        return Err(ValidationError::empty(field));
    }

    if url.len() > MAX_URL_LENGTH {
        return Err(ValidationError::too_long(field, MAX_URL_LENGTH, url.len()));
    }

    check_null_bytes(field, url)?;
    check_shell_chars(field, url)?;

    if !URL_REGEX.is_match(url) {
        return Err(ValidationError::invalid_format(
            field,
            "scheme://host format",
            url,
        ));
    }

    Ok(Sanitized::new(url.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Node Name Tests
    // =========================================================================

    #[test]
    fn test_valid_node_name() {
        assert!(sanitize_node_name("my-node-01").is_ok());
        assert!(sanitize_node_name("node").is_ok());
        assert!(sanitize_node_name("Node_123").is_ok());
        assert!(sanitize_node_name("a").is_ok());
    }

    #[test]
    fn test_node_name_empty() {
        assert!(sanitize_node_name("").is_err());
        assert!(sanitize_node_name("   ").is_err());
    }

    #[test]
    fn test_node_name_too_long() {
        let long_name = "a".repeat(MAX_NODE_NAME_LENGTH + 1);
        assert!(sanitize_node_name(&long_name).is_err());
    }

    #[test]
    fn test_node_name_invalid_start() {
        assert!(sanitize_node_name("-node").is_err());
        assert!(sanitize_node_name("_node").is_err());
    }

    #[test]
    fn test_node_name_injection() {
        assert!(sanitize_node_name("; rm -rf /").is_err());
        assert!(sanitize_node_name("node\0name").is_err());
    }

    // =========================================================================
    // Environment Variable Tests
    // =========================================================================

    #[test]
    fn test_valid_env_var() {
        assert!(sanitize_env_var("MY_VAR", "value").is_ok());
        assert!(sanitize_env_var("_VAR", "value").is_ok());
        assert!(sanitize_env_var("VAR123", "value").is_ok());
    }

    #[test]
    fn test_env_var_empty_key() {
        assert!(sanitize_env_var("", "value").is_err());
    }

    #[test]
    fn test_env_var_invalid_key() {
        assert!(sanitize_env_var("123VAR", "value").is_err());
        assert!(sanitize_env_var("MY-VAR", "value").is_err());
    }

    #[test]
    fn test_env_var_null_byte() {
        assert!(sanitize_env_var("VAR", "val\0ue").is_err());
    }

    // =========================================================================
    // Image Name Tests
    // =========================================================================

    #[test]
    fn test_valid_image_name() {
        assert!(sanitize_image_name("nginx").is_ok());
        assert!(sanitize_image_name("nginx:latest").is_ok());
        assert!(sanitize_image_name("library/nginx").is_ok());
        assert!(sanitize_image_name("gcr.io/project/image:v1").is_ok());
    }

    #[test]
    fn test_image_name_empty() {
        assert!(sanitize_image_name("").is_err());
    }

    #[test]
    fn test_image_name_injection() {
        assert!(sanitize_image_name("image; rm -rf /").is_err());
        assert!(sanitize_image_name("$(whoami)").is_err());
    }

    // =========================================================================
    // Job ID Tests
    // =========================================================================

    #[test]
    fn test_valid_job_id() {
        assert!(sanitize_job_id("job-123").is_ok());
        assert!(sanitize_job_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    #[test]
    fn test_job_id_empty() {
        assert!(sanitize_job_id("").is_err());
    }

    // =========================================================================
    // Command Tests
    // =========================================================================

    #[test]
    fn test_valid_command() {
        assert!(sanitize_command("python").is_ok());
        assert!(sanitize_command("node").is_ok());
    }

    #[test]
    fn test_command_traversal() {
        assert!(sanitize_command("../bin/sh").is_err());
    }

    // =========================================================================
    // Command Arg Tests
    // =========================================================================

    #[test]
    fn test_valid_command_arg() {
        assert!(sanitize_command_arg("--flag").is_ok());
        assert!(sanitize_command_arg("value").is_ok());
    }

    #[test]
    fn test_command_arg_injection() {
        assert!(sanitize_command_arg("; rm -rf /").is_err());
        assert!(sanitize_command_arg("$(whoami)").is_err());
        assert!(sanitize_command_arg("`id`").is_err());
    }

    // =========================================================================
    // Path Tests
    // =========================================================================

    #[test]
    fn test_valid_path() {
        assert!(sanitize_path("data/file.txt").is_ok());
        assert!(sanitize_path("config.yaml").is_ok());
    }

    #[test]
    fn test_path_traversal() {
        assert!(sanitize_path("../../../etc/passwd").is_err());
        assert!(sanitize_path("dir/../../../root").is_err());
    }

    #[test]
    fn test_path_absolute() {
        assert!(sanitize_path("/etc/passwd").is_err());
        assert!(sanitize_path("C:\\Windows").is_err());
    }

    // =========================================================================
    // Hostname Tests
    // =========================================================================

    #[test]
    fn test_valid_hostname() {
        assert!(sanitize_hostname("localhost").is_ok());
        assert!(sanitize_hostname("my-host.example.com").is_ok());
        assert!(sanitize_hostname("host123").is_ok());
    }

    #[test]
    fn test_hostname_injection() {
        assert!(sanitize_hostname("host; cat /etc/passwd").is_err());
    }

    // =========================================================================
    // Service Name Tests
    // =========================================================================

    #[test]
    fn test_valid_service_name() {
        assert!(sanitize_service_name("my-api").is_ok());
        assert!(sanitize_service_name("api-v2").is_ok());
        assert!(sanitize_service_name("a").is_ok());
    }

    #[test]
    fn test_service_name_invalid() {
        assert!(sanitize_service_name("API").is_err()); // uppercase
        assert!(sanitize_service_name("-api").is_err()); // starts with dash
        assert!(sanitize_service_name("api-").is_err()); // ends with dash
    }

    // =========================================================================
    // URL Tests
    // =========================================================================

    #[test]
    fn test_valid_url() {
        assert!(sanitize_url("http://localhost:8080").is_ok());
        assert!(sanitize_url("https://example.com").is_ok());
        assert!(sanitize_url("tcp://localhost:5432").is_ok());
    }

    #[test]
    fn test_url_invalid() {
        assert!(sanitize_url("not-a-url").is_err());
        assert!(sanitize_url("http://example.com; rm -rf /").is_err());
    }
}
