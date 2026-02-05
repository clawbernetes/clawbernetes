# Security Guide

Comprehensive guide to securing your Clawbernetes deployment.

## Table of Contents

1. [Security Overview](#security-overview)
2. [Authentication](#authentication)
3. [Authorization (RBAC)](#authorization-rbac)
4. [Secrets Management](#secrets-management)
5. [Network Security](#network-security)
6. [DDoS Protection](#ddos-protection)
7. [Audit Logging](#audit-logging)
8. [Security Checklist](#security-checklist)

---

## Security Overview

Clawbernetes implements defense-in-depth with multiple security layers:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           SECURITY LAYERS                                   │
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │ Layer 1: Network Security                                             │  │
│  │  • DDoS protection (rate limiting, IP blocking)                       │  │
│  │  • TLS encryption (all traffic)                                       │  │
│  │  • WireGuard/Tailscale mesh (node-to-node)                           │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │ Layer 2: Authentication                                               │  │
│  │  • API keys (long-lived automation)                                   │  │
│  │  • JWT tokens (user sessions)                                         │  │
│  │  • mTLS certificates (node identity)                                  │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │ Layer 3: Authorization                                                │  │
│  │  • Role-Based Access Control (RBAC)                                   │  │
│  │  • Scoped permissions                                                 │  │
│  │  • Resource-level policies                                            │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │ Layer 4: Data Protection                                              │  │
│  │  • Encrypted secrets (AES-256-GCM)                                    │  │
│  │  • Automatic key rotation                                             │  │
│  │  • Workload identity-based access                                     │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │ Layer 5: Audit & Monitoring                                           │  │
│  │  • Comprehensive audit logging                                        │  │
│  │  • Tamper-evident logs                                                │  │
│  │  • Security alerts                                                    │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Security Principles

1. **Zero Trust** — Verify every request, assume breach
2. **Least Privilege** — Grant minimum required permissions
3. **Defense in Depth** — Multiple independent security layers
4. **Secure by Default** — Safe defaults, explicit opt-in for risky features
5. **Audit Everything** — Complete trail of all security-relevant events

---

## Authentication

### API Keys

API keys are for automation and long-lived integrations.

#### Generating API Keys

```bash
# Generate a new API key
clawbernetes auth apikey create \
  --name "CI/CD Pipeline" \
  --scopes "workloads:*,nodes:read" \
  --expires 90d

# Output:
# API Key created successfully
# ════════════════════════════════════════════════════════════════
# Key ID:     ak_7x8Kj2mNp3Qr5tWv9yBc
# Secret:     clk_a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6q7r8s9t0  ← SAVE THIS!
# Name:       CI/CD Pipeline
# Scopes:     workloads:*, nodes:read
# Expires:    2026-05-06T08:00:00Z
# ════════════════════════════════════════════════════════════════
# WARNING: The secret will not be shown again!
```

#### Using API Keys

```bash
# CLI
export CLAWBERNETES_API_KEY="clk_a1b2c3d4..."
clawbernetes node list

# HTTP Header
curl -H "X-API-Key: clk_a1b2c3d4..." \
  http://gateway:8080/api/nodes

# Or as Bearer token
curl -H "Authorization: Bearer clk_a1b2c3d4..." \
  http://gateway:8080/api/nodes
```

#### Managing API Keys

```bash
# List all API keys
clawbernetes auth apikey list

# Revoke a key
clawbernetes auth apikey revoke ak_7x8Kj2mNp3Qr5tWv9yBc

# Rotate a key (creates new secret)
clawbernetes auth apikey rotate ak_7x8Kj2mNp3Qr5tWv9yBc
```

### JWT Tokens

JWT tokens are for user sessions and short-lived access.

#### Configuration

```toml
# gateway.toml
[auth.jwt]
# Secret for signing (min 32 bytes, use env var in production)
secret_env = "CLAWBERNETES_JWT_SECRET"

# Token settings
issuer = "clawbernetes"
default_expiry_hours = 24
max_expiry_hours = 168  # 7 days

# Refresh settings
allow_refresh = true
refresh_window_hours = 1
```

#### Token Operations

```bash
# Login and get token
clawbernetes auth login --username admin
# Password: ****
# Token: eyJhbGciOiJIUzI1NiIs...

# Use token
export CLAWBERNETES_TOKEN="eyJhbGciOiJIUzI1NiIs..."
clawbernetes status

# Refresh token
clawbernetes auth refresh

# Logout (invalidate token)
clawbernetes auth logout
```

### mTLS (Node Identity)

Nodes authenticate to the gateway using mTLS certificates.

#### Setup PKI

```bash
# Initialize certificate authority
clawbernetes pki init

# Output:
# PKI initialized at /etc/clawbernetes/pki/
# ├── ca.crt          (Root CA certificate)
# ├── ca.key          (Root CA private key - PROTECT THIS!)
# └── crl/            (Certificate revocation lists)

# Generate gateway certificate
clawbernetes pki issue gateway \
  --dns gateway.example.com \
  --ip 10.0.0.1

# Generate node certificate
clawbernetes pki issue node \
  --name gpu-node-1 \
  --dns gpu-node-1.example.com
```

#### Configure mTLS

**Gateway:**
```toml
# gateway.toml
[tls]
cert = "/etc/clawbernetes/pki/gateway.crt"
key = "/etc/clawbernetes/pki/gateway.key"
ca = "/etc/clawbernetes/pki/ca.crt"
require_client_cert = true
```

**Node:**
```toml
# clawnode.toml
[tls]
cert = "/etc/clawbernetes/pki/node.crt"
key = "/etc/clawbernetes/pki/node.key"
ca = "/etc/clawbernetes/pki/ca.crt"
```

---

## Authorization (RBAC)

Role-Based Access Control determines what authenticated users can do.

### Default Roles

| Role | Description | Permissions |
|------|-------------|-------------|
| `admin` | Full cluster access | `*:*` (everything) |
| `operator` | Manage workloads and view nodes | `workloads:*`, `nodes:read,list` |
| `viewer` | Read-only access | `*:read,list` |
| `node` | Node agent access | `metrics:write`, `logs:write`, `workloads:execute` |

### Permission Format

Permissions follow the format: `resource:action`

**Resources:**
- `nodes` — Node management
- `workloads` — Workload management
- `secrets` — Secrets management
- `users` — User management
- `roles` — Role management
- `audit` — Audit log access
- `molt` — MOLT marketplace
- `*` — All resources

**Actions:**
- `create` — Create new resources
- `read` — Read single resource
- `list` — List resources
- `update` — Modify resources
- `delete` — Delete resources
- `execute` — Execute operations
- `admin` — Full administrative access
- `*` — All actions

### Creating Custom Roles

```bash
# Create a role
clawbernetes auth role create ml-engineer \
  --description "ML team member" \
  --permissions "workloads:*,nodes:read,nodes:list,secrets:read"

# List roles
clawbernetes auth role list

# View role details
clawbernetes auth role info ml-engineer
```

### Managing Users

```bash
# Create a user
clawbernetes auth user create alice \
  --email alice@example.com \
  --roles operator,ml-engineer

# Assign role
clawbernetes auth user assign-role alice viewer

# Remove role
clawbernetes auth user remove-role alice viewer

# List users
clawbernetes auth user list

# Disable user
clawbernetes auth user disable alice
```

### RBAC Configuration

```toml
# gateway.toml
[auth.rbac]
# Enforce RBAC (disable only for testing!)
enabled = true

# Default role for new users
default_role = "viewer"

# Require explicit role assignment
require_role_assignment = true
```

---

## Secrets Management

Secrets are encrypted at rest and access-controlled by workload identity.

### Creating Secrets

```bash
# Create a secret
clawbernetes secrets create database-password \
  --value "super-secret-password" \
  --allowed-workloads "api-server,worker"

# From file
clawbernetes secrets create tls-cert \
  --file /path/to/cert.pem \
  --allowed-workloads "web-server"

# With rotation policy
clawbernetes secrets create api-key \
  --value "key123" \
  --rotate-days 30 \
  --allowed-workloads "*"
```

### Access Policies

```bash
# Allow specific workloads
clawbernetes secrets policy set database-password \
  --allow-workloads "api-server,worker,migration-job"

# Allow by label
clawbernetes secrets policy set database-password \
  --allow-labels "env=production,team=backend"

# Deny specific workloads
clawbernetes secrets policy set database-password \
  --deny-workloads "debug-container"
```

### Using Secrets in Workloads

```bash
# Inject as environment variable
clawbernetes run \
  --secret DATABASE_PASSWORD=database-password \
  my-app:latest

# Mount as file
clawbernetes run \
  --secret-mount /secrets/cert.pem=tls-cert \
  my-app:latest
```

### Secret Rotation

```bash
# Manual rotation
clawbernetes secrets rotate database-password \
  --new-value "new-super-secret"

# View rotation history
clawbernetes secrets history database-password

# Configure auto-rotation
clawbernetes secrets config database-password \
  --auto-rotate \
  --rotate-days 30
```

### Encryption Details

- **Algorithm:** ChaCha20-Poly1305 (AEAD)
- **Key derivation:** BLAKE3
- **Key storage:** Protected by master key (env or HSM)
- **At rest:** All secrets encrypted before storage
- **In transit:** TLS 1.3

---

## Network Security

### WireGuard Mesh

Self-hosted encrypted mesh network:

```toml
# clawnode.toml
[network]
provider = "wireguard"

[network.wireguard]
listen_port = 51820
endpoint = "node1.example.com:51820"
# Private key auto-generated on first run
# Or specify existing:
# private_key_path = "/etc/clawbernetes/wg-private.key"

# Allowed peer subnets
allowed_ips = ["10.100.0.0/16"]

# Persistent keepalive (for NAT traversal)
persistent_keepalive = 25
```

See [wireguard-integration.md](wireguard-integration.md) for full setup.

### Tailscale Mesh

Managed mesh network (zero-config):

```toml
# clawnode.toml
[network]
provider = "tailscale"

[network.tailscale]
auth_key_env = "TS_AUTHKEY"
hostname_prefix = "clawnode"
tags = ["tag:clawbernetes"]

# ACL recommendations (set in Tailscale admin console):
# - Allow clawbernetes nodes to communicate
# - Restrict gateway to specific ports
```

See [tailscale-integration.md](tailscale-integration.md) for full setup.

### TLS Configuration

```toml
# gateway.toml
[tls]
# Server certificate
cert = "/etc/clawbernetes/tls/server.crt"
key = "/etc/clawbernetes/tls/server.key"

# CA for client verification
ca = "/etc/clawbernetes/tls/ca.crt"

# Require client certificates
require_client_cert = true

# Minimum TLS version
min_version = "1.3"

# Allowed cipher suites (TLS 1.3)
cipher_suites = [
  "TLS_AES_256_GCM_SHA384",
  "TLS_CHACHA20_POLY1305_SHA256",
]
```

### Firewall Rules

Recommended iptables rules for gateway:

```bash
# Allow WebSocket (nodes and CLI)
iptables -A INPUT -p tcp --dport 8080 -j ACCEPT

# Allow HTTPS (dashboard)
iptables -A INPUT -p tcp --dport 443 -j ACCEPT

# Allow WireGuard (if using)
iptables -A INPUT -p udp --dport 51820 -j ACCEPT

# Drop everything else
iptables -A INPUT -j DROP
```

---

## DDoS Protection

Clawbernetes includes built-in DDoS protection.

### Configuration

```toml
# gateway.toml
[ddos]
enabled = true

# Connection limits
[ddos.connection]
max_per_ip = 100
max_total = 10000
slow_loris_timeout_secs = 30

# Rate limiting
[ddos.rate_limit]
requests_per_second = 100
burst_size = 200

# Compute cost limiting (expensive operations)
[ddos.compute]
max_cost_per_minute = 1000

# Bandwidth limiting
[ddos.bandwidth]
max_bytes_per_second = 10485760  # 10 MB/s

# Blocklist
[ddos.blocklist]
enabled = true
auto_block_threshold = 5  # violations before auto-block
block_duration_secs = 3600

# Reputation tracking
[ddos.reputation]
enabled = true
decay_rate = 0.1  # per minute
threshold = -10   # block below this score
```

### Manual IP Management

```bash
# Block an IP
clawbernetes security block 203.0.113.42 \
  --reason "Suspicious activity" \
  --duration 24h

# Unblock an IP
clawbernetes security unblock 203.0.113.42

# List blocked IPs
clawbernetes security blocklist

# View reputation scores
clawbernetes security reputation
```

### Geographic Blocking

```toml
# gateway.toml
[ddos.geo]
enabled = true
mode = "allowlist"  # or "blocklist"
countries = ["US", "CA", "GB", "DE"]  # Allowed countries
```

---

## Audit Logging

All security-relevant events are logged for compliance and forensics.

### Audit Events

| Event Type | Description |
|------------|-------------|
| `auth.login` | User login attempt |
| `auth.logout` | User logout |
| `auth.apikey.created` | API key created |
| `auth.apikey.revoked` | API key revoked |
| `rbac.role.assigned` | Role assigned to user |
| `secrets.accessed` | Secret read by workload |
| `secrets.created` | Secret created |
| `secrets.rotated` | Secret rotated |
| `node.connected` | Node connected to gateway |
| `node.disconnected` | Node disconnected |
| `workload.started` | Workload started |
| `workload.stopped` | Workload stopped |
| `security.blocked` | IP blocked |
| `security.violation` | Security policy violation |

### Viewing Audit Logs

```bash
# Recent events
clawbernetes audit logs --last 100

# Filter by event type
clawbernetes audit logs --type auth.login --last 50

# Filter by user
clawbernetes audit logs --user alice --since 24h

# Filter by time range
clawbernetes audit logs \
  --since 2026-02-01T00:00:00Z \
  --until 2026-02-05T00:00:00Z

# Export for analysis
clawbernetes audit export \
  --format json \
  --since 30d \
  > audit-export.json
```

### Audit Configuration

```toml
# gateway.toml
[audit]
enabled = true

# Storage
storage_path = "/var/log/clawbernetes/audit"
retention_days = 365

# Event filtering (reduce noise)
exclude_events = ["metrics.reported"]  # High-frequency events

# Integrity
sign_logs = true  # Tamper-evident signatures
hash_chain = true # Chain hashes for integrity

# External shipping
[audit.export]
enabled = true
endpoint = "https://siem.example.com/ingest"
format = "json"
batch_size = 100
```

---

## Security Checklist

### Initial Setup

- [ ] Generate strong JWT secret (`openssl rand -hex 32`)
- [ ] Initialize PKI for mTLS
- [ ] Configure TLS with valid certificates
- [ ] Set up WireGuard or Tailscale mesh
- [ ] Enable audit logging
- [ ] Configure DDoS protection

### User Management

- [ ] Create named user accounts (no shared accounts)
- [ ] Assign minimum required roles
- [ ] Set up API keys with scoped permissions
- [ ] Enable MFA for admin accounts (if available)
- [ ] Review and remove inactive users regularly

### Secrets

- [ ] Never hardcode secrets in configs or code
- [ ] Use environment variables for sensitive config
- [ ] Set rotation policies for all secrets
- [ ] Restrict secret access to specific workloads
- [ ] Audit secret access regularly

### Network

- [ ] Enable TLS everywhere
- [ ] Use mTLS for node authentication
- [ ] Configure firewall rules
- [ ] Enable DDoS protection
- [ ] Consider geographic restrictions

### Monitoring

- [ ] Set up alerts for failed authentications
- [ ] Monitor for unusual API activity
- [ ] Review audit logs weekly
- [ ] Set up SIEM integration for production

### MOLT-Specific

- [ ] Use dedicated wallet for MOLT (not main wallet)
- [ ] Start with conservative autonomy mode
- [ ] Set spending limits for buyers
- [ ] Require attestation for sensitive workloads
- [ ] Review provider reputation before large jobs

---

## Reporting Security Issues

If you discover a security vulnerability:

1. **Do NOT** open a public GitHub issue
2. Email security@clawbernetes.com with details
3. Include steps to reproduce if possible
4. We'll respond within 48 hours

See [SECURITY.md](../SECURITY.md) for our security policy.

---

## See Also

- [User Guide](user-guide.md) — Getting started
- [Architecture](architecture.md) — Security architecture details
- [CLI Reference](cli-reference.md) — Auth command reference
- [MOLT Network](molt-network.md) — MOLT security considerations
