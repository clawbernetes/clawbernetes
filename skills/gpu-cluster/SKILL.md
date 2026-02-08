---
name: gpu-cluster
description: Discover and manage GPU compute nodes in the Clawbernetes cluster
---

# GPU Cluster Management

You can discover and manage GPU compute nodes in the Clawbernetes cluster.

## Available Nodes

Use `node.list` to discover connected clawnode instances. Each node registers with the OpenClaw gateway and advertises its capabilities.

## Commands

### Discover Nodes

To see what nodes are available:
```
node.list
```

### Query GPU Capabilities

To check what GPUs a specific node has:
```
node.invoke <node-id> gpu.list
```

Returns: GPU count, model names, VRAM per GPU, total VRAM, PCI bus IDs.

### Check GPU Utilization

To see real-time GPU metrics on a node:
```
node.invoke <node-id> gpu.metrics
```

Returns per-GPU: utilization %, memory used/total, temperature, power draw/limit.

### Get Full Node Capabilities

To see everything a node can do:
```
node.invoke <node-id> node.capabilities
```

Returns: hostname, capabilities list, supported commands, container runtime, CPU count, total memory, GPU details, labels.

### Check Node Health

To see if a node is healthy and has resources available:
```
node.invoke <node-id> node.health
```

Returns: status, load average, memory usage, disk usage, GPU count, uptime.

## Choosing the Best Node for a Workload

When a user requests a GPU workload:

1. List all connected nodes with `node.list`
2. For each node, check `gpu.list` to see available GPUs
3. Check `gpu.metrics` to see current utilization
4. Check `node.health` to see overall load
5. Choose the node with the best combination of:
   - Enough free GPUs for the workload
   - Lowest GPU utilization
   - Lowest memory pressure
   - Lowest CPU load

For non-GPU workloads (like nginx, redis), prefer the node with:
- Lowest CPU load
- Most available memory
- No GPU contention (save GPUs for GPU workloads)

## Example Workflow

User: "I need to run a PyTorch job with 2 GPUs"

1. `node.list` -> gpu-node-1, gpu-node-2
2. `node.invoke gpu-node-1 gpu.list` -> 4x A100, 80GB each
3. `node.invoke gpu-node-1 gpu.metrics` -> GPU 0: 95%, GPU 1: 90%, GPU 2: 5%, GPU 3: 3%
4. `node.invoke gpu-node-2 gpu.list` -> 2x H100, 80GB each
5. `node.invoke gpu-node-2 gpu.metrics` -> GPU 0: 0%, GPU 1: 0%
6. Best choice: gpu-node-2 (both GPUs free, H100 > A100)
