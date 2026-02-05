# Cost Optimize

Analyze GPU cluster costs and recommend optimizations to reduce spending without impacting performance.

## When to Use

- "Reduce GPU costs"
- "Cost optimization" or "save money"
- Monthly cost review
- Budget exceeded alerts
- "Why is my bill so high?"

## Analysis Workflow

1. **Gather cost data** — current spend by workload/node
2. **Identify waste** — idle resources, over-provisioning
3. **Calculate savings** — potential optimizations
4. **Recommend actions** — prioritized by impact
5. **Implement** — with user approval

## Tools Used

- `cluster_status` — resource utilization overview
- `node_list` — find underutilized nodes
- `workload_list` — find inefficient workloads
- `metrics_query` — utilization metrics
- `molt_spot_prices` — spot pricing comparison
- `workload_scale` — right-size workloads
- `workload_stop` — terminate idle resources

## Cost Analysis Areas

### 1. Idle Resources

**Detection:**
```
metrics_query(
  metric: "gpu_utilization",
  startTime: "-24h",
  step: "1h"
)
```

Flag nodes/workloads with avg utilization < 20%

**Savings:** 
- Stop idle workloads: 100% of idle cost
- Consolidate to fewer nodes: hosting overhead

### 2. Over-Provisioned Workloads

**Detection:**
```
# For each workload, compare requested vs actual usage
metrics_query(metric: "gpu_memory_used", workloadId: "...")
metrics_query(metric: "gpu_utilization", workloadId: "...")
```

Flag if: requested_gpus × 0.5 > actual_max_usage

**Savings:**
- Right-size GPUs: 20-50% per workload
- Switch to smaller GPU models: significant

### 3. Spot Opportunity

**Detection:**
```
molt_spot_prices()
workload_list(labels: {preemptible: "false"})
```

Find on-demand workloads that could run on spot

**Savings:**
- Spot pricing: 60-80% vs on-demand
- See `spot-migration` skill

### 4. Time-Based Optimization

**Detection:**
```
# Check workload schedules
metrics_query(
  metric: "gpu_utilization",
  startTime: "-7d",
  step: "1h"
)
```

Identify off-peak hours with low utilization

**Savings:**
- Schedule batch jobs during off-peak
- Scale down inference during low-traffic hours

### 5. Reserved Capacity

**Analysis:**
If consistent baseline usage, reserved capacity is cheaper

```
# Calculate steady-state usage
avg_gpus_used = metrics average over 30 days
if avg_gpus_used > 0.7 × total_gpus:
    recommend reserved capacity
```

**Savings:**
- Reserved vs on-demand: 30-50%

## Cost Report Template

```
## Cost Analysis Report

### Current Spend (Last 30 Days)
- Total: $X,XXX
- By Node: [breakdown]
- By Workload Type: [breakdown]

### Identified Waste
| Category | Amount | % of Total |
|----------|--------|------------|
| Idle GPUs | $XXX | XX% |
| Over-provisioned | $XXX | XX% |
| On-demand (spot eligible) | $XXX | XX% |

### Recommended Actions
1. [Action] - Est. Savings: $XXX/month
2. [Action] - Est. Savings: $XXX/month
3. [Action] - Est. Savings: $XXX/month

### Total Potential Savings: $X,XXX/month (XX%)
```

## Optimization Actions

### Quick Wins (< 1 hour to implement)

1. **Stop idle workloads**
   ```
   workload_list(state: "running")
   # For each with 0% util for 24h:
   workload_stop(workloadId: "...")
   ```

2. **Scale down over-provisioned**
   ```
   workload_scale(workloadId: "...", replicas: <reduced>)
   ```

### Medium Term (1 day to implement)

3. **Migrate to spot**
   - See `spot-migration` skill
   - Prioritize fault-tolerant workloads

4. **Right-size GPU requests**
   - Update workload specs
   - Redeploy with smaller requirements

### Long Term (1 week+)

5. **Reserved capacity planning**
   - Analyze steady-state needs
   - Purchase reserved capacity

6. **Architecture optimization**
   - Model distillation (smaller models)
   - Quantization (INT8/FP16)
   - Batching improvements

## Example Session

**User:** "Analyze our GPU costs and find savings"

**Agent:**

1. Get cluster overview:
   ```
   cluster_status()
   ```

2. Query utilization:
   ```
   metrics_query(metric: "gpu_utilization", startTime: "-7d")
   ```

3. Identify waste:
   - Found 3 workloads with <10% avg utilization
   - Found 2 nodes idle 18+ hours/day
   - Found 5 on-demand workloads eligible for spot

4. Calculate savings:
   ```
   molt_spot_prices(gpuModel: "A100")
   ```

5. Generate report:
   ```
   ## Cost Optimization Report
   
   ### Current: $15,000/month
   
   ### Findings:
   - 3 idle workloads: $2,100/month waste
   - 2 underutilized nodes: $4,500/month waste
   - Spot opportunity: $3,600/month savings
   
   ### Recommended Actions:
   1. Stop "dev-experiment-old" - Savings: $700/month
   2. Stop "test-model-v2" - Savings: $700/month  
   3. Stop "batch-job-stale" - Savings: $700/month
   4. Migrate training jobs to spot - Savings: $3,600/month
   5. Consolidate nodes 7,8 - Savings: $2,250/month
   
   ### Total Potential Savings: $7,950/month (53%)
   ```

6. Offer to implement quick wins with user approval

## Ongoing Monitoring

Set up cost alerts:

```
alert_create(
  name: "daily-cost-threshold",
  severity: "warning",
  condition: "daily_cost > budget / 30 × 1.2",
  message: "Daily spend exceeds budget by 20%"
)

alert_create(
  name: "idle-resource-alert",
  severity: "info",
  condition: "gpu_utilization < 10% for 4h",
  message: "GPU idle for 4+ hours: <workload>"
)
```
