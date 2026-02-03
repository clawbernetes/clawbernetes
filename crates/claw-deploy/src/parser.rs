//! Natural language intent parsing for deployment commands.
//!
//! This module parses human-readable deployment commands into structured
//! `DeploymentIntent` objects.
//!
//! ## Supported Patterns
//!
//! - `"deploy myapp:v2.0"` - Basic deployment
//! - `"deploy myapp:v2.0 with 3 replicas"` - With replica count
//! - `"deploy myapp:v2.0 with 2 GPUs"` - With GPU resources
//! - `"deploy myapp:v2.0 canary 10%"` - With canary hint
//! - `"deploy myapp:v2.0 to production"` - With environment
//! - `"rollback if errors > 1%"` - With error threshold

use crate::error::{DeployError, DeployResult};
use crate::types::{DeploymentConstraints, DeploymentIntent, Environment, StrategyHint};
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Keywords recognized in deployment intents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentKeyword {
    /// Deploy command
    Deploy,
    /// Replica count modifier
    Replicas,
    /// GPU resource modifier
    Gpus,
    /// Canary strategy hint
    Canary,
    /// Blue-green strategy hint
    BlueGreen,
    /// Rolling strategy hint
    Rolling,
    /// Immediate strategy hint
    Immediate,
    /// Production environment
    Production,
    /// Staging environment
    Staging,
    /// Development environment
    Dev,
    /// Rollback condition
    Rollback,
    /// Error threshold
    Errors,
    /// Memory requirement
    Memory,
    /// CPU requirement
    Cpu,
}

/// Parses a natural language deployment command into a `DeploymentIntent`.
///
/// # Examples
///
/// ```rust
/// use claw_deploy::parse_intent;
///
/// let intent = parse_intent("deploy myapp:v2.0 with 3 replicas");
/// assert!(intent.is_ok());
/// let intent = intent.unwrap();
/// assert_eq!(intent.image, "myapp:v2.0");
/// assert_eq!(intent.replicas, 3);
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - No image is specified
/// - The input cannot be parsed
/// - Invalid values are provided (e.g., negative replicas)
pub fn parse_intent(input: &str) -> DeployResult<DeploymentIntent> {
    debug!("Parsing intent: {}", input);

    let input = input.trim().to_lowercase();
    if input.is_empty() {
        return Err(DeployError::Parse("empty input".to_string()));
    }

    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.is_empty() {
        return Err(DeployError::Parse("no tokens found".to_string()));
    }

    // Parse the image (first token after "deploy" if present, or first token)
    let (image, remaining_tokens) = extract_image(&tokens)?;

    // Build the intent with defaults
    let mut intent = DeploymentIntent::new(image);
    let mut constraints = DeploymentConstraints::default();

    // Parse remaining tokens for modifiers
    let mut i = 0;
    while i < remaining_tokens.len() {
        let token = remaining_tokens[i];

        match token {
            // Replicas: "with X replicas" or "X replicas"
            "replicas" => {
                if i > 0 {
                    if let Some(count) = parse_number(remaining_tokens[i - 1]) {
                        intent.replicas = count;
                    }
                }
            }

            // GPUs: "with X gpus" or "X gpu" or "X gpus"
            "gpu" | "gpus" => {
                if i > 0 {
                    if let Some(count) = parse_number(remaining_tokens[i - 1]) {
                        intent.gpus = count;
                    }
                }
            }

            // Canary: "canary X%" or "canary"
            "canary" => {
                let percentage = if i + 1 < remaining_tokens.len() {
                    parse_percentage(remaining_tokens[i + 1]).unwrap_or(10)
                } else {
                    10 // Default canary percentage
                };
                intent.strategy_hint = Some(StrategyHint::Canary { percentage });
            }

            // Blue-green
            "blue-green" | "bluegreen" => {
                intent.strategy_hint = Some(StrategyHint::BlueGreen);
            }

            // Rolling: "rolling" or "rolling batch X"
            "rolling" => {
                let batch_size = if i + 2 < remaining_tokens.len()
                    && remaining_tokens[i + 1] == "batch"
                {
                    parse_number(remaining_tokens[i + 2]).unwrap_or(1)
                } else {
                    1
                };
                intent.strategy_hint = Some(StrategyHint::Rolling { batch_size });
            }

            // Immediate
            "immediate" | "immediately" => {
                intent.strategy_hint = Some(StrategyHint::Immediate);
            }

            // Environment: "to production" or "in staging" or just "production"
            "production" | "prod" => {
                constraints.environment = Some(Environment::Production);
            }
            "staging" | "stage" => {
                constraints.environment = Some(Environment::Staging);
            }
            "dev" | "development" => {
                constraints.environment = Some(Environment::Dev);
            }

            // Error threshold: "rollback if errors > X%"
            "errors" => {
                // Look for the percentage after ">" or following token
                if i + 2 < remaining_tokens.len() && remaining_tokens[i + 1] == ">" {
                    if let Some(pct) = parse_percentage(remaining_tokens[i + 2]) {
                        constraints.max_error_rate = Some(f64::from(pct));
                    }
                }
            }

            // Memory: "X mb memory" or "memory X mb"
            "memory" | "mb" => {
                if i > 0 {
                    if let Some(mem) = parse_number(remaining_tokens[i - 1]) {
                        constraints.min_memory_mb = Some(u64::from(mem));
                    }
                }
            }

            // CPU: "X cores" or "X cpu"
            "cores" | "cpu" => {
                if i > 0 {
                    if let Some(cores) = parse_number(remaining_tokens[i - 1]) {
                        constraints.min_cpu_cores = Some(cores);
                    }
                }
            }

            _ => {}
        }

        i += 1;
    }

    intent.constraints = constraints;
    intent.validate()?;

    debug!("Parsed intent: {:?}", intent);
    Ok(intent)
}

/// Extracts the image from tokens.
fn extract_image<'a>(tokens: &[&'a str]) -> DeployResult<(String, Vec<&'a str>)> {
    if tokens.is_empty() {
        return Err(DeployError::Parse("no image specified".to_string()));
    }

    // Skip "deploy" if present
    let start_idx = usize::from(tokens[0] == "deploy");

    if start_idx >= tokens.len() {
        return Err(DeployError::Parse("no image specified after 'deploy'".to_string()));
    }

    // The image is the first meaningful token
    let image = tokens[start_idx].to_string();

    // Validate it looks like an image (contains : or is alphanumeric with possible /)
    if !is_valid_image_token(&image) {
        return Err(DeployError::Parse(format!("invalid image: {image}")));
    }

    let remaining = tokens[start_idx + 1..].to_vec();
    Ok((image, remaining))
}

/// Checks if a token looks like a valid container image.
fn is_valid_image_token(token: &str) -> bool {
    // Must contain alphanumeric characters
    // Can contain :, /, -, _, .
    // Cannot be a keyword
    let keywords = [
        "with", "to", "in", "and", "canary", "rolling", "immediate", "blue-green",
        "production", "staging", "dev", "replicas", "gpus", "gpu",
    ];

    if keywords.contains(&token) {
        return false;
    }

    // Must have at least one alphanumeric char
    token.chars().any(char::is_alphanumeric)
}

/// Parses a number from a string.
fn parse_number(s: &str) -> Option<u32> {
    s.parse().ok()
}

/// Parses a percentage from a string (e.g., "10%", "10", "0.1").
fn parse_percentage(s: &str) -> Option<u8> {
    let s = s.trim_end_matches('%');
    if let Ok(n) = s.parse::<u8>() {
        if n <= 100 {
            return Some(n);
        }
    }
    // Handle decimal like "0.1" meaning 10%
    if let Ok(f) = s.parse::<f64>() {
        if f > 0.0 && f <= 1.0 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            return Some((f * 100.0) as u8);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    mod parse_intent_tests {
        use super::*;

        #[test]
        fn parses_basic_deploy() {
            let intent = parse_intent("deploy myapp:v1.0");
            assert!(intent.is_ok());
            let intent = intent.map_err(|e| e.to_string());
            assert_eq!(intent.as_ref().map(|i| i.image.as_str()).ok(), Some("myapp:v1.0"));
            assert_eq!(intent.as_ref().map(|i| i.replicas).ok(), Some(1));
        }

        #[test]
        fn parses_image_without_deploy_keyword() {
            let intent = parse_intent("myapp:v1.0");
            assert!(intent.is_ok());
            let intent = intent.map_err(|e| e.to_string());
            assert_eq!(intent.as_ref().map(|i| i.image.as_str()).ok(), Some("myapp:v1.0"));
        }

        #[test]
        fn parses_replicas() {
            let intent = parse_intent("deploy myapp:v1.0 with 5 replicas");
            assert!(intent.is_ok());
            let intent = intent.map_err(|e| e.to_string());
            assert_eq!(intent.as_ref().map(|i| i.replicas).ok(), Some(5));
        }

        #[test]
        fn parses_gpus() {
            let intent = parse_intent("deploy myapp:v1.0 with 2 gpus");
            assert!(intent.is_ok());
            let intent = intent.map_err(|e| e.to_string());
            assert_eq!(intent.as_ref().map(|i| i.gpus).ok(), Some(2));
        }

        #[test]
        fn parses_single_gpu() {
            let intent = parse_intent("deploy myapp:v1.0 with 1 gpu");
            assert!(intent.is_ok());
            let intent = intent.map_err(|e| e.to_string());
            assert_eq!(intent.as_ref().map(|i| i.gpus).ok(), Some(1));
        }

        #[test]
        fn parses_canary_with_percentage() {
            let intent = parse_intent("deploy myapp:v1.0 canary 20%");
            assert!(intent.is_ok());
            let intent = intent.map_err(|e| e.to_string());
            assert_eq!(
                intent.as_ref().map(|i| i.strategy_hint.clone()).ok().flatten(),
                Some(StrategyHint::Canary { percentage: 20 })
            );
        }

        #[test]
        fn parses_canary_without_percentage_defaults_to_10() {
            let intent = parse_intent("deploy myapp:v1.0 canary");
            assert!(intent.is_ok());
            let intent = intent.map_err(|e| e.to_string());
            assert_eq!(
                intent.as_ref().map(|i| i.strategy_hint.clone()).ok().flatten(),
                Some(StrategyHint::Canary { percentage: 10 })
            );
        }

        #[test]
        fn parses_blue_green() {
            let intent = parse_intent("deploy myapp:v1.0 blue-green");
            assert!(intent.is_ok());
            let intent = intent.map_err(|e| e.to_string());
            assert_eq!(
                intent.as_ref().map(|i| i.strategy_hint.clone()).ok().flatten(),
                Some(StrategyHint::BlueGreen)
            );
        }

        #[test]
        fn parses_rolling_with_batch_size() {
            let intent = parse_intent("deploy myapp:v1.0 rolling batch 3");
            assert!(intent.is_ok());
            let intent = intent.map_err(|e| e.to_string());
            assert_eq!(
                intent.as_ref().map(|i| i.strategy_hint.clone()).ok().flatten(),
                Some(StrategyHint::Rolling { batch_size: 3 })
            );
        }

        #[test]
        fn parses_immediate() {
            let intent = parse_intent("deploy myapp:v1.0 immediately");
            assert!(intent.is_ok());
            let intent = intent.map_err(|e| e.to_string());
            assert_eq!(
                intent.as_ref().map(|i| i.strategy_hint.clone()).ok().flatten(),
                Some(StrategyHint::Immediate)
            );
        }

        #[test]
        fn parses_production_environment() {
            let intent = parse_intent("deploy myapp:v1.0 to production");
            assert!(intent.is_ok());
            let intent = intent.map_err(|e| e.to_string());
            assert_eq!(
                intent.as_ref().map(|i| i.constraints.environment).ok().flatten(),
                Some(Environment::Production)
            );
        }

        #[test]
        fn parses_staging_environment() {
            let intent = parse_intent("deploy myapp:v1.0 to staging");
            assert!(intent.is_ok());
            let intent = intent.map_err(|e| e.to_string());
            assert_eq!(
                intent.as_ref().map(|i| i.constraints.environment).ok().flatten(),
                Some(Environment::Staging)
            );
        }

        #[test]
        fn parses_error_threshold() {
            let intent = parse_intent("deploy myapp:v1.0 rollback if errors > 5%");
            assert!(intent.is_ok());
            let intent = intent.map_err(|e| e.to_string());
            assert_eq!(
                intent.as_ref().map(|i| i.constraints.max_error_rate).ok().flatten(),
                Some(5.0)
            );
        }

        #[test]
        fn parses_complex_command() {
            let intent =
                parse_intent("deploy myapp:v2.0 with 5 replicas and 2 gpus canary 15% to production");
            assert!(intent.is_ok());
            let intent = intent.map_err(|e| e.to_string()).ok();
            assert_eq!(intent.as_ref().map(|i| i.image.as_str()), Some("myapp:v2.0"));
            assert_eq!(intent.as_ref().map(|i| i.replicas), Some(5));
            assert_eq!(intent.as_ref().map(|i| i.gpus), Some(2));
            assert_eq!(
                intent.as_ref().map(|i| i.strategy_hint.clone()).flatten(),
                Some(StrategyHint::Canary { percentage: 15 })
            );
            assert_eq!(
                intent.as_ref().map(|i| i.constraints.environment).flatten(),
                Some(Environment::Production)
            );
        }

        #[test]
        fn rejects_empty_input() {
            let result = parse_intent("");
            assert!(result.is_err());
        }

        #[test]
        fn rejects_deploy_without_image() {
            let result = parse_intent("deploy");
            assert!(result.is_err());
        }

        #[test]
        fn case_insensitive() {
            let intent = parse_intent("DEPLOY MyApp:V1.0 WITH 3 REPLICAS");
            assert!(intent.is_ok());
            let intent = intent.map_err(|e| e.to_string());
            assert_eq!(intent.as_ref().map(|i| i.replicas).ok(), Some(3));
        }

        #[test]
        fn handles_extra_whitespace() {
            let intent = parse_intent("  deploy   myapp:v1.0   with   3   replicas  ");
            assert!(intent.is_ok());
            let intent = intent.map_err(|e| e.to_string());
            assert_eq!(intent.as_ref().map(|i| i.replicas).ok(), Some(3));
        }
    }

    mod parse_percentage_tests {
        use super::*;

        #[test]
        fn parses_integer_with_percent() {
            assert_eq!(parse_percentage("10%"), Some(10));
            assert_eq!(parse_percentage("100%"), Some(100));
        }

        #[test]
        fn parses_integer_without_percent() {
            assert_eq!(parse_percentage("10"), Some(10));
            assert_eq!(parse_percentage("100"), Some(100));
        }

        #[test]
        fn parses_decimal_as_percentage() {
            assert_eq!(parse_percentage("0.1"), Some(10));
            assert_eq!(parse_percentage("0.5"), Some(50));
        }

        #[test]
        fn rejects_over_100() {
            assert_eq!(parse_percentage("150%"), None);
            assert_eq!(parse_percentage("101"), None);
        }
    }

    mod is_valid_image_token_tests {
        use super::*;

        #[test]
        fn accepts_valid_images() {
            assert!(is_valid_image_token("myapp:v1.0"));
            assert!(is_valid_image_token("registry.io/myapp:latest"));
            assert!(is_valid_image_token("my-app_test:v1.0.0"));
        }

        #[test]
        fn rejects_keywords() {
            assert!(!is_valid_image_token("with"));
            assert!(!is_valid_image_token("canary"));
            assert!(!is_valid_image_token("production"));
        }
    }
}
