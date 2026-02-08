---
name: networking
description: Service mesh, ingress, and WireGuard networking for Clawbernetes clusters
metadata:
  openclaw:
    requires:
      bins: ["clawnode"]
---

# Networking

Manage services, ingress routes, network policies, mesh connectivity, and workload networking on Clawbernetes nodes.

Requires the `network` feature on the node.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  External Traffic                                           │
│        │                                                    │
│        ▼                                                    │
│  ┌─────────────────┐                                        │
│  │  Ingress Proxy   │ ← Host/path routing (port 8443)      │
│  │  (hyper)         │                                       │
│  └────────┬────────┘                                        │
│           ▼                                                 │
│  ┌─────────────────┐                                        │
│  │ Service Discovery│ ← ClusterIP VIPs (10.201.0.0/16)     │
│  │ (iptables DNAT)  │                                       │
│  └────────┬────────┘                                        │
│           ▼                                                 │
│  ┌─────────────────┐   ┌──────────────────┐                 │
│  │ Network Policy   │   │ Workload Net     │                │
│  │ (iptables filter)│   │ (10.200.x.0/24)  │                │
│  └─────────────────┘   └────────┬─────────┘                │
│                                  ▼                          │
│                         ┌──────────────────┐                │
│                         │ WireGuard Mesh    │                │
│                         │ (10.100.x.x/16)  │                │
│                         └──────────────────┘                │
└─────────────────────────────────────────────────────────────┘
```

## Service Commands

Services provide stable ClusterIP endpoints for workloads with iptables-based DNAT load balancing.

### Create a Service

```
node.invoke <node-id> service.create {
  "name": "api-svc",
  "selector": {"app": "api"},
  "port": 8080,
  "protocol": "tcp"
}
```

**Parameters:**
- `name` (required): Service name
- `selector` (optional): Label selector to match workloads
- `port` (required): Port number
- `protocol` (optional, default "tcp"): "tcp" or "udp"

**Returns:** `{ name, port, protocol, clusterIp, success }`

The `clusterIp` is a virtual IP from 10.201.0.0/16, routed via iptables DNAT to backend containers.

### Get a Service

```
node.invoke <node-id> service.get { "name": "api-svc" }
```

Returns: name, selector, port, protocol, endpoints, clusterIp, liveEndpoints, creation time.

### Delete a Service

```
node.invoke <node-id> service.delete { "name": "api-svc" }
```

Removes the service, releases its ClusterIP, and cleans up iptables DNAT rules.

### List Services

```
node.invoke <node-id> service.list
```

### Get Service Endpoints

```
node.invoke <node-id> service.endpoints { "name": "api-svc" }
```

Returns live endpoint information: clusterIp, total count, healthy count, and per-endpoint details (ip, port, containerId, healthy).

## Ingress Commands

Ingress rules route external HTTP traffic to backend services via the built-in reverse proxy (port 8443) based on `Host` header and path prefix matching.

### Create an Ingress

```
node.invoke <node-id> ingress.create {
  "name": "api-ingress",
  "rules": [
    {"host": "api.example.com", "path": "/", "service": "api-svc"},
    {"host": "api.example.com", "path": "/v2", "service": "api-v2-svc"}
  ],
  "tls": true
}
```

**Parameters:**
- `name` (required): Ingress name
- `rules` (required): Array of routing rules with host, path, and target service
- `tls` (optional, default false): Enable TLS termination

Longest path prefix wins when multiple rules match.

### Delete an Ingress

```
node.invoke <node-id> ingress.delete { "name": "api-ingress" }
```

## Network Status

```
node.invoke <node-id> network.status
```

Returns:
- **mesh**: Node count, per-node details (ID, mesh IP, region)
- **wireguard**: Interface, mesh IP, public key, listen port, per-peer traffic stats
- **serviceDiscovery**: Service count, iptables availability, per-service ClusterIP and endpoint counts
- **allocator**: IP allocator stats

## Network Policies

Network policies control traffic flow between workloads using iptables rules in the `CLAW-NETPOL` filter chain.

### Create a Network Policy

```
node.invoke <node-id> network.policy.create {
  "name": "deny-all",
  "selector": {"app": "secure"},
  "ingress": [],
  "egress": []
}
```

**Parameters:**
- `name` (required): Policy name
- `selector` (optional): Label selector for affected workloads
- `ingress` (optional): Allowed inbound traffic rules
- `egress` (optional): Allowed outbound traffic rules

Empty `ingress: []` means deny all inbound. Empty `egress: []` means deny all outbound.

### Ingress Rule Format

```json
{
  "port": 8080,
  "protocol": "tcp",
  "from": {
    "cidr": "10.0.0.0/8"
  }
}
```

Or using label selectors:
```json
{
  "port": 5432,
  "protocol": "tcp",
  "from": {
    "selector": {"app": "api"}
  }
}
```

### Delete a Network Policy

```
node.invoke <node-id> network.policy.delete { "name": "deny-all" }
```

### List Network Policies

```
node.invoke <node-id> network.policy.list
```

## Workload Networking

Containers launched with `workload.run` automatically receive mesh-routable IPs from the node's /24 workload subnet (10.200.{node}.0/24) when workload networking is initialized. IPs are released when the workload is stopped.

The response from `workload.run` includes `meshIp` and `network` fields when networking is active.

## Common Patterns

### Expose a Web Service with Load Balancing

```json
// 1. Create service with selector
{"name": "web", "selector": {"app": "web"}, "port": 80}
// → Returns clusterIp: "10.201.0.1"

// 2. Create ingress for external access
{"name": "web-ingress", "rules": [{"host": "www.example.com", "path": "/", "service": "web"}]}

// 3. Run workloads matching the selector
// workload.run with labels {"app": "web"} → gets mesh IP + auto-registered as endpoint
```

### Isolate a Database

```json
{
  "name": "db-isolation",
  "selector": {"app": "postgres"},
  "ingress": [
    {"port": 5432, "protocol": "tcp", "from": {"selector": {"app": "api"}}}
  ],
  "egress": []
}
```

Only workloads with label `app=api` can connect to port 5432 on workloads with label `app=postgres`. All outbound traffic from the database is denied.

### Multi-Service API Gateway

```json
// Service per backend
{"name": "auth-svc", "selector": {"app": "auth"}, "port": 8080}
{"name": "users-svc", "selector": {"app": "users"}, "port": 8080}
{"name": "orders-svc", "selector": {"app": "orders"}, "port": 8080}

// Single ingress with path-based routing
{
  "name": "api-gateway",
  "rules": [
    {"host": "api.example.com", "path": "/auth", "service": "auth-svc"},
    {"host": "api.example.com", "path": "/users", "service": "users-svc"},
    {"host": "api.example.com", "path": "/orders", "service": "orders-svc"}
  ],
  "tls": true
}
```
