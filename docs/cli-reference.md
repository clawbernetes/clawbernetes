# CLI Reference

Complete reference for the `clawbernetes` command-line interface.

## Synopsis

```
clawbernetes [OPTIONS] <COMMAND>
```

## Global Options

| Option | Short | Environment Variable | Default | Description |
|--------|-------|---------------------|---------|-------------|
| `--gateway <URL>` | `-g` | `CLAWBERNETES_GATEWAY` | `ws://localhost:8080` | Gateway WebSocket URL |
| `--format <FORMAT>` | `-f` | — | `table` | Output format: `table` or `json` |
| `--help` | `-h` | — | — | Print help information |
| `--version` | `-V` | — | — | Print version |

## Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `CLAWBERNETES_GATEWAY` | Default gateway URL | `ws://gateway.example.com:8080` |
| `CLAWNODE_GATEWAY` | Gateway URL for clawnode | `ws://gateway.example.com:8080` |
| `CLAWNODE_NAME` | Node name for clawnode | `gpu-node-1` |
| `CLAWNODE_CONFIG` | Config file path for clawnode | `/etc/clawbernetes/clawnode.toml` |
| `TS_AUTHKEY` | Tailscale auth key (if using Tailscale) | `tskey-auth-...` |
| `RUST_LOG` | Log level | `info`, `debug`, `trace` |

---

## Commands

### `status`

Show cluster status overview.

```bash
clawbernetes status
```

**Output (table):**
```
CLUSTER STATUS
═══════════════════════════════════════
Gateway:       ws://localhost:8080
Nodes:         3 healthy, 0 unhealthy  
Total GPUs:    16 (12 available)
Workloads:     4 running, 2 pending
```

**Output (JSON):**
```bash
clawbernetes -f json status
```
```json
{
  "gateway": "ws://localhost:8080",
  "nodes": { "healthy": 3, "unhealthy": 0 },
  "gpus": { "total": 16, "available": 12 },
  "workloads": { "running": 4, "pending": 2 }
}
```

---

### `node`

Node management commands.

#### `node list`

List all nodes in the cluster.

```bash
clawbernetes node list
```

**Output:**
```
┌─────────────┬──────────┬─────────┬───────────────┬──────────────┐
│ Name        │ GPUs     │ Status  │ Utilization   │ Last Seen    │
├─────────────┼──────────┼─────────┼───────────────┼──────────────┤
│ gpu-node-1  │ 4× A100  │ healthy │ 75%           │ 2s ago       │
│ gpu-node-2  │ 8× H100  │ healthy │ 50%           │ 1s ago       │
│ gpu-node-3  │ 4× RTX   │ drain   │ 0%            │ 3s ago       │
└─────────────┴──────────┴─────────┴───────────────┴──────────────┘
```

#### `node info <ID>`

Show detailed information about a node.

```bash
clawbernetes node info gpu-node-1
```

**Output:**
```
NODE: gpu-node-1
═══════════════════════════════════════
Status:       healthy
Connected:    2024-02-04 10:30:00 UTC
OS:           Linux 6.5.0 (Ubuntu 22.04)
Arch:         x86_64

GPUs (4):
┌─────┬────────────┬────────────┬─────────┬──────────┐
│ ID  │ Model      │ VRAM       │ Util    │ Temp     │
├─────┼────────────┼────────────┼─────────┼──────────┤
│ 0   │ A100-SXM4  │ 71/80 GB   │ 82%     │ 65°C     │
│ 1   │ A100-SXM4  │ 68/80 GB   │ 75%     │ 67°C     │
│ 2   │ A100-SXM4  │ 45/80 GB   │ 50%     │ 58°C     │
│ 3   │ A100-SXM4  │ 0/80 GB    │ 0%      │ 45°C     │
└─────┴────────────┴────────────┴─────────┴──────────┘

Workloads: 2 running
Network: WireGuard mesh (10.100.0.5)
MOLT: enabled (moderate autonomy)
```

#### `node drain <ID>`

Mark a node for draining. Prevents new workloads from being scheduled.

```bash
# Graceful drain (waits for workloads to complete)
clawbernetes node drain gpu-node-1

# Force drain (stops workloads immediately)  
clawbernetes node drain --force gpu-node-1
```

| Option | Short | Description |
|--------|-------|-------------|
| `--force` | `-f` | Force drain even with running workloads |

#### `node undrain <ID>`

Remove drain status, allowing the node to accept workloads again.

```bash
clawbernetes node undrain gpu-node-1
```

---

### `run`

Run a workload on the cluster.

```bash
clawbernetes run [OPTIONS] <IMAGE> [-- <COMMAND>...]
```

**Arguments:**

| Argument | Required | Description |
|----------|----------|-------------|
| `<IMAGE>` | Yes | Container image to run |
| `<COMMAND>...` | No | Command to execute (after `--`) |

**Options:**

| Option | Short | Description | Example |
|--------|-------|-------------|---------|
| `--gpus <INDICES>` | `-g` | GPU indices (comma-separated) | `-g 0,1,2` |
| `--env <KEY=VALUE>` | `-e` | Environment variables | `-e MODEL=gpt2` |
| `--memory <MiB>` | `-m` | Memory limit in MiB | `-m 8192` |
| `--detach` | `-d` | Run in background | `-d` |

**Examples:**

```bash
# Simple GPU test
clawbernetes run --gpus 0 nvidia/cuda:12.0-runtime -- nvidia-smi

# PyTorch training with multiple GPUs
clawbernetes run \
  --gpus 0,1,2,3 \
  --memory 80000 \
  -e WORLD_SIZE=4 \
  -d \
  pytorch/pytorch:latest \
  -- python -m torch.distributed.launch train.py

# LLM inference
clawbernetes run \
  -g 0,1 \
  -e MODEL=meta-llama/Llama-2-7b-chat-hf \
  -e MAX_TOKENS=2048 \
  vllm/vllm:latest
```

---

### `molt`

MOLT P2P marketplace commands.

#### `molt status`

Show MOLT participation status.

```bash
clawbernetes molt status
```

**Output:**
```
MOLT STATUS
═══════════════════════════════════════
Participation:   enabled
Autonomy:        moderate
Wallet:          7x8K...3mNp
Balance:         1,234.56 MOLT

Provider Stats:
  Jobs completed: 142
  Total earned:   456.78 MOLT
  Avg rating:     4.8/5.0

Active Jobs: 2
  • job-abc123: 2× H100, 3h remaining
  • job-def456: 4× A100, 45m remaining
```

#### `molt join`

Join the MOLT network as a provider.

```bash
clawbernetes molt join [OPTIONS]
```

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--autonomy <LEVEL>` | `-a` | `conservative` | Autonomy level: `conservative`, `moderate`, `aggressive` |
| `--max-spend <AMOUNT>` | — | none | Maximum spend per job (MOLT tokens) |

**Autonomy Levels:**

| Level | Description |
|-------|-------------|
| `conservative` | Only low-risk, pre-approved jobs. Manual approval for most. |
| `moderate` | Accept most jobs within budget and policy constraints. |
| `aggressive` | Maximum automation. Any job within capability. |

**Examples:**

```bash
# Join with conservative autonomy
clawbernetes molt join

# Join with moderate autonomy
clawbernetes molt join --autonomy moderate

# Join with spending limit
clawbernetes molt join --autonomy moderate --max-spend 100.0
```

#### `molt leave`

Leave the MOLT network.

```bash
clawbernetes molt leave
```

#### `molt earnings`

Show earnings summary.

```bash
clawbernetes molt earnings [OPTIONS]
```

| Option | Short | Description |
|--------|-------|-------------|
| `--detailed` | `-d` | Show per-job breakdown |

**Output:**
```bash
clawbernetes molt earnings --detailed
```
```
MOLT EARNINGS
═══════════════════════════════════════
Period:          Last 30 days
Total Earned:    456.78 MOLT
Jobs Completed:  142
Avg per Job:     3.22 MOLT

BY JOB:
┌──────────────┬─────────┬──────────┬─────────────┬──────────┐
│ Job ID       │ GPUs    │ Duration │ Earned      │ Rating   │
├──────────────┼─────────┼──────────┼─────────────┼──────────┤
│ job-abc123   │ 2× H100 │ 4h 23m   │ 12.50 MOLT  │ 5.0      │
│ job-def456   │ 4× A100 │ 8h 15m   │ 28.00 MOLT  │ 4.5      │
│ ...          │         │          │             │          │
└──────────────┴─────────┴──────────┴─────────────┴──────────┘
```

---

### `autoscale`

Autoscaling management commands.

#### `autoscale status`

Show autoscaling status for all pools.

```bash
clawbernetes autoscale status
```

**Output:**
```
AUTOSCALING STATUS
═══════════════════════════════════════
Enabled:         yes
Last Evaluation: 2m ago

POOLS:
┌──────────────┬─────────┬───────────┬─────────────┬──────────────┐
│ Pool         │ Nodes   │ Policy    │ Target      │ Status       │
├──────────────┼─────────┼───────────┼─────────────┼──────────────┤
│ gpu-pool-1   │ 5/2-20  │ util      │ 70%         │ scaling up   │
│ gpu-pool-2   │ 3/1-10  │ queue     │ 5 jobs/node │ stable       │
└──────────────┴─────────┴───────────┴─────────────┴──────────────┘
```

#### `autoscale pools`

List all node pools with scaling configuration.

```bash
clawbernetes autoscale pools
```

#### `autoscale pool <ID>`

Show detailed info for a specific pool.

```bash
clawbernetes autoscale pool gpu-pool-1
```

#### `autoscale set-policy`

Set or update a scaling policy for a pool.

```bash
clawbernetes autoscale set-policy <POOL_ID> [OPTIONS]
```

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--policy-type <TYPE>` | `-t` | required | Policy type: `utilization`, `queue-depth`, `schedule` |
| `--min-nodes <N>` | — | `1` | Minimum number of nodes |
| `--max-nodes <N>` | — | `10` | Maximum number of nodes |
| `--target-utilization <PCT>` | — | — | Target GPU utilization % |
| `--tolerance <PCT>` | — | `10` | Tolerance % around target |
| `--target-jobs-per-node <N>` | — | — | Target jobs per node |
| `--scale-up-threshold <N>` | — | — | Queue depth to trigger scale up |
| `--scale-down-threshold <N>` | — | — | Queue depth to trigger scale down |
| `--scale-up-cooldown <SECS>` | — | `300` | Seconds between scale-up events |
| `--scale-down-cooldown <SECS>` | — | `600` | Seconds between scale-down events |

**Examples:**

```bash
# Utilization-based scaling
clawbernetes autoscale set-policy gpu-pool-1 \
  -t utilization \
  --min-nodes 2 \
  --max-nodes 20 \
  --target-utilization 70 \
  --tolerance 10

# Queue-depth scaling
clawbernetes autoscale set-policy gpu-pool-1 \
  -t queue-depth \
  --min-nodes 1 \
  --max-nodes 10 \
  --target-jobs-per-node 5 \
  --scale-up-threshold 20 \
  --scale-down-threshold 2
```

#### `autoscale enable`

Enable autoscaling globally.

```bash
clawbernetes autoscale enable
```

#### `autoscale disable`

Disable autoscaling globally.

```bash
clawbernetes autoscale disable
```

#### `autoscale evaluate`

Trigger an immediate scaling evaluation.

```bash
clawbernetes autoscale evaluate
```

---

## Output Formats

### Table (Default)

Human-readable format with box-drawing characters:

```bash
clawbernetes node list
```

### JSON

Machine-readable format for scripting:

```bash
clawbernetes -f json node list
```

```json
{
  "nodes": [
    {
      "id": "gpu-node-1",
      "gpus": 4,
      "gpu_model": "A100-SXM4",
      "status": "healthy",
      "utilization": 0.75
    }
  ]
}
```

**Scripting example:**

```bash
# Get node names using jq
clawbernetes -f json node list | jq -r '.nodes[].id'

# Get total GPU count
clawbernetes -f json status | jq '.gpus.total'

# Check if cluster is healthy
if clawbernetes -f json status | jq -e '.nodes.unhealthy == 0' > /dev/null; then
  echo "Cluster healthy"
fi
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | General error |
| `2` | Connection failed (gateway unreachable) |
| `3` | Authentication failed |
| `4` | Resource not found |
| `5` | Invalid arguments |

---

## Configuration Files

### clawnode.toml

Full configuration reference:

```toml
# Node identity and connection
[node]
name = "gpu-node-1"                    # Node name (default: hostname)
gateway = "ws://localhost:8080"        # Gateway URL
reconnect_interval_secs = 5            # Reconnection interval
max_reconnect_attempts = 10            # Max reconnection attempts

# GPU configuration
[gpu]
auto_detect = true                     # Auto-detect GPUs
# gpus = ["0", "1"]                    # Or specify manually
memory_alert_threshold = 90            # Alert at this % VRAM usage

# Metrics reporting
[metrics]
interval_secs = 10                     # Reporting interval
detailed_gpu_metrics = true            # Include per-GPU details

# MOLT marketplace participation
[molt]
enabled = false                        # Enable MOLT
min_price = 1.0                        # Min price per GPU-hour
max_jobs = 2                           # Max concurrent jobs
wallet_path = "~/.config/clawbernetes/wallet.json"

# Network configuration
[network]
provider = "wireguard"                 # "wireguard" or "tailscale"

[network.wireguard]
listen_port = 51820
# private_key_path = "/etc/clawbernetes/wg-private.key"

[network.tailscale]
auth_key_env = "TS_AUTHKEY"
hostname_prefix = "clawnode"
tags = ["tag:clawbernetes"]

# TLS configuration (for secure gateway connection)
[tls]
# ca_cert = "/etc/clawbernetes/ca.crt"
# client_cert = "/etc/clawbernetes/node.crt"
# client_key = "/etc/clawbernetes/node.key"

# Logging
[logging]
level = "info"                         # trace, debug, info, warn, error
format = "pretty"                      # "pretty" or "json"
```

---

## See Also

- [User Guide](user-guide.md) — Getting started guide
- [Architecture](architecture.md) — System design
- [MOLT Network](molt-network.md) — P2P marketplace guide
- [Security Guide](security.md) — Auth and security setup
