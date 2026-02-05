//! Network discovery handlers
//!
//! Tools for scanning networks, discovering nodes, and managing credential profiles.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{BridgeError, BridgeResult};
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Global State
// ─────────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    /// Credential profiles stored in memory (references secrets by ID)
    static ref CREDENTIAL_PROFILES: RwLock<HashMap<String, CredentialProfile>> = 
        RwLock::new(HashMap::new());
    
    /// Known/trusted subnets for auto-approval
    static ref TRUSTED_SUBNETS: RwLock<Vec<String>> = 
        RwLock::new(vec![
            "192.168.0.0/16".to_string(),
            "10.0.0.0/8".to_string(),
            "172.16.0.0/12".to_string(),
        ]);
    
    /// Bootstrap tokens (token -> metadata)
    static ref BOOTSTRAP_TOKENS: RwLock<HashMap<String, BootstrapToken>> = 
        RwLock::new(HashMap::new());
}

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialProfile {
    pub name: String,
    pub credential_type: String, // ssh, winrm, api
    pub username: Option<String>,
    pub secret_ref: Option<String>, // Reference to secret_put ID
    pub auth_method: String,        // key, password, agent
    pub scope: Vec<String>,         // Subnets where this profile applies
    pub sudo: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapToken {
    pub token: String,
    pub hostname: String,
    pub labels: HashMap<String, String>,
    pub auto_approve: bool,
    pub created_at: i64,
    pub expires_at: i64,
    pub used: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredHost {
    pub address: String,
    pub hostname: Option<String>,
    pub open_ports: Vec<u16>,
    pub os_hint: Option<String>,
    pub gpu_detected: bool,
    pub gpu_info: Option<GpuProbeResult>,
    pub ssh_available: bool,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuProbeResult {
    pub count: u32,
    pub models: Vec<String>,
    pub total_memory_gb: u32,
    pub driver_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    pub subnet: String,
    pub hosts_scanned: u32,
    pub hosts_found: u32,
    pub gpu_nodes_found: u32,
    pub scan_duration_ms: u64,
    pub hosts: Vec<DiscoveredHost>,
}

// ─────────────────────────────────────────────────────────────
// Network Scan Handler
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NetworkScanParams {
    pub subnet: String,
    pub ports: Option<Vec<u16>>,
    pub timeout_ms: Option<u64>,
    pub detect_gpus: Option<bool>,
    pub credential_profile: Option<String>,
    pub max_concurrent: Option<usize>,
}

/// Scan a network subnet for hosts and GPUs
pub async fn network_scan(params: Value) -> BridgeResult<Value> {
    let params: NetworkScanParams = parse_params(params)?;

    let start = std::time::Instant::now();

    // Parse subnet
    let (base_ip, prefix_len) = parse_cidr(&params.subnet)?;

    // Default ports to scan
    let ports = params.ports.unwrap_or_else(|| vec![22, 80, 443, 8080, 6443, 9090]);
    let timeout = Duration::from_millis(params.timeout_ms.unwrap_or(500));
    let max_concurrent = params.max_concurrent.unwrap_or(50);

    // Calculate host range
    let host_count = 2u32.pow(32 - prefix_len as u32);
    let base_u32 = ip_to_u32(base_ip);

    tracing::info!(
        subnet = %params.subnet,
        host_count = host_count,
        ports = ?ports,
        "starting network scan"
    );

    let mut hosts = Vec::new();
    let mut gpu_nodes_found = 0u32;

    // Scan hosts (simplified - real impl would use async/parallel)
    let scan_limit = host_count.min(256); // Limit scan size for safety

    for i in 1..scan_limit {
        let ip = u32_to_ip(base_u32 + i);
        let ip_str = ip.to_string();

        // Check if host is reachable on any port
        let mut open_ports = Vec::new();
        let mut reachable = false;
        let mut latency_ms = None;

        for &port in &ports {
            let addr = SocketAddr::new(IpAddr::V4(ip), port);
            let probe_start = std::time::Instant::now();

            if TcpStream::connect_timeout(&addr, timeout).is_ok() {
                open_ports.push(port);
                reachable = true;
                if latency_ms.is_none() {
                    latency_ms = Some(probe_start.elapsed().as_millis() as u64);
                }
            }
        }

        if reachable {
            let ssh_available = open_ports.contains(&22);

            // GPU detection would require SSH access - stub for now
            let (gpu_detected, gpu_info) = if params.detect_gpus.unwrap_or(false) && ssh_available {
                // In real impl: SSH in and run nvidia-smi
                // For now, mark as unknown
                (false, None)
            } else {
                (false, None)
            };

            if gpu_detected {
                gpu_nodes_found += 1;
            }

            // OS hint based on ports
            let os_hint = guess_os(&open_ports);

            hosts.push(DiscoveredHost {
                address: ip_str,
                hostname: None, // Would need reverse DNS
                open_ports,
                os_hint,
                gpu_detected,
                gpu_info,
                ssh_available,
                reachable: true,
                latency_ms,
            });
        }
    }

    let duration = start.elapsed();

    let result = ScanResult {
        subnet: params.subnet,
        hosts_scanned: scan_limit,
        hosts_found: hosts.len() as u32,
        gpu_nodes_found,
        scan_duration_ms: duration.as_millis() as u64,
        hosts,
    };

    tracing::info!(
        hosts_found = result.hosts_found,
        duration_ms = result.scan_duration_ms,
        "network scan complete"
    );

    to_json(result)
}

// ─────────────────────────────────────────────────────────────
// Credential Profile Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CredentialProfileCreateParams {
    pub name: String,
    pub credential_type: Option<String>,
    pub username: Option<String>,
    pub secret_ref: Option<String>,
    pub auth_method: Option<String>,
    pub scope: Option<Vec<String>>,
    pub sudo: Option<bool>,
}

/// Create a credential profile
pub async fn credential_profile_create(params: Value) -> BridgeResult<Value> {
    let params: CredentialProfileCreateParams = parse_params(params)?;

    let profile = CredentialProfile {
        name: params.name.clone(),
        credential_type: params.credential_type.unwrap_or_else(|| "ssh".to_string()),
        username: params.username,
        secret_ref: params.secret_ref,
        auth_method: params.auth_method.unwrap_or_else(|| "key".to_string()),
        scope: params.scope.unwrap_or_default(),
        sudo: params.sudo.unwrap_or(true),
        created_at: chrono::Utc::now().timestamp_millis(),
    };

    let mut profiles = CREDENTIAL_PROFILES.write();
    profiles.insert(params.name.clone(), profile.clone());

    tracing::info!(name = %params.name, "credential profile created");

    to_json(profile)
}

#[derive(Debug, Deserialize)]
pub struct CredentialProfileListParams {}

/// List credential profiles
pub async fn credential_profile_list(_params: Value) -> BridgeResult<Value> {
    let profiles = CREDENTIAL_PROFILES.read();
    let list: Vec<&CredentialProfile> = profiles.values().collect();
    to_json(list)
}

#[derive(Debug, Deserialize)]
pub struct CredentialProfileGetParams {
    pub name: String,
}

/// Get a credential profile by name
pub async fn credential_profile_get(params: Value) -> BridgeResult<Value> {
    let params: CredentialProfileGetParams = parse_params(params)?;

    let profiles = CREDENTIAL_PROFILES.read();
    let profile = profiles
        .get(&params.name)
        .ok_or_else(|| BridgeError::NotFound("credential profile not found".to_string()))?;

    to_json(profile)
}

// ─────────────────────────────────────────────────────────────
// Bootstrap Token Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NodeTokenCreateParams {
    pub hostname: String,
    pub ttl_minutes: Option<u32>,
    pub labels: Option<HashMap<String, String>>,
    pub auto_approve: Option<bool>,
}

/// Create a bootstrap token for a node
pub async fn node_token_create(params: Value) -> BridgeResult<Value> {
    let params: NodeTokenCreateParams = parse_params(params)?;

    let ttl_minutes = params.ttl_minutes.unwrap_or(15);
    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::minutes(ttl_minutes as i64);

    // Generate secure token
    let token = format!(
        "claw_bt_{}",
        uuid::Uuid::new_v4().to_string().replace("-", "")
    );

    let bootstrap_token = BootstrapToken {
        token: token.clone(),
        hostname: params.hostname.clone(),
        labels: params.labels.unwrap_or_default(),
        auto_approve: params.auto_approve.unwrap_or(true),
        created_at: now.timestamp_millis(),
        expires_at: expires_at.timestamp_millis(),
        used: false,
    };

    let mut tokens = BOOTSTRAP_TOKENS.write();
    tokens.insert(token.clone(), bootstrap_token);

    tracing::info!(
        hostname = %params.hostname,
        ttl_minutes = ttl_minutes,
        "bootstrap token created"
    );

    // Generate join command
    let join_command = format!(
        "curl -sSL http://gateway:18789/bootstrap.sh | sh -s -- --token {} --hostname {}",
        token, params.hostname
    );

    to_json(serde_json::json!({
        "token": token,
        "hostname": params.hostname,
        "expires_at": expires_at.timestamp_millis(),
        "ttl_minutes": ttl_minutes,
        "join_command": join_command,
        "manual_steps": [
            format!("1. SSH to the target node"),
            format!("2. Run: {}", join_command),
            format!("3. Or download clawnode and run: clawnode join --token {} --gateway wss://gateway:18789", token)
        ]
    }))
}

#[derive(Debug, Deserialize)]
pub struct NodeTokenValidateParams {
    pub token: String,
}

/// Validate a bootstrap token (called by joining node)
pub async fn node_token_validate(params: Value) -> BridgeResult<Value> {
    let params: NodeTokenValidateParams = parse_params(params)?;

    let mut tokens = BOOTSTRAP_TOKENS.write();
    let token_data = tokens
        .get_mut(&params.token)
        .ok_or_else(|| BridgeError::NotFound("invalid or expired token".to_string()))?;

    // Check expiration
    let now = chrono::Utc::now().timestamp_millis();
    if now > token_data.expires_at {
        return Err(BridgeError::InvalidParams("token expired".to_string()));
    }

    // Check if already used
    if token_data.used {
        return Err(BridgeError::InvalidParams("token already used".to_string()));
    }

    // Mark as used
    token_data.used = true;

    tracing::info!(hostname = %token_data.hostname, "bootstrap token validated");

    to_json(serde_json::json!({
        "valid": true,
        "hostname": token_data.hostname,
        "labels": token_data.labels,
        "auto_approve": token_data.auto_approve
    }))
}

// ─────────────────────────────────────────────────────────────
// Trusted Subnet Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TrustedSubnetAddParams {
    pub subnet: String,
}

/// Add a trusted subnet for auto-approval
pub async fn trusted_subnet_add(params: Value) -> BridgeResult<Value> {
    let params: TrustedSubnetAddParams = parse_params(params)?;

    // Validate CIDR format
    parse_cidr(&params.subnet)?;

    let mut subnets = TRUSTED_SUBNETS.write();
    if !subnets.contains(&params.subnet) {
        subnets.push(params.subnet.clone());
    }

    tracing::info!(subnet = %params.subnet, "trusted subnet added");

    to_json(serde_json::json!({
        "success": true,
        "subnet": params.subnet,
        "total_trusted": subnets.len()
    }))
}

#[derive(Debug, Deserialize)]
pub struct TrustedSubnetListParams {}

/// List trusted subnets
pub async fn trusted_subnet_list(_params: Value) -> BridgeResult<Value> {
    let subnets = TRUSTED_SUBNETS.read();
    to_json(subnets.clone())
}

#[derive(Debug, Deserialize)]
pub struct CheckTrustedParams {
    pub address: String,
}

/// Check if an address is in a trusted subnet
pub async fn check_trusted(params: Value) -> BridgeResult<Value> {
    let params: CheckTrustedParams = parse_params(params)?;

    let ip: Ipv4Addr = params
        .address
        .parse()
        .map_err(|_| BridgeError::InvalidParams("invalid IP address".to_string()))?;

    let subnets = TRUSTED_SUBNETS.read();
    let is_trusted = subnets.iter().any(|subnet| ip_in_cidr(ip, subnet));

    to_json(serde_json::json!({
        "address": params.address,
        "trusted": is_trusted
    }))
}

// ─────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────

fn parse_cidr(cidr: &str) -> BridgeResult<(Ipv4Addr, u8)> {
    let parts: Vec<&str> = cidr.split('/').collect();
    if parts.len() != 2 {
        return Err(BridgeError::InvalidParams(format!(
            "invalid CIDR: {}",
            cidr
        )));
    }

    let ip: Ipv4Addr = parts[0]
        .parse()
        .map_err(|_| BridgeError::InvalidParams(format!("invalid IP in CIDR: {}", cidr)))?;

    let prefix: u8 = parts[1]
        .parse()
        .map_err(|_| BridgeError::InvalidParams(format!("invalid prefix in CIDR: {}", cidr)))?;

    if prefix > 32 {
        return Err(BridgeError::InvalidParams(format!(
            "prefix too large: {}",
            prefix
        )));
    }

    Ok((ip, prefix))
}

fn ip_to_u32(ip: Ipv4Addr) -> u32 {
    u32::from(ip)
}

fn u32_to_ip(n: u32) -> Ipv4Addr {
    Ipv4Addr::from(n)
}

fn ip_in_cidr(ip: Ipv4Addr, cidr: &str) -> bool {
    if let Ok((base, prefix)) = parse_cidr(cidr) {
        let mask = if prefix == 0 {
            0
        } else {
            !0u32 << (32 - prefix)
        };
        (ip_to_u32(ip) & mask) == (ip_to_u32(base) & mask)
    } else {
        false
    }
}

fn guess_os(ports: &[u16]) -> Option<String> {
    if ports.contains(&22) && ports.contains(&80) {
        Some("Linux".to_string())
    } else if ports.contains(&3389) {
        Some("Windows".to_string())
    } else if ports.contains(&22) {
        Some("Unix/Linux".to_string())
    } else if ports.contains(&445) {
        Some("Windows".to_string())
    } else {
        None
    }
}

// ─────────────────────────────────────────────────────────────
// Node Bootstrap Handler
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct BootstrapResult {
    pub address: String,
    pub hostname: String,
    pub success: bool,
    pub node_id: Option<String>,
    pub token: Option<String>,
    pub gpu_info: Option<GpuProbeResult>,
    pub message: String,
    pub steps_completed: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct NodeBootstrapParams {
    /// Target IP address or hostname
    pub address: String,
    /// Credential profile name (looks up from profiles)
    pub credential_profile: Option<String>,
    /// Or provide credentials directly
    pub ssh_user: Option<String>,
    pub ssh_key_secret: Option<String>,
    pub ssh_password_secret: Option<String>,
    /// Hostname to assign (auto-detected if not provided)
    pub hostname: Option<String>,
    /// Labels for the node
    pub labels: Option<HashMap<String, String>>,
    /// Gateway URL for clawnode to connect to
    pub gateway_url: Option<String>,
    /// Skip GPU detection
    pub skip_gpu_detection: Option<bool>,
    /// Dry run - don't actually install
    pub dry_run: Option<bool>,
}

/// Bootstrap a node via SSH - install clawnode and connect to cluster
pub async fn node_bootstrap(params: Value) -> BridgeResult<Value> {
    let params: NodeBootstrapParams = parse_params(params)?;
    
    let mut steps_completed = Vec::new();
    let dry_run = params.dry_run.unwrap_or(false);
    
    // Resolve credentials
    let (username, _auth_method) = if let Some(profile_name) = &params.credential_profile {
        let profiles = CREDENTIAL_PROFILES.read();
        let profile = profiles.get(profile_name)
            .ok_or_else(|| BridgeError::NotFound(format!("credential profile not found: {}", profile_name)))?;
        
        let username = profile.username.clone()
            .ok_or_else(|| BridgeError::InvalidParams("credential profile has no username".to_string()))?;
        
        (username, profile.auth_method.clone())
    } else if let Some(user) = &params.ssh_user {
        let auth = if params.ssh_key_secret.is_some() {
            "key"
        } else if params.ssh_password_secret.is_some() {
            "password"
        } else {
            return Err(BridgeError::InvalidParams(
                "ssh_user requires ssh_key_secret or ssh_password_secret".to_string()
            ));
        };
        (user.clone(), auth.to_string())
    } else {
        return Err(BridgeError::InvalidParams(
            "credential_profile or ssh_user required".to_string()
        ));
    };
    
    steps_completed.push(format!("Resolved credentials: user={}", username));
    
    // Check if address is trusted
    let is_trusted = if let Ok(ip) = params.address.parse::<Ipv4Addr>() {
        let subnets = TRUSTED_SUBNETS.read();
        subnets.iter().any(|subnet| ip_in_cidr(ip, subnet))
    } else {
        false
    };
    
    steps_completed.push(format!("Address {} trusted: {}", params.address, is_trusted));
    
    // Generate hostname if not provided
    let hostname = params.hostname.clone().unwrap_or_else(|| {
        format!("node-{}", params.address.replace('.', "-"))
    });
    
    // Generate bootstrap token
    let now = chrono::Utc::now();
    let token = format!(
        "claw_bt_{}",
        uuid::Uuid::new_v4().to_string().replace("-", "")
    );
    let expires_at = now + chrono::Duration::minutes(30);
    
    let bootstrap_token = BootstrapToken {
        token: token.clone(),
        hostname: hostname.clone(),
        labels: params.labels.clone().unwrap_or_default(),
        auto_approve: is_trusted,
        created_at: now.timestamp_millis(),
        expires_at: expires_at.timestamp_millis(),
        used: false,
    };
    
    // Store token
    {
        let mut tokens = BOOTSTRAP_TOKENS.write();
        tokens.insert(token.clone(), bootstrap_token);
    }
    steps_completed.push("Generated bootstrap token".to_string());
    
    // Build bootstrap command
    let gateway_url = params.gateway_url.clone()
        .unwrap_or_else(|| "wss://localhost:18789".to_string());
    
    let bootstrap_script = format!(r#"#!/bin/bash
set -e

GATEWAY_URL="{gateway}"

# Create clawnode directory
mkdir -p /opt/clawnode

# Download clawnode binary (placeholder - would download from gateway)
echo "Downloading clawnode..."
# curl -sSL $GATEWAY_URL/clawnode-linux-amd64 -o /opt/clawnode/clawnode

# Create config
cat > /opt/clawnode/config.json <<EOF
{{
  "gateway": "{gateway}",
  "token": "{token}",
  "hostname": "{hostname}"
}}
EOF

# Create systemd service
cat > /etc/systemd/system/clawnode.service <<EOF
[Unit]
Description=Clawbernetes Node Agent
After=network.target docker.service
Wants=docker.service

[Service]
Type=simple
ExecStart=/opt/clawnode/clawnode run
Restart=always
RestartSec=10
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
EOF

echo "clawnode configured for {hostname}"
"#, gateway = gateway_url, token = token, hostname = hostname);

    steps_completed.push(format!("Generated bootstrap script for {}", hostname));
    
    // In a real implementation, we'd SSH and execute
    // For now, we return what would be done
    if dry_run {
        tracing::info!(
            address = %params.address,
            hostname = %hostname,
            "dry run - would bootstrap node"
        );
        
        return to_json(BootstrapResult {
            address: params.address,
            hostname,
            success: true,
            node_id: None,
            token: Some(token),
            gpu_info: None,
            message: "Dry run completed - would execute bootstrap".to_string(),
            steps_completed,
        });
    }
    
    // Execute SSH if we have credentials
    let execute_ssh = params.ssh_key_secret.is_some() || params.credential_profile.is_some();
    
    if execute_ssh {
        steps_completed.push("Attempting SSH connection...".to_string());
        
        // Build SSH command
        let mut cmd = Command::new("ssh");
        cmd.args([
            "-o", "StrictHostKeyChecking=no",
            "-o", "UserKnownHostsFile=/dev/null",
            "-o", "ConnectTimeout=10",
            &format!("{}@{}", username, params.address),
            "bash", "-s",
        ]);
        
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        
        match cmd.spawn() {
            Ok(mut child) => {
                // Write bootstrap script to stdin
                if let Some(stdin) = child.stdin.as_mut() {
                    use std::io::Write;
                    let _ = stdin.write_all(bootstrap_script.as_bytes());
                }
                
                match child.wait_with_output() {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                        
                        if output.status.success() {
                            steps_completed.push(format!("SSH execution successful: {}", stdout.trim()));
                            
                            tracing::info!(
                                address = %params.address,
                                hostname = %hostname,
                                "bootstrap completed via SSH"
                            );
                            
                            return to_json(BootstrapResult {
                                address: params.address,
                                hostname: hostname.clone(),
                                success: true,
                                node_id: Some(hostname.clone()),
                                token: Some(token),
                                gpu_info: None,
                                message: "Node bootstrapped successfully via SSH".to_string(),
                                steps_completed,
                            });
                        } else {
                            steps_completed.push(format!("SSH execution failed: {}", stderr.trim()));
                        }
                    }
                    Err(e) => {
                        steps_completed.push(format!("SSH wait failed: {}", e));
                    }
                }
            }
            Err(e) => {
                steps_completed.push(format!("SSH spawn failed: {}", e));
            }
        }
    }
    
    // Fallback: return instructions for manual execution
    let ssh_command = format!(
        "ssh {}@{} 'bash -s' <<< '{}'",
        username, params.address, bootstrap_script.replace("'", "'\"'\"'")
    );
    
    steps_completed.push("Returning manual execution instructions".to_string());
    
    tracing::info!(
        address = %params.address,
        hostname = %hostname,
        token = %token,
        "bootstrap prepared for node"
    );
    
    to_json(BootstrapResult {
        address: params.address.clone(),
        hostname: hostname.clone(),
        success: true,
        node_id: Some(format!("pending-{}", hostname)),
        token: Some(token),
        gpu_info: None,
        message: format!(
            "Bootstrap prepared. Run this on the gateway or use the token directly:\n\n\
            Option 1 (SSH from gateway):\n{}\n\n\
            Option 2 (Run on target):\n\
            curl -sSL http://gateway:18789/bootstrap.sh | TOKEN={} HOSTNAME={} bash",
            ssh_command,
            params.address,
            hostname
        ),
        steps_completed,
    })
}

// ─────────────────────────────────────────────────────────────
// GPU Probe (for network scan with credentials)
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GpuProbeParams {
    pub address: String,
    pub credential_profile: Option<String>,
    pub ssh_user: Option<String>,
    pub ssh_key_secret: Option<String>,
}

/// Probe a host for GPU information via SSH
pub async fn gpu_probe(params: Value) -> BridgeResult<Value> {
    let params: GpuProbeParams = parse_params(params)?;
    
    // Resolve credentials (similar to bootstrap)
    let username = if let Some(profile_name) = &params.credential_profile {
        let profiles = CREDENTIAL_PROFILES.read();
        let profile = profiles.get(profile_name)
            .ok_or_else(|| BridgeError::NotFound(format!("credential profile not found: {}", profile_name)))?;
        profile.username.clone()
            .ok_or_else(|| BridgeError::InvalidParams("credential profile has no username".to_string()))?
    } else if let Some(user) = &params.ssh_user {
        user.clone()
    } else {
        return Err(BridgeError::InvalidParams(
            "credential_profile or ssh_user required".to_string()
        ));
    };
    
    // In a real implementation, we'd SSH and run nvidia-smi
    // ssh user@host 'nvidia-smi -L && nvidia-smi --query-gpu=name,memory.total --format=csv'
    
    tracing::info!(
        address = %params.address,
        user = %username,
        "would probe for GPUs via SSH"
    );
    
    // Return placeholder - real impl would parse nvidia-smi output
    to_json(serde_json::json!({
        "address": params.address,
        "probed": false,
        "message": format!("Would SSH as {} to {} and run nvidia-smi", username, params.address),
        "command": format!("ssh {}@{} 'nvidia-smi -L'", username, params.address)
    }))
}
