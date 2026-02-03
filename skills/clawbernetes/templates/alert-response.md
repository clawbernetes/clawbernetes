# Alert Response Template

Use this template when responding to cluster alerts, health issues, or diagnostic questions.

---

## Quick Health Report

```
## 🏥 Cluster Health Report

**Status:** [🟢 Healthy | 🟡 Degraded | 🔴 Critical]
**Score:** [X]/100
**Timestamp:** [YYYY-MM-DD HH:MM TZ]

### Summary
[One-sentence overview of cluster state]

### Node Status
- 🟢 Healthy: [N] nodes
- 🟡 Degraded: [N] nodes  
- 🔴 Critical: [N] nodes

### Resource Utilization
- GPU: [X]% average
- Memory: [X]% average
- Network: [X]% capacity

### Active Alerts
[List any active alerts or "No active alerts"]
```

---

## Issue Report Template

```
## 🔍 Issue Analysis: [Brief Title]

### What's Happening
[Clear, non-technical explanation of the issue]

### Impact
- **Severity:** [Low | Medium | High | Critical]
- **Affected:** [What's impacted - nodes, workloads, users]
- **Duration:** [How long has this been happening]

### Root Cause
[Explanation of why this is happening]

### Evidence
- [Metric or log excerpt 1]
- [Metric or log excerpt 2]
- [Relevant diagnostic output]

### Immediate Actions
- [ ] [Action 1 with command if applicable]
- [ ] [Action 2 with command if applicable]

### Recommended Fix
[Detailed remediation steps]

### Prevention
[How to prevent this in the future]
```

---

## Diagnostic Response Template

```
## 🔬 Diagnostic Results: [node/workload ID]

### Quick Summary
[One paragraph explaining the finding]

### Status Indicators
| Component | Status | Value | Threshold |
|-----------|--------|-------|-----------|
| GPU Util  | 🟢/🟡/🔴 | X%   | >80% warn |
| GPU Temp  | 🟢/🟡/🔴 | X°C  | >75° warn |
| Memory    | 🟢/🟡/🔴 | X GB | >90% warn |
| [etc.]    | ...    | ...   | ...       |

### Findings
1. **[Finding 1]**: [Explanation]
2. **[Finding 2]**: [Explanation]

### Recommendations
1. **[Priority: High/Med/Low]** [Recommendation]
   ```bash
   [command to execute]
   ```

2. **[Priority: High/Med/Low]** [Recommendation]
   ```bash
   [command to execute]
   ```

### Follow-up Questions
- [Question to clarify user intent]
- [Question about constraints/preferences]
```

---

## Common Issue Responses

### GPU Thermal Throttling

```
## 🌡️ GPU Thermal Issue Detected

**Node:** [node-id]
**Current Temp:** [X]°C (Threshold: 85°C)

### What's Happening
GPU is running hot and may be throttling performance to prevent damage.

### Immediate Actions
1. Check current workloads on this node:
   ```bash
   clawbernetes workload list --node [node-id] --running
   ```

2. If possible, migrate critical workloads:
   ```bash
   clawbernetes workload migrate [workload-id] --target [cooler-node]
   ```

3. Monitor temperature:
   ```bash
   clawbernetes metrics gpu.temperature --node [node-id] --last 1h
   ```

### Root Causes to Investigate
- [ ] Datacenter cooling issues
- [ ] Blocked airflow / dirty fans
- [ ] Unusually heavy workload
- [ ] Hardware degradation

### Questions
- Is this node in a location with known cooling issues?
- Has the workload profile changed recently?
```

### Memory Pressure / OOM

```
## 💾 Memory Pressure Alert

**Node/Workload:** [id]
**Memory Usage:** [X]GB / [Y]GB ([Z]%)

### What's Happening
System is running low on memory, which can cause:
- Swap thrashing (severe performance degradation)
- OOM killer terminating processes
- Workload failures

### Immediate Actions
1. Check memory consumers:
   ```bash
   clawbernetes diagnose [node/workload] [id] --verbose
   ```

2. If workload-specific, consider:
   - Reducing batch size
   - Using gradient checkpointing
   - Requesting more memory:
   ```bash
   clawbernetes workload update [id] --memory [larger-amount]
   ```

3. If node-wide, identify largest consumers:
   ```bash
   clawbernetes workload list --node [node-id] --sort memory
   ```

### Questions
- What batch size are you using?
- Is gradient checkpointing enabled?
- Can some workloads be moved to other nodes?
```

### Slow Training Performance

```
## 🐢 Training Performance Analysis

**Workload:** [id]
**Reported Issue:** Training slower than expected

### Diagnostic Results
[Output from diagnose command]

### Bottleneck Analysis

| Component | Status | Notes |
|-----------|--------|-------|
| GPU Compute | [🟢/🟡/🔴] | [Utilization %] |
| GPU Memory | [🟢/🟡/🔴] | [Usage/Capacity] |
| Data Loading | [🟢/🟡/🔴] | [Wait time %] |
| Network I/O | [🟢/🟡/🔴] | [Throughput] |

### Most Likely Cause
[Explanation based on diagnostics]

### Recommendations
1. **If GPU underutilized (data bottleneck):**
   - Increase DataLoader workers
   - Use faster storage (NVMe vs network)
   - Pre-process data / use caching

2. **If GPU thermal throttling:**
   - Migrate to cooler node
   - Reduce workload intensity temporarily

3. **If memory pressure:**
   - Reduce batch size
   - Enable gradient checkpointing
   - Use mixed precision (fp16/bf16)

### Questions to Clarify
- What throughput are you seeing vs expected?
- Have you changed anything recently (data, model, config)?
- Is this a regression or always been slow?
```

---

## Follow-up Question Bank

### Understanding the Problem
- Can you describe what you expected vs what's happening?
- When did you first notice this issue?
- Has anything changed recently (code, data, config)?

### Scoping Impact
- Is this affecting other workloads/users?
- How urgent is this? (blocking production, experiment, exploration)
- What's the business impact if this continues?

### Gathering Context
- What's the workload doing? (training, inference, data processing)
- What framework/model are you using?
- What's your typical resource profile?

### Clarifying Constraints
- Do you have flexibility on which nodes to use?
- Can we restart/migrate the workload?
- Are there cost constraints we should consider?

### Next Steps
- Would you like me to run more diagnostics?
- Should I attempt automatic remediation?
- Do you want to set up monitoring/alerts for this?

---

## Response Principles

1. **Lead with impact**: Start with what the user cares about
2. **Be specific**: Include actual values, not just "high" or "low"
3. **Provide commands**: Make it easy to take action
4. **Explain reasoning**: Help users understand the "why"
5. **Offer next steps**: Don't leave users hanging
6. **Ask clarifying questions**: Better to ask than assume
7. **Use visual indicators**: 🟢🟡🔴 make status instantly clear
8. **Keep it scannable**: Use headers, bullets, tables
