//! Workload networking — mesh-routable IPs for containers.
//!
//! Each node gets a /24 workload subnet (e.g., 10.200.{node}.0/24).
//! Containers launched on this node get IPs from that subnet via
//! a Docker bridge network (`claw-mesh`).

use std::collections::HashMap;
use std::net::Ipv4Addr;

use ipnet::Ipv4Net;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::commands::CommandError;

/// Name of the Docker bridge network for mesh-routed containers.
const DOCKER_NETWORK_NAME: &str = "claw-mesh";

/// Manages workload IP allocation and Docker network integration.
pub struct WorkloadNetManager {
    /// Docker network name.
    network_name: String,
    /// This node's workload subnet (e.g., 10.200.5.0/24).
    workload_subnet: Ipv4Net,
    /// Allocated IPs: workload_id -> ip.
    allocated_ips: HashMap<String, Ipv4Addr>,
    /// Reverse mapping: container_id -> workload_id (for release on stop).
    container_to_workload: HashMap<String, String>,
    /// Next candidate IP suffix (.2, .3, ...).
    next_ip: u8,
    /// Released IPs available for reuse.
    free_pool: Vec<u8>,
    /// WireGuard interface name (for routing).
    wg_interface: String,
    /// Whether the Docker network was successfully created.
    docker_network_created: bool,
}

/// Network info for a container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerNetworkInfo {
    pub network: String,
    pub ip_address: String,
    pub subnet: String,
    pub gateway: String,
}

impl WorkloadNetManager {
    /// Create a new workload network manager.
    ///
    /// Optionally creates a Docker bridge network with the given subnet.
    /// If Docker network creation fails (e.g., no Docker), the manager
    /// still works for IP tracking.
    pub fn init(workload_subnet: Ipv4Net, wg_interface: &str) -> Result<Self, CommandError> {
        let _network = workload_subnet.network();
        let prefix = workload_subnet.prefix_len();

        if prefix != 24 {
            return Err(format!("workload subnet must be /24, got /{prefix}").into());
        }

        info!(
            subnet = %workload_subnet,
            network = DOCKER_NETWORK_NAME,
            "initializing workload networking"
        );

        // Try to create Docker network
        let docker_created = create_docker_network(DOCKER_NETWORK_NAME, &workload_subnet);

        Ok(Self {
            network_name: DOCKER_NETWORK_NAME.to_string(),
            workload_subnet,
            allocated_ips: HashMap::new(),
            container_to_workload: HashMap::new(),
            next_ip: 2, // .1 is the gateway
            free_pool: Vec::new(),
            wg_interface: wg_interface.to_string(),
            docker_network_created: docker_created,
        })
    }

    /// Allocate an IP for a container from the workload subnet.
    pub fn allocate_ip(&mut self, container_id: &str) -> Result<Ipv4Addr, CommandError> {
        // Check if already allocated
        if let Some(ip) = self.allocated_ips.get(container_id) {
            return Ok(*ip);
        }

        // Try to reuse a freed IP first
        let suffix = if let Some(s) = self.free_pool.pop() {
            s
        } else {
            let s = self.next_ip;
            if s > 254 {
                return Err("workload subnet exhausted (max 253 containers)".into());
            }
            self.next_ip = s + 1;
            s
        };

        let octets = self.workload_subnet.network().octets();
        let ip = Ipv4Addr::new(octets[0], octets[1], octets[2], suffix);

        self.allocated_ips.insert(container_id.to_string(), ip);

        info!(
            container_id = %container_id,
            ip = %ip,
            "allocated workload IP"
        );

        Ok(ip)
    }

    /// Release an IP when a container stops.
    pub fn release_ip(&mut self, container_id: &str) -> Option<Ipv4Addr> {
        if let Some(ip) = self.allocated_ips.remove(container_id) {
            let suffix = ip.octets()[3];
            self.free_pool.push(suffix);
            info!(
                container_id = %container_id,
                ip = %ip,
                "released workload IP"
            );
            Some(ip)
        } else {
            None
        }
    }

    /// Get the IP assigned to a container.
    pub fn get_ip(&self, container_id: &str) -> Option<Ipv4Addr> {
        self.allocated_ips.get(container_id).copied()
    }

    /// Get the network info for container creation.
    pub fn network_info(&self, container_id: &str) -> Option<ContainerNetworkInfo> {
        self.allocated_ips.get(container_id).map(|ip| {
            let octets = self.workload_subnet.network().octets();
            let gateway = Ipv4Addr::new(octets[0], octets[1], octets[2], 1);

            ContainerNetworkInfo {
                network: self.network_name.clone(),
                ip_address: ip.to_string(),
                subnet: self.workload_subnet.to_string(),
                gateway: gateway.to_string(),
            }
        })
    }

    /// Track a container_id → workload_id mapping so we can release IPs at stop time.
    pub fn track_container(&mut self, container_id: &str, workload_id: &str) {
        self.container_to_workload
            .insert(container_id.to_string(), workload_id.to_string());
    }

    /// Release IP by container_id (looks up the workload_id mapping).
    pub fn release_by_container(&mut self, container_id: &str) -> Option<Ipv4Addr> {
        if let Some(workload_id) = self.container_to_workload.remove(container_id) {
            self.release_ip(&workload_id)
        } else {
            // Try direct release in case container_id was used as the key
            self.release_ip(container_id)
        }
    }

    /// Get the Docker network name.
    pub fn network_name(&self) -> &str {
        &self.network_name
    }

    /// Get the workload subnet.
    pub fn workload_subnet(&self) -> Ipv4Net {
        self.workload_subnet
    }

    /// Whether the Docker network was created.
    pub fn docker_network_created(&self) -> bool {
        self.docker_network_created
    }

    /// Number of allocated IPs.
    pub fn allocated_count(&self) -> usize {
        self.allocated_ips.len()
    }

    /// All currently allocated IPs.
    pub fn allocations(&self) -> &HashMap<String, Ipv4Addr> {
        &self.allocated_ips
    }

    /// Set up routes for remote workload subnets via the mesh.
    ///
    /// For each remote node, adds: `ip route add 10.200.{other}.0/24 via {mesh_ip} dev claw0`
    pub fn setup_routing(&self, remote_subnets: &[(Ipv4Net, std::net::IpAddr)]) {
        for (subnet, via_ip) in remote_subnets {
            let result = std::process::Command::new("ip")
                .args([
                    "route",
                    "add",
                    &subnet.to_string(),
                    "via",
                    &via_ip.to_string(),
                    "dev",
                    &self.wg_interface,
                ])
                .output();

            match result {
                Ok(output) if output.status.success() => {
                    info!(subnet = %subnet, via = %via_ip, "added workload route");
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    // "RTNETLINK answers: File exists" means route already set
                    if !stderr.contains("File exists") {
                        warn!(subnet = %subnet, error = %stderr, "failed to add workload route");
                    }
                }
                Err(e) => {
                    warn!(subnet = %subnet, error = %e, "failed to execute ip route add");
                }
            }
        }
    }
}

impl std::fmt::Debug for WorkloadNetManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkloadNetManager")
            .field("network_name", &self.network_name)
            .field("workload_subnet", &self.workload_subnet)
            .field("allocated_count", &self.allocated_ips.len())
            .field("docker_network_created", &self.docker_network_created)
            .finish()
    }
}

/// Try to create a Docker bridge network. Returns true on success.
fn create_docker_network(name: &str, subnet: &Ipv4Net) -> bool {
    let octets = subnet.network().octets();
    let gateway = format!("{}.{}.{}.1", octets[0], octets[1], octets[2]);

    let result = std::process::Command::new("docker")
        .args([
            "network",
            "create",
            "--driver",
            "bridge",
            "--subnet",
            &subnet.to_string(),
            "--gateway",
            &gateway,
            name,
        ])
        .output();

    match result {
        Ok(output) if output.status.success() => {
            info!(name = %name, subnet = %subnet, "created Docker network");
            true
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("already exists") {
                info!(name = %name, "Docker network already exists");
                true
            } else {
                warn!(name = %name, error = %stderr, "failed to create Docker network");
                false
            }
        }
        Err(e) => {
            warn!(error = %e, "Docker not available for network creation");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn test_subnet() -> Ipv4Net {
        Ipv4Net::from_str("10.200.5.0/24").expect("valid subnet")
    }

    #[test]
    fn workload_net_init() {
        // Note: Docker network creation will fail in CI, but IP allocation still works
        let mgr = WorkloadNetManager::init(test_subnet(), "claw0").expect("init");
        assert_eq!(mgr.network_name(), "claw-mesh");
        assert_eq!(mgr.workload_subnet(), test_subnet());
        assert_eq!(mgr.allocated_count(), 0);
    }

    #[test]
    fn workload_net_allocate_ip() {
        let mut mgr = WorkloadNetManager::init(test_subnet(), "claw0").expect("init");

        let ip1 = mgr.allocate_ip("container-1").expect("alloc");
        assert_eq!(ip1, Ipv4Addr::new(10, 200, 5, 2));

        let ip2 = mgr.allocate_ip("container-2").expect("alloc");
        assert_eq!(ip2, Ipv4Addr::new(10, 200, 5, 3));

        assert_eq!(mgr.allocated_count(), 2);
    }

    #[test]
    fn workload_net_allocate_idempotent() {
        let mut mgr = WorkloadNetManager::init(test_subnet(), "claw0").expect("init");

        let ip1 = mgr.allocate_ip("container-1").expect("alloc");
        let ip2 = mgr.allocate_ip("container-1").expect("alloc again");
        assert_eq!(ip1, ip2);
        assert_eq!(mgr.allocated_count(), 1);
    }

    #[test]
    fn workload_net_release_and_reuse() {
        let mut mgr = WorkloadNetManager::init(test_subnet(), "claw0").expect("init");

        let ip1 = mgr.allocate_ip("container-1").expect("alloc");
        assert_eq!(ip1, Ipv4Addr::new(10, 200, 5, 2));

        let ip2 = mgr.allocate_ip("container-2").expect("alloc");
        assert_eq!(ip2, Ipv4Addr::new(10, 200, 5, 3));

        // Release container-1
        let released = mgr.release_ip("container-1");
        assert_eq!(released, Some(Ipv4Addr::new(10, 200, 5, 2)));

        // Next allocation reuses the freed IP
        let ip3 = mgr.allocate_ip("container-3").expect("alloc");
        assert_eq!(ip3, Ipv4Addr::new(10, 200, 5, 2));
    }

    #[test]
    fn workload_net_release_nonexistent() {
        let mut mgr = WorkloadNetManager::init(test_subnet(), "claw0").expect("init");
        assert_eq!(mgr.release_ip("nonexistent"), None);
    }

    #[test]
    fn workload_net_get_ip() {
        let mut mgr = WorkloadNetManager::init(test_subnet(), "claw0").expect("init");

        assert_eq!(mgr.get_ip("container-1"), None);

        mgr.allocate_ip("container-1").expect("alloc");
        assert_eq!(mgr.get_ip("container-1"), Some(Ipv4Addr::new(10, 200, 5, 2)));
    }

    #[test]
    fn workload_net_network_info() {
        let mut mgr = WorkloadNetManager::init(test_subnet(), "claw0").expect("init");

        mgr.allocate_ip("container-1").expect("alloc");
        let info = mgr.network_info("container-1").expect("has info");
        assert_eq!(info.network, "claw-mesh");
        assert_eq!(info.ip_address, "10.200.5.2");
        assert_eq!(info.gateway, "10.200.5.1");
        assert_eq!(info.subnet, "10.200.5.0/24");
    }

    #[test]
    fn workload_net_invalid_subnet() {
        let subnet = Ipv4Net::from_str("10.200.0.0/16").expect("valid");
        let result = WorkloadNetManager::init(subnet, "claw0");
        assert!(result.is_err());
    }

    #[test]
    fn workload_net_exhaust_subnet() {
        let mut mgr = WorkloadNetManager::init(test_subnet(), "claw0").expect("init");

        // Allocate all 253 IPs (.2 through .254)
        for i in 0..253 {
            mgr.allocate_ip(&format!("c-{i}")).expect("alloc");
        }

        // 254th should fail
        let result = mgr.allocate_ip("c-253");
        assert!(result.is_err());
    }
}
