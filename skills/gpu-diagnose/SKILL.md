---
name: gpu-diagnose
description: Diagnose GPU health issues — thermal throttling, memory errors, utilization anomalies, driver problems.
metadata: {"openclaw": {"always": true}}
---

# GPU Diagnostics

## Quick Health Check

```bash
# Temperature, utilization, memory, power for all GPUs
exec host=node node=<name> command="nvidia-smi --query-gpu=index,temperature.gpu,utilization.gpu,utilization.memory,memory.used,memory.total,power.draw,power.limit,clocks.current.sm,clocks.max.sm --format=csv"
```

## Thermal Analysis

```bash
# Check for thermal throttling
exec host=node node=<name> command="nvidia-smi --query-gpu=index,temperature.gpu,temperature.gpu.tlimit,clocks_throttle_reasons.sw_thermal_slowdown --format=csv"

# Fan speed
exec host=node node=<name> command="nvidia-smi --query-gpu=index,fan.speed --format=csv"
```

**Thresholds:**
- < 70°C: Normal
- 70-80°C: Warm (acceptable under load)
- 80-90°C: Hot (investigate cooling)
- > 90°C: **Critical** — risk of throttling/shutdown

## Memory Errors

```bash
# ECC errors (correctable and uncorrectable)
exec host=node node=<name> command="nvidia-smi --query-gpu=index,ecc.errors.corrected.volatile.total,ecc.errors.uncorrected.volatile.total --format=csv"

# Retired pages
exec host=node node=<name> command="nvidia-smi --query-retired-pages=gpu_uuid,retired_pages.address --format=csv 2>/dev/null || echo 'No retired pages data'"
```

**Action on ECC errors:**
- Correctable: Monitor. Log increasing trends.
- Uncorrectable: **Migrate workloads off this GPU immediately.**

## Performance Analysis

```bash
# Clock speeds vs max (detect throttling)
exec host=node node=<name> command="nvidia-smi --query-gpu=index,clocks.current.sm,clocks.max.sm,clocks.current.memory,clocks.max.memory --format=csv"

# Process list on GPUs
exec host=node node=<name> command="nvidia-smi --query-compute-apps=pid,name,gpu_uuid,used_gpu_memory --format=csv"

# PCIe bandwidth
exec host=node node=<name> command="nvidia-smi --query-gpu=index,pcie.link.gen.current,pcie.link.gen.max,pcie.link.width.current,pcie.link.width.max --format=csv"
```

## Driver and CUDA

```bash
exec host=node node=<name> command="nvidia-smi --query-gpu=driver_version,cuda_version --format=csv,noheader | head -1"
exec host=node node=<name> command="nvcc --version 2>/dev/null || echo 'CUDA toolkit not installed'"
```

## Diagnosis Workflow

1. Check temperatures → identify thermal issues
2. Check utilization → stuck at 0% (process hung?) or 100% (expected under load?)
3. Check memory → OOM risk? Leaked allocations?
4. Check ECC → hardware degradation?
5. Check clocks → throttling below max?
6. Check PCIe → bandwidth bottleneck?
7. Recommend action: migrate, cool, restart, replace
