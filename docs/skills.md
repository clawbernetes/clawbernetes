# Skills

The plugin bundles 20 skills that teach the agent how to use Clawbernetes. Skills are markdown documents — the agent reads them as context to understand what commands are available and how to combine them.

---

## Core Skills (always available)

| Skill | What the agent learns |
|-------|----------------------|
| `gpu-cluster` | Discover nodes, list GPUs, read metrics, check health |
| `workload-manager` | Run containers, monitor workloads, view logs, stop/restart |
| `system-admin` | System info, node management, cluster administration |
| `secrets-config` | Create/rotate encrypted secrets, manage config maps |
| `observability` | Query metrics, emit events, create alert rules |

---

## Feature Skills (available when node features are enabled)

| Skill | Feature | What the agent learns |
|-------|---------|----------------------|
| `auth-rbac` | `auth` | API keys, RBAC roles, audit logs |
| `job-scheduler` | always | One-shot jobs, cron schedules |
| `storage` | `storage` | Volumes, snapshots, backups |
| `networking` | `network` | Services, ingress, WireGuard mesh |
| `autoscaler` | `autoscaler` | Auto-scaling policies |
| `molt-marketplace` | `molt` | P2P GPU trading |

---

## Workflow Skills (reference guides for complex operations)

| Skill | When the agent uses it |
|-------|----------------------|
| `canary-release` | "Deploy safely" or "canary rollout" |
| `blue-green-deploy` | "Zero-downtime deployment" |
| `auto-heal` | Node failures, crash loops, resource exhaustion |
| `incident-response` | "Production is down", critical alerts |
| `gpu-diagnose` | "GPU is slow", memory errors, thermal issues |
| `training-job` | "Train a model", distributed training setup |
| `cost-optimize` | "Reduce costs", underutilization analysis |
| `spot-migration` | "Use cheaper GPUs", spot/on-demand migration |

---

## See Also

- [Use Cases](use-cases.md) — Example conversations showing skills in action
- [Architecture](architecture.md) — How skills connect to node commands
