---
name: spot-migration
description: Handle spot/preemptible GPU instance evictions — checkpoint, migrate, and resume workloads.
metadata: {"openclaw": {"always": true}}
---

# Spot Migration

## Eviction Detection

Cloud spot instances give 30-120 seconds notice before termination. Monitor via:

```bash
# AWS spot termination notice
exec host=node node=<name> command="curl -s http://169.254.169.254/latest/meta-data/spot/instance-action 2>/dev/null || echo 'no notice'"

# GCP preemptible notice
exec host=node node=<name> command="curl -s -H 'Metadata-Flavor: Google' http://metadata.google.internal/computeMetadata/v1/instance/preempted 2>/dev/null || echo 'no notice'"

# Azure spot eviction
exec host=node node=<name> command="curl -s -H Metadata:true 'http://169.254.169.254/metadata/scheduledevents?api-version=2020-07-01' 2>/dev/null || echo 'no notice'"
```

## Emergency Checkpoint

When eviction is detected:

```bash
# 1. Signal the training process to save checkpoint NOW
exec host=node node=<name> command="docker exec <container> kill -USR1 1"  # Common checkpoint signal

# 2. Or stop gracefully (gives container time to save)
exec host=node node=<name> command="docker stop --time 60 <container>"

# 3. Verify checkpoint was saved
exec host=node node=<name> command="ls -lt /checkpoints/ | head -5"
```

## Migration Steps

1. **Identify target node** — find available capacity:
```bash
# Check all nodes for available GPUs
nodes status
exec host=node node=<other-node> command="nvidia-smi --query-gpu=index,utilization.gpu,memory.free --format=csv,noheader"
```

2. **Transfer checkpoint** (if not on shared storage):
```bash
exec host=node node=<source> command="rsync -avz /checkpoints/ <target-ip>:/checkpoints/"
```

3. **Resume on new node**:
```bash
exec host=node node=<target> command="docker run -d --gpus all --name <workload> -v /checkpoints:/checkpoints <image> train.py --resume /checkpoints/latest"
```

## Prevention

- Use **shared storage** (NFS, S3, GCS) for checkpoints so migration is instant
- Save checkpoints frequently (every N steps, not just per epoch)
- Keep a warm standby node when running critical training on spot
- Set up eviction monitoring via OpenClaw cron (check every 30s)

## Cron-Based Monitoring

```
Schedule a cron job every 30 seconds that:
1. Checks spot termination metadata on all spot nodes
2. If notice detected → trigger emergency checkpoint + migration
3. Alert the user
```
