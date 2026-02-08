---
name: canary-release
description: Gradual canary deployment strategy for Clawbernetes workloads
user-invocable: false
---

# Canary Release

Deploy workloads with canary strategy — gradual rollout with automatic rollback on failure.

## When to Use

- Deploying new versions of production workloads
- User says "deploy with canary" or "canary release"
- Risk-sensitive deployments that need validation before full rollout

## Workflow

1. **Deploy canary** (10% traffic)
2. **Monitor metrics** for 5 minutes
3. **Decision gate**: promote or rollback based on error rate
4. **Gradual promotion**: 10% → 50% → 100%
5. **Alert** on outcome

## Tools Used

- `workload_submit` — deploy the canary workload
- `workload_scale` — scale up during promotion
- `metrics_query` — monitor error rate and latency
- `alert_create` — notify on success/failure
- `workload_stop` — stop old version after promotion

## Parameters

| Param | Default | Description |
|-------|---------|-------------|
| `errorThreshold` | 1% | Max error rate before rollback |
| `latencyThreshold` | 500ms | Max p99 latency before rollback |
| `observationMinutes` | 5 | Time to observe each stage |
| `stages` | [10, 50, 100] | Traffic percentages for each stage |

## Example Usage

**User:** "Deploy my-model v2.1 to production with canary"

**Agent steps:**

1. Check current deployment state:
   ```
   workload_list(labels: {app: "my-model", env: "production"})
   ```

2. Submit canary workload:
   ```
   workload_submit(
     name: "my-model-v2.1-canary",
     image: "registry/my-model:v2.1",
     gpus: 1,
     labels: {app: "my-model", version: "v2.1", canary: "true"}
   )
   ```

3. Monitor error rate:
   ```
   metrics_query(
     metric: "request_error_rate",
     labels: {app: "my-model", version: "v2.1"},
     startTime: "-5m"
   )
   ```

4. If error rate < 1%, scale to 50%:
   ```
   workload_scale(workloadId: "...", replicas: 5)
   ```

5. Continue monitoring, then promote to 100%

6. Stop old version:
   ```
   workload_stop(workloadId: "my-model-v2.0-...")
   ```

7. Alert on completion:
   ```
   alert_create(
     name: "canary-complete",
     severity: "info",
     message: "my-model v2.1 successfully deployed via canary"
   )
   ```

## Rollback Conditions

Automatically rollback if:
- Error rate exceeds threshold for 2+ minutes
- Latency p99 exceeds threshold
- Health checks fail
- GPU memory errors detected

## Rollback Steps

1. Stop all canary replicas
2. Verify old version still healthy
3. Create alert for failed deployment
4. Log failure reason for analysis

## Tips

- Start with low traffic (10%) to catch obvious issues
- Use longer observation periods for critical workloads
- Consider time-of-day — avoid canary during peak hours
- Check GPU utilization during canary to validate resource needs
