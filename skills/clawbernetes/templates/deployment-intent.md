# Deployment Intent Templates

Reference guide for AI-assisted deployment intent parsing. These patterns help the AI understand natural language deployment requests and translate them into concrete actions.

## Intent Pattern Recognition

### GPU Workload Deployments

#### Basic GPU Request
```
"deploy <workload> with <N> GPUs"
"run <workload> on <N> <gpu-type>"
"launch <workload> using <N> A100s"
```

**Parsed as:**
```yaml
action: deploy
workload: <workload>
resources:
  gpus: <N>
  gpu_type: <gpu-type>  # optional
```

**Examples:**
```
"deploy pytorch-train with 4 GPUs"
"run inference-server on 2 H100s"  
"launch stable-diffusion using 8 A100-80GB"
```

#### GPU + Memory
```
"deploy <workload> with <N> GPUs and <M> memory"
"run <workload> on <N> GPUs, <M>Gi RAM"
```

**Examples:**
```
"deploy llama-finetune with 4 GPUs and 256Gi memory"
"run embedding-server on 2 A100s, 128Gi RAM"
```

#### GPU + Priority
```
"deploy <workload> with <N> GPUs, priority <level>"
"urgent: deploy <workload> with <N> GPUs"
```

**Examples:**
```
"deploy critical-inference with 4 GPUs, priority high"
"urgent: deploy customer-model with 8 A100s"
```

---

### Canary Deployments

#### Basic Canary
```
"canary <percentage>% <workload>"
"deploy <workload>, canary <percentage>% first"
"<workload> canary at <percentage>%"
```

**Parsed as:**
```yaml
action: deploy
workload: <workload>
strategy: canary
canary:
  percentage: <percentage>
  auto_promote: false
```

**Examples:**
```
"canary 10% model-server v2.0"
"deploy api-gateway, canary 5% first"
"inference-engine canary at 20%"
```

#### Canary with Auto-Promote
```
"canary <N>% <workload>, promote after <duration>"
"deploy <workload> canary <N>%, auto-promote in <duration> if healthy"
```

**Parsed as:**
```yaml
action: deploy
strategy: canary
canary:
  percentage: <N>
  auto_promote: true
  promote_after: <duration>
  health_check: true
```

**Examples:**
```
"canary 10% model-server, promote after 30m"
"deploy api-v3 canary 5%, auto-promote in 1h if healthy"
```

#### Canary with Conditions
```
"canary <N>% <workload>, rollback if <condition>"
"deploy <workload> canary, abort if <metric> > <threshold>"
```

**Parsed as:**
```yaml
action: deploy
strategy: canary
canary:
  percentage: <N>
  rollback_conditions:
    - metric: <metric>
      operator: <op>
      threshold: <value>
```

**Examples:**
```
"canary 10% api-server, rollback if errors > 1%"
"deploy model-v2 canary, abort if latency_p99 > 500ms"
"canary 20% inference, rollback if error_rate > 0.5% or cpu > 90%"
```

---

### Blue-Green Deployments

#### Basic Blue-Green
```
"blue-green deploy <workload>"
"deploy <workload> blue-green"
"<workload> blue-green switch"
```

**Parsed as:**
```yaml
action: deploy
strategy: blue_green
blue_green:
  instant_switch: false
  health_check: true
```

**Examples:**
```
"blue-green deploy payment-service"
"deploy auth-server blue-green"
```

#### Blue-Green with Switch Condition
```
"blue-green <workload>, switch after <condition>"
"deploy <workload> blue-green, cutover when <condition>"
```

**Examples:**
```
"blue-green deploy api v3.0, switch after health check passes"
"deploy model-server blue-green, cutover when latency < 100ms"
```

---

### Rolling Updates

#### Basic Rolling Update
```
"update <workload> to <version>"
"roll out <version> to <workload>"
"upgrade <workload> to <version>"
```

**Parsed as:**
```yaml
action: deploy
strategy: rolling
rolling:
  max_unavailable: "25%"
  max_surge: "25%"
```

**Examples:**
```
"update model-server to v2.1"
"roll out v3.0 to api-gateway"
"upgrade inference-engine to latest"
```

#### Rolling with Pace
```
"update <workload> slowly"
"roll out <workload> <N>% at a time"
"gradual update <workload>"
```

**Parsed as:**
```yaml
action: deploy
strategy: rolling
rolling:
  max_unavailable: "10%"
  max_surge: "10%"
  pause_between: "30s"
```

**Examples:**
```
"update critical-api slowly to v2.5"
"roll out model-server 10% at a time"
"gradual update payment-processor to v4.0"
```

---

### Scaling Operations

#### Scale Replicas
```
"scale <workload> to <N>"
"scale <workload> to <N> replicas"
"set <workload> replicas to <N>"
```

**Parsed as:**
```yaml
action: scale
workload: <workload>
replicas: <N>
```

**Examples:**
```
"scale inference to 8"
"scale api-server to 20 replicas"
"set model-server replicas to 5"
```

#### Relative Scaling
```
"scale up <workload> by <N>"
"scale down <workload> by <N>"
"double <workload> capacity"
"halve <workload> replicas"
```

**Examples:**
```
"scale up inference by 4"
"scale down api-server by 2"
"double model-server capacity"
```

#### Auto-Scaling Adjustment
```
"set <workload> min replicas to <N>"
"set <workload> max replicas to <N>"
"adjust <workload> autoscaling <min>-<max>"
```

**Examples:**
```
"set inference min replicas to 3"
"set api-server max replicas to 50"
"adjust model-server autoscaling 5-20"
```

---

### Rollback Scenarios

#### Immediate Rollback
```
"rollback <workload>"
"rollback <workload> now"
"emergency rollback <workload>"
```

**Parsed as:**
```yaml
action: rollback
workload: <workload>
target: previous
immediate: false  # or true for "emergency"
```

**Examples:**
```
"rollback model-server"
"rollback api-gateway now"
"emergency rollback payment-service"
```

#### Rollback to Specific Version
```
"rollback <workload> to <version>"
"revert <workload> to <version>"
"restore <workload> to <version>"
```

**Examples:**
```
"rollback model-server to v2.0"
"revert api to v1.5.2"
"restore inference to last-known-good"
```

#### Rollback Deployment
```
"rollback deployment <id>"
"abort deployment <id>"
"cancel and rollback <id>"
```

**Examples:**
```
"rollback deployment dep-abc123"
"abort deployment dep-xyz789"
```

---

### Scheduled Deployments

#### Deploy at Time
```
"deploy <workload> at <time>"
"schedule <workload> deployment for <time>"
"<workload> deploy at <time> <timezone>"
```

**Parsed as:**
```yaml
action: deploy
schedule:
  at: <time>
  timezone: <tz>  # optional
```

**Examples:**
```
"deploy model-v3 at 2am"
"schedule api-update for 3:00 AM PST"
"model-server deploy at midnight UTC"
```

#### Deploy with Maintenance Window
```
"deploy <workload> during maintenance window"
"deploy <workload> in next maintenance"
```

---

### Complex Multi-Condition Intents

#### Canary with Multiple Conditions
```
"deploy <workload> canary 10%, promote after 1h if healthy, rollback if errors > 1% or latency > 500ms"
```

**Parsed as:**
```yaml
action: deploy
strategy: canary
canary:
  percentage: 10
  auto_promote: true
  promote_after: "1h"
  health_check: true
  rollback_conditions:
    - metric: error_rate
      operator: ">"
      threshold: 0.01
    - metric: latency_p99
      operator: ">"
      threshold: "500ms"
```

#### GPU Workload with Full Options
```
"deploy training-job with 8 A100s, 512Gi memory, priority high, timeout 72h, canary 5% first"
```

**Parsed as:**
```yaml
action: deploy
workload: training-job
resources:
  gpus: 8
  gpu_type: A100
  memory: "512Gi"
priority: high
timeout: "72h"
strategy: canary
canary:
  percentage: 5
```

---

## AI Response Guidelines

When parsing deployment intents:

### 1. Confirm Understanding
Before executing, summarize what you understood:
```
I'll deploy model-server v2.1 with:
- Strategy: Canary (10%)
- Auto-promote: After 30 minutes if healthy
- Rollback: If error rate exceeds 1%

Proceed? [Y/n]
```

### 2. Fill Reasonable Defaults
If not specified:
- **Strategy**: Rolling update
- **Canary %**: 10%
- **Health check**: Enabled
- **Timeout**: 30 minutes

### 3. Warn About Risks
```
⚠️  This is a production deployment with no canary.
    Consider: "canary 10% first, rollback if errors > 1%"
```

### 4. Suggest Improvements
```
💡 Tip: Add "--watch" to monitor deployment progress
💡 Tip: Consider adding rollback conditions for safer deployment
```

### 5. Validate Pre-conditions
Before deploying, check:
- [ ] Workload exists or image is valid
- [ ] Required secrets are present
- [ ] Sufficient resources available
- [ ] No conflicting deployments in progress

---

## Common Intent Shortcuts

| User Says | Interpreted As |
|-----------|----------------|
| "ship it" | Deploy with defaults |
| "safely deploy" | Canary 10%, rollback on errors |
| "yolo deploy" | Immediate, no canary (warn!) |
| "careful update" | Canary 5%, manual promote |
| "fast rollback" | Immediate rollback, skip drain |
| "scale for traffic" | Double replicas |
| "wind down" | Scale to minimum |
