//! Application state for Clawbernetes TUI

use std::collections::VecDeque;
use serde::{Deserialize, Serialize};

/// Maximum number of activity items to keep
const MAX_ACTIVITY_ITEMS: usize = 100;

/// Main application state
#[derive(Debug, Default)]
pub struct App {
    /// Is the app running
    pub running: bool,
    
    /// Currently selected tab
    pub selected_tab: usize,
    
    /// Cluster state
    pub cluster: ClusterState,
    
    /// Node states
    pub nodes: Vec<NodeState>,
    
    /// Workload states
    pub workloads: Vec<WorkloadState>,
    
    /// Agent activity log
    pub activity: VecDeque<ActivityItem>,
    
    /// MOLT market data
    pub market: MarketState,
    
    /// Connection status
    pub connected: bool,
    
    /// Last update timestamp
    pub last_update: Option<i64>,
    
    /// Error message
    pub error: Option<String>,
    
    /// Scroll offset for activity log
    pub activity_scroll: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            running: true,
            selected_tab: 0,
            cluster: ClusterState::default(),
            nodes: Vec::new(),
            workloads: Vec::new(),
            activity: VecDeque::with_capacity(MAX_ACTIVITY_ITEMS),
            market: MarketState::default(),
            connected: false,
            last_update: None,
            error: None,
            activity_scroll: 0,
        }
    }
    
    pub fn add_activity(&mut self, item: ActivityItem) {
        self.activity.push_front(item);
        if self.activity.len() > MAX_ACTIVITY_ITEMS {
            self.activity.pop_back();
        }
    }
    
    pub fn next_tab(&mut self) {
        self.selected_tab = (self.selected_tab + 1) % 4;
    }
    
    pub fn prev_tab(&mut self) {
        if self.selected_tab > 0 {
            self.selected_tab -= 1;
        } else {
            self.selected_tab = 3;
        }
    }
    
    pub fn scroll_activity_up(&mut self) {
        if self.activity_scroll > 0 {
            self.activity_scroll -= 1;
        }
    }
    
    pub fn scroll_activity_down(&mut self) {
        if self.activity_scroll < self.activity.len().saturating_sub(1) {
            self.activity_scroll += 1;
        }
    }
}

/// Cluster-wide state
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ClusterState {
    pub total_nodes: u32,
    pub healthy_nodes: u32,
    pub total_gpus: u32,
    pub available_gpus: u32,
    pub total_memory_gb: u64,
    pub used_memory_gb: u64,
    pub running_workloads: u32,
    pub pending_workloads: u32,
    pub completed_workloads: u32,
    pub failed_workloads: u32,
}

/// Per-node state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeState {
    pub id: String,
    pub name: String,
    pub status: NodeStatus,
    pub gpus: Vec<GpuState>,
    pub cpu_percent: f32,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub workload_count: u32,
    pub last_heartbeat: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    Healthy,
    Unhealthy,
    Draining,
    Offline,
}

impl Default for NodeStatus {
    fn default() -> Self {
        Self::Healthy
    }
}

/// Per-GPU state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuState {
    pub index: u32,
    pub name: String,
    pub utilization_percent: u32,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub temperature_c: u32,
    pub power_draw_w: Option<f32>,
}

/// Workload state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadState {
    pub id: String,
    pub name: Option<String>,
    pub image: String,
    pub state: WorkloadStatus,
    pub gpu_count: u32,
    pub assigned_node: Option<String>,
    pub started_at: Option<i64>,
    pub progress_percent: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkloadStatus {
    Pending,
    Scheduling,
    Running,
    Completed,
    Failed,
}

/// Activity log item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityItem {
    pub timestamp: i64,
    pub event_type: ActivityType,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityType {
    Scale,
    Deploy,
    Preempt,
    NodeJoin,
    NodeLeave,
    Trade,
    Alert,
    Info,
}

impl ActivityType {
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Scale => "⚖",
            Self::Deploy => "🚀",
            Self::Preempt => "⏸",
            Self::NodeJoin => "➕",
            Self::NodeLeave => "➖",
            Self::Trade => "💰",
            Self::Alert => "⚠",
            Self::Info => "ℹ",
        }
    }
}

/// MOLT market state
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MarketState {
    pub spot_prices: Vec<SpotPrice>,
    pub active_offers: u32,
    pub active_bids: u32,
    pub recent_trades: VecDeque<Trade>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotPrice {
    pub gpu_model: String,
    pub price_per_hour: f64,
    pub change_percent: f64,
    pub available_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub timestamp: i64,
    pub gpu_model: String,
    pub gpu_count: u32,
    pub price_per_hour: f64,
    pub duration_hours: u32,
}
