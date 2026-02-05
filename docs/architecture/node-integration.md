# OpenClaw + Clawbernetes Node Integration

## Overview

OpenClaw already has a node pairing system for companion apps (iOS/macOS) that provides:
- Camera access, screen recording, notifications
- Location services
- Remote command execution (`system.run`)

Clawbernetes extends this with GPU-specific capabilities to create a unified node model.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           OPENCLAW GATEWAY                                   │
│                    (runs as main orchestration point)                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌──────────────────────┐  ┌──────────────────────┐  ┌───────────────────┐ │
│  │    Node Pairing      │  │   Clawbernetes      │  │    Agent Tools    │ │
│  │    (existing)        │  │   Bridge Plugin     │  │    (existing)     │ │
│  │                      │  │                      │  │                   │ │
│  │  - pending/approve   │  │  - cluster_status   │  │  - nodes tool     │ │
│  │  - token validation  │  │  - workload_*       │  │  - camera/screen  │ │
│  │  - capability disco  │  │  - network_scan     │  │  - system.run     │ │
│  └──────────────────────┘  └──────────────────────┘  └───────────────────┘ │
│            │                         │                         │           │
└────────────┼─────────────────────────┼─────────────────────────┼───────────┘
             │                         │                         │
             │  WebSocket              │  JSON-RPC               │  WebSocket
             │                         │  (stdio)                │
             ▼                         ▼                         ▼
┌─────────────────────┐    ┌─────────────────────┐    ┌─────────────────────┐
│   Mobile Node       │    │   claw-bridge       │    │   clawnode          │
│   (companion app)   │    │   (Rust binary)     │    │   (GPU agent)       │
│                     │    │                     │    │                     │
│   caps:             │    │   - 93 handlers     │    │   caps:             │
│   - camera          │    │   - state mgmt      │    │   - gpu.list        │
│   - screen          │    │   - real crate APIs │    │   - gpu.metrics     │
│   - location        │    │                     │    │   - workload.run    │
│   - notify          │    │                     │    │   - container.exec  │
│   - system.run      │    │                     │    │   - system.run      │
└─────────────────────┘    └─────────────────────┘    └─────────────────────┘
```

## Node Types

### 1. Companion Nodes (Existing)
Mobile/desktop apps that provide peripheral access.

```typescript
caps: ["camera", "screen", "location", "notify", "system.run"]
commands: ["camera.snap", "camera.clip", "screen.record", "location.get", "system.notify"]
```

### 2. GPU Nodes (New - Clawbernetes)
Server agents that provide compute access.

```typescript
caps: ["gpu", "container", "workload", "metrics", "system"]
commands: [
  "gpu.list",       // List GPUs on this node
  "gpu.metrics",    // Get GPU utilization, temp, memory
  "workload.run",   // Run a container workload
  "workload.stop",  // Stop a running workload
  "workload.logs",  // Stream workload logs
  "container.exec", // Execute command in container
  "system.run",     // Run arbitrary command (shared with companion)
  "system.info",    // Get system info (RAM, disk, network)
]
```

## Pairing Flow Comparison

### Companion App Pairing (Existing)
```
1. User opens companion app
2. App sends pairing request to gateway
3. Gateway shows pending request in chat
4. User approves via `nodes approve <requestId>`
5. Gateway issues token, stores paired node
6. App connects via WebSocket with token
```

### GPU Node Pairing (New)

**Option A: Pull-based (Self-added)**
```
1. User runs: clawnode join --gateway wss://gateway:18789
2. clawnode sends pairing request (with GPU caps)
3. Gateway auto-approves if on trusted subnet
   OR shows pending request for manual approval
4. Gateway issues token
5. clawnode stores token, maintains WebSocket connection
```

**Option B: Push-based (Auto-discovered)**
```
1. Agent runs network_scan, finds host with SSH + GPU
2. Agent runs node_bootstrap with credential profile
3. Gateway SSHs to target, installs clawnode
4. clawnode auto-starts, sends pairing request
5. Gateway auto-approves (trusted subnet + known install)
```

## Unified Node Commands

All nodes (companion or GPU) support a common protocol:

```typescript
// Invoke command on any node
nodes invoke --node <name> --command <cmd> --params <json>

// Examples:
nodes invoke --node iphone --command camera.snap --params '{"facing":"back"}'
nodes invoke --node gpu-server-1 --command gpu.metrics --params '{}'
nodes invoke --node gpu-server-1 --command workload.run --params '{"image":"pytorch:2.0","gpus":2}'
```

## clawnode Agent Design

```
┌─────────────────────────────────────────────────────────┐
│                      clawnode                           │
│               (runs on GPU servers)                     │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ┌─────────────────┐  ┌─────────────────┐              │
│  │  WebSocket      │  │  GPU Discovery  │              │
│  │  Client         │  │  (nvidia-smi)   │              │
│  │                 │  │                 │              │
│  │  - reconnect    │  │  - count        │              │
│  │  - heartbeat    │  │  - models       │              │
│  │  - command rx   │  │  - memory       │              │
│  └─────────────────┘  └─────────────────┘              │
│           │                    │                        │
│           ▼                    ▼                        │
│  ┌─────────────────────────────────────────────────┐   │
│  │              Command Handlers                    │   │
│  │                                                  │   │
│  │  gpu.list       → nvidia-smi -L                 │   │
│  │  gpu.metrics    → nvidia-smi --query-gpu=...    │   │
│  │  workload.run   → docker/podman run            │   │
│  │  workload.stop  → docker stop                   │   │
│  │  workload.logs  → docker logs -f                │   │
│  │  container.exec → docker exec                   │   │
│  │  system.run     → exec command                  │   │
│  │  system.info    → /proc/meminfo, df, etc       │   │
│  └─────────────────────────────────────────────────┘   │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

## Bootstrap Script

When `node_bootstrap` is called, it:

1. SSHs to target using credential profile
2. Downloads clawnode binary from gateway
3. Creates systemd/launchd service
4. Starts clawnode with bootstrap token
5. clawnode connects back, reports GPU capabilities
6. Gateway adds to cluster registry

```bash
#!/bin/bash
# /bootstrap.sh - served by gateway

set -e

GATEWAY="${GATEWAY:-wss://gateway:18789}"
TOKEN="${TOKEN:-}"
HOSTNAME="${HOSTNAME:-$(hostname)}"

# Download clawnode
curl -sSL "${GATEWAY}/clawnode-$(uname -s)-$(uname -m)" -o /usr/local/bin/clawnode
chmod +x /usr/local/bin/clawnode

# Create config
mkdir -p /etc/clawnode
cat > /etc/clawnode/config.json <<EOF
{
  "gateway": "$GATEWAY",
  "token": "$TOKEN",
  "hostname": "$HOSTNAME"
}
EOF

# Create systemd service
cat > /etc/systemd/system/clawnode.service <<EOF
[Unit]
Description=Clawbernetes Node Agent
After=network.target

[Service]
ExecStart=/usr/local/bin/clawnode
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable clawnode
systemctl start clawnode

echo "clawnode installed and started"
```

## Integration Points

### 1. Gateway Config
Add clawbernetes plugin to gateway config:

```yaml
plugins:
  entries:
    clawbernetes:
      path: /path/to/plugin
      config:
        bridge_binary: /path/to/claw-bridge
        auto_approve_subnets:
          - 10.0.0.0/8
          - 192.168.0.0/16
```

### 2. Unified Node List
The `nodes status` command shows both companion and GPU nodes:

```
┌─────────────────┬──────────┬─────────────┬────────────────────────────┐
│ Node            │ Type     │ Status      │ Capabilities               │
├─────────────────┼──────────┼─────────────┼────────────────────────────┤
│ iphone          │ companion│ connected   │ camera, screen, location   │
│ macbook         │ companion│ connected   │ camera, screen, system.run │
│ gpu-server-1    │ gpu      │ connected   │ 4x H100, 320GB VRAM        │
│ gpu-server-2    │ gpu      │ disconnected│ 8x A100, 640GB VRAM        │
└─────────────────┴──────────┴─────────────┴────────────────────────────┘
```

### 3. Tool Routing
The agent automatically routes commands to the right node type:

```
User: "Take a picture of my whiteboard"
→ nodes invoke --node iphone --command camera.snap

User: "Train this model on 4 GPUs"
→ workload_submit --gpus 4 --image pytorch:2.0
→ (scheduler picks gpu-server-1)
→ nodes invoke --node gpu-server-1 --command workload.run
```

## Security Model

1. **Token-based auth**: Each node has a unique token issued at pairing
2. **Trusted subnets**: Auto-approve nodes on known networks
3. **Capability restrictions**: Nodes only receive commands they advertise
4. **mTLS option**: For production, require mutual TLS
5. **Audit logging**: All node commands logged with session context

## Next Steps

1. [x] Add network discovery tools (network_scan, credential profiles)
2. [ ] Build node_bootstrap handler with SSH execution
3. [ ] Create clawnode agent binary
4. [ ] Wire GPU node registration to OpenClaw pairing
5. [ ] Add GPU-specific commands to nodes tool
