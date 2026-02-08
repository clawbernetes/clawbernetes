---
name: spot-migration
description: Cost optimization by migrating workloads between spot and on-demand capacity via MOLT
user-invocable: false
---

# Spot Migration

Migrate workloads between spot/preemptible and on-demand capacity to optimize costs while maintaining availability.

## When to Use

- "Save money on GPUs"
- "Use spot instances"
- "Migrate to cheaper capacity"
- Cost optimization during off-peak hours
- Preemption warnings received

## Workflow

1. **Identify candidates** — workloads that can tolerate interruption
2. **Check spot prices** — find cheaper capacity
3. **Migrate workloads** — move to spot with checkpoint
4. **Monitor for preemption** — be ready to migrate back
5. **Auto-fallback** — move to on-demand if preempted

## Tools Used

- `workload_list` — find migration candidates
- `molt_spot_prices` — check current spot prices
- `molt_offers` — find available spot capacity
- `molt_bid` — acquire spot capacity
- `workload_submit` — submit on new capacity
- `workload_stop` — stop old instance
- `alert_create` — preemption warnings

## Migration Candidates

Good candidates for spot:
- ✅ Fault-tolerant workloads (can checkpoint/restart)
- ✅ Batch processing jobs
- ✅ Development/testing workloads
- ✅ Non-time-critical training

Bad candidates:
- ❌ Production inference serving
- ❌ Real-time workloads
- ❌ Jobs without checkpointing
- ❌ Short jobs (<30 min, not worth migration overhead)

## Cost-Benefit Analysis

Before migration, calculate savings:

```
on_demand_cost = gpus × on_demand_price × expected_hours
spot_cost = gpus × spot_price × expected_hours
migration_overhead = 0.1 × expected_hours  // ~10% overhead
savings = on_demand_cost - spot_cost - migration_overhead

if savings > minimum_savings_threshold:
    proceed with migration
```

## Migration Steps

### 1. Prepare Workload

Ensure workload has:
- Checkpointing enabled
- Preemption signal handler
- State saved to persistent storage

### 2. Find Spot Capacity

```
molt_spot_prices(gpuModel: "A100")
molt_offers(minGpus: 4, maxPricePerHour: 2.0)
```

### 3. Acquire Spot Capacity

```
molt_bid(
  offerId: "<cheapest_offer>",
  pricePerHour: 1.50,  // bid slightly above spot price
  durationHours: 8
)
```

### 4. Migrate Workload

```
# Checkpoint current state
# (workload should auto-checkpoint on SIGTERM)

# Stop old workload
workload_stop(workloadId: "<old_id>")

# Submit on spot
workload_submit(
  name: "<name>-spot",
  ...spec,
  preemptible: true,
  labels: {
    migrated_from: "<old_id>",
    original_type: "on-demand"
  }
)
```

### 5. Set Up Preemption Alert

```
alert_create(
  name: "preemption-warning-<workload>",
  severity: "warning",
  condition: "preemption_signal == true",
  message: "Workload <name> received preemption warning"
)
```

## Preemption Handling

When preemption is detected:

1. **Checkpoint immediately** — save state to persistent storage
2. **Find on-demand capacity** — fallback to stable capacity
3. **Resume workload** — restore from checkpoint
4. **Update tracking** — log migration for cost analysis

```
# On preemption alert:
workload_submit(
  name: "<name>-fallback",
  ...spec,
  preemptible: false,  // on-demand for stability
  env: {
    RESUME_FROM_CHECKPOINT: "/checkpoints/latest"
  }
)
```

## Spot Price Monitoring

Set up continuous monitoring:

```
# Create alert for price spikes
alert_create(
  name: "spot-price-spike",
  severity: "info",
  condition: "spot_price > 0.8 × on_demand_price",
  message: "Spot prices approaching on-demand rates"
)
```

## Example Session

**User:** "Migrate my training jobs to spot to save money"

**Agent:**

1. Find training workloads:
   ```
   workload_list(labels: {type: "training"}, state: "running")
   ```

2. Check current costs vs spot:
   ```
   molt_spot_prices(gpuModel: "A100")
   ```
   
   Found: On-demand $3.00/hr, Spot $0.90/hr (70% savings)

3. Verify workloads support checkpointing (check labels/config)

4. For each eligible workload:
   - Trigger checkpoint
   - Stop on-demand instance
   - Submit on spot capacity
   - Set up preemption monitoring

5. Report migration summary:
   - 3 workloads migrated
   - Estimated savings: $12.60/hour
   - Preemption risk: Low (spot prices stable)

## Cost Tracking

After migration, track actual savings:

```
metrics_query(
  metric: "workload_cost",
  labels: {migrated_from: "<original>"},
  startTime: "-24h"
)
```

Compare to original on-demand cost for ROI analysis.
