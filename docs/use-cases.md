# Use Cases

Real conversations showing what Clawbernetes can do. These are examples of talking to the agent through OpenClaw (web UI, Slack, Discord, CLI, etc).

---

## ML Training

```
You:   "I need to fine-tune Llama 3 8B on my custom dataset. I have it in /data/training."
Agent:  Found 2 nodes with available GPUs. node-01 has 4x A100 (80GB each).
        Setting up distributed training with torchrun across 4 GPUs.
        Mounting /data/training as a volume.
        Deploying... workload fine-tune-llama running.

You:   "Set up a cron to checkpoint every 2 hours"
Agent:  Created cron job: saves checkpoint to /data/checkpoints every 2h.

You:   "GPU 2 seems slow"
Agent:  Checking GPU metrics on node-01...
        GPU 2: utilization 67% (vs 94% on GPUs 0,1,3), temp 91C — thermal throttling.
        Recommend: migrate workload to node-02 (8x H100, 42C ambient).
        Should I migrate?
```

---

## Inference Serving

```
You:   "Deploy a vLLM server for Mixtral 8x7B, make it accessible on port 8000"
Agent:  Mixtral 8x7B needs ~90GB VRAM. node-03 has 2x A100-80GB free.
        Deploying vllm/vllm-openai with tensor parallelism across 2 GPUs.
        Creating ingress route: https://mixtral.internal:8000
        Health check configured. Workload running.

You:   "Scale to handle more traffic"
Agent:  Current: 1 replica on node-03 (2 GPUs).
        node-01 has 4x A100 available. Deploying second replica.
        Load balancing across both endpoints.
```

---

## Multi-Node Batch Processing

```
You:   "Run this data processing job on every node in the cluster"
Agent:  Running on 5 nodes in parallel...
        node-01: completed (42s)
        node-02: completed (38s)
        node-03: completed (45s)
        node-04: completed (41s)
        node-05: failed — out of memory
        4/5 succeeded. node-05 has 16GB RAM, job needs 32GB. Skip or retry with lower batch size?
```

---

## GPU Cluster Monitoring

```
You:   "Give me a daily health report"
Agent:  Fleet: 8 nodes, 7 healthy, 1 degraded.
        GPUs: 32 total (24 A100, 8 H100), 18 allocated, 14 available.
        Alerts: node-06 GPU 3 memory errors (ECC count 47, was 12 yesterday).
        Workloads: 12 running, 3 pending (waiting for GPU).
        Recommendation: node-06 GPU 3 should be taken offline for diagnostics.

You:   "Do it"
Agent:  Draining node-06... migrating 2 workloads to node-04.
        Workloads migrated. node-06 cordoned. GPU 3 isolated.
```

---

## Cost Optimization with MOLT

The MOLT marketplace lets you buy and sell GPU compute peer-to-peer.

```
You:   "My training job only runs at night. Can I rent out GPUs during the day?"
Agent:  Your cluster has 24 GPUs. Daytime utilization averages 15%.
        Creating MOLT offer: 20 GPUs available 8am-6pm, $0.80/GPU-hour.
        Estimated daily revenue: ~$160 from marketplace demand.

You:   "Find me cheap H100s for a 4-hour batch job tonight"
Agent:  Found 3 MOLT offers for H100s:
        - provider-aws-7: 8x H100, $1.20/hr, 98% reputation
        - provider-lambda-3: 4x H100, $0.95/hr, 94% reputation
        - provider-coreweave-1: 8x H100, $1.50/hr, 99% reputation
        Recommend provider-aws-7. Total cost: $38.40 for 4 hours.
        Place bid?
```

---

## Canary Deployments

```
You:   "Update the inference server to the new model version, but do it safely"
Agent:  Using canary strategy. Deploying v2 alongside v1.
        Routing 10% of traffic to v2.
        Monitoring latency and error rates for 10 minutes...

        5min: v2 p99 latency 45ms (v1: 42ms), error rate 0.1% — looks good.
        10min: v2 stable. Promoting to 50%.
        20min: No regressions. Promoting to 100%. Removing v1.
        Canary complete. All traffic on v2.
```

---

## Incident Response

```
You:   "GPU utilization dropped to 0 on node-03, training is stuck"
Agent:  Investigating node-03...
        - Node healthy, network OK
        - GPU driver responding, but CUDA context hung on GPU 0
        - Workload fine-tune-7b stuck in CUDA_ERROR_LAUNCH_FAILED
        Root cause: OOM during backward pass (tried to allocate 78GB on 80GB card).
        Fix: restart workload with gradient checkpointing enabled (halves memory).
        Should I restart with --gradient-checkpointing?
```

---

## See Also

- [Architecture](architecture.md) — How the system works under the hood
- [Skills](skills.md) — All 20 agent skills that power these workflows
- [Configuration](configuration.md) — Gateway, node, and plugin config
