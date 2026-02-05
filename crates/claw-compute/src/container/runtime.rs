//! Container runtime trait definition.

use std::future::Future;
use std::pin::Pin;

use super::config::ContainerConfig;
use super::error::{ContainerError, ContainerId, ContainerResult};
use super::status::{ContainerState, ContainerStatus, ContainerSummary, ResourceStats};

/// Options for listing containers.
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    /// Include all containers (not just running).
    pub all: bool,

    /// Filter by label (key=value).
    pub label_filter: Option<String>,

    /// Filter by name pattern.
    pub name_filter: Option<String>,

    /// Filter by status.
    pub status_filter: Option<ContainerState>,

    /// Maximum number of containers to return.
    pub limit: Option<usize>,
}

impl ListOptions {
    /// Create options for listing all containers.
    #[must_use]
    pub fn all() -> Self {
        Self {
            all: true,
            ..Default::default()
        }
    }

    /// Create options for listing only running containers.
    #[must_use]
    pub fn running() -> Self {
        Self {
            all: false,
            status_filter: Some(ContainerState::Running),
            ..Default::default()
        }
    }

    /// Filter by label.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label_filter = Some(label.into());
        self
    }

    /// Filter by name pattern.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name_filter = Some(name.into());
        self
    }

    /// Limit number of results.
    #[must_use]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// Options for stopping a container.
#[derive(Debug, Clone)]
pub struct StopOptions {
    /// Timeout in seconds before killing.
    pub timeout_secs: Option<u32>,

    /// Signal to send (default: SIGTERM, then SIGKILL).
    pub signal: Option<String>,
}

impl Default for StopOptions {
    fn default() -> Self {
        Self {
            timeout_secs: Some(10),
            signal: None,
        }
    }
}

impl StopOptions {
    /// Create options with custom timeout.
    #[must_use]
    pub fn with_timeout(secs: u32) -> Self {
        Self {
            timeout_secs: Some(secs),
            signal: None,
        }
    }

    /// Force kill immediately.
    #[must_use]
    pub fn force() -> Self {
        Self {
            timeout_secs: Some(0),
            signal: Some("SIGKILL".to_string()),
        }
    }
}

/// Options for removing a container.
#[derive(Debug, Clone, Default)]
pub struct RemoveOptions {
    /// Force removal of running container.
    pub force: bool,

    /// Remove associated volumes.
    pub volumes: bool,

    /// Remove associated anonymous volumes.
    pub anonymous_volumes: bool,
}

impl RemoveOptions {
    /// Create options with force removal.
    #[must_use]
    pub fn force() -> Self {
        Self {
            force: true,
            volumes: false,
            anonymous_volumes: false,
        }
    }

    /// Remove volumes along with container.
    #[must_use]
    pub fn with_volumes(mut self) -> Self {
        self.volumes = true;
        self
    }
}

/// Logs streaming options.
#[derive(Debug, Clone, Default)]
pub struct LogsOptions {
    /// Include stdout.
    pub stdout: bool,

    /// Include stderr.
    pub stderr: bool,

    /// Include timestamps.
    pub timestamps: bool,

    /// Follow log output (streaming).
    pub follow: bool,

    /// Number of lines to tail.
    pub tail: Option<usize>,

    /// Show logs since timestamp.
    pub since: Option<std::time::SystemTime>,
}

impl LogsOptions {
    /// Create options for all logs.
    #[must_use]
    pub fn all() -> Self {
        Self {
            stdout: true,
            stderr: true,
            timestamps: false,
            follow: false,
            tail: None,
            since: None,
        }
    }

    /// Create options for following logs.
    #[must_use]
    pub fn follow() -> Self {
        Self {
            stdout: true,
            stderr: true,
            timestamps: false,
            follow: true,
            tail: None,
            since: None,
        }
    }

    /// Tail last N lines.
    #[must_use]
    pub fn tail(n: usize) -> Self {
        Self {
            stdout: true,
            stderr: true,
            timestamps: false,
            follow: false,
            tail: Some(n),
            since: None,
        }
    }

    /// Include timestamps.
    #[must_use]
    pub fn with_timestamps(mut self) -> Self {
        self.timestamps = true;
        self
    }
}

/// Exec options for running commands in containers.
#[derive(Debug, Clone)]
pub struct ExecOptions {
    /// Command to run.
    pub cmd: Vec<String>,

    /// Environment variables.
    pub env: Vec<String>,

    /// Working directory.
    pub working_dir: Option<String>,

    /// User to run as.
    pub user: Option<String>,

    /// Attach stdin.
    pub attach_stdin: bool,

    /// Attach stdout.
    pub attach_stdout: bool,

    /// Attach stderr.
    pub attach_stderr: bool,

    /// Allocate TTY.
    pub tty: bool,

    /// Run in privileged mode.
    pub privileged: bool,
}

impl ExecOptions {
    /// Create exec options for a command.
    #[must_use]
    pub fn cmd(cmd: Vec<String>) -> Self {
        Self {
            cmd,
            env: Vec::new(),
            working_dir: None,
            user: None,
            attach_stdin: false,
            attach_stdout: true,
            attach_stderr: true,
            tty: false,
            privileged: false,
        }
    }

    /// Set environment variable.
    #[must_use]
    pub fn with_env(mut self, env: impl Into<String>) -> Self {
        self.env.push(env.into());
        self
    }

    /// Set working directory.
    #[must_use]
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Set user.
    #[must_use]
    pub fn with_user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// Enable TTY.
    #[must_use]
    pub fn with_tty(mut self) -> Self {
        self.tty = true;
        self
    }
}

/// Exec result.
#[derive(Debug, Clone)]
pub struct ExecResult {
    /// Exit code.
    pub exit_code: i64,

    /// Stdout output.
    pub stdout: Vec<u8>,

    /// Stderr output.
    pub stderr: Vec<u8>,
}

impl ExecResult {
    /// Check if command succeeded.
    #[must_use]
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }

    /// Get stdout as string.
    #[must_use]
    pub fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.stdout).to_string()
    }

    /// Get stderr as string.
    #[must_use]
    pub fn stderr_str(&self) -> String {
        String::from_utf8_lossy(&self.stderr).to_string()
    }
}

/// Container runtime trait for managing container lifecycle.
///
/// This trait abstracts over different container runtimes (Docker, containerd, etc.)
/// to provide a unified interface for container operations.
pub trait ContainerRuntime: Send + Sync {
    /// Create a new container.
    ///
    /// # Errors
    ///
    /// Returns error if container creation fails.
    fn create<'a>(
        &'a self,
        config: &'a ContainerConfig,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<ContainerId>> + Send + 'a>>;

    /// Start a container.
    ///
    /// # Errors
    ///
    /// Returns error if container start fails.
    fn start<'a>(
        &'a self,
        id: &'a ContainerId,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<()>> + Send + 'a>>;

    /// Stop a container.
    ///
    /// # Errors
    ///
    /// Returns error if container stop fails.
    fn stop<'a>(
        &'a self,
        id: &'a ContainerId,
        options: &'a StopOptions,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<()>> + Send + 'a>>;

    /// Remove a container.
    ///
    /// # Errors
    ///
    /// Returns error if container removal fails.
    fn remove<'a>(
        &'a self,
        id: &'a ContainerId,
        options: &'a RemoveOptions,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<()>> + Send + 'a>>;

    /// Get container status.
    ///
    /// # Errors
    ///
    /// Returns error if status retrieval fails.
    fn status<'a>(
        &'a self,
        id: &'a ContainerId,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<ContainerStatus>> + Send + 'a>>;

    /// List containers.
    ///
    /// # Errors
    ///
    /// Returns error if listing fails.
    fn list<'a>(
        &'a self,
        options: &'a ListOptions,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<Vec<ContainerSummary>>> + Send + 'a>>;

    /// Get container resource stats.
    ///
    /// # Errors
    ///
    /// Returns error if stats retrieval fails.
    fn stats<'a>(
        &'a self,
        id: &'a ContainerId,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<ResourceStats>> + Send + 'a>>;

    /// Get container logs.
    ///
    /// # Errors
    ///
    /// Returns error if log retrieval fails.
    fn logs<'a>(
        &'a self,
        id: &'a ContainerId,
        options: &'a LogsOptions,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<Vec<u8>>> + Send + 'a>>;

    /// Execute a command in a running container.
    ///
    /// # Errors
    ///
    /// Returns error if exec fails.
    fn exec<'a>(
        &'a self,
        id: &'a ContainerId,
        options: &'a ExecOptions,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<ExecResult>> + Send + 'a>>;

    /// Wait for container to exit.
    ///
    /// # Errors
    ///
    /// Returns error if wait fails.
    fn wait<'a>(
        &'a self,
        id: &'a ContainerId,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<i64>> + Send + 'a>>;

    /// Pause a container.
    ///
    /// # Errors
    ///
    /// Returns error if pause fails.
    fn pause<'a>(
        &'a self,
        id: &'a ContainerId,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<()>> + Send + 'a>>;

    /// Unpause a container.
    ///
    /// # Errors
    ///
    /// Returns error if unpause fails.
    fn unpause<'a>(
        &'a self,
        id: &'a ContainerId,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<()>> + Send + 'a>>;

    /// Check if runtime is available.
    ///
    /// # Errors
    ///
    /// Returns error if runtime is not available.
    fn ping(&self) -> Pin<Box<dyn Future<Output = ContainerResult<()>> + Send + '_>>;

    /// Get runtime info.
    fn info(&self) -> RuntimeInfo;
}

/// Information about the container runtime.
#[derive(Debug, Clone, Default)]
pub struct RuntimeInfo {
    /// Runtime name (e.g., "docker", "containerd").
    pub name: String,

    /// Runtime version.
    pub version: String,

    /// API version.
    pub api_version: String,

    /// Whether GPU support is available.
    pub gpu_available: bool,

    /// GPU runtime (e.g., "nvidia").
    pub gpu_runtime: Option<String>,

    /// Number of containers.
    pub containers: u32,

    /// Number of running containers.
    pub containers_running: u32,

    /// Number of images.
    pub images: u32,
}

/// Extension trait for convenient container operations.
pub trait ContainerRuntimeExt: ContainerRuntime {
    /// Create and start a container in one operation.
    ///
    /// # Errors
    ///
    /// Returns error if create or start fails.
    fn run<'a>(
        &'a self,
        config: &'a ContainerConfig,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<ContainerId>> + Send + 'a>> {
        Box::pin(async move {
            let id = self.create(config).await?;
            self.start(&id).await?;
            Ok(id)
        })
    }

    /// Stop and remove a container.
    ///
    /// # Errors
    ///
    /// Returns error if stop or remove fails.
    fn stop_and_remove<'a>(
        &'a self,
        id: &'a ContainerId,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<()>> + Send + 'a>> {
        Box::pin(async move {
            // Try to stop, ignore error if already stopped
            let stop_opts = StopOptions::default();
            let _ = self.stop(id, &stop_opts).await;
            let remove_opts = RemoveOptions::force();
            self.remove(id, &remove_opts).await
        })
    }

    /// Check if a container exists.
    fn exists<'a>(
        &'a self,
        id: &'a ContainerId,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.status(id).await.is_ok() })
    }

    /// Wait for container to be in a specific state.
    fn wait_for_state<'a>(
        &'a self,
        id: &'a ContainerId,
        target_state: ContainerState,
        timeout_secs: u64,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<()>> + Send + 'a>> {
        Box::pin(async move {
            let start = std::time::Instant::now();
            let timeout = std::time::Duration::from_secs(timeout_secs);

            loop {
                let status = self.status(id).await?;
                if status.state == target_state {
                    return Ok(());
                }

                if start.elapsed() >= timeout {
                    return Err(ContainerError::Timeout {
                        seconds: timeout_secs,
                    });
                }

                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        })
    }
}

// Blanket implementation for all ContainerRuntime implementations
impl<T: ContainerRuntime> ContainerRuntimeExt for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_options_all() {
        let opts = ListOptions::all();
        assert!(opts.all);
    }

    #[test]
    fn test_list_options_running() {
        let opts = ListOptions::running();
        assert!(!opts.all);
        assert_eq!(opts.status_filter, Some(ContainerState::Running));
    }

    #[test]
    fn test_list_options_with_label() {
        let opts = ListOptions::all().with_label("app=test");
        assert_eq!(opts.label_filter, Some("app=test".to_string()));
    }

    #[test]
    fn test_stop_options_default() {
        let opts = StopOptions::default();
        assert_eq!(opts.timeout_secs, Some(10));
    }

    #[test]
    fn test_stop_options_force() {
        let opts = StopOptions::force();
        assert_eq!(opts.timeout_secs, Some(0));
        assert_eq!(opts.signal, Some("SIGKILL".to_string()));
    }

    #[test]
    fn test_remove_options_force() {
        let opts = RemoveOptions::force();
        assert!(opts.force);
    }

    #[test]
    fn test_logs_options_all() {
        let opts = LogsOptions::all();
        assert!(opts.stdout);
        assert!(opts.stderr);
        assert!(!opts.follow);
    }

    #[test]
    fn test_logs_options_follow() {
        let opts = LogsOptions::follow();
        assert!(opts.follow);
    }

    #[test]
    fn test_logs_options_tail() {
        let opts = LogsOptions::tail(100);
        assert_eq!(opts.tail, Some(100));
    }

    #[test]
    fn test_exec_options_cmd() {
        let opts = ExecOptions::cmd(vec!["ls".to_string(), "-la".to_string()]);
        assert_eq!(opts.cmd, vec!["ls", "-la"]);
        assert!(opts.attach_stdout);
    }

    #[test]
    fn test_exec_options_with_env() {
        let opts = ExecOptions::cmd(vec!["env".to_string()]).with_env("FOO=bar");
        assert_eq!(opts.env, vec!["FOO=bar"]);
    }

    #[test]
    fn test_exec_result_success() {
        let result = ExecResult {
            exit_code: 0,
            stdout: b"hello".to_vec(),
            stderr: Vec::new(),
        };
        assert!(result.success());
        assert_eq!(result.stdout_str(), "hello");
    }

    #[test]
    fn test_exec_result_failure() {
        let result = ExecResult {
            exit_code: 1,
            stdout: Vec::new(),
            stderr: b"error".to_vec(),
        };
        assert!(!result.success());
        assert_eq!(result.stderr_str(), "error");
    }

    #[test]
    fn test_runtime_info_default() {
        let info = RuntimeInfo::default();
        assert!(info.name.is_empty());
        assert!(!info.gpu_available);
    }
}
