---
name: cost-optimize
description: Optimize GPU infrastructure costs — right-size workloads, manage spot instances, clean unused resources.
metadata: {"openclaw": {"always": true}}
---

# Cost Optimization

## Resource Utilization Audit

```bash
# GPU utilization (look for underutilized GPUs)
exec host=node node=<name> command="nvidia-smi --query-gpu=index,utilization.gpu,memory.used,memory.total,power.draw,power.limit --format=csv"

# Container resource usage
exec host=node node=<name> command="docker stats --no-stream --format 'table {{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.MemPerc}}'"
```

## Right-Sizing Recommendations

After collecting metrics:
- **GPU util < 20% sustained** → downsize to fewer GPUs or smaller GPU
- **VRAM < 30% used** → model fits on smaller GPU (A100 40GB → A10 24GB)
- **CPU < 10%** → reduce CPU allocation
- **Power draw < 50% of limit** → GPU is idle, consider consolidation

## Clean Unused Resources

```bash
# Remove stopped containers
exec host=node node=<name> command="docker container prune -f"

# Remove unused images
exec host=node node=<name> command="docker image prune -a -f --filter 'until=72h'"

# Remove build cache
exec host=node node=<name> command="docker builder prune -f"

# Check disk savings
exec host=node node=<name> command="docker system df"
```

## Consolidation

If multiple nodes are underutilized:
1. Identify workloads that can share a node
2. Migrate workloads to fewer nodes
3. Power down or release idle nodes
4. Report cost savings

## Power Management

```bash
# Set GPU power limit (reduce power draw for inference)
exec host=node node=<name> command="sudo nvidia-smi -pl 250"  # Set to 250W instead of 350W default

# Check current power limits
exec host=node node=<name> command="nvidia-smi --query-gpu=power.limit,power.default_limit,power.max_limit --format=csv"
```

## Cost Report

Present findings as:
- Current utilization per node
- Identified waste (idle GPUs, oversized allocations)
- Recommended actions with estimated savings
- Cleanup actions taken and disk space recovered
