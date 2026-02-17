---
name: observability
description: Aggregate logs, metrics, and health data across the GPU fleet for monitoring and diagnosis.
metadata: {"openclaw": {"always": true}}
---

# Observability

## Fleet Health Snapshot

Run across all connected nodes:

```bash
# Per-node GPU summary
exec host=node node=<name> command="nvidia-smi --query-gpu=index,name,temperature.gpu,utilization.gpu,memory.used,memory.total,power.draw --format=csv"

# System resources
exec host=node node=<name> command="echo 'CPU:' && top -bn1 | head -3 && echo 'MEM:' && free -h | head -2 && echo 'DISK:' && df -h / | tail -1"

# Container status
exec host=node node=<name> command="docker ps --format '{{.Names}}\t{{.Status}}' 2>/dev/null || echo 'no containers'"
```

## Aggregate Logs

```bash
# Recent container logs (last 5 min)
exec host=node node=<name> command="docker logs --since 5m <container> 2>&1 | tail -50"

# System logs (errors only)
exec host=node node=<name> command="journalctl -p err --since '5 min ago' --no-pager 2>/dev/null | tail -20"

# NVIDIA driver logs
exec host=node node=<name> command="dmesg | grep -i 'nvidia\|gpu\|nvrm' | tail -20"
```

## GPU Metrics Collection

```bash
# Detailed time-series snapshot
exec host=node node=<name> command="nvidia-smi dmon -s pucvmet -c 3"

# DCGM metrics (if available)
exec host=node node=<name> command="dcgmi diag -r 1 2>/dev/null || echo 'DCGM not installed'"
```

## Network I/O

```bash
exec host=node node=<name> command="ss -tuln | grep -E ':(8000|8080|8443|3000)' || echo 'no listening services'"
exec host=node node=<name> command="cat /proc/net/dev | awk 'NR>2{print $1, $2, $10}'"
```

## Anomaly Detection

When checking fleet health, flag:
- **GPU temp > 85°C** — thermal warning
- **GPU util at 100% + memory full** — potential OOM risk
- **GPU util at 0% with running container** — process may be hung
- **ECC errors > 0** — hardware degradation
- **Container restarting** — crash loop
- **Disk > 90% full** — capacity warning

## Summary Report Format

```
Fleet Status: <timestamp>
━━━━━━━━━━━━━━━━━━━━━━━━━━
Nodes: X connected, Y total
GPUs: N total, M active
Alerts: [list any flagged issues]

Per-Node:
  node-01: 8×H100, 72°C avg, 85% util, 3 containers
  node-02: 4×A100, 65°C avg, 20% util, 1 container
```
