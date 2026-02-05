# MOLT Network Guide

The MOLT (Machine On-demand Liquidity Token) Network is a decentralized P2P marketplace for GPU compute. Providers earn MOLT tokens by renting out idle GPUs; buyers access on-demand compute without infrastructure overhead.

## Table of Contents

1. [Overview](#overview)
2. [Getting Started](#getting-started)
3. [For Providers](#for-providers)
4. [For Buyers](#for-buyers)
5. [Pricing & Policies](#pricing--policies)
6. [Staking Tiers](#staking-tiers)
7. [Earnings & Payments](#earnings--payments)
8. [Security & Trust](#security--trust)

---

## Overview

### How It Works

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              MOLT Network                                   │
│                                                                             │
│   ┌─────────────────┐              P2P Gossip              ┌─────────────┐ │
│   │    Provider     │◄────────────────────────────────────►│    Buyer    │ │
│   │  (Idle GPUs)    │                                      │  (Compute   │ │
│   │                 │                                      │   Demand)   │ │
│   └────────┬────────┘                                      └──────┬──────┘ │
│            │                                                      │        │
│            │ Execute job                                          │        │
│            ▼                                                      │        │
│   ┌─────────────────┐                                             │        │
│   │  Attestation    │───── Proof of execution ────────────────────┘        │
│   │  (TEE/TPM)      │                                                      │
│   └────────┬────────┘                                                      │
│            │                                                               │
│            ▼                              ┌─────────────────────────────┐  │
│   ┌─────────────────┐                     │          Solana             │  │
│   │ ExecutionProof  │────────────────────►│  • MOLT Token (SPL)         │  │
│   │                 │  Verify & release   │  • Escrow Contract          │  │
│   └─────────────────┘       payment       │  • Settlement               │  │
│                                           └─────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Key Concepts

| Concept | Description |
|---------|-------------|
| **Provider** | Node operator who rents out GPU compute for MOLT tokens |
| **Buyer** | User who pays MOLT tokens for on-demand GPU access |
| **MOLT Token** | Solana SPL token used for payments and staking |
| **Escrow** | Smart contract holding funds until job completion |
| **Attestation** | Hardware verification proving job execution |
| **Autonomy Mode** | How much control the agent has over accepting/rejecting jobs |

---

## Getting Started

### Prerequisites

1. **Clawbernetes node** running and connected to a gateway
2. **Solana wallet** with some SOL for transaction fees
3. **MOLT tokens** (for buyers) or staked tokens (for providers)

### Quick Setup

```bash
# 1. Configure MOLT in clawnode.toml
cat >> /etc/clawbernetes/clawnode.toml << EOF

[molt]
enabled = true
wallet_path = "~/.config/clawbernetes/wallet.json"

# Provider settings
min_price = 1.0        # Min MOLT per GPU-hour
max_jobs = 2           # Max concurrent jobs
EOF

# 2. Create or import a wallet
clawbernetes molt wallet create
# Or import existing:
clawbernetes molt wallet import ~/.solana/id.json

# 3. Join the network
clawbernetes molt join --autonomy moderate

# 4. Check status
clawbernetes molt status
```

---

## For Providers

As a provider, you earn MOLT tokens by renting out your idle GPU capacity.

### Configuration

```toml
# clawnode.toml
[molt]
enabled = true
wallet_path = "~/.config/clawbernetes/wallet.json"

# Minimum price per GPU-hour (in MOLT tokens)
min_price = 1.0

# Maximum concurrent jobs (to limit resource contention)
max_jobs = 2

# Allowed GPU types (empty = all)
allowed_gpus = ["0", "1"]  # Only GPUs 0 and 1

# Job filters
max_job_duration_hours = 24
min_job_duration_hours = 0.5
allowed_workload_types = ["inference", "training", "benchmark"]

# Network
advertise_interval_secs = 60
```

### Joining as Provider

```bash
# Conservative mode: Manual approval for most jobs
clawbernetes molt join --autonomy conservative

# Moderate mode: Auto-accept jobs within policy
clawbernetes molt join --autonomy moderate

# Aggressive mode: Maximum automation
clawbernetes molt join --autonomy aggressive
```

### Autonomy Modes

| Mode | Auto-Accept | Auto-Reject | Manual Review |
|------|-------------|-------------|---------------|
| **Conservative** | Pre-approved buyers | Policy violations | Everything else |
| **Moderate** | Within budget + policy | Clear violations | Edge cases |
| **Aggressive** | All within capability | None | None |

### Monitoring Your Node

```bash
# View current jobs
clawbernetes molt jobs

# View earnings
clawbernetes molt earnings

# View detailed earnings breakdown
clawbernetes molt earnings --detailed

# Check reputation
clawbernetes molt reputation
```

### Provider Best Practices

1. **Set realistic prices** — Check market rates with `clawbernetes molt market`
2. **Start conservative** — Use conservative mode until comfortable
3. **Monitor thermals** — Jobs may push GPUs harder than expected
4. **Keep nodes updated** — Run latest clawnode for security patches
5. **Stake tokens** — Higher tiers get better job visibility

---

## For Buyers

As a buyer, you pay MOLT tokens for on-demand GPU compute.

### Submitting Jobs

```bash
# Simple job request
clawbernetes molt request \
  --gpus 4 \
  --gpu-type H100 \
  --duration 4h \
  --max-price 50.0 \
  --image pytorch/pytorch:latest \
  -- python train.py

# With specific requirements
clawbernetes molt request \
  --gpus 8 \
  --gpu-type A100 \
  --prefer-nvlink \
  --duration 24h \
  --max-price 200.0 \
  --image my-registry/my-training:v1.2
```

### Job Specification File

For complex jobs, use a TOML spec:

```toml
# job.toml
[job]
name = "llm-finetune"
description = "Fine-tune Llama 70B"

[job.requirements]
gpus = 8
gpu_type = "H100"
prefer_nvlink = true
min_vram_gb = 80
duration_hours = 24

[job.budget]
max_total = 500.0       # Max total spend
max_per_hour = 25.0     # Max per GPU-hour
prefer_cheapest = true  # Or false for fastest match

[job.workload]
image = "my-registry/finetune:latest"
env = { MODEL = "meta-llama/Llama-2-70b-hf", EPOCHS = "3" }
command = ["python", "train.py"]

[job.data]
# Optional: Data transfer setup
source = "s3://my-bucket/training-data"
destination = "/data"
```

Submit with:
```bash
clawbernetes molt submit --spec job.toml
```

### Monitoring Jobs

```bash
# List your jobs
clawbernetes molt jobs --mine

# View job details
clawbernetes molt job info job-abc123

# View job logs
clawbernetes molt logs job-abc123

# Cancel a job (forfeits partial payment)
clawbernetes molt cancel job-abc123
```

### Buyer Best Practices

1. **Set max prices** — Protect against price spikes
2. **Use reputable providers** — Check ratings before accepting bids
3. **Start small** — Test with short jobs before committing
4. **Use checksums** — Verify data integrity for sensitive workloads
5. **Enable attestation** — Require hardware verification for critical jobs

---

## Pricing & Policies

### Market Rates

View current market rates:

```bash
clawbernetes molt market

# Output:
# MOLT MARKET RATES (last 24h)
# ═══════════════════════════════════════
# GPU Type      │ Median    │ Low      │ High
# ──────────────┼───────────┼──────────┼─────────
# H100-SXM5     │ 4.50      │ 3.20     │ 8.00
# H100-PCIe     │ 3.80      │ 2.50     │ 6.00
# A100-SXM4     │ 2.20      │ 1.50     │ 4.00
# A100-PCIe     │ 1.80      │ 1.20     │ 3.50
# RTX 4090      │ 0.80      │ 0.50     │ 1.50
```

### Provider Policies

Configure what jobs you accept:

```toml
# clawnode.toml
[molt.policy]
# Price requirements
min_price_per_gpu_hour = 1.0
min_price_multiplier_peak = 1.5  # 1.5x during peak hours

# Time constraints
max_job_duration_hours = 24
min_job_duration_hours = 0.5
blackout_hours = [0, 1, 2, 3, 4, 5]  # No jobs 12am-6am

# Buyer requirements
min_buyer_reputation = 3.0
require_verified_buyer = false

# Workload requirements
allowed_images = ["pytorch/*", "nvidia/*", "huggingface/*"]
blocked_images = ["*:latest"]  # Require specific tags
require_attestation = false
```

### Buyer Policies

Configure job preferences:

```bash
# Set default budget
clawbernetes molt config set --max-spend-per-job 100.0

# Set preferred providers
clawbernetes molt config set --preferred-providers "provider-1,provider-2"

# Set minimum provider reputation
clawbernetes molt config set --min-provider-reputation 4.0

# Require attestation
clawbernetes molt config set --require-attestation true
```

---

## Staking Tiers

Stake MOLT tokens to unlock benefits:

| Tier | Stake Required | Benefits |
|------|----------------|----------|
| **Bronze** | 100 MOLT | Basic marketplace access |
| **Silver** | 1,000 MOLT | Priority job matching, lower fees (1.5%) |
| **Gold** | 10,000 MOLT | Featured listing, lowest fees (1%), analytics |
| **Diamond** | 100,000 MOLT | Premium support, custom SLAs, 0.5% fees |

### Staking Commands

```bash
# View current stake
clawbernetes molt stake status

# Stake tokens
clawbernetes molt stake deposit 1000

# Unstake (7-day unbonding period)
clawbernetes molt stake withdraw 500

# View tier benefits
clawbernetes molt stake tiers
```

### Fee Structure

| Transaction | Bronze | Silver | Gold | Diamond |
|-------------|--------|--------|------|---------|
| Job completion | 2.5% | 1.5% | 1.0% | 0.5% |
| Instant withdrawal | 1.0% | 0.5% | 0.25% | 0% |
| Dispute resolution | 5% escrow | 3% | 2% | 1% |

---

## Earnings & Payments

### Viewing Earnings

```bash
# Summary
clawbernetes molt earnings

# Output:
# MOLT EARNINGS (Last 30 Days)
# ═══════════════════════════════════════
# Total Earned:     456.78 MOLT
# Jobs Completed:   142
# Avg per Job:      3.22 MOLT
# Avg Rating:       4.8/5.0
# 
# Pending:          23.50 MOLT (2 jobs in progress)
# Available:        433.28 MOLT

# Detailed breakdown
clawbernetes molt earnings --detailed --since 7d
```

### Payment Flow

```
1. Buyer submits job         ──► Funds escrowed on Solana
2. Provider accepts bid      ──► Job starts
3. Provider executes job     ──► Progress reported
4. Provider submits proof    ──► Attestation verified
5. Escrow releases payment   ──► Provider receives MOLT (minus fees)
```

### Withdrawing Earnings

```bash
# View balance
clawbernetes molt balance

# Withdraw to external wallet
clawbernetes molt withdraw 100.0 --to <SOLANA_ADDRESS>

# Withdraw all
clawbernetes molt withdraw --all --to <SOLANA_ADDRESS>
```

### Tax Reporting

```bash
# Export transaction history (CSV)
clawbernetes molt export --format csv --since 2025-01-01 > molt-2025.csv

# Export as JSON
clawbernetes molt export --format json --year 2025 > molt-2025.json
```

---

## Security & Trust

### Hardware Attestation

MOLT uses hardware attestation to verify job execution:

```
┌────────────────────────────────────────────────────────────────────────────┐
│                           Attestation Flow                                 │
│                                                                            │
│  ┌─────────────────┐                      ┌─────────────────┐              │
│  │     Provider    │                      │      Buyer      │              │
│  └────────┬────────┘                      └────────┬────────┘              │
│           │                                        │                       │
│           │◄───────── Challenge (nonce) ───────────│                       │
│           │                                        │                       │
│           ▼                                        │                       │
│  ┌─────────────────┐                               │                       │
│  │  TEE/TPM signs  │                               │                       │
│  │  • Hardware ID  │                               │                       │
│  │  • Nonce        │                               │                       │
│  │  • Job hash     │                               │                       │
│  │  • GPU metrics  │                               │                       │
│  └────────┬────────┘                               │                       │
│           │                                        │                       │
│           │─────── Signed Attestation ────────────►│                       │
│           │                                        │                       │
│           │                               ┌────────▼────────┐              │
│           │                               │ Verify against  │              │
│           │                               │ known hardware  │              │
│           │                               └────────┬────────┘              │
│           │                                        │                       │
│           │◄────────── Payment Released ───────────│                       │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

### Reputation System

Both providers and buyers have reputation scores:

| Metric | Weight | Description |
|--------|--------|-------------|
| Job completion rate | 30% | Percentage of jobs completed successfully |
| On-time delivery | 25% | Jobs finished within estimated time |
| Buyer/provider rating | 25% | Average rating from counterparties |
| Dispute rate | 15% | Lower is better |
| Tenure | 5% | Time active on network |

```bash
# View your reputation
clawbernetes molt reputation

# View another user's reputation
clawbernetes molt reputation --user <WALLET_ADDRESS>
```

### Dispute Resolution

If something goes wrong:

```bash
# File a dispute (within 24h of job completion)
clawbernetes molt dispute file job-abc123 \
  --reason "Job did not complete as specified" \
  --evidence "logs show GPU was not allocated"

# Check dispute status
clawbernetes molt dispute status dispute-xyz789
```

Disputes are resolved by:
1. **Automatic** — Clear evidence (attestation mismatch, logs)
2. **Arbitration** — Random selection of 3 staked community members
3. **Appeal** — Diamond tier can appeal to protocol governance

### Security Checklist

**For Providers:**
- [ ] Use dedicated machine for MOLT (no sensitive data)
- [ ] Enable attestation hardware if available (TPM, SGX)
- [ ] Set reasonable job limits (`max_jobs`)
- [ ] Review workload images before allowing
- [ ] Monitor for unusual resource usage

**For Buyers:**
- [ ] Require attestation for sensitive workloads
- [ ] Use reputable providers (4.0+ rating)
- [ ] Don't embed secrets in job specs (use secret injection)
- [ ] Verify results with checksums
- [ ] Set spending limits

---

## Troubleshooting

### "No providers available"

1. Check your requirements aren't too restrictive
2. Try increasing `max_price`
3. Check if you're requesting rare GPU types
4. Try during off-peak hours

### "Attestation failed"

1. Verify provider has compatible hardware
2. Check job didn't exceed resource limits
3. Review job logs for errors

### "Payment stuck in escrow"

1. Wait for attestation verification (can take up to 10 minutes)
2. Check Solana network status
3. Contact support if > 1 hour

---

## See Also

- [User Guide](user-guide.md) — Getting started with Clawbernetes
- [CLI Reference](cli-reference.md) — Full command reference
- [Architecture](architecture.md) — MOLT technical architecture
- [Security Guide](security.md) — Security best practices
