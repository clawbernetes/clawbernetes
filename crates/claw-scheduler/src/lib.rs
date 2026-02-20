//! Job scheduling, cron, namespaces, policies, alerts, and audit for Clawbernetes.
//!
//! Provides stores for batch jobs, cron jobs, namespace management with resource quotas,
//! admission policies, alert rules, and audit logging.

#![forbid(unsafe_code)]

use claw_persist::JsonStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, warn};

// ─────────────────────────────────────────────────────────────
// Alert Store
// ─────────────────────────────────────────────────────────────

/// Alert rule for metrics-based alerting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    /// Unique alert name.
    pub name: String,
    /// Metric name to evaluate.
    pub metric: String,
    /// Condition: "above" or "below".
    pub condition: String,
    /// Threshold value.
    pub threshold: f64,
    /// Current state: "ok", "firing", "acknowledged".
    pub state: String,
    /// When the alert was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the state last changed.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory alert store backed by JSON snapshots.
pub struct AlertStore {
    alerts: HashMap<String, AlertRule>,
    store: JsonStore,
}

impl AlertStore {
    /// Create a new alert store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "alerts");
        let alerts = store.load();
        debug!(count = alerts.len(), "loaded alerts from disk");
        Self { alerts, store }
    }

    /// Create a new alert rule.
    pub fn create(&mut self, rule: AlertRule) -> Result<(), String> {
        if self.alerts.contains_key(&rule.name) {
            return Err(format!("alert '{}' already exists", rule.name));
        }
        let name = rule.name.clone();
        self.alerts.insert(name, rule);
        self.snapshot();
        Ok(())
    }

    /// Get an alert by name.
    pub fn get(&self, name: &str) -> Option<&AlertRule> {
        self.alerts.get(name)
    }

    /// Get a mutable reference to an alert.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut AlertRule> {
        self.alerts.get_mut(name)
    }

    /// List all alerts.
    pub fn list(&self) -> Vec<&AlertRule> {
        self.alerts.values().collect()
    }

    /// Evaluate an alert rule against a metric value, updating state.
    pub fn evaluate(&mut self, name: &str, value: f64) -> Option<&AlertRule> {
        let alert = self.alerts.get_mut(name)?;
        let was_firing = alert.state == "firing";
        let is_firing = match alert.condition.as_str() {
            "above" => value > alert.threshold,
            "below" => value < alert.threshold,
            _ => false,
        };

        if is_firing && !was_firing {
            alert.state = "firing".to_string();
            alert.updated_at = chrono::Utc::now();
            self.snapshot();
        } else if !is_firing && was_firing {
            alert.state = "ok".to_string();
            alert.updated_at = chrono::Utc::now();
            self.snapshot();
        }

        self.alerts.get(name)
    }

    /// Acknowledge an alert (only if firing).
    pub fn acknowledge(&mut self, name: &str) -> Result<(), String> {
        let alert = self
            .alerts
            .get_mut(name)
            .ok_or_else(|| format!("alert '{name}' not found"))?;
        if alert.state != "firing" {
            return Err(format!("alert '{name}' is not firing (state: {})", alert.state));
        }
        alert.state = "acknowledged".to_string();
        alert.updated_at = chrono::Utc::now();
        self.snapshot();
        Ok(())
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.alerts) {
            warn!(error = %e, "failed to snapshot alert store");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Job Store
// ─────────────────────────────────────────────────────────────

/// A batch job entry tracking completion state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobEntry {
    /// Job name.
    pub name: String,
    /// Container image.
    pub image: String,
    /// Command to run.
    pub command: Vec<String>,
    /// Target completions.
    pub completions: u32,
    /// Completed count.
    pub completed: u32,
    /// Failed count.
    pub failed: u32,
    /// Parallelism level.
    pub parallelism: u32,
    /// Max retry attempts.
    pub backoff_limit: u32,
    /// Active container IDs.
    pub container_ids: Vec<String>,
    /// State: "pending", "running", "completed", "failed".
    pub state: String,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Completion timestamp.
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// In-memory job store backed by JSON snapshots.
pub struct JobStore {
    jobs: HashMap<String, JobEntry>,
    store: JsonStore,
}

impl JobStore {
    /// Create a new job store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "jobs");
        let jobs = store.load();
        debug!(count = jobs.len(), "loaded jobs from disk");
        Self { jobs, store }
    }

    /// Create a new job.
    pub fn create(&mut self, entry: JobEntry) -> Result<(), String> {
        if self.jobs.contains_key(&entry.name) {
            return Err(format!("job '{}' already exists", entry.name));
        }
        let name = entry.name.clone();
        self.jobs.insert(name, entry);
        self.snapshot();
        Ok(())
    }

    /// Get a job by name.
    pub fn get(&self, name: &str) -> Option<&JobEntry> {
        self.jobs.get(name)
    }

    /// Get a mutable reference to a job.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut JobEntry> {
        self.jobs.get_mut(name)
    }

    /// Delete a job.
    pub fn delete(&mut self, name: &str) -> Result<JobEntry, String> {
        self.jobs
            .remove(name)
            .ok_or_else(|| format!("job '{name}' not found"))
            .inspect(|_| self.snapshot())
    }

    /// List all jobs.
    pub fn list(&self) -> Vec<&JobEntry> {
        self.jobs.values().collect()
    }

    /// Snapshot after external mutation.
    pub fn update(&mut self) {
        self.snapshot();
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.jobs) {
            warn!(error = %e, "failed to snapshot job store");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Cron Store
// ─────────────────────────────────────────────────────────────

/// A cron job entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronEntry {
    /// Cron job name.
    pub name: String,
    /// Cron schedule expression.
    pub schedule: String,
    /// Container image.
    pub image: String,
    /// Command to run.
    pub command: Vec<String>,
    /// Whether the cron is suspended.
    pub suspended: bool,
    /// Last run timestamp.
    pub last_run: Option<chrono::DateTime<chrono::Utc>>,
    /// Next scheduled run.
    pub next_run: Option<chrono::DateTime<chrono::Utc>>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory cron store backed by JSON snapshots.
pub struct CronStore {
    crons: HashMap<String, CronEntry>,
    store: JsonStore,
}

impl CronStore {
    /// Create a new cron store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "crons");
        let crons = store.load();
        debug!(count = crons.len(), "loaded crons from disk");
        Self { crons, store }
    }

    /// Create a new cron job.
    pub fn create(&mut self, entry: CronEntry) -> Result<(), String> {
        if self.crons.contains_key(&entry.name) {
            return Err(format!("cron '{}' already exists", entry.name));
        }
        let name = entry.name.clone();
        self.crons.insert(name, entry);
        self.snapshot();
        Ok(())
    }

    /// Get a cron job by name.
    pub fn get(&self, name: &str) -> Option<&CronEntry> {
        self.crons.get(name)
    }

    /// Get a mutable reference.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut CronEntry> {
        self.crons.get_mut(name)
    }

    /// List all cron jobs.
    pub fn list(&self) -> Vec<&CronEntry> {
        self.crons.values().collect()
    }

    /// Snapshot after external mutation.
    pub fn update(&mut self) {
        self.snapshot();
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.crons) {
            warn!(error = %e, "failed to snapshot cron store");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Namespace Store
// ─────────────────────────────────────────────────────────────

/// Resource quota for a namespace.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceQuota {
    /// Maximum CPU cores.
    pub max_cpu: Option<f64>,
    /// Maximum memory in MB.
    pub max_memory_mb: Option<u64>,
    /// Maximum GPU count.
    pub max_gpus: Option<u32>,
    /// Maximum storage in GB.
    pub max_storage_gb: Option<u64>,
}

/// A namespace entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceEntry {
    /// Namespace name.
    pub name: String,
    /// Resource quotas.
    pub quotas: ResourceQuota,
    /// Labels.
    pub labels: HashMap<String, String>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// A taint applied to a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintEntry {
    /// Taint key.
    pub key: String,
    /// Taint value.
    pub value: String,
    /// Effect: "NoSchedule", "PreferNoSchedule", "NoExecute".
    pub effect: String,
}

/// In-memory namespace store backed by JSON snapshots.
pub struct NamespaceStore {
    namespaces: HashMap<String, NamespaceEntry>,
    /// Node taints.
    pub taints: Vec<TaintEntry>,
    store: JsonStore,
}

impl NamespaceStore {
    /// Create a new namespace store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "namespaces");
        let namespaces = store.load();
        debug!(count = namespaces.len(), "loaded namespaces from disk");
        Self {
            namespaces,
            taints: Vec::new(),
            store,
        }
    }

    /// Create a new namespace.
    pub fn create(&mut self, entry: NamespaceEntry) -> Result<(), String> {
        if self.namespaces.contains_key(&entry.name) {
            return Err(format!("namespace '{}' already exists", entry.name));
        }
        let name = entry.name.clone();
        self.namespaces.insert(name, entry);
        self.snapshot();
        Ok(())
    }

    /// Get a namespace by name.
    pub fn get(&self, name: &str) -> Option<&NamespaceEntry> {
        self.namespaces.get(name)
    }

    /// Get a mutable reference.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut NamespaceEntry> {
        self.namespaces.get_mut(name)
    }

    /// List all namespaces.
    pub fn list(&self) -> Vec<&NamespaceEntry> {
        self.namespaces.values().collect()
    }

    /// Snapshot after external mutation.
    pub fn update(&mut self) {
        self.snapshot();
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.namespaces) {
            warn!(error = %e, "failed to snapshot namespace store");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Audit Store
// ─────────────────────────────────────────────────────────────

/// An audit log entry (lightweight, for command-level auditing).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Principal who performed the action.
    pub principal: String,
    /// Action performed.
    pub action: String,
    /// Resource type.
    pub resource: String,
    /// Result.
    pub result: String,
    /// Timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// In-memory audit store backed by JSON snapshots.
pub struct AuditStore {
    entries: Vec<AuditEntry>,
    store: JsonStore,
}

impl AuditStore {
    /// Create a new audit store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "audit");
        let map: HashMap<String, AuditEntry> = store.load();
        let mut entries: Vec<_> = map.into_values().collect();
        entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        debug!(count = entries.len(), "loaded audit entries from disk");
        Self { entries, store }
    }

    /// Record an audit entry.
    pub fn record(&mut self, entry: AuditEntry) {
        self.entries.push(entry);
        self.snapshot();
    }

    /// Query audit entries with optional filters.
    pub fn query(
        &self,
        principal: Option<&str>,
        action: Option<&str>,
        range_minutes: Option<i64>,
    ) -> Vec<&AuditEntry> {
        let cutoff = range_minutes.map(|m| chrono::Utc::now() - chrono::Duration::minutes(m));
        self.entries
            .iter()
            .filter(|e| principal.is_none_or(|p| e.principal == p))
            .filter(|e| action.is_none_or(|a| e.action == a))
            .filter(|e| cutoff.is_none_or(|c| e.timestamp >= c))
            .collect()
    }

    fn snapshot(&self) {
        let map: HashMap<String, &AuditEntry> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (i.to_string(), e))
            .collect();
        if let Err(e) = self.store.save(&map) {
            warn!(error = %e, "failed to snapshot audit store");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Policy Store
// ─────────────────────────────────────────────────────────────

/// A policy rule for admission control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEntry {
    /// Policy name.
    pub name: String,
    /// Policy type.
    pub policy_type: String,
    /// Policy rules (JSON values).
    pub rules: Vec<serde_json::Value>,
    /// Whether the policy is enabled.
    pub enabled: bool,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory policy store backed by JSON snapshots.
pub struct PolicyStore {
    policies: HashMap<String, PolicyEntry>,
    store: JsonStore,
}

impl PolicyStore {
    /// Create a new policy store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "policies");
        let policies = store.load();
        debug!(count = policies.len(), "loaded policies from disk");
        Self { policies, store }
    }

    /// Create a new policy.
    pub fn create(&mut self, entry: PolicyEntry) -> Result<(), String> {
        if self.policies.contains_key(&entry.name) {
            return Err(format!("policy '{}' already exists", entry.name));
        }
        let name = entry.name.clone();
        self.policies.insert(name, entry);
        self.snapshot();
        Ok(())
    }

    /// Get a policy by name.
    pub fn get(&self, name: &str) -> Option<&PolicyEntry> {
        self.policies.get(name)
    }

    /// List all policies.
    pub fn list(&self) -> Vec<&PolicyEntry> {
        self.policies.values().collect()
    }

    /// List only enabled policies.
    pub fn list_enabled(&self) -> Vec<&PolicyEntry> {
        self.policies
            .values()
            .filter(|p| p.enabled)
            .collect()
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.policies) {
            warn!(error = %e, "failed to snapshot policy store");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_store_crud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = AlertStore::new(dir.path());

        let rule = AlertRule {
            name: "high-cpu".to_string(),
            metric: "cpu.usage".to_string(),
            condition: "above".to_string(),
            threshold: 90.0,
            state: "ok".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        store.create(rule).expect("create");
        assert!(store.get("high-cpu").is_some());
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn test_alert_evaluate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = AlertStore::new(dir.path());

        store.create(AlertRule {
            name: "cpu-alert".to_string(),
            metric: "cpu".to_string(),
            condition: "above".to_string(),
            threshold: 90.0,
            state: "ok".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }).expect("create");

        let alert = store.evaluate("cpu-alert", 50.0).expect("eval");
        assert_eq!(alert.state, "ok");

        let alert = store.evaluate("cpu-alert", 95.0).expect("eval");
        assert_eq!(alert.state, "firing");

        let alert = store.evaluate("cpu-alert", 80.0).expect("eval");
        assert_eq!(alert.state, "ok");
    }

    #[test]
    fn test_alert_acknowledge() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = AlertStore::new(dir.path());

        store.create(AlertRule {
            name: "mem-alert".to_string(),
            metric: "mem".to_string(),
            condition: "above".to_string(),
            threshold: 80.0,
            state: "ok".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }).expect("create");

        assert!(store.acknowledge("mem-alert").is_err());
        store.evaluate("mem-alert", 95.0);
        store.acknowledge("mem-alert").expect("ack");
        assert_eq!(store.get("mem-alert").expect("get").state, "acknowledged");
    }

    #[test]
    fn test_job_store_crud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = JobStore::new(dir.path());

        store.create(JobEntry {
            name: "train-v1".to_string(),
            image: "pytorch:latest".to_string(),
            command: vec!["python".to_string(), "train.py".to_string()],
            completions: 1,
            completed: 0,
            failed: 0,
            parallelism: 1,
            backoff_limit: 3,
            container_ids: vec![],
            state: "pending".to_string(),
            created_at: chrono::Utc::now(),
            finished_at: None,
        }).expect("create");

        assert!(store.get("train-v1").is_some());
        assert_eq!(store.list().len(), 1);
        store.delete("train-v1").expect("delete");
        assert!(store.get("train-v1").is_none());
    }

    #[test]
    fn test_cron_store_crud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = CronStore::new(dir.path());

        store.create(CronEntry {
            name: "nightly-backup".to_string(),
            schedule: "0 2 * * *".to_string(),
            image: "backup:latest".to_string(),
            command: vec!["backup.sh".to_string()],
            suspended: false,
            last_run: None,
            next_run: None,
            created_at: chrono::Utc::now(),
        }).expect("create");

        assert!(store.get("nightly-backup").is_some());

        if let Some(c) = store.get_mut("nightly-backup") {
            c.suspended = true;
        }
        store.update();

        assert!(store.get("nightly-backup").expect("get").suspended);
    }

    #[test]
    fn test_namespace_store_crud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = NamespaceStore::new(dir.path());

        store.create(NamespaceEntry {
            name: "production".to_string(),
            quotas: ResourceQuota {
                max_cpu: Some(32.0),
                max_memory_mb: Some(65536),
                max_gpus: Some(8),
                max_storage_gb: Some(1000),
            },
            labels: HashMap::new(),
            created_at: chrono::Utc::now(),
        }).expect("create");

        assert!(store.get("production").is_some());
        assert_eq!(store.list().len(), 1);

        assert!(store.create(NamespaceEntry {
            name: "production".to_string(),
            quotas: ResourceQuota::default(),
            labels: HashMap::new(),
            created_at: chrono::Utc::now(),
        }).is_err());
    }

    #[test]
    fn test_audit_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = AuditStore::new(dir.path());

        store.record(AuditEntry {
            principal: "admin".to_string(),
            action: "deploy".to_string(),
            resource: "deployment".to_string(),
            result: "success".to_string(),
            timestamp: chrono::Utc::now(),
        });

        store.record(AuditEntry {
            principal: "user".to_string(),
            action: "list".to_string(),
            resource: "workload".to_string(),
            result: "success".to_string(),
            timestamp: chrono::Utc::now(),
        });

        assert_eq!(store.query(None, None, None).len(), 2);
        assert_eq!(store.query(Some("admin"), None, None).len(), 1);
        assert_eq!(store.query(None, Some("deploy"), None).len(), 1);
    }

    #[test]
    fn test_policy_store_crud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = PolicyStore::new(dir.path());

        store.create(PolicyEntry {
            name: "no-privileged".to_string(),
            policy_type: "security".to_string(),
            rules: vec![serde_json::json!({"deny": "privileged"})],
            enabled: true,
            created_at: chrono::Utc::now(),
        }).expect("create");

        store.create(PolicyEntry {
            name: "resource-limits".to_string(),
            policy_type: "resource".to_string(),
            rules: vec![],
            enabled: false,
            created_at: chrono::Utc::now(),
        }).expect("create");

        assert_eq!(store.list().len(), 2);
        assert_eq!(store.list_enabled().len(), 1);
    }

    #[test]
    fn test_taint_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = NamespaceStore::new(dir.path());

        store.taints.push(TaintEntry {
            key: "gpu".to_string(),
            value: "true".to_string(),
            effect: "NoSchedule".to_string(),
        });

        assert_eq!(store.taints.len(), 1);
    }
}
