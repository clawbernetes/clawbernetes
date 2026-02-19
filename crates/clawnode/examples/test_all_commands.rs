//! Integration test exercising all command tiers via handle_command.
//!
//! Run with: cargo run -p clawnode --example test_all_commands

use clawnode::commands::{handle_command, CommandRequest};
use clawnode::config::NodeConfig;
use serde_json::json;

#[tokio::main]
async fn main() {
    let mut config = NodeConfig::default();
    let dir = tempfile::tempdir().expect("tempdir");
    config.state_path = dir.path().to_path_buf();
    let state = clawnode::create_state(config);

    let tests: Vec<(&str, serde_json::Value)> = vec![
        // ─── Tier 1: GPU ───
        ("gpu.list", json!({})),
        ("gpu.metrics", json!({})),

        // ─── Tier 2: System ───
        ("system.info", json!({})),
        ("system.run", json!({"command": ["echo", "hello from clawbernetes"]})),
        ("system.which", json!({"bins": ["docker", "nvidia-smi", "python3"]})),
        ("node.capabilities", json!({})),
        ("node.health", json!({})),

        // ─── Tier 3: Config Maps ───
        ("config.create", json!({"name": "app-config", "data": {"db_host": "10.0.0.5", "db_port": "5432"}, "immutable": false})),
        ("config.get", json!({"name": "app-config"})),
        ("config.list", json!({})),
        ("config.update", json!({"name": "app-config", "data": {"db_host": "10.0.0.6"}})),
        ("config.delete", json!({"name": "app-config"})),

        // ─── Tier 4: Encrypted Secrets ───
        ("secret.create", json!({"name": "db-creds", "data": {"username": "admin", "password": "s3cret!@#"}})),
        ("secret.get", json!({"name": "db-creds"})),
        ("secret.list", json!({})),
        ("secret.rotate", json!({"name": "db-creds", "data": {"username": "admin", "password": "n3w-p@ss"}})),
        ("secret.get", json!({"name": "db-creds"})),

        // ─── Tier 5: Deployments ───
        ("deploy.history", json!({})),
        // Note: deploy.create needs Docker, so we skip the actual container ops here

        // ─── Tier 6: Jobs & Cron ───
        ("job.create", json!({"name": "fine-tune-llm", "image": "pytorch/pytorch:2.0", "command": ["python", "train.py", "--epochs", "50"], "completions": 1, "parallelism": 1, "backoffLimit": 3})),
        ("job.status", json!({"name": "fine-tune-llm"})),
        ("cron.create", json!({"name": "nightly-backup", "schedule": "0 3 * * *", "image": "backup:latest", "command": ["./run-backup.sh"]})),
        ("cron.list", json!({})),

        // ─── Tier 7: Namespaces ───
        ("namespace.create", json!({"name": "production", "labels": {"env": "prod", "team": "ml"}})),
        ("namespace.set_quota", json!({"name": "production", "maxCpu": 64, "maxMemoryGb": 256, "maxGpus": 8})),
        ("namespace.usage", json!({"name": "production"})),
        ("namespace.list", json!({})),
        ("node.label", json!({"labels": {"gpu-type": "rtx-3050-ti", "zone": "us-west-1"}})),

        // ─── Tier 8: Volumes & Storage ───
        ("volume.create", json!({"name": "training-data", "type": "emptydir", "size": "100Gi"})),
        ("volume.create", json!({"name": "model-checkpoints", "type": "emptydir", "size": "50Gi"})),
        ("volume.list", json!({})),
        ("volume.mount", json!({"name": "training-data", "containerId": "container-abc", "mountPath": "/data/train"})),
        ("volume.unmount", json!({"name": "training-data"})),
        ("backup.list", json!({})),

        // ─── Tier 9: Auth & RBAC ───
        ("auth.create_key", json!({"name": "ci-pipeline", "role": "operator", "scopes": ["deploy.*", "workload.*", "secret.get"]})),
        ("auth.create_key", json!({"name": "monitoring", "role": "viewer", "scopes": ["node.health", "gpu.*", "workload.list"]})),
        ("auth.list_keys", json!({})),
        ("audit.query", json!({"limit": 20})),

        // ─── Tier 10: Autoscaling ───
        ("autoscale.create", json!({"name": "inference-scaler", "target": "llm-serving", "minReplicas": 1, "maxReplicas": 8, "policyType": "target_utilization", "metric": "gpu_utilization", "threshold": 75.0})),
        ("autoscale.status", json!({"name": "inference-scaler"})),
        ("autoscale.adjust", json!({"name": "inference-scaler", "replicas": 4})),
        ("autoscale.status", json!({"name": "inference-scaler"})),

        // ─── Tier 11: Policy ───
        ("policy.create", json!({"name": "resource-limits", "type": "resource-limit", "rules": [{"maxMemoryGb": 64, "maxGpus": 4}]})),
        ("policy.create", json!({"name": "image-allowlist", "type": "image-whitelist", "rules": [{"allowedPrefixes": ["pytorch/", "nvidia/"]}]})),
        ("policy.list", json!({})),
        ("policy.validate", json!({"workloadSpec": {"image": "pytorch/pytorch:2.0", "memory": "32Gi", "gpus": 2}})),

        // ─── Workload Management ───
        ("workload.list", json!({})),

        // ─── Cleanup ───
        ("secret.delete", json!({"name": "db-creds"})),
        ("volume.delete", json!({"name": "training-data"})),
        ("volume.delete", json!({"name": "model-checkpoints"})),
        ("autoscale.delete", json!({"name": "inference-scaler"})),
    ];

    println!("\n🔧 Clawbernetes Command Surface Test");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let mut passed = 0;
    let mut failed = 0;
    let mut current_tier = "";

    for (cmd, params) in &tests {
        let tier = match cmd.split('.').next().unwrap_or("") {
            "gpu" => "GPU Management",
            "system" | "node" if !cmd.starts_with("node.label") && !cmd.starts_with("node.taint") && !cmd.starts_with("node.drain") => "System Info",
            "config" => "Config Maps",
            "secret" => "Encrypted Secrets",
            "deploy" => "Deployments",
            "job" | "cron" => "Jobs & Cron",
            "namespace" | "node" => "Namespaces & Labels",
            "volume" | "backup" => "Volumes & Storage",
            "auth" | "audit" => "Auth & RBAC",
            "autoscale" => "Autoscaling",
            "policy" => "Policy Engine",
            "workload" | "container" => "Workload Management",
            _ => "Other",
        };

        if tier != current_tier {
            if !current_tier.is_empty() {
                println!();
            }
            println!("  ── {} ──", tier);
            current_tier = tier;
        }

        let req = CommandRequest {
            command: cmd.to_string(),
            params: params.clone(),
        };
        match handle_command(&state, req).await {
            Ok(result) => {
                let s = result.to_string();
                let display = if s.len() > 90 { format!("{}…", &s[..90]) } else { s };
                println!("  ✅ {:<25} {}", cmd, display);
                passed += 1;
            }
            Err(e) => {
                let e_str = e.to_string();
                let display = if e_str.len() > 80 { format!("{}…", &e_str[..80]) } else { e_str };
                println!("  ❌ {:<25} {}", cmd, display);
                failed += 1;
            }
        }
    }

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  Total: {} commands | ✅ {} passed | ❌ {} failed", passed + failed, passed, failed);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    if failed > 0 {
        std::process::exit(1);
    }
}
