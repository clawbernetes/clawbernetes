---
name: system-admin
description: Node administration — labels, taints, drains, OS updates, service management.
metadata: {"openclaw": {"always": true}}
---

# System Administration

## Node Information

```bash
# Full system info
exec host=node node=<name> command="hostnamectl 2>/dev/null || (uname -a && cat /etc/os-release | head -5)"

# Uptime and load
exec host=node node=<name> command="uptime"

# Disk usage
exec host=node node=<name> command="df -h | grep -E '^/dev'"

# Memory
exec host=node node=<name> command="free -h"

# Network interfaces
exec host=node node=<name> command="ip -br addr 2>/dev/null || ifconfig | grep -E 'inet '"
```

## Node Labels and Metadata

Labels are stored in clawnode's local config:
```bash
nodes invoke --node <name> --command node.label --params '{"action":"set","key":"gpu-type","value":"h100"}'
nodes invoke --node <name> --command node.label --params '{"action":"list"}'
```

## Node Drain (graceful workload removal)

```bash
# 1. Stop accepting new workloads
nodes invoke --node <name> --command node.taint --params '{"key":"drain","value":"true","effect":"NoSchedule"}'

# 2. List running workloads
exec host=node node=<name> command="docker ps --format '{{.Names}}'"

# 3. Migrate each workload to another node (use workload-manager skill)

# 4. Verify node is empty
exec host=node node=<name> command="docker ps -q | wc -l"
```

## OS Updates

```bash
# Check for updates (Ubuntu/Debian)
exec host=node node=<name> command="apt list --upgradable 2>/dev/null | head -20"

# Check for updates (RHEL/CentOS)
exec host=node node=<name> command="yum check-update 2>/dev/null | tail -20"

# NVIDIA driver version
exec host=node node=<name> command="nvidia-smi --query-gpu=driver_version --format=csv,noheader | head -1"
```

**⚠️ Never auto-apply OS or driver updates.** Always ask the user first — GPU driver updates can break running workloads.

## Service Management

```bash
# Check clawnode service
exec host=node node=<name> command="systemctl status clawnode 2>/dev/null || echo 'not a systemd service'"

# Docker service
exec host=node node=<name> command="systemctl status docker"

# NVIDIA persistence daemon
exec host=node node=<name> command="systemctl status nvidia-persistenced 2>/dev/null"
```

## Security

```bash
# Check listening ports
exec host=node node=<name> command="ss -tuln"

# Check for failed logins
exec host=node node=<name> command="journalctl -u sshd --since '24 hours ago' | grep -i 'failed' | wc -l"
```
