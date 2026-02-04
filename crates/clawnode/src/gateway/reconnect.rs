//! Reconnection configuration and exponential backoff.

use std::time::Duration;
use tokio::time::sleep;

/// Configuration for reconnection behavior.
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// Initial delay before first reconnection attempt.
    pub initial_delay: Duration,
    /// Maximum delay between reconnection attempts.
    pub max_delay: Duration,
    /// Multiplier for exponential backoff.
    pub backoff_multiplier: f64,
    /// Maximum number of reconnection attempts (None = infinite).
    pub max_attempts: Option<u32>,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            max_attempts: None,
        }
    }
}

impl ReconnectConfig {
    /// Calculate delay for the given attempt number.
    #[must_use]
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let multiplier = self.backoff_multiplier.powi(attempt.saturating_sub(1) as i32);
        let delay_millis = (self.initial_delay.as_millis() as f64 * multiplier) as u64;
        Duration::from_millis(delay_millis).min(self.max_delay)
    }

    /// Check if we should attempt reconnection.
    #[must_use]
    pub const fn should_reconnect(&self, attempt: u32) -> bool {
        match self.max_attempts {
            Some(max) => attempt < max,
            None => true,
        }
    }
}

/// Calculate reconnection delay using exponential backoff.
#[must_use]
pub fn calculate_backoff(
    attempt: u32,
    initial_delay: Duration,
    max_delay: Duration,
    multiplier: f64,
) -> Duration {
    let factor = multiplier.powi(attempt.saturating_sub(1) as i32);
    let delay_millis = (initial_delay.as_millis() as f64 * factor) as u64;
    Duration::from_millis(delay_millis).min(max_delay)
}

/// Attempt to reconnect with exponential backoff.
pub async fn reconnect_with_backoff<F, Fut, T, E>(
    config: &ReconnectConfig,
    mut connect_fn: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut attempt = 0;

    loop {
        attempt += 1;

        match connect_fn().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if !config.should_reconnect(attempt) {
                    return Err(e);
                }

                let delay = config.delay_for_attempt(attempt);
                sleep(delay).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_reconnect_config_default() {
        let config = ReconnectConfig::default();
        assert_eq!(config.initial_delay, Duration::from_secs(1));
        assert_eq!(config.max_delay, Duration::from_secs(60));
        assert_eq!(config.backoff_multiplier, 2.0);
        assert!(config.max_attempts.is_none());
    }

    #[test]
    fn test_delay_for_attempt() {
        let config = ReconnectConfig {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            max_attempts: None,
        };

        assert_eq!(config.delay_for_attempt(1), Duration::from_secs(1));
        assert_eq!(config.delay_for_attempt(2), Duration::from_secs(2));
        assert_eq!(config.delay_for_attempt(3), Duration::from_secs(4));
        assert_eq!(config.delay_for_attempt(4), Duration::from_secs(8));
        assert_eq!(config.delay_for_attempt(5), Duration::from_secs(16));
        assert_eq!(config.delay_for_attempt(6), Duration::from_secs(32));
        assert_eq!(config.delay_for_attempt(7), Duration::from_secs(60)); // capped
    }

    #[test]
    fn test_should_reconnect_infinite() {
        let config = ReconnectConfig {
            max_attempts: None,
            ..Default::default()
        };

        assert!(config.should_reconnect(1));
        assert!(config.should_reconnect(100));
        assert!(config.should_reconnect(1000));
    }

    #[test]
    fn test_should_reconnect_limited() {
        let config = ReconnectConfig {
            max_attempts: Some(5),
            ..Default::default()
        };

        assert!(config.should_reconnect(1));
        assert!(config.should_reconnect(4));
        assert!(!config.should_reconnect(5));
        assert!(!config.should_reconnect(6));
    }

    #[test]
    fn test_calculate_backoff() {
        let initial = Duration::from_millis(100);
        let max = Duration::from_secs(10);

        assert_eq!(
            calculate_backoff(1, initial, max, 2.0),
            Duration::from_millis(100)
        );
        assert_eq!(
            calculate_backoff(2, initial, max, 2.0),
            Duration::from_millis(200)
        );
        assert_eq!(
            calculate_backoff(3, initial, max, 2.0),
            Duration::from_millis(400)
        );
        assert_eq!(calculate_backoff(10, initial, max, 2.0), Duration::from_secs(10)); // capped
    }

    #[test]
    fn test_delay_with_zero_attempt() {
        let config = ReconnectConfig::default();
        let delay = config.delay_for_attempt(0);
        assert_eq!(delay, Duration::from_secs(1));
    }

    #[test]
    fn test_backoff_with_different_multipliers() {
        let initial = Duration::from_millis(100);
        let max = Duration::from_secs(60);

        // multiplier 1.5
        assert_eq!(
            calculate_backoff(2, initial, max, 1.5),
            Duration::from_millis(150)
        );
        assert_eq!(
            calculate_backoff(3, initial, max, 1.5),
            Duration::from_millis(225)
        );

        // multiplier 3.0
        assert_eq!(
            calculate_backoff(2, initial, max, 3.0),
            Duration::from_millis(300)
        );
        assert_eq!(
            calculate_backoff(3, initial, max, 3.0),
            Duration::from_millis(900)
        );
    }

    #[tokio::test]
    async fn test_reconnect_with_backoff_success_first_try() {
        let config = ReconnectConfig {
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            max_attempts: Some(3),
        };

        let mut attempts = 0;
        let result: Result<i32, &str> = reconnect_with_backoff(&config, || {
            attempts += 1;
            async move { Ok(42) }
        })
        .await;

        assert_eq!(result, Ok(42));
        assert_eq!(attempts, 1);
    }

    #[tokio::test]
    async fn test_reconnect_with_backoff_success_after_retries() {
        let config = ReconnectConfig {
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            backoff_multiplier: 2.0,
            max_attempts: Some(5),
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let result: Result<i32, &str> = reconnect_with_backoff(&config, || {
            let a = attempts_clone.fetch_add(1, Ordering::SeqCst);
            async move {
                if a < 2 {
                    Err("not yet")
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result, Ok(42));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_reconnect_with_backoff_max_attempts_exceeded() {
        let config = ReconnectConfig {
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            backoff_multiplier: 2.0,
            max_attempts: Some(3),
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let result: Result<i32, &str> = reconnect_with_backoff(&config, || {
            attempts_clone.fetch_add(1, Ordering::SeqCst);
            async move { Err("always fail") }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
}
