#!/bin/bash
# diagnose.sh - Diagnostic helper for Clawbernetes
# Formats diagnosis output with colors and severity icons

set -euo pipefail

# Colors
RED='\033[0;31m'
YELLOW='\033[0;33m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m' # No Color

# Severity icons
ICON_OK="🟢"
ICON_WARN="🟡"
ICON_CRITICAL="🔴"
ICON_INFO="ℹ️"
ICON_ARROW="→"
ARROW="→"

# Usage
usage() {
    echo "Usage: $0 <type> <id> [options]"
    echo ""
    echo "Types:"
    echo "  node       Diagnose a specific node"
    echo "  workload   Diagnose a specific workload"
    echo "  cluster    Overall cluster health"
    echo ""
    echo "Options:"
    echo "  --json     Output as JSON"
    echo "  --verbose  Include detailed metrics"
    echo ""
    exit 1
}

# Print section header
section() {
    echo ""
    echo -e "${BOLD}${BLUE}━━━ $1 ━━━${NC}"
    echo ""
}

# Print status line with severity
status_line() {
    local severity="$1"
    local label="$2"
    local value="$3"
    local detail="${4:-}"
    
    local icon=""
    local color=""
    
    case "$severity" in
        ok|healthy|good)
            icon="$ICON_OK"
            color="$GREEN"
            ;;
        warn|warning|degraded)
            icon="$ICON_WARN"
            color="$YELLOW"
            ;;
        critical|error|bad)
            icon="$ICON_CRITICAL"
            color="$RED"
            ;;
        info|*)
            icon="$ICON_INFO"
            color="$CYAN"
            ;;
    esac
    
    printf "  %s %-20s ${color}%s${NC}" "$icon" "$label:" "$value"
    if [[ -n "$detail" ]]; then
        printf " ${DIM}(%s)${NC}" "$detail"
    fi
    echo ""
}

# Print metric with trend
metric_line() {
    local label="$1"
    local value="$2"
    local unit="$3"
    local threshold_warn="${4:-}"
    local threshold_crit="${5:-}"
    
    local severity="ok"
    if [[ -n "$threshold_crit" ]] && (( $(echo "$value >= $threshold_crit" | bc -l) )); then
        severity="critical"
    elif [[ -n "$threshold_warn" ]] && (( $(echo "$value >= $threshold_warn" | bc -l) )); then
        severity="warn"
    fi
    
    status_line "$severity" "$label" "${value}${unit}"
}

# Print recommendation box
recommendation() {
    local severity="$1"
    local title="$2"
    local description="$3"
    local action="${4:-}"
    
    local icon=""
    local border_color=""
    
    case "$severity" in
        critical)
            icon="$ICON_CRITICAL"
            border_color="$RED"
            ;;
        warn)
            icon="$ICON_WARN"
            border_color="$YELLOW"
            ;;
        *)
            icon="$ICON_INFO"
            border_color="$CYAN"
            ;;
    esac
    
    echo ""
    echo -e "${border_color}┌─────────────────────────────────────────────────────────────┐${NC}"
    echo -e "${border_color}│${NC} $icon ${BOLD}$title${NC}"
    echo -e "${border_color}│${NC}"
    echo -e "${border_color}│${NC}   $description"
    if [[ -n "$action" ]]; then
        echo -e "${border_color}│${NC}"
        echo -e "${border_color}│${NC}   ${BOLD}Action:${NC} $action"
    fi
    echo -e "${border_color}└─────────────────────────────────────────────────────────────┘${NC}"
}

# Diagnose node
diagnose_node() {
    local node_id="$1"
    local verbose="${2:-false}"
    
    echo -e "${BOLD}${CYAN}╔════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}${CYAN}║  Node Diagnostics: ${node_id}${NC}"
    echo -e "${BOLD}${CYAN}╚════════════════════════════════════════════════════════════╝${NC}"
    
    # Fetch node data (simulated - replace with actual API call)
    # In production: clawbernetes node info "$node_id" --json
    
    section "GPU Status"
    status_line "ok" "GPU Model" "NVIDIA A100 80GB"
    metric_line "GPU Utilization" "87" "%" 80 95
    metric_line "GPU Memory" "68" "GB" 70 78
    metric_line "GPU Temperature" "72" "°C" 75 85
    status_line "ok" "Throttling" "None"
    
    section "Memory & CPU"
    metric_line "RAM Usage" "248" "GB / 512GB" 400 480
    metric_line "CPU Usage" "45" "%" 70 90
    status_line "ok" "Swap Activity" "0 MB/s"
    
    section "Storage & Network"
    metric_line "Disk Usage" "2.1" "TB / 4TB" 3.2 3.8
    metric_line "Disk IOPS" "12450" "" 15000 18000
    metric_line "Network In" "2.4" "Gbps"
    metric_line "Network Out" "1.8" "Gbps"
    
    section "Recent Events (24h)"
    echo -e "  ${DIM}[12:34:15]${NC} ${YELLOW}WARN${NC}  GPU temperature spike to 78°C (5 min)"
    echo -e "  ${DIM}[08:22:03]${NC} ${GREEN}INFO${NC}  Workload wl-xyz123 completed successfully"
    echo -e "  ${DIM}[02:15:44]${NC} ${GREEN}INFO${NC}  MOLT: Accepted 2 external jobs"
    
    section "Health Summary"
    status_line "ok" "Overall Status" "Healthy" "Score: 94/100"
    
    # Check for issues and provide recommendations
    if [[ "$verbose" == "true" ]]; then
        recommendation "info" "Optimization Opportunity" \
            "GPU memory usage is moderate. Consider running additional workloads." \
            "clawbernetes molt join --mode moderate"
    fi
}

# Diagnose workload
diagnose_workload() {
    local workload_id="$1"
    local verbose="${2:-false}"
    
    echo -e "${BOLD}${CYAN}╔════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}${CYAN}║  Workload Diagnostics: ${workload_id}${NC}"
    echo -e "${BOLD}${CYAN}╚════════════════════════════════════════════════════════════╝${NC}"
    
    section "Workload Info"
    status_line "info" "Name" "gpt-finetune-exp1"
    status_line "ok" "Status" "Running" "2h 34m elapsed"
    status_line "info" "Node" "node-gpu-a100-01"
    status_line "info" "GPUs Allocated" "4"
    
    section "Resource Utilization"
    metric_line "GPU Utilization" "92" "%" 0 0
    metric_line "GPU Memory" "72" "GB / 80GB" 75 78
    metric_line "CPU Usage" "34" "%"
    metric_line "RAM Usage" "48" "GB / 64GB" 56 62
    
    section "Performance Metrics"
    status_line "ok" "Throughput" "847 samples/sec" "↑12% from baseline"
    status_line "ok" "Loss Trend" "Decreasing" "0.342 → 0.198"
    status_line "info" "ETA" "4h 26m" "Based on current rate"
    
    section "Bottleneck Analysis"
    echo -e "  ${ICON_OK} GPU compute: ${GREEN}Not bottlenecked${NC}"
    echo -e "  ${ICON_OK} GPU memory:  ${GREEN}Not bottlenecked${NC}"
    echo -e "  ${ICON_WARN} Data I/O:    ${YELLOW}Potential bottleneck${NC} (loader wait: 12%)"
    echo -e "  ${ICON_OK} Network:     ${GREEN}Not bottlenecked${NC}"
    
    section "Recent Logs (Errors/Warnings)"
    echo -e "  ${DIM}No errors in last 24h${NC}"
    
    section "Health Summary"
    status_line "ok" "Overall Status" "Healthy" "Score: 91/100"
    
    recommendation "warn" "Data Pipeline Optimization" \
        "DataLoader is causing 12% GPU idle time. Consider increasing num_workers." \
        "Modify your training script: num_workers=8"
}

# Cluster health overview
diagnose_cluster() {
    echo -e "${BOLD}${CYAN}╔════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}${CYAN}║  Cluster Health Overview${NC}"
    echo -e "${BOLD}${CYAN}╚════════════════════════════════════════════════════════════╝${NC}"
    
    section "Cluster Summary"
    status_line "ok" "Cluster Status" "Healthy" "Score: 96/100"
    status_line "info" "Total Nodes" "12"
    echo ""
    echo -e "  Node Status:    ${ICON_OK} 10 Healthy   ${ICON_WARN} 2 Degraded   ${ICON_CRITICAL} 0 Critical"
    echo ""
    status_line "info" "Total GPUs" "48 (44 available)"
    status_line "info" "Active Workloads" "23"
    
    section "Resource Utilization"
    metric_line "Avg GPU Usage" "78" "%" 85 95
    metric_line "Avg Memory" "62" "%" 80 90
    metric_line "Network Load" "45" "%" 70 85
    
    section "Nodes Requiring Attention"
    echo -e "  ${ICON_WARN} ${BOLD}node-gpu-h100-03${NC}"
    echo -e "     GPU temperature elevated (82°C)"
    echo -e "     ${ARROW} Run: clawbernetes diagnose node node-gpu-h100-03"
    echo ""
    echo -e "  ${ICON_WARN} ${BOLD}node-gpu-a100-07${NC}"
    echo -e "     High memory pressure (94% RAM)"
    echo -e "     ${ARROW} Run: clawbernetes diagnose node node-gpu-a100-07"
    
    section "Active Alerts"
    echo -e "  ${ICON_WARN} 2 thermal warnings (non-critical)"
    echo -e "  ${ICON_INFO} 1 maintenance scheduled (node-gpu-a100-02, tomorrow 02:00)"
    
    section "MOLT Network"
    status_line "ok" "MOLT Status" "Active"
    status_line "info" "GPUs Shared" "8"
    status_line "info" "24h Earnings" "0.0847 ETH"
}

# Main
main() {
    if [[ $# -lt 1 ]]; then
        usage
    fi
    
    local type="$1"
    local id="${2:-}"
    local verbose=false
    
    # Parse options
    for arg in "$@"; do
        case "$arg" in
            --verbose)
                verbose=true
                ;;
            --json)
                echo "JSON output not yet implemented"
                exit 1
                ;;
        esac
    done
    
    case "$type" in
        node)
            if [[ -z "$id" ]]; then
                echo "Error: Node ID required"
                usage
            fi
            diagnose_node "$id" "$verbose"
            ;;
        workload)
            if [[ -z "$id" ]]; then
                echo "Error: Workload ID required"
                usage
            fi
            diagnose_workload "$id" "$verbose"
            ;;
        cluster|health)
            diagnose_cluster
            ;;
        *)
            echo "Unknown diagnostic type: $type"
            usage
            ;;
    esac
}

main "$@"
