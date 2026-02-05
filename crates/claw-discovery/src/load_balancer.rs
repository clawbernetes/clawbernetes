//! Load balancer implementations for distributing traffic across endpoints.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;
use rand::Rng;
use thiserror::Error;

use crate::types::{Endpoint, EndpointId, HealthStatus, LoadBalancerStrategy};

/// Errors that can occur during load balancing.
#[derive(Debug, Error)]
pub enum LoadBalancerError {
    /// No healthy endpoints available.
    #[error("no healthy endpoints available")]
    NoHealthyEndpoints,

    /// No endpoints registered.
    #[error("no endpoints registered")]
    NoEndpoints,

    /// Endpoint not found.
    #[error("endpoint {0} not found")]
    EndpointNotFound(EndpointId),

    /// Invalid weight configuration.
    #[error("invalid weight configuration: total weight is zero")]
    ZeroTotalWeight,
}

/// Result type for load balancer operations.
pub type Result<T> = std::result::Result<T, LoadBalancerError>;

/// A load balancer that distributes traffic across endpoints.
#[derive(Debug)]
pub struct LoadBalancer {
    /// The load balancing strategy.
    strategy: LoadBalancerStrategy,
    /// Registered endpoints.
    endpoints: RwLock<HashMap<EndpointId, Endpoint>>,
    /// Round-robin counter.
    round_robin_counter: AtomicU64,
    /// Session affinity map (client IP -> endpoint ID).
    session_map: RwLock<HashMap<IpAddr, EndpointId>>,
}

impl LoadBalancer {
    /// Creates a new load balancer with the given strategy.
    #[must_use]
    pub fn new(strategy: LoadBalancerStrategy) -> Self {
        Self {
            strategy,
            endpoints: RwLock::new(HashMap::new()),
            round_robin_counter: AtomicU64::new(0),
            session_map: RwLock::new(HashMap::new()),
        }
    }

    /// Creates a round-robin load balancer.
    #[must_use]
    pub fn round_robin() -> Self {
        Self::new(LoadBalancerStrategy::RoundRobin)
    }

    /// Creates a least-connections load balancer.
    #[must_use]
    pub fn least_connections() -> Self {
        Self::new(LoadBalancerStrategy::LeastConnections)
    }

    /// Creates a random load balancer.
    #[must_use]
    pub fn random() -> Self {
        Self::new(LoadBalancerStrategy::Random)
    }

    /// Creates a weighted random load balancer.
    #[must_use]
    pub fn weighted_random() -> Self {
        Self::new(LoadBalancerStrategy::WeightedRandom)
    }

    /// Creates an IP hash load balancer.
    #[must_use]
    pub fn ip_hash() -> Self {
        Self::new(LoadBalancerStrategy::IpHash)
    }

    /// Returns the current strategy.
    #[must_use]
    pub fn strategy(&self) -> &LoadBalancerStrategy {
        &self.strategy
    }

    /// Adds an endpoint to the load balancer.
    pub fn add_endpoint(&self, endpoint: Endpoint) {
        let mut endpoints = self.endpoints.write();
        endpoints.insert(endpoint.id, endpoint);
    }

    /// Removes an endpoint from the load balancer.
    pub fn remove_endpoint(&self, endpoint_id: EndpointId) -> Option<Endpoint> {
        let mut endpoints = self.endpoints.write();
        let endpoint = endpoints.remove(&endpoint_id);

        // Also clean up any session affinity entries pointing to this endpoint
        if endpoint.is_some() {
            let mut session_map = self.session_map.write();
            session_map.retain(|_, &mut eid| eid != endpoint_id);
        }

        endpoint
    }

    /// Gets an endpoint by ID.
    #[must_use]
    pub fn get_endpoint(&self, endpoint_id: EndpointId) -> Option<Endpoint> {
        let endpoints = self.endpoints.read();
        endpoints.get(&endpoint_id).cloned()
    }

    /// Gets a mutable reference to an endpoint and applies a function.
    pub fn with_endpoint_mut<F, R>(&self, endpoint_id: EndpointId, f: F) -> Option<R>
    where
        F: FnOnce(&mut Endpoint) -> R,
    {
        let mut endpoints = self.endpoints.write();
        endpoints.get_mut(&endpoint_id).map(f)
    }

    /// Updates an endpoint's health status.
    pub fn update_health(&self, endpoint_id: EndpointId, status: HealthStatus) {
        let mut endpoints = self.endpoints.write();
        if let Some(endpoint) = endpoints.get_mut(&endpoint_id) {
            endpoint.health_status = status;
        }
    }

    /// Returns the number of registered endpoints.
    #[must_use]
    pub fn endpoint_count(&self) -> usize {
        self.endpoints.read().len()
    }

    /// Returns the number of healthy endpoints.
    #[must_use]
    pub fn healthy_endpoint_count(&self) -> usize {
        self.endpoints
            .read()
            .values()
            .filter(|e| e.is_ready())
            .count()
    }

    /// Returns all registered endpoints.
    #[must_use]
    pub fn endpoints(&self) -> Vec<Endpoint> {
        self.endpoints.read().values().cloned().collect()
    }

    /// Returns all healthy endpoints.
    #[must_use]
    pub fn healthy_endpoints(&self) -> Vec<Endpoint> {
        self.endpoints
            .read()
            .values()
            .filter(|e| e.is_ready())
            .cloned()
            .collect()
    }

    /// Selects an endpoint for the given client IP.
    ///
    /// # Errors
    ///
    /// Returns an error if no healthy endpoints are available.
    pub fn select(&self, client_ip: Option<IpAddr>) -> Result<Endpoint> {
        let endpoints = self.endpoints.read();

        if endpoints.is_empty() {
            return Err(LoadBalancerError::NoEndpoints);
        }

        let healthy: Vec<&Endpoint> = endpoints.values().filter(|e| e.is_ready()).collect();

        if healthy.is_empty() {
            return Err(LoadBalancerError::NoHealthyEndpoints);
        }

        let selected = match &self.strategy {
            LoadBalancerStrategy::RoundRobin => self.select_round_robin(&healthy),
            LoadBalancerStrategy::LeastConnections => self.select_least_connections(&healthy),
            LoadBalancerStrategy::Random => self.select_random(&healthy),
            LoadBalancerStrategy::WeightedRandom => self.select_weighted_random(&healthy)?,
            LoadBalancerStrategy::IpHash => self.select_ip_hash(&healthy, client_ip),
        };

        Ok(selected.clone())
    }

    /// Selects an endpoint with session affinity.
    ///
    /// If the client has an existing session, the same endpoint is returned
    /// (if it's still healthy). Otherwise, a new endpoint is selected and
    /// the session is recorded.
    ///
    /// # Errors
    ///
    /// Returns an error if no healthy endpoints are available.
    pub fn select_with_affinity(&self, client_ip: IpAddr) -> Result<Endpoint> {
        // Check for existing session
        {
            let session_map = self.session_map.read();
            if let Some(&endpoint_id) = session_map.get(&client_ip) {
                let endpoints = self.endpoints.read();
                if let Some(endpoint) = endpoints.get(&endpoint_id) {
                    if endpoint.is_ready() {
                        return Ok(endpoint.clone());
                    }
                }
            }
        }

        // Select new endpoint and record session
        let endpoint = self.select(Some(client_ip))?;

        let mut session_map = self.session_map.write();
        session_map.insert(client_ip, endpoint.id);

        Ok(endpoint)
    }

    /// Clears all session affinity mappings.
    pub fn clear_sessions(&self) {
        let mut session_map = self.session_map.write();
        session_map.clear();
    }

    /// Clears the session for a specific client.
    pub fn clear_session(&self, client_ip: IpAddr) {
        let mut session_map = self.session_map.write();
        session_map.remove(&client_ip);
    }

    /// Records a connection to an endpoint.
    pub fn record_connection(&self, endpoint_id: EndpointId) {
        self.with_endpoint_mut(endpoint_id, super::types::Endpoint::connect);
    }

    /// Records a disconnection from an endpoint.
    pub fn record_disconnection(&self, endpoint_id: EndpointId) {
        self.with_endpoint_mut(endpoint_id, super::types::Endpoint::disconnect);
    }

    // ==================== Private Selection Methods ====================

    fn select_round_robin<'a>(&self, endpoints: &[&'a Endpoint]) -> &'a Endpoint {
        let counter = self.round_robin_counter.fetch_add(1, Ordering::Relaxed);
        let index = (counter as usize) % endpoints.len();
        endpoints[index]
    }

    fn select_least_connections<'a>(&self, endpoints: &[&'a Endpoint]) -> &'a Endpoint {
        endpoints
            .iter()
            .min_by_key(|e| e.active_connections)
            .copied()
            .unwrap_or(endpoints[0])
    }

    fn select_random<'a>(&self, endpoints: &[&'a Endpoint]) -> &'a Endpoint {
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..endpoints.len());
        endpoints[index]
    }

    fn select_weighted_random<'a>(&self, endpoints: &[&'a Endpoint]) -> Result<&'a Endpoint> {
        let total_weight: u64 = endpoints.iter().map(|e| u64::from(e.weight)).sum();

        if total_weight == 0 {
            return Err(LoadBalancerError::ZeroTotalWeight);
        }

        let mut rng = rand::thread_rng();
        let random_value = rng.gen_range(0..total_weight);

        let mut cumulative = 0u64;
        for endpoint in endpoints {
            cumulative += u64::from(endpoint.weight);
            if random_value < cumulative {
                return Ok(endpoint);
            }
        }

        // Fallback to last endpoint (shouldn't happen with valid weights)
        Ok(endpoints.last().unwrap_or(&endpoints[0]))
    }

    fn select_ip_hash<'a>(
        &self,
        endpoints: &[&'a Endpoint],
        client_ip: Option<IpAddr>,
    ) -> &'a Endpoint {
        let hash = if let Some(ip) = client_ip {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            ip.hash(&mut hasher);
            hasher.finish()
        } else {
            // Fallback to random if no client IP
            let mut rng = rand::thread_rng();
            rng.r#gen::<u64>()
        };

        let index = (hash as usize) % endpoints.len();
        endpoints[index]
    }
}

impl Default for LoadBalancer {
    fn default() -> Self {
        Self::round_robin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // ==================== Helper Functions ====================

    fn make_endpoint(ip: &str, port: u16) -> Endpoint {
        let mut endpoint = Endpoint::builder(ip.parse().ok().unwrap(), port).build();
        endpoint.health_status = HealthStatus::Healthy;
        endpoint
    }

    fn make_weighted_endpoint(ip: &str, port: u16, weight: u32) -> Endpoint {
        let mut endpoint = Endpoint::builder(ip.parse().ok().unwrap(), port)
            .weight(weight)
            .build();
        endpoint.health_status = HealthStatus::Healthy;
        endpoint
    }

    fn make_unhealthy_endpoint(ip: &str, port: u16) -> Endpoint {
        let mut endpoint = Endpoint::builder(ip.parse().ok().unwrap(), port).build();
        endpoint.health_status = HealthStatus::Unhealthy;
        endpoint
    }

    // ==================== Constructor Tests ====================

    #[test]
    fn test_new_round_robin() {
        let lb = LoadBalancer::round_robin();
        assert_eq!(lb.strategy(), &LoadBalancerStrategy::RoundRobin);
    }

    #[test]
    fn test_new_least_connections() {
        let lb = LoadBalancer::least_connections();
        assert_eq!(lb.strategy(), &LoadBalancerStrategy::LeastConnections);
    }

    #[test]
    fn test_new_random() {
        let lb = LoadBalancer::random();
        assert_eq!(lb.strategy(), &LoadBalancerStrategy::Random);
    }

    #[test]
    fn test_new_weighted_random() {
        let lb = LoadBalancer::weighted_random();
        assert_eq!(lb.strategy(), &LoadBalancerStrategy::WeightedRandom);
    }

    #[test]
    fn test_new_ip_hash() {
        let lb = LoadBalancer::ip_hash();
        assert_eq!(lb.strategy(), &LoadBalancerStrategy::IpHash);
    }

    #[test]
    fn test_default_is_round_robin() {
        let lb = LoadBalancer::default();
        assert_eq!(lb.strategy(), &LoadBalancerStrategy::RoundRobin);
    }

    // ==================== Endpoint Management Tests ====================

    #[test]
    fn test_add_endpoint() {
        let lb = LoadBalancer::round_robin();
        let endpoint = make_endpoint("10.0.0.1", 8080);

        lb.add_endpoint(endpoint);

        assert_eq!(lb.endpoint_count(), 1);
    }

    #[test]
    fn test_remove_endpoint() {
        let lb = LoadBalancer::round_robin();
        let endpoint = make_endpoint("10.0.0.1", 8080);
        let id = endpoint.id;

        lb.add_endpoint(endpoint);
        assert_eq!(lb.endpoint_count(), 1);

        let removed = lb.remove_endpoint(id);
        assert!(removed.is_some());
        assert_eq!(lb.endpoint_count(), 0);
    }

    #[test]
    fn test_remove_nonexistent_endpoint() {
        let lb = LoadBalancer::round_robin();
        let removed = lb.remove_endpoint(EndpointId::new());
        assert!(removed.is_none());
    }

    #[test]
    fn test_get_endpoint() {
        let lb = LoadBalancer::round_robin();
        let endpoint = make_endpoint("10.0.0.1", 8080);
        let id = endpoint.id;

        lb.add_endpoint(endpoint);

        let retrieved = lb.get_endpoint(id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).id, id);
    }

    #[test]
    fn test_get_nonexistent_endpoint() {
        let lb = LoadBalancer::round_robin();
        let retrieved = lb.get_endpoint(EndpointId::new());
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_endpoints() {
        let lb = LoadBalancer::round_robin();
        lb.add_endpoint(make_endpoint("10.0.0.1", 8080));
        lb.add_endpoint(make_endpoint("10.0.0.2", 8080));
        lb.add_endpoint(make_endpoint("10.0.0.3", 8080));

        let endpoints = lb.endpoints();
        assert_eq!(endpoints.len(), 3);
    }

    #[test]
    fn test_healthy_endpoints() {
        let lb = LoadBalancer::round_robin();
        lb.add_endpoint(make_endpoint("10.0.0.1", 8080));
        lb.add_endpoint(make_endpoint("10.0.0.2", 8080));
        lb.add_endpoint(make_unhealthy_endpoint("10.0.0.3", 8080));

        let healthy = lb.healthy_endpoints();
        assert_eq!(healthy.len(), 2);
    }

    #[test]
    fn test_healthy_endpoint_count() {
        let lb = LoadBalancer::round_robin();
        lb.add_endpoint(make_endpoint("10.0.0.1", 8080));
        lb.add_endpoint(make_unhealthy_endpoint("10.0.0.2", 8080));

        assert_eq!(lb.endpoint_count(), 2);
        assert_eq!(lb.healthy_endpoint_count(), 1);
    }

    #[test]
    fn test_update_health() {
        let lb = LoadBalancer::round_robin();
        let endpoint = make_endpoint("10.0.0.1", 8080);
        let id = endpoint.id;

        lb.add_endpoint(endpoint);
        assert_eq!(lb.healthy_endpoint_count(), 1);

        lb.update_health(id, HealthStatus::Unhealthy);
        assert_eq!(lb.healthy_endpoint_count(), 0);
    }

    // ==================== Selection Error Tests ====================

    #[test]
    fn test_select_no_endpoints() {
        let lb = LoadBalancer::round_robin();
        let result = lb.select(None);
        assert!(matches!(result, Err(LoadBalancerError::NoEndpoints)));
    }

    #[test]
    fn test_select_no_healthy_endpoints() {
        let lb = LoadBalancer::round_robin();
        lb.add_endpoint(make_unhealthy_endpoint("10.0.0.1", 8080));
        lb.add_endpoint(make_unhealthy_endpoint("10.0.0.2", 8080));

        let result = lb.select(None);
        assert!(matches!(result, Err(LoadBalancerError::NoHealthyEndpoints)));
    }

    // ==================== Round Robin Tests ====================

    #[test]
    fn test_round_robin_selection() {
        let lb = LoadBalancer::round_robin();

        let e1 = make_endpoint("10.0.0.1", 8080);
        let e2 = make_endpoint("10.0.0.2", 8080);
        let e3 = make_endpoint("10.0.0.3", 8080);

        lb.add_endpoint(e1);
        lb.add_endpoint(e2);
        lb.add_endpoint(e3);

        // Collect IDs from multiple selections
        let mut selected_ids: Vec<EndpointId> = Vec::new();
        for _ in 0..9 {
            let selected = lb.select(None).ok();
            selected_ids.push(selected.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).id);
        }

        // Each endpoint should be selected multiple times (cycling through)
        let unique: HashSet<_> = selected_ids.iter().collect();
        assert_eq!(unique.len(), 3);
    }

    #[test]
    fn test_round_robin_single_endpoint() {
        let lb = LoadBalancer::round_robin();
        let endpoint = make_endpoint("10.0.0.1", 8080);
        let id = endpoint.id;

        lb.add_endpoint(endpoint);

        for _ in 0..5 {
            let selected = lb.select(None).ok();
            assert_eq!(selected.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).id, id);
        }
    }

    // ==================== Least Connections Tests ====================

    #[test]
    fn test_least_connections_selection() {
        let lb = LoadBalancer::least_connections();

        let mut e1 = make_endpoint("10.0.0.1", 8080);
        e1.active_connections = 5;

        let mut e2 = make_endpoint("10.0.0.2", 8080);
        e2.active_connections = 2;

        let mut e3 = make_endpoint("10.0.0.3", 8080);
        e3.active_connections = 10;

        let e2_id = e2.id;

        lb.add_endpoint(e1);
        lb.add_endpoint(e2);
        lb.add_endpoint(e3);

        // Should always select endpoint with least connections
        let selected = lb.select(None).ok();
        assert_eq!(selected.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).id, e2_id);
    }

    #[test]
    fn test_least_connections_ties() {
        let lb = LoadBalancer::least_connections();

        let mut e1 = make_endpoint("10.0.0.1", 8080);
        e1.active_connections = 0;

        let mut e2 = make_endpoint("10.0.0.2", 8080);
        e2.active_connections = 0;

        lb.add_endpoint(e1);
        lb.add_endpoint(e2);

        // With ties, selection is deterministic (first found)
        let result = lb.select(None);
        assert!(result.is_ok());
    }

    // ==================== Random Tests ====================

    #[test]
    fn test_random_selection() {
        let lb = LoadBalancer::random();

        lb.add_endpoint(make_endpoint("10.0.0.1", 8080));
        lb.add_endpoint(make_endpoint("10.0.0.2", 8080));
        lb.add_endpoint(make_endpoint("10.0.0.3", 8080));

        // Make many selections and verify distribution
        let mut selections = HashMap::new();
        for _ in 0..1000 {
            let selected = lb.select(None).ok();
            let id = selected.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).id;
            *selections.entry(id).or_insert(0) += 1;
        }

        // All endpoints should be selected at least once
        assert_eq!(selections.len(), 3);

        // Each should be selected roughly equally (allow 10% variance)
        for &count in selections.values() {
            assert!(count > 200, "count {count} too low");
            assert!(count < 500, "count {count} too high");
        }
    }

    // ==================== Weighted Random Tests ====================

    #[test]
    fn test_weighted_random_selection() {
        let lb = LoadBalancer::weighted_random();

        // 80% weight, 10% weight, 10% weight
        lb.add_endpoint(make_weighted_endpoint("10.0.0.1", 8080, 80));
        lb.add_endpoint(make_weighted_endpoint("10.0.0.2", 8080, 10));
        lb.add_endpoint(make_weighted_endpoint("10.0.0.3", 8080, 10));

        let endpoints = lb.endpoints();
        let heavy_id = endpoints
            .iter()
            .find(|e| e.weight == 80)
            .map(|e| e.id)
            .unwrap_or_else(EndpointId::new);

        // Make many selections
        let mut heavy_count = 0;
        for _ in 0..1000 {
            let selected = lb.select(None).ok();
            if selected.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).id == heavy_id {
                heavy_count += 1;
            }
        }

        // Heavy endpoint should be selected roughly 80% of time
        // Allow some variance
        assert!(heavy_count > 700, "heavy count {heavy_count} too low");
        assert!(heavy_count < 900, "heavy count {heavy_count} too high");
    }

    #[test]
    fn test_weighted_random_zero_weight() {
        let lb = LoadBalancer::weighted_random();

        lb.add_endpoint(make_weighted_endpoint("10.0.0.1", 8080, 0));
        lb.add_endpoint(make_weighted_endpoint("10.0.0.2", 8080, 0));

        let result = lb.select(None);
        assert!(matches!(result, Err(LoadBalancerError::ZeroTotalWeight)));
    }

    // ==================== IP Hash Tests ====================

    #[test]
    fn test_ip_hash_consistency() {
        let lb = LoadBalancer::ip_hash();

        lb.add_endpoint(make_endpoint("10.0.0.1", 8080));
        lb.add_endpoint(make_endpoint("10.0.0.2", 8080));
        lb.add_endpoint(make_endpoint("10.0.0.3", 8080));

        let client_ip: IpAddr = "192.168.1.100".parse().ok().unwrap();

        // Same client IP should always get same endpoint
        let first = lb.select(Some(client_ip)).ok();
        let first_id = first.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).id;

        for _ in 0..10 {
            let selected = lb.select(Some(client_ip)).ok();
            assert_eq!(selected.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).id, first_id);
        }
    }

    #[test]
    fn test_ip_hash_different_clients() {
        let lb = LoadBalancer::ip_hash();

        lb.add_endpoint(make_endpoint("10.0.0.1", 8080));
        lb.add_endpoint(make_endpoint("10.0.0.2", 8080));
        lb.add_endpoint(make_endpoint("10.0.0.3", 8080));

        // Different clients may get different endpoints
        let mut selected_ids = HashSet::new();
        for i in 1..100 {
            let client_ip: IpAddr = format!("192.168.1.{i}").parse().ok().unwrap();
            let selected = lb.select(Some(client_ip)).ok();
            selected_ids.insert(selected.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).id);
        }

        // With 100 different IPs, we should hit multiple endpoints
        assert!(selected_ids.len() > 1);
    }

    // ==================== Session Affinity Tests ====================

    #[test]
    fn test_session_affinity() {
        let lb = LoadBalancer::round_robin();

        lb.add_endpoint(make_endpoint("10.0.0.1", 8080));
        lb.add_endpoint(make_endpoint("10.0.0.2", 8080));
        lb.add_endpoint(make_endpoint("10.0.0.3", 8080));

        let client_ip: IpAddr = "192.168.1.100".parse().ok().unwrap();

        // First selection establishes session
        let first = lb.select_with_affinity(client_ip).ok();
        let first_id = first.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).id;

        // Subsequent selections should return same endpoint
        for _ in 0..10 {
            let selected = lb.select_with_affinity(client_ip).ok();
            assert_eq!(selected.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).id, first_id);
        }
    }

    #[test]
    fn test_session_affinity_unhealthy_endpoint() {
        let lb = LoadBalancer::round_robin();

        let e1 = make_endpoint("10.0.0.1", 8080);
        let e2 = make_endpoint("10.0.0.2", 8080);
        let e1_id = e1.id;

        lb.add_endpoint(e1);
        lb.add_endpoint(e2);

        let client_ip: IpAddr = "192.168.1.100".parse().ok().unwrap();

        // Establish session
        let first = lb.select_with_affinity(client_ip).ok();
        let first_id = first.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).id;

        // Mark the endpoint as unhealthy
        lb.update_health(first_id, HealthStatus::Unhealthy);

        // Should select a different healthy endpoint
        let selected = lb.select_with_affinity(client_ip).ok();
        let selected_id = selected.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).id;

        // If e1 was first selected and is now unhealthy, we should get e2 (or vice versa)
        if first_id == e1_id {
            assert_ne!(selected_id, e1_id);
        }
    }

    #[test]
    fn test_clear_session() {
        let lb = LoadBalancer::round_robin();

        lb.add_endpoint(make_endpoint("10.0.0.1", 8080));
        lb.add_endpoint(make_endpoint("10.0.0.2", 8080));

        let client_ip: IpAddr = "192.168.1.100".parse().ok().unwrap();

        // Establish session
        let _first = lb.select_with_affinity(client_ip).ok();

        // Clear the specific session
        lb.clear_session(client_ip);

        // Next selection may be different (depending on round-robin counter)
        let result = lb.select_with_affinity(client_ip);
        assert!(result.is_ok());
    }

    #[test]
    fn test_clear_all_sessions() {
        let lb = LoadBalancer::round_robin();

        lb.add_endpoint(make_endpoint("10.0.0.1", 8080));

        let client1: IpAddr = "192.168.1.1".parse().ok().unwrap();
        let client2: IpAddr = "192.168.1.2".parse().ok().unwrap();

        // Establish sessions
        let _s1 = lb.select_with_affinity(client1).ok();
        let _s2 = lb.select_with_affinity(client2).ok();

        // Clear all sessions
        lb.clear_sessions();

        // Both clients should start fresh
        let result1 = lb.select_with_affinity(client1);
        let result2 = lb.select_with_affinity(client2);
        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }

    #[test]
    fn test_remove_endpoint_clears_session() {
        let lb = LoadBalancer::round_robin();

        let e1 = make_endpoint("10.0.0.1", 8080);
        let e2 = make_endpoint("10.0.0.2", 8080);
        let e1_id = e1.id;
        let e2_id = e2.id;

        lb.add_endpoint(e1);
        lb.add_endpoint(e2);

        let client_ip: IpAddr = "192.168.1.100".parse().ok().unwrap();

        // Establish session with e1
        // Force selection of e1 by using IP hash
        let lb2 = LoadBalancer::ip_hash();
        lb2.add_endpoint(make_endpoint("10.0.0.1", 8080));
        lb2.add_endpoint(make_endpoint("10.0.0.2", 8080));

        // Just use the regular LB
        let _first = lb.select_with_affinity(client_ip).ok();

        // Remove e1
        lb.remove_endpoint(e1_id);

        // Session should be cleared, and e2 should be selected
        let selected = lb.select_with_affinity(client_ip).ok();
        let selected_id = selected.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).id;
        assert_eq!(selected_id, e2_id);
    }

    // ==================== Connection Recording Tests ====================

    #[test]
    fn test_record_connection() {
        let lb = LoadBalancer::round_robin();
        let endpoint = make_endpoint("10.0.0.1", 8080);
        let id = endpoint.id;

        lb.add_endpoint(endpoint);

        lb.record_connection(id);
        lb.record_connection(id);

        let endpoint = lb.get_endpoint(id);
        assert_eq!(endpoint.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).active_connections, 2);
    }

    #[test]
    fn test_record_disconnection() {
        let lb = LoadBalancer::round_robin();
        let endpoint = make_endpoint("10.0.0.1", 8080);
        let id = endpoint.id;

        lb.add_endpoint(endpoint);

        lb.record_connection(id);
        lb.record_connection(id);
        lb.record_disconnection(id);

        let endpoint = lb.get_endpoint(id);
        assert_eq!(endpoint.unwrap_or_else(|| make_endpoint("0.0.0.0", 0)).active_connections, 1);
    }

    // ==================== Error Display Tests ====================

    #[test]
    fn test_error_display() {
        let err = LoadBalancerError::NoEndpoints;
        assert_eq!(err.to_string(), "no endpoints registered");

        let err = LoadBalancerError::NoHealthyEndpoints;
        assert_eq!(err.to_string(), "no healthy endpoints available");

        let id = EndpointId::new();
        let err = LoadBalancerError::EndpointNotFound(id);
        assert!(err.to_string().contains("not found"));

        let err = LoadBalancerError::ZeroTotalWeight;
        assert!(err.to_string().contains("zero"));
    }
}
