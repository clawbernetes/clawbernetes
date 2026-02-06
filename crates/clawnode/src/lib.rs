//! Clawnode - GPU Node Agent for Clawbernetes
//!
//! This agent runs on GPU servers and connects back to the OpenClaw gateway,
//! registering as a node with GPU capabilities.

pub mod client;
pub mod commands;
pub mod config;
pub mod gpu;

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
            "system.info".to_string(),
            "system.run".to_string(),
            "gpu.list".to_string(),
            "gpu.metrics".to_string(),
            "workload.run".to_string(),
            "workload.stop".to_string(),
            "workload.logs".to_string(),
            "container.exec".to_string(),
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
}

impl SharedState {
    pub fn new(config: NodeConfig) -> Self {
        let state = NodeState::new(config);
        let capabilities = state.capabilities.clone();
        let commands = state.commands.clone();
        
        Self {
            inner: Arc::new(RwLock::new(state)),
            capabilities,
            commands,
            node_token: None,
        }
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
