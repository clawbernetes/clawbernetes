//! Clawnode - GPU Node Agent for Clawbernetes
//!
//! This agent runs on GPU servers and connects back to the OpenClaw gateway,
//! registering as a node with GPU capabilities.

#![forbid(unsafe_code)]

pub mod client;
pub mod commands;
pub mod config;
pub mod config_cmd;
pub mod deploy_cmd;
#[cfg(feature = "docker")]
pub mod docker;
pub mod error;
pub mod gpu;
pub mod handlers;
pub mod identity;
pub mod job_cmd;
#[cfg(feature = "metrics")]
pub mod metrics_cmd;
#[cfg(feature = "molt")]
pub mod molt_cmd;
pub mod namespace_cmd;
pub mod network_types;
#[cfg(feature = "network")]
pub mod ingress_proxy;
#[cfg(feature = "network")]
pub mod mesh;
#[cfg(feature = "network")]
pub mod netpolicy;
#[cfg(feature = "network")]
pub mod network_cmd;
#[cfg(feature = "network")]
pub mod service_discovery;
#[cfg(feature = "network")]
pub mod workload_net;
pub mod persist;
pub mod policy_cmd;
pub mod runtime;
pub mod secrets_cmd;
pub mod state;
pub mod storage_cmd;
pub mod auth_cmd;
pub mod autoscale_cmd;

use std::sync::Arc;
use tokio::sync::RwLock;

pub use client::GatewayClient;
pub use config::NodeConfig;
pub use gpu::GpuManager;

/// Node state shared across components
#[derive(Debug)]
pub struct NodeState {
    pub config: NodeConfig,
    pub gpu_manager: GpuManager,
    pub connected: bool,
    pub node_id: Option<String>,
    pub node_token: Option<String>,
    pub approved: bool,
    pub capabilities: Vec<String>,
    pub commands: Vec<String>,
}

impl NodeState {
    pub fn new(config: NodeConfig) -> Self {
        let gpu_manager = GpuManager::new();

        // Build capabilities based on detected features
        let mut capabilities = vec!["system".to_string()];

        if gpu_manager.count() > 0 {
            capabilities.push("gpu".to_string());
            capabilities.push("nvidia".to_string());
        }

        // Check for container runtimes
        if std::process::Command::new("docker")
            .arg("--version")
            .output()
            .is_ok()
        {
            capabilities.push("docker".to_string());
            capabilities.push("container".to_string());
        } else if std::process::Command::new("podman")
            .arg("--version")
            .output()
            .is_ok()
        {
            capabilities.push("podman".to_string());
            capabilities.push("container".to_string());
        }

        // List of commands this node supports
        let commands = vec![
            // Tier 0 — Core (always)
            "system.info".to_string(),
            "system.run".to_string(),
            "system.which".to_string(),
            "gpu.list".to_string(),
            "gpu.metrics".to_string(),
            "workload.run".to_string(),
            "workload.stop".to_string(),
            "workload.logs".to_string(),
            "workload.list".to_string(),
            "workload.inspect".to_string(),
            "workload.stats".to_string(),
            "container.exec".to_string(),
            "node.capabilities".to_string(),
            "node.health".to_string(),
            // Config commands (always available)
            "config.create".to_string(),
            "config.get".to_string(),
            "config.update".to_string(),
            "config.delete".to_string(),
            "config.list".to_string(),
            // Tier 4 — Jobs & Cron (always)
            "job.create".to_string(),
            "job.status".to_string(),
            "job.logs".to_string(),
            "job.delete".to_string(),
            "cron.create".to_string(),
            "cron.list".to_string(),
            "cron.trigger".to_string(),
            "cron.suspend".to_string(),
            "cron.resume".to_string(),
            // Tier 8 — Namespaces (always)
            "namespace.create".to_string(),
            "namespace.set_quota".to_string(),
            "namespace.usage".to_string(),
            "namespace.list".to_string(),
            "node.label".to_string(),
            "node.taint".to_string(),
            "node.drain".to_string(),
            // Tier 11 — Policy (always)
            "policy.create".to_string(),
            "policy.validate".to_string(),
            "policy.list".to_string(),
        ];

        Self {
            config,
            gpu_manager,
            connected: false,
            node_id: None,
            node_token: None,
            approved: false,
            capabilities,
            commands,
        }
    }
}

/// Shared state type - allows interior mutability from client
pub struct SharedState {
    inner: Arc<RwLock<NodeState>>,
    pub capabilities: Vec<String>,
    pub commands: Vec<String>,
    pub node_token: Option<String>,
    /// Docker SDK runtime (when `docker` feature is enabled)
    #[cfg(feature = "docker")]
    pub docker_runtime: Option<docker::DockerContainerRuntime>,
    /// Workload store (persistent workload tracking)
    pub workload_store: Arc<RwLock<persist::WorkloadStore>>,
    /// Deploy store (deployment history & state)
    pub deploy_store: Arc<RwLock<persist::DeployStore>>,
    /// Secret store (encrypted at rest)
    pub secret_store: Arc<RwLock<persist::SecretStore>>,
    /// Config store (always available)
    pub config_store: Arc<RwLock<persist::ConfigStore>>,
    /// Metric store (when `metrics` feature is enabled)
    #[cfg(feature = "metrics")]
    pub metric_store: Arc<claw_metrics::MetricStore>,
    /// Alert store (when `metrics` feature is enabled)
    #[cfg(feature = "metrics")]
    pub alert_store: Arc<RwLock<persist::AlertStore>>,
    // ─── Tier 4: Jobs & Cron (always) ───
    pub job_store: Arc<RwLock<persist::JobStore>>,
    pub cron_store: Arc<RwLock<persist::CronStore>>,
    // ─── Tier 5: Networking (network feature) ───
    #[cfg(feature = "network")]
    pub wireguard_mesh: Arc<network_types::WireGuardMesh>,
    #[cfg(feature = "network")]
    pub service_store: Arc<RwLock<persist::ServiceStore>>,
    #[cfg(feature = "network")]
    pub mesh_manager: Arc<RwLock<Option<mesh::MeshManager>>>,
    #[cfg(feature = "network")]
    pub workload_net: Arc<RwLock<Option<workload_net::WorkloadNetManager>>>,
    #[cfg(feature = "network")]
    pub service_discovery: Arc<RwLock<Option<service_discovery::ServiceDiscovery>>>,
    #[cfg(feature = "network")]
    pub policy_engine: Arc<RwLock<Option<netpolicy::PolicyEngine>>>,
    #[cfg(feature = "network")]
    pub ingress_routes: ingress_proxy::RouteTable,
    // ─── Tier 6: Storage ───
    pub volume_store: Arc<RwLock<persist::VolumeStore>>,
    pub backup_store: Arc<RwLock<persist::BackupStore>>,
    // ─── Tier 7: Auth & RBAC ───
    pub api_key_store: Arc<RwLock<persist::ApiKeyStore>>,
    pub audit_log_store: Arc<RwLock<persist::AuditLogStore>>,
    // ─── Tier 8: Namespaces (always) ───
    pub namespace_store: Arc<RwLock<persist::NamespaceStore>>,
    // ─── Tier 9: Autoscaling ───
    pub autoscale_store: Arc<RwLock<persist::AutoscaleStore>>,
    // ─── Tier 10: MOLT (molt feature) ───
    #[cfg(feature = "molt")]
    pub molt_peer_table: Arc<RwLock<molt_p2p::PeerTable>>,
    #[cfg(feature = "molt")]
    pub molt_order_book: Arc<RwLock<molt_market::OrderBook>>,
    #[cfg(feature = "molt")]
    pub molt_wallet: Arc<RwLock<molt_core::Wallet>>,
    // ─── Tier 11: Policy (always) ───
    pub policy_store: Arc<RwLock<persist::PolicyStore>>,
}

impl SharedState {
    pub fn new(config: NodeConfig) -> Self {
        let state_path = config.state_path.clone();
        let state = NodeState::new(config);

        // Build the full command list based on enabled features
        let mut commands = state.commands.clone();

        commands.extend([
            "secret.create".to_string(),
            "secret.get".to_string(),
            "secret.delete".to_string(),
            "secret.list".to_string(),
            "secret.rotate".to_string(),
        ]);

        #[cfg(feature = "metrics")]
        {
            commands.extend([
                "metrics.query".to_string(),
                "metrics.list".to_string(),
                "metrics.snapshot".to_string(),
                "events.query".to_string(),
                "events.emit".to_string(),
                "alerts.create".to_string(),
                "alerts.list".to_string(),
                "alerts.acknowledge".to_string(),
            ]);
        }

        commands.extend([
            "deploy.create".to_string(),
            "deploy.status".to_string(),
            "deploy.update".to_string(),
            "deploy.rollback".to_string(),
            "deploy.history".to_string(),
            "deploy.promote".to_string(),
            "deploy.pause".to_string(),
            "deploy.delete".to_string(),
        ]);

        #[cfg(feature = "network")]
        {
            commands.extend([
                "service.create".to_string(),
                "service.get".to_string(),
                "service.delete".to_string(),
                "service.list".to_string(),
                "service.endpoints".to_string(),
                "ingress.create".to_string(),
                "ingress.delete".to_string(),
                "network.status".to_string(),
                "network.policy.create".to_string(),
                "network.policy.delete".to_string(),
                "network.policy.list".to_string(),
            ]);
        }

        commands.extend([
            "volume.create".to_string(),
            "volume.mount".to_string(),
            "volume.unmount".to_string(),
            "volume.snapshot".to_string(),
            "volume.list".to_string(),
            "volume.delete".to_string(),
            "backup.create".to_string(),
            "backup.restore".to_string(),
            "backup.list".to_string(),
        ]);

        commands.extend([
            "auth.create_key".to_string(),
            "auth.revoke_key".to_string(),
            "auth.list_keys".to_string(),
            "audit.query".to_string(),
        ]);

        commands.extend([
            "autoscale.create".to_string(),
            "autoscale.status".to_string(),
            "autoscale.adjust".to_string(),
            "autoscale.delete".to_string(),
        ]);

        #[cfg(feature = "molt")]
        {
            commands.extend([
                "molt.discover".to_string(),
                "molt.bid".to_string(),
                "molt.status".to_string(),
                "molt.balance".to_string(),
                "molt.reputation".to_string(),
            ]);
        }

        let capabilities = state.capabilities.clone();

        Self {
            inner: Arc::new(RwLock::new(state)),
            capabilities,
            commands,
            node_token: None,
            #[cfg(feature = "docker")]
            docker_runtime: None,
            workload_store: Arc::new(RwLock::new(persist::WorkloadStore::new(&state_path))),
            deploy_store: Arc::new(RwLock::new(persist::DeployStore::new(&state_path))),
            secret_store: Arc::new(RwLock::new(persist::SecretStore::new(&state_path))),
            config_store: Arc::new(RwLock::new(persist::ConfigStore::new(&state_path))),
            #[cfg(feature = "metrics")]
            metric_store: Arc::new(claw_metrics::MetricStore::new(
                std::time::Duration::from_secs(24 * 3600), // 24h retention
            )),
            #[cfg(feature = "metrics")]
            alert_store: Arc::new(RwLock::new(persist::AlertStore::new(&state_path))),
            // Tier 4: Jobs & Cron (always)
            job_store: Arc::new(RwLock::new(persist::JobStore::new(&state_path))),
            cron_store: Arc::new(RwLock::new(persist::CronStore::new(&state_path))),
            // Tier 5: Networking (network feature)
            #[cfg(feature = "network")]
            wireguard_mesh: Arc::new(
                network_types::WireGuardMesh::new(network_types::MeshConfig::default())
                    .expect("WireGuardMesh initialization"),
            ),
            #[cfg(feature = "network")]
            service_store: Arc::new(RwLock::new(persist::ServiceStore::new(&state_path))),
            #[cfg(feature = "network")]
            mesh_manager: Arc::new(RwLock::new(None)),
            #[cfg(feature = "network")]
            workload_net: Arc::new(RwLock::new(None)),
            #[cfg(feature = "network")]
            service_discovery: Arc::new(RwLock::new(None)),
            #[cfg(feature = "network")]
            policy_engine: Arc::new(RwLock::new(None)),
            #[cfg(feature = "network")]
            ingress_routes: Arc::new(RwLock::new(Vec::new())),
            // Tier 6: Storage
            volume_store: Arc::new(RwLock::new(persist::VolumeStore::new(&state_path))),
            backup_store: Arc::new(RwLock::new(persist::BackupStore::new(&state_path))),
            // Tier 7: Auth & RBAC
            api_key_store: Arc::new(RwLock::new(persist::ApiKeyStore::new(&state_path))),
            audit_log_store: Arc::new(RwLock::new(persist::AuditLogStore::new(&state_path))),
            // Tier 8: Namespaces (always)
            namespace_store: Arc::new(RwLock::new(persist::NamespaceStore::new(&state_path))),
            // Tier 9: Autoscaling
            autoscale_store: Arc::new(RwLock::new(persist::AutoscaleStore::new(&state_path))),
            // Tier 10: MOLT (molt feature)
            #[cfg(feature = "molt")]
            molt_peer_table: Arc::new(RwLock::new(molt_p2p::PeerTable::new())),
            #[cfg(feature = "molt")]
            molt_order_book: Arc::new(RwLock::new(molt_market::OrderBook::new())),
            #[cfg(feature = "molt")]
            molt_wallet: Arc::new(RwLock::new(molt_core::Wallet::new())),
            // Tier 11: Policy (always)
            policy_store: Arc::new(RwLock::new(persist::PolicyStore::new(&state_path))),
        }
    }

    /// Create shared state with Docker SDK runtime connected.
    #[cfg(feature = "docker")]
    pub fn with_docker(config: NodeConfig) -> Self {
        let mut shared = Self::new(config);
        match docker::DockerContainerRuntime::connect() {
            Ok(runtime) => {
                tracing::info!("Docker SDK connected");
                shared.docker_runtime = Some(runtime);
            }
            Err(e) => {
                tracing::warn!(error = %e, "Docker SDK unavailable, falling back to CLI");
            }
        }
        shared
    }

    pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, NodeState> {
        self.inner.read().await
    }

    pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, NodeState> {
        self.inner.write().await
    }
}

/// Create shared state from config
pub fn create_state(config: NodeConfig) -> SharedState {
    SharedState::new(config)
}

/// Reconcile persisted workload state with actual container runtime on startup.
///
/// Checks workloads marked as "running" in the store and verifies they still
/// exist in Docker/podman. Marks any missing containers as "exited".
pub async fn reconcile_workloads(state: &SharedState) {
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };

    let running = {
        let store = state.workload_store.read().await;
        store
            .running()
            .iter()
            .map(|w| (w.id.clone(), w.container_id.clone()))
            .collect::<Vec<_>>()
    };

    if running.is_empty() {
        return;
    }

    tracing::info!(count = running.len(), "reconciling persisted workloads");

    for (workload_id, container_id) in running {
        let Some(cid) = container_id else {
            // No container ID — mark as failed
            state
                .workload_store
                .write()
                .await
                .update_state(&workload_id, "failed", None);
            continue;
        };

        // Check if container still exists
        let output = std::process::Command::new(&runtime)
            .args(["inspect", "--format", "{{.State.Status}}", &cid])
            .output();

        match output {
            Ok(o) if o.status.success() => {
                let status = String::from_utf8_lossy(&o.stdout).trim().to_string();
                match status.as_str() {
                    "running" => {
                        tracing::debug!(workload_id = %workload_id, container = %cid, "still running");
                    }
                    "exited" | "dead" => {
                        tracing::info!(workload_id = %workload_id, container = %cid, status = %status, "container exited");
                        state
                            .workload_store
                            .write()
                            .await
                            .update_state(&workload_id, "exited", None);
                    }
                    other => {
                        tracing::info!(workload_id = %workload_id, container = %cid, status = %other, "unexpected state");
                        state
                            .workload_store
                            .write()
                            .await
                            .update_state(&workload_id, &other, None);
                    }
                }
            }
            _ => {
                tracing::info!(workload_id = %workload_id, container = %cid, "container not found, marking exited");
                state
                    .workload_store
                    .write()
                    .await
                    .update_state(&workload_id, "exited", None);
            }
        }
    }
}
