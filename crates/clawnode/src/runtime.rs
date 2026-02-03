#![allow(clippy::expect_used)]
//! Container runtime interface.
//!
//! Provides abstraction over container runtimes (containerd, podman, etc.)
//! for running GPU workloads.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::NodeError;

/// Container state in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerState {
    /// Container is being created.
    Creating,
    /// Container is running.
    Running,
    /// Container is paused.
    Paused,
    /// Container is stopping.
    Stopping,
    /// Container has stopped.
    Stopped,
    /// Container has exited.
    Exited,
    /// Container creation or execution failed.
    Failed,
}

impl ContainerState {
    /// Check if this is a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Stopped | Self::Exited | Self::Failed)
    }

    /// Check if the container is running.
    #[must_use]
    pub const fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }
}

/// Container information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Container {
    /// Unique container ID.
    pub id: String,
    /// Container image.
    pub image: String,
    /// Current state.
    pub state: ContainerState,
    /// GPU indices assigned to this container.
    pub gpu_ids: Vec<u32>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Labels/metadata.
    pub labels: HashMap<String, String>,
    /// Exit code (if exited).
    pub exit_code: Option<i32>,
}

impl Container {
    /// Create a new container with the given ID and image.
    #[must_use]
    pub fn new(id: impl Into<String>, image: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            image: image.into(),
            state: ContainerState::Creating,
            gpu_ids: Vec::new(),
            created_at: Utc::now(),
            labels: HashMap::new(),
            exit_code: None,
        }
    }

    /// Set GPU indices.
    #[must_use]
    pub fn with_gpus(mut self, gpu_ids: Vec<u32>) -> Self {
        self.gpu_ids = gpu_ids;
        self
    }

    /// Add a label.
    #[must_use]
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Check if the container uses a specific GPU.
    #[must_use]
    pub fn uses_gpu(&self, gpu_id: u32) -> bool {
        self.gpu_ids.contains(&gpu_id)
    }
}

/// Specification for creating a container.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContainerSpec {
    /// Container image.
    pub image: String,
    /// Command to run.
    pub command: Option<Vec<String>>,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// GPU indices to attach.
    pub gpu_ids: Vec<u32>,
    /// Memory limit in bytes.
    pub memory_limit: Option<u64>,
    /// CPU limit (fractional cores).
    pub cpu_limit: Option<f32>,
    /// Labels.
    pub labels: HashMap<String, String>,
    /// Working directory.
    pub working_dir: Option<String>,
}

impl ContainerSpec {
    /// Create a new container spec with just an image.
    #[must_use]
    pub fn new(image: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            command: None,
            env: HashMap::new(),
            gpu_ids: Vec::new(),
            memory_limit: None,
            cpu_limit: None,
            labels: HashMap::new(),
            working_dir: None,
        }
    }

    /// Set the command.
    #[must_use]
    pub fn with_command(mut self, cmd: Vec<String>) -> Self {
        self.command = Some(cmd);
        self
    }

    /// Add an environment variable.
    #[must_use]
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Set GPU indices.
    #[must_use]
    pub fn with_gpus(mut self, gpu_ids: Vec<u32>) -> Self {
        self.gpu_ids = gpu_ids;
        self
    }

    /// Set memory limit.
    #[must_use]
    pub const fn with_memory_limit(mut self, limit: u64) -> Self {
        self.memory_limit = Some(limit);
        self
    }

    /// Set CPU limit.
    #[must_use]
    pub const fn with_cpu_limit(mut self, limit: f32) -> Self {
        self.cpu_limit = Some(limit);
        self
    }

    /// Add a label.
    #[must_use]
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }
}

/// Trait for container runtime implementations.
pub trait ContainerRuntime: Send + Sync {
    /// Create and start a container.
    ///
    /// # Errors
    ///
    /// Returns an error if container creation fails.
    fn create(&self, spec: &ContainerSpec) -> Result<Container, NodeError>;

    /// Start a stopped container.
    ///
    /// # Errors
    ///
    /// Returns an error if the container cannot be started.
    fn start(&self, container_id: &str) -> Result<(), NodeError>;

    /// Stop a running container.
    ///
    /// # Errors
    ///
    /// Returns an error if the container cannot be stopped.
    fn stop(&self, container_id: &str, timeout_secs: u32) -> Result<(), NodeError>;

    /// Remove a container.
    ///
    /// # Errors
    ///
    /// Returns an error if the container cannot be removed.
    fn remove(&self, container_id: &str) -> Result<(), NodeError>;

    /// Get container by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the container is not found.
    fn get(&self, container_id: &str) -> Result<Container, NodeError>;

    /// List all containers.
    ///
    /// # Errors
    ///
    /// Returns an error if listing fails.
    fn list(&self) -> Result<Vec<Container>, NodeError>;

    /// Get container logs.
    ///
    /// # Errors
    ///
    /// Returns an error if logs cannot be retrieved.
    fn logs(&self, container_id: &str, tail: Option<usize>) -> Result<Vec<String>, NodeError>;
}

/// In-memory fake runtime for testing.
#[derive(Debug, Default)]
pub struct FakeContainerRuntime {
    containers: Arc<RwLock<HashMap<String, Container>>>,
    next_id: Arc<RwLock<u64>>,
}

impl FakeContainerRuntime {
    /// Create a new fake runtime.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the number of containers.
    #[must_use]
    pub fn container_count(&self) -> usize {
        self.containers
            .read()
            .map(|c| c.len())
            .unwrap_or(0)
    }

    fn generate_id(&self) -> String {
        let mut id = self.next_id.write().expect("lock poisoned");
        *id += 1;
        format!("container-{:08x}", *id)
    }
}

impl ContainerRuntime for FakeContainerRuntime {
    fn create(&self, spec: &ContainerSpec) -> Result<Container, NodeError> {
        let id = self.generate_id();
        let mut container = Container::new(&id, &spec.image)
            .with_gpus(spec.gpu_ids.clone());

        for (k, v) in &spec.labels {
            container = container.with_label(k, v);
        }

        container.state = ContainerState::Running;

        let mut containers = self
            .containers
            .write()
            .map_err(|_| NodeError::ContainerRuntime("lock poisoned".to_string()))?;

        containers.insert(id, container.clone());
        Ok(container)
    }

    fn start(&self, container_id: &str) -> Result<(), NodeError> {
        let mut containers = self
            .containers
            .write()
            .map_err(|_| NodeError::ContainerRuntime("lock poisoned".to_string()))?;

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

    fn stop(&self, container_id: &str, _timeout_secs: u32) -> Result<(), NodeError> {
        let mut containers = self
            .containers
            .write()
            .map_err(|_| NodeError::ContainerRuntime("lock poisoned".to_string()))?;

        let container = containers.get_mut(container_id).ok_or_else(|| {
            NodeError::ContainerRuntime(format!("container not found: {container_id}"))
        })?;

        container.state = ContainerState::Stopped;
        container.exit_code = Some(0);
        Ok(())
    }

    fn remove(&self, container_id: &str) -> Result<(), NodeError> {
        let mut containers = self
            .containers
            .write()
            .map_err(|_| NodeError::ContainerRuntime("lock poisoned".to_string()))?;

        containers.remove(container_id).ok_or_else(|| {
            NodeError::ContainerRuntime(format!("container not found: {container_id}"))
        })?;

        Ok(())
    }

    fn get(&self, container_id: &str) -> Result<Container, NodeError> {
        let containers = self
            .containers
            .read()
            .map_err(|_| NodeError::ContainerRuntime("lock poisoned".to_string()))?;

        containers.get(container_id).cloned().ok_or_else(|| {
            NodeError::ContainerRuntime(format!("container not found: {container_id}"))
        })
    }

    fn list(&self) -> Result<Vec<Container>, NodeError> {
        let containers = self
            .containers
            .read()
            .map_err(|_| NodeError::ContainerRuntime("lock poisoned".to_string()))?;

        Ok(containers.values().cloned().collect())
    }

    fn logs(&self, container_id: &str, tail: Option<usize>) -> Result<Vec<String>, NodeError> {
        // Check container exists
        let containers = self
            .containers
            .read()
            .map_err(|_| NodeError::ContainerRuntime("lock poisoned".to_string()))?;

        if !containers.contains_key(container_id) {
            return Err(NodeError::ContainerRuntime(format!(
                "container not found: {container_id}"
            )));
        }

        // Return fake logs
        let all_logs = vec![
            "Starting container...".to_string(),
            "Application initialized".to_string(),
            "Ready to serve requests".to_string(),
        ];

        Ok(match tail {
            Some(n) => all_logs.into_iter().rev().take(n).rev().collect(),
            None => all_logs,
        })
    }
}

/// GPU allocation tracker.
#[derive(Debug, Default)]
pub struct GpuAllocator {
    /// GPU index -> container ID mapping.
    allocations: HashMap<u32, String>,
}

impl GpuAllocator {
    /// Create a new GPU allocator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate GPUs to a container.
    ///
    /// # Errors
    ///
    /// Returns an error if any GPU is already allocated.
    pub fn allocate(&mut self, container_id: &str, gpu_ids: &[u32]) -> Result<(), NodeError> {
        // Check all GPUs are available
        for &gpu_id in gpu_ids {
            if let Some(existing) = self.allocations.get(&gpu_id) {
                return Err(NodeError::ContainerRuntime(format!(
                    "GPU {gpu_id} already allocated to container {existing}"
                )));
            }
        }

        // Allocate all GPUs
        for &gpu_id in gpu_ids {
            self.allocations.insert(gpu_id, container_id.to_string());
        }

        Ok(())
    }

    /// Release GPUs from a container.
    pub fn release(&mut self, container_id: &str) {
        self.allocations.retain(|_, v| v != container_id);
    }

    /// Check if a GPU is available.
    #[must_use]
    pub fn is_available(&self, gpu_id: u32) -> bool {
        !self.allocations.contains_key(&gpu_id)
    }

    /// Get available GPUs from a list.
    #[must_use]
    pub fn available_gpus(&self, gpu_ids: &[u32]) -> Vec<u32> {
        gpu_ids
            .iter()
            .copied()
            .filter(|id| self.is_available(*id))
            .collect()
    }

    /// Get the container using a GPU.
    #[must_use]
    pub fn get_container(&self, gpu_id: u32) -> Option<&str> {
        self.allocations.get(&gpu_id).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_state_is_terminal() {
        assert!(!ContainerState::Creating.is_terminal());
        assert!(!ContainerState::Running.is_terminal());
        assert!(!ContainerState::Paused.is_terminal());
        assert!(!ContainerState::Stopping.is_terminal());
        assert!(ContainerState::Stopped.is_terminal());
        assert!(ContainerState::Exited.is_terminal());
        assert!(ContainerState::Failed.is_terminal());
    }

    #[test]
    fn test_container_state_is_running() {
        assert!(ContainerState::Running.is_running());
        assert!(!ContainerState::Stopped.is_running());
        assert!(!ContainerState::Creating.is_running());
    }

    #[test]
    fn test_container_creation() {
        let container = Container::new("test-id", "nginx:latest")
            .with_gpus(vec![0, 1])
            .with_label("app", "test");

        assert_eq!(container.id, "test-id");
        assert_eq!(container.image, "nginx:latest");
        assert_eq!(container.state, ContainerState::Creating);
        assert_eq!(container.gpu_ids, vec![0, 1]);
        assert_eq!(container.labels.get("app"), Some(&"test".to_string()));
    }

    #[test]
    fn test_container_uses_gpu() {
        let container = Container::new("test", "image").with_gpus(vec![0, 2]);

        assert!(container.uses_gpu(0));
        assert!(!container.uses_gpu(1));
        assert!(container.uses_gpu(2));
    }

    #[test]
    fn test_container_spec_builder() {
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

    #[test]
    fn test_fake_runtime_create() {
        let runtime = FakeContainerRuntime::new();
        let spec = ContainerSpec::new("nginx:latest").with_gpus(vec![0]);

        let container = runtime.create(&spec).expect("should create");

        assert!(container.id.starts_with("container-"));
        assert_eq!(container.image, "nginx:latest");
        assert_eq!(container.state, ContainerState::Running);
        assert_eq!(container.gpu_ids, vec![0]);
    }

    #[test]
    fn test_fake_runtime_lifecycle() {
        let runtime = FakeContainerRuntime::new();
        let spec = ContainerSpec::new("nginx:latest");

        // Create
        let container = runtime.create(&spec).expect("should create");
        let id = container.id.clone();

        // Verify running
        let fetched = runtime.get(&id).expect("should get");
        assert_eq!(fetched.state, ContainerState::Running);

        // Stop
        runtime.stop(&id, 10).expect("should stop");
        let fetched = runtime.get(&id).expect("should get");
        assert_eq!(fetched.state, ContainerState::Stopped);
        assert_eq!(fetched.exit_code, Some(0));

        // Remove
        runtime.remove(&id).expect("should remove");
        let result = runtime.get(&id);
        assert!(result.is_err());
    }

    #[test]
    fn test_fake_runtime_list() {
        let runtime = FakeContainerRuntime::new();

        runtime
            .create(&ContainerSpec::new("nginx:1"))
            .expect("create");
        runtime
            .create(&ContainerSpec::new("nginx:2"))
            .expect("create");

        let containers = runtime.list().expect("should list");
        assert_eq!(containers.len(), 2);
    }

    #[test]
    fn test_fake_runtime_logs() {
        let runtime = FakeContainerRuntime::new();
        let spec = ContainerSpec::new("nginx:latest");
        let container = runtime.create(&spec).expect("should create");

        let logs = runtime.logs(&container.id, None).expect("should get logs");
        assert!(!logs.is_empty());

        let tail_logs = runtime
            .logs(&container.id, Some(1))
            .expect("should get logs");
        assert_eq!(tail_logs.len(), 1);
    }

    #[test]
    fn test_fake_runtime_not_found() {
        let runtime = FakeContainerRuntime::new();

        let result = runtime.get("nonexistent");
        assert!(result.is_err());

        let result = runtime.stop("nonexistent", 10);
        assert!(result.is_err());

        let result = runtime.remove("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_fake_runtime_start_terminal() {
        let runtime = FakeContainerRuntime::new();
        let spec = ContainerSpec::new("nginx:latest");
        let container = runtime.create(&spec).expect("should create");

        runtime.stop(&container.id, 10).expect("should stop");
        let result = runtime.start(&container.id);
        assert!(result.is_err());
    }

    #[test]
    fn test_gpu_allocator_allocate() {
        let mut allocator = GpuAllocator::new();

        allocator
            .allocate("container-1", &[0, 1])
            .expect("should allocate");

        assert!(!allocator.is_available(0));
        assert!(!allocator.is_available(1));
        assert!(allocator.is_available(2));
    }

    #[test]
    fn test_gpu_allocator_double_allocate() {
        let mut allocator = GpuAllocator::new();

        allocator
            .allocate("container-1", &[0, 1])
            .expect("should allocate");

        let result = allocator.allocate("container-2", &[1, 2]);
        assert!(result.is_err());
    }

    #[test]
    fn test_gpu_allocator_release() {
        let mut allocator = GpuAllocator::new();

        allocator.allocate("container-1", &[0, 1]).expect("allocate");
        assert!(!allocator.is_available(0));

        allocator.release("container-1");
        assert!(allocator.is_available(0));
        assert!(allocator.is_available(1));
    }

    #[test]
    fn test_gpu_allocator_available_gpus() {
        let mut allocator = GpuAllocator::new();
        allocator.allocate("container-1", &[1]).expect("allocate");

        let available = allocator.available_gpus(&[0, 1, 2, 3]);
        assert_eq!(available, vec![0, 2, 3]);
    }

    #[test]
    fn test_gpu_allocator_get_container() {
        let mut allocator = GpuAllocator::new();
        allocator.allocate("container-1", &[0]).expect("allocate");

        assert_eq!(allocator.get_container(0), Some("container-1"));
        assert_eq!(allocator.get_container(1), None);
    }

    #[test]
    fn test_container_serialization() {
        let container = Container::new("test-id", "nginx:latest")
            .with_gpus(vec![0, 1])
            .with_label("app", "test");

        let json = serde_json::to_string(&container).expect("serialize");
        let parsed: Container = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(container.id, parsed.id);
        assert_eq!(container.image, parsed.image);
        assert_eq!(container.gpu_ids, parsed.gpu_ids);
    }

    #[test]
    fn test_container_spec_serialization() {
        let spec = ContainerSpec::new("nginx:latest")
            .with_env("KEY", "VALUE")
            .with_gpus(vec![0]);

        let json = serde_json::to_string(&spec).expect("serialize");
        let parsed: ContainerSpec = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(spec, parsed);
    }
}
