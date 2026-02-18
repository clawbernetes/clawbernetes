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

## Natural Language → Commands

When the user asks about their cluster in plain language, map the question to the right tool/command. Think like a sysadmin who knows the cluster inside out.

**Information queries** — use `claw_fleet_status`, `claw_gpu_inventory`, `claw_multi_invoke`, or `nodes invoke`:
| User says | You do |
|-----------|--------|
| "What GPUs do we have?" | `claw_gpu_inventory` |
| "Show kernel versions across all nodes" | `claw_multi_invoke` → `system.info` on all nodes, extract `kernel_version` |
| "How much RAM is free?" | `claw_multi_invoke` → `system.info`, compute `total - used` |
| "Are all nodes healthy?" | `claw_fleet_status` |
| "What's running on morpheus?" | `nodes invoke` → `workload.list` on morpheus |
| "GPU temps?" | `claw_multi_invoke` → `gpu.metrics` on all nodes |
| "Which node has the most VRAM?" | `claw_gpu_inventory`, compare `memory_total_mb` |
| "Is Docker running everywhere?" | `claw_multi_invoke` → `system.run` with `docker info` |
| "Disk space?" | `claw_multi_invoke` → `system.run` with `df -h` |
| "Who's hogging the GPU?" | `nodes invoke` → `gpu.metrics` + `workload.stats` |

**Action queries** — use `claw_deploy`, `nodes invoke`, or `system.run`:
| User says | You do |
|-----------|--------|
| "Deploy X on the best node" | `claw_deploy` (auto-places based on resources) |
| "Run nginx on morpheus" | `nodes invoke` → `workload.run` |
| "Kill that container" | `nodes invoke` → `workload.stop` |
| "Update all nodes" | `claw_multi_invoke` → `system.run` with update commands |
| "Restart Docker on morpheus" | `nodes invoke` → `system.run` with `sudo systemctl restart docker` |

**Key principle**: The user should never need to know command names. Translate their intent into the right API calls, run them, and present a clean summary.

## Workflow Pattern

1. **Discover**: `nodes status` to see connected nodes
2. **Assess**: Run `gpu.list` and `gpu.metrics` across nodes
3. **Decide**: Reason about placement based on GPU type, memory, thermals, utilization
4. **Execute**: Deploy/scale/migrate workloads
5. **Monitor**: Set up cron jobs for health checks
6. **Report**: Summarize actions and current state
