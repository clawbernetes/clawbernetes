---
name: molt-marketplace
description: MOLT P2P GPU compute marketplace — offer idle GPUs, find compute, manage escrow and attestation.
metadata: {"openclaw": {"always": true}}
---

# MOLT Marketplace

MOLT is a peer-to-peer GPU compute marketplace. Providers offer idle GPUs, buyers rent them, payment via MOLT tokens on Solana.

## Prerequisites

clawnode must be built with `--features molt`:
```bash
cargo install --path crates/clawnode --features molt
```

## Provider Operations (selling GPU time)

### List Available Capacity
```bash
# Check which GPUs are idle
exec host=node node=<name> command="nvidia-smi --query-gpu=index,utilization.gpu,memory.used,memory.total --format=csv,noheader"
```

### Offer GPUs
```bash
# Via clawnode invoke
nodes invoke --node <name> --command molt.discover --params '{"action":"announce","gpus":[0,1],"pricePerHour":0.50,"maxHours":24}'
```

### Check Earnings
```bash
nodes invoke --node <name> --command molt.balance --params '{}'
```

### Set Autonomy Mode
- **Conservative**: approve every job manually
- **Moderate**: auto-accept jobs matching policy
- **Aggressive**: accept all jobs within price range

## Buyer Operations (renting GPU time)

### Find Available GPUs
```bash
nodes invoke --node <name> --command molt.discover --params '{"action":"search","gpuType":"H100","count":4,"maxPrice":1.00}'
```

### Place Bid
```bash
nodes invoke --node <name> --command molt.bid --params '{"offerId":"<id>","hours":6,"pricePerHour":0.75}'
```

### Check Job Status
```bash
nodes invoke --node <name> --command molt.status --params '{"jobId":"<id>"}'
```

## Reputation

```bash
nodes invoke --node <name> --command molt.reputation --params '{"peerId":"<peer>"}'
```

Reputation factors: uptime, job completion rate, hardware attestation validity.

## Security

- All compute is **hardware-attested** (TEE/TPM verification)
- Payment held in **Solana escrow** until job completion
- Attestation proves actual GPU model/VRAM matches listing

## Workflow

1. **Provider**: Identify idle GPUs → announce on MOLT network
2. **Buyer**: Search for matching GPUs → place bid
3. **Match**: Escrow funds → attestation check → job starts
4. **Complete**: Job finishes → escrow releases → reputation updated
