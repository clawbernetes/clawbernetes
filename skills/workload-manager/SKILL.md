---
name: workload-manager
description: Deploy, manage, and inspect containerized workloads on GPU nodes using Docker or Podman.
metadata: {"openclaw": {"always": true}}
---

# Workload Manager

## Deploy a Container

```bash
# Basic GPU workload
exec host=node node=<name> command="docker run -d --gpus all --name <workload-name> -p <host-port>:<container-port> <image> <cmd>"

# With specific GPUs
exec host=node node=<name> command="docker run -d --gpus '\"device=0,1\"' --name <name> -v /data:/data <image>"

# With resource limits
exec host=node node=<name> command="docker run -d --gpus all --memory=64g --cpus=16 --shm-size=16g --name <name> <image>"

# vLLM inference example
exec host=node node=<name> command="docker run -d --gpus all -p 8000:8000 --name vllm-llama --shm-size=16g vllm/vllm-openai --model meta-llama/Llama-3-70B-Instruct --tensor-parallel-size 4"
```

**Important flags for ML:**
- `--gpus all` or `--gpus '"device=0,1"'` — GPU access
- `--shm-size=16g` — shared memory for NCCL/distributed training
- `-v /data:/data` — mount data volumes
- `--ipc=host` — alternative to shm-size for multi-process

## List Workloads

```bash
exec host=node node=<name> command="docker ps --format 'table {{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}'"

# Include stopped
exec host=node node=<name> command="docker ps -a --format 'table {{.Names}}\t{{.Image}}\t{{.Status}}\t{{.CreatedAt}}'"
```

## Inspect

```bash
exec host=node node=<name> command="docker inspect <container> --format '{{.State.Status}} {{.HostConfig.DeviceRequests}}'"

# Resource usage
exec host=node node=<name> command="docker stats --no-stream --format 'table {{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.NetIO}}'"
```

## Logs

```bash
exec host=node node=<name> command="docker logs --tail 100 <container>"
exec host=node node=<name> command="docker logs --since 5m <container>"
```

## Stop / Remove

```bash
exec host=node node=<name> command="docker stop <container>"
exec host=node node=<name> command="docker rm <container>"
exec host=node node=<name> command="docker stop <container> && docker rm <container>"
```

## Execute Inside Container

```bash
exec host=node node=<name> command="docker exec <container> nvidia-smi"
exec host=node node=<name> command="docker exec <container> python -c 'import torch; print(torch.cuda.device_count())'"
```

## Health Check Pattern

After deploying, verify:
1. Container is running: `docker ps | grep <name>`
2. GPUs visible inside: `docker exec <name> nvidia-smi`
3. Service responding: `curl -s http://localhost:<port>/health`
4. No OOM or errors: `docker logs --tail 20 <name>`
