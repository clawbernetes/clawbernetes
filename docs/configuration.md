# Configuration

How to configure the gateway, nodes, and OpenClaw plugin.

---

## Gateway

The gateway accepts a single argument — the bind address:

```bash
./claw-gateway                    # binds to 0.0.0.0:8080
./claw-gateway 0.0.0.0:18789     # custom port
```

---

## Node

Nodes can be configured via CLI flags, environment variables, or a config file:

```toml
# /etc/clawnode/config.toml

[node]
name = "gpu-node-01"
gateway = "ws://gateway.example.com:8080"
reconnect_interval_secs = 5

[gpu]
memory_alert_threshold = 90

[metrics]
interval_secs = 10
detailed_gpu_metrics = true

[network]
provider = "wireguard"    # or "tailscale"

[network.wireguard]
listen_port = 51820

[molt]
enabled = false
# min_price = 1.0
# max_jobs = 2

[logging]
level = "info"
format = "pretty"
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `CLAWNODE_GATEWAY` | Gateway WebSocket URL | required |
| `CLAWNODE_NAME` | Node identifier | system hostname |
| `CLAWNODE_CONFIG` | Config file path | `/etc/clawnode/config.json` |
| `RUST_LOG` | Log level | `info` |

---

## Plugin

The OpenClaw plugin is configured in your OpenClaw settings:

```json
{
  "plugins": {
    "clawbernetes": {
      "enabled": true,
      "gatewayUrl": "http://127.0.0.1:8080",
      "healthIntervalMs": 60000,
      "invokeTimeoutMs": 30000
    }
  },
  "tools": {
    "alsoAllow": ["clawbernetes"]
  }
}
```

See `plugin/examples/openclaw-config.json` for a full example with agent configurations and tool permissions.

---

## See Also

- [Docker Deployment](docker.md) — Running with Docker
- [Architecture](architecture.md) — System design and crate map
- [WireGuard Integration](wireguard-integration.md) — Self-hosted mesh networking
- [Tailscale Integration](tailscale-integration.md) — Managed networking
