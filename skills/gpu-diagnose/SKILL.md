# GPU Diagnose

Diagnose GPU issues including performance problems, memory errors, thermal throttling, and hardware failures.

## When to Use

- "Why is my training slow?"
- "GPU not working" or "GPU errors"
- Workload stuck in pending state
- Performance degradation
- CUDA/driver errors in logs

## Diagnostic Workflow

1. **Identify the scope** — specific workload, node, or cluster-wide?
2. **Check GPU health** — temperature, utilization, memory
3. **Review logs** — CUDA errors, driver issues, OOM
4. **Check scheduling** — pending workloads, resource constraints
5. **Recommend action** — fix or escalate

## Tools Used

- `node_list` — list nodes with GPU status
- `node_get` — detailed node/GPU info
- `workload_get` — workload status and errors
- `workload_logs` — check for GPU errors
- `metrics_query` — GPU metrics (utilization, temp, memory)
- `logs_search` — search for error patterns
- `alert_list` — check existing alerts

## Common Issues & Diagnosis

### 1. Thermal Throttling

**Symptoms:** High GPU temp, reduced utilization despite load

**Diagnosis:**
```
metrics_query(metric: "gpu_temperature", nodeId: "...")
metrics_query(metric: "gpu_utilization", nodeId: "...")
```

If temp > 80°C and utilization fluctuates → thermal throttling

**Action:** Improve cooling, reduce batch size, or migrate workload

### 2. Out of Memory (OOM)

**Symptoms:** Workload crashes, "CUDA out of memory" in logs

**Diagnosis:**
```
workload_logs(workloadId: "...", level: "error")
metrics_query(metric: "gpu_memory_used", workloadId: "...")
```

Look for: "CUDA error: out of memory", memory usage near 100%

**Action:** Reduce batch size, use gradient checkpointing, request more GPUs

### 3. CUDA/Driver Errors

**Symptoms:** Workload fails immediately, driver errors

**Diagnosis:**
```
workload_logs(workloadId: "...")
logs_search(query: "CUDA error OR nvidia-smi OR driver", nodeId: "...")
```

Look for: driver version mismatch, CUDA initialization failed

**Action:** Check driver version, restart node, or update drivers

### 4. Scheduling Failure

**Symptoms:** Workload stuck in "pending"

**Diagnosis:**
```
workload_get(workloadId: "...")
cluster_status()
node_list(status: "ready")
```

Check: available GPUs vs requested, resource constraints

**Action:** Wait for resources, adjust requirements, or add capacity

### 5. Network/Communication Errors

**Symptoms:** Multi-GPU training hangs, NCCL errors

**Diagnosis:**
```
workload_logs(workloadId: "...", query: "NCCL")
logs_search(query: "NCCL timeout OR connection refused")
```

Look for: NCCL timeouts, socket errors, IB/network issues

**Action:** Check network config, reduce world size, restart training

## Diagnostic Checklist

```
□ GPU temperature normal (<80°C)?
□ GPU utilization matches expected?
□ GPU memory usage reasonable?
□ No CUDA/driver errors in logs?
□ Workload not stuck in pending?
□ No recent node failures?
□ Network healthy for multi-GPU?
□ No existing critical alerts?
```

## Example Session

**User:** "My training job is running slow"

**Agent:**

1. Get workload info:
   ```
   workload_get(workloadId: "training-job-123")
   ```

2. Check GPU metrics:
   ```
   metrics_query(metric: "gpu_utilization", workloadId: "training-job-123", startTime: "-1h")
   ```

3. Found: utilization dropping from 95% to 60% periodically

4. Check temperature:
   ```
   metrics_query(metric: "gpu_temperature", workloadId: "training-job-123", startTime: "-1h")
   ```

5. Found: temperature spikes to 85°C, correlates with utilization drops

6. **Diagnosis:** Thermal throttling

7. **Recommendation:** 
   - Short term: Reduce batch size by 20%
   - Long term: Improve node cooling or migrate to better-cooled node

## Escalation

If diagnosis is inconclusive:
1. Create alert for ops team review
2. Capture full diagnostic snapshot
3. Consider node drain if hardware issue suspected
