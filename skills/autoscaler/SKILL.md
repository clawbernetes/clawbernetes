---
name: autoscaler
description: GPU-aware autoscaling policies for Clawbernetes node pools
metadata:
  openclaw:
    requires:
      bins: ["clawnode"]
---

# Autoscaler

You can create and manage autoscaling policies for GPU node pools on Clawbernetes nodes.

Requires the `autoscaler` feature on the node.

## Commands

### Create an Autoscaler

```
node.invoke <node-id> autoscale.create {
  "target": "gpu-pool",
  "minReplicas": 2,
  "maxReplicas": 20,
  "policy": "target_utilization",
  "targetUtilization": 70.0,
  "tolerance": 10.0
}
```

**Parameters:**
- `target` (required): Name of the node pool to autoscale
- `minReplicas` (optional, default 1): Minimum number of nodes
- `maxReplicas` (optional, default 10): Maximum number of nodes
- `policy` (optional, default "target_utilization"): Scaling policy type

**Policy: `target_utilization`**
- `targetUtilization` (optional, default 70.0): Target GPU utilization percentage
- `tolerance` (optional, default 10.0): Tolerance band before scaling

**Policy: `queue_depth`**
- `targetQueueDepth` (optional, default 5): Target jobs per node
- `upThreshold` (optional, default 10): Queue depth to trigger scale-up
- `downThreshold` (optional, default 2): Queue depth to trigger scale-down

### Check Autoscaler Status

```
node.invoke <node-id> autoscale.status { "target": "gpu-pool" }
```

Returns: current nodes, total GPUs, min/max settings, enabled status, pending actions.

### Adjust Autoscaler Settings

```
node.invoke <node-id> autoscale.adjust {
  "target": "gpu-pool",
  "minReplicas": 3,
  "maxReplicas": 50
}
```

Updates min/max replicas and policy parameters without recreating the autoscaler.

### Delete an Autoscaler

```
node.invoke <node-id> autoscale.delete { "target": "gpu-pool" }
```

## Scaling Policies

| Policy | Scales Based On | Best For |
|--------|----------------|----------|
| `target_utilization` | GPU utilization % | Steady workloads, inference |
| `queue_depth` | Pending job count | Batch processing, training queues |

## Common Patterns

### Inference Cluster (Utilization-Based)
```json
{
  "target": "inference-pool",
  "minReplicas": 2,
  "maxReplicas": 20,
  "policy": "target_utilization",
  "targetUtilization": 70.0,
  "tolerance": 10.0
}
```

### Training Queue (Queue-Based)
```json
{
  "target": "training-pool",
  "minReplicas": 1,
  "maxReplicas": 50,
  "policy": "queue_depth",
  "targetQueueDepth": 3,
  "upThreshold": 8,
  "downThreshold": 1
}
```

## Workflow

1. `autoscale.create` to set up a scaling policy for a node pool
2. `autoscale.status` to monitor current scale and pending actions
3. `autoscale.adjust` to tune parameters as traffic patterns change
4. `autoscale.delete` to remove autoscaling and manage pool size manually
