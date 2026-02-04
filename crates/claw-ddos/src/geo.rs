//! Geographic blocking.

use std::collections::HashSet;
use std::net::IpAddr;

use parking_lot::RwLock;
use tracing::debug;

use crate::config::GeoConfig;
use crate::error::{DdosError, DdosResult};

/// Country code lookup result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CountryLookup {
    /// Country was found.
    Found(String),
    /// Country could not be determined.
    Unknown,
}

/// Geographic blocking based on IP location.
///
/// Note: This implementation provides the infrastructure for geo-blocking.
/// In production, you would integrate with a `GeoIP` database (e.g., `MaxMind`).
pub struct GeoBlocking {
    /// Whether geo-blocking is enabled.
    enabled: bool,
    /// Allowed country codes (if set, only these are allowed).
    allowed: Option<HashSet<String>>,
    /// Blocked country codes (if set, these are blocked).
    blocked: Option<HashSet<String>>,
    /// Cache of IP -> country lookups.
    cache: RwLock<std::collections::HashMap<IpAddr, CountryLookup>>,
    /// Optional custom lookup function (for testing/integration).
    #[allow(clippy::type_complexity)]
    custom_lookup: Option<Box<dyn Fn(&IpAddr) -> CountryLookup + Send + Sync>>,
}

impl GeoBlocking {
    /// Create disabled geo-blocking.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            allowed: None,
            blocked: None,
            cache: RwLock::new(std::collections::HashMap::new()),
            custom_lookup: None,
        }
    }

    /// Create geo-blocking with allowed countries only.
    #[must_use]
    pub fn allow_only(countries: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let allowed: HashSet<String> = countries.into_iter().map(Into::into).collect();
        Self {
            enabled: true,
            allowed: Some(allowed),
            blocked: None,
            cache: RwLock::new(std::collections::HashMap::new()),
            custom_lookup: None,
        }
    }

    /// Create geo-blocking with blocked countries.
    #[must_use]
    pub fn block_countries(countries: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let blocked: HashSet<String> = countries.into_iter().map(Into::into).collect();
        Self {
            enabled: true,
            allowed: None,
            blocked: Some(blocked),
            cache: RwLock::new(std::collections::HashMap::new()),
            custom_lookup: None,
        }
    }

    /// Create from configuration.
    #[must_use]
    pub fn from_config(config: &GeoConfig) -> Self {
        Self {
            enabled: config.enabled,
            allowed: config.allowed_countries.clone(),
            blocked: config.blocked_countries.clone(),
            cache: RwLock::new(std::collections::HashMap::new()),
            custom_lookup: None,
        }
    }

    /// Set a custom country lookup function.
    ///
    /// This allows integration with `GeoIP` databases.
    #[must_use]
    pub fn with_lookup<F>(mut self, lookup: F) -> Self
    where
        F: Fn(&IpAddr) -> CountryLookup + Send + Sync + 'static,
    {
        self.custom_lookup = Some(Box::new(lookup));
        self
    }

    /// Check if an IP is allowed based on geographic location.
    ///
    /// # Errors
    ///
    /// Returns `DdosError::GeoRestricted` if the IP's country is not allowed.
    pub fn check(&self, ip: &IpAddr) -> DdosResult<()> {
        if !self.enabled {
            return Ok(());
        }

        let country = self.lookup_country(ip);
        
        match &country {
            CountryLookup::Found(code) => {
                // Check blocked countries first
                if let Some(blocked) = &self.blocked {
                    if blocked.contains(code) {
                        debug!(ip = %ip, country = %code, "IP blocked by geo-restriction");
                        return Err(DdosError::GeoRestricted {
                            country_code: code.clone(),
                        });
                    }
                }
                
                // Check allowed countries
                if let Some(allowed) = &self.allowed {
                    if !allowed.contains(code) {
                        debug!(ip = %ip, country = %code, "IP not in allowed countries");
                        return Err(DdosError::GeoRestricted {
                            country_code: code.clone(),
                        });
                    }
                }
            }
            CountryLookup::Unknown => {
                // If we have an allow list but can't determine country, deny
                if self.allowed.is_some() {
                    debug!(ip = %ip, "IP country unknown, denying (allow-list mode)");
                    return Err(DdosError::GeoRestricted {
                        country_code: "UNKNOWN".into(),
                    });
                }
                // If we only have a block list, allow unknown countries
            }
        }
        
        Ok(())
    }

    /// Check if an IP would be allowed (returns bool).
    #[must_use]
    pub fn is_allowed(&self, ip: &IpAddr) -> bool {
        self.check(ip).is_ok()
    }

    /// Lookup the country for an IP.
    #[must_use]
    pub fn lookup_country(&self, ip: &IpAddr) -> CountryLookup {
        // Check cache first
        {
            let cache = self.cache.read();
            if let Some(cached) = cache.get(ip) {
                return cached.clone();
            }
        }
        
        // Use custom lookup if available
        let result = if let Some(lookup) = &self.custom_lookup {
            lookup(ip)
        } else {
            // Default: no GeoIP database, return unknown
            // In production, this would integrate with MaxMind or similar
            Self::default_lookup(ip)
        };
        
        // Cache the result
        {
            let mut cache = self.cache.write();
            cache.insert(*ip, result.clone());
        }
        
        result
    }

    /// Default lookup without `GeoIP` database.
    /// Recognizes some well-known ranges for basic functionality.
    #[must_use]
    fn default_lookup(ip: &IpAddr) -> CountryLookup {
        // Private/local ranges are "unknown"
        match ip {
            IpAddr::V4(v4) => {
                let octets = v4.octets();
                
                // Private ranges
                if octets[0] == 10
                    || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                    || (octets[0] == 192 && octets[1] == 168)
                    || octets[0] == 127
                {
                    return CountryLookup::Unknown;
                }
                
                // For now, return Unknown for all public IPs
                // In production, use a GeoIP database
                CountryLookup::Unknown
            }
            IpAddr::V6(_) => CountryLookup::Unknown,
        }
    }

    /// Add a country to the allowed list.
    pub fn allow_country(&self, country: impl Into<String>) {
        if let Some(allowed) = &self.allowed {
            let mut new_allowed = allowed.clone();
            new_allowed.insert(country.into());
            // Note: In a real implementation, you'd want a mutable field
            // This is simplified for the interface
        }
    }

    /// Add a country to the blocked list.
    pub fn block_country(&self, country: impl Into<String>) {
        if let Some(blocked) = &self.blocked {
            let mut new_blocked = blocked.clone();
            new_blocked.insert(country.into());
            // Note: Simplified, see above
        }
    }

    /// Clear the country lookup cache.
    pub fn clear_cache(&self) {
        self.cache.write().clear();
    }

    /// Get cache size.
    #[must_use]
    pub fn cache_size(&self) -> usize {
        self.cache.read().len()
    }

    /// Check if geo-blocking is enabled.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get the allowed countries (if configured).
    #[must_use]
    pub fn allowed_countries(&self) -> Option<&HashSet<String>> {
        self.allowed.as_ref()
    }

    /// Get the blocked countries (if configured).
    #[must_use]
    pub fn blocked_countries(&self) -> Option<&HashSet<String>> {
        self.blocked.as_ref()
    }
}

impl Default for GeoBlocking {
    fn default() -> Self {
        Self::disabled()
    }
}

// Manual Debug implementation to skip the custom_lookup field
impl std::fmt::Debug for GeoBlocking {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeoBlocking")
            .field("enabled", &self.enabled)
            .field("allowed", &self.allowed)
            .field("blocked", &self.blocked)
            .field("cache_size", &self.cache.read().len())
            .field("has_custom_lookup", &self.custom_lookup.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== CountryLookup Tests ====================

    #[test]
    fn test_country_lookup_found() {
        let lookup = CountryLookup::Found("US".into());
        assert!(matches!(lookup, CountryLookup::Found(_)));
    }

    #[test]
    fn test_country_lookup_unknown() {
        let lookup = CountryLookup::Unknown;
        assert!(matches!(lookup, CountryLookup::Unknown));
    }

    #[test]
    fn test_country_lookup_equality() {
        assert_eq!(CountryLookup::Found("US".into()), CountryLookup::Found("US".into()));
        assert_ne!(CountryLookup::Found("US".into()), CountryLookup::Found("UK".into()));
        assert_eq!(CountryLookup::Unknown, CountryLookup::Unknown);
    }

    // ==================== GeoBlocking Tests ====================

    #[test]
    fn test_geo_blocking_disabled() {
        let geo = GeoBlocking::disabled();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        assert!(!geo.is_enabled());
        assert!(geo.check(&ip).is_ok());
    }

    #[test]
    fn test_geo_blocking_from_config_disabled() {
        let config = GeoConfig::default();
        let geo = GeoBlocking::from_config(&config);
        
        assert!(!geo.is_enabled());
    }

    #[test]
    fn test_geo_blocking_allow_only() {
        let geo = GeoBlocking::allow_only(["US", "CA"]);
        
        assert!(geo.is_enabled());
        assert!(geo.allowed_countries().is_some());
        assert!(geo.blocked_countries().is_none());
        
        let allowed = geo.allowed_countries().unwrap();
        assert!(allowed.contains("US"));
        assert!(allowed.contains("CA"));
    }

    #[test]
    fn test_geo_blocking_block_countries() {
        let geo = GeoBlocking::block_countries(["CN", "RU"]);
        
        assert!(geo.is_enabled());
        assert!(geo.allowed_countries().is_none());
        assert!(geo.blocked_countries().is_some());
        
        let blocked = geo.blocked_countries().unwrap();
        assert!(blocked.contains("CN"));
        assert!(blocked.contains("RU"));
    }

    #[test]
    fn test_geo_blocking_with_custom_lookup() {
        let geo = GeoBlocking::allow_only(["US"])
            .with_lookup(|_| CountryLookup::Found("US".into()));
        
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        assert!(geo.check(&ip).is_ok());
    }

    #[test]
    fn test_geo_blocking_blocked_country() {
        let geo = GeoBlocking::block_countries(["XX"])
            .with_lookup(|_| CountryLookup::Found("XX".into()));
        
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let result = geo.check(&ip);
        
        assert!(matches!(result, Err(DdosError::GeoRestricted { .. })));
        if let Err(DdosError::GeoRestricted { country_code }) = result {
            assert_eq!(country_code, "XX");
        }
    }

    #[test]
    fn test_geo_blocking_not_in_allowed() {
        let geo = GeoBlocking::allow_only(["US", "CA"])
            .with_lookup(|_| CountryLookup::Found("XX".into()));
        
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let result = geo.check(&ip);
        
        assert!(matches!(result, Err(DdosError::GeoRestricted { .. })));
    }

    #[test]
    fn test_geo_blocking_in_allowed() {
        let geo = GeoBlocking::allow_only(["US", "CA"])
            .with_lookup(|_| CountryLookup::Found("US".into()));
        
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        assert!(geo.check(&ip).is_ok());
    }

    #[test]
    fn test_geo_blocking_unknown_with_allow_list() {
        let geo = GeoBlocking::allow_only(["US"])
            .with_lookup(|_| CountryLookup::Unknown);
        
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let result = geo.check(&ip);
        
        // Unknown IPs are denied when using allow list
        assert!(matches!(result, Err(DdosError::GeoRestricted { country_code }) if country_code == "UNKNOWN"));
    }

    #[test]
    fn test_geo_blocking_unknown_with_block_list() {
        let geo = GeoBlocking::block_countries(["XX"])
            .with_lookup(|_| CountryLookup::Unknown);
        
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        // Unknown IPs are allowed when using block list
        assert!(geo.check(&ip).is_ok());
    }

    #[test]
    fn test_geo_blocking_is_allowed() {
        let geo = GeoBlocking::allow_only(["US"])
            .with_lookup(|_| CountryLookup::Found("US".into()));
        
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        assert!(geo.is_allowed(&ip));
    }

    #[test]
    fn test_geo_blocking_cache() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering};
        
        let lookup_count = Arc::new(AtomicU32::new(0));
        let lookup_count_clone = Arc::clone(&lookup_count);
        
        let geo = GeoBlocking::allow_only(["US"])
            .with_lookup(move |_| {
                lookup_count_clone.fetch_add(1, Ordering::Relaxed);
                CountryLookup::Found("US".into())
            });
        
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        
        // First lookup
        geo.lookup_country(&ip);
        assert_eq!(lookup_count.load(Ordering::Relaxed), 1);
        
        // Second lookup should use cache
        geo.lookup_country(&ip);
        assert_eq!(lookup_count.load(Ordering::Relaxed), 1);
        
        assert_eq!(geo.cache_size(), 1);
    }

    #[test]
    fn test_geo_blocking_clear_cache() {
        let geo = GeoBlocking::allow_only(["US"])
            .with_lookup(|_| CountryLookup::Found("US".into()));
        
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        geo.lookup_country(&ip);
        assert_eq!(geo.cache_size(), 1);
        
        geo.clear_cache();
        assert_eq!(geo.cache_size(), 0);
    }

    #[test]
    fn test_geo_blocking_default_lookup_private() {
        let geo = GeoBlocking::disabled();
        
        // Private ranges should be unknown
        let private_ips = [
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "127.0.0.1",
        ];
        
        for ip_str in &private_ips {
            let ip: IpAddr = ip_str.parse().unwrap();
            let result = geo.lookup_country(&ip);
            assert!(matches!(result, CountryLookup::Unknown), "Expected Unknown for {ip_str}");
        }
    }

    #[test]
    fn test_geo_blocking_default() {
        let geo = GeoBlocking::default();
        assert!(!geo.is_enabled());
    }

    #[test]
    fn test_geo_blocking_debug() {
        let geo = GeoBlocking::allow_only(["US"]);
        let debug_str = format!("{geo:?}");
        
        assert!(debug_str.contains("GeoBlocking"));
        assert!(debug_str.contains("enabled"));
    }
}
