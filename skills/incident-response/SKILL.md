---
name: incident-response
description: Diagnose and respond to infrastructure incidents — node failures, GPU errors, performance degradation.
metadata: {"openclaw": {"always": true}}
---

# Incident Response

## Triage (first 60 seconds)

1. **Scope**: Which nodes/GPUs/workloads are affected?
2. **Impact**: Is production down? Training interrupted?
3. **Timeline**: When did it start?

```bash
# Quick fleet status
nodes status

# Per-node health
exec host=node node=<name> command="uptime && nvidia-smi --query-gpu=temperature.gpu,utilization.gpu,ecc.errors.uncorrected.volatile.total --format=csv,noheader"

# Container status
exec host=node node=<name> command="docker ps -a --format '{{.Names}} {{.Status}}' | head -20"
```

## Common Incidents

### GPU Xid Error
```bash
exec host=node node=<name> command="dmesg | grep -i 'xid\|nvrm' | tail -20"
```
- Xid 31/32: GPU memory page fault → restart workload
- Xid 48: Double Bit ECC → **GPU failing, migrate immediately**
- Xid 79: GPU fallen off bus → **hardware failure, drain node**

### Node OOM
```bash
exec host=node node=<name> command="dmesg | grep -i 'oom' | tail -10"
exec host=node node=<name> command="free -h"
```
→ Identify largest memory consumer, restart with limits or migrate

### Network Partition (multi-node training)
```bash
exec host=node node=<name> command="ping -c 3 <other-node-ip>"
exec host=node node=<name> command="ss -tuln | grep 29500"  # NCCL port
```
→ Check firewall, restart NCCL workers

### Container Crash Loop
```bash
exec host=node node=<name> command="docker inspect <container> --format '{{.RestartCount}} restarts, last exit: {{.State.ExitCode}}'"
exec host=node node=<name> command="docker logs --tail 50 <container> 2>&1"
```
→ Check exit code, fix root cause, restart

## Escalation

If automated remediation fails after 2 attempts:
1. **Alert the user** with full context
2. Include: affected workloads, error messages, actions tried
3. Suggest manual steps if known
4. Offer to collect full diagnostic dump

## Post-Incident

After resolution:
1. Document what happened in memory
2. Note root cause and fix
3. Suggest preventive measures (monitoring, limits, redundancy)
