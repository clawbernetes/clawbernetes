---
name: training-job
description: Manage distributed GPU training jobs — launch, monitor checkpoints, handle failures, and report progress.
metadata: {"openclaw": {"always": true}}
---

# Training Job Management

## Launch Single-Node Training

```bash
exec host=node node=<name> command="docker run -d --gpus all --shm-size=16g --name train-<model> \
  -v /data:/data -v /checkpoints:/checkpoints \
  <training-image> \
  torchrun --nproc_per_node=<num_gpus> train.py \
  --model <model> --data /data --output /checkpoints --batch-size <bs>"
```

## Launch Multi-Node Training

On each node:
```bash
# Node 0 (master)
exec host=node node=<master> command="docker run -d --gpus all --network host --shm-size=32g --name train-master \
  -v /data:/data -v /checkpoints:/checkpoints \
  <image> torchrun --nproc_per_node=<gpus> --nnodes=<num_nodes> --node_rank=0 \
  --master_addr=<master_ip> --master_port=29500 train.py --args"

# Node 1..N (workers)
exec host=node node=<worker> command="docker run -d --gpus all --network host --shm-size=32g --name train-worker-<N> \
  -v /data:/data \
  <image> torchrun --nproc_per_node=<gpus> --nnodes=<num_nodes> --node_rank=<N> \
  --master_addr=<master_ip> --master_port=29500 train.py --args"
```

## Monitor Progress

```bash
# Check training logs for loss/epoch
exec host=node node=<name> command="docker logs --tail 30 train-<id> 2>&1 | grep -iE 'epoch|loss|step|accuracy'"

# Check GPU utilization (should be high during training)
exec host=node node=<name> command="nvidia-smi --query-gpu=index,utilization.gpu,memory.used --format=csv,noheader"

# Check for checkpoint saves
exec host=node node=<name> command="ls -lht /checkpoints/ | head -10"
```

## Handle Failures

If training crashes:
1. Check logs: `docker logs train-<id> 2>&1 | tail -50`
2. Common issues:
   - **NCCL timeout** → check network between nodes, try `NCCL_DEBUG=INFO`
   - **CUDA OOM** → reduce batch size or gradient accumulation
   - **Checkpoint corrupt** → restart from previous checkpoint
3. Resume from checkpoint:
```bash
exec host=node node=<name> command="docker run -d --gpus all --name train-resume \
  -v /checkpoints:/checkpoints <image> train.py --resume /checkpoints/latest"
```

## Cleanup

```bash
exec host=node node=<name> command="docker stop train-<id> && docker rm train-<id>"
```
