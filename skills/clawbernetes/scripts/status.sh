#!/usr/bin/env bash
#
# status.sh - Format Clawbernetes cluster status output
# 
# Usage: clawbernetes status --json | ./status.sh
#        ./status.sh [--watch]
#

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Check for jq
if ! command -v jq &> /dev/null; then
    echo "Error: jq is required but not installed." >&2
    exit 1
fi

format_bytes() {
    local bytes=$1
    if [ "$bytes" -ge 1099511627776 ]; then
        echo "$(echo "scale=1; $bytes / 1099511627776" | bc)TB"
    elif [ "$bytes" -ge 1073741824 ]; then
        echo "$(echo "scale=1; $bytes / 1073741824" | bc)GB"
    elif [ "$bytes" -ge 1048576 ]; then
        echo "$(echo "scale=1; $bytes / 1048576" | bc)MB"
    else
        echo "${bytes}B"
    fi
}

status_icon() {
    case "$1" in
        healthy|running|online) echo -e "${GREEN}●${NC}" ;;
        degraded|warning)       echo -e "${YELLOW}●${NC}" ;;
        offline|error|failed)   echo -e "${RED}●${NC}" ;;
        pending)                echo -e "${BLUE}○${NC}" ;;
        *)                      echo -e "○" ;;
    esac
}

print_header() {
    echo -e "\n${BOLD}${CYAN}═══════════════════════════════════════════════════════════════${NC}"
    echo -e "${BOLD}${CYAN}                    CLAWBERNETES CLUSTER STATUS${NC}"
    echo -e "${BOLD}${CYAN}═══════════════════════════════════════════════════════════════${NC}\n"
}

print_section() {
    echo -e "\n${BOLD}${BLUE}▸ $1${NC}"
    echo -e "${BLUE}─────────────────────────────────────────${NC}"
}

format_status() {
    local json="$1"
    
    print_header
    
    # Cluster Overview
    print_section "CLUSTER OVERVIEW"
    
    local cluster_status=$(echo "$json" | jq -r '.cluster.status // "unknown"')
    local total_nodes=$(echo "$json" | jq -r '.cluster.totalNodes // 0')
    local healthy_nodes=$(echo "$json" | jq -r '.cluster.healthyNodes // 0')
    local total_gpus=$(echo "$json" | jq -r '.cluster.totalGPUs // 0')
    local available_gpus=$(echo "$json" | jq -r '.cluster.availableGPUs // 0')
    
    echo -e "  Status:          $(status_icon "$cluster_status") ${cluster_status^^}"
    echo -e "  Nodes:           ${healthy_nodes}/${total_nodes} healthy"
    echo -e "  GPUs:            ${available_gpus}/${total_gpus} available"
    
    # GPU Utilization Bar
    if [ "$total_gpus" -gt 0 ]; then
        local used_gpus=$((total_gpus - available_gpus))
        local pct=$((used_gpus * 100 / total_gpus))
        local bar_width=30
        local filled=$((pct * bar_width / 100))
        local empty=$((bar_width - filled))
        
        printf "  Utilization:     ["
        printf "${GREEN}%${filled}s${NC}" | tr ' ' '█'
        printf "%${empty}s" | tr ' ' '░'
        printf "] %d%%\n" "$pct"
    fi
    
    # Node List
    print_section "NODES"
    
    printf "  ${BOLD}%-20s %-12s %-10s %-15s %s${NC}\n" "NAME" "STATUS" "GPUs" "GPU TYPE" "UTILIZATION"
    
    echo "$json" | jq -r '.nodes[]? | "\(.name)\t\(.status)\t\(.gpus.available)/\(.gpus.total)\t\(.gpus.type)\t\(.utilization)"' | \
    while IFS=$'\t' read -r name status gpus gpu_type util; do
        printf "  %-20s $(status_icon "$status") %-10s %-10s %-15s %s%%\n" \
            "$name" "$status" "$gpus" "$gpu_type" "$util"
    done
    
    # Running Workloads
    local workload_count=$(echo "$json" | jq -r '.workloads | length // 0')
    if [ "$workload_count" -gt 0 ]; then
        print_section "RUNNING WORKLOADS"
        
        printf "  ${BOLD}%-25s %-10s %-8s %-15s %s${NC}\n" "NAME" "STATUS" "GPUs" "RUNTIME" "NODE"
        
        echo "$json" | jq -r '.workloads[]? | "\(.name)\t\(.status)\t\(.gpus)\t\(.runtime)\t\(.node)"' | \
        while IFS=$'\t' read -r name status gpus runtime node; do
            printf "  %-25s $(status_icon "$status") %-8s %-8s %-15s %s\n" \
                "$name" "$status" "$gpus" "$runtime" "$node"
        done
    fi
    
    # MOLT Status (if participating)
    local molt_active=$(echo "$json" | jq -r '.molt.active // false')
    if [ "$molt_active" = "true" ]; then
        print_section "MOLT NETWORK"
        
        local molt_mode=$(echo "$json" | jq -r '.molt.mode // "unknown"')
        local molt_shared=$(echo "$json" | jq -r '.molt.sharedGPUs // 0')
        local molt_earnings=$(echo "$json" | jq -r '.molt.earnings24h // "0.00"')
        
        echo -e "  Mode:            ${molt_mode}"
        echo -e "  Shared GPUs:     ${molt_shared}"
        echo -e "  Earnings (24h):  \$${molt_earnings}"
    fi
    
    # Footer with timestamp
    echo -e "\n${CYAN}─────────────────────────────────────────${NC}"
    echo -e "  Last updated: $(date '+%Y-%m-%d %H:%M:%S %Z')"
    echo ""
}

# Watch mode
watch_mode() {
    while true; do
        clear
        clawbernetes status --json 2>/dev/null | format_status
        sleep "${WATCH_INTERVAL:-5}"
    done
}

# Main
main() {
    case "${1:-}" in
        --watch|-w)
            watch_mode
            ;;
        --help|-h)
            echo "Usage: $0 [--watch]"
            echo ""
            echo "Format Clawbernetes cluster status output."
            echo ""
            echo "Options:"
            echo "  --watch, -w    Continuously update status"
            echo "  --help, -h     Show this help"
            echo ""
            echo "Examples:"
            echo "  clawbernetes status --json | $0"
            echo "  $0 --watch"
            ;;
        *)
            # Read from stdin or fetch directly
            if [ -t 0 ]; then
                # No stdin, fetch status
                clawbernetes status --json 2>/dev/null | format_status
            else
                # Read from stdin
                format_status "$(cat)"
            fi
            ;;
    esac
}

main "$@"
