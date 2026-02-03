#!/usr/bin/env bash
#
# deploy.sh - Clawbernetes Deployment Helper
# 
# Provides interactive deployment with progress tracking,
# status visualization, and rollback confirmations.
#
# Usage:
#   ./deploy.sh deploy <intent>
#   ./deploy.sh status <deployment-id>
#   ./deploy.sh promote <deployment-id>
#   ./deploy.sh rollback <deployment-id> [--to <version>] [--immediate]
#   ./deploy.sh history <workload>

set -euo pipefail

# Colors and formatting
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m' # No Color

# Status icons
ICON_SUCCESS="✅"
ICON_FAILED="❌"
ICON_PENDING="⏳"
ICON_RUNNING="🔄"
ICON_CANARY="🐤"
ICON_ROLLBACK="⏪"
ICON_WARNING="⚠️"
ICON_INFO="ℹ️"
ICON_ROCKET="🚀"
ICON_CHECK="✓"
ICON_CROSS="✗"

# Progress bar characters
PROGRESS_FULL="█"
PROGRESS_EMPTY="░"
PROGRESS_PARTIAL="▓"

# Configuration
GATEWAY_URL="${CLAWBERNETES_GATEWAY_URL:-ws://localhost:9000}"
POLL_INTERVAL="${DEPLOY_POLL_INTERVAL:-5}"

# ============================================================================
# Helper Functions
# ============================================================================

log_info() {
    echo -e "${BLUE}${ICON_INFO}${NC} $1"
}

log_success() {
    echo -e "${GREEN}${ICON_SUCCESS}${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}${ICON_WARNING}${NC} $1"
}

log_error() {
    echo -e "${RED}${ICON_FAILED}${NC} $1"
}

# Print a progress bar
# Usage: progress_bar <current> <total> [width]
progress_bar() {
    local current=$1
    local total=$2
    local width=${3:-40}
    
    if [[ $total -eq 0 ]]; then
        total=1
    fi
    
    local percentage=$((current * 100 / total))
    local filled=$((current * width / total))
    local empty=$((width - filled))
    
    printf "${CYAN}["
    printf "%${filled}s" | tr ' ' "$PROGRESS_FULL"
    printf "%${empty}s" | tr ' ' "$PROGRESS_EMPTY"
    printf "]${NC} ${BOLD}%3d%%${NC}" "$percentage"
}

# Print deployment phase with appropriate icon
print_phase() {
    local phase=$1
    local icon=""
    local color=""
    
    case $phase in
        "pending")
            icon="$ICON_PENDING"
            color="$YELLOW"
            ;;
        "rolling"|"updating")
            icon="$ICON_RUNNING"
            color="$BLUE"
            ;;
        "canary")
            icon="$ICON_CANARY"
            color="$MAGENTA"
            ;;
        "promoting")
            icon="$ICON_RUNNING"
            color="$CYAN"
            ;;
        "complete"|"success")
            icon="$ICON_SUCCESS"
            color="$GREEN"
            ;;
        "failed"|"error")
            icon="$ICON_FAILED"
            color="$RED"
            ;;
        "rolling_back"|"rollback")
            icon="$ICON_ROLLBACK"
            color="$YELLOW"
            ;;
        *)
            icon="$ICON_INFO"
            color="$NC"
            ;;
    esac
    
    echo -e "${color}${icon} ${phase}${NC}"
}

# Format duration in human-readable form
format_duration() {
    local seconds=$1
    
    if [[ $seconds -lt 60 ]]; then
        echo "${seconds}s"
    elif [[ $seconds -lt 3600 ]]; then
        echo "$((seconds / 60))m $((seconds % 60))s"
    else
        echo "$((seconds / 3600))h $((seconds % 3600 / 60))m"
    fi
}

# Spinner animation for waiting
spinner() {
    local pid=$1
    local delay=0.1
    local spinstr='⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'
    
    while kill -0 "$pid" 2>/dev/null; do
        for i in $(seq 0 9); do
            printf "\r${CYAN}%s${NC} " "${spinstr:$i:1}"
            sleep $delay
        done
    done
    printf "\r"
}

# ============================================================================
# Deployment Functions
# ============================================================================

# Parse and execute deployment intent
deploy_intent() {
    local intent="$1"
    local dry_run="${2:-false}"
    local watch="${3:-false}"
    
    echo -e "\n${BOLD}${ICON_ROCKET} Clawbernetes Deployment${NC}\n"
    echo -e "${DIM}Intent:${NC} ${CYAN}\"$intent\"${NC}\n"
    
    # Show parsing status
    echo -e "${BOLD}Parsing intent...${NC}"
    
    # Simulate intent parsing (replace with actual API call)
    sleep 0.5
    
    # Extract deployment details (mock parsing)
    local workload=""
    local strategy="rolling"
    local canary_pct=""
    local gpus=""
    local version=""
    local rollback_condition=""
    
    # Simple pattern matching for demo
    if [[ "$intent" =~ canary\ ([0-9]+)% ]]; then
        strategy="canary"
        canary_pct="${BASH_REMATCH[1]}"
    fi
    
    if [[ "$intent" =~ ([0-9]+)\ GPU ]]; then
        gpus="${BASH_REMATCH[1]}"
    fi
    
    if [[ "$intent" =~ (deploy|update|upgrade)\ ([a-zA-Z0-9_-]+) ]]; then
        workload="${BASH_REMATCH[2]}"
    fi
    
    if [[ "$intent" =~ to\ (v[0-9.]+) ]]; then
        version="${BASH_REMATCH[1]}"
    fi
    
    if [[ "$intent" =~ rollback\ if\ (.+) ]]; then
        rollback_condition="${BASH_REMATCH[1]}"
    fi
    
    # Display parsed intent
    echo -e "\n${BOLD}Parsed Configuration:${NC}"
    echo -e "  ${DIM}Workload:${NC}  ${workload:-<detected from context>}"
    echo -e "  ${DIM}Strategy:${NC}  ${strategy}"
    [[ -n "$canary_pct" ]] && echo -e "  ${DIM}Canary:${NC}    ${canary_pct}%"
    [[ -n "$gpus" ]] && echo -e "  ${DIM}GPUs:${NC}      ${gpus}"
    [[ -n "$version" ]] && echo -e "  ${DIM}Version:${NC}   ${version}"
    [[ -n "$rollback_condition" ]] && echo -e "  ${DIM}Rollback:${NC}  if ${rollback_condition}"
    echo ""
    
    if [[ "$dry_run" == "true" ]]; then
        log_info "Dry run - no changes will be made"
        echo -e "\n${DIM}Would execute:${NC}"
        echo -e "  clawbernetes deploy apply --strategy $strategy ..."
        return 0
    fi
    
    # Confirmation
    echo -e -n "${YELLOW}Proceed with deployment?${NC} [Y/n] "
    read -r confirm
    if [[ "$confirm" =~ ^[Nn] ]]; then
        log_warning "Deployment cancelled"
        return 1
    fi
    
    # Execute deployment (mock)
    echo -e "\n${BOLD}Initiating deployment...${NC}"
    
    # Generate deployment ID
    local deploy_id="dep-$(date +%s | tail -c 7)"
    
    log_success "Deployment created: ${BOLD}$deploy_id${NC}"
    echo ""
    
    if [[ "$watch" == "true" ]]; then
        watch_deployment "$deploy_id"
    else
        echo -e "Track progress with: ${CYAN}clawbernetes deploy status $deploy_id --watch${NC}"
    fi
    
    return 0
}

# Watch deployment progress
watch_deployment() {
    local deploy_id=$1
    local timeout=${2:-1800}  # 30 min default
    local start_time=$(date +%s)
    
    echo -e "\n${BOLD}Watching deployment: $deploy_id${NC}\n"
    
    # Simulated deployment phases for demo
    local phases=("pending" "rolling" "canary" "promoting" "complete")
    local phase_idx=0
    local replicas_ready=0
    local replicas_total=10
    
    while true; do
        local current_time=$(date +%s)
        local elapsed=$((current_time - start_time))
        
        # Check timeout
        if [[ $elapsed -gt $timeout ]]; then
            log_error "Deployment timed out after $(format_duration $elapsed)"
            return 1
        fi
        
        # Get current phase (simulated)
        local phase="${phases[$phase_idx]}"
        
        # Clear previous output
        echo -e "\033[2K\r"
        
        # Print status header
        echo -e "${BOLD}Deployment Status${NC} ${DIM}($(format_duration $elapsed) elapsed)${NC}"
        echo -e "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        
        # Print phase
        echo -e -n "Phase: "
        print_phase "$phase"
        
        # Print progress
        echo -e -n "Progress: "
        progress_bar $replicas_ready $replicas_total
        echo -e " (${replicas_ready}/${replicas_total} ready)"
        
        # Print health indicators
        echo -e "\n${BOLD}Health Checks:${NC}"
        if [[ $replicas_ready -gt 0 ]]; then
            echo -e "  ${GREEN}${ICON_CHECK}${NC} Readiness probe passing"
            echo -e "  ${GREEN}${ICON_CHECK}${NC} Liveness probe passing"
        else
            echo -e "  ${YELLOW}${ICON_PENDING}${NC} Waiting for pods..."
        fi
        
        if [[ "$phase" == "canary" ]]; then
            echo -e "\n${BOLD}Canary Metrics:${NC}"
            echo -e "  ${DIM}Error rate:${NC}  0.12% ${GREEN}(< 1% threshold)${NC}"
            echo -e "  ${DIM}Latency p99:${NC} 145ms ${GREEN}(< 500ms threshold)${NC}"
        fi
        
        # Check if complete
        if [[ "$phase" == "complete" ]]; then
            echo -e "\n${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
            log_success "Deployment completed successfully in $(format_duration $elapsed)"
            return 0
        fi
        
        if [[ "$phase" == "failed" ]]; then
            echo -e "\n${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
            log_error "Deployment failed"
            echo -e "\n${BOLD}Recent Events:${NC}"
            echo -e "  ${RED}${ICON_CROSS}${NC} Pod model-server-abc123 CrashLoopBackOff"
            echo -e "  ${RED}${ICON_CROSS}${NC} Error: OOMKilled"
            echo -e "\n${YELLOW}Consider:${NC} clawbernetes deploy rollback $deploy_id"
            return 1
        fi
        
        # Simulate progress
        sleep 2
        replicas_ready=$((replicas_ready + 2))
        if [[ $replicas_ready -gt $replicas_total ]]; then
            replicas_ready=$replicas_total
            phase_idx=$((phase_idx + 1))
            if [[ $phase_idx -ge ${#phases[@]} ]]; then
                phase_idx=$((${#phases[@]} - 1))
            fi
        fi
        
        # Move cursor up to overwrite
        echo -e "\033[12A"
    done
}

# Show deployment status
show_status() {
    local deploy_id=$1
    local watch=${2:-false}
    
    if [[ "$watch" == "true" ]]; then
        watch_deployment "$deploy_id"
        return
    fi
    
    echo -e "\n${BOLD}Deployment: $deploy_id${NC}\n"
    
    # Mock status display
    echo -e "${BOLD}Overview${NC}"
    echo -e "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo -e "  ${DIM}Workload:${NC}    model-server"
    echo -e "  ${DIM}Version:${NC}     v2.1.0"
    echo -e "  ${DIM}Strategy:${NC}    canary"
    echo -e "  ${DIM}Started:${NC}     2 minutes ago"
    echo -e "  ${DIM}Phase:${NC}       $(print_phase "canary")"
    echo ""
    
    echo -e "${BOLD}Replica Status${NC}"
    echo -e "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo -e -n "  "
    progress_bar 8 10
    echo -e " (8/10 ready)"
    echo ""
    
    echo -e "${BOLD}Canary Status${NC}"
    echo -e "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo -e "  ${DIM}Traffic:${NC}     10%"
    echo -e "  ${DIM}Duration:${NC}    12m 34s"
    echo -e "  ${DIM}Error Rate:${NC}  ${GREEN}0.08%${NC} (threshold: 1%)"
    echo -e "  ${DIM}Latency:${NC}     ${GREEN}89ms${NC} (threshold: 500ms)"
    echo ""
    
    echo -e "${DIM}Watch progress:${NC} clawbernetes deploy status $deploy_id --watch"
    echo -e "${DIM}Promote canary:${NC} clawbernetes deploy promote $deploy_id"
}

# Promote canary deployment
promote_deployment() {
    local deploy_id=$1
    local force=${2:-false}
    
    echo -e "\n${BOLD}${ICON_CANARY} Promote Canary: $deploy_id${NC}\n"
    
    # Show current canary status
    echo -e "${BOLD}Current Canary Status:${NC}"
    echo -e "  ${DIM}Traffic:${NC}     10%"
    echo -e "  ${DIM}Duration:${NC}    45m"
    echo -e "  ${DIM}Error Rate:${NC}  ${GREEN}0.05%${NC}"
    echo -e "  ${DIM}Latency p99:${NC} ${GREEN}92ms${NC}"
    echo ""
    
    if [[ "$force" != "true" ]]; then
        echo -e -n "${YELLOW}Promote to 100% traffic?${NC} [Y/n] "
        read -r confirm
        if [[ "$confirm" =~ ^[Nn] ]]; then
            log_warning "Promotion cancelled"
            return 1
        fi
    fi
    
    echo -e "\n${BOLD}Promoting deployment...${NC}"
    
    # Simulated promotion
    for pct in 25 50 75 100; do
        echo -e -n "\r  Traffic: "
        progress_bar $pct 100 30
        echo -e " ${pct}%"
        sleep 0.5
    done
    
    echo ""
    log_success "Deployment promoted to 100%"
}

# Rollback deployment
rollback_deployment() {
    local deploy_id=$1
    local target_version=${2:-""}
    local immediate=${3:-false}
    
    echo -e "\n${BOLD}${ICON_ROLLBACK} Rollback Deployment${NC}\n"
    
    # Show what will be rolled back
    echo -e "${BOLD}Current State:${NC}"
    echo -e "  ${DIM}Deployment:${NC}  $deploy_id"
    echo -e "  ${DIM}Workload:${NC}    model-server"
    echo -e "  ${DIM}Current:${NC}     v2.1.0"
    
    if [[ -n "$target_version" ]]; then
        echo -e "  ${DIM}Target:${NC}      ${YELLOW}$target_version${NC}"
    else
        echo -e "  ${DIM}Target:${NC}      ${YELLOW}v2.0.0${NC} (previous)"
    fi
    echo ""
    
    # Warning for immediate rollback
    if [[ "$immediate" == "true" ]]; then
        log_warning "IMMEDIATE rollback requested - will skip graceful drain"
        echo ""
    fi
    
    # Confirmation
    echo -e "${RED}${BOLD}⚠️  This will rollback the deployment!${NC}"
    echo -e -n "${YELLOW}Are you sure?${NC} [y/N] "
    read -r confirm
    if [[ ! "$confirm" =~ ^[Yy] ]]; then
        log_info "Rollback cancelled"
        return 1
    fi
    
    # Optional: Ask for reason
    echo -e -n "${DIM}Reason (optional):${NC} "
    read -r reason
    
    echo -e "\n${BOLD}Initiating rollback...${NC}"
    
    # Simulated rollback progress
    local steps=("Stopping new version" "Draining connections" "Scaling down v2.1.0" "Scaling up v2.0.0" "Verifying health" "Updating routing")
    
    if [[ "$immediate" == "true" ]]; then
        steps=("Stopping new version" "Force scaling down" "Scaling up previous" "Updating routing")
    fi
    
    for step in "${steps[@]}"; do
        echo -e "  ${ICON_RUNNING} $step..."
        sleep 0.5
        echo -e "\033[1A\033[2K  ${GREEN}${ICON_CHECK}${NC} $step"
    done
    
    echo ""
    log_success "Rollback completed successfully"
    
    if [[ -n "$reason" ]]; then
        echo -e "${DIM}Recorded reason: $reason${NC}"
    fi
    
    echo -e "\n${DIM}View status:${NC} clawbernetes deploy status $deploy_id"
}

# Show deployment history
show_history() {
    local workload=$1
    local limit=${2:-10}
    
    echo -e "\n${BOLD}Deployment History: $workload${NC}\n"
    
    # Header
    printf "${DIM}%-14s %-10s %-12s %-10s %-8s %s${NC}\n" \
        "DEPLOYMENT" "VERSION" "STRATEGY" "DURATION" "STATUS" "INITIATED"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    
    # Mock history entries
    printf "%-14s %-10s %-12s %-10s ${GREEN}%-8s${NC} %s\n" \
        "dep-847291" "v2.1.0" "canary" "12m 34s" "success" "2h ago"
    printf "%-14s %-10s %-12s %-10s ${RED}%-8s${NC} %s\n" \
        "dep-736182" "v2.0.5" "rolling" "3m 12s" "failed" "1d ago"
    printf "%-14s %-10s %-12s %-10s ${GREEN}%-8s${NC} %s\n" \
        "dep-625073" "v2.0.4" "canary" "45m 00s" "success" "3d ago"
    printf "%-14s %-10s %-12s %-10s ${GREEN}%-8s${NC} %s\n" \
        "dep-513964" "v2.0.3" "rolling" "8m 45s" "success" "1w ago"
    printf "%-14s %-10s %-12s %-10s ${YELLOW}%-8s${NC} %s\n" \
        "dep-402855" "v2.0.2" "blue-green" "15m 22s" "rolled-back" "2w ago"
    
    echo ""
    echo -e "${DIM}Showing last 5 of $limit deployments${NC}"
    echo -e "${DIM}View details:${NC} clawbernetes deploy status <deployment-id>"
}

# ============================================================================
# Main
# ============================================================================

usage() {
    cat << EOF
${BOLD}Clawbernetes Deployment Helper${NC}

${BOLD}Usage:${NC}
  $0 deploy "<intent>" [--dry-run] [--watch]
  $0 status <deployment-id> [--watch]
  $0 promote <deployment-id> [--force]
  $0 rollback <deployment-id> [--to <version>] [--immediate]
  $0 history <workload> [--limit <n>]

${BOLD}Commands:${NC}
  deploy      Deploy with natural language intent
  status      Check deployment status
  promote     Promote canary to full deployment
  rollback    Rollback to previous version
  history     View deployment history

${BOLD}Examples:${NC}
  $0 deploy "deploy model-server v2.1, canary 10%, rollback if errors > 1%"
  $0 deploy "scale inference to 8 replicas" --dry-run
  $0 status dep-123456 --watch
  $0 rollback dep-123456 --to v2.0.0
  $0 history model-server --limit 20

${BOLD}Environment:${NC}
  CLAWBERNETES_GATEWAY_URL   Gateway URL (default: ws://localhost:9000)
  DEPLOY_POLL_INTERVAL       Status poll interval in seconds (default: 5)
EOF
}

main() {
    if [[ $# -lt 1 ]]; then
        usage
        exit 1
    fi
    
    local command=$1
    shift
    
    case $command in
        deploy)
            local intent=""
            local dry_run="false"
            local watch="false"
            
            while [[ $# -gt 0 ]]; do
                case $1 in
                    --dry-run)
                        dry_run="true"
                        shift
                        ;;
                    --watch|-w)
                        watch="true"
                        shift
                        ;;
                    *)
                        intent="$1"
                        shift
                        ;;
                esac
            done
            
            if [[ -z "$intent" ]]; then
                log_error "Intent required"
                echo "Usage: $0 deploy \"<intent>\""
                exit 1
            fi
            
            deploy_intent "$intent" "$dry_run" "$watch"
            ;;
            
        status)
            local deploy_id=""
            local watch="false"
            
            while [[ $# -gt 0 ]]; do
                case $1 in
                    --watch|-w)
                        watch="true"
                        shift
                        ;;
                    *)
                        deploy_id="$1"
                        shift
                        ;;
                esac
            done
            
            if [[ -z "$deploy_id" ]]; then
                log_error "Deployment ID required"
                exit 1
            fi
            
            show_status "$deploy_id" "$watch"
            ;;
            
        promote)
            local deploy_id=""
            local force="false"
            
            while [[ $# -gt 0 ]]; do
                case $1 in
                    --force|-f)
                        force="true"
                        shift
                        ;;
                    *)
                        deploy_id="$1"
                        shift
                        ;;
                esac
            done
            
            if [[ -z "$deploy_id" ]]; then
                log_error "Deployment ID required"
                exit 1
            fi
            
            promote_deployment "$deploy_id" "$force"
            ;;
            
        rollback)
            local deploy_id=""
            local target_version=""
            local immediate="false"
            
            while [[ $# -gt 0 ]]; do
                case $1 in
                    --to)
                        target_version="$2"
                        shift 2
                        ;;
                    --immediate)
                        immediate="true"
                        shift
                        ;;
                    *)
                        deploy_id="$1"
                        shift
                        ;;
                esac
            done
            
            if [[ -z "$deploy_id" ]]; then
                log_error "Deployment ID required"
                exit 1
            fi
            
            rollback_deployment "$deploy_id" "$target_version" "$immediate"
            ;;
            
        history)
            local workload=""
            local limit=10
            
            while [[ $# -gt 0 ]]; do
                case $1 in
                    --limit|-n)
                        limit="$2"
                        shift 2
                        ;;
                    *)
                        workload="$1"
                        shift
                        ;;
                esac
            done
            
            if [[ -z "$workload" ]]; then
                log_error "Workload name required"
                exit 1
            fi
            
            show_history "$workload" "$limit"
            ;;
            
        -h|--help|help)
            usage
            ;;
            
        *)
            log_error "Unknown command: $command"
            usage
            exit 1
            ;;
    esac
}

main "$@"
