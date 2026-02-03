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

## Secrets Management

Secure storage and retrieval of sensitive data (API keys, credentials, certificates).

### Store a Secret
```bash
clawbernetes secret set <name> --value <value>
```
Store an encrypted secret. Value is encrypted at rest using cluster-level encryption.

```bash
clawbernetes secret set <name> --file <path>
```
Store secret from a file (useful for multi-line values, certificates).

**Flags:**
- `--namespace <ns>` - Target namespace (default: current context)
- `--description "<text>"` - Human-readable description
- `--expires <duration>` - Auto-expire after duration (e.g., "90d")

### Retrieve a Secret
```bash
clawbernetes secret get <name>
```
Get secret value. **Requires authorization** - access is logged.

**Flags:**
- `--output <format>` - Output format: `value`, `json`, `yaml`
- `--version <n>` - Get specific version (default: latest)

### List Secrets
```bash
clawbernetes secret list
```
List all secret names (values are NOT shown for security).

**Flags:**
- `--namespace <ns>` - Filter by namespace
- `--json` - Output as JSON with metadata

### Rotate a Secret
```bash
clawbernetes secret rotate <name>
```
Generate a new version of the secret. Old version remains accessible until TTL expires.

**Flags:**
- `--value <value>` - Set new value (prompted if not provided)
- `--keep-versions <n>` - Number of old versions to retain (default: 3)

### Delete a Secret
```bash
clawbernetes secret delete <name>
```
Remove secret. Requires confirmation unless `--force` is used.

**Flags:**
- `--force` - Skip confirmation
- `--all-versions` - Delete all versions (default: only latest)

### Certificate Management

#### Issue TLS Certificate
```bash
clawbernetes cert issue <name> --san <dns-name>
```
Issue a TLS certificate from the cluster CA.

**Flags:**
- `--san <name>` - Subject Alternative Names (repeatable)
- `--validity <duration>` - Certificate validity (default: "90d")
- `--key-size <bits>` - RSA key size (default: 2048)
- `--algorithm <alg>` - ecdsa, rsa (default: ecdsa)

#### List Certificates
```bash
clawbernetes cert list
```
List all certificates with expiry status.

**Flags:**
- `--expiring <duration>` - Show certs expiring within duration
- `--json` - Output as JSON

#### Rotate Certificate
```bash
clawbernetes cert rotate <name>
```
Rotate certificate before expiry. Old cert remains valid during transition.

**Flags:**
- `--force` - Rotate even if not near expiry
- `--revoke-old` - Revoke old certificate immediately

### Secrets Best Practices

When helping users with secrets:
1. **Never log or display secret values** unless explicitly requested
2. **Suggest rotation** for secrets older than 90 days
3. **Use file-based secrets** for certificates and multi-line values
4. **Check expiry dates** on certificates regularly
5. **Use namespaces** to isolate secrets between environments

## Smart Deployment

AI-assisted deployment with natural language intent parsing.

### Deploy with Intent
```bash
clawbernetes deploy "<intent>"
```
Natural language deployment. The AI parses your intent and generates the appropriate deployment strategy.

**Examples:**
```bash
# GPU workload with canary
clawbernetes deploy "deploy pytorch-train with 4 GPUs, canary 10% first"

# Update with auto-rollback
clawbernetes deploy "update model-server to v2.1, rollback if errors > 1%"

# Simple scaling
clawbernetes deploy "scale inference to 8 replicas"

# Blue-green deployment
clawbernetes deploy "blue-green deploy api-server v3.0, switch after health check"

# Resource adjustment
clawbernetes deploy "increase memory for data-processor to 64Gi"

# Scheduled deployment
clawbernetes deploy "deploy new-model at 2am with 5% canary, promote after 1 hour if healthy"
```

**Flags:**
- `--dry-run` - Show what would be deployed without executing
- `--confirm` - Require confirmation before applying
- `--watch` - Watch deployment progress after submission
- `--timeout <duration>` - Max deployment time (default: "30m")

### Deployment Status
```bash
clawbernetes deploy status <id>
```
Check deployment progress with real-time status.

**Output includes:**
- Current phase (pending, rolling, canary, promoting, complete, failed)
- Replica status (ready/total)
- Health check results
- Error messages if any
- Estimated time remaining

**Flags:**
- `--watch` / `-w` - Continuous status updates
- `--json` - Output as JSON

### Promote Canary
```bash
clawbernetes deploy promote <id>
```
Promote canary deployment to full rollout.

**Flags:**
- `--force` - Promote even if metrics show warnings
- `--percentage <n>` - Promote to specific percentage (incremental)

### Rollback
```bash
clawbernetes deploy rollback <id>
```
Rollback to previous version. Automatic if deployment fails health checks.

```bash
clawbernetes deploy rollback <id> --to <version>
```
Rollback to a specific version.

**Flags:**
- `--immediate` - Skip graceful drain (emergency rollback)
- `--reason "<text>"` - Record rollback reason

### Deployment History
```bash
clawbernetes deploy history <workload>
```
View deployment history for a workload.

**Output includes:**
- Deployment ID and timestamp
- Version deployed
- Strategy used (rolling, canary, blue-green)
- Duration and outcome
- Who initiated the deployment

**Flags:**
- `--limit <n>` - Number of entries (default: 10)
- `--json` - Output as JSON
- `--include-failed` - Include failed deployments

### Deployment Strategies

The AI recognizes these deployment patterns:

| Intent Keywords | Strategy | Behavior |
|-----------------|----------|----------|
| "canary X%" | Canary | Deploy to X% first, wait for validation |
| "blue-green" | Blue-Green | Full parallel deployment, instant switch |
| "rolling" | Rolling Update | Gradual replacement (default) |
| "immediate" | Recreate | Kill all, then create new |
| "scale to N" | HPA Adjustment | Adjust replica count |
| "rollback if" | Auto-Rollback | Set failure conditions |

### AI Deployment Assistance

When helping users deploy:

1. **Parse intent clearly** - Confirm understanding before deploying
2. **Suggest canary for risky changes** - New versions, major updates
3. **Recommend rollback conditions** - Error rate, latency thresholds
4. **Check resource availability** - GPUs, memory before deploying
5. **Verify secrets exist** - Ensure required secrets are in place
6. **Watch initial deployment** - Monitor first few minutes for issues

### Example Deployment Workflows

#### Safe Production Update
```bash
# User: "deploy model-server v2.5 to production safely"

# AI suggests:
clawbernetes deploy "canary 10% model-server v2.5, promote after 15m if error_rate < 0.5%"

# Watch progress
clawbernetes deploy status <id> --watch

# If healthy, promote (or it auto-promotes)
clawbernetes deploy promote <id>
```

#### Emergency Rollback
```bash
# User: "something's wrong, roll back model-server"

# Check recent deployments
clawbernetes deploy history model-server --limit 5

# Rollback to previous stable version
clawbernetes deploy rollback <id> --immediate --reason "elevated error rate"
```

#### GPU Training Deployment
```bash
# User: "deploy the new training job with 8 A100s"

clawbernetes deploy "deploy pytorch-trainer with 8 A100 GPUs, priority high, timeout 48h"

# Check status
clawbernetes deploy status <id>
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
