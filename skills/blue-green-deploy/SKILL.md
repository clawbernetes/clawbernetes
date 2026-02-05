# Blue-Green Deploy

Deploy with zero-downtime using blue-green strategy — maintain two identical environments and switch traffic instantly.

## When to Use

- "Deploy with zero downtime"
- "Blue-green deployment"
- Production deployments requiring instant rollback capability
- Critical services that cannot tolerate any downtime

## How It Works

```
        ┌─────────────┐
        │   Traffic   │
        │   Router    │
        └──────┬──────┘
               │
       ┌───────┴───────┐
       ▼               ▼
┌─────────────┐ ┌─────────────┐
│    BLUE     │ │   GREEN     │
│  (current)  │ │   (new)     │
│   v1.0      │ │   v1.1      │
│  ████████   │ │  ░░░░░░░░   │
│  (active)   │ │  (standby)  │
└─────────────┘ └─────────────┘
```

After verification, switch traffic:

```
┌─────────────┐ ┌─────────────┐
│    BLUE     │ │   GREEN     │
│   v1.0      │ │   v1.1      │
│  ░░░░░░░░   │ │  ████████   │
│  (standby)  │ │  (active)   │
└─────────────┘ └─────────────┘
```

## Workflow

1. **Deploy green** — new version alongside blue
2. **Verify green** — health checks, smoke tests
3. **Switch traffic** — route 100% to green
4. **Monitor** — watch for issues
5. **Cleanup or rollback** — remove blue or switch back

## Tools Used

- `workload_submit` — deploy green environment
- `workload_get` — check deployment status
- `workload_list` — list blue/green instances
- `metrics_query` — verify health
- `workload_stop` — cleanup old environment
- `alert_create` — notification on completion

## Deployment Steps

### 1. Identify Current (Blue) Deployment

```
workload_list(labels: {app: "<name>", env: "production"})
```

Record current:
- Workload ID
- Image version
- Replica count
- Resource allocation

### 2. Deploy Green

```
workload_submit(
  name: "<app>-green",
  image: "<new_image>",
  gpus: <same_as_blue>,
  labels: {
    app: "<name>",
    env: "production",
    deployment: "green",
    version: "<new_version>"
  }
)
```

### 3. Verify Green Health

Wait for workload to be running:
```
workload_get(workloadId: "<green_id>")
# Confirm state: "running"
```

Check health metrics:
```
metrics_query(
  metric: "health_check_success",
  workloadId: "<green_id>",
  startTime: "-5m"
)
```

Run smoke tests:
```
# Application-specific health verification
# e.g., inference endpoint returns valid response
```

### 4. Switch Traffic

Update routing to point to green:
```
# This depends on your service mesh / load balancer
# In Clawbernetes, update the service selector

workload_submit(
  name: "<app>-service",
  labels: {
    app: "<name>",
    activeDeployment: "green"
  }
)
```

### 5. Monitor

Watch error rates after switch:
```
metrics_query(
  metric: "request_error_rate",
  labels: {app: "<name>"},
  startTime: "-5m",
  step: "10s"
)
```

### 6. Cleanup or Rollback

**If successful:**
```
# Stop blue (old version)
workload_stop(workloadId: "<blue_id>")

# Rename green to blue for next deployment
# (or keep naming convention)
```

**If issues detected:**
```
# Instant rollback — switch traffic back to blue
workload_submit(
  name: "<app>-service",
  labels: {
    activeDeployment: "blue"
  }
)

# Stop failed green
workload_stop(workloadId: "<green_id>")
```

## Comparison with Canary

| Aspect | Blue-Green | Canary |
|--------|------------|--------|
| Traffic split | 0% → 100% instant | Gradual (10% → 50% → 100%) |
| Rollback speed | Instant | Fast (stop canary) |
| Resource cost | 2x during deploy | 1.1x during deploy |
| Risk exposure | All-or-nothing | Gradual |
| Best for | Critical services | Testing in production |

## Requirements

- **Resources:** Need 2x capacity during deployment
- **Stateless:** Workloads should be stateless (or handle state migration)
- **Health checks:** Must have reliable health verification
- **Traffic routing:** Need ability to switch traffic instantly

## Example Session

**User:** "Deploy inference-model v2.0 using blue-green"

**Agent:**

1. Get current deployment:
   ```
   workload_list(labels: {app: "inference-model", env: "production"})
   ```
   Found: inference-model-blue (v1.9), 4 GPUs, running

2. Deploy green:
   ```
   workload_submit(
     name: "inference-model-green",
     image: "registry/inference-model:v2.0",
     gpus: 4,
     labels: {
       app: "inference-model",
       env: "production",
       deployment: "green",
       version: "v2.0"
     }
   )
   ```

3. Wait for green to be ready:
   ```
   workload_get(workloadId: "inference-model-green-...")
   # State: running ✓
   ```

4. Verify health:
   ```
   metrics_query(
     metric: "health_check_success",
     workloadId: "...",
     startTime: "-2m"
   )
   # 100% success rate ✓
   ```

5. Switch traffic:
   ```
   # Update service routing to green
   ```

6. Monitor for 5 minutes:
   ```
   metrics_query(metric: "request_error_rate", startTime: "-5m")
   # Error rate: 0.1% (normal) ✓
   ```

7. Cleanup:
   ```
   workload_stop(workloadId: "inference-model-blue-...")
   ```

8. Report success:
   ```
   alert_create(
     name: "deploy-complete",
     severity: "info",
     message: "inference-model v2.0 deployed successfully via blue-green"
   )
   ```

## Rollback Procedure

If issues detected after switch:

1. **Immediate:** Switch traffic back to blue
2. **Verify:** Confirm blue is healthy
3. **Stop green:** Remove failed deployment
4. **Alert:** Notify team of failed deployment
5. **Investigate:** Check green logs for failure cause

```
# Instant rollback
# 1. Switch traffic
<update routing to blue>

# 2. Verify
metrics_query(metric: "request_error_rate", startTime: "-1m")

# 3. Stop green
workload_stop(workloadId: "<green_id>")

# 4. Alert
alert_create(
  name: "deploy-rollback",
  severity: "warning",
  message: "v2.0 deployment rolled back due to errors"
)
```

## Tips

- Always verify green is fully healthy before switching
- Keep blue running for at least 15-30 minutes after switch
- Have rollback command ready before switching
- Monitor closely for first 5 minutes after switch
- Use during low-traffic periods when possible
