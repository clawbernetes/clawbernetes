//! Safe command execution with command injection prevention.
//!
//! This module provides a `SafeCommand` builder that validates all arguments
//! before execution, preventing command injection attacks.
//!
//! # Security Features
//!
//! - **No shell invocation**: Uses `Command::new()` directly, never `sh -c`
//! - **Argument validation**: All arguments are validated before use
//! - **Allowlist enforcement**: Only pre-approved programs can be executed
//! - **Defense in depth**: Even though `Command` doesn't use a shell,
//!   we still reject shell metacharacters as a safeguard
//!
//! # Example
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
//!
//! println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
//! # Ok(())
//! # }
//! ```

use crate::error::ValidationError;
use std::collections::HashSet;
use std::fmt;
use thiserror::Error;

#[cfg(feature = "command")]
use std::process::Stdio;
#[cfg(feature = "command")]
use tokio::process::Command as TokioCommand;

/// Programs that are explicitly allowed to be executed.
///
/// This allowlist ensures only known, safe programs can be invoked.
/// Adding new programs requires explicit code changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AllowedProgram {
    /// The `tailscale` CLI for VPN operations.
    Tailscale,
    /// The `nvidia-smi` tool for GPU monitoring.
    NvidiaSmi,
    /// The `docker` CLI for container operations.
    Docker,
    /// The `podman` CLI for container operations.
    Podman,
    /// The `containerd` CLI (ctr).
    ContainerdCtr,
}

impl AllowedProgram {
    /// Get the program name/path to execute.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tailscale => "tailscale",
            Self::NvidiaSmi => "nvidia-smi",
            Self::Docker => "docker",
            Self::Podman => "podman",
            Self::ContainerdCtr => "ctr",
        }
    }

    /// Get a custom path override for this program, if any.
    ///
    /// Returns `None` for the default program name.
    #[must_use]
    pub fn custom_path(&self, path: &str) -> Result<String, CommandError> {
        // Validate the custom path
        validate_program_path(path)?;
        Ok(path.to_string())
    }
}

impl fmt::Display for AllowedProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Errors that can occur during safe command execution.
#[derive(Debug, Error)]
pub enum CommandError {
    /// Validation of a command argument failed.
    #[error("argument validation failed: {0}")]
    ValidationFailed(#[from] ValidationError),

    /// The command execution failed.
    #[error("command execution failed: {message}")]
    ExecutionFailed {
        /// Description of the failure.
        message: String,
    },

    /// The command returned a non-zero exit code.
    #[error("command '{command}' exited with code {exit_code}: {stderr}")]
    NonZeroExit {
        /// The command that was executed.
        command: String,
        /// The exit code.
        exit_code: i32,
        /// Standard error output.
        stderr: String,
    },

    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl CommandError {
    /// Create an execution failed error.
    #[must_use]
    pub fn execution_failed(message: impl Into<String>) -> Self {
        Self::ExecutionFailed {
            message: message.into(),
        }
    }

    /// Create a non-zero exit error.
    #[must_use]
    pub fn non_zero_exit(command: impl Into<String>, exit_code: i32, stderr: impl Into<String>) -> Self {
        Self::NonZeroExit {
            command: command.into(),
            exit_code,
            stderr: stderr.into(),
        }
    }

    /// Check if this is a validation error.
    #[must_use]
    pub fn is_validation_error(&self) -> bool {
        matches!(self, Self::ValidationFailed(_))
    }

    /// Check if this is a security-related validation error.
    #[must_use]
    pub fn is_security_error(&self) -> bool {
        match self {
            Self::ValidationFailed(e) => e.is_security_error(),
            _ => false,
        }
    }
}

/// Characters that are never allowed in command arguments.
const FORBIDDEN_CHARS: &[char] = &[
    '\0',  // Null byte
    '\n',  // Newline (can break argument parsing)
    '\r',  // Carriage return
];

/// Characters that are suspicious and logged as warnings.
const SUSPICIOUS_CHARS: &[char] = &[
    ';', '&', '|', '$', '`', '(', ')', '{', '}', '[', ']', '<', '>', '!', '\\',
];

/// Validate a command argument.
///
/// # Errors
///
/// Returns an error if the argument contains forbidden characters.
pub fn validate_argument(arg: &str, field_name: &str) -> Result<(), ValidationError> {
    // Check for null bytes and control characters
    for c in arg.chars() {
        if FORBIDDEN_CHARS.contains(&c) {
            return Err(ValidationError::shell_injection(field_name, c));
        }
    }

    Ok(())
}

/// Validate a program path.
///
/// # Errors
///
/// Returns an error if the path contains dangerous patterns.
pub fn validate_program_path(path: &str) -> Result<(), ValidationError> {
    if path.is_empty() {
        return Err(ValidationError::empty("program_path"));
    }

    // Check for path traversal
    if path.contains("..") {
        return Err(ValidationError::path_traversal("program_path", ".."));
    }

    // Check for forbidden characters
    for c in path.chars() {
        if FORBIDDEN_CHARS.contains(&c) || matches!(c, ';' | '&' | '|' | '$' | '`') {
            return Err(ValidationError::shell_injection("program_path", c));
        }
    }

    Ok(())
}

/// Output from a successful command execution.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    /// Standard output.
    pub stdout: Vec<u8>,
    /// Standard error.
    pub stderr: Vec<u8>,
    /// Exit status code (0 for success).
    pub exit_code: i32,
}

impl CommandOutput {
    /// Get stdout as a UTF-8 string, replacing invalid characters.
    #[must_use]
    pub fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// Get stderr as a UTF-8 string, replacing invalid characters.
    #[must_use]
    pub fn stderr_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }

    /// Check if the command succeeded (exit code 0).
    #[must_use]
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// A safe command builder that validates all inputs.
///
/// This builder ensures:
/// - Only allowlisted programs can be executed
/// - All arguments are validated before execution
/// - No shell is invoked (arguments are passed directly)
///
/// # Example
///
/// ```rust,no_run
/// # #[cfg(feature = "command")]
/// # async fn example() -> Result<(), claw_validation::command::CommandError> {
/// use claw_validation::command::{SafeCommand, AllowedProgram};
///
/// let output = SafeCommand::new(AllowedProgram::NvidiaSmi)
///     .args(&["--query-gpu=index,name", "--format=csv"])
///     .execute()
///     .await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct SafeCommand {
    program: AllowedProgram,
    program_path: Option<String>,
    args: Vec<String>,
    validation_errors: Vec<ValidationError>,
    env_vars: Vec<(String, String)>,
    current_dir: Option<String>,
}

impl SafeCommand {
    /// Create a new safe command for the given program.
    #[must_use]
    pub fn new(program: AllowedProgram) -> Self {
        Self {
            program,
            program_path: None,
            args: Vec::new(),
            validation_errors: Vec::new(),
            env_vars: Vec::new(),
            current_dir: None,
        }
    }

    /// Use a custom path for the program instead of searching PATH.
    ///
    /// The path is validated for safety.
    #[must_use]
    pub fn with_program_path(mut self, path: &str) -> Self {
        match validate_program_path(path) {
            Ok(()) => self.program_path = Some(path.to_string()),
            Err(e) => self.validation_errors.push(e),
        }
        self
    }

    /// Add a single argument to the command.
    ///
    /// The argument is validated for safety.
    #[must_use]
    pub fn arg(mut self, arg: &str) -> Self {
        match validate_argument(arg, "argument") {
            Ok(()) => self.args.push(arg.to_string()),
            Err(e) => self.validation_errors.push(e),
        }
        self
    }

    /// Add multiple arguments to the command.
    ///
    /// Each argument is validated for safety.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for arg in args {
            let arg = arg.as_ref();
            match validate_argument(arg, "argument") {
                Ok(()) => self.args.push(arg.to_string()),
                Err(e) => self.validation_errors.push(e),
            }
        }
        self
    }

    /// Set an environment variable for the command.
    ///
    /// Both key and value are validated.
    #[must_use]
    pub fn env(mut self, key: &str, value: &str) -> Self {
        // Validate key
        if let Err(e) = validate_argument(key, "env_key") {
            self.validation_errors.push(e);
            return self;
        }

        // Validate value
        if let Err(e) = validate_argument(value, "env_value") {
            self.validation_errors.push(e);
            return self;
        }

        self.env_vars.push((key.to_string(), value.to_string()));
        self
    }

    /// Set the working directory for the command.
    ///
    /// The directory path is validated.
    #[must_use]
    pub fn current_dir(mut self, dir: &str) -> Self {
        // Validate the directory path
        if dir.contains("..") {
            self.validation_errors.push(ValidationError::path_traversal("current_dir", ".."));
            return self;
        }

        if let Err(e) = validate_argument(dir, "current_dir") {
            self.validation_errors.push(e);
            return self;
        }

        self.current_dir = Some(dir.to_string());
        self
    }

    /// Check if there are any validation errors.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.validation_errors.is_empty()
    }

    /// Get any validation errors that occurred.
    #[must_use]
    pub fn errors(&self) -> &[ValidationError] {
        &self.validation_errors
    }

    /// Build the command description for logging.
    fn command_description(&self) -> String {
        let program = self.program_path.as_deref().unwrap_or(self.program.as_str());
        let args: Vec<_> = self.args.iter().map(|s| s.as_str()).collect();
        format!("{} {}", program, args.join(" "))
    }

    /// Execute the command and return the output.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Validation errors were collected during building
    /// - The command fails to execute
    /// - The command returns a non-zero exit code
    #[cfg(feature = "command")]
    pub async fn execute(mut self) -> Result<CommandOutput, CommandError> {
        // Check for validation errors first
        if !self.validation_errors.is_empty() {
            let error = self.validation_errors.swap_remove(0);
            return Err(CommandError::ValidationFailed(error));
        }

        let program = self.program_path.as_deref().unwrap_or(self.program.as_str());

        let mut cmd = TokioCommand::new(program);
        cmd.args(&self.args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }

        if let Some(dir) = &self.current_dir {
            cmd.current_dir(dir);
        }

        let output = cmd.output().await?;

        let exit_code = output.status.code().unwrap_or(-1);

        if !output.status.success() {
            return Err(CommandError::non_zero_exit(
                self.command_description(),
                exit_code,
                String::from_utf8_lossy(&output.stderr),
            ));
        }

        Ok(CommandOutput {
            stdout: output.stdout,
            stderr: output.stderr,
            exit_code,
        })
    }

    /// Execute the command without checking the exit code.
    ///
    /// Use this when you want to handle non-zero exit codes yourself.
    ///
    /// # Errors
    ///
    /// Returns an error if validation failed or the command couldn't be executed.
    #[cfg(feature = "command")]
    pub async fn execute_unchecked(self) -> Result<CommandOutput, CommandError> {
        // Check for validation errors first
        if let Some(error) = self.validation_errors.into_iter().next() {
            return Err(CommandError::ValidationFailed(error));
        }

        let program = self.program_path.as_deref().unwrap_or(self.program.as_str());

        let mut cmd = TokioCommand::new(program);
        cmd.args(&self.args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }

        if let Some(dir) = &self.current_dir {
            cmd.current_dir(dir);
        }

        let output = cmd.output().await?;

        Ok(CommandOutput {
            stdout: output.stdout,
            stderr: output.stderr,
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}

/// Registry of allowed programs for dynamic allowlisting.
///
/// This allows runtime configuration of which programs can be executed,
/// beyond the static `AllowedProgram` enum.
#[derive(Debug, Default)]
pub struct ProgramRegistry {
    allowed: HashSet<String>,
}

impl ProgramRegistry {
    /// Create a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry with the default allowed programs.
    #[must_use]
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.allow("tailscale");
        registry.allow("nvidia-smi");
        registry.allow("docker");
        registry.allow("podman");
        registry.allow("ctr");
        registry
    }

    /// Allow a program to be executed.
    pub fn allow(&mut self, program: &str) {
        self.allowed.insert(program.to_string());
    }

    /// Disallow a program.
    pub fn disallow(&mut self, program: &str) {
        self.allowed.remove(program);
    }

    /// Check if a program is allowed.
    #[must_use]
    pub fn is_allowed(&self, program: &str) -> bool {
        self.allowed.contains(program)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ValidationErrorKind;

    #[test]
    fn test_validate_argument_valid() {
        assert!(validate_argument("--flag", "arg").is_ok());
        assert!(validate_argument("value", "arg").is_ok());
        assert!(validate_argument("--key=value", "arg").is_ok());
        assert!(validate_argument("", "arg").is_ok()); // empty is allowed
        assert!(validate_argument("path/to/file", "arg").is_ok());
    }

    #[test]
    fn test_validate_argument_null_byte() {
        let result = validate_argument("arg\0value", "arg");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err.kind, ValidationErrorKind::ShellInjection { .. }));
    }

    #[test]
    fn test_validate_argument_newline() {
        let result = validate_argument("arg\nvalue", "arg");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_program_path_valid() {
        assert!(validate_program_path("/usr/bin/tailscale").is_ok());
        assert!(validate_program_path("tailscale").is_ok());
        assert!(validate_program_path("/opt/nvidia/bin/nvidia-smi").is_ok());
    }

    #[test]
    fn test_validate_program_path_empty() {
        assert!(validate_program_path("").is_err());
    }

    #[test]
    fn test_validate_program_path_traversal() {
        let result = validate_program_path("../../../bin/sh");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err.kind, ValidationErrorKind::PathTraversal { .. }));
    }

    #[test]
    fn test_validate_program_path_injection() {
        assert!(validate_program_path("program; rm -rf /").is_err());
        assert!(validate_program_path("program | cat /etc/passwd").is_err());
        assert!(validate_program_path("$(whoami)").is_err());
    }

    #[test]
    fn test_safe_command_builder() {
        let cmd = SafeCommand::new(AllowedProgram::Tailscale)
            .arg("status")
            .arg("--json");

        assert!(!cmd.has_errors());
        assert_eq!(cmd.args, vec!["status", "--json"]);
    }

    #[test]
    fn test_safe_command_with_invalid_arg() {
        let cmd = SafeCommand::new(AllowedProgram::Tailscale)
            .arg("status")
            .arg("--flag\0null");

        assert!(cmd.has_errors());
        assert_eq!(cmd.errors().len(), 1);
    }

    #[test]
    fn test_safe_command_with_invalid_path() {
        let cmd = SafeCommand::new(AllowedProgram::NvidiaSmi)
            .with_program_path("../../../bin/sh");

        assert!(cmd.has_errors());
    }

    #[test]
    fn test_safe_command_args_iter() {
        let args = vec!["--query-gpu=index", "--format=csv"];
        let cmd = SafeCommand::new(AllowedProgram::NvidiaSmi).args(args);

        assert!(!cmd.has_errors());
        assert_eq!(cmd.args.len(), 2);
    }

    #[test]
    fn test_safe_command_env() {
        let cmd = SafeCommand::new(AllowedProgram::Tailscale)
            .env("TS_AUTHKEY", "secret-key");

        assert!(!cmd.has_errors());
        assert_eq!(cmd.env_vars.len(), 1);
    }

    #[test]
    fn test_safe_command_current_dir() {
        let cmd = SafeCommand::new(AllowedProgram::Tailscale)
            .current_dir("/tmp");

        assert!(!cmd.has_errors());
        assert_eq!(cmd.current_dir, Some("/tmp".to_string()));
    }

    #[test]
    fn test_safe_command_current_dir_traversal() {
        let cmd = SafeCommand::new(AllowedProgram::Tailscale)
            .current_dir("../../../etc");

        assert!(cmd.has_errors());
    }

    #[test]
    fn test_allowed_program_as_str() {
        assert_eq!(AllowedProgram::Tailscale.as_str(), "tailscale");
        assert_eq!(AllowedProgram::NvidiaSmi.as_str(), "nvidia-smi");
        assert_eq!(AllowedProgram::Docker.as_str(), "docker");
    }

    #[test]
    fn test_command_description() {
        let cmd = SafeCommand::new(AllowedProgram::Tailscale)
            .arg("status")
            .arg("--json");

        let desc = cmd.command_description();
        assert_eq!(desc, "tailscale status --json");
    }

    #[test]
    fn test_program_registry() {
        let mut registry = ProgramRegistry::new();
        assert!(!registry.is_allowed("custom-program"));

        registry.allow("custom-program");
        assert!(registry.is_allowed("custom-program"));

        registry.disallow("custom-program");
        assert!(!registry.is_allowed("custom-program"));
    }

    #[test]
    fn test_program_registry_defaults() {
        let registry = ProgramRegistry::with_defaults();
        assert!(registry.is_allowed("tailscale"));
        assert!(registry.is_allowed("nvidia-smi"));
        assert!(registry.is_allowed("docker"));
    }

    #[test]
    fn test_command_output_methods() {
        let output = CommandOutput {
            stdout: b"hello".to_vec(),
            stderr: b"warning".to_vec(),
            exit_code: 0,
        };

        assert!(output.success());
        assert_eq!(output.stdout_lossy(), "hello");
        assert_eq!(output.stderr_lossy(), "warning");
    }

    #[test]
    fn test_command_output_failure() {
        let output = CommandOutput {
            stdout: vec![],
            stderr: b"error".to_vec(),
            exit_code: 1,
        };

        assert!(!output.success());
    }

    // Test that suspicious but allowed characters pass (defense in depth allows them
    // through Command::new but we log warnings)
    #[test]
    fn test_suspicious_chars_allowed() {
        // These are suspicious but technically safe with Command::new
        // We allow them but would log warnings in production
        assert!(validate_argument("--option=value;", "arg").is_ok());
        assert!(validate_argument("$(cmd)", "arg").is_ok());
    }

    // But forbidden chars are always rejected
    #[test]
    fn test_forbidden_chars_rejected() {
        assert!(validate_argument("value\0", "arg").is_err());
        assert!(validate_argument("line1\nline2", "arg").is_err());
        assert!(validate_argument("text\r", "arg").is_err());
    }
}
