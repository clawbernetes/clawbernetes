//! # claw-ddos
//!
//! Comprehensive `DDoS` protection layer for Clawbernetes.
//!
//! This crate provides multi-layer protection against various denial-of-service attacks:
//!
//! ## Connection-Level Protections
//!
//! - [`ConnectionLimiter`] - Limits concurrent connections per IP address
//! - [`SlowLorisProtection`] - Timeouts for slow/incomplete handshakes
//! - [`BandwidthLimiter`] - Per-connection bandwidth caps using token bucket
//!
//! ## Request-Level Protections
//!
//! - [`RequestRateLimiter`] - Requests per second limits using sliding window
//! - [`ComputeCostLimiter`] - Limits expensive operations per client
//!
//! ## Network-Level Protections
//!
//! - [`IpBlocklist`] - Persistent blocklist with automatic expiry
//! - [`GeoBlocking`] - Optional geographic restrictions
//! - [`ReputationTracker`] - Tracks bad behavior patterns and auto-escalates
//!
//! ## Configuration
//!
//! - [`DdosConfig`] - Unified configuration with sensible defaults
//! - Per-endpoint customizable limits
//! - Automatic escalation (rate limit → temp ban → permanent ban)
//!
//! # Example
//!
//! ```rust
//! use claw_ddos::{DdosProtection, DdosConfig, ProtectionResult};
//! use std::net::IpAddr;
//!
//! // Create protection layer with default config
//! let config = DdosConfig::default();
//! let protection = DdosProtection::new(config);
//!
//! // Check if a connection is allowed
//! let ip: IpAddr = "192.168.1.1".parse().unwrap();
//! match protection.check_connection(&ip) {
//!     ProtectionResult::Allow => println!("Connection allowed"),
//!     ProtectionResult::RateLimit { retry_after_ms } => {
//!         println!("Rate limited, retry after {}ms", retry_after_ms);
//!     }
//!     ProtectionResult::Block { reason, expires_at } => {
//!         println!("Blocked: {}", reason);
//!     }
//! }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod bandwidth;
pub mod blocklist;
pub mod config;
pub mod connection;
pub mod error;
pub mod geo;
pub mod protection;
pub mod rate_limit;
pub mod reputation;

// Re-export main types
pub use bandwidth::BandwidthLimiter;
pub use blocklist::IpBlocklist;
pub use config::{
    BandwidthConfig, BlocklistConfig, ConnectionConfig, DdosConfig, EndpointConfig,
    EscalationConfig, GeoConfig, RateLimitConfig, ReputationConfig,
};
pub use connection::{ConnectionLimiter, SlowLorisProtection};
pub use error::{DdosError, DdosResult};
pub use geo::GeoBlocking;
pub use protection::{DdosProtection, ProtectionResult};
pub use rate_limit::{ComputeCostLimiter, RequestRateLimiter};
pub use reputation::{ReputationScore, ReputationTracker, ViolationType};

/// Prelude for convenient imports.
pub mod prelude {
    pub use crate::bandwidth::BandwidthLimiter;
    pub use crate::blocklist::IpBlocklist;
    pub use crate::config::DdosConfig;
    pub use crate::connection::{ConnectionLimiter, SlowLorisProtection};
    pub use crate::error::{DdosError, DdosResult};
    pub use crate::geo::GeoBlocking;
    pub use crate::protection::{DdosProtection, ProtectionResult};
    pub use crate::rate_limit::{ComputeCostLimiter, RequestRateLimiter};
    pub use crate::reputation::{ReputationScore, ReputationTracker, ViolationType};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    #[test]
    fn test_basic_protection_flow() {
        let config = DdosConfig::default();
        let protection = DdosProtection::new(config);

        let ip: IpAddr = "10.0.0.1".parse().unwrap_or_else(|_| "127.0.0.1".parse().unwrap());
        
        // First connection should be allowed
        let result = protection.check_connection(&ip);
        assert!(matches!(result, ProtectionResult::Allow));
    }

    #[test]
    fn test_connection_limiter_integration() {
        let config = DdosConfig::builder()
            .connection(ConnectionConfig {
                max_per_ip: 2,
                ..ConnectionConfig::default()
            })
            .build();
        
        let protection = DdosProtection::new(config);
        let ip: IpAddr = "10.0.0.2".parse().unwrap_or_else(|_| "127.0.0.1".parse().unwrap());

        // Allow first two connections
        assert!(matches!(protection.check_connection(&ip), ProtectionResult::Allow));
        protection.on_connection_opened(&ip);
        
        assert!(matches!(protection.check_connection(&ip), ProtectionResult::Allow));
        protection.on_connection_opened(&ip);

        // Third should be blocked
        let result = protection.check_connection(&ip);
        assert!(matches!(result, ProtectionResult::RateLimit { .. }));
    }

    #[test]
    fn test_blocklist_integration() {
        let config = DdosConfig::default();
        let protection = DdosProtection::new(config);
        let ip: IpAddr = "10.0.0.3".parse().unwrap_or_else(|_| "127.0.0.1".parse().unwrap());

        // Add to blocklist
        protection.block_ip(&ip, "test block", None);

        // Should be blocked
        let result = protection.check_connection(&ip);
        assert!(matches!(result, ProtectionResult::Block { .. }));
    }
}
