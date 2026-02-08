# Secrets & Configuration Manager

You can create, retrieve, rotate, and delete encrypted secrets and plain configuration data on Clawbernetes nodes.

## Secret Commands

Secrets are encrypted at rest using ChaCha20-Poly1305 with per-secret key derivation. All access is audited.

### Create a Secret

```
node.invoke <node-id> secret.create {
  "name": "db.password",
  "data": {"username": "admin", "password": "s3cret"},
  "allowed_workloads": ["api-server", "worker"]
}
```

**Parameters:**
- `name` (required): Secret name (1-253 chars, lowercase alphanumeric + hyphen/underscore/period)
- `data` (required): Key-value pairs to encrypt
- `allowed_workloads` (optional): Restrict access to specific workloads

### Retrieve a Secret

```
node.invoke <node-id> secret.get { "name": "db.password" }
```

Returns decrypted data, version number, and timestamps. Access is audited.

### Delete a Secret

```
node.invoke <node-id> secret.delete { "name": "db.password" }
```

### List Secrets

```
node.invoke <node-id> secret.list { "prefix": "db." }
```

Returns secret names, versions, and timestamps (never values). Prefix filter is optional.

### Rotate a Secret

```
node.invoke <node-id> secret.rotate {
  "name": "db.password",
  "newData": {"username": "admin", "password": "n3w-s3cret"}
}
```

Increments version, logs rotation event. Old value is overwritten.

## Configuration Commands

Configs are plain key-value data (not encrypted). Use for non-sensitive settings.

### Create a Config

```
node.invoke <node-id> config.create {
  "name": "app.settings",
  "data": {"log_level": "info", "port": "8080", "workers": "4"},
  "immutable": false
}
```

**Parameters:**
- `name` (required): Config name
- `data` (required): Key-value pairs
- `immutable` (optional, default false): If true, config cannot be modified after creation

### Get a Config

```
node.invoke <node-id> config.get { "name": "app.settings" }
```

### Update a Config

```
node.invoke <node-id> config.update {
  "name": "app.settings",
  "data": {"log_level": "debug", "port": "8080", "workers": "8"}
}
```

Fails if the config is immutable.

### Delete a Config

```
node.invoke <node-id> config.delete { "name": "app.settings" }
```

### List Configs

```
node.invoke <node-id> config.list { "prefix": "app." }
```

## Common Patterns

### Database Credentials
```json
{
  "name": "db.credentials",
  "data": {"host": "db.internal", "port": "5432", "username": "app", "password": "secret"},
  "allowed_workloads": ["api-server"]
}
```

### API Keys
```json
{
  "name": "external.openai",
  "data": {"api_key": "sk-..."},
  "allowed_workloads": ["inference-server"]
}
```

### Application Config
```json
{
  "name": "app.production",
  "data": {"replicas": "3", "log_level": "warn", "feature_flags": "new-ui,fast-inference"},
  "immutable": true
}
```

## Secrets vs Configs

| | Secrets | Configs |
|---|---------|---------|
| Encrypted at rest | Yes (ChaCha20-Poly1305) | No |
| Access audited | Yes | No |
| Access policies | Yes (workload-scoped) | No |
| Versioned | Yes | No |
| Rotation support | Yes | No |
| Use for | Passwords, API keys, tokens | App settings, feature flags |
