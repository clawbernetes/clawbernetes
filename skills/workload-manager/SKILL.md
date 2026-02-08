---
name: workload-manager
description: Container lifecycle management for GPU workloads on Clawbernetes nodes
---

# Workload Manager

You can run, monitor, and manage container workloads across Clawbernetes nodes.

## Commands

All workload commands are invoked on a specific node via `node.invoke <node-id> <command> <params>`.

### Run a Container

```
node.invoke <node-id> workload.run {
  "image": "nginx:latest",
  "name": "my-nginx",
  "gpus": 0,
  "command": ["nginx", "-g", "daemon off;"],
  "env": ["PORT=8080", "DEBUG=true"],
  "volumes": ["/host/data:/container/data"],
  "detach": true,
  "memory": "4g",
  "cpu": 2.0,
  "shmSize": "2g"
}
```

**Parameters:**
- `image` (required): Docker image to run
- `name` (optional): Container name
- `gpus` (optional): Number of GPUs to attach (0 = no GPU)
- `command` (optional): Command to run in the container
- `env` (optional): Environment variables as `KEY=VALUE` strings
- `volumes` (optional): Volume mounts as `host:container` strings
- `detach` (optional, default true): Run in background
- `memory` (optional): Memory limit (e.g., "8g", "512m")
- `cpu` (optional): CPU limit as fractional cores (e.g., 4.0)
- `shmSize` (optional): Shared memory size (e.g., "2g", important for PyTorch)

**Returns:** `containerId`, `workloadId`, `image`, `success`

All containers are labeled with `managed-by=clawbernetes` and a unique `workload-id` for tracking.

### Stop a Container

```
node.invoke <node-id> workload.stop {
  "containerId": "abc123",
  "force": false
}
```

Or by name:
```
node.invoke <node-id> workload.stop {
  "name": "my-nginx",
  "force": true
}
```

### Get Container Logs

```
node.invoke <node-id> workload.logs {
  "containerId": "abc123",
  "tail": 100
}
```

### List All Managed Workloads

```
node.invoke <node-id> workload.list
```

Returns all containers with the `managed-by=clawbernetes` label: container ID, image, state, GPU assignments, creation time, workload ID.

### Inspect a Container

```
node.invoke <node-id> workload.inspect {
  "containerId": "abc123"
}
```

Returns detailed container info: state, image, GPU IDs, exit code, labels, creation time.

### Get Container Resource Stats

```
node.invoke <node-id> workload.stats {
  "containerId": "abc123"
}
```

Returns live CPU %, memory usage, network I/O, block I/O.

### Execute a Command in a Container

```
node.invoke <node-id> container.exec {
  "containerId": "abc123",
  "command": ["python", "-c", "print('hello')"],
  "workdir": "/app"
}
```

Returns: exitCode, stdout, stderr, success.

## Common Workload Patterns

### GPU Training Job
```json
{
  "image": "pytorch/pytorch:2.1.0-cuda12.1-cudnn8-runtime",
  "gpus": 2,
  "memory": "32g",
  "shmSize": "8g",
  "env": ["CUDA_VISIBLE_DEVICES=0,1"],
  "volumes": ["/data/datasets:/data", "/data/checkpoints:/checkpoints"],
  "command": ["python", "train.py", "--epochs", "100"]
}
```

### Web Service
```json
{
  "image": "nginx:latest",
  "name": "web-frontend",
  "gpus": 0,
  "memory": "512m",
  "cpu": 1.0
}
```

### AI Inference Server
```json
{
  "image": "vllm/vllm-openai:latest",
  "gpus": 1,
  "memory": "16g",
  "shmSize": "4g",
  "env": ["MODEL=meta-llama/Llama-2-7b-hf"],
  "name": "llm-server"
}
```

## Monitoring Workloads

To check on a running workload:
1. `workload.list` to see all managed containers on the node
2. `workload.stats` for resource usage
3. `workload.logs` for output
4. `gpu.metrics` for GPU-specific utilization

## Deployment Strategies

For production workloads, use deployment commands instead of raw `workload.run`. Deployments track desired state, support rollouts, and enable rollback.

### Create a Deployment

```
node.invoke <node-id> deploy.create {
  "name": "api-server",
  "image": "myapp:v2.0",
  "replicas": 3,
  "strategy": "canary:10",
  "gpus": 1,
  "memory": "8g"
}
```

**Strategy options:** `immediate`, `canary:N` (N% canary), `blue-green`, `rolling:N` (batch size N)

### Check Deployment Status

```
node.invoke <node-id> deploy.status { "deploymentId": "<id>" }
```

Returns: state, healthy/total replicas, health ratio, strategy, timestamps.

### Promote or Rollback

```
node.invoke <node-id> deploy.promote { "deploymentId": "<id>" }
node.invoke <node-id> deploy.rollback { "deploymentId": "<id>", "reason": "high error rate" }
```

### Deployment History

```
node.invoke <node-id> deploy.history
```

Lists all deployments with their state, image, replica counts, and timestamps.

### Update a Deployment

```
node.invoke <node-id> deploy.update {
  "deploymentId": "<id>",
  "image": "myapp:v3.0",
  "replicas": 5
}
```

### Delete a Deployment

```
node.invoke <node-id> deploy.delete { "deploymentId": "<id>" }
```

## Deployment Workflow

1. `deploy.create` with desired image, replicas, and strategy
2. `deploy.status` to monitor rollout progress
3. Check metrics and logs for the new version
4. `deploy.promote` if healthy, `deploy.rollback` if not
5. `deploy.history` to review all past deployments

## Troubleshooting

- If a container fails to start, check `workload.logs` for error output
- Use `container.exec` to run diagnostic commands inside a running container
- Use `workload.inspect` to check exit codes and state details
- If a container is stuck, use `workload.stop` with `force: true`
- If a deployment is unhealthy, check `deploy.status` then `deploy.rollback`
