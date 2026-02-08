---
name: auto-heal
description: Automatically detect and recover from infrastructure failures in Clawbernetes clusters
user-invocable: false
---

# Auto-Heal

Automatically detect and recover from common infrastructure failures including node failures, workload crashes, and resource exhaustion.

## When to Use

- Node becomes unresponsive
- Workload repeatedly crashes
- Resource exhaustion detected
- Health check failures
- "Fix the cluster" or "heal infrastructure"

## Healing Workflow

1. **Detect failure** — alerts, health checks, heartbeat loss
2. **Classify severity** — transient vs persistent
3. **Attempt recovery** — restart, migrate, or replace
4. **Verify recovery** — confirm health restored
5. **Post-mortem** — log for analysis

## Tools Used

- `cluster_status` — overall health
- `node_list` — find unhealthy nodes
- `node_get` — detailed diagnostics
- `node_drain` — safely remove workloads
- `node_cordon` — prevent new scheduling
- `workload_list` — find affected workloads
- `workload_submit` — restart workloads
- `alert_create` — notify on actions
- `logs_search` — find root cause

## Failure Types & Actions

### 1. Node Not Responding

**Detection:**
```
node_list(status: "offline")
```

**Actions:**
1. Wait 2 minutes (transient network issue?)
2. If still offline, cordon node
3. Migrate workloads to healthy nodes
4. Alert ops team for hardware check

```
node_cordon(nodeId: "<failing_node>")

# Get affected workloads
workload_list(nodeId: "<failing_node>")

# For each workload, reschedule
workload_stop(workloadId: "<wl>", force: true)
workload_submit(spec: <original_spec>)
```

### 2. GPU Failure

**Detection:**
```
logs_search(query: "Xid error OR GPU has fallen off", nodeId: "...")
metrics_query(metric: "gpu_health", nodeId: "...")
```

**Actions:**
1. Identify specific GPU
2. Cordon node
3. Migrate workloads
4. Alert for hardware replacement

### 3. Workload Crash Loop

**Detection:**
```
workload_list(state: "failed")
# Check restart count in workload details
workload_get(workloadId: "...")
```

**Actions:**
1. If < 3 restarts: auto-restart
2. If 3+ restarts: pause and alert
3. Check logs for root cause
4. If OOM: adjust resources and restart

```
# Check recent failures
workload_logs(workloadId: "<wl>", level: "error", tail: 50)

# If OOM, resubmit with more memory
workload_submit(
  ...originalSpec,
  memoryMb: originalSpec.memoryMb * 1.5
)
```

### 4. Resource Exhaustion

**Detection:**
```
cluster_status()
# Check if gpus.available == 0
```

**Actions:**
1. Identify low-priority preemptible workloads
2. Preempt if critical workloads are pending
3. Scale down non-critical deployments
4. Alert on capacity crunch

### 5. Network Partition

**Detection:**
```
logs_search(query: "NCCL timeout OR connection refused")
```

**Actions:**
1. Identify affected nodes
2. Check network metrics
3. Restart affected workloads
4. If persistent, isolate affected nodes

## Recovery Priorities

| Workload Type | Priority | Max Recovery Time |
|--------------|----------|-------------------|
| Production inference | Critical | < 1 minute |
| Training jobs | High | < 5 minutes |
| Batch processing | Medium | < 15 minutes |
| Dev/test | Low | < 1 hour |

## Auto-Heal Configuration

```yaml
autoHeal:
  enabled: true
  
  node:
    offlineGracePeriod: 2m
    autoCordon: true
    autoMigrate: true
  
  workload:
    maxRestarts: 3
    restartBackoff: exponential
    oomAutoScale: true
  
  alerts:
    notifyOnRecovery: true
    notifyOnFailure: true
```

## Example: Full Healing Run

**Trigger:** "Check cluster health and fix any issues"

**Agent:**

1. Get cluster status:
   ```
   cluster_status()
   ```
   Result: 2 nodes not_ready, 3 workloads failed

2. Check offline nodes:
   ```
   node_list(status: "not_ready")
   ```
   Found: node-5, node-7

3. For each offline node:
   ```
   node_get(nodeId: "node-5")
   # Last heartbeat 10 minutes ago
   
   node_cordon(nodeId: "node-5")
   workload_list(nodeId: "node-5")
   # Found 2 workloads to migrate
   ```

4. Migrate workloads:
   ```
   workload_stop(workloadId: "wl-123", force: true)
   workload_submit(spec: {...})  # reschedule
   ```

5. Check failed workloads:
   ```
   workload_list(state: "failed")
   ```
   
   For each, check logs and restart if appropriate

6. Create summary alert:
   ```
   alert_create(
     name: "auto-heal-complete",
     severity: "info",
     message: "Auto-heal: 2 nodes cordoned, 5 workloads restarted"
   )
   ```

7. Report to user:
   - Cordoned 2 unhealthy nodes
   - Migrated 4 workloads
   - Restarted 3 failed workloads
   - Recommend: Investigate node-5 and node-7 hardware

## Escalation

If auto-heal cannot recover:
1. Create critical alert
2. Provide diagnostic summary
3. Suggest manual intervention steps
4. Offer to drain affected infrastructure
