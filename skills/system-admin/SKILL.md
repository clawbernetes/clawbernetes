# System Administration

You can run system commands, gather diagnostics, and monitor node health across the Clawbernetes cluster.

## Commands

All commands are invoked on a specific node via `node.invoke <node-id> <command> <params>`.

### Get System Info

```
node.invoke <node-id> system.info
```

Returns: hostname, OS, kernel version, CPU count, total/used/available memory, GPU count, GPU VRAM, capabilities, supported commands.

### Run a Command

```
node.invoke <node-id> system.run {
  "command": ["ls", "-la", "/data"],
  "cwd": "/home/user",
  "env": ["DEBUG=1"]
}
```

**Parameters:**
- `command` (required): Command as array of strings (program + args)
- `cwd` (optional): Working directory
- `env` (optional): Environment variables as `KEY=VALUE` strings

**Returns:** exitCode, stdout, stderr, success.

### Check Node Health

```
node.invoke <node-id> node.health
```

Returns: status, CPU count, load average (1/5/15 min), memory usage (total/used/available/percent), disk usage per mount, GPU count, system uptime.

### Get Node Capabilities

```
node.invoke <node-id> node.capabilities
```

Returns: hostname, capabilities list, supported commands, container runtime, CPU/memory/GPU details, node labels.

## Cluster-Wide Operations

To get a full picture of the cluster:

1. `node.list` to get all connected nodes
2. For each node, run `node.health` to check status
3. For each node, run `system.info` for detailed specs
4. Aggregate the results for cluster-wide view

## Common Administrative Tasks

### Check disk space across cluster
For each node: `system.run {"command": ["df", "-h"]}`

### Check running processes
For each node: `system.run {"command": ["ps", "aux", "--sort=-%mem"]}`

### Check network connectivity
For each node: `system.run {"command": ["ss", "-tlnp"]}`

### Check Docker status
For each node: `system.run {"command": ["docker", "info"]}`

### Check GPU driver version
For each node: `system.run {"command": ["nvidia-smi", "--query-gpu=driver_version", "--format=csv,noheader"]}`

### Update a package
For each node: `system.run {"command": ["apt", "update"]}`
Then: `system.run {"command": ["apt", "install", "-y", "package-name"]}`

## Health Monitoring

A node is considered healthy when:
- Load average (1min) is below CPU count
- Memory usage is below 90%
- Disk usage is below 90% on all mounts
- GPU temperatures are below 85C (check via `gpu.metrics`)

Warning thresholds:
- Load average > 0.7 * CPU count
- Memory usage > 80%
- Disk usage > 80%
- GPU temperature > 75C
