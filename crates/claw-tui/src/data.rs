//! Data client for fetching cluster state
//!
//! Connects to claw-bridge and polls for updates.
//! This is READ-ONLY - no mutations are possible through this client.

use crate::app::{
    ActivityItem, ActivityType, ClusterState, GpuState, MarketState, NodeState, NodeStatus,
    SpotPrice, Trade, WorkloadState, WorkloadStatus,
};
use crate::events::{AppEvent, DataEvent};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// JSON-RPC request
#[derive(Debug, Serialize)]
struct RpcRequest {
    id: u64,
    method: String,
    params: Value,
}

/// JSON-RPC response
#[derive(Debug, Deserialize)]
struct RpcResponse {
    id: u64,
    result: Option<Value>,
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    code: i32,
    message: String,
}

/// Data client that polls the bridge for updates
pub struct DataClient {
    bridge_path: String,
    tx: mpsc::UnboundedSender<AppEvent>,
}

impl DataClient {
    pub fn new(bridge_path: String, tx: mpsc::UnboundedSender<AppEvent>) -> Self {
        Self { bridge_path, tx }
    }
    
    /// Start polling for data updates
    pub async fn run(&self, poll_interval: Duration) {
        let mut interval = tokio::time::interval(poll_interval);
        let mut request_id: u64 = 0;
        
        loop {
            interval.tick().await;
            
            // Fetch cluster status
            request_id += 1;
            if let Some(data) = self.call_bridge(request_id, "cluster_status", json!({})).await {
                let cluster = parse_cluster_state(&data);
                self.send_update(DataEvent::ClusterUpdate(json!(cluster)));
            }
            
            // Fetch node list
            request_id += 1;
            if let Some(data) = self.call_bridge(request_id, "node_list", json!({})).await {
                self.send_update(DataEvent::NodeUpdate(data));
            }
            
            // Fetch workload list
            request_id += 1;
            if let Some(data) = self.call_bridge(request_id, "workload_list", json!({})).await {
                self.send_update(DataEvent::WorkloadUpdate(data));
            }
            
            // Fetch MOLT prices
            request_id += 1;
            if let Some(data) = self.call_bridge(request_id, "molt_spot_prices", json!({})).await {
                self.send_update(DataEvent::MarketUpdate(data));
            }
            
            // Fetch alerts
            request_id += 1;
            if let Some(data) = self.call_bridge(request_id, "alert_list", json!({})).await {
                if let Some(alerts) = data.as_array() {
                    for alert in alerts {
                        if let Some(msg) = alert.get("message").and_then(|v| v.as_str()) {
                            self.send_update(DataEvent::Activity(json!({
                                "type": "alert",
                                "message": msg,
                            })));
                        }
                    }
                }
            }
        }
    }
    
    async fn call_bridge(&self, id: u64, method: &str, params: Value) -> Option<Value> {
        let request = RpcRequest {
            id,
            method: method.to_string(),
            params,
        };
        
        let input = serde_json::to_string(&request).ok()?;
        
        // Call bridge binary
        let output = tokio::process::Command::new(&self.bridge_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        
        let mut child = output;
        
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(input.as_bytes()).await.ok()?;
            stdin.flush().await.ok()?;
            drop(stdin);
        }
        
        let output = child.wait_with_output().await.ok()?;
        
        if output.status.success() {
            let response: RpcResponse = serde_json::from_slice(&output.stdout).ok()?;
            if response.error.is_some() {
                debug!(method = method, "RPC error: {:?}", response.error);
                return None;
            }
            response.result
        } else {
            None
        }
    }
    
    fn send_update(&self, event: DataEvent) {
        let _ = self.tx.send(AppEvent::DataUpdate(event));
    }
}

/// Parse cluster state from JSON
fn parse_cluster_state(data: &Value) -> ClusterState {
    ClusterState {
        total_nodes: data.get("total_nodes").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        healthy_nodes: data.get("healthy_nodes").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        total_gpus: data.get("total_gpus").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        available_gpus: data.get("available_gpus").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        total_memory_gb: data.get("total_memory_gb").and_then(|v| v.as_u64()).unwrap_or(0),
        used_memory_gb: data.get("used_memory_gb").and_then(|v| v.as_u64()).unwrap_or(0),
        running_workloads: data.get("running_workloads").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        pending_workloads: data.get("pending_workloads").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        completed_workloads: data.get("completed_workloads").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        failed_workloads: data.get("failed_workloads").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
    }
}

/// Update application state from data events
pub fn apply_data_event(app: &mut crate::app::App, event: DataEvent) {
    match event {
        DataEvent::ClusterUpdate(data) => {
            app.cluster = parse_cluster_state(&data);
            app.last_update = Some(chrono::Utc::now().timestamp_millis());
            app.connected = true;
        }
        DataEvent::NodeUpdate(data) => {
            if let Some(nodes) = data.as_array() {
                app.nodes = nodes.iter().filter_map(parse_node_state).collect();
            }
        }
        DataEvent::WorkloadUpdate(data) => {
            if let Some(workloads) = data.as_array() {
                app.workloads = workloads.iter().filter_map(parse_workload_state).collect();
            }
        }
        DataEvent::GpuMetrics(_data) => {
            // GPU metrics are embedded in node updates
        }
        DataEvent::Activity(data) => {
            if let Some(item) = parse_activity_item(&data) {
                app.add_activity(item);
            }
        }
        DataEvent::MarketUpdate(data) => {
            if let Some(prices) = data.as_array() {
                app.market.spot_prices = prices.iter().filter_map(parse_spot_price).collect();
            }
        }
        DataEvent::Connected => {
            app.connected = true;
            app.error = None;
        }
        DataEvent::Disconnected => {
            app.connected = false;
        }
        DataEvent::Error(msg) => {
            app.error = Some(msg);
        }
    }
}

fn parse_node_state(data: &Value) -> Option<NodeState> {
    Some(NodeState {
        id: data.get("id").and_then(|v| v.as_str())?.to_string(),
        name: data.get("name").and_then(|v| v.as_str())
            .or_else(|| data.get("id").and_then(|v| v.as_str()))?
            .to_string(),
        status: match data.get("status").and_then(|v| v.as_str()) {
            Some("healthy") => NodeStatus::Healthy,
            Some("unhealthy") => NodeStatus::Unhealthy,
            Some("draining") => NodeStatus::Draining,
            Some("offline") => NodeStatus::Offline,
            _ => NodeStatus::Healthy,
        },
        gpus: data.get("gpus")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(parse_gpu_state).collect())
            .unwrap_or_default(),
        cpu_percent: data.get("cpu_percent").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        memory_used_mb: data.get("memory_used_mb").and_then(|v| v.as_u64()).unwrap_or(0),
        memory_total_mb: data.get("memory_total_mb").and_then(|v| v.as_u64()).unwrap_or(0),
        workload_count: data.get("workload_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        last_heartbeat: data.get("last_heartbeat").and_then(|v| v.as_i64()),
    })
}

fn parse_gpu_state(data: &Value) -> Option<GpuState> {
    Some(GpuState {
        index: data.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        name: data.get("name").and_then(|v| v.as_str()).unwrap_or("GPU").to_string(),
        utilization_percent: data.get("utilization_percent").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        memory_used_mb: data.get("memory_used_mb").and_then(|v| v.as_u64()).unwrap_or(0),
        memory_total_mb: data.get("memory_total_mb").and_then(|v| v.as_u64()).unwrap_or(0),
        temperature_c: data.get("temperature_c").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        power_draw_w: data.get("power_draw_w").and_then(|v| v.as_f64()).map(|f| f as f32),
    })
}

fn parse_workload_state(data: &Value) -> Option<WorkloadState> {
    Some(WorkloadState {
        id: data.get("id").and_then(|v| v.as_str())?.to_string(),
        name: data.get("name").and_then(|v| v.as_str()).map(String::from),
        image: data.get("image").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
        state: match data.get("state").and_then(|v| v.as_str()) {
            Some("pending") => WorkloadStatus::Pending,
            Some("scheduling") => WorkloadStatus::Scheduling,
            Some("running") => WorkloadStatus::Running,
            Some("completed") => WorkloadStatus::Completed,
            Some("failed") => WorkloadStatus::Failed,
            _ => WorkloadStatus::Pending,
        },
        gpu_count: data.get("gpu_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        assigned_node: data.get("assigned_node").and_then(|v| v.as_str()).map(String::from),
        started_at: data.get("started_at").and_then(|v| v.as_i64()),
        progress_percent: data.get("progress_percent").and_then(|v| v.as_u64()).map(|v| v as u32),
    })
}

fn parse_activity_item(data: &Value) -> Option<ActivityItem> {
    let event_type = match data.get("type").and_then(|v| v.as_str())? {
        "scale" => ActivityType::Scale,
        "deploy" => ActivityType::Deploy,
        "preempt" => ActivityType::Preempt,
        "node_join" => ActivityType::NodeJoin,
        "node_leave" => ActivityType::NodeLeave,
        "trade" => ActivityType::Trade,
        "alert" => ActivityType::Alert,
        _ => ActivityType::Info,
    };
    
    Some(ActivityItem {
        timestamp: chrono::Utc::now().timestamp_millis(),
        event_type,
        message: data.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        details: data.get("details").and_then(|v| v.as_str()).map(String::from),
    })
}

fn parse_spot_price(data: &Value) -> Option<SpotPrice> {
    Some(SpotPrice {
        gpu_model: data.get("gpu_model").and_then(|v| v.as_str())?.to_string(),
        price_per_hour: data.get("price_per_hour").and_then(|v| v.as_f64()).unwrap_or(0.0),
        change_percent: data.get("change_percent").and_then(|v| v.as_f64()).unwrap_or(0.0),
        available_count: data.get("available_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
    })
}
