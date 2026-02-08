# Auth, RBAC & Audit

You can manage API keys, role-based access control, and audit logging on Clawbernetes nodes.

Requires the `auth` feature on the node.

## API Key Commands

### Create an API Key

```
node.invoke <node-id> auth.create_key {
  "name": "ci-pipeline",
  "scopes": ["workloads:*", "deploy:create"],
  "expiresIn": "90d"
}
```

**Parameters:**
- `name` (required): Human-readable key name
- `scopes` (optional): Permission scopes (e.g., `workloads:*`, `gpu:read`)
- `expiresIn` (optional): Expiration duration

**Returns:** `keyId`, `name`, `secret` (shown only once), `scopes`, `userId`

**Important:** The `secret` is only returned at creation time. Store it securely.

### Revoke an API Key

```
node.invoke <node-id> auth.revoke_key {
  "keyId": "<key-id>",
  "reason": "compromised"
}
```

### List API Keys

```
node.invoke <node-id> auth.list_keys
```

Returns count of all keys in the store.

## RBAC Commands

### Create a Role

```
node.invoke <node-id> rbac.create_role {
  "name": "gpu-operator",
  "description": "Can manage GPU workloads",
  "permissions": [
    {"resource": "workloads", "action": "create"},
    {"resource": "workloads", "action": "read"},
    {"resource": "workloads", "action": "delete"},
    {"resource": "gpu", "action": "read"}
  ]
}
```

**Actions:** `create`, `read`, `update`, `delete`, `list`, `execute`, `admin`

### Bind a Role to a User

```
node.invoke <node-id> rbac.bind {
  "userId": "<user-id>",
  "role": "gpu-operator"
}
```

Creates the user if they don't exist, then assigns the role.

### Check Permissions

```
node.invoke <node-id> rbac.check {
  "userId": "<user-id>",
  "action": "create",
  "resource": "workloads"
}
```

Returns: `allowed` (true/false). Also records an audit entry.

## Audit Commands

### Query Audit Log

```
node.invoke <node-id> audit.query {
  "principal": "<user-id>",
  "action": "auth.create_key",
  "rangeMinutes": 60
}
```

**Parameters (all optional):**
- `principal`: Filter by user/principal
- `action`: Filter by action type
- `rangeMinutes`: Only show events within the last N minutes

Returns timestamped audit events with principal, action, resource, and result (allowed/denied).

## Default Roles

The node ships with these built-in roles:
- **admin** — Full access to all resources
- **operator** — Can manage workloads and deployments
- **viewer** — Read-only access

## Common Patterns

### Set Up a CI Pipeline Key
```json
// 1. Create a scoped API key
{"name": "github-actions", "scopes": ["deploy:*", "workloads:read"]}
// 2. Store the returned secret in CI secrets
```

### Principle of Least Privilege
```json
// 1. Create a restricted role
{"name": "ml-engineer", "description": "GPU training only", "permissions": [
  {"resource": "workloads", "action": "create"},
  {"resource": "workloads", "action": "read"},
  {"resource": "gpu", "action": "read"},
  {"resource": "jobs", "action": "create"}
]}
// 2. Bind to users
{"userId": "alice", "role": "ml-engineer"}
```

### Investigate Access
```json
// Query recent denied actions
{"action": "denied", "rangeMinutes": 1440}
```
