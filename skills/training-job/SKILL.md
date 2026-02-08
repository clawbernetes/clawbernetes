---
name: training-job
description: ML/AI training job orchestration with resource allocation and checkpointing
user-invocable: false
---

# Training Job

Submit and manage ML/AI training jobs with proper resource allocation, checkpointing, and monitoring.

## When to Use

- "Train a model" or "start training"
- "Fine-tune" or "fine-tuning job"
- Distributed training setup
- Long-running GPU workloads

## Workflow

1. **Validate requirements** — GPUs, memory, storage
2. **Submit workload** with appropriate settings
3. **Set up monitoring** — loss curves, GPU utilization
4. **Configure alerts** — failure detection
5. **Track progress** — checkpoints, metrics

## Tools Used

- `workload_submit` — submit training job
- `workload_get` — check status
- `workload_logs` — view training output
- `metrics_query` — monitor GPU utilization
- `alert_create` — failure notifications

## Training Job Template

```
workload_submit(
  name: "<model>-training-<timestamp>",
  image: "<training-image>",
  gpus: <num_gpus>,
  gpuMemoryMb: <memory_required>,
  
  env: {
    WORLD_SIZE: "<num_gpus>",
    MASTER_ADDR: "localhost",
    MASTER_PORT: "29500",
    CHECKPOINT_DIR: "/checkpoints",
    WANDB_PROJECT: "<project>",  // optional
  },
  
  command: ["python", "-m", "torch.distributed.launch"],
  args: [
    "--nproc_per_node=<num_gpus>",
    "train.py",
    "--config=config.yaml"
  ],
  
  labels: {
    type: "training",
    model: "<model_name>",
    experiment: "<experiment_id>"
  },
  
  preemptible: false,  // training jobs usually not preemptible
  maxRuntimeSeconds: 86400,  // 24h default
)
```

## GPU Recommendations

| Model Size | GPUs | GPU Type | Memory |
|------------|------|----------|--------|
| < 1B params | 1 | A100-40G | 40GB |
| 1-7B params | 1-4 | A100-80G | 80GB |
| 7-13B params | 4-8 | A100-80G | 80GB |
| 13-70B params | 8+ | H100-80G | 80GB |
| > 70B params | 16+ | H100-80G | 80GB |

## Monitoring Setup

After job submission, set up monitoring:

```
# Monitor GPU utilization (should be >80% for healthy training)
metrics_query(
  metric: "gpu_utilization",
  workloadId: "<id>",
  startTime: "-30m"
)

# Monitor GPU memory (watch for OOM risk)
metrics_query(
  metric: "gpu_memory_used",
  workloadId: "<id>",
  startTime: "-30m"
)

# Create alert for job failure
alert_create(
  name: "training-failed-<job_id>",
  severity: "critical",
  condition: "workload_state == 'failed'",
  message: "Training job <job_id> failed"
)
```

## Checkpoint Strategy

Recommend checkpoint frequency based on job length:

| Job Duration | Checkpoint Interval |
|--------------|---------------------|
| < 1 hour | Every 10 minutes |
| 1-4 hours | Every 30 minutes |
| 4-24 hours | Every 1 hour |
| > 24 hours | Every 2 hours |

## Common Issues

### Job Stuck at 0% GPU

**Cause:** Data loading bottleneck

**Solution:** 
- Increase `num_workers` in DataLoader
- Use faster storage (NVMe)
- Pre-process data

### Loss Goes NaN

**Cause:** Learning rate too high, gradient explosion

**Solution:**
- Reduce learning rate
- Enable gradient clipping
- Check data for anomalies

### OOM During Training

**Cause:** Batch size too large, model too big

**Solution:**
- Reduce batch size
- Enable gradient checkpointing
- Use mixed precision (fp16/bf16)
- Request more GPUs

## Example Session

**User:** "Train a 7B parameter model for fine-tuning"

**Agent:**

1. Determine requirements:
   - 7B model → need ~4x A100-80G for comfortable training
   - Estimated time: 4-8 hours for fine-tuning

2. Submit job:
   ```
   workload_submit(
     name: "7b-finetune-20240201",
     image: "nvcr.io/nvidia/pytorch:24.01-py3",
     gpus: 4,
     gpuMemoryMb: 81920,
     env: {
       WORLD_SIZE: "4",
       NCCL_DEBUG: "INFO"
     },
     labels: {type: "training", model: "7b-finetune"},
     maxRuntimeSeconds: 28800
   )
   ```

3. Set up monitoring:
   ```
   alert_create(
     name: "7b-finetune-failed",
     severity: "critical",
     condition: "workload_state == 'failed'",
     message: "7B fine-tuning job failed"
   )
   ```

4. Report job ID and estimated completion time

## Distributed Training Notes

For multi-node training:
- Ensure NCCL environment variables are set
- Use high-bandwidth interconnect (InfiniBand preferred)
- Set `NCCL_IB_DISABLE=0` for IB, `=1` for Ethernet
- Monitor inter-node communication bandwidth
