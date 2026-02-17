---
name: autoscaler
description: Scale GPU workloads up or down based on utilization, queue depth, or request latency.
metadata: {"openclaw": {"always": true}}
---

# Autoscaler

## Scaling Decisions

Before scaling, gather:
1. **GPU utilization** across nodes (`nvidia-smi`)
2. **Request queue** depth / latency (application metrics)
3. **Available capacity** (idle GPUs on other nodes)
4. **Cost** implications

## Check Current Load

```bash
# GPU utilization across a node
exec host=node node=<name> command="nvidia-smi --query-gpu=index,utilization.gpu,memory.used,memory.total --format=csv,noheader"

# Container resource usage
exec host=node node=<name> command="docker stats --no-stream --format '{{.Name}}: CPU={{.CPUPerc}} MEM={{.MemUsage}}'"

# Application-level metrics (if exposed)
exec host=node node=<name> command="curl -s http://localhost:8000/metrics 2>/dev/null | grep -E 'request_queue|latency|throughput' || echo 'no app metrics'"
```

## Scale Up (add replicas)

```bash
# Find a node with available GPUs
# 1. Check all nodes for idle GPUs (utilization < 10%)
# 2. Pick the node with most free VRAM
# 3. Deploy additional replica

exec host=node node=<target-node> command="docker run -d --gpus all --name <service>-replica-2 -p <port>:<port> <image> <args>"
```

## Scale Down

```bash
# Identify underutilized replicas
# If GPU utilization < 5% for extended period, remove replica
exec host=node node=<name> command="docker stop <service>-replica-2 && docker rm <service>-replica-2"
```

## Scaling Thresholds (defaults)

| Metric | Scale Up | Scale Down |
|--------|----------|------------|
| GPU util | > 85% sustained 5min | < 15% sustained 10min |
| VRAM usage | > 90% | < 30% |
| Request latency | > 2x baseline | < 0.5x baseline |
| Queue depth | > 100 pending | 0 pending for 10min |

## Monitoring via Cron

Set up periodic checks:
```
Use cron tool to schedule a job every 5 minutes that:
1. Checks GPU utilization on all nodes
2. If any node > 85% for 3 consecutive checks → scale up
3. If all replicas < 15% → scale down one replica
4. Report scaling actions to the user
```
