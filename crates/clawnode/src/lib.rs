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
    pub approved: bool,
}

impl NodeState {
    pub fn new(config: NodeConfig) -> Self {
        Self {
            config,
            gpu_manager: GpuManager::new(),
            connected: false,
            node_id: None,
            approved: false,
        }
    }
}

pub type SharedState = Arc<RwLock<NodeState>>;

/// Create shared state from config
pub fn create_state(config: NodeConfig) -> SharedState {
    Arc::new(RwLock::new(NodeState::new(config)))
}
