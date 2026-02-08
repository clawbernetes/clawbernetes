# MOLT Marketplace

You can discover GPU providers, submit bids for compute, and manage marketplace interactions on the MOLT decentralized GPU network.

Requires the `molt` feature on the node.

## Commands

### Discover Providers

```
node.invoke <node-id> molt.discover {
  "gpuType": "nvidia",
  "minVram": 24,
  "maxPrice": 500
}
```

**Parameters (all optional):**
- `gpuType`: Filter by GPU type/capability (default "gpu")
- `minVram`: Minimum VRAM in GB
- `maxPrice`: Maximum price per GPU-hour (in MOLT nano-tokens)

Returns: matching providers with peer IDs and capabilities.

### Submit a Bid

```
node.invoke <node-id> molt.bid {
  "providerId": "peer-abc123",
  "jobSpec": {
    "gpus": 2,
    "gpuModel": "A100",
    "memoryGb": 80,
    "durationHours": 24
  },
  "maxPrice": 1000
}
```

**Parameters:**
- `providerId` (required): Target provider's peer ID
- `jobSpec` (required): Job requirements
  - `gpus`: Number of GPUs needed
  - `gpuModel` (optional): Specific GPU model
  - `memoryGb`: Minimum GPU memory
  - `durationHours`: Maximum job duration
- `maxPrice` (required): Maximum price willing to pay

Returns: `orderId`, `state` (submitted), `success`

### Check Order Status

```
node.invoke <node-id> molt.status {
  "orderId": "<order-id>"
}
```

Or by job ID:
```
node.invoke <node-id> molt.status {
  "jobId": "<job-id>"
}
```

Returns: order details, buyer, max price, state.

### Check Wallet Balance

```
node.invoke <node-id> molt.balance
```

Returns: public key, balance, and on-chain integration status.

### Check Provider Reputation

```
node.invoke <node-id> molt.reputation {
  "peerId": "<hex-encoded-peer-id>"
}
```

Returns: peer capabilities, reputation score, and attestation status.

## Common Patterns

### Find Cheap A100 Compute
```json
// 1. Discover
{"gpuType": "nvidia", "minVram": 80}
// 2. Bid on the best provider
{"providerId": "<id>", "jobSpec": {"gpus": 8, "memoryGb": 80, "durationHours": 48}, "maxPrice": 5000}
// 3. Monitor
{"orderId": "<returned-order-id>"}
```

### Quick Inference Job
```json
{
  "providerId": "<id>",
  "jobSpec": {"gpus": 1, "memoryGb": 24, "durationHours": 1},
  "maxPrice": 50
}
```

## How MOLT Works

1. **Discover** — Search the peer-to-peer network for providers matching your GPU requirements
2. **Bid** — Submit an order to the order book with your job spec and max price
3. **Match** — The marketplace matches orders with capacity offers from providers
4. **Execute** — Jobs run on provider hardware with attestation verification
5. **Settle** — Payment is released from escrow upon verified completion

## MOLT Token

MOLT is the native token for the decentralized GPU marketplace. Balances require on-chain integration for full tracking. The wallet uses Ed25519 keys for signing transactions.
