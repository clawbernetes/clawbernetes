---
name: gpu-cluster
description: GPU fleet inventory — list nodes, detect GPUs, map topology, and report cluster capacity.
metadata: {"openclaw": {"always": true}}
---

# GPU Cluster Inventory

## List Connected Nodes

```bash
# Get all connected nodes
nodes status
# Detailed info about a specific node
nodes describe --node <name>
```

## GPU Inventory (per node)

```bash
# Quick GPU list
exec host=node node=<name> command="nvidia-smi --query-gpu=index,name,uuid,memory.total,pci.bus_id --format=csv,noheader"

# Full GPU details with topology
exec host=node node=<name> command="nvidia-smi topo -m"

# NVLink status
exec host=node node=<name> command="nvidia-smi nvlink --status"

# AMD GPUs
exec host=node node=<name> command="rocm-smi --showid --showproductname --showmeminfo vram"
```

## Fleet-Wide Inventory

For each connected node, collect:
1. GPU count, model, and VRAM
2. NVLink/PCIe topology
3. CPU cores and system RAM
4. Container runtime (docker/podman)
5. Available disk space

```bash
# System overview
exec host=node node=<name> command="uname -a && nproc && free -h && df -h / | tail -1"

# Docker status
exec host=node node=<name> command="docker info --format '{{.ServerVersion}} containers={{.Containers}} images={{.Images}}' 2>/dev/null || echo 'docker not available'"
```

## Report Format

Present inventory as a table:

| Node | GPUs | Model | VRAM | NVLink | CPU | RAM | Status |
|------|------|-------|------|--------|-----|-----|--------|

## Scheduling Hints

When recommending placement:
- **NVLink nodes** → multi-GPU training (tensor parallelism)
- **PCIe-only nodes** → inference, single-GPU jobs
- **High VRAM** → large models (70B+ need 80GB+ per GPU)
- **Cool GPUs** (<70°C) → prefer for new workloads over hot ones
