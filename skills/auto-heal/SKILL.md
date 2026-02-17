---
name: auto-heal
description: Detect GPU and container failures, automatically remediate by restarting, migrating, or escalating.
metadata: {"openclaw": {"always": true}}
---

# Auto-Heal

## Detection Checks

Run these periodically (via cron or heartbeat):

```bash
# 1. Container health
exec host=node node=<name> command="docker ps -a --filter 'status=exited' --filter 'status=dead' --format '{{.Names}} {{.Status}}'"

# 2. GPU errors
exec host=node node=<name> command="nvidia-smi --query-gpu=index,ecc.errors.uncorrected.volatile.total,temperature.gpu --format=csv,noheader"

# 3. OOM kills
exec host=node node=<name> command="dmesg | grep -i 'out of memory\|oom' | tail -5"

# 4. Hung processes (GPU util 0% but process exists)
exec host=node node=<name> command="nvidia-smi --query-compute-apps=pid,used_gpu_memory --format=csv,noheader"
```

## Remediation Actions

### Container Crash → Restart
```bash
exec host=node node=<name> command="docker restart <container>"
```
If crashes > 3 times in 10 minutes → **escalate to user**, don't restart.

### GPU Thermal Throttle → Reduce Load
1. Check which container is on the hot GPU
2. If possible, migrate to a cooler node
3. If not, reduce batch size or pause the workload

### ECC Errors → Migrate Off
```bash
# 1. Identify workloads on the bad GPU
exec host=node node=<name> command="nvidia-smi -i <gpu-index> --query-compute-apps=pid,name --format=csv"
# 2. Stop workload
exec host=node node=<name> command="docker stop <container>"
# 3. Redeploy on healthy node
exec host=node node=<healthy-node> command="docker run -d --gpus all --name <container> <image>"
```

### OOM Kill → Restart with More Memory
```bash
exec host=node node=<name> command="docker rm <container> && docker run -d --gpus all --memory=128g --shm-size=32g --name <container> <image>"
```

### Node Unreachable → Alert
If `nodes status` shows a node offline:
1. Notify user immediately
2. Check if workloads were running on that node
3. Offer to redeploy on available nodes

## Decision Matrix

| Issue | Auto-Fix? | Action |
|-------|-----------|--------|
| Container exited (code 0) | No | Inform user (normal completion) |
| Container exited (code 1) | Yes (1x) | Restart, then escalate |
| Container OOMKilled | Yes | Restart with 2x memory |
| GPU ECC uncorrectable | Yes | Migrate workload off GPU |
| GPU > 90°C | Yes | Throttle workload |
| Node offline | No | Alert user, offer migration |
| Disk > 95% | Yes | Clean docker images/logs |
