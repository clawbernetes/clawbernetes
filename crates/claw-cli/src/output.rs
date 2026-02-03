//! Output formatting for CLI commands.
//!
//! Supports table (human-readable) and JSON output formats.

use std::io::Write;

use serde::Serialize;

use crate::cli::Format;
use crate::error::CliError;

/// Output formatter that handles both table and JSON output.
#[derive(Debug, Clone)]
pub struct OutputFormat {
    format: Format,
}

impl OutputFormat {
    /// Create a new output formatter.
    #[must_use]
    pub const fn new(format: Format) -> Self {
        Self { format }
    }

    /// Get the current format.
    #[must_use]
    pub const fn format(&self) -> Format {
        self.format
    }

    /// Check if JSON format is selected.
    #[must_use]
    pub const fn is_json(&self) -> bool {
        matches!(self.format, Format::Json)
    }

    /// Write a serializable value to the output.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or writing fails.
    pub fn write<W, T>(&self, writer: &mut W, value: &T) -> Result<(), CliError>
    where
        W: Write,
        T: Serialize + TableDisplay,
    {
        match self.format {
            Format::Json => {
                serde_json::to_writer_pretty(&mut *writer, value)
                    .map_err(|e| CliError::Format(format!("JSON serialization failed: {e}")))?;
                writeln!(writer)?;
            }
            Format::Table => {
                value.write_table(writer)?;
            }
        }
        Ok(())
    }

    /// Write a serializable value to a string.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_string<T>(&self, value: &T) -> Result<String, CliError>
    where
        T: Serialize + TableDisplay,
    {
        let mut buf = Vec::new();
        self.write(&mut buf, value)?;
        String::from_utf8(buf).map_err(|e| CliError::Format(format!("UTF-8 error: {e}")))
    }
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self::new(Format::Table)
    }
}

/// Trait for types that can be displayed as a table.
pub trait TableDisplay {
    /// Write the value as a human-readable table.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails.
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError>;
}

/// Cluster status information.
#[derive(Debug, Clone, Serialize)]
pub struct ClusterStatus {
    /// Total number of nodes in the cluster.
    pub node_count: usize,
    /// Number of healthy nodes.
    pub healthy_nodes: usize,
    /// Total number of GPUs.
    pub gpu_count: usize,
    /// Number of active workloads.
    pub active_workloads: usize,
    /// Total VRAM in MiB.
    pub total_vram_mib: u64,
    /// Gateway version.
    pub gateway_version: String,
}

impl TableDisplay for ClusterStatus {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        writeln!(writer, "Cluster Status")?;
        writeln!(writer, "══════════════════════════════════")?;
        writeln!(writer, "Gateway Version:  {}", self.gateway_version)?;
        writeln!(writer)?;
        writeln!(writer, "Nodes")?;
        writeln!(writer, "  Total:          {}", self.node_count)?;
        writeln!(writer, "  Healthy:        {}", self.healthy_nodes)?;
        writeln!(writer)?;
        writeln!(writer, "GPUs")?;
        writeln!(writer, "  Count:          {}", self.gpu_count)?;
        writeln!(writer, "  Total VRAM:     {} MiB", self.total_vram_mib)?;
        writeln!(writer)?;
        writeln!(writer, "Workloads")?;
        writeln!(writer, "  Active:         {}", self.active_workloads)?;
        Ok(())
    }
}

/// Node information for listing.
#[derive(Debug, Clone, Serialize)]
pub struct NodeInfo {
    /// Node ID.
    pub id: String,
    /// Node hostname.
    pub hostname: String,
    /// Node status.
    pub status: String,
    /// Number of GPUs.
    pub gpu_count: usize,
    /// Total VRAM in MiB.
    pub vram_mib: u64,
    /// Number of running workloads.
    pub workloads: usize,
}

/// List of nodes for display.
#[derive(Debug, Clone, Serialize)]
pub struct NodeList {
    /// Nodes in the cluster.
    pub nodes: Vec<NodeInfo>,
}

impl TableDisplay for NodeList {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.nodes.is_empty() {
            writeln!(writer, "No nodes in cluster")?;
            return Ok(());
        }

        // Header
        writeln!(
            writer,
            "{:<36}  {:<16}  {:<10}  {:>4}  {:>10}  {:>8}",
            "ID", "HOSTNAME", "STATUS", "GPUS", "VRAM (MiB)", "WORKLOADS"
        )?;
        writeln!(writer, "{}", "─".repeat(96))?;

        // Rows
        for node in &self.nodes {
            writeln!(
                writer,
                "{:<36}  {:<16}  {:<10}  {:>4}  {:>10}  {:>8}",
                node.id,
                truncate(&node.hostname, 16),
                node.status,
                node.gpu_count,
                node.vram_mib,
                node.workloads
            )?;
        }

        writeln!(writer)?;
        writeln!(writer, "Total: {} node(s)", self.nodes.len())?;
        Ok(())
    }
}

/// Detailed node information.
#[derive(Debug, Clone, Serialize)]
pub struct NodeDetail {
    /// Node ID.
    pub id: String,
    /// Node hostname.
    pub hostname: String,
    /// Node status.
    pub status: String,
    /// CPU cores.
    pub cpu_cores: u32,
    /// Memory in MiB.
    pub memory_mib: u64,
    /// GPU information.
    pub gpus: Vec<GpuDetail>,
    /// Running workloads.
    pub workloads: Vec<WorkloadSummary>,
}

/// GPU detail information.
#[derive(Debug, Clone, Serialize)]
pub struct GpuDetail {
    /// GPU index.
    pub index: u32,
    /// GPU name/model.
    pub name: String,
    /// Total VRAM in MiB.
    pub memory_mib: u64,
    /// GPU UUID.
    pub uuid: String,
    /// Current utilization percentage.
    pub utilization_percent: Option<u8>,
    /// Current temperature in Celsius.
    pub temperature_celsius: Option<u32>,
}

/// Workload summary information.
#[derive(Debug, Clone, Serialize)]
pub struct WorkloadSummary {
    /// Workload ID.
    pub id: String,
    /// Container image.
    pub image: String,
    /// Workload state.
    pub state: String,
}

impl TableDisplay for NodeDetail {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        writeln!(writer, "Node: {}", self.id)?;
        writeln!(writer, "══════════════════════════════════════════════════")?;
        writeln!(writer)?;
        writeln!(writer, "General")?;
        writeln!(writer, "  Hostname:       {}", self.hostname)?;
        writeln!(writer, "  Status:         {}", self.status)?;
        writeln!(writer, "  CPU Cores:      {}", self.cpu_cores)?;
        writeln!(writer, "  Memory:         {} MiB", self.memory_mib)?;
        writeln!(writer)?;

        if self.gpus.is_empty() {
            writeln!(writer, "GPUs: None")?;
        } else {
            writeln!(writer, "GPUs ({}):", self.gpus.len())?;
            for gpu in &self.gpus {
                writeln!(writer, "  [{}] {}", gpu.index, gpu.name)?;
                writeln!(writer, "      VRAM: {} MiB", gpu.memory_mib)?;
                if let Some(util) = gpu.utilization_percent {
                    writeln!(writer, "      Utilization: {util}%")?;
                }
                if let Some(temp) = gpu.temperature_celsius {
                    writeln!(writer, "      Temperature: {temp}°C")?;
                }
            }
        }

        writeln!(writer)?;

        if self.workloads.is_empty() {
            writeln!(writer, "Workloads: None")?;
        } else {
            writeln!(writer, "Workloads ({}):", self.workloads.len())?;
            for w in &self.workloads {
                writeln!(writer, "  {} ({}) - {}", w.id, w.state, w.image)?;
            }
        }

        Ok(())
    }
}

/// MOLT participation status.
#[derive(Debug, Clone, Serialize)]
pub struct MoltStatus {
    /// Whether participating in MOLT network.
    pub participating: bool,
    /// Current autonomy level.
    pub autonomy_level: Option<String>,
    /// Public key (if participating).
    pub public_key: Option<String>,
    /// Current balance.
    pub balance: Option<String>,
    /// Total earnings.
    pub total_earnings: Option<String>,
    /// Jobs completed.
    pub jobs_completed: Option<u64>,
}

impl TableDisplay for MoltStatus {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        writeln!(writer, "MOLT Network Status")?;
        writeln!(writer, "══════════════════════════════════")?;
        writeln!(writer)?;

        if self.participating {
            writeln!(writer, "Status:           ✓ Participating")?;
            if let Some(ref level) = self.autonomy_level {
                writeln!(writer, "Autonomy Level:   {level}")?;
            }
            if let Some(ref pk) = self.public_key {
                writeln!(writer, "Public Key:       {pk}")?;
            }
            if let Some(ref balance) = self.balance {
                writeln!(writer, "Balance:          {balance}")?;
            }
            if let Some(ref earnings) = self.total_earnings {
                writeln!(writer, "Total Earnings:   {earnings}")?;
            }
            if let Some(jobs) = self.jobs_completed {
                writeln!(writer, "Jobs Completed:   {jobs}")?;
            }
        } else {
            writeln!(writer, "Status:           ✗ Not participating")?;
            writeln!(writer)?;
            writeln!(writer, "Run 'clawbernetes molt join' to start participating.")?;
        }

        Ok(())
    }
}

/// MOLT earnings summary.
#[derive(Debug, Clone, Serialize)]
pub struct EarningsSummary {
    /// Total earnings.
    pub total: String,
    /// Earnings today.
    pub today: String,
    /// Earnings this week.
    pub this_week: String,
    /// Earnings this month.
    pub this_month: String,
    /// Jobs completed.
    pub jobs_completed: u64,
    /// Average earning per job.
    pub avg_per_job: String,
}

impl TableDisplay for EarningsSummary {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        writeln!(writer, "MOLT Earnings Summary")?;
        writeln!(writer, "══════════════════════════════════")?;
        writeln!(writer)?;
        writeln!(writer, "Total Earnings:   {}", self.total)?;
        writeln!(writer)?;
        writeln!(writer, "Breakdown")?;
        writeln!(writer, "  Today:          {}", self.today)?;
        writeln!(writer, "  This Week:      {}", self.this_week)?;
        writeln!(writer, "  This Month:     {}", self.this_month)?;
        writeln!(writer)?;
        writeln!(writer, "Statistics")?;
        writeln!(writer, "  Jobs Completed: {}", self.jobs_completed)?;
        writeln!(writer, "  Avg Per Job:    {}", self.avg_per_job)?;
        Ok(())
    }
}

/// Simple message output.
#[derive(Debug, Clone, Serialize)]
pub struct Message {
    /// Message text.
    pub message: String,
    /// Whether this is a success message.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub success: bool,
}

impl Message {
    /// Create a success message.
    #[must_use]
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            success: true,
        }
    }

    /// Create an informational message.
    #[must_use]
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            success: false,
        }
    }
}

impl TableDisplay for Message {
    fn write_table<W: Write>(&self, writer: &mut W) -> Result<(), CliError> {
        if self.success {
            writeln!(writer, "✓ {}", self.message)?;
        } else {
            writeln!(writer, "{}", self.message)?;
        }
        Ok(())
    }
}

/// Truncate a string to a maximum length.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len > 3 {
        format!("{}...", &s[..max_len - 3])
    } else {
        s[..max_len].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_format_default_is_table() {
        let fmt = OutputFormat::default();
        assert_eq!(fmt.format(), Format::Table);
        assert!(!fmt.is_json());
    }

    #[test]
    fn output_format_json() {
        let fmt = OutputFormat::new(Format::Json);
        assert_eq!(fmt.format(), Format::Json);
        assert!(fmt.is_json());
    }

    #[test]
    fn cluster_status_json_output() {
        let status = ClusterStatus {
            node_count: 5,
            healthy_nodes: 4,
            gpu_count: 12,
            active_workloads: 3,
            total_vram_mib: 245760,
            gateway_version: "0.1.0".into(),
        };

        let fmt = OutputFormat::new(Format::Json);
        let output = fmt.to_string(&status).expect("should format");
        
        assert!(output.contains("\"node_count\": 5"));
        assert!(output.contains("\"gpu_count\": 12"));
        assert!(output.contains("\"gateway_version\": \"0.1.0\""));
    }

    #[test]
    fn cluster_status_table_output() {
        let status = ClusterStatus {
            node_count: 5,
            healthy_nodes: 4,
            gpu_count: 12,
            active_workloads: 3,
            total_vram_mib: 245760,
            gateway_version: "0.1.0".into(),
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&status).expect("should format");
        
        assert!(output.contains("Cluster Status"));
        assert!(output.contains("Total:          5"));
        assert!(output.contains("Healthy:        4"));
        assert!(output.contains("Count:          12"));
    }

    #[test]
    fn node_list_empty() {
        let list = NodeList { nodes: vec![] };
        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&list).expect("should format");
        
        assert!(output.contains("No nodes in cluster"));
    }

    #[test]
    fn node_list_with_nodes() {
        let list = NodeList {
            nodes: vec![
                NodeInfo {
                    id: "node-abc-123".into(),
                    hostname: "gpu-worker-01".into(),
                    status: "healthy".into(),
                    gpu_count: 4,
                    vram_mib: 98304,
                    workloads: 2,
                },
                NodeInfo {
                    id: "node-def-456".into(),
                    hostname: "gpu-worker-02".into(),
                    status: "draining".into(),
                    gpu_count: 2,
                    vram_mib: 49152,
                    workloads: 0,
                },
            ],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&list).expect("should format");
        
        assert!(output.contains("node-abc-123"));
        assert!(output.contains("gpu-worker-01"));
        assert!(output.contains("healthy"));
        assert!(output.contains("Total: 2 node(s)"));
    }

    #[test]
    fn node_detail_table_output() {
        let detail = NodeDetail {
            id: "node-abc-123".into(),
            hostname: "gpu-worker-01".into(),
            status: "healthy".into(),
            cpu_cores: 32,
            memory_mib: 131072,
            gpus: vec![
                GpuDetail {
                    index: 0,
                    name: "NVIDIA RTX 4090".into(),
                    memory_mib: 24576,
                    uuid: "GPU-123".into(),
                    utilization_percent: Some(75),
                    temperature_celsius: Some(68),
                },
            ],
            workloads: vec![
                WorkloadSummary {
                    id: "wl-001".into(),
                    image: "pytorch:latest".into(),
                    state: "running".into(),
                },
            ],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&detail).expect("should format");
        
        assert!(output.contains("Node: node-abc-123"));
        assert!(output.contains("Hostname:       gpu-worker-01"));
        assert!(output.contains("CPU Cores:      32"));
        assert!(output.contains("NVIDIA RTX 4090"));
        assert!(output.contains("Utilization: 75%"));
        assert!(output.contains("Temperature: 68°C"));
    }

    #[test]
    fn node_detail_no_gpus() {
        let detail = NodeDetail {
            id: "node-cpu-only".into(),
            hostname: "cpu-worker".into(),
            status: "healthy".into(),
            cpu_cores: 64,
            memory_mib: 262144,
            gpus: vec![],
            workloads: vec![],
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&detail).expect("should format");
        
        assert!(output.contains("GPUs: None"));
        assert!(output.contains("Workloads: None"));
    }

    #[test]
    fn molt_status_participating() {
        let status = MoltStatus {
            participating: true,
            autonomy_level: Some("Moderate".into()),
            public_key: Some("5abc123...".into()),
            balance: Some("42.5 MOLT".into()),
            total_earnings: Some("150.25 MOLT".into()),
            jobs_completed: Some(47),
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&status).expect("should format");
        
        assert!(output.contains("✓ Participating"));
        assert!(output.contains("Moderate"));
        assert!(output.contains("42.5 MOLT"));
        assert!(output.contains("Jobs Completed:   47"));
    }

    #[test]
    fn molt_status_not_participating() {
        let status = MoltStatus {
            participating: false,
            autonomy_level: None,
            public_key: None,
            balance: None,
            total_earnings: None,
            jobs_completed: None,
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&status).expect("should format");
        
        assert!(output.contains("✗ Not participating"));
        assert!(output.contains("clawbernetes molt join"));
    }

    #[test]
    fn earnings_summary_table_output() {
        let earnings = EarningsSummary {
            total: "500.123 MOLT".into(),
            today: "12.5 MOLT".into(),
            this_week: "87.3 MOLT".into(),
            this_month: "342.1 MOLT".into(),
            jobs_completed: 156,
            avg_per_job: "3.21 MOLT".into(),
        };

        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&earnings).expect("should format");
        
        assert!(output.contains("MOLT Earnings Summary"));
        assert!(output.contains("Total Earnings:   500.123 MOLT"));
        assert!(output.contains("Today:          12.5 MOLT"));
        assert!(output.contains("Jobs Completed: 156"));
    }

    #[test]
    fn message_success() {
        let msg = Message::success("Node drained successfully");
        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&msg).expect("should format");
        
        assert!(output.contains("✓ Node drained successfully"));
    }

    #[test]
    fn message_info() {
        let msg = Message::info("Processing...");
        let fmt = OutputFormat::new(Format::Table);
        let output = fmt.to_string(&msg).expect("should format");
        
        assert!(output.contains("Processing..."));
        assert!(!output.contains("✓"));
    }

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_string() {
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn truncate_very_short_max() {
        assert_eq!(truncate("hello", 3), "hel");
    }

    #[test]
    fn json_output_with_write() {
        let status = ClusterStatus {
            node_count: 1,
            healthy_nodes: 1,
            gpu_count: 2,
            active_workloads: 0,
            total_vram_mib: 49152,
            gateway_version: "test".into(),
        };

        let fmt = OutputFormat::new(Format::Json);
        let mut buf = Vec::new();
        fmt.write(&mut buf, &status).expect("should write");
        
        let output = String::from_utf8(buf).expect("valid utf8");
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid json");
        
        assert_eq!(parsed["node_count"], 1);
        assert_eq!(parsed["gpu_count"], 2);
    }
}
