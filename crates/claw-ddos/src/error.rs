//! Error types for `DDoS` protection.

use std::net::IpAddr;
use thiserror::Error;

/// Errors that can occur in `DDoS` protection operations.
#[derive(Debug, Error)]
pub enum DdosError {
    /// IP address is blocked.
    #[error("IP {ip} is blocked: {reason}")]
    Blocked {
        /// The blocked IP address.
        ip: IpAddr,
        /// Reason for the block.
        reason: String,
    },

    /// Rate limit exceeded.
    #[error("Rate limit exceeded for {ip}: {limit_type}")]
    RateLimitExceeded {
        /// The rate-limited IP address.
        ip: IpAddr,
        /// Type of rate limit that was exceeded.
        limit_type: String,
    },

    /// Connection limit exceeded.
    #[error("Connection limit exceeded for {ip}: {current}/{max}")]
    ConnectionLimitExceeded {
        /// The IP address.
        ip: IpAddr,
        /// Current connection count.
        current: u32,
        /// Maximum allowed.
        max: u32,
    },

    /// Bandwidth limit exceeded.
    #[error("Bandwidth limit exceeded for {ip}")]
    BandwidthExceeded {
        /// The IP address.
        ip: IpAddr,
    },

    /// Compute cost limit exceeded.
    #[error("Compute cost limit exceeded for {ip}: used {used}/{budget}")]
    ComputeCostExceeded {
        /// The IP address.
        ip: IpAddr,
        /// Cost units used.
        used: u64,
        /// Cost budget.
        budget: u64,
    },

    /// Geographic restriction.
    #[error("Geographic restriction: {country_code} is not allowed")]
    GeoRestricted {
        /// The restricted country code.
        country_code: String,
    },

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Internal error.
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for `DDoS` operations.
pub type DdosResult<T> = Result<T, DdosError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_blocked() {
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let err = DdosError::Blocked {
            ip,
            reason: "suspicious activity".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("1.2.3.4"));
        assert!(msg.contains("blocked"));
        assert!(msg.contains("suspicious activity"));
    }

    #[test]
    fn test_error_display_rate_limit() {
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let err = DdosError::RateLimitExceeded {
            ip,
            limit_type: "requests per second".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Rate limit exceeded"));
        assert!(msg.contains("requests per second"));
    }

    #[test]
    fn test_error_display_connection_limit() {
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let err = DdosError::ConnectionLimitExceeded {
            ip,
            current: 10,
            max: 5,
        };
        let msg = err.to_string();
        assert!(msg.contains("10/5"));
    }

    #[test]
    fn test_error_display_bandwidth() {
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let err = DdosError::BandwidthExceeded { ip };
        let msg = err.to_string();
        assert!(msg.contains("Bandwidth"));
    }

    #[test]
    fn test_error_display_compute_cost() {
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let err = DdosError::ComputeCostExceeded {
            ip,
            used: 1000,
            budget: 500,
        };
        let msg = err.to_string();
        assert!(msg.contains("1000/500"));
    }

    #[test]
    fn test_error_display_geo() {
        let err = DdosError::GeoRestricted {
            country_code: "XX".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("XX"));
    }

    #[test]
    fn test_error_display_config() {
        let err = DdosError::Config("invalid value".into());
        assert!(err.to_string().contains("invalid value"));
    }

    #[test]
    fn test_error_display_internal() {
        let err = DdosError::Internal("unexpected state".into());
        assert!(err.to_string().contains("unexpected state"));
    }
}
