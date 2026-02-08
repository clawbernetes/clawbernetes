---
name: incident-response
description: Structured SRE incident response — detection, triage, mitigation, and post-mortem
user-invocable: false
---

# Incident Response

Structured approach to handling production incidents — detection, triage, mitigation, and post-mortem.

## When to Use

- Critical alert fired
- "Production is down"
- "Users are reporting errors"
- Sudden performance degradation
- Unexpected workload failures

## Incident Workflow

1. **DETECT** — Confirm and classify incident
2. **TRIAGE** — Assess severity and impact
3. **MITIGATE** — Take immediate action to reduce impact
4. **INVESTIGATE** — Find root cause
5. **RESOLVE** — Fix the underlying issue
6. **POST-MORTEM** — Document and learn

## Tools Used

- `cluster_status` — quick health check
- `alert_list` — active alerts
- `workload_list` — affected workloads
- `metrics_query` — performance metrics
- `logs_search` — error investigation
- `node_list` — infrastructure status
- `workload_stop/submit` — mitigation actions
- `node_drain` — isolate bad nodes

## Severity Levels

| Level | Impact | Response Time | Example |
|-------|--------|---------------|---------|
| **SEV1** | Total outage | Immediate | All inference down |
| **SEV2** | Major degradation | < 15 min | 50%+ requests failing |
| **SEV3** | Partial impact | < 1 hour | Single workload down |
| **SEV4** | Minor issue | < 4 hours | Performance degraded |

## Phase 1: DETECT

Quick assessment:

```
# 1. Check cluster health
cluster_status()

# 2. Check active alerts
alert_list(active: true)

# 3. Check for failed workloads
workload_list(state: "failed")
```

**Classify:**
- Is it infrastructure (nodes) or application (workloads)?
- Is it isolated or widespread?
- What is user impact?

## Phase 2: TRIAGE

**For infrastructure issues:**
```
node_list(status: "not_ready")
node_list(status: "offline")
```

**For workload issues:**
```
workload_get(workloadId: "<affected>")
workload_logs(workloadId: "<affected>", level: "error", tail: 100)
```

**For performance issues:**
```
metrics_query(metric: "request_latency_p99", startTime: "-30m")
metrics_query(metric: "gpu_utilization", startTime: "-30m")
```

## Phase 3: MITIGATE

### Scenario: Node Failure

```
# Cordon bad node
node_cordon(nodeId: "<bad_node>")

# Migrate workloads
node_drain(nodeId: "<bad_node>", gracePeriodSeconds: 60)
```

### Scenario: Workload Crash Loop

```
# Stop crashing workload
workload_stop(workloadId: "<crashing>", force: true)

# If needed, rollback to previous version
workload_submit(
  name: "<service>-rollback",
  image: "<previous_image>",
  ...
)
```

### Scenario: Resource Exhaustion

```
# Find low-priority workloads to preempt
workload_list(labels: {priority: "low"})

# Preempt to free resources
workload_stop(workloadId: "<low_priority>")
```

### Scenario: Network Issues

```
# Isolate affected nodes
node_cordon(nodeId: "<node1>")
node_cordon(nodeId: "<node2>")

# Restart affected workloads on healthy nodes
workload_stop(workloadId: "...", force: true)
workload_submit(...)
```

## Phase 4: INVESTIGATE

After immediate mitigation, find root cause:

```
# Search for error patterns
logs_search(
  query: "error OR exception OR fatal",
  startTime: "<incident_start>",
  endTime: "<incident_end>",
  limit: 500
)

# Check metrics around incident time
metrics_query(
  metric: "gpu_temperature",
  startTime: "-1h",
  step: "1m"
)

# Look for correlated events
logs_search(query: "restart OR OOM OR Xid")
```

**Common Root Causes:**
- Hardware failure (GPU, network, disk)
- Resource exhaustion (OOM, disk full)
- Code bug (new deployment)
- Configuration error
- External dependency failure

## Phase 5: RESOLVE

Once root cause identified:

1. **Hardware issue** → Replace/repair hardware
2. **Code bug** → Rollback or hot-fix
3. **Config error** → Correct configuration
4. **Resource issue** → Scale up or optimize

## Phase 6: POST-MORTEM

Document the incident:

```markdown
## Incident Post-Mortem: [TITLE]

**Date:** YYYY-MM-DD
**Duration:** X hours Y minutes
**Severity:** SEVX
**Impact:** [User impact description]

### Timeline
- HH:MM - First alert fired
- HH:MM - Incident declared
- HH:MM - Mitigation started
- HH:MM - Service restored
- HH:MM - Root cause identified
- HH:MM - Incident resolved

### Root Cause
[Detailed explanation]

### Mitigation Actions
1. [Action taken]
2. [Action taken]

### Prevention
- [ ] [Action item to prevent recurrence]
- [ ] [Action item]

### Lessons Learned
- [Lesson 1]
- [Lesson 2]
```

## Quick Reference Checklist

```
INCIDENT RESPONSE CHECKLIST

□ Confirm incident (not false alarm)
□ Classify severity (SEV1-4)
□ Alert stakeholders if SEV1/2
□ Check cluster_status()
□ Check alert_list(active: true)
□ Identify affected components
□ Take mitigation action
□ Verify mitigation effective
□ Investigate root cause
□ Document timeline
□ Create follow-up tasks
□ Schedule post-mortem
```

## Example: SEV1 Response

**Alert:** "All inference requests failing"

**Agent Response:**

1. **DETECT** (30 seconds):
   ```
   cluster_status()
   ```
   Result: 0/8 nodes ready, cluster unhealthy

2. **TRIAGE** (1 minute):
   ```
   node_list()
   ```
   All nodes showing "not_ready" — network partition?

3. **MITIGATE** (2 minutes):
   ```
   # Check if nodes are actually alive
   logs_search(query: "heartbeat", startTime: "-5m")
   
   # Found: network switch failure affecting rack 1
   # Nodes in rack 2 are healthy
   
   # Reschedule critical workloads to rack 2
   workload_list(labels: {critical: "true"})
   # For each, force stop and resubmit with rack2 selector
   ```

4. **VERIFY** (1 minute):
   ```
   workload_list(labels: {critical: "true"}, state: "running")
   # All critical workloads running on rack 2
   ```

5. **COMMUNICATE**:
   ```
   alert_create(
     name: "sev1-mitigated",
     severity: "info",
     message: "SEV1 mitigated: Critical services restored. Root cause: network switch failure in rack 1. Rack 1 nodes cordoned."
   )
   ```

6. **INVESTIGATE** and **POST-MORTEM** follow...
