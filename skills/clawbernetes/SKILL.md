# Clawbernetes Fleet Management Skill

## Description
Manage Clawbernetes GPU compute clusters through the gateway. This skill enables AI-assisted monitoring, workload submission, and MOLT network participation for distributed GPU computing.

## When to Use
- User asks about GPU nodes, cluster status, workloads
- User wants to submit or check jobs
- User mentions "clawbernetes", "GPU cluster", "compute fleet"
- User asks about MOLT network, GPU sharing, or compute earnings
- User wants to monitor running jobs or check node health

## Available Commands

### Cluster Overview

#### Check Cluster Status
```bash
clawbernetes status
```
Overview of all registered nodes, total GPUs, and cluster health.

#### List Nodes  
```bash
clawbernetes node list
```
List all nodes with GPU info (model, memory, utilization).

**Useful flags:**
- `--available` - Show only nodes with free GPUs
- `--gpu-type <type>` - Filter by GPU model (e.g., "A100", "H100")
- `--json` - Output as JSON for parsing

#### Node Details
```bash
clawbernetes node info <node-id>
```
Detailed node information including:
- Hardware specs (CPU, RAM, GPUs)
- Current utilization
- Running workloads
- Network connectivity
- MOLT participation status

### Workload Management

#### Submit Workload
```bash
clawbernetes run --image <image> --gpus <n> [options]
```
Submit a GPU workload to the cluster.

**Required:**
- `--image <image>` - Container image to run
- `--gpus <n>` - Number of GPUs needed

**Optional:**
- `--command "<cmd>"` - Command to execute
- `--name <name>` - Workload name for tracking
- `--memory <size>` - Memory requirement (e.g., "32Gi")
- `--priority <level>` - low, normal, high
- `--timeout <duration>` - Max runtime (e.g., "24h")
- `--env KEY=VALUE` - Environment variables (repeatable)
- `--mount <src>:<dst>` - Volume mounts

#### Check Workload Status
```bash
clawbernetes workload status <id>
```
Check status of a specific workload.

#### List Workloads
```bash
clawbernetes workload list
```
List all workloads (running, pending, completed).

**Useful flags:**
- `--running` - Show only running workloads
- `--mine` - Show only your workloads
- `--since <time>` - Filter by start time

#### View Logs
```bash
clawbernetes workload logs <id>
```
Stream or fetch workload logs.

**Flags:**
- `--follow` / `-f` - Stream logs in real-time
- `--tail <n>` - Show last n lines

#### Cancel Workload
```bash
clawbernetes workload cancel <id>
```
Cancel a running or pending workload.

### MOLT Network

MOLT (Managed Open Lending of Tensors) enables GPU sharing and earning.

#### MOLT Status
```bash
clawbernetes molt status
```
Current MOLT network participation status and statistics.

#### Join MOLT Network
```bash
clawbernetes molt join --mode <mode>
```
Join the MOLT network to share GPUs.

**Modes:**
- `conservative` - Share only when fully idle
- `moderate` - Share unused GPU capacity
- `aggressive` - Maximize sharing (may preempt)

#### Leave MOLT Network
```bash
clawbernetes molt leave
```
Stop participating in MOLT network.

#### View Earnings
```bash
clawbernetes molt earnings
```
View MOLT participation earnings and statistics.

**Flags:**
- `--period <range>` - Time period (e.g., "7d", "30d", "month")

## Gateway Connection

The gateway server runs on `ws://localhost:9000` by default.

**Configuration:**
```bash
# Set gateway URL
export CLAWBERNETES_GATEWAY_URL="ws://your-gateway:9000"

# Check connection
clawbernetes status
```

**Troubleshooting:**
- If connection fails, verify gateway is running: `clawbernetes gateway ping`
- Check firewall rules for WebSocket connections
- Verify authentication: `clawbernetes auth status`

## Example Workflows

### Deploy a Training Job

1. **Check available GPUs:**
   ```bash
   clawbernetes node list --available
   ```

2. **Submit the job:**
   ```bash
   clawbernetes run \
     --image pytorch/pytorch:2.1-cuda12.1-runtime \
     --gpus 4 \
     --memory 64Gi \
     --name "gpt-finetune-exp1" \
     --command "python train.py --epochs 100 --batch-size 32" \
     --env WANDB_API_KEY=$WANDB_KEY \
     --timeout 48h
   ```

3. **Monitor progress:**
   ```bash
   clawbernetes workload logs <id> --follow
   ```

4. **Check status:**
   ```bash
   clawbernetes workload status <id>
   ```

### Add a Node to MOLT

1. **Check current MOLT status:**
   ```bash
   clawbernetes molt status
   ```

2. **Join the network:**
   ```bash
   clawbernetes molt join --mode moderate
   ```

3. **Verify participation:**
   ```bash
   clawbernetes molt status
   ```

4. **Check earnings later:**
   ```bash
   clawbernetes molt earnings --period 7d
   ```

### Quick Cluster Health Check

```bash
# One-liner for cluster overview
clawbernetes status && clawbernetes node list --json | jq '.[] | select(.status != "healthy")'
```

### Submit from YAML Template

```bash
# Use a workload template
clawbernetes run --file workload.yaml

# Override specific values
clawbernetes run --file workload.yaml --gpus 8 --name "scaled-run"
```

## Tips for AI Assistance

When helping users with Clawbernetes:

1. **Always check cluster status first** before suggesting workload submission
2. **Verify GPU availability** matches the requested count
3. **Suggest appropriate GPU types** based on the workload (training vs inference)
4. **Include timeout flags** to prevent runaway jobs
5. **Use workload names** for easier tracking and discussion
6. **Check MOLT status** if user mentions cost optimization or GPU sharing

## Observability Commands

### Quick Health Check
```bash
clawbernetes health
```
Overall cluster health with AI diagnosis. Returns:
- Cluster-wide health score (0-100)
- Node status summary (healthy/degraded/critical counts)
- Resource utilization overview
- Active alerts and warnings

### Node Diagnostics
```bash
clawbernetes diagnose node <node-id>
```
Deep analysis of a specific node:
- **GPU thermal status** - Temperature, throttling state, fan speed
- **Memory pressure** - RAM usage, swap activity, OOM risk
- **Recent errors** - Last 24h of warnings/errors from logs
- **Performance trends** - Utilization patterns, anomaly detection

**Flags:**
- `--verbose` - Include raw metrics data
- `--json` - Output as JSON for parsing
- `--history <duration>` - Analysis window (default: "1h")

### Workload Diagnostics
```bash
clawbernetes diagnose workload <workload-id>
```
Analyze workload health:
- **Resource utilization** - GPU%, memory, CPU, I/O
- **Error patterns** - Crash loops, restarts, OOM events
- **Performance metrics** - Throughput, latency, efficiency
- **Bottleneck analysis** - Data pipeline, compute, memory bound

**Flags:**
- `--compare <baseline-id>` - Compare against baseline run
- `--recommendations` - Include AI-generated suggestions

### View Metrics
```bash
clawbernetes metrics <name> [--last 1h]
```
Query specific metrics:
- `gpu.utilization` - GPU compute usage
- `gpu.memory` - VRAM usage
- `gpu.temperature` - Thermal readings
- `cpu.usage` - CPU utilization
- `memory.usage` - RAM consumption
- `network.throughput` - Network I/O
- `disk.iops` - Storage performance

**Flags:**
- `--last <duration>` - Time range (e.g., "1h", "24h", "7d")
- `--node <id>` - Filter by node
- `--workload <id>` - Filter by workload
- `--aggregate <fn>` - avg, max, min, p95, p99

### View Logs
```bash
clawbernetes logs <workload-id> [--level error] [--tail]
```
View workload logs with filtering:

**Flags:**
- `--level <level>` - Filter by level: debug, info, warn, error, fatal
- `--tail [n]` - Show last n lines (default: 100)
- `--follow` / `-f` - Stream logs in real-time
- `--since <time>` - Logs since timestamp or duration (e.g., "1h ago")
- `--search <pattern>` - Grep-style pattern matching
- `--context <n>` - Lines of context around matches

### AI Insights

When user asks "why is X slow?" or "what's wrong?":

1. **Run `clawbernetes health`** for cluster overview
2. **If specific workload mentioned**, run `clawbernetes diagnose workload <id>`
3. **If specific node mentioned**, run `clawbernetes diagnose node <id>`
4. **Summarize insights** in plain language
5. **Suggest remediation steps** with concrete commands

**Key diagnostic questions to consider:**
- Is the cluster under resource pressure?
- Are there thermal/throttling issues?
- Is the workload hitting memory limits?
- Are there network or storage bottlenecks?
- Has performance degraded over time?

## Example Diagnostic Workflows

### "Why is my training slow?"

1. **Get workload diagnostics:**
   ```bash
   clawbernetes diagnose workload <id> --recommendations
   ```

2. **Check GPU utilization:**
   - If **low (<70%)** → Likely data pipeline bottleneck
   - If **high but slow** → May be memory-bound or batch size issue

3. **Check GPU temperature:**
   ```bash
   clawbernetes metrics gpu.temperature --workload <id> --last 1h
   ```
   - If **throttling (>80°C)** → Suggest migration to cooler node

4. **Check memory pressure:**
   ```bash
   clawbernetes metrics memory.usage --workload <id> --last 1h
   ```
   - If **near limit/swapping** → Suggest larger instance or reduce batch size

5. **Check for errors:**
   ```bash
   clawbernetes logs <id> --level error --since "1h ago"
   ```

### "Is the cluster healthy?"

1. **Get health overview:**
   ```bash
   clawbernetes health
   ```

2. **Report findings:**
   - Any degraded/critical nodes
   - Resource utilization summary
   - Active alerts

3. **Deep dive on problem nodes:**
   ```bash
   clawbernetes diagnose node <problem-node-id>
   ```

4. **Flag concerning trends:**
   ```bash
   clawbernetes metrics gpu.temperature --aggregate max --last 24h
   ```

### "What's wrong with node X?"

1. **Full node diagnosis:**
   ```bash
   clawbernetes diagnose node <id> --verbose
   ```

2. **Check recent events:**
   ```bash
   clawbernetes node events <id> --since "24h ago"
   ```

3. **Compare to healthy baseline:**
   ```bash
   clawbernetes metrics gpu.utilization --node <id> --last 7d
   ```

### "Why did my job fail?"

1. **Check workload status:**
   ```bash
   clawbernetes workload status <id>
   ```

2. **Get error logs:**
   ```bash
   clawbernetes logs <id> --level error --context 10
   ```

3. **Check for OOM:**
   ```bash
   clawbernetes metrics memory.usage --workload <id>
   ```

4. **Review exit diagnostics:**
   ```bash
   clawbernetes diagnose workload <id>
   ```

## Error Handling

Common errors and solutions:

| Error | Cause | Solution |
|-------|-------|----------|
| `ECONNREFUSED` | Gateway not running | Start gateway or check URL |
| `NO_GPUS_AVAILABLE` | Cluster at capacity | Wait or try `--priority high` |
| `AUTH_FAILED` | Token expired | Run `clawbernetes auth login` |
| `IMAGE_NOT_FOUND` | Invalid container image | Verify image name and registry |
| `OOM_KILLED` | Insufficient memory | Increase `--memory` allocation |
| `GPU_THERMAL_THROTTLE` | GPU overheating | Migrate workload or reduce load |
| `NODE_UNREACHABLE` | Network/node failure | Check node status, failover if needed |
| `WORKLOAD_TIMEOUT` | Exceeded time limit | Increase `--timeout` or optimize code |
