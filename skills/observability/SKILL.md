---
name: observability
description: Metrics, events, and alerts for Clawbernetes cluster observability
---

# Observability

You can query metrics, emit and query events, and manage alert rules on Clawbernetes nodes.

## Metrics Commands

Metrics are collected automatically every 30 seconds (CPU, memory, GPU utilization, temperature, power). You can also query custom metrics pushed by applications.

### Query Metrics

```
node.invoke <node-id> metrics.query {
  "name": "cpu:usage_percent",
  "rangeMinutes": 60,
  "aggregation": "avg"
}
```

**Parameters:**
- `name` (required): Metric name
- `rangeMinutes` (optional, default 60): Time window to query
- `aggregation` (optional): `sum`, `avg`, `min`, `max`, `last`, `count`

**Returns:** Array of `{timestamp, value, labels}` points. With aggregation, returns a single point.

### List Available Metrics

```
node.invoke <node-id> metrics.list
```

Returns all metric names with their point counts.

### Metrics Snapshot

```
node.invoke <node-id> metrics.snapshot
```

Returns the latest value of every metric — a quick health overview.

### Built-in Metrics

| Metric | Description |
|--------|-------------|
| `cpu:usage_percent` | CPU usage (load / cores * 100) |
| `cpu:load_1m` | 1-minute load average |
| `cpu:load_5m` | 5-minute load average |
| `cpu:load_15m` | 15-minute load average |
| `memory:usage_percent` | Memory usage percentage |
| `memory:used_mb` | Used memory in MB |
| `memory:available_mb` | Available memory in MB |
| `gpu:utilization_percent` | GPU compute utilization (labeled by gpu index) |
| `gpu:memory_used_mb` | GPU memory used (labeled by gpu index) |
| `gpu:temperature_c` | GPU temperature (labeled by gpu index) |
| `gpu:power_draw_w` | GPU power draw in watts (labeled by gpu index) |

## Event Commands

Events are structured log entries with source, severity, and message.

### Emit an Event

```
node.invoke <node-id> events.emit {
  "source": "deploy-controller",
  "severity": "info",
  "message": "Deployment api-v2 promoted to full rollout"
}
```

**Severity levels:** `info`, `warning`, `error`

### Query Events

```
node.invoke <node-id> events.query {
  "severity": "error",
  "source": "gpu-monitor",
  "rangeMinutes": 30,
  "limit": 50
}
```

All parameters are optional filters. Returns events newest-first.

## Alert Commands

Alert rules evaluate metric thresholds and transition between states: `ok` → `firing` → `acknowledged`.

### Create an Alert Rule

```
node.invoke <node-id> alerts.create {
  "name": "high-gpu-temp",
  "metric": "gpu:temperature_c",
  "condition": "above",
  "threshold": 85.0
}
```

**Parameters:**
- `name` (required): Unique alert name
- `metric` (required): Metric name to evaluate
- `condition` (required): `above` or `below`
- `threshold` (required): Numeric threshold

### List Alerts

```
node.invoke <node-id> alerts.list
```

Returns all alert rules with their current state (`ok`, `firing`, or `acknowledged`).

### Acknowledge an Alert

```
node.invoke <node-id> alerts.acknowledge { "name": "high-gpu-temp" }
```

Only works when the alert is in `firing` state.

## Common Observability Patterns

### Health Check Workflow
1. `metrics.snapshot` for a quick overview
2. If CPU or memory looks high, `metrics.query` with `rangeMinutes: 60` to see the trend
3. Check `events.query` with `severity: error` for recent issues
4. Check `alerts.list` for any firing alerts

### GPU Monitoring
```
metrics.query { "name": "gpu:utilization_percent", "rangeMinutes": 30 }
metrics.query { "name": "gpu:temperature_c", "rangeMinutes": 30 }
metrics.query { "name": "gpu:memory_used_mb", "rangeMinutes": 30 }
```

### Capacity Planning
Use `metrics.query` with `aggregation: "avg"` over longer windows (e.g., `rangeMinutes: 1440` for 24h) to understand utilization patterns and right-size resources.

### Alert Thresholds

| Metric | Warning | Critical |
|--------|---------|----------|
| `cpu:usage_percent` | above 70 | above 90 |
| `memory:usage_percent` | above 80 | above 95 |
| `gpu:temperature_c` | above 75 | above 85 |
| `gpu:utilization_percent` | below 10 (idle waste) | above 98 (saturated) |
