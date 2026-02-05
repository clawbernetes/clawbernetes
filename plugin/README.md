# Clawbernetes OpenClaw Plugin

AI-native GPU orchestration tools for OpenClaw.

## Installation

### 1. Build the Bridge Binary

```bash
cd /path/to/clawbernetes
cargo build -p claw-bridge --release
```

The binary will be at `target/release/claw-bridge`.

### 2. Install the Plugin

```bash
# Option A: Install globally
npm install -g @clawbernetes/openclaw-plugin

# Option B: Link locally
cd /path/to/clawbernetes/plugin
npm link
```

### 3. Configure OpenClaw

Add to your `~/.openclaw/openclaw.json`:

```json
{
  "plugins": {
    "clawbernetes": {
      "enabled": true,
      "bridgePath": "/path/to/clawbernetes/target/release/claw-bridge"
    }
  },
  "tools": {
    "alsoAllow": ["clawbernetes"]
  }
}
```

### 4. Restart OpenClaw

```bash
openclaw gateway restart
```

## Available Tools

### Cluster Management
- `cluster_status` - Get cluster health and resource summary
- `node_list` - List nodes with optional filtering
- `node_get` - Get detailed node information
- `node_drain` - Drain a node (migrate workloads off)
- `node_cordon` - Prevent new scheduling on a node
- `node_uncordon` - Allow scheduling on a node

### Workload Management
- `workload_submit` - Submit a GPU workload
- `workload_get` - Get workload details
- `workload_list` - List workloads with filtering
- `workload_stop` - Stop a running workload
- `workload_scale` - Scale workload replicas
- `workload_logs` - Get workload logs

### Observability
- `metrics_query` - Query GPU/workload metrics
- `logs_search` - Search logs across the cluster
- `alert_create` - Create an alert rule
- `alert_list` - List active alerts
- `alert_silence` - Silence an alert

### MOLT Marketplace
- `molt_offers` - List available GPU capacity offers
- `molt_offer_create` - Offer your GPUs for rent
- `molt_bid` - Bid on GPU capacity
- `molt_spot_prices` - Get current spot prices

## Skills

The plugin includes skills for common operations:

- **canary-release** - Deploy with gradual rollout
- **blue-green-deploy** - Zero-downtime deployments
- **gpu-diagnose** - Troubleshoot GPU issues
- **training-job** - Submit ML training jobs
- **spot-migration** - Optimize costs with spot
- **auto-heal** - Automatic failure recovery
- **cost-optimize** - Reduce GPU spend
- **incident-response** - Handle incidents

## Usage Examples

```
You: "What's the cluster status?"
Agent: cluster_status() → shows nodes, GPUs, workloads

You: "Submit a training job with 4 GPUs"
Agent: workload_submit({name: "training", image: "...", gpus: 4})

You: "Check GPU utilization on node-1"
Agent: metrics_query({name: "gpu_utilization", labels: {node_id: "node-1"}})

You: "Find me cheap GPU capacity for my batch job"
Agent: molt_offers({maxPricePerHour: 1.0}) → finds offers
       molt_bid({offerId: "...", pricePerHour: 0.90, durationHours: 2})
```

## Architecture

```
OpenClaw Agent
      │
      ▼
[@clawbernetes/openclaw-plugin]  (TypeScript)
      │ spawn + stdin/stdout
      ▼
[claw-bridge]                     (Rust binary)
      │
      ▼
[claw-* crates]                   (Rust libraries)
```

## Development

```bash
# Build everything
cd /path/to/clawbernetes
cargo build -p claw-bridge
cd plugin && npm run build

# Test the bridge
echo '{"id":1,"method":"cluster_status","params":{}}' | cargo run -p claw-bridge

# Watch mode for plugin
cd plugin && npm run dev
```

## License

MIT
