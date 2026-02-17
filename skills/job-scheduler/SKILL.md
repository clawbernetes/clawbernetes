---
name: job-scheduler
description: Schedule and manage GPU jobs and cron tasks across the fleet.
metadata: {"openclaw": {"always": true}}
---

# Job Scheduler

## Create a One-Shot Job

```bash
# Run a job on a specific node
nodes invoke --node <name> --command job.create --params '{
  "name": "fine-tune-llama",
  "image": "pytorch/pytorch:latest",
  "command": "python train.py --model llama --epochs 10",
  "gpus": "all",
  "env": {"WANDB_API_KEY": "..."},
  "volumes": ["/data:/data"],
  "restartPolicy": "never"
}'
```

## Check Job Status

```bash
nodes invoke --node <name> --command job.status --params '{"name": "fine-tune-llama"}'
```

## Get Job Logs

```bash
nodes invoke --node <name> --command job.logs --params '{"name": "fine-tune-llama", "tail": 100}'
```

## Delete a Job

```bash
nodes invoke --node <name> --command job.delete --params '{"name": "fine-tune-llama"}'
```

## Create a Cron Job

```bash
# Run GPU health check every hour
nodes invoke --node <name> --command cron.create --params '{
  "name": "gpu-health",
  "schedule": "0 * * * *",
  "command": "nvidia-smi --query-gpu=temperature.gpu,ecc.errors.uncorrected.volatile.total --format=csv,noheader",
  "image": "nvidia/cuda:12.4-base"
}'

# Daily cleanup at 3am
nodes invoke --node <name> --command cron.create --params '{
  "name": "docker-cleanup",
  "schedule": "0 3 * * *",
  "command": "docker system prune -f --filter until=72h"
}'
```

## List Cron Jobs

```bash
nodes invoke --node <name> --command cron.list --params '{}'
```

## Suspend / Resume

```bash
nodes invoke --node <name> --command cron.suspend --params '{"name": "gpu-health"}'
nodes invoke --node <name> --command cron.resume --params '{"name": "gpu-health"}'
```

## Trigger Manually

```bash
nodes invoke --node <name> --command cron.trigger --params '{"name": "gpu-health"}'
```

## Fleet-Wide Scheduling

To schedule across multiple nodes:
1. Use `nodes status` to list available nodes
2. Pick node based on GPU availability and load
3. Create job on the selected node
4. Monitor via `job.status` and `job.logs`

For recurring fleet-wide checks, use OpenClaw's cron tool to schedule agent turns that iterate over all nodes.
