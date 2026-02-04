//! Node configuration.
//!
//! Configuration for the Clawbernetes node agent, including:
//! - Node identity and naming
//! - Gateway connection settings
//! - GPU configuration
//! - Network/WireGuard settings
//! - Optional MOLT network settings

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::NodeError;
use crate::network::{NetworkConfig, WireGuardConfig};

/// Configuration for GPU resources on this node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GpuConfig {
    /// Whether to enable GPU detection.
    pub enabled: bool,
    /// Polling interval for GPU metrics in seconds.
    pub poll_interval_secs: u64,
    /// Maximum temperature threshold in Celsius before throttling warnings.
    pub max_temperature_celsius: Option<u32>,
}

impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            poll_interval_secs: 5,
            max_temperature_celsius: Some(85),
        }
    }
}

/// Configuration for MOLT network participation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MoltConfig {
    /// Whether to enable MOLT P2P network.
    pub enabled: bool,
    /// Minimum price per GPU-hour in microdollars.
    pub min_price_microdollars: u64,
    /// Maximum concurrent jobs to accept.
    pub max_concurrent_jobs: u32,
}

impl Default for MoltConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_price_microdollars: 100_000, // $0.10/hr default
            max_concurrent_jobs: 4,
        }
    }
}

/// Main node configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeConfig {
    /// Human-readable node name.
    pub name: String,
    /// Gateway WebSocket URL.
    pub gateway_url: String,
    /// GPU configuration.
    #[serde(default)]
    pub gpu: GpuConfig,
    /// Network configuration.
    #[serde(default)]
    pub network: NetworkConfig,
    /// MOLT configuration.
    #[serde(default)]
    pub molt: MoltConfig,
}

impl NodeConfig {
    /// Load configuration from a TOML file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, NodeError> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(|e| {
            NodeError::Config(format!(
                "failed to read config file '{}': {}",
                path.as_ref().display(),
                e
            ))
        })?;

        Self::from_toml(&content)
    }

    /// Parse configuration from a TOML string.
    ///
    /// # Errors
    ///
    /// Returns an error if the TOML is invalid.
    pub fn from_toml(content: &str) -> Result<Self, NodeError> {
        let config: Self = toml::from_str(content)
            .map_err(|e| NodeError::Config(format!("invalid TOML: {e}")))?;

        config.validate()?;
        Ok(config)
    }

    /// Validate the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if any configuration values are invalid.
    pub fn validate(&self) -> Result<(), NodeError> {
        if self.name.is_empty() {
            return Err(NodeError::Config("node name cannot be empty".to_string()));
        }

        if self.name.len() > 64 {
            return Err(NodeError::Config(
                "node name cannot exceed 64 characters".to_string(),
            ));
        }

        // Validate name contains only allowed characters
        if !self
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(NodeError::Config(
                "node name must contain only alphanumeric characters, hyphens, and underscores"
                    .to_string(),
            ));
        }

        if self.gateway_url.is_empty() {
            return Err(NodeError::Config(
                "gateway_url cannot be empty".to_string(),
            ));
        }

        // Basic URL validation
        if !self.gateway_url.starts_with("ws://") && !self.gateway_url.starts_with("wss://") {
            return Err(NodeError::Config(
                "gateway_url must start with ws:// or wss://".to_string(),
            ));
        }

        if self.gpu.poll_interval_secs == 0 {
            return Err(NodeError::Config(
                "gpu.poll_interval_secs must be greater than 0".to_string(),
            ));
        }

        if self.molt.max_concurrent_jobs == 0 {
            return Err(NodeError::Config(
                "molt.max_concurrent_jobs must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Helper to create a temporary config file
    fn create_temp_config(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("failed to create temp file");
        file.write_all(content.as_bytes())
            .expect("failed to write temp file");
        file
    }

    #[test]
    fn test_parse_minimal_config() {
        let toml = r#"
            name = "test-node"
            gateway_url = "wss://gateway.example.com:8080"
        "#;

        let config = NodeConfig::from_toml(toml).expect("should parse minimal config");

        assert_eq!(config.name, "test-node");
        assert_eq!(config.gateway_url, "wss://gateway.example.com:8080");
        // Defaults should be applied
        assert!(config.gpu.enabled);
        assert_eq!(config.gpu.poll_interval_secs, 5);
        assert!(!config.molt.enabled);
    }

    #[test]
    fn test_parse_full_config() {
        let toml = r#"
            name = "gpu-node-01"
            gateway_url = "wss://gateway.example.com:8080"

            [gpu]
            enabled = true
            poll_interval_secs = 10
            max_temperature_celsius = 90

            [molt]
            enabled = true
            min_price_microdollars = 200000
            max_concurrent_jobs = 8
        "#;

        let config = NodeConfig::from_toml(toml).expect("should parse full config");

        assert_eq!(config.name, "gpu-node-01");
        assert_eq!(config.gateway_url, "wss://gateway.example.com:8080");
        assert!(config.gpu.enabled);
        assert_eq!(config.gpu.poll_interval_secs, 10);
        assert_eq!(config.gpu.max_temperature_celsius, Some(90));
        assert!(config.molt.enabled);
        assert_eq!(config.molt.min_price_microdollars, 200_000);
        assert_eq!(config.molt.max_concurrent_jobs, 8);
    }

    #[test]
    fn test_load_from_file() {
        let toml = r#"
            name = "file-node"
            gateway_url = "ws://localhost:9000"
        "#;

        let temp_file = create_temp_config(toml);
        let config = NodeConfig::from_file(temp_file.path()).expect("should load from file");

        assert_eq!(config.name, "file-node");
        assert_eq!(config.gateway_url, "ws://localhost:9000");
    }

    #[test]
    fn test_file_not_found() {
        let result = NodeConfig::from_file("/nonexistent/path/config.toml");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NodeError::Config(_)));
    }

    #[test]
    fn test_empty_name_rejected() {
        let toml = r#"
            name = ""
            gateway_url = "wss://gateway.example.com"
        "#;

        let result = NodeConfig::from_toml(toml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("name cannot be empty"));
    }

    #[test]
    fn test_name_too_long_rejected() {
        let long_name = "a".repeat(65);
        let toml = format!(
            r#"
            name = "{long_name}"
            gateway_url = "wss://gateway.example.com"
        "#
        );

        let result = NodeConfig::from_toml(&toml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("cannot exceed 64 characters"));
    }

    #[test]
    fn test_invalid_name_characters_rejected() {
        let toml = r#"
            name = "node with spaces"
            gateway_url = "wss://gateway.example.com"
        "#;

        let result = NodeConfig::from_toml(toml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("alphanumeric"));
    }

    #[test]
    fn test_empty_gateway_url_rejected() {
        let toml = r#"
            name = "test-node"
            gateway_url = ""
        "#;

        let result = NodeConfig::from_toml(toml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("gateway_url cannot be empty"));
    }

    #[test]
    fn test_invalid_gateway_url_scheme_rejected() {
        let toml = r#"
            name = "test-node"
            gateway_url = "http://gateway.example.com"
        "#;

        let result = NodeConfig::from_toml(toml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("ws:// or wss://"));
    }

    #[test]
    fn test_zero_poll_interval_rejected() {
        let toml = r#"
            name = "test-node"
            gateway_url = "wss://gateway.example.com"

            [gpu]
            enabled = true
            poll_interval_secs = 0
        "#;

        let result = NodeConfig::from_toml(toml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("poll_interval_secs must be greater than 0"));
    }

    #[test]
    fn test_zero_max_concurrent_jobs_rejected() {
        let toml = r#"
            name = "test-node"
            gateway_url = "wss://gateway.example.com"

            [molt]
            enabled = true
            min_price_microdollars = 100000
            max_concurrent_jobs = 0
        "#;

        let result = NodeConfig::from_toml(toml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("max_concurrent_jobs must be greater than 0"));
    }

    #[test]
    fn test_invalid_toml_rejected() {
        let toml = "this is not valid toml {{{";

        let result = NodeConfig::from_toml(toml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid TOML"));
    }

    #[test]
    fn test_gpu_config_default() {
        let default = GpuConfig::default();
        assert!(default.enabled);
        assert_eq!(default.poll_interval_secs, 5);
        assert_eq!(default.max_temperature_celsius, Some(85));
    }

    #[test]
    fn test_molt_config_default() {
        let default = MoltConfig::default();
        assert!(!default.enabled);
        assert_eq!(default.min_price_microdollars, 100_000);
        assert_eq!(default.max_concurrent_jobs, 4);
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let original = NodeConfig {
            name: "roundtrip-node".to_string(),
            gateway_url: "wss://gateway.example.com:8080".to_string(),
            gpu: GpuConfig {
                enabled: true,
                poll_interval_secs: 15,
                max_temperature_celsius: Some(80),
            },
            network: NetworkConfig::default(),
            molt: MoltConfig {
                enabled: true,
                min_price_microdollars: 50_000,
                max_concurrent_jobs: 2,
            },
        };

        let toml_str = toml::to_string(&original).expect("should serialize");
        let parsed = NodeConfig::from_toml(&toml_str).expect("should parse");

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_valid_name_with_hyphens_and_underscores() {
        let toml = r#"
            name = "my-gpu_node-01"
            gateway_url = "wss://gateway.example.com"
        "#;

        let config = NodeConfig::from_toml(toml).expect("should accept hyphens and underscores");
        assert_eq!(config.name, "my-gpu_node-01");
    }

    #[test]
    fn test_ws_scheme_accepted() {
        let toml = r#"
            name = "local-node"
            gateway_url = "ws://localhost:8080"
        "#;

        let config = NodeConfig::from_toml(toml).expect("should accept ws:// scheme");
        assert_eq!(config.gateway_url, "ws://localhost:8080");
    }

    // =========================================================================
    // Additional Coverage Tests
    // =========================================================================

    #[test]
    fn test_gpu_config_equality() {
        let config1 = GpuConfig {
            enabled: true,
            poll_interval_secs: 5,
            max_temperature_celsius: Some(85),
        };
        let config2 = GpuConfig {
            enabled: true,
            poll_interval_secs: 5,
            max_temperature_celsius: Some(85),
        };
        let config3 = GpuConfig {
            enabled: false,
            poll_interval_secs: 10,
            max_temperature_celsius: None,
        };

        assert_eq!(config1, config2);
        assert_ne!(config1, config3);
    }

    #[test]
    fn test_gpu_config_clone() {
        let config = GpuConfig::default();
        let cloned = config.clone();
        assert_eq!(config, cloned);
    }

    #[test]
    fn test_gpu_config_debug() {
        let config = GpuConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("GpuConfig"));
    }

    #[test]
    fn test_gpu_config_serialization() {
        let config = GpuConfig {
            enabled: true,
            poll_interval_secs: 10,
            max_temperature_celsius: Some(90),
        };
        let json = serde_json::to_string(&config).expect("serialize");
        let parsed: GpuConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(config, parsed);
    }

    #[test]
    fn test_molt_config_equality() {
        let config1 = MoltConfig::default();
        let config2 = MoltConfig::default();
        assert_eq!(config1, config2);
    }

    #[test]
    fn test_molt_config_clone() {
        let config = MoltConfig::default();
        let cloned = config.clone();
        assert_eq!(config, cloned);
    }

    #[test]
    fn test_molt_config_debug() {
        let config = MoltConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("MoltConfig"));
    }

    #[test]
    fn test_molt_config_serialization() {
        let config = MoltConfig {
            enabled: true,
            min_price_microdollars: 500_000,
            max_concurrent_jobs: 16,
        };
        let json = serde_json::to_string(&config).expect("serialize");
        let parsed: MoltConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(config, parsed);
    }

    #[test]
    fn test_node_config_debug() {
        let toml = r#"
            name = "debug-node"
            gateway_url = "wss://gateway.example.com"
        "#;
        let config = NodeConfig::from_toml(toml).expect("should parse");
        let debug = format!("{:?}", config);
        assert!(debug.contains("NodeConfig"));
        assert!(debug.contains("debug-node"));
    }

    #[test]
    fn test_node_config_clone() {
        let toml = r#"
            name = "clone-node"
            gateway_url = "wss://gateway.example.com"
        "#;
        let config = NodeConfig::from_toml(toml).expect("should parse");
        let cloned = config.clone();
        assert_eq!(config, cloned);
    }

    #[test]
    fn test_max_name_length_exactly_64() {
        let name = "a".repeat(64);
        let toml = format!(
            r#"
            name = "{name}"
            gateway_url = "wss://gateway.example.com"
        "#
        );

        let config = NodeConfig::from_toml(&toml).expect("should accept 64 char name");
        assert_eq!(config.name.len(), 64);
    }

    #[test]
    fn test_name_with_numbers() {
        let toml = r#"
            name = "node123"
            gateway_url = "wss://gateway.example.com"
        "#;

        let config = NodeConfig::from_toml(toml).expect("should accept numbers");
        assert_eq!(config.name, "node123");
    }

    #[test]
    fn test_name_all_numbers() {
        let toml = r#"
            name = "12345"
            gateway_url = "wss://gateway.example.com"
        "#;

        let config = NodeConfig::from_toml(toml).expect("should accept all numbers");
        assert_eq!(config.name, "12345");
    }

    #[test]
    fn test_name_single_char() {
        let toml = r#"
            name = "a"
            gateway_url = "wss://gateway.example.com"
        "#;

        let config = NodeConfig::from_toml(toml).expect("should accept single char");
        assert_eq!(config.name, "a");
    }

    #[test]
    fn test_gpu_disabled() {
        let toml = r#"
            name = "no-gpu-node"
            gateway_url = "wss://gateway.example.com"

            [gpu]
            enabled = false
            poll_interval_secs = 60
        "#;

        let config = NodeConfig::from_toml(toml).expect("should parse");
        assert!(!config.gpu.enabled);
        assert_eq!(config.gpu.poll_interval_secs, 60);
    }

    #[test]
    fn test_gpu_explicit_temperature_threshold() {
        let toml = r#"
            name = "cool-node"
            gateway_url = "wss://gateway.example.com"

            [gpu]
            enabled = true
            poll_interval_secs = 5
            max_temperature_celsius = 75
        "#;

        let config = NodeConfig::from_toml(toml).expect("should parse");
        assert_eq!(config.gpu.max_temperature_celsius, Some(75));
    }

    #[test]
    fn test_molt_high_price() {
        let toml = r#"
            name = "expensive-node"
            gateway_url = "wss://gateway.example.com"

            [molt]
            enabled = true
            min_price_microdollars = 10000000
            max_concurrent_jobs = 1
        "#;

        let config = NodeConfig::from_toml(toml).expect("should parse");
        assert_eq!(config.molt.min_price_microdollars, 10_000_000);
    }

    #[test]
    fn test_wss_with_port() {
        let toml = r#"
            name = "port-node"
            gateway_url = "wss://gateway.example.com:443"
        "#;

        let config = NodeConfig::from_toml(toml).expect("should parse");
        assert!(config.gateway_url.contains(":443"));
    }

    #[test]
    fn test_wss_with_path() {
        let toml = r#"
            name = "path-node"
            gateway_url = "wss://gateway.example.com/api/v1/ws"
        "#;

        let config = NodeConfig::from_toml(toml).expect("should parse");
        assert!(config.gateway_url.contains("/api/v1/ws"));
    }

    #[test]
    fn test_validate_method_directly() {
        let config = NodeConfig {
            name: "valid-node".to_string(),
            gateway_url: "wss://example.com".to_string(),
            gpu: GpuConfig::default(),
            network: NetworkConfig::default(),
            molt: MoltConfig::default(),
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_name_directly() {
        let config = NodeConfig {
            name: "".to_string(),
            gateway_url: "wss://example.com".to_string(),
            gpu: GpuConfig::default(),
            network: NetworkConfig::default(),
            molt: MoltConfig::default(),
        };

        assert!(config.validate().is_err());
    }
}
