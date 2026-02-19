//! Persistence layer for node state
//!
//! Provides JSON-based persistence for low-write config data (secrets, configs, deployments)
//! and an in-memory config store for plain key-value configuration data.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// A simple JSON file-backed store for a single domain of data.
///
/// Keeps data in memory and snapshots to `{state_path}/state/{domain}.json` on every write.
pub struct JsonStore {
    path: PathBuf,
}

impl JsonStore {
    /// Create a new store for the given domain under `state_path`.
    pub fn new(state_path: &Path, domain: &str) -> Self {
        let path = state_path.join("state").join(format!("{domain}.json"));
        Self { path }
    }

    /// Load data from disk. Returns empty map if file doesn't exist.
    pub fn load<T: for<'de> Deserialize<'de>>(&self) -> HashMap<String, T> {
        match std::fs::read_to_string(&self.path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                warn!(path = %self.path.display(), error = %e, "corrupt state file, starting fresh");
                HashMap::new()
            }),
            Err(_) => {
                debug!(path = %self.path.display(), "no state file, starting fresh");
                HashMap::new()
            }
        }
    }

    /// Save data to disk. Creates directories as needed.
    pub fn save<T: Serialize>(&self, data: &HashMap<String, T>) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&self.path, content)
    }
}

/// A configuration entry (plain key-value data, no encryption).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    /// Key-value data
    pub data: HashMap<String, String>,
    /// Whether this config can be modified after creation
    pub immutable: bool,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last update timestamp
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory config store backed by JSON snapshots.
pub struct ConfigStore {
    configs: HashMap<String, ConfigEntry>,
    store: JsonStore,
}

impl ConfigStore {
    /// Create a new config store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "configs");
        let configs = store.load();
        debug!(count = configs.len(), "loaded configs from disk");
        Self { configs, store }
    }

    /// Create a new configuration. Fails if name already exists.
    pub fn create(
        &mut self,
        name: String,
        data: HashMap<String, String>,
        immutable: bool,
    ) -> Result<(), String> {
        if self.configs.contains_key(&name) {
            return Err(format!("config '{name}' already exists"));
        }
        let now = chrono::Utc::now();
        self.configs.insert(
            name,
            ConfigEntry {
                data,
                immutable,
                created_at: now,
                updated_at: now,
            },
        );
        self.snapshot();
        Ok(())
    }

    /// Get a configuration by name.
    pub fn get(&self, name: &str) -> Option<&ConfigEntry> {
        self.configs.get(name)
    }

    /// Update a configuration. Fails if immutable or not found.
    pub fn update(&mut self, name: &str, data: HashMap<String, String>) -> Result<(), String> {
        let entry = self
            .configs
            .get_mut(name)
            .ok_or_else(|| format!("config '{name}' not found"))?;
        if entry.immutable {
            return Err(format!("config '{name}' is immutable"));
        }
        entry.data = data;
        entry.updated_at = chrono::Utc::now();
        self.snapshot();
        Ok(())
    }

    /// Delete a configuration. Fails if not found.
    pub fn delete(&mut self, name: &str) -> Result<(), String> {
        if self.configs.remove(name).is_none() {
            return Err(format!("config '{name}' not found"));
        }
        self.snapshot();
        Ok(())
    }

    /// List all configuration names with metadata.
    pub fn list(&self, prefix: Option<&str>) -> Vec<(&str, &ConfigEntry)> {
        self.configs
            .iter()
            .filter(|(k, _)| prefix.is_none() || k.starts_with(prefix.unwrap_or("")))
            .map(|(k, v)| (k.as_str(), v))
            .collect()
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.configs) {
            warn!(error = %e, "failed to snapshot config store");
        }
    }
}

/// Alert rule for metrics-based alerting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    /// Unique alert name
    pub name: String,
    /// Metric name to evaluate
    pub metric: String,
    /// Condition: "above" or "below"
    pub condition: String,
    /// Threshold value
    pub threshold: f64,
    /// Current state: "ok", "firing", "acknowledged"
    pub state: String,
    /// When the alert was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the state last changed
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

    /// Create a new alert rule. Fails if name already exists.
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
// Job Store (Tier 4 — always available)
// ─────────────────────────────────────────────────────────────

/// A batch job entry tracking completion state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobEntry {
    pub name: String,
    pub image: String,
    pub command: Vec<String>,
    pub completions: u32,
    pub completed: u32,
    pub failed: u32,
    pub parallelism: u32,
    pub backoff_limit: u32,
    pub container_ids: Vec<String>,
    pub state: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// In-memory job store backed by JSON snapshots.
pub struct JobStore {
    jobs: HashMap<String, JobEntry>,
    store: JsonStore,
}

impl JobStore {
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "jobs");
        let jobs = store.load();
        debug!(count = jobs.len(), "loaded jobs from disk");
        Self { jobs, store }
    }

    pub fn create(&mut self, entry: JobEntry) -> Result<(), String> {
        if self.jobs.contains_key(&entry.name) {
            return Err(format!("job '{}' already exists", entry.name));
        }
        let name = entry.name.clone();
        self.jobs.insert(name, entry);
        self.snapshot();
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&JobEntry> {
        self.jobs.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut JobEntry> {
        self.jobs.get_mut(name)
    }

    pub fn delete(&mut self, name: &str) -> Result<JobEntry, String> {
        self.jobs
            .remove(name)
            .ok_or_else(|| format!("job '{name}' not found"))
            .inspect(|_| self.snapshot())
    }

    pub fn list(&self) -> Vec<&JobEntry> {
        self.jobs.values().collect()
    }

    pub fn update(&mut self) {
        self.snapshot();
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.jobs) {
            warn!(error = %e, "failed to snapshot job store");
        }
    }
}

/// A cron job entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronEntry {
    pub name: String,
    pub schedule: String,
    pub image: String,
    pub command: Vec<String>,
    pub suspended: bool,
    pub last_run: Option<chrono::DateTime<chrono::Utc>>,
    pub next_run: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory cron store backed by JSON snapshots.
pub struct CronStore {
    crons: HashMap<String, CronEntry>,
    store: JsonStore,
}

impl CronStore {
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "crons");
        let crons = store.load();
        debug!(count = crons.len(), "loaded crons from disk");
        Self { crons, store }
    }

    pub fn create(&mut self, entry: CronEntry) -> Result<(), String> {
        if self.crons.contains_key(&entry.name) {
            return Err(format!("cron '{}' already exists", entry.name));
        }
        let name = entry.name.clone();
        self.crons.insert(name, entry);
        self.snapshot();
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&CronEntry> {
        self.crons.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut CronEntry> {
        self.crons.get_mut(name)
    }

    pub fn list(&self) -> Vec<&CronEntry> {
        self.crons.values().collect()
    }

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
// Namespace Store (Tier 8 — always available)
// ─────────────────────────────────────────────────────────────

/// Resource quota for a namespace.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceQuota {
    pub max_cpu: Option<f64>,
    pub max_memory_mb: Option<u64>,
    pub max_gpus: Option<u32>,
    pub max_storage_gb: Option<u64>,
}

/// A namespace entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceEntry {
    pub name: String,
    pub quotas: ResourceQuota,
    pub labels: HashMap<String, String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// A taint applied to a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintEntry {
    pub key: String,
    pub value: String,
    pub effect: String,
}

/// In-memory namespace store backed by JSON snapshots.
pub struct NamespaceStore {
    namespaces: HashMap<String, NamespaceEntry>,
    pub taints: Vec<TaintEntry>,
    store: JsonStore,
}

impl NamespaceStore {
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

    pub fn create(&mut self, entry: NamespaceEntry) -> Result<(), String> {
        if self.namespaces.contains_key(&entry.name) {
            return Err(format!("namespace '{}' already exists", entry.name));
        }
        let name = entry.name.clone();
        self.namespaces.insert(name, entry);
        self.snapshot();
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&NamespaceEntry> {
        self.namespaces.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut NamespaceEntry> {
        self.namespaces.get_mut(name)
    }

    pub fn list(&self) -> Vec<&NamespaceEntry> {
        self.namespaces.values().collect()
    }

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
// Audit Store (Tier 7 — auth feature)
// ─────────────────────────────────────────────────────────────

/// An audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub principal: String,
    pub action: String,
    pub resource: String,
    pub result: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// In-memory audit store backed by JSON snapshots.
pub struct AuditStore {
    entries: Vec<AuditEntry>,
    store: JsonStore,
}

impl AuditStore {
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "audit");
        // Load as a map keyed by index
        let map: HashMap<String, AuditEntry> = store.load();
        let mut entries: Vec<_> = map.into_values().collect();
        entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        debug!(count = entries.len(), "loaded audit entries from disk");
        Self { entries, store }
    }

    pub fn record(&mut self, entry: AuditEntry) {
        self.entries.push(entry);
        self.snapshot();
    }

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
// Policy Store (Tier 11 — always available)
// ─────────────────────────────────────────────────────────────

/// A policy rule for admission control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEntry {
    pub name: String,
    pub policy_type: String,
    pub rules: Vec<serde_json::Value>,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory policy store backed by JSON snapshots.
pub struct PolicyStore {
    policies: HashMap<String, PolicyEntry>,
    store: JsonStore,
}

impl PolicyStore {
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "policies");
        let policies = store.load();
        debug!(count = policies.len(), "loaded policies from disk");
        Self { policies, store }
    }

    pub fn create(&mut self, entry: PolicyEntry) -> Result<(), String> {
        if self.policies.contains_key(&entry.name) {
            return Err(format!("policy '{}' already exists", entry.name));
        }
        let name = entry.name.clone();
        self.policies.insert(name, entry);
        self.snapshot();
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&PolicyEntry> {
        self.policies.get(name)
    }

    pub fn list(&self) -> Vec<&PolicyEntry> {
        self.policies.values().collect()
    }

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

// ─────────────────────────────────────────────────────────────
// Service Store (Tier 5 — network feature)
// ─────────────────────────────────────────────────────────────

/// A service entry for service discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEntry {
    pub name: String,
    pub selector: HashMap<String, String>,
    pub port: u16,
    pub protocol: String,
    pub endpoints: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// An ingress routing rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngressRule {
    pub host: String,
    pub path: String,
    pub service: String,
}

/// An ingress entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngressEntry {
    pub name: String,
    pub rules: Vec<IngressRule>,
    pub tls: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// A network policy entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicyEntry {
    pub name: String,
    pub selector: HashMap<String, String>,
    pub ingress_rules: Vec<serde_json::Value>,
    pub egress_rules: Vec<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory service store with services, ingresses, and network policies.
pub struct ServiceStore {
    services: HashMap<String, ServiceEntry>,
    ingresses: HashMap<String, IngressEntry>,
    policies: HashMap<String, NetworkPolicyEntry>,
    service_store: JsonStore,
    ingress_store: JsonStore,
    policy_store: JsonStore,
}

impl ServiceStore {
    pub fn new(state_path: &Path) -> Self {
        let service_store = JsonStore::new(state_path, "services");
        let ingress_store = JsonStore::new(state_path, "ingresses");
        let policy_store = JsonStore::new(state_path, "network_policies");
        Self {
            services: service_store.load(),
            ingresses: ingress_store.load(),
            policies: policy_store.load(),
            service_store,
            ingress_store,
            policy_store,
        }
    }

    pub fn create_service(&mut self, entry: ServiceEntry) -> Result<(), String> {
        if self.services.contains_key(&entry.name) {
            return Err(format!("service '{}' already exists", entry.name));
        }
        let name = entry.name.clone();
        self.services.insert(name, entry);
        self.snapshot_services();
        Ok(())
    }

    pub fn get_service(&self, name: &str) -> Option<&ServiceEntry> {
        self.services.get(name)
    }

    pub fn delete_service(&mut self, name: &str) -> Result<(), String> {
        self.services
            .remove(name)
            .ok_or_else(|| format!("service '{name}' not found"))?;
        self.snapshot_services();
        Ok(())
    }

    pub fn list_services(&self) -> Vec<&ServiceEntry> {
        self.services.values().collect()
    }

    pub fn create_ingress(&mut self, entry: IngressEntry) -> Result<(), String> {
        if self.ingresses.contains_key(&entry.name) {
            return Err(format!("ingress '{}' already exists", entry.name));
        }
        let name = entry.name.clone();
        self.ingresses.insert(name, entry);
        self.snapshot_ingresses();
        Ok(())
    }

    pub fn delete_ingress(&mut self, name: &str) -> Result<(), String> {
        self.ingresses
            .remove(name)
            .ok_or_else(|| format!("ingress '{name}' not found"))?;
        self.snapshot_ingresses();
        Ok(())
    }

    pub fn create_network_policy(&mut self, entry: NetworkPolicyEntry) -> Result<(), String> {
        if self.policies.contains_key(&entry.name) {
            return Err(format!("network policy '{}' already exists", entry.name));
        }
        let name = entry.name.clone();
        self.policies.insert(name, entry);
        self.snapshot_policies();
        Ok(())
    }

    pub fn delete_network_policy(&mut self, name: &str) -> Result<(), String> {
        self.policies
            .remove(name)
            .ok_or_else(|| format!("network policy '{name}' not found"))?;
        self.snapshot_policies();
        Ok(())
    }

    pub fn list_network_policies(&self) -> Vec<&NetworkPolicyEntry> {
        self.policies.values().collect()
    }

    fn snapshot_services(&self) {
        if let Err(e) = self.service_store.save(&self.services) {
            warn!(error = %e, "failed to snapshot service store");
        }
    }

    fn snapshot_ingresses(&self) {
        if let Err(e) = self.ingress_store.save(&self.ingresses) {
            warn!(error = %e, "failed to snapshot ingress store");
        }
    }

    fn snapshot_policies(&self) {
        if let Err(e) = self.policy_store.save(&self.policies) {
            warn!(error = %e, "failed to snapshot network policy store");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Backup Store (Tier 6 — storage feature)
// ─────────────────────────────────────────────────────────────

/// A backup entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEntry {
    pub id: String,
    pub scope: String,
    pub destination: String,
    pub state: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory backup store backed by JSON snapshots.
pub struct BackupStore {
    backups: HashMap<String, BackupEntry>,
    store: JsonStore,
}

impl BackupStore {
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "backups");
        let backups = store.load();
        Self { backups, store }
    }

    pub fn create(&mut self, entry: BackupEntry) -> Result<(), String> {
        if self.backups.contains_key(&entry.id) {
            return Err(format!("backup '{}' already exists", entry.id));
        }
        let id = entry.id.clone();
        self.backups.insert(id, entry);
        self.snapshot();
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&BackupEntry> {
        self.backups.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut BackupEntry> {
        self.backups.get_mut(id)
    }

    pub fn list(&self) -> Vec<&BackupEntry> {
        self.backups.values().collect()
    }

    pub fn update(&mut self) {
        self.snapshot();
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.backups) {
            warn!(error = %e, "failed to snapshot backup store");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Workload Store
// ─────────────────────────────────────────────────────────────

/// A persistent workload record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadRecord {
    /// Workload ID (UUID).
    pub id: String,
    /// Container image.
    pub image: String,
    /// Docker container ID (if running).
    pub container_id: Option<String>,
    /// Allocated GPU indices.
    pub gpu_ids: Vec<u32>,
    /// Current state: "running", "stopped", "failed", "exited".
    pub state: String,
    /// Container name (if assigned).
    pub name: Option<String>,
    /// Environment variables.
    pub env: Vec<String>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last state change timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Exit code (if exited).
    pub exit_code: Option<i32>,
}

/// In-memory workload store backed by JSON snapshots.
pub struct WorkloadStore {
    workloads: HashMap<String, WorkloadRecord>,
    store: JsonStore,
}

impl WorkloadStore {
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "workloads");
        let workloads = store.load();
        debug!(count = workloads.len(), "loaded workloads from disk");
        Self { workloads, store }
    }

    pub fn upsert(&mut self, record: WorkloadRecord) {
        self.workloads.insert(record.id.clone(), record);
        self.snapshot();
    }

    pub fn get(&self, id: &str) -> Option<&WorkloadRecord> {
        self.workloads.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut WorkloadRecord> {
        self.workloads.get_mut(id)
    }

    pub fn update_state(&mut self, id: &str, state: &str, exit_code: Option<i32>) {
        if let Some(record) = self.workloads.get_mut(id) {
            record.state = state.to_string();
            record.exit_code = exit_code;
            record.updated_at = chrono::Utc::now();
            self.snapshot();
        }
    }

    pub fn remove(&mut self, id: &str) -> Option<WorkloadRecord> {
        let record = self.workloads.remove(id);
        if record.is_some() {
            self.snapshot();
        }
        record
    }

    pub fn list(&self) -> Vec<&WorkloadRecord> {
        self.workloads.values().collect()
    }

    /// Get all workloads that were in "running" state (for reconciliation on startup).
    pub fn running(&self) -> Vec<&WorkloadRecord> {
        self.workloads
            .values()
            .filter(|w| w.state == "running")
            .collect()
    }

    /// Find a workload by its Docker container ID.
    pub fn find_by_container_id(&self, container_id: &str) -> Option<&WorkloadRecord> {
        self.workloads
            .values()
            .find(|w| w.container_id.as_deref() == Some(container_id))
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.workloads) {
            warn!(error = %e, "failed to snapshot workload store");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Deploy Store
// ─────────────────────────────────────────────────────────────

/// A deployment record tracking rolling updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRecord {
    /// Deployment name.
    pub name: String,
    /// Current image.
    pub image: String,
    /// Previous image (for rollback).
    pub previous_image: Option<String>,
    /// Target replica count.
    pub replicas: u32,
    /// Active container IDs.
    pub container_ids: Vec<String>,
    /// GPU count per replica.
    pub gpus_per_replica: u32,
    /// Memory limit per replica.
    pub memory: Option<String>,
    /// CPU limit per replica.
    pub cpu: Option<f32>,
    /// Deploy strategy: "rolling", "blue-green", "immediate".
    pub strategy: String,
    /// Current state: "active", "updating", "paused", "failed", "deleted".
    pub state: String,
    /// Revision number (increments on each update).
    pub revision: u32,
    /// History of previous revisions.
    pub history: Vec<DeployRevision>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last update timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// A historical deployment revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRevision {
    pub revision: u32,
    pub image: String,
    pub replicas: u32,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub reason: Option<String>,
}

/// In-memory deploy store backed by JSON snapshots.
pub struct DeployStore {
    deploys: HashMap<String, DeployRecord>,
    store: JsonStore,
}

impl DeployStore {
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "deploys");
        let deploys = store.load();
        debug!(count = deploys.len(), "loaded deploys from disk");
        Self { deploys, store }
    }

    pub fn create(&mut self, record: DeployRecord) -> Result<(), String> {
        if self.deploys.contains_key(&record.name) {
            return Err(format!("deployment '{}' already exists", record.name));
        }
        let name = record.name.clone();
        self.deploys.insert(name, record);
        self.snapshot();
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&DeployRecord> {
        self.deploys.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut DeployRecord> {
        self.deploys.get_mut(name)
    }

    pub fn update(&mut self, name: &str) {
        // Just snapshot after external mutation via get_mut
        if self.deploys.contains_key(name) {
            self.snapshot();
        }
    }

    pub fn delete(&mut self, name: &str) -> Option<DeployRecord> {
        let record = self.deploys.remove(name);
        if record.is_some() {
            self.snapshot();
        }
        record
    }

    pub fn list(&self) -> Vec<&DeployRecord> {
        self.deploys.values().collect()
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.deploys) {
            warn!(error = %e, "failed to snapshot deploy store");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Secret Store (encrypted at rest)
// ─────────────────────────────────────────────────────────────

/// An encrypted secret entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretEntry {
    /// Secret name.
    pub name: String,
    /// Encrypted data (base64-encoded ciphertext).
    pub encrypted_data: String,
    /// Nonce used for encryption (base64-encoded).
    pub nonce: String,
    /// Key version used for encryption.
    pub key_version: u32,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last rotation timestamp.
    pub rotated_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory secret store backed by encrypted JSON snapshots.
pub struct SecretStore {
    secrets: HashMap<String, SecretEntry>,
    store: JsonStore,
}

impl SecretStore {
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "secrets");
        let secrets = store.load();
        debug!(count = secrets.len(), "loaded secrets from disk");
        Self { secrets, store }
    }

    pub fn create(&mut self, entry: SecretEntry) -> Result<(), String> {
        if self.secrets.contains_key(&entry.name) {
            return Err(format!("secret '{}' already exists", entry.name));
        }
        let name = entry.name.clone();
        self.secrets.insert(name, entry);
        self.snapshot();
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&SecretEntry> {
        self.secrets.get(name)
    }

    pub fn update(&mut self, name: &str, entry: SecretEntry) -> Result<(), String> {
        if !self.secrets.contains_key(name) {
            return Err(format!("secret '{name}' not found"));
        }
        self.secrets.insert(name.to_string(), entry);
        self.snapshot();
        Ok(())
    }

    pub fn delete(&mut self, name: &str) -> Option<SecretEntry> {
        let entry = self.secrets.remove(name);
        if entry.is_some() {
            self.snapshot();
        }
        entry
    }

    pub fn list(&self) -> Vec<&SecretEntry> {
        self.secrets.values().collect()
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.secrets) {
            warn!(error = %e, "failed to snapshot secret store");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Volume Store
// ─────────────────────────────────────────────────────────────

/// A persistent volume record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeRecord {
    pub name: String,
    /// "emptydir", "hostpath", "nfs", "pvc"
    pub volume_type: String,
    /// Host path (for hostpath type).
    pub host_path: Option<String>,
    /// Size limit (e.g., "10Gi").
    pub size: Option<String>,
    /// State: "available", "bound", "released".
    pub state: String,
    /// Container ID this volume is mounted to (if bound).
    pub bound_to: Option<String>,
    /// Mount path inside the container.
    pub mount_path: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct VolumeStore {
    volumes: HashMap<String, VolumeRecord>,
    store: JsonStore,
}

impl VolumeStore {
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "volumes");
        let volumes = store.load();
        debug!(count = volumes.len(), "loaded volumes from disk");
        Self { volumes, store }
    }

    pub fn create(&mut self, record: VolumeRecord) -> Result<(), String> {
        if self.volumes.contains_key(&record.name) {
            return Err(format!("volume '{}' already exists", record.name));
        }
        let name = record.name.clone();
        self.volumes.insert(name, record);
        self.snapshot();
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&VolumeRecord> {
        self.volumes.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut VolumeRecord> {
        self.volumes.get_mut(name)
    }

    pub fn update(&mut self, name: &str) {
        if self.volumes.contains_key(name) {
            self.snapshot();
        }
    }

    pub fn delete(&mut self, name: &str) -> Option<VolumeRecord> {
        let r = self.volumes.remove(name);
        if r.is_some() {
            self.snapshot();
        }
        r
    }

    pub fn list(&self) -> Vec<&VolumeRecord> {
        self.volumes.values().collect()
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.volumes) {
            warn!(error = %e, "failed to snapshot volume store");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// API Key Store
// ─────────────────────────────────────────────────────────────

/// An API key record for RBAC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    /// Key ID (public identifier).
    pub key_id: String,
    /// Key name / description.
    pub name: String,
    /// Hashed secret (SHA-256 hex).
    pub secret_hash: String,
    /// Scopes granted to this key.
    pub scopes: Vec<String>,
    /// Role: "admin", "operator", "viewer".
    pub role: String,
    /// Whether the key is active.
    pub active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct ApiKeyStore {
    keys: HashMap<String, ApiKeyRecord>,
    store: JsonStore,
}

impl ApiKeyStore {
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "apikeys");
        let keys = store.load();
        debug!(count = keys.len(), "loaded API keys from disk");
        Self { keys, store }
    }

    pub fn create(&mut self, record: ApiKeyRecord) -> Result<(), String> {
        if self.keys.contains_key(&record.key_id) {
            return Err(format!("key '{}' already exists", record.key_id));
        }
        let id = record.key_id.clone();
        self.keys.insert(id, record);
        self.snapshot();
        Ok(())
    }

    pub fn get(&self, key_id: &str) -> Option<&ApiKeyRecord> {
        self.keys.get(key_id)
    }

    pub fn get_mut(&mut self, key_id: &str) -> Option<&mut ApiKeyRecord> {
        self.keys.get_mut(key_id)
    }

    pub fn find_by_hash(&self, secret_hash: &str) -> Option<&ApiKeyRecord> {
        self.keys.values().find(|k| k.secret_hash == secret_hash && k.active)
    }

    pub fn revoke(&mut self, key_id: &str) -> Result<(), String> {
        let key = self.keys.get_mut(key_id).ok_or_else(|| format!("key '{key_id}' not found"))?;
        key.active = false;
        self.snapshot();
        Ok(())
    }

    pub fn delete(&mut self, key_id: &str) -> Option<ApiKeyRecord> {
        let r = self.keys.remove(key_id);
        if r.is_some() {
            self.snapshot();
        }
        r
    }

    pub fn list(&self) -> Vec<&ApiKeyRecord> {
        self.keys.values().collect()
    }

    pub fn update(&mut self, key_id: &str) {
        if self.keys.contains_key(key_id) {
            self.snapshot();
        }
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.keys) {
            warn!(error = %e, "failed to snapshot API key store");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Autoscale Store
// ─────────────────────────────────────────────────────────────

/// An autoscaling policy record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoscaleRecord {
    pub name: String,
    /// Target deployment or workload name.
    pub target: String,
    /// Min replicas.
    pub min_replicas: u32,
    /// Max replicas.
    pub max_replicas: u32,
    /// Current replicas.
    pub current_replicas: u32,
    /// Policy type: "target_utilization", "queue_depth", "schedule".
    pub policy_type: String,
    /// Target metric (e.g., "gpu_utilization", "cpu_percent").
    pub metric: Option<String>,
    /// Target threshold value.
    pub threshold: Option<f64>,
    /// State: "active", "disabled".
    pub state: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub struct AutoscaleStore {
    policies: HashMap<String, AutoscaleRecord>,
    store: JsonStore,
}

impl AutoscaleStore {
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "autoscale");
        let policies = store.load();
        debug!(count = policies.len(), "loaded autoscale policies from disk");
        Self { policies, store }
    }

    pub fn create(&mut self, record: AutoscaleRecord) -> Result<(), String> {
        if self.policies.contains_key(&record.name) {
            return Err(format!("policy '{}' already exists", record.name));
        }
        let name = record.name.clone();
        self.policies.insert(name, record);
        self.snapshot();
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&AutoscaleRecord> {
        self.policies.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut AutoscaleRecord> {
        self.policies.get_mut(name)
    }

    pub fn update(&mut self, name: &str) {
        if self.policies.contains_key(name) {
            self.snapshot();
        }
    }

    pub fn delete(&mut self, name: &str) -> Option<AutoscaleRecord> {
        let r = self.policies.remove(name);
        if r.is_some() {
            self.snapshot();
        }
        r
    }

    pub fn list(&self) -> Vec<&AutoscaleRecord> {
        self.policies.values().collect()
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.policies) {
            warn!(error = %e, "failed to snapshot autoscale store");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Audit Log Store
// ─────────────────────────────────────────────────────────────

/// An audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub actor: String,
    pub action: String,
    pub resource: String,
    pub resource_id: Option<String>,
    pub result: String,
    pub details: Option<String>,
}

pub struct AuditLogStore {
    entries: HashMap<String, AuditLogEntry>,
    store: JsonStore,
}

impl AuditLogStore {
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "audit_log");
        let entries = store.load();
        debug!(count = entries.len(), "loaded audit log from disk");
        Self { entries, store }
    }

    pub fn append(&mut self, entry: AuditLogEntry) {
        self.entries.insert(entry.id.clone(), entry);
        self.snapshot();
    }

    pub fn query(&self, actor: Option<&str>, action: Option<&str>, limit: usize) -> Vec<&AuditLogEntry> {
        let mut results: Vec<_> = self.entries.values()
            .filter(|e| actor.map_or(true, |a| e.actor == a))
            .filter(|e| action.map_or(true, |a| e.action == a))
            .collect();
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        results.truncate(limit);
        results
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.entries) {
            warn!(error = %e, "failed to snapshot audit log");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_store_crud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = ConfigStore::new(dir.path());

        // Create
        let mut data = HashMap::new();
        data.insert("key1".to_string(), "value1".to_string());
        store.create("test-config".to_string(), data.clone(), false).expect("create");

        // Get
        let entry = store.get("test-config").expect("get");
        assert_eq!(entry.data.get("key1").unwrap(), "value1");
        assert!(!entry.immutable);

        // Update
        let mut new_data = HashMap::new();
        new_data.insert("key1".to_string(), "updated".to_string());
        store.update("test-config", new_data).expect("update");
        let entry = store.get("test-config").expect("get after update");
        assert_eq!(entry.data.get("key1").unwrap(), "updated");

        // List
        let list = store.list(None);
        assert_eq!(list.len(), 1);

        // Delete
        store.delete("test-config").expect("delete");
        assert!(store.get("test-config").is_none());
    }

    #[test]
    fn test_config_store_immutable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = ConfigStore::new(dir.path());

        let mut data = HashMap::new();
        data.insert("key".to_string(), "val".to_string());
        store.create("immutable-cfg".to_string(), data, true).expect("create");

        let result = store.update("immutable-cfg", HashMap::new());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("immutable"));
    }

    #[test]
    fn test_config_store_duplicate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = ConfigStore::new(dir.path());

        store.create("dup".to_string(), HashMap::new(), false).expect("create");
        let result = store.create("dup".to_string(), HashMap::new(), false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
    }

    #[test]
    fn test_config_store_persistence() {
        let dir = tempfile::tempdir().expect("tempdir");

        // Create and populate
        {
            let mut store = ConfigStore::new(dir.path());
            let mut data = HashMap::new();
            data.insert("db_host".to_string(), "localhost".to_string());
            store.create("db-config".to_string(), data, false).expect("create");
        }

        // Reload and verify
        {
            let store = ConfigStore::new(dir.path());
            let entry = store.get("db-config").expect("get after reload");
            assert_eq!(entry.data.get("db_host").unwrap(), "localhost");
        }
    }

    #[test]
    fn test_config_store_prefix_filter() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = ConfigStore::new(dir.path());

        store.create("app.db".to_string(), HashMap::new(), false).expect("create");
        store.create("app.cache".to_string(), HashMap::new(), false).expect("create");
        store.create("sys.network".to_string(), HashMap::new(), false).expect("create");

        let app_configs = store.list(Some("app."));
        assert_eq!(app_configs.len(), 2);

        let sys_configs = store.list(Some("sys."));
        assert_eq!(sys_configs.len(), 1);

        let all = store.list(None);
        assert_eq!(all.len(), 3);
    }

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

        let alert = store.get("high-cpu").expect("get");
        assert_eq!(alert.threshold, 90.0);
        assert_eq!(alert.state, "ok");

        let list = store.list();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_alert_evaluate() {
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

        // Below threshold — stays ok
        let alert = store.evaluate("high-cpu", 50.0).expect("evaluate");
        assert_eq!(alert.state, "ok");

        // Above threshold — fires
        let alert = store.evaluate("high-cpu", 95.0).expect("evaluate");
        assert_eq!(alert.state, "firing");

        // Drops below — resolves
        let alert = store.evaluate("high-cpu", 80.0).expect("evaluate");
        assert_eq!(alert.state, "ok");
    }

    #[test]
    fn test_alert_acknowledge() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = AlertStore::new(dir.path());

        let rule = AlertRule {
            name: "test-alert".to_string(),
            metric: "mem.usage".to_string(),
            condition: "above".to_string(),
            threshold: 80.0,
            state: "ok".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        store.create(rule).expect("create");

        // Can't ack a non-firing alert
        let result = store.acknowledge("test-alert");
        assert!(result.is_err());

        // Fire it
        store.evaluate("test-alert", 95.0);

        // Now ack works
        store.acknowledge("test-alert").expect("ack");
        let alert = store.get("test-alert").expect("get");
        assert_eq!(alert.state, "acknowledged");
    }

    #[test]
    fn test_alert_persistence() {
        let dir = tempfile::tempdir().expect("tempdir");

        {
            let mut store = AlertStore::new(dir.path());
            let rule = AlertRule {
                name: "persist-test".to_string(),
                metric: "gpu.temp".to_string(),
                condition: "above".to_string(),
                threshold: 85.0,
                state: "ok".to_string(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            store.create(rule).expect("create");
            store.evaluate("persist-test", 90.0);
        }

        {
            let store = AlertStore::new(dir.path());
            let alert = store.get("persist-test").expect("get after reload");
            assert_eq!(alert.state, "firing");
        }
    }
}
