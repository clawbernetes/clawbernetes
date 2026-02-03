//! CLI error types.

use std::fmt;

/// CLI-specific errors.
#[derive(Debug)]
pub enum CliError {
    /// Gateway connection failed.
    Connection(String),
    /// Invalid configuration.
    Config(String),
    /// Command execution failed.
    Command(String),
    /// Output formatting error.
    Format(String),
    /// Node not found.
    NodeNotFound(String),
    /// Invalid argument.
    InvalidArgument(String),
    /// IO error.
    Io(std::io::Error),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connection(msg) => write!(f, "connection error: {msg}"),
            Self::Config(msg) => write!(f, "configuration error: {msg}"),
            Self::Command(msg) => write!(f, "command error: {msg}"),
            Self::Format(msg) => write!(f, "format error: {msg}"),
            Self::NodeNotFound(id) => write!(f, "node not found: {id}"),
            Self::InvalidArgument(msg) => write!(f, "invalid argument: {msg}"),
            Self::Io(e) => write!(f, "IO error: {e}"),
        }
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for CliError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_error_display_connection() {
        let err = CliError::Connection("timeout".into());
        assert_eq!(err.to_string(), "connection error: timeout");
    }

    #[test]
    fn cli_error_display_node_not_found() {
        let err = CliError::NodeNotFound("node-123".into());
        assert_eq!(err.to_string(), "node not found: node-123");
    }

    #[test]
    fn cli_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let cli_err = CliError::from(io_err);
        assert!(matches!(cli_err, CliError::Io(_)));
    }
}
