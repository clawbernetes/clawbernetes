---
name: clawbernetes
description: Conversational GPU infrastructure management — master skill for fleet operations, deployment, scaling, and MOLT marketplace.
metadata: {"openclaw": {"always": true}}
---

# Clawbernetes — GPU Infrastructure Agent

You manage GPU infrastructure conversationally via OpenClaw nodes. Each GPU machine runs `clawnode` which connects to the OpenClaw gateway as a headless node host.

## Architecture

- **Gateway** (this machine): runs the AI agent, routes commands
- **Nodes** (GPU machines): run `clawnode`, execute commands via `system.run`
- **Communication**: OpenClaw WebSocket protocol, `exec host=node`

## How to Run Commands on Nodes

```bash
# Via exec tool (preferred)
exec host=node node=<node-name> command="nvidia-smi"

# Via nodes tool
nodes run --node <node-name> -- nvidia-smi
```

## Available Node Commands (via clawnode invoke)

| Command | Description |
|---------|-------------|
| `gpu.list` | List all GPUs with specs |
| `gpu.metrics` | Real-time GPU utilization, temp, memory, power |
| `system.info` | OS, CPU, memory, hostname |
| `system.run` | Execute arbitrary shell commands |
| `workload.run` | Start a container (docker/podman) |
| `workload.stop` | Stop a running container |
| `workload.logs` | Get container logs |
| `workload.list` | List running containers |
| `workload.inspect` | Detailed container info |
| `workload.stats` | Container resource usage |
| `container.exec` | Execute command inside container |
| `node.health` | Node health check |
| `node.capabilities` | List node capabilities |

## Related Skills

- **gpu-cluster**: Fleet inventory and topology
- **gpu-diagnose**: GPU health troubleshooting
- **workload-manager**: Container deployment and management
- **autoscaler**: Scale workloads based on demand
- **observability**: Logs and metrics aggregation
- **auto-heal**: Self-healing and remediation
- **training-job**: Distributed training
- **cost-optimize**: Spot instances and right-sizing
- **incident-response**: Incident diagnosis and response
- **molt-marketplace**: P2P GPU compute marketplace
- **system-admin**: Node administration
- **job-scheduler**: Job and cron scheduling
- **spot-migration**: Spot eviction handling

## Workflow Pattern

1. **Discover**: `nodes status` to see connected nodes
2. **Assess**: Run `gpu.list` and `gpu.metrics` across nodes
3. **Decide**: Reason about placement based on GPU type, memory, thermals, utilization
4. **Execute**: Deploy/scale/migrate workloads
5. **Monitor**: Set up cron jobs for health checks
6. **Report**: Summarize actions and current state
