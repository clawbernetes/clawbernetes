//! Docker container runtime integration.
//!
//! This module provides the bridge between clawnode and `claw-compute`'s
//! Docker runtime, enabling the node agent to run real containers.
//!
//! ## Architecture
//!
//! ```text
//! ┌───────────────────────────────────────────────────────────┐
//! │                     NodeAgent                             │
//! │  ┌─────────────────────────────────────────────────────┐  │
//! │  │            AsyncContainerRuntime trait              │  │
//! │  └─────────────────────────────────────────────────────┘  │
//! │                          │                                │
//! │                          ▼                                │
//! │  ┌─────────────────────────────────────────────────────┐  │
//! │  │          DockerContainerRuntime (adapter)           │  │
//! │  └─────────────────────────────────────────────────────┘  │
//! │                          │                                │
//! │                          ▼                                │
//! │  ┌─────────────────────────────────────────────────────┐  │
//! │  │      claw_compute::container::DockerRuntime        │  │
//! │  └─────────────────────────────────────────────────────┘  │
//! └───────────────────────────────────────────────────────────┘
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
#[cfg(feature = "docker")]
use tracing::{debug, info};

use crate::error::NodeError;
use crate::runtime::{Container, ContainerSpec, ContainerState};

// Import claw_compute's ContainerRuntime trait when docker feature is enabled
#[cfg(feature = "docker")]
use claw_compute::container::ContainerRuntime as ComputeContainerRuntime;

/// Async container runtime trait for production use.
///
/// This trait is object-safe and designed for use with Docker/containerd.
#[allow(async_fn_in_trait)]
pub trait AsyncContainerRuntime: Send + Sync + 'static {
    /// Create and start a container from the given spec.
    fn create(
        &self,
        spec: &ContainerSpec,
    ) -> impl std::future::Future<Output = Result<Container, NodeError>> + Send;

    /// Start a stopped container.
    fn start(
        &self,
        container_id: &str,
    ) -> impl std::future::Future<Output = Result<(), NodeError>> + Send;

    /// Stop a running container.
    fn stop(
        &self,
        container_id: &str,
        timeout_secs: u32,
    ) -> impl std::future::Future<Output = Result<(), NodeError>> + Send;

    /// Remove a container.
    fn remove(
        &self,
        container_id: &str,
    ) -> impl std::future::Future<Output = Result<(), NodeError>> + Send;

    /// Get container by ID.
    fn get(
        &self,
        container_id: &str,
    ) -> impl std::future::Future<Output = Result<Container, NodeError>> + Send;

    /// List all containers.
    fn list(&self) -> impl std::future::Future<Output = Result<Vec<Container>, NodeError>> + Send;

    /// Get container logs.
    fn logs(
        &self,
        container_id: &str,
        tail: Option<usize>,
    ) -> impl std::future::Future<Output = Result<Vec<String>, NodeError>> + Send;

    /// Stream logs in real-time (returns lines as they arrive).
    fn stream_logs(
        &self,
        container_id: &str,
    ) -> impl std::future::Future<Output = Result<tokio::sync::mpsc::Receiver<String>, NodeError>> + Send;

    /// Check if the runtime is available and responsive.
    fn ping(&self) -> impl std::future::Future<Output = Result<(), NodeError>> + Send;
}

/// Docker container runtime adapter.
///
/// Wraps `claw_compute::container::DockerRuntime` and provides the
/// `AsyncContainerRuntime` trait implementation for clawnode.
#[cfg(feature = "docker")]
pub struct DockerContainerRuntime {
    runtime: claw_compute::container::DockerRuntime,
    /// Track container ID -> our container ID mapping
    containers: Arc<RwLock<HashMap<String, Container>>>,
}

#[cfg(feature = "docker")]
impl DockerContainerRuntime {
    /// Connect to the Docker daemon.
    ///
    /// # Errors
    ///
    /// Returns an error if connection to Docker fails.
    pub fn connect() -> Result<Self, NodeError> {
        let runtime = claw_compute::container::DockerRuntime::connect().map_err(|e| {
            NodeError::ContainerRuntime(format!("failed to connect to Docker: {e}"))
        })?;

        Ok(Self {
            runtime,
            containers: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Connect with GPU runtime support.
    ///
    /// # Errors
    ///
    /// Returns an error if connection to Docker fails.
    pub fn connect_with_gpu(gpu_runtime: &str) -> Result<Self, NodeError> {
        let runtime = claw_compute::container::DockerRuntime::connect()
            .map_err(|e| {
                NodeError::ContainerRuntime(format!("failed to connect to Docker: {e}"))
            })?
            .with_gpu_runtime(gpu_runtime);

        Ok(Self {
            runtime,
            containers: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Convert our ContainerSpec to claw_compute's ContainerConfig.
    fn spec_to_config(spec: &ContainerSpec) -> claw_compute::container::ContainerConfig {
        let container_name = format!("claw-{}", uuid::Uuid::new_v4());

        let mut config = claw_compute::container::ContainerConfig::new(&container_name, &spec.image);

        // Set command if provided
        if let Some(ref cmd) = spec.command {
            config = config.with_command(cmd.clone());
        }

        // Set environment variables
        for (key, value) in &spec.env {
            config = config.with_env(key, value);
        }

        // Set working directory if provided
        if let Some(ref working_dir) = spec.working_dir {
            config = config.with_working_dir(working_dir);
        }

        // Set memory limit if provided
        if let Some(memory_limit) = spec.memory_limit {
            config = config.with_memory(
                claw_compute::container::MemoryConfig::limit_bytes(memory_limit),
            );
        }

        // Set CPU limit if provided
        if let Some(cpu_limit) = spec.cpu_limit {
            config = config.with_cpu(claw_compute::container::CpuConfig::cpus(f64::from(cpu_limit)));
        }

        // Set GPU requirements if any
        if !spec.gpu_ids.is_empty() {
            let devices: Vec<claw_compute::container::GpuDevice> = spec
                .gpu_ids
                .iter()
                .map(|&idx| claw_compute::container::GpuDevice::by_index(idx))
                .collect();
            config = config.with_gpu(claw_compute::container::GpuRequirements::devices(devices));
        }

        // Add labels
        for (key, value) in &spec.labels {
            config = config.with_label(key, value);
        }

        // Set network if specified (e.g., "claw-mesh" for workload networking)
        if let Some(ref network) = spec.network {
            config = config.with_network(
                claw_compute::container::NetworkMode::Custom(network.clone()),
            );
        }

        // Add port mappings
        for pm in &spec.port_mappings {
            config = config.with_port(pm.container_port, pm.host_port);
        }

        config
    }

    /// Convert claw_compute's ContainerState to our ContainerState.
    fn convert_state(
        state: claw_compute::container::ContainerState,
    ) -> crate::runtime::ContainerState {
        match state {
            claw_compute::container::ContainerState::Created => ContainerState::Creating,
            claw_compute::container::ContainerState::Running => ContainerState::Running,
            claw_compute::container::ContainerState::Paused => ContainerState::Paused,
            claw_compute::container::ContainerState::Stopping => ContainerState::Stopping,
            claw_compute::container::ContainerState::Exited => ContainerState::Exited,
            claw_compute::container::ContainerState::Error => ContainerState::Failed,
            _ => ContainerState::Failed,
        }
    }
}

#[cfg(feature = "docker")]
impl AsyncContainerRuntime for DockerContainerRuntime {
    async fn create(&self, spec: &ContainerSpec) -> Result<Container, NodeError> {
        let config = Self::spec_to_config(spec);
        let container_name = config.name.clone();

        debug!(image = %spec.image, name = %container_name, "creating container");

        // Create the container
        let container_id = self
            .runtime
            .create(&config)
            .await
            .map_err(|e| NodeError::ContainerRuntime(format!("create failed: {e}")))?;

        // Start the container
        self.runtime
            .start(&container_id)
            .await
            .map_err(|e| NodeError::ContainerRuntime(format!("start failed: {e}")))?;

        let container = Container::new(container_id.as_str(), &spec.image)
            .with_gpus(spec.gpu_ids.clone());

        // Store in our tracking map
        let mut containers = self.containers.write().await;
        containers.insert(container_id.to_string(), container.clone());

        info!(id = %container_id, image = %spec.image, "container created and started");

        Ok(Container {
            id: container_id.to_string(),
            state: ContainerState::Running,
            ..container
        })
    }

    async fn start(&self, container_id: &str) -> Result<(), NodeError> {
        let id = claw_compute::container::ContainerId::new(container_id).map_err(|e| {
            NodeError::ContainerRuntime(format!("invalid container ID: {e}"))
        })?;

        self.runtime
            .start(&id)
            .await
            .map_err(|e| NodeError::ContainerRuntime(format!("start failed: {e}")))?;

        // Update state in our tracking
        let mut containers = self.containers.write().await;
        if let Some(container) = containers.get_mut(container_id) {
            container.state = ContainerState::Running;
        }

        Ok(())
    }

    async fn stop(&self, container_id: &str, timeout_secs: u32) -> Result<(), NodeError> {
        let id = claw_compute::container::ContainerId::new(container_id).map_err(|e| {
            NodeError::ContainerRuntime(format!("invalid container ID: {e}"))
        })?;

        let stop_options = claw_compute::container::StopOptions::with_timeout(timeout_secs);

        self.runtime
            .stop(&id, &stop_options)
            .await
            .map_err(|e| NodeError::ContainerRuntime(format!("stop failed: {e}")))?;

        // Update state in our tracking
        let mut containers = self.containers.write().await;
        if let Some(container) = containers.get_mut(container_id) {
            container.state = ContainerState::Stopped;
            container.exit_code = Some(0); // Will be updated by actual exit code later
        }

        info!(id = %container_id, "container stopped");

        Ok(())
    }

    async fn remove(&self, container_id: &str) -> Result<(), NodeError> {
        let id = claw_compute::container::ContainerId::new(container_id).map_err(|e| {
            NodeError::ContainerRuntime(format!("invalid container ID: {e}"))
        })?;

        let remove_options = claw_compute::container::RemoveOptions::force();

        self.runtime
            .remove(&id, &remove_options)
            .await
            .map_err(|e| NodeError::ContainerRuntime(format!("remove failed: {e}")))?;

        // Remove from our tracking
        let mut containers = self.containers.write().await;
        containers.remove(container_id);

        debug!(id = %container_id, "container removed");

        Ok(())
    }

    async fn get(&self, container_id: &str) -> Result<Container, NodeError> {
        let id = claw_compute::container::ContainerId::new(container_id).map_err(|e| {
            NodeError::ContainerRuntime(format!("invalid container ID: {e}"))
        })?;

        let status = self
            .runtime
            .status(&id)
            .await
            .map_err(|e| NodeError::ContainerRuntime(format!("status failed: {e}")))?;

        let state = Self::convert_state(status.state);

        // Get from our cache and update state
        let containers = self.containers.read().await;
        if let Some(container) = containers.get(container_id) {
            return Ok(Container {
                state,
                exit_code: status.exit_code.map(|c| c as i32),
                ..container.clone()
            });
        }

        // Build from Docker info if not in cache
        Ok(Container {
            id: container_id.to_string(),
            image: status.image,
            state,
            gpu_ids: Vec::new(),
            created_at: chrono::Utc::now(),
            labels: status.labels,
            exit_code: status.exit_code.map(|c| c as i32),
        })
    }

    async fn list(&self) -> Result<Vec<Container>, NodeError> {
        let list_options = claw_compute::container::ListOptions::all()
            .with_label("managed-by=clawbernetes");

        let summaries = self
            .runtime
            .list(&list_options)
            .await
            .map_err(|e| NodeError::ContainerRuntime(format!("list failed: {e}")))?;

        let containers: Vec<Container> = summaries
            .into_iter()
            .map(|s| {
                let state = Self::convert_state(s.state);
                let created_at = s
                    .created_at
                    .map(chrono::DateTime::<chrono::Utc>::from)
                    .unwrap_or_else(chrono::Utc::now);
                Container {
                    id: s.id,
                    image: s.image,
                    state,
                    gpu_ids: Vec::new(),
                    created_at,
                    labels: HashMap::new(),
                    exit_code: None,
                }
            })
            .collect();

        Ok(containers)
    }

    async fn logs(
        &self,
        container_id: &str,
        tail: Option<usize>,
    ) -> Result<Vec<String>, NodeError> {
        let id = claw_compute::container::ContainerId::new(container_id).map_err(|e| {
            NodeError::ContainerRuntime(format!("invalid container ID: {e}"))
        })?;

        let logs_options = match tail {
            Some(n) => claw_compute::container::LogsOptions::tail(n),
            None => claw_compute::container::LogsOptions::all(),
        };

        let log_bytes = self
            .runtime
            .logs(&id, &logs_options)
            .await
            .map_err(|e| NodeError::ContainerRuntime(format!("logs failed: {e}")))?;

        // Convert bytes to lines
        let log_str = String::from_utf8_lossy(&log_bytes);
        let lines: Vec<String> = log_str.lines().map(String::from).collect();

        Ok(lines)
    }

    async fn stream_logs(
        &self,
        container_id: &str,
    ) -> Result<tokio::sync::mpsc::Receiver<String>, NodeError> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let container_id = container_id.to_string();

        // For now, we'll poll logs periodically
        // A full implementation would use Docker's follow mode
        let runtime_containers = Arc::clone(&self.containers);

        tokio::spawn(async move {
            let mut last_line_count = 0;

            loop {
                // Check if container still exists
                let containers = runtime_containers.read().await;
                let container = containers.get(&container_id);

                let is_running = container
                    .map(|c| c.state == ContainerState::Running)
                    .unwrap_or(false);
                drop(containers);

                if !is_running {
                    break;
                }

                // TODO: In production, use Docker's log streaming API
                // This is a simplified polling implementation
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

                // The actual streaming would be done via bollard's log stream
                // For now, we just break after a short time to not loop forever
                if last_line_count > 100 {
                    break;
                }
                last_line_count += 1;
            }

            // Signal end of stream
            drop(tx);
        });

        Ok(rx)
    }

    async fn ping(&self) -> Result<(), NodeError> {
        self.runtime
            .ping()
            .await
            .map_err(|e| NodeError::ContainerRuntime(format!("ping failed: {e}")))
    }
}

/// Sync adapter: implements the sync `ContainerRuntime` trait by bridging to async Docker calls.
///
/// This allows `DockerContainerRuntime` to be used in the synchronous handler path
/// (`handlers.rs`) which expects `ContainerRuntime`.
#[cfg(feature = "docker")]
impl crate::runtime::ContainerRuntime for DockerContainerRuntime {
    fn create(
        &self,
        spec: &crate::runtime::ContainerSpec,
    ) -> Result<crate::runtime::Container, NodeError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(AsyncContainerRuntime::create(self, spec))
        })
    }

    fn start(&self, container_id: &str) -> Result<(), NodeError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(AsyncContainerRuntime::start(self, container_id))
        })
    }

    fn stop(&self, container_id: &str, timeout_secs: u32) -> Result<(), NodeError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(AsyncContainerRuntime::stop(self, container_id, timeout_secs))
        })
    }

    fn remove(&self, container_id: &str) -> Result<(), NodeError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(AsyncContainerRuntime::remove(self, container_id))
        })
    }

    fn get(&self, container_id: &str) -> Result<crate::runtime::Container, NodeError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(AsyncContainerRuntime::get(self, container_id))
        })
    }

    fn list(&self) -> Result<Vec<crate::runtime::Container>, NodeError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(AsyncContainerRuntime::list(self))
        })
    }

    fn logs(
        &self,
        container_id: &str,
        tail: Option<usize>,
    ) -> Result<Vec<String>, NodeError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(AsyncContainerRuntime::logs(self, container_id, tail))
        })
    }
}

/// In-memory fake async runtime for testing.
#[derive(Debug, Default)]
pub struct FakeAsyncContainerRuntime {
    containers: Arc<RwLock<HashMap<String, Container>>>,
    next_id: Arc<RwLock<u64>>,
    /// Simulated logs per container.
    logs: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl FakeAsyncContainerRuntime {
    /// Create a new fake runtime.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the number of containers.
    pub async fn container_count(&self) -> usize {
        self.containers.read().await.len()
    }

    /// Add fake logs for a container.
    pub async fn add_logs(&self, container_id: &str, lines: Vec<String>) {
        let mut logs = self.logs.write().await;
        logs.insert(container_id.to_string(), lines);
    }

    async fn generate_id(&self) -> String {
        let mut id = self.next_id.write().await;
        *id += 1;
        format!("container-{:08x}", *id)
    }
}

impl AsyncContainerRuntime for FakeAsyncContainerRuntime {
    async fn create(&self, spec: &ContainerSpec) -> Result<Container, NodeError> {
        let id = self.generate_id().await;
        let container = Container::new(&id, &spec.image).with_gpus(spec.gpu_ids.clone());

        let container = Container {
            state: ContainerState::Running,
            ..container
        };

        let mut containers = self.containers.write().await;
        containers.insert(id.clone(), container.clone());

        // Add default logs
        let mut logs = self.logs.write().await;
        logs.insert(
            id,
            vec![
                "Starting container...".to_string(),
                "Application initialized".to_string(),
                "Ready to serve requests".to_string(),
            ],
        );

        Ok(container)
    }

    async fn start(&self, container_id: &str) -> Result<(), NodeError> {
        let mut containers = self.containers.write().await;
        let container = containers.get_mut(container_id).ok_or_else(|| {
            NodeError::ContainerRuntime(format!("container not found: {container_id}"))
        })?;

        if container.state.is_terminal() {
            return Err(NodeError::ContainerRuntime(
                "cannot start terminal container".to_string(),
            ));
        }

        container.state = ContainerState::Running;
        Ok(())
    }

    async fn stop(&self, container_id: &str, _timeout_secs: u32) -> Result<(), NodeError> {
        let mut containers = self.containers.write().await;
        let container = containers.get_mut(container_id).ok_or_else(|| {
            NodeError::ContainerRuntime(format!("container not found: {container_id}"))
        })?;

        container.state = ContainerState::Stopped;
        container.exit_code = Some(0);
        Ok(())
    }

    async fn remove(&self, container_id: &str) -> Result<(), NodeError> {
        let mut containers = self.containers.write().await;
        containers.remove(container_id).ok_or_else(|| {
            NodeError::ContainerRuntime(format!("container not found: {container_id}"))
        })?;

        // Remove logs too
        let mut logs = self.logs.write().await;
        logs.remove(container_id);

        Ok(())
    }

    async fn get(&self, container_id: &str) -> Result<Container, NodeError> {
        let containers = self.containers.read().await;
        containers.get(container_id).cloned().ok_or_else(|| {
            NodeError::ContainerRuntime(format!("container not found: {container_id}"))
        })
    }

    async fn list(&self) -> Result<Vec<Container>, NodeError> {
        let containers = self.containers.read().await;
        Ok(containers.values().cloned().collect())
    }

    async fn logs(
        &self,
        container_id: &str,
        tail: Option<usize>,
    ) -> Result<Vec<String>, NodeError> {
        // Check container exists
        let containers = self.containers.read().await;
        if !containers.contains_key(container_id) {
            return Err(NodeError::ContainerRuntime(format!(
                "container not found: {container_id}"
            )));
        }
        drop(containers);

        // Get logs
        let logs = self.logs.read().await;
        let all_logs = logs.get(container_id).cloned().unwrap_or_else(|| {
            vec![
                "Starting container...".to_string(),
                "Application initialized".to_string(),
                "Ready to serve requests".to_string(),
            ]
        });

        Ok(match tail {
            Some(n) => all_logs.into_iter().rev().take(n).rev().collect(),
            None => all_logs,
        })
    }

    async fn stream_logs(
        &self,
        container_id: &str,
    ) -> Result<tokio::sync::mpsc::Receiver<String>, NodeError> {
        // Check container exists
        let containers = self.containers.read().await;
        if !containers.contains_key(container_id) {
            return Err(NodeError::ContainerRuntime(format!(
                "container not found: {container_id}"
            )));
        }
        drop(containers);

        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let logs = self.logs.clone();
        let container_id = container_id.to_string();

        tokio::spawn(async move {
            // Send existing logs
            let logs_guard = logs.read().await;
            if let Some(lines) = logs_guard.get(&container_id) {
                for line in lines {
                    if tx.send(line.clone()).await.is_err() {
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn ping(&self) -> Result<(), NodeError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== FakeAsyncContainerRuntime Tests ====================

    #[tokio::test]
    async fn test_fake_runtime_create() {
        let runtime = FakeAsyncContainerRuntime::new();
        let spec = ContainerSpec::new("nginx:latest").with_gpus(vec![0]);

        let container = runtime.create(&spec).await.expect("should create");

        assert!(container.id.starts_with("container-"));
        assert_eq!(container.image, "nginx:latest");
        assert_eq!(container.state, ContainerState::Running);
        assert_eq!(container.gpu_ids, vec![0]);
    }

    #[tokio::test]
    async fn test_fake_runtime_lifecycle() {
        let runtime = FakeAsyncContainerRuntime::new();
        let spec = ContainerSpec::new("nginx:latest");

        // Create
        let container = runtime.create(&spec).await.expect("should create");
        let id = container.id.clone();

        // Verify running
        let fetched = runtime.get(&id).await.expect("should get");
        assert_eq!(fetched.state, ContainerState::Running);

        // Stop
        runtime.stop(&id, 10).await.expect("should stop");
        let fetched = runtime.get(&id).await.expect("should get");
        assert_eq!(fetched.state, ContainerState::Stopped);
        assert_eq!(fetched.exit_code, Some(0));

        // Remove
        runtime.remove(&id).await.expect("should remove");
        let result = runtime.get(&id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fake_runtime_list() {
        let runtime = FakeAsyncContainerRuntime::new();

        runtime
            .create(&ContainerSpec::new("nginx:1"))
            .await
            .expect("create");
        runtime
            .create(&ContainerSpec::new("nginx:2"))
            .await
            .expect("create");

        let containers = runtime.list().await.expect("should list");
        assert_eq!(containers.len(), 2);
    }

    #[tokio::test]
    async fn test_fake_runtime_logs() {
        let runtime = FakeAsyncContainerRuntime::new();
        let spec = ContainerSpec::new("nginx:latest");
        let container = runtime.create(&spec).await.expect("should create");

        let logs = runtime.logs(&container.id, None).await.expect("should get logs");
        assert!(!logs.is_empty());

        let tail_logs = runtime
            .logs(&container.id, Some(1))
            .await
            .expect("should get logs");
        assert_eq!(tail_logs.len(), 1);
    }

    #[tokio::test]
    async fn test_fake_runtime_not_found() {
        let runtime = FakeAsyncContainerRuntime::new();

        let result = runtime.get("nonexistent").await;
        assert!(result.is_err());

        let result = runtime.stop("nonexistent", 10).await;
        assert!(result.is_err());

        let result = runtime.remove("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fake_runtime_start_terminal() {
        let runtime = FakeAsyncContainerRuntime::new();
        let spec = ContainerSpec::new("nginx:latest");
        let container = runtime.create(&spec).await.expect("should create");

        runtime.stop(&container.id, 10).await.expect("should stop");
        let result = runtime.start(&container.id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fake_runtime_ping() {
        let runtime = FakeAsyncContainerRuntime::new();
        runtime.ping().await.expect("ping should succeed");
    }

    #[tokio::test]
    async fn test_fake_runtime_custom_logs() {
        let runtime = FakeAsyncContainerRuntime::new();
        let spec = ContainerSpec::new("nginx:latest");
        let container = runtime.create(&spec).await.expect("should create");

        // Add custom logs
        runtime
            .add_logs(
                &container.id,
                vec![
                    "Custom log 1".to_string(),
                    "Custom log 2".to_string(),
                    "Custom log 3".to_string(),
                ],
            )
            .await;

        let logs = runtime.logs(&container.id, None).await.expect("should get logs");
        assert_eq!(logs.len(), 3);
        assert_eq!(logs[0], "Custom log 1");
    }

    #[tokio::test]
    async fn test_fake_runtime_stream_logs() {
        let runtime = FakeAsyncContainerRuntime::new();
        let spec = ContainerSpec::new("nginx:latest");
        let container = runtime.create(&spec).await.expect("should create");

        let mut rx = runtime
            .stream_logs(&container.id)
            .await
            .expect("should stream");

        // Should receive some logs
        let mut received = Vec::new();
        while let Ok(Some(line)) = tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            rx.recv(),
        )
        .await
        {
            received.push(line);
        }

        assert!(!received.is_empty());
    }

    #[tokio::test]
    async fn test_container_spec_builder() {
        let spec = ContainerSpec::new("nvidia/cuda:12.0")
            .with_command(vec!["python".to_string(), "train.py".to_string()])
            .with_env("CUDA_VISIBLE_DEVICES", "0,1")
            .with_gpus(vec![0, 1])
            .with_memory_limit(8 * 1024 * 1024 * 1024) // 8GB
            .with_cpu_limit(4.0)
            .with_label("workload-id", "abc123");

        assert_eq!(spec.image, "nvidia/cuda:12.0");
        assert_eq!(
            spec.command,
            Some(vec!["python".to_string(), "train.py".to_string()])
        );
        assert_eq!(
            spec.env.get("CUDA_VISIBLE_DEVICES"),
            Some(&"0,1".to_string())
        );
        assert_eq!(spec.gpu_ids, vec![0, 1]);
        assert_eq!(spec.memory_limit, Some(8 * 1024 * 1024 * 1024));
        assert_eq!(spec.cpu_limit, Some(4.0));
    }
}
