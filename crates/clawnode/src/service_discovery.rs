//! Service discovery with ClusterIP allocation and iptables DNAT routing.
//!
//! Services get virtual IPs (VIPs) from the 10.201.0.0/16 CIDR range.
//! Traffic to a VIP is DNAT'd to the actual container backend IPs using
//! iptables rules in the `CLAW-SERVICES` chain.

use std::collections::HashMap;
use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::commands::CommandError;

/// CIDR range for ClusterIP allocation.
const SERVICE_CIDR_PREFIX: [u8; 2] = [10, 201];

/// iptables chain name for service DNAT rules.
const IPTABLES_CHAIN: &str = "CLAW-SERVICES";

/// A backend endpoint for a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub ip: Ipv4Addr,
    pub port: u16,
    pub container_id: String,
    pub healthy: bool,
}

/// A registered service with its ClusterIP and backends.
#[derive(Debug, Clone)]
struct ServiceRecord {
    name: String,
    cluster_ip: Ipv4Addr,
    port: u16,
    protocol: String,
    selector: HashMap<String, String>,
    endpoints: Vec<Endpoint>,
}

/// Manages ClusterIP allocation and iptables-based service routing.
pub struct ServiceDiscovery {
    /// service_name -> record
    services: HashMap<String, ServiceRecord>,
    /// VIP -> service_name (for reverse lookup)
    vip_to_service: HashMap<Ipv4Addr, String>,
    /// Next VIP allocation counter (low 16 bits of the /16 range).
    next_vip: u32,
    /// Whether iptables is available.
    iptables_available: bool,
}

impl ServiceDiscovery {
    /// Create a new ServiceDiscovery instance.
    ///
    /// Tries to set up the iptables chain. If iptables is unavailable,
    /// the service discovery still works for VIP allocation and endpoint
    /// tracking, but DNAT rules won't be applied.
    pub fn new() -> Self {
        let iptables_available = setup_iptables_chain();

        Self {
            services: HashMap::new(),
            vip_to_service: HashMap::new(),
            next_vip: 1, // Start at 10.201.0.1
            iptables_available,
        }
    }

    /// Register a new service and allocate a ClusterIP.
    pub fn register_service(
        &mut self,
        name: &str,
        port: u16,
        protocol: &str,
        selector: HashMap<String, String>,
    ) -> Result<Ipv4Addr, CommandError> {
        if self.services.contains_key(name) {
            return Err(format!("service '{}' already exists", name).into());
        }

        let vip = self.allocate_vip()?;

        info!(
            service = %name,
            cluster_ip = %vip,
            port = port,
            "registered service"
        );

        let record = ServiceRecord {
            name: name.to_string(),
            cluster_ip: vip,
            port,
            protocol: protocol.to_string(),
            selector,
            endpoints: Vec::new(),
        };

        self.vip_to_service.insert(vip, name.to_string());
        self.services.insert(name.to_string(), record);

        Ok(vip)
    }

    /// Update endpoints for a service and rewrite DNAT rules.
    pub fn update_endpoints(
        &mut self,
        name: &str,
        endpoints: Vec<Endpoint>,
    ) -> Result<(), CommandError> {
        let record = self
            .services
            .get_mut(name)
            .ok_or_else(|| format!("service '{}' not found", name))?;

        let healthy: Vec<&Endpoint> = endpoints.iter().filter(|e| e.healthy).collect();

        info!(
            service = %name,
            total = endpoints.len(),
            healthy = healthy.len(),
            "updating service endpoints"
        );

        record.endpoints = endpoints;

        // Rewrite iptables DNAT rules
        if self.iptables_available {
            let backends: Vec<(Ipv4Addr, u16)> = record
                .endpoints
                .iter()
                .filter(|e| e.healthy)
                .map(|e| (e.ip, e.port))
                .collect();

            apply_dnat_rules(
                record.cluster_ip,
                record.port,
                &record.protocol,
                &backends,
            );
        }

        Ok(())
    }

    /// Remove a service, its VIP, and its iptables rules.
    pub fn remove_service(&mut self, name: &str) -> Result<Ipv4Addr, CommandError> {
        let record = self
            .services
            .remove(name)
            .ok_or_else(|| format!("service '{}' not found", name))?;

        self.vip_to_service.remove(&record.cluster_ip);

        // Remove iptables rules
        if self.iptables_available {
            remove_dnat_rules(record.cluster_ip);
        }

        info!(
            service = %name,
            cluster_ip = %record.cluster_ip,
            "removed service"
        );

        Ok(record.cluster_ip)
    }

    /// Resolve a service name to its ClusterIP and endpoints.
    pub fn resolve(&self, name: &str) -> Option<(Ipv4Addr, &[Endpoint])> {
        self.services
            .get(name)
            .map(|r| (r.cluster_ip, r.endpoints.as_slice()))
    }

    /// Get a service's ClusterIP.
    pub fn get_cluster_ip(&self, name: &str) -> Option<Ipv4Addr> {
        self.services.get(name).map(|r| r.cluster_ip)
    }

    /// Get a service's endpoints.
    pub fn get_endpoints(&self, name: &str) -> Option<&[Endpoint]> {
        self.services.get(name).map(|r| r.endpoints.as_slice())
    }

    /// Get a service's selector.
    pub fn get_selector(&self, name: &str) -> Option<&HashMap<String, String>> {
        self.services.get(name).map(|r| &r.selector)
    }

    /// List all registered services.
    pub fn list_services(&self) -> Vec<ServiceInfo> {
        self.services
            .values()
            .map(|r| ServiceInfo {
                name: r.name.clone(),
                cluster_ip: r.cluster_ip,
                port: r.port,
                protocol: r.protocol.clone(),
                endpoint_count: r.endpoints.len(),
                healthy_count: r.endpoints.iter().filter(|e| e.healthy).count(),
            })
            .collect()
    }

    /// Refresh endpoints for all services by matching selectors against running workloads.
    ///
    /// `workloads` is a list of (container_id, ip, port, labels) for currently running containers.
    pub fn refresh_all_endpoints(
        &mut self,
        workloads: &[(String, Ipv4Addr, u16, HashMap<String, String>)],
    ) {
        let service_names: Vec<String> = self.services.keys().cloned().collect();

        for name in service_names {
            let (selector, port) = {
                let record = self.services.get(&name).unwrap();
                (record.selector.clone(), record.port)
            };

            let endpoints: Vec<Endpoint> = workloads
                .iter()
                .filter(|(_, _, _, labels)| matches_selector(labels, &selector))
                .map(|(container_id, ip, _, _)| Endpoint {
                    ip: *ip,
                    port,
                    container_id: container_id.clone(),
                    healthy: true,
                })
                .collect();

            // Update endpoints (ignore errors for individual services)
            if let Err(e) = self.update_endpoints(&name, endpoints) {
                warn!(service = %name, error = %e, "failed to refresh endpoints");
            }
        }
    }

    /// Number of registered services.
    pub fn service_count(&self) -> usize {
        self.services.len()
    }

    /// Whether iptables is available for DNAT.
    pub fn iptables_available(&self) -> bool {
        self.iptables_available
    }

    /// Allocate the next available VIP from 10.201.0.0/16.
    fn allocate_vip(&mut self) -> Result<Ipv4Addr, CommandError> {
        if self.next_vip >= 65534 {
            return Err("ClusterIP range exhausted".into());
        }

        let high = (self.next_vip >> 8) as u8;
        let low = (self.next_vip & 0xFF) as u8;
        let vip = Ipv4Addr::new(
            SERVICE_CIDR_PREFIX[0],
            SERVICE_CIDR_PREFIX[1],
            high,
            low,
        );

        self.next_vip += 1;

        Ok(vip)
    }
}

/// Summary info for a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub cluster_ip: Ipv4Addr,
    pub port: u16,
    pub protocol: String,
    pub endpoint_count: usize,
    pub healthy_count: usize,
}

/// Check if a set of labels matches a selector.
///
/// All selector key/value pairs must be present in the labels.
fn matches_selector(
    labels: &HashMap<String, String>,
    selector: &HashMap<String, String>,
) -> bool {
    if selector.is_empty() {
        return false; // Empty selector matches nothing (not everything)
    }

    selector
        .iter()
        .all(|(k, v)| labels.get(k).is_some_and(|lv| lv == v))
}

impl std::fmt::Debug for ServiceDiscovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceDiscovery")
            .field("service_count", &self.services.len())
            .field("iptables_available", &self.iptables_available)
            .field("next_vip", &self.next_vip)
            .finish()
    }
}

// ─────────────────────────────────────────────────────────────
// iptables helpers
// ─────────────────────────────────────────────────────────────

/// Set up the CLAW-SERVICES chain in the nat table.
fn setup_iptables_chain() -> bool {
    // Create the chain (ignore error if it already exists)
    let create = std::process::Command::new("iptables")
        .args(["-t", "nat", "-N", IPTABLES_CHAIN])
        .output();

    match create {
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if output.status.success() || stderr.contains("Chain already exists") {
                // Insert jump rule into PREROUTING and OUTPUT if not present
                let _ = std::process::Command::new("iptables")
                    .args([
                        "-t", "nat", "-C", "PREROUTING", "-j", IPTABLES_CHAIN,
                    ])
                    .output()
                    .and_then(|o| {
                        if !o.status.success() {
                            std::process::Command::new("iptables")
                                .args([
                                    "-t", "nat", "-I", "PREROUTING", "-j", IPTABLES_CHAIN,
                                ])
                                .output()
                        } else {
                            Ok(o)
                        }
                    });

                let _ = std::process::Command::new("iptables")
                    .args([
                        "-t", "nat", "-C", "OUTPUT", "-j", IPTABLES_CHAIN,
                    ])
                    .output()
                    .and_then(|o| {
                        if !o.status.success() {
                            std::process::Command::new("iptables")
                                .args([
                                    "-t", "nat", "-I", "OUTPUT", "-j", IPTABLES_CHAIN,
                                ])
                                .output()
                        } else {
                            Ok(o)
                        }
                    });

                info!("iptables CLAW-SERVICES chain ready");
                true
            } else {
                warn!(error = %stderr, "failed to create iptables chain");
                false
            }
        }
        Err(e) => {
            warn!(error = %e, "iptables not available");
            false
        }
    }
}

/// Apply DNAT rules for a service VIP to its backends (round-robin).
fn apply_dnat_rules(
    vip: Ipv4Addr,
    port: u16,
    protocol: &str,
    backends: &[(Ipv4Addr, u16)],
) {
    // First, remove existing rules for this VIP
    remove_dnat_rules(vip);

    if backends.is_empty() {
        return;
    }

    let n = backends.len();

    for (i, (backend_ip, backend_port)) in backends.iter().enumerate() {
        let mut args = vec![
            "-t".to_string(),
            "nat".to_string(),
            "-A".to_string(),
            IPTABLES_CHAIN.to_string(),
            "-d".to_string(),
            format!("{vip}/32"),
            "-p".to_string(),
            protocol.to_string(),
            "--dport".to_string(),
            port.to_string(),
        ];

        // Use statistic module for round-robin when multiple backends
        if n > 1 {
            args.extend([
                "-m".to_string(),
                "statistic".to_string(),
                "--mode".to_string(),
                "nth".to_string(),
                "--every".to_string(),
                (n - i).to_string(),
                "--packet".to_string(),
                "0".to_string(),
            ]);
        }

        args.extend([
            "-j".to_string(),
            "DNAT".to_string(),
            "--to-destination".to_string(),
            format!("{backend_ip}:{backend_port}"),
        ]);

        let result = std::process::Command::new("iptables")
            .args(&args)
            .output();

        match result {
            Ok(output) if output.status.success() => {
                info!(vip = %vip, backend = %backend_ip, "added DNAT rule");
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!(vip = %vip, error = %stderr, "failed to add DNAT rule");
            }
            Err(e) => {
                warn!(vip = %vip, error = %e, "failed to execute iptables");
            }
        }
    }
}

/// Remove all DNAT rules for a specific VIP.
fn remove_dnat_rules(vip: Ipv4Addr) {
    // Flush all rules matching this VIP from the chain.
    // We loop until no more rules match (iptables -D removes one at a time).
    loop {
        let result = std::process::Command::new("iptables")
            .args([
                "-t",
                "nat",
                "-D",
                IPTABLES_CHAIN,
                "-d",
                &format!("{vip}/32"),
            ])
            .output();

        match result {
            Ok(output) if output.status.success() => continue,
            _ => break,
        }
    }
}

/// Generate the iptables rule arguments as strings (for testing).
pub fn generate_dnat_rule_args(
    vip: Ipv4Addr,
    port: u16,
    protocol: &str,
    backends: &[(Ipv4Addr, u16)],
) -> Vec<Vec<String>> {
    let n = backends.len();
    let mut rules = Vec::new();

    for (i, (backend_ip, backend_port)) in backends.iter().enumerate() {
        let mut args = vec![
            "-t".to_string(),
            "nat".to_string(),
            "-A".to_string(),
            IPTABLES_CHAIN.to_string(),
            "-d".to_string(),
            format!("{vip}/32"),
            "-p".to_string(),
            protocol.to_string(),
            "--dport".to_string(),
            port.to_string(),
        ];

        if n > 1 {
            args.extend([
                "-m".to_string(),
                "statistic".to_string(),
                "--mode".to_string(),
                "nth".to_string(),
                "--every".to_string(),
                (n - i).to_string(),
                "--packet".to_string(),
                "0".to_string(),
            ]);
        }

        args.extend([
            "-j".to_string(),
            "DNAT".to_string(),
            "--to-destination".to_string(),
            format!("{backend_ip}:{backend_port}"),
        ]);

        rules.push(args);
    }

    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_sd() -> ServiceDiscovery {
        // iptables won't be available in tests, but allocation still works
        ServiceDiscovery {
            services: HashMap::new(),
            vip_to_service: HashMap::new(),
            next_vip: 1,
            iptables_available: false,
        }
    }

    #[test]
    fn register_service_allocates_vip() {
        let mut sd = test_sd();

        let vip = sd
            .register_service("api", 8080, "tcp", HashMap::from([("app".into(), "api".into())]))
            .expect("register");

        assert_eq!(vip, Ipv4Addr::new(10, 201, 0, 1));
        assert_eq!(sd.service_count(), 1);
    }

    #[test]
    fn register_service_sequential_vips() {
        let mut sd = test_sd();

        let vip1 = sd.register_service("svc-1", 80, "tcp", HashMap::new()).expect("r1");
        let vip2 = sd.register_service("svc-2", 80, "tcp", HashMap::new()).expect("r2");
        let vip3 = sd.register_service("svc-3", 80, "tcp", HashMap::new()).expect("r3");

        assert_eq!(vip1, Ipv4Addr::new(10, 201, 0, 1));
        assert_eq!(vip2, Ipv4Addr::new(10, 201, 0, 2));
        assert_eq!(vip3, Ipv4Addr::new(10, 201, 0, 3));
    }

    #[test]
    fn register_service_duplicate_fails() {
        let mut sd = test_sd();

        sd.register_service("api", 8080, "tcp", HashMap::new()).expect("register");
        let result = sd.register_service("api", 9090, "tcp", HashMap::new());
        assert!(result.is_err());
    }

    #[test]
    fn remove_service_works() {
        let mut sd = test_sd();

        let vip = sd.register_service("api", 8080, "tcp", HashMap::new()).expect("register");
        let removed_vip = sd.remove_service("api").expect("remove");

        assert_eq!(vip, removed_vip);
        assert_eq!(sd.service_count(), 0);
    }

    #[test]
    fn remove_nonexistent_fails() {
        let mut sd = test_sd();
        assert!(sd.remove_service("nope").is_err());
    }

    #[test]
    fn resolve_service() {
        let mut sd = test_sd();

        sd.register_service("api", 8080, "tcp", HashMap::new()).expect("register");

        let (vip, endpoints) = sd.resolve("api").expect("resolve");
        assert_eq!(vip, Ipv4Addr::new(10, 201, 0, 1));
        assert!(endpoints.is_empty());

        assert!(sd.resolve("nonexistent").is_none());
    }

    #[test]
    fn update_endpoints() {
        let mut sd = test_sd();

        sd.register_service("api", 8080, "tcp", HashMap::new()).expect("register");

        let endpoints = vec![
            Endpoint {
                ip: Ipv4Addr::new(10, 200, 1, 2),
                port: 8080,
                container_id: "c-1".to_string(),
                healthy: true,
            },
            Endpoint {
                ip: Ipv4Addr::new(10, 200, 1, 3),
                port: 8080,
                container_id: "c-2".to_string(),
                healthy: false,
            },
        ];

        sd.update_endpoints("api", endpoints).expect("update");

        let (_, eps) = sd.resolve("api").expect("resolve");
        assert_eq!(eps.len(), 2);
    }

    #[test]
    fn list_services() {
        let mut sd = test_sd();

        sd.register_service("api", 8080, "tcp", HashMap::new()).expect("r1");
        sd.register_service("web", 80, "tcp", HashMap::new()).expect("r2");

        let list = sd.list_services();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn matches_selector_works() {
        let labels = HashMap::from([
            ("app".to_string(), "api".to_string()),
            ("tier".to_string(), "backend".to_string()),
        ]);

        // Matches
        let selector = HashMap::from([("app".to_string(), "api".to_string())]);
        assert!(matches_selector(&labels, &selector));

        // Matches multiple
        let selector = HashMap::from([
            ("app".to_string(), "api".to_string()),
            ("tier".to_string(), "backend".to_string()),
        ]);
        assert!(matches_selector(&labels, &selector));

        // No match - wrong value
        let selector = HashMap::from([("app".to_string(), "web".to_string())]);
        assert!(!matches_selector(&labels, &selector));

        // No match - missing key
        let selector = HashMap::from([("env".to_string(), "prod".to_string())]);
        assert!(!matches_selector(&labels, &selector));

        // Empty selector matches nothing
        assert!(!matches_selector(&labels, &HashMap::new()));
    }

    #[test]
    fn refresh_all_endpoints_matches_selectors() {
        let mut sd = test_sd();

        sd.register_service(
            "api",
            8080,
            "tcp",
            HashMap::from([("app".into(), "api".into())]),
        )
        .expect("register");

        let workloads = vec![
            (
                "c-1".to_string(),
                Ipv4Addr::new(10, 200, 1, 2),
                8080u16,
                HashMap::from([("app".to_string(), "api".to_string())]),
            ),
            (
                "c-2".to_string(),
                Ipv4Addr::new(10, 200, 1, 3),
                8080u16,
                HashMap::from([("app".to_string(), "web".to_string())]),
            ),
            (
                "c-3".to_string(),
                Ipv4Addr::new(10, 200, 1, 4),
                8080u16,
                HashMap::from([("app".to_string(), "api".to_string())]),
            ),
        ];

        sd.refresh_all_endpoints(&workloads);

        let (_, eps) = sd.resolve("api").expect("resolve");
        assert_eq!(eps.len(), 2);
        assert_eq!(eps[0].container_id, "c-1");
        assert_eq!(eps[1].container_id, "c-3");
    }

    #[test]
    fn generate_dnat_rules_single_backend() {
        let rules = generate_dnat_rule_args(
            Ipv4Addr::new(10, 201, 0, 1),
            8080,
            "tcp",
            &[(Ipv4Addr::new(10, 200, 1, 2), 8080)],
        );

        assert_eq!(rules.len(), 1);
        assert!(rules[0].contains(&"DNAT".to_string()));
        assert!(rules[0].contains(&"10.200.1.2:8080".to_string()));
        // No statistic module for single backend
        assert!(!rules[0].contains(&"statistic".to_string()));
    }

    #[test]
    fn generate_dnat_rules_multiple_backends() {
        let rules = generate_dnat_rule_args(
            Ipv4Addr::new(10, 201, 0, 1),
            8080,
            "tcp",
            &[
                (Ipv4Addr::new(10, 200, 1, 2), 8080),
                (Ipv4Addr::new(10, 200, 1, 3), 8080),
                (Ipv4Addr::new(10, 200, 1, 4), 8080),
            ],
        );

        assert_eq!(rules.len(), 3);
        // All rules should use statistic module
        for rule in &rules {
            assert!(rule.contains(&"statistic".to_string()));
            assert!(rule.contains(&"nth".to_string()));
        }
        // First rule: --every 3 (3 backends, index 0)
        assert!(rules[0].contains(&"3".to_string()));
        // Second rule: --every 2
        assert!(rules[1].contains(&"2".to_string()));
        // Third rule: --every 1
        assert!(rules[2].contains(&"1".to_string()));
    }

    #[test]
    fn vip_allocation_wraps_to_next_octet() {
        let mut sd = test_sd();
        sd.next_vip = 256; // Should give 10.201.1.0

        let vip = sd.register_service("svc", 80, "tcp", HashMap::new()).expect("register");
        assert_eq!(vip, Ipv4Addr::new(10, 201, 1, 0));
    }
}
