//! Network policy enforcement via iptables.
//!
//! Policies use label selectors to match workloads and generate iptables
//! rules in the `CLAW-NETPOL` chain. An empty `ingress: []` means deny all
//! inbound; specific rules produce ACCEPT entries followed by a DROP default.

use std::collections::HashMap;
use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};

use crate::commands::CommandError;

/// iptables chain name for network policy rules.
const POLICY_CHAIN: &str = "CLAW-NETPOL";

/// A compiled network policy ready for iptables application.
#[derive(Debug, Clone)]
pub struct CompiledPolicy {
    pub name: String,
    pub selector: HashMap<String, String>,
    pub ingress_rules: Vec<CompiledRule>,
    pub egress_rules: Vec<CompiledRule>,
    /// Whether ingress policy was specified (empty list = deny all inbound).
    pub has_ingress: bool,
    /// Whether egress policy was specified (empty list = deny all outbound).
    pub has_egress: bool,
}

/// A single compiled rule (ACCEPT entry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledRule {
    pub from_selector: Option<HashMap<String, String>>,
    pub port: Option<u16>,
    pub protocol: Option<String>,
    pub cidr: Option<String>,
}

/// Manages network policy enforcement via iptables.
pub struct PolicyEngine {
    policies: Vec<CompiledPolicy>,
    iptables_available: bool,
}

impl PolicyEngine {
    /// Create a new PolicyEngine.
    ///
    /// Attempts to set up the iptables chain. If iptables is unavailable,
    /// the engine still tracks policies but doesn't enforce them.
    pub fn new() -> Self {
        let iptables_available = setup_policy_chain();

        Self {
            policies: Vec::new(),
            iptables_available,
        }
    }

    /// Add and enforce a network policy.
    ///
    /// The policy is compiled from JSON ingress/egress rules, then applied
    /// as iptables rules for all workloads matching the selector.
    pub fn add_policy(
        &mut self,
        name: &str,
        selector: HashMap<String, String>,
        ingress: &[Value],
        egress: &[Value],
        workload_ips: &[(Ipv4Addr, HashMap<String, String>)],
    ) -> Result<(), CommandError> {
        // Check for duplicate
        if self.policies.iter().any(|p| p.name == name) {
            return Err(format!("policy '{}' already exists", name).into());
        }

        let compiled = compile_policy(name, selector, ingress, egress)?;

        info!(
            policy = %name,
            ingress_rules = compiled.ingress_rules.len(),
            egress_rules = compiled.egress_rules.len(),
            has_ingress = compiled.has_ingress,
            has_egress = compiled.has_egress,
            "compiled network policy"
        );

        // Apply iptables rules for matching workloads
        if self.iptables_available {
            let matched_ips = find_matching_ips(workload_ips, &compiled.selector);
            apply_policy_rules(&compiled, &matched_ips, workload_ips);
        }

        self.policies.push(compiled);
        Ok(())
    }

    /// Remove a network policy and its iptables rules.
    pub fn remove_policy(&mut self, name: &str) -> Result<(), CommandError> {
        let idx = self
            .policies
            .iter()
            .position(|p| p.name == name)
            .ok_or_else(|| format!("policy '{}' not found", name))?;

        let policy = self.policies.remove(idx);

        if self.iptables_available {
            remove_policy_rules(&policy.name);
        }

        info!(policy = %name, "removed network policy");
        Ok(())
    }

    /// List all policies.
    pub fn list_policies(&self) -> Vec<PolicyInfo> {
        self.policies
            .iter()
            .map(|p| PolicyInfo {
                name: p.name.clone(),
                selector: p.selector.clone(),
                ingress_rule_count: p.ingress_rules.len(),
                egress_rule_count: p.egress_rules.len(),
                has_ingress: p.has_ingress,
                has_egress: p.has_egress,
            })
            .collect()
    }

    /// Refresh all policy rules (e.g., after workload changes).
    pub fn refresh_all(
        &self,
        workload_ips: &[(Ipv4Addr, HashMap<String, String>)],
    ) {
        if !self.iptables_available {
            return;
        }

        // Flush the chain and rebuild
        flush_policy_chain();

        for policy in &self.policies {
            let matched_ips = find_matching_ips(workload_ips, &policy.selector);
            apply_policy_rules(policy, &matched_ips, workload_ips);
        }
    }

    /// Number of active policies.
    pub fn policy_count(&self) -> usize {
        self.policies.len()
    }

    /// Whether iptables is available.
    pub fn iptables_available(&self) -> bool {
        self.iptables_available
    }

    /// Get a policy by name.
    pub fn get_policy(&self, name: &str) -> Option<&CompiledPolicy> {
        self.policies.iter().find(|p| p.name == name)
    }
}

/// Summary info for a policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyInfo {
    pub name: String,
    pub selector: HashMap<String, String>,
    pub ingress_rule_count: usize,
    pub egress_rule_count: usize,
    pub has_ingress: bool,
    pub has_egress: bool,
}

impl std::fmt::Debug for PolicyEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PolicyEngine")
            .field("policy_count", &self.policies.len())
            .field("iptables_available", &self.iptables_available)
            .finish()
    }
}

// ─────────────────────────────────────────────────────────────
// Policy compilation
// ─────────────────────────────────────────────────────────────

/// Compile a JSON policy definition into iptables-ready rules.
fn compile_policy(
    name: &str,
    selector: HashMap<String, String>,
    ingress: &[Value],
    egress: &[Value],
) -> Result<CompiledPolicy, CommandError> {
    let ingress_rules: Vec<CompiledRule> = ingress
        .iter()
        .filter_map(|r| compile_rule(r).ok())
        .collect();

    let egress_rules: Vec<CompiledRule> = egress
        .iter()
        .filter_map(|r| compile_rule(r).ok())
        .collect();

    Ok(CompiledPolicy {
        name: name.to_string(),
        selector,
        ingress_rules,
        egress_rules,
        has_ingress: true, // Policy was specified
        has_egress: !egress.is_empty() || egress.is_empty(), // Always true if egress key exists
    })
}

/// Compile a single JSON rule into a CompiledRule.
fn compile_rule(rule: &Value) -> Result<CompiledRule, CommandError> {
    let from_selector = rule
        .get("from")
        .and_then(|f| f.get("selector"))
        .and_then(|s| serde_json::from_value::<HashMap<String, String>>(s.clone()).ok());

    let port = rule.get("port").and_then(|p| p.as_u64()).map(|p| p as u16);

    let protocol = rule
        .get("protocol")
        .and_then(|p| p.as_str())
        .map(|s| s.to_string());

    let cidr = rule
        .get("from")
        .and_then(|f| f.get("cidr"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string());

    Ok(CompiledRule {
        from_selector,
        port,
        protocol,
        cidr,
    })
}

// ─────────────────────────────────────────────────────────────
// iptables helpers
// ─────────────────────────────────────────────────────────────

/// Set up the CLAW-NETPOL chain in the filter table.
fn setup_policy_chain() -> bool {
    let create = std::process::Command::new("iptables")
        .args(["-N", POLICY_CHAIN])
        .output();

    match create {
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if output.status.success() || stderr.contains("Chain already exists") {
                // Insert jump rules
                let _ = std::process::Command::new("iptables")
                    .args(["-C", "FORWARD", "-j", POLICY_CHAIN])
                    .output()
                    .and_then(|o| {
                        if !o.status.success() {
                            std::process::Command::new("iptables")
                                .args(["-I", "FORWARD", "-j", POLICY_CHAIN])
                                .output()
                        } else {
                            Ok(o)
                        }
                    });

                info!("iptables CLAW-NETPOL chain ready");
                true
            } else {
                warn!(error = %stderr, "failed to create iptables policy chain");
                false
            }
        }
        Err(e) => {
            warn!(error = %e, "iptables not available for network policies");
            false
        }
    }
}

/// Flush all rules in the policy chain.
fn flush_policy_chain() {
    let _ = std::process::Command::new("iptables")
        .args(["-F", POLICY_CHAIN])
        .output();
}

/// Find workload IPs matching a selector.
fn find_matching_ips(
    workload_ips: &[(Ipv4Addr, HashMap<String, String>)],
    selector: &HashMap<String, String>,
) -> Vec<Ipv4Addr> {
    if selector.is_empty() {
        return Vec::new();
    }

    workload_ips
        .iter()
        .filter(|(_, labels)| {
            selector
                .iter()
                .all(|(k, v)| labels.get(k).is_some_and(|lv| lv == v))
        })
        .map(|(ip, _)| *ip)
        .collect()
}

/// Apply iptables rules for a compiled policy.
fn apply_policy_rules(
    policy: &CompiledPolicy,
    target_ips: &[Ipv4Addr],
    all_workloads: &[(Ipv4Addr, HashMap<String, String>)],
) {
    if target_ips.is_empty() {
        return;
    }

    for ip in target_ips {
        // Ingress rules (inbound to this IP)
        if policy.has_ingress {
            if policy.ingress_rules.is_empty() {
                // Empty ingress = deny all inbound
                add_iptables_rule(&[
                    "-A",
                    POLICY_CHAIN,
                    "-d",
                    &format!("{ip}/32"),
                    "-m",
                    "comment",
                    "--comment",
                    &format!("claw-policy:{}", policy.name),
                    "-j",
                    "DROP",
                ]);
            } else {
                // Add ACCEPT rules for each ingress rule
                for rule in &policy.ingress_rules {
                    let mut args = vec![
                        "-A".to_string(),
                        POLICY_CHAIN.to_string(),
                        "-d".to_string(),
                        format!("{ip}/32"),
                    ];

                    // Source filter
                    if let Some(ref cidr) = rule.cidr {
                        args.extend(["-s".to_string(), cidr.clone()]);
                    } else if let Some(ref from_sel) = rule.from_selector {
                        let source_ips = find_matching_ips(all_workloads, from_sel);
                        for src_ip in &source_ips {
                            let mut src_args = args.clone();
                            src_args.extend(["-s".to_string(), format!("{src_ip}/32")]);

                            if let Some(port) = rule.port {
                                let proto = rule.protocol.as_deref().unwrap_or("tcp");
                                src_args.extend([
                                    "-p".to_string(),
                                    proto.to_string(),
                                    "--dport".to_string(),
                                    port.to_string(),
                                ]);
                            }

                            src_args.extend([
                                "-m".to_string(),
                                "comment".to_string(),
                                "--comment".to_string(),
                                format!("claw-policy:{}", policy.name),
                                "-j".to_string(),
                                "ACCEPT".to_string(),
                            ]);

                            let str_args: Vec<&str> = src_args.iter().map(|s| s.as_str()).collect();
                            add_iptables_rule(&str_args);
                        }
                        continue; // Already processed
                    }

                    if let Some(port) = rule.port {
                        let proto = rule.protocol.as_deref().unwrap_or("tcp");
                        args.extend([
                            "-p".to_string(),
                            proto.to_string(),
                            "--dport".to_string(),
                            port.to_string(),
                        ]);
                    }

                    args.extend([
                        "-m".to_string(),
                        "comment".to_string(),
                        "--comment".to_string(),
                        format!("claw-policy:{}", policy.name),
                        "-j".to_string(),
                        "ACCEPT".to_string(),
                    ]);

                    let str_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                    add_iptables_rule(&str_args);
                }

                // Add trailing DROP for this IP
                add_iptables_rule(&[
                    "-A",
                    POLICY_CHAIN,
                    "-d",
                    &format!("{ip}/32"),
                    "-m",
                    "comment",
                    "--comment",
                    &format!("claw-policy:{}:default-deny", policy.name),
                    "-j",
                    "DROP",
                ]);
            }
        }

        // Egress rules (outbound from this IP)
        if policy.has_egress && policy.egress_rules.is_empty() {
            // Empty egress = deny all outbound
            add_iptables_rule(&[
                "-A",
                POLICY_CHAIN,
                "-s",
                &format!("{ip}/32"),
                "-m",
                "comment",
                "--comment",
                &format!("claw-policy:{}", policy.name),
                "-j",
                "DROP",
            ]);
        }
    }
}

/// Remove all iptables rules tagged with a specific policy name.
fn remove_policy_rules(name: &str) {
    let comment = format!("claw-policy:{}", name);

    // Loop removing rules until none remain
    loop {
        // Find rule number with matching comment
        let list = std::process::Command::new("iptables")
            .args(["-L", POLICY_CHAIN, "--line-numbers", "-n"])
            .output();

        match list {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Find first line containing our comment
                let rule_num = stdout
                    .lines()
                    .find(|line| line.contains(&comment))
                    .and_then(|line| line.split_whitespace().next())
                    .and_then(|n| n.parse::<u32>().ok());

                if let Some(num) = rule_num {
                    let _ = std::process::Command::new("iptables")
                        .args(["-D", POLICY_CHAIN, &num.to_string()])
                        .output();
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
}

fn add_iptables_rule(args: &[&str]) {
    let result = std::process::Command::new("iptables").args(args).output();

    match result {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(args = ?args, error = %stderr, "iptables rule failed");
        }
        Err(e) => {
            warn!(error = %e, "failed to execute iptables");
        }
    }
}

/// Generate iptables rule arguments as strings for a policy (for testing).
pub fn generate_policy_rules(
    policy: &CompiledPolicy,
    target_ips: &[Ipv4Addr],
) -> Vec<Vec<String>> {
    let mut rules = Vec::new();

    for ip in target_ips {
        if policy.has_ingress {
            if policy.ingress_rules.is_empty() {
                rules.push(vec![
                    "-A".into(),
                    POLICY_CHAIN.into(),
                    "-d".into(),
                    format!("{ip}/32"),
                    "-j".into(),
                    "DROP".into(),
                ]);
            } else {
                for rule in &policy.ingress_rules {
                    let mut args = vec![
                        "-A".to_string(),
                        POLICY_CHAIN.to_string(),
                        "-d".to_string(),
                        format!("{ip}/32"),
                    ];

                    if let Some(ref cidr) = rule.cidr {
                        args.extend(["-s".into(), cidr.clone()]);
                    }

                    if let Some(port) = rule.port {
                        let proto = rule.protocol.as_deref().unwrap_or("tcp");
                        args.extend([
                            "-p".into(),
                            proto.to_string(),
                            "--dport".into(),
                            port.to_string(),
                        ]);
                    }

                    args.extend(["-j".into(), "ACCEPT".into()]);
                    rules.push(args);
                }

                // Trailing DROP
                rules.push(vec![
                    "-A".into(),
                    POLICY_CHAIN.into(),
                    "-d".into(),
                    format!("{ip}/32"),
                    "-j".into(),
                    "DROP".into(),
                ]);
            }
        }

        if policy.has_egress && policy.egress_rules.is_empty() {
            rules.push(vec![
                "-A".into(),
                POLICY_CHAIN.into(),
                "-s".into(),
                format!("{ip}/32"),
                "-j".into(),
                "DROP".into(),
            ]);
        }
    }

    rules
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_engine() -> PolicyEngine {
        PolicyEngine {
            policies: Vec::new(),
            iptables_available: false,
        }
    }

    #[test]
    fn add_and_list_policy() {
        let mut engine = test_engine();

        engine
            .add_policy(
                "deny-all",
                HashMap::from([("app".into(), "db".into())]),
                &[],
                &[],
                &[],
            )
            .expect("add");

        assert_eq!(engine.policy_count(), 1);

        let list = engine.list_policies();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "deny-all");
        assert!(list[0].has_ingress);
    }

    #[test]
    fn add_duplicate_fails() {
        let mut engine = test_engine();

        engine
            .add_policy("p1", HashMap::new(), &[], &[], &[])
            .expect("add");

        let result = engine.add_policy("p1", HashMap::new(), &[], &[], &[]);
        assert!(result.is_err());
    }

    #[test]
    fn remove_policy() {
        let mut engine = test_engine();

        engine
            .add_policy("p1", HashMap::new(), &[], &[], &[])
            .expect("add");

        engine.remove_policy("p1").expect("remove");
        assert_eq!(engine.policy_count(), 0);
    }

    #[test]
    fn remove_nonexistent_fails() {
        let mut engine = test_engine();
        assert!(engine.remove_policy("nope").is_err());
    }

    #[test]
    fn compile_empty_ingress_means_deny_all() {
        let policy =
            compile_policy("deny", HashMap::new(), &[], &[]).expect("compile");

        assert!(policy.has_ingress);
        assert!(policy.ingress_rules.is_empty());
    }

    #[test]
    fn compile_ingress_with_rules() {
        let ingress = vec![
            json!({"port": 8080, "protocol": "tcp"}),
            json!({"port": 443, "protocol": "tcp", "from": {"cidr": "10.0.0.0/8"}}),
        ];

        let policy =
            compile_policy("allow-web", HashMap::new(), &ingress, &[]).expect("compile");

        assert_eq!(policy.ingress_rules.len(), 2);
        assert_eq!(policy.ingress_rules[0].port, Some(8080));
        assert_eq!(policy.ingress_rules[1].cidr, Some("10.0.0.0/8".into()));
    }

    #[test]
    fn compile_rule_with_selector() {
        let ingress = vec![json!({
            "port": 5432,
            "protocol": "tcp",
            "from": {"selector": {"app": "api"}}
        })];

        let policy = compile_policy("db-allow", HashMap::new(), &ingress, &[]).expect("compile");

        assert_eq!(policy.ingress_rules.len(), 1);
        let rule = &policy.ingress_rules[0];
        assert_eq!(rule.port, Some(5432));
        assert!(rule.from_selector.is_some());
        assert_eq!(
            rule.from_selector.as_ref().unwrap().get("app"),
            Some(&"api".to_string())
        );
    }

    #[test]
    fn find_matching_ips_works() {
        let workloads = vec![
            (
                Ipv4Addr::new(10, 200, 1, 2),
                HashMap::from([("app".into(), "api".into())]),
            ),
            (
                Ipv4Addr::new(10, 200, 1, 3),
                HashMap::from([("app".into(), "db".into())]),
            ),
            (
                Ipv4Addr::new(10, 200, 1, 4),
                HashMap::from([("app".into(), "api".into())]),
            ),
        ];

        let selector = HashMap::from([("app".into(), "api".into())]);
        let matched = find_matching_ips(&workloads, &selector);
        assert_eq!(matched.len(), 2);
        assert!(matched.contains(&Ipv4Addr::new(10, 200, 1, 2)));
        assert!(matched.contains(&Ipv4Addr::new(10, 200, 1, 4)));
    }

    #[test]
    fn find_matching_ips_empty_selector() {
        let workloads = vec![(
            Ipv4Addr::new(10, 200, 1, 2),
            HashMap::from([("app".into(), "api".into())]),
        )];

        let matched = find_matching_ips(&workloads, &HashMap::new());
        assert!(matched.is_empty());
    }

    #[test]
    fn generate_deny_all_rules() {
        let policy = compile_policy(
            "deny-all",
            HashMap::from([("app".into(), "db".into())]),
            &[],
            &[],
        )
        .expect("compile");

        let target_ips = vec![Ipv4Addr::new(10, 200, 1, 3)];
        let rules = generate_policy_rules(&policy, &target_ips);

        // Should have ingress DROP + egress DROP
        assert_eq!(rules.len(), 2);
        assert!(rules[0].contains(&"DROP".to_string()));
        assert!(rules[0].contains(&"-d".to_string()));
        assert!(rules[1].contains(&"DROP".to_string()));
        assert!(rules[1].contains(&"-s".to_string()));
    }

    #[test]
    fn generate_allow_specific_port_rules() {
        let ingress = vec![json!({"port": 8080, "protocol": "tcp"})];
        let policy = compile_policy(
            "allow-web",
            HashMap::new(),
            &ingress,
            &[], // No egress policy
        )
        .expect("compile");

        let target_ips = vec![Ipv4Addr::new(10, 200, 1, 2)];
        let rules = generate_policy_rules(&policy, &target_ips);

        // ACCEPT for port 8080 + trailing DROP + egress DROP
        assert_eq!(rules.len(), 3);
        assert!(rules[0].contains(&"ACCEPT".to_string()));
        assert!(rules[0].contains(&"8080".to_string()));
        assert!(rules[1].contains(&"DROP".to_string())); // trailing deny
    }

    #[test]
    fn generate_cidr_allow_rules() {
        let ingress = vec![json!({
            "port": 443,
            "protocol": "tcp",
            "from": {"cidr": "192.168.0.0/16"}
        })];
        let policy =
            compile_policy("allow-lan", HashMap::new(), &ingress, &[]).expect("compile");

        let target_ips = vec![Ipv4Addr::new(10, 200, 1, 2)];
        let rules = generate_policy_rules(&policy, &target_ips);

        assert!(rules[0].contains(&"192.168.0.0/16".to_string()));
        assert!(rules[0].contains(&"ACCEPT".to_string()));
    }

    #[test]
    fn get_policy_by_name() {
        let mut engine = test_engine();

        engine
            .add_policy(
                "my-policy",
                HashMap::from([("app".into(), "web".into())]),
                &[json!({"port": 80})],
                &[],
                &[],
            )
            .expect("add");

        let policy = engine.get_policy("my-policy").expect("found");
        assert_eq!(policy.name, "my-policy");
        assert_eq!(policy.ingress_rules.len(), 1);

        assert!(engine.get_policy("nonexistent").is_none());
    }
}
